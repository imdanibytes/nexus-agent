export interface AgentSettings {
  llm_endpoint: string;
  llm_api_key: string;
  llm_model: string;
  system_prompt: string;
  max_tool_rounds: number;
}

export type ProviderType = "ollama" | "anthropic" | "bedrock" | "openai-compatible";

export interface Provider {
  id: string;
  name: string;
  type: ProviderType;
  endpoint?: string;
  apiKey?: string;
  // Bedrock-specific
  awsRegion?: string;
  awsAccessKeyId?: string;
  awsSecretAccessKey?: string;
  awsSessionToken?: string;
  // State
  createdAt: number;
  updatedAt: number;
}

/** Provider without secrets — safe for frontend consumption */
export type ProviderPublic = Omit<Provider, "apiKey" | "awsAccessKeyId" | "awsSecretAccessKey" | "awsSessionToken">;

export interface ToolFilter {
  mode: "allow" | "deny";
  tools: string[];
}

export interface Agent {
  id: string;
  name: string;
  providerId: string;
  model: string;
  systemPrompt: string;
  temperature?: number;
  maxTokens?: number;
  topP?: number;
  toolFilter?: ToolFilter;
  createdAt: number;
  updatedAt: number;
}

export interface ToolSettings {
  hiddenToolPatterns: string[];
  globalToolFilter?: ToolFilter;
}

/** @deprecated Use Agent instead */
export interface AgentProfile {
  id: string;
  name: string;
  model: string;
  systemPrompt: string;
  avatar?: string;
  createdAt: number;
  updatedAt: number;
}

export interface ConversationMeta {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  messageCount: number;
}

export type MessagePart =
  | { type: "text"; text: string }
  | { type: "tool-call"; id: string; name: string; args: Record<string, unknown>; result?: string; isError?: boolean };

export interface Message {
  id: string;
  role: "user" | "assistant";
  parts: MessagePart[];
  timestamp: number;
  uiSurfaces?: UiSurfaceInfo[];
  profileId?: string;
  profileName?: string;
  timingSpans?: import("./timing.js").Span[];
  mcpSource?: boolean;
}

export interface UiSurfaceInfo {
  toolUseId: string;
  name: string;
  input: Record<string, unknown>;
  response?: unknown;
}

export interface RepositoryMessage {
  message: unknown;
  parentId: string | null;
}

export interface Conversation {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  messages: Message[];
  /** Tree-structured message repository for branch persistence */
  repository?: {
    messages: RepositoryMessage[];
  };
}

export interface SseWriter {
  writeEvent(event: string, data: unknown): void;
  close(): void;
}
