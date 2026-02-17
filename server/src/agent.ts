import Anthropic from "@anthropic-ai/sdk";
import { v4 as uuidv4 } from "uuid";
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
import { SystemMessageBuilder } from "./system-message/builder.js";
import { corePromptProvider } from "./system-message/providers/core-prompt.js";
import { datetimeProvider } from "./system-message/providers/datetime.js";
import { conversationContextProvider } from "./system-message/providers/conversation-context.js";
import { messageBoundaryProvider } from "./system-message/providers/message-boundary.js";
import { SpanCollector } from "./timing.js";
import type {
  Conversation,
  MessagePart,
  SseWriter,
} from "./types.js";
import type { ToolContext, ToolDefinition } from "./tools/types.js";
import {
  EventType,
  type PendingToolCall,
  type ResolvedToolResult,
} from "./ag-ui-types.js";

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

/** Result returned from a turn — indicates whether frontend tools are pending */
export interface TurnResult {
  pendingToolCalls?: PendingToolCall[];
  resolvedToolResults?: ResolvedToolResult[];
}

// Active turns — prevents concurrent turns on the same conversation
const activeTurns = new Set<string>();

// System message builder — register providers once
const systemMessageBuilder = new SystemMessageBuilder();
systemMessageBuilder.register(messageBoundaryProvider);
systemMessageBuilder.register(corePromptProvider);
systemMessageBuilder.register(conversationContextProvider);
systemMessageBuilder.register(datetimeProvider);

// Frontend tool definitions are passed in by the client at turn start.
// The server only needs the schemas so the LLM knows they exist.

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
      result.push({
        role: "user",
        content: `<user_message>\n${msg.content}\n</user_message>`,
      });
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
  defs: ToolDefinition[],
  globalFilter?: ToolFilter,
  agentFilter?: ToolFilter,
): ToolDefinition[] {
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
  frontendTools?: ToolDefinition[],
): Promise<TurnResult> {
  if (activeTurns.has(conversationId)) {
    throw new Error(
      `Conversation ${conversationId} already has an active turn in progress`,
    );
  }
  activeTurns.add(conversationId);

  try {
    return await _runAgentTurnInner(conversationId, wireMessages, sse, agentId, externalAbort, frontendTools);
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
  frontendTools?: ToolDefinition[],
): Promise<TurnResult> {
  const timing = new SpanCollector();
  const turnSpan = timing.span("turn");
  const runId = uuidv4();
  const messageId = uuidv4();

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

  // Build tool executor (server-side tools only)
  const toolSetupSpan = setupSpan.span("build_tool_executor");
  const executor = new ToolExecutor();
  executor.register(setTitleTool);

  const mcpFetchSpan = toolSetupSpan.span("fetch_mcp_tools");
  executor.registerAll(await fetchMcpToolHandlers());
  mcpFetchSpan.end();
  toolSetupSpan.end();

  // Combined tool definitions: server-executed + frontend-executed (from client)
  const allToolDefs: ToolDefinition[] = [
    ...executor.definitions(),
    ...(frontendTools ?? []),
  ];

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

  // Abort controller
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
  const toolDefs = applyToolFilters(
    allToolDefs,
    toolSettings.globalToolFilter,
    agent?.toolFilter,
  );

  // LLM APIs require tool names to match ^[a-zA-Z0-9_-]+$ — no dots.
  const toWireName = new Map<string, string>();
  const toOrigName = new Map<string, string>();

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

  const apiMessages = buildApiMessages(wireMessages, wireName);

  let round = 0;
  const maxRounds = settings.max_tool_rounds;
  const assistantParts: MessagePart[] = [];
  let turnResult: TurnResult = {};

  // ── Emit RUN_STARTED ──
  sse.writeEvent(EventType.RUN_STARTED, {
    threadId: conversationId,
    runId,
    ...(agent ? { agentId: agent.id, agentName: agent.name } : {}),
  });

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

    // Emit step for multi-round tracking
    sse.writeEvent(EventType.STEP_STARTED, { stepName: `round:${round}` });

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
        ...(effectiveTemperature !== undefined
          ? { temperature: effectiveTemperature }
          : effectiveTopP !== undefined
            ? { top_p: effectiveTopP }
            : {}),
      });

      let stopReason: string | null = null;
      let firstTokenMarked = false;
      let textStarted = false;
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
            if (!textStarted) {
              sse.writeEvent(EventType.TEXT_MESSAGE_START, {
                messageId,
                role: "assistant",
              });
              textStarted = true;
            }
          } else if (block.type === "tool_use") {
            const realName = origName(block.name);
            toolUseBlocks.push({
              id: block.id,
              name: realName,
              partialJson: "",
            });
            sse.writeEvent(EventType.TOOL_CALL_START, {
              toolCallId: block.id,
              toolCallName: realName,
              parentMessageId: messageId,
            });
          }
        } else if (event.type === "content_block_delta") {
          const delta = event.delta;
          if (delta.type === "text_delta") {
            if (!firstTokenMarked) {
              llmSpan.mark("first_token");
              firstTokenMarked = true;
            }
            for (let i = assistantParts.length - 1; i >= 0; i--) {
              if (assistantParts[i].type === "text") {
                (assistantParts[i] as { type: "text"; text: string }).text += delta.text;
                break;
              }
            }
            sse.writeEvent(EventType.TEXT_MESSAGE_CONTENT, {
              messageId,
              delta: delta.text,
            });
          } else if (delta.type === "input_json_delta") {
            const current = toolUseBlocks[toolUseBlocks.length - 1];
            if (current) {
              current.partialJson += delta.partial_json;
              sse.writeEvent(EventType.TOOL_CALL_ARGS, {
                toolCallId: current.id,
                delta: delta.partial_json,
              });
            }
          }
        } else if (event.type === "message_delta") {
          stopReason = event.delta.stop_reason ?? null;
        }
      }

      // End text message if we started one
      if (textStarted) {
        sse.writeEvent(EventType.TEXT_MESSAGE_END, { messageId });
      }

      // Emit TOOL_CALL_END for each tool
      for (const block of toolUseBlocks) {
        sse.writeEvent(EventType.TOOL_CALL_END, { toolCallId: block.id });
      }

      llmSpan.end();

      // Process tool calls if stop_reason is tool_use
      if (stopReason === "tool_use" && toolUseBlocks.length > 0) {
        const assistantContentBlocks: Anthropic.ContentBlockParam[] = [];
        for (let i = roundStartIdx; i < assistantParts.length; i++) {
          const part = assistantParts[i];
          if (part.type === "text" && part.text) {
            assistantContentBlocks.push({ type: "text", text: part.text });
          }
        }

        // Parse args
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

        // Partition into server-side and frontend tools
        // A tool is "server-side" if the executor has a handler for it.
        // Everything else (from the client's frontendTools list) is frontend.
        const serverTools = parsed.filter((b) => executor.has(b.name));
        const frontendTools = parsed.filter((b) => !executor.has(b.name));

        // Execute server-side tools
        const toolExecSpan = roundSpan.span("tool_execution", {
          count: serverTools.length,
        });

        const serverResults = await Promise.all(
          serverTools.map((block) => {
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

        // Emit TOOL_CALL_RESULT for server-side tools
        const resolvedToolResults: ResolvedToolResult[] = [];
        for (let i = 0; i < serverResults.length; i++) {
          const result = serverResults[i];
          const block = serverTools[i];

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

          sse.writeEvent(EventType.TOOL_CALL_RESULT, {
            toolCallId: result.tool_use_id,
            content: result.content,
            isError: result.is_error,
          });

          resolvedToolResults.push({
            toolCallId: result.tool_use_id,
            content: result.content,
            isError: result.is_error ?? false,
          });
        }

        // If any frontend tools need execution, end the run and let the client handle them
        if (frontendTools.length > 0) {
          const pendingToolCalls: PendingToolCall[] = frontendTools.map((b) => ({
            toolCallId: b.id,
            toolCallName: b.name,
            args: b.args,
          }));

          turnResult = { pendingToolCalls, resolvedToolResults };

          sse.writeEvent(EventType.STEP_FINISHED, { stepName: `round:${round}` });
          roundSpan.end();
          break; // Exit loop — run will finish below with pendingToolCalls
        }

        // All tools were server-side — build result blocks and continue
        const toolResultBlocks: Anthropic.ToolResultBlockParam[] = [];
        for (let i = 0; i < serverResults.length; i++) {
          toolResultBlocks.push({
            type: "tool_result",
            tool_use_id: serverResults[i].tool_use_id,
            content: serverResults[i].content,
            is_error: serverResults[i].is_error,
          });
        }

        apiMessages.push({
          role: "assistant",
          content: assistantContentBlocks,
        });
        apiMessages.push({ role: "user", content: toolResultBlocks });

        sse.writeEvent(EventType.STEP_FINISHED, { stepName: `round:${round}` });
        roundSpan.end();
        continue;
      }

      // End of turn — no more tool calls
      sse.writeEvent(EventType.STEP_FINISHED, { stepName: `round:${round}` });
      roundSpan.end();
      break;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      sse.writeEvent(EventType.RUN_ERROR, { message });
      sse.writeEvent(EventType.STEP_FINISHED, { stepName: `round:${round}` });
      roundSpan.end();
      break;
    }
  }

  // Clean up abort controller
  abortController.abort();

  // Finalize timing
  turnSpan.end();
  const timingSpans = timing.toJSON();

  // Update conversation metadata
  const freshConv = getConversation(conversationId) || conv;
  freshConv.updatedAt = Date.now();
  saveConversation(freshConv);

  // Emit timing data
  sse.writeEvent(EventType.CUSTOM, {
    name: "timing",
    value: { spans: timingSpans },
  });

  // Emit RUN_FINISHED
  const stopReason = abortController.signal.aborted
    ? "abort"
    : turnResult.pendingToolCalls
      ? "pending_tool_calls"
      : "end_turn";

  sse.writeEvent(EventType.RUN_FINISHED, {
    threadId: conversationId,
    runId,
    result: {
      stopReason,
      ...(turnResult.pendingToolCalls
        ? {
            pendingToolCalls: turnResult.pendingToolCalls,
            resolvedToolResults: turnResult.resolvedToolResults,
          }
        : {}),
    },
  });
  sse.close();

  return turnResult;
}
