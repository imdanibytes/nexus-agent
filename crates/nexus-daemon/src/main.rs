mod agent;
mod anthropic;
mod config;
mod conversation;
mod mcp;
mod server;

use anyhow::{Context, Result};

use crate::anthropic::AnthropicClient;
use crate::config::NexusConfig;
use crate::conversation::ConversationStore;
use crate::mcp::McpManager;
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

    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .context("ANTHROPIC_API_KEY not set. Put it in .env or ~/.nexus/.env")?;

    let client = AnthropicClient::new(api_key);

    let conversations_dir = NexusConfig::nexus_dir().join("conversations");
    let conversations = ConversationStore::load(conversations_dir)?;

    let sse_hub = SseHub::new();
    let event_bridge = AgentEventBridge::new(sse_hub.clone());

    tracing::info!(
        model = %config.api.model,
        mcp_servers = config.mcp_servers.len(),
        "Nexus daemon starting"
    );

    let mcp = McpManager::from_configs(&config.mcp_servers).await;

    let state = AppState {
        client,
        config: config.clone(),
        conversations: tokio::sync::RwLock::new(conversations),
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
