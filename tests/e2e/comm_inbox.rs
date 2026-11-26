//! Isolated end-to-end proof of native cross-session inbox delivery.
//!
//! Everything here runs in-process: a real `server::Server` on a real unix
//! socket, a `MockProvider`, and raw wire clients. No pane, no TUI, no
//! terminal, no provider request, no external process.
//!
//! The test proves four things, in the order the daemon decides them:
//!
//! 1. **Default deny.** Two independently created root sessions each own a
//!    swarm (`session:<id>`, see `server/util.rs` `swarm_id_for_session`), so a
//!    DM from one to the other is rejected and nothing is delivered. Measured
//!    wording: `Unknown target '<id>' - use an exact session_id or a unique
//!    friendly name within the swarm.` The member of another swarm is not even
//!    visible at resolution time, so the sender cannot probe for foreign ids.
//!    This is the current behaviour for two separately launched panes, and it
//!    is why cross-pane messaging does not work out of the box.
//! 2. **Shared swarm delivers.** Two sessions that subscribe after
//!    `KCODE_SWARM_ID` is set do share a swarm, so the DM lands as a typed
//!    `Notification { notification_type: Message { scope: "dm" } }` on the
//!    recipient and the sender receives the request-level ack `Done { id }`.
//! 3. **`delivery: "notify"` starts no turn.** The recipient emits no
//!    `TextDelta`, no `MessageEnd` and no `TurnStopped` inside a quiet window.
//!    This is the property that makes an inbox usable without hijacking a pane.
//! 4. **A normal `message` is unaffected.** The same transport still produces
//!    an ordered `MessageEnd` followed by `Done { id }`.
//!
//! `delivery: "notify"` is explicit because the `dm` default is `Wake`, and a
//! wake would start a turn and invalidate (3).
//!
//! Sender authentication is UNVERIFIED: `from_session` is caller-supplied.
//! No connection-to-sender binding, spoofing protection or read receipt is tested.
//! Quiet windows bound observed turn events, not persisted transcript changes.
//! Independent-root rejection does not establish universal cross-swarm denial.
//!
//! Run with:
//!   cargo test --test e2e comm_inbox -- --nocapture --test-threads=1

use crate::test_support::*;
use jcode::protocol::ServerEvent;
use jcode::server;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// One raw wire client: subscribe, then write requests and read `ServerEvent`s.
///
/// The read/write halves are boxed because `server::connect_socket` returns the
/// transport `Stream` enum (unix / tcp / websocket), not a `tokio::net::UnixStream`.
struct RawClient {
    reader: BufReader<Box<dyn tokio::io::AsyncRead + Unpin + Send>>,
    writer: Box<dyn tokio::io::AsyncWrite + Unpin + Send>,
    session_id: Option<String>,
}

impl RawClient {
    async fn subscribe(socket: &Path, subscribe_id: u64) -> Result<Self> {
        let connection = server::connect_socket(socket).await?;
        let (reader, writer) = connection.into_split();
        let mut client = Self {
            reader: BufReader::new(Box::new(reader)),
            writer: Box::new(writer),
            session_id: None,
        };
        client
            .send(serde_json::json!({
                "type": "subscribe",
                "id": subscribe_id,
                "working_dir": std::env::current_dir()?,
                "crash_on_disconnect": true,
                "continue_on_disconnect": true,
            }))
            .await?;
        // Subscribe answers with `SessionId` and then `Done { id }`; a separate
        // `subscribe_client` helper in test_support does the same over `Client`.
        loop {
            match client.next_event().await? {
                ServerEvent::Done { id } if id == subscribe_id => break,
                ServerEvent::Error { message, .. } => anyhow::bail!("subscribe: {message}"),
                _ => {}
            }
        }
        anyhow::ensure!(
            client.session_id.is_some(),
            "subscribe {subscribe_id} never reported a session id"
        );
        Ok(client)
    }

    fn id(&self) -> &str {
        self.session_id.as_deref().unwrap_or_default()
    }

    async fn send(&mut self, value: serde_json::Value) -> Result<()> {
        self.writer
            .write_all(format!("{value}\n").as_bytes())
            .await?;
        self.writer.flush().await?;
        Ok(())
    }

    async fn next_event(&mut self) -> Result<ServerEvent> {
        let mut line = String::new();
        let read = timeout(Duration::from_secs(5), self.reader.read_line(&mut line)).await??;
        anyhow::ensure!(read > 0, "unexpected EOF from server");
        let event: ServerEvent = serde_json::from_str(&line)?;
        if let ServerEvent::SessionId { session_id } = &event {
            self.session_id = Some(session_id.clone());
        }
        Ok(event)
    }

    /// Read events until `Done`/`Error` for `expected_id` or the deadline.
    async fn collect(&mut self, expected_id: u64) -> Result<Vec<ServerEvent>> {
        let mut events = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let event = self.next_event().await?;
            let terminal = matches!(
                &event,
                ServerEvent::Done { id } | ServerEvent::Error { id, .. } if *id == expected_id
            );
            events.push(event);
            if terminal {
                return Ok(events);
            }
        }
        anyhow::bail!("timed out waiting for id {expected_id}; saw {events:?}");
    }

    /// Read whatever arrives inside `window`, without needing a terminal id.
    async fn drain_events(&mut self, window: Duration) -> Result<Vec<ServerEvent>> {
        let mut events = Vec::new();
        let deadline = Instant::now() + window;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let mut line = String::new();
            match timeout(remaining, self.reader.read_line(&mut line)).await {
                Err(_) => break,
                Ok(Err(err)) => return Err(err.into()),
                Ok(Ok(0)) => anyhow::bail!("unexpected EOF from server"),
                Ok(Ok(_)) => {
                    let event: ServerEvent = serde_json::from_str(&line)?;
                    if let ServerEvent::SessionId { session_id } = &event {
                        self.session_id = Some(session_id.clone());
                    }
                    events.push(event);
                }
            }
        }
        Ok(events)
    }

    /// Assert that no turn-producing event arrives inside `window`.
    async fn assert_quiet(&mut self, window: Duration) -> Result<()> {
        let deadline = Instant::now() + window;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match timeout(remaining, self.next_event()).await {
                Err(_) => return Ok(()),
                Ok(Err(err)) => return Err(err),
                Ok(Ok(event)) => anyhow::ensure!(
                    !matches!(
                        event,
                        ServerEvent::TextDelta { .. }
                            | ServerEvent::MessageEnd { .. }
                            | ServerEvent::TurnStopped { .. }
                    ),
                    "a notify delivery started a turn: {event:?}"
                ),
            }
        }
        Ok(())
    }
}

fn notification_scope(event: &ServerEvent) -> Option<(&str, &str, &str)> {
    match event {
        ServerEvent::Notification {
            from_session,
            notification_type: jcode::protocol::NotificationType::Message { scope, .. },
            message,
            ..
        } => Some((scope.as_deref().unwrap_or(""), from_session, message)),
        _ => None,
    }
}

#[tokio::test]
async fn comm_inbox_rejects_wrong_swarm_then_delivers_with_ack() -> Result<()> {
    let _env = setup_test_env()?;
    // Two sessions created before this is set own their own swarm.
    let runtime = tempfile::tempdir()?;
    let socket = runtime.path().join("server.sock");
    let debug_socket = runtime.path().join("debug.sock");

    let provider = Arc::new(MockProvider::new());
    provider.queue_response(vec![
        jcode::message::StreamEvent::TextDelta("ok".into()),
        jcode::message::StreamEvent::MessageEnd {
            stop_reason: Some("end_turn".into()),
        },
    ]);
    let server = server::Server::new_with_paths(
        provider as Arc<dyn Provider>,
        socket.clone(),
        debug_socket.clone(),
    );
    let handle = tokio::spawn(async move { server.run().await });

    let result = async {
        wait_for_server_ready(&socket, &debug_socket).await?;

        // ---- 1. default deny: two independent root sessions are separate swarms
        let mut solo_a = RawClient::subscribe(&socket, 1).await?;
        let mut solo_b = RawClient::subscribe(&socket, 2).await?;
        let (a_id, b_id) = (solo_a.id().to_string(), solo_b.id().to_string());
        anyhow::ensure!(
            a_id != b_id,
            "two subscribes must create two distinct sessions"
        );

        solo_a
            .send(serde_json::json!({
                "type": "comm_message",
                "id": 10,
                "from_session": a_id,
                "to_session": b_id,
                "message": "should be rejected",
                "delivery": "notify",
            }))
            .await?;
        let rejected = solo_a.collect(10).await?;
        let message = rejected
            .iter()
            .find_map(|event| match event {
                ServerEvent::Error { id: 10, message, .. } => Some(message.clone()),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("expected an Error for id 10, saw {rejected:?}"))?;
        anyhow::ensure!(
            message.contains("Unknown target") || message.contains("not in swarm"),
            "expected a swarm-scoped rejection, got: {message}"
        );
        println!("default-deny rejection: {message}");
        // Nothing was delivered to the other session.
        let rejected_delivery = solo_b.drain_events(Duration::from_millis(400)).await?;
        anyhow::ensure!(
            rejected_delivery.is_empty(),
            "foreign recipient received events: {rejected_delivery:?}"
        );

        // ---- 2. shared swarm: the same DM now delivers, with a sender ack
        let _swarm = EnvVarGuard::set("KCODE_SWARM_ID", "e2e-comm-inbox");
        let mut shared_a = RawClient::subscribe(&socket, 3).await?;
        let mut shared_b = RawClient::subscribe(&socket, 4).await?;
        let (from_id, to_id) = (shared_a.id().to_string(), shared_b.id().to_string());

        shared_a
            .send(serde_json::json!({
                "type": "comm_message",
                "id": 20,
                "from_session": from_id,
                "to_session": to_id,
                "message": "inbox delivery works",
                "tldr": "one line",
                "delivery": "notify",
            }))
            .await?;

        println!("phase2: DM sent from {from_id} to {to_id}; waiting for ack");
        let acked = shared_a.collect(20).await?;
        println!("phase2: ack received: {acked:?}");
        anyhow::ensure!(
            acked
                .iter()
                .any(|event| matches!(event, ServerEvent::Done { id: 20 })),
            "sender must receive Done {{ id: 20 }}, saw {acked:?}"
        );
        let ack_at = acked
            .iter()
            .position(|event| matches!(event, ServerEvent::Ack { id: 20 }));
        let done_at = acked
            .iter()
            .position(|event| matches!(event, ServerEvent::Done { id: 20 }));
        anyhow::ensure!(
            ack_at.is_some() && ack_at < done_at,
            "Ack must precede Done: {acked:?}"
        );

        // Liveness probe on the recipient connection: a `Pong` proves the socket
        // is readable, so an absent `Notification` is a delivery fact rather
        // than a client-side read fault.
        shared_b
            .send(serde_json::json!({ "type": "ping", "id": 21 }))
            .await?;
        let delivered = shared_b.drain_events(Duration::from_secs(3)).await?;
        anyhow::ensure!(
            !delivered.iter().any(|event| matches!(
                event,
                ServerEvent::TextDelta { .. }
                    | ServerEvent::MessageEnd { .. }
                    | ServerEvent::TurnStopped { .. }
            )),
            "notify delivery emitted turn events: {delivered:?}"
        );
        println!("phase2: recipient saw {delivered:?}");
        let scope = delivered
            .iter()
            .find_map(notification_scope)
            .ok_or_else(|| anyhow::anyhow!("no Message notification arrived: {delivered:?}"))?;
        anyhow::ensure!(scope.0 == "dm", "expected scope dm, got {:?}", scope.0);
        anyhow::ensure!(
            scope.1 == from_id,
            "notification must name the sender, got {:?}",
            scope.1
        );
        anyhow::ensure!(
            scope.2.contains("inbox delivery works"),
            "notification body must carry the message, got {:?}",
            scope.2
        );
        // Push, not poll: the notification must land *before* the reply to a
        // request the recipient sends afterwards, so delivery does not depend on
        // the recipient making another call.
        let notify_at = delivered
            .iter()
            .position(|event| notification_scope(event).is_some())
            .ok_or_else(|| anyhow::anyhow!("no notification in {delivered:?}"))?;
        let ping_at = delivered
            .iter()
            .position(|event| matches!(event, ServerEvent::Pong { id: 21, .. }))
            .ok_or_else(|| anyhow::anyhow!("recipient connection was not readable: {delivered:?}"))?;
        anyhow::ensure!(
            notify_at < ping_at,
            "notification must be pushed before the later ping reply, saw {delivered:?}"
        );
        println!("delivered as dm notification from {from_id} to {to_id} (notify at {notify_at}, pong at {ping_at})");

        // ---- 3. notify never starts a turn
        shared_b.assert_quiet(Duration::from_millis(500)).await?;

        // ---- 4. an ordinary message still runs a turn, ordered MessageEnd -> Done
        shared_b
            .send(serde_json::json!({
                "type": "message",
                "id": 30,
                "content": "hello",
                "no_reply": false,
            }))
            .await?;
        let turn = shared_b.collect(30).await?;
        let end_at = turn
            .iter()
            .position(|event| matches!(event, ServerEvent::MessageEnd { .. }))
            .ok_or_else(|| anyhow::anyhow!("no MessageEnd in {turn:?}"))?;
        let done_at = turn
            .iter()
            .position(|event| matches!(event, ServerEvent::Done { id: 30 }))
            .ok_or_else(|| anyhow::anyhow!("no Done {{ id: 30 }} in {turn:?}"))?;
        anyhow::ensure!(
            end_at < done_at,
            "MessageEnd must precede Done, saw {turn:?}"
        );
        println!(
            "normal turn ordered correctly: MessageEnd at {end_at}, Done at {done_at}"
        );

        Ok::<_, anyhow::Error>(())
    }
    .await;

    abort_server_and_cleanup(&handle, &socket, &debug_socket);
    result
}
