use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::agent;
use crate::agent::events::AgUiEvent;
use crate::agent::AgentTurnResult;
use crate::anthropic::types::{ContentBlock, Message, MessagesRequest, Role};
use crate::conversation::types::{
    ChatMessage, Conversation, ConversationUsage, MessagePart, MessageRole,
};
use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    #[serde(rename = "conversationId")]
    pub conversation_id: String,
    pub message: String,
    /// Client-generated Snowflake ID for the user message
    #[serde(rename = "userMessageId")]
    pub user_message_id: Option<String>,
    /// Client-generated Snowflake ID for the assistant response
    #[serde(rename = "assistantMessageId")]
    pub assistant_message_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BranchRequest {
    #[serde(rename = "conversationId")]
    pub conversation_id: String,
    /// The user message being edited (we create a sibling with the same parent)
    #[serde(rename = "messageId")]
    pub message_id: String,
    pub message: String,
    /// Client-generated Snowflake ID for the new user message
    #[serde(rename = "userMessageId")]
    pub user_message_id: Option<String>,
    /// Client-generated Snowflake ID for the assistant response
    #[serde(rename = "assistantMessageId")]
    pub assistant_message_id: Option<String>,
}

pub async fn start_turn(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ChatRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let conversation_id = body.conversation_id.clone();

    let cancel = cancel_existing_turn(&state, &conversation_id).await;

    let (conv, api_messages, tools, user_msg_id) = {
        let mut store = state.conversations.write().await;

        let mut conv = store
            .get(&conversation_id)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

        // Parent is the last message in the active path
        let parent_id = conv.active_path.last().cloned();

        let user_msg = ChatMessage {
            id: body.user_message_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string()),
            role: MessageRole::User,
            parts: vec![MessagePart::Text {
                text: body.message.clone(),
            }],
            timestamp: Utc::now(),
            parent_id,
            metadata: None,
        };

        let user_msg_id = user_msg.id.clone();
        conv.active_path.push(user_msg.id.clone());
        conv.messages.push(user_msg);
        conv.updated_at = Utc::now();

        store
            .save(&conv)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let api_messages = conv_to_api_messages(&conv.active_messages());

        // Filter MCP tools based on active agent's mcp_server_ids
        let tools = {
            let mcp = state.mcp.read().await;
            let agents = state.agents.read().await;
            let active_id = agents.active_agent_id().map(|s| s.to_string());
            let agent = active_id.as_deref().and_then(|id| agents.get(id));
            match agent.and_then(|a| a.mcp_server_ids.as_ref()) {
                Some(ids) => mcp.tools_for(Some(ids)),
                None => mcp.tools(),
            }
        };

        (conv, api_messages, tools, user_msg_id)
    };

    spawn_agent_turn(state, conv, api_messages, tools, conversation_id, cancel, body.assistant_message_id);

    Ok(Json(
        serde_json::json!({
            "ok": true,
            "conversationId": body.conversation_id,
            "messageId": user_msg_id,
        }),
    ))
}

pub async fn branch_turn(
    State(state): State<Arc<AppState>>,
    Json(body): Json<BranchRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let conversation_id = body.conversation_id.clone();

    let cancel = cancel_existing_turn(&state, &conversation_id).await;

    let (conv, api_messages, tools, new_msg_id) = {
        let mut store = state.conversations.write().await;

        let mut conv = store
            .get(&conversation_id)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

        // Find the original message being edited
        let original_msg = conv
            .messages
            .iter()
            .find(|m| m.id == body.message_id)
            .ok_or(StatusCode::BAD_REQUEST)?;

        if original_msg.role != MessageRole::User {
            return Err(StatusCode::BAD_REQUEST);
        }

        // New message is a sibling — same parent as the original
        let parent_id = original_msg.parent_id.clone();

        let new_user_msg = ChatMessage {
            id: body.user_message_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string()),
            role: MessageRole::User,
            parts: vec![MessagePart::Text {
                text: body.message.clone(),
            }],
            timestamp: Utc::now(),
            parent_id: parent_id.clone(),
            metadata: None,
        };

        let new_msg_id = new_user_msg.id.clone();

        // Recompute active path: path to the parent + new message
        let mut new_path = match &parent_id {
            Some(pid) => conv.path_to(pid),
            None => Vec::new(),
        };
        new_path.push(new_user_msg.id.clone());

        conv.active_path = new_path;
        conv.messages.push(new_user_msg);
        conv.updated_at = Utc::now();

        store
            .save(&conv)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let api_messages = conv_to_api_messages(&conv.active_messages());

        // Filter MCP tools based on active agent's mcp_server_ids
        let tools = {
            let mcp = state.mcp.read().await;
            let agents = state.agents.read().await;
            let active_id = agents.active_agent_id().map(|s| s.to_string());
            let agent = active_id.as_deref().and_then(|id| agents.get(id));
            match agent.and_then(|a| a.mcp_server_ids.as_ref()) {
                Some(ids) => mcp.tools_for(Some(ids)),
                None => mcp.tools(),
            }
        };

        (conv, api_messages, tools, new_msg_id)
    };

    spawn_agent_turn(
        state,
        conv,
        api_messages,
        tools,
        conversation_id.clone(),
        cancel,
        body.assistant_message_id,
    );

    Ok(Json(
        serde_json::json!({ "ok": true, "conversationId": conversation_id, "messageId": new_msg_id }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct RegenerateRequest {
    #[serde(rename = "conversationId")]
    pub conversation_id: String,
    /// The user message to regenerate a response for
    #[serde(rename = "messageId")]
    pub message_id: String,
    /// Client-generated Snowflake ID for the new assistant response
    #[serde(rename = "assistantMessageId")]
    pub assistant_message_id: Option<String>,
}

pub async fn regenerate_turn(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RegenerateRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let conversation_id = body.conversation_id.clone();

    let cancel = cancel_existing_turn(&state, &conversation_id).await;

    let (conv, api_messages, tools) = {
        let mut store = state.conversations.write().await;

        let mut conv = store
            .get(&conversation_id)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

        let msg = conv
            .messages
            .iter()
            .find(|m| m.id == body.message_id)
            .ok_or(StatusCode::BAD_REQUEST)?;

        if msg.role != MessageRole::User {
            return Err(StatusCode::BAD_REQUEST);
        }

        // Active path ends at this user message (strip any existing assistant response)
        let user_path = conv.path_to_only(&body.message_id);
        conv.active_path = user_path;
        conv.updated_at = Utc::now();

        store
            .save(&conv)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let api_messages = conv_to_api_messages(&conv.active_messages());

        let tools = {
            let mcp = state.mcp.read().await;
            let agents = state.agents.read().await;
            let active_id = agents.active_agent_id().map(|s| s.to_string());
            let agent = active_id.as_deref().and_then(|id| agents.get(id));
            match agent.and_then(|a| a.mcp_server_ids.as_ref()) {
                Some(ids) => mcp.tools_for(Some(ids)),
                None => mcp.tools(),
            }
        };

        (conv, api_messages, tools)
    };

    spawn_agent_turn(
        state,
        conv,
        api_messages,
        tools,
        conversation_id.clone(),
        cancel,
        body.assistant_message_id,
    );

    Ok(Json(
        serde_json::json!({ "ok": true, "conversationId": conversation_id }),
    ))
}

pub async fn abort_turn(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AbortRequest>,
) -> StatusCode {
    let mut active = state.active_cancel.lock().await;
    if let Some((ref cid, ref token)) = *active {
        if cid == &body.conversation_id {
            token.cancel();
            *active = None;
        }
    }
    StatusCode::OK
}

#[derive(Debug, Deserialize)]
pub struct AbortRequest {
    #[serde(rename = "conversationId")]
    pub conversation_id: String,
}

// ── Helpers ──

async fn cancel_existing_turn(
    state: &Arc<AppState>,
    conversation_id: &str,
) -> tokio_util::sync::CancellationToken {
    let mut active = state.active_cancel.lock().await;
    if let Some((ref cid, ref token)) = *active {
        if cid == conversation_id {
            token.cancel();
        }
    }
    let cancel = tokio_util::sync::CancellationToken::new();
    *active = Some((conversation_id.to_string(), cancel.clone()));
    cancel
}

fn spawn_agent_turn(
    state: Arc<AppState>,
    mut conv: Conversation,
    api_messages: Vec<Message>,
    tools: Vec<crate::anthropic::types::Tool>,
    conversation_id: String,
    cancel: tokio_util::sync::CancellationToken,
    assistant_message_id: Option<String>,
) {
    let agent_tx = state.event_bridge.agent_tx();
    let state_clone = Arc::clone(&state);

    let needs_title = conv.title == "New Chat";
    let last_active_id = conv.active_path.last().cloned();

    tokio::spawn(async move {
        // Resolve active agent → provider
        let (provider_record, model, max_tokens, system_prompt, temperature, agent_meta) = {
            let agents = state_clone.agents.read().await;
            let providers = state_clone.providers.read().await;

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
                    });
                    return;
                }
            }
        };

        let provider = match state_clone.factory.get(&provider_record).await {
            Ok(p) => p,
            Err(e) => {
                let _ = agent_tx.send(AgUiEvent::RunError {
                    thread_id: conversation_id.clone(),
                    run_id: String::new(),
                    message: format!("Failed to create provider client: {}", e),
                });
                return;
            }
        };

        let mcp_guard = state_clone.mcp.read().await;
        let result = agent::run_agent_turn(
            provider.as_ref(),
            &conversation_id,
            api_messages,
            tools,
            system_prompt,
            &model,
            max_tokens,
            temperature,
            &mcp_guard,
            &agent_tx,
            cancel,
        )
        .await;
        drop(mcp_guard);

        match result {
            Ok(AgentTurnResult { messages: new_messages, timing_spans, input_tokens, output_tokens, context_window }) => {
                let mut chat_messages =
                    api_messages_to_chat(&new_messages, last_active_id.as_deref(), assistant_message_id.as_deref());

                // Stamp every assistant message with agent info so it's
                // self-contained even if the agent is later deleted.
                for msg in chat_messages.iter_mut().filter(|m| m.role == MessageRole::Assistant) {
                    let meta = msg.metadata.get_or_insert_with(|| serde_json::json!({}));
                    if let Some(obj) = meta.as_object_mut() {
                        obj.insert("agent".to_string(), agent_meta.clone());
                    }
                }

                // Attach timing spans to the last assistant message's metadata
                if !timing_spans.is_empty() {
                    if let Some(last_assistant) = chat_messages.iter_mut().rev().find(|m| m.role == MessageRole::Assistant) {
                        let meta = last_assistant.metadata.get_or_insert_with(|| serde_json::json!({}));
                        if let Some(obj) = meta.as_object_mut() {
                            obj.insert("timingSpans".to_string(), serde_json::to_value(&timing_spans).unwrap_or_default());
                        }
                    }
                }

                let new_ids: Vec<String> =
                    chat_messages.iter().map(|m| m.id.clone()).collect();

                conv.messages.extend(chat_messages);
                conv.active_path.extend(new_ids);
                conv.updated_at = Utc::now();
                conv.agent_id = agent_meta["agent_id"].as_str().map(|s| s.to_string());
                conv.usage = Some(ConversationUsage {
                    input_tokens,
                    output_tokens,
                    context_window,
                });

                let mut store = state_clone.conversations.write().await;
                if let Err(e) = store.save(&conv) {
                    tracing::error!("Failed to save conversation: {}", e);
                }
                drop(store);

                // Notify UI that the conversation is persisted (IDs, branch info ready)
                let _ = agent_tx.send(AgUiEvent::Custom {
                    thread_id: conversation_id.clone(),
                    name: "conversation_updated".to_string(),
                    value: serde_json::json!({}),
                });

                if needs_title {
                    let active = conv.active_messages();
                    generate_title(
                        state_clone.clone(),
                        conversation_id.clone(),
                        &active,
                    )
                    .await;
                }
            }
            Err(e) => {
                tracing::error!("Agent turn failed: {}", e);
                // Safety net: run_agent_turn sends RUN_ERROR internally,
                // but if that was missed, send one from here too.
                let _ = agent_tx.send(AgUiEvent::RunError {
                    thread_id: conversation_id.clone(),
                    run_id: String::new(),
                    message: e.to_string(),
                });
            }
        }
    });
}

/// Convert active-path ChatMessages to Anthropic API Messages
fn conv_to_api_messages(messages: &[&ChatMessage]) -> Vec<Message> {
    messages
        .iter()
        .filter_map(|msg| {
            let role = match msg.role {
                MessageRole::User => Role::User,
                MessageRole::Assistant => Role::Assistant,
            };

            let content: Vec<ContentBlock> = msg
                .parts
                .iter()
                .filter_map(|part| match part {
                    MessagePart::Text { text } => {
                        Some(ContentBlock::Text { text: text.clone() })
                    }
                    MessagePart::ToolCall {
                        tool_call_id,
                        tool_name,
                        args,
                        ..
                    } => Some(ContentBlock::ToolUse {
                        id: tool_call_id.clone(),
                        name: tool_name.clone(),
                        input: if args.is_object() {
                            args.clone()
                        } else {
                            serde_json::json!({})
                        },
                    }),
                    MessagePart::Thinking { .. } => None,
                })
                .collect();

            let tool_results: Vec<ContentBlock> = msg
                .parts
                .iter()
                .filter_map(|part| match part {
                    MessagePart::ToolCall {
                        tool_call_id,
                        result: Some(result),
                        is_error,
                        ..
                    } if role == Role::User => Some(ContentBlock::ToolResult {
                        tool_use_id: tool_call_id.clone(),
                        content: result.clone(),
                        is_error: Some(*is_error),
                    }),
                    _ => None,
                })
                .collect();

            let all_content = if !tool_results.is_empty() {
                tool_results
            } else if content.is_empty() {
                return None;
            } else {
                content
            };

            Some(Message {
                role,
                content: all_content,
            })
        })
        .collect()
}

/// Generate a short title for a conversation and broadcast it.
async fn generate_title(
    state: Arc<AppState>,
    conversation_id: String,
    messages: &[&ChatMessage],
) {
    let mut summary = String::new();
    for msg in messages.iter().take(4) {
        let role = match msg.role {
            MessageRole::User => "User",
            MessageRole::Assistant => "Assistant",
        };
        for part in &msg.parts {
            if let MessagePart::Text { text } = part {
                let truncated: String = text.chars().take(300).collect();
                summary.push_str(&format!("{}: {}\n", role, truncated));
            }
        }
    }

    let request = MessagesRequest {
        model: "claude-haiku-4-5-20251001".to_string(),
        max_tokens: 30,
        system: Some(
            "Generate a very short title (3-6 words) for this conversation. \
             Reply with only the title, no quotes or punctuation."
                .to_string(),
        ),
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text { text: summary }],
        }],
        tools: Vec::new(),
        stream: false,
        temperature: None,
    };

    let Some(ref title_client) = state.title_client else {
        tracing::debug!("No title client configured, skipping title generation");
        return;
    };

    match title_client.create_message(request).await {
        Ok(response) => {
            let title = response
                .content
                .iter()
                .find_map(|block| {
                    if let ContentBlock::Text { text } = block {
                        Some(text.trim().to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

            if title.is_empty() {
                return;
            }

            {
                let mut store = state.conversations.write().await;
                if let Err(e) = store.rename(&conversation_id, &title) {
                    tracing::error!("Failed to save title: {}", e);
                }
            }

            let _ = state.event_bridge.agent_tx().send(AgUiEvent::Custom {
                thread_id: conversation_id,
                name: "title_update".to_string(),
                value: serde_json::json!({ "title": title }),
            });
        }
        Err(e) => {
            tracing::warn!("Title generation failed: {}", e);
        }
    }
}

/// Convert new API Messages back to ChatMessages with parent_id chaining.
/// If `assistant_message_id` is provided, the last assistant message uses that ID
/// (client-generated Snowflake) instead of a server UUID.
fn api_messages_to_chat(
    messages: &[Message],
    initial_parent_id: Option<&str>,
    assistant_message_id: Option<&str>,
) -> Vec<ChatMessage> {
    let mut parent_id = initial_parent_id.map(|s| s.to_string());

    let mut result: Vec<ChatMessage> = messages
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
                    } => MessagePart::ToolCall {
                        tool_call_id: tool_use_id.clone(),
                        tool_name: String::new(),
                        args: serde_json::Value::Null,
                        result: Some(content.clone()),
                        is_error: is_error.unwrap_or(false),
                    },
                    ContentBlock::Thinking { thinking } => MessagePart::Thinking {
                        thinking: thinking.clone(),
                    },
                })
                .collect();

            let chat_msg = ChatMessage {
                id: Uuid::new_v4().to_string(),
                role,
                parts,
                timestamp: Utc::now(),
                parent_id: parent_id.clone(),
                metadata: None,
            };
            parent_id = Some(chat_msg.id.clone());
            chat_msg
        })
        .collect();

    // Merge tool results from user messages into preceding assistant message's
    // ToolCall parts, then drop the now-empty user messages.  The Anthropic API
    // sends tool results as separate user-role messages, but the UI expects each
    // ToolCall part to carry its own result inline.
    let mut i = 0;
    while i < result.len() {
        if result[i].role == MessageRole::User {
            // Collect tool results from this user message
            let tool_results: Vec<(String, String, bool)> = result[i]
                .parts
                .iter()
                .filter_map(|p| match p {
                    MessagePart::ToolCall {
                        tool_call_id,
                        result: Some(res),
                        is_error,
                        ..
                    } => Some((tool_call_id.clone(), res.clone(), *is_error)),
                    _ => None,
                })
                .collect();

            if !tool_results.is_empty() {
                // Find preceding assistant message and merge results
                if let Some(asst) = result[..i]
                    .iter_mut()
                    .rev()
                    .find(|m| m.role == MessageRole::Assistant)
                {
                    for (tc_id, res, is_err) in &tool_results {
                        for part in asst.parts.iter_mut() {
                            if let MessagePart::ToolCall {
                                tool_call_id,
                                result: ref mut slot,
                                is_error: ref mut err_slot,
                                ..
                            } = part
                            {
                                if tool_call_id == tc_id && slot.is_none() {
                                    *slot = Some(res.clone());
                                    *err_slot = *is_err;
                                }
                            }
                        }
                    }
                }

                // Check if user message has any text parts worth keeping
                let has_text = result[i]
                    .parts
                    .iter()
                    .any(|p| matches!(p, MessagePart::Text { text } if !text.is_empty()));

                if !has_text {
                    // Fix parent_id chain: children of this message → its parent
                    let removed_id = result[i].id.clone();
                    let removed_parent = result[i].parent_id.clone();
                    result.remove(i);
                    for m in result.iter_mut() {
                        if m.parent_id.as_deref() == Some(&removed_id) {
                            m.parent_id = removed_parent.clone();
                        }
                    }
                    continue; // don't increment i
                }
            }
        }
        i += 1;
    }

    // Assign client-provided ID to the last assistant message
    if let Some(aid) = assistant_message_id {
        if let Some(last_assistant) = result.iter_mut().rev().find(|m| m.role == MessageRole::Assistant) {
            // Fix parent_id chain: anything that referenced the old ID needs updating
            let old_id = last_assistant.id.clone();
            last_assistant.id = aid.to_string();
            for m in result.iter_mut() {
                if m.parent_id.as_deref() == Some(&old_id) {
                    m.parent_id = Some(aid.to_string());
                }
            }
        }
    }

    result
}
