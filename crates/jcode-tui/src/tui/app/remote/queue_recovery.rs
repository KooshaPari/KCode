use super::*;

impl App {
    pub(super) fn track_pending_soft_interrupt(
        &mut self,
        request_id: u64,
        content: String,
        images: Vec<(String, String)>,
    ) {
        if crate::tui::is_ssh_remote() {
            self.pending_ssh_soft_interrupt_payloads
                .push((content.clone(), images));
        }
        let content_bytes = content.len();
        let content_chars = content.chars().count();
        self.pending_soft_interrupt_requests
            .push((request_id, content.clone()));
        self.pending_soft_interrupts.push(content);
        crate::logging::info(&format!(
            "REMOTE_SOFT_INTERRUPT_TRACK_PENDING id={} content_bytes={} content_chars={} pending_requests={} pending_messages={}",
            request_id,
            content_bytes,
            content_chars,
            self.pending_soft_interrupt_requests.len(),
            self.pending_soft_interrupts.len()
        ));
    }

    pub(super) fn acknowledge_pending_soft_interrupt(&mut self, request_id: u64) -> bool {
        if let Some(index) = self
            .pending_soft_interrupt_requests
            .iter()
            .position(|(id, _)| *id == request_id)
        {
            self.pending_soft_interrupt_requests.remove(index);
            crate::logging::info(&format!(
                "REMOTE_SOFT_INTERRUPT_ACK_MATCHED id={} pending_requests={} pending_messages={}",
                request_id,
                self.pending_soft_interrupt_requests.len(),
                self.pending_soft_interrupts.len()
            ));
            true
        } else {
            if !self.pending_soft_interrupt_requests.is_empty() {
                crate::logging::info(&format!(
                    "REMOTE_SOFT_INTERRUPT_ACK_UNMATCHED id={} pending_requests={} pending_messages={}",
                    request_id,
                    self.pending_soft_interrupt_requests.len(),
                    self.pending_soft_interrupts.len()
                ));
            }
            false
        }
    }

    pub(super) fn clear_pending_soft_interrupt_tracking(&mut self) {
        crate::logging::info(&format!(
            "REMOTE_SOFT_INTERRUPT_TRACKING_CLEAR pending_requests={} pending_messages={}",
            self.pending_soft_interrupt_requests.len(),
            self.pending_soft_interrupts.len()
        ));
        self.pending_soft_interrupts.clear();
        self.pending_soft_interrupt_requests.clear();
        self.pending_ssh_soft_interrupt_payloads.clear();
    }

    pub(super) fn mark_soft_interrupt_injected(&mut self, content: &str) {
        crate::logging::info(&format!(
            "REMOTE_SOFT_INTERRUPT_MARK_INJECTED content_bytes={} content_chars={} pending_requests={} pending_messages={}",
            content.len(),
            content.chars().count(),
            self.pending_soft_interrupt_requests.len(),
            self.pending_soft_interrupts.len()
        ));
        if self.mark_combined_soft_interrupt_injected(content) {
            return;
        }

        if let Some(index) = self
            .pending_soft_interrupts
            .iter()
            .position(|pending| pending == content)
        {
            self.pending_soft_interrupts.remove(index);
            if index < self.pending_ssh_soft_interrupt_payloads.len() {
                self.pending_ssh_soft_interrupt_payloads.remove(index);
            }
        }

        if let Some(index) = self
            .pending_soft_interrupt_requests
            .iter()
            .position(|(_, pending)| pending == content)
        {
            self.pending_soft_interrupt_requests.remove(index);
        }
    }

    fn mark_combined_soft_interrupt_injected(&mut self, content: &str) -> bool {
        let mut combined = String::new();
        for (index, pending) in self.pending_soft_interrupts.iter().enumerate() {
            if index > 0 {
                combined.push_str("\n\n");
            }
            combined.push_str(pending);

            if combined == content {
                let count = index + 1;
                let removed: Vec<String> = self.pending_soft_interrupts.drain(..count).collect();
                let payload_count = count.min(self.pending_ssh_soft_interrupt_payloads.len());
                self.pending_ssh_soft_interrupt_payloads
                    .drain(..payload_count);
                for removed_content in removed {
                    if let Some(request_index) = self
                        .pending_soft_interrupt_requests
                        .iter()
                        .position(|(_, pending)| pending == &removed_content)
                    {
                        self.pending_soft_interrupt_requests.remove(request_index);
                    }
                }
                return true;
            }

            if !content.starts_with(&combined) {
                break;
            }
        }

        false
    }
}

/// Recover an in-flight queued continuation back into the queue.
///
/// A queued follow-up that was already taken from `queued_messages` and handed
/// to `begin_remote_send` lives only in `rate_limit_pending_message` while it
/// is in flight. That pending shape (`is_system` with `auto_retry == false`)
/// has no retry path: the tick resend requires a rate-limit reset timestamp
/// and the disconnect resend requires `auto_retry`. If the connection dies
/// before the turn completes (typically a server reload handoff racing the
/// dispatch), clearing the pending message silently drops the user's queued
/// message (issue #391). Instead, put it back at the front of the queue so it
/// is re-sent once the turn is proven idle after reconnect, which is the
/// queue's contract.
pub(super) fn recover_undelivered_queued_continuation(app: &mut App, reason: &str) -> bool {
    recover_queued_continuation(app, reason, false)
}

/// A busy rejection proves the send was not accepted, including automatic
/// reminders. Unlike a disconnect, it is safe to queue these without risking
/// replaying a continuation that the server is already running.
pub(super) fn recover_rejected_queued_continuation(app: &mut App) -> bool {
    recover_queued_continuation(app, "server busy rejection", true)
}

fn recover_queued_continuation(app: &mut App, reason: &str, rejected: bool) -> bool {
    let is_recoverable = app
        .rate_limit_pending_message
        .as_ref()
        .is_some_and(|pending| {
            pending.is_system
                && (rejected || !pending.auto_retry)
                && (!pending.content.trim().is_empty() || pending.system_reminder.is_some())
        });
    if !is_recoverable {
        return false;
    }
    let Some(pending) = app.rate_limit_pending_message.take() else {
        return false;
    };
    app.rate_limit_reset = None;
    crate::logging::info(&format!(
        "Recovering in-flight queued continuation into queued follow-ups after {} (content_chars={}, has_reminder={})",
        reason,
        pending.content.chars().count(),
        pending.system_reminder.is_some()
    ));
    if let Some(reminder) = pending.system_reminder {
        app.hidden_queued_system_messages.insert(0, reminder);
    }
    if !pending.content.trim().is_empty() {
        app.queued_messages.insert(0, pending.content);
    }
    true
}

pub(super) fn recover_local_interleave_to_queue(app: &mut App, reason: &str) -> bool {
    if crate::tui::is_ssh_remote() {
        return false;
    }
    let Some(interleave) = app.interleave_message.take() else {
        return false;
    };
    if interleave.trim().is_empty() {
        return false;
    }

    crate::logging::info(&format!(
        "Recovering unsent interleave into queued follow-ups after {}",
        reason
    ));
    app.queued_messages.insert(0, interleave);
    true
}

pub(super) fn retain_staged_ssh_interleave(app: &mut App) -> bool {
    if !crate::tui::is_ssh_remote() {
        return false;
    }
    let Some(content) = app.interleave_message.take() else {
        return false;
    };
    let images = std::mem::take(&mut app.interleave_images);
    let session = app.staged_ssh_interleave_session.take().unwrap_or_else(|| {
        app.remote_session_id
            .clone()
            .or_else(|| app.resume_session_id.clone())
    });
    if content.trim().is_empty() && images.is_empty() {
        return false;
    }
    super::reconnect::record_uncertain_ssh_request_for_session(
        app,
        super::super::PendingRemoteMessage {
            content,
            images,
            is_system: false,
            system_reminder: None,
            auto_retry: false,
            retry_attempts: 0,
            retry_at: None,
        },
        None,
        session,
    );
    true
}

pub(super) fn retain_pending_ssh_on_session_switch(app: &mut App) {
    if !crate::tui::is_ssh_remote() {
        return;
    }
    app.ssh_hidden_selection = None;
    {
        let session = app.remote_session_id.clone().or_else(|| app.resume_session_id.clone());
        let mut messages = std::mem::take(&mut app.queued_messages);
        let hidden = std::mem::take(&mut app.hidden_queued_system_messages);
        let pending = std::mem::take(&mut app.pending_queued_dispatch);
        if session.is_none() {
            for content in messages.drain(..) {
                super::reconnect::record_uncertain_ssh_request_for_session(
                    app,
                    super::super::PendingRemoteMessage {
                        content,
                        images: Vec::new(),
                        is_system: false,
                        system_reminder: None,
                        auto_retry: false,
                        retry_attempts: 0,
                        retry_at: None,
                    },
                    None,
                    None,
                );
            }
        }
        if !messages.is_empty() || !hidden.is_empty() || pending {
            if let Some((_, old_messages, old_hidden, old_pending)) = app
                .parked_ssh_queues
                .iter_mut()
                .find(|(id, ..)| *id == session)
            {
                old_messages.extend(messages);
                old_hidden.extend(hidden);
                *old_pending |= pending;
            } else {
                app.parked_ssh_queues.push((session, messages, hidden, pending));
            }
        }
    }
    if app.remote_session_id.is_none()
        && app.resume_session_id.is_none()
        && app.pending_startup_prompt_echo.is_some()
        && app.current_message_id.is_some()
    {
        return;
    }
    retain_pending_ssh_soft_interrupts(app);
    retain_staged_ssh_interleave(app);
    if let Some(payload) = app.rate_limit_pending_message.take() {
        let queued_user_text = app.pending_queued_user_text.take();
        super::reconnect::record_uncertain_ssh_request(app, payload, queued_user_text);
    }
}

pub(super) fn restore_parked_ssh_queue(app: &mut App, session: &str) {
    if !crate::tui::is_ssh_remote() {
        return;
    }
    if let Some(index) = app.parked_ssh_queues.iter().position(|(id, ..)| id.as_deref() == Some(session)) {
        let (_, mut messages, mut hidden, pending) = app.parked_ssh_queues.remove(index);
        messages.append(&mut app.queued_messages);
        hidden.append(&mut app.hidden_queued_system_messages);
        app.queued_messages = messages;
        app.hidden_queued_system_messages = hidden;
        app.pending_queued_dispatch |= pending;
    }
}

pub(super) fn retain_pending_ssh_soft_interrupts(app: &mut App) -> bool {
    if !crate::tui::is_ssh_remote() || app.pending_ssh_soft_interrupt_payloads.is_empty() {
        return false;
    }
    let payloads = std::mem::take(&mut app.pending_ssh_soft_interrupt_payloads);
    app.pending_transfer_request = false;
    app.pending_split_label = None;
    app.pending_soft_interrupts.clear();
    app.pending_soft_interrupt_requests.clear();
    for (content, images) in payloads {
        input_dispatch::retain_failed_ssh_interleave(app, content, images);
    }
    true
}

pub(super) async fn recover_stranded_soft_interrupts(
    app: &mut App,
    remote: &mut RemoteConnection,
) -> bool {
    if !app.is_processing && retain_pending_ssh_soft_interrupts(app) {
        return true;
    }

    if app.is_processing || app.pending_soft_interrupts.is_empty() {
        return false;
    }

    let recovered_interrupts = std::mem::take(&mut app.pending_soft_interrupts);
    if recovered_interrupts.is_empty() {
        return false;
    }

    if let Err(err) = remote.cancel_soft_interrupts().await {
        app.pending_soft_interrupts = recovered_interrupts;
        app.push_display_message(DisplayMessage::error(format!(
            "Failed to recover queued interleave message: {}",
            err
        )));
        app.set_status_notice("Queued interleave recovery failed");
        return false;
    }

    crate::logging::info(&format!(
        "Recovering {} stranded soft interrupt(s) into queued follow-ups after turn boundary",
        recovered_interrupts.len()
    ));
    app.pending_soft_interrupt_requests.clear();

    let mut recovered_queue = recovered_interrupts;
    recovered_queue.append(&mut app.queued_messages);
    app.queued_messages = recovered_queue;
    app.set_status_notice("Recovered queued interleave after turn finished");
    true
}


pub(super) fn mark_hidden_unknown(app: &mut App) {
    if let (Some(generation), Some((Some(id), _, _))) = (app.ssh_hidden_generation, app.ssh_hidden_inflight.as_ref()) {
        let key = (generation, *id);
        if !app.ssh_hidden_tombstones.contains(&key) { app.ssh_hidden_tombstones.push(key); }
    }
    if let Some((id, _, _)) = &mut app.ssh_hidden_inflight { *id = None; }
    app.ssh_hidden_ack_deadline = None;
}

pub(super) fn hidden_late_event(app: &App, generation: u64, event: &crate::protocol::ServerEvent) -> bool {
    match event {
        crate::protocol::ServerEvent::Ack { id } | crate::protocol::ServerEvent::Done { id } | crate::protocol::ServerEvent::Error { id, .. } => app.ssh_hidden_tombstones.contains(&(generation, *id)),
        _ => false,
    }
}

pub(super) fn expire_hidden_ack(app: &mut App, now: Instant) -> bool {
    if !app.ssh_hidden_ack_deadline.is_some_and(|deadline| now >= deadline) { return false; }
    app.ssh_hidden_ack_deadline = None;
    let owns_current = app.ssh_hidden_inflight.as_ref().is_some_and(|(request_id, session, _)| request_id.is_some() && *request_id == app.current_message_id && app.remote_session_id.as_deref() == Some(session.as_str()));
    mark_hidden_unknown(app);
    if owns_current { app.current_message_id = None; app.is_processing = false; }
    app.set_status_notice("Hidden request timed out; outcome unknown and retained without retry");
    true
}

pub(super) fn complete_hidden_recovery(app: &mut App) -> bool {
    let Some((_, _, hidden)) = app.ssh_hidden_inflight.as_ref() else { return false; };
    let Some((_, _, parked, _)) = app.parked_ssh_queues.iter_mut().find(|(session, _, parked, _)| session.is_none() && parked.starts_with(hidden.as_slice())) else { return false; };
    parked.drain(..hidden.len());
    mark_hidden_unknown(app);
    app.ssh_hidden_inflight = None;
    app.ssh_hidden_ack_deadline = None;
    true
}

pub(super) async fn handle_hidden_recovery_command(app: &mut App, remote: &mut RemoteConnection) -> bool {
    if !crate::tui::is_ssh_remote() { return false; }
    let command = app.input.trim().to_string();
    if command != "/ssh-hidden-select" && command != "/ssh-hidden-cancel" && !command.starts_with("/ssh-hidden-send") && !command.starts_with("/ssh-hidden-accept") && !command.starts_with("/ssh-hidden-abandon") { return false; }
    if command.starts_with("/ssh-hidden-abandon") {
        let Some((request_id, session, _)) = app.ssh_hidden_inflight.as_mut() else {
            app.set_status_notice("No hidden request is pending");
            return true;
        };
        if command != format!("/ssh-hidden-abandon {session}") || request_id.is_none() {
            app.set_status_notice("Pending session does not match; hidden work remains held");
            return true;
        }
        let owns_current = *request_id == app.current_message_id && app.remote_session_id.as_deref() == Some(session.as_str());
        mark_hidden_unknown(app);
        if owns_current { app.current_message_id = None; app.is_processing = false; }
        app.input.clear(); app.cursor_pos = 0;
        app.set_status_notice("Stopped waiting; outcome unknown, hidden work retained without retry");
        return true;
    }
    if command.starts_with("/ssh-hidden-accept") {
        app.set_status_notice("Cannot release hidden outcome until the server proves turn isolation; work remains quarantined");
        return true;
    }
    if command == "/ssh-hidden-cancel" {
        app.ssh_hidden_selection = None;
        app.input.clear(); app.cursor_pos = 0;
        app.set_status_notice("Selection canceled; hidden work remains quarantined");
        return true;
    }
    if command.starts_with("/ssh-hidden-send") {
        app.set_status_notice("Hidden recovery send is disabled until the server proves turn isolation; work remains quarantined");
        return true;
    }
    if app.ssh_hidden_inflight.is_some() {
        app.set_status_notice("Hidden work outcome pending or unknown; retained without retry");
        return true;
    }
    let Some(session) = app.remote_session_id.clone() else { app.set_status_notice("Attach to the intended session first"); return true; };
    if remote.session_id() != Some(session.as_str()) { app.ssh_hidden_selection = None; app.set_status_notice("Session is changing; hidden work remains quarantined"); return true; }
    let Some(index) = app.parked_ssh_queues.iter().position(|(id, _, hidden, _)| id.is_none() && !hidden.is_empty()) else { app.set_status_notice("No unknown-session hidden work"); return true; };
    if command == "/ssh-hidden-select" {
        let hidden = app.parked_ssh_queues[index].2.clone();
        app.ssh_hidden_selection = Some((session.clone(), hidden.clone()));
        app.input.clear(); app.cursor_pos = 0;
        app.push_display_message(DisplayMessage::system(format!("Selected session {session} for {} hidden item(s). Contents cannot be inspected and may contain sensitive or automated data. Recovery send is disabled until turn isolation is proven. Retained in memory only.", hidden.len())));
        return true;
    }
    if command != format!("/ssh-hidden-send {session}") || !app.ssh_hidden_selection.as_ref().is_some_and(|(selected, hidden)| selected == &session && hidden == &app.parked_ssh_queues[index].2) {
        app.ssh_hidden_selection = None;
        app.set_status_notice("Session or hidden work changed; select again");
        return true;
    }
    if app.is_processing || app.rate_limit_pending_message.is_some() || super::input_dispatch::ssh_outcome_unresolved(app) { app.set_status_notice("Wait for idle and reconcile pending outcomes"); return true; }
    let hidden = app.parked_ssh_queues[index].2.clone();
    app.ssh_hidden_selection = None;
    app.ssh_hidden_inflight = Some((None, session.clone(), hidden.clone()));
    app.ssh_hidden_generation = Some(remote.generation());
    app.input.clear(); app.cursor_pos = 0;
    let (id, result) = remote.send_session_bound_hidden(session.clone(), hidden.join("\n\n")).await;
    if result.is_ok() {
        if let Some((request_id, _, _)) = &mut app.ssh_hidden_inflight { *request_id = Some(id); }
        app.ssh_hidden_ack_deadline = Some(Instant::now() + Duration::from_secs(30));
        app.current_message_id = Some(id);
        app.is_processing = true;
        app.status = ProcessingStatus::Sending;
        app.push_display_message(DisplayMessage::system(format!("Hidden work submitted to {session}; awaiting server response. To stop waiting and mark its outcome unknown, enter /ssh-hidden-abandon {session}. This does not cancel the remote request.")));
    } else {
        app.ssh_hidden_tombstones.push((remote.generation(), id));
        app.set_status_notice("Hidden outcome unknown; reconcile, then /ssh-hidden-accept SESSION to retain without retry");
    }
    true
}


#[cfg(test)]
mod hidden_command_tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    #[test]
    fn hidden_unknown_cannot_be_sent_or_released_without_origin_isolation() {
        let _lock = crate::storage::lock_test_env();
        let home = tempfile::tempdir().unwrap();
        let old_home = std::env::var_os("KCODE_HOME");
        let old_ssh = std::env::var_os("KCODE_SSH_REMOTE");
        crate::env::set_var("KCODE_HOME", home.path());
        crate::env::set_var("KCODE_SSH_REMOTE", "fixture");
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let mut app = App::new_for_remote(None);
            app.remote_session_id = Some("A".into());
            app.parked_ssh_queues.push((None, vec![], vec!["private reminder".into()], false));
            let mut remote = RemoteConnection::dummy();
            remote.set_session_id("A".into());
            let peer = remote.take_dummy_peer().unwrap();
            let (reader, _) = peer.into_split();
            let mut reader = BufReader::new(reader);
            app.input = "/ssh-hidden-send A".into();
            assert!(handle_hidden_recovery_command(&mut app, &mut remote).await);
            let mut line = String::new();
            assert!(tokio::time::timeout(Duration::from_millis(25), reader.read_line(&mut line)).await.is_err());
            assert!(line.is_empty());
            app.ssh_hidden_inflight = Some((None, "A".into(), vec!["private reminder".into()]));
            app.input = "/ssh-hidden-accept A".into();
            assert!(handle_hidden_recovery_command(&mut app, &mut remote).await);
            assert!(app.ssh_hidden_inflight.is_some());
            assert!(super::super::input_dispatch::ssh_outcome_unresolved(&app));
            assert_eq!(app.parked_ssh_queues[0].2, ["private reminder"]);
        });
        match old_home { Some(value) => crate::env::set_var("KCODE_HOME", value), None => crate::env::remove_var("KCODE_HOME") }
        match old_ssh { Some(value) => crate::env::set_var("KCODE_SSH_REMOTE", value), None => crate::env::remove_var("KCODE_SSH_REMOTE") }
    }

    #[test]
    #[ignore = "requires authoritative stream origin isolation before hidden recovery send is enabled"]
    fn command_sends_bound_variant_only_after_selection_and_keeps_quarantine() {
        let _lock = crate::storage::lock_test_env();
        let home = tempfile::tempdir().unwrap();
        let old_home = std::env::var_os("KCODE_HOME");
        let old_ssh = std::env::var_os("KCODE_SSH_REMOTE");
        crate::env::set_var("KCODE_HOME", home.path());
        crate::env::set_var("KCODE_SSH_REMOTE", "fixture");
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let mut app = App::new_for_remote(None);
            app.remote_session_id = Some("A".into());
            app.parked_ssh_queues.push((None, vec![], vec!["private reminder".into()], false));
            let mut remote = RemoteConnection::dummy();
            remote.set_session_id("A".into());
            let peer = remote.take_dummy_peer().unwrap();
            let (reader, mut writer) = peer.into_split();
            let mut reader = BufReader::new(reader);
            app.input = "/ssh-hidden-send A".into();
            super::super::key_handling::handle_remote_key(&mut app, KeyCode::Enter, KeyModifiers::NONE, &mut remote).await.unwrap();
            assert!(app.ssh_hidden_inflight.is_none());
            assert_eq!(app.parked_ssh_queues[0].2, ["private reminder"]);
            app.input = "/ssh-hidden-select".into();
            super::super::key_handling::handle_remote_key(&mut app, KeyCode::Enter, KeyModifiers::NONE, &mut remote).await.unwrap();
            assert!(app.display_messages().iter().all(|m| !m.content.contains("private reminder")));
            app.input = "/ssh-hidden-send A".into();
            super::super::key_handling::handle_remote_key(&mut app, KeyCode::Enter, KeyModifiers::NONE, &mut remote).await.unwrap();
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            assert!(matches!(serde_json::from_str::<crate::protocol::Request>(&line).unwrap(), crate::protocol::Request::SessionBoundHiddenMessage { session_id, system_reminder, .. } if session_id == "A" && system_reminder == "private reminder"));
            assert_eq!(app.parked_ssh_queues[0].2, ["private reminder"]);
            assert!(app.ssh_hidden_inflight.is_some());
            writer.write_all(b"{\"type\":\"error\",\"id\":0,\"message\":\"unsupported request\"}\n").await.unwrap();
            let event = remote.next_event().await;
            let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
            let mut state = RemoteRunState::default();
            super::super::handle_remote_event(&mut app, &mut terminal, &mut remote, &mut state, event).await.unwrap();
            assert!(app.ssh_hidden_inflight.as_ref().unwrap().0.is_none());
            assert_eq!(app.parked_ssh_queues[0].2, ["private reminder"]);
            let other_connection = RemoteConnection::dummy();
            let late = crate::protocol::ServerEvent::Done { id: 1 };
            assert!(hidden_late_event(&app, remote.generation(), &late));
            assert!(!hidden_late_event(&app, other_connection.generation(), &late));
            assert_ne!(remote.generation(), other_connection.generation());
            assert!(super::super::input_dispatch::ssh_outcome_unresolved(&app));
            app.ssh_hidden_inflight.as_mut().unwrap().0 = Some(1);
            app.ssh_hidden_ack_deadline = Some(Instant::now() + Duration::from_secs(30));
            let deadline = app.ssh_hidden_ack_deadline.unwrap();
            assert!(!expire_hidden_ack(&mut app, deadline - Duration::from_millis(1)));
            assert!(expire_hidden_ack(&mut app, deadline));
            assert!(app.ssh_hidden_inflight.as_ref().unwrap().0.is_none());
            assert_eq!(app.parked_ssh_queues[0].2, ["private reminder"]);
            writer.write_all(b"{\"type\":\"done\",\"id\":1}\n").await.unwrap();
            let late_done = remote.next_event().await;
            super::super::handle_remote_event(&mut app, &mut terminal, &mut remote, &mut state, late_done).await.unwrap();
            assert!(app.ssh_hidden_inflight.as_ref().unwrap().0.is_none());
            assert_eq!(app.parked_ssh_queues[0].2, ["private reminder"]);
            app.ssh_hidden_inflight.as_mut().unwrap().0 = Some(1);
            app.parked_ssh_queues[0].2.push("new reminder".into());
            assert!(complete_hidden_recovery(&mut app));
            assert_eq!(app.parked_ssh_queues[0].2, ["new reminder"]);
            assert!(!super::super::input_dispatch::ssh_outcome_unresolved(&app));
            app.ssh_hidden_inflight = Some((Some(9), "A".into(), vec!["new reminder".into()]));
            app.input = "/ssh-hidden-accept A".into();
            assert!(handle_hidden_recovery_command(&mut app, &mut remote).await);
            assert!(app.ssh_hidden_inflight.is_some());
            assert!(super::super::input_dispatch::ssh_outcome_unresolved(&app));
            app.input = "/ssh-hidden-abandon A".into();
            assert!(handle_hidden_recovery_command(&mut app, &mut remote).await);
            assert!(app.ssh_hidden_inflight.as_ref().unwrap().0.is_none());
            assert_eq!(app.parked_ssh_queues[0].2, ["new reminder"]);
            app.input = "/ssh-hidden-accept B".into();
            assert!(handle_hidden_recovery_command(&mut app, &mut remote).await);
            assert!(app.ssh_hidden_inflight.is_some());
            app.input = "/ssh-hidden-accept A".into();
            assert!(handle_hidden_recovery_command(&mut app, &mut remote).await);
            assert!(app.ssh_hidden_inflight.is_none());
            assert_eq!(app.parked_ssh_queues[0].2, ["new reminder"]);
            assert!(app.hidden_queued_system_messages.is_empty());
            app.ssh_hidden_inflight = Some((Some(42), "A".into(), vec!["new reminder".into()]));
            app.remote_session_id = Some("B".into());
            app.current_message_id = Some(77);
            app.is_processing = true;
            app.ssh_hidden_ack_deadline = Some(Instant::now());
            assert!(expire_hidden_ack(&mut app, Instant::now()));
            assert_eq!(app.current_message_id, Some(77));
            assert!(app.is_processing);
            app.remote_session_id = Some("A".into());
            app.ssh_hidden_inflight = Some((None, "A".into(), vec!["new reminder".into()]));
            app.input = "/ssh-hidden-accept A".into();
            app.session_picker_overlay = Some(std::cell::RefCell::new(crate::tui::session_picker::SessionPicker::new(vec![])));
            let key = crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
            let event = Some(Ok(crossterm::event::Event::Key(key)));
            let mut state = RemoteRunState::default();
            super::super::apply_terminal_event(&mut app, &mut remote, &mut state, event).await.unwrap();
            assert!(app.ssh_hidden_inflight.is_some(), "modal Enter must not accept hidden outcome");
            assert_eq!(app.input, "/ssh-hidden-accept A");
        });
        match old_home { Some(value) => crate::env::set_var("KCODE_HOME", value), None => crate::env::remove_var("KCODE_HOME") }
        match old_ssh { Some(value) => crate::env::set_var("KCODE_SSH_REMOTE", value), None => crate::env::remove_var("KCODE_SSH_REMOTE") }
    }
}
