export interface ModelInfo {
  id: string;
  name: string;
  provider: string;
}

export interface EndpointStatus {
  reachable: boolean;
  provider: string;
  error?: string;
  models: ModelInfo[];
}

export async function probeEndpoint(
  endpoint: string,
  apiKey?: string
): Promise<EndpointStatus> {
  // Try Ollama first: GET {endpoint}/api/tags
  try {
    const ollamaRes = await fetch(`${endpoint}/api/tags`, {
      signal: AbortSignal.timeout(5000),
    });
    if (ollamaRes.ok) {
      const data = (await ollamaRes.json()) as {
        models?: { name: string; details?: Record<string, unknown> }[];
      };
      if (data.models) {
        return {
          reachable: true,
          provider: "ollama",
          models: data.models.map((m) => ({
            id: m.name,
            name: m.name,
            provider: "ollama",
          })),
        };
      }
    }
  } catch {
    // Not Ollama, try OpenAI-compatible
  }

  // Try OpenAI-compatible: GET {endpoint}/v1/models
  try {
    const headers: Record<string, string> = {};
    if (apiKey) {
      headers["Authorization"] = `Bearer ${apiKey}`;
      headers["x-api-key"] = apiKey;
    }
    const oaiRes = await fetch(`${endpoint}/v1/models`, {
      headers,
      signal: AbortSignal.timeout(5000),
    });
    if (oaiRes.ok) {
      const data = (await oaiRes.json()) as {
        data?: { id: string; object?: string; owned_by?: string }[];
      };
      if (data.data) {
        const provider = detectProvider(endpoint, data.data);
        return {
          reachable: true,
          provider,
          models: data.data.map((m) => ({
            id: m.id,
            name: m.id,
            provider: m.owned_by || provider,
          })),
        };
      }
    }
  } catch {
    // Fall through
  }

  // Try base /models (some providers like Anthropic proxy differently)
  try {
    const headers: Record<string, string> = {};
    if (apiKey) {
      headers["Authorization"] = `Bearer ${apiKey}`;
      headers["x-api-key"] = apiKey;
    }
    const baseRes = await fetch(`${endpoint}/models`, {
      headers,
      signal: AbortSignal.timeout(5000),
    });
    if (baseRes.ok) {
      const data = (await baseRes.json()) as {
        data?: { id: string; owned_by?: string }[];
      };
      if (data.data) {
        const provider = detectProvider(endpoint, data.data);
        return {
          reachable: true,
          provider,
          models: data.data.map((m) => ({
            id: m.id,
            name: m.id,
            provider: m.owned_by || provider,
          })),
        };
      }
    }
  } catch {
    // Fall through
  }

  // Check if endpoint is reachable at all
  try {
    await fetch(endpoint, { method: "HEAD", signal: AbortSignal.timeout(3000) });
    return {
      reachable: true,
      provider: "unknown",
      error: "Endpoint reachable but no model listing API found",
      models: [],
    };
  } catch (err) {
    return {
      reachable: false,
      provider: "unknown",
      error: err instanceof Error ? err.message : "Connection failed",
      models: [],
    };
  }
}

function detectProvider(
  endpoint: string,
  models: { id: string; owned_by?: string }[]
): string {
  const url = endpoint.toLowerCase();
  if (url.includes("anthropic")) return "anthropic";
  if (url.includes("openai")) return "openai";
  if (url.includes("localhost") || url.includes("host.docker.internal")) {
    // Check model names for hints
    if (models.some((m) => m.id.includes("claude"))) return "anthropic";
    if (models.some((m) => m.id.includes("gpt"))) return "openai";
    return "vllm";
  }
  return "openai-compatible";
}
