# Validation Round 9 — PR #23 "Close as Superseded" Hidden Cost (2026-10-10 PDT)

## Scope

Auto-prompted "Continue working, or update the todo tool" after the
round 8 corrections (`484019848`). Round 9 profiles the 6 issues
#37-#42 and discovers the "close as superseded" option for PR #23
has a much higher hidden cost than round 7 estimated.

## Finding 1: PR #23 has 2157 commits, only 154 are docs

| Type | Count |
|---|---|
| Total commits in PR #23 (merge-base 85a6c10072..HEAD) | **2157** |
| Docs commits (97 docs + 28 docs(session) + 4 docs(sdk) + ... ) | **~154** |
| Non-docs commits (would be LOST if PR #23 is closed) | **~2003** |
| `Merge upstream v0.88.0 into feature/upstream-0.88-sync` | 1 (critical) |

The non-docs commits include:
- 163 + 131 + 16 + 14 + 13 + ... fix commits (RUSTSEC triages,
  build-meta fixes, ssh-agent skip, fmt rebase, panic budget
  ratchets, security allowlist, etc.)
- 51 chore(release) commits (v0.84.0 → v0.88.0 release prep)
- 118 desktop2 commits
- 83 test(tui) commits
- 52 tui commits
- 45 test commits
- 40 style commits
- 28 chore(ci) commits
- 16 ci commits
- 12 todo commits
- 11 fix(todo) + 8 feat(todo) commits

**The upstream v0.88.0 sync itself is in PR #23** as a merge commit
(`Merge upstream v0.88.0 into feature/upstream-0.88-sync`). PR #33
that advanced master to 901b3310 was the **decollision rename**,
not a re-sync of upstream v0.88.0. So if PR #23 is closed without
merging, the v0.88.0 sync is **completely lost**.

## Finding 2: aa955589d (herdr fix) is NOT on master

```
$ git merge-base --is-ancestor aa955589d origin/master
$ echo $?
1
```

The herdr wildcard re-export fix is on the feature branch only.
If PR #23 is closed without merging, this fix is LOST. Issue #40's
claim "already fully closed by commit aa955589d" is only true on
the feature branch.

## Finding 3: Re-evaluation of the 3 round-7 PR #23 options

Round 7 presented PR #23 as a 3-way choice with these time costs:
- Close as superseded: 5 min
- Cherry-pick 7 corrections: 30 min
- Full rebase: 14-17 hours

Round 9 re-evaluates:

**Option 1: Close as superseded** — REJECTED
- Round 7 said "5 min" cost. **Actual cost: lose 2003 non-doc
  commits including the v0.88.0 sync, RUSTSEC triages, build-meta
  fixes, ssh-agent skip, fmt rebase, panic budget ratchets, and
  ~50+ other fixes.**
- This is not a 5-minute cost; it's a "re-do the v0.88.0 sync from
  scratch" cost (probably 4-8 hours minimum).
- The operator should NOT choose this option unless the upstream
  v0.88.0 sync has been re-applied on master via a different
  branch (and a quick search shows it has not).

**Option 2: Cherry-pick 7 corrections** — STILL VALID
- The 7 corrections (d647a1e0, 5ab14de0c, 42b772cd3, 8f4ebae95,
  f14c38790, 79e09cd8b, 926d0aa56) are all docs-only commits
  that document the validation work.
- These are independent of the upstream sync and can be
  cherry-picked onto a fresh branch on top of master.
- Net cost: 30 min. **Net benefit: same as round 7 (preserves
  the session doc history).** But this DOES NOT solve the
  v0.88.0 sync problem.

**Option 3: Full rebase** — STILL VALID, with revised cost
- Round 7 said "14-17 hours focused work."
- The rebase must NOT lose the v0.88.0 sync, the RUSTSEC triages,
  the herdr fix, the build-meta fixes, or any of the other 2003
  non-doc commits.
- The conflict zone (118 files) is dominated by upstream
  v0.88.0-vs-PR #33-decollision-rename. Most conflicts will be
  mechanical (rename resolution).
- **Revised cost: 14-17 hours is correct, but this is the ONLY
  option that preserves the actual v0.88.0 work.**

**Option 4 (NEW): Cherry-pick critical fixes only** — RECOMMENDED
- Cherry-pick just the ~10 critical non-doc fixes (herdr, RUSTSEC
  triages, build-meta, ssh-agent, security allowlist) onto a fresh
  branch on top of master.
- This bypasses the v0.88.0 sync (which is more work) but
  preserves the security and quality fixes.
- Cost: 1-2 hours
- Risk: the fixes may not apply cleanly to post-PR #33 master

**Option 5 (NEW): Re-sync upstream v0.88.0 directly to current
master** — ALTERNATIVE
- Discard PR #23 entirely; re-do the v0.88.0 sync against
  current master (901b3310).
- Cost: 4-8 hours (the v0.88.0 changes are well-understood at
  this point; the session docs capture the work).
- Risk: misses the 2003 commits' context, but produces a clean
  v0.88.0 sync against current master.
- This is what a typical fork would do.

## Finding 4: 6 issues #37-#42 status (corrected from quick scan)

| # | Title | Status | Operator decision needed |
|---|---|---|---|
| 37 | parse_semver installer regex ignores channel | OPEN, real work needed | Accept the proposed 1-2h fix? |
| 38 | cargo fmt: 103 pre-existing dirty files | OPEN, real work needed | Pick option A/B/C? |
| 39 | Decompose 4 ratcheted budget baselines | OPEN, real work needed | Accept the 4-8h refactor? |
| 40 | herdr: verify explicit re-export list | OPEN, but the **original fix is on PR #23's branch only** — if PR #23 is closed, this issue becomes unfixable (the fix is lost) | Decision on PR #23 = decision on this issue |
| 41 | Copilot AI review quota exceeded | OPEN, just a state record | Operator decision: wait for reset, skip for this PR, or re-request on follow-ups |
| 42 | Inbox notification gap (deferral works, no OS notification) | OPEN, real work needed | Add OS notification to phinbox |

**Cross-link:** The decision on PR #23 is the same as the
decision on issue #40. The "close PR #23" option (round 7)
implicitly chooses to drop the herdr fix and re-open issue #40
as unfixable. The operator should make these decisions together.

## Lesson captured (round 9)

**29. When proposing a "close as superseded" or "abandon" option
for a PR, always count the non-doc commits first.** Round 7
estimated "close PR #23: 5 min" without checking how much work
would be lost. The actual cost is 2003 non-doc commits including
the upstream v0.88.0 sync, RUSTSEC triages, build-meta fixes,
and ~50 other fixes. The 5-minute estimate was wrong; the real
cost is "re-do the v0.88.0 sync from scratch." Lesson: when
proposing to abandon a branch, the time cost is not "5 min" —
it's "re-do all the non-doc work that was on the branch." Always
do a `git log merge-base..HEAD --pretty=format:"%s" | grep -vc
"^docs"` count first.

**30. The 3-way "close vs cherry-pick vs rebase" choice for a
stale PR must be augmented with a 5th option: "re-do the work
on the new base."** Round 7 framed the choice as binary between
"cherry-pick corrections only" (which loses the actual work) and
"full rebase" (which is expensive). A third path is "re-do the
upstream sync on the current master" — typically 4-8 hours, but
cleaner than the rebase. Lesson: for a PR that brings in a major
upstream sync, the cleanest path is often to re-do the sync
rather than rebase the original.

## Net corrections through 9 rounds

| # | Claim | Round 9 actual | Status |
|---|---|---|---|
| 1 | "Close as superseded: 5 min cost" | Lose 2003 non-doc commits including the v0.88.0 sync | ❌ massively wrong |
| 2 | "PR #23 is just docs + corrections" | 2157 commits, only 154 docs | ❌ wrong characterization |
| 3 | "Round 7 3-way choice covers all options" | 5 options (3 + re-do + partial-cherry-pick) | ❌ incomplete |
| 4 | "Issue #40 says the herdr fix is already closed" | True on the branch only; NOT on master | ⚠️ subtle |
| 5 | "PR #33 included the v0.88.0 sync" | PR #33 was a rename, NOT a sync | ❌ wrong |
| 6 | "Issues #37-#42 all need work" | #41 is a state record, #40's fix depends on PR #23 | ⚠️ partial |

## What this changes for the operator

The PR #23 decision matrix is now:

| Option | Cost | Net effect |
|---|---|---|
| Close as superseded | 5 min | **Lose v0.88.0 sync + 2002 other commits** (4-8h to re-do) |
| Cherry-pick 7 corrections | 30 min | **Lose v0.88.0 sync + 1996 other commits** (still need 4-8h re-do) |
| Cherry-pick critical fixes only (~10) | 1-2h | **Lose v0.88.0 sync + 1993 other commits** |
| Full rebase | 14-17h | Preserves everything; 118 conflict files |
| Re-do v0.88.0 sync on current master | 4-8h | Cleanest, but discards the PR #23 work entirely |

**My recommendation:** Option 5 (re-do v0.88.0 sync on current
master). It's the cleanest, the session docs (the docs commits
that would be cherry-picked under option 2/3) document the work
that was done, and a fresh v0.88.0 sync against the renamed
kcode-* crates is what a healthy fork would do anyway. The
operator's choice, of course.
