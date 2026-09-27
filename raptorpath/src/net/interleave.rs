//! Block interleaving buffer with tapered repair distribution.
//!
//! Spreads symbols from multiple blocks across time so a single burst loss
//! is distributed across N blocks instead of concentrated on one.
//!
//! The buffer sits between the scheduler (which assigns symbols to paths)
//! and the transport (which sends them). Symbols from up to `depth` blocks
//! accumulate, then drain in round-robin order across blocks.
//!
//! **Tapered interleaving**: When enabled, repairs from block B are interleaved
//! with block B+1's sources using an exponential decay schedule. This front-loads
//! repairs where they have the highest marginal recovery value, then tapers off.
//! The decay rate adapts to measured loss — higher loss = gentler slope.

use crate::fec::WireSymbol;
use crate::scheduler::PathId;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Compute the exponential taper schedule: how many repairs to insert at each
/// source position.
///
/// Returns a Vec of length `source_count` where entry `i` is the number of
/// repairs to emit after source `i`.
pub fn compute_taper_schedule(repair_count: usize, source_count: usize, loss_rate: f64) -> Vec<usize> {
    if source_count == 0 || repair_count == 0 {
        return vec![0; source_count];
    }

    // λ adapts to loss: high loss → gentler decay (smaller λ), low loss → steep
    // λ = -ln(0.01) / (1 + 10 * loss_rate) ≈ 4.6 / (1 + 10 * loss)
    let lambda = 4.605 / (1.0 + 10.0 * loss_rate.clamp(0.0, 1.0));

    // Compute raw weights: weight(i) = exp(-λ * i/k)
    let k = source_count as f64;
    let weights: Vec<f64> = (0..source_count)
        .map(|i| (-lambda * i as f64 / k).exp())
        .collect();
    let total_weight: f64 = weights.iter().sum();

    if total_weight < f64::EPSILON {
        return vec![0; source_count];
    }

    // Distribute repairs proportionally, rounding to integers
    let mut schedule = vec![0usize; source_count];
    let mut assigned = 0usize;
    for i in 0..source_count {
        let raw = repair_count as f64 * weights[i] / total_weight;
        schedule[i] = raw.round() as usize;
        assigned += schedule[i];
    }

    // Fix rounding residual: add/remove from front (highest priority)
    if assigned < repair_count {
        schedule[0] += repair_count - assigned;
    } else if assigned > repair_count {
        let mut excess = assigned - repair_count;
        // Remove from the tail (lowest priority positions)
        for i in (0..source_count).rev() {
            if excess == 0 {
                break;
            }
            let remove = schedule[i].min(excess);
            schedule[i] -= remove;
            excess -= remove;
        }
    }

    schedule
}

/// Interleaving buffer that holds symbols from multiple blocks and emits
/// them in round-robin order across blocks.
///
/// Supports tapered interleaving: repairs from block B are front-loaded
/// into block B+1's sources using exponential decay.
pub struct InterleavingBuffer {
    /// Pending block slots, in insertion order.
    slots: VecDeque<BlockSlot>,
    /// How many blocks to buffer before draining.
    depth: usize,
    /// Max total symbols before forcing a drain.
    max_buffered: usize,
    /// Timeout: force-drain if oldest slot exceeds this age.
    timeout: Duration,
    /// Current total symbols buffered.
    total_buffered: usize,
    /// Previous block's repairs that are pending interleave with the next block.
    pending_repairs: HashMap<PathId, Vec<WireSymbol>>,
    /// Whether tapered interleaving is enabled.
    tapered: bool,
}

struct BlockSlot {
    block_id: u64,
    /// Symbols assigned to each path, not yet sent.
    per_path: HashMap<PathId, VecDeque<WireSymbol>>,
    /// When this slot was created.
    created_at: Instant,
    /// Total symbols remaining in this slot.
    remaining: usize,
}

impl InterleavingBuffer {
    /// Create a new interleaving buffer (flat round-robin, no tapering).
    ///
    /// - `depth`: how many blocks to accumulate before draining (1 = no interleaving).
    /// - `timeout`: force-drain if the oldest block is older than this.
    pub fn new(depth: usize, timeout: Duration) -> Self {
        let depth = depth.max(1);
        Self {
            slots: VecDeque::new(),
            depth,
            max_buffered: 1024,
            timeout,
            total_buffered: 0,
            pending_repairs: HashMap::new(),
            tapered: false,
        }
    }

    /// Create a new interleaving buffer with tapered repair distribution.
    ///
    /// Repairs from block B are front-loaded into block B+1's sources using
    /// exponential decay adapted to the current loss rate.
    pub fn new_tapered(depth: usize, timeout: Duration) -> Self {
        let depth = depth.max(2); // tapered needs at least 2 blocks
        Self {
            slots: VecDeque::new(),
            depth,
            max_buffered: 1024,
            timeout,
            total_buffered: 0,
            pending_repairs: HashMap::new(),
            tapered: true,
        }
    }

    /// Push symbols from a newly encoded block.
    ///
    /// `assignments` comes from `scheduler.schedule()`: Vec<(PathId, Vec<WireSymbol>)>.
    pub fn push_block(&mut self, block_id: u64, assignments: Vec<(PathId, Vec<WireSymbol>)>) {
        let mut per_path = HashMap::new();
        let mut count = 0;
        for (path_id, symbols) in assignments {
            count += symbols.len();
            per_path.insert(path_id, VecDeque::from(symbols));
        }
        self.total_buffered += count;
        self.slots.push_back(BlockSlot {
            block_id,
            per_path,
            created_at: Instant::now(),
            remaining: count,
        });
    }

    /// Check if a drain should happen.
    pub fn should_drain(&self) -> bool {
        if self.slots.is_empty() {
            return false;
        }
        // Depth reached
        if self.slots.len() >= self.depth {
            return true;
        }
        // Buffer size exceeded
        if self.total_buffered >= self.max_buffered {
            return true;
        }
        // Timeout
        if let Some(oldest) = self.slots.front() {
            if oldest.created_at.elapsed() >= self.timeout {
                return true;
            }
        }
        false
    }

    /// Drain interleaved symbols. Returns batches per path with symbols
    /// from different blocks interleaved in round-robin order.
    ///
    /// When tapered mode is enabled, `loss_rate` controls the taper decay.
    /// In flat mode, `loss_rate` is ignored.
    ///
    /// Drains completely: all buffered blocks are emptied.
    pub fn drain(&mut self, loss_rate: f64) -> Vec<(PathId, Vec<WireSymbol>)> {
        if self.slots.is_empty() {
            return vec![];
        }
        if self.tapered {
            self.drain_tapered(loss_rate, false)
        } else {
            self.drain_flat()
        }
    }

    /// Force-drain all remaining symbols (for shutdown).
    pub fn drain_all(&mut self, loss_rate: f64) -> Vec<(PathId, Vec<WireSymbol>)> {
        if self.tapered {
            self.drain_tapered(loss_rate, true)
        } else {
            self.drain_flat()
        }
    }

    /// Core flat drain: round-robin across blocks, per path.
    fn drain_flat(&mut self) -> Vec<(PathId, Vec<WireSymbol>)> {
        // Collect all path IDs across all slots.
        let path_ids: Vec<PathId> = {
            let mut ids: Vec<PathId> = self
                .slots
                .iter()
                .flat_map(|s| s.per_path.keys().copied())
                .collect();
            ids.sort_unstable();
            ids.dedup();
            ids
        };

        let mut result: HashMap<PathId, Vec<WireSymbol>> = HashMap::new();

        // Round-robin: pop one symbol per block per path, repeat until empty.
        loop {
            let mut made_progress = false;
            for slot in self.slots.iter_mut() {
                for &pid in &path_ids {
                    if let Some(queue) = slot.per_path.get_mut(&pid) {
                        if let Some(sym) = queue.pop_front() {
                            result.entry(pid).or_default().push(sym);
                            slot.remaining -= 1;
                            self.total_buffered -= 1;
                            made_progress = true;
                        }
                    }
                }
            }
            if !made_progress {
                break;
            }
        }

        // Remove empty slots.
        self.slots.retain(|s| s.remaining > 0);

        result.into_iter().collect()
    }

    /// Tapered drain: interleave previous block's repairs with current block's
    /// sources using exponential decay front-loading.
    ///
    /// For each consecutive pair of blocks (B, B+1):
    /// - Split B into sources and repairs
    /// - Emit B's sources
    /// - Interleave B's repairs into B+1's source stream using taper schedule
    /// - B+1's repairs become `pending_repairs` for the next drain
    fn drain_tapered(&mut self, loss_rate: f64, flush: bool) -> Vec<(PathId, Vec<WireSymbol>)> {
        let path_ids: Vec<PathId> = {
            let mut ids: Vec<PathId> = self
                .slots
                .iter()
                .flat_map(|s| s.per_path.keys().copied())
                .chain(self.pending_repairs.keys().copied())
                .collect();
            ids.sort_unstable();
            ids.dedup();
            ids
        };

        let mut result: HashMap<PathId, Vec<WireSymbol>> = HashMap::new();

        // Take all slots out for processing
        let slots: Vec<BlockSlot> = self.slots.drain(..).collect();
        self.total_buffered = 0;

        for (_slot_idx, slot) in slots.into_iter().enumerate() {
            for &pid in &path_ids {
                let symbols: Vec<WireSymbol> = match slot.per_path.get(&pid) {
                    Some(q) => q.iter().cloned().collect(),
                    None => vec![],
                };

                // Split into sources and repairs
                let mut sources: Vec<WireSymbol> = Vec::new();
                let mut repairs: Vec<WireSymbol> = Vec::new();
                for sym in symbols {
                    if sym.is_repair {
                        repairs.push(sym);
                    } else {
                        sources.push(sym);
                    }
                }

                // Interleave any pending repairs (from previous block) into this
                // block's source stream
                let prev_repairs = self.pending_repairs.remove(&pid).unwrap_or_default();
                if !prev_repairs.is_empty() && !sources.is_empty() {
                    let schedule = compute_taper_schedule(
                        prev_repairs.len(),
                        sources.len(),
                        loss_rate,
                    );
                    let out = result.entry(pid).or_default();
                    let mut repair_iter = prev_repairs.into_iter();
                    for (i, src) in sources.into_iter().enumerate() {
                        out.push(src);
                        for _ in 0..schedule[i] {
                            if let Some(rep) = repair_iter.next() {
                                out.push(rep);
                            }
                        }
                    }
                    // Any leftover repairs (shouldn't happen, but be safe)
                    out.extend(repair_iter);
                } else if !prev_repairs.is_empty() {
                    // No sources in this slot for this path — emit repairs directly
                    result.entry(pid).or_default().extend(prev_repairs);
                    result.entry(pid).or_default().extend(sources);
                } else {
                    // No pending repairs — just emit sources
                    result.entry(pid).or_default().extend(sources);
                }

                // This block's repairs become pending for the next block
                if !repairs.is_empty() {
                    self.pending_repairs
                        .entry(pid)
                        .or_default()
                        .extend(repairs);
                }
            }
        }

        // If flushing (shutdown), emit any remaining pending repairs
        if flush {
            for (pid, repairs) in self.pending_repairs.drain() {
                result.entry(pid).or_default().extend(repairs);
            }
        }

        result.into_iter().collect()
    }

    /// Returns the deadline for the oldest slot, if any.
    pub fn oldest_deadline(&self) -> Option<Instant> {
        self.slots
            .front()
            .map(|s| s.created_at + self.timeout)
    }

    /// Whether the buffer has any pending symbols (including pending tapered repairs).
    pub fn is_empty(&self) -> bool {
        self.total_buffered == 0 && self.pending_repairs.values().all(|v| v.is_empty())
    }
}

#[cfg(test)]
mod tests;
