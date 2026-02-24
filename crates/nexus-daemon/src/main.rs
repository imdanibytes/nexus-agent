mod agent;
mod agent_config;
mod anthropic;
mod config;
mod conversation;
mod mcp;
mod provider;
mod server;

use anyhow::Result;
use std::sync::Arc;

use crate::agent_config::AgentStore;
use crate::anthropic::AnthropicClient;
use crate::config::NexusConfig;
use crate::conversation::ConversationStore;
use crate::mcp::McpManager;
use crate::provider::{ProviderFactory, ProviderStore, ProviderType};
use crate::server::sse::{AgentEventBridge, SseHub};
use crate::server::AppState;

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
            let provider = provider_store.create(
                "Default (Anthropic)".to_string(),
                ProviderType::Anthropic,
                None,
                Some(api_key),
                None,
                None,
            )?;

            if agent_store.list().is_empty() {
                let agent = agent_store.create(
                    "Default Agent".to_string(),
                    provider.id.clone(),
                    config.api.model.clone(),
                    config.agent.system_prompt.clone(),
                    None,
                    Some(config.api.max_tokens),
                )?;
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
    let conversations = ConversationStore::load(conversations_dir)?;

    let sse_hub = SseHub::new();
    let event_bridge = AgentEventBridge::new(sse_hub.clone());

    tracing::info!(
        providers = provider_store.list().len(),
        agents = agent_store.list().len(),
        mcp_servers = mcp_servers.len(),
        "Nexus daemon starting"
    );

    let mcp = McpManager::from_configs(&mcp_servers).await;
    let factory = Arc::new(ProviderFactory::new());

    let state = AppState {
        config: config.clone(),
        conversations: tokio::sync::RwLock::new(conversations),
        providers: tokio::sync::RwLock::new(provider_store),
        agents: tokio::sync::RwLock::new(agent_store),
        factory,
        title_client,
        mcp,
        sse_hub,
        event_bridge,
        active_cancel: tokio::sync::Mutex::new(None),
    };

    let router = server::build_router(state, "ui/dist");

    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Listening on http://{}", addr);
    axum::serve(listener, router).await?;

    Ok(())
}
