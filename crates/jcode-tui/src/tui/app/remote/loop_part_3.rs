pub(super) async fn handle_remote_event<B: Backend>(
    app: &mut App,
    terminal: &mut Terminal<B>,
    remote: &mut RemoteConnection,
    state: &mut RemoteRunState,
    event: RemoteRead,
) -> Result<(RemoteEventOutcome, bool)> {
    match event {
        RemoteRead::Disconnected(reason) => {
            queue_recovery::mark_hidden_unknown(app);
            if let RemoteDisconnectReason::Protocol(error) = &reason {
                if crate::tui::is_ssh_remote() {
                    handle_disconnect(app, state, Some(reason));
                    return Ok((RemoteEventOutcome::PauseProtocol, true));
                }
                let detail = format_disconnect_reason(&reason);
                crate::logging::error(&format!(
                    "Remote protocol error is not retryable; stopping reconnect loop: {}",
                    error
                ));
                app.push_display_message(DisplayMessage::error(format!(
                    "Remote protocol error. Stopped reconnecting to avoid replaying a large/corrupt session repeatedly. {}\n\nTry starting a fresh session, or resume after reducing/removing oversized tool output from the session history.",
                    detail
                )));
                app.set_status_notice("Remote protocol error");
                app.is_processing = false;
                app.status = ProcessingStatus::Idle;
                return Ok((RemoteEventOutcome::Quit, true));
            }
            handle_disconnect(app, state, Some(reason));
            Ok((RemoteEventOutcome::Reconnect, true))
        }
        RemoteRead::Event(ServerEvent::Reloading { new_socket }) => {
            let _ = new_socket;
            state.server_reload_in_progress = true;
            state.reload_recovery_attempted = false;
            state.last_disconnect_reason = Some("server reload in progress".to_string());
            let needs_redraw =
                handle_server_event(app, ServerEvent::Reloading { new_socket: None }, remote);
            process_remote_followups_guarded(app, remote, state).await;
            Ok((RemoteEventOutcome::Continue, needs_redraw))
        }
        RemoteRead::Event(ServerEvent::ClientDebugRequest { id, command }) => {
            // Frame-oriented debug commands used to enable visual debugging and
            // immediately read the frame buffer. On the first request the buffer
            // is still empty because no draw has happened since enabling capture.
            // Render once before producing the response so callers always receive
            // the current TUI state rather than "no frames captured".
            if debug_command_needs_current_frame(&command) {
                crate::tui::visual_debug::enable();
                if let Err(error) = terminal.draw(|frame| crate::tui::ui::draw(frame, app)) {
                    let output = format!("ERR: failed to capture current frame: {error}");
                    let _ = remote.send_client_debug_response(id, output).await;
                    return Ok((RemoteEventOutcome::Continue, false));
                }
            }
            let output = handle_debug_command(app, &command, remote).await;
            let _ = remote.send_client_debug_response(id, output).await;
            process_remote_followups_guarded(app, remote, state).await;
            Ok((RemoteEventOutcome::Continue, false))
        }
        RemoteRead::Event(ServerEvent::Transcript { text, mode }) => {
            if reconnect::has_unresolved_for_session(app, state) {
                apply_transcript_event(app, text, crate::protocol::TranscriptMode::Append);
                app.set_status_notice("Transcript retained in draft; accept the unknown outcome with Ctrl+E before sending");
                return Ok((RemoteEventOutcome::Continue, true));
            }
            let mut needs_redraw = false;
            if let Err(error) = apply_remote_transcript_event(app, remote, text, mode).await {
                app.push_display_message(DisplayMessage::error(format!(
                    "Failed to apply transcript: {}",
                    error
                )));
                app.set_status_notice("Transcript failed");
                needs_redraw = true;
            }
            process_remote_followups_guarded(app, remote, state).await;
            Ok((RemoteEventOutcome::Continue, needs_redraw))
        }
        RemoteRead::Event(server_event) => {
            if queue_recovery::hidden_late_event(app, remote.generation(), &server_event) {
                return Ok((RemoteEventOutcome::Continue, false));
            }
            if app.ssh_hidden_generation == Some(remote.generation()) && let Some((request_id, _, _)) = &app.ssh_hidden_inflight {
                match &server_event {
                    ServerEvent::Ack { id } if Some(*id) == *request_id => { app.ssh_hidden_ack_deadline = None; }
                    ServerEvent::Error { id, .. } if *id == 0 || Some(*id) == *request_id => {
                        let owns_current = app.ssh_hidden_inflight.as_ref().is_some_and(|(request_id, session, _)| request_id.is_some() && *request_id == app.current_message_id && app.remote_session_id.as_deref() == Some(session.as_str()));
                        queue_recovery::mark_hidden_unknown(app);
                        if owns_current { app.is_processing = false; app.current_message_id = None; }
                        app.set_status_notice("Hidden outcome unknown or rejected; reconcile, then /ssh-hidden-accept SESSION");
                        return Ok((RemoteEventOutcome::Continue, true));
                    }
                    ServerEvent::Done { id } if Some(*id) == *request_id => {
                        if queue_recovery::complete_hidden_recovery(app) {
                            app.set_status_notice("Hidden work completed for the selected session");
                        } else {
                            queue_recovery::mark_hidden_unknown(app);
                            app.set_status_notice("Hidden quarantine changed; retained for reconciliation without retry");
                        }
                    }
                    _ => {}
                }
            }
            if let ServerEvent::Error { id, .. } = &server_event
                && crate::tui::is_ssh_remote()
                && !app.usage_reset.invalidate_requests.contains_key(id)
                && app.current_message_id.is_some_and(|current| current != *id)
            {
                crate::logging::warn(&format!(
                    "Ignoring unrelated SSH error id={id} while a turn is in flight"
                ));
                return Ok((RemoteEventOutcome::Continue, false));
            }
            let uncertain_retained = if let ServerEvent::Error { id, .. } = &server_event
                && (app.current_message_id == Some(*id)
                    || (app.current_message_id.is_none()
                        && app.rate_limit_pending_message.is_some()))
                && !app.usage_reset.invalidate_requests.contains_key(id)
            {
                reconnect::retain_uncertain_ssh_request(app, state)
            } else {
                false
            };
            let needs_redraw = server_events::handle_server_event_with_uncertainty(
                app,
                server_event,
                remote,
                uncertain_retained,
            );
            process_remote_followups_guarded(app, remote, state).await;
            Ok((RemoteEventOutcome::Continue, needs_redraw))
        }
    }
}

pub(super) fn debug_command_needs_current_frame(command: &str) -> bool {
    matches!(
        command.trim(),
        "frame"
            | "frame-normalized"
            | "screen"
            | "screen-json"
            | "screen-json-normalized"
            | "layout"
            | "margins"
            | "widgets"
            | "info-widgets"
            | "render-stats"
            | "render-order"
            | "anomalies"
            | "theme"
    )
}

pub(super) fn handle_disconnect(
    app: &mut App,
    state: &mut RemoteRunState,
    reason: Option<RemoteDisconnectReason>,
) {
    let detail = if state.server_reload_in_progress {
        "server reload in progress".to_string()
    } else if let Some(reason) = reason.as_ref() {
        format_disconnect_reason(reason)
    } else {
        "connection to server dropped".to_string()
    };
    crate::logging::warn(&format!(
        "handle_disconnect: session={:?}, remote_session_id={:?}, reason={:?}, detail={}",
        app.resume_session_id, app.remote_session_id, reason, detail
    ));
    // A disconnect can drop the final auth catalog completion event. Keep any
    // queued prompt for reconnect, but release the transient model-setup gate so
    // it cannot remain blocked forever waiting for an event that was lost.
    app.finish_auth_catalog_refresh();
    state.last_disconnect_reason = Some(detail.clone());

    retain_pending_ssh_soft_interrupts(app);
    let scheduled_retry = reconnect::retain_uncertain_ssh_request(app, state)
        || app.schedule_pending_remote_retry(&format!("◈ Connection lost ({detail})."));
    if !scheduled_retry {
        // A queued follow-up that was already dispatched (dequeued into an
        // in-flight send) has no auto-retry path. Dropping it here would lose
        // the user's queued message when a reload/disconnect races the
        // turn-end dispatch (issue #391); put it back on the queue instead so
        // it is re-sent once the turn is proven idle after reconnect.
        if !recover_undelivered_queued_continuation(app, "disconnect") {
            app.clear_pending_remote_retry();
        }
    }
    let recovered_local = recover_local_interleave_to_queue(app, "disconnect");
    app.current_message_id = None;
    app.last_stream_activity = None;
    app.remote_resume_activity = None;
    let ops = app.stream_buffer.flush();
    app.apply_stream_ops(ops);
    if !app.streaming.streaming_text.is_empty() {
        let content = app.take_streaming_text();
        let content = app.collapse_reasoning_for_commit(content);
        if !content.trim().is_empty() {
            app.push_display_message(DisplayMessage {
                role: "assistant".to_string(),
                content,
                tool_calls: vec![],
                duration_secs: None,
                title: None,
                tool_data: None,
            });
        }
    }
    app.clear_streaming_render_state();
    app.streaming_tool_calls.clear();
    app.batch_progress = None;
    app.thought_line_inserted = false;
    app.thinking_prefix_emitted = false;
    app.thinking_buffer.clear();
    if recovered_local || !app.pending_soft_interrupts.is_empty() {
        crate::logging::info(&format!(
            "Preserving {} pending soft interrupt(s) across disconnect",
            app.pending_soft_interrupts.len()
        ));
    }
    app.reset_streaming_tps();
    app.is_processing = false;
    app.status = ProcessingStatus::Idle;
    app.stream_message_ended = false;
    app.clear_visible_turn_started();
    state.disconnect_start = Some(Instant::now());
    state.reconnect_attempts = state.reconnect_attempts.max(1);
    state.reload_recovery_attempted = false;
    app.push_display_message(DisplayMessage {
        role: "system".to_string(),
        content: reconnect_status_message(app, state, &detail),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: Some(CONNECTION_MESSAGE_TITLE.to_string()),
        tool_data: None,
    });
    state.disconnect_msg_idx = Some(app.display_messages.len() - 1);
    state.reconnect_attempts = 1;
}

/// First wait before the watchdog re-requests history. Generous enough that a
/// normal (slow) bootstrap completes on its own, short enough that a genuinely
/// stuck session recovers in seconds instead of requiring a manual `/restart`.
const REMOTE_HISTORY_RECOVERY_FIRST_DELAY: Duration = Duration::from_secs(6);
/// Spacing between subsequent re-requests after the first one.
const REMOTE_HISTORY_RECOVERY_RETRY_INTERVAL: Duration = Duration::from_secs(5);
/// How many times we re-request history before giving up and telling the user
/// to `/restart`. Bounded so a server that genuinely never answers does not
/// spin forever.
const REMOTE_HISTORY_RECOVERY_MAX_ATTEMPTS: u32 = 4;
