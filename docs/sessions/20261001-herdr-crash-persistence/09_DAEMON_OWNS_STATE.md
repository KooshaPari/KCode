# Herdr session persistence: the daemon owns the file

## Proven (2026-10-04)

1. `session.json` is **not** a plain config file. The herdr daemon holds
   session state in memory and rewrites the **entire** document on a timer
   (~60s observed).
2. Therefore **any external edit is reverted within about a minute.**
   Measured: applied a reconciled file (11675 B, resume=13, probes=0) at
   17:42:20; by 17:43:17 the daemon had restored 11385 B / resume=10 /
   probes=3. The 3 reverted rows were exactly the ones just fixed
   (slots 5 PhenoShared, 6 Agile, 8 ForgeCode).
3. A benign `herdr pane list` did **not** trigger a rewrite -> the save is a
   timer, not per-request.
4. **Consequence:** writing the file is necessary but NOT sufficient. The
   write must be followed by a **daemon restart** so the daemon re-reads
   the reconciled document as its startup state. After that restart the
   reconciled state persisted (probes=0 stable over many minutes).

## Correct ordering

    discover (server MUST be live)      # CLI is the source of truth
      -> stop daemon                     # verify with ps, never the CLI
      -> write file atomically           # os.replace
      -> let the daemon restart / start it
      -> verify + stability watch

Discovery cannot run with the daemon stopped: herdr's CLI auto-starts the
daemon, which would clobber the write. That is exactly why
`reconcile_sessions.py` is two-phase (`--emit-plan` / `--plan-file`).

## Auto-restart caveat

`herdr server stop` does not leave the server stopped. An **interactive
`herdr` client** (observed pid 41240/94469 on ttys000, inside Ghostty)
restarts it. So the stop must be followed immediately by the write; do not
assume a stopped server persists. Detection uses `ps`/`lsof` only.

## Wrong turn worth recording

I briefly repointed `~/.kcode/builds/current/kcode` after concluding the
build was broken from `spctl: invalid signature`. **That was a wrong
diagnosis**: the same directory backs `stable/kcode`, which is the binary
of the *running, healthy* server (pid 4603, up 3 days), and the binary
passes `--version` (rc=0). The signature complaint is expected for an
adhoc-signed local build and is not the cause of anything.

The real SIGKILL is narrower: it happens when the binary tries to start a
**server in a non-TTY context** (`kcode --resume <sid>` with stdin from
/dev/null -> "Server exited before signalling ready (signal: 9)"). That is
pre-existing and unrelated to session.json reconciliation. The pointer was
restored to its original target.

**Update 2026-10-08:** the underlying cause of that SIGKILL was diagnosed
after this write-up. The real culprit is `self_heal_macos_code_signature()`
in `src/main.rs` running unconditionally on every launch and stripping
the linker-placed `CS_LINKER_SIGNED` attribute via
`codesign --force --deep --sign -`. The durable fix landed in
`27bd2299c` (opt-in via `KCODE_MACOS_STARTUP_REPAIR=1`) and the post-install
signature check in `bfbcf898f` (see `10_SIGKILL_NON_TTY.md` for the full
root-cause write-up, build/install verification, and lesson notes).

Lesson: verify a diagnosis against a *running* instance before changing
shared launcher state.

## Pane ID mapping (session.json ↔ herdr CLI)

**Discovered 2026-10-09** during round 8.1 followup. The
session.json persistence file and the herdr CLI use two
DIFFERENT pane ID schemes:

| Source | Format | Example | Notes |
|---|---|---|---|
| `session.json` (key) | numeric | `"19"` | Internal sequence number; order in which panes were created |
| herdr CLI `pane list` | `workspace:pane` | `w7:p1R` | Stable identifier; the same pane across calls |
| herdr `pane_id` field | `workspace:tab:pane` | `w7:p1R` | Same as above; herdr CLI normalizes to 2-segment form |

The session.json pane ID 19 is NOT the same as herdr's `w7:p19`.
The mapping is established by matching fields like
`agent_resume.argv`, `agent_resume.source`, `agent_session.value`,
`terminal_id`, or `terminal_title` between the two sources.

Example from 2026-10-09 health check #2:

- session.json pane `19`:
  - `agent_resume: {source: "herdr:kcode", agent: "kcode", argv: ["kcode", "--resume", "session_evergreen_1791516626775_251c14ff3297d065"]}`
- herdr CLI `w7:p1R`:
  - `agent: kcode, agent_status: idle, terminal_id: term_65d602e1519b113, terminal_title: "🌲 I dont see my prior jcode sessions, theres a ton…"`
- herdr `pane process-info --pane w7:p1R`:
  - `pid: 22365, argv: ["/Users/kooshapari/.local/bin/kcode", "--resume", "session_evergreen_1791516626775_251c14ff3297d065"]`

The session.json `argv` matches herdr's `pane process-info`
`argv` exactly. The mapping is confirmed.

**To map session.json pane IDs to herdr CLI pane IDs:**

1. Get the session.json pane's `agent_resume.argv` (or
   `agent_session.value`)
2. Get `pane process-info` for each herdr CLI pane
3. Match the `argv` (or `agent_session.value`) — the
   kcode/jcode binaries include the session_id in argv;
   the codex binary uses a separate `agent_session.value`

**Why this matters for diagnostics:**

When investigating herdr behavior, the operator often needs
to correlate the persistence-layer state (session.json) with
the runtime state (`pane list`, `pane process-info`). The
two use different IDs, so a mapping step is required. This
section is the canonical reference for that mapping.

**Panes observed 2026-10-09 05:34 PDT (health check #2):**

| session.json | herdr CLI | agent | note |
|---|---|---|---|
| 2 | w7:p1A | jcode | Port |
| 3 | w7:p17 | jcode | Fabric |
| 5 | w7:p1E | jcode | KCode (done) |
| 6 | w7:p1J | jcode | ShareCLI |
| 7 | w7:p1D | jcode | Omni |
| 8 | w7:p1M | jcode | Agile |
| 9 | w7:p1Q | jcode | Mux |
| 13 | w8:pR | jcode | PhenoShared |
| 14 | w8:pP | jcode | Byte |
| 16 | w8:pS | jcode | HelioLite |
| **19** | **w7:p1R** | **kcode** | **kcode test pane (PID 22365)** |
| (no entry) | w7:p19 | jcode | Khostty |
| (no entry) | w7:p1R | (root) | (matches pane 19) |
| (canonical) | w8:pB | codex | Jcode? |
| (canonical) | w8:pT | codex | Clarify (FOCUSED) |
| (canonical) | w8:pE | codex | MoshClone |
| (canonical) | w8:pH | codex | (unknown) |
| (no entry) | w8:pN | unknown | Cockpit |
| (no entry) | w8:pW | kcode | KCode (?) |

The four `codex` panes have `agent_session.value` (the
canonical path); the 12 `jcode`/`kcode` panes have
`agent_resume` (the rescue path); the 2 unknown panes
(w7:p19 Khostty, w8:pN Cockpit) have neither — these are
likely auto-created Herdr TUI panes that haven't reported
agent yet.
