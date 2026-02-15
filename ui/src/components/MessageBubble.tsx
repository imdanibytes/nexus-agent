import type { Message } from "../api/client.js";
import { MarkdownRenderer } from "./MarkdownRenderer.js";
import { ToolCallBlock } from "./ToolCallBlock.js";
import { User, Bot } from "lucide-react";

interface Props {
  message: Message;
}

export function MessageBubble({ message }: Props) {
  if (message.role === "user") {
    return (
      <div className="flex justify-end mb-4">
        <div className="flex items-start gap-2 max-w-[80%]">
          <div className="bg-nx-accent/10 border border-nx-accent/20 rounded-2xl rounded-tr-sm px-4 py-2.5">
            <p className="text-sm whitespace-pre-wrap">{message.content}</p>
          </div>
          <div className="flex-shrink-0 w-7 h-7 rounded-full bg-nx-accent/20 flex items-center justify-center mt-0.5">
            <User size={14} className="text-nx-accent" />
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex mb-4">
      <div className="flex items-start gap-2 max-w-[85%]">
        <div className="flex-shrink-0 w-7 h-7 rounded-full bg-nx-surface border border-nx-border flex items-center justify-center mt-0.5">
          <Bot size={14} className="text-nx-text" />
        </div>
        <div className="min-w-0">
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
    </div>
  );
}
