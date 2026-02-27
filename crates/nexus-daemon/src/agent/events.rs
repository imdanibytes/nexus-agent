use serde::Serialize;

/// AG-UI protocol events streamed to the frontend via SSE.
///
/// Event-specific data only — routing metadata (`threadId`, `runId`) lives
/// on [`EventEnvelope`], which wraps this enum for broadcast.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum AgUiEvent {
    #[serde(rename = "RUN_STARTED")]
    RunStarted,
    #[serde(rename = "TEXT_MESSAGE_START")]
    TextMessageStart {
        #[serde(rename = "messageId")]
        message_id: String,
    },
    #[serde(rename = "TEXT_MESSAGE_CONTENT")]
    TextMessageContent {
        #[serde(rename = "messageId")]
        message_id: String,
        delta: String,
    },
    #[serde(rename = "TEXT_MESSAGE_END")]
    TextMessageEnd {
        #[serde(rename = "messageId")]
        message_id: String,
    },
    #[serde(rename = "TOOL_CALL_START")]
    ToolCallStart {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolCallName")]
        tool_call_name: String,
    },
    #[serde(rename = "TOOL_CALL_ARGS")]
    ToolCallArgs {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        delta: String,
    },
    #[serde(rename = "TOOL_CALL_END")]
    ToolCallEnd {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
    },
    #[serde(rename = "TOOL_CALL_RESULT")]
    ToolCallResult {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        content: String,
        #[serde(rename = "isError")]
        is_error: bool,
    },
    #[serde(rename = "RUN_FINISHED")]
    RunFinished {
        #[serde(rename = "hasRunningProcesses", default)]
        has_running_processes: bool,
    },
    #[serde(rename = "RUN_ERROR")]
    RunError {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<serde_json::Value>,
    },
    #[serde(rename = "CUSTOM")]
    Custom {
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
    pub fn is_run_started(&self) -> bool {
        matches!(self, Self::RunStarted)
    }

    pub fn is_run_terminal(&self) -> bool {
        matches!(self, Self::RunFinished { .. } | Self::RunError { .. })
    }
}

/// Envelope wrapping an [`AgUiEvent`] with routing metadata.
///
/// Serializes to a flat JSON object that merges `threadId`/`runId` with the
/// event's own fields, preserving the existing AG-UI wire format.
#[derive(Debug, Clone, Serialize)]
pub struct EventEnvelope {
    #[serde(rename = "threadId", skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(rename = "runId", skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(flatten)]
    pub event: AgUiEvent,
}

impl EventEnvelope {
    pub fn thread_id(&self) -> Option<&str> {
        self.thread_id.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(event: AgUiEvent) -> EventEnvelope {
        EventEnvelope {
            thread_id: Some("t1".into()),
            run_id: Some("r1".into()),
            event,
        }
    }

    #[test]
    fn run_started_serializes_correctly() {
        let json = serde_json::to_value(&envelope(AgUiEvent::RunStarted)).unwrap();
        assert_eq!(json["type"], "RUN_STARTED");
        assert_eq!(json["threadId"], "t1");
        assert_eq!(json["runId"], "r1");
    }

    #[test]
    fn text_message_content_serializes_correctly() {
        let json = serde_json::to_value(&envelope(AgUiEvent::TextMessageContent {
            message_id: "m1".into(),
            delta: "hello".into(),
        }))
        .unwrap();
        assert_eq!(json["type"], "TEXT_MESSAGE_CONTENT");
        assert_eq!(json["threadId"], "t1");
        assert_eq!(json["delta"], "hello");
    }

    #[test]
    fn tool_call_result_serializes_correctly() {
        let json = serde_json::to_value(&envelope(AgUiEvent::ToolCallResult {
            tool_call_id: "tc1".into(),
            content: "output".into(),
            is_error: true,
        }))
        .unwrap();
        assert_eq!(json["type"], "TOOL_CALL_RESULT");
        assert_eq!(json["toolCallId"], "tc1");
        assert_eq!(json["isError"], true);
    }

    #[test]
    fn custom_event_serializes_correctly() {
        let json = serde_json::to_value(&envelope(AgUiEvent::Custom {
            name: "bg_process_started".into(),
            value: serde_json::json!({"id": "p1"}),
        }))
        .unwrap();
        assert_eq!(json["type"], "CUSTOM");
        assert_eq!(json["name"], "bg_process_started");
        assert_eq!(json["value"]["id"], "p1");
    }

    #[test]
    fn run_error_optional_details() {
        let json = serde_json::to_value(&envelope(AgUiEvent::RunError {
            message: "boom".into(),
            details: None,
        }))
        .unwrap();
        assert_eq!(json["type"], "RUN_ERROR");
        assert!(json.get("details").is_none());
    }

    #[test]
    fn run_finished_has_running_processes_serializes() {
        let json = serde_json::to_value(&envelope(AgUiEvent::RunFinished {
            has_running_processes: true,
        }))
        .unwrap();
        assert_eq!(json["type"], "RUN_FINISHED");
        assert_eq!(json["hasRunningProcesses"], true);

        let json_false = serde_json::to_value(&envelope(AgUiEvent::RunFinished {
            has_running_processes: false,
        }))
        .unwrap();
        assert_eq!(json_false["hasRunningProcesses"], false);
    }

    #[test]
    fn sync_event_omits_thread_and_run_id() {
        let env = EventEnvelope {
            thread_id: None,
            run_id: None,
            event: AgUiEvent::Sync {
                active_runs: vec!["conv1".into()],
            },
        };
        let json = serde_json::to_value(&env).unwrap();
        assert_eq!(json["type"], "SYNC");
        assert!(json.get("threadId").is_none());
        assert!(json.get("runId").is_none());
        assert_eq!(json["activeRuns"], serde_json::json!(["conv1"]));
    }

    #[test]
    fn thread_id_accessor() {
        let env = envelope(AgUiEvent::RunStarted);
        assert_eq!(env.thread_id(), Some("t1"));

        let env_none = EventEnvelope {
            thread_id: None,
            run_id: None,
            event: AgUiEvent::Sync { active_runs: vec![] },
        };
        assert_eq!(env_none.thread_id(), None);
    }
}
