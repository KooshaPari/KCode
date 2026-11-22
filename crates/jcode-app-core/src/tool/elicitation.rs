//! Native elicitation tool — replaces the external phinbox MCP dependency.
//!
//! When the agent needs structured user input (multi-choice, text, boolean),
//! it calls this tool which sends a request through the global elicitation
//! channel. The TUI renders a modal overlay, collects the user's response,
//! and sends it back through the oneshot channel.

use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use jcode_tool_core::{ElicitMessage, ElicitRequest, elicit_channel};
use serde_json::{Value, json};

#[cfg(test)]
mod tests {
    use super::*;

    /// Reject the constructs OpenAI's function-schema validator refuses:
    /// `const`, union `type` arrays, root-level `propertyNames`, and
    /// `oneOf`/`anyOf`/`allOf`/`$ref` siblings.
    fn assert_openai_compatible(v: &Value, path: &str) {
        match v {
            Value::Object(m) => {
                for banned in ["const", "propertyNames", "oneOf", "anyOf", "allOf", "$ref"] {
                    assert!(
                        !m.contains_key(banned),
                        "`{banned}` is not accepted by OpenAI at {path}"
                    );
                }
                if let Some(ty) = m.get("type") {
                    assert!(
                        ty.is_string(),
                        "union type array is not accepted by OpenAI at {path}"
                    );
                }
                for (k, c) in m {
                    assert_openai_compatible(c, &format!("{path}/{k}"));
                }
            }
            Value::Array(a) => {
                for (i, c) in a.iter().enumerate() {
                    assert_openai_compatible(c, &format!("{path}/{i}"));
                }
            }
            _ => {}
        }
    }

    #[test]
    fn parameters_schema_is_openai_compatible() {
        assert_openai_compatible(&ElicitateMcpTool::new().parameters_schema(), "");
    }

    #[test]
    fn parameters_schema_field_is_flattened_object() {
        let schema = ElicitateMcpTool::new().parameters_schema();
        assert_eq!(schema["type"], json!("object"));
        assert!(schema.get("propertyNames").is_none());

        let field = &schema["properties"]["field"];
        assert_eq!(field["type"], json!("object"));
        assert_eq!(field["required"], json!(["kind", "label"]));
        assert!(field.get("oneOf").is_none());
        assert!(field.get("const").is_none());

        let props = field["properties"]
            .as_object()
            .expect("field.properties is an object");
        for name in [
            "kind",
            "label",
            "default",
            "placeholder",
            "max_length",
            "secret",
            "min",
            "max",
            "options",
            "default_index",
            "picker_kind",
        ] {
            assert!(props.contains_key(name), "field.{name} is missing");
        }
    }

    #[test]
    fn parameters_schema_field_kind_lists_all_variants() {
        let schema = ElicitateMcpTool::new().parameters_schema();
        assert_eq!(
            schema["properties"]["field"]["properties"]["kind"]["enum"],
            json!(["text", "long_text", "integer", "choice", "boolean", "date_time"])
        );
    }
}

pub struct ElicitateMcpTool;

impl ElicitateMcpTool {
    pub fn new() -> Self {
        Self
    }
}

#[derive(serde::Deserialize)]
struct ElicitInput {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    field: Option<Value>,
    #[serde(default)]
    intent: Option<String>,
    #[serde(default)]
    question: Option<String>,
    #[serde(default)]
    notes: Option<Value>,
    #[serde(default)]
    buttons: Option<Value>,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    timeout_secs: Option<u32>,
    #[serde(default)]
    urgency: Option<String>,
}

#[async_trait]
impl Tool for ElicitateMcpTool {
    fn name(&self) -> &str {
        "elicitate_mcp"
    }

    fn description(&self) -> &str {
        "Render a native OS popup and block until the human operator responds \
         (or the prompt times out). Use this whenever an autonomous agent needs \
         a single, structured decision from a human: a confirmation, a \
         multi-choice selection, a secret, a disambiguation. Returns a typed \
         JSON ElicitResponse."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["field", "title"],
            "properties": {
                "intent": super::intent_schema_property(),
                "title": {
                    "type": "string",
                    "description": "One-line title."
                },
                "question": {
                    "type": "string",
                    "description": "Multi-line body explaining context.",
                    "default": ""
                },
                "field": {
                    "type": "object",
                    "description": "The input field configuration. `kind` selects the variant, and only the properties listed for that variant apply. \
                        text: label, default (string), placeholder, max_length, secret. \
                        long_text: label, default (string), max_length. \
                        integer: label, default (integer), min, max. \
                        choice: label, options (required array of {label, value, description?}), default_index. \
                        boolean: label, default (boolean). \
                        date_time: label, default (string), picker_kind (date | time | datetime). \
                        `label` is required for every variant and `options` is required for `choice`.",
                    "required": ["kind", "label"],
                    "properties": {
                        "kind": {
                            "type": "string",
                            "enum": ["text", "long_text", "integer", "choice", "boolean", "date_time"],
                            "description": "Which input field variant to render."
                        },
                        "label": {
                            "type": "string",
                            "description": "Human-readable label for the input."
                        },
                        "default": {
                            "description": "Per-variant default value: string for text, long_text and date_time; integer for integer; boolean for boolean. Unused by choice."
                        },
                        "placeholder": {
                            "type": "string",
                            "description": "text only: placeholder shown when empty."
                        },
                        "max_length": {
                            "type": "integer",
                            "minimum": 0,
                            "description": "text and long_text only: maximum input length."
                        },
                        "secret": {
                            "type": "boolean",
                            "description": "text only: mask the typed value."
                        },
                        "min": {
                            "type": "integer",
                            "description": "integer only: minimum allowed value."
                        },
                        "max": {
                            "type": "integer",
                            "description": "integer only: maximum allowed value."
                        },
                        "options": {
                            "type": "array",
                            "description": "choice only (required): selectable options.",
                            "items": {
                                "type": "object",
                                "required": ["label", "value"],
                                "properties": {
                                    "label": {"type": "string"},
                                    "value": {"type": "string"},
                                    "description": {"type": "string"}
                                }
                            }
                        },
                        "default_index": {
                            "type": "integer",
                            "minimum": 0,
                            "description": "choice only: index of the option selected by default."
                        },
                        "picker_kind": {
                            "type": "string",
                            "enum": ["date", "time", "datetime"],
                            "description": "date_time only: which picker to show."
                        }
                    }
                },
                "notes": {
                    "description": "The optional notes / free-text box.",
                    "type": "object",
                    "properties": {
                        "label": {"type": "string"},
                        "default": {"type": "string"},
                        "max_length": {"type": "integer"},
                        "required": {"type": "boolean"}
                    }
                },
                "buttons": {
                    "description": "Custom button labels.",
                    "type": "object",
                    "properties": {
                        "confirm": {"type": "string", "description": "Confirm button label. Default: \"OK\"."},
                        "cancel": {"type": "string", "description": "Cancel button label. Default: \"Cancel\"."},
                        "default_is_cancel": {"type": "boolean", "description": "If true, swap which button is the default."}
                    }
                },
                "request_id": {
                    "type": "string",
                    "description": "Optional request ID for correlation."
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds.",
                    "default": 600,
                    "minimum": 0
                },
                "urgency": {
                    "type": "string",
                    "description": "Urgency hint — affects icon and sound on GUI popups.",
                    "enum": ["info", "warning", "error", "secret"],
                    "default": "info"
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: ElicitInput = serde_json::from_value(input)?;

        let field = params
            .field
            .ok_or_else(|| anyhow::anyhow!("field parameter is required"))?;

        validate_field(&field)?;

        let request = ElicitRequest {
            title: params.title.unwrap_or_default(),
            intent: params.intent.unwrap_or_default(),
            field,
            question: params.question,
            notes: params.notes,
            buttons: params.buttons,
            request_id: params.request_id,
            timeout_secs: params.timeout_secs.unwrap_or(600),
            urgency: params.urgency.unwrap_or_else(|| "info".to_string()),
        };

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        let msg = ElicitMessage {
            request,
            response_tx,
        };

        // Send request through global channel to TUI
        elicit_channel::try_send(msg)
            .map_err(|e| anyhow::anyhow!("failed to send elicitation request: {e}"))?;

        // Wait for user response (with timeout)
        let timeout = params.timeout_secs.unwrap_or(600);
        let response =
            tokio::time::timeout(std::time::Duration::from_secs(timeout as u64), response_rx)
                .await
                .map_err(|_| anyhow::anyhow!("elicitation timed out after {timeout}s"))?
                .map_err(|_| anyhow::anyhow!("elicitation response channel closed"))?;

        let output = json!({
            "action": response.action,
            "value": response.value,
            "notes": response.notes,
        });

        Ok(ToolOutput {
            output: output.to_string(),
            title: None,
            metadata: None,
            images: vec![],
        })
    }
}

/// Enforce the per-variant requirements that used to live in the schema's
/// `oneOf` branches, now that `field` is advertised as a single object schema.
fn validate_field(field: &Value) -> Result<()> {
    const KINDS: [&str; 6] = [
        "text",
        "long_text",
        "integer",
        "choice",
        "boolean",
        "date_time",
    ];

    let obj = field
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("field must be an object"))?;

    let kind = obj
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("field.kind is required and must be a string"))?;
    if !KINDS.contains(&kind) {
        return Err(anyhow::anyhow!(
            "field.kind must be one of {}",
            KINDS.join(", ")
        ));
    }

    if obj.get("label").and_then(Value::as_str).is_none() {
        return Err(anyhow::anyhow!("field.label is required and must be a string"));
    }

    if kind == "choice" {
        match obj.get("options") {
            Some(Value::Array(options)) if !options.is_empty() => {}
            _ => {
                return Err(anyhow::anyhow!(
                    "field.options is required and must be a non-empty array for kind \"choice\""
                ));
            }
        }
    }

    Ok(())
}
