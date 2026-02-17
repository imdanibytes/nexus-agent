import type { ServerResponse } from "node:http";
import type { SseWriter } from "./types.js";

/**
 * AG-UI compatible SSE format: `data: {"type":...,...}\n\n`
 *
 * The event type is embedded in the JSON payload rather than using SSE named
 * events, matching the AG-UI protocol specification.
 */
export function createSseWriter(
  res: ServerResponse,
  threadId?: string,
): SseWriter {
  res.writeHead(200, {
    "Content-Type": "text/event-stream",
    "Cache-Control": "no-cache",
    Connection: "keep-alive",
    "Access-Control-Allow-Origin": "*",
  });

  return {
    writeEvent(type: string, data: unknown) {
      const payload = { type, threadId, ...(data as object) };
      res.write(`data: ${JSON.stringify(payload)}\n\n`);
    },
    close() {
      res.end();
    },
  };
}

// ── Collecting writer (for MCP responses) ──

export interface CollectedEvent {
  type: string;
  data: unknown;
}

export type CollectingSseWriter = SseWriter & { events: CollectedEvent[] };

export function createCollectingSseWriter(): CollectingSseWriter {
  const events: CollectedEvent[] = [];
  return {
    events,
    writeEvent(type: string, data: unknown) {
      events.push({ type, data });
    },
    close() {},
  };
}

// ── BroadcastHub — manages persistent SSE connections ──

export class BroadcastHub {
  private clients = new Set<ServerResponse>();

  /** Register a new SSE client. Returns cleanup function. */
  add(res: ServerResponse): () => void {
    // Set up SSE headers
    res.writeHead(200, {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache",
      Connection: "keep-alive",
      "Access-Control-Allow-Origin": "*",
    });

    // Send initial connection event
    const connected = JSON.stringify({ type: "CONNECTED" });
    res.write(`data: ${connected}\n\n`);

    this.clients.add(res);

    const remove = () => {
      this.clients.delete(res);
    };

    res.on("close", remove);
    return remove;
  }

  /** Push an event to all connected SSE clients. */
  push(event: object): void {
    const line = `data: ${JSON.stringify(event)}\n\n`;
    for (const client of this.clients) {
      if (!client.writableEnded) {
        client.write(line);
      }
    }
  }

  /** Push an event to all clients AND collect it. */
  createCollectingWriter(threadId: string): CollectingSseWriter {
    const events: CollectedEvent[] = [];
    return {
      events,
      writeEvent: (type: string, data: unknown) => {
        events.push({ type, data });
        this.push({ type, threadId, ...(data as object) });
      },
      close() {},
    };
  }

  get size(): number {
    return this.clients.size;
  }
}
