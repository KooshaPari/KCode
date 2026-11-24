//! Mid-turn SIGKILL recovery for sessions restored by HC revive.
//!
//! A revived session is restored from the on-disk snapshot plus whatever the
//! journal replayed. If the process that owned the session was SIGKILLed in the
//! middle of a turn, the resumed transcript must contain every tool call that
//! had already completed, exactly once, with each `tool_use` still paired to
//! its `tool_result`. A duplicated or dropped tool call makes the next provider
//! request invalid, which is the failure mode this file pins down.

use super::*;
use anyhow::{Result, anyhow};

/// Invariants the resumed transcript must satisfy. Returns one message per
/// violation so the negative control can prove the checker is not vacuous.
fn recovery_invariant_violations(messages: &[StoredMessage]) -> Vec<String> {
    let mut violations = Vec::new();
    let mut seen_message_ids = std::collections::HashSet::new();
    let mut tool_use_ids = std::collections::HashSet::new();
    let mut tool_result_ids = std::collections::HashSet::new();

    for message in messages {
        if !seen_message_ids.insert(message.id.clone()) {
            violations.push(format!("duplicate message id {}", message.id));
        }
        for block in &message.content {
            match block {
                ContentBlock::ToolUse { id, .. } => {
                    if !tool_use_ids.insert(id.clone()) {
                        violations.push(format!("duplicate tool_use id {id}"));
                    }
                }
                ContentBlock::ToolResult { tool_use_id, .. } => {
                    if !tool_result_ids.insert(tool_use_id.clone()) {
                        violations.push(format!("duplicate tool_result for {tool_use_id}"));
                    }
                }
                _ => {}
            }
        }
    }

    for id in &tool_use_ids {
        if !tool_result_ids.contains(id) {
            violations.push(format!("tool_use {id} has no tool_result (dropped call)"));
        }
    }
    for id in &tool_result_ids {
        if !tool_use_ids.contains(id) {
            violations.push(format!("tool_result {id} has no tool_use"));
        }
    }
    violations
}

/// Child-process entry point. Normal runs return immediately; the parent test
/// re-executes this test binary with the env vars set, so the child journals a
/// completed tool call and then blocks mid-turn until it is killed.
#[test]
fn crash_recovery_child_appends_midstream() -> Result<()> {
    let Ok(session_id) = std::env::var("KCODE_RECOVERY_CHILD_SESSION") else {
        return Ok(());
    };
    let ready_path = std::env::var("KCODE_RECOVERY_CHILD_READY")
        .map_err(|e| anyhow!("missing KCODE_RECOVERY_CHILD_READY: {e}"))?;

    let mut session = Session::create_with_id(session_id, None, Some("kill-test".into()));
    session.add_message(Role::User, vec![text_block("start the turn")]);
    session.save()?;

    session.add_message(
        Role::Assistant,
        vec![ContentBlock::ToolUse {
            id: "toolu_kill_1".into(),
            name: "bash".into(),
            input: serde_json::json!({"command": "ls"}),
            thought_signature: None,
        }],
    );
    session.save()?;

    session.add_message(
        Role::User,
        vec![ContentBlock::ToolResult {
            tool_use_id: "toolu_kill_1".into(),
            content: "file-a\nfile-b".into(),
            is_error: None,
        }],
    );
    session.save()?;

    // Both completed entries are journaled; the turn is still "in flight"
    // because this process never reaches a clean shutdown.
    std::fs::write(&ready_path, b"ready")?;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn text_block(text: &str) -> ContentBlock {
    ContentBlock::Text {
        text: text.to_string(),
        cache_control: None,
    }
}

/// Kill a live session process mid-stream, then load the session the way HC
/// revive does. Every completed tool call must survive exactly once.
#[test]
fn test_midstream_sigkill_preserves_completed_tool_calls_without_duplicates() -> Result<()> {
    let temp_home =
        crate::test_temp::tempdir_with_prefix("kcode-session-midstream-sigkill-")
            .map_err(|e| anyhow!(e))?;
    let session_id = "session_midstream_sigkill_test";
    let ready_path = temp_home.path().join("child-ready");

    let mut child = std::process::Command::new(std::env::current_exe().map_err(|e| anyhow!(e))?)
        .arg("crash_recovery_child_appends_midstream")
        .arg("--test-threads=1")
        .arg("--nocapture")
        .env("KCODE_HOME", temp_home.path())
        .env("KCODE_RECOVERY_CHILD_SESSION", session_id)
        .env("KCODE_RECOVERY_CHILD_READY", &ready_path)
        .spawn()
        .map_err(|e| anyhow!("spawn child session process: {e}"))?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !ready_path.exists() {
        if let Some(status) = child.try_wait().map_err(|e| anyhow!(e))? {
            return Err(anyhow!(
                "child session process exited before journaling a tool call: {status}"
            ));
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!("child never reached the mid-stream marker"));
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    // SIGKILL: no destructors, no clean shutdown, exactly what a crash looks like.
    child.kill().map_err(|e| anyhow!("SIGKILL child: {e}"))?;
    let status = child.wait().map_err(|e| anyhow!(e))?;
    assert!(!status.success(), "child must be killed, not exit cleanly");

    let _lock = lock_env();
    let _home = EnvVarGuard::set("KCODE_HOME", temp_home.path());

    let resumed = Session::load(session_id)?;
    let violations = recovery_invariant_violations(&resumed.messages);
    assert!(
        violations.is_empty(),
        "resumed transcript is corrupt after mid-stream SIGKILL: {violations:?}"
    );

    let tool_uses: Vec<_> = resumed
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::ToolUse { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();
    let tool_results: Vec<_> = resumed
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        tool_uses,
        vec!["toolu_kill_1".to_string()],
        "completed tool call must survive exactly once, got {tool_uses:?}"
    );
    assert_eq!(tool_results, vec!["toolu_kill_1".to_string()]);

    // The resumed session must still accept new work, exactly like HC revive.
    let mut resumed = resumed;
    resumed.add_message(Role::Assistant, vec![text_block("resumed after kill")]);
    resumed.save()?;
    let reloaded = Session::load(session_id)?;
    assert!(recovery_invariant_violations(&reloaded.messages).is_empty());
    assert_eq!(
        reloaded.messages.last().map(|m| m.content_preview()),
        Some("resumed after kill".to_string())
    );
    Ok(())
}

/// Negative control: the invariant checker must reject the two ways recovery
/// can break (a replayed duplicate, and a dropped completed call). If this
/// passes with no violations the positive test above would prove nothing.
#[test]
fn negative_control_recovery_checker_rejects_broken_transcripts() -> Result<()> {
    let mut session = Session::create_with_id("session_recovery_control".into(), None, None);
    session.add_message(Role::User, vec![text_block("go")]);
    session.add_message(
        Role::Assistant,
        vec![ContentBlock::ToolUse {
            id: "toolu_ctl_1".into(),
            name: "bash".into(),
            input: serde_json::json!({"command": "true"}),
            thought_signature: None,
        }],
    );

    // Dropped result: a completed call whose tool_result never made it back.
    let dropped = recovery_invariant_violations(&session.messages);
    assert!(
        dropped.iter().any(|v| v.contains("no tool_result")),
        "checker missed a dropped tool_result: {dropped:?}"
    );

    session.add_message(
        Role::User,
        vec![ContentBlock::ToolResult {
            tool_use_id: "toolu_ctl_1".into(),
            content: "ok".into(),
            is_error: None,
        }],
    );
    assert!(recovery_invariant_violations(&session.messages).is_empty());

    // Duplicated replay: the same tool_use appearing twice.
    let duplicate = session.messages[1].clone();
    session.messages.push(duplicate);
    let duplicated = recovery_invariant_violations(&session.messages);
    assert!(
        duplicated.iter().any(|v| v.contains("duplicate tool_use id")),
        "checker missed a duplicated tool_use: {duplicated:?}"
    );
    Ok(())
}
