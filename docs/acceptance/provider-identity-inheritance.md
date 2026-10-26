# Acceptance evidence — provider identity inheritance (KCode fork)

- Branch under test: `fix/provider-identity-inheritance-master`
- Local repo: `/Users/kooshapari/.jcode/scratch/wt-port-master`
- Tested binary: `/Users/kooshapari/.jcode/scratch/wt-port-master/target/debug/kcode`
  - `kcode --version` -> `kcode v0.0.0-dev (35f2e3456)`
  - The commit hash is stamped by `crates/jcode-build-meta/build.rs` at first
    build and is deliberately NOT re-derived on a later commit, so the build
    was run with `KCODE_BUILD_GIT_HASH=$(git rev-parse --short=9 HEAD)`.
- Harness: `/private/tmp/kcode-acceptance/` (a fork-local copy of the
  upstream harness in `~/.jcode/scratch/acceptance-local/`).
  - The fork renames `JCODE_*` to `KCODE_*`, so `run_case.sh` sets
    `KCODE_HOME`, `KCODE_SHARED_RUNTIME_DIR` and `KCODE_SOCKET`.
  - `list_case_procs.py` and `kill_case.py` were rewritten for macOS: the
    upstream versions read `/proc/<pid>/environ`, which does not exist here.
  - The sandbox root is `/private/tmp/kcode-acceptance`, NOT under a jcode
    directory: `storage::require_kcode_path` rejects a `KCODE_HOME` that is
    or resolves into one, and `~/.jcode/scratch/...` is exactly that.
  - `kill_case.py` skips the harness's own `tee`, which carries the case dir
    in its argv and would otherwise truncate the result being recorded.
- Every case ran inside a fresh sandbox with its own `HOME`, `KCODE_HOME`,
  `KCODE_SOCKET` and `KCODE_SHARED_RUNTIME_DIR`. The real user's home and any
  real kcode daemon were never referenced.

## Result matrix

| Case | sidecar provider | client `--provider` | decision | peer session traffic | peer survived | client rc |
|---|---|---|---|---|---|---|
| 1 case1-match | openai-compatible | openai-compatible | ATTACH | YES (subscribe + get_history) | yes | 124 (TUI, timed out) |
| 2 case2-mismatch | cursor | openai-compatible | ISOLATE | NO (liveness connect only) | yes | 1 (no API key) |
| 3 case3-nosidecar | (absent) | openai-compatible | ISOLATE | NO (liveness connect only) | yes | 1 (no API key) |
| 4 case4-auto | cursor | (none) | ATTACH | YES (subscribe + get_history) | yes | 124 (TUI, timed out) |

This matches the upstream `jcode` matrix recorded in
`~/.jcode/scratch/acceptance-local/EVIDENCE.md`, row for row.

## Verbatim observations

### CASE 1 — case1-match (ATTACH)
stderr notice:
```
Server already running with --provider openai-compatible; reusing it.
Connecting to server...
```
peer log:
```
[   7.265s] CONN-OPEN peer_id=77816 from=
[   7.267s] REQ type=subscribe id=6481849209592535576 raw={"type":"subscribe",...}
[   7.267s] REQ type=get_history id=6481849209592535577 raw={"type":"get_history",...}
[   7.267s] REQ type=get_model_catalog id=6481849209592535578 raw={"type":"get_model_catalog",...}
```
```
PROBE-RC: 0
PEER-DEAD: yes
CLIENT-RC: 124 (duration 45s)
```

### CASE 2 — case2-mismatch (ISOLATE)
stderr notice:
```
Running server cannot be verified to serve --provider openai-compatible; starting a private server on /private/tmp/kcode-acceptance/cases/case2-mismatch/run/jcode-client-12543.sock.
Starting server...
Error: Server exited before signalling ready (exit status: 1):
Error: OPENAI_COMPAT_API_KEY not found in environment or /private/tmp/kcode-acceptance/cases/case2-mismatch/home/.kcode/config/kcode/openai-compatible.env
```
The private socket is a *sibling* of the shared one, and the socket-scoped
spawn lock is visible next to it:
```
--- socket files in sandbox run dir ---
-rw-r--r--@  1 kooshapari  staff     0 jcode-client-12543.sock.spawning
srw-------@  1 kooshapari  staff     0 jcode.sock
-rw-r--r--@  1 kooshapari  staff    44 jcode.sock.identity
```
peer log: a bare liveness connect, and no session traffic:
```
[   7.535s] CONN-OPEN peer_id=12525 from=
[   7.535s] CONN-EOF from=
[  13.268s] CONN-OPEN peer_id=12525 from=
[  13.269s] REQ type=ping id=999999 raw={"type": "ping", "id": 999999}
[  13.269s] CONN-EOF from=
```
```
PROBE-RC: 0
PEER-DEAD: yes
CLIENT-RC: 1 (duration 12s)
```
The isolated server exits with "no API key" because the sandbox has no
credentials. That is the same end state the upstream run recorded, and it is
the expected behaviour: the point of the case is that the client did NOT
attach to the shared daemon.

### CASE 3 — case3-nosidecar (ISOLATE)
Same as case 2: the missing sidecar cannot prove agreement, so the client
isolated and the shared daemon survived.
```
Running server cannot be verified to serve --provider openai-compatible; starting a private server on .../jcode-client-12542.sock.
PROBE-RC: 0
PEER-DEAD: yes
CLIENT-RC: 1 (duration 15s)
```

### CASE 4 — case4-auto (ATTACH)
No notice at all: an `auto` request claims no identity, so there is nothing
to report. The client attached and the shared daemon received the session
traffic.
```
[   7.822s] REQ type=subscribe id=6319769755088233450 raw={"type":"subscribe",...}
[   7.822s] REQ type=get_history id=6319769755088233451 raw={"type":"get_history",...}
```
```
PROBE-RC: 0
PEER-DEAD: yes
CLIENT-RC: 124 (duration 46s)
```

## What this proves

- A client that names a provider **attaches** to a daemon that publishes
  that provider.
- A client that names a provider **isolates** onto a private socket when the
  daemon publishes a different provider, or publishes nothing at all.
- An `auto` client **attaches**, because it claims no identity.
- In every case the shared daemon **survived** the decision.

## What this does NOT prove

- That the isolated server can serve a request: the sandbox has no API
  credentials, so the isolated server exits before it can.
- The `--model` axis: the model is advisory and never triggers isolation, so
  it has no separate acceptance case.
- Stale private-socket cleanup: each case starts from a fresh sandbox.
- That a real Herdr pane (a different `TMPDIR` and no `KCODE_HOME` override)
  behaves the same; the harness pins every environment variable explicitly.
