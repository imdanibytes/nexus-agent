use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::anthropic::types::{ContentBlock, Message, Role};
use crate::conversation::types::{ChatMessage, MessagePart, MessageRole};
use crate::server::AppState;
use crate::system_prompt::{fence_tool_result, fence_user_message};
use super::turn::spawn_agent_turn;

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
        let mut store = state.chat.conversations.write().await;

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

        let api_messages = build_api_messages(&conv.active_messages());


        // Filter MCP tools based on active agent's mcp_server_ids
        let tools = {
            let mcp = state.mcp.mcp.read().await;
            let agents = state.agents.agents.read().await;
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
        let mut store = state.chat.conversations.write().await;

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

        let api_messages = build_api_messages(&conv.active_messages());


        // Filter MCP tools based on active agent's mcp_server_ids
        let tools = {
            let mcp = state.mcp.mcp.read().await;
            let agents = state.agents.agents.read().await;
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
        let mut store = state.chat.conversations.write().await;

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

        let api_messages = build_api_messages(&conv.active_messages());


        let tools = {
            let mcp = state.mcp.mcp.read().await;
            let agents = state.agents.agents.read().await;
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
    let mut active = state.chat.active_cancel.lock().await;
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
    let mut active = state.chat.active_cancel.lock().await;
    if let Some((ref cid, ref token)) = *active {
        if cid == conversation_id {
            token.cancel();
        }
    }
    let cancel = tokio_util::sync::CancellationToken::new();
    *active = Some((conversation_id.to_string(), cancel.clone()));
    cancel
}

/// Build Anthropic API Messages from active-path ChatMessages.
///
/// Handles both old format (ToolCall with inline result on assistant messages)
/// and new format (separate ToolResult parts on user messages).
/// Fences tool results and user text at the API boundary.
fn build_api_messages(messages: &[&ChatMessage]) -> Vec<Message> {
    let mut result = Vec::new();

    for msg in messages {
        match msg.role {
            MessageRole::Assistant => {
                // Text + ToolUse blocks
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
                        MessagePart::Thinking { .. } | MessagePart::ToolResult { .. } => None,
                    })
                    .collect();

                if !content.is_empty() {
                    result.push(Message {
                        role: Role::Assistant,
                        content,
                    });
                }

                // Legacy: if ToolCall parts carry inline results, emit a user
                // message with ToolResult blocks (old merged format)
                let inline_results: Vec<ContentBlock> = msg
                    .parts
                    .iter()
                    .filter_map(|part| match part {
                        MessagePart::ToolCall {
                            tool_call_id,
                            result: Some(res),
                            is_error,
                            ..
                        } => Some(ContentBlock::ToolResult {
                            tool_use_id: tool_call_id.clone(),
                            content: fence_tool_result(res),
                            is_error: Some(*is_error),
                        }),
                        _ => None,
                    })
                    .collect();

                if !inline_results.is_empty() {
                    result.push(Message {
                        role: Role::User,
                        content: inline_results,
                    });
                }
            }
            MessageRole::User => {
                let mut text_blocks = Vec::new();
                let mut tool_result_blocks = Vec::new();

                for part in &msg.parts {
                    match part {
                        MessagePart::Text { text } => {
                            text_blocks.push(ContentBlock::Text {
                                text: fence_user_message(text),
                            });
                        }
                        MessagePart::ToolResult {
                            tool_call_id,
                            result,
                            is_error,
                        } => {
                            tool_result_blocks.push(ContentBlock::ToolResult {
                                tool_use_id: tool_call_id.clone(),
                                content: fence_tool_result(result),
                                is_error: Some(*is_error),
                            });
                        }
                        _ => {}
                    }
                }

                // Tool results and text can't be mixed in one API message.
                // Tool results go first (they pair with the preceding assistant).
                if !tool_result_blocks.is_empty() {
                    result.push(Message {
                        role: Role::User,
                        content: tool_result_blocks,
                    });
                }
                if !text_blocks.is_empty() {
                    result.push(Message {
                        role: Role::User,
                        content: text_blocks,
                    });
                }
            }
        }
    }

    result
}

// ── Ask-user answer endpoint ──

#[derive(Debug, Deserialize)]
pub struct AnswerRequest {
    #[serde(rename = "conversationId")]
    pub conversation_id: String,
    #[serde(rename = "questionId")]
    pub question_id: String,
    pub value: serde_json::Value,
}

/// POST /api/chat/answer
///
/// Resolves a pending `ask_user` question. Sends the answer through the oneshot
/// channel, which resumes the suspended agent turn.
pub async fn answer_question(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AnswerRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let question = {
        let mut store = state.chat.pending_questions.write().await;
        store.remove(&body.question_id)
    };

    let question = question.ok_or(StatusCode::NOT_FOUND)?;

    if question.conversation_id != body.conversation_id {
        return Err(StatusCode::BAD_REQUEST);
    }

    let answer = crate::ask_user::UserAnswer {
        value: body.value.clone(),
        dismissed: false,
    };

    // Send answer through the oneshot channel — this resumes the agent turn
    let _ = question.response_tx.send(answer);

    Ok(Json(serde_json::json!({
        "ok": true,
        "questionId": body.question_id,
    })))
}
