# 10-Minute Task Breakdown for Kcode Adoption

## Target 1: Bash Security Pipeline

| Task | Description | Dependencies | Crate |
|------|-------------|--------------|-------|
| 1.1 | Add `quote_parser.rs` module with 3 extraction modes | None | kcode-command-risk |
| 1.2 | Add `redirection.rs` with safe redirection stripping | None | kcode-command-risk |
| 1.3 | Add `substitution.rs` with 12 command substitution patterns | None | kcode-command-risk |
| 1.4 | Add `zsh_dangerous.rs` with 17 Zsh-specific dangerous commands | None | kcode-command-risk |
| 1.5 | Add `heredoc.rs` with line-based heredoc validation | None | kcode-command-risk |
| 1.6 | Add `pipeline.rs` orchestrating 23 checks in sequence | 1.1-1.5 | kcode-command-risk |
| 1.7 | Write tests for quote parser | 1.1 | kcode-command-risk |
| 1.8 | Write tests for heredoc validation | 1.5 | kcode-command-risk |
| 1.9 | Write tests for full pipeline | 1.6 | kcode-command-risk |

## Target 2: Micro-Compact

| Task | Description | Dependencies | Crate |
|------|-------------|--------------|-------|
| 2.1 | Create `kcode-micro-compact` crate structure | None | kcode-micro-compact |
| 2.2 | Implement `token_estimate.rs` with 4/3 padding | 2.1 | kcode-micro-compact |
| 2.3 | Implement `compactable_tools.rs` with tool set | None | kcode-micro-compact |
| 2.4 | Implement `time_based.rs` trigger | 2.2 | kcode-micro-compact |
| 2.5 | Implement `cached_mc.rs` path | 2.3 | kcode-micro-compact |
| 2.6 | Write tests | 2.4, 2.5 | kcode-micro-compact |

## Target 3: autoDream

| Task | Description | Dependencies | Crate |
|------|-------------|--------------|-------|
| 3.1 | Create `kcode-auto-dream` crate structure | None | kcode-auto-dream |
| 3.2 | Implement `time_gate.rs` with 24h threshold | 3.1 | kcode-auto-dream |
| 3.3 | Implement `session_gate.rs` with 5-session threshold | None | kcode-auto-dream |
| 3.4 | Implement `consolidation_lock.rs` with file lock | None | kcode-auto-dream |
| 3.5 | Implement `scan_throttle.rs` with 10min throttle | None | kcode-auto-dream |
| 3.6 | Implement `consolidation_prompt.rs` builder | 3.2-3.5 | kcode-auto-dream |
| 3.7 | Write tests | 3.6 | kcode-auto-dream |

## Target 4: Cache Vectors

| Task | Description | Dependencies | Crate |
|------|-------------|--------------|-------|
| 4.1 | Create `kcode-cache-vectors` crate structure | None | kcode-cache-vectors |
| 4.2 | Implement `cache_hash.rs` with djb2 + Bun.hash fallback | 4.1 | kcode-cache-vectors |
| 4.3 | Implement `prompt_state.rs` snapshot type | 4.2 | kcode-cache-vectors |
| 4.4 | Implement `cache_break.rs` detection logic | 4.3 | kcode-cache-vectors |
| 4.5 | Implement `diffable_content.rs` for debugging | None | kcode-cache-vectors |
| 4.6 | Write tests | 4.4 | kcode-cache-vectors |

## Target 5: Permission Bubble

| Task | Description | Dependencies | Crate |
|------|-------------|--------------|-------|
| 5.1 | Create `kcode-permission-bubble` crate structure | None | kcode-permission-bubble |
| 5.2 | Implement `fork_message.rs` construction | 5.1 | kcode-permission-bubble |
| 5.3 | Implement `child_template.rs` with 10 rules | None | kcode-permission-bubble |
| 5.4 | Implement `fork_guard.rs` recursive detection | None | kcode-permission-bubble |
| 5.5 | Write tests | 5.2-5.4 | kcode-permission-bubble |

## Target 6: Tool Search

| Task | Description | Dependencies | Crate |
|------|-------------|--------------|-------|
| 6.1 | Create `kcode-tool-search` crate structure | None | kcode-tool-search |
| 6.2 | Implement `tool_name.rs` parser (MCP + CamelCase) | 6.1 | kcode-tool-search |
| 6.3 | Implement `keyword_search.rs` with scoring | 6.2 | kcode-tool-search |
| 6.4 | Implement `description_cache.rs` memoization | None | kcode-tool-search |
| 6.5 | Write tests | 6.3 | kcode-tool-search |

## Target 7: Session Memory

| Task | Description | Dependencies | Crate |
|------|-------------|--------------|-------|
| 7.1 | Create `kcode-session-memory` crate structure | None | kcode-session-memory |
| 7.2 | Implement `section_analysis.rs` with token counting | 7.1 | kcode-session-memory |
| 7.3 | Implement `budget.rs` with 12K total / 2K per section | 7.2 | kcode-session-memory |
| 7.4 | Implement `template.rs` with custom template loading | None | kcode-session-memory |
| 7.5 | Implement `substitute.rs` with {{var}} syntax | None | kcode-session-memory |
| 7.6 | Write tests | 7.3, 7.5 | kcode-session-memory |
