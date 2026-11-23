//! Windows-only regression tests for background-command termination.
//!
//! `bash_tests.rs` is gated `#[cfg(all(test, not(windows)))]` because most of its fixtures are unix
//! shells, so Windows coverage has to live in its own module. Anything added here must stay valid
//! on Windows only: it will not be compiled or run on unix.

#![allow(clippy::await_holding_lock)]

use super::*;
use crate::bus::BackgroundTaskStatus;
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
/// The nested shell pings for ~5s before writing `sentinel`. If the timeout kills only the leader,
/// the nested shell keeps running and the sentinel appears a few seconds later.
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
            "@echo off\r\nping -n 6 127.0.0.1 >nul\r\necho leaked>\"{}\"\r\n",
            sentinel.display()
        ),
    )
    .expect("write child batch");
    std::fs::write(
        &leader_batch,
        format!(
            "@echo off\r\nstart \"\" /b \"{}\"\r\nping -n {leader_pings} 127.0.0.1 >nul\r\n",
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

// Regression: a timed-out background command must tear down cmd.exe's descendants, not just the
// direct child. The leader spawns a nested shell that writes a sentinel after ~5s; killing only the
// leader leaves that shell alive and the sentinel appears.
#[tokio::test]
async fn test_background_timeout_kills_descendant_tree() {
    let dir = tempfile::tempdir().expect("temp dir");
    let sentinel = dir.path().join("leaked.txt");
    let leader_batch = write_descendant_tree_fixture(dir.path(), &sentinel, 30);

    let tool = BashTool::new();
    let task_id = start_background(&tool, format!("call \"{}\"", leader_batch.display()), Some(300)).await;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut timed_out = false;
    while std::time::Instant::now() < deadline {
        let status = crate::background::global()
            .status(&task_id)
            .await
            .expect("status should exist");
        if status.status == BackgroundTaskStatus::Failed {
            timed_out = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(timed_out, "background task should fail after timeout");

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
#[tokio::test]
async fn test_background_descendant_sentinel_negative_control() {
    let dir = tempfile::tempdir().expect("temp dir");
    let sentinel = dir.path().join("leaked.txt");
    let leader_batch = write_descendant_tree_fixture(dir.path(), &sentinel, 10);

    let tool = BashTool::new();
    let task_id = start_background(&tool, format!("call \"{}\"", leader_batch.display()), None).await;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(25);
    while std::time::Instant::now() < deadline {
        if sentinel.exists() {
            let _ = crate::background::global().cancel(&task_id).await;
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let _ = crate::background::global().cancel(&task_id).await;
    panic!(
        "control fixture never wrote its sentinel; the regression test cannot detect orphan survival"
    );
}
