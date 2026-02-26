use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::agent;
use crate::agent::events::AgUiEvent;
use crate::agent::AgentTurnResult;
use crate::anthropic::types::{ContentBlock, Message, Role};
use crate::conversation::types::{
    ChatMessage, Conversation, ConversationUsage, MessagePart, MessageRole, Span,
};
use crate::server::AppState;
use crate::system_prompt::{SystemPromptBuilder, SystemPromptContext};

/// Spawn an agent turn as a background tokio task.
///
/// Resolves the active agent/provider, builds the system prompt, runs the
/// agent loop, persists results, and optionally generates a title.
pub fn spawn_agent_turn(
    state: Arc<AppState>,
    mut conv: Conversation,
    api_messages: Vec<Message>,
    tools: Vec<crate::anthropic::types::Tool>,
    conversation_id: String,
    cancel: tokio_util::sync::CancellationToken,
    assistant_message_id: Option<String>,
) {
    let agent_tx = state.chat.event_bridge.agent_tx();
    let state_clone = Arc::clone(&state);

    let needs_title = conv.title == "New Chat";
    let last_active_id = conv.active_path.last().cloned();

    tokio::spawn(async move {
        // Resolve active agent → provider
        let (provider_record, model, max_tokens, system_prompt, temperature, thinking_budget, agent_meta) = {
            let agents = state_clone.agents.agents.read().await;
            let providers = state_clone.agents.providers.read().await;

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
                            let _ = agent_tx.send(AgUiEvent::RunError {
                                thread_id: conversation_id.clone(),
                                run_id: String::new(),
                                message: format!(
                                    "Provider '{}' not found for agent '{}'",
                                    a.provider_id, a.name
                                ),
                                details: None,
                            });
                            return;
                        }
                    }
                }
                None => {
                    let _ = agent_tx.send(AgUiEvent::RunError {
                        thread_id: conversation_id.clone(),
                        run_id: String::new(),
                        message: "No active agent configured. Create one in Settings → Agents."
                            .to_string(),
                        details: None,
                    });
                    return;
                }
            }
        };

        let provider = match state_clone.agents.factory.get(&provider_record).await {
            Ok(p) => p,
            Err(e) => {
                let _ = agent_tx.send(AgUiEvent::RunError {
                    thread_id: conversation_id.clone(),
                    run_id: String::new(),
                    message: format!("Failed to create provider client: {}", e),
                    details: None,
                });
                return;
            }
        };

        // Assemble all tools (MCP + built-in task tools + ask_user + sub_agent)
        let mut tools = tools;
        tools.extend(crate::tasks::tools::definitions());
        tools.push(crate::ask_user::tool_definition());
        tools.push(crate::agent::sub_agent::tool_definition());
        if state_clone.config.fetch.enabled {
            tools.push(crate::fetch::tool_definition());
        }
        // Bash tool
        tools.push(crate::bash::tool_definition());
        // Background process tools
        tools.extend(crate::bg_process::tools::tool_definitions());
        // Use effective filesystem config (workspaces + base allowed_directories)
        let effective_fs = state_clone.effective_fs_config.read().await.clone();
        tools.extend(crate::filesystem::tool_definitions(&effective_fs));

        // Inject required "description" field into all tool schemas
        crate::anthropic::types::inject_tool_description_field(&mut tools);

        // Derive agent mode + full plan context from task state
        let (mode, mode_enum, plan_context) = {
            use crate::system_prompt::{PlanContext, PlanTaskSnapshot};
            use crate::tasks::types::AgentMode;

            let mut ts = state_clone.chat.task_store.write().await;
            match ts.get(&conversation_id) {
                Some(state) => {
                    let mode_enum = state.mode;
                    let mode = state.mode.to_string();
                    let plan_ctx = state.plan.as_ref().map(|plan| {
                        let tasks: Vec<PlanTaskSnapshot> = plan.task_ids.iter()
                            .filter_map(|id| state.tasks.get(id))
                            .map(|t| PlanTaskSnapshot {
                                id: t.id.clone(),
                                title: t.title.clone(),
                                description: t.description.clone(),
                                status: t.status.to_string(),
                                depends_on: t.depends_on.clone(),
                            })
                            .collect();
                        let current_id = tasks.iter()
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
        };

        // Apply composable tool filter chain
        let filter_ctx = crate::tool_filter::ToolFilterContext {
            mode: mode_enum,
            plan: None, // PlanSnapshot available for future filters
        };
        let tools = crate::tool_filter::ToolFilterChain::default_chain().apply(&filter_ctx, tools);
        tracing::debug!(
            mode = %mode,
            tool_count = tools.len(),
            "Tool filter applied"
        );

        // Build composable system prompt (split: static cached, dynamic injected)
        let context_window = crate::agent::context_window_for_model(&model);
        let prior_cost = conv.usage.as_ref().map(|u| u.total_cost).unwrap_or(0.0);
        let builder = SystemPromptBuilder::default_builder();
        let prompt_parts = builder.build_parts(&SystemPromptContext {
            conversation_title: conv.title.clone(),
            message_count: conv.active_path.len(),
            tool_names: tools.iter().map(|t| t.name.clone()).collect(),
            agent_name: agent_meta["agent_name"]
                .as_str()
                .unwrap_or("Assistant")
                .to_string(),
            custom_system_prompt: system_prompt,
            input_tokens: 0,
            context_window,
            mode,
            plan_context,
            working_directory: effective_fs.allowed_directories.first().cloned(),
            total_cost: prior_cost,
        });

        let mcp_guard = state_clone.mcp.mcp.read().await;

        // ── Context compaction ──
        let mut api_messages = api_messages;
        let estimated_tokens = crate::compaction::estimate_tokens(
            &api_messages,
            Some(prompt_parts.system.as_str()),
            &tools,
        );

        // Layer 1: Tool result pruning (mechanical, no LLM call)
        let prune_threshold = (context_window as f64 * crate::compaction::PRUNE_THRESHOLD_PCT) as u32;
        if estimated_tokens > prune_threshold {
            crate::compaction::prune_tool_results(&mut api_messages, 3);
        }

        // Layer 2: LLM summarization (last resort, or aggressive in execution mode)
        let effective_window = context_window.saturating_sub(20_000);
        let summarize_pct = if mode_enum == crate::tasks::types::AgentMode::Execution {
            0.4 // Aggressively compact in execution mode to reclaim planning context
        } else {
            crate::compaction::SUMMARIZE_THRESHOLD_PCT
        };
        let summarize_threshold = (effective_window as f64 * summarize_pct) as u32;
        if estimated_tokens > summarize_threshold {
            if let Some(ref title_client) = state_clone.title_client {
                match crate::compaction::summarize_messages(
                    title_client,
                    &conv.active_messages(),
                    10,
                )
                .await
                {
                    Ok((summary_text, consumed_ids)) => {
                        // Create spans: seal current, open new
                        if conv.spans.is_empty() {
                            // First compaction — bootstrap span chain
                            conv.spans.push(Span {
                                index: 0,
                                message_ids: consumed_ids.clone(),
                                summary: Some(summary_text),
                                sealed_at: Some(Utc::now()),
                            });
                            conv.spans.push(Span {
                                index: 1,
                                message_ids: Vec::new(),
                                summary: None,
                                sealed_at: None,
                            });
                        } else {
                            conv.seal_current_span(&consumed_ids, summary_text);
                            conv.open_new_span();
                        }

                        // Remove consumed IDs from active_path
                        conv.active_path.retain(|id| !consumed_ids.contains(id));

                        // Rebuild API messages (span summaries injected automatically)
                        api_messages = conv.build_api_messages();

                        // Save compacted conversation
                        {
                            let mut store = state_clone.chat.conversations.write().await;
                            if let Err(e) = store.save(&conv) {
                                tracing::error!("Failed to save compacted conversation: {}", e);
                            }
                        }

                        let _ = agent_tx.send(AgUiEvent::Custom {
                            thread_id: conversation_id.clone(),
                            name: "compaction".to_string(),
                            value: serde_json::json!({
                                "sealed_span_index": conv.spans.len() - 2,
                                "consumed_count": consumed_ids.len(),
                            }),
                        });
                    }
                    Err(e) => {
                        tracing::warn!("Compaction failed, continuing with full context: {}", e);
                    }
                }
            }
        }

        // Construct owned deps bundle for background sub-agent dispatch
        let bg_sub_agent_deps = Arc::new(agent::sub_agent::BgSubAgentDeps {
            provider: provider.clone(),
            chat: state_clone.chat.clone(),
            mcp: state_clone.mcp.clone(),
            fetch_config: state_clone.config.fetch.clone(),
            filesystem_config: effective_fs.clone(),
        });

        let result = agent::run_agent_turn(
            provider.as_ref(),
            &conversation_id,
            api_messages,
            tools,
            Some(prompt_parts.system),
            prompt_parts.state,
            &model,
            max_tokens,
            temperature,
            thinking_budget,
            &mcp_guard,
            &state_clone.config.fetch,
            &effective_fs,
            &state_clone.chat.task_store,
            &state_clone.chat.pending_questions,
            &agent_tx,
            cancel,
            0, // depth=0: parent turn, sub_agent tool available
            prior_cost,
            Some(state_clone.chat.process_manager.clone()),
            Some(bg_sub_agent_deps),
        )
        .await;
        drop(mcp_guard);

        match result {
            Ok(AgentTurnResult {
                messages: new_messages,
                timing_spans,
                input_tokens,
                output_tokens,
                cache_read_input_tokens,
                cache_creation_input_tokens,
                context_window,
                turn_cost,
                error: turn_error,
                ..
            }) => {
                if let Some(ref err_msg) = turn_error {
                    tracing::error!("Agent turn failed: {}", err_msg);
                }

                // Persist partial messages even on error — completed rounds
                // are valuable and prevent dangling tool_use on next turn.
                if !new_messages.is_empty() {
                    let mut chat_messages = api_messages_to_chat(
                        &new_messages,
                        last_active_id.as_deref(),
                        assistant_message_id.as_deref(),
                    );

                    for msg in chat_messages
                        .iter_mut()
                        .filter(|m| m.role == MessageRole::Assistant)
                    {
                        let meta =
                            msg.metadata.get_or_insert_with(|| serde_json::json!({}));
                        if let Some(obj) = meta.as_object_mut() {
                            obj.insert("agent".to_string(), agent_meta.clone());
                        }
                    }

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
                                    serde_json::to_value(&timing_spans)
                                        .unwrap_or_default(),
                                );
                            }
                        }
                    }

                    let new_ids: Vec<String> =
                        chat_messages.iter().map(|m| m.id.clone()).collect();

                    conv.messages.extend(chat_messages);
                    conv.active_path.extend(new_ids);
                    conv.updated_at = Utc::now();
                    conv.agent_id =
                        agent_meta["agent_id"].as_str().map(|s| s.to_string());
                    conv.usage = Some(ConversationUsage {
                        input_tokens,
                        output_tokens,
                        cache_read_input_tokens,
                        cache_creation_input_tokens,
                        context_window,
                        total_cost: prior_cost + turn_cost,
                    });

                    let mut store = state_clone.chat.conversations.write().await;
                    if let Err(e) = store.save(&conv) {
                        tracing::error!("Failed to save conversation: {}", e);
                    }
                    drop(store);
                }

                if turn_error.is_none() && needs_title {
                    let active = conv.active_messages();
                    crate::mechanics::auto_title::generate_title(
                        &state_clone,
                        &conversation_id,
                        &conv.title,
                        &active,
                    )
                    .await;
                }

                // Clear active_cancel so the queue watcher knows no turn is running
                {
                    let mut active = state_clone.chat.active_cancel.lock().await;
                    if active.as_ref().map(|(id, _)| id == &conversation_id).unwrap_or(false) {
                        *active = None;
                    }
                }

                // Drain any queued messages (bg process notifications, etc.)
                let queued = state_clone.chat.message_queue.drain(&conversation_id).await;
                if !queued.is_empty() {
                    drain_queue_and_follow_up(
                        state_clone.clone(),
                        conv,
                        conversation_id.clone(),
                        queued,
                    )
                    .await;
                }
            }
            Err(e) => {
                tracing::error!("Agent turn panicked: {}", e);
                // Clear active_cancel on error too
                {
                    let mut active = state_clone.chat.active_cancel.lock().await;
                    if active.as_ref().map(|(id, _)| id == &conversation_id).unwrap_or(false) {
                        *active = None;
                    }
                }
                let _ = agent_tx.send(AgUiEvent::RunError {
                    thread_id: conversation_id.clone(),
                    run_id: String::new(),
                    message: e.to_string(),
                    details: None,
                });
            }
        }
    });
}

// ── Private helpers ──

/// Drain queued messages into a conversation and spawn a follow-up agent turn.
///
/// Called after a turn ends (if messages arrived during the turn) or by the
/// queue watcher (if messages arrived while no turn was active).
pub async fn drain_queue_and_follow_up(
    state: Arc<AppState>,
    mut conv: Conversation,
    conversation_id: String,
    messages: Vec<super::message_queue::QueuedMessage>,
) {
    for msg in messages {
        let chat_msg = ChatMessage {
            id: Uuid::new_v4().to_string(),
            role: MessageRole::User,
            parts: vec![MessagePart::Text { text: msg.text }],
            timestamp: Utc::now(),
            parent_id: conv.active_path.last().cloned(),
            metadata: Some(msg.metadata),
        };
        conv.active_path.push(chat_msg.id.clone());
        conv.messages.push(chat_msg);
    }

    let mut store = state.chat.conversations.write().await;
    if let Err(e) = store.save(&conv) {
        tracing::error!("Failed to save queued messages: {}", e);
    }
    drop(store);

    let api_messages = conv.build_api_messages();
    let mcp_guard = state.mcp.mcp.read().await;
    let tools: Vec<crate::anthropic::types::Tool> = mcp_guard.tools();
    drop(mcp_guard);

    // Register the follow-up turn so the queue watcher skips this conversation
    let follow_up_cancel = tokio_util::sync::CancellationToken::new();
    {
        let mut active = state.chat.active_cancel.lock().await;
        *active = Some((conversation_id.clone(), follow_up_cancel.clone()));
    }

    spawn_agent_turn(
        state,
        conv,
        api_messages,
        tools,
        conversation_id,
        follow_up_cancel,
        None,
    );
}

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
                metadata: None,
            };
            parent_id = Some(chat_msg.id.clone());
            chat_msg
        })
        .collect()
}

