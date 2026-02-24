import { create } from "zustand";
import type { TaskState } from "../types/tasks";

interface TaskStoreState {
  /** Task state per conversation */
  states: Record<string, TaskState>;
  /** Whether the task panel is expanded */
  panelOpen: boolean;

  setTaskState: (conversationId: string, state: TaskState) => void;
  clearTaskState: (conversationId: string) => void;
  setPanelOpen: (open: boolean) => void;
}

export const useTaskStore = create<TaskStoreState>((set) => ({
  states: {},
  panelOpen: true,

  setTaskState: (conversationId, state) =>
    set((s) => ({
      states: { ...s.states, [conversationId]: state },
    })),

  clearTaskState: (conversationId) =>
    set((s) => {
      const { [conversationId]: _, ...rest } = s.states;
      return { states: rest };
    }),

  setPanelOpen: (panelOpen) => set({ panelOpen }),
}));
