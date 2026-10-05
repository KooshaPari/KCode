use super::*;

pub(super) fn validate(request: &Value, response: &Value) -> Result<()> {
    if request["method"] == "session/request_permission" {
        match response["outcome"]["outcome"].as_str() {
            Some("cancelled") => return Ok(()),
            Some("selected") => {
                let id = response["outcome"]["optionId"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("missing optionId"))?;
                if request["params"]["options"]
                    .as_array()
                    .is_some_and(|options| options.iter().any(|option| option["optionId"] == id))
                {
                    return Ok(());
                }
            }
            _ => {}
        }
        bail!("invalid permission outcome");
    }
    match response["action"].as_str() {
        Some("cancel" | "decline") => Ok(()),
        Some("accept") => {
            let schema = &request["params"]["requestedSchema"];
            let properties = schema["properties"]
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("invalid schema"))?;
            let content = response["content"]
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("answer must be an object"))?;
            if let Some(required) = schema["required"].as_array() {
                for key in required.iter().filter_map(Value::as_str) {
                    if !content.contains_key(key) {
                        bail!("missing answer field");
                    }
                }
            }
            for (key, value) in content {
                let field = properties
                    .get(key)
                    .ok_or_else(|| anyhow::anyhow!("unknown answer field"))?;
                let valid = match field["type"].as_str() {
                    Some("boolean") => value.is_boolean(),
                    Some("integer") => value.as_i64().is_some(),
                    Some("string") => value.is_string(),
                    _ => false,
                };
                if !valid {
                    bail!("answer has wrong type");
                }
                if let Some(options) = field["enum"].as_array() {
                    if !options.contains(value) {
                        bail!("answer is not a choice");
                    }
                }
                if let Some(number) = value.as_i64() {
                    if field["minimum"].as_i64().is_some_and(|min| number < min)
                        || field["maximum"].as_i64().is_some_and(|max| number > max)
                    {
                        bail!("answer outside bounds");
                    }
                }
                if let Some(text) = value.as_str() {
                    if field["maxLength"]
                        .as_u64()
                        .is_some_and(|limit| text.chars().count() as u64 > limit)
                    {
                        bail!("answer too long");
                    }
                }
            }
            Ok(())
        }
        _ => bail!("invalid elicitation action"),
    }
}
