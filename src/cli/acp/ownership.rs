use super::*;

fn loss_update(session: &str) -> Value {
    json!({"sessionId":session,"update":{"sessionUpdate":"session_info_update",
        "_meta":{"jcode.interactionController":false}}})
}

impl AcpRuntime {
    pub(super) async fn lose_interaction_ownership(&self, session: &DaemonSession) {
        if !session.interaction_active.swap(false, Ordering::SeqCst) {
            return;
        }
        self.interaction_requests
            .lock()
            .await
            .retain(|_, owner| owner != &session.session_id);
        let _ = self
            .write_notification("session/update", loss_update(&session.session_id))
            .await;
    }

    pub(super) async fn require_interaction_ownership(
        &self,
        session: &DaemonSession,
    ) -> Result<()> {
        if !session.interaction_requested.load(Ordering::SeqCst) {
            return Ok(());
        }
        if !session.interaction_active.load(Ordering::SeqCst) {
            anyhow::bail!("Interaction control lost; explicitly reload the session to reacquire");
        }
        let request = json!({"method":"renew","sessionId":session.session_id,"controller":self.controller,
            "form":self.form_elicitation.load(Ordering::SeqCst)});
        if interactions::exchange(&self.broker_path, request)
            .await
            .is_err()
        {
            self.lose_interaction_ownership(session).await;
            anyhow::bail!("Interaction control lost; explicitly reload the session to reacquire");
        }
        if !session.interaction_active.load(Ordering::SeqCst) {
            anyhow::bail!("Interaction control lost; explicitly reload the session to reacquire");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
