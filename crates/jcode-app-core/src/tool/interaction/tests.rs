use super::*;
fn insert(session: &str, id: &str) -> oneshot::Receiver<Value> {
    let (response, receiver) = oneshot::channel();
    pending().lock().unwrap().insert(id.into(), Operation {session:session.into(),response,
            request:json!({"method":"session/request_permission","params":{"options":[{"optionId":"allow-once"},{"optionId":"reject-once"}]}})});
    receiver
}
fn answer(session: &str, id: &str, option: &str) -> Value {
    json!({"method":"respond","controller":"test-owner","sessionId":session,"id":id,"result":{"outcome":{"outcome":"selected","optionId":option}}})
}
#[tokio::test]
async fn same_operation_allows_once_rejects_wrong_session_duplicate_and_bad_option() {
    let id = crate::id::new_id("test");
    let session = crate::id::new_id("session");
    let mut receiver = insert(&session, &id);
    assert!(dispatch(answer("wrong-session", &id, "allow-once")).is_err());
    assert!(dispatch(answer(&session, &id, "invented")).is_err());
    assert!(receiver.try_recv().is_err());
    dispatch(answer(&session, &id, "allow-once")).unwrap();
    assert_eq!(receiver.await.unwrap()["outcome"]["optionId"], "allow-once");
    assert!(dispatch(answer(&session, &id, "allow-once")).is_err());
}
#[tokio::test]
async fn denial_and_cancel_resolve_the_original_waiter() {
    let session = crate::id::new_id("session");
    let id = crate::id::new_id("test");
    let receiver = insert(&session, &id);
    dispatch(answer(&session, &id, "reject-once")).unwrap();
    assert_eq!(
        receiver.await.unwrap()["outcome"]["optionId"],
        "reject-once"
    );
    let receiver = insert(&session, &id);
    dispatch(json!({"method":"cancel","sessionId":session,"controller":"test-owner"})).unwrap();
    assert_eq!(receiver.await.unwrap()["outcome"]["outcome"], "cancelled");
}
#[test]
fn dropped_or_expired_operations_cannot_be_resumed() {
    let id = crate::id::new_id("test");
    let session = crate::id::new_id("session");
    let mut receiver = insert(&session, &id);
    drop(Guard(id.clone()));
    assert!(receiver.try_recv().is_err());
    assert!(dispatch(answer(&session, &id, "allow-once")).is_err());
}
#[test]
fn elicitation_rejects_values_outside_the_declared_schema() {
    let request = json!({"method":"elicitation/create","params":{"requestedSchema":{"properties":{"value":{"type":"string","enum":["a","b"]}}}}});
    assert!(
        validate(
            &request,
            &json!({"action":"accept","content":{"value":"a"}})
        )
        .is_ok()
    );
    assert!(
        validate(
            &request,
            &json!({"action":"accept","content":{"value":"c"}})
        )
        .is_err()
    );
    assert!(
        validate(
            &request,
            &json!({"action":"accept","content":{"value":true}})
        )
        .is_err()
    );
    assert!(validate(&request, &json!({"action":"decline"})).is_ok());
}

#[test]
fn controller_lease_is_exclusive_and_reconnect_recovers_pending_request() {
    let session = crate::id::new_id("session");
    let id = crate::id::new_id("test");
    let _receiver = insert(&session, &id);
    let query =
        |owner: &str| json!({"method":"list","sessionId":session,"controller":owner,"form":true});
    assert_eq!(
        dispatch(query("first")).unwrap()["requests"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(remotely_controlled(&session));
    assert!(dispatch(query("second")).is_err());
    controllers().lock().unwrap().get_mut(&session).unwrap().1 =
        std::time::Instant::now() - std::time::Duration::from_secs(1);
    assert!(!remotely_controlled(&session));
    assert_eq!(
        dispatch(query("second")).unwrap()["requests"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    drop(Guard(id));
}
