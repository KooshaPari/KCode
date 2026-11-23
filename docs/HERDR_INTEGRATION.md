# HERDR Integration

> **Runtime status: `herdr-core` (HC) is the supported terminal runtime.**
> Everything this page describes, the `HERDR_ENV`/`HERDR_PANE_ID` contract, the
> `pane.report_agent*` socket verbs, and the TOML screen manifests, belongs to
> the **deprecated legacy Herdr bridge** and is retained only for compatibility.
> See [herdr-core (HC)](#herdr-core-hc) at the end of this page for the
> supported runtime and for what KCode actually does under it.

Legacy HERDR terminal runtime integration for jcode and ForgeCode.

## Overview

HERDR is a terminal runtime that monitors agent state through socket-based
lifecycle reporting. jcode reports its state (working, idle, blocked) to HERDR
via Unix socket JSON-RPC, enabling HERDR to manage screen real estate, session
restore, and multi-agent coordination.

## Architecture

```
┌──────────┐    Unix Socket     ┌──────────┐
│  jcode   │ ────────────────── │  HERDR   │
│ (reporter)│  JSON-RPC msgs    │ (daemon) │
└──────────┘                    └──────────┘
     │                               │
     │  screen manifests             │  detection rules
     │  ~/.config/herdr/             │  (TOML)
     │  agent-detection/*.toml       │
```

## CLI Commands

### `jcode --herdr`

Force HERDR integration for testing outside a real HERDR pane:

```bash
jcode --herdr
```

### `jcode --herdr-kind <KIND>`

Specify agent type for HERDR reporting:

```bash
jcode --herdr --herdr-kind jcode      # default
jcode --herdr --herdr-kind forge       # ForgeCode
jcode --herdr --herdr-kind forgecode   # ForgeCode (alias)
```

### `kcode herdr-status`

Show current HERDR environment and reporter state:

```bash
$ kcode herdr-status
HERDR Pane Status
─────────────────
  HERDR_ENV          1
  HERDR_PANE_ID      pane-abc-123
  HERDR_SOCKET_PATH  /tmp/herdr.sock
  HERDR_BIN_PATH     (not set)
  HERDR_WORKSPACE_ID (not set)
  HERDR_TAB_ID       (not set)

  HERDR pane valid   yes
  Reporter active    yes
```

### `kcode herdr-install`

Write screen detection manifests:

> **Legacy-only.** These manifests are read by the legacy HERDR bridge, which is
> deprecated. herdr-core (HC) is the supported runtime (`hc`, alias
> `herdr-core`); it reads status from the program's own OSC 7501 reports and
> never reads `~/.config/herdr/agent-detection`, so under HC these files have no
> effect.

```bash
$ kcode herdr-install
Installed ~/.config/herdr/agent-detection/jcode.toml
Installed ~/.config/herdr/agent-detection/forgecode.toml

Screen manifests written to ~/.config/herdr/agent-detection/
HERDR will use these rules to detect jcode and ForgeCode agent state from terminal output.
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `HERDR_ENV` | (unset) | Set to `1` to force HERDR mode |
| `HERDR_PANE_ID` | (unset) | Pane identifier |
| `HERDR_SOCKET_PATH` | (unset) | Path to HERDR Unix socket |
| `HERDR_BIN_PATH` | (unset) | Path to HERDR binary |
| `HERDR_WORKSPACE_ID` | (unset) | Workspace identifier |
| `HERDR_TAB_ID` | (unset) | Tab identifier |
| `HERDR_KCODE_IDLE_DEBOUNCE_MS` | `250` | Debounce delay before idle transition (read by `crates/jcode-herdr/src/reporter.rs`) |
| `HERDR_KCODE_RETRY_GRACE_MS` | `2500` | Grace period for error-hold retry (read by `crates/jcode-herdr/src/reporter.rs`) |
| `HC_TERMINAL_ID` | (unset) | herdr-core terminal address; presence selects the HC runtime |
| `HC_SOCKET` | (unset) | herdr-core socket path (the HC transport endpoint) |
| `HC_SESSION` | (unset) | herdr-core session namespace |

`HERDR_JCODE_IDLE_DEBOUNCE_MS` and `HERDR_JCODE_RETRY_GRACE_MS` are **not read
by the code**. They are an earlier naming drift; only the `HERDR_KCODE_*`
names take effect.

## State Transitions

```
          ┌──────────┐
          │  IDLE    │ ◄──────────────────┐
          └────┬─────┘                    │
               │ user prompt              │ debounce (250ms)
               ▼                          │
          ┌──────────┐                    │
          │ WORKING  │ ───────────────────┘
          └────┬─────┘
               │ permission prompt
               ▼
          ┌──────────┐
          │ BLOCKED  │ ──── user responds ──► WORKING
          └──────────┘
```

- **Working → Idle**: Debounced (250ms). Rapid working→idle→working cycles cancel the transition.
- **Idle → Working**: Immediate on background task wake.
- **Working → Blocked**: On permission/confirmation prompt display.
- **Blocked → Working**: On user response.
- **Error-hold**: Errors within the retry grace period (2500ms) cancel pending idle transitions.

## Screen Manifests

Screen manifests are TOML files that tell HERDR how to detect agent state
from terminal output. They define text patterns that HERDR matches against
the pane's bottom buffer.

Example jcode manifest (`~/.config/herdr/agent-detection/jcode.toml`):

```toml
agent = "jcode"
version = "1"

[[rules]]
state = "working"
patterns = ["●"]
description = "jcode spinner active (working)"

[[rules]]
state = "working"
patterns = ["Thinking..."]
description = "jcode thinking state"

[[rules]]
state = "blocked"
patterns = ["❯", "Yes", "No"]
description = "jcode permission prompt"

[[rules]]
state = "idle"
patterns = ["❯"]
description = "jcode input prompt (idle)"
```

## Reporter Protocol (legacy only)

The reporter sends JSON-RPC requests over the legacy Herdr Unix socket. It is
inert under HC: HC has no verb that accepts pushed agent state, so there is
nothing to send.

State report:

```json
{
  "id": "jcode:jcode:working:<seq>",
  "method": "pane.report_agent",
  "params": {
    "agent": "jcode",
    "pane_id": "pane-abc-123",
    "source": "jcode:jcode",
    "state": "working",
    "seq": 42
  }
}
```

Session identity (used for restore):

```json
{
  "id": "jcode:jcode:session:<seq>",
  "method": "pane.report_agent_session",
  "params": {
    "pane_id": "pane-abc-123",
    "source": "jcode:jcode",
    "agent": "jcode",
    "agent_session_id": "sess_xyz",
    "seq": 43
  }
}
```

Release on exit:

```json
{
  "id": "jcode:jcode:release:<seq>",
  "method": "pane.release_agent",
  "params": {
    "agent": "jcode",
    "pane_id": "pane-abc-123",
    "source": "jcode:jcode",
    "seq": 44
  }
}
```

## Crate Structure

```
crates/jcode-herdr/
  src/
    lib.rs         # Public API (11 tests)
    env.rs         # HERDR environment capture
    state.rs       # AgentState enum + transitions
    socket.rs      # Unix socket JSON-RPC communication
    reporter.rs    # Lifecycle reporter with debounce/error-hold
    manifest.rs    # Screen manifest TOML generation
```

## Integration Points

1. **Startup** (`src/cli/startup.rs`): Reporter initialized before TUI launch
2. **TUI** (`crates/jcode-tui/src/tui/app/local.rs`): Working→idle transitions wired
3. **CLI** (`src/cli/args.rs`): `--herdr`, `--herdr-kind` flags
4. **Subcommands** (`src/cli/herdr.rs`): `herdr-status`, `herdr-install`

## herdr-core (HC)

The supported runtime. HC is a separate product from legacy Herdr: its binary is
`hc` (alias `herdr-core`), it speaks JSON-RPC 2.0 with a mandatory `hello`
handshake, and it has no `pane.report_agent*` verbs at all.

What exists in KCode today:

- **Detection** (`crates/jcode-herdr/src/env.rs`): `RuntimeKind::{None,
  LegacyHerdr, Hc}`. `HC_TERMINAL_ID` present means `Hc`, and HC wins when both
  contracts are present.
- **Transport** (`crates/jcode-herdr/src/socket.rs`): HC `hello` handshake
  (protocol generation 1) plus `terminal.get` / `terminal.send` request types.
  Unix-only; on Windows the transport returns an explicit unsupported-platform
  error so the workspace builds.
- **State reporting via OSC 7501.** HC has no report verb; a program tells it
  what it is doing by printing an OSC 7501 record on its own terminal output.
  `crates/jcode-herdr/src/osc.rs` encodes those records and `HerdrReporter`
  prints them under HC (`is_osc_capable()`), so state reaches the core without
  any socket call. Do not add a `pane.report_*` call for HC: the verb does not
  exist.
- **No HC launch integration.** Pane splitting in
  `crates/jcode-terminal-launch` still keys off the legacy `HERDR_ENV` +
  `HERDR_PANE_ID` contract and does not fire under HC.

HC is self-describing: run `hc schema` for the full API (32 methods at
`hc 0.1.21`) and `hc --skill` for the agent manual. Treat those as authoritative
over any list here. See `docs/HERDR.md#herdr-core-hc` for the method surface.
