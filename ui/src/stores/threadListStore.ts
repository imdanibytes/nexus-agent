import { create } from "zustand";
import {
  fetchConversations,
  createConversation,
  deleteConversation,
  renameConversation,
  type ConversationMeta,
} from "../api/client";
import { snowflake } from "../lib/snowflake";

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
  touchThread: (id: string) => void;
}

function sortByDate(a: ConversationMeta, b: ConversationMeta): number {
  return (
    new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime()
  );
}

export const useThreadListStore = create<ThreadListState>((set) => ({
  threads: [],
  activeThreadId: null,
  isLoading: false,

  loadThreads: async () => {
    set({ isLoading: true });
    try {
      const convos = await fetchConversations();
      convos.sort(sortByDate);
      set({ threads: convos, isLoading: false });
    } catch {
      set({ isLoading: false });
    }
  },

  createThread: async () => {
    const id = snowflake();
    const conv = await createConversation(id);
    set((s) => ({
      threads: [conv, ...s.threads],
      activeThreadId: conv.id,
    }));
    history.replaceState(null, "", `/c/${conv.id}`);
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
      history.replaceState(null, "", activeThreadId ? `/c/${activeThreadId}` : "/");
      return { threads, activeThreadId };
    });
  },

  renameThread: async (id, title) => {
    await renameConversation(id, title);
    set((s) => ({
      threads: s.threads.map((t) => (t.id === id ? { ...t, title } : t)),
    }));
  },

  switchThread: (id) => {
    set({ activeThreadId: id });
    history.replaceState(null, "", id ? `/c/${id}` : "/");
  },

  updateThreadTitle: (id, title) => {
    set((s) => {
      const now = new Date().toISOString();
      const threads = s.threads
        .map((t) => (t.id === id ? { ...t, title, updated_at: now } : t))
        .sort(sortByDate);
      return { threads };
    });
  },

  touchThread: (id) => {
    set((s) => {
      const now = new Date().toISOString();
      const threads = s.threads
        .map((t) => (t.id === id ? { ...t, updated_at: now } : t))
        .sort(sortByDate);
      return { threads };
    });
  },
}));
