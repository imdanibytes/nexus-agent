import { useThreadListStore } from "../stores/threadListStore";
import { useThreadStore } from "../stores/threadStore";
import { useTaskStore } from "../stores/taskStore";
import { useQuestionStore } from "../stores/questionStore";
import type {
  ChatMessage,
  MessagePart,
  TextPart,
  ToolCallPart,
  TimingSpan,
} from "../stores/threadStore";
import { eventBus, EventType } from "../runtime/event-bus";

// ── Persistence context ──

export interface TurnContext {
  conversationId: string;
  userMessage: ChatMessage;
  parentId: string | null;
  assistantMessageId: string;
  /** When true, user message already exists in repo -- only persist assistant */
  skipUserPersist?: boolean;
}

export async function consumeStream(
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
          } else if (name === "task_state_changed") {
            const val = event.value as {
              conversationId: string;
              plan: import("../types/tasks").Plan | null;
              tasks: Record<string, import("../types/tasks").Task>;
              mode?: import("../types/tasks").AgentMode;
            };
            useTaskStore.getState().setTaskState(val.conversationId, {
              plan: val.plan,
              tasks: val.tasks,
              mode: val.mode ?? "general",
            });
          } else if (name === "ask_user_pending") {
            const val = event.value as {
              questionId: string;
              toolCallId: string;
              question: string;
              type: "confirm" | "select" | "multi_select" | "text";
              options?: Array<{ value: string; label: string; description?: string }>;
              context?: string;
              placeholder?: string;
            };
            useQuestionStore.getState().setPending(val);
            useThreadStore.getState().setActivity(conversationId, "Waiting for your input...");
          } else if (name === "ask_user_answered") {
            const val = event.value as { toolCallId: string };
            useQuestionStore.getState().remove(val.toolCallId);
            useThreadStore.getState().setActivity(conversationId, "Thinking...");
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
          console.error("Stream error:", event.message);
          const details = event.details as
            | { kind: string; message: string; status_code?: number; retryable: boolean; provider: string }
            | undefined;
          const errorStatus = {
            type: "incomplete" as const,
            reason: "error",
            error: (event.message as string) ?? undefined,
            providerError: details ?? undefined,
          };
          // Persist partial messages from completed rounds before finalizing
          if (turnCtx && parts.length > 0) {
            persistTurnMessages(turnCtx, parts, errorStatus, metadata);
          }
          useThreadStore.getState().finalizeStreaming(conversationId, errorStatus, metadata);
          eventBus.endSubscription(conversationId);
          return;
        }
      }
    }

    if (drainTimeout) clearTimeout(drainTimeout);
    if (signal.aborted) {
      // Persist partial messages before finalizing (skip if nothing produced)
      if (turnCtx && parts.length > 0) {
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

    // Now finalize -- this makes the footer visible (BranchPicker, action bar)
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
