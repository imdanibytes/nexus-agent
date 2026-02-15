import Anthropic from "@anthropic-ai/sdk";
import type { Provider } from "./types.js";

/**
 * Create an LLM client from a provider configuration.
 * Ollama and OpenAI-compatible use the Anthropic SDK with a custom baseURL.
 * Anthropic uses the standard SDK.
 * Bedrock uses the Bedrock SDK (lazy-imported to keep it optional).
 */
export async function createLlmClient(
  provider: Provider,
): Promise<Anthropic> {
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
