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
