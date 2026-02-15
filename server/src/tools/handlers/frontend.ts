import type { ToolHandler, ToolResult, ToolContext, ToolDefinition } from "../types.js";

interface PendingRequest {
  resolve: (result: { content: string; isError: boolean }) => void;
  timer: ReturnType<typeof setTimeout>;
}

export class FrontendToolBridge {
  private pending = new Map<string, PendingRequest>();
  private defaultTimeoutMs: number;

  constructor(defaultTimeoutMs = 30_000) {
    this.defaultTimeoutMs = defaultTimeoutMs;
  }

  createHandler(definition: ToolDefinition, timeoutMs?: number): ToolHandler {
    const bridge = this;
    const timeout = timeoutMs ?? this.defaultTimeoutMs;

    return {
      definition,
      async execute(
        toolUseId: string,
        args: Record<string, unknown>,
        ctx: ToolContext,
      ): Promise<ToolResult> {
        ctx.sse.writeEvent("tool_request", {
          tool_use_id: toolUseId,
          name: definition.name,
          input: args,
        });

        const result = await bridge.waitForResult(toolUseId, timeout);

        return {
          tool_use_id: toolUseId,
          content: result.content,
          is_error: result.isError,
        };
      },
    };
  }

  resolve(toolUseId: string, content: string, isError: boolean): boolean {
    const pending = this.pending.get(toolUseId);
    if (!pending) return false;
    clearTimeout(pending.timer);
    pending.resolve({ content, isError });
    this.pending.delete(toolUseId);
    return true;
  }

  private waitForResult(
    toolUseId: string,
    timeoutMs: number,
  ): Promise<{ content: string; isError: boolean }> {
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        this.pending.delete(toolUseId);
        resolve({ content: "Frontend tool timed out", isError: true });
      }, timeoutMs);

      this.pending.set(toolUseId, { resolve, timer });
    });
  }
}

export function createClipboardTools(bridge: FrontendToolBridge): ToolHandler[] {
  return [
    bridge.createHandler({
      name: "_nexus_clipboard_read",
      description: "Read the current contents of the user's clipboard.",
      input_schema: {
        type: "object",
        properties: {},
        required: [],
      },
    }),
    bridge.createHandler({
      name: "_nexus_clipboard_write",
      description: "Write text to the user's clipboard.",
      input_schema: {
        type: "object",
        properties: {
          text: {
            type: "string",
            description: "Text to write to clipboard",
          },
        },
        required: ["text"],
      },
    }),
  ];
}
