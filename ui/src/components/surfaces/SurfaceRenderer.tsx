import { useState } from "react";
import { Send } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Badge } from "@/components/ui/badge";

interface A2Component {
  id: string;
  type: string;
  properties?: Record<string, unknown>;
  children?: string[];
}

interface Props {
  toolUseId: string;
  input: Record<string, unknown>;
  onRespond: (toolUseId: string, action: string, content: unknown) => void;
}

export function SurfaceRenderer({ toolUseId, input, onRespond }: Props) {
  const title = input.title as string | undefined;
  const components = (input.components || []) as A2Component[];
  const interactive = input.interactive as boolean;
  const [formData, setFormData] = useState<Record<string, unknown>>({});
  const [submitted, setSubmitted] = useState(false);

  const componentMap = new Map(components.map((c) => [c.id, c]));

  const handleSubmit = () => {
    setSubmitted(true);
    onRespond(toolUseId, "accept", formData);
  };

  function renderComponent(comp: A2Component): React.ReactNode {
    const props = comp.properties || {};
    const children = comp.children
      ?.map((id) => componentMap.get(id))
      .filter(Boolean)
      .map((c) => renderComponent(c!));

    switch (comp.type) {
      case "Card": {
        const cardTitle = props.title as string | undefined;
        const cardSubtitle = props.subtitle as string | undefined;
        return (
          <div key={comp.id} className="rounded-xl border border-border bg-card/80 p-4 my-2">
            {cardTitle && <h3 className="text-sm font-semibold mb-1">{cardTitle}</h3>}
            {cardSubtitle && <p className="text-[11px] text-muted-foreground mb-3">{cardSubtitle}</p>}
            {children}
          </div>
        );
      }

      case "Text": {
        const variant = (props.variant as string) || "body";
        const className =
          variant === "heading"
            ? "text-sm font-semibold"
            : variant === "caption"
              ? "text-[11px] text-muted-foreground"
              : "text-sm leading-relaxed";
        return (
          <p key={comp.id} className={className}>
            {props.content as string}
          </p>
        );
      }

      case "Table": {
        const columns = (props.columns || []) as string[];
        const rows = (props.rows || []) as string[][];
        return (
          <div key={comp.id} className="overflow-x-auto my-2 rounded-lg border border-border">
            <table className="w-full text-sm border-collapse">
              <thead>
                <tr>
                  {columns.map((col, i) => (
                    <th
                      key={i}
                      className="border-b border-border bg-secondary px-3 py-2 text-left text-xs font-medium text-muted-foreground"
                    >
                      {col}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {rows.map((row, i) => (
                  <tr key={i} className="border-b border-border last:border-0">
                    {row.map((cell, j) => (
                      <td key={j} className="px-3 py-2 text-xs">
                        {cell}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        );
      }

      case "DataList": {
        const items = (props.items || []) as { label: string; value: string }[];
        return (
          <dl key={comp.id} className="space-y-1.5 my-2">
            {items.map((item, i) => (
              <div key={i} className="flex gap-2 text-xs">
                <dt className="text-muted-foreground min-w-[80px]">{item.label}</dt>
                <dd className="font-medium">{item.value}</dd>
              </div>
            ))}
          </dl>
        );
      }

      case "CodeBlock":
        return (
          <pre
            key={comp.id}
            className="bg-secondary border border-border p-3 rounded-lg overflow-x-auto text-xs font-mono my-2"
          >
            {props.code as string}
          </pre>
        );

      case "Image":
        return (
          <img
            key={comp.id}
            src={props.src as string}
            alt={(props.alt as string) || ""}
            className="rounded-lg max-w-full my-2 border border-border"
          />
        );

      case "TextField": {
        const tfLabel = props.label as string | undefined;
        return (
          <div key={comp.id} className="space-y-1.5 my-2">
            {tfLabel && <Label className="text-xs">{tfLabel}</Label>}
            <Input
              type="text"
              placeholder={(props.placeholder as string) || ""}
              value={(formData[props.dataPath as string] as string) || ""}
              onChange={(e) =>
                setFormData((prev) => ({ ...prev, [props.dataPath as string]: e.target.value }))
              }
            />
          </div>
        );
      }

      case "Select": {
        const options = (props.options || []) as { label: string; value: string }[];
        const selLabel = props.label as string | undefined;
        const dataPath = props.dataPath as string;
        return (
          <div key={comp.id} className="space-y-1.5 my-2">
            {selLabel && <Label className="text-xs">{selLabel}</Label>}
            <Select
              value={(formData[dataPath] as string) || ""}
              onValueChange={(v) => setFormData((prev) => ({ ...prev, [dataPath]: v }))}
            >
              <SelectTrigger className="text-sm">
                <SelectValue placeholder="Select..." />
              </SelectTrigger>
              <SelectContent>
                {options.map((opt) => (
                  <SelectItem key={opt.value} value={opt.value}>
                    {opt.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        );
      }

      case "Button":
        return (
          <Button
            key={comp.id}
            onClick={() => {
              setSubmitted(true);
              onRespond(toolUseId, (props.action as string) || "click", { button: comp.id });
            }}
            disabled={submitted}
            size="sm"
            className="my-2"
          >
            {props.label as string}
          </Button>
        );

      default:
        return null;
    }
  }

  return (
    <div className="rounded-xl border border-border bg-card p-4 max-w-lg animate-slide-up">
      {title && <h3 className="text-sm font-semibold mb-3">{title}</h3>}
      {components
        .filter((c) => !components.some((parent) => parent.children?.includes(c.id)))
        .map((c) => renderComponent(c))}

      {interactive && !submitted && (
        <div className="mt-4 pt-3 border-t border-border">
          <Button onClick={handleSubmit} size="sm" className="gap-1.5">
            <Send size={13} />
            Submit
          </Button>
        </div>
      )}
    </div>
  );
}
