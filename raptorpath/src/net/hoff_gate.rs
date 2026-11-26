//! The sender hand-off gate (paper §6.8, revision 4), behind the default-off
//! experiment arm `RWM_HOFF_GATE`. Not a dial: nothing here reads a hint, δ or
//! ρ, and the arm is not a mode of the machine (deny-list test in
//! `hoff_gate_tests`).
//!
//! ```text
//!   a first send is handed to path i  ⇔  B_i < β_i
//!   β_i   = R_i · L_i / s_i            R_i = 1.25·cwnd_i/srtt_i  (RouteProbe atomics)
//!   L_i   = τ̂_o + ℓ̂_o + τ̂_p + ℓ̂_p      timer terms floored at 1 ms
//!   spare_i = 0 if path i refused a first send in the last completed turn, else +∞
//!   spare   = max over active paths of spare_i
//! ```
//!
//! Turn = one producer loop iteration. A first send is *waiting* when the TUN
//! intake has a queued packet. Control, resends and repair are exempt from the
//! bound and count in `B_i` (the producer's staged count, counted at `stage`,
//! minus quinn's `tx`, minus what the owner dropped before quinn).
//!
//! Decisions here are pure functions of their inputs (`PathIn` readings, the
//! realized timer samples); the engine glue at the bottom reads the views.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use super::hoff::{hist_p99_s, loop_latency, PathIn};
use crate::transport::io_owner::{PathView, WORK_BUCKETS};

/// "The last second": the window of the realized-wait / work p99s.
const WINDOW_US: u64 = 1_000_000;
/// The p99 terms are recomputed at most this often (cached between).
const REFRESH_US: u64 = 25_000;
/// Samples kept per rolling window.
const MAX_SAMPLES: usize = 1 << 14;

/// `β = R·L/s` for one path's live readings and latency `l_s`. `None` while a
/// factor is unknown (no window, no RTT or no datagram size yet): an unknown
/// bound admits (cold start), it never closes.
pub(crate) fn beta_of(i: &PathIn, l_s: f64) -> Option<f64> {
    let r = i.rate();
    if r <= 0.0 {
        return None;
    }
    super::hoff::beta(r, l_s, i.dgram_size())
}

/// The admission test: `B < β`. Never closes at `B = 0` and never on an
/// undefined β.
pub(crate) fn admits(b: u64, beta: Option<f64>) -> bool {
    b == 0 || beta.is_none_or(|be| (b as f64) < be)
}

/// `r_eff = min(r_law, spare)` at the repair seams.
pub(crate) fn r_eff(r_law: f64, spare: f64) -> f64 {
    r_law.min(spare.max(0.0))
}

/// The owner's republish rule: while the datagrams it handed to quinn exceed
/// what quinn had transmitted at its last publish, republish every
/// `VIEW_REFRESH` (an armed owner only).
pub(crate) fn owner_republishes(armed: bool, handed: u64, dropped: u64, tx_at_publish: u64) -> bool {
    armed && handed.saturating_sub(dropped).saturating_sub(tx_at_publish) > 0
}

/// Datagrams quinn discarded without a tx, found when its queue reads empty.
/// `space` is `datagram_send_buffer_space()` read BEFORE `tx`, with no hand
/// between; `space == capacity` means nothing is queued, so every datagram
/// handed and neither transmitted nor already counted was discarded (drop-oldest
/// on a full buffer, oversize after an MTU shrink). Zero while the queue holds data.
pub(crate) fn quinn_discards_found(space: usize, capacity: usize, handed: u64, tx: u64, counted: u64) -> u64 {
    if space >= capacity {
        handed.saturating_sub(tx).saturating_sub(counted)
    } else {
        0
    }
}

/// Whether the producer's 1 ms backpressure poll is armed: the store's pause
/// (`reliable &&` today) or the gate's own pause, at every ρ.
pub(crate) fn poll_armed(tx_paused: bool, gate_paused: bool) -> bool {
    tx_paused || gate_paused
}

/// The latest p99 over the last second of samples, held through silence.
#[derive(Default)]
struct Roll {
    s: VecDeque<(u64, u32)>,
    held: Option<f64>,
}

impl Roll {
    fn push(&mut self, now_us: u64, v_us: u64) {
        if self.s.len() >= MAX_SAMPLES {
            self.s.pop_front();
        }
        self.s.push_back((now_us, v_us.min(u32::MAX as u64) as u32));
    }

    /// p99 (seconds), nearest rank. An empty window holds the last estimate
    /// (silence leaves timer latency unchanged); `None` before any sample.
    fn p99_s(&mut self, now_us: u64) -> Option<f64> {
        while self.s.front().is_some_and(|(t, _)| t + WINDOW_US < now_us) {
            self.s.pop_front();
        }
        if !self.s.is_empty() {
            let mut v: Vec<u32> = self.s.iter().map(|(_, x)| *x).collect();
            v.sort_unstable();
            let i = ((v.len() as f64 * 0.99).ceil() as usize).clamp(1, v.len()) - 1;
            self.held = Some(v[i] as f64 / 1e6);
        }
        self.held
    }
}

/// The same for a cumulative log-spaced histogram (the owner's counters).
#[derive(Default)]
struct HistRoll {
    snaps: VecDeque<(u64, Vec<u64>)>,
    held: Option<f64>,
}

impl HistRoll {
    fn update(&mut self, now_us: u64, cur: &[u64]) -> Option<f64> {
        while self.snaps.len() > 1 && self.snaps[1].0 + WINDOW_US <= now_us {
            self.snaps.pop_front();
        }
        let zeros;
        let base: &[u64] = match self.snaps.front() {
            Some((_, b)) => b,
            None => {
                zeros = vec![0u64; cur.len()];
                &zeros
            }
        };
        let total: u64 = cur.iter().zip(base).map(|(a, b)| a.saturating_sub(*b)).sum();
        if total > 0 {
            self.held = Some(hist_p99_s(cur, base));
        }
        if self.snaps.back().is_none_or(|(t, _)| t + REFRESH_US <= now_us) {
            self.snaps.push_back((now_us, cur.to_vec()));
        }
        self.held
    }
}

/// The four latency terms of one path, seconds. `None` = not measured yet
/// (the floor and cold-start value, `VIEW_REFRESH` / `poll_1ms`).
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub(crate) struct Terms {
    pub tau_o: Option<f64>,
    pub l_o: f64,
    pub tau_p: Option<f64>,
    pub l_p: f64,
}

impl Terms {
    /// `L = τ̂_o + ℓ̂_o + τ̂_p + ℓ̂_p`, seconds (timer terms floored at 1 ms).
    pub(crate) fn l_s(&self) -> f64 {
        loop_latency(self.tau_o.unwrap_or(0.0), self.l_o, self.tau_p.unwrap_or(0.0), self.l_p).0
    }
}

#[derive(Default)]
struct PathGate {
    tau_o: HistRoll,
    l_o: HistRoll,
    owner: (Option<f64>, f64),
    /// This turn: path i refused a first send (closed with one waiting).
    refused: bool,
    /// The latched predicate: spare_i = 0.
    spare_zero: bool,
    // Window counters for the `[HOFF]` row.
    turns: u64,
    closed_turns: u64,
    sends: u64,
    cut_sends: u64,
    last_beta: Option<f64>,
}

/// One path's gauge row (the gate's real state).
#[derive(Clone, Copy, Debug)]
pub(crate) struct GateRow {
    pub beta: Option<f64>,
    pub terms: Terms,
    pub hand_bind: f64,
    pub spare_zero: bool,
    pub spare_bind: f64,
}

#[derive(Default)]
pub(crate) struct HandoffGate {
    on: bool,
    paths: HashMap<u32, PathGate>,
    /// Active paths of the last turn (the scheduler-wide spare's domain).
    active: Vec<u32>,
    tau_p: Roll,
    l_p: Roll,
    shared: (Option<f64>, f64),
    last_refresh_us: u64,
    refreshed: bool,
    /// The turn in progress ended because the store is full.
    store_ended: bool,
    turn_open: bool,
    views: Vec<(u32, Arc<PathView>)>,
    buf: Vec<(u32, PathIn)>,
    /// Test seam: terms fixed by the caller (no rolling measurement).
    fixed: Option<Terms>,
}

impl HandoffGate {
    pub(crate) fn new(on: bool) -> Self {
        Self { on, ..Self::default() }
    }

    pub(crate) fn on(&self) -> bool {
        self.on
    }

    #[cfg(test)]
    pub(crate) fn fix_terms(&mut self, t: Terms) {
        self.fixed = Some(t);
    }

    // ── measurement inputs ────────────────────────────────────────────────

    /// One realized wait of the producer's 1 ms backpressure poll, µs.
    pub(crate) fn paused_wait(&mut self, now_us: u64, wait_us: u64) {
        self.tau_p.push(now_us, wait_us);
    }

    /// One producer loop iteration's body time, µs.
    pub(crate) fn iter_work(&mut self, now_us: u64, work_us: u64) {
        self.l_p.push(now_us, work_us);
    }

    /// The owner's work and republish-wait histograms for path `id`.
    pub(crate) fn owner_hists(&mut self, id: u32, now_us: u64, work: &[u64], timer: &[u64]) {
        let p = self.paths.entry(id).or_default();
        p.owner = (p.tau_o.update(now_us, timer), p.l_o.update(now_us, work).unwrap_or(0.0));
    }

    fn refresh(&mut self, now_us: u64) {
        if self.refreshed && now_us.saturating_sub(self.last_refresh_us) < REFRESH_US {
            return;
        }
        self.refreshed = true;
        self.last_refresh_us = now_us;
        self.shared = (self.tau_p.p99_s(now_us), self.l_p.p99_s(now_us).unwrap_or(0.0));
    }

    /// The terms path `id` currently reads.
    pub(crate) fn terms(&self, id: u32) -> Terms {
        if let Some(t) = self.fixed {
            return t;
        }
        let o = self.paths.get(&id).map_or((None, 0.0), |p| p.owner);
        Terms { tau_o: o.0, l_o: o.1, tau_p: self.shared.0, l_p: self.shared.1 }
    }

    /// β_i for the live reading `i`.
    pub(crate) fn beta(&self, id: u32, i: &PathIn) -> Option<f64> {
        beta_of(i, self.terms(id).l_s())
    }

    /// May a first send be handed to path `id` now?
    pub(crate) fn admit(&self, id: u32, i: &PathIn) -> bool {
        admits(i.backlog(), self.beta(id, i))
    }

    // ── the turn ──────────────────────────────────────────────────────────

    /// The loop top. Latches the last turn's predicate (a store-full turn
    /// leaves it unchanged), opens the new turn, and returns whether the gate
    /// pauses the source (no active path has room). `waiting` = the intake has
    /// a queued first send; `store_paused` = the store's pause holds this turn.
    pub(crate) fn begin_turn(&mut self, now_us: u64, waiting: bool, store_paused: bool, ins: &[(u32, PathIn)]) -> bool {
        self.refresh(now_us);
        if self.turn_open && !self.store_ended {
            for p in self.paths.values_mut() {
                p.spare_zero = p.refused;
            }
        }
        for p in self.paths.values_mut() {
            p.refused = false;
        }
        self.turn_open = true;
        self.store_ended = store_paused;
        self.active.clear();
        self.active.extend(ins.iter().map(|(id, _)| *id));
        let mut any_open = ins.is_empty();
        for (id, i) in ins {
            let open = self.admit(*id, i);
            let beta = self.beta(*id, i);
            let p = self.paths.entry(*id).or_default();
            p.turns += 1;
            p.closed_turns += (!open) as u64;
            p.last_beta = beta;
            if !open && waiting && !store_paused {
                p.refused = true;
            }
            any_open |= open;
        }
        !store_paused && !any_open
    }

    /// Mid-turn, before a first send is taken from the intake: is any path
    /// open? A closed path with a first send waiting refuses it.
    pub(crate) fn check(&mut self, waiting: bool, ins: &[(u32, PathIn)]) -> bool {
        let mut any_open = ins.is_empty();
        for (id, i) in ins {
            let open = self.admit(*id, i);
            if !open && waiting {
                self.paths.entry(*id).or_default().refused = true;
            }
            any_open |= open;
        }
        any_open
    }

    /// Paths with room now (placement's candidate set for a first send). Empty
    /// only when every path is closed.
    pub(crate) fn open_ids(&self, ins: &[(u32, PathIn)]) -> Vec<u32> {
        ins.iter().filter(|(id, i)| self.admit(*id, i)).map(|(id, _)| *id).collect()
    }

    /// The turn ended because the store is full: it does not update the predicate.
    pub(crate) fn note_store_end(&mut self) {
        self.store_ended = true;
    }

    /// A first send was placed on `path`.
    pub(crate) fn note_first_send(&mut self, path: u32) {
        let cut = self.spare_all() <= 0.0;
        let p = self.paths.entry(path).or_default();
        p.sends += 1;
        p.cut_sends += cut as u64;
    }

    // ── the predicate ─────────────────────────────────────────────────────

    /// spare_i: 0 if the last completed turn refused a first send on `id`,
    /// +∞ otherwise (and at cold start).
    pub(crate) fn spare_of(&self, id: u32) -> f64 {
        if self.paths.get(&id).is_some_and(|p| p.spare_zero) { 0.0 } else { f64::INFINITY }
    }

    /// The scheduler-wide spare: the max over the active paths.
    pub(crate) fn spare_all(&self) -> f64 {
        if self.active.is_empty() {
            return f64::INFINITY;
        }
        self.active.iter().map(|id| self.spare_of(*id)).fold(0.0, f64::max)
    }

    // ── gauges ────────────────────────────────────────────────────────────

    /// The row of path `id`, and its window counters reset.
    pub(crate) fn take_row(&mut self, id: u32) -> Option<GateRow> {
        let terms = self.terms(id);
        let spare_zero = self.spare_of(id) == 0.0;
        let p = self.paths.get_mut(&id)?;
        let row = GateRow {
            beta: p.last_beta,
            terms,
            hand_bind: p.closed_turns as f64 / p.turns.max(1) as f64,
            spare_zero,
            spare_bind: p.cut_sends as f64 / p.sends.max(1) as f64,
        };
        p.turns = 0;
        p.closed_turns = 0;
        p.sends = 0;
        p.cut_sends = 0;
        Some(row)
    }

    // ── engine glue (views) ───────────────────────────────────────────────

    fn sync_views(&mut self, sched: &crate::scheduler::Scheduler, transport: &Arc<crate::transport::QuicTransport>) -> Vec<u32> {
        let ids = sched.active_ids();
        // A path id can come back on a new connection (a new `PathView`): compare the
        // views themselves, not only the ids, or a reconnected path reads a dead view.
        let fresh: Vec<(u32, Arc<PathView>)> =
            ids.iter().filter_map(|id| transport.view(*id).map(|v| (*id, v))).collect();
        let same = fresh.len() == self.views.len()
            && fresh.iter().zip(&self.views).all(|(a, b)| a.0 == b.0 && Arc::ptr_eq(&a.1, &b.1));
        if !same {
            self.views = fresh;
        }
        ids
    }

    fn read_all(&mut self) -> Vec<(u32, PathIn)> {
        let mut b = std::mem::take(&mut self.buf);
        b.clear();
        b.extend(self.views.iter().map(|(id, v)| (*id, super::hoff::HoffGauge::read(v))));
        b
    }

    /// Feed the owner's histograms (at the refresh cadence).
    fn feed_owner(&mut self, now_us: u64) {
        if self.refreshed && now_us.saturating_sub(self.last_refresh_us) < REFRESH_US {
            return;
        }
        let v: Vec<(u32, [u64; WORK_BUCKETS], [u64; WORK_BUCKETS])> = self
            .views
            .iter()
            .map(|(id, v)| (*id, v.hoff.work.snapshot(), v.hoff.timer_wait.snapshot()))
            .collect();
        for (id, w, t) in v {
            self.owner_hists(id, now_us, &w, &t);
        }
    }

    /// Loop top, live: see [`Self::begin_turn`].
    pub(crate) fn begin_turn_live(
        &mut self,
        now_us: u64,
        waiting: bool,
        store_paused: bool,
        sched: &crate::scheduler::Scheduler,
        transport: &Arc<crate::transport::QuicTransport>,
    ) -> bool {
        self.sync_views(sched, transport);
        self.feed_owner(now_us);
        let ins = self.read_all();
        let paused = self.begin_turn(now_us, waiting, store_paused, &ins);
        self.buf = ins;
        paused
    }

    /// Before a first send is taken, live: see [`Self::check`].
    pub(crate) fn check_live(&mut self, waiting: bool) -> bool {
        let ins = self.read_all();
        let open = self.check(waiting, &ins);
        self.buf = ins;
        open
    }

    /// Placement's candidate set for a first send, live.
    pub(crate) fn open_live(&mut self) -> Vec<u32> {
        let ins = self.read_all();
        let open = self.open_ids(&ins);
        self.buf = ins;
        open
    }

    /// The views' cumulative owner-timer wake count (the gauge).
    pub(crate) fn owner_timer_wakes(&self) -> u64 {
        use std::sync::atomic::Ordering::Relaxed;
        self.views.iter().map(|(_, v)| v.hoff.timer_wakes.load(Relaxed)).sum()
    }
}
