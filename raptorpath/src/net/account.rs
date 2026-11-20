//! The sender's recovery account (ADR-0081, plan S4, option B).
//!
//! One identity per sender, one formula on both seats (no `reliable` branch):
//!
//! ```text
//! S_sent = A + H + E + O_serv + O_strand
//! ```
//!
//! * `S_sent`: source seqs entered into the retransmit buffer;
//! * `A`: removed by the cumulative ack while still inside the coding window;
//! * `H`: shed (the ρ budget's sanctioned loss);
//! * `E`: removed by the cumulative ack after the coding window had passed
//!   them, with no verdict from the sender (legal; `H + E` is bounded by the
//!   1 − ρ budget);
//! * `O_serv`: outstanding seqs at or above the gap server's clamp bound;
//! * `O_strand`: outstanding seqs below it (`serve_gaps` cannot reach them).
//!
//! The clamp bound is [`clamp_lo`], the lower bound `serve_gaps` clamps with:
//! `0` where the whole store is served (reliable seat), else the window
//! start. So `E` and `O_strand` are 0 there by construction.
//!
//! Compiled only under `any(debug_assertions, feature = "test-seams")`, so the
//! release binary is unchanged (proved by the binid `.text`/`.rodata` hash).
//! Every counter is measured at its own site, so the identity is a check and
//! not a definition.

use super::sender_phases::gap_clamp;
use super::seq_ring::SeqRing;

/// The lower bound `serve_gaps` clamps a gap to: the whole store on the
/// reliable seat, the coding window on the evicting seat. Read off the same
/// function the server calls, so the two cannot disagree.
#[inline]
pub(crate) fn clamp_lo(reliable: bool, win_start: u64) -> u64 {
    gap_clamp!(reliable, 0u64, u64::MAX, win_start, u64::MAX).0
}

/// The retransmit buffer the account reads.
pub(crate) type Buf = SeqRing<(u64, u32)>;

#[derive(Debug, Default, Clone)]
pub(crate) struct Account {
    pub reliable: bool,
    /// `S_sent`.
    pub sent: u64,
    /// `A`.
    pub acked: u64,
    /// `E`.
    pub evicted: u64,
    /// Sheds by decision site (`H` = their sum). Same names as the
    /// `c8-gauges` branch's `SenderState` fields.
    pub shed_slot: u64,
    pub shed_nack: u64,
    /// Highest clamp bound seen at a window move: seqs below it that are
    /// still buffered left the window without a verdict.
    pub evict_floor: u64,
    /// High-water mark of the buffer length, against `store_cap`.
    pub o_hwm: u64,
    /// The effective store cap in force when `o_hwm` last rose.
    pub cap_at_hwm: u64,
    /// The effective store cap at the last loop iteration.
    pub store_cap: u64,
    /// Highest live 1 − ρ budget fraction seen.
    pub bud_max: f64,
    /// Execution witnesses: shed-check visits per site, clamp evaluations and
    /// the ones that excluded the whole gap.
    pub shed_visit_slot: u64,
    pub shed_visit_nack: u64,
    pub clamp_evals: u64,
    pub clamp_excl: u64,
    /// Tail-sweep fires, the ones whose gap produced zero sends, and how many
    /// consecutive sweeps found the same oldest un-acked seq (now / worst).
    pub sweeps: u64,
    /// Sweeps that sent nothing, for any reason (budget, cooldown, clamp).
    pub sweeps_nosend: u64,
    /// The stuck ones: nothing sent AND the clamp excluded a gap of the batch
    /// (the sweep re-targeted a seq the server cannot reach).
    pub inert_sweeps: u64,
    /// Clamp exclusions in the batch being served (reset per batch).
    pub batch_excl: u64,
    pub oldest_unchanged: u64,
    pub oldest_unchanged_max: u64,
    last_sweep_oldest: Option<u64>,
}

/// One reading of the account against the buffer.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Reading {
    pub sent: u64,
    pub a: u64,
    pub h: u64,
    pub e: u64,
    pub o_serv: u64,
    pub o_strand: u64,
}

impl Reading {
    pub fn closes(&self) -> bool {
        self.sent == self.a + self.h + self.e + self.o_serv + self.o_strand
    }
}

impl Account {
    pub fn new(reliable: bool) -> Self {
        Self { reliable, ..Self::default() }
    }

    pub fn h(&self) -> u64 {
        self.shed_slot + self.shed_nack
    }

    /// A source seq entered the buffer.
    pub fn on_sent(&mut self, buf: &Buf) {
        self.sent += 1;
        if buf.len() as u64 > self.o_hwm {
            self.o_hwm = buf.len() as u64;
            self.cap_at_hwm = self.store_cap;
        }
    }

    /// The coding window moved (or is about to): raise the evicted floor.
    pub fn note_window(&mut self, win_start: u64, bud: f64) {
        self.evict_floor = self.evict_floor.max(clamp_lo(self.reliable, win_start));
        self.bud_max = self.bud_max.max(bud);
    }

    /// Around the cumulative prune of the buffer: call `before`, prune, then
    /// `after` with the length before.
    pub fn prune_before(&self, buf: &Buf, ack: u64) -> (usize, usize) {
        let ev = buf.count_below(self.evict_floor.min(ack + 1));
        (buf.len(), ev)
    }

    pub fn prune_after(&mut self, buf: &Buf, before: (usize, usize)) {
        let removed = (before.0 - buf.len()) as u64;
        let ev = before.1 as u64;
        self.evicted += ev;
        self.acked += removed - ev;
    }

    /// A seq was shed from the buffer.
    pub fn on_shed(&mut self, slot: bool) {
        if slot {
            self.shed_slot += 1;
        } else {
            self.shed_nack += 1;
        }
    }

    /// A tail sweep fired with `oldest` the first un-acked seq.
    pub fn on_sweep(&mut self, oldest: u64) {
        self.sweeps += 1;
        if self.last_sweep_oldest == Some(oldest) {
            self.oldest_unchanged += 1;
        } else {
            self.oldest_unchanged = 0;
        }
        self.oldest_unchanged_max = self.oldest_unchanged_max.max(self.oldest_unchanged);
        self.last_sweep_oldest = Some(oldest);
    }

    /// A sweep batch was served; `sends` is what it produced. Inert = it sent
    /// nothing and the clamp excluded a gap of this batch.
    pub fn on_sweep_served(&mut self, sends: u64) {
        if sends == 0 {
            self.sweeps_nosend += 1;
            if self.batch_excl > 0 {
                self.inert_sweeps += 1;
            }
        }
    }

    /// A sweep batch was dropped before the gap loop (no repair budget).
    pub fn on_sweep_dropped(&mut self) {
        self.sweeps_nosend += 1;
    }

    pub fn read(&self, buf: &Buf, win_start: u64) -> Reading {
        let strand = buf.count_below(clamp_lo(self.reliable, win_start)) as u64;
        Reading {
            sent: self.sent,
            a: self.acked,
            h: self.h(),
            e: self.evicted,
            o_serv: buf.len() as u64 - strand,
            o_strand: strand,
        }
    }

    /// The `[ACCT]` line. Parsed by `tests/common/loopback.rs`: keep the
    /// `key=value` shape.
    pub fn line(&self, buf: &Buf, win_start: u64, fin: bool) -> String {
        let r = self.read(buf, win_start);
        format!(
            "[ACCT] reliable={} S_sent={} A={} H={} E={} O_serv={} O_strand={} O_max={} closes={} \
             inert_sweeps={} sweeps_nosend={} sweeps={} oldest_unchanged={} oldest_unchanged_max={} \
             O_hwm={} cap_at_hwm={} store_cap={} bud={:.6} shed_slot={} shed_nack={} \
             shed_visit_slot={} shed_visit_nack={} clamp_evals={} clamp_excl={}{}",
            u8::from(self.reliable),
            r.sent,
            r.a,
            r.h,
            r.e,
            r.o_serv,
            r.o_strand,
            buf.last_key().unwrap_or(0),
            u8::from(r.closes()),
            self.inert_sweeps,
            self.sweeps_nosend,
            self.sweeps,
            self.oldest_unchanged,
            self.oldest_unchanged_max,
            self.o_hwm,
            self.cap_at_hwm,
            self.store_cap,
            self.bud_max,
            self.shed_slot,
            self.shed_nack,
            self.shed_visit_slot,
            self.shed_visit_nack,
            self.clamp_evals,
            self.clamp_excl,
            if fin { " final=1" } else { "" },
        )
    }
}
