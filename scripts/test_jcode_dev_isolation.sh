#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
launcher="$repo_root/scripts/jcode-dev"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
home="$tmp/home"
mkdir -p "$home"
home="$(CDPATH= cd -- "$home" && pwd -P)"
root="$home/.jcode-dev"
mkdir -p "$root/bin" "$root/run" "$root/builds/current" \
  "$root/builds/shared-server" "$root/builds/versions/v1" "$root/cargo-target"
chmod 700 "$root"

cat > "$root/builds/versions/v1/jcode" <<'EOF'
#!/usr/bin/env bash
printf 'HOME=%s\n' "$HOME"
printf 'JCODE_HOME=%s\n' "$JCODE_HOME"
printf 'JCODE_COPILOT_ALLOW_GH_AUTH_TOKEN=%s\n' "${JCODE_COPILOT_ALLOW_GH_AUTH_TOKEN:-<unset>}"
printf 'JCODE_INSTALL_DIR=%s\n' "$JCODE_INSTALL_DIR"
printf 'JCODE_RUNTIME_DIR=%s\n' "$JCODE_RUNTIME_DIR"
printf 'CARGO_TARGET_DIR=%s\n' "$CARGO_TARGET_DIR"
printf 'XDG_CONFIG_HOME=%s\n' "$XDG_CONFIG_HOME"
printf 'XDG_DATA_HOME=%s\n' "$XDG_DATA_HOME"
printf 'XDG_STATE_HOME=%s\n' "$XDG_STATE_HOME"
printf 'XDG_CACHE_HOME=%s\n' "$XDG_CACHE_HOME"
printf 'JCODE_DEV_NAMESPACE=%s\n' "$JCODE_DEV_NAMESPACE"
printf 'JCODE_SOCKET=%s\n' "${JCODE_SOCKET:-<unset>}"
printf 'JCODE_API_SOCKET=%s\n' "${JCODE_API_SOCKET:-<unset>}"
printf 'HERDR_SOCKET_PATH=%s\n' "${HERDR_SOCKET_PATH:-<unset>}"
printf 'JCODE_REPO_DIR=%s\n' "${JCODE_REPO_DIR:-<unset>}"
printf 'ARGS=%s\n' "$*"
EOF
chmod 700 "$root/builds/versions/v1/jcode"
ln -s "$root/builds/versions/v1/jcode" "$root/builds/current/jcode"

output="$(HOME="$home" \
  JCODE_COPILOT_ALLOW_GH_AUTH_TOKEN=1 \
  JCODE_HOME="$home/.jcode" \
  JCODE_INSTALL_DIR="$home/.local/bin" \
  JCODE_RUNTIME_DIR="$home/.jcode/run" \
  CARGO_TARGET_DIR="$home/.jcode/target" \
  XDG_CONFIG_HOME="$home/.config" \
  XDG_DATA_HOME="$home/.local/share" \
  XDG_STATE_HOME="$home/.local/state" \
  XDG_CACHE_HOME="$home/.cache" \
  JCODE_SOCKET="$home/.jcode/jcode.sock" \
  JCODE_API_SOCKET="$home/.jcode/jcode-api.sock" \
  HERDR_ENV=1 \
  HERDR_PANE_ID=production-pane \
  HERDR_SOCKET_PATH="$home/herdr.sock" \
  JCODE_REPO_DIR="$home/production-checkout" \
  "$launcher" self-dev --build)"
for expected in \
  "HOME=$home" \
  "JCODE_HOME=$root" \
  'JCODE_COPILOT_ALLOW_GH_AUTH_TOKEN=<unset>' \
  "JCODE_INSTALL_DIR=$root/bin" \
  "JCODE_RUNTIME_DIR=$root/run" \
  "CARGO_TARGET_DIR=$root/cargo-target" \
  "XDG_CONFIG_HOME=$root/xdg/config" \
  "XDG_DATA_HOME=$root/xdg/data" \
  "XDG_STATE_HOME=$root/xdg/state" \
  "XDG_CACHE_HOME=$root/xdg/cache" \
  'JCODE_DEV_NAMESPACE=1' \
  'JCODE_SOCKET=<unset>' \
  'JCODE_API_SOCKET=<unset>' \
  'HERDR_SOCKET_PATH=<unset>' \
  'JCODE_REPO_DIR=<unset>' \
  'ARGS=--no-update self-dev --build'; do
  [[ "$output" == *"$expected"* ]] || {
    printf 'missing expected launcher output: %s\n%s\n' "$expected" "$output" >&2
    exit 1
  }
done

mkdir -p "$home/.jcode"
if HOME="$home/.jcode" "$launcher" --version >"$tmp/home-out" 2>"$tmp/home-err"; then
  echo 'launcher accepted HOME nested inside the production Jcode root' >&2
  exit 1
fi
if ! grep -qE 'overlaps a protected production path' "$tmp/home-err"; then
  cat "$tmp/home-err" >&2
  echo 'launcher failed for an unexpected HOME override reason' >&2
  exit 1
fi

mkdir -p "$home/.jcode/builds/current"
printf 'production sentinel\n' > "$home/.jcode/builds/current/jcode"
ln -sfn "$home/.jcode/builds/current/jcode" "$root/builds/current/jcode"
if HOME="$home" "$launcher" --version >"$tmp/out" 2>"$tmp/err"; then
  echo 'launcher accepted current symlink escaping into production' >&2
  exit 1
fi
if ! grep -qE 'escapes .*\.jcode-dev' "$tmp/err"; then
  cat "$tmp/err" >&2
  echo 'launcher failed for an unexpected reason' >&2
  exit 1
fi

ln -sfn "$root/builds/versions/v1/jcode" "$root/builds/current/jcode"
mkdir -p "$root/active_pids"
printf 'stable pid sentinel\n' > "$tmp/stable-pid-sentinel"
ln -s "$tmp/stable-pid-sentinel" "$root/active_pids/session"
if HOME="$home" "$launcher" --version >"$tmp/out" 2>"$tmp/err"; then
  echo 'launcher accepted a nested dynamic PID symlink into protected data' >&2
  exit 1
fi
if ! grep -qE 'refusing nested dynamic symlink' "$tmp/err"; then
  cat "$tmp/err" >&2
  echo 'launcher failed for an unexpected PID symlink reason' >&2
  exit 1
fi
[[ "$(cat "$tmp/stable-pid-sentinel")" == 'stable pid sentinel' ]] || {
  echo 'PID symlink check modified its target' >&2
  exit 1
}

install_home="$tmp/install-home"
mkdir -p "$install_home/.jcode"
ln -s "$install_home/.jcode" "$install_home/.jcode-dev"
if HOME="$install_home" "$repo_root/scripts/install_jcode_dev.sh" \
  >"$tmp/install-out" 2>"$tmp/install-err"; then
  echo 'installer accepted a dev root symlink into production data' >&2
  exit 1
fi
if ! grep -qE 'refusing symlinked namespace component' "$tmp/install-err"; then
  cat "$tmp/install-err" >&2
  echo 'installer failed for an unexpected reason' >&2
  exit 1
fi

version_install_home="$tmp/version-install-home"
version_install_root="$version_install_home/.jcode-dev"
mkdir -p "$version_install_home" "$version_install_root/builds"
chmod 700 "$version_install_root"
printf 'stable version sentinel\n' > "$tmp/stable-version-sentinel"
ln -s "$tmp/stable-version-sentinel" "$version_install_root/builds/current-version"
if HOME="$version_install_home" "$repo_root/scripts/install_jcode_dev.sh" \
  >"$tmp/version-install-out" 2>"$tmp/version-install-err"; then
  echo 'installer accepted a current-version symlink into stable data' >&2
  exit 1
fi
if ! grep -qE 'refusing symlinked namespace component: .*builds/current-version' \
  "$tmp/version-install-err"; then
  cat "$tmp/version-install-err" >&2
  echo 'installer failed for an unexpected version-file symlink reason' >&2
  exit 1
fi
[[ "$(cat "$tmp/stable-version-sentinel")" == 'stable version sentinel' ]] || {
  echo 'version-file symlink check modified its target' >&2
  exit 1
}

echo 'jcode-dev launcher isolation checks passed'
