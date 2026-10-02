# Product-integrated effect-recovery experiment — KCode

Date: 2026-09-30. Status: DESIGN_READY / IMPLEMENTATION_NOT_STARTED.

## Injection seam

Wrap the common `Registry::execute(name, input, ToolContext)` dispatch boundary or a single central caller immediately around it. Do not modify every Tool implementation.

`ToolContext` already carries session_id, message_id and tool_call_id. Preserve tool_call_id as correlation evidence, but allocate effect identity through a durable adapter because provider/model IDs do not imply downstream idempotency.

Add an optional effect adapter/context:
- durable_effort_ref;
- worker_attempt_id;
- before_effect/mark_dispatched/mark_outcome/query/reconcile.

Tool classification is centralized by resolved tool name. Read-only tools bypass the effect ledger. Unknown side-effect classification fails closed only for workflows that requested durable-effect guarantees; current ordinary sessions remain backward-compatible until the mature contract activates the policy.

## First tool

Use the existing WriteTool or a deterministic registered test tool through Registry. The existing tool tests already provide isolated working directories and ToolContext.

Crash boundaries:
A. persist effect INTENT, kill before Registry dispatch;
B. downstream file append/write commits, kill before ToolResult/session save;
C. effect receipt confirms, kill before terminal worker/session state.

On replacement, load the same durable effort/effect receipt. B must reconcile the file postcondition before any retry. For a non-queryable test tool, B remains UNCERTAIN and automatic retry is rejected.

## Interaction with session persistence

The existing ToolUse-before-execute / ToolResult-after-execute transcript ordering remains useful and should not be replaced. The effect ledger complements it:
- transcript says what the model/runtime attempted and observed;
- effect receipt says what is known about external commitment;
- product acceptance remains separate.

## Acceptance

Native test must prove one downstream effect, exact receipt transitions, worker-attempt replacement and session/result behavior. It must include a non-idempotent/non-queryable negative control. A passing standalone contract probe is not sufficient.

## Attempt-B reconciliation API — pinned
Do not make `Registry::execute` itself decide whether to retry an old uncertain effect. Replacement is a durable-effort decision before a fresh dispatch.

Minimum adapter extension:
- `load(effect_id) -> EffectRecord?`;
- `mark_uncertain(effect_id, reason)`;
- `reconcile(effect_id, ReconcileObservation) -> ReconcileDecision`.

`ReconcileDecision` is one of CONFIRMED_SUCCESS, CONFIRMED_FAILURE, RETRY_ALLOWED, STILL_UNCERTAIN. A runtime may dispatch only on RETRY_ALLOWED or a brand-new effect. STILL_UNCERTAIN fails closed.

For Write, the first reconciler binds target path plus expected content/hash captured in the intent. Attempt A writes then loses durable confirmation. Attempt B loads UNCERTAIN and hashes/reads the file: matching expected postcondition => RECONCILED_SUCCESS and no write; known absence/precondition => RETRY_ALLOWED; conflicting content => STILL_UNCERTAIN or explicit failure according to policy. The downstream effect count/postcondition must prove no duplicate write.

Do not use transcript ToolResult absence as RETRY_ALLOWED. Do not use tool_call_id alone as effect identity or downstream idempotency.