import type { SystemMessageProvider, SystemMessageContext } from "./types.js";
import type { SpanHandle } from "../timing.js";

function withTimeout<T>(promise: Promise<T>, ms: number): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`Timed out after ${ms}ms`)), ms);
    promise.then(
      (val) => { clearTimeout(timer); resolve(val); },
      (err) => { clearTimeout(timer); reject(err); },
    );
  });
}

export class SystemMessageBuilder {
  private providers: SystemMessageProvider[] = [];

  register(provider: SystemMessageProvider): void {
    this.providers.push(provider);
  }

  async build(ctx: SystemMessageContext, parentSpan?: SpanHandle): Promise<string> {
    const results = await Promise.allSettled(
      this.providers.map((p) => {
        const providerSpan = parentSpan?.span(`provider:${p.name}`);
        return withTimeout(p.provide(ctx), p.timeoutMs)
          .catch((err) => {
            console.warn(`[system-message] provider "${p.name}" failed:`, err.message);
            return null;
          })
          .finally(() => providerSpan?.end());
      }),
    );

    const sections: string[] = [];
    for (const result of results) {
      const value = result.status === "fulfilled" ? result.value : null;
      if (value) sections.push(value);
    }

    return sections.join("\n\n");
  }
}
