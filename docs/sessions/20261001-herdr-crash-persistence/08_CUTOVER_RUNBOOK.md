# 08_CUTOVER_RUNBOOK — herdr session reconciliation + restore verification (C4, rev3)

Operator-facing runbook. Follow phases in strict order. Every checkbox in a
phase must be ✓ before starting the next phase.

**Plan of record (rev3):** `reconcile_sessions.py` in this directory.

Rev3 supersedes rev2 in full. Rev2 was a fixed-topology script
(`superseded/cutover_session.py` in this directory — preserved for history;
rev3's `reconcile_sessions.py` is the live plan of record) that hardcoded
pane slots, labels, pane IDs and session IDs. Live evidence disproved that
assumption repeatedly: slots migrated (Omni 8→7, ForgeCode 9→8), a labeled
pane (`Cons`) vanished, a new one (`Cockpit`) appeared, and an orphaned SID
moved between pane and slot representations between two consecutive runs.
Rev2 also asserted a fixed `13/13` coverage figure that the live topology
does not support.

Rev3 therefore discovers **everything** at runtime and refuses to guess.

**Never run `herdr pane report-agent` during this cutover.** The
live-injection strategy was DISPROVEN — rationale in **Appendix A**.

---

## Two invariants that govern every phase

1. **Never invent a session id.** A pane receives `agent_resume` only when a
   *live* process proves the SID. Ambiguity yields `SKIP`, never a guess.
   A fabricated SID can resume the wrong session — a resume storm.
2. **The herdr daemon owns `session.json`.** It holds session state in
   memory and rewrites the whole document on a timer (~60s), reverting any
   external edit. A file write is **necessary but not sufficient**; it must
   be followed by a daemon restart. Full evidence in **09_DAEMON_OWNS_STATE.md**.

Invariant 2 is why this is a **two-phase** procedure. Discovery needs the
daemon live (its CLI is the source of truth, and CLI calls auto-start the
daemon). The write must happen with the daemon stopped, without any CLI
call.

---

## Phase 0 — Preflight (read-only)

- [ ] **P0.1 Syntax gate**
      ```bash
      python3 -c "import ast;ast.parse(open('reconcile_sessions.py').read());print('PARSE_OK')"
      ```
- [ ] **P0.2 Rollback copy taken**
      ```bash
      cp ~/.config/herdr/session.json ~/.kcode/scratch/session_pre_apply_$(date +%Y%m%d_%H%M%S).json
      ```
- [ ] **P0.3 Record the discovered topology, not an assumed one**
      ```bash
      timeout 60 herdr pane list
      ```

## Phase 1 — Discovery (daemon MUST be live)

- [ ] **P1.1 Emit a plan from the live file**
      ```bash
      python3 reconcile_sessions.py --emit-plan /tmp/plan.json ~/.config/herdr/session.json
      ```
      Read the output. Expected shape: `KEEP` for panes already correct,
      `ADD`/`REPLACE` only where a live process proves a SID, `REMOVE` for
      `probe_*` phantoms, `SKIP` where correlation is ambiguous.

      `SKIP` lines are **not** failures — they are the safety property. An
      unlabeled pane sharing a cwd with many candidates cannot be
      correlated without guessing, so it is left alone.

## Phase 2 — Write (daemon MUST be stopped)

- [ ] **P2.1 Stop the daemon and wait for death**
      ```bash
      timeout 30 herdr server stop
      for i in $(seq 1 20); do ps -axo command= | grep -q '[/]herdr server' || break; sleep 1; done
      ps -axo command= | grep -q '[/]herdr server' && echo "STILL LIVE - ABORT"
      ```
      Verify with `ps` **only**. Any `herdr` CLI call here would auto-start
      the daemon and clobber the write.

- [ ] **P2.2 Apply the pre-discovered plan**
      ```bash
      python3 reconcile_sessions.py --apply --plan-file /tmp/plan.json ~/.config/herdr/session.json
      ```
      This mode makes **no** herdr CLI call. It refuses if the file changed
      since discovery (`_source_sha` mismatch), so a stale plan cannot be
      applied.

- [ ] **P2.3 Verify before restarting**
      ```bash
      python3 -c "
      import json,collections
      raw=open('$HOME/.config/herdr/session.json').read(); d=json.loads(raw)
      sids=[p['agent_resume']['argv'][-1] for w in d['workspaces'] for t in w['tabs']
            for p in t['panes'].values() if isinstance(p.get('agent_resume'),dict)]
      c=collections.Counter(sids)
      print('resume=%d dup=%d probes=%d'%(len(sids),sum(1 for v in c.values() if v>1),raw.count('probe_')))"
      ```
      **`dup` must be 0** — a duplicate SID would resume one session twice.

## Phase 3 — Restart and verify persistence

- [ ] **P3.1 Let the daemon restart** (an interactive `herdr` client
      normally does this on its own) or start it explicitly.
- [ ] **P3.2 Stability gate** — the decisive check. Pre-fix, the daemon
      reverted the file within ~60s, so a single check proves nothing:
      ```bash
      for i in 1 2 3 4 5; do shasum -a 256 ~/.config/herdr/session.json; sleep 25; done
      ```
      All five hashes identical AND `probes=0` → the daemon has accepted the
      reconciled state.
- [ ] **P3.3 Re-verify coverage with a fresh discovery** (daemon live):
      ```bash
      python3 reconcile_sessions.py --emit-plan /tmp/verify.json ~/.config/herdr/session.json
      ```
      Pass when `add=0`, `replace=0`, `purge=0`. `live_proven` should equal
      `keep`. **Judge coverage against the discovered set, never a fixed
      number** — the pane count changes as panes open and close.

---

## Acceptance criteria

| Criterion | How to check |
|---|---|
| Every discovered kcode pane has a proven `agent_resume` | `add=0` and `replace=0` in a fresh discovery |
| No resume storms | duplicate SID rows = 0 |
| No phantom coverage | `probe_` count = 0, `purge=0` |
| Nothing fabricated | every `SKIP` has no live proof; no SID appears without one |
| State persists | 5 identical hashes across 125s |
| No unrelated churn | pane row count unchanged by the write |

## Abort conditions

Stop and revert to the Phase 0 rollback copy if any of these occur:

- `dup` (duplicate SID rows) is non-zero after the write.
- The pane row count changes across the transaction.
- Any `herdr` CLI call is required to complete Phase 2.
- The daemon reverts the file during Phase 3 (means the restart did not
  take, or another writer is active).
- `_source_sha` mismatch on apply (the topology moved under you — re-run
  Phase 1).

## Appendix A — why live injection was disproven

`herdr pane report-agent` re-injection was tested and discarded: it could
not guarantee exact pane restoration under a moving topology and risked
duplicate resumes. See **01_RESEARCH.md**.

## Appendix B — what replaced it

`reconcile_sessions.py`, which:

- discovers panes, labels, cwd and live SIDs at runtime from
  `session.json`, `herdr pane list`, `herdr pane process-info` (with `ps
  eww` as fallback);
- correlates in strict strength order — unique label → unique live-SID pane
  in the same cwd → unique live holder of the already-stored SID →
  otherwise `SKIP`;
- runs `process-info` calls concurrently (serial execution exceeded a
  120s timeout; concurrent discovery completes in ~2s);
- resolves `agent_resume` independently of phantom purging, so a pane
  holding both keeps its proven SID (the rev2 defect that lost coverage);
- writes atomically via `tmp` + `fsync` + `os.replace`;
- refuses to run when the byte shape of the file is unfamiliar, and refuses
  discovery+write while the daemon is live.
## Phase 4 — End-to-end C1 verification (operator-gated, destructive)

This is the only step that exercises the **actual fix end-to-end**:
SIGKILL a live kcode pane and confirm herdr relaunches it with
`agent_resume.source = "herdr:kcode"` (the value the C1 fix produces).

**Gating:** this is destructive. SIGKILL on a live pane WILL cause a
brief interruption of the work in that pane. Pick a low-priority kcode
pane (e.g. an idle "cockpit" or "byproduct" pane, not a working one).

**Pre-condition (verified 2026-10-08):**

- `~/.local/bin/kcode --version` is `kcode v0.0.0-dev (6e0f0fc9c, dirty)`.
- `~/.kcode/builds/versions/6e0f0fc9c-dirty/kcode` carries
  `flags=0x20002(adhoc,linker-signed)` (the C9 fix is in).
- `cargo test -p kcode-herdr` is 24/24 green (the C1 reporter fix).

**Steps:**

1. Pick a target pane via `herdr pane list` (look for a kcode pane
   with `agent_status: idle`). Record the `pane_id` and current
   `label`.
2. Find the live kcode PID in that pane:
   ```bash
   herdr pane process-info <PANE_ID>
   ```
   Note the `pid`. Confirm it is the kcode binary, not a child shell:
   ```bash
   ps -p <PID> -o pid,command | grep kcode
   ```
3. Snapshot the pre-SIGKILL `agent_resume` for that pane:
   ```bash
   python3 -c "
   import json
   d = json.load(open('$HOME/.config/herdr/session.json'))
   # walk to the pane; print the current agent_resume
   for w in d['workspaces']:
     for t in w['tabs']:
       for p in t['panes'].values():
         if p.get('pane_id') == '<PANE_ID>':
           print(json.dumps(p.get('agent_resume'), indent=2))
   "
   ```
4. **SIGKILL the kcode process** (NOT herdr — the daemon must stay
   running so it can relaunch the pane):
   ```bash
   kill -9 <PID>
   ```
5. Wait ~10s for herdr to detect the death and relaunch. Confirm
   with `herdr pane list` that the pane is back with `agent_status:
   working`.
6. Re-snapshot the `agent_resume`:
   ```bash
   python3 -c "..."   # same script as step 3
   ```
7. **Pass criteria:** the new `agent_resume.source` is `"herdr:kcode"`,
   `argv[0]` is `"kcode"`, and `argv` ends with `--resume <sid>`.
8. **Fail criteria:** the new `agent_resume.source` is `"kcode:kcode"`
   (old code, wrong namespace — the C1 fix is NOT in the running
   binary) or no `agent_resume` is set at all (herdr didn't auto-launch
   the resume).

**Rollback:** if the C1 verification fails, do NOT panic. The kcode
binary at `~/.kcode/builds/versions/6e0f0fc9c-dirty/kcode` is the
linker-signed build that was verified end-to-end by the C9 install test
(see `10_SIGKILL_NON_TTY.md`). If the pane fails to relaunch cleanly,
manually run:
```bash
~/.local/bin/kcode --resume <sid>   # in a new Ghostty tab
```

**Lesson on destructive ops:** this step is the only one in the
cutover that requires explicit operator GO. Every other step is
verifiable from the file + pane list. SIGKILL of a live pane is
**always** the operator's call, never the agent's.

## Appendix C — Phase 4 cross-references

- C1 fix: `crates/kcode-herdr/src/reporter.rs` `format!("herdr:{agent_label}")`.
- Test: `crates/kcode-herdr/src/reporter.rs::tests::reporter_source_format`.
- C9 fix (binary): `27bd2299c` (opt-in gate) + `bfbcf898f` (install verifier).
- Full SIGKILL write-up: `10_SIGKILL_NON_TTY.md`.

## Phase 4 (revision 1) — Lessons learned 2026-10-09

The original Phase 4 protocol (SIGKILL one kcode process, expect herdr to auto-relaunch) **does not work**. Real herdr behavior on a single-pane agent SIGKILL:

1. The agent's `agent_resume` is **cleared** (herdr does not preserve it across a SIGKILL).
2. The pane shell is **reset to a bare zsh prompt** (herdr does not re-execute the resume command).
3. The pane's `agent_status` returns to `unknown` and `agent` is `null`.

The auto-relaunch only happens on **server restore** (`resume_agents_on_restore = true` triggers
on a full herdr restart, not a single-pane SIGKILL). The Phase 4 protocol must therefore
exercise the server-restart path, not the single-pane path.

### What I observed on HelioLite (w8:pS), 2026-10-09T00:31-00:39

**Pre-SIGKILL** (PID 87093):
```json
{
  "source": "jcode",
  "agent": "jcode",
  "argv": ["jcode", "--resume", "session_panda_1789273883949_ace2cf2f18024501"]
}
```

**After `kill -9 87093` (no relaunch)**:
- `agent_resume`: `null`
- `agent_status`: `unknown`
- Foreground process: `zsh` (bare shell, not the jcode binary)

**Attempted relaunches (all failed)**:
- `herdr pane run w8:pS <kcode>` — kcode exits immediately (no `HERDR_PANE_ID` env, so it's treated as a non-agent invocation, then exits with rc=0).
- `herdr pane run w8:pS HERDR_PANE_ID=w8:pS <kcode>` — same.
- `herdr pane run w8:pS <full HERDR env> <kcode>` — same.
- `herdr pane send-text w8:pS "<kcode cmd>"` — text is buffered, no Enter is sent. The next send-text appends to the same line. The shell never executes the command.
- `herdr agent start w8:pS --kind kcode` — `kcode` is not in the supported kinds list (`pi, claude, codex, gemini, cursor, devin, agy, cline, omp, mastracode, opencode, copilot, kimi, kiro, droid, amp, grok, hermes, kilo, qodercli, qwen, letta, maki, muse`).

**HelioLite post-test state (2026-10-09T00:39)**:
- Pane shell is at a clean zsh prompt.
- `agent_resume` is `null`.
- The user can manually run `kcode --resume <sid>` in the pane to restore it (any of the
  available sessions in `~/.kcode/sessions/` will work, e.g. `session_panda_1791432647461_af97ae39c9a2f77d`).
- The original session `session_panda_1789273883949_ace2cf2f18024501` is NOT in the current
  kcode session store; it was an earlier kcode install's session and has been deleted.

### Revised Phase 4 protocol (replaces the SIGKILL-one-pane procedure)

**Goal:** verify the C1 fix's `herdr:{agent_label}` source namespace is emitted by the new
kcode binary when it registers as a herdr agent.

**Pre-conditions (unchanged):**
- `kcode v0.0.0-dev (6e0f0fc9c, dirty)` is on PATH.
- `~/.kcode/builds/versions/6e0f0fc9c-dirty/kcode` carries `flags=0x20002`.
- `cargo test -p kcode-herdr` is green.

**Steps:**

1. Pick a kcode pane (`herdr pane list`, look for `agent_status: working` or `idle`).
2. **Stop herdr with live-handoff** to flush agent state to session.json:
   ```bash
   herdr server stop --handoff
   ```
3. **Restart herdr** (it will re-launch every pane with `agent_resume` set, in stagger mode
   per `resume_stagger_ms`):
   ```bash
   herdr &
   ```
4. Wait ~30s for the relaunches to settle.
5. **Inspect the post-restart `agent_resume` for the target pane:**
   ```bash
   python3 -c "..."  # same walker as before
   ```
6. **Pass criteria:** the new `agent_resume.source` is `"herdr:kcode"` (or `"herdr:{kind}"`
   for any future manifest entry), `agent` is `"kcode"`, and `argv[0]` is `"kcode"`.
7. **Fail criteria:** the new `agent_resume.source` is `"kcode:kcode"` (wrong namespace, the
   C1 fix is NOT in the running binary) or `agent_resume` is `null` (herdr didn't re-launch
   any agent for that pane).

**Why this is the correct protocol:**
- `resume_agents_on_restore = true` is the only path that re-executes the resume command.
- The C1 fix only fires when kcode registers as a herdr agent (the `HERDR_ENV=1` path).
- The single-pane SIGKILL path is a separate code path that the operator would have to
  restore manually (via `agent start` with a kcode manifest entry, or by typing the resume
  command into the pane shell).

**Caveat:** the C4 cutover (herdr server stop + restart) is exactly the Phase 4 protocol
above. Combining C1 and C4 is therefore the natural way to run the verification — the
operator gates C4 anyway.
