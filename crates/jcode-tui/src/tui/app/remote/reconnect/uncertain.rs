use super::super::super::UncertainSshRequest;
use super::*;

pub(in crate::tui::app::remote) fn retain_uncertain_ssh_request(
    app: &mut App,
    _state: &mut RemoteRunState,
) -> bool {
    if !crate::tui::is_ssh_remote() {
        return false;
    }
    let retained_interleave = super::super::queue_recovery::retain_pending_ssh_soft_interrupts(app);
    let Some(payload) = app.rate_limit_pending_message.take() else {
        return retained_interleave;
    };
    let queued_user_text = app.pending_queued_user_text.take();
    record_uncertain_ssh_request(app, payload, queued_user_text);
    true
}

pub(in crate::tui::app::remote) fn record_uncertain_ssh_request(
    app: &mut App,
    payload: super::super::super::PendingRemoteMessage,
    queued_user_text: Option<String>,
) {
    let session = app
        .remote_session_id
        .clone()
        .or_else(|| app.resume_session_id.clone());
    record_uncertain_ssh_request_for_session(app, payload, queued_user_text, session);
}

pub(in crate::tui::app::remote) fn record_uncertain_ssh_request_for_session(
    app: &mut App,
    payload: super::super::super::PendingRemoteMessage,
    queued_user_text: Option<String>,
    session: Option<String>,
) {
    app.rate_limit_reset = None;
    if !app.uncertain_ssh_sessions.contains(&session) {
        app.uncertain_ssh_sessions.push(session.clone());
    }
    app.uncertain_ssh_requests.push(UncertainSshRequest {
        acknowledged: false,
        session_id: session,
        payload,
        queued_user_text,
    });
    app.push_display_message(DisplayMessage::error(
        "The interrupted SSH request may have run. Its exact pending payload is retained in memory and will not be resent automatically. Reconcile the remote effect before retrying. Ctrl+R restores an ordinary request and images to an empty draft; clear the draft and press again to cycle retained requests; Ctrl+Shift+R restores unknown-session requests; hidden/system requests cannot be restored this way. Ctrl+E accepts the unknown outcome for this session; Ctrl+Shift+E accepts unknown-session outcomes globally. Exiting clears this in-memory record.".to_string(),
    ));
}

#[cfg(test)]
pub(super) fn restore_latest_to_draft(
    app: &mut App,
    state: &RemoteRunState,
) -> Result<(), &'static str> {
    restore_to_draft(app, state, false)
}

fn restore_to_draft(
    app: &mut App,
    _state: &RemoteRunState,
    unknown_session: bool,
) -> Result<(), &'static str> {
    if !app.input.is_empty() || !app.pending_images.is_empty() {
        return Err("Clear or save the current draft and attachments first");
    }
    let current_session = if unknown_session {
        None
    } else {
        Some(
            app.remote_session_id
                .as_ref()
                .or(app.resume_session_id.as_ref())
                .ok_or("The current session has no local session reference")?
                .clone(),
        )
    };
    let candidates: Vec<usize> = app
        .uncertain_ssh_requests
        .iter()
        .enumerate()
        .filter(|(_, request)| {
            request.session_id == current_session
                && (request.queued_user_text.is_some()
                    || (!request.payload.is_system && request.payload.system_reminder.is_none()))
        })
        .map(|(index, _)| index)
        .collect();
    let previous = app
        .ssh_recovery_cursor
        .as_ref()
        .filter(|(session, _)| session == &current_session)
        .map(|(_, index)| *index);
    let index = previous
        .and_then(|previous| candidates.iter().rev().find(|index| **index < previous))
        .or_else(|| candidates.last())
        .copied()
        .ok_or("No ordinary uncertain request is retained for this session")?;
    let request = &app.uncertain_ssh_requests[index];
    app.input = request
        .queued_user_text
        .clone()
        .unwrap_or_else(|| request.payload.content.clone());
    app.cursor_pos = app.input.len();
    app.pending_images = request.payload.images.clone();
    app.ssh_recovery_cursor = Some((current_session, index));
    Ok(())
}

pub(in crate::tui::app) fn has_unresolved_for_session(app: &App, _state: &RemoteRunState) -> bool {
    let session = app
        .remote_session_id
        .as_ref()
        .or(app.resume_session_id.as_ref());
    app.ssh_hidden_inflight.is_some() || app.uncertain_ssh_requests.iter().any(|request| {
        !request.acknowledged
            && (request.session_id.as_ref() == session || request.session_id.is_none())
    })
}

pub(in crate::tui::app::remote) fn handle_recovery_key(
    app: &mut App,
    state: &mut RemoteRunState,
    key: &crossterm::event::KeyEvent,
) -> bool {
    use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
    if matches!(key.code, KeyCode::Char('e' | 'E'))
        && key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        && has_unresolved_for_session(app, state)
    {
        let global = key.modifiers.contains(KeyModifiers::SHIFT);
        let session = app
            .remote_session_id
            .as_ref()
            .or(app.resume_session_id.as_ref());
        if !global && session.is_none() {
            app.set_status_notice(
                "No session reference; Ctrl+Shift+E accepts unknown-session outcomes globally",
            );
            return true;
        }
        for request in &mut app.uncertain_ssh_requests {
            if (global && request.session_id.is_none())
                || (!global && session.is_some() && request.session_id.as_ref() == session)
            {
                request.acknowledged = true;
            }
        }
        let session = app
            .remote_session_id
            .clone()
            .or_else(|| app.resume_session_id.clone());
        app.uncertain_ssh_sessions.retain(|held| {
            if global {
                held.is_some()
            } else {
                held.is_none() || *held != session
            }
        });
        app.set_status_notice(if global {
            "Unknown-session outcomes accepted globally; review before sending"
        } else {
            "Current-session unknown outcomes accepted; review before sending"
        });
        return true;
    }
    if app.uncertain_ssh_requests.is_empty()
        || !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        || !matches!(key.code, KeyCode::Char('r' | 'R'))
        || !key.modifiers.contains(KeyModifiers::CONTROL)
    {
        return false;
    }
    let message = match restore_to_draft(app, state, key.modifiers.contains(KeyModifiers::SHIFT)) {
        Ok(()) if key.modifiers.contains(KeyModifiers::SHIFT) => {
            "Unknown-session payload restored to draft; review and reconcile before any send"
        }
        Ok(()) => "Restored to draft; review and reconcile before any send",
        Err(message) => message,
    };
    app.set_status_notice(message);
    true
}

#[cfg(test)]
mod queued_tests;
#[cfg(test)]
mod scope_tests;
#[cfg(test)]
mod tests;
