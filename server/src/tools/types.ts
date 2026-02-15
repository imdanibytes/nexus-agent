import type { Conversation, SseWriter } from "../types.js";

export interface ToolResult {
  tool_use_id: string;
  content: string;
  is_error?: boolean;
}

export interface ToolContext {
  conversationId: string;
  sse: SseWriter;
  conversation: Conversation;
  saveConversation: (conv: Conversation) => void;
  signal: AbortSignal;
}

export interface ToolDefinition {
  name: string;
  description: string;
  input_schema: {
    type: "object";
    properties: Record<string, unknown>;
    required?: string[];
  };
}

export interface ToolHandler {
  definition: ToolDefinition;
  execute(
    toolUseId: string,
    args: Record<string, unknown>,
    ctx: ToolContext,
  ): Promise<ToolResult>;
}
