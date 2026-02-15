import type { Server as HttpServer, IncomingMessage } from "node:http";
import { WebSocketServer, WebSocket } from "ws";
import { v4 as uuidv4 } from "uuid";
import { runAgentTurn, resolveFrontendToolResult } from "./agent.js";
import type { SseWriter } from "./types.js";
import type { WireMessage } from "./agent.js";

// ── Types ──

interface WsMessage {
  type: string;
  conversationId?: string;
  data?: Record<string, unknown>;
  requestId?: string;
}

interface ActiveTurn {
  conversationId: string;
  abort: AbortController;
}

// ── WsSseWriter — adapts SseWriter to write over WebSocket ──

function createWsSseWriter(
  ws: WebSocket,
  conversationId: string,
): SseWriter {
  return {
    writeEvent(event: string, data: unknown) {
      if (ws.readyState !== WebSocket.OPEN) return;
      const msg: WsMessage = {
        type: event,
        conversationId,
        data: data as Record<string, unknown>,
      };
      ws.send(JSON.stringify(msg));
    },
    close() {
      // WebSocket stays open — we don't close it per turn
    },
  };
}

// ── Collecting + WS tee writer for MCP turns ──

export interface CollectedEvent {
  event: string;
  data: unknown;
}

export function createCollectingWsSseWriter(
  conversationId: string,
  broadcastFn: (msg: WsMessage) => void,
): SseWriter & { events: CollectedEvent[] } {
  const events: CollectedEvent[] = [];
  return {
    events,
    writeEvent(event: string, data: unknown) {
      events.push({ event, data });
      broadcastFn({
        type: event,
        conversationId,
        data: data as Record<string, unknown>,
      });
    },
    close() {},
  };
}

// ── WebSocket Server ──

let wss: WebSocketServer | null = null;
const activeTurnsByWs = new WeakMap<WebSocket, ActiveTurn>();

export function attach(server: HttpServer): void {
  wss = new WebSocketServer({ server, path: "/ws" });

  wss.on("connection", (ws: WebSocket, _req: IncomingMessage) => {
    // Send connected event
    ws.send(JSON.stringify({ type: "connected" }));

    ws.on("message", (raw) => {
      try {
        const msg = JSON.parse(raw.toString()) as WsMessage;
        handleClientMessage(ws, msg);
      } catch {
        // Ignore malformed messages
      }
    });

    ws.on("close", () => {
      // Abort any active turn when the client disconnects
      const active = activeTurnsByWs.get(ws);
      if (active) {
        active.abort.abort();
        activeTurnsByWs.delete(ws);
      }
    });
  });
}

function handleClientMessage(ws: WebSocket, msg: WsMessage): void {
  switch (msg.type) {
    case "start_turn":
      handleStartTurn(ws, msg);
      break;
    case "abort_turn":
      handleAbortTurn(ws, msg);
      break;
    case "tool_result":
      handleToolResult(ws, msg);
      break;
  }
}

async function handleStartTurn(ws: WebSocket, msg: WsMessage): Promise<void> {
  const data = msg.data || {};
  const messages = data.messages as WireMessage[] | undefined;
  const agentId = data.agentId as string | undefined;
  const conversationId = msg.conversationId || uuidv4();

  if (!messages) {
    sendError(ws, conversationId, "messages is required");
    return;
  }

  // Create abort controller for this turn
  const abort = new AbortController();
  activeTurnsByWs.set(ws, { conversationId, abort });

  const sse = createWsSseWriter(ws, conversationId);

  try {
    await runAgentTurn(conversationId, messages, sse, agentId, abort.signal);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    sendError(ws, conversationId, message);
  } finally {
    activeTurnsByWs.delete(ws);
  }
}

function handleAbortTurn(ws: WebSocket, msg: WsMessage): void {
  const active = activeTurnsByWs.get(ws);
  if (active && (!msg.conversationId || active.conversationId === msg.conversationId)) {
    active.abort.abort();
  }
}

function handleToolResult(ws: WebSocket, msg: WsMessage): void {
  const data = msg.data || {};
  const toolUseId = data.toolUseId as string;
  const content = data.content as string;
  const isError = (data.isError as boolean) || false;

  if (!toolUseId) return;
  resolveFrontendToolResult(toolUseId, content, isError);
}

// ── Broadcasting ──

export function broadcast(msgOrType: WsMessage | string, data?: unknown): void {
  if (!wss) return;

  let payload: string;
  if (typeof msgOrType === "string") {
    // Legacy-style: broadcast(event, data) — used by tool-events and mcp-handler
    payload = JSON.stringify({ type: msgOrType, data: data ?? {} });
  } else {
    payload = JSON.stringify(msgOrType);
  }

  for (const client of wss.clients) {
    if (client.readyState === WebSocket.OPEN) {
      client.send(payload);
    }
  }
}

// ── Helpers ──

function sendError(ws: WebSocket, conversationId: string, message: string): void {
  if (ws.readyState !== WebSocket.OPEN) return;
  ws.send(JSON.stringify({
    type: "error",
    conversationId,
    data: { message },
  }));
}
