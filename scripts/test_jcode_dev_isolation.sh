#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
launcher="$repo_root/scripts/kcode-dev"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
home="$tmp/home"
mkdir -p "$home"
home="$(CDPATH= cd -- "$home" && pwd -P)"
root="$home/.kcode-dev"
mkdir -p "$root/bin" "$root/run" "$root/builds/current" \
  "$root/builds/shared-server" "$root/builds/versions/v1" "$root/cargo-target"
chmod 700 "$root"

cat > "$root/builds/versions/v1/kcode" <<'EOF'
#!/usr/bin/env bash
printf 'HOME=%s\n' "$HOME"
printf 'KCODE_HOME=%s\n' "$KCODE_HOME"
printf 'KCODE_COPILOT_ALLOW_GH_AUTH_TOKEN=%s\n' "${KCODE_COPILOT_ALLOW_GH_AUTH_TOKEN:-<unset>}"
printf 'KCODE_INSTALL_DIR=%s\n' "$KCODE_INSTALL_DIR"
printf 'KCODE_RUNTIME_DIR=%s\n' "$KCODE_RUNTIME_DIR"
printf 'CARGO_TARGET_DIR=%s\n' "$CARGO_TARGET_DIR"
printf 'XDG_CONFIG_HOME=%s\n' "$XDG_CONFIG_HOME"
printf 'XDG_DATA_HOME=%s\n' "$XDG_DATA_HOME"
printf 'XDG_STATE_HOME=%s\n' "$XDG_STATE_HOME"
printf 'XDG_CACHE_HOME=%s\n' "$XDG_CACHE_HOME"
printf 'KCODE_DEV_NAMESPACE=%s\n' "$KCODE_DEV_NAMESPACE"
printf 'KCODE_SOCKET=%s\n' "${KCODE_SOCKET:-<unset>}"
printf 'KCODE_API_SOCKET=%s\n' "${KCODE_API_SOCKET:-<unset>}"
printf 'HERDR_SOCKET_PATH=%s\n' "${HERDR_SOCKET_PATH:-<unset>}"
printf 'KCODE_REPO_DIR=%s\n' "${KCODE_REPO_DIR:-<unset>}"
printf 'ARGS=%s\n' "$*"
EOF
chmod 700 "$root/builds/versions/v1/kcode"
ln -s "$root/builds/versions/v1/kcode" "$root/builds/current/kcode"

output="$(HOME="$home" \
  KCODE_COPILOT_ALLOW_GH_AUTH_TOKEN=1 \
  KCODE_HOME="$home/.kcode" \
  KCODE_INSTALL_DIR="$home/.local/bin" \
  KCODE_RUNTIME_DIR="$home/.kcode/run" \
  CARGO_TARGET_DIR="$home/.kcode/target" \
  XDG_CONFIG_HOME="$home/.config" \
  XDG_DATA_HOME="$home/.local/share" \
  XDG_STATE_HOME="$home/.local/state" \
  XDG_CACHE_HOME="$home/.cache" \
  KCODE_SOCKET="$home/.kcode/kcode.sock" \
  KCODE_API_SOCKET="$home/.kcode/kcode-api.sock" \
  HERDR_ENV=1 \
  HERDR_PANE_ID=production-pane \
  HERDR_SOCKET_PATH="$home/herdr.sock" \
  KCODE_REPO_DIR="$home/production-checkout" \
  "$launcher" self-dev --build)"
for expected in \
  "HOME=$home" \
  "KCODE_HOME=$root" \
  'KCODE_COPILOT_ALLOW_GH_AUTH_TOKEN=<unset>' \
  "KCODE_INSTALL_DIR=$root/bin" \
  "KCODE_RUNTIME_DIR=$root/run" \
  "CARGO_TARGET_DIR=$root/cargo-target" \
  "XDG_CONFIG_HOME=$root/xdg/config" \
  "XDG_DATA_HOME=$root/xdg/data" \
  "XDG_STATE_HOME=$root/xdg/state" \
  "XDG_CACHE_HOME=$root/xdg/cache" \
  'KCODE_DEV_NAMESPACE=1' \
  'KCODE_SOCKET=<unset>' \
  'KCODE_API_SOCKET=<unset>' \
  'HERDR_SOCKET_PATH=<unset>' \
  'KCODE_REPO_DIR=<unset>' \
  'ARGS=--no-update self-dev --build'; do
  [[ "$output" == *"$expected"* ]] || {
    printf 'missing expected launcher output: %s\n%s\n' "$expected" "$output" >&2
    exit 1
  }
done

mkdir -p "$home/.kcode"
if HOME="$home/.kcode" "$launcher" --version >"$tmp/home-out" 2>"$tmp/home-err"; then
  echo 'launcher accepted HOME nested inside the production Kcode root' >&2
  exit 1
fi
if ! rg -q 'overlaps a protected production path' "$tmp/home-err"; then
  cat "$tmp/home-err" >&2
  echo 'launcher failed for an unexpected HOME override reason' >&2
  exit 1
fi

mkdir -p "$home/.kcode/builds/current"
printf 'production sentinel\n' > "$home/.kcode/builds/current/kcode"
ln -sfn "$home/.kcode/builds/current/kcode" "$root/builds/current/kcode"
if HOME="$home" "$launcher" --version >"$tmp/out" 2>"$tmp/err"; then
  echo 'launcher accepted current symlink escaping into production' >&2
  exit 1
fi
if ! rg -q 'escapes .*\.kcode-dev' "$tmp/err"; then
  cat "$tmp/err" >&2
  echo 'launcher failed for an unexpected reason' >&2
  exit 1
fi

ln -sfn "$root/builds/versions/v1/kcode" "$root/builds/current/kcode"
mkdir -p "$root/active_pids"
printf 'stable pid sentinel\n' > "$tmp/stable-pid-sentinel"
ln -s "$tmp/stable-pid-sentinel" "$root/active_pids/session"
if HOME="$home" "$launcher" --version >"$tmp/out" 2>"$tmp/err"; then
  echo 'launcher accepted a nested dynamic PID symlink into protected data' >&2
  exit 1
fi
if ! rg -q 'refusing nested dynamic symlink' "$tmp/err"; then
  cat "$tmp/err" >&2
  echo 'launcher failed for an unexpected PID symlink reason' >&2
  exit 1
fi
[[ "$(cat "$tmp/stable-pid-sentinel")" == 'stable pid sentinel' ]] || {
  echo 'PID symlink check modified its target' >&2
  exit 1
}

install_home="$tmp/install-home"
mkdir -p "$install_home/.kcode"
ln -s "$install_home/.kcode" "$install_home/.kcode-dev"
if HOME="$install_home" "$repo_root/scripts/install_jcode_dev.sh" \
  >"$tmp/install-out" 2>"$tmp/install-err"; then
  echo 'installer accepted a dev root symlink into production data' >&2
  exit 1
fi
if ! rg -q 'refusing symlinked namespace component' "$tmp/install-err"; then
  cat "$tmp/install-err" >&2
  echo 'installer failed for an unexpected reason' >&2
  exit 1
fi

echo 'kcode-dev launcher isolation checks passed'
