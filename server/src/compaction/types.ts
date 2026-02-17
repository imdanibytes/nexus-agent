import type { WireMessage } from "../types.js";

/** Context passed to each compaction pass */
export interface CompactionContext {
  /** Estimated current input tokens */
  tokenUsage: number;
  /** Model context window size in tokens */
  tokenLimit: number;
  /** Number of recent user messages to protect from compaction (default: 4) */
  recentWindowSize: number;
}

/** A single compaction action taken on a tool result */
export interface CompactionEntry {
  messageIndex: number;
  toolCallId: string;
  toolName: string;
  action: "truncated" | "pruned" | "summarized";
  originalSize: number;
  compactedSize: number;
}

/** Report of what the pipeline did — sent to frontend via SSE, optionally persisted */
export interface CompactionReport {
  passesRun: string[];
  entries: CompactionEntry[];
  estimatedTokensSaved: number;
}

/** Output of a compaction pass or the full pipeline */
export interface CompactionResult {
  messages: WireMessage[];
  report: CompactionReport;
}

/** A single compaction strategy with an activation threshold */
export interface CompactionPass {
  name: string;
  /** Activation threshold as a ratio (0.0–1.0) of tokenUsage/tokenLimit */
  threshold: number;
  compact(messages: WireMessage[], ctx: CompactionContext): CompactionResult;
}
