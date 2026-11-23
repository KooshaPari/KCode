//! TUI-side agent-state reporting for the herdr terminal multiplexer.
//!
//! This module is a call surface, not a transport. [`sync`] and [`release`]
//! forward to the process-wide reporter in [`jcode_herdr::global`], which is
//! the one owner of herdr state in this process and is initialized once at
//! startup (`src/cli/startup.rs`).
//!
//! It used to spawn `"$HERDR_BIN_PATH" pane report-agent ...` on its own
//! background thread, with its own env parse, sequence counter, command
//! timeout and resume-argv validator: a second transport speaking a protocol
//! `jcode-herdr` already speaks over herdr's socket. It also could not see
//! herdr-core, whose only marker is `HC_TERMINAL_ID`.
//!
//! The interactive TUI never blocks a turn on a user decision, so `blocked` is
//! never reported from here.
//!
//! See <https://herdr.dev/docs/add-herdr-support/>.

pub use jcode_herdr::AgentState;

/// The single authority for what this pane reports: a session is `working`
/// while a turn is processing, a background task is still running, or a
/// child/subagent session it spawned is still working. Everything else is
/// `idle`. Pure so the contract is testable without a live herdr pane.
pub fn resolve_agent_state(
    processing: bool,
    background_task_running: bool,
    child_agent_working: bool,
) -> AgentState {
    if processing || background_task_running || child_agent_working {
        AgentState::Working
    } else {
        AgentState::Idle
    }
}

/// Report this pane's state and, when known, the session a terminal can
/// resume, through the process-wide reporter.
///
/// Synchronous on purpose: every caller is a UI tick or a draw pass
/// (`app::terminal_title`), so a socket write must never run on the caller's
/// thread. The report is dispatched onto the runtime, where the crate reporter
/// owns dedup, heartbeats and sequencing.
///
/// A no-op outside herdr and outside a tokio runtime. Dropping a tick is safe
/// because the callers are level-triggered: the next tick re-evaluates the
/// state from scratch.
pub fn sync(state: AgentState, session_id: Option<&str>) {
    // Outside herdr there is nothing to report and nothing to spawn.
    if !jcode_herdr::global::is_active() {
        return;
    }
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let session_id = session_id.map(str::to_owned);
    handle.spawn(async move {
        jcode_herdr::global::report_state(state).await;
        if let Some(session_id) = session_id {
            jcode_herdr::global::report_session_id(session_id).await;
        }
    });
}

/// Release this pane because the user is quitting jcode.
///
/// Do not call this when jcode is about to exec itself (reload/update): the
/// same agent keeps running in the pane.
pub async fn release() {
    jcode_herdr::global::shutdown().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    #[test]
    fn processing_turn_is_working() {
        assert_eq!(resolve_agent_state(true, false, false), AgentState::Working);
    }

    #[test]
    fn running_background_task_is_working() {
        assert_eq!(resolve_agent_state(false, true, false), AgentState::Working);
    }

    #[test]
    fn live_child_agent_is_working() {
        assert_eq!(resolve_agent_state(false, false, true), AgentState::Working);
    }

    #[test]
    fn nothing_live_is_idle() {
        assert_eq!(resolve_agent_state(false, false, false), AgentState::Idle);
    }

    /// `sync` is called from UI ticks, which are synchronous and can outlive no
    /// runtime at all (a plain unit test, an early startup path). It must
    /// return rather than panic when no runtime is current.
    #[test]
    fn sync_without_a_runtime_is_a_no_op() {
        sync(AgentState::Working, Some("no-runtime"));
    }

    /// Collect the JSON-RPC frames a reporter sends to a mock socket.
    async fn start_mock_server() -> (
        PathBuf,
        tokio::sync::mpsc::Receiver<serde_json::Value>,
        tempfile::TempDir,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("tui.sock");
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

    fn legacy_env(sock: &Path) -> jcode_herdr::HerdrEnv {
        jcode_herdr::HerdrEnv {
            active: true,
            pane_id: Some("w1:p1".to_string()),
            socket_path: Some(sock.to_path_buf()),
            bin_path: None,
            workspace_id: None,
            tab_id: None,
            hc_terminal_id: None,
            hc_socket: None,
            hc_session: None,
        }
    }

    /// Liveness proof for the TUI path: a state report originating at this
    /// module's entry point must land on herdr's socket through the canonical
    /// reporter.
    ///
    /// The deleted transport cannot pass this. It spawned
    /// `"$HERDR_BIN_PATH" pane report-agent ...`, so with no `HERDR_BIN_PATH`
    /// it produced no frame at all, and even with one it wrote to a child
    /// process rather than to the socket these assertions read.
    ///
    /// One test initializes the process-wide `OnceLock`, because it can only be
    /// initialized once per process.
    #[tokio::test]
    async fn tui_sync_and_release_reach_herdr_through_the_global_reporter() {
        let (sock, mut rx, _dir) = start_mock_server().await;
        jcode_herdr::global::init_with_env("jcode", legacy_env(&sock));
        assert!(
            jcode_herdr::global::is_active(),
            "an initialized legacy reporter should be active"
        );

        sync(AgentState::Working, Some("tui-session-1"));

        let state_frame = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("state frame should arrive")
            .expect("frame channel open");
        assert_eq!(state_frame["method"], "pane.report_agent");
        assert_eq!(state_frame["params"]["state"], "working");
        assert_eq!(state_frame["params"]["pane_id"], "w1:p1");

        let session_frame = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("session frame should arrive")
            .expect("frame channel open");
        assert_eq!(session_frame["method"], "pane.report_agent_session");
        assert_eq!(session_frame["params"]["agent_session_id"], "tui-session-1");

        release().await;

        let release_frame = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("release frame should arrive")
            .expect("frame channel open");
        assert_eq!(release_frame["method"], "pane.release_agent");
        assert!(
            !jcode_herdr::global::is_active(),
            "a released reporter must not stay active"
        );
    }
}
