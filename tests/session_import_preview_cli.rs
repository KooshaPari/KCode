#![cfg(unix)]

use std::fs;
use std::process::Command;

#[test]
fn preview_import_and_decode_disposable_session() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let source = home.join(".jcode/sessions");
    let destination = temp.path().join("kcode");
    fs::create_dir_all(&source).unwrap();
    let session = jcode::session::Session::create_with_id(
        "fixture_session".to_owned(),
        None,
        Some("fixture title".to_owned()),
    );
    let snapshot = serde_json::to_vec(&session).unwrap();
    fs::write(source.join("fixture_session.json"), &snapshot).unwrap();
    fs::write(source.join("fixture_session.bak"), &snapshot).unwrap();
    fs::write(source.join("fixture_session.journal.jsonl"), b"").unwrap();
    let run = |preview: bool| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kcode"));
        cmd.env_clear()
            .env("HOME", &home)
            .env("KCODE_HOME", &destination);
        cmd.args(["session", "import-jcode", "fixture_session"]);
        if preview {
            cmd.arg("--preview");
        }
        cmd.output().unwrap()
    };
    let output = run(true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("future changes: unknown"));
    assert!(!destination.exists());
    let output = run(false);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let loaded =
        jcode::session::Session::load_from_path(&destination.join("sessions/fixture_session.json"))
            .unwrap();
    assert_eq!(loaded.id, "fixture_session");
    assert_eq!(loaded.title.as_deref(), Some("fixture title"));
    assert_eq!(
        fs::read(source.join("fixture_session.json")).unwrap(),
        snapshot
    );
    assert_eq!(
        fs::read(source.join("fixture_session.journal.jsonl")).unwrap(),
        b""
    );
    let conflict_preview = run(true);
    assert!(conflict_preview.status.success());
    assert!(
        String::from_utf8_lossy(&conflict_preview.stdout).contains("exists (import will refuse)")
    );
    assert!(!run(false).status.success());
    assert_eq!(
        fs::read(destination.join("sessions/fixture_session.json")).unwrap(),
        snapshot
    );
}
