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
| C3.1 | Cutover plan + script | **gates closed (awaiting runbook sync)** | `~/.kcode/scratch/cutover_session.py` (437L, PARSE_OK) + plan §4 rev2. Coordinator-verified dry-run: `agent_resume 5→13`, `agent_session 7→4`, bytes +1742, rc=0; guard (d) refused the live server (rc=2) so `--force` is copy-dry-run only. CLOSED: script, binary proof, build #2. OPEN: runbook 08 sync to rev2, operator go for C4 |
| C4 | File-level purge of persisted `agent_session` refs | **BLOCKED** | Needs operator go/no-go — restarting the herdr server kills live panes. No API clears `persisted_agent_session`; purge must run with server stopped |
| C5 | Isolated e2e teardown verification | **done** | Isolated e2e session fully gone; only inert `~/.config/herdr/sessions/e2e/` dir remains (no live holders, lsof empty). Report: `~/.kcode/scratch/c5_verify_report.md`; deleting that dir awaits operator ok |
| C6 | Session docs (this task) | **done** | 01_RESEARCH 2026-10-02 section, this overview, 07_DRAFT_PRS |
| K1 | Ghostty `super+enter` conflict vs kcode alternate-send | **done** | Appended `keybind = super+enter=unbind` to Ghostty config; needs operator Ghostty reload + test |

## Key references

- `01_RESEARCH.md` — Sep-28 forensics + 2026-10-02 verified findings (13 items:
  seq rule, phantom purge gap, reporter source bug, K1, ENOSPC ops incident).
- `07_DRAFT_PRS.md` — ready-to-file upstream issue drafts (herdr ×3, kcode ×2).

## Blockers

- **C4** awaits operator go/no-go: the file-level purge requires stopping the
  herdr server, which kills live panes.
