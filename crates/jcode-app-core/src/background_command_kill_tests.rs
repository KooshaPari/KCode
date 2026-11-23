//! Windows-only regression tests for background-command termination.
//!
//! These live beside [`super::kill_timed_out_command`], not in `tool/bash_tests.rs`, because that
//! file is gated `#[cfg(all(test, not(windows)))]` (most of its fixtures are unix shells) and so is
//! never compiled on Windows. Anything added here must stay valid on Windows only.

#![allow(clippy::await_holding_lock)]

use crate::bus::BackgroundTaskStatus;
use crate::tool::ToolContext;
use crate::tool::bash::BashTool;
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
/// The nested shell waits ~5s before writing `sentinel`. If the timeout kills only the leader, the
/// nested shell keeps running and the sentinel appears a few seconds later.
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
    sentinel: &std::path::Path,
    leader_pings: u32,
) -> std::path::PathBuf {
    let child_batch = dir.join("child.cmd");
    let leader_batch = dir.join("leader.cmd");
    std::fs::write(
        &child_batch,
        format!(
            "@echo off\r\n%SystemRoot%\\System32\\PING.EXE -n 6 127.0.0.1 >nul\r\necho leaked>\"{}\"\r\n",
            sentinel.display()
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

// Regression: a timed-out background command must tear down cmd.exe's descendants, not just the
// direct child. The leader spawns a nested shell that writes a sentinel after ~5s; killing only the
// leader leaves that shell alive and the sentinel appears.
#[tokio::test]
async fn test_background_timeout_kills_descendant_tree() {
    let dir = tempfile::tempdir().expect("temp dir");
    let sentinel = dir.path().join("leaked.txt");
    let leader_batch = write_descendant_tree_fixture(dir.path(), &sentinel, 30);
    eprintln!("[treekill] fixture dir {}", dir.path().display());

    let tool = BashTool::new();
    let task_id = start_background(
        &tool,
        format!("call \"{}\"", leader_batch.display()),
        Some(300),
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

    // Long enough for an orphaned nested shell (5s ping) to write the sentinel.
    tokio::time::sleep(std::time::Duration::from_secs(9)).await;
    assert!(
        !sentinel.exists(),
        "descendant survived the background timeout tree kill: {}",
        sentinel.display()
    );
}

// Negative control for the regression above: the same fixture without a timeout must reach the
// sentinel write, proving the sentinel is a valid detector rather than an unreachable code path.
// It also asserts the sentinel arrives late enough that a timeout kill can pre-empt it.
#[tokio::test]
async fn test_background_descendant_sentinel_negative_control() {
    let dir = tempfile::tempdir().expect("temp dir");
    let sentinel = dir.path().join("leaked.txt");
    let leader_batch = write_descendant_tree_fixture(dir.path(), &sentinel, 10);
    eprintln!("[negctrl] fixture dir {}", dir.path().display());

    let tool = BashTool::new();
    let task_id =
        start_background(&tool, format!("call \"{}\"", leader_batch.display()), None).await;

    let wrote_after = poll_for(std::time::Duration::from_secs(25), || async {
        sentinel.exists()
    })
    .await;
    let _ = crate::background::global().cancel(&task_id).await;

    let wrote_after = wrote_after.unwrap_or_else(|| {
        panic!(
            "control fixture never wrote its sentinel; the regression test cannot detect orphan survival"
        )
    });
    eprintln!("[negctrl] sentinel written after {wrote_after:?}");
    assert!(
        wrote_after >= std::time::Duration::from_secs(3),
        "sentinel appeared after only {wrote_after:?}; the fixture is too fast for a 300ms timeout to \
         pre-empt it, so the regression test would pass vacuously"
    );
}
