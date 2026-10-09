use super::debug_recording::save_recording_to_path;
use std::sync::MutexGuard;

struct EnvRestore {
    user_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
    namespace: Option<std::ffi::OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        match self.user_home.take() {
            Some(v) => kcode_core::env::set_var("HOME", v),
            None => kcode_core::env::remove_var("HOME"),
        }
        match self.home.take() {
            Some(value) => kcode_core::env::set_var("KCODE_HOME", value),
            None => kcode_core::env::remove_var("KCODE_HOME"),
        }
        match self.namespace.take() {
            Some(value) => kcode_core::env::set_var("KCODE_DEV_NAMESPACE", value),
            None => kcode_core::env::remove_var("KCODE_DEV_NAMESPACE"),
        }
    }
}

#[cfg(unix)]
#[test]
fn recording_symlink_leaf_preserves_stable_target() {
    use std::os::unix::fs::symlink;

    let lock = crate::storage::lock_test_env();
    let _restore = EnvRestore {
        user_home: std::env::var_os("HOME"),
        home: std::env::var_os("KCODE_HOME"),
        namespace: std::env::var_os("KCODE_DEV_NAMESPACE"),
        _lock: lock,
    };
    let home = tempfile::tempdir().expect("dev home");
    let stable = tempfile::NamedTempFile::new().expect("stable sentinel");
    std::fs::write(stable.path(), b"stable-recording").expect("write sentinel");
    kcode_core::env::set_var("HOME", home.path());
    kcode_core::env::set_var("KCODE_HOME", home.path().join(".kcode-dev"));
    kcode_core::env::set_var("KCODE_DEV_NAMESPACE", "1");

    let path = crate::storage::app_config_dir()
        .unwrap()
        .join("recordings/recording.json");
    std::fs::create_dir_all(path.parent().expect("recording parent")).expect("create parent");
    symlink(stable.path(), &path).expect("recording symlink");

    assert!(save_recording_to_path(&path, "dev-recording").is_err());
    assert_eq!(
        std::fs::read(stable.path()).expect("read sentinel"),
        b"stable-recording"
    );
}
