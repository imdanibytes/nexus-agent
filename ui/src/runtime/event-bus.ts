/**
 * SSE-based event bus with a single global EventSource connection.
 *
 * One connection per browser tab at /api/events. The server pushes all events
 * for all conversations. Turn-scoped events are routed to per-conversation
 * async iterables. Broadcast events (CUSTOM, SYNC, RUN_STARTED) dispatch to
 * registered handlers.
 */

export const EventType = {
  RUN_STARTED: "RUN_STARTED",
  RUN_FINISHED: "RUN_FINISHED",
  RUN_ERROR: "RUN_ERROR",
  TEXT_MESSAGE_START: "TEXT_MESSAGE_START",
  TEXT_MESSAGE_CONTENT: "TEXT_MESSAGE_CONTENT",
  TEXT_MESSAGE_END: "TEXT_MESSAGE_END",
  TOOL_CALL_START: "TOOL_CALL_START",
  TOOL_CALL_ARGS: "TOOL_CALL_ARGS",
  TOOL_CALL_END: "TOOL_CALL_END",
  TOOL_CALL_RESULT: "TOOL_CALL_RESULT",
  CUSTOM: "CUSTOM",
  SYNC: "SYNC",
} as const;

export type EventType = (typeof EventType)[keyof typeof EventType];

export interface AgUiEvent {
  type: string;
  threadId?: string;
  runId?: string;
  [key: string]: unknown;
}

const TURN_EVENTS = new Set<string>([
  EventType.RUN_STARTED,
  EventType.RUN_FINISHED,
  EventType.RUN_ERROR,
  EventType.TEXT_MESSAGE_START,
  EventType.TEXT_MESSAGE_CONTENT,
  EventType.TEXT_MESSAGE_END,
  EventType.TOOL_CALL_START,
  EventType.TOOL_CALL_ARGS,
  EventType.TOOL_CALL_END,
  EventType.TOOL_CALL_RESULT,
]);

interface TurnStream {
  queue: AgUiEvent[];
  resolve: ((result: IteratorResult<AgUiEvent>) => void) | null;
  done: boolean;
}

type BroadcastHandler = (event: AgUiEvent) => void;

class EventBus {
  private source: EventSource | null = null;
  private broadcastHandlers = new Map<string, Set<BroadcastHandler>>();
  private streams = new Map<string, TurnStream>();
  private openWaiters: Array<() => void> = [];

  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  /**
   * Open the global SSE connection to /api/events.
   * No-op if already connected and OPEN/CONNECTING.
   * Replaces CLOSED connections automatically.
   */
  connect(): void {
    // If existing source is CLOSED, tear it down first
    if (this.source && this.source.readyState === EventSource.CLOSED) {
      this.source.close();
      this.source = null;
    }

    if (this.source) return;

    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }

    const es = new EventSource("/api/events");

    es.onopen = () => {
      console.log("[EventBus] SSE connection opened");
      for (const resolve of this.openWaiters) resolve();
      this.openWaiters = [];
    };

    es.onmessage = (evt) => {
      try {
        const event = JSON.parse(evt.data) as AgUiEvent;
        this.dispatch(event);
      } catch {
        // Ignore malformed events
      }
    };

    es.onerror = () => {
      // EventSource has three states: CONNECTING (0), OPEN (1), CLOSED (2).
      // On transient errors it stays CONNECTING and auto-reconnects.
      // On fatal errors (server gone) it moves to CLOSED — no auto-reconnect.
      if (es.readyState === EventSource.CLOSED) {
        console.warn("[EventBus] SSE connection closed by server, scheduling reconnect");
        this.source = null;
        // Retry after a short delay to avoid tight loops during server downtime
        this.reconnectTimer = setTimeout(() => {
          this.reconnectTimer = null;
          this.connect();
        }, 2000);
      }
    };

    this.source = es;
  }

  /**
   * Resolves when the SSE connection is open.
   * Opens the connection if not already open.
   * Replaces CLOSED connections before waiting.
   */
  ensureConnected(): Promise<void> {
    if (this.source?.readyState === EventSource.OPEN) {
      return Promise.resolve();
    }

    // connect() handles CLOSED → teardown → new connection
    this.connect();

    if (this.source?.readyState === EventSource.OPEN) {
      return Promise.resolve();
    }

    return new Promise((resolve) => {
      this.openWaiters.push(resolve);
    });
  }

  /**
   * Close the global SSE connection.
   */
  disconnect(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (!this.source) return;
    console.log("[EventBus] closing global EventSource");
    this.source.close();
    this.source = null;
  }

  /**
   * Register a broadcast handler for CUSTOM/SYNC/RUN_STARTED events.
   * Receives matching events from the global connection.
   */
  on(name: string, handler: BroadcastHandler): () => void {
    let set = this.broadcastHandlers.get(name);
    if (!set) {
      set = new Set();
      this.broadcastHandlers.set(name, set);
    }
    set.add(handler);
    return () => set!.delete(handler);
  }

  /**
   * Subscribe to turn-scoped events for a specific conversation.
   * Returns an async iterable that yields events until the turn ends.
   */
  subscribe(threadId: string): AsyncIterable<AgUiEvent> {
    let stream = this.streams.get(threadId);
    if (!stream) {
      stream = { queue: [], resolve: null, done: false };
      this.streams.set(threadId, stream);
    }

    const streams = this.streams;
    return {
      [Symbol.asyncIterator](): AsyncIterator<AgUiEvent> {
        return {
          next(): Promise<IteratorResult<AgUiEvent>> {
            if (stream.queue.length > 0) {
              return Promise.resolve({
                value: stream.queue.shift()!,
                done: false,
              });
            }
            if (stream.done) {
              streams.delete(threadId);
              return Promise.resolve({
                value: undefined as never,
                done: true,
              });
            }
            return new Promise((resolve) => {
              stream.resolve = resolve;
            });
          },
        };
      },
    };
  }

  endSubscription(threadId: string): void {
    const stream = this.streams.get(threadId);
    if (!stream) return;

    stream.done = true;
    if (stream.resolve) {
      stream.resolve({ value: undefined as never, done: true });
      stream.resolve = null;
    }
    this.streams.delete(threadId);
  }

  private dispatch(event: AgUiEvent): void {
    const type = event.type;

    // SYNC events dispatch to broadcast handlers only (no threadId routing)
    if (type === EventType.SYNC) {
      const handlers = this.broadcastHandlers.get("SYNC");
      if (handlers) {
        for (const h of handlers) h(event);
      }
      return;
    }

    if (type === EventType.CUSTOM) {
      const name = event.name as string;
      const handlers = this.broadcastHandlers.get(name);
      if (handlers) {
        for (const h of handlers) h(event);
      }
      if (event.threadId) {
        this.routeToStream(event);
      }
      return;
    }

    // Notify broadcast handlers for RUN_STARTED so server-initiated
    // follow-up turns can be detected
    if (type === EventType.RUN_STARTED) {
      const handlers = this.broadcastHandlers.get("RUN_STARTED");
      if (handlers) {
        for (const h of handlers) h(event);
      }
    }

    if (TURN_EVENTS.has(type) && event.threadId) {
      this.routeToStream(event);
    }
  }

  private routeToStream(event: AgUiEvent): void {
    const threadId = event.threadId!;
    let stream = this.streams.get(threadId);

    if (!stream) {
      stream = { queue: [event], resolve: null, done: false };
      this.streams.set(threadId, stream);
      return;
    }

    if (stream.done) return;

    if (stream.resolve) {
      stream.resolve({ value: event, done: false });
      stream.resolve = null;
    } else {
      stream.queue.push(event);
    }
  }
}

export const eventBus = new EventBus();
