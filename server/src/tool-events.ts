import { getAccessToken } from "./auth.js";
import { invalidateMcpToolCache } from "./tools/handlers/remote.js";
import { broadcast } from "./ws-handler.js";

const NEXUS_HOST_URL =
  process.env.NEXUS_HOST_URL || "http://host.docker.internal:9600";

// ── Nexus Host API SSE subscription ──

let connected = false;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let backoffMs = 5_000;
const MAX_BACKOFF = 60_000;

async function subscribe(): Promise<void> {
  if (connected) return;

  try {
    const token = await getAccessToken();
    const res = await fetch(`${NEXUS_HOST_URL}/api/v1/mcp/events`, {
      headers: { Authorization: `Bearer ${token}` },
    });

    if (!res.ok || !res.body) {
      if (res.status !== 401 && res.status !== 403) {
        console.error(`MCP events: HTTP ${res.status}`);
      }
      return;
    }

    connected = true;
    backoffMs = 5_000;
    console.log("Subscribed to Nexus MCP tool events");

    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";

    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() || "";

        let currentEvent = "";
        for (const line of lines) {
          if (line.startsWith("event: ")) {
            currentEvent = line.slice(7).trim();
          } else if (line.startsWith("data: ")) {
            if (currentEvent === "tools_changed") {
              invalidateMcpToolCache();
              broadcast("tools_changed");
            }
            currentEvent = "";
          }
        }
      }
    } finally {
      reader.releaseLock();
    }
  } catch {
    // Network error — host not reachable yet, will retry
  }

  connected = false;
  scheduleReconnect();
}

function scheduleReconnect(): void {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    subscribe();
  }, backoffMs);
  backoffMs = Math.min(backoffMs * 2, MAX_BACKOFF);
}

/** Start listening for MCP tool changes from the Nexus Host API. */
export function startToolEventListener(): void {
  setTimeout(subscribe, 3_000);
}
