//! Live env-gated Cerebras auth-lifecycle test. Skips silently when
//! the `KCODE_AUTH_LIFECYCLE_LIVE` env var is unset, otherwise
//! fetches the live Cerebras model catalog, drives the auth
//! lifecycle, and records a `LiveVerificationEvent` to the
//! evidence ledger. Optional `KCODE_AUTH_LIFECYCLE_SMOKE` and
//! `KCODE_AUTH_LIFECYCLE_STREAM_SMOKE` env vars also exercise a
//! non-streaming and a streaming chat-completion probe against
//! Cerebras.
//!
//! Size: ~220L panic budget; current 218L.

use jcode_base::live_tests::{LiveVerificationStage, checkpoints, concise_model_sample};

use super::live_helpers::{
    LiveTestApiKey, append_live_event, cost_quota_safety_stage, covered_stage_names,
    env_truthy, live_cerebras_api_key, live_event,
};
use super::{
    AuthLifecycleAuthPath, AuthLifecycleDriver, AuthLifecycleSpec,
    fetch_live_openai_compatible_models, run_live_openai_compatible_smoke,
    run_live_openai_compatible_stream_smoke,
};

#[tokio::test(flavor = "current_thread")]
async fn cerebras_live_opt_in_catalog_lifecycle_uses_isolated_sandbox() {
    if !env_truthy("KCODE_AUTH_LIFECYCLE_LIVE") {
        eprintln!(
            "skipping live Cerebras auth lifecycle test; set KCODE_AUTH_LIFECYCLE_LIVE=1 and KCODE_AUTH_LIFECYCLE_CEREBRAS_API_KEY"
        );
        return;
    }
    let api_key = live_cerebras_api_key()
        .expect("KCODE_AUTH_LIFECYCLE_LIVE=1 requires KCODE_AUTH_LIFECYCLE_CEREBRAS_API_KEY");

    let spend_smoke = env_truthy("KCODE_AUTH_LIFECYCLE_SMOKE");
    let stream_smoke = env_truthy("KCODE_AUTH_LIFECYCLE_STREAM_SMOKE");
    let mut stages = vec![
        LiveVerificationStage::passed(checkpoints::AUTH_CREDENTIAL_LOADED)
            .with_evidence("auth_source", serde_json::json!(api_key.auth.source.clone()))
            .with_evidence("env_key", serde_json::json!(api_key.auth.env_key.clone())),
        cost_quota_safety_stage(spend_smoke || stream_smoke),
    ];
    let models_result = fetch_live_openai_compatible_models(
        jcode_base::provider_catalog::CEREBRAS_PROFILE,
        &api_key.secret,
    )
    .await;
    let models = match models_result {
        Ok(models) => models,
        Err(error) => {
            stages.push(LiveVerificationStage::failed(
                checkpoints::MODEL_CATALOG_LIVE_ENDPOINT,
                error.to_string(),
            ));
            let capabilities = covered_stage_names(&stages);
            let event = live_event(
                "cerebras_live_opt_in_catalog_lifecycle_uses_isolated_sandbox",
                jcode_base::provider_catalog::CEREBRAS_PROFILE,
                api_key.auth.clone(),
                None,
                capabilities,
                jcode_base::live_tests::LiveVerificationResult::Failed,
            )
            .with_stages(stages);
            append_live_event(&event);
            panic!("live Cerebras model catalog: {error:?}");
        }
    };
    stages.push(
        LiveVerificationStage::passed(checkpoints::MODEL_CATALOG_LIVE_ENDPOINT)
            .with_evidence(
                "models",
                serde_json::json!(concise_model_sample(&models, 12)),
            ),
    );
    let default_model = jcode_base::provider_catalog::CEREBRAS_PROFILE.default_model;
    let selected = default_model
        .filter(|default| models.iter().any(|model| model == default))
        .map(ToString::to_string)
        .or_else(|| models.first().cloned())
        .expect("live catalog has model");

    let driver = AuthLifecycleDriver::new().expect("driver");
    let mut spec = AuthLifecycleSpec::cerebras_fixture(AuthLifecycleAuthPath::RemoteTuiPasteApiKey);
    spec.api_key = api_key.secret.clone();
    spec.catalog_models_after_auth = models;
    spec.selected_model_override = Some(selected.clone());

    let result = driver
        .run_openai_compatible_fixture(&spec)
        .expect("live lifecycle result");

    result.assert_success(&spec);
    assert!(
        result
            .catalog_routes
            .iter()
            .any(|route| route.model == selected && route.provider == "Cerebras"),
        "{}",
        result.failure_report(&spec)
    );
    push_common_stages(&mut stages, &api_key, &result, &selected);

    if spend_smoke {
        match run_live_openai_compatible_smoke(
            jcode_base::provider_catalog::CEREBRAS_PROFILE,
            &api_key.secret,
            &selected,
        )
        .await
        {
            Ok(stage) => stages.push(stage),
            Err(error) => {
                stages.push(LiveVerificationStage::failed(
                    checkpoints::NON_STREAMING_CHAT_COMPLETION,
                    error.to_string(),
                ));
                let capabilities = covered_stage_names(&stages);
                let event = live_event(
                    "cerebras_live_opt_in_catalog_lifecycle_uses_isolated_sandbox",
                    jcode_base::provider_catalog::CEREBRAS_PROFILE,
                    api_key.auth.clone(),
                    Some(&selected),
                    capabilities,
                    jcode_base::live_tests::LiveVerificationResult::Failed,
                )
                .with_stages(stages);
                append_live_event(&event);
                panic!("live Cerebras smoke completion: {error:?}");
            }
        }
    }

    if stream_smoke {
        match run_live_openai_compatible_stream_smoke(
            jcode_base::provider_catalog::CEREBRAS_PROFILE,
            &api_key.secret,
            &selected,
        )
        .await
        {
            Ok(stage) => stages.push(stage),
            Err(error) => {
                stages.push(LiveVerificationStage::failed(
                    checkpoints::STREAMING_CHAT_COMPLETION,
                    error.to_string(),
                ));
                let capabilities = covered_stage_names(&stages);
                let event = live_event(
                    "cerebras_live_opt_in_catalog_lifecycle_uses_isolated_sandbox",
                    jcode_base::provider_catalog::CEREBRAS_PROFILE,
                    api_key.auth.clone(),
                    Some(&selected),
                    capabilities,
                    jcode_base::live_tests::LiveVerificationResult::Failed,
                )
                .with_stages(stages);
                append_live_event(&event);
                panic!("live Cerebras stream smoke completion: {error:?}");
            }
        }
    }

    let capabilities = covered_stage_names(&stages);
    let event = live_event(
        "cerebras_live_opt_in_catalog_lifecycle_uses_isolated_sandbox",
        jcode_base::provider_catalog::CEREBRAS_PROFILE,
        api_key.auth.clone(),
        Some(&selected),
        capabilities,
        jcode_base::live_tests::LiveVerificationResult::Passed,
    )
    .with_stages(stages);
    append_live_event(&event);
}

fn push_common_stages(
    stages: &mut Vec<LiveVerificationStage>,
    api_key: &LiveTestApiKey,
    result: &crate::lifecycle_contracts::AuthLifecycleResult,
    selected: &str,
) {
    stages.push(
        LiveVerificationStage::passed(checkpoints::AUTH_UX_KEY_ENTRY)
            .with_evidence("auth_path", serde_json::json!("remote_tui_paste_api_key"))
            .with_evidence("simulated_in_sandbox", serde_json::json!(true))
            .with_evidence("transcript_order_verified", serde_json::json!(true)),
    );
    stages.push(
        LiveVerificationStage::passed(checkpoints::CREDENTIAL_PERSISTENCE)
            .with_evidence(
                "credential_location",
                serde_json::json!(result.credential_location.clone()),
            )
            .with_evidence("sandboxed", serde_json::json!(true)),
    );
    stages.push(
        LiveVerificationStage::passed(checkpoints::CATALOG_HOT_RELOAD_CURRENT_SESSION)
            .with_evidence("transcript_markers_verified", serde_json::json!(true))
            .with_evidence(
                "catalog_route_count",
                serde_json::json!(result.catalog_routes.len()),
            ),
    );
    stages.push(
        LiveVerificationStage::passed(checkpoints::PICKER_LIVE_MODELS)
            .with_evidence("selected_model", serde_json::json!(selected))
            .with_evidence(
                "picker_entries",
                serde_json::json!(result.picker.provider_entries.clone()),
            ),
    );
    stages.push(
        LiveVerificationStage::passed(checkpoints::PICKER_FALLBACK_LABELING)
            .with_evidence("fallback_routes_present", serde_json::json!(false))
            .with_evidence(
                "all_picker_entries_from_live_catalog",
                serde_json::json!(true),
            ),
    );
    stages.push(
        LiveVerificationStage::passed(checkpoints::MODEL_SWITCH_ROUTE)
            .with_evidence(
                "switch_request",
                serde_json::json!(result.picker.switch_request.clone()),
            )
            .with_evidence(
                "switch_route_api_method",
                serde_json::json!(result.picker.switch_route_api_method.clone()),
            ),
    );
    // touch unused-binding sink so the destructuring above is not flagged
    let _: &LiveTestApiKey = api_key;
}
