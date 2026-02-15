import { create } from "zustand";
import type { Agent, AvailableTool, ProviderPublic } from "../api/client.js";

export interface TimingSpanMarker {
  label: string;
  timeMs: number;
}

export interface TimingSpan {
  id: string;
  name: string;
  parentId: string | null;
  startMs: number;
  endMs: number;
  durationMs: number;
  metadata?: Record<string, unknown>;
  markers?: TimingSpanMarker[];
}

interface ChatState {
  agents: Agent[];
  activeAgentId: string | null;
  providers: ProviderPublic[];
  availableTools: AvailableTool[];
  settingsOpen: boolean;

  setAgents: (agents: Agent[]) => void;
  setActiveAgentId: (id: string | null) => void;
  setProviders: (providers: ProviderPublic[]) => void;
  setAvailableTools: (tools: AvailableTool[]) => void;
  setSettingsOpen: (open: boolean) => void;
  updateTitle: (id: string, title: string) => void;
}

export const useChatStore = create<ChatState>((set) => ({
  agents: [],
  activeAgentId: null,
  providers: [],
  availableTools: [],
  settingsOpen: false,

  setAgents: (agents) => set({ agents }),
  setActiveAgentId: (activeAgentId) => set({ activeAgentId }),
  setProviders: (providers) => set({ providers }),
  setAvailableTools: (availableTools) => set({ availableTools }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),

  // No-op — title updates are handled by the thread list runtime now
  updateTitle: () => {},
}));
