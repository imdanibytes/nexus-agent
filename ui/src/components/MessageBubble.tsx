import type { Message } from "@/api/client.js";
import { MarkdownRenderer } from "./MarkdownRenderer.js";
import { ToolCallBlock } from "./ToolCallBlock.js";
import { Sparkles } from "lucide-react";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";

interface Props {
  message: Message;
}

export function MessageBubble({ message }: Props) {
  if (message.role === "user") {
    return (
      <div className="flex justify-end mb-6 animate-fade-in">
        <div className="max-w-[80%] bg-primary/8 border border-primary/12 rounded-2xl rounded-br-md px-4 py-2.5">
          <p className="text-sm whitespace-pre-wrap break-words leading-relaxed">
            {message.content}
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex gap-3 mb-6 animate-fade-in">
      <Avatar className="h-7 w-7 flex-shrink-0 mt-0.5">
        <AvatarFallback className="bg-card border border-border text-xs">
          {message.profileName ? (
            <span>{message.profileName.charAt(0)}</span>
          ) : (
            <Sparkles size={13} className="text-primary" />
          )}
        </AvatarFallback>
      </Avatar>

      <div className="min-w-0 flex-1">
        {message.profileName && (
          <div className="text-[11px] text-muted-foreground mb-1 font-medium">
            {message.profileName}
          </div>
        )}

        {message.toolCalls?.map((tc) => (
          <ToolCallBlock key={tc.id} toolCall={tc} />
        ))}

        {message.content && (
          <div className="text-sm">
            <MarkdownRenderer content={message.content} />
          </div>
        )}
      </div>
    </div>
  );
}
