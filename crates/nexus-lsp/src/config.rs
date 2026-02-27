use anyhow::Result;
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

fn default_diagnostics_timeout() -> u64 {
    3000
}

/// A known LSP server — either auto-detected or manually configured.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServerConfig {
    pub id: String,
    pub name: String,
    pub language_ids: Vec<String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub auto_detected: bool,
}

/// Top-level LSP settings, persisted to ~/.nexus/lsp.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub servers: Vec<LspServerConfig>,
    #[serde(default = "default_diagnostics_timeout")]
    pub diagnostics_timeout_ms: u64,
}

impl Default for LspSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            servers: Vec::new(),
            diagnostics_timeout_ms: default_diagnostics_timeout(),
        }
    }
}

/// Function that persists LSP settings to disk.
pub type SaveFn = Box<dyn Fn(&LspSettings) -> Result<()> + Send + Sync>;

/// Manages CRUD for LSP server configurations (persisted to lsp.json).
pub struct LspConfigStore {
    settings: LspSettings,
    save_fn: Option<SaveFn>,
}

impl LspConfigStore {
    pub fn new(settings: LspSettings) -> Self {
        Self {
            settings,
            save_fn: None,
        }
    }

    /// Set a persistence callback. Without this, mutations are in-memory only.
    pub fn with_save(mut self, f: SaveFn) -> Self {
        self.save_fn = Some(f);
        self
    }

    pub fn settings(&self) -> &LspSettings {
        &self.settings
    }

    pub fn servers(&self) -> &[LspServerConfig] {
        &self.settings.servers
    }

    #[allow(dead_code)]
    pub fn get(&self, id: &str) -> Option<&LspServerConfig> {
        self.settings.servers.iter().find(|s| s.id == id)
    }

    /// Toggle an individual server's enabled state.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<Option<LspServerConfig>> {
        if let Some(server) = self.settings.servers.iter_mut().find(|s| s.id == id) {
            server.enabled = enabled;
            let config = server.clone();
            self.save()?;
            Ok(Some(config))
        } else {
            Ok(None)
        }
    }

    /// Toggle global LSP integration on/off.
    pub fn set_global_enabled(&mut self, enabled: bool) -> Result<()> {
        self.settings.enabled = enabled;
        self.save()
    }

    pub fn set_diagnostics_timeout(&mut self, timeout_ms: u64) -> Result<()> {
        self.settings.diagnostics_timeout_ms = timeout_ms;
        self.save()
    }

    /// Merge auto-detected servers with existing config.
    /// Preserves user enable/disable choices for previously detected servers.
    pub fn upsert_detected(&mut self, detected: Vec<LspServerConfig>) -> Result<()> {
        for new in detected {
            if let Some(existing) = self.settings.servers.iter_mut().find(|s| s.command == new.command) {
                // Update path/args but preserve user's enabled choice
                existing.name = new.name;
                existing.language_ids = new.language_ids;
                existing.args = new.args;
                existing.auto_detected = true;
            } else {
                self.settings.servers.push(new);
            }
        }
        self.save()
    }

    fn save(&self) -> Result<()> {
        match &self.save_fn {
            Some(f) => f(&self.settings),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(id: &str, name: &str, cmd: &str, langs: &[&str]) -> LspServerConfig {
        LspServerConfig {
            id: id.to_string(),
            name: name.to_string(),
            language_ids: langs.iter().map(|s| s.to_string()).collect(),
            command: cmd.to_string(),
            args: vec![],
            enabled: true,
            auto_detected: true,
        }
    }

    #[test]
    fn default_settings() {
        let settings = LspSettings::default();
        assert!(settings.enabled);
        assert_eq!(settings.diagnostics_timeout_ms, 3000);
        assert!(settings.servers.is_empty());
    }

    #[test]
    fn settings_deserialization_defaults() {
        let json = r#"{"servers": []}"#;
        let settings: LspSettings = serde_json::from_str(json).unwrap();
        assert!(settings.enabled);
        assert_eq!(settings.diagnostics_timeout_ms, 3000);
    }

    #[test]
    fn upsert_detected_adds_new() {
        let mut store = LspConfigStore::new(LspSettings::default());
        let detected = vec![make_config("ra", "rust-analyzer", "rust-analyzer", &["rust"])];
        store.upsert_detected(detected).unwrap();
        assert_eq!(store.servers().len(), 1);
        assert_eq!(store.servers()[0].name, "rust-analyzer");
    }

    #[test]
    fn upsert_detected_preserves_enabled() {
        let mut settings = LspSettings::default();
        settings.servers.push(LspServerConfig {
            id: "ra".to_string(),
            name: "rust-analyzer".to_string(),
            language_ids: vec!["rust".to_string()],
            command: "rust-analyzer".to_string(),
            args: vec![],
            enabled: false, // User disabled it
            auto_detected: true,
        });

        let mut store = LspConfigStore::new(settings);
        let detected = vec![make_config("ra-new", "rust-analyzer", "rust-analyzer", &["rust"])];
        store.upsert_detected(detected).unwrap();

        // Should still have one server (matched by command), and enabled should be preserved
        assert_eq!(store.servers().len(), 1);
        assert!(!store.servers()[0].enabled);
    }

    #[test]
    fn get_by_id() {
        let mut settings = LspSettings::default();
        settings.servers.push(make_config("ra", "rust-analyzer", "rust-analyzer", &["rust"]));
        let store = LspConfigStore::new(settings);
        assert!(store.get("ra").is_some());
        assert!(store.get("nonexistent").is_none());
    }
}
