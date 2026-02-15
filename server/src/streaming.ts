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
