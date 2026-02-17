import { create } from "zustand";
import { fetchConversation, appendRepositoryMessage } from "../api/client.js";
import { convertToMessage, toServerMessage } from "../runtime/convert.js";
import {
  buildChildrenMap,
  resolveActiveBranch,
  getBranchInfo,
  parseRepository,
  type MessageNode,
} from "../lib/message-tree.js";
import type { TimingSpan } from "./chatStore.js";

// ── Types ──

export type TextPart = { type: "text"; text: string };

export type ToolCallStatus =
  | { type: "running" }
  | { type: "complete" }
  | { type: "incomplete"; reason: string; error?: unknown };

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

export type MessagePart = TextPart | ToolCallPart;

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
    mcpSource?: boolean;
    timingSpans?: TimingSpan[];
    profileName?: string;
  };
}

// ── Per-conversation state ──

export interface ConvState {
  messages: ChatMessage[];
  isStreaming: boolean;
  isLoadingHistory: boolean;
  repository: MessageNode[];
  childrenMap: Record<string, string[]>;
  branchSelections: Record<string, number>;
}

/** Stable empty default — same reference every time, avoids re-renders */
export const EMPTY_CONV: ConvState = {
  messages: [],
  isStreaming: false,
  isLoadingHistory: false,
  repository: [],
  childrenMap: {},
  branchSelections: {},
};

// ── Store ──

interface ThreadState {
  conversations: Record<string, ConvState>;

  loadHistory: (convId: string) => Promise<void>;
  dropConversation: (convId: string) => void;
  appendUserMessage: (
    convId: string,
    text: string,
    metadata?: { mcpSource?: boolean },
  ) => ChatMessage;
  startStreaming: (convId: string) => string;
  replaceMessages: (
    convId: string,
    messages: ChatMessage[],
    isStreaming: boolean,
  ) => void;
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

  // Persistence
  persistMessage: (
    convId: string,
    msg: ChatMessage,
    parentId: string | null,
  ) => Promise<void>;

  // Branch navigation
  navigateBranch: (
    convId: string,
    messageId: string,
    direction: "prev" | "next",
  ) => void;
  getLastMessageId: (convId: string) => string | null;
}

let msgCounter = 0;
function nextId(): string {
  return `msg-${Date.now()}-${++msgCounter}`;
}

/** Read a conversation's state, or return a fresh default */
function getConv(
  state: { conversations: Record<string, ConvState> },
  convId: string,
): ConvState {
  return state.conversations[convId] ?? { ...EMPTY_CONV };
}

/** Immutably update a single conversation's fields */
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

export const useThreadStore = create<ThreadState>((set, get) => ({
  conversations: {},

  loadHistory: async (convId) => {
    // If this conversation is actively streaming, don't blow it away
    const existing = get().conversations[convId];
    if (existing?.isStreaming) return;

    set((s) => patchConv(s, convId, { isLoadingHistory: true }));

    try {
      const conv = await fetchConversation(convId);
      if (!conv) {
        set((s) => patchConv(s, convId, { isLoadingHistory: false }));
        return;
      }

      // Re-check: if streaming started while we were fetching, don't overwrite
      if (get().conversations[convId]?.isStreaming) {
        set((s) => patchConv(s, convId, { isLoadingHistory: false }));
        return;
      }

      if (conv.repository?.messages?.length) {
        const repo = parseRepository(conv.repository.messages);
        const childrenMap = buildChildrenMap(repo);
        const selections: Record<string, number> = {};
        const messages = resolveActiveBranch(repo, childrenMap, selections);
        set((s) =>
          patchConv(s, convId, {
            repository: repo,
            childrenMap,
            branchSelections: selections,
            messages,
            isLoadingHistory: false,
          }),
        );
      } else {
        const messages = (conv.messages ?? []).map(convertToMessage);
        set((s) => patchConv(s, convId, { messages, isLoadingHistory: false }));
      }
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

  appendUserMessage: (convId, text, metadata) => {
    const msg: ChatMessage = {
      id: nextId(),
      role: "user",
      parts: [{ type: "text", text }],
      createdAt: new Date(),
      metadata: metadata ? { mcpSource: metadata.mcpSource } : undefined,
    };
    set((s) => {
      const conv = getConv(s, convId);
      return patchConv(s, convId, { messages: [...conv.messages, msg] });
    });
    return msg;
  },

  startStreaming: (convId) => {
    const id = nextId();
    const msg: ChatMessage = {
      id,
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
    return id;
  },

  replaceMessages: (convId, messages, isStreaming) => {
    set((s) => patchConv(s, convId, { messages, isStreaming }));
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
      return patchConv(s, convId, { messages: msgs, isStreaming: false });
    });
  },

  persistMessage: async (convId, msg, parentId) => {
    const serverMsg = toServerMessage(msg);
    const node: MessageNode = { message: serverMsg, parentId };
    set((s) => {
      const conv = getConv(s, convId);
      const repo = [...conv.repository, node];
      const childrenMap = buildChildrenMap(repo);
      return patchConv(s, convId, { repository: repo, childrenMap });
    });

    try {
      await appendRepositoryMessage(convId, serverMsg, parentId);
    } catch (err) {
      console.error("Failed to persist message:", err);
    }
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
    set((s) => patchConv(s, convId, { branchSelections: newSelections, messages }));
  },

  getLastMessageId: (convId) => {
    const conv = get().conversations[convId];
    if (!conv || conv.messages.length === 0) return null;
    return conv.messages[conv.messages.length - 1].id;
  },
}));
