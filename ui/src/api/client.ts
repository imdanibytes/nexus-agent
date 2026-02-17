export interface Config {
  token: string;
  apiUrl: string;
}

let cachedConfig: Config | null = null;

export async function getConfig(): Promise<Config> {
  if (cachedConfig) return cachedConfig;
  const res = await fetch("/api/config");
  if (!res.ok) throw new Error("Failed to fetch config");
  cachedConfig = (await res.json()) as Config;
  return cachedConfig;
}

export interface WireMessage {
  role: "user" | "assistant";
  content: string;
  toolCalls?: {
    id: string;
    name: string;
    args: Record<string, unknown>;
    result?: string;
    isError?: boolean;
  }[];
}

// ── Types ──

export interface ConversationMeta {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  messageCount: number;
}

export interface RepositoryMessage {
  message: unknown;
  parentId: string | null;
}

export interface ConversationFull {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  messages: Message[];
  repository?: {
    messages: RepositoryMessage[];
  };
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
  timingSpans?: import("../stores/chatStore.js").TimingSpan[];
  mcpSource?: boolean;
}

export interface UiSurfaceInfo {
  toolUseId: string;
  name: string;
  input: Record<string, unknown>;
  response?: unknown;
}

export interface ModelInfo {
  id: string;
  name: string;
  provider: string;
}

export interface EndpointStatus {
  reachable: boolean;
  provider: string;
  error?: string;
  models: ModelInfo[];
}

export interface AgentSettingsPublic {
  llm_endpoint: string;
  llm_model: string;
  system_prompt: string;
  max_tool_rounds: number;
}

// ── Provider types ──

export type ProviderType = "ollama" | "anthropic" | "bedrock" | "openai-compatible";

export interface ProviderPublic {
  id: string;
  name: string;
  type: ProviderType;
  endpoint?: string;
  awsRegion?: string;
  createdAt: number;
  updatedAt: number;
}

export interface ProviderCreateData {
  name: string;
  type: ProviderType;
  endpoint?: string;
  apiKey?: string;
  awsRegion?: string;
  awsAccessKeyId?: string;
  awsSecretAccessKey?: string;
  awsSessionToken?: string;
}

// ── Agent types ──

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

// ── Tool settings types ──

export interface ToolSettings {
  hiddenToolPatterns: string[];
  globalToolFilter?: ToolFilter;
}

export interface AvailableTool {
  name: string;
  description: string;
  source: string;
}

// ── Conversations ──

export async function fetchConversations(): Promise<ConversationMeta[]> {
  const res = await fetch("/api/conversations");
  if (!res.ok) return [];
  return res.json();
}

export async function fetchConversation(id: string): Promise<ConversationFull | null> {
  const res = await fetch(`/api/conversations/${id}`);
  if (!res.ok) return null;
  return res.json();
}

export async function createConversation(): Promise<{ id: string; title: string }> {
  const res = await fetch("/api/conversations", { method: "POST" });
  return res.json();
}

export async function deleteConversation(id: string): Promise<void> {
  await fetch(`/api/conversations/${id}`, { method: "DELETE" });
}

export async function appendRepositoryMessage(
  convId: string,
  message: unknown,
  parentId: string | null,
): Promise<void> {
  await fetch(`/api/conversations/${convId}/messages`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ message, parentId }),
  });
}

export async function renameConversation(id: string, title: string): Promise<void> {
  await fetch(`/api/conversations/${id}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ title }),
  });
}

// ── Providers ──

export async function fetchProviders(): Promise<ProviderPublic[]> {
  const res = await fetch("/api/providers");
  if (!res.ok) return [];
  return res.json();
}

export async function createProviderApi(data: ProviderCreateData): Promise<ProviderPublic> {
  const res = await fetch("/api/providers", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  return res.json();
}

export async function updateProviderApi(
  id: string,
  data: Partial<ProviderCreateData>,
): Promise<ProviderPublic> {
  const res = await fetch(`/api/providers/${id}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  return res.json();
}

export async function deleteProviderApi(id: string): Promise<void> {
  await fetch(`/api/providers/${id}`, { method: "DELETE" });
}

export async function probeProviderApi(id: string): Promise<EndpointStatus> {
  const res = await fetch(`/api/providers/${id}/probe`, { method: "POST" });
  return res.json();
}

export async function probeProviderDataApi(data: ProviderCreateData): Promise<EndpointStatus> {
  const res = await fetch("/api/providers/probe", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  return res.json();
}

// ── Agents ──

export async function fetchAgents(): Promise<Agent[]> {
  const res = await fetch("/api/agents");
  if (!res.ok) return [];
  return res.json();
}

export async function createAgentApi(data: {
  name: string;
  providerId: string;
  model: string;
  systemPrompt: string;
  temperature?: number | null;
  maxTokens?: number;
  topP?: number | null;
  toolFilter?: ToolFilter;
}): Promise<Agent> {
  const res = await fetch("/api/agents", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  return res.json();
}

export async function updateAgentApi(
  id: string,
  data: Partial<{
    name: string;
    providerId: string;
    model: string;
    systemPrompt: string;
    temperature: number | null;
    maxTokens: number;
    topP: number | null;
    toolFilter: ToolFilter;
  }>,
): Promise<Agent> {
  const res = await fetch(`/api/agents/${id}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  return res.json();
}

export async function deleteAgentApi(id: string): Promise<void> {
  await fetch(`/api/agents/${id}`, { method: "DELETE" });
}

export async function getActiveAgent(): Promise<{ agentId: string | null }> {
  const res = await fetch("/api/agents/active");
  return res.json();
}

export async function setActiveAgent(agentId: string | null): Promise<void> {
  await fetch("/api/agents/active", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ agentId }),
  });
}

// ── Tool settings ──

export async function fetchToolSettings(): Promise<ToolSettings> {
  const res = await fetch("/api/tool-settings");
  return res.json();
}

export async function updateToolSettingsApi(
  updates: Partial<ToolSettings>,
): Promise<ToolSettings> {
  const res = await fetch("/api/tool-settings", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(updates),
  });
  return res.json();
}

export async function fetchAvailableTools(): Promise<AvailableTool[]> {
  const res = await fetch("/api/tools");
  if (!res.ok) return [];
  return res.json();
}

// ── Discovery (legacy) ──

export async function discoverModels(
  endpoint?: string,
  apiKey?: string
): Promise<EndpointStatus> {
  const res = await fetch("/api/discover", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ endpoint, apiKey }),
  });
  return res.json();
}

// ── Settings (legacy) ──

export async function fetchSettings(): Promise<AgentSettingsPublic> {
  const res = await fetch("/api/settings");
  return res.json();
}

export async function saveSettings(updates: Partial<AgentSettingsPublic>): Promise<void> {
  await fetch("/api/settings", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(updates),
  });
}

