//! Block interleaving buffer.
//!
//! Spreads symbols from multiple blocks across time so a single burst loss
//! is distributed across N blocks instead of concentrated on one.
//!
//! The buffer sits between the scheduler (which assigns symbols to paths)
//! and the transport (which sends them). Symbols from up to `depth` blocks
//! accumulate, then drain in round-robin order across blocks.

use crate::fec::WireSymbol;
use crate::scheduler::PathId;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Interleaving buffer that holds symbols from multiple blocks and emits
/// them in round-robin order across blocks.
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
    /// Create a new interleaving buffer.
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
    /// Drains completely: all buffered blocks are emptied.
    pub fn drain(&mut self) -> Vec<(PathId, Vec<WireSymbol>)> {
        if self.slots.is_empty() {
            return vec![];
        }
        self.drain_inner()
    }

    /// Force-drain all remaining symbols (for shutdown).
    pub fn drain_all(&mut self) -> Vec<(PathId, Vec<WireSymbol>)> {
        self.drain_inner()
    }

    /// Core drain: round-robin across blocks, per path.
    fn drain_inner(&mut self) -> Vec<(PathId, Vec<WireSymbol>)> {
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

    /// Returns the deadline for the oldest slot, if any.
    pub fn oldest_deadline(&self) -> Option<Instant> {
        self.slots
            .front()
            .map(|s| s.created_at + self.timeout)
    }

    /// Whether the buffer has any pending symbols.
    pub fn is_empty(&self) -> bool {
        self.total_buffered == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(block_id: u64, payload_id: u32) -> WireSymbol {
        WireSymbol {
            block_id,
            payload_id,
            is_repair: false,
            data: vec![0u8; 64],
        }
    }

    #[test]
    fn test_depth_1_passthrough() {
        let mut buf = InterleavingBuffer::new(1, Duration::from_secs(10));
        let assignments = vec![(0u32, vec![sym(0, 0), sym(0, 1), sym(0, 2)])];
        buf.push_block(0, assignments);

        // Depth 1 = immediate drain
        assert!(buf.should_drain());
        let result = buf.drain();
        assert_eq!(result.len(), 1);
        let (pid, syms) = &result[0];
        assert_eq!(*pid, 0);
        assert_eq!(syms.len(), 3);
        // Order preserved when only one block
        assert_eq!(syms[0].payload_id, 0);
        assert_eq!(syms[1].payload_id, 1);
        assert_eq!(syms[2].payload_id, 2);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_interleave_two_blocks() {
        let mut buf = InterleavingBuffer::new(2, Duration::from_secs(10));

        buf.push_block(0, vec![(0u32, vec![sym(0, 0), sym(0, 1), sym(0, 2)])]);
        assert!(!buf.should_drain()); // only 1 block

        buf.push_block(1, vec![(0u32, vec![sym(1, 10), sym(1, 11), sym(1, 12)])]);
        assert!(buf.should_drain()); // 2 blocks = depth reached

        let result = buf.drain();
        let (_, syms) = result.iter().find(|(pid, _)| *pid == 0).unwrap();
        // Should be interleaved: block0, block1, block0, block1, ...
        assert_eq!(syms.len(), 6);
        assert_eq!(syms[0].block_id, 0);
        assert_eq!(syms[1].block_id, 1);
        assert_eq!(syms[2].block_id, 0);
        assert_eq!(syms[3].block_id, 1);
        assert_eq!(syms[4].block_id, 0);
        assert_eq!(syms[5].block_id, 1);
    }

    #[test]
    fn test_interleave_unequal_sizes() {
        let mut buf = InterleavingBuffer::new(2, Duration::from_secs(10));
        buf.push_block(0, vec![(0u32, vec![sym(0, 0), sym(0, 1), sym(0, 2), sym(0, 3), sym(0, 4)])]);
        buf.push_block(1, vec![(0u32, vec![sym(1, 10), sym(1, 11)])]);

        let result = buf.drain();
        let (_, syms) = result.iter().find(|(pid, _)| *pid == 0).unwrap();
        assert_eq!(syms.len(), 7);
        // First passes: alternating block0, block1
        assert_eq!(syms[0].block_id, 0);
        assert_eq!(syms[1].block_id, 1);
        assert_eq!(syms[2].block_id, 0);
        assert_eq!(syms[3].block_id, 1);
        // Block 1 exhausted, remaining block 0 symbols
        assert_eq!(syms[4].block_id, 0);
        assert_eq!(syms[5].block_id, 0);
        assert_eq!(syms[6].block_id, 0);
    }

    #[test]
    fn test_drain_all_before_depth() {
        let mut buf = InterleavingBuffer::new(4, Duration::from_secs(10));
        buf.push_block(0, vec![(0u32, vec![sym(0, 0), sym(0, 1)])]);
        buf.push_block(1, vec![(0u32, vec![sym(1, 10)])]);

        // Only 2 blocks, depth is 4
        assert!(!buf.should_drain());

        // drain_all forces it
        let result = buf.drain_all();
        let (_, syms) = result.iter().find(|(pid, _)| *pid == 0).unwrap();
        assert_eq!(syms.len(), 3);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_time_flush() {
        let mut buf = InterleavingBuffer::new(4, Duration::from_millis(0));
        buf.push_block(0, vec![(0u32, vec![sym(0, 0)])]);

        // Timeout is 0ms, so should drain immediately
        assert!(buf.should_drain());
    }

    #[test]
    fn test_max_buffered_triggers_drain() {
        let mut buf = InterleavingBuffer::new(10, Duration::from_secs(60));
        buf.max_buffered = 5;

        buf.push_block(0, vec![(0u32, vec![sym(0, 0), sym(0, 1), sym(0, 2)])]);
        assert!(!buf.should_drain()); // 3 < 5

        buf.push_block(1, vec![(0u32, vec![sym(1, 10), sym(1, 11), sym(1, 12)])]);
        assert!(buf.should_drain()); // 6 >= 5
    }

    #[test]
    fn test_multi_path_interleave() {
        let mut buf = InterleavingBuffer::new(2, Duration::from_secs(10));

        // Block 0: path 0 gets 2 symbols, path 1 gets 1 symbol
        buf.push_block(0, vec![
            (0u32, vec![sym(0, 0), sym(0, 1)]),
            (1u32, vec![sym(0, 100)]),
        ]);
        // Block 1: path 0 gets 1 symbol, path 1 gets 2 symbols
        buf.push_block(1, vec![
            (0u32, vec![sym(1, 10)]),
            (1u32, vec![sym(1, 110), sym(1, 111)]),
        ]);

        let result = buf.drain();

        let path0_syms: Vec<_> = result.iter()
            .filter(|(pid, _)| *pid == 0)
            .flat_map(|(_, s)| s.iter())
            .collect();
        assert_eq!(path0_syms.len(), 3);
        // Interleaved: block0, block1, block0
        assert_eq!(path0_syms[0].block_id, 0);
        assert_eq!(path0_syms[1].block_id, 1);
        assert_eq!(path0_syms[2].block_id, 0);

        let path1_syms: Vec<_> = result.iter()
            .filter(|(pid, _)| *pid == 1)
            .flat_map(|(_, s)| s.iter())
            .collect();
        assert_eq!(path1_syms.len(), 3);
        // Interleaved: block0, block1, block1
        assert_eq!(path1_syms[0].block_id, 0);
        assert_eq!(path1_syms[1].block_id, 1);
        assert_eq!(path1_syms[2].block_id, 1);

        assert!(buf.is_empty());
    }

    #[test]
    fn test_empty_drain_returns_empty() {
        let mut buf = InterleavingBuffer::new(2, Duration::from_secs(10));
        assert!(!buf.should_drain());
        assert!(buf.drain().is_empty());
    }
}
