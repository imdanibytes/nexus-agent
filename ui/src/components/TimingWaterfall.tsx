import { useMemo, useState } from "react";
import type { TimingSpan } from "@/stores/chatStore.js";
import { cn } from "@/lib/utils";

interface TreeNode {
  span: TimingSpan;
  children: TreeNode[];
  depth: number;
}

function buildTree(spans: TimingSpan[]): TreeNode[] {
  const byId = new Map<string, TreeNode>();
  const roots: TreeNode[] = [];

  for (const span of spans) {
    byId.set(span.id, { span, children: [], depth: 0 });
  }

  for (const span of spans) {
    const node = byId.get(span.id)!;
    if (span.parentId && byId.has(span.parentId)) {
      const parent = byId.get(span.parentId)!;
      node.depth = parent.depth + 1;
      parent.children.push(node);
    } else {
      roots.push(node);
    }
  }

  // Sort children by start time
  function sortChildren(node: TreeNode) {
    node.children.sort((a, b) => a.span.startMs - b.span.startMs);
    node.children.forEach(sortChildren);
  }
  roots.sort((a, b) => a.span.startMs - b.span.startMs);
  roots.forEach(sortChildren);

  return roots;
}

function flatten(nodes: TreeNode[]): TreeNode[] {
  const result: TreeNode[] = [];
  function walk(node: TreeNode) {
    result.push(node);
    node.children.forEach(walk);
  }
  nodes.forEach(walk);
  return result;
}

// Color mapping by span name prefix
function getBarColor(name: string): string {
  if (name === "turn") return "bg-transparent";
  if (name.startsWith("round:")) return "bg-transparent";
  if (name === "setup") return "bg-slate-400/60 dark:bg-slate-500/50";
  if (name === "fetch_settings") return "bg-slate-400 dark:bg-slate-500";
  if (name === "build_tool_executor") return "bg-slate-400/80 dark:bg-slate-500/70";
  if (name === "fetch_mcp_tools") return "bg-amber-400 dark:bg-amber-500";
  if (name === "system_message") return "bg-violet-400/60 dark:bg-violet-500/50";
  if (name.startsWith("provider:")) return "bg-violet-400 dark:bg-violet-500";
  if (name === "llm_call") return "bg-blue-400 dark:bg-blue-500";
  if (name === "tool_execution") return "bg-transparent";
  if (name.startsWith("tool:")) return "bg-emerald-400 dark:bg-emerald-500";
  return "bg-muted-foreground/40";
}

function getBarBorder(name: string): string {
  if (name === "turn") return "border border-border rounded-sm";
  if (name.startsWith("round:")) return "border border-border/50 rounded-sm";
  if (name === "tool_execution") return "border border-dashed border-emerald-400/40 dark:border-emerald-500/30 rounded-sm";
  return "";
}

function formatDuration(ms: number): string {
  if (ms < 1) return `${(ms * 1000).toFixed(0)}us`;
  if (ms < 1000) return `${ms.toFixed(1)}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

function formatName(name: string): string {
  // Clean up display names
  if (name.startsWith("provider:")) return name.slice(9);
  if (name.startsWith("tool:")) return name.slice(5);
  if (name.startsWith("round:")) return `Round ${name.slice(6)}`;
  return name.replace(/_/g, " ");
}

function getCategory(name: string): string {
  if (name === "setup" || name === "fetch_settings" || name === "build_tool_executor" || name === "fetch_mcp_tools")
    return "setup";
  if (name === "system_message" || name.startsWith("provider:")) return "system";
  if (name === "llm_call") return "llm";
  if (name === "tool_execution" || name.startsWith("tool:")) return "tool";
  return "other";
}

// Generate gridline positions
function getGridlines(totalMs: number): number[] {
  if (totalMs <= 0) return [];
  // Pick a nice interval
  const intervals = [10, 25, 50, 100, 250, 500, 1000, 2500, 5000, 10000];
  const targetCount = 6;
  const ideal = totalMs / targetCount;
  const interval = intervals.find((i) => i >= ideal) ?? intervals[intervals.length - 1];
  const lines: number[] = [];
  for (let t = interval; t < totalMs; t += interval) {
    lines.push(t);
  }
  return lines;
}

export function TimingWaterfall({ spans }: { spans: TimingSpan[] }) {
  const [hoveredId, setHoveredId] = useState<string | null>(null);

  const { rows, totalMs, gridlines } = useMemo(() => {
    const tree = buildTree(spans);
    const flat = flatten(tree);
    // Filter out the root 'turn' span from rows — its endMs defines the scale
    const turnSpan = flat.find((n) => n.span.name === "turn");
    const total = turnSpan ? turnSpan.span.endMs : Math.max(...spans.map((s) => s.endMs), 1);
    // Skip the root turn span from display — it's redundant with the total
    const display = flat.filter((n) => n.span.name !== "turn");
    return {
      rows: display,
      totalMs: total,
      gridlines: getGridlines(total),
    };
  }, [spans]);

  if (rows.length === 0) return null;

  return (
    <div className="flex flex-col text-xs font-mono">
      {/* Header */}
      <div className="flex items-center border-b border-border pb-1 mb-1 text-muted-foreground">
        <div className="w-40 shrink-0 px-1 font-semibold">Span</div>
        <div className="flex-1 relative h-4">
          {/* Timeline labels */}
          <span className="absolute left-0">0ms</span>
          {gridlines.map((t) => (
            <span
              key={t}
              className="absolute -translate-x-1/2"
              style={{ left: `${(t / totalMs) * 100}%` }}
            >
              {formatDuration(t)}
            </span>
          ))}
          <span className="absolute right-0">{formatDuration(totalMs)}</span>
        </div>
        <div className="w-20 shrink-0 px-1 text-right font-semibold">Duration</div>
      </div>

      {/* Rows */}
      <div className="relative">
        {rows.map((node) => {
          const { span } = node;
          const leftPct = (span.startMs / totalMs) * 100;
          const widthPct = Math.max((span.durationMs / totalMs) * 100, 0.3);
          const isContainer = span.name === "turn" || span.name.startsWith("round:") || span.name === "tool_execution" || span.name === "setup";
          const isHovered = hoveredId === span.id;

          return (
            <div
              key={span.id}
              className={cn(
                "flex items-center h-6 group",
                isHovered ? "bg-accent/50" : "hover:bg-accent/30",
              )}
              onMouseEnter={() => setHoveredId(span.id)}
              onMouseLeave={() => setHoveredId(null)}
            >
              {/* Name column */}
              <div
                className="w-40 shrink-0 px-1 truncate"
                style={{ paddingLeft: `${node.depth * 12 + 4}px` }}
                title={span.name}
              >
                <span
                  className={cn(
                    "inline-block w-2 h-2 rounded-full mr-1.5 shrink-0",
                    getBarColor(span.name).replace("bg-transparent", "bg-border"),
                  )}
                />
                <span className={cn(isContainer && "text-muted-foreground")}>
                  {formatName(span.name)}
                </span>
              </div>

              {/* Bar column */}
              <div className="flex-1 relative h-4">
                {/* Gridlines */}
                {gridlines.map((t) => (
                  <div
                    key={t}
                    className="absolute top-0 bottom-0 w-px bg-border/30"
                    style={{ left: `${(t / totalMs) * 100}%` }}
                  />
                ))}

                {/* The bar */}
                <div
                  className={cn(
                    "absolute top-0.5 bottom-0.5 rounded-[2px] min-w-[2px]",
                    getBarColor(span.name),
                    getBarBorder(span.name),
                  )}
                  style={{
                    left: `${leftPct}%`,
                    width: `${widthPct}%`,
                  }}
                />

                {/* Markers */}
                {span.markers?.map((marker, i) => {
                  const markerPct = (marker.timeMs / totalMs) * 100;
                  return (
                    <div
                      key={`${span.id}-m-${i}`}
                      className="absolute top-0 bottom-0 flex items-center group/marker z-10"
                      style={{ left: `${markerPct}%` }}
                    >
                      <div className="w-2 h-2 rotate-45 bg-rose-500 dark:bg-rose-400 border border-rose-700 dark:border-rose-300 -translate-x-1 cursor-pointer" />
                      <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-1 hidden group-hover/marker:block pointer-events-none">
                        <div className="whitespace-nowrap rounded bg-popover border border-border px-1.5 py-0.5 text-[10px] text-popover-foreground shadow-md">
                          {marker.label} <span className="text-muted-foreground">{formatDuration(marker.timeMs)}</span>
                        </div>
                      </div>
                    </div>
                  );
                })}
              </div>

              {/* Duration column */}
              <div className="w-20 shrink-0 px-1 text-right tabular-nums text-muted-foreground">
                {formatDuration(span.durationMs)}
              </div>
            </div>
          );
        })}
      </div>

      {/* Legend */}
      <div className="flex gap-3 mt-3 pt-2 border-t border-border text-muted-foreground flex-wrap">
        {[
          { label: "Setup", color: "bg-slate-400 dark:bg-slate-500" },
          { label: "Network", color: "bg-amber-400 dark:bg-amber-500" },
          { label: "System msg", color: "bg-violet-400 dark:bg-violet-500" },
          { label: "LLM", color: "bg-blue-400 dark:bg-blue-500" },
          { label: "Tool", color: "bg-emerald-400 dark:bg-emerald-500" },
        ].map(({ label, color }) => (
          <div key={label} className="flex items-center gap-1">
            <div className={cn("w-2.5 h-2.5 rounded-[2px]", color)} />
            <span>{label}</span>
          </div>
        ))}
        <div className="flex items-center gap-1">
          <div className="w-2 h-2 rotate-45 bg-rose-500 dark:bg-rose-400" />
          <span>Marker</span>
        </div>
      </div>

      {/* Hover detail */}
      {hoveredId && (() => {
        const span = spans.find((s) => s.id === hoveredId);
        if (!span) return null;
        const hasMetadata = span.metadata && Object.keys(span.metadata).length > 0;
        const hasMarkers = span.markers && span.markers.length > 0;
        if (!hasMetadata && !hasMarkers) return null;
        return (
          <div className="mt-2 pt-2 border-t border-border text-muted-foreground space-y-1">
            {hasMetadata && (
              <div>
                {Object.entries(span.metadata!).map(([k, v]) => (
                  <span key={k} className="mr-3">
                    <span className="text-foreground">{k}:</span> {String(v)}
                  </span>
                ))}
              </div>
            )}
            {hasMarkers && (
              <div className="flex flex-wrap gap-x-3 gap-y-0.5">
                {span.markers!.map((m, i) => (
                  <span key={i} className="inline-flex items-center gap-1">
                    <span className="w-1.5 h-1.5 rotate-45 bg-rose-500 dark:bg-rose-400 inline-block" />
                    <span className="text-foreground">{m.label}</span>
                    <span className="tabular-nums">{formatDuration(m.timeMs)}</span>
                  </span>
                ))}
              </div>
            )}
          </div>
        );
      })()}
    </div>
  );
}
