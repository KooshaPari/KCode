use super::*;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

struct EnvRestore(Option<std::ffi::OsString>, Option<std::ffi::OsString>);

impl Drop for EnvRestore {
    fn drop(&mut self) {
        if let Some(value) = self.0.take() {
            crate::env::set_var("KCODE_HOME", value);
        } else {
            crate::env::remove_var("KCODE_HOME");
        }
        if let Some(value) = self.1.take() {
            crate::env::set_var("KCODE_DEV_NAMESPACE", value);
        } else {
            crate::env::remove_var("KCODE_DEV_NAMESPACE");
        }
    }
}

#[test]
fn changelog_read_and_write_ignore_symlinked_state() {
    let _lock = crate::storage::lock_test_env();
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join(".kcode-dev");
    std::fs::create_dir_all(&home).unwrap();
    let target = temp.path().join("stable-changelog.txt");
    std::fs::write(&target, b"hash-1\n").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
    let before = std::fs::symlink_metadata(&target).unwrap();
    let identity = (before.dev(), before.ino(), before.permissions().mode());
    let state = home.join("last_seen_changelog");
    std::os::unix::fs::symlink(&target, &state).unwrap();
    let _restore = EnvRestore(
        std::env::var_os("KCODE_HOME"),
        std::env::var_os("KCODE_DEV_NAMESPACE"),
    );
    crate::env::set_var("KCODE_HOME", &home);
    crate::env::set_var("KCODE_DEV_NAMESPACE", "1");
    let entries =
        parse_changelog_from("hash-1\x1e\x1e1\x1esubject one\x1fhash-2\x1e\x1e2\x1esubject two");

    assert_eq!(
        unseen_changelog_entries(&state, &entries),
        vec!["subject one", "subject two"]
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"hash-1\n");
    let after = std::fs::symlink_metadata(&target).unwrap();
    assert_eq!(
        (after.dev(), after.ino(), after.permissions().mode()),
        identity
    );
    assert!(state.is_symlink());
}
