use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::agent;
use crate::agent::events::AgUiEvent;
use crate::agent::AgentTurnResult;
use crate::anthropic::types::{ContentBlock, Message, Role};
use crate::conversation::types::{
    ChatMessage, Conversation, ConversationUsage, MessagePart, MessageRole,
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
        let (provider_record, model, max_tokens, system_prompt, temperature, agent_meta) = {
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

        // Assemble all tools (MCP + built-in task tools + ask_user)
        let mut tools = tools;
        tools.extend(crate::tasks::tools::definitions());
        tools.push(crate::ask_user::tool_definition());

        // Derive agent mode + current task info from task state
        let (mode, mode_enum, current_task) = {
            use crate::system_prompt::CurrentTaskInfo;
            use crate::tasks::types::{AgentMode, TaskStatus};

            let ts = state_clone.chat.task_store.read().await;
            match ts.get(&conversation_id) {
                Some(state) => {
                    let mode_enum = state.mode;
                    let mode = state.mode.to_string();
                    let task_info = state.plan.as_ref().and_then(|plan| {
                        // Find the first in-progress task, or the first pending one
                        let current = plan.task_ids.iter()
                            .filter_map(|id| state.tasks.get(id))
                            .find(|t| matches!(t.status, TaskStatus::InProgress))
                            .or_else(|| {
                                plan.task_ids.iter()
                                    .filter_map(|id| state.tasks.get(id))
                                    .find(|t| matches!(t.status, TaskStatus::Pending))
                            })?;
                        let completed = state.tasks.values()
                            .filter(|t| matches!(t.status, TaskStatus::Completed))
                            .count();
                        Some(CurrentTaskInfo {
                            plan_title: plan.title.clone(),
                            task_id: current.id.clone(),
                            task_title: current.title.clone(),
                            task_description: current.description.clone(),
                            completed_count: completed,
                            total_count: state.tasks.len(),
                        })
                    });
                    (mode, mode_enum, task_info)
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

        // Build composable system prompt
        let context_window = crate::agent::context_window_for_model(&model);
        let builder = SystemPromptBuilder::default_builder();
        let full_system_prompt = builder.build(&SystemPromptContext {
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
            current_task,
        });

        let mcp_guard = state_clone.mcp.mcp.read().await;
        let result = agent::run_agent_turn(
            provider.as_ref(),
            &conversation_id,
            api_messages,
            tools,
            Some(full_system_prompt),
            &model,
            max_tokens,
            temperature,
            &mcp_guard,
            &state_clone.chat.task_store,
            &state_clone.chat.pending_questions,
            &agent_tx,
            cancel,
        )
        .await;
        drop(mcp_guard);

        match result {
            Ok(AgentTurnResult {
                messages: new_messages,
                timing_spans,
                input_tokens,
                output_tokens,
                context_window,
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
                        context_window,
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
            }
            Err(e) => {
                tracing::error!("Agent turn panicked: {}", e);
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

/// Strip `<tool_response>` fencing from tool results.
/// Fencing is added for the Anthropic API but should not be stored.
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

