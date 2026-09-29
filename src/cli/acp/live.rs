use super::*;

impl DaemonSession {
    pub(super) async fn clear_events(&self) {
        if let Some(receiver) = self.events.lock().await.as_mut() {
            while receiver.try_recv().is_ok() {}
        }
    }
    pub(super) async fn stop_pump(&self) {
        if let Some(pump) = self.interaction_pump.lock().await.take() {
            pump.abort();
        }
        if let Some(pump) = self.pump.lock().await.take() {
            pump.abort();
        }
    }
}

impl AcpRuntime {
    pub(super) async fn register_session(
        &self,
        session: DaemonSession,
        control_interactions: bool,
    ) -> Result<()> {
        let session = Arc::new(session);
        if control_interactions {
            self.claim_interactions(&session.session_id).await?;
            session.interaction_requested.store(true, Ordering::SeqCst);
            session.interaction_active.store(true, Ordering::SeqCst);
        }
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
        if control_interactions {
            self.start_interactions(session).await;
        }
        Ok(())
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
    async fn observer_terminal_events_do_not_complete_pending_prompt() {
        let (client, mut server) = crate::transport::stream_pair().unwrap();
        let (reader, writer) = client.into_split();
        let session = DaemonSession::new("observer".into(), reader, writer, 2);
        session.prompt_running.store(true, Ordering::SeqCst);
        let runtime = AcpRuntime::new(AcpProfile::Standard, ProviderChoice::Jcode, None, None);
        runtime.register_session(session, false).await.unwrap();
        let session = runtime.sessions.lock().await["observer"].clone();
        let pending = session.clone();
        let mut completion = tokio::spawn(async move { wait_for_done(&pending, 2).await });
        server.write_all(b"{\"type\":\"done\",\"id\":0}\n{\"type\":\"error\",\"id\":0,\"message\":\"other client failed\"}\n").await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut completion)
                .await
                .is_err()
        );
        server
            .write_all(b"{\"type\":\"done\",\"id\":2}\n")
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), completion)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        session.stop_pump().await;
    }

    #[tokio::test]
    async fn pump_preserves_completion_and_disconnect_for_active_consumer() {
        let (client, mut server) = crate::transport::stream_pair().unwrap();
        let (reader, writer) = client.into_split();
        let session = DaemonSession::new("isolated".into(), reader, writer, 2);
        session.prompt_running.store(true, Ordering::SeqCst);
        let runtime = AcpRuntime::new(AcpProfile::Standard, ProviderChoice::Jcode, None, None);
        runtime.register_session(session, false).await.unwrap();
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

#[cfg(test)]
mod ownership_tests {
    use super::*;
    #[test]
    fn interaction_ownership_requires_explicit_existing_session_claim() {
        assert!(interaction_control(&json!({}), true));
        assert!(interaction_control(
            &json!({"_meta":{"jcode.interactionController":true}}),
            false
        ));
        assert!(!interaction_control(
            &json!({"_meta":{"jcode.interactionController":false}}),
            true
        ));
    }
    #[tokio::test]
    async fn passive_attachment_never_starts_interaction_controller() {
        let (client, _server) = crate::transport::stream_pair().unwrap();
        let (reader, writer) = client.into_split();
        let runtime = AcpRuntime::new(AcpProfile::Standard, ProviderChoice::Jcode, None, None);
        runtime.form_elicitation.store(true, Ordering::SeqCst);
        runtime
            .register_session(
                DaemonSession::new("passive".into(), reader, writer, 2),
                false,
            )
            .await
            .unwrap();
        let session = runtime.sessions.lock().await["passive"].clone();
        assert!(session.interaction_pump.lock().await.is_none());
        session.stop_pump().await;
    }
}
