//! Opt-in request-body capture for provider debugging.
//!
//! When `JCODE_PROVIDER_BODY_LOG` is set to a directory path, every outgoing
//! OpenAI-compatible chat request body is dumped there as a timestamped JSON
//! file. This is the evidence tool for investigating provider-side corruption
//! (e.g. MiniMax-M3 `<function_calls>` markup leaks) where raw SSE bodies are
//! not otherwise logged.
//!
//! Disabled by default: unset or empty `JCODE_PROVIDER_BODY_LOG` writes nothing.

use bytes::Bytes;
use futures::StreamExt;
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

/// Tee adapter: appends every raw SSE chunk to the capture file when enabled.
///
/// When `JCODE_PROVIDER_SSE_LOG` is set to a directory, the raw response byte
/// stream is wrapped so each chunk is appended to
/// `<dir>/<unix_ms>-<model>-sse.txt` before being parsed. This captures the
/// model OUTPUT side (where the MiniMax-M3 markup corruption appears).
///
/// Disabled by default: unset or empty `JCODE_PROVIDER_SSE_LOG` passes chunks
/// through untouched. Failures never propagate: capture must not break the
/// stream.
pub fn capture_sse_stream<S>(stream: S, model: String) -> impl futures::Stream<Item = Result<Bytes, reqwest::Error>> + Send
where
    S: futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
{
    let capture_path = match std::env::var("JCODE_PROVIDER_SSE_LOG") {
        Ok(dir) if !dir.is_empty() => {
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
            Some(PathBuf::from(dir).join(format!("{ms}-{safe_model}-sse.txt")))
        }
        _ => None,
    };
    let file = capture_path.as_ref().and_then(|p| {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::OpenOptions::new().create(true).append(true).open(p).ok()
    });
    if let Some(p) = capture_path.as_ref() {
        if file.is_some() {
            jcode_base::logging::info(&format!("[openrouter] Capturing raw SSE to {}", p.display()));
        } else {
            jcode_base::logging::info(&format!(
                "[openrouter] Failed to open SSE capture file {}",
                p.display()
            ));
        }
    }

    let stream = Box::pin(stream);
    futures::stream::unfold(
        (stream, file),
        |(mut stream, file)| async move {
            match stream.as_mut().next().await {
                Some(item) => {
                    if let (Some(f), Ok(bytes)) = (file.as_ref(), item.as_ref()) {
                        let mut f = f;
                        let _ = std::io::Write::write_all(&mut f, bytes);
                    }
                    Some((item, (stream, file)))
                }
                None => None,
            }
        },
    )
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
mod sse_log_tests {
    use super::capture_sse_stream;
    use futures::StreamExt;

    #[tokio::test]
    async fn disabled_by_default_passes_chunks_through() {
        // No JCODE_PROVIDER_SSE_LOG: chunks pass through unchanged.
        let chunks: Vec<Result<bytes::Bytes, reqwest::Error>> = vec![
            Ok(bytes::Bytes::from_static(b"data: hello\n\n")),
            Ok(bytes::Bytes::from_static(b"data: world\n\n")),
        ];
        let mut out: Vec<u8> = Vec::new();
        let mut s = Box::pin(capture_sse_stream(futures::stream::iter(chunks), "minimax-m3".to_string()));
        while let Some(c) = s.next().await {
            out.extend_from_slice(&c.expect("no error expected"));
        }
        assert_eq!(out, b"data: hello\n\ndata: world\n\n");
    }

    #[tokio::test]
    async fn enabled_captures_chunks_to_file() {
        let dir = std::env::temp_dir().join(format!(
            "jcode-sse-log-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        // SAFETY: test-only env var, restored immediately; the single-threaded
        // current-thread runtime in #[tokio::test] avoids cross-test races.
        #[allow(unsafe_op_in_unsafe_fn)]
        unsafe {
            std::env::set_var("JCODE_PROVIDER_SSE_LOG", &dir);
        }
        let chunks: Vec<Result<bytes::Bytes, reqwest::Error>> = vec![
            Ok(bytes::Bytes::from_static(b"data: hello\n\n")),
            Ok(bytes::Bytes::from_static(b"data: world\n\n")),
        ];
        let mut s = Box::pin(capture_sse_stream(futures::stream::iter(chunks), "minimax-m3".to_string()));
        while let Some(_c) = s.next().await {}
        #[allow(unsafe_op_in_unsafe_fn)]
        unsafe {
            std::env::remove_var("JCODE_PROVIDER_SSE_LOG");
        }

        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("sse capture dir must be created")
            .filter_map(|e| e.ok())
            .collect();
        assert!(
            !entries.is_empty(),
            "at least one sse capture file expected"
        );
        // A concurrent run of this binary can share the temp dir when the
        // pid+nanos name collides; only the files written by *this* invocation
        // must contain our chunks, so assert on the newest entry rather than
        // the entry count.
        entries.sort_by_key(|e| e.file_name());
        let name = entries
            .last()
            .expect("capture entry")
            .file_name()
            .to_string_lossy()
            .to_string();
        assert!(
            name.ends_with("minimax-m3-sse.txt"),
            "sse capture name expected, got: {name}"
        );
        let written =
            std::fs::read_to_string(entries.last().expect("capture entry").path()).unwrap();
        assert!(written.contains("data: hello"));
        assert!(written.contains("data: world"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod body_log_tests {
    use super::dump_request_body;
    use serde_json::json;

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
        assert!(!written.contains("Recovered"));
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
