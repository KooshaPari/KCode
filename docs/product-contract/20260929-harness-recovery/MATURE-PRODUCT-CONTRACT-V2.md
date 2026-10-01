# Mature product contract — KCode

KCode is the jcode-lineage coding-agent client/runtime candidate over canonical shared harness semantics. Repository survival is not guaranteed; obligations may migrate to current jcode, shared harness, canonical CLI or adapters.

## Obligation families
K-FAM-CLIENT: interactive TUI/CLI, headless/API/SDK, config/auth/provider, diagnostics/update with explicit errors/degradation.
K-FAM-THREAD: thread/session/turn lifecycle, typed events, cancellation/reconnect/resume and stable identity.
K-FAM-DAEMON: daemon/client protocol identity, exact serving runtime evidence, stale/mismatched daemon handling and versioned compatibility.
K-FAM-TOOLS: coding tools, shell/process/files/git/search/external tools with authorization/effect identity and explicit failures.
K-FAM-PROVIDER: provider runtime adapters preserve system/history/tools/results/cancel/resume/stream semantics or declare loss.
K-FAM-DURABILITY: runtime session continuity is distinct from durable effort; uncertain effects reconcile before retry; worker replacement preserves effort/evidence.
K-FAM-PROJECTION: GUI/TUI/CLI/API/SDK clients consume shared semantic events without terminal scraping.
K-FAM-PARALLEL: isolated attempts/workspaces, scheduling/QoS/budgets and attributable evidence.
K-FAM-EVIDENCE: candidate/config/verifier-bound evidence and independent MACE grading.
K-FAM-SEC: sandbox/permissions/secrets/approval provenance; macOS/runtime identity must not be silently mutated.
K-FAM-PLATFORM: native Windows/POSIX/Pine role where accepted; unsupported environments explicit.
K-FAM-UPSTREAM: minimize delta from active jcode; upstream contribution/external adapters preferred; retained patches require semantic evidence.
K-FAM-OPERATIONS: release provenance, migration/coexistence/rollback, observability, performance/resource budgets and supported matrix.

## Non-goals
- retaining the deep fork for sunk cost;
- treating owned crate names/stubs/audit artifacts as differentiation;
- duplicating generic harness/durable orchestration in KCode;
- assuming tool_call_id implies external exactly-once;
- claiming provider adapter fidelity without golden semantic tests.

## Existence condition
KCode as a repository survives only if current jcode + contributions + shared harness + adapters cannot satisfy accepted obligations at lower maintenance cost while preserving measured behavior/performance. Capabilities may survive even if KCode does not.