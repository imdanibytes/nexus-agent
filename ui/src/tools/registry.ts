import { respondToUiSurface } from "../api/client.js";

type ToolHandler = (input: Record<string, unknown>) => Promise<string>;

const handlers: Record<string, ToolHandler> = {
  _nexus_clipboard_read: async () => {
    try {
      const text = await navigator.clipboard.readText();
      return text;
    } catch {
      return "Clipboard access denied";
    }
  },

  _nexus_clipboard_write: async (input) => {
    try {
      await navigator.clipboard.writeText(input.text as string);
      return "Copied to clipboard";
    } catch {
      return "Clipboard write denied";
    }
  },
};

export async function handleBrowserTool(
  toolUseId: string,
  name: string,
  input: Record<string, unknown>
): Promise<void> {
  const handler = handlers[name];
  if (!handler) return;

  const result = await handler(input);
  await respondToUiSurface(toolUseId, "accept", result);
}

export function isBrowserTool(name: string): boolean {
  return name in handlers;
}
