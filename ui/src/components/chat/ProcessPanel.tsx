import { type FC, useCallback, useEffect, useState } from "react";
import {
  CheckCircle2Icon,
  XCircleIcon,
  LoaderIcon,
  BanIcon,
  SquareIcon,
} from "lucide-react";
import { Tooltip } from "@heroui/react";
import { cn } from "../../lib/utils";
import { useProcessStore, type BgProcess } from "../../stores/processStore";
import { stopProcess } from "../../api/client";

const EMPTY: BgProcess[] = [];

function StatusIcon({ status }: { status: BgProcess["status"] }) {
  switch (status) {
    case "running":
      return (
        <LoaderIcon
          size={13}
          className="text-primary animate-spin shrink-0 mt-px"
        />
      );
    case "completed":
      return (
        <CheckCircle2Icon
          size={13}
          className="text-success shrink-0 mt-px"
        />
      );
    case "failed":
      return (
        <XCircleIcon size={13} className="text-danger shrink-0 mt-px" />
      );
    case "cancelled":
      return (
        <BanIcon size={13} className="text-default-400 shrink-0 mt-px" />
      );
  }
}

function elapsed(startedAt: string, completedAt?: string): string {
  const start = new Date(startedAt).getTime();
  const end = completedAt ? new Date(completedAt).getTime() : Date.now();
  const seconds = Math.floor((end - start) / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const remaining = seconds % 60;
  return `${minutes}m ${remaining}s`;
}

const ProcessRow: FC<{ process: BgProcess }> = ({ process }) => {
  const handleStop = useCallback(async () => {
    await stopProcess(process.id);
  }, [process.id]);

  return (
    <div
      className={cn(
        "flex items-center gap-2 py-1.5 px-2 rounded-lg text-xs",
        process.status === "running" && "bg-primary/5 dark:bg-primary/10",
        process.isError && "bg-danger/5 dark:bg-danger/10",
      )}
    >
      <StatusIcon status={process.status} />
      <span className="flex-1 truncate text-default-700">{process.label}</span>
      <span className="text-[10px] text-default-400 tabular-nums shrink-0">
        {elapsed(process.startedAt, process.completedAt)}
      </span>
      {process.status === "running" && (
        <Tooltip content="Stop process" delay={500}>
          <button
            onClick={handleStop}
            className="p-0.5 rounded hover:bg-default-200/50 text-default-400 hover:text-danger transition-colors"
          >
            <SquareIcon size={11} />
          </button>
        </Tooltip>
      )}
    </div>
  );
};

export const ProcessPanel: FC<{ conversationId: string }> = ({
  conversationId,
}) => {
  const processes = useProcessStore((s) => s.processes[conversationId] ?? EMPTY);
  const hasRunning = processes.some((p) => p.status === "running");

  // Tick every second to update elapsed timers for running processes
  const [, setTick] = useState(0);
  useEffect(() => {
    if (!hasRunning) return;
    const id = setInterval(() => setTick((t) => t + 1), 1000);
    return () => clearInterval(id);
  }, [hasRunning]);

  if (processes.length === 0) return null;

  // Sort: running first, then by start time descending
  const sorted = [...processes].sort((a, b) => {
    if (a.status === "running" && b.status !== "running") return -1;
    if (b.status === "running" && a.status !== "running") return 1;
    return new Date(b.startedAt).getTime() - new Date(a.startedAt).getTime();
  });

  return (
    <div className="flex flex-col gap-0.5 py-1 max-h-44 overflow-y-auto overflow-x-hidden">
      {sorted.map((p) => (
        <ProcessRow key={p.id} process={p} />
      ))}
    </div>
  );
};
