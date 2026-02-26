import { useState, type FC } from "react";
import { BugIcon, XIcon, ZapIcon, ArchiveIcon, ListChecksIcon } from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import { useThreadListStore } from "../../stores/threadListStore";

const TASK_PRESETS = [
  { key: "planning", label: "Planning", color: "text-blue-500" },
  { key: "execution", label: "Execution", color: "text-amber-500" },
  { key: "execution_progress", label: "In Progress", color: "text-green-500" },
  { key: "validation", label: "Validation", color: "text-purple-500" },
  { key: "clear", label: "Clear", color: "text-default-400" },
] as const;

async function debugCompact(threadId: string, keepRecent = 4) {
  const res = await fetch(`/api/debug/compact/${threadId}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ keep_recent: keepRecent }),
  });
  return res.json();
}

async function debugSetTaskState(threadId: string, preset: string) {
  const res = await fetch(`/api/debug/task-state/${threadId}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ preset }),
  });
  return res.json();
}

async function debugEmitEvent(threadId: string, name: string, value: unknown) {
  const res = await fetch("/api/debug/emit", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ thread_id: threadId, name, value }),
  });
  return res.json();
}

export const DebugPanel: FC = () => {
  const [open, setOpen] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const activeThreadId = useThreadListStore((s) => s.activeThreadId);

  const showStatus = (msg: string) => {
    setStatus(msg);
    setTimeout(() => setStatus(null), 2000);
  };

  const handleCompact = async () => {
    if (!activeThreadId) return showStatus("No active thread");
    try {
      const result = await debugCompact(activeThreadId);
      showStatus(result.compacted ? `Compacted ${result.consumed_count} msgs` : result.reason ?? "Nothing to compact");
    } catch {
      showStatus("Compact failed");
    }
  };

  const handleTaskState = async (preset: string) => {
    if (!activeThreadId) return showStatus("No active thread");
    try {
      const result = await debugSetTaskState(activeThreadId, preset);
      showStatus(result.ok ? `Set: ${result.mode}` : "Failed");
    } catch {
      showStatus("Task state failed");
    }
  };

  const handleEmit = async (name: string, value: unknown = {}) => {
    if (!activeThreadId) return showStatus("No active thread");
    try {
      await debugEmitEvent(activeThreadId, name, value);
      showStatus(`Emitted: ${name}`);
    } catch {
      showStatus("Emit failed");
    }
  };

  return (
    <div className="fixed bottom-4 right-4 z-50 flex flex-col items-end gap-2">
      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0, height: 0, transformOrigin: "bottom right" }}
            animate={{ opacity: 1, height: "auto" }}
            exit={{ opacity: 0, height: 0 }}
            transition={{ duration: 0.2, ease: "easeInOut" }}
            className="w-72 rounded-xl border border-default-200 bg-white dark:bg-default-50 shadow-xl overflow-hidden"
          >
            {/* Header */}
            <div className="flex items-center justify-between px-3 py-2 border-b border-default-200/50">
              <span className="text-xs font-semibold text-red-500 flex items-center gap-1.5">
                <BugIcon size={12} />
                Debug Panel
              </span>
              <button
                onClick={() => setOpen(false)}
                className="text-default-400 hover:text-default-600 transition-colors"
              >
                <XIcon size={14} />
              </button>
            </div>

            {/* Status */}
            <AnimatePresence>
              {status && (
                <motion.div
                  initial={{ height: 0, opacity: 0 }}
                  animate={{ height: "auto", opacity: 1 }}
                  exit={{ height: 0, opacity: 0 }}
                  transition={{ duration: 0.15 }}
                  className="overflow-hidden"
                >
                  <div className="px-3 py-1.5 bg-default-100/50 text-[11px] text-default-500 border-b border-default-200/50">
                    {status}
                  </div>
                </motion.div>
              )}
            </AnimatePresence>

            {/* Thread ID */}
            <div className="px-3 py-1.5 text-[10px] text-default-400 border-b border-default-200/30 font-mono truncate">
              {activeThreadId ?? "no thread"}
            </div>

            {/* Compaction */}
            <div className="px-3 py-2 border-b border-default-200/30">
              <div className="text-[10px] font-medium text-default-500 uppercase tracking-wider mb-1.5 flex items-center gap-1">
                <ArchiveIcon size={10} />
                Compaction
              </div>
              <button
                onClick={handleCompact}
                className="w-full text-left px-2 py-1 rounded text-xs text-default-600 hover:bg-default-100 transition-colors"
              >
                Force compact (keep 4)
              </button>
            </div>

            {/* Task State */}
            <div className="px-3 py-2 border-b border-default-200/30">
              <div className="text-[10px] font-medium text-default-500 uppercase tracking-wider mb-1.5 flex items-center gap-1">
                <ListChecksIcon size={10} />
                Task State
              </div>
              <div className="flex flex-wrap gap-1">
                {TASK_PRESETS.map(({ key, label, color }) => (
                  <button
                    key={key}
                    onClick={() => handleTaskState(key)}
                    className={`px-2 py-0.5 rounded text-[11px] font-medium ${color} bg-default-100/50 hover:bg-default-200/50 transition-colors`}
                  >
                    {label}
                  </button>
                ))}
              </div>
            </div>

            {/* Emit Event */}
            <div className="px-3 py-2">
              <div className="text-[10px] font-medium text-default-500 uppercase tracking-wider mb-1.5 flex items-center gap-1">
                <ZapIcon size={10} />
                Events
              </div>
              <div className="flex flex-wrap gap-1">
                <button
                  onClick={() => handleEmit("title_update", { title: "Debug Title " + Date.now() })}
                  className="px-2 py-0.5 rounded text-[11px] text-default-600 bg-default-100/50 hover:bg-default-200/50 transition-colors"
                >
                  Title Update
                </button>
                <button
                  onClick={() => handleEmit("usage_update", { inputTokens: 5000, outputTokens: 2000, cacheReadInputTokens: 1000, cacheCreationInputTokens: 500, contextWindow: 200000, totalCost: 0.05 })}
                  className="px-2 py-0.5 rounded text-[11px] text-default-600 bg-default-100/50 hover:bg-default-200/50 transition-colors"
                >
                  Usage Update
                </button>
                <button
                  onClick={() => handleEmit("compaction", {})}
                  className="px-2 py-0.5 rounded text-[11px] text-default-600 bg-default-100/50 hover:bg-default-200/50 transition-colors"
                >
                  Compaction Event
                </button>
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1.5 rounded-full bg-red-500/90 px-3 py-1.5 text-xs font-medium text-white shadow-lg hover:bg-red-500 transition-colors"
      >
        <BugIcon size={12} />
        Debug
      </button>
    </div>
  );
};
