type Args = Record<string, unknown>;

function basename(path: string): string {
  const parts = path.split("/");
  return parts[parts.length - 1] || path;
}

function shortenPath(path: string, maxLen = 40): string {
  if (path.length <= maxLen) return path;
  const parts = path.split("/").filter(Boolean);
  if (parts.length <= 2) return path;
  return `.../${parts.slice(-2).join("/")}`;
}

function tryParseArgs(argsText?: string): Args | null {
  if (!argsText) return null;
  try {
    return JSON.parse(argsText) as Args;
  } catch {
    return null;
  }
}

type Formatter = (args: Args) => string | null;

const TOOL_FORMATTERS: Record<string, Formatter> = {
  read_file: (a) => {
    const p = a.path as string | undefined;
    return p ? `Read ${basename(p)}` : null;
  },
  write_file: (a) => {
    const p = a.path as string | undefined;
    return p ? `Write ${basename(p)}` : null;
  },
  edit_file: (a) => {
    const p = a.path as string | undefined;
    return p ? `Edit ${basename(p)}` : null;
  },
  search_files: (a) => {
    const pattern = a.pattern as string | undefined;
    const path = a.path as string | undefined;
    if (pattern && path) return `Search for ${pattern} in ${shortenPath(path)}`;
    if (pattern) return `Search for ${pattern}`;
    return null;
  },
  list_directory: (a) => {
    const p = a.path as string | undefined;
    return p ? `List ${shortenPath(p)}` : null;
  },
  execute_shell: (a) => {
    const cmd = a.command as string | undefined;
    return cmd ? `Run ${cmd}` : null;
  },
  execute_command: (a) => {
    const cmd = a.command as string | undefined;
    const args = a.args as string[] | undefined;
    if (cmd && args?.length) return `Run ${cmd} ${args.join(" ")}`;
    return cmd ? `Run ${cmd}` : null;
  },
  web_search: (a) => {
    const q = a.query as string | undefined;
    return q ? `Search web: "${q}"` : null;
  },
  fetch_url: (a) => {
    const url = a.url as string | undefined;
    return url ? `Fetch ${url}` : null;
  },
};

export function formatToolDescription(
  toolName: string,
  argsText?: string,
): string {
  const args = tryParseArgs(argsText);

  const formatter = TOOL_FORMATTERS[toolName];
  if (formatter && args) {
    const result = formatter(args);
    if (result) return result;
  }

  const stripped = toolName.replace(/^(?:mcp__|mcp_|_nexus_|nexus_)/, "");
  const strippedFormatter = TOOL_FORMATTERS[stripped];
  if (strippedFormatter && args) {
    const result = strippedFormatter(args);
    if (result) return result;
  }

  return humanizeToolName(toolName);
}

function humanizeToolName(name: string): string {
  const clean = name
    .replace(/^mcp__[^_]+__/, "")
    .replace(/^nexus_/, "")
    .replace(/^_nexus_/, "");

  return clean
    .split("_")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}
