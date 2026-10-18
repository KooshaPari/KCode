//! Session content / identity / model-selection requests.
//!
//! These are the operator-driven mutations to a session: posting messages
//! and inline images, clearing, reloading, resuming, requesting history /
//! model catalog, history rewinds, model selection, route selection, model
//! selection for subagents, premium mode, reasoning effort, service tier,
//! transport.
//!
//! Separated from `controls.rs` so the session-payload surface can evolve
//! independently from the UI plumbing in `controls.rs`.

use super::RemoteConnection;
use crate::protocol::Request;
use anyhow::Result;

impl RemoteConnection {
    pub async fn send_message(&mut self, content: String) -> Result<u64> {
        self.send_message_with_images_and_reminder(content, vec![], None)
            .await
    }

    /// Clear the server-side conversation and replace it with a fresh session.
    pub async fn clear(&mut self) -> Result<u64> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.send_request(Request::Clear { id }).await?;
        Ok(id)
    }

    /// Send a message with images to the server and return the request ID
    pub async fn send_message_with_images(
        &mut self,
        content: String,
        images: Vec<(String, String)>,
    ) -> Result<u64> {
        self.send_message_with_images_and_reminder(content, images, None)
            .await
    }

    pub async fn send_message_with_images_and_reminder(
        &mut self,
        content: String,
        images: Vec<(String, String)>,
        system_reminder: Option<String>,
    ) -> Result<u64> {
        self.send_message_with_images_reminder_and_skill(content, images, system_reminder, None)
            .await
    }

    pub async fn send_message_with_images_reminder_and_skill(
        &mut self,
        content: String,
        images: Vec<(String, String)>,
        system_reminder: Option<String>,
        active_skill: Option<String>,
    ) -> Result<u64> {
        // Output token usage snapshots are cumulative within a single API call.
        // Reset per-call watermark before sending the next user request.
        self.reset_call_output_tokens_seen();

        let id = self.next_request_id;
        let request = Request::Message {
            id,
            content,
            images,
            system_reminder,
            active_skill,
            no_reply: false,
        };
        self.next_request_id += 1;
        self.send_request(request).await?;
        Ok(id)
    }

    /// Request server reload
    pub async fn reload(&mut self) -> Result<()> {
        let request = Request::Reload {
            id: self.next_request_id,
            force: true,
        };
        self.next_request_id += 1;
        self.send_request(request).await
    }

    /// Ask the server to continue every live session that was interrupted and
    /// would auto-resume on a reload. Returns the request id so the client can
    /// correlate the `ResumeAllResult` event.
    pub async fn resume_all_sessions(&mut self) -> Result<u64> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.send_request(Request::ResumeAllSessions { id }).await?;
        Ok(id)
    }

    /// Refresh daemon usage after a client-side banked reset attempt.
    pub async fn invalidate_openai_usage(&mut self, account_label: Option<String>) -> Result<u64> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.send_request(Request::InvalidateOpenAiUsage { id, account_label })
            .await?;
        Ok(id)
    }

    /// Re-request the session history payload from the server.
    ///
    /// Used by the client-side history-recovery watchdog: if the bootstrap
    /// `History` event never arrives after a (re)connect (e.g. it was dropped
    /// during a reload handoff, or the server was momentarily busy), the client
    /// would otherwise be stuck forever on "loading session…" with every prompt
    /// gated behind `has_loaded_history()`. Sending a fresh `GetHistory` lets the
    /// server resend the payload so the session can recover without a `/restart`.
    pub async fn request_history(&mut self) -> Result<u64> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.send_request(Request::GetHistory { id }).await?;
        Ok(id)
    }

    /// Ask the server for the fully route-expanded model catalog.
    ///
    /// The bootstrap `History` payload deliberately ships model *names* only
    /// (route expansion is expensive), and live bus catalog pushes are
    /// downgraded to names-only above a size cap. Without this request a client
    /// whose persisted catalog cache is missing or stale has nothing but
    /// placeholder "remote-catalog" rows in `/model`.
    pub async fn request_model_catalog(&mut self) -> Result<u64> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.send_request(Request::GetModelCatalog {
            id,
            subscribe_usage_updates: true,
        })
        .await?;
        Ok(id)
    }

    /// Resume a specific session by ID
    pub async fn resume_session(&mut self, session_id: &str) -> Result<()> {
        let request = Request::ResumeSession {
            id: self.next_request_id,
            session_id: session_id.to_string(),
            client_instance_id: self.client_instance_id.clone(),
            client_has_local_history: false,
            allow_session_takeover: false,
        };
        self.next_request_id += 1;
        self.send_request(request).await?;
        // An explicit picker/workspace switch must accept the target's History
        // even when a SessionId event (or reload-time local restore) associated
        // this connection with the target before History arrived. Otherwise
        // `has_loaded_history` still describes the source session and the TUI
        // drops the target replay as a duplicate, leaving an orphan resume with
        // an empty transcript despite its intact persisted messages (#753).
        // History application replaces the display vector, so repeated payloads
        // remain idempotent rather than appending duplicate messages.
        self.has_loaded_history = false;
        Ok(())
    }

    /// Request a wider compacted-history window for the active session.
    pub async fn get_compacted_history(&mut self, visible_messages: usize) -> Result<u64> {
        let id = self.next_request_id;
        let request = Request::GetCompactedHistory {
            id,
            visible_messages,
        };
        self.next_request_id += 1;
        self.send_request(request).await?;
        Ok(id)
    }

    /// Ask the server to truncate the active session to a 1-based message index.
    pub async fn rewind(&mut self, message_index: usize) -> Result<u64> {
        let id = self.next_request_id;
        let request = Request::Rewind { id, message_index };
        self.next_request_id += 1;

        // The server responds by sending a fresh History payload for the same
        // session. Allow that payload to replace the current display state even
        // though this connection has already completed its initial bootstrap.
        self.has_loaded_history = false;
        self.send_request(request).await?;
        Ok(id)
    }

    /// Ask the server to undo the most recent rewind for the active session.
    pub async fn rewind_undo(&mut self) -> Result<u64> {
        let id = self.next_request_id;
        self.next_request_id += 1;

        // The server responds by sending a fresh History payload for the same
        // session. Allow that payload to replace the current display state.
        self.has_loaded_history = false;
        self.send_request(Request::RewindUndo { id }).await?;
        Ok(id)
    }

    /// Cycle the active model on the server
    pub async fn cycle_model(&mut self, direction: i8) -> Result<()> {
        let request = Request::CycleModel {
            id: self.next_request_id,
            direction,
        };
        self.next_request_id += 1;
        self.send_request(request).await
    }

    /// Trigger a background refresh of available models on the server.
    pub async fn refresh_models(&mut self) -> Result<()> {
        let request = Request::RefreshModels {
            id: self.next_request_id,
        };
        self.next_request_id += 1;
        self.send_request(request).await
    }

    /// Set the active model on the server
    pub async fn set_model(&mut self, model: &str) -> Result<u64> {
        let id = self.next_request_id;
        let request = Request::SetModel {
            id,
            model: model.to_string(),
        };
        self.next_request_id += 1;
        self.send_request(request).await?;
        Ok(id)
    }

    pub async fn set_route_selection(
        &mut self,
        selection: crate::provider::RouteSelection,
    ) -> Result<u64> {
        let id = self.next_request_id;
        let request = Request::SetRoute { id, selection };
        self.next_request_id += 1;
        self.send_request(request).await?;
        Ok(id)
    }

    /// Set or clear the session-scoped subagent model on the server.
    pub async fn set_subagent_model(&mut self, model: Option<String>) -> Result<()> {
        let request = Request::SetSubagentModel {
            id: self.next_request_id,
            model,
        };
        self.next_request_id += 1;
        self.send_request(request).await
    }

    /// Launch a subagent immediately on the active remote session.
    pub async fn run_subagent(
        &mut self,
        prompt: String,
        subagent_type: String,
        model: Option<String>,
        session_id: Option<String>,
    ) -> Result<()> {
        let request = Request::RunSubagent {
            id: self.next_request_id,
            prompt,
            subagent_type,
            model,
            session_id,
        };
        self.next_request_id += 1;
        self.send_request(request).await
    }

    /// Set Copilot premium request conservation mode on the server
    pub async fn set_premium_mode(&mut self, mode: u8) -> Result<()> {
        let request = Request::SetPremiumMode {
            id: self.next_request_id,
            mode,
        };
        self.next_request_id += 1;
        self.send_request(request).await
    }

    /// Set reasoning effort on the server (for OpenAI models)
    pub async fn set_reasoning_effort(&mut self, effort: &str) -> Result<()> {
        let request = Request::SetReasoningEffort {
            id: self.next_request_id,
            effort: effort.to_string(),
            target_session_id: None,
        };
        self.next_request_id += 1;
        self.send_request(request).await
    }

    /// Set service tier on the server (for OpenAI models)
    pub async fn set_service_tier(&mut self, service_tier: &str) -> Result<()> {
        let request = Request::SetServiceTier {
            id: self.next_request_id,
            service_tier: service_tier.to_string(),
        };
        self.next_request_id += 1;
        self.send_request(request).await
    }

    /// Set connection transport on the server (for OpenAI models)
    pub async fn set_transport(&mut self, transport: &str) -> Result<()> {
        let request = Request::SetTransport {
            id: self.next_request_id,
            transport: transport.to_string(),
        };
        self.next_request_id += 1;
        self.send_request(request).await
    }

}
