import { Trash2 } from "lucide-react";
import type { ConversationMeta } from "../api/client.js";

interface Props {
  conversation: ConversationMeta;
  isActive: boolean;
  onSelect: () => void;
  onDelete: () => void;
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

export function ConversationItem({ conversation, isActive, onSelect, onDelete }: Props) {
  return (
    <button
      onClick={onSelect}
      className={`group w-full text-left px-3 py-2.5 rounded-lg transition-colors flex items-center gap-2 ${
        isActive
          ? "bg-nx-surface border border-nx-border"
          : "hover:bg-nx-surface/50"
      }`}
    >
      <div className="flex-1 min-w-0">
        <div className="text-sm truncate">{conversation.title}</div>
        <div className="text-xs text-nx-muted">{timeAgo(conversation.updatedAt)}</div>
      </div>
      <button
        onClick={(e) => {
          e.stopPropagation();
          onDelete();
        }}
        className="flex-shrink-0 p-1 rounded text-nx-muted hover:text-red-400 hover:bg-red-400/10 transition-colors"
      >
        <Trash2 size={14} />
      </button>
    </button>
  );
}
