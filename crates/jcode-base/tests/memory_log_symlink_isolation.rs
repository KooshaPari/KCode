#![cfg(unix)]

struct RestoreEnv(Vec<(&'static str, Option<std::ffi::OsString>)>);

impl Drop for RestoreEnv {
    fn drop(&mut self) {
        for (key, value) in self.0.drain(..) {
            if let Some(value) = value {
                jcode_core::env::set_var(key, value);
            } else {
                jcode_core::env::remove_var(key);
            }
        }
    }
}

#[test]
fn memory_log_does_not_append_through_a_dev_symlink_leaf() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join(".jcode-dev");
    std::fs::create_dir_all(home.join("logs")).unwrap();
    let home = std::fs::canonicalize(home).unwrap();
    let logs = home.join("logs");
    let target = temp.path().join("stable-memory-log.jsonl");
    std::fs::write(&target, b"stable sentinel\n").unwrap();
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    std::os::unix::fs::symlink(&target, logs.join(format!("memory-events-{date}.jsonl"))).unwrap();
    let _restore = RestoreEnv(
        ["JCODE_HOME", "JCODE_DEV_NAMESPACE"]
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    jcode_core::env::set_var("JCODE_HOME", &home);
    jcode_core::env::set_var("JCODE_DEV_NAMESPACE", "1");

    jcode_base::memory_log::log_event(&jcode_memory_types::MemoryEventKind::EmbeddingStarted);

    assert_eq!(std::fs::read(&target).unwrap(), b"stable sentinel\n");
}
