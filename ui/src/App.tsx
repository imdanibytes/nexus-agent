import { useEffect, useState } from "react";
import { Routes, Route, useParams, useNavigate } from "react-router";
import { Thread } from "./components/chat/Thread";
import { TaskPanel } from "./components/chat/TaskPanel";
import { TopBar } from "./components/chat/TopBar";
import { ThreadDrawer } from "./components/chat/ThreadDrawer";
import { SettingsModal } from "./components/settings/SettingsModal";
import { useThreadListStore } from "./stores/threadListStore";
import { useProviderStore } from "./stores/providerStore";
import { useAgentStore } from "./stores/agentStore";
import { useMcpStore } from "./stores/mcpStore";
import { useWorkspaceStore } from "./stores/workspaceStore";
import { useUIStore, applyTheme } from "./stores/uiStore";
import { eventBus } from "./runtime/event-bus";

export default function App() {
  const theme = useUIStore((s) => s.theme);

  useEffect(() => {
    eventBus.connect();
    Promise.all([
      useThreadListStore.getState().loadThreads(),
      useProviderStore.getState().loadProviders(),
      useAgentStore.getState().loadAgents(),
      useMcpStore.getState().loadServers(),
      useWorkspaceStore.getState().loadWorkspaces(),
    ]);
    return () => eventBus.disconnect();
  }, []);

  useEffect(() => {
    applyTheme(theme);
    if (theme !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => applyTheme("system");
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [theme]);

  return (
    <Routes>
      <Route path="/c/:threadId" element={<AppShell />} />
      <Route path="*" element={<AppShell />} />
    </Routes>
  );
}

function AppShell() {
  const [drawerOpen, setDrawerOpen] = useState(false);
  const params = useParams<{ threadId?: string }>();
  const navigate = useNavigate();

  // Sync route threadId → store
  useEffect(() => {
    useThreadListStore.getState().setActiveThread(params.threadId ?? null);
  }, [params.threadId]);

  // Deep-link: /settings/{tab} on initial load only
  useEffect(() => {
    const settingsMatch = window.location.pathname.match(/\/settings(?:\/(\w+))?/);
    if (settingsMatch) {
      useUIStore.getState().openSettings(settingsMatch[1] || "general");
    }
  }, []);

  return (
    <div className="relative flex h-full flex-col overflow-hidden rounded-2xl bg-white/80 dark:bg-default-50/40 backdrop-blur-xl border border-default-200 dark:border-default-200/50 shadow-sm dark:shadow-none">
      <TopBar onMenuPress={() => setDrawerOpen(true)} />
      <div className="flex flex-1 min-h-0 gap-2 p-2">
        <div className="relative flex-1 min-w-0">
          <Thread />
          <ThreadDrawer
            isOpen={drawerOpen}
            onClose={() => setDrawerOpen(false)}
            navigate={navigate}
          />
        </div>
        <TaskPanel />
      </div>
      <SettingsModal />
    </div>
  );
}
