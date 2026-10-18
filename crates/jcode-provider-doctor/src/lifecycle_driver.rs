//! Auth-lifecycle contract driver for `provider-doctor` and the
//! `AuthLifecycleResult` reusable contract that backs the
//! `assert_success` / `assert_rejected_success` assertions used by every
//! offline and live matrix in this crate.
//!
//! File layout (panic budget ≤ 500L per file, current size 340L):
//! - `lifecycle_driver.rs` (this file): imports, the
//!   `AuthLifecycleAuthPath` / `AuthLifecycleSpec` / `PickerSnapshot` /
//!   `AuthLifecycleDriver` production types, the `route_matches_spec`
//!   helper, the `live_provider_probes` re-export, and the
//!   `#[cfg(test)]` module declarations wiring the test files in
//!   `./lifecycle_driver/...`.
//! - `lifecycle_contracts.rs`: `AuthLifecycleResult` (the
//!   consumer-facing assertion struct, ~210L). Kept separate so the
//!   `assert_rejected_success` helper shared by the failure-contracts
//!   tests can live next to the assertion logic it exercises.
//! - `./lifecycle_driver/live_helpers.rs`: env-gated live-test
//!   helpers (`LiveTestApiKey`, `live_cerebras_api_key`,
//!   `live_event`, `cost_quota_safety_stage`, etc.), ~180L.
//! - `./lifecycle_driver/lifecycle_tests_provider_matrix.rs`: the
//!   large offline provider-matrix tests (~470L).
//! - `./lifecycle_driver/lifecycle_tests_failure_contracts.rs`:
//!   degraded-state rejection tests, ~140L.
//! - `./lifecycle_driver/lifecycle_tests_live_cerebras.rs`:
//!   env-gated live Cerebras lifecycle, ~220L.
//! - `./lifecycle_driver/lifecycle_tests_live_smoke.rs`: env-gated
//!   live OpenCode Zen tool-call smoke + issue-driven live target
//!   smoke, ~440L.

use anyhow::{Context, ensure};

use jcode_base::auth::lifecycle::{
    AuthActivationRequest, activate_auth_change, provider_model_to_select_after_auth,
    validate_catalog_invariants,
};
use jcode_base::auth::test_sandbox::AuthTestSandbox;
use jcode_base::protocol::{
    AuthChanged, AuthCredentialSource, AuthMethod, CatalogNamespace, RuntimeProviderKey,
};
use jcode_base::provider::ModelRoute;
use jcode_base::provider_catalog::OpenAiCompatibleProfile;

use crate::lifecycle_contracts::{AuthLifecycleResult, route_matches_spec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AuthLifecycleAuthPath {
    TuiPasteApiKey,
    RemoteTuiPasteApiKey,
    CliLogin,
    EnvFilePreseeded,
    ProcessEnvPreseeded,
}

impl AuthLifecycleAuthPath {
    fn auth_method(self) -> AuthMethod {
        match self {
            Self::TuiPasteApiKey => AuthMethod::TuiPasteApiKey,
            Self::RemoteTuiPasteApiKey => AuthMethod::RemoteTuiPasteApiKey,
            Self::CliLogin => AuthMethod::CliLogin,
            Self::EnvFilePreseeded => AuthMethod::EnvFilePreseeded,
            Self::ProcessEnvPreseeded => AuthMethod::ProcessEnvPreseeded,
        }
    }

    fn credential_source(self) -> AuthCredentialSource {
        match self {
            Self::TuiPasteApiKey
            | Self::RemoteTuiPasteApiKey
            | Self::CliLogin
            | Self::EnvFilePreseeded => AuthCredentialSource::ApiKeyFile,
            Self::ProcessEnvPreseeded => AuthCredentialSource::ProcessEnv,
        }
    }

    pub(crate) fn shows_paste_prompt(self) -> bool {
        matches!(self, Self::TuiPasteApiKey | Self::RemoteTuiPasteApiKey)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AuthLifecycleSpec {
    pub provider_id: &'static str,
    pub provider_label: &'static str,
    pub profile: OpenAiCompatibleProfile,
    pub auth_path: AuthLifecycleAuthPath,
    pub api_key: String,
    pub catalog_models_after_auth: Vec<String>,
    pub selected_model_override: Option<String>,
    pub current_runtime_provider_name: &'static str,
}

impl AuthLifecycleSpec {
    pub(crate) fn cerebras_fixture(auth_path: AuthLifecycleAuthPath) -> Self {
        let mut spec = Self::openai_compatible_fixture(
            jcode_base::provider_catalog::CEREBRAS_PROFILE,
            auth_path,
        );
        spec.catalog_models_after_auth = vec![
            "qwen-3-235b-a22b-instruct-2507".to_string(),
            "llama3.1-8b".to_string(),
        ];
        spec.selected_model_override = None;
        spec
    }

    pub(crate) fn openai_compatible_fixture(
        profile: OpenAiCompatibleProfile,
        auth_path: AuthLifecycleAuthPath,
    ) -> Self {
        let default_model = profile.default_model.unwrap_or("fixture-model");
        let mut catalog_models_after_auth = vec![default_model.to_string()];
        catalog_models_after_auth.push(format!("{}-alternate-fixture-model", profile.id));
        Self {
            provider_id: profile.id,
            provider_label: profile.display_name,
            profile,
            auth_path,
            api_key: format!("test-{}-key", profile.id),
            catalog_models_after_auth,
            selected_model_override: profile
                .default_model
                .is_none()
                .then(|| default_model.to_string()),
            current_runtime_provider_name: "mock-auth",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PickerSnapshot {
    pub selected_model: Option<String>,
    pub provider_entries: Vec<String>,
    pub switch_target: Option<String>,
    pub switch_request: Option<String>,
    pub switch_route_provider: Option<String>,
    pub switch_route_api_method: Option<String>,
}

impl PickerSnapshot {
    fn build(
        spec: &AuthLifecycleSpec,
        activation: &jcode_base::auth::lifecycle::AuthActivationResult,
        selected_model: Option<&str>,
        routes: &[ModelRoute],
    ) -> Self {
        let provider_routes = routes
            .iter()
            .filter(|route| route.available && route_matches_spec(route, spec))
            .collect::<Vec<_>>();
        let provider_entries = provider_routes
            .iter()
            .map(|route| route.model.clone())
            .collect::<Vec<_>>();
        let selected_model = selected_model
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(ToString::to_string);
        let switch_target = provider_entries
            .iter()
            .find(|model| Some(model.as_str()) != selected_model.as_deref())
            .or_else(|| provider_entries.first())
            .cloned();
        let switch_request = switch_target.as_deref().map(|model| {
            activation.model_switch_request(spec.current_runtime_provider_name, model)
        });
        let switch_route = switch_target.as_ref().and_then(|target| {
            provider_routes
                .iter()
                .find(|route| route.model == *target)
                .copied()
        });

        Self {
            selected_model,
            provider_entries,
            switch_target,
            switch_request,
            switch_route_provider: switch_route.map(|route| route.provider.clone()),
            switch_route_api_method: switch_route.map(|route| route.api_method.clone()),
        }
    }
}

pub(crate) struct AuthLifecycleDriver {
    sandbox: AuthTestSandbox,
}

impl AuthLifecycleDriver {
    pub(crate) fn new() -> anyhow::Result<Self> {
        Ok(Self {
            sandbox: AuthTestSandbox::new()?,
        })
    }

    pub(crate) fn run_openai_compatible_fixture(
        &self,
        spec: &AuthLifecycleSpec,
    ) -> anyhow::Result<AuthLifecycleResult> {
        let resolved =
            jcode_base::provider_catalog::resolve_openai_compatible_profile(spec.profile);
        ensure!(
            resolved.id == spec.provider_id,
            "spec provider id {} did not match profile {}",
            spec.provider_id,
            resolved.id
        );

        let credential_location = self.apply_credentials(spec, &resolved)?;
        let auth = AuthChanged {
            provider: jcode_base::protocol::AuthProviderId::new(spec.provider_id),
            credential_source: Some(spec.auth_path.credential_source()),
            auth_method: Some(spec.auth_path.auth_method()),
            expected_runtime: Some(RuntimeProviderKey::new("openai-compatible")),
            expected_catalog_namespace: Some(CatalogNamespace::new(spec.provider_id)),
        };
        let activation = activate_auth_change(&AuthActivationRequest::new(None, Some(auth)));
        let catalog_routes = self.catalog_routes_for_spec(spec);
        let session_model = spec
            .selected_model_override
            .clone()
            .or_else(|| activation.activated_model.clone());
        let selected_model = provider_model_to_select_after_auth(
            &activation,
            session_model.as_deref(),
            &catalog_routes,
        )
        .or(session_model);
        let catalog_report =
            validate_catalog_invariants(&activation, selected_model.as_deref(), &catalog_routes);
        let picker = PickerSnapshot::build(
            spec,
            &activation,
            selected_model.as_deref(),
            &catalog_routes,
        );
        let transcript = self.user_visible_transcript(
            spec,
            &resolved,
            selected_model.as_deref(),
            catalog_report.warning_message().as_deref(),
        );

        Ok(AuthLifecycleResult {
            activation,
            transcript,
            catalog_report,
            picker,
            catalog_routes,
            credential_location,
        })
    }

    fn apply_credentials(
        &self,
        spec: &AuthLifecycleSpec,
        resolved: &jcode_base::provider_catalog::ResolvedOpenAiCompatibleProfile,
    ) -> anyhow::Result<Option<String>> {
        match spec.auth_path {
            AuthLifecycleAuthPath::TuiPasteApiKey
            | AuthLifecycleAuthPath::RemoteTuiPasteApiKey
            | AuthLifecycleAuthPath::CliLogin
            | AuthLifecycleAuthPath::EnvFilePreseeded => {
                let path = self
                    .sandbox
                    .write_openai_compatible_api_key(spec.profile, &spec.api_key)
                    .with_context(|| format!("write {} API key file", spec.provider_label))?;
                Ok(Some(path.display().to_string()))
            }
            AuthLifecycleAuthPath::ProcessEnvPreseeded => {
                jcode_base::env::set_var(&resolved.api_key_env, &spec.api_key);
                jcode_base::auth::AuthStatus::invalidate_cache();
                Ok(Some(format!("process env {}", resolved.api_key_env)))
            }
        }
    }

    fn catalog_routes_for_spec(&self, spec: &AuthLifecycleSpec) -> Vec<ModelRoute> {
        spec.catalog_models_after_auth
            .iter()
            .map(|model| ModelRoute {
                model: model.clone(),
                provider: spec.provider_label.to_string(),
                api_method: format!("openai-compatible:{}", spec.provider_id),
                available: true,
                detail: "fixture live-catalog route".to_string(),
                usage: None,
                cheapness: None,
            })
            .collect()
    }

    fn user_visible_transcript(
        &self,
        spec: &AuthLifecycleSpec,
        resolved: &jcode_base::provider_catalog::ResolvedOpenAiCompatibleProfile,
        selected_model: Option<&str>,
        warning: Option<&str>,
    ) -> Vec<String> {
        let mut transcript = Vec::new();
        if spec.auth_path.shows_paste_prompt() {
            transcript.push(format!(
                "**{} API Key**\n\nSetup docs: {}\nStored variable: `{}`\nEndpoint: `{}`\nSuggested default model: `{}`\n\n**Paste your API key below** (it will be saved securely), or type `/cancel` to abort.",
                spec.provider_label,
                resolved.setup_url,
                resolved.api_key_env,
                resolved.api_base,
                resolved.default_model.as_deref().unwrap_or("none")
            ));
            transcript.push(format!(
                "**{} API key saved.**\n\nStored at `{}`.\nFetching models now. Jcode will switch to an accessible model returned by the live catalog and show the catalog diff when discovery finishes.",
                spec.provider_label,
                self.sandbox.env_file_path(&resolved.env_file).display()
            ));
        } else {
            transcript.push(format!(
                "**{} credentials detected.**\n\nCredential source: {:?}. Fetching models now.",
                spec.provider_label,
                spec.auth_path.credential_source()
            ));
        }
        let catalog_status = if warning.is_some() {
            format!(
                "{} catalog changed; some routes missing. Use `/model`.",
                spec.provider_label
            )
        } else {
            format!("{} catalog changed. Use `/model`.", spec.provider_label)
        };
        transcript.push(format!(
            "**Model ready:** `{}`\n{catalog_status}",
            selected_model.unwrap_or("none"),
        ));
        transcript
    }
}

#[cfg(test)]
pub(crate) use crate::live_provider_probes::{
    fetch_live_openai_compatible_models, run_live_openai_compatible_smoke,
    run_live_openai_compatible_stream_smoke, run_live_openai_compatible_tool_smoke,
};

#[cfg(test)]
mod lifecycle_tests_provider_matrix;

#[cfg(test)]
mod lifecycle_tests_failure_contracts;

#[cfg(test)]
mod lifecycle_tests_live_cerebras;

#[cfg(test)]
mod lifecycle_tests_live_smoke;

#[cfg(test)]
pub(crate) mod live_helpers;
