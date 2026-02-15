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

export interface SseEvent {
  event: string;
  data: unknown;
}

export async function* streamChat(
  conversationId: string | null,
  message: string,
  profileId?: string | null
): AsyncGenerator<SseEvent> {
  const res = await fetch("/api/chat", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ conversationId, message, profileId: profileId || undefined }),
  });

  if (!res.ok || !res.body) {
    throw new Error(`Chat request failed: ${res.status}`);
  }

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });
    const lines = buffer.split("\n");
    buffer = lines.pop() || "";

    let currentEvent = "";

    for (const line of lines) {
      if (line.startsWith("event: ")) {
        currentEvent = line.slice(7).trim();
      } else if (line.startsWith("data: ")) {
        const data = line.slice(6);
        try {
          yield { event: currentEvent || "message", data: JSON.parse(data) };
        } catch {
          yield { event: currentEvent || "message", data };
        }
        currentEvent = "";
      }
    }
  }
}

export async function respondToUiSurface(
  toolUseId: string,
  action: string,
  content: unknown
): Promise<void> {
  await fetch("/api/chat/respond", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ tool_use_id: toolUseId, action, content }),
  });
}

export interface ConversationMeta {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  messageCount: number;
}

export interface ConversationFull {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  messages: Message[];
}

export interface Message {
  id: string;
  role: "user" | "assistant";
  content: string;
  timestamp: number;
  toolCalls?: ToolCallInfo[];
  uiSurfaces?: UiSurfaceInfo[];
  profileId?: string;
  profileName?: string;
}

export interface AgentProfile {
  id: string;
  name: string;
  model: string;
  systemPrompt: string;
  avatar?: string;
  createdAt: number;
  updatedAt: number;
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

export async function renameConversation(id: string, title: string): Promise<void> {
  await fetch(`/api/conversations/${id}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ title }),
  });
}

// --- Profiles ---

export async function fetchProfiles(): Promise<AgentProfile[]> {
  const res = await fetch("/api/profiles");
  if (!res.ok) return [];
  return res.json();
}

export async function createProfile(data: {
  name: string;
  model: string;
  systemPrompt: string;
  avatar?: string;
}): Promise<AgentProfile> {
  const res = await fetch("/api/profiles", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  return res.json();
}

export async function updateProfile(
  id: string,
  data: Partial<Pick<AgentProfile, "name" | "model" | "systemPrompt" | "avatar">>
): Promise<AgentProfile> {
  const res = await fetch(`/api/profiles/${id}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
  return res.json();
}

export async function deleteProfile(id: string): Promise<void> {
  await fetch(`/api/profiles/${id}`, { method: "DELETE" });
}

export async function getActiveProfile(): Promise<{ profileId: string | null }> {
  const res = await fetch("/api/profiles/active");
  return res.json();
}

export async function setActiveProfile(profileId: string | null): Promise<void> {
  await fetch("/api/profiles/active", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ profileId }),
  });
}

// --- Discovery ---

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

// --- Settings ---

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
