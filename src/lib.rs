pub mod context;
pub mod decorator;
pub mod error;
pub mod events;
pub mod memory;
pub mod provider;
pub mod session;
pub mod tools;
pub mod types;

use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

pub use context::{
    estimate_str_tokens, estimate_tokens, ContextManager, ManagedContextManager, TokenBudget,
};
pub use error::{AgentError, InferenceError};
pub use events::AgentEvent;
pub use provider::{AnthropicProvider, InferenceProvider};
pub use session::{FileSessionManager, NoSessionManager, SessionManager, SessionState};
pub use decorator::{
    redaction::RedactionTransform, source_tag::SourceTagTransform, Decoration, DecoratorError,
    ToolDecorator, ToolTransform,
};
pub use tools::{ToolHandler, ToolPipeline, ToolRegistry};
pub use types::{ContentBlock, InferenceRequest, InferenceResponse, StopReason, Usage};

/// Agent configuration.
pub struct AgentConfig {
    pub model: String,
    pub max_tokens: u32,
    pub max_turns: usize,
    pub session_id: Option<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-20250514".into(),
            max_tokens: 4096,
            max_turns: 20,
            session_id: None,
        }
    }
}

/// Result of an agent invocation.
#[derive(Debug)]
pub struct AgentResult {
    pub text: String,
    pub turns: usize,
    pub usage: Usage,
}

/// The agent. Wire up a provider, context manager, tools, and go.
pub struct Agent {
    provider: Box<dyn InferenceProvider>,
    context: Box<dyn ContextManager>,
    session: Box<dyn SessionManager>,
    tools: ToolPipeline,
    config: AgentConfig,
}

impl Agent {
    pub fn new(
        provider: impl InferenceProvider + 'static,
        context: impl ContextManager + 'static,
        tools: ToolPipeline,
        config: AgentConfig,
    ) -> Self {
        Self {
            provider: Box::new(provider),
            context: Box::new(context),
            session: Box::new(NoSessionManager),
            tools,
            config,
        }
    }

    pub fn with_session(mut self, session: impl SessionManager + 'static) -> Self {
        self.session = Box::new(session);
        self
    }

    /// Simple invocation. Runs until the model stops or max turns is reached.
    pub async fn invoke(&mut self, prompt: &str) -> Result<AgentResult, AgentError> {
        self.context.add_prompt(prompt);
        self.run_loop(0, None, None).await
    }

    /// Invocation with cancellation support.
    pub async fn invoke_with_cancel(
        &mut self,
        prompt: &str,
        cancel: CancellationToken,
    ) -> Result<AgentResult, AgentError> {
        self.context.add_prompt(prompt);
        self.run_loop(0, Some(cancel), None).await
    }

    /// Invocation with streaming events.
    pub async fn invoke_streaming(
        &mut self,
        prompt: &str,
        tx: tokio::sync::mpsc::Sender<AgentEvent>,
    ) -> Result<AgentResult, AgentError> {
        self.context.add_prompt(prompt);
        self.run_loop(0, None, Some(tx)).await
    }

    /// Resume from a previously checkpointed session.
    pub async fn resume(&mut self, session_id: &str) -> Result<Option<AgentResult>, AgentError> {
        if let Some(state) = self.session.load(session_id).await? {
            self.context.restore(&state.context_snapshot)?;
            self.config.session_id = Some(session_id.to_string());
            let result = self.run_loop(state.turn + 1, None, None).await?;
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    async fn run_loop(
        &mut self,
        start_turn: usize,
        cancel: Option<CancellationToken>,
        tx: Option<tokio::sync::mpsc::Sender<AgentEvent>>,
    ) -> Result<AgentResult, AgentError> {
        let mut total_usage = Usage::default();
        let mut final_text = String::new();

        for turn in start_turn..self.config.max_turns {
            // Check cancellation
            if let Some(ref cancel) = cancel {
                if cancel.is_cancelled() {
                    info!(turn, "agent cancelled");
                    return Err(AgentError::Cancelled);
                }
            }

            if let Some(ref tx) = tx {
                let _ = tx.send(AgentEvent::TurnStart { turn }).await;
            }

            // Auto-compaction: if context is approaching limits, summarize
            if self.context.needs_compaction() {
                if let Some(compact_req) = self.context.build_compaction_request() {
                    info!(turn, "running auto-compaction");
                    let pre_tokens = context::estimate_tokens(
                        &serde_json::Value::Array(
                            self.context.build_request().messages,
                        ),
                    );
                    match self.provider.infer(compact_req).await {
                        Ok(summary_resp) => {
                            let summary_text = summary_resp
                                .content
                                .iter()
                                .filter_map(|b| {
                                    if let ContentBlock::Text(t) = b { Some(t.as_str()) } else { None }
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            self.context.compact(&summary_text);
                            let post_tokens = context::estimate_tokens(
                                &serde_json::Value::Array(
                                    self.context.build_request().messages,
                                ),
                            );
                            total_usage.accumulate(&summary_resp.usage);
                            info!(pre_tokens, post_tokens, "auto-compaction complete");
                            if let Some(ref tx) = tx {
                                let _ = tx
                                    .send(AgentEvent::Compacted {
                                        pre_tokens,
                                        post_tokens,
                                    })
                                    .await;
                            }
                        }
                        Err(e) => {
                            warn!(?e, "auto-compaction failed, continuing without it");
                        }
                    }
                }
            }

            info!(turn, "agent turn");

            // Build and send inference request
            let request = self.context.build_request();
            let response = if let Some(ref cancel) = cancel {
                tokio::select! {
                    result = self.provider.infer(request) => result?,
                    _ = cancel.cancelled() => {
                        info!(turn, "agent cancelled during inference");
                        return Err(AgentError::Cancelled);
                    }
                }
            } else {
                self.provider.infer(request).await?
            };

            total_usage.accumulate(&response.usage);
            self.context.record_response(&response);

            // Extract text from content blocks
            for block in &response.content {
                if let ContentBlock::Text(text) = block {
                    final_text = text.clone();
                    if let Some(ref tx) = tx {
                        let _ = tx
                            .send(AgentEvent::Text {
                                content: text.clone(),
                            })
                            .await;
                    }
                }
            }

            match response.stop_reason {
                StopReason::EndTurn => {
                    if let Some(ref tx) = tx {
                        let _ = tx.send(AgentEvent::Finished { turns: turn + 1 }).await;
                    }
                    info!(turns = turn + 1, "agent finished");
                    return Ok(AgentResult {
                        text: final_text,
                        turns: turn + 1,
                        usage: total_usage,
                    });
                }
                StopReason::ToolUse => {
                    for block in &response.content {
                        if let ContentBlock::ToolUse { id, name, input } = block {
                            if let Some(ref tx) = tx {
                                let _ = tx
                                    .send(AgentEvent::ToolCall {
                                        name: name.clone(),
                                        input: input.clone(),
                                    })
                                    .await;
                            }

                            let result = self.tools.execute(name, input).await;
                            let (output, is_err) = match &result {
                                Ok(s) => (s.as_str(), false),
                                Err(s) => (s.as_str(), true),
                            };

                            if let Some(ref tx) = tx {
                                let _ = tx
                                    .send(AgentEvent::ToolResult {
                                        name: name.clone(),
                                        output: output.to_string(),
                                        is_error: is_err,
                                    })
                                    .await;
                            }

                            self.context.record_tool_result(id, name, output, is_err);
                        }
                    }
                }
                StopReason::MaxTokens => {
                    info!(turn, "response truncated, continuing");
                }
            }

            // Checkpoint after each turn
            if let Some(ref sid) = self.config.session_id {
                let state = SessionState {
                    turn,
                    context_snapshot: self.context.snapshot(),
                    pending_tool_calls: vec![],
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                };
                self.session.checkpoint(sid, &state).await?;
            }
        }

        warn!(
            max_turns = self.config.max_turns,
            "agent hit max turns limit"
        );
        if let Some(ref tx) = tx {
            let _ = tx
                .send(AgentEvent::Finished {
                    turns: self.config.max_turns,
                })
                .await;
        }
        Ok(AgentResult {
            text: final_text,
            turns: self.config.max_turns,
            usage: total_usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use serde_json::json;
    use std::collections::VecDeque;
    use tokio::sync::Mutex;

    // --- Mock Provider ---

    struct MockProvider {
        responses: Mutex<VecDeque<Result<InferenceResponse, InferenceError>>>,
    }

    impl MockProvider {
        fn new(responses: Vec<InferenceResponse>) -> Self {
            Self {
                responses: Mutex::new(responses.into_iter().map(Ok).collect()),
            }
        }

        fn with_error(mut responses: Vec<InferenceResponse>, error: InferenceError) -> Self {
            let mut queue: VecDeque<Result<InferenceResponse, InferenceError>> =
                responses.drain(..).map(Ok).collect();
            queue.push_back(Err(error));
            Self {
                responses: Mutex::new(queue),
            }
        }
    }

    #[async_trait]
    impl InferenceProvider for MockProvider {
        async fn infer(
            &self,
            _request: InferenceRequest,
        ) -> Result<InferenceResponse, InferenceError> {
            self.responses
                .lock()
                .await
                .pop_front()
                .unwrap_or(Err(InferenceError::Request(
                    "no more mock responses".into(),
                )))
        }
    }

    // --- Echo Tool ---

    struct EchoTool;

    #[async_trait]
    impl ToolHandler for EchoTool {
        async fn call(&self, input: &serde_json::Value) -> Result<String, String> {
            Ok(input.to_string())
        }
    }

    // --- Error Tool ---

    struct ErrorTool;

    #[async_trait]
    impl ToolHandler for ErrorTool {
        async fn call(&self, _input: &serde_json::Value) -> Result<String, String> {
            Err("tool failed".into())
        }
    }

    // --- Helpers ---

    fn echo_schema() -> serde_json::Value {
        json!({
            "name": "echo",
            "description": "Echoes input",
            "input_schema": { "type": "object", "properties": {} }
        })
    }

    fn make_agent(provider: MockProvider) -> Agent {
        let registry = ToolRegistry::new().add("echo", echo_schema(), EchoTool);
        let context =
            ManagedContextManager::new("test-model", 1024, 200_000).with_tools(registry.schemas());
        let tools = ToolPipeline::new(registry);
        Agent::new(
            provider,
            context,
            tools,
            AgentConfig {
                model: "test-model".into(),
                max_tokens: 1024,
                max_turns: 20,
                session_id: None,
            },
        )
    }

    // --- Tests ---

    #[tokio::test]
    async fn single_turn_text_response() {
        let provider = MockProvider::new(vec![InferenceResponse {
            stop_reason: StopReason::EndTurn,
            content: vec![ContentBlock::Text("Hello!".into())],
            usage: Usage {
                input_tokens: 10,
                output_tokens: 5,
            },
        }]);

        let mut agent = make_agent(provider);
        let result = agent.invoke("Say hello").await.unwrap();
        assert_eq!(result.text, "Hello!");
        assert_eq!(result.turns, 1);
        assert_eq!(result.usage.input_tokens, 10);
        assert_eq!(result.usage.output_tokens, 5);
    }

    #[tokio::test]
    async fn multi_turn_with_tool_calls() {
        let provider = MockProvider::new(vec![
            InferenceResponse {
                stop_reason: StopReason::ToolUse,
                content: vec![
                    ContentBlock::Text("Let me check.".into()),
                    ContentBlock::ToolUse {
                        id: "call_1".into(),
                        name: "echo".into(),
                        input: json!({"msg": "test"}),
                    },
                ],
                usage: Usage {
                    input_tokens: 20,
                    output_tokens: 15,
                },
            },
            InferenceResponse {
                stop_reason: StopReason::EndTurn,
                content: vec![ContentBlock::Text("Done.".into())],
                usage: Usage {
                    input_tokens: 30,
                    output_tokens: 10,
                },
            },
        ]);

        let mut agent = make_agent(provider);
        let result = agent.invoke("Do something").await.unwrap();
        assert_eq!(result.text, "Done.");
        assert_eq!(result.turns, 2);
        assert_eq!(result.usage.input_tokens, 50);
        assert_eq!(result.usage.output_tokens, 25);
    }

    #[tokio::test]
    async fn max_turns_enforcement() {
        let responses: Vec<InferenceResponse> = (0..3)
            .map(|i| InferenceResponse {
                stop_reason: StopReason::ToolUse,
                content: vec![
                    ContentBlock::Text(format!("turn {i}")),
                    ContentBlock::ToolUse {
                        id: format!("call_{i}"),
                        name: "echo".into(),
                        input: json!({}),
                    },
                ],
                usage: Usage::default(),
            })
            .collect();

        let provider = MockProvider::new(responses);
        let registry = ToolRegistry::new().add("echo", echo_schema(), EchoTool);
        let context =
            ManagedContextManager::new("test-model", 1024, 200_000).with_tools(registry.schemas());
        let tools = ToolPipeline::new(registry);
        let mut agent = Agent::new(
            provider,
            context,
            tools,
            AgentConfig {
                model: "test-model".into(),
                max_tokens: 1024,
                max_turns: 3,
                session_id: None,
            },
        );

        let result = agent.invoke("Keep going").await.unwrap();
        assert_eq!(result.turns, 3);
        assert_eq!(result.text, "turn 2");
    }

    #[tokio::test]
    async fn cancellation_before_first_turn() {
        let provider = MockProvider::new(vec![InferenceResponse {
            stop_reason: StopReason::EndTurn,
            content: vec![ContentBlock::Text("should not reach".into())],
            usage: Usage::default(),
        }]);

        let cancel = CancellationToken::new();
        cancel.cancel();

        let mut agent = make_agent(provider);
        let err = agent
            .invoke_with_cancel("anything", cancel)
            .await
            .unwrap_err();
        assert!(matches!(err, AgentError::Cancelled));
    }

    #[tokio::test]
    async fn api_error_propagates() {
        let provider = MockProvider::with_error(
            vec![],
            InferenceError::ApiError {
                status: 429,
                body: "rate limited".into(),
            },
        );

        let mut agent = make_agent(provider);
        let err = agent.invoke("anything").await.unwrap_err();
        assert!(err.to_string().contains("429"));
    }

    #[tokio::test]
    async fn empty_response_returns_empty_text() {
        let provider = MockProvider::new(vec![InferenceResponse {
            stop_reason: StopReason::EndTurn,
            content: vec![],
            usage: Usage::default(),
        }]);

        let mut agent = make_agent(provider);
        let result = agent.invoke("Do nothing").await.unwrap();
        assert_eq!(result.text, "");
    }

    #[tokio::test]
    async fn tool_error_recorded_as_error() {
        let provider = MockProvider::new(vec![
            InferenceResponse {
                stop_reason: StopReason::ToolUse,
                content: vec![ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "fail_tool".into(),
                    input: json!({}),
                }],
                usage: Usage::default(),
            },
            InferenceResponse {
                stop_reason: StopReason::EndTurn,
                content: vec![ContentBlock::Text("Handled the error.".into())],
                usage: Usage::default(),
            },
        ]);

        let registry = ToolRegistry::new()
            .add("fail_tool", json!({"name": "fail_tool"}), ErrorTool)
            .add("echo", echo_schema(), EchoTool);
        let context =
            ManagedContextManager::new("test-model", 1024, 200_000).with_tools(registry.schemas());
        let tools = ToolPipeline::new(registry);
        let mut agent = Agent::new(
            provider,
            context,
            tools,
            AgentConfig {
                model: "test-model".into(),
                max_tokens: 1024,
                max_turns: 20,
                session_id: None,
            },
        );

        let result = agent.invoke("Try the failing tool").await.unwrap();
        assert_eq!(result.text, "Handled the error.");
        assert_eq!(result.turns, 2);
    }

    #[tokio::test]
    async fn max_tokens_continues_to_next_turn() {
        let provider = MockProvider::new(vec![
            InferenceResponse {
                stop_reason: StopReason::MaxTokens,
                content: vec![ContentBlock::Text("partial".into())],
                usage: Usage::default(),
            },
            InferenceResponse {
                stop_reason: StopReason::EndTurn,
                content: vec![ContentBlock::Text("complete".into())],
                usage: Usage::default(),
            },
        ]);

        let mut agent = make_agent(provider);
        let result = agent.invoke("Write something long").await.unwrap();
        assert_eq!(result.text, "complete");
        assert_eq!(result.turns, 2);
    }

    #[tokio::test]
    async fn session_checkpoint_and_resume() {
        let dir = tempfile::tempdir().unwrap();

        // First run: one turn with tool call, hits max_turns=1
        let provider1 = MockProvider::new(vec![InferenceResponse {
            stop_reason: StopReason::ToolUse,
            content: vec![ContentBlock::ToolUse {
                id: "call_1".into(),
                name: "echo".into(),
                input: json!({"step": 1}),
            }],
            usage: Usage::default(),
        }]);

        let registry1 = ToolRegistry::new().add("echo", echo_schema(), EchoTool);
        let context1 =
            ManagedContextManager::new("test-model", 1024, 200_000).with_tools(registry1.schemas());
        let tools1 = ToolPipeline::new(registry1);
        let mut agent1 = Agent::new(
            provider1,
            context1,
            tools1,
            AgentConfig {
                model: "test-model".into(),
                max_tokens: 1024,
                max_turns: 1,
                session_id: Some("test-session".into()),
            },
        )
        .with_session(FileSessionManager::new(dir.path()));

        let result1 = agent1.invoke("Start work").await.unwrap();
        assert_eq!(result1.turns, 1);

        // Second run: resume from checkpoint
        let provider2 = MockProvider::new(vec![InferenceResponse {
            stop_reason: StopReason::EndTurn,
            content: vec![ContentBlock::Text("Resumed and done.".into())],
            usage: Usage {
                input_tokens: 100,
                output_tokens: 20,
            },
        }]);

        let registry2 = ToolRegistry::new().add("echo", echo_schema(), EchoTool);
        let context2 =
            ManagedContextManager::new("test-model", 1024, 200_000).with_tools(registry2.schemas());
        let tools2 = ToolPipeline::new(registry2);
        let mut agent2 = Agent::new(
            provider2,
            context2,
            tools2,
            AgentConfig {
                model: "test-model".into(),
                max_tokens: 1024,
                max_turns: 20,
                session_id: Some("test-session".into()),
            },
        )
        .with_session(FileSessionManager::new(dir.path()));

        let result2 = agent2.resume("test-session").await.unwrap().unwrap();
        assert_eq!(result2.text, "Resumed and done.");
    }

    #[tokio::test]
    async fn streaming_emits_events() {
        let provider = MockProvider::new(vec![
            InferenceResponse {
                stop_reason: StopReason::ToolUse,
                content: vec![
                    ContentBlock::Text("Working...".into()),
                    ContentBlock::ToolUse {
                        id: "call_1".into(),
                        name: "echo".into(),
                        input: json!({"x": 1}),
                    },
                ],
                usage: Usage::default(),
            },
            InferenceResponse {
                stop_reason: StopReason::EndTurn,
                content: vec![ContentBlock::Text("Done!".into())],
                usage: Usage::default(),
            },
        ]);

        let mut agent = make_agent(provider);
        let (tx, mut rx) = tokio::sync::mpsc::channel(32);
        let result = agent.invoke_streaming("Test", tx).await.unwrap();
        assert_eq!(result.text, "Done!");

        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        // Expected: TurnStart, Text, ToolCall, ToolResult, TurnStart, Text, Finished
        assert!(events.len() >= 7, "got {} events", events.len());
        assert!(matches!(events[0], AgentEvent::TurnStart { turn: 0 }));
        assert!(matches!(events[1], AgentEvent::Text { .. }));
        assert!(matches!(events[2], AgentEvent::ToolCall { .. }));
        assert!(matches!(events[3], AgentEvent::ToolResult { .. }));
        assert!(matches!(events[4], AgentEvent::TurnStart { turn: 1 }));
        assert!(matches!(events[5], AgentEvent::Text { .. }));
        assert!(matches!(events[6], AgentEvent::Finished { turns: 2 }));
    }

    #[tokio::test]
    async fn multiple_tool_calls_in_single_turn() {
        let provider = MockProvider::new(vec![
            InferenceResponse {
                stop_reason: StopReason::ToolUse,
                content: vec![
                    ContentBlock::ToolUse {
                        id: "call_1".into(),
                        name: "echo".into(),
                        input: json!({"a": 1}),
                    },
                    ContentBlock::ToolUse {
                        id: "call_2".into(),
                        name: "echo".into(),
                        input: json!({"b": 2}),
                    },
                ],
                usage: Usage::default(),
            },
            InferenceResponse {
                stop_reason: StopReason::EndTurn,
                content: vec![ContentBlock::Text("Both done.".into())],
                usage: Usage::default(),
            },
        ]);

        let mut agent = make_agent(provider);
        let result = agent.invoke("Do two things").await.unwrap();
        assert_eq!(result.text, "Both done.");
        assert_eq!(result.turns, 2);
    }

    #[tokio::test]
    async fn custom_context_manager() {
        /// A context manager that always injects a custom system prompt.
        struct CustomContext {
            inner: ManagedContextManager,
        }

        impl ContextManager for CustomContext {
            fn build_request(&self) -> InferenceRequest {
                let mut req = self.inner.build_request();
                req.system = Some("You are a pirate.".into());
                req
            }

            fn add_prompt(&mut self, prompt: &str) {
                self.inner.add_prompt(prompt);
            }

            fn record_response(&mut self, response: &InferenceResponse) {
                self.inner.record_response(response);
            }

            fn record_tool_result(
                &mut self,
                call_id: &str,
                name: &str,
                result: &str,
                is_error: bool,
            ) {
                self.inner.record_tool_result(call_id, name, result, is_error);
            }

            fn snapshot(&self) -> serde_json::Value {
                self.inner.snapshot()
            }

            fn restore(
                &mut self,
                snapshot: &serde_json::Value,
            ) -> Result<(), AgentError> {
                self.inner.restore(snapshot)
            }
        }

        let provider = MockProvider::new(vec![InferenceResponse {
            stop_reason: StopReason::EndTurn,
            content: vec![ContentBlock::Text("Ahoy!".into())],
            usage: Usage::default(),
        }]);

        let context = CustomContext {
            inner: ManagedContextManager::new("test-model", 1024, 200_000),
        };

        let mut agent = Agent::new(
            provider,
            context,
            ToolPipeline::new(ToolRegistry::new()),
            AgentConfig {
                model: "test-model".into(),
                max_tokens: 1024,
                max_turns: 20,
                session_id: None,
            },
        );

        let result = agent.invoke("Speak").await.unwrap();
        assert_eq!(result.text, "Ahoy!");
    }

    // -----------------------------------------------------------------------
    // ManagedContextManager Tests
    // -----------------------------------------------------------------------

    mod managed {
        use super::*;
        use context::{estimate_str_tokens, estimate_tokens, ManagedContextManager};
        use serde_json::Value;

        #[test]
        fn token_estimation_chars_div_4() {
            assert_eq!(estimate_str_tokens("hello world"), 2); // 11 / 4 = 2
            assert_eq!(estimate_str_tokens(""), 0);
            // A 400-char string should be ~100 tokens
            let s = "a".repeat(400);
            assert_eq!(estimate_str_tokens(&s), 100);
        }

        #[test]
        fn token_estimation_json_value() {
            let v = json!({"role": "user", "content": "hello"});
            let tokens = estimate_tokens(&v);
            // JSON serialization adds quotes, braces, etc. — should be > 0
            assert!(tokens > 0);
        }

        #[test]
        fn token_budget_effective_window() {
            let budget = context::TokenBudget {
                context_window: 200_000,
                max_output: 4096,
                message_tokens: 0,
                system_tokens: 0,
                tool_schema_tokens: 0,
            };
            // max_output(4096) < 20_000, so effective = 200_000 - 4_096 = 195_904
            assert_eq!(budget.effective_window(), 195_904);
        }

        #[test]
        fn token_budget_effective_window_capped() {
            let budget = context::TokenBudget {
                context_window: 200_000,
                max_output: 32_000,
                message_tokens: 0,
                system_tokens: 0,
                tool_schema_tokens: 0,
            };
            // max_output(32_000) > 20_000, capped at 20_000. effective = 180_000
            assert_eq!(budget.effective_window(), 180_000);
        }

        #[test]
        fn token_budget_usage_fraction() {
            let budget = context::TokenBudget {
                context_window: 100_000,
                max_output: 4096,
                message_tokens: 50_000,
                system_tokens: 1_000,
                tool_schema_tokens: 500,
            };
            let frac = budget.usage_fraction();
            // total = 51_500, effective = 95_904
            assert!(frac > 0.5 && frac < 0.6, "fraction was {frac}");
        }

        #[test]
        fn micro_compaction_noop_under_threshold() {
            // Small context, well under prune threshold — nothing should be pruned
            let mut ctx = ManagedContextManager::new("test", 4096, 200_000);
            ctx.add_prompt("hello");

            // Add a tool result
            let resp = InferenceResponse {
                stop_reason: StopReason::ToolUse,
                content: vec![ContentBlock::ToolUse {
                    id: "c1".into(),
                    name: "read".into(),
                    input: json!({}),
                }],
                usage: Usage::default(),
            };
            ctx.record_response(&resp);
            ctx.record_tool_result("c1", "read", "file contents here", false);

            let req = ctx.build_request();
            // The tool result should NOT be pruned (we're well under threshold)
            let last_msg = req.messages.last().unwrap();
            let content = last_msg["content"][0]["content"].as_str().unwrap();
            assert_eq!(content, "file contents here");
        }

        #[test]
        fn micro_compaction_prunes_old_results() {
            // Create a context that's over the prune threshold
            // Use a tiny context window so our messages fill it up
            let mut ctx = ManagedContextManager::new("test", 100, 2000)
                .with_prune_threshold(0.01)
                .with_min_prune_savings(0);

            ctx.add_prompt("task");

            // Add 5 tool call/result pairs
            for i in 0..5 {
                let resp = InferenceResponse {
                    stop_reason: StopReason::ToolUse,
                    content: vec![ContentBlock::ToolUse {
                        id: format!("c{i}"),
                        name: "read".into(),
                        input: json!({}),
                    }],
                    usage: Usage::default(),
                };
                ctx.record_response(&resp);
                // Large result to make savings worthwhile
                let big_result = "x".repeat(500);
                ctx.record_tool_result(&format!("c{i}"), "read", &big_result, false);
            }

            let req = ctx.build_request();

            // Last 3 tool results should be intact, first 2 should be pruned
            let tool_result_msgs: Vec<&Value> = req
                .messages
                .iter()
                .filter(|m| {
                    m["role"] == "user"
                        && m["content"]
                            .as_array()
                            .and_then(|a| a.first())
                            .and_then(|c| c.get("type"))
                            .and_then(Value::as_str)
                            == Some("tool_result")
                })
                .collect();

            // Check first tool result is pruned
            let first_content = tool_result_msgs[0]["content"][0]["content"]
                .as_str()
                .unwrap();
            assert!(
                first_content.contains("pruned"),
                "expected pruned stub, got: {first_content}"
            );

            // Check last tool result is intact
            let last_content = tool_result_msgs
                .last()
                .unwrap()["content"][0]["content"]
                .as_str()
                .unwrap();
            assert!(
                !last_content.contains("pruned"),
                "last result should be intact"
            );
        }

        #[test]
        fn micro_compaction_preserves_recent_n() {
            let mut ctx = ManagedContextManager::new("test", 100, 2000)
                .with_prune_threshold(0.01)
                .with_min_prune_savings(0)
                .with_keep_recent(2);

            ctx.add_prompt("task");

            for i in 0..4 {
                let resp = InferenceResponse {
                    stop_reason: StopReason::ToolUse,
                    content: vec![ContentBlock::ToolUse {
                        id: format!("c{i}"),
                        name: "read".into(),
                        input: json!({}),
                    }],
                    usage: Usage::default(),
                };
                ctx.record_response(&resp);
                ctx.record_tool_result(
                    &format!("c{i}"),
                    "read",
                    &"data".repeat(200),
                    false,
                );
            }

            let req = ctx.build_request();

            let tool_result_msgs: Vec<&Value> = req
                .messages
                .iter()
                .filter(|m| {
                    m["role"] == "user"
                        && m["content"]
                            .as_array()
                            .and_then(|a| a.first())
                            .and_then(|c| c.get("type"))
                            .and_then(Value::as_str)
                            == Some("tool_result")
                })
                .collect();

            // With keep_recent=2, first 2 of 4 should be pruned
            let first = tool_result_msgs[0]["content"][0]["content"]
                .as_str()
                .unwrap();
            let second = tool_result_msgs[1]["content"][0]["content"]
                .as_str()
                .unwrap();
            let third = tool_result_msgs[2]["content"][0]["content"]
                .as_str()
                .unwrap();
            let fourth = tool_result_msgs[3]["content"][0]["content"]
                .as_str()
                .unwrap();

            assert!(first.contains("pruned"), "1st should be pruned");
            assert!(second.contains("pruned"), "2nd should be pruned");
            assert!(!third.contains("pruned"), "3rd should be intact");
            assert!(!fourth.contains("pruned"), "4th should be intact");
        }

        #[test]
        fn needs_compaction_threshold() {
            // Tiny window: 1000 tokens, threshold at 80%
            let mut ctx = ManagedContextManager::new("test", 100, 1000);
            ctx.add_prompt("short");
            assert!(!ctx.needs_compaction(), "should not need compaction with short prompt");

            // Add enough content to exceed 80% of effective window (900 tokens)
            // 900 * 0.8 = 720 tokens needed. chars/4 heuristic: 720*4 = 2880 chars
            let big = "x".repeat(4000);
            ctx.add_prompt(&big);
            assert!(ctx.needs_compaction(), "should need compaction with large content");
        }

        #[test]
        fn compaction_replaces_messages() {
            let mut ctx = ManagedContextManager::new("test", 100, 1000);
            ctx.add_prompt("first message");
            ctx.add_prompt("second message");
            ctx.add_prompt("third message");

            assert_eq!(ctx.build_request().messages.len(), 3);

            ctx.compact("Summary of all three messages.");
            let req = ctx.build_request();
            assert_eq!(req.messages.len(), 1);
            assert!(req.messages[0]["content"]
                .as_str()
                .unwrap()
                .contains("Summary of all three messages"));
        }

        #[test]
        fn partial_compaction_keeps_boundary() {
            let mut ctx = ManagedContextManager::new("test", 100, 1000);
            ctx.add_prompt("msg 1");
            ctx.add_prompt("msg 2");

            // First compaction: full
            ctx.compact("Summary of msgs 1-2.");
            assert_eq!(ctx.build_request().messages.len(), 1);

            // Add more messages
            ctx.add_prompt("msg 3");
            ctx.add_prompt("msg 4");
            assert_eq!(ctx.build_request().messages.len(), 3);

            // Second compaction: partial — keeps summary, replaces 3 & 4
            ctx.compact("Summary of msgs 3-4.");
            let req = ctx.build_request();
            // Should be: [original summary] + [partial summary]
            assert_eq!(req.messages.len(), 2);
            assert!(req.messages[0]["content"]
                .as_str()
                .unwrap()
                .contains("Summary of msgs 1-2"));
            assert!(req.messages[1]["content"]
                .as_str()
                .unwrap()
                .contains("Summary of msgs 3-4"));
        }

        #[test]
        fn tool_deferral_sends_all_when_no_usage() {
            let schemas = vec![
                json!({"name": "read", "description": "Read a file"}),
                json!({"name": "write", "description": "Write a file"}),
                json!({"name": "exec", "description": "Execute command"}),
            ];
            let ctx = ManagedContextManager::new("test", 4096, 200_000)
                .with_tools(schemas.clone());

            let req = ctx.build_request();
            assert_eq!(req.tools.len(), 3, "all tools sent when none used");
        }

        #[test]
        fn tool_deferral_drops_unused() {
            // Use a small window so schemas take a big fraction
            let schemas = vec![
                json!({"name": "read", "description": "Read a file", "input_schema": {"type": "object", "properties": {"path": {"type": "string", "description": "The file path to read, can be very long with lots of details about what kinds of paths are supported and other verbose documentation that takes up tokens"}}}}),
                json!({"name": "write", "description": "Write a file", "input_schema": {"type": "object", "properties": {"path": {"type": "string", "description": "The file path to write, similarly verbose documentation here to inflate the schema size beyond what would normally be needed"}}}}),
                json!({"name": "exec", "description": "Execute a command", "input_schema": {"type": "object", "properties": {"command": {"type": "string", "description": "The command to execute, with extensive documentation about supported commands and their options"}}}}),
            ];

            // Tiny window so schemas exceed 15% threshold
            let mut ctx = ManagedContextManager::new("test", 100, 500)
                .with_tools(schemas)
                .with_tool_defer_threshold(0.10);

            // Record a response that uses "read"
            let resp = InferenceResponse {
                stop_reason: StopReason::ToolUse,
                content: vec![ContentBlock::ToolUse {
                    id: "c1".into(),
                    name: "read".into(),
                    input: json!({}),
                }],
                usage: Usage::default(),
            };
            ctx.record_response(&resp);

            let req = ctx.build_request();
            // Should only include "read" (the one that was used)
            assert_eq!(req.tools.len(), 1);
            assert_eq!(req.tools[0]["name"], "read");
        }

        #[test]
        fn tool_deferral_preserves_active() {
            let schemas = vec![
                json!({"name": "read", "description": "Read a file", "input_schema": {"type": "object", "properties": {"path": {"type": "string", "description": "Verbose path description padding"}}}}),
                json!({"name": "write", "description": "Write a file", "input_schema": {"type": "object", "properties": {"path": {"type": "string", "description": "Verbose path description padding"}}}}),
            ];

            let mut ctx = ManagedContextManager::new("test", 100, 500)
                .with_tools(schemas)
                .with_tool_defer_threshold(0.10);

            // Use both tools
            let resp = InferenceResponse {
                stop_reason: StopReason::ToolUse,
                content: vec![
                    ContentBlock::ToolUse {
                        id: "c1".into(),
                        name: "read".into(),
                        input: json!({}),
                    },
                    ContentBlock::ToolUse {
                        id: "c2".into(),
                        name: "write".into(),
                        input: json!({}),
                    },
                ],
                usage: Usage::default(),
            };
            ctx.record_response(&resp);

            let req = ctx.build_request();
            assert_eq!(req.tools.len(), 2);
        }

        #[test]
        fn snapshot_restore_roundtrip() {
            let mut ctx = ManagedContextManager::new("test-model", 4096, 200_000)
                .with_system("You are helpful.");

            ctx.add_prompt("hello");
            let resp = InferenceResponse {
                stop_reason: StopReason::ToolUse,
                content: vec![ContentBlock::ToolUse {
                    id: "c1".into(),
                    name: "read".into(),
                    input: json!({}),
                }],
                usage: Usage::default(),
            };
            ctx.record_response(&resp);
            ctx.compact("test summary");

            let snap = ctx.snapshot();

            let mut ctx2 = ManagedContextManager::new("placeholder", 0, 0);
            ctx2.restore(&snap).unwrap();

            assert_eq!(ctx2.build_request().model, "test-model");
            assert_eq!(ctx2.build_request().max_tokens, 4096);
            assert_eq!(
                ctx2.build_request().system,
                Some("You are helpful.".into())
            );
            assert!(ctx2.build_request().messages[0]["content"]
                .as_str()
                .unwrap()
                .contains("test summary"));
        }

        #[tokio::test]
        async fn full_integration_with_compaction() {
            // Agent with ManagedContextManager that triggers compaction mid-conversation.
            // Window of 4000 tokens. Initial prompt + schema fits comfortably.
            // After turn 0 (tool call with large input + large tool result),
            // the context exceeds 60% threshold → compaction fires before turn 1.
            //
            // Mock response order:
            // 1. Turn 0 inference → ToolUse (large input, fills context)
            // 2. Compaction summary call → "Summary: echoed data."
            // 3. Turn 1 inference → EndTurn "All done."

            let provider = MockProvider {
                responses: Mutex::new(VecDeque::from([
                    // Turn 0: tool call with big input that will fill context
                    Ok(InferenceResponse {
                        stop_reason: StopReason::ToolUse,
                        content: vec![ContentBlock::ToolUse {
                            id: "c1".into(),
                            name: "echo".into(),
                            input: json!({"data": "x".repeat(2000)}),
                        }],
                        usage: Usage { input_tokens: 500, output_tokens: 100 },
                    }),
                    // Compaction summary (consumed by compaction call before turn 1)
                    Ok(InferenceResponse {
                        stop_reason: StopReason::EndTurn,
                        content: vec![ContentBlock::Text("Summary: echoed data.".into())],
                        usage: Usage { input_tokens: 300, output_tokens: 50 },
                    }),
                    // Turn 1: final answer (after compaction)
                    Ok(InferenceResponse {
                        stop_reason: StopReason::EndTurn,
                        content: vec![ContentBlock::Text("All done.".into())],
                        usage: Usage { input_tokens: 100, output_tokens: 20 },
                    }),
                ])),
            };

            let registry = ToolRegistry::new().add("echo", echo_schema(), EchoTool);
            // 4000-token window. Initial prompt + schema ≈ 50 tokens (well under 60%).
            // After tool call + result: assistant msg with 2000-char input ≈ 500 tokens,
            // plus tool result (echo returns the input) ≈ 500 tokens → total ≈ 1050.
            // Effective window = 4000 - 100 = 3900. Threshold 0.25 = 975. Should trigger.
            let context = ManagedContextManager::new("test-model", 100, 4000)
                .with_compaction_threshold(0.25)
                .with_tools(registry.schemas());
            let tools = ToolPipeline::new(registry);

            let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(32);

            let mut agent = Agent::new(
                provider,
                context,
                tools,
                AgentConfig {
                    model: "test-model".into(),
                    max_tokens: 100,
                    max_turns: 10,
                    session_id: None,
                },
            );

            let result = agent
                .invoke_streaming("Do something big", event_tx)
                .await
                .unwrap();
            assert_eq!(result.text, "All done.");

            // Check that a Compacted event was emitted
            let mut found_compacted = false;
            while let Ok(event) = event_rx.try_recv() {
                if matches!(event, AgentEvent::Compacted { .. }) {
                    found_compacted = true;
                }
            }
            assert!(found_compacted, "expected a Compacted event");
        }
    }
}
