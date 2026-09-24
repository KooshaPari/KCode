//! HERDR integration CLI subcommands.
//!
//! - `herdr-status`: Print current HERDR environment and reporter state.
//! - `herdr-install`: Write screen detection manifests to ~/.config/herdr/.

use anyhow::Result;
use std::path::PathBuf;

/// Print HERDR pane info: env vars, socket path, pane ID, and reporter state.
pub(crate) fn run_herdr_status() -> Result<()> {
    let env = jcode_herdr::env::HerdrEnv::capture();
    let reporter_active = crate::herdr::is_active();

    println!("HERDR Pane Status");
    println!("─────────────────");
    println!(
        "  HERDR_ENV          {}",
        value_or_missing(&std::env::var("HERDR_ENV"))
    );
    println!(
        "  HERDR_PANE_ID      {}",
        value_or_missing(&std::env::var("HERDR_PANE_ID"))
    );
    println!(
        "  HERDR_SOCKET_PATH  {}",
        value_or_missing(&std::env::var("HERDR_SOCKET_PATH"))
    );
    println!(
        "  HERDR_BIN_PATH     {}",
        value_or_missing(&std::env::var("HERDR_BIN_PATH"))
    );
    println!(
        "  HERDR_WORKSPACE_ID {}",
        value_or_missing(&std::env::var("HERDR_WORKSPACE_ID"))
    );
    println!(
        "  HERDR_TAB_ID       {}",
        value_or_missing(&std::env::var("HERDR_TAB_ID"))
    );
    println!();
    println!(
        "  HERDR pane valid   {}",
        if env.is_valid() { "yes" } else { "no" }
    );
    println!(
        "  Reporter active    {}",
        if reporter_active { "yes" } else { "no" }
    );

    if !env.is_valid() {
        println!();
        println!(
            "Not running inside a HERDR pane. Launch jcode from a HERDR session to enable integration."
        );
    }

    Ok(())
}

/// Write screen detection manifests for jcode and ForgeCode, plus the
/// HERDR `herdr-plugin.toml` plugin manifests that register these
/// agents with the HERDR loader.
///
/// On Windows this is a no-op file write (we still emit the plugin
/// manifest so users on mixed OS setups can see what *would* be
/// installed), but we print a one-line hint that Herdr is Unix-only
/// and direct the user to WSL2.
pub(crate) fn run_herdr_install() -> Result<()> {
    if std::env::consts::OS == "windows" {
        println!(
            "Note: HERDR is Unix-only. On Windows, run this command from inside WSL2 \
             (Ubuntu) after installing the Herdr WSL build there."
        );
    }

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

    // Also write the local HERDR plugin manifests under
    // ~/.config/herdr/plugins/local/. These register jcode (and
    // ForgeCode) as a HERDR plugin that points back at the in-process
    // HerdrReporter this binary already provides — avoiding the need
    // to clone leonardoacosta/herdr-jcode or
    // capt-marbles/jcode-integration.
    let plugins_dir = herdr_local_plugins_dir()?;
    std::fs::create_dir_all(&plugins_dir)?;

    let jcode_plugin_path = plugins_dir.join("jcode.toml");
    let jcode_plugin_manifest = jcode_herdr::plugin::jcode_plugin();
    jcode_herdr::plugin::write_plugin(&jcode_plugin_manifest, &jcode_plugin_path)?;
    println!("Installed {}", jcode_plugin_path.display());

    let forgecode_plugin_path = plugins_dir.join("forgecode.toml");
    let forgecode_plugin_manifest = jcode_herdr::plugin::forgecode_plugin();
    jcode_herdr::plugin::write_plugin(&forgecode_plugin_manifest, &forgecode_plugin_path)?;
    println!("Installed {}", forgecode_plugin_path.display());

    println!();
    println!(
        "HERDR plugin manifests written to {}",
        plugins_dir.display()
    );
    println!(
        "These register jcode and ForgeCode as local HERDR plugins — no separate plugin clone required."
    );

    Ok(())
}

/// Return the HERDR local plugins directory
/// (`~/.config/herdr/plugins/local/`).
fn herdr_local_plugins_dir() -> Result<PathBuf> {
    let home =
        std::env::var("HOME").map_err(|_| anyhow::anyhow!("HOME environment variable not set"))?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("herdr")
        .join("plugins")
        .join("local"))
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

/// Format a var result as the value or a placeholder.
fn value_or_missing(result: &std::result::Result<String, std::env::VarError>) -> &str {
    match result {
        Ok(val) => val,
        Err(_) => "(not set)",
    }
}
