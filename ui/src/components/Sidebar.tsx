import { useEffect } from "react";
import { Plus, PanelLeftClose } from "lucide-react";
import { useChatStore } from "../stores/chatStore.js";
import { ConversationItem } from "./ConversationItem.js";
import { fetchConversations, deleteConversation as apiDelete } from "../api/client.js";
import { useStreamingChat } from "../hooks/useStreamingChat.js";

export function Sidebar() {
  const { conversations, activeId, sidebarOpen, setConversations, setActiveId, setMessages, toggleSidebar } =
    useChatStore();
  const { loadConversation } = useStreamingChat();

  useEffect(() => {
    fetchConversations().then(setConversations);
  }, [setConversations]);

  const handleNewChat = () => {
    setActiveId(null);
    setMessages([]);
  };

  const handleDelete = async (id: string) => {
    await apiDelete(id);
    const updated = await fetchConversations();
    setConversations(updated);
    if (activeId === id) {
      setActiveId(null);
      setMessages([]);
    }
  };

  if (!sidebarOpen) return null;

  return (
    <div className="w-60 h-full bg-nx-bg border-r border-nx-border flex flex-col">
      <div className="flex items-center justify-between p-3 border-b border-nx-border">
        <button
          onClick={handleNewChat}
          className="flex items-center gap-1.5 px-3 py-1.5 text-sm bg-nx-accent text-white rounded-lg hover:bg-nx-accent/80 transition-colors"
        >
          <Plus size={14} />
          New Chat
        </button>
        <button
          onClick={toggleSidebar}
          className="p-1.5 rounded-lg hover:bg-nx-surface transition-colors text-nx-muted"
        >
          <PanelLeftClose size={16} />
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
        {conversations.map((conv) => (
          <ConversationItem
            key={conv.id}
            conversation={conv}
            isActive={activeId === conv.id}
            onSelect={() => loadConversation(conv.id)}
            onDelete={() => handleDelete(conv.id)}
          />
        ))}
        {conversations.length === 0 && (
          <p className="text-xs text-nx-muted text-center py-4">No conversations yet</p>
        )}
      </div>
    </div>
  );
}
