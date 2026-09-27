use super::*;

impl DaemonSession {
    pub(super) async fn stop_pump(&self) {
        if let Some(pump) = self.pump.lock().await.take() {
            pump.abort();
        }
    }
}

impl AcpRuntime {
    pub(super) async fn register_session(&self, session: DaemonSession) {
        let session = Arc::new(session);
        let (sender, receiver) = tokio::sync::mpsc::channel(256);
        *session.events.lock().await = Some(receiver);
        if let Some(old) = self
            .sessions
            .lock()
            .await
            .insert(session.session_id.clone(), session.clone())
        {
            old.stop_pump().await;
        }
        let runtime = self.clone();
        let active = session.clone();
        let task = tokio::spawn(async move {
            let mut mapper = EventMapper::new(active.session_id.clone(), runtime.profile);
            loop {
                let event = match active.read_raw_event().await {
                    Ok(event) => event,
                    Err(error) => {
                        let _ = sender.send(Err(error)).await;
                        break;
                    }
                };
                if runtime.profile.is_extended()
                    && runtime
                        .write_jcode_extension_event(&active.session_id, &event)
                        .await
                        .is_err()
                {
                    break;
                }
                for update in mapper.map_event(event.clone()) {
                    if runtime
                        .write_notification(
                            "session/update",
                            json!({"sessionId":active.session_id,"update":update}),
                        )
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                if active.prompt_running.load(Ordering::SeqCst)
                    && sender.send(Ok(event)).await.is_err()
                {
                    break;
                }
            }
        });
        *session.pump.lock().await = Some(task.abort_handle());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn handshake_preserves_buffered_events() {
        let (client, mut server) = crate::transport::stream_pair().unwrap();
        let (reader, writer) = client.into_split();
        let session = DaemonSession::new("s1".into(), reader, writer, 2);
        server
            .write_all(b"{\"type\":\"done\",\"id\":1}\n{\"type\":\"done\",\"id\":2}\n")
            .await
            .unwrap();
        assert!(matches!(
            session.read_event().await.unwrap(),
            ServerEvent::Done { id: 1 }
        ));
        let session = session.with_ui_state(SessionUiState::default());
        assert!(matches!(
            session.read_event().await.unwrap(),
            ServerEvent::Done { id: 2 }
        ));
    }

    #[tokio::test]
    async fn pump_preserves_completion_and_disconnect_for_active_consumer() {
        let (client, mut server) = crate::transport::stream_pair().unwrap();
        let (reader, writer) = client.into_split();
        let session = DaemonSession::new("isolated".into(), reader, writer, 2);
        session.prompt_running.store(true, Ordering::SeqCst);
        let runtime = AcpRuntime::new(AcpProfile::Standard, ProviderChoice::Jcode, None, None);
        runtime.register_session(session).await;
        let session = runtime.sessions.lock().await["isolated"].clone();
        server
            .write_all(b"{\"type\":\"done\",\"id\":7}\n")
            .await
            .unwrap();
        assert!(matches!(
            session.read_event().await.unwrap(),
            ServerEvent::Done { id: 7 }
        ));
        drop(server);
        assert!(session.read_event().await.is_err());
        session.stop_pump().await;
    }
}
