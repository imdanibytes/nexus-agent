import { useEffect, useRef } from "react";
import { mcpAppService } from "../../lib/mcp-app-service";

interface McpAppToolResultProps {
  serverId: string;
  resourceUri: string;
  conversationId: string;
  toolCallData?: Record<string, unknown>;
}

export function McpAppToolResult({
  serverId,
  resourceUri,
  conversationId,
  toolCallData,
}: McpAppToolResultProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const instanceIdRef = useRef<string | null>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    let mounted = true;

    mcpAppService
      .mount({ serverId, resourceUri, container, conversationId, toolCallData })
      .then((id) => {
        if (mounted) {
          instanceIdRef.current = id;
        } else {
          // Component unmounted before mount resolved — clean up
          mcpAppService.unmount(id);
        }
      })
      .catch((err) => {
        console.error("[McpAppToolResult] mount failed:", err);
      });

    return () => {
      mounted = false;
      if (instanceIdRef.current) {
        mcpAppService.unmount(instanceIdRef.current);
        instanceIdRef.current = null;
      }
    };
  }, [serverId, resourceUri, conversationId, toolCallData]);

  return (
    <div
      ref={containerRef}
      className="w-full min-h-[60px] rounded-lg overflow-hidden"
    />
  );
}
