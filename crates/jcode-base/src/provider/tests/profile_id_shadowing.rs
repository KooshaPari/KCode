// Provider-activation precedence tests, focused on shadowed profile ids.
// Included from provider/tests.rs.

/// A user `[providers.<id>]` block must win over a built-in login-provider
/// profile that happens to share the same id.
///
/// Regression: provider activation resolved the built-in profile FIRST and
/// that resolver always returns `Some` for a known id, so the branch that
/// reads the user's config block was unreachable dead code. A user who named
/// their gateway `openai-compatible` therefore had their `base_url`, api-key
/// env, and declared models silently discarded, and the model picker showed
/// only the built-in profile's catalog.
#[test]
fn user_config_block_shadowing_a_builtin_id_wins_at_activation() {
    let _lock = crate::storage::lock_test_env();
    register_test_external_runtimes();
    let temp = tempfile::TempDir::new().expect("create temp home");
    let jcode_home = temp.path().join("kcode-home");
    let _jcode_home = OrEnvVarGuard::set("KCODE_HOME", &jcode_home);
    let _home = OrEnvVarGuard::set("HOME", temp.path());
    let _appdata = OrEnvVarGuard::set("APPDATA", temp.path().join("AppData").join("Roaming"));
    let _env = isolate_openrouter_autodetect_env_or();
    // Set AFTER the isolate guard, which clears profile api-key env vars.
    let _key = OrEnvVarGuard::set("SHADOWED_GATEWAY_API_KEY", "shadowed-key");

    std::fs::create_dir_all(&jcode_home).expect("create test config dir");
    // The section id deliberately collides with the built-in
    // "openai-compatible" login provider.
    std::fs::write(
        jcode_home.join("config.toml"),
        r#"
[provider]
default_provider = "openai-compatible"

[providers.openai-compatible]
type = "open-ai-compatible"
base_url = "https://user-gateway.example.com/v1"
api_key_env = "SHADOWED_GATEWAY_API_KEY"
default_model = "MiniMax-M3.1-Flash-Preview"
model_catalog = false

[[providers.openai-compatible.models]]
id = "MiniMax-M3.1-Flash-Preview"
context_window = 1000000
"#,
    )
    .expect("write test config");
    crate::config::invalidate_config_cache();

    let _provider = MultiProvider::new_with_auth_status(crate::auth::AuthStatus::default());

    // The user's base_url must be applied, not the built-in api.openai.com.
    assert_eq!(
        std::env::var("KCODE_OPENROUTER_API_BASE").ok().as_deref(),
        Some("https://user-gateway.example.com/v1"),
        "a user config block must override the built-in profile sharing its id"
    );
    assert_eq!(
        std::env::var("KCODE_OPENROUTER_API_KEY_NAME")
            .ok()
            .as_deref(),
        Some("SHADOWED_GATEWAY_API_KEY"),
        "the user's api-key env must be applied, not the built-in default"
    );
    // And the declared model must reach the picker surface rather than
    // being dropped in favour of the built-in static catalog.
    assert_eq!(
        std::env::var("KCODE_OPENROUTER_MODEL").ok().as_deref(),
        Some("MiniMax-M3.1-Flash-Preview")
    );
    assert!(
        std::env::var("KCODE_OPENROUTER_STATIC_MODELS")
            .ok()
            .as_deref()
            .is_some_and(|models| models.contains("MiniMax-M3.1-Flash-Preview")),
        "the user's declared model must survive activation so the picker can list it"
    );
    assert_eq!(
        std::env::var("KCODE_PROVIDER_PROFILE_NAME").ok().as_deref(),
        Some("openai-compatible"),
        "the user profile must be marked active"
    );
}

/// Regression test for the reported symptom: `jcode --provider auto` (the
/// default) resolved to Cursor/gpt-5 even though `config.toml` declared
/// `default_provider = "openai-compatible"` with a working profile.
///
/// `auto_default_provider` ranks Cursor ahead of OpenRouter, so the bug is only
/// observable when Cursor credentials are ALSO present. Here we hand
/// `new_with_auth_status` an `AuthStatus` that reports Cursor available (the
/// decision layer consumes this struct, not the live credential files), and
/// assert the configured default still wins.
#[test]
fn config_default_provider_wins_over_cursor_in_auto_selection() {
    let _lock = crate::storage::lock_test_env();
    register_test_external_runtimes();
    // Force a reload so this test never reads a config cached by an earlier
    // test in the same process, and invalidate again on the way out.
    crate::config::invalidate_config_cache();
    let temp = tempfile::TempDir::new().expect("create temp home");
    let jcode_home = temp.path().join("kcode-home");
    let _jcode_home = OrEnvVarGuard::set("KCODE_HOME", &jcode_home);
    let _home = OrEnvVarGuard::set("HOME", temp.path());
    let _appdata = OrEnvVarGuard::set("APPDATA", temp.path().join("AppData").join("Roaming"));
    let _env = isolate_openrouter_autodetect_env_or();
    // Set AFTER the isolate guard, which clears profile api-key env vars.
    let _key = OrEnvVarGuard::set("SHADOWED_GATEWAY_API_KEY", "shadowed-key");
    // An explicit provider pin must not be inherited from the ambient shell.
    let _initial_provider = OrEnvVarGuard::remove("KCODE_ACTIVE_PROVIDER");
    let _initial_explicit = OrEnvVarGuard::remove("KCODE_INITIAL_PROVIDER_EXPLICIT");

    std::fs::create_dir_all(&jcode_home).expect("create test config dir");
    std::fs::write(
        jcode_home.join("config.toml"),
        r#"
[provider]
default_provider = "openai-compatible"
default_model = "MiniMax-M3.1-Flash-Preview"

[providers.openai-compatible]
type = "open-ai-compatible"
base_url = "https://user-gateway.example.com/v1"
api_key_env = "SHADOWED_GATEWAY_API_KEY"
default_model = "MiniMax-M3.1-Flash-Preview"
model_catalog = false

[[providers.openai-compatible.models]]
id = "MiniMax-M3.1-Flash-Preview"
context_window = 1000000
"#,
    )
    .expect("write test config");
    crate::config::invalidate_config_cache();

    // Cursor credentials are present, which is exactly the collision that made
    // auto pick Cursor and discard the configured default.
    let mut auth_status = crate::auth::AuthStatus::default();
    auth_status.cursor = crate::auth::AuthState::Available;

    let provider = MultiProvider::new_with_auth_status(auth_status);

    assert_eq!(
        provider.active_provider(),
        ActiveProvider::OpenRouter,
        "auto must honor the configured default_provider even when Cursor credentials exist"
    );
    assert_eq!(
        std::env::var("KCODE_OPENROUTER_API_BASE").ok().as_deref(),
        Some("https://user-gateway.example.com/v1"),
        "the configured profile's own base_url must be applied"
    );
    assert_eq!(
        std::env::var("KCODE_OPENROUTER_API_KEY_NAME")
            .ok()
            .as_deref(),
        Some("SHADOWED_GATEWAY_API_KEY"),
        "the configured profile's own api_key_env must be applied"
    );

    // Never leak this test's temp-home config into the next test in the process.
    crate::config::invalidate_config_cache();
}
