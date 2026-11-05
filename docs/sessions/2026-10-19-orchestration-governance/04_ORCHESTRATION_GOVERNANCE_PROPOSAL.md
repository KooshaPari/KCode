# Orchestration Governance Proposal
Status: draft for operator review | Date: 2026-10-19 | Author: subagent (triage)
Session: `docs/sessions/2026-10-19-orchestration-governance/`

---

## 1. Problem statement

The "management-only root lane" rule (verbatim operator, 2026-10-11 21:08: "use subagents ALWAYS, main ctx strictly must only be management\high level ctx") is **written down** in `~/AGENTS.md` and in `~/.jcode/memories/global/harness-agents.md`, but it is **not enforceable** in any harness today. Three independent failure modes recur:

1. **Drift across the 5 copies.** Five copies of AGENTS.md exist (`~/AGENTS.md`, `~/.codex/AGENTS.md`, `~/.forge/AGENTS.md`, `~/.kcode/AGENTS.md`, `~/.helioslite/AGENTS.md`). SHA256s differ; only the canonical `~/AGENTS.md` actually carries the "Always Subagent" CRITICAL section. The other 4 files do not contain the verbatim text even though `harness-agents.md` documents the change as "all three harnesses see this on next session start."
2. **Noisy-command output is not gated.** The existing pre-tool gate (`~/bin/jcode-tool-safety`) classifies *shell commands* (rm, dd, gh, git, …) and routes dangerous ones to `elicitate` for operator approval. It does not measure *output volume* — a parent agent running `rg -r ... | wc -l` or `git log --oneline -200` slips through untouched.
3. **No cross-harness contract for subagent prompts.** "One scope, one report shape, one stop condition" lives in prose only. Different orchestrators (swarm/Codex/Forge/herdr panes) format subagent prompts differently, so the "ask the subagent for a report, then read only the report" pattern breaks whenever a task hops harnesses.

This proposal names the rules, ranks the enforcement mechanisms, and lists the file deltas that would close the gaps.

---

## 2. Current state matrix

SHA256 checksum, line count, and rule-presence for each AGENTS.md copy (verified 2026-10-19 via `wc -l`, `sha256sum`, `grep -n`):

| Harness | Path | Lines | SHA256 (first 8) | Mgmt-only CRITICAL | 3-Harness rule | Routing pointer | Cockpit tree rule |
|---|---|---:|---|---|---|---|---|
| Jcode (canonical) | `~/AGENTS.md` | 4247 | `157104cd` | **YES** (L42) | **YES** (L3) | **YES** (herdr-org-model L82) | **YES** (L4239) |
| Codex CLI | `~/.codex/AGENTS.md` | 795 | `a3805086` | NO | **YES** (L10) | **YES** (L37) | **YES** (L787) |
| Forge CLI | `~/.forge/AGENTS.md` | 715 | `3762f13b` | NO | **YES** (L12) | **YES** (L39) | **YES** (L707) |
| KCode fork | `~/.kcode/AGENTS.md` | 69 | `ba190a36` | NO (stub) | NO (refers to canonical L48) | **YES** (L6) | **YES** (L67) |
| HeliosLite fork | `~/.helioslite/AGENTS.md` | 60 | `54c87417` | NO (stub) | NO (refers to canonical L38) | **YES** (L6) | **YES** (L58) |

**Reading the matrix:**

- "Mgmt-only CRITICAL" = literal section `## CRITICAL: Always Subagent — main context is management-only (operator 2026-10-11 21:08)` present. Only the Jcode canonical has it.
- The more recent **cockpit-tree rule** (operator 2026-10-17) IS in all 5 copies. So updates from 6 days ago propagated; updates from 8 days ago did not. The drift is not noise — it is older content that the propagation discipline skipped.
- The two fork stubs (KCode, HeliosLite) explicitly say `This file is a stub so the fork follows the same model without re-deriving it` — they link back to the canonical `~/.jcode/memories/` memory. They are not the failure point; the parent canonical file's absence from codex/forge is.

Tooling surfaces (verified):

| Tool | Path | Mechanism | Pre-tool hook? | Notes |
|---|---|---|---|---|
| Jcode `pre_tool` | `~/bin/jcode-tool-safety` (224 lines) | shell classifier + `elicitate` | **YES** (set in `~/.jcode/config.toml` L199) | Hard-blocks destructive shell; routes approval-required to `elicitate`. No output-volume check. |
| Shell classifier | `~/bin/jcode-shell-classifier.py` (192 lines) | quoted-aware lexer, risk tags (`rm`, `mkfs`, `gh … write verbs`, etc.) | n/a (called by jcode-tool-safety) | Classifies COMMAND shape, not OUTPUT shape. |
| Codex hooks | `~/.codex/hooks.json` + `~/.codex/config.toml` hooks.state (20+ entries) | PreToolUse / PostToolUse / SessionStart / UserPromptSubmit / Stop / PermissionRequest | **YES** (multiple matchers) | Hook events defined; matcher format verified. |
| Forge hooks | `~/.forge/.forge.toml` | (none visible in grep) | **NO** (not verified exhaustively — Forge evolves rapidly; not verified beyond `.forge.toml` contents) | Has `.mcp.json` (MCP only). |

---

## 3. Evidence from this session

Specific, dated observations from this subagent's run and from the canonical memory file `~/.jcode/memories/global/harness-agents.md`:

- **2026-10-11 21:08** — operator directive issued; `harness-agents.md` records "Files touched by this reassertion" as the canonical AGENTS.md plus all three harnesses. As of 2026-10-19, **8 days later**, codex/forge/kcode/helioslite copies still lack the verbatim CRITICAL section (verified by `grep -c "management-only"` = `2` for canonical, `0` for the other four).
- **2026-10-17T22:12Z** — operator correction on cockpit shape (`cockpit-progress-tree-policy.md`). Within 2 days, **all 5 copies** updated to include the rule. Recent edits do propagate.
- **2026-10-19** — this subagent's own output budget. Without bounded reads (sed/head windows, never `cat` of large files) and a fixed write target, the proposal would have polluted the parent orchestrator context exactly the way the rule forbids.
- **Reference drift cases from session history** (recorded in `~/.jcode/memories/global/harness-agents.md`):
  - Setup-hints orphan referenced in rebalance work.
  - 21 TUI panic failures during a single session (referenced via the ratatui upstream issue #2813 filing).
  - CI log triage that had to be delegated because the parent had been logging directly.
- **Pre-tool hook coverage gap** — external citable evidence in [anthropics/claude-code#69260](https://github.com/anthropics/claude-code/issues/69260): a subagent-hook coverage gap was reported June 2026; resolved via improved observability (hooks do fire, but subagent transcripts don't show hook results). Boris Cherny closed it. Cite as the historical "don't rely only on parent-session pre-tool stats to measure enforcement."
- **operator directive shape** (harness-agents.md L1112-1166) — includes an explicit "Pseudo-dispatching" failure mode: "the parent 'dispatches' but then runs the work itself 'just to check.' This is a bypass." This is the most common observed failure for the management-only rule.

---

## 4. Proposed model

Five numbered rules. Each one is binding only if enforcement is **automatic + operator-free**; otherwise it is **social-only** and can drift again.

### Rule 1 — Single canonical AGENTS.md with derived forks

**Statement:** `~/AGENTS.md` is the canonical authority for harness-level directives. The four derivative copies (`~/.codex/AGENTS.md`, `~/.forge/AGENTS.md`, `~/.kcode/AGENTS.md`, `~/.helioslite/AGENTS.md`) are generated from the canonical by a `~/bin/agents-md-sync.sh` script. Forks are stubs that import sections by reference (e.g. `$(cat ~/.jcode/memories/global/harness-agents.md | sed -n '1112,1166p')` at sync time).

**Rationale:** The matrix shows five hand-maintained copies already diverged on a binding directive within 8 days. The 3-harness rule already requires explicit updates; automation prevents the same drift.

**How checked:** `agents-md-sync.sh --check` runs SHA256 checksums; `agents-md-sync.sh --apply` re-emits all four derived copies. The sync script itself is operator-controlled (NOT under subagent authority).

**Enforcement:** AUTOMATIC (script + cron + post-tool hook on the canonical file).

### Rule 2 — Pre-tool noisy-output gate, not just dangerous-command gate

**Statement:** Before the parent executes a Bash command whose expected output is large (>=200 lines, or a `git log` over >20 commits, or `find`/`rg` over a path containing `*`), the orchestrator MUST route the call to a subagent (or to `bg run_in_background`) and read only the headline. The parent keeps a per-turn **noisy-operation tally**; a tally of >2 in a single turn triggers a "delegate now" reminder in the next turn's user-prompt submit.

**Rationale:** Today's gate (`~/bin/jcode-tool-safety`) classifies command shape, not output volume. A parent that runs `git log --oneline -100 | tail -50` is using 50+ turns of context for a 1-line summary.

**How checked:** Add a `JCODE_NOISY_BUDGET` env var (default 2). After each bash call, `jcode-tool-safety` increments if the call was a known high-volume pattern. After a parent resets the budget, the next session-start hook logs the previous session's count.

**Enforcement:** AUTOMATIC (hook + post-session-start reminder). Uses `PreToolUse` semantics analogous to Claude Code's hooks (see §6 cite #1).

### Rule 3 — Standardized subagent prompt skeleton

**Statement:** Every subagent dispatch uses a template with five sections, in this order: (1) one-line **Scope**, (2) self-contained **Context** (the subagent does NOT inherit parent memory), (3) **Report Shape** (what to return, in what format), (4) **Stop Condition**, (5) **Close instruction** (`close_agent` at end of turn). The template is the only legal form for `swarm spawn`, `herdr agent prompt`, and `bg run_in_background` in a management-only session.

**Rationale:** The matrix shows five harnesses have no shared subagent-prompt contract. A worker that says "do X" without "stop when Y, return Z" can easily drift into a 3-file edit when the parent only asked for one. This rule makes the contract explicit.

**How checked:** `jcode-tool-safety`'s prompt-submit hook can lint the first 200 chars of a `swarm spawn` argument for the five required sections. If absent, the parent receives a soft warning (NOT a block — this is a contract violation, not a destructive action).

**Enforcement:** AUTOMATIC soft warn + SOCIAL hard convention.

### Rule 4 — Cross-harness drift guard

**Statement:** A "drift guard" script `~/bin/agents-md-drift-check.sh` runs `agents-md-sync.sh --check` after every canonical AGENTS.md change, and once per session-start (via `SessionStart` hook). Drift status is recorded in `~/.jcode/memories/global/agents-md-drift.log`. A drift event that lasts >24h escalates to the operator via `elicitate`.

**Rationale:** Drift is silent today. Without an explicit detection cadence, the 3-harness rule is a toothless convention.

**How checked:** Session-start hook call. Drift log timestamp is the receipt.

**Enforcement:** AUTOMATIC (drift detection) + AUTOMATIC escalation (elicitate popup after 24h).

### Rule 5 — Per-turn context budget meter

**Statement:** Parent sessions emit a `cockpit-progress-tree` cell (PASS/FAIL/?) for the "Context Hygiene" dimension at end of every turn, citing the noisy-operation tally from Rule 2. A "Context Hygiene = FAIL" cell is a hard signal that the parent context may be polluted; the next turn the parent MUST consider `/clear` or strategic compaction.

**Rationale:** Today's cockpit tree has 4 dimensions (Implementation / Usability / Validation / Assurance) and never collapses. Adding **Context Hygiene** as a 5th dimension makes the management-only rule observable.

**How checked:** Mandatory cockpit cell presence is verified by the same drift guard (Rule 4) at session end.

**Enforcement:** SOCIAL (the cell is the operator-visible receipt; an agent that omits it is mis-citing).

---

## 5. Enforcement mechanisms (ranked)

Ranked by **enforcement strength × implementation cost × false-positive risk**:

### A. Large-output / noisy-command pre-tool hook (extension of `~/bin/jcode-tool-safety`)

- **Mechanism:** new helper `~/bin/jcode-noisy-classifier.py` invoked from `jcode-tool-safety`'s `bash` case. Pattern-matches known noisy patterns: `git log\b.*-[0-9]{2,}` (limit ≥20), `rg|grep.*-r` over multi-path, `find\b`, `ls -la\b` over `~/` or `/`, `cat\b` of `*.log` / `*.txt` files >50KB, `tail -f`, `gh pr list --json.*-L.*[0-9]{3,}`, `cargo (test|build|check|clippy)`, `npm test`, `pytest\b`, `rustc\b`. On match: increment `JCODE_NOISY_BUDGET`; if budget exhausted, route to `elicitate` with reason "noisy command in orchestrator lane; delegate to subagent or `bg run_in_background`."
- **What it blocks:** the parent directly firing large-output commands.
- **False-positive risk:** LOW. Pattern match is conservative; a parent can always wrap a single command in `bg run_in_background` to bypass (and that is parent-safe — it does not pollute context).
- **Cost:** ONE small Python script (~80 lines) + a 20-line patch in `jcode-tool-safety` to call it.
- **Operator approval needed:** NO. The script is gated behind `JCODE_APPROVAL_MODE=allow` for the first 24h (rollout), then default-on.

### B. Standardized swarm-spawn prompt injection

- **Mechanism:** new helper `~/bin/swarm-prompt-template.sh` that wraps every `swarm spawn` / `herdr agent prompt` / `bg run_in_background` invocation. The wrapper prepends the five-section template (Scope / Context / Report / Stop / Close) and appends the agent's working directory + repo context. The harness-side prefix is configurable per harness.
- **What it blocks:** non-conforming subagent dispatches.
- **False-positive risk:** MEDIUM. A parent that genuinely needs a one-liner research probe cannot use the full template. **Mitigation:** the wrapper accepts `--lite` for one-liner probes; their stop condition is implicit ("return headline, no file edits").
- **Cost:** ONE bash wrapper + a single-pass convenience alias.
- **Operator approval needed:** NO.

### C. Cross-harness drift checker

- **Mechanism:** new script `~/bin/agents-md-drift-check.sh`. Runs `agents-md-sync.sh --check`, records results to `~/.jcode/memories/global/agents-md-drift.log`. Wired into Jcode's `SessionStart` hook and into Codex's `session_start` hook (already exists at `~/.codex/hooks.json`).
- **What it blocks:** silent divergence of the 5 AGENTS.md copies.
- **False-positive risk:** LOW. A canonical edit followed by a stale derived copy IS drift; not a false positive.
- **Cost:** ONE bash script (~30 lines). Codex hook entry is one line in `~/.codex/hooks.json`.
- **Operator approval needed:** YES — the operator must approve the addition of a new hook entry to `~/.codex/hooks.json` for the drift check to fire there too.

### D. Per-turn noisy-operation tally with cockpit Context-Hygiene cell

- **Mechanism:** extends A's noisy classifier to emit a tally on session end. Parent emits a 5th cockpit cell `Context Hygiene: PASS|FAIL|?` citing the tally. A `FAIL` cell = budget exhausted in the prior turn.
- **What it blocks:** nothing directly; it makes the management-only rule observable to the operator.
- **False-positive risk:** NEGLIGIBLE (tally is empirical).
- **Cost:** SMALL. Edit to `~/.jcode/memories/global/cockpit-progress-tree-policy.md` to add the 5th dimension; one-line parent-system-prompt reinforcement.
- **Operator approval needed:** NO (cockpit policy is operator-owned; this is an additive cell).

### Comparison table

| Rank | Mechanism | Strength | FP risk | Operator approval | Rollout |
|---|---|---|---|---|---|
| A | Noisy pre-tool hook | STRONG (blocks at gate) | LOW | NO | 24h in `allow` mode, then default |
| B | Prompt-injection template | MEDIUM (soft warn) | MED | NO | immediate |
| C | Drift checker (sync + log) | STRONG (detects silent divergence) | LOW | YES for codex hook | after operator approval |
| D | Tally + cockpit cell | WEAK (observable only) | NONE | NO | immediate |

---

## 6. External practices consulted

| Source | Practice | How it informs this proposal |
|---|---|---|
| https://code.claude.com/docs/en/hooks | Anthropic PreToolUse / PostToolUse / matcher / handler model; `permissionDecision: "deny"` returns; exit code 2 blocks; matcher syntax (`Bash(git *)` etc.) | Mechanism A is a Jcode analog of PreToolUse. The matcher + `if` field combo is the right shape for "noisy command filter." |
| https://code.claude.com/docs/en/hooks#hooks-in-subagents | "Settings-file hooks run inside subagents" — verified by Boris Cherny in v2.1.233+ (issue anthropics/claude-code#69260). Subagent transcripts do not show hook activity; observability, not a coverage gap. | Confirms Mechanism A works for subagents too — the orchestrator who dispatches `swarm spawn` STILL benefits from noisy-classification at the subagent level. Quoting issue #69260 as historical note: "if 72% of sessions are subagents, hook-based safety that only runs on main is a partial safety net." |
| https://github.com/anthropics/claude-code/issues/69260 | Real-world subagent hook coverage report, status: closed (not a bug, a transcript-observability gap). | Citable precedent that "main-only pre-tool enforcement is insufficient." Bolsters Rule 2. |
| https://codex.danielvaughan.com/2026/05/17/codex-cli-hooks-lifecycle-pretooluse-posttooluse-governance-enforcement/ | Codex CLI hooks reference — every shell command, edit, MCP call can be inspected, blocked, or augmented. Five hook event types: SessionStart, PreToolUse, PostToolUse, UserPromptSubmit, Stop. | Mechanism C relies on Codex's existing `session_start` hook infrastructure. Codex hooks ARE the governance surface — we already have it; just wire `agents-md-drift-check.sh` in. |
| https://www.hookstack.app/guides/openai-codex-hooks | `.codex/hooks.json` schema and how it differs from Claude Code. | Same as above; useful for the operator-approval conversation about editing `~/.codex/hooks.json`. |
| https://gist.github.com/hqman/e29cb6386c539d795767e8c3fd2c959b | Boris Cherny's CLAUDE.md — Workflow Orchestration: "Use subagents liberally to keep main context window clean. Offload research, exploration, and parallel analysis to subagents. One task per subagent for focused execution." `Plan first / verify before done / self-improvement loop` after every correction. | Direct precedent for Rule 3 (subagent per focused task) and Rule 5 (cockpit cell = the operator's receipt). Note: this is THE most-referenced CLAUDE.md in the industry. |
| https://code.claude.com/docs/en/agent-sdk/hooks | Anthropic Agent SDK hooks (programmatic agent interception) | Optional, not adopted in this proposal; mentioned so operator can decide if Programmatic-rule A is preferred to script-based hook. |
| https://alicoding.github.io/claude-parser/anthropic/hook/ | Third-party Claude hook events reference | Useful as a backup spec; matches Anthropic docs. |

---

## 7. Proposed file-by-file change set

| File | Change | Diff size | Operator approval |
|---|---|---|---|
| `~/bin/jcode-noisy-classifier.py` | CREATE — pattern-matches noisy bash; emits `JCODE_NOISY_BUDGET` increment + soft warning | ~80 lines (new) | NO |
| `~/bin/jcode-tool-safety` | EDIT — add call to `jcode-noisy-classifier.py` in `bash` case; add `--noisy-budget` mode to `elicitate` prompt | ~20 lines added | NO (existing script) |
| `~/bin/swarm-prompt-template.sh` | CREATE — wraps swarm/herdr/bg invocations with five-section template | ~50 lines (new) | NO |
| `~/bin/agents-md-sync.sh` | CREATE — emits derived AGENTS.md files from the canonical; supports `--check` (SHA compare) and `--apply` (re-emit) | ~30 lines (new) | NO |
| `~/bin/agents-md-drift-check.sh` | CREATE — runs sync; logs to `~/.jcode/memories/global/agents-md-drift.log`; on >24h drift, calls `elicitate` | ~30 lines (new) | NO |
| `~/.codex/hooks.json` | EDIT — add `session_start` hook entry pointing at `~/bin/agents-md-drift-check.sh` | 1 line added | YES (operator decision: which append rules apply to hooks.json) |
| `~/AGENTS.md` | EDIT — add a "## Synchronization & Drift" section pointing at the sync script and the rule that operators must run it after editing the canonical | ~10 lines added | NO (canonical authority) |
| `~/.codex/AGENTS.md` | REGENERATED by `agents-md-sync.sh --apply` from canonical (after operator approves the new structure) | full re-emit | YES |
| `~/.forge/AGENTS.md` | REGENERATED similarly | full re-emit | YES |
| `~/.kcode/AGENTS.md` | REGENERATED similarly | full re-emit | YES |
| `~/.helioslite/AGENTS.md` | REGENERATED similarly | full re-emit | YES |
| `~/.jcode/memories/global/harness-agents.md` | EDIT — extend the 2026-10-11 directive section with a forward pointer to `agents-md-sync.sh`; cite Mechanism A | ~5 lines added | NO |
| `~/.jcode/memories/global/cockpit-progress-tree-policy.md` | EDIT — add `Context Hygiene` as 5th mandatory dimension; cite Mechanism D | ~15 lines added | YES (cockpit policy is operator-owned) |
| `~/.jcode/config.toml` | EDIT — add `JCODE_NOISY_BUDGET` default to env section | ~3 lines | NO |

**Total additive diff: ~270 lines across 13 files. Three files require operator approval.**

---

## 8. Open questions for the operator

1. **Forge hook support status:** my grep of `~/.forge/.forge.toml` did NOT show a `pre_tool` key. Is Forge hook support on the roadmap, or do we accept that the management-only rule is Jcode+Codex-enforced and Forge-only social? (Decision affects Mechanism C's Forge coverage.)
2. **Approval for Codex `hooks.json` edit:** adding a session_start hook entry requires explicit operator consent because hooks can elevate the harness's safety surface. Is this OK to add now, or do we prefer a separate `~/.codex/hooks.json.d/` indirection?
3. **Default `JCODE_NOISY_BUDGET` value:** 2 is my proposal. The actual budget should match operator tolerance for "the parent may directly run a small number of expected-large commands before forcing a delegation." 0 = "never run a noisy command from the parent," 5 = "loose." What value?
4. **Drift escalation threshold:** I proposed 24h before triggering `elicitate`. Should silent drift escalate at all, or only log? An always-log policy is the safe default; an always-escalate policy produces alert fatigue.
5. **Should `cockpit-progress-tree-policy.md` gain `Context Hygiene` as a 5th dimension?** Or should the cell live elsewhere (e.g. an "Orchestrator lane" cockpit separate from product cockpits)? Boris Cherny's CLAUDE.md precedent uses a simpler 2-line cockpit, not a tree.
6. **Subagent prompt template enforcement:** soft-warn vs. block? Soft-warn is operator-friendly; block enforces the rule but can be mistaken as gate noise during routine dispatches.
7. **Sync script authority:** who runs `agents-md-sync.sh`? Options: (a) canonical editor always runs `--apply` after editing; (b) cron every 6h; (c) hook fires on canonical mtime; (d) drift guard on session-start. (a) is single-owner; (c) and (d) automate but require hook setup.
8. **`elicitate` install state for `jcode-tool-safety`:** the script fail-closes if `elicitate` is not installed (verified at L46-47). Is `elicitate` installed in the Jcode environment AND in Codex? (Not verified.)

---

## 9. Appendix: raw findings

### A. Harness copies — `wc -l` and SHA256 (2026-10-19)

```
     4247 /Users/kooshapari/AGENTS.md
      795 /Users/kooshapari/.codex/AGENTS.md
      715 /Users/kooshapari/.forge/AGENTS.md
       69 /Users/kooshapari/.kcode/AGENTS.md
       60 /Users/kooshapari/.helioslite/AGENTS.md

157104cd06489a851d2de9b1d7d9b54583c464930629ba6db8208d7f19726cbb  /Users/kooshapari/AGENTS.md
a3805086cdc89392bc36185dafadfaa223a8529afa64b5d0e69220da06969015  /Users/kooshapari/.codex/AGENTS.md
3762f13b8a183251316d2751b54e3b20c0deb4e4c0b3dd3ff9221795a11a1294  /Users/kooshapari/.forge/AGENTS.md
ba190a3664bb0cc735a890363bcf2a802262e4b42bb00920acbbd0ed87b53e6d  /Users/kooshapari/.kcode/AGENTS.md
54c874176a514ac68e297bd310c493d1ba07461bf45cb9a3051ac4e4751a9742  /Users/kooshapari/.helioslite/AGENTS.md
```

### B. Grep evidence

- `~/AGENTS.md` contains `management-only` literal string 2 times (verified via `grep -c`).
- Other 4 copies: `grep -c "management-only"` = 0 each.
- `~/.kcode/AGENTS.md` L48 and `~/.helioslite/AGENTS.md` L38: "any AGENTS.md / rules / config change must be applied to jcode, codex, and forge (not just this fork)" — confirms the 3-harness rule was re-asserted into the stubs even though the management-only section was not.
- `~/.jcode/memories/global/harness-agents.md` L1112: `# === Operator directive 2026-10-11 21:08 — subagents ALWAYS, main ctx management-only ===`.
- `~/.jcode/memories/global/harness-agents.md` L1167-1169: "Files touched by this reassertion — `~/.jcode/memories/global/harness-agents.md` (this section); `~/AGENTS.md` (new top-level CRITICAL section: 'Always Subagent — main context is management-only'); All three harnesses see this on next session start (per the '3 Active Harnesses' rule in `~/AGENTS.md`)." The third promise is unverified across all derived copies as of 2026-10-19.

### C. Hook scripts (first 50 lines of mechanism)

- `~/bin/jcode-tool-safety` (224 lines total, read verified 1-100): defines `block()` returning `exit 2`; `request_approval()` routes through `elicitate`; bash case classifies input via `jcode-shell-classifier.py`; pattern matches `mkfs|dd if=|:\(\)\{ :|shutdown|reboot` always-blocked; `rm -rf` of `/etc /bin /sbin /usr /var /private` always-blocked (the regex was tightened specifically to catch `rm -rf /etc` which previously slipped through — confirmed via comment in the script).
- `~/bin/jcode-shell-classifier.py` (192 lines, read verified 1-80): quoted-aware lexer, separates command substitutions (`$(...)`, backticks) and inspects each separately. Tags risk by verb: `SENSITIVE = {rm, mkfs, dd, shutdown, reboot, git, gh, curl, wget, :}`; `GH_WRITE_VERBS` enumeration covers `gh repo / pr / issue / release / …` mutating subcommands. Does NOT tag `tail`, `grep`, `find`, `ls`, `wc`.
- `~/.codex/config.toml` L7 (`notify`), L43 (`hooks = true`), L203 (`[hooks.state]`), L205-247 — 20 hook entry references, includes `pre_tool_use`, `post_tool_use`, `permission_request`, `session_start`, `user_prompt_submit`, `stop`. Verified entry naming pattern: `/Users/kooshapari/.codex/hooks.json:<event>:<index>:<revision>`.

### D. Evidence the operator directive exists

Verbatim from `~/AGENTS.md` L43 (read via `sed -n '40,80p'`):

> "Verbatim operator: 'use subagents ALWAYS, main ctx strictly must only be management\high level ctx. update memories and agnets.md'"

And `harness-agents.md` L1112-1166: full reassertion with explicit "Pseudo-dispatching: the parent 'dispatches' but then runs the work itself 'just to check.' This is a bypass" failure-mode description.

### E. Cited URLs (full list)

1. https://code.claude.com/docs/en/hooks — Anthropic hooks reference
2. https://code.claude.com/docs/en/hooks#hooks-in-subagents — Subagent hook propagation
3. https://github.com/anthropics/claude-code/issues/69260 — PreToolUse subagent coverage issue (closed, not a bug)
4. https://codex.danielvaughan.com/2026/05/17/codex-cli-hooks-lifecycle-pretooluse-posttooluse-governance-enforcement/ — Codex hooks governance (third-party)
5. https://www.hookstack.app/guides/openai-codex-hooks — Codex hooks schema
6. https://gist.github.com/hqman/e29cb6386c539d795767e8c3fd2c959b — Boris Cherny CLAUDE.md
7. https://code.claude.com/docs/en/agent-sdk/hooks — Anthropic Agent SDK hooks (informational)

### F. Not verified

- `elicitate` install state on Jcode + Codex harnesses (referenced as fail-closed in `jcode-tool-safety`).
- Forge's pre-tool hook mechanism — `.forge.toml` grep returned no `pre_tool` / `hook` keys, but Forge evolves rapidly; not exhaustively searched.
- Whether subagents in `bg run_in_background` calls inherit `JCODE_NOISY_BUDGET`. Inferred YES from `~/.jcode/config.toml` env propagation; not verified empirically.
- Whether Codex's `~/.codex/hooks.json` is symlinked or duplicated; harness copy differences suggest duplication, not verified.
- The session-internal claims of "21 TUI failures" and "setup-hints orphan" — referenced from session history in `~/.jcode/memories/global/harness-agents.md`; the specific source session was not re-opened.

---
