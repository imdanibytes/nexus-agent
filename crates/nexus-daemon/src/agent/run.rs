use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use futures::StreamExt;
use tokio_util::sync::CancellationToken;

use nexus_provider::types::*;
use crate::bg_process::ProcessManager;
use crate::bg_process::tools::BgProcessToolHandler;
use crate::system_prompt::fence_tool_result;
use super::emitter::TurnEmitter;
use super::sub_agent::SubAgentHandler;
use super::tool_dispatch::{
    self, AskUserHandler, BashHandler, ControlPlaneHandler, FetchHandler, FilesystemHandler,
    McpToolHandler, ResourceToolHandler, TaskToolHandler, ToolContext,
};
use crate::module::{
    PreToolUseEvent, PreToolUseDecision, PostToolUseEvent, PostToolUseFailureEvent,
    StopEvent, StopDecision, PreCompactEvent, CompactionLayer,
};
use nexus_provider::InferenceRequest;
use super::{AgentTurnResult, InferenceConfig, TimingSpan, TurnContext, TurnServices};

const MAX_ROUNDS: usize = 50;

/// Accumulated tool call from streaming.
#[derive(Debug)]
struct PendingToolCall {
    id: String,
    name: String,
    args_json: String,
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

/// Runs a single agent turn: inference → tool calls → loop.
pub async fn run_agent_turn(
    inference: &InferenceConfig<'_>,
    context: TurnContext,
    services: &TurnServices<'_>,
    emitter: &TurnEmitter,
    cancel: CancellationToken,
) -> Result<AgentTurnResult> {
    // Bind struct fields to local names for ergonomics
    let conversation_id = &context.conversation_id;
    let mut messages = context.messages;
    let tools = context.tools;
    let depth = context.depth;
    let prior_cost = context.prior_cost;

    emitter.run_started();

    let mut new_messages = Vec::new();
    let turn_start = Instant::now();
    let mut round_count: usize = 0;
    let mut timing_spans: Vec<TimingSpan> = Vec::new();
    let turn_span_id = "t-turn".to_string();
    let mut cumulative_input: u32 = 0;
    let mut cumulative_output: u32 = 0;
    let mut cumulative_cache_creation: u32 = 0;
    let mut cumulative_cache_read: u32 = 0;
    let context_window = super::context_window_for_model(inference.model);
    let mut turn_error: Option<String> = None;
    let mut turn_error_details: Option<serde_json::Value> = None;
    let mut turn_cost: f64 = 0.0;
    let mut retried_after_prune = false;
    let mut retry_count: u32 = 0;

    // Construct stable handlers once — these don't change between rounds.
    let ask_handler = AskUserHandler { pending_questions: services.pending_questions };
    let task_handler = TaskToolHandler { task_store: services.task_store };
    let fetch_handler = FetchHandler { fetch_config: services.fetch_config };
    let fs_handler = FilesystemHandler::new(services.filesystem_config);
    let bash_handler = BashHandler {
        working_dir: services.filesystem_config
            .allowed_directories
            .first()
            .cloned(),
        process_manager: services.process_manager.clone()
            .unwrap_or_else(|| {
                let (queue, _rx) = crate::server::message_queue::MessageQueue::new();
                Arc::new(ProcessManager::new(
                    std::path::PathBuf::from("/tmp/nexus-bg"),
                    emitter.sender().clone(),
                    Arc::new(queue),
                ))
            }),
    };
    let bg_handler = services.process_manager.as_ref().map(|pm| BgProcessToolHandler {
        process_manager: pm.as_ref(),
    });
    let control_plane_handler = services.control_plane.as_ref().map(|deps| ControlPlaneHandler {
        deps: Arc::clone(deps),
    });
    let resource_handler = ResourceToolHandler { mcp: services.mcp };
    let mcp_handler = McpToolHandler { mcp: services.mcp };

    for round in 0..MAX_ROUNDS {
        if cancel.is_cancelled() {
            tracing::info!(round, "Agent turn cancelled");
            break;
        }

        // Log system prompt hash on first round to track stability across turns.
        if round == 0 {
            if let Some(ref sp) = inference.system_prompt {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                sp.hash(&mut hasher);
                tracing::info!(system_prompt_hash = hasher.finish(), "System prompt hash");
            }

            // Note: turn_start hook now fires in turn.rs during setup (before
            // the agent loop), where modules can contribute prompt/status sections.
        }

        // Mid-turn context compaction: prune tool results before they overflow.
        // On round 0 the caller (turn.rs) already compacted, so skip.
        if round > 0 {
            let estimated = nexus_compaction::estimate_tokens(
                &messages,
                inference.system_prompt.as_deref(),
                &tools,
            );
            let prune_threshold = (context_window as f64 * 0.70) as u32;
            let aggressive_threshold = (context_window as f64 * 0.85) as u32;

            if estimated > aggressive_threshold {
                // HOOK: PreCompact — let modules save state before pruning.
                services.modules.fire_pre_compact(&PreCompactEvent {
                    conversation_id,
                    estimated_tokens: estimated,
                    context_window,
                    layer: CompactionLayer::Prune,
                }).await;

                tracing::warn!(
                    round,
                    estimated,
                    threshold = aggressive_threshold,
                    "Mid-turn aggressive pruning (>85% context)"
                );
                nexus_compaction::prune_tool_results(&mut messages, 1);
            } else if estimated > prune_threshold {
                // HOOK: PreCompact
                services.modules.fire_pre_compact(&PreCompactEvent {
                    conversation_id,
                    estimated_tokens: estimated,
                    context_window,
                    layer: CompactionLayer::Prune,
                }).await;

                tracing::info!(
                    round,
                    estimated,
                    threshold = prune_threshold,
                    "Mid-turn pruning (>70% context)"
                );
                nexus_compaction::prune_tool_results(&mut messages, 3);
            }
        }

        tracing::debug!(round, tools = tools.len(), "Starting inference round");

        let round_span_id = format!("t-round-{}", round);
        let round_start = Instant::now();
        let round_start_ms = turn_start.elapsed().as_millis() as u64;

        let inference_start = Instant::now();
        let inference_start_ms = turn_start.elapsed().as_millis() as u64;

        let mut messages_for_api = messages.clone();
        if let Some(ref state) = inference.state_update {
            inject_state_update(&mut messages_for_api, state);
        }

        let stream = match inference.provider
            .create_message_stream(InferenceRequest {
                model: inference.model.to_string(),
                max_tokens: inference.max_tokens,
                system: inference.system_prompt.clone(),
                temperature: inference.temperature,
                thinking_budget: inference.thinking_budget,
                messages: messages_for_api,
                tools: tools.clone(),
            })
            .await
        {
            Ok(s) => s,
            Err(e) => {
                // Retry once on ContextLength with aggressive pruning
                if !retried_after_prune {
                    if let Some(pe) = e.downcast_ref::<nexus_provider::error::ProviderError>() {
                        if matches!(pe.kind, nexus_provider::error::ProviderErrorKind::ContextLength) {
                            retried_after_prune = true;
                            tracing::warn!("Context length exceeded, aggressive pruning and retrying");
                            nexus_compaction::prune_tool_results(&mut messages, 1);
                            continue;
                        }
                    }
                }

                // Retry transient errors with exponential backoff
                if let Some(pe) = e.downcast_ref::<nexus_provider::error::ProviderError>() {
                    if pe.retryable && retry_count < crate::retry::MAX_RETRIES {
                        retry_count += 1;
                        let delay = crate::retry::backoff_delay(retry_count);
                        tracing::warn!(
                            attempt = retry_count,
                            delay_ms = delay,
                            error_kind = ?pe.kind,
                            "Retrying after transient error"
                        );
                        emitter.retry(retry_count, crate::retry::MAX_RETRIES, format!("{:?}", pe.kind), delay);
                        tokio::select! {
                            _ = tokio::time::sleep(std::time::Duration::from_millis(delay)) => {}
                            _ = cancel.cancelled() => {
                                turn_error = Some(e.to_string());
                                break;
                            }
                        }
                        continue;
                    }
                }

                let details = e
                    .downcast_ref::<nexus_provider::error::ProviderError>()
                    .and_then(|pe| serde_json::to_value(pe).ok());
                emitter.run_error(e.to_string(), details.clone());
                turn_error = Some(e.to_string());
                turn_error_details = details;
                break;
            }
        };

        // Consume the stream, emitting AG-UI events
        let stream_result =
            match consume_stream(stream, emitter, &cancel).await {
                Ok(r) => {
                    // Successful stream consumption — reset retry counter
                    retry_count = 0;
                    r
                }
                Err(e) => {
                    // Retry transient SSE errors by restarting the round
                    if let Some(pe) = e.downcast_ref::<nexus_provider::error::ProviderError>() {
                        if pe.retryable && retry_count < crate::retry::MAX_RETRIES {
                            retry_count += 1;
                            let delay = crate::retry::backoff_delay(retry_count);
                            tracing::warn!(
                                attempt = retry_count,
                                delay_ms = delay,
                                error_kind = ?pe.kind,
                                "Retrying after stream error"
                            );
                            emitter.retry(retry_count, crate::retry::MAX_RETRIES, format!("{:?}", pe.kind), delay);
                            tokio::select! {
                                _ = tokio::time::sleep(std::time::Duration::from_millis(delay)) => {}
                                _ = cancel.cancelled() => {
                                    turn_error = Some(e.to_string());
                                    break;
                                }
                            }
                            continue;
                        }
                    }

                    let details = e
                        .downcast_ref::<nexus_provider::error::ProviderError>()
                        .and_then(|pe| serde_json::to_value(pe).ok());
                    emitter.run_error(e.to_string(), details.clone());
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
        timing_spans.push(TimingSpan {
            id: llm_span_id,
            name: "llm_call".into(),
            parent_id: Some(round_span_id.clone()),
            start_ms: inference_start_ms,
            end_ms: inference_start_ms + inference_duration,
            duration_ms: inference_duration,
            metadata: Some(serde_json::json!({
                "input_tokens": round_input_tokens,
                "output_tokens": round_output_tokens,
                "cache_read": round_cache_read,
                "cache_creation": round_cache_creation,
            })),
        });

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
        let round_cost = nexus_pricing::calculate_cost_with_cache(
            inference.model,
            round_input_tokens,
            round_cache_creation,
            round_cache_read,
            round_output_tokens,
        );
        turn_cost += round_cost;

        emitter.usage(
            cumulative_input,
            cumulative_output,
            cumulative_cache_read,
            cumulative_cache_creation,
            context_window,
            prior_cost + turn_cost,
        );

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
                let mut injected_blocks: Vec<String> = Vec::new();
                let tool_exec_start_ms = turn_start.elapsed().as_millis() as u64;
                let tool_exec_span_id = format!("t-toolexec-{}", round);
                let tool_exec_start = Instant::now();

                // Only SubAgentHandler needs per-round data (messages + cost)
                let sub_agent_handler = SubAgentHandler {
                    inference,
                    services,
                    parent_messages: &messages,
                    parent_tools: &tools,
                    cumulative_cost: prior_cost + turn_cost,
                };
                let mut handlers: Vec<&dyn tool_dispatch::ToolHandler> =
                    vec![&ask_handler, &task_handler, &fetch_handler, &fs_handler];
                handlers.push(&bash_handler);
                if depth == 0 {
                    handlers.push(&sub_agent_handler);
                }
                if let Some(ref bgh) = bg_handler {
                    handlers.push(bgh);
                }
                if let Some(ref cph) = control_plane_handler {
                    handlers.push(cph);
                }
                handlers.push(&resource_handler);
                handlers.push(&mcp_handler);

                for tc in &tool_calls {
                    let tool_start_ms = turn_start.elapsed().as_millis() as u64;
                    let tool_start = Instant::now();

                    // Extract the description field the model was required to fill out
                    let tool_description = serde_json::from_str::<serde_json::Value>(&tc.args_json)
                        .ok()
                        .and_then(|v| v.get("description").and_then(|d| d.as_str()).map(|s| s.to_string()));

                    if let Some(ref desc) = tool_description {
                        emitter.activity(desc);
                    }

                    // Parse tool input for hook events
                    let tool_input: serde_json::Value = serde_json::from_str(&tc.args_json)
                        .unwrap_or_else(|_| serde_json::json!({}));

                    // HOOK: PreToolUse — modules can deny or modify args.
                    let pre_decision = services.modules.fire_pre_tool_use(&PreToolUseEvent {
                        tool_name: &tc.name,
                        tool_input: &tool_input,
                        conversation_id,
                    }).await;

                    let effective_args = match pre_decision {
                        PreToolUseDecision::Allow => tc.args_json.clone(),
                        PreToolUseDecision::Deny(reason) => {
                            // Feed denial reason back as a tool error
                            let content = format!("Tool call denied: {}", reason);
                            emitter.tool_result(&tc.id, &content, true);
                            result_blocks.push(ContentBlock::ToolResult {
                                tool_use_id: tc.id.clone(),
                                content: fence_tool_result(&content),
                                is_error: Some(true),
                            });
                            continue;
                        }
                        PreToolUseDecision::ModifyArgs(new_args) => {
                            serde_json::to_string(&new_args).unwrap_or_else(|_| tc.args_json.clone())
                        }
                    };

                    let ctx = ToolContext {
                        tool_call_id: &tc.id,
                        tool_name: &tc.name,
                        args_json: &effective_args,
                        conversation_id,
                        emitter,
                        cancel: &cancel,
                    };
                    let mut result = tool_dispatch::dispatch_tool_call(&handlers, ctx).await;

                    // HOOK: PostToolUse / PostToolUseFailure
                    if result.is_error {
                        services.modules.fire_post_tool_use_failure(&PostToolUseFailureEvent {
                            tool_name: &tc.name,
                            tool_input: &tool_input,
                            error: &result.content,
                            conversation_id,
                        }).await;
                    } else {
                        services.modules.fire_post_tool_use(&mut PostToolUseEvent {
                            tool_name: &tc.name,
                            tool_call_id: &tc.id,
                            tool_input: &tool_input,
                            result: &mut result,
                            conversation_id,
                        }).await;
                    }

                    for msg in &result.injected_messages {
                        injected_blocks.push(msg.text.clone());
                    }
                    let content = result.content;
                    let is_error = result.is_error;

                    let tool_duration = tool_start.elapsed().as_millis() as u64;

                    emitter.tool_result(&tc.id, &content, is_error);

                    timing_spans.push(TimingSpan {
                        id: format!("t-tool-{}", tc.id),
                        name: format!("tool:{}", tc.name),
                        parent_id: Some(tool_exec_span_id.clone()),
                        start_ms: tool_start_ms,
                        end_ms: tool_start_ms + tool_duration,
                        duration_ms: tool_duration,
                        metadata: None,
                    });

                    result_blocks.push(ContentBlock::ToolResult {
                        tool_use_id: tc.id.clone(),
                        content: fence_tool_result(&content),
                        is_error: Some(is_error),
                    });
                }

                let tool_exec_duration = tool_exec_start.elapsed().as_millis() as u64;
                timing_spans.push(TimingSpan {
                    id: tool_exec_span_id,
                    name: "tool_execution".into(),
                    parent_id: Some(round_span_id.clone()),
                    start_ms: tool_exec_start_ms,
                    end_ms: tool_exec_start_ms + tool_exec_duration,
                    duration_ms: tool_exec_duration,
                    metadata: None,
                });

                let tool_results_msg = Message {
                    role: Role::User,
                    content: result_blocks,
                };
                messages.push(tool_results_msg.clone());
                new_messages.push(tool_results_msg);

                // HOOK: Inject ephemeral messages from modules (e.g. LSP diagnostics).
                // These are separate user messages so the model doesn't confuse
                // them with tool result content.
                if !injected_blocks.is_empty() {
                    let injected_msg = Message {
                        role: Role::User,
                        content: vec![ContentBlock::Text {
                            text: injected_blocks.join("\n"),
                        }],
                    };
                    messages.push(injected_msg);
                    // Not added to new_messages — injected context is ephemeral,
                    // not persisted to conversation history.
                }
            }
            _ => {
                // end_turn, max_tokens, or no tool calls

                // HOOK: Stop — modules can force continuation.
                if let Some(ref sr) = stop_reason {
                    let core_sr = crate::module::stop_reason_from_api(sr);
                    let stop_decision = services.modules.fire_stop(&StopEvent {
                        conversation_id,
                        round_count: round + 1,
                        stop_reason: &core_sr,
                    }).await;
                    if let StopDecision::Continue(reason) = stop_decision {
                        tracing::info!(reason = %reason, "Module requested continuation");
                        // Inject continuation reason as user message
                        messages.push(Message {
                            role: Role::User,
                            content: vec![ContentBlock::Text { text: reason }],
                        });
                        let round_duration = round_start.elapsed().as_millis() as u64;
                        timing_spans.push(TimingSpan {
                            id: round_span_id,
                            name: format!("round:{}", round + 1),
                            parent_id: Some(turn_span_id.clone()),
                            start_ms: round_start_ms,
                            end_ms: round_start_ms + round_duration,
                            duration_ms: round_duration,
                            metadata: None,
                        });
                        continue;
                    }
                }

                let round_duration = round_start.elapsed().as_millis() as u64;
                timing_spans.push(TimingSpan {
                    id: round_span_id,
                    name: format!("round:{}", round + 1),
                    parent_id: Some(turn_span_id.clone()),
                    start_ms: round_start_ms,
                    end_ms: round_start_ms + round_duration,
                    duration_ms: round_duration,
                    metadata: None,
                });
                break;
            }
        }

        let round_duration = round_start.elapsed().as_millis() as u64;
        timing_spans.push(TimingSpan {
            id: round_span_id,
            name: format!("round:{}", round + 1),
            parent_id: Some(turn_span_id.clone()),
            start_ms: round_start_ms,
            end_ms: round_start_ms + round_duration,
            duration_ms: round_duration,
            metadata: None,
        });
    }

    let turn_duration = turn_start.elapsed().as_millis() as u64;
    timing_spans.insert(0, TimingSpan {
        id: turn_span_id,
        name: "turn".into(),
        parent_id: None,
        start_ms: 0,
        end_ms: turn_duration,
        duration_ms: turn_duration,
        metadata: None,
    });

    emitter.timing(&timing_spans);

    // Turn-level cache summary
    let cache_savings_pct = if cumulative_input > 0 {
        (cumulative_cache_read as f64 / cumulative_input as f64 * 100.0) as u32
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

    let has_running_processes = match &services.process_manager {
        Some(pm) => pm.has_running(conversation_id).await,
        None => false,
    };
    emitter.run_finished(has_running_processes);

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

/// Consume the provider stream, emit AG-UI events, return accumulated content.
async fn consume_stream(
    mut stream: futures::stream::BoxStream<'static, Result<StreamEvent>>,
    emitter: &TurnEmitter,
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
                    emitter.text_start(&message_id);
                    current_text = Some((index, String::new()));
                }
                ContentBlockInfo::ToolUse { id, name } => {
                    emitter.tool_start(&id, &name);
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
                    emitter.thinking_start();
                    current_thinking = Some((index, String::new()));
                }
            },
            StreamEvent::ContentBlockDelta { index, delta } => match delta {
                Delta::TextDelta { text } => {
                    if let Some((idx, ref mut buf)) = current_text {
                        if idx == index {
                            buf.push_str(&text);
                            emitter.text_delta(&message_id, text);
                        }
                    }
                }
                Delta::InputJsonDelta { partial_json } => {
                    if let Some((idx, ref mut tc)) = current_tool {
                        if idx == index {
                            tc.args_json.push_str(&partial_json);
                            emitter.tool_args(&tc.id, partial_json);
                        }
                    }
                }
                Delta::ThinkingDelta { thinking } => {
                    if let Some((idx, ref mut buf)) = current_thinking {
                        if idx == index {
                            buf.push_str(&thinking);
                            emitter.thinking_delta(thinking);
                        }
                    }
                }
            },
            StreamEvent::ContentBlockStop { index } => {
                if let Some((idx, text)) = current_text.take() {
                    if idx == index {
                        emitter.text_end(&message_id);
                        content_blocks.push(ContentBlock::Text { text });
                    } else {
                        current_text = Some((idx, text));
                    }
                }
                if let Some((idx, tc)) = current_tool.take() {
                    if idx == index {
                        emitter.tool_end(&tc.id);
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
                        emitter.thinking_end();
                        content_blocks.push(ContentBlock::Thinking { thinking });
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
                let err = nexus_provider::error::ProviderError::from_anthropic_stream(
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

/// Inject a `<state_update>` user message into the API messages.
///
/// Insertion point: before the last message, UNLESS the last message
/// contains tool_result blocks. In that case, append AFTER it to
/// preserve the tool_use→tool_result pairing the Anthropic API requires.
fn inject_state_update(messages: &mut Vec<Message>, state: &str) {
    let state_msg = Message {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: state.to_string(),
        }],
    };
    let last_is_tool_result = messages.last().is_some_and(|m| {
        m.role == Role::User
            && m.content
                .iter()
                .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
    });
    let pos = if last_is_tool_result {
        messages.len()
    } else {
        messages.len().saturating_sub(1)
    };
    messages.insert(pos, state_msg);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_text(text: &str) -> Message {
        Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
        }
    }

    fn assistant_text(text: &str) -> Message {
        Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
        }
    }

    fn assistant_tool_use(id: &str, name: &str) -> Message {
        Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: id.to_string(),
                name: name.to_string(),
                input: serde_json::json!({}),
            }],
        }
    }

    fn user_tool_result(id: &str, content: &str) -> Message {
        Message {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: id.to_string(),
                content: content.to_string(),
                is_error: Some(false),
            }],
        }
    }

    // ── inject_state_update tests ──

    #[test]
    fn state_injected_before_user_prompt_on_round_0() {
        let mut msgs = vec![user_text("hello")];
        inject_state_update(&mut msgs, "<state/>");
        assert_eq!(msgs.len(), 2);
        // State should be BEFORE the user prompt
        assert!(matches!(&msgs[0].content[0], ContentBlock::Text { text } if text == "<state/>"));
        assert!(matches!(&msgs[1].content[0], ContentBlock::Text { text } if text == "hello"));
    }

    #[test]
    fn state_preserves_tool_use_tool_result_pairing() {
        // After round 0: [User(prompt), Assistant(tool_use), User(tool_result)]
        let mut msgs = vec![
            user_text("prompt"),
            assistant_tool_use("tc1", "bash"),
            user_tool_result("tc1", "output"),
        ];
        inject_state_update(&mut msgs, "<state/>");

        // State should be AFTER the tool_result (appended at end)
        assert_eq!(msgs.len(), 4);
        // tool_use at [1], tool_result at [2] — pairing preserved
        assert!(matches!(&msgs[1].content[0], ContentBlock::ToolUse { id, .. } if id == "tc1"));
        assert!(matches!(&msgs[2].content[0], ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "tc1"));
        // State at [3]
        assert!(matches!(&msgs[3].content[0], ContentBlock::Text { text } if text == "<state/>"));
    }

    #[test]
    fn state_before_last_text_in_follow_up_turn() {
        // Follow-up turn: [User(prompt), Assistant(tool_use), User(tool_result), Assistant(text), User(notification)]
        let mut msgs = vec![
            user_text("prompt"),
            assistant_tool_use("tc1", "bash"),
            user_tool_result("tc1", "output"),
            assistant_text("got it"),
            user_text("notification"),
        ];
        inject_state_update(&mut msgs, "<state/>");

        // State should be BEFORE the last user text (notification)
        assert_eq!(msgs.len(), 6);
        assert!(matches!(&msgs[4].content[0], ContentBlock::Text { text } if text == "<state/>"));
        assert!(matches!(&msgs[5].content[0], ContentBlock::Text { text } if text == "notification"));
        // tool_use/tool_result pairing still intact at [1]/[2]
        assert!(matches!(&msgs[1].content[0], ContentBlock::ToolUse { id, .. } if id == "tc1"));
        assert!(matches!(&msgs[2].content[0], ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "tc1"));
    }

    #[test]
    fn state_with_multiple_tool_rounds() {
        // Round 0 tools + round 1 tools: last message is tool_result
        let mut msgs = vec![
            user_text("prompt"),
            assistant_tool_use("tc1", "bash"),
            user_tool_result("tc1", "out1"),
            assistant_tool_use("tc2", "read"),
            user_tool_result("tc2", "out2"),
        ];
        inject_state_update(&mut msgs, "<state/>");

        assert_eq!(msgs.len(), 6);
        // Both tool pairings preserved
        assert!(matches!(&msgs[1].content[0], ContentBlock::ToolUse { id, .. } if id == "tc1"));
        assert!(matches!(&msgs[2].content[0], ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "tc1"));
        assert!(matches!(&msgs[3].content[0], ContentBlock::ToolUse { id, .. } if id == "tc2"));
        assert!(matches!(&msgs[4].content[0], ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "tc2"));
        // State appended at end
        assert!(matches!(&msgs[5].content[0], ContentBlock::Text { text } if text == "<state/>"));
    }

    #[test]
    fn state_on_empty_messages() {
        let mut msgs: Vec<Message> = vec![];
        inject_state_update(&mut msgs, "<state/>");
        assert_eq!(msgs.len(), 1);
        assert!(matches!(&msgs[0].content[0], ContentBlock::Text { text } if text == "<state/>"));
    }
}
