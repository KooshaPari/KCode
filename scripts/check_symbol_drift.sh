#!/usr/bin/env bash
#
# check_symbol_drift.sh - fail when a pre-rename `jcode` spelling survives in a
# build/CI/packaging path, where it is silently inert or resolves to nothing.
#
# WHY THIS GATE EXISTS
# --------------------
# KCode is a fork of `jcode` that was renamed in the source tree. The workspace
# package is `kcode` (Cargo.toml:2) and the shipped binary is `kcode`
# (Cargo.toml:112). `jcode` survives only as the LIBRARY target name
# (Cargo.toml:108), as `jcode-*` crate names under crates/, and as the legacy
# `~/.jcode` data directory. That leaves the old spelling legal in some places
# and fatal in others, and two real defects shipped because of it:
#
#   1. CI referenced `jcode.exe` and ran `cargo build -p jcode --bin jcode`.
#      There is no `jcode` package and no `jcode` binary target, so the steps
#      died on a missing path instead of validating the artifact they built.
#
#   2. `.github/workflows/release.yml` exported JCODE_RELEASE_BUILD,
#      JCODE_CI_BUILD and JCODE_BUILD_SEMVER.
#      crates/jcode-build-meta/build.rs reads only KCODE_RELEASE_BUILD,
#      KCODE_BUILD_SEMVER, KCODE_BUILD_GIT_DIRTY, KCODE_BUILD_METADATA_FILE and
#      KCODE_BUILD_CHANGELOG_RAW; the telemetry crates read KCODE_CI_BUILD.
#      Nothing read the JCODE_ spellings and there was no back-compat shim, so
#      every release was a dev-mode build carrying a `-dev` suffix, and
#      install.ps1 rejected its own release assets on the version mismatch.
#
# Both defects were invisible in review: a build-config file is not compiled, so
# nothing type-checks the names it sets. This script is that check.
#
# WHAT IT SCANS
# -------------
# Only paths where a pre-rename spelling selects a cargo target, names an
# artifact, or sets a build variable:
#
#   .github/workflows/  .github/scripts/  scripts/  install.sh  install.ps1
#   Cargo.toml  .cargo/
#
# It deliberately does NOT scan crates/, src/ or tests/. The fork legitimately
# keeps `jcode` crate names, the `jcode` library target, `~/.jcode` legacy data
# paths and `jcode-*` interop strings there. Scanning those directories would
# produce a permanent false-positive wall and the gate would be disabled within
# a week.
#
# Comment lines (first non-whitespace character is #) are skipped. A comment
# cannot set a variable or select a target, so it cannot be inert; the nearest
# harm is a stale note, which is not worth a merge block. This also lets a fix
# be explained in place, next to the line it defends.
#
# EXIT CODES
# ----------
#   0  no drift outside the known-debt register
#   1  drift found (new occurrence, or any occurrence under --strict)
#   2  usage or environment error (bad option, nothing to scan, unreadable path)
#
# The known-debt register at the top of this file is a ratchet, following
# scripts/check_code_size_budget.py: pre-existing debt is enumerated with a
# justification and reported on every run so it stays visible, while any NEW
# occurrence fails. The register may shrink, never grow. Delete an entry only by
# fixing the code it points at.
#
# Usage:
#   scripts/check_symbol_drift.sh            # ratchet mode (CI)
#   scripts/check_symbol_drift.sh --strict   # also fail on known debt
#   scripts/check_symbol_drift.sh --help
#
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
strict=0

usage() {
  cat <<'USAGE'
Usage:
  scripts/check_symbol_drift.sh [--strict]

Fails when a pre-rename `jcode` spelling appears in a build/CI/packaging path
where it is inert or resolves to no cargo target.

Scan roots: .github/workflows/ .github/scripts/ scripts/ install.sh install.ps1 Cargo.toml .cargo/
Excluded:   crates/ src/ tests/ (the fork keeps jcode crate, lib and data names)

Options:
  --strict   Also fail on the known-debt register entries. Use this to drive
             remediation; the default mode only fails on new occurrences.
USAGE
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --strict)
      strict=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown option: $1 (try --help)"
      ;;
  esac
  shift
done

cd "$repo_root"

# ---------------------------------------------------------------------------
# Detection rules. One rule per pre-rename spelling that selects a build
# target, names an artifact, or sets a build variable. Each entry is
# "LABEL<TAB>EXTENDED_REGEX". Labels are stable identifiers used by the
# known-debt register below, so keep them in sync if a rule changes.
#
# The trailing (^|[^A-Za-z0-9_])(...)([^A-Za-z0-9_]|$) guard on the env names is
# what stops the rules matching KCODE_RELEASE_BUILD, KCODE_CI_BUILD and friends.
# The ([[:space:]]|$) guard on the cargo selectors is what stops `-p jcode`
# matching the legitimate `-p jcode-app-core` / `-p jcode-sdk` workspace crates.
# ---------------------------------------------------------------------------
RULES=(
  $'env-release-build\t(^|[^A-Za-z0-9_])JCODE_RELEASE_BUILD([^A-Za-z0-9_]|$)'
  $'env-ci-build\t(^|[^A-Za-z0-9_])JCODE_CI_BUILD([^A-Za-z0-9_]|$)'
  $'env-build-semver\t(^|[^A-Za-z0-9_])JCODE_BUILD_SEMVER([^A-Za-z0-9_]|$)'
  $'env-build-git-dirty\t(^|[^A-Za-z0-9_])JCODE_BUILD_GIT_DIRTY([^A-Za-z0-9_]|$)'
  $'env-build-metadata-file\t(^|[^A-Za-z0-9_])JCODE_BUILD_METADATA_FILE([^A-Za-z0-9_]|$)'
  $'env-build-changelog-raw\t(^|[^A-Za-z0-9_])JCODE_BUILD_CHANGELOG_RAW([^A-Za-z0-9_]|$)'
  $'cargo-package-selector\t(^|[[:space:]])-p[[:space:]]+jcode([[:space:]]|$)'
  $'cargo-bin-selector\t--bin[[:space:]]+jcode([[:space:]]|$)'
  $'windows-exe-name\t(^|[^A-Za-z0-9_.-])jcode\\.exe'
)

# ---------------------------------------------------------------------------
# Known-drift register (ratchet baseline, NOT a whitelist of correct code).
#
# Every entry names one file and one rule label. Two kinds of entry are allowed:
#
#   LEGITIMATE  the spelling is correct in that file (a comment, prose, or a
#               generated inventory of the fork's legacy JCODE_ namespace).
#   DEBT        a real pre-rename reference found by this gate on first run.
#               Each is a one-token rename. They are not fixed in this commit
#               because the commit is scoped to the gate itself; they are
#               enumerated here so they cannot silently grow.
#
# Format: relpath<TAB>label<TAB>kind<TAB>justification
# ---------------------------------------------------------------------------
KNOWN_DRIFT=(
  $'scripts/remote/README.md\twindows-exe-name\tLEGITIMATE\tProse describing a stray jcode.exe left in the user-scope remote dir by an older build; names an artifact on disk, selects nothing.'
  $'scripts/docs/jcode_env.lst\tenv-release-build\tLEGITIMATE\tGenerated inventory (scripts/docs/build_env_table.py) of the fork JCODE_ env namespace; a docs data file that never sets a variable. Stale entry, prune on next regeneration.'
  $'scripts/docs/jcode_env.lst\tenv-ci-build\tLEGITIMATE\tGenerated inventory (scripts/docs/build_env_table.py) of the fork JCODE_ env namespace; a docs data file that never sets a variable. Stale entry, prune on next regeneration.'
  $'scripts/docs/jcode_env.lst\tenv-build-semver\tLEGITIMATE\tGenerated inventory (scripts/docs/build_env_table.py) of the fork JCODE_ env namespace; a docs data file that never sets a variable. Stale entry, prune on next regeneration.'
  $'scripts/docs/jcode_env.lst\tenv-build-git-dirty\tLEGITIMATE\tGenerated inventory (scripts/docs/build_env_table.py) of the fork JCODE_ env namespace; a docs data file that never sets a variable. Stale entry, prune on next regeneration.'
  $'scripts/docs/jcode_env.lst\tenv-build-metadata-file\tLEGITIMATE\tGenerated inventory (scripts/docs/build_env_table.py) of the fork JCODE_ env namespace; a docs data file that never sets a variable.'
  $'scripts/docs/jcode_env.lst\tenv-build-changelog-raw\tLEGITIMATE\tGenerated inventory (scripts/docs/build_env_table.py) of the fork JCODE_ env namespace; a docs data file that never sets a variable.'
  $'scripts/quick-release.sh\tcargo-package-selector\tDEBT\tDev release helper; -p jcode does not resolve (no such package). Rename to -p kcode.'
  $'scripts/quick-release.sh\tcargo-bin-selector\tDEBT\tDev release helper; --bin jcode does not exist (bins are kcode, test_api, kcode-harness). Rename to --bin kcode.'
  $'scripts/bench_compile.sh\tcargo-package-selector\tDEBT\tCompile benchmark preset; -p jcode does not resolve. Rename to -p kcode.'
  $'scripts/bench_compile.sh\tcargo-bin-selector\tDEBT\tCompile benchmark preset; --bin jcode does not exist. Rename to --bin kcode.'
  $'scripts/bench_selfdev_build.sh\tcargo-package-selector\tDEBT\tSelfdev build benchmark; -p jcode does not resolve. Rename to -p kcode.'
  $'scripts/build_linux_compat.sh\tcargo-package-selector\tDEBT\tLinux compat build; -p jcode does not resolve. Rename to -p kcode.'
  $'scripts/build_linux_compat.sh\tcargo-bin-selector\tDEBT\tLinux compat build; --bin jcode does not exist. Rename to --bin kcode.'
  $'scripts/stale_server_upgrade_sandbox.sh\tcargo-package-selector\tDEBT\tSandbox harness guard message; -p jcode does not resolve. Rename to -p kcode.'
  $'scripts/test_sdk_e2e.sh\tcargo-package-selector\tDEBT\tSDK e2e build; -p jcode does not resolve. Rename to -p kcode.'
  $'scripts/test_sdk_e2e.sh\tcargo-bin-selector\tDEBT\tSDK e2e build; --bin jcode does not exist. Rename to --bin kcode.'
  $'scripts/test_desktop_selfdev.py\tcargo-package-selector\tDEBT\tDocstring showing the desktop selfdev build command; -p jcode does not resolve.'
  $'scripts/test_desktop_selfdev.py\tcargo-bin-selector\tDEBT\tDocstring showing the desktop selfdev build command; --bin jcode does not exist.'
  $'scripts/test_auth_e2e.sh\tcargo-bin-selector\tDEBT\tAuth e2e runs cargo run --bin jcode, which does not exist. Rename to --bin kcode.'
  $'scripts/auth_regression_matrix.sh\tcargo-bin-selector\tDEBT\tAuth matrix default client command; --bin jcode does not exist. Rename to --bin kcode.'
  $'scripts/auth_fixture.sh\tcargo-bin-selector\tDEBT\tAuth fixture client command; --bin jcode does not exist. Rename to --bin kcode.'
  $'scripts/onboarding_sandbox.sh\tcargo-bin-selector\tDEBT\tOnboarding sandbox client command; --bin jcode does not exist. Rename to --bin kcode.'
  $'scripts/test_fast.sh\tcargo-bin-selector\tDEBT\tFast test runner; --bin jcode does not exist. Rename to --bin kcode.'
  $'scripts/test_windows_launcher_install.ps1\twindows-exe-name\tDEBT\tAsserts the launcher at LOCALAPPDATA\\jcode\\bin\\jcode.exe while scripts/install.ps1:997 writes kcode.exe. Run in windows CI; expected name is kcode.exe.'
  $'scripts/test_windows_setup_evaluation.ps1\twindows-exe-name\tDEBT\tStale assertion, not a fixture: line 272 asserts a CLEAN INSTALL populates BuildsDir\\stable\\jcode.exe and line 416 asserts that path is a managed executable, while scripts/install.ps1:1059/1123 write kcode.exe. Would fail if run. Hidden because the windows-build-test job is `if: false` in ci.yml. A rename alone is not enough - the test needs to run on a Windows host to confirm.'
  $'scripts/test_install_conversion.sh\twindows-exe-name\tDEBT\tAsserts the converted Windows tree contains jcode.exe. Two problems, not one: (a) the name is pre-rename, and (b) the test drives scripts/install.sh expecting a release DOWNLOAD, which this fork disables - running it locally exits 1 immediately with "KCode release downloads are disabled". The whole setup-friction job is `if: false` in ci.yml for that reason, so fixing the name alone will not make it pass; the installer path has to be restored first.'
)

SCAN_ROOTS=(
  .github/workflows
  .github/scripts
  scripts
  install.sh
  install.ps1
  Cargo.toml
  .cargo
)

self_rel="scripts/check_symbol_drift.sh"

existing_roots=()
for root in "${SCAN_ROOTS[@]}"; do
  if [[ -e "$root" ]]; then
    existing_roots+=("$root")
  fi
done
if [[ "${#existing_roots[@]}" -eq 0 ]]; then
  die "no scan roots exist under $repo_root (wrong working tree?)"
fi

printf '=== Rename Symbol Drift Gate ===\n'
printf 'scan roots: %s\n' "${existing_roots[*]}"
printf 'excluded:   crates/ src/ tests/ (jcode crate, lib and data names are legitimate there)\n'

# Parse "path:line:content" from grep -Hn output.
hit_path() { printf '%s\n' "${1%%:*}"; }
hit_content() { local rest=${1#*:}; printf '%s\n' "${rest#*:}"; }

declare -a new_hits=()
declare -a known_hits=()
declare -a seen_known=()

for rule in "${RULES[@]}"; do
  label=${rule%%$'\t'*}
  pattern=${rule#*$'\t'}
  set +e
  raw=$(grep -rInE --binary-files=without-match -e "$pattern" "${existing_roots[@]}" 2>&1)
  status=$?
  set -e
  if [[ "$status" -gt 1 ]]; then
    die "grep failed while scanning rule '$label': $raw"
  fi
  [[ "$status" -eq 0 && -n "$raw" ]] || continue

  while IFS= read -r line; do
    [[ -n "$line" ]] || continue
    path=$(hit_path "$line")
    [[ "$path" == "$self_rel" ]] && continue
    content=$(hit_content "$line")
    trimmed=${content#"${content%%[![:space:]]*}"}
    [[ "${trimmed:0:1}" == "#" ]] && continue
    # grep may prefix ./ on some platforms; normalise for matching and output.
    path=${path#./}

    match=""
    for entry in "${KNOWN_DRIFT[@]}"; do
      entry_path=${entry%%$'\t'*}
      rest=${entry#*$'\t'}
      entry_label=${rest%%$'\t'*}
      if [[ "$entry_path" == "$path" && "$entry_label" == "$label" ]]; then
        match=$entry
        break
      fi
    done

    if [[ -n "$match" ]]; then
      known_hits+=("$line [rule: $label]")
      seen_known+=("$path"$'\t'"$label")
    else
      new_hits+=("$line [rule: $label]")
    fi
  done <<< "$raw"
done

# Report register entries that no longer match anything: rot in the baseline.
stale_entries=()
for entry in "${KNOWN_DRIFT[@]}"; do
  entry_path=${entry%%$'\t'*}
  rest=${entry#*$'\t'}
  entry_label=${rest%%$'\t'*}
  found=0
  for seen in "${seen_known[@]:-}"; do
    [[ "$seen" == "$entry_path"$'\t'"$entry_label" ]] && found=1 && break
  done
  if [[ "$found" -eq 0 ]]; then
    stale_entries+=("$entry_path [rule: $entry_label]")
  fi
done

for line in "${known_hits[@]:-}"; do
  [[ -n "$line" ]] || continue
  printf 'known drift: %s\n' "$line" >&2
done

if [[ "${#stale_entries[@]}" -gt 0 ]]; then
  printf 'warning: known-drift register entry no longer matches (prune it):\n' >&2
  for entry in "${stale_entries[@]}"; do
    printf '  - %s\n' "$entry" >&2
  done
fi

if [[ "${#new_hits[@]}" -gt 0 ]]; then
  printf '\n' >&2
  printf 'Pre-rename spelling in a build/CI/packaging path. A build-config file is\n' >&2
  printf 'not compiled, so nothing else will catch this:\n' >&2
  for line in "${new_hits[@]}"; do
    printf '  %s\n' "$line" >&2
  done
  printf '\n' >&2
  printf 'The workspace package is kcode (Cargo.toml:2) and the binary is kcode\n' >&2
  printf '(Cargo.toml:112). Use -p kcode / --bin kcode, kcode.exe, and the KCODE_\n' >&2
  printf 'build variables that crates/jcode-build-meta/build.rs actually reads.\n' >&2
  exit 1
fi

if [[ "$strict" -eq 1 && "${#known_hits[@]}" -gt 0 ]]; then
  printf '\nerror: --strict given and %d known-drift occurrence(s) remain\n' "${#known_hits[@]}" >&2
  printf 'Fix them, then delete the matching entry from KNOWN_DRIFT in %s.\n' "$self_rel" >&2
  exit 1
fi

if [[ "$strict" -eq 1 && "${#stale_entries[@]}" -gt 0 ]]; then
  printf '\nerror: --strict given and %d stale register entry(s) remain\n' "${#stale_entries[@]}" >&2
  exit 1
fi

printf '=== Rename drift gate passed: 0 new occurrences, %d known (%d register entries) ===\n' \
  "${#known_hits[@]}" "${#KNOWN_DRIFT[@]}"
exit 0
