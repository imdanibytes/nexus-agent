use chrono::{DateTime, Utc};

use super::store::EvictionCandidate;
use super::types::Confidence;

/// Configuration for the decay scoring function.
#[derive(Debug, Clone)]
pub struct DecayConfig {
    /// Half-life in days. After this many days without access, the recency
    /// weight drops to 50%. Default: 14 days.
    pub half_life_days: f64,
    /// Score below which unpinned entries become eligible for eviction.
    pub eviction_threshold: f64,
    /// Maximum entries before size-based eviction kicks in.
    pub max_entries: usize,
    /// Low-confidence entries get evicted faster — their half-life is
    /// multiplied by this factor (< 1.0 means shorter half-life). Default: 0.5.
    pub low_confidence_decay_factor: f64,
}

impl Default for DecayConfig {
    fn default() -> Self {
        Self {
            half_life_days: 14.0,
            eviction_threshold: 0.5,
            max_entries: 10_000,
            low_confidence_decay_factor: 0.5,
        }
    }
}

/// Compute the decay score for an entry.
///
/// `score = access_count * e^(-age_days * ln(2) / half_life)`
///
/// Higher score = more valuable = keep longer.
pub fn decay_score(
    access_count: u32,
    last_accessed_at: DateTime<Utc>,
    confidence: Confidence,
    now: DateTime<Utc>,
    config: &DecayConfig,
) -> f64 {
    let age = now.signed_duration_since(last_accessed_at);
    let age_days = age.num_seconds() as f64 / 86_400.0;

    // Low-confidence entries decay faster
    let effective_half_life = match confidence {
        Confidence::High => config.half_life_days,
        Confidence::Medium => config.half_life_days,
        Confidence::Low => config.half_life_days * config.low_confidence_decay_factor,
    };

    let decay_rate = (2.0_f64).ln() / effective_half_life;
    let recency_weight = (-decay_rate * age_days).exp();

    // Ensure at least 1 access so brand new entries don't score zero
    let count = (access_count.max(1)) as f64;

    count * recency_weight
}

/// Given a list of candidates, return IDs that should be evicted.
/// Respects pinned status, decay threshold, and max entry count.
pub fn select_evictions(
    candidates: &[EvictionCandidate],
    total_count: usize,
    now: DateTime<Utc>,
    config: &DecayConfig,
) -> Vec<String> {
    // Score all unpinned candidates
    let mut scored: Vec<(String, f64)> = candidates
        .iter()
        .filter(|c| !c.pinned)
        .map(|c| {
            let score = decay_score(c.access_count, c.last_accessed_at, c.confidence, now, config);
            (c.id.clone(), score)
        })
        .collect();

    // Sort ascending by score (lowest = most evictable)
    scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut to_evict = Vec::new();

    // Phase 1: evict everything below threshold
    for (id, score) in &scored {
        if *score < config.eviction_threshold {
            to_evict.push(id.clone());
        }
    }

    // Phase 2: if still over budget, evict lowest-scored until under max_entries
    let remaining = total_count - to_evict.len();
    if remaining > config.max_entries {
        let need_to_cut = remaining - config.max_entries;
        let already_evicted: std::collections::HashSet<String> =
            to_evict.iter().cloned().collect();
        let extras: Vec<String> = scored
            .iter()
            .filter(|(id, _)| !already_evicted.contains(id))
            .take(need_to_cut)
            .map(|(id, _)| id.clone())
            .collect();
        to_evict.extend(extras);
    }

    to_evict
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> DecayConfig {
        DecayConfig {
            half_life_days: 14.0,
            eviction_threshold: 0.5,
            max_entries: 100,
            low_confidence_decay_factor: 0.5,
        }
    }

    #[test]
    fn fresh_entry_scores_high() {
        let now = Utc::now();
        let score = decay_score(5, now, Confidence::Medium, now, &config());
        assert!(score >= 5.0, "fresh entry with 5 accesses should score ~5.0, got {score}");
    }

    #[test]
    fn old_entry_decays() {
        let now = Utc::now();
        let old = now - chrono::Duration::days(30);
        let score = decay_score(5, old, Confidence::Medium, now, &config());
        assert!(score < 2.0, "30-day-old entry should have decayed significantly, got {score}");
    }

    #[test]
    fn low_confidence_decays_faster() {
        let now = Utc::now();
        let accessed = now - chrono::Duration::days(10);
        let high = decay_score(3, accessed, Confidence::High, now, &config());
        let low = decay_score(3, accessed, Confidence::Low, now, &config());
        assert!(low < high, "low confidence should decay faster: high={high}, low={low}");
    }

    #[test]
    fn pinned_entries_not_evicted() {
        let now = Utc::now();
        let old = now - chrono::Duration::days(60);
        let candidates = vec![EvictionCandidate {
            id: "pinned-1".into(),
            last_accessed_at: old,
            access_count: 1,
            confidence: Confidence::Low,
            pinned: true,
        }];

        let evictions = select_evictions(&candidates, 1, now, &config());
        assert!(evictions.is_empty(), "pinned entries must never be evicted");
    }

    #[test]
    fn stale_entries_evicted() {
        let now = Utc::now();
        let old = now - chrono::Duration::days(60);
        let candidates = vec![
            EvictionCandidate {
                id: "stale-1".into(),
                last_accessed_at: old,
                access_count: 1,
                confidence: Confidence::Low,
                pinned: false,
            },
            EvictionCandidate {
                id: "fresh-1".into(),
                last_accessed_at: now,
                access_count: 10,
                confidence: Confidence::High,
                pinned: false,
            },
        ];

        let evictions = select_evictions(&candidates, 2, now, &config());
        assert!(evictions.contains(&"stale-1".to_string()));
        assert!(!evictions.contains(&"fresh-1".to_string()));
    }
}
