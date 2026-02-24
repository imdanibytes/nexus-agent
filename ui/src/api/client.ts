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
