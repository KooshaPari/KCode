//! MCP registration policy for a target-aware client subscribe.
//!
//! A subscribe that resumes an existing session has to decide whether the MCP
//! servers for that session still need registering. That is not the same
//! question as "does the session exist": a session that is live in memory has
//! an owner that already registered its servers, while a session that exists
//! only on disk has none at all.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use crate::agent::Agent;

type SessionAgents = Arc<RwLock<HashMap<String, Arc<Mutex<Agent>>>>>;

/// What a target-aware subscribe must do about the target session's MCP servers.
pub(super) struct TargetSessionMcp {
    /// The target already exists, on disk or in memory, so the subscribe
    /// resumes it instead of creating a different session.
    pub(super) is_existing_session: bool,
    /// The subscribe must register the MCP servers for the resumed session.
    pub(super) register_mcp_tools: bool,
}

/// Resolve the MCP registration policy for a target-aware subscribe.
///
/// Whether the target is LIVE IN MEMORY decides two things: it admits the
/// target as an existing session, and it decides whether MCP servers get
/// registered for this subscribe.
///
/// Register MCP unless this is a takeover of a session that is ALREADY live:
/// that owner registered its servers, and a second registration would double
/// every tool and open a second connection to every MCP server.
///
/// A target that exists only on disk is the case this used to get wrong.
/// `session/load` - what the ACP adoption shim and a Kiro Crew turn both
/// issue - resumed a persisted session, took this branch with a hardcoded
/// `false`, and registered nothing: the session ran with no `mcp__*` tools,
/// silently, while a freshly created session had them.
pub(super) async fn resolve_target_session_mcp(
    sessions: &SessionAgents,
    target_session_id: &str,
) -> TargetSessionMcp {
    let exists_on_disk = crate::session::session_exists(target_session_id);
    let is_live = sessions.read().await.contains_key(target_session_id);
    TargetSessionMcp {
        is_existing_session: exists_on_disk || is_live,
        register_mcp_tools: !is_live,
    }
}
