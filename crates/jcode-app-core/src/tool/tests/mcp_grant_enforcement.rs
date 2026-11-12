//! The MCP grant list reaching the enforcement points that apply it.
//!
//! A grant only does something if the session's real grant list reaches the
//! checks that apply it. These two tests pin that down for the two paths the
//! feature exists for: an aliased MCP tool, and SDK tool execution.
//!
//! Each test states the negative case first: the same deny, the same tool,
//! without a grant. A grant that never arrives would leave the positive case
//! failing for the same reason the negative case fails.

use super::*;
use crate::protocol::{McpToolGrant, SessionToolConfig, SessionToolDefinition};
use serde_json::json;
use tokio::sync::mpsc;

/// The MCP tool this file grants: a server and a tool whose raw name is not
/// a legal provider name, so the grant has to resolve through the dispatch
/// name to match.
const GRANTED_SERVER: &str = "yc";
const GRANTED_TOOL: &str = "hiring.create_job";

fn granted(server: &str, tool: &str) -> McpToolGrant {
    McpToolGrant {
        server: server.into(),
        tool: tool.into(),
    }
}

fn custom(name: &str) -> SessionToolDefinition {
    SessionToolDefinition {
        name: name.into(),
        description: "Granted callback".into(),
        parameters: json!({"type":"object"}),
    }
}

fn ctx(session: &str) -> ToolContext {
    ToolContext {
        session_id: session.into(),
        message_id: "grant-test".into(),
        tool_call_id: "grant-parent".into(),
        working_dir: None,
        stdin_request_tx: None,
        graceful_shutdown_signal: None,
        execution_mode: ToolExecutionMode::Direct,
    }
}

struct Cleanup(String);
impl Drop for Cleanup {
    fn drop(&mut self) {
        sdk::remove_session(&self.0);
        clear_session_tool_policy(&self.0);
    }
}

/// A grant lifts the blanket `mcp` deny for an aliased MCP tool.
///
/// The alias path is the one that names a tool by its dispatch identity, so it
/// is the path where a grant has to resolve a `(server, tool)` pair into a
/// name before it can lift anything.
#[test]
fn a_grant_lifts_the_blanket_mcp_deny_for_an_aliased_mcp_tool() {
    let dispatch = crate::mcp::dispatch_name(GRANTED_SERVER, GRANTED_TOOL);
    assert_eq!(dispatch, "mcp__yc__hiring_create_job");

    // Without a grant, the blanket `mcp` deny covers the aliased tool.
    let denied = "grant-alias-denied";
    let _denied_cleanup = Cleanup(denied.into());
    let (denied_tx, _denied_rx) = mpsc::unbounded_channel();
    sdk::configure(
        denied,
        "grant-owner",
        SessionToolConfig {
            disabled: vec!["mcp".into()],
            ..Default::default()
        },
        denied_tx,
    )
    .unwrap();
    assert!(!session_mcp_alias_is_allowed(
        denied, &dispatch, &dispatch, "mcp_call"
    ));

    // With the grant, the very same deny no longer covers it.
    let session = "grant-alias";
    let _cleanup = Cleanup(session.into());
    let (tx, _rx) = mpsc::unbounded_channel();
    sdk::configure(
        session,
        "grant-owner",
        SessionToolConfig {
            disabled: vec!["mcp".into()],
            allowed_mcp_tools: vec![granted(GRANTED_SERVER, GRANTED_TOOL)],
            ..Default::default()
        },
        tx,
    )
    .unwrap();
    assert!(session_mcp_alias_is_allowed(
        session, &dispatch, &dispatch, "mcp_call"
    ));
}

/// A grant lifts the blanket `mcp` deny for SDK tool execution.
///
/// The SDK overlay judges its own tools in two places: the definition filter
/// that decides what the model is offered, and the authorization that decides
/// whether a call may run. Both read the overlay's deny list, so both have to
/// read the overlay's grants.
#[tokio::test]
async fn a_grant_lifts_the_blanket_mcp_deny_for_sdk_tool_execution() {
    let _lock = crate::storage::lock_test_env();
    let home = tempfile::tempdir().unwrap();
    let _home = TestHomeGuard::new(home.path());
    let name = crate::mcp::dispatch_name(GRANTED_SERVER, GRANTED_TOOL);
    let registry = Registry::new(Arc::new(MockProvider)).await;

    // Without a grant, the SDK tool is denied.
    let denied = "grant-sdk-denied";
    let _denied_cleanup = Cleanup(denied.into());
    let (denied_tx, _denied_rx) = mpsc::unbounded_channel();
    sdk::configure(
        denied,
        "grant-owner",
        SessionToolConfig {
            disabled: vec!["mcp".into()],
            custom: vec![custom(&name)],
            ..Default::default()
        },
        denied_tx,
    )
    .unwrap();
    let error = registry
        .execute(&name, json!({}), ctx(denied))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("is disabled"), "{error}");
    let denied_definitions = sdk::apply_definitions(
        denied,
        vec![ToolDefinition {
            name: name.clone(),
            description: "Granted callback".into(),
            input_schema: json!({}),
            defer_loading: false,
        }],
    );
    assert!(
        denied_definitions.iter().all(|d| d.name != name),
        "an ungranted tool must not survive the overlay's deny filter"
    );

    // With the grant, the same tool survives the filter and is authorized:
    // the call reaches the owner rendezvous instead of being rejected.
    let session = "grant-sdk";
    let _cleanup = Cleanup(session.into());
    let (tx, _rx) = mpsc::unbounded_channel();
    sdk::configure(
        session,
        "grant-owner",
        SessionToolConfig {
            disabled: vec!["mcp".into()],
            allowed_mcp_tools: vec![granted(GRANTED_SERVER, GRANTED_TOOL)],
            custom: vec![custom(&name)],
            ..Default::default()
        },
        tx,
    )
    .unwrap();
    let definitions = sdk::apply_definitions(
        session,
        vec![ToolDefinition {
            name: name.clone(),
            description: "Granted callback".into(),
            input_schema: json!({}),
            defer_loading: false,
        }],
    );
    assert!(
        definitions.iter().any(|d| d.name == name),
        "a granted tool must survive the overlay's deny filter"
    );

    // Dropping the owner leaves the overlay in place with its grants, so the
    // only thing that can reject this call is the deny.
    drop(sdk::ConnectionGuard("grant-owner".into()));
    let error = registry
        .execute(&name, json!({}), ctx(session))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("disconnected"),
        "a granted SDK tool must be authorized: {error}"
    );
}
