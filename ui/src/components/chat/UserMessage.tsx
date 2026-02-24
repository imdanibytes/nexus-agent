import { useCallback, useEffect, useRef, useState, type FC } from "react";
import {
  CheckIcon,
  CopyIcon,
  PencilIcon,
  XIcon,
} from "lucide-react";
import type { ChatMessage } from "../../stores/threadStore";
import { BranchPicker } from "./BranchPicker";
import { TooltipIconButton } from "../ui/TooltipIconButton";

// ── User Message ──

export const UserMessage: FC<{
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
