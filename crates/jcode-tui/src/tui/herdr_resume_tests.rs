use super::{resolve_session_program, resume_argv_for_local_session};
use std::path::{Path, PathBuf};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "jcode-herdr-resume-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn put_session(home: &Path, id: &str) {
    let sessions = home.join("sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(sessions.join(format!("{id}.json")), "{}").unwrap();
}

#[test]
fn kcode_only_session_resolves_to_kcode() {
    let root = scratch("kcode-only");
    let jcode_home = root.join(".jcode");
    let kcode_home = root.join(".kcode");
    std::fs::create_dir_all(&jcode_home).unwrap();
    put_session(&kcode_home, "session_kcode_1");

    assert_eq!(
        resolve_session_program("session_kcode_1", &jcode_home, &kcode_home),
        Some("kcode")
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn jcode_only_session_resolves_to_jcode() {
    let root = scratch("jcode-only");
    let jcode_home = root.join(".jcode");
    let kcode_home = root.join(".kcode");
    std::fs::create_dir_all(&kcode_home).unwrap();
    put_session(&jcode_home, "session_jcode_1");

    assert_eq!(
        resolve_session_program("session_jcode_1", &jcode_home, &kcode_home),
        Some("jcode")
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn id_in_neither_store_is_unresolved() {
    let root = scratch("neither");
    let jcode_home = root.join(".jcode");
    let kcode_home = root.join(".kcode");
    std::fs::create_dir_all(&jcode_home).unwrap();
    std::fs::create_dir_all(&kcode_home).unwrap();

    assert_eq!(
        resolve_session_program("session_ghost_1", &jcode_home, &kcode_home),
        None
    );
    // The full argv path must also refuse, not fall back to jcode.
    assert_eq!(
        resume_argv_for_local_session("session_ghost_1", |_| None),
        None
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn id_in_both_stores_is_unresolved() {
    let root = scratch("both");
    let jcode_home = root.join(".jcode");
    let kcode_home = root.join(".kcode");
    put_session(&jcode_home, "session_dup_1");
    put_session(&kcode_home, "session_dup_1");

    assert_eq!(
        resolve_session_program("session_dup_1", &jcode_home, &kcode_home),
        None
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn unsafe_id_never_touches_the_filesystem() {
    let root = scratch("unsafe");
    let jcode_home = root.join(".jcode");
    let kcode_home = root.join(".kcode");
    std::fs::create_dir_all(&kcode_home).unwrap();
    put_session(&kcode_home, "x");

    assert_eq!(
        resolve_session_program("../x", &jcode_home, &kcode_home),
        None
    );
    std::fs::remove_dir_all(&root).ok();
}
