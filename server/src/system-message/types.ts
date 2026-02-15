import type { Conversation, AgentSettings, AgentProfile } from "../types.js";

export interface SystemMessageContext {
  conversationId: string;
  conversation: Conversation;
  tokenUsage?: { input: number; output: number; limit: number };
  toolNames: string[];
  settings: AgentSettings;
  profile?: AgentProfile | null;
}

export interface SystemMessageProvider {
  name: string;
  timeoutMs: number;
  provide(ctx: SystemMessageContext): Promise<string | null>;
}
