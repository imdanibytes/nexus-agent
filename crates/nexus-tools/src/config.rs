use serde::{Deserialize, Serialize};

// ── Filesystem config ──

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

fn default_fs_enabled() -> bool {
    true
}

impl Default for FilesystemConfig {
    fn default() -> Self {
        Self {
            enabled: default_fs_enabled(),
            allowed_directories: Vec::new(),
        }
    }
}

// ── Fetch config ──

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

fn default_fetch_enabled() -> bool {
    true
}
fn default_max_response_bytes() -> usize {
    1_048_576 // 1 MB
}
fn default_timeout_secs() -> u32 {
    30
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
}
