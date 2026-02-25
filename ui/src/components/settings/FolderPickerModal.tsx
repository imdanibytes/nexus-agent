import { type FC, useState, useEffect, useCallback } from "react";
import {
  FolderIcon,
  FolderOpenIcon,
  ChevronRightIcon,
  CornerLeftUpIcon,
  Loader2Icon,
  XIcon,
  HomeIcon,
} from "lucide-react";
import { LazyMotion, domAnimation, m, AnimatePresence } from "framer-motion";
import { browseDirectory, type BrowseEntry } from "../../api/client";
import { cn } from "../../lib/utils";

interface FolderPickerModalProps {
  open: boolean;
  initialPath?: string;
  onSelect: (path: string) => void;
  onClose: () => void;
}

export const FolderPickerModal: FC<FolderPickerModalProps> = ({
  open,
  initialPath,
  onSelect,
  onClose,
}) => {
  const [currentPath, setCurrentPath] = useState("");
  const [parentPath, setParentPath] = useState<string | null>(null);
  const [entries, setEntries] = useState<BrowseEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const browse = useCallback(async (path?: string) => {
    setLoading(true);
    setError(null);
    try {
      const result = await browseDirectory(path);
      setCurrentPath(result.path);
      setParentPath(result.parent);
      setEntries(result.entries);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to browse");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (open) browse(initialPath || undefined);
  }, [open, initialPath, browse]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  const segments = currentPath.split("/").filter(Boolean);

  return (
    <LazyMotion features={domAnimation}>
      <AnimatePresence>
        {open && (
          <m.div
            className="fixed inset-0 z-[60] flex items-center justify-center"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15 }}
          >
            <div
              className="absolute inset-0 bg-black/20 dark:bg-black/30"
              onClick={onClose}
            />

            <m.div
              className="relative z-10 flex flex-col w-[min(90vw,32rem)] h-[min(80vh,28rem)] rounded-xl bg-white dark:bg-default-50 border border-default-200/50 shadow-lg dark:shadow-none overflow-hidden"
              initial={{ opacity: 0, scale: 0.96, y: 8 }}
              animate={{ opacity: 1, scale: 1, y: 0 }}
              exit={{ opacity: 0, scale: 0.96, y: 8 }}
              transition={{ duration: 0.2, ease: "easeOut" }}
            >
              {/* Header */}
              <div className="flex items-center justify-between px-4 py-3 border-b border-default-200/50">
                <h3 className="text-sm font-semibold text-foreground">
                  Select Folder
                </h3>
                <button
                  onClick={onClose}
                  className="p-1 rounded hover:bg-default-200/40 transition-colors text-default-400 hover:text-foreground"
                >
                  <XIcon className="size-4" />
                </button>
              </div>

              {/* Breadcrumb */}
              <div className="flex items-center gap-0.5 px-3 py-2 border-b border-default-200/30 bg-default-50/50 dark:bg-default-100/20 overflow-x-auto scrollbar-none">
                <button
                  onClick={() => browse("/")}
                  className="shrink-0 p-1 rounded hover:bg-default-200/40 transition-colors text-default-400 hover:text-foreground"
                  title="Root"
                >
                  <HomeIcon className="size-3" />
                </button>
                {segments.map((seg, i) => {
                  const segPath = "/" + segments.slice(0, i + 1).join("/");
                  const isLast = i === segments.length - 1;
                  return (
                    <span key={segPath} className="flex items-center shrink-0">
                      <ChevronRightIcon className="size-3 text-default-300" />
                      <button
                        onClick={() => !isLast && browse(segPath)}
                        className={cn(
                          "text-[11px] px-1 py-0.5 rounded transition-colors truncate max-w-[120px]",
                          isLast
                            ? "text-foreground font-medium"
                            : "text-default-500 hover:text-foreground hover:bg-default-200/40",
                        )}
                      >
                        {seg}
                      </button>
                    </span>
                  );
                })}
              </div>

              {/* Listing */}
              <div className="flex-1 min-h-0 overflow-y-auto">
                {loading && (
                  <div className="flex items-center justify-center py-12 text-default-400">
                    <Loader2Icon className="size-5 animate-spin" />
                  </div>
                )}

                {error && (
                  <div className="px-4 py-8 text-center text-xs text-danger">
                    {error}
                  </div>
                )}

                {!loading && !error && (
                  <div className="py-1">
                    {parentPath && (
                      <button
                        onClick={() => browse(parentPath)}
                        className="flex items-center gap-2.5 w-full px-4 py-2 text-left hover:bg-default-100/60 dark:hover:bg-default-200/20 transition-colors"
                      >
                        <CornerLeftUpIcon className="size-3.5 text-default-400" />
                        <span className="text-xs text-default-500">..</span>
                      </button>
                    )}

                    {entries.length === 0 && (
                      <div className="px-4 py-8 text-center text-xs text-default-400">
                        No subdirectories
                      </div>
                    )}

                    {entries.map((entry) => (
                      <button
                        key={entry.path}
                        onClick={() => browse(entry.path)}
                        className="flex items-center gap-2.5 w-full px-4 py-2 text-left hover:bg-default-100/60 dark:hover:bg-default-200/20 transition-colors group"
                      >
                        <FolderIcon className="size-3.5 text-default-400 group-hover:hidden" />
                        <FolderOpenIcon className="size-3.5 text-primary hidden group-hover:block" />
                        <span className="text-xs text-foreground truncate">
                          {entry.name}
                        </span>
                      </button>
                    ))}
                  </div>
                )}
              </div>

              {/* Footer */}
              <div className="flex items-center justify-between px-4 py-3 border-t border-default-200/50 bg-default-50/50 dark:bg-default-100/20">
                <div className="text-[11px] text-default-400 font-mono truncate mr-3 flex-1 min-w-0">
                  {currentPath}
                </div>
                <div className="flex items-center gap-2 shrink-0">
                  <button
                    onClick={onClose}
                    className="px-3 py-1.5 text-xs text-default-500 hover:text-foreground transition-colors"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={() => onSelect(currentPath)}
                    disabled={!currentPath}
                    className="px-3 py-1.5 text-xs font-medium rounded-md bg-primary text-white hover:bg-primary/90 transition-colors disabled:opacity-50"
                  >
                    Select
                  </button>
                </div>
              </div>
            </m.div>
          </m.div>
        )}
      </AnimatePresence>
    </LazyMotion>
  );
};
