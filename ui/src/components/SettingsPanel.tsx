import { useState, useEffect, useCallback } from "react";
import {
  Wifi, WifiOff, Loader2, Pencil, Plus, ArrowLeft, UserCircle, Save,
} from "lucide-react";
import { useChatStore } from "@/stores/chatStore.js";
import {
  fetchProfiles,
  createProfile as apiCreate,
  updateProfile as apiUpdate,
  deleteProfile as apiDelete,
  discoverModels,
  fetchSettings,
  saveSettings,
  setActiveProfile as apiSetActive,
} from "@/api/client.js";
import type { AgentProfile, ModelInfo, EndpointStatus, AgentSettingsPublic } from "@/api/client.js";
import { ProfileEditor } from "./ProfileEditor.js";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tooltip, TooltipTrigger, TooltipContent } from "@/components/ui/tooltip";

type SettingsTab = "connection" | "profiles";

const TABS: { id: SettingsTab; label: string; icon: typeof Wifi }[] = [
  { id: "connection", label: "Connection", icon: Wifi },
  { id: "profiles", label: "Profiles", icon: UserCircle },
];

interface Props {
  compact?: boolean;
}

export function SettingsPanel({ compact }: Props) {
  const { profiles, activeProfileId, setProfiles, setActiveProfileId, setSettingsOpen, setChatOpen } =
    useChatStore();
  const [active, setActive] = useState<SettingsTab>("connection");

  const handleBack = () => {
    setSettingsOpen(false);
    if (compact) setChatOpen(false);
  };

  return (
    <div className="flex-1 flex flex-col h-full min-w-0">
      {/* Header — same height as sidebar header so borders align */}
      <div className="flex items-center gap-2 px-4 py-3 border-b border-border flex-shrink-0">
        <Button variant="ghost" size="icon" onClick={handleBack} className="h-7 w-7">
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
      <ScrollArea className="flex-1">
        <div className="px-5 py-5">
          <div className="max-w-lg">
            {active === "connection" && <ConnectionTab />}
            {active === "profiles" && (
              <ProfilesTab
                profiles={profiles}
                activeProfileId={activeProfileId}
                setProfiles={setProfiles}
                setActiveProfileId={setActiveProfileId}
              />
            )}
          </div>
        </div>
      </ScrollArea>
    </div>
  );
}

/* ─── Connection Tab ─── */

function ConnectionTab() {
  const [settings, setSettings] = useState<AgentSettingsPublic | null>(null);
  const [endpoint, setEndpoint] = useState("");
  const [model, setModel] = useState("");
  const [systemPrompt, setSystemPrompt] = useState("");
  const [discovery, setDiscovery] = useState<EndpointStatus | null>(null);
  const [discovering, setDiscovering] = useState(false);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);

  useEffect(() => {
    fetchSettings().then((s) => {
      setSettings(s);
      setEndpoint(s.llm_endpoint);
      setModel(s.llm_model);
      setSystemPrompt(s.system_prompt);
    });
  }, []);

  useEffect(() => {
    if (!settings) return;
    setDirty(
      endpoint !== settings.llm_endpoint ||
      model !== settings.llm_model ||
      systemPrompt !== settings.system_prompt
    );
  }, [endpoint, model, systemPrompt, settings]);

  const handleDiscover = useCallback(async () => {
    setDiscovering(true);
    try {
      setDiscovery(await discoverModels());
    } catch {
      setDiscovery({ reachable: false, provider: "unknown", error: "Request failed", models: [] });
    } finally {
      setDiscovering(false);
    }
  }, []);

  const handleSave = useCallback(async () => {
    setSaving(true);
    try {
      await saveSettings({
        llm_endpoint: endpoint,
        llm_model: model,
        system_prompt: systemPrompt,
      });
      const updated = await fetchSettings();
      setSettings(updated);
    } finally {
      setSaving(false);
    }
  }, [endpoint, model, systemPrompt]);

  if (!settings) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 size={18} className="animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Endpoint */}
      <section className="space-y-2">
        <Label htmlFor="endpoint">Endpoint</Label>
        <div className="flex items-center gap-2">
          <Input
            id="endpoint"
            value={endpoint}
            onChange={(e) => setEndpoint(e.target.value)}
            placeholder="http://host.docker.internal:11434"
            className="font-mono text-xs"
          />
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="outline"
                size="icon"
                onClick={handleDiscover}
                disabled={discovering}
                className="flex-shrink-0 h-9 w-9"
              >
                {discovering ? (
                  <Loader2 size={14} className="animate-spin" />
                ) : discovery?.reachable ? (
                  <Wifi size={14} className="text-green-400" />
                ) : discovery && !discovery.reachable ? (
                  <WifiOff size={14} className="text-destructive" />
                ) : (
                  <Wifi size={14} />
                )}
              </Button>
            </TooltipTrigger>
            <TooltipContent>Test connection</TooltipContent>
          </Tooltip>
        </div>
        {discovery && (
          <p className="text-xs">
            {discovery.reachable ? (
              <span className="text-green-400">Connected — {discovery.provider}</span>
            ) : (
              <span className="text-destructive">{discovery.error || "Unreachable"}</span>
            )}
          </p>
        )}
      </section>

      {/* Model */}
      <section className="space-y-2">
        <Label htmlFor="model">Default Model</Label>
        <Input
          id="model"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          placeholder="qwen3:30b"
          list={discovery?.models.length ? "settings-model-options" : undefined}
          className="font-mono text-xs"
        />
        {discovery && discovery.models.length > 0 && (
          <datalist id="settings-model-options">
            {discovery.models.map((m) => (
              <option key={m.id} value={m.id} />
            ))}
          </datalist>
        )}
        <p className="text-[11px] text-muted-foreground">
          Used when no profile is active.
          {discovery && discovery.models.length > 0
            ? ` ${discovery.models.length} model${discovery.models.length !== 1 ? "s" : ""} available.`
            : " Test connection to discover models."}
        </p>
      </section>

      {/* System Prompt */}
      <section className="space-y-2">
        <Label htmlFor="sys-prompt">System Prompt</Label>
        <Textarea
          id="sys-prompt"
          value={systemPrompt}
          onChange={(e) => setSystemPrompt(e.target.value)}
          placeholder="You are a helpful assistant..."
          rows={4}
          className="resize-none text-xs"
        />
        <p className="text-[11px] text-muted-foreground">
          Default instructions for the agent. Profiles can override this.
        </p>
      </section>

      {/* Save */}
      {dirty && (
        <Button size="sm" onClick={handleSave} disabled={saving} className="gap-1.5">
          {saving ? <Loader2 size={13} className="animate-spin" /> : <Save size={13} />}
          Save Changes
        </Button>
      )}
    </div>
  );
}

/* ─── Profiles Tab ─── */

function ProfilesTab({
  profiles,
  activeProfileId,
  setProfiles,
  setActiveProfileId,
}: {
  profiles: AgentProfile[];
  activeProfileId: string | null;
  setProfiles: (p: AgentProfile[]) => void;
  setActiveProfileId: (id: string | null) => void;
}) {
  const [editingProfile, setEditingProfile] = useState<AgentProfile | null>(null);
  const [creating, setCreating] = useState(false);
  const [discovery, setDiscovery] = useState<EndpointStatus | null>(null);

  useEffect(() => {
    fetchProfiles().then(setProfiles);
    discoverModels().then(setDiscovery).catch(() => {});
  }, [setProfiles]);

  const models: ModelInfo[] = discovery?.models || [];

  const handleSaveNew = async (data: {
    name: string;
    model: string;
    systemPrompt: string;
    avatar?: string;
  }) => {
    await apiCreate(data);
    setCreating(false);
    setProfiles(await fetchProfiles());
  };

  const handleSaveEdit = async (data: {
    name: string;
    model: string;
    systemPrompt: string;
    avatar?: string;
  }) => {
    if (!editingProfile) return;
    await apiUpdate(editingProfile.id, data);
    setEditingProfile(null);
    setProfiles(await fetchProfiles());
  };

  const handleDelete = async (id: string) => {
    await apiDelete(id);
    setEditingProfile(null);
    if (activeProfileId === id) {
      setActiveProfileId(null);
      await apiSetActive(null);
    }
    setProfiles(await fetchProfiles());
  };

  const handleSetActive = async (id: string | null) => {
    setActiveProfileId(id);
    await apiSetActive(id);
  };

  if (creating) {
    return (
      <ProfileEditor
        models={models}
        onSave={handleSaveNew}
        onCancel={() => setCreating(false)}
      />
    );
  }

  if (editingProfile) {
    return (
      <ProfileEditor
        profile={editingProfile}
        models={models}
        onSave={handleSaveEdit}
        onDelete={() => handleDelete(editingProfile.id)}
        onCancel={() => setEditingProfile(null)}
      />
    );
  }

  return (
    <div className="space-y-5">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-sm font-medium">Agent Profiles</h3>
          <p className="text-[11px] text-muted-foreground mt-0.5">
            Named model + system prompt combos.
          </p>
        </div>
        <Button size="sm" onClick={() => setCreating(true)} className="gap-1.5">
          <Plus size={13} />
          New
        </Button>
      </div>

      {profiles.length === 0 ? (
        <div className="text-center py-12 rounded-xl border border-dashed border-border">
          <UserCircle size={28} className="mx-auto mb-3 text-muted-foreground/50" />
          <p className="text-sm text-muted-foreground">No profiles yet</p>
          <p className="text-[11px] text-muted-foreground/70 mt-1">
            Create one to save a model and system prompt combination.
          </p>
        </div>
      ) : (
        <div className="space-y-1">
          {profiles.map((p) => {
            const isActive = activeProfileId === p.id;
            return (
              <div
                key={p.id}
                className={`flex items-center gap-3 px-3 py-2.5 rounded-lg transition-colors cursor-pointer border ${
                  isActive
                    ? "bg-primary/5 border-primary/15"
                    : "border-transparent hover:bg-accent/50"
                }`}
                onClick={() => handleSetActive(isActive ? null : p.id)}
              >
                <div className="flex-shrink-0 w-8 h-8 rounded-full bg-secondary flex items-center justify-center text-sm">
                  {p.avatar || p.name.charAt(0)}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="text-sm font-medium truncate">{p.name}</div>
                  <div className="text-[11px] text-muted-foreground truncate font-mono">{p.model}</div>
                </div>
                {isActive && (
                  <Badge variant="secondary" className="text-[10px] bg-primary/10 text-primary border-primary/15 flex-shrink-0">
                    Active
                  </Badge>
                )}
                <Button
                  size="icon"
                  variant="ghost"
                  onClick={(e) => { e.stopPropagation(); setEditingProfile(p); }}
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
