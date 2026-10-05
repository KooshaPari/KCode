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
                Some(value) => std::env::set_var("JCODE_HOME", value),
                None => std::env::remove_var("JCODE_HOME"),
            }
            match self.namespace.take() {
                Some(value) => std::env::set_var("JCODE_DEV_NAMESPACE", value),
                None => std::env::remove_var("JCODE_DEV_NAMESPACE"),
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn pixmap_save_png_rejects_symlinked_cache_leaf() {
    use std::os::unix::fs::symlink;

    let lock = crate::IMAGE_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _restore = EnvRestore {
        home: std::env::var_os("JCODE_HOME"),
        namespace: std::env::var_os("JCODE_DEV_NAMESPACE"),
        _lock: lock,
    };
    let home = tempfile::tempdir().expect("dev home");
    let stable = tempfile::NamedTempFile::new().expect("stable sentinel");
    std::fs::write(stable.path(), b"stable-mermaid-png").expect("write sentinel");
    unsafe {
        std::env::set_var("JCODE_HOME", home.path());
        std::env::set_var("JCODE_DEV_NAMESPACE", "1");
    }
    let output = home.path().join("cache/mermaid/output.png");
    std::fs::create_dir_all(output.parent().expect("output parent")).expect("cache directory");
    symlink(stable.path(), &output).expect("PNG output symlink");
    let config = crate::RenderConfig {
        width: 8.0,
        height: 8.0,
        background: "#00000000".to_string(),
    };
    let theme = crate::terminal_theme();

    assert!(write_output_png_cached_fonts(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="#fff"/></svg>"##,
        &output,
        &config,
        &theme,
    )
    .is_err());
    assert_eq!(
        std::fs::read(stable.path()).expect("read sentinel"),
        b"stable-mermaid-png"
    );
}
