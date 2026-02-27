use futures::StreamExt;
use serde_json::Value;
use std::time::Duration;
use tokio::sync::mpsc;

/// An open SSE connection to `/api/events`.
///
/// Events are parsed from `data: <json>` lines and buffered in a channel.
pub struct SseSubscription {
    rx: mpsc::UnboundedReceiver<Value>,
    _task: tokio::task::JoinHandle<()>,
}

impl SseSubscription {
    pub fn connect(url: String) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        let task = tokio::spawn(async move {
            let client = reqwest::Client::new();
            let resp = match client
                .get(&url)
                .header("Accept", "text/event-stream")
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("SSE connection failed: {e}");
                    return;
                }
            };

            let mut stream = resp.bytes_stream();
            let mut buf = String::new();

            while let Some(chunk) = stream.next().await {
                let Ok(bytes) = chunk else { break };
                buf.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(end) = buf.find("\n\n") {
                    let event_str = buf[..end].to_string();
                    buf = buf[end + 2..].to_string();

                    for line in event_str.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            if let Ok(val) = serde_json::from_str::<Value>(data) {
                                let _ = tx.send(val);
                            }
                        }
                    }
                }
            }
        });

        Self { rx, _task: task }
    }

    /// Wait for an event matching `predicate`, up to `timeout`.
    pub async fn next_matching<F>(&mut self, predicate: F, timeout: Duration) -> Option<Value>
    where
        F: Fn(&Value) -> bool,
    {
        let deadline = tokio::time::sleep(timeout);
        tokio::pin!(deadline);

        loop {
            tokio::select! {
                Some(event) = self.rx.recv() => {
                    if predicate(&event) {
                        return Some(event);
                    }
                }
                _ = &mut deadline => {
                    return None;
                }
            }
        }
    }

    /// Assert that the SYNC event arrives within 5 seconds.
    pub async fn expect_sync(&mut self) -> Value {
        self.next_matching(
            |e| e.get("type").and_then(|t| t.as_str()) == Some("SYNC"),
            Duration::from_secs(5),
        )
        .await
        .expect("Expected SYNC event within 5 seconds")
    }

    /// Assert that an event with the given type arrives within `timeout`.
    pub async fn expect_event_type(&mut self, event_type: &str, timeout: Duration) -> Value {
        let ty = event_type.to_string();
        self.next_matching(
            move |e| e.get("type").and_then(|t| t.as_str()) == Some(ty.as_str()),
            timeout,
        )
        .await
        .unwrap_or_else(|| panic!("Expected event type '{event_type}' within {timeout:?}"))
    }

    /// Wait for a CUSTOM event with a specific `name` field.
    pub async fn expect_custom(&mut self, name: &str, timeout: Duration) -> Value {
        let n = name.to_string();
        self.next_matching(
            move |e| {
                e.get("type").and_then(|t| t.as_str()) == Some("CUSTOM")
                    && e.get("name").and_then(|n2| n2.as_str()) == Some(n.as_str())
            },
            timeout,
        )
        .await
        .unwrap_or_else(|| panic!("Expected CUSTOM event '{name}' within {timeout:?}"))
    }

    /// Collect all events matching `predicate` until `timeout` expires.
    pub async fn collect_matching<F>(&mut self, predicate: F, timeout: Duration) -> Vec<Value>
    where
        F: Fn(&Value) -> bool,
    {
        let mut results = Vec::new();
        let deadline = tokio::time::sleep(timeout);
        tokio::pin!(deadline);

        loop {
            tokio::select! {
                Some(event) = self.rx.recv() => {
                    if predicate(&event) {
                        results.push(event);
                    }
                }
                _ = &mut deadline => {
                    return results;
                }
            }
        }
    }
}
