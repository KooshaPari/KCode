use super::dump_to_file;
use std::sync::MutexGuard;

struct EnvRestore {
    user_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
    namespace: Option<std::ffi::OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        unsafe {
            match self.user_home.take() {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            match self.home.take() {
                Some(value) => std::env::set_var("KCODE_HOME", value),
                None => std::env::remove_var("KCODE_HOME"),
            }
            match self.namespace.take() {
                Some(value) => std::env::set_var("KCODE_DEV_NAMESPACE", value),
                None => std::env::remove_var("KCODE_DEV_NAMESPACE"),
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn visual_debug_symlink_leaf_preserves_stable_target() {
    use std::os::unix::fs::symlink;

    let lock = super::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _restore = EnvRestore {
        user_home: std::env::var_os("HOME"),
        home: std::env::var_os("KCODE_HOME"),
        namespace: std::env::var_os("KCODE_DEV_NAMESPACE"),
        _lock: lock,
    };
    let home = tempfile::tempdir().expect("dev home");
    let stable = tempfile::NamedTempFile::new().expect("stable sentinel");
    std::fs::write(stable.path(), b"stable-visual-debug").expect("write sentinel");
    unsafe {
        std::env::set_var("HOME", home.path());
        std::env::set_var("KCODE_HOME", home.path().join(".kcode-dev"));
        std::env::set_var("KCODE_DEV_NAMESPACE", "1");
    }
    let debug_dir = home.path().join(".kcode-dev/config/kcode");
    std::fs::create_dir_all(&debug_dir).expect("debug directory");
    symlink(stable.path(), debug_dir.join("visual-debug.txt")).expect("debug symlink");

    assert!(dump_to_file().is_err());
    assert_eq!(
        std::fs::read(stable.path()).expect("read sentinel"),
        b"stable-visual-debug"
    );
}
