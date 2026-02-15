import fs from "node:fs";
import path from "node:path";
import type { Conversation, ConversationMeta } from "./types.js";

const DATA_DIR = "/data/conversations";
const INDEX_PATH = path.join(DATA_DIR, "index.json");

function ensureDir(): void {
  if (!fs.existsSync(DATA_DIR)) {
    fs.mkdirSync(DATA_DIR, { recursive: true });
  }
}

function loadIndex(): ConversationMeta[] {
  ensureDir();
  if (!fs.existsSync(INDEX_PATH)) return [];
  try {
    return JSON.parse(fs.readFileSync(INDEX_PATH, "utf8"));
  } catch {
    return [];
  }
}

function saveIndex(index: ConversationMeta[]): void {
  ensureDir();
  const tmp = INDEX_PATH + ".tmp";
  fs.writeFileSync(tmp, JSON.stringify(index, null, 2));
  fs.renameSync(tmp, INDEX_PATH);
}

export function listConversations(): ConversationMeta[] {
  return loadIndex().sort((a, b) => b.updatedAt - a.updatedAt);
}

export function getConversation(id: string): Conversation | null {
  const filePath = path.join(DATA_DIR, `conv_${id}.json`);
  if (!fs.existsSync(filePath)) return null;
  try {
    return JSON.parse(fs.readFileSync(filePath, "utf8"));
  } catch {
    return null;
  }
}

export function saveConversation(conv: Conversation): void {
  ensureDir();
  const filePath = path.join(DATA_DIR, `conv_${conv.id}.json`);
  const tmp = filePath + ".tmp";
  fs.writeFileSync(tmp, JSON.stringify(conv, null, 2));
  fs.renameSync(tmp, filePath);

  // Update index
  const index = loadIndex();
  const existing = index.findIndex((c) => c.id === conv.id);
  const meta: ConversationMeta = {
    id: conv.id,
    title: conv.title,
    createdAt: conv.createdAt,
    updatedAt: conv.updatedAt,
    messageCount: conv.messages.length,
  };
  if (existing >= 0) {
    index[existing] = meta;
  } else {
    index.push(meta);
  }
  saveIndex(index);
}

export function deleteConversation(id: string): boolean {
  const filePath = path.join(DATA_DIR, `conv_${id}.json`);
  if (fs.existsSync(filePath)) {
    fs.unlinkSync(filePath);
  }
  const index = loadIndex();
  const filtered = index.filter((c) => c.id !== id);
  if (filtered.length !== index.length) {
    saveIndex(filtered);
    return true;
  }
  return false;
}

export function updateConversationTitle(id: string, title: string): boolean {
  const conv = getConversation(id);
  if (!conv) return false;
  conv.title = title;
  conv.updatedAt = Date.now();
  saveConversation(conv);
  return true;
}
