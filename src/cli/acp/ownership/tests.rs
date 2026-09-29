use super::*;
use tokio::time::{Duration, timeout};

#[test]
fn ownership_loss_identifies_exact_session_and_revoked_capability() {
    assert_eq!(
        loss_update("owned"),
        json!({"sessionId":"owned","update":{
        "sessionUpdate":"session_info_update","_meta":{"jcode.interactionController":false}}})
    );
}

#[tokio::test]
async fn lost_controller_cannot_prompt_cancel_or_keep_pending_responses() {
    let root = tempfile::tempdir().unwrap();
    let mut runtime = AcpRuntime::new(AcpProfile::Standard, ProviderChoice::Jcode, None, None);
    runtime.broker_path = root.path().join("absent-broker");
    let (client, server) = crate::transport::stream_pair().unwrap();
    let (reader, writer) = client.into_split();
    runtime
        .register_session(DaemonSession::new("owned".into(), reader, writer, 2), false)
        .await
        .unwrap();
    let session = runtime.sessions.lock().await["owned"].clone();
    session.interaction_requested.store(true, Ordering::SeqCst);
    session.interaction_active.store(true, Ordering::SeqCst);
    runtime
        .interaction_requests
        .lock()
        .await
        .insert("pending".into(), "owned".into());
    assert!(
        runtime
            .require_interaction_ownership(&session)
            .await
            .is_err()
    );
    assert!(!session.interaction_active.load(Ordering::SeqCst));
    assert!(runtime.interaction_requests.lock().await.is_empty());
    let prompt = JsonRpcMessage {
        id: Some(json!(1)),
        method: None,
        result: None,
        params: json!({"sessionId":"owned","prompt":[{"type":"text","text":"must not execute"}]}),
    };
    runtime.handle_session_prompt(prompt).await.unwrap();
    let cancel = JsonRpcMessage {
        id: Some(json!(2)),
        method: None,
        result: None,
        params: json!({"sessionId":"owned"}),
    };
    runtime.handle_session_cancel(cancel).await.unwrap();
    let (reader, _writer) = server.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    assert!(
        timeout(Duration::from_millis(100), reader.read_line(&mut line))
            .await
            .is_err()
    );
    assert!(!session.prompt_running.load(Ordering::SeqCst));
    session.stop_pump().await;
}

#[cfg(unix)]
#[tokio::test]
async fn renewal_failure_revokes_controller_without_automatic_reclaim() {
    let root = tempfile::tempdir_in("/tmp").unwrap();
    let path = root.path().join("broker.sock");
    let listener = tokio::net::UnixListener::bind(&path).unwrap();
    let broker = tokio::spawn(async move {
        for expected in ["list", "renew"] {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut line = String::new();
            BufReader::new(reader).read_line(&mut line).await.unwrap();
            let input: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(input["method"], expected);
            let response = if expected == "list" {
                "{\"result\":{\"requests\":[]}}\n"
            } else {
                "{\"error\":\"lease expired\"}\n"
            };
            writer.write_all(response.as_bytes()).await.unwrap();
        }
        assert!(
            timeout(Duration::from_millis(400), listener.accept())
                .await
                .is_err(),
            "loss requires explicit reacquire"
        );
    });
    let mut runtime = AcpRuntime::new(AcpProfile::Standard, ProviderChoice::Jcode, None, None);
    runtime.broker_path = path;
    let (client, _server) = crate::transport::stream_pair().unwrap();
    let (reader, writer) = client.into_split();
    runtime
        .register_session(DaemonSession::new("owned".into(), reader, writer, 2), true)
        .await
        .unwrap();
    let session = runtime.sessions.lock().await["owned"].clone();
    timeout(Duration::from_secs(2), async {
        while session.interaction_active.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(
        runtime
            .require_interaction_ownership(&session)
            .await
            .is_err()
    );
    broker.await.unwrap();
    session.stop_pump().await;
}
