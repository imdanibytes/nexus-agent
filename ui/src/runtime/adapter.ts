import type {
  ChatModelAdapter,
  ChatModelRunOptions,
  ChatModelRunResult,
  ThreadMessage,
} from "@assistant-ui/react";
import type { ThreadAssistantMessagePart } from "@assistant-ui/react";
import type { WireMessage } from "@/api/client.js";
import { fetchToolSettings } from "@/api/client.js";
import { useChatStore } from "@/stores/chatStore.js";
import { threadState } from "./thread-list-adapter.js";
import { wsClient } from "./ws-client.js";
import { turnRouter } from "./turn-router.js";
import type { WsMessage } from "./ws-client.js";

/** Cache tool settings for hidden patterns (refreshed per adapter creation) */
let cachedHiddenPatterns: string[] = ["_nexus_*"];
fetchToolSettings()
  .then((s) => { cachedHiddenPatterns = s.hiddenToolPatterns; })
  .catch(() => {});

function matchHiddenPattern(name: string): boolean {
  return cachedHiddenPatterns.some((pattern) => {
    const regex = new RegExp(
      "^" + pattern.replace(/\*/g, ".*").replace(/\?/g, ".") + "$",
    );
    return regex.test(name);
  });
}

/**
 * Convert assistant-ui's ThreadMessage[] (active branch) into wire-format
 * messages the server can convert directly to Anthropic API format.
 */
function toWireMessages(messages: readonly ThreadMessage[]): WireMessage[] {
  return messages
    .filter((m) => m.role === "user" || m.role === "assistant")
    .map((m) => {
      const text = m.content
        .filter((p): p is { type: "text"; text: string } => p.type === "text")
        .map((p) => p.text)
        .join("\n");

      if (m.role === "user") {
        return { role: "user" as const, content: text };
      }

      // Assistant: include tool calls
      const toolCalls = m.content
        .filter((p): p is Extract<(typeof m.content)[number], { type: "tool-call" }> => p.type === "tool-call")
        .map((p) => ({
          id: p.toolCallId,
          name: p.toolName,
          args: (p.args ?? {}) as Record<string, unknown>,
          result: typeof p.result === "string" ? p.result : p.result !== undefined ? JSON.stringify(p.result) : undefined,
          isError: p.isError,
        }));

      return {
        role: "assistant" as const,
        content: text,
        toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
      };
    });
}

export function createNexusAdapter(
  onTitleUpdate: (id: string, title: string) => void,
): ChatModelAdapter {
  return {
    async *run({ messages, abortSignal, runConfig }: ChatModelRunOptions): AsyncGenerator<ChatModelRunResult, void> {
      const mcpConvId = (runConfig?.custom as Record<string, unknown> | undefined)?.mcpConversationId as string | undefined;
      const conversationId = mcpConvId || threadState.activeConversationId;

      // Refresh hidden patterns
      fetchToolSettings()
        .then((s) => { cachedHiddenPatterns = s.hiddenToolPatterns; })
        .catch(() => {});

      // For user-initiated turns, send the start_turn message over WebSocket
      if (!mcpConvId) {
        const agentId = useChatStore.getState().activeAgentId;
        const wireMessages = toWireMessages(messages);

        wsClient.send({
          type: "start_turn",
          conversationId: conversationId || undefined,
          data: {
            messages: wireMessages,
            agentId: agentId || undefined,
          } as unknown as Record<string, unknown>,
        });
      }

      // Both user and MCP turns consume from the same TurnRouter stream.
      // For MCP turns, events are already being routed there by App.tsx.
      // For user turns, the server will start sending turn events after start_turn.
      if (!conversationId) {
        yield {
          content: [{ type: "text" as const, text: "No active conversation." }],
          status: { type: "incomplete" as const, reason: "error" as const, error: "No conversation ID" as any },
        };
        return;
      }

      const stream = turnRouter.createTurnStream(conversationId);
      const parts: ThreadAssistantMessagePart[] = [];

      function buildContent(): ThreadAssistantMessagePart[] {
        return parts.filter((p) => {
          if (p.type === "tool-call" && matchHiddenPattern(p.toolName)) {
            return false;
          }
          return true;
        });
      }

      // Handle abort
      const onAbort = () => {
        wsClient.send({
          type: "abort_turn",
          conversationId,
        });
        turnRouter.abort(conversationId);
      };
      if (abortSignal.aborted) {
        onAbort();
        return;
      }
      abortSignal.addEventListener("abort", onAbort, { once: true });

      try {
        for await (const msg of stream) {
          if (abortSignal.aborted) break;

          const data = msg.data || {};

          switch (msg.type) {
            case "text_start": {
              parts.push({ type: "text" as const, text: "" });
              break;
            }

            case "text_delta": {
              const chunk = (data.text as string) || "";
              let found = false;
              for (let i = parts.length - 1; i >= 0; i--) {
                if (parts[i].type === "text") {
                  (parts[i] as { type: "text"; text: string }).text += chunk;
                  found = true;
                  break;
                }
              }
              if (!found) {
                parts.push({ type: "text" as const, text: chunk });
              }
              yield { content: buildContent() };
              break;
            }

            case "tool_start": {
              parts.push({
                type: "tool-call" as const,
                toolCallId: data.id as string,
                toolName: data.name as string,
                args: {} as any,
                argsText: "{}",
              });
              yield { content: buildContent() };
              break;
            }

            case "tool_input_delta": {
              for (let i = parts.length - 1; i >= 0; i--) {
                const p = parts[i];
                if (p.type === "tool-call") {
                  const partial = (data.partial_json as string) || "";
                  const existing = p as ThreadAssistantMessagePart & { type: "tool-call" };
                  const currentText = existing.argsText || "";
                  parts[i] = { ...existing, argsText: currentText + partial };
                  break;
                }
              }
              yield { content: buildContent() };
              break;
            }

            case "tool_result": {
              const toolIdx = parts.findIndex(
                (p) => p.type === "tool-call" && p.toolCallId === data.id,
              );
              if (toolIdx !== -1) {
                const existing = parts[toolIdx] as ThreadAssistantMessagePart & { type: "tool-call" };
                parts[toolIdx] = {
                  ...existing,
                  result: data.content as string,
                  isError: (data.is_error as boolean) || false,
                };
              }
              yield { content: buildContent() };
              break;
            }

            case "ui_surface": {
              parts.push({
                type: "tool-call" as const,
                toolCallId: data.tool_use_id as string,
                toolName: data.name as string,
                args: data.input as any,
                argsText: JSON.stringify(data.input),
              });
              yield {
                content: buildContent(),
                status: { type: "requires-action" as const, reason: "tool-calls" as const },
              };
              break;
            }

            case "tool_request": {
              const toolUseId = data.tool_use_id as string;
              const toolName = data.name as string;
              const input = (data.input ?? {}) as Record<string, unknown>;

              parts.push({
                type: "tool-call" as const,
                toolCallId: toolUseId,
                toolName,
                args: input as any,
                argsText: JSON.stringify(input),
              });
              yield { content: buildContent() };

              // Execute locally and send result back over WebSocket
              executeFrontendTool(toolName, input)
                .then(({ content: result, isError }) => {
                  wsClient.send({
                    type: "tool_result",
                    conversationId,
                    data: { toolUseId, content: result, isError },
                  });
                })
                .catch((err) => {
                  wsClient.send({
                    type: "tool_result",
                    conversationId,
                    data: { toolUseId, content: String(err), isError: true },
                  });
                });
              break;
            }

            case "title_update": {
              const title = data.title as string;
              if (title && conversationId) {
                onTitleUpdate(conversationId, title);
              }
              break;
            }

            case "timing": {
              const spans = data.spans as import("@/stores/chatStore.js").TimingSpan[];
              if (spans) {
                yield {
                  content: buildContent(),
                  metadata: { custom: { timingSpans: spans } },
                };
              }
              break;
            }

            case "turn_end": {
              const returnedId = data.conversationId as string | undefined;
              if (returnedId) {
                threadState.activeConversationId = returnedId;
              }
              // Turn is done — the stream will end via turnRouter
              turnRouter.endTurn(conversationId);
              break;
            }

            case "error": {
              console.error("Stream error:", data.message);
              yield {
                content: buildContent().length > 0
                  ? buildContent()
                  : [{ type: "text" as const, text: "An error occurred." }],
                status: {
                  type: "incomplete" as const,
                  reason: "error" as const,
                  error: (data.message as any) ?? undefined,
                },
              };
              return;
            }
          }
        }
      } catch (err) {
        console.error("Chat error:", err);
        yield {
          content: buildContent().length > 0
            ? buildContent()
            : [{ type: "text" as const, text: "Connection error." }],
          status: {
            type: "incomplete" as const,
            reason: "error" as const,
            error: String(err) as any,
          },
        };
      } finally {
        abortSignal.removeEventListener("abort", onAbort);
      }
    },
  };
}

async function executeFrontendTool(
  name: string,
  input: Record<string, unknown>,
): Promise<{ content: string; isError: boolean }> {
  switch (name) {
    case "_nexus_clipboard_read": {
      try {
        const text = await navigator.clipboard.readText();
        return { content: text, isError: false };
      } catch (err) {
        return { content: `Clipboard read failed: ${err}`, isError: true };
      }
    }
    case "_nexus_clipboard_write": {
      try {
        await navigator.clipboard.writeText((input.text as string) || "");
        return { content: "Written to clipboard", isError: false };
      } catch (err) {
        return { content: `Clipboard write failed: ${err}`, isError: true };
      }
    }
    default:
      return { content: `Unknown frontend tool: ${name}`, isError: true };
  }
}
