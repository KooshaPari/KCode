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
