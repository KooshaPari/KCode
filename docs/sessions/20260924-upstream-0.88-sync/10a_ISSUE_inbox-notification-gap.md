## Title
Inbox notification gap: pre-tool-hook DEFERs of GitHub writes are never surfaced to the operator

## Symptom

Agent-initiated GitHub write actions that the pre-tool hook DEFERs to the operator inbox are
never surfaced to the operator. `notified_via: []` on every queued item, including items that
sat unanswered for 21+ hours.

**Evidence (observed 2026-10-09, `~/Library/Application Support/phinbox/inbox/`):**

```
hook-d018032a4550408d31185e813936945a  state=pending  urgency=warning  notified_via: []
hook-bf5be20865b4470f9bf40baa88c2af50  state=pending  urgency=warning  notified_via: []
hook-141bb721d7b79f1faedefc3a7268eda1  state=pending  urgency=warning  notified_via: []
hook-62ac11e658dadb89143f795b3281d5b7  state=pending  urgency=warning  notified_via: []
hook-ac50bec5dd2a3114839c97ceb1238c48  state=pending  urgency=warning  notified_via: []
hook-cf6be8c203a15a4339cca4d6c42b2102  state=pending  urgency=warning  notified_via: []
```

Each item had `expires_at_ms` ≥ 21h in the future at observation time. The deferral worked
(no command executed). The notification did not.

## Root cause (traced, source-level)

The deferral path is `jcode-tool-safety` → `request_approval()` → `elicitate ask --from-json`
→ `elicitate` shim → `phinbox ask --from-file <tmp> --async`.

**`elicitate` (shim at `~/bin/elicitate`, lines 155-190) discards the notification contract.**

1. `request_approval()` in `jcode-tool-safety:24-72` builds a `PromptSpec` with
   `"urgency":"warning"` and passes it to `elicitate ask --from-json`.
2. `elicitate`'s Python block (`/Users/kooshapari/bin/elicitate:95-190`) pops `details`,
   rebuilds a **stripped** spec with only `title`/`question`/`field`/`buttons`/`urgency`/
   `timeout_secs`, and writes `/tmp/elicitate-sanitized-<rid>.json`.
3. It then calls `phinbox ask --from-file <tmp> --async` — **async only, no notification
   channel is ever engaged.** `elicitate` never passes any notify flag, and the
   `--async` subcommand queues silently.
4. `elicitate`'s usage header (lines 19-25) is explicit that blocking popups require
   `--block`, and that the default is the defer flow — but no equivalent flag exists to
   route a deferred item to an OS notification.

**phinbox is capable of notifying.** `strings ~/.local/bin/phinbox` shows renderer spawn
paths `spawn osascript:`, `spawn notify-send:`, and `spawn powershell:` (macOS/Linux/Windows
native notification primitives). The capability exists; the caller never asks for it.

**Corroborating evidence:** every answered item in
`~/Library/Application Support/phinbox/answered/` that was actually handled carries
`notified_via: []` too, but the ones the operator *did* act on include notes like
`"Operator approved via native OS popup in current Jcode session"` — i.e. the operator was
sitting in the session and saw it incidentally. Nothing surfaced an unsurfaced item.

## Why it matters

The deferral contract in `AGENTS.md` states that write actions are "DEFERRED to operator
inbox" for operator review. That contract is only half-implemented: the write is blocked
correctly, but the operator has no passive signal that something is waiting. In practice this
means long-running agent work can queue a dozen GitHub writes, all silently, and the operator
discovers them only by chance or by opening the inbox manually.

It also creates a false sense of completion: the agent reports "filed to operator inbox" and
the work looks done, when in reality nothing will happen until someone happens to look.

## Proposed fix

**Option A (minimal, in `elicitate`):** after the `phinbox ask --from-file <tmp> --async`
call succeeds, emit an OS notification when `urgency` is `warning` or `error`:

```python
if spec.get("urgency") in ("warning", "error") and q.get("request_id"):
    subprocess.Popen(
        [PHINBOX, "ask", "--from-file", tmp, "--notify"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        start_new_session=True,
    )
```

or directly invoke `osascript -e 'display notification ...'` on macOS. The `osascript`
spawn path already exists in phinbox's renderer set, so no new dependency is introduced.

**Option B (in `jcode-tool-safety`):** after a successful `deferred` response, call a
`notify` helper with `$title`, `$cmd` (truncated), and the `request_id`. This keeps the
notification policy next to the approval policy where it is easy to audit.

**Option C (in phinbox):** make `--async` notify by default when `urgency=warning` or
`error`, with an explicit `--no-notify` escape hatch. This is the most correct location
because phinbox owns both the inbox and the native-renderer primitives, but it requires
rebuilding the Rust binary.

**Recommendation: Option B.** It is a shell-only change, keeps the approval and notification
policies in one auditable file, and needs no phinbox rebuild. Option C is the better
long-term home and should be filed as a follow-up.

## Acceptance criteria

- A deferred `warning`-urgency approval produces an OS notification within 5s of deferral.
- `notified_via` in the queued inbox JSON records the channel actually used
  (e.g. `["macos_notification"]`) instead of remaining `[]`.
- Re-running a deferred command whose approval is already recorded still replays from
  `answered/` without a duplicate notification.
- `urgency=info` items remain silent (no notification spam).
- Existing behavior for `answered`/`cancelled`/`timed_out`/`failed` responses is unchanged.

## Non-goals

- Changing the approval policy itself (which commands gate vs. pass) is out of scope.
- Adding a notification for `info`-urgency prompts is out of scope.

## Related

- Filed from `feature/upstream-0.88-sync` at `8ee60a85766eb29ab34cff0e2119cb5428e114c9`
- Source trace recorded in `docs/sessions/20260924-upstream-0.88-sync/10_INBOX_NOTIFICATION_GAP.md`
- Affects every harness that routes approvals through `elicitate`: jcode, kcode, forge, helioslite

---

## Cross-session record (added 2026-10-09 00:55)

The inbox-classification hazard (classify by `Command:` field, not by `--body-file` resolvability) was promoted to `~/.jcode/memories/agents.md` lesson **#19**, so future sessions that touch the operator inbox start from the correct mental model. The 6 high-hazard foreign items left in the inbox remain un-dispatched and continue to wait for explicit operator approval.

