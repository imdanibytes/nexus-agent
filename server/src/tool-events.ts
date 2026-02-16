import { setToolsChangedHandler } from "./mcp-client.js";
import { invalidateMcpToolCache } from "./tools/handlers/remote.js";
import { broadcast } from "./ws-handler.js";

/** Start listening for MCP tool changes from the Nexus Host API. */
export function startToolEventListener(): void {
  setToolsChangedHandler(() => {
    invalidateMcpToolCache();
    broadcast("tools_changed");
  });
}
