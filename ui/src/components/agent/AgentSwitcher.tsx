import { type FC, useState } from "react";
import { ChevronDownIcon, CheckIcon, SettingsIcon } from "lucide-react";
import { Popover, PopoverTrigger, PopoverContent } from "@heroui/react";
import { useAgentStore } from "../../stores/agentStore";
import { useProviderStore } from "../../stores/providerStore";
import { useUIStore } from "../../stores/uiStore";
import { cn } from "../../lib/utils";

export const AgentSwitcher: FC = () => {
  const [isOpen, setIsOpen] = useState(false);
  const agents = useAgentStore((s) => s.agents);
  const activeAgentId = useAgentStore((s) => s.activeAgentId);
  const setActive = useAgentStore((s) => s.setActiveAgent);
  const providers = useProviderStore((s) => s.providers);
  const openSettings = useUIStore((s) => s.openSettings);

  const activeAgent = agents.find((a) => a.id === activeAgentId);

  if (agents.length === 0) return null;

  const providerName = (providerId: string) =>
    providers.find((p) => p.id === providerId)?.name ?? "Unknown";

  return (
    <Popover placement="bottom-end" offset={4} isOpen={isOpen} onOpenChange={setIsOpen}>
      <PopoverTrigger>
        <button className="flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] font-medium text-default-500 hover:text-foreground hover:bg-default-200/40 transition-colors">
          <span className="truncate max-w-[120px]">
            {activeAgent?.name ?? "No agent"}
          </span>
          <ChevronDownIcon className="size-3 shrink-0" />
        </button>
      </PopoverTrigger>
      <PopoverContent className="min-w-[220px] rounded-lg p-0 border border-default-200 dark:border-default-200/50 bg-white dark:bg-default-100 shadow-lg">
        <div className="py-1">
          {agents.map((agent) => {
            const isActive = agent.id === activeAgentId;
            return (
              <button
                key={agent.id}
                onClick={() => { setActive(agent.id); setIsOpen(false); }}
                className={cn(
                  "w-full flex items-start gap-2 px-3 py-1.5 text-left hover:bg-default-100 dark:hover:bg-default-200/40 transition-colors",
                  isActive && "bg-default-100/50 dark:bg-default-200/30",
                )}
              >
                <div className="w-4 pt-0.5 shrink-0">
                  {isActive && (
                    <CheckIcon className="size-3 text-primary" />
                  )}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="text-xs font-medium text-foreground truncate">
                    {agent.name}
                  </div>
                  <div className="text-[10px] text-default-400 truncate">
                    {providerName(agent.provider_id)} · {agent.model}
                  </div>
                </div>
              </button>
            );
          })}
          <div className="border-t border-default-200/50 mt-1 pt-1">
            <button
              onClick={() => { setIsOpen(false); openSettings("agents"); }}
              className="w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs text-default-500 hover:text-foreground hover:bg-default-100 dark:hover:bg-default-200/40 transition-colors"
            >
              <SettingsIcon className="size-3" />
              Manage agents
            </button>
          </div>
        </div>
      </PopoverContent>
    </Popover>
  );
};
