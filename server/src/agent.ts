import Anthropic from "@anthropic-ai/sdk";
import { v4 as uuidv4 } from "uuid";
import { getSettings } from "./settings.js";
import { getConversation, saveConversation } from "./storage.js";
import { getToolSettings } from "./tool-settings.js";
import { resolveTurnConfig } from "./turn-config.js";
import { getToolRegistry } from "./tool-registry.js";
import { runRound } from "./round-runner.js";
import { SystemMessageBuilder } from "./system-message/builder.js";
import { corePromptProvider } from "./system-message/providers/core-prompt.js";
import { datetimeProvider } from "./system-message/providers/datetime.js";
import { conversationContextProvider } from "./system-message/providers/conversation-context.js";
import { messageBoundaryProvider } from "./system-message/providers/message-boundary.js";
import { SpanCollector } from "./timing.js";
import { CompactionPipeline, truncateOldToolResults } from "./compaction/pipeline.js";
import { toolResponsePruner } from "./compaction/passes/tool-response-pruner.js";
import { resolveContextWindow } from "./compaction/models.js";
import { resolvePrice, calculateCost } from "./compaction/pricing.js";
import type { Conversation, ConversationUsage, MessagePart, SseWriter, WireMessage } from "./types.js";
import type { ToolDefinition } from "./tools/types.js";
import { EventType, type PendingToolCall, type ResolvedToolResult } from "./ag-ui-types.js";

// Re-export WireMessage for consumers (sse-handler.ts etc.)
export type { WireMessage } from "./types.js";

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

// Compaction pipeline — register passes in escalation order
const compactionPipeline = new CompactionPipeline();
compactionPipeline.register(toolResponsePruner);

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
    return await _runAgentTurnInner(
      conversationId, wireMessages, sse, agentId, externalAbort, frontendTools,
    );
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    try {
      sse.writeEvent(EventType.RUN_ERROR, { message });
      sse.close();
    } catch {
      // SSE may already be closed
    }
    throw err;
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

  // ── 1. Setup ──
  const setupSpan = turnSpan.span("setup");

  const settingsSpan = setupSpan.span("fetch_settings");
  const settings = await getSettings();
  const toolSettings = await getToolSettings();
  settingsSpan.end();

  // ── 2. Resolve config ──
  const configSpan = setupSpan.span("resolve_config");
  const config = await resolveTurnConfig(agentId, settings);
  configSpan.end();

  console.log(
    `[agent] model=${config.model}` +
      (config.agent ? ` agent="${config.agent.name}"` : "") +
      ` messages=${wireMessages.length}`,
  );

  // ── 3. Build tools ──
  const toolSetupSpan = setupSpan.span("build_tools");
  const registry = await getToolRegistry(
    toolSettings.globalToolFilter,
    config.agent?.toolFilter,
    frontendTools,
  );
  toolSetupSpan.end();

  // ── 4. Load conversation ──
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

  // ── Abort controller ──
  const abortController = new AbortController();
  if (externalAbort) {
    if (externalAbort.aborted) {
      abortController.abort();
    } else {
      externalAbort.addEventListener(
        "abort",
        () => abortController.abort(),
        { once: true },
      );
    }
  }

  // ── Tool context ──
  const toolCtx = {
    conversationId,
    sse,
    conversation: conv,
    saveConversation,
    signal: abortController.signal,
  };

  // ── 5. Compaction (pre-turn) ──
  const compactionSpan = turnSpan.span("compaction");
  const contextWindow = await resolveContextWindow(config.model, config.provider);
  const lastUsage = conv.lastTokenUsage?.inputTokens ?? 0;
  const newCharsEstimate = wireMessages.reduce(
    (sum, m) =>
      sum +
      (m.content?.length ?? 0) +
      (m.toolCalls?.reduce((s, tc) => s + (tc.result?.length ?? 0), 0) ?? 0),
    0,
  );
  const estimatedTokens = lastUsage + Math.ceil(newCharsEstimate / 4);

  const compacted = compactionPipeline.run(wireMessages, {
    tokenUsage: estimatedTokens,
    tokenLimit: contextWindow,
    recentWindowSize: 4,
  });

  if (compacted.report.passesRun.length > 0) {
    sse.writeEvent(EventType.CUSTOM, {
      name: "compaction",
      value: compacted.report,
    });
  }
  compactionSpan.end();

  // ── 6. Build API messages ──
  const apiMessages = buildApiMessages(compacted.messages, registry.wireName);

  // ── 7. Round loop ──
  let round = 0;
  const maxRounds = settings.max_tool_rounds;
  const allAssistantParts: MessagePart[] = [];
  let turnResult: TurnResult = {};

  sse.writeEvent(EventType.RUN_STARTED, {
    threadId: conversationId,
    runId,
    ...(config.agent
      ? { agentId: config.agent.id, agentName: config.agent.name }
      : {}),
  });

  while (round < maxRounds) {
    if (abortController.signal.aborted) break;
    round++;

    const roundSpan = turnSpan.span(`round:${round}`, { round });

    // Build system message fresh each round (token usage may have changed)
    const smSpan = roundSpan.span("system_message");
    const systemMessage = await systemMessageBuilder.build(
      {
        conversationId,
        conversation: conv,
        toolNames: registry.definitions.map((d) => registry.wireName(d.name)),
        settings,
        tokenUsage: conv.lastTokenUsage
          ? {
              input: conv.lastTokenUsage.inputTokens,
              output: conv.lastTokenUsage.outputTokens,
              limit: contextWindow,
            }
          : undefined,
        agent: config.agent ?? null,
      },
      smSpan,
    );
    smSpan.end();

    const result = await runRound({
      client: config.client,
      model: config.model,
      maxTokens: config.maxTokens,
      temperature: config.temperature,
      topP: config.topP,
      systemMessage,
      apiMessages,
      toolRegistry: registry,
      toolCtx,
      messageId,
      signal: abortController.signal,
      sse,
      roundNumber: round,
      parentSpan: roundSpan,
    });

    allAssistantParts.push(...result.assistantParts);

    // Update token tracking
    if (result.tokenUsage) {
      conv.lastTokenUsage = {
        inputTokens: result.tokenUsage.inputTokens,
        outputTokens: result.tokenUsage.outputTokens,
        timestamp: Date.now(),
      };

      // Accumulate cumulative usage + cost
      const pricing = resolvePrice(config.model, config.provider);
      const turnCost = calculateCost(
        result.tokenUsage.inputTokens,
        result.tokenUsage.outputTokens,
        pricing,
      );

      if (!conv.usage) {
        conv.usage = {
          totalInputTokens: 0,
          totalOutputTokens: 0,
          totalCost: 0,
          contextTokens: 0,
          contextWindow: 0,
        };
      }
      conv.usage.totalInputTokens += result.tokenUsage.inputTokens;
      conv.usage.totalOutputTokens += result.tokenUsage.outputTokens;
      conv.usage.totalCost += turnCost;
      conv.usage.contextTokens = result.tokenUsage.inputTokens;
      conv.usage.contextWindow = contextWindow;
    }

    roundSpan.end();

    // Continue the tool loop or break
    if (result.stopReason === "tool_use" && result.newApiMessages) {
      apiMessages.push(...result.newApiMessages);
      truncateOldToolResults(apiMessages, 6);
      continue;
    }

    if (result.stopReason === "tool_use" && result.pendingToolCalls) {
      turnResult = {
        pendingToolCalls: result.pendingToolCalls,
        resolvedToolResults: result.resolvedToolResults,
      };
    }

    break;
  }

  // ── 8. Cleanup ──
  abortController.abort();
  turnSpan.end();
  const timingSpans = timing.toJSON();

  // ── 9. Persist conversation ──
  mergeAndSave(conv, conversationId);

  // ── 10. Emit timing + RUN_FINISHED ──
  sse.writeEvent(EventType.CUSTOM, {
    name: "timing",
    value: { spans: timingSpans },
  });

  if (conv.usage) {
    sse.writeEvent(EventType.CUSTOM, {
      name: "usage",
      value: conv.usage,
    });
  }

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

/** Merge agent-owned fields onto the latest disk copy and save. */
function mergeAndSave(agentConv: Conversation, id: string): void {
  const disk = getConversation(id) || agentConv;
  disk.updatedAt = Date.now();
  if (agentConv.lastTokenUsage) {
    disk.lastTokenUsage = agentConv.lastTokenUsage;
  }
  if (agentConv.usage) {
    disk.usage = agentConv.usage;
  }
  saveConversation(disk);
}
