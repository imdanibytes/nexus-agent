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
import { useUsageStore } from "@/stores/usageStore.js";
import { fetchToolSettings } from "@/api/client.js";
import type { ConversationUsage } from "@/api/client.js";
import type { WireMessage } from "@/api/client.js";
import {
  eventBus,
  EventType,
  type AgUiEvent,
  type PendingToolCall,
} from "@/runtime/event-bus.js";

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
  _input: Record<string, unknown>,
): Promise<{ content: string; isError: boolean }> {
  switch (name) {
    case "_nexus_get_location": {
      try {
        const pos = await new Promise<GeolocationPosition>((resolve, reject) => {
          navigator.geolocation.getCurrentPosition(resolve, reject, {
            enableHighAccuracy: true,
            timeout: 10000,
          });
        });
        const { latitude, longitude, accuracy } = pos.coords;
        return {
          content: JSON.stringify({ latitude, longitude, accuracy }),
          isError: false,
        };
      } catch (err) {
        const msg = err instanceof GeolocationPositionError
          ? err.message
          : String(err);
        return { content: `Location access failed: ${msg}`, isError: true };
      }
    }
    default:
      return { content: `Unknown frontend tool: ${name}`, isError: true };
  }
}

// ── Frontend tool definitions (passed to server so the LLM knows about them) ──

const FRONTEND_TOOLS = [
  {
    name: "_nexus_get_location",
    description: "Get the user's current geographic location (latitude, longitude, accuracy).",
    input_schema: { type: "object" as const, properties: {}, required: [] },
  },
];

// ── REST helpers ──

async function startTurn(
  conversationId: string,
  messages: WireMessage[],
  agentId?: string,
): Promise<void> {
  const res = await fetch("/api/v1/turn", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ conversationId, messages, agentId, frontendTools: FRONTEND_TOOLS }),
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: "Turn request failed" }));
    throw new Error(body.error || `Turn request failed (${res.status})`);
  }
}

async function abortTurn(conversationId: string): Promise<void> {
  await fetch("/api/v1/turn/abort", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ conversationId }),
  }).catch(() => {});
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

interface ConsumeOptions {
  /** Skip the POST to /api/v1/turn (for MCP turns — server already started it) */
  skipPost?: boolean;
}

async function consumeStream(
  conversationId: string,
  wireMessages: WireMessage[],
  agentId: string | undefined,
  signal: AbortSignal,
  turnCtx?: TurnContext,
  options?: ConsumeOptions,
): Promise<void> {
  // Bump thread to top of list
  useThreadListStore.getState().touchThread(conversationId);

  const parts: MessagePart[] = [];
  const chatState = useChatStore.getState();
  const activeAgent = chatState.agents.find((a) => a.id === chatState.activeAgentId);
  let metadata: ChatMessage["metadata"] = {
    ...(activeAgent ? { profileName: activeAgent.name } : {}),
  };

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

  // Subscribe to SSE events for this conversation
  const stream = eventBus.subscribe(conversationId);

  // Fire-and-forget the turn POST — events arrive via SSE
  // (Skip for MCP turns — the server already started the turn)
  if (!options?.skipPost) {
    startTurn(conversationId, wireMessages, agentId).catch((err) => {
      console.error("Turn start failed:", err);
      useThreadStore.getState().finalizeStreaming(conversationId, {
        type: "incomplete",
        reason: "error",
        error: err.message,
      });
      eventBus.endSubscription(conversationId);
    });
  }

  try {
    for await (const event of stream) {
      if (signal.aborted) break;

      switch (event.type) {
        case EventType.TEXT_MESSAGE_START: {
          parts.push({ type: "text", text: "" });
          break;
        }

        case EventType.TEXT_MESSAGE_CONTENT: {
          const chunk = (event.delta as string) || "";
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

        case EventType.TOOL_CALL_START: {
          parts.push({
            type: "tool-call",
            toolCallId: event.toolCallId as string,
            toolName: event.toolCallName as string,
            args: {},
            argsText: "{}",
            status: { type: "running" },
          });
          pushToStore();
          break;
        }

        case EventType.TOOL_CALL_ARGS: {
          for (let i = parts.length - 1; i >= 0; i--) {
            const p = parts[i];
            if (p.type === "tool-call" && p.toolCallId === event.toolCallId) {
              const tc = p as ToolCallPart;
              parts[i] = {
                ...tc,
                argsText: (tc.argsText || "") + (event.delta as string || ""),
              };
              break;
            }
          }
          pushToStore();
          break;
        }

        case EventType.TOOL_CALL_RESULT: {
          const toolIdx = parts.findIndex(
            (p) =>
              p.type === "tool-call" && p.toolCallId === event.toolCallId,
          );
          if (toolIdx !== -1) {
            const tc = parts[toolIdx] as ToolCallPart;
            parts[toolIdx] = {
              ...tc,
              result: event.content as string,
              isError: (event.isError as boolean) || false,
              status: { type: "complete" },
            };
          }
          pushToStore();
          break;
        }

        case EventType.CUSTOM: {
          const name = event.name as string;

          if (name === "title_update") {
            const val = event.value as { title?: string };
            if (val?.title && conversationId) {
              useThreadListStore
                .getState()
                .updateThreadTitle(conversationId, val.title);
            }
          } else if (name === "timing") {
            const val = event.value as { spans?: import("@/stores/chatStore.js").TimingSpan[] };
            if (val?.spans) {
              metadata = { ...metadata, timingSpans: val.spans };
              pushToStore();
            }
          } else if (name === "usage") {
            const val = event.value as ConversationUsage;
            if (val) {
              useUsageStore.getState().setUsage(conversationId, val);
            }
          }
          break;
        }

        case EventType.RUN_FINISHED: {
          const result = event.result as {
            pendingToolCalls?: PendingToolCall[];
          } | undefined;

          const pending = result?.pendingToolCalls;

          if (pending && pending.length > 0) {
            // Frontend tools need execution — run ends, we execute and re-POST
            await handlePendingToolCalls(
              conversationId,
              wireMessages,
              parts,
              pending,
              agentId,
              signal,
              turnCtx,
              metadata,
            );
            return; // handlePendingToolCalls takes over from here
          }

          // Normal completion — just let the stream end
          eventBus.endSubscription(conversationId);
          break;
        }

        case EventType.RUN_ERROR: {
          console.error("Stream error:", event.message);
          useThreadStore.getState().finalizeStreaming(
            conversationId,
            {
              type: "incomplete",
              reason: "error",
              error: (event.message as string) ?? undefined,
            },
            metadata,
          );
          eventBus.endSubscription(conversationId);
          return;
        }
      }
    }

    // Explicit abort (Stop button)
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

// ── Frontend tool handling (end-run pattern) ──

async function handlePendingToolCalls(
  conversationId: string,
  previousWireMessages: WireMessage[],
  currentParts: MessagePart[],
  pending: PendingToolCall[],
  agentId: string | undefined,
  signal: AbortSignal,
  turnCtx: TurnContext | undefined,
  metadata: ChatMessage["metadata"],
): Promise<void> {
  if (signal.aborted) {
    useThreadStore.getState().finalizeStreaming(conversationId, {
      type: "incomplete",
      reason: "aborted",
    });
    return;
  }

  // Execute frontend tools locally
  const results: { toolCallId: string; content: string; isError: boolean }[] = [];

  for (const tc of pending) {
    const { content, isError } = await executeFrontendTool(tc.toolCallName, tc.args);
    results.push({ toolCallId: tc.toolCallId, content, isError });

    // Update the part in the UI with the result
    const idx = currentParts.findIndex(
      (p) => p.type === "tool-call" && p.toolCallId === tc.toolCallId,
    );
    if (idx !== -1) {
      const part = currentParts[idx] as ToolCallPart;
      currentParts[idx] = { ...part, result: content, isError, status: { type: "complete" } };
    }

    useThreadStore
      .getState()
      .updateStreamingParts(
        conversationId,
        currentParts.filter(
          (p) => !(p.type === "tool-call" && matchHiddenPattern(p.toolName)),
        ),
        metadata,
      );
  }

  // Build updated wire messages: previous + assistant (with all tool calls + results) + tool results
  const assistantText = currentParts
    .filter((p): p is TextPart => p.type === "text")
    .map((p) => p.text)
    .join("");

  const allToolCalls = currentParts
    .filter((p): p is ToolCallPart => p.type === "tool-call")
    .map((tc) => ({
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
    }));

  const updatedMessages: WireMessage[] = [
    ...previousWireMessages,
    {
      role: "assistant",
      content: assistantText,
      toolCalls: allToolCalls.length > 0 ? allToolCalls : undefined,
    },
  ];

  // Re-POST with updated history — the server continues the agent loop
  await consumeStream(conversationId, updatedMessages, agentId, signal, turnCtx);
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
    const wireMessages = toWireMessages(conv.messages);

    const controller = new AbortController();
    abortRef.current = controller;
    consumeStream(conversationId, wireMessages, agentId || undefined, controller.signal, {
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
      const wireMessages = toWireMessages(allMessages);

      const controller = new AbortController();
      abortRef.current = controller;
      consumeStream(conversationId, wireMessages, agentId || undefined, controller.signal, {
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
      const wireMessages = toWireMessages(messagesUpToUser);

      const controller = new AbortController();
      abortRef.current = controller;
      consumeStream(conversationId, wireMessages, agentId || undefined, controller.signal, {
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
      abortTurn(conversationId);
      eventBus.endSubscription(conversationId);
    }
    abortRef.current?.abort();
  }, []);

  // MCP turn handling — runs in background, no thread switching
  const pendingConvId = useMcpTurnStore((s) => s.pendingConvId);
  const pendingUserMessage = useMcpTurnStore((s) => s.pendingUserMessage);

  useEffect(() => {
    if (!pendingConvId || !pendingUserMessage) return;

    const store = useThreadStore.getState();
    store.appendUserMessage(pendingConvId, pendingUserMessage, {
      mcpSource: true,
    });
    store.startStreaming(pendingConvId);

    // Separate controller — doesn't interfere with user-initiated turns
    const controller = new AbortController();

    // Don't POST (server already started the turn), don't persist (server handles it)
    consumeStream(pendingConvId, [], undefined, controller.signal, undefined, {
      skipPost: true,
    }).finally(() => {
      // Reload full history from server to get properly persisted messages
      useThreadStore.getState().loadHistory(pendingConvId);
    });

    useMcpTurnStore.getState().clearPendingTurn();
  }, [pendingConvId, pendingUserMessage]);

  return { sendMessage, sendMessageFromEdit, regenerateResponse, abort, isStreaming };
}

let msgEditCounter = 0;
