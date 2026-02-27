use std::sync::Arc;

use rmcp::handler::client::ClientHandler;
use rmcp::ErrorData as McpError;
use rmcp::model::{
    ClientCapabilities, ClientInfo, Implementation, ListRootsResult,
    LoggingLevel, LoggingMessageNotificationParam, Root,
};
use rmcp::service::{NotificationContext, RequestContext, RoleClient};
use tokio::sync::RwLock;

use crate::workspace::WorkspaceStore;

/// State shared with each MCP client handler.
///
/// Deliberately avoids holding `Arc<McpService>` to prevent a reference cycle:
/// `AppState → McpService → McpManager → McpServer → handler → AppState`.
#[derive(Clone)]
pub struct ClientHandlerState {
    pub workspaces: Arc<RwLock<WorkspaceStore>>,
}

/// MCP client handler for nexus-daemon.
///
/// Responds to server→client requests: root listing, logging, tool/resource
/// change notifications. Sampling and elicitation use defaults (not-found /
/// decline) until those features are implemented.
#[derive(Clone)]
pub struct NexusClientHandler {
    state: ClientHandlerState,
}

impl NexusClientHandler {
    pub fn new(state: ClientHandlerState) -> Self {
        Self { state }
    }
}

impl ClientHandler for NexusClientHandler {
    fn get_info(&self) -> ClientInfo {
        ClientInfo {
            meta: None,
            protocol_version: Default::default(),
            capabilities: ClientCapabilities::builder()
                .enable_roots()
                .build(),
            client_info: Implementation {
                name: "nexus-daemon".into(),
                title: None,
                version: env!("CARGO_PKG_VERSION").into(),
                description: None,
                icons: None,
                website_url: None,
            },
        }
    }

    fn list_roots(
        &self,
        _context: RequestContext<RoleClient>,
    ) -> impl std::future::Future<Output = Result<ListRootsResult, McpError>> + Send + '_ {
        async move {
            let store = self.state.workspaces.read().await;
            let roots: Vec<Root> = store
                .list()
                .iter()
                .map(|w| Root {
                    uri: format!("file://{}", w.path),
                    name: Some(w.name.clone()),
                })
                .collect();
            Ok(ListRootsResult { roots })
        }
    }

    fn on_logging_message(
        &self,
        params: LoggingMessageNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        async move {
            let logger = params.logger.as_deref().unwrap_or("mcp");
            let data = &params.data;
            match params.level {
                LoggingLevel::Debug => tracing::debug!(logger, %data, "MCP server log"),
                LoggingLevel::Info | LoggingLevel::Notice => {
                    tracing::info!(logger, %data, "MCP server log")
                }
                LoggingLevel::Warning => tracing::warn!(logger, %data, "MCP server log"),
                LoggingLevel::Error | LoggingLevel::Critical | LoggingLevel::Alert | LoggingLevel::Emergency => {
                    tracing::error!(logger, %data, "MCP server log")
                }
            }
        }
    }

    fn on_tool_list_changed(
        &self,
        _context: NotificationContext<RoleClient>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        async {
            tracing::info!("MCP server reported tool list changed (dynamic refresh not yet implemented)");
        }
    }

    fn on_resource_list_changed(
        &self,
        _context: NotificationContext<RoleClient>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        async {
            tracing::info!("MCP server reported resource list changed");
        }
    }
}
