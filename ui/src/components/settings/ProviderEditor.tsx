import { useState } from "react";
import {
  Save, Trash2, ArrowLeft, Loader2, Wifi, WifiOff, ChevronsUpDown, Check,
} from "lucide-react";
import {
  createProviderApi,
  updateProviderApi,
  deleteProviderApi,
  probeProviderDataApi,
  type ProviderPublic,
  type ProviderType,
  type ProviderCreateData,
  type EndpointStatus,
} from "@/api/client.js";
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
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { cn } from "@/lib/utils";

const TYPE_OPTIONS: { value: ProviderType; label: string }[] = [
  { value: "ollama", label: "Ollama" },
  { value: "anthropic", label: "Anthropic" },
  { value: "bedrock", label: "AWS Bedrock" },
  { value: "openai-compatible", label: "OpenAI-compatible" },
];

const NAME_PLACEHOLDERS: Record<ProviderType, string> = {
  ollama: "Local Ollama",
  anthropic: "Anthropic",
  bedrock: "AWS Bedrock",
  "openai-compatible": "vLLM Server",
};

const AWS_REGIONS = [
  "us-east-1",
  "us-east-2",
  "us-west-1",
  "us-west-2",
  "us-gov-east-1",
  "us-gov-west-1",
  "af-south-1",
  "ap-east-2",
  "ap-northeast-1",
  "ap-northeast-2",
  "ap-northeast-3",
  "ap-south-1",
  "ap-south-2",
  "ap-southeast-1",
  "ap-southeast-2",
  "ap-southeast-3",
  "ap-southeast-4",
  "ca-central-1",
  "eu-central-1",
  "eu-central-2",
  "eu-north-1",
  "eu-south-1",
  "eu-south-2",
  "eu-west-1",
  "eu-west-2",
  "eu-west-3",
  "il-central-1",
  "me-central-1",
  "me-south-1",
  "mx-central-1",
  "sa-east-1",
];

function RegionCombobox({ value, onSelect }: { value: string; onSelect: (v: string) => void }) {
  const [open, setOpen] = useState(false);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="outline"
          role="combobox"
          aria-expanded={open}
          className="w-full justify-between font-mono text-xs"
        >
          {value || "Select region..."}
          <ChevronsUpDown className="ml-2 h-3.5 w-3.5 shrink-0 opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[--radix-popover-trigger-width] p-0" align="start">
        <Command>
          <CommandInput placeholder="Search regions..." className="text-xs" />
          <CommandList>
            <CommandEmpty>No region found.</CommandEmpty>
            <CommandGroup>
              {AWS_REGIONS.map((r) => (
                <CommandItem
                  key={r}
                  value={r}
                  onSelect={(v) => { onSelect(v); setOpen(false); }}
                  className="font-mono text-xs"
                >
                  <Check className={cn("mr-2 h-3.5 w-3.5", value === r ? "opacity-100" : "opacity-0")} />
                  {r}
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}

interface Props {
  provider?: ProviderPublic;
  onSave: () => void;
  onCancel: () => void;
  onDelete?: () => void;
}

export function ProviderEditor({ provider, onSave, onCancel, onDelete }: Props) {
  const [name, setName] = useState(provider?.name || "");
  const [type, setType] = useState<ProviderType>(provider?.type || "ollama");
  const [endpoint, setEndpoint] = useState(provider?.endpoint || "");
  const [apiKey, setApiKey] = useState("");
  const [awsRegion, setAwsRegion] = useState(provider?.awsRegion || "us-east-1");
  const [awsAccessKeyId, setAwsAccessKeyId] = useState("");
  const [awsSecretAccessKey, setAwsSecretAccessKey] = useState("");
  const [awsSessionToken, setAwsSessionToken] = useState("");
  const [saving, setSaving] = useState(false);
  const [probing, setProbing] = useState(false);
  const [probeResult, setProbeResult] = useState<EndpointStatus | null>(null);

  // Build provider data from current form state
  const buildData = (): ProviderCreateData => {
    const data: ProviderCreateData = { name: name.trim(), type };
    if (type === "ollama" || type === "openai-compatible") {
      data.endpoint = endpoint.trim();
    }
    if (type === "anthropic") {
      if (endpoint.trim()) data.endpoint = endpoint.trim();
      if (apiKey) data.apiKey = apiKey;
    }
    if (type === "openai-compatible" && apiKey) {
      data.apiKey = apiKey;
    }
    if (type === "bedrock") {
      data.awsRegion = awsRegion;
      if (awsAccessKeyId) data.awsAccessKeyId = awsAccessKeyId;
      if (awsSecretAccessKey) data.awsSecretAccessKey = awsSecretAccessKey;
      if (awsSessionToken) data.awsSessionToken = awsSessionToken;
    }
    return data;
  };

  // Reset probe when connection-relevant fields change
  const clearProbe = () => setProbeResult(null);

  const handleProbe = async () => {
    setProbing(true);
    try {
      const data = buildData();
      // Include provider ID so the server can fill in stored secrets for empty fields
      if (provider) (data as any).id = provider.id;
      const result = await probeProviderDataApi(data);
      setProbeResult(result);
    } catch {
      setProbeResult({ reachable: false, provider: "unknown", error: "Request failed", models: [] });
    } finally {
      setProbing(false);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !probeResult?.reachable) return;
    setSaving(true);
    try {
      if (provider) {
        await updateProviderApi(provider.id, buildData());
      } else {
        await createProviderApi(buildData());
      }
      onSave();
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (!provider) return;
    await deleteProviderApi(provider.id);
    onDelete?.();
  };

  const canSave = name.trim() && probeResult?.reachable && !saving;

  return (
    <form onSubmit={handleSubmit} className="space-y-5">
      <div className="flex items-center gap-2">
        <Button type="button" variant="ghost" size="icon" onClick={onCancel} className="h-7 w-7">
          <ArrowLeft size={14} />
        </Button>
        <div>
          <h3 className="text-sm font-medium">
            {provider ? "Edit Provider" : "New Provider"}
          </h3>
          <p className="text-[11px] text-muted-foreground">
            {provider ? "Update connection settings." : "Connect to an LLM service."}
          </p>
        </div>
      </div>

      <Separator />

      {/* Name */}
      <div className="space-y-1.5">
        <Label htmlFor="prov-name" className="text-xs">Name</Label>
        <Input
          id="prov-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={NAME_PLACEHOLDERS[type]}
          required
        />
      </div>

      {/* Type */}
      <div className="space-y-1.5">
        <Label className="text-xs">Type</Label>
        <Select value={type} onValueChange={(v) => { setType(v as ProviderType); clearProbe(); }}>
          <SelectTrigger>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {TYPE_OPTIONS.map((o) => (
              <SelectItem key={o.value} value={o.value}>
                {o.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {/* Type-specific fields */}
      {(type === "ollama" || type === "openai-compatible") && (
        <div className="space-y-1.5">
          <Label htmlFor="prov-endpoint" className="text-xs">Endpoint</Label>
          <Input
            id="prov-endpoint"
            value={endpoint}
            onChange={(e) => { setEndpoint(e.target.value); clearProbe(); }}
            placeholder={type === "ollama" ? "http://host.docker.internal:11434" : "http://localhost:8080"}
            className="font-mono text-xs"
            required
          />
        </div>
      )}

      {type === "anthropic" && (
        <>
          <div className="space-y-1.5">
            <Label htmlFor="prov-apikey" className="text-xs">API Key</Label>
            <Input
              id="prov-apikey"
              type="password"
              value={apiKey}
              onChange={(e) => { setApiKey(e.target.value); clearProbe(); }}
              placeholder={provider ? "••••••••• (unchanged)" : "sk-ant-..."}
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="prov-endpoint-ant" className="text-xs">
              Endpoint Override <span className="text-muted-foreground">(optional)</span>
            </Label>
            <Input
              id="prov-endpoint-ant"
              value={endpoint}
              onChange={(e) => { setEndpoint(e.target.value); clearProbe(); }}
              placeholder="https://api.anthropic.com"
              className="font-mono text-xs"
            />
          </div>
        </>
      )}

      {type === "openai-compatible" && (
        <div className="space-y-1.5">
          <Label htmlFor="prov-apikey-oai" className="text-xs">
            API Key <span className="text-muted-foreground">(optional)</span>
          </Label>
          <Input
            id="prov-apikey-oai"
            type="password"
            value={apiKey}
            onChange={(e) => { setApiKey(e.target.value); clearProbe(); }}
            placeholder={provider ? "••••••••• (unchanged)" : "sk-..."}
          />
        </div>
      )}

      {type === "bedrock" && (
        <>
          <div className="space-y-1.5">
            <Label className="text-xs">AWS Region</Label>
            <RegionCombobox
              value={awsRegion}
              onSelect={(v) => { setAwsRegion(v); clearProbe(); }}
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="prov-aws-key" className="text-xs">Access Key ID</Label>
            <Input
              id="prov-aws-key"
              type="password"
              value={awsAccessKeyId}
              onChange={(e) => { setAwsAccessKeyId(e.target.value); clearProbe(); }}
              placeholder={provider ? "••••••••• (unchanged)" : "AKIA..."}
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="prov-aws-secret" className="text-xs">Secret Access Key</Label>
            <Input
              id="prov-aws-secret"
              type="password"
              value={awsSecretAccessKey}
              onChange={(e) => { setAwsSecretAccessKey(e.target.value); clearProbe(); }}
              placeholder={provider ? "••••••••• (unchanged)" : ""}
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="prov-aws-token" className="text-xs">
              Session Token <span className="text-muted-foreground">(optional, for temporary credentials)</span>
            </Label>
            <Input
              id="prov-aws-token"
              type="password"
              value={awsSessionToken}
              onChange={(e) => { setAwsSessionToken(e.target.value); clearProbe(); }}
              placeholder={provider ? "••••••••• (unchanged)" : ""}
            />
          </div>
        </>
      )}

      {/* Test connection */}
      <div className="space-y-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={handleProbe}
          disabled={probing}
          className="gap-1.5"
        >
          {probing ? (
            <Loader2 size={13} className="animate-spin" />
          ) : probeResult?.reachable ? (
            <Wifi size={13} className="text-green-400" />
          ) : probeResult && !probeResult.reachable ? (
            <WifiOff size={13} className="text-destructive" />
          ) : (
            <Wifi size={13} />
          )}
          Test Connection
        </Button>
        {probeResult && (
          <p className="text-xs">
            {probeResult.reachable ? (
              <span className="text-green-400">
                Connected — {probeResult.models.length} model
                {probeResult.models.length !== 1 ? "s" : ""} found
              </span>
            ) : (
              <span className="text-destructive">
                {probeResult.error || "Unreachable"}
              </span>
            )}
          </p>
        )}
      </div>

      <Separator />

      <div className="flex items-center gap-2">
        <Button type="submit" size="sm" disabled={!canSave} className="gap-1.5">
          {saving ? <Loader2 size={13} className="animate-spin" /> : <Save size={13} />}
          Save
        </Button>
        <Button type="button" variant="ghost" size="sm" onClick={onCancel}>
          Cancel
        </Button>
        {provider && onDelete && (
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
