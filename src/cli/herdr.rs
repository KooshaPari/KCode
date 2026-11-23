//! HERDR integration CLI subcommands.
//!
//! - `herdr-status`: Print current HERDR environment and reporter state.
//! - `herdr-install`: Write screen detection manifests to ~/.config/herdr/.

use anyhow::Result;
use jcode_herdr::env::{HerdrEnv, RuntimeKind};
use std::path::PathBuf;

/// Print HERDR pane info: which runtime was detected, its env vars, and
/// reporter state.
pub(crate) fn run_herdr_status() -> Result<()> {
    let env = HerdrEnv::capture();
    let reporter_active = crate::herdr::is_active();
    print!("{}", render_status(&env, reporter_active));
    Ok(())
}

/// Render the status block for a captured environment.
///
/// The runtime is classified by probe, not by a variable the user was
/// expected to set: `HerdrEnv::capture` reads the live process environment and
/// `HerdrEnv::kind` (crates/jcode-herdr/src/env.rs) returns `Hc` when
/// `HC_TERMINAL_ID` is present, `LegacyHerdr` when `HERDR_ENV=1`, and `None`
/// when neither is. Every row below is printed from that same captured struct,
/// so the classification and the values cannot disagree.
fn render_status(env: &HerdrEnv, reporter_active: bool) -> String {
    let kind = env.kind();
    let mut out = String::new();

    out.push_str("HERDR Pane Status\n");
    out.push_str("─────────────────\n");
    push_row(&mut out, "Runtime", runtime_label(kind));
    push_row(
        &mut out,
        "HERDR_ENV",
        if env.active { "1" } else { MISSING },
    );
    push_row(&mut out, "HERDR_PANE_ID", value_or_missing(env.pane_id()));
    push_row(&mut out, "HERDR_SOCKET_PATH", path_text(env.socket_path()));
    push_row(&mut out, "HERDR_BIN_PATH", path_text(env.bin_path.as_ref()));
    push_row(
        &mut out,
        "HERDR_WORKSPACE_ID",
        value_or_missing(env.workspace_id.as_deref()),
    );
    push_row(
        &mut out,
        "HERDR_TAB_ID",
        value_or_missing(env.tab_id.as_deref()),
    );

    // HC rows appear whenever an HC variable is present, even if the kind
    // resolved to legacy/none: a stray `HC_SOCKET` without `HC_TERMINAL_ID` is
    // exactly the misconfiguration this command exists to surface.
    if kind == RuntimeKind::Hc || env.hc_terminal_id().is_some() || env.hc_socket().is_some() {
        push_row(
            &mut out,
            "HC_TERMINAL_ID",
            value_or_missing(env.hc_terminal_id()),
        );
        push_row(&mut out, "HC_TERMINAL_SOCKET", path_text(env.hc_socket()));
        push_row(&mut out, "HC_SESSION", value_or_missing(env.hc_session()));
        push_row(&mut out, "HC_SCOPE", value_or_missing(env.hc_scope.as_deref()));
    }

    out.push('\n');
    push_row(
        &mut out,
        "HERDR pane valid",
        if env.is_valid() { "yes" } else { "no" },
    );
    push_row(
        &mut out,
        "Reporter active",
        if reporter_active { "yes" } else { "no" },
    );
    // Two delivery channels, two questions. `Push-capable` is socket frames
    // (`pane.report_agent`), which only legacy HERDR has. `OSC 7501` is the
    // print-on-the-terminal channel herdr-core reads, and it is the only
    // channel HC accepts. `Reporter active` says neither: it only says a
    // reporter exists. Each row has one definition, in
    // crates/jcode-herdr/src/reporter.rs.
    push_row(
        &mut out,
        "Push-capable",
        if kind == RuntimeKind::LegacyHerdr && env.is_valid() {
            "yes"
        } else {
            "no"
        },
    );
    push_row(
        &mut out,
        "OSC 7501",
        if kind == RuntimeKind::Hc && env.is_valid() {
            "yes"
        } else {
            "no"
        },
    );

    match kind {
        RuntimeKind::None => {
            out.push('\n');
            out.push_str(
                "Not running inside a HERDR or herdr-core pane. Launch kcode from a HERDR session, \
                 or set HC_TERMINAL_ID, to enable integration.\n",
            );
        }
        RuntimeKind::LegacyHerdr if !env.is_valid() => {
            out.push('\n');
            out.push_str(
                "Legacy HERDR variables are present but incomplete: needs HERDR_ENV=1, \
                 HERDR_PANE_ID and HERDR_SOCKET_PATH.\n",
            );
        }
        _ => {}
    }

    out
}

/// Machine-greppable name of the detected runtime.
fn runtime_label(kind: RuntimeKind) -> &'static str {
    match kind {
        RuntimeKind::Hc => "herdr-core (HC)",
        RuntimeKind::LegacyHerdr => "legacy herdr",
        RuntimeKind::None => "none",
    }
}

/// Append one `label  value` row.
fn push_row(out: &mut String, label: &str, value: &str) {
    out.push_str(&format!("  {label:<18} {value}\n"));
}

/// Write screen detection manifests for jcode and ForgeCode.
pub(crate) fn run_herdr_install() -> Result<()> {
    require_legacy_runtime(&HerdrEnv::capture())?;

    let dir = herdr_agent_detection_dir()?;
    std::fs::create_dir_all(&dir)?;

    let jcode_manifest = jcode_herdr::manifest::jcode_manifest();
    let forgecode_manifest = jcode_herdr::manifest::forgecode_manifest();

    let jcode_path = dir.join("jcode.toml");
    let forgecode_path = dir.join("forgecode.toml");

    jcode_herdr::manifest::write_manifest(&jcode_manifest, &jcode_path)?;
    println!("Installed {}", jcode_path.display());

    jcode_herdr::manifest::write_manifest(&forgecode_manifest, &forgecode_path)?;
    println!("Installed {}", forgecode_path.display());

    println!();
    println!("Screen manifests written to {}", dir.display());
    println!("{LEGACY_MANIFEST_NOTICE}");

    Ok(())
}

/// Refuse an install that the detected runtime would never read.
///
/// Screen manifests are a legacy-HERDR mechanism. Under herdr-core (HC) agent
/// state is inferred by HC itself (`terminal.wait` / `terminal.interrupt`), and
/// nothing reads `~/.config/herdr/agent-detection`. Writing the files anyway
/// would report success for an install that cannot do anything, so this is an
/// error rather than a warning.
///
/// Takes the whole captured environment rather than a pre-computed kind so that
/// the install decision and the `herdr status` label are always derived from
/// the same probe; `status_and_install_agree_on_runtime_kind` pins that.
fn require_legacy_runtime(env: &HerdrEnv) -> Result<()> {
    if env.kind() == RuntimeKind::Hc {
        anyhow::bail!("{LEGACY_MANIFEST_NOTICE} Nothing installed.");
    }
    Ok(())
}

/// Return the HERDR agent detection directory (~/.config/herdr/agent-detection/).
fn herdr_agent_detection_dir() -> Result<PathBuf> {
    let home =
        std::env::var("HOME").map_err(|_| anyhow::anyhow!("HOME environment variable not set"))?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("herdr")
        .join("agent-detection"))
}

/// Placeholder for an unset value.
const MISSING: &str = "(not set)";

/// One shared sentence for the legacy-only nature of screen manifests, so the
/// status text, the install footer and the HC refusal cannot drift apart.
const LEGACY_MANIFEST_NOTICE: &str = "Screen manifests are legacy-HERDR only: herdr-core (HC) \
     infers agent state from the terminal and never reads them.";

/// Format an optional value as itself or the placeholder.
fn value_or_missing(value: Option<&str>) -> &str {
    value.unwrap_or(MISSING)
}

/// Format an optional path as itself or the placeholder.
fn path_text(value: Option<&PathBuf>) -> &str {
    match value {
        Some(path) => path.to_str().unwrap_or("(non-UTF-8 path)"),
        None => MISSING,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_from(pairs: &[(&str, &str)]) -> HerdrEnv {
        HerdrEnv::capture_from(|key| {
            pairs
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_string())
        })
    }

    /// Value printed on the row with this label.
    fn row(rendered: &str, label: &str) -> String {
        rendered
            .lines()
            .find(|line| line.trim_start().starts_with(label))
            .map(|line| {
                line.trim_start()
                    .trim_start_matches(label)
                    .trim()
                    .to_string()
            })
            .unwrap_or_else(|| panic!("no {label} row in:\n{rendered}"))
    }

    #[test]
    fn status_reports_none_when_no_runtime_answers() {
        let rendered = render_status(&env_from(&[]), false);
        assert_eq!(row(&rendered, "Runtime"), "none");
        assert_eq!(row(&rendered, "HERDR pane valid"), "no");
        assert_eq!(row(&rendered, "Push-capable"), "no");
        assert_eq!(row(&rendered, "OSC 7501"), "no");
        assert!(rendered.contains("Not running inside a HERDR or herdr-core pane"));
        // No HC rows at all when nothing HC-ish is set (the hint below the
        // table does mention the variables, so match the row-prefixed form).
        assert!(!rendered.contains("  HC_TERMINAL_ID"));
        assert!(!rendered.contains("Legacy HERDR variables are present"));
    }

    #[test]
    fn status_reports_legacy_herdr_from_captured_env() {
        let env = env_from(&[
            ("HERDR_ENV", "1"),
            ("HERDR_PANE_ID", "w1:p1"),
            ("HERDR_SOCKET_PATH", "/tmp/h.sock"),
        ]);
        let rendered = render_status(&env, true);
        assert_eq!(row(&rendered, "Runtime"), "legacy herdr");
        assert_eq!(row(&rendered, "HERDR_PANE_ID"), "w1:p1");
        assert_eq!(row(&rendered, "HERDR_SOCKET_PATH"), "/tmp/h.sock");
        assert_eq!(row(&rendered, "HERDR pane valid"), "yes");
        assert_eq!(row(&rendered, "Reporter active"), "yes");
        assert_eq!(row(&rendered, "Push-capable"), "yes");
        assert_eq!(row(&rendered, "OSC 7501"), "no");
        assert!(!rendered.contains("HC_TERMINAL_ID"));
        assert!(!rendered.contains("Not running inside"));
    }

    #[test]
    fn status_reports_hc_runtime_and_hc_rows() {
        let env = env_from(&[
            ("HC_TERMINAL_ID", "t1"),
            ("HC_SOCKET", "/tmp/hc.sock"),
            ("HC_SESSION", "desk"),
        ]);
        let rendered = render_status(&env, true);
        assert_eq!(row(&rendered, "Runtime"), "herdr-core (HC)");
        assert_eq!(row(&rendered, "HC_TERMINAL_ID"), "t1");
        assert_eq!(row(&rendered, "HC_TERMINAL_SOCKET"), "/tmp/hc.sock");
        assert_eq!(row(&rendered, "HC_SESSION"), "desk");
        assert_eq!(row(&rendered, "HERDR pane valid"), "yes");
        // A reporter exists under HC, nothing is pushed, and state reaches the
        // core as OSC 7501 on this terminal instead.
        assert_eq!(row(&rendered, "Reporter active"), "yes");
        assert_eq!(row(&rendered, "Push-capable"), "no");
        assert_eq!(row(&rendered, "OSC 7501"), "yes");
        assert!(!rendered.contains("Not running inside"));
    }

    #[test]
    fn status_prefers_hc_over_stale_legacy_vars() {
        let env = env_from(&[
            ("HERDR_ENV", "1"),
            ("HERDR_PANE_ID", "w1:p1"),
            ("HERDR_SOCKET_PATH", "/tmp/h.sock"),
            ("HC_TERMINAL_ID", "t1"),
            ("HC_SOCKET", "/tmp/hc.sock"),
        ]);
        let rendered = render_status(&env, false);
        assert_eq!(row(&rendered, "Runtime"), "herdr-core (HC)");
        assert_eq!(row(&rendered, "Push-capable"), "no");
    }

    #[test]
    fn install_refuses_under_hc_and_allows_elsewhere() {
        let err = require_legacy_runtime(&env_from(&[("HC_TERMINAL_ID", "t1")]))
            .expect_err("install must not report success under HC");
        let message = err.to_string();
        assert!(message.contains("legacy-HERDR only"));
        assert!(message.contains("herdr-core (HC)"));
        assert!(message.contains("Nothing installed"));

        assert!(require_legacy_runtime(&env_from(&[])).is_ok());
        assert!(require_legacy_runtime(&env_from(&[("HERDR_ENV", "1")])).is_ok());
    }

    /// The contract: whichever kind `herdr status` names is the kind the
    /// install path branches on, for the same host environment.
    #[test]
    fn status_and_install_agree_on_runtime_kind() {
        type InstallCase = (&'static [(&'static str, &'static str)], &'static str, bool);
        let cases: [InstallCase; 4] = [
            (&[], "none", true),
            (&[("HERDR_ENV", "1")], "legacy herdr", true),
            (
                &[("HERDR_ENV", "1"), ("HERDR_PANE_ID", "w1:p1")],
                "legacy herdr",
                true,
            ),
            (&[("HC_TERMINAL_ID", "t1")], "herdr-core (HC)", false),
        ];

        for (pairs, label, install_allowed) in cases {
            let env = env_from(pairs);
            let rendered = render_status(&env, false);
            assert_eq!(row(&rendered, "Runtime"), label, "for {pairs:?}");
            assert_eq!(
                require_legacy_runtime(&env).is_ok(),
                install_allowed,
                "install decision disagrees with status label {label} for {pairs:?}"
            );
        }
    }

    #[test]
    fn status_reports_incomplete_legacy_env_distinctly() {
        let env = env_from(&[("HERDR_ENV", "1"), ("HERDR_PANE_ID", "w1:p1")]);
        let rendered = render_status(&env, false);
        assert_eq!(row(&rendered, "Runtime"), "legacy herdr");
        assert_eq!(row(&rendered, "HERDR pane valid"), "no");
        assert!(rendered.contains("Legacy HERDR variables are present but incomplete"));
        assert!(!rendered.contains("Not running inside"));
    }
}
