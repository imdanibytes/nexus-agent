pub mod events;
pub mod sub_agent;
pub mod tool_dispatch;

use std::time::Instant;

use anyhow::Result;
use futures::StreamExt;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::anthropic::types::*;
use crate::ask_user::PendingQuestionStore;
use crate::mcp::McpManager;
use crate::config::{FetchConfig, FilesystemConfig};
use crate::provider::InferenceProvider;
use crate::system_prompt::fence_tool_result;
use crate::tasks::store::TaskStateStore;
use events::AgUiEvent;
use sub_agent::SubAgentHandler;
use tool_dispatch::{
    AskUserHandler, BashHandler, FetchHandler, FilesystemHandler, McpToolHandler, TaskToolHandler,
    ToolContext,
};

const MAX_ROUNDS: usize = 50;

/// Accumulated tool call from streaming
#[derive(Debug)]
struct PendingToolCall {
    id: String,
    name: String,
    args_json: String,
}

/// Result of a completed agent turn: new messages + timing spans + usage.
/// Always contains partial results — even when the turn ended with an error,
/// messages from completed rounds are included so they can be persisted.
pub struct AgentTurnResult {
    pub messages: Vec<Message>,
    pub timing_spans: Vec<serde_json::Value>,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_input_tokens: u32,
    pub cache_creation_input_tokens: u32,
    pub context_window: u32,
    /// Cost incurred during this turn only (USD).
    pub turn_cost: f64,
    /// If the turn ended with an error, this contains the error message.
    pub error: Option<String>,
    /// Structured error details (serialized ProviderError) for the frontend.
    pub error_details: Option<serde_json::Value>,
}

/// Runs a single agent turn: inference → tool calls → loop.
pub async fn run_agent_turn(
    provider: &dyn InferenceProvider,
    conversation_id: &str,
    messages: Vec<Message>,
    tools: Vec<Tool>,
    system_prompt: Option<String>,
    state_update: Option<String>,
    model: &str,
    max_tokens: u32,
    temperature: Option<f32>,
    mcp: &McpManager,
    fetch_config: &FetchConfig,
    filesystem_config: &FilesystemConfig,
    task_store: &tokio::sync::RwLock<TaskStateStore>,
    pending_questions: &tokio::sync::RwLock<PendingQuestionStore>,
    tx: &broadcast::Sender<AgUiEvent>,
    cancel: CancellationToken,
    depth: u32,
    prior_cost: f64,
) -> Result<AgentTurnResult> {
    let run_id = Uuid::new_v4().to_string();

    let _ = tx.send(AgUiEvent::RunStarted {
        thread_id: conversation_id.to_string(),
        run_id: run_id.clone(),
    });

    let mut messages = messages;
    let mut new_messages = Vec::new();
    let turn_start = Instant::now();
    let mut round_count: usize = 0;
    let mut timing_spans: Vec<serde_json::Value> = Vec::new();
    let turn_span_id = "t-turn".to_string();
    let mut cumulative_input: u32 = 0;
    let mut cumulative_output: u32 = 0;
    let mut cumulative_cache_creation: u32 = 0;
    let mut cumulative_cache_read: u32 = 0;
    let context_window = context_window_for_model(model);
    let mut turn_error: Option<String> = None;
    let mut turn_error_details: Option<serde_json::Value> = None;
    let mut turn_cost: f64 = 0.0;

    for round in 0..MAX_ROUNDS {
        if cancel.is_cancelled() {
            tracing::info!(round, "Agent turn cancelled");
            break;
        }

        // Log system prompt hash on first round to track stability across turns.
        if round == 0 {
            if let Some(ref sp) = system_prompt {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                sp.hash(&mut hasher);
                tracing::info!(system_prompt_hash = hasher.finish(), "System prompt hash");
            }
        }

        tracing::debug!(round, tools = tools.len(), "Starting inference round");

        let round_span_id = format!("t-round-{}", round);
        let round_start = Instant::now();
        let round_start_ms = turn_start.elapsed().as_millis() as u64;

        let inference_start = Instant::now();
        let inference_start_ms = turn_start.elapsed().as_millis() as u64;

        // Inject dynamic state as a synthetic user message before the last
        // message in a per-round clone. The original `messages` stays clean so
        // state doesn't accumulate across rounds.
        let mut messages_for_api = messages.clone();
        if let Some(ref state) = state_update {
            let state_msg = Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: state.clone(),
                }],
            };
            let pos = messages_for_api.len().saturating_sub(1);
            messages_for_api.insert(pos, state_msg);
        }

        let stream = match provider
            .create_message_stream(
                model,
                max_tokens,
                system_prompt.clone(),
                temperature,
                messages_for_api,
                tools.clone(),
            )
            .await
        {
            Ok(s) => s,
            Err(e) => {
                let details = e
                    .downcast_ref::<crate::provider::error::ProviderError>()
                    .and_then(|pe| serde_json::to_value(pe).ok());
                let _ = tx.send(AgUiEvent::RunError {
                    thread_id: conversation_id.to_string(),
                    run_id: run_id.clone(),
                    message: e.to_string(),
                    details: details.clone(),
                });
                turn_error = Some(e.to_string());
                turn_error_details = details;
                break;
            }
        };

        // Consume the stream, emitting AG-UI events
        let stream_result =
            match consume_stream(stream, conversation_id, &run_id, tx, &cancel).await {
                Ok(r) => r,
                Err(e) => {
                    let details = e
                        .downcast_ref::<crate::provider::error::ProviderError>()
                        .and_then(|pe| serde_json::to_value(pe).ok());
                    let _ = tx.send(AgUiEvent::RunError {
                        thread_id: conversation_id.to_string(),
                        run_id: run_id.clone(),
                        message: e.to_string(),
                        details: details.clone(),
                    });
                    turn_error = Some(e.to_string());
                    turn_error_details = details;
                    break;
                }
            };
        let assistant_blocks = stream_result.content_blocks;
        let stop_reason = stream_result.stop_reason;
        let tool_calls = stream_result.tool_calls;
        let round_input_tokens = stream_result.input_tokens;
        let round_output_tokens = stream_result.output_tokens;
        let round_cache_creation = stream_result.cache_creation_input_tokens;
        let round_cache_read = stream_result.cache_read_input_tokens;

        let inference_duration = inference_start.elapsed().as_millis() as u64;
        let llm_span_id = format!("t-llm-{}", round);
        timing_spans.push(serde_json::json!({
            "id": llm_span_id,
            "name": "llm_call",
            "parentId": round_span_id,
            "startMs": inference_start_ms,
            "endMs": inference_start_ms + inference_duration,
            "durationMs": inference_duration,
        }));

        // Accumulate and emit usage.
        // With prompt caching, the API's input_tokens only counts uncached tokens.
        // Total input = input_tokens + cache_creation + cache_read.
        let round_total_input =
            round_input_tokens + round_cache_creation + round_cache_read;
        cumulative_input = round_total_input; // Latest call's input = full context
        cumulative_output += round_output_tokens;
        cumulative_cache_creation += round_cache_creation;
        cumulative_cache_read += round_cache_read;

        tracing::info!(
            round,
            input_tokens = round_input_tokens,
            cache_read = round_cache_read,
            cache_creation = round_cache_creation,
            cache_hit_pct = if round_total_input > 0 {
                (round_cache_read as f64 / round_total_input as f64 * 100.0) as u32
            } else {
                0
            },
            "Cache metrics"
        );

        // Calculate cost for this round (cache-aware)
        let round_cost = crate::pricing::calculate_cost_with_cache(
            model,
            round_input_tokens,
            round_cache_creation,
            round_cache_read,
            round_output_tokens,
        );
        turn_cost += round_cost;

        let _ = tx.send(AgUiEvent::Custom {
            thread_id: conversation_id.to_string(),
            name: "usage_update".to_string(),
            value: serde_json::json!({
                "inputTokens": cumulative_input,
                "outputTokens": cumulative_output,
                "cacheReadInputTokens": cumulative_cache_read,
                "cacheCreationInputTokens": cumulative_cache_creation,
                "contextWindow": context_window,
                "totalCost": prior_cost + turn_cost,
            }),
        });

        // Build assistant message from accumulated blocks
        let assistant_msg = Message {
            role: Role::Assistant,
            content: assistant_blocks,
        };
        messages.push(assistant_msg.clone());
        new_messages.push(assistant_msg);

        round_count = round + 1;

        match stop_reason {
            Some(StopReason::ToolUse) if !tool_calls.is_empty() => {
                let mut result_blocks = Vec::new();
                let tool_exec_start_ms = turn_start.elapsed().as_millis() as u64;
                let tool_exec_span_id = format!("t-toolexec-{}", round);
                let tool_exec_start = Instant::now();

                let ask_handler = AskUserHandler { pending_questions };
                let task_handler = TaskToolHandler { task_store };
                let fetch_handler = FetchHandler { fetch_config };
                let fs_handler = FilesystemHandler::new(filesystem_config);
                let bash_handler = BashHandler {
                    working_dir: filesystem_config
                        .allowed_directories
                        .first()
                        .cloned(),
                };
                let sub_agent_handler = SubAgentHandler {
                    provider,
                    model,
                    max_tokens,
                    temperature,
                    mcp,
                    fetch_config,
                    filesystem_config,
                    task_store,
                    pending_questions,
                    parent_messages: &messages,
                    parent_tools: &tools,
                    cumulative_cost: prior_cost + turn_cost,
                };
                let mcp_handler = McpToolHandler { mcp };
                let mut handlers: Vec<&dyn tool_dispatch::ToolHandler> =
                    vec![&ask_handler, &task_handler, &fetch_handler, &fs_handler, &bash_handler];
                if depth == 0 {
                    handlers.push(&sub_agent_handler);
                }
                handlers.push(&mcp_handler);

                for tc in &tool_calls {
                    let tool_start_ms = turn_start.elapsed().as_millis() as u64;
                    let tool_start = Instant::now();

                    // Extract the description field the model was required to fill out
                    let tool_description = serde_json::from_str::<serde_json::Value>(&tc.args_json)
                        .ok()
                        .and_then(|v| v.get("description").and_then(|d| d.as_str()).map(|s| s.to_string()));

                    if let Some(ref desc) = tool_description {
                        let _ = tx.send(AgUiEvent::Custom {
                            thread_id: conversation_id.to_string(),
                            name: "activity_update".to_string(),
                            value: serde_json::json!({ "activity": desc }),
                        });
                    }

                    let ctx = ToolContext {
                        tool_call_id: &tc.id,
                        tool_name: &tc.name,
                        args_json: &tc.args_json,
                        conversation_id,
                        tx,
                        cancel: &cancel,
                    };
                    let result = tool_dispatch::dispatch_tool_call(&handlers, ctx).await;
                    let content = result.content;
                    let is_error = result.is_error;
                    let tool_duration = tool_start.elapsed().as_millis() as u64;

                    let _ = tx.send(AgUiEvent::ToolCallResult {
                        thread_id: conversation_id.to_string(),
                        run_id: run_id.clone(),
                        tool_call_id: tc.id.clone(),
                        content: content.clone(),
                        is_error,
                    });

                    timing_spans.push(serde_json::json!({
                        "id": format!("t-tool-{}", tc.id),
                        "name": format!("tool:{}", tc.name),
                        "parentId": tool_exec_span_id,
                        "startMs": tool_start_ms,
                        "endMs": tool_start_ms + tool_duration,
                        "durationMs": tool_duration,
                    }));

                    result_blocks.push(ContentBlock::ToolResult {
                        tool_use_id: tc.id.clone(),
                        content: fence_tool_result(&content),
                        is_error: Some(is_error),
                    });
                }

                let tool_exec_duration = tool_exec_start.elapsed().as_millis() as u64;
                timing_spans.push(serde_json::json!({
                    "id": tool_exec_span_id,
                    "name": "tool_execution",
                    "parentId": round_span_id,
                    "startMs": tool_exec_start_ms,
                    "endMs": tool_exec_start_ms + tool_exec_duration,
                    "durationMs": tool_exec_duration,
                }));

                let tool_results_msg = Message {
                    role: Role::User,
                    content: result_blocks,
                };
                messages.push(tool_results_msg.clone());
                new_messages.push(tool_results_msg);
            }
            _ => {
                // end_turn, max_tokens, or no tool calls — we're done
                let round_duration = round_start.elapsed().as_millis() as u64;
                timing_spans.push(serde_json::json!({
                    "id": round_span_id,
                    "name": format!("round:{}", round + 1),
                    "parentId": turn_span_id,
                    "startMs": round_start_ms,
                    "endMs": round_start_ms + round_duration,
                    "durationMs": round_duration,
                }));
                break;
            }
        }

        let round_duration = round_start.elapsed().as_millis() as u64;
        timing_spans.push(serde_json::json!({
            "id": round_span_id,
            "name": format!("round:{}", round + 1),
            "parentId": turn_span_id,
            "startMs": round_start_ms,
            "endMs": round_start_ms + round_duration,
            "durationMs": round_duration,
        }));
    }

    let turn_duration = turn_start.elapsed().as_millis() as u64;
    timing_spans.insert(0, serde_json::json!({
        "id": turn_span_id,
        "name": "turn",
        "parentId": serde_json::Value::Null,
        "startMs": 0,
        "endMs": turn_duration,
        "durationMs": turn_duration,
    }));

    let _ = tx.send(AgUiEvent::Custom {
        thread_id: conversation_id.to_string(),
        name: "timing".to_string(),
        value: serde_json::json!({ "spans": timing_spans }),
    });

    // Turn-level cache summary
    let cache_savings_pct = if cumulative_input > 0 {
        (cumulative_cache_read as f64 / (cumulative_input + cumulative_cache_read) as f64
            * 100.0) as u32
    } else {
        0
    };
    tracing::info!(
        rounds = round_count,
        total_input = cumulative_input,
        total_output = cumulative_output,
        total_cache_read = cumulative_cache_read,
        total_cache_creation = cumulative_cache_creation,
        cache_savings_pct,
        turn_cost_usd = format!("{:.6}", turn_cost),
        "Turn complete"
    );

    let _ = tx.send(AgUiEvent::RunFinished {
        thread_id: conversation_id.to_string(),
        run_id,
    });

    Ok(AgentTurnResult {
        messages: new_messages,
        timing_spans,
        input_tokens: cumulative_input,
        output_tokens: cumulative_output,
        cache_creation_input_tokens: cumulative_cache_creation,
        cache_read_input_tokens: cumulative_cache_read,
        context_window,
        turn_cost,
        error: turn_error,
        error_details: turn_error_details,
    })
}

/// Result of consuming a single inference stream.
struct StreamResult {
    content_blocks: Vec<ContentBlock>,
    stop_reason: Option<StopReason>,
    tool_calls: Vec<PendingToolCall>,
    input_tokens: u32,
    output_tokens: u32,
    cache_creation_input_tokens: u32,
    cache_read_input_tokens: u32,
}

/// Consume the provider stream, emit AG-UI events, return accumulated content.
async fn consume_stream(
    mut stream: futures::stream::BoxStream<'static, Result<StreamEvent>>,
    conversation_id: &str,
    run_id: &str,
    tx: &broadcast::Sender<AgUiEvent>,
    cancel: &CancellationToken,
) -> Result<StreamResult>
{
    let mut content_blocks: Vec<ContentBlock> = Vec::new();
    let mut stop_reason = None;
    let mut pending_tool_calls: Vec<PendingToolCall> = Vec::new();
    let mut input_tokens: u32 = 0;
    let mut output_tokens: u32 = 0;
    let mut cache_creation_input_tokens: u32 = 0;
    let mut cache_read_input_tokens: u32 = 0;

    // Track current content blocks by index
    let mut current_text: Option<(usize, String)> = None;
    let mut current_tool: Option<(usize, PendingToolCall)> = None;
    let mut current_thinking: Option<(usize, String)> = None;
    let mut message_id = String::new();

    while let Some(event) = stream.next().await {
        if cancel.is_cancelled() {
            break;
        }

        let event = event?;

        match event {
            StreamEvent::MessageStart {
                message_id: mid,
                usage: u,
                ..
            } => {
                message_id = mid;
                if let Some(u) = u {
                    input_tokens = u.input_tokens;
                    cache_creation_input_tokens = u.cache_creation_input_tokens;
                    cache_read_input_tokens = u.cache_read_input_tokens;
                }
            }
            StreamEvent::ContentBlockStart {
                index,
                content_block,
            } => match content_block {
                ContentBlockInfo::Text => {
                    let _ = tx.send(AgUiEvent::TextMessageStart {
                        thread_id: conversation_id.to_string(),
                        run_id: run_id.to_string(),
                        message_id: message_id.clone(),
                    });
                    current_text = Some((index, String::new()));
                }
                ContentBlockInfo::ToolUse { id, name } => {
                    let _ = tx.send(AgUiEvent::ToolCallStart {
                        thread_id: conversation_id.to_string(),
                        run_id: run_id.to_string(),
                        tool_call_id: id.clone(),
                        tool_call_name: name.clone(),
                    });
                    current_tool = Some((
                        index,
                        PendingToolCall {
                            id,
                            name,
                            args_json: String::new(),
                        },
                    ));
                }
                ContentBlockInfo::Thinking => {
                    let _ = tx.send(AgUiEvent::Custom {
                        thread_id: conversation_id.to_string(),
                        name: "thinking_start".to_string(),
                        value: serde_json::json!({}),
                    });
                    current_thinking = Some((index, String::new()));
                }
            },
            StreamEvent::ContentBlockDelta { index, delta } => match delta {
                Delta::TextDelta { text } => {
                    if let Some((idx, ref mut buf)) = current_text {
                        if idx == index {
                            buf.push_str(&text);
                            let _ = tx.send(AgUiEvent::TextMessageContent {
                                thread_id: conversation_id.to_string(),
                                run_id: run_id.to_string(),
                                message_id: message_id.clone(),
                                delta: text,
                            });
                        }
                    }
                }
                Delta::InputJsonDelta { partial_json } => {
                    if let Some((idx, ref mut tc)) = current_tool {
                        if idx == index {
                            tc.args_json.push_str(&partial_json);
                            let _ = tx.send(AgUiEvent::ToolCallArgs {
                                thread_id: conversation_id.to_string(),
                                run_id: run_id.to_string(),
                                tool_call_id: tc.id.clone(),
                                delta: partial_json,
                            });
                        }
                    }
                }
                Delta::ThinkingDelta { thinking } => {
                    if let Some((idx, ref mut buf)) = current_thinking {
                        if idx == index {
                            buf.push_str(&thinking);
                            let _ = tx.send(AgUiEvent::Custom {
                                thread_id: conversation_id.to_string(),
                                name: "thinking_delta".to_string(),
                                value: serde_json::json!({ "delta": thinking }),
                            });
                        }
                    }
                }
            },
            StreamEvent::ContentBlockStop { index } => {
                if let Some((idx, text)) = current_text.take() {
                    if idx == index {
                        content_blocks.push(ContentBlock::Text { text });
                    } else {
                        current_text = Some((idx, text));
                    }
                }
                if let Some((idx, tc)) = current_tool.take() {
                    if idx == index {
                        let input: serde_json::Value =
                            serde_json::from_str(&tc.args_json)
                                .unwrap_or_else(|_| serde_json::json!({}));
                        content_blocks.push(ContentBlock::ToolUse {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                            input,
                        });
                        pending_tool_calls.push(tc);
                    } else {
                        current_tool = Some((idx, tc));
                    }
                }
                if let Some((idx, thinking)) = current_thinking.take() {
                    if idx == index {
                        content_blocks.push(ContentBlock::Thinking { thinking });
                        let _ = tx.send(AgUiEvent::Custom {
                            thread_id: conversation_id.to_string(),
                            name: "thinking_end".to_string(),
                            value: serde_json::json!({}),
                        });
                    } else {
                        current_thinking = Some((idx, thinking));
                    }
                }
            }
            StreamEvent::MessageDelta {
                stop_reason: sr,
                usage: u,
            } => {
                stop_reason = sr;
                if let Some(u) = u {
                    output_tokens = u.output_tokens;
                }
            }
            StreamEvent::MessageStop => break,
            StreamEvent::Error { error_type, message } => {
                let err = crate::provider::error::ProviderError::from_anthropic_stream(
                    error_type.as_deref(),
                    &message,
                );
                return Err(err.into());
            }
            StreamEvent::Ping => {}
        }
    }

    Ok(StreamResult {
        content_blocks,
        stop_reason,
        tool_calls: pending_tool_calls,
        input_tokens,
        output_tokens,
        cache_creation_input_tokens,
        cache_read_input_tokens,
    })
}

pub fn context_window_for_model(model: &str) -> u32 {
    crate::pricing::context_window(model)
}
