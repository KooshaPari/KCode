use super::{ToolContext, ToolOutput};
use anyhow::{Result, bail};
use jcode_tool_core::ElicitRequest;
use serde_json::{Value, json};

pub(super) async fn elicit(ctx: &ToolContext, request: ElicitRequest) -> Result<ToolOutput> {
    if request.urgency == "secret" || request.field["secret"] == true {
        bail!("ACP form elicitation cannot collect secrets");
    }
    let kind = request.field["kind"].as_str().unwrap_or_default();
    let mut field = match kind {
        "text" | "long_text" | "date_time" => json!({"type":"string"}),
        "boolean" => json!({"type":"boolean"}),
        "integer" => json!({"type":"integer"}),
        "choice" => {
            let options = request.field["options"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("choice options required"))?;
            let values: Vec<Value> = options
                .iter()
                .map(|option| option["value"].clone())
                .collect();
            if values.is_empty() || values.iter().any(|value| !value.is_string()) {
                bail!("invalid choice values");
            }
            json!({"type":"string","enum":values})
        }
        _ => bail!("unsupported ACP form field"),
    };
    field["title"] = request.field["label"].clone();
    for (source, target) in [
        ("min", "minimum"),
        ("max", "maximum"),
        ("max_length", "maxLength"),
        ("default", "default"),
    ] {
        if let Some(value) = request.field.get(source) {
            field[target] = value.clone();
        }
    }
    let mut schema = json!({"type":"object","properties":{"value":field},"required":["value"]});
    if let Some(notes) = request.notes {
        schema["properties"]["notes"] = json!({"type":"string","title":notes["label"]});
        if notes["required"] == true {
            schema["required"]
                .as_array_mut()
                .unwrap()
                .push(json!("notes"));
        }
    }
    let response = super::interaction::request(
        &ctx.session_id,
        "elicitation/create",
        json!({
            "sessionId":ctx.session_id,"toolCallId":ctx.tool_call_id,"mode":"form",
            "message":format!("{}\n{}",request.title, request.question.unwrap_or_default()),
            "requestedSchema":schema
        }),
        request.timeout_secs as u64,
    )
    .await?;
    let value = response["content"].get("value").map(|value| {
        value
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| value.to_string())
    });
    Ok(ToolOutput {
        output:
            json!({"action":response["action"],"value":value,"notes":response["content"]["notes"]})
                .to_string(),
        title: None,
        metadata: None,
        images: vec![],
    })
}
