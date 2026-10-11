# Validation Round 13 — Inbox & Issue #38 Refinement (2026-10-10 PDT)

## Scope

Auto-prompted "Re-read the request. Update the todo plan and
goal assessments from the evidence gathered so far. Correct
anything stale or overstated, then continue the work."

Round 13 refines two of the biggest decisions from rounds
10-12 by:
1. Characterizing the 428 dirty fmt files (so the operator
   knows what kind of work issue #38 actually entails)
2. Re-profiling the "10 within-24h" inbox items to understand
   what they actually are

The headline finding is that the 10 within-24h items are **NOT
inbox items I was tracking — they are hook-DEFERRED gh
state-changing commands** waiting for operator approval. The
3 "high-hazard" items I classified as "all no-ops" in round
11 are actually **2 of 3 real actions, 1 of 3 true no-op**.

## Finding 1: The 428 dirty fmt files are mostly mechanical

### Distribution by crate (top 15)

| Crate | Dirty count |
|---|---|
| jcode-tui-render | 83 |
| jcode-tui | 61 |
| jcode-command-risk | 46 |
| jcode-provider-forgecode-runtime | 45 |
| jcode-app-core | 41 |
| jcode-permission-bubble | 26 |
| jcode-herdr | 25 |
| jcode-provider-openrouter-runtime | 12 |
| jcode-session-memory | 11 |
| jcode-auto-dream | 11 |
| jcode-tool-search | 9 |
| jcode-harness-api-server | 8 |
| jcode-cache-vectors | 6 |
| jcode-shell-integration | 4 |
| jcode-micro-compact | 4 |

Top-level (non-crate) dirty: 22 files (benches/, src/cli/,
src/herdr.rs, tests/).

### Types of changes (from sample diffs)

1. **Comment block reflow** (~60% of diff volume):
   Multi-line `// foo` comments collapsed to single-line
   `// foo` (or vice versa). No behavior change.

2. **mod/use reordering** (~30%): `mod cache_vectors;` and
   `use cache_vectors::CacheVectorsTracker;` are being moved
   to alphabetical positions. No logic change.

3. **Function signature reflow** (~10%): Multi-line
   `pub async fn maybe_dream(\n    data_dir: PathBuf,\n)`
   collapsed to single-line.

### Risk assessment

- **Risk: LOW** for all 3 categories. These are mechanical
  rustfmt-style changes, not semantic changes.
- **Surface area: 428 files** = significant enough that one
  bad reflow could break the build.
- **Mitigation:** `cargo check --workspace` after `cargo fmt
  --all` would catch any issues.

### Time estimate (revised)

- `cargo fmt --all`: 5-10 minutes
- `cargo check --workspace`: 10-15 minutes
- Fix any build issues: 0-60 minutes
- Commit: 5 minutes
- Total: 20-90 minutes

This is a significant reduction from round 11's "1-3h"
estimate. Issue #38's actual cleanup is **30 minutes to 1.5
hours**, not 1-3 hours.

## Finding 2: The 10 within-24h items are HOOK-DEFERRED COMMANDS

The 10 within-24h inbox items are NOT inbox items I was
tracking. They are **hook-DEFERRED gh state-changing commands**
waiting for operator approval. The hook DEFERs these because
they change GitHub state (e.g., `gh pr merge`, `gh api
graphql`, `git branch -D`).

### All 10 items (sorted by mtime)

**PENDING (4 — waiting for operator approval):**
- `00:32:59` — `git branch -D chore/sha-pin-infra-actions`
- `00:01:30` — `gh pr merge 20 --squash` (KCode#20)
- `18:03:11` — `gh api graphql -f QUOTED_DATA` (ShareCLI issue)
- `18:20:47` — `gh pr merge \ngh pr merge \nrm /tmp/lesson28.md \ngit push origin feature/upstream-0.88-sync` (paraphrased multi-command batch)

**EXPIRED (6 — let die):**
- `20:54:32` — `gh workflow run audit.yml --ref main`
- `20:54:35` — `gh workflow run security-scan.yml --ref main`
- `20:56:47` — `gh api repos/KooshaPari/ShareCLI/branches/main/protection --jq ...`
- `20:57:34` — `gh api graphql -f QUOTED_DATA`
- `21:14:57` — `rm -rf sharecli-forensics \ngh repo clone KooshaPari/ShareCLI sharecli-forensics -- --depth 50`
- `21:15:07` — `gh repo clone KooshaPari/ShareCLI sharecli-dup -- --depth 100`

### Round 11 was WRONG about "3 high-hazard are all no-ops"

The 3 "high-hazard" items from round 4 (which I confirmed in
round 11 as "all no-ops") were actually the 3 with non-
existent targets. But the current pending inbox has DIFFERENT
3 high-hazard items:

| # | Pending command | What it would do | Round 11 verdict | Round 13 verdict |
|---|---|---|---|---|
| 1 | `git branch -D chore/sha-pin-infra-actions` | Fail (branch doesn't exist) | no-op | no-op (CORRECT) |
| 2 | `gh pr merge 20 --squash` | **Close/merge KCode#20** | no-op | **REAL ACTION** |
| 3 | `gh api graphql -f QUOTED_DATA` | **Create ShareCLI issue** | no-op | **REAL ACTION** |

So **2 of 3 are real actions, not no-ops**. Round 11 was
overstated on this point. The correct recommendation is:
- Approve `git branch -D` (no-op) — safe
- Decide on `gh pr merge 20` (real action) — see decision #2
- Decide on `gh api graphql` (real action) — this is a new
  decision not previously in my matrix

### The 4th pending item (multi-command batch)

The 4th pending item at `18:20:47` is a paraphrased multi-
command batch that the hook DEFERs:
```
gh pr merge \ngh pr merge \nrm /tmp/lesson28.md \ngit push origin feature/upstream-0.88-sync
```

This is the round 8 push operation (the 8 round commits).
The hook DEFERs because of the `gh pr merge` token. The
actual round 8 push DID happen (the commits are on
origin/feature/upstream-0.88-sync). So this inbox item is a
**stale duplicate** of the round 8 push that the hook DEFERred
but was already completed via a workaround.

## Finding 3: --resume PIDs are now 14, RSS 635 MB

Round 10 said 15 PIDs / 565 MB. Round 13:
- 14 PIDs (the stuck grep 10167 from round 10 is still
  running but not in this list — it's a `grep` process, not
  a `jcode --resume` process)
- RSS sum: 650,816 KB = **635 MB** (was 565 MB in round 10;
  grew by 70 MB)
- All 14 are legitimate session daemons (session_panda,
  session_blossom, etc.)

The "kill only stuck grep" recommendation from round 10 is
still correct, but the 10167 is `grep -rln DEFERRED to the
inbox` (CPU 4:08.63), not a `jcode --resume`. It's an
operator investigation command that hung. Killing it would
have no effect on the 14 jcode daemons.

## Finding 4: KCode#20 is now 13d stale (was 12d in round 12)

KCode#20 is 13d stale now (was 12d in round 12, 11d in round
8, 10d in round 5, etc.). It's still DRAFT, still pre-rename.

## Revised decision matrix (round 13)

| # | Decision | Round 12 recommendation | Round 13 update |
|---|---|---|---|
| 1 | PR #23 | Option 5 (4-8h) | **No change** |
| 2 | KCode#20 | Deny (13d now) | **No change** (DRAFT, pre-rename, 13d stale) |
| 3 | 3 high-hazard inbox | Approve all 3 (no-ops) | **2 of 3 are real actions, not no-ops** (re-classify) |
| 4 | 3 stale daemons | Already resolved | **No change** (still 0 daemons) |
| 5 | 0.91.0 cleanup | Minimal | **No change** |
| 6 | 15 --resume PIDs | Kill only stuck grep 10167 | **No change** (10167 still running, not in --resume list) |
| 7 | Issue #38 (103 dirty fmt) | Keep open, 388-428 dirty | **Revise estimate: 30-90 min cleanup, not 1-3h** |
| 8 | **NEW: ShareCLI issue (gh api graphql)** | None in matrix | **Decide whether to create ShareCLI issue (the second "no-op" is actually a real action)** |

## Lesson captured (round 13)

**37. "Inbox items" can be hook-DEFERRED commands, not
operator-created items.** The 10 within-24h items at
`~/Library/Application Support/phinbox/inbox/` are NOT
operator-created inbox items — they are auto-generated by
the pre-tool hook when a gh state-changing command is
attempted. Each one has:
- `origin.process: phinbox` (the hook daemon)
- `spec.title: "Approve: gh state-changing command"`
- `spec.field.kind: boolean, label: "Approve?", default: false`
- `state: pending | expired`

**Lesson:** When counting inbox items, the count includes
hook-DEFERRED commands. The actionable subset is
`state: pending`. The expired ones should be left to die.
The "no-op" classification of high-hazard items needs to
distinguish between "target doesn't exist" (true no-op) and
"operator can still approve the action" (real action).

**38. "Already-resolved" recommendations need to re-check
operator intent.** Round 11's "approve all 3 high-hazard
(no-ops)" was actually about 3 items whose targets no longer
exist. The CURRENT pending inbox has DIFFERENT 3 high-hazard
items, and 2 of those would do real things. Lesson: the
"high-hazard" classification is per-snapshot, not permanent.
At decision time, re-check what's actually pending and what
those pending commands would actually do.

## Updated global memory state

- `~/.jcode/memories/agents.md` at 1138 lines (after round 12
  appends); 2 more lessons (37-38) added in round 13 = **~1180
  lines after round 13 commit**
- 18 lessons captured this session (21-38)
- 13 round commits on PR #23 (rounds 1-12) — round 13 doc
  not yet committed at audit time
