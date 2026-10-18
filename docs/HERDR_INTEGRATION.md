# HERDR Integration

HERDR terminal runtime integration for jcode and ForgeCode.

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

### `jcode herdr status`

Show current HERDR environment and reporter state:

```bash
$ jcode herdr status
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

### `jcode herdr install`

Write screen detection manifests:

```bash
$ jcode herdr install
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
| `HERDR_JCODE_IDLE_DEBOUNCE_MS` | `250` | Debounce delay before idle transition |
| `HERDR_JCODE_RETRY_GRACE_MS` | `2500` | Grace period for error-hold retry |

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

## Reporter Protocol

The reporter sends JSON-RPC requests over the Unix socket:

```json
{
  "method": "agent.state",
  "params": {
    "agent": "jcode",
    "pane_id": "pane-abc-123",
    "state": "working",
    "seq": 42,
    "session_id": "sess_xyz"
  }
}
```

Release on exit:

```json
{
  "method": "agent.release",
  "params": {
    "agent": "jcode",
    "pane_id": "pane-abc-123",
    "seq": 43
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
4. **Subcommands** (`src/cli/herdr.rs`): `herdr status`, `herdr install`
