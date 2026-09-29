use super::{SwarmMember, fanout_session_event};
use crate::protocol::ServerEvent;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};

pub(in crate::server) fn session_event_fanout_sender_with_fallback(
    session_id: String,
    swarm_members: Arc<RwLock<HashMap<String, SwarmMember>>>,
    origin: mpsc::UnboundedSender<ServerEvent>,
) -> mpsc::UnboundedSender<ServerEvent> {
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerEvent>();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if matches!(event, ServerEvent::Done { .. } | ServerEvent::Error { .. }) {
                let observers = {
                    let mut members = swarm_members.write().await;
                    members
                        .get_mut(&session_id)
                        .map(|member| {
                            member.event_txs.retain(|_, tx| !tx.is_closed());
                            let targets = if member.event_txs.is_empty() {
                                vec![member.event_tx.clone()]
                            } else {
                                member.event_txs.values().cloned().collect()
                            };
                            targets
                                .into_iter()
                                .filter(|tx| !tx.same_channel(&origin))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                };
                // Request IDs belong to one connection. Observers receive the
                // established autonomous-turn ID, never another client's ID.
                let observed = match &event {
                    ServerEvent::Error {
                        message,
                        retry_after_secs,
                        ..
                    } => ServerEvent::Error {
                        id: 0,
                        message: message.clone(),
                        retry_after_secs: *retry_after_secs,
                    },
                    _ => ServerEvent::Done { id: 0 },
                };
                for observer in observers {
                    let _ = observer.send(observed.clone());
                }
                let _ = origin.send(event);
            } else if fanout_session_event(&swarm_members, &session_id, event.clone()).await == 0 {
                let _ = origin.send(event);
            }
        }
    });
    tx
}

#[cfg(test)]
mod tests;
