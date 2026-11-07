use super::{PollDecision, poll_decision};

#[test]
fn observed_exit_and_deadline_remain_distinct() {
    assert_eq!(poll_decision(Ok(Some(true)), true), PollDecision::Accepted);
    assert_eq!(poll_decision(Ok(Some(false)), true), PollDecision::Failed);
    assert_eq!(poll_decision(Err(()), false), PollDecision::Failed);
    assert_eq!(poll_decision(Ok(None), false), PollDecision::Continue);
    assert_eq!(poll_decision(Ok(None), true), PollDecision::Unconfirmed);
}
