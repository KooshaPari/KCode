use serde::Deserialize;
use serde_json::Value;

// ---------------------------------------------------------------------------
// CLI output parsing types
// ---------------------------------------------------------------------------

pub(crate) #[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum CliOutput {
    System {
        #[serde(default)]
        pub(crate) session_id: Option<String>,
    },
    StreamEvent {
        pub(crate) event: Value,
        #[serde(default, rename = "session_id")]
        pub(crate) _session_id: Option<String>,
    },
    Assistant {
        pub(crate) message: CliMessage,
        #[serde(default, rename = "session_id")]
        pub(crate) _session_id: Option<String>,
    },
    User {
        pub(crate) message: CliMessage,
        #[serde(default, rename = "session_id")]
        pub(crate) _session_id: Option<String>,
    },
    Result {
        #[serde(default)]
        pub(crate) is_error: bool,
        #[serde(default)]
        pub(crate) usage: Option<UsageInfo>,
        #[serde(default)]
        pub(crate) session_id: Option<String>,
    },
    Error {
        pub(crate) message: String,
        #[serde(default)]
        pub(crate) retry_after_secs: Option<u64>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
pub(crate) struct CliMessage {
    pub(crate) content: Value,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum SdkContentBlock {
    Text {
        pub(crate) text: String,
    },
    ToolUse {
        pub(crate) id: String,
        pub(crate) name: String,
        pub(crate) input: Value,
    },
    ToolResult {
        pub(crate) tool_use_id: String,
        pub(crate) content: Option<Value>,
        #[serde(default)]
        pub(crate) is_error: Option<bool>,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
pub(crate) enum SseEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: Value },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        #[serde(rename = "index")]
        pub(crate) _index: usize,
        pub(crate) content_block: ContentBlockInfo,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta {
        #[serde(rename = "index")]
        pub(crate) _index: usize,
        pub(crate) delta: DeltaInfo,
    },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop {
        #[serde(rename = "index")]
        pub(crate) _index: usize,
    },
    #[serde(rename = "message_delta")]
    MessageDelta {
        pub(crate) delta: MessageDeltaInfo,
        #[serde(default)]
        pub(crate) usage: Option<UsageInfo>,
    },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "error")]
    Error { pub(crate) error: ErrorInfo },
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
pub(crate) enum ContentBlockInfo {
    #[serde(rename = "text")]
    Text {
        #[serde(rename = "text")]
        pub(crate) _text: String,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        pub(crate) id: String,
        pub(crate) name: String,
    },
    #[serde(rename = "thinking")]
    Thinking {
        #[serde(rename = "thinking")]
        pub(crate) _thinking: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
pub(crate) enum DeltaInfo {
    #[serde(rename = "text_delta")]
    TextDelta { pub(crate) text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { pub(crate) partial_json: String },
    #[serde(rename = "thinking_delta")]
    ThinkingDelta {
        #[serde(rename = "thinking")]
        pub(crate) _thinking: String,
    },
    #[serde(rename = "signature_delta")]
    SignatureDelta {
        #[serde(rename = "signature")]
        pub(crate) _signature: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Debug)]
pub(crate) struct UsageInfo {
    #[serde(default)]
    pub(crate) input_tokens: Option<u64>,
    #[serde(default)]
    pub(crate) output_tokens: Option<u64>,
    #[serde(default)]
    pub(crate) cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    pub(crate) cache_read_input_tokens: Option<u64>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct MessageDeltaInfo {
    pub(crate) stop_reason: Option<String>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct ErrorInfo {
    pub(crate) message: String,
    #[serde(default)]
    pub(crate) retry_after_secs: Option<u64>,
    #[serde(default, rename = "status_code")]
    pub(crate) _status_code: Option<u16>,
    #[serde(default, rename = "error_type")]
    pub(crate) _error_type: Option<String>,
}
