//! Claude Code session import.
//!
//! Owns the Claude Code on-disk data model: the `sessions-index.json` index, the
//! per-session JSONL entries, the in-memory types used to surface them in the
//! import UI, and the loaders that turn a JSONL file into a stream of messages
//! for resume and search.
//!
//! `lib.rs` re-exports every public item at the crate root so downstream
//! consumers can keep their existing `use jcode_import_core::X` paths.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::sessions::{
    ExternalMessageRecord, clean_optional_text, extract_external_text_from_json, parse_rfc3339_json,
};

/// Entry in the Claude Code sessions-index.json file.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionIndexEntry {
    pub session_id: String,
    pub full_path: String,
    #[serde(default)]
    pub file_mtime: Option<u64>,
    #[serde(default)]
    pub first_prompt: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub message_count: Option<u32>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub modified: Option<String>,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub project_path: Option<String>,
    #[serde(default)]
    pub is_sidechain: Option<bool>,
}

/// Claude Code sessions-index.json format.
#[derive(Debug, Deserialize)]
pub struct SessionsIndex {
    pub version: u32,
    pub entries: Vec<SessionIndexEntry>,
}

/// Info about a Claude Code session for listing.
#[derive(Debug, Clone)]
pub struct ClaudeCodeSessionInfo {
    pub session_id: String,
    pub first_prompt: String,
    pub summary: Option<String>,
    pub message_count: u32,
    pub created: Option<DateTime<Utc>>,
    pub modified: Option<DateTime<Utc>>,
    pub project_path: Option<String>,
    pub full_path: String,
}

/// Entry in a Claude Code JSONL session file.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeCodeEntry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub uuid: Option<String>,
    pub parent_uuid: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub message: Option<ClaudeCodeMessage>,
    pub timestamp: Option<String>,
    #[serde(default)]
    pub is_sidechain: bool,
}

/// Message content in Claude Code format.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeCodeMessage {
    pub role: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub content: ClaudeCodeContent,
}

/// Content can be either a plain string or array of blocks.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(untagged)]
pub enum ClaudeCodeContent {
    #[default]
    Empty,
    Text(String),
    Blocks(Vec<ClaudeCodeContentBlock>),
}

/// Individual content block in Claude Code format.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeCodeContentBlock {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
        #[serde(default)]
        #[serde(rename = "signature")]
        _signature: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        // Claude Code (notably newer/macOS builds, observed on CLI v2.1.x)
        // sometimes writes `tool_result.content` as an array of content blocks
        // (e.g. `[{"type":"text","text":"..."}]` or `[{"type":"image",...}]`)
        // rather than a plain string. The previous `content: String` shape made
        // the *entire* JSONL entry fail to deserialize on the untagged
        // `ClaudeCodeContent` enum, so such messages were silently dropped on
        // import (real data loss). Accept string, null, and array forms.
        #[serde(default, deserialize_with = "deserialize_tool_result_content")]
        content: String,
        #[serde(default)]
        is_error: Option<bool>,
    },
    #[serde(other)]
    Unknown,
}

/// Deserialize a Claude Code `tool_result` content value that may be a plain
/// string, `null`, or an array of content blocks. The whole entry must keep
/// parsing (never get dropped) regardless of shape.
///
/// Array forms are flattened to their textual content. Crucially, `image`
/// blocks are reduced to a compact `[image]` placeholder rather than their
/// base64 payload: real sessions can carry hundreds of KiB of inline image
/// data per tool result, which would otherwise bloat the imported transcript
/// (and any downstream context) with unreadable base64 text.
fn deserialize_tool_result_content<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(tool_result_content_to_text(&value))
}

/// Convert a `tool_result` content value (string / null / array of blocks) to
/// plain text, replacing `image` blocks with a `[image]` placeholder.
fn tool_result_content_to_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Null => String::new(),
        serde_json::Value::Array(items) => {
            let mut parts: Vec<String> = Vec::new();
            for item in items {
                match item {
                    serde_json::Value::String(text) if !text.trim().is_empty() => {
                        parts.push(text.trim().to_string());
                    }
                    serde_json::Value::Object(map) => {
                        let block_type =
                            map.get("type").and_then(|v| v.as_str()).unwrap_or_default();
                        if block_type == "image" {
                            parts.push("[image]".to_string());
                        } else if let Some(text) = map.get("text").and_then(|v| v.as_str()) {
                            if !text.trim().is_empty() {
                                parts.push(text.trim().to_string());
                            }
                        } else if let Some(content) = map.get("content").and_then(|v| v.as_str())
                            && !content.trim().is_empty()
                        {
                            parts.push(content.trim().to_string());
                        }
                    }
                    _ => {}
                }
            }
            parts.join("\n")
        }
        // Defensive: an unexpected scalar/object shape still yields readable
        // text instead of dropping the message.
        other => extract_external_text_from_json(other, true),
    }
}

pub fn parse_rfc3339_string(value: Option<&str>) -> Option<DateTime<Utc>> {
    value
        .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

pub fn resolve_claude_session_path(
    project_dir: &Path,
    entry: &SessionIndexEntry,
) -> Option<PathBuf> {
    let indexed_path = PathBuf::from(&entry.full_path);
    let fallback_path = project_dir.join(format!("{}.jsonl", entry.session_id));
    if indexed_path.exists() {
        Some(indexed_path)
    } else if fallback_path.exists() {
        Some(fallback_path)
    } else {
        None
    }
}

pub fn claude_code_session_info_from_index(
    path: &Path,
    entry: &SessionIndexEntry,
) -> Option<ClaudeCodeSessionInfo> {
    let message_count = entry.message_count.filter(|count| *count > 0)?;
    let summary = clean_optional_text(entry.summary.clone());
    let first_prompt =
        clean_optional_text(entry.first_prompt.clone()).or_else(|| summary.clone())?;

    Some(ClaudeCodeSessionInfo {
        session_id: entry.session_id.clone(),
        first_prompt,
        summary,
        message_count,
        created: parse_rfc3339_string(entry.created.as_deref()),
        modified: parse_rfc3339_string(entry.modified.as_deref()),
        project_path: clean_optional_text(entry.project_path.clone()),
        full_path: path.to_string_lossy().to_string(),
    })
}

pub fn claude_text_from_content(content: &ClaudeCodeContent) -> Option<String> {
    match content {
        ClaudeCodeContent::Empty => None,
        ClaudeCodeContent::Text(text) => {
            let text = text.trim();
            if text.is_empty() {
                None
            } else {
                Some(text.to_string())
            }
        }
        ClaudeCodeContent::Blocks(blocks) => {
            let text = blocks
                .iter()
                .filter_map(|block| match block {
                    ClaudeCodeContentBlock::Text { text } => Some(text.trim()),
                    ClaudeCodeContentBlock::Thinking { thinking, .. } => Some(thinking.trim()),
                    ClaudeCodeContentBlock::ToolResult { content, .. } => Some(content.trim()),
                    _ => None,
                })
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if text.is_empty() { None } else { Some(text) }
        }
    }
}

pub fn ordered_claude_code_message_entries(entries: &[ClaudeCodeEntry]) -> Vec<&ClaudeCodeEntry> {
    let message_entries: Vec<&ClaudeCodeEntry> = entries
        .iter()
        .filter(|e| {
            (e.entry_type == "user" || e.entry_type == "assistant")
                && e.message.is_some()
                && !e.is_sidechain
        })
        .collect();

    let mut uuid_to_entry: HashMap<String, &ClaudeCodeEntry> = HashMap::new();
    for entry in &message_entries {
        if let Some(ref uuid) = entry.uuid {
            uuid_to_entry.insert(uuid.clone(), entry);
        }
    }

    let mut ordered_entries: Vec<&ClaudeCodeEntry> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();

    let mut roots: Vec<&ClaudeCodeEntry> = message_entries
        .iter()
        .filter(|e| {
            e.parent_uuid.is_none()
                || !uuid_to_entry.contains_key(e.parent_uuid.as_deref().unwrap_or_default())
        })
        .copied()
        .collect();

    // Multiple roots occur when a session's parent chain is broken (e.g. a
    // resumed/forked transcript whose first assistant entry references a
    // parent that lives in another file). Without a tiebreak the roots would
    // be walked in raw file order, which can place a later reply before the
    // prompt it answers (observed on real data: an "assistant" error emitted
    // before its "user" prompt). Order roots by timestamp so the reconstructed
    // transcript stays chronological; entries without a timestamp keep their
    // relative file order behind timestamped ones.
    roots.sort_by(|a, b| {
        let a_ts = parse_rfc3339_string(a.timestamp.as_deref());
        let b_ts = parse_rfc3339_string(b.timestamp.as_deref());
        match (a_ts, b_ts) {
            (Some(a_ts), Some(b_ts)) => a_ts.cmp(&b_ts),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });

    for root in roots {
        let mut current = root;
        loop {
            if let Some(ref uuid) = current.uuid {
                if visited.contains(uuid) {
                    break;
                }
                visited.insert(uuid.clone());
            }
            ordered_entries.push(current);

            let next = message_entries.iter().find(|e| {
                e.parent_uuid.as_ref() == current.uuid.as_ref()
                    && e.uuid
                        .as_ref()
                        .map(|u| !visited.contains(u))
                        .unwrap_or(true)
            });

            match next {
                Some(n) => current = n,
                None => break,
            }
        }
    }

    for entry in message_entries {
        if entry
            .uuid
            .as_ref()
            .map(|uuid| visited.contains(uuid))
            .unwrap_or(false)
        {
            continue;
        }
        ordered_entries.push(entry);
    }

    ordered_entries
}

pub fn load_claude_external_messages(
    path: &Path,
    include_tools: bool,
) -> Vec<ExternalMessageRecord> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(|line| line.ok())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .filter_map(|value| {
            let entry_type = value
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if entry_type != "user" && entry_type != "assistant" {
                return None;
            }
            let message = value.get("message")?;
            let role = message
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or(entry_type)
                .to_string();
            let text = extract_external_text_from_json(
                message.get("content").unwrap_or(&serde_json::Value::Null),
                include_tools,
            );
            if text.trim().is_empty() {
                return None;
            }
            Some(ExternalMessageRecord {
                role,
                text,
                timestamp: parse_rfc3339_json(value.get("timestamp")),
                id: value
                    .get("uuid")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            })
        })
        .collect()
}

/// Decide whether a Claude Code user message makes a good session title.
///
/// Claude Code injects synthetic "user" messages for slash commands, command
/// output, and local-command caveats (wrapped in tags like
/// `<local-command-caveat>`, `<command-name>`, `<local-command-stdout>`). Using
/// the raw first user message as the title surfaces this noise (observed on
/// real data: a session titled `<local-command-caveat>Caveat: ...`). Skip those
/// wrapper messages so the first *real* prompt becomes the title instead.
pub fn claude_title_candidate(text: &str) -> Option<String> {
    let cleaned = text.trim();
    if cleaned.is_empty() {
        return None;
    }
    const WRAPPER_PREFIXES: [&str; 6] = [
        "<local-command-caveat>",
        "<local-command-stdout>",
        "<local-command-stderr>",
        "<command-name>",
        "<command-message>",
        "<command-args>",
    ];
    if WRAPPER_PREFIXES
        .iter()
        .any(|prefix| cleaned.starts_with(prefix))
    {
        return None;
    }
    // Caveat blocks sometimes lead with stray whitespace/newlines before the
    // tag; catch those too without being so broad we drop legitimate prompts
    // that merely mention a command.
    if cleaned.contains("<local-command-caveat>")
        && cleaned.starts_with("Caveat: The messages below were generated")
    {
        return None;
    }
    Some(crate::text_utils::truncate_title(cleaned))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_text_from_blocks_joins_textual_content() {
        let content = ClaudeCodeContent::Blocks(vec![
            ClaudeCodeContentBlock::Text {
                text: " hello ".into(),
            },
            ClaudeCodeContentBlock::Thinking {
                thinking: " thought ".into(),
                _signature: None,
            },
            ClaudeCodeContentBlock::ToolResult {
                tool_use_id: "tool".into(),
                content: " result ".into(),
                is_error: None,
            },
            ClaudeCodeContentBlock::Unknown,
        ]);
        assert_eq!(
            claude_text_from_content(&content),
            Some("hello\nthought\nresult".into())
        );
    }

    #[test]
    fn ordered_claude_entries_follow_parent_chain() {
        let jsonl = [
            r#"{"type":"assistant","uuid":"b","parentUuid":"a","message":{"role":"assistant","content":"there"}}"#,
            r#"{"type":"user","uuid":"a","message":{"role":"user","content":"hi"}}"#,
        ];
        let entries = jsonl
            .iter()
            .map(|line| serde_json::from_str::<ClaudeCodeEntry>(line).unwrap())
            .collect::<Vec<_>>();
        let ordered = ordered_claude_code_message_entries(&entries);
        assert_eq!(
            ordered
                .iter()
                .map(|entry| entry.uuid.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("a"), Some("b")]
        );
    }

    #[test]
    fn tool_result_content_accepts_array_blocks() {
        // Newer/macOS Claude Code writes tool_result.content as an array of
        // content blocks. The entry must still parse (not be dropped) and the
        // array must flatten to its textual content.
        let line = r#"{"type":"user","uuid":"u1","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"hello world"}]}]}}"#;
        let entry = serde_json::from_str::<ClaudeCodeEntry>(line)
            .expect("entry with array tool_result content should parse");
        let ClaudeCodeContent::Blocks(blocks) = entry.message.unwrap().content else {
            panic!("expected block content");
        };
        match &blocks[0] {
            ClaudeCodeContentBlock::ToolResult { content, .. } => {
                assert_eq!(content, "hello world");
            }
            other => panic!("expected tool_result, got {other:?}"),
        }
    }

    #[test]
    fn tool_result_content_accepts_string() {
        let line = r#"{"type":"user","uuid":"u1","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"plain"}]}}"#;
        let entry = serde_json::from_str::<ClaudeCodeEntry>(line).unwrap();
        let ClaudeCodeContent::Blocks(blocks) = entry.message.unwrap().content else {
            panic!("expected block content");
        };
        match &blocks[0] {
            ClaudeCodeContentBlock::ToolResult { content, .. } => assert_eq!(content, "plain"),
            other => panic!("expected tool_result, got {other:?}"),
        }
    }

    #[test]
    fn ordered_claude_entries_sort_broken_chain_roots_by_timestamp() {
        // Real-data shape (session b18f57b9): the assistant reply has a
        // parentUuid that is NOT present in the file, so both entries are
        // "roots". The user prompt has an earlier timestamp than the assistant
        // reply, so it must come first even though it appears second in the
        // file.
        let jsonl = [
            r#"{"type":"assistant","uuid":"b","parentUuid":"missing","timestamp":"2026-06-25T23:37:31.001Z","message":{"role":"assistant","content":"Not logged in"}}"#,
            r#"{"type":"user","uuid":"a","timestamp":"2026-06-25T23:37:30.839Z","message":{"role":"user","content":"hi"}}"#,
        ];
        let entries = jsonl
            .iter()
            .map(|line| serde_json::from_str::<ClaudeCodeEntry>(line).unwrap())
            .collect::<Vec<_>>();
        let ordered = ordered_claude_code_message_entries(&entries);
        assert_eq!(
            ordered
                .iter()
                .map(|entry| entry.uuid.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("a"), Some("b")],
            "earlier-timestamped user prompt should precede the assistant reply"
        );
    }

    #[test]
    fn claude_title_candidate_skips_command_wrappers() {
        assert_eq!(
            claude_title_candidate("<local-command-caveat>Caveat: The messages below..."),
            None
        );
        assert_eq!(
            claude_title_candidate("<command-name>/model</command-name>"),
            None
        );
        assert_eq!(
            claude_title_candidate(
                "<local-command-stdout>Set model to Opus</local-command-stdout>"
            ),
            None
        );
        assert_eq!(claude_title_candidate("   "), None);
        assert_eq!(
            claude_title_candidate("can you fix my jcode server version?"),
            Some("can you fix my jcode server version?".into())
        );
    }

    fn tool_result_block(line: &str) -> ClaudeCodeContentBlock {
        let entry = serde_json::from_str::<ClaudeCodeEntry>(line)
            .expect("entry should parse regardless of tool_result content shape");
        let ClaudeCodeContent::Blocks(blocks) = entry.message.unwrap().content else {
            panic!("expected block content");
        };
        blocks.into_iter().next().expect("at least one block")
    }

    #[test]
    fn tool_result_content_accepts_plain_string() {
        let line = r#"{"type":"user","uuid":"u1","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"plain"}]}}"#;
        match tool_result_block(line) {
            ClaudeCodeContentBlock::ToolResult { content, .. } => assert_eq!(content, "plain"),
            other => panic!("expected tool_result, got {other:?}"),
        }
    }

    #[test]
    fn tool_result_content_accepts_array_text_blocks() {
        // Newer/macOS Claude Code writes tool_result.content as an array of
        // content blocks. The entry must still parse and flatten to text.
        let line = r#"{"type":"user","uuid":"u1","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"hello"},{"type":"text","text":"world"}]}]}}"#;
        match tool_result_block(line) {
            ClaudeCodeContentBlock::ToolResult { content, .. } => {
                assert_eq!(content, "hello\nworld");
            }
            other => panic!("expected tool_result, got {other:?}"),
        }
    }

    #[test]
    fn tool_result_image_block_becomes_placeholder_not_base64() {
        // Image tool_result content must not leak its base64 payload into the
        // imported transcript; it collapses to a compact `[image]` placeholder.
        let blob = "Q".repeat(120_000);
        let line = format!(
            r#"{{"type":"user","uuid":"u1","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"t1","content":[{{"type":"image","source":{{"type":"base64","media_type":"image/png","data":"{blob}"}}}}]}}]}}}}"#
        );
        match tool_result_block(&line) {
            ClaudeCodeContentBlock::ToolResult { content, .. } => {
                assert_eq!(content, "[image]");
                assert!(
                    !content.contains('Q'),
                    "base64 image data leaked into tool_result text"
                );
            }
            other => panic!("expected tool_result, got {other:?}"),
        }
    }

    #[test]
    fn tool_result_mixed_text_and_image_blocks() {
        let line = r#"{"type":"user","uuid":"u1","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"see this"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"AAAA"}}]}]}}"#;
        match tool_result_block(line) {
            ClaudeCodeContentBlock::ToolResult { content, .. } => {
                assert_eq!(content, "see this\n[image]");
            }
            other => panic!("expected tool_result, got {other:?}"),
        }
    }

    #[test]
    fn tool_result_null_content_keeps_entry_alive() {
        // A null/missing content must not drop the whole JSONL entry.
        let line = r#"{"type":"user","uuid":"u1","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":null}]}}"#;
        match tool_result_block(line) {
            ClaudeCodeContentBlock::ToolResult { content, .. } => assert_eq!(content, ""),
            other => panic!("expected tool_result, got {other:?}"),
        }
    }

    #[test]
    fn tool_result_missing_content_defaults_to_empty() {
        let line = r#"{"type":"user","uuid":"u1","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1"}]}}"#;
        match tool_result_block(line) {
            ClaudeCodeContentBlock::ToolResult { content, .. } => assert_eq!(content, ""),
            other => panic!("expected tool_result, got {other:?}"),
        }
    }
}
