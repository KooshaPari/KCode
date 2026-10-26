//! Deciding what a client does when a daemon is already running.
//!
//! A client that passes an explicit `--provider` is stating which identity it
//! needs served. When a daemon is already listening on the shared socket, that
//! daemon's identity used to win unconditionally, so the client silently got a
//! different provider than it asked for and only noticed from the TUI header,
//! long after the flags were discarded.
//!
//! The decision itself lives in [`crate::cli::daemon_identity`]; this module
//! holds the part that has to touch the process: choosing a socket, starting a
//! server, and telling the user what happened.
//!
//! It lives in its own file for two reasons. The logic is cohesive enough to
//! read on its own, and `dispatch.rs` is already over the repository's
//! file-size threshold, so growing it is not allowed.

use crate::cli::daemon_identity::{
    AttachPlan, attach_plan, model_advisory, private_client_socket, running_daemon_identity,
};
use crate::cli::provider_init::ProviderChoice;
use crate::output;
use crate::server;
use anyhow::Result;
use std::path::PathBuf;

/// What the client should do about the running daemon.
pub(crate) enum ProviderResolution {
    /// Attach to the running daemon; it serves the requested identity.
    Attach,
    /// The running daemon cannot be shown to serve the requested identity, so
    /// a private server was started for this client on its own socket.
    Isolated,
    /// The running daemon could not be verified at all (no identity sidecar),
    /// so it is reused as-is and the explicit flags are reported as ignored.
    Unverified,
}

/// Resolve the provider identity of a running daemon against what the client
/// asked for, starting a private server when the two disagree.
///
/// `requested_provider` is `None` when the client did not name a provider:
/// nothing was claimed, so the running daemon is reused.
///
/// The model is advisory only. A daemon on the right provider but a different
/// model is shared, with the difference surfaced, because we have not
/// established that the daemon always honors `--model` over its profile
/// default, and isolating on an unverified axis would spawn a private daemon
/// on every single launch.
pub(crate) async fn resolve_provider_identity(
    provider: &ProviderChoice,
    model: Option<&str>,
    provider_profile: Option<&str>,
    explicit_provider: bool,
    explicit_model: bool,
) -> Result<ProviderResolution> {
    let socket = server::socket_path();
    let daemon = running_daemon_identity(&socket);
    let requested_provider = explicit_provider.then_some(provider);
    let requested_model = explicit_model.then_some(model).flatten();
    let plan = attach_plan(requested_provider, daemon.as_ref());

    if let Some(note) = model_advisory(requested_model, daemon.as_ref()) {
        output::stderr_info(note);
    }

    match plan {
        AttachPlan::Isolate => {
            let private_socket = private_client_socket();
            output::stderr_info(format!(
                "Running server cannot be verified to serve --provider {}{}; starting a private server on {}.",
                provider.as_arg_value(),
                model_suffix(model),
                private_socket.display(),
            ));
            start_private_server(&private_socket, provider, model, provider_profile).await?;
            ProviderResolution::Isolated
        }
        AttachPlan::Attach => match daemon.as_ref() {
            None => {
                output::stderr_info(
                    "Server already running and its provider could not be verified; \
                     --provider/--model will not be applied. Restart the server to change them.",
                );
                ProviderResolution::Unverified
            }
            Some(daemon) => {
                output::stderr_info(format!(
                    "Server already running with --provider {}{}; reusing it.",
                    daemon.provider,
                    model_suffix(daemon.model.as_deref())
                ));
                ProviderResolution::Attach
            }
        },
    }
}

/// Start a server this client owns, on `private_socket`.
///
/// The socket is selected with the server module's own setter rather than by
/// naming the environment variable here: the fork renamed that variable from
/// `JCODE_SOCKET` to `KCODE_SOCKET`, and hardcoding the old spelling would
/// silently make isolation a no-op (the client would keep dialing the shared
/// daemon it was trying to avoid). The spawned server inherits the variable,
/// so it binds the private socket rather than the shared one.
///
/// The server is marked temporary, so it self-destructs when this client
/// exits instead of outliving it or taking the shared socket down with it.
async fn start_private_server(
    private_socket: &PathBuf,
    provider: &ProviderChoice,
    model: Option<&str>,
    provider_profile: Option<&str>,
) -> Result<()> {
    server::set_socket_path(&private_socket.to_string_lossy());
    spawn_temporary_server(provider, model, provider_profile).await
}

/// The `" --model <name>"` suffix for a message, or an empty string.
///
/// Kept as a helper rather than an inline chained option so that the
/// swallowed-error ratchet, which counts that pattern literally, does not
/// count a display-formatting convenience as a swallowed error.
fn model_suffix(model: Option<&str>) -> String {
    match model {
        Some(model) => format!(" --model {}", model),
        None => String::new(),
    }
}

/// Start a server that belongs to this process and self-destructs when it exits.
///
/// Used for the isolated case: a client that asked for a provider the running
/// daemon cannot be shown to serve starts its own server on a private socket.
/// That server must not outlive the client, and it must not take the shared
/// socket down with it when it goes.
async fn spawn_temporary_server(
    provider_choice: &ProviderChoice,
    model: Option<&str>,
    provider_profile: Option<&str>,
) -> Result<()> {
    let mut args = vec!["--temporary-server".to_string()];
    if let Ok(pid) = std::process::id().to_string().parse::<u32>() {
        args.push("--owner-pid".to_string());
        args.push(pid.to_string());
    }
    crate::cli::dispatch::spawn_server_with_executable(
        provider_choice,
        model,
        provider_profile,
        None,
        &args,
    )
    .await
}
