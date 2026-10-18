//! External session loaders (Codex, Pi, OpenCode) and shared helpers.
//!
//! Owns the on-disk data model and loaders for the non-Claude-Code external
//! sources (Codex, Pi, OpenCode), the shared file walking / timestamp /
//! `ExternalMessageRecord` / `ExternalSessionRecord` types, and the
//! `extract_external_text_from_json` / `extract_opencode_part_text` text
//! extraction helpers used by the Claude Code and Cursor loaders too.
//!
//! `lib.rs` re-exports every public item at the crate root so downstream
//! consumers can keep their existing `use jcode_import_core::X` paths.

use chrono::{DateTime, Utc};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::text_utils::truncate_title_text;
use crate::truncate_str;

pub type ImportCoreResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub fn clean_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub fn collect_files_recursive(root: &Path, extension: &str) -> Vec<PathBuf> {
    fn walk(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, extension, out);
            } else if path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case(extension))
                .unwrap_or(false)
            {
                out.push(path);
            }
        }
    }

    let mut files = Vec::new();
    walk(root, extension, &mut files);
    files.sort();
    files
}

pub fn collect_recent_files_recursive(root: &Path, extension: &str, limit: usize) -> Vec<PathBuf> {
    fn modified_sort_key(path: &Path) -> u64 {
        path.metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0)
    }

    fn walk(
        dir: &Path,
        extension: &str,
        limit: usize,
        out: &mut BinaryHeap<Reverse<(u64, PathBuf)>>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, extension, limit, out);
            } else if path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case(extension))
                .unwrap_or(false)
            {
                let key = (modified_sort_key(&path), path);
                if out.len() < limit {
                    out.push(Reverse(key));
                } else if out.peek().map(|smallest| key > smallest.0).unwrap_or(true) {
                    out.pop();
                    out.push(Reverse(key));
                }
            }
        }
    }

    if limit == 0 {
        return Vec::new();
    }

    let mut heap: BinaryHeap<Reverse<(u64, PathBuf)>> = BinaryHeap::new();
    walk(root, extension, limit, &mut heap);
    let mut files: Vec<(u64, PathBuf)> = heap.into_iter().map(|entry| entry.0).collect();
    files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    files.into_iter().map(|(_, path)| path).collect()
}

pub fn parse_rfc3339_json(value: Option<&serde_json::Value>) -> Option<DateTime<Utc>> {
    value
        .and_then(|v| v.as_str())
        .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

pub fn extract_external_text_from_json(value: &serde_json::Value, include_tools: bool) -> String {
    fn visit(value: &serde_json::Value, include_tools: bool, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(text) if !text.trim().is_empty() => {
                out.push(text.trim().to_string());
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    visit(item, include_tools, out);
                }
            }
            serde_json::Value::Object(map) => {
                let block_type = map.get("type").and_then(|v| v.as_str()).unwrap_or_default();
                if !include_tools
                    && matches!(block_type, "tool_use" | "tool_result" | "function_call")
                {
                    return;
                }
                if let Some(text) = map.get("text").and_then(|v| v.as_str()) {
                    if !text.trim().is_empty() {
                        out.push(text.trim().to_string());
                    }
                } else if include_tools
                    && let Some(content) = map.get("content").and_then(|v| v.as_str())
                    && !content.trim().is_empty()
                {
                    out.push(content.trim().to_string());
                }
                for (key, nested) in map {
                    if matches!(key.as_str(), "type" | "text" | "content") {
                        continue;
                    }
                    visit(nested, include_tools, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    visit(value, include_tools, &mut out);
    out.join("\n")
}

/// Extract the textual body of an OpenCode message from its part files.
///
/// Modern OpenCode (Go storage, v1.x) stores message bodies in
/// `storage/part/<messageID>/*.json` rather than inline on the message JSON.
/// Each part has a `type` (`text`, `reasoning`, `tool`, `step-start`,
/// `step-finish`, ...). Plain `text` parts are always included; reasoning and
/// tool input/output are included only when `include_tools` is set.
pub fn extract_opencode_part_text(
    parts_base: &Path,
    message_id: &str,
    include_tools: bool,
) -> String {
    let message_parts = parts_base.join(message_id);
    if !message_parts.exists() {
        return String::new();
    }
    let mut out: Vec<String> = Vec::new();
    for part_path in collect_files_recursive(&message_parts, "json") {
        let Ok(file) = File::open(&part_path) else {
            continue;
        };
        let Ok(part) = serde_json::from_reader::<_, serde_json::Value>(file) else {
            continue;
        };
        let part_type = part
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        match part_type {
            "text" => {
                if let Some(text) = part.get("text").and_then(|v| v.as_str())
                    && !text.trim().is_empty()
                {
                    out.push(text.trim().to_string());
                }
            }
            "reasoning" if include_tools => {
                if let Some(text) = part.get("text").and_then(|v| v.as_str())
                    && !text.trim().is_empty()
                {
                    out.push(text.trim().to_string());
                }
            }
            "tool" if include_tools => {
                if let Some(state) = part.get("state") {
                    if let Some(input) = state.get("input") {
                        let input_text = extract_external_text_from_json(input, include_tools);
                        if !input_text.trim().is_empty() {
                            out.push(input_text.trim().to_string());
                        }
                    }
                    if let Some(output) = state.get("output").and_then(|v| v.as_str())
                        && !output.trim().is_empty()
                    {
                        out.push(output.trim().to_string());
                    }
                }
            }
            _ => {}
        }
    }
    out.join("\n")
}

pub fn file_modified_datetime(path: &Path) -> Option<DateTime<Utc>> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .map(DateTime::<Utc>::from)
}

#[derive(Debug, Clone)]
pub struct ExternalMessageRecord {
    pub role: String,
    pub text: String,
    pub timestamp: Option<DateTime<Utc>>,
    pub id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExternalSessionRecord {
    pub source: &'static str,
    pub session_id: String,
    pub short_name: Option<String>,
    pub title: Option<String>,
    pub working_dir: Option<String>,
    pub provider_key: Option<String>,
    pub model: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub path: PathBuf,
    pub messages: Vec<ExternalMessageRecord>,
}

pub fn load_codex_external_session(
    path: &Path,
    include_tools: bool,
) -> ImportCoreResult<Option<ExternalSessionRecord>> {
    let file = File::open(path)?;
    let mut lines = BufReader::new(file).lines();
    let Some(first_line) = lines.next() else {
        return Ok(None);
    };
    let header: serde_json::Value = serde_json::from_str(&first_line?)?;
    let meta = if header.get("type").and_then(|v| v.as_str()) == Some("session_meta") {
        header.get("payload").unwrap_or(&header)
    } else {
        &header
    };
    let session_id = meta.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if session_id.is_empty() {
        return Ok(None);
    }
    let created_at = parse_rfc3339_json(meta.get("timestamp"))
        .or_else(|| parse_rfc3339_json(header.get("timestamp")))
        .unwrap_or_else(Utc::now);
    let mut updated_at = file_modified_datetime(path).unwrap_or(created_at);
    let working_dir = meta.get("cwd").and_then(|v| v.as_str()).map(str::to_string);
    let mut messages = Vec::new();
    for line in lines.map_while(|line| line.ok()) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };
        let line_type = value
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let (role, content_value) = if line_type == "message" {
            let Some(role) = value.get("role").and_then(|v| v.as_str()) else {
                continue;
            };
            (
                role,
                value.get("content").unwrap_or(&serde_json::Value::Null),
            )
        } else if line_type == "response_item" {
            let Some(payload) = value.get("payload") else {
                continue;
            };
            if payload.get("type").and_then(|v| v.as_str()) != Some("message") {
                continue;
            }
            let Some(role) = payload.get("role").and_then(|v| v.as_str()) else {
                continue;
            };
            (
                role,
                payload.get("content").unwrap_or(&serde_json::Value::Null),
            )
        } else {
            continue;
        };
        if role != "user" && role != "assistant" {
            continue;
        }
        let text = extract_external_text_from_json(content_value, include_tools);
        if text.trim().is_empty() {
            continue;
        }
        let timestamp = parse_rfc3339_json(value.get("timestamp"));
        if let Some(ts) = timestamp {
            updated_at = updated_at.max(ts);
        }
        messages.push(ExternalMessageRecord {
            role: role.to_string(),
            text,
            timestamp,
            id: value.get("id").and_then(|v| v.as_str()).map(str::to_string),
        });
    }
    Ok(Some(ExternalSessionRecord {
        source: "codex",
        session_id: session_id.to_string(),
        short_name: Some(format!("codex {}", truncate_str(session_id, 8))),
        title: Some(format!("Codex session {}", truncate_str(session_id, 8))),
        working_dir,
        provider_key: Some("openai-codex".to_string()),
        model: None,
        created_at,
        updated_at,
        path: path.to_path_buf(),
        messages,
    }))
}

pub fn load_pi_external_session(
    path: &Path,
    include_tools: bool,
) -> ImportCoreResult<Option<ExternalSessionRecord>> {
    let file = File::open(path)?;
    let mut lines = BufReader::new(file).lines();
    let Some(first_line) = lines.next() else {
        return Ok(None);
    };
    let header: serde_json::Value = serde_json::from_str(&first_line?)?;
    if header.get("type").and_then(|v| v.as_str()) != Some("session") {
        return Ok(None);
    }
    let session_id = header
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if session_id.is_empty() {
        return Ok(None);
    }
    let created_at = parse_rfc3339_json(header.get("timestamp")).unwrap_or_else(Utc::now);
    let mut updated_at = file_modified_datetime(path).unwrap_or(created_at);
    let working_dir = header
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let mut provider_key = Some("pi".to_string());
    let mut model = None;
    let mut messages = Vec::new();
    for line in lines.map_while(|line| line.ok()) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };
        if let Some(ts) = parse_rfc3339_json(value.get("timestamp")) {
            updated_at = updated_at.max(ts);
        }
        match value.get("type").and_then(|v| v.as_str()) {
            Some("model_change") => {
                provider_key = value
                    .get("provider")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .or(provider_key);
                model = value
                    .get("modelId")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .or(model);
            }
            Some("message") => {
                let Some(message) = value.get("message") else {
                    continue;
                };
                let role = message
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if role != "user" && role != "assistant" {
                    continue;
                }
                let text = extract_external_text_from_json(
                    message.get("content").unwrap_or(&serde_json::Value::Null),
                    include_tools,
                );
                if text.trim().is_empty() {
                    continue;
                }
                messages.push(ExternalMessageRecord {
                    role: role.to_string(),
                    text,
                    timestamp: parse_rfc3339_json(value.get("timestamp")),
                    id: value.get("id").and_then(|v| v.as_str()).map(str::to_string),
                });
            }
            _ => {}
        }
    }
    Ok(Some(ExternalSessionRecord {
        source: "pi",
        session_id: session_id.to_string(),
        short_name: Some(format!("pi {}", truncate_str(session_id, 8))),
        title: Some(format!("Pi session {}", truncate_str(session_id, 8))),
        working_dir,
        provider_key,
        model,
        created_at,
        updated_at,
        path: path.to_path_buf(),
        messages,
    }))
}

pub fn load_opencode_external_session(
    path: &Path,
    messages_base: &Path,
    parts_base: &Path,
    include_tools: bool,
    max_scan_sessions: usize,
) -> ImportCoreResult<Option<ExternalSessionRecord>> {
    let value: serde_json::Value = serde_json::from_reader(File::open(path)?)?;
    let session_id = value.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    if session_id.is_empty() {
        return Ok(None);
    }
    let created_at = value
        .get("time")
        .and_then(|time| time.get("created"))
        .and_then(|v| v.as_i64())
        .and_then(DateTime::<Utc>::from_timestamp_millis)
        .unwrap_or_else(Utc::now);
    let updated_at = value
        .get("time")
        .and_then(|time| time.get("updated"))
        .and_then(|v| v.as_i64())
        .and_then(DateTime::<Utc>::from_timestamp_millis)
        .or_else(|| file_modified_datetime(path))
        .unwrap_or(created_at);
    let working_dir = value
        .get("directory")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let title = value
        .get("title")
        .and_then(|v| v.as_str())
        .map(|title| truncate_title_text(title, 72))
        .unwrap_or_else(|| format!("OpenCode session {}", truncate_str(session_id, 8)));
    let mut provider_key = Some("opencode".to_string());
    let mut model = None;
    let mut messages = Vec::new();
    let messages_root = messages_base.join(session_id);
    if messages_root.exists() {
        for msg_path in collect_recent_files_recursive(&messages_root, "json", max_scan_sessions) {
            let Ok(msg_value) =
                serde_json::from_reader::<_, serde_json::Value>(File::open(&msg_path)?)
            else {
                continue;
            };
            let role = msg_value
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if role != "user" && role != "assistant" {
                continue;
            }
            if model.is_none() {
                model = msg_value
                    .get("modelID")
                    .or_else(|| msg_value.get("model").and_then(|m| m.get("modelID")))
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
            }
            provider_key = msg_value
                .get("providerID")
                .or_else(|| msg_value.get("model").and_then(|m| m.get("providerID")))
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .or(provider_key);
            let message_id = msg_value
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            // Modern OpenCode stores body text in part files keyed by message id.
            // Fall back to legacy inline content/summary for older stores.
            let mut text = message_id
                .as_deref()
                .map(|id| extract_opencode_part_text(parts_base, id, include_tools))
                .unwrap_or_default();
            if text.trim().is_empty() {
                text = msg_value
                    .get("content")
                    .or_else(|| msg_value.get("summary"))
                    .map(|value| extract_external_text_from_json(value, include_tools))
                    .unwrap_or_default();
            }
            if text.trim().is_empty() {
                continue;
            }
            messages.push(ExternalMessageRecord {
                role: role.to_string(),
                text,
                timestamp: None,
                id: message_id,
            });
        }
    }
    Ok(Some(ExternalSessionRecord {
        source: "opencode",
        session_id: session_id.to_string(),
        short_name: Some(format!("opencode {}", truncate_str(session_id, 8))),
        title: Some(title),
        working_dir,
        provider_key,
        model,
        created_at,
        updated_at,
        path: path.to_path_buf(),
        messages,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_optional_text_trims_and_drops_empty() {
        assert_eq!(
            clean_optional_text(Some("  hello  ".into())),
            Some("hello".into())
        );
        assert_eq!(clean_optional_text(Some("   ".into())), None);
        assert_eq!(clean_optional_text(None), None);
    }

    #[test]
    fn collect_recent_files_returns_empty_for_zero_limit() {
        assert!(collect_recent_files_recursive(Path::new("."), "rs", 0).is_empty());
    }

    #[test]
    fn extract_external_text_respects_include_tools() {
        let value = serde_json::json!([
            {"type": "text", "text": " hello "},
            {"type": "tool_result", "content": " tool output "}
        ]);
        assert_eq!(extract_external_text_from_json(&value, false), "hello");
        assert_eq!(
            extract_external_text_from_json(&value, true),
            "hello\ntool output"
        );
    }
}
