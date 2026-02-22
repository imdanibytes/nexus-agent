use async_trait::async_trait;
use qdrant_client::qdrant::{
    vector_output::Vector, CountPointsBuilder, CreateCollectionBuilder, DeletePointsBuilder,
    Distance, GetPointsBuilder, PointStruct, PointsIdsList, ScrollPointsBuilder,
    SearchPointsBuilder, SetPayloadPointsBuilder, UpsertPointsBuilder, VectorParamsBuilder,
    Value as QdrantValue,
};
use qdrant_client::{Payload, Qdrant};
use serde_json::json;

use crate::memory::error::MemoryError;
use crate::memory::store::{EvictionCandidate, MemoryStore};
use crate::memory::types::{
    Confidence, ContentType, MemoryEntry, MemoryHit, RequiredMetadata,
};

/// Qdrant-backed memory store. Vectors live on disk, metadata in payload.
pub struct QdrantStore {
    client: Qdrant,
    collection: String,
    dimensions: usize,
}

impl QdrantStore {
    /// Connect to a Qdrant instance and ensure the collection exists.
    pub async fn new(url: &str, collection: &str, dimensions: usize) -> Result<Self, MemoryError> {
        let client = Qdrant::from_url(url)
            .build()
            .map_err(|e| MemoryError::Store(format!("failed to connect to qdrant: {e}")))?;

        let store = Self {
            client,
            collection: collection.to_string(),
            dimensions,
        };

        store.ensure_collection().await?;
        Ok(store)
    }

    async fn ensure_collection(&self) -> Result<(), MemoryError> {
        let exists = self
            .client
            .collection_exists(&self.collection)
            .await
            .map_err(|e| MemoryError::Store(format!("failed to check collection: {e}")))?;

        if !exists {
            self.client
                .create_collection(
                    CreateCollectionBuilder::new(&self.collection)
                        .vectors_config(VectorParamsBuilder::new(
                            self.dimensions as u64,
                            Distance::Cosine,
                        )),
                )
                .await
                .map_err(|e| MemoryError::Store(format!("failed to create collection: {e}")))?;
        }

        Ok(())
    }
}

#[async_trait]
impl MemoryStore for QdrantStore {
    async fn upsert(&self, entry: &MemoryEntry) -> Result<String, MemoryError> {
        let payload: Payload = json!({
            "content": entry.content,
            "summary": entry.summary,
            "source": entry.metadata.source,
            "content_type": serde_json::to_string(&entry.metadata.content_type)
                .unwrap_or_default().trim_matches('"'),
            "title": entry.metadata.title,
            "uri": entry.metadata.uri,
            "language": entry.metadata.language,
            "tags": entry.metadata.tags,
            "confidence": serde_json::to_string(&entry.confidence)
                .unwrap_or_default().trim_matches('"'),
            "pinned": entry.pinned,
            "created_at": entry.created_at.to_rfc3339(),
            "last_accessed_at": entry.last_accessed_at.to_rfc3339(),
            "access_count": entry.access_count,
        })
        .try_into()
        .map_err(|e| MemoryError::Serialization(format!("payload: {e}")))?;

        let point = PointStruct::new(&*entry.id, entry.embedding.clone(), payload);

        self.client
            .upsert_points(
                UpsertPointsBuilder::new(&self.collection, vec![point]).wait(true),
            )
            .await
            .map_err(|e| MemoryError::Store(format!("upsert failed: {e}")))?;

        Ok(entry.id.clone())
    }

    async fn search(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<MemoryHit>, MemoryError> {
        let results = self
            .client
            .search_points(
                SearchPointsBuilder::new(&self.collection, query_embedding.to_vec(), limit as u64)
                    .with_payload(true),
            )
            .await
            .map_err(|e| MemoryError::Store(format!("search failed: {e}")))?;

        let mut hits = Vec::with_capacity(results.result.len());
        for point in results.result {
            let p = &point.payload;
            hits.push(MemoryHit {
                id: point_id_to_string(&point.id),
                summary: extract_string(p, "summary"),
                relevance: point.score,
                metadata: RequiredMetadata {
                    source: extract_string(p, "source"),
                    content_type: parse_content_type(&extract_string(p, "content_type")),
                    title: extract_string(p, "title"),
                    uri: extract_option_string(p, "uri"),
                    language: extract_option_string(p, "language"),
                    tags: extract_string_list(p, "tags"),
                },
                confidence: parse_confidence(&extract_string(p, "confidence")),
                access_count: extract_u32(p, "access_count"),
            });
        }

        Ok(hits)
    }

    async fn get(&self, id: &str) -> Result<Option<MemoryEntry>, MemoryError> {
        let result = self
            .client
            .get_points(
                GetPointsBuilder::new(&self.collection, vec![id.to_string().into()])
                    .with_payload(true)
                    .with_vectors(true),
            )
            .await
            .map_err(|e| MemoryError::Store(format!("get failed: {e}")))?;

        let Some(point) = result.result.into_iter().next() else {
            return Ok(None);
        };

        let p = &point.payload;
        let embedding = point
            .vectors
            .and_then(|v| v.get_vector())
            .and_then(|v| match v {
                Vector::Dense(dv) => Some(dv.data),
                _ => None,
            })
            .unwrap_or_default();

        Ok(Some(MemoryEntry {
            id: point_id_to_string(&point.id),
            content: extract_string(p, "content"),
            summary: extract_string(p, "summary"),
            embedding,
            metadata: RequiredMetadata {
                source: extract_string(p, "source"),
                content_type: parse_content_type(&extract_string(p, "content_type")),
                title: extract_string(p, "title"),
                uri: extract_option_string(p, "uri"),
                language: extract_option_string(p, "language"),
                tags: extract_string_list(p, "tags"),
            },
            confidence: parse_confidence(&extract_string(p, "confidence")),
            pinned: extract_bool(p, "pinned"),
            created_at: parse_datetime(&extract_string(p, "created_at")),
            last_accessed_at: parse_datetime(&extract_string(p, "last_accessed_at")),
            access_count: extract_u32(p, "access_count"),
        }))
    }

    async fn delete(&self, id: &str) -> Result<(), MemoryError> {
        self.client
            .delete_points(
                DeletePointsBuilder::new(&self.collection)
                    .points(PointsIdsList {
                        ids: vec![id.to_string().into()],
                    })
                    .wait(true),
            )
            .await
            .map_err(|e| MemoryError::Store(format!("delete failed: {e}")))?;

        Ok(())
    }

    async fn touch(&self, id: &str) -> Result<(), MemoryError> {
        // We need to read-then-write for access_count increment.
        // Qdrant doesn't have atomic increment, so we get the current value first.
        let current = self.get(id).await?;
        let Some(entry) = current else {
            return Err(MemoryError::NotFound(id.to_string()));
        };

        let now = chrono::Utc::now();
        let payload: Payload = json!({
            "last_accessed_at": now.to_rfc3339(),
            "access_count": entry.access_count + 1,
        })
        .try_into()
        .map_err(|e| MemoryError::Serialization(format!("touch payload: {e}")))?;

        self.client
            .set_payload(
                SetPayloadPointsBuilder::new(&self.collection, payload)
                    .points_selector(PointsIdsList {
                        ids: vec![id.to_string().into()],
                    })
                    .wait(true),
            )
            .await
            .map_err(|e| MemoryError::Store(format!("touch failed: {e}")))?;

        Ok(())
    }

    async fn list_candidates(&self) -> Result<Vec<EvictionCandidate>, MemoryError> {
        let mut candidates = Vec::new();
        let mut offset = None;

        loop {
            let mut builder = ScrollPointsBuilder::new(&self.collection)
                .limit(100)
                .with_payload(true)
                .with_vectors(false);

            if let Some(off) = offset {
                builder = builder.offset(off);
            }

            let result = self
                .client
                .scroll(builder)
                .await
                .map_err(|e| MemoryError::Store(format!("scroll failed: {e}")))?;

            for point in &result.result {
                let p = &point.payload;
                candidates.push(EvictionCandidate {
                    id: point_id_to_string(&point.id),
                    last_accessed_at: parse_datetime(&extract_string(p, "last_accessed_at")),
                    access_count: extract_u32(p, "access_count"),
                    confidence: parse_confidence(&extract_string(p, "confidence")),
                    pinned: extract_bool(p, "pinned"),
                });
            }

            match result.next_page_offset {
                Some(next) => offset = Some(next),
                None => break,
            }
        }

        Ok(candidates)
    }

    async fn count(&self) -> Result<usize, MemoryError> {
        let result = self
            .client
            .count(CountPointsBuilder::new(&self.collection).exact(true))
            .await
            .map_err(|e| MemoryError::Store(format!("count failed: {e}")))?;

        Ok(result.result.map(|r| r.count as usize).unwrap_or(0))
    }

    async fn set_pinned(&self, id: &str, pinned: bool) -> Result<(), MemoryError> {
        let payload: Payload = json!({ "pinned": pinned })
            .try_into()
            .map_err(|e| MemoryError::Serialization(format!("pin payload: {e}")))?;

        self.client
            .set_payload(
                SetPayloadPointsBuilder::new(&self.collection, payload)
                    .points_selector(PointsIdsList {
                        ids: vec![id.to_string().into()],
                    })
                    .wait(true),
            )
            .await
            .map_err(|e| MemoryError::Store(format!("set_pinned failed: {e}")))?;

        Ok(())
    }
}

// --- Payload extraction helpers ---

fn point_id_to_string(id: &Option<qdrant_client::qdrant::PointId>) -> String {
    match id {
        Some(pid) => match &pid.point_id_options {
            Some(qdrant_client::qdrant::point_id::PointIdOptions::Uuid(s)) => s.clone(),
            Some(qdrant_client::qdrant::point_id::PointIdOptions::Num(n)) => n.to_string(),
            None => String::new(),
        },
        None => String::new(),
    }
}

fn extract_string(payload: &std::collections::HashMap<String, QdrantValue>, key: &str) -> String {
    payload
        .get(key)
        .and_then(|v| v.as_str())
        .cloned()
        .unwrap_or_default()
}

fn extract_option_string(
    payload: &std::collections::HashMap<String, QdrantValue>,
    key: &str,
) -> Option<String> {
    payload
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

fn extract_bool(payload: &std::collections::HashMap<String, QdrantValue>, key: &str) -> bool {
    payload
        .get(key)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn extract_u32(payload: &std::collections::HashMap<String, QdrantValue>, key: &str) -> u32 {
    payload
        .get(key)
        .and_then(|v| v.as_integer())
        .unwrap_or(0) as u32
}

fn extract_string_list(
    payload: &std::collections::HashMap<String, QdrantValue>,
    key: &str,
) -> Vec<String> {
    payload
        .get(key)
        .and_then(|v| v.as_list())
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_content_type(s: &str) -> ContentType {
    match s {
        "code" => ContentType::Code,
        "documentation" => ContentType::Documentation,
        "api_response" => ContentType::ApiResponse,
        "conversation_excerpt" => ContentType::ConversationExcerpt,
        "configuration" => ContentType::Configuration,
        "error_log" => ContentType::ErrorLog,
        _ => ContentType::Other,
    }
}

fn parse_confidence(s: &str) -> Confidence {
    match s {
        "high" => Confidence::High,
        "low" => Confidence::Low,
        _ => Confidence::Medium,
    }
}

fn parse_datetime(s: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now())
}
