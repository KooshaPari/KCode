use super::*;
use std::ffi::OsString;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::sync::MutexGuard;

#[cfg(unix)]
#[path = "dev_namespace_tests/store_roots.rs"]
mod store_roots;

fn env_lock() -> MutexGuard<'static, ()> {
    crate::storage::lock_test_env()
}

fn setup_env(home: &Path) -> Vec<(&'static str, Option<OsString>)> {
    let vars = [
        "HOME",
        "JCODE_DEV_NAMESPACE",
        "JCODE_HOME",
        "JCODE_INSTALL_DIR",
        "JCODE_RUNTIME_DIR",
        "CARGO_TARGET_DIR",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "JCODE_SOCKET",
        "JCODE_API_SOCKET",
        "OPENAI_API_KEY",
        "CUSTOM_GATEWAY_KEY",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "CLAUDE_CONFIG_DIR",
        "GROK_HOME",
        "JCODE_GROK_CLI_PATH",
        "JCODE_ANTHROPIC_HEADERS",
        "JCODE_ANTHROPIC_AUTH_HEADER",
        "JCODE_ANTHROPIC_API_BASE",
        "ANTHROPIC_BASE_URL",
        "JCODE_OPENROUTER_API_BASE",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_ACCESS_KEY_ID",
        "OPENAI_BASE_URL",
        "OPENAI_API_BASE",
        "JCODE_ANTHROPIC_ENV_FILE",
        "JCODE_OPENROUTER_ENV_FILE",
        "AWS_CONFIG_FILE",
        "AWS_PROFILE",
        "AWS_DEFAULT_PROFILE",
        "AWS_WEB_IDENTITY_TOKEN_FILE",
        "AWS_CONTAINER_CREDENTIALS_FULL_URI",
        "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI",
        "AWS_SHARED_CREDENTIALS_FILE",
        "AWS_EC2_METADATA_DISABLED",
        "GOOGLE_APPLICATION_CREDENTIALS",
        "AZURE_CONFIG_DIR",
        "CLOUDSDK_CONFIG",
        "JCODE_OPENROUTER_MAX_TOKENS",
    ];
    let saved = vars
        .into_iter()
        .map(|key| (key, std::env::var_os(key)))
        .collect::<Vec<_>>();
    let home = std::fs::canonicalize(home).unwrap();
    let paths = resolve_for_home(&home).unwrap();
    crate::env::set_var("HOME", &home);
    crate::env::set_var("JCODE_DEV_NAMESPACE", "1");
    crate::env::set_var("JCODE_HOME", &paths.home);
    crate::env::set_var("JCODE_INSTALL_DIR", &paths.install);
    crate::env::set_var("JCODE_RUNTIME_DIR", &paths.runtime);
    crate::env::set_var("CARGO_TARGET_DIR", &paths.target);
    crate::env::set_var("XDG_CONFIG_HOME", &paths.xdg_config);
    crate::env::set_var("XDG_DATA_HOME", &paths.xdg_data);
    crate::env::set_var("XDG_STATE_HOME", &paths.xdg_state);
    crate::env::set_var("XDG_CACHE_HOME", &paths.xdg_cache);
    crate::env::remove_var("JCODE_SOCKET");
    crate::env::remove_var("JCODE_API_SOCKET");
    saved
}

struct RestoreEnv(Vec<(&'static str, Option<OsString>)>);

impl Drop for RestoreEnv {
    fn drop(&mut self) {
        for (key, value) in &self.0 {
            if let Some(value) = value {
                crate::env::set_var(key, value);
            } else {
                crate::env::remove_var(key);
            }
        }
    }
}

#[test]
fn resolves_distinct_absolute_dev_roots() {
    let temp = tempfile::tempdir().unwrap();
    let paths = resolve_for_home(temp.path()).unwrap();
    let home = std::fs::canonicalize(temp.path()).unwrap();
    assert_eq!(paths.home, home.join(DEV_HOME_NAME));
    assert_eq!(paths.install, paths.home.join("bin"));
    assert_eq!(paths.runtime, paths.home.join("run"));
    assert_eq!(paths.target, paths.home.join("cargo-target"));
    assert_eq!(paths.xdg_config, paths.home.join("xdg/config"));
    assert_eq!(paths.xdg_data, paths.home.join("xdg/data"));
    assert_eq!(paths.xdg_state, paths.home.join("xdg/state"));
    assert_eq!(paths.xdg_cache, paths.home.join("xdg/cache"));
    assert!(paths.home.is_absolute());
}

#[test]
fn rejects_dev_root_symlink_into_production_home() {
    let temp = tempfile::tempdir().unwrap();
    let production = temp.path().join(".jcode");
    std::fs::create_dir(&production).unwrap();
    let dev = temp.path().join(DEV_HOME_NAME);
    #[cfg(unix)]
    std::os::unix::fs::symlink(&production, &dev).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&production, &dev).unwrap();
    assert!(resolve_for_home(temp.path()).is_err());
}

#[cfg(unix)]
#[test]
fn rejects_dev_root_with_group_or_other_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let dev = temp.path().join(DEV_HOME_NAME);
    std::fs::create_dir(&dev).unwrap();
    std::fs::set_permissions(&dev, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(resolve_for_home(temp.path()).is_err());
}

#[test]
fn rejects_dev_subdirectory_symlink_into_production_build_channels() {
    let temp = tempfile::tempdir().unwrap();
    let paths = resolve_for_home(temp.path()).unwrap();
    std::fs::create_dir_all(&paths.home).unwrap();
    #[cfg(unix)]
    std::fs::set_permissions(&paths.home, std::fs::Permissions::from_mode(0o700)).unwrap();
    let production = temp.path().join(".jcode/builds/current");
    std::fs::create_dir_all(&production).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&production, &paths.runtime).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&production, &paths.runtime).unwrap();
    assert!(resolve_for_home(temp.path()).is_err());
}

#[test]
fn rejects_xdg_path_symlink_outside_the_dev_root() {
    let temp = tempfile::tempdir().unwrap();
    let paths = resolve_for_home(temp.path()).unwrap();
    std::fs::create_dir_all(&paths.home).unwrap();
    #[cfg(unix)]
    std::fs::set_permissions(&paths.home, std::fs::Permissions::from_mode(0o700)).unwrap();
    let production_config = temp.path().join(".config");
    std::fs::create_dir_all(&production_config).unwrap();
    std::fs::create_dir_all(paths.xdg_config.parent().unwrap()).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&production_config, &paths.xdg_config).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&production_config, &paths.xdg_config).unwrap();
    assert!(resolve_for_home(temp.path()).is_err());
}

#[test]
fn rejects_overlapping_parent_fallback_paths() {
    assert!(paths_overlap(
        Path::new("/home/u/.jcode-dev"),
        Path::new("/home/u")
    ));
    assert!(paths_overlap(
        Path::new("/home/u/.jcode/builds/current"),
        Path::new("/home/u/.jcode")
    ));
    assert!(!paths_overlap(
        Path::new("/home/u/.jcode-dev"),
        Path::new("/home/u/.jcode")
    ));
}

#[test]
fn validates_roots_and_fails_closed_on_inherited_path_overrides() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(setup_env(temp.path()));
    assert!(validate_environment().is_ok());
    for (name, bad) in [
        ("JCODE_HOME", temp.path().join(".jcode")),
        ("JCODE_INSTALL_DIR", temp.path().join(".local/bin")),
        ("JCODE_RUNTIME_DIR", temp.path().join(".jcode/run")),
        ("CARGO_TARGET_DIR", temp.path().join(".jcode/target")),
        ("XDG_CONFIG_HOME", temp.path().join(".config")),
        ("XDG_DATA_HOME", temp.path().join(".local/share")),
        ("XDG_STATE_HOME", temp.path().join(".local/state")),
        ("XDG_CACHE_HOME", temp.path().join(".cache")),
    ] {
        crate::env::set_var(name, bad);
        assert!(validate_environment().is_err(), "accepted inherited {name}");
        let paths = resolve_for_home(temp.path()).unwrap();
        crate::env::set_var("JCODE_HOME", &paths.home);
        crate::env::set_var("JCODE_INSTALL_DIR", &paths.install);
        crate::env::set_var("JCODE_RUNTIME_DIR", &paths.runtime);
        crate::env::set_var("CARGO_TARGET_DIR", &paths.target);
        crate::env::set_var("XDG_CONFIG_HOME", &paths.xdg_config);
        crate::env::set_var("XDG_DATA_HOME", &paths.xdg_data);
        crate::env::set_var("XDG_STATE_HOME", &paths.xdg_state);
        crate::env::set_var("XDG_CACHE_HOME", &paths.xdg_cache);
    }
}

#[test]
fn rejects_socket_override_outside_dev_runtime() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(setup_env(temp.path()));
    crate::env::set_var("JCODE_SOCKET", temp.path().join(".jcode/jcode.sock"));
    assert!(validate_environment().is_err());
    assert!(validate_socket_override(Some("relative.sock")).is_err());
    crate::env::set_var("JCODE_API_SOCKET", temp.path().join(".jcode/api.sock"));
    assert!(validate_environment().is_err());
}

#[test]
fn dev_marker_is_not_accepted_without_the_verified_dev_home() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(setup_env(temp.path()));
    crate::env::set_var("JCODE_HOME", temp.path().join(".jcode"));
    assert!(!launcher_requested());
    assert!(validate_environment().is_err());
}

#[test]
fn verified_dev_namespace_denies_global_integrations() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(setup_env(temp.path()));
    assert!(ensure_global_integrations_allowed("setup-hotkey").is_err());
    assert!(ensure_global_integrations_allowed("setup-launcher").is_err());
}

#[test]
fn provider_environment_isolated_from_inherited_keys_and_profiles() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(setup_env(temp.path()));
    crate::env::set_var("OPENAI_API_KEY", "inherited-test-secret");
    crate::env::set_var("CUSTOM_GATEWAY_KEY", "custom-test-secret");
    crate::env::set_var("CLAUDE_CODE_OAUTH_TOKEN", "inherited-oauth-secret");
    crate::env::set_var("CLAUDE_CONFIG_DIR", "/stable/.claude");
    crate::env::set_var("GROK_HOME", "/stable/.grok");
    crate::env::set_var("JCODE_GROK_CLI_PATH", "/stable/bin/grok");
    crate::env::set_var("JCODE_ANTHROPIC_HEADERS", "x-api-key: inherited");
    crate::env::set_var("JCODE_ANTHROPIC_AUTH_HEADER", "x-auth: inherited");
    crate::env::set_var("JCODE_ANTHROPIC_API_BASE", "https://prod.example/v1");
    crate::env::set_var("ANTHROPIC_BASE_URL", "https://prod.example/v1");
    crate::env::set_var("JCODE_OPENROUTER_API_BASE", "https://prod.example/v1");
    crate::env::set_var("AWS_SECRET_ACCESS_KEY", "inherited-aws-secret");
    crate::env::set_var("AWS_ACCESS_KEY_ID", "inherited-aws-access-id");
    crate::env::set_var("OPENAI_BASE_URL", "https://stable.example/v1");
    crate::env::set_var("OPENAI_API_BASE", "https://stable.example/v1");
    crate::env::set_var("JCODE_ANTHROPIC_ENV_FILE", "/stable/anthropic.env");
    crate::env::set_var("JCODE_OPENROUTER_ENV_FILE", "/stable/openrouter.env");
    crate::env::set_var("AWS_PROFILE", "production");
    crate::env::set_var("JCODE_COPILOT_ALLOW_GH_AUTH_TOKEN", "1");
    crate::env::set_var("JCODE_OPENROUTER_MAX_TOKENS", "4096");

    let paths = validate_environment().unwrap();
    isolate_provider_environment(&paths);

    for name in [
        "OPENAI_API_KEY",
        "CUSTOM_GATEWAY_KEY",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "CLAUDE_CONFIG_DIR",
        "JCODE_GROK_CLI_PATH",
        "JCODE_ANTHROPIC_HEADERS",
        "JCODE_ANTHROPIC_AUTH_HEADER",
        "JCODE_ANTHROPIC_API_BASE",
        "ANTHROPIC_BASE_URL",
        "JCODE_OPENROUTER_API_BASE",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_ACCESS_KEY_ID",
        "OPENAI_BASE_URL",
        "OPENAI_API_BASE",
        "JCODE_ANTHROPIC_ENV_FILE",
        "JCODE_OPENROUTER_ENV_FILE",
        "AWS_PROFILE",
        "JCODE_COPILOT_ALLOW_GH_AUTH_TOKEN",
    ] {
        assert!(
            std::env::var_os(name).is_none(),
            "inherited {name} survived"
        );
    }
    assert_eq!(
        std::env::var_os("AWS_CONFIG_FILE").unwrap(),
        paths.home.join("external/aws/config")
    );
    assert_eq!(
        std::env::var_os("GROK_HOME").unwrap(),
        paths.home.join(".grok")
    );
    assert!(std::env::var_os("JCODE_GROK_CLI_PATH").is_none());
    assert_eq!(
        std::env::var_os("AZURE_CONFIG_DIR").unwrap(),
        paths.home.join("external/azure")
    );
    assert_eq!(
        std::env::var_os("CLOUDSDK_CONFIG").unwrap(),
        paths.home.join("external/gcloud")
    );
    assert_eq!(std::env::var("AWS_EC2_METADATA_DISABLED").unwrap(), "true");
    assert_eq!(
        std::env::var("JCODE_OPENROUTER_MAX_TOKENS").unwrap(),
        "4096"
    );
}

#[test]
fn rejects_target_path_fallback_into_repo_or_production_home() {
    let temp = tempfile::tempdir().unwrap();
    let paths = resolve_for_home(temp.path()).unwrap();
    assert_ne!(paths.target, temp.path().join("target"));
    assert!(paths_overlap(
        &temp.path().join(".jcode/builds/shared-server"),
        &temp.path().join(".jcode")
    ));
}

#[cfg(unix)]
#[test]
fn legacy_import_copies_only_selected_session_without_credentials_or_shared_inode() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(setup_env(temp.path()));
    let source_dir = temp.path().join(".jcode/sessions");
    std::fs::create_dir_all(&source_dir).unwrap();
    let id = "session_test_123";
    let json = source_dir.join(format!("{id}.json"));
    let journal = source_dir.join(format!("{id}.journal.jsonl"));
    std::fs::write(&json, b"session record").unwrap();
    std::fs::write(&journal, b"journal record").unwrap();
    std::fs::write(temp.path().join(".jcode/auth.json"), b"credential").unwrap();

    assert_eq!(import_legacy_session(id).unwrap().len(), 2);
    assert_eq!(std::fs::read(&json).unwrap(), b"session record");
    assert_eq!(std::fs::read(&journal).unwrap(), b"journal record");
    let dev_home = resolve_for_home(temp.path()).unwrap().home;
    let imported = dev_home.join("sessions").join(format!("{id}.json"));
    assert_eq!(std::fs::read(&imported).unwrap(), b"session record");
    assert_eq!(
        std::fs::read_dir(dev_home.join("sessions"))
            .unwrap()
            .count(),
        2
    );
    assert!(!dev_home.join("external").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_ne!(
            std::fs::metadata(&imported).unwrap().ino(),
            std::fs::metadata(&json).unwrap().ino()
        );
    }
}

#[cfg(unix)]
#[test]
fn legacy_import_refuses_overwrite_and_path_traversal() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(setup_env(temp.path()));
    let source = temp.path().join(".jcode/sessions");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("session_safe.json"), b"original").unwrap();
    import_legacy_session("session_safe").unwrap();
    assert!(import_legacy_session("session_safe").is_err());
    assert!(import_legacy_session("../production").is_err());
    assert_eq!(
        std::fs::read(temp.path().join(".jcode-dev/sessions/session_safe.json")).unwrap(),
        b"original"
    );
}

#[cfg(unix)]
#[test]
fn legacy_import_retries_when_stale_stage_directory_exists() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(setup_env(temp.path()));
    let source = temp.path().join(".jcode/sessions");
    let destination = temp.path().join(".jcode-dev/sessions");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&destination).unwrap();
    std::fs::set_permissions(
        destination.parent().unwrap(),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(source.join("session_retry.json"), b"session").unwrap();
    let stale = destination.join(format!(".import-session_retry-{}", std::process::id()));
    std::fs::create_dir(&stale).unwrap();

    let imported = import_legacy_session("session_retry").unwrap();
    assert_eq!(imported, vec![destination.join("session_retry.json")]);
    assert!(stale.is_dir());
    assert_eq!(std::fs::read(&imported[0]).unwrap(), b"session");
}

#[cfg(unix)]
#[test]
fn legacy_import_refuses_symlink_source_file() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(setup_env(temp.path()));
    let source = temp.path().join(".jcode/sessions");
    std::fs::create_dir_all(&source).unwrap();
    let protected = temp.path().join("protected.json");
    std::fs::write(&protected, b"protected").unwrap();
    std::os::unix::fs::symlink(&protected, source.join("session_link.json")).unwrap();
    assert!(import_legacy_session("session_link").is_err());
    assert_eq!(std::fs::read(&protected).unwrap(), b"protected");
    assert!(
        !temp
            .path()
            .join(".jcode-dev/sessions/session_link.json")
            .exists()
    );
}
