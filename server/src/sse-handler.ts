import type { IncomingMessage, ServerResponse } from "node:http";
import { v4 as uuidv4 } from "uuid";
import { runAgentTurn, type TurnResult } from "./agent.js";
import type { WireMessage } from "./agent.js";
import { BroadcastHub } from "./streaming.js";

// ── Shared state ──

export const hub = new BroadcastHub();

interface ActiveTurn {
  conversationId: string;
  abort: AbortController;
}

/** One active turn at a time (single-user desktop app). */
let activeTurn: ActiveTurn | null = null;

// ── Route handler ──

export async function handleSseRoute(
  req: IncomingMessage,
  res: ServerResponse,
  url: string,
  method: string,
  readBody: () => Promise<string>,
  json: (res: ServerResponse, status: number, data: unknown) => void,
): Promise<boolean> {
  // Persistent SSE stream — client opens this on mount
  if (method === "GET" && url === "/api/v1/events") {
    hub.add(res);
    return true;
  }

  // Start a new turn
  if (method === "POST" && url === "/api/v1/turn") {
    const body = JSON.parse(await readBody());
    const {
      messages,
      conversationId: rawConvId,
      agentId,
      frontendTools,
    } = body as {
      messages?: WireMessage[];
      conversationId?: string;
      agentId?: string;
      frontendTools?: { name: string; description: string; input_schema: { type: "object"; properties: Record<string, unknown>; required?: string[] } }[];
    };

    if (!messages) {
      json(res, 400, { error: "messages is required" });
      return true;
    }

    const conversationId = rawConvId || uuidv4();

    // Create abort controller
    const abort = new AbortController();
    activeTurn = { conversationId, abort };

    // Create an SseWriter that pushes events to the broadcast hub
    const writer = hub.createCollectingWriter(conversationId);

    try {
      const result = await runAgentTurn(
        conversationId,
        messages,
        writer,
        agentId,
        abort.signal,
        frontendTools,
      );
      json(res, 200, { ok: true, conversationId, ...result });
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      json(res, 500, { error: message, conversationId });
    } finally {
      activeTurn = null;
    }

    return true;
  }

  // Abort the active turn
  if (method === "POST" && url === "/api/v1/turn/abort") {
    if (activeTurn) {
      const body = await readBody().catch(() => "{}");
      const parsed = JSON.parse(body || "{}") as { conversationId?: string };
      if (
        !parsed.conversationId ||
        activeTurn.conversationId === parsed.conversationId
      ) {
        activeTurn.abort.abort();
        json(res, 200, { ok: true });
      } else {
        json(res, 404, { error: "No matching active turn" });
      }
    } else {
      json(res, 404, { error: "No active turn" });
    }
    return true;
  }

  return false;
}
