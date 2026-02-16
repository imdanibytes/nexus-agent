import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FC,
  type KeyboardEvent,
} from "react";
import {
  ArrowDownIcon,
  BugIcon,
  CheckIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  CopyIcon,
  DownloadIcon,
  MoreHorizontalIcon,
  PencilIcon,
  RefreshCwIcon,
} from "lucide-react";
import { useThreadStore, EMPTY_CONV } from "@/stores/threadStore.js";
import { useThreadListStore } from "@/stores/threadListStore.js";
import type { ChatMessage, ToolCallPart } from "@/stores/threadStore.js";
import { getBranchInfo } from "@/lib/message-tree.js";
import { useAutoScroll } from "@/hooks/useAutoScroll.js";
import { useChatStream } from "@/hooks/useChatStream.js";
import { MarkdownText } from "@/components/chat/MarkdownText.js";
import { ToolFallback } from "@/components/chat/ToolFallback.js";
import { TooltipIconButton } from "@/components/chat/tooltip-icon-button.js";
import { Composer } from "@/components/chat/Composer.js";
import { cn } from "@/lib/utils.js";
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet.js";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu.js";
import { TimingWaterfall } from "@/components/TimingWaterfall.js";
import type { TimingSpan } from "@/stores/chatStore.js";

// ── Thread ──

export const Thread: FC = () => {
  const activeThreadId = useThreadListStore((s) => s.activeThreadId);
  const conv = useThreadStore(
    (s) => s.conversations[activeThreadId ?? ""] ?? EMPTY_CONV,
  );
  const { messages, isLoadingHistory } = conv;
  const { sendMessage, sendMessageFromEdit, regenerateResponse, abort, isStreaming } =
    useChatStream();
  const {
    containerRef,
    sentinelRef,
    isAtBottom,
    scrollToBottom,
    scrollToBottomIfNeeded,
  } = useAutoScroll();

  // Load history when thread changes
  useEffect(() => {
    if (activeThreadId) {
      useThreadStore.getState().loadHistory(activeThreadId);
    }
  }, [activeThreadId]);

  // Auto-scroll on content changes
  useEffect(() => {
    scrollToBottomIfNeeded();
  }, [messages, scrollToBottomIfNeeded]);

  const isEmpty = messages.length === 0 && !isStreaming && !isLoadingHistory;

  return (
    <div
      className="aui-thread-root flex h-full flex-col bg-background"
      style={{ ["--thread-max-width" as string]: "44rem" }}
    >
      <div
        ref={containerRef}
        className="aui-thread-viewport relative flex flex-1 flex-col overflow-x-auto overflow-y-scroll scroll-smooth px-4 pt-4"
      >
        {isEmpty && <ThreadWelcome />}

        {messages.map((msg, idx) =>
          msg.role === "user" ? (
            <UserMessage
              key={msg.id}
              message={msg}
              previousMessageId={idx > 0 ? messages[idx - 1].id : null}
              isStreaming={isStreaming}
              onEdit={(text, parentId) =>
                sendMessageFromEdit(text, parentId)
              }
            />
          ) : (
            <AssistantMessage
              key={msg.id}
              message={msg}
              isStreaming={isStreaming}
              onReload={() => {
                // Find the user message that precedes this assistant message
                // and regenerate a new response off it (creates a sibling branch)
                for (let j = idx - 1; j >= 0; j--) {
                  if (messages[j].role === "user") {
                    regenerateResponse(messages[j].id);
                    return;
                  }
                }
              }}
            />
          ),
        )}

        <div ref={sentinelRef} className="h-px shrink-0" />

        {/* Footer — sticky at bottom */}
        <div className="aui-thread-viewport-footer sticky bottom-0 mx-auto mt-auto flex w-full max-w-(--thread-max-width) flex-col gap-4 overflow-visible rounded-t-3xl bg-background pb-4 md:pb-6">
          {!isAtBottom && (
            <TooltipIconButton
              tooltip="Scroll to bottom"
              variant="outline"
              className="aui-thread-scroll-to-bottom absolute -top-12 z-10 self-center rounded-full p-4 dark:bg-background dark:hover:bg-accent"
              onClick={scrollToBottom}
            >
              <ArrowDownIcon />
            </TooltipIconButton>
          )}
          <Composer
            onSend={sendMessage}
            onCancel={abort}
            isStreaming={isStreaming}
          />
        </div>
      </div>
    </div>
  );
};

// ── Welcome ──

const ThreadWelcome: FC = () => {
  return (
    <div className="aui-thread-welcome-root mx-auto my-auto flex w-full max-w-(--thread-max-width) grow flex-col">
      <div className="aui-thread-welcome-center flex w-full grow flex-col items-center justify-center">
        <div className="aui-thread-welcome-message flex size-full flex-col justify-center px-4">
          <h1 className="aui-thread-welcome-message-inner fade-in slide-in-from-bottom-1 animate-in fill-mode-both font-semibold text-2xl duration-200">
            Hello there!
          </h1>
          <p className="aui-thread-welcome-message-inner fade-in slide-in-from-bottom-1 animate-in fill-mode-both text-muted-foreground text-xl delay-75 duration-200">
            How can I help you today?
          </p>
        </div>
      </div>
    </div>
  );
};

// ── Thinking Indicator ──

const ThinkingIndicator: FC = () => (
  <div className="flex items-center gap-1.5 px-2 py-1">
    {[0, 1, 2].map((i) => (
      <span
        key={i}
        className="inline-block size-2 rounded-full bg-muted-foreground"
        style={{
          animation: "dot-pulse 1.4s ease-in-out infinite",
          animationDelay: `${i * 0.2}s`,
        }}
      />
    ))}
  </div>
);

// ── Branch Picker ──

const BranchPicker: FC<{ messageId: string }> = ({ messageId }) => {
  const activeId = useThreadListStore((s) => s.activeThreadId);
  const conv = useThreadStore(
    (s) => s.conversations[activeId ?? ""] ?? EMPTY_CONV,
  );
  const navigateBranch = useThreadStore((s) => s.navigateBranch);

  const info = getBranchInfo(messageId, conv.repository, conv.childrenMap);
  if (!info || info.count <= 1) return null;

  return (
    <div className="flex items-center gap-0.5 text-xs text-muted-foreground">
      <button
        type="button"
        className="rounded p-0.5 hover:bg-accent disabled:opacity-30"
        disabled={info.index === 0}
        onClick={() => activeId && navigateBranch(activeId, messageId, "prev")}
        aria-label="Previous branch"
      >
        <ChevronLeftIcon className="size-3.5" />
      </button>
      <span className="tabular-nums">
        {info.index + 1}/{info.count}
      </span>
      <button
        type="button"
        className="rounded p-0.5 hover:bg-accent disabled:opacity-30"
        disabled={info.index === info.count - 1}
        onClick={() => activeId && navigateBranch(activeId, messageId, "next")}
        aria-label="Next branch"
      >
        <ChevronRightIcon className="size-3.5" />
      </button>
    </div>
  );
};

// ── User Message ──

const UserMessage: FC<{
  message: ChatMessage;
  previousMessageId: string | null;
  isStreaming: boolean;
  onEdit: (text: string, parentId: string | null) => void;
}> = ({ message, previousMessageId, isStreaming, onEdit }) => {
  const isMcp = !!message.metadata?.mcpSource;
  const [isEditing, setIsEditing] = useState(false);
  const [editText, setEditText] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const text = message.parts
    .filter((p) => p.type === "text")
    .map((p) => (p as { text: string }).text)
    .join("\n");

  const startEditing = useCallback(() => {
    setEditText(text);
    setIsEditing(true);
  }, [text]);

  const cancelEditing = useCallback(() => {
    setIsEditing(false);
    setEditText("");
  }, []);

  const submitEdit = useCallback(() => {
    const trimmed = editText.trim();
    if (!trimmed || trimmed === text) {
      cancelEditing();
      return;
    }
    setIsEditing(false);
    setEditText("");
    onEdit(trimmed, previousMessageId);
  }, [editText, text, cancelEditing, onEdit, previousMessageId]);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        submitEdit();
      }
      if (e.key === "Escape") {
        cancelEditing();
      }
    },
    [submitEdit, cancelEditing],
  );

  // Auto-resize textarea
  useEffect(() => {
    if (isEditing && textareaRef.current) {
      const el = textareaRef.current;
      el.style.height = "auto";
      el.style.height = `${el.scrollHeight}px`;
      el.focus();
    }
  }, [isEditing, editText]);

  return (
    <div
      className="aui-user-message-root fade-in slide-in-from-bottom-1 group/user mx-auto grid w-full max-w-(--thread-max-width) animate-in auto-rows-auto grid-cols-[minmax(72px,1fr)_auto] content-start gap-y-2 px-2 py-3 duration-150"
      data-role="user"
    >
      {isEditing ? (
        <div className="col-span-2 flex flex-col gap-2">
          <textarea
            ref={textareaRef}
            value={editText}
            onChange={(e) => setEditText(e.target.value)}
            onKeyDown={handleKeyDown}
            className="min-h-[2.5rem] w-full resize-none overflow-hidden rounded-2xl border border-input bg-muted px-4 py-2.5 text-foreground text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/20"
            rows={1}
          />
          <div className="flex justify-end gap-1.5">
            <button
              type="button"
              onClick={cancelEditing}
              className="rounded-lg px-3 py-1 text-muted-foreground text-xs hover:bg-accent"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={submitEdit}
              className="rounded-lg bg-primary px-3 py-1 text-primary-foreground text-xs hover:bg-primary/90"
            >
              Save
            </button>
          </div>
        </div>
      ) : (
        <div className="relative col-start-2 min-w-0">
          <div className="aui-user-message-content wrap-break-word rounded-2xl bg-muted px-4 py-2.5 text-foreground">
            {text}
          </div>
          {isMcp && (
            <span className="mt-1 inline-flex items-center gap-1 rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary">
              MCP
            </span>
          )}

          {/* Footer — always rendered for stable layout, right-aligned to match bubble */}
          <div className="mt-1 flex h-7 items-center justify-end gap-2">
            {!isMcp && !isStreaming && (
              <div className="opacity-0 transition-opacity group-hover/user:opacity-100">
                <TooltipIconButton
                  tooltip="Edit"
                  className="size-6 text-muted-foreground"
                  onClick={startEditing}
                >
                  <PencilIcon className="size-3" />
                </TooltipIconButton>
              </div>
            )}
            <BranchPicker messageId={message.id} />
          </div>
        </div>
      )}
    </div>
  );
};

// ── Assistant Message ──

const AssistantMessage: FC<{
  message: ChatMessage;
  isStreaming: boolean;
  onReload: () => void;
}> = ({ message, isStreaming, onReload }) => {
  const hasError = message.status?.type === "incomplete";
  const isActiveStream = message.status?.type === "streaming";
  const visibleParts = message.parts.filter(
    (p) => p.type === "text" ? (p as { text: string }).text.length > 0 : true,
  );

  return (
    <div
      className="aui-assistant-message-root fade-in slide-in-from-bottom-1 group/assistant relative mx-auto w-full max-w-(--thread-max-width) animate-in py-3 duration-150"
      data-role="assistant"
    >
      <div className="aui-assistant-message-content wrap-break-word px-2 text-foreground leading-relaxed">
        {isActiveStream && visibleParts.length === 0 && <ThinkingIndicator />}

        {message.parts.map((part, i) => {
          if (part.type === "text") {
            return (
              <MarkdownText
                key={i}
                text={part.text}
                isStreaming={isActiveStream}
              />
            );
          }
          if (part.type === "tool-call") {
            const tc = part as ToolCallPart;
            return (
              <ToolFallback
                key={tc.toolCallId}
                toolName={tc.toolName}
                argsText={tc.argsText}
                result={tc.result}
                status={tc.status}
              />
            );
          }
          return null;
        })}

        {hasError && (
          <div className="aui-message-error-root mt-2 rounded-md border border-destructive bg-destructive/10 p-3 text-destructive text-sm dark:bg-destructive/5 dark:text-red-200">
            {typeof message.status?.error === "string"
              ? message.status.error
              : "An error occurred."}
          </div>
        )}
      </div>

      {/* Footer — always rendered for stable layout, actions fade in on hover */}
      <div className="aui-assistant-message-footer mt-1 ml-2 flex h-7 items-center gap-2">
        {!isStreaming && (
          <>
            <BranchPicker messageId={message.id} />
            <div className="opacity-0 transition-opacity group-hover/assistant:opacity-100">
              <AssistantActionBar
                message={message}
                onReload={onReload}
              />
            </div>
          </>
        )}
      </div>
    </div>
  );
};

// ── Action Bar ──

const AssistantActionBar: FC<{
  message: ChatMessage;
  onReload: () => void;
}> = ({ message, onReload }) => {
  const [copied, setCopied] = useState(false);
  const [debugOpen, setDebugOpen] = useState(false);
  const timingSpans = message.metadata?.timingSpans;

  const copyText = () => {
    const text = message.parts
      .filter((p) => p.type === "text")
      .map((p) => (p as { text: string }).text)
      .join("\n");
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  const exportMarkdown = () => {
    const text = message.parts
      .filter((p) => p.type === "text")
      .map((p) => (p as { text: string }).text)
      .join("\n\n");
    const blob = new Blob([text], { type: "text/markdown" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "message.md";
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <>
      <div className="aui-assistant-action-bar-root -ml-1 flex gap-1 text-muted-foreground">
        <TooltipIconButton tooltip="Copy" onClick={copyText}>
          {copied ? <CheckIcon /> : <CopyIcon />}
        </TooltipIconButton>

        <TooltipIconButton tooltip="Refresh" onClick={onReload}>
          <RefreshCwIcon />
        </TooltipIconButton>

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <TooltipIconButton
              tooltip="More"
              className="data-[state=open]:bg-accent"
            >
              <MoreHorizontalIcon />
            </TooltipIconButton>
          </DropdownMenuTrigger>
          <DropdownMenuContent side="bottom" align="start" className="min-w-32">
            <DropdownMenuItem
              className="flex cursor-pointer items-center gap-2"
              onClick={exportMarkdown}
            >
              <DownloadIcon className="size-4" />
              Export as Markdown
            </DropdownMenuItem>
            {timingSpans && (
              <DropdownMenuItem
                className="flex cursor-pointer items-center gap-2"
                onClick={() => setDebugOpen(true)}
              >
                <BugIcon className="size-4" />
                Debug timing
              </DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      <Sheet open={debugOpen} onOpenChange={setDebugOpen}>
        <SheetContent
          side="right"
          className="sm:max-w-2xl w-[90vw] flex flex-col p-0"
        >
          <SheetHeader className="px-4 pt-4 pb-2 border-b border-border">
            <SheetTitle>Turn timing</SheetTitle>
          </SheetHeader>
          <div className="flex-1 overflow-auto px-4 py-3">
            {timingSpans && (
              <TimingWaterfall spans={timingSpans as TimingSpan[]} />
            )}
          </div>
        </SheetContent>
      </Sheet>
    </>
  );
};
