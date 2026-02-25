import type { FC } from "react";
import { Chip } from "@heroui/react";
import type { AgentMode } from "../../types/tasks";

const MODE_CONFIG: Record<
  AgentMode,
  { label: string; color: "default" | "warning" | "primary" | "success" | "secondary" }
> = {
  general: { label: "General", color: "default" },
  discovery: { label: "Discovery", color: "warning" },
  planning: { label: "Planning", color: "primary" },
  execution: { label: "Executing", color: "success" },
  validation: { label: "Validating", color: "secondary" },
};

export const ModeChip: FC<{ mode: AgentMode }> = ({ mode }) => {
  const config = MODE_CONFIG[mode];
  return (
    <Chip size="sm" variant="flat" color={config.color} className="text-[10px] h-5">
      {config.label}
    </Chip>
  );
};
