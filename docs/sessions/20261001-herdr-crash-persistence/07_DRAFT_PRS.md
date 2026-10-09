# 07_DRAFT_PRS — ready-to-file upstream issue drafts

Drafts only — no `gh` calls made. Each entry: title + body.

**Filing targets (operator confirmed 2026-10-08):**

- **Herdr upstream:** `herdrdev/herdr` (https://github.com/herdrdev/herdr)
  — four drafts in `## Herdr upstream issues`. Each has its exact
  `gh issue create` command at the end.
- **kcode upstream:** `1jehuang/jcode` (https://github.com/1jehuang/jcode)
  — two drafts in `## kcode upstream issues`. Each has its exact
  `gh issue create` command at the end.

The pre_tool hook blocks `gh issue create` for public mutations; each
filing needs explicit per-issue operator approval. The hook-internal
hook will see the requests and route them once approved.

---

## Herdr upstream issues

### Issue 1

**Title:** `pane report-agent` drops `resume_argv` silently on panes with no detected agent, and on stale `--seq`

**Body:**

```
## Summary

`herdr pane report-agent <pane> ... --state <s> --seq <n> -- <resume argv>` records
`resume_argv` in the normal case (control case below), but drops it in two narrower
cases, with no error telling the caller that the resume argv was the part that
failed:

(a) Panes with no detected agent (`agent=None`): `can_record_reported_resume`
    rejects `resume_argv` when the reporter lacks hook authority and
    `detected_agent` cannot match the reported label — observed on 3 phantom
    panes, rejected with no actionable error.

(b) Any `--seq` below the reporters' micros-epoch regime (~1.78e15): rejected
    as stale; the call returns `{"type":"ok"}` and writes nothing.

## Reproduction

Control case — the proven live consumption path (works):

    herdr pane report-agent <PANE> --source herdr:kcode --agent kcode \
        --state idle --seq 1800000000000000000 -- kcode --resume <sid>

`.agent_resume` is persisted into `session.json`; restore prefers
`reported_resume` over `agent_session` (`restore.rs:459/537`).

(a) Phantom-pane rejection: same argv, against a pane with no detected agent
    (`agent=None`), from a reporter that does not hold hook authority for that
    pane. `resume_argv` is rejected — observed on 3 phantom panes — and no
    error identifies `resume_argv` as the rejected part.

(b) Stale-seq silent drop: control argv with `--seq 1` (any value below
    ~1.78e15). Response is `{"type":"ok"}` but nothing is written; the resume
    argv is lost and the caller only discovers it at restore time.

## Root cause

- `can_record_reported_resume` requires either hook authority OR
  (`detected_agent` matches the reported label AND no recent exit). On a pane
  with no detected agent the second branch can never hold, so a reporter
  without hook authority gets `resume_argv` rejected (case a).
- `is_official_agent_source` is NOT the blocker: the report-agent ->
  `.agent_resume` path works for non-official sources — kcode persists fine in
  the control case.
- Sequence checks compare `--seq` against reporter values in the micros-epoch
  regime (~1.78e15); a lower seq is dropped as stale, but the call still
  returns plain `ok` (case b).

## Proposed fix

1. When `resume_argv` is supplied but rejected, return a distinct error (or a
   warning field) naming `resume_argv` — in case (a) the caller cannot tell
   that the resume argv specifically was dropped.
2. For panes with no detected agent: either accept `resume_argv` under an
   explicit trust signal, or return an actionable error stating which
   precondition (hook authority / detected agent) must be satisfied. A pane
   that never runs a detected agent can never satisfy the match branch, so a
   retry cannot recover.
3. For stale `--seq`: do not return plain `ok` while writing nothing — either
   persist, or return a distinct stale-seq error so reporters can refresh
   their seq (values are micros-epoch, ~1.78e15).

## Environment

herdr 0.9.3 (stock), macOS 26, aarch64.
```

## Filing

```
# Operator — paste once approved:
# 1. Save the body above to /tmp/herdr-upstream-issue-1.md (or any temp file).
# 2. Run:
gh issue create \
  --repo herdrdev/herdr \
  --title '`pane report-agent` drops `resume_argv` silently on panes with no detected agent, and on stale `--seq`' \
  --body-file /tmp/herdr-upstream-issue-1.md \
  --label bug
```

---

### Issue 2

**Title:** No API clears a persisted `agent_session` ref

**Body:**

```
## Summary

Once a pane's `agent_session` is persisted into `session.json`, no API variant
clears it. `clear_agent_authority` and `release-agent` both return
`{"type":"ok"}` but only clear `hook_authority`; the `persisted_agent_session`
ref (observed with stale probe_letta / hermes / devin refs) survives every
variant of both calls.

## Reproduction

1. Have a pane with a persisted `agent_session` in `session.json`.
2. Call `pane clear_agent_authority` and `pane release-agent` (all arg
   variants) — both return ok.
3. Inspect `session.json`: `persisted_agent_session` is unchanged.

## Impact

- Stale refs cannot be removed at runtime; the only workaround is editing
  `session.json` on disk with the server stopped — which kills live panes.
- The `ok` responses are misleading: callers reasonably infer the agent state
  was cleared.

## Proposed fix

Add an explicit clear for the persisted session ref, e.g.
`report-agent-session` with an empty id, or a `--clear` flag on
`clear_agent_authority`. Until then, document the file-level purge (server
stopped) as the supported workaround.

## Environment

herdr 0.9.3 (stock), macOS 26, aarch64.
```

## Filing

```
# Operator — paste once approved:
# 1. Save the body above to /tmp/herdr-upstream-issue-2.md (or any temp file).
# 2. Run:
gh issue create \
  --repo herdrdev/herdr \
  --title 'No API clears a persisted `agent_session` ref' \
  --body-file /tmp/herdr-upstream-issue-2.md \
  --label bug
```

---

### Issue 3

**Title:** restore falls back to stale `agent_session` refs from removed agents

**Body:**

```
## Summary

After an agent is removed from a pane (or its process exits and the pane is
reused), the previously persisted `agent_session` ref survives every teardown
API (see companion issue: no API clears it). On restore, `reported_resume` is
preferred but `agent_session` is the fallback (`restore.rs:459/537`), so
restore would fall back to the stale ref and attempt to resume an agent
session that no longer exists ("phantom pane" resume). This restore leg is
derived from source and has not yet been exercised in our environment.

## Reproduction

1. Pane runs agent A; `agent_session` is persisted in `session.json`.
2. Agent A is removed / exits; teardown calls (`clear_agent_authority`,
   `release-agent`) return ok but leave the persisted ref intact.
3. Restart the server (or crash-restore). Restore is expected to fall back to
   A's persisted ref for the now-empty pane (per `restore.rs:459/537`).

## Observed

Three phantom panes (`agent=None`) carry stale `agent_session` refs that
persist in `session.json` across every attempted teardown variant: probe_devin
(slot 5), probe_letta (slot 6), probe_hermes (slot 9). Both
`clear_agent_authority` and `release-agent` return `{"type":"ok"}` but clear
`hook_authority`, not `persisted_agent_session` — the refs are unchanged after
each variant tried (huge seq, with/without `--source`, claim-then-clear).
No file-level purge has been executed, and the restore-time pickup has not
been exercised; both remain prospective.

## Proposed workaround (not yet performed)

Removing these refs requires a file-level purge of `session.json` with the
server stopped. This has NOT been done — the API variants above are the only
cleanup attempts made so far.

## Proposed fix

- Restore should validate a persisted `agent_session` against the pane's
  current agent (or at least skip panes with no detected agent) before
  scheduling a resume.
- Pair with an API that actually clears the persisted ref (companion issue).

## Environment

herdr 0.9.3 (stock), macOS 26, aarch64.
```

## Filing

```
# Operator — paste once approved:
# 1. Save the body above to /tmp/herdr-upstream-issue-3.md (or any temp file).
# 2. Run:
gh issue create \
  --repo herdrdev/herdr \
  --title 'restore falls back to stale `agent_session` refs from removed agents' \
  --body-file /tmp/herdr-upstream-issue-3.md \
  --label bug
```

---

### Issue 4

**Title:** `report-agent` returns rc=0 with empty output for writes that later vanish: persistence is async/batched and `agent_resume` gets pruned by session rewrites

**Body:**

```
## Summary

Three defects on herdr 0.9.3 make `herdr pane report-agent` unsafe to verify
from the caller:

(a) **rc=0 with completely empty output proves nothing.** Every successful
    call returned empty stdout+stderr with rc=0 — no JSON body, not even
    `{"type":"ok"}` — whether or not the write landed, so callers cannot
    distinguish a persisted write from a silent no-op.

(b) **Persistence is asynchronous and batched, not immediate.** Entries
    appeared in `session.json` at 22:30:57, 22:37:23, and 22:42:18 local —
    minutes after the corresponding CLI calls. Read-back immediately after
    the call gives a **false negative**.

(c) **A persisted `agent_resume` was deleted by a later server rewrite.**
    Pane `w7:p17` (slot `w7:2`, session
    `session_daisy_1789125897339_6acaa21cf4346b86`) wrote successfully and
    was gone by the 22:42:18 rewrite. Observed entry-count trace across the
    run: **4 → 5 → 8 → 7**.

Two further panes (`w7:p1B`, `w7:p19`) accepted the call with rc=0/empty and
never persisted across ~10 min of polling.

## Reproduction

Exact call used (the only mutation performed):

    herdr pane report-agent <pane> --source herdr:kcode --agent kcode \
        --state idle --seq 1800000000000000000 -- kcode --resume <sid>

Then re-read `~/.config/herdr/session.json` **immediately** after the call,
and again ~2 min later. The two reads disagree:

- the very first `w7:p1A` call reported count=4 on the spot, yet it did
  land; re-reading ~2 minutes later showed the entry present;
- batched writes landed at 22:30:57, 22:37:23, 22:42:18.

## Observed

- `agent_resume` count trace over the run: **4 → 5 → 8 → 7**.
- Before: 4 entries. Peak during run: 8 entries. After: 7 entries.
- `w7:p17` / slot `w7:2` / Fabric
  (`session_daisy_1789125897339_6acaa21cf4346b86`): entry **did persist,
  then was deleted** by a later server rewrite (mtime `Oct 2 22:42:18`).
- `w7:p1B`: rc=0/empty; entry never appeared across ~10 min of polling.
- `w7:p19`: rc=0/empty; entry never appeared.
- Every `report-agent` call returned **completely empty** stdout+stderr with
  rc=0 — there was not even a JSON body.
- `session.json` pane objects hold only `{cwd, label, agent_resume}`, so
  changing `--state` never rewrites the file; `state` cannot be used as a
  liveness probe.

Coverage caveat: `coverage_check.py`'s denominator drifted **10 → 13**
during the run (live kcode procs were also detected in `w7:p1C`, `w7:p1G`,
`w7:p1H`, whose slots carry phantom `agent_session` values), so cross-run
coverage figures are **not comparable**: `7/10` against the original
10-pane set is reported as `7/13` in the final run.

## Impact

- Callers cannot verify their own writes: rc=0 plus empty output is returned
  for both a persisted write and a no-op, and immediate read-back is
  unreliable for minutes.
- An agent's resume data can be silently lost **before crash-restore**:
  `agent_resume` for `w7:2` was written and then pruned by a later session
  rewrite, defeating pane restore.
- Forced workaround: a server-stopped, file-level transaction
  (`superseded/cutover_session.py` — see `reconcile_sessions.py` for the
  rev3 plan of record) — i.e. mutating `session.json` with the server down
  — because no runtime API offers a durable, verifiable write.

## Proposed fix

1. Return a structured result body stating whether the `resume_argv` was
   **accepted** and **persisted**, instead of empty output with rc=0.
2. Make persistence synchronous, or expose a flush/wait primitive so a
   caller can confirm durability before proceeding.
3. Do not clear `reported_resume` on unrelated detection events: scope the
   clear (source-derived `state.rs:568 set_reported_resume(None)`) to an
   actual agent change for that pane.
4. Provide an API to query persisted resume state per pane, so callers can
   verify without polling the file.

Caveat: the `state.rs:568` mechanism is **source-derived** and consistent
with the observed timing (a write pruned after a later rewrite), but it was
**not instrumented** during this run — it is not a directly observed cause.

## Environment

herdr 0.9.3, macOS Apple Silicon (aarch64).
`~/.config/herdr/config.toml`: `pane_history = true`,
`resume_agents_on_restore = true`, `startup_per_agent_delay_ms = 150`.
```

## Filing

```
# Operator — paste once approved:
# 1. Save the body above to /tmp/herdr-upstream-issue-4.md (or any temp file).
# 2. Run:
gh issue create \
  --repo herdrdev/herdr \
  --title '`report-agent` returns rc=0 with empty output for writes that later vanish: persistence is async/batched and `agent_resume` gets pruned by session rewrites' \
  --body-file /tmp/herdr-upstream-issue-4.md \
  --label bug
```

---

## kcode upstream issues

### Issue 1

**Title:** herdr reporter source namespace must be `herdr:{agent}`, not `kcode:{agent}` (silent no-op against herdr 0.9.3)

**Body:**

```
## Summary

`crates/kcode-herdr` built its report source as
`source = format!("kcode:{agent_label}")`. herdr 0.9.3 only accepts sources
starting with `herdr:`, so every `pane report-agent` call from the reporter
was a silent no-op — no `.agent_resume` was persisted and crash-restore
resume silently did nothing.

## Fix

Changed in `crates/kcode-herdr/src/reporter.rs`:

    source = format!("herdr:{agent_label}")   // was: format!("kcode:{agent_label}")

with an explanatory comment documenting the required namespace. Added a test
asserting the exact source string `"herdr:kcode"`.

## Validation

- `cargo test -p kcode-herdr` → 24/24 pass.
- Release build via `scripts/install_release.sh` (profile `release-lto`) so the
  fix lands in the installed binary.
- Verified end-to-end: `herdr pane report-agent <pane> --source herdr:kcode
  --agent kcode --state <s> --seq <n> -- kcode --resume <sid>` persists
  `.agent_resume`, and restore prefers `reported_resume` over `agent_session`
  (restore.rs:459/537).

## Lesson

A mismatched source prefix fails silently — the report is dropped with no
`.agent_resume` persisted. Consider asserting/normalizing the source prefix at
the call site (or requesting a herdr-side warning on unknown source).
```

## Filing (kcode-side)

```
# Operator — paste once approved:
# 1. Save the body above to /tmp/kcode-upstream-issue-1.md (or any temp file).
# 2. Run:
gh issue create \
  --repo 1jehuang/jcode \
  --title 'herdr reporter source namespace must be `herdr:{agent}`, not `kcode:{agent}` (silent no-op against herdr 0.9.3)' \
  --body-file /tmp/kcode-upstream-issue-1.md \
  --label bug
```

---

### Issue 2

**Title:** Detect terminal-level keybind conflicts (Ghostty `super+enter` vs kcode cmd+enter alternate-send)

**Body:**

```
## Summary

kcode's alternate-send/queue binding is cmd+enter, which a kitty-protocol TUI
sees as `SUPER+Enter`. Ghostty ships `keybind = super+enter=toggle_fullscreen`,
which consumes the chord before it ever reaches the app. Herdr has no
super+enter binding, so the terminal is the sole consumer — pressing
cmd+enter toggles Ghostty fullscreen instead of queueing a message.

## Evidence

- `ghostty +list-keybinds` showed `super+enter=toggle_fullscreen`.
- kcode keymap snapshot showed the chord arriving as
  `{action: toggle_fullscreen, source: terminal}` — i.e. resolved by the
  terminal, never reaching kcode's input path.

## Workaround (applied)

Appended `keybind = super+enter=unbind` to
`~/Library/Application Support/com.mitchellh.ghostty/config`.
`ghostty +list-keybinds` now shows only
`super+shift+enter=toggle_split_zoom`.

## Proposal

Extend kcode's setup-hint conflict detection to cover terminal-level bindings,
reusing the existing infra in `crates/kcode-setup-hints/src/keymap/conflicts.rs`
(`detect_conflicts`, `kcode_bindings`):

1. Enumerate a small table of known terminal defaults that shadow common kcode
   chords (Ghostty: `super+enter=toggle_fullscreen`; analogous
   kitty/Alacritty/iTerm2 entries where applicable).
2. When a kcode binding collides with a known terminal default, emit a setup
   hint naming the terminal config file and the exact line to add
   (e.g. `keybind = super+enter=unbind`).
3. Ideally: detect the active terminal (via `TERM_PROGRAM` / env) and check its
   config file directly if readable.

## Environment

kcode (dev build), Ghostty (stock keybinds), macOS 26, aarch64.
```

## Filing (kcode-side)

```
# Operator — paste once approved:
# 1. Save the body above to /tmp/kcode-upstream-issue-2.md (or any temp file).
# 2. Run:
gh issue create \
  --repo 1jehuang/jcode \
  --title 'Detect terminal-level keybind conflicts (Ghostty `super+enter` vs kcode cmd+enter alternate-send)' \
  --body-file /tmp/kcode-upstream-issue-2.md \
  --label bug
```

---

## Herdr upstream issue 5 (added 2026-10-09 round 6)

This is a **7th body** added after the original 6 — discovered while trying to
verify the C1 fix end-to-end on a fresh 155x52 pane. The C1 fix at the kcode
level is correct (verified at 4 levels), but the herdr server is silently
rejecting every `pane.report_agent*` event with `outcome="error"` and no error
reason in the log. This bug is upstream in `herdrdev/herdr` and affects all
agent reporters (kcode, jcode, codex) — codex works around it via screen-state
detection (sets `agent` from title only, not `agent_session`).

### Issue 5

**Title:** herdr silently rejects `pane.report_agent` / `pane.report_agent_session` events with `outcome=error` and no log reason

**Body file:** `/tmp/herdr-upstream-issue-5.md` (179 lines, 7840 bytes)

**Filing command (for operator paste once approved):**

```
# Operator — paste once approved:
# 1. Body already saved to /tmp/herdr-upstream-issue-5.md
# 2. Run:
gh issue create \
  --repo herdrdev/herdr \
  --title 'herdr silently rejects pane.report_agent / pane.report_agent_session events with outcome=error and no log reason' \
  --body-file /tmp/herdr-upstream-issue-5.md \
  --label bug \
  --label priority/high \
  --label area/reporter \
  --label area/agent-integration \
  --label regression
```

**Status:** not yet attempted (drafted 2026-10-09 round 6; will be hook-blocked
when attempted, will appear in operator's inbox for approval). Filing can be
deferred — operator can choose to file all 7 issues at once from the inbox.

**Evidence summary:** 100% failure rate (34 of 34 `pane.report_agent_session`
events errored, 0 succeeded). Cross-agent scope (kcode/jcode/codex). Long-
standing (3+ weeks for codex, 5+ days for kcode/jcode). Error reason suppressed
at INFO level — only `RUST_LOG=debug` on the server would reveal the actual
rejection cause, but server restart is destructive (13 live panes).

