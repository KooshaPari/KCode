//! Spawn metadata and client terminal environment propagation.
//!
//! Assembles the `KCODE_SPAWN_*` env exported to spawn hooks and launched
//! terminals, and carries the requesting client's terminal-identity snapshot so
//! a server-owned spawn follows the terminal the user is attached to instead of
//! the long-lived server's stale startup env (#405, #424).

use crate::{TerminalCommand, command_parts, shell_command};
use std::path::Path;
use std::process::Command;

/// Terminal/window-manager environment variables that identify *which*
/// terminal, multiplexer, or display a client is attached to.
///
/// The jcode server process is long-lived and captures these at *its* startup,
/// so once a client connects from a different terminal/tmux/zellij session the
/// server's copies are stale. Spawn and focus hooks executed by the server then
/// target the wrong terminal (see issue #405). To fix this, clients snapshot
/// these vars from their own environment and send them to the server, which
/// re-exports them to spawn/focus hooks so the hook places windows in the
/// terminal the user is actually looking at.
///
/// This intentionally covers terminal multiplexers (tmux, screen, zellij),
/// terminal emulators (kitty, wezterm, ghostty, iTerm, ...), and the display
/// server (X11 `DISPLAY`, Wayland `WAYLAND_DISPLAY`) so window placement and
/// routing all follow the connecting client.
pub const CLIENT_TERMINAL_ENV_VARS: &[&str] = &[
    // Client-owned store root. A server-owned spawn must use the requesting
    // client's root rather than the daemon's inherited one (#424).
    "KCODE_HOME",
    // Terminal multiplexers
    "ZELLIJ",
    "ZELLIJ_SESSION_NAME",
    "ZELLIJ_PANE_ID",
    "TMUX",
    "TMUX_PANE",
    "STY",
    // herdr terminal multiplexer (https://herdr.dev), see issue #405
    "HERDR_ENV",
    "HERDR_SOCKET_PATH",
    "HERDR_PANE_ID",
    "HERDR_TAB_ID",
    "HERDR_WORKSPACE_ID",
    "HERDR_BIN_PATH",
    "HERDR_SESSION",
    "HERDR_AGENT",
    // Terminal emulators
    "TERM",
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "COLORTERM",
    "KITTY_PID",
    "KITTY_WINDOW_ID",
    "KITTY_LISTEN_ON",
    "WEZTERM_PANE",
    "WEZTERM_EXECUTABLE",
    "WEZTERM_UNIX_SOCKET",
    "ALACRITTY_WINDOW_ID",
    "ALACRITTY_SOCKET",
    "GHOSTTY_RESOURCES_DIR",
    "GHOSTTY_BIN_DIR",
    "ITERM_SESSION_ID",
    "WINDOWID",
    "HANDTERM_SESSION",
    "HANDTERM_PID",
    "WT_SESSION",
    "WT_PROFILE_ID",
    // Display / window manager
    "DISPLAY",
    "WAYLAND_DISPLAY",
];

/// Snapshot the current process's terminal-identifying env vars (see
/// [`CLIENT_TERMINAL_ENV_VARS`]). Only vars that are actually set are included,
/// so the map is empty when nothing identifies the terminal.
pub fn snapshot_client_terminal_env() -> Vec<(String, String)> {
    CLIENT_TERMINAL_ENV_VARS
        .iter()
        .filter_map(|&key| {
            std::env::var(key)
                .ok()
                .map(|value| (key.to_string(), value))
        })
        .collect()
}

/// Replace inherited terminal identity with an authoritative client snapshot.
///
/// Removing every known key first is important for a shared server: an empty
/// client snapshot must not leak the pane that happened to start the server.
/// Aliases let integrations explicitly distinguish client values from other
/// process environment while native names preserve existing hook behavior.
pub fn apply_client_terminal_env(cmd: &mut Command, env: &[(String, String)]) {
    for key in CLIENT_TERMINAL_ENV_VARS {
        cmd.env_remove(key);
        cmd.env_remove(format!("KCODE_CLIENT_{key}"));
    }
    for (key, value) in env {
        if CLIENT_TERMINAL_ENV_VARS.contains(&key.as_str()) {
            cmd.env(key, value);
            cmd.env(format!("KCODE_CLIENT_{key}"), value);
        }
    }
}

/// The `KCODE_SPAWN_*` metadata env exported to spawn hooks and to terminals
/// launched by the built-in fallback:
///
/// - `KCODE_SPAWN_KIND`: why this spawn happened ("resume", "selfdev",
///   "swarm-agent", ...), when known.
/// - `KCODE_SPAWN_SESSION_ID`: the jcode session the window will run.
/// - `KCODE_SPAWN_TITLE`: the suggested window/tab title.
/// - `KCODE_SPAWN_CWD`: the working directory for the session.
/// - `KCODE_SPAWN_PROGRAM`: path of the jcode binary to execute.
/// - `KCODE_SPAWN_COMMAND`: the full command line, shell-escaped, for hooks
///   (like tmux) that take a single shell-command string.
///
/// `TerminalCommand::extra_env` entries (e.g. `KCODE_SPAWN_SWARM_ID`,
/// `KCODE_SPAWN_COORDINATOR_SESSION_ID`) are appended last and win collisions.
pub(crate) fn spawn_metadata_env(command: &TerminalCommand, cwd: &Path) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = Vec::new();
    if let Some(kind) = &command.kind {
        env.push(("KCODE_SPAWN_KIND".to_string(), kind.clone()));
    }
    if let Some(session_id) = &command.session_id {
        env.push(("KCODE_SPAWN_SESSION_ID".to_string(), session_id.clone()));
    }
    if let Some(title) = &command.title {
        env.push(("KCODE_SPAWN_TITLE".to_string(), title.clone()));
    }
    env.push((
        "KCODE_SPAWN_CWD".to_string(),
        cwd.to_string_lossy().into_owned(),
    ));
    let metadata_program = command
        .metadata_program
        .as_deref()
        .unwrap_or(&command.program);
    env.push((
        "KCODE_SPAWN_PROGRAM".to_string(),
        metadata_program.to_string_lossy().into_owned(),
    ));
    env.push((
        "KCODE_SPAWN_COMMAND".to_string(),
        shell_command(&command_parts(command)),
    ));
    // Re-export the requesting client's terminal env so spawn/focus hooks use
    // the client's terminal, not the server's stale startup env (#405). Each
    // var is exported both under its native name (overriding the inherited
    // value the spawned process/hook would otherwise see) and under a
    // `KCODE_CLIENT_<NAME>` alias so hooks can explicitly distinguish the
    // client's terminal from the server's.
    for (key, value) in &command.client_terminal_env {
        env.push((key.clone(), value.clone()));
        env.push((format!("KCODE_CLIENT_{key}"), value.clone()));
    }
    env.extend(command.extra_env.iter().cloned());
    env
}
