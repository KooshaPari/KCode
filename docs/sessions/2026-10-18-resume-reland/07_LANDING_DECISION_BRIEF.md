# Landing Decision Brief — Open KCode PRs

Session: `2026-10-18-resume-reland` (continuation)
Compiled 2026-10-19 by jcode. Decision is the OPERATOR's.

All evidence below was produced by re-running commands in this session, with
`export TMPDIR=/tmp/kcode-neutral-tmp` on every cargo chain, in a detached
worktree at `origin/master` (`b9886143c`), which was removed afterward.

## RETRACTIONS made this session

1. **I falsely announced mid-session that this was "actually 8 warnings." That
   announcement was my own miscount. The real number is 7.**

   Re-deriving it from the tool output, master emits exactly these 7
   `dead_code` warnings in `update_product.rs`: `GITHUB_REPO`, `update_repo`,
   `update_repo_for`, `fork_update_disallowed_error`, `resolve_update_repo`,
   `is_inside_git_repo`, `log_disabled`. #127 adds exactly 7 `#[allow(dead_code)]`
   attributes covering exactly those 7 items. After merging #127:
   `EXITCODE=0`, zero warnings in that file. The PR title is accurate.

   So the original claim held, and the intervening claim I made (">8") is
   retracted. I record it here because an uncorrected wrong announcement is
   exactly the failure mode the operator asked me to surface, even when the
   final answer matches the first one.

2. **Retracting the general sloppiness.** Mid-session I twice ran imprecise
   greps (`grep -oE` over a live `gh pr diff`, and an awk with a broken
   expression) and drew a conclusion off each before re-checking. Both
   conclusions were discarded on re-derivation, but they should never have
   been stated as findings.

## Evidence per PR

### PR #127 — `fix(jcode-app-core): suppress 7 dead_code warnings on update_product public API`
- Scope: `crates/jcode-app-core/src/update_product.rs`, +7/-0. Pure `#[allow(dead_code)]` attributes. No behavior change.
- Verified on master: 7 dead_code warnings on the listed items.
- Verified after merge: `cargo check -p jcode-app-core` → `EXITCODE=0`, 0 warnings in that file.
- `cargo test -p jcode-app-core --lib update_product` → `EXITCODE=0`, 8 passed / 0 failed.
- Rationale is sound: these are fork-decollision public API surface, uncalled by any build variant. Silence is correct over deletion.
- CI: `Format` FAIL, `Build & Test` FAIL on all 3 OS — same pre-existing pattern as #125/#128.

### PR #132 — `fix(tui): 3 leftover kcode/jcode decollision mismatches`
- Scope: 3 files, +8/-8. Pure string rename `jcode` → `kcode`: one path fallback, two test assertions, one notification title.
- Verified after merge-with-others: `cargo fmt --all -- --check` → `EXITCODE=0`.
- `cargo test -p jcode-tui --lib turn_notify` → `EXITCODE=0`, 12 passed / 0 failed.
- `cargo test -p jcode-tui --lib title_includes` → `EXITCODE=0`, 1 passed.
- `cargo test -p jcode-tui --lib build_resume_command` → `EXITCODE=0`, 2 passed.
- Self-consistent: code string and its assertions both renamed together. Low risk.
- CI: `Format` FAIL, `Build & Test` FAIL on all 3 OS.

### PR #129 — `fix(ci): clear repo-wide rustfmt drift + env! macro in cli_exit_status`
- **The rustfmt claim is TRUE and fully verified.**
  Master `b9886143c` with CI's exact command `cargo fmt --all -- --check`:
  `EXITCODE=1`, 9 `Diff in` hunks across 7 distinct files.
  Affected files: `jcode-app-core/examples/print_build_info.rs`,
  `jcode-base/tests/mcp_xj07_failure_modes.rs`,
  `jcode-core/examples/smoke.rs`, `jcode-core/src/stdin_detect_tests.rs`,
  `jcode-harness-api-server/src/translate_tests.rs`,
  `jcode-herdr/examples/round_trip_state.rs`, `jcode-tui/src/tui/mod.rs`.
  After merging #129: `EXITCODE=0`, zero hunks, zero drift.
- Note: the 9th file `crates/jcode-harness-api/src/sockets.rs` is **not** pre-existing
  drift; it is a live rustfmt normalization of let-chains. Still legitimate.
- **The `env!` change is NOT a fix and should be called out.** `tests/cli_exit_status.rs`
  referenced `CARGO_BIN_EXE_jcode`, but the root binary is **`kcode`**. Cargo
  provides `CARGO_BIN_EXE_kcode` (probed at runtime: the set is
  `CARGO_BIN_EXE_kcode`, `CARGO_BIN_EXE_kcode-harness`,
  `CARGO_BIN_EXE_memory_recall_bench`, `CARGO_BIN_EXE_mermaid_side_panel_probe`,
  `CARGO_BIN_EXE_session_memory_bench`, `CARGO_BIN_EXE_test_api`,
  `CARGO_BIN_EXE_tui_bench`).
  - Before (master, `env!`): **compile error** —
    `error: environment variable CARGO_BIN_EXE_jcode not defined at compile time`,
    `error: could not compile kcode (test "cli_exit_status") due to 1 previous error`.
  - After (`std::env::var`): **compiles, then all 4 tests fail at runtime** with
    `CARGO_BIN_EXE_jcode: NotPresent`; `EXITCODE=101`; 0 passed / 4 failed.
  - Net: it converts a hard build failure into a runtime test failure. It does
    not make the test pass. The real fix is `CARGO_BIN_EXE_kcode`.
  - **Impact is nil for CI**: the `build` job only compiles/runs `--lib --bins`
    and targeted per-crate cohorts. Root integration tests under `tests/` are
    never compiled or run by CI, which is why this never appeared as a red check.
    The red `Build & Test` is the pre-existing `jcode-tui` lib failures.
  - **Ambiguity resolved, no longer an assumption.** Root `Cargo.toml` reads
    `name = "kcode"`, `autobins = false`, and the explicit `[[bin]]` list
    contains `kcode` (path `src/main.rs`) plus `test_api`, `kcode-harness`,
    `session_memory_bench`, `memory_recall_bench`,
    `mermaid_side_panel_probe`, `tui_bench`. **There is no `jcode` bin target
    at all.** The runtime probe's set matches this list exactly. So
    `CARGO_BIN_EXE_jcode` is never set, under any invocation.
  - A complete one-line fix exists and is not in #129: use `CARGO_BIN_EXE_kcode`.
  - This is why #129's `Format` check PASSES while #127/#132 fail it.

## CI status (all three)

Same pre-existing cause as #125/#128, already root-caused earlier this arc:
`Build & Test` fails on all 3 OS runners. The exact count, re-verified in the
extended pass, is **27 distinct `tui::` test failures** on baseline (I
previously wrote 25; 27 is correct). 0 non-tui failures. Not caused by any of
these PRs. `Format` fails on #127 and #132 for unrelated repo-adjacent drift;
#129 is the PR that clears it. Master has no branch protection, so these gates
are advisory only.

## Interaction check

All three merge into `master` with **no conflicts**. Stacking #129 + #127 + #132
in one scratch branch: `cargo fmt --all -- --check` → `EXITCODE=0`.

## Non-draft, mergeable summary

Updated verdicts after the extended second pass (see below).

| PR | Scope | Verdict on technical merit | Risk |
|----|-------|---------------------------|------|
| #127 | dead_code silences, +7/-0 | Verified: 7/7 warnings silenced, tests pass, 0 regressions | minimal |
| #132 | jcode->kcode strings, +8/-8 | Verified: fmt clean, assertions renamed with code, fixes 3 tests | minimal |
| #129 | rustfmt drift, 9 files | Verified: repo-wide drift real and fully cleared. Split: land rustfmt half, revert/fix the `env!` half | minimal |
| #124 | Windows keyboard fix + CI migration, +513/-351 | **LAND.** Fixes 25/27 pre-existing tui failures, 0 regressions, clippy `--lib` EXITCODE=0, fmt=0 | low code risk, high ops surface |
| #107 | draft on upstream jcode 0.98.1 | **DO NOT MERGE.** 2,808 commits diverged, 1,658 conflicts, would revert decollision | blocked |

## Drafts / large PRs (verdict updated in the extended pass)

- **#124** `fix(tui): keep Windows keyboard negotiation on native console input (cherry-pick of #97)`
  — `isDraft=false`, +449/-277 as reported by GitHub but +513/-351 as merged,
  66 files, `mergeable=MERGEABLE`. **No longer "needs verification" — see the
  extended pass. It is now the recommended land.**
- **#107** `Native wake receipts on Jcode 0.98.1 source baseline`
  — draft, +654/-1, 14 files, base is `codex/native-wake-base-1c7426e`
  (upstream, not master). **Blocked: must be ported, not merged.**

## Operator call required

Landing is the operator's decision. After the extended second pass the
recommendation is:

- **Land #127 and #132 now.** Minimal risk, verified, 0 regressions.
- **Land #124.** Highest value in the batch. It fixes 25 of the 27 pre-existing
  tui failures. Confirm the `desk-linux` self-hosted runner is intended first,
  because it also migrates all 7 CI jobs off `ubuntu-latest` and removes the
  macOS and hosted Windows legs. If the operator wants that CI change
  separated, the keyboard fix alone should be split out.
- **Split #129.** Land the rustfmt half (verified: clears all drift); revert the
  `env!` half or change it to `CARGO_BIN_EXE_kcode`.
- **Do not merge #107.** Port it onto a `master`-based branch first.

---

# EXTENDED VALIDATION (second pass, 2026-10-19)

The first pass above answered "is each PR internally correct?". It did **not**
answer "does each PR cause no regression?" or validate #124/#107 at all.
This pass closes those gaps. Method: two detached worktrees at `origin/master`
(`b9886143c`) — one clean baseline, one with all three stacked — run under
identical conditions with `export TMPDIR=/tmp/kcode-neutral-tmp`. Both removed
afterward.

## RETRACTION: the failure count was wrong

I previously stated **25** distinct failing tui tests. The correct baseline
number is **27**. Re-derived by extracting test names from the rustc
`failures:` block, not by counting summary lines:

- Baseline `cargo test -p jcode-tui --lib -- --test-threads=4`:
  `2482 passed; 27 failed; 19 ignored`, `EXITCODE=101`, 64.22s
- Merged (#127+#132+#129): `2485 passed; 24 failed; 19 ignored`, `EXITCODE=101`

The "25" figure came from a CI log line and was an undercount. **27 is the
number to use.** (Also see PR #124 below, which cuts this to 2.)

## Regression test: baseline vs merged

Failure sets differenced with `comm`:

- **Fixed by the three PRs (3):**
  - `tui::app::helpers::helpers_tests::build_resume_command_uses_imported_jcode_session_for_claude_code`
  - `tui::app::helpers::helpers_tests::build_resume_command_uses_imported_jcode_session_for_codex`
  - `tui::app::turn_notify::tests::kitty_notification_is_one_completed_clickable_message`
- **Newly broken: ZERO.** No test fails only after the merge.

All three fixed tests are exactly the ones PR #132 touches. Net effect:
+3 passing, 0 regressions, 24 remaining failures all pre-existing.

## CI gate scripts, both trees

Exit codes **identical** on baseline and merged: `deps=0`, `wildcard=0` pass;
`warning`, `codesize`, `testsize`, `panic`, `swallow` fail. The PRs do not
flip any gate. Two output diffs, both improvements or neutral:

- **warning gate:** distinct rustc warnings drop **9 -> 2**. Exactly the 7
  `update_product` dead-code warnings are removed and **0 are added**. This
  independently reconfirms the "7" count from a second, stronger source than
  the earlier grep. The remaining 2 failures (`with_spawn_notice`, an unused
  import in `tui/mod.rs`) are pre-existing on master.
- **testsize gate:** `translate_tests.rs` is over budget and **grew** on both
  trees, but **less** on merged (`4028 -> 4033`) than baseline
  (`4028 -> 4037`), because #129's rustfmt compaction shrank the file.
- `codesize`, `panic`, `swallow`: byte-identical output on both trees.

## PR #124 — re-validated, and it is far more valuable than first assessed

`gh pr view` reports `mergeable=MERGEABLE`, which I initially took at face
value and then mis-flagged as conflicting (my merge used a bad ref and the
failure was my error, not a real conflict). Real merge into clean master:
**0 conflicts**, `HEAD=126a6586c`, 66 files, +513/-351.

**What it actually is: it bundles TWO things.**

1. A genuine, well-documented Windows keyboard fix. New file
   `crates/jcode-tui/src/tui/keyboard_protocol.rs` matches keyboard
   negotiation to the active input decoder: on Windows, crossterm 0.29 uses
   `ReadConsoleInputW`, so requesting kitty/modifyOtherKeys reporting can lose
   special keys. `write_mode` returns `Unsupported` for `Push` on native
   console and no-ops `Set`/`Pop`. Arguably the single cleanest fix in this
   whole batch.
2. **A complete CI-infrastructure migration.** All **7** `runs-on: ubuntu-latest`
   jobs move to `runs-on: [self-hosted, desk-linux]`; the `macos-latest` leg
   is deleted; hosted Windows CI is disabled. Cover comments note fork PRs
   never run on an unisolated persistent self-hosted runner, and
   `cargo install` steps are guarded with `command -v ... ||` for persistent
   `CARGO_HOME`. Touches `.github/workflows/{ci,freebsd-smoke,ios-testflight,
   require-issue,update-star-history}.yml` plus scripts and TS SDK tests.

**Effect on the repo's failing test suite — verified, not inferred:**

- `cargo test -p jcode-tui --lib -- --test-threads=4` on #124:
  `2510 passed; 2 failed; 19 ignored`, `EXITCODE=101`
- vs baseline `2482 passed; 27 failed`
- **Fixes 25 of the 27 pre-existing failures. Zero regressions.**
- New `crates/jcode-tui/tests/keyboard_protocol.rs`: 3 tests pass.
- `cargo test -p jcode-tui --lib keyboard`: 13 passed.
- `cargo fmt --all -- --check`: `EXITCODE=0`
- `cargo clippy -p jcode-tui --lib -- -D warnings`: **`EXITCODE=0` on #124 vs
  `EXITCODE=101` on baseline** — it fixes clippy failures too.

**Two failures remain, both unrelated to keyboard, both separate work:**

- `tui::app::helpers::helpers_tests::detected_resume_terminal_recognizes_handterm_term_program`
  — terminal-detection priority: got `Some("herdr")`, expected `Some("handterm")`.
- `tui::app::tests::test_tui_cerebras_paste_key_lifecycle_has_no_degraded_success_messages`
  — panics at `tests/state_model_poke_03.rs:947`.

**One clippy caveat, resolved.** `cargo clippy -p jcode-tui --all-targets -- -D warnings`
on #124 reports 3 errors (`unused import: cache_expected_warm`,
`variable does not need to be mutable`, `field pending_swarm_rename is never
read`). I first read these as newly introduced. **That was wrong.** They are
pre-existing warnings on master (confirmed at `tui/mod.rs:1070/1850`,
`remote_startup_input_02/part_01.rs:1822`, `ui_tests/mod.rs:159`). Baseline has
9 `-D warnings` errors but **aborts on `jcode-tui-mermaid` / `jcode-setup-hints`
/ `jcode-harness-api` before reaching `jcode-tui`**, which masks them. On the
identical `--lib` scope, baseline is `EXITCODE=101` and #124 is `EXITCODE=0`.

**Recommendation changes: land #124.** It is the highest-value PR in the
batch. The CI migration is a real operational change and the operator should
confirm the `desk-linux` self-hosted runner exists and is intended before
merging, but the code side is the best-tested change here.

## PR #107 — DO NOT MERGE (blocked, not a judgment call)

- Base branch `codex/native-wake-base-1c7426e` is **not** an ancestor of
  master and diverges at `752df77d3`, sitting **2,808 commits ahead**.
- The head commit's own `Cargo.toml` declares `name = "jcode"`,
  `version = "0.98.1"` — it is built on the **upstream** product baseline,
  whereas master is `name = "kcode"`, `version = "0.93.0-k0.1.0`.
- The PR's real delta vs its own base is only 14 files / +654/-1 (native wake
  receipts), so it **looks** tiny — but the delta relative to master is
  enormous because the histories diverged.
- Attempted merge into master: **1,658 conflicting hunks.**
- Merging it would revert the fork decollision.
- **The GitHub `mergeable=MERGEABLE` badge is misleading here.** It reflects
  GitHub's own merge-base logic, not what a real merge would do.

Concrete path forward if the operator wants this work: re-target the 14-file
delta onto `master` (or a `master`-based branch) as a fresh PR. It must be
ported, not merged.

