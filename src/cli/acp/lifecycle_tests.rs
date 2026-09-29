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

async fn attach(runtime: &AcpRuntime, id: &str) -> (BufReader<ReadHalf>, WriteHalf) {
    let (client, server) = crate::transport::stream_pair().unwrap();
    let (reader, writer) = client.into_split();
    runtime
        .register_session(DaemonSession::new(id.into(), reader, writer, 2), false)
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

#[tokio::test]
async fn prompt_and_cancel_use_selected_attached_session_only() {
    let directory = tempfile::tempdir().unwrap();
    let mut runtime = AcpRuntime::new(AcpProfile::Standard, ProviderChoice::Jcode, None, None);
    runtime.broker_path = directory.path().join("absent-broker");
    let (mut selected, mut selected_writer) = attach(&runtime, "selected").await;
    let (mut unrelated, _unrelated_writer) = attach(&runtime, "unrelated").await;
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
}

#[tokio::test]
async fn closing_passive_attachment_sends_no_cancel() {
    let runtime = AcpRuntime::new(AcpProfile::Standard, ProviderChoice::Jcode, None, None);
    let (mut daemon, _writer) = attach(&runtime, "passive-close").await;
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
