import type { Message, RepositoryMessage } from "../api/client.js";
import type { ChatMessage } from "../stores/threadStore.js";
import { convertToMessage, toServerMessage } from "../runtime/convert.js";

// ── Types ──

export interface MessageNode {
  message: Message;
  parentId: string | null;
}

// ── Tree operations ──

/** Build a lookup from parentId → sorted child message IDs */
export function buildChildrenMap(
  repo: MessageNode[],
): Record<string, string[]> {
  const map: Record<string, string[]> = {};
  for (const node of repo) {
    const key = node.parentId ?? "__root__";
    if (!map[key]) map[key] = [];
    map[key].push(node.message.id);
  }
  return map;
}

/** Walk the tree using branch selections to produce the active branch's ordered message list */
export function resolveActiveBranch(
  repo: MessageNode[],
  childrenMap: Record<string, string[]>,
  selections: Record<string, number>,
): ChatMessage[] {
  const byId = new Map<string, MessageNode>();
  for (const node of repo) byId.set(node.message.id, node);

  const result: ChatMessage[] = [];
  let parentKey = "__root__";

  while (true) {
    const children = childrenMap[parentKey];
    if (!children || children.length === 0) break;

    const selectedIndex = selections[parentKey] ?? children.length - 1;
    const childId = children[Math.min(selectedIndex, children.length - 1)];
    const node = byId.get(childId);
    if (!node) break;

    result.push(convertToMessage(node.message));
    parentKey = childId;
  }

  return result;
}

/** Get branch info for a message: its index among siblings and total sibling count */
export function getBranchInfo(
  messageId: string,
  repo: MessageNode[],
  childrenMap: Record<string, string[]>,
): { index: number; count: number; parentKey: string } | null {
  const node = repo.find((n) => n.message.id === messageId);
  if (!node) return null;

  const parentKey = node.parentId ?? "__root__";
  const siblings = childrenMap[parentKey];
  if (!siblings) return null;

  const index = siblings.indexOf(messageId);
  return { index, count: siblings.length, parentKey };
}

/** Convert RepositoryMessage[] from server into MessageNode[] */
export function parseRepository(
  repoMessages: RepositoryMessage[],
): MessageNode[] {
  return repoMessages
    .filter((rm) => rm.message != null)
    .map((rm) => ({
      message: rm.message as Message,
      parentId: rm.parentId,
    }));
}
