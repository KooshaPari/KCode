use super::debug_recording::save_recording_to_path;
use std::sync::MutexGuard;

struct EnvRestore {
    home: Option<std::ffi::OsString>,
    namespace: Option<std::ffi::OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        match self.home.take() {
            Some(value) => jcode_core::env::set_var("JCODE_HOME", value),
            None => jcode_core::env::remove_var("JCODE_HOME"),
        }
        match self.namespace.take() {
            Some(value) => jcode_core::env::set_var("JCODE_DEV_NAMESPACE", value),
            None => jcode_core::env::remove_var("JCODE_DEV_NAMESPACE"),
        }
    }
}

#[cfg(unix)]
#[test]
fn recording_symlink_leaf_preserves_stable_target() {
    use std::os::unix::fs::symlink;

    let lock = crate::storage::lock_test_env();
    let _restore = EnvRestore {
        home: std::env::var_os("JCODE_HOME"),
        namespace: std::env::var_os("JCODE_DEV_NAMESPACE"),
        _lock: lock,
    };
    let home = tempfile::tempdir().expect("dev home");
    let stable = tempfile::NamedTempFile::new().expect("stable sentinel");
    std::fs::write(stable.path(), b"stable-recording").expect("write sentinel");
    jcode_core::env::set_var("JCODE_HOME", home.path());
    jcode_core::env::set_var("JCODE_DEV_NAMESPACE", "1");

    let path = home.path().join("config/jcode/recordings/recording.json");
    std::fs::create_dir_all(path.parent().expect("recording parent")).expect("create parent");
    symlink(stable.path(), &path).expect("recording symlink");

    assert!(save_recording_to_path(&path, "dev-recording").is_err());
    assert_eq!(
        std::fs::read(stable.path()).expect("read sentinel"),
        b"stable-recording"
    );
}
