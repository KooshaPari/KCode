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
| #124 | Windows keyboard fix + CI migration, +514/-352 | Keyboard fix: LAND (best-tested change in the batch). Bundled CI migration: BLOCKED — targets an offline runner and disables 3 of 12 `ci.yml` jobs | low code risk, high ops surface *(see third pass)* |
| #107 | draft on upstream jcode 0.98.1 | **DO NOT MERGE.** 2,808 commits diverged, 357 conflicted files / 1,149 hunks, would revert decollation | blocked |

## Drafts / large PRs (verdict updated in the extended pass)

- **#124** `fix(tui): keep Windows keyboard negotiation on native console input (cherry-pick of #97)`
  — `isDraft=false`, **+514/-352**, 66 files, `mergeable=MERGEABLE`.
  ~~+449/-277 as reported by GitHub but +513/-351 as merged~~ *(both figures
  wrong; GitHub API says 514/352 — see Correction 4).*
  ~~"It is now the recommended land."~~ *(SUPERSEDED — see Correction 2/3: the
  bundled CI migration targets an offline runner and disables 3 jobs.)*
- **#107** `Native wake receipts on Jcode 0.98.1 source baseline`
  — draft, +654/-1, 14 files, base is `codex/native-wake-base-1c7426e`
  (upstream, not master). **Blocked: must be ported, not merged.**

## Operator call required

Landing is the operator's decision. After the extended second pass the
recommendation is:

- **Land #127 and #132 now.** Minimal risk, verified, 0 regressions.
- **#124** *(This bullet's conclusion was reversed in the third pass — kept
  for the record.)* ~~Highest value in the batch. It fixes 25 of the 27
  pre-existing tui failures. Confirm the `desk-linux` self-hosted runner is
  intended first, because it also migrates all 7 CI jobs off `ubuntu-latest`
  and removes the macOS and hosted Windows legs. If the operator wants that CI
  change separated, the keyboard fix alone should be split out.~~

  > **Third-pass correction:** the runner is **offline** (not merely "needs
  > confirmation"), and the CI change disables `windows-build-test`,
  > `powershell-syntax` and `setup-friction` outright. **Land the keyboard fix;
  > treat the CI migration as a separate, currently-blocked change.** Also: the
  > "27" and "25" figures are single-run samples (see the run-dependence note
  > below).
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
**0 conflicts**, `HEAD=126a6586c`, 66 files, ~~+513/-351~~ *(GitHub API says
`+514/-352` — see Correction 4; the 513/351 figure is wrong).*

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

   > **SUPERSEDED — see "Correction 2" and "Correction 3" below before relying
   > on this paragraph.** The Windows sentence here happens to be closer to the
   > truth than my later "Windows retained" correction, but the specifics of
   > *which* jobs are disabled, and the fact that the `desk-linux` runner is
   > **offline**, are both in the third-pass section. Do not merge on the strength
   > of this paragraph alone.

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
batch. ~~The CI migration is a real operational change and the operator should
confirm the `desk-linux` self-hosted runner exists and is intended before
merging, but the code side is the best-tested change here.~~

> **SUPERSEDED by the third pass.** The runner exists but is **offline**, and
> #124 disables 3 of 12 `ci.yml` jobs. The recommendation below now reads
> "land the keyboard fix, resolve the CI blockers first." This paragraph's
> "land #124" referred to the bundle, which is no longer the advice.

## PR #107 — DO NOT MERGE (blocked, not a judgment call)

- Base branch `codex/native-wake-base-1c7426e` is **not** an ancestor of
  master and diverges at `752df77d3`, sitting **2,808 commits ahead**.
- The head commit's own `Cargo.toml` declares `name = "jcode"`,
  `version = "0.98.1"` — it is built on the **upstream** product baseline,
  whereas master is `name = "kcode"`, `version = "0.93.0-k0.1.0`.
- The PR's real delta vs its own base is only 14 files / +654/-1 (native wake
  receipts), so it **looks** tiny — but the delta relative to master is
  enormous because the histories diverged.
- Attempted merge into master: ~~**1,658 conflicting hunks.**~~ *(CORRECTED: 357
  conflicted files / 1,149 conflict hunks. See Correction 1 below.)*
- Merging it would revert the fork decollision.
- **The GitHub `mergeable=MERGEABLE` badge is misleading here.** It reflects
  GitHub's own merge-base logic, not what a real merge would do.

Concrete path forward if the operator wants this work: re-target the 14-file
delta onto `master` (or a `master`-based branch) as a fresh PR. It must be
ported, not merged.

---

# THIRD VALIDATION PASS (2026-10-19, session-continuation)

This pass exists because the second pass above **contains errors**. It fixes
them, and it found a scope error serious enough to restate what is still open.

## Correction 1 — #107 conflict count was wrong

The second pass said "**1,658 conflicting hunks**." That number is wrong.
Measured from an actual `git merge` attempt:

- **357 conflicted files**
- **1,149 conflict hunks**

The root `Cargo.toml` conflict is confirmed at
`kcode 0.93.0-k0.1.0` (master) vs `jcode 0.98.1` (#107 head), so the
decollision-revert conclusion is unchanged and **#107 remains DO NOT MERGE**.
Only the arithmetic was wrong. I am not sure where 1,658 came from and cannot
reconstruct it as a sum of any quantity I can measure; treat it as a fabricated
number and use 357/1,149.

## Correction 2 (amended) — the runner exists but is **OFFLINE**

The second pass told the operator to "confirm the `desk-linux` self-hosted
runner exists and is intended," as if existence were an open question. That
caveat was wrong: the runner exists. **But I then compounded the error**: I
wrote it as "currently online." That is false.

`gh api repos/KooshaPari/KCode/actions/runners`, re-queried three times this
pass, returns exactly one runner:

- **`desk-kcode-1`**, Linux, X64, labels `self-hosted`, `Linux`, `X64`,
  `desk-linux`, `desk-kcode`
- **`status=offline`**, `busy=false` — on all three queries, not a transient
  blip I caught mid-restart.

**This is a blocking fact, not a footnote.** #124 migrates its CI jobs onto
`[self-hosted, desk-linux]`, and that runner is not running. If #124 lands as
written, those jobs will queue forever and never report. The operator must
either bring `desk-kcode-1` online before merging #124, or accept that the
migrated jobs are effectively silent until they do. Retracting "online."

Separately, capacity is still a real concern even once online: this is a
**single** runner and the migrated jobs carry `cancel-in-progress: true`, so
they serialize rather than run in parallel.

## Correction 3 (amended) — #124 DOES disable Windows CI. My "Windows is
## retained" correction was also wrong.

Pass 2 said "hosted Windows CI is disabled." In the third pass I flagged that
as an error and claimed Windows legs were retained. **That correction was
itself wrong.** The real state is messier than either of my two claims:

- **macOS leg: removed.** `os: [ubuntu-latest, macos-latest]` → `ubuntu-latest`
  in the `build-sweep` matrix.
- **`windows-build-test` — disabled.** In #124's post-image `ci.yml` the job at
  line 451 still exists but gains **`if: false`** (line 455). It never runs.
- **`powershell-syntax` — disabled.** Same treatment: `if: false` at line 708,
  with the comment *"Disabled with windows-build-test; PowerShell syntax is
  checked locally on the desk."*
- **`setup-friction` — disabled** by `if: false` as well.
- **`windows-cross-check`** keeps its name and survives, but moves
  `ubuntu-latest` → `[self-hosted, desk-linux]`.

So: **3 of the 12 jobs in `ci.yml` are disabled outright, the macOS leg is
removed, and 7 jobs move to the offline self-hosted runner.** Of my two
conflicting accounts, the original pass-2 claim ("Windows CI disabled") was
**closer to the truth**, and the pass-3 correction that supposedly fixed it was
the wrong one. Both are retracted; the table above is the verified state,
extracted from #124's post-image file rather than from the diff I had skimmed.

The reason I got this wrong twice is worth recording: I read the `runs-on`
lines and saw `windows-latest` still present, and stopped there. The job was
disabled by a different mechanism (`if: false`) two lines away, which I did not
check.

## Correction 4 — #124 size stat was wrong in two places

The second pass carries two conflicting figures for #124: "+449/-277 as
reported by GitHub but +513/-351 as merged" and the table says "+513/-351".
Both halves of that are unreliable. GitHub's API, queried this pass, reports
**`+514/-352`** across **66 files**. Use **514/352**.

The "+449/-277" number appears nowhere in the GitHub API response and I cannot
account for it. The 65-file figure the second pass uses in one place is also
wrong: the API reports 66 files (including 2 ADDED: `keyboard_protocol.rs` and
`tests/keyboard_protocol.rs`).

## Correction 5 — my own setup bug corrupted the B-7b cohort results

The second pass reports that all 4 B-7b cohort builds and SDK parity pass
`EXITCODE=0` on base/merged/#124. Those results are **void**: the scratch dir
I exported as `TMPDIR`, `/tmp/w3-tmp`, **did not exist**, so every cargo
invocation in that step inherited a bad temp path and the "passes" were not
real measurements. I found this by noticing the log files were all empty.

After `mkdir -p /tmp/w3-tmp` and re-running: all 4 cohorts
(`jcode-command-risk`, `jcode-tui-markdown`,
`jcode-provider-openai-runtime`, `jcode-provider-core`) plus SDK parity do in
fact return `EXITCODE=0` on all three trees. **The conclusions survived; the
measurements did not.** Recorded because the operator asked for retraction of
my own errors, not just favorable ones.

## Correction 6 — retraction of the "cold-target build failure"

The second pass recorded `ring`/`aws-lc-sys` build-script errors as part of
the evidence set. Those are an **environment flake on a cold target dir, not a
PR regression**. Re-running the same trees with `--target-dir` pointed at an
existing warm target returns `EXITCODE=0` for all of base, merged, and #124.
Warm-target rustc warning counts: **base=9, merged=2, #124=1**.

## Correction 7 — #132 attribution: CONFIRMED, my detour was the error

I briefly built a "129-only" tree that still contained all three PRs and drew
a wrong conclusion from it, then rebuilt genuine isolation trees. Result on
true isolation:

- **132-only**: all 3 tests PASS
- **129-only**: all 3 tests FAIL

The 3 tests attributed in the second pass to #132
(`build_resume_command_..._claude_code`,
`build_resume_command_..._codex`,
`kitty_notification_is_one_completed_clickable_message`) are fixed by **#132
alone**. **The attribution in the second pass was correct.**

## SCOPE ERROR — 5 open PRs were never examined at all

**This is the most important item in this pass.** The second pass audited
#127, #132, #129, #124 and #107. It presented those as "the open KCode PRs."
They are not. The full open set is **10 PRs**, and **five of them were never
looked at**:

| # | Title | Draft | Size | Mergeable | Base |
|---|---|---|---|---|---|
| #123 | fix(provider-core): opt out of proxy discovery for qualification (cherry-pick of #122) | no | +327/-4, 5 files | MERGEABLE | master |
| #119 | feat(tui): /poke context recovery (PR #25, closes #26) | no | +211/-16, 9 files | MERGEABLE | master |
| #121 | fix(setup-hints): align launch-hotkey markers with the kcode decollation (cherry-pick of #120) | no | +16/-13, 5 files | MERGEABLE | master |
| #8 | fix(openrouter): stop double-polling dead unfold after EOF (panic) + CI/lefthook coverage + herdr plugin manifest | no | **+225,375/-211,309** | **CONFLICTING** | master |
| #10 | feat(acp): discover sessions and broker remote interactions | **yes** | +2,985/-118 | **CONFLICTING** | **feature/herdr-plugin-manifest** (#8's branch) |

New evidence gathered this pass:

- **#8 is genuinely divergent**, not mergeable as-is: **119 commits ahead of
  master, 249 commits behind**. Its 225k-line diff is an artifact of the
  rebase, not 225k lines of work. Merging it would need a rebase, and its
  `Cargo.lock`/`Cargo.toml` churn is the same decollation-revert hazard as #107.
  **Recommendation: do not merge; rebase onto master first or close/stale it.**
- **#10 targets #8's branch, not master**, so it is **not independently
  landable** even once #8 is resolved: 58 ahead / 249 behind master. Anything
  #10 lands lands on top of #8.
- **#121 conflicts with #124** in `crates/jcode-setup-hints/src/linux_env.rs`:
  #121 wants `entry.contains("kcode-launch-") && !entry.contains("jcode-launch-")`
  while #124 writes just `!entry.contains("kcode-launch-")`. **A real landing-order
  constraint:** #121 before #124, or #124 rebased onto #121. #121 merges cleanly
  on top of the #127+#132+#129 stack.
- #123, #119 and #121 have **no CI on their head at all** (`gh pr checks`
  empty), so they were never validated by anything until now.

Local validation of the three small ones (all on the warm target,
`TMPDIR=/tmp/kcode-neutral-tmp`):

- **#123**: `cargo check -p jcode-provider-core` `EXITCODE=0`, 0 warnings;
  `cargo test -p jcode-provider-core --lib` → **150 passed / 0 failed / 1 ignored**;
  the new `qualification_proxy` integration test → **3 passed / 0 failed**.
- **#121**: `cargo check -p jcode-setup-hints` `EXITCODE=0`, 0 warnings;
  `cargo test -p jcode-setup-hints` → **149 passed / 0 failed**.
- **#119**: `cargo check -p jcode-tui --lib` `EXITCODE=0` (10 warnings);
  `cargo test -p jcode-tui --lib poke_context` → **3 passed / 0 failed**
  (2528 filtered), including
  `prompt_requires_context_recovery_and_safe_progress`,
  `parse_poke_context_scope`, and
  `test_remote_poke_context_preserves_prompt_and_attached_images`.

## The baseline failure count is run-dependent — do not treat 27 as fixed

Running the **unmodified baseline** tui suite again this pass on the warm
target gives `2482`-family numbers of **31 failed**, not 27. Re-running #119's
full suite gives **28 failed**, and the diff against that base run is:

- only in 119: `tui::ui::messages::tests::render_system_message_uses_scheduled_task_card`
- only in base: `test_save_and_restore_reload_state_preserves_interleave_and_pending_retry`,
  `..._preserves_observe_mode`, `..._preserves_queued_messages`,
  `test_tui_openai_compatible_local_refresh_failure_is_pending_not_final_failure`

`render_system_message_uses_scheduled_task_card` is **flaky, not a #119
regression**: in isolation it passes on both trees, and on a second full-suite
run on #119 it passes while a *different* test fails. **The tui suite is
flaky independent of any PR.** Any "N pre-existing failures" number quoted in
this brief, including the 27 and the "#124 leaves 2", is a single-run sample,
not a stable measurement. The rankings are still meaningful; the exact counts
are not.

## Revised recommendation set (third pass)

The operator's call is unchanged in principle — but the set of things to decide
is now 10 PRs, not 5.

1. **Safe, verified, small:** #127, #132, #129 (rustfmt half only), #123, #121.
2. **#119:** feature-sized, tests pass, but confirm it is wanted; 0 conflicts.
3. **#124:** highest value (fixes most pre-existing tui failures and clippy),
   **but it is now blocked on CI infrastructure, not on code.** Two hard blockers
   the operator must resolve first:
   - `desk-kcode-1` is **offline**, and #124 migrates 7 jobs onto it. Those jobs
     will silently queue forever.
   - #124 disables **3 of the 12 `ci.yml` jobs outright** (`windows-build-test`,
     `powershell-syntax`, `setup-friction`) and removes the macOS leg. That is a
     large coverage reduction to accept knowingly, not a side effect.
   - Landing-order still matters: merge **#121 before #124**, or rebase #124
     after #121.
   - **The keyboard fix alone is the part I would land.** Splitting it from the
     CI migration is cleaner than taking both together.
4. **#8 and #10:** do not merge as-is. #8 is **119 commits ahead / 249 behind** master and
   needs a rebase; #10 inherits that block because it targets #8's branch.
   #10 is a draft anyway.
5. **#107:** unchanged, DO NOT MERGE (357 files / 1,149 hunks).

## What this brief has been wrong about, in summary, for the record

Manufactured or unsupported numbers: the "1,658" #107 conflict count; the
"+449/-277" #124 stat; the "65 files" #124 file count. Overstated or simply
wrong claims:

- "confirm the desk-linux runner exists" — it does exist. My caveat was wrong.
- Then, correcting that caveat, I wrote that the runner was **"currently
  online."** **It is offline.** This is the most consequential error in the
  brief, because it converts a blocking CI concern into a non-blocking one.
- "hosted Windows CI is retained" (#124) — **wrong.** #124 disables
  `windows-build-test`, `powershell-syntax` and `setup-friction` via `if: false`
  and removes the macOS leg. My pass-2 "Windows CI disabled" claim was closer to
  correct; my pass-3 "correction" of it was the error.

Voided measurements: the B-7b/SDK-parity cohort passes (my missing `TMPDIR`
dir). Run-dependent presented as fixed: the 27-failure tui baseline.
Unstated scope: 5 of 10 open PRs were never examined. The pass-2 #132
attribution itself was correct, and the "129-only" conclusion I briefly drew
was the error, not the attribution.

**A methodological note on why these errors happened.** Both of the pass-3
errors above came from the same failure mode: I ran *one* query, read the first
matching line, and wrote the finding down. The runner status was read once and
recorded as online; the Windows question was answered by grepping `runs-on` and
stopping before reaching the `if: false` two lines later. Neither claim was
stress-tested against a second read, and neither survived one. Any claim in this
brief that rests on a single un-repeated observation should be treated as
unverified.

## New nuance found on re-read: root `[lib]` is still named `jcode`

While re-verifying, I found something pass-2 got subtly wrong. Root
`Cargo.toml` has **both** names:

- line 2: `name = "kcode"` → the release binary, which is correct
- **line 108: `[lib]` `name = "jcode"`** → the library target is **still named
  `jcode`**, and no `[[bin]]` is named `jcode`.

This does **not** overturn the pass-2 `env!` conclusion (which is about
`CARGO_BIN_EXE_jcode`, a *binary* env var, and no bin target named `jcode`
exists), but it does correct one draft claim that the pass-1/2 work implies:
**the decollation is not complete.** The library target was never renamed.
Anyone auditing decollision completeness needs to look at `[lib]` too, and
#121/#124 do not touch it. This is a legitimate finding, and it was missed
twice.

