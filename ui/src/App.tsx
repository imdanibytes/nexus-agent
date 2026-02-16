import { useEffect } from "react";
import { ThreadList } from "./components/chat/ThreadList.js";
import { Thread } from "./components/chat/Thread.js";
import { SettingsPage } from "./components/settings/SettingsPage.js";
import { useChatStore } from "./stores/chatStore.js";
import { useThreadListStore } from "./stores/threadListStore.js";
import { useMcpTurnStore } from "./stores/mcpTurnStore.js";
import { wsClient } from "./runtime/ws-client.js";
import { turnRouter } from "./runtime/turn-router.js";
import {
  fetchAgents,
  fetchProviders,
  fetchAvailableTools,
  getActiveAgent,
} from "./api/client.js";
import { Settings } from "lucide-react";
import { Button } from "./components/ui/button.js";

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

  // Connect WebSocket and route events
  useEffect(() => {
    wsClient.connect();

    const unsubTools = wsClient.on("tools_changed", () => {
      fetchAvailableTools().then(setAvailableTools);
    });

    const unsubMcpPending = wsClient.on("mcp_turn_pending", (msg) => {
      const d = msg.data!;
      const convId = d.conversationId as string;
      const userMessage = d.userMessage as string;
      useMcpTurnStore.getState().setPendingTurn(convId, userMessage);
    });

    // Route all turn-scoped events to TurnRouter
    const turnEvents = [
      "turn_start",
      "text_start",
      "text_delta",
      "tool_start",
      "tool_input_delta",
      "tool_result",
      "tool_request",
      "ui_surface",
      "title_update",
      "timing",
      "turn_end",
      "error",
    ];

    const unsubTurn = turnEvents.map((type) =>
      wsClient.on(type, (msg) => turnRouter.route(msg)),
    );

    return () => {
      unsubTools();
      unsubMcpPending();
      unsubTurn.forEach((unsub) => unsub());
      wsClient.disconnect();
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
