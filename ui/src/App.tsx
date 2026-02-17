import { useEffect } from "react";
import { ThreadList } from "./components/chat/ThreadList.js";
import { Thread } from "./components/chat/Thread.js";
import { SettingsPage } from "./components/settings/SettingsPage.js";
import { useChatStore } from "./stores/chatStore.js";
import { useThreadListStore } from "./stores/threadListStore.js";
import { useMcpTurnStore } from "./stores/mcpTurnStore.js";
import { eventBus } from "./runtime/event-bus.js";
import {
  fetchAgents,
  fetchProviders,
  fetchAvailableTools,
  getActiveAgent,
} from "./api/client.js";
import { Settings } from "lucide-react";
import { Button } from "@imdanibytes/nexus-ui";

function NexusApp() {
  const { setAgents, setActiveAgentId, setProviders, setAvailableTools } =
    useChatStore();
  const { settingsOpen, setSettingsOpen } = useChatStore();

  // Load agents, providers, and tools on startup
  useEffect(() => {
    fetchAgents().then(setAgents);
    fetchProviders().then(setProviders);
    fetchAvailableTools().then(setAvailableTools);
    getActiveAgent().then((r) => setActiveAgentId(r.agentId));
  }, [setAgents, setActiveAgentId, setProviders, setAvailableTools]);

  // Load thread list on startup
  useEffect(() => {
    useThreadListStore.getState().loadThreads();
  }, []);

  // Connect EventSource and register broadcast handlers
  useEffect(() => {
    eventBus.connect();

    const unsubTools = eventBus.on("tools_changed", () => {
      fetchAvailableTools().then(setAvailableTools);
    });

    const unsubMcpPending = eventBus.on("mcp_turn_pending", (event) => {
      const convId = event.value as { conversationId: string; userMessage: string };
      useMcpTurnStore.getState().setPendingTurn(convId.conversationId, convId.userMessage);
    });

    const unsubConvChanged = eventBus.on("conversations_changed", () => {
      useThreadListStore.getState().loadThreads();
    });

    return () => {
      unsubTools();
      unsubMcpPending();
      unsubConvChanged();
      eventBus.disconnect();
    };
  }, [setAvailableTools]);

  // MCP thread switching: when an MCP turn targets a different thread, switch to it
  const pendingConvId = useMcpTurnStore((s) => s.pendingConvId);

  useEffect(() => {
    if (!pendingConvId) return;
    const tls = useThreadListStore.getState();
    if (tls.activeThreadId !== pendingConvId) {
      tls.switchThread(pendingConvId);
    }
  }, [pendingConvId]);

  return (
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
        {settingsOpen ? <SettingsPage /> : <Thread />}
      </div>
    </div>
  );
}

export function App() {
  return <NexusApp />;
}
