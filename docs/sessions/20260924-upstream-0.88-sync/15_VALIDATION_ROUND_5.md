# Validation Round 5 — `gh pr merge` Inbox Items: Stale-vs-Actionable (2026-10-11 01:12 PDT)

## Scope

Auto-prompted "Continue working, or update the todo tool" after the
round 4 corrections (`f14c38790d`). Round 5 specifically re-verifies
the 4 `gh pr merge --squash` items found in round 4, since they have
no `--repo` flag and might be stale-queued from completed work in
other repos.

## Method

Cross-reference each inbox `gh pr merge N` against the operator's
likely repos: `KooshaPari/KCode`, `KooshaPari/ShareCLI`,
`KooshaPari/herdr`. Use `gh pr view N --repo X --json` (read-only,
no side effects) to find where each PR lives and its current state.

## Findings

### Inbox items 1-4: where each PR lives

| Inbox hook ID | cmd | KCode | ShareCLI | herdr | Verdict |
|---|---|---|---|---|---|
| `hook-6f57ffa991d15d3578fb7d239c74894c` | `gh pr merge 430 --squash` | n/a (404) | **MERGED** | n/a | **STALE** |
| `hook-7ed9ba33408490364a7d0ffa9e599576` | `gh pr merge 20 --squash` | **OPEN** | CLOSED | n/a | **ACTIONABLE** |
| `hook-8166431a44a0a3cc4104e93b6daac3de` | `gh pr merge 429 --squash` | n/a (404) | **MERGED** | n/a | **STALE** |
| `hook-b8703857fc2040a28756e1c12a773b8d` | `gh pr merge 433 --squash` | n/a (404) | **MERGED** | n/a | **STALE** |

### The 1 actionable item: KCode#20

```
PR:    https://github.com/KooshaPari/KCode/pull/20
title: feat(tool): add optional durable effect recovery hook
author: KooshaPari (operator)
state:  OPEN
base:   master
head:   impl/effect-recovery-hook-20260930
diff:   +518 / -23
```

This PR is in the KCode repo. If the operator approves
`hook-7ed9ba33...`, the resulting `gh pr merge 20 --squash` would
target KCode#20 (the default `gh` repo for the current cwd is
`KooshaPari/KCode`).

**However:** KCode#20 was filed against `master`, but master has
since moved to `901b3310` (PR #33 squash-merged). KCode#20 is
therefore likely behind master and may or may not be mergeable
without a rebase. The 518 additions are substantial enough to
warrant a CI run before merge.

### The 3 stale items: ShareCLI PRs already merged

| Hook ID | ShareCLI PR | Status | Title |
|---|---|---|---|
| `hook-6f57ffa9...` | #430 | MERGED | feat(fuse): provenance inspect CLI (AC-009.11) |
| `hook-8166431a...` | #429 | MERGED | feat(mesh): surface Maildir queue depth in status and thermal TUI (AC-010.11) |
| `hook-b8703857...` | #433 | MERGED | feat(fuse): FUSE mount/backing path remap + spawn lifecycle (AC-009.14) |

**These are work artifacts from a prior agent session.** The agent
queued inbox approvals to merge these PRs, but the operator
presumably approved the work via a different mechanism (direct
`gh pr merge --repo KooshaPari/ShareCLI`) without going through
the inbox queue. The inbox items are now orphaned.

**The inbox commands have no `--repo` flag**, so if approved as-is:
- From a KCode cwd: `gh pr merge 429 --squash` → GraphQL "no pull
  request found" (KCode#429 doesn't exist).
- From a ShareCLI cwd: `gh pr merge 429 --squash` → "Pull request
  #429 is already merged" (fails cleanly).

**Net effect:** approval of any of these 3 would result in a
**no-op error**, not a destructive action. The pre-tool hook's
inbox queue was the safety net that caught the staleness.

## The repo-context ambiguity risk

None of the 4 `gh pr merge` items specify `--repo`. This is a
**general anti-pattern** in inbox items. The lessons 19/20 in
memory describe how to classify inbox items by `Command:` field;
this round 5 reveals an additional classification axis: **inbox
items lacking `--repo` (or other context flags) are intrinsically
ambiguous and should be denied by default.**

The `gh` command's repo resolution order is:
1. `--repo` flag (none here)
2. `GH_REPO` env var (unset)
3. CWD's git remote
4. The gh config default

If approved, the same inbox command can have very different
behavior depending on cwd at the time of execution. This is a
**non-deterministic approval**: approving the same item at
different times could affect different repos.

## Recommendation

**For the operator:**

1. **Approve `hook-7ed9ba33...` only if** you want to merge
   `KooshaPari/KCode#20` (the durable effect recovery hook).
   Verify the PR is still mergeable against current master first.
2. **Deny `hook-6f57ffa9...`, `hook-8166431a...`, `hook-b8703857...`**
   (all 3 are stale ShareCLI merges that would no-op).
3. **Update memory** to capture this round 5 finding as lesson 22:
   inbox items lacking `--repo` (or equivalent context flags) are
   non-deterministic and should default to deny.

## New lesson captured (round 5)

**22. Inbox items lacking context flags (`--repo`, `--branch`,
`--ref`) are non-deterministic and should default to deny.** A
`gh pr merge N` command with no `--repo` resolves the target repo
from cwd, env var, or `gh` config — and approving the same item
at different times can target different repos. Lesson: when
classifying inbox items, count the items missing required context
flags as a separate hazard class (medium: ambiguous rather than
low: explicit). Discovered 2026-10-11 in round 5: 3 of 4
`gh pr merge` items had no `--repo` flag, and 3 of those 4 PRs
were already MERGED in a different repo (ShareCLI). Approval
would have failed cleanly in this case, but the broader pattern
is unsafe.

## Net corrections through 5 rounds

| # | Claim in earlier rounds | Round 5 actual | Status |
|---|---|---|---|
| 1 | "4 'gh pr merge' medium-priority items" | 1 actionable (KCode#20) + 3 stale (ShareCLI#429/430/433 already merged) | ❌ miscounted actionable subset |
| 2 | "Approval would auto-merge the listed PR" | 3 of 4 would fail with 'already merged' or 'not found' | ❌ approval is no-op, not destructive |
| 3 | (no prior claim) | 4 of 4 lack --repo flag (lesson 22) | ✓ new finding |
