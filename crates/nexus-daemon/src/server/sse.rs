use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use std::convert::Infallible;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::agent::events::AgUiEvent;

/// Broadcast hub for SSE events. Clients subscribe to receive all events.
#[derive(Clone)]
pub struct SseHub {
    tx: broadcast::Sender<String>,
}

impl SseHub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self { tx }
    }

    /// Push a serialized event to all subscribers.
    pub fn push(&self, event: &AgUiEvent) {
        if let Ok(json) = serde_json::to_string(event) {
            let _ = self.tx.send(json);
        }
    }

    /// Create an SSE stream for an Axum handler.
    pub fn subscribe(&self) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
        let rx = self.tx.subscribe();
        let stream = BroadcastStream::new(rx)
            .filter_map(|msg| msg.ok())
            .map(|json| Ok(Event::default().data(json)));
        Sse::new(stream).keep_alive(KeepAlive::default())
    }
}

/// A bridge between the agent loop (which uses broadcast::Sender<AgUiEvent>)
/// and the SseHub (which serializes and forwards).
#[derive(Clone)]
pub struct AgentEventBridge {
    tx: broadcast::Sender<AgUiEvent>,
}

impl AgentEventBridge {
    pub fn new(hub: SseHub) -> Self {
        let (tx, _) = broadcast::channel(1024);
        let bridge = Self { tx: tx.clone() };

        // Spawn a task that forwards AgUiEvents to the SseHub as JSON.
        // IMPORTANT: We must handle Lagged errors by continuing, not breaking.
        // If the broadcast buffer overflows, recv() returns Err(Lagged(n)),
        // and breaking out of the loop would kill the forwarder permanently,
        // causing all subsequent events (including RUN_FINISHED) to be lost.
        let mut rx = tx.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        hub.push(&event);
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            skipped = n,
                            "SSE bridge lagged — {} events dropped. Consider increasing buffer size.",
                            n
                        );
                        // Continue receiving — don't break the forwarding loop
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        // Channel closed — all senders dropped, shut down cleanly
                        break;
                    }
                }
            }
        });

        bridge
    }

    /// Get the sender that the agent loop uses to emit events.
    pub fn agent_tx(&self) -> broadcast::Sender<AgUiEvent> {
        self.tx.clone()
    }
}
