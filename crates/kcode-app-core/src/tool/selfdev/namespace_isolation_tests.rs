use super::*;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

struct EnvRestore(&'static str, Option<std::ffi::OsString>);

impl Drop for EnvRestore {
    fn drop(&mut self) {
        if let Some(value) = self.1.take() {
            crate::env::set_var(self.0, value);
        } else {
            crate::env::remove_var(self.0);
        }
    }
}

#[test]
fn reload_context_read_write_and_remove_reject_symlinked_state() {
    let _storage_guard = crate::storage::lock_test_env();
    let temp_home = tempfile::TempDir::new().expect("temp home");
    let _home_guard = EnvRestore("KCODE_HOME", std::env::var_os("KCODE_HOME"));
    let _namespace_guard = EnvRestore(
        "KCODE_DEV_NAMESPACE",
        std::env::var_os("KCODE_DEV_NAMESPACE"),
    );
    crate::env::set_var("KCODE_HOME", temp_home.path());
    crate::env::set_var("KCODE_DEV_NAMESPACE", "1");
    let target = temp_home.path().join("stable-reload-sentinel.json");
    std::fs::write(&target, b"stable reload context sentinel\n").expect("sentinel");
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640))
        .expect("sentinel mode");
    let before = std::fs::symlink_metadata(&target).expect("sentinel metadata");
    let identity = (before.dev(), before.ino(), before.permissions().mode());

    let session_path = ReloadContext::path_for_session("symlink-target").expect("session path");
    std::os::unix::fs::symlink(&target, &session_path).expect("session symlink");
    let legacy_path = temp_home.path().join("reload-context.json");
    std::os::unix::fs::symlink(&target, &legacy_path).expect("legacy symlink");
    let context = ReloadContext {
        task_context: Some("must not write through a symlink".to_string()),
        version_before: "before".to_string(),
        version_after: "after".to_string(),
        session_id: "symlink-target".to_string(),
        timestamp: "2026-09-29T00:00:00Z".to_string(),
    };

    assert!(context.save().is_err());
    assert!(ReloadContext::peek_for_session("symlink-target").is_err());
    assert!(ReloadContext::load_for_session("symlink-target").is_err());
    assert!(ReloadContext::load().is_err());
    assert_eq!(
        std::fs::read(&target).expect("sentinel bytes"),
        b"stable reload context sentinel\n"
    );
    let after = std::fs::symlink_metadata(&target).expect("sentinel metadata after");
    assert_eq!(
        (after.dev(), after.ino(), after.permissions().mode()),
        identity
    );
    assert!(session_path.is_symlink());
    assert!(legacy_path.is_symlink());
}
