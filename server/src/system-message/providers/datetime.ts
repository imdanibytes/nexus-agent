import type { SystemMessageProvider } from "../types.js";

export const datetimeProvider: SystemMessageProvider = {
  name: "datetime",
  timeoutMs: 50,

  async provide(): Promise<string> {
    const now = new Date();
    return `Current date and time: ${now.toLocaleDateString("en-US", {
      weekday: "long",
      year: "numeric",
      month: "long",
      day: "numeric",
    })} ${now.toLocaleTimeString("en-US", {
      hour: "2-digit",
      minute: "2-digit",
      timeZoneName: "short",
    })}`;
  },
};
