//! Permission-bubble integration for subagent session forking.
//!
//! When creating child/coordinator sessions, applies `jcode-permission-bubble`
//! constraints: byte-identical prefix construction, fork depth guards, and
//! the 10-rule child message template.
//!
//! Fork depth state is persisted directly on the `Session` struct (fields:
//! `fork_depth`, `fork_max_depth`, `fork_chain`) so that depth tracking
//! survives session saves/loads and is available across forking boundaries.

use jcode_permission_bubble::{
    build_child_message, ForkDepthGuard, ForkDepthResult,
};
use jcode_session_types::StoredMessage;

use crate::logging;

/// Result of applying permission-bubble constraints to a new child session.
pub(crate) struct PermissionBubbleResult {
    /// Messages to use in the child session (forked with prefix integrity).
    pub messages: Vec<StoredMessage>,
    /// Child's initial system prompt (with the 10-rule boilerplate).
    pub system_prompt: String,
    /// Whether the fork was denied due to depth limits.
    pub denied: bool,
}

/// Reconstruct a `ForkDepthGuard` from persisted session fields.
fn guard_from_session(session: &crate::session::Session) -> ForkDepthGuard {
    let mut guard = ForkDepthGuard::with_max_depth(session.fork_max_depth as usize);
    // Replay the fork chain to bring the guard up to the session's current depth.
    for fork_id in &session.fork_chain {
        let _ = guard.advance(fork_id);
    }
    guard
}

/// Create permission-bubble-constrained messages for a child session.
///
/// Takes the parent's message history and constructs a fork payload with
/// byte-identical prefix sharing for cache efficiency. The parent session's
/// fork depth state is read and updated in place.
pub(crate) fn create_forked_child_messages(
    parent_messages: &[StoredMessage],
    directive: &str,
    fork_id: &str,
    parent_session: &mut crate::session::Session,
) -> PermissionBubbleResult {
    let mut guard = guard_from_session(parent_session);

    match guard.can_fork() {
        ForkDepthResult::Denied {
            current_depth,
            max_depth,
        } => {
            logging::warn(&format!(
                "FORK_DENIED: depth {} would exceed max {} (chain: {:?})",
                current_depth,
                max_depth,
                guard.fork_chain(),
            ));
            return PermissionBubbleResult {
                messages: parent_messages.to_vec(),
                system_prompt: build_child_message(directive),
                denied: true,
            };
        }
        ForkDepthResult::Allowed { .. } => {}
    }

    // Advance the guard to record this fork in the chain.
    let _result = guard.advance(fork_id);

    // Persist the updated guard state back to the session.
    parent_session.fork_depth = guard.depth() as u8;
    parent_session.fork_chain = guard.fork_chain().to_vec();

    let messages = parent_messages.to_vec();
    logging::info(&format!(
        "FORK_CREATED: fork_id={} depth={}",
        fork_id,
        guard.depth(),
    ));

    PermissionBubbleResult {
        messages,
        system_prompt: build_child_message(directive),
        denied: false,
    }
}
