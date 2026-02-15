import { useState, useEffect } from "react";
import { Plus, X, Loader2 } from "lucide-react";
import {
  fetchToolSettings,
  updateToolSettingsApi,
  type ToolSettings,
  type ToolFilter,
} from "@/api/client.js";
import { useChatStore } from "@/stores/chatStore.js";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

type FilterMode = "all" | "allow" | "deny";

export function ToolsTab() {
  const { availableTools } = useChatStore();
  const [settings, setSettings] = useState<ToolSettings | null>(null);
  const [patterns, setPatterns] = useState<string[]>([]);
  const [newPattern, setNewPattern] = useState("");
  const [filterMode, setFilterMode] = useState<FilterMode>("all");
  const [filterTools, setFilterTools] = useState<Set<string>>(new Set());
  const [toolSearch, setToolSearch] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    fetchToolSettings().then((s) => {
      setSettings(s);
      setPatterns(s.hiddenToolPatterns);
      setFilterMode(s.globalToolFilter?.mode || "all");
      setFilterTools(new Set(s.globalToolFilter?.tools || []));
    });
  }, []);

  const save = async (
    newPatterns?: string[],
    newFilterMode?: FilterMode,
    newFilterTools?: Set<string>,
  ) => {
    setSaving(true);
    try {
      const p = newPatterns ?? patterns;
      const fm = newFilterMode ?? filterMode;
      const ft = newFilterTools ?? filterTools;

      const globalToolFilter: ToolFilter | undefined =
        fm === "all"
          ? undefined
          : { mode: fm, tools: Array.from(ft) };

      const updated = await updateToolSettingsApi({
        hiddenToolPatterns: p,
        globalToolFilter,
      });
      setSettings(updated);
    } finally {
      setSaving(false);
    }
  };

  const addPattern = () => {
    const trimmed = newPattern.trim();
    if (!trimmed || patterns.includes(trimmed)) return;
    const next = [...patterns, trimmed];
    setPatterns(next);
    setNewPattern("");
    save(next);
  };

  const removePattern = (idx: number) => {
    const next = patterns.filter((_, i) => i !== idx);
    setPatterns(next);
    save(next);
  };

  const handleFilterModeChange = (mode: FilterMode) => {
    setFilterMode(mode);
    save(undefined, mode);
  };

  const toggleTool = (toolName: string) => {
    setFilterTools((prev) => {
      const next = new Set(prev);
      if (next.has(toolName)) {
        next.delete(toolName);
      } else {
        next.add(toolName);
      }
      save(undefined, undefined, next);
      return next;
    });
  };

  if (!settings) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 size={18} className="animate-spin text-muted-foreground" />
      </div>
    );
  }

  const filteredTools = availableTools.filter(
    (t) => !toolSearch || t.name.toLowerCase().includes(toolSearch.toLowerCase()),
  );

  return (
    <div className="space-y-6">
      {/* UI Visibility */}
      <section className="space-y-3">
        <div>
          <h3 className="text-sm font-medium">UI Visibility</h3>
          <p className="text-[11px] text-muted-foreground mt-0.5">
            Tool calls matching these patterns won't appear in the chat.
          </p>
        </div>

        <div className="space-y-1.5">
          <Label className="text-xs">Hidden Tool Patterns</Label>
          <div className="space-y-1">
            {patterns.map((p, i) => (
              <div
                key={i}
                className="flex items-center gap-2 px-3 py-1.5 rounded-md bg-secondary/50 text-xs font-mono"
              >
                <span className="flex-1 truncate">{p}</span>
                <button
                  onClick={() => removePattern(i)}
                  className="text-muted-foreground hover:text-destructive transition-colors"
                >
                  <X size={12} />
                </button>
              </div>
            ))}
          </div>
          <div className="flex gap-2">
            <Input
              value={newPattern}
              onChange={(e) => setNewPattern(e.target.value)}
              placeholder="_nexus_*"
              className="font-mono text-xs"
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  addPattern();
                }
              }}
            />
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={addPattern}
              disabled={!newPattern.trim()}
              className="flex-shrink-0"
            >
              <Plus size={13} />
            </Button>
          </div>
        </div>
      </section>

      <Separator />

      {/* Global Tool Filter */}
      <section className="space-y-3">
        <div>
          <h3 className="text-sm font-medium">Global Tool Filter</h3>
          <p className="text-[11px] text-muted-foreground mt-0.5">
            Control which tools are available to all agents.
          </p>
        </div>

        <Select value={filterMode} onValueChange={(v) => handleFilterModeChange(v as FilterMode)}>
          <SelectTrigger>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">Allow all tools</SelectItem>
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
                    <span className="text-muted-foreground/60 text-[10px]">{tool.source}</span>
                  </label>
                ))
              )}
            </div>
            <p className="text-[11px] text-muted-foreground">
              {filterMode === "allow"
                ? "Only checked tools will be available to any agent."
                : "Checked tools will be blocked for all agents."}
            </p>
          </div>
        )}

        {saving && (
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <Loader2 size={12} className="animate-spin" />
            Saving...
          </div>
        )}
      </section>
    </div>
  );
}
