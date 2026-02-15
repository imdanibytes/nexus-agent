/**
 * Singleton WebSocket client with auto-reconnect and typed event dispatch.
 */

export interface WsMessage {
  type: string;
  conversationId?: string;
  data?: Record<string, unknown>;
  requestId?: string;
}

type MessageHandler = (msg: WsMessage) => void;

const RECONNECT_BASE = 1_000;
const RECONNECT_CAP = 30_000;

class WsClient {
  private ws: WebSocket | null = null;
  private url = "";
  private handlers = new Map<string, Set<MessageHandler>>();
  private wildcardHandlers = new Set<MessageHandler>();
  private reconnectMs = RECONNECT_BASE;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private intentionalClose = false;

  connect(): void {
    if (this.ws) return;
    const proto = location.protocol === "https:" ? "wss:" : "ws:";
    this.url = `${proto}//${location.host}/ws`;
    this.intentionalClose = false;
    this.open();
  }

  disconnect(): void {
    this.intentionalClose = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.ws?.close();
    this.ws = null;
  }

  /** Register a handler for a specific message type */
  on(type: string, handler: MessageHandler): () => void {
    let set = this.handlers.get(type);
    if (!set) {
      set = new Set();
      this.handlers.set(type, set);
    }
    set.add(handler);
    return () => set!.delete(handler);
  }

  /** Register a handler that receives ALL messages */
  onAny(handler: MessageHandler): () => void {
    this.wildcardHandlers.add(handler);
    return () => this.wildcardHandlers.delete(handler);
  }

  send(msg: WsMessage): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(msg));
    }
  }

  get connected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  private open(): void {
    const ws = new WebSocket(this.url);

    ws.addEventListener("open", () => {
      this.reconnectMs = RECONNECT_BASE;
    });

    ws.addEventListener("message", (evt) => {
      try {
        const msg = JSON.parse(evt.data as string) as WsMessage;
        this.dispatch(msg);
      } catch {
        // Ignore malformed messages
      }
    });

    ws.addEventListener("close", () => {
      this.ws = null;
      if (!this.intentionalClose) {
        this.scheduleReconnect();
      }
    });

    ws.addEventListener("error", () => {
      // Error events are followed by close events, so we reconnect there
    });

    this.ws = ws;
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer) return;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.open();
    }, this.reconnectMs);
    this.reconnectMs = Math.min(this.reconnectMs * 2, RECONNECT_CAP);
  }

  private dispatch(msg: WsMessage): void {
    // Type-specific handlers
    const handlers = this.handlers.get(msg.type);
    if (handlers) {
      for (const h of handlers) h(msg);
    }

    // Wildcard handlers
    for (const h of this.wildcardHandlers) h(msg);
  }
}

/** Singleton WebSocket client */
export const wsClient = new WsClient();
