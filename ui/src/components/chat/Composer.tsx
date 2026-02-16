import {
  useCallback,
  useRef,
  useState,
  type FC,
  type KeyboardEvent,
} from "react";
import { ArrowUpIcon, SquareIcon } from "lucide-react";
import { AgentSwitcher } from "@/components/AgentSwitcher.js";
import { TooltipIconButton } from "@/components/chat/tooltip-icon-button.js";
import { Button } from "@/components/ui/button.js";

interface ComposerProps {
  onSend: (text: string) => void;
  onCancel: () => void;
  isStreaming: boolean;
}

export const Composer: FC<ComposerProps> = ({
  onSend,
  onCancel,
  isStreaming,
}) => {
  const [text, setText] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleSend = useCallback(() => {
    const trimmed = text.trim();
    if (!trimmed || isStreaming) return;
    onSend(trimmed);
    setText("");
    // Reset textarea height
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }
  }, [text, isStreaming, onSend]);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend],
  );

  const handleInput = useCallback(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, []);

  return (
    <div className="aui-composer-root relative flex w-full flex-col rounded-2xl border border-input bg-background px-1 pt-2 outline-none transition-shadow has-[textarea:focus-visible]:border-ring has-[textarea:focus-visible]:ring-2 has-[textarea:focus-visible]:ring-ring/20">
      <textarea
        ref={textareaRef}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={handleKeyDown}
        onInput={handleInput}
        placeholder="Send a message..."
        className="aui-composer-input mb-1 max-h-32 min-h-14 w-full resize-none bg-transparent px-4 pt-2 pb-3 text-sm outline-none placeholder:text-muted-foreground focus-visible:ring-0"
        rows={1}
        autoFocus
        aria-label="Message input"
      />
      <div className="aui-composer-action-wrapper relative mx-2 mb-2 flex items-center justify-between">
        <div className="flex items-center gap-1">
          <AgentSwitcher />
        </div>
        {!isStreaming ? (
          <TooltipIconButton
            tooltip="Send message"
            side="bottom"
            type="submit"
            variant="default"
            size="icon"
            className="aui-composer-send size-8 rounded-full"
            aria-label="Send message"
            onClick={handleSend}
            disabled={!text.trim()}
          >
            <ArrowUpIcon className="aui-composer-send-icon size-4" />
          </TooltipIconButton>
        ) : (
          <Button
            type="button"
            variant="default"
            size="icon"
            className="aui-composer-cancel size-8 rounded-full"
            aria-label="Stop generating"
            onClick={onCancel}
          >
            <SquareIcon className="aui-composer-cancel-icon size-3 fill-current" />
          </Button>
        )}
      </div>
    </div>
  );
};
