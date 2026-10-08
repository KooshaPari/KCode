#!/usr/bin/env bash
# Install the current release binary into the immutable version store,
# update the stable + current channel symlinks, and point the launcher at current.
#
# Paths after install:
# - ~/.kcode/builds/versions/<hash>/kcode (immutable)
# - ~/.kcode/builds/stable/kcode -> .../versions/<hash>/kcode
# - ~/.kcode/builds/current/kcode -> .../versions/<hash>/kcode
# - ~/.local/bin/kcode -> ~/.kcode/builds/current/kcode (launcher)
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

# shellcheck source=scripts/lib_install_signature.sh
. "$repo_root/scripts/lib_install_signature.sh"

# Profile default: `release` (no LTO) for macOS linker-signed reliability.
# LTO + high `codegen-units` is known-fragile for Apple's CS_LINKER_SIGNED
# attribute (it sometimes produces flags=0x2 instead of 0x20002, which
# causes amfid to SIGKILL on non-TTY launches — see
# docs/sessions/20261001-herdr-crash-persistence/10_SIGKILL_NON_TTY.md for
# the full root cause). Operators who want LTO can opt in with
# KCODE_RELEASE_PROFILE=release-lto, and the post-install codesign verifier
# in this script will surface a warning if the LTO path drops
# CS_LINKER_SIGNED.
profile="${KCODE_RELEASE_PROFILE:-release}"
if [[ "${1:-}" == "--fast" ]]; then
  profile="release"
  shift
fi

if [[ "${1:-}" == "--lto" ]]; then
  profile="release-lto"
  shift
fi

if [[ "$#" -gt 0 ]]; then
  echo "Usage: $0 [--fast] [--lto]" >&2
  exit 1
fi

case "$profile" in
  release-lto)
    echo "Building with LTO (this takes a few minutes)..."
    ;;
  release)
    echo "Building fast release profile (no LTO)..."
    ;;
  *)
    echo "Unsupported profile: $profile (expected: release or release-lto)" >&2
    exit 1
    ;;
esac

git_hash=""
git_date=""
git_dirty="0"
if command -v git >/dev/null 2>&1; then
  if git -C "$repo_root" rev-parse --git-dir >/dev/null 2>&1; then
    git_hash="$(git -C "$repo_root" rev-parse --short HEAD 2>/dev/null || true)"
    git_date="$(git -C "$repo_root" log -1 --format=%ci 2>/dev/null || true)"
    if [[ -n "${git_hash}" ]] && [[ -n "$(git -C "$repo_root" status --porcelain 2>/dev/null || true)" ]]; then
      git_dirty="1"
    fi
  fi
fi

hash="$git_hash"
if [[ -n "$hash" ]] && [[ "$git_dirty" == "1" ]]; then
  hash="${hash}-dirty"
fi
if [[ -z "$hash" ]]; then
  hash="$(date +%Y%m%d%H%M%S)"
fi

if [[ -n "$git_hash" ]]; then
  KCODE_BUILD_GIT_HASH="$git_hash" \
    KCODE_BUILD_GIT_DATE="$git_date" \
    KCODE_BUILD_GIT_DIRTY="$git_dirty" \
    cargo build --profile "$profile" --manifest-path "$repo_root/Cargo.toml"
else
  cargo build --profile "$profile" --manifest-path "$repo_root/Cargo.toml"
fi
bin="$repo_root/target/$profile/kcode"

if [[ ! -x "$bin" ]]; then
  echo "Release binary not found: $bin" >&2
  exit 1
fi

if [[ -n "$git_hash" ]]; then
  expected_git_identity="($git_hash)"
  if [[ "$git_dirty" == "1" ]]; then
    expected_git_identity="($git_hash, dirty)"
  fi
  if [[ "$($bin --version)" != *"$expected_git_identity"* ]]; then
    echo "Release binary does not report expected git identity: $expected_git_identity" >&2
    exit 1
  fi
fi

# Install versioned binary into ~/.kcode/builds/versions/<hash>/
builds_dir="$HOME/.kcode/builds"
version_dir="$builds_dir/versions/$hash"
mkdir -p "$version_dir"
install -m 755 "$bin" "$version_dir/kcode"

# Post-install signature verification (macOS):
#
# macOS `amfid` rejects binaries that are only adhoc-signed (flags=0x2) when
# launched from a non-TTY context, sending SIGKILL. Binaries produced with
# LTO (`lto = "thin"` or fat-LTO) and the LLVM linker-plugin path take that
# path. The reliable signals we want are:
#
#   flags=0x20002(adhoc,linker-signed)  -- produced by `cargo build --release`
#                                          (no LTO) and by older upstream release
#                                          builds. amfid accepts.
#   flags=0x2(adhoc)                    -- produced when LTO is enabled and the
#                                          build skips linkersigned output.
#                                          amfid REJECTS, SIGKILL on launch.
#
# Detect the bad case, surface it loudly, and let the install either continue
# (default) or abort (when KCODE_REQUIRE_LINKER_SIGNED=1 is set). The default
# keeps the install green for CI/dev workflows that intentionally ship
# LTO-only binaries, but prints a one-line warning that is grep-friendly.
#
# See docs/sessions/20261001-herdr-crash-persistence/10_SIGKILL_NON_TTY.md.
if [ "$(uname -s)" = "Darwin" ] && [ -x "$version_dir/kcode" ]; then
  # Classifier lives in scripts/lib_install_signature.sh (unit-tested by
  # tests/install_release_signature_test.sh).
  install_signature_check "$version_dir"
fi

# Update stable symlink
stable_dir="$builds_dir/stable"
mkdir -p "$stable_dir"
ln -sfn "$version_dir/kcode" "$stable_dir/kcode"

# Update stable-version marker
printf '%s\n' "$hash" > "$builds_dir/stable-version"

# Update current symlink + marker
current_dir="$builds_dir/current"
mkdir -p "$current_dir"
ln -sfn "$version_dir/kcode" "$current_dir/kcode"
printf '%s\n' "$hash" > "$builds_dir/current-version"

# Update launcher path to current channel
install_dir="${KCODE_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$install_dir"
ln -sfn "$current_dir/kcode" "$install_dir/kcode"

echo "Installed: $version_dir/kcode"
echo "Updated stable symlink: $stable_dir/kcode -> $version_dir/kcode"
echo "Updated current symlink: $current_dir/kcode -> $version_dir/kcode"
echo "Updated launcher symlink: $install_dir/kcode -> $current_dir/kcode"

# Configure supported desktop launch hotkeys as part of installation. This is
# idempotent and best-effort because headless installs may not expose a desktop
# session; the first interactive launch retries automatically.
case "$(uname -s)" in
  Darwin)
    if "$install_dir/kcode" setup-launcher </dev/null >/dev/null 2>&1; then
      echo "Installed macOS launcher and turn-notification broker."
    fi
    if "$install_dir/kcode" setup-hotkey </dev/null >/dev/null 2>&1; then
      echo "Configured system-wide kcode launch hotkeys (when supported)."
    fi
    ;;
  Linux)
    if "$install_dir/kcode" setup-hotkey </dev/null >/dev/null 2>&1; then
      echo "Configured system-wide kcode launch hotkeys (when supported)."
    fi
    ;;
esac

# Gracefully reload any running background server onto the binary we just
# installed (issue #291). `server reload` only reloads when the running daemon
# is genuinely older, hands live headless/swarm sessions to the new process, and
# is a no-op when no server is running, so it is safe to call unconditionally.
if [ "${KCODE_SKIP_SERVER_RELOAD:-}" != "1" ]; then
  if "$install_dir/kcode" server reload </dev/null >/dev/null 2>&1; then
    echo "Reloaded the running kcode server onto $hash (if one was active)."
  fi
fi

if ! echo "$PATH" | tr ':' '\n' | grep -qx "$install_dir"; then
  echo ""
  echo "Tip: add $install_dir to PATH if needed."
fi

# Ensure the launcher dir is on PATH for bash, zsh and fish in future shells.
# shellcheck source=scripts/lib/configure_path.sh
. "$(dirname "$0")/lib/configure_path.sh"
kcode_configure_path "$install_dir"
