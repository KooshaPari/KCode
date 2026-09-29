# Upstream 0.88.0 Sync

## Goal

Bring the KCode fork up to speed with upstream `1jehuang/jcode` v0.88.0 using a
ledger-preserving **non-squash merge**, preserving every fork-only capability.

## Baseline facts

| Fact | Value |
|---|---|
| Fork branch at start | `feature/herdr-plugin-manifest` @ `115170054` |
| Fork version | `0.85.1-k1.1.1` |
| Upstream target | `v0.88.0` = `ee4cd3db3311ce2e95ef9b82e9f125d56516dad3` |
| Merge base | `752df77d3c13fa7a648eda257dbfcb8d3ea5974d` |
| Divergence | 2104 upstream commits vs 134 fork commits |
| Files changed | 439 fork-side, 489 upstream-side |
| Sync worktree | `/Users/kooshapari/CodeProjects/Phenotype/repos/jcode-upstream-sync` |
| Sync branch | `feature/upstream-0.88-sync` |

## Conflict resolution ledger

45 conflicts total. 20 were upstream-only and safe to take wholesale after confirming
the fork never touched them. 25 were changed on both sides and each needed archaeology.

### Resolved by the coordinator

| File | Resolution | Rationale |
|---|---|---|
| `jcode-tui-render/src/swarm_gallery.rs` | Keep fork's deletion | Upstream's only delta was a style refactor (`let used: usize;` to an expression). Fork's decomposed `swarm_gallery/strip.rs` already uses the expression form, so no behavior was lost. |
| `jcode-provider-openrouter/src/stream.rs` | Upstream + fork's EOF fix and 9 tests | See the dedicated section below. This was the highest-risk file and needed two rounds of correction. |
| `jcode-tui/src/tui/ui.rs` | Hybrid | Upstream's `notification_height` is a strict superset of the fork's `has_notification` check: it additionally wraps multi-line OpenAI reset hints. But the helper survives in the merged tree as `ui_input::notification_height` (not `input_ui::`), and the fork's `idle_donut_reserved_height` evolved to a 3-argument signature. Took upstream's notification wrapping with the corrected module path, and kept the fork's 3-arg donut call. |
| `src/cli/provider_init.rs` | Take upstream | Pure rename `OrcaRouter` to `Orcarouter` across 4 sites. |
| `.../commands_accounts_02/part_01.rs` | Keep fork | The fork's test-drift fix captures the canonical account label returned by `upsert_account` instead of hardcoding a literal. The hardcoded `"openai-otter"` that upstream reintroduced would re-break under a different label set. |
| `Cargo.toml` | Fork version scheme | `0.85.1-k1.1.1` becomes `0.88.0-k1.2.0`: BASEVERSION tracks upstream, first fork semver is minor. |
| `Cargo.lock` | Union | 4 hunks. `itertools` takes upstream's 0.13.0. `jcode` version tracks Cargo.toml. `jcode-harness-api` gains upstream's two new deps. The root `jcode` package dep list keeps **both** `jcode-herdr` (fork) and `jcode-harness-api` (upstream), since the merged workspace contains both crates. |
| `jcode-harness-api-server/src/translate.rs` | Take upstream, 10 hunks | 1 formatting-only. 9 semantic, all genuinely new upstream behavior: `detach_session` becomes a release barrier, plus new `side_panel_frame`, `text_message_id`/`finish_text`, and `tools` handling. No fork-only content was at stake. |

### Resolved by swarm workers

Batch A (scorpion), 12 files: `turn_execution.rs`, `agent_tests.rs`, `ambient/runner.rs`,
`ambient/runner_tests.rs`, `server/client_actions_tests.rs`, `server/client_lifecycle.rs`,
`base/auth/lifecycle.rs`, `base/model_usage.rs`, `base/session.rs`,
`base/session/persistence.rs`, `translate_tests.rs`, `command-risk/lib.rs`.

Batch B (zebra), 5 files: `tool/mod.rs`, `tool/multiedit.rs`, `setup-hints/lib.rs`,
`tui-mermaid/lib.rs`, `install.sh`.

Fork deltas on these files were small (1-19 added lines) versus upstream deltas of up to
1034 lines, so the correct resolution was upstream plus the fork's small additions.

## `stream.rs`: the one file that needed real porting

Both sides cherry-picked the same upstream stream fixes (#565, #609, #884, #1040) under
different SHAs, so the textual conflict was small. The substance was not.

**Attempt 1 failed.** Treating the conflict as "both sides append disjoint tests" and
concatenating them produced an unclosed delimiter. A re-read of the conflict showed three
logical regions, not two: the fork's side continued past its EOF tests into
`parse_next_event_coalesces_repeated_tool_call_id_chunks`, which upstream also has, so the
concatenation orphaned it. Structural extraction was the wrong tool.

**The real finding.** Upstream's `poll_next` still re-polls the inner stream after EOF.
Its `loop` calls `self.inner.as_mut().poll_next(cx)` again after emitting terminal events,
and a `Stream` only guarantees to return `None` once. The fork's `f08897519` fixes this with
an `inner_done` flag plus a `drain_after_eof` helper that owns all post-EOF emission
(UTF-8 tail flush, `#609` buffer force-close, tool-accumulator flush, terminal `MessageEnd`)
and is idempotent. Upstream has neither symbol. This is a genuine fork-only production fix,
not a test-only delta, so it had to be ported into the production `impl Stream` block
rather than appended to the test module.

Final shape: upstream's file as the base, plus the `inner_done` field, the
`drain_after_eof` helper, the `poll_next` guard, and all 9 fork-only tests. All 13 tests
from both sides are present and pass.

**Second defect: superseded test contract.** The fork's
`tool_call_deltas_coalesce_into_exactly_n_final_blocks_by_id` paired each positional
`ToolUseEnd` with the `ToolUseStart` two events back. Upstream's parallel-tool-call work
(#1326) replaced that with the keyed `ToolUseEndFor { id }` and added `ToolInputDeltaFor`.
The test's invariant is unchanged, so the assertion was updated to read the id off the
end event rather than inferring it positionally. This is test drift, not a behavior
regression: cite upstream #1326, not a fork commit.

## Fork-only capability verification

A first retention audit reported near-zero retention, but its per-file counters reset
inside a subshell, making the measurement invalid. A direct symbol check against the
resolved tree is authoritative and confirmed every fork-only feature survived:

| Feature | Evidence |
|---|---|
| Cache-vector reset on fork paths | `cache_vectors.reset()` present 4x in `turn_execution.rs` |
| Session fork-depth guard | `fork_depth` / `fork_max_depth` present 8x in `session.rs` |
| Auto-dream consolidation | `auto_dream` present 2x in `ambient/runner.rs` |
| Command-risk zsh/heredoc/pipeline modules | all 21 source files present, `mod heredoc` present |
| Pre-existing tests | zero test function names lost across all 4 test files |

## Validation

- Zero unmerged paths, zero conflict markers repo-wide.
- `cargo build --workspace --all-targets` with
  `SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.sdk` and
  `MACOSX_DEPLOYMENT_TARGET=15.0`.
- Serial test suite run and triaged.

## Post-resolution build defects found by the compiler

Textual conflict review is not a correctness gate. `cargo build --workspace --all-targets`
surfaced four stale-reference defects that the 25-conflict ledger did not cover.

### 1. `benches/startup.rs`: deleted `multiedit` tool

Upstream merged `multiedit` into `edit` and deleted the module, but the fork-only startup
bench still referenced it in two places. Both benchmark entries were removed. `patch`
remains and was left intact.

### 2. `ScheduledItem` recurrence test drift

Upstream commit `dbc9fb118` ("feat: /goal command, /loop recurrence, provider
defaults, PWSH+HERDR triage") added `recurrence: Recurrence` to `ScheduledItem`. The
fork's `runner_live_delivery_tests.rs` predates that field and failed to compile. Fixed
with `recurrence: Recurrence::Once`, matching the sibling precedent already present in
`runner_tests.rs`.

### 3. `Outbound::Reply` box patterns

Fork commit `2d8c7b00a` ("fix(clippy): resolve lints in openrouter-runtime and
harness-api-server") changed `Outbound::Reply(ServerFrame)` to
`Outbound::Reply(Box<ServerFrame>)` to shrink the enum's clippy footprint. That change
landed *after* the four upstream tests in `translate_tests.rs` were written and never
updated them. Box patterns are not stable Rust, so `matches!` on
`[Outbound::Reply(ServerFrame { .. })]` stopped compiling.

All four sites were rewritten to the bind-and-deref idiom already used elsewhere in the
same file (`[Outbound::Reply(frame)]` plus `frame.reply_to` / `frame.event`). This keeps
the assertions on `reply_to` and `ErrorCode` intact, so the tests still verify exactly
what they verified before.

### 4. `Cargo.lock` dependency blocks swapped

This is the most serious defect and it was introduced by the manual lockfile union, not
by upstream. The resolution grafted the `jcode-app-core` and `jcode-harness-api`
dependency lists onto the wrong adjacent package blocks, producing:

- `jcode-harness-api` depending on **itself** plus `jcode-herdr`
- `jcode-app-core` missing `jcode-harness-api` and `jcode-herdr`, and carrying a
  duplicated `jcode-session-types`

Cargo's own regeneration on the next build corrected both blocks. Verified with a TOML
parser (926 packages): the only remaining self-dependency is
`jcode-schema-dialect -> jcode-schema-dialect`, which is a legitimate
dev-dependency on its own `test-support` feature and predates this sync. Both blocks now
match their manifests exactly.

**Lesson:** never hand-union `Cargo.lock` package blocks. Let cargo regenerate it from
the resolved manifests, then diff the result to catch exactly this class of error.

## Test-gate triage

The first serial run was killed by a harness timeout during compilation, not by a test
hang. Cargo's cache was then warm, so the rerun reached the suites that had not yet been
executed and surfaced four failures. All four are resolved.

### `edit_schema_requires_file_path_old_string_new_string` (real, fixed)

The test is fork-only (`3f5e2df22`, "feat: add benchmarks, status bar, and tool tests").
Upstream `42285059b` ("tools: merge multiedit into atomic edit, add replace, nudge bash
edits") merged `multiedit` into `edit`, so the schema now requires only `file_path` at
the top level. `old_string` / `new_string` became optional single-edit shorthand and the
`edits` array became the primary form.

The test was rewritten as
`edit_schema_requires_file_path_and_supports_edits_and_shorthand`. It now asserts that
`file_path` is required, that `old_string` is *not* required, that the `edits` array
still requires `old_string` and `new_string` per item, and that the three shorthand
properties remain present. The replacement verifies the new contract rather than merely
deleting the two stale assertions.

### Three parallel-execution flakes (not real defects)

- `live_model_catalog_failure_keeps_static_and_selected_model_picker_fallbacks`
- `live_models_contract_supports_api_key_header_mode`
- `desktop_busy_owner_disconnect_with_successor_finishes_original_turn`
- `test_websocket_transport_matches_unix_socket_for_subscribe_history_message_and_resume`

The two live-catalog tests share one fake `/v1/models` server and race when run
concurrently. In isolation the whole `auth_login_flow` file passed 3/3 consecutive runs,
and the two e2e tests passed together under `--test-threads=1`. The full workspace suite
is already run serially, so no production change was made. Notably, the two catalog
failures *moved* between runs, which is itself evidence of a race rather than a
contract break.

### `test_queued_file_activity_repaint_does_not_leave_trailing_digit_artifact` (real, fixed)

Fork commit `0b0a97bd8` ("feat(tui): replace emojis with mature Unicode symbols") swept
`⚠/⚠️ → ▲` across 23 files. In this test it rewrote **both** the input strings and the
assertions, which made the two assertions mutually exclusive: the test required the
rendered output to both contain and not contain `▲ File activity:`. The second
assertion always failed.

The fork's production path was never migrated. `jcode-app-core/src/server.rs` and
`jcode-tui/src/tui/app/remote_notifications.rs` both still emit `⚠ File activity:`, and
`normalize_repaint_sensitive_notice_text` in `ui_input.rs:58` still implements the
`⚠️` → `⚠` strip that gives the width-stable glyph. Upstream `5a68156548` ("tui:
stabilize file activity warning glyphs") is the contract authority: it deliberately
feeds `⚠️` in and asserts `⚠` out.

The test was restored to that contract, which re-derives the anti-artifact property from
real production behavior rather than from a fork-local glyph convention.

### Test-run contamination: orphaned test processes (2026-09-27)

The apparent "deterministic `jcode-app-core` deadlock" (serial suite hanging at
`server::viewer_attach_reuses_live_owner_without_tracking_placeholder`, reproduced 3x)
was later shown to be contaminated by the execution environment:

1. **Orphaned test binaries.** The harness `bash` tool hard-caps commands at 600s and
   sends `SIGTERM` to the wrapper shell, but the test binary child sometimes survives and
   is reparented to PID 1. Two orphans were found still running 45-58 minutes after their
   parent runs were "killed" (`jcode_app_core-20376b93d6359a7d`, one with no filter =
   a full-suite copy, one with the H1 bisect filter). Multiple live copies of the same
   serial test suite share ports, sockets, telemetry slots and fixture paths, which
   produces exactly the observed signature: tokio runtime parked, 0% CPU, zero timer
   wakeups, hang point shifting between runs.
   - `pkill -f <pattern>` did **not** remove them (they survived `SIGTERM`); `kill -9`
     by PID did.
2. **Cargo package-cache lock contention.** Other agents on this box run
   `cargo install` / `cargo test` concurrently; `cargo install cargo-deny` held the
   shared `~/.cargo` package-cache lock for 14+ minutes, and a `timeout 900` budget was
   consumed almost entirely by "Blocking waiting for file lock on package cache".
   Fix: run tests with `--offline` (no registry phase → no package-cache lock) and a
   generous timeout in a `nohup` detached script.
3. **Machine load 400+** from ~6 concurrent cargo jobs (other agents) across the whole
   session, shifting which deadline-sensitive tests fail between runs.

**Validation method for the final gate:** pre-kill all
`jcode-upstream-sync/target/debug/deps/*` processes, run
`cargo test --offline ... -- --test-threads=1` from a `nohup` script with
`timeout 2400`, poll the log. The two tests that "hung" inside H1a/H1b both pass in
5.5s in isolation at low load. Whether any residual serial-suite hang is real is
judged only by this clean run (`.scratch-appcore4.log`).

## Serial-hang provenance verdict (2026-09-29): PRE-EXISTING, not merge-induced

The deterministic serial-suite hang at test #272
(`server::client_session::tests::concurrency::viewer_attach_reuses_live_owner_without_tracking_placeholder`)
was bisected to two contaminating tests, each sufficient alone to cause the target
test to deadlock (300s timeout, SIGKILL / exit 137):

- `agent::tests::agent_drop_removes_its_configured_session_tool_policy`
- `agent::tests::agent_clear_moves_tool_policy_registration_to_new_session`

Both contamination tests and the target test are session-tool-policy related
(global `SESSION_TOOL_POLICIES` static + `IsolatedConcurrencyEnv` JCODE_HOME swap).

### Provenance experiment (decisive)

Two detached worktrees built from clean checkouts and run with identical harness
(`cargo test --offline -p jcode-app-core --lib --no-run`, then `timeout -s KILL 300`
two-test pair, `--test-threads=1 --exact`):

| Tree | CTRL (target alone) | PAIR1 (drop + target) | PAIR2 (clear + target) | Verdict |
|---|---|---|---|---|
| Pre-merge fork HEAD `115170054` | EXIT=0 (1.41s) | **EXIT=137 (hang)** | **EXIT=137 (hang)** | hangs |
| Upstream v0.88.0 `ee4cd3db3` | EXIT=0 (30.65s) | **EXIT=137 (hang)** | **EXIT=137 (hang)** | hangs |
| Merged tree (index) | EXIT=0 (2.5s, prior) | EXIT=137 (prior bisection) | EXIT=137 (prior bisection) | hangs |

All three trees hang identically; target test alone passes on all three.

### Conclusion

The hang reproduces **on pure upstream v0.88.0 and on the pre-merge fork** — both
parents of this merge. It is a pre-existing upstream/fork test-isolation defect,
**not introduced by the merge** (the hang-path sources `client_session.rs`,
`concurrency.rs`, `tool/mod.rs` policy registry are byte-identical to upstream in
the merged index). **Not a merge blocker**: document as known issue, file upstream,
proceed with the merge. Fix is a separate post-merge work item (test isolation of
`SESSION_TOOL_POLICIES` / `JCODE_HOME` between `agent::tests` and `server` tests).

Provenance artifacts: worktrees `jcode-prov-premerge`, `jcode-prov-upstream`
(logs `.scratch-prov.log`), scripts `.scratch-prov.sh`.

## Full-suite deadlock provenance (2026-09-29): PRE-EXISTING on upstream alone

The 2-test serial-pair result was extended to the FULL suite. Gold-standard run:
full-parallel `jcode-app-core --lib` (default threads) on the **upstream parent alone**
(`ee4cd3db3`, detached worktree `jcode-prov-upstream`, binary `jcode_app_core-22917bb8f1ade6da`):

| Tree | Full-parallel result | Signature | Target test stuck? |
|---|---|---|---|
| Upstream v0.88.0 `ee4cd3db3` | **deadlock @ ok=283** (flat 2min, threads all sleeping) | `run_tests_console` parked + `reqwest-internal-sync-runtime` blocked in `reqwest::blocking::ClientHandle::new` | YES |
| Merged index | **deadlock @ ok=286** (flat, CPU 0:10.20 flat, threads all sleeping) | identical `reqwest::blocking::ClientHandle::new` park | YES |

The `reqwest::blocking::ClientHandle::new` call is reached transitively (no fork-added
reqwest call sites; the only merged-vs-upstream `update.rs` delta is a clippy nested-if
flatten, semantically identical). A test earlier in the suite pollutes global state such
that a later `reqwest::blocking` Client construction hangs in `spawn_unchecked`.

### Merge-gate verdict (final)

- **Build**: workspace `--all-targets` green (0 errors).
- **CI's actual `jcode-app-core --lib` gates**: `retention_readiness` (CI L219) and
  `tool::bash::tests::test_stdin_forwarding` (CI L318) both **EXIT=0**. CI never runs the
  full suite — only these filtered subsets plus `--no-run` compile checks.
- **Full suite** (serial AND parallel): deadlocks identically on upstream alone →
  **pre-existing upstream defect, not merge-induced, NOT a merge blocker.**
- Skipping the 2 minimal contaminators still deadlocks serially → more contaminators
  exist; the full suite cannot go green on any tree without an upstream test-isolation fix.

Recorded as known issue; separate post-merge work item. Merge proceeds on CI-gate evidence.
