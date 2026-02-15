import { useState, useMemo } from "react";
import { SchemaField } from "./SchemaField.js";
import { Send, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";

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
    <div className="rounded-xl border border-border bg-card p-4 max-w-lg animate-slide-up">
      {message && (
        <p className="text-sm mb-3 leading-relaxed">{message}</p>
      )}

      {message && Object.keys(properties).length > 0 && <Separator className="my-3" />}

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

      <div className="flex gap-2 mt-4 pt-3 border-t border-border">
        <Button onClick={handleSubmit} disabled={submitted} size="sm" className="gap-1.5">
          <Send size={13} />
          Submit
        </Button>
        <Button onClick={handleDecline} disabled={submitted} variant="outline" size="sm" className="gap-1.5">
          <X size={13} />
          Skip
        </Button>
      </div>
    </div>
  );
}
