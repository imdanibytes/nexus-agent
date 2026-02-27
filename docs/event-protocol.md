# Event Protocol Spec

Contract between the Nexus daemon and the UI for real-time communication.
Every event listed here must have: a backend emitter, a wire format test,
and a frontend consumer. If any column says "none", that's either a bug
or documented as intentional below.

## Wire Format

All events travel as SSE `data:` lines over `GET /api/events`. Each line
is a JSON-serialized `EventEnvelope`:

```json
{
  "type": "CUSTOM",
  "threadId": "conv-abc-123",
  "runId": "run-def-456",
  "name": "title_update",
  "value": { "id": "conv-abc-123", "title": "New Title" }
}
```

### Envelope fields

| Field | Type | Present | Description |
|-------|------|---------|-------------|
| `type` | string | always | Discriminator (see tables below) |
| `threadId` | string? | turn-scoped + thread-scoped events | Conversation ID. Absent on global events. |
| `runId` | string? | turn-scoped events | Turn ID. Absent on service/system events. |

Additional fields depend on `type`.

### Serialization

`AgUiEvent` uses `#[serde(tag = "type")]`. `EventEnvelope` uses `#[serde(flatten)]`
to merge routing metadata with event fields into a single flat JSON object.
Serialization tests in `crates/nexus-daemon/src/agent/events.rs` lock the wire format.

---

## 1. Turn-Scoped Streaming Events

Emitted by `TurnEmitter` during an active agent turn. Routed to the
per-conversation async iterable in `event-bus.ts`.

| Wire `type` | Emitter method | Additional fields | UI consumer |
|-------------|---------------|-------------------|-------------|
| `RUN_STARTED` | `emitter.run_started()` | — | `event-bus.ts` routes to stream; `useStreamBroadcasts.ts` auto-consumes |
| `RUN_FINISHED` | `emitter.run_finished(has)` | `hasRunningProcesses: bool` | `stream-consumer.ts` ends subscription |
| `RUN_ERROR` | `emitter.run_error(msg, details)` | `message: string`, `details?: { kind, message, status_code?, retryable, provider }` | `stream-consumer.ts` finalizes with error |
| `TEXT_MESSAGE_START` | `emitter.text_start(id)` | `messageId: string` | `stream-consumer.ts` pushes text part |
| `TEXT_MESSAGE_CONTENT` | `emitter.text_delta(id, delta)` | `messageId: string`, `delta: string` | `stream-consumer.ts` appends delta |
| `TEXT_MESSAGE_END` | `emitter.text_end(id)` | `messageId: string` | `stream-consumer.ts` (implicit) |
| `TOOL_CALL_START` | `emitter.tool_start(id, name)` | `toolCallId: string`, `toolCallName: string` | `stream-consumer.ts` pushes tool-call part |
| `TOOL_CALL_ARGS` | `emitter.tool_args(id, delta)` | `toolCallId: string`, `delta: string` | `stream-consumer.ts` appends args delta |
| `TOOL_CALL_END` | `emitter.tool_end(id)` | `toolCallId: string` | `stream-consumer.ts` (received, no action) |
| `TOOL_CALL_RESULT` | `emitter.tool_result(id, content, err)` | `toolCallId: string`, `content: string`, `isError: bool` | `stream-consumer.ts` sets result |

---

## 2. Turn-Scoped CUSTOM Events

Wire `type` is `CUSTOM` with a `name` field. Routed to the turn stream
AND to broadcast handlers.

| `name` | Emitter | Payload (`value`) | UI consumer |
|--------|---------|-------------------|-------------|
| `thinking_start` | `TurnEmitter.thinking_start()` | `{}` | `stream-consumer.ts` pushes thinking part |
| `thinking_delta` | `TurnEmitter.thinking_delta(d)` | `{ delta: string }` | `stream-consumer.ts` appends delta |
| `thinking_end` | `TurnEmitter.thinking_end()` | `{}` | `stream-consumer.ts` clears activity |
| `usage_update` | `TurnEmitter.usage(...)` | `{ inputTokens, outputTokens, cacheReadInputTokens, cacheCreationInputTokens, contextWindow, totalCost }` | `useStreamBroadcasts.ts` → usageStore |
| `compaction` | `TurnEmitter.compaction(idx, n)` | `{ sealed_span_index, consumed_count }` | `useStreamBroadcasts.ts` reloads history |
| `timing` | `TurnEmitter.timing(spans)` | `{ spans: TimingSpan[] }` | `stream-consumer.ts` stores in metadata |
| `task_state_changed` | `TaskService.emit_changed()` | `{ conversationId, plan, tasks, mode }` | `stream-consumer.ts` → taskStore |
| `ask_user_pending` | tool dispatch in `agent/tool_dispatch.rs` | `{ questionId, toolCallId, question, type, options?, context?, placeholder? }` | `stream-consumer.ts` → questionStore |
| `ask_user_answered` | tool dispatch in `agent/tool_dispatch.rs` | `{ toolCallId }` | `stream-consumer.ts` removes question |
| `activity_update` | `TurnEmitter.activity(desc)` | `{ activity: string }` | **not consumed** (see Unconsumed Events) |
| `retry` | `TurnEmitter.retry(...)` | `{ attempt, maxAttempts, reason, delayMs }` | **not consumed** |
| `sub_agent_start` | `TurnEmitter.sub_agent_start(...)` | `{ agent_type, task, context }` | **not consumed** |
| `sub_agent_end` | `TurnEmitter.sub_agent_end(...)` | `{ agent_type, ...result }` | **not consumed** |

---

## 3. Broadcast Events (Service Mutations)

Wire `type` is `CUSTOM`. No `runId`. Dispatched to broadcast handlers only.

### Thread events (thread-scoped: `threadId` = conversation ID)

| `name` | Emitter | Payload | UI handler (`useStreamBroadcasts.ts`) |
|--------|---------|---------|---------------------------------------|
| `thread_created` | `ThreadService.create()` | `{ id, title }` | Reloads thread list |
| `thread_deleted` | `ThreadService.delete()` | `{ id }` | Removes thread from list |
| `title_update` | `ThreadService.rename()` | `{ id, title }` | Updates thread title |
| `message_added` | `ThreadService.add_message()` / `add_messages()` | `{ id }` | **not consumed** |
| `thread_updated` | `ThreadService.commit()` | `{ id }` | **not consumed** |

### Agent events (global: no `threadId`)

| `name` | Emitter | Payload | UI handler |
|--------|---------|---------|------------|
| `agent_created` | `AgentService.create()` | Full AgentEntry JSON | Reloads agents |
| `agent_updated` | `AgentService.update()` | Full AgentEntry JSON | Reloads agents |
| `agent_deleted` | `AgentService.delete()` | `{ id }` | Reloads agents |
| `active_agent_changed` | `AgentService.set_active()` | `{ agent_id }` | Sets activeAgentId |

### Provider events (global: no `threadId`)

| `name` | Emitter | Payload | UI handler |
|--------|---------|---------|------------|
| `provider_created` | `ProviderService.create()` | Full Provider JSON | Reloads providers |
| `provider_updated` | `ProviderService.update()` | Full Provider JSON | Reloads providers |
| `provider_deleted` | `ProviderService.delete()` | `{ id }` | Reloads providers |

### Workspace events (thread-scoped: `threadId` = workspace ID)

| `name` | Emitter | Payload | UI handler |
|--------|---------|---------|------------|
| `workspace_created` | `workspace_api::create()` | `{ id, name }` | Reloads workspaces |
| `workspace_updated` | `workspace_api::update()` | `{ id, name }` | Reloads workspaces |
| `workspace_deleted` | `workspace_api::delete()` | `{ id }` | Reloads workspaces |

### Background process events (thread-scoped: `threadId` = conversation ID)

| `name` | Emitter | Payload | UI handler |
|--------|---------|---------|------------|
| `bg_process_started` | `ProcessManager` | Full BgProcess JSON | Adds process |
| `bg_process_completed` | `ProcessManager` | Full BgProcess JSON | Updates process |
| `bg_process_cancelled` | `ProcessManager` | Full BgProcess JSON | Updates process (status=cancelled) |

---

## 4. System Events

| Wire `type` | When | Payload | UI consumer |
|-------------|------|---------|-------------|
| `SYNC` | On SSE connection open | `{ activeRuns: string[] }` | `useStreamBroadcasts.ts` auto-consumes active runs |

---

## Unconsumed Events

These events are emitted but have no UI handler. Each is intentional:

| Event | Reason |
|-------|--------|
| `message_added` | UI reloads full history after turn; granular add not needed yet |
| `thread_updated` | Commit-based mutations; UI reloads history when turn ends |
| `activity_update` | Activity text set directly by stream-consumer inline (e.g., "Using bash...") |
| `retry` | No retry UI yet |
| `sub_agent_start` / `sub_agent_end` | Sub-agent UI not implemented yet |

If you add a consumer for any of these, add an integration test.

---

## Adding a New Event: Checklist

### Backend

- [ ] **Define the event name** — add it to the appropriate table in this document
- [ ] **Choose emission path:**
  - Turn-scoped streaming → add method to `TurnEmitter` (`src/agent/emitter.rs`)
  - Service mutation → call `EventBus.emit_data()` or `emit_global()` in the service
  - Background process → emit via `agent_tx.send()` in `ProcessManager`
- [ ] **Add wire format test** — serialization test in the emitter's `#[cfg(test)]` module
- [ ] **Add integration test** in `crates/nexus-daemon-tests/src/tests/event_emission.rs`:
  - Trigger the mutation via HTTP
  - Assert the SSE event arrives with correct `type`, `name`, and payload
- [ ] **Update this document** — add a row to the appropriate table

### Frontend

- [ ] **Choose consumption path:**
  - Turn-scoped (streaming during agent run) → handle in `stream-consumer.ts` CUSTOM switch
  - Broadcast (cross-tab sync, non-turn) → register handler in `useStreamBroadcasts.ts`
  - Both paths fire automatically for CUSTOM events with a `threadId`
- [ ] **Register handler** — `eventBus.on("event_name", handler)` for broadcasts
- [ ] **Update Zustand store** — handler should update the relevant store
- [ ] **Update this document** — fill in the UI consumer column

---

## Naming Conventions

| Category | Convention | Examples |
|----------|-----------|----------|
| Turn-scoped streaming | `SCREAMING_SNAKE_CASE` | `RUN_STARTED`, `TEXT_MESSAGE_CONTENT` |
| Service/broadcast events | `snake_case` | `thread_created`, `agent_updated` |
| Custom turn events | `snake_case` | `thinking_start`, `usage_update` |

Event names must **never** contain the `data:` prefix — that was a previous bug
that made 15 events invisible to the UI.

## Common Mistakes

1. **Emit without consume** — Adding `emit_data()` in a service without a handler
   in `useStreamBroadcasts.ts`. The event arrives on the wire and is silently dropped.

2. **Name mismatch** — Backend emits `"thread_created"` but frontend listens for
   `"threadCreated"`. Names are not transformed — use the exact string.

3. **Missing threadId** — Using `emit_global()` when the event should be thread-scoped.
   Thread-scoped CUSTOM events route to both broadcast handlers AND the turn stream.
   Global events only reach broadcast handlers.

4. **No integration test** — Unit tests verify serialization but not the full HTTP → SSE
   path. The `event_emission.rs` integration tests catch name mismatches that unit tests miss.

## Test Coverage

| Test file | What it verifies |
|-----------|-----------------|
| `crates/nexus-daemon/src/agent/events.rs` | Wire format: JSON shape of every AgUiEvent variant |
| `crates/nexus-daemon/src/agent/emitter.rs` | TurnEmitter methods produce correct event types + fields |
| `crates/nexus-daemon/src/event_bus.rs` | EventBus emits clean names (no `data:` prefix) |
| `crates/nexus-daemon/src/server/sse.rs` | AgentEventBridge buffer/replay lifecycle |
| `crates/nexus-daemon-tests/src/tests/event_emission.rs` | Full HTTP → SSE roundtrip for all service mutation events |
