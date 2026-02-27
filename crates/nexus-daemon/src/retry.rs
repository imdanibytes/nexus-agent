//! Retry utilities for transient provider errors.

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
