use tokio::sync::broadcast;

use super::events::{AgUiEvent, EventEnvelope};

/// Facade over the broadcast channel that eliminates boilerplate from event
/// emission sites. Owns the sender + conversation/run identifiers so callers
/// only provide event-specific fields.
///
/// Constructed once per turn in `spawn_agent_turn`, passed by reference
/// through the turn. Background sub-agents clone the emitter for `tokio::spawn`.
#[derive(Clone)]
pub struct TurnEmitter {
    tx: broadcast::Sender<EventEnvelope>,
    thread_id: String,
    run_id: String,
}

impl TurnEmitter {
    pub fn new(
        tx: broadcast::Sender<EventEnvelope>,
        thread_id: String,
        run_id: String,
    ) -> Self {
        Self { tx, thread_id, run_id }
    }

    /// Raw sender — for out-of-scope uses (ProcessManager init, etc.)
    pub fn sender(&self) -> &broadcast::Sender<EventEnvelope> {
        &self.tx
    }

    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }

    #[allow(dead_code)] // part of emitter API, used in tests
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Wrap an event in an envelope with this emitter's routing metadata and send.
    fn emit(&self, event: AgUiEvent) {
        let _ = self.tx.send(EventEnvelope {
            thread_id: Some(self.thread_id.clone()),
            run_id: Some(self.run_id.clone()),
            event,
        });
    }

    // ── Core protocol events ──

    pub fn run_started(&self) {
        self.emit(AgUiEvent::RunStarted);
    }

    pub fn run_finished(&self, has_running_processes: bool) {
        self.emit(AgUiEvent::RunFinished { has_running_processes });
    }

    pub fn run_error(&self, message: impl Into<String>, details: Option<serde_json::Value>) {
        self.emit(AgUiEvent::RunError {
            message: message.into(),
            details,
        });
    }

    // ── Text message events ──

    pub fn text_start(&self, message_id: &str) {
        self.emit(AgUiEvent::TextMessageStart {
            message_id: message_id.to_string(),
        });
    }

    pub fn text_delta(&self, message_id: &str, delta: impl Into<String>) {
        self.emit(AgUiEvent::TextMessageContent {
            message_id: message_id.to_string(),
            delta: delta.into(),
        });
    }

    pub fn text_end(&self, message_id: &str) {
        self.emit(AgUiEvent::TextMessageEnd {
            message_id: message_id.to_string(),
        });
    }

    // ── Tool call events ──

    pub fn tool_start(&self, tool_call_id: &str, tool_name: &str) {
        self.emit(AgUiEvent::ToolCallStart {
            tool_call_id: tool_call_id.to_string(),
            tool_call_name: tool_name.to_string(),
        });
    }

    pub fn tool_args(&self, tool_call_id: &str, delta: impl Into<String>) {
        self.emit(AgUiEvent::ToolCallArgs {
            tool_call_id: tool_call_id.to_string(),
            delta: delta.into(),
        });
    }

    pub fn tool_end(&self, tool_call_id: &str) {
        self.emit(AgUiEvent::ToolCallEnd {
            tool_call_id: tool_call_id.to_string(),
        });
    }

    pub fn tool_result(&self, tool_call_id: &str, content: impl Into<String>, is_error: bool) {
        self.emit(AgUiEvent::ToolCallResult {
            tool_call_id: tool_call_id.to_string(),
            content: content.into(),
            is_error,
        });
    }

    // ── Custom event helpers ──

    pub fn activity(&self, description: impl Into<String>) {
        self.emit(AgUiEvent::Custom {
            name: "activity_update".to_string(),
            value: serde_json::json!({ "activity": description.into() }),
        });
    }

    pub fn usage(
        &self,
        input_tokens: u32,
        output_tokens: u32,
        cache_read: u32,
        cache_creation: u32,
        context_window: u32,
        total_cost: f64,
    ) {
        self.emit(AgUiEvent::Custom {
            name: "usage_update".to_string(),
            value: serde_json::json!({
                "inputTokens": input_tokens,
                "outputTokens": output_tokens,
                "cacheReadInputTokens": cache_read,
                "cacheCreationInputTokens": cache_creation,
                "contextWindow": context_window,
                "totalCost": total_cost,
            }),
        });
    }

    pub fn thinking_start(&self) {
        self.emit(AgUiEvent::Custom {
            name: "thinking_start".to_string(),
            value: serde_json::json!({}),
        });
    }

    pub fn thinking_delta(&self, delta: impl Into<String>) {
        self.emit(AgUiEvent::Custom {
            name: "thinking_delta".to_string(),
            value: serde_json::json!({ "delta": delta.into() }),
        });
    }

    pub fn thinking_end(&self) {
        self.emit(AgUiEvent::Custom {
            name: "thinking_end".to_string(),
            value: serde_json::json!({}),
        });
    }

    pub fn timing(&self, spans: &[super::TimingSpan]) {
        self.emit(AgUiEvent::Custom {
            name: "timing".to_string(),
            value: serde_json::json!({ "spans": spans }),
        });
    }

    pub fn retry(
        &self,
        attempt: u32,
        max_attempts: u32,
        reason: impl Into<String>,
        delay_ms: u64,
    ) {
        self.emit(AgUiEvent::Custom {
            name: "retry".to_string(),
            value: serde_json::json!({
                "attempt": attempt,
                "maxAttempts": max_attempts,
                "reason": reason.into(),
                "delayMs": delay_ms,
            }),
        });
    }

    pub fn sub_agent_start(&self, agent_type: &str, task: &str, context: &str) {
        self.emit(AgUiEvent::Custom {
            name: "sub_agent_start".to_string(),
            value: serde_json::json!({
                "agent_type": agent_type,
                "task": task,
                "context": context,
            }),
        });
    }

    pub fn sub_agent_end(&self, agent_type: &str, value: serde_json::Value) {
        let mut map = match value {
            serde_json::Value::Object(m) => m,
            _ => serde_json::Map::new(),
        };
        map.insert("agent_type".to_string(), serde_json::json!(agent_type));
        self.emit(AgUiEvent::Custom {
            name: "sub_agent_end".to_string(),
            value: serde_json::Value::Object(map),
        });
    }

    pub fn compaction(&self, sealed_span_index: usize, consumed_count: usize) {
        self.emit(AgUiEvent::Custom {
            name: "compaction".to_string(),
            value: serde_json::json!({
                "sealed_span_index": sealed_span_index,
                "consumed_count": consumed_count,
            }),
        });
    }

    /// Escape hatch for truly ad-hoc custom events.
    pub fn custom(&self, name: impl Into<String>, value: serde_json::Value) {
        self.emit(AgUiEvent::Custom {
            name: name.into(),
            value,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_emitter() -> TurnEmitter {
        let (tx, _rx) = broadcast::channel(16);
        TurnEmitter::new(tx, "thread-1".into(), "run-1".into())
    }

    #[test]
    fn run_started_sends_correct_event() {
        let emitter = make_emitter();
        let mut rx = emitter.sender().subscribe();
        emitter.run_started();
        let env = rx.try_recv().unwrap();
        let json = serde_json::to_value(&env).unwrap();
        assert_eq!(json["type"], "RUN_STARTED");
        assert_eq!(json["threadId"], "thread-1");
        assert_eq!(json["runId"], "run-1");
    }

    #[test]
    fn text_lifecycle_emits_start_delta_end() {
        let emitter = make_emitter();
        let mut rx = emitter.sender().subscribe();

        emitter.text_start("msg-1");
        emitter.text_delta("msg-1", "hello ");
        emitter.text_delta("msg-1", "world");
        emitter.text_end("msg-1");

        let e1 = serde_json::to_value(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(e1["type"], "TEXT_MESSAGE_START");
        assert_eq!(e1["messageId"], "msg-1");

        let e2 = serde_json::to_value(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(e2["type"], "TEXT_MESSAGE_CONTENT");
        assert_eq!(e2["delta"], "hello ");

        let e3 = serde_json::to_value(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(e3["type"], "TEXT_MESSAGE_CONTENT");
        assert_eq!(e3["delta"], "world");

        let e4 = serde_json::to_value(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(e4["type"], "TEXT_MESSAGE_END");
        assert_eq!(e4["messageId"], "msg-1");
    }

    #[test]
    fn tool_lifecycle_emits_start_args_end_result() {
        let emitter = make_emitter();
        let mut rx = emitter.sender().subscribe();

        emitter.tool_start("tc-1", "bash");
        emitter.tool_args("tc-1", r#"{"command":"ls"}"#);
        emitter.tool_end("tc-1");
        emitter.tool_result("tc-1", "file.txt", false);

        let e1 = serde_json::to_value(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(e1["type"], "TOOL_CALL_START");
        assert_eq!(e1["toolCallName"], "bash");

        let e2 = serde_json::to_value(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(e2["type"], "TOOL_CALL_ARGS");

        let e3 = serde_json::to_value(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(e3["type"], "TOOL_CALL_END");

        let e4 = serde_json::to_value(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(e4["type"], "TOOL_CALL_RESULT");
        assert_eq!(e4["isError"], false);
    }

    #[test]
    fn activity_sends_custom_event() {
        let emitter = make_emitter();
        let mut rx = emitter.sender().subscribe();
        emitter.activity("Reading files...");
        let json = serde_json::to_value(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(json["type"], "CUSTOM");
        assert_eq!(json["name"], "activity_update");
        assert_eq!(json["value"]["activity"], "Reading files...");
    }

    #[test]
    fn retry_sends_structured_custom() {
        let emitter = make_emitter();
        let mut rx = emitter.sender().subscribe();
        emitter.retry(2, 5, "RateLimit", 4000);
        let json = serde_json::to_value(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(json["name"], "retry");
        assert_eq!(json["value"]["attempt"], 2);
        assert_eq!(json["value"]["maxAttempts"], 5);
        assert_eq!(json["value"]["delayMs"], 4000);
    }

    #[test]
    fn usage_sends_all_fields() {
        let emitter = make_emitter();
        let mut rx = emitter.sender().subscribe();
        emitter.usage(1000, 500, 800, 200, 200_000, 0.05);
        let json = serde_json::to_value(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(json["name"], "usage_update");
        assert_eq!(json["value"]["inputTokens"], 1000);
        assert_eq!(json["value"]["contextWindow"], 200_000);
    }

    #[test]
    fn run_error_with_details() {
        let emitter = make_emitter();
        let mut rx = emitter.sender().subscribe();
        emitter.run_error("boom", Some(serde_json::json!({"kind": "ContextLength"})));
        let json = serde_json::to_value(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(json["type"], "RUN_ERROR");
        assert_eq!(json["message"], "boom");
        assert_eq!(json["details"]["kind"], "ContextLength");
    }

    #[test]
    fn compaction_event() {
        let emitter = make_emitter();
        let mut rx = emitter.sender().subscribe();
        emitter.compaction(3, 42);
        let json = serde_json::to_value(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(json["name"], "compaction");
        assert_eq!(json["value"]["sealed_span_index"], 3);
        assert_eq!(json["value"]["consumed_count"], 42);
    }

    #[test]
    fn sub_agent_end_merges_agent_type() {
        let emitter = make_emitter();
        let mut rx = emitter.sender().subscribe();
        emitter.sub_agent_end("explore", serde_json::json!({
            "summary": "found 3 files",
            "input_tokens": 100,
        }));
        let json = serde_json::to_value(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(json["name"], "sub_agent_end");
        assert_eq!(json["value"]["agent_type"], "explore");
        assert_eq!(json["value"]["summary"], "found 3 files");
    }

    #[test]
    fn clone_preserves_identity() {
        let emitter = make_emitter();
        let cloned = emitter.clone();
        assert_eq!(cloned.thread_id(), "thread-1");
        assert_eq!(cloned.run_id(), "run-1");
    }
}
