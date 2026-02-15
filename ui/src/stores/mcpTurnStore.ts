/**
 * MCP turn store — reactive state for MCP-initiated turns.
 * React effects watch this to trigger thread switching + message appending.
 */

import { create } from "zustand";

interface McpTurnState {
  /** Conversation ID of the pending/active MCP turn */
  pendingConvId: string | null;
  /** The user message that started this turn */
  pendingUserMessage: string | null;

  setPendingTurn: (convId: string, userMessage: string) => void;
  clearPendingTurn: () => void;
}

export const useMcpTurnStore = create<McpTurnState>((set) => ({
  pendingConvId: null,
  pendingUserMessage: null,

  setPendingTurn: (convId, userMessage) =>
    set({ pendingConvId: convId, pendingUserMessage: userMessage }),

  clearPendingTurn: () =>
    set({ pendingConvId: null, pendingUserMessage: null }),
}));
