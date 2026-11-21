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
        push_row(&mut out, "HC_SOCKET", path_text(env.hc_socket()));
        push_row(&mut out, "HC_SESSION", value_or_missing(env.hc_session()));
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
    // `is_active()` is true whenever a reporter exists, including under HC
    // where nothing is ever pushed. Push capability is the narrower question
    // and has one definition: `HerdrReporter::is_push_capable`
    // (crates/jcode-herdr/src/reporter.rs), i.e. legacy HERDR and valid.
    push_row(
        &mut out,
        "Push-capable",
        if kind == RuntimeKind::LegacyHerdr && env.is_valid() {
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
                 or set HC_TERMINAL_ID/HC_SOCKET, to enable integration.\n",
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
    println!(
        "HERDR will use these rules to detect jcode and ForgeCode agent state from terminal output."
    );

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
        assert_eq!(row(&rendered, "HC_SOCKET"), "/tmp/hc.sock");
        assert_eq!(row(&rendered, "HC_SESSION"), "desk");
        assert_eq!(row(&rendered, "HERDR pane valid"), "yes");
        // A reporter exists under HC but nothing is ever pushed.
        assert_eq!(row(&rendered, "Reporter active"), "yes");
        assert_eq!(row(&rendered, "Push-capable"), "no");
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
    fn status_reports_incomplete_legacy_env_distinctly() {
        let env = env_from(&[("HERDR_ENV", "1"), ("HERDR_PANE_ID", "w1:p1")]);
        let rendered = render_status(&env, false);
        assert_eq!(row(&rendered, "Runtime"), "legacy herdr");
        assert_eq!(row(&rendered, "HERDR pane valid"), "no");
        assert!(rendered.contains("Legacy HERDR variables are present but incomplete"));
        assert!(!rendered.contains("Not running inside"));
    }
}
