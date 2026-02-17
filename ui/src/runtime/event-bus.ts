/**
 * SSE-based event bus replacing WebSocket client + TurnRouter.
 *
 * Opens a single persistent EventSource to /api/v1/events.
 * Broadcast events (tools_changed, mcp_turn_pending, etc.) are dispatched
 * to registered handlers. Turn-scoped events are routed to per-conversation
 * async iterables for consumption by useChatStream.
 */

// ── AG-UI Event Types (client-side mirror) ──

export const EventType = {
  RUN_STARTED: "RUN_STARTED",
  RUN_FINISHED: "RUN_FINISHED",
  RUN_ERROR: "RUN_ERROR",
  STEP_STARTED: "STEP_STARTED",
  STEP_FINISHED: "STEP_FINISHED",
  TEXT_MESSAGE_START: "TEXT_MESSAGE_START",
  TEXT_MESSAGE_CONTENT: "TEXT_MESSAGE_CONTENT",
  TEXT_MESSAGE_END: "TEXT_MESSAGE_END",
  TOOL_CALL_START: "TOOL_CALL_START",
  TOOL_CALL_ARGS: "TOOL_CALL_ARGS",
  TOOL_CALL_END: "TOOL_CALL_END",
  TOOL_CALL_RESULT: "TOOL_CALL_RESULT",
  CUSTOM: "CUSTOM",
} as const;

export type EventType = (typeof EventType)[keyof typeof EventType];

export interface AgUiEvent {
  type: string;
  threadId?: string;
  runId?: string;
  [key: string]: unknown;
}

export interface PendingToolCall {
  toolCallId: string;
  toolCallName: string;
  args: Record<string, unknown>;
}

// ── Turn-scoped event types ──

const TURN_EVENTS = new Set<string>([
  EventType.RUN_STARTED,
  EventType.RUN_FINISHED,
  EventType.RUN_ERROR,
  EventType.STEP_STARTED,
  EventType.STEP_FINISHED,
  EventType.TEXT_MESSAGE_START,
  EventType.TEXT_MESSAGE_CONTENT,
  EventType.TEXT_MESSAGE_END,
  EventType.TOOL_CALL_START,
  EventType.TOOL_CALL_ARGS,
  EventType.TOOL_CALL_END,
  EventType.TOOL_CALL_RESULT,
]);

// ── Stream infrastructure ──

interface TurnStream {
  queue: AgUiEvent[];
  resolve: ((result: IteratorResult<AgUiEvent>) => void) | null;
  done: boolean;
}

// ── EventBus ──

type BroadcastHandler = (event: AgUiEvent) => void;

class EventBus {
  private source: EventSource | null = null;
  private broadcastHandlers = new Map<string, Set<BroadcastHandler>>();
  private streams = new Map<string, TurnStream>();

  /** Open the persistent SSE connection. */
  connect(): void {
    if (this.source) return;

    const es = new EventSource("/api/v1/events");

    es.onmessage = (evt) => {
      try {
        const event = JSON.parse(evt.data) as AgUiEvent;
        this.dispatch(event);
      } catch {
        // Ignore malformed events
      }
    };

    es.onerror = () => {
      // EventSource auto-reconnects — nothing extra needed
    };

    this.source = es;
  }

  /** Close the SSE connection. */
  disconnect(): void {
    this.source?.close();
    this.source = null;
  }

  /** Register a handler for broadcast events (CUSTOM events by name). */
  on(name: string, handler: BroadcastHandler): () => void {
    let set = this.broadcastHandlers.get(name);
    if (!set) {
      set = new Set();
      this.broadcastHandlers.set(name, set);
    }
    set.add(handler);
    return () => set!.delete(handler);
  }

  /** Create an async iterable that yields turn events for a conversation. */
  subscribe(threadId: string): AsyncIterable<AgUiEvent> {
    let stream = this.streams.get(threadId);
    if (!stream) {
      stream = { queue: [], resolve: null, done: false };
      this.streams.set(threadId, stream);
    }

    const self = this;
    return {
      [Symbol.asyncIterator](): AsyncIterator<AgUiEvent> {
        return {
          next(): Promise<IteratorResult<AgUiEvent>> {
            if (stream.queue.length > 0) {
              return Promise.resolve({ value: stream.queue.shift()!, done: false });
            }
            if (stream.done) {
              self.streams.delete(threadId);
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

  /** Signal that a turn is complete for a conversation (ends the async iterable). */
  endSubscription(threadId: string): void {
    const stream = this.streams.get(threadId);
    if (!stream) return;

    stream.done = true;
    if (stream.resolve) {
      stream.resolve({ value: undefined as never, done: true });
      stream.resolve = null;
    }
  }

  // ── Internal dispatch ──

  private dispatch(event: AgUiEvent): void {
    const type = event.type;

    // CUSTOM events → dispatch to broadcast handlers by name
    if (type === EventType.CUSTOM) {
      const name = event.name as string;
      const handlers = this.broadcastHandlers.get(name);
      if (handlers) {
        for (const h of handlers) h(event);
      }
      // CUSTOM events with threadId also get routed to turn streams
      // (e.g. title_update, timing)
      if (event.threadId) {
        this.routeToStream(event);
      }
      return;
    }

    // Turn-scoped events → route to the correct conversation stream
    if (TURN_EVENTS.has(type) && event.threadId) {
      this.routeToStream(event);
      return;
    }
  }

  private routeToStream(event: AgUiEvent): void {
    const threadId = event.threadId!;
    let stream = this.streams.get(threadId);

    if (!stream) {
      // No consumer yet — create a provisional buffer
      stream = { queue: [event], resolve: null, done: false };
      this.streams.set(threadId, stream);
      return;
    }

    if (stream.resolve) {
      stream.resolve({ value: event, done: false });
      stream.resolve = null;
    } else {
      stream.queue.push(event);
    }
  }
}

/** Singleton event bus */
export const eventBus = new EventBus();
