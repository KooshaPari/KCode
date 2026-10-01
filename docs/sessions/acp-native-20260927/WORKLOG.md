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

Passive-close correction: closing an attachment previously always sent Cancel
and attempted interaction cancellation. Now only an ACP-owned active prompt
triggers cancellation; closing a passive observation only disconnects its
pumps. Added synthetic daemon socket tests for same-session prompt/cancel,
unrelated-session silence, and passive-close EOF without a Cancel request.

Run 36582268979 / source468a9b6e31e6e3e3c2a3576a7fc5f0aa71ec507c:
Ubuntu production build passed; ACP27passed/1failed. Failure was the previous
standard initialization assertion requiring no _meta at all, now contradicted
by the explicitly advertised interaction-controller capability. Updated the
assertion to the exact sole capability (still excludes full-profile raw events).
New observer-pump and same-session prompt/cancel/passive-close tests passed.
Independent focused suites now continue after a peer test failure when the
production compile succeeded, preserving all results without hiding failure.

HarnessDesk contract refinement: explicit controller attachments now revalidate
lease before prompt/cancel/config changes or permission/form answers. Failed
renewal emits session/update with update.sessionUpdate=session_info_update and
update._meta["jcode.interactionController"]=false, clears pending response IDs,
and stops forwarding. Reacquisition requires explicit session reload; passive
generic ACP prompt semantics are unchanged. Broker renew/respond/cancel reject
expired/unowned leases; only explicit list/claim can acquire ownership. New
synthetic socket and broker expiry tests cover these negative cases.

Hosted gate passed on both Ubuntu/macOS: run36584965070, headbdcf18d1fba3a12b79dd75f40684234193d981c4.
Actual checkout25fe77d568b8deffa23f8d0228b6320bb119fc5c is the PR merge; its
treeb1df121c7d8286c3b10d83e7a44f56167b02f859 exactly equals the source-head tree.
Each platform: production minimal binary built; ACP31/0, routing4/0, broker8/0.
Ubuntu hooks11/0 (54total); macOS hooks10/0 (53total, platform-specific difference). Real isolated IPC, timeout/deny/cancel side-effect controls, typed
form response, competing/expired owner and concurrent-ID controls all passed.
See hosted-qualification.json for exact artifact IDs/digests. No full-feature,
whole-workspace, physical deployment or live-user-session claims. Final follow-up
only applies rustfmt ordering/wrapping to ACP glue and records these receipts.

Receipt correction: b9638caf6 initially recorded hooks11/0 for both platforms
before macOS log extraction was reviewed. Actual macOS is10/0; Ubuntu11/0.
This forward correction preserves history and the original raw hosted logs.

Contract correction (2026-09-29 follow-up): CLIENT-CONTRACT sections2.2/3 require
existing-session attachments without an explicit control grant to remain
read-only. The previous interaction-passive behavior still accepted prompt,
cancel and config changes; this was a real acceptance gap despite green tests.
Now absent ownership fails these mutations before daemon dispatch. New-session
interactive default and explicit load/resume acquisition remain unchanged.
Added passive mutation negative controls, extended lease-loss config rejection,
and changed the positive routing fixture to acquire through isolated broker IPC.
Prior artifacts do not qualify this corrected server read-only boundary.

Independent-review race reproduction: added two deterministic regressions before
changing production behavior. Re-registering an active session must retain the
original prompt transport/waiter; blocked stdout must keep prompt ownership
until its terminal RPC response is delivered. Current source aborts the original
pump and clears prompt state early plus again in the outer task. Hosted red
execution is required before the lifetime fix; no local heavy build.

Hosted red evidence: run36625615836 / macOS job109601693841 on7182ca385
executed32ACP tests successfully and reproduced exactly2failures:
reload_preserves_original_prompt_transport_and_waiter -> daemon disconnected;
prompt_ownership_survives_backpressured_terminal_response -> ownership released
before final response. Independent routing/broker/hooks cohorts remained green.

Fix: reload reuses a healthy registered DaemonSession and event pump, renewing
control without replacing the original prompt owner or callback forwarder. ACP
subscriptions alone opt into existing continue_on_disconnect handling; explicit
Cancel still routes through broker cancellation and daemon session control.
Prompt state cleanup occurs once, after final success/error delivery.

Added scripts/qualification/acp_actor.py (real isolated daemon, loopback provider,
harmless bootstrap turn for normal persistence) and acp_lifetime.py (original
question/permission reload, exact tool result, exactly-once approved marker,
cancellation and late-answer negatives, full ACP disconnect/reacquisition).
HarnessDesk independently executed actor fixture through its authenticated bridge
with original callbacks and side-effect oracles; these new lifetime fixes still
require their own hosted green result. No user sessions or installed channels
were touched; no heavy local build was run.

Corrected macOS lifetime gate passed: run36627996741/job109610349844, source
6afaaff2a, checkout507fea85d; both trees equal cdf93dbf5a84aec329061df45173ad2169f9de0f.
ACP34/0, routing4/0, broker8/0, hooks10/0 (56 total); real actor held
question/permission reload, original tool result, exactly-once approval marker,
cancel/late-answer negatives and full-disconnect original-question recovery all
passed. Artifact identity and limits are in lifetime-qualification.json. Ubuntu
job109610350230 remains queued at 2026-09-29T20:53Z; no Ubuntu green claim.
