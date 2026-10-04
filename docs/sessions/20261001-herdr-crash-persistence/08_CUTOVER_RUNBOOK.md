# 08_CUTOVER_RUNBOOK — herdr session cutover + restore verification (C4, rev2)

Operator-facing runbook. Follow phases in strict order. Every checkbox in a
phase must be ✓ before starting the next phase.

**Plan of record (rev2):** `~/.jcode/scratch/c3_purge_plan.md` §4.1+ executed by
`~/.jcode/scratch/cutover_session.py`. Rev2 replaces the old
stop → purge → start → live `report-agent` re-inject → verify order with ONE
server-stopped file-level transaction (phantom purge + 13-slot gap-fill in a
single atomic write), then start → coverage → prune watch. The live-injection
strategy was DISPROVEN — rationale and evidence in **Appendix A**.
**Never run `herdr pane report-agent` during this cutover.** The old
`purge_phantom_sessions.py` materialization step is superseded by
`cutover_session.py` and is not part of this runbook.

---

## Phase 0 — Preflight (strictly read-only; safe before the operator gate)

- [ ] **P0.1 PARSE_OK syntax gate on the cutover script:**
      ```bash
      python3 -c "import ast;ast.parse(open('$HOME/.jcode/scratch/cutover_session.py').read());print('PARSE_OK')"
      ```
      Must print `PARSE_OK`. Missing file or any parse error = STOP.
- [ ] **P0.2 Coverage baseline recorded — NOT a blocker.**
      ```bash
      python3 ~/.jcode/scratch/coverage_check.py
      ```
      Pre-cutover baseline (coordinator-verified at 06:01 local) — **expected,
      NOT a blocker**:
      ```
      TOTALS agent_resume=5 agent_session=7 | jcode panes covered 5/13 | phantoms 3 | sid mismatches 3 | codex mismatches 0
      VERDICT GAPS_FOUND, exit=1
      ```
      **Reconciliation — read this so nobody miscounts the baseline:**
      - The **3 `sid mismatches` ARE the 3 phantoms** — same 3 rows counted
        twice by different checks: slot w7:5 stored=`probe_devin` vs live
        `session_gorilla…`, w7:6 `probe_letta` vs `session_palmtree…`,
        w7:9 `probe_hermes` vs `session_panda…`. **NOT 3 additional problems.**
      - The **5 KEEP = the 5 MATCH rows**: w7:p1D (hamster), p1E (stallion),
        p1F (seedling), p1J (cricket), w8:pD (snake).
      - The **8 ADDs = the 8 gap rows**: p1A, p1B, p1C, p1G, p1H, p17, p18, p19.
      Closing those gaps is exactly what Phases 2–3 do, with the server stopped.
      **Record the actual output here: `________`. Do NOT treat `GAPS_FOUND` as
      a blocker and do NOT attempt to close gaps live.**
- [ ] **P0.3 Reporter-fixed build installed.**
      `jcode --version` → `v0.0.0-dev (b87cd9955, dirty)` (record exact output:
      `________`). Artifact: `~/.jcode/builds/versions/b87cd9955-dirty/jcode`
      (141,962,592 B). Reporter fix binary-proven: `~/.jcode/scratch/binary_fix_proof.md`
      → verdict **PROVEN**. Stale/wrong binary = STOP, rebuild via
      `bash scripts/install_release.sh`.
- [ ] **P0.4 Baseline counts recorded** (read-only, immediately before stop):
      ```bash
      python3 -c "raw=open('$HOME/.config/herdr/session.json').read();print('resume=',raw.count('\"agent_resume\"'),'session=',raw.count('\"agent_session\"'))"
      ```
      Plan-time baseline: **resume=5, session=7** at 2026-10-02 21:5x +02.
      Take yours fresh — it churns. Do **not** assume 10: the live server
      actively clears entries; the baseline taken now is the only number that
      counts.
- [ ] **P0.5 C1–C4 status cited (peacock's fixes — cite, do not re-do):**
      C1 consumption + reporter source fix **done** (`herdr:jcode` source
      persists `.agent_resume`; restore prefers `reported_resume`
      restore.rs:459/537; `cargo test -p jcode-herdr` 24/24 @ commit
      **6f886a0f**, cite only unless the tree changed). C2 config **done**
      (`~/.config/herdr/config.toml`: `[experimental] pane_history = true`,
      `resume_agents_on_restore = true`, `startup_per_agent_delay_ms = 150`).
      C3 revised → this rev2 plan. C4 = this runbook (operator gate below).
- [ ] **P0.6 Evidence pointers exist** (rev2 rationale; detail in Appendix A):
      `~/.jcode/scratch/gaps_filled.md`, `~/.jcode/scratch/diag_report_agent.md`,
      `~/.jcode/scratch/binary_fix_proof.md`.

---

> ## OPERATOR DECISION GATE — resolve BEFORE Phase 1
>
> (Position note: preflight above is read-only, so it runs first; this gate sits
> immediately before the first mutating step. Gate content unchanged.)
>
> **C4 requires operator go/no-go because restart kills live panes.**
> Stopping the main herdr server (pid 1266) terminates every live pane session.
> Nothing past this point may run until the operator explicitly says **GO**.
> If NO-GO: stop here; the cutover stays pending.

> ### ⚠ WARNING — point of no return
> Stopping main herdr (**pid 1266**) **ends all live panes**. Under rev2 this is
> safe for resume coverage: any `agent_resume` gaps present at stop time
> (P0.2 `GAPS_FOUND`) are **expected** and are closed by the file-level
> transaction in Phases 2–3 while the server is stopped — so `session.json` is
> correct at the exact instant of restart. **Do NOT close gaps with live
> `herdr pane report-agent`** (disproven — Appendix A). The failure mode this
> project exists to fix (the Sep-28 manual Up+Enter storm) is a pane missing
> `agent_resume` **at start time**; Phase 4's immediate coverage gate and the
> Phase 5 prune watch catch exactly that.

---

## Phase 1 — STOP the herdr server

- [ ] **1.1 Graceful stop** (plan's exact stop command; the API-socket stop of
      pid 1266 persists `session.json` on shutdown):
      ```bash
      herdr server stop
      ```
      Verify: `herdr status server` → not running; `~/.config/herdr/herdr.sock`
      gone; pid 1266 exited (`lsof -p 1266` empty). **Never `kill -9`; never
      touch pane processes.**

---

## Phase 2 — COPY + dry-run on the COPY (live file untouched)

- [ ] **2.1 COPY:**
      ```bash
      cp ~/.config/herdr/session.json ~/.jcode/scratch/session_cutover_dryrun.json
      ```
      The dry-run NEVER touches the live file.
- [ ] **2.2 DRY-RUN on the COPY:**
      ```bash
      python3 ~/.jcode/scratch/cutover_session.py --force --dry-run ~/.jcode/scratch/session_cutover_dryrun.json
      ```
      Guard (d) REFUSES whenever a live server is detected (lsof socket holder +
      `herdr server` pid) **even against a copy**; `--force` overrides it —
      and **`--force` is permitted ONLY for `--dry-run` against a COPY**. The
      script hard-refuses `--force` combined with `--apply`, and refuses any
      live-server run without `--force`.
- [ ] **2.3 Review the diff — accept ONLY if:**
      - `agent_resume: 5 -> 13   (+8 added, 0 replaced, 5 unchanged)`
      - `agent_session: 7 -> 4   (-3 removed)`
      - `bytes: 11038 -> 12780 (+1742)`
      - trailing line `DRY-RUN: nothing written`, exit code **rc=0**
      - exactly 3 `REMOVE` lines (slots **5/6/9** — probe_devin / probe_letta /
        probe_hermes under labels PhenoShared / Agile / ForgeCode), 8 `ADD`
        (slots 1,2,3,4,5,6,7,9), 5 `KEEP` (slots 8,10,11,12,14)
      - unified diff touches ONLY those pane objects; no `REFUSED` / `WARN`
      - **Review the diff by CONTENT (3 REMOVE / 8 ADD / 5 KEEP), NEVER by line
        position:** herdr rewrites JSON with non-deterministic key order — churn
        observed 05:53 local produced a 293-line diff that was semantically
        identical (5/7/11038). Line numbers moving is expected; the content
        tally is the acceptance criterion.
      **Invariants if the live file churned since plan time** (counts are taken
      fresh at P0.4): final `agent_resume` = **13**, final `agent_session` =
      baseline − 3, exactly 3 REMOVE on slots 5/6/9, rc = 0. Any other shape →
      **ABORT (Abort criteria 1)** — do not proceed to Phase 3.

---

## Phase 3 — APPLY the transaction to the live file

- [ ] **3.1 Apply** (server still stopped — Phase 1 verified):
      ```bash
      python3 ~/.jcode/scratch/cutover_session.py --apply ~/.config/herdr/session.json
      ```
      **No `--force`.** Guard (d) must PASS on its own merits, not be
      overridden: if it refuses, the server is not actually stopped → back to
      Phase 1 verification. `--force` with `--apply` is hard-refused by the
      script and must never be attempted.
- [ ] **3.2 Verify read-only:**
      - `grep -c 'probe_devin\|probe_letta\|probe_hermes' ~/.config/herdr/session.json` → `0`
      - `agent_resume` count → **13**; `agent_session` → **4** (codex only)
      - script printed `WROTE … atomically (os.replace)` and exited 0
      - any other result → **ABORT (Abort criteria 2)** before starting.

---

## Phase 4 — START server + IMMEDIATE coverage gate

- [ ] **4.1 Start** (peacock C2-corrected argv — keep byte-identical, never
      merge the binary and subcommand into one quoted string):
      ```bash
      nohup "$HOME/.local/bin/herdr" server >/dev/null 2>&1 &
      ```
      Verify: `herdr status server` → `running`; `herdr pane list` responds.
      Failure to come up → **Abort criteria 3** → Phase 8 rollback.
- [ ] **4.2 IMMEDIATE coverage gate** — as the very next command, before any
      startup agent-detection event can wipe entries (`state.rs:568`):
      ```bash
      python3 ~/.jcode/scratch/coverage_check.py
      ```
      **Acceptance — ALL of these must hold (coordinator-verified):**
      ```
      jcode panes covered 13/13 | phantoms 0 | sid mismatches 0 | codex mismatches 0
      agent_resume=13 agent_session=4
      VERDICT COVERAGE_OK, exit=0
      ```
      Missing ANY one → Phase 5 loop / Abort criteria.

---

## Phase 5 — PRUNE WATCH (explicit loop, max 3 attempts)

- [ ] **5.1 Re-run coverage after ~5 min** to catch a `state.rs:568` wipe fired
      by a startup agent-detection event:
      ```bash
      sleep 300 && python3 ~/.jcode/scratch/coverage_check.py
      ```
      **ANY drop below 13/13 = `state.rs:568` wipe** (likewise a non-zero
      phantom/sid/codex-mismatch count or `agent_resume` < 13), regardless of
      which individual slots survived.
- [ ] **5.2 Decision:**
      - **13/13 + 0 phantoms** → cutover verified; proceed to Phase 6.
      - **Entries wiped** (count dropped, slots re-cleared) → **STOP the server
        again** (`herdr server stop`) and **re-apply**: repeat Phase 2 → 3 → 4
        (copy → dry-run on copy → apply → start → immediate coverage). This is
        an explicit loop, not a one-off: document **every** iteration in
        `05_KNOWN_ISSUES.md` (timestamp, which slots were wiped, iteration
        count) and re-run 5.1 after each restart.
      - **Max 3 consecutive attempts.** If the wipe recurs **3×** → **ABORT
        (Abort criteria 4) with the observed counts recorded** (the full
        `TOTALS … | covered … | phantoms … | sid mismatches … | codex
        mismatches …` line + verdict + exit code from the failing run):
        root-cause `state.rs:568` before any further re-apply; do not loop
        indefinitely.

---

## Phase 6 — On-disk verification of resume argv/source

- [ ] **6.1 Every target slot verified in `session.json`** — all 13 slots
      (ws0 `"1".."12"` + ws1 `"14"`), each byte-shape:
      `{"source": "herdr:jcode", "agent": "jcode", "argv": ["jcode", "--resume", "<sid>"]}`
      with the sid matching its live pane (discover sids via the `HERDR_PANE_ID`
      loop in `c3_purge_plan.md` §3 — **never guess a sid**):
      ```bash
      grep -c '"agent_resume"' ~/.config/herdr/session.json                                   # → 13
      grep -c 'probe_devin\|probe_letta\|probe_hermes' ~/.config/herdr/session.json           # → 0
      python3 -c "import json;p=json.load(open('$HOME/.config/herdr/session.json'))['workspaces'][0]['tabs'][0]['panes']['3'];print(p['agent_resume']['source'],p['agent_resume']['argv'])"
      # → herdr:jcode ['jcode', '--resume', 'session_calf_1789553284753_4beadc3f37d2b6dc']
      ```
- [ ] **6.2 Abort criteria (below) re-checked** — any that fire → stop
      immediately per that criterion.

---

## Phase 7 — Restore verification (C4)

Prove restore actually resumes panes across a kill/relaunch.

- [ ] **7.1 Stop herdr again** (graceful: `herdr server stop`; snapshot +
      `session.json` persist on shutdown), **then start** (Phase 4.1 command).
- [ ] **7.2 Confirmed resumptions:** restored jcode panes have live `jcode`
      processes with the correct sessions:
      ```bash
      for pid in $(pgrep -f 'jcode(\.real)? --resume'); do
        ps eww -p "$pid" -o command= | grep -o 'HERDR_PANE_ID=[^ ]*'
        ps -p "$pid" -o command= | grep -o 'session_[a-z0-9_]*'
      done
      ```
      Each restored pane: `HERDR_PANE_ID` present and process sid **matches**
      its `session.json` `agent_resume` argv sid. Mismatch = FAIL.
- [ ] **7.3 No bare `jcode --resume` storms:** server log
      (`~/.config/herdr/herdr-server.log`) shows **0 unexpected resume fires**;
      expected set = **exactly the panes with `agent_resume`** (compare counts;
      sid-less `jcode --resume` = FAIL, that is the Sep-28 bug signature).
- [ ] **7.4 Phantom slots absent:** slots **5/6/9** carry no `agent_session`;
      no `probe_*` anywhere in `session.json` after restore.
- [ ] **7.5 Pacing visible:** consecutive resume-fire timestamps in the server
      log are spaced by `startup_per_agent_delay_ms = 150` (±clock jitter) —
      a burst with ~0 gap = FAIL (see Abort criteria).

---

## Phase 8 — Rollback (restore misbehaves)

herdr writes a pre-cutover snapshot of `session.json` automatically on shutdown.

- [ ] **8.1 Locate the snapshot dir / name pattern:**
      `~/.config/herdr/session-snapshots/` — files named
      `session-<20-digit nanos-epoch>-<server-pid>-<n>.json`
      (e.g. `session-000000000000000000001790969158730900000-1266-0.json`).
- [ ] **8.2 Pick the newest snapshot by mtime at/before the cutover stop.**
      If unsure which is pre-cutover, STOP and compare contents (the pre-cutover
      one contains the phantom `probe_*` refs; the post-cutover one does not).
- [ ] **8.3 Rollback:**
      ```bash
      herdr server stop                                     # graceful
      cp ~/.config/herdr/session.json ~/.config/herdr/session.json.failed-cutover.bak
      cp ~/.config/herdr/session-snapshots/session-<chosen>.json ~/.config/herdr/session.json
      nohup "$HOME/.local/bin/herdr" server >/dev/null 2>&1 &
      ```
- [ ] **8.4 Verify:** server running; `session.json` matches the chosen snapshot;
      panes restore as before. Known cost: phantom `probe_*` refs return
      (they were present pre-cutover) and must be purged again at a later cutover.

---

## Abort criteria (stop immediately if any fire)

1. **Phase 2.3 dry-run diff ≠ expected** (REMOVE lines outside slots 5/6/9,
   `agent_resume` after-count ≠ 13, `agent_session` after-count ≠ baseline − 3,
   or any `REFUSED`/`WARN`, or rc ≠ 0) → abort **before** Phase 3.
2. **Cutover guard refuses** (`--apply` without a stopped server; byte
   round-trip not byte-identical; label/value mismatch; `--force` used or
   suggested outside dry-run-on-copy) → do not force; abort and re-investigate.
3. **Server fails to start** after 4.1 (socket never appears / `herdr pane list`
   fails) → rollback via Phase 8.
4. **`agent_resume` coverage drops below 13/13 after start** (entries actively
   cleared, `state.rs:568`) → run the Phase 5 prune-watch loop (stop → re-apply
   → start), max 3 attempts; if it drops again after the third attempt, abort
   and report blocked **with the observed counts** (full TOTALS/verdict/exit
   line of the failing run) — churn beyond control, root-cause `state.rs:568`
   first.
5. **`--apply` refuses (REFUSED / rc=2) or calf's slot `"3"` `agent_resume` is
   absent from `session.json` after Phase 3** → do not start the server
   (Phase 4); do not proceed to Phase 7; re-investigate per the Appendix A
   evidence and the P0 pointers.
6. **Restore fires more resumes than panes with `agent_resume`** (bare
   `jcode --resume` storm, >N fires where N = `agent_resume` pane count) → abort,
   this is the exact regression the project fixes.
7. **Resume fires are not spaced by ~150 ms** (bunched bursts with ~0 gap) →
   abort; config (`startup_per_agent_delay_ms`) not in effect.
8. **Phantom `probe_*` refs reappear** after restore → abort; the transaction
   did not hold or server re-persisted stale state.
9. **A pane's sid is undiscoverable and absent from `session.json`** → STOP,
   report blocked. **Never guess a sid.**
10. **Live data loss suspicion** (panes gone, snapshot missing) → stop all
    activity, preserve current `session.json` as `.bak`, escalate to operator.

---

## Appendix A — why the live "immediate re-inject" phase was abandoned

The former execution order — stop → purge → start → **immediate re-inject** via
`herdr pane report-agent` → verify — was DISPROVEN and replaced by rev2's
single server-stopped file transaction. Evidence (read 2026-10-02,
coordinator-verified):

- `~/.jcode/scratch/gaps_filled.md` — live `report-agent` writes are
  asynchronous/batched: the CLI returns **rc=0 with EMPTY output** and entries
  land minutes later or never; the live count oscillated **4→5→8→7**; the
  server's own rewrite **pruned** `w7:p17` (Fabric) written 22:37 at 22:42; and
  two panes (`w7:p1B` Cons, `w7:p19` Khostty/calf) **never persisted at all**
  despite rc=0.
- `~/.jcode/scratch/diag_report_agent.md` §4 — `state.rs:568`
  `set_reported_resume(None)` fires on agent-detection events and **wipes
  reported resumes post-write**, independent of any injection attempt.
- `report-agent` is a socket-API client and **requires a running server** — and
  the running server is exactly what batches and prunes the writes. The only
  reliable write path is therefore **file-level with the server STOPPED**, so
  the file is correct at the exact instant of restart: no pruning window, no
  async race, no rc=0 ambiguity.

No live-injection step remains in this runbook. Do not reintroduce
`herdr pane report-agent` into the cutover path; the transaction in
`cutover_session.py` (purge + gap-fill, guards (a)–(e)) is the only write path.
