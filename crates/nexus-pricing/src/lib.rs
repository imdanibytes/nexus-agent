/// Model pricing lookup table and cost calculation.
///
/// Prices are in USD per million tokens (MTok), sourced from
/// https://docs.anthropic.com/en/docs/about-claude/pricing
///
/// For unknown models, falls back to the cheapest tier to avoid overstating costs.
/// Per-token pricing for a model.
#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    /// Cost per million input tokens (USD).
    pub input_per_mtok: f64,
    /// Cost per million output tokens (USD).
    pub output_per_mtok: f64,
    /// Context window size in tokens.
    pub context_window: u32,
}

/// Lookup pricing for a model by its ID string.
///
/// Model IDs can be in various formats:
/// - Anthropic direct: `claude-opus-4-6`, `claude-sonnet-4-20250514`
/// - Bedrock: `us.anthropic.claude-sonnet-4-20250514-v1:0`, `anthropic.claude-opus-4-6-v1`
///
/// We match by searching for known substrings in the model ID.
pub fn lookup(model: &str) -> ModelPricing {
    let m = model.to_lowercase();

    // Match from most specific to least specific.
    // Order matters — check versioned names before generic family names.

    // ── Opus family ──
    if m.contains("opus-4-6") || m.contains("opus-4.6") {
        return OPUS_4_6;
    }
    if m.contains("opus-4-5") || m.contains("opus-4.5") {
        return OPUS_4_5;
    }
    if m.contains("opus-4-1") || m.contains("opus-4.1") {
        return OPUS_4_1;
    }
    if m.contains("opus-4-0") || m.contains("opus-4.0") || m.contains("opus-4-2") {
        // Opus 4 and 4.2 share the same pricing
        return OPUS_4;
    }
    if m.contains("opus-3") {
        return OPUS_3;
    }
    // Generic "opus" without version — assume latest
    if m.contains("opus") {
        return OPUS_4_6;
    }

    // ── Sonnet family ──
    if m.contains("sonnet-4-6") || m.contains("sonnet-4.6") {
        return SONNET_4_6;
    }
    if m.contains("sonnet-4-5") || m.contains("sonnet-4.5") {
        return SONNET_4_5;
    }
    if m.contains("sonnet-4-0") || m.contains("sonnet-4.0") || m.contains("sonnet-4-2") {
        return SONNET_4;
    }
    if m.contains("sonnet-3") {
        return SONNET_3_7;
    }
    // Generic "sonnet" without version — assume latest
    if m.contains("sonnet") {
        return SONNET_4_6;
    }

    // ── Haiku family ──
    if m.contains("haiku-4-5") || m.contains("haiku-4.5") {
        return HAIKU_4_5;
    }
    if m.contains("haiku-3-5") || m.contains("haiku-3.5") {
        return HAIKU_3_5;
    }
    if m.contains("haiku-3") {
        return HAIKU_3;
    }
    // Generic "haiku" without version — assume latest
    if m.contains("haiku") {
        return HAIKU_4_5;
    }

    // ── Unknown model — fall back to cheapest (Haiku 3) ──
    tracing::warn!(model, "Unknown model for pricing, using Haiku 3 fallback");
    FALLBACK
}

/// Calculate cost in USD for a given token count (no caching).
pub fn calculate_cost(model: &str, input_tokens: u32, output_tokens: u32) -> f64 {
    calculate_cost_with_cache(model, input_tokens, 0, 0, output_tokens)
}

/// Calculate cost in USD with prompt caching breakdown.
///
/// When prompt caching is active, the API returns three categories of input tokens:
/// - `input_tokens`: uncached tokens (after the last cache breakpoint) at base price
/// - `cache_creation_input_tokens`: tokens written to cache at 1.25x base price
/// - `cache_read_input_tokens`: tokens read from cache at 0.1x base price
///
/// Reference: https://platform.claude.com/docs/en/build-with-claude/prompt-caching#pricing
pub fn calculate_cost_with_cache(
    model: &str,
    input_tokens: u32,
    cache_creation_input_tokens: u32,
    cache_read_input_tokens: u32,
    output_tokens: u32,
) -> f64 {
    let pricing = lookup(model);
    let uncached_cost = (input_tokens as f64 / 1_000_000.0) * pricing.input_per_mtok;
    let cache_write_cost =
        (cache_creation_input_tokens as f64 / 1_000_000.0) * pricing.input_per_mtok * 1.25;
    let cache_read_cost =
        (cache_read_input_tokens as f64 / 1_000_000.0) * pricing.input_per_mtok * 0.1;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * pricing.output_per_mtok;
    uncached_cost + cache_write_cost + cache_read_cost + output_cost
}

/// Get the context window for a model.
pub fn context_window(model: &str) -> u32 {
    lookup(model).context_window
}

// ── Pricing constants ──
// Source: https://docs.anthropic.com/en/docs/about-claude/pricing

const OPUS_4_6: ModelPricing = ModelPricing {
    input_per_mtok: 5.0,
    output_per_mtok: 25.0,
    context_window: 200_000,
};

const OPUS_4_5: ModelPricing = ModelPricing {
    input_per_mtok: 5.0,
    output_per_mtok: 25.0,
    context_window: 200_000,
};

const OPUS_4_1: ModelPricing = ModelPricing {
    input_per_mtok: 15.0,
    output_per_mtok: 75.0,
    context_window: 200_000,
};

const OPUS_4: ModelPricing = ModelPricing {
    input_per_mtok: 15.0,
    output_per_mtok: 75.0,
    context_window: 200_000,
};

const OPUS_3: ModelPricing = ModelPricing {
    input_per_mtok: 15.0,
    output_per_mtok: 75.0,
    context_window: 200_000,
};

const SONNET_4_6: ModelPricing = ModelPricing {
    input_per_mtok: 3.0,
    output_per_mtok: 15.0,
    context_window: 200_000,
};

const SONNET_4_5: ModelPricing = ModelPricing {
    input_per_mtok: 3.0,
    output_per_mtok: 15.0,
    context_window: 200_000,
};

const SONNET_4: ModelPricing = ModelPricing {
    input_per_mtok: 3.0,
    output_per_mtok: 15.0,
    context_window: 200_000,
};

const SONNET_3_7: ModelPricing = ModelPricing {
    input_per_mtok: 3.0,
    output_per_mtok: 15.0,
    context_window: 200_000,
};

const HAIKU_4_5: ModelPricing = ModelPricing {
    input_per_mtok: 1.0,
    output_per_mtok: 5.0,
    context_window: 200_000,
};

const HAIKU_3_5: ModelPricing = ModelPricing {
    input_per_mtok: 0.8,
    output_per_mtok: 4.0,
    context_window: 200_000,
};

const HAIKU_3: ModelPricing = ModelPricing {
    input_per_mtok: 0.25,
    output_per_mtok: 1.25,
    context_window: 200_000,
};

/// Fallback for unknown models — cheapest tier to avoid overstating.
const FALLBACK: ModelPricing = HAIKU_3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_anthropic_direct_ids() {
        let p = lookup("claude-opus-4-6");
        assert_eq!(p.input_per_mtok, 5.0);
        assert_eq!(p.output_per_mtok, 25.0);

        let p = lookup("claude-sonnet-4-20250514");
        assert_eq!(p.input_per_mtok, 3.0);

        let p = lookup("claude-haiku-4-5-20251001");
        assert_eq!(p.input_per_mtok, 1.0);
    }

    #[test]
    fn lookup_bedrock_ids() {
        let p = lookup("us.anthropic.claude-sonnet-4-6-v1:0");
        assert_eq!(p.input_per_mtok, 3.0);
        assert_eq!(p.output_per_mtok, 15.0);

        let p = lookup("anthropic.claude-opus-4-6-v1");
        assert_eq!(p.input_per_mtok, 5.0);

        let p = lookup("anthropic.claude-haiku-4-5-20251001-v1:0");
        assert_eq!(p.input_per_mtok, 1.0);
    }

    #[test]
    fn lookup_generic_family_names() {
        let p = lookup("some-opus-model");
        assert_eq!(p.input_per_mtok, 5.0); // defaults to latest opus

        let p = lookup("some-sonnet-model");
        assert_eq!(p.input_per_mtok, 3.0); // defaults to latest sonnet
    }

    #[test]
    fn lookup_unknown_model_uses_fallback() {
        let p = lookup("gpt-4o");
        assert_eq!(p.input_per_mtok, 0.25); // Haiku 3 fallback
    }

    #[test]
    fn calculate_cost_basic() {
        // 10k input + 1k output on Sonnet 4.6
        // Input: 10_000 / 1_000_000 * 3.0 = 0.03
        // Output: 1_000 / 1_000_000 * 15.0 = 0.015
        // Total: 0.045
        let cost = calculate_cost("claude-sonnet-4-6", 10_000, 1_000);
        assert!((cost - 0.045).abs() < 1e-10);
    }

    #[test]
    fn calculate_cost_zero_tokens() {
        let cost = calculate_cost("claude-opus-4-6", 0, 0);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn calculate_cost_with_cache_savings() {
        // Sonnet 4.6: $3/MTok input, $15/MTok output
        // 50 uncached input + 10k cache write + 100k cache read + 1k output
        // uncached: 50/1M * 3.0 = 0.00015
        // cache write: 10_000/1M * 3.0 * 1.25 = 0.0375
        // cache read: 100_000/1M * 3.0 * 0.1 = 0.03
        // output: 1_000/1M * 15.0 = 0.015
        // total = 0.08265
        let cost = calculate_cost_with_cache(
            "claude-sonnet-4-6",
            50,
            10_000,
            100_000,
            1_000,
        );
        assert!((cost - 0.08265).abs() < 1e-10);

        // Compare: without caching, all 110k tokens are at base price
        // input: 110_050/1M * 3.0 = 0.330150
        // output: 1_000/1M * 15.0 = 0.015
        // total = 0.345150
        let no_cache = calculate_cost("claude-sonnet-4-6", 110_050, 1_000);
        assert!(cost < no_cache, "cached cost should be cheaper");
    }

    #[test]
    fn context_window_returns_correct_value() {
        assert_eq!(context_window("claude-opus-4-6"), 200_000);
        assert_eq!(context_window("claude-sonnet-4-6"), 200_000);
        assert_eq!(context_window("unknown-model"), 200_000);
    }

    #[test]
    fn older_opus_is_more_expensive() {
        let new = lookup("claude-opus-4-6");
        let old = lookup("claude-opus-4-1");
        assert!(old.input_per_mtok > new.input_per_mtok);
    }
}
