use tokio::sync::broadcast;

use crate::agent::events::{AgUiEvent, EventEnvelope};

/// Shared event bus for all services.
///
/// Thin wrapper around a broadcast channel. Services emit data events here;
/// the SSE layer and any other consumers subscribe. Two event categories
/// flow through the same channel:
///
/// - **Data events** (from services): `data:message_added`, `data:title_changed`, etc.
/// - **Streaming events** (from TurnEmitter): `TEXT_MESSAGE_CONTENT`, `RUN_FINISHED`, etc.
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<EventEnvelope>,
}

#[allow(dead_code)] // core API surface: new, sender, subscribe used across services
impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(4096);
        Self { tx }
    }

    /// Wrap an existing sender (e.g. from AgentEventBridge) so both share
    /// the same underlying channel.
    pub fn from_sender(tx: broadcast::Sender<EventEnvelope>) -> Self {
        Self { tx }
    }

    /// Emit an event envelope.
    pub fn emit(&self, envelope: EventEnvelope) {
        let _ = self.tx.send(envelope);
    }

    /// Convenience: emit a `CUSTOM` data event scoped to a thread.
    ///
    /// The event name is prefixed with `data:` so subscribers can distinguish
    /// data-plane events from streaming events.
    pub fn emit_data(&self, thread_id: &str, name: &str, value: serde_json::Value) {
        self.emit(EventEnvelope {
            thread_id: Some(thread_id.to_string()),
            run_id: None,
            event: AgUiEvent::Custom {
                name: format!("data:{}", name),
                value,
            },
        });
    }

    /// Convenience: emit a global data event (not scoped to a thread).
    ///
    /// Used by services like AgentService and ProviderService whose events
    /// aren't tied to a specific conversation.
    pub fn emit_global(&self, name: &str, value: serde_json::Value) {
        self.emit(EventEnvelope {
            thread_id: None,
            run_id: None,
            event: AgUiEvent::Custom {
                name: name.to_string(),
                value,
            },
        });
    }

    /// Get the raw sender (for TurnEmitter, ProcessManager, etc.)
    pub fn sender(&self) -> broadcast::Sender<EventEnvelope> {
        self.tx.clone()
    }

    /// Subscribe to all events.
    pub fn subscribe(&self) -> broadcast::Receiver<EventEnvelope> {
        self.tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_data_prefixes_name() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        bus.emit_data("t1", "thread_created", serde_json::json!({"id": "t1"}));

        let envelope = rx.try_recv().unwrap();
        assert_eq!(envelope.thread_id.as_deref(), Some("t1"));
        match &envelope.event {
            AgUiEvent::Custom { name, value } => {
                assert_eq!(name, "data:thread_created");
                assert_eq!(value["id"], "t1");
            }
            _ => panic!("expected Custom event"),
        }
    }

    #[test]
    fn from_sender_shares_channel() {
        let (tx, _) = broadcast::channel(16);
        let bus = EventBus::from_sender(tx.clone());
        let mut rx = bus.subscribe();

        // Send via the original sender
        let _ = tx.send(EventEnvelope {
            thread_id: None,
            run_id: None,
            event: AgUiEvent::RunStarted,
        });

        let envelope = rx.try_recv().unwrap();
        assert!(envelope.event.is_run_started());
    }
}
