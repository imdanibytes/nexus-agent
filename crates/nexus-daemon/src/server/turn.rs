use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::agent;
use crate::agent::emitter::TurnEmitter;
use crate::agent::{AgentTurnResult, TimingSpan};
use crate::anthropic::types::{ContentBlock, Message, Role};
use crate::conversation::types::{
    ChatMessage, ConversationUsage, MessagePart, MessageRole, MessageSource, Span,
};
use crate::provider::InferenceProvider;
use crate::server::AppState;
use crate::system_prompt::{SystemPromptBuilder, SystemPromptContext};
use crate::tasks::types::AgentMode;

/// Everything needed to launch an agent turn. Assembled by the caller,
/// consumed by `spawn_agent_turn`.
pub struct TurnRequest {
    pub conversation_id: String,
    pub api_messages: Vec<Message>,
    pub tools: Vec<crate::anthropic::types::Tool>,
    pub cancel: tokio_util::sync::CancellationToken,
    pub run_id: String,
    pub assistant_message_id: Option<String>,
    pub last_active_id: Option<String>,
    pub prior_cost: f64,
    pub title: String,
    pub message_count: usize,
}

/// Resolved agent configuration from AppState.
struct ResolvedAgent {
    provider: Arc<dyn InferenceProvider>,
    model: String,
    max_tokens: u32,
    system_prompt: Option<String>,
    temperature: Option<f32>,
    thinking_budget: Option<u32>,
    meta: serde_json::Value,
}

/// Spawn an agent turn as a background tokio task.
///
/// Resolves the active agent/provider, builds the system prompt, runs the
/// agent loop, persists results, and optionally generates a title.
pub fn spawn_agent_turn(state: Arc<AppState>, req: TurnRequest) {
    let agent_tx = state.chat.event_bridge.agent_tx();
    let state_clone = Arc::clone(&state);

    let TurnRequest {
        conversation_id,
        api_messages,
        tools,
        cancel,
        run_id,
        assistant_message_id,
        last_active_id,
        prior_cost,
        title,
        message_count,
    } = req;

    tokio::spawn(async move {
        let setup_start = std::time::Instant::now();

        let emitter = TurnEmitter::new(
            agent_tx.clone(),
            conversation_id.clone(),
            run_id.clone(),
        );

        // 1. Resolve active agent → provider
        let resolved = match resolve_agent(&state_clone, &emitter).await {
            Some(r) => r,
            None => return,
        };

        // 2. Assemble tools (MCP + built-in + ask_user + sub_agent + fetch + bash + bg + fs)
        let mut tools = tools;
        tools.extend(crate::tasks::tools::definitions());
        tools.push(crate::ask_user::tool_definition());
        tools.push(crate::agent::sub_agent::tool_definition());
        if state_clone.config.fetch.enabled {
            tools.push(crate::fetch::tool_definition());
        }
        tools.push(crate::bash::tool_definition());
        tools.extend(crate::bg_process::tools::tool_definitions());
        let effective_fs = state_clone.effective_fs_config.read().await.clone();
        tools.extend(crate::filesystem::tool_definitions(&effective_fs));
        crate::anthropic::types::inject_tool_description_field(&mut tools);

        // 3. Derive agent mode + plan context from task state
        let (mode, mode_enum, plan_context) = resolve_task_mode(&state_clone, &conversation_id).await;

        // 4. Apply composable tool filter chain
        let filter_ctx = crate::tool_filter::ToolFilterContext {
            mode: mode_enum,
            plan: None,
        };
        let tools = crate::tool_filter::ToolFilterChain::default_chain().apply(&filter_ctx, tools);
        tracing::debug!(mode = %mode, tool_count = tools.len(), "Tool filter applied");

        // 5. Build system prompt
        let context_window = crate::agent::context_window_for_model(&resolved.model);
        let builder = SystemPromptBuilder::default_builder();
        let prompt_parts = builder.build_parts(&SystemPromptContext {
            conversation_title: title.clone(),
            message_count,
            tool_names: tools.iter().map(|t| t.name.clone()).collect(),
            agent_name: resolved.meta["agent_name"]
                .as_str()
                .unwrap_or("Assistant")
                .to_string(),
            custom_system_prompt: resolved.system_prompt.clone(),
            context_window,
            mode,
            plan_context,
            working_directory: effective_fs.allowed_directories.first().cloned(),
            total_cost: prior_cost,
        });

        let mcp_guard = state_clone.mcp.mcp.read().await;

        // 6. Context compaction
        let mut api_messages = api_messages;
        compact_context(
            &mut api_messages,
            &prompt_parts.system,
            &tools,
            context_window,
            mode_enum,
            &state_clone,
            &conversation_id,
            &emitter,
        )
        .await;

        // 7. Build InferenceConfig, TurnContext, TurnServices
        let bg_sub_agent_deps = Arc::new(agent::sub_agent::BgSubAgentDeps {
            provider: resolved.provider.clone(),
            chat: state_clone.chat.clone(),
            mcp: state_clone.mcp.clone(),
            fetch_config: state_clone.config.fetch.clone(),
            filesystem_config: effective_fs.clone(),
        });

        let setup_duration_ms = setup_start.elapsed().as_millis() as u64;

        let inference_cfg = agent::InferenceConfig {
            provider: resolved.provider.as_ref(),
            model: &resolved.model,
            max_tokens: resolved.max_tokens,
            temperature: resolved.temperature,
            thinking_budget: resolved.thinking_budget,
            system_prompt: Some(prompt_parts.system),
            state_update: prompt_parts.state,
        };

        let turn_ctx = agent::TurnContext {
            conversation_id: conversation_id.clone(),
            messages: api_messages,
            tools,
            prior_cost,
            depth: 0,
        };

        let turn_svc = agent::TurnServices {
            mcp: &mcp_guard,
            fetch_config: &state_clone.config.fetch,
            filesystem_config: &effective_fs,
            task_store: &state_clone.chat.task_store,
            pending_questions: &state_clone.chat.pending_questions,
            process_manager: Some(state_clone.chat.process_manager.clone()),
            bg_sub_agent_deps: Some(bg_sub_agent_deps),
        };

        // 8. Run agent loop
        let result = agent::run_agent_turn(
            &inference_cfg,
            turn_ctx,
            &turn_svc,
            &emitter,
            cancel,
        )
        .await;
        drop(mcp_guard);

        // 9–12. Handle results
        match result {
            Ok(AgentTurnResult {
                messages: new_messages,
                mut timing_spans,
                input_tokens,
                output_tokens,
                cache_read_input_tokens,
                cache_creation_input_tokens,
                context_window,
                turn_cost,
                error: turn_error,
                ..
            }) => {
                // 9. Adjust timing spans to include setup phase
                adjust_timing_spans(&mut timing_spans, setup_duration_ms);

                if let Some(ref err_msg) = turn_error {
                    tracing::error!("Agent turn failed: {}", err_msg);
                }

                // 10. Persist turn results
                if !new_messages.is_empty() {
                    let usage = ConversationUsage {
                        input_tokens,
                        output_tokens,
                        cache_read_input_tokens,
                        cache_creation_input_tokens,
                        context_window,
                        total_cost: prior_cost + turn_cost,
                    };
                    persist_turn_results(
                        &state_clone,
                        &conversation_id,
                        &new_messages,
                        last_active_id.as_deref(),
                        assistant_message_id.as_deref(),
                        &resolved.meta,
                        &timing_spans,
                        usage,
                    )
                    .await;
                }

                // 11. Auto-title
                if turn_error.is_none() {
                    let title_conv = state_clone.threads.get(&conversation_id).await.ok().flatten();
                    if let Some(title_conv) = title_conv {
                        let active = title_conv.active_messages();
                        crate::mechanics::auto_title::generate_title(
                            &state_clone,
                            &conversation_id,
                            &title_conv.title,
                            &active,
                        )
                        .await;
                    }
                }

                // 12. Cleanup + follow-up
                let queued = {
                    let mut active = state_clone.chat.active_turns.lock().await;
                    let is_mine = active
                        .get(&conversation_id)
                        .map(|t| t.run_id == run_id)
                        .unwrap_or(false);
                    if is_mine {
                        active.remove(&conversation_id);
                        state_clone.chat.message_queue.drain(&conversation_id).await
                    } else {
                        vec![]
                    }
                };

                if !queued.is_empty() {
                    drain_queue_and_follow_up(
                        state_clone.clone(),
                        conversation_id.clone(),
                        queued,
                    )
                    .await;
                }
            }
            Err(e) => {
                tracing::error!("Agent turn panicked: {}", e);
                {
                    let mut active = state_clone.chat.active_turns.lock().await;
                    if active.get(&conversation_id).map(|t| t.run_id == run_id).unwrap_or(false) {
                        active.remove(&conversation_id);
                    }
                }
                emitter.run_error(e.to_string(), None);
            }
        }
    });
}

// ── Extracted helpers ──

/// Resolve the active agent from AppState, returning provider + config.
async fn resolve_agent(state: &AppState, emitter: &TurnEmitter) -> Option<ResolvedAgent> {
    let (provider_record, model, max_tokens, system_prompt, temperature, thinking_budget, meta) = {
        let agents = state.agents.agents.read().await;
        let providers = state.agents.providers.read().await;

        let active_id = agents.active_agent_id().map(|s| s.to_string());
        let agent = active_id.as_deref().and_then(|id| agents.get(id));

        match agent {
            Some(a) => {
                let provider = providers.get(&a.provider_id).cloned();
                match provider {
                    Some(p) => (
                        p,
                        a.model.clone(),
                        a.max_tokens.unwrap_or(8192),
                        a.system_prompt.clone(),
                        a.temperature,
                        a.thinking_budget,
                        serde_json::json!({
                            "agent_id": a.id,
                            "agent_name": a.name,
                            "model": a.model,
                        }),
                    ),
                    None => {
                        emitter.run_error(
                            format!("Provider '{}' not found for agent '{}'", a.provider_id, a.name),
                            None,
                        );
                        return None;
                    }
                }
            }
            None => {
                emitter.run_error(
                    "No active agent configured. Create one in Settings → Agents.",
                    None,
                );
                return None;
            }
        }
    };

    let provider = match state.agents.factory.get(&provider_record).await {
        Ok(p) => p,
        Err(e) => {
            emitter.run_error(format!("Failed to create provider client: {}", e), None);
            return None;
        }
    };

    Some(ResolvedAgent {
        provider,
        model,
        max_tokens,
        system_prompt,
        temperature,
        thinking_budget,
        meta,
    })
}

/// Derive agent mode and plan context from the task store.
async fn resolve_task_mode(
    state: &AppState,
    conversation_id: &str,
) -> (String, AgentMode, Option<crate::system_prompt::PlanContext>) {
    use crate::system_prompt::{PlanContext, PlanTaskSnapshot};

    let mut ts = state.chat.task_store.write().await;
    match ts.get(conversation_id) {
        Some(task_state) => {
            let mode_enum = task_state.mode;
            let mode = task_state.mode.to_string();
            let plan_ctx = task_state.plan.as_ref().map(|plan| {
                let tasks: Vec<PlanTaskSnapshot> = plan
                    .task_ids
                    .iter()
                    .filter_map(|id| task_state.tasks.get(id))
                    .map(|t| PlanTaskSnapshot {
                        id: t.id.clone(),
                        title: t.title.clone(),
                        description: t.description.clone(),
                        status: t.status.to_string(),
                        depends_on: t.depends_on.clone(),
                    })
                    .collect();
                let current_id = tasks
                    .iter()
                    .find(|t| t.status == "in_progress")
                    .or_else(|| tasks.iter().find(|t| t.status == "pending"))
                    .map(|t| t.id.clone());
                PlanContext {
                    plan_title: plan.title.clone(),
                    plan_summary: plan.summary.clone(),
                    tasks,
                    current_task_id: current_id,
                    mode: mode.clone(),
                }
            });
            (mode, mode_enum, plan_ctx)
        }
        None => ("general".to_string(), AgentMode::General, None),
    }
}

/// Compact context if approaching the context window limit.
///
/// Layer 1: Mechanical tool result pruning (no LLM call).
/// Layer 2: LLM summarization with span tracking.
async fn compact_context(
    api_messages: &mut Vec<Message>,
    system_prompt: &str,
    tools: &[crate::anthropic::types::Tool],
    context_window: u32,
    mode_enum: AgentMode,
    state: &AppState,
    conversation_id: &str,
    emitter: &TurnEmitter,
) {
    let estimated_tokens =
        crate::compaction::estimate_tokens(api_messages, Some(system_prompt), tools);

    // Layer 1: Tool result pruning
    let prune_threshold =
        (context_window as f64 * crate::compaction::PRUNE_THRESHOLD_PCT) as u32;
    if estimated_tokens > prune_threshold {
        crate::compaction::prune_tool_results(api_messages, 3);
    }

    // Layer 2: LLM summarization
    let effective_window = context_window.saturating_sub(20_000);
    let summarize_pct = if mode_enum == AgentMode::Execution {
        0.4
    } else {
        crate::compaction::SUMMARIZE_THRESHOLD_PCT
    };
    let summarize_threshold = (effective_window as f64 * summarize_pct) as u32;

    if estimated_tokens <= summarize_threshold {
        return;
    }

    let title_client = match state.title_client {
        Some(ref c) => c,
        None => return,
    };

    let compact_conv = state.threads.get(conversation_id).await.ok().flatten();

    let mut compact_conv = match compact_conv {
        Some(c) => c,
        None => return,
    };

    match crate::compaction::summarize_messages(
        title_client,
        &compact_conv.active_messages(),
        10,
    )
    .await
    {
        Ok((summary_text, consumed_ids)) => {
            if compact_conv.spans.is_empty() {
                compact_conv.spans.push(Span {
                    index: 0,
                    message_ids: consumed_ids.clone(),
                    summary: Some(summary_text),
                    sealed_at: Some(Utc::now()),
                });
                compact_conv.spans.push(Span {
                    index: 1,
                    message_ids: Vec::new(),
                    summary: None,
                    sealed_at: None,
                });
            } else {
                compact_conv.seal_current_span(&consumed_ids, summary_text);
                compact_conv.open_new_span();
            }

            compact_conv
                .active_path
                .retain(|id| !consumed_ids.contains(id));
            *api_messages = compact_conv.build_api_messages();

            let sealed_span_count = compact_conv.spans.len();

            if let Err(e) = state.threads.commit(compact_conv).await {
                tracing::error!("Failed to save compacted conversation: {}", e);
            }

            emitter.compaction(sealed_span_count - 2, consumed_ids.len());
        }
        Err(e) => {
            tracing::warn!(
                "Compaction failed, continuing with full context: {}",
                e
            );
        }
    }
}

/// Inject setup span and offset agent timing spans by setup duration.
fn adjust_timing_spans(timing_spans: &mut Vec<TimingSpan>, setup_duration_ms: u64) {
    if setup_duration_ms == 0 {
        return;
    }

    let insert_idx = if !timing_spans.is_empty() { 1 } else { 0 };
    timing_spans.insert(
        insert_idx,
        TimingSpan {
            id: "t-setup".into(),
            name: "setup".into(),
            parent_id: Some("t-turn".into()),
            start_ms: 0,
            end_ms: setup_duration_ms,
            duration_ms: setup_duration_ms,
            metadata: None,
        },
    );

    // Offset all non-turn/non-setup spans by setup duration
    for span in timing_spans.iter_mut() {
        if span.id == "t-turn" || span.id == "t-setup" {
            continue;
        }
        span.start_ms += setup_duration_ms;
        span.end_ms += setup_duration_ms;
    }

    // Update turn span endMs to include setup
    if let Some(turn_span) = timing_spans.first_mut() {
        turn_span.end_ms += setup_duration_ms;
        turn_span.duration_ms += setup_duration_ms;
    }
}

/// Convert API messages to ChatMessages, inject metadata, and save to store.
async fn persist_turn_results(
    state: &AppState,
    conversation_id: &str,
    new_messages: &[Message],
    last_active_id: Option<&str>,
    assistant_message_id: Option<&str>,
    agent_meta: &serde_json::Value,
    timing_spans: &[TimingSpan],
    usage: ConversationUsage,
) {
    let mut chat_messages =
        api_messages_to_chat(new_messages, last_active_id, assistant_message_id);

    // Tag every message with its source
    let agent_source = MessageSource::Agent {
        agent_id: agent_meta["agent_id"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        agent_name: agent_meta["agent_name"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        model: agent_meta["model"].as_str().unwrap_or("").to_string(),
    };
    for msg in chat_messages.iter_mut() {
        if msg.source.is_none() {
            msg.source = Some(agent_source.clone());
        }
        // Keep metadata.agent for backward compat
        if msg.role == MessageRole::Assistant {
            let meta = msg
                .metadata
                .get_or_insert_with(|| serde_json::json!({}));
            if let Some(obj) = meta.as_object_mut() {
                obj.insert("agent".to_string(), agent_meta.clone());
            }
        }
    }

    // Inject timing spans into the last assistant message
    if !timing_spans.is_empty() {
        if let Some(last_assistant) = chat_messages
            .iter_mut()
            .rev()
            .find(|m| m.role == MessageRole::Assistant)
        {
            let meta = last_assistant
                .metadata
                .get_or_insert_with(|| serde_json::json!({}));
            if let Some(obj) = meta.as_object_mut() {
                obj.insert(
                    "timingSpans".to_string(),
                    serde_json::to_value(timing_spans).unwrap_or_default(),
                );
            }
        }
    }

    let new_ids: Vec<String> = chat_messages.iter().map(|m| m.id.clone()).collect();

    // Reload fresh conversation — no stale in-memory copy.
    let save_result = match state.threads.checkout(conversation_id).await {
        Ok(Some(mut fresh_conv)) => {
            fresh_conv.messages.extend(chat_messages);
            fresh_conv.active_path.extend(new_ids);
            fresh_conv.updated_at = Utc::now();
            fresh_conv.agent_id = agent_meta["agent_id"]
                .as_str()
                .map(|s| s.to_string());
            fresh_conv.usage = Some(usage);
            state.threads.commit(fresh_conv).await
        }
        Ok(None) => {
            tracing::warn!(
                "Conversation {} deleted during turn, discarding results",
                conversation_id
            );
            Ok(())
        }
        Err(e) => {
            tracing::error!("Failed to reload conversation: {}", e);
            Err(e)
        }
    };
    if let Err(e) = save_result {
        tracing::error!("Failed to save conversation: {}", e);
    }
}

// ── Queue + follow-up ──

/// Drain queued messages into a conversation and spawn a follow-up agent turn.
///
/// Called after a turn ends (if messages arrived during the turn) or by the
/// queue watcher (if messages arrived while no turn was active).
///
/// Reloads the conversation from the store to avoid stale data.
pub async fn drain_queue_and_follow_up(
    state: Arc<AppState>,
    conversation_id: String,
    messages: Vec<super::message_queue::QueuedMessage>,
) {
    // Reload fresh conversation from store
    let mut conv = match state.threads.checkout(&conversation_id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            tracing::warn!(conversation_id = %conversation_id, "drain_queue_and_follow_up: conversation not found");
            return;
        }
        Err(e) => {
            tracing::error!(conversation_id = %conversation_id, "drain_queue_and_follow_up: failed to load: {}", e);
            return;
        }
    };

    for msg in messages {
        let meta = match msg.metadata {
            serde_json::Value::Null => None,
            other => Some(other),
        };

        let chat_msg = ChatMessage {
            id: Uuid::new_v4().to_string(),
            role: MessageRole::User,
            parts: vec![MessagePart::Text { text: msg.text }],
            timestamp: Utc::now(),
            parent_id: conv.active_path.last().cloned(),
            source: Some(MessageSource::Mcp),
            metadata: meta,
        };
        conv.active_path.push(chat_msg.id.clone());
        conv.messages.push(chat_msg);
    }

    let api_messages = conv.build_api_messages();
    let last_active_id = conv.active_path.last().cloned();
    let prior_cost = conv.usage.as_ref().map(|u| u.total_cost).unwrap_or(0.0);
    let title = conv.title.clone();
    let message_count = conv.active_path.len();

    if let Err(e) = state.threads.commit(conv).await {
        tracing::error!("Failed to save queued messages: {}", e);
    }

    let mcp_guard = state.mcp.mcp.read().await;
    let tools: Vec<crate::anthropic::types::Tool> = mcp_guard.tools();
    drop(mcp_guard);

    // Register the follow-up turn so the queue watcher skips this conversation
    let follow_up_run_id = Uuid::new_v4().to_string();
    let follow_up_cancel = tokio_util::sync::CancellationToken::new();
    {
        let mut active = state.chat.active_turns.lock().await;
        active.insert(
            conversation_id.clone(),
            crate::server::services::ActiveTurn {
                run_id: follow_up_run_id.clone(),
                cancel: follow_up_cancel.clone(),
            },
        );
    }

    spawn_agent_turn(
        state,
        TurnRequest {
            conversation_id,
            api_messages,
            tools,
            cancel: follow_up_cancel,
            run_id: follow_up_run_id,
            assistant_message_id: None,
            last_active_id,
            prior_cost,
            title,
            message_count,
        },
    );
}

// ── Message conversion ──

/// Strip `<tool_response>` fencing from tool results.
fn unfence_tool_result(content: &str) -> String {
    let trimmed = content.trim();
    if let Some(rest) = trimmed.strip_prefix("<tool_response>") {
        if let Some(inner) = rest.split("</tool_response>").next() {
            return inner.trim().to_string();
        }
    }
    content.to_string()
}

/// Convert API Messages back to ChatMessages with parent_id chaining.
///
/// Stores messages in API-native format: assistant messages have ToolCall parts
/// (without results), user messages have ToolResult parts. No merging.
fn api_messages_to_chat(
    messages: &[Message],
    initial_parent_id: Option<&str>,
    assistant_message_id: Option<&str>,
) -> Vec<ChatMessage> {
    let mut parent_id = initial_parent_id.map(|s| s.to_string());
    let mut used_assistant_id = false;

    messages
        .iter()
        .map(|msg| {
            let role = match msg.role {
                Role::User => MessageRole::User,
                Role::Assistant => MessageRole::Assistant,
            };

            let parts: Vec<MessagePart> = msg
                .content
                .iter()
                .map(|block| match block {
                    ContentBlock::Text { text } => {
                        MessagePart::Text { text: text.clone() }
                    }
                    ContentBlock::ToolUse { id, name, input } => MessagePart::ToolCall {
                        tool_call_id: id.clone(),
                        tool_name: name.clone(),
                        args: input.clone(),
                        result: None,
                        is_error: false,
                    },
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => MessagePart::ToolResult {
                        tool_call_id: tool_use_id.clone(),
                        result: unfence_tool_result(content),
                        is_error: is_error.unwrap_or(false),
                    },
                    ContentBlock::Thinking { thinking } => MessagePart::Thinking {
                        thinking: thinking.clone(),
                    },
                })
                .collect();

            // Use client-provided ID for the first assistant message
            let id = if role == MessageRole::Assistant && !used_assistant_id {
                used_assistant_id = true;
                assistant_message_id
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| Uuid::new_v4().to_string())
            } else {
                Uuid::new_v4().to_string()
            };

            let chat_msg = ChatMessage {
                id,
                role,
                parts,
                timestamp: Utc::now(),
                parent_id: parent_id.clone(),
                source: None, // Set by caller after conversion
                metadata: None,
            };
            parent_id = Some(chat_msg.id.clone());
            chat_msg
        })
        .collect()
}
