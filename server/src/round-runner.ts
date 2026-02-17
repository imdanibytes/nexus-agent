import Anthropic from "@anthropic-ai/sdk";
import type { ToolRegistry } from "./tool-registry.js";
import type { MessagePart, SseWriter } from "./types.js";
import type { ToolContext } from "./tools/types.js";
import {
  EventType,
  type PendingToolCall,
  type ResolvedToolResult,
} from "./ag-ui-types.js";
import type { SpanHandle } from "./timing.js";

export interface RoundParams {
  client: Anthropic;
  model: string;
  maxTokens: number;
  temperature?: number;
  topP?: number;
  systemMessage: string;
  apiMessages: Anthropic.MessageParam[];
  toolRegistry: ToolRegistry;
  toolCtx: ToolContext;
  messageId: string;
  signal: AbortSignal;
  sse: SseWriter;
  roundNumber: number;
  parentSpan: SpanHandle;
}

export interface RoundResult {
  stopReason: "end_turn" | "tool_use" | "abort" | "error";
  assistantParts: MessagePart[];
  tokenUsage?: { inputTokens: number; outputTokens: number };
  /** Messages to append to apiMessages for the next round (tool_use only). */
  newApiMessages?: Anthropic.MessageParam[];
  /** Frontend tools that need client-side execution. */
  pendingToolCalls?: PendingToolCall[];
  /** Server-side tool results already computed this round. */
  resolvedToolResults?: ResolvedToolResult[];
  error?: string;
}

/**
 * Execute a single LLM round: stream the response, process tool calls,
 * and return structured results for the orchestrator.
 */
export async function runRound(params: RoundParams): Promise<RoundResult> {
  const {
    client, model, maxTokens, temperature, topP,
    systemMessage, apiMessages, toolRegistry, toolCtx,
    messageId, signal, sse, roundNumber, parentSpan,
  } = params;

  if (signal.aborted) {
    return { stopReason: "abort", assistantParts: [] };
  }

  sse.writeEvent(EventType.STEP_STARTED, { stepName: `round:${roundNumber}` });

  try {
    console.log(`[agent] round=${roundNumber} calling LLM...`);
    const llmSpan = parentSpan.span("llm_call", { model, round: roundNumber });

    const stream = client.messages.stream({
      model,
      max_tokens: maxTokens,
      system: systemMessage,
      messages: apiMessages,
      tools: toolRegistry.anthropicTools.length > 0
        ? toolRegistry.anthropicTools
        : undefined,
      ...(temperature !== undefined
        ? { temperature }
        : topP !== undefined
          ? { top_p: topP }
          : {}),
    });

    const assistantParts: MessagePart[] = [];
    let stopReason: string | null = null;
    let firstTokenMarked = false;
    let textStarted = false;
    const toolUseBlocks: { id: string; name: string; partialJson: string }[] = [];

    // ── Stream processing ──
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
          const realName = toolRegistry.origName(block.name);
          toolUseBlocks.push({ id: block.id, name: realName, partialJson: "" });
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
              (assistantParts[i] as { type: "text"; text: string }).text +=
                delta.text;
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

    if (textStarted) {
      sse.writeEvent(EventType.TEXT_MESSAGE_END, { messageId });
    }
    for (const block of toolUseBlocks) {
      sse.writeEvent(EventType.TOOL_CALL_END, { toolCallId: block.id });
    }

    // Capture token usage
    let tokenUsage: RoundResult["tokenUsage"];
    try {
      const finalMsg = await stream.finalMessage();
      if (finalMsg.usage) {
        tokenUsage = {
          inputTokens: finalMsg.usage.input_tokens,
          outputTokens: finalMsg.usage.output_tokens,
        };
      }
    } catch {
      // Some providers don't support finalMessage()
    }

    llmSpan.end();

    // ── End of turn (no tool calls) ──
    if (stopReason !== "tool_use" || toolUseBlocks.length === 0) {
      sse.writeEvent(EventType.STEP_FINISHED, {
        stepName: `round:${roundNumber}`,
      });
      return { stopReason: "end_turn", assistantParts, tokenUsage };
    }

    // ── Tool execution ──
    const assistantContentBlocks: Anthropic.ContentBlockParam[] = [];
    for (const part of assistantParts) {
      if (part.type === "text" && part.text) {
        assistantContentBlocks.push({ type: "text", text: part.text });
      }
    }

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
        name: toolRegistry.wireName(block.name),
        input: args,
      });
      return { ...block, args };
    });

    const serverTools = parsed.filter((b) => toolRegistry.executor.has(b.name));
    const clientTools = parsed.filter((b) => !toolRegistry.executor.has(b.name));

    const toolExecSpan = parentSpan.span("tool_execution", {
      count: serverTools.length,
    });

    const serverResults = await Promise.all(
      serverTools.map((block) => {
        const toolSpan = toolExecSpan.span(`tool:${block.name}`, {
          toolName: block.name,
          toolUseId: block.id,
        });
        return toolRegistry.executor
          .execute(block.name, block.id, block.args, toolCtx)
          .finally(() => toolSpan.end());
      }),
    );

    toolExecSpan.end();

    // Emit results + build resolvedToolResults
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

    sse.writeEvent(EventType.STEP_FINISHED, {
      stepName: `round:${roundNumber}`,
    });

    // Frontend tools pending — orchestrator handles the break
    if (clientTools.length > 0) {
      return {
        stopReason: "tool_use",
        assistantParts,
        tokenUsage,
        pendingToolCalls: clientTools.map((b) => ({
          toolCallId: b.id,
          toolCallName: b.name,
          args: b.args,
        })),
        resolvedToolResults,
      };
    }

    // All server-side — build apiMessages for next round
    const toolResultBlocks: Anthropic.ToolResultBlockParam[] = serverResults.map(
      (r) => ({
        type: "tool_result" as const,
        tool_use_id: r.tool_use_id,
        content: r.content,
        is_error: r.is_error,
      }),
    );

    return {
      stopReason: "tool_use",
      assistantParts,
      tokenUsage,
      resolvedToolResults,
      newApiMessages: [
        { role: "assistant" as const, content: assistantContentBlocks },
        { role: "user" as const, content: toolResultBlocks },
      ],
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    sse.writeEvent(EventType.RUN_ERROR, { message });
    sse.writeEvent(EventType.STEP_FINISHED, {
      stepName: `round:${roundNumber}`,
    });
    return { stopReason: "error", assistantParts: [], error: message };
  }
}
