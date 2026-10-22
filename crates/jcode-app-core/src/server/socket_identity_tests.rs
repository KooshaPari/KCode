//! Regression tests for the shared socket namespace and the daemon
//! identity scrub applied when spawning a server.
//!
//! Split out of `socket.rs` to keep that module within the file-size limit.

use super::socket::{
    INHERITED_PROVIDER_IDENTITY_VARS, default_socket_path, scrub_inherited_provider_identity,
    socket_path,
};
use std::path::PathBuf;

mod identity_scrub_tests {

    const PRESERVED_DAEMON_CONFIG: &[&str] = &[
        "KCODE_OPENROUTER_API_BASE",
        "KCODE_OPENROUTER_API_KEY_NAME",
        "KCODE_OPENROUTER_ENV_FILE",
        "KCODE_OPENROUTER_TRANSPORT_STATE",
        "KCODE_OPENROUTER_PROVIDER_FEATURES",
        "KCODE_SCRATCH_DIR",
        "KCODE_DEFERRED_AUTH_BOOTSTRAP",
        "KCODE_RELOAD_FAST_START",
        "KCODE_READY_FD",
    ];

    /// `Command::get_envs` reports only the delta from the parent environment,
    /// so this asserts the scrub records a removal for exactly the identity
    /// variables and leaves genuine daemon configuration untouched.
    #[test]
    fn scrub_removes_identity_and_keeps_daemon_config() {
        let mut cmd = std::process::Command::new("jcode");
        for var in INHERITED_PROVIDER_IDENTITY_VARS {
            cmd.env(var, "inherited");
        }
        for var in PRESERVED_DAEMON_CONFIG {
            cmd.env(var, "kept");
        }

        scrub_inherited_provider_identity(&mut cmd);

        let delta: std::collections::HashMap<String, Option<String>> = cmd
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect();

        for var in INHERITED_PROVIDER_IDENTITY_VARS {
            assert_eq!(
                delta.get(*var),
                Some(&None),
                "{var} must be removed from the server child env"
            );
        }
        for var in PRESERVED_DAEMON_CONFIG {
            assert_eq!(
                delta.get(*var).and_then(|value| value.clone()),
                Some("kept".to_string()),
                "{var} must survive the scrub"
            );
        }
    }

    /// Guards against a new identity variable being introduced later and
    /// silently reintroducing the inheritance bug.
    #[test]
    fn scrub_covers_every_documented_identity_var() {
        for var in [
            "KCODE_RUNTIME_PROVIDER",
            "KCODE_ACTIVE_PROVIDER",
            "KCODE_INITIAL_PROVIDER_EXPLICIT",
            "KCODE_RESUMING",
            "KCODE_SESSION_ID",
        ] {
            assert!(
                INHERITED_PROVIDER_IDENTITY_VARS.contains(&var),
                "{var} must be scrubbed from the server child env"
            );
        }
    }
}

mod shared_socket_path_tests {
    use std::ffi::OsStr;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// Environment mutation is process-global, so these tests serialize.
    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Restores the named variable to its prior value (or absence) on drop.
    struct VarGuard {
        key: &'static str,
        prior: Option<std::ffi::OsString>,
    }

    impl VarGuard {
        fn set(key: &'static str, value: &OsStr) -> Self {
            let prior = std::env::var_os(key);
            // `crate::env` centralizes the Rust 2024 `unsafe` requirement for
            // process-global environment mutation.
            crate::env::set_var(key, value);
            Self { key, prior }
        }
    }

    impl Drop for VarGuard {
        fn drop(&mut self) {
            match self.prior.take() {
                Some(prior) => crate::env::set_var(self.key, prior),
                None => crate::env::remove_var(self.key),
            }
        }
    }

    /// The shared socket must resolve identically no matter which `TMPDIR` the
    /// launcher happens to inherit.
    ///
    /// `default_socket_path()` used to go through `runtime_dir()`, which on
    /// macOS returns `$TMPDIR`. A Herdr pane and a shell could therefore dial
    /// two different sockets, and the client silently attached to whichever stale
    /// daemon its `TMPDIR` named, ignoring the provider it had explicitly asked
    /// for.
    #[test]
    fn shared_socket_path_ignores_tmpdir() {
        let _lock = env_lock();
        let home = std::env::temp_dir().join("jcode-socket-home-test");
        std::fs::create_dir_all(&home).expect("create fake home");
        let _home = VarGuard::set("KCODE_HOME", home.as_os_str());
        // An explicit override would mask the default we are exercising here.
        std::env::remove_var("KCODE_SOCKET");

        let first = default_socket_path();
        let _tmp = VarGuard::set("TMPDIR", OsStr::new("/nonexistent-tmp-a"));
        let second = default_socket_path();
        let _tmp = VarGuard::set("TMPDIR", OsStr::new("/nonexistent-tmp-b"));
        let third = default_socket_path();

        assert_eq!(first, second, "TMPDIR must not move the shared socket");
        assert_eq!(second, third);
        assert_eq!(
            first,
            home.join("run").join("jcode.sock"),
            "the shared socket lives under a stable per-user directory"
        );
    }

    /// An explicit `--socket`/`KCODE_SOCKET` still wins over the shared default.
    #[test]
    fn explicit_socket_override_still_wins() {
        let _lock = env_lock();
        let _socket = VarGuard::set("KCODE_SOCKET", OsStr::new("/tmp/custom-jcode.sock"));
        assert_eq!(socket_path(), PathBuf::from("/tmp/custom-jcode.sock"));
    }
}
