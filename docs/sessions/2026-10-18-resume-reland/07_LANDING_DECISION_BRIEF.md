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
`Build & Test` fails on all 3 OS runners with 25 distinct `tui::` test
failures, 0 non-tui. Not caused by any of these PRs. `Format` fails on
#127 and #132 for unrelated repo-adjacent drift; #129 is the PR that clears it.
Master has no branch protection, so these gates are advisory only.

## Interaction check

All three merge into `master` with **no conflicts**. Stacking #129 + #127 + #132
in one scratch branch: `cargo fmt --all -- --check` → `EXITCODE=0`.

## Non-draft, mergeable summary

| PR | Scope | Verdict on technical merit | Risk |
|----|-------|---------------------------|------|
| #127 | dead_code silences, +7/-0 | Verified: 7/7 warnings silenced, tests pass | minimal |
| #132 | jcode→kcode strings, +8/-8 | Verified: fmt clean, assertions renamed with code | minimal |
| #129 | rustfmt drift, 9 files | Verified: repo-wide drift is real and fully cleared | minimal, but see caveat |

## Drafts (not recommended for landing now)

- **#124** `fix(tui): keep Windows keyboard negotiation on native console input (cherry-pick of #97)`
  — now `isDraft=false`, +449/-277, 65 files, `mergeable=MERGEABLE`. Large surface, no verification performed this session.
- **#107** `Native wake receipts on Jcode 0.98.1 source baseline`
  — draft, +654/-1, 14 files, base is `codex/native-wake-base-1c7426e`
  (not master), head `codex/native-wake-receipts-20261016`. Feature work on a non-master baseline.

## Operator call required

Landing is the operator's decision. The three above are technically low-risk
and verified. #129 carries the caveat that one of its two changes is cosmetic
relocation rather than a fix. #124 and #107 need their own verification passes
before any call.
