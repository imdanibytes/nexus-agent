import { create } from "zustand";
import type { ConversationMeta, Message, ToolCallInfo, UiSurfaceInfo, AgentProfile } from "../api/client.js";

export interface UiSurfaceData {
  toolUseId: string;
  name: string;
  input: Record<string, unknown>;
  responded: boolean;
}

interface ChatState {
  conversations: ConversationMeta[];
  activeId: string | null;
  messages: Message[];
  isStreaming: boolean;
  streamingText: string;
  currentToolCalls: ToolCallInfo[];
  currentUiSurfaces: UiSurfaceInfo[];
  pendingUiSurface: UiSurfaceData | null;
  chatOpen: boolean;
  profiles: AgentProfile[];
  activeProfileId: string | null;
  settingsOpen: boolean;

  setConversations: (conversations: ConversationMeta[]) => void;
  setActiveId: (id: string | null) => void;
  setMessages: (messages: Message[]) => void;
  startStreaming: () => void;
  appendStreamingText: (text: string) => void;
  addToolCall: (tc: ToolCallInfo) => void;
  updateToolCallResult: (id: string, result: string, isError: boolean) => void;
  addUiSurface: (surface: UiSurfaceData) => void;
  resolveUiSurface: (toolUseId: string) => void;
  finishStreaming: (finalMessage?: Message) => void;
  addUserMessage: (message: Message) => void;
  updateTitle: (id: string, title: string) => void;
  setChatOpen: (open: boolean) => void;
  setProfiles: (profiles: AgentProfile[]) => void;
  setActiveProfileId: (id: string | null) => void;
  setSettingsOpen: (open: boolean) => void;
}

export const useChatStore = create<ChatState>((set) => ({
  conversations: [],
  activeId: null,
  messages: [],
  isStreaming: false,
  streamingText: "",
  currentToolCalls: [],
  currentUiSurfaces: [],
  pendingUiSurface: null,
  chatOpen: false,
  profiles: [],
  activeProfileId: null,
  settingsOpen: false,

  setConversations: (conversations) => set({ conversations }),
  setActiveId: (activeId) => set({ activeId }),
  setMessages: (messages) => set({ messages }),

  startStreaming: () =>
    set({
      isStreaming: true,
      streamingText: "",
      currentToolCalls: [],
      currentUiSurfaces: [],
      pendingUiSurface: null,
    }),

  appendStreamingText: (text) =>
    set((state) => ({ streamingText: state.streamingText + text })),

  addToolCall: (tc) =>
    set((state) => ({
      currentToolCalls: [...state.currentToolCalls, tc],
    })),

  updateToolCallResult: (id, result, isError) =>
    set((state) => ({
      currentToolCalls: state.currentToolCalls.map((tc) =>
        tc.id === id ? { ...tc, result, isError } : tc
      ),
    })),

  addUiSurface: (surface) =>
    set({
      pendingUiSurface: surface,
    }),

  resolveUiSurface: (toolUseId) =>
    set((state) => ({
      pendingUiSurface:
        state.pendingUiSurface?.toolUseId === toolUseId
          ? { ...state.pendingUiSurface, responded: true }
          : state.pendingUiSurface,
    })),

  finishStreaming: (finalMessage) =>
    set((state) => {
      const newMessages = finalMessage
        ? [...state.messages, finalMessage]
        : state.messages;
      return {
        isStreaming: false,
        streamingText: "",
        currentToolCalls: [],
        currentUiSurfaces: [],
        pendingUiSurface: null,
        messages: newMessages,
      };
    }),

  addUserMessage: (message) =>
    set((state) => ({
      messages: [...state.messages, message],
    })),

  updateTitle: (id, title) =>
    set((state) => ({
      conversations: state.conversations.map((c) =>
        c.id === id ? { ...c, title } : c
      ),
    })),

  setChatOpen: (chatOpen) => set({ chatOpen }),
  setProfiles: (profiles) => set({ profiles }),
  setActiveProfileId: (activeProfileId) => set({ activeProfileId }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
}));
