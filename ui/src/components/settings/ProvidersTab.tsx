import { type FC, useState } from "react";
import {
  PlusIcon,
  TrashIcon,
  CheckCircleIcon,
  XCircleIcon,
  Loader2Icon,
} from "lucide-react";
import { Select, SelectItem } from "@heroui/react";
import { useProviderStore } from "../../stores/providerStore";
import type {
  ProviderPublic,
  CreateProviderRequest,
  ProviderType,
} from "../../api/client";
import { cn } from "../../lib/utils";

type EditorMode = { type: "closed" } | { type: "create" } | { type: "edit"; provider: ProviderPublic };

export const ProvidersTab: FC = () => {
  const { providers, deleteProvider } = useProviderStore();
  const [mode, setMode] = useState<EditorMode>({ type: "closed" });

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-medium text-foreground">Providers</h3>
        <button
          onClick={() => setMode({ type: "create" })}
          className="flex items-center gap-1 px-2 py-1 text-xs font-medium rounded-md bg-primary text-white hover:bg-primary/90 transition-colors"
        >
          <PlusIcon className="size-3" />
          Add
        </button>
      </div>

      {providers.length === 0 && mode.type === "closed" && (
        <p className="text-xs text-default-400">
          No providers configured. Add one to get started.
        </p>
      )}

      {providers.map((p) => (
        <div
          key={p.id}
          className="flex items-center gap-3 p-3 rounded-lg border border-default-200/50 bg-default-50/30"
        >
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2">
              <span className="text-sm font-medium text-foreground truncate">
                {p.name}
              </span>
              <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-default-200/50 text-default-500 uppercase tracking-wide">
                {p.type}
              </span>
            </div>
            <div className="text-[11px] text-default-400 mt-0.5">
              {p.type === "anthropic" && (p.endpoint ?? "api.anthropic.com")}
              {p.type === "bedrock" && (p.aws_region ?? "no region")}
              {p.has_api_key && " · Key set"}
              {p.has_aws_credentials && " · AWS creds set"}
            </div>
          </div>
          <button
            onClick={() => setMode({ type: "edit", provider: p })}
            className="text-[11px] text-default-500 hover:text-foreground px-2 py-1 rounded hover:bg-default-200/40 transition-colors"
          >
            Edit
          </button>
          <button
            onClick={() => {
              if (confirm(`Delete provider "${p.name}"?`)) {
                deleteProvider(p.id);
              }
            }}
            className="text-default-400 hover:text-danger p-1 rounded hover:bg-danger/10 transition-colors"
          >
            <TrashIcon className="size-3.5" />
          </button>
        </div>
      ))}

      {mode.type !== "closed" && (
        <ProviderEditor
          provider={mode.type === "edit" ? mode.provider : undefined}
          onClose={() => setMode({ type: "closed" })}
        />
      )}
    </div>
  );
};

const ProviderEditor: FC<{
  provider?: ProviderPublic;
  onClose: () => void;
}> = ({ provider, onClose }) => {
  const { createProvider, updateProvider, testProvider } = useProviderStore();
  const isEdit = !!provider;

  const [name, setName] = useState(provider?.name ?? "");
  const [type, setType] = useState<ProviderType>(provider?.type ?? "anthropic");
  const [endpoint, setEndpoint] = useState(provider?.endpoint ?? "");
  const [apiKey, setApiKey] = useState("");
  const [awsRegion, setAwsRegion] = useState(provider?.aws_region ?? "us-east-1");
  const [awsAccessKeyId, setAwsAccessKeyId] = useState("");
  const [awsSecretAccessKey, setAwsSecretAccessKey] = useState("");
  const [awsSessionToken, setAwsSessionToken] = useState("");
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{
    ok: boolean;
    error?: string;
  } | null>(null);

  const handleSave = async () => {
    setSaving(true);
    try {
      const data: CreateProviderRequest = {
        name,
        type,
        ...(endpoint ? { endpoint } : {}),
        ...(apiKey ? { api_key: apiKey } : {}),
        ...(type === "bedrock"
          ? {
              aws_region: awsRegion,
              ...(awsAccessKeyId ? { aws_access_key_id: awsAccessKeyId } : {}),
              ...(awsSecretAccessKey
                ? { aws_secret_access_key: awsSecretAccessKey }
                : {}),
              ...(awsSessionToken
                ? { aws_session_token: awsSessionToken }
                : {}),
            }
          : {}),
      };

      if (isEdit) {
        await updateProvider(provider!.id, data);
      } else {
        await createProvider(data);
      }
      onClose();
    } finally {
      setSaving(false);
    }
  };

  const handleTest = async () => {
    if (!isEdit) return;
    setTesting(true);
    setTestResult(null);
    try {
      const result = await testProvider(provider!.id);
      setTestResult(result);
    } finally {
      setTesting(false);
    }
  };

  return (
    <div className="rounded-lg border border-primary/30 bg-primary/5 p-4 space-y-3">
      <h4 className="text-xs font-semibold text-foreground">
        {isEdit ? "Edit Provider" : "New Provider"}
      </h4>

      <div className="grid grid-cols-2 gap-3">
        <Field label="Name">
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="My Provider"
            className="input-field"
          />
        </Field>

        <Field label="Type">
          <Select
            aria-label="Type"
            size="sm"
            variant="bordered"
            isDisabled={isEdit}
            selectedKeys={[type]}
            onSelectionChange={(keys) => {
              const key = [...keys][0] as ProviderType | undefined;
              if (key) setType(key);
            }}
            classNames={{
              trigger: "input-field !h-auto",
              value: "text-xs",
            }}
          >
            <SelectItem key="anthropic">Anthropic</SelectItem>
            <SelectItem key="bedrock">AWS Bedrock</SelectItem>
          </Select>
        </Field>
      </div>

      {type === "anthropic" && (
        <>
          <Field label="API Key">
            <input
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder={isEdit && provider?.has_api_key ? "••••••••" : "sk-ant-..."}
              className="input-field"
            />
          </Field>
          <Field label="Endpoint (optional)">
            <input
              value={endpoint}
              onChange={(e) => setEndpoint(e.target.value)}
              placeholder="https://api.anthropic.com"
              className="input-field"
            />
          </Field>
        </>
      )}

      {type === "bedrock" && (
        <>
          <Field label="AWS Region">
            <input
              value={awsRegion}
              onChange={(e) => setAwsRegion(e.target.value)}
              placeholder="us-east-1"
              className="input-field"
            />
          </Field>
          <Field label="AWS Access Key ID (optional)">
            <input
              type="password"
              value={awsAccessKeyId}
              onChange={(e) => setAwsAccessKeyId(e.target.value)}
              placeholder={
                isEdit && provider?.has_aws_credentials ? "••••••••" : "AKIA..."
              }
              className="input-field"
            />
          </Field>
          <Field label="AWS Secret Access Key (optional)">
            <input
              type="password"
              value={awsSecretAccessKey}
              onChange={(e) => setAwsSecretAccessKey(e.target.value)}
              placeholder={
                isEdit && provider?.has_aws_credentials ? "••••••••" : ""
              }
              className="input-field"
            />
          </Field>
          <Field label="AWS Session Token (optional)">
            <input
              type="password"
              value={awsSessionToken}
              onChange={(e) => setAwsSessionToken(e.target.value)}
              className="input-field"
            />
          </Field>
        </>
      )}

      {testResult && (
        <div
          className={cn(
            "flex items-center gap-2 text-xs p-2 rounded",
            testResult.ok
              ? "bg-success/10 text-success"
              : "bg-danger/10 text-danger",
          )}
        >
          {testResult.ok ? (
            <CheckCircleIcon className="size-3.5" />
          ) : (
            <XCircleIcon className="size-3.5" />
          )}
          {testResult.ok ? "Connection successful" : testResult.error}
        </div>
      )}

      <div className="flex items-center gap-2 pt-1">
        <button
          onClick={handleSave}
          disabled={!name || saving}
          className="px-3 py-1.5 text-xs font-medium rounded-md bg-primary text-white hover:bg-primary/90 disabled:opacity-50 transition-colors"
        >
          {saving ? "Saving..." : isEdit ? "Update" : "Create"}
        </button>
        {isEdit && (
          <button
            onClick={handleTest}
            disabled={testing}
            className="flex items-center gap-1 px-3 py-1.5 text-xs font-medium rounded-md border border-default-200 hover:bg-default-100 transition-colors"
          >
            {testing && <Loader2Icon className="size-3 animate-spin" />}
            Test Connection
          </button>
        )}
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
