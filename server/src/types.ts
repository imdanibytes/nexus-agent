export interface AgentSettings {
  llm_endpoint: string;
  llm_api_key: string;
  llm_model: string;
  system_prompt: string;
  max_tool_rounds: number;
}

export interface ConversationMeta {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  messageCount: number;
}

export interface Message {
  id: string;
  role: "user" | "assistant";
  content: string;
  timestamp: number;
  toolCalls?: ToolCallInfo[];
  uiSurfaces?: UiSurfaceInfo[];
}

export interface ToolCallInfo {
  id: string;
  name: string;
  args: Record<string, unknown>;
  result?: string;
  isError?: boolean;
}

export interface UiSurfaceInfo {
  toolUseId: string;
  name: string;
  input: Record<string, unknown>;
  response?: unknown;
}

export interface Conversation {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  messages: Message[];
}

export interface McpTool {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
  plugin_id: string;
  plugin_name: string;
  enabled: boolean;
}

export interface SseWriter {
  writeEvent(event: string, data: unknown): void;
  close(): void;
}
