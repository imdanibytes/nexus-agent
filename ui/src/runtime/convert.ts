import type { Message, MessagePart as ServerPart } from "../api/client.js";
import type { ChatMessage, MessagePart, ToolCallPart } from "../stores/threadStore.js";

/**
 * Convert server Message to our ChatMessage for loading history.
 */
export function convertToMessage(msg: Message): ChatMessage {
  const parts: MessagePart[] = [];

  for (const part of msg.parts) {
    if (part.type === "text") {
      parts.push({ type: "text", text: part.text });
    } else if (part.type === "tool-call") {
      parts.push({
        type: "tool-call",
        toolCallId: part.id,
        toolName: part.name,
        args: part.args,
        result: part.result,
        isError: part.isError,
        status: { type: "complete" },
      });
    }
  }

  if (msg.uiSurfaces) {
    for (const surface of msg.uiSurfaces) {
      parts.push({
        type: "tool-call",
        toolCallId: surface.toolUseId,
        toolName: surface.name,
        args: surface.input,
        result: surface.response ?? undefined,
        status: { type: "complete" },
      });
    }
  }

  return {
    id: msg.id,
    role: msg.role,
    parts,
    createdAt: new Date(msg.timestamp),
    status: msg.role === "assistant" ? { type: "complete" } : undefined,
    metadata: msg.profileName ? { profileName: msg.profileName } : undefined,
  };
}

/**
 * Convert our ChatMessage back to server Message format for persistence.
 */
export function toServerMessage(msg: ChatMessage): Message {
  const parts: ServerPart[] = [];

  for (const part of msg.parts) {
    if (part.type === "text") {
      parts.push({ type: "text", text: part.text });
    } else if (part.type === "tool-call") {
      const tc = part as ToolCallPart;
      parts.push({
        type: "tool-call",
        id: tc.toolCallId,
        name: tc.toolName,
        args: tc.args,
        result:
          typeof tc.result === "string"
            ? tc.result
            : tc.result !== undefined
              ? JSON.stringify(tc.result)
              : undefined,
        isError: tc.isError,
      });
    }
  }

  return {
    id: msg.id,
    role: msg.role,
    parts,
    timestamp: msg.createdAt.getTime(),
    profileName: msg.metadata?.profileName,
  };
}
