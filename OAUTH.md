# Auth Notes: OAuth + API-key Providers

This document explains how authentication works in Kcode.

## Overview

Kcode can detect existing local credentials and can also run built-in OAuth and API-key login flows.

For auth files managed by other tools/CLIs, kcode asks before reading them. If you
approve a source, kcode remembers that approval for that external auth file path
for future sessions and still leaves the original file untouched (no move,
rewrite, or permission mutation). Symlinked external auth files are rejected.

Credentials are stored locally:
- Kcode Claude OAuth (if logged in via `kcode login --provider claude`): `~/.kcode/auth.json`
- Claude Code CLI: `~/.claude/.credentials.json` (Linux/Windows), or the **macOS login Keychain** item `Claude Code-credentials` (the default on macOS, where the JSON file usually does not exist), or the `CLAUDE_CODE_OAUTH_TOKEN` env var (set by `claude setup-token`)
- OpenCode (optional provider/OAuth import source): `~/.local/share/opencode/auth.json`
- pi (optional provider/OAuth import source): `~/.pi/agent/auth.json`
- Kcode OpenAI/Codex OAuth: `~/.kcode/openai-auth.json`
- Codex CLI auth source (read in place only after confirmation): `~/.codex/auth.json`
- Gemini native OAuth: `~/.kcode/gemini_oauth.json`
- Gemini CLI import fallback: `~/.gemini/oauth_creds.json`
- Copilot CLI plaintext fallback: `~/.copilot/config.json`
- Legacy Copilot JSON sources: `~/.config/github-copilot/hosts.json`, `~/.config/github-copilot/apps.json`

Relevant code:
- Claude provider: `src/provider/claude.rs`
- OpenAI login + refresh: `src/auth/oauth.rs`
- OpenAI credentials parsing: `src/auth/codex.rs`
- OpenAI requests: `src/provider/openai.rs`
- Azure OpenAI auth/config: `src/auth/azure.rs`
- Azure OpenAI transport: `src/provider/openrouter.rs`
- Gemini login + refresh: `src/auth/gemini.rs`
- Gemini Code Assist provider: `src/provider/gemini.rs`
- OpenAI-compatible provider metadata/login descriptors: `crates/kcode-provider-metadata/src/lib.rs`

## Claude (Claude Max)

### Login steps
1. Run `kcode login --provider claude` (recommended), or `kcode login` and choose Claude.
   - For headless / SSH use: `kcode login --provider claude --no-browser`
   - For scriptable remote flows: `kcode login --provider claude --print-auth-url`, then later complete with `--callback-url` or `--auth-code`
2. Alternative: run `claude` (or `claude setup-token`). kcode can detect Claude Code's credentials, ask before reading them, and remember that approval for future sessions. This works whether Claude Code stored them in `~/.claude/.credentials.json` (Linux/Windows), the macOS login Keychain (`Claude Code-credentials`), or the `CLAUDE_CODE_OAUTH_TOKEN` env var. On macOS, approving the Keychain source copies the credentials into `~/.kcode/auth.json` once so later sessions never re-prompt the Keychain.
3. Verify with `kcode --provider claude run "Say hello from kcode"`.

Credential discovery order is:
1. `~/.kcode/auth.json`
2. `~/.claude/.credentials.json`
3. Claude Code native credentials (macOS Keychain `Claude Code-credentials`, or `CLAUDE_CODE_OAUTH_TOKEN` env var) once approved
4. `~/.local/share/opencode/auth.json`
5. `~/.pi/agent/auth.json`

### Direct Anthropic API (default)
`--provider claude` uses the direct Anthropic Messages API by default.
kcode owns the full runtime path itself: auth, refresh, request shaping, tool
compatibility, and transport.

#### Anthropic API-key setup

The direct API-key route is separate from both a Claude subscription login and
OpenRouter. In the TUI, run `/login anthropic-api` and enter your key in the
login prompt, not in an agent message. The CLI equivalent is
`kcode login --provider anthropic-api`. You can also configure
`ANTHROPIC_API_KEY` in the process environment. Login persists the key in
`anthropic.env` inside kcode's configuration directory.

To explicitly select the direct API route, use
`/model claude-api:claude-opus-5-5`. The Claude subscription route uses
`claude-oauth:` instead. These route prefixes avoid accidentally selecting an
OpenRouter entry for the same model.

Configured direct Anthropic routes discover models from Anthropic's Models API.
New releases do not require a bundled-list update once the authenticated catalog
advertises them. API-key and OAuth availability can differ, so a model advertised
for one route does not establish access on the other. Without credentials, kcode
can only show its bundled fallback list, not verify account availability.

#### Claude OAuth direct API compatibility
Claude Code OAuth tokens can be used directly against the Messages API, but only
if the request matches the Claude Code "OAuth contract". kcode applies this
automatically for the default Claude runtime path.

Required behaviors (applied by the Anthropic provider):
- Use the Messages endpoint with `?beta=true`.
- Send `User-Agent: claude-cli/1.0.0`.
- Send `anthropic-beta: oauth-2025-04-20,claude-code-20250219`.
- Prepend the system blocks with the Claude Code identity line as the first
  block:
  - `You are Claude Code, Anthropic's official CLI for Claude.`

Tool name allow-list:
Claude OAuth requests reject certain tool names. kcode remaps a small set of
builtin tool names on the wire to the Claude-Code builtin names and maps them
back on responses so native tools continue to work. Every other tool is
forwarded under its own name, so the full custom toolset (websearch, webfetch,
browser, codesearch, memory, swarm, multiedit, open, ...) stays available on
OAuth. The remapped names are:
- `bash` → `Bash`
- `read` → `Read`
- `write` → `Write`
- `edit` → `Edit`
- `glob` → `Glob`
- `grep` → `Grep`
- `subagent` → `Agent`
- `schedule` → `ScheduleWakeup`
- `skill_manage` → `Skill`

Notes:
- If the OAuth token expires, refresh via the Claude OAuth refresh endpoint.
- Without the identity line and allow-listed tool names, the API will reject
  OAuth requests even if the token is otherwise valid.

### Removed Claude CLI transport
The old Claude Code CLI shell-out transport has been removed. Kcode always talks
to the Anthropic API directly. `--provider claude-subprocess` is accepted as an
alias for `--provider claude`, and `KCODE_USE_CLAUDE_CLI` is ignored.

## OpenAI / Codex OAuth

### Login steps
1. Run `kcode login --provider openai`.
   - For headless / SSH use: `kcode login --provider openai --no-browser`
   - For scriptable remote flows: `kcode login --provider openai --print-auth-url`, then later complete with `--callback-url`
2. Your browser opens to the OpenAI OAuth page unless you use `--no-browser`. The local callback listens on
   `http://localhost:1455/auth/callback` by default.
   If port `1455` is unavailable, kcode falls back to a manual paste flow where
   you can paste the full callback URL or query string.
3. After login, tokens are saved to `~/.kcode/openai-auth.json`.

Credential discovery order is:
1. `~/.kcode/openai-auth.json`
2. `~/.codex/auth.json`
3. trusted OpenCode/pi OAuth in `~/.local/share/opencode/auth.json` / `~/.pi/agent/auth.json`
4. `OPENAI_API_KEY`

If kcode finds existing credentials in `~/.codex/auth.json`, it asks before
reading them. When approved, it remembers that trust decision for future kcode
sessions and still does not move, delete, or rewrite the Codex file.

### Request details
Kcode uses the Responses API. If you have a ChatGPT subscription (refresh
token or id_token present), requests go to:
- `https://chatgpt.com/backend-api/codex/responses`
with headers:
- `originator: codex_cli_rs`
- `chatgpt-account-id: <from token>`

Otherwise it uses:
- `https://api.openai.com/v1/responses`

For **API-key** usage (no ChatGPT/Codex OAuth), the Responses API base URL is
overridable so you can target a local or proxied Responses-API endpoint. Set one
of (checked in this order) to an absolute `http(s)://` base that ends in the API
version, e.g. `http://127.0.0.1:8317/v1`:
- `KCODE_OPENAI_API_BASE`
- `OPENAI_BASE_URL`
- `OPENAI_API_BASE`

kcode appends `/responses` itself, derives the WebSocket and `/compact`
endpoints from the same base, and also points the `/models` catalog probe at it.
The override is ignored in ChatGPT/Codex OAuth mode (that backend is fixed), and
a malformed value is logged and ignored rather than breaking requests.

### Banked Codex usage resets

`/reset` (or `/reset usage limits openai`) checks the active OpenAI OAuth account's banked
resets and shows the selected reset, account, and expiry. It selects the
soonest-expiring available reset. Nothing is spent until you run
`/reset usage limits openai confirm`. Use `/reset usage limits openai cancel`
to dismiss the pending confirmation. API keys cannot redeem these resets.

When the active ChatGPT account is fully limited and has a banked reset, the
TUI notification shows the reset count and each known expiry in UTC, followed by
`/reset usage limits openai`. The hint wraps on narrow terminals. Expiries that
cannot be retrieved are explicitly marked unknown. Credit details are fetched
read-only alongside usage data and are also listed in the confirmation review.
This uses fresh, account-matched quota data and respects OpenAI's `allowed`
flag rather than suggesting a reset merely because a percentage rounds to 100%.
Hard quota failures trigger a read-only refresh. The hint never redeems a reset.

The implementation follows [Codex's backend client](https://github.com/openai/codex/blob/5c5308fc9a9ee789049d646ef11e5400384b9c6f/codex-rs/backend-client/src/client/rate_limit_resets.rs):

- Read: `GET https://chatgpt.com/backend-api/wham/rate-limit-reset-credits`
- Redeem: `POST` to that URL plus `/consume`, with JSON `credit_id` and a UUID
  `redeem_request_id`.
- Both requests use the ChatGPT OAuth bearer token and `chatgpt-account-id`
  when available. This is the Codex backend contract, not a public OpenAI API-key
  endpoint, and availability depends on the account.
- Confirmation pins the original account and credit. Retrying a failed or
  timed-out confirmation reuses the same redemption UUID, since the original
  request may already have succeeded. Check `/usage` before abandoning an
  uncertain redemption. Pending confirmations are session-local, not persisted.
- A reset spends one earned, single-use grant. It does not purchase credits,
  increase the subscription's limits, or bypass OpenAI's eligibility rules.
  See [OpenAI's banked reset explanation](https://help.openai.com/en/articles/20001498-how-banked-codex-resets-work).

### Troubleshooting
- Claude 401/auth errors: run `kcode login --provider claude`.
- 401/403: re-run `kcode login --provider openai`.
- Callback issues: make sure port 1455 is free and the browser can reach
  `http://localhost:1455/auth/callback`.

## Azure OpenAI

This was added after comparing Kcode to OpenCode/Crush. The meaningful auth gap
was not another browser OAuth flow, but support for **Azure OpenAI** using either:
- **Microsoft Entra ID** credentials (via Azure's `DefaultAzureCredential` chain), or
- **Azure OpenAI API keys**.

### Login/setup steps
1. Run `kcode login --provider azure`.
2. Enter your Azure OpenAI endpoint, for example:
   - `https://your-resource.openai.azure.com`
3. Enter your Azure deployment/model name.
4. Choose one auth mode:
   - **Entra ID** (recommended)
   - **API key**
5. kcode saves settings to `~/.config/kcode/azure-openai.env`.

### Stored configuration
The Azure env file may contain:
- `AZURE_OPENAI_ENDPOINT`
- `AZURE_OPENAI_MODEL`
- `AZURE_OPENAI_USE_ENTRA`
- `AZURE_OPENAI_API_KEY` (only when using key auth)

### Runtime behavior
- kcode normalizes the endpoint to the newer Azure OpenAI `/openai/v1` base.
- In **Entra ID** mode, kcode obtains bearer tokens using `azure_identity::DefaultAzureCredential` with scope:
  - `https://cognitiveservices.azure.com/.default`
- In **API key** mode, kcode sends the credential in the Azure-style `api-key` header.
- The Azure provider currently reuses Kcode's OpenAI-compatible transport layer under the hood.
- Model catalog fetching is disabled for Azure by default, so you should configure a deployment/model explicitly.

### Entra ID credential sources
`DefaultAzureCredential` can resolve credentials from sources like:
- `az login`
- managed identity
- Azure environment credentials

### Troubleshooting
- If Entra ID auth fails locally, try `az login` first.
- Make sure your identity has access to the Azure OpenAI resource.
- If requests fail with deployment/model errors, verify `AZURE_OPENAI_MODEL` matches your deployed model name.
- If you prefer static credentials, re-run `kcode login --provider azure` and choose API key mode.

## Gemini OAuth

### Login steps
1. Run `kcode login --provider gemini` or `/login gemini` inside the TUI.
   - For headless / SSH use: `kcode login --provider gemini --no-browser`
   - For scriptable remote flows: `kcode login --provider gemini --print-auth-url`, then later complete with `--auth-code`
2. kcode opens a browser to the Google OAuth flow used for Gemini Code Assist unless you use `--no-browser`.
3. If local callback binding is unavailable, kcode falls back to a manual paste flow using `https://codeassist.google.com/authcode`.
4. Tokens are saved to `~/.kcode/gemini_oauth.json`.

### Credential discovery order
1. Native kcode Gemini tokens: `~/.kcode/gemini_oauth.json`
2. Gemini CLI OAuth source (read only after approval): `~/.gemini/oauth_creds.json`
3. trusted OpenCode/pi OAuth in `~/.local/share/opencode/auth.json` / `~/.pi/agent/auth.json`

### Runtime notes
- kcode uses native Google OAuth and talks to the Google Code Assist backend directly.
- Expired tokens are refreshed automatically using the Google refresh token.
- Some school / Workspace accounts may require `GOOGLE_CLOUD_PROJECT` or `GOOGLE_CLOUD_PROJECT_ID` for Code Assist entitlement checks.

### Troubleshooting
- If browser launch fails, use `--no-browser` and the pasted callback/code flow.
- If entitlement or onboarding fails for a Workspace account, set `GOOGLE_CLOUD_PROJECT` and retry.
- If login succeeds but requests fail later, re-run `kcode login --provider gemini` to refresh the stored session.

### Auth verification
Use the built-in auth verifier to test the full local auth/runtime path after login:

```bash
# Run Gemini login now, then verify token refresh + provider smoke
kcode --provider gemini auth-test --login

# Verify existing Gemini auth without re-running login
kcode --provider gemini auth-test

# Check every currently configured supported auth provider
kcode auth-test --all-configured
```

For model providers, `auth-test` attempts:
1. credential discovery
2. refresh/auth probe
3. a real provider smoke prompt expecting `AUTH_TEST_OK`
4. a tool-enabled smoke prompt using the same tool-attached request path as normal chat

Use `--no-tool-smoke` if you only want the auth/simple-runtime checks.

For Gmail/Google it verifies credential discovery and token refresh, but skips model smoke because it is not a model provider.

## OpenAI-compatible API-key providers

Kcode also ships first-class provider presets for many OpenAI-compatible APIs.
These providers use the same built-in login flow pattern: `kcode login --provider <name>`.

For arbitrary OpenAI-compatible APIs, especially when an agent is doing setup, prefer the named profile command instead of hand-editing config:

```bash
printf '%s' "$MY_API_KEY" | kcode provider add my-api \
  --base-url https://llm.example.com/v1 \
  --model my-model-id \
  --api-key-stdin \
  --set-default \
  --json

kcode --provider-profile my-api auth-test --no-tool-smoke
```

This writes `[providers.my-api]` in `~/.kcode/config.toml` and stores the key in kcode's private app config dir, for example `~/.config/kcode/provider-my-api.env`. For localhost servers, use `--no-api-key`.

Notable presets include:

### Fireworks
- Login: `kcode login --provider fireworks`
- Stored env file: `~/.config/kcode/fireworks.env`
- API key env var: `FIREWORKS_API_KEY`
- Base URL: `https://api.fireworks.ai/inference/v1`
- Default model hint: `accounts/fireworks/routers/kimi-k2p5-turbo`
- Docs: <https://docs.fireworks.ai/tools-sdks/openai-compatibility>

### Novita AI
- Login: `kcode login --provider novita` or `/login novita` in the TUI
- Authentication: pay-as-you-go API key, not a subscription login or browser OAuth
- Stored env file: `~/.config/kcode/novita.env`
- API key env var: `NOVITA_API_KEY`
- Base URL: `https://api.novita.ai/openai`
- Default model hint: `zai-org/glm-5.3`
- Get a key: <https://novita.ai/settings/key-management>
- Docs: <https://novita.ai/docs/guides/llm-api>

### MiniMax
- Login: `kcode login --provider minimax`
- Stored env file: `~/.config/kcode/minimax.env`
- API key env var: `OPENAI_API_KEY`
- Base URL: `https://api.minimax.io/v1`
- Default model hint: `MiniMax-M2.7`
- Docs: <https://platform.minimax.io/docs/guides/text-generation>

These are first-class kcode provider presets, not just manual custom endpoint examples.
You can still use `openai-compatible` for arbitrary custom providers when there is not a built-in preset.

If kcode finds matching API keys in trusted OpenCode/pi auth files, it can reuse them for the corresponding provider preset without asking you to paste the key again.

## Experimental CLI Providers

Kcode also supports experimental CLI-backed providers, plus Antigravity with native OAuth login:
- `--provider cursor`
- `--provider copilot`
- `--provider antigravity`

Cursor uses kcode's native HTTPS transport. Copilot uses GitHub device-flow auth. Antigravity login/auth storage is handled natively by kcode.

### Cursor
- Login: `kcode login --provider cursor`
  - saves `CURSOR_API_KEY` to `~/.config/kcode/cursor.env`
- Runtime:
  - kcode uses native HTTPS requests
  - if a Cursor API key is configured, kcode exchanges/uses it directly
- Env vars:
  - `KCODE_CURSOR_MODEL` (default: `composer-1.5`)
  - `CURSOR_API_KEY` (optional; overrides saved key)

### GitHub Copilot
- Login: `kcode login --provider copilot`
  - Headless / SSH: `kcode login --provider copilot --no-browser`
  - Scriptable remote flow: `kcode login --provider copilot --print-auth-url`, then later `kcode login --provider copilot --complete`
  - kcode uses GitHub device code flow and can print the verification URL/QR without opening a local browser.
- Credential discovery order:
  1. `COPILOT_GITHUB_TOKEN`
  2. `GH_TOKEN`
  3. `GITHUB_TOKEN`
  4. trusted `~/.copilot/config.json`
  5. trusted legacy `~/.config/github-copilot/hosts.json`
  6. trusted legacy `~/.config/github-copilot/apps.json`
  7. trusted OpenCode/pi OAuth entries
  8. `gh auth token`
- Env vars:
  - `KCODE_COPILOT_CLI_PATH` (optional override for CLI path)
  - `KCODE_COPILOT_MODEL` (default: `claude-sonnet-4`)

### Antigravity
- Login: `kcode login --provider antigravity` (native Google OAuth flow; does **not** require Antigravity to be installed)
  - Headless / SSH: `kcode login --provider antigravity --no-browser`
  - Scriptable remote flow: `kcode login --provider antigravity --print-auth-url`, then later complete with `--callback-url`
- Tokens: `~/.kcode/antigravity_oauth.json`
- Credential discovery order:
  1. native kcode tokens at `~/.kcode/antigravity_oauth.json`
  2. trusted OpenCode/pi OAuth entries when present
- Runtime:
  - kcode authenticates directly and stores/refreshes Antigravity OAuth tokens itself
  - the provider transport still shells out to the Antigravity CLI for completions if you choose `--provider antigravity`
- Env vars:
  - `KCODE_ANTIGRAVITY_CLIENT_ID` (optional override for OAuth client id)
  - `KCODE_ANTIGRAVITY_CLIENT_SECRET` (optional override for OAuth client secret)
  - `KCODE_ANTIGRAVITY_VERSION` (optional override for Antigravity request fingerprint/version)
  - `KCODE_ANTIGRAVITY_CLI_PATH` (default: `antigravity`, runtime only)
  - `KCODE_ANTIGRAVITY_MODEL` (default: `default`)
  - `KCODE_ANTIGRAVITY_PROMPT_FLAG` (default: `-p`)
  - `KCODE_ANTIGRAVITY_MODEL_FLAG` (default: `--model`)

## Google / Gmail OAuth

### Login steps
1. Run `kcode login --provider google`.
   - For headless / SSH use: `kcode login --provider google --no-browser`
   - For scriptable remote flows after credentials are already configured: `kcode login --provider google --print-auth-url`
2. If Google credentials are not configured yet, kcode first walks you through saving your client ID/client secret or importing the JSON credentials file.
3. For scriptable Google flows, choose the Gmail scope with `--google-access-tier full|readonly` if you do not want the default full access tier.
4. Complete the printed flow later with `kcode login --provider google --callback-url '<full callback url or query>'`.

### Notes
- Google/Gmail scriptable auth requires saved OAuth client credentials first.
- The callback URL can come from a remote browser session that fails on the loopback redirect. Copy the final URL from the address bar and paste or pass it back to kcode.

## Scriptable auth state lifecycle

- kcode stores temporary scriptable login state in `~/.kcode/pending-login/*.json`
- pending state expires automatically
- stale pending entries are cleaned up when scriptable login flows start or resume
- Copilot `--print-auth-url` stores the GitHub device code session and `--complete` resumes polling later
