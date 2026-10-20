# Merge Integration Plan — KCode PR Stack (2026-10-19)

Status: ready for operator review | Date: 2026-10-19 | Author: subagent (merge triage)
Session: `docs/sessions/2026-10-19-orchestration-governance/`
Repo: `KooshaPari/KCode` @ `b9886143c13e683111aaf73d7f3ceab655860685`
Local clone: `/Users/kooshapari/CodeProjects/Phenotype/repos/jcode`

---

## 1. Purpose and scope

This document is the **fix-forward integration plan** for the 11 open PRs on
`KooshaPari/KCode` that touch the broken CI pipeline on master. It exists to
let the operator land every PR in a single, no-force-push sequence that
clears `Format` and `Quality Guardrails`, resolves the `cli_exit_status.rs`
compile error, and gates the cache-TTL test regression behind `#[cfg(test)]`.

Scope: 11 PRs (`#127, #129, #132, #134, #137, #140, #144, #145, #148, #150,
#152`). The 6 unrelated open PRs (`#138, #124, #123, #121, #119, #107`) and
2 stale PRs (`#10, #8`) are flagged out-of-scope in §7.

Hard constraints (binding for every step below):

- NO force-push, NO rebase of published branches, NO `git reset --hard`, NO
  `git reset HEAD~`, NO `git clean -fd`, NO `git branch -D`, NO `gh pr merge`,
  NO `gh pr close`. The operator merges via the GitHub UI or via a local
  fast-forward/squash merge; agent never rewrites history.
- All commits authored by subagents carry `tx-agent` / `tx-validated` ledger
  trailers.
- The plan must be **re-verified against the live PR list** before
  execution — at least one of the PRs in scope (`#152`) opened during the
  triage window and there is no guarantee no further PRs have appeared.
  Operator must run `gh pr list --repo KooshaPari/KCode --state open --json
  number,title,headRefName` immediately before step 1 and abort if the list
  has drifted.

---

## 2. Verified stack state (re-checked 2026-10-19T23:09Z)

Local repo: `/Users/kooshapari/CodeProjects/Phenotype/repos/jcode`.
`origin/master` = `b9886143c13e683111aaf73d7f3ceab655860685`.
All ancestry numbers verified via `git rev-list --count
origin/master..origin/<branch>`. PR metadata verified via `gh pr list`.

| PR  | Branch                                          | ahead | files | Issue | File set                                                                                                                            |
|----:|-------------------------------------------------|------:|------:|-------|-------------------------------------------------------------------------------------------------------------------------------------|
| #127 | `fix/update-product-warnings`                   | 1     | 1     | #142  | `crates/jcode-app-core/src/update_product.rs`                                                                                       |
| #129 | `fix/repo-ci-pre-existing-drift`                | 4     | 10    | #130  | repo-wide rustfmt + 2 clippy fixes + `tests/cli_exit_status.rs` (wrong fix)                                                         |
| #132 | `fix/tui-decollision-test-expectations`         | 6     | 13    | #131  | #129 commits merged-in (`f8f488392`, `1922c0f56`) + 3 TUI test literal fixes                                                       |
| #134 | `fix/cache-estimate-not-evidence`               | 7     | 14    | #133  | #129 + #132 commits merged-in + `cache_expected_warm` semantics fix                                                                  |
| #137 | `fix/setup-hints-orphan-spawn-notice`           | 1     | 1     | #136  | `crates/jcode-setup-hints/src/lib.rs` (dead-code removal)                                                                           |
| #140 | `fix/setup-hints-pr128-test-regression`         | 2     | 2     | #139  | `lib.rs` + `setup_hints_tests.rs` (SUPERSET of #137)                                                                                |
| #148 | `fix/sdk-test-paths-kcode-prefix`               | 1     | 4     | #147  | `sdk/typescript/test/{client.test.ts,launch.test.ts,live-options.mjs,mock-harness.ts}`                                               |
| #150 | `fix/tui-cache-expected-warm-unused-import`     | 1     | 1     | #149  | `crates/jcode-tui/src/tui/mod.rs` (gates re-export behind `#[cfg(test)]`)                                                          |
| #145 | `docs/orchestration-governance-docs-only`        | 1     | 1     | #143  | `docs/sessions/2026-10-19-orchestration-governance/04_ORCHESTRATION_GOVERNANCE_PROPOSAL.md` (new file, 276 lines)                     |
| #144 | `docs/orchestration-governance-proposal`        | 3     | 3     | #143  | SUPERSEDED — touches `crates/jcode-setup-hints/src/lib.rs` + `setup_hints_tests.rs` + the same docs file. Close in favour of #145 + #140 |
| #152 | `fix/cli-exit-status-kcode-bin`                 | 1     | 1     | #151  | `tests/cli_exit_status.rs` (correct `env!("CARGO_BIN_EXE_kcode")` fix; supersedes the wrong runtime `std::env::var` attempt in #129) |

Notes:

- PR #152 was created at 2026-10-19T22:34:50Z (after this triage session
  started). The plan was originally drafted assuming it would need to be
  discovered by title search; it has been incorporated into the sequence
  below. **Step 0 explicitly re-runs the search to confirm no further PRs
  have appeared since this was written.**
- `crates/jcode-tui/src/tui/mod.rs` shows up in PR #132, #134, and #150.
  Verified by `git diff origin/<branch> -- crates/jcode-tui/src/tui/mod.rs`
  for each.

---

## 3. Blocking defects

Two independent blockers must be resolved before `Quality Guardrails`
becomes green.

### 3.1 Wrong `cli_exit_status.rs` fix in #129

Master at `b9886143c`:

```rust
let output = Command::new(env!("CARGO_BIN_EXE_jcode"))
```

Cargo.toml `[[bin]]` target is named `kcode` (`path = "src/main.rs"`),
so `CARGO_BIN_EXE_jcode` is **never defined at compile time** — the
integration test fails to compile with E0461 (environment variable not
defined at compile time).

PR #129 attempted the fix as:

```rust
let output = Command::new(std::env::var("CARGO_BIN_EXE_jcode").expect("CARGO_BIN_EXE_jcode"))
```

This is also wrong: `CARGO_BIN_EXE_*` is set by Cargo **at compile time**
for integration test targets, never as a runtime environment variable. The
test would compile but panic at runtime with "environment variable not
found".

The correct form is `env!("CARGO_BIN_EXE_kcode")` (which PR #152 ships):

```rust
let output = Command::new(env!("CARGO_BIN_EXE_kcode"))
```

Verified on `tests/cli_exit_status.rs` from PR #152, branch
`fix/cli-exit-status-kcode-bin`, commit `d639961e4`, ahead of master by
exactly 1 commit, touching only that one file.

### 3.2 Accumulated-merge duplicates in #132 and #134

PR #132 is **6 commits** ahead of master:

```
f8f488392  merge: bring in PR #129's fmt + clippy fixes
1922c0f56  fix(jcode-tui-mermaid): collapse split doc comment
9ead3963e  fix(tui): 3 leftover kcode/jcode decollision mismatches
68e322d8a  Merge remote-tracking branch 'origin/master' into fix/repo-ci-pre-existing-drift
5b65b1878  fix(ci): collapse nested if-let in sockets.rs (clippy collapsible_if)
36375a526  fix(ci): clear repo-wide rustfmt drift + fix env! macro in cli_exit_status
```

The last three (`68e322d8a`, `5b65b1878`, `36375a526`) are byte-equivalent
to #129's three rustfmt/clippy/env! commits. The `f8f488392` merge commit
brought them in explicitly to make #132 pass CI standalone.

PR #134 is **7 commits** ahead of master. Its last commit
(`c182fcb7c`, "cache_expected_warm returns false on estimates, not just
cold") is the only #134-specific change. The first six are identical to
#132's six (and therefore inherit #129's three).

**Consequence of merge order:**

If the operator merges in the order #129 → #132 → #134:

1. After #129: master carries the rustfmt/clippy/env! commits. The env!
   fix is wrong (see §3.1) but `cargo check` will pass because
   `std::env::var(...)` is valid Rust even if it never executes.
2. After #132: git applies #132's three new commits
   (`9ead3963e`, `1922c0f56`, `f8f488392`). The `f8f488392` merge commit
   is a no-op against the just-merged #129 content (its purpose was to
   bring #129's content into #132's branch, which is now redundant). Git
   will accept this as a clean fast-forward because all of #132's
   pre-merge content already matches what #129 just brought in. The two
   new fixes (`9ead3963e` TUI literals, `1922c0f56` mermaid comment) apply
   cleanly because their contexts are unaffected by #129.
3. After #134: git applies #134's lone semantic change `c182fcb7c`. Clean.
   The mod.rs reorder of `cache_expected_warm` from position A to
   position B happens here.

If the operator merges in any other order:

- #134 first: master has no rustfmt/clippy cleanup yet. #134's branch
  builds against #129's content; pulling that into a master without
  rustfmt will produce a cascade of fmt/clippy failures in the
  integration test infrastructure. The merge will technically succeed
  but `Quality Guardrails` will go red.
- #132 first: same problem as #134 first.
- #129 last: the wrong `cli_exit_status.rs` fix lands last and is the
  blocking CI failure that motivated the whole stack. No benefit.

**Conclusion:** the correct order is #152 → #127 → #140 → #137 close →
#148 → #129 → #132 → #134 → #150 → #145, with #144 closed (operator
action, no merge).

### 3.3 Textual conflict: #134 vs #150 in `crates/jcode-tui/src/tui/mod.rs`

Verified by `git diff origin/<branch>` against master on both branches:

Master at line 1069 of `crates/jcode-tui/src/tui/mod.rs`:

```rust
mod kv_cache_problem;
pub(crate) use kv_cache_problem::cache_expected_warm;       <- position A
#[cfg(test)]
pub(crate) use kv_cache_problem::KvCacheProblemKind;
pub(crate) use kv_cache_problem::detect_kv_cache_problem;
```

PR #134 (after applying) at the same location:

```rust
mod kv_cache_problem;
#[cfg(test)]
pub(crate) use kv_cache_problem::KvCacheProblemKind;
pub(crate) use kv_cache_problem::cache_expected_warm;       <- moved to position B, NO cfg(test)
pub(crate) use kv_cache_problem::detect_kv_cache_problem;
```

PR #150 (after applying) at the same location:

```rust
mod kv_cache_problem;
#[cfg(test)]
pub(crate) use kv_cache_problem::KvCacheProblemKind;
#[cfg(test)]
pub(crate) use kv_cache_problem::cache_expected_warm;       <- moved to position B, WITH cfg(test)
pub(crate) use kv_cache_problem::detect_kv_cache_problem;
```

Both branches perform an identical "remove from position A" operation.
Both branches add a `cache_expected_warm` line at position B, but #150
also inserts a `#[cfg(test)]` attribute one line above it. The two
insertion sites overlap (they both target the gap between `KvCacheProblemKind`
and `detect_kv_cache_problem`) but produce different content (#134 adds
1 line, #150 adds 2 lines).

**Verdict: textual conflict, yes.** Git's three-way merge will report a
conflict on the position-B insertion. The operator (or auto-resolve
script) must take #150's version — that is the desired final state
(both re-exports gated behind `#[cfg(test)]`).

---

## 4. Recommended merge sequence

Numbered, with per-step commands and verification. The operator runs the
GitHub UI "Merge pull request" button (squash or rebase is fine — both
`Merge commit` and `Squash and merge` work because the conflict is
contained in the mod.rs file and is resolved manually before push). The
local `git fetch && git merge --no-ff` alternative is shown only where
the auto-merge UI cannot resolve a conflict.

### Step 0: Pre-flight verification

```bash
cd /Users/kooshapari/CodeProjects/Phenotype/repos/jcode
git fetch origin
gh pr list --repo KooshaPari/KCode --state open --json number,title,headRefName,isDraft \
  > /tmp/open-prs-pre-flight.json
test -s /tmp/open-prs-pre-flight.json
```

Compare the resulting list against the table in §2. **If any of
`#127, #129, #132, #134, #137, #140, #144, #145, #148, #150, #152` is
missing**, or if any new open PR appears with a title matching
`/cli_exit_status|CARGO_BIN_EXE|tui.*test|format|drift/i`, STOP and
re-read this plan before continuing.

### Step 1: Merge #152 (`fix/cli-exit-status-kcode-bin`)

This is the correct `cli_exit_status.rs` fix. Merge first so it is in
place before #129's wrong fix is even considered.

**Operator action (UI):**

1. Open PR #152.
2. Review the diff: 4 lines added, 4 lines removed, single file
   `tests/cli_exit_status.rs`. The diff is the canonical fix described
   in §3.1: `CARGO_BIN_EXE_kcode` (not `jcode`), `KCODE_HOME` (not
   `JCODE_HOME`), `.starts_with("kcode ")` (not `"jcode "`).
3. Click `Merge pull request` → `Squash and merge` (or `Merge commit`).

**Verification after merge:**

```bash
git pull --ff-only origin master
git show origin/master:tests/cli_exit_status.rs | head -16
# expect line 6: let output = Command::new(env!("CARGO_BIN_EXE_kcode"))
```

Wait for `Quality Guardrails` (cargo check --all-targets --all-features) on
the resulting master SHA. Expected: GREEN — the integration test now
compiles.

### Step 2: Merge #127 (`fix/update-product-warnings`)

Adds 7 `#[allow(dead_code)]` attributes to `update_product.rs`. Reviewer
should note that this is a deliberate CI-unblock with three real defects
hidden behind the allow-list (see §8 operator decisions). The merge is
correct; the cleanup is a follow-up.

**Operator action (UI):**

1. Open PR #127. Diff: 1 file, +7 lines (all `#[allow(dead_code)]`).
2. Click `Merge pull request` → `Squash and merge`.

**Verification after merge:**

```bash
git pull --ff-only origin master
git show origin/master:crates/jcode-app-core/src/update_product.rs | grep -c '#\[allow(dead_code)\]'
# expect 7
cargo check -p jcode-app-core --all-targets --all-features
```

Wait for `Quality Guardrails` and `Build & Test (ubuntu-latest)`. Expected:
GREEN.

### Step 3: Close #137 (operator action — no merge)

PR #137 (delete `with_spawn_notice` from `lib.rs`) is a strict subset of
PR #140 (same `lib.rs` change plus a `setup_hints_tests.rs` regression
guard). Merging both would create a textual conflict on `lib.rs` (both
remove the same `with_spawn_notice` block — git would auto-resolve, but
the second merge is redundant work).

**Operator action (UI):**

1. Open PR #137.
2. Post a comment: `Superseded by #140 (which also includes the
   lib.rs change plus the setup_hints_tests.rs regression guard). Closing
   this PR to keep the merge graph clean.`
3. Click `Close pull request`.

### Step 4: Merge #140 (`fix/setup-hints-pr128-test-regression`)

Removes `with_spawn_notice` from `lib.rs` (same as #137) AND rewrites the
`first_three_launches_can_include_hotkey_notice_too` test in
`setup_hints_tests.rs` to assert the now-retired behaviour.

**Operator action (UI):**

1. Open PR #140. Diff: 2 files, ~30 lines changed. The `lib.rs` change is
   a 7-line deletion; the `setup_hints_tests.rs` change is a net rewrite
   of the `first_three_launches_can_include_hotkey_notice_too` test.
2. Click `Merge pull request` → `Squash and merge`.

**Verification after merge:**

```bash
git pull --ff-only origin master
git show origin/master:crates/jcode-setup-hints/src/lib.rs | grep -c 'with_spawn_notice'
# expect 0
cargo test -p jcode-setup-hints --lib
```

Wait for `Quality Guardrails`. Expected: GREEN — the
`Quality Guardrails` job no longer fails on the orphaned
`with_spawn_notice` reference.

### Step 5: Merge #148 (`fix/sdk-test-paths-kcode-prefix`)

Renames 14 `jcode-`-prefixed tmp-path fixtures in
`sdk/typescript/test/{client.test.ts,launch.test.ts,live-options.mjs,mock-harness.ts}`
to `kcode-` prefix so `assertKcodePath` stops rejecting them. The
`TypeScript SDK` CI job had been timing out at 10 minutes because every
test fixture had been rejected at the path-isolation guard, leaving the
job with nothing to do but wait.

**Operator action (UI):**

1. Open PR #148. Diff: 4 files, all TypeScript test fixtures. No
   production code changes. No semantic changes (the path prefix is the
   only thing changing — the test logic is untouched).
2. Click `Merge pull request` → `Squash and merge`.

**Verification after merge:**

```bash
git pull --ff-only origin master
grep -rn 'os.tmpdir(), `jcode-' sdk/typescript/test/ | head
# expect 0 hits
```

Wait for `TypeScript SDK`. Expected: GREEN within 10 minutes (previously
cancelled at 10m).

### Step 6: Merge #129 (`fix/repo-ci-pre-existing-drift`)

Repo-wide rustfmt drift + 2 clippy fixes in `sockets.rs` + the
**wrong** `cli_exit_status.rs` fix.

**Operator action (UI, two-stage because of the wrong fix):**

1. Open PR #129. Diff: 10 files, 66 insertions, 45 deletions.
2. Note in the review comment: the `cli_exit_status.rs` change is wrong
   (the `std::env::var("CARGO_BIN_EXE_jcode")` form panics at runtime).
   Step 1 already merged #152 which corrected this in a different commit
   on master. After this PR lands, master will contain BOTH the wrong
   fix (in #129) and the correct fix (in #152) — they will conflict.
3. Click `Merge pull request` → `Squash and merge`. After the merge,
   expect a `Quality Guardrails` failure on the now-double-fixed file.

**Verification after merge (manual conflict resolution required):**

```bash
git pull --ff-only origin master
cargo check --workspace --all-targets --all-features
# expected: compile error on tests/cli_exit_status.rs because #129
# re-introduces the wrong std::env::var line and #152's correct env!
# line is now also in master.
```

The operator (or a follow-up subagent, dispatched from this plan) must
resolve this conflict by reverting #129's `cli_exit_status.rs` hunk to
match #152's `env!("CARGO_BIN_EXE_kcode")` form. The recommended
mechanics are described in §5 as the **"post-#129 fix-forward patch"**.

### Step 7: merge #132 (`fix/tui-decollision-test-expectations`)

#132 is the "merge in #129's content + add 2 TUI fix commits" branch.
After #129 is in master, #132's accumulated-merge commits become a
no-op fast-forward (the content is already in master from #129), and
only #132's two specific commits (`9ead3963e`, `1922c0f56`) need to
apply.

**Operator action (UI):**

1. Open PR #132. Diff: 13 files.
2. Click `Merge pull request` → `Squash and merge`.

**Verification after merge:**

```bash
git pull --ff-only origin master
git show origin/master:crates/jcode-tui/src/tui/app/helpers.rs | grep -A 1 'launch_client_executable'
# expect PathBuf::from("kcode") not PathBuf::from("jcode")
cargo check -p jcode-tui --all-targets --all-features
```

Wait for `Quality Guardrails`. Expected: GREEN — the 7 leftover
kcode/jcode decollision mismatches are now correct.

### Step 8: Merge #134 (`fix/cache-estimate-not-evidence`)

#134 is the "merge in #129+#132 + add 1 semantic fix" branch. After #129
and #132 are in master, #134's accumulated-merge commits are a no-op
fast-forward, and only the lone commit `c182fcb7c` (the
`cache_expected_warm` semantics fix) needs to apply.

The commit also reorders `pub(crate) use kv_cache_problem::cache_expected_warm;`
in `tui/mod.rs` from position A to position B. This will conflict with
#150 (see §3.3); the conflict is resolved in step 9, not here.

**Operator action (UI):**

1. Open PR #134. Diff: 14 files.
2. Click `Merge pull request` → `Squash and merge`.

**Verification after merge (preparing for §3.3 conflict):**

```bash
git pull --ff-only origin master
git show origin/master:crates/jcode-tui/src/tui/mod.rs | sed -n '1067,1075p'
# expect:
#   mod kv_cache_problem;
#   #[cfg(test)]
#   pub(crate) use kv_cache_problem::KvCacheProblemKind;
#   pub(crate) use kv_cache_problem::cache_expected_warm;     <-- no cfg(test) yet
#   pub(crate) use kv_cache_problem::detect_kv_cache_problem;
```

### Step 9: Merge #150 with manual conflict resolution

PR #150 (`fix/tui-cache-expected-warm-unused-import`) is the smallest
change in the stack (1 commit, 1 file, 2 line changes) but it conflicts
textually with #134's reordering of the same line.

**Operator action (local merge, not UI — UI cannot resolve this):**

```bash
git fetch origin
git checkout master
git pull --ff-only origin master
git merge --no-ff origin/fix/tui-cache-expected-warm-unused-import
# expect a CONFLICT in crates/jcode-tui/src/tui/mod.rs
```

When the conflict reports:

```
Auto-merging crates/jcode-tui/src/tui/mod.rs
CONFLICT (content): Merge conflict in crates/jcode-tui/src/tui/mod.rs
```

Open the file and resolve. The conflict will be:

```diff
  mod kv_cache_problem;
  #[cfg(test)]
  pub(crate) use kv_cache_problem::KvCacheProblemKind;
<<<<<<< HEAD (after #134)
  pub(crate) use kv_cache_problem::cache_expected_warm;
=======
  #[cfg(test)]
  pub(crate) use kv_cache_problem::cache_expected_warm;
>>>>>>> origin/fix/tui-cache-expected-warm-unused-import
  pub(crate) use kv_cache_problem::detect_kv_cache_problem;
```

**Resolution: take the `=======` side** (PR #150's version). This is
the desired final state — `cache_expected_warm` gated behind
`#[cfg(test)]`, matching `KvCacheProblemKind` above it.

After resolving:

```bash
git add crates/jcode-tui/src/tui/mod.rs
git commit -m "merge: PR #150 — gate cache_expected_warm re-export behind #[cfg(test)] (#149)

Conflict resolved by taking PR #150's version. #134 reordered the
re-export from position A to position B (without the attribute); #150
reorders AND adds the #[cfg(test)] attribute. #150's stricter form is
the desired final state.

Resolves the textual conflict between #134 and #150 in
crates/jcode-tui/src/tui/mod.rs.

tx-agent: jcode
tx-validated: manual
tx-task: 05_MERGE_INTEGRATION_PLAN
tx-intent: ship cache_expected_warm gating forward
tx-scope: crates/jcode-tui/src/tui/mod.rs"
git push origin master
```

**Verification after merge:**

```bash
git pull --ff-only origin master
git show origin/master:crates/jcode-tui/src/tui/mod.rs | sed -n '1067,1076p'
# expect:
#   mod kv_cache_problem;
#   #[cfg(test)]
#   pub(crate) use kv_cache_problem::KvCacheProblemKind;
#   #[cfg(test)]
#   pub(crate) use kv_cache_problem::cache_expected_warm;
#   pub(crate) use kv_cache_problem::detect_kv_cache_problem;
cargo check -p jcode-tui --all-targets --all-features
cargo test -p jcode-tui --lib cache_expected_warm
```

Wait for `Quality Guardrails`. Expected: GREEN — the unused-import
warning is gone, the test still passes because `#[cfg(test)]` is on.

### Step 10: Merge #145 (`docs/orchestration-governance-docs-only`)

Pure documentation PR. Adds a single 276-line Markdown file with the
operator-asked orchestration-governance proposal. No Rust, no
TypeScript, no scripts.

**Operator action (UI):**

1. Open PR #145. Diff: 1 file (new), 276 lines.
2. Click `Merge pull request` → `Squash and merge`.

**Verification after merge:**

```bash
git pull --ff-only origin master
test -f docs/sessions/2026-10-19-orchestration-governance/04_ORCHESTRATION_GOVERNANCE_PROPOSAL.md
wc -l docs/sessions/2026-10-19-orchestration-governance/04_ORCHESTRATION_GOVERNANCE_PROPOSAL.md
# expect 276
```

No CI gating change expected.

### Step 11: Close #144 (operator action — superseded)

PR #144 carries the same docs file as #145 (276 lines) PLUS the two
setup-hints files that #140 already ships.

**Operator action (UI):**

1. Open PR #144.
2. Post a comment: `Superseded by #145 (docs only) and #140 (the
   setup-hints files this PR also touched). The docs in this PR are
   identical to #145's; the setup-hints change is in #140. Closing.`
3. Click `Close pull request`.

### Step 12: post-#129 fix-forward patch (see §5)

If the operator accepted step 6 as-is, master now has BOTH #152's
correct fix and #129's wrong `std::env::var` form. A follow-up commit
must remove the wrong line. This is dispatched as a separate
`fix-forward` PR; details in §5.

### End-state summary

After all steps, master should pass:

- `cargo fmt --all -- --check` → exit 0
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  → exit 0
- `cargo check --workspace --all-targets --all-features` → exit 0
  (specifically `tests/cli_exit_status.rs` compiles)
- `Quality Guardrails` → GREEN
- `Format` → GREEN
- `TypeScript SDK` → GREEN within 10m (previously cancelled)

The remaining failures (macOS provider-matrix, Windows setup-hints,
Ubuntu TUI test failures tracked by #135) are pre-existing and not in
the scope of this stack.

---

## 5. PR-by-PR disposition table

| PR   | Disposition             | Reason (one line)                                                                                                  |
|----:|------------------------|------------------------------------------------------------------------------------------------------------------|
| #127 | MERGE (step 2)         | Suppresses 7 dead_code warnings on `update_product.rs`; correct CI-unblock; #141 follow-up tracks real defects. |
| #129 | MERGE (step 6) + fix-forward patch (step 12) | Repo-wide rustfmt + 2 clippy fixes; the cli_exit_status.rs fix is wrong but is corrected by #152 which is already on master; a post-merge fix-forward patch removes the wrong line. |
| #132 | MERGE (step 7)         | 2 TUI fix commits (`9ead3963e`, `1922c0f56`); after #129 is in master, #132's accumulated-merge commits are a no-op fast-forward. |
| #134 | MERGE (step 8)         | 1 semantic fix (`c182fcb7c`) to `cache_expected_warm`; after #129 and #132 are in master, #134's accumulated-merge is a no-op. |
| #137 | CLOSE (step 3)         | Strict subset of #140; merging both would create a redundant `with_spawn_notice` removal conflict on `lib.rs`. |
| #140 | MERGE (step 4)         | Strict superset of #137 plus a regression guard in `setup_hints_tests.rs`. |
| #144 | CLOSE (step 11)        | SUPERSEDED — its docs file is identical to #145's, and its setup-hints files are #140.                            |
| #145 | MERGE (step 10)        | Clean docs-only re-land of #144's docs portion.                                                                  |
| #148 | MERGE (step 5)         | Renames 14 jcode-prefixed test paths so the `TypeScript SDK` job stops timing out.                                |
| #150 | MERGE (step 9)         | Gates `cache_expected_warm` re-export behind `#[cfg(test)]`; conflicts with #134, resolved by taking #150's side. |
| #152 | MERGE (step 1)         | The correct `env!("CARGO_BIN_EXE_kcode")` fix; supersedes #129's wrong `std::env::var` attempt on this one file. |

**Critical cli verdict (see §3.1):** #129's `cli_exit_status.rs` change is
wrong (uses `std::env::var` on a compile-time-only variable). The
correct fix is already on master as PR #152. The plan lands #152 first
(step 1), then #129 (step 6) which will re-introduce the wrong line on
top of #152's correct line — a textual conflict that is resolved by the
post-merge fix-forward patch (step 12, detailed below).

**Conflict verdict #134 vs #150 (see §3.3):** textual conflict, YES.
Both branches modify the same line in `crates/jcode-tui/src/tui/mod.rs`
(position A of the `cache_expected_warm` re-export). #134 removes from
position A and re-adds at position B without `#[cfg(test)]`. #150
removes from position A and re-adds at position B with `#[cfg(test)]`
attribute. Three-way merge will report a conflict on the position-B
insertion. Resolution: take #150's side (stricter gating).

### Step 12 details: post-#129 fix-forward patch

After step 6, master has:

```rust
let output = Command::new(env!("CARGO_BIN_EXE_kcode"))                <- from #152
// plus, somewhere, the wrong std::env::var line from #129
```

Wait — #152 only touches 4 lines and only modifies `tests/cli_exit_status.rs`.
#129 also only touches the same file with 2 lines (1 +, 1 -). After both
land, the file will have one of two outcomes:

- **Outcome A (most likely):** git's squash-merge of #129 sees that #152
  already fixed the same line; squash collapses to a no-op or to a
  partial revert. In this case, the fix-forward patch is unnecessary.
- **Outcome B (also likely):** #129's squash lands a commit that
  reverts #152's fix back to the wrong form. The fix-forward patch is
  required.

Either way, the operator (or a subagent dispatched after step 6) must
verify the file state and apply a fix-forward if needed:

```bash
git pull --ff-only origin master
grep -n 'CARGO_BIN_EXE' tests/cli_exit_status.rs | head -5
```

If the grep shows `std::env::var("CARGO_BIN_EXE_jcode")`:

```bash
# Fix-forward: restore #152's correct line
git checkout origin/fix/cli-exit-status-kcode-bin -- tests/cli_exit_status.rs
git add tests/cli_exit_status.rs
git commit -m "fix(tests): restore CARGO_BIN_EXE_kcode form after #129 re-introduced wrong std::env::var (#129 follow-up)

PR #129's attempt at fixing tests/cli_exit_status.rs:6 used
std::env::var(\"CARGO_BIN_EXE_jcode\").expect(...), which fails at runtime
because CARGO_BIN_EXE_* is set only at compile time. PR #152 shipped
the correct env!(\"CARGO_BIN_EXE_kcode\") form, but #129's squash reverts
that to the broken form. This commit restores the correct form.

tx-agent: jcode
tx-validated: manual
tx-task: cli-exit-status-kcode-bin
tx-intent: keep #152's correct fix on master"
git push origin master
```

If the grep shows `env!("CARGO_BIN_EXE_kcode")`:

The squash merge auto-resolved cleanly; no fix-forward needed.

**Verification after step 12:**

```bash
grep -n 'CARGO_BIN_EXE' tests/cli_exit_status.rs | head -5
# expect: env!("CARGO_BIN_EXE_kcode")  (no skip, no std::env::var)
cargo test --test cli_exit_status --no-run
```

Expected: GREEN. This file now compiles cleanly on master.

---

## 6. Verification gates

After every step, the operator runs a subset of the following. After the
final step, all of them must pass.

```bash
cd /Users/kooshapari/CodeProjects/Phenotype/repos/jcode
git checkout master
git pull --ff-only origin master

# 1. cli_exit_status.rs is correct on master
grep -rn "CARGO_BIN_EXE_jcode" tests/ src/ crates/
# expected: 0 hits (the wrong file is gone)

grep -rn "CARGO_BIN_EXE_kcode" tests/ src/ crates/
# expected: 1 hit (tests/cli_exit_status.rs:6)

# 2. update_product.rs carries 7 #[allow(dead_code)] markers
grep -c '#\[allow(dead_code)\]' crates/jcode-app-core/src/update_product.rs
# expected: 7

# 3. setup-hints no longer carries the orphaned with_spawn_notice helper
grep -c 'fn with_spawn_notice' crates/jcode-setup-hints/src/lib.rs
# expected: 0

# 4. SDK test fixtures have no jcode-prefixed tmpdir paths
grep -rn 'os.tmpdir(), `jcode-' sdk/typescript/test/
# expected: 0 hits

grep -rn 'os.tmpdir(), `kcode-' sdk/typescript/test/ | wc -l
# expected: 14 hits (was 14 jcode-, now 14 kcode-)

# 5. cache_expected_warm re-export is gated behind #[cfg(test)]
sed -n '1067,1076p' crates/jcode-tui/src/tui/mod.rs
# expected:
#   mod kv_cache_problem;
#   #[cfg(test)]
#   pub(crate) use kv_cache_problem::KvCacheProblemKind;
#   #[cfg(test)]
#   pub(crate) use kv_cache_problem::cache_expected_warm;
#   pub(crate) use kv_cache_problem::detect_kv_cache_problem;

# 6. Whole-workspace compile + lint pass
cargo fmt --all -- --check
# expected: exit 0
cargo clippy --workspace --all-targets --all-features -- -D warnings
# expected: exit 0
cargo check --workspace --all-targets --all-features
# expected: exit 0

# 7. CI checks (GitHub Actions)
gh pr checks --repo KooshaPari/KCode $(git rev-parse origin/master)
# expected on the latest master SHA:
#   Quality Guardrails     pass
#   Format                 pass
#   TypeScript SDK         pass  (no longer cancelled at 10m)
#   Build & Test (ubuntu)  pass  (or pre-existing #135 failures remain)
```

If any of (1)-(6) fails after step 12, the plan has a gap; revert to
step 11 and dispatch a triage subagent.

---

## 7. Pre-existing / out-of-scope failures

The following PRs are open on `KooshaPari/KCode` but are unrelated to
the CI stack above. Verified by reading their titles and bodies via
`gh pr view <n> --json title,body,headRefName`.

| PR  | Title                                                              | Scope                                                                              | Disposition                                                                                                  |
|----:|--------------------------------------------------------------------|------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------|
| #138 | fix(portal): XJ-03 bridge — `--host 127.0.0.1`                     | `scripts/*` and `docs/sessions/2026-10-15-portal-xirp-resume/STATE.md` (Portal OAuth bridge). | OUT OF SCOPE. Touches no Rust or TypeScript; CI failures on this PR are explicitly tracked as pre-existing. |
| #124 | fix(tui): keep Windows keyboard negotiation on native console input | `crates/jcode-tui/src/` — cherry-pick of #97.                                       | OUT OF SCOPE. Windows-specific; not on the macOS/Linux path that this stack unblocks.                      |
| #123 | fix(provider-core): opt out of proxy discovery for qualification   | `crates/jcode-provider-core/` — cherry-pick of #122.                                | OUT OF SCOPE. Provider qualification change; not on the CI-fix path.                                       |
| #121 | fix(setup-hints): align launch-hotkey markers with the kcode decollation | `crates/jcode-setup-hints/` — cherry-pick of #120.                              | OUT OF SCOPE. Touches different files than #140 (#140 covers `with_spawn_notice`; #121 covers Linux keymap). |
| #119 | feat(tui): /poke context recovery (PR #25, closes #26)              | `crates/jcode-tui/src/tui/app/commands_poke_context.rs` (new 118 LoC file).        | OUT OF SCOPE. Feature PR, not a CI fix. Independent test runs required before merging.                    |
| #107 | Native wake receipts on Jcode 0.98.1 source baseline                | `codex/native-wake-receipts-20261016` (draft, base is `codex/native-wake-base-1c7426e`, NOT master). | OUT OF SCOPE. Targets a non-master base branch; will not affect `KooshaPari/KCode` master unless rebased.   |
| #10  | feat(acp): discover sessions and broker remote interactions        | DRAFT, base is `feature/herdr-plugin-manifest`, NOT master.                        | OUT OF SCOPE. Long-running draft; unrelated to CI.                                                          |
| #8   | fix(openrouter): stop double-polling dead unfold after EOF (panic) + CI/lefthook coverage + herdr plugin manifest | Mixed features + herdr plugin manifest; already merged into a feature branch. | OUT OF SCOPE. No path to master without a separate decision.                                              |

**Setup Friction Eval** is pre-existing on master because `scripts/install.sh`
is an intentional KCode release-download stub (release-policy). Do not
attempt to fix it; it is not caused by any open PR.

**macOS provider-matrix** failures are credential/environment failures,
not code. They will continue to fail until the CI environment is
reconfigured.

**Windows setup-hints** failures are pre-existing platform/fork-residue
failures. Out of scope for this stack.

**Ubuntu TUI test failures** beyond the 4 fixed by #132 and #134 are
tracked by issue #135 (21 failures across fork-command, session-picker,
terminal-mode, overscroll, visual/disconnect, keyboard-enhancement).
A worker is diagnosing these separately and may land more PRs. Those
PRs are not in this plan and should be sequenced after this stack
clears.

---

## 8. Open items requiring operator decision

Three decisions are deferred to the operator. None blocks this plan's
execution; all are post-merge follow-ups.

### Decision 1: #137 vs #140 (decide in step 3)

**Question:** the plan closes #137 and merges #140. Operator may
prefer to merge both and let git auto-resolve (the `with_spawn_notice`
removal is identical between the two branches, so git would auto-merge
without conflict). The plan's recommendation is **close #137, merge
#140** to keep the merge graph minimal and avoid a redundant merge
commit. Operator action: confirm in step 3.

### Decision 2: #127 blanket `#[allow(dead_code)]` vs #141 proper fix

**Question:** #127 silences 7 dead_code warnings with a blanket
`#[allow(dead_code)]`. This is correct for CI unblock but hides real
defects:

1. `fork_update_disallowed_error` (line ~115) is unwired — should be
   surfaced in `/update` error paths.
2. `is_inside_git_repo` (line ~167) duplicates `update.rs:160` —
   should be deleted and `update.rs` should call the canonical helper.
3. `resolve_update_repo` (line ~155) is dead with a stale doc comment
   that claims "the call sites in `super::update` do not each repeat
   the resolve-or-bail boilerplate" — but no such call sites exist
   any more after the #128 `refactor(setup-hints): remove unreachable
   startup spawn notice` merge landed.
4. `GITHUB_REPO`, `update_repo`, `update_repo_for` are unreachable
   because `mod update_product;` is private with no `pub use` in
   `mod.rs`. The allow-list masks this. Either delete them or wire
   them up via a `pub use` re-export.

**Recommendation: merge #127 now (CI unblock), open issue #141 (already
exists per the prompt) as a follow-up.** The four defects above are
real but each is contained within `update_product.rs` (no external
behaviour changes). They can be cleaned up in a follow-up PR.

**Alternative:** refuse to land the blanket allow and write a proper
fix PR first (would close all 4 defects and remove the allow-list). The
risk is that the proper fix is a 50-100 line PR across `update_product.rs`,
`update.rs`, and `mod.rs`, and it would block all the other PRs in this
stack from clearing CI.

### Decision 3: step 12 fix-forward patch

**Question:** after step 6, master may have BOTH #152's correct line
and #129's wrong line in `tests/cli_exit_status.rs`. The plan's
recommendation is **detect the conflict after step 6 and dispatch a
fix-forward patch if needed**.

**Alternative:** the operator could pre-emptively close PR #129 in
favour of a clean re-land that excludes the `cli_exit_status.rs`
change. This would require opening a new branch off master, applying
only #129's rustfmt/clippy commits (excluding the wrong env! change),
and landing that. The risk is more work (a new branch + PR) for the
same final outcome.

The plan's recommendation is the **detect-and-patch** approach because
it preserves the operator's authority over the merge order (close vs
land) and isolates the corrective commit to one file.

---

## 9. Appendix: command reference

These commands are the exact ones used to verify the plan and can be
re-run by a future agent.

```bash
# Repo state
cd /Users/kooshapari/CodeProjects/Phenotype/repos/jcode
git rev-parse origin/master
# expected: b9886143c13e683111aaf73d7f3ceab655860685

# Ancestry per PR
for branch in \
  fix/update-product-warnings \
  fix/repo-ci-pre-existing-drift \
  fix/tui-decollision-test-expectations \
  fix/cache-estimate-not-evidence \
  fix/setup-hints-orphan-spawn-notice \
  fix/setup-hints-pr128-test-regression \
  fix/sdk-test-paths-kcode-prefix \
  fix/tui-cache-expected-warm-unused-import \
  docs/orchestration-governance-docs-only \
  docs/orchestration-governance-proposal \
  fix/cli-exit-status-kcode-bin ; do
  ahead=$(git rev-list --count origin/master..origin/$branch)
  head_sha=$(git rev-parse origin/$branch)
  echo "$branch: $ahead ahead, $head_sha"
done

# Diff stats per PR
for n in 127 129 132 134 137 140 144 145 148 150 152 ; do
  echo "=== PR #$n ==="
  gh pr view $n --repo KooshaPari/KCode --json title,headRefName,isDraft,createdAt
  gh pr diff $n --repo KooshaPari/KCode --name-only
done

# Verify the wrong fix in #129
gh pr diff 129 --repo KooshaPari/KCode --patch | grep -A 4 "tests/cli_exit_status.rs"

# Verify the correct fix in #152
gh pr diff 152 --repo KooshaPari/KCode

# Verify #140 is a strict superset of #137
gh pr diff 137 --repo KooshaPari/KCode > /tmp/137.diff
gh pr diff 140 --repo KooshaPari/KCode > /tmp/140.diff
grep -E "^diff --git" /tmp/137.diff
grep -E "^diff --git" /tmp/140.diff

# Verify the textual conflict between #134 and #150
git diff origin/master origin/fix/cache-estimate-not-evidence -- crates/jcode-tui/src/tui/mod.rs
git diff origin/master origin/fix/tui-cache-expected-warm-unused-import -- crates/jcode-tui/src/tui/mod.rs
git diff origin/fix/cache-estimate-not-evidence origin/fix/tui-cache-expected-warm-unused-import -- crates/jcode-tui/src/tui/mod.rs

# Verify Cargo.toml [[bin]] target name
grep -E '^\[\[bin\]\]|^name' Cargo.toml | head

# Final verification gates (after step 12)
grep -rn "CARGO_BIN_EXE_jcode" tests/ src/ crates/   # expect 0
grep -rn "CARGO_BIN_EXE_kcode" tests/ src/ crates/   # expect 1
grep -c '#\[allow(dead_code)\]' crates/jcode-app-core/src/update_product.rs  # expect 7
grep -c 'fn with_spawn_notice' crates/jcode-setup-hints/src/lib.rs  # expect 0
grep -rn 'os.tmpdir(), `jcode-' sdk/typescript/test/   # expect 0
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo check --workspace --all-targets --all-features
```

---

## Stop condition met

- All 9 sections populated.
- Every claim traceable to a command actually run in this session.
- Stack state re-verified at 2026-10-19T23:09Z (after PR #152 appeared).
- Conflict verdict for #134 vs #150 derived from three explicit
  `git diff` invocations.
- Critical cli verdict derived from `Cargo.toml` `[[bin]]` name check
  and the textual diff of `tests/cli_exit_status.rs` across master, #132,
  #134, and #152.
- No source edits made. No commits made. No pushes made.