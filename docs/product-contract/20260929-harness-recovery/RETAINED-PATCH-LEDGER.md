# KCode retained-patch ledger — pass 1

Date: 2026-09-29. Comparison: current upstream control `1jehuang/jcode@76df6464bf64b504996056b8934ec4ec6e8a4d2d` versus frozen owned master `046ea2af5e01e84449f65d086510b51152360215`.

Git compare reports the owned master and current upstream are diverged: owned master has 136 commits not in the current upstream control and is 2,320 commits behind it. **This is not a value score.** The 136 commits are the initial retained-patch search space; the 2,320 upstream commits are rebase/reintegration pressure.

The compare includes large generated/audit artifacts, branding, handoff/task docs and product code. Therefore 136 commits is not equivalent to 136 necessary product patches.

## First classified surfaces

| Surface | Owned-master evidence | Current disposition |
|---|---|---|
| Sep 15 assessment/evidence corpus | `.audit/jcode-assessment-2026-09-15/**` | HISTORICAL/AUXILIARY; not a reason to fork; preserve outside behavioral patch count |
| Fork branding/release docs | README/CHANGELOG/branding/release workflow | PRODUCT IDENTITY / distribution; evaluate thin overlay rather than deep fork |
| Startup/resource benchmark | `benches/startup.rs` and README performance material | UNVERIFIED DIFFERENTIATION; rerun matched against current upstream |
| Cache vectors / micro-compaction / permission bubble and other added agent modules | owned source additions in compare | CANDIDATE RETAINED BEHAVIOR; inspect current upstream equivalents before counting as unique |
| HANDOFF/TASK_BREAKDOWN | historical work reports | SUPPORTING/HISTORICAL; never implementation proof |
| Current daemon identity hardening | implementation candidate #14, not frozen master | CANDIDATE OWNED ADAPTER/PATCH; independently validate, then test whether upstream contribution/thin adapter is preferable |

## Decision rule

For every candidate retained behavior, classify UPSTREAMED/COMMODITY, CONTRIBUTE UPSTREAM, THIN OVERLAY/ADAPTER, RETAIN PATCH, or REJECT/SUPERSEDED.

A deep fork survives only if the RETAIN PATCH set is material enough that upstream + overlay/contributions is worse under measured correctness, maintenance, integration and user outcomes.

## Next pass

Diff the 136-commit owned side semantically by user intent and behavior, not file count. Start with provider/subscription routes, compaction/cache, permission/safety, harness API/SDK, self-dev/server lifecycle, Windows/Pine compatibility and owned integration crates. For each, search current upstream source before calling it unique.

# Retained-patch ledger update — 2026-09-29

Historical assessment at `.audit/jcode-assessment-2026-09-15/ASSESSMENT.md` states that the fork extended upstream with elicitation overlays, swarm coordination enhancements and AGENTS.md governance. Treat this as historical assessment interpretation, not direct current user intent.

Current-upstream falsification changed the disposition:
- **Harness API/SDK**: current upstream has jcode-harness-api, jcode-harness-api-server, jcode-sdk and TypeScript SDK surfaces. Generic harness API/SDK is UPSTREAMED/COMMODITY unless an owned semantic difference is demonstrated.
- **Swarm coordination**: current upstream documents implemented swarm coordination and daemon snapshot recovery. Generic swarm functionality is UPSTREAMED/COMMODITY; only behaviorally distinct owned semantics can survive.
- **Elicitation/discovery**: current upstream contains a current discovery elicitation specification and eval design. The old assessment's generic elicitation claim is no longer sufficient differentiation.
- **Governance/audit files**: process/support artifacts, not product differentiation.
- **Runtime evidence identity**: current candidate #14 remains a potentially useful owned patch; current upstream equivalence has not yet been established.
- **Provider/subscription routes, Pine/native-Windows obligations, Pheno integrations**: still require direct source comparison.

This is evidence falsifying several historical differentiation candidates. It does not prove the deep fork has no remaining value.

### Current-upstream runtime identity check
A later current-upstream source observation at `1jehuang/jcode@de65ade33d514b31a43885318179b3622f321170` shows Pong carries `native_ssh_protocol` and a capabilities vector, but the inspected Pong shape does not carry server version, git hash, PID or executable digest. Upstream does hash binaries in benchmark tooling, so hashing itself is commodity; binding the digest to the daemon that answered a runtime Ping remains a candidate semantic delta. This is source evidence, not a claim that no other upstream endpoint exposes equivalent identity; continue searching before declaring uniqueness.

### Correction: upstream already has substantial runtime identity
Further current-upstream inspection found server version/name in state/gateway surfaces, server identity with git hash in debug/server state, UI logic specifically distinguishing client/server versions, and stale-server reload handling. Therefore **runtime identity in general is not KCode differentiation**. The narrower candidate delta in #14 is responder-bound executable SHA-256/PID in the cheap Ping readiness path, plus evidence-policy integration around it. Even that may be better contributed upstream or implemented as a thin adapter; survival requires a matched operational benefit, not merely absence from Pong.
