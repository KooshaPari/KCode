# 10-Minute Task Breakdown for Jcode Adoption

## Target 1: Bash Security Pipeline

| Task | Description | Dependencies | Crate |
|------|-------------|--------------|-------|
| 1.1 | Add `quote_parser.rs` module with 3 extraction modes | None | jcode-command-risk |
| 1.2 | Add `redirection.rs` with safe redirection stripping | None | jcode-command-risk |
| 1.3 | Add `substitution.rs` with 12 command substitution patterns | None | jcode-command-risk |
| 1.4 | Add `zsh_dangerous.rs` with 17 Zsh-specific dangerous commands | None | jcode-command-risk |
| 1.5 | Add `heredoc.rs` with line-based heredoc validation | None | jcode-command-risk |
| 1.6 | Add `pipeline.rs` orchestrating 23 checks in sequence | 1.1-1.5 | jcode-command-risk |
| 1.7 | Write tests for quote parser | 1.1 | jcode-command-risk |
| 1.8 | Write tests for heredoc validation | 1.5 | jcode-command-risk |
| 1.9 | Write tests for full pipeline | 1.6 | jcode-command-risk |

## Target 2: Micro-Compact

| Task | Description | Dependencies | Crate |
|------|-------------|--------------|-------|
| 2.1 | Create `jcode-micro-compact` crate structure | None | jcode-micro-compact |
| 2.2 | Implement `token_estimate.rs` with 4/3 padding | 2.1 | jcode-micro-compact |
| 2.3 | Implement `compactable_tools.rs` with tool set | None | jcode-micro-compact |
| 2.4 | Implement `time_based.rs` trigger | 2.2 | jcode-micro-compact |
| 2.5 | Implement `cached_mc.rs` path | 2.3 | jcode-micro-compact |
| 2.6 | Write tests | 2.4, 2.5 | jcode-micro-compact |

## Target 3: autoDream

| Task | Description | Dependencies | Crate |
|------|-------------|--------------|-------|
| 3.1 | Create `jcode-auto-dream` crate structure | None | jcode-auto-dream |
| 3.2 | Implement `time_gate.rs` with 24h threshold | 3.1 | jcode-auto-dream |
| 3.3 | Implement `session_gate.rs` with 5-session threshold | None | jcode-auto-dream |
| 3.4 | Implement `consolidation_lock.rs` with file lock | None | jcode-auto-dream |
| 3.5 | Implement `scan_throttle.rs` with 10min throttle | None | jcode-auto-dream |
| 3.6 | Implement `consolidation_prompt.rs` builder | 3.2-3.5 | jcode-auto-dream |
| 3.7 | Write tests | 3.6 | jcode-auto-dream |

## Target 4: Cache Vectors

| Task | Description | Dependencies | Crate |
|------|-------------|--------------|-------|
| 4.1 | Create `jcode-cache-vectors` crate structure | None | jcode-cache-vectors |
| 4.2 | Implement `cache_hash.rs` with djb2 + Bun.hash fallback | 4.1 | jcode-cache-vectors |
| 4.3 | Implement `prompt_state.rs` snapshot type | 4.2 | jcode-cache-vectors |
| 4.4 | Implement `cache_break.rs` detection logic | 4.3 | jcode-cache-vectors |
| 4.5 | Implement `diffable_content.rs` for debugging | None | jcode-cache-vectors |
| 4.6 | Write tests | 4.4 | jcode-cache-vectors |

## Target 5: Permission Bubble

| Task | Description | Dependencies | Crate |
|------|-------------|--------------|-------|
| 5.1 | Create `jcode-permission-bubble` crate structure | None | jcode-permission-bubble |
| 5.2 | Implement `fork_message.rs` construction | 5.1 | jcode-permission-bubble |
| 5.3 | Implement `child_template.rs` with 10 rules | None | jcode-permission-bubble |
| 5.4 | Implement `fork_guard.rs` recursive detection | None | jcode-permission-bubble |
| 5.5 | Write tests | 5.2-5.4 | jcode-permission-bubble |

## Target 6: Tool Search

| Task | Description | Dependencies | Crate |
|------|-------------|--------------|-------|
| 6.1 | Create `jcode-tool-search` crate structure | None | jcode-tool-search |
| 6.2 | Implement `tool_name.rs` parser (MCP + CamelCase) | 6.1 | jcode-tool-search |
| 6.3 | Implement `keyword_search.rs` with scoring | 6.2 | jcode-tool-search |
| 6.4 | Implement `description_cache.rs` memoization | None | jcode-tool-search |
| 6.5 | Write tests | 6.3 | jcode-tool-search |

## Target 7: Session Memory

| Task | Description | Dependencies | Crate |
|------|-------------|--------------|-------|
| 7.1 | Create `jcode-session-memory` crate structure | None | jcode-session-memory |
| 7.2 | Implement `section_analysis.rs` with token counting | 7.1 | jcode-session-memory |
| 7.3 | Implement `budget.rs` with 12K total / 2K per section | 7.2 | jcode-session-memory |
| 7.4 | Implement `template.rs` with custom template loading | None | jcode-session-memory |
| 7.5 | Implement `substitute.rs` with {{var}} syntax | None | jcode-session-memory |
| 7.6 | Write tests | 7.3, 7.5 | jcode-session-memory |
