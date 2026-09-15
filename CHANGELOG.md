# Changelog

All notable changes to the KooshaPari fork of Jcode will be documented in this file.

## v0.85.1-k1.1.0 (2026-09-15)

HERDR integration release. Terminal runtime integration, ForgeCode provider improvements.

### Added

- **HERDR terminal runtime integration** (`jcode-herdr` crate) -- 6 modules, 11+ tests
  - Agent lifecycle state reporting (working/idle/blocked) via Unix socket
  - Debounced idle transitions (configurable, default 250ms)
  - Error-hold with retry grace (configurable, default 2500ms)
  - Monotonic sequence numbers for event ordering
  - Screen manifest TOML generation for jcode and ForgeCode detection
  - `--herdr` CLI flag for testing outside HERDR pane
  - `--herdr-kind` CLI arg for agent type routing (jcode/forge/forgecode)
  - `jcode herdr status` subcommand (pane env, socket path, reporter state)
  - `jcode herdr install` subcommand (screen detection manifests)
  - Env var overrides: `HERDR_JCODE_IDLE_DEBOUNCE_MS`, `HERDR_JCODE_RETRY_GRACE_MS`
- **ForgeCode runtime provider split** -- monolithic lib.rs decomposed into modules
  - `config.rs`: ForgeCodeCliConfig (env-based configuration)
  - `parser.rs`: CLI output types (CliOutput, SseEvent, CliOutputParser)
  - `translator.rs`: ForgeCodeEventTranslator (SSE-to-StreamEvent mapping)
  - `lib.rs`: Provider impl + subprocess execution (~650 lines, down from 1132)
- HERDR install scripts for jcode and ForgeCode screen detection

### Changed

- TUI wired to report working→idle state transitions via HERDR
- Startup sequence initializes HERDR reporter before TUI launch
- HERDR reporter released on clean exit and via drop guard

### Fixed

- ForgeCode provider tool name mapping roundtrip correctness

## v0.85.0-k1.0.0 (2026-09-14)

KooshaPari fork release. First tagged fork version (0.85.0 base + k1.0.0 fork suffix).

### Added

- **Terminal detection crate** (`jcode-terminal-detect`) -- 14 terminals, 15 capability flags
- **Shell integration crate** (`jcode-shell-integration`) -- 6 shells, hook/completion generation
- TUI synchronized update wrapping (DECSET 2026) in 5 render sites
- Bun installer script (`scripts/install_bun.sh`)
- GitHub Packages publish workflow (`.github/workflows/publish-github-packages.yml`)
- ASCII branding assets (`assets/branding/`)
- Elicitation types and TuiEvent wiring
- Swarm dispatch stats to status bar
- Multi-select batch operations for swarm agents
- Agent rename/label editing UI (r key)
- Agent search/filter to swarm panel (/ key)
- Swarm plan DAG visualization to full-page panel
- Permission bubble into session forking with persistent fork depth guard
- Consolidation ops to memory graph
- ToolSearchIndex wired into Registry
- Per-role border colors for swarm gallery tiles
- Subtle background tinting for selected swarm tiles
- Active-vs-idle visual differentiation in strip, dock, and vertical views
- Auto-dream consolidation LLM call
- Micro-compact crate wiring into jcode-app-core
- Cache-vectors and permission-bubble crates wired into jcode-app-core
- Tool-search and session-memory crates wired into jcode-app-core
- `fork_message.rs` with `build_forked_messages` in permission-bubble crate
- `PromptStateSnapshot` fields and `snapshot_current_state` in cache-vectors crate
- `time_gate.rs` with 24h threshold in jcode-auto-dream crate
- Protocol fields: `input_tokens`, `output_tokens`, `queue_depth`, `cost_cents` on `SwarmMemberRuntime`
- Token counts, cost, and queue depth display in agent detail card
- Elapsed time, effort, and auth display in agent detail card
- Enhanced summary line with per-status agent counts
- Integration tests for swarm gallery rendering

### Fixed

- Env var test locking in `apply_patch_tests`, `bash_tests`, `perf`, `swarm` (`lock_test_env`)
- Missing `SwarmMemberRuntime` fields
- Missing `batch_selected` arg in fuzz_audit strip call
- Preserve enriched model routes in busy-session fallback
- Hide bundled docs in self-dev and disable initiative tool
- Missing test imports (`disp_w`, `rgb`) and duplicate `use` statement
- `SwarmStripHint` duplicate definition between `types.rs` and `strip.rs`
- Missing `search_index` field to test Registry initializers
- Thread `selected` param through gallery callers
- Unmatched model pricing coverage across providers
- Dead_code warnings for new crate integrations
- Compile errors for new crates (auto-dream, micro-compact, permission-bubble)
- Unused imports and dead code in `substitution.rs` and permission_bubble integration
- Missing `status_text_color` import and mut binding
- Various crate integration wiring issues (auto-dream, micro-compact, cache-vectors, permission-bubble, tool-search, session-memory)

### Refactored

- Decompose `swarm_gallery.rs` into domain modules
- Extract vertical strip into `strip_vertical.rs`
- Format pending API and TUI changes

### Removed

- 50 stale remote branches cleaned up
