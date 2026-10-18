//! Offline provider-matrix tests for the `AuthLifecycleDriver`.
//! These tests run without network and exist to pin the behaviour of
//! the full OpenAI-compatible provider matrix (every
//! `OpenAiCompatibleProfile` × every `AuthLifecycleAuthPath`) and
//! the cross-provider reauth matrix.
//!
//! Size: 500L panic budget.

use jcode_base::auth::lifecycle::{
    AuthActivationRequest, activate_auth_change, provider_model_to_select_after_auth,
    validate_catalog_invariants,
};
use jcode_base::protocol::{AuthChanged, CatalogNamespace, RuntimeProviderKey};
use jcode_base::provider::ModelRoute;

use super::live_helpers::stale_openai_route;
use super::{AuthLifecycleAuthPath, AuthLifecycleDriver, AuthLifecycleSpec, PickerSnapshot};
use crate::lifecycle_contracts::assert_rejected_success;

fn is_native_routed_compat_profile(
    profile: &jcode_base::provider_catalog::OpenAiCompatibleProfile,
) -> bool {
    jcode_base::auth::lifecycle::normalized_auth_provider_id(Some(profile.id))
        .is_some_and(|canonical| canonical != profile.id)
}

#[test]
fn cerebras_remote_tui_paste_key_fixture_covers_catalog_picker_and_switch() {
    let driver = AuthLifecycleDriver::new().expect("driver");
    let spec = AuthLifecycleSpec::cerebras_fixture(AuthLifecycleAuthPath::RemoteTuiPasteApiKey);

    let result = driver
        .run_openai_compatible_fixture(&spec)
        .expect("lifecycle result");

    result.assert_success(&spec);
    assert!(result.transcript_text().contains("**Cerebras API Key**"));
    assert!(
        result
            .transcript_text()
            .contains("**Cerebras API key saved.**")
    );
    assert_eq!(
        result.picker.selected_model.as_deref(),
        Some("qwen-3-235b-a22b-instruct-2507")
    );
    assert_eq!(result.picker.switch_target.as_deref(), Some("llama3.1-8b"));
    assert_eq!(
        result.picker.switch_request.as_deref(),
        Some("cerebras:llama3.1-8b")
    );
}

#[test]
fn cerebras_state_space_catches_stale_openai_catalog_after_auth() {
    let driver = AuthLifecycleDriver::new().expect("driver");
    let mut spec =
        AuthLifecycleSpec::cerebras_fixture(AuthLifecycleAuthPath::RemoteTuiPasteApiKey);
    spec.catalog_models_after_auth.clear();
    spec.selected_model_override = Some("gpt-5.5".to_string());

    let mut result = driver
        .run_openai_compatible_fixture(&spec)
        .expect("lifecycle result");
    result.catalog_routes = vec![stale_openai_route("gpt-5.5")];
    result.catalog_report = validate_catalog_invariants(
        &result.activation,
        result.picker.selected_model.as_deref(),
        &result.catalog_routes,
    );
    result.picker = PickerSnapshot::build(
        &spec,
        &result.activation,
        result.picker.selected_model.as_deref(),
        &result.catalog_routes,
    );

    assert!(!result.catalog_report.ok());
    let failure = result.failure_report(&spec);
    assert!(failure.contains("Expected selectable Cerebras model routes"));
    assert!(failure.contains("Selected model: `gpt-5.5`"));
    assert!(failure.contains("OpenAI"));
}

#[test]
fn cerebras_env_file_and_process_env_paths_share_same_lifecycle_invariants() {
    for auth_path in [
        AuthLifecycleAuthPath::TuiPasteApiKey,
        AuthLifecycleAuthPath::CliLogin,
        AuthLifecycleAuthPath::EnvFilePreseeded,
        AuthLifecycleAuthPath::ProcessEnvPreseeded,
    ] {
        let driver = AuthLifecycleDriver::new().expect("driver");
        let spec = AuthLifecycleSpec::cerebras_fixture(auth_path);

        let result = driver
            .run_openai_compatible_fixture(&spec)
            .expect("lifecycle result");

        result.assert_success(&spec);
        if auth_path.shows_paste_prompt() {
            assert!(result.transcript_text().contains("**Cerebras API Key**"));
        } else {
            assert!(
                result
                    .transcript_text()
                    .contains("**Cerebras credentials detected.**")
            );
        }
    }
}

#[test]
fn openai_compatible_provider_matrix_preserves_identity_catalog_and_picker() {
    let auth_paths = [
        AuthLifecycleAuthPath::RemoteTuiPasteApiKey,
        AuthLifecycleAuthPath::CliLogin,
        AuthLifecycleAuthPath::EnvFilePreseeded,
        AuthLifecycleAuthPath::ProcessEnvPreseeded,
    ];

    for profile in jcode_base::provider_catalog::openai_compatible_profiles()
        .iter()
        .copied()
        .filter(|profile| !is_native_routed_compat_profile(profile))
    {
        for auth_path in auth_paths {
            let driver = AuthLifecycleDriver::new().unwrap_or_else(|error| {
                panic!(
                    "driver for provider {} via {:?}: {error:?}",
                    profile.id, auth_path
                )
            });
            let spec = AuthLifecycleSpec::openai_compatible_fixture(profile, auth_path);

            let result = driver
                .run_openai_compatible_fixture(&spec)
                .unwrap_or_else(|error| {
                    panic!(
                        "lifecycle setup failed for provider {} via {:?}: {error:?}",
                        profile.id, auth_path
                    )
                });

            result.assert_success(&spec);
            assert!(
                result
                    .picker
                    .switch_request
                    .as_deref()
                    .is_some_and(|request| request.starts_with(&format!("{}:", profile.id))),
                "{}",
                result.failure_report(&spec)
            );
        }
    }
}

#[test]
fn provider_switch_reauth_matrix_recovers_from_stale_previous_provider_state() {
    // Native-routed profiles (Anthropic/OpenAI API-key) deliberately use a
    // native runtime route, not the generic `openai-compatible:<id>` one, so
    // exclude them from this generic switch/reauth contract.
    let profiles: Vec<_> = jcode_base::provider_catalog::openai_compatible_profiles()
        .iter()
        .copied()
        .filter(|profile| !is_native_routed_compat_profile(profile))
        .collect();
    assert!(
        profiles.len() >= 2,
        "switch/reauth matrix needs at least two OpenAI-compatible providers"
    );

    for window in profiles.windows(2) {
        let previous_profile = window[0];
        let reauth_profile = window[1];
        let driver = AuthLifecycleDriver::new().expect("driver");
        let previous_spec = AuthLifecycleSpec::openai_compatible_fixture(
            previous_profile,
            AuthLifecycleAuthPath::RemoteTuiPasteApiKey,
        );
        let reauth_spec = AuthLifecycleSpec::openai_compatible_fixture(
            reauth_profile,
            AuthLifecycleAuthPath::RemoteTuiPasteApiKey,
        );

        let previous = driver
            .run_openai_compatible_fixture(&previous_spec)
            .unwrap_or_else(|error| {
                panic!(
                    "previous provider {} setup failed: {error:?}",
                    previous_spec.provider_id
                )
            });
        let reauth = driver
            .run_openai_compatible_fixture(&reauth_spec)
            .unwrap_or_else(|error| {
                panic!(
                    "reauth provider {} setup failed: {error:?}",
                    reauth_spec.provider_id
                )
            });

        let stale_selected_model = previous.picker.selected_model.as_deref();
        let mut mixed_routes = previous.catalog_routes.clone();
        mixed_routes.extend(reauth.catalog_routes.clone());
        let session_model_after_reauth = provider_model_to_select_after_auth(
            &reauth.activation,
            stale_selected_model,
            &mixed_routes,
        )
        .or_else(|| stale_selected_model.map(ToString::to_string));
        let catalog_report = validate_catalog_invariants(
            &reauth.activation,
            session_model_after_reauth.as_deref(),
            &mixed_routes,
        );
        let picker = PickerSnapshot::build(
            &reauth_spec,
            &reauth.activation,
            session_model_after_reauth.as_deref(),
            &mixed_routes,
        );

        assert!(
            catalog_report.ok(),
            "reauth of {} after {} left stale selected/catalog state: {:?}",
            reauth_spec.provider_id,
            previous_spec.provider_id,
            catalog_report.warning_message()
        );
        assert!(
            session_model_after_reauth
                .as_ref()
                .is_some_and(|selected| picker
                    .provider_entries
                    .iter()
                    .any(|entry| entry == selected)),
            "reauth of {} after {} selected {:?}, picker entries {:?}",
            reauth_spec.provider_id,
            previous_spec.provider_id,
            session_model_after_reauth,
            picker.provider_entries
        );
        assert!(
            picker.provider_entries.iter().all(|entry| reauth
                .catalog_routes
                .iter()
                .any(|route| route.model == *entry)),
            "reauth picker for {} leaked previous provider {} entries: {:?}",
            reauth_spec.provider_id,
            previous_spec.provider_id,
            picker.provider_entries
        );
        assert!(
            picker.switch_request.as_deref().is_some_and(
                |request| request.starts_with(&format!("{}:", reauth_spec.provider_id))
            ),
            "reauth picker switch must target {}, got {:?}",
            reauth_spec.provider_id,
            picker.switch_request
        );
    }
}

#[test]
fn picker_switch_target_uses_profile_route_not_matching_label_only_route() {
    let spec = AuthLifecycleSpec::cerebras_fixture(AuthLifecycleAuthPath::RemoteTuiPasteApiKey);
    let auth = AuthChanged {
        provider: jcode_base::protocol::AuthProviderId::new(spec.provider_id),
        credential_source: Some(spec.auth_path.credential_source()),
        auth_method: Some(spec.auth_path.auth_method()),
        expected_runtime: Some(RuntimeProviderKey::new("openai-compatible")),
        expected_catalog_namespace: Some(CatalogNamespace::new(spec.provider_id)),
    };
    let activation = activate_auth_change(&AuthActivationRequest::new(None, Some(auth)));
    let routes = vec![
        ModelRoute {
            model: "wrong-profile-first".to_string(),
            provider: "Cerebras".to_string(),
            api_method: "openai-compatible:other-provider".to_string(),
            available: true,
            detail: "wrong namespace".to_string(),
            usage: None,
            cheapness: None,
        },
        ModelRoute {
            model: "qwen-3-235b-a22b-instruct-2507".to_string(),
            provider: "Cerebras".to_string(),
            api_method: "openai-compatible:cerebras".to_string(),
            available: true,
            detail: "correct namespace".to_string(),
            usage: None,
            cheapness: None,
        },
        ModelRoute {
            model: "llama3.1-8b".to_string(),
            provider: "Cerebras".to_string(),
            api_method: "openai-compatible:cerebras".to_string(),
            available: true,
            detail: "correct namespace".to_string(),
            usage: None,
            cheapness: None,
        },
    ];

    let picker = PickerSnapshot::build(
        &spec,
        &activation,
        Some("qwen-3-235b-a22b-instruct-2507"),
        &routes,
    );

    assert_eq!(
        picker.provider_entries,
        vec![
            "qwen-3-235b-a22b-instruct-2507".to_string(),
            "llama3.1-8b".to_string()
        ]
    );
    assert_eq!(picker.switch_target.as_deref(), Some("llama3.1-8b"));
    assert_eq!(
        picker.switch_route_api_method.as_deref(),
        Some("openai-compatible:cerebras")
    );
    assert!(
        !picker
            .provider_entries
            .iter()
            .any(|model| model == "wrong-profile-first")
    );
}

#[test]
fn auth_lifecycle_success_rejects_static_fallback_route_sources() {
    let driver = AuthLifecycleDriver::new().expect("driver");
    let spec = AuthLifecycleSpec::cerebras_fixture(AuthLifecycleAuthPath::RemoteTuiPasteApiKey);
    let mut result = driver
        .run_openai_compatible_fixture(&spec)
        .expect("lifecycle result");
    for route in &mut result.catalog_routes {
        route.detail = "fixture static fallback route".to_string();
    }

    assert!(
        result.catalog_report.ok(),
        "the catalog shape is valid, so only source attribution should fail"
    );
    assert_rejected_success(
        &spec,
        result,
        "static fallback route source",
        "static fallback",
    );
}

#[test]
fn auth_lifecycle_success_rejects_provider_routes_not_returned_by_live_catalog() {
    let driver = AuthLifecycleDriver::new().expect("driver");
    let spec = AuthLifecycleSpec::cerebras_fixture(AuthLifecycleAuthPath::RemoteTuiPasteApiKey);
    let mut result = driver
        .run_openai_compatible_fixture(&spec)
        .expect("lifecycle result");
    result.catalog_routes.push(ModelRoute {
        model: "zai-glm-4.7".to_string(),
        provider: "Cerebras".to_string(),
        api_method: "openai-compatible:cerebras".to_string(),
        available: true,
        detail: "https://api.cerebras.ai/v1".to_string(),
        usage: None,
        cheapness: None,
    });
    result.catalog_report = validate_catalog_invariants(
        &result.activation,
        result.picker.selected_model.as_deref(),
        &result.catalog_routes,
    );
    result.picker = PickerSnapshot::build(
        &spec,
        &result.activation,
        result.picker.selected_model.as_deref(),
        &result.catalog_routes,
    );

    assert!(
        result.catalog_report.ok(),
        "the route shape is valid, so only live-catalog membership should fail"
    );
    assert!(
        result
            .picker
            .provider_entries
            .iter()
            .any(|model| model == "zai-glm-4.7"),
        "test setup should mimic a stale static/provider route leaking into /model"
    );
    assert_rejected_success(
        &spec,
        result,
        "provider route absent from live catalog",
        "not returned by the live catalog",
    );
}

#[test]
fn auth_lifecycle_success_rejects_duplicate_or_out_of_order_transcript_markers() {
    let driver = AuthLifecycleDriver::new().expect("driver");
    let spec = AuthLifecycleSpec::cerebras_fixture(AuthLifecycleAuthPath::RemoteTuiPasteApiKey);
    let result = driver
        .run_openai_compatible_fixture(&spec)
        .expect("lifecycle result");

    let mut duplicated = result.clone();
    duplicated
        .transcript
        .push("**Model ready:** `duplicate`\nDuplicate final success.".to_string());
    assert_rejected_success(
        &spec,
        duplicated,
        "duplicate final catalog update",
        "duplicate",
    );

    let mut out_of_order = result.clone();
    out_of_order.transcript.swap(1, 2);
    assert_rejected_success(
        &spec,
        out_of_order,
        "out-of-order auth transcript",
        "out of order",
    );
}

#[test]
fn fresh_start_sandbox_is_unconfigured_then_tui_key_lifecycle_configures_provider() {
    let driver = AuthLifecycleDriver::new().expect("driver");
    let spec = AuthLifecycleSpec::cerebras_fixture(AuthLifecycleAuthPath::TuiPasteApiKey);
    let resolved =
        jcode_base::provider_catalog::resolve_openai_compatible_profile(spec.profile);
    let env_file = driver.sandbox.env_file_path(&resolved.env_file);
    let provider = jcode_base::provider_catalog::resolve_login_provider(spec.provider_id)
        .expect("Cerebras login provider descriptor");

    assert!(
        !env_file.exists(),
        "fresh sandbox should not start with a provider env file: {}",
        env_file.display()
    );
    assert_eq!(
        jcode_base::provider_catalog::load_api_key_from_env_or_config(
            &resolved.api_key_env,
            &resolved.env_file,
        ),
        None,
        "fresh sandbox should not inherit credentials from the developer machine"
    );
    assert!(
        !jcode_base::provider_catalog::openai_compatible_profile_is_configured(spec.profile),
        "fresh sandbox should report the provider as unconfigured before setup"
    );
    jcode_base::auth::AuthStatus::invalidate_cache();
    assert_eq!(
        jcode_base::auth::AuthStatus::check_fast().state_for_provider(provider),
        jcode_base::auth::AuthState::NotConfigured
    );

    let result = driver
        .run_openai_compatible_fixture(&spec)
        .expect("fresh-start TUI paste-key lifecycle");

    result.assert_success(&spec);
    assert!(
        env_file.exists(),
        "TUI paste-key lifecycle should create env file"
    );
    assert_eq!(
        jcode_base::provider_catalog::load_api_key_from_env_or_config(
            &resolved.api_key_env,
            &resolved.env_file,
        )
        .as_deref(),
        Some(spec.api_key.as_str())
    );
    assert!(
        result
            .transcript_text()
            .contains("**Cerebras API key saved.**"),
        "fresh-start lifecycle should show the user that the key was saved: {}",
        result.transcript_text()
    );
    jcode_base::auth::AuthStatus::invalidate_cache();
    assert_eq!(
        jcode_base::auth::AuthStatus::check_fast().state_for_provider(provider),
        jcode_base::auth::AuthState::Available
    );
}

