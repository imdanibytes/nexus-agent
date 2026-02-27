import { type FC } from "react";
import { ChevronDownIcon, SettingsIcon } from "lucide-react";
import {
  Dropdown,
  DropdownTrigger,
  DropdownMenu,
  DropdownItem,
  DropdownSection,
} from "@heroui/react";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { useThreadListStore } from "../../stores/threadListStore";
import { useUIStore } from "../../stores/uiStore";

export const WorkspaceSwitcher: FC = () => {
  const workspaces = useWorkspaceStore((s) => s.workspaces);
  const activeThreadId = useThreadListStore((s) => s.activeThreadId);
  const threads = useThreadListStore((s) => s.threads);
  const setThreadWorkspace = useThreadListStore((s) => s.setThreadWorkspace);
  const openSettings = useUIStore((s) => s.openSettings);

  const activeThread = threads.find((t) => t.id === activeThreadId);
  const currentWorkspaceId = activeThread?.workspace_id ?? null;
  const currentWorkspace = workspaces.find((w) => w.id === currentWorkspaceId);

  if (workspaces.length === 0) return null;

  const selectedKeys = new Set(currentWorkspaceId ? [currentWorkspaceId] : ["__none"]);

  return (
    <Dropdown>
      <DropdownTrigger>
        <button className="flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] font-medium text-default-500 hover:text-foreground hover:bg-default-200/40 transition-colors">
          <span className="truncate max-w-[120px]">
            {currentWorkspace?.name ?? "No workspace"}
          </span>
          <ChevronDownIcon className="size-3 shrink-0" />
        </button>
      </DropdownTrigger>
      <DropdownMenu
        aria-label="Select workspace"
        selectionMode="single"
        selectedKeys={selectedKeys}
        onAction={(key) => {
          const k = String(key);
          if (k === "__settings") {
            openSettings("workspaces");
          } else if (activeThreadId) {
            setThreadWorkspace(activeThreadId, k === "__none" ? null : k);
          }
        }}
      >
        <DropdownSection showDivider items={[{ id: "__none" }, ...workspaces]}>
          {(item) =>
            item.id === "__none" ? (
              <DropdownItem key="__none" className="text-default-400">
                None
              </DropdownItem>
            ) : (
              <DropdownItem
                key={item.id}
                description={"description" in item ? (item.description ?? undefined) : undefined}
              >
                {"name" in item ? item.name : ""}
              </DropdownItem>
            )
          }
        </DropdownSection>
        <DropdownSection>
          <DropdownItem
            key="__settings"
            startContent={<SettingsIcon className="size-3" />}
          >
            Manage workspaces
          </DropdownItem>
        </DropdownSection>
      </DropdownMenu>
    </Dropdown>
  );
};
