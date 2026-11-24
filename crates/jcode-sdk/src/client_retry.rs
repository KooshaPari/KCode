//! Turn-driving loop that wires [`TurnRetry`] into a one-shot `run`.
//!
//! [`JcodeClient::run`](crate::JcodeClient::run) is a convenience wrapper: stream
//! one turn, retry a transient failure in the same session, then hand back the
//! collected reply. The retry bookkeeping lives here so `client.rs` stays under
//! the file-size budget, and so the transport/API surface stays separate from
//! the recovery policy. The event decode rules are the ones `client.rs` always
//! had; they were moved here unchanged.

use crate::client::{AssistantTextMessage, JcodeClient, RunOptions, ToolCall, TurnResult, Usage};
use crate::errors::{Error, ErrorKind, Result};
use crate::turn_retry::{CONTINUATION_REMINDER, RetryDecision, TurnRetry};
use jcode_harness_api::{ApiEvent, PermissionDecision};
use std::time::{Duration, Instant};

impl JcodeClient {
    /// Drive one turn, retrying a transient failure within the same session.
    ///
    /// Returns the collected turn on `TurnDone`, or the first failure the
    /// retry policy says is not worth another attempt (including a zero
    /// budget, which surfaces the first failure).
    pub(crate) fn run_turn(
        &self,
        session_id: &str,
        content: &str,
        options: RunOptions,
    ) -> Result<TurnResult> {
        let stream = self.events(Some(session_id));
        self.send_message(
            session_id,
            content,
            options.images.clone(),
            Some(Duration::from_secs(10)),
        )?;
        let mut retry = TurnRetry::new(options.retry);
        loop {
            let mut result = TurnResult::default();
            let mut text_stream = TextCollector::default();
            // Set when this attempt failed transiently and a retry is pending.
            let mut retry_at = None;
            while let Some(event) = stream.next() {
                if let Some(on_event) = &options.on_event {
                    on_event(&event);
                }
                // Feed the recovery state machine first; it needs the TurnStopped
                // that precedes an error to know why the turn ended.
                let decision = retry.observe(&event, Instant::now());
                if let ApiEvent::Error { code, message } = &event {
                    if matches!(decision, RetryDecision::Scheduled { .. }) {
                        retry_at = retry.pending();
                        continue;
                    }
                    return Err(Error::new(ErrorKind::Harness(*code), message.clone()));
                }
                match event {
                    ApiEvent::TextDelta {
                        text, message_id, ..
                    } => {
                        text_stream.message(message_id).0.text.push_str(&text);
                    }
                    ApiEvent::TextReplace {
                        text, message_id, ..
                    } => {
                        text_stream.message(message_id).0.text = text;
                    }
                    ApiEvent::TextDone { message_id, .. } => {
                        if let Some(index) = text_stream.index(&message_id) {
                            text_stream.parts[index].1 = true;
                        }
                    }
                    ApiEvent::ReasoningDelta { text, .. } => result.reasoning.push_str(&text),
                    ApiEvent::ToolDone {
                        call_id,
                        name,
                        output,
                        error,
                        ..
                    } => result.tool_calls.push(ToolCall {
                        call_id,
                        name,
                        output,
                        error,
                    }),
                    ApiEvent::TokenUsage {
                        input,
                        output,
                        cache_read_input,
                        cache_creation_input,
                        ..
                    } => {
                        result.usage = Some(Usage {
                            input,
                            output,
                            cache_read_input,
                            cache_creation_input,
                        })
                    }
                    ApiEvent::PermissionRequest { request_id, .. } if options.auto_approve => {
                        self.respond_to_permission(
                            session_id,
                            &request_id,
                            PermissionDecision::Allow,
                        )?;
                    }
                    ApiEvent::TurnStopped {
                        reason, message, ..
                    } => {
                        result.stop_reason = Some(reason);
                        result.stop_message = Some(message);
                    }
                    ApiEvent::TurnDone { .. } => {
                        text_stream.finish(&mut result);
                        if retry_at.is_some() {
                            // The failed attempt closed; wait out the backoff and
                            // resume the same session instead of reporting it.
                            break;
                        }
                        return Ok(result);
                    }
                    _ => {}
                }
            }
            let Some(at) = retry_at else {
                return Err(self.disconnect_error());
            };
            let now = Instant::now();
            if at > now {
                std::thread::sleep(at.duration_since(now) + Duration::from_millis(1));
            }
            if !retry.take_due(Instant::now()) {
                return Err(self.disconnect_error());
            }
            // The failed prompt is already in the transcript, so the retry is a
            // hidden continuation rather than a resend.
            self.send_system_reminder(session_id, CONTINUATION_REMINDER)?;
        }
    }
}

/// Aggregates text events into the framed messages of one turn.
#[derive(Default)]
struct TextCollector {
    parts: Vec<(AssistantTextMessage, bool)>,
}

impl TextCollector {
    fn index(&self, id: &Option<String>) -> Option<usize> {
        self.parts
            .iter()
            .rposition(|(part, done)| &part.message_id == id && (id.is_some() || !done))
    }

    fn message(&mut self, id: Option<String>) -> &mut (AssistantTextMessage, bool) {
        let index = self.index(&id).unwrap_or_else(|| {
            self.parts.push((
                AssistantTextMessage {
                    message_id: id,
                    text: String::new(),
                },
                false,
            ));
            self.parts.len() - 1
        });
        &mut self.parts[index]
    }

    fn finish(self, result: &mut TurnResult) {
        result.text = self
            .parts
            .iter()
            .map(|(part, _)| part.text.as_str())
            .collect();
        result.messages = self
            .parts
            .into_iter()
            .filter_map(|(part, done)| (done && !part.text.is_empty()).then_some(part))
            .collect();
        result.final_text = result
            .messages
            .last()
            .map(|part| part.text.clone())
            .unwrap_or_else(|| result.text.clone());
    }
}
