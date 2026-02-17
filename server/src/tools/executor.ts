import type { ToolHandler, ToolResult, ToolContext, ToolDefinition } from "./types.js";

export class ToolExecutor {
  private handlers = new Map<string, ToolHandler>();

  register(handler: ToolHandler): void {
    this.handlers.set(handler.definition.name, handler);
  }

  registerAll(handlers: ToolHandler[]): void {
    for (const h of handlers) {
      this.register(h);
    }
  }

  has(name: string): boolean {
    return this.handlers.has(name);
  }

  definitions(): ToolDefinition[] {
    return Array.from(this.handlers.values()).map((h) => h.definition);
  }

  async execute(
    name: string,
    toolUseId: string,
    args: Record<string, unknown>,
    ctx: ToolContext,
  ): Promise<ToolResult> {
    const handler = this.handlers.get(name);
    if (!handler) {
      return {
        tool_use_id: toolUseId,
        content: `Unknown tool: ${name}`,
        is_error: true,
      };
    }

    try {
      return await handler.execute(toolUseId, args, ctx);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      return {
        tool_use_id: toolUseId,
        content: `Tool execution failed: ${message}`,
        is_error: true,
      };
    }
  }
}
