import Anthropic from "@anthropic-ai/sdk";
import { getSettings } from "./settings.js";
import { getConversation, saveConversation } from "./storage.js";
import { getAgent, getActiveAgentId } from "./agents.js";
import { getProvider } from "./providers.js";
import { getToolSettings } from "./tool-settings.js";
import { createLlmClient } from "./client-factory.js";
import { ToolExecutor } from "./tools/executor.js";
import type { Agent, Provider, ToolFilter } from "./types.js";
import { setTitleTool } from "./tools/handlers/local.js";
import { fetchMcpToolHandlers } from "./tools/handlers/remote.js";
import {
  FrontendToolBridge,
  createClipboardTools,
} from "./tools/handlers/frontend.js";
import { SystemMessageBuilder } from "./system-message/builder.js";
import { corePromptProvider } from "./system-message/providers/core-prompt.js";
import { datetimeProvider } from "./system-message/providers/datetime.js";
import { conversationContextProvider } from "./system-message/providers/conversation-context.js";
import { SpanCollector } from "./timing.js";
import type {
  Conversation,
  MessagePart,
  SseWriter,
} from "./types.js";
import type { ToolContext } from "./tools/types.js";

/** Wire format from the frontend — matches active branch messages */
export interface WireMessage {
  role: string;
  content: string;
  toolCalls?: {
    id: string;
    name: string;
    args: Record<string, unknown>;
    result?: string;
    isError?: boolean;
  }[];
}

// Active turns — prevents concurrent turns on the same conversation
const activeTurns = new Set<string>();

// Shared frontend bridge — lives for the process lifetime
const frontendBridge = new FrontendToolBridge();

export function resolveFrontendToolResult(
  toolUseId: string,
  content: string,
  isError: boolean,
): boolean {
  return frontendBridge.resolve(toolUseId, content, isError);
}

// System message builder — register providers once
const systemMessageBuilder = new SystemMessageBuilder();
systemMessageBuilder.register(corePromptProvider);
systemMessageBuilder.register(conversationContextProvider);
systemMessageBuilder.register(datetimeProvider);

/**
 * Convert frontend wire messages to Anthropic API format.
 * The frontend sends the active branch — we just translate the format.
 */
function buildApiMessages(
  wireMessages: WireMessage[],
  mapName: (name: string) => string = (n) => n,
): Anthropic.MessageParam[] {
  const result: Anthropic.MessageParam[] = [];

  for (const msg of wireMessages) {
    if (msg.role === "user") {
      result.push({ role: "user", content: msg.content });
    } else if (msg.role === "assistant") {
      const blocks: Anthropic.ContentBlockParam[] = [];

      if (msg.content) {
        blocks.push({ type: "text", text: msg.content });
      }

      if (msg.toolCalls) {
        for (const tc of msg.toolCalls) {
          blocks.push({
            type: "tool_use",
            id: tc.id,
            name: mapName(tc.name),
            input: tc.args,
          });
        }
      }

      if (blocks.length > 0) {
        result.push({ role: "assistant", content: blocks });
      }

      // Tool results go as a user message (Anthropic API requirement)
      if (msg.toolCalls) {
        const toolResults: Anthropic.ToolResultBlockParam[] = msg.toolCalls
          .filter((tc) => tc.result !== undefined)
          .map((tc) => ({
            type: "tool_result" as const,
            tool_use_id: tc.id,
            content: tc.result || "",
            is_error: tc.isError,
          }));
        if (toolResults.length > 0) {
          result.push({ role: "user", content: toolResults });
        }
      }
    }
  }

  return result;
}

/** Match a tool name against a glob pattern (supports * wildcard) */
function matchGlob(pattern: string, name: string): boolean {
  const regex = new RegExp(
    "^" + pattern.replace(/\*/g, ".*").replace(/\?/g, ".") + "$",
  );
  return regex.test(name);
}

/** Apply tool filters (global + agent-level) to a list of tool definitions */
function applyToolFilters(
  defs: import("./tools/types.js").ToolDefinition[],
  globalFilter?: ToolFilter,
  agentFilter?: ToolFilter,
): import("./tools/types.js").ToolDefinition[] {
  let result = defs;

  if (globalFilter) {
    if (globalFilter.mode === "allow") {
      result = result.filter((d) =>
        globalFilter.tools.some((t) => matchGlob(t, d.name)),
      );
    } else {
      result = result.filter(
        (d) => !globalFilter.tools.some((t) => matchGlob(t, d.name)),
      );
    }
  }

  if (agentFilter) {
    if (agentFilter.mode === "allow") {
      result = result.filter((d) =>
        agentFilter.tools.some((t) => matchGlob(t, d.name)),
      );
    } else {
      result = result.filter(
        (d) => !agentFilter.tools.some((t) => matchGlob(t, d.name)),
      );
    }
  }

  return result;
}

export async function runAgentTurn(
  conversationId: string,
  wireMessages: WireMessage[],
  sse: SseWriter,
  agentId?: string,
  externalAbort?: AbortSignal,
): Promise<void> {
  if (activeTurns.has(conversationId)) {
    throw new Error(
      `Conversation ${conversationId} already has an active turn in progress`,
    );
  }
  activeTurns.add(conversationId);

  try {
    await _runAgentTurnInner(conversationId, wireMessages, sse, agentId, externalAbort);
  } finally {
    activeTurns.delete(conversationId);
  }
}

async function _runAgentTurnInner(
  conversationId: string,
  wireMessages: WireMessage[],
  sse: SseWriter,
  agentId?: string,
  externalAbort?: AbortSignal,
): Promise<void> {
  const timing = new SpanCollector();
  const turnSpan = timing.span("turn");

  // --- Setup phase ---
  const setupSpan = turnSpan.span("setup");

  const settingsSpan = setupSpan.span("fetch_settings");
  const settings = await getSettings();
  const toolSettings = await getToolSettings();
  settingsSpan.end();

  // Resolve effective agent: explicit > active > none
  const effectiveAgentId = agentId || getActiveAgentId();
  const agent: Agent | null = effectiveAgentId
    ? getAgent(effectiveAgentId)
    : null;

  // Resolve provider from agent, or fall back to legacy settings
  let client: Anthropic;
  let effectiveModel: string;
  let effectiveMaxTokens = 8192;
  let effectiveTemperature: number | undefined;
  let effectiveTopP: number | undefined;


  if (agent) {
    const provider = await getProvider(agent.providerId);
    if (provider) {
      client = await createLlmClient(provider);

    } else {
      // Provider was deleted — fall back to legacy
      client = new Anthropic({
        apiKey: settings.llm_api_key || "ollama",
        baseURL: settings.llm_endpoint,
      });
    }
    effectiveModel = agent.model;
    if (agent.maxTokens) effectiveMaxTokens = agent.maxTokens;
    if (agent.temperature !== undefined) effectiveTemperature = agent.temperature;
    if (agent.topP !== undefined) effectiveTopP = agent.topP;
  } else {
    // Legacy fallback — no agent active
    client = new Anthropic({
      apiKey: settings.llm_api_key || "ollama",
      baseURL: settings.llm_endpoint,
    });
    effectiveModel = settings.llm_model;
  }

  console.log(
    `[agent] model=${effectiveModel}` +
      (agent ? ` agent="${agent.name}"` : "") +
      ` messages=${wireMessages.length}`,
  );

  // Build tool executor
  const toolSetupSpan = setupSpan.span("build_tool_executor");
  const executor = new ToolExecutor();
  executor.register(setTitleTool);
  executor.registerAll(createClipboardTools(frontendBridge));

  const mcpFetchSpan = toolSetupSpan.span("fetch_mcp_tools");
  executor.registerAll(await fetchMcpToolHandlers());
  mcpFetchSpan.end();
  toolSetupSpan.end();

  // Get or create conversation for context
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

  setupSpan.end();

  // Abort controller for frontend tool bridge timeout cleanup + external abort
  const abortController = new AbortController();
  if (externalAbort) {
    if (externalAbort.aborted) {
      abortController.abort();
    } else {
      externalAbort.addEventListener("abort", () => abortController.abort(), { once: true });
    }
  }

  // Build tool context
  const toolCtx: ToolContext = {
    conversationId,
    sse,
    conversation: conv,
    saveConversation,
    signal: abortController.signal,
  };

  // Map tool definitions to Anthropic format, applying filters
  const allToolDefs = executor.definitions();
  const toolDefs = applyToolFilters(
    allToolDefs,
    toolSettings.globalToolFilter,
    agent?.toolFilter,
  );

  // LLM APIs require tool names to match ^[a-zA-Z0-9_-]+$ — no dots.
  // MCP tools use dot-namespaced names, so sanitize with a bidirectional map.
  const toWireName = new Map<string, string>(); // original → sanitized
  const toOrigName = new Map<string, string>(); // sanitized → original

  for (const d of toolDefs) {
    if (d.name.includes(".")) {
      const sanitized = d.name.replace(/\./g, "__");
      toWireName.set(d.name, sanitized);
      toOrigName.set(sanitized, d.name);
    }
  }

  const wireName = (name: string) => toWireName.get(name) ?? name;
  const origName = (name: string) => toOrigName.get(name) ?? name;

  const anthropicTools: Anthropic.Tool[] = toolDefs.map((d) => ({
    name: wireName(d.name),
    description: d.description,
    input_schema: d.input_schema as Anthropic.Tool["input_schema"],
  }));

  // Build API messages from frontend-provided history (active branch)
  // Must come after wireName is defined so Bedrock tool names get sanitized in history.
  const apiMessages = buildApiMessages(wireMessages, wireName);

  let round = 0;
  const maxRounds = settings.max_tool_rounds;
  const assistantParts: MessagePart[] = [];

  while (round < maxRounds) {
    if (abortController.signal.aborted) break;

    round++;
    const roundStartIdx = assistantParts.length;
    const roundSpan = turnSpan.span(`round:${round}`, { round });

    // Build system message fresh each round
    const smSpan = roundSpan.span("system_message");
    const systemMessage = await systemMessageBuilder.build(
      {
        conversationId,
        conversation: conv,
        toolNames: toolDefs.map((d) => wireName(d.name)),
        settings,
        profile: agent
          ? { id: agent.id, name: agent.name, model: agent.model, systemPrompt: agent.systemPrompt, createdAt: agent.createdAt, updatedAt: agent.updatedAt }
          : null,
      },
      smSpan,
    );
    smSpan.end();

    sse.writeEvent("turn_start", {
      round,
      ...(agent
        ? { agentId: agent.id, agentName: agent.name }
        : {}),
    });

    try {
      console.log(`[agent] round=${round} calling LLM...`);
      const llmSpan = roundSpan.span("llm_call", {
        model: effectiveModel,
        round,
      });

      const stream = client.messages.stream({
        model: effectiveModel,
        max_tokens: effectiveMaxTokens,
        system: systemMessage,
        messages: apiMessages,
        tools: anthropicTools.length > 0 ? anthropicTools : undefined,
        ...(effectiveTemperature !== undefined ? { temperature: effectiveTemperature } : {}),
        ...(effectiveTopP !== undefined ? { top_p: effectiveTopP } : {}),
      });

      let stopReason: string | null = null;
      let firstTokenMarked = false;
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
            assistantParts.push({ type: "text", text: "" });
            sse.writeEvent("text_start", {});
          } else if (block.type === "tool_use") {
            const realName = origName(block.name);
            toolUseBlocks.push({
              id: block.id,
              name: realName,
              partialJson: "",
            });
            sse.writeEvent("tool_start", {
              id: block.id,
              name: realName,
            });
          }
        } else if (event.type === "content_block_delta") {
          const delta = event.delta;
          if (delta.type === "text_delta") {
            if (!firstTokenMarked) {
              llmSpan.mark("first_token");
              firstTokenMarked = true;
            }
            // Append to the most recent text part
            for (let i = assistantParts.length - 1; i >= 0; i--) {
              if (assistantParts[i].type === "text") {
                (assistantParts[i] as { type: "text"; text: string }).text += delta.text;
                break;
              }
            }
            sse.writeEvent("text_delta", { text: delta.text });
          } else if (delta.type === "input_json_delta") {
            const current = toolUseBlocks[toolUseBlocks.length - 1];
            if (current) {
              current.partialJson += delta.partial_json;
              sse.writeEvent("tool_input_delta", {
                partial_json: delta.partial_json,
              });
            }
          }
        } else if (event.type === "message_delta") {
          stopReason = event.delta.stop_reason ?? null;
        }
      }

      llmSpan.end();

      // Process tool calls if stop_reason is tool_use
      if (stopReason === "tool_use" && toolUseBlocks.length > 0) {
        // Build assistant content blocks for Anthropic API from parts added this round
        const assistantContentBlocks: Anthropic.ContentBlockParam[] = [];
        for (let i = roundStartIdx; i < assistantParts.length; i++) {
          const part = assistantParts[i];
          if (part.type === "text" && part.text) {
            assistantContentBlocks.push({ type: "text", text: part.text });
          }
        }

        // Parse args and build tool_use content blocks
        const parsed = toolUseBlocks.map((block) => {
          let args: Record<string, unknown> = {};
          try {
            args = JSON.parse(block.partialJson || "{}");
          } catch {
            args = {};
          }

          assistantContentBlocks.push({
            type: "tool_use",
            id: block.id,
            name: wireName(block.name),
            input: args,
          });

          return { ...block, args };
        });

        // Execute all tools in parallel
        const toolExecSpan = roundSpan.span("tool_execution", {
          count: parsed.length,
        });

        const results = await Promise.all(
          parsed.map((block) => {
            const toolSpan = toolExecSpan.span(`tool:${block.name}`, {
              toolName: block.name,
              toolUseId: block.id,
            });
            return executor
              .execute(block.name, block.id, block.args, toolCtx)
              .finally(() => toolSpan.end());
          }),
        );

        toolExecSpan.end();

        // Build tool result blocks for next round and track tool-call parts
        const toolResultBlocks: Anthropic.ToolResultBlockParam[] = [];
        for (let i = 0; i < results.length; i++) {
          const result = results[i];
          const block = parsed[i];

          // Track non-local tool calls as ordered parts
          if (!block.name.startsWith("_nexus_")) {
            assistantParts.push({
              type: "tool-call",
              id: block.id,
              name: block.name,
              args: block.args,
              result: result.content,
              isError: result.is_error,
            });
          }

          toolResultBlocks.push({
            type: "tool_result",
            tool_use_id: result.tool_use_id,
            content: result.content,
            is_error: result.is_error,
          });
        }

        // Append for next round
        apiMessages.push({
          role: "assistant",
          content: assistantContentBlocks,
        });
        apiMessages.push({ role: "user", content: toolResultBlocks });

        roundSpan.end();
        continue;
      }

      // End of turn — no more tool calls
      roundSpan.end();
      break;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      sse.writeEvent("error", { message });
      roundSpan.end();
      break;
    }
  }

  // Clean up abort controller
  abortController.abort();

  // Finalize timing
  turnSpan.end();
  const timingSpans = timing.toJSON();

  // Update conversation metadata only — message persistence is handled by
  // the frontend's repository (tree-structured append via historyAdapter).
  const freshConv = getConversation(conversationId) || conv;
  freshConv.updatedAt = Date.now();
  saveConversation(freshConv);

  // Emit timing data to frontend
  sse.writeEvent("timing", { spans: timingSpans });

  const stopReason = abortController.signal.aborted ? "abort" : "end_turn";
  sse.writeEvent("turn_end", { stop_reason: stopReason, conversationId });
  sse.close();
}
