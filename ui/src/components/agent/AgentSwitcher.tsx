import { type FC } from "react";
import { ChevronDownIcon, SettingsIcon } from "lucide-react";
import {
  Dropdown,
  DropdownTrigger,
  DropdownMenu,
  DropdownItem,
  DropdownSection,
} from "@heroui/react";
import { useAgentStore } from "../../stores/agentStore";
import { useProviderStore } from "../../stores/providerStore";
import { useUIStore } from "../../stores/uiStore";

export const AgentSwitcher: FC = () => {
  const agents = useAgentStore((s) => s.agents);
  const activeAgentId = useAgentStore((s) => s.activeAgentId);
  const setActive = useAgentStore((s) => s.setActiveAgent);
  const providers = useProviderStore((s) => s.providers);
  const openSettings = useUIStore((s) => s.openSettings);

  const activeAgent = agents.find((a) => a.id === activeAgentId);

  if (agents.length === 0) return null;

  const providerName = (providerId: string) =>
    providers.find((p) => p.id === providerId)?.name ?? "Unknown";

  const selectedKeys = activeAgentId ? new Set([activeAgentId]) : new Set<string>();

  return (
    <Dropdown>
      <DropdownTrigger>
        <button className="flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] font-medium text-default-500 hover:text-foreground hover:bg-default-200/40 transition-colors">
          <span className="truncate max-w-[120px]">
            {activeAgent?.name ?? "No agent"}
          </span>
          <ChevronDownIcon className="size-3 shrink-0" />
        </button>
      </DropdownTrigger>
      <DropdownMenu
        aria-label="Select agent"
        selectionMode="single"
        selectedKeys={selectedKeys}
        onAction={(key) => {
          const k = String(key);
          if (k === "__settings") {
            openSettings("agents");
          } else {
            setActive(k);
          }
        }}
      >
        <DropdownSection showDivider>
          {agents.map((agent) => (
            <DropdownItem
              key={agent.id}
              description={`${providerName(agent.provider_id)} · ${agent.model}`}
            >
              {agent.name}
            </DropdownItem>
          ))}
        </DropdownSection>
        <DropdownSection>
          <DropdownItem
            key="__settings"
            startContent={<SettingsIcon className="size-3" />}
          >
            Manage agents
          </DropdownItem>
        </DropdownSection>
      </DropdownMenu>
    </Dropdown>
  );
};
