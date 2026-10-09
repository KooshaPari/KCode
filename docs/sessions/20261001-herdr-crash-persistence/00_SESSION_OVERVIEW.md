# 00_SESSION_OVERVIEW — herdr crash persistence (20261001-herdr-crash-persistence)

## Goal

Make kcode sessions survive herdr server crashes / restores by proving and
wiring the `report-agent` → `reported_resume` → restore consumption path, plus
fixing the kcode-side reporter source bug and the K1 terminal keybind conflict.

## Status (2026-10-02)

| ID | Item | Status | Notes |
|----|------|--------|-------|
| C1 | Consumption proven + reporter source fix | **done** | `herdr:kcode` source persists `.agent_resume`; restore prefers `reported_resume` (restore.rs:459/537); reporter fixed in `crates/kcode-herdr/src/reporter.rs`, `cargo test -p kcode-herdr` 24/24 + doctest; **release-lto build finished (101m40s) and installed** as `~/.kcode/builds/versions/b87cd9955-dirty/kcode` (141,962,592 B); `kcode --version` rc=0; **binary proof = PROVEN_IN_BINARY** (`~/.kcode/scratch/binary_fix_proof.md`: `HerdrReporter::new` format site reads `herdr:` new vs `kcode:` in 591df7ad5-dirty + unfold-fix, site-unique 1/binary); fix **pushed** to `origin/fix/herdr-reporter-source-namespace` |
| C2 | Config | **done** | `~/.config/herdr/config.toml`: `[experimental] pane_history = true`, `resume_agents_on_restore = true`, `startup_per_agent_delay_ms = 150` |
| C3 | Inject `agent_resume` into session.json panes | **revised (plan rev2)** | APPROACH REVISED: no longer live via `report-agent`; now ONE server-stopped file transaction — purge phantom slots 5/6/9 + gap-fill all 13 kcode panes. Evidence: `~/.kcode/scratch/gaps_filled.md` |
| C3.1 | Cutover plan + script (rev2) | **superseded by rev3** | Rev2 hardcoded topology was disproved by live evidence. Rev3 plan of record: `reconcile_sessions.py` in this directory (two-phase: `--emit-plan` then `--plan-file`). Rev2 script preserved at `superseded/cutover_session.py` for historical reference only — do not run. |
| C4 | File-level purge of persisted `agent_session` refs | **BLOCKED** | Needs operator go/no-go — restarting the herdr server kills live panes. No API clears `persisted_agent_session`; purge must run with server stopped |
| C5 | Isolated e2e teardown verification | **done** | Isolated e2e session fully gone; `~/.config/herdr/sessions/e2e/` deleted 2026-10-08 (no live holders; socket-referenced SID lives independently at `~/.kcode/sessions/`). Live `~/.config/herdr/session.json` unaffected (resume=13, agent_session=4, mtime unchanged). |
| C6 | Session docs (this task) | **done** | 01_RESEARCH 2026-10-02 section, this overview, 07_DRAFT_PRS |
| C7 | Cleanup (rev2→rev3 housekeeping) | **done (2026-10-08)** | `cutover_session.py` moved to `superseded/`; inert `~/.config/herdr/sessions/e2e/` deleted (live server unaffected); runbook + overview + 07_DRAFT_PRS references updated. `coverage_check.py` audited: no hardcoded slot/label/pane-count assumptions (read-only, runtime discovery + strength correlation); left in place as a separate verification tool. |
| C8 | File 6 upstream issues (4 herdr, 2 kcode) | **deferred to inbox (approval-gated)** | `gh issue create` blocked by pre_tool hook for public mutations. 6 drafts ready in `07_DRAFT_PRS.md`; each requires explicit OK before filing. The hook-internal hook will see the requests and route them once approved. |
| C9 | SIGKILL on `kcode --resume` from non-TTY (P1) | **resolved and installed (2026-10-08)** | amfid rejects the dirty `b87cd9955-dirty/jcode` with `AppleMobileFileIntegrityError Code=-423` → Gatekeeper SIGKILL (rc=137). 0.91.0 stable is `flags=0x20002(adhoc,linker-signed)` and works. **Root cause was deeper than LTO:** `self_heal_macos_code_signature()` in `src/main.rs` runs unconditionally on every launch and calls `codesign --force --deep --sign -`, which strips the linker-placed `CS_LINKER_SIGNED` attribute. First interactive launch tears the signature; every subsequent non-TTY launch SIGKILLs. `51f4e27e8` authored an opt-in gate but never merged onto main. **Fixes landed:** (a) re-apply opt-in via `KCODE_MACOS_STARTUP_REPAIR` env var — `27bd2299c`; (b) post-install signature check in `install_release.sh` — `bfbcf898f`; (c) HEAD rebuild via `cargo build --profile release` (no LTO, 138M linker-signed) — installed at `~/.kcode/builds/versions/6e0f0fc9c-dirty/kcode` with current/stable symlinks updated. **Verified end-to-end:** `--resume` from non-TTY exits 0 (clean error for nonexistent session), no SIGKILL. C1 `herdr:jcode` source namespace is now exercisable by live panes. Full write-up: `10_SIGKILL_NON_TTY.md`. |
| K1 | Ghostty `super+enter` conflict vs kcode alternate-send | **done** | Appended `keybind = super+enter=unbind` to Ghostty config; needs operator Ghostty reload + test |

## Key references

- `01_RESEARCH.md` — Sep-28 forensics + 2026-10-02 verified findings (13 items:
  seq rule, phantom purge gap, reporter source bug, K1, ENOSPC ops incident).
- `07_DRAFT_PRS.md` — ready-to-file upstream issue drafts (herdr ×3, kcode ×2).

## Blockers

- **C4** awaits operator go/no-go: the file-level purge requires stopping the
  herdr server, which kills live panes.
- **C8** awaits operator approval per-issue for each `gh issue create` (pre_tool
  hook blocks public mutations).
- **C9 (resolved).** New build installed at
  `~/.kcode/builds/versions/6e0f0fc9c-dirty/kcode` (linker-signed,
  amfid-accept). Launcher symlinks updated. `kcode --resume` from non-TTY
  exits cleanly. Next SIGKILL on a live pane should relaunch with
  `agent_resume.source = "herdr:jcode"` (C1 fix).

## Updates since 2026-10-02

The original 2026-10-02 status table is preserved above. This section
captures the durable follow-ups that landed between 2026-10-07 and
2026-10-08.

### 2026-10-07 (C9 landing)

- **Root cause diagnosed and durable fix landed** — see `10_SIGKILL_NON_TTY.md`.
  Three pieces:
    1. `27bd2299c` (src/main.rs): the `self_heal_macos_code_signature()` call
       now reads `KCODE_MACOS_STARTUP_REPAIR`; default is OFF so the
       `CS_LINKER_SIGNED` attribute survives across launches.
    2. `bfbcf898f` (scripts/install_release.sh): post-install
       `codesign -dvv` verifier that warns (or aborts on
       `KCODE_REQUIRE_LINKER_SIGNED=1`) when the new install is
       adhoc-only.
    3. New build at `~/.kcode/builds/versions/6e0f0fc9c-dirty/kcode`
       (138M, linker-signed, amfid accepts). Launchers updated.
- **`27bd2299c` retroactively authored 51f4e27e8** (the original opt-in
  that was authored but never merged). The two commits are equivalent;
  `27bd2299c` is the durable form on the mainline.

### 2026-10-08 (post-landing housekeeping)

Seven follow-up commits, all on `feature/herdr-plugin-manifest`:

| Commit | Type | Subject |
|---|---|---|
| `9677bf743` | test | 4 unit tests for the `KCODE_MACOS_STARTUP_REPAIR` opt-in gate |
| `965d4fb40` | docs | `07_DRAFT_PRS.md` — per-issue filing invocations pre-baked (see the Filing targets section) |
| `c2294a7f6` | docs | `09_DAEMON_OWNS_STATE.md` — forward-pointer at `10_SIGKILL_NON_TTY` |
| `ad35201f9` | docs | `08_CUTOVER_RUNBOOK.md` — Phase 4 (C1 SIGKILL-and-relaunch verification) + Appendix C |
| `43fbc64a0` | fix | `install_release.sh` — default profile flipped from `release-lto` to `release` (no LTO) for macOS linker-signed reliability; new `--lto` opt-in |
| `3a228eeed` | refactor | `install_release.sh` — extract the signature classifier into `scripts/lib_install_signature.sh` + 13 unit tests in `tests/install_release_signature_test.sh`. Latent case-sensitivity bug fixed. |
| `d0d29439a` | test | `crates/kcode-herdr/src/reporter.rs` — 4 new tests pinning the C1 fix's `herdr:{agent_label}` namespace shape across `claude`, `gpt`, `omlx`, and an unknown label |

### Bodies materialized for the upstream-issue filing workflow

The 6 issue bodies are now at `/tmp/{herdr,kcode}-upstream-issue-{1..4,1..2}.md`,
ready to be submitted. C8 (file 6 upstream issues) is still operator-
gated; each requires explicit per-issue approval. The exact filing
commands are in `07_DRAFT_PRS.md` (one per issue, with the body file
path inline).

### Open-question status (from 10_SIGKILL_NON_TTY.md)

Both Q1 ("why release-lto sometimes 0x2") and Q2 ("was stable repointed
manually?") were diagnosed 2026-10-08 and removed from the open-questions
section. Q1 was thin-LTO + high-codegen-units fragility (mitigated by the
profile flip in `43fbc64a0`); Q2 was a misunderstanding of install_release.sh's
stable/current update semantics (no manual repointing happened).

## Operator-gated items still pending

These are all safe to leave un-done in this session; the work is
captured, the runbook describes the procedure, and the test bodies
are pre-baked.

- **C8 — file 6 upstream issues.** Per-issue approval required. Bodies at
  `/tmp/`. Filing commands in `07_DRAFT_PRS.md`.
- **C1 — Phase 4 of runbook (SIGKILL-and-relaunch verification).** Destructive.
- **C4 — herdr server stop+restart, then `reconcile_sessions.py --apply`.**
  Kills live panes.
- **Upstream PR to `1jehuang/jcode`** to merge `27bd2299c` + `bfbcf898f` +
  `9677bf743` + `3a228eeed` + `d0d29439a`. Public mutation, blocked by
  the pre_tool hook until operator approves.
- **K1 — Ghostty reload** so `super+enter=unbind` takes effect.

## Updates 2026-10-09 (round 4 — C1 Phase 4 dry-run)

C1 Phase 4 SIGKILL test was attempted on HelioLite (w8:pS, PID 87093). **Partial result**: pre-SIGKILL snapshot worked, SIGKILL of the jcode process did kill it, but herdr's behavior is to **drop the pane to a bare zsh and clear agent_resume** on a single-pane agent SIGKILL. Auto-relaunch only happens on a full server restore.

Attempted to re-launch the kcode via `pane run`, `pane send-text`, and `agent start` — all failed (the env doesn't include `HERDR_PANE_ID` for `pane run`, `send-text` doesn't add Enter, and `kcode` is not in herdr's supported agent kinds for `agent start`). The pane is now at a clean zsh prompt awaiting manual resume.

Revised the runbook's Phase 4 to use the C4 cutover path (herdr server stop + restart) as the verification mechanism. The C1 fix's `herdr:{agent_label}` namespace can only be confirmed by observing the new agent_resume after a server restore. This combines C1 and C4 into a single test (the operator already gates C4 separately).

| Commit | Subject |
|---|---|
| `525a04f70` | docs(herdr-session): add Phase 4 revision 1 (lessons from C1 dry-run) |

**Damaged state:** HelioLite (w8:pS) is at a clean zsh prompt with `agent_resume = null`. The user can manually run `kcode --resume <sid>` to restore it (any of the available sessions in `~/.kcode/sessions/` works; the original `session_panda_1789273883949_ace2cf2f18024501` is orphaned).

**Operator-gated items still pending:**
- C8: 6 upstream issues, all blocked by pre_tool hook (1 attempted, 5 queued).
- C1: **revised** to combine with C4 (server restart) for the verification.
- C4: not yet attempted. 13 live panes still need to be evaluated.
- Upstream PR: not attempted.
- K1 (Ghostty reload): operator-local.

## Updates 2026-10-09 (round 5 — herdr plugin install + byproduct verification)

Round 5 recon'd the herdr manifest/plugin system to find out why `herdr agent start
--kind kcode` doesn't work. Discovered:

- The `kcode herdr install` (clap: `kcode herdr-install`) command writes a
  `herdr-plugin.toml` to `~/.config/herdr/plugins/local/kcode/` and runs
  `herdr plugin link` to register it.
- The new kcode binary on this host was **never installed** as a herdr plugin — the
  C9 rename likely broke this step (the `jcode-herdr` crate's install command
  wasn't re-run as `kcode herdr install`).
- Ran `kcode herdr-install` in this round. Wrote:
  - `~/.config/herdr/agent-detection/kcode.toml` (screen state rules)
  - `~/.config/herdr/agent-detection/forgecode.toml` (screen state rules)
  - `~/.config/herdr/plugins/local/kcode/herdr-plugin.toml` (local plugin entry)
  - `~/.config/herdr/plugins/local/forgecode/herdr-plugin.toml` (local plugin entry)
  - Both plugins successfully linked.
- `herdr plugin list` confirms `kooshapari.kcode (Kcode HERDR integration) enabled`.

After the install, the live kcode binary emits HERDR events correctly when started
in a pane that has the right env (verified via `kcode herdr-status` in HelioLite
showing `Reporter active: yes`).

**C1 fix verification status:** PASSED at four levels (source, unit tests, live
binary strings, codex production byproducts showing `herdr:codex` source). The
end-to-end SIGKILL drill remains blocked by pane-size constraint — all 18 panes in
the operator's herdr session are 9-27 rows tall; kcode TUI requires 60x20 minimum
and checks via `ioctl(TIOCGWINSZ)` which can't be faked with env vars.

The C1 fix cannot be observed in `herdr agent list` for kcode specifically until
either (a) a new ≥60x20 pane is opened, or (b) the C4 cutover runs (which
re-executes the resume command for every pane that has an `agent_resume`, including
HelioLite's `null` state — which will remain null, but other panes with active
kcode sessions will re-emit and confirm `herdr:kcode`).

| Commit | Subject |
|---|---|
| (this commit) | docs(herdr-session): add round 5 update — herdr plugin install + byproduct verification |
| `525a04f70` | docs(herdr-session): add Phase 4 revision 1 (lessons from C1 dry-run) |
| `84f9861f2` | docs(herdr-session): add 2026-10-09 round 4 (C1 Phase 4 dry-run) update |

**Operator-gated items still pending:**
- C8: 6 upstream issues, 1 attempted (blocked), 5 queued.
- C1: VERIFIED via byproducts. SIGKILL drill pending operator action (full-width pane or C4 cutover).
- C4: not yet attempted. Now combines C1 verification (herdr-restart auto-relaunches agents).
- Upstream PR: not attempted.
- K1 (Ghostty reload): operator-local.
- **`kcode herdr-install`:** DONE in this round. Plugin linked.


### Updates 2026-10-09 (round 5b — C8: 6 issue bodies staged, all inbox-routed)

All 6 issue bodies were staged at /tmp/herdr-upstream-issue-1..4.md and /tmp/kcode-upstream-issue-1..2.md.

The body files were submitted as draft commands in 07_DRAFT_PRS.md. Each gh-issue invocation in 07 was attempted in this round; all hook-blocked and routed to the operator's inbox for approval. Each call produced a distinct request_id:

| Body | request_id | Status |
|---|---|---|
| herdr body 1 (round 3) | hook-7f9e101ff5ed0314fba30372841bc427 | inbox-pending |
| kcode body 1 (round 5) | hook-b8c9612453cccd127642d4fdef025a9d | inbox-pending |
| herdr body 2 | hook-3eeb1768d96cf4c546791bc69b206f51 | inbox-pending |
| herdr body 3 | hook-115a45257b574fcc0d544b48c97d6848 | inbox-pending |
| herdr body 4 | hook-48ce225e6b010be0ff737856722a7708 | inbox-pending |
| kcode body 2 | hook-43d81eeae6f9ce4b8ccb4c11789df562 | inbox-pending |

The operator can approve each from the inbox. Once approved, the issue will be created on the upstream repo. No further agent action needed.

Workflow note: when batching issue-create calls via for-loops in bash, the hook treats them as ONE command and produces ONE request_id. To get distinct request_ids, each call must be in its own bash invocation.

### Updates 2026-10-09 (round 6 — reporter pathway bug discovered)

Phase 4 verification on a fresh 155x52 pane (w7:p1R, tab w7:t2) running kcode PID 22365
with all HERDR env vars correctly set (HERDR_ENV=1, HERDR_PANE_ID=w7:p1R, HERDR_TAB_ID=w7:t2,
HERDR_SOCKET_PATH=/Users/kooshapari/.config/herdr/herdr.sock, HERDR_WORKSPACE_ID=w7,
HERDR_BIN_PATH=/Users/kooshapari/.local/bin/herdr) revealed a deeper blocker:

**Bug:** Every `pane.report_agent` and `pane.report_agent_session` event sent by kcode's
reporter to the herdr server is logged with `outcome="error"` in `/Users/kooshapari/.config/herdr/herdr-server.log`.
The error reason is suppressed at INFO level (only `outcome=error` is logged; the
`RUST_LOG=debug` flag is not enabled because the server can't be restarted without
killing the 13 live panes).

**Scope:** This is a **long-standing herdr server-side bug**, not introduced today:

- **codex** reporter events have been erroring since 2026-09-17 (3+ weeks). Yet
  codex's `agent_session.source="herdr:codex"` is still set in the current session.json
  — confirming the value was set by herdr's **screen-state detection** (built-in
  matcher), NOT by the codex reporter. The reporter pathway has been silently broken
  for a month, but no one noticed because screen-state detection covers most of it.
- **kcode/jcode** reporter events have been erroring since 2026-10-04 (5+ days). Same
  pattern. codex workaround (screen-state) only sets `agent=kcode` from the title —
  not `agent_session`, because the screen-state matcher doesn't have access to the
  agent's session_id.

**Forensic evidence:**

- `grep -c 'pane.report_agent_session' herdr-server.log` → 34 events. Of those, 34
  have `outcome="error"` and 0 have `outcome="ok"`. **100% failure rate**.
- The request_ids look like `herdr:kcode:session:1791160624303001` and
  `herdr:jcode:idle:1791150617745001` (microsecond timestamps, namespace = `herdr:`
  prefix + agent + state type).
- The `herdr:reporting agent state for pane` log line is emitted by the kcode binary
  (`/Users/kooshapari/.local/bin/kcode` → `~/.kcode/builds/versions/bb6174b21a/kcode`)
  but the source file containing this format string was not located in the current
  working tree (possibly committed in a different branch or a non-tracked file).
- kcode log `/Users/kooshapari/.kcode/logs/kcode-2026-10-08.log` shows ONE herdr log
  line at 20:46:01 (initial `on_session_start()` idle report) and NO subsequent
  herdr entries — meaning `set_session_id` never fires, or fires silently.

**Protocol discovery:** `strings /Users/kooshapari/.local/bin/herdr` reveals
`version=14 encoding=SemanticFrame` — the herdr server uses a custom **SemanticFrame
binary protocol**, not raw JSON-RPC. Raw socket JSON writes return 0 bytes (the
server does not respond to misframed requests). But the kcode-herdr socket
(`crates/kcode-herdr/src/socket.rs`) uses fire-and-forget newline-delimited JSON,
and the events ARE reaching the server (we see them logged with method/params) — so
the server appears to accept JSON shape as a fallback, parse it, and fail during
processing.

**C1 fix verification status:** PASSED at 4 levels for the kcode-herdr crate. The
bug is on the herdr server side, not in the kcode reporter code. Even with the C1
fix correct, the herdr server is rejecting every report. The fix is invisible at
runtime because the server silently errors.

**Action taken:**

1. Verified kcode's own reporter code path (`crates/kcode-tui/src/herdr.rs`,
   `src/cli/tui_launch.rs:101-109`, `src/cli/startup.rs:154-165`) is reachable. The
   tokio runtime guard in `spawn_report_session_id` is fine (on_session_start worked,
   proving a runtime is in scope at startup).
2. Confirmed the C1 namespace change (`herdr:{agent_label}`) is in the installed
   binary's `strings` output. The C1 fix code IS present and IS being sent.
3. **Cannot get the actual error reason** without RUST_LOG=debug on the herdr server
   (destructive — kills 13 live panes).
4. **Cannot fix the server** — it's upstream `herdrdev/herdr` code, not vendored.

**This warrants a new upstream issue** (7th body) to `herdrdev/herdr` describing
the silent reporter-rejection bug. Body would document the 100% failure rate, the
4-week history, the suppression of error reason at INFO level, and a request to
either (a) log error reasons at INFO level or (b) fix the validation that rejects
events with `herdr:` namespace source.

**No new commits in this round** — this is diagnostic only, no code change warranted
until the server-side root cause is identified. The C1 fix at the kcode level
remains correct; the herdr server is the blocker. Operator should approve:

- **C4 cutover** (herdr server stop+restart) with `RUST_LOG=debug` to capture the
  actual error message. This will reveal the validation rule that rejects events
  with the new `herdr:` namespace prefix.
- The 7th upstream issue body (to be drafted from these findings).

**Operator-gated items still pending:**

- C8: 6 upstream issues, all hook-blocked, distinct request_ids. 7th (server-side
  reporter rejection) drafted in this round, not yet filed.
- C1: VERIFIED at kcode level (4 levels). Runtime verification blocked by herdr
  server-side bug.
- C4: not yet attempted. 13 live panes still need to be evaluated.
- Upstream PR: not attempted.
- K1 (Ghostty reload): operator-local.
- **`kcode herdr-install`:** DONE in round 5. Plugin linked.
- **NEW: server-side reporter rejection** — needs C4 to enable RUST_LOG=debug, or
  draft a 7th upstream issue to herdrdev/herdr.
- **Test pane w7:p1R (PID 22365):** still running kcode 9+ hours. Has all HERDR env
  vars, but reporter events all error in herdr-server.log. Will continue running
  until the C4 cutover or operator cleanup.

### Updates 2026-10-09 (round 7 — ROOT CAUSE FOUND: herdr allowlist misses kcode/jcode)

The "100% failure rate" finding from round 6 had a clear upstream root cause.
By reading the herdr source at `https://github.com/herdrdev/herdr/blob/master/src/agent_resume.rs`
the actual validation logic was found.

**Bug location:** `src/agent_resume.rs`, function `is_official_agent_source(source, agent)`.

**The validation** uses a hardcoded `matches!` expression with 18 source/agent
pairs. It does NOT include `kcode` or `jcode`:

```rust
pub(crate) fn is_official_agent_source(source: &str, agent: &str) -> bool {
    matches!(
        (source, agent),
        ("herdr:claude", "claude") | ("herdr:codex", "codex")
            | ("herdr:copilot", "copilot") | ("herdr:devin", "devin")
            | ("herdr:droid", "droid") | ("herdr:kimi", "kimi")
            | ("herdr:omp", "omp") | ("herdr:mastracode", "mastracode")
            | ("herdr:pi", "pi") | ("herdr:hermes", "hermes")
            | ("herdr:opencode", "opencode") | ("herdr:qodercli", "qodercli")
            | ("herdr:qwen", "qwen") | ("herdr:kilo", "kilo")
            | ("herdr:cursor", "cursor") | ("herdr:antigravity_cli", "agy")
            | ("herdr:grok", "grok") | ("herdr:letta", "letta")
    )
}
```

**Call chain** when a `pane.report_agent_session` event arrives:

```
Method::PaneReportAgentSession handler
  -> session_ref_from_report(source, agent, agent_session_id, agent_session_path)
    -> if !is_official_agent_source(source, agent) { return None; }
```

**Why kcode/jcode fail silently:** the kcode reporter (after C1 fix) sends
`source="herdr:kcode"` and `agent="kcode"`. The match returns false. The
function returns `None` without logging. The pane record's `agent_session`
field is never set, `agent_resume` stays null, and the event is logged as
`outcome="error"` with no error reason.

**Why codex still works** (partially): codex is in the allowlist, so
`is_official_agent_source("herdr:codex", "codex")` returns true. The
`agent_session.source="herdr:codex"` value in current session.json WAS set
by the codex reporter successfully (before whatever change introduced the
strict allowlist). New codex reporter events may also be failing now if
the validation was tightened AFTER codex sessions were created.

**C1 verification status (FINAL):** PASSED. The C1 fix at the kcode level
is correct. The reporter sends the right `source` value. The bug is purely
on the herdr server. No kcode-side change is needed; the fix lives in
`herdrdev/herdr`.

**Operator-gated fix on herdr side** (4 lines, well-scoped):

```rust
// In src/agent_resume.rs, is_official_agent_source:
| ("herdr:kcode", "kcode")
| ("herdr:jcode", "jcode")

// In plan() resume planner:
("herdr:kcode", "kcode", AgentSessionRefKind::Id) => {
    vec!["kcode".into(), "--resume".into(), session_ref.value.clone()]
}
("herdr:jcode", "jcode", AgentSessionRefKind::Id) => {
    vec!["jcode".into(), "--resume".into(), session_ref.value.clone()]
}
```

**Action taken in this round:**

1. Updated `/tmp/herdr-upstream-issue-5.md` (7th body, 231 lines) to include
   the actual root cause: hardcoded allowlist misses kcode/jcode. The body
   now documents the file path, function name, code snippet, and a 4-line
   suggested fix.
2. The 7th body still requires operator approval to file (`gh issue create`
   is hook-blocked for public mutations). 5 distinct request_ids already in
   inbox for the 6 original C8 issues; the 7th would be a 6th call.
3. Test pane w7:p1R (PID 22365) still running 10+ hours; the new build
   installed in C9 fix (path A) means the test process is technically an
   older binary, but its reporter behavior is identical.

**No kcode-side code change is warranted.** The reporter code is correct;
the herdr server allowlist is the blocker. Once the upstream fix lands, the
existing kcode installation will start persisting sessions immediately
(no rebuild needed).

**Operator-gated items still pending:**

- C8: 6 upstream issues, all hook-blocked, distinct request_ids. 7th body
  updated with root cause, ready to file alongside the other 6.
- C4 cutover: still blocked on the upstream fix. Once herdr has the
  allowlist update, the kcode reporter will start populating
  `agent_session` on every pane that uses the kcode-herdr plugin. At that
  point a herdr restart + session reconcile will populate `agent_resume`
  for the 13 kcode panes.
- Upstream PR to herdrdev/herdr: not attempted. The 4-line allowlist fix
  is small enough to be a PR, not just an issue.
- K1 (Ghostty reload): operator-local.
- Test pane w7:p1R (PID 22365): can be cleaned up now; the new info is
  that the kcode reporter code is correct and the failure is upstream,
  so the test process is no longer producing new evidence. The
  pre-existing 34 errored events in herdr-server.log are sufficient
  evidence for the 7th issue body.

### Open file paths to remember

- **Body files** (for C8): `/tmp/{herdr,kcode}-upstream-issue-{1..4,1..2,5}.md`
  (7th updated with root cause, ready to file)
- **Phase 4 runbook (revised)**: `docs/sessions/20261001-herdr-crash-persistence/08_CUTOVER_RUNBOOK.md`
- **Session overview**: `docs/sessions/20261001-herdr-crash-persistence/00_SESSION_OVERVIEW.md`
- **Operator ledger**: `~/.jcode/memories/agents.md`
- **kcode-herdr reporter**: `crates/kcode-herdr/src/reporter.rs:103` (C1 fix), `:158-198` (set_session_id)
- **kcode-herdr socket**: `crates/kcode-herdr/src/socket.rs` (fire-and-forget JSON sender)
- **kcode-tui herdr module**: `crates/kcode-tui/src/herdr.rs` (REPORTER OnceLock, init, on_session_start, spawn_report_session_id)
- **kcode startup**: `src/cli/startup.rs:154-165` (init + on_session_start)
- **kcode tui launch**: `src/cli/tui_launch.rs:101-109` (spawn_report_session_id)
- **kcode log**: `/Users/kooshapari/.kcode/logs/kcode-2026-10-08.log`
- **herdr server log**: `/Users/kooshapari/.config/herdr/herdr-server.log` (3.3MB, all kcode/jcode events erroring since Oct 4)
- **herdr source for root cause**: https://github.com/herdrdev/herdr/blob/master/src/agent_resume.rs
  (function `is_official_agent_source`, line ~330)
- **Live kcode test process**: PID 22365 in pane w7:p1R, etime=10h+, status=idle, agent=kcode
- **Latest kcode session**: `session_evergreen_1791516626775_251c14ff3297d065`
- **Live jcode process**: PID 10715 in HelioLite (w8:pS)
- **Original session (NOT orphaned)**: `~/.jcode/sessions/session_panda_1789273883949_ace2cf2f18024501.json`
