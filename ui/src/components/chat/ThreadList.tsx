import { type FC } from "react";
import { PlusIcon, MoreHorizontalIcon, TrashIcon } from "lucide-react";
import { useThreadListStore } from "@/stores/threadListStore.js";
import { useChatStore } from "@/stores/chatStore.js";
import { Button } from "@/components/ui/button.js";
import { Skeleton } from "@/components/ui/skeleton.js";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu.js";
import { cn } from "@/lib/utils.js";

export const ThreadList: FC = () => {
  const threads = useThreadListStore((s) => s.threads);
  const activeThreadId = useThreadListStore((s) => s.activeThreadId);
  const isLoading = useThreadListStore((s) => s.isLoading);
  const { createThread, switchThread, deleteThread } =
    useThreadListStore.getState();

  const handleNew = async () => {
    useChatStore.getState().setSettingsOpen(false);
    await createThread();
  };

  const handleSwitch = (id: string) => {
    useChatStore.getState().setSettingsOpen(false);
    switchThread(id);
  };

  return (
    <div className="aui-thread-list-root flex flex-col gap-1">
      <Button
        variant="outline"
        className="aui-thread-list-new h-9 justify-start gap-2 rounded-lg px-3 text-sm hover:bg-muted"
        onClick={handleNew}
      >
        <PlusIcon className="size-4" />
        New Thread
      </Button>

      {isLoading ? (
        <div className="flex flex-col gap-1">
          {Array.from({ length: 5 }, (_, i) => (
            <div
              key={i}
              className="flex h-9 items-center px-3"
              role="status"
              aria-label="Loading threads"
            >
              <Skeleton className="h-4 w-full" />
            </div>
          ))}
        </div>
      ) : (
        threads.map((thread) => {
          const isActive = thread.id === activeThreadId;
          return (
            <div
              key={thread.id}
              className={cn(
                "aui-thread-list-item group flex h-9 items-center gap-2 rounded-lg transition-colors hover:bg-muted focus-visible:bg-muted focus-visible:outline-none",
                isActive && "bg-muted",
              )}
            >
              <button
                className="flex h-full min-w-0 flex-1 items-center truncate px-3 text-start text-sm"
                onClick={() => handleSwitch(thread.id)}
              >
                {thread.title || "New Chat"}
              </button>
              <ThreadItemMenu
                threadId={thread.id}
                onDelete={() => deleteThread(thread.id)}
              />
            </div>
          );
        })
      )}
    </div>
  );
};

const ThreadItemMenu: FC<{
  threadId: string;
  onDelete: () => void;
}> = ({ onDelete }) => {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className="mr-2 size-7 p-0 opacity-0 transition-opacity group-hover:opacity-100 data-[state=open]:bg-accent data-[state=open]:opacity-100 group-[.bg-muted]:opacity-100"
        >
          <MoreHorizontalIcon className="size-4" />
          <span className="sr-only">More options</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent side="bottom" align="start" className="min-w-32">
        <DropdownMenuItem
          className="flex cursor-pointer items-center gap-2 text-destructive focus:text-destructive"
          onClick={onDelete}
        >
          <TrashIcon className="size-4" />
          Delete
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
};
