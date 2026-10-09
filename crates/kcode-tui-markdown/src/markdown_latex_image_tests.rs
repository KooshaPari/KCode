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
    let lock = ENV_LOCK
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
fn cache_read_rejects_symlinked_artifact_without_touching_target() {
    use std::os::unix::fs::symlink;

    let (home, _restore) = dev_home();
    let stable = tempfile::NamedTempFile::new().expect("stable sentinel");
    std::fs::write(stable.path(), b"stable-latex-cache").expect("write sentinel");
    let path = home.path().join("cache/latex/artifact.png");
    std::fs::create_dir_all(path.parent().expect("cache parent")).expect("cache directory");
    symlink(stable.path(), &path).expect("cache symlink");

    assert!(load_cached_artifact(&path).is_err());
    assert_eq!(
        std::fs::read(stable.path()).expect("read sentinel"),
        b"stable-latex-cache"
    );
}

#[cfg(unix)]
#[test]
fn cache_temp_copy_rejects_symlinked_leaf_without_touching_target() {
    use std::os::unix::fs::symlink;

    let (home, _restore) = dev_home();
    let stable = tempfile::NamedTempFile::new().expect("stable sentinel");
    std::fs::write(stable.path(), b"stable-latex-temp").expect("write sentinel");
    let source = tempfile::NamedTempFile::new().expect("rendered image");
    std::fs::write(source.path(), b"new-rendered-image").expect("write rendered image");
    let destination = home.path().join("cache/latex/artifact.123.tmp");
    std::fs::create_dir_all(destination.parent().expect("cache parent")).expect("cache directory");
    symlink(stable.path(), &destination).expect("temporary cache symlink");

    assert!(copy_to_cache_temp(source.path(), &destination).is_err());
    assert_eq!(
        std::fs::read(stable.path()).expect("read sentinel"),
        b"stable-latex-temp"
    );
}
