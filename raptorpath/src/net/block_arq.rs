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
    pub fn ledger_len(&self) -> usize {
        self.ledger.len()
    }

    /// Retained blocks / bytes (tests/diagnostics).
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
mod tests;
