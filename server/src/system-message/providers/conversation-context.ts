import type { SystemMessageProvider, SystemMessageContext } from "../types.js";

export const conversationContextProvider: SystemMessageProvider = {
  name: "conversation-context",
  timeoutMs: 100,

  async provide(ctx: SystemMessageContext): Promise<string | null> {
    const parts: string[] = [];

    if (ctx.conversation.title && ctx.conversation.title !== "New conversation") {
      parts.push(`Conversation: "${ctx.conversation.title}"`);
    }

    const msgCount = ctx.conversation.messages.length;
    if (msgCount > 0) {
      parts.push(`Messages in history: ${msgCount}`);
    }

    if (ctx.tokenUsage) {
      const pct = Math.round((ctx.tokenUsage.input / ctx.tokenUsage.limit) * 100);
      parts.push(`Token usage: ${ctx.tokenUsage.input.toLocaleString()}/${ctx.tokenUsage.limit.toLocaleString()} (${pct}%)`);
    }

    return parts.length > 0 ? parts.join("\n") : null;
  },
};
