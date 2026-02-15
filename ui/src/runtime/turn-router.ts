/**
 * Per-conversation async iterable dispatch for turn events.
 * Routes WebSocket turn-scoped events to the correct consumer.
 */

import type { WsMessage } from "./ws-client.js";

interface TurnStream {
  queue: WsMessage[];
  resolve: ((result: IteratorResult<WsMessage>) => void) | null;
  done: boolean;
}

/** Event types that are part of a turn (carry conversationId) */
const TURN_EVENTS = new Set([
  "turn_start",
  "text_start",
  "text_delta",
  "tool_start",
  "tool_input_delta",
  "tool_result",
  "tool_request",
  "ui_surface",
  "title_update",
  "timing",
  "turn_end",
  "error",
]);

class TurnRouter {
  private streams = new Map<string, TurnStream>();

  /** Route a turn-scoped WS message to the appropriate consumer */
  route(msg: WsMessage): void {
    if (!msg.conversationId || !TURN_EVENTS.has(msg.type)) return;

    const stream = this.streams.get(msg.conversationId);
    if (!stream) {
      // No consumer registered yet — buffer the event.
      // Create a provisional stream that buffers until someone consumes.
      const provisional: TurnStream = { queue: [msg], resolve: null, done: false };
      this.streams.set(msg.conversationId, provisional);
      return;
    }

    if (stream.resolve) {
      stream.resolve({ value: msg, done: false });
      stream.resolve = null;
    } else {
      stream.queue.push(msg);
    }
  }

  /** Create an async iterable that yields turn events for a conversation */
  createTurnStream(conversationId: string): AsyncIterable<WsMessage> {
    // Reuse existing stream (may have buffered events) or create new
    let stream = this.streams.get(conversationId);
    if (!stream) {
      stream = { queue: [], resolve: null, done: false };
      this.streams.set(conversationId, stream);
    }

    const self = this;
    return {
      [Symbol.asyncIterator](): AsyncIterator<WsMessage> {
        return {
          next(): Promise<IteratorResult<WsMessage>> {
            if (stream.queue.length > 0) {
              return Promise.resolve({ value: stream.queue.shift()!, done: false });
            }
            if (stream.done) {
              self.streams.delete(conversationId);
              return Promise.resolve({ value: undefined as never, done: true });
            }
            return new Promise((resolve) => {
              stream.resolve = resolve;
            });
          },
        };
      },
    };
  }

  /** Signal that a turn is complete for a conversation */
  endTurn(conversationId: string): void {
    const stream = this.streams.get(conversationId);
    if (!stream) return;

    stream.done = true;
    if (stream.resolve) {
      stream.resolve({ value: undefined as never, done: true });
      stream.resolve = null;
    }
  }

  /** Abort an active turn — ends the stream */
  abort(conversationId: string): void {
    this.endTurn(conversationId);
  }

  /** Check if there's an active/buffered stream for a conversation */
  has(conversationId: string): boolean {
    return this.streams.has(conversationId);
  }
}

export const turnRouter = new TurnRouter();
