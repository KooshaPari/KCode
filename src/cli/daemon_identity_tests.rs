//! Unit tests for the daemon identity decision.
//!
//! These were inline in `daemon_identity.rs`; they live in a sibling file so
//! the module itself stays inside the file-size budget, mirroring the layout
//! already used for the socket-level coverage in `daemon_identity_socket_tests.rs`.

use super::*;

fn daemon(provider: &str, model: Option<&str>) -> SocketIdentity {
    SocketIdentity {
        provider: provider.to_string(),
        model: model.map(str::to_string),
    }
}

/// The bug this whole module exists for: a daemon launched as `--provider
/// auto` that resolved to `cursor`/GPT-5 must not capture a client that
/// explicitly asked for `openai-compatible`/`MiniMax-M3.1-Flash`.
#[test]
fn isolates_when_auto_resolved_to_another_provider() {
    let daemon = daemon("cursor", Some("GPT-5"));
    assert_eq!(
        attach_plan(Some(&ProviderChoice::OpenaiCompatible), Some(&daemon)),
        AttachPlan::Isolate,
        "an auto-launched daemon serving cursor must not capture a client \
         that explicitly asked for openai-compatible"
    );
}

#[test]
fn attaches_when_daemon_serves_the_requested_provider() {
    let daemon = daemon("openai-compatible", Some("MiniMax-M3.1-Flash"));
    assert_eq!(
        attach_plan(Some(&ProviderChoice::OpenaiCompatible), Some(&daemon)),
        AttachPlan::Attach
    );
}

/// `auto` must never be read as proof of a match on the daemon side.
#[test]
fn isolates_when_daemon_reports_auto() {
    let daemon = daemon("auto", Some("GPT-5"));
    assert_eq!(
        attach_plan(Some(&ProviderChoice::OpenaiCompatible), Some(&daemon)),
        AttachPlan::Isolate
    );
}

/// No sidecar (older daemon) cannot prove agreement either.
#[test]
fn isolates_when_daemon_identity_is_unknown() {
    assert_eq!(
        attach_plan(Some(&ProviderChoice::OpenaiCompatible), None),
        AttachPlan::Isolate
    );
}

/// An `auto` *request* claims nothing, so it must attach rather than spawn.
#[test]
fn auto_request_attaches_and_never_isolates() {
    let daemon = daemon("cursor", Some("GPT-5"));
    assert_eq!(
        attach_plan(Some(&ProviderChoice::Auto), Some(&daemon)),
        AttachPlan::Attach
    );
    assert_eq!(attach_plan(None, Some(&daemon)), AttachPlan::Attach);
}

/// The model is advisory: a mismatch must not isolate.
#[test]
fn model_mismatch_does_not_isolate() {
    let daemon = daemon("openai-compatible", Some("GPT-5"));
    assert_eq!(
        attach_plan(Some(&ProviderChoice::OpenaiCompatible), Some(&daemon)),
        AttachPlan::Attach
    );
    let note = model_advisory(Some("MiniMax-M3.1-Flash"), Some(&daemon))
        .expect("a model difference must be surfaced");
    assert!(note.contains("MiniMax-M3.1-Flash"), "note was {note:?}");
    assert!(note.contains("GPT-5"), "note was {note:?}");
}

#[test]
fn no_model_advisory_when_models_agree_or_are_absent() {
    let daemon = daemon("openai-compatible", Some("MiniMax-M3.1-Flash"));
    assert_eq!(
        model_advisory(Some("MiniMax-M3.1-Flash"), Some(&daemon)),
        None
    );
    assert_eq!(model_advisory(None, Some(&daemon)), None);
}

/// Pins the two mappings this bug turned on.
#[test]
fn pinned_canonical_keys() {
    assert_eq!(
        canonical_provider_key(&ProviderChoice::OpenaiCompatible),
        Some("openai-compatible")
    );
    assert_eq!(
        canonical_provider_key(&ProviderChoice::Cursor),
        Some("cursor")
    );
    assert_eq!(
        canonical_provider_key(&ProviderChoice::Openrouter),
        Some("openrouter")
    );
}

/// Regression: the API-key provider spellings must resolve to the key the
/// daemon *actually publishes*, not to their `LoginProviderTarget` name.
///
/// The daemon's serve path calls `select_initial_model_provider("claude")`
/// for BOTH `--provider claude` and `--provider anthropic-api`, and
/// `select_initial_model_provider("openai")` for BOTH `--provider openai`
/// and `--provider openai-api`. A client that expected `claude-api` /
/// `openai-api` would isolate onto a private daemon on *every* launch,
/// which is worse than the bug being fixed.
#[test]
fn api_key_spellings_match_the_published_runtime_key() {
    assert_eq!(
        canonical_provider_key(&ProviderChoice::AnthropicApi),
        Some("claude"),
        "the daemon publishes `claude` for --provider anthropic-api"
    );
    assert_eq!(
        canonical_provider_key(&ProviderChoice::Claude),
        Some("claude"),
        "--provider claude and --provider anthropic-api share a runtime"
    );
    assert_eq!(
        canonical_provider_key(&ProviderChoice::OpenaiApi),
        Some("openai"),
        "the daemon publishes `openai` for --provider openai-api"
    );
    assert_eq!(
        canonical_provider_key(&ProviderChoice::Openai),
        Some("openai")
    );
}

/// The API-key spellings must therefore *attach* to a daemon that serves
/// the shared runtime, not isolate forever.
#[test]
fn api_key_spellings_attach_to_a_matching_daemon() {
    let claude = daemon("claude", Some("claude-sonnet-4"));
    assert_eq!(
        attach_plan(Some(&ProviderChoice::AnthropicApi), Some(&claude)),
        AttachPlan::Attach
    );
    let openai = daemon("openai", Some("gpt-5"));
    assert_eq!(
        attach_plan(Some(&ProviderChoice::OpenaiApi), Some(&openai)),
        AttachPlan::Attach
    );
}

/// Azure goes through `apply_azure_openai_runtime()`, which publishes the
/// `azure-openai` runtime key, not the CLI spelling `azure`.
#[test]
fn azure_matches_the_azure_openai_runtime_key() {
    assert_eq!(
        canonical_provider_key(&ProviderChoice::Azure),
        Some("azure-openai")
    );
    let azure = daemon("azure-openai", Some("gpt-4o"));
    assert_eq!(
        attach_plan(Some(&ProviderChoice::Azure), Some(&azure)),
        AttachPlan::Attach
    );
}

/// The exact failure mode of the original bug: an `openai-compatible`
/// request must never canonicalise to a neighbouring runtime.
#[test]
fn openai_compatible_never_canonicalises_to_a_neighbour() {
    let key = canonical_provider_key(&ProviderChoice::OpenaiCompatible);
    assert_ne!(key, Some("openrouter"));
    assert_ne!(key, Some("openai"));
    assert_ne!(key, Some("openai-api"));
}

/// `auto` has no canonical identity, so it must never produce a key.
#[test]
fn auto_has_no_canonical_key() {
    assert_eq!(canonical_provider_key(&ProviderChoice::Auto), None);
}

/// Every provider choice must produce *some* key. A `None` means "cannot
/// verify", which forces isolation, so a missing arm would silently make
/// that provider isolate on every launch.
#[test]
fn every_non_auto_provider_choice_yields_a_key() {
    let choices = [
        ProviderChoice::Jcode,
        ProviderChoice::Claude,
        ProviderChoice::AnthropicApi,
        ProviderChoice::Openai,
        ProviderChoice::OpenaiApi,
        ProviderChoice::Openrouter,
        ProviderChoice::Bedrock,
        ProviderChoice::Azure,
        ProviderChoice::OpenaiCompatible,
        ProviderChoice::Cursor,
        ProviderChoice::Copilot,
        ProviderChoice::Gemini,
        ProviderChoice::GrokBuild,
        ProviderChoice::Antigravity,
    ];
    for choice in choices {
        let key = canonical_provider_key(&choice);
        assert!(
            key.is_some_and(|k| !k.is_empty()),
            "{choice:?} produced no publishable runtime key"
        );
    }
}

#[test]
fn private_client_socket_is_process_scoped() {
    let socket = private_client_socket();
    assert!(
        socket
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("jcode-client-"),
        "isolated clients get a private socket, got {}",
        socket.display()
    );
    assert_ne!(socket, crate::server::default_socket_path());
}

/// The notice must name the provider the daemon actually serves, and the
/// model only when it knows one.
#[test]
fn attach_notice_names_the_served_identity() {
    let notice = attach_notice(&daemon("openai-compatible", Some("MiniMax-M3.1-Flash")));
    assert!(
        notice.contains("openai-compatible"),
        "notice does not name the served provider: {notice}"
    );
    assert!(
        notice.contains("MiniMax-M3.1-Flash"),
        "notice does not name the served model: {notice}"
    );
}

#[test]
fn attach_notice_omits_an_unknown_model() {
    let notice = attach_notice(&daemon("cursor", None));
    assert!(
        notice.contains("cursor"),
        "notice does not name the served provider: {notice}"
    );
    assert!(
        !notice.contains("--model"),
        "notice invents a model it does not know: {notice}"
    );
}
