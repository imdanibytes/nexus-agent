mod agent;
mod agent_config;
mod anthropic;
mod ask_user;
mod bash;
mod bg_process;
mod compaction;
mod config;
mod conversation;
mod event_bus;
mod fetch;
mod filesystem;
mod mcp;
mod mcp_resources;
mod mechanics;
mod pricing;
mod provider;
mod retry;
mod server;
mod system_prompt;
mod tasks;
mod thread;
mod tool_filter;
mod project;
mod workspace;

use anyhow::Result;
use std::sync::Arc;

use crate::agent_config::{AgentService, AgentStore};
use crate::agent_config::store::CreateAgentParams;
use crate::anthropic::AnthropicClient;
use crate::config::NexusConfig;
use crate::conversation::ConversationStore;
use crate::event_bus::EventBus;
use crate::mcp::store::McpServerStore;
use crate::mcp::{ClientHandlerState, McpManager};
use crate::provider::{ProviderService, ProviderStore, ProviderType};
use crate::provider::store::CreateProviderParams;
use crate::server::sse::AgentEventBridge;
use crate::server::{AppState, McpService, TurnManager};
use crate::thread::ThreadService;
use crate::project::ProjectStore;
use crate::workspace::WorkspaceStore;

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env from cwd, ~/.nexus/.env, or workspace root — silent if missing
    for path in [".env", &format!("{}/.env", NexusConfig::nexus_dir().display())] {
        let _ = dotenvy::from_filename(path);
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nexus=info".into()),
        )
        .init();

    let config = NexusConfig::load()?;
    let mcp_servers = NexusConfig::load_mcp_servers()?;

    // Load and apply corporate fetch policy (if any)
    let fetch_policy = NexusConfig::load_fetch_policy();
    let mut config = config;
    config.fetch.apply_policy(&fetch_policy);
    tracing::debug!(
        fetch_enabled = config.fetch.enabled,
        deny_domains = ?config.fetch.deny_domains,
        allow_domains = ?config.fetch.allow_domains,
        "Fetch config resolved (user + policy)"
    );

    let mut provider_store = ProviderStore::new(config.providers.clone());
    let mut agent_store = AgentStore::new(
        config.agents.clone(),
        config.active_agent_id.clone(),
    );

    // Title generation client (optional — from env var)
    let title_client = std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .map(AnthropicClient::new);

    // Backward compat: seed default provider + agent from ANTHROPIC_API_KEY if none exist
    if provider_store.list().is_empty() {
        if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
            tracing::info!("No providers configured — seeding default from ANTHROPIC_API_KEY");
            let provider = provider_store.create(CreateProviderParams {
                name: "Default (Anthropic)".to_string(),
                provider_type: ProviderType::Anthropic,
                endpoint: None,
                api_key: Some(api_key),
                aws_region: None,
                aws_profile: None,
            })?;

            if agent_store.list().is_empty() {
                let agent = agent_store.create(CreateAgentParams {
                    name: "Default Agent".to_string(),
                    provider_id: provider.id.clone(),
                    model: config.api.model.clone(),
                    system_prompt: config.agent.system_prompt.clone(),
                    temperature: None,
                    max_tokens: Some(config.api.max_tokens),
                    mcp_server_ids: None,
                })?;
                agent_store.set_active(Some(agent.id))?;
                tracing::info!("Created default agent with model {}", config.api.model);
            }
        } else {
            tracing::warn!(
                "No providers configured and ANTHROPIC_API_KEY not set. \
                 Configure providers via the API or set ANTHROPIC_API_KEY."
            );
        }
    }

    let nexus_dir = NexusConfig::nexus_dir();
    let conversations_dir = nexus_dir.join("conversations");

    let event_bridge = AgentEventBridge::new();
    // EventBus shares the same broadcast channel as AgentEventBridge
    let event_bus = EventBus::from_sender(event_bridge.agent_tx());
    // ThreadService owns the ConversationStore — all conversation CRUD goes through it
    let conversations = ConversationStore::load(conversations_dir)?;
    let threads = Arc::new(ThreadService::new(conversations, event_bus.clone()));

    // Projects + workspaces + effective filesystem config
    let project_store = ProjectStore::new(config.projects.clone());
    let workspace_store = WorkspaceStore::new(
        config.workspaces.clone(),
        config.active_workspace_id.clone(),
    );
    let effective_fs = config.effective_filesystem_config();

    tracing::info!(
        providers = provider_store.list().len(),
        agents = agent_store.list().len(),
        mcp_servers = mcp_servers.len(),
        projects = config.projects.len(),
        workspaces = config.workspaces.len(),
        allowed_dirs = effective_fs.allowed_directories.len(),
        "Nexus daemon starting"
    );

    // Build shared handler state for MCP client connections.
    // Uses Arc to the same RwLock<ProjectStore> that AppState will hold,
    // avoiding a reference cycle (handler doesn't hold Arc<McpService>).
    let projects_lock = std::sync::Arc::new(tokio::sync::RwLock::new(project_store));
    let workspaces_lock = std::sync::Arc::new(tokio::sync::RwLock::new(workspace_store));
    let handler_state = ClientHandlerState {
        projects: std::sync::Arc::clone(&projects_lock),
    };

    let mcp = McpManager::from_configs(&mcp_servers, &handler_state).await;
    let mcp_configs = McpServerStore::new(mcp_servers);

    let (message_queue, queue_rx) = server::message_queue::MessageQueue::new();
    let message_queue = Arc::new(message_queue);

    let process_manager = Arc::new(bg_process::ProcessManager::new(
        nexus_dir.join("bg-processes"),
        event_bridge.agent_tx(),
        message_queue.clone(),
    ));

    let turns = Arc::new(TurnManager::new(
        event_bridge,
        ask_user::PendingQuestionStore::new(),
        process_manager,
        message_queue,
    ));

    let agents_svc = Arc::new(AgentService::new(agent_store, event_bus.clone()));
    let providers_svc = Arc::new(ProviderService::new(provider_store, event_bus.clone()));
    let task_svc = Arc::new(tasks::TaskService::new(nexus_dir.join("tasks"), event_bus.clone()));

    let mcp_svc = Arc::new(McpService {
        mcp: tokio::sync::RwLock::new(mcp),
        configs: tokio::sync::RwLock::new(mcp_configs),
    });

    let state = AppState {
        base_filesystem_config: config.filesystem.clone(),
        effective_fs_config: tokio::sync::RwLock::new(effective_fs),
        projects: projects_lock,
        workspaces: workspaces_lock,
        config: config.clone(),
        turns,
        agents: agents_svc,
        providers: providers_svc,
        mcp: mcp_svc,
        tasks: task_svc,
        threads,
        event_bus,
        title_client,
    };

    let router = server::build_router(state, queue_rx, "ui/dist");

    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    let actual_addr = listener.local_addr()?;
    tracing::info!("Listening on http://{}", actual_addr);

    // Write actual port to file (useful when port=0 is used for OS-assigned ports)
    let port_file = nexus_dir.join("port");
    let _ = std::fs::write(&port_file, actual_addr.port().to_string());

    axum::serve(listener, router).await?;

    Ok(())
}
