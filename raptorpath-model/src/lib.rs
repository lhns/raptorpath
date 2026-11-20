//! A hand-written stateright model of the hole lifecycle (test-detection
//! plan S7, ADR-0083). See `docs/derivations/recovery-model.md`.
//!
//! The model follows each seq's fate over a lossy link, copying the order
//! the shipped engine takes its decisions in (file:line cited at each step,
//! against github/main 54198a2a). The model is checked; its match to the
//! engine is ASSUMED (nothing here calls engine code).
//!
//! Abstractions (the doc lists them with their reasons): FEC is off (no
//! decode fills), one source per loop iteration, one FIFO path, the ack path
//! is lossless and coalesced (the receiver's latest snapshot), the per-seq
//! cooldown and the ripeness gates are treated as ripe whenever a chance is
//! taken, a timer fires only once the previous copy of its target has
//! resolved, reports and sweeps judge past the shed deadline.

use stateright::{Model, Property};

/// Bitmask capacity: object + next object must fit.
pub const MAX_SEQS: usize = 16;

/// The retention seat, as a value. The engine derives its `reliable` bool
/// from ρ = 1 and its shed budget 1 − ρ (`shed_budget_frac`,
/// net/emit_source.rs:635); the model carries both numbers and branches only
/// where the engine branches on `reliable` (each site cited).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Seat {
    /// ρ = 1 (`pol.reliable`).
    pub reliable: bool,
    /// 1 − ρ as num/den: the shed budget fraction.
    pub shed_budget_num: u32,
    pub shed_budget_den: u32,
}

impl Seat {
    /// ρ = 1: retain until acked; budget 1 − ρ = 0.
    pub const RELIABLE: Seat = Seat { reliable: true, shed_budget_num: 0, shed_budget_den: 1 };
    /// ρ = 3/4: the evicting (realtime) seat; budget 1 − ρ = 1/4.
    pub const EVICTING: Seat = Seat { reliable: false, shed_budget_num: 1, shed_budget_den: 4 };

    /// `shed_armed(unified, reliable, gate)` = `unified && !reliable && gate`
    /// (net/shed.rs:23, read at net/sender_policy.rs:667); unified and the
    /// gate are taken as on.
    fn shed_on(&self) -> bool {
        !self.reliable
    }
}

#[derive(Clone, Debug)]
pub struct HoleModel {
    pub seat: Seat,
    /// Coding window W (`pol.win_cap`, MAX_WINDOW_SIZE = 200 at
    /// net/mod.rs:205; eviction at net/mod.rs:3594).
    pub window: u8,
    /// Source packets in the object.
    pub object: u8,
    /// Packets of a next object, sent if the sender keeps sending.
    pub next_object: u8,
    /// L: drops per seq, over its original send and every resend.
    pub loss_budget: u8,
    /// Timer-driven chances per hole (sweeps of a cumulative blocker, and on
    /// the reliable seat refreshes of the receiver's blocker).
    pub timer_budget: u8,
    /// Reliable intake cap (`effective_store_cap`, net/mod.rs:2473).
    pub store_cap: u8,
    /// Evicting reorder buffer capacity (`max_buffered`, net/reorder.rs:88).
    pub reorder_cap: u8,
}

impl HoleModel {
    /// The S7 bounds: W = 3, 8 source packets (+2 if the sender keeps
    /// sending), L drops per seq, L + 1 timer chances per hole.
    pub fn s7(seat: Seat, loss_budget: u8) -> Self {
        HoleModel {
            seat,
            window: 3,
            object: 8,
            next_object: 2,
            loss_budget,
            timer_budget: loss_budget + 1,
            store_cap: 4,
            reorder_cap: 4,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct State {
    // ---- sender ----
    /// None until the object boundary; then Some(total packets to send).
    pub total: Option<u8>,
    /// Next source seq to emit (= sources sent so far).
    pub next_src: u8,
    /// `retransmit_buffer` keys (un-acked, un-shed).
    pub outstanding: u16,
    /// `shed_seqs`.
    pub shed: u16,
    pub shed_total: u8,
    /// Tail-sweep budget, kept for the current cumulative blocker only
    /// (`first()` never decreases).
    pub sweep_seq: u8,
    pub sweep_count: u8,
    /// Witness: sweeps of `sweep_seq` whose (seq,seq) gap the clamp emptied.
    pub sweep_clamped: u8,
    // ---- witnesses over the whole run ----
    /// Seqs resent (by a report, a sweep or a correction copy), kept only
    /// while the seq is outstanding (see `canonical`).
    pub ever_served: u16,
    /// Some copy was dropped in this run (for the non-vacuity property).
    pub any_drop: bool,
    // ---- link ----
    /// Data copies in flight, a FIFO (one path, no reordering); a copy of a
    /// seq already queued coalesces with it.
    pub link: Vec<u8>,
    pub drops: [u8; MAX_SEQS],
    /// A WindowAck snapshot is waiting for the sender (coalesced, lossless).
    pub report: bool,
    // ---- receiver ----
    /// Seqs that arrived, kept only above the frontier (see `canonical`).
    pub received: u16,
    /// `next_deliver_seq` of the reorder buffer.
    pub frontier: u8,
    /// Reliable hole-refresh budget for the current blocker.
    pub refresh_seq: u8,
    pub refresh_count: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    /// At the object boundary: stop, or keep sending the next object.
    Boundary { keep_sending: bool },
    /// Emit the next source; `copy`: the P_lost correction slot picked
    /// `retransmit_buffer.first()` (net/emit_source.rs:757); `past`: that
    /// seq is already past the shed deadline D.
    Emit { copy: bool, past: bool },
    Drop(u8),
    Deliver(u8),
    /// The sender processes the receiver's latest WindowAck.
    SenderReport,
    /// Tail-sweep timer fires (net/mod.rs:3031).
    TailSweep,
    /// Reliable receiver hole-refresh (net/receiver.rs:860).
    HoleRefresh,
    /// Evicting receiver hold expiry with `k` the largest expired entry
    /// (net/reorder.rs:112).
    HoldExpire(u8),
}

fn bit(s: u8) -> u16 {
    1u16 << s
}
fn has(m: u16, s: u8) -> bool {
    m & bit(s) != 0
}
fn below(n: u8) -> u16 {
    if n as usize >= 16 { u16::MAX } else { (1u16 << n) - 1 }
}
fn lowest(m: u16) -> Option<u8> {
    if m == 0 { None } else { Some(m.trailing_zeros() as u8) }
}
fn highest(m: u16) -> Option<u8> {
    if m == 0 { None } else { Some(15 - m.leading_zeros() as u8) }
}

impl HoleModel {
    fn sent_total(&self, s: &State) -> u8 {
        s.total.unwrap_or(self.object)
    }

    /// Coding window after the loop's eviction to `win_cap`
    /// (net/mod.rs:3594): the last W sent seqs.
    fn window_span(&self, s: &State) -> Option<(u8, u8)> {
        if s.next_src == 0 {
            return None;
        }
        Some((s.next_src.saturating_sub(self.window), s.next_src - 1))
    }

    /// Receiver entries held above the frontier (`pending`).
    fn pending(&self, s: &State) -> u16 {
        s.received & !below(s.frontier) & below(self.sent_total(s).max(s.next_src))
    }

    /// `shed_allowed` (net/shed.rs:39): past deadline AND
    /// (shed_total + 1) <= (1 − ρ) × src_total.
    ///
    /// Past deadline: D = min(b·RTprop, 2·RTprop) with b(Realtime) = ½
    /// (net/shed.rs:31), measured from the original send. A receiver gap
    /// report about a seq needs a later arrival and an ack flight, so it is
    /// judged at least one RTprop after the send, and a tail sweep fires at
    /// last activity + 2×SRTT: both are past D, so `past` is true there.
    /// Only the correction slot at emission can see a younger seq; it takes
    /// `past` as a choice.
    fn shed_allowed(&self, s: &State, past: bool) -> bool {
        let src_total = s.next_src as u32;
        past
            && (s.shed_total as u32 + 1) * self.seat.shed_budget_den
                <= self.seat.shed_budget_num * src_total
    }

    fn do_shed(&self, s: &mut State, seq: u8) {
        // net/sender_phases.rs:860-863 (and net/emit_source.rs:782).
        s.outstanding &= !bit(seq);
        s.shed |= bit(seq);
        s.shed_total += 1;
    }

    fn send(&self, s: &mut State, seq: u8) {
        if !s.link.contains(&seq) {
            s.link.push(seq);
        }
    }

    fn in_flight(s: &State) -> u16 {
        s.link.iter().fold(0, |m, &q| m | bit(q))
    }

    fn resend(&self, s: &mut State, seq: u8) {
        self.send(s, seq);
        s.ever_served |= bit(seq);
    }

    /// One gap through the shipped order (net/sender_phases.rs:791-...):
    /// clamp to the coding window on the evicting seat (:795-799), then for
    /// each seq the shed check (:847-869), then serve. Returns true when the
    /// clamp emptied the gap (:800).
    fn judge_gap(&self, s: &mut State, gap_start: u8, gap_end: u8) -> bool {
        let Some((win_start, win_end)) = self.window_span(s) else { return true };
        let (cs, ce) = if self.seat.reliable {
            (gap_start, gap_end)
        } else {
            (gap_start.max(win_start), gap_end.min(win_end))
        };
        if cs > ce {
            return true;
        }
        for seq in cs..=ce {
            if self.seat.shed_on() {
                if has(s.shed, seq) {
                    continue; // :848
                }
                if has(s.outstanding, seq) && self.shed_allowed(s, true) {
                    self.do_shed(s, seq); // :853-864
                    continue;
                }
            }
            // Cooldown (:871) and the ripeness gates (:890-960) are taken as
            // ripe. Serve: reliable from the sent store, evicting from the
            // window (:984); after the clamp both hold the seq. A stale gap
            // for an acked seq has nothing to serve.
            if has(s.outstanding, seq) {
                self.resend(s, seq);
            }
        }
        false
    }

    /// State canonicalization (sound: drops only history no later step
    /// reads). A seq that is neither outstanding nor in flight is never sent
    /// again (every resend path picks an outstanding seq), so its drop count
    /// and resend witness are dead; arrivals below the frontier are read by
    /// nothing (a late fill below the frontier is delivered at once,
    /// net/reorder.rs:75).
    fn canonical(&self, s: &mut State) {
        let live = s.outstanding | Self::in_flight(s);
        for q in 0..MAX_SEQS as u8 {
            if !has(live, q) {
                s.drops[q as usize] = 0;
            }
        }
        s.ever_served &= s.outstanding;
        s.received &= !below(s.frontier);
    }

    /// `drain_contiguous` (net/reorder.rs:97).
    fn drain_contiguous(&self, s: &mut State) {
        while has(s.received, s.frontier) && (s.frontier as usize) < MAX_SEQS {
            s.frontier += 1;
        }
    }

    fn enabled(&self, s: &State, out: &mut Vec<Action>) {
        // Sender emission. The object boundary is a choice.
        if s.total.is_none() && s.next_src == self.object {
            out.push(Action::Boundary { keep_sending: false });
            if self.next_object > 0 {
                out.push(Action::Boundary { keep_sending: true });
            }
        }
        let can_emit = match s.total {
            None => s.next_src < self.object,
            Some(t) => s.next_src < t,
        };
        // Intake pause, reliable seat only (net/mod.rs:2473):
        // `tx_paused = reliable && store_len >= effective_store_cap`.
        let tx_paused =
            self.seat.reliable && s.outstanding.count_ones() >= self.store_cap as u32;
        if can_emit && !tx_paused {
            out.push(Action::Emit { copy: false, past: false });
            out.push(Action::Emit { copy: true, past: false });
            out.push(Action::Emit { copy: true, past: true });
        }
        // The link: the head copy is lost (within its budget) or arrives.
        if let Some(&q) = s.link.first() {
            if s.drops[q as usize] < self.loss_budget {
                out.push(Action::Drop(q));
            }
            out.push(Action::Deliver(q));
        }
        let in_flight = Self::in_flight(s);
        if s.report {
            out.push(Action::SenderReport);
        }
        // Tail sweep: armed while `retransmit_buffer.first()` exists
        // (net/mod.rs:2789); its deadline is last activity + 2×SRTT, so it
        // fires only once the blocker's last copy has resolved.
        if let Some(first) = lowest(s.outstanding) {
            let used = if s.sweep_seq == first { s.sweep_count } else { 0 };
            if !has(in_flight, first) && used < self.timer_budget {
                out.push(Action::TailSweep);
            }
        }
        // Receiver timer: armed only with an entry held above the hole
        // (`waiting && pending`, net/receiver.rs:725).
        let pending = self.pending(s);
        if pending != 0 {
            if self.seat.reliable {
                // net/receiver.rs:860: the reliable seat re-advertises.
                let used = if s.refresh_seq == s.frontier { s.refresh_count } else { 0 };
                if !has(in_flight, s.frontier) && used < self.timer_budget {
                    out.push(Action::HoleRefresh);
                }
            } else {
                // net/receiver.rs:973: the evicting seat gives up on the
                // hole. The largest expired entry depends on arrival times;
                // the model takes the lowest pending one (the first to
                // expire under in-order sending): the fewest seqs skipped.
                out.push(Action::HoldExpire(lowest(pending).unwrap()));
            }
        }
    }

    /// No action is enabled: the run has ended.
    pub fn is_terminal(&self, s: &State) -> bool {
        let mut v = Vec::new();
        self.enabled(s, &mut v);
        v.is_empty()
    }

    fn step(&self, last: &State, a: Action) -> State {
        let mut s = last.clone();
        match a {
            Action::Boundary { keep_sending } => {
                s.total = Some(self.object + if keep_sending { self.next_object } else { 0 });
            }
            Action::Emit { copy, past } => {
                let seq = s.next_src;
                s.next_src += 1;
                self.send(&mut s, seq);
                // net/emit_source.rs:505: into the retransmit buffer.
                s.outstanding |= bit(seq);
                if copy {
                    // The correction slot (net/emit_source.rs:740-826), run
                    // before this iteration's eviction: the window still
                    // holds W + 1 seqs. The seq just emitted has age 0, so
                    // it is never past the deadline.
                    if let Some(oldest) = lowest(s.outstanding) {
                        if self.seat.shed_on() && self.shed_allowed(&s, past && oldest < seq) {
                            self.do_shed(&mut s, oldest); // :774-786
                        } else {
                            // :821-826: sent store (reliable, :346) or the
                            // encoder window; else a coded repair (FEC is
                            // abstracted away: no effect).
                            let ws = s.next_src.saturating_sub(self.window + 1);
                            if self.seat.reliable || oldest >= ws {
                                self.resend(&mut s, oldest);
                            }
                        }
                    }
                }
            }
            Action::Drop(q) => {
                s.link.remove(0);
                s.drops[q as usize] += 1;
                s.any_drop = true;
            }
            Action::Deliver(q) => {
                s.link.remove(0);
                s.received |= bit(q);
                // ReorderBuffer::push (net/reorder.rs:68-94). Below the
                // frontier: a late fill, delivered at once (:75).
                if q >= s.frontier {
                    if !self.seat.reliable
                        && self.pending(&s).count_ones() > self.reorder_cap as u32
                    {
                        // Capacity force-drain, evicting only (:88, :149):
                        // drain the oldest until <= max_buffered / 2.
                        while self.pending(&s).count_ones() > (self.reorder_cap / 2) as u32 {
                            let p = lowest(self.pending(&s)).unwrap();
                            s.frontier = s.frontier.max(p + 1);
                        }
                    }
                    self.drain_contiguous(&mut s);
                }
                // The arrival's WindowAck (net/receiver.rs:2007).
                s.report = true;
            }
            Action::SenderReport => {
                s.report = false;
                // Cumulative ack = next_expected - 1: prune the retransmit
                // buffer and the shed set (net/sender_phases.rs:1403, :1407).
                s.outstanding &= !below(s.frontier);
                s.shed &= !below(s.frontier);
                // The SACK gaps: missing seqs between the frontier and the
                // highest arrival, as contiguous runs.
                if let Some(top) = highest(s.received) {
                    let mut q = s.frontier;
                    while q <= top {
                        if has(s.received, q) {
                            q += 1;
                            continue;
                        }
                        let start = q;
                        while q <= top && !has(s.received, q) {
                            q += 1;
                        }
                        self.judge_gap(&mut s, start, q - 1);
                    }
                }
            }
            Action::TailSweep => {
                // net/mod.rs:3035-3042: a (first, first) gap through the
                // same judgement as a receiver report.
                let first = lowest(s.outstanding).unwrap();
                if s.sweep_seq != first {
                    s.sweep_seq = first;
                    s.sweep_count = 0;
                    s.sweep_clamped = 0;
                }
                s.sweep_count += 1;
                if self.judge_gap(&mut s, first, first) {
                    s.sweep_clamped += 1;
                }
            }
            Action::HoleRefresh => {
                if s.refresh_seq != s.frontier {
                    s.refresh_seq = s.frontier;
                    s.refresh_count = 0;
                }
                s.refresh_count += 1;
                s.report = true; // the SACK-bearing WindowAck, :933
            }
            Action::HoldExpire(k) => {
                // drain_expired (net/reorder.rs:112-145): give up through k.
                s.frontier = s.frontier.max(k + 1);
                self.drain_contiguous(&mut s);
                // The hold-expiry unwedge ack (net/receiver.rs:1031).
                s.report = true;
            }
        }
        self.canonical(&mut s);
        s
    }

    /// Per-seq fate, for printing a trace's final state.
    pub fn fates(&self, s: &State) -> String {
        let total = self.sent_total(s).max(s.next_src);
        let mut out = String::new();
        for q in 0..total {
            let rx = if q < s.frontier {
                "passed"
            } else if has(s.received, q) {
                "held above the frontier"
            } else {
                "HOLE"
            };
            let tx = if q >= s.next_src {
                "unsent"
            } else if has(s.outstanding, q) {
                "OUTSTANDING"
            } else if has(s.shed, q) {
                "shed"
            } else {
                "released"
            };
            out.push_str(&format!(
                "  seq {q}: receiver {rx}, sender {tx}, drops {}, resent {}\n",
                s.drops[q as usize],
                has(s.ever_served, q)
            ));
        }
        out.push_str(&format!(
            "  window {:?}, receiver frontier {}, pending {:#b}, sweeps of blocker {} = {} ({} clamped out)\n",
            self.window_span(s),
            s.frontier,
            self.pending(s),
            s.sweep_seq,
            s.sweep_count,
            s.sweep_clamped
        ));
        out
    }

    /// The window span at a state (for the tests' trace-shape checks).
    pub fn window_at(&self, s: &State) -> Option<(u8, u8)> {
        self.window_span(s)
    }
    pub fn pending_at(&self, s: &State) -> u16 {
        self.pending(s)
    }
    pub fn first_outstanding(s: &State) -> Option<u8> {
        lowest(s.outstanding)
    }
}

pub const P_SENDER_CLEAN: &str = "terminal: sender holds no outstanding seq";
pub const P_RECEIVER_PASSED: &str = "terminal: receiver frontier passed every seq";
/// The sender-clean property restricted to runs where the receiver
/// frontier moved: its counterexample is a lost TAIL behind a delivered
/// prefix (the wedge's shape), not a whole-object loss.
pub const P_TAIL_CLEAN: &str = "terminal, frontier moved: sender holds no outstanding seq";
/// The receiver-frontier property restricted to runs where the sender is
/// clean: its counterexample is a hole the sender released (shed) that the
/// receiver never passes.
pub const P_RELEASED_PASSED: &str = "terminal, sender clean: receiver frontier passed every seq";
pub const P_STORE_CAP: &str = "always: reliable outstanding <= store cap";
pub const P_COMPLETES: &str = "sometimes: a run completes cleanly";

impl Model for HoleModel {
    type State = State;
    type Action = Action;

    fn init_states(&self) -> Vec<State> {
        assert!((self.object + self.next_object) as usize <= MAX_SEQS);
        vec![State {
            total: None,
            next_src: 0,
            outstanding: 0,
            shed: 0,
            shed_total: 0,
            sweep_seq: 0,
            sweep_count: 0,
            sweep_clamped: 0,
            ever_served: 0,
            any_drop: false,
            link: Vec::new(),
            drops: [0; MAX_SEQS],
            report: false,
            received: 0,
            frontier: 0,
            refresh_seq: 0,
            refresh_count: 0,
        }]
    }

    fn actions(&self, s: &State, actions: &mut Vec<Action>) {
        self.enabled(s, actions);
    }

    fn next_state(&self, last: &State, a: Action) -> Option<State> {
        Some(self.step(last, a))
    }

    fn properties(&self) -> Vec<Property<Self>> {
        vec![
            Property::always(P_SENDER_CLEAN, |m: &HoleModel, s: &State| {
                !m.is_terminal(s) || (s.outstanding == 0 && Some(s.next_src) == s.total)
            }),
            Property::always(P_RECEIVER_PASSED, |m: &HoleModel, s: &State| {
                !m.is_terminal(s) || Some(s.frontier) == s.total
            }),
            Property::always(P_TAIL_CLEAN, |m: &HoleModel, s: &State| {
                !m.is_terminal(s) || s.frontier == 0 || s.outstanding == 0
            }),
            Property::always(P_RELEASED_PASSED, |m: &HoleModel, s: &State| {
                !m.is_terminal(s) || s.outstanding != 0 || Some(s.frontier) == s.total
            }),
            Property::always(P_STORE_CAP, |m: &HoleModel, s: &State| {
                !m.seat.reliable || s.outstanding.count_ones() <= m.store_cap as u32
            }),
            Property::sometimes(P_COMPLETES, |m: &HoleModel, s: &State| {
                m.is_terminal(s)
                    && s.outstanding == 0
                    && Some(s.frontier) == s.total
                    && s.any_drop
            }),
        ]
    }
}
