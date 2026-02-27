# Nexus v0 Architecture

Working reference for how the daemon and UI fit together.

## System Overview

Nexus is a local-first AI agent platform. A Rust daemon (`nexus-daemon`) serves
an HTTP+SSE API. A React/TypeScript UI consumes that API. One daemon, one SSE
connection per browser tab, multiple concurrent agent turns.

```
┌──────────────────────────────────────────────────────────────┐
│  Browser Tab                                                 │
│  ┌──────────────┐  ┌─────────────┐  ┌────────────────────┐  │
│  │ Zustand       │  │ event-bus   │  │ stream-consumer    │  │
│  │ stores        │←─│ (SSE)       │──│ (turn-scoped iter) │  │
│  └──────┬────────┘  └─────┬───────┘  └────────────────────┘  │
│         │ REST            │ EventSource /api/events           │
└─────────┼─────────────────┼──────────────────────────────────┘
          │                 │
┌─────────┼─────────────────┼──────────────────────────────────┐
│  Daemon │                 │                                   │
│  ┌──────▼────────┐  ┌─────▼───────────┐                     │
│  │ Axum Router   │  │ AgentEventBridge │                     │
│  │ (HTTP)        │  │ (SSE + buffer)   │                     │
│  └──────┬────────┘  └─────▲────────────┘                     │
│         │                 │ broadcast::Sender<EventEnvelope>  │
│  ┌──────▼─────────────────┴────────────────────────┐         │
│  │              AppState (Arc<AppState>)            │         │
│  │  ┌───────────────┐  ┌────────────────────┐      │         │
│  │  │ ThreadService  │  │ TurnManager        │      │         │
│  │  │ AgentService   │  │ ProcessManager     │      │         │
│  │  │ ProviderSvc    │  │ MessageQueue       │      │         │
│  │  │ TaskService    │  │ PendingQuestions    │      │         │
│  │  └───────┬────────┘  └───────┬────────────┘      │         │
│  │          │                   │                    │         │
│  │          ▼                   ▼                    │         │
│  │       EventBus ◄──── TurnEmitter                 │         │
│  └─────────────────────────────────────────────────┘         │
└──────────────────────────────────────────────────────────────┘
```

## Services

Each service wraps a store with internal locking, encapsulates reads/writes,
and emits events on mutations via `EventBus`.

| Service | Owns | File | Events emitted |
|---------|------|------|----------------|
| **ThreadService** | Conversation CRUD, cache, persistence | `src/thread/mod.rs` | `thread_created`, `thread_deleted`, `title_update`, `message_added`, `thread_updated` |
| **AgentService** | Agent config CRUD, active agent | `src/agent_config/service.rs` | `agent_created`, `agent_updated`, `agent_deleted`, `active_agent_changed` |
| **ProviderService** | Provider CRUD, cached inference clients | `src/provider/service.rs` | `provider_created`, `provider_updated`, `provider_deleted` |
| **TaskService** | Plan/task state per conversation | `src/tasks/service.rs` | `task_state_changed` |
| **TurnManager** | Active turn tracking, cancellation | `src/server/services.rs` | _(none directly; owns AgentEventBridge)_ |
| **ProcessManager** | Background process lifecycle | `src/bg_process/manager.rs` | `bg_process_started`, `bg_process_completed`, `bg_process_cancelled` |

**Workspace CRUD** lives in API handlers (not a service):
`src/server/workspace_api.rs` emits `workspace_created`, `workspace_updated`,
`workspace_deleted` via `EventBus::emit_data`.

All paths above are relative to `crates/nexus-daemon/`.

## Data Flow: REST Mutation

Example: rename a conversation.

```
HTTP PATCH /api/conversations/{id}  { title: "New Title" }
  → server/conversations.rs handler
  → ThreadService.rename(id, title)
    → store.rename(id, title)          — persists to disk
    → cache.invalidate(id)
    → EventBus.emit_data(id, "title_update", { id, title })
      → broadcast::Sender.send(EventEnvelope { threadId, event: CUSTOM })
        → AgentEventBridge picks up from broadcast channel
          → SSE serializes to JSON, pushes as `data:` line
            → Browser EventSource.onmessage
              → event-bus.ts dispatch()
                → type == "CUSTOM" → broadcastHandlers.get("title_update")
                  → useStreamBroadcasts.ts handler
                    → threadListStore.updateThreadTitle()
```

## Data Flow: Agent Turn (Streaming)

```
HTTP POST /api/chat  { conversationId, message }
  → server/chat.rs start_turn()
  → TurnManager.register_turn()  →  (cancel_token, run_id)
  → spawn_agent_turn()           →  tokio::spawn (background task)
    ├── TurnEmitter::new(agent_tx, thread_id, run_id)
    ├── resolve_agent()           →  provider client
    ├── Assemble tools             (MCP + built-in + task + ask_user + sub_agent + fetch + bash + bg + fs)
    ├── resolve_task_mode()       →  AgentMode + PlanContext
    ├── ToolFilterChain.apply()   →  filtered tools
    ├── SystemPromptBuilder       →  system prompt (cached) + state_update (dynamic)
    ├── compact_context()         →  prune + summarize if near limit
    ├── run_agent_turn()          →  RUN_STARTED ... TEXT_* ... TOOL_* ... RUN_FINISHED
    ├── persist_turn_results()    →  ThreadService.checkout/commit
    ├── auto_title::generate_title()  →  title_update event
    └── finish_turn() + drain_queue_and_follow_up()
```

## Agent Turn Lifecycle

Step-by-step breakdown of `spawn_agent_turn` in `src/server/turn.rs`:

| Step | Action | Key function |
|------|--------|-------------|
| 1 | Register turn, get cancel token + run_id | `TurnManager::register_turn()` |
| 2 | Create TurnEmitter with routing metadata | `TurnEmitter::new()` |
| 3 | Resolve active agent → provider client | `resolve_agent()` |
| 4 | Assemble tools: MCP + tasks + ask_user + sub_agent + fetch + bash + bg + fs | inline in `spawn_agent_turn` |
| 5 | Derive agent mode + plan context | `resolve_task_mode()` |
| 6 | Apply tool filter chain (mode-gated, client-only) | `ToolFilterChain::default_chain().apply()` |
| 7 | Build system prompt: static identity + dynamic state | `SystemPromptBuilder::build_parts()` |
| 8 | Context compaction (prune tool results, LLM summarization) | `compact_context()` |
| 9 | Run agent loop (up to 50 inference rounds) | `agent::run_agent_turn()` |
| 10 | Persist new messages + usage | `persist_turn_results()` |
| 11 | Auto-title generation (best-effort) | `auto_title::generate_title()` |
| 12 | Cleanup, drain queue, spawn follow-ups | `finish_turn()`, `drain_queue_and_follow_up()` |

## System Prompt Assembly

The system prompt is split for prompt caching efficiency:

**Static part** (system prompt, cached by provider):

| Provider | Purpose |
|----------|---------|
| MessageBoundaryProvider | Conversation framing |
| IdentityProvider | Agent name + persona from config |
| SystemInfoProvider | Platform/OS context |
| ModeProvider | Agent mode description (general/planning/execution/validation) |
| WorkflowProvider | Task workflow instructions |
| CorePromptProvider | Main behavioral instructions |
| StateProtocolProvider | Describes the `<state_update>` format |

**Dynamic part** (injected as `<state_update>` user message, not part of system prompt):

| Provider | Data |
|----------|------|
| DatetimeProvider | Current date/time |
| TaskContextProvider | Plan + task state |
| ConversationContextProvider | Title, message count, cost, workspace |

Source: `src/system_prompt/mod.rs` (builder), `src/system_prompt/providers.rs` (implementations).

## Event Infrastructure

All events flow through a single `broadcast::channel<EventEnvelope>`:

```
Services ──emit_data/emit_global──► EventBus ──► broadcast::Sender
TurnEmitter ──emit()──────────────────────────► (same Sender)
ProcessManager ──agent_tx.send()──────────────► (same Sender)

broadcast::Sender ──► AgentEventBridge buffer task (per-turn buffering)
                  ──► AgentEventBridge.subscribe() (SSE stream → browser)
```

- **EventBus** (`src/event_bus.rs`): `emit_data(thread_id, name, value)` for thread-scoped events, `emit_global(name, value)` for global events
- **TurnEmitter** (`src/agent/emitter.rs`): per-turn facade with typed methods (`run_started()`, `text_delta()`, `tool_result()`, etc.)
- **AgentEventBridge** (`src/server/sse.rs`): SSE subscriber lifecycle, per-turn event buffering, SYNC → replay → live stream on reconnect

See [Event Protocol Spec](event-protocol.md) for the full event catalog.

## Key File Locations

### Backend (Rust)

All paths relative to `crates/nexus-daemon/`.

| Concern | Files |
|---------|-------|
| HTTP routes + router | `src/server/mod.rs` |
| Chat endpoint | `src/server/chat.rs` |
| Turn orchestration | `src/server/turn.rs` |
| Agent turn loop | `src/agent/run.rs` |
| Tool dispatch | `src/agent/tool_dispatch.rs` |
| Event types (AgUiEvent) | `src/agent/events.rs` |
| Event emission facade | `src/agent/emitter.rs` |
| Event bus | `src/event_bus.rs` |
| SSE bridge | `src/server/sse.rs` |
| System prompt | `src/system_prompt/mod.rs`, `providers.rs` |
| Context compaction | `src/compaction/mod.rs`, `summarize.rs`, `pruning.rs` |
| Background processes | `src/bg_process/manager.rs` |
| Conversation types | `src/conversation/types.rs` |
| Tool filter | `src/tool_filter/mod.rs` |
| Integration tests | `crates/nexus-daemon-tests/src/tests/` |

### Frontend (TypeScript/React)

All paths relative to `ui/`.

| Concern | Files |
|---------|-------|
| SSE connection + dispatch | `src/runtime/event-bus.ts` |
| Turn event consumer | `src/lib/stream-consumer.ts` |
| Broadcast event handlers | `src/hooks/useStreamBroadcasts.ts` |
| Chat stream hook | `src/hooks/useChatStream.ts` |
| Thread state | `src/stores/threadStore.ts`, `threadListStore.ts` |
| Agent state | `src/stores/agentStore.ts` |
| Provider state | `src/stores/providerStore.ts` |
| Task state | `src/stores/taskStore.ts` |
| Workspace state | `src/stores/workspaceStore.ts` |
| Usage state | `src/stores/usageStore.ts` |
| Process state | `src/stores/processStore.ts` |
| API client | `src/api/client.ts` |

### Shared State (AppState)

Defined in `src/server/mod.rs`:

```rust
pub struct AppState {
    pub config: NexusConfig,
    pub turns: Arc<TurnManager>,
    pub agents: Arc<AgentService>,
    pub providers: Arc<ProviderService>,
    pub mcp: Arc<McpService>,
    pub tasks: Arc<TaskService>,
    pub threads: Arc<ThreadService>,
    pub event_bus: EventBus,
    pub projects: Arc<RwLock<ProjectStore>>,
    pub workspaces: Arc<RwLock<WorkspaceStore>>,
    pub base_filesystem_config: FilesystemConfig,
    pub effective_fs_config: RwLock<FilesystemConfig>,
    pub title_client: Option<AnthropicClient>,
}
```
