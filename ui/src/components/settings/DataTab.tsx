import { useState, useEffect } from "react";
import { Check, Download, Trash2, Loader2 } from "lucide-react";
import {
  fetchConversations,
  deleteAllConversations,
  exportConversations,
} from "@/api/client.js";
import { useThreadListStore } from "@/stores/threadListStore.js";
import {
  Button,
  Separator,
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@imdanibytes/nexus-ui";

export function DataTab() {
  const [count, setCount] = useState<number | null>(null);
  const [exporting, setExporting] = useState(false);
  const [exportedPath, setExportedPath] = useState<string | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const loadThreads = useThreadListStore((s) => s.loadThreads);

  useEffect(() => {
    fetchConversations().then((c) => setCount(c.length));
  }, []);

  async function handleExport() {
    setExporting(true);
    setExportedPath(null);
    setExportError(null);
    try {
      const { path } = await exportConversations();
      setExportedPath(path);
    } catch (err) {
      setExportError(err instanceof Error ? err.message : "Export failed");
    } finally {
      setExporting(false);
    }
  }

  async function handleDeleteAll() {
    setDeleting(true);
    try {
      await deleteAllConversations();
      setCount(0);
      await loadThreads();
    } finally {
      setDeleting(false);
    }
  }

  const hasConversations = count !== null && count > 0;

  return (
    <div className="space-y-6">
      {/* Export */}
      <div className="space-y-3">
        <div className="flex items-center gap-2">
          <Download size={14} strokeWidth={1.5} className="text-muted-foreground" />
          <h3 className="text-sm font-medium">Export conversations</h3>
        </div>
        <p className="text-xs text-muted-foreground">
          Save all conversations as a JSON file to your Downloads folder.
        </p>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={handleExport}
            disabled={exporting || !hasConversations}
          >
            {exporting && <Loader2 size={14} className="animate-spin mr-1.5" />}
            Export{count !== null ? ` (${count})` : ""}
          </Button>
          {exportedPath && (
            <span className="flex items-center gap-1 text-xs text-emerald-500">
              <Check size={12} />
              Saved to {exportedPath}
            </span>
          )}
          {exportError && (
            <span className="text-xs text-destructive">{exportError}</span>
          )}
        </div>
      </div>

      <Separator />

      {/* Delete all */}
      <div className="space-y-3">
        <div className="flex items-center gap-2">
          <Trash2 size={14} strokeWidth={1.5} className="text-muted-foreground" />
          <h3 className="text-sm font-medium">Delete all conversations</h3>
        </div>
        <p className="text-xs text-muted-foreground">
          Permanently remove all conversations. This cannot be undone.
        </p>
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button
              variant="destructive"
              size="sm"
              disabled={!hasConversations || deleting}
            >
              {deleting && <Loader2 size={14} className="animate-spin mr-1.5" />}
              Delete all{count !== null ? ` (${count})` : ""}
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Delete all conversations?</AlertDialogTitle>
              <AlertDialogDescription>
                This will permanently delete {count} conversation{count !== 1 ? "s" : ""}.
                This action cannot be undone.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Cancel</AlertDialogCancel>
              <AlertDialogAction onClick={handleDeleteAll}>
                Delete all
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
    </div>
  );
}
