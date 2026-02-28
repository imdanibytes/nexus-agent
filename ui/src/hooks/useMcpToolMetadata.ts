import { useEffect, useState } from "react";
import { fetchMcpToolMetadata, type McpToolUiMeta } from "../api/client";

/**
 * Fetches MCP tool UI metadata (tools with `_meta.ui.resourceUri`).
 * Returns a map of namespaced tool name → { serverId, resourceUri }.
 * Refetches when MCP servers are reloaded.
 */
export function useMcpToolMetadata(): Map<string, McpToolUiMeta> {
  const [meta, setMeta] = useState<Map<string, McpToolUiMeta>>(new Map());

  useEffect(() => {
    let cancelled = false;

    async function load() {
      try {
        const data = await fetchMcpToolMetadata();
        if (!cancelled) {
          setMeta(new Map(Object.entries(data)));
        }
      } catch {
        // MCP metadata is best-effort — don't break the UI
      }
    }

    load();
    return () => { cancelled = true; };
  }, []);

  return meta;
}
