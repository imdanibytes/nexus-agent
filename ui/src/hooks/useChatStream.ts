import { useCallback, useEffect, useRef } from "react";
import { useThreadListStore } from "../stores/threadListStore";
import { useThreadStore, EMPTY_CONV } from "../stores/threadStore";
import { useUsageStore } from "../stores/usageStore";
import type {
  ChatMessage,
  MessagePart,
  TextPart,
  ToolCallPart,
  TimingSpan,
} from "../stores/threadStore";
import { eventBus, EventType } from "../runtime/event-bus";
import { startChat, branchChat, regenerateChat, abortChat, fetchConversation } from "../api/client";
import { snowflake } from "../lib/snowflake";

// ── Persistence context ──

interface TurnContext {
  conversationId: string;
  userMessage: ChatMessage;
  parentId: string | null;
  assistantMessageId: string;
  /** When true, user message already exists in repo — only persist assistant */
  skipUserPersist?: boolean;
}

// ── Stream consumer ──

async function consumeStream(
  conversationId: string,
  signal: AbortSignal,
  turnCtx?: TurnContext,
): Promise<void> {
  useThreadListStore.getState().touchThread(conversationId);

  const parts: MessagePart[] = [];
  let metadata: ChatMessage["metadata"] = {};

  function pushToStore(): void {
    useThreadStore
      .getState()
      .updateStreamingParts(conversationId, [...parts], metadata);
  }

  const stream = eventBus.subscribe(conversationId);

  let currentRunId: string | null = null;
  let draining = false;
  let drainTimeout: ReturnType<typeof setTimeout> | null = null;

  try {
    for await (const event of stream) {
      if (signal.aborted && !draining) {
        draining = true;
        useThreadStore.getState().finalizeStreaming(conversationId, {
          type: "incomplete",
          reason: "aborted",
        }, metadata);
        drainTimeout = setTimeout(() => {
          eventBus.endSubscription(conversationId);
        }, 5_000);
      }

      if (draining) {
        if (
          event.type !== EventType.CUSTOM &&
          event.type !== EventType.RUN_FINISHED &&
          event.type !== EventType.RUN_ERROR &&
          event.type !== EventType.RUN_STARTED
        ) {
          continue;
        }
      }

      if (currentRunId && event.runId && event.runId !== currentRunId) {
        continue;
      }

      switch (event.type) {
        case EventType.RUN_STARTED: {
          currentRunId = (event.runId as string) ?? null;
          break;
        }

        case EventType.TEXT_MESSAGE_START: {
          parts.push({ type: "text", text: "" });
          useThreadStore.getState().setActivity(conversationId, null);
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
            argsText: "",
            status: { type: "running" },
          });
          useThreadStore.getState().setActivity(
            conversationId,
            `Using ${event.toolCallName as string}...`,
          );
          pushToStore();
          break;
        }

        case EventType.TOOL_CALL_ARGS: {
          for (let i = parts.length - 1; i >= 0; i--) {
            const p = parts[i];
            if (
              p.type === "tool-call" &&
              p.toolCallId === event.toolCallId
            ) {
              const tc = p as ToolCallPart;
              parts[i] = {
                ...tc,
                argsText:
                  (tc.argsText || "") + ((event.delta as string) || ""),
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
              p.type === "tool-call" &&
              p.toolCallId === event.toolCallId,
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
          useThreadStore
            .getState()
            .setActivity(conversationId, "Thinking...");
          pushToStore();
          break;
        }

        case EventType.CUSTOM: {
          const name = event.name as string;
          if (name === "thinking_start") {
            parts.push({ type: "thinking", thinking: "" });
            useThreadStore
              .getState()
              .setActivity(conversationId, "Thinking deeply...");
            pushToStore();
          } else if (name === "thinking_delta") {
            const val = event.value as { delta?: string };
            if (val?.delta) {
              for (let i = parts.length - 1; i >= 0; i--) {
                if (parts[i].type === "thinking") {
                  (
                    parts[i] as { type: "thinking"; thinking: string }
                  ).thinking += val.delta;
                  break;
                }
              }
              pushToStore();
            }
          } else if (name === "thinking_end") {
            useThreadStore.getState().setActivity(conversationId, null);
          } else if (name === "timing") {
            const val = event.value as { spans?: TimingSpan[] };
            if (val?.spans) {
              metadata = { ...metadata, timingSpans: val.spans };
              pushToStore();
            }
          }
          break;
        }

        case EventType.RUN_FINISHED: {
          if (!currentRunId) break;
          useThreadStore.getState().setActivity(conversationId, null);
          eventBus.endSubscription(conversationId);
          break;
        }

        case EventType.RUN_ERROR: {
          if (!currentRunId) break;
          console.error("Stream error:", event.message);
          useThreadStore.getState().finalizeStreaming(conversationId, {
            type: "incomplete",
            reason: "error",
            error: (event.message as string) ?? undefined,
          }, metadata);
          eventBus.endSubscription(conversationId);
          return;
        }
      }
    }

    if (drainTimeout) clearTimeout(drainTimeout);
    if (signal.aborted) {
      // Persist partial messages before finalizing
      if (turnCtx) {
        persistTurnMessages(turnCtx, parts, {
          type: "incomplete",
          reason: "aborted",
        }, metadata);
      }
      if (!draining) {
        useThreadStore.getState().finalizeStreaming(conversationId, {
          type: "incomplete",
          reason: "aborted",
        }, metadata);
      }
      return;
    }

    // Persist messages to client-side repository tree BEFORE finalizing,
    // so BranchPicker data is ready when the footer becomes visible.
    if (turnCtx) {
      persistTurnMessages(turnCtx, parts, { type: "complete" }, metadata);
    }

    // Now finalize — this makes the footer visible (BranchPicker, action bar)
    useThreadStore
      .getState()
      .finalizeStreaming(conversationId, { type: "complete" }, metadata);
  } catch (err) {
    if (signal.aborted) return;
    console.error("Chat stream error:", err);
    useThreadStore.getState().finalizeStreaming(conversationId, {
      type: "incomplete",
      reason: "error",
      error: String(err),
    }, metadata);
  }
}

/** Persist user + assistant messages to the client-side repository tree */
function persistTurnMessages(
  ctx: TurnContext,
  parts: MessagePart[],
  status: ChatMessage["status"],
  metadata: ChatMessage["metadata"],
): void {
  const store = useThreadStore.getState();

  if (!ctx.skipUserPersist) {
    store.persistMessage(ctx.conversationId, ctx.userMessage, ctx.parentId);
  }

  const assistantMsg: ChatMessage = {
    id: ctx.assistantMessageId,
    role: "assistant",
    parts: [...parts],
    createdAt: new Date(),
    status,
    metadata,
  };
  store.persistMessage(ctx.conversationId, assistantMsg, ctx.userMessage.id);
}

// ── Repo sync ──

/**
 * Sync the client-side repository with the server's authoritative data.
 * Replaces temp IDs with server UUIDs and ensures metadata (timing) persists.
 * Preserves the current branch selection by mapping old IDs → new IDs.
 */
async function syncRepoFromServer(conversationId: string): Promise<void> {
  try {
    const conv = await fetchConversation(conversationId);
    if (!conv?.messages?.length) return;

    // Hydrate usage from persisted data
    if (conv.usage) {
      useUsageStore.getState().setUsage(conversationId, {
        inputTokens: conv.usage.input_tokens,
        outputTokens: conv.usage.output_tokens,
        contextWindow: conv.usage.context_window,
      });
    }

    const store = useThreadStore.getState();
    store.syncRepository(conversationId, conv.messages, conv.active_path);
  } catch {
    // Non-critical — client repo still works, just might have temp IDs
  }
}

// ── Hook ──

export function useChatStream(): {
  sendMessage: (text: string) => void;
  branchMessage: (messageId: string, text: string) => void;
  regenerate: (userMessageId: string) => void;
  abort: () => void;
  isStreaming: boolean;
} {
  const abortRef = useRef<AbortController | null>(null);
  const activeThreadId = useThreadListStore((s) => s.activeThreadId);
  const isStreaming = useThreadStore(
    (s) => s.conversations[activeThreadId ?? ""]?.isStreaming ?? false,
  );

  // Broadcast handlers (arrive after RUN_FINISHED)
  useEffect(() => {
    const unsubTitle = eventBus.on("title_update", (event) => {
      const val = event.value as { title?: string };
      if (val?.title && event.threadId) {
        useThreadListStore
          .getState()
          .updateThreadTitle(event.threadId, val.title);
      }
    });

    const unsubUsage = eventBus.on("usage_update", (event) => {
      const val = event.value as {
        inputTokens?: number;
        outputTokens?: number;
        contextWindow?: number;
      };
      const threadId = event.threadId as string | undefined;
      if (threadId && val) {
        useUsageStore.getState().setUsage(threadId, {
          inputTokens: val.inputTokens ?? 0,
          outputTokens: val.outputTokens ?? 0,
          contextWindow: val.contextWindow ?? 200_000,
        });
      }
    });

    // Sync repo with server after conversation is saved (ensures timing
    // persists, temp IDs are replaced with server UUIDs, and branches resolve
    // correctly across refreshes).
    const unsubUpdated = eventBus.on("conversation_updated", (event) => {
      const threadId = event.threadId as string | undefined;
      if (!threadId) return;
      const store = useThreadStore.getState();
      // Only sync if we're NOT currently streaming (avoid clobbering live data)
      if (store.conversations[threadId]?.isStreaming) return;
      syncRepoFromServer(threadId);
    });

    return () => {
      unsubTitle();
      unsubUsage();
      unsubUpdated();
    };
  }, []);

  const sendMessage = useCallback(async (text: string) => {
    let conversationId = useThreadListStore.getState().activeThreadId;
    if (!conversationId) {
      conversationId = await useThreadListStore.getState().createThread();
    }

    // Generate Snowflake IDs upfront — client owns the IDs, server uses them
    const userMsgId = snowflake();
    const assistantMsgId = snowflake();

    const controller = new AbortController();
    abortRef.current = controller;

    try {
      await startChat(conversationId, text, userMsgId, assistantMsgId);
    } catch (err: unknown) {
      console.error("Turn start failed:", err);
      return;
    }

    const store = useThreadStore.getState();
    const parentId = store.getLastMessageId(conversationId);
    const userMessage = store.appendUserMessage(conversationId, text, userMsgId);
    store.startStreaming(conversationId, assistantMsgId);

    consumeStream(conversationId, controller.signal, {
      conversationId,
      userMessage,
      parentId,
      assistantMessageId: assistantMsgId,
    });
  }, []);

  const branchMessage = useCallback(async (messageId: string, text: string) => {
    const conversationId = useThreadListStore.getState().activeThreadId;
    if (!conversationId) return;

    const store = useThreadStore.getState();
    const conv = store.conversations[conversationId] ?? { ...EMPTY_CONV };
    const branchIdx = conv.messages.findIndex((m) => m.id === messageId);
    if (branchIdx === -1) return;

    // Parent of the edited message (for tree placement)
    const parentId = branchIdx > 0 ? conv.messages[branchIdx - 1].id : null;
    const kept = conv.messages.slice(0, branchIdx);

    const userMsgId = snowflake();
    const assistantMsgId = snowflake();

    const controller = new AbortController();
    abortRef.current = controller;

    try {
      await branchChat(conversationId, messageId, text, userMsgId, assistantMsgId);
    } catch (err: unknown) {
      console.error("Branch start failed:", err);
      return;
    }

    const userMessage: ChatMessage = {
      id: userMsgId,
      role: "user",
      parts: [{ type: "text", text }],
      createdAt: new Date(),
    };
    const streamingMsg: ChatMessage = {
      id: assistantMsgId,
      role: "assistant",
      parts: [],
      createdAt: new Date(),
      status: { type: "streaming" },
    };

    // Atomic replace
    useThreadStore.getState().replaceMessages(
      conversationId,
      [...kept, userMessage, streamingMsg],
      true,
    );

    consumeStream(conversationId, controller.signal, {
      conversationId,
      userMessage,
      parentId,
      assistantMessageId: assistantMsgId,
    });
  }, []);

  const regenerate = useCallback(async (userMessageId: string) => {
    const conversationId = useThreadListStore.getState().activeThreadId;
    if (!conversationId) return;

    const store = useThreadStore.getState();
    const conv = store.conversations[conversationId] ?? { ...EMPTY_CONV };

    const userIdx = conv.messages.findIndex((m) => m.id === userMessageId);
    if (userIdx === -1) return;
    const userMessage = conv.messages[userIdx];

    const assistantMsgId = snowflake();
    const messagesUpToUser = conv.messages.slice(0, userIdx + 1);
    const streamingMsg: ChatMessage = {
      id: assistantMsgId,
      role: "assistant",
      parts: [],
      createdAt: new Date(),
      status: { type: "streaming" },
    };

    const controller = new AbortController();
    abortRef.current = controller;

    // Tell server to regenerate (resets active_path, spawns new agent turn)
    try {
      await regenerateChat(conversationId, userMessageId, assistantMsgId);
    } catch (err: unknown) {
      console.error("Regenerate failed:", err);
      return;
    }

    useThreadStore.getState().replaceMessages(
      conversationId,
      [...messagesUpToUser, streamingMsg],
      true,
    );

    consumeStream(conversationId, controller.signal, {
      conversationId,
      userMessage,
      parentId: userIdx > 0 ? conv.messages[userIdx - 1].id : null,
      assistantMessageId: assistantMsgId,
      skipUserPersist: true,
    });
  }, []);

  const abort = useCallback(() => {
    const conversationId = useThreadListStore.getState().activeThreadId;
    if (conversationId) {
      abortChat(conversationId);
    }
    abortRef.current?.abort();
  }, []);

  return { sendMessage, branchMessage, regenerate, abort, isStreaming };
}
