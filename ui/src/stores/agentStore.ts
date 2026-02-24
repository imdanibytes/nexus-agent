import { create } from "zustand";
import {
  fetchAgents,
  fetchActiveAgent,
  createAgent,
  updateAgent,
  deleteAgent,
  setActiveAgent,
  type AgentConfig,
  type CreateAgentRequest,
} from "../api/client";

interface AgentState {
  agents: AgentConfig[];
  activeAgentId: string | null;
  isLoading: boolean;

  loadAgents: () => Promise<void>;
  createAgent: (data: CreateAgentRequest) => Promise<AgentConfig>;
  updateAgent: (
    id: string,
    data: Partial<CreateAgentRequest> & {
      set_temperature?: boolean;
      set_max_tokens?: boolean;
      set_mcp_server_ids?: boolean;
    },
  ) => Promise<AgentConfig>;
  deleteAgent: (id: string) => Promise<void>;
  setActiveAgent: (id: string | null) => Promise<void>;
  getActiveAgent: () => AgentConfig | undefined;
}

export const useAgentStore = create<AgentState>((set, get) => ({
  agents: [],
  activeAgentId: null,
  isLoading: false,

  loadAgents: async () => {
    set({ isLoading: true });
    try {
      const [agents, activeId] = await Promise.all([
        fetchAgents(),
        fetchActiveAgent(),
      ]);
      set({ agents, activeAgentId: activeId, isLoading: false });
    } catch (err) {
      console.error("Failed to load agents:", err);
      set({ isLoading: false });
    }
  },

  createAgent: async (data) => {
    const agent = await createAgent(data);
    set((s) => ({ agents: [...s.agents, agent] }));
    return agent;
  },

  updateAgent: async (id, data) => {
    const agent = await updateAgent(id, data);
    set((s) => ({
      agents: s.agents.map((a) => (a.id === id ? agent : a)),
    }));
    return agent;
  },

  deleteAgent: async (id) => {
    await deleteAgent(id);
    set((s) => ({
      agents: s.agents.filter((a) => a.id !== id),
      activeAgentId: s.activeAgentId === id ? null : s.activeAgentId,
    }));
  },

  setActiveAgent: async (id) => {
    set({ activeAgentId: id });
    await setActiveAgent(id);
  },

  getActiveAgent: () => {
    const { agents, activeAgentId } = get();
    return agents.find((a) => a.id === activeAgentId);
  },
}));
