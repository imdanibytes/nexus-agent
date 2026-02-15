import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";

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

  // oneOf with const/title → radio group
  if (schema.oneOf && schema.oneOf.length > 0) {
    return (
      <fieldset className="space-y-2.5">
        <Label className="text-xs">
          {label}
          {required && <span className="text-destructive ml-0.5">*</span>}
        </Label>
        {schema.description && (
          <p className="text-[11px] text-muted-foreground">{schema.description}</p>
        )}
        <div className="space-y-1.5">
          {schema.oneOf.map((opt) => (
            <label
              key={opt.const}
              className="flex items-center gap-2.5 px-3 py-2 rounded-lg border border-border hover:bg-accent/50 cursor-pointer transition-colors text-sm has-[:checked]:bg-primary/5 has-[:checked]:border-primary/20"
            >
              <input
                type="radio"
                name={name}
                value={opt.const}
                checked={value === opt.const}
                onChange={() => onChange(opt.const)}
                className="accent-[var(--primary)]"
              />
              {opt.title}
            </label>
          ))}
        </div>
      </fieldset>
    );
  }

  // enum → select
  if (schema.enum && schema.enum.length > 0) {
    return (
      <div className="space-y-1.5">
        <Label className="text-xs">
          {label}
          {required && <span className="text-destructive ml-0.5">*</span>}
        </Label>
        {schema.description && (
          <p className="text-[11px] text-muted-foreground">{schema.description}</p>
        )}
        <Select value={(value as string) || ""} onValueChange={onChange}>
          <SelectTrigger className="text-sm">
            <SelectValue placeholder="Select..." />
          </SelectTrigger>
          <SelectContent>
            {schema.enum.map((opt) => (
              <SelectItem key={opt} value={opt}>
                {opt}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
    );
  }

  // boolean → switch
  if (schema.type === "boolean") {
    return (
      <div className="flex items-center justify-between gap-3 rounded-lg border border-border px-3 py-2.5">
        <div className="space-y-0.5">
          <Label className="text-xs font-medium">{label}</Label>
          {schema.description && (
            <p className="text-[11px] text-muted-foreground">{schema.description}</p>
          )}
        </div>
        <Switch
          checked={Boolean(value)}
          onCheckedChange={onChange}
        />
      </div>
    );
  }

  // number/integer
  if (schema.type === "number" || schema.type === "integer") {
    return (
      <div className="space-y-1.5">
        <Label className="text-xs">
          {label}
          {required && <span className="text-destructive ml-0.5">*</span>}
        </Label>
        {schema.description && (
          <p className="text-[11px] text-muted-foreground">{schema.description}</p>
        )}
        <Input
          type="number"
          value={(value as number) ?? (schema.default as number) ?? ""}
          onChange={(e) => onChange(e.target.value ? Number(e.target.value) : undefined)}
          min={schema.minimum}
          max={schema.maximum}
          step={schema.type === "integer" ? 1 : undefined}
        />
      </div>
    );
  }

  // string (default)
  return (
    <div className="space-y-1.5">
      <Label className="text-xs">
        {label}
        {required && <span className="text-destructive ml-0.5">*</span>}
      </Label>
      {schema.description && (
        <p className="text-[11px] text-muted-foreground">{schema.description}</p>
      )}
      <Input
        type={schema.format === "email" ? "email" : schema.format === "uri" ? "url" : "text"}
        value={(value as string) || ""}
        onChange={(e) => onChange(e.target.value)}
        placeholder={schema.placeholder}
      />
    </div>
  );
}
