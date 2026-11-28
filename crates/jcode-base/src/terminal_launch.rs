use anyhow::{Result, bail};
pub use jcode_terminal_launch::{
    SpawnAttempt, TerminalCommand, apply_client_terminal_env, build_hook_spawn_command,
    detected_resume_terminal, expand_home, parse_hook_command, resume_terminal_candidates,
    sh_escape, shell_command, snapshot_client_terminal_env, spawn_command_in_new_terminal_with,
};
use std::path::Path;

/// The configured external spawn hook, if any.
///
/// Resolution order: `KCODE_SPAWN_HOOK` env (handled by config env overrides;
/// empty value disables) then `[terminal] spawn_hook` in config.toml.
pub fn configured_spawn_hook() -> Option<String> {
    crate::config::config()
        .terminal
        .spawn_hook
        .as_deref()
        .map(str::trim)
        .filter(|hook| !hook.is_empty())
        .map(str::to_string)
}

/// Spawn `command` in a new terminal window/pane.
///
/// When a spawn hook is configured (`[terminal] spawn_hook` / `KCODE_SPAWN_HOOK`),
/// the hook takes over the spawn: jcode runs `<hook> <program> <args...>` with
/// `KCODE_SPAWN_*` metadata env vars so external programs (tmux, kitty remote,
/// herd, window managers) control where and how the session appears. If the
/// hook cannot be started, jcode falls back to its built-in terminal detection.
pub fn spawn_command_in_new_terminal(command: &TerminalCommand, cwd: &Path) -> Result<bool> {
    if try_spawn_via_configured_hook(command, cwd) {
        return Ok(true);
    }
    jcode_terminal_launch::spawn_command_in_new_terminal_with(command, cwd, |cmd| {
        let child = crate::platform::spawn_detached(cmd)?;
        // The terminal outlives this call, so collect its exit status rather
        // than dropping the handle and leaking a zombie when it exits.
        crate::platform::reap_detached(child);
        Ok(())
    })
}

/// Try launching via the configured spawn hook.
///
/// Returns `true` when the hook process started; `false` when no hook is
/// configured or it failed to start (callers should fall back to the built-in
/// terminal spawning for their platform).
pub fn try_spawn_via_configured_hook(command: &TerminalCommand, cwd: &Path) -> bool {
    let Some(hook) = configured_spawn_hook() else {
        return false;
    };
    match spawn_via_hook(&hook, command, cwd) {
        Ok(()) => true,
        Err(error) => {
            crate::logging::warn(&format!(
                "Spawn hook '{hook}' failed ({error}); falling back to built-in terminal spawn"
            ));
            false
        }
    }
}

fn spawn_via_hook(hook: &str, command: &TerminalCommand, cwd: &Path) -> Result<()> {
    let mut cmd = build_hook_spawn_command(hook, command, cwd)?;
    let mut child = cmd.spawn()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                bail!("hook exited with {status}");
            }
            break;
        }
        if std::time::Instant::now() >= deadline {
            // Long-running hooks may intentionally own their terminal process.
            // Reap them asynchronously while treating survival past the startup
            // window as successful launch admission.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    crate::logging::info(&format!(
        "Spawn hook '{hook}' launched terminal spawn (kind={:?} session={:?})",
        command.kind, command.session_id
    ));
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::ffi::OsString;

    struct KcodeHomeGuard(Option<OsString>);

    impl Drop for KcodeHomeGuard {
        fn drop(&mut self) {
            match self.0.take() {
                Some(value) => crate::env::set_var("KCODE_HOME", value),
                None => crate::env::remove_var("KCODE_HOME"),
            }
        }
    }

    fn wait_for_file(path: &std::path::Path) -> String {
        for _ in 0..100 {
            if let Ok(data) = std::fs::read_to_string(path)
                && !data.is_empty()
            {
                return data;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("hook child did not write {}", path.display());
    }

    #[test]
    fn launch_hook_forwards_client_kcode_home_to_child_argv() {
        use std::os::unix::fs::PermissionsExt;

        let _lock = crate::storage::lock_test_env();
        let _guard = KcodeHomeGuard(std::env::var_os("KCODE_HOME"));
        crate::env::set_var("KCODE_HOME", "/tmp/server-store");

        let temp = tempfile::TempDir::new().expect("temp dir");
        let metadata_record = temp.path().join("metadata.txt");
        let child_record = temp.path().join("child.txt");
        let negative_record = temp.path().join("negative.txt");
        let hook_path = temp.path().join("hook.sh");
        let child_path = temp.path().join("child.sh");

        // The hook unsets KCODE_HOME before executing the target argv. Any
        // value seen by the child therefore came from the `env KCODE_HOME=...`
        // argv prefix, not from the hook's inherited/client env (#424).
        std::fs::write(
            &hook_path,
            format!(
                "#!/bin/sh\nprintf '%s' \"$KCODE_SPAWN_PROGRAM\" > {}\nunset KCODE_HOME\nexec \"$@\"\n",
                sh_escape(&metadata_record.to_string_lossy())
            ),
        )
        .expect("write hook");
        std::fs::write(
            &child_path,
            format!(
                "#!/bin/sh\nprintf '%s' \"${{KCODE_HOME:-<unset>}}\" > {}\n",
                sh_escape(&child_record.to_string_lossy())
            ),
        )
        .expect("write child");
        for path in [&hook_path, &child_path] {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
                .expect("chmod script");
        }

        let client_home = "/tmp/client-store";
        let command = TerminalCommand::new(
            "env",
            vec![
                format!("KCODE_HOME={client_home}"),
                child_path.to_string_lossy().into_owned(),
            ],
        )
        .metadata_program("/usr/local/bin/kcode")
        .client_terminal_env(vec![("KCODE_HOME".to_string(), client_home.to_string())])
        .kind("swarm-agent")
        .session_id("ses_hooked");
        spawn_via_hook(&hook_path.to_string_lossy(), &command, temp.path())
            .expect("hook should spawn");

        assert_eq!(
            wait_for_file(&metadata_record),
            "/usr/local/bin/kcode",
            "spawn metadata must name the original executable"
        );
        assert_eq!(
            wait_for_file(&child_record),
            client_home,
            "child must receive the client store root through the production hook argv"
        );

        // Negative control: the same client env is exported to the hook, but
        // without the argv prefix the child must not inherit KCODE_HOME.
        let negative_child_path = temp.path().join("negative-child.sh");
        std::fs::write(
            &negative_child_path,
            format!(
                "#!/bin/sh\nprintf '%s' \"${{KCODE_HOME:-<unset>}}\" > {}\n",
                sh_escape(&negative_record.to_string_lossy())
            ),
        )
        .expect("write negative child");
        std::fs::set_permissions(&negative_child_path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod negative child");
        let negative_command = TerminalCommand::new(negative_child_path, Vec::new())
            .client_terminal_env(vec![("KCODE_HOME".to_string(), client_home.to_string())]);
        spawn_via_hook(&hook_path.to_string_lossy(), &negative_command, temp.path())
            .expect("negative hook should spawn");
        assert_eq!(
            wait_for_file(&negative_record),
            "<unset>",
            "negative control must not see KCODE_HOME without the argv prefix"
        );
    }

    #[cfg(unix)]
    #[test]
    fn spawn_via_hook_runs_hook_with_metadata_env() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::TempDir::new().expect("temp dir");
        let record = temp.path().join("record.txt");
        let hook_path = temp.path().join("hook.sh");
        std::fs::write(
            &hook_path,
            format!(
                "#!/bin/sh\nprintf '%s|%s|%s|%s' \"$KCODE_SPAWN_KIND\" \"$KCODE_SPAWN_SESSION_ID\" \"$KCODE_SPAWN_SWARM_ID\" \"$*\" > {}\n",
                sh_escape(&record.to_string_lossy())
            ),
        )
        .expect("write hook");
        std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod hook");

        let command = TerminalCommand::new(
            "/usr/local/bin/jcode",
            vec!["--resume".to_string(), "ses_hooked".to_string()],
        )
        .kind("swarm-agent")
        .session_id("ses_hooked")
        .spawn_env("KCODE_SPAWN_SWARM_ID", "swarm-7");

        spawn_via_hook(&hook_path.to_string_lossy(), &command, temp.path())
            .expect("hook should spawn");

        // The hook runs detached; poll briefly for its output.
        let mut recorded = String::new();
        for _ in 0..100 {
            if let Ok(data) = std::fs::read_to_string(&record)
                && !data.is_empty()
            {
                recorded = data;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(
            recorded, "swarm-agent|ses_hooked|swarm-7|/usr/local/bin/jcode --resume ses_hooked",
            "hook should receive metadata env and the jcode command as argv"
        );
    }

    #[test]
    fn spawn_via_hook_reports_immediate_nonzero_exit() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::TempDir::new().expect("temp dir");
        let hook_path = temp.path().join("reject-outside-tmux.sh");
        std::fs::write(&hook_path, "#!/bin/sh\nexit 1\n").expect("write hook");
        std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod hook");

        let command = TerminalCommand::new(
            "/usr/local/bin/jcode",
            vec!["--resume".to_string(), "ses_fallback".to_string()],
        )
        .kind("swarm-agent")
        .session_id("ses_fallback");

        let error = spawn_via_hook(&hook_path.to_string_lossy(), &command, temp.path())
            .expect_err("non-zero hook exit must trigger built-in/headless fallback");
        assert!(error.to_string().contains("hook exited with"));
    }
}
