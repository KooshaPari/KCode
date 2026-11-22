fn handle_ctrl_kill_to_end(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> bool {
    // Match the local draft semantics before remote navigation can claim Ctrl+K.
    // Ctrl+Shift+K remains reserved for scrolling.
    if modifiers.contains(KeyModifiers::CONTROL)
        && !modifiers.contains(KeyModifiers::SHIFT)
        && matches!(code, KeyCode::Char('k'))
        && !app.input.is_empty()
    {
        input::delete_input_to_end(app);
        return true;
    }

    false
}

pub(super) enum RemoteEventOutcome {
    Continue,
    Reconnect,
    PauseProtocol,
    Quit,
}

pub(super) async fn handle_tick(
    app: &mut App,
    remote: &mut RemoteConnection,
    state: &mut RemoteRunState,
) -> bool {
    app.refresh_terminal_title_metrics();
    app.sync_herdr_agent_state();
    crate::tui::ui::set_frame_input_attribution(crate::tui::ui::FrameInputAttribution {
        event: Some("tick".to_string()),
        scroll_delta: None,
        model_picker_open: app
            .inline_interactive_state
            .as_ref()
            .is_some_and(|state| state.kind == crate::tui::PickerKind::Model),
    });
    let mut needs_redraw = crate::tui::periodic_redraw_required(app);
    needs_redraw |= queue_recovery::expire_hidden_ack(app, Instant::now());
    needs_redraw |= app.retire_expired_status_notice();
    needs_redraw |= app.poll_clipboard_outcome(); // settle a late copy outcome (#231)
    needs_redraw |= app.poll_usage_reset();
    if let Some(account) = app.usage_reset.invalidate_account.take() {
        match remote.invalidate_openai_usage(account).await {
            Ok(id) => {
                app.usage_reset.invalidate_requests.insert(id, Some(Instant::now()));
            }
            Err(error) => app.push_display_message(DisplayMessage::error(format!(
                "Reset result is unchanged, but the daemon usage cache could not be refreshed: {error}. Reconnect to refresh daemon state."
            ))),
        }
        needs_redraw = true;
    }
    let mut refresh_timed_out = false;
    for sent_at in app.usage_reset.invalidate_requests.values_mut() {
        if sent_at.is_some_and(|sent| sent.elapsed() >= Duration::from_secs(10)) {
            // Retain the ID so a late control acknowledgement never ends an agent turn.
            *sent_at = None;
            refresh_timed_out = true;
        }
    }
    if refresh_timed_out {
        app.push_display_message(DisplayMessage::system(
            "Reset result is unchanged, but the daemon usage refresh has not been acknowledged. Reconnect if usage stays stale.".to_string(),
        ));
        needs_redraw = true;
    }
    needs_redraw |= app.poll_ssh_login(remote).await;
    needs_redraw |= app.poll_ssh_login_onboarding();
    needs_redraw |= app.flush_pending_resize_redraw();
    app.maybe_capture_runtime_memory_heartbeat();
    app.maybe_release_idle_heap();
    // Surface the cold-cache transcript warning the moment the TTL expires
    // while idle, not only when the next request starts.
    needs_redraw |= app.maybe_push_idle_cold_cache_warning();
    needs_redraw |= app.progress_copy_selection_edge_autoscroll();
    app.progress_mouse_scroll_animation();
    needs_redraw |= app.update_pinned_images_auto_hide();
    // Dissolve stale (off-screen) reasoning traces with zero visible motion.
    needs_redraw |= app.gc_offscreen_reasoning_traces();
    needs_redraw |= dispatch_compacted_history_load(app, remote).await;
    // Adopt the resolved scroll position once a frame containing newly loaded
    // older history has rendered, so manual scrolling resumes seamlessly.
    needs_redraw |= app.reconcile_history_anchor();
    // Same for a resize: adopt the resolved row once the rewrap has rendered.
    needs_redraw |= app.reconcile_resize_anchor();
    // Reveal buffered streaming text at the smooth paced rate on each tick, the
    // same as the local turn loop. When Done arrived with a backlog, leave one
    // rendered live frame after the final reveal before committing the turn.
    let stream_backlog_was_empty = app.stream_buffer.is_empty();
    let ops = app.stream_buffer.flush_smooth_frame();
    if app.apply_stream_ops(ops) {
        needs_redraw = true;
    }
    if stream_backlog_was_empty
        && app.stream_buffer.is_empty()
        && let Some(id) = app.deferred_stream_done_id.take()
    {
        needs_redraw |= app.handle_server_event(crate::protocol::ServerEvent::Done { id }, remote);
    }

    needs_redraw |= app.refresh_todos_view_if_needed();
    needs_redraw |= app.refresh_todo_card_if_needed();
    needs_redraw |= app.refresh_pinned_todos_if_needed();
    needs_redraw |= app.prune_irrelevant_background_tasks();
    needs_redraw |= app.refresh_side_panel_linked_content_if_due();
    needs_redraw |= app.poll_model_picker_load();
    needs_redraw |= app.poll_session_picker_load();
    needs_redraw |= app.poll_session_picker_presence();
    needs_redraw |= app.onboarding_tick();
    needs_redraw |= app.progress_update_simulator();
    needs_redraw |= app.refresh_keybindings_if_config_reloaded();

    let _ = check_debug_command(app, remote).await;

    if !app.is_processing {
        if let Some(request) = app.take_pending_catchup_resume() {
            match remote.resume_session(&request.target_session_id).await {
                Ok(()) => {
                    let label = crate::id::extract_session_name(&request.target_session_id)
                        .map(|name| name.to_string())
                        .unwrap_or_else(|| request.target_session_id.clone());
                    let show_brief = request.show_brief;
                    app.begin_in_flight_catchup_resume(request);
                    app.set_status_notice(if show_brief {
                        format!("Catch Up → {}", label)
                    } else {
                        format!("Back → {}", label)
                    });
                    return true;
                }
                Err(err) => {
                    app.clear_in_flight_catchup_resume();
                    app.push_display_message(DisplayMessage::error(format!(
                        "Failed to switch Catch Up session: {}",
                        err
                    )));
                    needs_redraw = true;
                }
            }
        }

        if let Some(target_session) = app.workspace_client.take_pending_resume_session() {
            match remote.resume_session(&target_session).await {
                Ok(()) => {
                    let label = crate::id::extract_session_name(&target_session)
                        .map(|name| name.to_string())
                        .unwrap_or(target_session);
                    app.set_status_notice(format!("Workspace → {}", label));
                    return true;
                }
                Err(err) => {
                    app.push_display_message(DisplayMessage::error(format!(
                        "Failed to switch workspace session: {}",
                        err
                    )));
                    needs_redraw = true;
                }
            }
        }
    }

    if reconnect::has_unresolved_for_session(app, state) {
        needs_redraw |= recover_stuck_remote_history(app, remote).await;
        return needs_redraw;
    }

    let session = app
        .remote_session_id
        .clone()
        .or_else(|| app.resume_session_id.clone());
    if let Some(index) = state
        .held_voice_wakes
        .iter()
        .position(|held| *held == session)
    {
        state.held_voice_wakes.remove(index);
        needs_redraw |= handle_bus_event(app, remote, Ok(BusEvent::VoiceInputWake), state).await;
    }

    if let Some(reset_time) = app.rate_limit_reset
        && Instant::now() >= reset_time
    {
        app.rate_limit_reset = None;
        if !app.is_processing
            && let Some(pending) = app.rate_limit_pending_message.clone()
        {
            if matches!(app.status, ProcessingStatus::WaitingForNetwork { .. })
                && !crate::network_retry::is_probably_online().await
            {
                app.schedule_pending_remote_network_wait("network probe still failing");
                return true;
            }
            if matches!(app.status, ProcessingStatus::WaitingForNetwork { .. }) {
                app.status = ProcessingStatus::Idle;
                app.status_detail = None;
            }
            let status = if pending.auto_retry {
                format!(
                    "✓ Retrying continuation...{}",
                    if pending.is_system {
                        " (system message)"
                    } else {
                        ""
                    }
                )
            } else {
                format!(
                    "✓ Rate limit reset. Retrying...{}",
                    if pending.is_system {
                        " (system message)"
                    } else {
                        ""
                    }
                )
            };
            app.push_display_message(DisplayMessage::system(status));
            let _ = begin_remote_send(
                app,
                remote,
                pending.content,
                pending.images,
                pending.is_system,
                pending.system_reminder,
                pending.auto_retry,
                pending.retry_attempts,
            )
            .await;
            return true;
        }
    }

    // Esc redirect hold. On timeout this arms pending_queued_dispatch, which
    // the next check hands to the normal follow-up path.
    let holding_for_interrupt = app.awaiting_remote_interrupt_ack();
    if holding_for_interrupt || app.pending_queued_dispatch {
        return needs_redraw;
    }

    if !app.is_processing && !app.queued_messages.is_empty() {
        let queued_messages = std::mem::take(&mut app.queued_messages);
        let hidden_reminders = std::mem::take(&mut app.hidden_queued_system_messages);
        let (messages, reminder, display_system_messages) =
            super::helpers::partition_queued_messages(queued_messages, hidden_reminders);
        let combined = messages.join("\n\n");
        let auto_retry = reminder.is_some() && messages.is_empty();
        crate::logging::info(&format!(
            "Sending queued continuation message ({} chars)",
            combined.len()
        ));
        for msg in display_system_messages {
            app.push_display_message(DisplayMessage::system(msg));
        }
        for msg in &messages {
            app.push_display_message(DisplayMessage::user(msg.clone()));
        }
        let result = input_dispatch::begin_remote_send_with_origin(
            app,
            remote,
            combined.clone(),
            vec![],
            true,
            reminder.clone(),
            auto_retry,
            0,
            input_dispatch::queued_user_text(&messages),
        )
        .await;
        if let Err(error) = result {
            if input_dispatch::is_uncertain_ssh_write(&error) {
                return true;
            }
            // The send never reached the server (e.g. the socket died under a
            // reload handoff). Dropping the dequeued messages here would lose
            // them permanently (issue #391); put them back so the queue
            // re-dispatches after reconnect.
            crate::logging::error(
                "Failed to send queued continuation message; restoring it to the queue",
            );
            if let Some(reminder) = reminder {
                app.hidden_queued_system_messages.insert(0, reminder);
            }
            if !combined.is_empty() {
                app.queued_messages.insert(0, combined);
            }
        }
        needs_redraw = true;
    }

    if !app.is_processing && !app.hidden_queued_system_messages.is_empty() {
        let reminders = std::mem::take(&mut app.hidden_queued_system_messages);
        let combined = reminders.join("\n\n");
        crate::logging::info(&format!(
            "Sending hidden continuation reminder ({} chars)",
            combined.len()
        ));
        if let Err(error) = begin_remote_send(
            app,
            remote,
            String::new(),
            vec![],
            true,
            Some(combined.clone()),
            true,
            0,
        )
        .await
        {
            if input_dispatch::is_uncertain_ssh_write(&error) {
                return true;
            }
            crate::logging::error(
                "Failed to send hidden continuation reminder; restoring it to the queue",
            );
            app.hidden_queued_system_messages.insert(0, combined);
        }
        needs_redraw = true;
    }

    detect_and_cancel_stall(app, remote, state).await;
    needs_redraw |= recover_stuck_remote_history(app, remote).await;
    needs_redraw |= detect_starved_queued_followup(app);
    needs_redraw |= app.maybe_finish_background_client_reload();
    needs_redraw
}
