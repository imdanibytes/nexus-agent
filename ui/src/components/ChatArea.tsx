import { useEffect, useRef } from "react";
import { useChatStore } from "../stores/chatStore.js";
import { useStreamingChat } from "../hooks/useStreamingChat.js";
import { MessageBubble } from "./MessageBubble.js";
import { ToolCallBlock } from "./ToolCallBlock.js";
import { StreamingCursor } from "./StreamingCursor.js";
import { ChatInput } from "./ChatInput.js";
import { ElicitForm } from "./surfaces/ElicitForm.js";
import { SurfaceRenderer } from "./surfaces/SurfaceRenderer.js";
import { MarkdownRenderer } from "./MarkdownRenderer.js";
import { Bot, MessageSquare } from "lucide-react";

export function ChatArea() {
  const {
    messages,
    isStreaming,
    streamingText,
    currentToolCalls,
    pendingUiSurface,
  } = useChatStore();
  const { sendMessage, respondToSurface } = useStreamingChat();
  const scrollRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages, streamingText, currentToolCalls, pendingUiSurface]);

  return (
    <div className="flex-1 flex flex-col h-full">
      <div ref={scrollRef} className="flex-1 overflow-y-auto px-4 py-6">
        <div className="max-w-3xl mx-auto">
          {messages.length === 0 && !isStreaming && (
            <div className="flex items-center justify-center h-full min-h-[50vh]">
              <div className="text-center">
                <MessageSquare size={48} className="mx-auto mb-4 text-nx-muted" />
                <h2 className="text-lg font-medium text-nx-text mb-1">Nexus Agent</h2>
                <p className="text-sm text-nx-muted">Send a message to start a conversation.</p>
              </div>
            </div>
          )}
          {messages.map((msg) => (
            <MessageBubble key={msg.id} message={msg} />
          ))}

          {/* Streaming state */}
          {isStreaming && (
            <div className="flex mb-4">
              <div className="flex items-start gap-2 max-w-[85%]">
                <div className="flex-shrink-0 w-7 h-7 rounded-full bg-nx-surface border border-nx-border flex items-center justify-center mt-0.5">
                  <Bot size={14} className="text-nx-text" />
                </div>
                <div className="min-w-0">
                  {currentToolCalls.map((tc) => (
                    <ToolCallBlock key={tc.id} toolCall={tc} />
                  ))}
                  {streamingText && (
                    <div className="text-sm">
                      <MarkdownRenderer content={streamingText} />
                    </div>
                  )}
                  {!streamingText && currentToolCalls.length === 0 && (
                    <StreamingCursor />
                  )}

                  {/* UI Surface rendering */}
                  {pendingUiSurface && !pendingUiSurface.responded && (
                    <div className="mt-3">
                      {pendingUiSurface.name === "_nexus_elicit" ? (
                        <ElicitForm
                          toolUseId={pendingUiSurface.toolUseId}
                          input={pendingUiSurface.input}
                          onRespond={respondToSurface}
                        />
                      ) : pendingUiSurface.name === "_nexus_surface" ? (
                        <SurfaceRenderer
                          toolUseId={pendingUiSurface.toolUseId}
                          input={pendingUiSurface.input}
                          onRespond={respondToSurface}
                        />
                      ) : null}
                    </div>
                  )}
                </div>
              </div>
            </div>
          )}
        </div>
      </div>

      <ChatInput
        onSend={sendMessage}
        disabled={isStreaming || (pendingUiSurface !== null && !pendingUiSurface.responded)}
      />
    </div>
  );
}
