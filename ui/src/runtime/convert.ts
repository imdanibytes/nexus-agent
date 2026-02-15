import type { ThreadMessageLike } from "@assistant-ui/react";
import type { Message } from "@/api/client.js";

/**
 * Convert our Message type to assistant-ui's ThreadMessageLike
 * for loading conversation history.
 */
export function convertToThreadMessage(msg: Message): ThreadMessageLike {
  if (msg.role === "user") {
    const text = msg.parts
      .filter((p): p is { type: "text"; text: string } => p.type === "text")
      .map((p) => p.text)
      .join("\n");
    return {
      role: "user",
      id: msg.id,
      createdAt: new Date(msg.timestamp),
      content: [{ type: "text", text }],
    };
  }

  // Assistant message: build content array preserving part order
  const content: NonNullable<Extract<ThreadMessageLike["content"], readonly unknown[]>[number]>[] = [];

  for (const part of msg.parts) {
    if (part.type === "text") {
      content.push({ type: "text" as const, text: part.text });
    } else if (part.type === "tool-call") {
      content.push({
        type: "tool-call" as const,
        toolCallId: part.id,
        toolName: part.name,
        args: part.args as any,
        result: part.result,
        isError: part.isError,
      });
    }
  }

  if (msg.uiSurfaces) {
    for (const surface of msg.uiSurfaces) {
      content.push({
        type: "tool-call" as const,
        toolCallId: surface.toolUseId,
        toolName: surface.name,
        args: surface.input as any,
        result: surface.response ?? undefined,
      });
    }
  }

  return {
    role: "assistant",
    id: msg.id,
    createdAt: new Date(msg.timestamp),
    content,
    status: { type: "complete", reason: "stop" },
    metadata: msg.profileName
      ? { custom: { profileName: msg.profileName } }
      : undefined,
  };
}
