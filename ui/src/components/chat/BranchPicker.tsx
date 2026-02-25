import { memo, type FC } from "react";
import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react";
import { useThreadStore } from "../../stores/threadStore";
import { useThreadListStore } from "../../stores/threadListStore";
import { getBranchInfo } from "../../lib/message-tree";
import type { MessageNode } from "../../lib/message-tree";

const EMPTY_REPO: MessageNode[] = [];
const EMPTY_CHILDREN: Record<string, string[]> = {};

const BranchPickerImpl: FC<{ messageId: string }> = ({ messageId }) => {
  const activeId = useThreadListStore((s) => s.activeThreadId);
  const repository = useThreadStore(
    (s) => s.conversations[activeId ?? ""]?.repository ?? EMPTY_REPO,
  );
  const childrenMap = useThreadStore(
    (s) => s.conversations[activeId ?? ""]?.childrenMap ?? EMPTY_CHILDREN,
  );
  const navigateBranch = useThreadStore((s) => s.navigateBranch);

  const info = getBranchInfo(messageId, repository, childrenMap);
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

export const BranchPicker = memo(BranchPickerImpl);
