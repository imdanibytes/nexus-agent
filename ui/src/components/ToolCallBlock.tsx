import { useState } from "react";
import { ChevronDown, ChevronRight, Wrench, AlertCircle, CheckCircle2 } from "lucide-react";
import type { ToolCallInfo } from "../api/client.js";

interface Props {
  toolCall: ToolCallInfo;
}

export function ToolCallBlock({ toolCall }: Props) {
  const [expanded, setExpanded] = useState(false);

  // Extract short tool name (last segment after dots)
  const shortName = toolCall.name.split(".").pop() || toolCall.name;

  return (
    <div className="my-2 rounded-lg border border-nx-border bg-nx-raised overflow-hidden">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center gap-2 w-full px-3 py-2 text-sm hover:bg-nx-surface transition-colors"
      >
        {expanded ? (
          <ChevronDown size={14} className="text-nx-muted" />
        ) : (
          <ChevronRight size={14} className="text-nx-muted" />
        )}
        <Wrench size={14} className="text-nx-accent" />
        <span className="font-mono text-xs px-1.5 py-0.5 bg-nx-accent/20 text-nx-accent rounded">
          {shortName}
        </span>
        <span className="flex-1" />
        {toolCall.result !== undefined && (
          toolCall.isError ? (
            <AlertCircle size={14} className="text-red-400" />
          ) : (
            <CheckCircle2 size={14} className="text-green-400" />
          )
        )}
        {toolCall.result === undefined && (
          <span className="text-xs text-nx-muted animate-pulse">running...</span>
        )}
      </button>

      {expanded && (
        <div className="border-t border-nx-border px-3 py-2 space-y-2">
          {Object.keys(toolCall.args).length > 0 && (
            <div>
              <div className="text-xs text-nx-muted mb-1">Arguments</div>
              <pre className="text-xs font-mono bg-nx-bg p-2 rounded overflow-x-auto">
                {JSON.stringify(toolCall.args, null, 2)}
              </pre>
            </div>
          )}
          {toolCall.result !== undefined && (
            <div>
              <div className="text-xs text-nx-muted mb-1">Result</div>
              <pre
                className={`text-xs font-mono p-2 rounded overflow-x-auto ${
                  toolCall.isError ? "bg-red-950/30 text-red-300" : "bg-nx-bg"
                }`}
              >
                {toolCall.result}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
