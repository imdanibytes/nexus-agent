use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Notify, RwLock};

/// Cached diagnostics for a single file URI.
struct CachedDiagnostics {
    diagnostics: Vec<lsp_types::Diagnostic>,
    _version: Option<i32>,
    _updated_at: Instant,
}

/// Thread-safe cache of diagnostics, updated by LSP push notifications.
pub struct DiagnosticCache {
    store: RwLock<HashMap<String, CachedDiagnostics>>,
    changed: Notify,
}

impl DiagnosticCache {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            store: RwLock::new(HashMap::new()),
            changed: Notify::new(),
        })
    }

    /// Called by the background task when publishDiagnostics arrives.
    pub async fn update(&self, uri: &str, diagnostics: Vec<lsp_types::Diagnostic>, version: Option<i32>) {
        let mut store = self.store.write().await;
        store.insert(uri.to_string(), CachedDiagnostics {
            diagnostics,
            _version: version,
            _updated_at: Instant::now(),
        });
        drop(store);
        self.changed.notify_waiters();
    }

    /// Get cached diagnostics for a URI. Returns empty vec if none cached.
    pub async fn get(&self, uri: &str) -> Vec<lsp_types::Diagnostic> {
        let store = self.store.read().await;
        store.get(uri)
            .map(|c| c.diagnostics.clone())
            .unwrap_or_default()
    }

    /// Wait for the LSP to publish diagnostics for a URI, with timeout.
    ///
    /// Returns `Ready` as soon as the LSP has published ANY diagnostics for
    /// the URI (including an empty list, meaning "no errors"). Returns
    /// `Pending` if the timeout expires before the LSP has analyzed the file.
    pub async fn wait_for(&self, uri: &str, timeout: Duration) -> DiagnosticStatus {
        let deadline = tokio::time::sleep(timeout);
        tokio::pin!(deadline);

        loop {
            // Register notification BEFORE checking cache to prevent race:
            // notify_waiters() only wakes already-registered futures.
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            // Any cache entry (even empty) means the LSP has analyzed this file.
            {
                let store = self.store.read().await;
                if let Some(cached) = store.get(uri) {
                    return DiagnosticStatus::Ready(cached.diagnostics.clone());
                }
            }

            tokio::select! {
                _ = &mut notified => {}
                _ = &mut deadline => {
                    // Timeout: check once more, then report pending
                    let store = self.store.read().await;
                    return match store.get(uri) {
                        Some(cached) => DiagnosticStatus::Ready(cached.diagnostics.clone()),
                        None => DiagnosticStatus::Pending,
                    };
                }
            }
        }
    }
}

/// Result of waiting for LSP diagnostics.
pub enum DiagnosticStatus {
    /// LSP has analyzed this file. Vec may be empty (clean file).
    Ready(Vec<lsp_types::Diagnostic>),
    /// Timed out — server is likely still indexing the project.
    Pending,
}
