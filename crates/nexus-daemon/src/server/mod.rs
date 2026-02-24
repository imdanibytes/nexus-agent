pub mod agent_api;
pub mod chat;
pub mod conversations;
pub mod mcp_api;
pub mod providers;
pub mod sse;

use axum::extract::State;
use axum::routing::{get, patch, post, put};
use axum::{Json, Router};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::agent_config::AgentStore;
use crate::anthropic::AnthropicClient;
use crate::config::NexusConfig;
use crate::conversation::ConversationStore;
use crate::mcp::store::McpServerStore;
use crate::mcp::McpManager;
use crate::provider::{ProviderFactory, ProviderStore};
use sse::{AgentEventBridge, SseHub};

pub struct AppState {
    pub config: NexusConfig,
    pub conversations: RwLock<ConversationStore>,
    pub providers: RwLock<ProviderStore>,
    pub agents: RwLock<AgentStore>,
    pub factory: Arc<ProviderFactory>,
    /// Anthropic client used only for title generation (from ANTHROPIC_API_KEY env)
    pub title_client: Option<AnthropicClient>,
    pub mcp: RwLock<McpManager>,
    pub mcp_configs: RwLock<McpServerStore>,
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
        // Providers
        .route(
            "/api/providers",
            get(providers::list).post(providers::create),
        )
        .route(
            "/api/providers/{id}",
            get(providers::get)
                .put(providers::update)
                .delete(providers::delete),
        )
        .route(
            "/api/providers/test",
            post(providers::test_inline),
        )
        .route(
            "/api/providers/{id}/test",
            post(providers::test_connection),
        )
        .route(
            "/api/providers/{id}/models",
            get(providers::list_models),
        )
        // Agents — /active must come before /{id}
        .route(
            "/api/agents/active",
            get(agent_api::get_active).put(agent_api::set_active),
        )
        .route(
            "/api/agents",
            get(agent_api::list).post(agent_api::create),
        )
        .route(
            "/api/agents/{id}",
            get(agent_api::get)
                .put(agent_api::update)
                .delete(agent_api::delete),
        )
        // MCP Servers
        .route(
            "/api/mcp-servers/test",
            post(mcp_api::test_inline),
        )
        .route(
            "/api/mcp-servers",
            get(mcp_api::list).post(mcp_api::create),
        )
        .route(
            "/api/mcp-servers/{id}",
            put(mcp_api::update).delete(mcp_api::delete),
        )
        // Tools
        .route("/api/tools", get(list_tools))
        // SSE events
        .route("/api/events", get(events_stream))
        // Status
        .route("/api/status", get(health))
        // Static files (UI) — SPA fallback: serve index.html for non-file routes
        .fallback_service(
            ServeDir::new(ui_dist_path)
                .not_found_service(tower_http::services::ServeFile::new(
                    format!("{}/index.html", ui_dist_path),
                )),
        )
        .layer(CorsLayer::permissive())
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn list_tools(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let mcp = state.mcp.read().await;
    let tools = mcp.tools();
    Json(serde_json::json!({ "tools": tools }))
}

async fn events_stream(
    State(state): State<Arc<AppState>>,
) -> axum::response::sse::Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>>
{
    state.sse_hub.subscribe()
}
