use super::*;

pub(in crate::tui::app) async fn handle_post_connect<B: ratatui::backend::Backend>(
    app: &mut App,
    terminal: &mut ratatui::Terminal<B>,
    remote: &mut RemoteConnection,
    state: &mut RemoteRunState,
    session_to_resume: Option<&str>,
) -> Result<PostConnectOutcome> {
    crate::logging::info(&format!(
        "Reload check: session_to_resume={:?}, remote_session_id={:?}, reconnect_attempts={}",
        session_to_resume, app.remote_session_id, state.reconnect_attempts
    ));
    let hints = load_reload_reconnect_hints(app, session_to_resume);
    let has_reload_ctx_for_session = hints.reload_ctx_for_session.is_some();
    if state.reconnect_attempts > 0 {
        if let Some(disconnect_start) = state.disconnect_start {
            crate::logging::info(&format!(
                "Reload reconnect succeeded after {}ms (attempts={})",
                disconnect_start.elapsed().as_millis(),
                state.reconnect_attempts
            ));
        }
        if app.reload_info.is_empty()
            && let Some(ctx) = hints.reload_ctx_for_session.as_ref()
        {
            app.reload_info.push(ctx.reconnect_notice_line());
        }

        // A plain reconnect (server crashed and restarted by the operator, network
        // blip, or PeerClosed after `cargo build`) MUST NOT trigger a client re-exec.
        // Doing so loses the in-flight turn, forces the user to re-launch `jcode`,
        // and creates session-alias drift (short alias vs full session id).
        //
        // A real "server reloaded" is signalled by `ServerEvent::Reloading`, which
        // sets `state.server_reload_in_progress = true` *before* the disconnect
        // arrives. That flag is the only correct trigger for a client reload from
        // the reconnect path. Self-dev idle reloads are handled separately by
        // `maybe_self_reload_after_server_reload`, which gates on `is_selfdev_session`.
        //
        // Bug history: previously this also gated on `app.has_newer_binary()`, which
        // is true whenever the launcher payload has been rebuilt (cargo build in
        // another shell, self-dev iteration, package upgrade). On a transient
        // PeerClosed reconnect that flag triggered an unwanted client restart.
        let must_reload_client = !crate::tui::is_ssh_remote() && state.server_reload_in_progress;

        if must_reload_client {
            app.push_display_message(DisplayMessage::system(
                "Server reloaded. Reloading client binary...".to_string(),
            ));
            terminal
                .draw(|frame| crate::tui::ui::draw(frame, app))
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            // Resolve the real session id (see `reload_handoff_session_id`):
            // prefer the live id, then a deferred-history id, then the launch
            // resume target, and only fabricate as a last resort. Fabricating
            // eagerly here was the root cause of issue #328.
            let session_id = app.reload_handoff_session_id();
            if (has_reload_ctx_for_session || !app.reload_info.is_empty())
                && let Ok(jcode_dir) = crate::storage::jcode_dir()
            {
                let marker = jcode_dir.join(format!("client-reload-pending-{}", session_id));
                let info = if app.reload_info.is_empty() {
                    "reload".to_string()
                } else {
                    app.reload_info.join("\n")
                };
                match std::fs::write(&marker, &info) {
                    Ok(()) => crate::logging::info(&format!(
                        "Wrote client-reload-pending marker for {} before re-exec",
                        session_id
                    )),
                    Err(error) => {
                        crate::logging::warn(&format!("Could not write reload marker: {error}"))
                    }
                }
            }
            app.save_input_for_reload(&session_id);
            app.reload_requested = Some(session_id);
            app.should_quit = true;
            return Ok(PostConnectOutcome::Quit);
        }

        let reload_details = if !app.reload_info.is_empty() {
            format!("\n  {}", app.reload_info.join("\n  "))
        } else if has_reload_ctx_for_session {
            "\n  Reload context restored".to_string()
        } else {
            String::new()
        };

        app.push_display_message(DisplayMessage::system(format!(
            "✓ Reconnected successfully.{}",
            reload_details
        )));
    }

    let reload_ctx_available = hints.reload_ctx_for_session.is_some();
    let history_already_loaded = remote.has_loaded_history();
    let reload_reconnect_needs_server_history = state.server_reload_in_progress
        && !reload_ctx_available
        && !hints.has_client_reload_marker
        && !history_already_loaded;
    let same_session_reload_fast_path = !crate::tui::is_ssh_remote()
        && should_use_same_session_fast_path(
            state.reconnect_attempts > 0,
            session_to_resume,
            app.remote_session_id.as_deref(),
            !app.display_messages.is_empty(),
            reload_reconnect_needs_server_history,
        );

    if reload_reconnect_needs_server_history {
        app.pending_reload_reconnect_status = Some(PendingReloadReconnectStatus::AwaitingHistory {
            session_id: session_to_resume.map(str::to_string),
        });
        app.push_display_message(DisplayMessage::system(
            "Reload complete - checking restored history to decide whether continuation is needed."
                .to_string(),
        ));
        ReloadContext::log_recovery_outcome(
            "tui_reconnect",
            session_to_resume.unwrap_or("unknown"),
            "deferred",
            "reload reconnect without local reload context; waiting for server history payload to determine continuation",
        );
    }

    if same_session_reload_fast_path
        || hints.has_client_reload_marker
        || (reload_ctx_available && history_already_loaded)
    {
        finalize_reload_reconnect(app, session_to_resume, hints, state.reconnect_attempts > 0);
    } else if reload_ctx_available {
        ReloadContext::log_recovery_outcome(
            "tui_reconnect",
            session_to_resume.unwrap_or("unknown"),
            "deferred",
            "waiting for server history payload to deliver reload recovery directive",
        );
        app.reload_info.clear();
    }

    state.reconnect_attempts = 0;
    state.initial_server_start = false;
    state.server_reload_in_progress = false;

    if same_session_reload_fast_path {
        crate::logging::info(
            "Same-session reload fast path: skipping blocking History wait and reusing local display state",
        );
        remote.mark_history_loaded();
        app.clear_remote_startup_phase();
        app.clear_remote_history_wait();
    } else if !remote.has_loaded_history() {
        app.set_remote_startup_phase(super::super::super::RemoteStartupPhase::LoadingSession);
        // Start a fresh history-recovery budget for this connection so the
        // watchdog can re-request the bootstrap History payload if it never
        // arrives (otherwise the session is stuck on "loading session…" until
        // the user runs /restart).
        app.begin_remote_history_wait();
    } else {
        app.clear_remote_startup_phase();
        app.clear_remote_history_wait();
    }

    // Hydrate route metadata as part of connection bootstrap, not when the
    // user opens `/model`. History intentionally carries names only, so without
    // this follow-up the public picker exposes generic `remote-catalog` rows.
    // Request ordering keeps the authoritative History snapshot ahead of this
    // expanded catalog response on a newly connected transport.
    if let Err(error) = remote.request_model_catalog().await {
        crate::logging::warn(&format!(
            "Failed to hydrate model routes during remote bootstrap: {error}"
        ));
    }

    // Dispatch restored work once the server history is in place. This must
    // also cover a pending startup submission (e.g. a headed swarm spawn whose
    // initial prompt was staged into `app.input` with `submit_input_on_startup`),
    // not just queued follow-ups. Without this, a freshly spawned visible agent
    // would show its prompt in the input box but never actually submit it,
    // because `process_remote_followups` (the only production dispatcher) was
    // never invoked post-connect. See issues #267/#268/#76.
    if !super::uncertain::has_unresolved_for_session(app, state)
        && !app.is_processing
        && (app.has_queued_followups() || app.has_pending_startup_submission())
    {
        crate::logging::info(
            "Post-connect has queued followups or startup submission; dispatching immediately",
        );
        if app.pending_queued_dispatch {
            crate::logging::info(
                "Clearing deferred queued-dispatch flag during post-connect so restored followups continue immediately",
            );
            app.pending_queued_dispatch = false;
        }
        process_remote_followups(app, remote).await;
    }

    Ok(PostConnectOutcome::Ready)
}

pub(super) fn load_reload_reconnect_hints(
    app: &mut App,
    session_to_resume: Option<&str>,
) -> ReloadReconnectHints {
    if crate::tui::is_ssh_remote() {
        return ReloadReconnectHints::default();
    }
    let reload_ctx_for_session = session_to_resume.and_then(|sid| {
        let result = ReloadContext::peek_for_session(sid);
        crate::logging::info(&format!(
            "Reload peek_for_session({}) = {:?}",
            sid,
            result.as_ref().map(|r| r.is_some())
        ));
        match result {
            Ok(value) => value,
            Err(error) => {
                crate::logging::warn(&format!("Reload context preview failed: {error}"));
                None
            }
        }
    });

    let has_client_reload_marker = session_to_resume
        .and_then(|sid| {
            let jcode_dir = match crate::storage::jcode_dir() {
                Ok(path) => path,
                Err(error) => {
                    crate::logging::warn(&format!("Reload marker directory unavailable: {error}"));
                    return None;
                }
            };
            let marker = jcode_dir.join(format!("client-reload-pending-{}", sid));
            if marker.exists() {
                let info = match std::fs::read_to_string(&marker) {
                    Ok(info) => info,
                    Err(error) => {
                        crate::logging::warn(&format!("Reload marker read failed: {error}"));
                        return None;
                    }
                };
                if let Err(error) = std::fs::remove_file(&marker) {
                    crate::logging::warn(&format!("Reload marker removal failed: {error}"));
                }
                crate::logging::info(&format!(
                    "Found client-reload-pending marker for {}, injecting reload info",
                    sid
                ));
                if app.reload_info.is_empty() {
                    for line in info.lines() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            app.reload_info.push(trimmed.to_string());
                        }
                    }
                }
                Some(())
            } else {
                None
            }
        })
        .is_some();

    ReloadReconnectHints {
        reload_ctx_for_session,
        has_client_reload_marker,
    }
}

pub(in crate::tui::app) fn finalize_reload_reconnect(
    app: &mut App,
    session_to_resume: Option<&str>,
    hints: ReloadReconnectHints,
    reconnected_after_disconnect: bool,
) {
    let should_queue_reload_continuation = hints.reload_ctx_for_session.is_some();
    crate::logging::info(&format!(
        "Reload continuation check: should_queue={}, reload_info_empty={}, has_ctx={}, has_marker={}",
        should_queue_reload_continuation,
        app.reload_info.is_empty(),
        hints.reload_ctx_for_session.is_some(),
        hints.has_client_reload_marker
    ));
    if should_queue_reload_continuation {
        app.pending_reload_reconnect_status = None;
        let reload_ctx = session_to_resume.and_then(|sid| {
            let result = ReloadContext::load_for_session(sid);
            crate::logging::info(&format!(
                "Reload load_for_session({}) = {:?}",
                sid,
                result.as_ref().map(|r| r.is_some())
            ));
            match result {
                Ok(value) => value,
                Err(error) => {
                    crate::logging::warn(&format!("Reload context load failed: {error}"));
                    None
                }
            }
        });

        let background_task_note = match session_to_resume {
            Some(id) => super::super::super::reload_persisted_background_tasks_note(id),
            None => String::new(),
        };
        let directive = ReloadContext::recovery_directive(
            reload_ctx.as_ref(),
            false,
            &background_task_note,
            None,
        );

        if let Some(directive) = directive {
            let session_id = session_to_resume.unwrap_or("unknown");
            if app.current_message_id.is_none()
                && (app.remote_resume_activity.is_some() || app.is_processing)
            {
                crate::logging::info(
                    "Reload reconnect: clearing stale resumed-processing state before dispatching continuation",
                );
                app.remote_resume_activity = None;
                app.is_processing = false;
                app.status = ProcessingStatus::Idle;
                app.processing_started = None;
                app.clear_visible_turn_started();
                app.last_stream_activity = None;
                app.replay_processing_started_ms = None;
                app.replay_elapsed_override = None;
            }

            crate::logging::info(&format!(
                "Queuing reload continuation message ({} chars)",
                directive.continuation_message.len()
            ));
            ReloadContext::log_recovery_outcome(
                "tui_reconnect",
                session_id,
                "resumed",
                "queued initiator continuation after reconnect",
            );
            app.push_display_message(DisplayMessage::system(
                "Reload complete - continuing because reload recovery was pending.",
            ));
            app.hidden_queued_system_messages
                .push(directive.continuation_message);
        } else {
            ReloadContext::log_recovery_outcome(
                "tui_reconnect",
                session_to_resume.unwrap_or("unknown"),
                "failed",
                "reload context missing for reconnecting initiator session",
            );
            crate::logging::warn(
                "Reload context missing for initiating session after reconnect; skipping selfdev continuation",
            );
        }
        app.reload_info.clear();
    } else if hints.has_client_reload_marker {
        app.pending_reload_reconnect_status = None;
        ReloadContext::log_recovery_outcome(
            "tui_reconnect",
            session_to_resume.unwrap_or("unknown"),
            "skipped",
            "client reload marker present without session reload context",
        );
        if !reconnected_after_disconnect && !app.reload_info.is_empty() {
            app.push_display_message(DisplayMessage::system(app.reload_info.join("\n")));
        }
        app.push_display_message(DisplayMessage::system(
            "Reload complete - no continuation queued because no recovery context was pending."
                .to_string(),
        ));
        app.reload_info.clear();
    }
}
