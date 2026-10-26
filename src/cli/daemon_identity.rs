//! Deciding whether a client may attach to an already-running daemon.
//!
//! A client that passes an explicit `--provider` is stating which identity it
//! needs served. When a daemon is already listening on the shared socket, that
//! daemon's identity used to win unconditionally, so the client silently got a
//! different provider than it asked for and only noticed from the TUI header,
//! long after the flags were discarded. This module makes that decision
//! explicit, canonical, and testable.
//!
//! The daemon's identity is read from the sidecar file beside its socket
//! (`socket::read_socket_identity`), not from the server registry: the registry
//! is a shared JSON map that concurrent daemons overwrite, so the daemon a
//! client is about to attach to is frequently missing from it.
//!
//! # Vocabulary
//!
//! The daemon publishes the canonical runtime key that `provider::activation`
//! already writes to `JCODE_RUNTIME_PROVIDER` (`RuntimeProviderId::key()`).
//! The client must therefore *derive* that same key from its `--provider`
//! value rather than comparing the raw CLI spelling against it. The two
//! vocabularies genuinely diverge, and in two different ways:
//!
//! * **CLI spelling vs runtime key.** `ProviderChoice::Azure` spells `azure`
//!   while the runtime key is `azure-openai`.
//! * **Login target vs published key.** `ProviderChoice::AnthropicApi` maps
//!   to the `ClaudeApiKey` login target, but the daemon's serve path
//!   publishes `claude` for it (it calls
//!   `select_initial_model_provider("claude")`, the same call as
//!   `ProviderChoice::Claude`). Likewise `ProviderChoice::OpenaiApi` maps to
//!   `OpenAiApiKey` but the daemon publishes `openai`.
//!
//! [`canonical_provider_key`] therefore delegates to
//! `provider_init::serve_published_provider_key`, which lives next to the
//! daemon's serve path and reproduces exactly the strings that path writes.
//! Both sides then agree by construction.
//!
//! # Known limitation: vendor-level selection is unverifiable
//!
//! `ProviderChoice` carries roughly forty vendor spellings (`lmstudio`,
//! `chutes`, `cerebras`, `meta-muse`, ...) that all resolve to the single
//! runtime key `openrouter`. A client asking for one vendor and a daemon
//! serving another therefore compare equal. This is an accepted imprecision,
//! not an oversight: at runtime-key granularity the daemon cannot distinguish
//! them. Publishing the resolved vendor name is the follow-up that would close
//! it.

use crate::cli::provider_init::ProviderChoice;
use crate::server::{SocketIdentity, read_socket_identity};
use std::path::PathBuf;

/// What the client should do about an already-running daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttachPlan {
    /// Attach to the running daemon.
    Attach,
    /// The running daemon cannot be shown to serve the identity this client
    /// asked for. Start a private server for this client so the explicit flags
    /// are actually honored.
    Isolate,
}

/// The canonical runtime key the daemon will publish for `choice`.
///
/// This mirrors the daemon's **serve path** in
/// `provider_init::init_provider_with_options`, not the `LoginProviderTarget`
/// name. Those two vocabularies are *not* the same, and assuming they are
/// caused a permanent-isolation bug:
///
/// * `--provider anthropic-api` and `--provider claude` both take the
///   `ClaudeApiKey`/`Claude` login targets, but the daemon's serve path calls
///   `select_initial_model_provider("claude")` for *both*, so the daemon
///   publishes `claude` — not `claude-api`.
/// * `--provider openai-api` likewise publishes `openai`, not `openai-api`.
/// * `--provider azure` goes through `apply_azure_openai_runtime()`, which
///   calls `ProviderActivation::azure_openai(..).apply_env()` and publishes
///   `azure-openai`.
///
/// The daemon publishes `JCODE_RUNTIME_PROVIDER`, which is set from
/// `RuntimeProviderId::key()` by `ProviderActivation::apply_env()` and from
/// `select_initial_runtime_provider_key()` by the serve path. This function
/// reproduces exactly those values so both sides agree by construction.
///
/// Returns `None` when no exact key can be derived, which includes `auto` (no
/// specific identity was requested) and any target with no `RuntimeProviderId`
/// counterpart. Callers must treat `None` as "cannot verify" and isolate, never
/// as agreement.
pub(crate) fn canonical_provider_key(choice: &ProviderChoice) -> Option<&'static str> {
    if matches!(choice, ProviderChoice::Auto) {
        return None;
    }
    // The daemon's own serve path is the source of truth for what it publishes.
    // `provider_init::serve_published_provider_key` lives next to that path so
    // the two cannot drift.
    let key = crate::cli::provider_init::serve_published_provider_key(choice);
    if key.is_empty() {
        return None;
    }
    Some(key)
}

/// Whether the running daemon can serve the identity this client requested.
///
/// `daemon` is the identity the daemon **resolved to**, never the flag it was
/// launched with. That distinction is the whole point: the daemon that caused
/// this bug was launched as `--provider auto` and resolved to `cursor`/GPT-5, so
/// treating a reported `auto` as "agrees" would re-open the bug.
///
/// Every branch that cannot *prove* a match isolates:
/// * no provider was requested (only possibly a model): attach, nothing claimed;
/// * the request cannot be canonicalised: isolate;
/// * the daemon publishes nothing, or publishes `auto`: isolate.
///
/// The model is deliberately **advisory**. It is published so a mismatch is
/// visible (see [`model_advisory`]), but it never causes isolation: we have not
/// established that the daemon always honors `--model` over its profile
/// default, and isolating on an unverified axis risks spawning a private daemon
/// on every single launch, which is worse than the bug being fixed.
pub(crate) fn attach_plan(
    requested: Option<&ProviderChoice>,
    daemon: Option<&SocketIdentity>,
) -> AttachPlan {
    // An `auto` request claims no identity, so it must attach rather than
    // spawn. `Auto` is a real variant of `ProviderChoice`, not `None`, so it
    // has to be checked explicitly: routing it through
    // `canonical_provider_key` would return `None` ("cannot verify") and
    // isolate, which is the opposite of what an unclaimed request means.
    let Some(requested) = requested else {
        return AttachPlan::Attach;
    };
    if matches!(requested, ProviderChoice::Auto) {
        return AttachPlan::Attach;
    }

    let Some(want) = canonical_provider_key(requested) else {
        return AttachPlan::Isolate;
    };

    let Some(daemon) = daemon else {
        return AttachPlan::Isolate;
    };

    let have = daemon.provider.trim();
    if have.is_empty() || have.eq_ignore_ascii_case("auto") {
        return AttachPlan::Isolate;
    }

    if have.eq_ignore_ascii_case(want) {
        AttachPlan::Attach
    } else {
        AttachPlan::Isolate
    }
}

/// A human-readable note when the daemon's model differs from the one requested.
///
/// Advisory only: see [`attach_plan`]. Returns `None` when there is nothing to
/// report.
pub(crate) fn model_advisory(
    requested_model: Option<&str>,
    daemon: Option<&SocketIdentity>,
) -> Option<String> {
    let requested = requested_model?;
    let served = daemon?.model.as_deref()?;
    if requested == served {
        return None;
    }
    Some(format!(
        "requested model {requested:?} but the attached daemon serves {served:?}; \
         sharing it anyway (model isolation is not enabled)"
    ))
}

/// The provider/model the daemon listening on `socket` says it is serving.
pub(crate) fn running_daemon_identity(socket: &std::path::Path) -> Option<SocketIdentity> {
    read_socket_identity(socket)
}

/// A private socket path for a client that must not share the running daemon.
///
/// Scoped to this process so two simultaneously-isolated clients cannot collide.
pub(crate) fn private_client_socket() -> PathBuf {
    crate::server::default_socket_path()
        .with_file_name(format!("jcode-client-{}.sock", std::process::id()))
}

#[cfg(test)]
mod tests {
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
}
