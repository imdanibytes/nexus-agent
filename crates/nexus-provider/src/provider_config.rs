use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    Anthropic,
    Bedrock,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub provider_type: ProviderType,
    /// Custom API endpoint (Anthropic-compatible base URL)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// API key for Anthropic
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// AWS region for Bedrock
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aws_region: Option<String>,
    /// AWS CLI profile name (e.g. "default", "my-bedrock-profile")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aws_profile: Option<String>,
    #[serde(default = "chrono::Utc::now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "chrono::Utc::now")]
    pub updated_at: DateTime<Utc>,
}

/// Safe for frontend — no secrets
#[derive(Debug, Clone, Serialize)]
pub struct ProviderPublic {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub provider_type: ProviderType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    pub has_api_key: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aws_region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aws_profile: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&Provider> for ProviderPublic {
    fn from(p: &Provider) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            provider_type: p.provider_type.clone(),
            endpoint: p.endpoint.clone(),
            has_api_key: p.api_key.is_some(),
            aws_region: p.aws_region.clone(),
            aws_profile: p.aws_profile.clone(),
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}
