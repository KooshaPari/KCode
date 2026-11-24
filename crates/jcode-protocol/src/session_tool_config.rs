//! The session tool-selection contract.
//!
//! `SessionToolConfig` is the overlay a client sends to replace the session's
//! normal tool selection, `SessionToolDefinition` is one SDK-owned tool
//! declaration inside that overlay, and `McpToolGrant` is the narrow exception
//! that lets a single MCP tool survive a blanket MCP deny.
//!
//! These three types travel together on the wire, so they live together here
//! rather than inside the much larger `wire` module.

use serde::{Deserialize, Serialize};

/// SDK-owned session tool declaration. Parameters is a JSON schema object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// An MCP tool the operator explicitly allowed in a restricted session.
///
/// A grant names ONE server and ONE tool. It is deliberately not a prefix and
/// not a surface: the pair resolves to a single `mcp__<server>__<tool>`
/// identity, so a grant can never widen into the generic `mcp_call`,
/// `mcp_search`, or `mcp` surfaces, which reach every server at once.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpToolGrant {
    pub server: String,
    pub tool: String,
}

/// Replaces the session SDK overlay. None inherits normal selection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionToolConfig {
    #[serde(default)]
    pub enabled: Option<Vec<String>>,
    #[serde(default)]
    pub disabled: Vec<String>,
    #[serde(default)]
    pub custom: Vec<SessionToolDefinition>,
    /// MCP tools that survive a blanket MCP deny.
    ///
    /// A grant lifts the blanket `mcp` deny for exactly the named
    /// `(server, tool)` pair and for nothing else. An explicit per-tool deny
    /// still wins over a grant, and the generic surfaces are never grantable.
    /// An empty list means no exception at all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_mcp_tools: Vec<McpToolGrant>,
}
