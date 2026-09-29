use super::*;
use std::collections::HashSet;

#[cfg(unix)]
async fn exchange(path: &std::path::Path, input: Value) -> Result<Value> {
    use tokio::io::AsyncReadExt;
    let stream = tokio::net::UnixStream::connect(path).await?;
    let (reader, mut writer) = stream.into_split();
    writer.write_all(format!("{input}\n").as_bytes()).await?;
    let mut reader = BufReader::new(reader.take(1024 * 1024));
    let mut line = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        reader.read_line(&mut line),
    )
    .await??;
    let result: Value = serde_json::from_str(&line)?;
    if let Some(error) = result.get("error") {
        anyhow::bail!("{error}");
    }
    Ok(result["result"].clone())
}
#[cfg(not(unix))]
async fn exchange(_: &std::path::Path, _: Value) -> Result<Value> {
    anyhow::bail!("remote interactions require Unix IPC")
}

impl AcpRuntime {
    pub(super) async fn cancel_interactions(&self, session: &str) {
        let _ = exchange(
            &self.broker_path,
            json!({"method":"cancel","sessionId":session,"controller":self.controller}),
        )
        .await;
        self.interaction_requests
            .lock()
            .await
            .retain(|_, owner| owner != session);
    }

    pub(super) async fn claim_interactions(&self, session_id: &str) -> Result<()> {
        let claim = json!({"method":"list","sessionId":session_id,"controller":self.controller,
            "form":self.form_elicitation.load(Ordering::SeqCst)});
        exchange(&self.broker_path, claim)
            .await
            .context("Cannot acquire session interaction ownership")?;
        Ok(())
    }

    pub(super) async fn start_interactions(&self, session: Arc<DaemonSession>) {
        let runtime = self.clone();
        let target = session.clone();
        let task = tokio::spawn(async move {
            let mut sent: HashSet<String> = HashSet::new();
            loop {
                let query = json!({"method":"list","sessionId":target.session_id,"controller":runtime.controller,
                    "form":runtime.form_elicitation.load(Ordering::SeqCst)});
                if let Ok(result) = exchange(&runtime.broker_path, query).await {
                    let requests = result["requests"].as_array().cloned().unwrap_or_default();
                    let current: HashSet<String> = requests
                        .iter()
                        .filter_map(|request| request["id"].as_str().map(str::to_string))
                        .collect();
                    sent.retain(|id| current.contains(id));
                    for request in requests {
                        let Some(id) = request["id"].as_str() else {
                            continue;
                        };
                        if sent.contains(id) {
                            continue;
                        }
                        if request["method"] == "elicitation/create"
                            && !runtime.form_elicitation.load(Ordering::SeqCst)
                        {
                            continue;
                        }
                        runtime
                            .interaction_requests
                            .lock()
                            .await
                            .insert(id.into(), target.session_id.clone());
                        let output = json!({"jsonrpc":"2.0","id":id,"method":request["method"],"params":request["params"]});
                        let mut stdout = runtime.stdout.lock().await;
                        if stdout
                            .write_all(format!("{output}\n").as_bytes())
                            .await
                            .is_err()
                        {
                            return;
                        }
                        if stdout.flush().await.is_err() {
                            return;
                        }
                        sent.insert(id.into());
                    }
                    runtime
                        .interaction_requests
                        .lock()
                        .await
                        .retain(|id, session| {
                            session != &target.session_id || current.contains(id)
                        });
                }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
        });
        *session.interaction_pump.lock().await = Some(task.abort_handle());
    }

    pub(super) async fn interaction_response(
        &self,
        id: Value,
        result: Option<Value>,
    ) -> Result<()> {
        let Some(key) = id.as_str() else {
            return Ok(());
        };
        let Some(session) = self.interaction_requests.lock().await.get(key).cloned() else {
            return Ok(());
        };
        let result =
            result.unwrap_or_else(|| json!({"outcome":{"outcome":"cancelled"},"action":"cancel"}));
        let response = exchange(
            &self.broker_path,
            json!({"method":"respond","sessionId":session,"controller":self.controller,
            "id":key,"result":result}),
        )
        .await;
        if response.is_ok() {
            self.interaction_requests.lock().await.remove(key);
        }
        if let Err(error) = response {
            crate::logging::warn(&format!("ACP interaction response rejected: {error}"));
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejected_ownership_does_not_register_or_replace_attachment() {
        let directory = tempfile::tempdir().unwrap();
        let broker_path = directory.path().join("broker.sock");
        let listener = tokio::net::UnixListener::bind(&broker_path).unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut reader = BufReader::new(reader);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["sessionId"], "controlled");
            writer
                .write_all(b"{\"error\":\"session has another interaction controller\"}\n")
                .await
                .unwrap();
        });
        let (client, _daemon) = crate::transport::stream_pair().unwrap();
        let (reader, writer) = client.into_split();
        let session = DaemonSession::new("controlled".into(), reader, writer, 2);
        let mut runtime = AcpRuntime::new(AcpProfile::Standard, ProviderChoice::Jcode, None, None);
        runtime.broker_path = broker_path;
        let error = runtime.register_session(session, true).await.unwrap_err();
        assert!(format!("{error:#}").contains("another interaction controller"));
        assert!(runtime.sessions.lock().await.is_empty());
        server.await.unwrap();
    }
}
