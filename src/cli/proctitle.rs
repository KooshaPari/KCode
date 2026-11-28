//! Mapping from parsed CLI arguments to an initial process title.
//!
//! This logic depends on the clap `Args`/`Command` types defined in `cli`, so
//! it lives in the CLI layer. The low-level title-setting primitives it uses
//! (`compact_process_title`, `session_name`, `set_title`) live in the
//! `process_title` core module.

use crate::cli::args::{AmbientCommand, Args, Command};
use crate::process_title::{
    PRODUCT_NAME, client_title_prefix, compact_process_title, session_name, set_title,
};

fn product_title(suffix: &str) -> String {
    format!("{PRODUCT_NAME} {suffix}")
}

pub(crate) fn initial_title(args: &Args) -> String {
    match &args.command {
        Some(Command::Coexistence { .. }) => "kcode coexistence".to_string(),
        Some(Command::Serve { .. }) => format!("{PRODUCT_NAME}:server"),
        Some(Command::Acp) => product_title("acp"),
        Some(Command::Server { .. }) => product_title("server"),
        Some(Command::Connect) => format!("{PRODUCT_NAME}:client"),
        #[cfg(unix)]
        Some(Command::ApiBridge { .. }) => product_title("api-bridge"),
        Some(Command::Run { .. }) => product_title("run"),
        Some(Command::Login { .. }) => product_title("login"),
        Some(Command::Account { .. }) => product_title("account"),
        Some(Command::Repl) => product_title("repl"),
        Some(Command::Update) => product_title("update"),
        Some(Command::Version { .. }) => product_title("version"),
        Some(Command::Usage { .. }) => product_title("usage"),
        Some(Command::Telemetry(_)) => product_title("telemetry"),
        Some(Command::SelfDev { .. }) => format!("{PRODUCT_NAME}:selfdev"),
        Some(Command::Debug { .. }) => product_title("debug"),
        Some(Command::Auth(_)) => product_title("auth"),
        Some(Command::Provider(_)) => product_title("provider"),
        Some(Command::Memory(_)) => product_title("memory"),
        Some(Command::Session(_)) => product_title("session"),
        Some(Command::Ambient(subcommand)) => match subcommand {
            AmbientCommand::RunVisible => product_title("ambient visible"),
            _ => product_title("ambient"),
        },
        Some(Command::Cloud(_)) => product_title("cloud"),
        Some(Command::Pair { .. }) => product_title("pair"),
        Some(Command::Permissions) => product_title("permissions"),
        Some(Command::Transcript { .. }) => product_title("transcript"),
        Some(Command::Dictate { .. }) => product_title("dictate"),
        Some(Command::SetupHotkey {
            listen_macos_hotkey,
            notify_cli_launch,
            listen_windows_hotkey,
            uninstall,
        }) => {
            if *listen_macos_hotkey || *listen_windows_hotkey {
                product_title("hotkey listener")
            } else if notify_cli_launch.is_some() {
                product_title("shortcut reminder")
            } else if *uninstall {
                product_title("hotkey uninstall")
            } else {
                product_title("hotkey setup")
            }
        }
        Some(Command::Browser { .. }) => product_title("browser"),
        Some(Command::Replay { .. }) => product_title("replay"),
        Some(Command::Model(_)) => product_title("model"),
        Some(Command::HerdrStatus) => product_title("herdr-status"),
        Some(Command::HerdrInstall) => product_title("herdr-install"),
        Some(Command::ProviderTestCoverage { .. }) => product_title("provider-test-coverage"),
        Some(Command::ProviderDoctor { .. }) => product_title("provider-doctor"),
        Some(Command::AuthTest { .. }) => product_title("auth-test"),
        Some(Command::Restart { .. }) => product_title("restart"),
        Some(Command::Menubar { .. }) => product_title("menubar"),
        Some(Command::SetupLauncher) => product_title("setup-launcher"),
        None => {
            if let Some(resume) = args.resume.as_deref().filter(|resume| !resume.is_empty()) {
                let prefix = client_title_prefix(crate::cli::selfdev::client_selfdev_requested());
                compact_process_title(&prefix, Some(&session_name(resume)))
            } else if crate::cli::selfdev::client_selfdev_requested() {
                format!("{PRODUCT_NAME}:selfdev")
            } else {
                format!("{PRODUCT_NAME}:client")
            }
        }
    }
}

pub(crate) fn set_initial_title(args: &Args) {
    set_title(initial_title(args));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::lock_test_env;
    use clap::Parser;

    const SELFDEV_ENV: &str = jcode_selfdev_types::CLIENT_SELFDEV_ENV;

    fn with_selfdev_env_removed<T>(f: impl FnOnce() -> T) -> T {
        let _guard = lock_test_env();
        let previous = std::env::var_os(SELFDEV_ENV);
        crate::env::remove_var(SELFDEV_ENV);
        let result = f();
        if let Some(value) = previous {
            crate::env::set_var(SELFDEV_ENV, value);
        }
        result
    }

    #[test]
    fn initial_title_labels_server() {
        with_selfdev_env_removed(|| {
            let args = Args::parse_from([PRODUCT_NAME, "serve"]);
            assert_eq!(initial_title(&args), format!("{PRODUCT_NAME}:server"));
        });
    }

    #[test]
    fn initial_title_labels_resume_client_with_short_name() {
        with_selfdev_env_removed(|| {
            let args = Args::parse_from([PRODUCT_NAME, "--resume", "session_fox_123"]);
            assert_eq!(initial_title(&args), format!("{PRODUCT_NAME}:c:fox"));
        });
    }

    #[test]
    fn initial_title_labels_selfdev_command() {
        with_selfdev_env_removed(|| {
            let args = Args::parse_from([PRODUCT_NAME, "self-dev"]);
            assert_eq!(initial_title(&args), format!("{PRODUCT_NAME}:selfdev"));
        });
    }

    #[test]
    fn initial_title_labels_windows_hotkey_listener() {
        let args = Args::parse_from([PRODUCT_NAME, "setup-hotkey", "--listen-windows-hotkey"]);
        assert_eq!(initial_title(&args), product_title("hotkey listener"));
    }

    #[test]
    fn initial_title_labels_hotkey_uninstall() {
        let args = Args::parse_from([PRODUCT_NAME, "setup-hotkey", "--uninstall"]);
        assert_eq!(initial_title(&args), product_title("hotkey uninstall"));
    }
}
