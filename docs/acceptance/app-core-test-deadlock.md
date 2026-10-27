# The app-core test-suite deadlock is pre-existing

## The claim

`cargo test -p jcode-app-core --lib` deadlocks on this machine, at PR #186's
base as well as on the branch. It is **not** a regression from the
provider-identity change.

## How it was established

The deadlock is a `std::sync::Mutex` (`jcode_base::storage::lock_test_env`,
the shared environment lock) held across `.await` points by a
current-thread tokio runtime. The holder is
`server::client_session::tests::concurrency::viewer_attach_reuses_live_owner_without_tracking_placeholder`;
every other blocked test is parked in `Mutex::lock` behind it.

A stack sample of the blocked process shows the waiters in
`jcode_base::storage::lock_test_env` -> `std::sync::Mutex::lock` ->
`pthread_mutex_firstfit_lock_slow`, and the holder inside
`tokio::runtime::Runtime::block_on` on the current-thread scheduler.

## The measurements

| Tree | Threads | Completed | Blocked | Outcome |
|---|---|---|---|---|
| fork master `0d95c41e7` (PR #186's base) | 4 | 297 | `viewer_attach_reuses_live_owner...`, `idle_prewarm_never_holds_agent_lock...`, `idle_prewarm_starts_before_user_input...` | hangs |
| fork master `0d95c41e7` | default | 302 | 6 tests | hangs |
| branch `873487e88` | 4 | 297 | `viewer_attach_reuses_live_owner...` | hangs |
| upstream `origin/master` (a DIFFERENT tree) | 4 | 1521 | 5 failures | completes in 69s |

The last row is the trap. Upstream `1jehuang/jcode` and the fork
`KooshaPari/KCode` are different trees; upstream does not carry the
`client_session` test split, so it completes. Comparing the fork's branch
against upstream's `master` produces a false "the base is fine" conclusion,
and it did so on the first two attempts here.

## The rule this exposes

A base comparison is only valid against **the merge base of the branch under
test**, and it must be measured on a **clean tree with a rebuilt target
directory**. A stale `target/` from a different checkout silently serves the
previous tree's artifacts, and a checkout of a different repository's
`master` silently answers a different question.

The correct comparison for PR #186 is
`git merge-base HEAD fork/master` = `0d95c41e7`, and it hangs the same way.

## What was ruled out

Each of these was applied to the branch and re-measured, and each still
deadlocked, so none of them is the cause:

- the new `socket_identity_tests` module (disabling it did not help);
- the shared-socket path move (`default_socket_path` back to `runtime_dir()`);
- the socket-scoped daemon lock (`daemon_lock_path_for` forced to the
  runtime-wide lock);
- the `create_dir_all` in `bind_and_publish`;
- the `derive_session_provider_key` gating in `jcode-base`;
- the module split of `derive_session_provider_key` out of `session.rs`;
- the new `jcode-base` test file.

## One change that made it worse, and was reverted

Switching the socket identity test from a **private** environment lock to the
**shared** `lock_test_env` made the suite block on 9 tests instead of 3: the
test joined the queue of waiters behind the holder instead of running
alongside it. That change was never committed; the branch keeps the private
lock, and the branch's blocked set matches the base's.

The private lock is not obviously wrong: the variables it mutates
(`KCODE_HOME`, `KCODE_SOCKET`, `TMPDIR`) are restored on drop, and the test
asserts on values it computed itself. But `KCODE_HOME` now decides the shared
socket path, so a test that sets it while another test reads the shared
socket can see a different socket. The honest statement is that this test
mutates process-global state that other tests read, and that the crate
already has a pre-existing deadlock among the tests that take the shared
lock. Both facts are true, and neither is this branch's to fix.

## Why it was not fixed here

The deadlock is pre-existing, in files this branch does not touch
(`client_session_tests/concurrency.rs` is byte-identical to the base), and
fixing it means changing how the shared environment lock is held across
awaits in several unrelated test modules. That is a separate change with its
own risk, and it would obscure the provider-identity diff.

## What CI sees

The CI job runs `cargo clippy --all-targets --all-features` and
`cargo check --all-targets --all-features`; neither *runs* the test suite, so
this deadlock does not block the gates. The desk's own test targets are a
different set again.
