# Candidate mature contract and oracle design — KCode

Date: 2026-09-29. Snapshot: SNAPSHOT.json. **DRAFT; product identity, architecture and existence decisions remain open.** No target requirement count.

## Mature horizon, without granting survival by default

Candidate horizon: a resource-conscious coding-agent runtime exposed through a terminal client and machine/harness interfaces, with durable sessions, replaceable workers, reliable provider/tool behavior and independently qualified outcomes. A reusable harness-client SDK is not automatically the general agent framework previously intended for Agentora. Reconcile that scope against HeliosLite, HeliosCLI and the preserved Shared capabilities before selecting an enduring host or merging anything.

The minimum realistic substitute starts with pinned upstream jcode plus its supported client/runtime interfaces and an independent acceptance/evidence adapter. A second challenger uses Codex App Server with a thin task adapter and durable effort/evidence outside the worker. OpenHands SDK and durable-execution libraries are candidates for framework-level needs, not compulsory extra layers. Measure actual unsupported capabilities and total integration/maintenance burden; no benchmark or framework winner is declared by this pass.

## Ontology: independently traversable views

Product/release identity; installed client/live daemon/protocol/configuration; actor journeys; session/turn/tool-call/provider state; ephemeral worker attempts and leases; durable effort/work packages; accepted artifacts/evidence; storage/migration/import; CLI/TUI/API/SDK/mobile projections; security/authorization; resource/performance profiles; release/operations; verification. Preserve authority and provenance on cross-view relations. Do not mistake workspace extraction for a functional boundary.

## Accepted mission invariants, applied once

`K-INV-IDENTITY`: qualification binds exact subject, contract/criterion, client AND serving runtime candidate, configuration/environment, verifier/version, run/time and raw evidence. `K-INV-LIFETIMES`: attempt replacement preserves durable work and accepted product truth, without conflating a resumed conversation with complete effort recovery. `K-INV-AUTHORITY`: authorized decisions, deterministic source facts, observations and inferences remain distinct. `K-INV-GRADER`: no empty/skipped/stale/wrong-scope evidence, collector failure or worker-weakened grader can grant acceptance. `K-INV-GROWTH`: stage expansion preserves the identity/state spine; rewrites accrue explicit transition debt.

These derive from the user's current mission. Product-specific requirements and quality thresholds are not silently inferred from them.

## Proposed journeys and stage projections

| ID | Actor to outcome | Earliest proposed projection | Required witness |
|---|---|---|---|
| K-J01 | Operator installs an identified fork and completes one bounded coding task through its actual serving daemon | CVP | Client/daemon identity, actual persistent edit, independent outcome check, typed failure |
| K-J02 | Client detaches/reconnects; operator resumes the intended session without switching effort, workspace or policy | MVP | Endpoint/protocol/storage identity; reconnect history; wrong-socket/old-daemon rejection |
| K-J03 | Worker/daemon fails around a tool side effect; replacement continues the durable effort safely | MVP/Beta | Effect identity, uncertainty reconciliation, restart persistence, no duplicate unauthorized effect |
| K-J04 | Headless/API/SDK consumer submits/cancels work and observes a truthful terminal state | MVP | Mounted handler, SDK client conformance, failure and collector controls, real consumer E2E |
| K-J05 | Operator safely updates/rolls back supported clients and daemons and migrates retained sessions | GA by supported profile | Channel and trust policy, version negotiation, migration/recovery, live candidate binding |
| K-J06 | Multiple supported users/agents/consumers run concurrently within explicit resource and authority boundaries | Mature | Workload-qualified resource data, isolation, no cross-session evidence, recovery and usability |

These are candidate stage projections, not invented successive product identities. CVP closes K-J01 rather than presenting an unmounted UI. API, mobile or general-framework breadth remains explicit unsupported scope until accepted and qualified, not a fake success stub.

## Quality overlays and transition debt

Quality targets require accepted profiles: latency and startup; memory with embeddings/cache/session count identified; throughput under provider limits; cancellation/recovery reliability; terminal accessibility; user-visible error actionability; tool credential and workspace isolation; update/signature provenance. No numerical target is fabricated. Transition debts include jcode/KCode naming and install redirection, live daemon/client version skew, extracted-crate API compatibility, storage migration, framework-versus-client-SDK ambiguity, and native Windows compatibility via the user-named Pine boundary.

## First vertical slice (proposal)

Use an isolated socket and storage root, an actual built client and daemon, a temporary repository, a deterministic local provider fixture, a real file mutation and a separately controlled grader. Record both executable digests and protocol. Run attempt A against effort E; crash/restart around persistence/effect receipt boundaries; reconnect using attempt B; prove the actual result and trace continuity. Repeat with the wrong daemon, swapped endpoint, stale contract, conflicting evidence, failed provider, denied tool, empty result, skipped criterion and worker-modified acceptance policy. Local fixtures do not qualify live providers or supported platforms.

## Oracle and evidence design

Evidence envelope: product, subject, accepted contract/baseline, criterion, source/dependency/build candidate, installed client, live daemon, protocol, socket/endpoint and data-root identity, configuration, environment, verifier/version/policy, run, timestamp, raw artifact/digest and provenance. Model pass/fail/not-run/skipped/collector-error/invalid-scope/stale/conflict explicitly. An authorized N/A is distinct from a skipped check. Critical dimensions are non-compensable.

Independent grader policy must be outside worker-writeable scope. A string claiming a verifier or a freshness flag is not proof: implement and test trusted identity, artifact verification, time policy, immutability and authorization before operational acceptance. Preserve raw history and localized feedback; never let an implementation PR weaken its own acceptance unreviewed.

Acceptance design covers positive, negative, wrong-scope, stale/missing/conflicting evidence, invalid input, dependencies, authorization, cancellation/timeouts, restart/recovery, regression and worker replacement. Mutation-style witnesses must fail when a guard is removed. The synthetic identity probe demonstrates selected predicate behavior only; it is not a production verifier.

Track separate functional, traceability, evidence, journey, regression, resource/performance, reliability, security, accessibility, usability, uncertainty and transition-debt dimensions. Report scope changes separately from engineering progress. No trend/asymptote without comparable repeated observations.

## Remaining architecture gates

Close source/history denominator; reconcile accepted product horizon and SDK custody; choose a realistic alternative by experiments; trace actual live client/daemon behavior; validate native macOS trust policy and Windows consumer behavior; resolve persistence semantics; qualify grader trust; complete bidirectional mapping and orphan checks; obtain fresh independent challenge. No 100% status or consolidation follows from this document.
