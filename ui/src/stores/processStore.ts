import { create } from "zustand";

export interface BgProcess {
  id: string;
  conversationId: string;
  label: string;
  command: string;
  kind: "bash" | "sub_agent";
  status: "running" | "completed" | "failed" | "cancelled";
  startedAt: string;
  completedAt?: string;
  exitCode?: number;
  isError: boolean;
  outputPreview?: string;
  outputSize?: number;
}

interface ProcessState {
  /** All known processes keyed by conversation ID → array. */
  processes: Record<string, BgProcess[]>;

  addProcess: (process: BgProcess) => void;
  updateProcess: (processId: string, patch: Partial<BgProcess>) => void;
  removeProcess: (processId: string) => void;
  removeConversation: (conversationId: string) => void;
  setProcesses: (conversationId: string, processes: BgProcess[]) => void;
}

export const useProcessStore = create<ProcessState>((set) => ({
  processes: {},

  addProcess: (process) =>
    set((s) => {
      const existing = s.processes[process.conversationId] ?? [];
      return {
        processes: {
          ...s.processes,
          [process.conversationId]: [...existing, process],
        },
      };
    }),

  updateProcess: (processId, patch) =>
    set((s) => {
      const updated: Record<string, BgProcess[]> = {};
      for (const [convId, procs] of Object.entries(s.processes)) {
        updated[convId] = procs.map((p) =>
          p.id === processId ? { ...p, ...patch } : p,
        );
      }
      return { processes: updated };
    }),

  removeProcess: (processId) =>
    set((s) => {
      const updated: Record<string, BgProcess[]> = {};
      for (const [convId, procs] of Object.entries(s.processes)) {
        const filtered = procs.filter((p) => p.id !== processId);
        if (filtered.length > 0) {
          updated[convId] = filtered;
        }
      }
      return { processes: updated };
    }),

  removeConversation: (conversationId) =>
    set((s) => {
      const { [conversationId]: _, ...rest } = s.processes;
      return { processes: rest };
    }),

  setProcesses: (conversationId, processes) =>
    set((s) => ({
      processes: { ...s.processes, [conversationId]: processes },
    })),
}));
