//! HERDR environment variable capture.
//!
//! When jcode runs inside a HERDR pane, HERDR injects these variables:
//! - `HERDR_ENV=1` — signals HERDR is active
//! - `HERDR_PANE_ID` — pane identifier (e.g. `w1:p1`)
//! - `HERDR_SOCKET_PATH` — path to HERDR's Unix domain socket
//! - `HERDR_BIN_PATH` — path to the HERDR CLI binary
//! - `HERDR_WORKSPACE_ID` — workspace identifier
//! - `HERDR_TAB_ID` — tab identifier
//!
//! herdr-core (HC) is a separate runtime that injects its own variables:
//! - `HC_TERMINAL_ID` — this terminal's address (presence means HC is active)
//! - `HC_SOCKET` — path to the HC socket
//! - `HC_SESSION` — session namespace
//!
//! `HC_INSTALL_DIR` and `HC_DRAFT_MS` exist too, but a client does not need
//! them, so they are deliberately not captured.

use std::path::PathBuf;

/// Which terminal runtime this process is running inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    /// Neither legacy HERDR nor herdr-core was detected.
    None,
    /// Legacy HERDR (`HERDR_ENV=1` + `HERDR_PANE_ID` + `HERDR_SOCKET_PATH`).
    LegacyHerdr,
    /// herdr-core (`HC_TERMINAL_ID` set; endpoint from `HC_SOCKET`).
    Hc,
}

/// Captured HERDR environment. All fields are `None` when not running
/// inside a HERDR pane.
#[derive(Debug, Clone)]
pub struct HerdrEnv {
    /// Whether HERDR is active (`HERDR_ENV=1`).
    pub active: bool,
    /// Pane identifier (e.g. `w1:p1`).
    pub pane_id: Option<String>,
    /// Path to HERDR's Unix domain socket.
    pub socket_path: Option<PathBuf>,
    /// Path to the HERDR CLI binary.
    pub bin_path: Option<PathBuf>,
    /// Workspace identifier.
    pub workspace_id: Option<String>,
    /// Tab identifier.
    pub tab_id: Option<String>,
    /// HC terminal address (`HC_TERMINAL_ID`). Present only under HC.
    pub hc_terminal_id: Option<String>,
    /// HC socket path (`HC_SOCKET`). Present only under HC.
    pub hc_socket: Option<PathBuf>,
    /// HC session namespace (`HC_SESSION`).
    pub hc_session: Option<String>,
}

impl HerdrEnv {
    /// Capture HERDR environment from process environment variables.
    pub fn capture() -> Self {
        Self::capture_from(|key| std::env::var(key).ok())
    }

    /// Capture the environment through an arbitrary lookup, so tests can
    /// supply a fake environment without mutating the process.
    pub fn capture_from(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let get = |key: &str| lookup(key).filter(|value| !value.is_empty());
        let active = get("HERDR_ENV").as_deref() == Some("1");
        Self {
            active,
            pane_id: get("HERDR_PANE_ID"),
            socket_path: get("HERDR_SOCKET_PATH").map(PathBuf::from),
            bin_path: get("HERDR_BIN_PATH").map(PathBuf::from),
            workspace_id: get("HERDR_WORKSPACE_ID"),
            tab_id: get("HERDR_TAB_ID"),
            hc_terminal_id: get("HC_TERMINAL_ID"),
            hc_socket: get("HC_SOCKET").map(PathBuf::from),
            hc_session: get("HC_SESSION"),
        }
    }

    /// Claim a legacy HERDR runtime without touching the process environment.
    ///
    /// This is what the `--herdr` force flag uses: it marks the captured
    /// environment active even when `HERDR_ENV` is unset. It does not invent a
    /// pane identity, so a forced environment with no `HERDR_PANE_ID` and no
    /// `HERDR_SOCKET_PATH` still fails [`is_valid`](Self::is_valid) and stays
    /// silent — the flag selects a runtime, it cannot make one exist. Faking
    /// one by setting `HERDR_ENV=1` in the process instead lied to every other
    /// reader of that variable, including `jcode-tui-mermaid`'s
    /// directory-hiding check and every child process spawned afterwards.
    pub fn with_forced_active(mut self) -> Self {
        self.active = true;
        self
    }

    /// Which runtime this environment belongs to. HC wins when both are
    /// present: inside HC a stale legacy variable must not shadow it.
    pub fn kind(&self) -> RuntimeKind {
        if self.hc_terminal_id.is_some() {
            RuntimeKind::Hc
        } else if self.active {
            RuntimeKind::LegacyHerdr
        } else {
            RuntimeKind::None
        }
    }

    /// Returns `true` if running inside a terminal runtime with valid
    /// identity and a usable endpoint. True for both legacy HERDR and HC.
    pub fn is_valid(&self) -> bool {
        match self.kind() {
            RuntimeKind::Hc => self.hc_socket.is_some(),
            RuntimeKind::LegacyHerdr => self.pane_id.is_some() && self.socket_path.is_some(),
            RuntimeKind::None => false,
        }
    }

    /// The socket to talk to for the detected runtime, if any.
    pub fn endpoint(&self) -> Option<&PathBuf> {
        match self.kind() {
            RuntimeKind::Hc => self.hc_socket.as_ref(),
            RuntimeKind::LegacyHerdr => self.socket_path.as_ref(),
            RuntimeKind::None => None,
        }
    }

    /// HC terminal address (`HC_TERMINAL_ID`), or `None` outside HC.
    pub fn hc_terminal_id(&self) -> Option<&str> {
        self.hc_terminal_id.as_deref()
    }

    /// HC socket path (`HC_SOCKET`), or `None` outside HC.
    pub fn hc_socket(&self) -> Option<&PathBuf> {
        self.hc_socket.as_ref()
    }

    /// HC session namespace (`HC_SESSION`), or `None` when unset.
    pub fn hc_session(&self) -> Option<&str> {
        self.hc_session.as_deref()
    }

    /// Returns the pane ID, or `None` if not in a HERDR pane. Callers should
    /// gate behind [`Self::is_valid`] to preserve the previous
    /// precondition-on-call semantics; this method no longer panics.
    pub fn pane_id(&self) -> Option<&str> {
        self.pane_id.as_deref()
    }

    /// Returns the socket path, or `None` if not in a HERDR pane. Callers
    /// should gate behind [`Self::is_valid`]; this method no longer panics.
    pub fn socket_path(&self) -> Option<&PathBuf> {
        self.socket_path.as_ref()
    }

    /// Convenience accessor for callers that have already checked
    /// [`Self::is_active`]. Returns an empty string when the pane ID is
    /// absent, which should not occur post-guard but keeps the call panic
    /// free.
    pub fn pane_id_or_empty(&self) -> &str {
        self.pane_id.as_deref().unwrap_or("")
    }

    /// Convenience accessor for callers that have already checked
    /// [`Self::is_active`]. Returns an empty path when the socket path is
    /// absent; this fallback is only reached when the active guard is
    /// bypassed.
    pub fn socket_path_or_default(&self) -> &PathBuf {
        static EMPTY_PATHBUF: PathBuf = PathBuf::new();
        self.socket_path.as_ref().unwrap_or(&EMPTY_PATHBUF)
    }
}

impl Default for HerdrEnv {
    fn default() -> Self {
        Self::capture()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn capture_inactive_when_no_env() {
        let _env_lock = crate::env::test_support::lock_test_env();
        unsafe {
            env::remove_var("HERDR_ENV");
            env::remove_var("HERDR_PANE_ID");
        }
        let env = HerdrEnv::capture();
        assert!(!env.active);
        assert!(!env.is_valid());
    }

    #[test]
    fn capture_active_with_valid_pane() {
        let _env_lock = crate::env::test_support::lock_test_env();
        // Save existing values to restore after test.
        let saved_env = std::env::var("HERDR_ENV").ok();
        let saved_pane = std::env::var("HERDR_PANE_ID").ok();
        let saved_sock = std::env::var("HERDR_SOCKET_PATH").ok();

        unsafe {
            env::set_var("HERDR_ENV", "1");
            env::set_var("HERDR_PANE_ID", "w1:p1");
            env::set_var("HERDR_SOCKET_PATH", "/tmp/herdr.sock");
        }
        let env = HerdrEnv::capture();
        assert!(env.active);
        assert!(env.is_valid());
        assert_eq!(env.pane_id(), Some("w1:p1"));
        assert_eq!(env.socket_path(), Some(&PathBuf::from("/tmp/herdr.sock")));

        // Restore original values.
        match saved_env {
            Some(v) => unsafe { env::set_var("HERDR_ENV", v) },
            None => unsafe { env::remove_var("HERDR_ENV") },
        }
        match saved_pane {
            Some(v) => unsafe { env::set_var("HERDR_PANE_ID", v) },
            None => unsafe { env::remove_var("HERDR_PANE_ID") },
        }
        match saved_sock {
            Some(v) => unsafe { env::set_var("HERDR_SOCKET_PATH", v) },
            None => unsafe { env::remove_var("HERDR_SOCKET_PATH") },
        }
    }

    #[test]
    fn capture_inactive_when_wrong_value() {
        let _env_lock = crate::env::test_support::lock_test_env();
        unsafe {
            env::set_var("HERDR_ENV", "0");
        }
        let env = HerdrEnv::capture();
        assert!(!env.active);
        unsafe {
            env::remove_var("HERDR_ENV");
        }
    }

    fn lookup_from(
        pairs: &'static [(&'static str, &'static str)],
    ) -> impl Fn(&str) -> Option<String> {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_string())
        }
    }

    #[test]
    fn capture_hc_when_terminal_id_set() {
        let env = HerdrEnv::capture_from(lookup_from(&[
            ("HC_TERMINAL_ID", "t7"),
            ("HC_SOCKET", "/tmp/hc.sock"),
            ("HC_SESSION", "work"),
        ]));
        assert_eq!(env.kind(), RuntimeKind::Hc);
        assert!(env.is_valid());
        assert_eq!(env.hc_terminal_id(), Some("t7"));
        assert_eq!(env.hc_socket(), Some(&PathBuf::from("/tmp/hc.sock")));
        assert_eq!(env.hc_session(), Some("work"));
        assert_eq!(env.endpoint(), Some(&PathBuf::from("/tmp/hc.sock")));
        assert!(!env.active);
        assert_eq!(env.pane_id(), None);
    }

    #[test]
    fn hc_without_socket_is_not_valid() {
        let env = HerdrEnv::capture_from(lookup_from(&[("HC_TERMINAL_ID", "t7")]));
        assert_eq!(env.kind(), RuntimeKind::Hc);
        assert!(!env.is_valid());
        assert_eq!(env.endpoint(), None);
    }

    #[test]
    fn legacy_detection_still_true_alongside_absent_hc() {
        let env = HerdrEnv::capture_from(lookup_from(&[
            ("HERDR_ENV", "1"),
            ("HERDR_PANE_ID", "w1:p1"),
            ("HERDR_SOCKET_PATH", "/tmp/herdr.sock"),
        ]));
        assert_eq!(env.kind(), RuntimeKind::LegacyHerdr);
        assert!(env.is_valid());
        assert_eq!(env.endpoint(), Some(&PathBuf::from("/tmp/herdr.sock")));
        assert_eq!(env.hc_terminal_id(), None);
    }

    #[test]
    fn hc_wins_when_both_runtimes_present() {
        let env = HerdrEnv::capture_from(lookup_from(&[
            ("HERDR_ENV", "1"),
            ("HERDR_PANE_ID", "w1:p1"),
            ("HERDR_SOCKET_PATH", "/tmp/herdr.sock"),
            ("HC_TERMINAL_ID", "t7"),
            ("HC_SOCKET", "/tmp/hc.sock"),
        ]));
        assert_eq!(env.kind(), RuntimeKind::Hc);
        assert_eq!(env.endpoint(), Some(&PathBuf::from("/tmp/hc.sock")));
    }

    #[test]
    fn no_runtime_detected() {
        let env = HerdrEnv::capture_from(lookup_from(&[]));
        assert_eq!(env.kind(), RuntimeKind::None);
        assert!(!env.is_valid());
        assert_eq!(env.endpoint(), None);
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// Process-wide lock serializing tests that mutate `HERDR_*`
    /// environment variables. Mutating process env under libtest's
    /// parallel workers is undefined behavior (data race): it has
    /// been observed to hang the libtest console waiting for
    /// CompletedTest events and to fail assertions on captured
    /// values. Every test that touches `HERDR_*` keys must hold
    /// this guard for its entire body.
    pub(crate) fn lock_test_env() -> MutexGuard<'static, ()> {
        static TEST_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        TEST_ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
