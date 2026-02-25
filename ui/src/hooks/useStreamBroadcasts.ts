import { useEffect } from "react";
import { useThreadListStore } from "../stores/threadListStore";
import { useUsageStore } from "../stores/usageStore";
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
        contextWindow?: number;
        totalCost?: number;
      };
      const threadId = event.threadId as string | undefined;
      if (threadId && val) {
        useUsageStore.getState().setUsage(threadId, {
          inputTokens: val.inputTokens ?? 0,
          outputTokens: val.outputTokens ?? 0,
          contextWindow: val.contextWindow ?? 200_000,
          totalCost: val.totalCost ?? 0,
        });
      }
    });

    return () => {
      unsubTitle();
      unsubUsage();
    };
  }, []);
}
