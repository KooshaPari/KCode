# External-effect recovery contract — KCode

Date: 2026-09-30. Status: DRAFT mature-contract obligation; implementation mapping evidence-backed.

## Current implementation facts
- `jcode-tool-core::ToolContext` carries `session_id`, `message_id`, and `tool_call_id` into every tool execution.
- persisted conversation content associates `ToolUse.id` with `ToolResult.tool_use_id`.
- `tool/inflight.rs` intentionally tracks executing tool-call IDs in a process-global RAII map. Its own comment documents a production duplicate-result wedge and states guards cannot leak past panic/cancellation/early return.
- that in-flight map is **not durable across process death**.
- reload recovery persists continuation intent and delivery state; it does not establish whether an arbitrary external tool side effect completed before a crash.

Therefore tool-call identity and session/reload continuity are insufficient evidence of external-effect outcome.

## Required adapter contract
Before a side-effecting operation crosses the runtime boundary, emit an `ExternalEffectIntent` containing durable effort reference, worker attempt, runtime/session/tool-call identity, operation class/target digest, authorization reference, and a stable effect ID.

Outcome states are: `intent_recorded`, `dispatched`, `confirmed_success`, `confirmed_failure`, `uncertain`, `reconciled_success`, `reconciled_not_applied`, and `manual_resolution_required`. A process disappearing after dispatch but before a trustworthy result creates `uncertain`, not failure and not success.

Replacement workers MUST reconcile an uncertain effect before retry. Downstream idempotency keys/provider request IDs should be used where supported. Where unsupported, the runtime must expose uncertainty instead of asserting exactly-once semantics.

## Runtime responsibility versus durable-effort authority
KCode owns runtime/tool-call identity and must expose enough pre/post execution hooks to correlate the effect. The durable receipt/state machine belongs to the external development-effort authority unless a thin adapter is experimentally inadequate. KCode session history is observation/context, not the canonical effect ledger.

## Falsifying experiment
Use a local deterministic side-effect service with durable counter + optional idempotency key. Crash attempt A: (1) before dispatch, (2) after service mutation before client acknowledgement, (3) after acknowledgement before terminal session persistence. Attempt B resumes the same external effort. Prove: case 1 retries once; case 2 reconciles before retry and never increments twice with idempotency; non-idempotent case 2 becomes explicit uncertainty/manual-policy path; case 3 recognizes confirmed effect. Wrong effort/effect IDs must be rejected.

Current state: contract defined; runtime adapter hooks not yet qualified end-to-end; native crash experiment NOT_RUN.