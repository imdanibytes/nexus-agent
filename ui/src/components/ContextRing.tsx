import type { FC } from "react";
import { useThreadListStore } from "@/stores/threadListStore.js";
import { useUsageStore } from "@/stores/usageStore.js";
import { cn, Tooltip, TooltipContent, TooltipTrigger } from "@imdanibytes/nexus-ui";

const RADIUS = 9;
const STROKE = 2;
const SIZE = (RADIUS + STROKE) * 2;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(0)}k`;
  return String(n);
}

function formatCost(cost: number): string {
  if (cost >= 1) return `$${cost.toFixed(2)}`;
  if (cost >= 0.01) return `$${cost.toFixed(2)}`;
  if (cost > 0) return `$${cost.toFixed(3)}`;
  return "$0.00";
}

export const ContextRing: FC = () => {
  const activeThreadId = useThreadListStore((s) => s.activeThreadId);
  const usage = useUsageStore((s) =>
    activeThreadId ? s.usage[activeThreadId] : undefined,
  );

  if (!usage || !usage.contextWindow) return null;

  const percent = Math.min(
    100,
    (usage.contextTokens / usage.contextWindow) * 100,
  );
  const offset = CIRCUMFERENCE * (1 - percent / 100);

  const strokeColor =
    percent > 90
      ? "stroke-destructive"
      : percent > 70
        ? "stroke-amber-500"
        : "stroke-primary";

  const tooltip = `Context: ${Math.round(percent)}% (${formatTokens(usage.contextTokens)} / ${formatTokens(usage.contextWindow)} tokens)`;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div className="flex items-center gap-1.5 px-1">
          <svg
            width={SIZE}
            height={SIZE}
            viewBox={`0 0 ${SIZE} ${SIZE}`}
            className="shrink-0 -rotate-90"
          >
            <circle
              cx={SIZE / 2}
              cy={SIZE / 2}
              r={RADIUS}
              fill="none"
              strokeWidth={STROKE}
              className="stroke-muted-foreground/20"
            />
            <circle
              cx={SIZE / 2}
              cy={SIZE / 2}
              r={RADIUS}
              fill="none"
              strokeWidth={STROKE}
              strokeLinecap="round"
              strokeDasharray={CIRCUMFERENCE}
              strokeDashoffset={offset}
              className={cn("transition-all duration-300", strokeColor)}
            />
          </svg>
          <span className="text-[10px] tabular-nums text-muted-foreground">
            {formatCost(usage.totalCost)}
          </span>
        </div>
      </TooltipTrigger>
      <TooltipContent side="top" className="text-xs">
        {tooltip}
      </TooltipContent>
    </Tooltip>
  );
};
