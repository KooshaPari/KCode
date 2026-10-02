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
**SKIPPED**.

=> Only the **Format** job ever executed project code. Its step #4
`Check module declarations resolve` **PASSED** (no `mod x;` resolution broke), and
its step #5 `cargo fmt --all -- --check` failed on the pre-existing dirt documented
in `00_SESSION_OVERVIEW.md`.

## B) The 5 ssh-agent failures are structural, not content-dependent

`gh secret list --repo KooshaPari/KCode` returns **EMPTY** (zero secrets
configured), while `.github/workflows/ci.yml:29-32` runs `webfactory/ssh-agent`
with `ssh-private-key: ${{ secrets.DEPLOY_KEY }}` unguarded as an early step.
Therefore **every** branch that runs `ci.yml` fails those 5 jobs regardless of
branch content.

## C) Built binary proven post-merge (the `--version` stamp is misleading but cosmetic)

- `./target/debug/jcode --version` -> `v0.0.0-dev (115170054, dirty)`
- `jcode version --json` -> git_hash `115170054`, git_tag
  `v0.85.1-k1.1.0-59-g115170054`
- Actual HEAD is `ba7c2e14b`.

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
| `#[test]` / `#[tokio::test]` attributes workspace-wide | **8,964** |
| Tests in `tests/e2e` (13 files) | **69** |
| E2E re-verify executed | **6 of 69 (8.7%)** |

A broader sweep (`-p jcode-app-core --lib`, `--workspace --lib`,
`--workspace --tests`) was launched to widen this. Status at time of writing:
**sweep still running** — `.scratch-broader-tests.log` contains no
`test result:` lines and no `=== DONE` marker yet.
