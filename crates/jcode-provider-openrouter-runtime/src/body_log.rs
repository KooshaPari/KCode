//! Opt-in request-body capture for provider debugging.
//!
//! When `JCODE_PROVIDER_BODY_LOG` is set to a directory path, every outgoing
//! OpenAI-compatible chat request body is dumped there as a timestamped JSON
//! file. This is the evidence tool for investigating provider-side corruption
//! (e.g. MiniMax-M3 `<function_calls>` markup leaks) where raw SSE bodies are
//! not otherwise logged.
//!
//! Disabled by default: unset or empty `JCODE_PROVIDER_BODY_LOG` writes nothing.

use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Dump the serialized request body to the body-log directory if enabled.
///
/// Reads `JCODE_PROVIDER_BODY_LOG`; unset or empty disables the dump.
/// Failures are logged at info level and never propagate: body logging must
/// not break the request.
pub fn maybe_dump_request_body(model: &str, request: &Value) {
    let Ok(dir) = std::env::var("JCODE_PROVIDER_BODY_LOG") else {
        return;
    };
    if dir.is_empty() {
        return;
    }
    dump_request_body(model, request, Some(&PathBuf::from(dir)));
}

/// Write the serialized request body when `dir` is `Some`.
/// File naming: `<unix_ms>-<model-sanitized>.json`.
fn dump_request_body(model: &str, request: &Value, dir: Option<&Path>) {
    let Some(dir) = dir else {
        return;
    };

    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let safe_model: String = model
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let path = dir.join(format!("{ms}-{safe_model}.json"));

    let write_result = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let mut f = std::fs::File::create(&path)?;
        f.write_all(
            serde_json::to_string_pretty(request)
                .unwrap_or_default()
                .as_bytes(),
        )
    })();

    match write_result {
        Ok(()) => {
            jcode_base::logging::info(&format!(
                "[openrouter] Dumped request body to {}",
                path.display()
            ));
        }
        Err(e) => {
            jcode_base::logging::info(&format!(
                "[openrouter] Failed to dump request body to {}: {e}",
                path.display()
            ));
        }
    }
}

#[cfg(test)]
mod body_log_tests {
    use super::dump_request_body;
    use serde_json::json;
    use std::path::PathBuf;

    #[test]
    fn none_dir_writes_nothing() {
        // No-op path: must not panic and must not touch the filesystem.
        dump_request_body("test-model", &json!({"messages": []}), None);
    }

    #[test]
    fn enabled_writes_timestamped_body() {
        let dir = std::env::temp_dir().join(format!("jcode-body-log-test-{}", std::process::id()));
        let request = json!({
            "model": "minimax-m3",
            "messages": [{"role": "user", "content": "hi"}]
        });
        dump_request_body("minimax-m3", &request, Some(&dir));

        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("body-log dir must be created")
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries.len(), 1, "exactly one body dump expected");
        entries.sort_by_key(|e| e.file_name());
        let name = entries[0].file_name().to_string_lossy().to_string();
        assert!(
            name.ends_with("minimax-m3.json") && name.chars().next().is_some_and(|c| c.is_ascii_digit()),
            "timestamped model name expected, got: {name}"
        );
        let written = std::fs::read_to_string(entries[0].path()).unwrap();
        assert!(written.contains("minimax-m3"));
        assert!(written.contains("Recovered") == false);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_chars_in_model_are_sanitized() {
        let dir = std::env::temp_dir().join(format!(
            "jcode-body-log-sanitize-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        dump_request_body("z-ai/glm:5.3", &json!({"model": "x"}), Some(&dir));
        let entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("body-log dir must be created")
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries.len(), 1);
        let name = entries[0].file_name().to_string_lossy().to_string();
        assert!(name.ends_with("z-ai_glm_5.3.json"), "sanitized model expected, got: {name}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
