import fs from "node:fs";
import path from "node:path";
import { v4 as uuidv4 } from "uuid";
import type { Provider, ProviderPublic } from "./types.js";
import { invalidateClientCache } from "./client-factory.js";

const PROVIDERS_DIR = "/data/providers";
const INDEX_PATH = path.join(PROVIDERS_DIR, "index.json");

function ensureDir(): void {
  if (!fs.existsSync(PROVIDERS_DIR)) {
    fs.mkdirSync(PROVIDERS_DIR, { recursive: true });
  }
}

function atomicWrite(filePath: string, data: unknown): void {
  ensureDir();
  const tmp = filePath + ".tmp";
  fs.writeFileSync(tmp, JSON.stringify(data, null, 2));
  fs.renameSync(tmp, filePath);
}

function loadProviders(): Provider[] {
  ensureDir();
  if (!fs.existsSync(INDEX_PATH)) return [];
  try {
    return JSON.parse(fs.readFileSync(INDEX_PATH, "utf8"));
  } catch {
    return [];
  }
}

function stripSecrets(provider: Provider): ProviderPublic {
  const { apiKey, awsAccessKeyId, awsSecretAccessKey, awsSessionToken, ...pub } = provider;
  return pub;
}

export async function listProviders(): Promise<ProviderPublic[]> {
  return loadProviders().map(stripSecrets);
}

export async function getProvider(id: string): Promise<Provider | null> {
  return loadProviders().find((p) => p.id === id) ?? null;
}

export async function getProviderPublic(id: string): Promise<ProviderPublic | null> {
  const provider = await getProvider(id);
  return provider ? stripSecrets(provider) : null;
}

export async function createProvider(
  data: Omit<Provider, "id" | "createdAt" | "updatedAt">,
): Promise<ProviderPublic> {
  const providers = loadProviders();
  const now = Date.now();
  const provider: Provider = {
    ...data,
    id: uuidv4(),
    createdAt: now,
    updatedAt: now,
  };
  providers.push(provider);
  atomicWrite(INDEX_PATH, providers);
  return stripSecrets(provider);
}

export async function updateProvider(
  id: string,
  data: Partial<Omit<Provider, "id" | "createdAt" | "updatedAt">>,
): Promise<ProviderPublic | null> {
  const providers = loadProviders();
  const idx = providers.findIndex((p) => p.id === id);
  if (idx < 0) return null;

  const updated: Provider = {
    ...providers[idx],
    ...data,
    updatedAt: Date.now(),
  };
  providers[idx] = updated;
  atomicWrite(INDEX_PATH, providers);
  invalidateClientCache(id);
  return stripSecrets(updated);
}

export async function deleteProvider(id: string): Promise<boolean> {
  const providers = loadProviders();
  const filtered = providers.filter((p) => p.id !== id);
  if (filtered.length === providers.length) return false;
  atomicWrite(INDEX_PATH, filtered);
  invalidateClientCache(id);
  return true;
}
