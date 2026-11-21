//! HERDR runtime classification for terminal launching.
//!
//! The classification rule itself lives in `jcode-herdr`; this module only
//! adapts the launch crate's client environment to it. Kept in its own file so
//! `lib.rs` does not grow past its size ratchet (#329).

#[cfg(unix)]
use jcode_herdr::env::{HerdrEnv, RuntimeKind};

#[cfg(unix)]
use crate::terminal_env_value;

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
