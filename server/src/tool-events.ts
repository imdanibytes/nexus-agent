import { setToolsChangedHandler } from "./mcp-client.js";
import { invalidateMcpToolCache } from "./tools/handlers/remote.js";
import { hub } from "./sse-handler.js";
import { EventType } from "./ag-ui-types.js";

/** Start listening for MCP tool changes from the Nexus Host API. */
export function startToolEventListener(): void {
  setToolsChangedHandler(() => {
    invalidateMcpToolCache();
    hub.push({ type: EventType.CUSTOM, name: "tools_changed", value: {} });
  });
}
