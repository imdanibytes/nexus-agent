import { type FC, useState } from "react";
import {
  PlusIcon,
  TrashIcon,
  PencilIcon,
  LayersIcon,
  CheckIcon,
  XIcon,
} from "lucide-react";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { useProjectStore } from "../../stores/projectStore";
import { cn } from "../../lib/utils";

type EditorMode =
  | { type: "closed" }
  | { type: "create" }
  | {
      type: "edit";
      id: string;
      name: string;
      description: string;
      projectIds: string[];
    };

export const WorkspacesTab: FC = () => {
  const { workspaces, deleteWorkspace } = useWorkspaceStore();
  const { projects } = useProjectStore();
  const [mode, setMode] = useState<EditorMode>({ type: "closed" });

  const projectName = (id: string) =>
    projects.find((p) => p.id === id)?.name ?? id;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-sm font-medium text-foreground">Workspaces</h3>
          <p className="text-[11px] text-default-400 mt-0.5">
            Logical groupings of projects. Assign a workspace to a conversation
            to give the agent project context.
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
          No workspaces configured. Add a workspace to group your projects.
        </p>
      )}

      {workspaces.map((ws) => (
        <div
          key={ws.id}
          className="flex items-center gap-3 rounded-lg border border-default-200/50 bg-default-50/30 p-3"
        >
          <LayersIcon className="size-4 text-default-400 shrink-0" />
          <div className="flex-1 min-w-0">
            <span className="text-sm font-medium text-foreground">
              {ws.name}
            </span>
            {ws.description && (
              <div className="text-[11px] text-default-400 truncate">
                {ws.description}
              </div>
            )}
            {ws.project_ids.length > 0 && (
              <div className="text-[11px] text-default-400 mt-0.5">
                {ws.project_ids.map(projectName).join(", ")}
              </div>
            )}
          </div>
          <button
            onClick={() =>
              setMode({
                type: "edit",
                id: ws.id,
                name: ws.name,
                description: ws.description ?? "",
                projectIds: ws.project_ids,
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
          initialDescription={mode.type === "edit" ? mode.description : ""}
          initialProjectIds={mode.type === "edit" ? mode.projectIds : []}
          onClose={() => setMode({ type: "closed" })}
        />
      )}
    </div>
  );
};

const WorkspaceEditor: FC<{
  editId?: string;
  initialName: string;
  initialDescription: string;
  initialProjectIds: string[];
  onClose: () => void;
}> = ({ editId, initialName, initialDescription, initialProjectIds, onClose }) => {
  const { createWorkspace, updateWorkspace } = useWorkspaceStore();
  const { projects } = useProjectStore();
  const isEdit = !!editId;

  const [name, setName] = useState(initialName);
  const [description, setDescription] = useState(initialDescription);
  const [selectedIds, setSelectedIds] = useState<string[]>(initialProjectIds);
  const [saving, setSaving] = useState(false);

  const canSave = name.trim().length > 0;

  const toggleProject = (id: string) => {
    setSelectedIds((prev) =>
      prev.includes(id) ? prev.filter((p) => p !== id) : [...prev, id],
    );
  };

  const handleSave = async () => {
    if (!canSave) return;
    setSaving(true);
    try {
      if (isEdit) {
        await updateWorkspace(editId!, {
          name: name.trim(),
          description: description.trim() || undefined,
          project_ids: selectedIds,
        });
      } else {
        await createWorkspace({
          name: name.trim(),
          description: description.trim() || undefined,
          project_ids: selectedIds,
        });
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
    <div className="rounded-lg border border-primary/30 bg-primary/5 p-4 space-y-3">
      <h4 className="text-xs font-semibold text-foreground">
        {isEdit ? "Edit Workspace" : "New Workspace"}
      </h4>

      <div>
        <label className="block text-[11px] text-default-500 mb-1">Name</label>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="My Workspace"
          className="input-field"
          autoFocus
        />
      </div>

      <div>
        <label className="block text-[11px] text-default-500 mb-1">
          Description
        </label>
        <input
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Optional description"
          className="input-field"
        />
      </div>

      {projects.length > 0 && (
        <div>
          <label className="block text-[11px] text-default-500 mb-1">
            Projects
          </label>
          <div className="space-y-1">
            {projects.map((proj) => {
              const checked = selectedIds.includes(proj.id);
              return (
                <label
                  key={proj.id}
                  className="flex items-center gap-2 px-2 py-1.5 rounded-md hover:bg-default-200/30 cursor-pointer transition-colors"
                >
                  <input
                    type="checkbox"
                    checked={checked}
                    onChange={() => toggleProject(proj.id)}
                    className="rounded border-default-300 text-primary focus:ring-primary/30"
                  />
                  <span className="text-xs text-foreground">{proj.name}</span>
                  <span className="text-[10px] text-default-400 font-mono truncate">
                    {proj.path}
                  </span>
                </label>
              );
            })}
          </div>
        </div>
      )}

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
  );
};
