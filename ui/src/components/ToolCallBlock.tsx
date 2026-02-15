import { useState } from "react";
import { ChevronRight, Wrench, AlertCircle, CheckCircle2, Loader2 } from "lucide-react";
import type { ToolCallInfo } from "@/api/client.js";
import { Badge } from "@/components/ui/badge";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { cn } from "@/lib/utils";

interface Props {
  toolCall: ToolCallInfo;
}

export function ToolCallBlock({ toolCall }: Props) {
  const [open, setOpen] = useState(false);
  const shortName = toolCall.name.split(".").pop() || toolCall.name;
  const isRunning = toolCall.result === undefined;

  return (
    <Collapsible open={open} onOpenChange={setOpen} className="my-2">
      <CollapsibleTrigger className="flex items-center gap-2 w-full px-3 py-2 text-sm rounded-lg border border-border bg-card hover:bg-accent/50 transition-colors">
        <ChevronRight
          size={13}
          className={cn(
            "text-muted-foreground transition-transform duration-200",
            open && "rotate-90"
          )}
        />
        <Wrench size={13} className="text-primary" />
        <Badge
          variant="secondary"
          className="font-mono text-[11px] bg-primary/10 text-primary border-primary/15 px-1.5 py-0"
        >
          {shortName}
        </Badge>
        <span className="flex-1" />
        {isRunning ? (
          <Loader2 size={13} className="text-muted-foreground animate-spin" />
        ) : toolCall.isError ? (
          <AlertCircle size={13} className="text-destructive" />
        ) : (
          <CheckCircle2 size={13} className="text-green-400" />
        )}
      </CollapsibleTrigger>

      <CollapsibleContent>
        <div className="border border-t-0 border-border rounded-b-lg px-3 py-2.5 space-y-2 bg-card/50">
          {Object.keys(toolCall.args).length > 0 && (
            <div>
              <div className="text-[11px] text-muted-foreground mb-1 font-medium">Input</div>
              <pre className="text-xs font-mono bg-background p-2.5 rounded-md overflow-x-auto border border-border">
                {JSON.stringify(toolCall.args, null, 2)}
              </pre>
            </div>
          )}
          {toolCall.result !== undefined && (
            <div>
              <div className="text-[11px] text-muted-foreground mb-1 font-medium">Output</div>
              <pre
                className={cn(
                  "text-xs font-mono p-2.5 rounded-md overflow-x-auto border",
                  toolCall.isError
                    ? "bg-destructive/5 border-destructive/20 text-red-300"
                    : "bg-background border-border"
                )}
              >
                {toolCall.result}
              </pre>
            </div>
          )}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
