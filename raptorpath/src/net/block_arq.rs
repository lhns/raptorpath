//! Block-mode ARQ via batch acknowledgements (P8, paper §14.27).
//!
//! Block mode had NO loss recovery of its own: on a failed block the sender
//! only updated stats/CC and the receiver evicted the incomplete decoder
//! after a timeout. With the Bulk completion-exposure glide (P6) mid-stream
//! r* = 0, so there were no proactive repairs either — the inner flow saw
//! the raw channel loss and collapsed (L1: 1.8 MB in ~8 s at C2 vs quinn's
//! 0.175 s). This module implements the retransmission half of the paper §5
//! correction model for block mode:
//!
//! - **Batch ledger**: every sent SymbolBatch is recorded under its
//!   `batch_seq` with the (block_id, payload_id) pairs it carried. The
//!   receiver acks every batch (v4 Acks echo `batch_seq`), so an Ack is
//!   SACK-grade evidence: P(lost | acked batches follow with none for this
//!   one) ≈ 1. A batch is declared lost after `LATER_ACK_LOSS_THRESHOLD`
//!   later batches on the same path are acked (dup-ACK analogue, reorder
//!   tolerant) or after an SRTT-scaled timeout (tail batches with no
//!   later traffic).
//! - **Retained blocks**: source data for the last `RETAIN_MAX_BLOCKS`
//!   blocks (byte-capped LRU) so fresh repairs can be minted post-hoc.
//!   Rateless backends (RaptorQ, RLC) mint NEW repair symbols — any repair
//!   fills any hole, strictly better than resending the lost symbol.
//!   Fixed-rate backends (RS) resend the exact missing symbols,
//!   which every backend accepts.
//! - **Margin**: repairs per loss event = missing + fractional-accumulated
//!   ε̂ margin (continuous, no per-event ceil). Repair batches re-enter the
//!   ledger, so a lost repair triggers the next round with doubled margin,
//!   up to `MAX_REPAIR_ROUNDS`; the receiver's decoder-eviction timeout
//!   stays as the final backstop.
//!
//! A lost Ack is indistinguishable from a lost batch; the resulting
//! spurious repair (~ε̂ of batches) is bounded overhead, and the receiver
//! ignores symbols for already-decoded blocks.

use crate::fec::{EncodingParams, FecBackend, FecEncoder, WireSymbol};
use bytes::Bytes;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

/// Maximum blocks retained for post-hoc repair generation.
pub const RETAIN_MAX_BLOCKS: usize = 64;
/// Maximum bytes of retained block data (~4 MB).
pub const RETAIN_MAX_BYTES: usize = 4 * 1024 * 1024;
/// Maximum un-acked batches tracked; oldest entries drop silently beyond
/// this (the in_flight budget bounds real outstanding data well below it).
pub const LEDGER_MAX_BATCHES: usize = 4096;
/// Later same-path acks required to declare an un-acked batch lost
/// (dup-ACK analogue; tolerates datagram reordering within a path).
pub const LATER_ACK_LOSS_THRESHOLD: u8 = 3;
/// Maximum repair rounds per block; beyond this the receiver's decoder
/// eviction timeout is the backstop.
pub const MAX_REPAIR_ROUNDS: u8 = 3;
/// Maximum idle re-announce rounds per block (BlockStart + spare repair for a
/// still-un-decoded block once the sender goes quiet — see `idle_reannounce`).
/// Kept generous: a lost BlockStart with all its symbols delivered-and-acked
/// leaves the ARQ ledger empty, so this is the ONLY recovery path for that
/// block, and each round only clears if the re-announced BlockStart datagram
/// itself survives the channel (~ε̂ loss per try).
pub const MAX_REANNOUNCE_ROUNDS: u8 = 16;
/// Per-round cap on idle re-announce spare symbols. The spare ramps
/// geometrically toward a block's deficit (unknown to the sender), but each
/// round's burst is capped small so a stuck block cannot flood a constrained
/// path or jam the in_flight budget — the L1 C3 (20 Mbit) failure mode when a
/// full-block resend went out every round. A deficit up to k recovers in a
/// handful of capped rounds at the (clamped) re-announce cadence.
pub const REANNOUNCE_PER_ROUND_CAP: u32 = 16;
/// Completed/failed block ids remembered to suppress late spurious repairs.
const DONE_RING_CAP: usize = 1024;

/// One sent batch awaiting its Ack.
struct BatchEntry {
    path_id: u32,
    /// (block_id, payload_id) of every symbol in the batch, in send order.
    sent: Vec<(u64, u32)>,
    sent_at: Instant,
    /// Later same-path batches acked while this one was not.
    later_acks: u8,
}

/// Retained source data for one encoded block.
struct RetainedBlock {
    data: Bytes,
    params: EncodingParams,
    backend: FecBackend,
    /// Next fresh repair index (starts past the proactive repairs).
    next_repair_esi: u32,
    /// Repair rounds already spent on this block.
    rounds: u8,
    /// Lazily rebuilt encoder, cached across rounds for this block.
    encoder: Option<Box<dyn FecEncoder>>,
    /// Last time any symbol of this block hit the wire (encode, normal send,
    /// repair, or re-announce). Drives the idle re-announce: a block quiet for
    /// longer than the loss timeout while still un-decoded is stuck.
    last_activity: Instant,
    /// Idle re-announce rounds already spent (separate from `rounds`: a lost
    /// BlockStart is orphaned with an EMPTY ledger, so ARQ repair rounds never
    /// engage — this is its own recovery budget).
    reannounce_rounds: u8,
}

/// Symbols of `block_id` presumed lost on `path_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LossEvent {
    pub block_id: u64,
    pub path_id: u32,
    pub missing: Vec<u32>,
}

/// A planned repair send for one block.
pub struct RepairPlan {
    pub block_id: u64,
    /// Path the loss was observed on (prefer sending repairs elsewhere).
    pub avoid_path: u32,
    pub symbols: Vec<WireSymbol>,
    /// Params + length for a defensive BlockStart re-announce (covers the
    /// case where the original BlockStart datagram itself was lost).
    pub params: EncodingParams,
    pub backend: FecBackend,
    pub transfer_length: u64,
}

/// Sender-side block-mode ARQ state: batch ledger + retained blocks.
pub struct BlockArq {
    ledger: BTreeMap<u64, BatchEntry>,
    retained: HashMap<u64, RetainedBlock>,
    /// LRU order of retained block ids (front = coldest).
    retain_order: VecDeque<u64>,
    retained_bytes: usize,
    /// Blocks decoded (or abandoned) — loss events for these are ignored.
    done_ring: VecDeque<u64>,
    done_set: HashSet<u64>,
    /// Fractional ε̂-margin accumulator (continuous, paper §14.27).
    margin_debt: f64,
    max_ledger: usize,
    max_retained_blocks: usize,
    max_retained_bytes: usize,
}

impl BlockArq {
    pub fn new() -> Self {
        Self::with_caps(LEDGER_MAX_BATCHES, RETAIN_MAX_BLOCKS, RETAIN_MAX_BYTES)
    }

    pub fn with_caps(
        max_ledger: usize,
        max_retained_blocks: usize,
        max_retained_bytes: usize,
    ) -> Self {
        Self {
            ledger: BTreeMap::new(),
            retained: HashMap::new(),
            retain_order: VecDeque::new(),
            retained_bytes: 0,
            done_ring: VecDeque::new(),
            done_set: HashSet::new(),
            margin_debt: 0.0,
            max_ledger,
            max_retained_blocks,
            max_retained_bytes,
        }
    }

    // ------------------------------------------------------------------
    // Retention
    // ------------------------------------------------------------------

    /// Retain a freshly encoded block's source data for post-hoc repairs.
    pub fn on_block_encoded(
        &mut self,
        block_id: u64,
        data: Bytes,
        params: EncodingParams,
        backend: FecBackend,
        now: Instant,
    ) {
        if self.done_set.contains(&block_id) || self.retained.contains_key(&block_id) {
            return;
        }
        self.retained_bytes += data.len();
        self.retained.insert(
            block_id,
            RetainedBlock {
                data,
                params,
                backend,
                next_repair_esi: params.repair_count,
                rounds: 0,
                encoder: None,
                last_activity: now,
                reannounce_rounds: 0,
            },
        );
        self.retain_order.push_back(block_id);
        self.evict_retained();
    }

    fn evict_retained(&mut self) {
        while self.retained.len() > self.max_retained_blocks
            || self.retained_bytes > self.max_retained_bytes
        {
            let Some(oldest) = self.retain_order.pop_front() else {
                break;
            };
            if let Some(rb) = self.retained.remove(&oldest) {
                self.retained_bytes -= rb.data.len();
            }
        }
    }

    /// Move a block to the back of the LRU order (it is being repaired —
    /// keep it alive for potential further rounds).
    fn touch_retained(&mut self, block_id: u64) {
        if let Some(pos) = self.retain_order.iter().position(|&b| b == block_id) {
            self.retain_order.remove(pos);
            self.retain_order.push_back(block_id);
        }
    }

    /// Block decoded successfully (or abandoned): drop retained data and
    /// suppress any pending/late loss events for it.
    pub fn on_block_done(&mut self, block_id: u64) {
        if let Some(rb) = self.retained.remove(&block_id) {
            self.retained_bytes -= rb.data.len();
            if let Some(pos) = self.retain_order.iter().position(|&b| b == block_id) {
                self.retain_order.remove(pos);
            }
        }
        if self.done_set.insert(block_id) {
            self.done_ring.push_back(block_id);
            while self.done_ring.len() > DONE_RING_CAP {
                if let Some(old) = self.done_ring.pop_front() {
                    self.done_set.remove(&old);
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Ledger
    // ------------------------------------------------------------------

    /// Record a sent SymbolBatch.
    pub fn on_batch_sent(
        &mut self,
        batch_seq: u64,
        path_id: u32,
        sent: Vec<(u64, u32)>,
        now: Instant,
    ) {
        if sent.is_empty() {
            return;
        }
        // Refresh idle-reannounce activity for every block this batch carried:
        // a block is "quiet" (candidate for re-announce) only once none of its
        // symbols have hit the wire for the loss timeout.
        for &(bid, _) in &sent {
            if let Some(rb) = self.retained.get_mut(&bid) {
                rb.last_activity = now;
            }
        }
        self.ledger.insert(
            batch_seq,
            BatchEntry {
                path_id,
                sent,
                sent_at: now,
                later_acks: 0,
            },
        );
        // Cap: silently drop the oldest (no repair — beyond the horizon).
        while self.ledger.len() > self.max_ledger {
            let Some((&oldest, _)) = self.ledger.iter().next() else {
                break;
            };
            self.ledger.remove(&oldest);
        }
    }

    /// Process an Ack for `batch_seq` received on `path_id`.
    ///
    /// Returns loss events from (a) any sent-vs-received diff within the
    /// acked batch itself (defensive — QUIC datagram atomicity normally
    /// makes this empty) and (b) older un-acked batches on the same path
    /// that crossed the dup-ack threshold or `loss_timeout`.
    pub fn on_ack(
        &mut self,
        batch_seq: u64,
        path_id: u32,
        received_ids: &[u32],
        now: Instant,
        loss_timeout: Duration,
    ) -> Vec<LossEvent> {
        let mut events = Vec::new();

        // (a) Diff the acked batch: multiset-match received payload_ids
        // against the sent list (payload_ids can repeat across blocks in a
        // mixed batch; the ledger, not the Ack, is authoritative for which
        // blocks were involved).
        if let Some(entry) = self.ledger.remove(&batch_seq) {
            if received_ids.len() < entry.sent.len() {
                let mut avail: HashMap<u32, u32> = HashMap::new();
                for &id in received_ids {
                    *avail.entry(id).or_default() += 1;
                }
                let mut missing: Vec<(u64, u32)> = Vec::new();
                for &(bid, pid) in &entry.sent {
                    match avail.get_mut(&pid) {
                        Some(c) if *c > 0 => *c -= 1,
                        _ => missing.push((bid, pid)),
                    }
                }
                self.push_events(&mut events, entry.path_id, missing);
            }
        }

        // (b) Older un-acked batches on the same path: bump later_acks;
        // declare lost past the threshold or the timeout.
        let candidates: Vec<u64> = self
            .ledger
            .range(..batch_seq)
            .filter(|(_, e)| e.path_id == path_id)
            .map(|(&seq, _)| seq)
            .collect();
        for seq in candidates {
            let declare = {
                let entry = self.ledger.get_mut(&seq).expect("candidate exists");
                entry.later_acks = entry.later_acks.saturating_add(1);
                entry.later_acks >= LATER_ACK_LOSS_THRESHOLD
                    || now.duration_since(entry.sent_at) >= loss_timeout
            };
            if declare {
                let entry = self.ledger.remove(&seq).expect("candidate exists");
                self.push_events(&mut events, entry.path_id, entry.sent);
            }
        }

        events
    }

    /// Timeout-only sweep for batches with no later traffic (transfer
    /// tails). `timeout_for` maps path_id → loss timeout (SRTT-scaled).
    pub fn sweep(&mut self, now: Instant, timeout_for: &dyn Fn(u32) -> Duration) -> Vec<LossEvent> {
        let expired: Vec<u64> = self
            .ledger
            .iter()
            .filter(|(_, e)| now.duration_since(e.sent_at) >= timeout_for(e.path_id))
            .map(|(&seq, _)| seq)
            .collect();
        let mut events = Vec::new();
        for seq in expired {
            let entry = self.ledger.remove(&seq).expect("expired exists");
            self.push_events(&mut events, entry.path_id, entry.sent);
        }
        events
    }

    /// Idle re-announce (paper §14.27, send-idle recovery leg).
    ///
    /// A block whose BlockStart datagram was lost is orphaned in a way the
    /// batch ledger cannot see: the receiver buffers its symbols pre-decoder
    /// and ACKs them anyway, so every batch clears the ledger and neither the
    /// dup-ack diff nor the tail `sweep` ever fires — yet the block never
    /// decodes (no decoder without its params). Once the sender goes quiet,
    /// this re-sends BlockStart (via the RepairPlan's defensive re-announce)
    /// plus a small ε̂-sized spare repair for any block still retained (i.e.
    /// not `on_block_done`) and quiet for `timeout_for(path)`. Bounded by
    /// `MAX_REANNOUNCE_ROUNDS`; stops the instant the block completes.
    ///
    /// `now - last_activity >= timeout` gates it above normal completion (a
    /// healthy block is acked+decoded, hence `on_block_done`-removed, within
    /// ~1 RTT ≪ the loss timeout), so this does not fire during steady
    /// pipelining. `default_path` is the loss-path hint stamped on each plan
    /// (dispatch prefers a different live path when one exists).
    pub fn idle_reannounce(
        &mut self,
        now: Instant,
        timeout_for: &dyn Fn(u32) -> Duration,
        default_path: u32,
        eps_hat: f64,
    ) -> Vec<RepairPlan> {
        let mut plans = Vec::new();
        // Snapshot ids first (mutable borrow of each rb happens in the loop).
        let candidates: Vec<u64> = self
            .retained
            .iter()
            .filter(|(_, rb)| {
                rb.reannounce_rounds < MAX_REANNOUNCE_ROUNDS
                    && now.duration_since(rb.last_activity) >= timeout_for(default_path)
            })
            .map(|(&id, _)| id)
            .collect();
        for block_id in candidates {
            if self.done_set.contains(&block_id) {
                continue;
            }
            let Some(rb) = self.retained.get_mut(&block_id) else {
                continue;
            };
            // Capped geometric spare. The receiver's deficit is unknown (its
            // symbols were "acked" pre-decoder, so no feedback reveals how many
            // it actually holds — a lost BlockStart can leave it with anywhere
            // from k down to 0 usable symbols after its pre-start buffer caps
            // drop the overflow). Round 0 is a cheap probe: BlockStart
            // re-announce + a ε̂ spare, which recovers the common pure-orphan
            // case outright (the receiver replays its full pre-start buffer).
            // If still short, ramp the spare geometrically but cap each round
            // (`REANNOUNCE_PER_ROUND_CAP`) so the burst stays small — a deficit
            // up to k accumulates across a few capped rounds without flooding a
            // constrained path. Rateless repairs are universal, so the receiver
            // decodes once the cumulative fresh repairs cover its hole;
            // BlockResult stops the ramp the instant it completes.
            let k = rb.params.source_symbols.max(1);
            let margin = (eps_hat * k as f64).ceil() as u32 + 1;
            let n_spare = if rb.reannounce_rounds == 0 {
                margin
            } else {
                (1u32 << (rb.reannounce_rounds.min(5) + 1))
                    .min(REANNOUNCE_PER_ROUND_CAP)
                    .max(margin)
                    .min(k + margin)
            };
            let encoder = rb
                .encoder
                .get_or_insert_with(|| rb.backend.create_encoder(&rb.data, rb.params));
            let symbols: Vec<WireSymbol> = if encoder.max_repairs() == u32::MAX {
                let s = encoder.repair_symbols_from(rb.next_repair_esi, n_spare);
                rb.next_repair_esi += s.len() as u32;
                s
            } else {
                // Fixed-rate: a few source symbols are always decoder-accepted.
                encoder
                    .source_symbols()
                    .into_iter()
                    .take(n_spare as usize)
                    .collect()
            };
            rb.reannounce_rounds += 1;
            rb.last_activity = now;
            plans.push(RepairPlan {
                block_id,
                avoid_path: default_path,
                symbols,
                params: rb.params,
                backend: rb.backend,
                transfer_length: rb.data.len() as u64,
            });
            self.touch_retained(block_id);
        }
        plans
    }

    /// Group missing (block_id, payload_id) pairs into per-block events,
    /// dropping blocks already done.
    fn push_events(&self, events: &mut Vec<LossEvent>, path_id: u32, missing: Vec<(u64, u32)>) {
        let mut by_block: BTreeMap<u64, Vec<u32>> = BTreeMap::new();
        for (bid, pid) in missing {
            if self.done_set.contains(&bid) {
                continue;
            }
            by_block.entry(bid).or_default().push(pid);
        }
        for (block_id, missing) in by_block {
            events.push(LossEvent {
                block_id,
                path_id,
                missing,
            });
        }
    }

    /// Number of un-acked batches currently tracked (tests/diagnostics).
    #[allow(dead_code)] // used by tests and external diagnostics (bin target compiles modules privately)
    pub fn ledger_len(&self) -> usize {
        self.ledger.len()
    }

    /// Retained blocks / bytes (tests/diagnostics).
    #[allow(dead_code)]
    pub fn retained_stats(&self) -> (usize, usize) {
        (self.retained.len(), self.retained_bytes)
    }

    // ------------------------------------------------------------------
    // Repair planning
    // ------------------------------------------------------------------

    /// Turn loss events into concrete repair sends.
    ///
    /// `eps_hat` is the current channel loss estimate: each event accrues
    /// `missing × ε̂ × 2^round` of fractional margin debt; whole margin
    /// symbols are emitted as they accumulate (continuous margin — no
    /// per-event ceil, honest at the ~1-symbol-per-batch granularity of
    /// MTU-sized batches).
    pub fn plan_repairs(&mut self, events: Vec<LossEvent>, eps_hat: f64) -> Vec<RepairPlan> {
        // Merge events per block (one encoder use per block per round).
        let mut merged: BTreeMap<u64, (u32, Vec<u32>)> = BTreeMap::new();
        for ev in events {
            let e = merged.entry(ev.block_id).or_insert((ev.path_id, Vec::new()));
            e.0 = ev.path_id;
            for pid in ev.missing {
                if !e.1.contains(&pid) {
                    e.1.push(pid);
                }
            }
        }

        let mut plans = Vec::new();
        for (block_id, (path_id, missing)) in merged {
            if missing.is_empty() || self.done_set.contains(&block_id) {
                continue;
            }
            let Some(rb) = self.retained.get_mut(&block_id) else {
                continue; // evicted — receiver eviction timeout is the backstop
            };
            if rb.rounds >= MAX_REPAIR_ROUNDS {
                continue;
            }

            // Continuous ε̂ margin, doubled per retry round.
            self.margin_debt += missing.len() as f64 * eps_hat * (1u32 << rb.rounds) as f64;
            let extra = self.margin_debt.floor() as u32;
            self.margin_debt -= extra as f64;

            let encoder = rb
                .encoder
                .get_or_insert_with(|| rb.backend.create_encoder(&rb.data, rb.params));

            let rateless = encoder.max_repairs() == u32::MAX;
            let mut symbols: Vec<WireSymbol>;
            if rateless {
                // Fresh repairs: any repair fills any hole.
                let count = missing.len() as u32 + extra;
                symbols = encoder.repair_symbols_from(rb.next_repair_esi, count);
                rb.next_repair_esi += count;
            } else {
                // Fixed-rate: resend the exact missing symbols.
                let k = rb.params.source_symbols;
                let want: HashSet<u32> = missing.iter().copied().collect();
                symbols = encoder
                    .source_symbols()
                    .into_iter()
                    .filter(|s| want.contains(&s.payload_id))
                    .collect();
                if missing.iter().any(|&pid| pid >= k) {
                    symbols.extend(
                        encoder
                            .repair_symbols(rb.params.repair_count)
                            .into_iter()
                            .filter(|s| want.contains(&s.payload_id)),
                    );
                }
                // Margin from any still-unsent parity capacity (self-clamps).
                if extra > 0 {
                    let minted = encoder.repair_symbols_from(rb.next_repair_esi, extra);
                    rb.next_repair_esi += minted.len() as u32;
                    symbols.extend(minted);
                }
            }

            if symbols.is_empty() {
                continue;
            }
            rb.rounds += 1;
            let plan = RepairPlan {
                block_id,
                avoid_path: path_id,
                symbols,
                params: rb.params,
                backend: rb.backend,
                transfer_length: rb.data.len() as u64,
            };
            self.touch_retained(block_id);
            plans.push(plan);
        }
        plans
    }

    /// A whole-block failure signal (BlockResult { success: false }): mint
    /// `deficit` fresh repairs with doubled margin, if the block is still
    /// retained and rateless. (The wire currently only sends BlockResult on
    /// success; this path is kept for completeness/forward-compat.)
    pub fn on_block_failed(
        &mut self,
        block_id: u64,
        deficit: u32,
        path_id: u32,
        eps_hat: f64,
    ) -> Option<RepairPlan> {
        if deficit == 0 {
            return None;
        }
        // Synthesize a loss event with unknown ids: rateless backends do
        // not need ids; fixed-rate backends cannot help here.
        let rb = self.retained.get_mut(&block_id)?;
        if rb.rounds >= MAX_REPAIR_ROUNDS || self.done_set.contains(&block_id) {
            return None;
        }
        let encoder = rb
            .encoder
            .get_or_insert_with(|| rb.backend.create_encoder(&rb.data, rb.params));
        if encoder.max_repairs() != u32::MAX {
            return None;
        }
        self.margin_debt += deficit as f64 * eps_hat * 2.0 * (1u32 << rb.rounds) as f64;
        let extra = self.margin_debt.floor() as u32;
        self.margin_debt -= extra as f64;
        let count = deficit + extra;
        let symbols = encoder.repair_symbols_from(rb.next_repair_esi, count);
        rb.next_repair_esi += count;
        rb.rounds += 1;
        let plan = RepairPlan {
            block_id,
            avoid_path: path_id,
            symbols,
            params: rb.params,
            backend: rb.backend,
            transfer_length: rb.data.len() as u64,
        };
        self.touch_retained(block_id);
        Some(plan)
    }
}

impl Default for BlockArq {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: Duration = Duration::from_millis(0);
    const TIMEOUT: Duration = Duration::from_millis(100);

    fn params(k: u32, sym: u16, r: u32, block_id: u64) -> EncodingParams {
        EncodingParams {
            source_symbols: k,
            symbol_size: sym,
            repair_count: r,
            block_id,
        }
    }

    fn retain_block(arq: &mut BlockArq, block_id: u64, len: usize, backend: FecBackend) -> Bytes {
        let data = Bytes::from(vec![(block_id & 0xff) as u8; len]);
        let k = (len as f64 / 64.0).ceil() as u32;
        arq.on_block_encoded(block_id, data.clone(), params(k, 64, 2, block_id), backend, Instant::now());
        data
    }

    #[test]
    fn ack_full_batch_no_events() {
        let mut arq = BlockArq::new();
        let now = Instant::now();
        arq.on_batch_sent(1, 0, vec![(10, 0), (10, 1)], now);
        let ev = arq.on_ack(1, 0, &[0, 1], now + T0, TIMEOUT);
        assert!(ev.is_empty());
        assert_eq!(arq.ledger_len(), 0);
    }

    #[test]
    fn ack_partial_batch_diffs_missing() {
        let mut arq = BlockArq::new();
        let now = Instant::now();
        arq.on_batch_sent(1, 0, vec![(10, 0), (10, 1), (10, 2)], now);
        let ev = arq.on_ack(1, 0, &[0, 2], now, TIMEOUT);
        assert_eq!(
            ev,
            vec![LossEvent {
                block_id: 10,
                path_id: 0,
                missing: vec![1]
            }]
        );
    }

    #[test]
    fn mixed_block_batch_duplicate_payload_ids() {
        // Two blocks contribute payload_id 3; only one instance arrives.
        // Multiset matching must charge exactly one loss (which block is
        // ambiguous from the Ack alone — datagram atomicity makes this a
        // theoretical case; we only require no over/under-counting).
        let mut arq = BlockArq::new();
        let now = Instant::now();
        arq.on_batch_sent(7, 2, vec![(100, 3), (200, 3), (200, 4)], now);
        let ev = arq.on_ack(7, 2, &[3, 4], now, TIMEOUT);
        let total_missing: usize = ev.iter().map(|e| e.missing.len()).sum();
        assert_eq!(total_missing, 1);
        assert!(ev.iter().all(|e| e.path_id == 2));
    }

    #[test]
    fn dup_ack_threshold_declares_loss() {
        let mut arq = BlockArq::new();
        let now = Instant::now();
        arq.on_batch_sent(1, 0, vec![(10, 0)], now); // will be lost
        for seq in 2..=4 {
            arq.on_batch_sent(seq, 0, vec![(10, seq as u32)], now);
        }
        // Two later acks: not yet lost.
        assert!(arq.on_ack(2, 0, &[2], now, TIMEOUT).is_empty());
        assert!(arq.on_ack(3, 0, &[3], now, TIMEOUT).is_empty());
        // Third later ack crosses LATER_ACK_LOSS_THRESHOLD.
        let ev = arq.on_ack(4, 0, &[4], now, TIMEOUT);
        assert_eq!(
            ev,
            vec![LossEvent {
                block_id: 10,
                path_id: 0,
                missing: vec![0]
            }]
        );
        assert_eq!(arq.ledger_len(), 0);
    }

    #[test]
    fn other_path_acks_do_not_count() {
        let mut arq = BlockArq::new();
        let now = Instant::now();
        arq.on_batch_sent(1, 0, vec![(10, 0)], now);
        for seq in 2..=6 {
            arq.on_batch_sent(seq, 1, vec![(10, seq as u32)], now);
            assert!(
                arq.on_ack(seq, 1, &[seq as u32], now, TIMEOUT).is_empty(),
                "path-1 acks must not declare a path-0 batch lost"
            );
        }
        assert_eq!(arq.ledger_len(), 1);
    }

    #[test]
    fn lost_ack_no_spurious_repair_before_timeout() {
        // Batch delivered but its Ack lost: nothing should fire until the
        // dup-ack evidence or timeout — a sweep inside the timeout is a
        // no-op.
        let mut arq = BlockArq::new();
        let now = Instant::now();
        arq.on_batch_sent(1, 0, vec![(10, 0)], now);
        let ev = arq.sweep(now + Duration::from_millis(50), &|_| TIMEOUT);
        assert!(ev.is_empty());
        assert_eq!(arq.ledger_len(), 1);
        // Past the timeout the batch is delivered-or-lost either way; it
        // fires exactly once (entry removed — no duplicate on re-sweep).
        let ev = arq.sweep(now + Duration::from_millis(150), &|_| TIMEOUT);
        assert_eq!(ev.len(), 1);
        assert!(arq.sweep(now + Duration::from_millis(300), &|_| TIMEOUT).is_empty());
    }

    #[test]
    fn reordered_ack_for_declared_batch_is_harmless() {
        let mut arq = BlockArq::new();
        let now = Instant::now();
        arq.on_batch_sent(1, 0, vec![(10, 0)], now);
        for seq in 2..=4 {
            arq.on_batch_sent(seq, 0, vec![(10, seq as u32)], now);
            arq.on_ack(seq, 0, &[seq as u32], now, TIMEOUT);
        }
        // Batch 1 was declared lost above; its late Ack must not panic or
        // emit events.
        assert!(arq.on_ack(1, 0, &[0], now, TIMEOUT).is_empty());
    }

    #[test]
    fn done_blocks_suppress_events() {
        let mut arq = BlockArq::new();
        let now = Instant::now();
        arq.on_batch_sent(1, 0, vec![(10, 0)], now);
        arq.on_block_done(10);
        let ev = arq.sweep(now + TIMEOUT, &|_| TIMEOUT);
        assert!(ev.is_empty(), "decoded block must not trigger repairs");
    }

    #[test]
    fn ledger_cap_drops_oldest() {
        let mut arq = BlockArq::with_caps(4, RETAIN_MAX_BLOCKS, RETAIN_MAX_BYTES);
        let now = Instant::now();
        for seq in 0..10u64 {
            arq.on_batch_sent(seq, 0, vec![(seq, 0)], now);
        }
        assert_eq!(arq.ledger_len(), 4);
    }

    #[test]
    fn retention_count_and_byte_caps() {
        let mut arq = BlockArq::with_caps(LEDGER_MAX_BATCHES, 4, 10_000);
        for b in 0..8u64 {
            retain_block(&mut arq, b, 1000, FecBackend::RaptorQ);
        }
        let (blocks, bytes) = arq.retained_stats();
        assert_eq!(blocks, 4, "count cap");
        assert!(bytes <= 10_000);

        // Byte cap dominates: 3 × 4000 > 10000 → evicts to 2.
        let mut arq = BlockArq::with_caps(LEDGER_MAX_BATCHES, 64, 10_000);
        for b in 0..3u64 {
            retain_block(&mut arq, b, 4000, FecBackend::RaptorQ);
        }
        let (blocks, bytes) = arq.retained_stats();
        assert_eq!(blocks, 2, "byte cap");
        assert!(bytes <= 10_000);
    }

    #[test]
    fn rateless_repairs_are_fresh_and_distinct() {
        let mut arq = BlockArq::new();
        let data = Bytes::from(vec![7u8; 640]);
        let p = params(10, 64, 3, 42);
        arq.on_block_encoded(42, data, p, FecBackend::RaptorQ, Instant::now());

        let ev = vec![LossEvent {
            block_id: 42,
            path_id: 0,
            missing: vec![2, 5],
        }];
        let plans = arq.plan_repairs(ev, 0.0);
        assert_eq!(plans.len(), 1);
        let first_ids: Vec<u32> = plans[0].symbols.iter().map(|s| s.payload_id).collect();
        assert_eq!(plans[0].symbols.len(), 2);
        assert!(plans[0].symbols.iter().all(|s| s.is_repair));

        // A second round must mint DIFFERENT repair symbols.
        let ev = vec![LossEvent {
            block_id: 42,
            path_id: 0,
            missing: vec![2, 5],
        }];
        let plans2 = arq.plan_repairs(ev, 0.0);
        assert_eq!(plans2.len(), 1);
        for s in &plans2[0].symbols {
            assert!(
                !first_ids.contains(&s.payload_id),
                "fresh repairs must not repeat earlier ESIs"
            );
        }
    }

    #[test]
    fn fixed_rate_resends_exact_missing() {
        let mut arq = BlockArq::new();
        let data = Bytes::from(vec![9u8; 640]);
        let p = params(10, 64, 3, 43);
        arq.on_block_encoded(43, data, p, FecBackend::ReedSolomon, Instant::now());

        // Missing: source 4 and repair 11 (k=10 → repair ids 10..13).
        let ev = vec![LossEvent {
            block_id: 43,
            path_id: 1,
            missing: vec![4, 11],
        }];
        let plans = arq.plan_repairs(ev, 0.0);
        assert_eq!(plans.len(), 1);
        let mut ids: Vec<u32> = plans[0].symbols.iter().map(|s| s.payload_id).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![4, 11], "exact missing symbols resent");
    }

    #[test]
    fn margin_accumulates_fractionally_and_doubles_on_retry() {
        let mut arq = BlockArq::new();
        for b in 0..20u64 {
            let data = Bytes::from(vec![b as u8; 640]);
            arq.on_block_encoded(b, data, params(10, 64, 2, b), FecBackend::RaptorQ, Instant::now());
        }
        // ε̂ = 0.25, 1 missing per event, round 0 → 0.25 margin per event:
        // every 4th event carries one extra symbol.
        let mut total = 0usize;
        for b in 0..8u64 {
            let plans = arq.plan_repairs(
                vec![LossEvent {
                    block_id: b,
                    path_id: 0,
                    missing: vec![0],
                }],
                0.25,
            );
            total += plans[0].symbols.len();
        }
        assert_eq!(total, 8 + 2, "8 missing + floor(8×0.25) margin symbols");

        // Retry on an already-repaired block doubles the margin rate:
        // round 1 → 1 missing × 0.25 × 2 = 0.5 debt (was 0.25).
        let before = arq.margin_debt;
        let plans = arq.plan_repairs(
            vec![LossEvent {
                block_id: 0,
                path_id: 0,
                missing: vec![0],
            }],
            0.25,
        );
        assert_eq!(plans.len(), 1);
        let debt_gain = arq.margin_debt - before + plans[0].symbols.len() as f64 - 1.0;
        assert!(
            (debt_gain - 0.5).abs() < 1e-9,
            "round-1 margin must be doubled (got {debt_gain})"
        );
    }

    #[test]
    fn repair_rounds_capped() {
        let mut arq = BlockArq::new();
        let data = Bytes::from(vec![1u8; 640]);
        arq.on_block_encoded(50, data, params(10, 64, 2, 50), FecBackend::RaptorQ, Instant::now());
        for round in 0..MAX_REPAIR_ROUNDS + 2 {
            let plans = arq.plan_repairs(
                vec![LossEvent {
                    block_id: 50,
                    path_id: 0,
                    missing: vec![0],
                }],
                0.0,
            );
            if round < MAX_REPAIR_ROUNDS {
                assert_eq!(plans.len(), 1, "round {round} should plan");
            } else {
                assert!(plans.is_empty(), "round {round} must be capped");
            }
        }
    }

    #[test]
    fn evicted_block_yields_no_plan() {
        let mut arq = BlockArq::new();
        let ev = vec![LossEvent {
            block_id: 999,
            path_id: 0,
            missing: vec![0],
        }];
        assert!(arq.plan_repairs(ev, 0.1).is_empty());
    }

    #[test]
    fn block_failed_mints_deficit_repairs() {
        let mut arq = BlockArq::new();
        let data = Bytes::from(vec![3u8; 640]);
        arq.on_block_encoded(60, data, params(10, 64, 2, 60), FecBackend::RaptorQ, Instant::now());
        let plan = arq.on_block_failed(60, 3, 0, 0.0).expect("plan");
        assert_eq!(plan.symbols.len(), 3);
        assert!(plan.symbols.iter().all(|s| s.is_repair));
        // Fixed-rate backends cannot mint post-hoc without ids.
        let data = Bytes::from(vec![3u8; 640]);
        arq.on_block_encoded(61, data, params(10, 64, 2, 61), FecBackend::ReedSolomon, Instant::now());
        assert!(arq.on_block_failed(61, 3, 0, 0.0).is_none());
    }

    #[test]
    fn idle_reannounce_recovers_orphaned_block() {
        // Reproduce the L1 idle-stall: a block's BlockStart datagram is lost,
        // its symbols are all delivered-and-acked, so the ARQ ledger is EMPTY
        // and `sweep` sees nothing — yet the block never decoded. The idle
        // re-announce must re-send BlockStart (+ spare) once the block has been
        // quiet past the loss timeout, and stop the instant it completes.
        let mut arq = BlockArq::new();
        let t0 = Instant::now();
        let data = Bytes::from(vec![7u8; 640]);
        arq.on_block_encoded(70, data, params(10, 64, 2, 70), FecBackend::RaptorQ, t0);
        // Its batch was sent AND acked (ledger cleared): activity at t0.
        arq.on_batch_sent(1, 0, vec![(70, 0)], t0);
        arq.on_ack(1, 0, &[0], t0, TIMEOUT);
        assert_eq!(arq.ledger_len(), 0, "ledger empty — sweep is blind here");
        assert!(arq.sweep(t0 + TIMEOUT, &|_| TIMEOUT).is_empty());

        // Before the timeout: no re-announce (block might still be pipelining).
        let early = arq.idle_reannounce(t0 + Duration::from_millis(20), &|_| TIMEOUT, 0, 0.1);
        assert!(early.is_empty(), "must not fire during normal pipelining");

        // Past the timeout: re-announce fires with a BlockStart + spare.
        let plans = arq.idle_reannounce(t0 + Duration::from_millis(150), &|_| TIMEOUT, 0, 0.1);
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].block_id, 70);
        assert!(!plans[0].symbols.is_empty(), "spare repair accompanies re-announce");

        // It backs off within a round (last_activity refreshed): immediate
        // re-call is a no-op until the next timeout elapses.
        assert!(arq
            .idle_reannounce(t0 + Duration::from_millis(160), &|_| TIMEOUT, 0, 0.1)
            .is_empty());

        // Block completes → re-announce stops permanently.
        arq.on_block_done(70);
        assert!(arq
            .idle_reannounce(t0 + Duration::from_millis(400), &|_| TIMEOUT, 0, 0.1)
            .is_empty());
    }

    #[test]
    fn idle_reannounce_bounded_by_round_cap() {
        let mut arq = BlockArq::new();
        let t0 = Instant::now();
        let data = Bytes::from(vec![1u8; 640]);
        arq.on_block_encoded(71, data, params(10, 64, 2, 71), FecBackend::RaptorQ, t0);
        let mut fired = 0u8;
        let mut counts: Vec<usize> = Vec::new();
        // Keep the block un-done and always past-timeout: it must fire at most
        // MAX_REANNOUNCE_ROUNDS times, then give way to the receiver backstop.
        for r in 0..(MAX_REANNOUNCE_ROUNDS as u32 + 4) {
            let now = t0 + Duration::from_millis(200 * (r as u64 + 1));
            let plans = arq.idle_reannounce(now, &|_| TIMEOUT, 0, 0.1);
            if let Some(p) = plans.first() {
                fired += 1;
                counts.push(p.symbols.len());
            }
        }
        assert_eq!(fired, MAX_REANNOUNCE_ROUNDS, "re-announce is bounded");
        // Escalation: the spare grows across rounds (cheap probe -> full block).
        assert!(
            counts.last().unwrap() > counts.first().unwrap(),
            "spare must escalate: {counts:?}"
        );
    }
}
