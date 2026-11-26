//! Text and ID helpers shared by the other import-core modules.
//!
//! Owns the small text-shape utilities (title truncation, JSON value text
//! extraction, environment-noise filtering for session titles) and the
//! `imported_*_session_id` namespace-prefix helpers used when an external
//! session is materialized inside the local store.
//!
//! `lib.rs` re-exports every public item at the crate root so downstream
//! consumers can keep their existing `use jcode_import_core::X` paths.

use sha2::{Digest, Sha256};

pub fn truncate_title_text(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        trimmed.to_string()
    } else {
        format!(
            "{}…",
            trimmed
                .chars()
                .take(max_chars.saturating_sub(1))
                .collect::<String>()
        )
    }
}

pub fn extract_text_from_json_value(value: &serde_json::Value) -> String {
    fn visit(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(text) if !text.trim().is_empty() => {
                out.push(text.trim().to_string());
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    visit(item, out);
                }
            }
            serde_json::Value::Object(map) => {
                if let Some(text) = map.get("text").and_then(|v| v.as_str()) {
                    if !text.trim().is_empty() {
                        out.push(text.trim().to_string());
                    }
                    return;
                }
                if let Some(text) = map.get("title").and_then(|v| v.as_str())
                    && !text.trim().is_empty()
                {
                    out.push(text.trim().to_string());
                }
                for (key, nested) in map {
                    if key == "type" || key == "title" {
                        continue;
                    }
                    visit(nested, out);
                }
            }
            _ => {}
        }
    }

    let mut out = Vec::new();
    visit(value, &mut out);
    out.join(" ")
}

pub fn truncate_title(s: &str) -> String {
    let trimmed = s.lines().next().unwrap_or_default().trim();
    const MAX_CHARS: usize = 80;
    if trimmed.chars().count() <= MAX_CHARS {
        trimmed.to_string()
    } else {
        let mut out = trimmed
            .chars()
            .take(MAX_CHARS.saturating_sub(3))
            .collect::<String>();
        out.push_str("...");
        out
    }
}

pub fn codex_title_candidate(text: &str) -> Option<String> {
    let cleaned = text.replace("<environment_context>", "");
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return None;
    }
    if cleaned.starts_with("# AGENTS.md instructions")
        || cleaned.starts_with("<permissions instructions>")
        || cleaned.contains("\n<INSTRUCTIONS>")
    {
        return None;
    }
    Some(truncate_title(cleaned))
}

pub fn imported_claude_code_session_id(session_id: &str) -> String {
    format!("imported_cc_{}", session_id)
}

pub fn imported_codex_session_id(session_id: &str) -> String {
    format!("imported_codex_{}", session_id)
}

pub fn imported_opencode_session_id(session_id: &str) -> String {
    format!("imported_opencode_{}", session_id)
}

pub fn imported_pi_session_id(session_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(session_path.as_bytes());
    let digest = hasher.finalize();
    format!("imported_pi_{}", hex::encode(&digest[..8]))
}

pub fn imported_cursor_session_id(session_id: &str) -> String {
    format!("imported_cursor_{}", session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_pi_id_is_stable_and_prefixed() {
        assert_eq!(
            imported_pi_session_id("/tmp/session"),
            imported_pi_session_id("/tmp/session")
        );
        assert!(imported_pi_session_id("/tmp/session").starts_with("imported_pi_"));
    }

    #[test]
    fn extract_text_from_json_collects_nested_text() {
        let value = serde_json::json!({
            "type": "message",
            "content": [
                {"type": "text", "text": " hello "},
                {"title": "ignored title", "other": " world "}
            ]
        });
        assert_eq!(
            extract_text_from_json_value(&value),
            "hello ignored title world"
        );
    }

    #[test]
    fn codex_title_candidate_filters_environment_noise() {
        assert_eq!(
            codex_title_candidate("<environment_context> Build feature"),
            Some("Build feature".into())
        );
        assert_eq!(
            codex_title_candidate(
                "# AGENTS.md instructions
Do x"
            ),
            None
        );
    }
}
