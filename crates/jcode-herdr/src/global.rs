//! Process-global HERDR reporter.
//!
//! One reporter per process, one `OnceLock` behind it, owned by the crate that
//! owns the reporter. Every reporter of lifecycle state — the binary and the
//! TUI — reaches the same instance, so a state push cannot land on a copy
//! nothing ever initialized.
//!
//! Initialization is explicit: [`init`] or [`init_with_env`] runs once at
//! startup. Before that, and when no terminal runtime is detected, every
//! function here is a no-op.
//!
//! Nothing in this module reads or writes the process environment except
//! [`init`], which reads it once through [`HerdrEnv::capture`]. Callers that
//! need a runtime the process is not actually running inside pass a
//! synthesized [`HerdrEnv`] to [`init_with_env`] instead of mutating global
//! state.

use crate::{AgentState, HerdrEnv, HerdrReporter};
use std::sync::OnceLock;
use tokio::sync::Mutex;

static REPORTER: OnceLock<Mutex<Option<HerdrReporter>>> = OnceLock::new();

/// Initialize the global reporter from the process environment.
///
/// Safe to call more than once; only the first call takes effect.
pub fn init(agent_label: &str) {
    init_with_env(agent_label, HerdrEnv::capture());
}

/// Initialize the global reporter from an explicitly captured environment.
///
/// Never touches the process environment. Use this to probe a runtime that is
/// not present (the `--herdr` force flag) or to drive the reporter from a test
/// without racing every other thread's `getenv`.
pub fn init_with_env(agent_label: &str, env: HerdrEnv) {
    let reporter = Mutex::new(Some(HerdrReporter::from_env(agent_label, env)));
    if REPORTER.set(reporter).is_err() {
        tracing::debug!("HERDR reporter already initialized; keeping the first instance");
    }
}

/// Report a state transition. No-op until [`init`]/[`init_with_env`] runs, or
/// when no terminal runtime is present.
pub async fn report_state(state: AgentState) {
    if let Some(m) = REPORTER.get() {
        let guard = m.lock().await;
        if let Some(reporter) = guard.as_ref() {
            reporter.set_state(state).await;
        }
    }
}

/// Report the session identity a terminal can restore.
pub async fn report_session_id(session_id: String) {
    if let Some(m) = REPORTER.get() {
        let guard = m.lock().await;
        if let Some(reporter) = guard.as_ref() {
            reporter.set_session_id(session_id).await;
        }
    }
}

/// Send the initial idle report on session start.
pub async fn on_session_start() {
    if let Some(m) = REPORTER.get() {
        let guard = m.lock().await;
        if let Some(reporter) = guard.as_ref() {
            reporter.on_session_start().await;
        }
    }
}

/// Release the agent and shut the reporter down.
pub async fn shutdown() {
    if let Some(m) = REPORTER.get() {
        let mut guard = m.lock().await;
        if let Some(reporter) = guard.take() {
            reporter.release().await;
        }
    }
}

/// `true` when a terminal runtime is present and the reporter is initialized.
///
/// Non-blocking: returns `false` rather than waiting if the reporter is busy.
pub fn is_active() -> bool {
    let Some(holder) = REPORTER.get() else {
        return false;
    };
    let Ok(guard) = holder.try_lock() else {
        return false;
    };
    guard.as_ref().is_some_and(HerdrReporter::is_active)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RuntimeKind;
    use std::path::{Path, PathBuf};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    /// Collect the JSON-RPC frames a reporter sends to a mock socket.
    async fn start_mock_server() -> (
        PathBuf,
        tokio::sync::mpsc::Receiver<serde_json::Value>,
        tempfile::TempDir,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("global.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        let (tx, rx) = tokio::sync::mpsc::channel(64);

        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let tx = tx.clone();
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut byte = [0u8; 1];
                    loop {
                        match stream.read(&mut byte).await {
                            Ok(0) | Err(_) => return,
                            Ok(_) => {}
                        }
                        if byte[0] == b'\n' {
                            break;
                        }
                        buf.push(byte[0]);
                    }
                    if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&buf) {
                        let _ = tx.send(val).await;
                    }
                    let resp = serde_json::json!({"ok": true}).to_string();
                    let _ = stream.write_all(format!("{resp}\n").as_bytes()).await;
                });
            }
        });

        (sock, rx, dir)
    }

    fn legacy_env(sock: &Path) -> HerdrEnv {
        HerdrEnv {
            active: true,
            pane_id: Some("w1:p1".to_string()),
            socket_path: Some(sock.to_path_buf()),
            bin_path: None,
            workspace_id: None,
            tab_id: None,
            hc_terminal_id: None,
            hc_socket: None,
            hc_session: None,
            hc_scope: None,
        }
    }

    /// The global reporter must be reachable, and a state push through it must
    /// actually reach the socket. This is the check the duplicated TUI wrapper
    /// could never pass: its `OnceLock` had no initializer, so its six
    /// `report_state` calls were inert.
    ///
    /// One test exercises the process-global `OnceLock` because it can only be
    /// initialized once per process.
    #[tokio::test]
    async fn global_reporter_delivers_state_to_the_socket() {
        let (sock, mut rx, _dir) = start_mock_server().await;
        let env = legacy_env(&sock);
        assert_eq!(env.kind(), RuntimeKind::LegacyHerdr);

        // No process-env mutation anywhere: the environment is passed in.
        init_with_env("global-test", env);
        assert!(is_active(), "initialized legacy reporter should be active");

        report_state(AgentState::Working).await;

        let frame = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("state frame should arrive")
            .expect("frame channel open");
        assert_eq!(frame["method"], "pane.report_agent");
        assert_eq!(frame["params"]["state"], "working");

        // Cleanly release so later tests in this process see a stopped reporter.
        shutdown().await;
        assert!(!is_active(), "released reporter should not stay active");
    }

    /// An HC environment must never push frames, and initialization must not
    /// panic or block on the absent verb.
    #[tokio::test]
    async fn global_reporter_is_inert_under_hc() {
        let (sock, _rx, _dir) = start_mock_server().await;
        let env = HerdrEnv {
            active: false,
            pane_id: None,
            socket_path: None,
            bin_path: None,
            workspace_id: None,
            tab_id: None,
            hc_terminal_id: Some("t1".to_string()),
            hc_socket: Some(sock),
            hc_session: None,
            hc_scope: None,
        };
        assert_eq!(env.kind(), RuntimeKind::Hc);

        // `OnceLock` is already set by the test above; the point here is that
        // classification, not initialization order, decides delivery.
        assert!(!HerdrReporter::from_env("hc-test", env).is_push_capable());
    }
}
