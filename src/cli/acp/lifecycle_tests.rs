use super::*;
use tokio::time::{Duration, timeout};

fn message(id: u64, session: &str, prompt: bool) -> JsonRpcMessage {
    JsonRpcMessage {
        id: Some(json!(id)),
        method: None,
        result: None,
        params: if prompt {
            json!({"sessionId":session,"prompt":[{"type":"text","text":"synthetic prompt"}]})
        } else {
            json!({"sessionId":session})
        },
    }
}

async fn attach(runtime: &AcpRuntime, id: &str, control: bool) -> (BufReader<ReadHalf>, WriteHalf) {
    let (client, server) = crate::transport::stream_pair().unwrap();
    let (reader, writer) = client.into_split();
    runtime
        .register_session(DaemonSession::new(id.into(), reader, writer, 2), control)
        .await
        .unwrap();
    let (reader, writer) = server.into_split();
    (BufReader::new(reader), writer)
}

async fn request(reader: &mut BufReader<ReadHalf>) -> Value {
    let mut line = String::new();
    timeout(Duration::from_secs(2), reader.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    serde_json::from_str(&line).unwrap()
}

#[cfg(unix)]
#[tokio::test]
async fn prompt_and_cancel_use_selected_attached_session_only() {
    let directory = tempfile::tempdir_in("/tmp").unwrap();
    let mut runtime = AcpRuntime::new(AcpProfile::Standard, ProviderChoice::Jcode, None, None);
    runtime.broker_path = directory.path().join("broker.sock");
    let listener = tokio::net::UnixListener::bind(&runtime.broker_path).unwrap();
    let broker = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut line = String::new();
            BufReader::new(reader).read_line(&mut line).await.unwrap();
            let input: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(input["sessionId"], "selected");
            assert!(matches!(
                input["method"].as_str(),
                Some("list" | "renew" | "cancel")
            ));
            writer
                .write_all(b"{\"result\":{\"requests\":[]}}\n")
                .await
                .unwrap();
        }
    });
    let (mut selected, mut selected_writer) = attach(&runtime, "selected", true).await;
    let (mut unrelated, _unrelated_writer) = attach(&runtime, "unrelated", false).await;
    runtime
        .handle_session_prompt(message(1, "selected", true))
        .await
        .unwrap();
    let prompt = request(&mut selected).await;
    assert_eq!(prompt["type"], "message");
    assert_eq!(prompt["content"], "synthetic prompt");
    assert_eq!(prompt["id"], 2);
    runtime
        .handle_session_cancel(message(2, "selected", false))
        .await
        .unwrap();
    let cancel = request(&mut selected).await;
    assert_eq!(cancel["type"], "cancel");
    assert_eq!(cancel["id"], 3);
    selected_writer
        .write_all(b"{\"type\":\"interrupted\"}\n{\"type\":\"done\",\"id\":2}\n")
        .await
        .unwrap();
    let active = runtime.sessions.lock().await["selected"].clone();
    timeout(Duration::from_secs(2), async {
        while active.prompt_running.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let mut line = String::new();
    assert!(
        timeout(Duration::from_millis(100), unrelated.read_line(&mut line))
            .await
            .is_err()
    );
    for session in runtime.sessions.lock().await.values() {
        session.stop_pump().await;
    }
    broker.abort();
}

#[tokio::test]
async fn closing_passive_attachment_sends_no_cancel() {
    let runtime = AcpRuntime::new(AcpProfile::Standard, ProviderChoice::Jcode, None, None);
    let (mut daemon, _writer) = attach(&runtime, "passive-close", false).await;
    runtime
        .handle_session_close(message(1, "passive-close", false))
        .await
        .unwrap();
    let mut line = String::new();
    let count = timeout(Duration::from_secs(2), daemon.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        count, 0,
        "passive close must disconnect without a daemon request: {line}"
    );
    assert!(!runtime.sessions.lock().await.contains_key("passive-close"));
}

#[tokio::test]
async fn passive_attachment_rejects_prompt_cancel_and_config_without_daemon_mutation() {
    let runtime = AcpRuntime::new(AcpProfile::Standard, ProviderChoice::Jcode, None, None);
    let (mut daemon, _writer) = attach(&runtime, "passive", false).await;
    let session = runtime.sessions.lock().await["passive"].clone();
    let error = runtime
        .require_interaction_ownership(&session)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("attached for observation"));
    runtime
        .handle_session_prompt(message(1, "passive", true))
        .await
        .unwrap();
    runtime
        .handle_session_cancel(message(2, "passive", false))
        .await
        .unwrap();
    let mut config = message(3, "passive", false);
    config.params["configId"] = json!("model");
    config.params["value"] = json!("must-not-change");
    runtime.handle_set_config_option(config).await.unwrap();
    let mut line = String::new();
    assert!(
        timeout(Duration::from_millis(100), daemon.read_line(&mut line))
            .await
            .is_err()
    );
    assert!(!session.prompt_running.load(Ordering::SeqCst));
    assert!(!session.interaction_requested.load(Ordering::SeqCst));
    assert!(!session.interaction_active.load(Ordering::SeqCst));
    assert!(runtime.interaction_requests.lock().await.is_empty());
    session.stop_pump().await;
}
