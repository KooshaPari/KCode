//! Unit tests for the HERDR plugin manifest module.

use super::*;

#[test]
fn jcode_plugin_round_trips_as_valid_toml() {
    let m = jcode_plugin();
    let toml_text = m.to_toml();

    // Must parse cleanly.
    let parsed: toml::Value =
        toml::from_str(&toml_text).expect("plugin manifest must be valid TOML");

    // Top-level scalars preserved.
    assert_eq!(
        parsed.get("id").and_then(|v| v.as_str()),
        Some(JCODE_PLUGIN_ID)
    );
    assert_eq!(
        parsed.get("name").and_then(|v| v.as_str()),
        Some("Jcode HERDR integration")
    );
    assert_eq!(
        parsed.get("min_herdr_version").and_then(|v| v.as_str()),
        Some(MIN_HERDR_VERSION)
    );
    assert_eq!(
        parsed
            .get("platforms")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(2)
    );

    // Agents array has at least one jcode entry.
    let agents = parsed
        .get("agents")
        .and_then(|v| v.as_array())
        .expect("agents array");
    assert!(!agents.is_empty());
    assert_eq!(
        agents[0].get("kind").and_then(|v| v.as_str()),
        Some("jcode")
    );
    assert!(
        agents[0]
            .get("manifest")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .ends_with("jcode.toml")
    );

    // Actions array has install + status.
    let actions = parsed
        .get("actions")
        .and_then(|v| v.as_array())
        .expect("actions array");
    let action_ids: Vec<&str> = actions
        .iter()
        .map(|a| a.get("id").and_then(|v| v.as_str()).unwrap_or(""))
        .collect();
    assert!(action_ids.contains(&"install"));
    assert!(action_ids.contains(&"status"));

    // Install command should be [jcode, herdr, install].
    let install = actions
        .iter()
        .find(|a| a.get("id").and_then(|v| v.as_str()) == Some("install"))
        .expect("install action");
    let cmd = install
        .get("command")
        .and_then(|v| v.as_array())
        .expect("command array");
    let cmd_strs: Vec<&str> = cmd.iter().map(|c| c.as_str().unwrap_or("")).collect();
    assert_eq!(cmd_strs, vec!["jcode", "herdr", "install"]);
}

#[test]
fn forgecode_plugin_emits_forgecode_agent() {
    let m = forgecode_plugin();
    let toml_text = m.to_toml();
    let parsed: toml::Value = toml::from_str(&toml_text).expect("valid TOML");

    assert_eq!(
        parsed.get("id").and_then(|v| v.as_str()),
        Some(FORGECODE_PLUGIN_ID)
    );
    let agents = parsed
        .get("agents")
        .and_then(|v| v.as_array())
        .expect("agents");
    assert_eq!(agents.len(), 1);
    assert_eq!(
        agents[0].get("kind").and_then(|v| v.as_str()),
        Some("forgecode")
    );
    assert!(
        agents[0]
            .get("manifest")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .ends_with("forgecode.toml")
    );
}

#[test]
fn platforms_reflect_host_os() {
    let platforms = platforms::platforms_for_host();
    if cfg!(unix) {
        // Linux always, macOS only on darwin.
        assert!(platforms.contains(&"linux".to_string()));
        if std::env::consts::OS == "macos" {
            assert!(platforms.contains(&"macos".to_string()));
        } else {
            assert!(!platforms.contains(&"macos".to_string()));
        }
    } else {
        // Windows: the plugin is unsupportable, so we ship an
        // empty platforms list and surface the install hint
        // through `description` instead.
        assert!(platforms.is_empty());
    }
}

#[test]
fn windows_install_hint_mentions_wsl2() {
    let hint = platforms::windows_install_hint();
    assert!(hint.contains("WSL2"), "hint must mention WSL2: {hint}");
    assert!(
        hint.contains("Unix"),
        "hint must explain the Unix-only constraint: {hint}"
    );
}

#[test]
fn install_command_hint_targets_jcode() {
    let cmd = platforms::install_command_hint();
    assert!(!cmd.is_empty());
    // First element is the binary name; on jcode it is "jcode".
    // When compiled under jcode-herdr, CARGO_BIN_NAME is unset
    // and we fall back to "jcode".
    assert_eq!(cmd[0], "jcode");
    assert_eq!(cmd[1], "herdr");
    assert_eq!(cmd[2], "install");
}

#[test]
fn write_plugin_creates_parent_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("sub").join("plugin.toml");
    let m = jcode_plugin();
    write_plugin(&m, &path).expect("write_plugin must create parent dirs");
    assert!(path.exists());

    // Round-trip the written file as TOML.
    let body = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Value = toml::from_str(&body).expect("written file must be valid TOML");
    assert_eq!(
        parsed.get("id").and_then(|v| v.as_str()),
        Some(JCODE_PLUGIN_ID)
    );
}

#[test]
fn quote_str_escapes_special_chars() {
    // quote_str is private; reach it via the `super` re-export path
    // by triggering the same escaping through to_toml + parse round-trip.
    let m = jcode_plugin();
    let mut description = m.description.clone();
    description.push_str(" \\\\ \" \n \t end");
    let roundtrip = PluginManifest {
        description: description.clone(),
        ..m
    };
    let body = roundtrip.to_toml();
    let parsed: toml::Value = toml::from_str(&body).expect("must round-trip");
    assert_eq!(
        parsed.get("description").and_then(|v| v.as_str()),
        Some(description.as_str())
    );
}
