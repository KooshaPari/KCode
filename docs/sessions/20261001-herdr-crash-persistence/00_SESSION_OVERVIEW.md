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
