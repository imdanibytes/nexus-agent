import { useState, useEffect, useCallback } from "react";
import { Plus, Pencil, Bot } from "lucide-react";
import { useChatStore } from "@/stores/chatStore.js";
import {
  fetchAgents,
  fetchProviders,
  setActiveAgent as apiSetActive,
  type Agent,
} from "@/api/client.js";
import { AgentEditor } from "./AgentEditor.js";
import { Button, Badge } from "@imdanibytes/nexus-ui";

export function AgentsTab() {
  const { agents, activeAgentId, setAgents, setActiveAgentId, providers, setProviders } =
    useChatStore();
  const [editing, setEditing] = useState<Agent | null>(null);
  const [creating, setCreating] = useState(false);

  const refresh = useCallback(async () => {
    const [agentList, providerList] = await Promise.all([
      fetchAgents(),
      fetchProviders(),
    ]);
    setAgents(agentList);
    setProviders(providerList);
  }, [setAgents, setProviders]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const handleSetActive = async (id: string | null) => {
    setActiveAgentId(id);
    await apiSetActive(id);
  };

  const providerName = (id: string) =>
    providers.find((p) => p.id === id)?.name || "Unknown";

  if (creating) {
    return (
      <AgentEditor
        providers={providers}
        onSave={() => { setCreating(false); refresh(); }}
        onCancel={() => setCreating(false)}
      />
    );
  }

  if (editing) {
    return (
      <AgentEditor
        agent={editing}
        providers={providers}
        onSave={() => { setEditing(null); refresh(); }}
        onCancel={() => setEditing(null)}
        onDelete={() => { setEditing(null); refresh(); }}
      />
    );
  }

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-sm font-medium">Agents</h3>
          <p className="text-[11px] text-muted-foreground mt-0.5">
            Named model + prompt + parameter combinations.
          </p>
        </div>
        <Button size="sm" onClick={() => setCreating(true)} disabled={providers.length === 0} className="gap-1.5">
          <Plus size={13} />
          New
        </Button>
      </div>

      {agents.length === 0 ? (
        <div className="text-center py-12 rounded-xl border border-dashed border-border">
          <Bot size={28} className="mx-auto mb-3 text-muted-foreground/50" />
          <p className="text-sm text-muted-foreground">No agents yet</p>
          <p className="text-[11px] text-muted-foreground/70 mt-1">
            {providers.length === 0
              ? "Add a provider first, then create an agent."
              : "Create one to save a provider, model, and prompt combination."}
          </p>
        </div>
      ) : (
        <div className="space-y-1">
          {agents.map((a) => {
            const isActive = activeAgentId === a.id;
            return (
              <div
                key={a.id}
                className={`flex items-center gap-3 px-3 py-2.5 rounded-lg transition-colors cursor-pointer border ${
                  isActive
                    ? "bg-primary/5 border-primary/15"
                    : "border-transparent hover:bg-accent/50"
                }`}
                onClick={() => handleSetActive(isActive ? null : a.id)}
              >
                <div className="flex-1 min-w-0">
                  <div className="text-sm font-medium truncate">{a.name}</div>
                  <div className="text-[11px] text-muted-foreground truncate">
                    <span className="font-mono">{a.model}</span>
                    <span className="mx-1.5 text-muted-foreground/40">·</span>
                    <span>{providerName(a.providerId)}</span>
                  </div>
                </div>
                {isActive && (
                  <Badge
                    variant="secondary"
                    className="text-[10px] bg-primary/10 text-primary border-primary/15 flex-shrink-0"
                  >
                    Active
                  </Badge>
                )}
                <Button
                  size="icon"
                  variant="ghost"
                  onClick={(e) => { e.stopPropagation(); setEditing(a); }}
                  className="h-7 w-7 flex-shrink-0 text-muted-foreground/50 hover:text-foreground"
                >
                  <Pencil size={12} />
                </Button>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
