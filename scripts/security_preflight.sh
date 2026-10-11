#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
strict=0

usage() {
  cat <<'USAGE'
Usage:
  scripts/security_preflight.sh [--strict]

Checks:
  1) Secret-pattern scan in tracked source/docs/scripts
  2) World-writable file check under scripts/
  3) Rust dependency advisory scan via cargo-audit (when available)

Options:
  --strict   Fail if cargo-audit is not installed
USAGE
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
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
      die "unknown option: $1"
      ;;
  esac
  shift
done

cd "$repo_root"

# Temp files for the secret scan. mktemp yields a private (mode 0600),
# unpredictable path under the system temp dir rather than a fixed
# world-predictable /tmp/jcode-secret-scan*.txt (DEFECT 3): on a shared CI
# runner a prior job could pre-create the old fixed path as a symlink to a
# sensitive file, and the `mv` below would then clobber that target.
# The EXIT trap removes both files on every path, including `die`.
scan_out=$(mktemp -t jcode-secret-scan.XXXXXX)
scan_kept=$(mktemp -t jcode-secret-scan.kept.XXXXXX)
cleanup() {
  rm -f -- "$scan_out" "$scan_kept"
}
trap cleanup EXIT

echo "=== Security Preflight ==="

echo "[1/3] Scanning for likely secrets"
secret_regex='(AKIA[0-9A-Z]{16}|ASIA[0-9A-Z]{16}|gh[pousr]_[A-Za-z0-9]{36,}|xox[baprs]-[A-Za-z0-9-]{10,}|-----BEGIN (RSA|OPENSSH|EC|DSA|PGP) PRIVATE KEY-----|AIza[0-9A-Za-z_-]{35})'

# Exact placeholder literals allowed to match the scan. Applied as a
# post-scan line filter so the detection patterns themselves stay intact
# and only these verbatim strings are suppressed. Entries must be
# sequential-alphabet placeholders (never structurally valid credentials).
secret_allowlist=(
  'AKIAABCDEFGHIJKLMNOP' # redact_secrets AWS-key fixture in crates/jcode-base/src/message/tests.rs
)

set +e
mapfile -d '' tracked_files < <(git ls-files -z)
scan_status=1
if [[ "${#tracked_files[@]}" -gt 0 ]]; then
  if command -v rg >/dev/null 2>&1; then
    rg -n --color=never -e "$secret_regex" \
      --glob '!Cargo.lock' --glob '!*.snap' --glob '!*.png' --glob '!*.jpg' --glob '!*.jpeg' \
      --glob '!*.gif' --glob '!*.svg' --glob '!*.pdf' --glob '!*.woff' --glob '!*.woff2' --glob '!*.ttf' \
      "${tracked_files[@]}" > "$scan_out"
    scan_status=$?
  else
    scan_files=()
    for tracked_file in "${tracked_files[@]}"; do
      case "$tracked_file" in
        Cargo.lock|*.snap|*.png|*.jpg|*.jpeg|*.gif|*.svg|*.pdf|*.woff|*.woff2|*.ttf)
          ;;
        *)
          scan_files+=("$tracked_file")
          ;;
      esac
    done
    if [[ "${#scan_files[@]}" -gt 0 ]]; then
      grep -I -n -E "$secret_regex" "${scan_files[@]}" > "$scan_out"
      scan_status=$?
    fi
  fi
fi
set -e

if [[ "$scan_status" -gt 1 ]]; then
  die "secret scan failed to execute"
fi

# Allowlist filtering is token-precise, not whole-line (DEFECT 1). The
# previous `grep -F -v -f <allowlist>` deleted any finding *line* that merely
# contained an allowlisted placeholder; a real secret sharing that line (for
# example a multi-token dump) was silently suppressed. Here we strip only the
# allowlisted literals from each line and re-test the residue against the
# detection regex, so a line is dropped only when every secret-shaped match on
# it is an allowlisted placeholder.
# The `${#secret_allowlist[@]} -gt 0` guard is the DEFECT 4 mitigation: an
# empty allowlist must skip this block entirely, because an empty `-f` pattern
# list matches every line and would suppress every finding (gate goes green).
if [[ -s "$scan_out" && "${#secret_allowlist[@]}" -gt 0 ]]; then
  while IFS= read -r finding_line; do
    residue=$finding_line
    for placeholder in "${secret_allowlist[@]}"; do
      residue=${residue//"$placeholder"/}
    done
    # grep exit 0  => residue still holds secret material, keep the finding.
    # grep exit 1  => nothing left after allowlisting, safe to drop.
    # grep exit >1 (DEFECT 2) => the filter genuinely failed; do not swallow
    #               it with `|| true`, surface it instead of overwriting the
    #               real findings with a truncated/empty kept-file.
    if grep -qE -e "$secret_regex" <<<"$residue"; then
      printf '%s\n' "$finding_line" >> "$scan_kept"
    else
      filter_status=$?
      if [[ "$filter_status" -gt 1 ]]; then
        die "allowlist filter failed (grep exit $filter_status)"
      fi
    fi
  done < "$scan_out"
  mv -- "$scan_kept" "$scan_out"
fi

if [[ -s "$scan_out" ]]; then
  cat "$scan_out"
  die "potential secret material detected"
fi

echo "[2/3] Checking script permissions"
if find scripts -type f -perm -0002 -print -quit | grep -q .; then
  find scripts -type f -perm -0002 -print
  die "world-writable files detected under scripts/"
fi

echo "[3/3] Dependency advisories (cargo-audit)"
audit_ignores=(
  # Documented in docs/SECURITY_DEPENDENCIES.md. These are transitive
  # advisories with tracked remediation paths; keep them visible in the triage
  # doc while preventing unrelated CI/release work from being blocked.
  --ignore RUSTSEC-2026-0141 # lettre via notify-email, Boring TLS backend not used by jcode
  --ignore RUSTSEC-2026-0099 # rustls-webpki via rustls stack, awaiting upstream upgrade
  --ignore RUSTSEC-2026-0104 # rustls-webpki via rustls stack, awaiting upstream upgrade
  --ignore RUSTSEC-2026-0098 # rustls-webpki via rustls stack, awaiting upstream upgrade
  --ignore RUSTSEC-2026-0049 # rustls-webpki via rustls stack, awaiting upstream upgrade
  --ignore RUSTSEC-2026-0187 # lopdf via pdf-extract 0.8.2 (pins lopdf 0.34); PDF text extraction only, awaiting pdf-extract upgrade to lopdf >=0.42
  --ignore RUSTSEC-2026-0194 # quick-xml via wayland-scanner (proc-macro); parses trusted Wayland protocol XML at build time only, never untrusted input at runtime
  --ignore RUSTSEC-2026-0195 # quick-xml via wayland-scanner (proc-macro); same build-time-only exposure as RUSTSEC-2026-0194
)
if command -v cargo-audit >/dev/null 2>&1; then
  cargo audit "${audit_ignores[@]}"
elif cargo audit --version >/dev/null 2>&1; then
  cargo audit "${audit_ignores[@]}"
else
  if [[ "$strict" -eq 1 ]]; then
    die "cargo-audit is not installed (install with: cargo install cargo-audit --locked)"
  fi
  echo "warning: cargo-audit not installed; skipping advisory check"
fi

echo "=== Security preflight passed ==="
