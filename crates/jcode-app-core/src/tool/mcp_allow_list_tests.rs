//! Tests for the session allow/deny predicates and the MCP grant rules.
//!
//! These live in their own file so the production `tool` module carries the
//! predicates and not their assertions. Every case here is behaviour that the
//! MCP grant work had to pin down, so all of them move as-is.

use super::{tool_name_is_allowed, tool_name_is_disabled};
use crate::protocol::McpToolGrant;
use std::collections::HashSet;

#[test]
fn allowing_mcp_also_allows_dynamic_server_tools() {
    let allowed = HashSet::from(["mcp".to_string()]);

    assert!(tool_name_is_allowed(&allowed, "mcp", &[]));
    assert!(tool_name_is_allowed(
        &allowed,
        "mcp__filesystem__read_file",
        &[]
    ));
    assert!(!tool_name_is_allowed(&allowed, "mcpish", &[]));
    assert!(!tool_name_is_allowed(&allowed, "bash", &[]));
}

#[test]
fn disabling_mcp_also_disables_dynamic_server_tools() {
    let disabled = HashSet::from(["mcp".to_string()]);

    assert!(tool_name_is_disabled(&disabled, "mcp", &[]));
    assert!(tool_name_is_disabled(
        &disabled,
        "mcp__filesystem__read_file",
        &[]
    ));
    assert!(!tool_name_is_disabled(&disabled, "mcpish", &[]));
    assert!(!tool_name_is_disabled(&disabled, "bash", &[]));
}

#[test]
fn an_empty_grant_list_keeps_the_default_mcp_deny() {
    let disabled = HashSet::from(["mcp".to_string()]);

    assert!(tool_name_is_disabled(&disabled, "mcp", &[]));
    assert!(tool_name_is_disabled(&disabled, "mcp_call", &[]));
    assert!(tool_name_is_disabled(&disabled, "mcp_search", &[]));
    assert!(tool_name_is_disabled(
        &disabled,
        "mcp__filesystem__read_file",
        &[]
    ));
    assert!(!tool_name_is_disabled(&disabled, "bash", &[]));
}

#[test]
fn a_grant_lifts_the_blanket_mcp_deny_for_its_pair_only() {
    let disabled = HashSet::from(["mcp".to_string()]);
    let grants = vec![McpToolGrant {
        server: "filesystem".into(),
        tool: "read_file".into(),
    }];

    assert!(!tool_name_is_disabled(
        &disabled,
        "mcp__filesystem__read_file",
        &grants
    ));
    // The grant names one pair, not the server and not the surface.
    assert!(tool_name_is_disabled(
        &disabled,
        "mcp__filesystem__write_file",
        &grants
    ));
    assert!(tool_name_is_disabled(
        &disabled,
        "mcp__other__read_file",
        &grants
    ));
    assert!(tool_name_is_disabled(&disabled, "mcp", &grants));
}

#[test]
fn a_grant_matches_the_model_facing_dispatch_name() {
    let disabled = HashSet::from(["mcp".to_string()]);
    let grants = vec![McpToolGrant {
        server: "yc".into(),
        tool: "hiring.create_job".into(),
    }];

    assert!(!tool_name_is_disabled(
        &disabled,
        "mcp__yc__hiring_create_job",
        &grants
    ));
}

#[test]
fn a_grant_never_exempts_the_generic_mcp_surfaces() {
    let disabled = HashSet::from(["mcp".to_string()]);
    let grants = vec![
        McpToolGrant {
            server: "mcp".into(),
            tool: "call".into(),
        },
        McpToolGrant {
            server: "mcp_call".into(),
            tool: "server".into(),
        },
        McpToolGrant {
            server: "mcp_search".into(),
            tool: "server".into(),
        },
    ];

    assert!(tool_name_is_disabled(&disabled, "mcp", &grants));
    assert!(tool_name_is_disabled(&disabled, "mcp_call", &grants));
    assert!(tool_name_is_disabled(&disabled, "mcp_search", &grants));
}

#[test]
fn an_explicit_per_tool_deny_still_wins_over_a_grant() {
    let disabled = HashSet::from(["mcp".to_string(), "mcp__filesystem__read_file".to_string()]);
    let grants = vec![McpToolGrant {
        server: "filesystem".into(),
        tool: "read_file".into(),
    }];

    assert!(tool_name_is_disabled(
        &disabled,
        "mcp__filesystem__read_file",
        &grants
    ));
}

/// The allow side is asymmetric on purpose, and that asymmetry is a trap.
///
/// `tool_name_is_allowed` pulls the generic surfaces in as soon as *any*
/// `mcp__*` name is allowed, so an allow-list that mentions a single server
/// tool silently re-admits `mcp_call` and `mcp_search`. This test records
/// the behaviour rather than blessing it: a grant must never be expressed by
/// widening `allowed`, because that would re-enable the escape hatches the
/// deny-side grant exists to keep shut.
#[test]
fn allowing_any_server_tool_also_readmits_the_generic_surfaces() {
    let allowed = HashSet::from(["mcp__filesystem__read_file".to_string()]);

    assert!(tool_name_is_allowed(
        &allowed,
        "mcp__filesystem__read_file",
        &[]
    ));
    assert!(
        tool_name_is_allowed(&allowed, "mcp_call", &[]),
        "known asymmetry: allowing one server tool re-admits mcp_call"
    );
    assert!(
        tool_name_is_allowed(&allowed, "mcp_search", &[]),
        "known asymmetry: allowing one server tool re-admits mcp_search"
    );
}

/// A grant is deny-side only. It must not change the allow verdict at all.
#[test]
fn a_grant_never_widens_the_allow_list() {
    let allowed = HashSet::from(["bash".to_string()]);
    let grants = vec![McpToolGrant {
        server: "filesystem".into(),
        tool: "read_file".into(),
    }];

    assert!(!tool_name_is_allowed(
        &allowed,
        "mcp__filesystem__read_file",
        &grants
    ));
    assert!(!tool_name_is_allowed(&allowed, "mcp_call", &grants));
}
