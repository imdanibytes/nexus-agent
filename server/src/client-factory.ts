import Anthropic from "@anthropic-ai/sdk";
import type { Provider } from "./types.js";

// Cache keyed by provider ID — stale entries detected via updatedAt
const clientCache = new Map<string, { client: Anthropic; updatedAt: number }>();

/**
 * Create (or return a cached) LLM client from a provider configuration.
 * Cache invalidates automatically when the provider's updatedAt changes.
 */
export async function createLlmClient(
  provider: Provider,
): Promise<Anthropic> {
  const cached = clientCache.get(provider.id);
  if (cached && cached.updatedAt === provider.updatedAt) {
    return cached.client;
  }

  const client = await buildClient(provider);
  clientCache.set(provider.id, { client, updatedAt: provider.updatedAt });
  return client;
}

/** Remove a provider's cached client (belt-and-suspenders for CRUD ops). */
export function invalidateClientCache(providerId: string): void {
  clientCache.delete(providerId);
}

async function buildClient(provider: Provider): Promise<Anthropic> {
  switch (provider.type) {
    case "ollama":
      return new Anthropic({
        apiKey: "ollama",
        baseURL: provider.endpoint!,
      });

    case "anthropic":
      return new Anthropic({
        apiKey: provider.apiKey!,
        ...(provider.endpoint ? { baseURL: provider.endpoint } : {}),
      });

    case "bedrock": {
      // Dynamic import — @anthropic-ai/bedrock-sdk is optional
      const { AnthropicBedrock } = await import("@anthropic-ai/bedrock-sdk");
      return new AnthropicBedrock({
        awsRegion: provider.awsRegion,
        awsAccessKey: provider.awsAccessKeyId,
        awsSecretKey: provider.awsSecretAccessKey,
        awsSessionToken: provider.awsSessionToken,
      }) as unknown as Anthropic;
    }

    case "openai-compatible":
      return new Anthropic({
        apiKey: provider.apiKey || "no-key",
        baseURL: provider.endpoint!,
      });

    default:
      throw new Error(`Unknown provider type: ${provider.type}`);
  }
}
