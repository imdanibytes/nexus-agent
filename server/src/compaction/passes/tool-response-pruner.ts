import type { WireMessage } from "../../types.js";
import type {
  CompactionPass,
  CompactionContext,
  CompactionResult,
  CompactionEntry,
} from "../types.js";

const TOOL_RESULT_TRUNCATE_THRESHOLD = 1500;
const ERROR_RESULT_KEEP_CHARS = 500;

/**
 * Find the index in the message array where the "recent window" starts.
 * The recent window protects the last N user messages (and everything after them)
 * from compaction.
 */
function findRecentWindowStart(
  messages: WireMessage[],
  recentWindowSize: number,
): number {
  let userCount = 0;
  for (let i = messages.length - 1; i >= 0; i--) {
    if (messages[i].role === "user") {
      userCount++;
      if (userCount >= recentWindowSize) return i;
    }
  }
  // Fewer user messages than the window size — protect everything
  return 0;
}

/**
 * Tool Response Pruner — Pass 1 (threshold: 0.5)
 *
 * Heuristic-based truncation of old tool call results.
 * No LLM calls, zero latency cost.
 *
 * Rules:
 * 1. Messages inside the recent window are untouched
 * 2. Tool results > 1500 chars outside the window are truncated
 * 3. Error results keep the first 500 chars
 */
export const toolResponsePruner: CompactionPass = {
  name: "tool-response-pruner",
  threshold: 0.5,

  compact(
    messages: WireMessage[],
    ctx: CompactionContext,
  ): CompactionResult {
    const recentStart = findRecentWindowStart(messages, ctx.recentWindowSize);
    const entries: CompactionEntry[] = [];
    let tokensSaved = 0;

    const compacted = messages.map((msg, idx) => {
      // Protect recent window
      if (idx >= recentStart) return msg;

      // Only assistant messages have tool calls
      if (msg.role !== "assistant" || !msg.toolCalls) return msg;

      let modified = false;
      const newToolCalls = msg.toolCalls.map((tc) => {
        if (tc.result === undefined) return tc;

        const originalSize = tc.result.length;

        // Error results — keep first N chars
        if (tc.isError && originalSize > ERROR_RESULT_KEEP_CHARS) {
          const truncated = tc.result.slice(0, ERROR_RESULT_KEEP_CHARS) +
            `\n\n[Error output truncated — ${originalSize} chars total]`;
          const saved = Math.ceil((originalSize - truncated.length) / 4);
          tokensSaved += saved;
          modified = true;
          entries.push({
            messageIndex: idx,
            toolCallId: tc.id,
            toolName: tc.name,
            action: "truncated",
            originalSize,
            compactedSize: truncated.length,
          });
          return { ...tc, result: truncated };
        }

        // Large results — truncate
        if (originalSize > TOOL_RESULT_TRUNCATE_THRESHOLD) {
          const truncated =
            `[Tool result truncated — ${originalSize} chars, tool: ${tc.name}]`;
          const saved = Math.ceil((originalSize - truncated.length) / 4);
          tokensSaved += saved;
          modified = true;
          entries.push({
            messageIndex: idx,
            toolCallId: tc.id,
            toolName: tc.name,
            action: "truncated",
            originalSize,
            compactedSize: truncated.length,
          });
          return { ...tc, result: truncated };
        }

        return tc;
      });

      if (!modified) return msg;
      return { ...msg, toolCalls: newToolCalls };
    });

    return {
      messages: compacted,
      report: {
        passesRun: entries.length > 0 ? ["tool-response-pruner"] : [],
        entries,
        estimatedTokensSaved: tokensSaved,
      },
    };
  },
};
