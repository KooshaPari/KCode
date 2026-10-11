#!/usr/bin/env bash
# scripts/lib_install_signature.sh — Sourced by install_release.sh
#
# Provides:
#   install_signature_classify <flags-string>          -> echoes good|bad|unknown
#   install_signature_check <binary>                   -> runs the classify, emits
#                                                       a warning on bad/unknown,
#                                                       and exits non-zero on bad
#                                                       when KCODE_REQUIRE_LINKER_SIGNED=1.
#
# The classifier is pure (no side effects) and is the single source of truth
# for the linker-signed check. Test it with synthetic input strings via
# tests/install_release_signature_test.sh.

# `flags=0x20002(adhoc,linker-signed)` is the good case (amfid accepts).
# `flags=0x2(adhoc)` is the bad case (amfid SIGKILLs on non-TTY).
# Anything else (empty, parse-fail, different output) is unknown.
install_signature_classify() {
  local sig_flags
  # codesign output is uppercase on real macOS, but synthetic inputs (and
  # older systems) may be mixed case. Normalize so the case patterns can
  # be the canonical uppercase form.
  sig_flags="$(printf '%s' "${1:-}" | tr '[:lower:]' '[:upper:]')"
  case "$sig_flags" in
    *0X20002*|*CS_LINKER_SIGNED*)
      echo "good"
      ;;
    *0X2*|*CS_ADHOC*)
      echo "bad"
      ;;
    *)
      echo "unknown"
      ;;
  esac
}

# Run the check on a real binary. Emits the same warning/behavior as the
# original inline case statement, so behavior is preserved.
install_signature_check() {
  local version_dir="$1"
  local bin="$version_dir/kcode"

  if [ "$(uname -s)" != "Darwin" ] || [ ! -x "$bin" ]; then
    return 0
  fi

  local sig_flags
  sig_flags="$(codesign -dvv "$bin" 2>&1 | awk '/^CodeDirectory/ {for (i=1;i<=NF;i++) if ($i ~ /^flags=/) {print $i; exit}}')"

  local verdict
  verdict="$(install_signature_classify "$sig_flags")"

  case "$verdict" in
    good)
      : # silent — binary is good
      ;;
    bad)
      echo "WARNING: $bin is adhoc-only (${sig_flags:-unknown}), not linker-signed." >&2
      echo "         amfid will SIGKILL this binary on non-TTY launches (e.g. crash restore)." >&2
      echo "         Build with --profile release (no LTO) or set KCODE_RELEASE_PROFILE=release." >&2
      echo "         See docs/sessions/20261001-herdr-crash-persistence/10_SIGKILL_NON_TTY.md." >&2
      if [ "${KCODE_REQUIRE_LINKER_SIGNED:-0}" = "1" ]; then
        echo "Aborting per KCODE_REQUIRE_LINKER_SIGNED=1." >&2
        rm -f "$bin"
        return 1
      fi
      ;;
    *)
      echo "WARNING: could not determine signature flags for $bin (got: ${sig_flags:-empty})." >&2
      ;;
  esac
  return 0
}
