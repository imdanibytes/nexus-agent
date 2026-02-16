import { memo, useState, type FC } from "react";
import { Streamdown, type Components } from "streamdown";
import { CheckIcon, CopyIcon } from "lucide-react";
import { TooltipIconButton } from "@/components/chat/tooltip-icon-button.js";
import { cn } from "@/lib/utils.js";

interface MarkdownTextProps {
  text: string;
  isStreaming?: boolean;
}

const MarkdownTextImpl: FC<MarkdownTextProps> = ({ text, isStreaming }) => {
  return (
    <Streamdown
      mode={isStreaming ? "streaming" : "static"}
      isAnimating={isStreaming}
      components={markdownComponents}
      className="aui-md"
    >
      {text}
    </Streamdown>
  );
};

export const MarkdownText = memo(MarkdownTextImpl);

// ── Copy hook ──

function useCopyToClipboard(copiedDuration = 3000) {
  const [isCopied, setIsCopied] = useState(false);

  const copyToClipboard = (value: string) => {
    if (!value) return;
    navigator.clipboard.writeText(value).then(() => {
      setIsCopied(true);
      setTimeout(() => setIsCopied(false), copiedDuration);
    });
  };

  return { isCopied, copyToClipboard };
}

// ── Code header ──

const CodeHeader: FC<{ language: string; code: string }> = ({
  language,
  code,
}) => {
  const { isCopied, copyToClipboard } = useCopyToClipboard();
  const onCopy = () => {
    if (!code || isCopied) return;
    copyToClipboard(code);
  };

  return (
    <div className="aui-code-header-root mt-2.5 flex items-center justify-between rounded-t-lg border border-border/50 border-b-0 bg-muted/50 px-3 py-1.5 text-xs">
      <span className="aui-code-header-language font-medium text-muted-foreground lowercase">
        {language}
      </span>
      <TooltipIconButton tooltip="Copy" onClick={onCopy}>
        {!isCopied && <CopyIcon />}
        {isCopied && <CheckIcon />}
      </TooltipIconButton>
    </div>
  );
};

// ── Component overrides ──

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
  h5: ({ className, node: _, ...props }) => (
    <h5
      className={cn(
        "aui-md-h5 mt-2 mb-1 font-medium text-sm first:mt-0 last:mb-0",
        className,
      )}
      {...props}
    />
  ),
  h6: ({ className, node: _, ...props }) => (
    <h6
      className={cn(
        "aui-md-h6 mt-2 mb-1 font-medium text-sm first:mt-0 last:mb-0",
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
  a: ({ className, node: _, ...props }) => (
    <a
      className={cn(
        "aui-md-a text-primary underline underline-offset-2 hover:text-primary/80",
        className,
      )}
      target="_blank"
      rel="noopener noreferrer"
      {...props}
    />
  ),
  blockquote: ({ className, node: _, ...props }) => (
    <blockquote
      className={cn(
        "aui-md-blockquote my-2.5 border-muted-foreground/30 border-l-2 pl-3 text-muted-foreground italic",
        className,
      )}
      {...props}
    />
  ),
  ul: ({ className, node: _, ...props }) => (
    <ul
      className={cn(
        "aui-md-ul my-2 ml-4 list-disc marker:text-muted-foreground [&>li]:mt-1",
        className,
      )}
      {...props}
    />
  ),
  ol: ({ className, node: _, ...props }) => (
    <ol
      className={cn(
        "aui-md-ol my-2 ml-4 list-decimal marker:text-muted-foreground [&>li]:mt-1",
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
      className={cn("aui-md-hr my-2 border-muted-foreground/20", className)}
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
        "aui-md-th bg-muted px-2 py-1 text-left font-medium first:rounded-tl-lg last:rounded-tr-lg [[align=center]]:text-center [[align=right]]:text-right",
        className,
      )}
      {...props}
    />
  ),
  td: ({ className, node: _, ...props }) => (
    <td
      className={cn(
        "aui-md-td border-muted-foreground/20 border-b border-l px-2 py-1 text-left last:border-r [[align=center]]:text-center [[align=right]]:text-right",
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
  sup: ({ className, node: _, ...props }) => (
    <sup
      className={cn(
        "aui-md-sup [&>a]:text-xs [&>a]:no-underline",
        className,
      )}
      {...props}
    />
  ),
  pre: ({ className, children, node: _, ...props }) => {
    // Extract language + code from the child <code> for CodeHeader
    let language = "";
    let code = "";

    const child = Array.isArray(children) ? children[0] : children;
    if (child && typeof child === "object" && "props" in child) {
      const codeProps = child.props as {
        className?: string;
        children?: React.ReactNode;
      };
      const match = codeProps.className?.match(/language-(\w+)/);
      language = match?.[1] ?? "";
      code =
        typeof codeProps.children === "string" ? codeProps.children : "";
    }

    return (
      <>
        {language && <CodeHeader language={language} code={code} />}
        <pre
          className={cn(
            "aui-md-pre overflow-x-auto rounded-b-lg border border-border/50 bg-muted/30 p-3 text-xs leading-relaxed",
            language
              ? "rounded-t-none border-t-0"
              : "rounded-lg mt-2.5",
            className,
          )}
          {...props}
        >
          {children}
        </pre>
      </>
    );
  },
  code: ({ className, node: _, ...props }) => {
    const isCodeBlock = className?.includes("language-");
    return (
      <code
        className={cn(
          !isCodeBlock &&
            "aui-md-inline-code rounded-md border border-border/50 bg-muted/50 px-1.5 py-0.5 font-mono text-[0.85em]",
          className,
        )}
        {...props}
      />
    );
  },
};
