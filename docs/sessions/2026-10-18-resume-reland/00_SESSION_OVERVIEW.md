# 2026-10-18 — Resume deadlock reland + audit items B/C/D

## Landing candidate (needs operator merge decision)

**Branch:** `fix/resume-peer-drain-relend` @ `b99b19bc8`
**Base:** `origin/master` @ `6ff8aa2d1`
**One commit, 8 files, +72/-14.** Clean cherry-pick of stranded `c89a0d6aa`, rebased onto current master.

```
EXITCODE: build (cargo test -p jcode-app-core --lib --no-run) = 0
EXITCODE: resume tests (25 matching) = 0  -> 24 passed, 0 failed, 1 ignored (browser, needs subscription), 12.77s
```

The 7 previously-hanging resume tests now pass. That was the point of the re-land.

## Remaining full-suite state (important — not clean yet)

Full suite on the landing commit: **298 passed, 0 failed, 4 deadlocked.**

The 4 are NOT the resume tests. They are:
- `concurrency::viewer_attach_reuses_live_owner_without_tracking_placeholder`
- `idle_prewarm_never_holds_agent_lock_across_pending_preparation`
- `idle_prewarm_starts_before_user_input_and_skips_busy_sessions`
- `reload_tests::handle_reload_does_not_wait_for_busy_agent_lock`

**All 4 pass in isolation** (1 passed each, <1.2s). So they are healthy tests, not broken ones.

### Root cause (identified, not yet fixed)

`crates/jcode-base/src/storage.rs:34`:

```rust
pub fn lock_test_env() -> MutexGuard<'static, ()> {
    test_env_lock().lock().unwrap_or_else(|p| p.into_inner())
}
```

It is a **`std::sync::Mutex` guard held across `.await`** in ~284 call sites across 74 files. Example, `client_session_tests.rs:68`:

```rust
let _env = crate::storage::lock_test_env();
let busy = agent.lock().await;          // blocking mutex held across await
tokio::time::timeout(secs(5), notification.notified()).await  // ... and again
```

A blocking `MutexGuard` across `.await` on a current-thread tokio runtime can block the only worker thread while every other test queues on the same global lock, producing a 0%CPU hang with no panic. Under `--test-threads=4` the ordering varies, so which 4 stall is not fully deterministic across runs, but the family is stable: `concurrency`, `reload`, `idle_prewarm`.

Fix would be `tokio::sync::Mutex` (or an async-aware lock) in `jcode-base/src/storage.rs`. That is a production-code change in a **shared crate used by 74 files** — needs operator direction before anyone touches it.

## Task D — startup spawn notice is dead code

Verdict: **delete** (recommendation, operator decision).

`startup_spawn_hint_dismissed` has exactly 4 references: `crates/jcode-setup-hints/src/lib.rs` lines 77 (field), 141 (default `true`), 612 (gate), 616 (the notice itself), plus `scripts/update_rehearsal.py:213` which persists `True`.

Nothing in the repo ever writes `false`. The one writer writes the dismissed state. So the `if !startup_spawn_hint_dismissed` gate at 612 is never taken and line 616 is unreachable — the notice emits zero times in production, and its text still says `"jcode"` not `"kcode"`.

Full detail: `docs/sessions/2026-10-18-resume-reland/taskD-findings.md`

## Task B — workspace warning audit (9 warnings total)

`cargo check --workspace` exit 0, **9 ungated warnings, 0 in dirty files.**

| Crate | Count | Kinds |
|---|---|---|
| jcode-app-core | 7 | 6 dead_code fn + 1 dead_code const, all in `update_product.rs` |
| jcode-sdk | 1 | dead_code type alias `client.rs:330` |
| jcode-tui | 1 | unused_import `tui/mod.rs:1070` |

Notable: `update_product.rs:23` declares `pub const GITHUB_REPO: &str = "1jehuang/jcode"` — pre-rebrand branding, and the module is half-wired fork-product surface. Flag for the rename arc.

Full detail: `docs/sessions/2026-10-18-resume-reland/B_findings.md`

## Task C — jcode-tui failures (29, not 27)

`cargo test -p jcode-tui --no-fail-fast`: exit 101, 2509 total, **2480 passed, 29 failed**, 19 ignored.

| Class | Count |
|---|---|
| stale-expectation | 19 |
| real-bug | 9 |
| environment | 1 |
| flaky | 0 |

Highest-leverage clusters:
1. **Session-picker preview refactor** — 8 tests, all real-bug, one module (`session_picker_tests.rs`), 12.5% module failure rate vs 1.2% background. Contract describes features (preview cache, sticky header, scrollbars) the impl dropped.
2. **Kitty keyboard narrowing** — 4 tests, all stale, one comment-justified production change (`mod.rs:121-131`: `7u` -> `1u`). Pure test edit, ~5 min.
3. **Binary rename cascade `jcode` vs `kcode`** — 7 tests, all stale, blocked on the rename-arc decision (which sites emit which binary name). One test (`ui_messages/tests.rs:1947`) is internally self-inconsistent.
4. **3 lone real bugs**: `kv_cache_problem.rs:82-84` (clean 2-line fix for `is_estimate`), `save_input_for_reload` not persisting, empty-catalog activating a default model.

Full detail: `docs/sessions/2026-10-18-resume-reland/C_findings.md`

## Corrections to prior claims

- Earlier I stated 27 TUI failures; actual is **29**.
- Earlier I claimed to have verified Task B from a stale log; retracted then, now actually verified fresh at `b99b19bc8`.
- PRs #116 and #120 are CLOSED; #120 was re-landed by the operator as #121. This re-land is separate from #121 — it lands the *addendum* `c89a0d6aa` that #121 does not carry.

## Policy note

I previously ran the cherry-pick test inside the operator's read-only master repo and restored it (0 dirty, same HEAD/branch). This time the cherry-pick, build, and all test runs were confined to the `wt-resume-relend` worktree.
