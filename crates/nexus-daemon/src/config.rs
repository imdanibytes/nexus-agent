use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct NexusConfig {
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
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
            mcp_servers: Vec::new(),
        }
    }
}

impl NexusConfig {
    pub fn nexus_dir() -> PathBuf {
        dirs::home_dir()
            .expect("Could not determine home directory")
            .join(".nexus")
    }

    pub fn config_path() -> PathBuf {
        Self::nexus_dir().join("config.toml")
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();

        if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config from {}", path.display()))?;
            let config: NexusConfig = toml::from_str(&content)
                .with_context(|| format!("Failed to parse config at {}", path.display()))?;
            Ok(config)
        } else {
            let config = Self::default();
            // Create directory and write default config
            let dir = Self::nexus_dir();
            fs::create_dir_all(&dir)?;
            let default_toml = r#"# Nexus configuration
# API key is read from ANTHROPIC_API_KEY environment variable

[api]
model = "claude-sonnet-4-20250514"
max_tokens = 8192

[server]
host = "127.0.0.1"
port = 9600

# [agent]
# system_prompt = "You are a helpful coding assistant."

# [[mcp_servers]]
# id = "filesystem"
# command = "npx"
# args = ["-y", "@modelcontextprotocol/server-filesystem", "/Users/you/projects"]
"#;
            fs::write(&path, default_toml)?;
            tracing::info!("Created default config at {}", path.display());
            Ok(config)
        }
    }
}
