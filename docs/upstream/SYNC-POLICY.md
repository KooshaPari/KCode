# Upstream Sync Policy for KCode

KCode is a fork of upstream `1jehuang/jcode`. We diverge intentionally; we
do not attempt to stay in lockstep with upstream master.

## Baseline

**Pinned baseline (historical)**: `901b33108` (squash-merge of the historical
decollision work; merged 2026-10-08, reverted 2026-10-10 in PR #36).

The fork is currently ~10,283 commits behind upstream master (as of
2026-10-17). The bulk of that gap is closed-over upstream history that the
fork does not need; we are NOT planning to rebase onto the latest upstream
in a single operation — the conflict surface is too large.

The current working tree of `master` is `028ae83b7` (2026-10-17). The
decollision work that defines KCode's identity lives on the working tree
in the form of pre-PR-#33 commits (the `046ea2af5` parent and its
descendants), NOT in the broken `901b33108` squash-merge. The historical
squash-merge is preserved as a reference point only.

## Sync cadence

Quarterly (every 3 months) or on upstream security advisory, whichever
comes first.

The sync process is:

1. `git fetch upstream` (or the latest tag/release).
2. `git log upstream/master --oneline | head -30` — review what's new.
3. Identify valuable changes (see "Pick criteria" below).
4. Cherry-pick individually, with separate commits and separate PRs.
5. NEVER rebase or merge upstream/master wholesale.

Reminder cadence (informal): when reviewing the quarterly roadmap, run
`git log upstream/master --oneline | head -20` to see what's new and
decide whether to spin a cherry-pick lane.

## Pick criteria (cherry-pick what)

- Security fixes (CVEs, advisory mentions).
- Performance fixes (benchmarks, allocations).
- Deprecation removals (using deprecated APIs we still call).
- Bug fixes for known issues.
- Test infrastructure improvements.
- Small, mechanically-applicable changes (< 200 lines diff).

## Don't pick (do not cherry-pick)

- New features.
- Breaking API changes.
- Upstream-only refactors that conflict with the decollision work
  (jcode → kcode renames, crate restructuring, etc.).
- Anything that depends on a chain of recent upstream commits.
- Windows-only changes (the fork is macOS/Linux focused).
- Changes that require a chain of dependent upstream commits to compile.

## Conflict policy

If a cherry-pick conflicts:

1. `git cherry-pick --abort`.
2. Document the conflict in the PR description.
3. Surface to operator — the sync may need a manual review.

Do NOT force a cherry-pick through with arbitrary resolutions. The risk of
silently introducing a wrong fix is too high.

If the conflict is too large for one PR, do NOT silently absorb it — open
a follow-up lane task to handle the cherry-pick manually with full review.

## When to consider a full rebase

A full rebase onto a newer upstream is justified when:

1. The upstream has changed so much that cherry-picks are no longer
   mechanically possible.
2. The fork is so far behind that security patches can't be backported
   cleanly.
3. A specific upstream feature has become essential to the fork's purpose.

The last full rebase attempt (PR #33, SHA `901b33108`) was reverted
because it was lossy: `cargo check --workspace` failed with 6+ errors
across 3 crates (kcode-base, kcode-swarm-core, kcode-setup-hints). The
revert landed in PR #36 at SHA `f19b47b19`. The next attempt must be
done conflict-by-conflict with stricter validation per the postmortem in
`docs/sessions/2026-10-12-constitution-reconciliation/`.

## Branch-point for sync work

Sync work (cherry-picks, doc updates) lives on feature branches off
`master`, not off `901b33108`. Branching from the historical squash-merge
produces a broken tree (`cargo check` fails on the baseline alone), so
the policy doc and cherry-pick pilots start from `master` HEAD. The
historical squash-merge `901b33108` is preserved as a reference commit
only — it is NOT a working branch base.

The frozen-history branches that must NOT be touched during sync work:

- `feature/rebase-onto-v0.90.0` (PR #33 source branch — already reverted)
- `revert/pr-33-merge` (PR #36 source branch — already merged to master)

## Pilot (2026-10-17)

The first cherry-pick under this policy is captured in the commit log of
branch `refactor/upstream-sync-policy-2026-10-17`. As of this writing,
no cherry-pick landed. The candidates surveyed:

| Upstream SHA | Subject | Files | Lines | Verdict |
|---|---|---|---|---|
| `78e77cd2e` | fix: apply speed tier changes promptly | 24 | 481 | too broad; touches decollided crates |
| `ec6efc35a` | fix(tui): suppress repeated unchanged fast-mode notices | 3 | 48 | smallest; touches `jcode-tui/*` (renamed `kcode-tui/*`) — would need 3-way resolution |
| `217c196bb` | fix(windows): use cancellable duplex named-pipe I/O | 4 | 478 | Windows-only; out of fork scope |

No cherry-pick landed in this pilot because:

- The fork's decollision (jcode → kcode renames, crate restructuring)
  prevents clean mechanical application of upstream patches.
- Cherry-picking the smallest candidate (`ec6efc35a`) would still require
  manual path mapping from `crates/jcode-tui/...` to
  `crates/kcode-tui/...` plus handling the TUI submodule renames.
- Without a clean cherry-pick baseline to verify against, the value of a
  half-applied patch is unclear.

A future sync lane can re-attempt the cherry-pick pilot after a
mechanical-cherry-pick helper is in place (e.g. a sed-based path mapper
for `jcode-*` → `kcode-*` in cherry-pick context).

If no cherry-pick lands in the next 3 quarters, that's also a valid
result — sometimes upstream has nothing valuable to backport, and the
fork is intentionally divergent.
