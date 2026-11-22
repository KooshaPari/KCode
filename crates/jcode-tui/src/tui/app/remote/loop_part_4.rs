/// Watchdog for the "stuck on loading session…" bug.
///
/// Every remote prompt path is gated behind `RemoteConnection::has_loaded_history()`:
/// `submit_prepared_remote_input` parks prompts in `pending_prompt_before_history`,
/// and `process_remote_followups` returns early at the `!has_loaded_history()`
/// gate. That gate only clears when the server delivers a `History` event. If
/// that event never arrives after a (re)connect or reload handoff (dropped
/// event, momentarily busy agent, or a path that returned without history), the
/// client is stuck forever showing "loading session…" and the only escape is a
/// manual `/restart`.
///
/// This watchdog detects a connection that has been waiting too long for the
/// bootstrap history and re-requests it a bounded number of times. If history
/// still never loads, it surfaces an actionable message instead of leaving the
/// user staring at a frozen header.
async fn recover_stuck_remote_history(app: &mut App, remote: &mut RemoteConnection) -> bool {
    // Once history has loaded the watchdog has nothing to do; make sure its
    // budget is cleared so a later rewind-triggered reload starts fresh.
    if remote.has_loaded_history() {
        if app.remote_history_wait_started.is_some() {
            app.clear_remote_history_wait();
        }
        return false;
    }

    // A pending server reload intentionally leaves history unloaded until the
    // reload fires (see `process_remote_followups`); don't fight that path.
    if app.pending_server_reload {
        return false;
    }

    // Only meaningful for an established remote client connection. During the
    // initial connect/reconnect handshake the run loop drives history loading
    // directly, and there is no point re-requesting before we've attached.
    if !app.is_remote {
        return false;
    }

    let now = Instant::now();
    let waited = match app.remote_history_wait_started {
        Some(started) => now.saturating_duration_since(started),
        None => {
            // Begin tracking from the first tick that observes unloaded history
            // on a live connection.
            app.remote_history_wait_started = Some(now);
            return false;
        }
    };

    if waited < REMOTE_HISTORY_RECOVERY_FIRST_DELAY {
        return false;
    }

    // A large newline-delimited History event may take longer than the
    // watchdog delay to arrive. Once any part of a frame is buffered, the
    // response was not dropped: it is actively being assembled by the reader.
    // Re-requesting here queues another complete (potentially tens-of-MB)
    // History payload behind the first and can keep the connection saturated
    // for minutes. Let the in-flight frame finish instead.
    if remote.has_buffered_inbound_frame() {
        return false;
    }

    if app.remote_history_recovery_attempts >= REMOTE_HISTORY_RECOVERY_MAX_ATTEMPTS {
        // We've exhausted re-requests. Surface a one-time actionable hint so the
        // user isn't stuck on a silent "loading session…" forever.
        if app.remote_history_recovery_last_attempt.is_some() {
            crate::logging::warn(
                "Remote history never loaded after repeated re-requests; session is stuck on \
                 'loading session…'. Advising /restart.",
            );
            app.push_display_message(DisplayMessage::system(
                "▲ Still loading session… the server hasn't sent the conversation history. \
                 This usually clears on its own; if it persists, run /restart to reconnect."
                    .to_string(),
            ));
            app.set_status_notice("Session history not loading - try /restart");
            // Clear last_attempt so we don't repeat the message every tick, but
            // keep attempts at max so we don't re-enter the retry path.
            app.remote_history_recovery_last_attempt = None;
            return true;
        }
        return false;
    }

    // Rate-limit re-requests so we don't flood the server.
    if let Some(last) = app.remote_history_recovery_last_attempt
        && now.saturating_duration_since(last) < REMOTE_HISTORY_RECOVERY_RETRY_INTERVAL
    {
        return false;
    }

    app.remote_history_recovery_attempts += 1;
    app.remote_history_recovery_last_attempt = Some(now);
    crate::logging::warn(&format!(
        "Remote history still not loaded after {}s; re-requesting session history (attempt {}/{}, session={:?})",
        waited.as_secs(),
        app.remote_history_recovery_attempts,
        REMOTE_HISTORY_RECOVERY_MAX_ATTEMPTS,
        app.remote_session_id,
    ));
    match remote.request_history().await {
        Ok(_) => {
            app.set_status_notice("Loading session… re-requesting history");
        }
        Err(err) => {
            crate::logging::error(&format!(
                "History recovery re-request failed: {err}; will retry on next watchdog tick"
            ));
        }
    }
    true
}

/// Record (once per distinct reason) why the restored startup auto-submit is
/// being deferred. This makes "headed-spawn prompt seen but never sent" cases
/// debuggable without spamming a log line on every event-loop tick.
fn note_startup_submit_deferred(app: &mut App, reason: &'static str) {
    if !app.submit_input_on_startup {
        // Nothing pending; clear any stale reason so the next deferral logs.
        app.startup_submit_deferred_reason = None;
        return;
    }
    if app.startup_submit_deferred_reason == Some(reason) {
        return;
    }
    app.startup_submit_deferred_reason = Some(reason);
    crate::logging::info(&format!(
        "Startup auto-submit deferred: {reason} (input_chars={}, pending_images={})",
        app.input.chars().count(),
        app.pending_images.len(),
    ));
}

/// Fire a pending server reload (newer binary on disk, or a server/client
/// runtime-identity mismatch that the History handler intentionally deferred).
///
/// Extracted so it can run BEFORE the `has_loaded_history` gate in
/// `process_remote_followups`: the runtime-identity defer path never marks
/// history as loaded, so the reload must be allowed to fire while history is
/// still pending, otherwise the connection stalls indefinitely on
/// "Loading session...".
async fn dispatch_pending_server_reload(app: &mut App, remote: &mut RemoteConnection) {
    app.pending_server_reload = false;
    if app.auto_server_reload {
        // Defense-in-depth for issue #277: a correct reload only needs to
        // happen once. If we keep being told a newer binary is available
        // after several auto-reloads, treat it as a false-positive loop and
        // stop hammering the server (which would otherwise flicker the UI).
        const MAX_AUTO_SERVER_RELOADS: u32 = 3;
        if app.server_auto_reload_attempts >= MAX_AUTO_SERVER_RELOADS {
            crate::logging::warn(&format!(
                "Suppressing server auto-reload after {} attempts; server keeps reporting an update (possible reload loop, issue #277)",
                app.server_auto_reload_attempts
            ));
            app.push_display_message(DisplayMessage::system(
                "ℹ Server keeps reporting a newer binary after repeated reloads; auto-reload paused to avoid a loop. Use `/reload` manually if needed.".to_string(),
            ));
            app.set_status_notice("Server auto-reload paused (possible loop)");
        } else {
            app.server_auto_reload_attempts += 1;
            app.append_reload_message("Reloading server with newer binary...");
            if let Err(err) = remote.reload().await {
                app.push_display_message(DisplayMessage::error(format!(
                    "Failed to auto-reload server: {}. Use `/reload` to retry.",
                    err
                )));
                app.set_status_notice("Server update available - auto reload failed");
            }
        }
    } else {
        app.push_display_message(DisplayMessage::system(
            "ℹ Newer server binary detected. Auto-reload is disabled by `display.auto_server_reload = false`. Use `/reload` manually when you're ready.".to_string(),
        ));
        app.set_status_notice("Server update available - manual /reload recommended");
    }
}

