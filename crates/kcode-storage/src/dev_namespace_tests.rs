use super::*;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

struct EnvRestore(Option<std::ffi::OsString>, Option<std::ffi::OsString>);

impl Drop for EnvRestore {
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
fn dev_namespace_path_guard_rejects_symlink_components_without_touching_target() {
    let _lock = lock_test_env();
    let temp = tempfile::tempdir().expect("temp dir");
    let home = temp.path().join(".kcode-dev");
    let stable = temp.path().join("stable.json");
    let link = home.join("telemetry_milestone_provider_login_1");
    std::fs::create_dir_all(&home).expect("dev home");
    std::fs::write(&stable, b"sentinel bytes").expect("sentinel");
    std::fs::set_permissions(&stable, std::fs::Permissions::from_mode(0o640))
        .expect("sentinel mode");
    let before = std::fs::symlink_metadata(&stable).expect("sentinel metadata");
    let identity = (before.dev(), before.ino(), before.permissions().mode());
    std::os::unix::fs::symlink(&stable, &link).expect("symlink");

    let _restore = EnvRestore(
        std::env::var_os("KCODE_HOME"),
        std::env::var_os("KCODE_DEV_NAMESPACE"),
    );
    kcode_core::env::set_var("KCODE_HOME", &home);
    kcode_core::env::set_var("KCODE_DEV_NAMESPACE", "1");

    assert!(reject_dev_home_symlink_path(&link).is_err());
    assert_eq!(
        std::fs::read(&stable).expect("sentinel bytes"),
        b"sentinel bytes"
    );
    let after = std::fs::symlink_metadata(&stable).expect("sentinel metadata after");
    assert_eq!(
        (after.dev(), after.ino(), after.permissions().mode()),
        identity
    );
}

#[test]
fn json_recovery_rejects_a_symlinked_backup_without_reading_or_repairing_it() {
    use serde_json::Value;

    let _lock = lock_test_env();
    let temp = tempfile::tempdir().expect("temp dir");
    let home = temp.path().join(".kcode-dev");
    std::fs::create_dir_all(&home).expect("dev home");
    let primary = home.join("reload-context-session.json");
    let backup = primary.with_extension("bak");
    let target = temp.path().join("stable-backup.json");
    std::fs::write(&primary, b"corrupt json").expect("corrupt primary");
    std::fs::write(&target, br#"{"sentinel":"stable"}"#).expect("backup sentinel");
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640))
        .expect("sentinel mode");
    let before = std::fs::symlink_metadata(&target).expect("sentinel metadata");
    let identity = (before.dev(), before.ino(), before.permissions().mode());
    std::os::unix::fs::symlink(&target, &backup).expect("backup symlink");
    let _restore = EnvRestore(
        std::env::var_os("KCODE_HOME"),
        std::env::var_os("KCODE_DEV_NAMESPACE"),
    );
    kcode_core::env::set_var("KCODE_HOME", &home);
    kcode_core::env::set_var("KCODE_DEV_NAMESPACE", "1");

    assert!(read_json_with_recovery_handler::<Value, _>(&primary, |_| {}).is_err());
    assert_eq!(
        std::fs::read(&target).expect("sentinel bytes"),
        br#"{"sentinel":"stable"}"#
    );
    let after = std::fs::symlink_metadata(&target).expect("sentinel metadata after");
    assert_eq!(
        (after.dev(), after.ino(), after.permissions().mode()),
        identity
    );
    assert!(backup.is_symlink());
}
