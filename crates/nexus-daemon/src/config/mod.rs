mod fetch;

pub use fetch::*;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::agent_config::types::AgentEntry;
use crate::provider::types::Provider;

/// A project — a single codebase root the agent can access.
///
/// Projects form the basis of `allowed_directories` for the built-in
/// filesystem tools. Future extensions: LSP configuration, build system
/// settings, project-specific MCP servers/rules, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default = "chrono_now")]
    pub created_at: String,
    #[serde(default = "chrono_now")]
    pub updated_at: String,
}

/// A workspace — a logical grouping of projects by intent.
///
/// Workspaces give the agent mission context ("what am I working on and why")
/// while projects provide the concrete codebase paths. A workspace can span
/// multiple unrelated repos/directories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub project_ids: Vec<String>,
    #[serde(default = "chrono_now")]
    pub created_at: String,
    #[serde(default = "chrono_now")]
    pub updated_at: String,
}

fn chrono_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Main nexus configuration — persisted to ~/.nexus/nexus.json
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NexusConfig {
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub filesystem: FilesystemConfig,
    #[serde(default)]
    pub fetch: FetchConfig,
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub workspaces: Vec<Workspace>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentConfig {
    pub system_prompt: Option<String>,
}

/// Filesystem tool configuration — controls the built-in filesystem tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesystemConfig {
    /// Whether filesystem tools are available at all.
    #[serde(default = "default_fs_enabled")]
    pub enabled: bool,
    /// Directories the agent is allowed to access.  Paths outside these
    /// are rejected.  Empty vec = no filesystem access.
    #[serde(default)]
    pub allowed_directories: Vec<String>,
}

/// MCP server configuration — persisted to ~/.nexus/mcp.json
///
/// Two transport modes:
/// - **Stdio** (default): `command` + `args` + `env` spawn a child process.
/// - **HTTP**: `url` points to a streamable-HTTP MCP endpoint; `headers` are
///   sent with every request (e.g. auth tokens). `command` is ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// If present, connect via streamable HTTP instead of stdio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Custom HTTP headers (e.g. Authorization). Only used with `url`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

fn default_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}
fn default_max_tokens() -> u32 {
    8192
}
fn default_fs_enabled() -> bool {
    true
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

// AgentConfig derives Default: all fields are Option<_> → None.
// FilesystemConfig can't derive: `enabled` defaults to true, not false.

impl Default for FilesystemConfig {
    fn default() -> Self {
        Self {
            enabled: default_fs_enabled(),
            allowed_directories: Vec::new(),
        }
    }
}


impl NexusConfig {
    /// Compute the effective filesystem config by merging project paths
    /// with the explicit `allowed_directories`.
    ///
    /// Project paths always contribute to allowed_directories.  The user
    /// can add additional directories beyond projects via the config.
    pub fn effective_filesystem_config(&self) -> FilesystemConfig {
        let mut dirs = Vec::new();
        for proj in &self.projects {
            if !dirs.contains(&proj.path) {
                dirs.push(proj.path.clone());
            }
        }
        for dir in &self.filesystem.allowed_directories {
            if !dirs.contains(dir) {
                dirs.push(dir.clone());
            }
        }
        FilesystemConfig {
            enabled: self.filesystem.enabled,
            allowed_directories: dirs,
        }
    }

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

            // Migrate old format: "workspaces" with path-bearing entries → "projects"
            let content = Self::migrate_workspaces_to_projects(content)?;

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

    /// Detect old config format where "workspaces" array contains items with
    /// a "path" field (these are really projects). Migrate by moving them to
    /// "projects" and resetting "workspaces" to an empty array.
    fn migrate_workspaces_to_projects(content: String) -> Result<String> {
        let mut value: serde_json::Value = serde_json::from_str(&content)?;

        let needs_migration = value
            .get("workspaces")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().any(|item| item.get("path").is_some()))
            .unwrap_or(false);

        // Also check: if "projects" key already exists, no migration needed
        let has_projects = value.get("projects").is_some();

        if needs_migration && !has_projects {
            tracing::info!("Migrating old workspaces (with paths) → projects");
            let old_workspaces = value["workspaces"].take();
            value["projects"] = old_workspaces;
            value["workspaces"] = serde_json::json!([]);

            // Write migrated config back to disk
            let migrated = serde_json::to_string_pretty(&value)?;
            let path = Self::config_path();
            fs::write(&path, &migrated)
                .with_context(|| "Failed to write migrated config")?;

            Ok(migrated)
        } else {
            Ok(content)
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

    pub fn save_mcp_servers(servers: &[McpServerConfig]) -> Result<()> {
        let path = Self::mcp_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(servers)?;
        fs::write(&path, content)
            .with_context(|| format!("Failed to write MCP config to {}", path.display()))?;
        Ok(())
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

    /// Load the corporate/admin fetch policy from platform-specific paths.
    ///
    /// Checks these locations in order (first found wins):
    /// - macOS: `/Library/Application Support/Nexus/policy.json`
    /// - Linux: `/etc/nexus/policy.json`
    /// - Windows: `C:\ProgramData\Nexus\policy.json`
    /// - All platforms: `~/.nexus/policy.json` (fallback / dev override)
    ///
    /// Returns `FetchPolicy::default()` if no policy file exists.
    pub fn load_fetch_policy() -> FetchPolicy {
        let candidates = Self::policy_paths();

        for path in &candidates {
            if path.exists() {
                match fs::read_to_string(path) {
                    Ok(content) => {
                        // The policy file wraps FetchPolicy under a "fetch" key
                        // to allow future expansion (e.g. "mcp" policy, etc.)
                        #[derive(Deserialize)]
                        struct PolicyFile {
                            #[serde(default)]
                            fetch: FetchPolicy,
                        }

                        match serde_json::from_str::<PolicyFile>(&content) {
                            Ok(pf) => {
                                tracing::info!(
                                    "Loaded fetch policy from {}",
                                    path.display()
                                );
                                return pf.fetch;
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to parse policy file at {}: {}",
                                    path.display(),
                                    e
                                );
                                // Don't fall through to next file — a malformed
                                // policy file should fail closed (no fetch).
                                return FetchPolicy {
                                    enabled: Some(false),
                                    ..Default::default()
                                };
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to read policy file at {}: {}",
                            path.display(),
                            e
                        );
                    }
                }
            }
        }

        tracing::debug!("No fetch policy file found, using defaults");
        FetchPolicy::default()
    }

    /// Platform-specific policy file search paths.
    #[allow(clippy::vec_init_then_push)] // cfg-gated pushes can't use vec![]
    fn policy_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        #[cfg(target_os = "macos")]
        paths.push(PathBuf::from("/Library/Application Support/Nexus/policy.json"));

        #[cfg(target_os = "linux")]
        paths.push(PathBuf::from("/etc/nexus/policy.json"));

        #[cfg(target_os = "windows")]
        {
            if let Ok(pd) = std::env::var("ProgramData") {
                paths.push(PathBuf::from(pd).join("Nexus").join("policy.json"));
            } else {
                paths.push(PathBuf::from("C:\\ProgramData\\Nexus\\policy.json"));
            }
        }

        // Fallback: user-level policy (useful for dev/testing)
        paths.push(Self::nexus_dir().join("policy.json"));

        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_fs_merges_projects_and_base_dirs() {
        let config = NexusConfig {
            projects: vec![
                Project {
                    id: "1".into(),
                    name: "Project A".into(),
                    path: "/home/user/project-a".into(),
                    created_at: String::new(),
                    updated_at: String::new(),
                },
                Project {
                    id: "2".into(),
                    name: "Project B".into(),
                    path: "/home/user/project-b".into(),
                    created_at: String::new(),
                    updated_at: String::new(),
                },
            ],
            filesystem: FilesystemConfig {
                enabled: true,
                allowed_directories: vec![
                    "/home/user/project-a".into(), // duplicate of project
                    "/tmp/scratch".into(),
                ],
            },
            ..Default::default()
        };

        let effective = config.effective_filesystem_config();
        assert!(effective.enabled);
        // Deduplicates: project paths first, then base dirs that aren't dupes
        assert_eq!(effective.allowed_directories.len(), 3);
        assert_eq!(effective.allowed_directories[0], "/home/user/project-a");
        assert_eq!(effective.allowed_directories[1], "/home/user/project-b");
        assert_eq!(effective.allowed_directories[2], "/tmp/scratch");
    }

    #[test]
    fn effective_fs_empty_projects_uses_base_only() {
        let config = NexusConfig {
            filesystem: FilesystemConfig {
                enabled: true,
                allowed_directories: vec!["/Volumes/work".into()],
            },
            ..Default::default()
        };
        let effective = config.effective_filesystem_config();
        assert_eq!(effective.allowed_directories, vec!["/Volumes/work"]);
    }

    #[test]
    fn effective_fs_propagates_disabled() {
        let config = NexusConfig {
            projects: vec![Project {
                id: "1".into(),
                name: "P".into(),
                path: "/p".into(),
                created_at: String::new(),
                updated_at: String::new(),
            }],
            filesystem: FilesystemConfig {
                enabled: false,
                allowed_directories: vec![],
            },
            ..Default::default()
        };
        let effective = config.effective_filesystem_config();
        assert!(!effective.enabled);
        // Still merges paths even when disabled (tool_definitions will return empty)
        assert_eq!(effective.allowed_directories.len(), 1);
    }
}
