use serde::Serialize;

/// AG-UI protocol events streamed to the frontend via SSE.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum AgUiEvent {
    #[serde(rename = "RUN_STARTED")]
    RunStarted {
        #[serde(rename = "threadId")]
        thread_id: String,
        #[serde(rename = "runId")]
        run_id: String,
    },
    #[serde(rename = "TEXT_MESSAGE_START")]
    TextMessageStart {
        #[serde(rename = "threadId")]
        thread_id: String,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "messageId")]
        message_id: String,
    },
    #[serde(rename = "TEXT_MESSAGE_CONTENT")]
    TextMessageContent {
        #[serde(rename = "threadId")]
        thread_id: String,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "messageId")]
        message_id: String,
        delta: String,
    },
    #[serde(rename = "TOOL_CALL_START")]
    ToolCallStart {
        #[serde(rename = "threadId")]
        thread_id: String,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolCallName")]
        tool_call_name: String,
    },
    #[serde(rename = "TOOL_CALL_ARGS")]
    ToolCallArgs {
        #[serde(rename = "threadId")]
        thread_id: String,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        delta: String,
    },
    #[serde(rename = "TOOL_CALL_RESULT")]
    ToolCallResult {
        #[serde(rename = "threadId")]
        thread_id: String,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        content: String,
        #[serde(rename = "isError")]
        is_error: bool,
    },
    #[serde(rename = "RUN_FINISHED")]
    RunFinished {
        #[serde(rename = "threadId")]
        thread_id: String,
        #[serde(rename = "runId")]
        run_id: String,
    },
    #[serde(rename = "RUN_ERROR")]
    RunError {
        #[serde(rename = "threadId")]
        thread_id: String,
        #[serde(rename = "runId")]
        run_id: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<serde_json::Value>,
    },
    #[serde(rename = "CUSTOM")]
    Custom {
        #[serde(rename = "threadId")]
        thread_id: String,
        name: String,
        value: serde_json::Value,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_started_serializes_correctly() {
        let event = AgUiEvent::RunStarted {
            thread_id: "t1".into(),
            run_id: "r1".into(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "RUN_STARTED");
        assert_eq!(json["threadId"], "t1");
        assert_eq!(json["runId"], "r1");
    }

    #[test]
    fn text_message_content_serializes_correctly() {
        let event = AgUiEvent::TextMessageContent {
            thread_id: "t1".into(),
            run_id: "r1".into(),
            message_id: "m1".into(),
            delta: "hello".into(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "TEXT_MESSAGE_CONTENT");
        assert_eq!(json["delta"], "hello");
    }

    #[test]
    fn tool_call_result_serializes_correctly() {
        let event = AgUiEvent::ToolCallResult {
            thread_id: "t1".into(),
            run_id: "r1".into(),
            tool_call_id: "tc1".into(),
            content: "output".into(),
            is_error: true,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "TOOL_CALL_RESULT");
        assert_eq!(json["toolCallId"], "tc1");
        assert_eq!(json["isError"], true);
    }

    #[test]
    fn custom_event_serializes_correctly() {
        let event = AgUiEvent::Custom {
            thread_id: "t1".into(),
            name: "bg_process_started".into(),
            value: serde_json::json!({"id": "p1"}),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "CUSTOM");
        assert_eq!(json["name"], "bg_process_started");
        assert_eq!(json["value"]["id"], "p1");
    }

    #[test]
    fn run_error_optional_details() {
        let event = AgUiEvent::RunError {
            thread_id: "t1".into(),
            run_id: "r1".into(),
            message: "boom".into(),
            details: None,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "RUN_ERROR");
        assert!(json.get("details").is_none());
    }
}
