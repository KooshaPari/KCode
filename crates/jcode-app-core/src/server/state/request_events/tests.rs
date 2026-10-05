use super::*;
use std::time::{Duration, Instant};

type Receiver = mpsc::UnboundedReceiver<ServerEvent>;

fn fixture() -> (
    Arc<RwLock<HashMap<String, SwarmMember>>>,
    mpsc::UnboundedSender<ServerEvent>,
    Receiver,
    mpsc::UnboundedSender<ServerEvent>,
    Receiver,
) {
    let (first, first_rx) = mpsc::unbounded_channel();
    let (second, second_rx) = mpsc::unbounded_channel();
    let member = SwarmMember {
        session_id: "shared".into(),
        event_tx: first.clone(),
        event_txs: HashMap::from([
            ("first".into(), first.clone()),
            ("second".into(), second.clone()),
        ]),
        working_dir: None,
        swarm_id: None,
        swarm_enabled: false,
        status: "ready".into(),
        detail: None,
        task_label: None,
        friendly_name: None,
        report_back_to_session_id: None,
        latest_completion_report: None,
        role: "agent".into(),
        joined_at: Instant::now(),
        last_status_change: Instant::now(),
        is_headless: false,
        output_tail: None,
        todo_progress: None,
        todo_items: Vec::new(),
        runtime: crate::protocol::SwarmMemberRuntime::default(),
    };
    (
        Arc::new(RwLock::new(HashMap::from([("shared".into(), member)]))),
        first,
        first_rx,
        second,
        second_rx,
    )
}

async fn receive(rx: &mut Receiver) -> ServerEvent {
    tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn equal_request_ids_on_two_clients_do_not_cross_complete_and_keep_fifo() {
    let (members, first, mut first_rx, second, mut second_rx) = fixture();
    let first_turn =
        session_event_fanout_sender_with_fallback("shared".into(), members.clone(), first);
    let second_turn = session_event_fanout_sender_with_fallback("shared".into(), members, second);
    first_turn
        .send(ServerEvent::TextDelta {
            text: "first tail".into(),
        })
        .unwrap();
    first_turn.send(ServerEvent::Done { id: 2 }).unwrap();
    assert!(
        matches!(receive(&mut first_rx).await, ServerEvent::TextDelta { text } if text == "first tail")
    );
    assert!(matches!(
        receive(&mut first_rx).await,
        ServerEvent::Done { id: 2 }
    ));
    assert!(
        matches!(receive(&mut second_rx).await, ServerEvent::TextDelta { text } if text == "first tail")
    );
    assert!(matches!(
        receive(&mut second_rx).await,
        ServerEvent::Done { id: 0 }
    ));
    assert!(
        second_rx.try_recv().is_err(),
        "second request must remain pending"
    );
    second_turn
        .send(ServerEvent::TextDelta {
            text: "second tail".into(),
        })
        .unwrap();
    second_turn.send(ServerEvent::Done { id: 2 }).unwrap();
    assert!(
        matches!(receive(&mut second_rx).await, ServerEvent::TextDelta { text } if text == "second tail")
    );
    assert!(matches!(
        receive(&mut second_rx).await,
        ServerEvent::Done { id: 2 }
    ));
    assert!(
        matches!(receive(&mut first_rx).await, ServerEvent::TextDelta { text } if text == "second tail")
    );
    assert!(matches!(
        receive(&mut first_rx).await,
        ServerEvent::Done { id: 0 }
    ));
}

#[tokio::test]
async fn error_only_fails_origin_request_and_preserves_observer_detail() {
    let (members, first, mut first_rx, _, mut second_rx) = fixture();
    let turn = session_event_fanout_sender_with_fallback("shared".into(), members, first);
    turn.send(ServerEvent::TextDelta {
        text: "tail".into(),
    })
    .unwrap();
    turn.send(ServerEvent::Error {
        id: 2,
        message: "failure".into(),
        retry_after_secs: Some(3),
    })
    .unwrap();
    for (rx, expected) in [(&mut first_rx, 2), (&mut second_rx, 0)] {
        assert!(matches!(receive(rx).await, ServerEvent::TextDelta { .. }));
        assert!(
            matches!(receive(rx).await, ServerEvent::Error { id, message, retry_after_secs: Some(3) } if id == expected && message == "failure")
        );
    }
}

#[tokio::test]
async fn disconnected_origin_never_transfers_request_completion_to_observer() {
    let (members, first, first_rx, _, mut second_rx) = fixture();
    let turn = session_event_fanout_sender_with_fallback("shared".into(), members, first);
    drop(first_rx);
    turn.send(ServerEvent::TextDelta {
        text: "tail".into(),
    })
    .unwrap();
    turn.send(ServerEvent::Done { id: 2 }).unwrap();
    assert!(matches!(
        receive(&mut second_rx).await,
        ServerEvent::TextDelta { .. }
    ));
    assert!(matches!(
        receive(&mut second_rx).await,
        ServerEvent::Done { id: 0 }
    ));
    assert!(second_rx.try_recv().is_err());
}

#[tokio::test]
async fn unattached_origin_still_receives_ordered_stream_and_completion() {
    let (origin, mut rx) = mpsc::unbounded_channel();
    let turn = session_event_fanout_sender_with_fallback(
        "missing".into(),
        Arc::new(RwLock::new(HashMap::new())),
        origin,
    );
    turn.send(ServerEvent::TextDelta {
        text: "tail".into(),
    })
    .unwrap();
    turn.send(ServerEvent::Done { id: 2 }).unwrap();
    assert!(matches!(
        receive(&mut rx).await,
        ServerEvent::TextDelta { .. }
    ));
    assert!(matches!(
        receive(&mut rx).await,
        ServerEvent::Done { id: 2 }
    ));
}
