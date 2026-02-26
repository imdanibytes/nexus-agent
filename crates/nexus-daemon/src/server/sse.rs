use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::agent::events::AgUiEvent;

/// Bridge between the agent loop and the global SSE stream.
///
/// All events flow through a single broadcast channel. A background task
/// buffers events per active turn so that new subscribers (page refresh)
/// can replay the full turn from the beginning.
#[derive(Clone)]
pub struct AgentEventBridge {
    tx: broadcast::Sender<AgUiEvent>,
    /// Per-conversation event buffer for replay on reconnect.
    /// Key = conversation_id, Value = serialized JSON events.
    /// Created on RUN_STARTED, cleared on RUN_FINISHED/RUN_ERROR.
    turn_buffers: Arc<Mutex<HashMap<String, Vec<String>>>>,
}

impl AgentEventBridge {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(4096);
        let turn_buffers: Arc<Mutex<HashMap<String, Vec<String>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Spawn buffer-capture task: subscribes to broadcast and maintains
        // per-turn event buffers for replay on reconnect.
        let mut rx: broadcast::Receiver<AgUiEvent> = tx.subscribe();
        let buffers = Arc::clone(&turn_buffers);
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let json = match serde_json::to_string(&event) {
                            Ok(j) => j,
                            Err(_) => continue,
                        };

                        let mut bufs = buffers.lock().await;

                        if let Some(tid) = event.thread_id() {
                            let tid_owned = tid.to_string();

                            if event.is_run_started() {
                                bufs.entry(tid_owned.clone())
                                    .or_insert_with(Vec::new);
                            }

                            if let Some(buf) = bufs.get_mut(&tid_owned) {
                                buf.push(json);
                            }

                            if event.is_run_terminal() {
                                bufs.remove(&tid_owned);
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            skipped = n,
                            "SSE buffer-capture lagged — {} events dropped",
                            n
                        );
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        Self { tx, turn_buffers }
    }

    /// Get the sender that the agent loop uses to emit events.
    pub fn agent_tx(&self) -> broadcast::Sender<AgUiEvent> {
        self.tx.clone()
    }

    /// Create a global SSE stream for a new subscriber.
    ///
    /// Emits a SYNC event with the list of active conversation IDs, then
    /// replays all buffered events for those conversations, then streams
    /// live events from the broadcast channel.
    pub async fn subscribe(
        &self,
        active_runs: Vec<String>,
    ) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
        // Lock buffers, snapshot replay data, and subscribe to broadcast
        // atomically — no events can be lost between snapshot and subscribe.
        let bufs = self.turn_buffers.lock().await;

        let mut replay: Vec<String> = Vec::new();
        for conv_id in &active_runs {
            if let Some(events) = bufs.get(conv_id) {
                replay.extend(events.iter().cloned());
            }
        }

        let rx = self.tx.subscribe();
        drop(bufs);

        // Build the SYNC event
        let sync_event = AgUiEvent::Sync {
            active_runs,
        };
        let sync_json = serde_json::to_string(&sync_event).unwrap_or_default();

        // Chain: SYNC → replay → live
        let sync_stream = futures::stream::once(async move {
            Ok(Event::default().data(sync_json))
        });

        let replay_stream = futures::stream::iter(
            replay.into_iter().map(|json| Ok(Event::default().data(json))),
        );

        let live_stream = BroadcastStream::new(rx)
            .filter_map(|msg| msg.ok())
            .map(|event| {
                let json = serde_json::to_string(&event).unwrap_or_default();
                Ok(Event::default().data(json))
            });

        let stream = sync_stream.chain(replay_stream).chain(live_stream);
        Sse::new(stream).keep_alive(KeepAlive::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_event_serializes_correctly() {
        let event = AgUiEvent::Sync {
            active_runs: vec!["conv1".into(), "conv2".into()],
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "SYNC");
        assert_eq!(json["activeRuns"], serde_json::json!(["conv1", "conv2"]));
    }

    #[tokio::test]
    async fn buffer_captures_and_clears_on_run_finished() {
        let bridge = AgentEventBridge::new();
        let tx = bridge.agent_tx();

        // Emit RUN_STARTED
        let _ = tx.send(AgUiEvent::RunStarted {
            thread_id: "conv1".into(),
            run_id: "run1".into(),
        });

        // Emit some content
        let _ = tx.send(AgUiEvent::TextMessageContent {
            thread_id: "conv1".into(),
            run_id: "run1".into(),
            message_id: "m1".into(),
            delta: "hello".into(),
        });

        // Allow buffer-capture task to process
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Buffer should have 2 events
        {
            let bufs = bridge.turn_buffers.lock().await;
            assert_eq!(bufs.get("conv1").map(|v| v.len()), Some(2));
        }

        // Emit RUN_FINISHED — buffer should be cleared
        let _ = tx.send(AgUiEvent::RunFinished {
            thread_id: "conv1".into(),
            run_id: "run1".into(),
            has_running_processes: false,
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        {
            let bufs = bridge.turn_buffers.lock().await;
            assert!(!bufs.contains_key("conv1"));
        }
    }
}
