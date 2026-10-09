use super::*;
use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvRestore {
    home: Option<std::ffi::OsString>,
    namespace: Option<std::ffi::OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
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

fn dev_home() -> (tempfile::TempDir, EnvRestore) {
    let lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let restore = EnvRestore {
        home: std::env::var_os("KCODE_HOME"),
        namespace: std::env::var_os("KCODE_DEV_NAMESPACE"),
        _lock: lock,
    };
    let home = tempfile::tempdir().expect("temporary dev home");
    kcode_core::env::set_var("KCODE_HOME", home.path());
    kcode_core::env::set_var("KCODE_DEV_NAMESPACE", "1");
    (home, restore)
}

#[cfg(unix)]
#[test]
fn model_cache_symlink_leaf_preserves_stable_target() {
    use std::os::unix::fs::symlink;

    let (_home, _restore) = dev_home();
    let stable = tempfile::NamedTempFile::new().expect("stable sentinel");
    std::fs::write(stable.path(), b"stable-model-cache").expect("write sentinel");
    let path = cache_path();
    std::fs::create_dir_all(path.parent().expect("cache parent")).expect("cache directory");
    symlink(stable.path(), &path).expect("cache symlink");

    save_disk_cache(&[]);
    assert!(load_disk_cache_entry().is_none());
    assert_eq!(
        std::fs::read(stable.path()).expect("read sentinel"),
        b"stable-model-cache"
    );
}

#[cfg(unix)]
#[test]
fn endpoints_cache_symlink_leaf_preserves_stable_target() {
    use std::os::unix::fs::symlink;

    let (_home, _restore) = dev_home();
    let stable = tempfile::NamedTempFile::new().expect("stable sentinel");
    std::fs::write(stable.path(), b"stable-endpoints-cache").expect("write sentinel");
    let path = endpoints_cache_path("provider/model");
    std::fs::create_dir_all(path.parent().expect("cache parent")).expect("cache directory");
    symlink(stable.path(), &path).expect("endpoints symlink");

    save_endpoints_disk_cache("provider/model", &[]);
    assert!(load_endpoints_disk_cache("provider/model").is_none());
    assert!(load_endpoints_disk_cache_public("provider/model").is_none());
    assert_eq!(
        std::fs::read(stable.path()).expect("read sentinel"),
        b"stable-endpoints-cache"
    );
}
