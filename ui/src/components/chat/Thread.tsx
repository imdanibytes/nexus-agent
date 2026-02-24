import { useCallback, useEffect, useRef, useState, type FC } from "react";
import {
  ArrowDownIcon,
  BrainIcon,
  BugIcon,
  CheckIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  CopyIcon,
  DownloadIcon,
  MessageSquareIcon,
  MoreHorizontalIcon,
  PencilIcon,
  RefreshCwIcon,
  SearchIcon,
  SparklesIcon,
  XIcon,
} from "lucide-react";
import {
  Dropdown,
  DropdownTrigger,
  DropdownMenu,
  DropdownItem,
  Modal,
  ModalContent,
  ModalHeader,
  ModalBody,
  Spinner,
} from "@heroui/react";
import { useThreadStore, EMPTY_CONV } from "../../stores/threadStore";
import { useThreadListStore } from "../../stores/threadListStore";
import { useUsageStore } from "../../stores/usageStore";
import type {
  ChatMessage,
  ToolCallPart,
  ThinkingPart,
} from "../../stores/threadStore";
import { getBranchInfo } from "../../lib/message-tree";
import { useAutoScroll } from "../../hooks/useAutoScroll";
import { useChatStream } from "../../hooks/useChatStream";
import { MarkdownText } from "../ui/MarkdownText";
import { ToolFallback } from "../ui/ToolFallback";
import { TooltipIconButton } from "../ui/TooltipIconButton";
import { ContextRing } from "../ui/ContextRing";
import { Composer } from "../ui/Composer";
import { AgentSwitcher } from "../agent/AgentSwitcher";
import { TimingWaterfall } from "../ui/TimingWaterfall";

// ── Thread ──

export const Thread: FC = () => {
  const activeThreadId = useThreadListStore((s) => s.activeThreadId);
  // Select only messages + loading flag — avoids re-renders from repo/childrenMap changes
  const messages = useThreadStore(
    (s) => s.conversations[activeThreadId ?? ""]?.messages ?? EMPTY_CONV.messages,
  );
  const isLoadingHistory = useThreadStore(
    (s) => s.conversations[activeThreadId ?? ""]?.isLoadingHistory ?? false,
  );
  const { sendMessage, branchMessage, regenerate, abort, isStreaming } =
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

  // If activeThreadId is set but the conv state hasn't been created yet
  // (loadHistory hasn't fired), don't flash the welcome screen.
  const convExists = useThreadStore(
    (s) => !!(activeThreadId && s.conversations[activeThreadId]),
  );
  const isEmpty =
    !activeThreadId ||
    (convExists && messages.length === 0 && !isStreaming && !isLoadingHistory);

  return (
    <div
      className="aui-thread-root flex h-full flex-col"
      style={{ ["--thread-max-width" as string]: "44rem" }}
    >
      <div className="relative flex-1 min-h-0">
        <div
          ref={containerRef}
          className="aui-thread-viewport flex h-full flex-col overflow-x-auto overflow-y-scroll scroll-smooth px-4 pt-4"
          style={{
            maskImage:
              "linear-gradient(to bottom, transparent 0%, black 20px, black calc(100% - 20px), transparent 100%)",
            WebkitMaskImage:
              "linear-gradient(to bottom, transparent 0%, black 20px, black calc(100% - 20px), transparent 100%)",
          }}
        >
          {isEmpty && <ThreadWelcome onSend={sendMessage} />}

          {messages.map((msg, idx) =>
            msg.role === "user" ? (
              <UserMessage
                key={msg.id}
                message={msg}
                isStreaming={isStreaming}
                onBranch={branchMessage}
              />
            ) : (
              <AssistantMessage
                key={msg.id}
                message={msg}
                isStreaming={isStreaming}
                onReload={() => {
                  // Find the actual human user message (has text parts),
                  // skipping tool_results messages which also have role "user"
                  for (let j = idx - 1; j >= 0; j--) {
                    const m = messages[j];
                    if (
                      m.role === "user" &&
                      m.parts.some((p) => p.type === "text")
                    ) {
                      regenerate(m.id);
                      return;
                    }
                  }
                }}
              />
            ),
          )}

          <div ref={sentinelRef} className="h-px shrink-0" />
        </div>
      </div>

      {/* Footer */}
      <div className="aui-thread-footer relative mx-auto flex w-full max-w-(--thread-max-width) shrink-0 flex-col gap-4 px-4 pt-3 pb-4 md:pb-6">
        {!isAtBottom && (
          <TooltipIconButton
            tooltip="Scroll to bottom"
            variant="outline"
            className="aui-thread-scroll-to-bottom absolute -top-12 z-10 self-center !size-9 !min-w-9 rounded-full bg-white dark:bg-default-50/60 backdrop-blur-xl border border-default-200 dark:border-default-200/50 shadow-sm dark:shadow-none hover:bg-default-100 dark:hover:bg-default-100/60"
            onPress={scrollToBottom}
          >
            <ArrowDownIcon className="size-4" />
          </TooltipIconButton>
        )}
        <Composer
          onSend={sendMessage}
          onCancel={abort}
          isStreaming={isStreaming}
          leftSlot={<ComposerLeftSlot />}
        />
      </div>
    </div>
  );
};

// ── Composer Left Slot ──

const ComposerLeftSlot: FC = () => {
  const activeThreadId = useThreadListStore((s) => s.activeThreadId);
  const usage = useUsageStore((s) =>
    activeThreadId ? s.usage[activeThreadId] : undefined,
  );

  return (
    <div className="flex items-center gap-1.5">
      <AgentSwitcher />
      {usage && usage.contextWindow > 0 && (
        <ContextRing
          contextTokens={usage.inputTokens + usage.outputTokens}
          contextWindow={usage.contextWindow}
        />
      )}
    </div>
  );
};

// ── Welcome ──

const SUGGESTIONS = [
  { icon: MessageSquareIcon, label: "Ask me anything", prompt: "" },
  { icon: SparklesIcon, label: "Build something", prompt: "Help me build " },
  { icon: SearchIcon, label: "Research a topic", prompt: "Research " },
];

const ThreadWelcome: FC<{ onSend: (text: string) => void }> = ({ onSend }) => {
  return (
    <div className="mx-auto my-auto flex w-full max-w-(--thread-max-width) grow flex-col">
      <div className="flex w-full grow flex-col items-center justify-center gap-6">
        <div className="flex flex-col items-center gap-2 animate-fade-in">
          <div className="flex size-12 items-center justify-center rounded-xl bg-default-100 dark:bg-default-50/40 backdrop-blur-xl border border-default-200 dark:border-default-200/50 shadow-sm dark:shadow-none">
            <SparklesIcon className="size-6 text-primary" />
          </div>
          <h1 className="text-xl font-semibold text-default-900">Nexus</h1>
          <p
            className="text-sm text-default-500 animate-fade-in"
            style={{ animationDelay: "75ms" }}
          >
            What would you like to work on?
          </p>
        </div>

        <div
          className="flex flex-wrap justify-center gap-3 animate-fade-in"
          style={{ animationDelay: "150ms" }}
        >
          {SUGGESTIONS.map((s) => {
            const Icon = s.icon;
            return (
              <button
                key={s.label}
                type="button"
                onClick={() => s.prompt && onSend(s.prompt)}
                className="group flex items-center gap-2 rounded-xl border border-default-200 dark:border-default-200/50 bg-white dark:bg-default-50/40 backdrop-blur-xl shadow-sm dark:shadow-none px-4 py-2.5 text-sm text-default-700 transition-all hover:bg-default-100 dark:hover:bg-default-100/50 hover:text-default-900 hover:border-default-300 dark:hover:border-default-300/50"
              >
                <Icon className="size-4 text-default-400 group-hover:text-primary transition-colors" />
                {s.label}
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
};

// ── Thinking Block ──

const ThinkingBlock: FC<{ thinking: string; isStreaming?: boolean }> = ({
  thinking,
  isStreaming,
}) => {
  const [expanded, setExpanded] = useState(isStreaming ?? false);

  return (
    <div className="border-l-2 border-default-300 dark:border-default-300/50 pl-3 my-2">
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="text-xs text-default-400 hover:text-default-600 flex items-center gap-1.5 transition-colors"
      >
        <BrainIcon className="size-3" />
        <span>{expanded ? "Hide" : "Show"} thinking</span>
        {isStreaming && <Spinner variant="dots" size="sm" />}
      </button>
      {expanded && (
        <div className="text-sm text-default-500 mt-2 whitespace-pre-wrap leading-relaxed">
          {thinking}
        </div>
      )}
    </div>
  );
};

// ── Thinking Indicator ──

const ThinkingIndicator: FC<{ label?: string | null }> = ({ label }) => (
  <div className="flex items-center gap-2 px-2 py-3">
    <Spinner variant="dots" color="primary" size="sm" />
    {label && (
      <span className="text-xs text-default-500 animate-pulse">{label}</span>
    )}
  </div>
);

const StreamingActivity: FC<{ label: string }> = ({ label }) => (
  <div className="flex items-center gap-2 px-2 pt-1">
    <Spinner variant="dots" color="primary" size="sm" />
    <span className="text-xs text-default-500 animate-pulse">{label}</span>
  </div>
);

// ── Helpers ──

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

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
    <div className="flex items-center gap-0.5 text-[11px] text-default-400">
      <button
        type="button"
        disabled={info.index === 0}
        onClick={() =>
          activeId && navigateBranch(activeId, messageId, "prev")
        }
        className="size-5 flex items-center justify-center rounded hover:bg-default-200/50 disabled:opacity-30 disabled:cursor-default transition-colors"
      >
        <ChevronLeftIcon className="size-3" />
      </button>
      <span className="tabular-nums">
        {info.index + 1}/{info.count}
      </span>
      <button
        type="button"
        disabled={info.index === info.count - 1}
        onClick={() =>
          activeId && navigateBranch(activeId, messageId, "next")
        }
        className="size-5 flex items-center justify-center rounded hover:bg-default-200/50 disabled:opacity-30 disabled:cursor-default transition-colors"
      >
        <ChevronRightIcon className="size-3" />
      </button>
    </div>
  );
};

// ── User Message ──

const UserMessage: FC<{
  message: ChatMessage;
  isStreaming: boolean;
  onBranch: (messageId: string, text: string) => void;
}> = ({ message, isStreaming, onBranch }) => {
  const [copied, setCopied] = useState(false);
  const [editing, setEditing] = useState(false);
  const [editText, setEditText] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const text = message.parts
    .filter((p) => p.type === "text")
    .map((p) => (p as { text: string }).text)
    .join("\n");

  const startEdit = useCallback(() => {
    setEditText(text);
    setEditing(true);
  }, [text]);

  const cancelEdit = useCallback(() => {
    setEditing(false);
  }, []);

  const submitEdit = useCallback(() => {
    const trimmed = editText.trim();
    if (!trimmed || trimmed === text) {
      setEditing(false);
      return;
    }
    setEditing(false);
    onBranch(message.id, trimmed);
  }, [editText, text, onBranch, message.id]);

  // Auto-resize textarea and focus
  useEffect(() => {
    if (editing && textareaRef.current) {
      const ta = textareaRef.current;
      ta.focus();
      ta.style.height = "auto";
      ta.style.height = `${ta.scrollHeight}px`;
    }
  }, [editing]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        submitEdit();
      } else if (e.key === "Escape") {
        cancelEdit();
      }
    },
    [submitEdit, cancelEdit],
  );

  const handleInput = useCallback(
    (e: React.ChangeEvent<HTMLTextAreaElement>) => {
      setEditText(e.target.value);
      const ta = e.target;
      ta.style.height = "auto";
      ta.style.height = `${ta.scrollHeight}px`;
    },
    [],
  );

  const copyText = useCallback(() => {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }, [text]);

  return (
    <div
      className="group/user mx-auto flex w-full max-w-(--thread-max-width) animate-fade-in justify-end px-2 py-3"
      data-role="user"
    >
      <div className="relative min-w-0 max-w-[85%] flex flex-col items-end">
        {editing ? (
          <div className="w-full rounded-2xl bg-white dark:bg-default-50/40 backdrop-blur-xl border border-primary/50 shadow-sm dark:shadow-none px-4 py-2.5">
            <textarea
              ref={textareaRef}
              value={editText}
              onChange={handleInput}
              onKeyDown={handleKeyDown}
              className="w-full min-w-0 resize-none bg-transparent text-foreground outline-none"
              rows={1}
            />
            <div className="mt-2 flex justify-end gap-2">
              <button
                type="button"
                onClick={cancelEdit}
                className="flex items-center gap-1 rounded-lg px-2.5 py-1 text-xs text-default-500 hover:bg-default-200/50 transition-colors"
              >
                <XIcon className="size-3" />
                Cancel
              </button>
              <button
                type="button"
                onClick={submitEdit}
                className="flex items-center gap-1 rounded-lg bg-primary px-2.5 py-1 text-xs text-primary-foreground hover:opacity-90 transition-opacity"
              >
                Submit
              </button>
            </div>
          </div>
        ) : (
          <div className="wrap-break-word inline-block rounded-2xl bg-default-100 dark:bg-default-50/40 backdrop-blur-xl border border-default-200 dark:border-default-200/50 shadow-sm dark:shadow-none px-4 py-2.5 text-foreground">
            {text}
          </div>
        )}

        <div className="mt-1 flex h-7 items-center justify-end gap-2">
          <BranchPicker messageId={message.id} />
          {!editing && (
            <div className="flex gap-0.5 opacity-0 transition-opacity group-hover/user:opacity-100">
              <TooltipIconButton
                tooltip="Edit"
                className="size-6 text-default-400"
                onPress={startEdit}
              >
                <PencilIcon className="size-3" />
              </TooltipIconButton>
              <TooltipIconButton
                tooltip="Copy"
                className="size-6 text-default-400"
                onPress={copyText}
              >
                {copied ? (
                  <CheckIcon className="size-3" />
                ) : (
                  <CopyIcon className="size-3" />
                )}
              </TooltipIconButton>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

// ── Assistant Message ──

const AssistantMessage: FC<{
  message: ChatMessage;
  isStreaming: boolean;
  onReload: () => void;
}> = ({ message, isStreaming, onReload }) => {
  const [copied, setCopied] = useState(false);
  const [timingOpen, setTimingOpen] = useState(false);
  const hasError =
    message.status?.type === "incomplete" &&
    message.status.reason !== "aborted";
  const isActiveStream = message.status?.type === "streaming";
  const visibleParts = message.parts.filter((p) =>
    p.type === "text" ? (p as { text: string }).text.length > 0 : true,
  );
  const activeConvId = useThreadListStore((s) => s.activeThreadId);
  const activity = useThreadStore(
    (s) => s.conversations[activeConvId ?? ""]?.activity ?? null,
  );

  const timingSpans = message.metadata?.timingSpans;

  // Build tool timing lookup from per-message spans
  const toolTimingMap = new Map<string, number>();
  if (timingSpans) {
    for (const s of timingSpans) {
      if (s.name.startsWith("tool:")) {
        const tcId = s.id.replace("t-tool-", "");
        toolTimingMap.set(tcId, s.durationMs);
      }
    }
  }

  const turnSpan = timingSpans?.find((t) => t.name === "turn");

  const copyText = useCallback(() => {
    const text = message.parts
      .filter((p) => p.type === "text")
      .map((p) => (p as { text: string }).text)
      .join("\n");
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }, [message.parts]);

  return (
    <div
      className="group/assistant relative mx-auto w-full max-w-(--thread-max-width) animate-fade-in py-3"
      data-role="assistant"
    >
      <div className="wrap-break-word px-2 text-foreground leading-relaxed space-y-2.5">
        {isActiveStream && visibleParts.length === 0 && (
          <ThinkingIndicator label={activity} />
        )}

        {message.parts.map((part, i) => {
          if (part.type === "thinking") {
            const tp = part as ThinkingPart;
            return (
              <ThinkingBlock
                key={i}
                thinking={tp.thinking}
                isStreaming={isActiveStream}
              />
            );
          }
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
            const argsText =
              tc.argsText ||
              (tc.args && Object.keys(tc.args).length > 0
                ? JSON.stringify(tc.args)
                : undefined);
            return (
              <ToolFallback
                key={tc.toolCallId}
                toolName={tc.toolName}
                argsText={argsText}
                result={tc.result}
                status={tc.status}
                durationMs={toolTimingMap.get(tc.toolCallId)}
              />
            );
          }
          return null;
        })}

        {isActiveStream && visibleParts.length > 0 && activity && (
          <StreamingActivity label={activity} />
        )}

        {hasError && (
          <div className="mt-2 rounded-md border border-danger/30 bg-danger/10 p-3 text-danger text-sm">
            {typeof message.status?.error === "string"
              ? message.status.error
              : "An error occurred."}
          </div>
        )}
      </div>

      {/* Footer */}
      <div className="mt-1 ml-2 flex h-7 items-center gap-2">
        {message.metadata?.agent && (
          <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-default-100 dark:bg-default-100/40 text-default-400 border border-default-200/50 truncate max-w-48">
            {message.metadata.agent.agent_name}
          </span>
        )}
        {!isActiveStream && (
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
  const turnSpan = timingSpans?.find((t) => t.name === "turn");

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
      <div className="flex gap-0.5 text-default-400">
        <TooltipIconButton tooltip="Copy" className="size-6 text-default-400" onPress={copyText}>
          {copied ? <CheckIcon className="size-3" /> : <CopyIcon className="size-3" />}
        </TooltipIconButton>

        <TooltipIconButton tooltip="Regenerate" className="size-6 text-default-400" onPress={onReload}>
          <RefreshCwIcon className="size-3" />
        </TooltipIconButton>

        <Dropdown
          classNames={{
            content:
              "min-w-36 bg-white/90 dark:bg-default-50/80 backdrop-blur-2xl border border-default-200/50 shadow-lg dark:shadow-none",
          }}
        >
          <DropdownTrigger>
            <TooltipIconButton tooltip="More" className="size-6 text-default-400">
              <MoreHorizontalIcon className="size-3" />
            </TooltipIconButton>
          </DropdownTrigger>
          <DropdownMenu aria-label="Message actions">
            <DropdownItem
              key="export"
              startContent={<DownloadIcon className="size-3.5" />}
              onPress={exportMarkdown}
              className="text-sm"
            >
              Export as Markdown
            </DropdownItem>
            {timingSpans ? (
              <DropdownItem
                key="debug"
                startContent={<BugIcon className="size-3.5" />}
                onPress={() => setDebugOpen(true)}
                className="text-sm"
              >
                Debug timing
              </DropdownItem>
            ) : (
              <DropdownItem key="no-debug" className="hidden">
                —
              </DropdownItem>
            )}
          </DropdownMenu>
        </Dropdown>
      </div>

      <Modal
        isOpen={debugOpen}
        onOpenChange={setDebugOpen}
        size="3xl"
        scrollBehavior="inside"
        classNames={{
          base: "bg-white/90 dark:bg-default-50/80 backdrop-blur-2xl border border-default-200/50 shadow-lg dark:shadow-none",
          header: "border-b border-default-200/50",
        }}
      >
        <ModalContent>
          <ModalHeader className="text-sm font-mono">
            Turn timing
            {turnSpan && (
              <span className="ml-2 text-default-400 font-normal">
                {formatDuration(turnSpan.durationMs)}
              </span>
            )}
          </ModalHeader>
          <ModalBody className="pb-6">
            {timingSpans && <TimingWaterfall spans={timingSpans} />}
          </ModalBody>
        </ModalContent>
      </Modal>
    </>
  );
};
