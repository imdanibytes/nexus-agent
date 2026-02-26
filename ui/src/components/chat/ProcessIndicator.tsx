import { type FC, useMemo } from "react";
import { Popover, PopoverTrigger, PopoverContent } from "@heroui/react";
import { useProcessStore } from "../../stores/processStore";
import { useThreadListStore } from "../../stores/threadListStore";
import { ProcessPanel } from "./ProcessPanel";

export const ProcessIndicator: FC = () => {
  const activeThreadId = useThreadListStore((s) => s.activeThreadId);
  const processes = useProcessStore((s) =>
    activeThreadId ? (s.processes[activeThreadId] ?? []) : [],
  );

  const runningCount = useMemo(
    () => processes.filter((p) => p.status === "running").length,
    [processes],
  );

  if (processes.length === 0) return null;

  return (
    <Popover placement="top-start" offset={8}>
      <PopoverTrigger>
        <button className="flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] font-medium text-default-500 hover:bg-default-100 transition-colors">
          {runningCount > 0 ? (
            <>
              <span className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse" />
              <span>{runningCount} running</span>
            </>
          ) : (
            <span>{processes.length} processes</span>
          )}
        </button>
      </PopoverTrigger>
      <PopoverContent className="w-72 p-1.5 bg-default-50 border border-default-200/50 backdrop-blur">
        {activeThreadId && (
          <ProcessPanel conversationId={activeThreadId} />
        )}
      </PopoverContent>
    </Popover>
  );
};
