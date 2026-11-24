//! The session's tool policy: what a session may call, and what it may not.
//!
//! A policy merges the agent's own allow and deny lists with the SDK overlay
//! the client installed. It also carries the overlay's MCP grants - the
//! `(server, tool)` pairs that survive a blanket `mcp` deny - because a grant
//! is a property of the session's configuration, not of a single call site.

use std::collections::HashSet;

use crate::protocol::{McpToolGrant, SessionToolConfig};

#[derive(Clone, Debug, Default)]
pub(crate) struct SessionToolPolicy {
    pub(crate) allowed_tools: Option<HashSet<String>>,
    pub(crate) disabled_tools: HashSet<String>,
    /// The session's MCP grants: the `(server, tool)` pairs that survive the
    /// blanket `mcp` deny.
    pub(crate) allowed_mcp_tools: Vec<McpToolGrant>,
    pub(crate) owner: Option<u64>,
}

impl SessionToolPolicy {
    /// The deny view of one SDK overlay: its disabled list and its grants.
    ///
    /// The overlay's own view, not the merged session policy: a custom SDK
    /// tool is judged against the overlay's deny list alone.
    pub(crate) fn from_sdk_config(config: &SessionToolConfig) -> Self {
        Self {
            disabled_tools: config.disabled.iter().cloned().collect(),
            allowed_mcp_tools: config.allowed_mcp_tools.clone(),
            ..Default::default()
        }
    }
}
