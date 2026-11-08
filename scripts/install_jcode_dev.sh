#!/usr/bin/env bash
set -euo pipefail
umask 077

fail() { printf 'kcode-dev install: %s\n' "$*" >&2; exit 1; }

check_private_root() {
  [[ -d "$root" ]] || return 0
  if [[ "$(uname -s)" == Darwin ]]; then
    owner="$(stat -f '%u' "$root")" || fail "cannot inspect dev root owner: $root"
    mode="$(stat -f '%Lp' "$root")" || fail "cannot inspect dev root mode: $root"
  else
    owner="$(stat -c '%u' "$root")" || fail "cannot inspect dev root owner: $root"
    mode="$(stat -c '%a' "$root")" || fail "cannot inspect dev root mode: $root"
  fi
  [[ "$owner" == "$(id -u)" ]] || fail "dev root is not owned by the current user: $root"
  [[ "$mode" != *[!0-7]* && -n "$mode" ]] || fail "cannot validate dev root mode: $root"
  (( (8#$mode & 077) == 0 )) || fail "dev root permissions expose it to other users: $root"
}

check_dynamic_roots() {
  local path link
  for path in "$root"/reload-context-*.json "$root"/telemetry_milestone_* \
    "$root"/telemetry_active_days_*.txt "$root"/telemetry_session_starts_*.txt; do
    [[ -e "$path" || -L "$path" ]] || continue
    [[ ! -L "$path" ]] || fail "refusing dynamic symlinked state file: $path"
  done
  for path in "$root/telemetry_active_sessions" "$root/reload-recovery" \
    "$root/active_pids" "$root/streaming_pids" "$root/internal_pids" \
    "$root/run/durable-state" "$root/run/kcode-swarm-state"; do
    [[ ! -L "$path" ]] || fail "refusing symlinked dynamic state root: $path"
    [[ -d "$path" ]] || continue
    link="$(find "$path" -type l -print -quit)" || fail "cannot inspect dynamic state root: $path"
    [[ -z "$link" ]] || fail "refusing nested dynamic symlink: $link"
  done
}
repo_root="$(git rev-parse --show-toplevel 2>/dev/null)" || fail 'run from a Kcode source checkout'
[[ -n "${HOME:-}" && "$HOME" = /* ]] || fail 'HOME must be absolute'
home="$(CDPATH= cd -- "$HOME" && pwd -P)"
case "$home" in
  */.kcode|*/.kcode/*|*/.kcode-dev|*/.kcode-dev/*)
    fail "HOME overlaps a protected production path: $home" ;;
esac
root="$home/.kcode-dev"
production_home="$home/.kcode"
production_launcher="$home/.local/bin/kcode"
private_storage_roots=(
  "$root/external/aws" "$root/external/azure" "$root/external/gcloud"
  "$root/external/aws/credentials" "$root/external/aws/config"
  "$root/external/azure/azureProfile.json" "$root/external/azure/az.json"
  "$root/external/azure/msal_token_cache.json" "$root/external/azure/accessTokens.json"
  "$root/external/gcloud/credentials.db" "$root/external/gcloud/access_tokens.db"
  "$root/external/gcloud/application_default_credentials.json" "$root/external/gcloud/active_config"
  "$root/external/gcloud/configurations/config_default"
  "$root/config/kcode/kcode-subscription.env" "$root/config/kcode/openrouter.env"
  "$root/config/kcode/openai.env" "$root/config/kcode/anthropic.env" "$root/config/kcode/gemini.env"
  "$root/config/kcode/cursor.env" "$root/config/kcode/bedrock.env" "$root/config/kcode/custom.env"
  "$root/config/kcode/302ai.env" "$root/config/kcode/alibaba-coding-plan.env" "$root/config/kcode/baseten.env"
  "$root/config/kcode/belvedir.env" "$root/config/kcode/celeris.env" "$root/config/kcode/cerebras.env"
  "$root/config/kcode/chutes.env" "$root/config/kcode/comtegra.env" "$root/config/kcode/conifer.env"
  "$root/config/kcode/cortecs.env" "$root/config/kcode/deepinfra.env" "$root/config/kcode/deepseek.env"
  "$root/config/kcode/fireworks.env" "$root/config/kcode/firmware.env" "$root/config/kcode/fpt.env"
  "$root/config/kcode/groq.env" "$root/config/kcode/huggingface.env" "$root/config/kcode/kimi.env"
  "$root/config/kcode/lmstudio.env" "$root/config/kcode/meta-muse.env" "$root/config/kcode/minimax.env"
  "$root/config/kcode/mistral.env" "$root/config/kcode/moonshotai.env" "$root/config/kcode/nebius.env"
  "$root/config/kcode/novita.env" "$root/config/kcode/nvidia-nim.env" "$root/config/kcode/ollama.env"
  "$root/config/kcode/openai-compatible.env" "$root/config/kcode/opencode-go.env" "$root/config/kcode/opencode.env"
  "$root/config/kcode/orcarouter.env" "$root/config/kcode/perplexity.env" "$root/config/kcode/scaleway.env"
  "$root/config/kcode/stackit.env" "$root/config/kcode/togetherai.env" "$root/config/kcode/xai.env"
  "$root/config/kcode/xiaomi-mimo.env" "$root/config/kcode/zai.env"
  "$root/external/.agents/skills" "$root/external/.claude/skills" "$root/external/.codex/skills"
  "$root/provider-backends/grok-build" "$root/provider-backends/grok" "$root/builds/source"
  "$root/provider-backends/grok-build/grok" "$root/provider-backends/grok/grok"
  "$root/browser" "$root/notifications" "$root/Applications"
  "$root/browser/.setup-complete"
  "$root/Applications/KcodeNotificationBroker.app/Contents/MacOS/KcodeNotificationBroker"
  "$root/ambient" "$root/safety" "$root/goals"
  "$root/ambient/transcripts" "$root/safety/queue.json" "$root/safety/history.json"
  "$root/goals/global" "$root/notifications/macos/inbox"
  "$root/session-memory/config"
  "$root/Applications/KcodeNotificationBroker.app"
  "$root/build.log" "$root/build-progress" "$root/pending-login"
  "$root/openai_oauth_usage.json" "$root/auth-refresh-state.json" "$root/auth-validation.json"
  "$root/composio_gmail.json" "$root/cloud_sessions.json" "$root/cloud_sessions_sync.json"
  "$root/session-metadata-v1.sqlite3" "$root/model-usage-v1.sqlite3" "$root/mcp-schema-cache.json"
  "$root/restart-snapshot.json" "$root/update_metadata.json" "$root/reload-context.json"
  "$root/reload-recovery" "$root/reload-traces" "$root/selfdev-build-requests"
  "$root/selfdev-build-locks" "$root/debug_control" "$root/testers.json"
  "$root/devices.json" "$root/servers.json" "$root/ssh_remotes.json"
  "$root/generated-images" "$root/models" "$root/side_panel" "$root/last_focused_client_session"
  "$root/keymap-snapshot.json" "$root/preferred_terminal.json" "$root/setup_hints.json"
  "$root/hotkey" "$root/launcher" "$root/pending-soft-interrupts"
  "$root/preferred-tools.md" "$root/prompt-overlay.md" "$root/telemetry_install_sent"
  "$root/install_conversion_id" "$root/telemetry_version_sent" "$root/telemetry_milestone"
  "$root/catchup_seen.json" "$root/cache/session-picker-list-v2.json" "$root/profiles/heap"
  "$root/schema-quirks.json" "$root/last_seen_changelog" "$root/telemetry_active_sessions"
  "$root/run/durable-state" "$root/run/durable-state/swarm" "$root/run/kcode-swarm-state"
)

for path in "$root" "$root/bin" "$root/run" "$root/builds" \
  "$root/builds/current-version" "$root/builds/shared-server-version" \
  "$root/builds/current" "$root/builds/shared-server" "$root/builds/versions" \
  "$root/cargo-target" "$root/xdg" "$root/xdg/config" "$root/xdg/data" \
  "$root/xdg/state" "$root/xdg/cache" "$root/.grok" "$root/.grok/auth.json" \
  "$root/external" "$root/external/.agents" "$root/external/.claude" \
  "$root/external/.claude/.credentials.json" "$root/external/.claude.json" \
  "$root/external/.codex" "$root/external/.config" "$root/external/.copilot" \
  "$root/external/.codex/auth.json" "$root/external/.codex/config.toml" \
  "$root/external/.config/github-copilot/hosts.json" \
  "$root/external/.config/github-copilot/apps.json" "$root/external/.config/cursor/auth.json" \
  "$root/external/.copilot/config.json" \
  "$root/external/.cursor" "$root/external/.hermes" "$root/external/.local" \
  "$root/external/.cursor/auth.json" "$root/external/.cursor/cli-config.json" \
  "$root/external/.cursor/projects" "$root/external/.gemini/oauth_creds.json" \
  "$root/external/.hermes/auth.json" "$root/external/.local/share/opencode/auth.json" \
  "$root/external/.local/share/opencode/storage/session" \
  "$root/external/.local/share/opencode/storage/message" \
  "$root/external/.local/share/opencode/storage/part" \
  "$root/external/.openclaw" "$root/external/.pi" "$root/external/AppData/Roaming" \
  "$root/external/.openclaw/agent/auth.json" \
  "$root/external/.openclaw/credentials/oauth.json" "$root/external/.openclaw/agents" \
  "$root/external/.pi/agent/auth.json" "$root/external/.pi/agent/sessions" \
  "$root/external/AppData/Roaming/Cursor/auth.json" "$root/external/AGENTS.md" \
  "$root/config" "$root/config/kcode" "$root/sessions" "$root/logs" \
  "$root/state" "$root/cache" "$root/memory" "$root/notes" "$root/profiles" \
  "$root/overnight" "$root/loops" "$root/active_pids" "$root/streaming_pids" \
  "$root/internal_pids" "$root/ssh-control" "$root/provider-backends" \
  "$root/source" "$root/source/kcode" "$root/auth.json" "$root/config.toml" \
  "$root/openai-auth.json" "$root/antigravity_oauth.json" \
  "$root/google_credentials.json" "$root/google_oauth.json" "$root/gemini_oauth.json" \
  "$root/telemetry_id" "$root/no_telemetry" "${private_storage_roots[@]}"; do
  [[ ! -L "$path" ]] || fail "refusing symlinked namespace component: $path"
done
check_dynamic_roots
case "$root" in
  "$production_home"|"$production_home"/*|"$production_launcher"|"$production_launcher"/*)
    fail "dev root overlaps a protected production path: $root" ;;
esac
case "$production_home" in
  "$root"|"$root"/*) fail "dev root contains production data: $root" ;;
esac
check_private_root

mkdir -p "$root/bin" "$root/run" "$root/builds/current" \
  "$root/builds/shared-server" "$root/builds/versions" "$root/cargo-target" \
  "$root/xdg/config" "$root/xdg/data" "$root/xdg/state" "$root/xdg/cache"
check_private_root
for path in "$root" "$root/bin" "$root/run" "$root/builds" \
  "$root/builds/current-version" "$root/builds/shared-server-version" \
  "$root/builds/current" "$root/builds/shared-server" \
  "$root/builds/versions" "$root/cargo-target" "$root/xdg" \
  "$root/xdg/config" "$root/xdg/data" "$root/xdg/state" "$root/xdg/cache" \
  "$root/.grok" "$root/.grok/auth.json" "$root/external" \
  "$root/external/.agents" "$root/external/.claude" "$root/external/.codex" \
  "$root/external/.claude/.credentials.json" "$root/external/.claude.json" \
  "$root/external/.config" "$root/external/.copilot" "$root/external/.cursor" \
  "$root/external/.codex/auth.json" "$root/external/.codex/config.toml" \
  "$root/external/.config/github-copilot/hosts.json" \
  "$root/external/.config/github-copilot/apps.json" "$root/external/.config/cursor/auth.json" \
  "$root/external/.copilot/config.json" \
  "$root/external/.hermes" "$root/external/.local" "$root/external/.openclaw" \
  "$root/external/.cursor/auth.json" "$root/external/.cursor/cli-config.json" \
  "$root/external/.cursor/projects" "$root/external/.gemini/oauth_creds.json" \
  "$root/external/.hermes/auth.json" \
  "$root/external/.local/share/opencode/auth.json" \
  "$root/external/.local/share/opencode/storage/session" \
  "$root/external/.local/share/opencode/storage/message" \
  "$root/external/.local/share/opencode/storage/part" \
  "$root/external/.openclaw/agent/auth.json" \
  "$root/external/.openclaw/credentials/oauth.json" "$root/external/.openclaw/agents" \
  "$root/external/.pi" "$root/external/AppData/Roaming" "$root/config" \
  "$root/external/.pi/agent/auth.json" "$root/external/.pi/agent/sessions" \
  "$root/external/AppData/Roaming/Cursor/auth.json" "$root/external/AGENTS.md" \
  "$root/config/kcode" "$root/sessions" "$root/logs" "$root/state" "$root/cache" \
  "$root/memory" "$root/notes" "$root/profiles" "$root/overnight" "$root/loops" \
  "$root/active_pids" "$root/streaming_pids" "$root/internal_pids" \
  "$root/ssh-control" "$root/provider-backends" "$root/source" "$root/source/kcode" \
  "$root/auth.json" "$root/config.toml" "$root/openai-auth.json" \
  "$root/antigravity_oauth.json" "$root/google_credentials.json" \
  "$root/google_oauth.json" "$root/gemini_oauth.json" "$root/telemetry_id" \
  "$root/no_telemetry" "${private_storage_roots[@]}"; do
  [[ ! -L "$path" ]] || fail "namespace path became a symlink: $path"
done
check_dynamic_roots

check_existing_channel() {
  local channel="$1"
  local binary="$channel/kcode"
  [[ -L "$binary" || ! -e "$binary" ]] || fail "unexpected non-symlink channel payload: $binary"
  [[ -L "$binary" ]] || return 0
  for _ in 1 2 3 4 5 6 7 8; do
    [[ -L "$binary" ]] || break
    local target
    target="$(readlink "$binary")"
    if [[ "$target" = /* ]]; then
      binary="$target"
    else
      binary="$(dirname "$binary")/$target"
    fi
  done
  [[ ! -L "$binary" ]] || fail "channel has too many nested symlinks: $channel"
  [[ -f "$binary" ]] || fail "channel target is missing: $binary"
  binary="$(CDPATH= cd -- "$(dirname "$binary")" && pwd -P)/$(basename "$binary")"
  case "$binary" in
    "$root"/*) ;;
    *) fail "existing channel escapes dev root: $binary" ;;
  esac
}
check_existing_channel "$root/builds/current"
check_existing_channel "$root/builds/shared-server"

version="$(git -C "$repo_root" rev-parse --short=12 HEAD)-dev-$(date +%s)"
version_dir="$root/builds/versions/$version"
mkdir -m 700 "$version_dir"

cd "$repo_root"
env -u KCODE_SOCKET -u KCODE_API_SOCKET -u KCODE_REPO_DIR \
  -u HERDR_ENV -u HERDR_PANE_ID -u HERDR_SOCKET_PATH -u HERDR_BIN_PATH \
  -u HERDR_WORKSPACE_ID -u HERDR_TAB_ID -u HERDR_PLUGIN_ROOT -u HERDR_SESSION \
  -u HERDR_AGENT -u HERDR_VERSION -u HERDR_VS_ACP \
  -u HERDR_KCODE_IDLE_DEBOUNCE_MS -u HERDR_KCODE_RETRY_GRACE_MS \
  KCODE_REMOTE_CARGO=0 \
  KCODE_DEV_NAMESPACE=1 \
  KCODE_HOME="$root" \
  KCODE_INSTALL_DIR="$root/bin" \
  KCODE_RUNTIME_DIR="$root/run" \
  CARGO_TARGET_DIR="$root/cargo-target" \
  XDG_CONFIG_HOME="$root/xdg/config" \
  XDG_DATA_HOME="$root/xdg/data" \
  XDG_STATE_HOME="$root/xdg/state" \
  XDG_CACHE_HOME="$root/xdg/cache" \
  scripts/dev_cargo.sh build --profile selfdev -p kcode --bin kcode

binary="$root/cargo-target/selfdev/kcode"
[[ -x "$binary" ]] || fail "build did not produce $binary"
install -m 700 "$binary" "$version_dir/kcode"

ln -sfn "$version_dir/kcode" "$root/builds/current/kcode"
ln -sfn "$version_dir/kcode" "$root/builds/shared-server/kcode"
printf '%s\n' "$version" > "$root/builds/current-version"
printf '%s\n' "$version" > "$root/builds/shared-server-version"
install -m 700 scripts/kcode-dev "$root/bin/kcode-dev"
printf 'Installed isolated launcher: %s\n' "$root/bin/kcode-dev"
printf 'Dev data/build/runtime root: %s\n' "$root"
printf 'Production kcode paths were not modified. Invoke the dev fork explicitly by its absolute path.\n'
