use super::*;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

struct EnvRestore(Vec<(&'static str, Option<std::ffi::OsString>)>);

impl Drop for EnvRestore {
    fn drop(&mut self) {
        for (key, value) in self.0.drain(..) {
            if let Some(value) = value {
                crate::env::set_var(key, value);
            } else {
                crate::env::remove_var(key);
            }
        }
    }
}

#[test]
fn symlinked_swarm_runtime_roots_are_not_migrated_or_written() {
    let _lock = storage::lock_test_env();
    let keys = ["KCODE_HOME", "KCODE_RUNTIME_DIR", "KCODE_DEV_NAMESPACE"];
    let _restore = EnvRestore(
        keys.into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join(".kcode-dev");
    let runtime = home.join("run");
    let stable = temp.path().join("stable-swarm-state");
    std::fs::create_dir_all(&runtime).unwrap();
    std::fs::create_dir_all(&stable).unwrap();
    let sentinel = stable.join("swarm-123.json");
    std::fs::write(&sentinel, b"stable swarm sentinel\n").unwrap();
    std::fs::set_permissions(&sentinel, std::fs::Permissions::from_mode(0o640)).unwrap();
    let before = std::fs::symlink_metadata(&sentinel).unwrap();
    let identity = (before.dev(), before.ino(), before.permissions().mode());
    crate::env::set_var("KCODE_HOME", &home);
    crate::env::set_var("KCODE_RUNTIME_DIR", &runtime);
    crate::env::set_var("KCODE_DEV_NAMESPACE", "1");

    let current_root = storage::durable_state_dir().join(SWARM_STATE_DIR);
    std::fs::create_dir_all(current_root.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&stable, &current_root).unwrap();
    assert!(load_runtime_state().plans.is_empty());
    persist_swarm_state("swarm-123", None, Some("session-123"), &[]);
    assert_eq!(
        std::fs::read(&sentinel).unwrap(),
        b"stable swarm sentinel\n"
    );
    let after_current = std::fs::symlink_metadata(&sentinel).unwrap();
    assert_eq!(
        (
            after_current.dev(),
            after_current.ino(),
            after_current.permissions().mode()
        ),
        identity
    );

    std::fs::remove_file(&current_root).unwrap();
    let legacy_root = legacy_state_dir();
    std::os::unix::fs::symlink(&stable, &legacy_root).unwrap();
    assert!(load_runtime_state().plans.is_empty());
    assert_eq!(
        std::fs::read(&sentinel).unwrap(),
        b"stable swarm sentinel\n"
    );
    let after_legacy = std::fs::symlink_metadata(&sentinel).unwrap();
    assert_eq!(
        (
            after_legacy.dev(),
            after_legacy.ino(),
            after_legacy.permissions().mode()
        ),
        identity
    );
}

#[test]
fn rejected_swarm_persist_path_message_includes_path_and_guard_error() {
    let _lock = storage::lock_test_env();
    let keys = ["KCODE_HOME", "KCODE_RUNTIME_DIR", "KCODE_DEV_NAMESPACE"];
    let _restore = EnvRestore(
        keys.into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join(".kcode-dev");
    let runtime = home.join("run");
    let stable = temp.path().join("stable-swarm-state");
    std::fs::create_dir_all(&runtime).unwrap();
    std::fs::create_dir_all(&stable).unwrap();
    let sentinel = stable.join("swarm-123.json");
    std::fs::write(&sentinel, b"stable swarm sentinel\n").unwrap();
    crate::env::set_var("KCODE_HOME", &home);
    crate::env::set_var("KCODE_RUNTIME_DIR", &runtime);
    crate::env::set_var("KCODE_DEV_NAMESPACE", "1");
    let state_root = storage::durable_state_dir().join(SWARM_STATE_DIR);
    std::fs::create_dir_all(state_root.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&stable, &state_root).unwrap();
    let path = state_path("swarm-123");

    let message = rejected_swarm_persist_path(&path).expect("symlinked path rejection");
    assert!(message.contains(&path.display().to_string()));
    assert!(message.contains("symlink"));
    persist_swarm_state("swarm-123", None, Some("session-123"), &[]);

    assert_eq!(
        std::fs::read(&sentinel).unwrap(),
        b"stable swarm sentinel\n"
    );
}
