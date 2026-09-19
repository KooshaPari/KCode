//! Host-platform helpers for the HERDR plugin manifest.
//!
//! HERDR is Unix-only; this module centralizes the cross-platform
//! decision tree so the manifest builders stay focused on TOML
//! structure. Every helper here is pure (no I/O, no globals) and
//! `cfg`-gated where it matters.

/// Return the host-appropriate platforms list.
///
/// HERDR is Unix-only, so we emit `["macos", "linux"]` on Unix and an
/// empty list on Windows (the Windows install hint is surfaced
/// separately).
pub fn platforms_for_host() -> Vec<String> {
    #[cfg(unix)]
    {
        let mut v = vec!["linux".to_string()];
        if std::env::consts::OS == "macos" {
            v.push("macos".to_string());
        }
        v
    }
    #[cfg(not(unix))]
    {
        Vec::new()
    }
}

/// Install instructions for Windows. Returned as a single string that
/// callers can append to the description or print directly.
pub fn windows_install_hint() -> String {
    "Herdr is Unix-only. On Windows, install WSL2 (Ubuntu) and run the \
     Herdr WSL build inside the WSL distro. Then `jcode herdr install` \
     from WSL. See docs/HERDR_VS_ACP.md for the full rationale."
        .to_string()
}

/// Recommended install command for `jcode herdr install` (or
/// `forgecode herdr install` when invoked from the ForgeCode binary).
///
/// Emits the cargo-discovered package binary name. The first element
/// is what `cargo run -p <bin>` would invoke. We resolve via the
/// `CARGO_BIN_NAME` env var (set at compile time by Cargo for bins) and
/// fall back to "jcode".
pub fn install_command_hint() -> Vec<String> {
    // CARGO_BIN_NAME is set by Cargo only when this crate is being
    // compiled as a binary. When jcode-herdr is compiled as a library
    // (the default) it is unset; we fall back to "jcode".
    let bin = option_env!("CARGO_BIN_NAME").unwrap_or("jcode").to_string();
    vec![bin, "herdr".to_string(), "install".to_string()]
}

/// Replace the trailing subcommand of an `[install_bin, "herdr", ...]`
/// argv vector with the given `new_subcmd` form. Used to derive the
/// `status` action from the `install` action without duplicating the
/// binary-name selection.
pub fn replace_cmd(mut argv: Vec<String>, new_subcmd: &str) -> Vec<String> {
    // argv looks like ["jcode", "herdr", "install"]. We want
    // ["jcode", "herdr", new_subcmd] — so keep the first two, drop
    // the rest, then push the new subcommand.
    argv.truncate(2);
    argv.push(new_subcmd.to_string());
    argv
}
