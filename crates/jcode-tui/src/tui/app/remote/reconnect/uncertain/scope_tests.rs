use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn session_ack_does_not_release_unknown_or_other_sessions() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("KCODE_HOME");
    crate::env::set_var("KCODE_HOME", home.path());
    let mut app = App::new_for_remote(Some("A".into()));
    app.remote_session_id = Some("A".into());
    let payload = super::super::super::super::PendingRemoteMessage {
        content: "request".into(), images: vec![], is_system: false,
        system_reminder: None, auto_retry: false, retry_attempts: 0, retry_at: None,
    };
    for session in [Some("A".to_string()), Some("B".to_string()), None] {
        app.uncertain_ssh_sessions.push(session.clone());
        app.uncertain_ssh_requests.push(UncertainSshRequest {
            session_id: session, acknowledged: false, payload: payload.clone(), queued_user_text: None,
        });
    }
    let mut state = RemoteRunState::default();
    let key = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL);
    assert!(handle_recovery_key(&mut app, &mut state, &key));
    assert!(app.uncertain_ssh_requests[0].acknowledged);
    assert!(!app.uncertain_ssh_requests[1].acknowledged);
    assert!(!app.uncertain_ssh_requests[2].acknowledged);
    assert!(has_unresolved_for_session(&app, &state));
    let global = KeyEvent::new(KeyCode::Char('E'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
    assert!(handle_recovery_key(&mut app, &mut state, &global));
    assert!(app.uncertain_ssh_requests[2].acknowledged);
    assert!(!app.uncertain_ssh_requests[1].acknowledged);
    app.remote_session_id = Some("B".into());
    assert!(has_unresolved_for_session(&app, &state));
    match previous { Some(value) => crate::env::set_var("KCODE_HOME", value), None => crate::env::remove_var("KCODE_HOME") }
}

#[test]
fn session_switch_parks_queued_work_until_its_session_returns() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let previous_home = std::env::var_os("KCODE_HOME");
    let previous_ssh = std::env::var_os("KCODE_SSH_REMOTE");
    crate::env::set_var("KCODE_HOME", home.path());
    crate::env::set_var("KCODE_SSH_REMOTE", "synthetic-host");
    let mut app = App::new_for_remote(Some("A".into()));
    app.remote_session_id = Some("A".into());
    app.queued_messages.push("A followup".into());
    app.hidden_queued_system_messages.push("A reminder".into());
    app.pending_queued_dispatch = true;
    super::super::super::queue_recovery::retain_pending_ssh_on_session_switch(&mut app);
    app.remote_session_id = Some("B".into());
    super::super::super::queue_recovery::restore_parked_ssh_queue(&mut app, "B");
    assert!(app.queued_messages.is_empty());
    assert!(app.hidden_queued_system_messages.is_empty());
    assert!(!app.pending_queued_dispatch);
    app.queued_messages.push("B followup".into());
    super::super::super::queue_recovery::retain_pending_ssh_on_session_switch(&mut app);
    app.remote_session_id = Some("A".into());
    super::super::super::queue_recovery::restore_parked_ssh_queue(&mut app, "A");
    assert_eq!(app.queued_messages, ["A followup"]);
    assert_eq!(app.hidden_queued_system_messages, ["A reminder"]);
    assert!(app.pending_queued_dispatch);
    assert_eq!(app.parked_ssh_queues[0].0.as_deref(), Some("B"));
    match previous_home { Some(value) => crate::env::set_var("KCODE_HOME", value), None => crate::env::remove_var("KCODE_HOME") }
    match previous_ssh { Some(value) => crate::env::set_var("KCODE_SSH_REMOTE", value), None => crate::env::remove_var("KCODE_SSH_REMOTE") }
}
