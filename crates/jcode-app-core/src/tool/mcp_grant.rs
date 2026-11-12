//! Session-scoped MCP tool grants.
//!
//! A grant names ONE `(server, tool)` pair and lifts the blanket `mcp` deny
//! for exactly that pair. It is a deny-side concept: it never widens the
//! allow-list, and it can never exempt the generic `mcp`, `mcp_call`, or
//! `mcp_search` surfaces, which reach every server at once.
//!
//! The allow/deny predicates live here rather than in the much larger `tool`
//! module because the grant rules and the predicates that apply them are one
//! concern: a reader who has to reason about a grant needs both at once.

use std::collections::HashSet;

use super::{is_fixed_mcp_tool, is_mcp_tool_name};

/// The generic MCP surfaces. They reach every server at once, so no grant can
/// ever exempt them.
const GENERIC_MCP_SURFACES: [&str; 3] = ["mcp", "mcp_call", "mcp_search"];

/// Whether `name` survives the session allow-list.
///
/// `grants` is accepted for symmetry with [`tool_name_is_disabled`] so one
/// grant list can be threaded through both checks. A grant is a deny-side
/// concept: it lifts the blanket `mcp` deny for one `mcp__<server>__<tool>`
/// identity and never widens the allow-list.
pub(crate) fn tool_name_is_allowed(
    allowed: &HashSet<String>,
    name: &str,
    _grants: &[(String, String)],
) -> bool {
    allowed.contains(name)
        || (allowed.contains("mcp") && is_mcp_tool_name(name))
        || (is_fixed_mcp_tool(name) && allowed.iter().any(|tool| tool.starts_with("mcp__")))
}

/// Whether `name` is denied by the session disable-list.
///
/// A blanket `mcp` deny covers every MCP surface at once. A grant lifts that
/// blanket for exactly one `mcp__<server>__<tool>` identity and for nothing
/// else. An explicit per-tool deny still wins over a grant.
pub(crate) fn tool_name_is_disabled(
    disabled: &HashSet<String>,
    name: &str,
    grants: &[(String, String)],
) -> bool {
    disabled.contains(name)
        || (disabled.contains("mcp") && is_mcp_tool_name(name) && !mcp_grant_covers(name, grants))
}

/// Whether a grant covers the `mcp__<server>__<tool>` identity `name`.
///
/// Grants are `(server, tool)` pairs. A pair is matched against the
/// model-facing dispatch name, so a raw MCP tool name is normalized exactly
/// the way the registry normalizes it. The generic surfaces are rejected
/// first: they are not `mcp__<server>__<tool>` identities at all, so no pair
/// can address them.
fn mcp_grant_covers(name: &str, grants: &[(String, String)]) -> bool {
    if GENERIC_MCP_SURFACES.contains(&name) {
        return false;
    }
    grants
        .iter()
        .any(|(server, tool)| crate::mcp::dispatch_name(server, tool) == name)
}
