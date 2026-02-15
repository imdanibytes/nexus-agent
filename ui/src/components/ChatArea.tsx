import { useEffect, useRef, useState, useCallback } from "react";
import { useChatStore } from "@/stores/chatStore.js";
import { useStreamingChat } from "@/hooks/useStreamingChat.js";
import { MessageBubble } from "./MessageBubble.js";
import { ToolCallBlock } from "./ToolCallBlock.js";
import { StreamingCursor } from "./StreamingCursor.js";
import { ChatInput } from "./ChatInput.js";
import { ElicitForm } from "./surfaces/ElicitForm.js";
import { SurfaceRenderer } from "./surfaces/SurfaceRenderer.js";
import { MarkdownRenderer } from "./MarkdownRenderer.js";
import { Sparkles, ArrowLeft, ArrowDown } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";

interface Props {
  compact?: boolean;
}

export function ChatArea({ compact }: Props) {
  const {
    messages,
    isStreaming,
    streamingText,
    currentToolCalls,
    pendingUiSurface,
    setChatOpen,
  } = useChatStore();
  const { sendMessage, respondToSurface } = useStreamingChat();
  const scrollRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const [showScrollBtn, setShowScrollBtn] = useState(false);

  const scrollToBottom = useCallback(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, []);

  // Auto-scroll on new content
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    // Only auto-scroll if near bottom
    const isNearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 120;
    if (isNearBottom) {
      bottomRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [messages, streamingText, currentToolCalls, pendingUiSurface]);

  // Track scroll position for scroll-to-bottom button
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const onScroll = () => {
      const distFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
      setShowScrollBtn(distFromBottom > 200);
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => el.removeEventListener("scroll", onScroll);
  }, []);

  const hasContent = messages.length > 0 || isStreaming;

  return (
    <div className="flex-1 flex flex-col h-full min-w-0 relative">
      {/* Compact header */}
      {compact && (
        <div className="flex items-center gap-2 px-3 h-12 border-b border-border flex-shrink-0">
          <Button variant="ghost" size="icon" onClick={() => setChatOpen(false)} className="h-8 w-8">
            <ArrowLeft size={16} />
          </Button>
          <span className="text-sm font-medium">Chat</span>
        </div>
      )}

      {/* Messages */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto overflow-x-hidden">
        <div className="max-w-3xl mx-auto px-4 py-6">
          {!hasContent && <EmptyState />}

          {messages.map((msg) => (
            <MessageBubble key={msg.id} message={msg} />
          ))}

          {/* Streaming assistant turn */}
          {isStreaming && (
            <div className="flex gap-3 mb-6 animate-fade-in">
              <Avatar className="h-7 w-7 flex-shrink-0 mt-0.5">
                <AvatarFallback className="bg-card border border-border text-xs">
                  <Sparkles size={13} className="text-primary" />
                </AvatarFallback>
              </Avatar>

              <div className="min-w-0 flex-1">
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

                {pendingUiSurface && !pendingUiSurface.responded && (
                  <div className="mt-3 animate-slide-up">
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
          )}

          <div ref={bottomRef} />
        </div>
      </div>

      {/* Scroll to bottom */}
      {showScrollBtn && (
        <div className="absolute bottom-20 left-1/2 -translate-x-1/2 z-10">
          <Button
            variant="secondary"
            size="icon"
            onClick={scrollToBottom}
            className="h-8 w-8 rounded-full shadow-lg border border-border"
          >
            <ArrowDown size={14} />
          </Button>
        </div>
      )}

      {/* Input */}
      <ChatInput
        onSend={sendMessage}
        disabled={isStreaming || (pendingUiSurface !== null && !pendingUiSurface.responded)}
      />
    </div>
  );
}

function EmptyState() {
  return (
    <div className="flex items-center justify-center min-h-[60vh]">
      <div className="text-center animate-fade-in">
        <div className="w-12 h-12 rounded-2xl bg-primary/10 border border-primary/20 flex items-center justify-center mx-auto mb-4">
          <Sparkles size={22} className="text-primary" />
        </div>
        <h2 className="text-lg font-semibold mb-1">Nexus Agent</h2>
        <p className="text-sm text-muted-foreground max-w-xs mx-auto">
          Ask a question, run a tool, or start a conversation.
        </p>
      </div>
    </div>
  );
}
