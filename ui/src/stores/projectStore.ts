import { create } from "zustand";
import {
  fetchProjects,
  createProject,
  updateProject,
  deleteProject,
  type ProjectConfig,
  type CreateProjectRequest,
} from "../api/client";

interface ProjectState {
  projects: ProjectConfig[];
  isLoading: boolean;

  loadProjects: () => Promise<void>;
  createProject: (data: CreateProjectRequest) => Promise<ProjectConfig>;
  updateProject: (
    id: string,
    data: Partial<CreateProjectRequest>,
  ) => Promise<ProjectConfig>;
  deleteProject: (id: string) => Promise<void>;
}

export const useProjectStore = create<ProjectState>((set) => ({
  projects: [],
  isLoading: false,

  loadProjects: async () => {
    set({ isLoading: true });
    try {
      const projects = await fetchProjects();
      set({ projects, isLoading: false });
    } catch (err) {
      console.error("Failed to load projects:", err);
      set({ isLoading: false });
    }
  },

  createProject: async (data) => {
    const project = await createProject(data);
    set((s) => ({ projects: [...s.projects, project] }));
    return project;
  },

  updateProject: async (id, data) => {
    const project = await updateProject(id, data);
    set((s) => ({
      projects: s.projects.map((p) => (p.id === id ? project : p)),
    }));
    return project;
  },

  deleteProject: async (id) => {
    await deleteProject(id);
    set((s) => ({
      projects: s.projects.filter((p) => p.id !== id),
    }));
  },
}));
