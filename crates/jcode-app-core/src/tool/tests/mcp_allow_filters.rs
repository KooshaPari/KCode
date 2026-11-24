//! The session allow/deny filters as they apply to deferred MCP surfaces.
//!
//! Split out of `tool/tests.rs` because that file is already over the
//! oversized-test-file budget, and this is the one test in it that is about
//! the MCP allow/deny filters rather than about the registry.

use super::*;

#[test]
fn deferred_mcp_surfaces_follow_umbrella_and_per_tool_filters() {
    use std::collections::HashSet;

    let umbrella = HashSet::from(["mcp".to_string()]);
    assert!(tool_name_is_allowed(&umbrella, "mcp_search", &[]));
    assert!(tool_name_is_allowed(&umbrella, "mcp_call", &[]));
    assert!(tool_name_is_allowed(&umbrella, "mcp__server__tool", &[]));

    let one_tool = HashSet::from(["mcp__server__allowed".to_string()]);
    assert!(tool_name_is_allowed(&one_tool, "mcp_search", &[]));
    assert!(tool_name_is_allowed(&one_tool, "mcp_call", &[]));

    let disabled = HashSet::from(["mcp".to_string()]);
    assert!(tool_name_is_disabled(&disabled, "mcp_search", &[]));
    assert!(tool_name_is_disabled(&disabled, "mcp_call", &[]));
    assert!(tool_name_is_disabled(&disabled, "mcp__server__tool", &[]));

    set_session_tool_policy(
        "deferred-filter-test",
        Some(one_tool),
        HashSet::from(["mcp__server__blocked".to_string()]),
    );
    assert!(session_mcp_dispatch_is_allowed(
        "deferred-filter-test",
        "mcp__server__allowed",
        "mcp_call"
    ));
    assert!(!session_mcp_dispatch_is_allowed(
        "deferred-filter-test",
        "mcp__server__blocked",
        "mcp_call"
    ));
    assert!(!session_mcp_dispatch_is_allowed(
        "deferred-filter-test",
        "mcp__server__other",
        "mcp_call"
    ));
    clear_session_tool_policy("deferred-filter-test");
}
