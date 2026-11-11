use std::process::Command;

#[test]
fn coexistence_does_not_initialize_user_state() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let path = root.path().join("path");
    std::fs::create_dir(&home).unwrap();
    std::fs::create_dir(&path).unwrap();
    for args in [
        vec!["coexistence", "--json"],
        vec!["coexistence", "--help"],
        vec!["coexistence", "--bad-option"],
        vec!["--ssh", "host", "coexistence"],
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_kcode"));
        command
            .env_clear()
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("LOCALAPPDATA", &home)
            .env("APPDATA", &home)
            .env("PATH", &path)
            .args(&args);
        if let Some(system_root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", system_root);
        }
        let output = command.output().unwrap();
        if args.contains(&"--bad-option") || args.contains(&"--ssh") {
            assert!(!output.status.success());
            if args.contains(&"--ssh") {
                assert!(
                    String::from_utf8_lossy(&output.stderr)
                        .contains("Coexistence discovery is local")
                );
            }
        } else {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        if args.contains(&"--json") {
            let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            let sources = value["sources"].as_array().unwrap();
            assert_eq!(sources.len(), 3);
            for source in sources {
                assert_eq!(source["command_discovery"], "not_found_on_path");
                for field in ["authentication", "provider", "model", "sessions"] {
                    assert_eq!(source[field], "unknown");
                }
            }
        }
        assert_eq!(std::fs::read_dir(&home).unwrap().count(), 0);
    }

    for path_value in [None, Some("relative")] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_kcode"));
        command
            .env_clear()
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("LOCALAPPDATA", &home)
            .env("APPDATA", &home)
            .args(["coexistence", "--json"]);
        if let Some(system_root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", system_root);
        }
        if let Some(path_value) = path_value {
            command.env("PATH", path_value);
        }
        let output = command.output().unwrap();
        assert!(output.status.success());
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        for source in value["sources"].as_array().unwrap() {
            assert_eq!(source["command_discovery"], "unknown");
        }
        assert_eq!(std::fs::read_dir(&home).unwrap().count(), 0);
    }
}
