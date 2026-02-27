import { type FC, useEffect, useState } from "react";
import {
  Loader2Icon,
  SearchIcon,
  ToggleLeftIcon,
  ToggleRightIcon,
} from "lucide-react";
import {
  fetchLspSettings,
  toggleLspServer,
  updateLspSettings,
  detectLspServers,
  type LspSettingsResponse,
} from "../../api/client";
import { cn } from "../../lib/utils";

export const LspTab: FC = () => {
  const [data, setData] = useState<LspSettingsResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [detecting, setDetecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchLspSettings()
      .then(setData)
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, []);

  const handleGlobalToggle = async () => {
    if (!data) return;
    try {
      const updated = await updateLspSettings({ enabled: !data.enabled });
      setData(updated);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleServerToggle = async (id: string, enabled: boolean) => {
    if (!data) return;
    try {
      await toggleLspServer(id, enabled);
      setData({
        ...data,
        servers: data.servers.map((s) =>
          s.id === id ? { ...s, enabled } : s,
        ),
      });
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDetect = async () => {
    setDetecting(true);
    setError(null);
    try {
      const updated = await detectLspServers();
      setData(updated);
    } catch (e) {
      setError(String(e));
    } finally {
      setDetecting(false);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12 text-default-400">
        <Loader2Icon className="size-4 animate-spin" />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-sm font-medium text-foreground">
            Language Servers
          </h3>
          <p className="text-[11px] text-default-400 mt-0.5">
            LSP servers provide diagnostics for file operations.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={handleDetect}
            disabled={detecting}
            className="flex items-center gap-1 px-2 py-1 text-xs font-medium rounded-md bg-default-200/50 text-default-700 hover:bg-default-200 transition-colors disabled:opacity-50"
          >
            {detecting ? (
              <Loader2Icon className="size-3 animate-spin" />
            ) : (
              <SearchIcon className="size-3" />
            )}
            {detecting ? "Scanning..." : "Re-detect"}
          </button>
        </div>
      </div>

      {error && (
        <p className="text-xs text-danger">{error}</p>
      )}

      {/* Global toggle */}
      {data && (
        <div className="flex items-center justify-between rounded-lg border border-default-200/50 bg-default-50/30 p-3">
          <div>
            <div className="text-sm font-medium text-foreground">
              LSP Integration
            </div>
            <div className="text-[11px] text-default-400 mt-0.5">
              {data.enabled
                ? "Diagnostics will be appended to file tool results"
                : "Diagnostics are disabled globally"}
            </div>
          </div>
          <button
            onClick={handleGlobalToggle}
            className="text-default-500 hover:text-foreground transition-colors"
          >
            {data.enabled ? (
              <ToggleRightIcon className="size-6 text-primary" />
            ) : (
              <ToggleLeftIcon className="size-6" />
            )}
          </button>
        </div>
      )}

      {/* Server list */}
      {data && data.servers.length === 0 && (
        <p className="text-xs text-default-400">
          No language servers detected. Click Re-detect to scan your PATH.
        </p>
      )}

      {data?.servers.map((server) => (
        <div
          key={server.id}
          className={cn(
            "rounded-lg border border-default-200/50 bg-default-50/30 p-3",
            !data.enabled && "opacity-50",
          )}
        >
          <div className="flex items-center gap-3">
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-2">
                <span className="text-sm font-medium text-foreground">
                  {server.name}
                </span>
                {server.auto_detected && (
                  <span className="text-[10px] px-1.5 py-0.5 rounded bg-default-200/50 text-default-400">
                    auto-detected
                  </span>
                )}
              </div>
              <div className="text-[11px] text-default-400 mt-0.5 font-mono truncate">
                {server.command}
                {server.args.length > 0 && ` ${server.args.join(" ")}`}
              </div>
              <div className="flex gap-1 mt-1.5">
                {server.language_ids.map((lang) => (
                  <span
                    key={lang}
                    className="text-[10px] px-1.5 py-0.5 rounded-md bg-primary/10 text-primary font-medium"
                  >
                    {lang}
                  </span>
                ))}
              </div>
            </div>
            <button
              onClick={() => handleServerToggle(server.id, !server.enabled)}
              disabled={!data.enabled}
              className="text-default-500 hover:text-foreground transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {server.enabled ? (
                <ToggleRightIcon className="size-6 text-primary" />
              ) : (
                <ToggleLeftIcon className="size-6" />
              )}
            </button>
          </div>
        </div>
      ))}
    </div>
  );
};
