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

export interface TokenUsage {
  inputTokens: number;
  outputTokens: number;
  timestamp: number;
}

export interface ConversationUsage {
  /** Sum of input tokens billed across all API calls in this conversation */
  totalInputTokens: number;
  /** Sum of output tokens billed across all API calls */
  totalOutputTokens: number;
  /** Running USD cost total */
  totalCost: number;
  /** Latest context fill — input tokens from most recent API response */
  contextTokens: number;
  /** Model context window at time of last call */
  contextWindow: number;
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
  /** Last known token usage from LLM API response — persisted for compaction budgeting */
  lastTokenUsage?: TokenUsage;
  /** Cumulative usage / cost tracking for the conversation */
  usage?: ConversationUsage;
}

export interface SseWriter {
  writeEvent(event: string, data: unknown): void;
  close(): void;
}

/** Wire format from the frontend — matches active branch messages */
export interface WireMessage {
  role: string;
  content: string;
  toolCalls?: WireToolCall[];
}

export interface WireToolCall {
  id: string;
  name: string;
  args: Record<string, unknown>;
  result?: string;
  isError?: boolean;
}
