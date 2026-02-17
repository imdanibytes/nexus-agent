import { v4 as uuidv4 } from "uuid";
import { listAgents, getAgent, getActiveAgentId } from "./agents.js";
import {
  listConversations,
  getConversation,
  saveConversation,
} from "./storage.js";
import { runAgentTurn, type WireMessage } from "./agent.js";
import { hub } from "./sse-handler.js";
import { EventType } from "./ag-ui-types.js";
import type { CollectedEvent } from "./streaming.js";
import type { Message, MessagePart } from "./types.js";

interface McpCallRequest {
  tool_name: string;
  arguments: Record<string, unknown>;
}

interface McpContent {
  type: string;
  text: string;
}

interface McpCallResponse {
  content: McpContent[];
  is_error: boolean;
}

function ok(text: string): McpCallResponse {
  return { content: [{ type: "text", text }], is_error: false };
}

function err(text: string): McpCallResponse {
  return { content: [{ type: "text", text }], is_error: true };
}

/**
 * Convert stored Message[] (parts format) to WireMessage[] for the LLM.
 */
function messagesToWire(messages: Message[]): WireMessage[] {
  const wire: WireMessage[] = [];

  for (const msg of messages) {
    const textParts = msg.parts.filter((p) => p.type === "text") as {
      type: "text";
      text: string;
    }[];
    const toolParts = msg.parts.filter((p) => p.type === "tool-call") as {
      type: "tool-call";
      id: string;
      name: string;
      args: Record<string, unknown>;
      result?: string;
      isError?: boolean;
    }[];

    const content = textParts.map((p) => p.text).join("");

    wire.push({
      role: msg.role,
      content,
      toolCalls:
        toolParts.length > 0
          ? toolParts.map((tc) => ({
              id: tc.id,
              name: tc.name,
              args: tc.args,
              result: tc.result,
              isError: tc.isError,
            }))
          : undefined,
    });
  }

  return wire;
}

/**
 * Build a readable text response from collected SSE events for the MCP caller.
 */
function buildMcpResponse(events: CollectedEvent[]): McpCallResponse {
  const chunks: string[] = [];
  let error: string | undefined;

  for (const ev of events) {
    const d = ev.data as Record<string, unknown>;

    switch (ev.type) {
      case EventType.TEXT_MESSAGE_CONTENT:
        chunks.push(d.delta as string);
        break;
      case EventType.RUN_ERROR:
        error = d.message as string;
        break;
    }
  }

  const text = chunks.join("");

  if (error && !text) {
    return err(error);
  }

  if (!text && !error) {
    return err("Agent produced no response");
  }

  return ok(text);
}

// ---------- Tool handlers ----------

function handleListAgents(): McpCallResponse {
  const agents = listAgents();
  const summary = agents.map((a) => ({
    id: a.id,
    name: a.name,
    model: a.model,
  }));
  return ok(JSON.stringify(summary, null, 2));
}

function handleGetAgent(args: Record<string, unknown>): McpCallResponse {
  const id = args.agent_id as string;
  if (!id) return err("agent_id is required");

  const agent = getAgent(id);
  if (!agent) return err(`Agent '${id}' not found`);

  return ok(
    JSON.stringify(
      {
        id: agent.id,
        name: agent.name,
        model: agent.model,
        providerId: agent.providerId,
        systemPrompt: agent.systemPrompt,
        temperature: agent.temperature,
        maxTokens: agent.maxTokens,
        topP: agent.topP,
        createdAt: agent.createdAt,
        updatedAt: agent.updatedAt,
      },
      null,
      2,
    ),
  );
}

function handleListConversations(): McpCallResponse {
  const convs = listConversations();
  const summary = convs.map((c) => ({
    id: c.id,
    title: c.title,
    messageCount: c.messageCount,
    createdAt: new Date(c.createdAt).toISOString(),
    updatedAt: new Date(c.updatedAt).toISOString(),
  }));
  return ok(JSON.stringify(summary, null, 2));
}

function handleGetConversation(args: Record<string, unknown>): McpCallResponse {
  const id = args.conversation_id as string;
  if (!id) return err("conversation_id is required");

  const conv = getConversation(id);
  if (!conv) return err(`Conversation '${id}' not found`);

  const messages = conv.messages.map((m) => {
    const textParts = m.parts
      .filter((p) => p.type === "text")
      .map((p) => (p as { type: "text"; text: string }).text)
      .join("");

    const toolParts = m.parts
      .filter((p) => p.type === "tool-call")
      .map((p) => {
        const tc = p as {
          type: "tool-call";
          id: string;
          name: string;
          args: Record<string, unknown>;
          result?: string;
          isError?: boolean;
        };
        return {
          name: tc.name,
          args: tc.args,
          result: tc.result,
          isError: tc.isError,
        };
      });

    return {
      role: m.role,
      text: textParts || undefined,
      toolCalls: toolParts.length > 0 ? toolParts : undefined,
      timestamp: new Date(m.timestamp).toISOString(),
    };
  });

  return ok(
    JSON.stringify(
      {
        id: conv.id,
        title: conv.title,
        createdAt: new Date(conv.createdAt).toISOString(),
        updatedAt: new Date(conv.updatedAt).toISOString(),
        messages,
      },
      null,
      2,
    ),
  );
}

async function handleSendMessage(
  args: Record<string, unknown>,
): Promise<McpCallResponse> {
  const message = args.message as string;
  if (!message) return err("message is required");

  const conversationId = (args.conversation_id as string) || uuidv4();
  const agentId = (args.agent_id as string) || undefined;

  // Load or create conversation
  let conv = getConversation(conversationId);
  if (!conv) {
    conv = {
      id: conversationId,
      title: message.slice(0, 60) + (message.length > 60 ? "..." : ""),
      createdAt: Date.now(),
      updatedAt: Date.now(),
      messages: [],
    };
    saveConversation(conv);
  }

  const wireMessages = messagesToWire(conv.messages);
  wireMessages.push({ role: "user", content: message });

  // Notify UI that an MCP turn is starting
  hub.push({
    type: EventType.CUSTOM,
    name: "mcp_turn_pending",
    value: { conversationId, userMessage: message, agentId },
  });

  // Create collecting writer that also broadcasts to SSE clients
  const collector = hub.createCollectingWriter(conversationId);

  try {
    await runAgentTurn(conversationId, wireMessages, collector, agentId);
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    hub.push({
      type: EventType.CUSTOM,
      name: "conversations_changed",
      value: { conversationId },
    });
    return err(msg);
  }

  const response = buildMcpResponse(collector.events);

  // Persist: append user message + assistant response
  const now = Date.now();

  const userMsg: Message = {
    id: uuidv4(),
    role: "user",
    parts: [{ type: "text", text: message }],
    timestamp: now,
  };

  const assistantParts: MessagePart[] = [];
  let assistantText = "";
  for (const ev of collector.events) {
    const d = ev.data as Record<string, unknown>;
    if (ev.type === EventType.TEXT_MESSAGE_CONTENT) {
      assistantText += d.delta as string;
    }
  }
  if (assistantText) {
    assistantParts.push({ type: "text", text: assistantText });
  }

  const resolvedAgentId = agentId || getActiveAgentId();
  const agent = resolvedAgentId ? getAgent(resolvedAgentId) : null;

  const assistantMsg: Message = {
    id: uuidv4(),
    role: "assistant",
    parts: assistantParts.length > 0 ? assistantParts : [{ type: "text", text: "" }],
    timestamp: Date.now(),
    ...(agent ? { profileId: agent.id, profileName: agent.name } : {}),
  };

  conv = getConversation(conversationId) || conv;
  conv.messages.push(userMsg, assistantMsg);
  conv.updatedAt = Date.now();
  saveConversation(conv);

  hub.push({
    type: EventType.CUSTOM,
    name: "conversations_changed",
    value: { conversationId },
  });

  if (!response.is_error) {
    const text = response.content[0].text;
    response.content[0].text = JSON.stringify(
      {
        conversation_id: conversationId,
        response: text,
      },
      null,
      2,
    );
  }

  return response;
}

// ---------- Dispatcher ----------

export async function handleMcpCall(
  body: McpCallRequest,
): Promise<McpCallResponse> {
  const { tool_name, arguments: args } = body;

  switch (tool_name) {
    case "list_agents":
      return handleListAgents();
    case "get_agent":
      return handleGetAgent(args);
    case "list_conversations":
      return handleListConversations();
    case "get_conversation":
      return handleGetConversation(args);
    case "send_message":
      return await handleSendMessage(args);
    default:
      return err(`Unknown tool: ${tool_name}`);
  }
}
