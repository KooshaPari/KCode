# Acceptance evidence (2026-10-02)

Companion to `00_SESSION_OVERVIEW.md`. Records the step-level CI proof, the built
binary's provenance, and honest coverage denominators for the upstream 0.88 merge.
All findings were verified by the orchestrator before transcription.

## A) CI never compiled this merge — step-level proof

Run `36925706533`, **Build & Test ubuntu** (job `110582409958`):

| Step | Name | Outcome |
|---|---|---|
| #3 | Configure SSH for cargo git dependencies | **failure** |
| #4 | rust-toolchain | SKIPPED |
| #5 | rust-cache | SKIPPED |
| #6 | mold | SKIPPED |
| #7 | Build | SKIPPED |
| #8 | Compile library and binary tests | SKIPPED |

Same shape on **Quality Guardrails** (job `110582409688`): step #3 `ssh-agent`
failed, so steps #6 `module-check`, #7 `cargo fmt`, and #8 check-all-targets were
**SKIPPED**. (Step indices here match the GitHub Actions log UI; the
`gh api .../jobs/{id}/logs` stream prepends a `Set up job` line, so any
REST cross-check that lists the steps as a JSON array is off by one for
indices after that header — the log-UI numbering is the canonical one
this doc uses throughout.)

=> Only the **Format** job ever executed project code. Its step #4
`Check module declarations resolve` **PASSED** (no `mod x;` resolution broke), and
its step #5 `cargo fmt --all -- --check` failed on the pre-existing dirt documented
in `00_SESSION_OVERVIEW.md`.

## B) The 5 ssh-agent failures were structural (now guarded)

`gh secret list --repo KooshaPari/KCode` returns **EMPTY** (zero secrets
configured), and `webfactory/ssh-agent` runs `ssh-private-key: ${{ secrets.DEPLOY_KEY }}`.
At the time this section was first written the step was unguarded, so **every** branch that
ran `ci.yml` failed those 5 jobs regardless of branch content. That condition no longer holds
in HEAD: commit `3c96175f6` added the job-level guard
`if: env.DEPLOY_KEY_PRESENT == 'true'` (`.github/workflows/ci.yml:38`, `:172`, `:441`, `:703`;
env declared at `:24`, `:148`, `:427`, `:689`), so on this PR the 5 jobs now SKIP Configure
SSH and proceed to compile. The "actual HEAD is `ba7c2e14b`" line in the original draft was
already stale when written (3c96175f6 was in HEAD at that point); current HEAD is
`e6135802a` (`git rev-parse HEAD`, verified).

## C) Built binary proven post-merge (the `--version` stamp is misleading but cosmetic)

- `./target/debug/jcode --version` -> `v0.0.0-dev (115170054, dirty)`
- `jcode version --json` -> git_hash `115170054`, git_tag
  `v0.85.1-k1.1.0-59-g115170054`
- Actual HEAD at this writing is `e6135802a` (`git rev-parse HEAD`, verified); the binary's
  embedded stamp `115170054` is the pre-merge tree and lags by design (see below).

**Root cause is INTENTIONAL** and documented at
`crates/jcode-build-meta/build.rs:145-165`: the build script deliberately does
NOT declare `.git/HEAD` as `rerun-if-changed` (to avoid full-tree recompiles), and
states that for ordinary dev builds the embedded git hash "may lag the very latest
commit … that is a cosmetic `--version` detail". Release builds force a rerun via
`JCODE_RELEASE_BUILD` / `JCODE_BUILD_SEMVER`.

**Code currency was proven independently.** Identifiers `apply_definitions`
(31 hits), `action_failed` (11), `action_still_valid` (6) are present in the
binary's symbol table AND verified absent from the whole pre-merge tree
`115170054` via `git grep`. Mangled paths resolve to
`jcode_app_core::tool::sdk::apply_definitions` and
`jcode_app_core::tool::browser::browser_fast::action_failed`, from
`crates/jcode-app-core/src/tool/sdk.rs` and `tool/browser_fast.rs` — files added
entirely by the merge (97 files added).

**Real public interface exercised:** `jcode --version`, `jcode --help`,
`jcode version --json` all **EXIT=0**.

## D) Honest coverage denominators

| Denominator | Value |
|---|---|
| `#[test]` / `#[tokio::test]` attributes workspace-wide | **8,964** (measured at head `d76fb0882`; producing command was not recorded — TODO: record the exact command in a follow-up issue) |
| Tests in `tests/e2e` (13 files) | **69** |
| E2E re-verify executed | **6 of 69 (8.7%)** |

A broader sweep (`-p jcode-app-core --lib`, `--workspace --lib`,
`--workspace --tests`) was launched to widen this. Status at time of writing:
**sweep still running** — `.scratch-broader-tests.log` contains no
`test result:` lines and no `=== DONE` marker yet.

## Requirement-to-check traceability (observed)

Every row pairs one requirement with the concrete check that was run and the
result actually observed at that moment.

| Requirement | Check performed | Observed result | Verdict |
|---|---|---|---|
| CI step-level evidence for the merge | `gh api` Actions run jobs+steps at head `d76fb0882` (subagent read) | 10 pass / 6 fail; 0 unexpected checks; Require Linked Issue = pass | closed |
| Failure set matches known baseline | compare observed failing steps vs known set | exact match, no new classes at that head | closed |
| Baseline provenance: setup-hints failure | identical test run on pristine upstream worktree `ee4cd3db3` (second parent of merge `41fae89f3`) | 147 passed / 1 failed, same test name, same panic at `setup_hints_tests.rs:39:50` | UPSTREAM_INHERITED |
| Baseline provenance: jcode-base failure set | same target on pristine upstream vs merged tree; serial 2x re-runs of merge-only names | 10 common, 3 merge-only, 2 upstream-only; 2 of 3 merge-only also fail upstream | 1 real mismatch isolated |
| Real merge-introduced failure | root-cause of `provider_catalog_tests::auth_issue_profile_metadata_matches_direct_provider_endpoints`; fork commit `dbc9fb118` set catalog default to glm-5.3 while the stale test still asserted glm-4.5 (assertion byte-identical in fork parent `115170054`) | FORK_PREEXISTING, fixed in `cb82724f3`; targeted re-run 1 passed/0 failed; full serial run 1551 passed/8 failed with fix absent from failures | FORK_PREEXISTING |
| Empty-DEPLOY_KEY ssh-agent structural defect | `gh secret list` (empty) + `ci.yml` passing `secrets.DEPLOY_KEY` unconditionally + web-checked that secrets cannot be used in `if:` | all 5 jobs failed at step Configure SSH with build steps SKIPPED | root cause confirmed; fixed in `3c96175f6` by job-level env `DEPLOY_KEY_PRESENT` gate; observed after push: all 5 jobs SKIP Configure SSH and compile |
| Windows-only E0433 exposed by that fix | failure at `client_actions.rs:39` under `#[cfg(windows)]`; crate root has no `mod shell` (re-export is at `crate::terminal::shell`) | fork-introduced, never compiled by either parent | fixed in `1eb94968a` to `jcode_shell_integration::Shell` (dependency already present, re-exports `detect::Shell`); local cross-check blocked by ring/aws-lc-sys needing cargo-xwin, CI cargo-xwin step is the acceptance check |
| EOF cohort arg-order defect | ran old arg order locally | exit 1, "test_filter_error: unexpected argument 'eof_tail_flush_does_not_repoll_a_non_fused_inner_stream' found" | reproduced CI exactly; fixed in `1eb94968a` (filters after `--`); new order exit 0, "2 passed; 0 failed" |
| Product path exercised | run built binary: `--version`, `--help`, `version --json` | all exit 0 | closed |
| Product path currency | `git grep` merge-only symbols in binary output and tree; same grep on pre-merge tree `115170054` | present at HEAD, absent pre-merge | fork's code rides along |
| Coverage widened | run test targets | 1551 passed serial in jcode-base, 147 in setup-hints; workspace lists 8,964 test attributes (producing command not recorded — see §D TODO) | recorded |
| Coverage denominator re-derivation | `grep -rF '#[test]' --include='*.rs' . \| grep -v '/target/' \| wc -l` plus the `#[tokio::test]` equivalent, at HEAD `e6135802a` | 8020 + 950 = **8,970** at current HEAD (6 higher than the 8,964 recorded at `d76fb0882`, consistent with test/docs commits added since); original 8,964 command remains unrecorded | recorded |
| Known accepted failures | classification of remaining 8 serial + 1 hints failure | all also fail on pristine upstream `ee4cd3db3` | not merge-caused |

**Open loop**
- CI result for head `1eb94968a` (fixes rows 7 and 8) is in flight; row 7's CI
  acceptance and the final pass/fail bucket get appended once observed.
