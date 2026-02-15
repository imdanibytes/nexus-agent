import { getAccessToken } from "./auth.js";
import type { AgentSettings } from "./types.js";

const NEXUS_HOST_URL = process.env.NEXUS_HOST_URL || "http://host.docker.internal:9600";

const DEFAULTS: AgentSettings = {
  llm_endpoint: "http://host.docker.internal:11434",
  llm_api_key: "",
  llm_model: "qwen3:30b",
  system_prompt: "You are a helpful assistant with access to tools from the Nexus platform.",
  max_tool_rounds: 10,
};

export async function getSettings(): Promise<AgentSettings> {
  try {
    const token = await getAccessToken();
    const res = await fetch(`${NEXUS_HOST_URL}/api/v1/settings`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    if (!res.ok) return DEFAULTS;
    const data = await res.json() as Record<string, unknown>;
    return {
      llm_endpoint: (data.llm_endpoint as string) || DEFAULTS.llm_endpoint,
      llm_api_key: (data.llm_api_key as string) || DEFAULTS.llm_api_key,
      llm_model: (data.llm_model as string) || DEFAULTS.llm_model,
      system_prompt: (data.system_prompt as string) || DEFAULTS.system_prompt,
      max_tool_rounds: (data.max_tool_rounds as number) || DEFAULTS.max_tool_rounds,
    };
  } catch {
    return DEFAULTS;
  }
}
