//! Periodic eviction of stale block decoders (ADR-0004).
//!
//! Behavior contract: the loop body is the former inline `async move` block
//! from `run_impl` verbatim — same `CLEANUP_INTERVAL` tick, same single
//! `retain` pass computing `timed_out` while it removes, same
//! `feedback_update(false)` per timed-out block under ONE `fec_controller`
//! guard, same `decoded_fail` stat and warn. This task never returns.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use dashmap::DashMap;
use tracing::warn;

use super::super::{CLEANUP_INTERVAL, DECODER_TIMEOUT};
use crate::control::FecRateController;
use crate::fec::FecDecoder;
use crate::monitor::stats::SharedStats;

/// ADR-0004: periodic cleanup of stale decoders.
pub(crate) async fn run_decoder_gc(
    cleanup_decoders: Arc<DashMap<u64, Box<dyn FecDecoder>>>,
    cleanup_fec: Arc<parking_lot::Mutex<FecRateController>>,
    cleanup_stats: Arc<SharedStats>,
) {
    let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
    loop {
        interval.tick().await;
        let now = Instant::now();
        let mut timed_out = Vec::new();

        cleanup_decoders.retain(|block_id, decoder| {
            if now.duration_since(decoder.created_at()) > DECODER_TIMEOUT {
                if !decoder.is_decoded() {
                    timed_out.push(*block_id);
                }
                false // remove
            } else {
                true // keep
            }
        });

        // Report timed-out blocks as failures to FEC controller
        if !timed_out.is_empty() {
            let mut ctrl = cleanup_fec.lock();
            for _block_id in &timed_out {
                ctrl.feedback_update(false);
            }
            // ADR-0013: update monitoring stats for timed-out blocks
            cleanup_stats.blocks.decoded_fail.fetch_add(timed_out.len() as u64, Ordering::Relaxed);
            warn!(
                count = timed_out.len(),
                "evicted timed-out decoders (block decode failures)"
            );
        }
    }
}
