import { useState } from "react";
import { MoreHorizontal, Pencil, Trash2, Check, X } from "lucide-react";
import type { ConversationMeta } from "@/api/client.js";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";

interface Props {
  conversation: ConversationMeta;
  isActive: boolean;
  onSelect: () => void;
  onDelete: () => void;
  onRename: (title: string) => void;
}

function timeAgo(ts: number): string {
  const diff = Date.now() - ts;
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "now";
  if (mins < 60) return `${mins}m`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  return `${days}d`;
}

export function ConversationItem({ conversation, isActive, onSelect, onDelete, onRename }: Props) {
  const [renaming, setRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(conversation.title);

  const handleRenameSubmit = () => {
    const trimmed = renameValue.trim();
    if (trimmed && trimmed !== conversation.title) {
      onRename(trimmed);
    }
    setRenaming(false);
  };

  const handleRenameCancel = () => {
    setRenameValue(conversation.title);
    setRenaming(false);
  };

  if (renaming) {
    return (
      <div className="flex items-center gap-1 px-2 py-1">
        <Input
          value={renameValue}
          onChange={(e) => setRenameValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") handleRenameSubmit();
            if (e.key === "Escape") handleRenameCancel();
          }}
          className="h-7 text-xs"
          autoFocus
        />
        <Button variant="ghost" size="icon" onClick={handleRenameSubmit} className="h-6 w-6 flex-shrink-0">
          <Check size={12} />
        </Button>
        <Button variant="ghost" size="icon" onClick={handleRenameCancel} className="h-6 w-6 flex-shrink-0">
          <X size={12} />
        </Button>
      </div>
    );
  }

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onSelect}
      onKeyDown={(e) => e.key === "Enter" && onSelect()}
      className={cn(
        "group flex items-center gap-2 w-full text-left px-3 py-2 rounded-lg transition-colors cursor-pointer",
        isActive
          ? "bg-accent text-accent-foreground"
          : "hover:bg-accent/50 text-foreground"
      )}
    >
      <div className="flex-1 min-w-0">
        <div className="text-sm truncate leading-snug">{conversation.title}</div>
        <div className="text-[11px] text-muted-foreground mt-0.5">{timeAgo(conversation.updatedAt)}</div>
      </div>

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6 flex-shrink-0 text-muted-foreground/50 hover:text-foreground"
            onClick={(e) => e.stopPropagation()}
          >
            <MoreHorizontal size={13} />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-36">
          <DropdownMenuItem
            onClick={(e) => {
              e.stopPropagation();
              setRenameValue(conversation.title);
              setRenaming(true);
            }}
          >
            <Pencil size={13} />
            Rename
          </DropdownMenuItem>
          <DropdownMenuItem
            className="text-destructive focus:text-destructive"
            onClick={(e) => {
              e.stopPropagation();
              onDelete();
            }}
          >
            <Trash2 size={13} />
            Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
