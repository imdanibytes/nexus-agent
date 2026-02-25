import { useCallback, useEffect, type FC } from "react";
import { ArrowDownIcon } from "lucide-react";
import { useThreadStore, EMPTY_CONV } from "../../stores/threadStore";
import { useThreadListStore } from "../../stores/threadListStore";
import { useUsageStore } from "../../stores/usageStore";
import { useAutoScroll } from "../../hooks/useAutoScroll";
import { useChatStream } from "../../hooks/useChatStream";
import { useStreamBroadcasts } from "../../hooks/useStreamBroadcasts";
import { TooltipIconButton } from "../ui/TooltipIconButton";
import { ContextRing } from "../ui/ContextRing";
import { Composer } from "../ui/Composer";
import { AgentSwitcher } from "../agent/AgentSwitcher";
import { ThreadWelcome } from "./ThreadWelcome";
import { UserMessage } from "./UserMessage";
import { AssistantMessage } from "./AssistantMessage";

export const Thread: FC = () => {
  const activeThreadId = useThreadListStore((s) => s.activeThreadId);
  const messages = useThreadStore(
    (s) => s.conversations[activeThreadId ?? ""]?.messages ?? EMPTY_CONV.messages,
  );
  const isLoadingHistory = useThreadStore(
    (s) => s.conversations[activeThreadId ?? ""]?.isLoadingHistory ?? false,
  );
  const { sendMessage, branchMessage, regenerate, abort, isStreaming } =
    useChatStream();
  useStreamBroadcasts();
  const {
    containerRef,
    sentinelRef,
    isAtBottom,
    scrollToBottom,
    scrollToBottomIfNeeded,
  } = useAutoScroll();

  useEffect(() => {
    if (activeThreadId) {
      useThreadStore.getState().loadHistory(activeThreadId);
    }
  }, [activeThreadId]);

  useEffect(() => {
    scrollToBottomIfNeeded();
  }, [messages, scrollToBottomIfNeeded]);

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

          {messages.map((msg, idx) => {
            // Skip user messages that only carry tool results (API plumbing)
            if (
              msg.role === "user" &&
              msg.parts.length > 0 &&
              msg.parts.every((p) => p.type === "tool-result")
            ) {
              return null;
            }

            if (msg.role === "user") {
              return (
                <UserMessage
                  key={msg.id}
                  message={msg}
                  isStreaming={isStreaming}
                  onBranch={branchMessage}
                />
              );
            }

            // Find the preceding user message for regeneration
            let regenId: string | undefined;
            for (let j = idx - 1; j >= 0; j--) {
              const m = messages[j];
              if (m.role === "user" && m.parts.some((p) => p.type === "text")) {
                regenId = m.id;
                break;
              }
            }

            return (
              <AssistantMessage
                key={msg.id}
                message={msg}
                isStreaming={isStreaming}
                regenId={regenId}
                onRegenerate={regenerate}
              />
            );
          })}

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
          totalCost={usage.totalCost}
        />
      )}
    </div>
  );
};
