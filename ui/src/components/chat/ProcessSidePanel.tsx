import { type FC, useCallback, useEffect, useMemo, useState } from "react";
import {
  CheckCircle2Icon,
  XCircleIcon,
  LoaderIcon,
  BanIcon,
  SquareIcon,
  ChevronRightIcon,
  ChevronLeftIcon,
  TerminalIcon,
} from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import { Button, Tooltip } from "@heroui/react";
import { cn } from "../../lib/utils";
import { useProcessStore, type BgProcess } from "../../stores/processStore";
import { useThreadListStore } from "../../stores/threadListStore";
import { stopProcess } from "../../api/client";

const EMPTY: BgProcess[] = [];

// ── Status icon ──

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

// ── Elapsed time helper ──

function elapsed(startedAt: string, completedAt?: string): string {
  const start = new Date(startedAt).getTime();
  const end = completedAt ? new Date(completedAt).getTime() : Date.now();
  const seconds = Math.floor((end - start) / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const remaining = seconds % 60;
  return `${minutes}m ${remaining}s`;
}

// ── Process row ──

const ProcessRow: FC<{ process: BgProcess }> = ({ process }) => {
  const handleStop = useCallback(async () => {
    await stopProcess(process.id);
  }, [process.id]);

  return (
    <div
      className={cn(
        "flex items-start gap-2 py-1.5 px-2 rounded-lg text-xs transition-colors",
        process.status === "running" && "bg-primary/5 dark:bg-primary/10",
        process.isError && "bg-danger/5 dark:bg-danger/10",
        process.status === "completed" &&
          !process.isError &&
          "opacity-60",
      )}
    >
      <StatusIcon status={process.status} />
      <div className="flex-1 min-w-0">
        <span
          className={cn(
            "block leading-tight truncate",
            process.status === "running" && "text-default-900 font-medium",
            process.status === "completed" &&
              !process.isError &&
              "text-default-400",
            process.status === "failed" && "text-danger-600",
            process.status === "cancelled" && "text-default-400",
          )}
        >
          {process.label}
        </span>
        {process.outputPreview && process.status !== "running" && (
          <span className="block text-[10px] text-default-400 truncate mt-0.5">
            {process.outputPreview}
          </span>
        )}
      </div>
      <div className="flex items-center gap-1 shrink-0">
        <span className="text-[10px] text-default-400 tabular-nums">
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
    </div>
  );
};

// ── Spring config ──

const SPRING = { type: "spring" as const, stiffness: 400, damping: 35 };
const FADE = { duration: 0.15 };

// ── Main panel ──

export const ProcessSidePanel: FC = () => {
  const activeThreadId = useThreadListStore((s) => s.activeThreadId);
  const processes = useProcessStore((s) =>
    activeThreadId ? (s.processes[activeThreadId] ?? EMPTY) : EMPTY,
  );
  const panelOpen = useProcessStore((s) => s.panelOpen);
  const setPanelOpen = useProcessStore((s) => s.setPanelOpen);

  const runningCount = useMemo(
    () => processes.filter((p) => p.status === "running").length,
    [processes],
  );

  // Tick every second to update elapsed timers for running processes
  const hasRunning = runningCount > 0;
  const [, setTick] = useState(0);
  useEffect(() => {
    if (!hasRunning) return;
    const id = setInterval(() => setTick((t) => t + 1), 1000);
    return () => clearInterval(id);
  }, [hasRunning]);

  // Sort: running first, then by start time descending
  const sorted = [...processes].sort((a, b) => {
    if (a.status === "running" && b.status !== "running") return -1;
    if (b.status === "running" && a.status !== "running") return 1;
    return new Date(b.startedAt).getTime() - new Date(a.startedAt).getTime();
  });

  const isEmpty = processes.length === 0;

  return (
    <motion.div
      className="shrink-0 h-full"
      initial={false}
      animate={{ width: panelOpen ? 256 : 44 }}
      transition={SPRING}
    >
      <div className="h-full rounded-xl bg-white/60 dark:bg-default-50/30 backdrop-blur-xl border border-default-200 dark:border-default-200/50 overflow-hidden">
        <AnimatePresence mode="wait" initial={false}>
          {panelOpen ? (
            <motion.div
              key="expanded"
              className="flex flex-col h-full"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={FADE}
            >
              {/* Header */}
              <div className="flex items-center justify-between gap-2 px-3 py-2.5 border-b border-default-200/50">
                <div className="flex items-center gap-2 min-w-0 flex-1">
                  <TerminalIcon
                    size={14}
                    className="text-default-400 shrink-0"
                  />
                  <span className="text-xs font-semibold truncate text-default-900">
                    Processes
                  </span>
                </div>
                <div className="flex items-center gap-1.5 shrink-0">
                  {runningCount > 0 && (
                    <span className="flex items-center gap-1 text-[10px] font-medium text-primary">
                      <span className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse" />
                      {runningCount}
                    </span>
                  )}
                  <Button
                    variant="light"
                    size="sm"
                    onPress={() => setPanelOpen(false)}
                    isIconOnly
                    className="h-6 w-6 min-w-6 p-0 text-default-400"
                  >
                    <ChevronLeftIcon size={12} />
                  </Button>
                </div>
              </div>

              {/* Process list */}
              <div className="flex-1 overflow-y-auto py-1.5 px-1.5">
                {isEmpty ? (
                  <div className="flex flex-col items-center justify-center h-full text-default-400">
                    <TerminalIcon size={20} className="mb-2 opacity-40" />
                    <span className="text-[11px]">No processes</span>
                  </div>
                ) : (
                  sorted.map((p) => (
                    <ProcessRow key={p.id} process={p} />
                  ))
                )}
              </div>
            </motion.div>
          ) : (
            <motion.div
              key="collapsed"
              className="flex flex-col items-center gap-1.5 py-2 px-1 h-full"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={FADE}
            >
              <Button
                variant="flat"
                size="sm"
                onPress={() => setPanelOpen(true)}
                isIconOnly
                className="h-7 w-7 min-w-7 rounded-lg"
                aria-label="Show processes"
              >
                <ChevronRightIcon size={14} />
              </Button>

              <Tooltip content="Processes" placement="right" delay={300}>
                <div className="p-1 text-default-400">
                  <TerminalIcon size={14} />
                </div>
              </Tooltip>

              {runningCount > 0 && (
                <div className="flex items-center justify-center">
                  <span className="flex items-center gap-1 text-[10px] font-medium text-primary">
                    <span className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse" />
                    {runningCount}
                  </span>
                </div>
              )}

              {/* Process status icons */}
              <div className="flex flex-col items-center gap-1 flex-1 overflow-y-auto py-1">
                {sorted.map((p) => (
                  <StatusIcon key={p.id} status={p.status} />
                ))}
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </motion.div>
  );
};
