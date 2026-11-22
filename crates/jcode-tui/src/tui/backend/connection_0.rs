use super::*;

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
        let mut reader = BufReader::new(reader);
        if crate::tui::is_ssh_remote()
            && std::env::var_os("KCODE_SSH_LOCAL_GATE").as_deref()
                == Some(std::ffi::OsStr::new("1"))
        {
            ssh_gate::wait_ready(&mut reader).await?;
        }

        let mut conn = Self {
            generation: CONNECTION_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            reader,
            writer: Arc::new(Mutex::new(writer)),
            _dummy_peer: None,
            #[cfg(test)]
            fail_after_test_prefix: std::sync::atomic::AtomicBool::new(false),
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
        let (working_dir, selfdev) = crate::tui::subscribe_metadata(remote_working_dir);
        let resume_target = resume_session
            .filter(|session_id| {
                crate::tui::is_ssh_remote() || crate::session::session_exists(session_id)
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
            client_has_local_history: client_has_local_history && !crate::tui::is_ssh_remote(),
            allow_session_takeover,
            crash_on_disconnect: false,
            continue_on_disconnect: crate::tui::is_ssh_remote(),
            terminal_env: if crate::tui::is_ssh_remote() {
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

    pub(super) fn interrupt_request_log_fields(
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

    pub(super) async fn send_request_with_interrupt_trigger(
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
        #[cfg(test)]
        if self
            .fail_after_test_prefix
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            w.write_all(&json.as_bytes()[..json.len().min(64)]).await?;
            anyhow::bail!("injected failure after partial request write");
        }
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

    pub(super) async fn send_request(&self, request: Request) -> Result<()> {
        self.send_request_with_interrupt_trigger(request, None)
            .await
    }

    pub(super) fn send_request_detached(&self, request: Request, label: &'static str) {
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
}
