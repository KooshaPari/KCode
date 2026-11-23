//! Windows-only regression tests for background-command termination.
//!
//! These live beside [`super::kill_timed_out_command`], not in `tool/bash_tests.rs`, because that
//! file is gated `#[cfg(all(test, not(windows)))]` (most of its fixtures are unix shells) and so is
//! never compiled on Windows. Anything added here must stay valid on Windows only.

#![allow(clippy::await_holding_lock)]

use crate::bus::BackgroundTaskStatus;
use crate::tool::bash::BashTool;
use crate::tool::{Tool, ToolContext};
use serde_json::json;

fn make_ctx() -> ToolContext {
    ToolContext {
        session_id: "test-session".to_string(),
        message_id: "test-msg".to_string(),
        tool_call_id: "test-call".to_string(),
        working_dir: Some(std::env::temp_dir()),
        stdin_request_tx: None,
        graceful_shutdown_signal: None,
        execution_mode: crate::tool::ToolExecutionMode::Direct,
    }
}

/// Writes a leader batch file that starts a nested `cmd.exe` and then waits.
///
/// The nested shell writes `ready` immediately, waits ~5s, then writes `sentinel`. `ready` is what
/// proves the descendant was alive before any timeout could fire: without it a test can pass because
/// the timeout beat the descendant into existence rather than because the tree was killed.
///
/// Two details keep the fixture honest on a developer machine:
///
/// * The nested shell is launched as an explicit `cmd.exe`, matching how `cmd` itself starts
///   `start /b` children. Handing `start` a bare `.cmd` path goes through shell association and can
///   re-parent the process, which would test something other than the leader's own tree.
/// * `ping` is invoked by absolute path. `C:\Program Files\Git\usr\bin` often precedes
///   `%SystemRoot%\System32` on `PATH`, and Git's `ping.exe` does not honour `-n`, so it returns
///   instantly and the wait collapses to nothing.
fn write_descendant_tree_fixture(
    dir: &std::path::Path,
    ready: &std::path::Path,
    sentinel: &std::path::Path,
    leader_pings: u32,
) -> std::path::PathBuf {
    let child_batch = dir.join("child.cmd");
    let leader_batch = dir.join("leader.cmd");
    std::fs::write(
        &child_batch,
        format!(
            "@echo off\r\necho started>\"{}\"\r\n%SystemRoot%\\System32\\PING.EXE -n 6 127.0.0.1 >nul\r\necho leaked>\"{}\"\r\n",
            ready.display(),
            sentinel.display(),
        ),
    )
    .expect("write child batch");
    std::fs::write(
        &leader_batch,
        format!(
            "@echo off\r\nstart \"\" /b cmd.exe /d /c \"\"{}\"\"\r\n%SystemRoot%\\System32\\PING.EXE -n {leader_pings} 127.0.0.1 >nul\r\n",
            child_batch.display()
        ),
    )
    .expect("write leader batch");
    leader_batch
}

/// Removes the output and status artifacts the global background manager keeps outside the test's
/// `TempDir`, so a run does not accumulate records in the shared `kcode-bg-tasks` directory.
fn cleanup_task_artifacts(task_id: &str) {
    let manager = crate::background::global();
    let _ = std::fs::remove_file(manager.output_path_for(task_id));
    let _ = std::fs::remove_file(manager.status_path_for(task_id));
}

async fn start_background(tool: &BashTool, command: String, timeout: Option<u64>) -> String {
    let mut params = json!({
        "command": command,
        "run_in_background": true,
        "notify": false,
        "wake": false,
    });
    if let Some(timeout) = timeout {
        params["timeout"] = json!(timeout);
    }
    let result = tool
        .execute(params, make_ctx())
        .await
        .expect("background command should start");
    result.metadata.expect("expected metadata")["task_id"]
        .as_str()
        .expect("task id should be present")
        .to_string()
}

/// Polls until `predicate` returns true, returning the elapsed time when it did.
async fn poll_for<F, Fut>(
    budget: std::time::Duration,
    mut predicate: F,
) -> Option<std::time::Duration>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();
    while start.elapsed() < budget {
        if predicate().await {
            return Some(start.elapsed());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    None
}

/// Proves the kill policy itself, without going through the tool's timeout plumbing.
///
/// The leader's descendant is confirmed alive through `ready` before the kill, so a pass cannot be
/// explained by the descendant never having started. The sentinel is written ~5s after `ready`, far
/// outside any scheduling jitter, so its absence afterwards is caused by the tree kill.
#[tokio::test]
async fn test_kill_timed_out_command_tears_down_descendant_tree() {
    let dir = tempfile::tempdir().expect("temp dir");
    let ready = dir.path().join("ready.txt");
    let sentinel = dir.path().join("leaked.txt");
    let leader_batch = write_descendant_tree_fixture(dir.path(), &ready, &sentinel, 30);
    eprintln!("[policy] fixture dir {}", dir.path().display());

    let mut child = tokio::process::Command::new("cmd.exe")
        .arg("/d")
        .arg("/c")
        .arg("call")
        .arg(&leader_batch)
        .current_dir(dir.path())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn leader");

    let started_after = poll_for(std::time::Duration::from_secs(20), || async {
        ready.exists()
    })
    .await
    .unwrap_or_else(|| panic!("descendant never started; the kill below would prove nothing"));
    assert!(
        !sentinel.exists(),
        "sentinel already present {started_after:?} after the descendant started"
    );
    eprintln!("[policy] descendant started after {started_after:?}");

    super::kill_timed_out_command(&mut child);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(10), child.wait()).await;

    // Long enough for the orphaned descendant's remaining ping to finish and write the sentinel.
    tokio::time::sleep(std::time::Duration::from_secs(9)).await;
    assert!(
        !sentinel.exists(),
        "descendant survived kill_timed_out_command: {}",
        sentinel.display()
    );
}

// Regression: a timed-out background command must tear down cmd.exe's descendants, not just the
// direct child. The leader spawns a nested shell that writes a sentinel after ~5s; killing only the
// leader leaves that shell alive and the sentinel appears.
#[tokio::test]
async fn test_background_timeout_kills_descendant_tree() {
    let dir = tempfile::tempdir().expect("temp dir");
    let ready = dir.path().join("ready.txt");
    let sentinel = dir.path().join("leaked.txt");
    let leader_batch = write_descendant_tree_fixture(dir.path(), &ready, &sentinel, 30);
    eprintln!("[treekill] fixture dir {}", dir.path().display());

    let tool = BashTool::new();
    let task_id = start_background(
        &tool,
        format!("call \"{}\"", leader_batch.display()),
        // Long enough that the descendant is certainly running when the timeout fires, short enough
        // that the sentinel write (~5s after the descendant starts) cannot happen first.
        Some(2000),
    )
    .await;

    let failed_after = poll_for(std::time::Duration::from_secs(30), || {
        let task_id = task_id.clone();
        async move {
            let status = crate::background::global()
                .status(&task_id)
                .await
                .expect("status should exist");
            status.status == BackgroundTaskStatus::Failed
        }
    })
    .await
    .expect("background task should fail after its timeout");
    eprintln!(
        "[treekill] task reported Failed after {failed_after:?}; sentinel present={}",
        sentinel.exists()
    );
    assert!(
        ready.exists(),
        "the descendant had not started when the timeout fired (task failed after {failed_after:?}), \
         so the timeout killed nothing and this test would pass even without the tree kill"
    );

    // Long enough for an orphaned nested shell (5s ping) to write the sentinel.
    tokio::time::sleep(std::time::Duration::from_secs(9)).await;
    assert!(
        !sentinel.exists(),
        "descendant survived the background timeout tree kill: {}",
        sentinel.display()
    );
    cleanup_task_artifacts(&task_id);
}

// Negative control for the regression above: the same fixture without a timeout must reach the
// sentinel write, proving the sentinel is a valid detector rather than an unreachable code path.
// It also asserts the sentinel arrives late enough that a timeout kill can pre-empt it.
#[tokio::test]
async fn test_background_descendant_sentinel_negative_control() {
    let dir = tempfile::tempdir().expect("temp dir");
    let ready = dir.path().join("ready.txt");
    let sentinel = dir.path().join("leaked.txt");
    let leader_batch = write_descendant_tree_fixture(dir.path(), &ready, &sentinel, 10);
    eprintln!("[negctrl] fixture dir {}", dir.path().display());

    let tool = BashTool::new();
    let task_id =
        start_background(&tool, format!("call \"{}\"", leader_batch.display()), None).await;

    let wrote_after = poll_for(std::time::Duration::from_secs(25), || async {
        sentinel.exists()
    })
    .await;
    let _ = crate::background::global().cancel(&task_id).await;
    cleanup_task_artifacts(&task_id);

    let wrote_after = wrote_after.unwrap_or_else(|| {
        panic!(
            "control fixture never wrote its sentinel; the regression test cannot detect orphan survival"
        )
    });
    eprintln!("[negctrl] sentinel written after {wrote_after:?}");
    assert!(
        ready.exists(),
        "control fixture never announced its descendant start"
    );
    assert!(
        wrote_after >= std::time::Duration::from_secs(3),
        "sentinel appeared after only {wrote_after:?}; the fixture is too fast for the 2s timeout to \
         pre-empt it, so the regression test would pass vacuously"
    );
}
