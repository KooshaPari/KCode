//! Control-plane requests on [`super::RemoteConnection`].
//!
//! Operator/UI plumbing requests that aren't session content: feature toggles,
//! compaction / saved-state, transcript injection, shell + stdin plumbing,
//! cancel / interrupt / background / handoff, auth account switching, and
//! client debug responses.
//!
//! These are deliberately separated from `actions.rs` (which holds content /
//! identity / model selection) so that session-payload changes and UI
//! plumbing can evolve independently.

use super::RemoteConnection;
use crate::protocol::{AuthChanged, FeatureToggle, Request};
use anyhow::Result;

impl RemoteConnection {
    pub async fn set_feature(&mut self, feature: FeatureToggle, enabled: bool) -> Result<()> {
        let request = Request::SetFeature {
            id: self.next_request_id,
            feature,
            enabled,
        };
        self.next_request_id += 1;
        self.send_request(request).await
    }

    /// Set compaction mode on the server for this session.
    pub async fn set_compaction_mode(&mut self, mode: crate::config::CompactionMode) -> Result<()> {
        let request = Request::SetCompactionMode {
            id: self.next_request_id,
            mode,
        };
        self.next_request_id += 1;
        self.send_request(request).await
    }

    /// Bookmark or unbookmark the active session on the server.
    pub async fn set_session_saved(&mut self, saved: bool, label: Option<String>) -> Result<()> {
        let request = Request::SetSessionSaved {
            id: self.next_request_id,
            saved,
            label,
        };
        self.next_request_id += 1;
        self.send_request(request).await
    }

    /// Set or clear the custom session display title on the server.
    /// When `target_session_id` is set, renames that specific session
    /// (used by the coordinator to rename swarm members).
    pub async fn rename_session(
        &mut self,
        title: Option<String>,
        target_session_id: Option<String>,
    ) -> Result<()> {
        let request = match target_session_id {
            Some(target_session_id) => Request::RenameSwarmSession {
                id: self.next_request_id,
                title,
                target_session_id,
            },
            None => Request::RenameSession {
                id: self.next_request_id,
                title,
            },
        };
        self.next_request_id += 1;
        self.send_request(request).await
    }

    /// Inject externally transcribed text into the active remote TUI session.
    pub async fn send_transcript(
        &mut self,
        text: String,
        mode: crate::protocol::TranscriptMode,
    ) -> Result<()> {
        let request = Request::Transcript {
            id: self.next_request_id,
            text,
            mode,
            session_id: self.session_id.clone(),
        };
        self.next_request_id += 1;
        self.send_request(request).await
    }

    /// Execute a `!cmd` shell command in the active remote session.
    pub async fn send_input_shell(&mut self, command: String) -> Result<u64> {
        let id = self.next_request_id;
        let request = Request::InputShell { id, command };
        self.next_request_id += 1;
        self.send_request(request).await?;
        Ok(id)
    }

    /// Send stdin input back to a running command
    pub async fn send_stdin_response(&mut self, request_id: &str, input: &str) -> Result<()> {
        let request = Request::StdinResponse {
            id: self.next_request_id,
            request_id: request_id.to_string(),
            input: input.to_string(),
        };
        self.next_request_id += 1;
        self.send_request(request).await
    }

    /// Cancel the current generation on the server
    pub async fn cancel(&mut self) -> Result<()> {
        self.cancel_with_reason("remote.cancel").await
    }

    /// Cancel the current generation on the server, tagging logs with the UI trigger.
    pub async fn cancel_with_reason(&mut self, reason: &'static str) -> Result<()> {
        let request = Request::Cancel {
            id: self.next_request_id,
        };
        self.next_request_id += 1;
        self.send_request_with_interrupt_trigger(request, Some(reason))
            .await
    }

    /// Move the currently executing tool to background
    pub async fn background_tool(&mut self) -> Result<()> {
        let request = Request::BackgroundTool {
            id: self.next_request_id,
        };
        self.next_request_id += 1;
        self.send_request_with_interrupt_trigger(request, Some("background_tool"))
            .await
    }

    /// Queue a soft interrupt message to be injected at the next safe point
    /// This doesn't cancel anything - the message is naturally incorporated
    pub async fn soft_interrupt(
        &mut self,
        content: String,
        images: Vec<(String, String)>,
        urgent: bool,
    ) -> Result<u64> {
        let id = self.next_request_id;
        let request = Request::SoftInterrupt {
            id,
            content,
            images,
            urgent,
        };
        self.next_request_id += 1;
        self.send_request_with_interrupt_trigger(request, Some("soft_interrupt"))
            .await?;
        Ok(id)
    }

    pub async fn cancel_soft_interrupts(&mut self) -> Result<()> {
        let request = Request::CancelSoftInterrupts {
            id: self.next_request_id,
        };
        self.next_request_id += 1;
        self.send_request_with_interrupt_trigger(request, Some("cancel_soft_interrupts"))
            .await
    }

    /// Split the current session - ask server to clone conversation into a new session
    pub async fn split(&mut self) -> Result<u64> {
        let id = self.next_request_id;
        let request = Request::Split { id };
        self.next_request_id += 1;
        self.send_request(request).await?;
        Ok(id)
    }

    /// Transfer the current session into a compacted handoff session
    pub async fn transfer(&mut self) -> Result<u64> {
        let id = self.next_request_id;
        let request = Request::Transfer { id };
        self.next_request_id += 1;
        self.send_request(request).await?;
        Ok(id)
    }

    /// Trigger manual context compaction on the server
    pub async fn compact(&mut self) -> Result<u64> {
        let id = self.next_request_id;
        let request = Request::Compact { id };
        self.next_request_id += 1;
        self.send_request(request).await?;
        Ok(id)
    }

    /// Trigger immediate memory extraction on the server for the active session.
    pub async fn trigger_memory_extraction(&mut self) -> Result<()> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.send_request(Request::TriggerMemoryExtraction { id })
            .await
    }

    /// Notify the server that auth credentials changed (e.g., after login)
    pub async fn notify_auth_changed(&mut self) -> Result<()> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.send_request(Request::NotifyAuthChanged {
            id,
            provider: None,
            auth: None,
            prefer_strongest: false,
        })
        .await
    }

    /// Notify the server about a typed auth lifecycle change and report
    /// transport failure to the caller.
    pub async fn notify_auth_changed_event(
        &mut self,
        provider: Option<&str>,
        auth: Option<AuthChanged>,
        prefer_strongest: bool,
    ) -> Result<()> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.send_request(Request::NotifyAuthChanged {
            id,
            provider: provider.map(str::to_string),
            auth,
            prefer_strongest,
        })
        .await
    }

    /// Notify the server about auth changes without blocking the caller.
    pub fn notify_auth_changed_detached(&mut self) {
        self.notify_auth_changed_for_provider_detached(None);
    }

    /// Notify the server about a provider-specific auth change without blocking the caller.
    pub fn notify_auth_changed_for_provider_detached(&mut self, provider: Option<&str>) {
        self.notify_auth_changed_detached_event(provider, None, false);
    }

    /// Notify the server about a typed auth lifecycle change without blocking the caller.
    pub fn notify_auth_changed_detached_event(
        &mut self,
        provider: Option<&str>,
        auth: Option<AuthChanged>,
        prefer_strongest: bool,
    ) {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.send_request_detached(
            Request::NotifyAuthChanged {
                id,
                provider: provider.map(str::to_string),
                auth,
                prefer_strongest,
            },
            "notify_auth_changed",
        );
    }

    /// Ask server to switch active Anthropic account for this process/session.
    pub async fn switch_anthropic_account(&mut self, label: &str) -> Result<()> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.send_request(Request::SwitchAnthropicAccount {
            id,
            label: label.to_string(),
        })
        .await
    }

    /// Ask server to switch active OpenAI account for this process/session.
    pub async fn switch_openai_account(&mut self, label: &str) -> Result<()> {
        let id = self.next_request_id;
        self.next_request_id += 1;
        self.send_request(Request::SwitchOpenAiAccount {
            id,
            label: label.to_string(),
        })
        .await
    }

    /// Send a response for a client debug request
    pub async fn send_client_debug_response(&mut self, id: u64, output: String) -> Result<()> {
        self.send_request(Request::ClientDebugResponse { id, output })
            .await
    }

    /// Read the next event from the server.
}
