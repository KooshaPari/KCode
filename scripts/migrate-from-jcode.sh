#!/usr/bin/env bash
# scripts/migrate-from-jcode.sh
# One-time migration from jcode install to kcode install.
# Copies state from ~/.jcode/ to ~/.kcode/ (does NOT delete source).
# The backwards-compat shim in kcode-launcher keeps reading ~/.jcode/ for one release cycle.
#
# Usage:
#   ./scripts/migrate-from-jcode.sh           # perform migration
#   ./scripts/migrate-from-jcode.sh --dry-run # show what would happen
#   ./scripts/migrate-from-jcode.sh --force   # overwrite existing ~/.kcode/
#   ./scripts/migrate-from-jcode.sh --help    # this message

set -euo pipefail

SRC="${HOME}/.jcode"
DST="${HOME}/.kcode"

# Args
DRY_RUN=0
FORCE=0
for arg in "$@"; do
  case "$arg" in
    --dry-run|-n) DRY_RUN=1 ;;
    --force|-f)   FORCE=1 ;;
    --help|-h)
      sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) echo "unknown arg: $arg (use --help)" >&2; exit 2 ;;
  esac
done

run() {
  if [[ $DRY_RUN -eq 1 ]]; then
    printf '  [dry-run] %s\n' "$*"
  else
    "$@"
  fi
}

echo "migrate-from-jcode.sh: jcode -> kcode (one-time, non-destructive)"

if [[ ! -d "${SRC}" ]]; then
  echo "no source ${SRC}; nothing to migrate"
  exit 0
fi

if [[ -d "${DST}" && $FORCE -eq 0 ]]; then
  echo "destination ${DST} already exists; refusing to overwrite." >&2
  echo "  pass --force to overwrite (DESTROYING any existing kcode state)." >&2
  exit 1
fi

# Sanity-check source: refuse to migrate from an empty / unexpected dir
if [[ ! -d "${SRC}/builds" && ! -d "${SRC}/config.toml" && -z "$(ls -A "${SRC}" 2>/dev/null)" ]]; then
  echo "source ${SRC} appears empty; refusing to migrate a non-jcode dir." >&2
  exit 1
fi

# Choose copy tool: rsync preferred, fall back to cp -a
copy_tree() {
  if command -v rsync >/dev/null 2>&1; then
    rsync -a --exclude='builds/versions/' "$@"
  else
    # Fallback: tar-piped cp -a semantics with --exclude
    # rsync is preferred because --exclude= is more flexible.
    echo "  (rsync not found; using cp -a — builds/versions/ will be included; safe to delete later)" >&2
    cp -a "$@"
  fi
}

echo
echo "  source: ${SRC}"
echo "  dest:   ${DST}"
echo "  mode:   $([[ $DRY_RUN -eq 1 ]] && echo dry-run || echo apply)  $([[ $FORCE -eq 1 ]] && echo '(force)' || true)"
echo

# 1. Top-level copy (excluding the immutable build artifacts)
if [[ $DRY_RUN -eq 1 ]]; then
  echo "  [dry-run] would create ${DST} and copy ${SRC}/* (excluding builds/versions/)"
else
  mkdir -p "${DST}"
  if [[ $FORCE -eq 1 && -d "${DST}" ]]; then
    # Wipe DST before re-copying under --force so stale files don't linger
    rm -rf "${DST}"
    mkdir -p "${DST}"
  fi
  copy_tree "${SRC}/" "${DST}/"
fi

# 2. Re-link build channels: ~/.jcode/builds/<ch>/jcode -> .../kcode
for ch in current stable shared-server; do
  if [[ -L "${DST}/builds/${ch}/jcode" ]]; then
    src_target="$(readlink "${DST}/builds/${ch}/jcode")"
    # Translate the link target: replace /jcode -> /kcode in the path
    new_target="${src_target//\/jcode/\/kcode}"
    echo "  re-link ${ch}: jcode -> kcode (target: ${new_target})"
    if [[ $DRY_RUN -eq 0 ]]; then
      rm "${DST}/builds/${ch}/jcode"
      ln -s "${new_target}" "${DST}/builds/${ch}/kcode"
    fi
  fi
done

# 3. Translate session log file names (jcode-*.log -> kcode-*.log; metadata updates deferred)
shopt -s nullglob 2>/dev/null || true
for log in "${DST}"/logs/jcode-*.log; do
  if [[ -f "${log}" ]]; then
    new_log="${log//\/jcode-/\/kcode-}"
    echo "  rename: $(basename "${log}") -> $(basename "${new_log}")"
    if [[ $DRY_RUN -eq 0 ]]; then
      mv "${log}" "${new_log}"
    fi
  fi
done

echo
if [[ $DRY_RUN -eq 1 ]]; then
  echo "dry-run done. Re-run without --dry-run to apply."
else
  echo "migration done."
  echo "  KCode launcher prefers ${DST} (new) over ${SRC} (legacy)."
  echo "  You can delete ${SRC} any time after confirming \`kcode\` runs cleanly."
fi
