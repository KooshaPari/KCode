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
fn model_cache_read_does_not_follow_symlink_leaf_in_dev_namespace() {
    use std::os::unix::fs::symlink;

    let (_home, _restore) = dev_home();
    let cache = DiskCache {
        cached_at: current_unix_secs().unwrap(),
        source_api_base: None,
        models: Vec::new(),
    };
    let stable = tempfile::NamedTempFile::new().expect("stable cache target");
    std::fs::write(
        stable.path(),
        serde_json::to_vec(&cache).expect("serialize cache"),
    )
    .expect("write cache target");
    let path = cache_path();
    std::fs::create_dir_all(path.parent().expect("cache parent")).expect("cache directory");
    symlink(stable.path(), &path).expect("cache symlink");

    assert!(read_cache_content(&path).is_none());
    assert!(load_disk_cache_entry().is_none());
}

#[cfg(unix)]
#[test]
fn stable_cache_read_keeps_following_existing_symlinks() {
    use std::os::unix::fs::symlink;

    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _restore = EnvRestore {
        home: std::env::var_os("KCODE_HOME"),
        namespace: std::env::var_os("KCODE_DEV_NAMESPACE"),
        _lock,
    };
    kcode_core::env::remove_var("KCODE_DEV_NAMESPACE");
    let stable = tempfile::NamedTempFile::new().expect("stable cache target");
    std::fs::write(stable.path(), b"stable cache contents").expect("write target");
    let link = stable.path().with_extension("link");
    symlink(stable.path(), &link).expect("cache symlink");

    assert_eq!(
        read_cache_content(&link).as_deref(),
        Some("stable cache contents")
    );
}

#[cfg(unix)]
#[test]
fn stable_cache_write_keeps_the_canonical_symlink_target_after_link_replacement() {
    use std::os::unix::fs::symlink;

    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _restore = EnvRestore {
        home: std::env::var_os("KCODE_HOME"),
        namespace: std::env::var_os("KCODE_DEV_NAMESPACE"),
        _lock,
    };
    kcode_core::env::remove_var("KCODE_DEV_NAMESPACE");
    let temp = tempfile::tempdir().expect("cache directory");
    let intended = temp.path().join("intended.json");
    let replacement = temp.path().join("replacement.json");
    let link = temp.path().join("cache.json");
    std::fs::write(&intended, b"old intended").unwrap();
    std::fs::write(&replacement, b"stable replacement").unwrap();
    symlink(&intended, &link).unwrap();

    write_cache_content_with_resolution_hook(&link, "new intended", |_| {
        std::fs::remove_file(&link)?;
        symlink(&replacement, &link)
    })
    .unwrap();

    assert_eq!(std::fs::read(&intended).unwrap(), b"new intended");
    assert_eq!(std::fs::read(&replacement).unwrap(), b"stable replacement");
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
