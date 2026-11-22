use super::{
    App, DisplayMessage, ProcessingStatus, handle_terminal_event_while_disconnected,
    process_remote_followups,
};
use crate::tool::selfdev::ReloadContext;
use crate::tui::app::PendingReloadReconnectStatus;
use crate::tui::backend::{RemoteConnection, RemoteDisconnectReason};
use anyhow::Result;
use crossterm::event::EventStream;
use futures::StreamExt;
use ratatui::DefaultTerminal;
use std::time::{Duration, Instant};
use tokio::time::MissedTickBehavior;

const RELOAD_MARKER_MAX_AGE: Duration = Duration::from_secs(30);

#[derive(Default)]
pub(in crate::tui::app) struct RemoteRunState {
    pub reconnect_attempts: u32,
    pub disconnect_msg_idx: Option<usize>,
    pub disconnect_start: Option<Instant>,
    pub initial_server_start: bool,
    pub last_disconnect_reason: Option<String>,
    pub server_reload_in_progress: bool,
    pub reload_recovery_attempted: bool,
    pub last_reload_pid: Option<u32>,
    pub(in crate::tui::app) held_voice_wakes: Vec<Option<String>>,
}

#[expect(
    clippy::large_enum_variant,
    reason = "connected outcome carries a live RemoteConnection while the small control variants remain simple"
)]
pub(in crate::tui::app) enum ConnectOutcome {
    Connected(RemoteConnection),
    Retry,
    Quit,
}

pub(in crate::tui::app) enum PostConnectOutcome {
    Ready,
    Quit,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::tui::app) struct ReloadReconnectHints {
    pub reload_ctx_for_session: Option<ReloadContext>,
    pub has_client_reload_marker: bool,
}

pub(super) fn format_disconnect_reason(reason: &RemoteDisconnectReason) -> String {
    match reason {
        RemoteDisconnectReason::PeerClosed => "server closed the connection".to_string(),
        RemoteDisconnectReason::Io(err) => {
            let lowered = err.to_lowercase();
            if lowered.contains("connection reset") {
                "connection reset by server".to_string()
            } else if lowered.contains("broken pipe") {
                "broken pipe while talking to server".to_string()
            } else if lowered.contains("timed out") {
                "connection timed out".to_string()
            } else {
                err.clone()
            }
        }
        RemoteDisconnectReason::Protocol(err) => {
            format!("protocol error while reading server event: {}", err)
        }
    }
}

pub(in crate::tui::app) fn should_allow_reconnect_takeover(
    app: &App,
    state: &RemoteRunState,
    session_to_resume: Option<&str>,
) -> bool {
    if state.reconnect_attempts == 0 {
        return false;
    }

    let Some(session_to_resume) = session_to_resume else {
        return false;
    };

    app.remote_session_id
        .as_deref()
        .map(|remote_session_id| remote_session_id == session_to_resume)
        .unwrap_or(false)
}

pub(super) fn reconnect_status_message(app: &App, state: &RemoteRunState, detail: &str) -> String {
    let elapsed = state
        .disconnect_start
        .map(|start| start.elapsed())
        .unwrap_or_default();
    let elapsed_str = if elapsed.as_secs() < 60 {
        format!("{}s", elapsed.as_secs())
    } else {
        format!("{}m {}s", elapsed.as_secs() / 60, elapsed.as_secs() % 60)
    };

    let session_id = app
        .remote_session_id
        .as_deref()
        .or(app.resume_session_id.as_deref());
    let resume_hint = if let Some(host) = crate::tui::ssh_remote_host() {
        let id = session_id.unwrap_or("pending");
        format!(" · SSH {host} · remote session {id}")
    } else if let Some(id) = session_id {
        // Resume by full session id: a reusable short name can collide with
        // another session and reopen the wrong one.
        format!(" · resume: kcode --resume {}", id)
    } else {
        String::new()
    };

    format!(
        "⚡ Connection lost - retrying (attempt {}, {}) - {}{}",
        state.reconnect_attempts.max(1),
        elapsed_str,
        detail,
        resume_hint,
    )
}

pub(super) fn reload_wait_status_message(
    app: &App,
    state: &RemoteRunState,
    detail: &str,
) -> String {
    let elapsed = state
        .disconnect_start
        .map(|start| start.elapsed())
        .unwrap_or_default();
    let elapsed_str = if elapsed.as_secs() < 60 {
        format!("{}s", elapsed.as_secs())
    } else {
        format!("{}m {}s", elapsed.as_secs() / 60, elapsed.as_secs() % 60)
    };

    let session_id = app
        .remote_session_id
        .as_deref()
        .or(app.resume_session_id.as_deref());
    let resume_hint = if let Some(host) = crate::tui::ssh_remote_host() {
        let id = session_id.unwrap_or("pending");
        format!(" · SSH {host} · remote session {id}")
    } else if let Some(id) = session_id {
        // Resume by full session id: a reusable short name can collide with
        // another session and reopen the wrong one.
        format!(" · resume: kcode --resume {}", id)
    } else {
        String::new()
    };

    format!(
        "⚡ Server reload in progress - waiting for handoff ({}) - {}{}",
        elapsed_str, detail, resume_hint,
    )
}

fn set_disconnect_status_message(app: &mut App, state: &mut RemoteRunState, content: String) {
    if let Some(idx) = state.disconnect_msg_idx {
        let _ = app.replace_display_message_content(idx, content);
    } else {
        app.push_display_message(DisplayMessage {
            role: "system".to_string(),
            content,
            tool_calls: Vec::new(),
            duration_secs: None,
            title: None,
            tool_data: None,
        });
        state.disconnect_msg_idx = Some(app.display_messages.len() - 1);
    }
}

fn disconnected_redraw_interval(initial_connect: bool) -> tokio::time::Interval {
    let period = if initial_connect {
        crate::tui::REDRAW_REMOTE_STARTUP
    } else {
        Duration::from_millis(1000)
    };
    let mut interval = tokio::time::interval(period);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    interval
}

pub(in crate::tui::app) fn reload_handoff_active(state: &RemoteRunState) -> bool {
    !crate::tui::is_ssh_remote()
        && (state.server_reload_in_progress || super::session_persistence::reload_marker_active())
}

pub(in crate::tui::app) fn should_use_same_session_fast_path(
    reconnected_after_disconnect: bool,
    session_to_resume: Option<&str>,
    remote_session_id: Option<&str>,
    has_display_messages: bool,
    reload_reconnect_needs_server_history: bool,
) -> bool {
    reconnected_after_disconnect
        && !reload_reconnect_needs_server_history
        && session_to_resume
            .zip(remote_session_id)
            .map(|(resume_id, remote_id)| resume_id == remote_id)
            .unwrap_or(false)
        && has_display_messages
}

async fn wait_for_reload_handoff_before_reconnect(
    app: &mut App,
    terminal: &mut DefaultTerminal,
    event_stream: &mut EventStream,
    state: &mut RemoteRunState,
) -> Result<Option<ConnectOutcome>> {
    if !reload_handoff_active(state) {
        return Ok(None);
    }

    // The marker stays `SocketReady` for up to RELOAD_MARKER_MAX_AGE after a
    // reload completes. Any client launched in that window would otherwise
    // flash "Waiting for reload handoff..." (an extra notice row that shoved
    // the layout) even though the replacement server is already serving.
    if !state.server_reload_in_progress
        && crate::server::recent_reload_state(RELOAD_MARKER_MAX_AGE)
            .is_some_and(|marker| marker.phase == crate::server::ReloadPhase::SocketReady)
    {
        return Ok(None);
    }

    state.disconnect_start.get_or_insert_with(Instant::now);
    app.set_remote_startup_phase(super::super::RemoteStartupPhase::WaitingForReload);
    app.set_status_notice("Waiting for reload handoff...");
    let detail = state
        .last_disconnect_reason
        .as_deref()
        .unwrap_or("server reload in progress");
    set_disconnect_status_message(app, state, reload_wait_status_message(app, state, detail));
    terminal.draw(|frame| crate::tui::ui::draw(frame, app))?;

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
                "Reconnect reload handoff: ready before next connect attempt (state={})",
                crate::server::reload_state_summary(RELOAD_MARKER_MAX_AGE)
            ));
            Ok(None)
        }
        crate::server::ReloadWaitStatus::Failed(detail) => {
            crate::logging::warn(&format!(
                "Reconnect reload handoff pre-check: failed detail={:?} state={}",
                detail,
                crate::server::reload_state_summary(RELOAD_MARKER_MAX_AGE)
            ));
            let detail = detail.unwrap_or_else(|| {
                "reload failed before the replacement server became ready; starting replacement server"
                    .to_string()
            });
            if recover_reloading_server(app, terminal, state, &detail).await? {
                Ok(Some(ConnectOutcome::Retry))
            } else {
                Ok(None)
            }
        }
        crate::server::ReloadWaitStatus::Idle => {
            crate::logging::warn(&format!(
                "Reconnect reload handoff pre-check: idle without ready server state={}",
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
                Ok(Some(ConnectOutcome::Retry))
            } else {
                Ok(None)
            }
        }
        crate::server::ReloadWaitStatus::Waiting { pid } => {
            state.last_reload_pid = pid;
            crate::logging::info(&format!(
                "Reconnect wait: pausing reconnect attempts for reload lifecycle event (pid={:?}, state={})",
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
                        if handle_terminal_event_while_disconnected(app, terminal, event)? {
                            return Ok(Some(ConnectOutcome::Quit));
                        }
                    }
                }
            }
            Ok(Some(ConnectOutcome::Retry))
        }
    }
}

async fn recover_reloading_server(
    app: &mut App,
    terminal: &mut DefaultTerminal,
    state: &mut RemoteRunState,
    detail: &str,
) -> Result<bool> {
    if crate::tui::is_ssh_remote()
        || state.reload_recovery_attempted
        || crate::server_spawn::is_running().await
    {
        return Ok(false);
    }

    state.reload_recovery_attempted = true;
    state.last_disconnect_reason = Some(detail.to_string());

    let content = reconnect_status_message(app, state, detail);
    if let Some(idx) = state.disconnect_msg_idx {
        let _ = app.replace_display_message_content(idx, content);
    } else {
        app.push_display_message(DisplayMessage::system(content));
        state.disconnect_msg_idx = Some(app.display_messages.len() - 1);
    }
    terminal.draw(|frame| crate::tui::ui::draw(frame, app))?;

    crate::logging::warn(&format!(
        "Reload reconnect failed definitively ({}); spawning a replacement shared server",
        detail
    ));

    match crate::server_spawn::spawn_default_server().await {
        Ok(()) => {
            state.initial_server_start = true;
            state.last_disconnect_reason =
                Some("replacement server started; reconnecting".to_string());
            crate::logging::info("Replacement shared server started after stalled reload");
            Ok(true)
        }
        Err(error) => {
            state.last_disconnect_reason = Some(format!(
                "reload recovery failed while starting server: {}",
                error
            ));
            crate::logging::error(&format!(
                "Failed to start replacement server after reload failure: {}",
                error
            ));
            Ok(false)
        }
    }
}

mod connect;
mod local_gate_pause;
mod post_connect;
mod uncertain;
pub(super) use uncertain::retain_uncertain_ssh_request;
pub(super) use uncertain::record_uncertain_ssh_request;
pub(super) use uncertain::record_uncertain_ssh_request_for_session;
pub(super) use uncertain::handle_recovery_key;
pub(in crate::tui::app) use uncertain::has_unresolved_for_session;
pub(in crate::tui::app) use local_gate_pause::pause as pause_after_protocol_error;
pub(in crate::tui::app) use connect::connect_with_retry;
pub(in crate::tui::app) use post_connect::{finalize_reload_reconnect, handle_post_connect};
