import { create } from "zustand";
import {
  fetchMcpServers,
  createMcpServer,
  updateMcpServer,
  deleteMcpServer,
  type McpServerConfig,
  type CreateMcpServerRequest,
} from "../api/client";

interface McpState {
  servers: McpServerConfig[];
  isLoading: boolean;

  loadServers: () => Promise<void>;
  createServer: (data: CreateMcpServerRequest) => Promise<McpServerConfig>;
  updateServer: (
    id: string,
    data: Partial<CreateMcpServerRequest>,
  ) => Promise<McpServerConfig>;
  deleteServer: (id: string) => Promise<void>;
}

export const useMcpStore = create<McpState>((set) => ({
  servers: [],
  isLoading: false,

  loadServers: async () => {
    set({ isLoading: true });
    try {
      const servers = await fetchMcpServers();
      set({ servers, isLoading: false });
    } catch {
      set({ isLoading: false });
    }
  },

  createServer: async (data) => {
    const server = await createMcpServer(data);
    set((s) => ({ servers: [...s.servers, server] }));
    return server;
  },

  updateServer: async (id, data) => {
    const server = await updateMcpServer(id, data);
    set((s) => ({
      servers: s.servers.map((srv) => (srv.id === id ? server : srv)),
    }));
    return server;
  },

  deleteServer: async (id) => {
    await deleteMcpServer(id);
    set((s) => ({
      servers: s.servers.filter((srv) => srv.id !== id),
    }));
  },
}));
