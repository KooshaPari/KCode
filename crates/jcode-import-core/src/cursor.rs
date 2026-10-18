//! Cursor agent session import.
//!
//! Owns the Cursor agent on-disk data model: the project directory layout
//! (working dir encoded with hyphens), the `agent-transcripts/<uuid>/<uuid>.jsonl`
//! transcript format, the subagent detection heuristic, and the loader that
//! turns a transcript into an `ExternalSessionRecord` for the import UI.
//!
//! `lib.rs` re-exports every public item at the crate root so downstream
//! consumers can keep their existing `use jcode_import_core::X` paths.

use chrono::Utc;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::sessions::{
    ExternalMessageRecord, ExternalSessionRecord, ImportCoreResult, extract_external_text_from_json,
    file_modified_datetime,
};
use crate::text_utils::truncate_title_text;
use crate::truncate_str;

/// Decode a Cursor transcript file's session id.
///
/// Cursor agent stores transcripts at
/// `~/.cursor/projects/<project>/agent-transcripts/<session-id>/<session-id>.jsonl`,
/// so the file stem is the session UUID. Fall back to a hash of the path when the
/// stem is not a UUID (e.g. unexpected layouts).
pub fn cursor_session_id_from_path(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();
    if looks_like_uuid(&stem) {
        return stem;
    }
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    hex::encode(&digest[..8])
}

fn looks_like_uuid(s: &str) -> bool {
    let groups = [8usize, 4, 4, 4, 12];
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != groups.len() {
        return false;
    }
    parts
        .iter()
        .zip(groups.iter())
        .all(|(part, &len)| part.len() == len && part.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Whether a Cursor transcript path is a *subagent* transcript rather than a
/// top-level session. Cursor nests subagent runs at
/// `.../agent-transcripts/<parent>/subagents/<child>.jsonl`; these are not
/// independently resumable sessions, so the resume picker skips them (they would
/// otherwise appear as duplicate/stray rows alongside the parent session).
pub fn is_cursor_subagent_transcript(path: &Path) -> bool {
    path.parent()
        .and_then(|dir| dir.file_name())
        .and_then(|name| name.to_str())
        .map(|name| name == "subagents")
        .unwrap_or(false)
}

/// Best-effort decode of a Cursor project directory name back into an absolute
/// path. Cursor encodes the working directory by replacing `/` with `-`, e.g.
/// `/Users/alex/Repo` -> `Users-alex-Repo`. Because real path segments can also
/// contain hyphens, we greedily walk segment boundaries and prefer prefixes that
/// exist on disk, always returning a decoded absolute path even when the final
/// directory no longer exists.
pub fn cursor_cwd_from_project_dir(project_name: &str) -> Option<String> {
    if project_name.is_empty() || project_name == "projects" || project_name == "empty-window" {
        return None;
    }
    let segments: Vec<&str> = project_name.split('-').collect();
    if segments.is_empty() {
        return None;
    }
    let mut resolved_prefix = String::new();
    let mut current = segments[0].to_string();
    let mut i = 1;
    while i < segments.len() {
        let candidate = format!("{resolved_prefix}/{current}");
        if Path::new(&candidate).is_dir() {
            resolved_prefix = candidate;
            current = segments[i].to_string();
        } else {
            current = format!("{current}-{}", segments[i]);
        }
        i += 1;
    }
    let best_effort = format!("{resolved_prefix}/{current}");
    Some(best_effort)
}

/// Infer the Cursor working directory from a transcript path by walking up to the
/// `<project>/agent-transcripts/` boundary and decoding the project dir name.
pub fn cursor_cwd_from_transcript_path(path: &Path) -> Option<String> {
    let mut components: Vec<String> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    let idx = components.iter().position(|c| c == "agent-transcripts")?;
    if idx == 0 {
        return None;
    }
    let project = std::mem::take(&mut components[idx - 1]);
    cursor_cwd_from_project_dir(&project)
}

/// Load a Cursor agent transcript into an [`ExternalSessionRecord`].
///
/// Cursor transcripts are JSONL with one object per line; each line has `role`
/// at the top level and an Anthropic-style `message.content[]` array of blocks
/// (`text`, `thinking`, `tool_use`, `tool_result`). There are no per-line
/// timestamps or model hints in the transcript, so created/updated fall back to
/// the file mtime and the working dir is inferred from the project dir name.
pub fn load_cursor_external_session(
    path: &Path,
    include_tools: bool,
) -> ImportCoreResult<Option<ExternalSessionRecord>> {
    // Skip Cursor subagent transcripts: they are nested runs of a parent session,
    // not independently resumable/searchable top-level sessions.
    if is_cursor_subagent_transcript(path) {
        return Ok(None);
    }
    let file = File::open(path)?;
    let mut messages = Vec::new();
    for line in BufReader::new(file).lines().map_while(|line| line.ok()) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        let role = match value
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
        {
            "user" | "human" => "user",
            "assistant" | "model" => "assistant",
            _ => continue,
        };
        let content = value
            .get("message")
            .and_then(|message| message.get("content"))
            .or_else(|| value.get("content"))
            .unwrap_or(&serde_json::Value::Null);
        let text = extract_external_text_from_json(content, include_tools);
        if text.trim().is_empty() {
            continue;
        }
        messages.push(ExternalMessageRecord {
            role: role.to_string(),
            text,
            timestamp: None,
            id: None,
        });
    }
    if messages.is_empty() {
        return Ok(None);
    }
    let session_id = cursor_session_id_from_path(path);
    let created_at = file_modified_datetime(path).unwrap_or_else(Utc::now);
    let updated_at = created_at;
    let working_dir = cursor_cwd_from_transcript_path(path);
    let title = messages
        .iter()
        .find(|message| message.role == "user")
        .map(|message| truncate_title_text(&message.text, 72))
        .unwrap_or_else(|| format!("Cursor session {}", truncate_str(&session_id, 8)));
    Ok(Some(ExternalSessionRecord {
        source: "cursor",
        session_id: session_id.clone(),
        short_name: Some(format!("cursor {}", truncate_str(&session_id, 8))),
        title: Some(title),
        working_dir,
        provider_key: Some("cursor".to_string()),
        model: None,
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
    fn cursor_session_id_from_path_uses_uuid_stem() {
        let path = Path::new(
            "/home/u/.cursor/projects/demo/agent-transcripts/\
11111111-2222-3333-4444-555555555555/11111111-2222-3333-4444-555555555555.jsonl",
        );
        assert_eq!(
            cursor_session_id_from_path(path),
            "11111111-2222-3333-4444-555555555555"
        );
        // Non-UUID stems fall back to a stable hash (hex, 16 chars).
        let other = Path::new("/tmp/not-a-uuid.jsonl");
        let id = cursor_session_id_from_path(other);
        assert_eq!(id.len(), 16);
        assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn cursor_cwd_decodes_project_dir_with_hyphenated_segment() {
        // The greedy decoder commits prefixes that exist on disk and rejoins the
        // rest with literal hyphens. With no real dirs it returns the all-slash
        // best-effort decode of the first segment plus a hyphen-joined tail.
        let decoded = cursor_cwd_from_project_dir("tmp-cursor-demo").unwrap();
        assert!(
            decoded.starts_with('/'),
            "decoded path must be absolute: {decoded}"
        );
        assert!(decoded.contains("tmp"));
        assert_eq!(cursor_cwd_from_project_dir("projects"), None);
        assert_eq!(cursor_cwd_from_project_dir("empty-window"), None);
    }

    #[test]
    fn cursor_cwd_from_transcript_path_walks_to_project_dir() {
        let path =
            Path::new("/home/u/.cursor/projects/Users-alex-Repo/agent-transcripts/abc/abc.jsonl");
        let cwd = cursor_cwd_from_transcript_path(path).unwrap();
        assert!(cwd.starts_with('/'));
        assert!(cwd.contains("Users"));
    }

    #[test]
    fn cursor_subagent_transcripts_are_detected() {
        let subagent = Path::new(
            "/home/u/.cursor/projects/demo/agent-transcripts/parent/subagents/child.jsonl",
        );
        assert!(is_cursor_subagent_transcript(subagent));
        let top_level =
            Path::new("/home/u/.cursor/projects/demo/agent-transcripts/parent/parent.jsonl");
        assert!(!is_cursor_subagent_transcript(top_level));
    }

    #[test]
    fn load_cursor_external_session_skips_subagent_transcripts() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!(
            "kcode-cursor-subagent-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let sub_dir = dir.join("projects/demo/agent-transcripts/parent/subagents");
        std::fs::create_dir_all(&sub_dir).unwrap();
        let path = sub_dir.join("child.jsonl");
        let mut file = File::create(&path).unwrap();
        writeln!(
            file,
            "{{\"role\":\"user\",\"message\":{{\"content\":[{{\"type\":\"text\",\"text\":\"subagent work\"}}]}}}}"
        )
        .unwrap();
        drop(file);
        assert!(
            load_cursor_external_session(&path, false)
                .unwrap()
                .is_none(),
            "subagent transcripts should be skipped"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_cursor_external_session_parses_content_blocks() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!(
            "kcode-cursor-loader-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let session_dir = dir.join("projects/demo/agent-transcripts/abc");
        std::fs::create_dir_all(&session_dir).unwrap();
        let path = session_dir.join("abc.jsonl");
        let mut file = File::create(&path).unwrap();
        writeln!(
            file,
            "{{\"role\":\"user\",\"message\":{{\"content\":[{{\"type\":\"text\",\"text\":\"hello cursor\"}}]}}}}"
        )
        .unwrap();
        writeln!(
            file,
            "{{\"role\":\"assistant\",\"message\":{{\"content\":[{{\"type\":\"text\",\"text\":\"hi there\"}}]}}}}"
        )
        .unwrap();
        drop(file);

        let record = load_cursor_external_session(&path, false)
            .unwrap()
            .expect("cursor record");
        assert_eq!(record.source, "cursor");
        assert_eq!(record.provider_key.as_deref(), Some("cursor"));
        assert_eq!(record.messages.len(), 2);
        assert_eq!(record.messages[0].role, "user");
        assert!(record.messages[0].text.contains("hello cursor"));
        assert_eq!(record.title.as_deref(), Some("hello cursor"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
