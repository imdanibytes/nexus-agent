mod config;

use anyhow::{Context, Result};
use axum::{routing::get, Json, Router};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::config::NexusConfig;

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nexus=info".into()),
        )
        .init();

    let config = NexusConfig::load()?;

    let _api_key = std::env::var("ANTHROPIC_API_KEY")
        .context("ANTHROPIC_API_KEY environment variable not set")?;

    tracing::info!(
        model = %config.api.model,
        mcp_servers = config.mcp_servers.len(),
        "Nexus daemon starting"
    );

    let router = Router::new()
        .route("/api/status", get(health))
        .fallback_service(ServeDir::new("ui/dist"))
        .layer(CorsLayer::permissive());

    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Listening on http://{}", addr);
    axum::serve(listener, router).await?;

    Ok(())
}
