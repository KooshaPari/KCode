//! Deriving the `provider_key` persisted on a session.
//!
//! Ambient `KCODE_RUNTIME_PROVIDER` / `KCODE_ACTIVE_PROVIDER` legitimately
//! refine a generic OpenAI-compatible transport with the profile actually in
//! use (`"azure-openai"`, `"zai"`, ...). But when the value names a
//! *concrete* provider, it describes a different provider than the one
//! serving, so it is stale ambient state and must not be stamped onto the
//! session.

/// Provider identities that name a *specific* provider rather than refining
/// the OpenAI-compatible transport family.
///
/// Ambient `KCODE_RUNTIME_PROVIDER` / `KCODE_ACTIVE_PROVIDER` legitimately
/// refine a generic OpenAI-compatible transport with the profile actually in
/// use (`"azure-openai"`, `"zai"`, ...). But when the value names one of these
/// concrete providers, it describes a *different* provider than the one
/// serving, so it is stale ambient state and must not be stamped onto the
/// session.
fn is_concrete_provider_identity(value: &str) -> bool {
    matches!(
        value,
        "claude"
            | "anthropic"
            | "claude cli"
            | "openai"
            | "openai-api"
            | "openai-oauth"
            | "claude-api"
            | "claude-oauth"
            | "copilot"
            | "github copilot"
            | "cursor"
            | "gemini"
            | "google"
            | "antigravity"
            | "bedrock"
            | "aws bedrock"
            | "grok-build"
    )
}

/// The OpenAI-compatible transport family: a provider that speaks the
/// OpenAI-compatible API and therefore cannot name the concrete endpoint/profile
/// serving it on its own.
///
/// Only for these may ambient env refine the identity — for any other live
/// provider the live identity is authoritative.
fn is_openai_compatible_transport(value: &str) -> bool {
    matches!(value, "openrouter" | "openai-compatible")
}

/// Whether an ambient identity value may refine the live `provider_name`.
fn ambient_identity_may_refine(live_provider_name: &str, ambient: &str) -> bool {
    is_openai_compatible_transport(live_provider_name) && !is_concrete_provider_identity(ambient)
}

/// Derive the `provider_key` persisted on a session.
///
/// `provider_name` is the *live* provider serving this session. Ambient env can
/// only refine a generic OpenAI-compatible transport with its profile identity;
/// it must never override the live provider, otherwise the session records a
/// provider that is not actually serving it while `provider_model()` reports the
/// live model (e.g. `provider_key = "openai"` paired with a MiniMax model).
pub fn derive_session_provider_key(provider_name: &str) -> Option<String> {
    let normalized_name = provider_name.trim().to_ascii_lowercase();
    if normalized_name == "jcode" {
        return Some("jcode".to_string());
    }

    if let Ok(runtime_provider) = std::env::var("KCODE_RUNTIME_PROVIDER") {
        let runtime_provider = runtime_provider.trim().to_ascii_lowercase();
        if !runtime_provider.is_empty()
            && runtime_provider != "openai-compatible"
            && ambient_identity_may_refine(&normalized_name, &runtime_provider)
        {
            return Some(runtime_provider);
        }
    }

    if let Ok(namespace) = std::env::var("KCODE_OPENROUTER_CACHE_NAMESPACE") {
        let namespace = namespace.trim().to_ascii_lowercase();
        if !namespace.is_empty() && ambient_identity_may_refine(&normalized_name, &namespace) {
            return Some(namespace);
        }
    }

    if let Ok(active) = std::env::var("KCODE_ACTIVE_PROVIDER") {
        let active = active.trim().to_ascii_lowercase();
        if !active.is_empty() && ambient_identity_may_refine(&normalized_name, &active) {
            return Some(active);
        }
    }

    let fallback = match normalized_name.as_str() {
        "anthropic" | "claude" | "claude cli" => "claude",
        "openai" => "openai",
        "github copilot" | "copilot" => "copilot",
        "openrouter" => "openrouter",
        "cursor" => "cursor",
        "gemini" => "gemini",
        "antigravity" => "antigravity",
        "" => return None,
        other => other,
    };

    Some(fallback.to_string())
}
