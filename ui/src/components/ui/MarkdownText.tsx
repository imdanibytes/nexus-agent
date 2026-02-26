import { memo, useCallback, type FC } from "react";
import { Streamdown, type Components } from "streamdown";
import { createMathPlugin } from "@streamdown/math";
import { createCodePlugin } from "@streamdown/code";
import remarkGfm from "remark-gfm";
import {
  remarkHighlight,
  remarkSubSuperscript,
  remarkAbbreviations,
} from "../../lib/remark-plugins";
import { remarkAlert } from "remark-github-blockquote-alert";
import remarkGemoji from "remark-gemoji";
import "katex/dist/katex.min.css";
import { Alert, Tooltip } from "@heroui/react";
import { cn } from "../../lib/utils";

interface MarkdownTextProps {
  text: string;
  isStreaming?: boolean;
}

const streamdownPlugins = {
  math: createMathPlugin({ singleDollarTextMath: true }),
  code: createCodePlugin({ themes: ["github-light", "github-dark"] }),
};

const streamdownRemarkPlugins: import("streamdown").StreamdownProps["remarkPlugins"] =
  [
    [remarkGfm, { singleTilde: false }],
    remarkAlert,
    remarkGemoji,
    remarkHighlight,
    remarkSubSuperscript,
    remarkAbbreviations,
  ];

const MarkdownTextImpl: FC<MarkdownTextProps> = ({ text, isStreaming }) => {
  return (
    <Streamdown
      mode={isStreaming ? "streaming" : "static"}
      isAnimating={isStreaming}
      components={markdownComponents}
      plugins={streamdownPlugins}
      remarkPlugins={streamdownRemarkPlugins}
      controls={{ code: true }}
      allowedTags={{
        mark: ["className", "style"],
        sub: [],
        sup: [],
        abbr: ["title"],
        section: ["dataFootnotes"],
        div: ["className", "dir"],
        p: ["className", "dir"],
        svg: ["className", "viewBox", "width", "height", "ariaHidden", "fill"],
        path: ["d", "fill", "fillRule"],
      }}
      className="aui-md"
    >
      {text}
    </Streamdown>
  );
};

export const MarkdownText = memo(MarkdownTextImpl);

// ── Component overrides ──

const alertColorMap: Record<string, "primary" | "success" | "secondary" | "warning" | "danger" | "default"> = {
  note: "primary",
  tip: "success",
  important: "secondary",
  warning: "warning",
  caution: "danger",
};

const markdownComponents: Components = {
  h1: ({ className, node: _, ...props }) => (
    <h1
      className={cn(
        "aui-md-h1 mb-2 scroll-m-20 font-semibold text-base first:mt-0 last:mb-0",
        className,
      )}
      {...props}
    />
  ),
  h2: ({ className, node: _, ...props }) => (
    <h2
      className={cn(
        "aui-md-h2 mt-3 mb-1.5 scroll-m-20 font-semibold text-sm first:mt-0 last:mb-0",
        className,
      )}
      {...props}
    />
  ),
  h3: ({ className, node: _, ...props }) => (
    <h3
      className={cn(
        "aui-md-h3 mt-2.5 mb-1 scroll-m-20 font-semibold text-sm first:mt-0 last:mb-0",
        className,
      )}
      {...props}
    />
  ),
  h4: ({ className, node: _, ...props }) => (
    <h4
      className={cn(
        "aui-md-h4 mt-2 mb-1 scroll-m-20 font-medium text-sm first:mt-0 last:mb-0",
        className,
      )}
      {...props}
    />
  ),
  p: ({ className, node: _, ...props }) => (
    <p
      className={cn(
        "aui-md-p my-2.5 leading-normal first:mt-0 last:mb-0",
        className,
      )}
      {...props}
    />
  ),
  a: ({ className, node: _, href, ...props }) => {
    const isAnchor = href?.startsWith("#");

    const handleClick = useCallback(
      (e: React.MouseEvent<HTMLAnchorElement>) => {
        if (!isAnchor || !href) return;
        e.preventDefault();
        const id = href.slice(1);
        const el =
          document.getElementById(id) ||
          document.getElementById(`user-content-${id}`);
        if (!el) return;
        el.scrollIntoView({ behavior: "smooth", block: "center" });
        el.classList.add("aui-md-flash");
        setTimeout(() => el.classList.remove("aui-md-flash"), 1500);
      },
      [isAnchor, href],
    );

    return (
      <a
        className={cn(
          "aui-md-a text-primary underline underline-offset-2 hover:text-primary/80 cursor-pointer",
          isAnchor && "no-underline",
          className,
        )}
        href={href}
        onClick={isAnchor ? handleClick : undefined}
        {...(!isAnchor && { target: "_blank", rel: "noopener noreferrer" })}
        {...props}
      />
    );
  },
  blockquote: ({ className, node: _, ...props }) => (
    <blockquote
      className={cn(
        "aui-md-blockquote my-2.5 border-default-400/30 border-l-2 pl-3 text-default-500 italic",
        className,
      )}
      {...props}
    />
  ),
  ul: ({ className, node: _, ...props }) => (
    <ul
      className={cn(
        "aui-md-ul my-2 ml-4 list-disc marker:text-default-500 [&>li]:mt-1",
        className,
      )}
      {...props}
    />
  ),
  ol: ({ className, node: _, ...props }) => (
    <ol
      className={cn(
        "aui-md-ol my-2 ml-4 list-decimal marker:text-default-500 [&>li]:mt-1",
        className,
      )}
      {...props}
    />
  ),
  li: ({ className, node: _, ...props }) => (
    <li className={cn("aui-md-li leading-normal", className)} {...props} />
  ),
  hr: ({ className, node: _, ...props }) => (
    <hr
      className={cn("aui-md-hr my-2 border-default-400/20", className)}
      {...props}
    />
  ),
  table: ({ className, node: _, ...props }) => (
    <table
      className={cn(
        "aui-md-table my-2 w-full border-separate border-spacing-0 overflow-y-auto",
        className,
      )}
      {...props}
    />
  ),
  th: ({ className, node: _, ...props }) => (
    <th
      className={cn(
        "aui-md-th bg-default-100/40 px-2 py-1 text-left font-medium first:rounded-tl-lg last:rounded-tr-lg",
        className,
      )}
      {...props}
    />
  ),
  td: ({ className, node: _, ...props }) => (
    <td
      className={cn(
        "aui-md-td border-default-400/20 border-b border-l px-2 py-1 text-left last:border-r",
        className,
      )}
      {...props}
    />
  ),
  tr: ({ className, node: _, ...props }) => (
    <tr
      className={cn(
        "aui-md-tr m-0 border-b p-0 first:border-t [&:last-child>td:first-child]:rounded-bl-lg [&:last-child>td:last-child]:rounded-br-lg",
        className,
      )}
      {...props}
    />
  ),
  mark: ({ className, node: _, ...props }) => (
    <mark
      className={cn("aui-md-mark rounded-sm px-0.5 text-inherit", className)}
      style={{ backgroundColor: "hsl(var(--heroui-primary) / 0.2)" }}
      {...props}
    />
  ),
  abbr: ({ className, node: _, title, children, ...props }) => (
    <Tooltip content={title} placement="top" className="max-w-xs text-xs">
      <abbr
        className={cn(
          "aui-md-abbr cursor-help border-b border-dotted border-default-400/50 no-underline",
          className,
        )}
        title={undefined}
        {...props}
      >
        {children}
      </abbr>
    </Tooltip>
  ),
  div: ({ className, node: _, children, ...props }) => {
    const alertMatch = className?.match(/markdown-alert-(\w+)/);
    if (alertMatch) {
      const type = alertMatch[1] as keyof typeof alertColorMap;
      const color = alertColorMap[type] ?? "default";
      // Children from the plugin: [title-p, ...content-p]
      // The title-p has class="markdown-alert-title" and contains the icon SVG + label
      // We skip it since HeroUI Alert provides its own icon
      const childArray = Array.isArray(children) ? children : [children];
      const contentChildren = childArray.filter((child) => {
        if (!child || typeof child !== "object" || !("props" in child)) return true;
        return !child.props?.className?.includes("markdown-alert-title");
      });
      const titleLabel = alertMatch[1].charAt(0).toUpperCase() + alertMatch[1].slice(1);
      return (
        <Alert
          color={color}
          variant="flat"
          title={titleLabel}
          className="my-2.5"
          description={<div className="text-sm leading-normal [&>p]:my-1 [&>p:first-child]:mt-0 [&>p:last-child]:mb-0">{contentChildren}</div>}
        />
      );
    }
    return <div className={className} {...props}>{children}</div>;
  },
  span: ({ className, node: _, ...props }) => {
    // Wrap display math in a scrollable block container
    if (className?.includes("katex-display")) {
      return (
        <span
          className={className}
          style={{ display: "block", overflowX: "auto", overflowY: "hidden", maxWidth: "100%", WebkitOverflowScrolling: "touch", padding: "4px 0" }}
          {...props}
        />
      );
    }
    // Wrap inline math in a constrained inline-block
    if (className?.includes("katex")) {
      return (
        <span
          className={className}
          style={{ display: "inline-block", maxWidth: "100%", overflowX: "auto", overflowY: "hidden", verticalAlign: "bottom" }}
          {...props}
        />
      );
    }
    return <span className={className} {...props} />;
  },
  section: ({ className, node: _, ...props }) => {
    const isFootnotes =
      (props as Record<string, unknown>)["data-footnotes"] !== undefined;
    return (
      <section
        className={cn(
          isFootnotes &&
            "aui-md-footnotes mt-4 border-t border-default-400/20 pt-3 text-xs text-default-500 [&_ol]:ml-4 [&_ol]:list-decimal [&_li]:mt-1",
          className,
        )}
        {...props}
      />
    );
  },
};
