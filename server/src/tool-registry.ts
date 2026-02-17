import Anthropic from "@anthropic-ai/sdk";
import { ToolExecutor } from "./tools/executor.js";
import type { ToolDefinition } from "./tools/types.js";
import type { ToolFilter } from "./types.js";
import { setTitleTool } from "./tools/handlers/local.js";
import { fetchMcpToolHandlers } from "./tools/handlers/remote.js";

export interface ToolRegistry {
  executor: ToolExecutor;
  definitions: ToolDefinition[];
  anthropicTools: Anthropic.Tool[];
  wireName(name: string): string;
  origName(name: string): string;
}

/** Match a tool name against a glob pattern (supports * and ? wildcards). */
function matchGlob(pattern: string, name: string): boolean {
  const regex = new RegExp(
    "^" + pattern.replace(/\*/g, ".*").replace(/\?/g, ".") + "$",
  );
  return regex.test(name);
}

/** Apply tool filters (global + agent-level) to a list of tool definitions. */
function applyToolFilters(
  defs: ToolDefinition[],
  globalFilter?: ToolFilter,
  agentFilter?: ToolFilter,
): ToolDefinition[] {
  let result = defs;

  if (globalFilter) {
    if (globalFilter.mode === "allow") {
      result = result.filter((d) =>
        globalFilter.tools.some((t) => matchGlob(t, d.name)),
      );
    } else {
      result = result.filter(
        (d) => !globalFilter.tools.some((t) => matchGlob(t, d.name)),
      );
    }
  }

  if (agentFilter) {
    if (agentFilter.mode === "allow") {
      result = result.filter((d) =>
        agentFilter.tools.some((t) => matchGlob(t, d.name)),
      );
    } else {
      result = result.filter(
        (d) => !agentFilter.tools.some((t) => matchGlob(t, d.name)),
      );
    }
  }

  return result;
}

/**
 * Build a ToolRegistry for a turn.
 *
 * The executor is rebuilt each call. This is cheap because `fetchMcpToolHandlers()`
 * has its own 30s TTL cache — the network call only fires when the cache expires.
 * Rebuilding per-turn keeps the executor coherent with MCP tool changes without
 * needing a separate invalidation mechanism.
 */
export async function getToolRegistry(
  globalFilter?: ToolFilter,
  agentFilter?: ToolFilter,
  frontendTools?: ToolDefinition[],
): Promise<ToolRegistry> {
  const executor = new ToolExecutor();
  executor.register(setTitleTool);
  executor.registerAll(await fetchMcpToolHandlers());

  const allDefs: ToolDefinition[] = [
    ...executor.definitions(),
    ...(frontendTools ?? []),
  ];

  const filtered = applyToolFilters(allDefs, globalFilter, agentFilter);

  // LLM APIs require tool names to match ^[a-zA-Z0-9_-]+$ — sanitize dots
  const toWire = new Map<string, string>();
  const toOrig = new Map<string, string>();

  for (const d of filtered) {
    if (d.name.includes(".")) {
      const sanitized = d.name.replace(/\./g, "__");
      toWire.set(d.name, sanitized);
      toOrig.set(sanitized, d.name);
    }
  }

  const anthropicTools: Anthropic.Tool[] = filtered.map((d) => ({
    name: toWire.get(d.name) ?? d.name,
    description: d.description,
    input_schema: d.input_schema as Anthropic.Tool["input_schema"],
  }));

  return {
    executor,
    definitions: filtered,
    anthropicTools,
    wireName: (name: string) => toWire.get(name) ?? name,
    origName: (name: string) => toOrig.get(name) ?? name,
  };
}
