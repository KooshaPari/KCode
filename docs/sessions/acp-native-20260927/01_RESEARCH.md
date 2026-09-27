# ACP native bridge checkpoint

Owner: Codex KCode worker. Base 42cfcfde4, branch codex/acp-native-remote.
Installed binary reports 591df7ad5 dirty; it is not proof of this source.
Other worktrees remain untouched. No live user sessions used.

Official contracts retrieved 2026-09-27:
- https://agentclientprotocol.com/protocol/v1/session-list
- https://agentclientprotocol.com/protocol/v1/session-setup
- https://agentclientprotocol.com/protocol/v1/elicitation
- https://agentclientprotocol.com/protocol/v1/tool-calls

Discovery implements standard session/list with absolute cwd filtering and bounded
100-row cursor pages. Session metadata uses existing journal-aware loader.
Next: passive attached event reader and correlated prompt cancellation; genuine
held elicitation/permission paths only. Current global elicitation channel is
TUI-process-local and lacks session ownership, so cannot safely expose it remotely
without adding daemon routing. Never synthesize approvals from observed tool events.

Compute placement: heavy Cargo checks assigned to desktop-kooshapari-desk only.
SSH probe failed DNS resolution; no heavy local fallback permitted. Rust tests
added but not executed until remote available. Local formatting is lightweight.

Implemented discovery plus per-session continuous event pump. The pump owns the
socket reader; active request handlers consume a bounded forwarding queue and
never compete with an idle reader. Closing a session/replacing attachment/stdio
EOF aborts only owned pump tasks. Preserved the original buffered reader across
load/create handshake, fixing lost already-buffered events.

Installed qualification: isolated JCODE_HOME, initialize and list only, no daemon
or prompt requested. Initialize succeeded and session/list returned -32601.
See installed-qualification.json. No claim of new-source runtime qualification.

## Held interactions (second source slice)

The daemon starts owner-only Unix IPC at its socket path plus `.interactions`.
An ACP attachment claims a session-specific 5-second renewable controller lease.
Pending requests survive connection loss until their own timeout, and a new
controller can recover them after lease expiry. The broker validates session,
request ID, exact option ID or typed form value, then consumes the oneshot once.
Cancellation and dropped tool futures remove the pending operation.

Pre-tool hook exit 3 explicitly opts that operation into `Ask`; stderr supplies
the displayed reason. Exit 0/2 and all other historical behavior remain unchanged.
Ask waits before calling the tool implementation; timeout/deny/cancel cannot run
that tool. No hook configuration or default approval policy is changed.

Existing elicitation calls route through the broker when their own session has a
form-capable ACP controller. Unsupported/secret forms fail without exposing data.
No ACP controller preserves the existing TUI elicitation path.

Source validation: rustfmt parses modules; diff whitespace check passes. Added
same-operation allow/deny/cancel/wrong-session/duplicate/schema/cleanup tests.
Cargo test execution and live cross-process qualification remain pending remote
compute. This is not a completed runtime or security qualification.

Interaction ownership is explicit: `session/new`, `session/load` or
`session/resume` must include `_meta: {"jcode.interactionController": true}` to
claim the broker lease. Plain loading/observation never claims it, even when the
client advertises form support. This is ACP extension metadata, not a new method.
One controller can own a session lease. Disconnection stops renewal; pending
requests remain recoverable after the 5-second lease expires, until request TTL.
Existing TUI questions stay with the TUI unless the explicit controller is active.

Interoperability refinement: brand-new ACP-created sessions own their interactions
by default (they have no existing TUI owner), so standard clients need no vendor
metadata. Existing-session load/resume remains passive by default. An explicit
false metadata value suppresses ownership even for new sessions. Generic clients
that cannot supply metadata can launch `env JCODE_ACP_CONTROL=1 jcode acp` to opt
into ownership on load/resume; this is process-scoped, not persisted global policy.
`agentCapabilities._meta["jcode.interactionController"]` advertises Unix support.
