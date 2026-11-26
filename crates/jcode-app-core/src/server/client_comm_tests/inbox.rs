//! Routing only: caller-supplied sender identity is explicitly unverified.
use super::*;

struct CommFixture {
    sender_id: String,
    target_id: String,
    sessions: Arc<RwLock<HashMap<String, Arc<Mutex<Agent>>>>>,
    soft_interrupt_queues: SessionInterruptQueues,
    swarm_members: Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    channel_subscriptions: Arc<RwLock<HashMap<String, HashMap<String, HashSet<String>>>>>,
    event_history: Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>,
    event_counter: Arc<AtomicU64>,
    swarm_event_tx: broadcast::Sender<SwarmEvent>,
    client_connections: Arc<RwLock<HashMap<String, ClientConnectionInfo>>>,
    client_event_tx: mpsc::UnboundedSender<ServerEvent>,
    client_event_rx: mpsc::UnboundedReceiver<ServerEvent>,
    target_event_rx: mpsc::UnboundedReceiver<ServerEvent>,
}

fn comm_member(
    session_id: &str,
    event_tx: mpsc::UnboundedSender<ServerEvent>,
    swarm_id: &str,
    friendly_name: &str,
    role: &str,
) -> SwarmMember {
    SwarmMember {
        session_id: session_id.to_string(),
        event_tx,
        event_txs: HashMap::new(),
        working_dir: None,
        swarm_id: Some(swarm_id.to_string()),
        swarm_enabled: true,
        status: "ready".to_string(),
        detail: None,
        friendly_name: Some(friendly_name.to_string()),
        report_back_to_session_id: None,
        latest_completion_report: None,
        role: role.to_string(),
        joined_at: Instant::now(),
        last_status_change: Instant::now(),
        is_headless: false,
        output_tail: None,
        todo_progress: None,
        todo_items: Vec::new(),
        runtime: crate::protocol::SwarmMemberRuntime::default(),
        task_label: None,
    }
}

/// Two registered sessions, each in its own swarm unless the same id is passed.
async fn comm_fixture(sender_swarm: &str, target_swarm: &str) -> CommFixture {
    let sender = test_agent().await;
    let target = test_agent().await;
    let sender_id = sender.lock().await.session_id().to_string();
    let target_id = target.lock().await.session_id().to_string();

    let sessions = Arc::new(RwLock::new(HashMap::from([
        (sender_id.clone(), sender),
        (target_id.clone(), target),
    ])));

    let (sender_event_tx, _sender_event_rx) = mpsc::unbounded_channel();
    let (target_event_tx, target_event_rx) = mpsc::unbounded_channel();

    let mut members = HashMap::new();
    members.insert(
        sender_id.clone(),
        comm_member(&sender_id, sender_event_tx, sender_swarm, "falcon", "agent"),
    );
    members.insert(
        target_id.clone(),
        comm_member(&target_id, target_event_tx, target_swarm, "bear", "agent"),
    );

    let mut swarms = HashMap::new();
    swarms
        .entry(sender_swarm.to_string())
        .or_insert_with(HashSet::new)
        .insert(sender_id.clone());
    swarms
        .entry(target_swarm.to_string())
        .or_insert_with(HashSet::new)
        .insert(target_id.clone());

    let (client_event_tx, client_event_rx) = mpsc::unbounded_channel();
    let (swarm_event_tx, _) = broadcast::channel(16);

    CommFixture {
        sender_id,
        target_id,
        sessions,
        soft_interrupt_queues: Arc::new(RwLock::new(HashMap::new())),
        swarm_members: Arc::new(RwLock::new(members)),
        swarms_by_id: Arc::new(RwLock::new(swarms)),
        channel_subscriptions: Arc::new(RwLock::new(HashMap::new())),
        event_history: Arc::new(RwLock::new(std::collections::VecDeque::new())),
        event_counter: Arc::new(AtomicU64::new(0)),
        swarm_event_tx,
        client_connections: Arc::new(RwLock::new(HashMap::new())),
        client_event_tx,
        client_event_rx,
        target_event_rx,
    }
}

async fn send_dm(fixture: &mut CommFixture, id: u64, to_session: String) {
    handle_comm_message(
        id,
        fixture.sender_id.clone(),
        "hello".to_string(),
        Some(to_session),
        None,
        Some(CommDeliveryMode::Notify),
        None,
        None,
        None,
        &fixture.client_event_tx,
        &fixture.sessions,
        &fixture.soft_interrupt_queues,
        &fixture.swarm_members,
        &fixture.swarms_by_id,
        &fixture.channel_subscriptions,
        &fixture.event_history,
        &fixture.event_counter,
        &fixture.swarm_event_tx,
        &fixture.client_connections,
    )
    .await;
}

/// A session in a different swarm must be refused by its exact id, and must receive nothing.
///
/// Pinned to the wording measured on the wire. `to_session` resolution is filtered by
/// `swarm_session_ids`, so a foreign session is `Unknown target ...`, not
/// `not in swarm` (`client_comm_message.rs:37-48` vs the second-line guard at 265-290).
#[tokio::test]
async fn comm_message_rejects_foreign_session_by_exact_id() {
    let mut fixture = comm_fixture("swarm-sender", "swarm-target").await;
    let target_id = fixture.target_id.clone();

    send_dm(&mut fixture, 7, target_id).await;

    match fixture
        .client_event_rx
        .recv()
        .await
        .expect("sender error event")
    {
        ServerEvent::Error { id, message, .. } => {
            assert_eq!(id, 7);
            assert!(
                message.contains("Unknown target"),
                "expected `Unknown target` rejection, got: {message}"
            );
        }
        other => panic!("expected Error, got {other:?}"),
    }

    let leaked =
        tokio::time::timeout(Duration::from_millis(200), fixture.target_event_rx.recv()).await;
    assert!(leaked.is_err(), "foreign session must receive nothing");
}

/// A DM inside one swarm reaches the target with `scope: "dm"` and the sender gets `Done{id}`.
#[tokio::test]
async fn comm_message_dm_emits_done_and_dm_scoped_notification() {
    let mut fixture = comm_fixture("swarm-shared", "swarm-shared").await;
    let sender_id = fixture.sender_id.clone();
    let target_id = fixture.target_id.clone();

    send_dm(&mut fixture, 9, target_id).await;

    match fixture
        .target_event_rx
        .recv()
        .await
        .expect("target notification")
    {
        ServerEvent::Notification {
            from_session,
            from_name,
            notification_type,
            message,
        } => {
            assert_eq!(from_session, sender_id);
            assert_eq!(from_name.as_deref(), Some("falcon"));
            match notification_type {
                NotificationType::Message { scope, channel, .. } => {
                    assert_eq!(scope.as_deref(), Some("dm"));
                    assert_eq!(channel, None);
                }
                other => panic!("unexpected notification type: {other:?}"),
            }
            assert_eq!(message, "DM from falcon: hello");
        }
        other => panic!("expected Notification, got {other:?}"),
    }

    match fixture
        .client_event_rx
        .recv()
        .await
        .expect("sender done event")
    {
        ServerEvent::Done { id } => assert_eq!(id, 9),
        other => panic!("expected Done, got {other:?}"),
    }
}
