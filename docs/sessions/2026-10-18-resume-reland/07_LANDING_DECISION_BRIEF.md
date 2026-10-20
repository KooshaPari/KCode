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
| #124 | Windows keyboard fix + CI migration, +524/-352, 66 files | Keyboard fix: LAND (best-tested change in the batch). Bundled CI migration: BLOCKED — targets an offline runner and disables 3 of 9 `ci.yml` jobs | low code risk, high ops surface *(see third pass)* |
| #107 | draft on upstream jcode 0.98.1 | **DO NOT MERGE.** 2,808 commits diverged, 357 conflicted files / 1,149 hunks, would revert decollation | blocked |

## Drafts / large PRs (verdict updated in the extended pass)

- **#124** `fix(tui): keep Windows keyboard negotiation on native console input (cherry-pick of #97)`
  — `isDraft=false`, **+524/-352**, 66 files, `mergeable=MERGEABLE` *(corrected in pass 4)*.
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

- **Land #127 now.** Minimal risk, verified.
- **#132** — minimal risk and verified, but see the constraint added by pass 5:
  it **conflicts with #124** in `helpers_tests.rs`, and #124's version of that
  fix is strictly better (`file_stem()` for `kcode.exe` on Windows). #124 also
  fixes far more pre-existing failures (~25-26 vs #132's 3-4; both numbers are
  run-dependent). Land **#124 first**, or rebase #132 after it — the reverse
  order silently loses the Windows `file_stem()` improvement. *(An earlier pass-5
  draft claimed #124's fixed-set was a strict superset of #132's; that claim is
  retracted in the pass-5 section — it holds on one run and breaks on the next.)*
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
- **#129 — no longer a separate decision; it is inside #132.** *(Pass 6
  supersedes the older "Split #129" advice below.)* #129's head `1922c0f56` is an
  **ancestor of #132's current head `f8f488392`** — #132 merged the #129 branch in
  so its own Format + Quality Guardrails would pass. Landing #132 therefore lands
  #129 too. If the `env!` change is unwanted it must be reverted **inside #132**,
  not in a separate PR.
- ~~**Split #129.** Land the rustfmt half (verified: clears all drift); revert the
  `env!` half or change it to `CARGO_BIN_EXE_kcode`.~~ *(Obsolete — see the bullet
  above.)*
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
`+524/-352` — see Correction 4; the 513/351 figure is wrong).*

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
> #124 disables 3 of 9 `ci.yml` jobs. The recommendation below now reads
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

So: **3 of the 9 jobs in `ci.yml` are disabled outright, the macOS leg is
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
**`+514/-352`** across **66 files**. *(SUPERSEDED in pass 4: the true figure is
  **`+524/-352`** across 66 files, and the head SHA moved. See Correction 8.)*

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
  ~~#121 wants `entry.contains("kcode-launch-") && !entry.contains("jcode-launch-")`
  while #124 writes just `!entry.contains("kcode-launch-")`.~~ **That is backwards
  — corrected in pass 6.** The real split is: **#121** is a pure `jcode`→`kcode`
  rename (only `[services][kcode-launch-`) while **#124** keeps BOTH markers
  (`jcode` **or** `kcode`), i.e. #124 is backward compatible and #121 is a clean
  break. Verified by reading both blobs **and** by a real test merge: exactly
  **1 conflicted file, 1 conflict hunk**. The hunk is the `skipping = ...` line,
  not the `entry.contains(...)` predicate the old text pointed at (#121 never
  touches that line). **Resolve toward #124** — taking #121's side silently drops
  handling for pre-existing `jcode-launch-*` slots and orphans user config from
  before the rename. #121's other 4 files do not conflict.
- ~~#123, #119 and #121 have **no CI on their head at all** (`gh pr checks`
  empty), so they were never validated by anything until now.~~ **WRONG —
  retracted in pass 6.** All three have CI, it ran at 03:17-03:22Z (before this
  sentence was written), and it FAILED on all three platforms. The failures are
  pre-existing (master's own push runs fail too). See "Closing the remaining
  'not re-validated' items" at the end of this brief.

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
   - #124 disables **3 of the 9 `ci.yml` jobs outright** (`windows-build-test`,
     `powershell-syntax`, `setup-friction`) and removes the macOS leg. That is a
     large coverage reduction to accept knowingly, not a side effect.
   - Landing-order still matters: **resolve the `linux_env.rs` hunk toward
     #124's both-markers version** (see the corrected #121 note above — the old
     "merge #121 before #124" framing was based on a backwards reading).
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

### Pass-5 additions to this list

- **"#124's fixed set is a strict superset of #132's."** **RETRACTED — this was
  my own pass-5 claim and it fails its own cross-check.** Against #124's run-1
  failure set, `fixed_132 - fixed_124` is **empty** (26 fixed vs 4), which is
  what made me write the claim. Against run-2 it is **one test**:
  `build_resume_command_..._codex`, which #124 fixed on run 1 and failed again
  on run 2. The lesson is the same as pass 4's, one level subtler: pass 4 taught
  that a *count* is run-dependent. Pass 5 teaches that a *set relationship*
  derived from those counts is run-dependent too. I had computed it once, found
  zero asymmetry, and wrote it down as a property of the PRs instead of a
  property of the run. Also note: I first documented the retraction with the
  comparison direction described backwards (as if run 1 alone produced the
  counterexample) and had to re-verify and correct that too — two wrong
  accounts of one relationship, both mine.
- I also wrote a **fabricated self-accusation** into the pass-5 draft (claiming
  I had mis-transcribed a #129 path). Grep showed the brief always had it
  correct, and I removed it before commit. Worth noting because the direction
  was opposite: not an overstatement, an invented error.

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


---

# FOURTH VALIDATION PASS (2026-10-19, later same session)

Trigger: while re-verifying single un-repeated observations (per pass 3's own
methodological warning), I found **#124 was rebased under me** and that two
figures I had written as "corrections" were themselves wrong.

## Correction 8 — #124 was rebased; my tested tree is stale (but conclusions survive)

The commit I tested, `126a6586c`, is **no longer an ancestor of #124's head.**

- Head then: `126a6586c`
- Head now: **`e37ca7a38ae4`** (`test(resume): restore KCODE_HOME via drop guard
  in busy-attach test`), **37 commits** total on the PR
- `126a6586c` is still reachable in the object store; it is just superseded
- Delta between them is **2 commits**: `c32f99086` (YAML key dedup in a disabled
  job) and `e37ca7a38ae4` (a `KcodeHomeGuard` drop-guard refactor in
  `jcode-app-core` test code). Neither touches `jcode-tui` behavior.

Because the delta is 2 commits and neither is behavioral for the tui suite, I
**re-ran every #124 gate at the current head** rather than assuming the old
results carried over:

| Check | Re-run at current head | Prior (old head) | Verdict |
|---|---|---|---|
| `cargo test -p jcode-tui --lib -- --test-threads=4` | `2510 passed; 2 failed; 19 ignored` | `2510 passed; 2 failed` | **unchanged** |
| `cargo clippy -p jcode-tui --lib -- -D warnings` | `EXITCODE=0` | `EXITCODE=0` | **unchanged** |
| `cargo fmt --all -- --check` | `EXITCODE=0` | `EXITCODE=0` | **unchanged** |

So the #124 keyboard-fix recommendation stands on fresh evidence at the current
head. Note the surface for this is real: a PR can change between "I tested it"
and "the operator merges it," so any #124 conclusion should be re-checked at
merge time, not inherited from this brief.

## Correction 9 — my "+514/-352" was wrong too. The real stat is +524/-352

In Correction 4 I replaced one fabricated pair with another. I read
`additions=514` off a single `gh pr view` call and wrote it down as the correct
number. Re-measured two independent ways this pass:

- `gh pr view 124 --json additions,deletions` → **`additions=524`,
  `deletions=352`**, `mergeable=MERGEABLE`, `draft=false`, `changed_files=66`
- A second derivation, summing per-file `.additions`/`.deletions` from
  `gh api .../pulls/124/files` → **524 / 352 across 66 files**. This is a real
  second source; the `changes` field on that endpoint equals `additions +
  deletions` per file (verified), so summing `changes` instead yields 876/876,
  which is a **counting trap, not a contradiction** — do not compare it to the
  PR-level fields. The PR-level `additions`/`deletions` (524/352) are the
  correct source, and they are what both derivations agree on.

> *(Method note added in pass 5: the second bullet above was re-derived
> correctly in pass 5. The pass-4 wording implied a single call with a
> malformed `?per_page=100` path was a valid independent source. It landed on
> the same numbers, so the conclusion held, but the described method did not.)*

**Use `+524/-352`, 66 files.** The `+513/-351`, `+449/-277` and `+514/-352`
figures are all retracted; 514 was mine.

## Correction 10 — "3 of 12 jobs" was wrong: it is 9 jobs total, 7 on the desk runner

Two separate miscounts:

1. **The denominator was wrong.** Pass 3 said "12 jobs in `ci.yml`." Enumerating
   top-level job keys in #124's post-image `ci.yml` gives **9**:
   `quality`, `release-automation`, `build`, `windows-build-test`, `fmt`,
   `typescript-sdk`, `setup-friction`, `powershell-syntax`,
   `windows-cross-check`. (`push` and the `on:` triggers are not jobs; a naive
   `grep -cE '^  [a-z-]+:$'` returns 10 because it catches `push:` too.)
2. **The desk-runner numerator was right for the wrong reason.** Pass 3 said "7
   jobs move to `[self-hosted, desk-linux]`." Counting `runs-on` lines
   containing `desk-linux` gives only **6** (`quality`, `release-automation`,
   `fmt`, `typescript-sdk`, `setup-friction`, `windows-cross-check`). The 7th is
   **`build`**, which does *not* name `desk-linux` on its `runs-on` line — it
   uses `runs-on: ${{ matrix.os }}` with `matrix.os: [desk-linux]`. So **7 is
   correct, but it requires reading the matrix, not just `runs-on`.**

Verified final state on #124's current head:

- **3 of 9 `ci.yml` jobs are `if: false`**: `windows-build-test`,
  `powershell-syntax`, `setup-friction` (all confirmed by reading the `if:`
  line, per the pass-3 lesson about not stopping at `runs-on`)
- **7 of 9 run on `[self-hosted, desk-linux]`**, one of which (`setup-friction`)
  is itself disabled
- macOS leg removed from the `build` matrix (`os: [desk-linux]` only, comment
  says hosted minutes are capped at $0)
- Both remaining `windows-latest` jobs are `if: false`

**Blocking status unchanged and still the binding constraint:** `desk-kcode-1`
remains **`status=offline`** on re-query this pass, and #124 routes 7 jobs to it.
Those jobs queue forever until it is brought up.

## Housekeeping note

The pass-3 cleanup item ("remove the 5 `wt3-*` worktrees") is **already done**:
`git worktree list` shows no `wt3-*` entries and none exist on disk. The removal
happened during the context-overflow recovery that dropped the preceding
messages. Total registered worktrees: **54**, including pre-existing ones not
created by this audit — left untouched deliberately.

The `wt3-124-current` worktree used for the re-runs in Correction 8 was removed
after the gates passed, so no new worktrees remain from this pass either.

---

# FIFTH VALIDATION PASS — independent re-validation of the whole brief

Instruction: re-validate the claims marked complete, because several rested on
worktrees that no longer existed or on single un-repeated observations. Nothing
here is a re-statement of an earlier pass unless it was re-measured this pass.

## Re-validated and confirmed

**Commits are on origin.** `82e5e5f6b` and `c0f40e398` are both ancestors of
`origin/fix/resume-peer-drain-relend`, whose tip now equals local HEAD. `c0f40e398`
confirmed independently via `gh api .../commits`.

**#124 size: two independent derivations agree exactly.** Re-derived again this
pass with corrected command syntax, because the syntax documented in the
pass-5 text below was itself wrong (see note).

- `gh pr view --json additions,deletions,changedFiles,headRefOid,state,isDraft,mergeable`
  → **`additions=524`, `deletions=352`, `changedFiles=66`**, head
  `e37ca7a38ae4...`, `state=OPEN`, `isDraft=false`, `mergeable=MERGEABLE`
- Summing per-file `.additions`/`.deletions` from `gh pr view --json files`
  → **524 / 352 across 66 files** (matches all three)
- The REST endpoint `gh api repos/KooshaPari/KCode/pulls/124/files` returns all
  66 files in **one page** (no `per_page` juggling needed) and additionally
  exposes `changes`, whose per-file sum is **876** — consistent with
  `additions + deletions = 524 + 352` in every single file (checked individually:
  zero exceptions). That is why summing `changes` and comparing it to the
  PR-level numbers is a **counting trap, not a contradiction.**

> *(Correction added this pass to my own documentation: the method as written in
> pass 5 could not have run. `changed_files` is not a valid `--json` field (the
> real name is `changedFiles`), and `changes` is absent from
> `gh pr view --json files` entirely — it only exists on the REST endpoint. The
> numbers 524/352/66 are correct and now reproduced from both sources, but the
> recorded commands were wrong and are fixed above.)*

**#124 ci.yml state re-read from the commit object** (`git show <head>:.github/
workflows/ci.yml`, no worktree needed), parsed by a real YAML parser this pass
(an independent method — the pass-5 text says "a script that walks the `jobs:`
block rather than by grep"; `yaml.safe_load` is stronger and agrees): **9
top-level jobs** (`quality`, `release-automation`, `build`, `windows-build-test`,
`fmt`, `typescript-sdk`, `setup-friction`, `powershell-syntax`,
`windows-cross-check`); **3 disabled** by `if: false` (`windows-build-test`,
`powershell-syntax`, `setup-friction`); **7 desk-routed**, `build` reaching the
desk via `matrix.os: [desk-linux]` on a `runs-on: ${{ matrix.os }}` line,
exactly as pass 4 said. Both remaining `windows-latest` jobs are `if: false`.

> **REFINEMENT (this pass) — a 7-vs-6 detail the brief left ambiguous.** #124 is
> described throughout as "migrating 7 jobs onto the offline runner." That is
> correct as *routing*, but **only 6 of the 7 are actually enabled**:
> `setup-friction` is desk-routed *and* one of the three `if: false` jobs. So the
> accurate operational statement is: **6 enabled jobs would silently queue
> forever** on the offline `desk-kcode-1` runner; the 7th is skipped anyway. This
> makes the CI picture marginally less severe than the brief implies, but the
> blocking conclusion is unchanged — #124's CI migration cannot be relied on
> until that runner is up. Non-desk enabled jobs: **none.** Every enabled job in
> #124's `ci.yml` runs on the desk.

**#132 attribution: now confirmed with a real control.** Pass 3's version had no
proof that its trees were genuinely isolated. This pass built three trees at the
same master tip (`b9886143c`) and **proved isolation by file-set equality** —
each tree's diff-vs-master matched the corresponding PR's own `files` list
exactly. (Pass 3 has no such proof anywhere: it asserted isolation without ever
comparing a tree's changed-file set against the PR's declared file list.)

| Test | base | 132-only | 129-only |
|---|---|---|---|
| `build_resume_command_..._claude_code` | FAIL | **PASS** | FAIL |
| `build_resume_command_..._codex` | FAIL | **PASS** | FAIL |
| `kitty_notification_is_one_completed_clickable_message` | FAIL | **PASS** | FAIL |

Causal mechanism inspected, not just correlated: #132 changes
`String::from("jcode")` → `("kcode")` in `build_turn_notification` and
`PathBuf::from("jcode")` → `("kcode")` in `launch_client_executable`, and updates
the three assertions to match. On base the panic reads
`left: Some("kcode") / right: Some("jcode")`, which is the expected shape.

Full-suite on 132-only: **2485 passed / 24 failed** vs base 2481/28 in the same
session — +3 fixed, **0 regressions** (empty `comm -13`).

## NEW FINDING 1 — #124 and #132 CONFLICT in `helpers_tests.rs`

Not recorded anywhere in earlier passes. Merging #132 (`9ead3963e5a95`) onto
#124's current head:

```
CONFLICT (content): Merge conflict in crates/jcode-tui/src/tui/app/helpers_tests.rs
```

The cause is that **both PRs fix the same two assertions**, and #124 fixes them
*better*: it switches `program.file_name()` → `program.file_stem()` with the
comment "the launcher binary is kcode.exe on Windows," whereas #132 only rewrote
`Some("jcode")` → `Some("kcode")` on `file_name()`. Both sides end at
`Some("kcode")`; only the accessor differs.

**Landing consequence:** with both open on `master`, whoever lands second hits a
one-hunk conflict in two places. It is trivially resolvable in #124's favour
(keeping `file_stem()`), which I verified: resolving toward #124 leaves both
tests passing (`EXITCODE=0` each). The operator should land **#124 first, or
rebased #132 after it** — the reverse order silently loses the Windows
`file_stem()` improvement. This is a stronger version of the #121/#124 ordering
note above — same shape of problem (two PRs editing the same predicate), but for
that pair the correct resolution is toward #124's **wider** predicate, not
whichever lands second.

## NEW FINDING 2 — the "0 regressions / fixes 25 of 27" claim does not
## survive repeated runs. It churns.

Every earlier pass quoted a single full-suite run per tree. Running each tree
**twice in the same session on the same warm target**:

| Tree | Run 1 | Run 2 |
|---|---|---|
| base | 2481 passed / **28 failed** | 2482 passed / **27 failed** |
| #124 | 2509 passed / **3 failed** | 2508 passed / **4 failed** |

Failure-set churn between the two runs of the *same tree*:

- **base:** 1 test flipped — `test_new_for_remote_restores_observe_mode_from_reload_state`
- **#124:** 3 tests flipped, including
  `build_resume_command_uses_imported_jcode_session_for_codex` (a test #124
  *fixes*, which then failed on run 2) and
  `render_system_message_uses_scheduled_task_card` — precisely the test pass 3
  had already flagged as flaky.

I chased the one apparent #124 regression
(`test_model_picker_waits_for_async_post_login_catalog_activation`, which failed
in the full suite but not on base) rather than reporting it: it **passes in
isolation on both trees** (`EXITCODE=0` each), so it is a parallel-load/timing
flake, not a regression. Worth noting its two sibling failures in the same log
are network-dependent (`https://auth.x.ai/...` DNS cancellation), i.e. the suite
has an environmental-noise floor independent of any PR.

**Conclusion for the operator: treat every exact count in this brief as a
distribution, not a measurement — and that includes the set-comparison claim I
am about to make, which did NOT survive its own cross-check.**

On the run-1 numbers, #124's fixed set appears to be a strict superset of
#132's: 26 tests fixed vs 4, and `fixed_132 - fixed_124` came out **empty**.
Re-checking the same comparison against #124's **run-2** failure set **breaks
it** — `build_resume_command_uses_imported_jcode_session_for_codex`
(fixed by #124 on run 1) *failed* on run 2, so against run 2 the difference
`fixed_132 - fixed_124` is that one test, not zero. That is the same churn
already documented above, and it means **the superset is a property of one
run, not of the PRs.**

Do not lean on it. The defensible version is narrower and still useful: #132
fixes very few tests (3-4), #124 fixes many more (25-26), and the two PRs
overlap in at least the one test both fixes — `build_resume_command_..._codex`
— which is itself evidence of genuine coupling, consistent with the real
conflict found above. No genuine PR-caused regression was found. The exact
integers (27, 25, 2, 3, 4) should not be quoted as facts, including the #124
headline number.

## What this pass did NOT re-validate

Deliberately out of scope: the #123/#119/#121 one-shot check runs (their trees
were not rebuilt), and the #8/#10 divergence counts (measured once, earlier).
Those remain single-observation claims and should be treated as unverified.

## Housekeeping

All 5 `w5-*` worktrees created for this pass were removed after use; total
registered worktrees back to **54**, matching the pass-4 baseline. No `w5-`
dirs or logs left in scratch.

---

# SIXTH VALIDATION PASS — deep re-derivation of every pass-5 claim

Trigger: re-check each pass-5 claim against its primary source instead of against
my own notes. Method upgrade: a real YAML parser for the CI file, corrected `gh`
field names, and a fresh set of isolation trees at the **current** PR heads.

## THE BIG ONE — #132 has absorbed #129, and pass 5 tested a stale head

Pass 5 tested #132 at `9ead3963e5a95` (12-13 files, tui-only). **That is not
#132's head any more.** Current head is **`f8f488392fe7d`**, and it is a merge
commit:

```
f8f488392 merge: bring in PR #129's fmt + clippy fixes so this branch passes
          Format + Quality Guardrails
1922c0f56 fix(jcode-tui-mermaid): collapse split doc comment to satisfy clippy
68e322d8a Merge remote-tracking branch 'origin/master' into fix/repo-ci-pre-existing-drift
5b65b1878 fix(ci): collapse nested if-let in sockets.rs (clippy collapsible_if)
36375a526 fix(ci): clear repo-wide rustfmt drift + fix env! macro in cli_status
```

Verified by ancestry, not by reading the message:

- `9ead3963e5a95` **is an ancestor of** `f8f488392fe7d` (the branch was extended,
  not rebased) — so pass-5's results are not *wrong*, they are *incomplete*
- **#129's head `1922c0f56` IS an ancestor of #132's head `f8f488392`**
- reverse does not hold: #132 is not contained in #129
- both PRs target `master`

**Consequence for the operator: landing #132 now lands #129 as well.** #132's
file list grew from the tui-only set to 13 files that include #129's full 10-file
rustfmt/clippy list. This makes the standing advice **"Split #129: land the
rustfmt half, revert the `env!` half"** obsolete — that split has already
happened on the other side, inside #132. The decision is no longer "three small
PRs"; it is "#127, and one combined #132+#129 PR".

This is the same class of error as pass 4's "#124 was rebased under me," and it
should be assumed to keep happening: **PR heads move between validation and
merge.** Every conclusion here is pinned to a SHA for that reason.

## Re-verified at the CURRENT heads

**Isolation, re-proven by file-set equality** (the proof pass 3 never had, now
repeated at the new heads). Three trees at `master = b9886143c`, each merged with
one PR head:

| Tree | diff-vs-master files | PR-declared files | Exact match |
|---|---|---|---|
| #132 @ `f8f488392` | 13 | 13 | **yes** |
| #129 @ `1922c0f56` | 10 | 10 | **yes** |

**Attribution holds at the new #132 head.** `cargo test -p jcode-tui --lib --
build_resume_command`:

| Tree | Result | Exit |
|---|---|---|
| base @ `b9886143c` | 0 passed / **2 failed** | 101 |
| #132 @ `f8f488392` (current) | **2 passed** / 0 failed | 0 |

So #132 still fixes the attribution tests after absorbing #129. **All six runs
completed — this is the full matrix, and it is unambiguous:**

| Test | base @ `b9886143c` | #132 @ `f8f488392` (current) | #129 @ `1922c0f56` |
|---|---|---|---|
| `build_resume_command` (2 tests) | 0 passed / **2 failed** | **2 passed** / 0 failed | 0 passed / **2 failed** |
| `kitty_notification` (1 test) | 0 passed / **1 failed** | **1 passed** / 0 failed | 0 passed / **1 failed** |

Exit codes: base `101`, #132 `0`, #129 `101` on both invocations. The
attribution from pass 5 therefore survives the head change intact: **#132 fixes
these tests, #129 alone does not**, and the fact that #132 now contains #129 does
not dilute that. Note the #129 column is the control that makes this meaningful —
without it, "#132 passes" would be consistent with "either PR passing."

**#124 size, re-derived with corrected syntax — 524/352/66 confirmed twice.**
This also fixes an error in my own documentation: the command I recorded in
pass 5 (`--json additions,deletions,changed_files`) **cannot run**.
`changed_files` is not a valid field (it is `changedFiles`), and `changes` does
not exist on `gh pr view --json files` at all — only on the REST endpoint. The
numbers were right; the recorded method was wrong. Corrected in the pass-5 text.

**#124 ci.yml, re-parsed with `yaml.safe_load`** (independent of my hand-rolled
walker): 9 jobs, 3 `if: false`, 7 desk-routed, `build` reaching the desk through
`matrix.os: [desk-linux]`. Confirmed, plus one refinement: **only 6 of the 7
desk jobs are enabled** — `setup-friction` is desk-routed *and* disabled. So
**6** enabled jobs, not 7, would queue forever on the offline runner. Non-desk
enabled jobs: **none**.

**Runner, re-queried twice this pass:** `desk-kcode-1`, `status=offline`,
`busy=true`, labels include `desk-linux`. Identical across both queries. The
`busy=true` + `offline` combination means it holds a job lock while not
connected — it will not drain that job by itself.

**Commits on origin:** `82e5e5f6b` and `c0f40e398` are ancestors of
`origin/fix/resume-peer-drain-relend` (3 and 2 commits back respectively); local
HEAD equals origin tip.

## Worktree state (asked directly, recorded here)

- **57** registered worktrees, all of `KooshaPari/KCode`. Baseline before this
  pass: 54. The 3 extra are this pass's isolation trees under `/tmp`.
- Zero prunable, zero locked, zero missing paths.
- **The main repo is not on `master`.** It moved twice while I was looking:
  `fix/repo-ci-pre-existing-drift` → `fix/setup-hints-orphan-spawn-notice`. Other
  agents are switching branches in the shared checkout, so it is **not** a
  reliable read-only baseline. Use a detached worktree at a pinned SHA instead.
- Box state during this pass: 10 cores, 16 GB, **load average 848**, ~12
  concurrent `cargo` processes from other agents. Any timing or parallelism claim
  made under this load is not meaningful.

## Method lesson from this pass

I tried to parallelise the three trees by **hardlink-cloning** the warm target
(`cp -al`), expecting three independent builds for ~2 GB of extra disk. It does
not work: the clones share the **same `target/debug/.cargo-lock` inode**
(verified: all three at inode `958582255`), so cargo serialises them exactly as if
they were one directory. It is safe, but it is not parallel. True parallelism
needs `--target-dir` per tree on genuinely separate copies, or removing the lock
file per clone and accepting that hardlinked artifact writes race.

## Closing the remaining "not re-validated" items (pass 6)

The pass-5 section listed #123/#119/#121 and #8/#10 as deliberately out of scope.
The cheap parts are now re-derived. The expensive parts (rebuilding those trees
and re-running their package tests) stay out of scope, and are marked as such.

### CORRECTION — "#123/#119/#121 have no CI on their head at all" is WRONG

Pass 3 recorded: *"#123, #119 and #121 have **no CI on their head at all**
(`gh pr checks` empty), so they were never validated by anything until now."*
That is false as stated. All three DO have CI, and it ran **before** pass 3 wrote
that sentence:

| PR | run | created | conclusion |
|---|---|---|---|
| #123 (head `587ba412f`) | 39194359668 | 2026-10-19T03:22:11Z | **failure** |
| #119 (head `b3ec4dab4`) | 39193978433 | 2026-10-19T03:17:21Z | **failure** |
| #121 (head `b65a4701b`) | 39193983666 | 2026-10-19T03:17:25Z | **failure** |

Each fails `Build & Test` on all three of macos-latest, ubuntu-latest and
windows-latest. So the true statement is "CI ran and failed," not "CI never ran."

**But the failures are pre-existing, not PR-caused** — established with a control
rather than assumed. Every `push`-event CI run on `master` fails too:

```
2026-10-19T06:53:03Z failure b9886143c1   <- current master
2026-10-19T03:23:05Z failure 081c1cf608
2026-10-19T00:10:59Z failure 19ae474570
2026-10-18T08:57:00Z failure 6ff8aa2d15
```

Master is red on its own. A three-platform, three-PR uniform failure — including
for #121, which touches 5 files in `jcode-setup-hints` and cannot plausibly break
macOS *and* Windows *and* Linux — is the same pre-existing drift already
documented here (failing `warning`, `codesize`, `testsize`, `panic`, `swallow`
gates and ~27 pre-existing tui failures). **Conclusion unchanged: these PRs are
not making CI worse, because CI was already red.** What the failing badge does
mean is that **the "mergeable" signal from CI is worthless on this repo right
now** — a red check tells you nothing about the PR.

Metadata re-verified and identical to the earlier record: #123 `+327/-4` 5 files,
#119 `+211/-16` 9 files, #121 `+16/-13` 5 files; all OPEN, non-draft,
MERGEABLE, all targeting `master`.

### #121 still conflicts with #124 — file confirmed

`crates/jcode-setup-hints/src/linux_env.rs` is in #121's 5-file list, as recorded.
The one-line landing-order constraint (#121 before #124, or rebase #124 onto
#121) stands unverified at the content level this pass — I confirmed the file is
in scope, not the exact predicate collision, which was read in pass 3.

### #8 and #10 divergence — re-derived exactly, and a new detail

| PR | ahead of master | behind master |
|---|---|---|
| #8 (head `adb5669e6`) | **119** | **249** |
| #10 (head `09680ac23`) | **58** | **249** |

Both match the earlier figures exactly. New detail worth recording: **#10 is 27
commits ahead of, and 88 behind, #8's current head.** Its base branch
`feature/herdr-plugin-manifest` now points at `adb5669e6` — the same commit as
#8's head — so #10 was branched from an older state of that branch and has since
fallen 88 commits behind it. #10 is therefore blocked twice over: it inherits
#8's divergence from master, and it is itself stale relative to #8. It is also
still a draft. **Do not merge; do not treat as landable-in-sequence behind #8
without a rebase.**

### CORRECTION — the #121/#124 collision was described with the two PRs SWAPPED

Pass 3 recorded the `linux_env.rs` collision as:

> "#121 wants `entry.contains("kcode-launch-") && !entry.contains("jcode-launch-")`
> while #124 writes just `!entry.contains("kcode-launch-")`."

**That is backwards, and the predicate is also mis-transcribed** (both sides are
negated; neither side "wants" a positive `contains`). Verified by reading both
blobs and by an actual test merge:

```
$ git merge b65a4701b   (into #124's head e37ca7a38)
CONFLICT (content): Merge conflict in crates/jcode-setup-hints/src/linux_env.rs
1 conflicted file, 1 conflict hunk
```

The real conflict, at the `skipping = ...` line:

```
<<<<<<< HEAD                                    (#124)
            skipping = line.starts_with("[services][jcode-launch-")
                || line.starts_with("[services][kcode-launch-");
=======                                          (#121)
            skipping = line.starts_with("[services][kcode-launch-");
>>>>>>> b65a4701b
```

The two PRs are doing **different jobs**, not the same job twice:

| | approach | effect |
|---|---|---|
| **#121** | pure rename `jcode` → `kcode` everywhere (code, comments, tests) | legacy `jcode-launch-*` entries are **no longer matched** — a clean break |
| **#124** | adds `kcode` **alongside** `jcode` (`!kcode && !jcode`; `jcode \|\| kcode`) | legacy and new entries are both handled — backward compatible |

So the landing-order note needs inverting in spirit: the question is not "#121
before #124" but **which marker set you want to keep**. Resolution should
preserve **#124's both-markers version**; taking #121's side would silently drop
handling for pre-existing `jcode-launch-*` slots, orphaning user config from before
the rename. #121's other 4 files (`lib.rs`, `linux_niri.rs`,
`macos_launcher_tests.rs`, `setup_hints_tests.rs`) do not conflict.

Also note: the collision is **not** at the `entry.contains(...)` predicate the
pass-3 text points at. #121 never touches that line — the diff at
`if !entry.contains(...)` is #124's alone. The single conflict hunk is the
`skipping = ...` line, 144 lines further down.

### Root `[lib]` decollision check — confirmed still unfixed

Pass 3's finding holds, verified by parsing the manifest this pass: root
`Cargo.toml` has `package.name = "kcode"` and `[lib] name = "jcode"`
(`Cargo.toml:107-109`), and there are **7 `[[bin]]` targets, none of them named
`jcode`** — `kcode`, `test_api`, `kcode-harness`, `session_memory_bench`,
`memory_recall_bench`, `mermaid_side_panel_probe`, `tui_bench`. The single
`name = "jcode"` occurrence in the whole manifest is the library target.
Neither #121 nor #124 touches this, so the decollision is still incomplete at
the library-target level even after the rename work lands. This does not
invalidate the `CARGO_BIN_EXE_jcode` conclusion (no bin of that name exists), but
it is an open piece of the rename that nothing in the current PR set closes.
