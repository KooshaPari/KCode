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
