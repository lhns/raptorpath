//! `[ETA]` — THE SENDER'S OWN PREDICTION, READ AT BOTH ENDS.
//!
//! **The measurand, and why it is owed.** The placement law (`place_costs`,
//! paper §16.3) already computes, for every candidate path, the time a symbol
//! handed to that path now takes to reach the receiver — `E_i` =
//! [`crate::scheduler::PathState::expected_delivery_load`]. It uses that
//! number to CHOOSE, and then throws it away. The receiver, at the other end
//! of the same symbol, detects holes by SEQUENCE ALONE: it has never been told
//! what the sender expected, so "late" is a guess it makes from ordering.
//!
//! Two ends, one model, never shared. Seed S1 of the search plan says: put the
//! sender's own ETA on the wire (wire v8 `SymbolBatch::eta_rel_us`) and the
//! receiver's hole test becomes *"late against the sender's prediction"* — the
//! lateness measurand §16.80 named and bracketed but never built.
//!
//! **This module is the INSTRUMENT for that, and only the instrument.** It
//! contains no law, no threshold, no gate that changes a decision. Two gauges:
//!
//!   * [`SenderEta`] — owned by the `Scheduler`, because both the placement
//!     site (`net/emit_source.rs`) and the ack site (`net/control_msg.rs`)
//!     already hold that lock and neither would otherwise see the other.
//!     It carries `F̂` (the running max of stamped arrival times — the
//!     sender's own expected frontier, which Track A's HOL term will price
//!     against in Stage 2 and which NOTHING reads today), a bounded per-path
//!     `send_ts → eta_rel` map, and the PREDICTION ERROR
//!
//!         e  =  rtt_us  −  (eta_rel  +  RTprop/2)
//!
//!     evaluated when the ack for that exact batch comes back. The subtracted
//!     `RTprop/2` is the return leg: `rtt_us` is a round trip and `eta_rel`
//!     predicts the forward one, so `e` is what the forward prediction missed
//!     by. Its dispersion `σ̂_e` is the §16.75 τ-lag estimator, at `τ =
//!     RTprop`, over the SAME pairing rule the shipped `tlag_us=` gauge uses.
//!
//!   * [`RecvEta`] — owned by the receiver task. Per path it reads
//!
//!         d  =  (t_arr − send_ts) − eta_rel          (sender clock domain)
//!         ℓ  =  d − min_running(d)                   (the LATENESS)
//!
//!     The subtraction of the path's running minimum is what makes this a
//!     measurement at all: `t_arr` and `send_ts` are two different clocks, so
//!     `d` carries an unknown constant offset, and only DIFFERENCES of `d` are
//!     meaningful. The running min is the best-case realization seen so far,
//!     so `ℓ ≥ 0` by construction and `ℓ = 0` reads "as early as this path has
//!     ever delivered relative to the sender's prediction".
//!
//! **THE PRE-STATED WITNESS, written before either gauge was fed:**
//!
//!         σ̂_sender  ≥  σ̂_recv
//!
//! The sender's error rides a ROUND TRIP (forward queue + return queue + ack
//! scheduling) and the receiver's lateness rides only the FORWARD leg, so the
//! sender's dispersion contains the receiver's plus the return path's. A run
//! where the sender reads STEADIER than the receiver is evidence that one of
//! the two is not measuring what this header says it is — which is why both
//! lines print `sig_us=` in the same units on the same cadence, and why the
//! reachability test reads them as a PAIR rather than one at a time.
//!
//! **The `-`-iff-`n = 0` convention** is `[SUCC]`'s verbatim: an absent
//! reading is never a measured zero, and every value sits beside its own
//! sample count. **No threshold gates any field** — `diag.rs`'s standing
//! reason: a field that disappears below a threshold cannot be told apart from
//! a path that was never sampled.
//!
//! **The BIND GAUGES.** `eta = 0` is the wire's "no prediction" sentinel, and
//! the fraction of arrivals carrying it is printed on BOTH lines rather than
//! filtered away: today only the window source path stamps a prediction, so
//! that fraction IS the instrument's coverage and a reader must be able to see
//! it. `cold_r` and `cold_ge` count the placement law's two COLD PRICES (a
//! path whose correction rate is `∞`, pinned at the 10.0 literal; a path with
//! no burst-model estimate yet) — every clamp gets a bind-fraction gauge.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::time::{Duration, Instant};

use super::succ::Hist;

// ── THE τ-LAG DISPERSION, OVER AN ARBITRARY SERIES ──────────────────────

/// Ring capacity. The shipped `tlag_us=` gauge's `SIGMA_CAND_WINDOW`.
const TLAG_RING: usize = 256;
/// Band width `c`: a pair is admitted iff `τ ≤ lag ≤ c·τ` (§16.75.0).
const TLAG_BAND_C: u32 = 2;
/// Decimation: at most one sample admitted per `τ / m`, so the ring spans
/// `32·τ` at every sample rate (the shipped gauge's arithmetic verbatim).
const TLAG_DECIM_M: u32 = 8;

/// **THE §16.75 τ-LAG DISPERSION over any timestamped µs series.**
///
/// The shipped `PathState::rtt_tlag_us` implements exactly this pairing rule
/// over the RTT series and is not reusable here: it is welded to
/// `copa.rtt_tlag` and to `copa.min_rtt` as `τ`. This is the same rule with
/// the series and `τ` supplied by the caller, so the ETA error, the receiver
/// lateness and the shipped RTT gauge are all the SAME estimand read on three
/// processes rather than three different statistics that happen to share a
/// name.
///
///     P(τ) = { (i, j(i)) : j(i) = argmax { t_j : t_i − t_j ≥ τ }, j < i
///                          admitted iff t_i − t_{j(i)} ≤ c·τ }
///     σ̂_Δ(τ) = median { |v_i − v_j| : (i, j) ∈ P(τ) }
///
/// `None` — rendered `-` — **iff** [`Tlag::pairs`] is 0, by construction:
/// value and count come from one pair-set function.
#[derive(Default, Clone)]
pub struct Tlag {
    ring: VecDeque<(Instant, u64)>,
    last_admit: Option<Instant>,
}

impl Tlag {
    /// Offer one sample. Admitted at most once per `τ / m`; `τ = 0` (no
    /// reference yet) admits nothing, which is the honest reading rather than
    /// a patched one.
    pub fn push(&mut self, t: Instant, v: u64, tau: Duration) {
        if tau.is_zero() {
            return;
        }
        let spacing = tau / TLAG_DECIM_M;
        if let Some(prev) = self.last_admit {
            if t.saturating_duration_since(prev) < spacing {
                return;
            }
        }
        self.last_admit = Some(t);
        if self.ring.len() == TLAG_RING {
            self.ring.pop_front();
        }
        self.ring.push_back((t, v));
    }

    fn diffs(&self, tau: Duration) -> Vec<u64> {
        if tau.is_zero() || self.ring.len() < 2 {
            return Vec::new();
        }
        let hi = tau * TLAG_BAND_C;
        let n = self.ring.len();
        let mut out = Vec::with_capacity(n);
        let mut j = 0usize;
        for i in 0..n {
            let (ti, vi) = self.ring[i];
            while j + 1 < i && ti.saturating_duration_since(self.ring[j + 1].0) >= tau {
                j += 1;
            }
            if j < i {
                let (tj, vj) = self.ring[j];
                let lag = ti.saturating_duration_since(tj);
                if lag >= tau && lag <= hi {
                    out.push(vi.abs_diff(vj));
                }
            }
        }
        out
    }

    /// `σ̂_Δ(τ)`, µs. `None` iff [`Tlag::pairs`] is 0.
    pub fn sigma_us(&self, tau: Duration) -> Option<u64> {
        let mut d = self.diffs(tau);
        if d.is_empty() {
            return None;
        }
        d.sort_unstable();
        // Median as the LOWER order statistic — the shipped gauge's
        // `cand_quantile(.., 0.50)` convention, no interpolation.
        let rank = (d.len() - 1) / 2;
        Some(d[rank])
    }

    /// `|P(τ)|` — the PAIR count, which is what the median is taken over.
    pub fn pairs(&self, tau: Duration) -> u64 {
        self.diffs(tau).len() as u64
    }

    /// Samples currently in the ring (an occupancy reading, not `|P(τ)|`).
    pub fn ring_len(&self) -> usize {
        self.ring.len()
    }
}

/// `-`-iff-none rendering for an optional µs reading.
fn opt(v: Option<u64>) -> String {
    v.map_or_else(|| "-".to_string(), |x| x.to_string())
}

/// `-`-iff-none rendering for an optional fraction.
fn optf(n: u64, d: u64) -> String {
    if d == 0 {
        "-".to_string()
    } else {
        format!("{:.4}", n as f64 / d as f64)
    }
}

// ── THE SENDER SIDE ─────────────────────────────────────────────────────

/// How many outstanding `send_ts → eta_rel` records one path keeps. A hard
/// bound, not a timeout: the map is a lookup for an ack that may never come,
/// and an unbounded one would be a leak on a lossy path. Oldest-first
/// eviction, and the evicted count is printed (`drop=`) so a reader can see
/// when the bound is what is limiting the match rate rather than the loss.
const SEND_BOOK_MAX: usize = 8192;

#[derive(Default)]
struct PathEta {
    /// `send_ts → eta_rel_us` for batches this path stamped, plus the FIFO
    /// that bounds it.
    book: HashMap<u64, u32>,
    order: VecDeque<u64>,
    dropped: u64,
    /// Batches stamped on this path.
    stamped: u64,
    /// Acks that matched a stamped batch — the `e` sample count.
    matched: u64,
    /// `|e|`, µs. A magnitude histogram: the SIGN is carried separately
    /// because "the prediction was optimistic" and "pessimistic" are
    /// different findings and averaging them would hide both.
    err_abs: Hist,
    /// Acks whose realized RTT EXCEEDED the prediction (`e > 0`) — the
    /// optimistic direction.
    late_n: u64,
    /// `σ̂_e` over the SIGNED error, offset into `u64` at [`ERR_BIAS`] so the
    /// τ-lag differences are differences of the signed series.
    err_sig: Tlag,
    /// `τ` for this path's τ-lag, µs — RTprop when measured.
    tau_us: u64,
}

/// The signed prediction error is stored as `e + ERR_BIAS` so it can ride the
/// unsigned τ-lag ring. Differences are unaffected; the bias never reaches a
/// printed value.
const ERR_BIAS: i64 = 1 << 40;

/// **THE SENDER-SITE ETA GAUGE.** Owned by the `Scheduler`. Read by nothing
/// in the data plane; every method is observation only.
#[derive(Default)]
pub struct SenderEta {
    /// **`F̂` — the sender's own expected frontier**: the running max over
    /// every stamped placement of `now + E_picked`, µs on the sender clock.
    /// Track A's HOL term (`s_i = [(now + E_i) − F̂]⁺`) is priced against
    /// exactly this, in Stage 2. NOTHING READS IT TODAY.
    frontier_eta_us: u64,
    /// Every placement offered to the gauge, and how many carried the 0
    /// sentinel. The BIND FRACTION of the wire field.
    stamped_n: u64,
    stamped_zero: u64,
    /// The placement law's two COLD PRICES, counted at `place_costs`:
    /// `cold_r` — a path whose correction rate was `∞` and was priced at the
    /// 10.0 literal; `cold_ge` — a path with no burst-model estimate.
    /// `place_n` is the denominator (per-path cost evaluations).
    cold_r: u64,
    cold_ge: u64,
    place_n: u64,
    paths: BTreeMap<u32, PathEta>,
}

impl SenderEta {
    /// **THE PLACEMENT STAMP.** `eta_rel_us` is the `E_picked` of the path the
    /// law just chose, µs; `send_ts_us` is the batch's own
    /// `send_timestamp_us`, so the ack's echo keys this record exactly.
    ///
    /// Updates `F̂` in the same call — one site, so the frontier can never
    /// describe a different set of placements from the book.
    pub fn stamp(&mut self, path_id: u32, send_ts_us: u64, eta_rel_us: u64) {
        self.stamped_n += 1;
        if eta_rel_us == 0 {
            self.stamped_zero += 1;
        }
        self.frontier_eta_us = self.frontier_eta_us.max(send_ts_us.saturating_add(eta_rel_us));
        let p = self.paths.entry(path_id).or_default();
        p.stamped += 1;
        if p.book.insert(send_ts_us, eta_rel_us.min(u32::MAX as u64) as u32).is_none() {
            p.order.push_back(send_ts_us);
        }
        while p.order.len() > SEND_BOOK_MAX {
            if let Some(old) = p.order.pop_front() {
                if p.book.remove(&old).is_some() {
                    p.dropped += 1;
                }
            }
        }
    }

    /// **THE ACK.** `echo_send_ts_us` is the ack's echoed sender timestamp,
    /// `rtt_us` the realized round trip, `rtprop_us` the path's RTprop (0 when
    /// it has none yet — the sample is still taken, the τ-lag simply admits
    /// nothing until `τ` exists).
    ///
    /// A no-op unless the echo keys a stamped batch, which is what keeps the
    /// error distribution about PREDICTED placements only: repairs, the tail
    /// sweep and every other emitter stamp the 0 sentinel and are matched
    /// here as `e = rtt − rtprop/2` — a reading about the wire, not about a
    /// prediction — so they are DELIBERATELY excluded by the `eta == 0` test.
    pub fn on_ack(&mut self, path_id: u32, echo_send_ts_us: u64, rtt_us: u64, rtprop_us: u64) {
        let Some(p) = self.paths.get_mut(&path_id) else {
            return;
        };
        p.tau_us = rtprop_us;
        let Some(eta_rel) = p.book.remove(&echo_send_ts_us) else {
            return;
        };
        if eta_rel == 0 {
            return;
        }
        let predicted = eta_rel as i64 + (rtprop_us / 2) as i64;
        let e = rtt_us as i64 - predicted;
        p.matched += 1;
        if e > 0 {
            p.late_n += 1;
        }
        p.err_abs.add(e.unsigned_abs());
        p.err_sig.push(
            Instant::now(),
            (e + ERR_BIAS).max(0) as u64,
            Duration::from_micros(rtprop_us),
        );
    }

    /// Fold in a batch of `place_costs` cold-price binds: `cold_r` and
    /// `cold_ge` out of `n` per-path cost evaluations. Observation only; the
    /// costs themselves are untouched.
    pub fn add_place_bind(&mut self, cold_r: u64, cold_ge: u64, n: u64) {
        self.cold_r += cold_r;
        self.cold_ge += cold_ge;
        self.place_n += n;
    }

    /// `F̂`, µs on the sender clock. Read by nothing today.
    pub fn frontier_eta_us(&self) -> u64 {
        self.frontier_eta_us
    }

    /// Has this gauge ever stamped a placement? A receiver-role scheduler
    /// never does and must stay silent.
    pub fn is_sender_site(&self) -> bool {
        self.stamped_n > 0
    }

    /// `σ̂_e` for one path, µs — the witness's left-hand side.
    pub fn sigma_us(&self, path_id: u32) -> Option<u64> {
        let p = self.paths.get(&path_id)?;
        p.err_sig.sigma_us(Duration::from_micros(p.tau_us))
    }

    /// The `[ETA] site=sender` line. Cumulative: the LAST line is the
    /// reading — the `[SUCC]` / `[RFA]` convention.
    pub fn line(&self) -> String {
        let mut s = format!(
            "[ETA] site=sender fhat_us={} n={} zero={} place_n={} cold_r={} cold_ge={}",
            self.frontier_eta_us,
            self.stamped_n,
            optf(self.stamped_zero, self.stamped_n),
            self.place_n,
            optf(self.cold_r, self.place_n),
            optf(self.cold_ge, self.place_n),
        );
        for (id, p) in &self.paths {
            let tau = Duration::from_micros(p.tau_us);
            let q = |x: f64| opt(p.err_abs.quantile(x));
            s.push_str(&format!(
                " p{}:n={}/{} drop={} tau_us={} e_p50={} e_p90={} e_p99={} e_mx={} \
                 late={} sig_us={}/n{}",
                id,
                p.matched,
                p.stamped,
                p.dropped,
                p.tau_us,
                q(0.50),
                q(0.90),
                q(0.99),
                if p.err_abs.n() == 0 { "-".to_string() } else { p.err_abs.max_us().to_string() },
                optf(p.late_n, p.matched),
                opt(p.err_sig.sigma_us(tau)),
                p.err_sig.pairs(tau),
            ));
        }
        s
    }
}

// ── THE RECEIVER SIDE ───────────────────────────────────────────────────

/// Which clock the path's SRTT came from. Printed because the τ-lag's `τ` is
/// read off it and because the Copa wire-RTT and the app echo are known to
/// disagree by the sender's own reservoir dwell (the #80 battery, arm D) —
/// a reading whose reference is unstated is not a reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SrttSource {
    /// Copa's wire-clocked RTT (`RWM_COPA_WIRE` active).
    Wire,
    /// The application-level ack echo.
    Echo,
}

impl SrttSource {
    fn tag(self) -> &'static str {
        match self {
            SrttSource::Wire => "wire",
            SrttSource::Echo => "echo",
        }
    }
}

#[derive(Default)]
struct RecvPathEta {
    /// Arrivals seen on this path (the bind denominator).
    n: u64,
    /// Arrivals carrying the 0 "no prediction" sentinel.
    zero_n: u64,
    /// The running MIN of `d = (t_arr − send_ts) − eta_rel`. `None` until the
    /// first predicted arrival. Carries the (constant, unknown) clock offset,
    /// which is exactly why only `d − min` is ever printed.
    min_d: Option<i64>,
    /// `ℓ = d − min_d`, µs.
    lat: Hist,
    /// `σ̂_ℓ` — the τ-lag over the same series.
    lat_sig: Tlag,
    /// `τ` for this path, µs, and where its SRTT came from.
    tau_us: u64,
    srtt_src: Option<SrttSource>,
    /// How many arrivals RESET the running minimum — a warm-up witness. A
    /// path whose min is still moving has an `ℓ` distribution biased HIGH,
    /// and this is the count that says so rather than a hidden filter.
    min_resets: u64,
}

/// **THE RECEIVER-SITE ETA GAUGE.** Owned by the receiver task; no engine
/// handle, no shared state, nothing reachable from a decision site.
#[derive(Default)]
pub struct RecvEta {
    paths: BTreeMap<u32, RecvPathEta>,
    total_n: u64,
}

impl RecvEta {
    /// One data arrival. `send_ts_us` / `eta_rel_us` are the batch envelope's
    /// own fields; `arr_us` is the receiver's clock; `tau_us` is the path's
    /// reference lag (RTprop when the receiver has one, its SRTT otherwise)
    /// and `srtt_src` says which clock produced it.
    ///
    /// ALWAYS FED — including the `eta_rel = 0` sentinel, which is counted and
    /// then skipped, so the printed bind fraction is the instrument's own
    /// coverage rather than a number a filter already decided.
    pub fn observe(
        &mut self,
        path_id: u32,
        send_ts_us: u64,
        arr_us: u64,
        eta_rel_us: u32,
        tau_us: u64,
        srtt_src: SrttSource,
    ) {
        self.total_n += 1;
        let p = self.paths.entry(path_id).or_default();
        p.n += 1;
        p.tau_us = tau_us;
        p.srtt_src = Some(srtt_src);
        if eta_rel_us == 0 {
            p.zero_n += 1;
            return;
        }
        let d = arr_us as i64 - send_ts_us as i64 - eta_rel_us as i64;
        let base = match p.min_d {
            Some(m) if m <= d => m,
            _ => {
                p.min_resets += 1;
                p.min_d = Some(d);
                d
            }
        };
        let l = (d - base).max(0) as u64;
        p.lat.add(l);
        p.lat_sig.push(Instant::now(), l, Duration::from_micros(tau_us));
    }

    /// Has this gauge seen ANY arrival — i.e. does it sit at a RECEIVER?
    pub fn is_receiver_site(&self) -> bool {
        self.total_n > 0
    }

    /// `σ̂_ℓ` for one path, µs — the witness's right-hand side.
    pub fn sigma_us(&self, path_id: u32) -> Option<u64> {
        let p = self.paths.get(&path_id)?;
        p.lat_sig.sigma_us(Duration::from_micros(p.tau_us))
    }

    /// The `[ETA] site=receiver` line, on `[SUCC]`'s cadence and under its
    /// gate.
    pub fn line(&self) -> String {
        let mut s = format!("[ETA] site=receiver n={}", self.total_n);
        for (id, p) in &self.paths {
            let tau = Duration::from_micros(p.tau_us);
            let q = |x: f64| opt(p.lat.quantile(x));
            s.push_str(&format!(
                " p{}:n={} bind={} tau_us={} srtt_src={} l_p50={} l_p90={} l_p95={} \
                 l_p99={} l_mx={} sig_us={}/n{} minrst={}",
                id,
                p.lat.n(),
                optf(p.zero_n, p.n),
                p.tau_us,
                p.srtt_src.map_or("-", |x| x.tag()),
                q(0.50),
                q(0.90),
                q(0.95),
                q(0.99),
                if p.lat.n() == 0 { "-".to_string() } else { p.lat.max_us().to_string() },
                opt(p.lat_sig.sigma_us(tau)),
                p.lat_sig.pairs(tau),
                p.min_resets,
            ));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The τ-lag admits a pair only inside `[τ, 2τ]`, and value/count come
    /// from ONE pair-set function so `-` iff `n = 0` holds by construction.
    #[test]
    fn tlag_pairs_only_inside_the_band_and_the_biconditional_holds() {
        let tau = Duration::from_millis(10);
        let mut t = Tlag::default();
        assert_eq!(t.sigma_us(tau), None);
        assert_eq!(t.pairs(tau), 0);
        let t0 = Instant::now();
        // Spacing 1.25 ms = τ/8, the decimation floor: every sample admitted.
        for i in 0..40u32 {
            t.push(t0 + Duration::from_micros(1250 * i as u64), (i as u64) * 100, tau);
        }
        assert_eq!(t.ring_len(), 40);
        let n = t.pairs(tau);
        assert!(n > 0, "a 50 ms series at τ = 10 ms must find pairs");
        assert!(t.sigma_us(tau).is_some());
        // Every admitted lag is in [τ, 2τ]; the series rises 100 µs per
        // 1.25 ms, so a τ-lag difference is 8·100 = 800 and a 2τ one 1600.
        let s = t.sigma_us(tau).unwrap();
        assert!((800..=1600).contains(&s), "σ̂ = {s} outside the band's own range");
        // τ = 0 (no reference) admits nothing — reported, not patched.
        assert_eq!(t.sigma_us(Duration::ZERO), None);
        assert_eq!(t.pairs(Duration::ZERO), 0);
    }

    /// Decimation: at most one sample per `τ/8`, so the ring spans `32·τ` at
    /// any offered rate.
    #[test]
    fn tlag_decimates_at_tau_over_eight() {
        let tau = Duration::from_millis(8);
        let mut t = Tlag::default();
        let t0 = Instant::now();
        for i in 0..1000u64 {
            t.push(t0 + Duration::from_micros(i * 10), i, tau);
        }
        // 10 ms of samples at a 1 ms floor ⇒ ~11 admitted, not 1000.
        assert!(t.ring_len() <= 12, "decimation did not bind: {}", t.ring_len());
    }

    /// The sender's `F̂` is the running MAX of stamped arrival times, and the
    /// zero-sentinel bind fraction is reported rather than filtered.
    #[test]
    fn sender_frontier_is_a_running_max_and_the_sentinel_is_counted() {
        let mut g = SenderEta::default();
        assert!(!g.is_sender_site());
        g.stamp(1, 1_000, 500); // arrival 1500
        g.stamp(1, 1_100, 200); // arrival 1300 — does NOT lower F̂
        g.stamp(2, 1_050, 900); // arrival 1950
        g.stamp(2, 1_060, 0); // the sentinel
        assert!(g.is_sender_site());
        assert_eq!(g.frontier_eta_us(), 1_950);
        let l = g.line();
        assert!(l.starts_with("[ETA] site=sender fhat_us=1950 n=4 zero=0.2500"), "{l}");
        // No ack yet ⇒ every per-path slot renders `-`, never 0.
        assert!(l.contains("p1:n=0/2"), "{l}");
        assert!(l.contains("e_p50=- e_p90=- e_p99=- e_mx=-"), "{l}");
        assert!(l.contains("late=- sig_us=-/n0"), "{l}");
    }

    /// The prediction error is `rtt − (eta + RTprop/2)`, matched by the echo,
    /// and the sentinel batches are excluded from it by construction.
    #[test]
    fn sender_error_is_the_forward_miss_and_the_sentinel_is_excluded() {
        let mut g = SenderEta::default();
        g.stamp(1, 1_000, 5_000);
        g.stamp(1, 2_000, 0);
        // rtt 20 ms, RTprop 10 ms ⇒ predicted 5000 + 5000 = 10 000, e = +10 000.
        g.on_ack(1, 1_000, 20_000, 10_000);
        // The sentinel batch: matched by echo, but excluded.
        g.on_ack(1, 2_000, 20_000, 10_000);
        // An echo that keys nothing: a no-op, never a panic.
        g.on_ack(1, 999_999, 20_000, 10_000);
        g.on_ack(7, 1_000, 20_000, 10_000);
        let l = g.line();
        assert!(l.contains("p1:n=1/2"), "only the predicted batch is a sample: {l}");
        assert!(l.contains("e_mx=10000"), "{l}");
        assert!(l.contains("late=1.0000"), "the realized RTT exceeded the prediction: {l}");
    }

    /// The receiver's lateness is offset-free: an arbitrary constant clock
    /// skew between the two ends must not move `ℓ` at all.
    #[test]
    fn receiver_lateness_is_invariant_to_the_clock_offset() {
        let read = |offset: i64| {
            let mut g = RecvEta::default();
            for (send, owd, eta) in
                [(0u64, 10_000i64, 8_000u32), (1_000, 12_000, 8_000), (2_000, 11_000, 8_000)]
            {
                let arr = (send as i64 + owd + offset) as u64;
                g.observe(1, send, arr, eta, 10_000, SrttSource::Wire);
            }
            g.line()
        };
        let a = read(0);
        let b = read(5_000_000);
        // The whole per-path slot is identical under a 5 s offset.
        let slot = |s: &str| s[s.find(" p1:").unwrap()..].to_string();
        assert_eq!(slot(&a), slot(&b), "a constant clock offset moved ℓ:\n{a}\n{b}");
        // d = owd − eta ⇒ 2000, 4000, 3000; min 2000 ⇒ ℓ = 0, 2000, 1000.
        assert!(a.contains("l_mx=2000"), "{a}");
        assert!(a.contains("p1:n=3 bind=0.0000"), "{a}");
    }

    /// The 0 sentinel is COUNTED and skipped — an absent prediction and a
    /// zero lateness are different readings.
    #[test]
    fn receiver_counts_the_sentinel_as_bind_and_never_as_a_sample() {
        let mut g = RecvEta::default();
        assert!(!g.is_receiver_site());
        g.observe(1, 0, 10_000, 0, 10_000, SrttSource::Echo);
        g.observe(1, 0, 10_000, 8_000, 10_000, SrttSource::Echo);
        assert!(g.is_receiver_site());
        let l = g.line();
        assert!(l.starts_with("[ETA] site=receiver n=2"), "{l}");
        assert!(l.contains("p1:n=1 bind=0.5000"), "{l}");
        assert!(l.contains("srtt_src=echo"), "{l}");
        // A path with nothing but sentinels renders `-` on every quantile.
        let mut e = RecvEta::default();
        e.observe(3, 0, 1, 0, 0, SrttSource::Wire);
        let el = e.line();
        assert!(el.contains("p3:n=0 bind=1.0000 tau_us=0 srtt_src=wire"), "{el}");
        assert!(el.contains("l_p50=- l_p90=- l_p95=- l_p99=- l_mx=- sig_us=-/n0"), "{el}");
    }
}
