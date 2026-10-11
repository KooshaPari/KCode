//! End-to-end guard for the prerelease-channel fix (Kilo-review CRITICAL #4).
//!
//! The build script derives `JCODE_BASE_SEMVER` / `JCODE_UPDATE_SEMVER` from the
//! root `Cargo.toml` `[package].version`. For the fork that version carries a
//! `-k<major>.<minor>.<patch>` channel (`0.88.0-k1.2.0`); those env values must
//! keep the channel rather than collapsing to the bare numeric core (`0.88.0`).
//!
//! These tests read the same root `Cargo.toml` the build script reads and assert
//! the compiled-in accessors agree with it, so a regression in `build.rs`
//! (e.g. dropping `pkg_suffix`) fails `cargo test -p jcode-build-meta`.

use std::fs;
use std::path::PathBuf;

/// Mirror of the build script's `root_package_version`: parse `[package].version`
/// from the workspace-root `Cargo.toml` without a toml dependency.
fn root_package_version() -> String {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let data =
        fs::read_to_string(&root).unwrap_or_else(|err| panic!("read {}: {err}", root.display()));
    let mut in_package = false;
    for line in data.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_package = trimmed == "[package]";
            continue;
        }
        if in_package && let Some(rest) = trimmed.strip_prefix("version") {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix('=') {
                let value = rest.trim().trim_matches('"').to_string();
                if !value.is_empty() {
                    return value;
                }
            }
        }
    }
    panic!("no [package].version found in {}", root.display());
}

/// Independent oracle: the prerelease channel including its leading `-`
/// (`0.88.0-k1.2.0` -> `-k1.2.0`, `0.88.0` -> `""`).
fn channel_of(version: &str) -> String {
    let trimmed = version.trim().trim_start_matches('v');
    let core_len = trimmed.find('-').unwrap_or(trimmed.len());
    trimmed[core_len..].to_string()
}

#[test]
fn base_and_update_semver_carry_the_package_channel() {
    let pkg = root_package_version();
    let channel = channel_of(&pkg);

    let base = jcode_build_meta::BASE_SEMVER;
    let update = jcode_build_meta::UPDATE_SEMVER;
    let version = jcode_build_meta::VERSION;

    if channel.is_empty() {
        // Upstream (non-fork) versions have no channel: values stay bare.
        assert!(
            !base.contains('-'),
            "upstream BASE_SEMVER must be channel-less, got `{base}`"
        );
        assert!(
            !update.contains('-'),
            "upstream UPDATE_SEMVER must be channel-less, got `{update}`"
        );
    } else {
        assert!(
            base.ends_with(&channel),
            "BASE_SEMVER `{base}` must end with channel `{channel}`"
        );
        assert!(
            update.ends_with(&channel),
            "UPDATE_SEMVER `{update}` must end with channel `{channel}`"
        );
        assert!(
            version.contains(&channel),
            "VERSION `{version}` must include channel `{channel}`"
        );
    }
}

#[test]
fn base_semver_matches_root_package_version_core_and_channel() {
    let pkg = root_package_version();
    let expected = pkg.trim_start_matches('v');
    // With no explicit JCODE_BUILD_SEMVER override, base semver is exactly the
    // root package version (numeric core + channel).
    if std::env::var("JCODE_BUILD_SEMVER").is_err() {
        assert_eq!(
            jcode_build_meta::BASE_SEMVER,
            expected,
            "BASE_SEMVER must equal the root [package].version"
        );
    }
}
