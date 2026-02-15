import fs from "node:fs";
import type { ToolSettings } from "./types.js";

const SETTINGS_PATH = "/data/tool-settings.json";

const DEFAULTS: ToolSettings = {
  hiddenToolPatterns: ["_nexus_*"],
};

function load(): ToolSettings {
  try {
    if (fs.existsSync(SETTINGS_PATH)) {
      return JSON.parse(fs.readFileSync(SETTINGS_PATH, "utf8"));
    }
  } catch {
    // fall through
  }
  return { ...DEFAULTS };
}

function save(data: ToolSettings): void {
  const dir = "/data";
  if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });
  const tmp = SETTINGS_PATH + ".tmp";
  fs.writeFileSync(tmp, JSON.stringify(data, null, 2));
  fs.renameSync(tmp, SETTINGS_PATH);
}

export async function getToolSettings(): Promise<ToolSettings> {
  const stored = load();
  return {
    hiddenToolPatterns: stored.hiddenToolPatterns ?? DEFAULTS.hiddenToolPatterns,
    globalToolFilter: stored.globalToolFilter,
  };
}

export async function updateToolSettings(
  updates: Partial<ToolSettings>,
): Promise<ToolSettings> {
  const current = load();
  const merged: ToolSettings = { ...current, ...updates };
  save(merged);
  return merged;
}
