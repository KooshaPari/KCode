//! Tests for the provider identity derived for a session.
//!
//! Split out of `cases.rs` to keep that file within the file-size limit.

use super::*;

#[test]
fn derive_session_provider_key_live_profile_beats_stale_ambient_provider() {
    let _lock = lock_env();
    let _runtime = EnvVarGuard::set("KCODE_RUNTIME_PROVIDER", "openai");
    let _namespace = EnvVarGuard::remove("KCODE_OPENROUTER_CACHE_NAMESPACE");
    let _active = EnvVarGuard::remove("KCODE_ACTIVE_PROVIDER");

    // The live provider is an OpenAI-compatible profile serving a MiniMax model.
    // The ambient "openai" is stale state left by an earlier run and must not be
    // stamped onto the session: it would pair provider_key="openai" with
    // model="MiniMax-M3.1-Flash".
    assert_eq!(
        derive_session_provider_key("minimax").as_deref(),
        Some("minimax")
    );
}

#[test]
fn derive_session_provider_key_ignores_stale_concrete_active_provider() {
    let _lock = lock_env();
    let _runtime = EnvVarGuard::remove("KCODE_RUNTIME_PROVIDER");
    let _namespace = EnvVarGuard::remove("KCODE_OPENROUTER_CACHE_NAMESPACE");
    let _active = EnvVarGuard::set("KCODE_ACTIVE_PROVIDER", "cursor");

    // The generic transport is live; a concrete ambient provider names a
    // different provider and must not win.
    assert_eq!(
        derive_session_provider_key("openai-compatible").as_deref(),
        Some("openai-compatible")
    );
}

#[test]
fn derive_session_provider_key_live_concrete_provider_beats_ambient_profile() {
    let _lock = lock_env();
    let _runtime = EnvVarGuard::set("KCODE_RUNTIME_PROVIDER", "azure-openai");
    let _namespace = EnvVarGuard::remove("KCODE_OPENROUTER_CACHE_NAMESPACE");
    let _active = EnvVarGuard::remove("KCODE_ACTIVE_PROVIDER");

    // A concrete live provider is authoritative. Ambient profile refinement only
    // applies to the OpenAI-compatible transport family, never to Claude.
    assert_eq!(
        derive_session_provider_key("claude").as_deref(),
        Some("claude")
    );
}

#[test]
fn derive_session_provider_key_still_honors_explicit_jcode_identity() {
    let _lock = lock_env();
    let _runtime = EnvVarGuard::set("KCODE_RUNTIME_PROVIDER", "openrouter");
    let _namespace = EnvVarGuard::remove("KCODE_OPENROUTER_CACHE_NAMESPACE");
    let _active = EnvVarGuard::remove("KCODE_ACTIVE_PROVIDER");

    // The subscription provider is an explicit identity that outranks ambient.
    assert_eq!(
        derive_session_provider_key("jcode").as_deref(),
        Some("jcode")
    );
}
