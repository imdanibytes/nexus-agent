import type { FC } from "react";
import { BellIcon } from "lucide-react";
import type { ChatMessage } from "../../stores/threadStore";

/**
 * Inline notification for synthetic system messages (e.g. background process completion).
 * Renders as a compact, centered bar instead of a user/assistant bubble.
 */
export const SystemNotification: FC<{ message: ChatMessage }> = ({
  message,
}) => {
  const text = message.parts
    .filter((p) => p.type === "text")
    .map((p) => (p as { text: string }).text)
    .join("\n");

  // Parse the notification text to extract structured info
  const lines = text.split("\n").filter(Boolean);
  const processLine = lines.find((l) => l.startsWith("Process:"));
  const statusLine = lines.find((l) => l.startsWith("Status:"));
  const label = processLine
    ?.replace("Process:", "")
    .trim()
    .replace(/^"(.*)".*$/, "$1");
  const status = statusLine?.match(/Status:\s*(\w+)/)?.[1] ?? "completed";

  return (
    <div className="mx-auto flex w-full max-w-(--thread-max-width) justify-center px-2 py-1.5">
      <div className="flex items-center gap-2 rounded-lg border border-default-200/50 bg-default-50/60 backdrop-blur-sm px-3 py-1.5 text-xs text-default-500">
        <BellIcon size={12} className="shrink-0 text-default-400" />
        <span>
          Background process{label ? ` "${label}"` : ""}{" "}
          <span
            className={
              status === "completed"
                ? "text-success font-medium"
                : status === "failed"
                  ? "text-danger font-medium"
                  : "font-medium"
            }
          >
            {status}
          </span>
        </span>
      </div>
    </div>
  );
};
