import {
  createContext,
  memo,
  useCallback,
  useContext,
  useRef,
  useState,
  type FC,
} from "react";
import { CheckIcon, ChevronDownIcon, CopyIcon } from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";
import { cn } from "../../lib/utils";
import { useScrollLock } from "../../hooks/useScrollLock";
import { formatToolDescription } from "../../lib/tool-descriptions";
import { MarkdownText } from "./MarkdownText";
import type { ToolCallStatus } from "../../stores/threadStore";

const ANIMATION_DURATION = 200;

// ── Context ──

const ToolFallbackContext = createContext<{
  isOpen: boolean;
  toggle: () => void;
}>({
  isOpen: false,
  toggle: () => {},
});

// ── Root ──

function ToolFallbackRoot({
  className,
  children,
  ...props
}: React.ComponentProps<"div">) {
  const collapsibleRef = useRef<HTMLDivElement>(null);
  const [isOpen, setIsOpen] = useState(false);
  const lockScroll = useScrollLock(collapsibleRef, ANIMATION_DURATION);

  const handleToggle = useCallback(() => {
    const next = !isOpen;
    if (!next) lockScroll();
    setIsOpen(next);
  }, [isOpen, lockScroll]);

  return (
    <ToolFallbackContext.Provider value={{ isOpen, toggle: handleToggle }}>
      <div
        ref={collapsibleRef}
        data-state={isOpen ? "open" : "closed"}
        className={cn(
          "aui-tool-fallback-root group/tool-fallback-root w-full cursor-pointer select-none nx-glass py-3",
          className,
        )}
        onClick={handleToggle}
        style={
          { "--animation-duration": `${ANIMATION_DURATION}ms` } as React.CSSProperties
        }
        {...props}
      >
        {children}
      </div>
    </ToolFallbackContext.Provider>
  );
}

// ── Trigger ──

type ToolStatusType = NonNullable<ToolCallStatus>["type"];

const Throbber = () => (
  <span className="relative flex size-4 shrink-0 items-center justify-center">
    <span className="absolute size-2 rounded-full bg-primary/80 animate-ping" />
    <span className="relative size-2 rounded-full bg-primary" />
  </span>
);

const StatusDot = ({ color }: { color: string }) => (
  <span className={cn("size-2 rounded-full shrink-0", color)} />
);

const statusIconMap: Record<ToolStatusType, React.ElementType> = {
  running: Throbber,
  complete: () => <StatusDot color="bg-success" />,
  incomplete: () => <StatusDot color="bg-danger" />,
};

/** Extract the model-provided description from tool args JSON. */
function extractDescription(argsText?: string): string | undefined {
  if (!argsText) return undefined;
  try {
    const parsed = JSON.parse(argsText);
    if (parsed && typeof parsed === "object" && typeof parsed.description === "string") {
      return parsed.description;
    }
  } catch {
    /* not JSON */
  }
  return undefined;
}

function ToolFallbackTrigger({
  toolName,
  argsText,
  status,
  durationMs,
}: {
  toolName: string;
  argsText?: string;
  status?: ToolCallStatus;
  durationMs?: number;
}) {
  const { isOpen } = useContext(ToolFallbackContext);
  const statusType = status?.type ?? "complete";
  const isRunning = statusType === "running";
  const isCancelled =
    status?.type === "incomplete" && status.reason === "cancelled";

  const Icon = statusIconMap[statusType];
  const modelDescription = extractDescription(argsText);
  const fallbackDescription = formatToolDescription(toolName, argsText);

  return (
    <button
      type="button"
      className="flex w-full cursor-pointer items-center gap-2 px-4 text-sm transition-colors"
    >
      <Icon className={cn("size-4 shrink-0", isCancelled && "text-default-400")} />
      <span
        className={cn(
          "grow text-left",
          isCancelled && "text-default-400 line-through",
          isRunning && "tool-pulse motion-reduce:animate-none",
        )}
      >
        {modelDescription ? (
          <>
            <span className="leading-none">{modelDescription}</span>
            <span className="ml-2 text-xs text-default-400">{fallbackDescription}</span>
          </>
        ) : (
          <span className="leading-none">{fallbackDescription}</span>
        )}
      </span>
      {durationMs != null && statusType !== "running" && (
        <span className="shrink-0 text-[10px] font-mono text-default-400">
          {durationMs < 1000
            ? `${durationMs}ms`
            : `${(durationMs / 1000).toFixed(1)}s`}
        </span>
      )}
      <ChevronDownIcon
        className={cn(
          "size-4 shrink-0 text-default-400 transition-transform duration-200 ease-out",
          isOpen ? "rotate-0" : "-rotate-90",
        )}
      />
    </button>
  );
}

// ── Content ──

function ToolFallbackContent({ children }: { children?: React.ReactNode }) {
  const { isOpen } = useContext(ToolFallbackContext);

  return (
    <AnimatePresence initial={false}>
      {isOpen && (
        <motion.div
          initial={{ height: 0, opacity: 0 }}
          animate={{ height: "auto", opacity: 1 }}
          exit={{ height: 0, opacity: 0 }}
          transition={{ duration: 0.2, ease: "easeOut" }}
          className="overflow-hidden text-sm outline-none"
        >
          <div className="mt-3 flex flex-col gap-2 border-t border-default-200/50 pt-2">
            {children}
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

// ── Args / Result ──

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation();
        navigator.clipboard.writeText(text).then(() => {
          setCopied(true);
          setTimeout(() => setCopied(false), 2000);
        });
      }}
      className="shrink-0 rounded-md p-1 text-default-400 transition-colors hover:bg-default-200/40 hover:text-default-600"
      aria-label="Copy"
    >
      {copied ? (
        <CheckIcon className="size-3 text-success" />
      ) : (
        <CopyIcon className="size-3" />
      )}
    </button>
  );
}

function parseArgs(argsText: string): Record<string, unknown> | null {
  try {
    const parsed = JSON.parse(argsText);
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed))
      return parsed;
  } catch {
    /* fall through */
  }
  return null;
}

function formatValue(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean")
    return String(value);
  return JSON.stringify(value, null, 2);
}

function ToolFallbackArgs({ argsText }: { argsText?: string }) {
  if (!argsText) return null;

  const parsed = parseArgs(argsText);

  if (parsed) {
    // Filter out the "description" field — it's already shown in the trigger header
    const entries = Object.entries(parsed).filter(([key]) => key !== "description");
    if (entries.length === 0) return null;
    return (
      <div className="px-4">
        <div className="mb-1 flex items-center justify-between">
          <p className="text-xs font-medium text-default-400">Request</p>
          <CopyButton text={argsText} />
        </div>
        <div className="flex flex-col gap-1">
          {entries.map(([key, value]) => (
            <div key={key} className="flex gap-2 text-xs">
              <span className="shrink-0 text-default-400 font-mono">
                {key}
              </span>
              <pre className="whitespace-pre-wrap break-all text-default-600 font-mono">
                {formatValue(value)}
              </pre>
            </div>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="px-4">
      <div className="mb-1 flex items-center justify-between">
        <p className="text-xs font-medium text-default-400">Request</p>
        <CopyButton text={argsText} />
      </div>
      <pre className="whitespace-pre-wrap text-xs text-default-500 font-mono">
        {argsText}
      </pre>
    </div>
  );
}

// Smart content detection for results
const MD_PATTERN = /(?:^#{1,6}\s|^\s*[-*]\s|\*\*|__|\[.+\]\(.+\)|^```)/m;

function detectContentType(text: string): "json" | "markdown" | "plain" {
  const trimmed = text.trim();
  if (
    (trimmed.startsWith("{") && trimmed.endsWith("}")) ||
    (trimmed.startsWith("[") && trimmed.endsWith("]"))
  ) {
    try {
      JSON.parse(trimmed);
      return "json";
    } catch {
      /* not json */
    }
  }
  if (MD_PATTERN.test(trimmed)) return "markdown";
  return "plain";
}

const MAX_LINES = 20;

function ResultContent({ text }: { text: string }) {
  const [expanded, setExpanded] = useState(false);
  const type = detectContentType(text);

  const displayText =
    type === "json"
      ? JSON.stringify(JSON.parse(text.trim()), null, 2)
      : text;

  const lines = displayText.split("\n");
  const isTruncated = lines.length > MAX_LINES;
  const shownText =
    isTruncated && !expanded
      ? lines.slice(0, MAX_LINES).join("\n")
      : displayText;

  const content =
    type === "markdown" ? (
      <div className="text-xs [&_.aui-md-p]:my-1 [&_.aui-md-p]:text-xs">
        <MarkdownText text={shownText} />
      </div>
    ) : (
      <pre className="whitespace-pre-wrap text-xs text-default-600 font-mono">
        {shownText}
      </pre>
    );

  return (
    <div>
      <div
        className="overflow-hidden"
        style={
          isTruncated && !expanded
            ? {
                maskImage:
                  "linear-gradient(to bottom, black 70%, transparent 100%)",
                WebkitMaskImage:
                  "linear-gradient(to bottom, black 70%, transparent 100%)",
              }
            : undefined
        }
      >
        {content}
      </div>
      {isTruncated && (
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            setExpanded(!expanded);
          }}
          className="mt-1.5 text-xs text-primary hover:text-primary/80 font-medium"
        >
          {expanded ? "Show less" : "Show more"}
        </button>
      )}
    </div>
  );
}

function ToolFallbackResult({ result }: { result?: unknown }) {
  if (result === undefined) return null;
  const text =
    typeof result === "string" ? result : JSON.stringify(result, null, 2);
  return (
    <div className="border-t border-dashed border-default-200/50 px-4 pt-2">
      <div className="mb-1 flex items-center justify-between">
        <p className="text-xs font-medium text-default-400">Result</p>
        <CopyButton text={text} />
      </div>
      <ResultContent text={text} />
    </div>
  );
}

// ── Composed ToolFallback ──

interface ToolFallbackProps {
  toolName: string;
  argsText?: string;
  result?: unknown;
  status?: ToolCallStatus;
  durationMs?: number;
}

const ToolFallbackImpl: FC<ToolFallbackProps> = ({
  toolName,
  argsText,
  result,
  status,
  durationMs,
}) => {
  const isCancelled =
    status?.type === "incomplete" && status.reason === "cancelled";

  return (
    <ToolFallbackRoot
      className={cn(
        isCancelled && "border-default-200/20 bg-default-50/20",
      )}
    >
      <ToolFallbackTrigger
        toolName={toolName}
        argsText={argsText}
        status={status}
        durationMs={durationMs}
      />
      <ToolFallbackContent>
        <ToolFallbackArgs
          argsText={argsText}
        />
        {!isCancelled && <ToolFallbackResult result={result} />}
      </ToolFallbackContent>
    </ToolFallbackRoot>
  );
};

export const ToolFallback = memo(ToolFallbackImpl);
