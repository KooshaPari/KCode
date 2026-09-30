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

### Provider/subscription pressure
Current-upstream source searches also find OpenCode Go/provider metadata, Cursor authentication/runtime, account pools/failover, provider profiles, subscription APIs/catalogs and OAuth/account management. Therefore generic subscription/provider breadth is also UPSTREAMED/COMMODITY. Any retained provider patch must identify an exact behavioral contract absent or materially wrong upstream, with a matched negative/positive test. Historical breadth alone no longer supports the deep fork.

### Windows/Pine correction
Current upstream documents first-class Windows 11 x64/ARM64 support, native named pipes/process lifecycle, Windows CI/E2E/install verification and release/update machinery. Therefore **Windows support itself is not differentiation**. The accepted user pain is narrower: avoid PowerShell/CMD/WSL as the normal development substrate for POSIX-oriented workflows and integrate with the separately scoped Pine compatibility boundary where useful. This must be specified as command/path/env/PTY/process/cancel semantics and tested against current upstream native Windows behavior; do not create a KCode-local shell-translation subsystem merely to claim differentiation.

## Fork-only crate audit — pass 1

Exact crate-name set comparison against current upstream `1jehuang/jcode@de65ade33d514b31a43885318179b3622f321170` finds these owned-snapshot crates absent upstream by name: `jcode-auto-dream`, `jcode-cache-vectors`, `jcode-herdr`, `jcode-micro-compact`, `jcode-permission-bubble`, `jcode-provider-forgecode-runtime`, `jcode-provider-claude-cli-runtime`, `jcode-session-memory`, `jcode-shell-integration`, `jcode-terminal-detect`, `jcode-tool-search`. Name absence is not semantic uniqueness.

First source classification:
- `jcode-auto-dream`: library explicitly says actual consolidation is TODO and `maybe_dream` only logs that consolidation *would* run. EXPERIMENT/STUB; cannot justify fork survival.
- `jcode-provider-claude-cli-runtime`: manifest/source labels it deprecated. SUPERSEDED CANDIDATE unless a current accepted consumer proves otherwise.
- `jcode-provider-forgecode-runtime`: real subprocess provider adapter with tool-name translation, retries and resume handling. CANDIDATE RETAINED INTEGRATION, but architecture preference is external adapter/upstream contribution unless core placement is necessary.
- `jcode-herdr`: real optional Unix-socket lifecycle/session/screen-manifest adapter; commit history includes concrete race/deadlock fixes. CANDIDATE INTEGRATION, but it is conceptually an external-runtime adapter rather than evidence the jcode core must remain forked.
- `jcode-cache-vectors`: concrete prompt-cache snapshot/break diagnostic library. CANDIDATE BEHAVIOR; compare upstream cache diagnostics before retention.
- `jcode-micro-compact`: concrete old-tool-result compaction library. CANDIDATE BEHAVIOR; compare current upstream compaction behavior.
- `jcode-permission-bubble`: concrete fork-message/cache-prefix/depth-guard data library. CANDIDATE BEHAVIOR; compare current upstream subagent permission/fork semantics.
- `jcode-session-memory`: structured bounded session-note library. CANDIDATE BEHAVIOR; compare current upstream memory/session summaries.
- `jcode-shell-integration` and `jcode-terminal-detect`: exported through app-core integration namespace; CANDIDATE UTILITY. Native/Pine contract decides whether retained.
- `jcode-tool-search`: CANDIDATE BEHAVIOR; current upstream has tool discovery/search surfaces under different names, so semantic comparison required.

Workspace membership or a dependency declaration is not reachability. Code search did not return direct symbol references for several of these libraries; because search indexing can miss references, this is an **unresolved mountedness question**, not proof of dead code. Trace Cargo features, reexports and actual call paths before awarding product value.

Recent master history also contains substantial maintenance/infrastructure commits (clippy, provider catalog parity, HERDR test race/deadlock, host Cargo serialization/sccache). These may be valuable engineering work but are not automatically user-facing differentiation.
## Divergent-file decomposition — pass 2
Against current-upstream control, Git reports 300 changed files on frozen owned master. 96 are under the historical assessment `.audit/` corpus; 188 are crate files; the rest are root/governance/benchmark artifacts. The crate delta is concentrated in jcode-app-core (68 files), then jcode-tui-render (13), jcode-command-risk (12), jcode-shell-integration (12), jcode-base (11), and smaller extracted capability crates.

Name-level absence upstream is **not uniqueness**. Behavioral searches already find current-upstream equivalents or descendants for several fork-named areas: compaction/micro-compaction, session/memory architecture, cache monitoring/invalidation, discovery/tool search, and permission/approval surfaces. `jcode-command-risk` exists upstream by name. Therefore these candidates require semantic/matched-test comparison before RETAIN PATCH status.

The `.audit/` 96-file corpus is excluded from behavioral retained-patch value. It remains historical evidence only.

## Owned-only capability source inspection — pass 3
- `jcode-auto-dream`: explicitly a stub at the action boundary (`WouldConsolidate`; TODO actually run consolidation). Its presence is not implemented memory consolidation and earns no mature-product credit.
- `jcode-provider-forgecode-runtime`: real subprocess provider adapter. It serializes CLI requests, translates tools/messages, extracts only the latest usable user prompt, and fingerprints a canonicalized input. This is a composition adapter, not proof of lossless provider semantics. Retention requires golden history/tool/cancel/resume comparison against direct Forgecode.
- `jcode-herdr`: real optional terminal-runtime adapter with lifecycle/session reporting and monotonic sequence handling. This is adapter-shaped integration, not core coding-agent differentiation.
- cache-vectors, micro-compact, session-memory, permission-bubble, shell-integration, terminal-detect and tool-search are substantive libraries by source, but current upstream has behaviorally related cache/compaction/memory/permission/shell/discovery surfaces. They remain SEMANTIC_COMPARE, not RETAIN PATCH.

Architecture implication: adapter-shaped ForgeCode/HERDR functionality can survive without justifying a deep fork if stable extension points exist upstream. Stub functionality must not be counted as product breadth.

## Mounted-liability findings — pass 4
- **ForgeCode provider runtime**: mounted/user-selectable, but its subprocess launch contract uses flags absent from pinned current official ForgeCode (`--output-format`, `--input-format`, `--permission-mode`, `--resume`, `--tools`). This is BROKEN_CURRENT_INTERFACE until a compatible versioned shim or supported machine interface is qualified. Draft candidate #16 fails closed rather than silently degrading semantics.
- **macOS startup self-heal**: frozen fork unconditionally strips provenance/quarantine xattrs and force ad-hoc re-signs the running executable. Current upstream removed this path. This is FORK-SPECIFIC LIABILITY, not differentiation. Draft candidate #18 removes implicit runtime mutation; explicit installer/dev repair may be reintroduced only with scoped evidence.

These findings reduce rather than expand the retained patch set: fork-only code that is broken or harms artifact identity is transition debt.
### HERDR semantic comparison
Current upstream has built-in HERDR support despite lacking the fork's `jcode-herdr` crate name: current source/docs include HERDR environment/socket routing, pane/session reporting, headed terminal spawning, TUI agent-state synchronization, and HERDR-specific terminal behavior. Therefore the fork crate name is not unique capability evidence. Classify generic HERDR integration as UPSTREAMED/COMMODITY; retain only a demonstrated behavioral delta after matched tests.
