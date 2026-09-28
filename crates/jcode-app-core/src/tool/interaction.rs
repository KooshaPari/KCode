//! Session-bound operations held until an authenticated local client responds.
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::oneshot;

type Pending = HashMap<String, Operation>;
struct Operation {
    session: String,
    request: Value,
    response: oneshot::Sender<Value>,
}
static CONTROLLERS: OnceLock<Mutex<HashMap<String, (String, std::time::Instant, bool)>>> =
    OnceLock::new();
fn controllers() -> &'static Mutex<HashMap<String, (String, std::time::Instant, bool)>> {
    CONTROLLERS.get_or_init(|| Mutex::new(HashMap::new()))
}
pub fn controller_active(session: &str) -> bool {
    controllers()
        .lock()
        .map(|owners| {
            owners
                .get(session)
                .is_some_and(|(_, expires, _)| *expires > std::time::Instant::now())
        })
        .unwrap_or(false)
}

pub fn remotely_controlled(session: &str) -> bool {
    controllers()
        .lock()
        .map(|owners| {
            owners
                .get(session)
                .is_some_and(|(_, expires, form)| *form && *expires > std::time::Instant::now())
        })
        .unwrap_or(false)
}

static PENDING: OnceLock<Arc<Mutex<Pending>>> = OnceLock::new();
fn pending() -> &'static Arc<Mutex<Pending>> {
    PENDING.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}
struct Guard(String);
impl Drop for Guard {
    fn drop(&mut self) {
        if let Ok(mut requests) = pending().lock() {
            requests.remove(&self.0);
        }
    }
}

pub fn socket_path() -> std::path::PathBuf {
    let mut path = crate::server::socket_path().into_os_string();
    path.push(".interactions");
    path.into()
}

pub async fn request(session: &str, method: &str, params: Value, timeout: u64) -> Result<Value> {
    ensure_server().await?;
    let id = crate::id::new_id("interaction");
    let (response, receiver) = oneshot::channel();
    params["_meta"]["jcode.timeoutSeconds"] = json!(timeout.clamp(1, 3600));
    let request = json!({"id":id,"method":method,"params":params});
    pending()
        .lock()
        .map_err(|_| anyhow::anyhow!("interaction lock poisoned"))?
        .insert(
            id.clone(),
            Operation {
                session: session.into(),
                request,
                response,
            },
        );
    let _guard = Guard(id);
    tokio::time::timeout(
        std::time::Duration::from_secs(timeout.clamp(1, 3600)),
        receiver,
    )
    .await
    .map_err(|_| anyhow::anyhow!("interaction expired"))?
    .map_err(|_| anyhow::anyhow!("interaction cancelled"))
}

fn dispatch(input: Value) -> Result<Value> {
    let session = input["sessionId"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing session"))?;
    let token = input["controller"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing controller"))?;
    {
        let mut owners = controllers()
            .lock()
            .map_err(|_| anyhow::anyhow!("controller lock poisoned"))?;
        let now = std::time::Instant::now();
        owners.retain(|_, (_, expires, _)| *expires > now);
        if owners
            .get(session)
            .is_some_and(|(owner, expires, _)| owner != token && *expires > now)
        {
            bail!("session has another interaction controller");
        }
        let form = input["form"]
            .as_bool()
            .unwrap_or_else(|| owners.get(session).is_some_and(|(_, _, form)| *form));
        owners.insert(
            session.into(),
            (token.into(), now + std::time::Duration::from_secs(5), form),
        );
    }
    let mut requests = pending()
        .lock()
        .map_err(|_| anyhow::anyhow!("interaction lock poisoned"))?;
    if input["method"] == "cancel" {
        let ids: Vec<_> = requests
            .iter()
            .filter(|(_, op)| op.session == session)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            if let Some(op) = requests.remove(&id) {
                let result = if op.request["method"] == "session/request_permission" {
                    json!({"outcome":{"outcome":"cancelled"}})
                } else {
                    json!({"action":"cancel"})
                };
                let _ = op.response.send(result);
            }
        }
        return Ok(json!({}));
    }
    if input["method"] == "list" {
        return Ok(
            json!({"requests":requests.values().filter(|op| op.session == session)
            .map(|op| op.request.clone()).collect::<Vec<_>>()}),
        );
    }
    if input["method"] != "respond" {
        bail!("unknown interaction method");
    }
    let id = input["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing request id"))?;
    let operation = requests
        .get(id)
        .ok_or_else(|| anyhow::anyhow!("expired or unknown request"))?;
    if operation.session != session {
        bail!("session mismatch");
    }
    let response = input
        .get("result")
        .ok_or_else(|| anyhow::anyhow!("missing result"))?;
    validate(&operation.request, response)?;
    let operation = requests.remove(id).expect("request checked under lock");
    operation
        .response
        .send(response.clone())
        .map_err(|_| anyhow::anyhow!("operation cancelled"))?;
    Ok(json!({}))
}

mod validation;
use validation::validate;

#[cfg(unix)]
pub async fn ensure_server() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    static SERVER: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
    SERVER
        .get_or_try_init(|| async {
            let path = socket_path();
            if let Ok(metadata) = std::fs::symlink_metadata(&path) {
                use std::os::unix::fs::{FileTypeExt, MetadataExt};
                if !metadata.file_type().is_socket() || metadata.uid() != unsafe { libc::geteuid() }
                {
                    bail!("unsafe interaction socket path");
                }
                if tokio::net::UnixStream::connect(&path).await.is_ok() {
                    bail!("interaction socket already owned");
                }
                std::fs::remove_file(&path)?;
            }
            let listener = tokio::net::UnixListener::bind(&path)?;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    tokio::spawn(async move {
                        let (reader, mut writer) = stream.into_split();
                        let mut reader = BufReader::new(reader.take(65536));
                        let mut line = String::new();
                        let result = tokio::time::timeout(
                            std::time::Duration::from_secs(5),
                            reader.read_line(&mut line),
                        )
                        .await;
                        if !matches!(result, Ok(Ok(_))) || !line.ends_with('\n') {
                            return;
                        }
                        let result = serde_json::from_str(&line)
                            .map_err(anyhow::Error::from)
                            .and_then(dispatch);
                        let output = match result {
                            Ok(value) => json!({"result":value}),
                            Err(error) => json!({"error":error.to_string()}),
                        };
                        let _ = writer.write_all(format!("{output}\n").as_bytes()).await;
                    });
                }
            });
            Ok::<(), anyhow::Error>(())
        })
        .await?;
    Ok(())
}
#[cfg(not(unix))]
pub async fn ensure_server() -> Result<()> {
    bail!("remote interactions require Unix local IPC")
}

pub async fn permission(
    ctx: &super::ToolContext,
    name: &str,
    input: &Value,
    reason: &str,
) -> Result<()> {
    if !controller_active(&ctx.session_id) {
        bail!("Tool requires approval but this session has no active interaction controller");
    }
    let response = request(&ctx.session_id, "session/request_permission", json!({
        "sessionId":ctx.session_id,"toolCall":{"toolCallId":ctx.tool_call_id,"title":reason,"name":name,"rawInput":input,"status":"pending"},
        "options":[{"optionId":"allow-once","name":"Allow once","kind":"allow_once"},{"optionId":"reject-once","name":"Reject","kind":"reject_once"}]
    }), 600).await?;
    if response["outcome"]["outcome"] != "selected"
        || response["outcome"]["optionId"] != "allow-once"
    {
        bail!("tool permission rejected or cancelled");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
