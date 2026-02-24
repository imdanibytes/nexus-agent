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

export type MessagePart = TextPart | ThinkingPart | ToolCallPart;

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

export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  parts: MessagePart[];
  createdAt: Date;
  status?: {
    type: "complete" | "incomplete" | "streaming";
    reason?: string;
    error?: unknown;
  };
  metadata?: {
    timingSpans?: TimingSpan[];
  };
}

// ── Per-conversation state ──

export interface ConvState {
  messages: ChatMessage[];
  isStreaming: boolean;
  isLoadingHistory: boolean;
  activity: string | null;
  repository: MessageNode[];
  childrenMap: Record<string, string[]>;
  branchSelections: Record<string, number>;
}

export const EMPTY_CONV: ConvState = {
  messages: [],
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
    });
  }
}

export const useThreadStore = create<ThreadState>((set, get) => ({
  conversations: {},

  loadHistory: async (convId) => {
    const existing = get().conversations[convId];
    if (existing?.isStreaming) return;

    set((s) => patchConv(s, convId, { isLoadingHistory: true }));

    try {
      const conv = await fetchConversation(convId);
      if (!conv) {
        set((s) => patchConv(s, convId, { isLoadingHistory: false }));
        return;
      }

      if (get().conversations[convId]?.isStreaming) {
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

      const messages = resolveActiveBranch(repo, childrenMap, selections);

      hydrateUsage(convId, conv);

      set((s) =>
        patchConv(s, convId, {
          repository: repo,
          childrenMap,
          branchSelections: selections,
          messages,
          isLoadingHistory: false,
        }),
      );
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
    const messages = resolveActiveBranch(
      conv.repository,
      conv.childrenMap,
      newSelections,
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
      const resolved = resolveActiveBranch(repo, childrenMap, mergedSelections);

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
