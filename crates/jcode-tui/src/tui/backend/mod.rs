//! Orchestrator for the `backend/` module.
//!
//! This file used to be a 1996-line `backend.rs`. Per AGENTS.md's 500-line
//! hard cap, it is decomposed into 6 sibling files in this directory. The
//! `mod.rs` is the only file that owns the public surface of the module -
//! every type, function, and impl method that callers depend on is re-exported
//! from here. The other 5 sibling files are organized by concern:
//!
//! - [`debug`] - debug-socket events (`DebugEvent`, `DebugMessage`).
//! - [`protocol`] - read pipeline (socket -> `RemoteRead`), write pipeline
//!   (`send_request` + interrupt trigger), and the `RemoteEventState` trait
//!   plumbing for tool-diff tracking.
//! - [`actions`] - content / identity / model-selection requests.
//! - [`controls`] - operator / UI plumbing requests (features, compaction,
//!   transcript, cancel, auth).
//! - [`tests`] - `#[cfg(test)] mod tests` (was 460+ lines, the bulk of the
//!   regression coverage).
//!
//! This orchestrator still owns the `RemoteConnection` struct definition and
//! its public lifecycle methods (`connect`, `connect_with_session`, `dummy`,
//! `take_dummy_peer`, `writer`, session id accessors, history-load marker),
//! since those are the natural cohesion boundary that all sibling files hook
//! into via `impl RemoteConnection` blocks. Rust allows impl blocks to be
//! split across files in the same module, so each sibling file holds the
//! `impl` for its own concern without duplicating struct fields.

mod actions;
mod controls;
mod debug;
mod protocol;

#[cfg(test)]
mod tests;

pub use debug::{DebugEvent, DebugMessage};

use crate::message::ToolCall;
use crate::protocol::{AuthChanged, FeatureToggle, Request, ServerEvent};
use crate::server;
use crate::transport::{Stream, WriteHalf};
use crate::tui::remote_diff::RemoteDiffTracker;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

/// Events emitted by backends during message processing
#[derive(Debug, Clone)]
pub enum BackendEvent {
    /// Text content delta from assistant
    TextDelta(String),

    /// Tool execution started
    ToolStart {
        id: String,
        name: String,
    },

    /// Tool input JSON delta
    ToolInput {
        delta: String,
    },

    /// Tool is about to execute (after input complete)
    ToolExec {
        id: String,
        name: String,
    },

    /// Tool execution completed
    ToolDone {
        id: String,
        name: String,
        output: String,
        is_error: bool,
    },

    /// Token usage update
    TokenUsage {
        input_tokens: u64,
        output_tokens: u64,
        cache_read_input_tokens: Option<u64>,
        cache_creation_input_tokens: Option<u64>,
    },

    /// Thinking started (extended thinking mode)
    ThinkingStart,

    /// Thinking ended
    ThinkingEnd,

    /// Thinking completed with duration
    ThinkingDone {
        duration_secs: f32,
    },

    /// Context compaction occurred
    Compaction {
        trigger: String,
        pre_tokens: u64,
    },

    /// Session ID assigned/updated
    SessionId(String),

    /// Message processing complete
    Done,

    /// Error occurred
    Error(String),

    /// Server is reloading (remote only)
    Reloading,

    /// Connection state changed
    Connected,
    Disconnected,
}

#[derive(Debug, Clone)]
pub enum RemoteDisconnectReason {
    PeerClosed,
    Io(String),
    Protocol(String),
}

#[derive(Debug, Clone)]
#[expect(
    clippy::large_enum_variant,
    reason = "remote reads carry full server events directly to keep transport handling simple"
)]
pub enum RemoteRead {
    Event(ServerEvent),
    Disconnected(RemoteDisconnectReason),
}

/// Classification of a single decoded protocol line read from the server
/// stream. Used by [`RemoteConnection::next_event`] to keep the cancellation-
/// safe read loop readable. The event is boxed because [`ServerEvent`] is large.
enum LineOutcome {
    Event(Box<ServerEvent>),
    Skip,
    Disconnect(RemoteDisconnectReason),
}

/// Information about the backend's provider
#[derive(Debug, Clone)]
pub struct BackendInfo {
    pub provider_name: String,
    pub provider_model: String,
    pub mcp_servers: Vec<String>,
    pub skills: Vec<String>,
}

/// Remote connection to jcode server
pub struct RemoteConnection {
    reader: BufReader<crate::transport::ReadHalf>,
    writer: Arc<Mutex<WriteHalf>>,
    _dummy_peer: Option<Stream>,
    session_id: Option<String>,
    client_instance_id: Option<String>,
    next_request_id: u64,
    // Bootstrap Done acknowledgments are not completions of a detached turn.
    // Retain recent ids because target Subscribe can acknowledge twice.
    control_done_ids: std::sync::Mutex<std::collections::VecDeque<u64>>,
    tool_diff: RemoteDiffTracker,
    /// Bytes pulled from the socket that have not yet been split into complete
    /// newline-delimited protocol lines. This buffer is persistent across
    /// `next_event` calls so a future cancelled by a `tokio::select!` peer
    /// branch never loses partially-read bytes.
    read_buffer: Vec<u8>,
    /// First byte in `read_buffer` that has not yet been checked for a newline.
    ///
    /// Large History events can be tens of megabytes and arrive over thousands
    /// of socket reads. Searching from byte zero after every read makes framing
    /// quadratic and eventually backpressures the server writer. Keeping this
    /// cursor makes each received byte participate in at most one newline scan.
    read_buffer_scan_start: usize,
    #[cfg(test)]
    protocol_bytes_scanned: usize,
    has_loaded_history: bool,
    call_output_tokens_seen: u64,
}

const DETACHED_REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_STRAY_REMOTE_PROTOCOL_LINES: usize = 32;
/// Hard cap for one newline-delimited server event. History events can be large
/// because they may contain images, but an authenticated or compromised peer
/// must not be able to grow the client process without bound.
const MAX_REMOTE_PROTOCOL_FRAME_BYTES: usize = 256 * 1024 * 1024;
/// Capacity above which the persistent `read_buffer` is considered oversized
/// once its backlog drains. A single multi-megabyte protocol line (e.g. a
/// `History` event with embedded images) would otherwise pin that capacity
/// for the lifetime of the connection.
const READ_BUFFER_SHRINK_THRESHOLD: usize = 256 * 1024;
/// Capacity retained after shrinking an oversized `read_buffer`. Comfortably
/// above typical streaming line sizes so steady-state traffic never causes
/// grow/shrink thrash.
const READ_BUFFER_RETAIN_CAPACITY: usize = 64 * 1024;

fn remote_protocol_frame_exceeds_limit(buffered: usize, incoming: usize) -> bool {
    buffered
        .checked_add(incoming)
        .is_none_or(|total| total > MAX_REMOTE_PROTOCOL_FRAME_BYTES)
}

pub(crate) trait RemoteEventState {
    fn handle_tool_start(&mut self, id: &str, name: &str);
    fn handle_tool_input(&mut self, id: Option<&str>, delta: &str);
    fn get_tool_input(&self, id: &str) -> serde_json::Value;
    fn handle_tool_exec(&mut self, id: &str, name: &str);
    fn handle_tool_done(&mut self, id: &str, name: &str, output: &str) -> String;
    fn clear_pending(&mut self);
    fn call_output_tokens_seen(&mut self) -> &mut u64;
    fn reset_call_output_tokens_seen(&mut self);
    fn set_session_id(&mut self, id: String);
    fn has_loaded_history(&self) -> bool;
    fn mark_history_loaded(&mut self);
}

#[derive(Default)]
pub(crate) struct ReplayRemoteState {
    tool_diff: RemoteDiffTracker,
    call_output_tokens_seen: u64,
}

impl RemoteConnection {
    /// Connect to the server
    pub async fn connect() -> Result<Self> {
        Self::connect_with_session(None, None, false, false, None).await
    }

    /// Connect to the server and optionally resume a specific session.
    ///
    /// When `client_has_local_history` is true, the client already restored the
    /// transcript locally and only needs lightweight session metadata from the server.
    pub async fn connect_with_session(
        resume_session: Option<&str>,
        client_instance_id: Option<&str>,
        client_has_local_history: bool,
        allow_session_takeover: bool,
        remote_working_dir: Option<&str>,
    ) -> Result<Self> {
        let connect_start = Instant::now();
        let socket_connect_start = Instant::now();
        let stream = Stream::connect(server::socket_path()).await?;
        let socket_connect_ms = socket_connect_start.elapsed().as_millis();
        let (reader, writer) = stream.into_split();

        let mut conn = Self {
            reader: BufReader::new(reader),
            writer: Arc::new(Mutex::new(writer)),
            _dummy_peer: None,
            session_id: None,
            client_instance_id: client_instance_id.map(str::to_string),
            // A reattached turn carries the old connection's request id on
            // local sockets as well as SSH. Keep its Done distinct from this
            // connection's Subscribe/GetHistory acknowledgments.
            next_request_id: (rand::random::<u64>() & ((1_u64 << 62) - 1)) | (1_u64 << 62),
            control_done_ids: Default::default(),
            tool_diff: RemoteDiffTracker::default(),
            read_buffer: Vec::new(),
            read_buffer_scan_start: 0,
            #[cfg(test)]
            protocol_bytes_scanned: 0,
            has_loaded_history: false,
            call_output_tokens_seen: 0,
        };

        // Subscribe to events
        let subscribe_start = Instant::now();
        let (working_dir, selfdev) = super::subscribe_metadata(remote_working_dir);
        let resume_target = resume_session
            .filter(|session_id| {
                super::is_ssh_remote() || crate::session::session_exists(session_id)
            })
            .map(|session_id| session_id.to_string());
        conn.send_request(Request::Subscribe {
            system_prompt: None,
            supports_pdf_panels: false,
            id: conn.next_request_id,
            working_dir,
            selfdev,
            target_session_id: resume_target.clone(),
            client_instance_id: conn.client_instance_id.clone(),
            client_has_local_history: client_has_local_history && !super::is_ssh_remote(),
            allow_session_takeover,
            crash_on_disconnect: false,
            continue_on_disconnect: super::is_ssh_remote(),
            terminal_env: if super::is_ssh_remote() {
                Vec::new()
            } else {
                crate::terminal_launch::snapshot_client_terminal_env()
            },
        })
        .await?;
        let subscribe_ms = subscribe_start.elapsed().as_millis();
        conn.next_request_id += 1;

        // If resuming a session, the target-aware Subscribe attaches directly to
        // that session and returns History, so avoid a second bootstrap request.
        let bootstrap_request_start = Instant::now();
        let mut bootstrap_request = "get_history";
        if resume_target.is_none() {
            conn.send_request(Request::GetHistory {
                id: conn.next_request_id,
            })
            .await?;
            conn.next_request_id += 1;
        } else {
            bootstrap_request = "subscribe_resume";
        }
        // Avoid a reconnect/reload thundering herd: every headed client used to
        // request the full expanded model catalog immediately after attach. On
        // large OpenRouter catalogs this is ~800KB per client and can make many
        // TUI processes parse/render at once, which showed up as multi-second
        // draw stalls during scrolling. The TUI hydrates the persisted remote
        // catalog cache for normal `/model` use; explicit refresh paths still
        // request fresh catalog data when needed.
        if std::env::var_os("KCODE_REMOTE_BOOTSTRAP_MODEL_CATALOG").is_some() {
            conn.send_request(Request::GetModelCatalog {
                subscribe_usage_updates: true,
                id: conn.next_request_id,
            })
            .await?;
            conn.next_request_id += 1;
        }

        let bootstrap_request_ms = bootstrap_request_start.elapsed().as_millis();

        crate::logging::info(&format!(
            "[TIMING] remote connect: socket={}ms, subscribe={}ms, bootstrap_request={}ms, total={}ms, resumed={}, request={}",
            socket_connect_ms,
            subscribe_ms,
            bootstrap_request_ms,
            connect_start.elapsed().as_millis(),
            resume_session.is_some(),
            bootstrap_request,
        ));

        Ok(conn)
    }

    pub fn writer(&self) -> Arc<Mutex<WriteHalf>> {
        Arc::clone(&self.writer)
    }

    /// Get session ID
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Create a dummy RemoteConnection for replay mode (no real server)
    #[cfg(test)]
    pub fn dummy() -> Self {
        let (a, b) = crate::transport::Stream::pair()
            .unwrap_or_else(|err| panic!("failed to create dummy socketpair for tests: {}", err));
        let (reader, writer) = a.into_split();
        Self {
            reader: BufReader::new(reader),
            writer: Arc::new(Mutex::new(writer)),
            _dummy_peer: Some(b),
            session_id: None,
            client_instance_id: None,
            next_request_id: 1,
            control_done_ids: Default::default(),
            tool_diff: RemoteDiffTracker::default(),
            read_buffer: Vec::new(),
            read_buffer_scan_start: 0,
            #[cfg(test)]
            protocol_bytes_scanned: 0,
            has_loaded_history: false,
            call_output_tokens_seen: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn take_dummy_peer(&mut self) -> Option<Stream> {
        self._dummy_peer.take()
    }

    #[cfg(test)]
    pub(crate) fn next_request_id_for_test(&self) -> u64 {
        self.next_request_id
    }

    /// Set session ID
    pub fn set_session_id(&mut self, id: String) {
        self.session_id = Some(id);
    }

    /// Check if history has been loaded
    pub fn has_loaded_history(&self) -> bool {
        self.has_loaded_history
    }

    /// Whether the socket reader already holds part of an inbound protocol
    /// frame. A history recovery request must not be sent in this state: the
    /// original response is in flight, and another request only queues another
    /// full copy behind it.
    pub fn has_buffered_inbound_frame(&self) -> bool {
        !self.read_buffer.is_empty()
    }

    /// Mark history as loaded
    pub fn mark_history_loaded(&mut self) {
        self.has_loaded_history = true;
    }


impl RemoteEventState for RemoteConnection {
    fn handle_tool_start(&mut self, id: &str, name: &str) {
        Self::handle_tool_start(self, id, name);
    }

    fn handle_tool_input(&mut self, id: Option<&str>, delta: &str) {
        Self::handle_tool_input(self, id, delta);
    }

    fn get_tool_input(&self, id: &str) -> serde_json::Value {
        Self::get_tool_input(self, id)
    }

    fn handle_tool_exec(&mut self, id: &str, name: &str) {
        Self::handle_tool_exec(self, id, name);
    }

    fn handle_tool_done(&mut self, id: &str, name: &str, output: &str) -> String {
        Self::handle_tool_done(self, id, name, output)
    }

    fn clear_pending(&mut self) {
        Self::clear_pending(self);
    }

    fn call_output_tokens_seen(&mut self) -> &mut u64 {
        Self::call_output_tokens_seen(self)
    }

    fn reset_call_output_tokens_seen(&mut self) {
        Self::reset_call_output_tokens_seen(self);
    }

    fn set_session_id(&mut self, id: String) {
        Self::set_session_id(self, id);
    }

    fn has_loaded_history(&self) -> bool {
        Self::has_loaded_history(self)
    }

    fn mark_history_loaded(&mut self) {
        Self::mark_history_loaded(self);
    }
}

impl RemoteEventState for ReplayRemoteState {
    fn handle_tool_start(&mut self, id: &str, name: &str) {
        self.tool_diff.handle_tool_start(id, name);
    }

    fn handle_tool_input(&mut self, id: Option<&str>, delta: &str) {
        self.tool_diff.handle_tool_input(id, delta);
    }

    fn get_tool_input(&self, id: &str) -> serde_json::Value {
        self.tool_diff.tool_input_json(id)
    }

    fn handle_tool_exec(&mut self, id: &str, name: &str) {
        self.tool_diff.handle_tool_exec(id, name);
    }

    fn handle_tool_done(&mut self, id: &str, name: &str, output: &str) -> String {
        self.tool_diff.finish_tool(id, name, output)
    }

    fn clear_pending(&mut self) {
        self.tool_diff.clear();
    }

    fn call_output_tokens_seen(&mut self) -> &mut u64 {
        &mut self.call_output_tokens_seen
    }

    fn reset_call_output_tokens_seen(&mut self) {
        self.call_output_tokens_seen = 0;
    }

    fn set_session_id(&mut self, _id: String) {}

    fn has_loaded_history(&self) -> bool {
        true
    }

    fn mark_history_loaded(&mut self) {}
}
