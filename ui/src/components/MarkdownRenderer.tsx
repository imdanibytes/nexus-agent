import { useMemo } from "react";
import { marked } from "marked";

interface Props {
  content: string;
}

export function MarkdownRenderer({ content }: Props) {
  const html = useMemo(() => {
    marked.setOptions({ breaks: true, gfm: true });
    const raw = marked.parse(content) as string;
    // Wrap tables in a scrollable container
    return raw.replace(/<table>/g, '<div class="table-wrap"><table>').replace(/<\/table>/g, '</table></div>');
  }, [content]);

  return (
    <div
      className="markdown-content"
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}
