#![cfg(unix)]

use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn executable(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(path, permissions).unwrap();
}

fn file_identity(path: &Path) -> (u64, u64, Vec<u8>) {
    let metadata = std::fs::metadata(path).unwrap();
    (metadata.dev(), metadata.ino(), std::fs::read(path).unwrap())
}

fn tree_snapshot(root: &Path) -> Vec<(PathBuf, u64, bool, Vec<u8>)> {
    fn visit(path: &Path, entries: &mut Vec<(PathBuf, u64, bool, Vec<u8>)>) {
        let metadata = std::fs::symlink_metadata(path).unwrap();
        let is_dir = metadata.is_dir();
        let bytes = if is_dir {
            Vec::new()
        } else {
            std::fs::read(path).unwrap()
        };
        entries.push((path.to_path_buf(), metadata.ino(), is_dir, bytes));
        if is_dir {
            for entry in std::fs::read_dir(path).unwrap() {
                visit(&entry.unwrap().path(), entries);
            }
        }
    }

    let mut entries = Vec::new();
    visit(root, &mut entries);
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

#[test]
fn dev_setup_commands_cannot_touch_global_hotkeys_or_launchers() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let home = std::fs::canonicalize(home).unwrap();
    let dev = home.join(".kcode-dev");
    let xdg = dev.join("xdg");
    let fake_bin = temp.path().join("fake-bin");
    std::fs::create_dir_all(&fake_bin).unwrap();

    let protected_files = [
        home.join(".kcode/config.toml"),
        home.join(".config/kcode/config.toml"),
        home.join("Library/LaunchAgents/com.kcode.hotkey.plist"),
        home.join(".config/niri/config.kdl"),
        home.join(
            "Library/Application Support/Mozilla/NativeMessagingHosts/firefox_agent_bridge.json",
        ),
        home.join(".config/herdr/agent-detection/kcode.toml"),
        home.join(".config/herdr/plugins/local/kcode.toml"),
    ];
    for file in &protected_files {
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, b"operator-owned sentinel\n").unwrap();
    }
    let before = protected_files
        .iter()
        .map(|path| file_identity(path))
        .collect::<Vec<_>>();
    let protected_roots = [
        home.join(".kcode"),
        home.join(".config"),
        home.join("Library"),
    ];
    let before_trees = protected_roots
        .iter()
        .map(|path| tree_snapshot(path))
        .collect::<Vec<_>>();

    let invoked = temp.path().join("global-command-invoked");
    for command in [
        "launchctl",
        "dconf",
        "xfconf-query",
        "firefox",
        "firefox-agent-bridge-host",
        "herdr",
        "security",
    ] {
        executable(
            &fake_bin.join(command),
            &format!(
                "#!/bin/sh\nprintf '%s\\n' '{}' >> '{}'\n",
                command,
                invoked.display()
            ),
        );
    }

    let binary = PathBuf::from(env!("CARGO_BIN_EXE_kcode"));
    let mut commands = vec![
        vec!["setup-hotkey"],
        vec!["setup-launcher"],
        vec!["browser", "setup"],
        vec!["herdr-install"],
        vec!["--provider", "claude", "auth", "import", "--stdin"],
    ];
    #[cfg(target_os = "macos")]
    commands.push(vec!["menubar"]);
    #[cfg(unix)]
    commands.push(vec!["api", "--api-socket", "/tmp/production-api.sock"]);
    commands.push(vec!["debug", "-s", "/tmp/production-debug.sock"]);
    #[cfg(target_os = "macos")]
    commands.push(vec!["setup-hotkey", "--listen-macos-hotkey"]);
    for command in commands {
        let output = Command::new(&binary)
            .env_clear()
            .arg("--no-update")
            .args(&command)
            .env("HOME", &home)
            .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
            .env("KCODE_DEV_NAMESPACE", "1")
            .env("KCODE_HOME", &dev)
            .env("KCODE_INSTALL_DIR", dev.join("bin"))
            .env("KCODE_RUNTIME_DIR", dev.join("run"))
            .env("CARGO_TARGET_DIR", dev.join("cargo-target"))
            .env("XDG_CONFIG_HOME", xdg.join("config"))
            .env("XDG_DATA_HOME", xdg.join("data"))
            .env("XDG_STATE_HOME", xdg.join("state"))
            .env("XDG_CACHE_HOME", xdg.join("cache"))
            .env_remove("KCODE_SOCKET")
            .env_remove("KCODE_API_SOCKET")
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "{command:?} unexpectedly succeeded"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        if command.starts_with(&["api"]) || command.starts_with(&["debug"]) {
            assert!(stderr.contains("outside"), "{command:?}: {stderr}");
        } else {
            assert!(
                stderr.contains("disabled in the kcode-dev namespace"),
                "{command:?}: {stderr}"
            );
        }
    }

    assert!(
        !invoked.exists(),
        "a global integration command was invoked"
    );
    let after = protected_files
        .iter()
        .map(|path| file_identity(path))
        .collect::<Vec<_>>();
    assert_eq!(before, after, "a protected home file changed");
    let after_trees = protected_roots
        .iter()
        .map(|path| tree_snapshot(path))
        .collect::<Vec<_>>();
    assert_eq!(before_trees, after_trees, "a protected home tree changed");
}

#[test]
fn dev_auth_status_cannot_reuse_stable_gh_cli_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("operator-home");
    let dev = home.join(".kcode-dev");
    let versions = dev.join("builds/versions/v1");
    let current = dev.join("builds/current/kcode");
    let fake_bin = temp.path().join("fake-bin");
    let invoked = temp.path().join("gh-invoked");
    std::fs::create_dir_all(&versions).unwrap();
    std::fs::create_dir_all(current.parent().unwrap()).unwrap();
    std::fs::create_dir_all(dev.join("home")).unwrap();
    std::fs::create_dir_all(&fake_bin).unwrap();
    std::fs::set_permissions(&dev, std::fs::Permissions::from_mode(0o700)).unwrap();
    let installed_binary = versions.join("kcode");
    std::fs::copy(env!("CARGO_BIN_EXE_kcode"), &installed_binary).unwrap();
    std::fs::set_permissions(
        &installed_binary,
        std::fs::metadata(env!("CARGO_BIN_EXE_kcode"))
            .unwrap()
            .permissions(),
    )
    .unwrap();
    std::os::unix::fs::symlink(&installed_binary, &current).unwrap();
    executable(
        &fake_bin.join("gh"),
        &format!(
            "#!/bin/sh\nprintf invoked > '{}'\nprintf 'FAKE_GH_TOKEN_SENTINEL\\n'\n",
            invoked.display()
        ),
    );

    let wrapper = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/kcode-dev");
    let output = Command::new(wrapper)
        .args(["--provider", "copilot", "auth", "status", "--json"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env_clear()
        .env("HOME", &home)
        .env("PATH", format!("{}:/usr/bin:/bin", fake_bin.display()))
        .env("KCODE_COPILOT_ALLOW_GH_AUTH_TOKEN", "1")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "auth status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !invoked.exists(),
        "dev auth status invoked stable gh auth token"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("FAKE_GH_TOKEN_SENTINEL"));
}
