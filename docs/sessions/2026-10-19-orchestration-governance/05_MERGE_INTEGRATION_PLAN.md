# Merge Integration Plan - KCode PR Stack (2026-10-21T05:46Z)

Status: ready for operator review | Date: 2026-10-21 | Author: subagent (merge triage)
Session: `docs/sessions/2026-10-19-orchestration-governance/`
Repo: `KooshaPari/KCode` @ `2aea04dd871f21da5c5b21800d156a2c9b0830b1`
Local clone: `/Users/kooshapari/CodeProjects/Phenotype/repos/jcode`
Supersedes: the 2026-10-19 edition of this document (written against `b9886143c`)
and the intermediate `docs(ci)` rebuilds at `86c1c2896`, `5d8a0b6fa`,
`d18d5faeb`, `88e64119a`.
Companion evidence file: `05a_MERGE_EVIDENCE.md` (applied conflict
resolutions with merge commit SHAs, and the 2026-10-21 runner wedge
root-cause analysis).

---

## 1. What changed since 2026-10-19

Master advanced from `b9886143c` to `2aea04dd8`. Two upstream PRs landed in that
window and they absorbed a large slice of the local fix stack:

| Upstream PR | Title | Effect on this plan |
|---|---|---|
| #124 | Windows keyboard negotiation | Added `crates/jcode-tui/src/tui/keyboard_protocol.rs`. Its TUI assertions landed on master, so 8 of the 21 issue-135 tests now pass without any local PR. |
| #138 | Portal XJ-03 bridge | Lands independently of this stack. No direct file overlap with the local PRs below. |

In the same window the four PRs that previously required the operator to
hand-resolve merge conflicts were **already merged against origin/master** by a
parallel agent. Each of `#140`, `#127`, `#157`, `#158` now has a real
2-parent merge commit whose second parent is `origin/master`, and
`git merge-tree --write-tree origin/master origin/<branch>` reports
**ZERO conflicts and ZERO conflict markers** for every one of them. The
hand-resolution instructions in the 2026-10-19 edition of this document are
**obsolete**; section 4 records the resolutions as APPLIED.

Also new:

- The PR stack grew from 10 to **12**. `#163` (Windows transport pipe-fallback
  stem) and `#164` (CI compile-timeout headroom) were opened.
- The CI clippy gate has a **coverage gap** that affects every PR in the stack
  but is not caused by any of them (section 5.1). A separate agent owns the
  fix; do not duplicate it.
- The KCode self-hosted runner `desk-kcode-1` wedged on 2026-10-21 with a
  real root cause in the WSL2 memory budget. The wedge is recorded in
  section 5.6 for future operators.

Every file and line count in section 2 is a **true residual** against
`origin/master` at `2aea04dd8`, computed with `git merge-tree --write-tree`
+ diff against `origin/master^{tree}` (the procedure in section 10). For the
four previously-conflicting PRs, the merge-tree run is on the **already-merged**
branch tip, not on the original branch head.

---

## 2. Current stack

All residuals computed against `origin/master` = `2aea04dd8`. "Conflict-free"
means `git merge-tree --write-tree origin/master origin/<branch>` reports
zero conflict markers (post-merge for the four re-merged PRs).

| PR | Branch | Files | +lines | Conflict-free? | Verdict |
|---:|---|---:|---:|---|---|
| #160 | `fix/sdk-typescript-kcode-assertions` | 7 | +18 | yes | **MERGE.** Recommended SDK PR. Strict superset of #148. |
| #152 | `fix/cli-exit-status-kcode-bin` | 1 | +3 | yes | **MERGE.** Real env-var fix; see section 2.1. |
| #145 | `docs/orchestration-governance-docs-only` | 2 | +675 | yes | **MERGE.** This document. |
| #140 | `fix/setup-hints-pr128-test-regression` | 2 | +17 | yes (resolved) | **MERGE.** Conflicts applied at `d6e4cfac2`. |
| #127 | `fix/update-product-warnings` | 1 | +1 | yes (resolved) | **MERGE.** Comment-only now; conflicts applied at `b6508e696`. |
| #157 | `fix/tui-kcode-decollision` | 1 | +4 | yes (resolved) | **MERGE.** Conflicts applied at `264c09a10`. |
| #158 | `fix/tui-cache-estimate-not-evidence` | 1 | +5 | yes (resolved) | **MERGE.** Comment-only now; conflicts applied at `47b42e24d`. |
| #163 | `fix/transport-pipe-fallback-stem` | 1 | +1 | yes | **MERGE.** Windows pipe fallback assertion. |
| #164 | `fix/ci-compile-timeout-headroom` | 1 | +44 | yes | **MERGE.** CI timeout/cache budget bump. |
| #150 | `fix/tui-cache-expected-warm-unused-import` | 0 | 0 | yes | **CLOSE.** True no-op vs current master. |
| #156 | `fix/ci-rustfmt-clippy-drift-clean` | 0 | 0 | yes | **CLOSE.** True no-op; see section 5.1. |
| #148 | `fix/sdk-test-paths-kcode-prefix` | 2 | +4 | yes | **CLOSE.** Superseded by #160 (strict subset). |

### 2.1 The `JCODE_HOME` finding

This is the one remaining item in the whole stack that is a genuine fix rather than a
rename already done. Verified across the entire Rust tree on master: `KCODE_HOME` is
used everywhere across dozens of crates. `JCODE_HOME` survives in **exactly one Rust
file**, `tests/cli_exit_status.rs`. One further mention lives in
`crates/jcode-telemetry-core/CONCURRENCY.md`, which is documentation prose.

That is precisely the 3-line residual #152 still carries. The
`env!("CARGO_BIN_EXE_kcode")` half of #152 is already on master. Only the
env-var half remains. It should be merged as a real fix, not dismissed as
cosmetic.

### 2.2 Why the empty residuals are closing, not merging

`#150` and `#156` each have a true residual of 0 files / 0 lines against
master. Merging them would be a strict no-op and would only add a merge
commit to the history. The 2026-10-19 edition listed them as "optional" -
this edition recommends closing them outright. The behavior they were
written to introduce is already on master.

`#148` is in a third category: its content is **inside** `#160`'s 7-file
residual as a strict subset. Closing `#148` and merging `#160` is the
correct outcome; merging `#148` after `#160` would be a strict no-op.

---

## 3. Recommended merge order

The order is chosen so that no PR is merged into a tree that a later PR
then invalidates. Every PR in this list is small, disjoint, or already
resolved; the order is about visibility, not dependency resolution.

| Step | PR | Why here |
|---:|---|---|
| 1 | #160 | SDK tests, 7 files under `sdk/typescript/`. No Rust. Closes #148 by superset. |
| 2 | #152 | `tests/cli_exit_status.rs` only. Disjoint from #160; order with it is presentation-only. |
| 3 | #145 | Docs only. Zero code interaction. Safe at any position; placed here after the two small Rust touches. |
| 4 | #140 | `jcode-setup-hints` is a workspace edge; merging it before the TUI back-end changes keeps the TUI PRs on a tree that already matches `kcode` naming. |
| 5 | #127 | 1-line comment-only in `jcode-app-core`. Disjoint from everything below. |
| 6 | #157 | 1-file TUI rename, already resolved (`264c09a10`). |
| 7 | #158 | 1-file TUI comment, already resolved (`47b42e24d`). |
| 8 | #163 | 1-line transport pipe-stem assertion. Disjoint from all PRs above. |
| 9 | #164 | CI workflow only. Last so the workflow file is the most-recent touched file on master before push. |

`#148` is **superseded by #160** and must NOT be merged before #160; if
#148 lands alone it is a harmless but redundant intermediate state.

### 3.1 Independence statements

- **#160 and #152 touch disjoint trees.** One is `sdk/typescript/**`, the
  other is `tests/cli_exit_status.rs`. Either order is fine.
- **#160 subsumes #148.** #148's residual is a strict subset of #160's.
  Recommend merge #160, close #148.
- **#140, #127, #157, #158, #163** are on disjoint files. Order between
  them is presentation-only.
- **#145** (this PR) is `docs/` only; safe at any position; placed here
  for diff-history readability.
- **#164** is the only PR that touches `.github/workflows/ci.yml`. It
  belongs late so it does not gate PRs whose CI we want to run on the
  existing matrix.

---

## 4. Conflict-resolution guidance

All four conflicts that the 2026-10-19 edition told the operator to
hand-resolve have already been resolved by a parallel agent and landed
as 2-parent merge commits on the respective branches. **No manual
conflict resolution is needed at operator time.**

| PR | Branch | Resolved at | Status |
|---:|---|---|---|
| #140 | `fix/setup-hints-pr128-test-regression` | `d6e4cfac2` | `with_spawn_notice` DELETED. |
| #127 | `fix/update-product-warnings` | `b6508e696` | Comment-only now. |
| #157 | `fix/tui-kcode-decollision` | `264c09a10` | Master wins on `file_stem()`. |
| #158 | `fix/tui-cache-estimate-not-evidence` | `47b42e24d` | Comment-only now. |

Full resolution text, file paths, per-PR verification commands, and
the canonical re-verification recipe live in
[`05a_MERGE_EVIDENCE.md`](./05a_MERGE_EVIDENCE.md) section 1.

---

## 5. CI gate status

### 5.1 Format / Quality Guardrails - PASS for 93 of 94 workspace members

Verified against the green CI run GitHub Actions run id `39390720989`,
job `Quality Guardrails`:

- The clippy step runs `cargo clippy --all-targets --all-features -- -D warnings`.
- That command has **NO `--workspace`**. cargo therefore builds only the
  dependency graph reachable from the root package.
- The step checked **93 crates**. The workspace has **94 member
  packages** (93 crates + the root package `kcode`).
- **EXACTLY ONE workspace member is never linted: `jcode-sdk`**
  (`crates/jcode-sdk`). It is a workspace member with NO dependents, so
  it is unreachable from the root package.
- The gap was real, not theoretical: in the same green run, the step
  `cargo test -p jcode-sdk parity -- --nocapture` compiled `jcode-sdk`
  and emitted:
  `warning: type alias 'Subscriber' is never used --> crates/jcode-sdk/src/client.rs:330:6`
  `warning: 'jcode-sdk' (lib) generated 1 warning`
- The dead alias at `crates/jcode-sdk/src/client.rs:330`:
  `type Subscriber = (u64, Option<String>, Sender<ApiEvent>);`
  is never used anywhere in the crate (the only occurrence of the
  identifier `Subscriber` is its own definition; `SubscriberList` is a
  different, used identifier).
- The dead alias is present on EVERY open branch, so no existing PR
  fixes it.

**OPEN RISK.** A separate agent is opening a PR that (a) removes the
dead alias and (b) changes the clippy command to
`cargo clippy --workspace --all-targets --all-features -- -D warnings`.

The `--workspace` change is **NOT a no-op**. It may surface additional
pre-existing clippy violations in other crates. Those are NOT yet known
as of 2026-10-21T05:46Z. Treat them as **UNVERIFIED** until the
clippy-coverage PR runs a full CI gate and the gate is green. **This
document does not assert that `--workspace` clippy passes.**

Until that PR lands, #156 remains a true no-op: master Rust source is
byte-identical to the last green CI source, and the existing
(non-workspace) clippy check is green for the crates it covers.

### 5.2 TypeScript SDK

- #148 fixed fixture paths but not stale assertions. #160 completes the job.
- Baseline on master: 82 tests, 81 pass, 1 fail.
- That single failure is macOS-only:
  `login inheritance creates file links, never credential directory links`.
- The 82 / 81 / 1 baseline is unchanged on the #160 branch.
  #160 does not regress anything.
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

`fix/tui-refactor-regressions` exists on origin at `0f4afc872`, based on the old
master `b9886143c`. Merging it would revert upstream #124's Windows keyboard work and
delete `crates/jcode-tui/src/tui/keyboard_protocol.rs`. **It must not be merged.
The operator should delete the branch.** Agents do not delete branches.

### 5.6 Runner wedge 2026-10-21

The KCode repo's only self-hosted runner `desk-kcode-1` wedged on
2026-10-21 in the `Build & Test (desk-linux)` job of run `39530002307`
for 1h38m. **Root cause: the desk WSL2 instance is capped at 16 GB**
(`memory=16GB` in `.wslconfig`), not the 64 GB the Windows host has.
Under concurrency the box hit 13.9 / 16 GB with no swap, plus a 1h43m
orphaned coverage job competing for the same `CARGO_TARGET_DIR`; the
`Build` step has no per-step `run_with_timeout.py` wrapper, so a
starved Build serializes the whole queue until the 120-minute job cap.

Remediation applied: the orphaned coverage job was terminated on the
desk node. Memory fell from 13.9 / 16 GB to 9.4 / 16 GB and load fell
from ~47 to ~14. Standing risks: the WSL2 16 GB cap, the missing
per-step `Build` timeout, and the single-runner serial-CI topology.

Full root-cause analysis, before/after memory counters, evidence
commands, and the standing-risk recommendations live in
[`05a_MERGE_EVIDENCE.md`](./05a_MERGE_EVIDENCE.md) section 2.

---

## 6. Operator action list

### 6.1 Merge, in this order

```
1. #160  fix/sdk-typescript-kcode-assertions           (subsumes #148)
2. #152  fix/cli-exit-status-kcode-bin                 (real fix, section 2.1)
3. #145  docs/orchestration-governance-docs-only       (this PR)
4. #140  fix/setup-hints-pr128-test-regression         (resolved at d6e4cfac2)
5. #127  fix/update-product-warnings                   (resolved at b6508e696, comment-only)
6. #157  fix/tui-kcode-decollision                     (resolved at 264c09a10)
7. #158  fix/tui-cache-estimate-not-evidence           (resolved at 47b42e24d, comment-only)
8. #163  fix/transport-pipe-fallback-stem              (new)
9. #164  fix/ci-compile-timeout-headroom               (new)
```

No manual conflict resolution is required for steps 4-7. All four
have already been resolved by a parallel agent; section 4 records what
was applied and the merge commit SHA.

Steps 1 and 2 are independent and may be swapped. Steps 1 and 9
modify disjoint files; any order between them is a presentation
choice, not a correctness one.

### 6.2 Close

```
#148   superseded by #160 (strict subset)
#150   empty true residual (the import it claimed to remove is already on master)
#156   empty true residual (master source is byte-identical to last green CI source)
```

### 6.3 Delete branch

```
fix/tui-refactor-regressions   at 0f4afc872   MUST NOT be merged (section 5.5)
```

### 6.4 Open implementation task

- Category A TUI fork-command gap, issue #135. No PR exists (section 5.4).

### 6.5 Watch list

- **Clippy coverage gap on `jcode-sdk`** (section 5.1): the
  `--workspace` clippy the new PR adds is **UNVERIFIED** on master;
  the wider check may surface pre-existing violations in other crates.
- **Runner `desk-kcode-1` wedge** (section 5.6): single runner,
  WSL2 16 GB cap, missing per-step `Build` timeout are standing risks.
- **After PR #164 merges**: re-verify Build step timeout behavior;
  the step still has no per-step `run_with_timeout.py` wrapper.

### 6.6 Division of responsibility

**The agent does not merge PRs, does not close PRs, and does not delete branches.**
Every item in this section is an operator action.

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

- **No force-push.** The docs branch is based on the old master `b9886143c`,
  so the push is a normal fast-forward of the branch's own commits. Do
  **not** rebase, do **not** merge master into the branch, do **not**
  force-push.
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

### 10.1 Authoritative method: a PR's TRUE residual

The two-dot diff against a stale branch head is **WRONG** for this stack.
A branch like `fix/update-product-warnings` may have been rebased or
re-merged multiple times since it was opened; a `git diff origin/master
origin/<branch>` against the current head understates the *current*
residual because the branch already had to diverge once to apply the
merge.

The authoritative procedure is `git merge-tree --write-tree` + diff
against `origin/master^{tree}`:

```
git fetch origin

# Merge-tree gives the resolved tree (or reports the conflict).
# For the four re-merged PRs (#140, #127, #157, #158) this reports a
# CLEAN merge; do not mistake "clean merge" for "empty diff".
mt=$(git merge-tree --write-tree origin/master origin/<branch> | head -1)

# Diff that tree against master. Empty output means a true no-op PR.
git diff --shortstat origin/master^{tree} "$mt"

# What changed between master and the merge-tree result, file by file:
git diff --stat origin/master^{tree} "$mt"
```

A "0 files" row in section 2 is the assertion that
`git diff --shortstat origin/master^{tree} "$mt"` returned empty
output. The PR is then a true no-op against current master and can be
**closed** rather than merged.

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

### 10.6 Verifying the clippy coverage gap

```
git grep -n 'type Subscriber' crates/jcode-sdk/src/client.rs             # expect: 1 hit on master
cargo clippy --workspace --all-targets --all-features -- -D warnings 2>&1 | head -50
```

The first confirms the dead alias (closed by the clippy-coverage PR);
the second is the post-merge wider gate, UNVERIFIED on current master
and possibly surfacing pre-existing violations.
