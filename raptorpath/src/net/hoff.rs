//! The `[HOFF]` gauge line: the hand-off reserve's quantities (paper §6.8,
//! revision 4), REPORTED ONLY. No decision reads anything here; `gate=off`
//! says so.
//!
//! It rides the `[DIAG]` cadence and gate (`RWM_DIAG`, one `report()` call per
//! sender-loop iteration under `pol.diag_on`), so the shipped path pays
//! nothing. The producer-side inputs (`HoffCounters` on the path view) are
//! counted only when `RWM_DIAG` was on at construction.
//!
//! ```text
//!   B_i  = staged_i − tx_i                  producer's datagrams handed to the owner minus quinn's DATAGRAM frames sent
//!   R_i  = 1.25 · cwnd_i / srtt_i           cwnd_i = the controller's `window()` (RouteProbe atomic)
//!   L_i  = τ̂_o + ℓ̂_o + τ̂_p + ℓ̂_p
//!   β_i  = R_i · L_i / s_i                  (datagrams)
//! ```
//!
//! Terms, per tick window (250 ms; the registration's "last second" is the
//! same p99 over a shorter span):
//!   * τ̂_p: p99 of the producer's realized wait per `paused`-arm wake (the 1 ms
//!     backpressure poll), floored at 1 ms; no sample = cold start = floor.
//!   * τ̂_o: the owner's republish timer does not exist yet (it ships with the
//!     gate commit): printed as `assumed`, 2 ms, floored at 1 ms.
//!   * ℓ̂_p: p99 of the producer's loop-body time per iteration.
//!   * ℓ̂_o: p99 of the owner's work in its data-drain arm plus the publish
//!     (log2/2 histogram, bucket upper edge, so within 41 %); other arms are
//!     not timed.
//!   * `tau_floor_bind`: cumulative share of ticks on which τ̂_p sat at its
//!     1 ms floor (the assumed τ̂_o is 2 ms and never binds).
//!   * `g_batch` (window wall while units wait over the owner's data batches)
//!     is printed for reference only; it is not an input to β.
//!
//! Where the derivation leaves room:
//!   * a *turn* is one `report()` call (one sender-loop iteration) and considers
//!     every registered path; "units wait" = the store is non-empty (global);
//!   * a turn is *closed* for path i when units wait and B_i ≥ β_i (β of the
//!     previous tick; open before the first tick). `hand_bind` = share of turns
//!     closed. `spare_i` = the predicate: 0 if the last completed turn was
//!     closed, else inf (a store-full turn leaves it unchanged); `spare_bind` =
//!     share of waiting turns that read spare_i = 0;
//!   * `reopen_i`: from B_i first reading below β_i after a closed turn to the
//!     next turn that sees a new hand-off (turn resolution; p50/max ms / n);
//!   * `starve` = share of turns with B_i = 0, units waiting and quinn's window
//!     open (window > bytes in flight);
//!   * `app_limited` = share of quinn's `on_end_acks` that were app-limited;
//!     `batch` = datagrams per data hand-off; `store_pin` = cumulative share of
//!     lines with the store at its cap; `probe_acks` = cumulative `on_end_acks`
//!     publishes (the atomics are the only quinn read: `congestion_state()` is
//!     never called);
//!   * `fifo_wait_resend` = B_i·s_i/R_i at each source resend's hand-off (p95 ms
//!     / count), at the SACK-gap and request-law resend site;
//!   * `owner_timer_wakes` = 0 until the gate commit builds the timer.

use std::collections::HashMap;
use std::fmt::Write as _;

/// Samples kept per window (a turn beyond it still counts).
const MAX_SAMPLES: usize = 1 << 15;
/// Quinn's pacer refill gain (RFC 9002 §7.7).
const PACER_GAIN: f64 = 1.25;
/// The timer terms' cited period and floor (`poll_1ms`, `VIEW_REFRESH`), s.
const TIMER_FLOOR_S: f64 = 0.001;
/// τ̂_o until the owner's republish timer exists, s.
const OWNER_TIMER_ASSUMED_S: f64 = 0.002;

/// `R_i = 1.25·cwnd/srtt`, bytes/s. 0 for a path with no RTT yet.
pub(crate) fn pacer_rate(cwnd_bytes: f64, srtt_s: f64) -> f64 {
    if srtt_s > 0.0 { PACER_GAIN * cwnd_bytes / srtt_s } else { 0.0 }
}

/// `L = τ̂_o + ℓ̂_o + τ̂_p + ℓ̂_p` with both timer terms floored at their period;
/// the flag says a timer term sat at its floor. Seconds.
pub(crate) fn loop_latency(tau_o: f64, l_o: f64, tau_p: f64, l_p: f64) -> (f64, bool) {
    let at_floor = tau_o <= TIMER_FLOOR_S || tau_p <= TIMER_FLOOR_S;
    (tau_o.max(TIMER_FLOOR_S) + l_o + tau_p.max(TIMER_FLOOR_S) + l_p, at_floor)
}

/// `β = R·L/s`, datagrams. `None` without a datagram size.
pub(crate) fn beta(r_bps: f64, l_s: f64, s_bytes: f64) -> Option<f64> {
    (s_bytes > 0.0 && l_s.is_finite()).then(|| r_bps * l_s / s_bytes)
}

/// Nearest-rank quantile of an ascending slice (`q` in 0..=1).
fn quantile<T: Copy + Default>(sorted: &[T], q: f64) -> T {
    if sorted.is_empty() {
        return T::default();
    }
    let i = ((sorted.len() as f64 * q).ceil() as usize).clamp(1, sorted.len()) - 1;
    sorted[i]
}

/// p99 (upper bucket edge, s) of the window delta of a cumulative histogram.
pub(crate) fn hist_p99_s(now: &[u64], prev: &[u64]) -> f64 {
    let d: Vec<u64> = now.iter().zip(prev).map(|(a, b)| a.saturating_sub(*b)).collect();
    let tot: u64 = d.iter().sum();
    if tot == 0 {
        return 0.0;
    }
    let need = (tot as f64 * 0.99).ceil() as u64;
    let mut acc = 0;
    for (k, c) in d.iter().enumerate() {
        acc += c;
        if acc >= need {
            return crate::transport::io_owner::WorkHist::upper_ns(k) / 1e9;
        }
    }
    0.0
}

/// One path's raw readings at a turn (cumulative counters, as published).
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct PathIn {
    pub staged: u64,
    pub staged_bytes: u64,
    /// quinn's DATAGRAM frames sent (the owner's snapshot).
    pub tx: u64,
    /// Datagrams quinn accepted (`RWM_DIAG` audit).
    pub hand: u64,
    /// The view's snapshot cwnd (used until the probe has published a window).
    pub cwnd: u64,
    /// The controller's `window()` as the probe last published it.
    pub qwin: u64,
    pub srtt_us: u64,
    pub qinfl: u64,
    pub end_acks: u64,
    pub app_limited_acks: u64,
    pub polls: u64,
    pub tx_batches: u64,
    pub tx_dgrams: u64,
    /// Datagrams the owner discarded before quinn (send errors, shim loss).
    pub dropped: u64,
}

impl PathIn {
    pub(crate) fn backlog(&self) -> u64 {
        self.staged.saturating_sub(self.tx)
    }
    pub(crate) fn dgram_size(&self) -> f64 {
        if self.staged > 0 { self.staged_bytes as f64 / self.staged as f64 } else { 0.0 }
    }
    fn window(&self) -> u64 {
        if self.qwin > 0 { self.qwin } else { self.cwnd }
    }
    pub(crate) fn rate(&self) -> f64 {
        pacer_rate(self.window() as f64, self.srtt_us as f64 / 1e6)
    }
}

#[derive(Default)]
struct PathState {
    /// B_i per turn this window.
    b: Vec<u32>,
    turns: u64,
    starve: u64,
    closed: u64,
    waiting: u64,
    spare_zero: u64,
    /// The spare predicate's latch: the last completed turn was closed.
    prev_closed: bool,
    /// β of the previous tick (None before the first).
    beta_prev: Option<f64>,
    below_since: Option<u64>,
    last_staged: u64,
    reopen_ms: Vec<f64>,
    /// The previous tick's cumulative counters (window deltas).
    prev: PathIn,
    prev_hist: Vec<u64>,
    /// ℓ̂_o p99 of the window, s (set at the tick).
    l_o: f64,
    /// Latest reading (resends use it).
    last: PathIn,
    /// fifo_wait_resend samples, ms.
    resend_ms: Vec<f64>,
    resend_n: u64,
}

/// The gauge's per-path windows. Lives in `DiagState`.
pub(crate) struct HoffGauge {
    paths: HashMap<u32, PathState>,
    last_turn_us: u64,
    wait_us: u64,
    start_us: u64,
    win_start_us: u64,
    lines: u64,
    pinned: u64,
    /// Realized paused-arm waits and producer loop-body times, µs, this window.
    paused_us: Vec<u32>,
    work_us: Vec<u32>,
    floor_reads: u64,
    floor_binds: u64,
    /// The path views read each turn (refreshed at every tick).
    views: Vec<(u32, std::sync::Arc<crate::transport::io_owner::PathView>)>,
}

impl HoffGauge {
    pub(crate) fn new(now_us: u64) -> Self {
        Self {
            paths: HashMap::new(),
            last_turn_us: now_us,
            wait_us: 0,
            start_us: now_us,
            win_start_us: now_us,
            lines: 0,
            pinned: 0,
            paused_us: Vec::new(),
            work_us: Vec::new(),
            floor_reads: 0,
            floor_binds: 0,
            views: Vec::new(),
        }
    }

    /// One realized wait of the producer's paused-arm (1 ms) poll, µs.
    pub(crate) fn paused_wait(&mut self, us: u64) {
        if self.paused_us.len() < MAX_SAMPLES {
            self.paused_us.push(us.min(u32::MAX as u64) as u32);
        }
    }

    /// One producer loop iteration's body time, µs.
    pub(crate) fn iter_work(&mut self, us: u64) {
        if self.work_us.len() < MAX_SAMPLES {
            self.work_us.push(us.min(u32::MAX as u64) as u32);
        }
    }

    /// The owner's ℓ̂_o p99 for path `id` (s); the tick reads it from the view.
    #[cfg(test)]
    fn set_owner_work(&mut self, id: u32, l_o_s: f64) {
        self.paths.entry(id).or_default().l_o = l_o_s;
    }

    /// One sender-loop iteration (`report()`): read every path view and sample.
    pub(crate) fn turn_views(&mut self, now_us: u64, units_wait: bool, store_full: bool) {
        let ins: Vec<(u32, PathIn)> = self.views.iter().map(|(id, v)| (*id, Self::read(v))).collect();
        self.turn(now_us, units_wait, store_full, &ins);
    }

    pub(crate) fn no_views(&self) -> bool {
        self.views.is_empty()
    }

    /// Re-read the registered path set (called at every tick).
    pub(crate) fn set_views(&mut self, views: Vec<(u32, std::sync::Arc<crate::transport::io_owner::PathView>)>) {
        self.views = views;
    }

    /// A source resend went to `path`: record its FIFO wait.
    pub(crate) fn resend_on(&mut self, path: u32) {
        if let Some(i) = self.views.iter().find(|(id, _)| *id == path).map(|(_, v)| Self::read(v)) {
            self.resend(path, i);
        }
    }

    /// One producer turn: sample B_i, the gate's would-be state and the starve condition.
    pub(crate) fn turn(&mut self, now_us: u64, units_wait: bool, store_full: bool, ins: &[(u32, PathIn)]) {
        if units_wait {
            self.wait_us += now_us.saturating_sub(self.last_turn_us);
        }
        self.last_turn_us = now_us;
        for (id, i) in ins {
            let st = self.paths.entry(*id).or_default();
            st.turns += 1;
            let b = i.backlog();
            if st.b.len() < MAX_SAMPLES {
                st.b.push(b.min(u32::MAX as u64) as u32);
            }
            if b == 0 && units_wait && i.window() > i.qinfl {
                st.starve += 1;
            }
            let closed = units_wait && st.beta_prev.is_some_and(|be| b as f64 >= be);
            if units_wait {
                st.waiting += 1;
                st.spare_zero += st.prev_closed as u64;
            }
            st.closed += closed as u64;
            // reopen: B back below β after a closed turn, until the next hand-off.
            if i.staged > st.last_staged {
                if let Some(t0) = st.below_since.take() {
                    if st.reopen_ms.len() < MAX_SAMPLES {
                        st.reopen_ms.push(now_us.saturating_sub(t0) as f64 / 1e3);
                    }
                }
            }
            if closed {
                st.below_since = None;
            } else if st.below_since.is_none() && st.prev_closed {
                st.below_since = Some(now_us);
            }
            // The latch: a store-full turn has no first send waiting for a path.
            if !store_full {
                st.prev_closed = closed;
            }
            st.last_staged = i.staged;
            st.last = *i;
        }
    }

    /// A source resend handed to path `id` now: record B_i·s_i/R_i.
    pub(crate) fn resend(&mut self, id: u32, i: PathIn) {
        let st = self.paths.entry(id).or_default();
        st.resend_n += 1;
        let r = i.rate();
        if r > 0.0 && st.resend_ms.len() < MAX_SAMPLES {
            st.resend_ms.push(i.backlog() as f64 * i.dgram_size() / r * 1e3);
        }
    }

    /// Read ℓ̂_o from the views, then close the window (see [`Self::line`]).
    pub(crate) fn line_from_views(&mut self, now_us: u64, engine_infl: u64, store_full: bool) -> String {
        for (id, v) in &self.views {
            let st = self.paths.entry(*id).or_default();
            let h = v.hoff.work.snapshot();
            if st.prev_hist.is_empty() {
                st.prev_hist = vec![0; h.len()];
            }
            st.l_o = hist_p99_s(&h, &st.prev_hist);
            st.prev_hist = h.to_vec();
        }
        self.line(now_us, engine_infl, store_full)
    }

    /// [`Self::line`] reading the hand-off gate's real state when one is given.
    pub(crate) fn line_gated(
        &mut self,
        now_us: u64,
        engine_infl: u64,
        store_full: bool,
        _gate: Option<&mut super::hoff_gate::HandoffGate>,
    ) -> String {
        self.line(now_us, engine_infl, store_full)
    }

    /// Close the window and render the `[HOFF]` line. `engine_infl` is the
    /// engine's `in_flight` (datagrams), `store_full` whether `n ≥ N·P`.
    pub(crate) fn line(&mut self, now_us: u64, engine_infl: u64, store_full: bool) -> String {
        let win_s = now_us.saturating_sub(self.win_start_us) as f64 / 1e6;
        self.lines += 1;
        self.pinned += store_full as u64;
        // The producer's terms (shared by every path: one loop).
        self.paused_us.sort_unstable();
        self.work_us.sort_unstable();
        let tau_p_raw = (!self.paused_us.is_empty()).then(|| quantile(&self.paused_us, 0.99) as f64 / 1e6);
        let tau_p = tau_p_raw.unwrap_or(TIMER_FLOOR_S);
        let l_p = quantile(&self.work_us, 0.99) as f64 / 1e6;
        let tau_o = OWNER_TIMER_ASSUMED_S;
        self.floor_reads += 1;
        self.floor_binds += (tau_p <= TIMER_FLOOR_S) as u64;
        let mut ids: Vec<u32> = self.paths.keys().copied().collect();
        ids.sort_unstable();
        let mut out = String::new();
        let mut outstanding = 0.0;
        for id in ids {
            let st = self.paths.get_mut(&id).expect("id from keys");
            let (cur, prev) = (st.last, st.prev);
            let d = |a: u64, b: u64| a.saturating_sub(b);
            let s = cur.dgram_size();
            outstanding += if s > 0.0 { cur.qinfl as f64 / s } else { 0.0 } + cur.backlog() as f64;
            let r = cur.rate();
            let (l, _) = loop_latency(tau_o, st.l_o, tau_p, l_p);
            let beta = beta(r, l, s);
            st.beta_prev = beta;
            let batches = d(cur.tx_batches, prev.tx_batches);
            let g_batch = if batches > 0 { self.wait_us as f64 / 1e6 / batches as f64 } else { f64::NAN };
            let polls = d(cur.polls, prev.polls);
            let phat = if polls > 0 { win_s / polls as f64 } else { f64::NAN };
            st.b.sort_unstable();
            let n = st.turns.max(1) as f64;
            let acks = d(cur.end_acks, prev.end_acks);
            let al = if acks > 0 { d(cur.app_limited_acks, prev.app_limited_acks) as f64 / acks as f64 } else { f64::NAN };
            let batch = if batches > 0 { d(cur.tx_dgrams, prev.tx_dgrams) as f64 / batches as f64 } else { f64::NAN };
            st.resend_ms.sort_by(|a, b| a.total_cmp(b));
            st.reopen_ms.sort_by(|a, b| a.total_cmp(b));
            let _ = write!(
                out,
                " p{id}[B={}/{}/{} n={} dgq={} F={} win={} R={:.0} tau_o={}ms(assumed) tau_p={}ms l_o={}ms l_p={}ms L={}ms beta={} hand_bind={} spare_i={} spare_bind={} reopen={}/{}ms/{} starve={} app_limited={} batch={} g_batch={}ms phat={}ms fifo_wait_resend={}ms/{}]",
                quantile(&st.b, 0.5),
                quantile(&st.b, 0.95),
                st.b.last().copied().unwrap_or(0),
                st.turns,
                cur.hand.saturating_sub(cur.tx),
                cur.qinfl,
                cur.window(),
                r,
                ms(tau_o),
                ms(tau_p),
                ms(st.l_o),
                ms(l_p),
                ms(l),
                f1(beta.unwrap_or(f64::NAN)),
                f2(st.closed as f64 / n),
                if st.prev_closed { "0" } else { "inf" },
                f2(st.spare_zero as f64 / st.waiting.max(1) as f64),
                f2(quantile(&st.reopen_ms, 0.5)),
                f2(st.reopen_ms.last().copied().unwrap_or(0.0)),
                st.reopen_ms.len(),
                f2(st.starve as f64 / n),
                f2(al),
                f1(batch),
                ms(g_batch),
                ms(phat),
                f2(quantile(&st.resend_ms, 0.95)),
                st.resend_n,
            );
            st.b.clear();
            st.turns = 0;
            st.starve = 0;
            st.closed = 0;
            st.waiting = 0;
            st.spare_zero = 0;
            st.reopen_ms.clear();
            st.resend_ms.clear();
            st.resend_n = 0;
            st.prev = cur;
        }
        let ratio = if outstanding > 0.0 { engine_infl as f64 / outstanding } else { f64::NAN };
        let probe_acks: u64 = self.paths.values().map(|p| p.last.end_acks).sum();
        let l = format!(
            "[HOFF] t={:.1}s gate=off owner_timer_wakes=0 probe_acks={} tau_floor_bind={:.2}{} infl_vs_out={}/{:.0}/x{} store_pin={:.2}",
            now_us.saturating_sub(self.start_us) as f64 / 1e6,
            probe_acks,
            self.floor_binds as f64 / self.floor_reads as f64,
            out,
            engine_infl,
            outstanding,
            f2(ratio),
            self.pinned as f64 / self.lines as f64,
        );
        self.win_start_us = now_us;
        self.wait_us = 0;
        self.paused_us.clear();
        self.work_us.clear();
        l
    }
}

fn ms(s: f64) -> String {
    if s.is_finite() { format!("{:.2}", s * 1e3) } else { "-".into() }
}
fn f1(v: f64) -> String {
    if v.is_finite() { format!("{v:.2}") } else { "-".into() }
}
fn f2(v: f64) -> String {
    if v.is_finite() { format!("{v:.3}") } else { "-".into() }
}

impl HoffGauge {
    /// Read one path view's raw counters.
    pub(crate) fn read(view: &crate::transport::io_owner::PathView) -> PathIn {
        use std::sync::atomic::Ordering::Relaxed;
        let h = &view.hoff;
        let c = &view.ctr;
        PathIn {
            staged: h.staged.load(Relaxed),
            staged_bytes: h.staged_bytes.load(Relaxed),
            tx: view.frame_stats().map_or(0, |(_, tx)| tx),
            hand: view.audit.handoff.load(Relaxed),
            cwnd: view.path_stats().map_or(0, |p| p.0),
            qwin: h.qwin.load(Relaxed),
            srtt_us: view.rtt().map_or(0, |d| d.as_micros() as u64),
            qinfl: h.qinfl.load(Relaxed),
            end_acks: h.end_acks.load(Relaxed),
            app_limited_acks: h.app_limited_acks.load(Relaxed),
            polls: c.polls.load(Relaxed),
            tx_batches: c.tx_batches.load(Relaxed),
            tx_dgrams: c.tx_dgrams.load(Relaxed),
            dropped: h.dropped.load(Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: f64 = 1233.0;

    /// Leg 1 of the registration's red-first list: cwnd 280 000 B, srtt 22 ms,
    /// τ̂_o 2.00 ms, ℓ̂_o 16 µs, τ̂_p 2.06 ms, ℓ̂_p 96 µs → L 4.172 ms, β 53.83.
    #[test]
    fn beta_anchor_leg1() {
        let r = pacer_rate(280_000.0, 0.022);
        assert!((r - 15_909_090.9).abs() < 1.0, "R = {r}");
        let (l, floor) = loop_latency(0.002, 16e-6, 0.00206, 96e-6);
        assert!((l - 0.004172).abs() < 1e-9 && !floor, "L = {l}");
        let b = beta(r, l, S).unwrap();
        assert!((b - 53.83).abs() < 0.005, "beta = {b}");
    }

    /// Second leg: cwnd 230 000 B, srtt 88 ms, τ̂_o 2.00, ℓ̂_o 10 µs, τ̂_p 2.07,
    /// ℓ̂_p 557 µs → L 4.637 ms, R 3 267 045 B/s, β 12.29.
    #[test]
    fn beta_anchor_second_leg() {
        let r = pacer_rate(230_000.0, 0.088);
        let (l, _) = loop_latency(0.002, 10e-6, 0.00207, 557e-6);
        assert!((l - 0.004637).abs() < 1e-9, "L = {l}");
        let b = beta(r, l, S).unwrap();
        assert!((b - 12.29).abs() < 0.005, "beta = {b}");
    }

    /// Cold start: both timer terms at the 1 ms floor, ℓ̂ as leg 1 → L 2.112 ms,
    /// β 27.25; the floor flag is set, and a sub-floor timer reads as the floor.
    #[test]
    fn beta_anchor_cold_start_floors() {
        let r = pacer_rate(280_000.0, 0.022);
        let (l, floor) = loop_latency(0.0, 16e-6, 0.0, 96e-6);
        assert!(floor && (l - 0.002112).abs() < 1e-9, "L = {l}");
        assert!((beta(r, l, S).unwrap() - 27.25).abs() < 0.005);
        assert!(beta(r, l, 0.0).is_none());
        assert_eq!(pacer_rate(1.0, 0.0), 0.0);
    }

    fn input(staged: u64, tx: u64) -> PathIn {
        PathIn {
            staged,
            staged_bytes: staged * 1233,
            tx,
            hand: tx + 3,
            cwnd: 280_000,
            qwin: 280_000,
            srtt_us: 22_000,
            qinfl: 60_000,
            end_acks: 10,
            app_limited_acks: 5,
            polls: 100,
            tx_batches: 50,
            tx_dgrams: 500,
            dropped: 0,
        }
    }

    /// Feed the c2-like window: paused waits 2.06 ms, producer work 96 µs.
    fn fed(g: &mut HoffGauge) {
        for _ in 0..100 {
            g.paused_wait(2060);
            g.iter_work(96);
        }
        g.set_owner_work(7, 16e-6);
    }

    /// B_i = staged − tx with p50/p95/max over the window's turns, and β from
    /// the four L terms (the first line is the cold-open one: β printed, no
    /// closed turns yet).
    #[test]
    fn backlog_quantiles_and_beta_from_given_inputs() {
        let mut g = HoffGauge::new(0);
        fed(&mut g);
        for k in 1..=20u64 {
            g.turn(k * 1000, true, false, &[(7, input(1000 + k, 1000))]);
        }
        let l = g.line(20_000, 100, false);
        // p50 = 10th, p95 = 19th, max = 20 (nearest rank).
        assert!(l.contains(" p7[B=10/19/20 n=20 "), "{l}");
        assert!(l.contains(" dgq=3 "), "{l}");
        assert!(l.contains(" L=4.17ms "), "{l}");
        assert!(l.contains(" beta=53.83 "), "{l}");
        assert!(l.contains(" hand_bind=0.000 "), "{l}");
    }

    /// With β = 53.83 known, closed turns count, spare_i latches to 0 through
    /// a store-full turn, and the reopen time is read at the next hand-off.
    #[test]
    fn closed_turns_latch_spare_and_reopen() {
        let mut g = HoffGauge::new(0);
        fed(&mut g);
        g.turn(1000, true, false, &[(7, input(1000, 1000))]);
        let _ = g.line(250_000, 0, false); // β_prev = 53.83
        for k in 1..=4u64 {
            g.turn(250_000 + k * 1000, true, false, &[(7, input(1100, 1000))]); // B = 100 ≥ β
        }
        // store-full turn with B = 0 must not unlatch
        g.turn(255_000, true, true, &[(7, input(1100, 1100))]);
        // B first reads below β at 255 ms; a hand-off is seen at 258 ms
        g.turn(256_000, true, false, &[(7, input(1100, 1100))]);
        g.turn(258_000, true, false, &[(7, input(1120, 1100))]);
        let l = g.line(500_000, 0, false);
        assert!(l.contains(" hand_bind=0.571 "), "{l}"); // 4 of 7 turns
        assert!(l.contains(" reopen=3.000/3.000ms/1 "), "{l}");
        assert!(l.contains(" spare_i=inf "), "{l}");
    }

    /// A 1.5 s silence (no turns, no hand-off) does not move β: it is built
    /// from timer latency and work, not from an interval.
    #[test]
    fn beta_bounded_across_silence() {
        let mut g = HoffGauge::new(0);
        fed(&mut g);
        g.turn(1000, true, false, &[(7, input(1000, 1000))]);
        let _ = g.line(250_000, 0, false);
        fed(&mut g);
        g.turn(1_750_000, true, false, &[(7, input(1000, 1000))]);
        let l = g.line(1_750_001, 0, false);
        let b: f64 = l.split(" beta=").nth(1).unwrap().split(' ').next().unwrap().parse().unwrap();
        assert!((48.4..=59.2).contains(&b), "{l}");
    }

    /// The line prints `gate=off` and every field when enabled.
    #[test]
    fn line_prints_gate_off_and_all_fields() {
        let mut g = HoffGauge::new(0);
        g.turn(1000, true, false, &[(0, input(10, 4))]);
        g.resend(0, input(10, 4));
        let l = g.line(300_000, 40, true);
        assert!(l.starts_with("[HOFF] t=0.3s gate=off owner_timer_wakes=0 "), "{l}");
        for f in [
            "B=", "n=", "dgq=", "F=", "win=", "R=", "tau_o=", "(assumed)", "tau_p=", "l_o=", "l_p=",
            "L=", "beta=", "hand_bind=", "spare_i=", "spare_bind=", "reopen=", "starve=",
            "app_limited=", "batch=", "g_batch=", "phat=", "fifo_wait_resend=", "tau_floor_bind=",
            "probe_acks=", "infl_vs_out=", "store_pin=",
        ] {
            assert!(l.contains(f), "missing {f}: {l}");
        }
        assert!(l.contains(" batch=10.00 "), "{l}");
        assert!(l.contains(" app_limited=0.500 "), "{l}");
        assert!(l.contains("store_pin=1.00"), "{l}");
        assert!(l.contains("tau_floor_bind=1.00"), "{l}"); // cold start: floor
        assert!(!l.contains("gate=on"), "{l}");
        // fifo wait: B = 6, s = 1233, R = 15 909 091 → 0.465 ms
        assert!(l.contains(" fifo_wait_resend=0.465ms/1]"), "{l}");
    }
}
