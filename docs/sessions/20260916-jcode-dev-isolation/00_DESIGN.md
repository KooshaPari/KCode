# kcode-dev Binary Isolation Design

## Problem

When running `cargo build` in the kcode workspace, the server process reloads and interrupts other active chat sessions sharing the same socket. This makes it impossible to develop kcode while simultaneously using it for coding tasks.

## Root Cause

Both the production `kcode` binary and a development build share the same socket paths:
- API socket: `runtime_dir()/kcode-api.sock`
- Legacy socket: `runtime_dir()/kcode.sock`
- Runtime dir: `$KCODE_RUNTIME_DIR` > `$XDG_RUNTIME_DIR` > `$TMPDIR` > fallback

When `cargo build` produces a new binary and the server restarts, it reclaims the same socket, disrupting all connected clients.

## Solution: Environment-Based Isolation

The socket path resolution in `crates/kcode-harness-api/src/sockets.rs` already supports env var overrides:
- `KCODE_RUNTIME_DIR` — overrides runtime directory (socket parent)
- `KCODE_API_SOCKET` — overrides API socket path directly
- `KCODE_SOCKET` — overrides legacy socket path directly

**kcode-dev** uses these overrides to get a fully isolated runtime.

## Design

### 1. Separate Config Directory

| Path | Production | Development |
|------|-----------|-------------|
| Config | `~/.kcode/` | `~/.kcode-dev/` |
| Runtime | `$TMPDIR/kcode-$USER/` | `$TMPDIR/kcode-dev-$USER/` |
| API socket | `runtime/kcode-api.sock` | `runtime/kcode-api.sock` (isolated by runtime dir) |
| Legacy socket | `runtime/kcode.sock` | `runtime/kcode.sock` (isolated by runtime dir) |

### 2. Wrapper Script

A shell wrapper `kcode-dev` sets the environment and launches the dev binary:

```bash
#!/bin/sh
# kcode-dev — isolated development wrapper
# Uses separate config and runtime dirs so cargo build doesn't interrupt production sessions.

export KCODE_HOME="${KCODE_HOME:-$HOME/.kcode-dev}"
export KCODE_RUNTIME_DIR="${KCODE_RUNTIME_DIR:-$(mktemp -d)/kcode-dev-$(id -u)}"
export KCODE_CONFIG_DIR="${KCODE_CONFIG_DIR:-$HOME/.kcode-dev}"

exec "$(dirname "$0")/kcode-bin" "$@"
```

### 3. Binary Name

The workspace already has `autobins = false` and explicit `[[bin]]` entries. Add a `kcode-dev` binary entry:

```toml
[[bin]]
name = "kcode-dev"
path = "src/main.rs"
```

This reuses the same entrypoint. The env vars control isolation, not the binary code.

### 4. What Gets Isolated

| Resource | Isolated? | Mechanism |
|----------|----------|-----------|
| Socket files | YES | `KCODE_RUNTIME_DIR` |
| Config files | YES | `KCODE_HOME` / `KCODE_CONFIG_DIR` |
| Session history | YES | Config dir contains sessions |
| Log files | YES | `KCODE_LOG_DIR` if set |
| HERDR state | YES | HERDR reads from `KCODE_HOME` |
| Memory files | YES | Config dir contains memories |
| Binary on disk | NO | Same `target/debug/kcode-dev` |
| Provider keys | NO | Same env vars (shared) |
| Upstream repo | NO | Same source tree |

### 5. Safety Properties

1. **No socket collision**: Different `KCODE_RUNTIME_DIR` = different socket files
2. **No config collision**: Different `KCODE_HOME` = different config/state
3. **Same provider keys**: Both instances share API keys (intentional)
4. **Independent sessions**: Each instance manages its own session history
5. **Clean shutdown**: Killing kcode-dev does not affect kcode production

### 6. Implementation Steps

1. Add `[[bin]] name = "kcode-dev"` to `Cargo.toml` (trivial, reuses main.rs)
2. Create `scripts/kcode-dev` wrapper script
3. Document in README
4. Optional: Add `kcode-dev` to `PATH` via install script

### 7. Verification

```bash
# Terminal 1: Start production kcode
kcode

# Terminal 2: Start dev kcode (isolated)
kcode-dev

# Terminal 3: Build kcode (does NOT interrupt Terminal 1 or 2)
cargo build --bin kcode-dev

# Verify sockets are separate
ls /tmp/kcode-*/kcode-api.sock       # production
ls /tmp/kcode-dev-*/kcode-api.sock   # development
```
