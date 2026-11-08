use super::*;
use std::sync::MutexGuard;

struct EnvRestore {
    home: Option<std::ffi::OsString>,
    namespace: Option<std::ffi::OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        unsafe {
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

fn dev_home() -> (tempfile::TempDir, EnvRestore) {
    let lock = crate::IMAGE_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let restore = EnvRestore {
        home: std::env::var_os("KCODE_HOME"),
        namespace: std::env::var_os("KCODE_DEV_NAMESPACE"),
        _lock: lock,
    };
    let home = tempfile::tempdir().expect("dev home");
    unsafe {
        std::env::set_var("KCODE_HOME", home.path());
        std::env::set_var("KCODE_DEV_NAMESPACE", "1");
    }
    (home, restore)
}

#[cfg(unix)]
#[test]
fn cache_enumeration_skips_symlinked_png_without_touching_target() {
    use std::os::unix::fs::symlink;

    const HASH: u64 = 0x7a11_1eaf;
    let (home, _restore) = dev_home();
    let cache_dir = home.path().join("cache/mermaid");
    std::fs::create_dir_all(&cache_dir).expect("cache directory");
    let stable = tempfile::NamedTempFile::new().expect("stable sentinel");
    std::fs::write(stable.path(), b"stable-mermaid-cache").expect("write sentinel");
    symlink(
        stable.path(),
        cache_dir.join(format!("{HASH:016x}_w100.png")),
    )
    .expect("PNG cache symlink");
    let cache = MermaidCache {
        entries: HashMap::new(),
        order: VecDeque::new(),
        cache_dir,
        width_miss_floor: HashMap::new(),
    };

    assert!(cache.discover_on_disk(HASH, None, None).is_none());
    assert_eq!(
        std::fs::read(stable.path()).expect("read sentinel"),
        b"stable-mermaid-cache"
    );
}
