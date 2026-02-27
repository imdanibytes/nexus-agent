import type { AppBridge } from "@modelcontextprotocol/ext-apps/app-bridge";

export interface McpAppInstance {
  id: string;
  serverId: string;
  resourceUri: string;
  bridge: AppBridge;
  iframe: HTMLIFrameElement;
}

export interface McpAppMountOptions {
  serverId: string;
  resourceUri: string;
  container: HTMLElement;
  conversationId: string;
  toolCallData?: unknown;
}
