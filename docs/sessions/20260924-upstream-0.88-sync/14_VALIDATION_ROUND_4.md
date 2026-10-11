# Validation Round 4 — Inbox Hazard Re-classification (2026-10-11 01:10 PDT)

## Scope

Auto-prompted "Continue working, or update the todo tool" after the
round 3 corrections (`8f4ebae95`). Round 4 specifically re-verifies
the **inbox state** and **high-hazard count** that were claimed in
rounds 1+2. The lesson 21 application is direct: a count that is
more than a few sentences old must be re-checked from the
filesystem before being re-stated.

## Method

A `V-25` Python-driven sweep that re-counts inbox items by `state`
(pending/approved/denied/expired), then hazard-classifies the
pending set by command shape (`rm -rf`, `git branch -D`, `gh repo
fork`, etc.).

## Critical finding: prior inbox claims were wrong

| Claim in rounds 1+2 | Round 4 actual | Status |
|---|---|---|
| "127 pending inbox items" | **19 pending** (108 already expired) | ❌ under-reported expiration; the "127 pending" was a total-file-count, not a pending count |
| "17 expired" | **108 expired** | ❌ undercount by 6× |
| "144 total files" | **127 total files** | ❌ inflated by 17 |
| "5 high-hazard pending items" | **3 high-hazard pending** (2 of the original 5 have expired) | ❌ 2 of the 5 are no longer actionable |
| "5 high-hazard types: 3 rm -rf + 1 git branch -D + 1 gh repo fork" | **3 high-hazard: 1 gh repo fork + 1 multi-command template (rm -rf inside) + 1 git branch -D** | ❌ miscounted rm -rf items |

## Real inbox state (verified 2026-10-11 01:10 PDT)

```
Total inbox files: 127
  pending:  19
  approved: 0
  denied:   0
  expired:  108
```

## Real high-hazard items (3, not 5)

### HH-1: `hook-62d32ea8dfa51ffa87ad83c31162f51b` — gh repo fork

```
title:   Approve: gh repo fork
state:   pending
queued:  2026-10-09 06:11:46 UTC (1d 19h ago)
command: gh repo fork herdrdev/herdr --clone=false
reason:  Forking creates a new repository under the account and may clone large history.
```

**Why high-hazard:** creates a new public repository under the
GitHub account. Permanent, public-visible side effect.

### HH-2: `hook-ed5ea169006aeed0a3768dbbb0953296` — 4-command template

```
title:   Approve: gh state-changing command
state:   pending
queued:  2026-10-11 00:59:13 UTC (11m ago)
command: gh pr merge
         gh issue create --repo X
         rm -rf X
         gh pr comment --repo X
reason:  This changes repository or account state on GitHub.
```

**Why high-hazard:** this is a multi-command template that
explicitly includes `rm -rf X` in its approved-command set. If
approved, the agent could choose to run any of the 4 listed
commands, including the `rm -rf X` variant. The placeholder `X` is
not filled in, so the actual target is variable.

**Note:** my round 1+2 analysis described this as a "generic
`rm -rf` prompt" — that was an oversimplification. It's a
4-command template; `rm -rf` is one of 4 options.

### HH-3: `hook-f54b38b732a31f5b9ddaa162dc59d9ca` — git branch -D + push --delete

```
title:   Approve: git branch -D
state:   pending
queued:  2026-10-09 05:54:15 UTC (1d 19h ago)
command: git branch -D chore/sha-pin-infra-actions
         git push origin --delete fix/perf-bench-advisory-20261009-shas
         git branch -D fix/perf-bench-advisory-20261009-shas
reason:  Unmerged commits on the branch may be lost.
```

**Why high-hazard:** force-deletes two local branches and pushes a
delete to the remote origin. The pre-tool hook explicitly warns
"unmerged commits may be lost." Both branches are local to the
inbox/operator flow (chore/, fix/ prefixes) — the operator
should confirm the branches have been merged or are no longer
needed.

## Items that have EXPIRED (no longer need triage)

These were claimed as "high-hazard" in rounds 1+2 but have since
expired (the pre-tool hook will not execute them now even if
approved):

| Hook ID | Command | When queued | Expired at |
|---|---|---|---|
| `hook-2fe6c81976d5d288811bd2d5f8feb8dc` | `rm -rf sharecli-forensics && gh repo clone KooshaPari/ShareCLI` | 2026-10-09 05:54:54 UTC | 2026-10-10 05:54:54 UTC |
| `hook-e88f8f24a5e6dbeef520ae8b0a5c6d62` | `rm -rf phenodesign-fix && gh repo clone KooshaPari/PhenoDesign` | 2026-10-09 06:33:30 UTC | 2026-10-10 06:33:30 UTC |

The 24h expiry is built into phinbox; the operator did not
explicitly approve/deny them.

## Real LOW-hazard items (16, all are queued gh subcommands)

| Hook ID | Command (abbrev) | What it would do |
|---|---|---|
| `hook-09bc49d5...` | `gh workflow run audit.yml` + `gh workflow run security-scan.yml` | Trigger 2 CI workflows on `main` (idempotent) |
| `hook-1effaf68...` | `gh pr edit $pr --body-file /tmp/pr-body.md` | Update a PR body |
| `hook-287b24e5...` | `gh issue create --repo herdrdev/herdr` | File an upstream issue on herdr |
| `hook-2bb3c44e...` | `gh issue create --title ... --body-file /tmp/issue-chronic-cis.md` | File an issue in default repo |
| `hook-4ad95ec8...` | `gh repo clone KooshaPari/herdr` | Clone herdr locally (50 commits) |
| `hook-53740233...` | `gh api graphql -f QUOTED_DATA` | GraphQL API write (the spec body was QUOTED_DATA, not parsed) |
| `hook-6f57ffa9...` | `gh pr merge 430 --squash` | Squash-merge PR #430 if approved |
| `hook-78ace1bb6...` | `gh api graphql -f QUOTED_DATA` | Another GraphQL write |
| `hook-7ed9ba33...` | `gh pr merge 20 --squash` | Squash-merge PR #20 (likely herdr or upstream) |
| `hook-8166431a...` | `gh pr merge 429 --squash` | Squash-merge PR #429 |
| `hook-95ee937f...` | `gh run rerun 37893476353 --repo KooshaPari/ShareCLI --failed` + `gh api ...check-ru` | Re-run failed jobs in CI |
| `hook-a278210f...` | `gh workflow run cargo-deny.yml` | Trigger cargo-deny CI |
| `hook-a8088a79...` | `gh issue create --repo KooshaPari/KCode` | File issue on KCode |
| `hook-b8703857...` | `gh pr merge 433 --squash` | Squash-merge PR #433 |
| `hook-e4141499...` | `gh pr edit 876 --repo KooshaPari/ShareCLI` | Edit PR body on ShareCLI |
| `hook-fc896a2f...` | `gh issue create --repo herdrdev/herdr` | Another herdr issue |

**4 of these are `gh pr merge --squash`** (PRs 20, 429, 430, 433).
These are the most likely candidates for the operator to want
approved, since they are concrete merge actions.

## What's now actionable

The operator has 19 pending approvals. Of those:

1. **3 high-hazard** that need explicit decision (HH-1, HH-2, HH-3 above).
2. **4 medium-priority** (the `gh pr merge --squash` items — concrete
   PRs that would merge if approved).
3. **12 low-priority** (workflow runs, issue creates, pr edits —
   could be batch-approved or batch-denied).

None of these will execute without operator approval. The
`appr` prefix (per memory) approves the previously-identified N
items. Round 4 has now correctly identified the N=19 set and the
high-hazard subset of 3.

## Lesson reinforcement

The round 1+2 inbox analysis was the SINGLE LARGEST SOURCE of
fabricated claims in this session. The 144 total, 127 pending, and
5 high-hazard numbers were all wrong by significant margins
(127 total, 19 pending, 3 high-hazard). The lesson 21 framework
(re-verify from filesystem) caught all three of these in round 4.

The reason this slipped through rounds 1-3 is that I was re-checking
*what I had just said* in the prior round, not the underlying
filesystem. Round 4 went back to the source directory and read the
JSON files directly, which revealed the real numbers.

**Process note for future rounds:** whenever the user prompt is
"validate further," the validation must include re-probing the
*source* of every claim, not just verifying the *most recent
restatement* of the claim. Two layers of caching (memory of
prior-turn + memory of prior-session) compound the error.
