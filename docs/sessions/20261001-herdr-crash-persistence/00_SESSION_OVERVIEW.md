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
