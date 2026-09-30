# Semantic findings — KCode

Source `046ea2af5e01e84449f65d086510b51152360215`; inspected 2026-09-29. No native KCode build, daemon or installed artifact was executed.

## K-F001 — Client build identity is insufficient for runtime acceptance

AGENTS.md states that `jcode run` and interactive sessions use the long-lived shared-server binary under `~/.jcode/builds/shared-server/jcode`, a symlink into immutable version directories. It explicitly warns that a new cargo build may remain inert while runtime checks hit the old daemon. This is a documented risk, not a fresh observation of an installed process.

Acceptance must bind source/lock/configuration, built and installed client digest, resolved live daemon digest, negotiated protocol, socket/endpoint, storage root and run identity. A new-client/old-daemon negative control must be rejected even if behavior appears correct. Never repoint a user's shared daemon merely to test; use an isolated instance/socket/storage root and record the isolation. Source-to-runtime tracing and native reproduction remain open.

## K-F002 — Mounted macOS startup changes executable trust metadata

In src/main.rs, `run_main()` invokes `self_heal_macos_code_signature()` before general startup. The macOS branch removes `com.apple.provenance` and `com.apple.quarantine` xattrs and invokes `codesign --force --deep --sign -` on the current executable, with failures ignored. The code comment's OS-diagnostic explanation is not independently verified by this pass.

This is an architecture and supply-chain question, not an instruction to delete the function or an established exploit claim. A release whose signature/digest can change after installation needs an explicit accepted policy and before/after identity. Experiment on supported clean macOS environments with signed release, local development build, read-only executable, failed tooling and repeated startup. Determine whether a narrowly scoped installer repair or opt-in local-build path can replace unconditional startup mutation. No macOS experiment has run here.

## K-F003 — README identity is not installed fork identity

The fork README names KCode's upstream as 1jehuang/jcode, while installation points to jcode.sh and manual links use KooshaPari/jcode. Those may be legitimate redirects or upstream delivery; their actual artifact resolution was not tested. A fork-qualified journey must record repository/release/tag/channel, downloaded digest, installed target and live daemon. Do not call an upstream installation a verified KCode installation without evidence.

## K-F004 — Low-memory leadership remains an unverified differentiation

README memory tables vary materially with embeddings on/off and session count. The inspected excerpt does not establish a matched independent workload, model, machine, cache, build profile and measurement protocol. Compare current pinned alternatives using equal tasks/settings, reporting PSS/RSS, startup, throughput, correctness and maintenance cost separately. Do not import a marketing superlative as a product requirement or fact.

## K-F005 — Historical test fractions are not product completion

HANDOFF.md is a Sep 15 author report of a 266-test subset with eight named failures and broad claims about core functionality. It is neither current full-suite evidence nor a mature-contract denominator. Its failures include authorization/provider wiring and process/terminal lifecycle, not only cosmetic output. Preserve the report, follow fixing commits and rerun relevant tests at an exact candidate before classifying defects as present or resolved.

## K-F006 — A transcript heuristic is auxiliary, not a behavior oracle

score_shard.py scores planning text, number of todo calls and completed statuses. It does not observe implementation correctness. No caller or CI evidence was found in this pass showing it is used for product acceptance, so it is NOT reported as a live acceptance bug. Explicitly exclude it and any similar self-reported task completion from product qualification.

## K-F007 — Exported framework scope and mounted API scope are unresolved

src/lib.rs reexports jcode_tui and delegates run to cli::startup::run; the crates tree includes harness API/server, protocol and runtime modules. Those are actual source surfaces, not proof that the SDK replaces Agentora or that every capability is reachable. Trace handlers, consumers and persistence before a general SDK or consolidation decision.

## Diagnostic evidence boundary

The companion identity-policy model uses synthetic receipts to test wrong daemon, wrong contract/configuration/environment/verifier, missing evidence, skipped cases, stale/conflicting results and collector failure. It is an oracle-design experiment only: no native daemon, cryptographic attestation, real freshness clock, immutable store or policy authorization is implemented or verified by it. Passing the model never qualifies KCode itself.

## K-F008 — Persisted ToolUse does not close the post-effect/pre-result crash window

`crates/jcode-app-core/src/agent/turn_loops.rs` persists the assistant message containing `ToolUse` before local tool execution. It then calls `registry.execute(...)`; on success it adds a `ToolResult` to the in-memory session and only later persists tool results with `session.save()`. This ordering is useful because intent can survive a restart, but it does not tell a replacement worker whether an external side effect committed when the process dies after `registry.execute` reaches the downstream target and before the result save.

The existing session journal/reload recovery protects transcript continuity and corrupted/torn persistence; it is not, from the inspected source, an external-effect reconciliation ledger. This is a source-level ambiguity finding, not a claim that current restart code blindly duplicates every tool. Required resolution: map tool classes by side-effect/reconcilability, persist a durable effect intent/dispatch identity before execution, record confirmed outcome/receipt afterward, and force UNCERTAIN reconciliation before retry when the downstream outcome is unknowable. Never infer exactly-once from session resume alone.

## K-F009 — Startup-time executable mutation is fork-specific and conflicts with runtime identity provenance

Frozen KCode invokes a macOS startup routine that removes provenance/quarantine xattrs and force ad-hoc re-signs the current executable. Current upstream source inspected on 2026-09-30 does not contain this startup routine; upstream installation removes quarantine instead. This means K-F002 is a fork-specific retained risk rather than inherited upstream behavior.

Candidate #22 (`a9daae12...`) changes the startup repair to explicit opt-in via `JCODE_MACOS_STARTUP_REPAIR`; its macOS Trust Policy workflow passed. That qualifies the policy/compile primitive only. Before closing the finding, run signed-release and local-build experiments recording executable digest, codesign identity/designated requirement and xattrs before/after first/repeated launch, plus read-only executable and unavailable-tool cases. Runtime evidence must bind the post-policy artifact actually serving requests.
