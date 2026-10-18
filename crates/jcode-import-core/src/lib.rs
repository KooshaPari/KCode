//! External session import core.
//!
//! Owns the loaders, data model, and shared helpers used to surface external
//! agent transcripts (Claude Code, Codex, Pi, OpenCode, Cursor) inside the
//! resume picker and import UI.
//!
//! The crate is split by source/topic to keep individual files under the
//! panic-budget cap:
//!
//! - [`claude_code`] — Claude Code on-disk data model, JSONL loaders, session
//!   metadata, tool_result flattening.
//! - [`cursor`] — Cursor agent transcripts, project-dir decoding,
//!   subagent-detection.
//! - [`sessions`] — Codex, Pi, and OpenCode loaders, the shared
//!   `ExternalMessageRecord` / `ExternalSessionRecord` types, file walking
//!   helpers, and `extract_external_text_from_json`.
//! - [`text_utils`] — title truncation, JSON value text extraction, and the
//!   `imported_*_session_id` namespace-prefix helpers.
//! - [`repo_ranking`] — repository ranking heuristics (unchanged, was
//!   pre-existing).
//!
//! Every public item is re-exported at the crate root so downstream consumers
//! can keep their existing `use jcode_import_core::X` paths.

pub mod claude_code;
pub mod cursor;
pub mod repo_ranking;
pub mod sessions;
pub mod text_utils;

pub use claude_code::*;
pub use cursor::*;
pub use sessions::*;
pub use text_utils::*;

/// Truncate a string at a valid UTF-8 character boundary.
///
/// Returns a slice of at most `max_bytes` bytes, ending at a valid char
/// boundary so it never panics on multibyte input. This mirrors
/// `jcode_core::util::truncate_str`, duplicated here to keep this leaf crate
/// free of the heavier `jcode-core` dependency.
pub(crate) fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_str_respects_utf8_boundaries() {
        // A 4-byte char split mid-codepoint must round down, not panic.
        let s = "a🙂b";
        assert_eq!(truncate_str(s, 1), "a");
        assert_eq!(truncate_str(s, 2), "a");
        assert_eq!(truncate_str(s, 3), "a");
        assert_eq!(truncate_str(s, 5), "a🙂");
        assert_eq!(truncate_str(s, 100), s);
    }
}
