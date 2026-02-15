import {
  BedrockClient,
  ListFoundationModelsCommand,
} from "@aws-sdk/client-bedrock";

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

  // HEAD-only reachability is not enough — we need a working model API
  try {
    await fetch(endpoint, { method: "HEAD", signal: AbortSignal.timeout(3000) });
    return {
      reachable: false,
      provider: "unknown",
      error: "Server is reachable but no model listing API responded. Check the endpoint URL or API key.",
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

/** Probe a Provider by resolving its endpoint/key and calling probeEndpoint */
export async function probeProvider(
  provider: import("./types.js").Provider,
): Promise<EndpointStatus> {
  switch (provider.type) {
    case "ollama":
      return probeEndpoint(provider.endpoint!);
    case "anthropic":
      return probeAnthropic(provider);
    case "openai-compatible":
      return probeEndpoint(provider.endpoint!, provider.apiKey);
    case "bedrock":
      return probeBedrock(provider);
    default:
      return {
        reachable: false,
        provider: "unknown",
        error: `Unknown provider type: ${provider.type}`,
        models: [],
      };
  }
}

async function probeAnthropic(
  provider: import("./types.js").Provider,
): Promise<EndpointStatus> {
  const baseUrl = provider.endpoint || "https://api.anthropic.com";
  try {
    const res = await fetch(`${baseUrl}/v1/models`, {
      headers: {
        "x-api-key": provider.apiKey || "",
        "anthropic-version": "2023-06-01",
      },
      signal: AbortSignal.timeout(10000),
    });

    if (!res.ok) {
      const text = await res.text().catch(() => "");
      return {
        reachable: false,
        provider: "anthropic",
        error: `API returned ${res.status}: ${text.slice(0, 200)}`,
        models: [],
      };
    }

    const data = (await res.json()) as {
      data?: { id: string; display_name?: string; type?: string }[];
    };

    const models: ModelInfo[] = (data.data ?? []).map((m) => ({
      id: m.id,
      name: m.display_name || m.id,
      provider: "anthropic",
    }));

    return { reachable: true, provider: "anthropic", models };
  } catch (err) {
    return {
      reachable: false,
      provider: "anthropic",
      error: err instanceof Error ? err.message : "Connection failed",
      models: [],
    };
  }
}

async function probeBedrock(
  provider: import("./types.js").Provider,
): Promise<EndpointStatus> {
  try {
    const client = new BedrockClient({
      region: provider.awsRegion || "us-east-1",
      ...(provider.awsAccessKeyId && provider.awsSecretAccessKey
        ? {
            credentials: {
              accessKeyId: provider.awsAccessKeyId,
              secretAccessKey: provider.awsSecretAccessKey,
              ...(provider.awsSessionToken ? { sessionToken: provider.awsSessionToken } : {}),
            },
          }
        : {}),
    });

    const res = await client.send(
      new ListFoundationModelsCommand({ byInferenceType: "ON_DEMAND" }),
    );

    const models: ModelInfo[] = (res.modelSummaries ?? []).map((m) => ({
      id: m.modelId!,
      name: m.modelName || m.modelId!,
      provider: m.providerName || "bedrock",
    }));

    return { reachable: true, provider: "bedrock", models };
  } catch (err) {
    return {
      reachable: false,
      provider: "bedrock",
      error: err instanceof Error ? err.message : "Bedrock connection failed",
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
