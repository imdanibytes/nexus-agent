import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import { ToolListChangedNotificationSchema } from "@modelcontextprotocol/sdk/types.js";
import { getAccessToken } from "./auth.js";

const NEXUS_HOST_URL =
  process.env.NEXUS_HOST_URL || "http://host.docker.internal:9600";

let client: Client | null = null;
let toolsChangedHandler: (() => void) | null = null;

export function setToolsChangedHandler(handler: () => void): void {
  toolsChangedHandler = handler;
}

export async function getMcpClient(): Promise<Client> {
  if (client) return client;

  const token = await getAccessToken();

  const transport = new StreamableHTTPClientTransport(
    new URL(`${NEXUS_HOST_URL}/mcp`),
    {
      requestInit: {
        headers: {
          Authorization: `Bearer ${token}`,
        },
      },
    },
  );

  const c = new Client({ name: "nexus-agent", version: "1.0.0" });

  c.setNotificationHandler(ToolListChangedNotificationSchema, async () => {
    toolsChangedHandler?.();
  });

  transport.onclose = () => {
    client = null;
  };

  transport.onerror = () => {
    client = null;
  };

  await c.connect(transport);
  client = c;
  return client;
}

export async function closeMcpClient(): Promise<void> {
  if (client) {
    try {
      await client.close();
    } catch {
      // Already closed
    }
    client = null;
  }
}
