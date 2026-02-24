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
}

export interface ConversationFull {
  id: string;
  title: string;
  messages: ServerMessage[];
  active_path: string[];
  branch_info?: Record<string, string[]>;
  usage?: ConversationUsage;
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
    };

// ── Conversations ──

export async function fetchConversations(): Promise<ConversationMeta[]> {
  const res = await fetch("/api/conversations");
  if (!res.ok) return [];
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

// ── Providers ──

export type ProviderType = "anthropic" | "bedrock";

export interface ProviderPublic {
  id: string;
  name: string;
  type: ProviderType;
  endpoint?: string;
  has_api_key: boolean;
  aws_region?: string;
  has_aws_credentials: boolean;
  created_at: string;
  updated_at: string;
}

export interface CreateProviderRequest {
  name: string;
  type: ProviderType;
  endpoint?: string;
  api_key?: string;
  aws_region?: string;
  aws_access_key_id?: string;
  aws_secret_access_key?: string;
  aws_session_token?: string;
}

export async function fetchProviders(): Promise<ProviderPublic[]> {
  const res = await fetch("/api/providers");
  if (!res.ok) return [];
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

// ── Agents ──

export interface AgentConfig {
  id: string;
  name: string;
  provider_id: string;
  model: string;
  system_prompt?: string;
  temperature?: number;
  max_tokens?: number;
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
}

export async function fetchAgents(): Promise<AgentConfig[]> {
  const res = await fetch("/api/agents");
  if (!res.ok) return [];
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

// ── Model Discovery ──

export interface ModelInfo {
  id: string;
  name: string;
}

export async function fetchProviderModels(
  providerId: string,
): Promise<ModelInfo[]> {
  const res = await fetch(`/api/providers/${providerId}/models`);
  if (!res.ok) return [];
  const data = await res.json();
  return data.models ?? [];
}
