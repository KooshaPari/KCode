use super::*;
use std::ffi::OsString;

fn lock_env() -> std::sync::MutexGuard<'static, ()> {
    crate::lock_test_env()
}

struct RestoreEnv(Option<OsString>, Option<OsString>);

impl Drop for RestoreEnv {
    fn drop(&mut self) {
        match self.0.take() {
            Some(value) => kcode_core::env::set_var("KCODE_HOME", value),
            None => kcode_core::env::remove_var("KCODE_HOME"),
        }
        match self.1.take() {
            Some(value) => kcode_core::env::set_var("KCODE_DEV_NAMESPACE", value),
            None => kcode_core::env::remove_var("KCODE_DEV_NAMESPACE"),
        }
    }
}

#[test]
fn session_counts_counts_live_and_streaming_only() {
    let _guard = lock_env();
    let original_home = std::env::var_os("KCODE_HOME");
    let _restore = RestoreEnv(
        original_home.clone(),
        std::env::var_os("KCODE_DEV_NAMESPACE"),
    );
    let temp = tempfile::tempdir().expect("tempdir");
    kcode_core::env::set_var("KCODE_HOME", temp.path());

    let live = std::process::id();
    let dead = 999_999u32;
    register_active_pid("session_alpha", live);
    mark_streaming("session_alpha");
    register_active_pid("session_beta", live);
    register_active_pid("session_gamma", dead);
    register_active_pid("session_delta", live);
    if let Some(dir) = streaming_pids_dir() {
        let _ = std::fs::write(dir.join("session_delta"), dead.to_string());
    }

    let counts = session_counts();
    assert_eq!(counts.total, 3);
    assert_eq!(counts.streaming, 1);
    let sessions = session_presence();
    assert_eq!(sessions.len(), 3);
    let by_id = |id: &str| {
        sessions
            .iter()
            .find(|session| session.session_id == id)
            .unwrap_or_else(|| panic!("{id} should be present"))
    };
    assert!(by_id("session_alpha").streaming);
    assert!(!by_id("session_beta").streaming);
    assert!(!by_id("session_delta").streaming);
    assert_eq!(by_id("session_alpha").pid, live);
    assert!(
        !sessions
            .iter()
            .any(|session| session.session_id == "session_gamma")
    );
    unmark_streaming("session_alpha");
    assert_eq!(session_counts().streaming, 0);
    register_active_pid("session_epsilon", live);
    mark_streaming("session_epsilon");
    assert_eq!(session_counts().streaming, 1);
    unregister_active_pid("session_epsilon");
    assert_eq!(session_counts().streaming, 0);

    drop(_restore);
    assert_eq!(std::env::var_os("KCODE_HOME"), original_home);
}

#[test]
fn streaming_guard_marks_and_clears_on_drop() {
    let _guard = lock_env();
    let original_home = std::env::var_os("KCODE_HOME");
    let _restore = RestoreEnv(
        original_home.clone(),
        std::env::var_os("KCODE_DEV_NAMESPACE"),
    );
    let temp = tempfile::tempdir().expect("tempdir");
    kcode_core::env::set_var("KCODE_HOME", temp.path());
    register_active_pid("session_guard", std::process::id());
    assert_eq!(session_counts().streaming, 0);
    {
        let _streaming = StreamingGuard::new("session_guard");
        assert_eq!(session_counts().streaming, 1);
    }
    assert_eq!(session_counts().streaming, 0);
    drop(_restore);
    assert_eq!(std::env::var_os("KCODE_HOME"), original_home);
}

#[test]
fn user_session_counts_exclude_internal_sessions() {
    let _guard = lock_env();
    let original_home = std::env::var_os("KCODE_HOME");
    let _restore = RestoreEnv(
        original_home.clone(),
        std::env::var_os("KCODE_DEV_NAMESPACE"),
    );
    let temp = tempfile::tempdir().expect("tempdir");
    kcode_core::env::set_var("KCODE_HOME", temp.path());
    let live = std::process::id();
    register_active_pid("session_user", live);
    register_active_pid("session_worker", live);
    set_session_internal("session_worker", true);
    mark_streaming("session_worker");
    assert!(session_is_internal("session_worker"));
    assert!(!session_is_internal("session_user"));
    assert_eq!(session_counts().total, 2);
    assert_eq!(session_counts().streaming, 1);
    let user_counts = user_session_counts();
    assert_eq!(user_counts.total, 1);
    assert_eq!(user_counts.streaming, 0);
    let user_sessions = user_session_presence();
    assert_eq!(user_sessions.len(), 1);
    assert_eq!(user_sessions[0].session_id, "session_user");
    set_session_internal("session_worker", false);
    assert_eq!(user_session_counts().total, 2);
    set_session_internal("session_worker", true);
    unregister_active_pid("session_worker");
    assert!(!session_is_internal("session_worker"));
    drop(_restore);
    assert_eq!(std::env::var_os("KCODE_HOME"), original_home);
}

#[cfg(unix)]
#[test]
fn pid_state_symlinks_never_read_or_write_targets() {
    let _guard = lock_env();
    let _restore = RestoreEnv(
        std::env::var_os("KCODE_HOME"),
        std::env::var_os("KCODE_DEV_NAMESPACE"),
    );
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("dev");
    std::fs::create_dir_all(&home).unwrap();
    let home = std::fs::canonicalize(home).unwrap();
    let active = home.join("active_pids");
    let streaming = home.join("streaming_pids");
    let internal = home.join("internal_pids");
    let target = temp.path().join("sentinel");
    std::fs::create_dir_all(&active).unwrap();
    std::fs::create_dir_all(&streaming).unwrap();
    std::fs::create_dir_all(&internal).unwrap();
    std::fs::write(&target, b"stable sentinel").unwrap();
    for leaf in [&active, &streaming, &internal] {
        std::os::unix::fs::symlink(&target, leaf.join("write_link")).unwrap();
    }
    kcode_core::env::set_var("KCODE_HOME", &home);
    kcode_core::env::set_var("KCODE_DEV_NAMESPACE", "1");
    register_active_pid("write_link", 7);
    mark_streaming("write_link");
    set_session_internal("write_link", true);
    assert_eq!(std::fs::read(&target).unwrap(), b"stable sentinel");

    let pid = std::process::id();
    std::fs::write(&target, pid.to_string()).unwrap();
    std::os::unix::fs::symlink(&target, active.join("read_link")).unwrap();
    register_active_pid("session_read", pid);
    std::os::unix::fs::symlink(&target, streaming.join("session_read")).unwrap();
    std::os::unix::fs::symlink(&target, internal.join("session_read")).unwrap();
    assert!(!active_session_ids().iter().any(|id| id == "read_link"));
    assert_eq!(
        find_active_session_id_by_pid(pid).as_deref(),
        Some("session_read")
    );
    assert!(!session_is_internal("session_read"));
    let session = session_presence().pop().unwrap();
    assert!(!session.streaming);
    assert!(!session.internal);
    assert_eq!(std::fs::read_to_string(&target).unwrap(), pid.to_string());
}
