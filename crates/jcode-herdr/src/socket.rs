//! HERDR Unix socket communication.
//!
//! Two runtimes speak JSON over the same socket plumbing:
//!
//! - **Legacy HERDR**: newline-delimited JSON, no handshake. Used for
//!   `pane.report_agent`, `pane.report_agent_session`, and
//!   `pane.release_agent`.
//! - **herdr-core (HC)**: JSON-RPC 2.0, one message per line, with a
//!   mandatory `hello` handshake that advertises the protocol generation,
//!   core identity, method list and features.
//!
//! Both are Unix-only (they rely on a Unix-domain socket). On Windows
//! callers receive an explicit "unsupported on this platform" error so
//! the rest of the workspace still compiles — purely a build fix; no
//! behavior change on Unix.

#[cfg(not(unix))]
use anyhow::anyhow;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
#[cfg(unix)]
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
#[cfg(unix)]
use tokio::net::UnixStream;

/// Protocol generation this client speaks. The HC schema's `info` block
/// states protocol generation 1, and `hello` rejects anything else with
/// `data.kind = unsupported_generation`.
pub const HC_PROTOCOL_GENERATION: u32 = 1;

/// Send a JSON-RPC-style request to HERDR's socket and return the
/// parsed response. Each request is one newline-terminated JSON line;
/// the response is the first line from HERDR.
#[cfg(unix)]
pub async fn send_request(socket_path: &Path, request: &Value) -> Result<Value> {
    let mut stream = UnixStream::connect(socket_path).await.with_context(|| {
        format!(
            "failed to connect to HERDR socket at {}",
            socket_path.display()
        )
    })?;
    send_line(&mut stream, request).await
}

/// Windows stub. HERDR only ships on Unix; surface a clear error so
/// the workspace still compiles.
#[cfg(not(unix))]
pub async fn send_request(_socket_path: &Path, _request: &Value) -> Result<Value> {
    Err(anyhow!(
        "HERDR is not supported on this platform (Unix-only)"
    ))
}

/// Write one newline-terminated JSON message and read one JSON reply
/// line. Shared by the legacy and HC transports, and generic over the
/// stream so tests can drive it with an in-memory duplex.
#[cfg(unix)]
pub async fn send_line<S>(stream: &mut S, request: &Value) -> Result<Value>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut line = serde_json::to_string(request).context("failed to serialize HERDR request")?;
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .await
        .context("failed to write to HERDR socket")?;
    stream.flush().await?;

    read_json_line(stream).await
}

/// Read a single newline-terminated JSON message.
#[cfg(unix)]
pub async fn read_json_line<S>(stream: &mut S) -> Result<Value>
where
    S: AsyncRead + Unpin,
{
    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    reader
        .read_line(&mut response_line)
        .await
        .context("failed to read HERDR response")?;

    if response_line.is_empty() {
        anyhow::bail!("HERDR returned empty response");
    }

    serde_json::from_str(&response_line)
        .with_context(|| format!("failed to parse HERDR response: {response_line}"))
}

/// Fire-and-forget: send a request and ignore the response.
/// Used for non-critical state reports where latency matters more
/// than confirmation.
pub async fn send_fire_and_forget(socket_path: &Path, request: &Value) {
    #[cfg(unix)]
    {
        if let Err(e) = send_request(socket_path, request).await {
            tracing::debug!("HERDR fire-and-forget failed: {e}");
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (socket_path, request);
    }
}

// ---------------------------------------------------------------------------
// HC (herdr-core) transport
// ---------------------------------------------------------------------------

/// Build the `hello` request. `params` is by-name per the HC schema; the
/// only field this client needs is `generation`. `framed` is left at its
/// schema default (`false`), so the connection stays newline-delimited.
pub fn hello_request(id: u64) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "hello",
        "params": { "generation": HC_PROTOCOL_GENERATION }
    })
}

/// A stable HC error kind (`error.data.kind`). The schema enumerates the
/// kinds per method; an absent or non-string `kind` is reported as `None`
/// rather than invented.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcError {
    /// JSON-RPC error code.
    pub code: i64,
    /// Stable `data.kind`, when the server supplied one.
    pub kind: Option<String>,
    /// Human-readable message.
    pub message: String,
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            Some(kind) => write!(f, "HC error {} ({kind}): {}", self.code, self.message),
            None => write!(f, "HC error {}: {}", self.code, self.message),
        }
    }
}

impl std::error::Error for RpcError {}

impl RpcError {
    /// The stable `data.kind`, if present.
    pub fn kind(&self) -> Option<&str> {
        self.kind.as_deref()
    }
}

/// Extract a JSON-RPC error from a response, if it carries one.
pub fn rpc_error(response: &Value) -> Option<RpcError> {
    let error = response.get("error")?;
    let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let kind = error
        .get("data")
        .and_then(|data| data.get("kind"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(RpcError {
        code,
        kind,
        message,
    })
}

/// The `result` member of a JSON-RPC response.
pub fn response_result(response: &Value) -> Result<&Value> {
    response
        .get("result")
        .ok_or_else(|| anyhow::anyhow!("HC response carried no result: {response}"))
}

/// An error returned by an HC call, preserving `data.kind`.
#[derive(Debug)]
pub struct HcCallError {
    /// The underlying stable-kind error.
    pub error: RpcError,
}

impl fmt::Display for HcCallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for HcCallError {}

impl HcCallError {
    /// The stable `data.kind`, if the server supplied one.
    pub fn kind(&self) -> Option<&str> {
        self.error.kind()
    }
}

/// The parsed `hello` result. Only the schema's required members are kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloOutcome {
    /// Protocol generation negotiated by the core.
    pub generation: u32,
    /// Core version string.
    pub version: String,
    /// Permanent random core id.
    pub core_id: String,
    /// Core display name.
    pub core_name: String,
    /// Whether the connection switched to framed mode after the reply.
    pub framed: bool,
    /// Methods the core advertises.
    pub methods: Vec<String>,
    /// Features the core advertises.
    pub features: Vec<String>,
}

impl HelloOutcome {
    /// Parse a `hello` result object.
    pub fn from_result(result: &Value) -> Result<Self> {
        let generation = result
            .get("generation")
            .and_then(Value::as_u64)
            .context("HC hello result missing generation")? as u32;
        let version = required_str(result, "version")?;
        let core = result
            .get("core")
            .context("HC hello result missing core")?;
        let core_id = required_str(core, "id")?;
        let core_name = required_str(core, "name")?;
        let framed = result
            .get("framed")
            .and_then(Value::as_bool)
            .context("HC hello result missing framed")?;
        let methods = string_array(result, "methods")?;
        let features = string_array(result, "features")?;
        Ok(Self {
            generation,
            version,
            core_id,
            core_name,
            framed,
            methods,
            features,
        })
    }

    /// Whether the core advertises a method.
    pub fn advertises(&self, method: &str) -> bool {
        self.methods.iter().any(|m| m == method)
    }
}

fn required_str(value: &Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .with_context(|| format!("HC result missing string field {key}"))
}

#[cfg(unix)]
fn string_array(value: &Value, key: &str) -> Result<Vec<String>> {
    let array = value
        .get(key)
        .and_then(Value::as_array)
        .with_context(|| format!("HC result missing array field {key}"))?;
    array
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .with_context(|| format!("HC result field {key} contained a non-string"))
        })
        .collect()
}

#[cfg(not(unix))]
fn string_array(value: &Value, key: &str) -> Result<Vec<String>> {
    let _ = value;
    let _ = key;
    anyhow::bail!("HC is not supported on this platform (Unix-only)")
}

/// Parameters for `terminal.get` (by-name).
pub fn terminal_get_params(terminal: &str, caller: Option<&str>) -> Value {
    let mut params = json!({ "terminal": terminal });
    if let Some(caller) = caller {
        params["caller"] = Value::String(caller.to_string());
    }
    params
}

/// Parameters for `terminal.send` (by-name). `force` and `if_status` map
/// to the schema's `force` and `if` fields.
pub fn terminal_send_params(
    terminal: &str,
    text: &str,
    caller: Option<&str>,
    force: bool,
    if_status: Option<&str>,
) -> Value {
    let mut params = json!({
        "terminal": terminal,
        "text": text,
        "force": force,
    });
    if let Some(caller) = caller {
        params["caller"] = Value::String(caller.to_string());
    }
    if let Some(status) = if_status {
        params["if"] = Value::String(status.to_string());
    }
    params
}

/// Parameters for `terminal.list` (by-name).
pub fn terminal_list_params(all: bool, labels: &BTreeMap<String, String>) -> Value {
    json!({ "all": all, "labels": labels })
}

/// One terminal, limited to the schema's required members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Terminal {
    /// `t7`: this terminal forever on this core.
    pub address: String,
    /// Labels carried by the terminal.
    pub labels: BTreeMap<String, String>,
    /// Argv the terminal was started with.
    pub argv: Vec<String>,
    /// Working directory.
    pub cwd: String,
    /// `"live"` or `"finished"`.
    pub state: String,
    /// Creation timestamp in milliseconds.
    pub created_ms: u64,
    /// `"exact"`, or `"inexact"` after a core crash.
    pub screen: String,
    /// Terminal columns.
    pub cols: u16,
    /// Terminal rows.
    pub rows: u16,
}

impl Terminal {
    /// Parse one `Terminal` object.
    pub fn from_value(value: &Value) -> Result<Self> {
        let labels = value
            .get("labels")
            .and_then(Value::as_object)
            .context("HC terminal missing labels")?
            .iter()
            .map(|(k, v)| {
                v.as_str()
                    .map(|v| (k.clone(), v.to_string()))
                    .with_context(|| format!("HC terminal label {k} is not a string"))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let argv = value
            .get("argv")
            .and_then(Value::as_array)
            .context("HC terminal missing argv")?
            .iter()
            .map(|arg| {
                arg.as_str()
                    .map(str::to_string)
                    .context("HC terminal argv contained a non-string")
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            address: required_str(value, "address")?,
            labels,
            argv,
            cwd: required_str(value, "cwd")?,
            state: required_str(value, "state")?,
            created_ms: value
                .get("created_ms")
                .and_then(Value::as_u64)
                .context("HC terminal missing created_ms")?,
            screen: required_str(value, "screen")?,
            cols: required_u16(value, "cols")?,
            rows: required_u16(value, "rows")?,
        })
    }
}

fn required_u16(value: &Value, key: &str) -> Result<u16> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .with_context(|| format!("HC result missing integer field {key}"))
        .and_then(|n| u16::try_from(n).with_context(|| format!("HC field {key} out of range")))
}

/// The part of `terminal.list`'s result this client consumes. `machines`
/// is deliberately not modelled: nothing here needs it, so it is not
/// guessed at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalList {
    /// This machine's terminals, then each connected machine's.
    pub terminals: Vec<Terminal>,
    /// Finished terminals left out (see `all`).
    pub hidden_finished: u64,
}

impl TerminalList {
    /// Parse a `terminal.list` result object.
    pub fn from_result(result: &Value) -> Result<Self> {
        let terminals = result
            .get("terminals")
            .and_then(Value::as_array)
            .context("HC terminal.list missing terminals")?
            .iter()
            .map(Terminal::from_value)
            .collect::<Result<Vec<_>>>()?;
        let hidden_finished = result
            .get("hidden_finished")
            .and_then(Value::as_u64)
            .context("HC terminal.list missing hidden_finished")?;
        Ok(Self {
            terminals,
            hidden_finished,
        })
    }
}

/// The required part of `terminal.send`'s result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendOutcome {
    /// The turn when the text went in.
    pub turn: u64,
    /// Whether it went `working` within a few seconds; `None` when the
    /// program does not report status.
    pub started: Option<bool>,
    /// The program's status word when the text went in.
    pub status: Option<String>,
}

impl SendOutcome {
    /// Parse a `terminal.send` result object.
    pub fn from_result(result: &Value) -> Result<Self> {
        Ok(Self {
            turn: result
                .get("turn")
                .and_then(Value::as_u64)
                .context("HC terminal.send missing turn")?,
            started: result.get("started").and_then(Value::as_bool),
            status: result
                .get("status")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }
}

/// Perform the `hello` handshake on an open stream and return the
/// advertised surface.
#[cfg(unix)]
pub async fn hc_handshake<S>(stream: &mut S) -> Result<HelloOutcome>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let response = send_line(stream, &hello_request(1)).await?;
    if let Some(error) = rpc_error(&response) {
        return Err(HcCallError { error }.into());
    }
    HelloOutcome::from_result(response_result(&response)?)
}

/// Windows stub.
#[cfg(not(unix))]
pub async fn hc_handshake<S>(_stream: &mut S) -> Result<HelloOutcome>
where
    S: Unpin,
{
    Err(anyhow!(
        "herdr-core is not supported on this platform (Unix-only)"
    ))
}

/// Connect to the HC socket and run the `hello` handshake.
#[cfg(unix)]
pub async fn hc_connect(socket_path: &Path) -> Result<(UnixStream, HelloOutcome)> {
    let mut stream = UnixStream::connect(socket_path).await.with_context(|| {
        format!(
            "failed to connect to HC socket at {}",
            socket_path.display()
        )
    })?;
    let hello = hc_handshake(&mut stream).await?;
    Ok((stream, hello))
}

/// Windows stub.
#[cfg(not(unix))]
pub async fn hc_connect(_socket_path: &Path) -> Result<((), HelloOutcome)> {
    Err(anyhow!(
        "herdr-core is not supported on this platform (Unix-only)"
    ))
}

/// Send one HC request on a connected stream and return the `result`.
/// A JSON-RPC error is surfaced as [`HcCallError`] with its `data.kind`
/// preserved.
#[cfg(unix)]
pub async fn hc_call_on<S>(stream: &mut S, request: &Value) -> Result<Value>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let response = send_line(stream, request).await?;
    if let Some(error) = rpc_error(&response) {
        return Err(HcCallError { error }.into());
    }
    Ok(response_result(&response)?.clone())
}

/// Connect, handshake, and send one HC request.
#[cfg(unix)]
pub async fn hc_call(socket_path: &Path, request: &Value) -> Result<Value> {
    let (mut stream, _hello) = hc_connect(socket_path).await?;
    hc_call_on(&mut stream, request).await
}

/// Windows stub.
#[cfg(not(unix))]
pub async fn hc_call(_socket_path: &Path, _request: &Value) -> Result<Value> {
    Err(anyhow!(
        "herdr-core is not supported on this platform (Unix-only)"
    ))
}

/// Windows stub.
#[cfg(not(unix))]
pub async fn hc_call_on<S>(_stream: &mut S, _request: &Value) -> Result<Value>
where
    S: Unpin,
{
    Err(anyhow!(
        "herdr-core is not supported on this platform (Unix-only)"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serialization() {
        let req = serde_json::json!({
            "id": "test:1",
            "method": "pane.report_agent",
            "params": {
                "pane_id": "w1:p1",
                "source": "custom:jcode",
                "agent": "jcode",
                "state": "working"
            }
        });
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("pane.report_agent"));
        assert!(s.contains("w1:p1"));
    }

    #[test]
    fn hello_request_shape_matches_schema() {
        let req = hello_request(7);
        assert_eq!(req["jsonrpc"], "2.0");
        assert_eq!(req["id"], 7);
        assert_eq!(req["method"], "hello");
        // paramStructure is by-name: params must be an object.
        assert!(req["params"].is_object());
        assert_eq!(req["params"]["generation"], HC_PROTOCOL_GENERATION);
        // `framed` is omitted, leaving the schema default (false).
        assert!(req["params"].get("framed").is_none());
    }

    #[test]
    fn hello_outcome_exposes_methods_and_identity() {
        let result = serde_json::json!({
            "generation": 1,
            "version": "0.1.21",
            "core": { "id": "abc", "name": "rohan" },
            "framed": false,
            "methods": ["hello", "terminal.get", "terminal.send", "terminal.list"],
            "features": ["size-notify"],
            "keeps_running_after_logout": null
        });
        let hello = HelloOutcome::from_result(&result).unwrap();
        assert_eq!(hello.generation, 1);
        assert_eq!(hello.version, "0.1.21");
        assert_eq!(hello.core_id, "abc");
        assert_eq!(hello.core_name, "rohan");
        assert!(!hello.framed);
        assert_eq!(hello.methods.len(), 4);
        assert!(hello.advertises("terminal.send"));
        assert!(!hello.advertises("terminal.kill"));
    }

    #[test]
    fn hello_outcome_rejects_missing_methods() {
        let result = serde_json::json!({
            "generation": 1,
            "version": "0.1.21",
            "core": { "id": "abc", "name": "rohan" },
            "framed": false,
            "features": []
        });
        assert!(HelloOutcome::from_result(&result).is_err());
    }

    #[test]
    fn rpc_error_surfaces_stable_kind() {
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": { "code": -32000, "message": "blocked", "data": { "kind": "blocked" } }
        });
        let error = rpc_error(&response).expect("error present");
        assert_eq!(error.code, -32000);
        assert_eq!(error.kind(), Some("blocked"));
        assert!(error.to_string().contains("blocked"));
    }

    #[test]
    fn rpc_error_without_kind_is_not_invented() {
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": { "code": -32602, "message": "bad params" }
        });
        let error = rpc_error(&response).unwrap();
        assert_eq!(error.kind(), None);
    }

    #[test]
    fn successful_response_has_no_error() {
        let response = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": {} });
        assert!(rpc_error(&response).is_none());
        assert!(response_result(&response).is_ok());
    }

    #[test]
    fn terminal_params_match_schema_fields() {
        let get = terminal_get_params("t7", Some("rohan/t3"));
        assert_eq!(get["terminal"], "t7");
        assert_eq!(get["caller"], "rohan/t3");

        let send = terminal_send_params("t7", "hi", None, true, Some("idle"));
        assert_eq!(send["text"], "hi");
        assert_eq!(send["force"], true);
        assert_eq!(send["if"], "idle");
        assert!(send.get("caller").is_none());

        let labels = BTreeMap::new();
        let list = terminal_list_params(true, &labels);
        assert_eq!(list["all"], true);
        assert!(list["labels"].is_object());
    }

    #[test]
    fn terminal_list_parses_required_fields() {
        let result = serde_json::json!({
            "terminals": [{
                "address": "t7",
                "labels": { "name": "review" },
                "argv": ["bash"],
                "cwd": "/repo",
                "state": "live",
                "created_ms": 5,
                "screen": "exact",
                "cols": 80,
                "rows": 24
            }],
            "hidden_finished": 2,
            "machines": []
        });
        let list = TerminalList::from_result(&result).unwrap();
        assert_eq!(list.hidden_finished, 2);
        assert_eq!(list.terminals.len(), 1);
        let terminal = &list.terminals[0];
        assert_eq!(terminal.address, "t7");
        assert_eq!(terminal.state, "live");
        assert_eq!(terminal.labels.get("name").map(String::as_str), Some("review"));
        assert_eq!(terminal.cols, 80);
    }

    #[test]
    fn send_outcome_parses_required_turn() {
        let result = serde_json::json!({ "turn": 4, "started": true, "status": "idle" });
        let outcome = SendOutcome::from_result(&result).unwrap();
        assert_eq!(outcome.turn, 4);
        assert_eq!(outcome.started, Some(true));
        assert_eq!(outcome.status.as_deref(), Some("idle"));
        assert!(SendOutcome::from_result(&serde_json::json!({})).is_err());
    }

    #[cfg(unix)]
    mod transport {
        use super::*;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        /// A fake HC peer: reads one request line, replies with `reply`.
        async fn fake_peer<S>(mut stream: S, reply: Value)
        where
            S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
        {
            let mut reader = BufReader::new(&mut stream);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            drop(reader);
            let mut payload = serde_json::to_string(&reply).unwrap();
            payload.push('\n');
            stream.write_all(payload.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
        }

        #[tokio::test]
        async fn handshake_parses_advertised_methods() {
            let reply = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "generation": 1,
                    "version": "0.1.21",
                    "core": { "id": "abc", "name": "rohan" },
                    "framed": false,
                    "methods": ["hello", "terminal.get", "terminal.send", "terminal.list"],
                    "features": []
                }
            });
            let (mut client, server) = tokio::io::duplex(4096);
            let peer = tokio::spawn(fake_peer(server, reply));
            let hello = hc_handshake(&mut client).await.unwrap();
            peer.await.unwrap();
            assert!(hello.advertises("terminal.list"));
            assert_eq!(hello.core_name, "rohan");
        }

        #[tokio::test]
        async fn handshake_surfaces_error_kind() {
            let reply = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "error": {
                    "code": -32000,
                    "message": "unsupported_generation",
                    "data": { "kind": "unsupported_generation" }
                }
            });
            let (mut client, server) = tokio::io::duplex(4096);
            let peer = tokio::spawn(fake_peer(server, reply));
            let error = hc_handshake(&mut client).await.unwrap_err();
            peer.await.unwrap();
            let text = format!("{error:#}");
            assert!(text.contains("unsupported_generation"), "{text}");
        }

        #[tokio::test]
        async fn call_preserves_data_kind_for_terminal_send() {
            let reply = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "error": { "code": -32000, "message": "blocked", "data": { "kind": "blocked" } }
            });
            let (mut client, server) = tokio::io::duplex(4096);
            let peer = tokio::spawn(fake_peer(server, reply));
            let request = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "terminal.send",
                "params": terminal_send_params("t7", "hi", None, false, None)
            });
            let error = hc_call_on(&mut client, &request).await.unwrap_err();
            peer.await.unwrap();
            let hc_error = error
                .downcast_ref::<HcCallError>()
                .expect("HcCallError preserved");
            assert_eq!(hc_error.kind(), Some("blocked"));
        }

        #[tokio::test]
        async fn legacy_newline_round_trip_still_works() {
            let reply = serde_json::json!({ "ok": true });
            let (mut client, server) = tokio::io::duplex(4096);
            let peer = tokio::spawn(fake_peer(server, reply));
            let response = send_line(&mut client, &serde_json::json!({ "id": "x" }))
                .await
                .unwrap();
            peer.await.unwrap();
            assert_eq!(response["ok"], true);
        }
    }
}
