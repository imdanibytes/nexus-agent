import type { Provider } from "../types.js";

export interface ModelPricing {
  inputPer1M: number;
  outputPer1M: number;
}

const ZERO: ModelPricing = { inputPer1M: 0, outputPer1M: 0 };

// ── Anthropic pricing ───────────────────────────────────────────────────────

const ANTHROPIC_PRICING: Record<string, ModelPricing> = {
  // Claude 4
  "claude-opus-4": { inputPer1M: 15, outputPer1M: 75 },
  "claude-sonnet-4": { inputPer1M: 3, outputPer1M: 15 },
  // Claude 3.5
  "claude-3-5-sonnet": { inputPer1M: 3, outputPer1M: 15 },
  "claude-3-5-haiku": { inputPer1M: 0.80, outputPer1M: 4 },
  // Claude 3
  "claude-3-opus": { inputPer1M: 15, outputPer1M: 75 },
  "claude-3-sonnet": { inputPer1M: 3, outputPer1M: 15 },
  "claude-3-haiku": { inputPer1M: 0.25, outputPer1M: 1.25 },
};

function resolveAnthropicPrice(model: string): ModelPricing | null {
  for (const [prefix, pricing] of Object.entries(ANTHROPIC_PRICING)) {
    if (model.startsWith(prefix)) return pricing;
  }
  return null;
}

// ── OpenAI pricing ──────────────────────────────────────────────────────────

const OPENAI_PRICING: Record<string, ModelPricing> = {
  "gpt-4o-mini": { inputPer1M: 0.15, outputPer1M: 0.60 },
  "gpt-4o": { inputPer1M: 2.50, outputPer1M: 10 },
  "gpt-4-turbo": { inputPer1M: 10, outputPer1M: 30 },
  "gpt-4": { inputPer1M: 30, outputPer1M: 60 },
  "gpt-3.5-turbo": { inputPer1M: 0.50, outputPer1M: 1.50 },
};

function resolveOpenAIPrice(model: string): ModelPricing | null {
  // Exact match first (important: "gpt-4o-mini" before "gpt-4o")
  if (OPENAI_PRICING[model]) return OPENAI_PRICING[model];
  for (const [prefix, pricing] of Object.entries(OPENAI_PRICING)) {
    if (model.startsWith(prefix)) return pricing;
  }
  return null;
}

// ── Public API ──────────────────────────────────────────────────────────────

export function resolvePrice(model: string, provider?: Provider | null): ModelPricing {
  // Strip Bedrock prefix if present
  const normalized = model.replace(/^anthropic\./, "").replace(/-v\d+:\d+$/, "");

  const anthropic = resolveAnthropicPrice(normalized);
  if (anthropic) return anthropic;

  const openai = resolveOpenAIPrice(normalized);
  if (openai) return openai;

  // Unknown model — return 0 rather than guessing
  return ZERO;
}

export function calculateCost(
  inputTokens: number,
  outputTokens: number,
  pricing: ModelPricing,
): number {
  return (
    (inputTokens / 1_000_000) * pricing.inputPer1M +
    (outputTokens / 1_000_000) * pricing.outputPer1M
  );
}
