import type { FC } from "react";
import { MenuIcon, SettingsIcon } from "lucide-react";
import { useThreadListStore } from "../../stores/threadListStore";
import { useUIStore } from "../../stores/uiStore";

interface TopBarProps {
  onMenuPress: () => void;
}

export const TopBar: FC<TopBarProps> = ({ onMenuPress }) => {
  const activeThreadId = useThreadListStore((s) => s.activeThreadId);
  const threads = useThreadListStore((s) => s.threads);
  const setSettingsOpen = useUIStore((s) => s.setSettingsOpen);

  const activeThread = threads.find((t) => t.id === activeThreadId);
  const title = activeThread?.title;

  return (
    <div className="flex items-center gap-0 px-1 h-9 shrink-0 border-b border-default-200/50 bg-white/50 dark:bg-default-50/30 backdrop-blur-xl">
      <button
        onClick={onMenuPress}
        className="px-2 py-1 text-[13px] rounded hover:bg-default-200/40 transition-colors text-default-500 hover:text-default-900"
        aria-label="Toggle thread drawer"
      >
        <MenuIcon className="size-3.5" />
      </button>

      {title ? (
        <span className="flex-1 min-w-0 truncate px-2 text-[13px] font-semibold text-foreground">
          {title}
        </span>
      ) : (
        <span className="flex-1" />
      )}

      <button
        onClick={() => setSettingsOpen(true)}
        className="px-2 py-1 rounded hover:bg-default-200/40 transition-colors text-default-400 hover:text-default-900"
        aria-label="Settings"
      >
        <SettingsIcon className="size-3.5" />
      </button>
    </div>
  );
};
