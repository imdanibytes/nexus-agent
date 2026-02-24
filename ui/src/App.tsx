import { useEffect, useState } from "react";
import { Thread } from "./components/chat/Thread";
import { TopBar } from "./components/chat/TopBar";
import { ThreadDrawer } from "./components/chat/ThreadDrawer";
import { SettingsModal } from "./components/settings/SettingsModal";
import { useThreadListStore } from "./stores/threadListStore";
import { useProviderStore } from "./stores/providerStore";
import { useAgentStore } from "./stores/agentStore";
import { useUIStore, applyTheme } from "./stores/uiStore";
import { eventBus } from "./runtime/event-bus";

export default function App() {
  const [drawerOpen, setDrawerOpen] = useState(false);
  const theme = useUIStore((s) => s.theme);

  useEffect(() => {
    // Connect SSE and load all data on mount
    eventBus.connect();
    Promise.all([
      useThreadListStore.getState().loadThreads(),
      useProviderStore.getState().loadProviders(),
      useAgentStore.getState().loadAgents(),
    ]).then(() => {
      const path = window.location.pathname;

      // Deep link: /settings/{tab}
      const settingsMatch = path.match(/\/settings(?:\/(\w+))?/);
      if (settingsMatch) {
        useUIStore.getState().openSettings(settingsMatch[1] || "general");
      }

      // Restore active thread from URL path: /c/{conversationId}
      const threadMatch = path.replace(/\/settings.*/, "").match(/^\/c\/(.+)/);
      if (threadMatch) {
        useThreadListStore.getState().switchThread(threadMatch[1]);
      }
    });
    return () => eventBus.disconnect();
  }, []);

  // Apply theme on mount and react to system preference changes
  useEffect(() => {
    applyTheme(theme);
    if (theme !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => applyTheme("system");
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [theme]);

  return (
    <div className="relative flex h-full flex-col overflow-hidden rounded-2xl bg-white/80 dark:bg-default-50/40 backdrop-blur-xl border border-default-200 dark:border-default-200/50 shadow-sm dark:shadow-none">
      <TopBar onMenuPress={() => setDrawerOpen(true)} />
      <div className="relative flex-1 min-h-0">
        <Thread />
        <ThreadDrawer
          isOpen={drawerOpen}
          onClose={() => setDrawerOpen(false)}
        />
      </div>
      <SettingsModal />
    </div>
  );
}
