//! Starting the server a client dials.
//!
//! This holds the spawn path and the helpers it needs. It lives in its own
//! file because `dispatch.rs` is already over the repository's file-size
//! threshold, so growing it is not allowed.

use super::startup_profile;
use crate::build;
use crate::cli::provider_init::ProviderChoice;
use crate::output;
use crate::server;
use anyhow::Result;
use std::process::{Command as ProcessCommand, Stdio};

pub(crate) async fn spawn_server_with_executable(
    provider_choice: &ProviderChoice,
    model: Option<&str>,
    provider_profile: Option<&str>,
    executable: Option<std::path::PathBuf>,
    extra_args: &[String],
) -> Result<()> {
    let socket_path = server::socket_path();
    if server_is_running_at(&socket_path).await {
        startup_profile::mark("server_ready");
        return Ok(());
    }

    if wait_for_existing_reload_server("server spawn").await {
        startup_profile::mark("server_ready");
        return Ok(());
    }

    #[cfg(unix)]
    let _spawn_lock = acquire_spawn_lock_or_wait(&socket_path).await?;

    if server_is_running_at(&socket_path).await {
        startup_profile::mark("server_ready");
        return Ok(());
    }

    if wait_for_existing_reload_server("server spawn after lock").await {
        startup_profile::mark("server_ready");
        return Ok(());
    }

    startup_profile::mark("server_spawn_start");
    output::stderr_info("Starting server...");
    let client_requested_selfdev = selfdev::client_selfdev_requested();
    let exe = executable
        .or_else(|| {
            build::shared_server_update_candidate(client_requested_selfdev).map(|(path, _)| path)
        })
        .or_else(|| {
            // A failure to read the current executable path is not fatal on its
            // own: the shared-server candidate above may still supply one, and
            // the error below reports it if nothing does.
            std::env::current_exe().map_err(|error| {
                anyhow::anyhow!(
                    "Could not determine executable path for server spawn: {}",
                    error
                )
            })
        })
        .ok_or_else(|| anyhow::anyhow!("Could not determine executable path for server spawn"))?;
    let mut cmd = ProcessCommand::new(&exe);
    cmd.env_remove(selfdev::CLIENT_SELFDEV_ENV);
    // This client already resolved a provider for its own run. Those ambient
    // identity variables must not follow the daemon across the process
    // boundary: the daemon re-derives its own provider from config and
    // auto-detection, and inheriting ours lets a session get stamped with a
    // provider that is not the one actually serving it.
    server::scrub_inherited_provider_identity(&mut cmd);
    // Re-assert this launch's identity *after* the scrub. The scrub drops
    // whatever the client happened to have in its own environment; the values set
    // here are the ones the daemon is actually being asked to serve, and the
    // daemon publishes them in `~/.jcode/servers.json` so a later client can
    // detect that it is attached to a daemon serving someone else's provider.
    cmd.env("KCODE_RUNTIME_PROVIDER", provider_choice.as_arg_value());
    if let Some(model) = model {
        cmd.env("KCODE_RUNTIME_MODEL", model);
    }
    if client_requested_selfdev {
        cmd.env("KCODE_DEBUG_CONTROL", "1");
    }
    cmd.arg("--provider").arg(provider_choice.as_arg_value());
    // The interactive TUI owns first-run onboarding/login. Let the spawned
    // server boot with a deferred (credential-less) provider when nothing is
    // configured yet, instead of bailing; the TUI activates a provider via the
    // in-TUI `/login` flow. See init_provider_with_options.
    cmd.env("KCODE_DEFERRED_AUTH_BOOTSTRAP", "1");
    if let Some(provider_profile) = provider_profile {
        cmd.arg("--provider-profile").arg(provider_profile);
    }
    if let Some(model) = model {
        cmd.arg("--model").arg(model);
    }
    cmd.arg("serve")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for arg in extra_args {
        cmd.arg(arg);
    }

    #[cfg(unix)]
    {
        let _child = server::spawn_server_notify(&mut cmd).await?;
        startup_profile::mark("server_ready");
    }
    #[cfg(not(unix))]
    {
        use std::io::Read;

        let mut child = cmd.spawn()?;
        let start = std::time::Instant::now();
        // Windows server bootstrap can legitimately take tens of seconds on
        // slow hosts (auth preflights + provider init were observed at 15-60s
        // on a Windows Server VPS, issue #503). The child's liveness is
        // checked every poll, so a generous budget only delays the error for
        // a genuinely hung server, while a crashed server still fails fast
        // with its stderr.
        let timeout = std::time::Duration::from_secs(120);
        while start.elapsed() < timeout {
            if server::has_live_listener(&socket_path).await {
                startup_profile::mark("server_ready");
                return Ok(());
            }

            if let Some(status) = child.try_wait()? {
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stderr.take() {
                    // The stderr text is only used to enrich the error message,
                    // so a read failure must not mask the real problem (the
                    // child exited). Record it in the message instead of
                    // discarding it.
                    if let Err(error) = pipe.read_to_string(&mut stderr) {
                        stderr.push_str(&format!("(could not read server stderr: {})", error));
                    }
                }
                let detail = stderr.trim();
                if detail.is_empty() {
                    anyhow::bail!("Server exited before becoming ready (status: {})", status);
                }
                anyhow::bail!(
                    "Server exited before becoming ready (status: {}). {}",
                    status,
                    detail
                );
            }

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        anyhow::bail!(
            "Timed out waiting for server to become ready at {} after {}ms",
            server::socket_path().display(),
            timeout.as_millis()
        );
    }

    #[cfg(unix)]
    Ok(())
}
async fn acquire_spawn_lock_or_wait(
    socket_path: &std::path::Path,
) -> Result<Option<SpawnLockGuard>> {
    let lock_path = spawn_lock_path(socket_path);
    let wait_start = std::time::Instant::now();
    let wait_timeout = std::time::Duration::from_secs(10);
    let mut announced_wait = false;

    loop {
        if let Some(lock) = try_acquire_spawn_lock(&lock_path)? {
            return Ok(Some(lock));
        }

        if server_is_running_at(socket_path).await {
            return Ok(None);
        }

        if !announced_wait {
            output::stderr_info("Another client is starting the server, waiting...");
            announced_wait = true;
        }

        if wait_start.elapsed() >= wait_timeout {
            anyhow::bail!(
                "Timed out waiting for another client to start server at {}",
                socket_path.display()
            );
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
async fn server_is_running_at(path: &std::path::Path) -> bool {
    // Check liveness before performing a protocol handshake. On Windows the
    // named pipe may be busy while another client is connecting; that already
    // proves a daemon exists, while a handshake connect can otherwise wait in
    // the transport's ERROR_PIPE_BUSY retry loop and block server startup.
    server::has_live_listener(path).await || server::is_server_ready(path).await
}
async fn wait_for_existing_reload_server(context: &str) -> bool {
    if let Some(state) = server::recent_reload_state(std::time::Duration::from_secs(30)) {
        match state.phase {
            server::ReloadPhase::Starting => {
                crate::logging::info(&format!(
                    "Reload state=starting during {}; waiting for existing server to return",
                    context
                ));
                return wait_for_reloading_server().await;
            }
            server::ReloadPhase::Failed => {
                crate::logging::warn(&format!(
                    "Reload state=failed during {} on {}: {}; recent_state={}",
                    context,
                    server::socket_path().display(),
                    state
                        .detail
                        .unwrap_or_else(|| "unknown reload failure".to_string()),
                    server::reload_state_summary(std::time::Duration::from_secs(60))
                ));
            }
            server::ReloadPhase::SocketReady => {}
        }
    }

    false
}
