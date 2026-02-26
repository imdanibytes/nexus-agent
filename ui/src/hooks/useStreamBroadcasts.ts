import { useEffect } from "react";
import { useThreadListStore } from "../stores/threadListStore";
import { useThreadStore } from "../stores/threadStore";
import { useUsageStore } from "../stores/usageStore";
import { useProcessStore, type BgProcess } from "../stores/processStore";
import { eventBus } from "../runtime/event-bus";

/** Register broadcast event handlers (title_update, usage_update). */
export function useStreamBroadcasts(): void {
  useEffect(() => {
    const unsubTitle = eventBus.on("title_update", (event) => {
      const val = event.value as { title?: string };
      if (val?.title && event.threadId) {
        useThreadListStore
          .getState()
          .updateThreadTitle(event.threadId, val.title);
      }
    });

    const unsubUsage = eventBus.on("usage_update", (event) => {
      const val = event.value as {
        inputTokens?: number;
        outputTokens?: number;
        cacheReadInputTokens?: number;
        cacheCreationInputTokens?: number;
        contextWindow?: number;
        totalCost?: number;
      };
      const threadId = event.threadId as string | undefined;
      if (threadId && val) {
        useUsageStore.getState().setUsage(threadId, {
          inputTokens: val.inputTokens ?? 0,
          outputTokens: val.outputTokens ?? 0,
          cacheReadInputTokens: val.cacheReadInputTokens ?? 0,
          cacheCreationInputTokens: val.cacheCreationInputTokens ?? 0,
          contextWindow: val.contextWindow ?? 200_000,
          totalCost: val.totalCost ?? 0,
        });
      }
    });

    const unsubCompaction = eventBus.on("compaction", (event) => {
      if (event.threadId) {
        useThreadStore.getState().loadHistory(event.threadId as string);
      }
    });

    const unsubBgStarted = eventBus.on("bg_process_started", (event) => {
      const proc = event.value as BgProcess | undefined;
      if (proc) {
        useProcessStore.getState().addProcess(proc);
      }
    });

    const unsubBgCompleted = eventBus.on("bg_process_completed", (event) => {
      const proc = event.value as BgProcess | undefined;
      if (proc) {
        useProcessStore.getState().updateProcess(proc.id, {
          status: proc.status,
          completedAt: proc.completedAt,
          exitCode: proc.exitCode,
          isError: proc.isError,
          outputPreview: proc.outputPreview,
          outputSize: proc.outputSize,
        });
        // Auto-remove from indicator after 5s
        setTimeout(() => {
          useProcessStore.getState().removeProcess(proc.id);
        }, 5000);
      }
    });

    const unsubBgCancelled = eventBus.on("bg_process_cancelled", (event) => {
      const proc = event.value as BgProcess | undefined;
      if (proc) {
        useProcessStore.getState().updateProcess(proc.id, {
          status: "cancelled",
          completedAt: proc.completedAt,
        });
        // Auto-remove from indicator after 5s
        setTimeout(() => {
          useProcessStore.getState().removeProcess(proc.id);
        }, 5000);
      }
    });

    return () => {
      unsubTitle();
      unsubUsage();
      unsubCompaction();
      unsubBgStarted();
      unsubBgCompleted();
      unsubBgCancelled();
    };
  }, []);
}
