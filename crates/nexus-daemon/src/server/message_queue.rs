use std::collections::HashMap;

use tokio::sync::{mpsc, Mutex};

/// A queued user-role message waiting to be injected into a conversation.
pub struct QueuedMessage {
    pub text: String,
    pub metadata: serde_json::Value,
}

/// Generic per-conversation message queue.
///
/// Any subsystem (background processes, external notifications, etc.) can
/// enqueue user-role messages. The turn loop drains the queue after each
/// turn and spawns follow-up turns when messages are pending. A notify
/// channel wakes a receiver task for idle conversations (no active turn).
pub struct MessageQueue {
    messages: Mutex<HashMap<String, Vec<QueuedMessage>>>,
    notify_tx: mpsc::UnboundedSender<String>,
}

impl MessageQueue {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<String>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                messages: Mutex::new(HashMap::new()),
                notify_tx: tx,
            },
            rx,
        )
    }

    /// Enqueue a user-role message for a conversation.
    /// Fires a notification so idle conversations get processed immediately.
    pub async fn enqueue(&self, conversation_id: &str, message: QueuedMessage) {
        let mut msgs = self.messages.lock().await;
        msgs.entry(conversation_id.to_string())
            .or_default()
            .push(message);
        drop(msgs);
        let _ = self.notify_tx.send(conversation_id.to_string());
    }

    /// Drain all queued messages for a conversation.
    pub async fn drain(&self, conversation_id: &str) -> Vec<QueuedMessage> {
        let mut msgs = self.messages.lock().await;
        msgs.remove(conversation_id).unwrap_or_default()
    }

    /// Remove all queued messages for a conversation.
    pub async fn clear(&self, conversation_id: &str) {
        let mut msgs = self.messages.lock().await;
        msgs.remove(conversation_id);
    }
}
