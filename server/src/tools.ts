import Anthropic from "@anthropic-ai/sdk";
import { getAccessToken } from "./auth.js";
import type { McpTool } from "./types.js";

const NEXUS_HOST_URL = process.env.NEXUS_HOST_URL || "http://host.docker.internal:9600";

let cachedTools: Anthropic.Tool[] = [];
let lastFetch = 0;
const CACHE_TTL = 30_000;

export async function getMcpTools(): Promise<Anthropic.Tool[]> {
  if (cachedTools.length > 0 && Date.now() - lastFetch < CACHE_TTL) {
    return cachedTools;
  }

  try {
    const token = await getAccessToken();
    const res = await fetch(`${NEXUS_HOST_URL}/api/v1/mcp/tools`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    if (!res.ok) return cachedTools;

    const tools = (await res.json()) as McpTool[];
    cachedTools = tools
      .filter((t) => t.enabled)
      .map((t) => ({
        name: t.name,
        description: `[${t.plugin_name}] ${t.description}`,
        input_schema: t.input_schema as Anthropic.Tool["input_schema"],
      }));
    lastFetch = Date.now();
  } catch (err) {
    console.error("Failed to fetch MCP tools:", err);
  }

  return cachedTools;
}

export function invalidateToolCache(): void {
  lastFetch = 0;
}

export async function callMcpTool(
  toolName: string,
  args: Record<string, unknown>
): Promise<{ content: string; isError: boolean }> {
  const token = await getAccessToken();
  const res = await fetch(`${NEXUS_HOST_URL}/api/v1/mcp/call`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ tool_name: toolName, arguments: args }),
  });

  if (!res.ok) {
    return { content: `MCP call failed: HTTP ${res.status}`, isError: true };
  }

  const data = (await res.json()) as {
    content: { type: string; text: string }[];
    is_error: boolean;
  };

  const text = data.content.map((c) => c.text).join("\n");
  return { content: text, isError: data.is_error };
}
