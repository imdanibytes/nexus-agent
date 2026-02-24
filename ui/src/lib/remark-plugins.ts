import type { Root, PhrasingContent, RootContent } from "mdast";
import { visit } from "unist-util-visit";

// ── Mark / Highlight ──

const MARK_RE = /==([^\n]+?)==/g;

export function remarkHighlight() {
  return (tree: Root) => {
    visit(tree, "text", (node, index, parent) => {
      if (index === undefined || !parent) return;

      MARK_RE.lastIndex = 0;
      const parts: PhrasingContent[] = [];
      let last = 0;
      let match: RegExpExecArray | null;

      while ((match = MARK_RE.exec(node.value)) !== null) {
        if (match.index > last) {
          parts.push({ type: "text", value: node.value.slice(last, match.index) });
        }
        parts.push({ type: "html", value: `<mark>${match[1]}</mark>` });
        last = match.index + match[0].length;
      }

      if (parts.length === 0) return;

      if (last < node.value.length) {
        parts.push({ type: "text", value: node.value.slice(last) });
      }

      parent.children.splice(index, 1, ...(parts as RootContent[]));
      return index + parts.length;
    });
  };
}

// ── Subscript & Superscript ──

const SUB_SUPER_RE = /~([^~\s][^~]*)~|\^([^^\s][^^]*)\^/g;

export function remarkSubSuperscript() {
  return (tree: Root) => {
    visit(tree, "text", (node, index, parent) => {
      if (index === undefined || !parent) return;

      SUB_SUPER_RE.lastIndex = 0;
      const parts: PhrasingContent[] = [];
      let last = 0;
      let match: RegExpExecArray | null;

      while ((match = SUB_SUPER_RE.exec(node.value)) !== null) {
        if (match.index > last) {
          parts.push({ type: "text", value: node.value.slice(last, match.index) });
        }

        if (match[1] !== undefined) {
          parts.push({ type: "html", value: `<sub>${match[1]}</sub>` });
        } else if (match[2] !== undefined) {
          parts.push({ type: "html", value: `<sup>${match[2]}</sup>` });
        }

        last = match.index + match[0].length;
      }

      if (parts.length === 0) return;

      if (last < node.value.length) {
        parts.push({ type: "text", value: node.value.slice(last) });
      }

      parent.children.splice(index, 1, ...(parts as RootContent[]));
      return index + parts.length;
    });
  };
}

// ── Abbreviations ──

const ABBR_DEF_RE = /^\*\[([^\]]+)\]:\s*(.+)$/gm;

export function remarkAbbreviations() {
  return (tree: Root) => {
    const abbrs = new Map<string, string>();

    visit(tree, "text", (node) => {
      ABBR_DEF_RE.lastIndex = 0;
      let match: RegExpExecArray | null;
      while ((match = ABBR_DEF_RE.exec(node.value)) !== null) {
        abbrs.set(match[1], match[2]);
      }
    });

    if (abbrs.size === 0) return;

    visit(tree, "paragraph", (node, index, parent) => {
      if (index === undefined || !parent) return;

      const text = node.children
        .filter((c): c is Extract<typeof c, { type: "text" }> => c.type === "text")
        .map((c) => c.value)
        .join("");

      const stripped = text.replace(ABBR_DEF_RE, "").trim();
      if (stripped === "") {
        parent.children.splice(index, 1);
        return index;
      }
    });

    const sorted = [...abbrs.keys()].sort((a, b) => b.length - a.length);
    const abbrRe = new RegExp(`\\b(${sorted.map(escapeRegex).join("|")})\\b`, "g");

    visit(tree, "text", (node, index, parent) => {
      if (index === undefined || !parent) return;

      abbrRe.lastIndex = 0;
      const parts: PhrasingContent[] = [];
      let last = 0;
      let match: RegExpExecArray | null;

      while ((match = abbrRe.exec(node.value)) !== null) {
        if (match.index > last) {
          parts.push({ type: "text", value: node.value.slice(last, match.index) });
        }

        const title = abbrs.get(match[1])!;
        parts.push({
          type: "html",
          value: `<abbr title="${escapeHtml(title)}">${match[1]}</abbr>`,
        });

        last = match.index + match[0].length;
      }

      if (parts.length === 0) return;

      if (last < node.value.length) {
        parts.push({ type: "text", value: node.value.slice(last) });
      }

      parent.children.splice(index, 1, ...(parts as RootContent[]));
      return index + parts.length;
    });
  };
}

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
