export interface ConversationMeta {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
}

export interface ConversationUsage {
  input_tokens: number;
  output_tokens: number;
  context_window: number;
  total_cost?: number;
}

export interface ServerSpan {
  index: number;
  message_ids: string[];
  summary?: string;
  sealed_at?: string;
}

export interface ConversationFull {
  id: string;
  title: string;
  messages: ServerMessage[];
  active_path: string[];
  usage?: ConversationUsage;
  agent_id?: string;
  task_state?: {
    plan: import("../types/tasks").Plan | null;
    tasks: Record<string, import("../types/tasks").Task>;
    mode?: import("../types/tasks").AgentMode;
  };
  spans?: ServerSpan[];
  created_at: string;
  updated_at: string;
}

export interface ServerMessage {
  id: string;
  role: "user" | "assistant";
  parts: ServerPart[];
  timestamp: string;
  parent_id: string | null;
  metadata?: Record<string, unknown> | null;
}

export type ServerPart =
  | { type: "text"; text: string }
  | { type: "thinking"; thinking: string }
  | {
      type: "tool-call";
      toolCallId: string;
      toolName: string;
      args: Record<string, unknown>;
      result?: string;
      is_error?: boolean;
    }
  | {
      type: "tool-result";
      toolCallId: string;
      result: string;
      is_error?: boolean;
    };

// ── Conversations ──

export async function fetchConversations(): Promise<ConversationMeta[]> {
  const res = await fetch("/api/conversations");
  if (!res.ok) throw new Error(`Failed to load conversations (${res.status})`);
  return res.json();
}

export async function fetchConversation(
  id: string,
): Promise<ConversationFull | null> {
  const res = await fetch(`/api/conversations/${id}`);
  if (!res.ok) return null;
  return res.json();
}

export async function createConversation(id: string): Promise<ConversationMeta> {
  const res = await fetch("/api/conversations", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ id }),
  });
  return res.json();
}

export async function deleteConversation(id: string): Promise<void> {
  await fetch(`/api/conversations/${id}`, { method: "DELETE" });
}

export async function renameConversation(
  id: string,
  title: string,
): Promise<void> {
  await fetch(`/api/conversations/${id}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ title }),
  });
}

// ── Chat ──

export async function startChat(
  conversationId: string,
  message: string,
  userMessageId?: string,
  assistantMessageId?: string,
): Promise<{ messageId: string }> {
  const res = await fetch("/api/chat", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ conversationId, message, userMessageId, assistantMessageId }),
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: "Chat failed" }));
    throw new Error(body.error || `Chat request failed (${res.status})`);
  }
  const data = await res.json();
  return { messageId: data.messageId };
}

export async function branchChat(
  conversationId: string,
  messageId: string,
  message: string,
  userMessageId?: string,
  assistantMessageId?: string,
): Promise<{ messageId: string }> {
  const res = await fetch("/api/chat/branch", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ conversationId, messageId, message, userMessageId, assistantMessageId }),
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: "Branch failed" }));
    throw new Error(body.error || `Branch request failed (${res.status})`);
  }
  const data = await res.json();
  return { messageId: data.messageId };
}

export async function regenerateChat(
  conversationId: string,
  messageId: string,
  assistantMessageId?: string,
): Promise<void> {
  const res = await fetch("/api/chat/regenerate", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ conversationId, messageId, assistantMessageId }),
  });
  if (!res.ok) {
    const body = await res
      .json()
      .catch(() => ({ error: "Regenerate failed" }));
    throw new Error(
      body.error || `Regenerate request failed (${res.status})`,
    );
  }
}

export async function abortChat(conversationId: string): Promise<void> {
  await fetch("/api/chat/abort", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ conversationId }),
  }).catch(() => {});
}

export async function switchPath(
  conversationId: string,
  messageId: string,
): Promise<ConversationFull> {
  const res = await fetch(`/api/conversations/${conversationId}/path`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ messageId }),
  });
  if (!res.ok) throw new Error(`Switch path failed (${res.status})`);
  return res.json();
}

// ── Client-initiated tool invocation ──

export async function invokeToolCall(
  conversationId: string,
  toolName: string,
  args: Record<string, unknown>,
  assistantMessageId?: string,
): Promise<void> {
  const res = await fetch("/api/chat/tool-invoke", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ conversationId, toolName, args, assistantMessageId }),
  });
  if (!res.ok) throw new Error(`Tool invoke failed (${res.status})`);
}

// ── Ask User ──

export async function answerQuestion(
  conversationId: string,
  questionId: string,
  value: unknown,
): Promise<void> {
  const res = await fetch("/api/chat/answer", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ conversationId, questionId, value }),
  });
  if (!res.ok) throw new Error(`Answer question failed (${res.status})`);
}

// ── Providers ──

export type ProviderType = "anthropic" | "bedrock";

export interface ProviderPublic {
  id: string;
  name: string;
  type: ProviderType;
  endpoint?: string;
  has_api_key: boolean;
  aws_region?: string;
  aws_profile?: string;
  created_at: string;
  updated_at: string;
}

export interface CreateProviderRequest {
  name: string;
  type: ProviderType;
  endpoint?: string;
  api_key?: string;
  aws_region?: string;
  aws_profile?: string;
}

export async function fetchProviders(): Promise<ProviderPublic[]> {
  const res = await fetch("/api/providers");
  if (!res.ok) throw new Error(`Failed to load providers (${res.status})`);
  return res.json();
}

export async function createProvider(
  data: CreateProviderRequest,
): Promise<ProviderPublic> {
  const res = await fetch("/api/providers", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  if (!res.ok) throw new Error(`Create provider failed (${res.status})`);
  return res.json();
}

export async function updateProvider(
  id: string,
  data: Partial<CreateProviderRequest>,
): Promise<ProviderPublic> {
  const res = await fetch(`/api/providers/${id}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  if (!res.ok) throw new Error(`Update provider failed (${res.status})`);
  return res.json();
}

export async function deleteProvider(id: string): Promise<void> {
  await fetch(`/api/providers/${id}`, { method: "DELETE" });
}

export async function testProvider(
  id: string,
): Promise<{ ok: boolean; error?: string }> {
  const res = await fetch(`/api/providers/${id}/test`, { method: "POST" });
  if (!res.ok) return { ok: false, error: `HTTP ${res.status}` };
  return res.json();
}

export async function testProviderInline(
  data: CreateProviderRequest,
): Promise<{ ok: boolean; error?: string }> {
  const res = await fetch("/api/providers/test", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  if (!res.ok) return { ok: false, error: `HTTP ${res.status}` };
  return res.json();
}

// ── Agents ──

export interface AgentConfig {
  id: string;
  name: string;
  provider_id: string;
  model: string;
  system_prompt?: string;
  temperature?: number;
  max_tokens?: number;
  mcp_server_ids?: string[];
  created_at: string;
  updated_at: string;
}

export interface CreateAgentRequest {
  name: string;
  provider_id: string;
  model: string;
  system_prompt?: string;
  temperature?: number;
  max_tokens?: number;
  mcp_server_ids?: string[];
}

export async function fetchAgents(): Promise<AgentConfig[]> {
  const res = await fetch("/api/agents");
  if (!res.ok) throw new Error(`Failed to load agents (${res.status})`);
  return res.json();
}

export async function createAgent(
  data: CreateAgentRequest,
): Promise<AgentConfig> {
  const res = await fetch("/api/agents", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  if (!res.ok) throw new Error(`Create agent failed (${res.status})`);
  return res.json();
}

export async function updateAgent(
  id: string,
  data: Partial<CreateAgentRequest> & {
    set_temperature?: boolean;
    set_max_tokens?: boolean;
    set_mcp_server_ids?: boolean;
  },
): Promise<AgentConfig> {
  const res = await fetch(`/api/agents/${id}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  if (!res.ok) throw new Error(`Update agent failed (${res.status})`);
  return res.json();
}

export async function deleteAgent(id: string): Promise<void> {
  await fetch(`/api/agents/${id}`, { method: "DELETE" });
}

export async function fetchActiveAgent(): Promise<string | null> {
  const res = await fetch("/api/agents/active");
  if (!res.ok) return null;
  const data = await res.json();
  return data.agent_id ?? null;
}

export async function setActiveAgent(
  agentId: string | null,
): Promise<void> {
  await fetch("/api/agents/active", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ agent_id: agentId }),
  });
}

// ── MCP Servers ──

export interface McpServerConfig {
  id: string;
  name: string;
  command: string;
  args: string[];
  env: Record<string, string>;
}

export interface CreateMcpServerRequest {
  name: string;
  command: string;
  args?: string[];
  env?: Record<string, string>;
}

export async function fetchMcpServers(): Promise<McpServerConfig[]> {
  const res = await fetch("/api/mcp-servers");
  if (!res.ok) throw new Error(`Failed to load MCP servers (${res.status})`);
  return res.json();
}

export async function createMcpServer(
  data: CreateMcpServerRequest,
): Promise<McpServerConfig> {
  const res = await fetch("/api/mcp-servers", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  if (!res.ok) throw new Error(`Create MCP server failed (${res.status})`);
  return res.json();
}

export async function updateMcpServer(
  id: string,
  data: Partial<CreateMcpServerRequest>,
): Promise<McpServerConfig> {
  const res = await fetch(`/api/mcp-servers/${id}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  if (!res.ok) throw new Error(`Update MCP server failed (${res.status})`);
  return res.json();
}

export async function deleteMcpServer(id: string): Promise<void> {
  await fetch(`/api/mcp-servers/${id}`, { method: "DELETE" });
}

export async function testMcpServerInline(
  data: CreateMcpServerRequest,
): Promise<{ ok: boolean; tools?: number; tool_names?: string[]; error?: string }> {
  const res = await fetch("/api/mcp-servers/test", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  if (!res.ok) return { ok: false, error: `HTTP ${res.status}` };
  return res.json();
}

// ── Workspaces ──

export interface WorkspaceConfig {
  id: string;
  name: string;
  path: string;
  created_at: string;
  updated_at: string;
}

export interface CreateWorkspaceRequest {
  name: string;
  path: string;
}

export async function fetchWorkspaces(): Promise<WorkspaceConfig[]> {
  const res = await fetch("/api/workspaces");
  if (!res.ok) throw new Error(`Failed to load workspaces (${res.status})`);
  return res.json();
}

export async function createWorkspace(
  data: CreateWorkspaceRequest,
): Promise<WorkspaceConfig> {
  const res = await fetch("/api/workspaces", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  if (!res.ok) throw new Error(`Create workspace failed (${res.status})`);
  return res.json();
}

export async function updateWorkspace(
  id: string,
  data: Partial<CreateWorkspaceRequest>,
): Promise<WorkspaceConfig> {
  const res = await fetch(`/api/workspaces/${id}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  if (!res.ok) throw new Error(`Update workspace failed (${res.status})`);
  return res.json();
}

export async function deleteWorkspace(id: string): Promise<void> {
  await fetch(`/api/workspaces/${id}`, { method: "DELETE" });
}

// ── Folder Picking ──

export interface BrowseEntry {
  name: string;
  path: string;
}

export interface BrowseResult {
  path: string;
  parent: string | null;
  entries: BrowseEntry[];
}

/** Browse directories on the server's filesystem. */
export async function browseDirectory(path?: string): Promise<BrowseResult> {
  const params = path ? `?path=${encodeURIComponent(path)}` : "";
  const res = await fetch(`/api/browse${params}`);
  if (!res.ok) throw new Error(`Browse failed (${res.status})`);
  return res.json();
}


// ── Model Discovery ──

export interface ModelInfo {
  id: string;
  name: string;
  group?: string;
}

export async function fetchProviderModels(
  providerId: string,
): Promise<ModelInfo[]> {
  const res = await fetch(`/api/providers/${providerId}/models`);
  if (!res.ok) return [];
  const data = await res.json();
  return data.models ?? [];
}
