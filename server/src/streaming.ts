import type { ServerResponse } from "node:http";
import type { SseWriter } from "./types.js";

export function createSseWriter(res: ServerResponse): SseWriter {
  res.writeHead(200, {
    "Content-Type": "text/event-stream",
    "Cache-Control": "no-cache",
    Connection: "keep-alive",
    "Access-Control-Allow-Origin": "*",
  });

  return {
    writeEvent(event: string, data: unknown) {
      const json = JSON.stringify(data);
      res.write(`event: ${event}\ndata: ${json}\n\n`);
    },
    close() {
      res.end();
    },
  };
}

export interface CollectedEvent {
  event: string;
  data: unknown;
}

export type CollectingSseWriter = SseWriter & { events: CollectedEvent[] };

export function createCollectingSseWriter(): CollectingSseWriter {
  const events: CollectedEvent[] = [];
  return {
    events,
    writeEvent(event: string, data: unknown) {
      events.push({ event, data });
    },
    close() {},
  };
}
