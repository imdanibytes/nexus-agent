import { useState } from "react";

interface SchemaProperty {
  type?: string;
  title?: string;
  description?: string;
  default?: unknown;
  enum?: string[];
  oneOf?: { const: string; title: string }[];
  minimum?: number;
  maximum?: number;
  placeholder?: string;
  format?: string;
}

interface Props {
  name: string;
  schema: SchemaProperty;
  value: unknown;
  onChange: (value: unknown) => void;
  required: boolean;
}

export function SchemaField({ name, schema, value, onChange, required }: Props) {
  const label = schema.title || name;

  // oneOf with const/title → select or radio
  if (schema.oneOf && schema.oneOf.length > 0) {
    return (
      <div className="space-y-2">
        <label className="block text-sm font-medium">{label}{required && " *"}</label>
        {schema.description && (
          <p className="text-xs text-nx-muted">{schema.description}</p>
        )}
        <div className="space-y-1">
          {schema.oneOf.map((opt) => (
            <label key={opt.const} className="flex items-center gap-2 text-sm cursor-pointer">
              <input
                type="radio"
                name={name}
                value={opt.const}
                checked={value === opt.const}
                onChange={() => onChange(opt.const)}
                className="accent-nx-accent"
              />
              {opt.title}
            </label>
          ))}
        </div>
      </div>
    );
  }

  // enum → select
  if (schema.enum && schema.enum.length > 0) {
    return (
      <div className="space-y-1">
        <label className="block text-sm font-medium">{label}{required && " *"}</label>
        {schema.description && (
          <p className="text-xs text-nx-muted">{schema.description}</p>
        )}
        <select
          value={(value as string) || ""}
          onChange={(e) => onChange(e.target.value)}
          className="w-full bg-nx-raised border border-nx-border rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-nx-accent"
        >
          <option value="">Select...</option>
          {schema.enum.map((opt) => (
            <option key={opt} value={opt}>{opt}</option>
          ))}
        </select>
      </div>
    );
  }

  // boolean → checkbox
  if (schema.type === "boolean") {
    return (
      <label className="flex items-center gap-2 cursor-pointer">
        <input
          type="checkbox"
          checked={Boolean(value)}
          onChange={(e) => onChange(e.target.checked)}
          className="accent-nx-accent"
        />
        <span className="text-sm font-medium">{label}</span>
        {schema.description && (
          <span className="text-xs text-nx-muted">— {schema.description}</span>
        )}
      </label>
    );
  }

  // number/integer
  if (schema.type === "number" || schema.type === "integer") {
    return (
      <div className="space-y-1">
        <label className="block text-sm font-medium">{label}{required && " *"}</label>
        {schema.description && (
          <p className="text-xs text-nx-muted">{schema.description}</p>
        )}
        <input
          type="number"
          value={(value as number) ?? (schema.default as number) ?? ""}
          onChange={(e) => onChange(e.target.value ? Number(e.target.value) : undefined)}
          min={schema.minimum}
          max={schema.maximum}
          step={schema.type === "integer" ? 1 : undefined}
          className="w-full bg-nx-raised border border-nx-border rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-nx-accent"
        />
      </div>
    );
  }

  // string (default)
  return (
    <div className="space-y-1">
      <label className="block text-sm font-medium">{label}{required && " *"}</label>
      {schema.description && (
        <p className="text-xs text-nx-muted">{schema.description}</p>
      )}
      <input
        type={schema.format === "email" ? "email" : schema.format === "uri" ? "url" : "text"}
        value={(value as string) || ""}
        onChange={(e) => onChange(e.target.value)}
        placeholder={schema.placeholder}
        className="w-full bg-nx-raised border border-nx-border rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-nx-accent"
      />
    </div>
  );
}
