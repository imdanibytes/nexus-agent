import { type FC, useState } from "react";
import {
  PlusIcon,
  TrashIcon,
  PencilIcon,
  FolderIcon,
  FolderOpenIcon,
  CheckIcon,
  XIcon,
} from "lucide-react";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { FolderPickerModal } from "./FolderPickerModal";
import { cn } from "../../lib/utils";

type EditorMode =
  | { type: "closed" }
  | { type: "create" }
  | { type: "edit"; id: string; name: string; path: string };

export const WorkspacesTab: FC = () => {
  const { workspaces, deleteWorkspace } = useWorkspaceStore();
  const [mode, setMode] = useState<EditorMode>({ type: "closed" });

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-sm font-medium text-foreground">Workspaces</h3>
          <p className="text-[11px] text-default-400 mt-0.5">
            Folders the agent can access. Workspace paths are automatically
            added to the filesystem sandbox.
          </p>
        </div>
        <button
          onClick={() => setMode({ type: "create" })}
          className="flex items-center gap-1 px-2 py-1 text-xs font-medium rounded-md bg-primary text-white hover:bg-primary/90 transition-colors"
        >
          <PlusIcon className="size-3" />
          Add
        </button>
      </div>

      {workspaces.length === 0 && mode.type === "closed" && (
        <p className="text-xs text-default-400">
          No workspaces configured. Add a workspace to give agents access to
          your project files.
        </p>
      )}

      {workspaces.map((ws) => (
        <div
          key={ws.id}
          className="flex items-center gap-3 rounded-lg border border-default-200/50 bg-default-50/30 p-3"
        >
          <FolderIcon className="size-4 text-default-400 shrink-0" />
          <div className="flex-1 min-w-0">
            <div className="text-sm font-medium text-foreground">{ws.name}</div>
            <div className="text-[11px] text-default-400 font-mono truncate">
              {ws.path}
            </div>
          </div>
          <button
            onClick={() =>
              setMode({
                type: "edit",
                id: ws.id,
                name: ws.name,
                path: ws.path,
              })
            }
            className="text-default-400 hover:text-foreground p-1 rounded hover:bg-default-200/40 transition-colors"
          >
            <PencilIcon className="size-3.5" />
          </button>
          <button
            onClick={async () => {
              if (confirm(`Remove workspace "${ws.name}"?`)) {
                await deleteWorkspace(ws.id);
              }
            }}
            className="text-default-400 hover:text-danger p-1 rounded hover:bg-danger/10 transition-colors"
          >
            <TrashIcon className="size-3.5" />
          </button>
        </div>
      ))}

      {mode.type !== "closed" && (
        <WorkspaceEditor
          editId={mode.type === "edit" ? mode.id : undefined}
          initialName={mode.type === "edit" ? mode.name : ""}
          initialPath={mode.type === "edit" ? mode.path : ""}
          onClose={() => setMode({ type: "closed" })}
        />
      )}
    </div>
  );
};

const WorkspaceEditor: FC<{
  editId?: string;
  initialName: string;
  initialPath: string;
  onClose: () => void;
}> = ({ editId, initialName, initialPath, onClose }) => {
  const { createWorkspace, updateWorkspace } = useWorkspaceStore();
  const isEdit = !!editId;

  const [name, setName] = useState(initialName);
  const [path, setPath] = useState(initialPath);
  const [saving, setSaving] = useState(false);
  const [browserOpen, setBrowserOpen] = useState(false);

  const canSave = name.trim() && path.trim();

  const applyPath = (selected: string) => {
    setPath(selected);
    if (!name.trim()) {
      const folderName = selected.split("/").filter(Boolean).pop();
      if (folderName) setName(folderName);
    }
  };

  const handleSave = async () => {
    if (!canSave) return;
    setSaving(true);
    try {
      if (isEdit) {
        await updateWorkspace(editId!, { name: name.trim(), path: path.trim() });
      } else {
        await createWorkspace({ name: name.trim(), path: path.trim() });
      }
      onClose();
    } finally {
      setSaving(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && canSave && !saving) handleSave();
    else if (e.key === "Escape") onClose();
  };

  return (
    <>
      <div className="rounded-lg border border-primary/30 bg-primary/5 p-4 space-y-3">
        <h4 className="text-xs font-semibold text-foreground">
          {isEdit ? "Edit Workspace" : "New Workspace"}
        </h4>

        <div>
          <label className="block text-[11px] text-default-500 mb-1">
            Folder
          </label>
          <button
            onClick={() => setBrowserOpen(true)}
            className={cn(
              "flex items-center gap-2 w-full px-3 py-2 rounded-lg border text-left transition-colors",
              path
                ? "border-default-200/50 bg-white dark:bg-default-100/30 hover:border-primary/40"
                : "border-dashed border-default-300 bg-default-50/50 hover:border-primary/50 hover:bg-primary/5",
            )}
          >
            {path ? (
              <FolderIcon className="size-4 text-primary shrink-0" />
            ) : (
              <FolderOpenIcon className="size-4 text-default-400 shrink-0" />
            )}
            {path ? (
              <span className="text-xs font-mono text-foreground truncate">
                {path}
              </span>
            ) : (
              <span className="text-xs text-default-400">
                Choose a folder...
              </span>
            )}
          </button>
        </div>

        <div>
          <label className="block text-[11px] text-default-500 mb-1">
            Name
          </label>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={
              path
                ? path.split("/").filter(Boolean).pop() || "Workspace"
                : "My Project"
            }
            className="input-field"
            autoFocus={!!path}
          />
        </div>

        <div className="flex items-center gap-2 pt-1">
          <button
            onClick={handleSave}
            disabled={!canSave || saving}
            className={cn(
              "flex items-center gap-1 px-3 py-1.5 text-xs font-medium rounded-md transition-colors disabled:opacity-50",
              canSave
                ? "bg-primary text-white hover:bg-primary/90"
                : "bg-default-200 text-default-500",
            )}
          >
            <CheckIcon className="size-3" />
            {saving ? "Saving..." : isEdit ? "Update" : "Add Workspace"}
          </button>
          <button
            onClick={onClose}
            className="flex items-center gap-1 px-3 py-1.5 text-xs text-default-500 hover:text-foreground transition-colors"
          >
            <XIcon className="size-3" />
            Cancel
          </button>
        </div>
      </div>

      <FolderPickerModal
        open={browserOpen}
        initialPath={path || undefined}
        onSelect={(selected) => {
          applyPath(selected);
          setBrowserOpen(false);
        }}
        onClose={() => setBrowserOpen(false)}
      />
    </>
  );
};
