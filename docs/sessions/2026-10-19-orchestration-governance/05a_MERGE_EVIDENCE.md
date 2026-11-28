# Merge Integration Evidence - KCode PR Stack (2026-10-21T05:46Z)

Status: evidence companion for `05_MERGE_INTEGRATION_PLAN.md`
Session: `docs/sessions/2026-10-19-orchestration-governance/`
Repo: `KooshaPari/KCode` @ `2aea04dd871f21da5c5b21800d156a2c9b0830b1`

This file holds detailed evidence that does not fit inside the 500-line
cap of the merge plan. The plan itself (`05_MERGE_INTEGRATION_PLAN.md`)
points here. Cross-link from the plan; do not duplicate content.

---

## 1. Applied conflict resolutions (PR stack)

Each subsection below records the resolution that was applied for one
of the four PRs that the 2026-10-19 edition told the operator to
hand-resolve. All four have already been resolved by a parallel agent
and landed as 2-parent merge commits on the respective branches.
**No manual conflict resolution is needed at operator time.**

Authoritative re-verification:
`git merge-tree --write-tree origin/master origin/<branch>` reports a
clean merge (zero conflict markers) for every branch below. The merge
commits are real (each has two parents, second parent = `origin/master`).

### 1.1 PR #140 - `d6e4cfac2`

Branch: `fix/setup-hints-pr128-test-regression`
File: `crates/jcode-setup-hints/src/lib.rs`

Resolution: PR side wins. `with_spawn_notice` is DELETED entirely (both
the `#[allow(dead_code)]` attribute and the fn body go away). Master
had the fn with an `#[allow(dead_code)]` attribute; the branch removes
it because it is never called.

Verification: `with_spawn_notice` occurrences in the merged file: 0.

### 1.2 PR #127 - `b6508e696`

Branch: `fix/update-product-warnings`
File: `crates/jcode-app-core/src/update_product.rs`

Resolution: PR side wins on the comment only.
`#[allow(dead_code)] // Internal helper, called from update_repo_for;
flagged in non-test builds.` (Master had `// Called only from the gated
resolver surface above.`.) The `#[allow(dead_code)]` attributes
themselves are unchanged.

The PR-vs-master residual is now COMMENT-ONLY (1 line). Verification: 8
`allow(dead_code)` sites in the merged file; master already had 7 of
them, so the PR added the 8th comment change only.

### 1.3 PR #157 - `264c09a10`

Branch: `fix/tui-kcode-decollision`
File: `crates/jcode-tui/src/tui/app/helpers_tests.rs`

Resolution: MASTER wins on the accessor. Use
`program.file_stem().and_then(|name| name.to_str())` (NOT
`file_name()`). The Windows binary is `kcode.exe` and `file_name()`
would yield `kcode.exe` while the assertion expects `kcode`. The PR's
genuine `jcode` -> `kcode` expected-value change is retained.

`turn_notify.rs` merged cleanly and retains `String::from("kcode")` and
the `kcode-turn-session` kitty sequence.

Verification: `file_stem()` occurrences in the merged
`helpers_tests.rs`: 2; `file_name()` occurrences: 0.

### 1.4 PR #158 - `47b42e24d`

Branch: `fix/tui-cache-estimate-not-evidence`
File: `crates/jcode-tui/src/tui/kv_cache_problem.rs`

Resolution: PR side wins, but the residual is effectively COMMENT-ONLY.
Master already has the correct behavior `!info.is_cold &&
!info.is_estimate` AND already has the regression test
`cache_estimate_is_not_evidence_of_an_expected_warm_hit` at
`crates/jcode-tui/src/tui/mod.rs:1887`. The PR's only real content is
the explanatory comment about `is_estimate` being "a heuristic guess,
not evidence".

Verification: the merged file contains the `is_estimate` "heuristic
guess, not evidence" comment and references
`cache_estimate_is_not_evidence_of_an_expected_warm_hit`.

### 1.5 Re-verification commands

```bash
# #140: with_spawn_notice should be 0 in the merged file
git grep -c 'with_spawn_notice' origin/fix/setup-hints-pr128-test-regression -- crates/jcode-setup-hints/src/lib.rs
# expected: 0

# #127: 8 allow(dead_code) sites in the merged file
git grep -c 'allow(dead_code)' origin/fix/update-product-warnings -- crates/jcode-app-core/src/update_product.rs
# expected: 8

# #157: file_stem in helpers_tests.rs; file_name absent
git grep -c 'file_stem' origin/fix/tui-kcode-decollision -- crates/jcode-tui/src/tui/app/helpers_tests.rs
git grep -c 'file_name' origin/fix/tui-kcode-decollision -- crates/jcode-tui/src/tui/app/helpers_tests.rs
# expected: 2 / 0

# #158: the 'heuristic guess, not evidence' comment present
git grep -c 'heuristic guess, not evidence' origin/fix/tui-cache-estimate-not-evidence -- crates/jcode-tui/src/tui/kv_cache_problem.rs
# expected: >= 1
```

Each `git merge-tree --write-tree origin/master origin/<branch>` should
also exit cleanly with no conflict markers.

---

## 2. Runner wedge 2026-10-21 - root cause + remediation

Observed on the KCode repo's only self-hosted runner,
`desk-kcode-1` (labels `self-hosted,Linux,X64,desk-linux,desk-kcode`).
On 2026-10-21 the `Build & Test (desk-linux)` job of run `39530002307`
sat in the `Build` step for 1h38m with its `cargo build --release`
process at 0.0% CPU and no progress; its
`rustc --crate-name jcode_app_core` child sat at 0.5-0.6% CPU for
1h06m. Seven CI runs piled up in `queued`, the oldest for 3.5 hours.
The runner's 120-minute job-level cap cancelled the job at
05:41:26Z.

### 2.1 Root cause (verified directly on the desk compute node)

- The desk box's WSL2 instance is capped by `.wslconfig` at
  `memory=16GB` with `swapfile=D:\WSL\swap.vhdx` - **NOT the 64 GB
  previously believed.** The 64 GB figure is the Windows host's
  physical RAM; the WSL2 build environment has its own cap.
- Under concurrency the box hit 13.9 / 16 GB used with 0 free, 2.0 GB
  available, and **NO swap in use**; load average climbed past 46;
  processes fell into uninterruptible D-state (IO wait).
- A stray `cargo llvm-cov --workspace` coverage job (root-owned,
  orphaned by an earlier local session whose driver had exited but
  whose remote job kept running for 1h43m) was competing for the same
  16 GB and the same `CARGO_TARGET_DIR`.
- The `Build` step in `.github/workflows/ci.yml` has **NO per-step
  timeout wrapper** (only `Compile library and binary tests` and the
  later cohort steps use `run_with_timeout.py`), so a starved Build
  hangs until the job-level cap and serializes the whole queue behind
  it.

### 2.2 Remediation applied (2026-10-21)

- Terminated the orphaned coverage job on the desk node.
- Memory went from 13.9 / 16 GB used to 9.4 GB used (4.5 GB free,
  6.6 GB available).
- Load average fell from ~47 to ~14.
- The runner picked up a new job immediately afterward.

### 2.3 Standing risks

- **Single runner serializes all CI.** Until a second runner is
  provisioned, one stuck job blocks every other PR's CI.
- **`Build` step has no per-step timeout.** A starved Build hangs
  until the 120-minute job-level cap.
- **WSL2 memory cap (16 GB)** is being re-evaluated out-of-band. If
  it is raised, this section should be retired. Until then, treat
  the desk node as a 16 GB / 16 vCPU build host, not a 64 GB / 2 GPU
  build host, for CI expectations.

### 2.4 Recommendations (NOT done changes)

- Add a `run_with_timeout.py` wrapper around the `Build` step in
  `.github/workflows/ci.yml`, matching the sibling steps that already
  use it. This requires editing `.github/workflows/ci.yml` and is NOT
  in this stack.
- Provision at least one additional self-hosted runner for the
  KCode repo so a single wedge does not serialize the queue.
- Avoid running heavy local builds on the desk node concurrently
  with a CI build (they share `CARGO_TARGET_DIR` and 16 GB of WSL2
  memory).

### 2.5 Evidence commands

```bash
# Memory snapshot at wedge time (run on the desk node)
ssh desk 'wsl -e bash -lc "free -h && uptime"'

# Locate the orphan coverage job and its PID
ssh desk 'wsl -e bash -lc "ps -ef | grep -E \"cargo llvm-cov|rustc\" | head -20"'

# Inspect .wslconfig from the desk host
ssh desk 'cat %UserProfile%\\.wslconfig 2>/dev/null || true'
# expected key lines: memory=16GB, swapfile=D:\\WSL\\swap.vhdx

# Confirm the Build step in CI has no per-step timeout
grep -nE 'Build|run_with_timeout|timeout-minutes' .github/workflows/ci.yml | head -40
# expected: Build step lacks a run_with_timeout wrapper.
```
