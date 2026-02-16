import { useState, useEffect } from "react";
import {
  Save, Trash2, ArrowLeft, Loader2,
} from "lucide-react";
import {
  createAgentApi,
  updateAgentApi,
  deleteAgentApi,
  probeProviderApi,
  type Agent,
  type ProviderPublic,
  type ModelInfo,
  type ToolFilter,
} from "@/api/client.js";
import { useChatStore } from "@/stores/chatStore.js";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

interface Props {
  agent?: Agent;
  providers: ProviderPublic[];
  onSave: () => void;
  onCancel: () => void;
  onDelete?: () => void;
}

type FilterMode = "all" | "allow" | "deny";

export function AgentEditor({ agent, providers, onSave, onCancel, onDelete }: Props) {
  const [name, setName] = useState(agent?.name || "");
  const [providerId, setProviderId] = useState(agent?.providerId || providers[0]?.id || "");
  const [model, setModel] = useState(agent?.model || "");
  const [systemPrompt, setSystemPrompt] = useState(agent?.systemPrompt || "");
  const [samplingMode, setSamplingMode] = useState<"temperature" | "top_p">(
    agent?.topP !== undefined && agent?.temperature === undefined ? "top_p" : "temperature",
  );
  const [temperature, setTemperature] = useState(agent?.temperature ?? 1);
  const [maxTokens, setMaxTokens] = useState(agent?.maxTokens ?? 8192);
  const [topP, setTopP] = useState(agent?.topP ?? 1);
  const [filterMode, setFilterMode] = useState<FilterMode>(
    agent?.toolFilter?.mode || "all",
  );
  const [filterTools, setFilterTools] = useState<Set<string>>(
    new Set(agent?.toolFilter?.tools || []),
  );
  const [saving, setSaving] = useState(false);
  const [discoveredModels, setDiscoveredModels] = useState<ModelInfo[]>([]);
  const [toolSearch, setToolSearch] = useState("");
  const { availableTools } = useChatStore();

  // Discover models when provider changes
  useEffect(() => {
    if (!providerId) return;
    probeProviderApi(providerId)
      .then((status) => {
        if (status.reachable) setDiscoveredModels(status.models);
      })
      .catch(() => {});
  }, [providerId]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !providerId || !model.trim()) return;
    setSaving(true);
    try {
      const toolFilter: ToolFilter | undefined =
        filterMode === "all"
          ? undefined
          : { mode: filterMode, tools: Array.from(filterTools) };

      const data = {
        name: name.trim(),
        providerId,
        model: model.trim(),
        systemPrompt: systemPrompt.trim(),
        ...(samplingMode === "temperature"
          ? { temperature, topP: null }
          : { temperature: null, topP }),
        maxTokens,
        toolFilter,
      };

      if (agent) {
        await updateAgentApi(agent.id, data);
      } else {
        await createAgentApi(data);
      }
      onSave();
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (!agent) return;
    await deleteAgentApi(agent.id);
    onDelete?.();
  };

  const toggleTool = (toolName: string) => {
    setFilterTools((prev) => {
      const next = new Set(prev);
      if (next.has(toolName)) {
        next.delete(toolName);
      } else {
        next.add(toolName);
      }
      return next;
    });
  };

  const filteredTools = availableTools.filter(
    (t) =>
      !toolSearch || t.name.toLowerCase().includes(toolSearch.toLowerCase()),
  );

  return (
    <form onSubmit={handleSubmit} className="space-y-5">
      <div className="flex items-center gap-2">
        <Button type="button" variant="ghost" size="icon" onClick={onCancel} className="h-7 w-7">
          <ArrowLeft size={14} />
        </Button>
        <div>
          <h3 className="text-sm font-medium">
            {agent ? "Edit Agent" : "New Agent"}
          </h3>
          <p className="text-[11px] text-muted-foreground">
            {agent ? "Update this agent's configuration." : "Create a new agent configuration."}
          </p>
        </div>
      </div>

      <Separator />

      {/* Name */}
      <div className="space-y-1.5">
        <Label htmlFor="agent-name" className="text-xs">Name</Label>
        <Input
          id="agent-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Research Assistant"
          required
        />
      </div>

      {/* Provider */}
      <div className="space-y-1.5">
        <Label className="text-xs">Provider</Label>
        {providers.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            No providers configured. Add one in the Providers tab first.
          </p>
        ) : (
          <Select value={providerId} onValueChange={setProviderId}>
            <SelectTrigger>
              <SelectValue placeholder="Select a provider" />
            </SelectTrigger>
            <SelectContent>
              {providers.map((p) => (
                <SelectItem key={p.id} value={p.id}>
                  {p.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
      </div>

      {/* Model */}
      <div className="space-y-1.5">
        <Label htmlFor="agent-model" className="text-xs">Model</Label>
        <Input
          id="agent-model"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          placeholder="qwen3:30b"
          list={discoveredModels.length > 0 ? "agent-model-options" : undefined}
          required
          className="font-mono text-xs"
        />
        {discoveredModels.length > 0 && (
          <datalist id="agent-model-options">
            {discoveredModels.map((m) => (
              <option key={m.id} value={m.id} />
            ))}
          </datalist>
        )}
        <p className="text-[11px] text-muted-foreground">
          {discoveredModels.length > 0
            ? `${discoveredModels.length} model${discoveredModels.length !== 1 ? "s" : ""} discovered.`
            : "Enter a model ID."}
        </p>
      </div>

      {/* System Prompt */}
      <div className="space-y-1.5">
        <Label htmlFor="agent-prompt" className="text-xs">System Prompt</Label>
        <Textarea
          id="agent-prompt"
          value={systemPrompt}
          onChange={(e) => setSystemPrompt(e.target.value)}
          placeholder="You are a helpful assistant..."
          rows={4}
          className="resize-none text-xs"
        />
      </div>

      <Separator />

      {/* Model Parameters */}
      <div className="space-y-4">
        <h4 className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
          Model Parameters
        </h4>

        <div className="space-y-1.5">
          <Label className="text-xs">Sampling Method</Label>
          <Select value={samplingMode} onValueChange={(v) => setSamplingMode(v as "temperature" | "top_p")}>
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="temperature">Temperature</SelectItem>
              <SelectItem value="top_p">Top P</SelectItem>
            </SelectContent>
          </Select>
        </div>

        {samplingMode === "temperature" ? (
          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <Label htmlFor="agent-temp" className="text-xs">Temperature</Label>
              <span className="text-xs text-muted-foreground font-mono">{temperature.toFixed(1)}</span>
            </div>
            <input
              id="agent-temp"
              type="range"
              min="0"
              max="2"
              step="0.1"
              value={temperature}
              onChange={(e) => setTemperature(parseFloat(e.target.value))}
              className="w-full accent-primary"
            />
          </div>
        ) : (
          <div className="space-y-1.5">
            <div className="flex items-center justify-between">
              <Label htmlFor="agent-top-p" className="text-xs">Top P</Label>
              <span className="text-xs text-muted-foreground font-mono">{topP.toFixed(2)}</span>
            </div>
            <input
              id="agent-top-p"
              type="range"
              min="0"
              max="1"
              step="0.01"
              value={topP}
              onChange={(e) => setTopP(parseFloat(e.target.value))}
              className="w-full accent-primary"
            />
          </div>
        )}

        <div className="space-y-1.5">
          <Label htmlFor="agent-max-tokens" className="text-xs">Max Tokens</Label>
          <Input
            id="agent-max-tokens"
            type="number"
            value={maxTokens}
            onChange={(e) => setMaxTokens(parseInt(e.target.value) || 8192)}
            min={1}
            className="font-mono text-xs"
          />
        </div>
      </div>

      <Separator />

      {/* Tool Filter */}
      <div className="space-y-3">
        <h4 className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
          Tool Access
        </h4>

        <Select value={filterMode} onValueChange={(v) => setFilterMode(v as FilterMode)}>
          <SelectTrigger>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">All tools</SelectItem>
            <SelectItem value="allow">Allow list</SelectItem>
            <SelectItem value="deny">Deny list</SelectItem>
          </SelectContent>
        </Select>

        {filterMode !== "all" && (
          <div className="space-y-2">
            <Input
              value={toolSearch}
              onChange={(e) => setToolSearch(e.target.value)}
              placeholder="Search tools..."
              className="text-xs"
            />
            <div className="max-h-48 overflow-y-auto rounded-lg border border-border">
              {filteredTools.length === 0 ? (
                <p className="text-xs text-muted-foreground p-3">No tools found.</p>
              ) : (
                filteredTools.map((tool) => (
                  <label
                    key={tool.name}
                    className="flex items-center gap-2 px-3 py-1.5 hover:bg-accent/30 cursor-pointer text-xs"
                  >
                    <input
                      type="checkbox"
                      checked={filterTools.has(tool.name)}
                      onChange={() => toggleTool(tool.name)}
                      className="accent-primary"
                    />
                    <span className="font-mono truncate flex-1">{tool.name}</span>
                  </label>
                ))
              )}
            </div>
            <p className="text-[11px] text-muted-foreground">
              {filterMode === "allow"
                ? "Only checked tools will be available."
                : "Checked tools will be blocked."}
            </p>
          </div>
        )}
      </div>

      <Separator />

      <div className="flex items-center gap-2">
        <Button
          type="submit"
          size="sm"
          disabled={!name.trim() || !providerId || !model.trim() || saving}
          className="gap-1.5"
        >
          {saving ? <Loader2 size={13} className="animate-spin" /> : <Save size={13} />}
          Save
        </Button>
        <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
          Cancel
        </Button>
        {agent && onDelete && (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={handleDelete}
            className="ml-auto text-destructive hover:text-destructive gap-1.5"
          >
            <Trash2 size={13} />
            Delete
          </Button>
        )}
      </div>
    </form>
  );
}
