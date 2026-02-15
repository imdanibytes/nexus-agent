import { useState, useEffect, useCallback } from "react";
import {
  Plus, Pencil, Server, CircleDot,
} from "lucide-react";
import { useChatStore } from "@/stores/chatStore.js";
import {
  fetchProviders,
  probeProviderApi,
  type ProviderPublic,
  type EndpointStatus,
} from "@/api/client.js";
import { ProviderEditor } from "./ProviderEditor.js";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";

const TYPE_LABELS: Record<string, string> = {
  ollama: "Ollama",
  anthropic: "Anthropic",
  bedrock: "Bedrock",
  "openai-compatible": "OpenAI",
};

export function ProvidersTab() {
  const { providers, setProviders } = useChatStore();
  const [editing, setEditing] = useState<ProviderPublic | null>(null);
  const [creating, setCreating] = useState(false);
  const [probeResults, setProbeResults] = useState<Record<string, EndpointStatus>>({});

  const refresh = useCallback(async () => {
    const list = await fetchProviders();
    setProviders(list);
    // Clear cached probe results so they re-run with fresh data
    setProbeResults({});
  }, [setProviders]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // Probe all providers on load
  useEffect(() => {
    for (const p of providers) {
      if (!probeResults[p.id]) {
        probeProviderApi(p.id)
          .then((status) =>
            setProbeResults((prev) => ({ ...prev, [p.id]: status })),
          )
          .catch(() => {});
      }
    }
  }, [providers]);

  if (creating) {
    return (
      <ProviderEditor
        onSave={() => { setCreating(false); refresh(); }}
        onCancel={() => setCreating(false)}
      />
    );
  }

  if (editing) {
    return (
      <ProviderEditor
        provider={editing}
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
          <h3 className="text-sm font-medium">Providers</h3>
          <p className="text-[11px] text-muted-foreground mt-0.5">
            LLM service connections.
          </p>
        </div>
        <Button size="sm" onClick={() => setCreating(true)} className="gap-1.5">
          <Plus size={13} />
          New
        </Button>
      </div>

      {providers.length === 0 ? (
        <div className="text-center py-12 rounded-xl border border-dashed border-border">
          <Server size={28} className="mx-auto mb-3 text-muted-foreground/50" />
          <p className="text-sm text-muted-foreground">No providers yet</p>
          <p className="text-[11px] text-muted-foreground/70 mt-1">
            Add a provider to connect to an LLM service.
          </p>
        </div>
      ) : (
        <div className="space-y-1">
          {providers.map((p) => {
            const probe = probeResults[p.id];
            return (
              <div
                key={p.id}
                className="flex items-center gap-3 px-3 py-2.5 rounded-lg border border-transparent hover:bg-accent/50 transition-colors"
              >
                <CircleDot
                  size={10}
                  strokeWidth={2.5}
                  className={
                    probe?.reachable
                      ? "text-green-400"
                      : probe && !probe.reachable
                        ? "text-destructive"
                        : "text-muted-foreground/40"
                  }
                />
                <div className="flex-1 min-w-0">
                  <div className="text-sm font-medium truncate">{p.name}</div>
                  {p.endpoint && (
                    <div className="text-[11px] text-muted-foreground truncate font-mono">
                      {p.endpoint}
                    </div>
                  )}
                </div>
                <Badge
                  variant="secondary"
                  className="text-[10px] flex-shrink-0"
                >
                  {TYPE_LABELS[p.type] || p.type}
                </Badge>
                <Button
                  size="icon"
                  variant="ghost"
                  onClick={() => setEditing(p)}
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
