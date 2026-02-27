import { useState, type FC } from "react";
import { ChevronUpIcon, ChevronDownIcon, ArchiveIcon } from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import type { SealedSpan, ChatMessage } from "../../stores/threadStore";
import { MarkdownText } from "../ui/MarkdownText";

interface SpanDividerProps {
  spans: SealedSpan[];
  onLoadAll: () => void;
}

export const SpanDivider: FC<SpanDividerProps> = ({ spans, onLoadAll }) => {
  const [visible, setVisible] = useState(false);

  const totalMessages = spans.reduce((sum, s) => sum + s.messageIds.length, 0);
  const allLoaded = spans.every((s) => s.messages != null);

  const handleClick = () => {
    if (!allLoaded) onLoadAll();
    setVisible((v) => !v);
  };

  return (
    <div className="mx-auto w-full max-w-(--thread-max-width) px-2 py-2">
      {/* Load / hide toggle pill */}
      <button
        type="button"
        onClick={handleClick}
        className="mx-auto flex items-center gap-2 rounded-full border border-default-200/50 bg-default-50/50 dark:bg-default-50/20 backdrop-blur-sm px-4 py-1.5 text-xs text-default-400 hover:text-default-600 hover:bg-default-100/50 dark:hover:bg-default-100/20 transition-colors cursor-pointer"
      >
        {visible ? (
          <ChevronDownIcon size={12} />
        ) : (
          <ChevronUpIcon size={12} />
        )}
        <ArchiveIcon size={12} />
        <span>
          {visible ? "Hide" : "Load"} earlier messages
          <span className="ml-1 text-default-300">· {totalMessages}</span>
        </span>
      </button>

      {/* Expanded span messages */}
      <AnimatePresence>
        {visible && allLoaded && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="overflow-hidden"
          >
            <div className="mt-2 rounded-lg border border-default-200/30 bg-default-50/30 dark:bg-default-50/10">
              {spans.map((span, spanIdx) => (
                <div key={span.index}>
                  {/* Span summary header */}
                  {spans.length > 1 && (
                    <div className="px-3 py-1.5 text-[10px] font-medium uppercase tracking-wider text-default-300 border-b border-default-200/20">
                      Segment {span.index + 1} · {span.messageIds.length}{" "}
                      messages
                    </div>
                  )}

                  {/* Read-only messages */}
                  {span.messages?.map((msg) => (
                    <ReadOnlyMessage key={msg.id} message={msg} />
                  ))}

                  {/* Separator between spans */}
                  {spanIdx < spans.length - 1 && (
                    <div className="border-b border-default-200/30" />
                  )}
                </div>
              ))}
            </div>

            {/* Divider between sealed history and current span */}
            <div className="mt-2 flex items-center gap-2 text-[10px] text-default-300">
              <div className="flex-1 border-t border-default-200/30" />
              <span>current conversation</span>
              <div className="flex-1 border-t border-default-200/30" />
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
};

// ── Read-only message (muted, no interactions) ──

const ReadOnlyMessage: FC<{ message: ChatMessage }> = ({ message }) => {
  const text = message.parts
    .filter((p) => p.type === "text")
    .map((p) => (p as { text: string }).text)
    .join("\n");

  const toolCalls = message.parts.filter((p) => p.type === "tool-call");
  const toolResults = message.parts.filter((p) => p.type === "tool-result");
  const isUser = message.role === "user";

  const details: string[] = [];
  if (toolCalls.length > 0)
    details.push(
      `${toolCalls.length} tool call${toolCalls.length !== 1 ? "s" : ""}`,
    );
  if (toolResults.length > 0)
    details.push(
      `${toolResults.length} tool result${toolResults.length !== 1 ? "s" : ""}`,
    );

  return (
    <div className="px-3 py-2 border-b border-default-200/20 last:border-b-0 opacity-50 pointer-events-none select-none">
      <div className="flex items-start gap-2">
        <span className="text-[10px] font-medium uppercase tracking-wider text-default-400 shrink-0 pt-0.5">
          {isUser ? "You" : "Agent"}
        </span>
        <div className="min-w-0 flex-1 text-xs text-default-500">
          {text ? (
            <div className="line-clamp-3">
              <MarkdownText text={text} isStreaming={false} />
            </div>
          ) : details.length > 0 ? (
            <span className="text-[10px] text-default-300 italic">
              {details.join(", ")}
            </span>
          ) : (
            <span className="text-[10px] text-default-300 italic">
              (empty)
            </span>
          )}
          {text && details.length > 0 && (
            <div className="mt-0.5 text-[10px] text-default-300">
              {details.join(", ")}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
