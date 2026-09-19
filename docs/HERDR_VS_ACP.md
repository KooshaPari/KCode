# HERDR vs ACP

This document answers a recurring architectural question: **if jcode already
implements the Agent Communication Protocol (ACP), why does it also ship a
separate HERDR integration that emits screen-detection manifests and a Unix
socket reporter?** Short answer: ACP and HERDR operate at different layers
and answer different questions about a running agent. They coexist; they do
not replace each other.

## TL;DR

| Concern | ACP | HERDR |
| --- | --- | --- |
| Layer | Agent control plane (editor/IDE ↔ agent) | Terminal runtime pane observation plane |
| Topology | JSON-RPC client/server, 1 session at a time | Unix-socket daemon broadcasting pane lifecycle, N concurrent agents |
| Identity | Session IDs the IDE controls | Pane IDs the terminal multiplexer owns |
| Primary signal | Structured method calls (`session/update`, `fs/read_text_file`, …) | Terminal-output pattern matching (`agent-detection/*.toml`) for agents that will not — or cannot — speak any protocol |
| Wire format | NDJSON over stdio / socket, `jsonrpc="2.0"` | Newline-delimited JSON over a Unix-domain socket |
| Scope in this repo | `src/cli/acp.rs` (2,195 lines) | `crates/jcode-herdr/` (7 modules, 1,643 lines) |

The two systems do not share an integration point in this repo. See
[Touchpoints](#touchpoints) below for why and [Future Work](#future-work)
for an optional integration lane.

## What ACP is (in this repo)

The Agent Communication Protocol is a JSON-RPC 2.0 wire protocol that an
editor, IDE, or other front-end uses to *control* an agent session. In this
repo it is implemented in `src/cli/acp.rs`:

- `src/cli/acp.rs:14` declares the protocol version constant
  (`ACP_PROTOCOL_VERSION: u64 = 1`).
- `src/cli/acp.rs:52-80` defines the `JsonRpcMessage` parser — every
  inbound line must be an object with `"jsonrpc": "2.0"`; otherwise it
  returns a `JSONRPC_PARSE_ERROR` / `JSONRPC_INVALID_REQUEST` per the spec.
- `src/cli/acp.rs` (whole file, 2,195 lines) implements the rest of the
  surface — `initialize`, `session/new`, `session/prompt`,
  `session/update` notifications, tool calls, cancel, and the
  `Standard` / `Extended` / `Full` capability profiles declared at
  `src/cli/acp.rs:23-28`.

ACP's mental model is: *one editor client holds a session open with one
agent*. The agent advertises capabilities, the client invokes methods, the
agent streams `session/update` notifications back. Identity belongs to the
session; the session belongs to the editor. It is the right shape for a
"chat with the agent from inside my IDE" workflow.

## What HERDR is (in this repo)

HERDR is a terminal-runtime pane orchestrator. It watches terminal panes
and figures out what each pane is doing, so it can multiplex real-estate,
restore sessions across restarts, and coordinate multiple agents that share
a workspace. In this repo it is implemented in `crates/jcode-herdr/`:

- `crates/jcode-herdr/src/lib.rs` re-exports the public surface:
  `HerdrEnv` (`crates/jcode-herdr/src/env.rs`),
  `HerdrReporter` (`crates/jcode-herdr/src/reporter.rs`),
  `AgentState` (`crates/jcode-herdr/src/state.rs`),
  `PluginManifest` (`crates/jcode-herdr/src/plugin.rs`),
  and the screen-manifest helpers (`crates/jcode-herdr/src/manifest.rs`).
- `crates/jcode-herdr/src/env.rs` captures the HERDR env (`HERDR_ENV`,
  `HERDR_PANE_ID`, `HERDR_SOCKET_PATH`, `HERDR_BIN_PATH`,
  `HERDR_WORKSPACE_ID`, `HERDR_TAB_ID`) so the in-process reporter can
  find its pane and its socket.
- `crates/jcode-herdr/src/socket.rs` is the wire layer. The Unix path is
  at `crates/jcode-herdr/src/socket.rs:26` (`send_request`); the Windows
  stub is at `crates/jcode-herdr/src/socket.rs:59` and surfaces a clear
  `"HERDR is not supported on this platform (Unix-only)"` error.
- `crates/jcode-herdr/src/reporter.rs` implements debounce
  (`crates/jcode-herdr/src/reporter.rs:125-148`), error-hold, and a
  monotonic per-source sequence counter at
  `crates/jcode-herdr/src/reporter.rs:52`. These exist because terminal
  state is noisy — we do not want a 50ms token to flip the pane from
  `working` to `idle` and back.
- `crates/jcode-herdr/src/manifest.rs` generates the screen-detection
  rules written to `~/.config/herdr/agent-detection/*.toml`
  (`crates/jcode-herdr/src/manifest.rs:34` for `to_toml`,
  `crates/jcode-herdr/src/manifest.rs:70` for the jcode manifest,
  `crates/jcode-herdr/src/manifest.rs:115` for the ForgeCode manifest).
- `crates/jcode-herdr/src/plugin.rs` is the in-process `herdr-plugin.toml`
  emitter added alongside this doc. It produces the local plugin
  manifest under `~/.config/herdr/plugins/local/` so users do not have
  to clone the upstream community plugins.

HERDR's mental model is: *a daemon watches N terminal panes, each of
which is some kind of agent, and tries to give the user a useful
picture of who is doing what*. Identity belongs to the pane; the pane
belongs to the terminal. It is the right shape for "show me which of my
six terminal panes are stuck" and "restore my workspace after a reboot".

## Why the two operate at different layers

ACP and HERDR diverge on three axes:

1. **Direction of control.** ACP is editor → agent (the editor drives the
   agent). HERDR is daemon → pane (the daemon observes the pane). An
   ACP client would feel out of place attaching itself to a pane it does
   not own; a HERDR daemon would feel out of place trying to call
   `session/prompt` on something it has no authority over.

2. **Identity and concurrency.** ACP assumes one session at a time and
   uses opaque session IDs handed out by the agent. HERDR assumes N
   concurrent agents in N panes and uses pane IDs owned by the terminal
   multiplexer. A single ACP session has no notion of "the pane next
   to me" — there are no panes in ACP's model. A HERDR pane has no
   notion of "the editor holding my session open" — there are no
   editors in HERDR's model.

3. **Signal source.** ACP's primary signal is structured JSON-RPC: the
   agent emits `session/update` with rich deltas. HERDR's primary
   signal is the bottom of a terminal pane, matched against
   `agent-detection/*.toml` rules (`crates/jcode-herdr/src/manifest.rs`).
   That signal is useful *even when the agent is not running ACP* — a
   shell-only workflow with no IDE, or an agent that does not speak
   ACP, still benefits from HERDR detecting that something is stuck.

## Why HERDR can't just consume ACP

It is tempting to imagine HERDR as an ACP client: have every pane's agent
speak ACP, have HERDR subscribe to its `session/update` notifications,
derive state from the structured events. That would let us delete the
screen-manifest layer and the in-process `HerdrReporter`. It does not
work, for three concrete reasons rooted in this repo:

1. **Wire topology mismatch.** ACP is JSON-RPC client/server — exactly
   one client holds a session open with exactly one agent
   (`src/cli/acp.rs:60-80` parses one inbound line per request; the
   server expects synchronous or streaming replies). HERDR is a
   *broadcasting* daemon that watches N panes owned by N different
   agents, none of which have handed it a session. Reusing ACP would
   require either inventing an ACP server-per-pane fan-out (which is
   not what ACP is for) or having every pane's agent keep a permanent
   ACP connection back to the daemon (which is not what the agents are
   designed to do, and which would couple the agent's lifetime to the
   daemon's lifetime in exactly the wrong direction).

2. **ACP does not carry the concepts HERDR needs.** There is no
   pane-identity field, no notion of "session-state semantics" (idle
   vs. blocked vs. approval-pending vs. token-rate-limited), and no
   concept of screen-detection rules. `session/update` carries the
   agent's view of its own state — useful, but orthogonal to HERDR's
   observation plane. Even if HERDR subscribed to ACP updates, it
   would still need its own screen-manifest layer to detect state in
   agents that do not speak ACP at all.

3. **HERDR's primary signal is terminal-output pattern matching.**
   `crates/jcode-herdr/src/manifest.rs:70` produces a TOML document
   keyed on text patterns like `"●"`, `"Thinking..."`, and the prompt
   character `❯`. That signal is the whole point of HERDR's value
   proposition: *detect what an agent is doing even when the agent
   cannot or will not announce it*. ACP-only integration would
   silently exclude every agent that doesn't speak ACP — exactly the
   case HERDR exists to handle.

## Touchpoints

None required. They coexist:

- An ACP-capable IDE running jcode in a HERDR pane gets both:
  structured ACP traffic to the editor, and screen-detection-driven
  state from HERDR. The two observe different surfaces and do not
  conflict.
- A non-ACP user (terminal-only, or a different agent) still gets
  HERDR's value via the screen manifests in
  `~/.config/herdr/agent-detection/*.toml`. No ACP code is loaded on
  that path — `src/cli/acp.rs` is only compiled into the binary when
  the user actually invokes the ACP subcommand.

## Cross-platform guards

HERDR is Unix-only because it depends on Unix-domain sockets and
`cfg!(unix)` build paths:

- `crates/jcode-herdr/src/socket.rs:7-12` documents the Unix-only
  constraint.
- `crates/jcode-herdr/src/socket.rs:17-25` is the Unix build path.
- `crates/jcode-herdr/src/socket.rs:58-63` is the Windows stub,
  emitting `"HERDR is not supported on this platform (Unix-only)"`.
- `crates/jcode-herdr/src/plugin.rs` exports
  `windows_install_hint()` (referenced from
  `src/cli/herdr.rs::run_herdr_install`) which prints a single line
  directing Windows users to WSL2 + a Herdr WSL build.

ACP, by contrast, runs over stdio and arbitrary transports — there is
no platform gate in `src/cli/acp.rs`. That asymmetry is the cleanest
illustration of the layers split: ACP only needs a JSON stream;
HERDR needs Unix IPC primitives.

## Future Work

A speculative future integration could let HERDR pipe ACP
`session/update` notifications as a *fallback lane* for ACP-capable
agents — i.e., when an agent *also* exposes ACP, HERDR could prefer
its structured state over screen-pattern matching. Concretely:

1. The jcode binary's `--herdr` startup could spawn a one-shot ACP
   `initialize` / `session/load` against the daemon's pane-id
   representative, then forward `session/update` events into
   `HerdrReporter::set_state_with_message` (currently at
   `crates/jcode-herdr/src/reporter.rs:130`).
2. Screen manifests would remain the authoritative signal for
   non-ACP agents and as a tie-breaker when the ACP stream is silent.

This is **speculative**. Do not enable it without:

- A protocol version negotiation that fails closed if the daemon
  doesn't support the lane.
- A panic-safe downgrade path so a misbehaving ACP stream cannot
  permanently mark a pane `working`.
- End-to-end tests that prove the screen-manifest lane still
  produces correct state when ACP is unavailable.

None of that work is required today. The two systems coexist, and
keeping them separate is what lets each one stay small.

## See also

- `docs/HERDR.md` — the HERDR integration contract (what jcode does
  for HERDR, what HERDR-side work is required, restore via
  `jcode --resume <session-id>`).
- `docs/HERDR_INTEGRATION.md` — operational guide for
  `jcode herdr status`, `jcode herdr install`, env vars, the
  reporter protocol, and the screen-manifest format.
