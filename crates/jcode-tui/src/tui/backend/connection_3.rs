use super::*;

impl RemoteConnection {
    /// Read the next event from the server.
    ///
    /// This is **cancellation safe** and may be used directly as a branch in a
    /// `tokio::select!`. Every byte pulled from the socket is moved into the
    /// persistent [`Self::read_buffer`] immediately, and the only `.await`
    /// point is [`AsyncBufReadExt::fill_buf`], which tokio guarantees reads
    /// nothing when its future is dropped. A previous implementation used
    /// `BufReader::read_line`, which tokio documents as **not** cancellation
    /// safe: when a large payload (e.g. a multi-megabyte `History` event for a
    /// session with images) was mid-read and a sibling `select!` branch (a
    /// redraw tick, terminal event, or bus event) completed first, the
    /// `read_line` future was dropped and the bytes it had already consumed
    /// from the socket were lost. That desynced the newline-framed protocol
    /// stream: the next read began mid-payload, surfaced as "stray non-JSON
    /// protocol line" warnings, the real `History` was discarded, and the
    /// session stayed stuck on "loading sessionâ€¦" until a manual `/restart`.
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
    pub(super) fn take_buffered_line(&mut self) -> Option<Vec<u8>> {
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
    pub(super) fn classify_protocol_line(
        &self,
        line: Vec<u8>,
        stray_lines: &mut usize,
    ) -> LineOutcome {
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

    /// Get writer for sending requests
    pub fn writer(&self) -> Arc<Mutex<WriteHalf>> {
        Arc::clone(&self.writer)
    }

    pub fn generation(&self) -> u64 { self.generation }

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
            generation: CONNECTION_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            reader: BufReader::new(reader),
            writer: Arc::new(Mutex::new(writer)),
            _dummy_peer: Some(b),
            fail_after_test_prefix: std::sync::atomic::AtomicBool::new(false),
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
    pub(crate) fn inject_partial_write_failure(&self) {
        self.fail_after_test_prefix
            .store(true, std::sync::atomic::Ordering::SeqCst);
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

    /// Handle tool start - begin tracking for diff generation
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
