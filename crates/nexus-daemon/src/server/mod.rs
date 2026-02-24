pub mod chat;
pub mod conversations;
pub mod sse;

use axum::extract::State;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::anthropic::AnthropicClient;
use crate::config::NexusConfig;
use crate::conversation::ConversationStore;
use crate::mcp::McpManager;
use sse::{AgentEventBridge, SseHub};

pub struct AppState {
    pub client: AnthropicClient,
    pub config: NexusConfig,
    pub conversations: RwLock<ConversationStore>,
    pub mcp: McpManager,
    pub sse_hub: SseHub,
    pub event_bridge: AgentEventBridge,
    pub active_cancel:
        Mutex<Option<(String, tokio_util::sync::CancellationToken)>>,
}

pub fn build_router(state: AppState, ui_dist_path: &str) -> Router {
    let state = Arc::new(state);

    Router::new()
        // Chat
        .route("/api/chat", post(chat::start_turn))
        .route("/api/chat/branch", post(chat::branch_turn))
        .route("/api/chat/regenerate", post(chat::regenerate_turn))
        .route("/api/chat/abort", post(chat::abort_turn))
        // Conversations
        .route(
            "/api/conversations",
            get(conversations::list).post(conversations::create),
        )
        .route(
            "/api/conversations/{id}",
            get(conversations::get)
                .delete(conversations::delete)
                .patch(conversations::rename),
        )
        .route(
            "/api/conversations/{id}/path",
            patch(conversations::switch_path),
        )
        // Tools
        .route("/api/tools", get(list_tools))
        // SSE events
        .route("/api/events", get(events_stream))
        // Status
        .route("/api/status", get(health))
        // Static files (UI)
        .fallback_service(ServeDir::new(ui_dist_path))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn list_tools(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let tools = state.mcp.tools();
    Json(serde_json::json!({ "tools": tools }))
}

async fn events_stream(
    State(state): State<Arc<AppState>>,
) -> axum::response::sse::Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>>
{
    state.sse_hub.subscribe()
}
