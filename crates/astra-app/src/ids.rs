// SPDX-License-Identifier: GPL-3.0-only
//! The idempotency-key mint the write families share, split from the crate
//! root so neither file grows past the cap.

/// A unique-per-intent idempotency key under `prefix`, from the wall clock
/// and a process counter: the one mint every write family shares, so no
/// family carries its own copy of the recipe.
pub(crate) fn mint_idempotency(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!("{prefix}:{nanos}:{seq}")
}
