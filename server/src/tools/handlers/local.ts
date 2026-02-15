import type { ToolHandler, ToolResult, ToolContext } from "../types.js";

export const setTitleTool: ToolHandler = {
  definition: {
    name: "_nexus_set_title",
    description:
      "Update the conversation title. Call this after your first response to set an initial title, and again whenever the topic shifts significantly. Keep titles brief (3-8 words).",
    input_schema: {
      type: "object",
      properties: {
        title: {
          type: "string",
          description: "The new conversation title (3-8 words)",
        },
      },
      required: ["title"],
    },
  },

  async execute(
    toolUseId: string,
    args: Record<string, unknown>,
    ctx: ToolContext,
  ): Promise<ToolResult> {
    const newTitle = ((args.title as string) || "").trim().slice(0, 100);

    if (newTitle) {
      ctx.conversation.title = newTitle;
      ctx.conversation.updatedAt = Date.now();
      ctx.saveConversation(ctx.conversation);
      ctx.sse.writeEvent("title_update", { title: newTitle });
    }

    return {
      tool_use_id: toolUseId,
      content: newTitle ? "Title updated" : "No title provided",
    };
  },
};
