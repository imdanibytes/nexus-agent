import { create } from "zustand";
import {
  fetchWorkspaces,
  createWorkspace,
  updateWorkspace,
  deleteWorkspace,
  fetchActiveWorkspace,
  setActiveWorkspace,
  type WorkspaceConfig,
  type CreateWorkspaceRequest,
} from "../api/client";

interface WorkspaceState {
  workspaces: WorkspaceConfig[];
  activeWorkspace: WorkspaceConfig | null;
  isLoading: boolean;

  loadWorkspaces: () => Promise<void>;
  createWorkspace: (data: CreateWorkspaceRequest) => Promise<WorkspaceConfig>;
  updateWorkspace: (
    id: string,
    data: Partial<CreateWorkspaceRequest>,
  ) => Promise<WorkspaceConfig>;
  deleteWorkspace: (id: string) => Promise<void>;
  setActive: (id: string | null) => Promise<void>;
}

export const useWorkspaceStore = create<WorkspaceState>((set) => ({
  workspaces: [],
  activeWorkspace: null,
  isLoading: false,

  loadWorkspaces: async () => {
    set({ isLoading: true });
    try {
      const [workspaces, activeWorkspace] = await Promise.all([
        fetchWorkspaces(),
        fetchActiveWorkspace(),
      ]);
      set({ workspaces, activeWorkspace, isLoading: false });
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
      activeWorkspace:
        s.activeWorkspace?.id === id ? workspace : s.activeWorkspace,
    }));
    return workspace;
  },

  deleteWorkspace: async (id) => {
    await deleteWorkspace(id);
    set((s) => ({
      workspaces: s.workspaces.filter((w) => w.id !== id),
      activeWorkspace: s.activeWorkspace?.id === id ? null : s.activeWorkspace,
    }));
  },

  setActive: async (id) => {
    await setActiveWorkspace(id);
    set((s) => ({
      activeWorkspace: id
        ? s.workspaces.find((w) => w.id === id) ?? null
        : null,
    }));
  },
}));
