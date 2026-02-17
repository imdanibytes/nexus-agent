import Anthropic from "@anthropic-ai/sdk";
import type {
  CompactionPass,
  CompactionContext,
  CompactionResult,
  CompactionReport,
} from "./types.js";
import type { WireMessage } from "../types.js";

function emptyReport(): CompactionReport {
  return { passesRun: [], entries: [], estimatedTokensSaved: 0 };
}

function mergeReports(a: CompactionReport, b: CompactionReport): CompactionReport {
  return {
    passesRun: [...a.passesRun, ...b.passesRun],
    entries: [...a.entries, ...b.entries],
    estimatedTokensSaved: a.estimatedTokensSaved + b.estimatedTokensSaved,
  };
}

/**
 * Threshold-triggered compaction pipeline.
 *
 * Each registered pass has an activation threshold (0.0–1.0).
 * At runtime, the pipeline checks (tokenUsage / tokenLimit) and runs
 * all passes whose threshold has been crossed, in threshold order.
 * Passes compose: output of pass N is input to pass N+1.
 */
export class CompactionPipeline {
  private passes: CompactionPass[] = [];

  register(pass: CompactionPass): void {
    this.passes.push(pass);
    // Keep sorted by threshold so passes run in escalation order
    this.passes.sort((a, b) => a.threshold - b.threshold);
  }

  run(messages: WireMessage[], ctx: CompactionContext): CompactionResult {
    const ratio = ctx.tokenLimit > 0 ? ctx.tokenUsage / ctx.tokenLimit : 0;
    const activePasses = this.passes.filter((p) => ratio >= p.threshold);

    if (activePasses.length === 0) {
      return { messages, report: emptyReport() };
    }

    let current = messages;
    let mergedReport = emptyReport();

    for (const pass of activePasses) {
      const result = pass.compact(current, ctx);
      current = result.messages;
      mergedReport = mergeReports(mergedReport, result.report);
    }

    if (mergedReport.passesRun.length > 0) {
      console.log(
        `[compaction] ratio=${(ratio * 100).toFixed(1)}% ` +
          `passes=[${mergedReport.passesRun.join(", ")}] ` +
          `entries=${mergedReport.entries.length} ` +
          `~${mergedReport.estimatedTokensSaved} tokens saved`,
      );
    }

    return { messages: current, report: mergedReport };
  }
}

/**
 * Lightweight inter-round truncation for accumulated apiMessages.
 *
 * During a multi-round tool loop, tool_result blocks pile up in the working
 * apiMessages array. This replaces old results (beyond the most recent N)
 * with a short placeholder to prevent unbounded context growth within a turn.
 *
 * Mutates in place — these are ephemeral working copies, not stored data.
 */
export function truncateOldToolResults(
  apiMessages: Anthropic.MessageParam[],
  keepRecentResults: number,
): { truncated: number; tokensSaved: number } {
  // Collect indices of all tool_result content blocks (walking backwards)
  const resultLocations: { msgIdx: number; blockIdx: number; size: number }[] = [];

  for (let i = apiMessages.length - 1; i >= 0; i--) {
    const msg = apiMessages[i];
    if (msg.role !== "user" || !Array.isArray(msg.content)) continue;

    for (let j = (msg.content as Anthropic.ContentBlockParam[]).length - 1; j >= 0; j--) {
      const block = (msg.content as Anthropic.ContentBlockParam[])[j];
      if (
        typeof block === "object" &&
        block !== null &&
        "type" in block &&
        block.type === "tool_result"
      ) {
        const content = (block as Anthropic.ToolResultBlockParam).content;
        const size = typeof content === "string" ? content.length : 0;
        resultLocations.push({ msgIdx: i, blockIdx: j, size });
      }
    }
  }

  // resultLocations is newest-first — skip the first `keepRecentResults`
  let truncated = 0;
  let tokensSaved = 0;

  for (let k = keepRecentResults; k < resultLocations.length; k++) {
    const loc = resultLocations[k];
    if (loc.size <= 200) continue; // not worth truncating small results

    const blocks = (apiMessages[loc.msgIdx].content as Anthropic.ToolResultBlockParam[]);
    const block = blocks[loc.blockIdx];
    const placeholder = `[Tool result truncated — ${loc.size} chars]`;
    (block as Anthropic.ToolResultBlockParam).content = placeholder;
    tokensSaved += Math.ceil((loc.size - placeholder.length) / 4);
    truncated++;
  }

  if (truncated > 0) {
    console.log(
      `[compaction:inter-round] truncated=${truncated} ~${tokensSaved} tokens saved`,
    );
  }

  return { truncated, tokensSaved };
}
