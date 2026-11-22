//! Backend abstraction for TUI runtime transports.
//!
//! This module provides a unified interface for message processing across
//! local harnesses and server-backed remote clients.
//!
//! Also provides debug socket events for exposing full TUI state.

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

/// Debug events broadcast by local harnesses via debug socket.
/// These expose the full internal state for debugging/comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DebugEvent {
    /// Full state snapshot (sent on connect)
    StateSnapshot {
        display_messages: Vec<DebugMessage>,
        streaming_text: String,
        streaming_tool_calls: Vec<ToolCall>,
        input: String,
        cursor_pos: usize,
        is_processing: bool,
        scroll_offset: usize,
        status: String,
        provider_name: String,
        provider_model: String,
        mcp_servers: Vec<String>,
        skills: Vec<String>,
        session_id: Option<String>,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_input_tokens: Option<u64>,
        cache_creation_input_tokens: Option<u64>,
        queued_messages: Vec<String>,
    },

    /// Text delta appended to streaming_text
    TextDelta { text: String },

    /// Tool started
    ToolStart { id: String, name: String },

    /// Tool input delta
    ToolInput { delta: String },

    /// Tool about to execute
    ToolExec { id: String, name: String },

    /// Tool completed
    ToolDone {
        id: String,
        name: String,
        output: String,
        is_error: bool,
    },

    /// Message added to display_messages
    MessageAdded { message: DebugMessage },

    /// Streaming text cleared (turn complete)
    StreamingCleared,

    /// Processing state changed
    ProcessingChanged { is_processing: bool },

    /// Status changed
    StatusChanged { status: String },

    /// Token usage update
    TokenUsage {
        input_tokens: u64,
        output_tokens: u64,
        cache_read_input_tokens: Option<u64>,
        cache_creation_input_tokens: Option<u64>,
    },

    /// Input changed (user typing)
    InputChanged { input: String, cursor_pos: usize },

    /// Scroll offset changed
    ScrollChanged { offset: usize },

    /// Message queued
    MessageQueued { content: String },

    /// Queued message sent
    QueuedMessageSent { index: usize },

    /// Session ID set
    SessionId { id: String },

    /// Thinking started
    ThinkingStart,

    /// Thinking ended
    ThinkingEnd,

    /// Compaction occurred
    Compaction { trigger: String, pre_tokens: u64 },

    /// Error occurred
    Error { message: String },
}

/// Simplified message for debug serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugMessage {
    pub role: String,
    pub content: String,
    pub tool_calls: Vec<String>,
    pub duration_secs: Option<f32>,
    pub title: Option<String>,
    pub tool_data: Option<ToolCall>,
}

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
static CONNECTION_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub struct RemoteConnection {
    generation: u64,
    reader: BufReader<crate::transport::ReadHalf>,
    writer: Arc<Mutex<WriteHalf>>,
    _dummy_peer: Option<Stream>,
    #[cfg(test)]
    fail_after_test_prefix: std::sync::atomic::AtomicBool,
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

mod connection_0;
mod connection_1;
mod connection_2;
mod connection_3;
pub(in crate::tui) mod ssh_gate;

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

#[cfg(test)]
#[path = "backend/tests.rs"]
mod tests;
