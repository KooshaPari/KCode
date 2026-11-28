use super::{
    maybe_run_pending_restart_restore_on_startup, run_restart_clear_command,
    run_restart_save_command,
};
use crate::session::Session;
use chrono::{DateTime, Utc};
use std::ffi::OsString;
use std::fs;

struct TestEnvGuard {
    prev_home: Option<OsString>,
    prev_socket: Option<OsString>,
    _temp_home: tempfile::TempDir,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl TestEnvGuard {
    fn new() -> anyhow::Result<Self> {
        let lock = crate::storage::lock_test_env();
        #[cfg(unix)]
        let temp_root = std::path::Path::new("/tmp");
        #[cfg(not(unix))]
        let temp_root = std::env::temp_dir();
        let temp_home = tempfile::Builder::new()
            .prefix("kcode-cli-restart-test-home-")
            .tempdir_in(temp_root)?;
        let prev_home = std::env::var_os("KCODE_HOME");
        let prev_socket = std::env::var_os("KCODE_SOCKET");
        crate::env::set_var("KCODE_HOME", temp_home.path());
        crate::env::set_var("KCODE_SOCKET", temp_home.path().join("kcode.sock"));
        Ok(Self {
            prev_home,
            prev_socket,
            _temp_home: temp_home,
            _lock: lock,
        })
    }
}

impl Drop for TestEnvGuard {
    fn drop(&mut self) {
        if let Some(prev_home) = &self.prev_home {
            crate::env::set_var("KCODE_HOME", prev_home);
        } else {
            crate::env::remove_var("KCODE_HOME");
        }
        if let Some(prev_socket) = &self.prev_socket {
            crate::env::set_var("KCODE_SOCKET", prev_socket);
        } else {
            crate::env::remove_var("KCODE_SOCKET");
        }
    }
}

#[tokio::test]
async fn restart_save_writes_empty_snapshot_without_auto_restore_flag() {
    let _guard = TestEnvGuard::new().expect("setup test env");

    run_restart_save_command(false, false)
        .await
        .expect("save restart snapshot");

    let snapshot = crate::restart_snapshot::load_snapshot().expect("load snapshot");
    assert!(!snapshot.auto_restore_on_next_start);
    assert!(snapshot.sessions.is_empty());
}

#[tokio::test]
async fn restart_save_refuses_to_replace_armed_snapshot_without_opt_in() {
    let _guard = TestEnvGuard::new().expect("setup test env");
    let created_at = Utc::now() - chrono::Duration::hours(1);
    let existing = sample_snapshot(true, "existing-sentinel", created_at);
    write_snapshot_bytes(&existing).expect("write armed snapshot directly");
    let path = crate::restart_snapshot::snapshot_path().expect("snapshot path");
    let before = fs::read(&path).expect("read existing bytes");

    let error = run_restart_save_command(false, false)
        .await
        .expect_err("plain save must refuse an armed snapshot");

    let after = fs::read(&path).expect("read snapshot after refusal");
    assert_eq!(after, before, "refusal must leave the file byte-identical");
    let message = format!("{error:#}");
    assert!(message.contains("Refusing to overwrite armed reboot snapshot"));
    assert!(message.contains(&existing.created_at.to_string()));
    assert!(message.contains("1 session(s)"));
    assert!(message.contains("--replace-armed"));
}

#[tokio::test]
async fn restart_save_auto_restore_with_zero_sessions_reports_not_armed() {
    let _guard = TestEnvGuard::new().expect("setup test env");
    let mut output = Vec::new();
    super::run_restart_save_command_with_writer(true, false, &mut output)
        .await
        .expect("save empty unarmed snapshot");

    let snapshot = crate::restart_snapshot::load_snapshot().expect("load snapshot");
    assert!(snapshot.sessions.is_empty());
    assert!(!snapshot.auto_restore_on_next_start);
    let message = String::from_utf8(output).expect("save output is UTF-8");
    assert!(message.contains("Automatic restore was not armed because no sessions were captured."));
}

#[tokio::test]
async fn restart_save_replace_armed_opt_in_overwrites_armed_snapshot() {
    let _guard = TestEnvGuard::new().expect("setup test env");
    let existing = sample_snapshot(
        true,
        "existing-sentinel",
        Utc::now() - chrono::Duration::hours(1),
    );
    write_snapshot_bytes(&existing).expect("write armed snapshot directly");
    let path = crate::restart_snapshot::snapshot_path().expect("snapshot path");
    let before = fs::read(&path).expect("read existing bytes");

    run_restart_save_command(true, true)
        .await
        .expect("replace armed snapshot with explicit opt-in");

    let after = fs::read(&path).expect("read replacement bytes");
    assert_ne!(
        after, before,
        "--replace-armed must overwrite the old snapshot"
    );
    let snapshot = crate::restart_snapshot::load_snapshot().expect("load replacement");
    assert!(snapshot.sessions.is_empty());
    assert!(!snapshot.auto_restore_on_next_start);
    assert_ne!(snapshot.created_at, existing.created_at);
}

#[tokio::test]
async fn restart_save_replaces_unarmed_snapshot() {
    let _guard = TestEnvGuard::new().expect("setup test env");
    let existing = sample_snapshot(
        false,
        "unarmed-sentinel",
        Utc::now() - chrono::Duration::hours(1),
    );
    write_snapshot_bytes(&existing).expect("write unarmed snapshot directly");
    let path = crate::restart_snapshot::snapshot_path().expect("snapshot path");
    let before = fs::read(&path).expect("read existing bytes");

    run_restart_save_command(true, false)
        .await
        .expect("save over unarmed snapshot");

    let after = fs::read(&path).expect("read replacement bytes");
    assert_ne!(
        after, before,
        "plain save must overwrite an unarmed snapshot"
    );
    let snapshot = crate::restart_snapshot::load_snapshot().expect("load replacement");
    assert!(snapshot.sessions.is_empty());
    assert!(!snapshot.auto_restore_on_next_start);
    assert_ne!(snapshot.created_at, existing.created_at);
}

fn write_snapshot_bytes(snapshot: &crate::restart_snapshot::RestartSnapshot) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec(snapshot)?;
    let path = crate::restart_snapshot::snapshot_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}

fn sample_snapshot(
    auto_restore_on_next_start: bool,
    session_id: &str,
    created_at: DateTime<Utc>,
) -> crate::restart_snapshot::RestartSnapshot {
    crate::restart_snapshot::RestartSnapshot {
        version: 1,
        created_at,
        auto_restore_on_next_start,
        sessions: vec![crate::restart_snapshot::RestartSnapshotSession {
            session_id: session_id.to_string(),
            display_name: "Existing sentinel".to_string(),
            working_dir: Some("/existing".to_string()),
            is_selfdev: false,
        }],
    }
}

#[tokio::test]
async fn pending_restore_returns_false_for_unarmed_snapshot() {
    let _guard = TestEnvGuard::new().expect("setup test env");

    run_restart_save_command(false, false)
        .await
        .expect("save restart snapshot");

    assert!(
        !maybe_run_pending_restart_restore_on_startup()
            .await
            .expect("check pending restore")
    );
    assert!(crate::restart_snapshot::load_snapshot().is_ok());
}

#[tokio::test]
async fn pending_restore_does_not_auto_restore_recent_crash_without_snapshot() {
    let _guard = TestEnvGuard::new().expect("setup test env");

    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg("exit 0")
        .spawn()
        .expect("spawn child");
    let dead_pid = child.id();
    let _ = child.wait().expect("wait for child");

    let mut crashed = Session::create_with_id(
        "session_no_startup_auto_restore_crash".to_string(),
        None,
        Some("Do Not Respawn".to_string()),
    );
    crashed.mark_active_with_pid(dead_pid);
    crashed.save().expect("save active session with dead pid");

    assert!(
        !maybe_run_pending_restart_restore_on_startup()
            .await
            .expect("check pending restore")
    );
    assert!(crate::restart_snapshot::load_snapshot().is_err());
}

#[tokio::test]
async fn restart_clear_removes_saved_snapshot() {
    let _guard = TestEnvGuard::new().expect("setup test env");

    run_restart_save_command(false, false)
        .await
        .expect("save restart snapshot");
    run_restart_clear_command().expect("clear restart snapshot");

    assert!(crate::restart_snapshot::load_snapshot().is_err());
}
