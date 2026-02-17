import Anthropic from "@anthropic-ai/sdk";
import type { Agent, AgentSettings, Provider } from "./types.js";
import { getAgent, getActiveAgentId } from "./agents.js";
import { getProvider } from "./providers.js";
import { createLlmClient } from "./client-factory.js";

export interface TurnConfig {
  client: Anthropic;
  provider: Provider | null;
  model: string;
  maxTokens: number;
  temperature?: number;
  topP?: number;
  agent: Agent | null;
}

/**
 * Resolve the LLM client and model configuration for a turn.
 *
 * Resolution order: explicit agentId → active agent → legacy settings fallback.
 */
export async function resolveTurnConfig(
  agentId: string | undefined,
  settings: AgentSettings,
): Promise<TurnConfig> {
  const effectiveAgentId = agentId || getActiveAgentId();
  const agent: Agent | null = effectiveAgentId
    ? getAgent(effectiveAgentId)
    : null;

  let client: Anthropic;
  let provider: Provider | null = null;
  let model: string;
  let maxTokens = 8192;
  let temperature: number | undefined;
  let topP: number | undefined;

  if (agent) {
    const p = await getProvider(agent.providerId);
    provider = p ?? null;
    client = provider
      ? await createLlmClient(provider)
      : new Anthropic({
          apiKey: settings.llm_api_key || "ollama",
          baseURL: settings.llm_endpoint,
        });
    model = agent.model;
    if (agent.maxTokens) maxTokens = agent.maxTokens;
    if (agent.temperature !== undefined) temperature = agent.temperature;
    if (agent.topP !== undefined) topP = agent.topP;
  } else {
    client = new Anthropic({
      apiKey: settings.llm_api_key || "ollama",
      baseURL: settings.llm_endpoint,
    });
    model = settings.llm_model;
  }

  return { client, provider, model, maxTokens, temperature, topP, agent };
}
