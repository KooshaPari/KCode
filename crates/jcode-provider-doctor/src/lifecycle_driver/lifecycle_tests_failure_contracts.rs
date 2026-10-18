//! Degraded-state rejection tests. These tests pin the
//! `assert_success` contract's negative side: every "degraded
//! success" state the production code must NOT accept as happy
//! gets a focused scenario that verifies the `assert_success`
//! call panics with the expected marker.
//!
//! The shared `assert_rejected_success` helper lives in
//! `crate::lifecycle_contracts` (so production and tests share
//! one definition); this file wires together the four canonical
//! degraded scenarios plus a transcript-marker ordering
//! regression and a static-fallback source-attribution
//! regression.
//!
//! Size: ~150L panic budget; current 137L.

use jcode_base::auth::lifecycle::validate_catalog_invariants;

use super::live_helpers::stale_openai_route;
use super::{AuthLifecycleAuthPath, AuthLifecycleDriver, AuthLifecycleSpec, PickerSnapshot};
use crate::lifecycle_contracts::assert_rejected_success;

#[test]
fn auth_lifecycle_failure_contracts_reject_degraded_success_states() {
    let driver = AuthLifecycleDriver::new().expect("driver");
    let spec = AuthLifecycleSpec::cerebras_fixture(AuthLifecycleAuthPath::RemoteTuiPasteApiKey);
    let success = driver
        .run_openai_compatible_fixture(&spec)
        .expect("lifecycle result");

    // (1) Invalid API key
    let mut invalid_key = success.clone();
    invalid_key.transcript.push(
        "**Login: Cerebras failed**\n\nInvalid API key. No model catalog was activated."
            .to_string(),
    );
    assert_rejected_success(&spec, invalid_key, "invalid api key", "failed");

    // (2) Network catalog failure pending state
    let mut network_failure = success.clone();
    network_failure.transcript = vec![
        format!("**{} API key saved.**", spec.provider_label),
        "**Auth Change Received**\n\nThe server is reloading provider credentials.".to_string(),
        "**Model Discovery Still Updating**\n\nCould not fetch the live catalog yet; waiting for server refresh."
            .to_string(),
    ];
    assert_rejected_success(
        &spec,
        network_failure,
        "network catalog failure pending state",
        "Model Discovery Still Updating",
    );

    // (3) Empty catalog
    let mut empty_catalog = success.clone();
    empty_catalog.catalog_routes.clear();
    empty_catalog.catalog_report = validate_catalog_invariants(
        &empty_catalog.activation,
        empty_catalog.picker.selected_model.as_deref(),
        &empty_catalog.catalog_routes,
    );
    empty_catalog.picker = PickerSnapshot::build(
        &spec,
        &empty_catalog.activation,
        empty_catalog.picker.selected_model.as_deref(),
        &empty_catalog.catalog_routes,
    );
    assert_rejected_success(&spec, empty_catalog, "empty catalog", "catalog invariant");

    // (4) Wrong provider profile catalog
    let mut wrong_profile = success.clone();
    for route in &mut wrong_profile.catalog_routes {
        route.api_method = "openai-compatible:other-provider".to_string();
        route.detail = "wrong profile live-catalog route".to_string();
    }
    wrong_profile.catalog_report = validate_catalog_invariants(
        &wrong_profile.activation,
        wrong_profile.picker.selected_model.as_deref(),
        &wrong_profile.catalog_routes,
    );
    wrong_profile.picker = PickerSnapshot::build(
        &spec,
        &wrong_profile.activation,
        wrong_profile.picker.selected_model.as_deref(),
        &wrong_profile.catalog_routes,
    );
    assert_rejected_success(
        &spec,
        wrong_profile,
        "wrong provider profile catalog",
        "catalog invariant",
    );

    // (5) Stale cached OpenAI catalog
    let mut stale_cached_catalog = success.clone();
    stale_cached_catalog.catalog_routes = vec![stale_openai_route("gpt-5.5")];
    stale_cached_catalog.catalog_report = validate_catalog_invariants(
        &stale_cached_catalog.activation,
        Some("gpt-5.5"),
        &stale_cached_catalog.catalog_routes,
    );
    stale_cached_catalog.picker = PickerSnapshot::build(
        &spec,
        &stale_cached_catalog.activation,
        Some("gpt-5.5"),
        &stale_cached_catalog.catalog_routes,
    );
    assert_rejected_success(
        &spec,
        stale_cached_catalog,
        "stale cached OpenAI catalog",
        "catalog invariant",
    );
}

#[test]
fn auth_lifecycle_failure_contracts_reject_logged_in_but_no_catalog_marker() {
    // The "Saved the API key and fetched the model catalog, but..." warning
    // is forbidden by `assert_success`. This test re-uses the
    // `assert_rejected_success` helper from the contracts module.
    let driver = AuthLifecycleDriver::new().expect("driver");
    let spec = AuthLifecycleSpec::cerebras_fixture(AuthLifecycleAuthPath::RemoteTuiPasteApiKey);
    let mut result = driver
        .run_openai_compatible_fixture(&spec)
        .expect("lifecycle result");
    result.transcript[0] = "**Cerebras API Key**\n\nSaved the API key and fetched the model catalog, but the live catalog was empty.".to_string();
    assert_rejected_success(
        &spec,
        result,
        "saved but no-catalog warning",
        "Saved the API key and fetched the model catalog, but",
    );
}
