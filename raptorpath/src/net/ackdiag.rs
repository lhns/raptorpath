//! The ACK-CADENCE GAUGE (`RWM_ACKDIAG`, default OFF, observation only).
//!
//! WHY IT EXISTS — the matrix row it discharges.
//!
//! `docs/goal-gate.md`'s PIPELINE VERIFICATION MATRIX, row 21 ("Receiver —
//! ack/echo generation and cadence UNDER ACK-MERGE"), records the state of
//! the art as: *the ack-emission PREDICATE is pinned three ways and the
//! counter-diff re-homing four ways, but **the STREAM SHAPE has no instrument
//! of any kind, anywhere**.* Every consumer of that shape therefore had to
//! INVENT it, and the inventions disagree with the wire by one to three
//! orders of magnitude:
//!
//!   * `store_cap_sf_bench::sf_derived_overread_from_ack_batching` sweeps an
//!     ack period of 0.25–10 ms and lands at a realized over-read of ×24–2400,
//!     against the wire's measured ×4.6–7.4 band;
//!   * `honest_inputs_bench` assumes 5 ms;
//!   * `tests/common/recovery_model.rs:319` models a 2 ms periodic `Ev::Ack`
//!     — the PRE-merge cadence.
//!
//! The "SF Accounting Axis" investigation (2026-08-11) failed its level gate
//! at c7 and named this gauge as its own fallback, for exactly that reason:
//! *before a component branch is opened to chase a suspect, check whether the
//! input it depends on has ever been measured.* This module measures it.
//!
//! WHAT IT RECORDS — four readouts, all sender-side, all per path.
//!
//!   1. **WindowAck arrival spacing.** The inter-arrival Δt of `WindowAck`
//!      control datagrams as the SENDER sees them, in µs: p50/p90/p99 and the
//!      count. This is the cadence every one of the three benches above
//!      invented.
//!   2. **Delivered-count deltas.** `d_received` per ack — the `(cum_received
//!      − cursor)` diff `PathState::ack_merge_counter_delta` returns, i.e.
//!      literally the `count` the rate sampler is fed — as p50/p90/max, plus
//!      the ZERO-DELTA fraction. A zero delta is the sentinel/stale class
//!      (`cum_received == 0`, or a duplicate/reordered ack whose counters have
//!      not advanced); it costs a datagram and moves no estimator.
//!   3. **The realized rate-sampler input.** For every ACCEPTED
//!      `CopaState::record_delivery` sample (the ones that clear its 1 ms
//!      `elapsed` floor), the sample rate `Δdelivered/Δt_ack`, normalized at
//!      print time by the window's OWN long-run delivered rate
//!      `Σcount / Δt_window` — that ratio IS the over-read x, per sample,
//!      with no invented input anywhere in it. Beside it the ledger's own
//!      formula, `x_anchor = copa_bdp_anchor() / (rate_lr · RTprop)`, which
//!      reduces to `max_bw / rate_lr` and is the number the store-cap Σ and
//!      the cwnd anchor floor actually consume.
//!   4. **The repair-counting reconciliation.** Whether repair/retransmit
//!      symbols enter the receiver's expected/received counters. The engine's
//!      counters come from `PathBatchTracker::record_batch(batch_seq,
//!      batch.symbols.len())` (`net/mod.rs:7129`, fed at `receiver.rs:1109`),
//!      which counts SYMBOLS IN AN ARRIVING BATCH without ever looking at
//!      `symbol.is_repair` — so the READ says repairs are counted. The gauge
//!      MEASURES it instead of reading it: `recon[…]` prints, per path,
//!      `sent` (the always-on `PathStats::symbols_sent`, incremented at every
//!      wire handoff — source, repair and retransmit alike), `crecv`
//!      (Σ `d_received`) and `cexp` (Σ `d_expected`), with the two ratios.
//!      `crecv/sent ≈ 1` is repairs-ARE-counted; `crecv/sent ≈ src/(src+rep)`
//!      would be repairs-are-NOT-counted. `cexp/crecv` is the gap-estimator's
//!      inflation (`record_batch` charges `gap × received` across a
//!      batch-seq gap).
//!
//! COST AND BEHAVIOUR.
//!
//! Zero cost with the gate off: [`gauge`] is a `OnceLock<Option<…>>` resolved
//! once at first touch (the `stall_witness` pattern), so every feed site is a
//! null check that never allocates, never locks and never reads the clock.
//! Behaviour-neutral with the gate on: the gauge OWNS all of its state — it
//! reads no engine state on the feed paths and writes none anywhere — so no
//! emission, admission, pacing, recovery or estimator decision can observe
//! it. `ackdiag_is_observation_only` pins that structurally.
//!
//! LOCK ORDER. The feed sites are called while the caller holds the scheduler
//! lock, so the gauge lock is INNER: `scheduler → gauge`. The report keeps
//! that order by taking its scheduler snapshot and RELEASING it before it
//! touches the gauge, so the gauge lock is never held while acquiring the
//! scheduler lock and no cycle exists.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::monitor::stats::SharedStats;
use crate::scheduler::Scheduler;

/// Print cadence, DEFAULT — the ~2 s the dispatch specified, ~8× the `[DIAG]`
/// line's 250 ms. Longer windows are deliberate here: these readouts are
/// DISTRIBUTIONS, and a p99 over a 250 ms window of a few hundred acks is
/// noise.
///
/// **THE DEFAULT IS UNCHANGED AND THAT IS LOAD-BEARING.** Every committed
/// `[ACKDIAG]` ledger in `docs/l1-raw/` was captured at 2 s, and the window is
/// the unit of every series read off them (the c7/c8 correlation estimates are
/// correlations OF 2 s WINDOWS). A driver that changes it is changing the
/// measurand, so it must say so explicitly — hence an override rather than a
/// new default. See [`window_us`].
pub const ACKDIAG_WINDOW_US: u64 = 2_000_000;

/// `RWM_ACKDIAG_WINDOW_US` clamp, low end. 50 ms. Below this the window
/// stops being a distribution and becomes a sample: at the loopback ceiling a
/// 50 ms window holds ~500 acks, which is already thin for a p99, and the
/// per-window fixed cost (a sort of three series plus a format) starts to be
/// a measurable share of the sender loop.
pub const ACKDIAG_WINDOW_US_MIN: u64 = 50_000;

/// `RWM_ACKDIAG_WINDOW_US` clamp, high end. 60 s — longer than any invocation
/// this harness runs, so a value above it can only be a typo, and a typo that
/// silently produced ZERO reports would look exactly like a dead gauge.
pub const ACKDIAG_WINDOW_US_MAX: u64 = 60_000_000;

/// Resolve the window from a raw env string. Pure, so the clamp and the
/// rejection rules are testable without touching the process environment
/// (`window_us` caches, and a cached `OnceLock` cannot be re-resolved by a
/// second test in the same binary).
///
/// Returns the DEFAULT for: unset, empty, unparseable, or zero. A garbage
/// value must not be able to silently disable the instrument — the caller
/// echoes the RESOLVED number, so a driver that mistyped the override reads
/// `2000000` in the `[GATES]` line and knows its arm did not take.
pub fn resolve_window_us(raw: Option<&str>) -> u64 {
    match raw.map(str::trim) {
        None | Some("") => ACKDIAG_WINDOW_US,
        Some(s) => match s.parse::<u64>() {
            Ok(0) | Err(_) => ACKDIAG_WINDOW_US,
            Ok(v) => v.clamp(ACKDIAG_WINDOW_US_MIN, ACKDIAG_WINDOW_US_MAX),
        },
    }
}

/// The ACTIVE print cadence, µs — [`ACKDIAG_WINDOW_US`] unless
/// `RWM_ACKDIAG_WINDOW_US` overrides it.
///
/// WHY THE OVERRIDE EXISTS. goal-gate "Eppen's Condition at c8" NEEDS-MORE 1:
/// the 2 s window against a 9–11 s invocation yields FOUR window pairs per
/// rep, which supports an ordering test between two cells and nothing finer.
/// The c9 pre-registration (C9-1 … C9-4) needs SIX pairwise correlations at a
/// quad, and four windows per rep cannot carry them — the 250 ms window is
/// recorded there as a BLOCKING DEPENDENCY, not a nice-to-have. This is that
/// dependency, and it is one env read.
///
/// Resolved ONCE (the gauge's own `OnceLock` discipline) so the cadence cannot
/// change mid-run and split a ledger's windows into two populations.
/// Observation-only, exactly like the rest of this module: the window governs
/// when a line is PRINTED and nothing else.
pub fn window_us() -> u64 {
    static W: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *W.get_or_init(|| resolve_window_us(std::env::var("RWM_ACKDIAG_WINDOW_US").ok().as_deref()))
}

/// Per-window sample cap, per path, per series. At the loopback ceiling
/// (~11–12 k sym/s, one ack per data message) a 2 s window can produce ~20 k
/// acks; the cap bounds the gauge's memory at ~3 × 128 KB per path and the
/// overflow count is PRINTED (`ov=`) so a truncated window is never mistaken
/// for a complete one.
const SAMPLE_CAP: usize = 32_768;

/// One path's ack-stream statistics. Window-scoped series (reset by the
/// report) plus run-cumulative totals (never reset — the end-of-run line is
/// the accounting read, exactly like the `[DIAG]` `cum=` totals).
#[derive(Default)]
struct PathAck {
    // ── readout 1: arrival spacing ───────────────────────────────────────
    /// Previous WindowAck arrival, µs on the gauge's monotonic epoch.
    last_arrival_us: u64,
    /// Inter-arrival gaps this window (µs).
    gaps_us: Vec<u32>,
    /// WindowAcks seen this window / over the run.
    acks_win: u64,
    acks_total: u64,

    // ── readout 2: delivered-count deltas ────────────────────────────────
    /// `d_received` per ack this window (symbols).
    deltas: Vec<u32>,
    /// Acks whose counter diff was (0, 0) — sentinel/stale.
    zero_win: u64,
    zero_total: u64,
    /// Σ `d_received` / Σ `d_expected` over the run (readout 4's `crecv`/`cexp`).
    d_recv_total: u64,
    d_exp_total: u64,
    /// Σ `d_received` this window — the cross-check against `rd_count_win`.
    d_recv_win: u64,

    // ── readout 3: the realized rate-sampler input ───────────────────────
    /// Accepted `record_delivery` sample rates this window (sym/s).
    rates: Vec<f32>,
    /// Accepted / rejected (sub-1 ms `elapsed`) samples this window.
    rd_acc_win: u64,
    rd_rej_win: u64,
    rd_acc_total: u64,
    rd_rej_total: u64,
    /// Σ of the `count` argument over ALL `record_delivery` calls this window
    /// — the delivered quantity the sampler itself saw. Used as the over-read
    /// DENOMINATOR, so the normalizer and the numerator count the same thing
    /// whichever ack arm drove the sampler.
    rd_count_win: u64,

    /// Samples dropped by [`SAMPLE_CAP`] this window, any series.
    overflow: u64,
    /// Window start, µs on the gauge epoch.
    win_start_us: u64,
}

impl PathAck {
    fn new(now_us: u64) -> Self {
        Self {
            win_start_us: now_us,
            ..Default::default()
        }
    }
}

/// Push with the cap, counting what the cap refused.
fn push_capped<T>(v: &mut Vec<T>, x: T, overflow: &mut u64) {
    if v.len() < SAMPLE_CAP {
        v.push(x);
    } else {
        *overflow += 1;
    }
}

/// Nearest-rank quantile of an UNSORTED slice copy. `q ∈ [0, 1]`.
/// Returns 0 for an empty series (and the caller prints `n=0` beside it, so a
/// zero from emptiness is never confusable with a measured zero).
fn quantile_u32(sorted: &[u32], q: f64) -> u32 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Nearest-rank quantile over `f32` samples (the rate series).
fn quantile_f32(sorted: &[f32], q: f64) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// What one path's report line needs from the ENGINE, snapshotted under the
/// scheduler lock and passed in after it is released (see the lock-order note
/// in the module header).
#[derive(Debug, Clone, Copy)]
pub struct PathSnapshot {
    pub path_id: u32,
    /// `copa_bdp_anchor()` — `max_bw · RTprop` in symbols, the quantity the
    /// store-cap Σ and the cwnd anchor floor consume. 0 = anchor cold.
    pub anchor_syms: f64,
    /// The windowed-min RTprop, seconds. 0 = no sample yet.
    pub rtprop_s: f64,
    /// `PathStats::symbols_sent` — every wire handoff on this path (source,
    /// repair and retransmit alike). The reconciliation's `sent`.
    pub sent_total: u64,
    /// The cumulative WindowAck frontier — DELIVERED SOURCE symbols, the same
    /// `window_ack_seq` the `[DIAG]` line computes its goodput from. The
    /// reconciliation's `srcack`, and the DISCRIMINATOR of readout 4: it
    /// counts source symbols ONLY, so `crecv ≈ srcack` would mean the
    /// receiver's counters exclude repair/retransmit symbols and
    /// `crecv > srcack` means they do not. CONNECTION-wide, not per-path (the
    /// frontier is one sequence space), so at N ≥ 2 it is comparable against
    /// Σ`crecv` rather than against one path's.
    pub src_ack: u64,
}

/// One path's run-cumulative ack-stream totals (see [`AckCadenceGauge::totals`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AckTotals {
    /// WindowAcks the sender received on this path.
    pub acks: u64,
    /// Of which carried a `(0, 0)` counter diff (sentinel or stale).
    pub zero_acks: u64,
    /// Σ `d_received` — symbols the receiver's per-path batch tracker counted
    /// as ARRIVED (source, repair and retransmit alike — see readout 4).
    pub d_recv: u64,
    /// Σ `d_expected` — the tracker's gap-inflated estimate of symbols sent.
    pub d_exp: u64,
    /// `record_delivery` calls that cleared the 1 ms `elapsed` floor.
    pub rd_accepted: u64,
    /// `record_delivery` calls the floor rejected.
    pub rd_rejected: u64,
}

/// The gauge. One per process (the ack stream is a per-process phenomenon and
/// the feed sites are static functions), keyed by path inside.
pub struct AckCadenceGauge {
    epoch: Instant,
    /// Last report stamp, µs on `epoch` (0 = never).
    last_report_us: AtomicU64,
    /// The delivered-SOURCE frontier as of the last report — readout 4's
    /// discriminator, kept so a test can assert on it without scraping the
    /// gauge's own stderr. 0 = no report has fired yet.
    last_src_ack: AtomicU64,
    /// Σ`d_received` over all paths, sampled at the SAME instant as
    /// [`Self::last_src_ack`]. The pair must be read together: the frontier
    /// keeps advancing after the last report, so an end-of-run `crecv` paired
    /// with a mid-run `srcack` inflates the ratio.
    last_crecv: AtomicU64,
    /// The highest delivered-SOURCE frontier any engine in this process has
    /// presented — the SINGLE-ENGINE SCOPE latch (see [`maybe_report`]).
    frontier_hi: AtomicU64,
    paths: parking_lot::Mutex<HashMap<u32, PathAck>>,
}

impl Default for AckCadenceGauge {
    fn default() -> Self {
        Self::new()
    }
}

impl AckCadenceGauge {
    pub fn new() -> Self {
        Self {
            epoch: Instant::now(),
            last_report_us: AtomicU64::new(0),
            last_src_ack: AtomicU64::new(0),
            last_crecv: AtomicU64::new(0),
            frontier_hi: AtomicU64::new(0),
            paths: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    /// µs on the gauge's own monotonic epoch. Deliberately NOT `net::now_us`
    /// (a `SystemTime` read): inter-arrival spacing is the measurement, and a
    /// wall clock can step.
    pub fn now_us(&self) -> u64 {
        self.epoch.elapsed().as_micros() as u64
    }

    /// **Readouts 1 + 2 + 4's feed.** One inbound `WindowAck` on `path_id`,
    /// with the counter diff the sender derived from it. Call for EVERY
    /// WindowAck arrival, including the ones whose diff is `(0, 0)` — that
    /// class is readout 2's zero-delta fraction and dropping it would make
    /// the gauge report a cadence the sender does not have.
    pub fn note_ack(&self, path_id: u32, d_expected: u32, d_received: u32) {
        self.note_ack_at(self.now_us(), path_id, d_expected, d_received)
    }

    /// [`Self::note_ack`] at an explicit gauge-epoch stamp (tests).
    pub fn note_ack_at(&self, now_us: u64, path_id: u32, d_expected: u32, d_received: u32) {
        let mut m = self.paths.lock();
        let p = m.entry(path_id).or_insert_with(|| PathAck::new(now_us));
        if p.last_arrival_us > 0 {
            let gap = now_us.saturating_sub(p.last_arrival_us).min(u32::MAX as u64) as u32;
            push_capped(&mut p.gaps_us, gap, &mut p.overflow);
        }
        p.last_arrival_us = now_us.max(1);
        p.acks_win += 1;
        p.acks_total += 1;
        if d_expected == 0 && d_received == 0 {
            p.zero_win += 1;
            p.zero_total += 1;
        } else {
            push_capped(&mut p.deltas, d_received, &mut p.overflow);
        }
        p.d_recv_total += d_received as u64;
        p.d_exp_total += d_expected as u64;
        p.d_recv_win += d_received as u64;
    }

    /// **Readout 3's feed.** One `CopaState::record_delivery` call:
    /// `count` symbols offered, `rate` sym/s produced, `accepted` = the call
    /// cleared the sampler's 1 ms `elapsed` floor and pushed a windowed-max
    /// sample. Rejected calls carry no rate (the sampler returns `max_bw`
    /// unchanged) and are counted, not sampled.
    pub fn note_rate_sample(&self, path_id: u32, count: u32, rate: f64, accepted: bool) {
        let now = self.now_us();
        let mut m = self.paths.lock();
        let p = m.entry(path_id).or_insert_with(|| PathAck::new(now));
        p.rd_count_win += count as u64;
        if accepted {
            p.rd_acc_win += 1;
            p.rd_acc_total += 1;
            push_capped(&mut p.rates, rate as f32, &mut p.overflow);
        } else {
            p.rd_rej_win += 1;
            p.rd_rej_total += 1;
        }
    }

    /// True when a report window has elapsed; stamps the new window start.
    /// One relaxed atomic per call — the whole per-iteration cost of the gate
    /// once it is on.
    fn report_due(&self, now_us: u64) -> bool {
        let last = self.last_report_us.load(Ordering::Relaxed);
        if last == 0 {
            self.last_report_us.store(now_us.max(1), Ordering::Relaxed);
            return false;
        }
        if now_us.saturating_sub(last) < window_us() {
            return false;
        }
        self.last_report_us.store(now_us.max(1), Ordering::Relaxed);
        true
    }

    /// Render one path's line and RESET its window series. `None` when the
    /// path produced no ack and no rate sample this window — a path that has
    /// nothing to say prints nothing, so a zero on the line always means a
    /// measured zero.
    ///
    /// UNITS, field by field:
    ///   * `acks=<n>/z=<n>(<pct>%)` — WindowAcks this window / of which
    ///     zero-delta (sentinel or stale), and that fraction.
    ///   * `gap_us[p50 p90 p99 n]` — inter-arrival Δt, MICROSECONDS.
    ///   * `drecv[p50 p90 max n]` — `d_received` per NON-ZERO ack, SYMBOLS.
    ///   * `rd[acc rej cnt]` — `record_delivery` calls accepted / rejected
    ///     (sub-1 ms) and Σ`count` offered, SYMBOLS for the last.
    ///   * `rate_lr` — Σ`count`/window, SYM/S: the long-run delivered rate the
    ///     sampler itself saw, and the over-read denominator.
    ///   * `x[p50 p90 p99]` — per-accepted-sample `rate/rate_lr`,
    ///     DIMENSIONLESS. `-` when `rate_lr` is 0 (no delivery this window).
    ///   * `xanchor` — `anchor_syms/(rate_lr·rtprop_s)` = `max_bw/rate_lr`,
    ///     DIMENSIONLESS: the ledger's own over-read formula, on the quantity
    ///     the store-cap Σ and the cwnd floor consume. `-` when the anchor is
    ///     cold or RTprop has no sample.
    ///   * `anchor=<sym> rtprop=<ms>` — the two inputs of `xanchor`.
    ///   * `recon[sent crecv cexp srcack cr/s ce/cr cr/sa]` — readout 4,
    ///     CUMULATIVE symbols and three dimensionless ratios. `cr/sa` is the
    ///     repair-counting discriminator: `> 1` means the receiver's counters
    ///     include repair and retransmit symbols, `≈ 1` means they count
    ///     source only.
    ///   * `ov=<n>` — samples the per-window cap refused (0 = complete).
    pub fn report_line(&self, snap: PathSnapshot, now_us: u64) -> Option<String> {
        let mut m = self.paths.lock();
        let p = m.get_mut(&snap.path_id)?;
        if p.acks_win == 0 && p.rd_acc_win == 0 && p.rd_rej_win == 0 {
            return None;
        }
        let win_s = now_us.saturating_sub(p.win_start_us) as f64 / 1e6;
        let mut gaps = std::mem::take(&mut p.gaps_us);
        gaps.sort_unstable();
        let mut deltas = std::mem::take(&mut p.deltas);
        deltas.sort_unstable();
        let mut rates = std::mem::take(&mut p.rates);
        rates.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // The over-read denominator: the delivered rate the SAMPLER saw, over
        // this window. Zero (no delivery, or the window has no duration) makes
        // every x undefined, and the line says so rather than dividing.
        let rate_lr = if win_s > 0.0 {
            p.rd_count_win as f64 / win_s
        } else {
            0.0
        };
        let xq = |q: f64| -> String {
            if rate_lr > 0.0 && !rates.is_empty() {
                format!("{:.2}", quantile_f32(&rates, q) as f64 / rate_lr)
            } else {
                "-".to_string()
            }
        };
        let xanchor = if rate_lr > 0.0 && snap.anchor_syms > 0.0 && snap.rtprop_s > 0.0 {
            format!("{:.2}", snap.anchor_syms / (rate_lr * snap.rtprop_s))
        } else {
            "-".to_string()
        };
        let zpct = p.zero_win as f64 * 100.0 / p.acks_win.max(1) as f64;
        let cr_over_s = if snap.sent_total > 0 {
            format!("{:.3}", p.d_recv_total as f64 / snap.sent_total as f64)
        } else {
            "-".to_string()
        };
        let ce_over_cr = if p.d_recv_total > 0 {
            format!("{:.3}", p.d_exp_total as f64 / p.d_recv_total as f64)
        } else {
            "-".to_string()
        };
        let cr_over_sa = if snap.src_ack > 0 {
            format!("{:.3}", p.d_recv_total as f64 / snap.src_ack as f64)
        } else {
            "-".to_string()
        };
        let line = format!(
            "p{} win={:.2}s acks={}/z={}({:.1}%) gap_us[p50={} p90={} p99={} n={}] \
             drecv[p50={} p90={} max={} n={} sum={}] rd[acc={} rej={} cnt={}] \
             rate_lr={:.0}sym/s x[p50={} p90={} p99={}] xanchor={} anchor={:.0}sym \
             rtprop={:.2}ms recon[sent={} crecv={} cexp={} srcack={} cr/s={} ce/cr={} \
             cr/sa={}] ov={}",
            snap.path_id,
            win_s,
            p.acks_win,
            p.zero_win,
            zpct,
            quantile_u32(&gaps, 0.50),
            quantile_u32(&gaps, 0.90),
            quantile_u32(&gaps, 0.99),
            gaps.len(),
            quantile_u32(&deltas, 0.50),
            quantile_u32(&deltas, 0.90),
            deltas.last().copied().unwrap_or(0),
            deltas.len(),
            p.d_recv_win,
            p.rd_acc_win,
            p.rd_rej_win,
            p.rd_count_win,
            rate_lr,
            xq(0.50),
            xq(0.90),
            xq(0.99),
            xanchor,
            snap.anchor_syms,
            snap.rtprop_s * 1000.0,
            snap.sent_total,
            p.d_recv_total,
            p.d_exp_total,
            snap.src_ack,
            cr_over_s,
            ce_over_cr,
            cr_over_sa,
            p.overflow,
        );
        // Reset the WINDOW series only. The cumulative totals and the
        // arrival cursor survive, so the next window's first gap is measured
        // against this window's last ack rather than being lost.
        p.acks_win = 0;
        p.zero_win = 0;
        p.d_recv_win = 0;
        p.rd_acc_win = 0;
        p.rd_rej_win = 0;
        p.rd_count_win = 0;
        p.overflow = 0;
        p.win_start_us = now_us;
        Some(line)
    }

    /// One path's RUN-CUMULATIVE totals — the same numbers the `recon[…]`
    /// and `rd[…]` fields print, in machine-readable form so a loopback can
    /// ASSERT on the gauge instead of scraping its own stderr.
    pub fn totals(&self, path_id: u32) -> Option<AckTotals> {
        let m = self.paths.lock();
        let p = m.get(&path_id)?;
        Some(AckTotals {
            acks: p.acks_total,
            zero_acks: p.zero_total,
            d_recv: p.d_recv_total,
            d_exp: p.d_exp_total,
            rd_accepted: p.rd_acc_total,
            rd_rejected: p.rd_rej_total,
        })
    }

    /// Readout 4's CONTEMPORANEOUS pair as of the last `[ACKDIAG]` report:
    /// `(Σ d_received over all paths, delivered-SOURCE frontier)`. `(0, 0)`
    /// when no report has fired. Sampled together on purpose — the frontier
    /// advances after a report, so pairing an end-of-run `crecv` with a
    /// mid-run `srcack` inflates the ratio and would let the discriminator
    /// pass for the wrong reason.
    pub fn last_recon(&self) -> (u64, u64) {
        (
            self.last_crecv.load(Ordering::Relaxed),
            self.last_src_ack.load(Ordering::Relaxed),
        )
    }

    /// The path ids the gauge has seen, sorted — so the report covers a path
    /// that has since left `active_paths()` and the line ordering is stable
    /// for scraping.
    pub fn known_paths(&self) -> Vec<u32> {
        let mut v: Vec<u32> = self.paths.lock().keys().copied().collect();
        v.sort_unstable();
        v
    }
}

/// Process-global gauge, resolved once at first touch. `None` — and therefore
/// no state, no lock and no clock read at any feed site — unless
/// `RWM_ACKDIAG=1`.
///
/// `RWM_ACKDIAG` (default OFF, DIAG-surface, ADR-0052 class): the ack-cadence
/// gauge of matrix row 21. Independent of `RWM_DIAG` on purpose — this
/// instrument is meant to be runnable on an arm that is NOT paying for the
/// 250 ms `[DIAG]` report, and its own `[ACKDIAG]` line is separately
/// scrapeable.
pub fn gauge() -> Option<&'static AckCadenceGauge> {
    static G: std::sync::OnceLock<Option<AckCadenceGauge>> = std::sync::OnceLock::new();
    G.get_or_init(|| {
        if crate::config::env_flag("RWM_ACKDIAG", false) {
            Some(AckCadenceGauge::new())
        } else {
            None
        }
    })
    .as_ref()
}

/// Emit the `[ACKDIAG]` report if a window has elapsed. Called once per
/// sender-loop iteration under the caller's `if pol.ackdiag_on` guard, so the
/// shipped path pays nothing.
///
/// Takes the scheduler lock for the SNAPSHOT ONLY and releases it before
/// touching the gauge — the lock order the feed sites establish (see the
/// module header).
///
/// SINGLE-ENGINE SCOPE. The gauge is process-global (its feed sites are
/// static functions on the ack path and in `CopaState`), but `stats` and
/// `window_ack_seq` are per-ENGINE handles. The shipped binary and every L1
/// driver run one engine per process, where that distinction does not exist.
/// The in-process loopback tests run TWO — a bulk sender and its peer, whose
/// own reverse stream is a trickle — and pairing the merged per-path series
/// with the TRICKLE engine's `sent`/`srcack` would produce a nonsense
/// reconciliation. So the report binds to the engine with the LEADING source
/// frontier: an engine whose frontier is behind the highest seen returns
/// without consuming the window. In a single-engine process this is a no-op
/// (`src_ack` is always the max), so it costs the shipped configuration one
/// relaxed `fetch_max`.
pub(crate) fn maybe_report(
    scheduler: &Arc<parking_lot::Mutex<Scheduler>>,
    stats: &Arc<SharedStats>,
    window_ack_seq: &Arc<AtomicU64>,
) {
    let Some(g) = gauge() else { return };
    let src_ack = window_ack_seq.load(Ordering::Relaxed);
    let hi = g.frontier_hi.fetch_max(src_ack, Ordering::Relaxed).max(src_ack);
    if src_ack < hi {
        return;
    }
    let now = g.now_us();
    if !g.report_due(now) {
        return;
    }
    let ids = g.known_paths();
    let snaps: Vec<PathSnapshot> = {
        let sched = scheduler.lock();
        ids.iter()
            .map(|id| {
                let (anchor_syms, rtprop_s) = sched
                    .path(*id)
                    .map(|p| {
                        (
                            p.copa_bdp_anchor().unwrap_or(0.0),
                            p.min_rtt().map(|d| d.as_secs_f64()).unwrap_or(0.0),
                        )
                    })
                    .unwrap_or((0.0, 0.0));
                PathSnapshot {
                    path_id: *id,
                    anchor_syms,
                    rtprop_s,
                    sent_total: 0,
                    src_ack: 0,
                }
            })
            .collect()
    };
    // Readout 4's pair, sampled together (see `last_recon`).
    let crecv: u64 = g.paths.lock().values().map(|p| p.d_recv_total).sum();
    g.last_src_ack.store(src_ack, Ordering::Relaxed);
    g.last_crecv.store(crecv, Ordering::Relaxed);
    for mut s in snaps {
        s.src_ack = src_ack;
        s.sent_total = stats
            .path(s.path_id)
            .map(|ps| ps.symbols_sent.load(Ordering::Relaxed))
            .unwrap_or(0);
        if let Some(line) = g.report_line(s, now) {
            eprintln!("[ACKDIAG] {line}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(id: u32) -> PathSnapshot {
        PathSnapshot {
            path_id: id,
            anchor_syms: 0.0,
            rtprop_s: 0.0,
            sent_total: 0,
            src_ack: 0,
        }
    }

    /// **Readout 1, ABSOLUTE.** A known arrival train produces exactly its own
    /// spacing quantiles — nearest-rank, in µs, with n = arrivals − 1 (the
    /// first arrival opens the series, it does not close a gap).
    #[test]
    fn arrival_spacing_quantiles_are_the_injected_train() {
        let g = AckCadenceGauge::new();
        // Ten arrivals 1 ms apart, then one 50 ms late: gaps are
        // [1000 ×9, 50000], so p50 = 1000 and the max/p99 = 50000.
        let mut t = 0u64;
        g.note_ack_at(t, 0, 1, 1);
        for _ in 0..9 {
            t += 1_000;
            g.note_ack_at(t, 0, 1, 1);
        }
        t += 50_000;
        g.note_ack_at(t, 0, 1, 1);
        let line = g.report_line(snap(0), t).expect("path 0 reported");
        assert!(
            line.contains("gap_us[p50=1000 p90=1000 p99=50000 n=10]"),
            "spacing quantiles are not the injected train: {line}"
        );
        assert!(line.contains("acks=11/z=0(0.0%)"), "{line}");
    }

    /// **Readout 2, ABSOLUTE.** The zero-delta class is counted, NOT sampled:
    /// a `(0, 0)` ack raises `z=` and must not enter the `drecv` series (it
    /// would drag every quantile toward zero and make a stale-ack storm look
    /// like a low-delivery cell).
    #[test]
    fn zero_delta_acks_are_counted_and_excluded_from_the_delta_series() {
        let g = AckCadenceGauge::new();
        let mut t = 0u64;
        for i in 0..10u32 {
            t += 1_000;
            // Five real acks of 4 symbols, five sentinels.
            if i % 2 == 0 {
                g.note_ack_at(t, 0, 4, 4);
            } else {
                g.note_ack_at(t, 0, 0, 0);
            }
        }
        let line = g.report_line(snap(0), t).expect("path 0 reported");
        assert!(line.contains("acks=10/z=5(50.0%)"), "{line}");
        assert!(
            line.contains("drecv[p50=4 p90=4 max=4 n=5 sum=20]"),
            "the delta series must hold the five NON-zero acks only: {line}"
        );
    }

    /// **Readout 3, ABSOLUTE.** The over-read is the sample rate over the
    /// window's own long-run rate, and nothing else. Feed 1000 sym over a
    /// 1 s window (⇒ `rate_lr` = 1000 sym/s) with accepted samples at 1000,
    /// 5000 and 10000 sym/s: x must read exactly 1.00 / 5.00 / 10.00.
    #[test]
    fn realized_overread_is_the_sample_rate_over_the_windows_own_long_run_rate() {
        let g = AckCadenceGauge::new();
        // Three calls of 200 sym each are accepted; 400 more arrive on
        // rejected (sub-1 ms) calls, so `count` sums to 1000 either way —
        // the denominator must see the delivery the SAMPLER saw, accepted or
        // not, or the ratio is inflated by the rejection rate.
        g.note_rate_sample(0, 200, 1_000.0, true);
        g.note_rate_sample(0, 200, 5_000.0, true);
        g.note_rate_sample(0, 200, 10_000.0, true);
        g.note_rate_sample(0, 400, 0.0, false);
        // Report at exactly 1 s of gauge-epoch window.
        let mut s = snap(0);
        {
            let mut m = g.paths.lock();
            m.get_mut(&0).unwrap().win_start_us = 0;
        }
        s.anchor_syms = 500.0;
        s.rtprop_s = 0.100; // ⇒ xanchor = 500 / (1000 · 0.1) = 5.00
        let line = g.report_line(s, 1_000_000).expect("path 0 reported");
        assert!(line.contains("rate_lr=1000sym/s"), "{line}");
        assert!(
            line.contains("x[p50=5.00 p90=10.00 p99=10.00]"),
            "per-sample over-read must be rate/rate_lr exactly: {line}"
        );
        assert!(
            line.contains("xanchor=5.00"),
            "xanchor must be the ledger's anchor/(rate·RTprop): {line}"
        );
        assert!(line.contains("rd[acc=3 rej=1 cnt=1000]"), "{line}");
    }

    /// **Readout 3's guard.** With no delivery in the window there is no
    /// denominator, and the gauge must print `-` rather than a divide — a
    /// fabricated over-read is exactly the failure this instrument exists to
    /// end.
    #[test]
    fn overread_is_undefined_not_invented_when_the_window_delivered_nothing() {
        let g = AckCadenceGauge::new();
        g.note_ack_at(0, 0, 0, 0);
        g.note_ack_at(1_000, 0, 0, 0);
        let line = g.report_line(snap(0), 1_000_000).expect("path 0 reported");
        assert!(line.contains("rate_lr=0sym/s"), "{line}");
        assert!(line.contains("x[p50=- p90=- p99=-]"), "{line}");
        assert!(line.contains("xanchor=-"), "{line}");
    }

    /// **Readout 4, ABSOLUTE.** The reconciliation ratios are arithmetic over
    /// cumulative counters, with no constant anywhere: `cr/s` = Σd_received /
    /// symbols_sent and `ce/cr` = Σd_expected / Σd_received.
    #[test]
    fn reconciliation_ratios_are_arithmetic_over_the_cumulative_counters() {
        let g = AckCadenceGauge::new();
        // 100 acks × (expected 11, received 10) = 1100 / 1000.
        let mut t = 0u64;
        for _ in 0..100 {
            t += 1_000;
            g.note_ack_at(t, 0, 11, 10);
        }
        let mut s = snap(0);
        s.sent_total = 1_000; // every wire symbol acked ⇒ cr/s = 1.000
        // 800 delivered SOURCE symbols against 1000 counted arrivals ⇒
        // cr/sa = 1.250: the counters hold 200 symbols the source frontier
        // does not, which is exactly the repair/retransmit signature.
        s.src_ack = 800;
        let line = g.report_line(s, t).expect("path 0 reported");
        assert!(
            line.contains(
                "recon[sent=1000 crecv=1000 cexp=1100 srcack=800 cr/s=1.000 \
                 ce/cr=1.100 cr/sa=1.250]"
            ),
            "{line}"
        );
    }

    /// The cumulative totals SURVIVE a window boundary and the window series
    /// do not — the property that makes the last line of a run the accounting
    /// read while each line is still a distribution over its own window.
    #[test]
    fn windows_reset_the_series_and_carry_the_totals() {
        let g = AckCadenceGauge::new();
        let mut t = 0u64;
        for _ in 0..5 {
            t += 1_000;
            g.note_ack_at(t, 0, 2, 2);
        }
        let first = g.report_line(snap(0), t).expect("first window");
        assert!(first.contains("acks=5/") && first.contains("crecv=10"), "{first}");
        for _ in 0..5 {
            t += 1_000;
            g.note_ack_at(t, 0, 2, 2);
        }
        let second = g.report_line(snap(0), t).expect("second window");
        assert!(
            second.contains("acks=5/") && second.contains("crecv=20"),
            "the window count must reset and the total must not: {second}"
        );
    }

    /// A path with nothing to report prints nothing, so a `0` on an
    /// `[ACKDIAG]` line is always a MEASURED zero (the `dgq` discipline:
    /// a gauge that reads 0 for two different reasons is not a gauge).
    #[test]
    fn a_silent_path_emits_no_line() {
        let g = AckCadenceGauge::new();
        assert!(g.report_line(snap(7), 1_000).is_none());
        g.note_ack_at(1_000, 7, 1, 1);
        assert!(g.report_line(snap(7), 2_000).is_some());
        // Reported and then silent again ⇒ silent again.
        assert!(g.report_line(snap(7), 3_000).is_none());
    }

    /// The per-window cap is REPORTED, never silent: a truncated window must
    /// be distinguishable from a complete one.
    #[test]
    fn the_sample_cap_is_reported_not_hidden() {
        let g = AckCadenceGauge::new();
        let mut t = 0u64;
        for _ in 0..(SAMPLE_CAP + 10) {
            t += 10;
            g.note_ack_at(t, 0, 1, 1);
        }
        let line = g.report_line(snap(0), t).expect("path 0 reported");
        assert!(!line.contains(" ov=0"), "the cap must be visible: {line}");
    }

    /// **THE BEHAVIOUR-NEUTRALITY PIN.** The gauge is observation-only, and
    /// this is asserted STRUCTURALLY rather than promised in prose: no method
    /// on this module may reach back into the engine's mutable state. The
    /// gauge's own source must contain no `&mut Scheduler` / `path_mut` /
    /// `set_` call and no write to any engine handle — its only writes are to
    /// its OWN fields, and its only engine reads are the four immutable
    /// snapshot values in [`PathSnapshot`].
    ///
    /// Why a source scrape and not a runtime assertion: the failure mode is
    /// someone LATER adding a convenient write here (the `[SF]` gauge's own
    /// history), and that omission has no runtime symptom to assert on — the
    /// same reasoning `gates::forwarding_audit` and the wait-bucket audit
    /// already use in this crate.
    #[test]
    fn ackdiag_is_observation_only() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/net/ackdiag.rs"),
        )
        .expect("read src/net/ackdiag.rs");
        // The GAUGE, not this test module — the forbidden list below is
        // itself a set of those literals.
        let src = &src[..src.find("#[cfg(test)]").expect("the test module marker")];
        // Strip comments and doc comments: the module header NAMES these
        // things to explain why it does not do them.
        let code: String = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in [
            "path_mut",
            "&mut Scheduler",
            "set_cc_window_bytes",
            "release_in_flight",
            "charge_in_flight",
            "record_delivery(",
            "on_delivery_signal",
        ] {
            assert!(
                !code.contains(forbidden),
                "the ack-cadence gauge must not touch engine state: found `{forbidden}`"
            );
        }
        // And the ONE mutable engine borrow it could plausibly acquire — the
        // scheduler lock — must be read-only: `sched.path(` , never
        // `scheduler.lock()` results used mutably.
        assert!(
            code.contains(".path(*id)"),
            "the snapshot must read paths through the IMMUTABLE `Scheduler::path` \
             accessor — `path_mut` is on the forbidden list above"
        );
    }

    /// **THE WINDOW OVERRIDE, ABSOLUTE.** The default is the SHIPPED 2 s and
    /// every path into it is pinned to a number — an ordinal "smaller than
    /// the default" test would pass on a resolver that returned garbage.
    ///
    /// The default arm is the load-bearing one: every committed `[ACKDIAG]`
    /// ledger was captured at 2 s and the window is the unit of every series
    /// read off them, so an override that shifted the DEFAULT would silently
    /// re-unit the whole era's record.
    #[test]
    fn the_window_override_resolves_to_absolute_values_and_defaults_unchanged() {
        // ERA COMPARABILITY: unset is 2 s, exactly as before the override.
        assert_eq!(resolve_window_us(None), 2_000_000);
        assert_eq!(resolve_window_us(None), ACKDIAG_WINDOW_US);
        // The c9 arm's value, which is the `[DIAG]` line's own cadence and
        // the blocking dependency C9-1..4 are written against.
        assert_eq!(resolve_window_us(Some("250000")), 250_000);
        // Whitespace is trimmed (an `env` prefix can carry it).
        assert_eq!(resolve_window_us(Some("  250000 ")), 250_000);
        // GARBAGE FALLS BACK TO THE DEFAULT, never to 0 and never to a panic.
        // A 0 window would fire a report on every sender-loop iteration; an
        // unparseable one is a driver typo, and both must be VISIBLE in the
        // echo as "your override did not take" rather than as a dead or
        // screaming gauge.
        for bad in ["", "0", "abc", "-1", "250_000", "2e5", "250000ms"] {
            assert_eq!(
                resolve_window_us(Some(bad)),
                ACKDIAG_WINDOW_US,
                "{bad:?} must fall back to the shipped default"
            );
        }
        // THE CLAMP, at both ends and on both sides of each edge.
        assert_eq!(resolve_window_us(Some("1")), ACKDIAG_WINDOW_US_MIN);
        assert_eq!(resolve_window_us(Some("49999")), ACKDIAG_WINDOW_US_MIN);
        assert_eq!(resolve_window_us(Some("50000")), ACKDIAG_WINDOW_US_MIN);
        assert_eq!(resolve_window_us(Some("60000000")), ACKDIAG_WINDOW_US_MAX);
        assert_eq!(
            resolve_window_us(Some("999999999")),
            ACKDIAG_WINDOW_US_MAX
        );
        // And the clamp does not touch anything inside its own range.
        assert_eq!(resolve_window_us(Some("2000000")), 2_000_000);
    }

    /// The window is what `report_due` actually gates on — the wiring, not
    /// just the resolver (MEASUREMENT DISCIPLINE rule 1: prove the mechanism
    /// under test executes). A resolver that no caller reads is a constant.
    #[test]
    fn report_due_gates_on_the_active_window() {
        let g = AckCadenceGauge::new();
        let w = window_us();
        // First call only stamps the epoch — it never reports.
        assert!(!g.report_due(1_000));
        // One µs short of the window: still closed.
        assert!(!g.report_due(1_000 + w - 1));
        // Exactly one window: open.
        assert!(g.report_due(1_000 + w));
        // And the window re-arms from the new stamp, not from the old one.
        assert!(!g.report_due(1_000 + w + 1));
        assert!(g.report_due(1_000 + 2 * w));
    }

    /// The gate ships OFF, so the gauge is absent and every feed site is a
    /// null check. (Set-env semantics are `config::env_flag`'s.)
    #[test]
    fn the_gauge_is_absent_on_the_shipped_default() {
        // NOTE: relies on the test env not exporting RWM_ACKDIAG — the same
        // assumption every engine-default test in this crate makes.
        if std::env::var("RWM_ACKDIAG").is_ok() {
            return;
        }
        assert!(
            gauge().is_none(),
            "RWM_ACKDIAG ships default OFF: the gauge must not exist"
        );
    }
}
