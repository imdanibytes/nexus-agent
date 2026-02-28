import { useEffect } from "react";
import { useThreadListStore } from "../stores/threadListStore";
import { useThreadStore } from "../stores/threadStore";
import { useUsageStore } from "../stores/usageStore";
import { useProcessStore, type BgProcess } from "../stores/processStore";
import { useAgentStore } from "../stores/agentStore";
import { useProviderStore } from "../stores/providerStore";
import { useProjectStore } from "../stores/projectStore";
import { useWorkspaceStore } from "../stores/workspaceStore";
import { useUIStore } from "../stores/uiStore";
import { eventBus } from "../runtime/event-bus";
import { consumeStream } from "../lib/stream-consumer";
import { snowflake } from "../lib/snowflake";
import { mcpAppService } from "../lib/mcp-app-service";

/** Register broadcast event handlers (title_update, usage_update). */
export function useStreamBroadcasts(): void {
  // Sync MCP app iframes when theme changes
  const theme = useUIStore((s) => s.theme);
  useEffect(() => {
    mcpAppService.broadcastContextChange();
  }, [theme]);

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
        // Don't disconnect SSE here — the server will spawn a follow-up turn
        // for the bg process notification, and RUN_FINISHED will handle disconnect.
      }
    });

    const unsubBgCancelled = eventBus.on("bg_process_cancelled", (event) => {
      const proc = event.value as BgProcess | undefined;
      if (proc) {
        useProcessStore.getState().updateProcess(proc.id, {
          status: "cancelled",
          completedAt: proc.completedAt,
        });
        // Don't disconnect SSE here — same as bg_process_completed above.
      }
    });

    // Thread list sync (cross-tab)
    const unsubThreadCreated = eventBus.on("thread_created", () => {
      useThreadListStore.getState().loadThreads();
    });

    const unsubThreadDeleted = eventBus.on("thread_deleted", (event) => {
      const id = (event.value as { id?: string })?.id;
      if (id) useThreadListStore.getState().removeThread(id);
    });

    // Agent sync (cross-tab)
    const unsubAgentCreated = eventBus.on("agent_created", () => {
      useAgentStore.getState().loadAgents();
    });

    const unsubAgentUpdated = eventBus.on("agent_updated", () => {
      useAgentStore.getState().loadAgents();
    });

    const unsubAgentDeleted = eventBus.on("agent_deleted", () => {
      useAgentStore.getState().loadAgents();
    });

    const unsubActiveAgent = eventBus.on("active_agent_changed", (event) => {
      const id = (event.value as { agent_id?: string | null })?.agent_id ?? null;
      useAgentStore.setState({ activeAgentId: id });
    });

    // Provider sync (cross-tab)
    const unsubProviderCreated = eventBus.on("provider_created", () => {
      useProviderStore.getState().loadProviders();
    });

    const unsubProviderUpdated = eventBus.on("provider_updated", () => {
      useProviderStore.getState().loadProviders();
    });

    const unsubProviderDeleted = eventBus.on("provider_deleted", () => {
      useProviderStore.getState().loadProviders();
    });

    // Project sync (cross-tab)
    const unsubProjCreated = eventBus.on("project_created", () => {
      useProjectStore.getState().loadProjects();
    });

    const unsubProjUpdated = eventBus.on("project_updated", () => {
      useProjectStore.getState().loadProjects();
    });

    const unsubProjDeleted = eventBus.on("project_deleted", () => {
      useProjectStore.getState().loadProjects();
    });

    // Per-conversation agent change
    const unsubAgentChanged = eventBus.on("agent_changed", (event) => {
      const val = event.value as { id?: string; agent_id?: string | null };
      if (val?.id) {
        useThreadListStore
          .getState()
          .updateThreadAgent(val.id, val.agent_id ?? null);
      }
    });

    // Per-conversation workspace change
    const unsubWorkspaceChanged = eventBus.on("workspace_changed", (event) => {
      const val = event.value as { id?: string; workspace_id?: string | null };
      if (val?.id) {
        useThreadListStore
          .getState()
          .updateThreadWorkspace(val.id, val.workspace_id ?? null);
      }
    });

    // Workspace sync (cross-tab)
    const unsubWsCreated = eventBus.on("workspace_created", () => {
      useWorkspaceStore.getState().loadWorkspaces();
    });

    const unsubWsUpdated = eventBus.on("workspace_updated", () => {
      useWorkspaceStore.getState().loadWorkspaces();
    });

    const unsubWsDeleted = eventBus.on("workspace_deleted", () => {
      useWorkspaceStore.getState().loadWorkspaces();
    });

    // Auto-consume server-initiated turns. This handles:
    // - Follow-up turns (e.g., after bg_process_completed)
    // - Reconnection (SYNC replays buffered events, then RUN_STARTED arrives)
    // - Programmatic API triggers
    const autoConsumeControllers = new Map<string, AbortController>();

    function autoConsume(conversationId: string): void {
      // Only auto-consume if this conversation is NOT already streaming
      // (i.e., this is a server-initiated turn, not a user-initiated one)
      const conv = useThreadStore.getState().conversations[conversationId];
      if (conv?.isStreaming) return;

      console.debug(`[AutoConsume] Server-initiated turn for ${conversationId}`);

      const assistantMsgId = snowflake();
      useThreadStore.getState().startStreaming(conversationId, assistantMsgId);

      const controller = new AbortController();
      autoConsumeControllers.set(conversationId, controller);

      // No turnCtx — server handles persistence for follow-up turns.
      // After the stream ends, reload history to sync client state.
      consumeStream(conversationId, controller.signal).then(() => {
        autoConsumeControllers.delete(conversationId);
        useThreadStore.getState().loadHistory(conversationId);
      });
    }

    const unsubRunStarted = eventBus.on("RUN_STARTED", (event) => {
      const conversationId = event.threadId as string | undefined;
      if (!conversationId) return;
      autoConsume(conversationId);
    });

    // SYNC: sent on SSE connect with the list of active conversations.
    // Start auto-consuming each so replayed/live events stream in.
    // loadHistory (called by Thread.tsx on mount) will fetch persisted
    // messages and merge them under the streaming message.
    const unsubSync = eventBus.on("SYNC", (event) => {
      const activeRuns = event.activeRuns as string[] | undefined;
      if (!activeRuns?.length) return;
      console.debug(`[SYNC] Active runs:`, activeRuns);
      for (const conversationId of activeRuns) {
        autoConsume(conversationId);
      }
    });

    return () => {
      unsubTitle();
      unsubUsage();
      unsubCompaction();
      unsubBgStarted();
      unsubBgCompleted();
      unsubBgCancelled();
      unsubThreadCreated();
      unsubThreadDeleted();
      unsubAgentCreated();
      unsubAgentUpdated();
      unsubAgentDeleted();
      unsubActiveAgent();
      unsubProviderCreated();
      unsubProviderUpdated();
      unsubProviderDeleted();
      unsubProjCreated();
      unsubProjUpdated();
      unsubProjDeleted();
      unsubAgentChanged();
      unsubWorkspaceChanged();
      unsubWsCreated();
      unsubWsUpdated();
      unsubWsDeleted();
      unsubRunStarted();
      unsubSync();
      for (const c of autoConsumeControllers.values()) c.abort();
      autoConsumeControllers.clear();
    };
  }, []);
}
