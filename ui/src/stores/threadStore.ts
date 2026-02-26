import { create } from "zustand";
import { fetchConversation } from "../api/client";
import type { ConversationFull } from "../api/client";
import {
  buildChildrenMap,
  resolveActiveBranch,
  getBranchInfo,
  type MessageNode,
} from "../lib/message-tree";
import { snowflake } from "../lib/snowflake";
import { useTaskStore } from "./taskStore";
import { useUsageStore } from "./usageStore";

// ── Types ──

export type ToolCallStatus =
  | { type: "running" }
  | { type: "complete" }
  | { type: "incomplete"; reason: string; error?: unknown };

export type TextPart = { type: "text"; text: string };
export type ThinkingPart = { type: "thinking"; thinking: string };
export type ToolCallPart = {
  type: "tool-call";
  toolCallId: string;
  toolName: string;
  args: Record<string, unknown>;
  argsText?: string;
  result?: unknown;
  isError?: boolean;
  status?: ToolCallStatus;
};

export type ToolResultPart = {
  type: "tool-result";
  toolCallId: string;
  result: string;
  isError?: boolean;
};

export type MessagePart = TextPart | ThinkingPart | ToolCallPart | ToolResultPart;

export interface TimingSpan {
  id: string;
  name: string;
  parentId: string | null;
  startMs: number;
  endMs: number;
  durationMs: number;
  metadata?: Record<string, unknown>;
  markers?: Array<{ label: string; timeMs: number }>;
}

export interface ProviderErrorDetails {
  kind: string;
  message: string;
  status_code?: number;
  retryable: boolean;
  provider: string;
}

export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  parts: MessagePart[];
  createdAt: Date;
  status?: {
    type: "complete" | "incomplete" | "streaming";
    reason?: string;
    error?: unknown;
    providerError?: ProviderErrorDetails;
  };
  metadata?: {
    timingSpans?: TimingSpan[];
    agent?: {
      agent_id: string;
      agent_name: string;
      model: string;
    };
    synthetic?: boolean;
    source?: string;
  };
}

// ── Per-conversation state ──

export interface SealedSpan {
  index: number;
  summary: string;
  messageIds: string[];
  /** Original messages, loaded from repository when user clicks "load earlier" */
  messages?: ChatMessage[];
}

export interface ConvState {
  messages: ChatMessage[];
  /** Sealed conversation spans — compacted segments with summaries */
  sealedSpans: SealedSpan[];
  isStreaming: boolean;
  isLoadingHistory: boolean;
  activity: string | null;
  repository: MessageNode[];
  childrenMap: Record<string, string[]>;
  branchSelections: Record<string, number>;
}

export const EMPTY_CONV: ConvState = {
  messages: [],
  sealedSpans: [],
  isStreaming: false,
  isLoadingHistory: false,
  activity: null,
  repository: [],
  childrenMap: {},
  branchSelections: {},
};

// ── Store ──

interface ThreadState {
  conversations: Record<string, ConvState>;

  loadHistory: (convId: string) => Promise<void>;
  dropConversation: (convId: string) => void;
  appendUserMessage: (convId: string, text: string, id?: string) => ChatMessage;
  replaceMessages: (
    convId: string,
    messages: ChatMessage[],
    isStreaming: boolean,
  ) => void;
  startStreaming: (convId: string, id?: string) => string;
  updateStreamingParts: (
    convId: string,
    parts: MessagePart[],
    metadata?: ChatMessage["metadata"],
  ) => void;
  finalizeStreaming: (
    convId: string,
    status?: ChatMessage["status"],
    metadata?: ChatMessage["metadata"],
  ) => void;
  persistMessage: (
    convId: string,
    msg: ChatMessage,
    parentId: string | null,
  ) => void;
  navigateBranch: (
    convId: string,
    messageId: string,
    direction: "prev" | "next",
  ) => void;
  getLastMessageId: (convId: string) => string | null;
  setActivity: (convId: string, activity: string | null) => void;
  loadAllSpanMessages: (convId: string) => void;
  syncRepository: (
    convId: string,
    serverMessages: Array<{
      id: string;
      role: string;
      parts: Array<Record<string, unknown>>;
      timestamp: string;
      parent_id?: string | null;
      metadata?: Record<string, unknown> | null;
    }>,
    activePath?: string[],
  ) => void;
}

// Re-export for BranchPicker
export { getBranchInfo } from "../lib/message-tree";

// Message IDs use Snowflakes — time-sortable, client-generated, no server mismatch

function getConv(
  state: { conversations: Record<string, ConvState> },
  convId: string,
): ConvState {
  return state.conversations[convId] ?? { ...EMPTY_CONV };
}

function patchConv(
  state: { conversations: Record<string, ConvState> },
  convId: string,
  patch: Partial<ConvState>,
): { conversations: Record<string, ConvState> } {
  const prev = getConv(state, convId);
  return {
    conversations: {
      ...state.conversations,
      [convId]: { ...prev, ...patch },
    },
  };
}

/**
 * Merge consecutive assistant messages from a single agent turn into one.
 * Tool results from intermediate user messages (API plumbing) are folded
 * back into the assistant's ToolCall parts, matching the streaming format
 * where a single message accumulates all parts across rounds.
 */
function mergeAssistantTurns(messages: ChatMessage[]): ChatMessage[] {
  const result: ChatMessage[] = [];
  let i = 0;

  while (i < messages.length) {
    const msg = messages[i];

    if (msg.role !== "assistant") {
      result.push(msg);
      i++;
      continue;
    }

    // Start accumulating an assistant turn
    const mergedParts: MessagePart[] = [...msg.parts];
    let metadata = msg.metadata;
    let j = i + 1;

    while (j < messages.length) {
      const next = messages[j];

      // User message with only tool-result parts → fold results into tool calls
      if (
        next.role === "user" &&
        next.parts.length > 0 &&
        next.parts.every((p) => p.type === "tool-result")
      ) {
        for (const part of next.parts) {
          if (part.type === "tool-result") {
            const tr = part as ToolResultPart;
            const tcIdx = mergedParts.findIndex(
              (p) =>
                p.type === "tool-call" &&
                (p as ToolCallPart).toolCallId === tr.toolCallId,
            );
            if (tcIdx !== -1) {
              const tc = mergedParts[tcIdx] as ToolCallPart;
              mergedParts[tcIdx] = {
                ...tc,
                result: tr.result,
                isError: tr.isError,
                status: { type: "complete" },
              };
            }
          }
        }
        j++;
        continue;
      }

      // Another assistant message → next round in same turn, append its parts
      if (next.role === "assistant") {
        mergedParts.push(...next.parts);
        if (next.metadata) {
          metadata = { ...metadata, ...next.metadata };
        }
        j++;
        continue;
      }

      // Real user message (has text content) → turn boundary
      break;
    }

    result.push({
      ...msg,
      parts: mergedParts,
      metadata,
    });
    i = j;
  }

  return result;
}

/** Convert server message to client ChatMessage */
function toClientMessage(m: {
  id: string;
  role: string;
  parts: Array<Record<string, unknown>>;
  timestamp: string;
  parent_id?: string | null;
  metadata?: Record<string, unknown> | null;
}): ChatMessage {
  const msg: ChatMessage = {
    id: m.id,
    role: m.role as "user" | "assistant",
    parts: (m.parts ?? []).map((p): MessagePart => {
      if (p.type === "text") return { type: "text", text: p.text as string };
      if (p.type === "thinking")
        return { type: "thinking", thinking: p.thinking as string };
      if (p.type === "tool-result")
        return {
          type: "tool-result",
          toolCallId: (p.toolCallId ?? p.tool_call_id) as string,
          result: p.result as string,
          isError: (p.is_error ?? p.isError) as boolean | undefined,
        };
      return {
        type: "tool-call",
        toolCallId: (p.toolCallId ?? p.tool_call_id) as string,
        toolName: (p.toolName ?? p.tool_name) as string,
        args: (p.args as Record<string, unknown>) ?? {},
        result: p.result as string | undefined,
        isError: (p.is_error ?? p.isError) as boolean | undefined,
        status: { type: "complete" },
      };
    }),
    createdAt: new Date(m.timestamp),
  };
  if (m.metadata) {
    msg.metadata = m.metadata as ChatMessage["metadata"];
  }
  return msg;
}

/** Convert server response into repository nodes */
function serverToRepository(
  messages: Array<{
    id: string;
    role: string;
    parts: Array<Record<string, unknown>>;
    timestamp: string;
    parent_id?: string | null;
    metadata?: Record<string, unknown> | null;
  }>,
): MessageNode[] {
  return messages.map((m) => ({
    message: toClientMessage(m),
    parentId: m.parent_id ?? null,
  }));
}

/** Populate the usage store from server conversation data */
function hydrateUsage(convId: string, conv: ConversationFull): void {
  if (conv.usage) {
    useUsageStore.getState().setUsage(convId, {
      inputTokens: conv.usage.input_tokens,
      outputTokens: conv.usage.output_tokens,
      contextWindow: conv.usage.context_window,
      totalCost: conv.usage.total_cost ?? 0,
    });
  }
}

/** Populate the task store from server conversation data */
function hydrateTaskState(convId: string, conv: ConversationFull): void {
  if (conv.task_state) {
    useTaskStore.getState().setTaskState(convId, {
      plan: conv.task_state.plan,
      tasks: conv.task_state.tasks,
      mode: conv.task_state.mode ?? "general",
    });
  }
}

export const useThreadStore = create<ThreadState>((set, get) => ({
  conversations: {},

  loadHistory: async (convId) => {
    set((s) => patchConv(s, convId, { isLoadingHistory: true }));

    try {
      const conv = await fetchConversation(convId);
      if (!conv) {
        set((s) => patchConv(s, convId, { isLoadingHistory: false }));
        return;
      }

      const repo = serverToRepository(conv.messages ?? []);
      const childrenMap = buildChildrenMap(repo);
      const selections: Record<string, number> = {};

      // If server provides active_path, derive selections from it
      if (conv.active_path?.length) {
        const pathSet = new Set(conv.active_path);
        for (const [parentKey, children] of Object.entries(childrenMap)) {
          const idx = children.findIndex((id) => pathSet.has(id));
          if (idx !== -1) {
            selections[parentKey] = idx;
          }
        }
      }

      const historyMessages = mergeAssistantTurns(
        resolveActiveBranch(repo, childrenMap, selections),
      );

      // Parse sealed spans from server response
      const sealedSpans: SealedSpan[] = (conv.spans ?? [])
        .filter((s) => s.sealed_at != null && s.summary)
        .map((s) => ({
          index: s.index,
          summary: s.summary!,
          messageIds: s.message_ids,
        }));

      hydrateUsage(convId, conv);
      hydrateTaskState(convId, conv);

      set((s) => {
        const current = s.conversations[convId];
        // If streaming started while we were fetching (reconnect scenario),
        // prepend loaded history before the streaming message(s) so the user
        // sees the full conversation while the new response streams in.
        if (current?.isStreaming) {
          const streamingMsgs = current.messages;
          return patchConv(s, convId, {
            repository: repo,
            childrenMap,
            branchSelections: selections,
            messages: [...historyMessages, ...streamingMsgs],
            sealedSpans,
            isLoadingHistory: false,
          });
        }
        return patchConv(s, convId, {
          repository: repo,
          childrenMap,
          branchSelections: selections,
          messages: historyMessages,
          sealedSpans,
          isLoadingHistory: false,
        });
      });
    } catch {
      set((s) => patchConv(s, convId, { isLoadingHistory: false }));
    }
  },

  dropConversation: (convId) => {
    set((s) => {
      const { [convId]: _, ...rest } = s.conversations;
      return { conversations: rest };
    });
  },

  appendUserMessage: (convId, text, id) => {
    const msg: ChatMessage = {
      id: id ?? snowflake(),
      role: "user",
      parts: [{ type: "text", text }],
      createdAt: new Date(),
    };
    set((s) => {
      const conv = getConv(s, convId);
      return patchConv(s, convId, { messages: [...conv.messages, msg] });
    });
    return msg;
  },

  replaceMessages: (convId, messages, isStreaming) => {
    set((s) => patchConv(s, convId, { messages, isStreaming }));
  },

  startStreaming: (convId, id) => {
    const msgId = id ?? snowflake();
    const msg: ChatMessage = {
      id: msgId,
      role: "assistant",
      parts: [],
      createdAt: new Date(),
      status: { type: "streaming" },
    };
    set((s) => {
      const conv = getConv(s, convId);
      return patchConv(s, convId, {
        messages: [...conv.messages, msg],
        isStreaming: true,
      });
    });
    return msgId;
  },

  updateStreamingParts: (convId, parts, metadata) => {
    set((s) => {
      const conv = getConv(s, convId);
      const msgs = [...conv.messages];
      const last = msgs[msgs.length - 1];
      if (!last || last.role !== "assistant") return s;
      msgs[msgs.length - 1] = {
        ...last,
        parts,
        metadata: metadata ? { ...last.metadata, ...metadata } : last.metadata,
      };
      return patchConv(s, convId, { messages: msgs });
    });
  },

  finalizeStreaming: (convId, status, metadata) => {
    set((s) => {
      const conv = getConv(s, convId);
      const msgs = [...conv.messages];
      const last = msgs[msgs.length - 1];
      if (!last || last.role !== "assistant") {
        return patchConv(s, convId, { isStreaming: false });
      }
      msgs[msgs.length - 1] = {
        ...last,
        status: status ?? { type: "complete" },
        metadata: metadata ? { ...last.metadata, ...metadata } : last.metadata,
      };
      return patchConv(s, convId, {
        messages: msgs,
        isStreaming: false,
        activity: null,
      });
    });
  },

  persistMessage: (convId, msg, parentId) => {
    set((s) => {
      const conv = getConv(s, convId);
      const node: MessageNode = { message: msg, parentId };
      const repo = [...conv.repository, node];
      const childrenMap = buildChildrenMap(repo);
      return patchConv(s, convId, { repository: repo, childrenMap });
    });
  },

  navigateBranch: (convId, messageId, direction) => {
    const conv = getConv(get(), convId);
    const info = getBranchInfo(messageId, conv.repository, conv.childrenMap);
    if (!info || info.count <= 1) return;

    const newIndex =
      direction === "prev"
        ? Math.max(0, info.index - 1)
        : Math.min(info.count - 1, info.index + 1);

    if (newIndex === info.index) return;

    const newSelections = {
      ...conv.branchSelections,
      [info.parentKey]: newIndex,
    };
    const messages = mergeAssistantTurns(
      resolveActiveBranch(conv.repository, conv.childrenMap, newSelections),
    );
    set((s) =>
      patchConv(s, convId, { branchSelections: newSelections, messages }),
    );
  },

  getLastMessageId: (convId) => {
    const conv = get().conversations[convId];
    if (!conv || conv.messages.length === 0) return null;
    return conv.messages[conv.messages.length - 1].id;
  },

  setActivity: (convId, activity) => {
    set((s) => patchConv(s, convId, { activity }));
  },

  loadAllSpanMessages: (convId) => {
    const conv = get().conversations[convId];
    if (!conv) return;

    const repoMap = new Map(conv.repository.map((n) => [n.message.id, n.message]));

    set((s) =>
      patchConv(s, convId, {
        sealedSpans: conv.sealedSpans.map((span) => {
          if (span.messages) return span; // already loaded
          const msgs = span.messageIds
            .map((id) => repoMap.get(id))
            .filter((m): m is ChatMessage => m != null);
          return { ...span, messages: msgs };
        }),
      }),
    );
  },

  syncRepository: (convId, serverMessages, activePath) => {
    set((s) => {
      const conv = getConv(s, convId);
      if (conv.isStreaming) return s; // Don't clobber live data

      const repo = serverToRepository(serverMessages);
      const childrenMap = buildChildrenMap(repo);

      // Derive selections from server's active_path
      const selections: Record<string, number> = {};
      if (activePath?.length) {
        const pathSet = new Set(activePath);
        for (const [parentKey, children] of Object.entries(childrenMap)) {
          const idx = children.findIndex((id) => pathSet.has(id));
          if (idx !== -1) {
            selections[parentKey] = idx;
          }
        }
      }

      // Server selections take precedence (correct branch after regeneration)
      const mergedSelections = { ...conv.branchSelections, ...selections };
      const resolved = mergeAssistantTurns(
        resolveActiveBranch(repo, childrenMap, mergedSelections),
      );

      // Check if the displayed messages match (same IDs in same order).
      // With client-generated Snowflake IDs, the server uses the same IDs —
      // so for simple responses, IDs match and we skip the messages update
      // to avoid unnecessary React re-renders.
      const currentMsgs = conv.messages;
      const idsMatch =
        currentMsgs.length === resolved.length &&
        currentMsgs.every((m, i) => m.id === resolved[i].id);

      const patch: Partial<ConvState> = {
        repository: repo,
        childrenMap,
        branchSelections: mergedSelections,
      };

      if (idsMatch) {
        // Same messages — only merge metadata updates from server
        const metadataChanged = currentMsgs.some((m, i) => {
          const srv = resolved[i];
          return (
            srv.metadata &&
            JSON.stringify(srv.metadata) !== JSON.stringify(m.metadata)
          );
        });
        if (metadataChanged) {
          patch.messages = currentMsgs.map((m, i) => {
            const srv = resolved[i];
            return srv.metadata ? { ...m, metadata: srv.metadata } : m;
          });
        }
      } else {
        // IDs differ (multi-round turn, branch change) — use server data
        patch.messages = resolved;
      }

      return patchConv(s, convId, patch);
    });
  },
}));
