import type { SystemMessageProvider, SystemMessageContext } from "../types.js";

export const corePromptProvider: SystemMessageProvider = {
  name: "core-prompt",
  timeoutMs: 100,

  async provide(ctx: SystemMessageContext): Promise<string | null> {
    return ctx.agent?.systemPrompt || ctx.settings.system_prompt || null;
  },
};
