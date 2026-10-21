# Merge Integration Plan - KCode PR Stack (2026-10-21)

Status: ready for operator review | Date: 2026-10-21 | Author: subagent (merge triage)
Session: `docs/sessions/2026-10-19-orchestration-governance/`
Repo: `KooshaPari/KCode` @ `2aea04dd871f21da5c5b21800d156a2c9b0830b1`
Local clone: `/Users/kooshapari/CodeProjects/Phenotype/repos/jcode`
Supersedes: the 2026-10-19 edition of this document (written against `b9886143c`).

---

## 1. What changed since 2026-10-19

Master advanced from `b9886143c` to `2aea04dd8`. Two upstream PRs landed in that
window and they absorbed a large slice of the local fix stack:

| Upstream PR | Title | Effect on this plan |
|---|---|---|
| #124 | Windows keyboard negotiation | Added `crates/jcode-tui/src/tui/keyboard_protocol.rs`. Its TUI assertions landed on master, so 8 of the 21 issue-135 tests now pass without any local PR. |
| #138 | Portal XJ-03 bridge | Lands independently of this stack. No direct file overlap with the local PRs below. |

Consequences for the previous edition:

- Steps 6 through 9, which merged `#129`, `#132`, and `#134`, are **obsolete**. All
  three are superseded, and their replacements (`#156`, `#157`, `#158`) have mostly
  become no-ops on master.
- The 11-PR scope has shrunk to ten. Two are provably empty and one is comment-only.
- The file-set table was recomputed against current master rather than carried forward.

Every file and line count below is a **true residual** against `origin/master` at
`2aea04dd8`, computed with the procedure in section 10. A true residual is what the PR
would actually change if merged today, not the diff between its base and its head.

---

## 2. Current stack

All residuals computed against `origin/master` = `2aea04dd8`.

| PR | Branch | True residual vs master | Issue | Conflict | Recommendation |
|---:|---|---|---|---|---|
| #156 | `fix/ci-rustfmt-clippy-drift-clean` | **EMPTY (0 files).** All 8 of its files are byte-identical to master. | #130 | clean | **NO-OPTIONAL.** Content absorbed by upstream merges. Merging is a harmless no-op. |
| #157 | `fix/tui-kcode-decollision` | **3 files, 15 lines.** `helpers.rs` (1), `helpers_tests.rs` (4), `turn_notify.rs` (10). All `jcode` to `kcode` string decollision in the turn-notification title and the resume-command binary name. | #131 | **CONFLICTS** on `crates/jcode-tui/src/tui/app/helpers_tests.rs` | **MERGE-AFTER-CONFLICT-RESOLUTION.** See section 4.1. |
| #158 | `fix/tui-cache-estimate-not-evidence` | **COMMENT-ONLY.** Master already has `!info.is_cold && !info.is_estimate`. The PR adds an explanatory comment and reorders the two conditions. Regression test `cache_estimate_is_not_evidence_of_an_expected_warm_hit` already exists on master at `crates/jcode-tui/src/tui/mod.rs:1887`. | #133 | **CONFLICTS** on `crates/jcode-tui/src/tui/kv_cache_problem.rs` | **SUPERSEDED.** Behavior already on master. Recommend **CLOSE**, or resolve manually to keep only the comment. |
| #150 | `fix/tui-cache-expected-warm-unused-import` | **EMPTY (0 files).** Master already has `#[cfg(test)] pub(crate) use kv_cache_problem::cache_expected_warm;`. | #149 | clean | **NO-OPTIONAL.** Already on master. |
| #152 | `fix/cli-exit-status-kcode-bin` | **1 file, 3 lines.** `tests/cli_exit_status.rs`. `JCODE_HOME` to `KCODE_HOME`, `join("jcode")` to `join("kcode")`, `starts_with("jcode ")` to `starts_with("kcode ")`. | #147 | clean | **MERGE.** The `env!("CARGO_BIN_EXE_kcode")` half is already on master. The env-var half is **not**, and is a real fix (section 3). |
| #148 | `fix/sdk-test-paths-kcode-prefix` | **2 files, 4 lines.** `sdk/typescript/test/launch.test.ts`, `sdk/typescript/test/live-options.mjs`. Fixture-path renames only. | #147 | clean | **SUPERSEDED** by #160. Its 2-file residual is a strict subset of #160's 7-file residual. |
| #160 | `fix/sdk-typescript-kcode-assertions` | **7 files, 18 lines.** `launch.test.ts`, `live-isolation.mjs`, `live-launch.mjs`, `live-options.mjs`, `live-text-framing.mjs`, `live-tools.mjs`, `pipe-name.test.ts`. | #159 | clean | **MERGE.** Fully subsumes #148. PR body carries `Closes #159`. |
| #145 | `docs/orchestration-governance-docs-only` | **2 docs files.** This proposal plus this plan. | #143 | clean | **MERGE** (this PR). |
| #140 | `fix/setup-hints-pr128-test-regression` | Superset of #137. | #139 | clean | **MERGE.** |
| #127 | `fix/update-product-warnings` | CI unblock for update-product warnings. | #142 | clean | **MERGE.** |

### 2.1 The `JCODE_HOME` finding

This is the one remaining item in the whole stack that is a genuine fix rather than a
rename already done. Verified across the entire Rust tree on master: `KCODE_HOME` is
used everywhere across dozens of crates. `JCODE_HOME` survives in **exactly one Rust
file**, `tests/cli_exit_status.rs`. One further mention lives in
`crates/jcode-telemetry-core/CONCURRENCY.md`, which is documentation prose.

That is precisely the 3-line residual #152 still carries. The `env!("CARGO_BIN_EXE_kcode")`
half of #152 is already on master. Only the env-var half remains. It should be merged
as a real fix, not dismissed as cosmetic.

---

## 3. Recommended merge order

Reasoning is given inline. The order is chosen so that no PR is merged into a tree
that a later PR then invalidates.

| Step | PR | Why here |
|---:|---|---|
| 1 | #127 | Smallest independent CI unblock. No file overlap with any later step, so it can also be merged at any point without risk. |
| 2 | #140 | Superset of #137. Merging the superset first makes #137 redundant, which is the intent. |
| 3 | #152 | Independent of every other step in this table. Touches only `tests/cli_exit_status.rs`. |
| 4 | #160 | Independent of #152. Touches only `sdk/typescript/`. **Fully subsumes #148**, so #148 should be closed rather than merged. |
| 5 | #157 | Requires manual conflict resolution (section 4.1). Sequenced late so the operator has the most information before resolving it. |
| 6 | #145 | This PR. Docs only, zero code interaction, safe to land at any time. |
| 7 | #156 | **Optional.** Empty residual. Merging changes nothing. Skipping is equally valid. |
| 8 | #150 | **Optional.** Empty residual. Same reasoning as #156. |

### 3.1 Explicit independence statements

- **#152 and #160 touch disjoint files.** #152 is `tests/cli_exit_status.rs` only.
  #160 is seven files under `sdk/typescript/`. They can merge in **either order**
  with no interaction. Steps 3 and 4 are not a dependency, only a stable presentation
  order.
- **#160 subsumes #148.** #148's true residual (`launch.test.ts`, `live-options.mjs`,
  4 lines) is a strict subset of #160's true residual (those same two files plus five
  more, 18 lines). Merging #148 after #160 would be a no-op. The recommendation is to
  **merge #160 and close #148**.
- **#150 and #156 need no merge at all.** Both have an empty true residual. They are
  listed as optional steps purely so the operator can see they were evaluated.

---

## 4. Conflict-resolution guidance

### 4.1 PR #157, conflict on `helpers_tests.rs`

The conflict file is `crates/jcode-tui/src/tui/app/helpers_tests.rs`.

Master already uses `program.file_stem()`, with the comment *"Compare the stem: the
launcher binary is kcode.exe on Windows"*. That master form is **newer** and should be
kept.

Resolution procedure:

1. **Take the PR's `helpers.rs` change.** 1 line, a `jcode` to `kcode` decollision in
   the turn-notification title.
2. **Take the PR's `turn_notify.rs` change.** 10 lines, the same decollision plus the
   resume-command binary name.
3. **DISCARD the PR's `helpers_tests.rs` change.** Master already has the newer
   `file_stem()` form with its explanatory comment. Keeping the PR's version would
   regress to the older comparison.

Net result after resolution: a 11-line effective diff across 2 files.

### 4.2 PR #158, conflict on `kv_cache_problem.rs`

The conflict file is `crates/jcode-tui/src/tui/kv_cache_problem.rs`.

Master already carries the correct behavior, expressed as `!info.is_cold &&
!info.is_estimate`. The regression test `cache_estimate_is_not_evidence_of_an_expected_warm_hit`
already exists on master at `crates/jcode-tui/src/tui/mod.rs:1887`. The PR's only real
contribution is an explanatory comment plus a reorder of the two conditions.

Recommendation: **close #158.** Merging it costs a conflict resolution and buys a
comment. The alternative, if the comment is considered valuable, is to resolve the
conflict by hand and keep **only** the comment, discarding the reorder.

---

## 5. CI gate status

### 5.1 Format / Quality Guardrails - OPEN

**This gate is open and its outcome is not yet known.** A parallel audit is still
running to determine whether master itself still carries rustfmt or clippy drift.
This document therefore makes **no claim that master is fmt-clean**.

What can be stated precisely:

- PR #156's own 8-file set is already **byte-identical to master**. Whatever drift
  #156 was written to fix, its intended file set has already been reconciled.
- The master-side drift question is a **separate** question from #156's residual, and
  is being audited independently.
- Until that audit returns, the operator should treat this gate as **UNKNOWN**, not
  passing and not failing.

### 5.2 TypeScript SDK

- #148 fixed fixture paths but not stale assertions. #160 completes the job.
- **Baseline on master: 82 tests, 81 pass, 1 fail.**
- That single failure is **macOS-only**:
  `login inheritance creates file links, never credential directory links`.
  The production darwin branch ignores `XDG_CONFIG_HOME`, so the test passes on Linux CI.
- **#160's branch measures the same 82 / 81 / 1**, with the same single macOS-only
  failure. #160 does not regress anything.
- `npm run typecheck` exits 0 on the #160 branch.

### 5.3 Pre-existing failures, OUT OF SCOPE

Do not chase these. They are unrelated to this stack and were present before it:

- macOS provider-matrix failures.
- Setup Friction Eval, caused by the KCode release-download stub.
- Windows and setup failures.
- Unrelated provider failures.

### 5.4 Category A TUI fork-command gap - OPEN implementation task

Three tests **still fail on master**:

- `test_fork_command_with_prompt_forks_session`
- `test_fork_command_without_prompt_forks_idle_session`
- `test_btw_command_forks_session_with_question`

They fail at `crates/jcode-tui/src/tui/app/tests/commands_accounts_01/part_01.rs:1206`
with `left: "error"`, `right: "system"`. The fork handler pushes an error message
where the test expects a system message.

Issue #135 recommends an **implementation** fix, not a test change: emit the
`kcode --resume <new-id>` hint.

**No PR exists for this yet.** It is recorded here as an open implementation task for
a future agent.

### 5.5 Stray branch, must be deleted

`fix/tui-refactor-regressions` exists on origin at `0f4afc872`, based on the **old**
master `b9886143c`. Merging it would **revert upstream #124's Windows keyboard work**
and **delete** `crates/jcode-tui/src/tui/keyboard_protocol.rs`.

**It must not be merged. The operator should delete the branch.** Agents do not delete
branches.

---

## 6. Operator action list

### 6.1 Merge, in this order

```
1. #127  fix/update-product-warnings
2. #140  fix/setup-hints-pr128-test-regression
3. #152  fix/cli-exit-status-kcode-bin          (real fix, see section 2.1)
4. #160  fix/sdk-typescript-kcode-assertions   (subsumes #148)
5. #157  fix/tui-kcode-decollision            (resolve conflict per section 4.1 first)
6. #145  docs/orchestration-governance-docs-only
7. #156  fix/ci-rustfmt-clippy-drift-clean     OPTIONAL, empty residual
8. #150  fix/tui-cache-expected-warm-unused-import   OPTIONAL, empty residual
```

Steps 3 and 4 are independent and may be swapped. Steps 7 and 8 may be skipped
entirely.

### 6.2 Close

```
#148   superseded by #160 (strict subset)
#137   superseded by #140
#144   superseded by #145
#129   superseded by #156
#132   superseded by #157
#134   superseded by #158
#158   superseded, behavior already on master (see section 4.2)
#156   OPTIONAL close, empty residual (see section 5.1)
```

Each of `#129`, `#132`, `#134`, `#137`, `#144`, `#148` already carries a verified
"Closed by #NNN" supersession comment on GitHub.

### 6.3 Delete branch

```
fix/tui-refactor-regressions   at 0f4afc872   MUST NOT be merged (section 5.5)
```

### 6.4 Open implementation task

- Category A TUI fork-command gap, issue #135. No PR exists (section 5.4).

### 6.5 Awaiting the parallel audit

- Master-side rustfmt / clippy drift determination (section 5.1).

### 6.6 Division of responsibility

**The agent does not merge PRs, does not close PRs, and does not delete branches.**
Every item in section 6 is an operator action.

---

## 7. Preserved strings

The following `jcode` occurrences are **intentional and must not be "fixed"**.
A mechanical find-and-replace over these strings is a known trap.

| String | Why it stays |
|---|---|
| `jcode_not_found` | Stable public error code. `sdk/typescript/src/launch.ts:727` throws exactly this, mirrored by `crates/jcode-sdk/src/errors.rs:56`. Callers branch on it. |
| `crates/jcode-harness-api/` | The crate directory really is named that. Changing the path breaks the test. |
| `jcodeHome` | Still the SDK option name. `sdk/typescript/src/launch.ts:163`, `:209`, `sdk/typescript/src/client.ts:274`. |
| `jcode-transport` | The Rust module name, `crates/jcode-transport/`. |
| `jcode-api` pipe stem | Pinned by `crates/jcode-transport/src/windows.rs`. The TS SDK mirrors that derivation in `sdk/typescript/src/sockets.ts`. |
| `JCODE_SDK_TEST_BINARY`, `JCODE_SCRATCH_DIR`, `JCODE_DISABLE_TELEMETRY` | Opt-in `live-*.mjs` script-local env vars, not a production contract. |

Contrast with `JCODE_HOME`, which **is** a real defect in exactly one Rust file. See
section 2.1.

---

## 8. Binding hard constraints

These are binding for every step in this document and for any agent acting on it.

Forbidden:

```
git push --force
git push --force-with-lease
git reset --hard
git reset HEAD~
git clean -fd
git branch -D
gh pr merge
gh pr close
gh repo delete
gh repo create
```

- **No force-push.** The docs branch is based on the old master `b9886143c`, so the
  push is a normal fast-forward of the branch's own commits. Do **not** rebase, do
  **not** merge master into the branch, do **not** force-push.
- **No rebase of published branches.** History is an immutable ledger.
- **Every agent commit carries ledger trailers.** At minimum `tx-agent` and
  `tx-validated`.

---

## 9. TUI verification method

Reproduced on a clean `origin/master` worktree. Per-module filters are used to isolate
subsystems.

```
cargo test -p jcode-tui
cargo test -p jcode-tui session_picker
cargo test -p jcode-tui app::tests
cargo test -p jcode-tui ui_prefs
```

**Master result: 2293 passed / 219 failed / 19 ignored.**

The 219 failures are environment-dependent local failures, notably:

- `session_picker::loading::*`, which are filesystem-dependent.
- `app::tests::test_poke_*`.
- `ui_prefs::tests`, which are disk-dependent.

These are **not** the issue-135 regressions.

Of the 21 issue-135 tests: **18 pass on master, 3 fail** (the Category A fork-command
tests from section 5.4).

### 9.1 Why 8 of the 21 now pass without any local PR

Upstream #124 already landed these behaviors:

| Behavior | Location on master |
|---|---|
| Keyboard-enhancement assertions (`\x1b[>1u` / `\x1b[=1u`) | `crates/jcode-tui/src/tui/mod.rs:272-292` |
| Disconnect glyph, `⚡ Connection lost - retrying` | `crates/jcode-tui/src/tui/app/remote/reconnect.rs:117` |
| `kcode --resume` hint | `crates/jcode-tui/src/tui/app/commands.rs:460`, `:467`, `:2523`, and `commands_review.rs:768` |
| Session-picker height 24 | `crates/jcode-tui/src/tui/session_picker_tests.rs:1983`, `:2039`, `:2103`, `:2167`, `:2534` |
| Overscroll row computation `height-2` when width is 80 or greater | compact status bar renders exactly when `chat_area.width >= 80`, at `crates/jcode-tui/src/tui/ui.rs:3042`, with the test at `crates/jcode-tui/src/tui/ui_tests/swarm_buffer.rs:556-558` |

The compact-layout threshold for session picker height is `height < 20` at
`crates/jcode-tui/src/tui/session_picker.rs:2200`.

---

## 10. Verification commands

### 10.1 Recomputing a PR's true residual

```
git fetch origin

# Merge-tree gives the resolved tree (or reports the conflict).
git merge-tree --write-tree origin/master origin/<branch>

# Diff that tree against master. Empty output means a no-op PR.
git diff --stat origin/master <tree-sha>
```

For #157 and #158, `git merge-tree --write-tree` reports a conflict. In that case the
true residual is computed by resolving the conflict locally and diffing the result.

### 10.2 Confirming the merge list

```
gh pr list --repo KooshaPari/KCode --state open \
  --json number,title,headRefName,mergeable
```

Always pass `--repo KooshaPari/KCode` explicitly. In a fork, `gh` resolves to the
upstream remote by default.

### 10.3 Rust

```
cargo test -p jcode-tui
cargo test -p jcode-tui session_picker
cargo test -p jcode-tui app::tests
cargo test -p jcode-tui ui_prefs
```

### 10.4 TypeScript SDK

```
cd sdk/typescript
npm test
npm run typecheck
```

Expected: 82 tests, 81 pass, 1 fail. The single failure is macOS-only
(`login inheritance creates file links, never credential directory links`) and passes
on Linux CI.

### 10.5 Verifying no stray `JCODE_HOME` in Rust

```
git grep -n 'JCODE_HOME' -- '*.rs'
```

Expected on master: exactly one hit, `tests/cli_exit_status.rs`. Expected after
merging #152: zero hits.