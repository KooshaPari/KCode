use super::*;
use std::ffi::OsString;

struct RestoreEnv(Vec<(&'static str, Option<OsString>)>);
impl Drop for RestoreEnv {
    fn drop(&mut self) {
        for (name, value) in self.0.drain(..) {
            match value {
                Some(value) => crate::env::set_var(name, value),
                None => crate::env::remove_var(name),
            }
        }
    }
}

#[test]
fn disconnect_retains_exact_payload_without_retry_and_restores_only_by_choice() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(
        ["KCODE_HOME", "KCODE_SSH_REMOTE"]
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    crate::env::set_var("KCODE_HOME", home.path());
    crate::env::set_var("KCODE_SSH_REMOTE", "synthetic-host");
    let mut app = App::new_for_remote(Some("session_one".into()));
    app.remote_session_id = Some("session_one".into());
    app.queued_messages.push("other queued work".into());
    app.input = "existing draft".into();
    app.rate_limit_pending_message = Some(super::super::super::super::PendingRemoteMessage {
        content: "uncertain text".into(),
        images: vec![("image/png".into(), "synthetic-image".into())],
        is_system: false,
        system_reminder: None,
        auto_retry: true,
        retry_attempts: 2,
        retry_at: None,
    });
    let mut state = RemoteRunState::default();
    super::super::super::handle_disconnect(
        &mut app,
        &mut state,
        Some(RemoteDisconnectReason::PeerClosed),
    );
    assert!(app.rate_limit_pending_message.is_none());
    assert_eq!(app.queued_messages, vec!["other queued work"]);
    assert_eq!(app.input, "existing draft");
    let saved = &app.uncertain_ssh_requests[0];
    assert_eq!(saved.session_id.as_deref(), Some("session_one"));
    assert_eq!(saved.payload.content, "uncertain text");
    assert_eq!(
        saved.payload.images,
        vec![("image/png".into(), "synthetic-image".into())]
    );
    assert!(saved.payload.auto_retry);
    assert_eq!(saved.payload.retry_attempts, 2);
    assert!(restore_latest_to_draft(&mut app, &state).is_err());
    app.input.clear();
    app.remote_session_id = Some("different_session".into());
    assert!(restore_latest_to_draft(&mut app, &state).is_err());
    app.remote_session_id = Some("session_one".into());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut remote = runtime.block_on(async { crate::tui::backend::RemoteConnection::dummy() });
    let mut peer = remote.take_dummy_peer().unwrap();
    runtime.block_on(async {
        use tokio::io::AsyncReadExt;
        let key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('r'),
            crossterm::event::KeyModifiers::CONTROL,
        );
        assert!(
            super::super::super::apply_terminal_event(
                &mut app,
                &mut remote,
                &mut state,
                Some(Ok(crossterm::event::Event::Key(key)))
            )
            .await
            .unwrap()
        );
        let mut byte = [0u8; 1];
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), peer.read(&mut byte))
                .await
                .is_err(),
            "restoring a draft must not dispatch"
        );
    });
    assert_eq!(app.input, "uncertain text");
    assert_eq!(
        app.pending_images,
        vec![("image/png".into(), "synthetic-image".into())]
    );
    assert!(app.rate_limit_pending_message.is_none());
    assert_eq!(app.queued_messages, vec!["other queued work"]);
}
#[test]
fn stalled_ssh_continuation_is_retained_without_redispatch() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(
        ["KCODE_HOME", "KCODE_SSH_REMOTE"]
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    crate::env::set_var("KCODE_HOME", home.path());
    crate::env::set_var("KCODE_SSH_REMOTE", "synthetic-host");
    let mut app = App::new_for_remote(Some("session_one".into()));
    app.remote_session_id = Some("session_one".into());
    app.is_processing = true;
    app.processing_started =
        Some(Instant::now() - super::super::super::stall_timeout() - Duration::from_secs(1));
    app.last_stream_activity = app.processing_started;
    app.queued_messages.push("other queued work".into());
    app.rate_limit_pending_message = Some(super::super::super::super::PendingRemoteMessage {
        content: "queued continuation".into(),
        images: vec![("image/png".into(), "exact-image".into())],
        is_system: true,
        system_reminder: None,
        auto_retry: false,
        retry_attempts: 0,
        retry_at: None,
    });
    let mut state = RemoteRunState::default();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut remote = runtime.block_on(async { crate::tui::backend::RemoteConnection::dummy() });
    let mut peer = remote.take_dummy_peer().unwrap();
    runtime.block_on(async {
        use tokio::io::{AsyncBufReadExt, BufReader};
        super::super::super::detect_and_cancel_stall(&mut app, &mut remote, &mut state).await;
        let mut reader = BufReader::new(&mut peer);
        let mut cancel = String::new();
        tokio::time::timeout(Duration::from_secs(1), reader.read_line(&mut cancel))
            .await
            .unwrap()
            .unwrap();
        assert!(
            cancel.contains("cancel"),
            "expected only a cancel frame: {cancel}"
        );
        let mut extra = String::new();
        assert!(
            tokio::time::timeout(Duration::from_millis(30), reader.read_line(&mut extra))
                .await
                .is_err(),
            "stall must not resend"
        );
    });
    assert!(app.rate_limit_pending_message.is_none());
    assert_eq!(app.queued_messages, vec!["other queued work"]);
    assert_eq!(app.uncertain_ssh_requests.len(), 1);
    assert_eq!(
        app.uncertain_ssh_requests[0].payload.content,
        "queued continuation"
    );
    assert_eq!(
        app.uncertain_ssh_requests[0].payload.images,
        vec![("image/png".into(), "exact-image".into())]
    );
}
#[test]
fn ssh_connectivity_error_event_retains_payload_without_automatic_retry() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(
        ["KCODE_HOME", "KCODE_SSH_REMOTE"]
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    crate::env::set_var("KCODE_HOME", home.path());
    crate::env::set_var("KCODE_SSH_REMOTE", "synthetic-host");
    let mut app = App::new_for_remote(Some("session_one".into()));
    app.remote_session_id = Some("session_one".into());
    app.is_processing = true;
    app.current_message_id = Some(1);
    app.queued_messages.push("later queued work".into());
    app.pending_queued_dispatch = true;
    app.rate_limit_pending_message = Some(super::super::super::super::PendingRemoteMessage {
        content: "effect may have happened".into(),
        images: vec![("image/png".into(), "exact-image".into())],
        is_system: false,
        system_reminder: None,
        auto_retry: true,
        retry_attempts: 0,
        retry_at: None,
    });
    let mut state = RemoteRunState::default();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut remote = runtime.block_on(async { crate::tui::backend::RemoteConnection::dummy() });
    let mut peer = remote.take_dummy_peer().unwrap();
    remote.mark_history_loaded();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    runtime.block_on(async {
        use tokio::io::AsyncReadExt;
        super::super::super::handle_remote_event(
            &mut app,
            &mut terminal,
            &mut remote,
            &mut state,
            crate::tui::backend::RemoteRead::Event(crate::protocol::ServerEvent::Error {
                id: 1,
                message: "provider returned HTTP 500".into(),
                retry_after_secs: None,
            }),
        )
        .await
        .unwrap();
        super::super::super::handle_tick(&mut app, &mut remote, &mut state).await;
        super::super::super::process_remote_followups_guarded(&mut app, &mut remote, &state).await;
        assert_eq!(app.queued_messages, vec!["later queued work"]);
        assert!(app.pending_queued_dispatch);
        assert!(
            super::super::super::begin_remote_send(
                &mut app,
                &mut remote,
                "must not send".into(),
                vec![("image/png".into(), "image".into())],
                false,
                None,
                false,
                0
            )
            .await
            .is_err()
        );
        app.input = "existing draft".into();
        app.pending_images = vec![("image/png".into(), "draft-image".into())];
        let enter = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        );
        super::super::super::apply_terminal_event(
            &mut app,
            &mut remote,
            &mut state,
            Some(Ok(crossterm::event::Event::Key(enter))),
        )
        .await
        .unwrap();
        assert_eq!(app.input, "existing draft");
        let poke = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('p'),
            crossterm::event::KeyModifiers::CONTROL,
        );
        assert!(app.toggle_keys.auto_poke.matches(poke.code, poke.modifiers));
        let was_enabled = app.auto_poke_incomplete_todos;
        super::super::super::apply_terminal_event(
            &mut app,
            &mut remote,
            &mut state,
            Some(Ok(crossterm::event::Event::Key(poke))),
        )
        .await
        .unwrap();
        assert_eq!(app.auto_poke_incomplete_todos, was_enabled);
        assert_eq!(
            app.pending_images,
            vec![("image/png".into(), "draft-image".into())]
        );
        app.dictation_in_flight = true;
        app.dictation_request_id = Some("voice-one".into());
        app.dictation_target_session_id = Some("session_one".into());
        super::super::super::handle_bus_event(
            &mut app,
            &mut remote,
            Ok(crate::bus::BusEvent::DictationCompleted {
                dictation_id: "voice-one".into(),
                session_id: Some("session_one".into()),
                text: " voice".into(),
                mode: crate::protocol::TranscriptMode::Send,
            }),
            &mut state,
        )
        .await;
        assert_eq!(app.input, "existing draft voice");
        super::super::super::handle_bus_event(
            &mut app,
            &mut remote,
            Ok(crate::bus::BusEvent::VoiceInputWake),
            &mut state,
        )
        .await;
        assert_eq!(state.held_voice_wakes, vec![Some("session_one".into())]);
        let mut byte = [0u8; 1];
        assert!(
            tokio::time::timeout(Duration::from_millis(30), peer.read(&mut byte))
                .await
                .is_err()
        );
    });
    assert!(has_unresolved_for_session(&app, &state));
    app.remote_session_id = Some("other_session".into());
    assert!(!has_unresolved_for_session(&app, &state));
    app.remote_session_id = Some("session_one".into());
    let acknowledge = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('e'),
        crossterm::event::KeyModifiers::CONTROL,
    );
    assert!(handle_recovery_key(&mut app, &mut state, &acknowledge));
    assert!(!has_unresolved_for_session(&app, &state));
    assert_eq!(state.held_voice_wakes, vec![Some("session_one".into())]);
    runtime.block_on(async {
        super::super::super::handle_tick(&mut app, &mut remote, &mut state).await;
    });
    assert!(state.held_voice_wakes.is_empty());
    assert_eq!(app.queued_messages, vec!["later queued work"]);
    assert!(app.rate_limit_pending_message.is_none());
    assert!(app.rate_limit_reset.is_none());
    assert_eq!(app.uncertain_ssh_requests.len(), 1);
    assert_eq!(
        app.uncertain_ssh_requests[0].payload.content,
        "effect may have happened"
    );
    assert_eq!(
        app.uncertain_ssh_requests[0].payload.images,
        vec![("image/png".into(), "exact-image".into())]
    );
}
#[test]
fn restoration_selects_latest_ordinary_request_for_current_session() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(vec![("KCODE_HOME", std::env::var_os("KCODE_HOME"))]);
    crate::env::set_var("KCODE_HOME", home.path());
    let mut app = App::new_for_remote(Some("A".into()));
    let payload =
        |content: &str, image: &str, is_system| super::super::super::super::PendingRemoteMessage {
            content: content.into(),
            images: vec![("image/png".into(), image.into())],
            is_system,
            system_reminder: None,
            auto_retry: false,
            retry_attempts: 0,
            retry_at: None,
        };
    let mut state = RemoteRunState::default();
    app.uncertain_ssh_requests.push(UncertainSshRequest {
        acknowledged: false,
        session_id: Some("A".into()),
        payload: payload("A text", "A image", false),
        queued_user_text: None,
    });
    app.uncertain_ssh_requests.push(UncertainSshRequest {
        acknowledged: false,
        session_id: Some("B".into()),
        payload: payload("B text", "B image", false),
        queued_user_text: None,
    });
    app.uncertain_ssh_requests.push(UncertainSshRequest {
        acknowledged: false,
        session_id: Some("A".into()),
        payload: payload("hidden", "hidden image", true),
        queued_user_text: None,
    });
    app.remote_session_id = Some("A".into());
    restore_latest_to_draft(&mut app, &state).unwrap();
    assert_eq!(app.input, "A text");
    assert_eq!(
        app.pending_images,
        vec![("image/png".into(), "A image".into())]
    );
    app.input.clear();
    app.pending_images.clear();
    app.remote_session_id = Some("B".into());
    restore_latest_to_draft(&mut app, &state).unwrap();
    assert_eq!(app.input, "B text");
    assert_eq!(
        app.pending_images,
        vec![("image/png".into(), "B image".into())]
    );
    app.input.clear();
    app.pending_images.clear();
    app.remote_session_id = Some("A".into());
    restore_latest_to_draft(&mut app, &state).unwrap();
    assert_eq!(app.input, "A text");
    assert_eq!(
        app.pending_images,
        vec![("image/png".into(), "A image".into())]
    );
}

#[test]
fn partial_ssh_write_retains_payload_and_blocks_another_send() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(
        ["KCODE_HOME", "KCODE_SSH_REMOTE"]
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    crate::env::set_var("KCODE_HOME", home.path());
    crate::env::set_var("KCODE_SSH_REMOTE", "synthetic-host");
    let mut app = App::new_for_remote(Some("session_one".into()));
    app.remote_session_id = Some("session_one".into());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        use tokio::io::AsyncReadExt;
        let mut remote = crate::tui::backend::RemoteConnection::dummy();
        remote.inject_partial_write_failure();
        let mut peer = remote.take_dummy_peer().unwrap();
        let reader = tokio::spawn(async move {
            let mut prefix = [0u8; 64];
            peer.read_exact(&mut prefix).await.unwrap();
            prefix
        });
        let text = "x".repeat(128);
        let images = vec![("image/png".into(), "exact-image".into())];
        let error = super::super::super::begin_remote_send(
            &mut app,
            &mut remote,
            text.clone(),
            images.clone(),
            false,
            None,
            true,
            0,
        )
        .await
        .unwrap_err();
        assert!(super::super::super::input_dispatch::is_uncertain_ssh_write(
            &error
        ));
        assert_eq!(reader.await.unwrap().len(), 64);
        assert_eq!(app.uncertain_ssh_requests.len(), 1);
        assert_eq!(app.uncertain_ssh_requests[0].payload.content, text);
        assert_eq!(app.uncertain_ssh_requests[0].payload.images, images);
        let mut next = crate::tui::backend::RemoteConnection::dummy();
        let mut next_peer = next.take_dummy_peer().unwrap();
        assert!(
            super::super::super::begin_remote_send(
                &mut app,
                &mut next,
                "must not send".into(),
                vec![],
                false,
                None,
                false,
                0,
            )
            .await
            .is_err()
        );
        let mut byte = [0u8; 1];
        assert!(
            tokio::time::timeout(Duration::from_millis(30), next_peer.read(&mut byte))
                .await
                .is_err()
        );
    });
}

#[test]
fn ssh_protocol_failure_retains_pending_request_and_pauses() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(
        ["KCODE_HOME", "KCODE_SSH_REMOTE"]
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    crate::env::set_var("KCODE_HOME", home.path());
    crate::env::set_var("KCODE_SSH_REMOTE", "synthetic-host");
    let mut app = App::new_for_remote(Some("session_one".into()));
    app.remote_session_id = Some("session_one".into());
    app.rate_limit_pending_message = Some(super::super::super::super::PendingRemoteMessage {
        content: "pending".into(),
        images: vec![("image/png".into(), "image".into())],
        is_system: false,
        system_reminder: None,
        auto_retry: true,
        retry_attempts: 0,
        retry_at: None,
    });
    let mut state = RemoteRunState::default();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut remote = crate::tui::backend::RemoteConnection::dummy();
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        let (outcome, _) = super::super::super::handle_remote_event(
            &mut app,
            &mut terminal,
            &mut remote,
            &mut state,
            crate::tui::backend::RemoteRead::Disconnected(RemoteDisconnectReason::Protocol(
                "bad frame".into(),
            )),
        )
        .await
        .unwrap();
        assert!(matches!(
            outcome,
            super::super::super::RemoteEventOutcome::PauseProtocol
        ));
        assert_eq!(app.uncertain_ssh_requests[0].payload.content, "pending");
        assert_eq!(app.uncertain_ssh_requests[0].payload.images[0].1, "image");
        assert!(has_unresolved_for_session(&app, &state));
    });
}

#[test]
fn ssh_interleave_disconnect_retains_full_payload_without_requeue() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(
        ["KCODE_HOME", "KCODE_SSH_REMOTE"]
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    crate::env::set_var("KCODE_HOME", home.path());
    crate::env::set_var("KCODE_SSH_REMOTE", "synthetic-host");
    let mut app = App::new_for_remote(Some("session_one".into()));
    app.remote_session_id = Some("session_one".into());
    app.track_pending_soft_interrupt(42, "same".into(), vec![("image/png".into(), "one".into())]);
    app.track_pending_soft_interrupt(43, "same".into(), vec![("image/png".into(), "two".into())]);
    app.mark_soft_interrupt_injected("same");
    let mut state = RemoteRunState::default();
    super::super::super::handle_disconnect(
        &mut app,
        &mut state,
        Some(RemoteDisconnectReason::PeerClosed),
    );
    assert!(app.queued_messages.is_empty());
    assert!(app.pending_soft_interrupts.is_empty());
    assert_eq!(app.uncertain_ssh_requests.len(), 1);
    assert_eq!(app.uncertain_ssh_requests[0].payload.content, "same");
    assert_eq!(app.uncertain_ssh_requests[0].payload.images[0].1, "two");
    assert!(has_unresolved_for_session(&app, &state));
    restore_latest_to_draft(&mut app, &state).unwrap();
    assert_eq!(app.input, "same");
    assert_eq!(app.pending_images[0].1, "two");
}

#[test]
fn ssh_partial_interleave_write_keeps_text_and_images() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(
        ["KCODE_HOME", "KCODE_SSH_REMOTE"]
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    crate::env::set_var("KCODE_HOME", home.path());
    crate::env::set_var("KCODE_SSH_REMOTE", "synthetic-host");
    let mut app = App::new_for_remote(Some("session_one".into()));
    app.remote_session_id = Some("session_one".into());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        use tokio::io::AsyncReadExt;
        let mut remote = crate::tui::backend::RemoteConnection::dummy();
        remote.inject_partial_write_failure();
        let mut peer = remote.take_dummy_peer().unwrap();
        let reader = tokio::spawn(async move {
            let mut prefix = [0u8; 64];
            peer.read_exact(&mut prefix).await.unwrap();
        });
        let text = "x".repeat(128);
        let images = vec![("image/png".into(), "image".into())];
        super::super::super::send_interleave_now(
            &mut app,
            text.clone(),
            images.clone(),
            &mut remote,
        )
        .await;
        reader.await.unwrap();
        assert_eq!(app.uncertain_ssh_requests.len(), 1);
        assert_eq!(app.uncertain_ssh_requests[0].payload.content, text);
        assert_eq!(app.uncertain_ssh_requests[0].payload.images, images);
        assert!(app.queued_messages.is_empty());
    });
}

#[test]
fn ssh_written_interleave_without_ack_is_not_replayed() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(
        ["KCODE_HOME", "KCODE_SSH_REMOTE"]
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    crate::env::set_var("KCODE_HOME", home.path());
    crate::env::set_var("KCODE_SSH_REMOTE", "synthetic-host");
    let mut app = App::new_for_remote(Some("session_one".into()));
    app.remote_session_id = Some("session_one".into());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut remote = crate::tui::backend::RemoteConnection::dummy();
        let _peer = remote.take_dummy_peer().unwrap();
        let images = vec![("image/png".into(), "image".into())];
        super::super::super::send_interleave_now(
            &mut app,
            "payload".into(),
            images.clone(),
            &mut remote,
        )
        .await;
        assert_eq!(app.pending_soft_interrupts.len(), 1);
        let mut state = RemoteRunState::default();
        super::super::super::handle_disconnect(
            &mut app,
            &mut state,
            Some(RemoteDisconnectReason::PeerClosed),
        );
        assert!(app.queued_messages.is_empty());
        assert!(has_unresolved_for_session(&app, &state));
        assert_eq!(app.uncertain_ssh_requests[0].payload.content, "payload");
        assert_eq!(app.uncertain_ssh_requests[0].payload.images, images);
    });
}

#[test]
fn ssh_partial_shell_write_retains_original_command() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(
        ["KCODE_HOME", "KCODE_SSH_REMOTE"]
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    crate::env::set_var("KCODE_HOME", home.path());
    crate::env::set_var("KCODE_SSH_REMOTE", "synthetic-host");
    let mut app = App::new_for_remote(Some("session_one".into()));
    app.remote_session_id = Some("session_one".into());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        use tokio::io::AsyncReadExt;
        let mut remote = crate::tui::backend::RemoteConnection::dummy();
        remote.inject_partial_write_failure();
        let mut peer = remote.take_dummy_peer().unwrap();
        let reader = tokio::spawn(async move {
            let mut prefix = [0u8; 64];
            peer.read_exact(&mut prefix).await.unwrap();
        });
        let command = "x".repeat(128);
        let original = format!("!{command}");
        let error = super::super::super::input_dispatch::submit_remote_input_shell(
            &mut app,
            &mut remote,
            original.clone(),
            command,
        )
        .await
        .unwrap_err();
        reader.await.unwrap();
        assert!(super::super::super::input_dispatch::is_uncertain_ssh_write(
            &error
        ));
        assert_eq!(app.uncertain_ssh_requests[0].payload.content, original);
        assert!(app.queued_messages.is_empty());
    });
}

#[test]
fn recovery_cycles_through_all_retained_requests() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(vec![("KCODE_HOME", std::env::var_os("KCODE_HOME"))]);
    crate::env::set_var("KCODE_HOME", home.path());
    let mut app = App::new_for_remote(Some("session_one".into()));
    app.remote_session_id = Some("session_one".into());
    for text in ["first", "second"] {
        record_uncertain_ssh_request(
            &mut app,
            super::super::super::super::PendingRemoteMessage {
                content: text.into(),
                images: Vec::new(),
                is_system: false,
                system_reminder: None,
                auto_retry: false,
                retry_attempts: 0,
                retry_at: None,
            },
            None,
        );
    }
    let state = RemoteRunState::default();
    restore_latest_to_draft(&mut app, &state).unwrap();
    assert_eq!(app.input, "second");
    app.input.clear();
    restore_latest_to_draft(&mut app, &state).unwrap();
    assert_eq!(app.input, "first");
    app.input.clear();
    restore_latest_to_draft(&mut app, &state).unwrap();
    assert_eq!(app.input, "second");
    assert_eq!(app.uncertain_ssh_requests.len(), 2);
}

#[test]
fn ssh_written_shell_without_result_is_not_replayed() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(
        ["KCODE_HOME", "KCODE_SSH_REMOTE"]
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    crate::env::set_var("KCODE_HOME", home.path());
    crate::env::set_var("KCODE_SSH_REMOTE", "synthetic-host");
    let mut app = App::new_for_remote(Some("session_one".into()));
    app.remote_session_id = Some("session_one".into());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut remote = crate::tui::backend::RemoteConnection::dummy();
        let _peer = remote.take_dummy_peer().unwrap();
        super::super::super::input_dispatch::submit_remote_input_shell(
            &mut app,
            &mut remote,
            "!echo hello".into(),
            "echo hello".into(),
        )
        .await
        .unwrap();
        let mut state = RemoteRunState::default();
        super::super::super::handle_disconnect(
            &mut app,
            &mut state,
            Some(RemoteDisconnectReason::PeerClosed),
        );
        assert!(app.queued_messages.is_empty());
        assert!(has_unresolved_for_session(&app, &state));
        assert_eq!(app.uncertain_ssh_requests[0].payload.content, "!echo hello");
    });
}

#[test]
fn queued_origin_remains_bound_when_interleave_is_also_unresolved() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(
        ["KCODE_HOME", "KCODE_SSH_REMOTE"]
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    crate::env::set_var("KCODE_HOME", home.path());
    crate::env::set_var("KCODE_SSH_REMOTE", "synthetic-host");
    let mut app = App::new_for_remote(Some("session_one".into()));
    app.remote_session_id = Some("session_one".into());
    app.pending_queued_user_text = Some("queued user".into());
    app.rate_limit_pending_message = Some(super::super::super::super::PendingRemoteMessage {
        content: "queued user plus hidden".into(),
        images: vec![("image/png".into(), "main image".into())],
        is_system: true,
        system_reminder: Some("hidden".into()),
        auto_retry: false,
        retry_attempts: 0,
        retry_at: None,
    });
    app.track_pending_soft_interrupt(
        44,
        "interleave".into(),
        vec![("image/png".into(), "interrupt image".into())],
    );
    let mut state = RemoteRunState::default();
    super::super::super::handle_disconnect(
        &mut app,
        &mut state,
        Some(RemoteDisconnectReason::PeerClosed),
    );
    assert_eq!(app.uncertain_ssh_requests.len(), 2);
    let interrupt = &app.uncertain_ssh_requests[0];
    assert_eq!(interrupt.session_id.as_deref(), Some("session_one"));
    assert_eq!(interrupt.queued_user_text, None);
    assert_eq!(interrupt.payload.content, "interleave");
    assert_eq!(interrupt.payload.images[0].1, "interrupt image");
    let main = &app.uncertain_ssh_requests[1];
    assert_eq!(main.session_id.as_deref(), Some("session_one"));
    assert_eq!(main.queued_user_text.as_deref(), Some("queued user"));
    assert_eq!(main.payload.images[0].1, "main image");
    restore_latest_to_draft(&mut app, &state).unwrap();
    assert_eq!(app.input, "queued user");
    assert_eq!(app.pending_images[0].1, "main image");
    app.input.clear();
    app.pending_images.clear();
    restore_latest_to_draft(&mut app, &state).unwrap();
    assert_eq!(app.input, "interleave");
    assert_eq!(app.pending_images[0].1, "interrupt image");
    assert!(app.queued_messages.is_empty());
}

#[test]
fn disconnect_preserves_unsent_image_only_interleave_together() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(
        ["KCODE_HOME", "KCODE_SSH_REMOTE"]
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    crate::env::set_var("KCODE_HOME", home.path());
    crate::env::set_var("KCODE_SSH_REMOTE", "synthetic-host");
    let mut app = App::new_for_remote(Some("session_one".into()));
    app.remote_session_id = Some("session_one".into());
    app.interleave_message = Some(String::new());
    app.interleave_images = vec![("image/png".into(), "staged".into())];
    let mut state = RemoteRunState::default();
    super::super::super::handle_disconnect(
        &mut app,
        &mut state,
        Some(RemoteDisconnectReason::PeerClosed),
    );
    assert_eq!(app.interleave_message.as_deref(), Some(""));
    assert_eq!(app.interleave_images[0].1, "staged");
    assert!(app.queued_messages.is_empty());
}

#[test]
fn switching_session_scopes_unsent_interleave_to_original_session() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(
        ["KCODE_HOME", "KCODE_SSH_REMOTE"]
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    crate::env::set_var("KCODE_HOME", home.path());
    crate::env::set_var("KCODE_SSH_REMOTE", "synthetic-host");
    let mut app = App::new_for_remote(Some("old".into()));
    app.remote_session_id = Some("old".into());
    super::super::super::super::input::stage_local_interleave(
        &mut app,
        String::new(),
        vec![("image/png".into(), "staged".into())],
    );
    app.remote_session_id = Some("new".into());
    let state = RemoteRunState::default();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        use tokio::io::AsyncReadExt;
        let mut remote = crate::tui::backend::RemoteConnection::dummy();
        remote.mark_history_loaded();
        let mut peer = remote.take_dummy_peer().unwrap();
        super::super::super::process_remote_followups_guarded(&mut app, &mut remote, &state).await;
        let mut byte = [0u8; 1];
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), peer.read(&mut byte))
                .await
                .is_err()
        );
    });
    assert!(app.interleave_message.is_none());
    assert!(app.interleave_images.is_empty());
    assert!(app.queued_messages.is_empty());
    assert_eq!(
        app.uncertain_ssh_requests[0].session_id.as_deref(),
        Some("old")
    );
    assert_eq!(app.uncertain_ssh_requests[0].payload.images[0].1, "staged");
    assert!(restore_latest_to_draft(&mut app, &state).is_err());
    app.remote_session_id = Some("old".into());
    restore_latest_to_draft(&mut app, &state).unwrap();
    assert_eq!(app.pending_images[0].1, "staged");
}

#[test]
fn unknown_session_payload_can_only_be_restored_explicitly() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(
        ["KCODE_HOME", "KCODE_SSH_REMOTE"]
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    crate::env::set_var("KCODE_HOME", home.path());
    crate::env::set_var("KCODE_SSH_REMOTE", "synthetic-host");
    let mut app = App::new_for_remote(None);
    app.remote_session_id = None;
    app.resume_session_id = None;
    super::super::super::super::input::stage_local_interleave(
        &mut app,
        "unknown text".into(),
        vec![("image/png".into(), "unknown image".into())],
    );
    app.remote_session_id = Some("new".into());
    super::super::super::queue_recovery::retain_staged_ssh_interleave(&mut app);
    let mut state = RemoteRunState::default();
    assert!(restore_latest_to_draft(&mut app, &state).is_err());
    let key = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('R'),
        crossterm::event::KeyModifiers::CONTROL | crossterm::event::KeyModifiers::SHIFT,
    );
    assert!(handle_recovery_key(&mut app, &mut state, &key));
    assert_eq!(app.input, "unknown text");
    assert_eq!(app.pending_images[0].1, "unknown image");
    assert!(has_unresolved_for_session(&app, &state));
    assert!(app.interleave_message.is_none());
    assert!(app.queued_messages.is_empty());
}
