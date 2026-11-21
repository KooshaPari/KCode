//! HERDR runtime classification and spawn adapters for terminal launching.
//!
//! The classification rule itself lives in `jcode-herdr`; this module only
//! adapts the launch crate's client environment to it and owns the two HERDR
//! spawn shapes. Kept out of `lib.rs` so that oversized file does not grow
//! past its size ratchet (#329).

#[cfg(unix)]
use crate::{TerminalCommand, command_parts, shell_command, terminal_env_value};
#[cfg(unix)]
use jcode_herdr::env::{HerdrEnv, RuntimeKind};
#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::process::{Command, Stdio};

/// Which HERDR runtime the requesting client is inside, if any.
///
/// The classification rule lives in `jcode-herdr`, not here: herdr-core (HC)
/// injects `HC_TERMINAL_ID` and no legacy variable at all, so the old
/// "`HERDR_ENV` + `HERDR_PANE_ID`" test read an HC pane as "not herdr" and let
/// the spawn escape into the outer emulator. One rule, both runtimes (#317).
#[cfg(unix)]
pub(crate) fn herdr_runtime_kind(client_terminal_env: &[(String, String)]) -> RuntimeKind {
    HerdrEnv::capture_from(|key| terminal_env_value(client_terminal_env, key)).kind()
}

/// Build the herdr-core (HC) spawn command, or `None` when this is not HC.
///
/// HERDR and herdr-core (HC) are distinct runtimes with distinct verbs.
/// `pane split` / `pane run` exist only on legacy HERDR's socket API; HC's CLI
/// has no split verb at all. Its verb for "start a program in a terminal I
/// manage" is `run`, measured on hc 0.1.21: `hc run --bg --rm --cwd <dir> --
/// <argv>` returns the new terminal's address and removes it once it exits. So
/// HC gets a NEW terminal rather than a split of this one - the smallest
/// mapping its surface allows, and still infinitely better than the old
/// behaviour of escaping into the outer emulator.
#[cfg(unix)]
pub(crate) fn hc_run_command(command: &TerminalCommand, cwd: &Path) -> Option<Command> {
    if herdr_runtime_kind(&command.client_terminal_env) != RuntimeKind::Hc {
        return None;
    }
    let program_line = shell_command(&command_parts(command));
    // No `&& exit` here: HC runs the program directly, so the terminal ends
    // with it - there is no lingering interactive shell to close. `--rm`
    // keeps `hc ls` clean, at the cost of a failed worker's screen
    // disappearing once it exits, so errors are reported through jcode's own
    // output instead of a parked terminal. `--cwd` is explicit because HC's
    // default is the core's cwd, not the requesting client's.
    let mut cmd = Command::new("hc");
    cmd.current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .args(["run", "--bg", "--rm", "--cwd"])
        .arg(cwd)
        .arg("--")
        .arg("sh")
        .arg("-c")
        .arg(&program_line);
    if command.fresh_spawn {
        cmd.env("KCODE_FRESH_SPAWN", "1");
    }
    Some(cmd)
}

/// Build the legacy HERDR `pane split` + `pane run` spawn command.
#[cfg(unix)]
pub(crate) fn legacy_herdr_spawn(
    command: &TerminalCommand,
    cwd: &Path,
    program_line: &str,
) -> Command {
    // `pane split` deliberately creates a shell and returns its pane id;
    // `pane run` is the atomic, bracketed-paste-aware way to start a command
    // in that shell. Keep the small composition here rather than requiring
    // every user to configure an equivalent spawn hook.
    let herdr = terminal_env_value(&command.client_terminal_env, "HERDR_BIN_PATH")
        .unwrap_or_else(|| "herdr".to_string());
    // Unlike tmux/zellij, herdr runs the command inside an interactive shell,
    // which would outlive jcode and leave an empty pane behind (e.g. after
    // `swarm stop`). Exit that shell when jcode exits cleanly so the pane
    // closes, but keep it open after a failure so the error stays readable.
    let shell = format!("{} && exit", program_line);
    let script = concat!(
        "set -eu; ",
        "response=\"$(\"$1\" pane split --current --direction right --cwd \"$2\" --focus)\"; ",
        "pane_id=\"$(printf '%s\\n' \"$response\" | sed -n '",
        "s/.*\\\"pane_id\\\"[[:space:]]*:[[:space:]]*\\\"\\([^\\\"]*\\)\\\".*/\\1/p' | head -n 1)\"; ",
        "test -n \"$pane_id\"; ",
        "exec \"$1\" pane run \"$pane_id\" \"$3\""
    );
    let mut cmd = Command::new("sh");
    cmd.current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .args(["-c", script, "kcode-herdr-spawn", &herdr])
        .arg(cwd)
        .arg(shell);
    if command.fresh_spawn {
        cmd.env("KCODE_FRESH_SPAWN", "1");
    }
    cmd
}

#[cfg(all(test, unix))]
mod tests {
    use super::herdr_runtime_kind;
    use crate::{
        TerminalCommand, build_spawn_command, detected_resume_terminal_with_client_env,
        resume_terminal_candidates_with_client_env,
    };
    use jcode_herdr::env::RuntimeKind;
    use std::path::Path;

    #[test]
    fn hc_client_env_is_detected_as_herdr() {
        // herdr-core injects `HC_TERMINAL_ID` and no legacy variable. Before
        // #317 this env produced no candidate at all, so the spawn escaped
        // into the outer emulator instead of staying in the HC workspace.
        let hc_env = vec![("HC_TERMINAL_ID".to_string(), "t1".to_string())];
        assert_eq!(
            detected_resume_terminal_with_client_env(&hc_env),
            Some("herdr".to_string())
        );
        assert_eq!(
            resume_terminal_candidates_with_client_env(&hc_env, None)[0],
            "herdr"
        );
    }

    #[test]
    fn hc_spawn_adapter_uses_hc_run() {
        let command = TerminalCommand::new(
            "/usr/local/bin/jcode",
            vec!["--resume".to_string(), "ses hc".to_string()],
        )
        .client_terminal_env(vec![
            ("HC_TERMINAL_ID".to_string(), "t1".to_string()),
            ("HC_SOCKET".to_string(), "/tmp/hc.sock".to_string()),
        ]);

        let cmd = build_spawn_command("herdr", &command, Path::new("/work/a b"))
            .expect("HC spawn command should build");
        assert_eq!(cmd.get_program().to_string_lossy(), "hc");
        let args: Vec<String> = cmd
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            vec![
                "run",
                "--bg",
                "--rm",
                "--cwd",
                "/work/a b",
                "--",
                "sh",
                "-c",
                "'/usr/local/bin/jcode' '--resume' 'ses hc'",
            ]
        );
        // HC has no persistent shell to close, so `&& exit` (legacy HERDR's
        // empty-pane guard) must not leak into the HC path.
        assert!(!args.iter().any(|arg| arg.contains("&& exit")));
        assert!(!args.iter().any(|arg| arg.contains("pane split")));
    }

    #[test]
    fn legacy_herdr_client_env_keeps_pane_split_adapter() {
        let command =
            TerminalCommand::new("/usr/local/bin/jcode", Vec::new()).client_terminal_env(vec![
                ("HERDR_ENV".to_string(), "1".to_string()),
                ("HERDR_PANE_ID".to_string(), "w1:p1".to_string()),
            ]);
        let cmd = build_spawn_command("herdr", &command, Path::new("/work/dir"))
            .expect("legacy HERDR spawn command should build");
        assert_eq!(cmd.get_program().to_string_lossy(), "sh");
        let script = cmd
            .get_args()
            .nth(1)
            .map(|arg| arg.to_string_lossy().into_owned())
            .unwrap();
        assert!(script.contains("pane split --current --direction right"));
    }

    #[test]
    fn classification_matches_herdr_env_kind() {
        // Guard that this module really delegates the decision instead of
        // re-deriving it: an HC-only env is HC, a legacy env is legacy.
        let hc = vec![("HC_TERMINAL_ID".to_string(), "t1".to_string())];
        let legacy = vec![
            ("HERDR_ENV".to_string(), "1".to_string()),
            ("HERDR_PANE_ID".to_string(), "w1:p1".to_string()),
        ];
        assert_eq!(herdr_runtime_kind(&hc), RuntimeKind::Hc);
        assert_ne!(herdr_runtime_kind(&legacy), RuntimeKind::None);
    }
}
