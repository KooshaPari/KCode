use super::*;
use std::collections::BTreeMap;
use std::time::SystemTime;

type Snapshot = BTreeMap<PathBuf, (Vec<u8>, SystemTime, bool)>;

#[test]
fn preview_then_import_preserves_source_and_reports_conflict() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join(".jcode");
    let destination = temp.path().join(".kcode");
    fs::create_dir_all(source.join("sessions")).unwrap();
    for (suffix, data) in [
        ("json", b"snapshot".as_slice()),
        ("bak", b"backup"),
        ("journal.jsonl", b"journal"),
    ] {
        fs::write(source.join("sessions").join(format!("fox.{suffix}")), data).unwrap();
    }
    let before = snapshot(&source);
    let preview = preview_session(&source, &destination, "fox").unwrap();
    assert_eq!(preview.len(), 3);
    assert!(preview.iter().all(|file| !file.conflict));
    assert!(!destination.exists());
    assert_eq!(before, snapshot(&source));
    let imported = import_session(&source, &destination, "fox").unwrap();
    assert_eq!(imported.len(), 3);
    for file in &imported {
        assert_eq!(
            fs::read(file).unwrap(),
            fs::read(source.join("sessions").join(file.file_name().unwrap())).unwrap()
        );
    }
    let copied = snapshot(&destination);
    assert!(
        preview_session(&source, &destination, "fox")
            .unwrap()
            .iter()
            .all(|file| file.conflict)
    );
    assert!(import_session(&source, &destination, "fox").is_err());
    assert_eq!(copied, snapshot(&destination));
    assert_eq!(before, snapshot(&source));
    assert!(preview_session(&source, &destination, "missing").is_err());
    assert!(preview_session(&source, &destination, "../fox").is_err());
}

fn snapshot(root: &Path) -> Snapshot {
    fn visit(path: &Path, root: &Path, state: &mut Snapshot) {
        let metadata = fs::metadata(path).unwrap();
        let content = if metadata.is_file() {
            fs::read(path).unwrap()
        } else {
            vec![]
        };
        state.insert(
            path.strip_prefix(root).unwrap().to_owned(),
            (
                content,
                metadata.modified().unwrap(),
                metadata.permissions().readonly(),
            ),
        );
        if metadata.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                visit(&entry.unwrap().path(), root, state);
            }
        }
    }
    let mut state = Snapshot::new();
    visit(root, root, &mut state);
    state
}

#[test]
fn import_then_edit_kcode_never_writes_back_to_jcode() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join(".jcode");
    let destination = temp.path().join(".kcode");
    fs::create_dir_all(source.join("sessions")).unwrap();
    for id in ["fox", "otter"] {
        for suffix in ["json", "bak", "journal.jsonl"] {
            fs::write(
                source.join("sessions").join(format!("{id}.{suffix}")),
                format!("original {id} {suffix}\n"),
            )
            .unwrap();
        }
    }
    fs::write(source.join("unrelated-marker"), b"never import me").unwrap();
    let before = snapshot(&source);
    for id in ["fox", "otter"] {
        let copied = import_session(&source, &destination, id).unwrap();
        assert_eq!(copied.len(), 3);
        for path in copied {
            assert_eq!(
                fs::read(&path).unwrap(),
                fs::read(source.join("sessions").join(path.file_name().unwrap())).unwrap()
            );
            fs::write(path, b"KCode independently edited this session").unwrap();
        }
        assert!(import_session(&source, &destination, id).is_err());
    }
    assert_eq!(
        before,
        snapshot(&source),
        "source bytes, mtimes, permissions and tree must be identical"
    );
    assert!(!destination.join("unrelated-marker").exists());
}

#[test]
fn overlapping_storage_and_traversal_fail_without_source_changes() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join(".jcode");
    fs::create_dir_all(source.join("sessions")).unwrap();
    fs::write(source.join("sessions/fox.json"), b"snapshot").unwrap();
    let before = snapshot(&source);
    for destination in [&source, &source.join("nested")] {
        assert!(import_session(&source, destination, "fox").is_err());
    }
    assert!(import_session(&source, &temp.path().join(".kcode"), "../fox").is_err());
    assert_eq!(before, snapshot(&source));
}

#[test]
fn existing_destination_is_never_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join(".jcode");
    let destination = temp.path().join(".kcode");
    fs::create_dir_all(source.join("sessions")).unwrap();
    fs::create_dir_all(destination.join("sessions")).unwrap();
    fs::write(source.join("sessions/fox.json"), b"upstream").unwrap();
    fs::write(destination.join("sessions/fox.json"), b"local").unwrap();
    assert!(import_session(&source, &destination, "fox").is_err());
    assert_eq!(
        fs::read(destination.join("sessions/fox.json")).unwrap(),
        b"local"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_source_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join(".jcode");
    fs::create_dir_all(source.join("sessions")).unwrap();
    fs::write(temp.path().join("outside"), b"outside").unwrap();
    std::os::unix::fs::symlink(
        temp.path().join("outside"),
        source.join("sessions/fox.json"),
    )
    .unwrap();
    assert!(preview_session(&source, &temp.path().join(".kcode"), "fox").is_err());
    assert!(import_session(&source, &temp.path().join(".kcode"), "fox").is_err());
}
