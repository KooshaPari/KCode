use super::super::{DEV_HOME_NAME, resolve_for_home};
use std::os::unix::fs::{MetadataExt, PermissionsExt};

fn assert_rejected_sentinel_symlink(root: &str, relative_sentinel: &str) {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let dev = home.join(DEV_HOME_NAME);
    let stable_root = home.join("stable").join(root.replace('/', "_"));
    let sentinel = stable_root.join(relative_sentinel);
    std::fs::create_dir_all(sentinel.parent().unwrap()).unwrap();
    std::fs::write(&sentinel, b"stable sentinel\n").unwrap();
    std::fs::set_permissions(&sentinel, std::fs::Permissions::from_mode(0o640)).unwrap();
    let before = std::fs::metadata(&sentinel).unwrap();
    let before_identity = (before.dev(), before.ino(), before.permissions().mode());
    let before_content = std::fs::read(&sentinel).unwrap();

    std::fs::create_dir_all(&dev).unwrap();
    std::fs::set_permissions(&dev, std::fs::Permissions::from_mode(0o700)).unwrap();
    let destination = dev.join(root);
    std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&stable_root, &destination).unwrap();

    assert!(
        resolve_for_home(home).is_err(),
        "accepted symlinked private root {root}"
    );
    let after = std::fs::metadata(&sentinel).unwrap();
    assert_eq!(
        (after.dev(), after.ino(), after.permissions().mode()),
        before_identity,
        "stable sentinel identity or mode changed for {root}"
    );
    assert_eq!(std::fs::read(&sentinel).unwrap(), before_content);
}

fn assert_rejected_leaf_symlink(relative_path: &str, executable: bool) {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let stable = home.join("stable-leaf");
    let marker = home.join("invoked");
    let contents = if executable {
        format!("#!/bin/sh\nprintf invoked > '{}'\n", marker.display()).into_bytes()
    } else {
        b"stable credential sentinel\n".to_vec()
    };
    std::fs::write(&stable, &contents).unwrap();
    let mode = if executable { 0o750 } else { 0o640 };
    std::fs::set_permissions(&stable, std::fs::Permissions::from_mode(mode)).unwrap();
    let before = std::fs::metadata(&stable).unwrap();
    let before_identity = (before.dev(), before.ino(), before.permissions().mode());

    let dev = home.join(DEV_HOME_NAME);
    std::fs::create_dir_all(&dev).unwrap();
    std::fs::set_permissions(&dev, std::fs::Permissions::from_mode(0o700)).unwrap();
    let destination = dev.join(relative_path);
    std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&stable, &destination).unwrap();

    let _env_lock = crate::storage::lock_test_env();
    let saved = ["JCODE_HOME", "JCODE_DEV_NAMESPACE"]
        .map(|name| (name, std::env::var_os(name)))
        .to_vec();
    let _restore = super::RestoreEnv(saved);
    crate::env::set_var("JCODE_HOME", &dev);
    crate::env::set_var("JCODE_DEV_NAMESPACE", "1");
    assert!(
        crate::storage::reject_dev_home_symlink_path(&destination).is_err(),
        "per-use guard accepted symlinked leaf {relative_path}"
    );

    assert!(
        resolve_for_home(home).is_err(),
        "accepted symlinked private leaf {relative_path}"
    );
    let after = std::fs::metadata(&stable).unwrap();
    assert_eq!(
        (after.dev(), after.ino(), after.permissions().mode()),
        before_identity,
        "stable leaf identity or mode changed for {relative_path}"
    );
    assert_eq!(std::fs::read(&stable).unwrap(), contents);
    assert!(!marker.exists(), "fake executable ran for {relative_path}");
}

#[test]
fn rejects_sensitive_root_symlinks_without_touching_stable_sentinels() {
    let cases = [
        (".grok", "auth.json"),
        ("external", ".codex/auth.json"),
        ("config/jcode", "config.toml"),
        ("external/aws", "credentials"),
        ("external/azure", "Azure/TokenCache.dat"),
        ("external/gcloud", "credentials.db"),
        ("external/.agents/skills", "index.json"),
        ("external/.claude/skills", "index.json"),
        ("external/.codex/skills", "index.json"),
        ("browser", ".setup-complete"),
        ("notifications", "macos/inbox/notification.json"),
        ("ambient", "transcripts/turn.jsonl"),
        ("safety", "queue.json"),
        ("goals", "global/goal.json"),
        ("provider-backends/grok-build/grok", "fake-tool"),
        ("provider-backends/grok/grok", "fake-tool"),
        (
            "Applications",
            "JcodeNotificationBroker.app/Contents/MacOS/JcodeNotificationBroker",
        ),
    ];
    for (root, sentinel) in cases {
        assert_rejected_sentinel_symlink(root, sentinel);
    }
}

#[test]
fn rejects_sensitive_leaf_symlinks_without_reading_or_invoking_stable_targets() {
    for path in [
        "external/aws/credentials",
        "external/aws/config",
        "external/azure/msal_token_cache.json",
        "external/gcloud/credentials.db",
        "config/jcode/openrouter.env",
        "browser/.setup-complete",
        "session-memory/config",
        "schema-quirks.json",
        "last_seen_changelog",
        "reload-context-session-123.json",
        "telemetry_milestone_provider_login_1",
        "telemetry_active_days_123.txt",
        "telemetry_session_starts_123.txt",
        "telemetry_active_sessions/session-123.active",
        "reload-recovery/session-123.json",
        "run/durable-state/swarm/swarm-123.json",
        "run/jcode-swarm-state/swarm-123.json",
    ] {
        assert_rejected_leaf_symlink(path, false);
    }
    assert_rejected_leaf_symlink(
        "Applications/JcodeNotificationBroker.app/Contents/MacOS/JcodeNotificationBroker",
        true,
    );
    assert_rejected_leaf_symlink("provider-backends/grok-build/grok", true);
}
