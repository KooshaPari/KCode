//! The runtime key a daemon publishes for a given provider choice.
//!
//! This is the *serve path* answer, not the `LoginProviderTarget` answer. The
//! two differ, and a client that compares against the wrong one isolates
//! forever:
//!
//! * `--provider anthropic-api` and `--provider claude` both publish `claude`
//!   (the serve path calls `select_initial_model_provider("claude")` for
//!   both), even though their login targets are `ClaudeApiKey` and `Claude`.
//! * `--provider openai-api` likewise publishes `openai`, not `openai-api`.
//! * `--provider azure` publishes `azure-openai`, because the serve path
//!   calls `apply_azure_openai_runtime()` -> `ProviderActivation::azure_openai`.
//! * `--provider openai-compatible` publishes `openai-compatible` via
//!   `apply_openai_compatible_runtime()`.
//!
//! The client side (`cli::daemon_identity::canonical_provider_key`) calls this
//! function so both sides agree by construction. Keep the two in lockstep:
//! any new provider branch in the serve path needs a matching arm here.

use crate::cli::provider_init::ProviderChoice;

/// The runtime key the daemon publishes for a given provider choice.
///
/// This is the *serve path* answer, not the `LoginProviderTarget` answer. The
/// two differ, and a client that compares against the wrong one isolates
/// forever:
///
/// * `--provider anthropic-api` and `--provider claude` both publish `claude`
///   (the serve path calls `select_initial_model_provider("claude")` for
///   both), even though their login targets are `ClaudeApiKey` and `Claude`.
/// * `--provider openai-api` likewise publishes `openai`, not `openai-api`.
/// * `--provider azure` publishes `azure-openai`, because the serve path
///   calls `apply_azure_openai_runtime()` -> `ProviderActivation::azure_openai`.
/// * `--provider openai-compatible` publishes `openai-compatible` via
///   `apply_openai_compatible_runtime()`.
///
/// The client side (`cli::daemon_identity::canonical_provider_key`) calls this
/// function so both sides agree by construction. Keep the two in lockstep:
/// any new provider branch in the serve path needs a matching arm here.
pub fn serve_published_provider_key(choice: &ProviderChoice) -> &'static str {
    match choice {
        ProviderChoice::Claude | ProviderChoice::AnthropicApi => "claude",
        ProviderChoice::Openai | ProviderChoice::OpenaiApi => "openai",
        ProviderChoice::Openrouter => "openrouter",
        ProviderChoice::Bedrock => "bedrock",
        ProviderChoice::Copilot => "copilot",
        ProviderChoice::Azure => "azure-openai",
        ProviderChoice::OpenaiCompatible => "openai-compatible",
        // These clear the initial provider and only set `KCODE_ACTIVE_PROVIDER`,
        // so the daemon falls back to `provider.display_name()`.
        ProviderChoice::Cursor => "cursor",
        ProviderChoice::Gemini => "gemini",
        ProviderChoice::GrokBuild => "grok-build",
        ProviderChoice::Antigravity => "antigravity",
        ProviderChoice::Jcode => "jcode",
        // Every remaining vendor spelling resolves to the single OpenRouter
        // runtime key at runtime-key granularity.
        _ => "openrouter",
    }
}
