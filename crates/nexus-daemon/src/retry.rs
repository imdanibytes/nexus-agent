//! Retry with exponential backoff for transient provider errors.

use std::future::Future;
use std::time::Duration;

use anyhow::Result;
use tokio_util::sync::CancellationToken;

use crate::provider::error::{ProviderError, ProviderErrorKind};

const MAX_ATTEMPTS: u32 = 3;
const INITIAL_DELAY_MS: u64 = 1000;
const MAX_DELAY_MS: u64 = 30_000;
const BACKOFF_FACTOR: f64 = 2.0;

/// Maximum number of retries for transient errors.
pub const MAX_RETRIES: u32 = MAX_ATTEMPTS;

/// Calculate backoff delay in ms for a given attempt (1-indexed).
pub fn backoff_delay(attempt: u32) -> u64 {
    let base = INITIAL_DELAY_MS as f64 * BACKOFF_FACTOR.powi((attempt - 1) as i32);
    let delay = (base as u64).min(MAX_DELAY_MS);
    // Add ±25% jitter
    let jitter = (delay as f64 * 0.25 * (rand_jitter() * 2.0 - 1.0)) as i64;
    (delay as i64 + jitter).max(100) as u64
}

/// Retry a fallible async operation with exponential backoff.
///
/// Only retries if the error is a `ProviderError` with `retryable: true`.
/// Non-retryable errors are returned immediately. Respects cancellation.
///
/// Returns a tuple of (result, attempt_count) so callers can log retry info.
pub async fn with_backoff<F, Fut, T>(
    cancel: &CancellationToken,
    notify: Option<&dyn Fn(u32, u32, &ProviderErrorKind, u64)>,
    mut f: F,
) -> (Result<T>, u32)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut attempt: u32 = 0;

    loop {
        attempt += 1;
        match f().await {
            Ok(val) => return (Ok(val), attempt),
            Err(e) => {
                if attempt >= MAX_ATTEMPTS {
                    return (Err(e), attempt);
                }

                let is_retryable = e
                    .downcast_ref::<ProviderError>()
                    .is_some_and(|pe| pe.retryable);

                if !is_retryable {
                    return (Err(e), attempt);
                }

                let kind = e
                    .downcast_ref::<ProviderError>()
                    .map(|pe| pe.kind)
                    .unwrap_or(ProviderErrorKind::Unknown);

                // Exponential backoff with jitter
                let base_delay = INITIAL_DELAY_MS as f64
                    * BACKOFF_FACTOR.powi((attempt - 1) as i32);
                let delay_ms = (base_delay as u64).min(MAX_DELAY_MS);
                // Add ±25% jitter
                let jitter = (delay_ms as f64 * 0.25 * (rand_jitter() * 2.0 - 1.0)) as i64;
                let actual_delay = (delay_ms as i64 + jitter).max(100) as u64;

                tracing::warn!(
                    attempt,
                    max_attempts = MAX_ATTEMPTS,
                    delay_ms = actual_delay,
                    error_kind = ?kind,
                    "Retrying after transient error"
                );

                if let Some(notify_fn) = notify {
                    notify_fn(attempt, MAX_ATTEMPTS, &kind, actual_delay);
                }

                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(actual_delay)) => {}
                    _ = cancel.cancelled() => {
                        return (Err(e), attempt);
                    }
                }
            }
        }
    }
}

/// Simple deterministic-ish jitter using the current time's nanoseconds.
/// Not cryptographic, just enough to spread out retry storms.
fn rand_jitter() -> f64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    (nanos % 1000) as f64 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn success_on_first_try() {
        let cancel = CancellationToken::new();
        let (result, attempts) = with_backoff(&cancel, None, || async { Ok::<_, anyhow::Error>(42) }).await;
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts, 1);
    }

    #[tokio::test]
    async fn non_retryable_error_stops_immediately() {
        let cancel = CancellationToken::new();
        let call_count = Arc::new(AtomicU32::new(0));
        let cc = call_count.clone();

        let (result, attempts) = with_backoff(&cancel, None, || {
            let cc = cc.clone();
            async move {
                cc.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(ProviderError {
                    kind: ProviderErrorKind::Authentication,
                    message: "bad key".into(),
                    status_code: Some(401),
                    retryable: false,
                    provider: "test".into(),
                }.into())
            }
        }).await;

        assert!(result.is_err());
        assert_eq!(attempts, 1);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retries_on_retryable_error() {
        let cancel = CancellationToken::new();
        let call_count = Arc::new(AtomicU32::new(0));
        let cc = call_count.clone();

        let (result, attempts) = with_backoff(&cancel, None, || {
            let cc = cc.clone();
            async move {
                let n = cc.fetch_add(1, Ordering::SeqCst) + 1;
                if n < 3 {
                    Err::<i32, _>(ProviderError {
                        kind: ProviderErrorKind::RateLimit,
                        message: "rate limited".into(),
                        status_code: Some(429),
                        retryable: true,
                        provider: "test".into(),
                    }.into())
                } else {
                    Ok(99)
                }
            }
        }).await;

        assert_eq!(result.unwrap(), 99);
        assert_eq!(attempts, 3);
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn exhausts_max_attempts() {
        let cancel = CancellationToken::new();
        let call_count = Arc::new(AtomicU32::new(0));
        let cc = call_count.clone();

        let (result, attempts) = with_backoff(&cancel, None, || {
            let cc = cc.clone();
            async move {
                cc.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(ProviderError {
                    kind: ProviderErrorKind::Overloaded,
                    message: "overloaded".into(),
                    status_code: Some(529),
                    retryable: true,
                    provider: "test".into(),
                }.into())
            }
        }).await;

        assert!(result.is_err());
        assert_eq!(attempts, MAX_ATTEMPTS);
        assert_eq!(call_count.load(Ordering::SeqCst), MAX_ATTEMPTS);
    }
}
