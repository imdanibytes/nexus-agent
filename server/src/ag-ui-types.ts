/**
 * AG-UI Protocol event types and payload interfaces.
 *
 * These follow the AG-UI specification (https://docs.ag-ui.com/concepts/events)
 * but are defined locally to avoid pulling in @ag-ui/core (rxjs, zod transitive deps).
 */

// ── Event Type Enum ──

export const EventType = {
  // Lifecycle
  RUN_STARTED: "RUN_STARTED",
  RUN_FINISHED: "RUN_FINISHED",
  RUN_ERROR: "RUN_ERROR",
  STEP_STARTED: "STEP_STARTED",
  STEP_FINISHED: "STEP_FINISHED",

  // Text messages
  TEXT_MESSAGE_START: "TEXT_MESSAGE_START",
  TEXT_MESSAGE_CONTENT: "TEXT_MESSAGE_CONTENT",
  TEXT_MESSAGE_END: "TEXT_MESSAGE_END",

  // Tool calls
  TOOL_CALL_START: "TOOL_CALL_START",
  TOOL_CALL_ARGS: "TOOL_CALL_ARGS",
  TOOL_CALL_END: "TOOL_CALL_END",
  TOOL_CALL_RESULT: "TOOL_CALL_RESULT",

  // State
  STATE_SNAPSHOT: "STATE_SNAPSHOT",
  STATE_DELTA: "STATE_DELTA",
  MESSAGES_SNAPSHOT: "MESSAGES_SNAPSHOT",

  // Custom / extension
  CUSTOM: "CUSTOM",
} as const;

export type EventType = (typeof EventType)[keyof typeof EventType];

// ── Base Event ──

export interface BaseEvent {
  type: EventType;
  threadId?: string;
  runId?: string;
  timestamp?: number;
}

// ── Lifecycle Events ──

export interface RunStartedEvent extends BaseEvent {
  type: typeof EventType.RUN_STARTED;
  threadId: string;
  runId: string;
}

export interface RunFinishedEvent extends BaseEvent {
  type: typeof EventType.RUN_FINISHED;
  threadId: string;
  runId: string;
  result?: {
    /** When the run ended because frontend tools need execution */
    pendingToolCalls?: PendingToolCall[];
    /** Server-side tool results already computed this round */
    resolvedToolResults?: ResolvedToolResult[];
  };
}

export interface PendingToolCall {
  toolCallId: string;
  toolCallName: string;
  args: Record<string, unknown>;
}

export interface ResolvedToolResult {
  toolCallId: string;
  content: string;
  isError: boolean;
}

export interface RunErrorEvent extends BaseEvent {
  type: typeof EventType.RUN_ERROR;
  message: string;
  code?: string;
}

export interface StepStartedEvent extends BaseEvent {
  type: typeof EventType.STEP_STARTED;
  stepName: string;
}

export interface StepFinishedEvent extends BaseEvent {
  type: typeof EventType.STEP_FINISHED;
  stepName: string;
}

// ── Text Message Events ──

export interface TextMessageStartEvent extends BaseEvent {
  type: typeof EventType.TEXT_MESSAGE_START;
  messageId: string;
  role: "assistant";
}

export interface TextMessageContentEvent extends BaseEvent {
  type: typeof EventType.TEXT_MESSAGE_CONTENT;
  messageId: string;
  delta: string;
}

export interface TextMessageEndEvent extends BaseEvent {
  type: typeof EventType.TEXT_MESSAGE_END;
  messageId: string;
}

// ── Tool Call Events ──

export interface ToolCallStartEvent extends BaseEvent {
  type: typeof EventType.TOOL_CALL_START;
  toolCallId: string;
  toolCallName: string;
  parentMessageId?: string;
}

export interface ToolCallArgsEvent extends BaseEvent {
  type: typeof EventType.TOOL_CALL_ARGS;
  toolCallId: string;
  delta: string;
}

export interface ToolCallEndEvent extends BaseEvent {
  type: typeof EventType.TOOL_CALL_END;
  toolCallId: string;
}

export interface ToolCallResultEvent extends BaseEvent {
  type: typeof EventType.TOOL_CALL_RESULT;
  toolCallId: string;
  content: string;
  isError?: boolean;
}

// ── Custom Events ──

export interface CustomEvent extends BaseEvent {
  type: typeof EventType.CUSTOM;
  name: string;
  value: unknown;
}

// ── Union ──

export type AgUiEvent =
  | RunStartedEvent
  | RunFinishedEvent
  | RunErrorEvent
  | StepStartedEvent
  | StepFinishedEvent
  | TextMessageStartEvent
  | TextMessageContentEvent
  | TextMessageEndEvent
  | ToolCallStartEvent
  | ToolCallArgsEvent
  | ToolCallEndEvent
  | ToolCallResultEvent
  | CustomEvent;

// ── Helpers ──

/** Check if a tool is a frontend-executed tool (runs in the browser, not on server). */
export function isFrontendTool(name: string): boolean {
  return name.startsWith("_nexus_");
}
