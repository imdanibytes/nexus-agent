import { useCallback, useEffect, useMemo } from "react";
import {
  AssistantRuntimeProvider,
  useLocalRuntime,
  unstable_useRemoteThreadListRuntime as useRemoteThreadListRuntime,
  ExportedMessageRepository,
  useThreadListItem,
  useThreadListItemRuntime,
} from "@assistant-ui/react";
import { ThreadList } from "./components/assistant-ui/thread-list.js";
import { Thread } from "./components/assistant-ui/thread.js";
import { SettingsPage } from "./components/settings/SettingsPage.js";
import { useChatStore } from "./stores/chatStore.js";
import { createNexusAdapter } from "./runtime/adapter.js";
import { NexusThreadListAdapter, threadState } from "./runtime/thread-list-adapter.js";
import {
  fetchAgents,
  fetchProviders,
  fetchAvailableTools,
  getActiveAgent,
  fetchConversation,
  appendRepositoryMessage,
} from "./api/client.js";
import { convertToThreadMessage } from "./runtime/convert.js";
import { Settings } from "lucide-react";
import { Button } from "./components/ui/button.js";

function useNexusLocalRuntime() {
  // Get the remoteId from the thread list item context —
  // this is the server conversation ID for the current thread.
  const threadListItem = useThreadListItem({ optional: true });
  const remoteId = threadListItem?.remoteId ?? null;
  const threadListItemRuntime = useThreadListItemRuntime({ optional: true });

  // Keep threadState in sync so the chat adapter can read it during streaming
  if (remoteId) {
    threadState.activeConversationId = remoteId;
  }

  // Title update callback — renames the thread in the sidebar
  const onTitleUpdate = useCallback(
    (_convId: string, title: string) => {
      threadListItemRuntime?.rename(title);
    },
    [threadListItemRuntime],
  );

  const adapter = useMemo(
    () => createNexusAdapter(onTitleUpdate),
    [onTitleUpdate],
  );

  const historyAdapter = useMemo(
    () => ({
      async load() {
        if (!remoteId) return { messages: [] };

        const conv = await fetchConversation(remoteId);
        if (!conv) return { messages: [] };

        // Prefer tree repository (preserves branches)
        if (conv.repository?.messages?.length) {
          return { messages: conv.repository.messages as any };
        }

        // Fallback: convert flat messages to linear tree (old conversations)
        if (conv.messages?.length) {
          return ExportedMessageRepository.fromArray(
            conv.messages.map(convertToThreadMessage),
          );
        }

        return { messages: [] };
      },
      async append({ message, parentId }: { message: any; parentId: string | null }) {
        const convId = threadState.activeConversationId;
        if (!convId) return;
        await appendRepositoryMessage(convId, message, parentId);
      },
    }),
    [remoteId],
  );

  return useLocalRuntime(adapter, {
    adapters: { history: historyAdapter },
  });
}

function NexusApp() {
  const threadListAdapter = useMemo(() => new NexusThreadListAdapter(), []);
  const { setAgents, setActiveAgentId, setProviders, setAvailableTools } = useChatStore();

  // Load agents, providers, and tools on startup
  useEffect(() => {
    fetchAgents().then(setAgents);
    fetchProviders().then(setProviders);
    fetchAvailableTools().then(setAvailableTools);
    getActiveAgent().then((r) => setActiveAgentId(r.agentId));
  }, [setAgents, setActiveAgentId, setProviders, setAvailableTools]);

  // Subscribe to MCP tool list changes — refetch when tools change
  useEffect(() => {
    const es = new EventSource("/api/tool-events");
    es.addEventListener("tools_changed", () => {
      fetchAvailableTools().then(setAvailableTools);
    });
    return () => es.close();
  }, [setAvailableTools]);

  const runtime = useRemoteThreadListRuntime({
    runtimeHook: useNexusLocalRuntime,
    adapter: threadListAdapter,
  });

  const { settingsOpen, setSettingsOpen } = useChatStore();

  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <div className="flex h-full">
        {/* Sidebar */}
        <div className="w-64 h-full bg-background border-r border-border flex flex-col flex-shrink-0 overflow-hidden">
          <div className="flex-1 overflow-y-auto p-2">
            <ThreadList />
          </div>
          <div className="border-t border-border p-2">
            <Button
              variant={settingsOpen ? "secondary" : "ghost"}
              size="sm"
              onClick={() => setSettingsOpen(!settingsOpen)}
              className="w-full justify-start gap-2 h-9 text-muted-foreground hover:text-foreground"
            >
              <Settings size={14} />
              <span className="text-xs">Settings</span>
            </Button>
          </div>
        </div>

        {/* Main content */}
        <div className="flex-1 min-w-0 h-full min-h-0">
          {settingsOpen ? (
            <SettingsPage />
          ) : (
            <Thread />
          )}
        </div>
      </div>
    </AssistantRuntimeProvider>
  );
}

export function App() {
  return <NexusApp />;
}
