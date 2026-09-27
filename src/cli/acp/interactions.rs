use super::*;
use std::collections::HashSet;

#[cfg(unix)]
async fn exchange(input: Value) -> Result<Value> {
    use tokio::io::AsyncReadExt;
    let stream = tokio::net::UnixStream::connect(crate::tool::interaction::socket_path()).await?;
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
async fn exchange(_: Value) -> Result<Value> {
    anyhow::bail!("remote interactions require Unix IPC")
}

impl AcpRuntime {
    pub(super) async fn cancel_interactions(&self, session: &str) {
        let _ =
            exchange(json!({"method":"cancel","sessionId":session,"controller":self.controller}))
                .await;
        self.interaction_requests
            .lock()
            .await
            .retain(|_, owner| owner != session);
    }

    pub(super) async fn start_interactions(&self, session: Arc<DaemonSession>) {
        let claim = json!({"method":"list","sessionId":session.session_id,"controller":self.controller,"form":self.form_elicitation.load(Ordering::SeqCst)});
        if let Err(error) = exchange(claim).await {
            crate::logging::warn(&format!(
                "ACP interaction controller claim pending: {error}"
            ));
        }
        let runtime = self.clone();
        let target = session.clone();
        let task = tokio::spawn(async move {
            let mut sent = HashSet::new();
            loop {
                let query = json!({"method":"list","sessionId":target.session_id,"controller":runtime.controller,
                    "form":runtime.form_elicitation.load(Ordering::SeqCst)});
                if let Ok(result) = exchange(query).await {
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
