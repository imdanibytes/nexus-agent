/**
 * Snowflake ID generator — 64-bit, time-sortable, unique.
 *
 * Layout (same as Discord/Twitter):
 *   42 bits — timestamp (ms since Nexus epoch) → ~139 years
 *   10 bits — worker ID (random per page load) → 1024 values
 *   12 bits — sequence (per-ms counter) → 4096/ms
 *
 * Represented as decimal string (JS numbers lose precision above 2^53).
 */

// Nexus epoch: 2024-01-01T00:00:00.000Z
const EPOCH = 1704067200000n;

// Random 10-bit worker ID per page load
const WORKER_ID = BigInt(Math.floor(Math.random() * 1024));

let lastTs = 0n;
let seq = 0n;

export function snowflake(): string {
  let ts = BigInt(Date.now()) - EPOCH;

  if (ts === lastTs) {
    seq = (seq + 1n) & 0xFFFn;
    if (seq === 0n) {
      // Sequence overflow — spin until next ms
      while (BigInt(Date.now()) - EPOCH <= lastTs) {
        /* spin */
      }
      ts = BigInt(Date.now()) - EPOCH;
    }
  } else {
    seq = 0n;
  }

  lastTs = ts;
  return ((ts << 22n) | (WORKER_ID << 12n) | seq).toString();
}

/** Extract Unix timestamp (ms) from a Snowflake ID */
export function snowflakeTimestamp(id: string): number {
  return Number((BigInt(id) >> 22n) + EPOCH);
}
