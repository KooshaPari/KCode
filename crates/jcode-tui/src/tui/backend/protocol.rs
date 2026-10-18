//! Read and write pipelines for the remote protocol.
//!
//! - **Read pipeline**: `next_event`, `take_buffered_line`,
//!   `classify_protocol_line` turn a `ReadHalf` socket into a stream of
//!   `RemoteRead` values while staying cancellation-safe across `select!`
//!   peers.
//! - **Write pipeline**: `send_request_with_interrupt_trigger`,
//!   `send_request`, `send_request_detached` write JSON-request frames to
//!   the shared writer and tag interrupt-class requests with their UI
//!   trigger so server-side logs can correlate them.
//! - **Tool-diff state machine**: `handle_tool_*`, `get_tool_input`,
//!   `clear_pending`, `call_output_tokens_seen`,
//!   `reset_call_output_tokens_seen` forward into the
//!   `RemoteDiffTracker`.
//!
//! Kept as a sibling of `mod.rs` so the framing / classification logic is
//! not buried inside the orchestrator that owns the connection state.

use super::{
    DETACHED_REQUEST_TIMEOUT, LineOutcome, MAX_STRAY_REMOTE_PROTOCOL_LINES,
    READ_BUFFER_RETAIN_CAPACITY, READ_BUFFER_SHRINK_THRESHOLD,
    RemoteConnection, RemoteDisconnectReason, RemoteRead,
    remote_protocol_frame_exceeds_limit,
};
use crate::protocol::{Request, ServerEvent};
use anyhow::Result;
use std::time::Instant;
use tokio::io::AsyncWriteExt;

    fn interrupt_request_log_fields(
        &self,
        request: &Request,
        trigger: Option<&str>,
    ) -> Option<String> {
        let trigger = trigger.unwrap_or("unspecified");
        let base = |kind: &str, id: u64| {
            format!(
                "kind={} id={} trigger={} session={:?} client_instance={:?}",
                kind, id, trigger, self.session_id, self.client_instance_id
            )
        };

        match request {
            Request::Cancel { id } => Some(base("cancel", *id)),
            Request::SoftInterrupt {
                id,
                content,
                urgent,
                ..
            } => Some(format!(
                "{} urgent={} content_bytes={} content_chars={}",
                base("soft_interrupt", *id),
                urgent,
                content.len(),
                content.chars().count()
            )),
            Request::CancelSoftInterrupts { id } => Some(base("cancel_soft_interrupts", *id)),
            Request::BackgroundTool { id } => Some(base("background_tool", *id)),
            _ => None,
        }
    }

    async fn send_request_with_interrupt_trigger(
        &self,
        request: Request,
        interrupt_trigger: Option<&str>,
    ) -> Result<()> {
        let control_id = match &request {
            Request::Subscribe { id, .. }
            | Request::GetHistory { id }
            | Request::ResumeSession { id, .. }
            | Request::GetModelCatalog { id, .. }
            | Request::GetState { id } => Some(*id),
            _ => None,
        };
        if let Some(id) = control_id {
            let mut ids = self
                .control_done_ids
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if ids.len() >= 256 {
                ids.pop_front();
            }
            ids.push_back(id);
        }
        let json = serde_json::to_string(&request)? + "\n";
        let interrupt_log = self.interrupt_request_log_fields(&request, interrupt_trigger);
        if let Some(fields) = &interrupt_log {
            crate::logging::info(&format!(
                "REMOTE_INTERRUPT_SEND_START {} json_bytes={}",
                fields,
                json.len()
            ));
        }

        let total_start = Instant::now();
        let writer_wait_start = Instant::now();
        let mut w = self.writer.lock().await;
        let writer_wait_ms = writer_wait_start.elapsed().as_millis();
        let write_start = Instant::now();
        let result = w.write_all(json.as_bytes()).await;
        if let Some(fields) = &interrupt_log {
            match &result {
                Ok(()) => crate::logging::info(&format!(
                    "REMOTE_INTERRUPT_SEND_OK {} writer_wait_ms={} write_ms={} total_ms={}",
                    fields,
                    writer_wait_ms,
                    write_start.elapsed().as_millis(),
                    total_start.elapsed().as_millis()
                )),
                Err(error) => crate::logging::warn(&format!(
                    "REMOTE_INTERRUPT_SEND_ERR {} writer_wait_ms={} write_ms={} total_ms={} error={}",
                    fields,
                    writer_wait_ms,
                    write_start.elapsed().as_millis(),
                    total_start.elapsed().as_millis(),
                    error
                )),
            }
        }
        result?;
        Ok(())
    }

    async fn send_request(&self, request: Request) -> Result<()> {
        self.send_request_with_interrupt_trigger(request, None)
            .await
    }

    fn send_request_detached(&self, request: Request, label: &'static str) {
        let writer = Arc::clone(&self.writer);
        tokio::spawn(async move {
            let json = match serde_json::to_string(&request) {
                Ok(json) => json + "\n",
                Err(error) => {
                    crate::logging::warn(&format!(
                        "Failed to serialize detached remote request {}: {}",
                        label, error
                    ));
                    return;
                }
            };

            let write_future = async {
                let mut w = writer.lock().await;
                w.write_all(json.as_bytes()).await
            };

            match tokio::time::timeout(DETACHED_REQUEST_TIMEOUT, write_future).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    crate::logging::warn(&format!(
                        "Detached remote request {} failed: {}",
                        label, error
                    ));
                }
                Err(_) => {
                    crate::logging::warn(&format!(
                        "Detached remote request {} timed out after {:?}",
                        label, DETACHED_REQUEST_TIMEOUT
                    ));
                }
            }
        });
    }

    /// Send a message to the server

    pub async fn next_event(&mut self) -> RemoteRead {
        let mut stray_lines = 0usize;
        loop {
            // Serve any complete line already buffered before touching the
            // socket. This drains backlog left by a previous (possibly
            // cancelled) call before issuing a new read.
            if let Some(line) = self.take_buffered_line() {
                match self.classify_protocol_line(line, &mut stray_lines) {
                    LineOutcome::Event(event) => return RemoteRead::Event(*event),
                    LineOutcome::Skip => continue,
                    LineOutcome::Disconnect(reason) => return RemoteRead::Disconnected(reason),
                }
            }

            // No complete line yet: pull more bytes. `fill_buf` is cancel safe,
            // so if a `select!` peer wins the race here nothing is read or lost.
            let chunk = match self.reader.fill_buf().await {
                Ok(buf) => buf,
                Err(error) => {
                    crate::logging::warn(&format!(
                        "RemoteConnection::next_event: io error={} (session_id={:?}, client_instance_id={:?})",
                        error, self.session_id, self.client_instance_id
                    ));
                    return RemoteRead::Disconnected(RemoteDisconnectReason::Io(error.to_string()));
                }
            };
            if chunk.is_empty() {
                // EOF. Surface any trailing partial line for diagnostics, then
                // report a clean peer-closed disconnect.
                if self.read_buffer.is_empty() {
                    crate::logging::warn(&format!(
                        "RemoteConnection::next_event: peer closed (session_id={:?}, client_instance_id={:?})",
                        self.session_id, self.client_instance_id
                    ));
                } else {
                    let preview: String = String::from_utf8_lossy(&self.read_buffer)
                        .chars()
                        .take(240)
                        .collect();
                    crate::logging::warn(&format!(
                        "RemoteConnection::next_event: peer closed mid-line, discarding {} buffered bytes preview={:?} (session_id={:?}, client_instance_id={:?})",
                        self.read_buffer.len(),
                        preview,
                        self.session_id,
                        self.client_instance_id
                    ));
                    self.read_buffer.clear();
                    self.read_buffer_scan_start = 0;
                }
                return RemoteRead::Disconnected(RemoteDisconnectReason::PeerClosed);
            }
            let len = chunk.len();
            if remote_protocol_frame_exceeds_limit(self.read_buffer.len(), len) {
                self.reader.consume(len);
                self.read_buffer.clear();
                self.read_buffer_scan_start = 0;
                return RemoteRead::Disconnected(RemoteDisconnectReason::Protocol(format!(
                    "remote protocol frame exceeded {} bytes",
                    MAX_REMOTE_PROTOCOL_FRAME_BYTES
                )));
            }
            self.read_buffer.extend_from_slice(chunk);
            self.reader.consume(len);
        }
    }

    /// Split off the next complete newline-delimited line (without the trailing
    /// `\n`) from the persistent read buffer, leaving any partial remainder in
    /// place for the next read.
    fn take_buffered_line(&mut self) -> Option<Vec<u8>> {
        // Only inspect bytes appended since the previous unsuccessful scan.
        // Without this cursor, a 27 MB History line delivered in 8 KB chunks
        // causes roughly 45 GB of redundant memory scanning.
        let scan_start = self.read_buffer_scan_start.min(self.read_buffer.len());
        let unscanned = &self.read_buffer[scan_start..];
        let relative_newline = unscanned.iter().position(|&b| b == b'\n');
        #[cfg(test)]
        {
            self.protocol_bytes_scanned += relative_newline.map_or(unscanned.len(), |i| i + 1);
        }
        let Some(relative_newline) = relative_newline else {
            self.read_buffer_scan_start = self.read_buffer.len();
            return None;
        };
        let newline = scan_start + relative_newline;
        let mut line: Vec<u8> = self.read_buffer.drain(..=newline).collect();
        line.pop(); // drop trailing '\n'
        // `position` stopped at the first newline, so none of the remaining
        // bytes have been inspected yet.
        self.read_buffer_scan_start = 0;
        // A single oversized line (e.g. a multi-megabyte `History` event)
        // permanently grows this persistent buffer. Once the line has been
        // split off, release the excess so each connection returns to a small
        // steady-state footprint. The mostly-unused check (len < cap/4) plus
        // the retain floor keep normal streaming from ever reallocating.
        if self.read_buffer.capacity() > READ_BUFFER_SHRINK_THRESHOLD
            && self.read_buffer.len() < self.read_buffer.capacity() / 4
        {
            self.read_buffer
                .shrink_to(self.read_buffer.len().max(READ_BUFFER_RETAIN_CAPACITY));
        }
        Some(line)
    }

    /// Decide what a single decoded protocol line means: a real event, a line
    /// to skip (blank/stray), or a fatal protocol/transport error.
    fn classify_protocol_line(&self, line: Vec<u8>, stray_lines: &mut usize) -> LineOutcome {
        let text = match String::from_utf8(line) {
            Ok(text) => text,
            Err(error) => {
                *stray_lines += 1;
                let preview: String = String::from_utf8_lossy(error.as_bytes())
                    .chars()
                    .take(240)
                    .collect();
                crate::logging::warn(&format!(
                    "RemoteConnection::next_event: skipping stray non-UTF-8 protocol line {}/{} preview={:?} (session_id={:?}, client_instance_id={:?})",
                    *stray_lines,
                    MAX_STRAY_REMOTE_PROTOCOL_LINES,
                    preview,
                    self.session_id,
                    self.client_instance_id
                ));
                if *stray_lines >= MAX_STRAY_REMOTE_PROTOCOL_LINES {
                    return LineOutcome::Disconnect(RemoteDisconnectReason::Protocol(
                        "too many stray non-JSON protocol lines".to_string(),
                    ));
                }
                return LineOutcome::Skip;
            }
        };
        let trimmed = text.trim_start();
        if trimmed.trim().is_empty() {
            crate::logging::warn(&format!(
                "RemoteConnection::next_event: skipping blank line (session_id={:?}, client_instance_id={:?})",
                self.session_id, self.client_instance_id
            ));
            return LineOutcome::Skip;
        }
        if !trimmed.starts_with('{') {
            *stray_lines += 1;
            let preview: String = text.chars().take(240).collect();
            crate::logging::warn(&format!(
                "RemoteConnection::next_event: skipping stray non-JSON protocol line {}/{} preview={:?} (session_id={:?}, client_instance_id={:?})",
                *stray_lines,
                MAX_STRAY_REMOTE_PROTOCOL_LINES,
                preview,
                self.session_id,
                self.client_instance_id
            ));
            if *stray_lines >= MAX_STRAY_REMOTE_PROTOCOL_LINES {
                return LineOutcome::Disconnect(RemoteDisconnectReason::Protocol(
                    "too many stray non-JSON protocol lines".to_string(),
                ));
            }
            return LineOutcome::Skip;
        }
        match serde_json::from_str(&text) {
            Ok(ServerEvent::Done { id })
                if self
                    .control_done_ids
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .contains(&id) =>
            {
                LineOutcome::Skip
            }
            Ok(event) => LineOutcome::Event(Box::new(event)),
            Err(error) => {
                // A single unparseable JSON line (e.g. the tail half of a frame
                // split by a lost write, or an event variant this client build
                // doesn't know) must not kill the whole session. Count it
                // against the stray-line budget and resync on the next line;
                // only give up if the stream keeps failing to parse (which
                // indicates a real protocol/version mismatch). See issue #422:
                // huge sessions used to die permanently on one corrupt frame.
                *stray_lines += 1;
                let preview: String = text.chars().take(240).collect();
                crate::logging::warn(&format!(
                    "RemoteConnection::next_event: skipping unparseable protocol line {}/{} error={} preview={:?} (session_id={:?}, client_instance_id={:?})",
                    *stray_lines,
                    MAX_STRAY_REMOTE_PROTOCOL_LINES,
                    error,
                    preview,
                    self.session_id,
                    self.client_instance_id
                ));
                if *stray_lines >= MAX_STRAY_REMOTE_PROTOCOL_LINES {
                    return LineOutcome::Disconnect(RemoteDisconnectReason::Protocol(format!(
                        "too many unparseable protocol lines; last error: {}",
                        error
                    )));
                }
                LineOutcome::Skip
            }
        }
    }


    pub fn handle_tool_start(&mut self, id: &str, name: &str) {
        self.tool_diff.handle_tool_start(id, name);
    }

    /// Handle tool input delta
    pub fn handle_tool_input(&mut self, id: Option<&str>, delta: &str) {
        self.tool_diff.handle_tool_input(id, delta);
    }

    /// Get parsed input for this call (before handle_tool_exec clears it)
    pub fn get_tool_input(&self, id: &str) -> serde_json::Value {
        self.tool_diff.tool_input_json(id)
    }

    /// Handle tool exec - cache file content if edit/write
    pub fn handle_tool_exec(&mut self, id: &str, name: &str) {
        self.tool_diff.handle_tool_exec(id, name);
    }

    /// Handle tool done - generate diff if we have pending data
    pub fn handle_tool_done(&mut self, id: &str, name: &str, output: &str) -> String {
        self.tool_diff.finish_tool(id, name, output)
    }

    /// Clear pending diff state
    pub fn clear_pending(&mut self) {
        self.tool_diff.clear();
    }

    /// Per-API-call output token watermark (for TPS delta accumulation).
    pub fn call_output_tokens_seen(&mut self) -> &mut u64 {
        &mut self.call_output_tokens_seen
    }

    /// Reset per-call output token watermark.
    pub fn reset_call_output_tokens_seen(&mut self) {
        self.call_output_tokens_seen = 0;
    }
}
