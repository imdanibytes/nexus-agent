# Nexus v0

Local-first AI agent platform. Rust daemon (`nexus-daemon`) serves HTTP+SSE, React/TypeScript UI consumes it.

## Quick Reference

- **Architecture**: [`docs/architecture.md`](docs/architecture.md) — system overview, service map, data flows, key file locations
- **Event Protocol**: [`docs/event-protocol.md`](docs/event-protocol.md) — SSE wire format, full event catalog, naming rules

Read both before touching the event system or adding new features that emit events.

## Project Structure

```
crates/
  nexus-daemon/          Axum HTTP+SSE server (binary: nexus)
  nexus-daemon-tests/    Integration tests (black-box HTTP)
ui/                      React 19 + TypeScript + TailwindCSS 4 + HeroUI
docs/                    Architecture and protocol specs
```

## Commands

```bash
# Backend
cargo build                              # build daemon
cargo test                               # all unit + integration tests
cargo test -p nexus-daemon               # unit tests only
cargo test -p nexus-daemon-tests         # integration tests only (73 tests)
cargo clippy --workspace                 # lint (zero warnings policy)

# Frontend
cd ui
npm run dev                              # vite dev server (port 5173, proxies /api → :9600)
npm run build                            # tsc + vite build
npm run lint                             # eslint
npm run lint:fix                         # eslint --fix
```

## Architecture at a Glance

Services wrap stores with internal locking and emit events via `EventBus` on mutations:

| Service | File (relative to `crates/nexus-daemon/`) |
|---------|------------------------------------------|
| ThreadService | `src/thread/mod.rs` |
| AgentService | `src/agent_config/service.rs` |
| ProviderService | `src/provider/service.rs` |
| TaskService | `src/tasks/service.rs` |
| TurnManager | `src/server/services.rs` |
| ProcessManager | `src/bg_process/manager.rs` |

Event flow: Services → `EventBus` → `broadcast::channel` → `AgentEventBridge` → SSE → browser `event-bus.ts` → Zustand stores.

## Adding a New Event (Mandatory Checklist)

**Every event must have: a backend emitter, a wire format test, an integration test, and a frontend consumer.** If any is missing, that's a bug. See [`docs/event-protocol.md`](docs/event-protocol.md) for the full checklist.

The short version:

1. Add method to `TurnEmitter` (turn-scoped) or call `EventBus.emit_data()`/`emit_global()` (service mutation)
2. Add serialization test in the emitter's `#[cfg(test)]` module
3. Add integration test in `crates/nexus-daemon-tests/src/tests/event_emission.rs`
4. Add handler in `ui/src/lib/stream-consumer.ts` (turn-scoped) or `ui/src/hooks/useStreamBroadcasts.ts` (broadcast)
5. Update `docs/event-protocol.md`

**Naming**: `SCREAMING_SNAKE` for turn-scoped streaming events, `snake_case` for service/custom events. Never prefix with `data:`.

## Key Frontend Files

| Concern | Path (relative to `ui/`) |
|---------|--------------------------|
| SSE connection + dispatch | `src/runtime/event-bus.ts` |
| Turn event consumer | `src/lib/stream-consumer.ts` |
| Broadcast handlers | `src/hooks/useStreamBroadcasts.ts` |
| Chat stream hook | `src/hooks/useChatStream.ts` |
| Stores | `src/stores/*.ts` |
| API client | `src/api/client.ts` |

## Conventions

- **Rust**: Default `rustfmt` and `clippy` settings. Zero clippy warnings.
- **TypeScript**: ESLint flat config. `@` alias maps to `ui/src/`. No explicit `any` rule is off but don't abuse it.
- **Events**: Names must match exactly between backend and frontend — no camelCase conversion happens.
- **Services**: All mutations go through service methods, never direct store access. Services emit their own events.
- **Testing**: Integration tests use `TestDaemon` harness that spins up a real server per test. Flaky startup timeouts are known — retry once before investigating.
