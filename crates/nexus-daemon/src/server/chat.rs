use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::conversation::types::{ChatMessage, MessagePart, MessageRole, MessageSource};
use crate::server::AppState;
use super::turn::{spawn_agent_turn, TurnRequest};

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

    let (cancel, run_id) = state.turns.register_turn(&conversation_id).await;

    let (req, user_msg_id) = {
        let mut conv = state.threads.checkout(&conversation_id).await
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
            source: Some(MessageSource::Human),
            metadata: None,
        };

        let user_msg_id = user_msg.id.clone();
        conv.active_path.push(user_msg.id.clone());
        conv.messages.push(user_msg);
        conv.updated_at = Utc::now();

        // Build TurnRequest BEFORE commit (commit takes ownership of conv)
        let req = TurnRequest {
            conversation_id,
            api_messages: conv.build_api_messages(),
            tools: resolve_mcp_tools(&state).await,
            cancel,
            run_id,
            assistant_message_id: body.assistant_message_id,
            last_active_id: conv.active_path.last().cloned(),
            prior_cost: conv.usage.as_ref().map(|u| u.total_cost).unwrap_or(0.0),
            title: conv.title.clone(),
            message_count: conv.active_path.len(),
        };

        state.threads.commit(conv).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        (req, user_msg_id)
    };

    spawn_agent_turn(state, req);

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

    let (cancel, run_id) = state.turns.register_turn(&conversation_id).await;

    let (req, new_msg_id) = {
        let mut conv = state.threads.checkout(&conversation_id).await
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

        // Cannot branch into a sealed span
        if conv.is_in_sealed_span(&body.message_id) {
            return Err(StatusCode::CONFLICT);
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
            source: Some(MessageSource::Human),
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

        // Build TurnRequest BEFORE commit (commit takes ownership of conv)
        let req = TurnRequest {
            conversation_id,
            api_messages: conv.build_api_messages(),
            tools: resolve_mcp_tools(&state).await,
            cancel,
            run_id,
            assistant_message_id: body.assistant_message_id,
            last_active_id: conv.active_path.last().cloned(),
            prior_cost: conv.usage.as_ref().map(|u| u.total_cost).unwrap_or(0.0),
            title: conv.title.clone(),
            message_count: conv.active_path.len(),
        };

        state.threads.commit(conv).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        (req, new_msg_id)
    };

    spawn_agent_turn(state, req);

    Ok(Json(
        serde_json::json!({ "ok": true, "conversationId": body.conversation_id, "messageId": new_msg_id }),
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

    let (cancel, run_id) = state.turns.register_turn(&conversation_id).await;

    let req = {
        let mut conv = state.threads.checkout(&conversation_id).await
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

        // Cannot regenerate within a sealed span
        if conv.is_in_sealed_span(&body.message_id) {
            return Err(StatusCode::CONFLICT);
        }

        // Active path ends at this user message (strip any existing assistant response)
        let user_path = conv.path_to_only(&body.message_id);
        conv.active_path = user_path;
        conv.updated_at = Utc::now();

        // Build TurnRequest BEFORE commit (commit takes ownership of conv)
        let req = TurnRequest {
            conversation_id,
            api_messages: conv.build_api_messages(),
            tools: resolve_mcp_tools(&state).await,
            cancel,
            run_id,
            assistant_message_id: body.assistant_message_id,
            last_active_id: conv.active_path.last().cloned(),
            prior_cost: conv.usage.as_ref().map(|u| u.total_cost).unwrap_or(0.0),
            title: conv.title.clone(),
            message_count: conv.active_path.len(),
        };

        state.threads.commit(conv).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        req
    };

    spawn_agent_turn(state, req);

    Ok(Json(
        serde_json::json!({ "ok": true, "conversationId": body.conversation_id }),
    ))
}

// ── Client-initiated tool invocation ──
//
// Adapted from MCP Apps `visibility: ["app"]` pattern — tools hidden from
// the model but callable by the UI. The server executes the tool, injects
// a synthetic assistant ToolCall + result into the conversation, then starts
// a new agent turn so the model processes the result naturally.

#[derive(Debug, Deserialize)]
pub struct ToolInvokeRequest {
    #[serde(rename = "conversationId")]
    pub conversation_id: String,
    #[serde(rename = "toolName")]
    pub tool_name: String,
    pub args: serde_json::Value,
    /// Client-generated Snowflake ID for the assistant response that follows
    #[serde(rename = "assistantMessageId")]
    pub assistant_message_id: Option<String>,
}

pub async fn tool_invoke(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ToolInvokeRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let conversation_id = body.conversation_id.clone();

    // Only client-only and built-in task tools are allowed through this endpoint
    if !crate::tasks::tools::is_builtin(&body.tool_name) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let (cancel, run_id) = state.turns.register_turn(&conversation_id).await;

    let req = {
        let mut conv = state.threads.checkout(&conversation_id).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

        // Execute the tool (synthetic emitter — client-initiated, no real run)
        let tx = state.turns.event_bridge.agent_tx();
        let tool_emitter = crate::agent::emitter::TurnEmitter::new(
            tx,
            conversation_id.clone(),
            String::new(),
        );
        let (content, is_error) = crate::tasks::tools::handle_builtin(
            &body.tool_name,
            &body.args,
            &conversation_id,
            state.tasks.store(),
            &tool_emitter,
        )
        .await;

        // Create synthetic assistant message with the tool call + inline result
        let tool_call_id = uuid::Uuid::new_v4().to_string();
        let parent_id = conv.active_path.last().cloned();

        let synthetic_msg = ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            parts: vec![MessagePart::ToolCall {
                tool_call_id,
                tool_name: body.tool_name.clone(),
                args: body.args.clone(),
                result: Some(content),
                is_error,
            }],
            timestamp: chrono::Utc::now(),
            parent_id,
            source: Some(MessageSource::System { reason: Some("tool_invoke".into()) }),
            metadata: None,
        };

        conv.active_path.push(synthetic_msg.id.clone());
        conv.messages.push(synthetic_msg);
        conv.updated_at = chrono::Utc::now();

        // Build TurnRequest BEFORE commit (commit takes ownership of conv)
        let req = TurnRequest {
            conversation_id,
            api_messages: conv.build_api_messages(),
            tools: resolve_mcp_tools(&state).await,
            cancel,
            run_id,
            assistant_message_id: body.assistant_message_id,
            last_active_id: conv.active_path.last().cloned(),
            prior_cost: conv.usage.as_ref().map(|u| u.total_cost).unwrap_or(0.0),
            title: conv.title.clone(),
            message_count: conv.active_path.len(),
        };

        state.threads.commit(conv).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        req
    };

    // Start agent turn — model will see the tool result and continue naturally
    spawn_agent_turn(state, req);

    Ok(Json(serde_json::json!({
        "ok": true,
        "conversationId": body.conversation_id,
    })))
}

pub async fn abort_turn(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AbortRequest>,
) -> StatusCode {
    state.turns.cancel_turn(&body.conversation_id).await;
    StatusCode::OK
}

#[derive(Debug, Deserialize)]
pub struct AbortRequest {
    #[serde(rename = "conversationId")]
    pub conversation_id: String,
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
        let mut store = state.turns.pending_questions.write().await;
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

/// Resolve MCP tools filtered by the active agent's mcp_server_ids.
async fn resolve_mcp_tools(state: &AppState) -> Vec<crate::anthropic::types::Tool> {
    let mcp = state.mcp.mcp.read().await;
    let agent = state.agents.active_agent().await;
    match agent.and_then(|a| a.mcp_server_ids).as_ref() {
        Some(ids) => mcp.tools_for(Some(ids)),
        None => mcp.tools(),
    }
}
