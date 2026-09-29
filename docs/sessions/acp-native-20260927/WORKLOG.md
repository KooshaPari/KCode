# ACP validation worklog

2026-09-29: Refreshed HEAD/origin d94f49bfb8710a92223b90e908467097864e6f25.
Hosted run 36394070581 failed E0282/E0283 before tests. Retained explicit
HashSet<String> annotation fixes inference for interaction request IDs.
Validation: focused rustfmt and git diff --check; execution delegated to free
standard hosted runners. No live sessions touched; no local Cargo builds.

Ledger correction: e549ca5df6345d20d0788fdb84c0f160ed71d6f6 was validated
only with rustfmt and git diff --check. The git-commit wrapper inferred
tx-validated: cargo-check from Cargo.toml; no cargo check ran. History is
preserved; subsequent commits supply explicit truthful validation trailers.

Confirmed request-ID collision: two attached clients both begin at ID 2 and
client_lifecycle streamed Done/Error through session-wide fanout. The sender
now preserves FIFO while giving the original ID only to its originating
channel. Observers receive existing autonomous-turn ID 0, preserving lifecycle
and error detail without completing a different client request. Added synthetic
two-client equal-ID, failure, disconnected-origin and missing-member tests.
Hosted test workflow now includes this production routing module. Execution
pending; local module and formatting checks are source checks only.

Added a separate-process production broker IPC test with short canonical /tmp
paths and JCODE_SOCKET/JCODE_HOME overrides. It covers mode0600, exclusive
controller claims, held side-effect-before-approval oracle, allow/deny/cancel,
wrong-session/invalid-choice/duplicate rejection, timeout cleanup and typed
elicitation resolution. The process uses no real session or provider. Hosted
run 36581379368 targeted e52c169f5; later source additions require a new run.
