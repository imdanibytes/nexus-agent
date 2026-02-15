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
  message: string
): AsyncGenerator<SseEvent> {
  const res = await fetch("/api/chat", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ conversationId, message }),
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
