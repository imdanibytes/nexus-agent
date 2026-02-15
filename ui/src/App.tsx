import { PanelLeftOpen } from "lucide-react";
import { Sidebar } from "./components/Sidebar.js";
import { ChatArea } from "./components/ChatArea.js";
import { useChatStore } from "./stores/chatStore.js";

export function App() {
  const { sidebarOpen, toggleSidebar } = useChatStore();

  return (
    <div className="flex h-full">
      <Sidebar />

      {!sidebarOpen && (
        <button
          onClick={toggleSidebar}
          className="absolute top-3 left-3 z-10 p-1.5 rounded-lg hover:bg-nx-surface transition-colors text-nx-muted"
        >
          <PanelLeftOpen size={16} />
        </button>
      )}

      <ChatArea />
    </div>
  );
}
