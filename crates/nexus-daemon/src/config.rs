use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::agent_config::types::AgentEntry;
use crate::provider::types::Provider;

/// A workspace — a named folder the agent can access.
///
/// Workspaces form the basis of `allowed_directories` for the built-in
/// filesystem tools. Future extensions: LSP configuration, build system
/// settings, workspace-specific MCP servers/rules, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default = "chrono_now")]
    pub created_at: String,
    #[serde(default = "chrono_now")]
    pub updated_at: String,
}

fn chrono_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

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
    pub filesystem: FilesystemConfig,
    #[serde(default)]
    pub fetch: FetchConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Fetch tool configuration — controls the built-in HTTP fetch tool.
///
/// This is the **resolved** config used at runtime, produced by merging the
/// user config (`~/.nexus/nexus.json` → `fetch`) with corporate policy
/// (`/etc/nexus/policy.json` or platform equivalent). See [`FetchPolicy`] and
/// [`FetchConfig::apply_policy`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchConfig {
    /// Whether the fetch tool is available at all.
    #[serde(default = "default_fetch_enabled")]
    pub enabled: bool,
    /// If set, only these domains (and their subdomains) are allowed.
    /// `None` means no allowlist filtering (all domains permitted unless denied).
    #[serde(default)]
    pub allow_domains: Option<Vec<String>>,
    /// Domains that are always blocked, regardless of the allow list.
    #[serde(default)]
    pub deny_domains: Vec<String>,
    /// Maximum response body size in bytes (default 1 MB).
    #[serde(default = "default_max_response_bytes")]
    pub max_response_bytes: usize,
    /// HTTP request timeout in seconds (default 30).
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u32,
}

/// Corporate/admin fetch policy — loaded from a system-wide config file.
///
/// Deployed by IT via MDM / config management. The daemon reads this at
/// startup and merges it with the user's `FetchConfig`. Policy always wins:
///
/// - `deny_domains` are **added** to the user's deny list (cannot be removed).
/// - `allow_domains` **constrains** the user's allow list (user can only narrow, not widen).
/// - `enforce_allowlist: true` means the user cannot add domains beyond what policy permits.
/// - `enabled: Some(false)` force-disables the fetch tool entirely.
///
/// Platform-specific paths (checked in order, first found wins):
/// - macOS: `/Library/Application Support/Nexus/policy.json`
/// - Linux: `/etc/nexus/policy.json`
/// - Windows: `C:\ProgramData\Nexus\policy.json`
/// - All platforms (fallback): `~/.nexus/policy.json`
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FetchPolicy {
    /// If `Some(false)`, the fetch tool is force-disabled.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Policy-level allowlist. When `enforce_allowlist` is true, the user
    /// cannot fetch domains outside this list.
    #[serde(default)]
    pub allow_domains: Option<Vec<String>>,
    /// Domains that are always blocked. Merged (union) with user deny list.
    #[serde(default)]
    pub deny_domains: Vec<String>,
    /// When true, the policy `allow_domains` is the hard ceiling —
    /// the user can only narrow it, not add new domains.
    #[serde(default)]
    pub enforce_allowlist: bool,
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
fn default_fetch_enabled() -> bool {
    true
}
fn default_max_response_bytes() -> usize {
    1_048_576 // 1 MB
}
fn default_timeout_secs() -> u32 {
    30
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

impl Default for FilesystemConfig {
    fn default() -> Self {
        Self {
            enabled: default_fs_enabled(),
            allowed_directories: Vec::new(),
        }
    }
}

impl Default for FetchConfig {
    fn default() -> Self {
        Self {
            enabled: default_fetch_enabled(),
            allow_domains: None,
            deny_domains: Vec::new(),
            max_response_bytes: default_max_response_bytes(),
            timeout_secs: default_timeout_secs(),
        }
    }
}

impl FetchConfig {
    /// Merge a corporate policy into this user config. Policy always wins.
    ///
    /// Rules:
    /// 1. If policy says `enabled: false`, fetch is disabled regardless.
    /// 2. Policy `deny_domains` are unioned with user deny list.
    /// 3. If policy has `allow_domains` + `enforce_allowlist`:
    ///    - User allow list is intersected with policy (user can only narrow).
    ///    - If user has no allow list, policy allow list becomes the effective list.
    /// 4. If policy has `allow_domains` but NOT `enforce_allowlist`:
    ///    - Policy domains are added to the user's allow list.
    pub fn apply_policy(&mut self, policy: &FetchPolicy) {
        // 1. Force-disable
        if let Some(false) = policy.enabled {
            self.enabled = false;
        }

        // 2. Union deny lists (deduplicated)
        for domain in &policy.deny_domains {
            let lower = domain.to_lowercase();
            if !self.deny_domains.iter().any(|d| d.to_lowercase() == lower) {
                self.deny_domains.push(domain.clone());
            }
        }

        // 3/4. Merge allow lists
        if let Some(ref policy_allow) = policy.allow_domains {
            if policy.enforce_allowlist {
                // Policy is the hard ceiling
                match &self.allow_domains {
                    Some(user_allow) => {
                        // Intersect: keep only user domains that are also in policy
                        let narrowed: Vec<String> = user_allow
                            .iter()
                            .filter(|ud| {
                                let ud_lower = ud.to_lowercase();
                                policy_allow.iter().any(|pd| {
                                    let pd_lower = pd.to_lowercase();
                                    ud_lower == pd_lower
                                        || ud_lower.ends_with(&format!(".{pd_lower}"))
                                        || pd_lower.ends_with(&format!(".{ud_lower}"))
                                        || ud_lower == pd_lower
                                })
                            })
                            .cloned()
                            .collect();
                        self.allow_domains = Some(if narrowed.is_empty() {
                            policy_allow.clone()
                        } else {
                            narrowed
                        });
                    }
                    None => {
                        // User had no allow list — policy becomes the allow list
                        self.allow_domains = Some(policy_allow.clone());
                    }
                }
            } else {
                // Policy domains are suggestions — add to user list
                match &mut self.allow_domains {
                    Some(user_allow) => {
                        for domain in policy_allow {
                            let lower = domain.to_lowercase();
                            if !user_allow.iter().any(|d| d.to_lowercase() == lower) {
                                user_allow.push(domain.clone());
                            }
                        }
                    }
                    None => {
                        // User has no allow list — stays open (policy is non-enforcing)
                    }
                }
            }
        }
    }
}

impl Default for NexusConfig {
    fn default() -> Self {
        Self {
            api: ApiConfig::default(),
            server: ServerConfig::default(),
            agent: AgentConfig::default(),
            filesystem: FilesystemConfig::default(),
            fetch: FetchConfig::default(),
            workspaces: Vec::new(),
            providers: Vec::new(),
            agents: Vec::new(),
            active_agent_id: None,
        }
    }
}

impl NexusConfig {
    /// Compute the effective filesystem config by merging workspace paths
    /// with the explicit `allowed_directories`.
    ///
    /// Workspace paths always contribute to allowed_directories.  The user
    /// can add additional directories beyond workspaces via the config.
    pub fn effective_filesystem_config(&self) -> FilesystemConfig {
        let mut dirs = Vec::new();
        for ws in &self.workspaces {
            if !dirs.contains(&ws.path) {
                dirs.push(ws.path.clone());
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
    fn policy_force_disables_fetch() {
        let mut config = FetchConfig::default();
        assert!(config.enabled);

        let policy = FetchPolicy {
            enabled: Some(false),
            ..Default::default()
        };
        config.apply_policy(&policy);
        assert!(!config.enabled);
    }

    #[test]
    fn policy_enabled_none_does_not_override() {
        let mut config = FetchConfig::default();
        config.enabled = true;

        let policy = FetchPolicy {
            enabled: None,
            ..Default::default()
        };
        config.apply_policy(&policy);
        assert!(config.enabled);
    }

    #[test]
    fn policy_deny_domains_are_unioned() {
        let mut config = FetchConfig {
            deny_domains: vec!["user-blocked.com".into()],
            ..Default::default()
        };

        let policy = FetchPolicy {
            deny_domains: vec!["corp-blocked.com".into(), "user-blocked.com".into()],
            ..Default::default()
        };
        config.apply_policy(&policy);

        assert_eq!(config.deny_domains.len(), 2);
        assert!(config.deny_domains.iter().any(|d| d == "user-blocked.com"));
        assert!(config.deny_domains.iter().any(|d| d == "corp-blocked.com"));
    }

    #[test]
    fn policy_enforced_allowlist_constrains_user() {
        // User allows github.com and google.com
        // Policy only allows github.com with enforce
        let mut config = FetchConfig {
            allow_domains: Some(vec!["github.com".into(), "google.com".into()]),
            ..Default::default()
        };

        let policy = FetchPolicy {
            allow_domains: Some(vec!["github.com".into()]),
            enforce_allowlist: true,
            ..Default::default()
        };
        config.apply_policy(&policy);

        let allowed = config.allow_domains.unwrap();
        assert_eq!(allowed, vec!["github.com".to_string()]);
    }

    #[test]
    fn policy_enforced_allowlist_applies_when_user_has_none() {
        // User has no allow list (open access)
        // Policy enforces an allow list
        let mut config = FetchConfig {
            allow_domains: None,
            ..Default::default()
        };

        let policy = FetchPolicy {
            allow_domains: Some(vec!["internal.corp.com".into()]),
            enforce_allowlist: true,
            ..Default::default()
        };
        config.apply_policy(&policy);

        let allowed = config.allow_domains.unwrap();
        assert_eq!(allowed, vec!["internal.corp.com".to_string()]);
    }

    #[test]
    fn policy_non_enforced_allowlist_adds_to_user() {
        // User allows github.com
        // Policy suggests docs.rs (non-enforcing)
        let mut config = FetchConfig {
            allow_domains: Some(vec!["github.com".into()]),
            ..Default::default()
        };

        let policy = FetchPolicy {
            allow_domains: Some(vec!["docs.rs".into()]),
            enforce_allowlist: false,
            ..Default::default()
        };
        config.apply_policy(&policy);

        let allowed = config.allow_domains.unwrap();
        assert!(allowed.contains(&"github.com".to_string()));
        assert!(allowed.contains(&"docs.rs".to_string()));
    }

    #[test]
    fn policy_non_enforced_allowlist_keeps_user_open() {
        // User has no allow list (open access)
        // Policy suggests some domains but doesn't enforce
        let mut config = FetchConfig {
            allow_domains: None,
            ..Default::default()
        };

        let policy = FetchPolicy {
            allow_domains: Some(vec!["docs.rs".into()]),
            enforce_allowlist: false,
            ..Default::default()
        };
        config.apply_policy(&policy);

        // Should remain open (no allowlist)
        assert!(config.allow_domains.is_none());
    }

    #[test]
    fn policy_combined_deny_and_enforced_allow() {
        let mut config = FetchConfig::default();

        let policy = FetchPolicy {
            enabled: None,
            deny_domains: vec!["evil.com".into()],
            allow_domains: Some(vec!["github.com".into(), "docs.rs".into()]),
            enforce_allowlist: true,
        };
        config.apply_policy(&policy);

        assert!(config.enabled);
        assert_eq!(config.deny_domains, vec!["evil.com".to_string()]);
        let allowed = config.allow_domains.unwrap();
        assert_eq!(allowed.len(), 2);
        assert!(allowed.contains(&"github.com".to_string()));
        assert!(allowed.contains(&"docs.rs".to_string()));
    }

    #[test]
    fn empty_policy_changes_nothing() {
        let mut config = FetchConfig {
            enabled: true,
            allow_domains: Some(vec!["github.com".into()]),
            deny_domains: vec!["evil.com".into()],
            ..Default::default()
        };
        let original = config.clone();

        let policy = FetchPolicy::default();
        config.apply_policy(&policy);

        assert_eq!(config.enabled, original.enabled);
        assert_eq!(config.allow_domains, original.allow_domains);
        assert_eq!(config.deny_domains, original.deny_domains);
    }

    #[test]
    fn effective_fs_merges_workspaces_and_base_dirs() {
        let config = NexusConfig {
            workspaces: vec![
                Workspace {
                    id: "1".into(),
                    name: "Project A".into(),
                    path: "/home/user/project-a".into(),
                    created_at: String::new(),
                    updated_at: String::new(),
                },
                Workspace {
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
                    "/home/user/project-a".into(), // duplicate of workspace
                    "/tmp/scratch".into(),
                ],
            },
            ..Default::default()
        };

        let effective = config.effective_filesystem_config();
        assert!(effective.enabled);
        // Deduplicates: workspace paths first, then base dirs that aren't dupes
        assert_eq!(effective.allowed_directories.len(), 3);
        assert_eq!(effective.allowed_directories[0], "/home/user/project-a");
        assert_eq!(effective.allowed_directories[1], "/home/user/project-b");
        assert_eq!(effective.allowed_directories[2], "/tmp/scratch");
    }

    #[test]
    fn effective_fs_empty_workspaces_uses_base_only() {
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
            workspaces: vec![Workspace {
                id: "1".into(),
                name: "W".into(),
                path: "/w".into(),
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
