# Durable child-session findings

Bug: headless swarm child sessions die mid-task. Symptoms: "status crashed", "idle worker not restored after server restart", sessions vanishing with no report. Eleven deaths in one swarm and three more investigators lost. The model route is healthy, so the suspicion is the jcode server restarting and taking children with it.

Repo: `E:\koosh\CodeProjects\jcode`. Source reading and evidence collection only.

## FINDINGS

### Q1: When the "idle worker not restored after server restart" note is emitted

It is emitted during **startup reconciliation**, not at shutdown. `recover_member_status()` at `swarm_persistence.rs:404-453` transforms statuses loaded from disk:
- `Running` becomes `Crashed` with note "recovered after reload while running" (lines 409-413).
- `Ready` becomes `Stopped` with `IDLE_WORKER_NOT_RESTORED_NOTE` (lines 421-432):
  > `// No client or headless process survives a server restart. A connected TUI`
  > `// will explicitly mark its member ready again during subscribe; until that`
  > `// happens, restoring a persisted `ready` member as live creates a ghost`
  > `// that can never enter terminal-member GC. This previously resurrected`
  > `// hundreds of detached historical clients as ready on every reload.`
> `if status == SwarmLifecycleStatus::Ready { return (SwarmLifecycleStatus::Stopped, append_recovery_detail(detail, if is_headless { IDLE_WORKER_NOT_RESTORED_NOTE } else { "client not attached after server restart" })); }`
- Other non-terminal headless statuses become `Crashed` with "headless session did not survive reload" (lines 435-449).

`recover_member_status` is called from `from_persisted_member()` at line 471, inside `load_runtime_state()` (line 525 onward), i.e. when the new server process loads the persisted swarm snapshot. A second occurrence is a UI-facing summary at `crates/jcode-app-core/src/server/swarm.rs:1563`: `"{count} idle worker{} not restored after restart"`. `crates/jcode-app-core/src/server/swarm_persistence_tests.rs:179` uses the exact string as a test expectation.

### Q2: Which session states survive a server restart (and is idle excluded deliberately?)

There are two distinct layers: **session transcript/status files** and **swarm member status**.

Session files: `Session` has a durable `status: SessionStatus` field (`crates/jcode-base/src/session.rs:146-148`), and `SessionStatus` has exactly `Active, Closed, Crashed{message}, Reloaded, Compacted, RateLimited, Error{message}` (`crates/jcode-session-types/src/lib.rs:156-170`). There is no `idle` session status. Journal entries persist `status: SessionStatus` (`session/journal.rs:55`) and `apply_journal_meta()` restores it verbatim (`session.rs:770`: `self.status = meta.status;`). All session statuses therefore survive a restart in the file; what changes is whether a live agent is rebuilt.

Swarm members: lifecycle statuses are strings (`ready`, `running`, `stopped`, `crashed`, `completed`, `done`, `failed`). `swarm_persistence.rs:404-453` deliberately rewrites persisted statuses during startup (see Q1): `ready -> stopped` for **both** TUI and headless members, `running -> crashed`, and any other non-terminal headless status -> `crashed`. This is a deliberate policy, not a missing match arm. The comment at lines 416-420 explicitly justifies the `ready -> stopped` rewrite.

Startup restoration is gated by `headless_member_should_restore()` at `crates/jcode-app-core/src/server.rs:414-420`:
> `is_headless && !matches!(status, "ready" | "completed" | "done" | "failed" | "stopped")`

`recover_headless_sessions_on_startup()` (`server_headless_recovery.rs:6-22`) does include a narrow exception for stopped idle headless workers:
> `member.is_headless && (headless_member_should_restore(&member.status, true) || member_has_idle_restart_note(member) || reload_recovery::has_pending_for_session(&member.session_id))`

`member_has_idle_restart_note()` (lines 409-413) merely checks the detail string for `IDLE_WORKER_NOT_RESTORED_NOTE`. So after a restart, a truly idle headless worker is loaded as an agent only because the note triggers this exception. It is then marked `ready` and its note cleared (`server_headless_recovery.rs:166-210`, especially lines 177-200). But the note is cleared only if the worker is in the loaded swarm snapshot: `from_persisted_member()` drops terminal members whose `terminal_since_unix_ms` is older than the retention window (`swarm_persistence.rs:469-487`), and `Stopped` is terminal (`swarm.rs` classification). Once pruned, there is no note and no member, so that worker is gone permanently.

For children that were mid-turn, `restored_session_was_interrupted()` (`client_session.rs:67-115`) returns true for `SessionStatus::Crashed`, for `Active` with a pending user/tool message, or for reload-interruption markers, so those sessions do auto-resume on startup. Plain `Active` idle children are not resumed (`should_resume` false at `server_headless_recovery.rs:144-151`), which is deliberate: line 141 reads `// A genuine idle worker was alive and ready before the restart.`

`worker_revive.rs:1-9` documents the earlier failure mode: startup recovery only restored headless workers mid-turn; failed or idle workers stayed as durable member records with no live agent, and every later wake/resume/retry/DM found "no live agent". `live_or_revived_agent()` (lines 43-142) now rebuilds such an agent lazily when a swarm member is headless and its session file loads, and replaces the closed placeholder event channel (lines 126-138). It refuses revival when the session registry is not the canonical one (lines 60-65) or when a migration lease is outstanding (lines 69-74).

`crates/jcode-app-core/src/server/swarm_persistence.rs:99-102` defines the exact string:

> `/// Recovery note on an idle headless worker that a server restart stopped.`
> `/// No headless process survives a restart (issue #1651).`
> `pub(super) const IDLE_WORKER_NOT_RESTORED_NOTE: &str =`
> `    "idle worker not restored after server restart";`

### Q3: Child supervision (heartbeat, socket close, supervisor, reconnect)

**No supervision loop exists for headless children.** `create_headless_session()` (`crates/jcode-app-core/src/server/headless.rs:37-339`) creates the agent, registers it in `sessions`, registers interrupt queues/background signals, then makes a swarm member with `status: "ready"`, `is_headless: true` (lines 260-292). The only spawned task is an event drain at lines 250-255:
> `let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<ServerEvent>();`
> `tokio::spawn(async move { while event_rx.recv().await.is_some() { // Drain events to keep channel alive } });`

That is a sink, not a supervisor. There is no socket, hence no socket-close handler for the child; no heartbeat ping/pong; no watchdog task that checks the child agent's liveness; no supervisor that respawns it on failure. `worker_revive.rs:126-138` similarly installs only a fresh draining channel. Any liveness judgement is therefore derived from persisted status/member state at restart, not from active monitoring. The only lifecycle writes are status updates by turn paths (`update_member_status`) and crash detection in `client_disconnect_cleanup.rs:43-73` which applies **only to TUI socket disconnects**: `DisconnectDisposition::Crashed` when the socket dies mid-turn and no reload marker is active. Headless workers have no such connection and never traverse that code.

`worker_revive.rs:1-9` is explicit that revival is lazy and on-demand rather than supervised: "This module rebuilds the agent on demand from the persisted session so the coordinator can simply resume it."

### Q4: Runtime evidence from logs and state

Direct evidence from `C:\Users\koosh\.jcode\logs\jcode-2026-11-18.log` shows at least one "child died" incident was a provider failure, not a child-supervisor crash. The persisted swarm file `C:\Users\koosh\.jcode\state\swarm\session_session_bird_1795125104129_992b2921fa6f56a9.json` ends with the parent `session_bird...` at `status: "failed"` (`OpenAI (openai-otter) is unavailable: OAuth usage exhausted`) and a headless child `session_turtle...` still at `status: "ready"`.

The log correlates all of it:
- 2026-11-19 13:53:13.631: `Server-initiated turn failed for live session session_bird...: OpenAI (openai-otter) is unavailable: OAuth usage exhausted`, then `new_status=failed old_status=running` and `TURN_STOP abnormal=true needs_investigation=true`.
- 2026-11-19 18:18:36.729 onward: a child `session_turtle...` is created, set to `running`, then at 18:18:37.515 `AGENT_PROVIDER_STREAM_LIFECYCLE ... phase=stream_event_error error="usage_limit_reached: The usage limit has been reached"`, followed at 18:18:37.543 by `new_status=failed old_status=running` and another `TURN_STOP abnormal=true needs_investigation=true`.
- At 18:18:36.773 there is a distinct transport warning: `lightweight control writer failed while sending CommSpawnResponse ...: The pipe is being closed. (os error 232)`. That affects the parent control channel response, not the child agent stream, which continued to open and stream an error.
- At 18:48:55.234, roughly 30 minutes later, the child session is closed and `Reaped idle spawned swarm worker session_turtle... (idle > 1800s)` is logged. That is a later cleanup of an already failed worker, not the cause of failure.

Separately, `C:\Users\koosh\.jcode\logs\jcode-2026-11-19.log` contains many `jcode starting (version=v1.0.1 (10114429c), exe=C:\Users\koosh\AppData\Local\jcode\bin\jcode.exe)` records across the day. Reload trace `reload_1795048061503_14788532390254921633.jsonl` shows `graceful_shutdown_complete` followed by `exec_start`, and its `candidate_snapshot` contains a headless member at `status: "stopped"`. This confirms server reloads occurred and matches the Q1/Q2 status rewrite, but the inspected logs do not show those reloads killing a live provider turn. The same logs do show reload recovery `outcome=failed detail=OpenAI-compatible chat request failed` for several headless sessions at 17:18:08-10, another model/provider failure rather than a supervision failure.

No `panic` was found in the inspected headless session logs. One `status="Crashed { message: Some(\"Panic: panicked at ... failed printing to stderr...\"` was observed in the large session-persistence log line for an unrelated long-lived session (`session_pig...`), and it was a persisted transcript status, not a headless swarm child event.

### Q5: "status crashed" and panic paths

`SessionStatus::Crashed` (`crates/jcode-session-types/src/lib.rs:157-170`, displayed as `"crashed"` at lines 173-182) is a **session transcript status**, distinct from the **swarm member string status** `"crashed"`. The user-visible phrase can arise from either layer, and the code does not tie it to an actual panic in most cases:

1. Caught panics: `client_processing.rs:357-388` wraps turn processing with `AssertUnwindSafe(...).catch_unwind()`. On `Err(panic_payload)` it sets `TurnStopReason::Crash` (`line 374`), logs `Processing task PANICKED...`, and returns an error. `TurnStopReason::Crash` is explicitly "a caught runtime panic, not an inference from a lost connection" (`crates/jcode-session-types/src/lib.rs:1143-1151`). The completion handler then reports the member as `"failed"` with a truncated error detail (`client_processing.rs:82-95`), not `"crashed"`. So a caught panic in a turn does not itself write the swarm member status `"crashed"`.
2. Client-socket crash inference: `client_disconnect_cleanup.rs:50-73,283-297,350-369` marks the TUI session `Crashed` and the member `"crashed"` when a connected client disconnects while processing and no reload marker is active. This is an inference from lost connection, explicitly not a panic. Headless workers have no client socket and do not traverse this path.
3. Startup/scan crash detection: `Session::detect_crash()` (`crates/jcode-base/src/session.rs:1186-1213`) marks an `Active` session `Crashed` when its PID is gone with no shutdown signal or when an old session has no PID and is more than 120 seconds stale. `capture_current_snapshot()` calls it at `crates/jcode-app-core/src/restart_snapshot.rs:140-155`, and `detect_crashed_sessions()` calls it on every JSON session at `crates/jcode-base/src/session/crash.rs:204-223`. These are heuristics, not evidence of a Rust panic.
4. PID-file crash recovery (`crates/jcode-base/src/session/crash.rs:318-401`) uses a streaming marker: a missing marker means idle and is closed normally; a marker means mid-turn and is marked `Crashed`. The comment at lines 352-362 states the marker is cleared on normal return, `?`, interrupt, and panic, so only a killed process leaves it behind. This is again a killed/abrupt-exit inference.
5. Swarm reconciliation: on startup, a persisted headless member that was `running` is rewritten to `crashed` with detail "headless session did not survive reload" (`swarm_persistence.rs:435-449`). This can make a worker appear crashed after a server reload even if no panic occurred.

Therefore, "status crashed" is not proof of a panic. In the inspected runtime evidence (Q4), the observed children ended with provider `usage_limit_reached` / `OAuth usage exhausted` errors and member status `failed`, while reload-era headless statuses were rewritten by policy. Nothing in the inspected logs correlates a headless child's status change with a caught panic.

## MECHANISM

The durable facts support a layered explanation, not a single "server restart kills children" mechanism:

1. **Model/provider failure is the observed immediate cause of the sampled child deaths.** The logs show provider/session failures followed by `new_status=failed` and `TURN_STOP abnormal=true`. A rate limit or exhausted OAuth allowance also makes newly spawned children fail immediately. This is the only direct runtime evidence of children dying mid-task.
2. **Reloads can create the "vanished with no report" symptom by policy.** A server reload is an exec/new process. Headless children do not survive as live processes. On startup, `recover_member_status()` rewrites a persisted `running` member to `crashed` and a persisted `ready` member to `stopped` with the idle note (`swarm_persistence.rs:404-453`). Startup recovery rebuilds agents only for non-terminal statuses, the idle note, or a pending reload directive (`server_headless_recovery.rs:6-22`). Failed/stopped members can then be pruned by terminal GC (`server.rs:220-285`), and idle spawned workers are reaped after the inactivity window (`server.rs:288-385`). A failed provider turn therefore becomes a status record, then a missing member, with no completion report.
3. **"Crashed" is overloaded and often inferred.** TUI disconnect cleanup, PID/age heuristics, and reload reconciliation can all produce a crashed-looking status without a Rust panic (Q5). A caught turn panic is labeled `TurnStopReason::Crash` but the member completion status becomes `failed` (`client_processing.rs:357-388,82-95`).
4. **There is no headless child supervisor to preserve live work.** `create_headless_session()` only drains events; there is no child heartbeat, socket-close handler, watchdog, or respawn (`headless.rs:250-255`, Q3). Revival is lazy and on-demand (`worker_revive.rs:1-9,43-142`). This does not cause the sampled provider failures, but it explains why the system cannot distinguish an interrupted child from an idle or failed one and cannot keep it alive across reloads.

So the most likely end-to-end path is: a child turn hits a provider/limit error (or a reload interrupts it), the server writes a status rather than preserving a durable continuation report, startup reconciliation classifies the member as crashed/stopped, and terminal GC or idle reaping removes it. The user sees "status crashed", "idle worker not restored", or a vanished child. The healthy model route does not rule out account/provider limits and does not protect against server reloads.

## FIX SKETCH

Shortest robust fix, in priority order:

1. Before a planned reload/shutdown, record a durable reload-recovery intent for every headless member in `running` state, including the pending directive and last known task. Ensure `recover_headless_sessions_on_startup()` treats that intent as authoritative over the status rewrite (`server_headless_recovery.rs:6-22,134-151`). This is the existing mechanism; the gap is making it cover all live headless children, not just sessions that happened to have a directive.
2. Distinguish provider failure from crash. Keep `failed` for provider/rate-limit errors and reserve `crashed` for caught panics or verified abnormal exits. Write a completion report on every terminal path, including failure paths, so a dead child never vanishes without a report.
3. Give headless children a minimal liveness contract: a periodic heartbeat or watchdog that marks a missing child `stopped`/`crashed` deterministically, plus a lazy revival trigger that is safe under terminal GC. Keep the idle note until revival actually succeeds, rather than clearing it on a mere load.
4. Prevent cleanup races: delay terminal GC and idle reaping for recently interrupted headless members with pending recovery intent (`server.rs:220-385`). Re-validate that no live agent or pending recovery record exists before pruning.
5. Add integration tests for: running child plus reload; idle child plus reload; provider-limit failure; caught panic; and a child with pending directive surviving terminal GC. Assert final member status, report presence, and whether a live agent is rebuilt.

No implementation was performed. This document is source-reading and log-evidence analysis only.
