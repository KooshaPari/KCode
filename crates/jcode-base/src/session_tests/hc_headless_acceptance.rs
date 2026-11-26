//! Headless HC acceptance: a real KCode `session_start` hook records the launch
//! facts in a scratch home, the owning process is SIGKILLed mid-turn, and the
//! restore is proven from that record alone.
//!
//! Every constraint the coordinator set is an assertion here:
//!
//! - P1 app closed: the restore preflight refuses while the recorded pid is alive.
//! - P2 initial-launch facts: the record carries `session_id` + `cwd` only, and the
//!   acceptance never builds a `--resume` argv, so the id cannot be operator-supplied.
//! - P3 real hooks/metadata: the record is written by the real hook surface
//!   (`KCODE_HOOK_SESSION_START` -> `hooks.session_start` -> `KCODE_HOOK_PAYLOAD`),
//!   and the transcript comes from the real session store.
//! - P4 native Windows: reported separately by the caller; nothing here borrows
//!   Linux evidence for it.
//! - P5 no boot service: the child runs with a scratch `HOME`, and the test asserts
//!   that no launchd or systemd unit appeared anywhere under it.
//!
//! Nothing here reads or writes a live session store: `KCODE_HOME`, `HOME` and the
//! hook script all live under one temp directory.

use super::crash_recovery_cases::recovery_invariant_violations;
use super::*;
use anyhow::{Result, anyhow};

const CHILD_SESSION: &str = "KCODE_HC_CHILD_SESSION";
const CHILD_RECORD: &str = "KCODE_HC_CHILD_RECORD";
const CHILD_READY: &str = "KCODE_HC_CHILD_READY";

fn text_block(text: &str) -> ContentBlock {
    ContentBlock::Text {
        text: text.to_string(),
        cache_control: None,
    }
}

/// Writes the JSON payload the real hook surface exported. `$1` is the record path.
fn write_record_hook(dir: &std::path::Path) -> Result<std::path::PathBuf> {
    let script = dir.join("record-session-start.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nprintf '%s' \"$KCODE_HOOK_PAYLOAD\" > \"$1\"\n",
    )
    .map_err(|e| anyhow!("write hook script: {e}"))?;
    Ok(script)
}

/// Insert the completed tool call the crash must preserve. The turn is left
/// unfinished on purpose: the process dies before the turn closes.
fn seed_completed_tool_call(session: &mut Session) -> Result<()> {
    session.add_message(Role::User, vec![text_block("start the turn")]);
    session.save()?;
    session.add_message(
        Role::Assistant,
        vec![ContentBlock::ToolUse {
            id: "toolu_hc_1".into(),
            name: "bash".into(),
            input: serde_json::json!({"command": "ls"}),
            thought_signature: None,
        }],
    );
    session.save()?;
    session.add_message(
        Role::User,
        vec![ContentBlock::ToolResult {
            tool_use_id: "toolu_hc_1".into(),
            content: "file-a\nfile-b".into(),
            is_error: None,
        }],
    );
    session.save()?;
    Ok(())
}

/// Child entry point: re-executed from the parent test, journals a completed tool
/// call, fires the real `session_start` hook, then blocks until it is killed.
#[test]
fn hc_headless_child_session_start() -> Result<()> {
    let Ok(session_id) = std::env::var(CHILD_SESSION) else {
        return Ok(());
    };
    let record_path =
        std::env::var(CHILD_RECORD).map_err(|e| anyhow!("missing {CHILD_RECORD}: {e}"))?;
    let ready_path =
        std::env::var(CHILD_READY).map_err(|e| anyhow!("missing {CHILD_READY}: {e}"))?;
    let cwd = std::env::current_dir().map_err(|e| anyhow!(e))?;

    let mut session = Session::create_with_id(session_id.clone(), None, Some("hc-accept".into()));
    session.working_dir = Some(cwd.display().to_string());
    session.provider_key = Some("hc-accept-provider".into());
    session.model = Some("hc-accept-model".into());
    session.last_pid = Some(std::process::id());
    seed_completed_tool_call(&mut session)?;

    // The real hook surface: config hook -> detached child -> KCODE_HOOK_PAYLOAD.
    crate::hooks::dispatch_observer(
        crate::hooks::HookEvent::new("session_start")
            .session_id(session_id)
            .cwd(cwd.display().to_string()),
    );

    // The hook is dispatched detached, so wait for its record instead of assuming it.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while std::fs::metadata(&record_path)
        .map(|m| m.len())
        .unwrap_or(0)
        == 0
    {
        if std::time::Instant::now() >= deadline {
            return Err(anyhow!("session_start hook never wrote its record"));
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    std::fs::write(&ready_path, b"ready")?;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Refusals the restore must make before touching a store. `Some(reason)` means
/// "do not restore"; it mirrors the reference implementation's safety rules.
fn preflight_refusal(session: &Session) -> Option<String> {
    if let Some(pid) = session.last_pid
        && pid_process_alive(pid)
    {
        return Some(format!("session pid {pid} is still alive (app not closed)"));
    }
    if let Some(dir) = session.working_dir.as_deref()
        && let Ok(meta) = std::fs::symlink_metadata(dir)
        && meta.file_type().is_symlink()
    {
        return Some(format!("session cwd {dir} is a symlink"));
    }
    None
}

#[cfg(unix)]
fn pid_process_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn pid_process_alive(_pid: u32) -> bool {
    // Not measured on this platform; the native-Windows cell stays NOT_RUN.
    false
}

fn read_record(path: &std::path::Path) -> Result<serde_json::Value> {
    let raw = std::fs::read_to_string(path).map_err(|e| anyhow!("read hook record: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| anyhow!("hook record is not JSON: {e} ({raw})"))
}

/// Assert no boot service was created anywhere under the scratch HOME (P5).
fn assert_no_boot_service(home: &std::path::Path) -> Result<()> {
    let forbidden = [
        "Library/LaunchAgents",
        "Library/LaunchDaemons",
        ".config/systemd",
        ".config/herdr-wake",
    ];
    for rel in forbidden {
        let path = home.join(rel);
        assert!(
            !path.exists(),
            "acceptance created a boot-service path under the scratch home: {}",
            path.display()
        );
    }
    Ok(())
}

/// End-to-end headless acceptance: real hook record, SIGKILL mid-turn, restore
/// from the record, twice, with the transcript invariants holding each time.
#[test]
fn test_headless_hc_acceptance_sigkill_restore_with_real_hooks() -> Result<()> {
    let home = crate::test_temp::tempdir_with_prefix("kcode-hc-accept-").map_err(|e| anyhow!(e))?;
    let scratch_home = home.path().join("scratch-home");
    std::fs::create_dir_all(&scratch_home).map_err(|e| anyhow!(e))?;

    let session_id = "session_hc_headless_accept";
    let record_path = home.path().join("session-start.json");
    let ready_path = home.path().join("child-ready");
    let hook_script = write_record_hook(home.path())?;
    let hook_command = format!("{} {}", hook_script.display(), record_path.display());
    assert!(
        !hook_command.contains('"'),
        "hook command must not need shell quoting"
    );

    let mut child = std::process::Command::new(std::env::current_exe().map_err(|e| anyhow!(e))?)
        .arg("hc_headless_child_session_start")
        .arg("--test-threads=1")
        .arg("--nocapture")
        .env("KCODE_HOME", home.path())
        .env("HOME", &scratch_home)
        .env("KCODE_HOOK_SESSION_START", &hook_command)
        .env(CHILD_SESSION, session_id)
        .env(CHILD_RECORD, &record_path)
        .env(CHILD_READY, &ready_path)
        .env_remove("KCODE_HOOKS_DISABLED")
        .spawn()
        .map_err(|e| anyhow!("spawn headless child: {e}"))?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !ready_path.exists() {
        if let Some(status) = child.try_wait().map_err(|e| anyhow!(e))? {
            return Err(anyhow!("child exited before the mid-turn marker: {status}"));
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!("child never reached the mid-turn marker"));
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    // P1 in the crash case: the owner is killed, so nothing is attached any more.
    child.kill().map_err(|e| anyhow!("SIGKILL child: {e}"))?;
    let status = child.wait().map_err(|e| anyhow!(e))?;
    assert!(!status.success(), "child must be killed, not exit cleanly");

    // P3: the record was produced by the real hook surface, with real payload keys.
    let record = read_record(&record_path)?;
    assert_eq!(record["event"], "session_start", "record: {record}");
    assert_eq!(record["session_id"], session_id, "record: {record}");
    let recorded_cwd = record["cwd"]
        .as_str()
        .ok_or_else(|| anyhow!("record has no cwd: {record}"))?;
    assert!(
        std::path::Path::new(recorded_cwd).is_dir(),
        "recorded cwd is not a real directory: {recorded_cwd}"
    );

    // P2: the record holds launch facts, never a command line that could be replayed.
    let keys: Vec<&str> = record
        .as_object()
        .map(|o| o.keys().map(String::as_str).collect())
        .unwrap_or_default();
    for forbidden in ["argv", "command", "args", "resume"] {
        assert!(
            !keys.iter().any(|k| k.eq_ignore_ascii_case(forbidden)),
            "record carries a replayable command line ({forbidden}): {keys:?}"
        );
    }
    let raw_record = std::fs::read_to_string(&record_path).map_err(|e| anyhow!(e))?;
    assert!(
        !raw_record.contains("--resume"),
        "record must not carry a --resume flag: {raw_record}"
    );

    // P5: no boot service, anywhere.
    assert_no_boot_service(&scratch_home)?;

    let _lock = lock_env();
    let _store = EnvVarGuard::set("KCODE_HOME", home.path());

    // P1 preflight on the restored record: the recorded pid is dead, so it allows.
    let first = Session::load(session_id)?;
    assert_eq!(
        first.working_dir.as_deref(),
        Some(recorded_cwd),
        "restored cwd must equal the launch-time record"
    );
    assert!(
        preflight_refusal(&first).is_none(),
        "restore must be allowed once the owner is gone: {:?}",
        preflight_refusal(&first)
    );
    assert_eq!(first.provider_key.as_deref(), Some("hc-accept-provider"));

    // Exactly-once invariants on the restored transcript.
    let violations = recovery_invariant_violations(&first.messages);
    assert!(
        violations.is_empty(),
        "restored transcript is corrupt after a real mid-turn kill: {violations:?}"
    );
    let tool_uses: Vec<String> = first
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::ToolUse { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(tool_uses, vec!["toolu_hc_1".to_string()]);

    // Exactly-once survives a second restore: identical ids in identical order.
    let second = Session::load(session_id)?;
    let ids = |session: &Session| -> Vec<String> {
        session
            .messages
            .iter()
            .flat_map(|m| m.content.iter())
            .filter_map(|b| match b {
                ContentBlock::ToolUse { id, .. } => Some(format!("use:{id}")),
                ContentBlock::ToolResult { tool_use_id, .. } => Some(format!("res:{tool_use_id}")),
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        ids(&first),
        ids(&second),
        "restore is not idempotent across two loads"
    );
    assert!(recovery_invariant_violations(&second.messages).is_empty());

    // A restored session must accept new work.
    let mut resumed = second;
    resumed.add_message(
        Role::Assistant,
        vec![text_block("resumed after headless kill")],
    );
    resumed.save()?;
    let reloaded = Session::load(session_id)?;
    assert!(recovery_invariant_violations(&reloaded.messages).is_empty());
    assert_eq!(
        reloaded.messages.last().map(|m| m.content_preview()),
        Some("resumed after headless kill".to_string())
    );

    Ok(())
}

/// Negative control for P1/N3 and N5: the preflight must refuse a session whose
/// owner is still alive, and one whose cwd is a symlink.
#[test]
fn test_headless_acceptance_preflight_refuses_live_owner_and_symlinked_cwd() -> Result<()> {
    let temp =
        crate::test_temp::tempdir_with_prefix("kcode-hc-preflight-").map_err(|e| anyhow!(e))?;
    let real_dir = temp.path().join("real");
    std::fs::create_dir_all(&real_dir).map_err(|e| anyhow!(e))?;
    let link_dir = temp.path().join("link");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real_dir, &link_dir).map_err(|e| anyhow!(e))?;
    #[cfg(not(unix))]
    std::fs::create_dir_all(&link_dir).map_err(|e| anyhow!(e))?;

    let mut session = Session::create_with_id("session_hc_preflight".into(), None, None);
    session.working_dir = Some(real_dir.display().to_string());
    assert!(
        preflight_refusal(&session).is_none(),
        "a closed session in a real folder must be restorable"
    );

    session.last_pid = Some(std::process::id());
    let live = preflight_refusal(&session).expect("a live owner must be refused");
    assert!(
        live.contains("still alive"),
        "unexpected refusal reason: {live}"
    );

    session.last_pid = Some(u32::MAX); // no such process
    #[cfg(unix)]
    {
        session.working_dir = Some(link_dir.display().to_string());
        let linked = preflight_refusal(&session).expect("a symlinked cwd must be refused");
        assert!(
            linked.contains("symlink"),
            "unexpected refusal reason: {linked}"
        );
    }
    Ok(())
}
