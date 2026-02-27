import { create } from "zustand";
import {
  fetchConversations,
  createConversation,
  deleteConversation,
  renameConversation,
  updateConversationWorkspace,
  updateConversationAgent,
  type ConversationMeta,
} from "../api/client";
import { snowflake } from "../lib/snowflake";

interface ThreadListState {
  threads: ConversationMeta[];
  activeThreadId: string | null;
  isLoading: boolean;

  loadThreads: () => Promise<void>;
  /** Create a conversation on the server. Returns the new ID. Caller navigates. */
  createThread: () => Promise<string>;
  /** Delete a conversation. Returns the next thread ID (or null). Caller navigates. */
  deleteThread: (id: string) => Promise<string | null>;
  renameThread: (id: string, title: string) => Promise<void>;
  /** Set the active thread from route params. No URL side-effects. */
  setActiveThread: (id: string | null) => void;
  /** @deprecated alias — use setActiveThread */
  switchThread: (id: string) => void;
  updateThreadTitle: (id: string, title: string) => void;
  setThreadWorkspace: (id: string, workspaceId: string | null) => Promise<void>;
  /** Update workspace_id locally (e.g., from SSE workspace_changed event). */
  updateThreadWorkspace: (id: string, workspaceId: string | null) => void;
  setThreadAgent: (id: string, agentId: string | null) => Promise<void>;
  /** Update agent_id locally (e.g., from SSE agent_changed event). */
  updateThreadAgent: (id: string, agentId: string | null) => void;
  /** Remove a thread from the local list (e.g., after SSE thread_deleted event). */
  removeThread: (id: string) => void;
  touchThread: (id: string) => void;
}

function sortByDate(a: ConversationMeta, b: ConversationMeta): number {
  return (
    new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime()
  );
}

export const useThreadListStore = create<ThreadListState>((set, get) => ({
  threads: [],
  activeThreadId: null,
  isLoading: false,

  loadThreads: async () => {
    set({ isLoading: true });
    try {
      const convos = await fetchConversations();
      convos.sort(sortByDate);
      set({ threads: convos, isLoading: false });
    } catch (err) {
      console.error("Failed to load conversations:", err);
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
    return conv.id;
  },

  deleteThread: async (id) => {
    await deleteConversation(id);
    let nextId: string | null = null;
    set((s) => {
      const threads = s.threads.filter((t) => t.id !== id);
      const activeThreadId =
        s.activeThreadId === id
          ? threads[0]?.id ?? null
          : s.activeThreadId;
      nextId = activeThreadId;
      return { threads, activeThreadId };
    });
    return nextId;
  },

  renameThread: async (id, title) => {
    await renameConversation(id, title);
    set((s) => ({
      threads: s.threads.map((t) => (t.id === id ? { ...t, title } : t)),
    }));
  },

  setActiveThread: (id) => {
    set({ activeThreadId: id });
  },

  switchThread: (id) => {
    get().setActiveThread(id);
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

  setThreadWorkspace: async (id, workspaceId) => {
    await updateConversationWorkspace(id, workspaceId);
    set((s) => ({
      threads: s.threads.map((t) =>
        t.id === id ? { ...t, workspace_id: workspaceId } : t,
      ),
    }));
  },

  updateThreadWorkspace: (id, workspaceId) => {
    set((s) => ({
      threads: s.threads.map((t) =>
        t.id === id ? { ...t, workspace_id: workspaceId } : t,
      ),
    }));
  },

  setThreadAgent: async (id, agentId) => {
    await updateConversationAgent(id, agentId);
    set((s) => ({
      threads: s.threads.map((t) =>
        t.id === id ? { ...t, agent_id: agentId } : t,
      ),
    }));
  },

  updateThreadAgent: (id, agentId) => {
    set((s) => ({
      threads: s.threads.map((t) =>
        t.id === id ? { ...t, agent_id: agentId } : t,
      ),
    }));
  },

  removeThread: (id) => {
    set((s) => ({
      threads: s.threads.filter((t) => t.id !== id),
      activeThreadId: s.activeThreadId === id ? null : s.activeThreadId,
    }));
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
