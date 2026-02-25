import { useThreadStore } from "../stores/threadStore";
import { eventBus } from "../runtime/event-bus";
import { invokeToolCall } from "../api/client";
import { snowflake } from "./snowflake";
import { consumeStream } from "./stream-consumer";

/**
 * Client-initiated tool invocation.
 *
 * Adapted from the MCP Apps `visibility: ["app"]` pattern — the tool is
 * hidden from the model's tool list but callable by the UI. The server
 * executes the tool, injects a synthetic assistant ToolCall + result into
 * the conversation, then starts a new agent turn so the model processes
 * the result naturally.
 *
 * The UI streams the model's response in real-time, then reloads the
 * full message tree to include the synthetic tool call.
 */
export async function invokeClientTool(
  conversationId: string,
  toolName: string,
  args: Record<string, unknown>,
): Promise<void> {
  const assistantMsgId = snowflake();
  useThreadStore.getState().startStreaming(conversationId, assistantMsgId);

  await eventBus.ensureConnected();
  const controller = new AbortController();

  try {
    await invokeToolCall(conversationId, toolName, args, assistantMsgId);
  } catch (err: unknown) {
    console.error("Tool invoke failed:", err);
    useThreadStore.getState().finalizeStreaming(conversationId, {
      type: "incomplete",
      reason: "error",
      error: String(err),
    });
    return;
  }

  // Consume stream for real-time rendering of the model's response.
  // No turnCtx — server handles all message persistence for tool invocations.
  await consumeStream(conversationId, controller.signal);

  // Reload full message tree from server to pick up the synthetic tool call
  // and ensure the repository/branch data is consistent.
  useThreadStore.getState().loadHistory(conversationId);
}
