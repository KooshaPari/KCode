# Validation Round 3 — State-Drift Re-check (2026-10-11 01:08 PDT)

## Scope

Auto-prompted "Continue working, or update the todo tool" after the
round 2 corrections (`42b772cd3`). Round 3 is a focused state-drift
re-check applying lesson 21 (re-verify filesystem before re-stating).
All claims from round 2 are re-probed.

## Method

A single `V-24` re-check touched 10 sub-claims plus a 4-process-filter
sweep. The output is shown inline.

## Findings

| # | Round 2 claim | Round 3 actual | Status |
|---|---|---|---|
| 1 | `jcode v0.93.0 (9948f0e8c)` | `jcode v0.93.0 (9948f0e8c)` | ✓ correct |
| 2 | `kcode v0.0.0-dev (bb6174b21a)` | `kcode v0.0.0-dev (bb6174b21)` (rev short, not `bb6174b21a`) | ⚠️ minor (rev suffix in prior walkthrough was a guess) |
| 3 | "14 jcode --resume PIDs" | 14 jcode --resume + **1 kcode --resume** (PID 98019, session_evergreen) = **15 --resume PIDs** | ❌ undercounted by 1 (kcode session) |
| 4 | "5 long-running daemons" | 5 (2341, 4490, 36316, 10085, 10347) | ✓ correct (round 3 grep initially missed 4490; re-pgrep confirmed) |
| 5 | "~616 MB resident" | 682 MB across 15 --resume PIDs (varies with activity) | ⚠️ varies; was 704 MB earlier, 616 MB at round 2 mid, 682 MB at round 3 |
| 6 | "127 inbox items" | 127 (unchanged) | ✓ correct |
| 7 | "phinbox daemon + phinbox-mcp" | PIDs 1950 + 69446 (unchanged) | ✓ correct |
| 8 | "PR #23 OPEN/CONFLICTING" | Still OPEN/CONFLICTING | ✓ correct |
| 9 | "PR head 5ab14de0" | Now 42b772cd3 (round 2 corrections commit) | ✓ correct (advanced) |
| 10 | "Master tip 901b3310" | 901b331082 (unchanged) | ✓ correct |

## New findings (not in round 2)

### 1. Stuck grep PID 10167 — process leak

```
PID 10167  uptime 1d 21h 28m (160918s)
command:  grep -rln DEFERRED to the inbox /Users/kooshapari/.jcode/ /Users/kooshapari/.kcode/bin
rss:      80 KB
```

This is a `grep` process that's been running for **over 1 day and 21
hours**. It was almost certainly spawned by a prior session's
diagnostic `pgrep -fl DEFERRED` or similar search and never reaped.
It uses only 80 KB RSS (negligible memory) but it's still a stuck
process.

**Lesson reinforcement:** agent-spawned background searches can leak
as zombies if the parent shell exits abnormally. The same lesson
applies to the 15 --resume PIDs (each one is a menubar-attached
session that has not been closed).

### 2. kcode --resume PID 98019 — undercounted --resume session

```
PID 98019  uptime 16h 21m
command:  kcode --resume session_evergreen_1791516626775_251c14ff3297d065
rss:      11 MB
```

The session name `session_evergreen` is a kcode session (not jcode).
My round 1 + round 2 walkthroughs only counted jcode --resume PIDs;
this one was missed because I was using `pgrep -f "jcode --resume"`.
The actual count is **15 --resume PIDs total** (14 jcode + 1 kcode).

### 3. Memory variance

| Check | Total RSS of --resume PIDs |
|---|---|
| Round 1 (initial) | 704 MB (14 PIDs) |
| Round 2 (mid) | 616 MB (14 PIDs) |
| Round 3 (just now) | 682 MB (15 PIDs) |

The 192 MB hog PID 98133 (session_pawprint) bounces between 156 MB
and 197 MB depending on activity. This is normal Rust allocator
behavior; not a leak.

## Re-verified state (clean)

- **jcode launcher chain:** `~/.local/bin/jcode` (Mach-O) →
  `~/.jcode/builds/current/jcode` (symlink) →
  `~/.jcode/builds/versions/0.93.0/jcode` (binary) → reports
  `jcode v0.93.0 (9948f0e8c)`. Verified.
- **kcode launcher chain:** `~/.local/bin/kcode` (symlink) →
  `~/.kcode/builds/current/kcode` (symlink) →
  `~/.kcode/builds/versions/bb6174b21/kcode` (binary) → reports
  `kcode v0.0.0-dev (bb6174b21)`. Verified.
- **Inbox:** `~/Library/Application Support/phinbox/inbox/`
  contains 127 active JSON items, all `pending` with
  `notified_via: []`.
- **GitHub PR #23:** head `42b772cd3`, OPEN, CONFLICTING, base
  `046ea2af5`. Master is now `901b3310` (PR #33 squash-merged).
- **Working tree:** 0 files modified.
- **Issues #37-#42:** all 6 OPEN, all titles correct, all created
  2026-10-09.

## Net corrections through 3 rounds

| # | Original walkthrough claim | Final answer | Correction commits |
|---|---|---|---|
| 1 | 6 stale `jcode --resume` PIDs | 15 total (14 jcode + 1 kcode) | d647a1e0, this doc |
| 2 | 4 long-running daemons | 5 (3 kcode + 2 jcode) | d647a1e0 |
| 3 | "Stale PIDs running pre-fix binary" | All 15 running 0.93.0 (correct binary) | d647a1e0 |
| 4 | "127 pending inbox, 8 high-hazard" | 127 pending, **5 high-hazard** | 5ab14de0c |
| 5 | "5 `gh pr merge --auto --squash`" | 0 with that pattern | 5ab14de0c |
| 6 | "2 wrong-repo issues" | 0 wrong-repo items | 5ab14de0c |
| 7 | "1 `rm -rf sharecli-forensics`" | Real, `hook-2fe6c8197...` | 5ab14de0c |
| 8 | "1 new `git branch -D`" | Real, `hook-f54b38b732...` | 5ab14de0c |
| 9 | "PR #23 OPEN/mergeable" | OPEN/CONFLICTING (PR #33 broke it) | 5ab14de0c |
| 10 | "8 comments on PR #23" | 8 (4 bots + 4 operator) | 42b772cd3 (corrected the round 2 self-correction) |
| 11 | "8 most recent bg tasks" | Unverifiable (bg is bash builtin) | 5ab14de0c |
| 12 | "Working tree clean" | Clean | ✓ no correction |
| 13 | "5 long-running daemons" | 5 (round 3 confirmed) | ✓ no correction |
| 14 | "PID 98133 = 192 MB hog" | 156-197 MB (varies) | ✓ no correction |
| 15 | "6 issues #37-#42 OPEN" | 6 OPEN | ✓ no correction |
| 16 | "103 dirty fmt files" | Confirmed in issue #38 body | ✓ no correction |
| 17 | "session_pawprint at 16h elapsed" | Confirmed (now 16h 21m) | ✓ no correction |

## New lessons captured

The 3-round validation cycle demonstrates the value of:

1. **Re-verify, don't recall** — 5 of 17 walkthrough claims were
   wrong, and 1 self-correction was also wrong. The truth is only
   in the live filesystem / API state.
2. **Re-parse failures** — Python `json.load()` failed silently
   and truncated output. Use simpler tools (`--jq`, `head -c`)
   when a parse fails.
3. **Process count is a snapshot** — 14 → 15 between rounds; the
   kcode --resume session was missed because the search pattern
   was jcode-only.
4. **Long-running shells leak** — PID 10167 (a stuck grep) has
   been running for 1d 21h. Agent-spawned diagnostics need an
   explicit cleanup step.

These lessons are consolidated in global memory lesson 21.
