use super::{
    AUTO_RESTORE_CRASH_MAX_AGE_HOURS, RESTORE_CLAIM_STALE_MAX_AGE_SECS,
    arm_auto_restore_from_recent_crashes, capture_current_snapshot, clear_snapshot, load_snapshot,
    resolve_session_cwd, restore_claim_path, save_current_snapshot, try_acquire_restore_claim,
};
use crate::session::Session;
use chrono::Utc;
use std::ffi::OsString;

struct TestEnvGuard {
    prev_home: Option<OsString>,
    _temp_home: tempfile::TempDir,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl TestEnvGuard {
    fn new() -> anyhow::Result<Self> {
        let lock = crate::storage::lock_test_env();
        let root = std::path::PathBuf::from("/private/tmp");
        let root = if root.is_dir() {
            root
        } else {
            std::env::temp_dir()
        };
        let temp_home = tempfile::Builder::new()
            .prefix("kcode-restart-snapshot-test-home-")
            .tempdir_in(root)?;
        let prev_home = std::env::var_os("KCODE_HOME");
        crate::env::set_var("KCODE_HOME", temp_home.path());
        Ok(Self {
            prev_home,
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
    }
}

#[test]
fn capture_current_snapshot_includes_active_sessions_only() {
    let _guard = TestEnvGuard::new().expect("setup test env");

    let mut active = Session::create(None, Some("Active".to_string()));
    active.working_dir = Some("/tmp".to_string());
    active.mark_active_with_pid(std::process::id());
    active.save().expect("save active session");

    let mut closed = Session::create(None, Some("Closed".to_string()));
    closed.mark_closed();
    closed.save().expect("save closed session");

    let snapshot = capture_current_snapshot().expect("capture snapshot");
    assert_eq!(snapshot.sessions.len(), 1);
    assert_eq!(snapshot.sessions[0].session_id, active.id);
    assert_eq!(snapshot.sessions[0].working_dir.as_deref(), Some("/tmp"));
}

#[test]
fn save_and_load_snapshot_round_trip() {
    let _guard = TestEnvGuard::new().expect("setup test env");

    let mut active = Session::create(None, Some("Restore Me".to_string()));
    active.mark_active_with_pid(std::process::id());
    active.save().expect("save active session");

    let saved = save_current_snapshot().expect("save snapshot");
    let loaded = load_snapshot().expect("load snapshot");
    assert_eq!(saved.sessions.len(), 1);
    assert_eq!(loaded.sessions.len(), 1);
    assert!(!loaded.auto_restore_on_next_start);
    assert_eq!(loaded.sessions[0].session_id, active.id);
}

#[test]
fn set_auto_restore_updates_saved_snapshot() {
    let _guard = TestEnvGuard::new().expect("setup test env");

    let mut active = Session::create(None, Some("Auto Restore".to_string()));
    active.mark_active_with_pid(std::process::id());
    active.save().expect("save active session");
    save_current_snapshot().expect("save snapshot");

    assert!(super::set_auto_restore_on_next_start(true).expect("set auto restore"));
    let loaded = load_snapshot().expect("load snapshot");
    assert!(loaded.auto_restore_on_next_start);
}

#[test]
fn clear_snapshot_removes_saved_file() {
    let _guard = TestEnvGuard::new().expect("setup test env");

    let mut active = Session::create(None, Some("Clear Me".to_string()));
    active.mark_active_with_pid(std::process::id());
    active.save().expect("save active session");
    save_current_snapshot().expect("save snapshot");

    assert!(clear_snapshot().expect("clear snapshot"));
    assert!(load_snapshot().is_err());
}

#[test]
fn arm_auto_restore_from_recent_crashes_captures_dead_active_sessions() {
    let _guard = TestEnvGuard::new().expect("setup test env");

    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg("exit 0")
        .spawn()
        .expect("spawn child");
    let dead_pid = child.id();
    let _ = child.wait().expect("wait for child");

    let mut crashed = Session::create_with_id(
        "session_auto_restore_crash".to_string(),
        None,
        Some("Crash Me".to_string()),
    );
    crashed.working_dir = Some("/tmp".to_string());
    crashed.mark_active_with_pid(dead_pid);
    crashed.save().expect("save crashed session");

    let snapshot = arm_auto_restore_from_recent_crashes()
        .expect("arm crash snapshot")
        .expect("expected crash snapshot");
    assert!(snapshot.auto_restore_on_next_start);
    assert_eq!(snapshot.sessions.len(), 1);
    assert_eq!(snapshot.sessions[0].session_id, crashed.id);
    assert_eq!(snapshot.sessions[0].working_dir.as_deref(), Some("/tmp"));

    let persisted = load_snapshot().expect("load persisted snapshot");
    assert!(persisted.auto_restore_on_next_start);
    assert_eq!(persisted.sessions.len(), 1);

    let refreshed = Session::load(&crashed.id).expect("reload crashed session");
    assert!(matches!(
        refreshed.status,
        crate::session::SessionStatus::Crashed { .. }
    ));
}

#[test]
fn arm_auto_restore_from_recent_crashes_ignores_old_crashes() {
    let _guard = TestEnvGuard::new().expect("setup test env");

    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg("exit 0")
        .spawn()
        .expect("spawn child");
    let dead_pid = child.id();
    let _ = child.wait().expect("wait for child");

    let mut crashed = Session::create_with_id(
        "session_old_auto_restore_crash".to_string(),
        None,
        Some("Old Crash".to_string()),
    );
    let old_ts = Utc::now() - chrono::Duration::hours(AUTO_RESTORE_CRASH_MAX_AGE_HOURS + 2);
    crashed.updated_at = old_ts;
    crashed.last_active_at = Some(old_ts);
    crashed.status = crate::session::SessionStatus::Active;
    crashed.last_pid = Some(dead_pid);
    crashed.save().expect("save stale active session");
    let active_dir = crate::storage::jcode_dir()
        .expect("jcode dir")
        .join("active_pids");
    std::fs::create_dir_all(&active_dir).expect("create active pid dir");
    std::fs::write(active_dir.join(&crashed.id), dead_pid.to_string())
        .expect("write active pid file");

    assert!(
        arm_auto_restore_from_recent_crashes()
            .expect("arm stale crash snapshot")
            .is_none()
    );
    assert!(load_snapshot().is_err());
}

#[test]
fn resolve_session_cwd_keeps_existing_configured_dir() {
    let configured = tempfile::Builder::new()
        .prefix("kcode-restart-cwd-existing-")
        .tempdir()
        .expect("create configured dir");
    let resolved = resolve_session_cwd(configured.path().to_str());
    assert_eq!(resolved, configured.path());
}

#[test]
fn resolve_session_cwd_uses_home_when_configured_missing() {
    let home = dirs::home_dir().expect("resolve home dir");
    assert!(home.is_dir());
    let resolved = resolve_session_cwd(None);
    assert_eq!(resolved, home);
}

#[test]
fn resolve_session_cwd_uses_home_when_configured_path_missing() {
    let missing = tempfile::Builder::new()
        .prefix("kcode-restart-cwd-missing-")
        .tempdir()
        .expect("create temp dir")
        .path()
        .join("does-not-exist");
    assert!(!missing.exists());
    let home = dirs::home_dir().expect("resolve home dir");
    assert!(home.is_dir());
    let resolved = resolve_session_cwd(missing.to_str());
    assert_eq!(resolved, home);
}

#[test]
fn resolve_session_cwd_uses_home_when_configured_path_empty() {
    let home = dirs::home_dir().expect("resolve home dir");
    assert!(home.is_dir());
    let resolved = resolve_session_cwd(Some(""));
    assert_eq!(resolved, home);
}

#[test]
fn restore_claim_allows_single_winner() {
    let _guard = TestEnvGuard::new().expect("setup test env");

    let winner = try_acquire_restore_claim()
        .expect("acquire first claim")
        .expect("first claim should win");
    let loser = try_acquire_restore_claim().expect("second claim attempt");
    assert!(loser.is_none(), "second concurrent restore claim must lose");

    winner.release().expect("release winner claim");
    assert!(!restore_claim_path().expect("claim path").exists());
}

#[test]
fn restore_claim_loser_does_not_corrupt_snapshot() {
    let _guard = TestEnvGuard::new().expect("setup test env");

    let mut active = Session::create(None, Some("Restore Me".to_string()));
    active.mark_active_with_pid(std::process::id());
    active.save().expect("save active session");
    let saved = save_current_snapshot().expect("save snapshot");

    let winner = try_acquire_restore_claim()
        .expect("acquire first claim")
        .expect("first claim should win");
    let loser = try_acquire_restore_claim().expect("second claim attempt");
    assert!(loser.is_none());

    let loaded = load_snapshot().expect("snapshot must remain readable");
    assert_eq!(loaded.sessions.len(), saved.sessions.len());
    assert_eq!(loaded.sessions[0].session_id, saved.sessions[0].session_id);

    winner.release().expect("release winner claim");
    let loaded_after = load_snapshot().expect("snapshot must remain after loser");
    assert_eq!(
        loaded_after.sessions[0].session_id,
        saved.sessions[0].session_id
    );
}

#[test]
fn stale_restore_claim_is_reclaimed_after_dead_pid() {
    let _guard = TestEnvGuard::new().expect("setup test env");

    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg("exit 0")
        .spawn()
        .expect("spawn child");
    let dead_pid = child.id();
    let _ = child.wait().expect("wait for child");

    let claim_path = restore_claim_path().expect("claim path");
    std::fs::create_dir_all(claim_path.parent().expect("claim parent")).expect("create claim dir");
    let stale = serde_json::json!({
        "pid": dead_pid,
        "created_at": (Utc::now() - chrono::Duration::seconds(RESTORE_CLAIM_STALE_MAX_AGE_SECS + 5)).to_rfc3339(),
    });
    std::fs::write(
        &claim_path,
        serde_json::to_vec(&stale).expect("serialize stale claim"),
    )
    .expect("write stale claim");

    let winner = try_acquire_restore_claim()
        .expect("acquire stale claim")
        .expect("stale dead-pid claim should be reclaimed");
    winner.release().expect("release reclaimed claim");
}
