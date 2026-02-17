import { create } from "zustand";
import {
  fetchConversations,
  createConversation,
  deleteConversation,
  renameConversation,
  type ConversationMeta,
} from "../api/client.js";

interface ThreadListState {
  threads: ConversationMeta[];
  activeThreadId: string | null;
  isLoading: boolean;

  loadThreads: () => Promise<void>;
  createThread: () => Promise<string>;
  deleteThread: (id: string) => Promise<void>;
  renameThread: (id: string, title: string) => Promise<void>;
  switchThread: (id: string) => void;
  updateThreadTitle: (id: string, title: string) => void;
  /** Bump updatedAt and re-sort so the thread floats to top */
  touchThread: (id: string) => void;
  /** Add a thread if it doesn't already exist (for MCP-created conversations) */
  ensureThread: (id: string, title: string) => void;
}

export const useThreadListStore = create<ThreadListState>((set) => ({
  threads: [],
  activeThreadId: null,
  isLoading: false,

  loadThreads: async () => {
    set({ isLoading: true });
    try {
      const convos = await fetchConversations();
      convos.sort((a, b) => b.updatedAt - a.updatedAt);
      set({ threads: convos, isLoading: false });
    } catch {
      set({ isLoading: false });
    }
  },

  createThread: async () => {
    const conv = await createConversation();
    const meta: ConversationMeta = {
      id: conv.id,
      title: conv.title,
      createdAt: Date.now(),
      updatedAt: Date.now(),
      messageCount: 0,
    };
    set((s) => ({
      threads: [meta, ...s.threads],
      activeThreadId: conv.id,
    }));
    return conv.id;
  },

  deleteThread: async (id) => {
    await deleteConversation(id);
    set((s) => {
      const threads = s.threads.filter((t) => t.id !== id);
      const activeThreadId =
        s.activeThreadId === id
          ? threads[0]?.id ?? null
          : s.activeThreadId;
      return { threads, activeThreadId };
    });
  },

  renameThread: async (id, title) => {
    await renameConversation(id, title);
    set((s) => ({
      threads: s.threads.map((t) =>
        t.id === id ? { ...t, title } : t,
      ),
    }));
  },

  switchThread: (id) => {
    set({ activeThreadId: id });
  },

  updateThreadTitle: (id, title) => {
    set((s) => {
      const threads = s.threads
        .map((t) => (t.id === id ? { ...t, title, updatedAt: Date.now() } : t))
        .sort((a, b) => b.updatedAt - a.updatedAt);
      return { threads };
    });
  },

  touchThread: (id) => {
    set((s) => {
      const threads = s.threads
        .map((t) => (t.id === id ? { ...t, updatedAt: Date.now() } : t))
        .sort((a, b) => b.updatedAt - a.updatedAt);
      return { threads };
    });
  },

  ensureThread: (id, title) => {
    set((s) => {
      if (s.threads.some((t) => t.id === id)) return s;
      const meta: ConversationMeta = {
        id,
        title,
        createdAt: Date.now(),
        updatedAt: Date.now(),
        messageCount: 0,
      };
      return { threads: [meta, ...s.threads] };
    });
  },
}));
