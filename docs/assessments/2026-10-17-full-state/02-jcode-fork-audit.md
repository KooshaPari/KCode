# Jcode / KCode Fork State Audit

**Evidence snapshot:** October 16, 2026, approximately 23:40 PDT  
**Artifact written:** October 17, 2026  
**Scope:** KooshaPari/KCode and the installed Jcode runtime, plus the isolated native wake-receipt candidate. This is a bounded recovery from the independent auditor's recorded findings, not a fresh repository audit. No source changes, builds, tests, runtime actions, or hosted checks were performed for this report.

## Executive assessment

KCode is an actively maintained Jcode fork with a current remote tip at `0.93.0-k0.1.0` and a merged isolation/session-import change. The installed Mac runtime is a separate upstream Jcode `0.98.1` build, older in lineage context than the fork candidate and using its own `~/.jcode` profile and socket. The two must not be treated as one delivered product state.

The wake-receipt work is a focused, isolated candidate with useful local checks and seven focused tests reported passing. It is not an accepted or promoted runtime capability: its draft PR targets a dedicated exact-upstream snapshot base, and the recorded checks do not establish full workspace, hosted CI, native bridge, release, or user acceptance. Two source-level concurrency/recovery weaknesses were independently identified but not reproduced by execution. A separate power-loss durability concern is a fault-model observation, not a demonstrated failure.

The fork has meaningful usable functionality, but the available evidence does not support a numerical completion percentage or a claim that the fork is production-complete. “CVP/MVP” below is treated as an operationally useful candidate threshold, not as a claim of release readiness. The precise expansion of CVP was not established in the supplied evidence.

## Identity and state boundaries

| Surface | Verified identity/state | Boundary |
|---|---|---|
| Shared KCode/Jcode working tree | `/Users/kooshapari/CodeProjects/Phenotype/repos/jcode`, HEAD `748eeaa1e`; origin `KooshaPari/KCode`; Cargo version `0.85.1-k1.1.1` | Preserve the working tree. It is not the installed binary's exact source snapshot. |
| KCode remote | `origin/master` at `6cf71ed`; `0.93.0-k0.1.0`; PR #98 merged October 16 at 22:40 PDT | Includes `KCODE_*` configuration and `~/.kcode`/socket isolation, read-only Jcode session import, and intentionally disabled unconditional self-update. Remote state does not prove installed-binary behavior. |
| Installed Mac Jcode | Wrapper `/Users/kooshapari/.local/bin/jcode` -> `jcode.real` -> `~/.jcode/builds/current/jcode` -> versioned `0.98.1/jcode`; full upstream commit `1c7426eb1925a1167eb56011fa479ad707cc28b6`; build date October 15 | Live daemon PID 12682 maps to this release and uses `~/.jcode` plus `/var/folders/wl/z8733y815nzg28yy0_1_lh7w0000gn/T/jcode.sock`. Not KCode. |
| Historical shared-server symlink | Points at historical `unfold` build | Not the live daemon executable; do not use it to identify current runtime. |
| Upstream source | `https://github.com/1jehuang/jcode.git` | Read-only authority for this audit. No upstream writes. |
| Native wake-receipt candidate | Clean worktree `/Users/kooshapari/CodeProjects/Phenotype/repos/jcode-native-wake-receipts`, commit `87bd48b29664e51a68217ac351ff794b375c77f3`, branch `codex/native-wake-receipts-20261016` | Isolated candidate; no shared-profile promotion or live-session change. |
| Candidate draft PR | [KooshaPari/KCode PR #107](https://github.com/KooshaPari/KCode/pull/107) | Targets `codex/native-wake-base-1c7426e`, an exact `1c7426e` snapshot, not fork `master` or upstream. The candidate base and fork origin had 338 versus 2,808 divergent commits in the earlier audit. Treat as snapshot-context work, not reconciled release work. |

Both candidate branch and base were reported pushed with ledger trailers. No commit, push, or PR action was taken while recovering this artifact.

## Fork intent and goals

### Confirmed intent

The fork is intended to have its own identity, configuration namespace, state directory, and socket; it can import Jcode session data read-only. Its unconditional self-update path is deliberately disabled. The version scheme records upstream base version and KooshaPari fork version (`BASEVERSION-kSEMVER`). A remaining product goal is to turn fork code into an owned, qualified artifact and release rather than treating upstream build state as fork delivery.

### Evidence-supported work areas

- Maintain the Jcode-derived CLI/application while separating KCode-owned settings and runtime state.
- Preserve usable read-only access to existing Jcode sessions while preventing KCode from silently taking over the Jcode profile.
- Keep fork release/update behavior under KCode control; unconditional self-update is intentionally off.
- Close honest feature gaps and replace misleading success-shaped stubs with implemented behavior or explicit unsupported results.
- Qualify and publish an owned fork artifact through local gates and an appropriate release path.
- The native wake-receipt candidate explores durable, opt-in notification and wake acknowledgement for a specific session. It is an experimental candidate, not established fork-wide intent or a live capability.

### Inference and unknowns

The broader intended product positioning, named customer/user, and complete fork acceptance contract were not established by these findings. “CVP/MVP” has no confirmed expansion here. There is no fresh operator usability feedback in the evidence. Do not infer demand, satisfaction, or readiness from merges, tests, traffic, or a draft PR.

## Usable state versus completion

| Area | Evidence-backed usable state | Remaining qualification |
|---|---|---|
| KCode configuration/runtime isolation | Remote KCode `0.93.0-k0.1.0` includes `KCODE_*`, `~/.kcode` and socket isolation, plus read-only Jcode session import (PR #98 merged). | Installed Mac process is upstream Jcode with `~/.jcode`; no evidence here that an installed KCode build was installed and exercised end-to-end. Verify each supported launch path and prove no cross-profile writes before calling it operationally qualified. |
| Update behavior | Unconditional self-update is intentionally disabled on remote KCode tip. | Owned update/distribution lifecycle and a qualified fork release remain incomplete. Do not conflate disabled upstream updater defects with a complete KCode updater. |
| Native wake receipt | Candidate adds receipt-oriented APIs/events and a durable queued store; reported isolated checks and focused tests passed. | Not merged to fork master, not promoted, and not live. Concurrency, Windows backup recovery, transcript durability, native bridge, and end-to-end acceptance remain open. |
| Feature behavior | Several core features exist in the inherited application, but this evidence packet does not inventory or independently revalidate all of them. | No whole-product acceptance or feature-coverage denominator was supplied. |
| Build/release | Local candidate checks are reported; fork remote has a versioned tip. | Fork-owned artifact/release qualification is incomplete. The recorded release workflow is gated to upstream only. |

## Stub, mock, and misleading-gap register

The following are reported as authentic unfinished production paths in the current fork context. PR state was last checked at approximately 23:40 PDT October 16; these should be rechecked before planning implementation because hosted state can change.

| Gap | Current observed behavior | User impact / acceptance |
|---|---|---|
| Swarm rename | Reports a synthetic success such as “Rename sent” without performing the rename. Open PR #105 was reported as addressing it, not merged at snapshot. | A success message can mislead users and agents. Verify state actually changes, or return an explicit unsupported/error result. |
| Auto-dream | No LLM execution and no opened gate. Open PR #99 was reported as addressing it, not merged at snapshot. | The apparent workflow does not perform the promised model-backed action. Require a real run and an auditable gate/decision, or clearly disable the path. |
| Windows stdin | Returns `Unknown`. | Windows interactive use is incomplete for this input path. Define and implement supported behavior, with platform-specific acceptance. |
| Windows build/code-size gates | Fork documentation acknowledges complete Windows build support and code-size gate are not green. | Windows support and repository policy are not fully qualified. Exact remaining work and current status were not freshly checked. |
| Release workflow | Recorded workflow is upstream-only. | A fork source change does not by itself yield an owned KCode artifact/release. Add and qualify an authorized fork release path. |
| Wake receipt dispatch | Queue/receipt candidate has concurrency and recovery gaps described below. | It cannot yet claim exactly-once dispatch or robust cross-platform recovery. |

No extra mock/stub inventory was performed for this artifact. The register is not exhaustive.

## Native wake-receipt candidate: design and evidence

The existing `notify_session` NDJSON request is `{id, session_id, message}`. Its `Done` response conflates “ran,” “notified,” and “queued”; the live queue is memory-backed with disk fallback and no receipt deduplication.

The isolated candidate adds `notify_session_receipt` and `get_wake_receipt`, a `wake_receipt` event, an exact opt-in `JCODE_NATIVE_WAKE_SESSION_ALLOWLIST`, persisted per-session gating, and durable queued storage. It checks ID/message/session conflicts and records that a target session awakened after saving the message in that target's transcript. Busy or unattached targets require retry. “Awakened” does not mean provider completion.

Reported candidate validation: core `cargo check`, downstream `cargo check -p jcode -p jcode-tui -j1`, formatting/diff checks, and seven focused receipt tests (six app-core, one protocol) passed in an isolated synthetic `JCODE_HOME`. These are author-reported results from the earlier audit. They do not establish full-workspace test status, coverage, hosted CI, native-bridge behavior, release acceptance, or real-user outcomes.

### Correctness risks and proposed negative cases

1. **Concurrent delayed retry can dispatch twice (source-level finding; reproduction not run).** `submit` returns a stale queued snapshot before awaiting; dispatch does not recheck receipt state after agent reservation. A retry can begin while the first dispatch is in flight, wait for the reservation, then enter after the first turn finishes and dispatch the same receipt again. Add a deterministic interleaving test: pause the first dispatch after reservation/await, submit the same receipt concurrently, complete the first turn, resume the second, and assert only one dispatch and a stable terminal receipt. The current source finding is not an executed reproduction.
2. **Windows backup recovery can lose receipt history (source-level finding; fault injection not run).** Non-Unix storage replacement moves the primary file to `.bak` before final rename. Receipt loading checks only whether the primary exists; if absent, it treats the store as empty and does not recover `.bak`. Exercise failure/crash between backup move and final rename on Windows, then require recovery of the prior awakened history without replay.
3. **Transcript/receipt process-crash gap (documented candidate limitation).** A process crash between transcript save and receipt-state save can replay. The design is at-least-once, not exactly-once. Add crash-point tests around both persistence steps and document the duplicate-delivery contract. Do not claim exactly-once.
4. **Transcript power-loss durability (fault-model concern; not reproduced).** Transcript writes use `write_json_fast`/`append_json_line_fast` without `fsync`, while receipt-file writes do `fsync`. A power loss could preserve an “Awakened” receipt while losing the transcript write. This is a separate power-loss concern, not the process-crash gap above. Establish the required durability contract, then test or change ordering/sync behavior accordingly.
5. **Busy/unattached retry and allowlist negatives.** Verify opt-in denies non-allowlisted sessions, busy/unattached sessions remain retryable without duplicate messages, and ID reuse with changed session or message is rejected. The supplied evidence does not say these negative cases were all executed.
6. **Awakened is not completed.** Verify callers cannot interpret the wake acknowledgement as provider completion; preserve a distinct state/result if completion tracking is required.

## Acceptance gates and dependency order

Keep these acceptance layers separate. A pass at one layer does not prove the next.

```text
source correctness
      |
      v
focused/local checks -----> full repository gates
      |                           |
      +---------------------------v
                         hosted/branch qualification
                                   |
                                   v
                         native runtime/platform checks
                                   |
                                   v
                         owned artifact and release
                                   |
                                   v
                         operator/user acceptance
```

| Gate | Evidence required | Current state from supplied findings |
|---|---|---|
| Source correctness | Resolve the receipt interleaving and Windows recovery issues; specify crash/power-loss contract; turn fake success paths into real behavior or explicit unsupported results. | Open/incomplete. Some issues are source-level findings only; no reproduction was run for this report. |
| Focused local validation | Deterministic regression/negative tests for changed behavior and platform-specific paths. | Candidate reports seven focused receipt tests passing; proposed negative cases above remain unverified unless separately recorded. |
| Full local repository gates | Required project build/test/lint/code-size gates for the exact candidate/release source and target platforms. | Not established by the focused candidate checks. Fork docs report Windows build/code-size gates not green. |
| Hosted/branch qualification | Correct target/base, required checks, review, merged state, and fork-specific workflow evidence. | PR #107 is draft and targets an exact snapshot base. PRs #105/#99 were open/unmerged at snapshot. No hosted CI result asserted. |
| Live runtime/platform | Install the exact KCode artifact in isolated `~/.kcode`, exercise launch/session import/socket/update boundaries, and qualify Windows paths. | Not established. The observed live daemon is upstream Jcode under `~/.jcode`. No candidate promotion occurred. |
| Owned artifact/release | Fork-owned build artifact, reproducible version/provenance, update and rollback policy, and release workflow. | Incomplete; recorded workflow is upstream-only. |
| User acceptance | Fresh operator/user workflow evidence against explicit criteria. | Unknown; no fresh usability feedback supplied. |

## CVP/MVP threshold and full-fork completion

Because “CVP” and the product-specific MVP contract were not defined in the supplied evidence, these are proposed operational thresholds for planning, not claims of previously approved requirements.

### Proposed useful-candidate threshold

A KCode candidate is useful for controlled evaluation when it builds from an identified fork commit, runs in an isolated KCode profile/socket, can import Jcode sessions read-only, does not write into Jcode state, and reports truthful outcomes for the workflows it exposes. Unsupported paths must fail clearly. The artifact and exact commit must be identifiable. Wake receipts should remain opt-in and isolated until their deduplication/recovery contract and negative cases are qualified. This threshold does not imply a public release or broad production readiness.

### Proposed complete-fork threshold

A complete fork handoff requires: a fork-owned and repeatable artifact/release path; green required local gates for the declared supported platforms; resolved or explicitly scoped Windows support; truthful implementation of advertised workflows; separated KCode/Jcode state with tested no-cross-write boundaries; defined updater/rollback behavior; source and release provenance; platform/runtime qualification; and fresh user acceptance against a small explicit workflow set. A numerical coverage or completion percentage cannot be derived from the supplied findings.

## Finish incrementally versus rewrite

**Recommendation: finish incrementally.** The fork already has a meaningful remote identity and isolation/session-import implementation, while the native receipt work is bounded and the major reported gaps have identifiable behaviors. The known issues call for honest completion, targeted negative tests, platform recovery work, and fork release wiring. Nothing in the evidence demonstrates that the inherited architecture is unworkable or that a rewrite would reduce risk.

A rewrite would discard existing integration and isolation work and force broad parity/requalification across a large upstream divergence (earlier audit: 338 versus 2,808 divergent commits between candidate base and fork origin). Reconsider only if a measured architecture constraint blocks the explicit acceptance criteria, after documenting a parity inventory and comparing migration cost. No such constraint was established here.

## Prioritized work plan and planning ranges

Ranges are qualified engineering-hour estimates for one engineer, based only on this evidence packet. They exclude queue time, review/CI latency, unknown upstream conflicts, and new product scope. Confidence is lower where Windows environment, exact supported-platform contract, or fork release workflow is unknown. Parallel work can reduce calendar time but not total effort.

| Priority / node | Work | Estimate | Confidence / dependency |
|---|---|---:|---|
| P0-A | Confirm exact acceptance contract and supported platform matrix; define “success,” session import boundary, update/rollback expectation, and whether wake receipts are in first useful candidate. | 2-4 h | Low-medium; owner/product intent partly unknown. Must precede final acceptance claims. |
| P0-B | Fix and deterministically test receipt concurrent retry/double-dispatch; define terminal receipt semantics. | 4-8 h | Medium; depends on P0-A receipt scope. |
| P0-C | Repair Windows `.bak` recovery and add fault-injection/recovery tests. | 4-10 h | Low-medium; requires Windows-capable execution and confirmed storage contract. |
| P0-D | Decide transcript durability contract; close/document process-crash and power-loss windows with tests or explicit at-least-once semantics. | 4-10 h | Low; storage guarantees and desired duplicate tolerance need specification. |
| P1-A | Resolve fake swarm rename and auto-dream behavior; review PR #105/#99 current status, then finish forward with truthful behavior and focused checks. | 6-16 h | Low-medium; PR state and change quality need recheck. |
| P1-B | Define and implement Windows stdin behavior; run declared Windows build/runtime gates. | 6-20 h | Low; exact UX and Windows environment unclear. |
| P1-C | Establish fork-specific owned artifact/release workflow, provenance, versioning, and rollback/update policy. | 8-20 h | Low-medium; release conventions/hosting details are not in the evidence packet. |
| P1-D | Reconcile isolated wake candidate with the current fork lineage or rebase/port deliberately, preserving snapshot provenance and reviewing divergence. | 6-16 h | Low; large divergence makes integration uncertain. Depends on correctness fixes and explicit decision to ship wake receipts. |
| P2-A | Run complete required local gates on exact release candidate and supported platforms; repair failures and record evidence. | 8-24 h | Low; scope and Windows baseline unknown; this is work, not assumed-green validation. |
| P2-B | Isolated runtime qualification: KCode profile/socket, read-only import, no cross-writes, update behavior, and declared user workflows. | 4-12 h | Medium; depends on an owned artifact and P0-A criteria. |
| P2-C | Fresh operator/user acceptance and release decision against the explicit criteria. | 2-6 h agent effort, plus user availability | Low; cannot be inferred or substituted with test results. |

A plausible minimum useful-candidate path is P0-A, the relevant source fixes, P1-C artifact availability, focused/full local gates, then P2-B. The full-fork path additionally requires platform completion, release qualification, and P2-C. Total ranges should not be summed as a committed schedule: nodes can overlap, and unresolved platform/release assumptions may change them.

## Evidence limits and explicit unknowns

- All source/runtime/hosted-state observations above are attributed to the earlier audit at approximately October 16, 23:40 PDT; this artifact does not refresh them.
- No source line numbers are included because none were supplied and no targeted line verification was needed to produce this recovery.
- Reported checks are not independently rerun here. No coverage percentage, whole-workspace green status, hosted CI result, Windows fault-injection result, candidate live-runtime result, or user acceptance is claimed.
- The concurrent retry interleaving and `.bak` loss path are source-level findings, not executed reproductions. Power-loss transcript loss is a fault-model concern, not reproduced evidence.
- PR #105/#99 and other remote states may have changed after the snapshot. Recheck before acting.
- The installed Jcode process and KCode remote/candidate are distinct identities and state roots. No conclusion about installed KCode runtime behavior follows from the upstream Jcode daemon observation.
- No screenshots, recordings, secrets, outbound messages, upstream writes, or destructive operations were used or performed.
