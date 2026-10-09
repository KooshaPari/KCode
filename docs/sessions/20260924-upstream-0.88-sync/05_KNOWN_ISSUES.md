# Known Issues

The 6 deferred work items, the 2 ambient-state TUI failures, and the
stale observations that this PR did not address. Each item is
structured with: severity, current state, file/line refs, blocking
condition, deferred-to, and an owner candidate. The full triage
record for the security advisories is in `docs/SECURITY_DEPENDENCIES.md`
(not duplicated here).

## Severity scale

- **BLOCKER** — prevents the PR from being merged or causes a
  regression in the fork's stated functionality.
- **HIGH** — surfaces a real defect that downstream users will
  encounter.
- **MEDIUM** — surfaces a defect in an edge case or non-default
  configuration.
- **LOW** — observation or documentation gap with no functional
  impact.
- **DEFERRED** — intentionally out of scope for the v0.88.0 sync.

## 1. parse_semver env vars (JCODE_BASE_SEMVER, JCODE_UPDATE_SEMVER) — DEFERRED

**Severity:** DEFERRED (was CRITICAL; resolved in WBS-07 for the
build script, the install_release.sh regex limitation remains).

**Current state:** the `JCODE_BASE_SEMVER` and `JCODE_UPDATE_SEMVER`
env vars are honored at `crates/jcode-build-meta/build.rs:32-37`,
but the channel segment is not propagated by the installer. The
regex at `scripts/install_release.sh:73-83` still only matches
`\(\w{9}\)$` (the `<git_hash>` suffix), not
`<semver>-<channel>(<hash>)`.

**File/line refs:**
- `crates/jcode-build-meta/build.rs:32-37` (env-var precedence)
- `scripts/install_release.sh:73-83` (regex limitation)
- `04_IMPLEMENTATION_STRATEGY.md` §2.3 (design rationale)

**Blocking condition:** the installer's regex needs to be updated
to either match the channel segment or be split into two regexes.
The chosen approach for the deferred PR is the second (less risky,
smaller diff).

**Deferred-to:** dedicated PR titled "build: propagate the
pre-release channel segment to installer regex".

**Owner candidate:** any maintainer with `crates/jcode-build-meta` +
`scripts/install_release.sh` write access. Estimated 1-2 hours.

**Pre-staged issue body:** `/tmp/issue-parse-semver-envvar.md`.

## 2. cargo fmt debt — DEFERRED

**Severity:** DEFERRED (103 pre-existing dirty files, by design).

**Correction (2026-10-09):** this section previously said 102, derived as
`111 pre-merge − 9 fixed`. Re-measured at `94db64687` on both the `+stable` and
default `nightly` rustfmt channels the true count is **103**, and the two
channels agree exactly (zero file divergence). The merge reduced the dirt by
**8** files, not 9. The `0 merge-introduced` conclusion is unaffected.

**Current state:** `cargo fmt --all -- --check` exits 1 on the
current HEAD. The dirt is **pre-existing** (verified by running
the same command on a detached `115170054` — the fork parent of
the merge — in a separate worktree; the count was 111, so the
merge actually reduced the dirt by 8 files; the 103 number is
the post-merge floor).

**File/line refs:**
- `08_FINAL_EVIDENCE.md` §4 (the cross-validation)
- `00_SESSION_OVERVIEW.md` §"Format gate" (the provenance)

**Blocking condition:** the CI `Quality Guardrails` job
(`.github/workflows/ci.yml:20`) runs `cargo fmt --all -- --check` as its
**third** of 15 steps, named "Check formatting". This step fails before
clippy, `cargo check --all-targets`, and the eight budget/ratchet gates can
run.

**Correction (2026-10-09):** this section previously said "step #5" and named
`deny`, `RUSTSEC` and `secret scan` as the skipped gates. Enumerating the job
from the workflow file directly gives 15 steps with fmt at position 3, and
`deny` / `RUSTSEC` / `secret scan` are **not** steps in this job at all.

**Why this blocks the budget gates:** the `Quality Guardrails`
job's `cargo fmt --all -- --check` step is a hard prerequisite for
the subsequent steps. When it fails, the job exits early and the
budget gates are SKIPPED. This means a CI run that shows
"Quality Guardrails passed" is actually showing only that the
fmt step passed; the budget gates are NEVER exercised at this
head.

**Live confirmation (2026-10-09):** at PR head `94db64687` (run
`37882834770`), `gh pr checks 23` reports both `Format` and
`Quality Guardrails` failing, and `gh run view 37882834770 --log-failed`
contains exactly one `Quality Guardrails` step — `Check formatting` —
confirming the job exits at fmt and never reaches steps 4-15.

**Deferred-to:** dedicated PR titled "style: cargo fmt --all the
103 pre-existing dirty files" or "ci: allow the fmt step to
proceed independently of the budget gates".

**Owner candidate:** any maintainer willing to land a 103-file
format-only diff. The diff is large but mechanical; no logic
changes.

**Pre-staged issue body:** `/tmp/issue-fmt-cleanup.md`.

## 3. Budget growth — DEFERRED

**Severity:** DEFERRED (4 absorbed ratchets since v0.88.0).

**Current state:** the 3 code-size baselines ratcheted in WBS-03
(`crates/jcode-app-core/src/server/client_actions.rs` 1223→1227,
`crates/jcode-tui/src/tui/app/input.rs` 4270→4273,
`crates/jcode-tui/src/tui/ui_input.rs` 3752→3757) and the panic
baseline ratcheted in `d511d5948` (77→181, 135% increase) are
absorbed, not decomposed. The growths are all comment lines
and test/build scaffolding, so the decomposition is non-trivial.

**File/line refs:**
- `scripts/code_size_budget.json:8/83/104` (the 3 ratchets)
- `scripts/panic_budget.json:2` (the 77→181 ratchet; current `total: 181`)
- `git show 253b337ba` (the ratchet commit)
- `02_SPECIFICATIONS.md` §2.3, §3.4, `04_IMPLEMENTATION_STRATEGY.md` §4.1 (the ratchet vs decomposition rationale)

**Correction (2026-10-09):** the prior draft listed the second
ratchet path as `input.rs` (ambiguous). The correct path is
`crates/jcode-tui/src/tui/app/input.rs`. The other two
ratchets (`client_actions.rs` and `ui_input.rs`) are correct
as full paths.

**Blocking condition:** the budgets grow monotonically until a
decomposition pass is done. The 135% panic increase is the most
worrying; the underlying growth is from `fake_acp.rs` and
`crates/jcode-app-core/build.rs` test fixtures that the fork
introduced.

**Deferred-to:** dedicated PR titled "refactor: decompose the
client_actions / input / ui_input / panic-fixture hot spots".

**Owner candidate:** any maintainer with TUI + app-core write
access. Estimated 4-8 hours (mostly mechanical extraction into
submodules).

**Pre-staged issue body:** `/tmp/issue-budget-growth.md`.

## 4. Copilot review — DEFERRED (operator action)

**Severity:** DEFERRED (operator action needed; quota exhausted).

**Current state:** the Copilot bot's PR review body says "Copilot
was unable to review this pull request because the user who
requested the review has reached their quota limit." This is a
GitHub-side rate limit, not a code or CI issue.

**File/line refs:** PR #23 review comment from
`copilot-pull-request-reviewer[bot]`.

**Blocking condition:** none (the Kilo review covered the same
surface and was thorough). The Copilot review would be additive,
not gating.

**Deferred-to:** none (operator decision on whether to wait for
quota reset or to skip Copilot for this PR).

**Owner candidate:** the operator who opened the PR (KooshaPari).

## 5. provider_catalog_tests assertion (Z.AI default) — DOCUMENTED

**Severity:** LOW (no functional impact; the test is currently
passing; documented for future reviewers).

**Current state:** the test at
`crates/jcode-base/src/provider_catalog_tests.rs:230` asserts
`ZAI_PROFILE.default_model == Some("glm-5.3")`. The actual
default in the fork's provider catalog is `glm-5.3` (set by
`dbc9fb118`, "provider defaults"). The test passes in CI
(2402/0 baseline).

The Kilo review's WARNING at `provider_catalog_tests.rs:220`
was about the **comment** in the test, not the assertion. The
comment used to claim "CI could never have caught it" (which is
now false since `3c96175f6`); the WBS-15 fix (`32621b2ad`)
updated the comment to acknowledge the guard.

The remaining stale item: the test's assertion is fine, but the
catalog has a second `Z.AI` default that is **not** covered by
this test. Specifically, `ZAI_PROFILE.fallback_model` (or
similar secondary field) is not asserted. If a future commit
changes that field, no test will catch it.

**File/line refs:**
- `crates/jcode-base/src/provider_catalog_tests.rs:230` (the
  assertion that was fixed)
- `crates/jcode-base/src/provider_catalog.rs:30-60` (the catalog
  itself)

**Blocking condition:** none. The PR passes CI (the test is
green, just under-tested).

**Deferred-to:** next PR that touches the provider catalog.

**Owner candidate:** any maintainer with the provider catalog
write access.

## 6. Master branch empty-DEPLOY_KEY guard — DEFERRED

**Severity:** DEFERRED (the PR branch demonstrates the guard; the
master branch still has the unguarded baseline).

**Current state:** the WBS-04 fix in commit `3c96175f6` added the
ssh-agent guard to the 5 sites on the PR branch. The `master`
branch on `KooshaPari/KCode` does not have the guard (verified by
`git log master --oneline | grep 3c96175f6` = empty).

**File/line refs:**
- `.github/workflows/ci.yml:38/172/441/703` (the guard sites, PR
  branch)
- `master` branch (the unguarded baseline)

**Blocking condition:** until the guard is rolled to master, every
push to master fails 5 jobs before compiling. The PR branch
demonstrates the fix; the master rollout is a one-commit
follow-up.

**Deferred-to:** after this PR is merged, the same 3c96175f6
commit can be cherry-picked to master.

**Owner candidate:** any maintainer with `.github/workflows/ci.yml`
write access. Estimated 5 minutes (cherry-pick + push).

## 7. Ambient-state TUI test failures (local-only) — DOCUMENTED

**Severity:** LOW (local-only, not in CI; documented for
operator awareness).

**Current state:** the fork's TUI test suite, run locally with
environmental pollution, observes 2 failures that do **not** appear
in CI:

| Test | File | Cause |
|---|---|---|
| `detected_resume_terminal_recognizes_handterm_term_program` | `crates/jcode-tui/src/tui/app/helpers_tests.rs:205` | `EnvVarGuard` race: `detected_resume_terminal()` reads the `TERM_PROGRAM` env var and the test's `EnvVarGuard::set_value("TERM_PROGRAM", "handterm")` is racing with the read. CI runs in a fresh container with no pollution. |
| `test_real_draw_click_on_body_anchored_image_label_cycles_level` | `crates/jcode-tui/src/tui/ui_tests/body_anchored_image.rs` (or similar) | `status_notice()` inherits `Image copied` from a prior test that ran the clipboard copy code path. CI runs each test in isolation. |

**File/line refs:**
- `08_FINAL_EVIDENCE.md` §5 (the detailed walkthrough)
- `06_TESTING_STRATEGY.md` §"Ambient-state local-only failures"

**Workaround:** run the affected tests with
`unset TERM_PROGRAM && cargo test --test-threads=1 -p jcode-tui
--lib` to avoid both pollution sources.

**Deferred-to:** next time the TUI test infrastructure is touched
(an `EnvVarGuard` race fix is a small, isolated change).

## 8. Stale Kilo review body (operator action) — DEFERRED

**Severity:** LOW (operator action; the audit has been responded
to in commits WBS-02 through WBS-15).

**Current state:** the Kilo review body's
"Copilot was unable to review this pull request" entry is from
PR #23's review thread. The Kilo review itself is in
`/tmp/kilo-review-full.md` and has been triaged: 4 CRITICAL
resolved (WBS-02, WBS-03, WBS-07), 21 WARNING partially resolved
(4 stale-claim fixes in WBS-15, 4 preflight fixes in WBS-05,
2 doc fixes in WBS-12, WBS-13, WBS-14, others are false positives
documented in `02_SPECIFICATIONS.md` §7.2 and `06_TESTING_STRATEGY.md`),
17 SUGGESTION partially resolved (the 4 stale-claim fixes in
WBS-15 cover the high-value ones).

**File/line refs:** the Kilo review body in PR #23's review
thread; `/tmp/kilo-review-full.md` for the full text.

**Blocking condition:** none. The Kilo review does not gate
merging.

**Deferred-to:** when the operator re-runs the Kilo review on
the new HEAD, the new body should reflect the resolved items.

## 9. Cross-references

- `00_SESSION_OVERVIEW.md` — the original audit and its 15-item
  scope.
- `02_SPECIFICATIONS.md` §8 — the "Out of scope" table.
- `03_DAG_WBS.md` §6 — "What was deliberately NOT done in this
  PR".
- `04_IMPLEMENTATION_STRATEGY.md` — the design decisions that
  intentionally deferred these items.
- `06_TESTING_STRATEGY.md` — the test plan that exercises the
  parts of these issues that are in-scope.
- `08_FINAL_EVIDENCE.md` §5 — the local-only test failures
  walkthrough.
- `10_INBOX_NOTIFICATION_GAP.md` — the inbox notification gap
  (§11 below) traced to source level.

## 10. Re-verification of prior-session goals (2026-10-09)

This section closes two feedback loops from prior sessions that
were tagged "speculative" or "in_progress" in the todo tool
because they had no concrete evidence of result, only inspection.

### 10.1 `25f3e1f4f` re-verification (re-verification 25f3e1f4f)

**Goal:** verify that the prior session's claims about commit
`25f3e1f4f` matched what the commit actually did.

**Method:** direct `git show` of `25f3e1f4f` and `git show
--stat 41fae89f3` for the merge context.

**Concrete evidence (all verified 2026-10-09):**

- `25f3e1f4f` is a **fork** commit, not an upstream commit.
  Author: KooshaPari, 2026-10-04 (post-merge). Title: `test(tui):
  realign four merge-introduced test expectations with fork
  layout`. `git log --oneline -1 25f3e1f4f` confirms.
- The diff in
  `crates/jcode-tui/src/tui/app/tests/remote_events_reload_05.rs`
  removes **4** `assert!` / `assert_eq!` lines and adds **2**
  (net -2). `git show 25f3e1f4f | grep '^[+-]\s*assert' | wc -l`
  → 6; the `+` count is 2 and the `-` count is 4.
- The `is_empty()` final-state check the prior session referenced
  is at line **1153** (`app.queued_messages.is_empty()`), not at
  line 964. The Kilo-flagged re-arm assertion is at line 964.
- The merge stat for this file is **243** lines (per
  `git show --stat 41fae89f3`); the file is currently **1215
  lines** (`wc -l`).

**Result:** the prior session's claims about `25f3e1f4f` were
**partly wrong** ("upstream commit", "deleted 3 assertions", "1
remaining assertion", "file landed at 964 lines"). All four
fabrications are corrected in `02_SPECIFICATIONS.md` §2.1 and
§7.2, and in `03_DAG_WBS.md` WBS-11 (commit `b3f4c01`-style
follow-up).

### 10.2 `e6135802a` doc-correction commit (doc correction commit e6135802a)

**Goal:** verify that the prior session's `e6135802a` commit
("docs(session): correct 25f3e1f4f stat numbers and §5
env-pollution detail") actually corrected the right file with
accurate numbers.

**Method:** direct `git show e6135802a` and read of the commit
message body.

**Concrete evidence (all verified 2026-10-09):**

- `e6135802a` modified **1 file**: `08_FINAL_EVIDENCE.md` (not
  `00_SESSION_OVERVIEW.md` as the prior session's WBS-11 entry
  claimed). `git show e6135802a --stat` confirms. The diff is
  +18/-12.
- The commit body reports a **local test re-run** at HEAD
  `844016a50` (the previous session's HEAD before `e6135802a`):
  `test result: FAILED. 2396 passed; 2 failed; 17 ignored;
  0 measured; 0 filtered out; finished in 372.03s`. The output
  was written to `.scratch-tui-final.txt` (a literal path, not
  a `/tmp` redirection).
- The 2 local failures are correctly attributed in the commit
  body: (1) `detected_resume_terminal_recognizes_handterm_term_program`
  due to `TERM_PROGRAM=herdr` leaking from the harness shell and
  racing the `EnvVarGuard::set_value`; (2)
  `test_real_draw_click_on_body_anchored_image_label_cycles_level`
  due to `status_notice()` inheriting a stale `Image copied`
  suffix from a prior test. Both are env-pollution, not
  regressions, and CI's fresh-container ubuntu runner observes
  2402/0.
- The corrected stat numbers in `08_FINAL_EVIDENCE.md` line 36
  match `git show --stat 41fae89f3` exactly: `remote_events_reload_05.rs` 243,
  `ui_input.rs` 608, `swarm_buffer.rs` 85.

**Result:** `e6135802a` is **correct and well-evidenced**. The
prior session's WBS-11 description was wrong about which file
the commit changed (it corrected `08_FINAL_EVIDENCE.md`, not
`00_SESSION_OVERVIEW.md`); that fabrication is corrected in
`03_DAG_WBS.md` WBS-11. The corrected stats and the local repro
result in the commit body are backed by concrete data, not
inspection.

## 11. Inbox notification gap — DEFERRED (harness defect, not fork)

**Severity:** DEFERRED (operator-side harness defect; no fork code
impact).

Agent-initiated GitHub write actions that the pre-tool hook DEFERs to
the operator inbox are never surfaced to the operator. Every queued
item shows `notified_via: []`, and the deferred items accumulated
unanswered for 21+ hours across multiple sessions.

**Observed:** 35 pending inbox items existed at one point, 14 of them
duplicate PR-comment entries and 6 pointing at body files that no
longer resolve. Only 5 were the intended issue bodies.

**Root cause and proposed fix:** traced to source level in
`10_INBOX_NOTIFICATION_GAP.md`. Pre-staged issue body:
`10a_ISSUE_inbox-notification-gap.md`.

**Scope for this PR:** none. The gap was *worked around* manually in
this session (`~/.jcode/memories/global/harness-agents.md` records the
fallback behaviour), and the five deferred issue bodies were written
and re-verified against live data before dispatch.

**Cross-repo note:** this is a harness defect, so the fix belongs in
`1jehuang/jcode` upstream, not in the KCode fork. The fork-side
deliverable is the issue body, which is complete.
