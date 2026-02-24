import { useMemo, useState, type FC } from "react";
import {
  PlusIcon,
  SearchIcon,
  XIcon,
  MoreHorizontalIcon,
  TrashIcon,
} from "lucide-react";
import { AnimatePresence, motion } from "framer-motion";
import {
  Button,
  Input,
  Dropdown,
  DropdownTrigger,
  DropdownMenu,
  DropdownItem,
} from "@heroui/react";
import { cn } from "../../lib/utils";
import { useThreadListStore } from "../../stores/threadListStore";
import { useThreadStore } from "../../stores/threadStore";

interface ThreadDrawerProps {
  isOpen: boolean;
  onClose: () => void;
  navigate: (path: string) => void;
}

function groupByTime(
  threads: { id: string; title: string; updated_at: string }[],
): { label: string; items: typeof threads }[] {
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const todayMs = today.getTime();
  const yesterdayMs = todayMs - 86_400_000;
  const weekMs = todayMs - 7 * 86_400_000;

  const groups: Record<string, typeof threads> = {
    Today: [],
    Yesterday: [],
    "This Week": [],
    Earlier: [],
  };

  for (const t of threads) {
    const ts = new Date(t.updated_at).getTime();
    if (ts >= todayMs) groups.Today.push(t);
    else if (ts >= yesterdayMs) groups.Yesterday.push(t);
    else if (ts >= weekMs) groups["This Week"].push(t);
    else groups.Earlier.push(t);
  }

  return Object.entries(groups)
    .filter(([, items]) => items.length > 0)
    .map(([label, items]) => ({ label, items }));
}

export const ThreadDrawer: FC<ThreadDrawerProps> = ({ isOpen, onClose, navigate }) => {
  const threads = useThreadListStore((s) => s.threads);
  const activeThreadId = useThreadListStore((s) => s.activeThreadId);
  const { createThread, deleteThread } = useThreadListStore.getState();
  const [search, setSearch] = useState("");

  const filtered = useMemo(() => {
    if (!search.trim()) return threads;
    const q = search.toLowerCase();
    return threads.filter((t) =>
      (t.title || "New Chat").toLowerCase().includes(q),
    );
  }, [threads, search]);

  const groups = useMemo(() => groupByTime(filtered), [filtered]);

  const handleNew = async () => {
    const id = await createThread();
    navigate(`/c/${id}`);
    onClose();
  };

  const handleSwitch = (id: string) => {
    navigate(`/c/${id}`);
    onClose();
  };

  return (
    <AnimatePresence>
      {isOpen && (
        <>
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="absolute inset-0 z-40 bg-black/30 dark:bg-black/40 backdrop-blur-sm"
            onClick={onClose}
          />

          <motion.div
            initial={{ x: "-100%" }}
            animate={{ x: 0 }}
            exit={{ x: "-100%" }}
            transition={{ type: "spring", damping: 30, stiffness: 300 }}
            className="absolute inset-y-0 left-0 z-50 flex w-72 flex-col bg-white/95 dark:bg-default-50/80 backdrop-blur-3xl border-r border-default-200 dark:border-default-200/50 rounded-l-2xl"
          >
            {/* Header */}
            <div className="flex items-center justify-between px-3 pt-3 pb-2">
              <span className="text-sm font-semibold text-default-900">
                Threads
              </span>
              <Button
                variant="light"
                isIconOnly
                size="sm"
                onPress={onClose}
                className="size-7 min-w-7 text-default-500"
              >
                <XIcon className="size-4" />
              </Button>
            </div>

            {/* Search */}
            <div className="px-3 pb-2">
              <Input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search..."
                size="sm"
                variant="flat"
                startContent={
                  <SearchIcon className="size-3.5 text-default-400" />
                }
                classNames={{
                  inputWrapper:
                    "bg-default-100 dark:bg-default-100/40 border border-default-200 dark:border-default-200/50",
                  input: "text-xs",
                }}
              />
            </div>

            {/* Thread list */}
            <div className="flex-1 overflow-y-auto px-2 pb-2">
              {groups.map((group) => (
                <div key={group.label} className="mb-3">
                  <div className="px-2 pb-1 text-[10px] font-semibold uppercase tracking-wider text-default-400">
                    {group.label}
                  </div>
                  {group.items.map((thread) => {
                    const isActive = thread.id === activeThreadId;
                    return (
                      <div
                        key={thread.id}
                        className={cn(
                          "group flex h-9 items-center gap-2 rounded-lg transition-colors hover:bg-default-100/40",
                          isActive && "bg-default-100/50",
                        )}
                      >
                        <button
                          className="flex h-full min-w-0 flex-1 items-center gap-2 truncate px-3 text-start text-sm text-default-800"
                          onClick={() => handleSwitch(thread.id)}
                        >
                          <StreamingDot threadId={thread.id} />
                          {thread.title || "New Chat"}
                        </button>
                        <DrawerThreadMenu
                          onDelete={async () => {
                            const nextId = await deleteThread(thread.id);
                            navigate(nextId ? `/c/${nextId}` : "/");
                          }}
                        />
                      </div>
                    );
                  })}
                </div>
              ))}
              {groups.length === 0 && search && (
                <p className="px-3 py-6 text-center text-xs text-default-400">
                  No threads match "{search}"
                </p>
              )}
            </div>

            {/* New thread */}
            <div className="shrink-0 border-t border-default-200/50 p-3">
              <Button
                variant="bordered"
                className="w-full justify-start gap-2 h-9 rounded-lg text-sm text-default-700 border-default-200/40 hover:bg-default-100/30"
                onPress={handleNew}
              >
                <PlusIcon className="size-4" />
                New Thread
              </Button>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
};

const StreamingDot: FC<{ threadId: string }> = ({ threadId }) => {
  const isStreaming = useThreadStore(
    (s) => s.conversations[threadId]?.isStreaming ?? false,
  );
  if (!isStreaming) return null;
  return (
    <span className="relative flex size-2 shrink-0">
      <span className="absolute inline-flex size-full animate-ping rounded-full bg-primary/60" />
      <span className="relative inline-flex size-2 rounded-full bg-primary" />
    </span>
  );
};

const DrawerThreadMenu: FC<{ onDelete: () => void }> = ({ onDelete }) => (
  <Dropdown>
    <DropdownTrigger>
      <Button
        variant="light"
        isIconOnly
        size="sm"
        className="mr-2 size-7 min-w-7 p-0 opacity-0 transition-opacity group-hover:opacity-100 data-[open=true]:opacity-100"
      >
        <MoreHorizontalIcon className="size-4" />
        <span className="sr-only">More options</span>
      </Button>
    </DropdownTrigger>
    <DropdownMenu aria-label="Thread actions">
      <DropdownItem
        key="delete"
        className="text-danger"
        color="danger"
        startContent={<TrashIcon className="size-4" />}
        onPress={onDelete}
      >
        Delete
      </DropdownItem>
    </DropdownMenu>
  </Dropdown>
);
