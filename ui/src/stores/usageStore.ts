import { create } from "zustand";

export interface ConversationUsage {
  inputTokens: number;
  outputTokens: number;
  cacheReadInputTokens: number;
  cacheCreationInputTokens: number;
  contextWindow: number;
  totalCost: number;
}

interface UsageState {
  usage: Record<string, ConversationUsage>;
  setUsage: (convId: string, usage: ConversationUsage) => void;
}

export const useUsageStore = create<UsageState>((set) => ({
  usage: {},
  setUsage: (convId, usage) =>
    set((s) => ({ usage: { ...s.usage, [convId]: usage } })),
}));
