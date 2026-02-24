/**
 * SSE-based event bus. Opens a single persistent EventSource to /api/events.
 * Turn-scoped events are routed to per-conversation async iterables.
 * Broadcast events (CUSTOM) dispatch to registered handlers.
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

  connect(): void {
    if (this.source?.readyState === EventSource.OPEN) return;

    // Close stale connection that's stuck reconnecting
    this.source?.close();

    const es = new EventSource("/api/events");

    es.onopen = () => {
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
      // EventSource auto-reconnects, but we need to force a fresh
      // connection if the server restarted (new process = new broadcast
      // channel, old reconnect lands on a clean hub that missed events).
    };

    this.source = es;
  }

  /** Resolves when the SSE connection is open. Forces reconnect if needed. */
  ensureConnected(): Promise<void> {
    if (this.source?.readyState === EventSource.OPEN) {
      return Promise.resolve();
    }
    this.connect();
    return new Promise((resolve) => {
      // Double-check after connect() — might already be open
      if (this.source?.readyState === EventSource.OPEN) {
        resolve();
        return;
      }
      this.openWaiters.push(resolve);
    });
  }

  disconnect(): void {
    this.source?.close();
    this.source = null;
  }

  on(name: string, handler: BroadcastHandler): () => void {
    let set = this.broadcastHandlers.get(name);
    if (!set) {
      set = new Set();
      this.broadcastHandlers.set(name, set);
    }
    set.add(handler);
    return () => set!.delete(handler);
  }

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
              return Promise.resolve({
                value: stream.queue.shift()!,
                done: false,
              });
            }
            if (stream.done) {
              self.streams.delete(threadId);
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
