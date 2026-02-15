import { useMemo } from "react";
import { marked } from "marked";

interface Props {
  content: string;
}

export function MarkdownRenderer({ content }: Props) {
  const html = useMemo(() => {
    marked.setOptions({ breaks: true, gfm: true });
    return marked.parse(content) as string;
  }, [content]);

  return (
    <div
      className="markdown-content"
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}
