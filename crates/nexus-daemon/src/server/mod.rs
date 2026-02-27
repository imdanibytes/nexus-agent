pub mod agent_api;
pub mod browse;
pub mod chat;
pub mod conversations;
#[cfg(debug_assertions)]
pub mod debug;
pub mod introspect;
pub mod lsp_api;
pub mod mcp_api;
pub mod message_queue;
pub mod providers;
pub mod services;
pub mod sse;
pub mod turn;
pub mod project_api;
pub mod workspace_api;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, post, put};
use axum::{Json, Router};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::agent_config::AgentService;
use crate::anthropic::AnthropicClient;
use crate::config::{FilesystemConfig, NexusConfig};
use crate::event_bus::EventBus;
use crate::provider::ProviderService;
use crate::tasks::TaskService;
use crate::thread::ThreadService;
use crate::project::ProjectStore;
use crate::workspace::WorkspaceStore;
use tokio::sync::RwLock;

pub use services::{McpService, TurnManager};

pub struct AppState {
    pub config: NexusConfig,
    pub turns: Arc<TurnManager>,
    pub agents: Arc<AgentService>,
    pub providers: Arc<ProviderService>,
    pub mcp: Arc<McpService>,
    pub tasks: Arc<TaskService>,
    pub threads: Arc<ThreadService>,
    pub event_bus: EventBus,
    pub projects: Arc<RwLock<ProjectStore>>,
    pub workspaces: Arc<RwLock<WorkspaceStore>>,
    /// Base filesystem config from nexus.json (without project paths merged).
    pub base_filesystem_config: FilesystemConfig,
    /// Effective filesystem config (projects + base). Updated on project CRUD.
    pub effective_fs_config: RwLock<FilesystemConfig>,
    /// Anthropic client used only for title generation (from ANTHROPIC_API_KEY env)
    pub title_client: Option<AnthropicClient>,
    /// LSP integration — manages language servers and diagnostics
    pub lsp: Arc<crate::lsp::LspService>,
}

pub fn build_router(state: AppState, queue_rx: tokio::sync::mpsc::UnboundedReceiver<String>, ui_dist_path: &str) -> Router {
    let state = Arc::new(state);

    // Start event-driven queue watcher for idle conversations
    start_queue_watcher(queue_rx, Arc::clone(&state));

    let mut router = Router::new()
        // Chat
        .route("/api/chat", post(chat::start_turn))
        .route("/api/chat/branch", post(chat::branch_turn))
        .route("/api/chat/regenerate", post(chat::regenerate_turn))
        .route("/api/chat/abort", post(chat::abort_turn))
        .route("/api/chat/tool-invoke", post(chat::tool_invoke))
        // Conversations
        .route(
            "/api/conversations",
            get(conversations::list).post(conversations::create),
        )
        .route(
            "/api/conversations/{id}",
            get(conversations::get)
                .delete(conversations::delete)
                .patch(conversations::update),
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
        .route(
            "/api/mcp-servers/{id}/resources",
            get(mcp_api::list_resources),
        )
        .route(
            "/api/mcp-servers/{id}/resources/read",
            post(mcp_api::read_resource),
        )
        .route(
            "/api/mcp-servers/{id}/prompts",
            get(mcp_api::list_prompts),
        )
        .route(
            "/api/mcp-servers/{id}/prompts/get",
            post(mcp_api::get_prompt),
        )
        // Projects (codebase roots — renamed from workspaces)
        .route(
            "/api/projects",
            get(project_api::list).post(project_api::create),
        )
        .route(
            "/api/projects/{id}",
            put(project_api::update).delete(project_api::delete),
        )
        // Workspaces (logical groupings of projects)
        .route(
            "/api/workspaces",
            get(workspace_api::list).post(workspace_api::create),
        )
        .route(
            "/api/workspaces/{id}",
            get(workspace_api::get_by_id)
                .put(workspace_api::update)
                .delete(workspace_api::delete),
        )
        // LSP Servers
        .route(
            "/api/lsp-servers",
            get(lsp_api::list),
        )
        .route(
            "/api/lsp-servers/detect",
            post(lsp_api::detect),
        )
        .route(
            "/api/lsp-servers/{id}",
            patch(lsp_api::toggle),
        )
        .route(
            "/api/lsp-settings",
            patch(lsp_api::update_settings),
        )
        // Background processes
        .route(
            "/api/processes/{conversationId}",
            get(list_processes),
        )
        .route(
            "/api/processes/{processId}/stop",
            post(stop_process),
        )
        // Folder browser (for workspace picker)
        .route("/api/browse", get(browse::browse))
        // Ask-user answer endpoint
        .route("/api/chat/answer", post(chat::answer_question))
        // Tools
        .route("/api/tools", get(list_tools))
        // SSE events (global multiplexed stream)
        .route("/api/events", get(events_stream))
        // Status
        .route("/api/status", get(health));

    // Debug endpoints (debug builds only)
    #[cfg(debug_assertions)]
    {
        router = router
            .route(
                "/api/debug/compact/{id}",
                post(debug::force_compact),
            )
            .route(
                "/api/debug/task-state/{id}",
                post(debug::set_task_state),
            )
            .route("/api/debug/emit", post(debug::emit_event));
    }

    // Introspection MCP server — exposes daemon state to MCP clients
    let introspect_state = Arc::clone(&state);
    let mcp_service = introspect::IntrospectService::new(
        move || Ok(introspect::IntrospectMcpServer::new(Arc::clone(&introspect_state))),
        Arc::new(introspect::IntrospectSessionManager::default()),
        introspect::IntrospectServerConfig {
            stateful_mode: true,
            ..Default::default()
        },
    );

    router
        .nest_service("/mcp", mcp_service)
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
    let mcp = state.mcp.mcp.read().await;
    let tools = mcp.tools();
    Json(serde_json::json!({ "tools": tools }))
}

async fn events_stream(
    State(state): State<Arc<AppState>>,
) -> axum::response::sse::Sse<impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>>
{
    let active_runs = state.turns.active_run_ids().await;
    state.turns.event_bridge.subscribe(active_runs).await
}

async fn list_processes(
    State(state): State<Arc<AppState>>,
    Path(conversation_id): Path<String>,
) -> Json<serde_json::Value> {
    let processes = state.turns.process_manager.list(&conversation_id).await;
    Json(serde_json::to_value(&processes).unwrap_or_default())
}

async fn stop_process(
    State(state): State<Arc<AppState>>,
    Path(process_id): Path<String>,
) -> StatusCode {
    match state.turns.process_manager.cancel(&process_id).await {
        Ok(()) => StatusCode::OK,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

/// Event-driven queue watcher. Receives conversation IDs when messages are
/// enqueued. If no turn is active for that conversation, drains the queue
/// and spawns a follow-up turn.
fn start_queue_watcher(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<String>,
    state: Arc<AppState>,
) {
    tokio::spawn(async move {
        while let Some(conv_id) = rx.recv().await {
            // If a turn is active, the after-turn drain will handle it
            if state.turns.is_active(&conv_id).await {
                continue;
            }

            let queued = state.turns.message_queue.drain(&conv_id).await;
            if queued.is_empty() {
                continue;
            }

            tracing::info!(
                conversation_id = %conv_id,
                count = queued.len(),
                "Queue watcher: injecting messages into idle conversation"
            );

            turn::drain_queue_and_follow_up(Arc::clone(&state), conv_id, queued).await;
        }
    });
}
