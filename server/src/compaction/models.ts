import type { Provider } from "../types.js";

/** Resolved metadata about a model */
export interface ModelMeta {
  /** Model identifier as passed to the API */
  name: string;
  /** Architecture family (e.g., "llama", "qwen3", "gemma3", "claude") */
  family: string;
  /** Context window size in tokens */
  contextWindow: number;
}

const DEFAULT_CONTEXT_WINDOW = 200_000;
const DEFAULT_FAMILY = "unknown";

// ─── Provider-specific resolvers ────────────────────────────────────────────

/**
 * Resolve model metadata by querying an Ollama-compatible /api/show endpoint.
 * Returns `general.architecture` as family and `{family}.context_length` as context window.
 */
class OllamaModelResolver {
  private cache = new Map<string, ModelMeta>();

  async resolve(model: string, endpoint: string): Promise<ModelMeta | null> {
    const cacheKey = `${endpoint}::${model}`;
    const cached = this.cache.get(cacheKey);
    if (cached) return cached;

    try {
      const base = endpoint.replace(/\/v1\/?$/, "");
      const res = await fetch(`${base}/api/show`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name: model }),
        signal: AbortSignal.timeout(3000),
      });

      if (!res.ok) return null;

      const data = (await res.json()) as {
        model_info?: Record<string, unknown>;
      };

      if (!data.model_info) return null;

      const family =
        (data.model_info["general.architecture"] as string) ?? DEFAULT_FAMILY;

      let contextWindow = DEFAULT_CONTEXT_WINDOW;
      for (const [key, value] of Object.entries(data.model_info)) {
        if (key.endsWith(".context_length") && typeof value === "number") {
          contextWindow = value;
          break;
        }
      }

      const meta: ModelMeta = { name: model, family, contextWindow };
      this.cache.set(cacheKey, meta);
      return meta;
    } catch {
      return null;
    }
  }
}

/**
 * Static resolver for Anthropic models.
 * All current Anthropic models share the same context window.
 */
class AnthropicModelResolver {
  private static readonly MODELS: Record<string, ModelMeta> = {
    // Claude 4 family
    "claude-opus-4-20250514": { name: "claude-opus-4-20250514", family: "claude-4", contextWindow: 200_000 },
    "claude-sonnet-4-20250514": { name: "claude-sonnet-4-20250514", family: "claude-4", contextWindow: 200_000 },
    // Claude 3.5 family
    "claude-3-5-sonnet-20241022": { name: "claude-3-5-sonnet-20241022", family: "claude-3.5", contextWindow: 200_000 },
    "claude-3-5-sonnet-20240620": { name: "claude-3-5-sonnet-20240620", family: "claude-3.5", contextWindow: 200_000 },
    "claude-3-5-haiku-20241022": { name: "claude-3-5-haiku-20241022", family: "claude-3.5", contextWindow: 200_000 },
    // Claude 3 family
    "claude-3-opus-20240229": { name: "claude-3-opus-20240229", family: "claude-3", contextWindow: 200_000 },
    "claude-3-sonnet-20240229": { name: "claude-3-sonnet-20240229", family: "claude-3", contextWindow: 200_000 },
    "claude-3-haiku-20240307": { name: "claude-3-haiku-20240307", family: "claude-3", contextWindow: 200_000 },
  };

  resolve(model: string): ModelMeta | null {
    if (AnthropicModelResolver.MODELS[model]) {
      return AnthropicModelResolver.MODELS[model];
    }
    // Prefix match for versioned variants
    for (const [key, meta] of Object.entries(AnthropicModelResolver.MODELS)) {
      if (model.startsWith(key)) return { ...meta, name: model };
    }
    // Any "claude-" model we don't know — assume latest context window
    if (model.startsWith("claude-")) {
      return { name: model, family: "claude", contextWindow: 200_000 };
    }
    return null;
  }
}

/**
 * Static resolver for OpenAI models.
 */
class OpenAIModelResolver {
  private static readonly MODELS: Record<string, ModelMeta> = {
    "gpt-4o": { name: "gpt-4o", family: "gpt-4o", contextWindow: 128_000 },
    "gpt-4o-mini": { name: "gpt-4o-mini", family: "gpt-4o", contextWindow: 128_000 },
    "gpt-4-turbo": { name: "gpt-4-turbo", family: "gpt-4", contextWindow: 128_000 },
    "gpt-4": { name: "gpt-4", family: "gpt-4", contextWindow: 8_192 },
    "gpt-3.5-turbo": { name: "gpt-3.5-turbo", family: "gpt-3.5", contextWindow: 16_385 },
  };

  resolve(model: string): ModelMeta | null {
    if (OpenAIModelResolver.MODELS[model]) {
      return OpenAIModelResolver.MODELS[model];
    }
    for (const [key, meta] of Object.entries(OpenAIModelResolver.MODELS)) {
      if (model.startsWith(key)) return { ...meta, name: model };
    }
    return null;
  }
}

// ─── Unified resolver ───────────────────────────────────────────────────────

const ollamaResolver = new OllamaModelResolver();
const anthropicResolver = new AnthropicModelResolver();
const openaiResolver = new OpenAIModelResolver();

/**
 * Resolve model metadata for any provider.
 *
 * - Ollama / OpenAI-compatible: queries the server's /api/show for exact metadata
 * - Anthropic: static lookup
 * - Bedrock: resolves as Anthropic (same models, different routing)
 * - Unknown: returns generous defaults
 */
export async function resolveModel(
  model: string,
  provider?: Provider | null,
): Promise<ModelMeta> {
  if (provider) {
    switch (provider.type) {
      case "ollama":
      case "openai-compatible": {
        if (provider.endpoint) {
          const meta = await ollamaResolver.resolve(model, provider.endpoint);
          if (meta) return meta;
        }
        // Ollama endpoint unreachable — try static resolvers as fallback
        break;
      }
      case "anthropic": {
        const meta = anthropicResolver.resolve(model);
        if (meta) return meta;
        break;
      }
      case "bedrock": {
        // Bedrock uses Anthropic models with different IDs
        // e.g., "anthropic.claude-3-5-sonnet-20241022-v2:0"
        const stripped = model
          .replace(/^anthropic\./, "")
          .replace(/-v\d+:\d+$/, "");
        const meta = anthropicResolver.resolve(stripped);
        if (meta) return { ...meta, name: model };
        break;
      }
    }
  }

  // No provider or provider-specific lookup failed — try all static resolvers
  const anthropicMeta = anthropicResolver.resolve(model);
  if (anthropicMeta) return anthropicMeta;

  const openaiMeta = openaiResolver.resolve(model);
  if (openaiMeta) return openaiMeta;

  return { name: model, family: DEFAULT_FAMILY, contextWindow: DEFAULT_CONTEXT_WINDOW };
}

/** Convenience — resolve just the context window */
export async function resolveContextWindow(
  model: string,
  provider?: Provider | null,
): Promise<number> {
  const meta = await resolveModel(model, provider);
  return meta.contextWindow;
}
