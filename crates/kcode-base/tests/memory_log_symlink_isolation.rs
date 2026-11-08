#![cfg(unix)]

struct RestoreEnv(Vec<(&'static str, Option<std::ffi::OsString>)>);

impl Drop for RestoreEnv {
    fn drop(&mut self) {
        for (key, value) in self.0.drain(..) {
            if let Some(value) = value {
                kcode_core::env::set_var(key, value);
            } else {
                kcode_core::env::remove_var(key);
            }
        }
    }
}

#[test]
fn memory_log_does_not_append_through_a_dev_symlink_leaf() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join(".kcode-dev");
    std::fs::create_dir_all(home.join("logs")).unwrap();
    let home = std::fs::canonicalize(home).unwrap();
    let logs = home.join("logs");
    let target = temp.path().join("stable-memory-log.jsonl");
    std::fs::write(&target, b"stable sentinel\n").unwrap();
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    std::os::unix::fs::symlink(&target, logs.join(format!("memory-events-{date}.jsonl"))).unwrap();
    let _restore = RestoreEnv(
        ["KCODE_HOME", "KCODE_DEV_NAMESPACE"]
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect(),
    );
    kcode_core::env::set_var("KCODE_HOME", &home);
    kcode_core::env::set_var("KCODE_DEV_NAMESPACE", "1");

    kcode_base::memory_log::log_event(&kcode_memory_types::MemoryEventKind::EmbeddingStarted);

    assert_eq!(std::fs::read(&target).unwrap(), b"stable sentinel\n");
}
