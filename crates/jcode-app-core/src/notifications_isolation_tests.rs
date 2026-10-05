use super::*;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

struct EnvRestore(Vec<(&'static str, Option<std::ffi::OsString>)>);

impl Drop for EnvRestore {
    fn drop(&mut self) {
        for (key, value) in self.0.drain(..) {
            if let Some(value) = value {
                crate::env::set_var(key, value);
            } else {
                crate::env::remove_var(key);
            }
        }
    }
}

#[test]
fn notification_inbox_and_broker_symlinks_are_rejected_before_write_or_launch() {
    let _lock = crate::storage::lock_test_env();
    let keys = [
        "JCODE_HOME",
        "JCODE_DEV_NAMESPACE",
        "JCODE_MACOS_NOTIFICATION_BROKER_APP",
        "JCODE_MACOS_NOTIFICATION_INBOX",
    ];
    let _restore = EnvRestore(
        keys.into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    let temp = tempfile::tempdir().expect("temp dir");
    let home = temp.path().join(".jcode-dev");
    let stable = temp.path().join(".jcode-stable");
    let marker = temp.path().join("broker-invoked");
    let bundle = stable.join("Jcode Notifications.app");
    let executable = bundle.join("Contents/MacOS/jcode-notification-broker");
    std::fs::create_dir_all(executable.parent().unwrap()).expect("broker bundle");
    std::fs::write(
        &executable,
        format!("#!/bin/sh\nprintf invoked > '{}'\n", marker.display()),
    )
    .expect("fake executable");
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o750))
        .expect("executable mode");
    let before = std::fs::symlink_metadata(&executable).expect("executable metadata");
    let identity = (before.dev(), before.ino(), before.permissions().mode());
    let app_path = home.join("Applications/Jcode Notifications.app");
    std::fs::create_dir_all(app_path.parent().unwrap()).expect("application parent");
    std::os::unix::fs::symlink(&bundle, &app_path).expect("broker bundle symlink");

    let inbox_target = stable.join("notification-inbox");
    std::fs::create_dir_all(&inbox_target).expect("stable inbox");
    let inbox_sentinel = inbox_target.join("sentinel.json");
    std::fs::write(&inbox_sentinel, b"stable inbox sentinel\n").expect("inbox sentinel");
    std::fs::set_permissions(&inbox_sentinel, std::fs::Permissions::from_mode(0o640))
        .expect("inbox sentinel mode");
    let inbox_before = std::fs::symlink_metadata(&inbox_sentinel).expect("inbox metadata");
    let inbox_identity = (
        inbox_before.dev(),
        inbox_before.ino(),
        inbox_before.permissions().mode(),
    );
    let inbox = home.join("notifications/macos/inbox");
    std::fs::create_dir_all(inbox.parent().unwrap()).expect("inbox parent");
    std::os::unix::fs::symlink(&inbox_target, &inbox).expect("inbox symlink");

    crate::env::set_var("JCODE_HOME", &home);
    crate::env::set_var("JCODE_DEV_NAMESPACE", "1");
    crate::env::set_var("JCODE_MACOS_NOTIFICATION_BROKER_APP", &app_path);
    crate::env::set_var("JCODE_MACOS_NOTIFICATION_INBOX", &inbox);
    let envelope = MacosNotificationEnvelope {
        schema_version: MACOS_NOTIFICATION_SCHEMA_VERSION,
        notification_id: "negative-control".to_string(),
        title: "title".to_string(),
        subtitle: None,
        body: "body".to_string(),
        sound: None,
        origin: MacosNotificationOrigin {
            terminal: MacosTerminalKind::Unknown,
            bundle_id: None,
            tty: None,
            session_id: None,
        },
    };

    assert!(enqueue_macos_notification(&envelope).is_err());
    remove_queued_notification(&inbox.join("queued.json"));
    assert!(!send_macos_turn_notification("title", None, "body", None));
    assert!(!marker.exists());
    assert_eq!(
        std::fs::read(&executable).expect("executable bytes"),
        format!("#!/bin/sh\nprintf invoked > '{}'\n", marker.display()).as_bytes()
    );
    let after = std::fs::symlink_metadata(&executable).expect("executable metadata after");
    assert_eq!(
        (after.dev(), after.ino(), after.permissions().mode()),
        identity
    );
    assert_eq!(
        std::fs::read(&inbox_sentinel).expect("inbox bytes"),
        b"stable inbox sentinel\n"
    );
    let inbox_after = std::fs::symlink_metadata(&inbox_sentinel).expect("inbox metadata after");
    assert_eq!(
        (
            inbox_after.dev(),
            inbox_after.ino(),
            inbox_after.permissions().mode()
        ),
        inbox_identity
    );
    assert!(inbox.is_symlink());
    assert!(app_path.is_symlink());
}
