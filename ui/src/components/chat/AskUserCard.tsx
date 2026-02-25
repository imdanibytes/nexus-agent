import { memo, type FC, useCallback, useMemo, useState } from "react";
import { Button, Textarea, Chip } from "@heroui/react";
import {
  CheckIcon,
  XIcon,
  CircleDotIcon,
  MessageSquareIcon,
} from "lucide-react";
import { cn } from "../../lib/utils";
import { useQuestionStore, type PendingQuestion } from "../../stores/questionStore";
import { answerQuestion } from "../../api/client";
import { MarkdownText } from "../ui/MarkdownText";
import type { ToolCallPart } from "../../stores/threadStore";

interface AskUserCardProps {
  toolCall: ToolCallPart;
  conversationId: string;
}

const AskUserCardImpl: FC<AskUserCardProps> = ({
  toolCall,
  conversationId,
}) => {
  const pending = useQuestionStore((s) => s.questions[toolCall.toolCallId]);

  if (toolCall.result !== undefined) {
    return <AnsweredCard toolCall={toolCall} />;
  }

  if (!pending) {
    return <DismissedCard />;
  }

  return <PendingCard pending={pending} conversationId={conversationId} />;
};

export const AskUserCard = memo(AskUserCardImpl);

// ── Pending (interactive) ──

const TEXTAREA_CLASS_NAMES = {
  inputWrapper:
    "bg-default-50/40 dark:bg-default-100/20 border border-default-200 dark:border-default-200/50",
};

const PendingCardImpl: FC<{
  pending: PendingQuestion;
  conversationId: string;
}> = ({ pending, conversationId }) => {
  const [selected, setSelected] = useState<string | null>(null);
  const [multiSelected, setMultiSelected] = useState<Set<string>>(new Set());
  const [textValue, setTextValue] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const submit = useCallback(
    async (value: unknown) => {
      setSubmitting(true);
      try {
        await answerQuestion(conversationId, pending.questionId, value);
      } catch (err) {
        console.error("Failed to answer question:", err);
        setSubmitting(false);
      }
    },
    [conversationId, pending.questionId],
  );

  const placeholder = useMemo(
    () => pending.placeholder ?? "Type your answer...",
    [pending.placeholder],
  );

  return (
    <div className="my-3 rounded-xl border-l-3 border-primary bg-white/60 dark:bg-default-50/30 backdrop-blur-xl border border-default-200 dark:border-default-200/50 shadow-sm dark:shadow-none overflow-hidden animate-fade-in">
      {/* Context block */}
      {pending.context && (
        <div className="px-4 pt-3 pb-2 border-b border-default-200/50 text-sm text-default-600">
          <MarkdownText text={pending.context} />
        </div>
      )}

      <div className="px-4 py-3 space-y-3">
        {/* Question */}
        <div className="flex items-start gap-2">
          <MessageSquareIcon className="size-4 text-primary mt-0.5 shrink-0" />
          <div className="text-sm font-semibold text-default-900">
            <MarkdownText text={pending.question} />
          </div>
        </div>

        {/* Input area */}
        {pending.type === "confirm" && (
          <div className="flex items-center gap-2">
            <Button
              color="primary"
              size="sm"
              onPress={() => submit("yes")}
              isDisabled={submitting}
              className="gap-1"
            >
              <CheckIcon className="size-3.5" />
              Yes
            </Button>
            <Button
              variant="flat"
              size="sm"
              onPress={() => submit("no")}
              isDisabled={submitting}
            >
              <XIcon className="size-3.5" />
              No
            </Button>
          </div>
        )}

        {pending.type === "select" && pending.options && (
          <div className="space-y-1.5">
            {pending.options.map((opt) => (
              <button
                key={opt.value}
                type="button"
                disabled={submitting}
                onClick={() => {
                  setSelected(opt.value);
                  submit(opt.value);
                }}
                className={cn(
                  "w-full flex items-start gap-2.5 rounded-lg border px-3 py-2 text-left text-sm transition-all",
                  selected === opt.value
                    ? "border-primary bg-primary/5 dark:bg-primary/10"
                    : "border-default-200 dark:border-default-200/50 hover:border-default-300 hover:bg-default-50",
                )}
              >
                <CircleDotIcon
                  className={cn(
                    "size-4 mt-0.5 shrink-0",
                    selected === opt.value
                      ? "text-primary"
                      : "text-default-300",
                  )}
                />
                <div>
                  <span className="font-medium text-default-900">
                    {opt.label}
                  </span>
                  {opt.description && (
                    <p className="text-xs text-default-500 mt-0.5">
                      {opt.description}
                    </p>
                  )}
                </div>
              </button>
            ))}
          </div>
        )}

        {pending.type === "multi_select" && pending.options && (
          <div className="space-y-1.5">
            {pending.options.map((opt) => {
              const checked = multiSelected.has(opt.value);
              return (
                <button
                  key={opt.value}
                  type="button"
                  disabled={submitting}
                  onClick={() => {
                    setMultiSelected((prev) => {
                      const next = new Set(prev);
                      if (next.has(opt.value)) next.delete(opt.value);
                      else next.add(opt.value);
                      return next;
                    });
                  }}
                  className={cn(
                    "w-full flex items-start gap-2.5 rounded-lg border px-3 py-2 text-left text-sm transition-all",
                    checked
                      ? "border-primary bg-primary/5 dark:bg-primary/10"
                      : "border-default-200 dark:border-default-200/50 hover:border-default-300 hover:bg-default-50",
                  )}
                >
                  <div
                    className={cn(
                      "size-4 mt-0.5 shrink-0 rounded border flex items-center justify-center",
                      checked
                        ? "border-primary bg-primary"
                        : "border-default-300",
                    )}
                  >
                    {checked && (
                      <CheckIcon className="size-3 text-primary-foreground" />
                    )}
                  </div>
                  <div>
                    <span className="font-medium text-default-900">
                      {opt.label}
                    </span>
                    {opt.description && (
                      <p className="text-xs text-default-500 mt-0.5">
                        {opt.description}
                      </p>
                    )}
                  </div>
                </button>
              );
            })}
            <Button
              color="primary"
              size="sm"
              onPress={() => submit(Array.from(multiSelected))}
              isDisabled={submitting}
              className="mt-2"
            >
              Submit
            </Button>
          </div>
        )}

        {pending.type === "text" && (
          <div className="space-y-2">
            <Textarea
              value={textValue}
              onValueChange={setTextValue}
              placeholder={placeholder}
              minRows={2}
              maxRows={6}
              isDisabled={submitting}
              classNames={TEXTAREA_CLASS_NAMES}
            />
            <Button
              color="primary"
              size="sm"
              onPress={() => submit(textValue)}
              isDisabled={submitting || !textValue.trim()}
            >
              Submit
            </Button>
          </div>
        )}
      </div>
    </div>
  );
};

const PendingCard = memo(PendingCardImpl);

// ── Answered (collapsed) ──

/** Strip `<tool_response>` fencing if present (legacy persisted data) */
function unfence(raw: string): string {
  const trimmed = raw.trim();
  const start = trimmed.indexOf("<tool_response>");
  const end = trimmed.indexOf("</tool_response>");
  if (start !== -1 && end !== -1) {
    return trimmed.slice(start + "<tool_response>".length, end).trim();
  }
  return raw;
}

const AnsweredCard: FC<{ toolCall: ToolCallPart }> = ({ toolCall }) => {
  let answerText = "";
  try {
    const raw = unfence(toolCall.result as string);
    const parsed = JSON.parse(raw);
    const val = parsed?.answer;
    if (Array.isArray(val)) {
      answerText = val.join(", ");
    } else if (typeof val === "string") {
      answerText = val;
    } else {
      answerText = JSON.stringify(val);
    }
  } catch {
    answerText = String(toolCall.result ?? "");
  }

  const question =
    typeof toolCall.args?.question === "string"
      ? toolCall.args.question
      : "Question";
  const truncatedQ =
    question.length > 60 ? `${question.slice(0, 60)}...` : question;

  return (
    <div className="my-1.5 flex items-center gap-2 text-xs text-default-500">
      <span className="size-2 rounded-full bg-success shrink-0" />
      <span className="truncate">{truncatedQ}</span>
      <Chip size="sm" variant="flat" className="text-[10px] h-5 shrink-0">
        {answerText}
      </Chip>
    </div>
  );
};

// ── Dismissed ──

const DismissedCard: FC = () => (
  <div className="my-1.5 text-xs text-default-400 italic">
    Question cancelled
  </div>
);
