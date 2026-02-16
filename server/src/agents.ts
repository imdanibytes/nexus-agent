import fs from "node:fs";
import path from "node:path";
import { v4 as uuidv4 } from "uuid";
import type { Agent } from "./types.js";

const AGENTS_DIR = "/data/agents";
const INDEX_PATH = path.join(AGENTS_DIR, "index.json");
const ACTIVE_PATH = path.join(AGENTS_DIR, "active.json");

function ensureDir(): void {
  if (!fs.existsSync(AGENTS_DIR)) {
    fs.mkdirSync(AGENTS_DIR, { recursive: true });
  }
}

function atomicWrite(filePath: string, data: unknown): void {
  ensureDir();
  const tmp = filePath + ".tmp";
  fs.writeFileSync(tmp, JSON.stringify(data, null, 2));
  fs.renameSync(tmp, filePath);
}

function loadAgents(): Agent[] {
  ensureDir();
  if (!fs.existsSync(INDEX_PATH)) return [];
  try {
    return JSON.parse(fs.readFileSync(INDEX_PATH, "utf8"));
  } catch {
    return [];
  }
}

export function listAgents(): Agent[] {
  return loadAgents().sort((a, b) => b.updatedAt - a.updatedAt);
}

export function getAgent(id: string): Agent | null {
  return loadAgents().find((a) => a.id === id) ?? null;
}

export function createAgent(data: {
  name: string;
  providerId: string;
  model: string;
  systemPrompt: string;
  temperature?: number;
  maxTokens?: number;
  topP?: number;
  toolFilter?: Agent["toolFilter"];
}): Agent {
  const agents = loadAgents();
  const now = Date.now();
  // Enforce mutual exclusivity: temperature takes priority if both are set
  const temperature = data.temperature ?? undefined;
  const topP = temperature !== undefined ? undefined : (data.topP ?? undefined);
  const agent: Agent = {
    id: uuidv4(),
    name: data.name,
    providerId: data.providerId,
    model: data.model,
    systemPrompt: data.systemPrompt,
    temperature,
    maxTokens: data.maxTokens,
    topP,
    toolFilter: data.toolFilter,
    createdAt: now,
    updatedAt: now,
  };
  agents.push(agent);
  atomicWrite(INDEX_PATH, agents);
  return agent;
}

export function updateAgent(
  id: string,
  data: Partial<
    Pick<
      Agent,
      "name" | "providerId" | "model" | "systemPrompt" | "temperature" | "maxTokens" | "topP" | "toolFilter"
    >
  >,
): Agent | null {
  const agents = loadAgents();
  const idx = agents.findIndex((a) => a.id === id);
  if (idx < 0) return null;

  const merged = { ...agents[idx], ...data, updatedAt: Date.now() };
  // Null means "clear this field" — strip nulls so they become undefined
  for (const key of ["temperature", "topP"] as const) {
    if (merged[key] === null) delete (merged as Record<string, unknown>)[key];
  }
  const updated: Agent = merged;
  agents[idx] = updated;
  atomicWrite(INDEX_PATH, agents);
  return updated;
}

export function deleteAgent(id: string): boolean {
  const agents = loadAgents();
  const filtered = agents.filter((a) => a.id !== id);
  if (filtered.length === agents.length) return false;

  atomicWrite(INDEX_PATH, filtered);

  // If deleted agent was active, clear active
  if (getActiveAgentId() === id) {
    setActiveAgentId(null);
  }
  return true;
}

export function getActiveAgentId(): string | null {
  ensureDir();
  if (!fs.existsSync(ACTIVE_PATH)) return null;
  try {
    const data = JSON.parse(fs.readFileSync(ACTIVE_PATH, "utf8"));
    return data.agentId ?? null;
  } catch {
    return null;
  }
}

export function setActiveAgentId(id: string | null): void {
  atomicWrite(ACTIVE_PATH, { agentId: id });
}
