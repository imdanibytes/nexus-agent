import { getMcpClient, closeMcpClient } from "../../mcp-client.js";
import type { ToolHandler, ToolResult, ToolContext } from "../types.js";

let cachedHandlers: ToolHandler[] = [];
let lastFetch = 0;
const CACHE_TTL = 30_000;

export async function fetchMcpToolHandlers(): Promise<ToolHandler[]> {
  if (cachedHandlers.length > 0 && Date.now() - lastFetch < CACHE_TTL) {
    return cachedHandlers;
  }

  try {
    const client = await getMcpClient();
    const { tools } = await client.listTools();

    cachedHandlers = tools.map((t) => createMcpToolHandler(t.name, t.description ?? "", t.inputSchema));
    lastFetch = Date.now();
  } catch (err) {
    console.error("Failed to fetch MCP tools:", err);
    await closeMcpClient();
  }

  return cachedHandlers;
}

export function invalidateMcpToolCache(): void {
  lastFetch = 0;
}

function createMcpToolHandler(
  name: string,
  description: string,
  inputSchema: Record<string, unknown>,
): ToolHandler {
  return {
    definition: {
      name,
      description,
      input_schema: inputSchema as ToolHandler["definition"]["input_schema"],
    },

    async execute(
      toolUseId: string,
      args: Record<string, unknown>,
      ctx: ToolContext,
    ): Promise<ToolResult> {
      ctx.sse.writeEvent("tool_executing", {
        id: toolUseId,
        name,
      });

      try {
        const client = await getMcpClient();
        const result = await client.callTool({ name, arguments: args });

        const text = (result.content as { type: string; text: string }[])
          .map((c) => c.text)
          .join("\n");
        const isError = result.isError === true;

        ctx.sse.writeEvent("tool_result", {
          id: toolUseId,
          name,
          content: text,
          is_error: isError,
        });

        return {
          tool_use_id: toolUseId,
          content: text,
          is_error: isError,
        };
      } catch (err) {
        await closeMcpClient();
        const msg = err instanceof Error ? err.message : String(err);
        return {
          tool_use_id: toolUseId,
          content: `MCP call failed: ${msg}`,
          is_error: true,
        };
      }
    },
  };
}
