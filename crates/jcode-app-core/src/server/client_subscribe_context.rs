//! The context a client subscribe resolves before any turn runs.
//!
//! Two independent facts come out of the very first `Subscribe` request: the
//! system prompt a brand-new session inherits, and the terminal environment the
//! client asked for. Both are decided once, at connect time, and both are
//! pure functions of the request, so they live together here instead of in
//! the much larger client-lifecycle module.

use crate::protocol::Request;

/// The system prompt a brand-new session inherits, if any.
///
/// A reattachment names an existing session, so an override that arrives with
/// it must not be applied to a session that already has one.
pub(super) fn new_session_system_prompt<'a>(
    provisional_session: bool,
    target_session_id: Option<&str>,
    system_prompt: Option<&'a str>,
) -> Option<&'a str> {
    if provisional_session && target_session_id.is_none() {
        system_prompt
    } else {
        None
    }
}

/// The terminal environment carried by the initial `Subscribe` request.
pub(super) fn initial_subscribe_terminal_env(request: &Request) -> Vec<(String, String)> {
    match request {
        Request::Subscribe { terminal_env, .. } => terminal_env.clone(),
        _ => Vec::new(),
    }
}
