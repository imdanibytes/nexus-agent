use std::collections::HashMap;
use std::time::Duration;

use super::config::LspServerConfig;
use super::diagnostics::DiagnosticStatus;
use super::languages;
use super::server::LspServer;

/// Diagnostic result from the manager, includes server state info.
pub struct DiagnosticReport {
    pub status: DiagnosticStatus,
    pub server_name: String,
}

/// Manages multiple LSP server instances, scoped to project roots.
///
/// Key: `(config_id, project_path)` — each project root gets its own LSP instance.
/// LSP servers are lazily spawned on first file access.
pub struct LspManager {
    servers: HashMap<(String, String), LspServer>,
    /// language_id → config_id routing
    language_routing: HashMap<String, String>,
    configs: Vec<LspServerConfig>,
    diagnostics_timeout_ms: u64,
}

impl LspManager {
    pub fn new(configs: Vec<LspServerConfig>, diagnostics_timeout_ms: u64) -> Self {
        let mut language_routing = HashMap::new();
        for config in &configs {
            for lang in &config.language_ids {
                language_routing.entry(lang.clone()).or_insert_with(|| config.id.clone());
            }
        }

        Self {
            servers: HashMap::new(),
            language_routing,
            configs,
            diagnostics_timeout_ms,
        }
    }

    /// Get or lazily spawn an LSP server for the given file + project root.
    /// Returns None if no LSP is configured for this language.
    pub async fn ensure_server(
        &mut self,
        file_path: &str,
        project_path: &str,
    ) -> Option<&LspServer> {
        let lang_id = languages::language_id_for_path(file_path)?;
        let config_id = match self.language_routing.get(lang_id) {
            Some(id) => id.clone(),
            None => {
                tracing::debug!(
                    file = %file_path,
                    lang = %lang_id,
                    configured_langs = ?self.language_routing.keys().collect::<Vec<_>>(),
                    "No LSP server configured for language"
                );
                return None;
            }
        };
        let key = (config_id.clone(), project_path.to_string());

        // Remove crashed servers (background loop exited → channel closed)
        if let Some(existing) = self.servers.get(&key) {
            if existing.is_dead() {
                tracing::info!(lsp = %existing.name, root = %project_path, "LSP server crashed, restarting");
                self.servers.remove(&key);
            }
        }

        if !self.servers.contains_key(&key) {
            let config = self.configs.iter().find(|c| c.id == config_id)?;
            let root_uri = format!("file://{project_path}");
            match LspServer::spawn(config, &root_uri).await {
                Ok(server) => {
                    tracing::info!(
                        lsp = %config.name,
                        root = %project_path,
                        "LSP server started"
                    );
                    self.servers.insert(key.clone(), server);
                }
                Err(e) => {
                    tracing::warn!(
                        lsp = %config.name,
                        root = %project_path,
                        error = %e,
                        "Failed to spawn LSP server"
                    );
                    return None;
                }
            }
        }

        self.servers.get(&key)
    }

    /// Get cached diagnostics for a file (non-blocking).
    #[allow(dead_code)]
    pub async fn diagnostics_for(
        &self,
        file_path: &str,
        project_path: &str,
    ) -> Vec<lsp_types::Diagnostic> {
        let lang_id = match languages::language_id_for_path(file_path) {
            Some(l) => l,
            None => return vec![],
        };
        let config_id = match self.language_routing.get(lang_id) {
            Some(id) => id,
            None => return vec![],
        };
        let key = (config_id.clone(), project_path.to_string());
        match self.servers.get(&key) {
            Some(server) => server.cached_diagnostics(file_path).await,
            None => vec![],
        }
    }

    /// After a write: open file if needed, notify change, wait for diagnostics.
    pub async fn diagnostics_after_write(
        &mut self,
        file_path: &str,
        content: &str,
        project_path: &str,
    ) -> Option<DiagnosticReport> {
        let lang_id = match languages::language_id_for_path(file_path) {
            Some(l) => l,
            None => return None,
        };

        self.ensure_server(file_path, project_path).await?;

        let config_id = match self.language_routing.get(lang_id) {
            Some(id) => id.clone(),
            None => return None,
        };
        let key = (config_id, project_path.to_string());
        let server = self.servers.get(&key)?;
        let server_name = server.name.clone();

        if let Err(e) = server.open_file(file_path, lang_id).await {
            tracing::debug!(error = %e, "Failed to open file in LSP");
        }

        if let Err(e) = server.notify_change(file_path, content).await {
            tracing::debug!(error = %e, "Failed to send didChange to LSP");
        }

        let timeout = Duration::from_millis(self.diagnostics_timeout_ms);
        let status = server.diagnostics_for(file_path, timeout).await;
        Some(DiagnosticReport { status, server_name })
    }

    /// After a read: open file if needed, wait for diagnostics.
    pub async fn diagnostics_after_read(
        &mut self,
        file_path: &str,
        project_path: &str,
    ) -> Option<DiagnosticReport> {
        let lang_id = match languages::language_id_for_path(file_path) {
            Some(l) => l,
            None => return None,
        };

        self.ensure_server(file_path, project_path).await?;

        let config_id = match self.language_routing.get(lang_id) {
            Some(id) => id.clone(),
            None => return None,
        };
        let key = (config_id, project_path.to_string());
        let server = self.servers.get(&key)?;
        let server_name = server.name.clone();

        if let Err(e) = server.open_file(file_path, lang_id).await {
            tracing::debug!(error = %e, "Failed to open file in LSP");
        }

        let timeout = Duration::from_millis(self.diagnostics_timeout_ms);
        let status = server.diagnostics_for(file_path, timeout).await;
        Some(DiagnosticReport { status, server_name })
    }

    /// Eagerly spawn LSP servers for all configured languages × project paths.
    /// Called at startup so servers can index in the background.
    pub async fn warm_up(&mut self, project_paths: &[String]) {
        for config in self.configs.clone() {
            for project_path in project_paths {
                let key = (config.id.clone(), project_path.to_string());
                if self.servers.contains_key(&key) {
                    continue;
                }
                let root_uri = format!("file://{project_path}");
                match LspServer::spawn(&config, &root_uri).await {
                    Ok(server) => {
                        tracing::info!(
                            lsp = %config.name,
                            root = %project_path,
                            "LSP server pre-warmed"
                        );
                        self.servers.insert(key, server);
                    }
                    Err(e) => {
                        tracing::warn!(
                            lsp = %config.name,
                            root = %project_path,
                            error = %e,
                            "Failed to pre-warm LSP server"
                        );
                    }
                }
            }
        }
    }

    /// Shutdown all running LSP servers.
    pub async fn shutdown_all(&self) {
        for ((_config_id, project_path), server) in &self.servers {
            tracing::info!(lsp = %server.name, root = %project_path, "Shutting down LSP server");
            server.shutdown().await;
        }
    }

    /// Reload with new configs. Shuts down all existing servers.
    pub async fn reload(&mut self, configs: Vec<LspServerConfig>, diagnostics_timeout_ms: u64) {
        self.shutdown_all().await;
        self.servers.clear();

        self.language_routing.clear();
        for config in &configs {
            for lang in &config.language_ids {
                self.language_routing.entry(lang.clone()).or_insert_with(|| config.id.clone());
            }
        }
        self.configs = configs;
        self.diagnostics_timeout_ms = diagnostics_timeout_ms;
    }
}
