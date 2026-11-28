//! Launching jcode sessions in new terminal windows.
//!
//! These helpers spawn a fresh `jcode` process (resume or self-dev) inside a
//! new terminal window. They are pure process/terminal orchestration built on
//! the low-level `terminal_launch` facade and depend only on core modules
//! (`id`, `process_title`, `platform`), so
//! they live in the core layer rather than the CLI command layer. This lets
//! lower layers like `server`, `restart_snapshot`, and `tool` relaunch
//! sessions without depending on `cli`.

use anyhow::Result;

use crate::{id, server};

/// Map a persisted session/runtime provider key (e.g. `anthropic-api-key`,
/// `claude-oauth`) to the value the resumed process accepts for `--provider`
/// (the CLI `ProviderChoice` vocabulary, e.g. `anthropic-api`, `claude`).
///
/// The two vocabularies are not identical, so passing the raw runtime key
/// straight through makes clap reject it (`invalid value 'anthropic-api-key'`)
/// and the freshly spawned window exits immediately before the TUI starts.
/// Returns `None` when the key has no clean standalone CLI provider value; the
/// flag is then omitted and the persisted session reconstructs the route on
/// resume.
fn resume_provider_arg(provider_key: Option<&str>) -> Option<&'static str> {
    provider_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(crate::provider::cli_provider_arg_for_session_key)
}

/// Metadata describing why a session window is being spawned, exported to
/// spawn hooks and spawned terminals as `KCODE_SPAWN_*` env vars so external
/// programs (tmux, kitty remote, herd, window managers) can reroute or place
/// the window. See `[terminal] spawn_hook` in config.
#[derive(Debug, Clone, Default)]
pub struct SessionSpawnContext {
    /// Spawn kind override (e.g. "swarm-agent", "restart"). Defaults to
    /// "resume" or "selfdev" based on the launch helper used.
    pub kind: Option<String>,
    /// Extra `KCODE_SPAWN_*` env entries (e.g. swarm/coordinator ids).
    pub extra_env: Vec<(String, String)>,
    /// Terminal-identifying env vars captured from the client that requested
    /// the spawn (tmux/zellij/kitty/DISPLAY/...). Re-exported to spawn/focus
    /// hooks so the new window lands in the client's terminal instead of the
    /// server's stale startup env (#405).
    pub client_terminal_env: Vec<(String, String)>,
}

impl SessionSpawnContext {
    pub fn kind(kind: impl Into<String>) -> Self {
        Self {
            kind: Some(kind.into()),
            extra_env: Vec::new(),
            client_terminal_env: Vec::new(),
        }
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_env.push((key.into(), value.into()));
        self
    }

    /// Attach the requesting client's terminal env snapshot (#405).
    pub fn with_client_terminal_env(mut self, env: Vec<(String, String)>) -> Self {
        self.client_terminal_env = env;
        self
    }

    fn apply(
        &self,
        mut command: crate::terminal_launch::TerminalCommand,
        default_kind: &str,
        session_id: &str,
    ) -> crate::terminal_launch::TerminalCommand {
        command = command
            .kind(self.kind.as_deref().unwrap_or(default_kind))
            .session_id(session_id);
        for (key, value) in &self.extra_env {
            command = command.spawn_env(key.clone(), value.clone());
        }
        if !self.client_terminal_env.is_empty() {
            command = command.client_terminal_env(self.client_terminal_env.clone());
        }
        command
    }
}

fn kcode_home_env_value() -> Option<String> {
    match std::env::var("KCODE_HOME") {
        Ok(value) if !value.is_empty() => Some(value),
        _ => None,
    }
}

/// Store root explicitly supplied by the requesting client (#424).
///
/// A shared server owns its own `KCODE_HOME`, but a visible spawn belongs to
/// the client that asked for it. `comm_session` forwards the client snapshot
/// through `SessionSpawnContext`; CLI-driven and device-driven spawns have no
/// client snapshot and legitimately use the process value instead.
fn client_kcode_home_for_spawn(context: &SessionSpawnContext) -> Option<String> {
    context
        .client_terminal_env
        .iter()
        .find(|(key, _)| key == "KCODE_HOME")
        .map(|(_, value)| value.clone())
        .filter(|value| !value.is_empty())
}

/// Build a terminal command with the effective store root forwarded at argv
/// level, preserving the original executable for spawn metadata (#424).
fn resume_command_with_context(
    exe: &std::path::Path,
    args: Vec<String>,
    title: String,
    context: &SessionSpawnContext,
    default_kind: &str,
    session_id: &str,
) -> crate::terminal_launch::TerminalCommand {
    let (program, args) = match client_kcode_home_for_spawn(context) {
        Some(home) => forward_kcode_home_for_spawn_with(exe.to_path_buf(), args, Some(&home)),
        None => forward_kcode_home_for_spawn(exe.to_path_buf(), args),
    };
    let wrapped = program.as_path() != exe;
    let command = crate::terminal_launch::TerminalCommand::new(program, args)
        .title(title)
        .fresh_spawn();
    let command = if wrapped {
        command.metadata_program(exe.to_path_buf())
    } else {
        command
    };
    context.apply(command, default_kind, session_id)
}

/// Prefix a spawned child command with `env KCODE_HOME=<value>` when the
/// current process has `KCODE_HOME` set.
///
/// Terminal-spawn hooks execute children with the hook runner's environment,
/// so a restored child would otherwise resolve the default store root instead
/// of the client's configured store root (#424). Forwarding at argv level
/// makes the child see the same store root regardless of hook environment.
///
/// Unix only. Windows has no `env` binary, so argv cannot be prefixed there;
/// a Windows child must inherit the variable or receive it from its hook, which
/// is still open on #424.
pub(crate) fn forward_kcode_home_for_spawn(
    program: std::path::PathBuf,
    args: Vec<String>,
) -> (std::path::PathBuf, Vec<String>) {
    forward_kcode_home_for_spawn_with(program, args, kcode_home_env_value().as_deref())
}

pub(crate) fn forward_kcode_home_for_spawn_with(
    program: std::path::PathBuf,
    args: Vec<String>,
    kcode_home: Option<&str>,
) -> (std::path::PathBuf, Vec<String>) {
    let Some(kcode_home) = kcode_home.filter(|value| !value.is_empty()) else {
        return (program, args);
    };

    if !cfg!(unix) {
        return (program, args);
    }

    let mut forwarded_args = Vec::with_capacity(args.len() + 2);
    forwarded_args.push(format!("KCODE_HOME={kcode_home}"));
    forwarded_args.push(program.to_string_lossy().into_owned());
    forwarded_args.extend(args);
    (std::path::PathBuf::from("env"), forwarded_args)
}

/// Compute the window/terminal title used when (re)launching a session.
pub fn resumed_window_title(session_id: &str) -> String {
    let session_name = crate::process_title::session_name(session_id);
    let icon = id::session_icon(&session_name);
    let display_title = crate::process_title::terminal_display_title_for_id(session_id);
    let session_label = crate::process_title::terminal_session_label(&session_name, None);
    let fallback_label = if let Some(server_info) =
        crate::registry::find_server_by_socket_sync(&server::socket_path())
    {
        crate::process_title::remote_window_label(&server_info.name, &session_label)
    } else {
        crate::process_title::window_label(&session_label)
    };
    crate::process_title::terminal_window_title(
        icon,
        display_title.as_deref(),
        Some(&fallback_label),
        false,
    )
}

/// Focus/raise the window for `session_id` via the configured focus hook.
///
/// Returns `true` when a hook was configured and its process started (the
/// built-in wmctrl/xdotool fallback should then be skipped). The hook receives
/// `KCODE_FOCUS_SESSION_ID` and `KCODE_FOCUS_TITLE` env vars.
pub fn focus_session_via_hook(session_id: &str, title: &str) -> bool {
    focus_session_via_hook_with_env(session_id, title, &[])
}

/// Like [`focus_session_via_hook`] but also re-exports the requesting client's
/// terminal env (#405) so focus hooks (e.g. `zellij action go-to-tab-name`)
/// target the client's terminal session instead of the server's stale env. Each
/// var is exported natively and under a `KCODE_CLIENT_<NAME>` alias.
pub fn focus_session_via_hook_with_env(
    session_id: &str,
    title: &str,
    client_terminal_env: &[(String, String)],
) -> bool {
    let hook = {
        let config = &crate::config::config().terminal;
        config
            .focus_hook
            .as_deref()
            .map(str::trim)
            .filter(|hook| !hook.is_empty())
            .map(str::to_string)
    };
    let Some(hook) = hook else {
        return false;
    };

    let parts = match crate::terminal_launch::parse_hook_command(&hook) {
        Ok(parts) => parts,
        Err(error) => {
            crate::logging::warn(&format!("Focus hook '{hook}' failed to parse: {error}"));
            return false;
        }
    };
    let (program, args) = parts
        .split_first()
        .expect("parse_hook_command guarantees at least one part");

    let mut cmd = std::process::Command::new(crate::terminal_launch::expand_home(program));
    cmd.args(args)
        .env("KCODE_FOCUS_SESSION_ID", session_id)
        .env("KCODE_FOCUS_TITLE", title)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    for (key, value) in client_terminal_env {
        cmd.env(key, value);
        cmd.env(format!("KCODE_CLIENT_{key}"), value);
    }
    match crate::platform::spawn_detached(&mut cmd) {
        Ok(child) => {
            // The hook outlives this call; reap it so its exit status is
            // collected instead of leaving a zombie under the server.
            crate::platform::reap_detached(child);
            true
        }
        Err(error) => {
            crate::logging::warn(&format!(
                "Focus hook '{hook}' failed to start ({error}); falling back to built-in focus"
            ));
            false
        }
    }
}

/// Focus a session window: configured focus hook first, then the built-in
/// wmctrl/xdotool title search (Linux only) as a best-effort fallback.
pub fn focus_session_window_best_effort(session_id: &str, title: &str) {
    focus_session_window_best_effort_with_env(session_id, title, &[]);
}

/// Like [`focus_session_window_best_effort`] but forwards the requesting
/// client's terminal env to the focus hook (#405).
pub fn focus_session_window_best_effort_with_env(
    session_id: &str,
    title: &str,
    client_terminal_env: &[(String, String)],
) {
    if focus_session_via_hook_with_env(session_id, title, client_terminal_env) {
        return;
    }
    focus_title_best_effort(title);
}

#[cfg(all(unix, not(target_os = "macos")))]
fn focus_title_best_effort(title: &str) {
    use std::process::{Command, Stdio};

    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(
            "sleep 0.4; \
             if command -v wmctrl >/dev/null 2>&1; then wmctrl -a \"$KCODE_WINDOW_TITLE\" >/dev/null 2>&1 && exit 0; fi; \
             if command -v xdotool >/dev/null 2>&1; then xdotool search --name \"$KCODE_WINDOW_TITLE\" windowactivate >/dev/null 2>&1 && exit 0; fi; \
             exit 0",
        )
        .env("KCODE_WINDOW_TITLE", title)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let _ = crate::platform::spawn_detached(&mut cmd);
}

#[cfg(any(not(unix), target_os = "macos"))]
fn focus_title_best_effort(_title: &str) {}

#[cfg(unix)]
pub fn spawn_resume_in_new_terminal(
    exe: &std::path::Path,
    session_id: &str,
    cwd: &std::path::Path,
) -> Result<bool> {
    spawn_resume_in_new_terminal_with_provider(exe, session_id, cwd, None)
}

#[cfg(unix)]
pub fn spawn_resume_in_new_terminal_with_provider(
    exe: &std::path::Path,
    session_id: &str,
    cwd: &std::path::Path,
    provider_key: Option<&str>,
) -> Result<bool> {
    spawn_resume_in_new_terminal_with_context(
        exe,
        session_id,
        cwd,
        provider_key,
        &SessionSpawnContext::default(),
    )
}

#[cfg(unix)]
pub fn spawn_resume_in_new_terminal_with_context(
    exe: &std::path::Path,
    session_id: &str,
    cwd: &std::path::Path,
    provider_key: Option<&str>,
    context: &SessionSpawnContext,
) -> Result<bool> {
    let title = resumed_window_title(session_id);
    let mut args = vec!["--fresh-spawn".to_string()];
    if let Some(provider_arg) = resume_provider_arg(provider_key) {
        args.push("--provider".to_string());
        args.push(provider_arg.to_string());
    }
    args.extend(["--resume".to_string(), session_id.to_string()]);
    let command = resume_command_with_context(exe, args, title, context, "resume", session_id);
    crate::terminal_launch::spawn_command_in_new_terminal(&command, cwd)
}

#[cfg(unix)]
pub fn spawn_selfdev_in_new_terminal(
    exe: &std::path::Path,
    session_id: &str,
    cwd: &std::path::Path,
) -> Result<bool> {
    spawn_selfdev_in_new_terminal_with_provider(exe, session_id, cwd, None)
}

#[cfg(unix)]
pub fn spawn_selfdev_in_new_terminal_with_provider(
    exe: &std::path::Path,
    session_id: &str,
    cwd: &std::path::Path,
    provider_key: Option<&str>,
) -> Result<bool> {
    spawn_selfdev_in_new_terminal_with_context(
        exe,
        session_id,
        cwd,
        provider_key,
        &SessionSpawnContext::default(),
    )
}

#[cfg(unix)]
pub fn spawn_selfdev_in_new_terminal_with_context(
    exe: &std::path::Path,
    session_id: &str,
    cwd: &std::path::Path,
    provider_key: Option<&str>,
    context: &SessionSpawnContext,
) -> Result<bool> {
    let selfdev_title = format!("{} [self-dev]", resumed_window_title(session_id));
    let mut args = vec!["--fresh-spawn".to_string()];
    if let Some(provider_arg) = resume_provider_arg(provider_key) {
        args.push("--provider".to_string());
        args.push(provider_arg.to_string());
    }
    args.extend([
        "--resume".to_string(),
        session_id.to_string(),
        "self-dev".to_string(),
    ]);
    let command = resume_command_with_context(
        exe,
        args,
        selfdev_title.clone(),
        context,
        "selfdev",
        session_id,
    );
    let spawned = crate::terminal_launch::spawn_command_in_new_terminal(&command, cwd)?;
    if spawned {
        focus_session_window_best_effort_with_env(
            session_id,
            &selfdev_title,
            &context.client_terminal_env,
        );
    }
    Ok(spawned)
}

#[cfg(not(unix))]
fn find_wezterm_gui_binary() -> Option<String> {
    use std::process::{Command, Stdio};

    if let Ok(exe) = std::env::var("WEZTERM_EXECUTABLE") {
        let p = std::path::Path::new(&exe);
        let gui = p.with_file_name("wezterm-gui.exe");
        if gui.exists() {
            return Some(gui.to_string_lossy().into_owned());
        }
        return Some(exe);
    }

    let candidates = [
        r"C:\Program Files\WezTerm\wezterm-gui.exe",
        r"C:\Program Files (x86)\WezTerm\wezterm-gui.exe",
    ];
    for c in &candidates {
        if std::path::Path::new(c).exists() {
            return Some(c.to_string());
        }
    }

    for bin in &["wezterm-gui", "wezterm"] {
        if let Ok(output) = Command::new("where")
            .arg(bin)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(line) = stdout.lines().next() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        if *bin == "wezterm" {
                            let p = std::path::Path::new(trimmed);
                            let gui = p.with_file_name("wezterm-gui.exe");
                            if gui.exists() {
                                return Some(gui.to_string_lossy().into_owned());
                            }
                        }
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
    }

    None
}

#[cfg(not(unix))]
fn resume_terminal_candidates_windows() -> Vec<String> {
    std::env::var("KCODE_RESUME_TERMINAL")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|candidates| !candidates.is_empty())
        .unwrap_or_else(|| {
            vec![
                "wezterm".to_string(),
                "wt".to_string(),
                "alacritty".to_string(),
            ]
        })
}

#[cfg(not(unix))]
pub fn spawn_resume_in_new_terminal(
    exe: &std::path::Path,
    session_id: &str,
    cwd: &std::path::Path,
) -> Result<bool> {
    spawn_resume_in_new_terminal_with_provider(exe, session_id, cwd, None)
}

#[cfg(not(unix))]
pub fn spawn_resume_in_new_terminal_with_provider(
    exe: &std::path::Path,
    session_id: &str,
    cwd: &std::path::Path,
    provider_key: Option<&str>,
) -> Result<bool> {
    spawn_resume_in_new_terminal_with_context(
        exe,
        session_id,
        cwd,
        provider_key,
        &SessionSpawnContext::default(),
    )
}

#[cfg(not(unix))]
pub fn spawn_resume_in_new_terminal_with_context(
    exe: &std::path::Path,
    session_id: &str,
    cwd: &std::path::Path,
    provider_key: Option<&str>,
    context: &SessionSpawnContext,
) -> Result<bool> {
    use std::process::{Command, Stdio};

    let mut jcode_args: Vec<String> = Vec::new();
    if let Some(provider_arg) = resume_provider_arg(provider_key) {
        jcode_args.push("--provider".to_string());
        jcode_args.push(provider_arg.to_string());
    }
    jcode_args.push("--resume".to_string());
    jcode_args.push(session_id.to_string());

    let hook_command = crate::terminal_launch::TerminalCommand::new(exe, jcode_args.clone())
        .title(resumed_window_title(session_id));
    let hook_command = context.apply(hook_command, "resume", session_id);
    if crate::terminal_launch::try_spawn_via_configured_hook(&hook_command, cwd) {
        return Ok(true);
    }

    let wezterm_gui = find_wezterm_gui_binary();
    let alacritty_available = Command::new("where")
        .arg("alacritty")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let wt_available = std::env::var("WT_SESSION").is_ok()
        || Command::new("where")
            .arg("wt")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

    for term in resume_terminal_candidates_windows() {
        let status = match term.as_str() {
            "wezterm" => {
                let Some(ref wezterm_bin) = wezterm_gui else {
                    continue;
                };
                let mut cmd = Command::new(wezterm_bin);
                cmd.args(["start", "--always-new-process", "--"])
                    .arg(exe)
                    .args(&jcode_args)
                    .current_dir(cwd)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                crate::platform::spawn_detached(&mut cmd)
            }
            "wt" | "windows-terminal" => {
                if !wt_available {
                    continue;
                }
                let mut cmd = Command::new("wt.exe");
                cmd.args(["-p", "Command Prompt"])
                    .arg(exe)
                    .args(&jcode_args)
                    .current_dir(cwd)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                crate::platform::spawn_detached(&mut cmd)
            }
            "alacritty" => {
                if !alacritty_available {
                    continue;
                }
                let mut cmd = Command::new("alacritty");
                cmd.args(["-e"])
                    .arg(exe)
                    .args(&jcode_args)
                    .current_dir(cwd)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                crate::platform::spawn_detached(&mut cmd)
            }
            _ => continue,
        };

        if status.is_ok() {
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(not(unix))]
pub fn spawn_selfdev_in_new_terminal(
    exe: &std::path::Path,
    session_id: &str,
    cwd: &std::path::Path,
) -> Result<bool> {
    spawn_selfdev_in_new_terminal_with_provider(exe, session_id, cwd, None)
}

#[cfg(not(unix))]
pub fn spawn_selfdev_in_new_terminal_with_provider(
    exe: &std::path::Path,
    session_id: &str,
    cwd: &std::path::Path,
    provider_key: Option<&str>,
) -> Result<bool> {
    spawn_selfdev_in_new_terminal_with_context(
        exe,
        session_id,
        cwd,
        provider_key,
        &SessionSpawnContext::default(),
    )
}

#[cfg(not(unix))]
pub fn spawn_selfdev_in_new_terminal_with_context(
    exe: &std::path::Path,
    session_id: &str,
    cwd: &std::path::Path,
    provider_key: Option<&str>,
    context: &SessionSpawnContext,
) -> Result<bool> {
    use std::process::{Command, Stdio};

    let mut jcode_args: Vec<String> = Vec::new();
    if let Some(provider_arg) = resume_provider_arg(provider_key) {
        jcode_args.push("--provider".to_string());
        jcode_args.push(provider_arg.to_string());
    }
    jcode_args.extend([
        "--resume".to_string(),
        session_id.to_string(),
        "self-dev".to_string(),
    ]);

    let hook_command = crate::terminal_launch::TerminalCommand::new(exe, jcode_args.clone())
        .title(format!("{} [self-dev]", resumed_window_title(session_id)));
    let hook_command = context.apply(hook_command, "selfdev", session_id);
    if crate::terminal_launch::try_spawn_via_configured_hook(&hook_command, cwd) {
        return Ok(true);
    }

    let wezterm_gui = find_wezterm_gui_binary();
    let alacritty_available = Command::new("where")
        .arg("alacritty")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let wt_available = std::env::var("WT_SESSION").is_ok()
        || Command::new("where")
            .arg("wt")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

    for term in resume_terminal_candidates_windows() {
        let status = match term.as_str() {
            "wezterm" => {
                let Some(ref wezterm_bin) = wezterm_gui else {
                    continue;
                };
                let mut cmd = Command::new(wezterm_bin);
                cmd.args(["start", "--always-new-process", "--"])
                    .arg(exe)
                    .args(&jcode_args)
                    .current_dir(cwd)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                crate::platform::spawn_detached(&mut cmd)
            }
            "wt" | "windows-terminal" => {
                if !wt_available {
                    continue;
                }
                let mut cmd = Command::new("wt.exe");
                cmd.args(["-p", "Command Prompt"])
                    .arg(exe)
                    .args(&jcode_args)
                    .current_dir(cwd)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                crate::platform::spawn_detached(&mut cmd)
            }
            "alacritty" => {
                if !alacritty_available {
                    continue;
                }
                let mut cmd = Command::new("alacritty");
                cmd.args(["-e"])
                    .arg(exe)
                    .args(&jcode_args)
                    .current_dir(cwd)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                crate::platform::spawn_detached(&mut cmd)
            }
            _ => continue,
        };

        if status.is_ok() {
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::{
        SessionSpawnContext, client_kcode_home_for_spawn, forward_kcode_home_for_spawn_with,
        resume_command_with_context,
    };
    use std::ffi::OsString;
    use std::path::PathBuf;

    const EXE: &str = "/usr/local/bin/kcode";

    fn resume_args() -> Vec<String> {
        vec!["--resume".to_string(), "ses-1".to_string()]
    }

    fn spawn(exe: &str, home: Option<&str>) -> (PathBuf, Vec<String>) {
        forward_kcode_home_for_spawn_with(PathBuf::from(exe), resume_args(), home)
    }

    struct KcodeHomeGuard(Option<OsString>);

    impl Drop for KcodeHomeGuard {
        fn drop(&mut self) {
            match self.0.take() {
                Some(value) => crate::env::set_var("KCODE_HOME", value),
                None => crate::env::remove_var("KCODE_HOME"),
            }
        }
    }

    #[test]
    fn client_kcode_home_is_selected_explicitly_without_mutating_process_env() {
        let context = SessionSpawnContext::kind("swarm-agent").with_client_terminal_env(vec![
            ("TMUX_PANE".to_string(), "%1".to_string()),
            ("KCODE_HOME".to_string(), "/tmp/client-store".to_string()),
        ]);
        assert_eq!(
            client_kcode_home_for_spawn(&context).as_deref(),
            Some("/tmp/client-store")
        );

        let empty = SessionSpawnContext::kind("restart")
            .with_client_terminal_env(vec![("KCODE_HOME".to_string(), String::new())]);
        assert_eq!(client_kcode_home_for_spawn(&empty), None);
    }

    #[test]
    fn client_kcode_home_is_forwarded_and_metadata_keeps_original_executable() {
        let context = SessionSpawnContext::kind("swarm-agent").with_client_terminal_env(vec![(
            "KCODE_HOME".to_string(),
            "/tmp/client-store".to_string(),
        )]);
        let command = resume_command_with_context(
            std::path::Path::new(EXE),
            resume_args(),
            "session title".to_string(),
            &context,
            "resume",
            "ses-1",
        );

        assert_eq!(
            command.metadata_program.as_deref(),
            Some(std::path::Path::new(EXE))
        );
        if cfg!(unix) {
            assert_eq!(command.program, PathBuf::from("env"));
            assert_eq!(command.args[0], "KCODE_HOME=/tmp/client-store");
            assert_eq!(command.args[1], EXE);
        } else {
            assert_eq!(command.program, PathBuf::from(EXE));
            assert_eq!(command.args, resume_args());
        }
        assert_eq!(command.client_terminal_env, context.client_terminal_env);
    }

    #[test]
    fn process_kcode_home_is_preserved_when_client_does_not_supply_one() {
        let _lock = crate::storage::lock_test_env();
        let _guard = KcodeHomeGuard(std::env::var_os("KCODE_HOME"));
        crate::env::set_var("KCODE_HOME", "/tmp/server-store");

        let context = SessionSpawnContext::kind("restart");
        let command = resume_command_with_context(
            std::path::Path::new(EXE),
            resume_args(),
            "session title".to_string(),
            &context,
            "restart",
            "ses-1",
        );

        if cfg!(unix) {
            assert_eq!(command.program, PathBuf::from("env"));
            assert_eq!(command.args[0], "KCODE_HOME=/tmp/server-store");
        } else {
            assert_eq!(command.program, PathBuf::from(EXE));
            assert_eq!(command.args, resume_args());
        }
    }

    #[test]
    fn forward_kcode_home_for_spawn_matches_the_platform_contract() {
        for home in [None, Some("")] {
            assert_eq!(
                spawn(EXE, home),
                (PathBuf::from(EXE), resume_args()),
                "{home:?}"
            );
        }

        let (program, args) = spawn(EXE, Some("/tmp/custom-kcode-home"));
        if cfg!(unix) {
            assert_eq!(program, PathBuf::from("env"));
            let mut expected = vec!["KCODE_HOME=/tmp/custom-kcode-home".to_string()];
            expected.push(EXE.to_string());
            expected.extend(resume_args());
            assert_eq!(args, expected);
        } else {
            // Windows has no `env` binary and the hook contract has no env
            // passthrough, so the command must stay byte-identical there.
            assert_eq!((program, args), (PathBuf::from(EXE), resume_args()));
        }
    }
}
