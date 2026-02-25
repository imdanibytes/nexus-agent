import type { FC } from "react";
import { CircularProgress, Tooltip } from "@heroui/react";

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(0)}k`;
  return String(n);
}

function formatCost(cost: number): string {
  if (cost >= 0.01) return `$${cost.toFixed(2)}`;
  if (cost > 0) return `$${cost.toFixed(3)}`;
  return "$0.00";
}

export interface ContextRingProps {
  contextTokens: number;
  contextWindow: number;
  totalCost?: number;
  cacheReadInputTokens?: number;
  cacheCreationInputTokens?: number;
}

export const ContextRing: FC<ContextRingProps> = ({
  contextTokens,
  contextWindow,
  totalCost,
  cacheReadInputTokens,
  cacheCreationInputTokens,
}) => {
  if (contextWindow === 0) return null;

  const percent = Math.min(100, (contextTokens / contextWindow) * 100);
  const color = percent > 90 ? "danger" : percent > 70 ? "warning" : "primary";

  const costInfo = totalCost != null && totalCost > 0 ? ` · ${formatCost(totalCost)}` : "";
  const cacheRead = cacheReadInputTokens ?? 0;
  const cacheWrite = cacheCreationInputTokens ?? 0;
  const cacheInfo = cacheRead > 0 || cacheWrite > 0
    ? `\nCache: ${formatTokens(cacheRead)} read · ${formatTokens(cacheWrite)} write`
    : "";
  const tooltip = `Context: ${Math.round(percent)}% (${formatTokens(contextTokens)} / ${formatTokens(contextWindow)} tokens)${costInfo}${cacheInfo}`;

  return (
    <Tooltip content={tooltip} placement="top" className="text-xs">
      <div className="flex items-center gap-1.5 px-1 cursor-default">
        <CircularProgress
          size="sm"
          value={contextTokens}
          maxValue={contextWindow}
          color={color}
          aria-label="Context usage"
          classNames={{
            base: "w-5 h-5",
            svg: "w-5 h-5",
            track: "stroke-default-300/20",
          }}
        />
        {totalCost != null && totalCost > 0 && (
          <span className="text-[10px] tabular-nums text-default-500">
            {formatCost(totalCost)}
          </span>
        )}
      </div>
    </Tooltip>
  );
};
