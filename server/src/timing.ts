import { randomUUID } from "node:crypto";
import { performance } from "node:perf_hooks";

export interface SpanMarker {
  label: string;
  timeMs: number; // relative to collector origin
}

export interface Span {
  id: string;
  name: string;
  parentId: string | null;
  startMs: number;
  endMs: number;
  durationMs: number;
  metadata?: Record<string, unknown>;
  markers?: SpanMarker[];
}

interface RawSpan {
  id: string;
  name: string;
  parentId: string | null;
  startMs: number;
  endMs: number;
  metadata?: Record<string, unknown>;
  markers?: { label: string; absoluteMs: number }[];
}

export class SpanCollector {
  private spans: RawSpan[] = [];
  private origin: number;

  constructor() {
    this.origin = performance.now();
  }

  span(name: string, metadata?: Record<string, unknown>): SpanHandle {
    return new SpanHandle(this, name, null, metadata);
  }

  /** @internal Called by SpanHandle.end() */
  _record(raw: RawSpan): void {
    this.spans.push(raw);
  }

  get originMs(): number {
    return this.origin;
  }

  toJSON(): Span[] {
    const origin = this.origin;
    return this.spans
      .map((s) => {
        const span: Span = {
          id: s.id,
          name: s.name,
          parentId: s.parentId,
          startMs: Math.round((s.startMs - origin) * 100) / 100,
          endMs: Math.round((s.endMs - origin) * 100) / 100,
          durationMs: Math.round((s.endMs - s.startMs) * 100) / 100,
        };
        if (s.metadata && Object.keys(s.metadata).length > 0) {
          span.metadata = s.metadata;
        }
        if (s.markers && s.markers.length > 0) {
          span.markers = s.markers.map((m) => ({
            label: m.label,
            timeMs: Math.round((m.absoluteMs - origin) * 100) / 100,
          }));
        }
        return span;
      })
      .sort((a, b) => a.startMs - b.startMs);
  }
}

export class SpanHandle {
  readonly id: string;
  private collector: SpanCollector;
  private name: string;
  private parentId: string | null;
  private startMs: number;
  private metadata?: Record<string, unknown>;
  private markers: { label: string; absoluteMs: number }[] = [];
  private ended = false;

  constructor(
    collector: SpanCollector,
    name: string,
    parentId: string | null,
    metadata?: Record<string, unknown>,
  ) {
    this.id = randomUUID();
    this.collector = collector;
    this.name = name;
    this.parentId = parentId;
    this.startMs = performance.now();
    this.metadata = metadata;
  }

  /** Create a child span nested under this one. */
  span(name: string, metadata?: Record<string, unknown>): SpanHandle {
    return new SpanHandle(this.collector, name, this.id, metadata);
  }

  /** Record a point-in-time marker within this span. */
  mark(label: string): void {
    this.markers.push({ label, absoluteMs: performance.now() });
  }

  /** End this span and record it. Safe to call multiple times. */
  end(): void {
    if (this.ended) return;
    this.ended = true;
    this.collector._record({
      id: this.id,
      name: this.name,
      parentId: this.parentId,
      startMs: this.startMs,
      endMs: performance.now(),
      metadata: this.metadata,
      markers: this.markers.length > 0 ? this.markers : undefined,
    });
  }
}
