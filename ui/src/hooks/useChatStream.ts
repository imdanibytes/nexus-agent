import { useCallback, useEffect, useRef } from "react";
import { useThreadListStore } from "@/stores/threadListStore.js";
import { useThreadStore, EMPTY_CONV } from "@/stores/threadStore.js";
import type {
  ChatMessage,
  MessagePart,
  TextPart,
  ToolCallPart,
} from "@/stores/threadStore.js";
import { useChatStore } from "@/stores/chatStore.js";
import { useMcpTurnStore } from "@/stores/mcpTurnStore.js";
import { fetchToolSettings } from "@/api/client.js";
import type { WireMessage } from "@/api/client.js";
import { wsClient } from "@/runtime/ws-client.js";
import { turnRouter } from "@/runtime/turn-router.js";

// ── Hidden tool filtering ──

let cachedHiddenPatterns: string[] = ["_nexus_*"];
fetchToolSettings()
  .then((s) => {
    cachedHiddenPatterns = s.hiddenToolPatterns;
  })
  .catch(() => {});

function matchHiddenPattern(name: string): boolean {
  return cachedHiddenPatterns.some((pattern) => {
    const regex = new RegExp(
      "^" + pattern.replace(/\*/g, ".*").replace(/\?/g, ".") + "$",
    );
    return regex.test(name);
  });
}

// ── Wire format conversion ──

function toWireMessages(messages: ChatMessage[]): WireMessage[] {
  return messages
    .filter((m) => m.role === "user" || m.role === "assistant")
    .map((m) => {
      const text = m.parts
        .filter((p): p is TextPart => p.type === "text")
        .map((p) => p.text)
        .join("\n");

      if (m.role === "user") {
        return { role: "user" as const, content: text };
      }

      const toolCalls = m.parts
        .filter((p): p is ToolCallPart => p.type === "tool-call")
        .map((p) => ({
          id: p.toolCallId,
          name: p.toolName,
          args: p.args,
          result:
            typeof p.result === "string"
              ? p.result
              : p.result !== undefined
                ? JSON.stringify(p.result)
                : undefined,
          isError: p.isError,
        }));

      return {
        role: "assistant" as const,
        content: text,
        toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
      };
    });
}

// ── Frontend tool execution ──

async function executeFrontendTool(
  name: string,
  input: Record<string, unknown>,
): Promise<{ content: string; isError: boolean }> {
  switch (name) {
    case "_nexus_clipboard_read": {
      try {
        const text = await navigator.clipboard.readText();
        return { content: text, isError: false };
      } catch (err) {
        return { content: `Clipboard read failed: ${err}`, isError: true };
      }
    }
    case "_nexus_clipboard_write": {
      try {
        await navigator.clipboard.writeText((input.text as string) || "");
        return { content: "Written to clipboard", isError: false };
      } catch (err) {
        return { content: `Clipboard write failed: ${err}`, isError: true };
      }
    }
    default:
      return { content: `Unknown frontend tool: ${name}`, isError: true };
  }
}

// ── Persistence context ──

interface TurnContext {
  conversationId: string;
  userMessage: ChatMessage;
  parentId: string | null;
  assistantMessageId: string;
  /** When true, the user message already exists in the repo — only persist the assistant response */
  skipUserPersist?: boolean;
}

// ── Stream consumer ──

async function consumeStream(
  conversationId: string,
  signal: AbortSignal,
  turnCtx?: TurnContext,
): Promise<void> {
  const stream = turnRouter.createTurnStream(conversationId);
  const parts: MessagePart[] = [];
  let metadata: ChatMessage["metadata"] = {};

  /** Write streaming parts to this conversation's slot in the store */
  function pushToStore(): void {
    useThreadStore
      .getState()
      .updateStreamingParts(conversationId, filteredParts(), metadata);
  }

  function filteredParts(): MessagePart[] {
    return parts.filter((p) => {
      if (p.type === "tool-call" && matchHiddenPattern(p.toolName)) {
        return false;
      }
      return true;
    });
  }

  try {
    for await (const msg of stream) {
      if (signal.aborted) break;

      const data = msg.data || {};

      switch (msg.type) {
        case "text_start": {
          parts.push({ type: "text", text: "" });
          break;
        }

        case "text_delta": {
          const chunk = (data.text as string) || "";
          let found = false;
          for (let i = parts.length - 1; i >= 0; i--) {
            if (parts[i].type === "text") {
              (parts[i] as TextPart).text += chunk;
              found = true;
              break;
            }
          }
          if (!found) {
            parts.push({ type: "text", text: chunk });
          }
          pushToStore();
          break;
        }

        case "tool_start": {
          parts.push({
            type: "tool-call",
            toolCallId: data.id as string,
            toolName: data.name as string,
            args: {},
            argsText: "{}",
            status: { type: "running" },
          });
          pushToStore();
          break;
        }

        case "tool_input_delta": {
          for (let i = parts.length - 1; i >= 0; i--) {
            const p = parts[i];
            if (p.type === "tool-call") {
              const partial = (data.partial_json as string) || "";
              const tc = p as ToolCallPart;
              parts[i] = {
                ...tc,
                argsText: (tc.argsText || "") + partial,
              };
              break;
            }
          }
          pushToStore();
          break;
        }

        case "tool_result": {
          const toolIdx = parts.findIndex(
            (p) =>
              p.type === "tool-call" && p.toolCallId === data.id,
          );
          if (toolIdx !== -1) {
            const tc = parts[toolIdx] as ToolCallPart;
            parts[toolIdx] = {
              ...tc,
              result: data.content as string,
              isError: (data.is_error as boolean) || false,
              status: { type: "complete" },
            };
          }
          pushToStore();
          break;
        }

        case "tool_request": {
          const toolUseId = data.tool_use_id as string;
          const toolName = data.name as string;
          const input = (data.input ?? {}) as Record<string, unknown>;

          parts.push({
            type: "tool-call",
            toolCallId: toolUseId,
            toolName,
            args: input,
            argsText: JSON.stringify(input),
            status: { type: "running" },
          });
          pushToStore();

          // Execute locally and send result back
          executeFrontendTool(toolName, input)
            .then(({ content: result, isError }) => {
              wsClient.send({
                type: "tool_result",
                conversationId,
                data: { toolUseId, content: result, isError },
              });
            })
            .catch((err) => {
              wsClient.send({
                type: "tool_result",
                conversationId,
                data: { toolUseId, content: String(err), isError: true },
              });
            });
          break;
        }

        case "ui_surface": {
          parts.push({
            type: "tool-call",
            toolCallId: data.tool_use_id as string,
            toolName: data.name as string,
            args: data.input as Record<string, unknown>,
            argsText: JSON.stringify(data.input),
            status: { type: "complete" },
          });
          pushToStore();
          break;
        }

        case "title_update": {
          const title = data.title as string;
          if (title && conversationId) {
            useThreadListStore
              .getState()
              .updateThreadTitle(conversationId, title);
          }
          break;
        }

        case "timing": {
          const spans = data.spans as import("@/stores/chatStore.js").TimingSpan[];
          if (spans) {
            metadata = { ...metadata, timingSpans: spans };
            pushToStore();
          }
          break;
        }

        case "turn_end": {
          const returnedId = data.conversationId as string | undefined;
          if (returnedId) {
            const tls = useThreadListStore.getState();
            if (tls.activeThreadId !== returnedId) {
              tls.switchThread(returnedId);
            }
          }
          turnRouter.endTurn(conversationId);
          break;
        }

        case "error": {
          console.error("Stream error:", data.message);
          useThreadStore.getState().finalizeStreaming(
            conversationId,
            {
              type: "incomplete",
              reason: "error",
              error: (data.message as string) ?? undefined,
            },
            metadata,
          );
          return;
        }
      }
    }

    // Explicit abort (Stop button) — finalize as incomplete, don't persist
    if (signal.aborted) {
      useThreadStore.getState().finalizeStreaming(conversationId, {
        type: "incomplete",
        reason: "aborted",
      });
      return;
    }

    // Normal completion
    useThreadStore
      .getState()
      .finalizeStreaming(conversationId, { type: "complete" }, metadata);

    // Persist messages to the repository tree
    if (turnCtx) {
      const convId = turnCtx.conversationId;
      const store = useThreadStore.getState();

      if (!turnCtx.skipUserPersist) {
        await store.persistMessage(convId, turnCtx.userMessage, turnCtx.parentId);
      }

      // Build assistant message from local parts buffer
      const assistantMsg: ChatMessage = {
        id: turnCtx.assistantMessageId,
        role: "assistant",
        parts: filteredParts(),
        createdAt: new Date(),
        status: { type: "complete" },
        metadata,
      };

      await store.persistMessage(convId, assistantMsg, turnCtx.userMessage.id);
    }
  } catch (err) {
    if (signal.aborted) return;
    console.error("Chat stream error:", err);
    useThreadStore.getState().finalizeStreaming(
      conversationId,
      {
        type: "incomplete",
        reason: "error",
        error: String(err),
      },
      metadata,
    );
  }
}

// ── Hook ──

export function useChatStream(): {
  sendMessage: (text: string) => void;
  sendMessageFromEdit: (text: string, branchParentId: string | null) => void;
  regenerateResponse: (userMessageId: string) => void;
  abort: () => void;
  isStreaming: boolean;
} {
  const abortRef = useRef<AbortController | null>(null);
  const activeThreadId = useThreadListStore((s) => s.activeThreadId);
  const isStreaming = useThreadStore(
    (s) => s.conversations[activeThreadId ?? ""]?.isStreaming ?? false,
  );

  const sendMessage = useCallback(async (text: string) => {
    fetchToolSettings()
      .then((s) => {
        cachedHiddenPatterns = s.hiddenToolPatterns;
      })
      .catch(() => {});

    let conversationId = useThreadListStore.getState().activeThreadId;
    if (!conversationId) {
      conversationId = await useThreadListStore.getState().createThread();
    }

    const store = useThreadStore.getState();
    const parentId = store.getLastMessageId(conversationId);
    const userMessage = store.appendUserMessage(conversationId, text);
    const assistantMessageId = store.startStreaming(conversationId);

    const agentId = useChatStore.getState().activeAgentId;
    const conv = useThreadStore.getState().conversations[conversationId] ?? EMPTY_CONV;

    wsClient.send({
      type: "start_turn",
      conversationId,
      data: {
        messages: toWireMessages(conv.messages),
        agentId: agentId || undefined,
      } as unknown as Record<string, unknown>,
    });

    const controller = new AbortController();
    abortRef.current = controller;
    consumeStream(conversationId, controller.signal, {
      conversationId,
      userMessage,
      parentId,
      assistantMessageId,
    });
  }, []);

  const sendMessageFromEdit = useCallback(
    async (text: string, branchParentId: string | null) => {
      fetchToolSettings()
        .then((s) => {
          cachedHiddenPatterns = s.hiddenToolPatterns;
        })
        .catch(() => {});

      let conversationId = useThreadListStore.getState().activeThreadId;
      if (!conversationId) {
        conversationId = await useThreadListStore.getState().createThread();
      }

      const conv = useThreadStore.getState().conversations[conversationId] ?? EMPTY_CONV;

      // Find messages up to the branch point
      let branchMessages: ChatMessage[] = [];
      if (branchParentId) {
        const idx = conv.messages.findIndex((m) => m.id === branchParentId);
        if (idx !== -1) {
          branchMessages = conv.messages.slice(0, idx + 1);
        }
      }

      const userMessage: ChatMessage = {
        id: `msg-${Date.now()}-${++msgEditCounter}`,
        role: "user",
        parts: [{ type: "text", text }],
        createdAt: new Date(),
      };

      const streamingMsg: ChatMessage = {
        id: `msg-${Date.now()}-${++msgEditCounter}`,
        role: "assistant",
        parts: [],
        createdAt: new Date(),
        status: { type: "streaming" },
      };

      useThreadStore.getState().replaceMessages(
        conversationId,
        [...branchMessages, userMessage, streamingMsg],
        true,
      );

      const agentId = useChatStore.getState().activeAgentId;
      const allMessages = [...branchMessages, userMessage];

      wsClient.send({
        type: "start_turn",
        conversationId,
        data: {
          messages: toWireMessages(allMessages),
          agentId: agentId || undefined,
        } as unknown as Record<string, unknown>,
      });

      const controller = new AbortController();
      abortRef.current = controller;
      consumeStream(conversationId, controller.signal, {
        conversationId,
        userMessage,
        parentId: branchParentId,
        assistantMessageId: streamingMsg.id,
      });
    },
    [],
  );

  const regenerateResponse = useCallback(
    async (userMessageId: string) => {
      fetchToolSettings()
        .then((s) => {
          cachedHiddenPatterns = s.hiddenToolPatterns;
        })
        .catch(() => {});

      const conversationId = useThreadListStore.getState().activeThreadId;
      if (!conversationId) return;

      const conv = useThreadStore.getState().conversations[conversationId] ?? EMPTY_CONV;

      const userIdx = conv.messages.findIndex((m) => m.id === userMessageId);
      if (userIdx === -1) return;
      const userMessage = conv.messages[userIdx];

      const messagesUpToUser = conv.messages.slice(0, userIdx + 1);

      const streamingMsg: ChatMessage = {
        id: `msg-${Date.now()}-${++msgEditCounter}`,
        role: "assistant",
        parts: [],
        createdAt: new Date(),
        status: { type: "streaming" },
      };

      useThreadStore.getState().replaceMessages(
        conversationId,
        [...messagesUpToUser, streamingMsg],
        true,
      );

      const agentId = useChatStore.getState().activeAgentId;

      wsClient.send({
        type: "start_turn",
        conversationId,
        data: {
          messages: toWireMessages(messagesUpToUser),
          agentId: agentId || undefined,
        } as unknown as Record<string, unknown>,
      });

      const controller = new AbortController();
      abortRef.current = controller;
      consumeStream(conversationId, controller.signal, {
        conversationId,
        userMessage,
        parentId: userIdx > 0 ? conv.messages[userIdx - 1].id : null,
        assistantMessageId: streamingMsg.id,
        skipUserPersist: true,
      });
    },
    [],
  );

  const abort = useCallback(() => {
    const conversationId = useThreadListStore.getState().activeThreadId;
    if (conversationId) {
      wsClient.send({ type: "abort_turn", conversationId });
      turnRouter.abort(conversationId);
    }
    abortRef.current?.abort();
  }, []);

  // MCP turn handling
  const pendingConvId = useMcpTurnStore((s) => s.pendingConvId);
  const pendingUserMessage = useMcpTurnStore((s) => s.pendingUserMessage);

  useEffect(() => {
    if (!pendingConvId || !pendingUserMessage) return;
    if (pendingConvId !== activeThreadId) return;

    const store = useThreadStore.getState();
    const parentId = store.getLastMessageId(pendingConvId);
    const userMessage = store.appendUserMessage(pendingConvId, pendingUserMessage, {
      mcpSource: true,
    });
    const assistantMessageId = store.startStreaming(pendingConvId);

    const controller = new AbortController();
    abortRef.current = controller;
    consumeStream(pendingConvId, controller.signal, {
      conversationId: pendingConvId,
      userMessage,
      parentId,
      assistantMessageId,
    });
    useMcpTurnStore.getState().clearPendingTurn();
  }, [pendingConvId, pendingUserMessage, activeThreadId]);

  return { sendMessage, sendMessageFromEdit, regenerateResponse, abort, isStreaming };
}

let msgEditCounter = 0;
