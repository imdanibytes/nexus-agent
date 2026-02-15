import { useEffect } from "react";
import { Plus, Settings, Sparkles } from "lucide-react";
import { useChatStore } from "@/stores/chatStore.js";
import { ConversationItem } from "./ConversationItem.js";
import {
  fetchConversations,
  deleteConversation as apiDelete,
  renameConversation as apiRename,
  fetchProfiles,
  getActiveProfile,
} from "@/api/client.js";
import { useStreamingChat } from "@/hooks/useStreamingChat.js";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { Tooltip, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";

interface Props {
  compact?: boolean;
}

export function Sidebar({ compact }: Props) {
  const {
    conversations,
    activeId,
    settingsOpen,
    setConversations,
    setActiveId,
    setMessages,
    setChatOpen,
    setSettingsOpen,
    setProfiles,
    setActiveProfileId,
  } = useChatStore();
  const { loadConversation } = useStreamingChat();

  useEffect(() => {
    fetchConversations().then(setConversations);
    fetchProfiles().then(setProfiles);
    getActiveProfile().then((r) => setActiveProfileId(r.profileId));
  }, [setConversations, setProfiles, setActiveProfileId]);

  const handleNewChat = () => {
    setActiveId(null);
    setMessages([]);
    setChatOpen(true);
    setSettingsOpen(false);
  };

  const handleSelect = (id: string) => {
    loadConversation(id);
    setChatOpen(true);
    setSettingsOpen(false);
  };

  const handleDelete = async (id: string) => {
    await apiDelete(id);
    const updated = await fetchConversations();
    setConversations(updated);
    if (activeId === id) {
      setActiveId(null);
      setMessages([]);
      setChatOpen(false);
    }
  };

  const handleRename = async (id: string, title: string) => {
    await apiRename(id, title);
    const updated = await fetchConversations();
    setConversations(updated);
  };

  const handleSettings = () => {
    setSettingsOpen(!settingsOpen);
    setChatOpen(false);
  };

  return (
    <div className="w-64 h-full bg-background border-r border-border flex flex-col flex-shrink-0">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3">
        <div className="flex items-center gap-2">
          <div className="w-6 h-6 rounded-md bg-primary/15 flex items-center justify-center">
            <Sparkles size={13} className="text-primary" />
          </div>
          <span className="text-sm font-semibold tracking-tight">Nexus Agent</span>
        </div>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button variant="ghost" size="icon" onClick={handleNewChat} className="h-7 w-7">
              <Plus size={15} />
            </Button>
          </TooltipTrigger>
          <TooltipContent side="bottom">New chat</TooltipContent>
        </Tooltip>
      </div>

      <Separator />

      {/* Conversation list */}
      <ScrollArea className="flex-1">
        <div className="p-2 space-y-0.5">
          {conversations.map((conv) => (
            <ConversationItem
              key={conv.id}
              conversation={conv}
              isActive={activeId === conv.id}
              onSelect={() => handleSelect(conv.id)}
              onDelete={() => handleDelete(conv.id)}
              onRename={(title) => handleRename(conv.id, title)}
            />
          ))}
          {conversations.length === 0 && (
            <div className="px-3 py-8 text-center">
              <p className="text-xs text-muted-foreground">No conversations yet</p>
              <p className="text-[11px] text-muted-foreground/60 mt-1">
                Start one with the + button above
              </p>
            </div>
          )}
        </div>
      </ScrollArea>

      <Separator />

      {/* Footer */}
      <div className="p-2">
        <Button
          variant={settingsOpen ? "secondary" : "ghost"}
          size="sm"
          onClick={handleSettings}
          className="w-full justify-start gap-2 h-9 text-muted-foreground hover:text-foreground"
        >
          <Settings size={14} />
          <span className="text-xs">Settings</span>
        </Button>
      </div>
    </div>
  );
}
