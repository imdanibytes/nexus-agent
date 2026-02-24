import { type FC, useState } from "react";
import {
  PlusIcon,
  TrashIcon,
  CheckCircleIcon,
  XCircleIcon,
  Loader2Icon,
  ChevronDownIcon,
  ChevronRightIcon,
} from "lucide-react";
import { useMcpStore } from "../../stores/mcpStore";
import { testMcpServerInline, type McpServerConfig, type CreateMcpServerRequest } from "../../api/client";
import { cn } from "../../lib/utils";

type EditorMode =
  | { type: "closed" }
  | { type: "create" }
  | { type: "edit"; server: McpServerConfig };

export const McpTab: FC = () => {
  const { servers, deleteServer } = useMcpStore();
  const [mode, setMode] = useState<EditorMode>({ type: "closed" });

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-sm font-medium text-foreground">MCP Servers</h3>
          <p className="text-[11px] text-default-400 mt-0.5">
            Model Context Protocol servers provide tools for agents.
          </p>
        </div>
        <button
          onClick={() => setMode({ type: "create" })}
          className="flex items-center gap-1 px-2 py-1 text-xs font-medium rounded-md bg-primary text-white hover:bg-primary/90 transition-colors"
        >
          <PlusIcon className="size-3" />
          Add
        </button>
      </div>

      {servers.length === 0 && mode.type === "closed" && (
        <p className="text-xs text-default-400">
          No MCP servers configured. Add one to give agents access to tools.
        </p>
      )}

      {servers.map((srv) => (
        <McpServerCard
          key={srv.id}
          server={srv}
          onEdit={() => setMode({ type: "edit", server: srv })}
          onDelete={async () => {
            if (confirm(`Delete MCP server "${srv.name}"?`)) {
              await deleteServer(srv.id);
            }
          }}
        />
      ))}

      {mode.type !== "closed" && (
        <McpServerEditor
          server={mode.type === "edit" ? mode.server : undefined}
          onClose={() => setMode({ type: "closed" })}
        />
      )}
    </div>
  );
};

const McpServerCard: FC<{
  server: McpServerConfig;
  onEdit: () => void;
  onDelete: () => void;
}> = ({ server, onEdit, onDelete }) => {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="rounded-lg border border-default-200/50 bg-default-50/30 overflow-hidden">
      <div className="flex items-center gap-3 p-3">
        <button
          onClick={() => setExpanded(!expanded)}
          className="text-default-400 hover:text-foreground transition-colors"
        >
          {expanded ? (
            <ChevronDownIcon className="size-3.5" />
          ) : (
            <ChevronRightIcon className="size-3.5" />
          )}
        </button>
        <div className="flex-1 min-w-0">
          <span className="text-sm font-medium text-foreground">
            {server.name}
          </span>
          <div className="text-[11px] text-default-400 mt-0.5 font-mono truncate">
            {server.command} {server.args.join(" ")}
          </div>
        </div>
        <button
          onClick={onEdit}
          className="text-[11px] text-default-500 hover:text-foreground px-2 py-1 rounded hover:bg-default-200/40 transition-colors"
        >
          Edit
        </button>
        <button
          onClick={onDelete}
          className="text-default-400 hover:text-danger p-1 rounded hover:bg-danger/10 transition-colors"
        >
          <TrashIcon className="size-3.5" />
        </button>
      </div>

      {expanded && (
        <div className="border-t border-default-200/30 px-3 py-2 space-y-2">
          <div>
            <div className="text-[10px] font-semibold uppercase tracking-wider text-default-400 mb-0.5">
              Command
            </div>
            <div className="text-[11px] font-mono text-default-600">
              {server.command}
            </div>
          </div>
          {server.args.length > 0 && (
            <div>
              <div className="text-[10px] font-semibold uppercase tracking-wider text-default-400 mb-0.5">
                Arguments
              </div>
              {server.args.map((arg, i) => (
                <div key={i} className="text-[11px] font-mono text-default-500">
                  {arg}
                </div>
              ))}
            </div>
          )}
          {Object.keys(server.env).length > 0 && (
            <div>
              <div className="text-[10px] font-semibold uppercase tracking-wider text-default-400 mb-0.5">
                Environment
              </div>
              {Object.entries(server.env).map(([k, v]) => (
                <div key={k} className="text-[11px] font-mono text-default-500">
                  <span className="text-default-600">{k}</span>=
                  <span className="text-default-400">
                    {k.toLowerCase().includes("key") || k.toLowerCase().includes("secret") || k.toLowerCase().includes("token")
                      ? "***"
                      : v}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
};

const McpServerEditor: FC<{
  server?: McpServerConfig;
  onClose: () => void;
}> = ({ server, onClose }) => {
  const { createServer, updateServer } = useMcpStore();
  const isEdit = !!server;

  const [name, setName] = useState(server?.name ?? "");
  const [command, setCommand] = useState(server?.command ?? "");
  const [argsText, setArgsText] = useState(
    server?.args.join("\n") ?? "",
  );
  const [envEntries, setEnvEntries] = useState<{ key: string; value: string }[]>(
    server
      ? Object.entries(server.env).map(([key, value]) => ({ key, value }))
      : [],
  );

  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{
    ok: boolean;
    tools?: number;
    tool_names?: string[];
    error?: string;
  } | null>(null);
  const [saving, setSaving] = useState(false);

  const verified = testResult?.ok === true;

  const clearVerified = () => setTestResult(null);

  const buildRequest = (): CreateMcpServerRequest => {
    const args = argsText
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean);
    const env: Record<string, string> = {};
    for (const { key, value } of envEntries) {
      if (key.trim()) env[key.trim()] = value;
    }
    return { name, command, args, env };
  };

  const handleTest = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const result = await testMcpServerInline(buildRequest());
      setTestResult(result);
    } catch (e) {
      setTestResult({ ok: false, error: String(e) });
    } finally {
      setTesting(false);
    }
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      const data = buildRequest();
      if (isEdit) {
        await updateServer(server!.id, data);
      } else {
        await createServer(data);
      }
      onClose();
    } finally {
      setSaving(false);
    }
  };

  const addEnvEntry = () => {
    setEnvEntries([...envEntries, { key: "", value: "" }]);
    clearVerified();
  };

  const updateEnvEntry = (idx: number, field: "key" | "value", val: string) => {
    setEnvEntries(envEntries.map((e, i) => (i === idx ? { ...e, [field]: val } : e)));
    clearVerified();
  };

  const removeEnvEntry = (idx: number) => {
    setEnvEntries(envEntries.filter((_, i) => i !== idx));
    clearVerified();
  };

  const canSave = name.trim() && command.trim() && verified;

  return (
    <div className="rounded-lg border border-primary/30 bg-primary/5 p-4 space-y-3">
      <h4 className="text-xs font-semibold text-foreground">
        {isEdit ? "Edit MCP Server" : "New MCP Server"}
      </h4>

      <div className="grid grid-cols-2 gap-3">
        <Field label="Name">
          <input
            value={name}
            onChange={(e) => { setName(e.target.value); clearVerified(); }}
            placeholder="filesystem"
            className="input-field"
          />
        </Field>

        <Field label="Command">
          <input
            value={command}
            onChange={(e) => { setCommand(e.target.value); clearVerified(); }}
            placeholder="npx"
            className="input-field"
          />
        </Field>
      </div>

      <Field label="Arguments (one per line)">
        <textarea
          value={argsText}
          onChange={(e) => { setArgsText(e.target.value); clearVerified(); }}
          rows={3}
          placeholder={"@modelcontextprotocol/server-filesystem\n/path/to/allowed/dir"}
          className="input-field resize-y min-h-[60px] font-mono text-[11px]"
        />
      </Field>

      <div>
        <div className="flex items-center justify-between mb-1">
          <label className="text-[11px] text-default-500">
            Environment Variables
          </label>
          <button
            onClick={addEnvEntry}
            className="text-[10px] text-primary hover:text-primary/80 transition-colors"
          >
            + Add variable
          </button>
        </div>
        {envEntries.length === 0 && (
          <p className="text-[10px] text-default-400 italic">
            No environment variables
          </p>
        )}
        {envEntries.map((entry, idx) => (
          <div key={idx} className="flex gap-2 mb-1">
            <input
              value={entry.key}
              onChange={(e) => updateEnvEntry(idx, "key", e.target.value)}
              placeholder="KEY"
              className="input-field flex-1 font-mono text-[11px]"
            />
            <input
              value={entry.value}
              onChange={(e) => updateEnvEntry(idx, "value", e.target.value)}
              placeholder="value"
              className="input-field flex-[2] font-mono text-[11px]"
            />
            <button
              onClick={() => removeEnvEntry(idx)}
              className="text-default-400 hover:text-danger p-1 transition-colors"
            >
              <XCircleIcon className="size-3.5" />
            </button>
          </div>
        ))}
      </div>

      {/* Test / status */}
      <div className="flex items-center gap-2">
        <button
          onClick={handleTest}
          disabled={!name.trim() || !command.trim() || testing}
          className={cn(
            "px-3 py-1.5 text-xs font-medium rounded-md transition-colors",
            verified
              ? "bg-success/10 text-success border border-success/30"
              : "bg-default-200/50 text-default-700 hover:bg-default-200",
            "disabled:opacity-50",
          )}
        >
          {testing ? (
            <span className="flex items-center gap-1.5">
              <Loader2Icon className="size-3 animate-spin" />
              Testing...
            </span>
          ) : verified ? (
            <span className="flex items-center gap-1.5">
              <CheckCircleIcon className="size-3" />
              Verified ({testResult.tools} tools)
            </span>
          ) : (
            "Test Connection"
          )}
        </button>

        {testResult && !testResult.ok && (
          <span className="text-[11px] text-danger flex items-center gap-1">
            <XCircleIcon className="size-3" />
            {testResult.error || "Connection failed"}
          </span>
        )}
      </div>

      {/* Tool names preview */}
      {testResult?.ok && testResult.tool_names && testResult.tool_names.length > 0 && (
        <div className="text-[10px] text-default-400">
          Tools: {testResult.tool_names.join(", ")}
        </div>
      )}

      <div className="flex items-center gap-2 pt-1">
        <button
          onClick={handleSave}
          disabled={!canSave || saving}
          className={cn(
            "px-3 py-1.5 text-xs font-medium rounded-md transition-colors disabled:opacity-50",
            canSave
              ? "bg-primary text-white hover:bg-primary/90"
              : "bg-default-200 text-default-500",
          )}
        >
          {saving ? "Saving..." : isEdit ? "Update" : "Create"}
        </button>
        <button
          onClick={onClose}
          className="px-3 py-1.5 text-xs text-default-500 hover:text-foreground transition-colors"
        >
          Cancel
        </button>
        {!verified && name.trim() && command.trim() && (
          <span className="text-[10px] text-default-400">
            Test connection before saving
          </span>
        )}
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
