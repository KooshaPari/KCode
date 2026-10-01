//! Opt-in request-body capture for provider debugging.
//!
//! When `JCODE_PROVIDER_BODY_LOG` is set to a directory path, every outgoing
//! OpenAI-compatible chat request body is dumped there as a timestamped JSON
//! file. This is the evidence tool for investigating provider-side corruption
//! (e.g. MiniMax-M3 `<function_calls>` markup leaks) where raw SSE bodies are
//! not otherwise logged.
//!
//! Raw captures intentionally retain prompt/tool content for diagnostics. Use a dedicated
//! private directory; Unix directories/files are restricted to 0700/0600.
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
pub fn capture_sse_stream<S>(
    stream: S,
    model: String,
) -> impl futures::Stream<Item = Result<Bytes, reqwest::Error>> + Send
where
    S: futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
{
    let dir = std::env::var_os("JCODE_PROVIDER_SSE_LOG")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    capture_sse_stream_to(stream, &model, dir.as_deref())
}

fn capture_sse_stream_to<S>(
    stream: S,
    model: &str,
    dir: Option<&Path>,
) -> impl futures::Stream<Item = Result<Bytes, reqwest::Error>> + Send + use<S>
where
    S: futures::Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static,
{
    let file = dir.and_then(|dir| match capture_file(dir, model, "-sse.txt") {
        Ok(file) => Some(file),
        Err(error) => {
            jcode_base::logging::warn(&format!("[openrouter] Failed to open SSE capture: {error}"));
            None
        }
    });

    let stream = Box::pin(stream);
    // `.fuse()`: a raw `unfold` panics if polled after it returned
    // `Ready(None)`. Fusing makes the tee return `None` forever after EOF, so
    // it cannot take down a consumer that probes for termination.
    futures::stream::unfold((stream, file), |(mut stream, mut file)| async move {
        match stream.as_mut().next().await {
            Some(item) => {
                if let (Some(f), Ok(bytes)) = (file.as_mut(), item.as_ref()) {
                    if let Err(error) = f.write_all(bytes) {
                        jcode_base::logging::warn(&format!(
                            "[openrouter] Disabling failed SSE capture: {error}"
                        ));
                        file = None;
                    }
                }
                Some((item, (stream, file)))
            }
            None => None,
        }
    })
    .fuse()
}

/// Write the serialized request body when `dir` is `Some`.
/// File naming: `<unix_ms>-<model-sanitized>.json`.
fn dump_request_body(model: &str, request: &Value, dir: Option<&Path>) {
    let Some(dir) = dir else {
        return;
    };

    let result = (|| -> std::io::Result<()> {
        let mut file = capture_file(dir, model, ".json")?;
        serde_json::to_writer_pretty(&mut file, request)?;
        file.flush()
    })();
    if let Err(error) = result {
        jcode_base::logging::warn(&format!(
            "[openrouter] Failed to dump request body: {error}"
        ));
    }
}

fn capture_file(dir: &Path, model: &str, suffix: &str) -> std::io::Result<std::fs::File> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if dir.metadata()?.permissions().mode() & 0o077 != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "capture requires a private directory; existing permissions were preserved",
            ));
        }
    }
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let safe_model: String = model
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let path = dir.join(format!(
        "{ms}-{}-{safe_model}{suffix}",
        uuid::Uuid::new_v4()
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

#[cfg(test)]
mod sse_log_tests {
    use super::capture_sse_stream_to;
    use futures::StreamExt;

    #[tokio::test]
    async fn disabled_by_default_passes_chunks_through() {
        // No JCODE_PROVIDER_SSE_LOG: chunks pass through unchanged.
        let chunks: Vec<Result<bytes::Bytes, reqwest::Error>> = vec![
            Ok(bytes::Bytes::from_static(b"data: hello\n\n")),
            Ok(bytes::Bytes::from_static(b"data: world\n\n")),
        ];
        let mut out: Vec<u8> = Vec::new();
        let mut s = Box::pin(capture_sse_stream_to(
            futures::stream::iter(chunks),
            "minimax-m3",
            None,
        ));
        while let Some(c) = s.next().await {
            out.extend_from_slice(&c.expect("no error expected"));
        }
        assert_eq!(out, b"data: hello\n\ndata: world\n\n");
    }

    #[tokio::test]
    async fn enabled_captures_chunks_to_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let dir = temp.path();
        let chunks: Vec<Result<bytes::Bytes, reqwest::Error>> = vec![
            Ok(bytes::Bytes::from_static(b"data: hello\n\n")),
            Ok(bytes::Bytes::from_static(b"data: world\n\n")),
        ];
        let mut s = Box::pin(capture_sse_stream_to(
            futures::stream::iter(chunks),
            "minimax-m3",
            Some(dir),
        ));
        while let Some(_c) = s.next().await {}
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("sse capture dir must be created")
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries.len(), 1, "exactly one capture expected");
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
        let temp = tempfile::TempDir::new().unwrap();
        let dir = temp.path();
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
            name.ends_with("minimax-m3.json")
                && name.chars().next().is_some_and(|c| c.is_ascii_digit()),
            "timestamped model name expected, got: {name}"
        );
        let written = std::fs::read_to_string(entries[0].path()).unwrap();
        assert!(written.contains("minimax-m3"));
        assert!(!written.contains("Recovered"));
    }

    #[test]
    fn invalid_chars_in_model_are_sanitized() {
        let temp = tempfile::TempDir::new().unwrap();
        let dir = temp.path();
        dump_request_body("z-ai/glm:5.3", &json!({"model": "x"}), Some(&dir));
        let entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("body-log dir must be created")
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries.len(), 1);
        let name = entries[0].file_name().to_string_lossy().to_string();
        assert!(
            name.ends_with("z-ai_glm_5.3.json"),
            "sanitized model expected, got: {name}"
        );
    }
}

#[cfg(test)]
mod capture_file_tests {
    use super::*;
    #[test]
    fn captures_do_not_overwrite_each_other() {
        let dir = tempfile::TempDir::new().unwrap();
        for _ in 0..20 {
            dump_request_body(
                "model",
                &serde_json::json!({"messages": []}),
                Some(dir.path()),
            );
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 20);
    }
    #[cfg(unix)]
    #[test]
    fn captures_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let file = capture_file(dir.path(), "model", ".json").unwrap();
        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(
            dir.path().metadata().unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    #[cfg(unix)]
    #[test]
    fn shared_directory_permissions_are_preserved() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(capture_file(dir.path(), "model", ".json").is_err());
        assert_eq!(
            dir.path().metadata().unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
    #[tokio::test]
    async fn invalid_capture_destination_preserves_stream() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let stream = futures::stream::iter(vec![Ok(Bytes::from_static(b"data"))]);
        let mut captured = Box::pin(capture_sse_stream_to(stream, "model", Some(file.path())));
        assert_eq!(
            captured.next().await.unwrap().unwrap(),
            Bytes::from_static(b"data")
        );
        assert!(captured.next().await.is_none());
        assert!(captured.next().await.is_none());
    }
}
