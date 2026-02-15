import Anthropic from "@anthropic-ai/sdk";
import { v4 as uuidv4 } from "uuid";
import { getSettings } from "./settings.js";
import { getMcpTools, callMcpTool } from "./tools.js";
import { getUiTools, isUiTool, isInteractiveUiTool } from "./ui-tools.js";
import {
  getConversation,
  saveConversation,
} from "./storage.js";
import { getProfile, getActiveProfileId } from "./profiles.js";
import type { Conversation, Message, SseWriter, ToolCallInfo, UiSurfaceInfo } from "./types.js";

// Pending UI surface responses — keyed by tool_use_id
const pendingResponses = new Map<
  string,
  { resolve: (value: { action: string; content: unknown }) => void }
>();

export function resolveUiResponse(
  toolUseId: string,
  action: string,
  content: unknown
): boolean {
  const pending = pendingResponses.get(toolUseId);
  if (!pending) return false;
  pending.resolve({ action, content });
  pendingResponses.delete(toolUseId);
  return true;
}

export async function runAgentTurn(
  conversationId: string,
  userMessage: string,
  sse: SseWriter,
  profileId?: string
): Promise<void> {
  const settings = await getSettings();

  // Resolve effective profile: explicit > active > none
  const effectiveProfileId = profileId || getActiveProfileId();
  const profile = effectiveProfileId ? getProfile(effectiveProfileId) : null;

  const effectiveModel = profile?.model || settings.llm_model;
  const effectivePrompt = profile?.systemPrompt || settings.system_prompt;

  console.log(
    `[agent] endpoint=${settings.llm_endpoint} model=${effectiveModel}` +
      (profile ? ` profile="${profile.name}"` : "")
  );

  // Load or create conversation
  let conv = getConversation(conversationId);
  if (!conv) {
    conv = {
      id: conversationId,
      title: "New conversation",
      createdAt: Date.now(),
      updatedAt: Date.now(),
      messages: [],
    };
  }

  // Add user message
  const userMsg: Message = {
    id: uuidv4(),
    role: "user",
    content: userMessage,
    timestamp: Date.now(),
  };
  conv.messages.push(userMsg);

  // Create Anthropic client
  const client = new Anthropic({
    apiKey: settings.llm_api_key || "ollama",
    baseURL: settings.llm_endpoint,
  });

  // Collect tools
  const mcpTools = await getMcpTools();
  const uiTools = getUiTools();
  const allTools = [...mcpTools, ...uiTools];

  // Build messages for the API
  const apiMessages = buildApiMessages(conv.messages);

  let round = 0;
  const maxRounds = settings.max_tool_rounds;
  let assistantText = "";
  const assistantToolCalls: ToolCallInfo[] = [];
  const assistantUiSurfaces: UiSurfaceInfo[] = [];

  while (round < maxRounds) {
    round++;
    sse.writeEvent("turn_start", {
      round,
      ...(profile ? { profileId: profile.id, profileName: profile.name } : {}),
    });

    try {
      console.log(`[agent] round=${round} calling LLM...`);
      const stream = client.messages.stream({
        model: effectiveModel,
        max_tokens: 8192,
        system: effectivePrompt,
        messages: apiMessages,
        tools: allTools.length > 0 ? allTools : undefined,
      });

      let stopReason: string | null = null;
      const toolUseBlocks: {
        id: string;
        name: string;
        partialJson: string;
      }[] = [];

      // Process stream events
      for await (const event of stream) {
        if (event.type === "content_block_start") {
          const block = event.content_block;
          if (block.type === "text") {
            sse.writeEvent("text_start", {});
          } else if (block.type === "tool_use") {
            toolUseBlocks.push({ id: block.id, name: block.name, partialJson: "" });
            sse.writeEvent("tool_start", { id: block.id, name: block.name });
          }
        } else if (event.type === "content_block_delta") {
          const delta = event.delta;
          if (delta.type === "text_delta") {
            assistantText += delta.text;
            sse.writeEvent("text_delta", { text: delta.text });
          } else if (delta.type === "input_json_delta") {
            const current = toolUseBlocks[toolUseBlocks.length - 1];
            if (current) {
              current.partialJson += delta.partial_json;
              sse.writeEvent("tool_input_delta", { partial_json: delta.partial_json });
            }
          }
        } else if (event.type === "message_delta") {
          stopReason = event.delta.stop_reason ?? null;
        }
      }

      // Process tool calls if stop_reason is tool_use
      if (stopReason === "tool_use" && toolUseBlocks.length > 0) {
        // Build assistant message with tool_use blocks for the API
        const assistantContentBlocks: Anthropic.ContentBlockParam[] = [];
        if (assistantText) {
          assistantContentBlocks.push({ type: "text", text: assistantText });
        }

        const toolResultBlocks: Anthropic.ToolResultBlockParam[] = [];

        for (const toolBlock of toolUseBlocks) {
          let parsedArgs: Record<string, unknown> = {};
          try {
            parsedArgs = JSON.parse(toolBlock.partialJson || "{}");
          } catch {
            parsedArgs = {};
          }

          assistantContentBlocks.push({
            type: "tool_use",
            id: toolBlock.id,
            name: toolBlock.name,
            input: parsedArgs,
          });

          if (isUiTool(toolBlock.name)) {
            // UI tool — emit to frontend
            sse.writeEvent("ui_surface", {
              tool_use_id: toolBlock.id,
              name: toolBlock.name,
              input: parsedArgs,
            });

            let resultContent: string;

            if (isInteractiveUiTool(toolBlock.name, parsedArgs)) {
              // Wait for frontend response
              const response = await new Promise<{ action: string; content: unknown }>(
                (resolve) => {
                  pendingResponses.set(toolBlock.id, { resolve });
                }
              );

              resultContent = JSON.stringify(response);
              assistantUiSurfaces.push({
                toolUseId: toolBlock.id,
                name: toolBlock.name,
                input: parsedArgs,
                response: response,
              });
            } else {
              // Non-interactive — auto-resolve
              resultContent = "Displayed to user";
              assistantUiSurfaces.push({
                toolUseId: toolBlock.id,
                name: toolBlock.name,
                input: parsedArgs,
              });
            }

            toolResultBlocks.push({
              type: "tool_result",
              tool_use_id: toolBlock.id,
              content: resultContent,
            });
          } else {
            // MCP tool — execute server-side
            sse.writeEvent("tool_executing", { id: toolBlock.id, name: toolBlock.name });
            const result = await callMcpTool(toolBlock.name, parsedArgs);

            sse.writeEvent("tool_result", {
              id: toolBlock.id,
              name: toolBlock.name,
              content: result.content,
              is_error: result.isError,
            });

            assistantToolCalls.push({
              id: toolBlock.id,
              name: toolBlock.name,
              args: parsedArgs,
              result: result.content,
              isError: result.isError,
            });

            toolResultBlocks.push({
              type: "tool_result",
              tool_use_id: toolBlock.id,
              content: result.content,
              is_error: result.isError,
            });
          }
        }

        // Append assistant + tool_result messages for next round
        apiMessages.push({ role: "assistant", content: assistantContentBlocks });
        apiMessages.push({ role: "user", content: toolResultBlocks });

        // Reset text for next round (tool calls may produce more text)
        assistantText = "";
        continue;
      }

      // End of turn (end_turn or max_tokens)
      break;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      sse.writeEvent("error", { message });
      break;
    }
  }

  // Build final assistant message
  const assistantMsg: Message = {
    id: uuidv4(),
    role: "assistant",
    content: assistantText,
    timestamp: Date.now(),
    toolCalls: assistantToolCalls.length > 0 ? assistantToolCalls : undefined,
    uiSurfaces: assistantUiSurfaces.length > 0 ? assistantUiSurfaces : undefined,
    ...(profile ? { profileId: profile.id, profileName: profile.name } : {}),
  };
  conv.messages.push(assistantMsg);
  conv.updatedAt = Date.now();

  // Auto-generate title after first exchange
  if (conv.messages.length === 2 && conv.title === "New conversation") {
    try {
      const titleConv = await client.messages.create({
        model: effectiveModel,
        max_tokens: 50,
        system: "Generate a brief title (3-6 words) for this conversation. Respond with just the title, no quotes or punctuation.",
        messages: [{ role: "user", content: userMessage }],
      });
      const titleBlock = titleConv.content[0];
      if (titleBlock.type === "text" && titleBlock.text.trim()) {
        conv.title = titleBlock.text.trim().slice(0, 100);
        sse.writeEvent("title_update", { title: conv.title });
      }
    } catch {
      // Title generation is best-effort
    }
  }

  saveConversation(conv);
  sse.writeEvent("turn_end", { stop_reason: "end_turn" });
  sse.close();
}

function buildApiMessages(
  messages: Message[]
): Anthropic.MessageParam[] {
  const result: Anthropic.MessageParam[] = [];
  for (const msg of messages) {
    if (msg.role === "user") {
      result.push({ role: "user", content: msg.content });
    } else {
      // For assistant messages, reconstruct content blocks
      const blocks: Anthropic.ContentBlockParam[] = [];
      if (msg.content) {
        blocks.push({ type: "text", text: msg.content });
      }
      if (msg.toolCalls) {
        for (const tc of msg.toolCalls) {
          blocks.push({
            type: "tool_use",
            id: tc.id,
            name: tc.name,
            input: tc.args,
          });
        }
      }
      if (blocks.length > 0) {
        result.push({ role: "assistant", content: blocks });
      }

      // Add tool results as user messages
      if (msg.toolCalls) {
        const toolResults: Anthropic.ToolResultBlockParam[] = msg.toolCalls.map((tc) => ({
          type: "tool_result" as const,
          tool_use_id: tc.id,
          content: tc.result || "",
          is_error: tc.isError,
        }));
        result.push({ role: "user", content: toolResults });
      }
    }
  }
  return result;
}
