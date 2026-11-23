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

/// Returns the PIDs of live processes whose command line contains `needle`.
///
/// `needle` is the fixture's own `child.cmd` path, which is unique per test, so this cannot match an
/// unrelated process. Only the nested shell's command line carries that path; its `PING.EXE` child
/// does not.
fn find_processes_by_command_line(needle: &str) -> Vec<u32> {
    let script = format!(
        "Get-CimInstance Win32_Process | Where-Object {{ $_.CommandLine -and \
         $_.CommandLine.Contains('{needle}') }} | ForEach-Object {{ $_.ProcessId }}"
    );
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .expect("query processes by command line");
    assert!(
        output.status.success(),
        "process query failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect()
}

/// Waits on the kernel handle for `pid` to exit, rather than sleeping a fixed guess.
///
/// `Ok(true)` means the process is gone: it exited inside the budget, or the handle could not be
/// opened because it had already exited. `Ok(false)` means it is still running.
fn wait_for_process_exit(pid: u32, budget: std::time::Duration) -> std::io::Result<bool> {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };

    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return Ok(true);
    }
    let budget_ms = budget.as_millis().min(u128::from(u32::MAX)) as u32;
    let result = unsafe { WaitForSingleObject(handle, budget_ms) };
    unsafe { CloseHandle(handle) };
    match result {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        other => Err(std::io::Error::other(format!(
            "WaitForSingleObject returned {other}"
        ))),
    }
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
    let child_batch = dir.path().join("child.cmd");
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
    let descendant_pids = find_processes_by_command_line(&child_batch.to_string_lossy());
    assert!(
        !descendant_pids.is_empty(),
        "the descendant announced itself after {started_after:?} but no process matches {}, so its \
         exit cannot be observed",
        child_batch.display()
    );
    eprintln!("[policy] descendant started after {started_after:?}");

    super::kill_timed_out_command(&mut child);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(10), child.wait()).await;

    for pid in &descendant_pids {
        let exited = wait_for_process_exit(*pid, std::time::Duration::from_secs(15))
            .expect("query descendant process exit");
        eprintln!("[policy] descendant PID {pid} exited={exited}");
        assert!(
            exited,
            "descendant PID {pid} was still running 15s after kill_timed_out_command"
        );
    }
    // The descendant writes the sentinel before it exits, so checking it after its exit is
    // conclusive and needs no fixed delay.
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
    let child_batch = dir.path().join("child.cmd");
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

    let started_after = poll_for(std::time::Duration::from_secs(20), || async {
        ready.exists()
    })
    .await
    .unwrap_or_else(|| {
        panic!("the descendant never started, so the timeout below cannot prove a tree kill")
    });
    let descendant_pids = find_processes_by_command_line(&child_batch.to_string_lossy());
    assert!(
        !descendant_pids.is_empty(),
        "the descendant announced itself after {started_after:?} but no process matches {}, so its \
         exit cannot be observed",
        child_batch.display()
    );

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
        "[treekill] task reported Failed after {failed_after:?}; descendant PIDs {descendant_pids:?}; \
         sentinel present={}",
        sentinel.exists()
    );

    for pid in &descendant_pids {
        let exited = wait_for_process_exit(*pid, std::time::Duration::from_secs(15))
            .expect("query descendant process exit");
        eprintln!("[treekill] descendant PID {pid} exited={exited}");
        assert!(
            exited,
            "descendant PID {pid} survived the background timeout tree kill (task failed after \
             {failed_after:?}): only the direct child was terminated"
        );
    }
    // The descendant writes the sentinel before it exits, so checking it after its exit is
    // conclusive and needs no fixed delay.
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
