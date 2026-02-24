use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::agent_config::types::AgentEntry;
use crate::provider::types::Provider;

/// Main nexus configuration — persisted to ~/.nexus/nexus.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusConfig {
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub providers: Vec<Provider>,
    #[serde(default)]
    pub agents: Vec<AgentEntry>,
    #[serde(default)]
    pub active_agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub system_prompt: Option<String>,
}

/// MCP server configuration — persisted to ~/.nexus/mcp.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

fn default_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}
fn default_max_tokens() -> u32 {
    8192
}
fn default_host() -> String {
    "127.0.0.1".to_string()
}
fn default_port() -> u16 {
    9600
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            max_tokens: default_max_tokens(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            system_prompt: None,
        }
    }
}

impl Default for NexusConfig {
    fn default() -> Self {
        Self {
            api: ApiConfig::default(),
            server: ServerConfig::default(),
            agent: AgentConfig::default(),
            providers: Vec::new(),
            agents: Vec::new(),
            active_agent_id: None,
        }
    }
}

impl NexusConfig {
    pub fn nexus_dir() -> PathBuf {
        dirs::home_dir()
            .expect("Could not determine home directory")
            .join(".nexus")
    }

    fn config_path() -> PathBuf {
        Self::nexus_dir().join("nexus.json")
    }

    fn mcp_path() -> PathBuf {
        Self::nexus_dir().join("mcp.json")
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        let dir = Self::nexus_dir();
        fs::create_dir_all(&dir)?;

        if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config from {}", path.display()))?;
            let config: NexusConfig = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse config at {}", path.display()))?;
            Ok(config)
        } else {
            let config = Self::default();
            config.save()?;
            tracing::info!("Created default config at {}", path.display());
            Ok(config)
        }
    }

    pub fn load_mcp_servers() -> Result<Vec<McpServerConfig>> {
        let path = Self::mcp_path();
        if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read MCP config from {}", path.display()))?;
            let servers: Vec<McpServerConfig> = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse MCP config at {}", path.display()))?;
            Ok(servers)
        } else {
            // Write empty array as scaffold
            fs::write(&path, "[]")?;
            Ok(Vec::new())
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;
        Ok(())
    }
}
