use super::*;

pub(in crate::tui::app) async fn connect_with_retry(
    app: &mut App,
    terminal: &mut DefaultTerminal,
    event_stream: &mut EventStream,
    state: &mut RemoteRunState,
    session_to_resume: Option<&str>,
    remote_working_dir: Option<&str>,
) -> Result<ConnectOutcome> {
    if let Some(outcome) =
        wait_for_reload_handoff_before_reconnect(app, terminal, event_stream, state).await?
    {
        return Ok(outcome);
    }

    let client_has_local_history = !crate::tui::is_ssh_remote()
        && session_to_resume.is_some()
        && !app.display_messages().is_empty();
    let client_instance_id = app.remote_client_instance_id.clone();
    let allow_session_takeover = should_allow_reconnect_takeover(app, state, session_to_resume);
    let connect = RemoteConnection::connect_with_session(
        session_to_resume,
        Some(client_instance_id.as_str()),
        client_has_local_history,
        allow_session_takeover,
        remote_working_dir,
    );
    crate::logging::info(&format!(
        "Remote reconnect attempt: resume={:?}, reconnect_attempts={}, client_instance_id={}, local_history={}, allow_takeover={}",
        session_to_resume,
        state.reconnect_attempts,
        client_instance_id,
        client_has_local_history,
        allow_session_takeover,
    ));
    tokio::pin!(connect);
    let mut redraw = disconnected_redraw_interval(state.reconnect_attempts == 0);

    match loop {
        tokio::select! {
            result = &mut connect => break result,
            _ = redraw.tick() => {
                terminal.draw(|frame| crate::tui::ui::draw(frame, app))?;
            }
            event = event_stream.next() => {
                if handle_terminal_event_while_disconnected(app, terminal, event)? {
                    return Ok(ConnectOutcome::Quit);
                }
            }
        }
    } {
        Ok(remote) => {
            // Request IDs are scoped to a connection. Old acknowledgements cannot
            // arrive on this new socket and must not swallow a new turn's Done.
            app.usage_reset.invalidate_requests.clear();
            crate::logging::info(&format!(
                "[TIMING] remote bootstrap: connected after {}ms (resume={:?}, reconnect_attempts={})",
                app.app_started.elapsed().as_millis(),
                session_to_resume,
                state.reconnect_attempts
            ));
            if let Some(idx) = state.disconnect_msg_idx.take() {
                if app.remove_display_message(idx).is_none() {
                    crate::logging::warn("Reconnect status message was already absent");
                }
            }
            state.disconnect_start = None;
            state.last_disconnect_reason = None;
            state.reload_recovery_attempted = false;
            state.last_reload_pid = None;
            Ok(ConnectOutcome::Connected(remote))
        }
        Err(e) => {
            if let Some(reason) =
                e.downcast_ref::<crate::tui::backend::ssh_gate::LocalSshGateError>()
            {
                return super::local_gate_pause::pause(app, terminal, event_stream, state, reason).await;
            }
            if state.reconnect_attempts == 0 && !app.server_spawning {
                if let Some(host) = crate::tui::ssh_remote_host() {
                    return Err(anyhow::anyhow!(
                        "Failed to connect to the SSH adapter for {host}: {e}"
                    ));
                }
                return Err(anyhow::anyhow!(
                    "Failed to connect to server. Is `jcode serve` running? Error: {}",
                    e
                ));
            }

            let is_initial_server_start = app.server_spawning && state.reconnect_attempts == 0;
            if app.server_spawning && state.reconnect_attempts == 0 {
                state.initial_server_start = true;
                app.server_spawning = false;
            }
            state.reconnect_attempts += 1;
            state.disconnect_start.get_or_insert_with(Instant::now);

            let msg_content = if is_initial_server_start || state.initial_server_start {
                app.set_remote_startup_phase(super::super::super::RemoteStartupPhase::StartingServer);
                "⏳ Starting server...".to_string()
            } else {
                app.set_remote_startup_phase(super::super::super::RemoteStartupPhase::Reconnecting {
                    attempt: state.reconnect_attempts,
                });
                let fallback_reason = e.root_cause().to_string();
                reconnect_status_message(
                    app,
                    state,
                    state
                        .last_disconnect_reason
                        .as_deref()
                        .unwrap_or(fallback_reason.as_str()),
                )
            };

            set_disconnect_status_message(app, state, msg_content);
            terminal.draw(|frame| crate::tui::ui::draw(frame, app))?;

            if reload_handoff_active(state) {
                let socket_path = crate::server::socket_path();
                match crate::server::inspect_reload_wait_status(
                    &socket_path,
                    RELOAD_MARKER_MAX_AGE,
                    state.last_reload_pid,
                )
                .await
                {
                    crate::server::ReloadWaitStatus::Ready => {
                        crate::logging::info(&format!(
                            "Reconnect reload handoff: ready immediately (state={})",
                            crate::server::reload_state_summary(RELOAD_MARKER_MAX_AGE)
                        ));
                        return Ok(ConnectOutcome::Retry);
                    }
                    crate::server::ReloadWaitStatus::Failed(detail) => {
                        crate::logging::warn(&format!(
                            "Reconnect reload handoff: failed detail={:?} state={}",
                            detail,
                            crate::server::reload_state_summary(RELOAD_MARKER_MAX_AGE)
                        ));
                        let detail = detail.unwrap_or_else(|| {
                            "reload failed before the replacement server became ready; starting replacement server"
                                .to_string()
                        });
                        if recover_reloading_server(app, terminal, state, &detail).await? {
                            return Ok(ConnectOutcome::Retry);
                        }
                    }
                    crate::server::ReloadWaitStatus::Idle => {
                        crate::logging::warn(&format!(
                            "Reconnect reload handoff: idle without ready server state={}",
                            crate::server::reload_state_summary(RELOAD_MARKER_MAX_AGE)
                        ));
                        if recover_reloading_server(
                            app,
                            terminal,
                            state,
                            "reload ended without a ready replacement server; starting replacement server",
                        )
                        .await?
                        {
                            return Ok(ConnectOutcome::Retry);
                        }
                    }
                    crate::server::ReloadWaitStatus::Waiting { pid } => {
                        state.last_reload_pid = pid;
                        crate::logging::info(&format!(
                            "Reconnect wait: awaiting reload lifecycle event (pid={:?}, state={})",
                            pid,
                            crate::server::reload_state_summary(RELOAD_MARKER_MAX_AGE)
                        ));
                        let wait = crate::server::wait_for_reload_handoff_event(pid, &socket_path);
                        tokio::pin!(wait);
                        let mut redraw = disconnected_redraw_interval(false);
                        loop {
                            tokio::select! {
                                _ = &mut wait => break,
                                _ = redraw.tick() => {
                                    terminal.draw(|frame| crate::tui::ui::draw(frame, app))?;
                                }
                                event = event_stream.next() => {
                                    if handle_terminal_event_while_disconnected(
                                        app,
                                        terminal,
                                        event,
                                    )? {
                                        return Ok(ConnectOutcome::Quit);
                                    }
                                }
                            }
                        }
                        return Ok(ConnectOutcome::Retry);
                    }
                }
            }

            let backoff = if (state.initial_server_start && state.reconnect_attempts <= 20)
                || state.reconnect_attempts <= 2
            {
                Duration::from_millis(100)
            } else {
                if state.initial_server_start {
                    state.initial_server_start = false;
                }
                Duration::from_secs((1u64 << (state.reconnect_attempts - 2).min(5)).min(30))
            };
            let sleep = tokio::time::sleep(backoff);
            tokio::pin!(sleep);
            let mut redraw = disconnected_redraw_interval(false);
            loop {
                tokio::select! {
                    _ = &mut sleep => break,
                    _ = redraw.tick() => {
                        terminal.draw(|frame| crate::tui::ui::draw(frame, app))?;
                    }
                    event = event_stream.next() => {
                        if handle_terminal_event_while_disconnected(
                            app,
                            terminal,
                            event,
                        )? {
                            return Ok(ConnectOutcome::Quit);
                        }
                    }
                }
            }

            Ok(ConnectOutcome::Retry)
        }
    }
}
