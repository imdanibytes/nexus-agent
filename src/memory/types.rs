use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// What content providers MUST supply when ingesting into memory.
/// Tools are responsible for gathering this — the memory system just
/// declares what it needs and rejects anything incomplete.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequiredMetadata {
    /// Where this came from: "tool:web_fetch", "tool:file_read", "user:manual", etc.
    pub source: String,
    /// What kind of content this is.
    pub content_type: ContentType,
    /// Human-readable title or summary hint. Providers should give their best
    /// one-liner — the memory system may refine it, but this seeds the summary.
    pub title: String,
    /// Original location: URL, file path, API endpoint, etc.
    pub uri: Option<String>,
    /// Programming language, if this is code.
    pub language: Option<String>,
    /// Freeform tags for categorical filtering.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// The kind of content being stored. Affects chunking strategy and retrieval ranking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Code,
    Documentation,
    ApiResponse,
    ConversationExcerpt,
    Configuration,
    ErrorLog,
    Other,
}

/// How much to trust this knowledge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Official docs, verified sources.
    High,
    /// Community answers, well-known patterns.
    Medium,
    /// Model-generated conclusions, debugging hunches.
    Low,
}

impl Default for Confidence {
    fn default() -> Self {
        Self::Medium
    }
}

/// What gets submitted for ingestion. Content + metadata. That's the contract.
#[derive(Debug, Clone)]
pub struct IngestRequest {
    pub content: String,
    pub metadata: RequiredMetadata,
    pub confidence: Confidence,
}

/// A single entry stored in episodic memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub summary: String,
    pub embedding: Vec<f32>,
    pub metadata: RequiredMetadata,
    pub confidence: Confidence,
    pub pinned: bool,
    pub created_at: DateTime<Utc>,
    pub last_accessed_at: DateTime<Utc>,
    pub access_count: u32,
}

/// What comes back from a search — the lightweight version for top-k injection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryHit {
    pub id: String,
    pub summary: String,
    pub relevance: f32,
    pub metadata: RequiredMetadata,
    pub confidence: Confidence,
    pub access_count: u32,
}
