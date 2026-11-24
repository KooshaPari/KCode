//! The session tool surface of the SDK client.
//!
//! Split out of `client_behavior.rs` because that file is already over the
//! oversized-test-file budget. The fake-harness helpers stay in the parent
//! module and are reached through `super`.

use super::*;

fn custom_tool() -> jcode_sdk::SessionToolDefinition {
    jcode_sdk::SessionToolDefinition {
        name: "greet".into(),
        description: "Greet a person".into(),
        parameters: serde_json::from_value(serde_json::json!({
            "type": "object", "properties": {"name": {"type": "string"}},
            "required": ["name"]
        }))
        .unwrap(),
    }
}

#[test]
fn session_tool_configuration_and_listing_preserve_typed_payloads() {
    let client = fake_harness(|frame, writer| match &frame.request {
        ApiRequest::ConfigureTools { session_id, tools } => {
            assert_eq!(session_id, "s1");
            assert_eq!(tools.enabled, Some(vec!["read".into(), "greet".into()]));
            assert_eq!(tools.disabled, vec!["bash"]);
            assert_eq!(
                serde_json::to_value(&tools.custom).unwrap(),
                serde_json::to_value(vec![custom_tool()]).unwrap()
            );
            reply(frame, ApiEvent::Ok, writer);
        }
        ApiRequest::ListTools { session_id } => {
            assert_eq!(session_id, "s1");
            reply(
                frame,
                ApiEvent::Tools {
                    session_id: session_id.clone(),
                    tools: vec![custom_tool()],
                },
                writer,
            );
        }
        other => panic!("unexpected request: {other:?}"),
    });
    client
        .configure_tools(
            "s1",
            jcode_sdk::ToolConfiguration {
                enabled: Some(vec!["read".into(), "greet".into()]),
                disabled: vec!["bash".into()],
                custom: vec![custom_tool()],
                // `..Default::default()` rather than naming every field: these
                // tests are about one aspect of the configuration, not about its
                // full shape, and a new optional field should not fail the SDK
                // test build the way `allowed_mcp_tools` just did.
                ..Default::default()
            },
        )
        .unwrap();
    let tools = client.list_tools("s1").unwrap();
    assert_eq!(
        serde_json::to_value(tools).unwrap(),
        serde_json::to_value(vec![custom_tool()]).unwrap()
    );
}

#[test]
fn custom_tool_events_are_session_filtered_and_results_preserve_success_and_failure() {
    let client = fake_harness(|frame, writer| match &frame.request {
        ApiRequest::SendMessage { session_id, .. } => {
            for (id, call_id) in [
                ("other", "ignore"),
                (session_id.as_str(), "success"),
                (session_id.as_str(), "failure"),
            ] {
                push(
                    ApiEvent::ToolCall {
                        session_id: id.into(),
                        call_id: call_id.into(),
                        name: "greet".into(),
                        input: serde_json::json!({"name": "Ada"}),
                    },
                    writer,
                );
            }
        }
        ApiRequest::ToolResult {
            session_id,
            call_id,
            output,
            error,
        } => {
            assert_eq!(session_id, "s1");
            match call_id.as_str() {
                "success" => {
                    assert_eq!(output, "Hello, Ada!");
                    assert_eq!(error, &None);
                }
                "failure" => {
                    assert_eq!(output, "");
                    assert_eq!(error.as_deref(), Some("unavailable"));
                }
                other => panic!("unexpected call id: {other}"),
            }
            reply(frame, ApiEvent::Ok, writer);
        }
        other => panic!("unexpected request: {other:?}"),
    });
    let events = client.events(Some("s1"));
    client
        .send_message("s1", "greet Ada", vec![], None)
        .unwrap();
    for expected in ["success", "failure"] {
        let event = events
            .next_timeout(Duration::from_secs(2))
            .expect("custom tool event");
        let ApiEvent::ToolCall {
            session_id,
            call_id,
            name,
            input,
        } = event
        else {
            panic!("unexpected event: {event:?}");
        };
        assert_eq!(call_id, expected);
        assert_eq!(name, "greet");
        assert_eq!(input, serde_json::json!({"name": "Ada"}));
        let (output, error) = if expected == "success" {
            ("Hello, Ada!", None)
        } else {
            ("", Some("unavailable".into()))
        };
        client
            .submit_tool_result(&session_id, &call_id, output, error)
            .unwrap();
    }
}

#[test]
fn session_tool_methods_propagate_harness_errors() {
    let client = fake_harness(|frame, writer| {
        reply(
            frame,
            ApiEvent::Error {
                code: jcode_sdk::api::ErrorCode::UnknownRequest,
                message: "session tools unavailable".into(),
            },
            writer,
        );
    });
    let errors = [
        client
            .configure_tools(
                "s1",
                jcode_sdk::ToolConfiguration {
                    enabled: None,
                    disabled: vec![],
                    custom: vec![],
                    ..Default::default()
                },
            )
            .unwrap_err(),
        client.list_tools("s1").unwrap_err(),
        client
            .submit_tool_result("s1", "call", "ok", None)
            .unwrap_err(),
    ];
    for error in errors {
        assert_eq!(
            error.kind,
            jcode_sdk::ErrorKind::Harness(jcode_sdk::api::ErrorCode::UnknownRequest)
        );
        assert!(error.to_string().contains("session tools unavailable"));
    }
}

#[test]
fn session_tool_methods_reject_unexpected_replies() {
    let client = fake_harness(|frame, writer| reply(frame, ApiEvent::Pong, writer));
    assert!(
        client
            .configure_tools(
                "s1",
                jcode_sdk::ToolConfiguration {
                    enabled: Some(vec![]),
                    disabled: vec![],
                    custom: vec![],
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert!(client.list_tools("s1").is_err());
    assert!(client.submit_tool_result("s1", "call", "", None).is_err());
}
