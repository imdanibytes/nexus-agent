import { useState, useMemo } from "react";
import { SchemaField } from "./SchemaField.js";
import { Send, X } from "lucide-react";

interface Props {
  toolUseId: string;
  input: Record<string, unknown>;
  onRespond: (toolUseId: string, action: string, content: unknown) => void;
}

export function ElicitForm({ toolUseId, input, onRespond }: Props) {
  const message = input.message as string;
  const schema = input.requestedSchema as {
    type: string;
    properties?: Record<string, unknown>;
    required?: string[];
  };

  const properties = schema?.properties || {};
  const requiredFields = schema?.required || [];

  const defaults = useMemo(() => {
    const d: Record<string, unknown> = {};
    for (const [key, prop] of Object.entries(properties)) {
      const p = prop as { default?: unknown };
      if (p.default !== undefined) d[key] = p.default;
    }
    return d;
  }, [properties]);

  const [values, setValues] = useState<Record<string, unknown>>(defaults);
  const [submitted, setSubmitted] = useState(false);

  const handleSubmit = () => {
    setSubmitted(true);
    onRespond(toolUseId, "accept", values);
  };

  const handleDecline = () => {
    setSubmitted(true);
    onRespond(toolUseId, "decline", null);
  };

  return (
    <div className="rounded-xl border border-nx-border bg-nx-surface p-4 max-w-lg">
      {message && (
        <p className="text-sm mb-4">{message}</p>
      )}

      <div className="space-y-4">
        {Object.entries(properties).map(([key, prop]) => (
          <SchemaField
            key={key}
            name={key}
            schema={prop as Record<string, unknown>}
            value={values[key]}
            onChange={(v) => setValues((prev) => ({ ...prev, [key]: v }))}
            required={requiredFields.includes(key)}
          />
        ))}
      </div>

      <div className="flex gap-2 mt-4">
        <button
          onClick={handleSubmit}
          disabled={submitted}
          className="flex items-center gap-1.5 px-4 py-2 text-sm bg-nx-accent text-white rounded-lg hover:bg-nx-accent/80 transition-colors disabled:opacity-50"
        >
          <Send size={14} />
          Submit
        </button>
        <button
          onClick={handleDecline}
          disabled={submitted}
          className="flex items-center gap-1.5 px-4 py-2 text-sm bg-nx-raised border border-nx-border text-nx-text rounded-lg hover:bg-nx-surface transition-colors disabled:opacity-50"
        >
          <X size={14} />
          Skip
        </button>
      </div>
    </div>
  );
}
