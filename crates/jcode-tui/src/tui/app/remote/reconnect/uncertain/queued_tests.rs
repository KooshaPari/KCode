use super::*;
use tokio::io::AsyncReadExt;

#[test]
fn queued_user_partial_write_restores_only_user_text() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let old_home = std::env::var_os("KCODE_HOME");
    let old_ssh = std::env::var_os("KCODE_SSH_REMOTE");
    crate::env::set_var("KCODE_HOME", home.path());
    crate::env::set_var("KCODE_SSH_REMOTE", "fixture");
    let mut app = App::new_for_remote(Some("session_one".into()));
    app.remote_session_id = Some("session_one".into());
    let message = "user".repeat(32);
    app.queued_messages.push(message.clone());
    app.hidden_queued_system_messages
        .push("secret system reminder".into());
    let state = RemoteRunState::default();
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut remote = crate::tui::backend::RemoteConnection::dummy();
        remote.inject_partial_write_failure();
        remote.mark_history_loaded();
        let expected = serde_json::to_string(&crate::protocol::Request::Message {
            id: 1,
            content: message.clone(),
            images: Vec::new(),
            system_reminder: Some("secret system reminder".into()),
            active_skill: app.active_skill.clone(),
            no_reply: false,
        })
        .unwrap();
        let mut peer = remote.take_dummy_peer().unwrap();
        let read = tokio::spawn(async move {
            let mut prefix = [0; 64];
            peer.read_exact(&mut prefix).await.unwrap();
            prefix
        });
        super::super::super::process_remote_followups_guarded(&mut app, &mut remote, &state).await;
        assert_eq!(read.await.unwrap().as_slice(), &expected.as_bytes()[..64]);
        assert!(
            app.queued_messages.is_empty(),
            "uncertain message must not be auto-requeued"
        );
        assert!(has_unresolved_for_session(&app, &state));
        restore_latest_to_draft(&mut app, &state).unwrap();
        assert_eq!(app.input, message);
        assert!(!app.input.contains("secret system reminder"));
    });
    match old_home {
        Some(v) => crate::env::set_var("KCODE_HOME", v),
        None => crate::env::remove_var("KCODE_HOME"),
    }
    match old_ssh {
        Some(v) => crate::env::set_var("KCODE_SSH_REMOTE", v),
        None => crate::env::remove_var("KCODE_SSH_REMOTE"),
    }
}
