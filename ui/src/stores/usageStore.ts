import { create } from "zustand";
import type { ConversationUsage } from "@/api/client.js";

interface UsageState {
  usage: Record<string, ConversationUsage>;
  setUsage: (convId: string, usage: ConversationUsage) => void;
}

export const useUsageStore = create<UsageState>((set) => ({
  usage: {},
  setUsage: (convId, usage) =>
    set((s) => ({ usage: { ...s.usage, [convId]: usage } })),
}));
