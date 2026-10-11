# Mounted surface ledger — pass 1

Frozen source: `046ea2af5e01e84449f65d086510b51152360215`. Date: 2026-09-30.

| Surface | Entry evidence | Current classification | Next proof |
|---|---|---|---|
| Main CLI/TUI | src/main.rs, src/cli/**, jcode-tui | CURRENT / primary | command -> handler -> runtime map; installed smoke |
| Server/daemon | jcode-app-core/server/** | CURRENT / core spine | protocol/journey map; runtime identity candidate qualified |
| Harness API/server | jcode-harness-api + server + src/bin/harness.rs | CURRENT/PRESENT; generic capability upstreamed | prove owned semantic delta or move to upstream/adapter |
| SDKs | Rust SDK + TypeScript SDK | CURRENT/PRESENT; generic capability upstreamed | compatibility contract, versioning, client/server matrix |
| MCP | jcode-base/mcp + tool/mcp | CURRENT | auth/schema/cache/restart journeys |
| Provider runtimes | Claude/Gemini/Grok/OpenRouter/ForgeCode etc. | CURRENT/PRESENT | distinguish upstreamed providers from owned adapters |
| ForgeCode runtime | owned jcode-provider-forgecode-runtime | ADAPTER candidate | golden semantic fidelity; default contribute/external runtime |
| HERDR | jcode-herdr | ADAPTER candidate | real caller and lifecycle; does not justify deep fork |
| Phone/remote server scripts | scripts/phone-server, remote gateway | PRESENT / operational candidate | accepted scope, deployment evidence, security boundary |
| Swarm/comm DAG | server comm_* and swarm_* | CURRENT but generic capability upstreamed | retain only semantic differences |
| Memory/cache/compaction | owned + upstream descendants | SEMANTIC_COMPARE | matched behavior/perf, no crate-name credit |
| Shell/terminal/tool-search skeletons | divergent crate paths | HISTORICAL/STRUCTURAL partly zero-line | excluded from mature breadth unless substantive mounted code proven |
| Bench/probe binaries | src/bin/*bench/probe | AUXILIARY evidence tooling | never product completion by themselves |

Immediate rule: current upstream + thin adapters is the challenger. Mounted owned behavior must survive that comparison to remain KCode product scope.
