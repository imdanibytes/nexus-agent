import { memo, useCallback, useState, type FC } from "react";
import {
  BrainIcon,
  BugIcon,
  CheckIcon,
  CopyIcon,
  DownloadIcon,
  MoreHorizontalIcon,
  RefreshCwIcon,
} from "lucide-react";
import {
  Alert,
  Button,
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
import { useThreadStore } from "../../stores/threadStore";
import { useThreadListStore } from "../../stores/threadListStore";
import type {
  ChatMessage,
  ToolCallPart,
  ThinkingPart,
  ProviderErrorDetails,
} from "../../stores/threadStore";
import { BranchPicker } from "./BranchPicker";
import { MarkdownText } from "../ui/MarkdownText";
import { ToolFallback } from "../ui/ToolFallback";
import { TooltipIconButton } from "../ui/TooltipIconButton";
import { TimingWaterfall } from "../ui/TimingWaterfall";
import { AskUserCard } from "./AskUserCard";

// ── Error Alert ──

const ERROR_TITLES: Record<string, string> = {
  rate_limit: "Rate limit exceeded",
  authentication: "Authentication failed",
  invalid_request: "Invalid request",
  overloaded: "Service overloaded",
  server_error: "Server error",
  context_length: "Context too long",
  network_error: "Connection error",
};

const ErrorAlert: FC<{
  error: unknown;
  providerError?: ProviderErrorDetails;
  onRetry: () => void;
}> = ({ error, providerError, onRetry }) => {
  const title = providerError
    ? ERROR_TITLES[providerError.kind] ?? "Error"
    : "Error";

  const description = providerError?.message
    ?? (typeof error === "string" ? error : "An error occurred.");

  const showRetry = providerError ? providerError.retryable : true;

  return (
    <Alert
      color="danger"
      variant="flat"
      title={title}
      description={description}
      endContent={
        showRetry ? (
          <Button
            size="sm"
            variant="flat"
            color="danger"
            onPress={onRetry}
            startContent={<RefreshCwIcon className="size-3" />}
            className="shrink-0"
          >
            Retry
          </Button>
        ) : undefined
      }
      className="mt-2"
    />
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
            <Button
              isIconOnly
              size="sm"
              variant="light"
              className="size-6 min-w-6 p-1 text-default-400"
              aria-label="More"
            >
              <MoreHorizontalIcon className="size-3" />
            </Button>
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

// ── Assistant Message ──

const AssistantMessageImpl: FC<{
  message: ChatMessage;
  isStreaming: boolean;
  regenId?: string;
  onRegenerate: (userMessageId: string) => void;
}> = ({ message, isStreaming, regenId, onRegenerate }) => {
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
  const onReload = useCallback(
    () => regenId && onRegenerate(regenId),
    [regenId, onRegenerate],
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

  return (
    <div
      className="group/assistant relative mx-auto w-full max-w-(--thread-max-width) animate-fade-in py-3"
      data-role="assistant"
    >
      <div className="wrap-break-word min-w-0 px-2 text-foreground leading-relaxed space-y-2.5">
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
            if (tc.toolName === "ask_user") {
              return (
                <AskUserCard
                  key={tc.toolCallId}
                  toolCall={tc}
                  conversationId={activeConvId ?? ""}
                />
              );
            }
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
          <ErrorAlert
            error={message.status?.error}
            providerError={message.status?.providerError}
            onRetry={onReload}
          />
        )}
      </div>

      {/* Footer */}
      <div className="mt-1 ml-2 flex h-7 items-center gap-2">
        {message.source?.type === "agent" && (
          <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-default-100 dark:bg-default-100/40 text-default-400 border border-default-200/50 truncate max-w-48">
            {message.source.agent_name}
          </span>
        )}
        {message.source?.type === "system" && (
          <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-warning-100 dark:bg-warning-100/30 text-warning-600 border border-warning-200/50 truncate max-w-48">
            system
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

export const AssistantMessage = memo(AssistantMessageImpl);
