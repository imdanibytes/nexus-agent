import {
  AppBridge,
  PostMessageTransport,
  buildAllowAttribute,
} from "@modelcontextprotocol/ext-apps/app-bridge";
import type { McpUiHostContext } from "@modelcontextprotocol/ext-apps/app-bridge";
import { readMcpResource, invokeToolCall } from "../api/client";
import { useUIStore } from "../stores/uiStore";
import type { McpAppInstance, McpAppMountOptions } from "../types/mcp-apps";

const HOST_INFO = { name: "Nexus", version: "0.1.0" } as const;

const HOST_CAPABILITIES = {
  openLinks: {},
  serverTools: {},
  logging: {},
} as const;

function resolveTheme(): "light" | "dark" {
  const theme = useUIStore.getState().theme;
  if (theme !== "system") return theme;
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

class McpAppService {
  private instances = new Map<string, McpAppInstance>();

  async mount(opts: McpAppMountOptions): Promise<string> {
    const { serverId, resourceUri, container, conversationId, toolCallData } =
      opts;
    const id = crypto.randomUUID();

    // Fetch the HTML resource from the MCP server via the daemon
    const contents = await readMcpResource(serverId, resourceUri);
    const html = contents.find((c) => c.text)?.text;
    if (!html) throw new Error(`No HTML content for resource ${resourceUri}`);

    // Create sandboxed iframe
    const iframe = document.createElement("iframe");
    iframe.setAttribute("sandbox", "allow-scripts allow-forms");
    iframe.setAttribute(
      "allow",
      buildAllowAttribute(undefined),
    );
    iframe.style.border = "none";
    iframe.style.width = "100%";
    iframe.srcdoc = html;
    container.appendChild(iframe);

    // Wait for iframe to load before connecting transport
    await new Promise<void>((resolve) => {
      iframe.addEventListener("load", () => resolve(), { once: true });
    });

    const hostContext: McpUiHostContext = {
      theme: resolveTheme(),
      containerDimensions: {
        width: container.clientWidth,
        maxHeight: container.clientHeight || 600,
      },
    };

    // Create bridge without MCP client — we proxy tool calls manually
    const bridge = new AppBridge(null, HOST_INFO, HOST_CAPABILITIES, {
      hostContext,
    });

    // Size changes from the app
    bridge.onsizechange = ({ width, height }) => {
      if (width != null) iframe.style.width = `${width}px`;
      if (height != null) iframe.style.height = `${height}px`;
    };

    // Open external links
    bridge.onopenlink = async ({ url }) => {
      window.open(url, "_blank", "noopener,noreferrer");
      return {};
    };

    // Logging
    bridge.onloggingmessage = ({ level, logger, data }) => {
      const method = level === "error" ? "error" : "log";
      console[method](`[MCP App ${logger ?? id}]`, data);
    };

    // Tool call proxy — route through daemon REST API
    bridge.oncalltool = async (params) => {
      try {
        await invokeToolCall(conversationId, params.name, params.arguments ?? {});
        return { content: [] };
      } catch (err) {
        return {
          content: [{ type: "text" as const, text: String(err) }],
          isError: true,
        };
      }
    };

    // Display mode (log only for now)
    bridge.onrequestdisplaymode = async ({ mode }) => {
      console.log(`[MCP App ${id}] requested display mode: ${mode}`);
      return { mode: "inline" };
    };

    // Connect the postMessage transport
    const transport = new PostMessageTransport(
      iframe.contentWindow!,
      iframe.contentWindow!,
    );

    bridge.oninitialized = () => {
      if (toolCallData) {
        bridge.sendToolInput({ arguments: toolCallData as Record<string, unknown> });
      }
    };

    await bridge.connect(transport);

    const instance: McpAppInstance = {
      id,
      serverId,
      resourceUri,
      bridge,
      iframe,
    };
    this.instances.set(id, instance);

    return id;
  }

  async unmount(instanceId: string): Promise<void> {
    const instance = this.instances.get(instanceId);
    if (!instance) return;

    try {
      await instance.bridge.teardownResource({});
    } catch {
      // Best-effort teardown
    }
    await instance.bridge.close();
    instance.iframe.remove();
    this.instances.delete(instanceId);
  }

  async unmountAll(): Promise<void> {
    const ids = [...this.instances.keys()];
    await Promise.all(ids.map((id) => this.unmount(id)));
  }

  broadcastContextChange(): void {
    const ctx: McpUiHostContext = { theme: resolveTheme() };
    for (const instance of this.instances.values()) {
      instance.bridge.setHostContext(ctx);
    }
  }

  getInstance(instanceId: string): McpAppInstance | undefined {
    return this.instances.get(instanceId);
  }
}

export const mcpAppService = new McpAppService();
