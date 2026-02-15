import fs from "node:fs";
import path from "node:path";
import { v4 as uuidv4 } from "uuid";
import type { AgentProfile } from "./types.js";

const PROFILES_DIR = "/data/profiles";
const INDEX_PATH = path.join(PROFILES_DIR, "index.json");
const ACTIVE_PATH = path.join(PROFILES_DIR, "active.json");

function ensureDir(): void {
  if (!fs.existsSync(PROFILES_DIR)) {
    fs.mkdirSync(PROFILES_DIR, { recursive: true });
  }
}

function atomicWrite(filePath: string, data: unknown): void {
  ensureDir();
  const tmp = filePath + ".tmp";
  fs.writeFileSync(tmp, JSON.stringify(data, null, 2));
  fs.renameSync(tmp, filePath);
}

function loadProfiles(): AgentProfile[] {
  ensureDir();
  if (!fs.existsSync(INDEX_PATH)) return [];
  try {
    return JSON.parse(fs.readFileSync(INDEX_PATH, "utf8"));
  } catch {
    return [];
  }
}

export function listProfiles(): AgentProfile[] {
  return loadProfiles().sort((a, b) => b.updatedAt - a.updatedAt);
}

export function getProfile(id: string): AgentProfile | null {
  return loadProfiles().find((p) => p.id === id) ?? null;
}

export function createProfile(data: {
  name: string;
  model: string;
  systemPrompt: string;
  avatar?: string;
}): AgentProfile {
  const profiles = loadProfiles();
  const now = Date.now();
  const profile: AgentProfile = {
    id: uuidv4(),
    name: data.name,
    model: data.model,
    systemPrompt: data.systemPrompt,
    avatar: data.avatar,
    createdAt: now,
    updatedAt: now,
  };
  profiles.push(profile);
  atomicWrite(INDEX_PATH, profiles);
  return profile;
}

export function updateProfile(
  id: string,
  data: Partial<Pick<AgentProfile, "name" | "model" | "systemPrompt" | "avatar">>
): AgentProfile | null {
  const profiles = loadProfiles();
  const idx = profiles.findIndex((p) => p.id === id);
  if (idx < 0) return null;

  const existing = profiles[idx];
  const updated: AgentProfile = {
    ...existing,
    ...data,
    updatedAt: Date.now(),
  };
  profiles[idx] = updated;
  atomicWrite(INDEX_PATH, profiles);
  return updated;
}

export function deleteProfile(id: string): boolean {
  const profiles = loadProfiles();
  const filtered = profiles.filter((p) => p.id !== id);
  if (filtered.length === profiles.length) return false;

  atomicWrite(INDEX_PATH, filtered);

  // If deleted profile was active, clear active
  if (getActiveProfileId() === id) {
    setActiveProfileId(null);
  }
  return true;
}

export function getActiveProfileId(): string | null {
  ensureDir();
  if (!fs.existsSync(ACTIVE_PATH)) return null;
  try {
    const data = JSON.parse(fs.readFileSync(ACTIVE_PATH, "utf8"));
    return data.profileId ?? null;
  } catch {
    return null;
  }
}

export function setActiveProfileId(id: string | null): void {
  atomicWrite(ACTIVE_PATH, { profileId: id });
}
