import { useState } from "react";
import { ArrowLeft, Bot, Database, Server, Wrench } from "lucide-react";
import { useChatStore } from "@/stores/chatStore.js";
import { AgentsTab } from "./AgentsTab.js";
import { ProvidersTab } from "./ProvidersTab.js";
import { ToolsTab } from "./ToolsTab.js";
import { DataTab } from "./DataTab.js";
import { Button } from "@imdanibytes/nexus-ui";

type SettingsTab = "agents" | "providers" | "tools" | "data";

const TABS: { id: SettingsTab; label: string; icon: typeof Bot }[] = [
  { id: "agents", label: "Agents", icon: Bot },
  { id: "providers", label: "Providers", icon: Server },
  { id: "tools", label: "Tools", icon: Wrench },
  { id: "data", label: "Data", icon: Database },
];

export function SettingsPage() {
  const { setSettingsOpen } = useChatStore();
  const [active, setActive] = useState<SettingsTab>("agents");

  return (
    <div className="flex-1 flex flex-col h-full min-w-0 min-h-0 overflow-hidden">
      {/* Header */}
      <div className="flex items-center gap-2 px-4 py-3 border-b border-border flex-shrink-0">
        <Button
          variant="ghost"
          size="icon"
          onClick={() => setSettingsOpen(false)}
          className="h-7 w-7"
        >
          <ArrowLeft size={15} />
        </Button>
        <h2 className="text-sm font-semibold">Settings</h2>
      </div>

      {/* Tab strip */}
      <div className="flex gap-0.5 px-4 pt-1.5 border-b border-border flex-shrink-0">
        {TABS.map((tab) => {
          const Icon = tab.icon;
          const isActive = active === tab.id;
          return (
            <button
              key={tab.id}
              onClick={() => setActive(tab.id)}
              className={`flex items-center gap-1.5 px-3 py-2 text-[12px] font-medium rounded-t-lg transition-colors whitespace-nowrap border-b-2 ${
                isActive
                  ? "border-primary text-foreground bg-card/50"
                  : "border-transparent text-muted-foreground hover:text-foreground hover:bg-accent/30"
              }`}
            >
              <Icon size={14} strokeWidth={1.5} />
              {tab.label}
            </button>
          );
        })}
      </div>

      {/* Content */}
      <div className="flex-1 min-h-0 overflow-y-auto">
        <div className="px-5 py-5">
          <div className="max-w-lg">
            {active === "agents" && <AgentsTab />}
            {active === "providers" && <ProvidersTab />}
            {active === "tools" && <ToolsTab />}
            {active === "data" && <DataTab />}
          </div>
        </div>
      </div>
    </div>
  );
}
