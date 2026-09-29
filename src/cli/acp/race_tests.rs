use super::*;
use tokio::time::{Duration, timeout};

async fn attach(
    runtime: &AcpRuntime,
    id: &str,
) -> (Arc<DaemonSession>, BufReader<ReadHalf>, WriteHalf) {
    let (client, server) = crate::transport::stream_pair().unwrap();
    let (reader, writer) = client.into_split();
    runtime
        .register_session(DaemonSession::new(id.into(), reader, writer, 2), false)
        .await
        .unwrap();
    let session = runtime.sessions.lock().await[id].clone();
    let (reader, writer) = server.into_split();
    (session, BufReader::new(reader), writer)
}

async fn read_request(reader: &mut BufReader<ReadHalf>) -> Value {
    let mut line = String::new();
    timeout(Duration::from_secs(2), reader.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    serde_json::from_str(&line).unwrap()
}

#[tokio::test]
async fn reload_preserves_original_prompt_transport_and_waiter() {
    let runtime = AcpRuntime::new(AcpProfile::Standard, ProviderChoice::Jcode, None, None);
    let (original, mut daemon, mut writer) = attach(&runtime, "same-session").await;
    original.prompt_running.store(true, Ordering::SeqCst);
    let prompt_runtime = runtime.clone();
    let prompt_session = original.clone();
    let prompt = tokio::spawn(async move {
        prompt_runtime
            .run_prompt(json!(1), prompt_session, "held prompt".into(), vec![])
            .await
    });
    assert_eq!(read_request(&mut daemon).await["id"], 2);
    let (replacement, _replacement_reader, _replacement_writer) =
        attach(&runtime, "same-session").await;
    writer
        .write_all(b"{\"type\":\"done\",\"id\":2}\n")
        .await
        .unwrap();
    let result = timeout(Duration::from_secs(2), prompt)
        .await
        .unwrap()
        .unwrap();
    assert!(
        result.is_ok(),
        "reload destroyed the original prompt waiter: {result:?}"
    );
    assert!(
        Arc::ptr_eq(&original, &replacement),
        "reload must retain the live transport owner"
    );
    original.stop_pump().await;
}

#[cfg(unix)]
#[tokio::test]
async fn prompt_ownership_survives_backpressured_terminal_response() {
    let root = tempfile::tempdir_in("/tmp").unwrap();
    let path = root.path().join("broker.sock");
    let listener = tokio::net::UnixListener::bind(&path).unwrap();
    let broker = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut line = String::new();
            BufReader::new(reader).read_line(&mut line).await.unwrap();
            writer
                .write_all(b"{\"result\":{\"requests\":[]}}\n")
                .await
                .unwrap();
        }
    });
    let mut runtime = AcpRuntime::new(AcpProfile::Standard, ProviderChoice::Jcode, None, None);
    runtime.broker_path = path;
    let (session, mut daemon, mut writer) = attach(&runtime, "backpressure").await;
    session.interaction_requested.store(true, Ordering::SeqCst);
    session.interaction_active.store(true, Ordering::SeqCst);
    let blocked_stdout = runtime.stdout.lock().await;
    runtime
        .handle_session_prompt(JsonRpcMessage {
            id: Some(json!(1)),
            method: None,
            result: None,
            params: json!({"sessionId":"backpressure","prompt":[{"type":"text","text":"first"}]}),
        })
        .await
        .unwrap();
    assert_eq!(read_request(&mut daemon).await["id"], 2);
    writer
        .write_all(b"{\"type\":\"done\",\"id\":2}\n")
        .await
        .unwrap();
    let released_before_response = timeout(Duration::from_millis(200), async {
        while session.prompt_running.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(
        released_before_response.is_err(),
        "prompt released before its final response; a successor can be cleared by outer cleanup"
    );
    assert_eq!(*session.active_prompt_id.lock().await, Some(2));
    let competing_runtime = runtime.clone();
    let competing = tokio::spawn(async move {
        competing_runtime.handle_session_prompt(JsonRpcMessage {
            id: Some(json!(2)), method: None, result: None,
            params: json!({"sessionId":"backpressure","prompt":[{"type":"text","text":"must not overlap"}]}),
        }).await
    });
    let mut unexpected = String::new();
    assert!(
        timeout(
            Duration::from_millis(200),
            daemon.read_line(&mut unexpected)
        )
        .await
        .is_err(),
        "a competing prompt reached the daemon before the first response: {unexpected}"
    );
    assert_eq!(*session.active_prompt_id.lock().await, Some(2));
    drop(blocked_stdout);
    timeout(Duration::from_secs(2), competing)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    timeout(Duration::from_secs(2), async {
        while session.prompt_running.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(*session.active_prompt_id.lock().await, None);
    session.stop_pump().await;
    broker.abort();
}
