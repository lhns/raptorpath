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

/// The grant a decision gives one path.
fn grant_of(open: bool, beta: Option<f64>) -> Grant {
    match (open, beta) {
        (false, _) => Grant::Closed,
        (true, None) => Grant::Cold,
        (true, Some(_)) => Grant::Admit,
    }
}

/// The sites that stage a first send without asking the gate (witness tags;
/// no decision reads them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Bypass {
    /// The intake packet deferred behind the resend phase: read at the loop
    /// top, staged after the gate may have closed.
    Defer = 0,
    /// The packer's timer flush.
    Timer = 1,
    /// The shutdown / TUN-closed flush.
    Shutdown = 2,
}

/// What the gate's latest decision said about one path for the next first send.
///
/// The witness is a logic witness, not a second reading: a decision that admits a
/// path (`begin_turn`, `check`, the placement's candidate set `open_ids`) grants it
/// a token; staging a first send spends it. The gate's own read of `B` and `β`
/// decided, so the staging site re-reads nothing and cannot disagree with it.
///
/// A decision is the gate's NEWEST word: each one overwrites the grant of every
/// path it covered. Staging spends the decision for ALL paths (a candidate set pays
/// for one first send, on one path), so a token left on the other admitted path
/// cannot mask a leak there. A path no decision covered (no view yet) has no entry
/// and reads as cold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Grant {
    /// Admitted with `B < β`.
    Admit,
    /// Admitted while β was undefined (cold start).
    Cold,
    /// Closed by the newest decision.
    Closed,
    /// The decision's one first send was already staged; no new decision since.
    Spent,
}

/// How one first send stood against the gate when it was staged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FsClass {
    /// Staged under the gate's own admit.
    Gated,
    /// Staged with no admit and no tagged bypass site: the gate leaked.
    Over,
    /// Staged with no admit through a tagged bypass site.
    Bypass(Bypass),
    /// β undefined at the decision, or no decision covered the path: the cold-start admit (any tag).
    Cold,
}

/// Classify a first send at staging time from the path's grant (`None`: no decision
/// ever covered the path) and the bypass tag in force.
pub(crate) fn classify_first_send(grant: Option<Grant>, bypass: Option<Bypass>) -> FsClass {
    match grant {
        Some(Grant::Admit) => FsClass::Gated,
        None | Some(Grant::Cold) => FsClass::Cold,
        Some(Grant::Closed | Grant::Spent) => bypass.map_or(FsClass::Over, FsClass::Bypass),
    }
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
    /// The newest decision's word on this path (see [`Grant`]); `None` before any.
    grant: Option<Grant>,
    // `[HOFF]` witness counters (see `GateRow`).
    fs_over: u64,
    /// `fs_over` by why: [closed by the newest decision, decision already spent].
    fs_over_cs: [u64; 2],
    // Window gauges (reported only; see `GateRow`).
    beta_max: Option<(f64, u64, u64, f64, f64)>,
    b_max: u64,
    bat_start: Option<(u64, u64)>,
    bat_last: (u64, u64),
    acks: VecDeque<(u64, u64)>,
    ack_turns: u64,
    ack_bound: u64,
    fs_byp: [u64; 4],
    beta_s: Vec<f64>,
    ex_max: Option<(f64, f64, f64)>,
    /// Cumulative staged-by-kind at the window start, the latest read and the turn start.
    kinds_start: Option<[u64; 4]>,
    kinds_last: [u64; 4],
    /// The kinds at the start of the last turn that admitted (B < β) on this path.
    kinds_open: Option<[u64; 4]>,
    kinds_turn: [u64; 4],
    own_gap_max_us: u64,
    pend_max: u64,
}

impl PathGate {
    /// One turn's window gauges: the largest β with its tuple, the largest B, the
    /// batch bookkeeping for the U bound, and Revision 5's cap against today's β·s.
    /// Read-only with respect to every decision.
    fn note_window(&mut self, now_us: u64, i: &PathIn, beta: Option<f64>, l_s: f64) {
        self.b_max = self.b_max.max(i.backlog());
        self.bat_start.get_or_insert((i.tx_batches, i.tx_dgrams));
        self.bat_last = (i.tx_batches, i.tx_dgrams);
        if let Some(be) = beta {
            if self.beta_max.is_none_or(|m| be > m.0) {
                self.beta_max = Some((be, i.window(), i.srtt_us, l_s, i.rate()));
            }
        }
        // ẋ = bytes acked over the last srtt / srtt: the oldest sample at or before the horizon.
        self.acks.push_back((now_us, i.qacked));
        if self.acks.len() > MAX_SAMPLES {
            self.acks.pop_front();
        }
        let srtt = i.srtt_us;
        if srtt == 0 {
            return;
        }
        while self.acks.len() > 1 && self.acks[1].0 + srtt <= now_us {
            self.acks.pop_front();
        }
        let Some(&(t0, a0)) = self.acks.front() else { return };
        let dt = now_us - t0;
        if beta.is_none() || dt * 2 < srtt {
            return; // no ack clock yet (or cold start): no verdict
        }
        let xdot = i.qacked.saturating_sub(a0) as f64 / (dt as f64 / 1e6);
        let room = i.window().saturating_sub(i.qf) as f64;
        let cap = room + 1.25 * xdot * l_s;
        self.ack_turns += 1;
        self.ack_bound += (cap < i.rate() * l_s) as u64;
    }

    fn note_kinds(&mut self, k: [u64; 4]) {
        self.kinds_start.get_or_insert(k);
        self.kinds_last = k;
    }
}

/// One path's gauge row (the gate's real state).
#[derive(Clone, Copy, Debug)]
pub(crate) struct GateRow {
    pub beta: Option<f64>,
    pub terms: Terms,
    pub hand_bind: f64,
    pub spare_zero: bool,
    pub spare_bind: f64,
    /// First sends staged with no admit from the gate's newest decision and through
    /// no tagged bypass site (a logic witness: see [`Grant`]).
    pub fs_over: u64,
    /// `fs_over` split by why there was no admit: the newest decision closed the
    /// path, or it had already paid for an earlier send (no new decision).
    pub fs_over_cs: [u64; 2],
    /// First sends staged with no admit through a tagged site (defer, timer,
    /// shutdown), then those staged under a cold-start admit or on a path no
    /// decision covered (cold).
    pub fs_bypass: [u64; 4],
    /// The window's largest end-of-turn `B − β` as `(total, first sends, exempt)`:
    /// the first sends and exempt datagrams (resends, repair, control) staged
    /// since the path last admitted; the rest of the total is carry-in or drain.
    pub ex_max: Option<(f64, f64, f64)>,
    /// Exempt datagrams staged in the window: control, resend, repair.
    pub ex_n: [u64; 3],
    /// First sends staged in the window.
    pub fs_n: u64,
    /// The window's largest β with the tuple at that turn: (β, W bytes, srtt µs, L s, R bytes/s).
    pub beta_max: Option<(f64, u64, u64, f64, f64)>,
    /// The window's largest backlog B read at a turn start or end.
    pub b_max: u64,
    /// 1 if `b_max` > the window's `beta_max` + one batch (the U witness), else 0.
    pub u_over: u64,
    /// Share of turns (with an ack clock) where Revision 5's cap (W − F)⁺ + 1.25·ẋ·L is below β·s.
    /// Reported only: no decision reads it.
    pub ack_bind: Option<f64>,
    /// β's p5 / p50 / p95 over the window's per-turn values.
    pub beta_q: Option<(f64, f64, f64)>,
    /// The owner's longest drain-to-drain gap in the window, µs.
    pub own_gap_max_us: u64,
    /// The window's largest staged − handed − dropped, read at turn starts.
    pub pend_max: u64,
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
    /// The bypass site in force (witness tag).
    bypass: Option<Bypass>,
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
            let beta = self.beta(*id, i);
            let l_s = self.terms(*id).l_s();
            let open = admits(i.backlog(), beta);
            let p = self.paths.entry(*id).or_default();
            p.grant = Some(grant_of(open, beta));
            p.note_window(now_us, i, beta, l_s);
            p.turns += 1;
            p.closed_turns += (!open) as u64;
            p.last_beta = beta;
            if let Some(be) = beta {
                if p.beta_s.len() < MAX_SAMPLES {
                    p.beta_s.push(be);
                }
            }
            p.pend_max = p.pend_max.max(i.pending());
            p.note_kinds(i.kinds);
            p.kinds_turn = i.kinds;
            if open {
                p.kinds_open = Some(i.kinds);
            }
            if !open && waiting && !store_paused {
                p.refused = true;
            }
            any_open |= open;
        }
        !store_paused && !any_open
    }

    /// The loop's end, before the flush: sample `B − β` after this turn's staging
    /// and take the owner's gap (the readings carry it in `own_gap_us`).
    pub(crate) fn end_turn(&mut self, ins: &[(u32, PathIn)]) {
        for (id, i) in ins {
            let beta = self.beta(*id, i);
            let p = self.paths.entry(*id).or_default();
            if let Some(be) = beta {
                let total = i.backlog() as f64 - be;
                // Booked since the last turn that admitted: first sends and exempt datagrams staged
                // since then; the rest of the excess is carry-in or drain, never booked to a kind.
                let base = p.kinds_open.unwrap_or(p.kinds_turn);
                let first = i.kinds[0].saturating_sub(base[0]) as f64;
                let exempt = (1..4).map(|k| i.kinds[k].saturating_sub(base[k])).sum::<u64>() as f64;
                if p.ex_max.is_none_or(|m| total > m.0) {
                    p.ex_max = Some((total, first, exempt));
                }
            }
            p.own_gap_max_us = p.own_gap_max_us.max(i.own_gap_us);
            p.b_max = p.b_max.max(i.backlog());
            p.note_kinds(i.kinds);
        }
    }

    /// Mid-turn, before a first send is taken from the intake: is any path
    /// open? A closed path with a first send waiting refuses it.
    pub(crate) fn check(&mut self, waiting: bool, ins: &[(u32, PathIn)]) -> bool {
        let mut any_open = ins.is_empty();
        for (id, i) in ins {
            let beta = self.beta(*id, i);
            let open = admits(i.backlog(), beta);
            let p = self.paths.entry(*id).or_default();
            p.grant = Some(grant_of(open, beta));
            if !open && waiting {
                p.refused = true;
            }
            any_open |= open;
        }
        any_open
    }

    /// The newest decision's candidate set, read-free: the paths it admitted
    /// (an admit, or a cold-start admit). The emission spends THIS, not a second
    /// read of the views, so what it places on is what the site decided.
    pub(crate) fn granted(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self
            .paths
            .iter()
            .filter(|(_, p)| matches!(p.grant, Some(Grant::Admit | Grant::Cold)))
            .map(|(id, _)| *id)
            .collect();
        ids.sort_unstable();
        ids
    }

    /// Whether the gate reads any path view yet (a decision over no view admits nothing and refuses nothing).
    pub(crate) fn has_views(&self) -> bool {
        !self.views.is_empty()
    }

    /// The bypass site in force, if any.
    pub(crate) fn bypass(&self) -> Option<Bypass> {
        self.bypass
    }

    /// Paths with room now (placement's candidate set for a first send). Empty
    /// only when every path is closed. The newest decision: it grants (or
    /// revokes) the token of every path it covers.
    pub(crate) fn open_ids(&mut self, ins: &[(u32, PathIn)]) -> Vec<u32> {
        let mut open = Vec::new();
        for (id, i) in ins {
            let beta = self.beta(*id, i);
            let admitted = admits(i.backlog(), beta);
            self.paths.entry(*id).or_default().grant = Some(grant_of(admitted, beta));
            if admitted {
                open.push(*id);
            }
        }
        open
    }

    /// The turn ended because the store is full: it does not update the predicate.
    pub(crate) fn note_store_end(&mut self) {
        self.store_ended = true;
    }

    /// Tag the first sends staged from now on as passing a bypass site (`None`
    /// clears). Witness only: no decision reads it.
    pub(crate) fn set_bypass(&mut self, b: Option<Bypass>) {
        self.bypass = b;
    }

    /// A first send is about to be staged on `path`: spend the newest decision's
    /// token and count how the send stands ([`Grant`]), then as
    /// [`Self::note_first_send`]. No view read: a re-read at staging time can be
    /// skewed against the decision, which is what this witness must not be.
    ///
    /// (An `fs_late` field, the old fresh-read comparison, is dropped: its only
    /// reading without a per-send view read is the one the decision already used.)
    pub(crate) fn stage_first_send(&mut self, path: u32) {
        let grant = self.paths.get(&path).and_then(|p| p.grant);
        let class = classify_first_send(grant, self.bypass);
        for p in self.paths.values_mut() {
            if p.grant.is_some() {
                p.grant = Some(Grant::Spent);
            }
        }
        let p = self.paths.entry(path).or_default();
        match class {
            FsClass::Gated => {}
            FsClass::Over => {
                p.fs_over += 1;
                p.fs_over_cs[(grant == Some(Grant::Spent)) as usize] += 1;
            }
            FsClass::Bypass(b) => p.fs_byp[b as usize] += 1,
            FsClass::Cold => p.fs_byp[3] += 1,
        }
        self.note_first_send(path);
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
            fs_over: p.fs_over,
            fs_over_cs: p.fs_over_cs,
            fs_bypass: p.fs_byp,
            ex_max: p.ex_max,
            ex_n: [3usize, 1, 2].map(|k| p.kinds_last[k].saturating_sub(p.kinds_start.unwrap_or(p.kinds_last)[k])),
            fs_n: p.kinds_last[0].saturating_sub(p.kinds_start.unwrap_or(p.kinds_last)[0]),
            beta_max: p.beta_max,
            b_max: p.b_max,
            u_over: {
                let (db, dg) = p.bat_start.map_or((0, 0), |s| (p.bat_last.0.saturating_sub(s.0), p.bat_last.1.saturating_sub(s.1)));
                let batch = if db > 0 { dg.div_ceil(db).max(1) } else { 1 };
                p.beta_max.is_some_and(|m| p.b_max as f64 > m.0 + batch as f64) as u64
            },
            ack_bind: (p.ack_turns > 0).then(|| p.ack_bound as f64 / p.ack_turns as f64),
            beta_q: {
                p.beta_s.sort_by(|a, b| a.total_cmp(b));
                (!p.beta_s.is_empty()).then(|| {
                    let q = |x| super::hoff::quantile(&p.beta_s, x);
                    (q(0.05), q(0.5), q(0.95))
                })
            },
            own_gap_max_us: p.own_gap_max_us,
            pend_max: p.pend_max,
        };
        p.fs_over = 0;
        p.fs_over_cs = [0; 2];
        p.beta_max = None;
        p.b_max = 0;
        p.bat_start = Some(p.bat_last);
        p.ack_turns = 0;
        p.ack_bound = 0;
        p.fs_byp = [0; 4];
        p.beta_s.clear();
        p.ex_max = None;
        p.kinds_start = Some(p.kinds_last);
        p.own_gap_max_us = 0;
        p.pend_max = 0;
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

    /// The decision's inputs alone, per view ([`super::hoff::HoffGauge::read_decision`]).
    fn read_decisions(&mut self) -> Vec<(u32, PathIn)> {
        let mut b = std::mem::take(&mut self.buf);
        b.clear();
        b.extend(self.views.iter().map(|(id, v)| (*id, super::hoff::HoffGauge::read_decision(v))));
        b
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

    /// Loop end, live: see [`Self::end_turn`]. Takes each owner's gap.
    pub(crate) fn end_turn_live(&mut self) {
        use std::sync::atomic::Ordering::Relaxed;
        let mut ins = self.read_all();
        for ((_, i), (_, v)) in ins.iter_mut().zip(&self.views) {
            i.own_gap_us = v.hoff.own_gap_max_us.swap(0, Relaxed);
        }
        self.end_turn(&ins);
        self.buf = ins;
    }

    /// A first send is about to be staged on `path`, live: see [`Self::stage_first_send`].
    pub(crate) fn note_first_send_live(&mut self, path: u32) {
        self.stage_first_send(path);
    }

    /// Before a first send is taken, live: see [`Self::check`].
    pub(crate) fn check_live(&mut self, waiting: bool) -> bool {
        let ins = self.read_decisions();
        let open = self.check(waiting, &ins);
        self.buf = ins;
        open
    }

    /// Placement's candidate set for a first send, live.
    pub(crate) fn open_live(&mut self) -> Vec<u32> {
        let ins = self.read_decisions();
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
