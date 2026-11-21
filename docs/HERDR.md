# Herdr integration contract

> **Runtime status: `herdr-core` (HC) is the supported terminal runtime.**
>
> The legacy Herdr bridge documented on this page, the
> `HERDR_ENV` / `HERDR_PANE_ID` environment contract, the
> `pane.report_agent*` socket verbs and the TOML screen manifests, is
> **deprecated**. It is retained only for compatibility with a host that has
> not yet migrated, and it is not the target of new work.
>
> See [herdr-core (HC)](#herdr-core-hc) below for what the supported runtime
> actually is, and for what KCode does and does not do under it.

## Legacy bridge (deprecated)

When a headed session launch is requested from a client with `HERDR_ENV=1` and `HERDR_PANE_ID`, KCode splits the calling pane to the right, focuses the new pane, and starts the resumed session there. `HERDR_BIN_PATH` is honored when present.

Everything below this section describes the legacy bridge.

This covers visible swarm spawns, resume-in-new-terminal, self-development launches, and restart restores because they all use the shared terminal launcher. A configured `[terminal].spawn_hook` still takes precedence.

`pane run` starts the session inside the new pane's interactive shell, so the launcher runs `<jcode command> && exit`: the pane closes when Jcode exits cleanly (quit, `swarm stop`), and stays open on a non-zero exit so the error remains readable.

Inside a Herdr pane, the TUI also reports `idle`/`working`, the session id, and its `jcode --resume` command through `"$HERDR_BIN_PATH" pane report-agent`, and releases the pane on quit (`crates/jcode-tui/src/tui/herdr.rs`). Herdr older than 0.9.2 rejects the `--` resume separator as a usage error (exit 2) and drops the whole report. On that specific error Jcode retries once without the resume command and keeps reporting state only for the rest of the process.

## Legacy compatibility

Jcode already:

- forwards `HERDR_ENV`, `HERDR_SOCKET_PATH`, `HERDR_PANE_ID`, `HERDR_TAB_ID`, `HERDR_WORKSPACE_ID`, `HERDR_BIN_PATH`, `HERDR_SESSION`, and `HERDR_AGENT` from the requesting client to server-side spawn and focus paths;
- recognizes Herdr as a masking terminal multiplexer for Mermaid graphics capability detection;
- exports stable lifecycle observer hooks for `session_start`, `session_end`, `turn_start`, and `turn_end`;
- exports `JCODE_HOOK_SESSION_ID`, `JCODE_HOOK_CWD`, event fields, and a JSON `JCODE_HOOK_PAYLOAD`;
- resumes a native session with `jcode --resume <session-id>`.

## Recommended first Herdr integration

The initial upstream Herdr integration should provide **native session identity plus screen-manifest state**, matching Herdr's Claude Code and Codex model. Jcode's current hooks reliably identify session and turn boundaries, but do not yet provide a complete authoritative `blocked` lifecycle. Reporting only `working` and `idle` as lifecycle authority would suppress Herdr's screen fallback and make approval/question detection worse.

On Jcode `session_start`, the Herdr hook should send one newline-delimited JSON request to `HERDR_SOCKET_PATH`:

```json
{
  "id": "herdr:jcode:<unique-request-id>",
  "method": "pane.report_agent_session",
  "params": {
    "pane_id": "<HERDR_PANE_ID>",
    "source": "herdr:jcode",
    "agent": "jcode",
    "seq": 1,
    "agent_session_id": "<JCODE_HOOK_SESSION_ID>",
    "session_start_source": "startup"
  }
}
```

The sequence must be monotonically increasing for the source. Map Jcode hook sources as follows where possible:

- `create` or `attach` to `startup`
- `resume` to `resume`

Herdr should restore the session with:

```text
jcode --resume <agent_session_id>
```

Jcode session IDs are opaque strings and fit Herdr's ID-based session reference model. No transcript path is needed.

## Required Herdr-side work (legacy Herdr upstream, deprecated)

A first-class integration cannot be shipped only as a remote detection manifest. Herdr currently hard-codes known agent kinds, official session sources, restore commands, and install targets. The upstream implementation needs:

1. Add `jcode` to `IntegrationTarget`, CLI parsing, labels, command discovery, recommendations, status, install, and uninstall handling.
2. Install a config-safe Jcode session hook adapter without overwriting an existing user hook. If Herdr cannot safely compose the single Jcode hook command, coordinate a small multi-hook or native-emitter addition in Jcode first.
3. Accept `("herdr:jcode", "jcode")` as an official session source.
4. Persist its ID session reference and map it to `jcode --resume <id>` during restore.
5. Add Jcode process detection and a bundled screen manifest for idle, working, and blocked UI states.
6. Keep screen-manifest detection authoritative until Jcode exposes complete blocked, approval-result, interrupt, and exit transitions.
7. Add integration versioning, replacement-source handling, schema/UI wiring, install/uninstall tests, restore-plan tests, detection fixtures, and documentation.

Relevant upstream files as of Herdr commit `eacea2daf0b72973173b728936b27478374f2cd2`:

- `src/integration/{mod.rs,registry.rs,targets.rs,actions.rs,version.rs}`
- `src/integration/assets/`
- `src/api/schema/integrations.rs`
- `src/agent_resume.rs`
- `src/detect/mod.rs`
- `src/terminal/state.rs`

## herdr-core (HC)

HC is a different product from legacy Herdr and speaks a different protocol. Its
binary is `hc` (alias `herdr-core`); the legacy multiplexer is `herdr`.

### Detection

`crates/jcode-herdr/src/env.rs` classifies the environment into
`RuntimeKind::{None, LegacyHerdr, Hc}`:

- `HC_TERMINAL_ID` set -> `Hc`. HC wins when both contracts are present, so a
  stale legacy variable cannot shadow it.
- Otherwise `HERDR_ENV=1` -> `LegacyHerdr`.

HC's own variables are `HC_TERMINAL_ID` (the terminal's address),
`HC_SOCKET` (the transport), and `HC_SESSION` (the core instance namespace).

### Transport

`crates/jcode-herdr/src/socket.rs` implements the HC transport: JSON-RPC 2.0,
one message per line, with a mandatory `hello` handshake that advertises the
protocol generation, core identity, and method list. This client speaks
protocol generation 1; `hello` rejects any other generation with
`error.data.kind = unsupported_generation`. Errors carry a stable
`error.data.kind`.

The HC endpoint is Unix-only. On Windows the transport returns an explicit
"unsupported on this platform" error so the workspace still builds.

### Method surface (measured, HC 0.1.21)

`hc schema` advertises 32 methods:

```
hello  rpc.discover  server.status  server.stop  server.restart
events.subscribe  templates.list  doc.get  doc.set
file.read  file.write  file.edit
machine.add  machine.list  machine.connect  machine.exec
machine.rename  machine.rm
terminal.run  terminal.list  terminal.get  terminal.kill  terminal.rm
terminal.keep  terminal.label  terminal.read  terminal.attach
terminal.send  terminal.wait  terminal.keys  terminal.interrupt
terminal.put
```

HC has **no** `pane.report_agent`, `pane.report_agent_session` or
`pane.release_agent`. It infers agent state from the terminal itself
(`terminal.wait` waits until the program is not working;
`terminal.interrupt` presses esc if the program reports status, else ctrl+c).

### What KCode does under HC

- KCode detects HC and exposes `HerdrEnv::kind()`, `hc_terminal_id()`,
  `hc_socket()` and `hc_session()`.
- KCode does **not** push lifecycle state into HC. The `HerdrReporter` state
  path (`pane.report_agent*`) is legacy-only: under HC the legacy pane id and
  socket path are absent, so the reporter is inert. This is deliberate, because
  HC has no verb that accepts pushed state.
- Pane splitting in the terminal launcher still keys off the legacy
  `HERDR_ENV` + `HERDR_PANE_ID` contract (`crates/jcode-terminal-launch`). It
  does not fire under HC, which injects `HC_TERMINAL_ID` instead. HC-driven
  launch is not implemented.

### `kcode herdr-status`

`kcode herdr status` starts with a `Runtime` row naming what it actually
probed: `herdr-core (HC)`, `legacy herdr`, or `none`. The kind comes from
`HerdrEnv::kind()` over the live environment, so the label and the printed
values always agree. Under HC it also prints `HC_TERMINAL_ID`, `HC_SOCKET` and
`HC_SESSION`; under legacy HERDR it prints the `HERDR_*` variables. Where a
legacy `HERDR_*` row is still shown under HC, treat it as legacy-bridge
diagnostics, not as an HC view.

Two rows are easy to misread. `HERDR pane valid` means only that identity and
an endpoint were detected, for either runtime. `Reporter active` reports
whether a reporter object exists, which is true under HC too. The row that
answers "will state actually be delivered" is `Push-capable`, which is
legacy-HERDR-and-valid, matching `HerdrReporter::is_push_capable`.

Official references (legacy Herdr; HC self-describes through `hc schema` and
`hc --skill` instead):

- <https://herdr.dev/docs/integrations/>
- <https://herdr.dev/docs/socket-api/>
- <https://herdr.dev/docs/agents/>
- <https://herdr.dev/docs/session-state/>
