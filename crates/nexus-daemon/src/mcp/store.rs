use anyhow::Result;
use uuid::Uuid;

use crate::config::{McpServerConfig, NexusConfig};

/// Manages CRUD for MCP server configurations (persisted to mcp.json).
/// Separate from McpManager which manages running server processes.
pub struct McpServerStore {
    configs: Vec<McpServerConfig>,
}

impl McpServerStore {
    pub fn new(configs: Vec<McpServerConfig>) -> Self {
        Self { configs }
    }

    pub fn list(&self) -> &[McpServerConfig] {
        &self.configs
    }

    pub fn get(&self, id: &str) -> Option<&McpServerConfig> {
        self.configs.iter().find(|c| c.id == id)
    }

    pub fn create(
        &mut self,
        name: String,
        command: String,
        args: Vec<String>,
        env: std::collections::HashMap<String, String>,
    ) -> Result<McpServerConfig> {
        let config = McpServerConfig {
            id: Uuid::new_v4().to_string(),
            name,
            command,
            args,
            env,
        };
        self.configs.push(config.clone());
        self.save()?;
        Ok(config)
    }

    pub fn update(&mut self, id: &str, updates: McpServerUpdate) -> Result<Option<McpServerConfig>> {
        let Some(config) = self.configs.iter_mut().find(|c| c.id == id) else {
            return Ok(None);
        };

        if let Some(name) = updates.name {
            config.name = name;
        }
        if let Some(command) = updates.command {
            config.command = command;
        }
        if let Some(args) = updates.args {
            config.args = args;
        }
        if let Some(env) = updates.env {
            config.env = env;
        }

        let updated = config.clone();
        self.save()?;
        Ok(Some(updated))
    }

    pub fn delete(&mut self, id: &str) -> Result<bool> {
        let len = self.configs.len();
        self.configs.retain(|c| c.id != id);
        if self.configs.len() < len {
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn save(&self) -> Result<()> {
        NexusConfig::save_mcp_servers(&self.configs)
    }
}

pub struct McpServerUpdate {
    pub name: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<std::collections::HashMap<String, String>>,
}
