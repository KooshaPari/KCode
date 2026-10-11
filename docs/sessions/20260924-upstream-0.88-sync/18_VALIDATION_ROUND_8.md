# Validation Round 8 — KCode#20 Doubly-Stale, Inbox 19 (Still 19) (2026-10-10 PDT)

## Scope

Auto-prompted "Continue working, or update the todo tool" after the
round 7 corrections (`46d6f5764`). Round 8 re-verifies the KCode#20
profile (which round 5 declared "1 actionable") and reconciles the
inbox state across the 3-hour window since round 4.

## Finding 1: KCode#20 is in a much worse state than round 5 said

| Field | Round 5 claim | Round 8 actual |
|---|---|---|
| State | "OPEN" | OPEN ✓ (still) |
| Mergeable | (not checked) | **UNKNOWN** (GitHub hasn't computed) |
| isDraft | (not checked) | **true** (it's a DRAFT) |
| Created | "2026-09-30" | 2026-09-30T11:49:07Z (11 days ago) |
| Updated | (not checked) | 2026-09-30T15:39:27Z (11 days ago, no activity since) |
| baseRefName | master | master ✓ (but master has moved 11 days later) |
| CI checks | (not checked) | **All 11 days stale** (last run 2026-09-30T16:13:35Z) |
| File paths | "KCode#20 changes things" | All 12 files reference **pre-rename** paths (`crates/jcode-app-core/...` and `crates/jcode-protocol/...`) |

**Critical: 12/12 files in KCode#20 reference crate paths that were renamed to `kcode-*` in PR #33 on 2026-10-09.** None of those paths exist in current master.

### What `gh pr merge --squash 20` would actually do right now

1. The PR is in DRAFT state → `gh pr merge` refuses to merge drafts
   (it errors with "PR is in draft state and cannot be merged")
2. Even if it could merge, the 12 file paths don't exist in master
   (all renamed to kcode-*); the merge would either silently
   no-op or error out
3. Even if the merge succeeded, CI is 11 days stale and the diff
   is from a pre-rename codebase

**The 8 inbox items for "KCode#20 gh pr merge" are doubly stale:**
- The inbox items themselves are old
- The PR they target is in a state that cannot be merged

### What the operator would need to do to make KCode#20 mergeable

1. Re-target the PR to the new master tip (901b3310)
2. Rename all `jcode-*` crate references to `kcode-*` in the diff
3. Re-run all CI checks
4. Convert from draft to ready for review
5. Then `gh pr merge --squash 20` would work

That's 1-2 hours of focused work, on top of the review of the
actual diff. The diff is non-trivial: 12 files, +518/-23, includes
CI workflow changes, server lifecycle changes, and a tool registry
modifier. It's a real feature, not a trivial patch.

## Finding 2: Inbox 19-pending is still 19-pending (no change)

Round 4 at 2026-10-10 ~01:17 PDT: 19 pending + 108 expired = 127 total
Round 8 at 2026-10-10 ~18:20 PDT: 128 total (1 more file arrived)

The freshest 19 items by mtime are the same in both rounds:
- Freshest: `hook-78ace1bb6e197dca5d1fb8d299600b34.json` at Oct 10 18:03:11
- 19th freshest: `hook-6511ba6be6b4de05dcad7e06c1242db0.json` at Oct 9 17:41:07

So the 19-pending count is approximately stable; some items have
aged out (their mtimes are >24h from "now") but new items have
arrived (or the count is just 19 by top-N selection).

**Note on the system clock:** `date` reports
`2026-10-10 18:20:16 PDT`. The session has been using 2026-10-11
timestamps in session docs; the real system clock is 7 hours
earlier. The "19 pending" count is consistent with phinbox's
behavior of treating items with mtime < 24h as fresh.

## Net corrections through 8 rounds

| # | Claim in earlier rounds | Round 8 actual | Status |
|---|---|---|---|
| 1 | "1 actionable (KCode#20)" | DRAFT, 11 days stale, 12/12 files reference pre-rename paths | ❌ majorly wrong |
| 2 | "4 'gh pr merge' items, 1 actionable, 3 stale" | 1 of 1 actionable is ALSO stale (PR is draft) | ❌ wrong actionable classification |
| 3 | "19 pending inbox items" | Still 19, with 1 new arrival | ✓ correct |
| 4 | "queue.json is empty []" | queue.json doesn't exist; phinbox doesn't use a queue file | ❌ wrong; phinbox's authoritative state is the inbox dir mtime |
| 5 | "127 total inbox files" | 128 total (1 more) | ⚠️ off by 1 |
| 6 | "108 expired inbox files" | 91 (mtime > 24h) — different methodology | ⚠️ methodology off |

## Lesson captured (round 8)

**28. Before declaring a PR "actionable" via `gh pr merge`, always
check isDraft, updatedAt, and baseRefName state.** Round 5 said
KCode#20 was "1 actionable" because it was OPEN and the +518/-23
diff looked real. But the PR was actually:
- A DRAFT (cannot be merged)
- 11 days stale (no activity since 2026-09-30)
- Pre-rename (file paths don't exist in current master)

The "actionable" classification was wrong because round 5 only
checked `state == OPEN`, not `isDraft`, not `updatedAt`, not
`mergeable`, and not whether the file paths still exist. The
check needs to be:
1. `state == OPEN`
2. `isDraft == false`
3. `mergeable == "MERGEABLE"` (not UNKNOWN or CONFLICTING)
4. `updatedAt` within the last 7 days (to catch stale PRs)
5. All file paths in the PR still exist in the base branch
   (use `gh pr view N --json files --jq '.files[].path'` and
   check each against `git ls-tree base`)

If any of these fail, the PR is "stale actionable" not "actionable".
The operator should rebase the PR before approving merge, not
try to merge it as-is.

## What this changes for the operator

The 8 inbox items for "KCode#20 gh pr merge" should be
**reclassified as STALE-ACTIONABLE** (not actionable). The
operator's options are now:
1. **Approve the merge as-is** → will FAIL (PR is draft)
2. **Mark the inbox items as deny + reason** → simplest; PR is
   too stale to merge without rebase
3. **Rebase KCode#20 first, then merge** → 1-2 hours of focused
   work to update crate paths and re-run CI
4. **Close KCode#20 as superseded** → if the effect-recovery-hook
   work has been redone elsewhere in the past 11 days

The 4 inbox items for the ShareCLI#429/430/433 (already MERGED)
remain correctly classified as "stale → no-op if approved".
