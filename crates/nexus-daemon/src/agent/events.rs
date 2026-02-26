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
    #[serde(rename = "TEXT_MESSAGE_END")]
    TextMessageEnd {
        #[serde(rename = "threadId")]
        thread_id: String,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "messageId")]
        message_id: String,
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
    #[serde(rename = "TOOL_CALL_END")]
    ToolCallEnd {
        #[serde(rename = "threadId")]
        thread_id: String,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
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
        /// True if the conversation has background processes still running.
        /// The client should keep the SSE connection open to receive their
        /// completion notifications.
        #[serde(rename = "hasRunningProcesses", default)]
        has_running_processes: bool,
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
    /// Sent once when a client connects to the global SSE stream.
    /// Contains the list of conversations with in-progress turns.
    #[serde(rename = "SYNC")]
    Sync {
        #[serde(rename = "activeRuns")]
        active_runs: Vec<String>,
    },
}

impl AgUiEvent {
    /// Extract the thread_id from any event variant (if present).
    pub fn thread_id(&self) -> Option<&str> {
        match self {
            Self::RunStarted { thread_id, .. }
            | Self::TextMessageStart { thread_id, .. }
            | Self::TextMessageContent { thread_id, .. }
            | Self::TextMessageEnd { thread_id, .. }
            | Self::ToolCallStart { thread_id, .. }
            | Self::ToolCallArgs { thread_id, .. }
            | Self::ToolCallEnd { thread_id, .. }
            | Self::ToolCallResult { thread_id, .. }
            | Self::RunFinished { thread_id, .. }
            | Self::RunError { thread_id, .. }
            | Self::Custom { thread_id, .. } => Some(thread_id),
            Self::Sync { .. } => None,
        }
    }

    /// Check if this is a RUN_STARTED event.
    pub fn is_run_started(&self) -> bool {
        matches!(self, Self::RunStarted { .. })
    }

    /// Check if this is a terminal event (RUN_FINISHED or RUN_ERROR).
    pub fn is_run_terminal(&self) -> bool {
        matches!(self, Self::RunFinished { .. } | Self::RunError { .. })
    }
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

    #[test]
    fn run_finished_has_running_processes_serializes() {
        let event = AgUiEvent::RunFinished {
            thread_id: "t1".into(),
            run_id: "r1".into(),
            has_running_processes: true,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "RUN_FINISHED");
        assert_eq!(json["hasRunningProcesses"], true);

        let event_false = AgUiEvent::RunFinished {
            thread_id: "t1".into(),
            run_id: "r1".into(),
            has_running_processes: false,
        };
        let json_false = serde_json::to_value(&event_false).unwrap();
        assert_eq!(json_false["hasRunningProcesses"], false);
    }
}
