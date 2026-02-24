import { type FC, useState, useRef, useEffect } from "react";
import { ChevronDownIcon, CheckIcon, SettingsIcon } from "lucide-react";
import { useAgentStore } from "../../stores/agentStore";
import { useProviderStore } from "../../stores/providerStore";
import { useUIStore } from "../../stores/uiStore";
import { cn } from "../../lib/utils";

export const AgentSwitcher: FC = () => {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  const agents = useAgentStore((s) => s.agents);
  const activeAgentId = useAgentStore((s) => s.activeAgentId);
  const setActive = useAgentStore((s) => s.setActiveAgent);
  const providers = useProviderStore((s) => s.providers);
  const setSettingsOpen = useUIStore((s) => s.setSettingsOpen);

  const activeAgent = agents.find((a) => a.id === activeAgentId);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  if (agents.length === 0) return null;

  const providerName = (providerId: string) =>
    providers.find((p) => p.id === providerId)?.name ?? "Unknown";

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] font-medium text-default-500 hover:text-foreground hover:bg-default-200/40 transition-colors"
      >
        <span className="truncate max-w-[120px]">
          {activeAgent?.name ?? "No agent"}
        </span>
        <ChevronDownIcon className="size-3 shrink-0" />
      </button>

      {open && (
        <div className="absolute top-full left-0 mt-1 z-50 min-w-[220px] rounded-lg border border-default-200 bg-white dark:bg-default-50 shadow-lg py-1">
          {agents.map((agent) => {
            const isActive = agent.id === activeAgentId;
            return (
              <button
                key={agent.id}
                onClick={() => {
                  setActive(agent.id);
                  setOpen(false);
                }}
                className={cn(
                  "w-full flex items-start gap-2 px-3 py-1.5 text-left hover:bg-default-100 transition-colors",
                  isActive && "bg-default-100/50",
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
              onClick={() => {
                setOpen(false);
                setSettingsOpen(true);
              }}
              className="w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs text-default-500 hover:text-foreground hover:bg-default-100 transition-colors"
            >
              <SettingsIcon className="size-3" />
              Manage agents
            </button>
          </div>
        </div>
      )}
    </div>
  );
};
