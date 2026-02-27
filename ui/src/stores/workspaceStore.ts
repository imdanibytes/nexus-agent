import { create } from "zustand";
import {
  fetchWorkspaces,
  createWorkspace,
  updateWorkspace,
  deleteWorkspace,
  type WorkspaceConfig,
  type CreateWorkspaceRequest,
} from "../api/client";

interface WorkspaceState {
  workspaces: WorkspaceConfig[];
  isLoading: boolean;

  loadWorkspaces: () => Promise<void>;
  createWorkspace: (data: CreateWorkspaceRequest) => Promise<WorkspaceConfig>;
  updateWorkspace: (
    id: string,
    data: Partial<CreateWorkspaceRequest>,
  ) => Promise<WorkspaceConfig>;
  deleteWorkspace: (id: string) => Promise<void>;
}

export const useWorkspaceStore = create<WorkspaceState>((set) => ({
  workspaces: [],
  isLoading: false,

  loadWorkspaces: async () => {
    set({ isLoading: true });
    try {
      const workspaces = await fetchWorkspaces();
      set({ workspaces, isLoading: false });
    } catch (err) {
      console.error("Failed to load workspaces:", err);
      set({ isLoading: false });
    }
  },

  createWorkspace: async (data) => {
    const workspace = await createWorkspace(data);
    set((s) => ({ workspaces: [...s.workspaces, workspace] }));
    return workspace;
  },

  updateWorkspace: async (id, data) => {
    const workspace = await updateWorkspace(id, data);
    set((s) => ({
      workspaces: s.workspaces.map((w) => (w.id === id ? workspace : w)),
    }));
    return workspace;
  },

  deleteWorkspace: async (id) => {
    await deleteWorkspace(id);
    set((s) => ({
      workspaces: s.workspaces.filter((w) => w.id !== id),
    }));
  },
}));
