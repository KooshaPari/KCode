//! Live-test helpers shared by every `#[tokio::test]` in
//! `lifecycle_driver`'s submodules. Pulled out of the test files so
//! each test file can stay under the 500L panic budget and so the
//! helpers are also reachable from any future live-test module
//! without re-defining them.
//!
//! All helpers are gated by `KCODE_AUTH_LIFECYCLE_LIVE` /
//! `KCODE_OPENCODE_ZEN_LIVE_TOOL_TEST` / `KCODE_ISSUE_DRIVEN_LIVE_PROVIDER`
//! environment variables; tests that are missing the env-gates
//! short-circuit with a `eprintln!` and return, so a developer can
//! run the suite without live credentials without paying the
//! network round-trip cost.
//!
//! Size: ~180L panic budget; current 178L.

use jcode_base::live_tests::{
    LiveVerificationAuth, LiveVerificationEvent, LiveVerificationResult, LiveVerificationStage,
    LiveVerificationStageStatus, append_event, checkpoints, concise_model_sample,
};
use jcode_base::provider::ModelRoute;
use jcode_base::provider_catalog::OpenAiCompatibleProfile;

pub(crate) struct LiveTestApiKey {
    pub(crate) secret: String,
    pub(crate) auth: LiveVerificationAuth,
}

pub(crate) fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| {
            let value = value.trim();
            !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
        })
        .unwrap_or(false)
}

pub(crate) fn live_cerebras_api_key() -> Option<LiveTestApiKey> {
    std::env::var("KCODE_AUTH_LIFECYCLE_CEREBRAS_API_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|secret| LiveTestApiKey {
            auth: LiveVerificationAuth::from_secret(
                "env:KCODE_AUTH_LIFECYCLE_CEREBRAS_API_KEY",
                Some("KCODE_AUTH_LIFECYCLE_CEREBRAS_API_KEY"),
                &secret,
            ),
            secret,
        })
}

pub(crate) fn live_opencode_zen_api_key() -> Option<LiveTestApiKey> {
    if let Some(secret) = std::env::var("KCODE_AUTH_LIFECYCLE_OPENCODE_API_KEY")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return Some(LiveTestApiKey {
            auth: LiveVerificationAuth::from_secret(
                "env:KCODE_AUTH_LIFECYCLE_OPENCODE_API_KEY",
                Some("KCODE_AUTH_LIFECYCLE_OPENCODE_API_KEY"),
                &secret,
            ),
            secret,
        });
    }

    let resolved = jcode_base::provider_catalog::resolve_openai_compatible_profile(
        jcode_base::provider_catalog::OPENCODE_PROFILE,
    );
    jcode_base::provider_catalog::load_api_key_from_env_or_config(
        &resolved.api_key_env,
        &resolved.env_file,
    )
    .map(|secret| LiveTestApiKey {
        auth: LiveVerificationAuth::from_secret(
            format!("{} via {}", resolved.api_key_env, resolved.env_file),
            Some(resolved.api_key_env.clone()),
            &secret,
        ),
        secret,
    })
}

pub(crate) fn live_openai_compatible_api_key(
    profile: OpenAiCompatibleProfile,
) -> Option<LiveTestApiKey> {
    let resolved = jcode_base::provider_catalog::resolve_openai_compatible_profile(profile);
    jcode_base::provider_catalog::load_api_key_from_env_or_config(
        &resolved.api_key_env,
        &resolved.env_file,
    )
    .map(|secret| LiveTestApiKey {
        auth: LiveVerificationAuth::from_secret(
            format!("{} via {}", resolved.api_key_env, resolved.env_file),
            Some(resolved.api_key_env.clone()),
            &secret,
        ),
        secret,
    })
}

pub(crate) fn live_event<I, S>(
    test_name: &str,
    profile: OpenAiCompatibleProfile,
    auth: LiveVerificationAuth,
    model: Option<&str>,
    capabilities: I,
    result: LiveVerificationResult,
) -> LiveVerificationEvent
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let resolved = jcode_base::provider_catalog::resolve_openai_compatible_profile(profile);
    let mut event = LiveVerificationEvent::new(
        test_name,
        resolved.id,
        resolved.display_name,
        auth,
        result,
    )
    .with_endpoint(resolved.api_base)
    .with_capabilities(capabilities)
    .with_standard_end_to_end_checkpoints()
    .with_metadata("cost_policy", serde_json::json!("env_gated_may_spend_balance"));
    if let Some(model) = model {
        event = event.with_model(model);
    }
    event
}

pub(crate) fn append_live_event(event: &LiveVerificationEvent) {
    let event = event.clone().with_not_run_for_missing_expected_checkpoints(
        "checkpoint not exercised by this live test invocation",
    );
    let paths = append_event(&event).expect("append live verification evidence ledger");
    eprintln!(
        "live verification recorded: events={} coverage={} user_ready={} gaps={:?}",
        paths.events.display(),
        paths.coverage.display(),
        event.user_ready(),
        event.readiness_gaps()
    );
}

pub(crate) fn cost_quota_safety_stage(spend_enabled: bool) -> LiveVerificationStage {
    LiveVerificationStage::passed(checkpoints::COST_QUOTA_SAFETY)
        .with_evidence("live_tests_env_gated", serde_json::json!(true))
        .with_evidence("spend_enabled", serde_json::json!(spend_enabled))
        .with_evidence("auth_secret_logged", serde_json::json!(false))
        .with_evidence("usage_cost_recorded_when_available", serde_json::json!(true))
}

pub(crate) fn covered_stage_names(stages: &[LiveVerificationStage]) -> Vec<String> {
    stages
        .iter()
        .filter(|stage| stage.status != LiveVerificationStageStatus::NotRun)
        .map(|stage| stage.name.clone())
        .collect()
}

pub(crate) fn stale_openai_route(model: &str) -> ModelRoute {
    ModelRoute {
        model: model.to_string(),
        provider: "OpenAI".to_string(),
        api_method: "openai".to_string(),
        available: true,
        detail: "stale route".to_string(),
        usage: None,
        cheapness: None,
    }
}

pub(crate) fn concise_model_sample_for(
    models: &[String],
    limit: usize,
) -> serde_json::Value {
    serde_json::json!(concise_model_sample(models, limit))
}
