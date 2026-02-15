import { getAccessToken } from "../../auth.js";
import type { McpTool } from "../../types.js";
import type { ToolHandler, ToolResult, ToolContext } from "../types.js";

const NEXUS_HOST_URL =
  process.env.NEXUS_HOST_URL || "http://host.docker.internal:9600";

let cachedHandlers: ToolHandler[] = [];
let lastFetch = 0;
const CACHE_TTL = 30_000;

export async function fetchMcpToolHandlers(): Promise<ToolHandler[]> {
  if (cachedHandlers.length > 0 && Date.now() - lastFetch < CACHE_TTL) {
    return cachedHandlers;
  }

  try {
    const token = await getAccessToken();
    const res = await fetch(`${NEXUS_HOST_URL}/api/v1/mcp/tools`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    if (!res.ok) return cachedHandlers;

    const tools = (await res.json()) as McpTool[];
    cachedHandlers = tools
      .filter((t) => t.enabled)
      .map((t) => createMcpToolHandler(t));
    lastFetch = Date.now();
  } catch (err) {
    console.error("Failed to fetch MCP tools:", err);
  }

  return cachedHandlers;
}

export function invalidateMcpToolCache(): void {
  lastFetch = 0;
}

function createMcpToolHandler(tool: McpTool): ToolHandler {
  return {
    definition: {
      name: tool.name,
      description: `[${tool.plugin_name}] ${tool.description}`,
      input_schema: tool.input_schema as ToolHandler["definition"]["input_schema"],
    },

    async execute(
      toolUseId: string,
      args: Record<string, unknown>,
      ctx: ToolContext,
    ): Promise<ToolResult> {
      ctx.sse.writeEvent("tool_executing", {
        id: toolUseId,
        name: tool.name,
      });

      const token = await getAccessToken();
      const res = await fetch(`${NEXUS_HOST_URL}/api/v1/mcp/call`, {
        method: "POST",
        headers: {
          Authorization: `Bearer ${token}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ tool_name: tool.name, arguments: args }),
      });

      if (!res.ok) {
        return {
          tool_use_id: toolUseId,
          content: `MCP call failed: HTTP ${res.status}`,
          is_error: true,
        };
      }

      const data = (await res.json()) as {
        content: { type: string; text: string }[];
        is_error: boolean;
      };

      const text = data.content.map((c) => c.text).join("\n");

      ctx.sse.writeEvent("tool_result", {
        id: toolUseId,
        name: tool.name,
        content: text,
        is_error: data.is_error,
      });

      return {
        tool_use_id: toolUseId,
        content: text,
        is_error: data.is_error,
      };
    },
  };
}
