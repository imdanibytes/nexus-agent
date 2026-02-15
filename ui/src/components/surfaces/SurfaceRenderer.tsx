import { useState } from "react";
import { Send } from "lucide-react";

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
          <div key={comp.id} className="rounded-xl border border-nx-border bg-nx-surface p-4">
            {cardTitle && <h3 className="text-sm font-semibold mb-1">{cardTitle}</h3>}
            {cardSubtitle && <p className="text-xs text-nx-muted mb-2">{cardSubtitle}</p>}
            {children}
          </div>
        );
      }

      case "Text":
        const variant = (props.variant as string) || "body";
        const className = variant === "heading" ? "text-base font-semibold" :
          variant === "caption" ? "text-xs text-nx-muted" : "text-sm";
        return <p key={comp.id} className={className}>{props.content as string}</p>;

      case "Table": {
        const columns = (props.columns || []) as string[];
        const rows = (props.rows || []) as string[][];
        return (
          <div key={comp.id} className="overflow-x-auto my-2">
            <table className="w-full text-sm border-collapse">
              <thead>
                <tr>
                  {columns.map((col, i) => (
                    <th key={i} className="border border-nx-border bg-nx-raised px-3 py-1.5 text-left font-medium">
                      {col}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {rows.map((row, i) => (
                  <tr key={i}>
                    {row.map((cell, j) => (
                      <td key={j} className="border border-nx-border px-3 py-1.5">{cell}</td>
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
          <dl key={comp.id} className="space-y-1 my-2">
            {items.map((item, i) => (
              <div key={i} className="flex gap-2 text-sm">
                <dt className="text-nx-muted">{item.label}:</dt>
                <dd>{item.value}</dd>
              </div>
            ))}
          </dl>
        );
      }

      case "CodeBlock":
        return (
          <pre key={comp.id} className="bg-nx-raised p-3 rounded-lg overflow-x-auto text-xs font-mono my-2">
            {props.code as string}
          </pre>
        );

      case "Image":
        return (
          <img
            key={comp.id}
            src={props.src as string}
            alt={(props.alt as string) || ""}
            className="rounded-lg max-w-full my-2"
          />
        );

      case "TextField": {
        const tfLabel = props.label as string | undefined;
        return (
          <div key={comp.id} className="space-y-1 my-2">
            {tfLabel && <label className="block text-sm font-medium">{tfLabel}</label>}
            <input
              type="text"
              placeholder={(props.placeholder as string) || ""}
              value={(formData[props.dataPath as string] as string) || ""}
              onChange={(e) =>
                setFormData((prev) => ({ ...prev, [props.dataPath as string]: e.target.value }))
              }
              className="w-full bg-nx-raised border border-nx-border rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-nx-accent"
            />
          </div>
        );
      }

      case "Select": {
        const options = (props.options || []) as { label: string; value: string }[];
        const selLabel = props.label as string | undefined;
        return (
          <div key={comp.id} className="space-y-1 my-2">
            {selLabel && <label className="block text-sm font-medium">{selLabel}</label>}
            <select
              value={(formData[props.dataPath as string] as string) || ""}
              onChange={(e) =>
                setFormData((prev) => ({ ...prev, [props.dataPath as string]: e.target.value }))
              }
              className="w-full bg-nx-raised border border-nx-border rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-nx-accent"
            >
              <option value="">Select...</option>
              {options.map((opt) => (
                <option key={opt.value} value={opt.value}>{opt.label}</option>
              ))}
            </select>
          </div>
        );
      }

      case "Button":
        return (
          <button
            key={comp.id}
            onClick={() => {
              setSubmitted(true);
              onRespond(toolUseId, props.action as string || "click", { button: comp.id });
            }}
            disabled={submitted}
            className="px-4 py-2 text-sm bg-nx-accent text-white rounded-lg hover:bg-nx-accent/80 transition-colors disabled:opacity-50 my-2"
          >
            {props.label as string}
          </button>
        );

      default:
        return null;
    }
  }

  return (
    <div className="rounded-xl border border-nx-border bg-nx-surface p-4 max-w-lg">
      {title && <h3 className="text-sm font-semibold mb-3">{title}</h3>}
      {components
        .filter((c) => !components.some((parent) => parent.children?.includes(c.id)))
        .map((c) => renderComponent(c))}

      {interactive && !submitted && (
        <div className="mt-4">
          <button
            onClick={handleSubmit}
            className="flex items-center gap-1.5 px-4 py-2 text-sm bg-nx-accent text-white rounded-lg hover:bg-nx-accent/80 transition-colors"
          >
            <Send size={14} />
            Submit
          </button>
        </div>
      )}
    </div>
  );
}
