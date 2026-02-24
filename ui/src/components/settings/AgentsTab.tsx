import { type FC, useState, useEffect } from "react";
import { PlusIcon, TrashIcon, CheckIcon, Loader2Icon } from "lucide-react";
import { Select, SelectItem } from "@heroui/react";
import { useAgentStore } from "../../stores/agentStore";
import { useProviderStore } from "../../stores/providerStore";
import { fetchProviderModels, type ModelInfo } from "../../api/client";
import type { AgentConfig, CreateAgentRequest } from "../../api/client";
import { cn } from "../../lib/utils";

type EditorMode = { type: "closed" } | { type: "create" } | { type: "edit"; agent: AgentConfig };

export const AgentsTab: FC = () => {
  const { agents, activeAgentId, setActiveAgent, deleteAgent } =
    useAgentStore();
  const providers = useProviderStore((s) => s.providers);
  const [mode, setMode] = useState<EditorMode>({ type: "closed" });

  const providerName = (id: string) =>
    providers.find((p) => p.id === id)?.name ?? "Unknown";

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium text-foreground">Agents</h3>
        <button
          onClick={() => setMode({ type: "create" })}
          disabled={providers.length === 0}
          className="flex items-center gap-1 px-2 py-1 text-xs font-medium rounded-md bg-primary text-white hover:bg-primary/90 disabled:opacity-50 transition-colors"
        >
          <PlusIcon className="size-3" />
          Add
        </button>
      </div>

      {providers.length === 0 && (
        <p className="text-xs text-default-400">
          Add a provider first before creating agents.
        </p>
      )}

      {agents.length === 0 && providers.length > 0 && mode.type === "closed" && (
        <p className="text-xs text-default-400">
          No agents configured. Add one to get started.
        </p>
      )}

      {agents.map((a) => {
        const isActive = a.id === activeAgentId;
        return (
          <div
            key={a.id}
            className={cn(
              "flex items-center gap-3 p-3 rounded-lg border bg-default-50/30 cursor-pointer transition-colors",
              isActive
                ? "border-primary/40 bg-primary/5"
                : "border-default-200/50 hover:border-default-300/50",
            )}
            onClick={() => setActiveAgent(a.id)}
          >
            <div className="w-5 shrink-0 flex justify-center">
              {isActive && <CheckIcon className="size-3.5 text-primary" />}
            </div>
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-2">
                <span className="text-sm font-medium text-foreground truncate">
                  {a.name}
                </span>
                {isActive && (
                  <span className="text-[9px] px-1.5 py-0.5 rounded-full bg-primary/10 text-primary font-medium uppercase tracking-wide">
                    Active
                  </span>
                )}
              </div>
              <div className="text-[11px] text-default-400 mt-0.5">
                {providerName(a.provider_id)} · {a.model}
                {a.max_tokens && ` · ${a.max_tokens} tokens`}
              </div>
            </div>
            <button
              onClick={(e) => {
                e.stopPropagation();
                setMode({ type: "edit", agent: a });
              }}
              className="text-[11px] text-default-500 hover:text-foreground px-2 py-1 rounded hover:bg-default-200/40 transition-colors"
            >
              Edit
            </button>
            <button
              onClick={(e) => {
                e.stopPropagation();
                if (confirm(`Delete agent "${a.name}"?`)) {
                  deleteAgent(a.id);
                }
              }}
              className="text-default-400 hover:text-danger p-1 rounded hover:bg-danger/10 transition-colors"
            >
              <TrashIcon className="size-3.5" />
            </button>
          </div>
        );
      })}

      {mode.type !== "closed" && (
        <AgentEditor
          agent={mode.type === "edit" ? mode.agent : undefined}
          onClose={() => setMode({ type: "closed" })}
        />
      )}
    </div>
  );
};

const AgentEditor: FC<{
  agent?: AgentConfig;
  onClose: () => void;
}> = ({ agent, onClose }) => {
  const { createAgent, updateAgent } = useAgentStore();
  const providers = useProviderStore((s) => s.providers);
  const isEdit = !!agent;

  const [name, setName] = useState(agent?.name ?? "");
  const [providerId, setProviderId] = useState(
    agent?.provider_id ?? providers[0]?.id ?? "",
  );
  const [model, setModel] = useState(agent?.model ?? "");
  const [discoveredModels, setDiscoveredModels] = useState<ModelInfo[]>([]);
  const [loadingModels, setLoadingModels] = useState(false);
  const [systemPrompt, setSystemPrompt] = useState(
    agent?.system_prompt ?? "",
  );
  const [temperature, setTemperature] = useState<string>(
    agent?.temperature?.toString() ?? "",
  );
  const [maxTokens, setMaxTokens] = useState<string>(
    agent?.max_tokens?.toString() ?? "8192",
  );
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!providerId) return;
    setLoadingModels(true);
    fetchProviderModels(providerId)
      .then((models) => setDiscoveredModels(models))
      .catch(() => setDiscoveredModels([]))
      .finally(() => setLoadingModels(false));
  }, [providerId]);

  const handleSave = async () => {
    setSaving(true);
    try {
      const data: CreateAgentRequest = {
        name,
        provider_id: providerId,
        model,
        ...(systemPrompt ? { system_prompt: systemPrompt } : {}),
        ...(temperature ? { temperature: parseFloat(temperature) } : {}),
        ...(maxTokens ? { max_tokens: parseInt(maxTokens) } : {}),
      };

      if (isEdit) {
        await updateAgent(agent!.id, {
          ...data,
          set_temperature: true,
          set_max_tokens: true,
        });
      } else {
        await createAgent(data);
      }
      onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="rounded-lg border border-primary/30 bg-primary/5 p-4 space-y-3">
      <h4 className="text-xs font-semibold text-foreground">
        {isEdit ? "Edit Agent" : "New Agent"}
      </h4>

      <div className="grid grid-cols-2 gap-3">
        <Field label="Name">
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="My Agent"
            className="input-field"
          />
        </Field>

        <Field label="Provider">
          <Select
            aria-label="Provider"
            size="sm"
            variant="bordered"
            selectedKeys={providerId ? [providerId] : []}
            onSelectionChange={(keys) => {
              const key = [...keys][0] as string | undefined;
              if (key) setProviderId(key);
            }}
            classNames={{
              trigger: "input-field !h-auto",
              value: "text-xs",
            }}
          >
            {providers.map((p) => (
              <SelectItem key={p.id}>{p.name}</SelectItem>
            ))}
          </Select>
        </Field>
      </div>

      <Field label="Model">
        {loadingModels ? (
          <div className="flex items-center gap-2 h-[30px] text-xs text-default-400">
            <Loader2Icon className="size-3 animate-spin" />
            Loading models...
          </div>
        ) : discoveredModels.length > 0 ? (
          <Select
            aria-label="Model"
            size="sm"
            variant="bordered"
            selectedKeys={model ? [model] : []}
            onSelectionChange={(keys) => {
              const key = [...keys][0] as string | undefined;
              if (key) setModel(key);
            }}
            classNames={{
              trigger: "input-field !h-auto",
              value: "text-xs",
            }}
          >
            {discoveredModels.map((m) => (
              <SelectItem key={m.id} textValue={m.name}>
                <div className="flex flex-col">
                  <span className="text-xs">{m.name}</span>
                  <span className="text-[10px] text-default-400">{m.id}</span>
                </div>
              </SelectItem>
            ))}
          </Select>
        ) : (
          <input
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder="claude-sonnet-4-20250514"
            className="input-field"
          />
        )}
      </Field>

      <Field label="System Prompt (optional)">
        <textarea
          value={systemPrompt}
          onChange={(e) => setSystemPrompt(e.target.value)}
          rows={3}
          placeholder="You are a helpful assistant..."
          className="input-field resize-y min-h-[60px]"
        />
      </Field>

      <div className="grid grid-cols-2 gap-3">
        <Field label="Temperature (optional)">
          <input
            type="number"
            min="0"
            max="1"
            step="0.1"
            value={temperature}
            onChange={(e) => setTemperature(e.target.value)}
            placeholder="0.7"
            className="input-field"
          />
        </Field>

        <Field label="Max Tokens">
          <input
            type="number"
            min="1"
            max="200000"
            value={maxTokens}
            onChange={(e) => setMaxTokens(e.target.value)}
            placeholder="8192"
            className="input-field"
          />
        </Field>
      </div>

      <div className="flex items-center gap-2 pt-1">
        <button
          onClick={handleSave}
          disabled={!name || !model || !providerId || saving}
          className="px-3 py-1.5 text-xs font-medium rounded-md bg-primary text-white hover:bg-primary/90 disabled:opacity-50 transition-colors"
        >
          {saving ? "Saving..." : isEdit ? "Update" : "Create"}
        </button>
        <button
          onClick={onClose}
          className="px-3 py-1.5 text-xs text-default-500 hover:text-foreground transition-colors"
        >
          Cancel
        </button>
      </div>
    </div>
  );
};

const Field: FC<{ label: string; children: React.ReactNode }> = ({
  label,
  children,
}) => (
  <div>
    <label className="block text-[11px] text-default-500 mb-1">{label}</label>
    {children}
  </div>
);
