import type {
  ChatModelAdapter,
  ChatModelRunOptions,
  ChatModelRunResult,
  ThreadMessage,
} from "@assistant-ui/react";
import type { ThreadAssistantMessagePart } from "@assistant-ui/react";
import type { WireMessage } from "@/api/client.js";
import { streamChat, postToolResult, fetchToolSettings } from "@/api/client.js";
import { useChatStore } from "@/stores/chatStore.js";
import { threadState } from "./thread-list-adapter.js";

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
    async *run({ messages, abortSignal }: ChatModelRunOptions): AsyncGenerator<ChatModelRunResult, void> {
      const agentId = useChatStore.getState().activeAgentId;
      const conversationId = threadState.activeConversationId;

      // Refresh hidden patterns
      fetchToolSettings()
        .then((s) => { cachedHiddenPatterns = s.hiddenToolPatterns; })
        .catch(() => {});

      // Send the full active-branch history to the server
      const wireMessages = toWireMessages(messages);

      const parts: ThreadAssistantMessagePart[] = [];

      function buildContent(): ThreadAssistantMessagePart[] {
        return parts.filter((p) => {
          if (p.type === "tool-call" && matchHiddenPattern(p.toolName)) {
            return false;
          }
          return true;
        });
      }

      try {
        for await (const event of streamChat(conversationId, wireMessages, agentId)) {
          if (abortSignal.aborted) break;

          const data = event.data as Record<string, unknown>;

          switch (event.event) {
            case "text_start": {
              parts.push({ type: "text" as const, text: "" });
              break;
            }

            case "text_delta": {
              const chunk = (data.text as string) || "";
              // Append to the most recent text part, or create one if none exists
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
              // Update the most recent tool-call's argsText with streaming JSON
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

              // Show the tool call in the UI
              parts.push({
                type: "tool-call" as const,
                toolCallId: toolUseId,
                toolName,
                args: input as any,
                argsText: JSON.stringify(input),
              });
              yield { content: buildContent() };

              // Execute locally and POST result back
              executeFrontendTool(toolName, input)
                .then(({ content: result, isError }) => postToolResult(toolUseId, result, isError))
                .catch((err) => postToolResult(toolUseId, String(err), true));
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
              // Update the active conversation ID if the server assigned one
              const returnedId = data.conversationId as string | undefined;
              if (returnedId) {
                threadState.activeConversationId = returnedId;
              }
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
