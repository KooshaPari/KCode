use super::super::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

async fn rpc(value: Value) -> Value {
    let stream = tokio::net::UnixStream::connect(socket_path())
        .await
        .unwrap();
    let (reader, mut writer) = stream.into_split();
    writer
        .write_all(format!("{value}\n").as_bytes())
        .await
        .unwrap();
    let mut line = String::new();
    BufReader::new(reader).read_line(&mut line).await.unwrap();
    serde_json::from_str(&line).unwrap()
}

fn query(session: &str) -> Value {
    json!({"method":"list","sessionId":session,"controller":"isolated-owner","form":true})
}

#[tokio::test]
async fn held_permission_ipc() {
    if std::env::var_os("JCODE_INTERACTION_TEST_CHILD").is_none() {
        let root = tempfile::tempdir_in("/tmp").unwrap();
        let directory = root.path().canonicalize().unwrap();
        let output = tokio::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "tool::interaction::tests::ipc::held_permission_ipc",
                "--nocapture",
            ])
            .env("JCODE_INTERACTION_TEST_CHILD", "1")
            .env("JCODE_SOCKET", directory.join("daemon.sock"))
            .env("JCODE_HOME", &directory)
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "child failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    tokio::time::timeout(std::time::Duration::from_secs(10), exercise())
        .await
        .unwrap();
}

async fn exercise() {
    use std::os::unix::fs::PermissionsExt;
    ensure_server().await.unwrap();
    assert_eq!(
        std::fs::metadata(socket_path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    for outcome in ["allow-once", "reject-once", "cancel"] {
        let session = format!("isolated-{outcome}");
        assert!(rpc(query(&session)).await.get("error").is_none());
        let mut competing = query(&session);
        competing["controller"] = json!("competing-owner");
        assert!(rpc(competing).await.get("error").is_some());
        let marker = socket_path().with_extension(format!("{outcome}.effect"));
        let operation_session = session.clone();
        let operation_marker = marker.clone();
        let operation = tokio::spawn(async move {
            let ctx = crate::tool::ToolContext {
                session_id: operation_session,
                message_id: "message".into(),
                tool_call_id: "tool".into(),
                working_dir: None,
                stdin_request_tx: None,
                graceful_shutdown_signal: None,
                execution_mode: crate::tool::ToolExecutionMode::Direct,
            };
            permission(&ctx, "synthetic-marker", &json!({}), "test held operation").await?;
            std::fs::write(operation_marker, "executed exactly once")?;
            Ok::<(), anyhow::Error>(())
        });
        let request = loop {
            let pending = rpc(query(&session)).await;
            if let Some(request) = pending["result"]["requests"].as_array().unwrap().first() {
                break request.clone();
            }
            tokio::task::yield_now().await;
        };
        assert!(
            !marker.exists(),
            "operation must remain held before approval"
        );
        assert!(!operation.is_finished());
        let answer = |session: &str, choice: &str| json!({"method":"respond","controller":"isolated-owner","sessionId":session,"id":request["id"],"result":{"outcome":{"outcome":"selected","optionId":choice}}});
        assert!(
            rpc(answer("wrong-session", "allow-once"))
                .await
                .get("error")
                .is_some()
        );
        assert!(
            rpc(answer(&session, "invented"))
                .await
                .get("error")
                .is_some()
        );
        assert!(!marker.exists());
        let response = if outcome == "cancel" {
            rpc(json!({"method":"cancel","sessionId":session,"controller":"isolated-owner"})).await
        } else {
            rpc(answer(&session, outcome)).await
        };
        assert!(response.get("error").is_none(), "{response}");
        let result = operation.await.unwrap();
        assert_eq!(result.is_ok(), outcome == "allow-once");
        assert_eq!(marker.exists(), outcome == "allow-once");
        assert!(
            rpc(answer(&session, "allow-once"))
                .await
                .get("error")
                .is_some()
        );
        assert!(
            rpc(query(&session)).await["result"]["requests"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    expires_without_side_effect().await;
    form_answer_resolves_original_waiter().await;
}

async fn expires_without_side_effect() {
    let marker = socket_path().with_extension("timeout.effect");
    let effect = marker.clone();
    let operation = tokio::spawn(async move {
        request("expires", "elicitation/create", json!({}), 1).await?;
        std::fs::write(effect, "must never execute")?;
        Ok::<(), anyhow::Error>(())
    });
    let held = next_request("expires").await;
    assert!(!marker.exists());
    assert!(
        operation
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("expired")
    );
    assert!(!marker.exists());
    assert!(rpc(json!({"method":"respond","controller":"isolated-owner","sessionId":"expires","id":held["id"],"result":{"action":"decline"}})).await.get("error").is_some());
}

async fn next_request(session: &str) -> Value {
    loop {
        let response = rpc(query(session)).await;
        if let Some(request) = response["result"]["requests"].as_array().unwrap().first() {
            return request.clone();
        }
        tokio::task::yield_now().await;
    }
}

async fn form_answer_resolves_original_waiter() {
    let operation = tokio::spawn(request(
        "form",
        "elicitation/create",
        json!({
            "requestedSchema":{"type":"object","properties":{"value":{"type":"boolean"}},"required":["value"]}
        }),
        5,
    ));
    let held = next_request("form").await;
    let answer = |value: Value| json!({"method":"respond","controller":"isolated-owner","sessionId":"form","id":held["id"],"result":{"action":"accept","content":{"value":value}}});
    assert!(rpc(answer(json!("true"))).await.get("error").is_some());
    assert!(!operation.is_finished());
    assert!(rpc(answer(json!(true))).await.get("error").is_none());
    assert_eq!(operation.await.unwrap().unwrap()["content"]["value"], true);
    assert!(rpc(answer(json!(true))).await.get("error").is_some());
}
