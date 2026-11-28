use anyhow::{Context, Result};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

use crate::config::ForgeCodeCliConfig;

pub(crate) const MACHINE_CONTRACT_MARKER: &str = "jcode-forgecode-machine-contract-v1";

const REQUIRED_MACHINE_FLAGS: &[&str] = &[
    "--output-format",
    "--input-format",
    "--permission-mode",
    "--resume",
    "--tools",
];

pub(crate) fn validate_machine_contract_help(help: &str) -> Result<()> {
    validate_contract_marker(help)?;
    validate_required_flags(help)?;
    Ok(())
}

fn validate_contract_marker(help: &str) -> Result<()> {
    if !has_exact_v1_marker(help) {
        anyhow::bail!(
            "Configured ForgeCode executable does not declare the required machine contract marker '{}'. \
A versioned compatibility shim is required; the official ForgeCode CLI is not assumed to implement \
KCode's structured provider protocol. Set KCODE_FORGECODE_CLI_PATH to a compatible shim.",
            MACHINE_CONTRACT_MARKER
        );
    }
    Ok(())
}

fn has_exact_v1_marker(help: &str) -> bool {
    help.lines().any(|line| line.trim() == MACHINE_CONTRACT_MARKER)
}

fn validate_required_flags(help: &str) -> Result<()> {
    let missing: Vec<&str> = REQUIRED_MACHINE_FLAGS
        .iter()
        .copied()
        .filter(|flag| !help_declares_flag(help, flag))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    anyhow::bail!(
        "Configured ForgeCode executable is missing required machine contract flags: {}. \
A versioned compatibility shim is required; the official ForgeCode CLI is not assumed to implement \
KCode's structured provider protocol. Set KCODE_FORGECODE_CLI_PATH to a compatible shim.",
        missing.join(", ")
    );
}

fn help_declares_flag(help: &str, flag: &str) -> bool {
    help.lines().any(|line| line_declares_flag(line, flag))
}

fn line_declares_flag(line: &str, flag: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let mut search_from = 0;
    while let Some(pos) = lower[search_from..].find(flag) {
        let idx = search_from + pos;
        let before = lower[..idx].chars().next_back();
        let after = lower[idx + flag.len()..].chars().next();
        let before_ok = before.is_none_or(|c| !c.is_ascii_alphanumeric() && c != '-');
        let after_ok = after.is_none_or(|c| !c.is_ascii_alphanumeric() && c != '-' && c != '_');
        if before_ok && after_ok && !is_negating_flag_prose(&lower, idx) {
            return true;
        }
        search_from = idx + flag.len();
    }
    false
}

fn is_negating_flag_prose(lower_line: &str, flag_idx: usize) -> bool {
    let prefix = &lower_line[..flag_idx];
    let negations = [
        "does not support",
        "doesn't support",
        "do not support",
        "don't support",
        "not supported",
        "unsupported",
        "missing",
        "without",
        "no support for",
        "lacks",
    ];
    negations.iter().any(|n| prefix.contains(n))
}

pub(crate) async fn probe_cli_contract(config: &ForgeCodeCliConfig) -> Result<()> {
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        Command::new(&config.cli_path)
            .arg("--help")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .context("Timed out probing ForgeCode CLI machine contract with --help")?
    .with_context(|| {
        format!(
            "Failed to probe ForgeCode CLI machine contract using {}. \
Set KCODE_FORGECODE_CLI_PATH to a compatible shim.",
            config.cli_path
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let help = format!("{stdout}\n{stderr}");

    if !output.status.success() {
        anyhow::bail!(
            "ForgeCode CLI machine contract probe failed: `{} --help` exited with status {}. \
A versioned compatibility shim is required; the official ForgeCode CLI is not assumed to implement \
KCode's structured provider protocol. Set KCODE_FORGECODE_CLI_PATH to a compatible shim.",
            config.cli_path,
            output.status
        );
    }

    validate_machine_contract_help(&help).with_context(|| {
        format!(
            "ForgeCode CLI at {} is incompatible with KCode's machine contract",
            config.cli_path
        )
    })
}

#[cfg(test)]
mod contract_tests {
    use super::*;

    fn accepted_help() -> String {
        format!(
            "{}\n--output-format <FMT>\n--input-format <FMT>\n--permission-mode <MODE>\n--resume <ID>\n--tools <TOOLS>\n",
            MACHINE_CONTRACT_MARKER
        )
    }

    #[test]
    fn current_official_forge_style_help_is_rejected() {
        let help = "Forge CLI\n--print\n--prompt\n--help\n";
        let err = validate_machine_contract_help(help).unwrap_err();
        assert!(
            err.to_string().contains(MACHINE_CONTRACT_MARKER),
            "expected marker in error: {err:#}"
        );
    }

    #[test]
    fn compatible_versioned_shim_contract_is_accepted() {
        let help = accepted_help();
        assert!(validate_machine_contract_help(&help).is_ok());
    }

    #[test]
    fn versioned_marker_without_required_flags_is_rejected() {
        let help = format!("{MACHINE_CONTRACT_MARKER}\n--help\n");
        let err = validate_machine_contract_help(&help).unwrap_err();
        let msg = err.to_string();
        for flag in REQUIRED_MACHINE_FLAGS {
            assert!(msg.contains(flag), "missing flag {flag} in error: {msg}");
        }
    }

    #[test]
    fn wrong_contract_version_is_rejected() {
        for bad in [
            "jcode-forgecode-machine-contract-v0",
            "jcode-forgecode-machine-contract-v10",
            "jcode-forgecode-machine-contract-v1-beta",
        ] {
            let mut help = accepted_help().replace(MACHINE_CONTRACT_MARKER, bad);
            if !help.contains(bad) {
                help = format!("{bad}\n{help}");
            }
            let err = validate_machine_contract_help(&help).unwrap_err();
            assert!(
                err.to_string().contains(MACHINE_CONTRACT_MARKER),
                "wrong version {bad} should name required v1 marker: {err:#}"
            );
        }
    }

    #[test]
    fn marker_in_error_prose_is_rejected() {
        let help = format!(
            "error: expected {}\n--output-format\n--input-format\n--permission-mode\n--resume\n--tools\n",
            MACHINE_CONTRACT_MARKER
        );
        let err = validate_machine_contract_help(help.as_str()).unwrap_err();
        assert!(
            err.to_string().contains(MACHINE_CONTRACT_MARKER),
            "marker in prose should be rejected: {err:#}"
        );
    }

    #[test]
    fn negating_prose_is_rejected() {
        let help = format!(
            "{}\n--output-format\n--input-format\n--permission-mode\ndoes not support --resume\n--tools\n",
            MACHINE_CONTRACT_MARKER
        );
        let err = validate_machine_contract_help(&help).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("--resume"),
            "negated --resume should be reported missing: {msg}"
        );
    }
}
