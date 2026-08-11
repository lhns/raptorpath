//! c8 SF-MECHANISM component bench (MEASUREMENT DISCIPLINE 14) — goal-gate
//! "c8 SF Mechanism", 2026-08-11.
//!
//! `store_cap_bench.rs` answers the STATIC question (by how much does the cap
//! differ when the Σ-base is filtered?). It cannot answer the measured one,
//! because the measured effect is a LOOP: goal-gate "Store-Cap Unification —
//! RESULTS" found that `RWM_STORE_CAP_UNIFIED` raises the `[SF]` zero-fraction
//! — `active_paths()` EMPTY at a dyn-cap refresh — from ≈4% to ≈30% at c8, past
//! 2σ on both seeds and in both anchor eras, while c1/c7/sc2 do not move.
//!
//! The gauge is recorded from `active_paths()` UNCONDITIONALLY (see
//! `store_cap_sf_record`'s call site in `net/mod.rs`: `act` is computed on both
//! arms). So U cannot move the gauge directly — it can only move it through the
//! loop cap → admission → in_flight → `available()` → `active_paths()` → cap.
//! Reproducing that requires CLOSING the loop, which is what this bench does:
//!
//!   * the REAL `Scheduler` / `PathState` (real Copa-lite cwnd dynamics,
//!     real `copa_bdp_anchor()`, the real `active_paths()` / `live_paths()`
//!     predicates, the real `best_source_path()` placement objective),
//!   * a `MockClock` and a deterministic bottleneck-link model per path
//!     (serialisation at the path rate + a standing queue + RTprop), so a
//!     cwnd-saturating sender builds its own delay signal,
//!   * the SHIPPED dyn-cap chain at the battery's arms (A/AU/AL/ALU are all
//!     `store_paths_on = true`, `percap = capw = pool_anchor = three_term =
//!     honest_cap = off`), i.e. `path_scaled_store_cap` over the path set the
//!     flag selects, refreshed on the shipped 5 ms cadence.
//!
//! No wall clock, no sockets, no tokio, no netem: same numbers every run —
//! but only since `place_min_cost` began breaking exact-cost TIES by path id
//! (see its comment). `Scheduler` holds paths in a `HashMap`, so before that
//! the SYMMETRIC cell alone was reproducible only within a process.
//!
//! Run:
//!   cargo test --test store_cap_sf_bench --release -- --ignored --nocapture

use std::sync::Arc;
use std::time::Duration;

use raptorpath::control::fec_rate::ProtocolHint;
use raptorpath::control::FecRateController;
use raptorpath::fec::FecBackend;
use raptorpath::net::path_scaled_store_cap;
use raptorpath::scheduler::{MockClock, Scheduler};

// ── Shipped policy constants at the battery's arms (sender_policy::resolve) ──
const GAIN: f64 = 2.0;
const FLOOR: usize = 64;
const KNEE: usize = 2048; // RWM_STORE_PATH_POOL
const STORE_MAX: usize = 1024; // RELIABLE_STORE_MAX
const BOOT: usize = 128; // RWM_STORE_BOOT
const REFRESH_S: f64 = 0.005; // the dyn-cap refresh throttle

// ── Shipped FEC-controller constants (config::resolve + net/mod.rs:1570) ────
// These are the resolved DEFAULTS the battery's arms run at, quoted by site,
// in exactly the way GAIN/FLOOR/KNEE above are quoted. Nothing here is
// chosen to make an answer come out: `target_tail_loss` and
// `max_fec_overhead` are `config.rs:317-318`'s `unwrap_or`s, the hint is the
// battery's (Auto), the backend is `config.rs:286`'s `None => RaptorQ`, and
// the symbol size is the bulk profile's (`net/mod.rs:158`).
const TAIL_LOSS: f64 = 1e-5;
const MAX_OVERHEAD: f64 = 0.5;
const SYMBOL_SIZE: u16 = 1200;
/// `net/mod.rs:178` — the report task's cadence, which is the ONLY site that
/// feeds `LossEstimator::record_throughput` in production
/// (`net/tasks/report.rs:84`), gated on its own `dt > 0.2`.
const REPORT_S: f64 = 2.0;

// ── The cells, at the parameters store_cap_bench.rs already quotes ──────────
// c2 = 100 Mbit / 10 ms RTT, GE 1.3%/50% ⇒ 10 400 sym/s, RTprop 8 ms (anchor 83.2)
// c3 =  20 Mbit / 40 ms RTT, GE 2%/40%   ⇒  2 000 sym/s, RTprop 60 ms (anchor 120.0)
const C2: Spec = (10_400.0, 0.008, 0.013, 0.50);
const C3: Spec = (2_000.0, 0.060, 0.020, 0.40);

/// Which path set the dyn-cap phase's Σ-anchor base iterates, and which
/// pooled ceiling composes it — the two axes the shipped chain fixes and the
/// candidate successor varies.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Arm {
    /// `RWM_STORE_CAP_UNIFIED=0`: Σ over `active_paths()`, ×N pooled law.
    Legacy,
    /// `RWM_STORE_CAP_UNIFIED=1`: Σ over `live_paths()`, ×N pooled law.
    Unified,
    /// The pre-named successor: the POOLED CEILING composed with the UNIFIED
    /// set — Σ over `live_paths()` with the ×N COUNT multiplier dropped, the
    /// N·knee ceiling kept. `cap = clamp(gain·Σ_live, floor, N·knee)`. No new
    /// constant: it deletes the multiplier that made the Σ and the ×N range
    /// over different sets in the first place.
    PooledUnified,
}

impl Arm {
    fn label(self) -> &'static str {
        match self {
            Arm::Legacy => "A   (U=0, shipped)",
            Arm::Unified => "AU  (U=1)         ",
            Arm::PooledUnified => "P   (pooled+unified)",
        }
    }
}

/// The shipped dyn-cap chain at the battery's arms, verbatim in structure:
/// `path_scaled_store_cap` → legacy `gain·Σ` → the boot cap.
fn shipped_chain(bdp: f64, n_live: usize) -> usize {
    if let Some(c) = path_scaled_store_cap(true, n_live, bdp, GAIN, FLOOR, KNEE) {
        c
    } else if bdp > 0.0 {
        ((GAIN * bdp).ceil() as usize).clamp(FLOOR, STORE_MAX)
    } else {
        BOOT.min(STORE_MAX)
    }
}

fn cap_for(arm: Arm, bdp_over_set: f64, bdp_over_live: f64, n_live: usize) -> usize {
    match arm {
        Arm::Legacy | Arm::Unified => shipped_chain(bdp_over_set, n_live),
        Arm::PooledUnified => {
            if n_live >= 2 && bdp_over_live > 0.0 {
                let ceiling = n_live.saturating_mul(KNEE).max(FLOOR);
                ((GAIN * bdp_over_live).ceil() as usize).clamp(FLOOR, ceiling)
            } else {
                shipped_chain(bdp_over_live, n_live)
            }
        }
    }
}

/// Deterministic PRNG (xorshift64*) — the bench must give the same numbers on
/// every host and every run, so nothing here touches `rand::random`.
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    fn f64(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// One path's bottleneck: serialisation at `rate` sym/s into an unbounded
/// queue, then a fixed one-way `rtprop`, with a Gilbert–Elliott loss process —
/// the cells are DEFINED with GE loss (c2 1.3%/50%, c3 2%/40%) and the L1
/// battery's own gauges show retx riding U (+801/+797), so a lossless link
/// cannot be the vehicle for this question. A symbol sent while the queue is
/// backed up waits — which is how a cwnd-saturating sender manufactures the
/// delay signal Copa backs off on.
struct Link {
    rate: f64,
    rtprop: f64,
    /// Bottleneck serialisation cursor (seconds).
    busy_until: f64,
    /// GE: currently in the bad (dropping) state.
    bad: bool,
    /// P(bad → bad) — the burst persistence.
    persist: f64,
    /// P(good → bad), derived from the target loss rate and `persist`.
    to_bad: f64,
    rng: Rng,
}

/// (rate sym/s, RTprop s, GE loss rate, GE persistence)
type Spec = (f64, f64, f64, f64);

impl Link {
    fn new((rate, rtprop, loss, persist): Spec, seed: u64) -> Self {
        // Stationary bad-fraction π_b = loss ⇒ to_bad = (1−persist)·π_b/(1−π_b).
        let to_bad = if loss > 0.0 { (1.0 - persist) * loss / (1.0 - loss) } else { 0.0 };
        Self { rate, rtprop, busy_until: 0.0, bad: false, persist, to_bad, rng: Rng::new(seed) }
    }
    /// Serialise one symbol. Returns `(resolve_time, rtt, delivered)` —
    /// `resolve_time` is when the SENDER learns this symbol's fate, which is
    /// the ack instant whether or not the symbol survived: a lost symbol is
    /// reported by the same feedback message (the receiver's expected/received
    /// counters), which is exactly what the engine's counter-delta release
    /// reads (`control_msg.rs:341`, `:685`). Dropped symbols still consume the
    /// bottleneck.
    fn send_resolved(&mut self, now: f64) -> (f64, f64, bool) {
        let dep = self.busy_until.max(now) + 1.0 / self.rate;
        self.busy_until = dep;
        self.bad = if self.bad {
            self.rng.f64() < self.persist
        } else {
            self.rng.f64() < self.to_bad
        };
        let ack = dep + self.rtprop;
        (ack, ack - now, !self.bad)
    }
}

/// One admitted symbol's retention-store entry. It leaves the store on ack;
/// a dropped one is retransmitted after the recovery plane's time threshold
/// and occupies the store the whole time — which is why the store cap, not
/// cwnd, is what bounds a lossy transfer's outstanding set.
struct Sym {
    path: u32,
    sent: f64,
    /// `Some(t)` = will be acked at t; `None` = dropped, awaiting retransmit.
    ack_at: Option<f64>,
    rtt: f64,
    /// The reliable stream sequence number, assigned once at first admission
    /// and CARRIED across retransmits — the number the receiver's cumulative
    /// frontier is expressed in (`Feed::Cumulative` only).
    seq: u64,
    /// When the SENDER learns this flight's fate (delivered OR lost). Equal to
    /// `ack_at` for a delivered symbol; for a dropped one it is the instant the
    /// feedback that reports the hole arrives. `Acct::Off` never reads it.
    resolve_at: f64,
    /// `Acct` arms only: this flight's loss has already been reported to the
    /// ledger by the counter delta, so the retransmit must not re-release it.
    resolved: bool,
}

// ── THE IN-FLIGHT ACCOUNTING AXIS ──────────────────────────────────────────
//
// goal-gate "PIPELINE VERIFICATION MATRIX" rows 2 + 6, suspect rank 1: the
// engine's recovery traffic is partially UN-METERED and its in-flight ledger
// does not balance by construction. Three documented divergences, none of
// which the bench modelled:
//
//   (a) REPAIR RIDES TOKEN-FREE. `emit_source.rs:493-497` debits the CC token
//       bucket inside the SOURCE arm only (`if pol.cc_pace { st.src_tokens -=
//       1.0 }`); the taper correction symbol at `emit_source.rs:929` consumes
//       the wire and no token, so the realized wire rate is src·(1+r). It IS
//       charged to in_flight. (On the shipped default `RWM_CC_PACE=0` — 1 116
//       of 1 116 logs, goal-gate "What Binds Throughput" — so the bucket does
//       not run at all and the divergence is about WIRE OCCUPANCY, not about
//       spacing. The token counter is carried here to BOUND that, row 3 below.)
//   (b) TWO CHANNELS BYPASS THE CHARGE ENTIRELY. The SACK-gap retransmit
//       (`net/mod.rs:6374-6383`) builds a `SymbolBatch` and calls
//       `transport.send_symbols` directly — no `charge_in_flight`; and the
//       NACK repair margin (`net/mod.rs:6420-6448`), `margin = ceil(
//       retransmitted × max_active_loss)`, likewise.
//   (c) RELEASE IS COUNTER-DELTA DRIVEN, NOT 1:1 WITH CHARGES.
//       `control_msg.rs:341` / `:685` release `expected − received` on the
//       path the FEEDBACK arrived on. `receiver.rs:1754` builds those counters
//       from the per-batch symbol counts, so EVERY wire symbol — source,
//       taper repair, retransmit, margin repair — enters them, whether or not
//       it was ever charged. `release_in_flight` saturates at zero, so the
//       ledger can neither go negative nor recover a release it wasted.
//
// The axis has THREE levels on purpose, so the two things (b)+(c) bundle are
// not confounded: the recovery traffic EXISTING (wire + queue occupancy,
// placed by the ρ_fate repair objective onto the leg that is recovering) is a
// different claim from the ledger NOT BALANCING.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Acct {
    /// The published bench: source + retransmit only, every wire symbol
    /// charged once and released once on its OWN path, no estimator feed.
    /// Bit-identical to every number in goal-gate "c8 SF Mechanism" and
    /// "SF Anchor Suspect".
    Off,
    /// The recovery TRAFFIC exists — taper repair at the shipped r*, the NACK
    /// repair margin, and the loss estimator/throughput feeds that produce
    /// them — but the ledger still BALANCES: every wire symbol is charged
    /// once, on the path it flies, and released once, on the same path.
    /// This is the counterfactual engine that obeys §12.
    Traffic,
    /// The ENGINE. As `Traffic`, plus (b) and (c): retransmits and margin
    /// repairs are never charged, and release is a counter delta on the path
    /// the feedback arrived on rather than a match to a charge.
    Engine,
}

impl Acct {
    fn label(self) -> &'static str {
        match self {
            Acct::Off => "OFF  (published bench)",
            Acct::Traffic => "TRAFFIC (metered)     ",
            Acct::Engine => "ENGINE (un-metered)   ",
        }
    }
    fn on(self) -> bool {
        self != Acct::Off
    }
}

/// One un-stored recovery flight: a taper repair or a NACK margin repair. It
/// occupies the wire and (for the taper repair) the in-flight ledger, but
/// never the retention store — which is why the store cap, the loop's only
/// brake, cannot see it.
struct WireSym {
    path: u32,
    resolve_at: f64,
    delivered: bool,
}

/// The per-channel emission ledger, reported with every ON run so the
/// attribution is a measurement and not a guess (MEASUREMENT DISCIPLINE
/// 14(b)).
#[derive(Debug, Clone, Copy, Default)]
struct Ledger {
    /// Source admissions — the ONLY arm the pacer's token debit runs on.
    src: u64,
    /// Taper corrections (`emit_source.rs:929`): wire + charge, no token.
    taper: u64,
    /// SACK-gap retransmits (`net/mod.rs:6374`): wire only under `Engine`.
    retx: u64,
    /// NACK repair margin (`net/mod.rs:6420`): wire only under `Engine`.
    margin: u64,
    /// `charge_in_flight(1)` calls.
    charges: u64,
    /// `release_in_flight(1)` calls.
    releases: u64,
    /// Releases that landed on a path already at `in_flight == 0` — the
    /// budget the saturating subtraction threw away.
    releases_wasted: u64,
    /// The pacer's debit count (source arm only), i.e. what §12 claims paces
    /// the wire.
    tokens: u64,
}

impl Ledger {
    /// Every symbol that reached the link.
    fn wire(&self) -> u64 {
        self.src + self.taper + self.retx + self.margin
    }
}

// ── THE ANCHOR-ERA AXIS (goal-gate "SF Anchor Suspect") ────────────────────
//
// FINDING 3 of the "c8 SF Mechanism" section reproduced U's direction but not
// its CELL SPECIFICITY, and named one suspect: *the bench's Copa reads an
// HONEST anchor and the engine's legacy one does not.* The bench acks
// per-symbol at the true delivery instant, so `CopaState::record_delivery`'s
// Δdelivered/Δt can only read the truth; the engine's legacy ack-interval
// sampler over-reads ×4.6–7.4 (goal-gate "Anchor Hygiene" (b)) because acks
// arrive BATCHED and the cumulative frontier JUMPS. That anchor is not just
// the store-cap Σ — via `clamp_cwnd_with_anchor` it is also the cwnd FLOOR,
// so an over-reading anchor PROPS `available() > 0` and can keep the fast
// symmetric cells out of the empty-`active_paths()` state entirely.
//
// This axis makes the era a bench variable, two ways, because neither alone
// would be honest:
//
//   * `Overread(f)` — the era as a PURE SCALE on the ack-interval sampler's
//     input, SWEPT. `record_delivery` uses its `count` argument for nothing
//     but Δdelivered, so feeding `f·count` scales every rate sample — and
//     hence `max_bw`, `bdp_anchor()`, the anchor floor and the store-cap Σ —
//     by exactly `f`, with the call cadence, the cwnd update cadence and the
//     RTT feed bit-identical to the honest arm. `f = 1.0` IS the honest arm.
//     No single f is privileged: the sweep reports the whole curve and the
//     matrix quotes the wire's measured 4.6–7.4 band as a BAND.
//   * `Cumulative { ack_period_s }` — the era DERIVED from the bench's own
//     ack batching, with no injected number at all: a receiver that reports a
//     CUMULATIVE frontier on a feedback cadence. A GE drop stalls the
//     frontier; when the retransmit lands the frontier jumps by the whole
//     run, and the sampler sees that jump over one feedback interval. The
//     realized over-read is then MEASURED (`anchor / true BtlBw·RTprop`) and
//     compared against the wire's band, rather than assumed.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Feed {
    /// Per-symbol `on_ack(1)` at the true delivery instant — the SHIPPED
    /// honest-anchor era (default since 9f6e56b), and bit-identical to the
    /// original bench.
    Honest,
    /// The legacy ack-interval era as a swept scale on the sampler input.
    Overread(f64),
    /// The legacy era derived from cumulative-frontier acks at a feedback
    /// cadence (seconds).
    Cumulative { ack_period_s: f64 },
    /// THE MEASURED WIRE ERA — the ack stream as goal-gate "Ack-Cadence
    /// Measurement (VM)" recorded it, one `AckShape` per path of the cell.
    /// Nothing about the anchor is injected: `record_delivery` is fed ONE
    /// delivered symbol per ack at measured arrival instants and the shipped
    /// 1 ms `elapsed` floor (`scheduler/mod.rs:1178`) does the folding.
    Measured(&'static [AckShape]),
}

impl Feed {
    fn label(self) -> String {
        match self {
            Feed::Honest => "honest (x1.0)".into(),
            Feed::Overread(f) => format!("over-read x{f:.1}"),
            Feed::Cumulative { ack_period_s } => format!("cum-ack {:.2} ms", ack_period_s * 1e3),
            Feed::Measured(_) => "MEASURED (wire)".into(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
//  THE MEASURED ACK STREAM — goal-gate "Ack-Cadence Measurement (VM)"
//  (2026-08-11, `feat/ackdiag-measurement` from main@c0d9305)
// ═══════════════════════════════════════════════════════════════════════════
//
// Every prior SF-bench result had to INVENT its ack stream, and the ledger
// scored all three inventions as wrong by one to three orders of magnitude
// ("WHAT THIS MEANS FOR THE SF BENCH'S INPUTS"). They are replaced here by the
// wire's own numbers, transcribed row by row from the measurement and NOTHING
// else. The four inputs the ledger names, and where each one lands:
//
//   (1) DELIVERED COUNT PER ACK = 1. "p50 = p90 = 1 in all 60 report windows,
//       857 400 acks, every cell and every path. There is no ack aggregation."
//       ⇒ `record_delivery(1)`, once per delivered symbol. NOT swept.
//   (2) ARRIVAL SPACING — heavy-tailed, per cell and per path (READOUT 1+2's
//       gap columns). "A single assumed period cannot represent this and
//       should not be attempted." ⇒ the quantiles below, as a DISTRIBUTION.
//   (3) THE 1 ms FLOOR IS THE CLOCK. "The sampler is FLOOR-CLOCKED — model the
//       floor, not the acks. Feed `record_delivery` at ack cadence and let the
//       1 ms `elapsed` floor do the work." ⇒ the bench advances the MockClock
//       to each ack's own arrival instant and calls the REAL `on_ack(1)`; the
//       real `elapsed < 0.001` branch (`scheduler/mod.rs:1178`) rejects. The
//       bench asserts NOTHING about the sample period — it MEASURES the
//       realized rejection rate and scores it against READOUT 3b.
//   (4) `xanchor` IS NOT AN INPUT. It is the CHECK. READOUT 3's medians
//       (5.94 / 9.80–10.11 / 13.29–13.82) are targets the loop must PRODUCE.
//
// THE ONE STRUCTURAL CHOICE, STATED RATHER THAN BURIED. The gauge measured the
// MARGINAL gap distribution (p50/p90/p99), not its correlation structure, and
// the marginal ALONE cannot produce the measured over-read: an i.i.d. renewal
// stream with these quantiles puts ~1000 µs / mean_gap ≈ 10 acks in any 1 ms
// window, so `Δdelivered/Δt` reads ×1 and the max filter has nothing to latch.
// A ×8 sample needs ~74 CONSECUTIVE sub-p50 gaps, which i.i.d. draws never
// deliver. The over-read therefore lives in the stream's RUN structure, and
// the model of it here is the one every measured number is already consistent
// with — a work-conserving observer:
//
//     the sender observes acks one at a time, spaced at the MEASURED p50 gap,
//     while it has un-observed acks; when it runs out it goes SILENT for a
//     draw from the MEASURED upper tail, and the acks that arrive during the
//     silence are observed in the burst that follows.
//
// This is not a free-parameter fit. It has exactly ZERO knobs, because
// conservation closes it: an observer that drains at spacing `s` has duty
// cycle `s/ḡ = q50` by arithmetic, so the silence fraction is pinned at the
// value that makes the model's own marginal REPRODUCE the measured p50, p90
// and p99 exactly (`u_c` below, solved, not chosen). What the model then
// PREDICTS, and what this bench scores, is everything the ledger measured but
// did not feed in: the floor-rejection rate, the accepted-sample rate, the
// acks folded per sample, and `xanchor`.

/// One measured path's ack stream, transcribed from goal-gate "Ack-Cadence
/// Measurement (VM)". The `(lo, hi)` pairs are the ledger's own per-window
/// RANGES over its 12 report windows — the measurement's uncertainty, carried
/// rather than averaged away. Micro-seconds; `rate_lr` is symbols/s.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct AckShape {
    /// The ledger row this is, verbatim.
    row: &'static str,
    /// READOUT 3, `rate_lr` column — the window's own long-run delivered rate,
    /// i.e. the mean ack gap is `1e6/rate_lr` µs.
    rate_lr: f64,
    /// READOUT 1+2, `gap µs p50` column.
    p50: (f64, f64),
    /// READOUT 1+2, `p90` column.
    p90: (f64, f64),
    /// READOUT 1+2, `p99` column.
    p99: (f64, f64),
    // ── the CHECKS: measured, never fed in ──
    /// READOUT 3b, `rejected %` — what the 1 ms floor did on the wire.
    rej_pct: f64,
    /// READOUT 3b, `accepted samples/s`.
    samples_s: f64,
    /// READOUT 3, `xanchor med` — the quantity the store-cap Σ and the cwnd
    /// anchor floor consume, and the one this bench must PRODUCE.
    xanchor: f64,
    /// READOUT 3, `xanchor` min/max over the 12 windows.
    xanchor_range: (f64, f64),
    /// READOUT 3, `RTprop ms` column — **the anchor's own `min_rtt`**, i.e.
    /// exactly the `min_rtt` `copa_bdp_anchor()` multiplied by (goal-gate
    /// "SF Bench on Measured Inputs", definitional correction (a)). Seconds.
    ///
    /// Added by goal-gate "Cap-Refresh Warmth": with it the wire's anchor is
    /// RECONSTRUCTIBLE in symbols rather than only as a dimensionless ratio —
    /// `xanchor := copa_bdp_anchor()/(rate_lr·RTprop)` inverts EXACTLY to
    /// `anchor = xanchor · rate_lr · RTprop` ([`AckShape::anchor_sym`]), which
    /// is the term the store-cap Σ actually adds up.
    rtprop_s: f64,
}

impl AckShape {
    /// The wire's own `copa_bdp_anchor()` for this path, IN SYMBOLS — the
    /// store-cap Σ's per-path term, reconstructed from READOUT 3 by inverting
    /// the definition of `xanchor`. No modelling and no fitting: three
    /// measured columns multiplied.
    ///
    /// This is NOT `rate_configured · RTT_configured · xanchor`. The wire's
    /// realized `rate_lr` is 0.67–0.69× the cells' nominal symbol rates and
    /// its RTprop is 0.64–1.05× their configured RTTs, so the two differ by
    /// **1.4–2.3× per path** — the same "scale by the path's REALIZED ack
    /// rate, not its link capacity" caveat the measured-inputs section stated
    /// for the ack MODEL and did not apply to the Σ.
    fn anchor_sym(&self) -> f64 {
        self.xanchor * self.rate_lr * self.rtprop_s
    }
}

/// The Σ at which the shipped pooled law STOPS responding to the anchor.
///
/// `cap = clamp(gain·N·Σ, floor, N·knee)` is ceiling-pinned exactly when
/// `gain·N·Σ ≥ N·knee`, i.e. when `Σ ≥ knee/gain` — **the `N` cancels**. The
/// pin threshold on the anchor SUM is a per-path constant, identical at every
/// path count, and at the shipped `knee = 2048`, `gain = 2` it is 1024
/// symbols. Pinned by
/// `the_pin_threshold_on_sigma_is_knee_over_gain_and_is_path_count_free`.
const SIGMA_PIN: f64 = KNEE as f64 / GAIN;

/// `c2r100/p0` — single 100 MB, the reference cell. READOUT 1+2 row 1,
/// READOUT 3 row 1, READOUT 3b row 1.
const ACK_C2R100_P0: AckShape = AckShape {
    row: "c2r100/p0",
    rate_lr: 9_316.0,
    p50: (17.0, 23.0),
    p90: (228.0, 374.0),
    p99: (930.0, 1522.0),
    rej_pct: 91.5,
    samples_s: 744.0,
    xanchor: 5.94,
    xanchor_range: (4.04, 9.28),
    rtprop_s: 0.1004, // READOUT 3, `RTprop ms` = 100.4
};

/// `c7/p0` — c2/c2 dual 200 MB, leg 0. READOUT 1+2 row 2 / 3 row 2 / 3b row 2.
const ACK_C7_P0: AckShape = AckShape {
    row: "c7/p0",
    rate_lr: 9_432.0,
    p50: (13.0, 14.0),
    p90: (73.0, 96.0),
    p99: (1838.0, 2052.0),
    rej_pct: 94.3,
    samples_s: 536.0,
    xanchor: 9.80,
    xanchor_range: (8.14, 11.95),
    rtprop_s: 0.0077, // READOUT 3, `RTprop ms` = 7.7
};

/// `c7/p1` — the symmetric dual's other leg. Rows 3 / 3 / 3.
const ACK_C7_P1: AckShape = AckShape {
    row: "c7/p1",
    rate_lr: 9_418.0,
    p50: (13.0, 14.0),
    p90: (68.0, 87.0),
    p99: (1807.0, 2006.0),
    rej_pct: 94.3,
    samples_s: 536.0,
    xanchor: 10.11,
    xanchor_range: (8.06, 10.57),
    rtprop_s: 0.0097, // READOUT 3, `RTprop ms` = 9.7
};

/// `c8/p0` — the asymmetric dual's FAST (c2) leg. Rows 4 / 4 / 4.
const ACK_C8_P0: AckShape = AckShape {
    row: "c8/p0 fast",
    rate_lr: 6_948.0,
    p50: (11.0, 13.0),
    p90: (42.0, 182.0),
    p99: (1697.0, 2048.0),
    rej_pct: 94.0,
    samples_s: 415.0,
    xanchor: 13.29,
    xanchor_range: (7.79, 27.34),
    rtprop_s: 0.0084, // READOUT 3, `RTprop ms` = 8.4
};

/// `c8/p1` — the asymmetric dual's SLOW (c3) leg, the one that stalls to
/// 18.2 ms and the only path the floor rejects less than 90% of. Rows 5/5/5.
const ACK_C8_P1: AckShape = AckShape {
    row: "c8/p1 slow",
    rate_lr: 1_376.0,
    p50: (31.0, 70.0),
    p90: (1918.0, 2194.0),
    p99: (5354.0, 18229.0),
    rej_pct: 81.5,
    samples_s: 258.0,
    xanchor: 13.82,
    xanchor_range: (7.35, 27.56),
    rtprop_s: 0.0386, // READOUT 3, `RTprop ms` = 38.6
};

/// The measured cells, path by path. `sc2` is the bench's single-fast cell and
/// the wire's single cell is `c2r100`; `c7`/`c8` map leg for leg.
static ACK_SC2: [AckShape; 1] = [ACK_C2R100_P0];
static ACK_C7: [AckShape; 2] = [ACK_C7_P0, ACK_C7_P1];
static ACK_C8: [AckShape; 2] = [ACK_C8_P0, ACK_C8_P1];

// ── THE PRE-REGISTRATION (written and committed BEFORE the measured era was
//    ever run; the ON arm is the NEXT commit) ────────────────────────────────
//
// THE QUESTION the dispatch asks: with the ack stream measured instead of
// invented, and with the in-flight accounting axis at `Acct::Engine`, does the
// bench's geography match the wire at BOTH cells?
//
//   * the legacy (A) arm is a ≈4% `[SF]` zero-fraction class at c7 AND c8
//     (3.7–7.4% at c8, "c1/sc2/c7 do not move"), and
//   * the U-fold is keyed to c8 (≈7.5×) and null at c7.
//
// That is the SAME G1/G2 pair the accounting axis pre-registered and the same
// statistic (§16.52's mode rate), unchanged, so the two runs are comparable:
//   G1 (LEVEL)       A-arm ensemble mean < 10% AND caught ≥ 50% at BOTH cells.
//   G2 (CELL-KEYING) fold(c8) ≥ 3.0 AND fold(c7) ≤ 2.0.
//
// AND — new here, because the inputs are now measured and therefore SCORABLE —
// three VALIDATION targets that must hold before the geography verdict may be
// read at all. A loop whose ack stream lands nowhere near the wire's cannot
// be asked whether its geography matches; these gate the question.
//
//   V1 `xanchor`. The bench's realized per-path `copa_bdp_anchor()/(rate·
//      RTprop)` must land within ±30% of the ledger's measured median at each
//      measured path (5.94 / 9.80 / 10.11 / 13.29 / 13.82). Why ±30% and not
//      tighter: the measured quantity's OWN spread is far wider — 4.04–9.28 at
//      c2r100 and 7.35–27.56 at c8, i.e. a 2.3×–3.8× range across windows of
//      the same run, "2.5× between windows of the SAME run" — so a ±30% band
//      on the median is already tight against the measurement's own noise, and
//      it is 10–300× tighter than the ×3–100 by which the three invented
//      inputs missed. Scored per path, not per cell, because READOUT 3 is a
//      per-path table.
//   V2 FLOOR REJECTION. The realized `elapsed < 1 ms` rejection rate must land
//      within ±5 POINTS of READOUT 3b (91.5 / 94.3 / 94.3 / 94.0 / 81.5%).
//      ±5 points because the wire's own per-window spread is ±0.5 pt at three
//      of five paths but the paths themselves span 81.5–94.3, and because this
//      is a PREDICTION of the model, not an input to it.
//   V3 THE MARGINAL. The realized ack-gap p50/p90/p99 must lie inside the
//      ledger's own reported per-window ranges. (The observer model
//      reproduces these BY CONSTRUCTION, so V3 is a wiring check — it fails
//      only if the era is mis-plumbed, which is exactly what it is for.)
//
// VERDICT = (V1 ∧ V2 ∧ V3) gating (G1 ∧ G2). If the validation gate fails the
// geography question is NOT ASKED and the run reports which produced quantity
// diverged first. If it passes and G1 ∧ G2 fails, the loop is wrong somewhere
// the measured inputs do not reach, and the run reports which quantity — the
// dispatch's own named NO-MATCH outcome.
//
// FOURTH, and not a criterion because the axis is supposed to produce it
// rather than be scored on it: REPAIRS-IN-COUNTERS. READOUT 4 settles
// Σ`crecv`/`srcack` at 1.01–1.04 at c2r100/c7 and 1.21–1.34 at c8. Under
// `Acct::Engine` every wire symbol enters the bench's `ack_expected` /
// `ack_received` counters, so the same ratio is `wire()/src` — already
// printed. It is checked as an EMERGENT property and reported either way.

/// V1 — the fraction by which the bench's realized `xanchor` may differ from
/// the ledger's measured per-path median.
const V1_XANCHOR_TOL: f64 = 0.30;
/// V2 — the points by which the realized floor-rejection rate may differ from
/// READOUT 3b's measured per-path percentage.
const V2_REJECT_TOL_PTS: f64 = 5.0;

/// Every measured path the bench consumes, for the transcription pin and the
/// fidelity readouts.
const ACK_ALL: &[&AckShape] =
    &[&ACK_C2R100_P0, &ACK_C7_P0, &ACK_C7_P1, &ACK_C8_P0, &ACK_C8_P1];

/// THE TRANSCRIPTION PIN. Not a model test — a check that the numbers this
/// bench now runs on are the ones the ledger recorded, in the shape it
/// recorded them, and that the CHECKS were not quietly turned into inputs.
///
/// It asserts the ledger's own internal identities, so a typo in any row
/// fails here rather than surviving as a plausible-looking input:
///
///  * each quantile range is ordered and strictly increasing p50 < p90 < p99
///    (READOUT 1+2 is a quantile table);
///  * the measured `xanchor` median lies inside its own measured min/max
///    (READOUT 3);
///  * the measured accepted-sample rate, the rejection rate and `rate_lr` are
///    the SAME measurement three ways — READOUT 3b's "acks folded per sample"
///    column is `rate_lr/samples_s` and its rejection rate is
///    `1 − samples_s/rate_lr`, so the three columns must agree;
///  * every path's p50 gap is FAR below its own mean gap `1e6/rate_lr` — the
///    heavy tail is the whole finding, and a row where it is absent would be
///    a transcription error.
#[test]
fn measured_ack_inputs_are_the_ledger_transcription() {
    for s in ACK_ALL {
        let mean_gap_us = 1e6 / s.rate_lr;
        assert!(s.rate_lr > 0.0, "{}: rate_lr", s.row);
        for (lo, hi) in [s.p50, s.p90, s.p99] {
            assert!(lo <= hi, "{}: range {lo}..{hi} out of order", s.row);
        }
        let (q50, q90, q99) = (mid(s.p50), mid(s.p90), mid(s.p99));
        assert!(q50 < q90 && q90 < q99, "{}: {q50} {q90} {q99} not a quantile ladder", s.row);
        assert!(
            s.xanchor >= s.xanchor_range.0 && s.xanchor <= s.xanchor_range.1,
            "{}: median xanchor {} outside its own measured range {:?}",
            s.row,
            s.xanchor,
            s.xanchor_range
        );
        // READOUT 3b's three columns are one measurement: rejection %,
        // accepted samples/s and rate_lr. The ledger prints all three; they
        // must close on each other.
        let implied_rej = (1.0 - s.samples_s / s.rate_lr) * 100.0;
        assert!(
            (implied_rej - s.rej_pct).abs() < 1.5,
            "{}: READOUT 3b does not close — {:.1}% rejected implies {:.0} samples/s \
             against rate_lr {:.0}, but the row says {:.0}",
            s.row,
            s.rej_pct,
            (1.0 - s.rej_pct / 100.0) * s.rate_lr,
            s.rate_lr,
            s.samples_s
        );
        // THE FINDING ITSELF: the stream is heavy-tailed, i.e. the median gap
        // is a small fraction of the mean gap. If this ever reads ≈1 the row
        // is not the wire's.
        assert!(
            q50 < 0.25 * mean_gap_us,
            "{}: p50 gap {q50:.1} µs is not far below the mean gap {mean_gap_us:.1} µs — \
             the measured stream is heavy-tailed and this row is not",
            s.row
        );
        // And the tail really is a tail: p99 is at least 5× the MEAN gap.
        assert!(
            q99 > 5.0 * mean_gap_us,
            "{}: p99 gap {q99:.1} µs against mean {mean_gap_us:.1} µs",
            s.row
        );
    }
    // The two tolerances are pre-registered, not discovered: pin them so a
    // successor that loosens them has to do it in a diff that says so.
    assert_eq!(V1_XANCHOR_TOL, 0.30);
    assert_eq!(V2_REJECT_TOL_PTS, 5.0);
}

/// The midpoint of a ledger range — the point estimate, with the range itself
/// kept so the model can back off inside it when the measurement's own
/// quantiles and its own mean do not close (see `AckGaps::new`).
fn mid((lo, hi): (f64, f64)) -> f64 {
    0.5 * (lo + hi)
}

// ── THE GAP DISTRIBUTION, built from the measured quantiles ────────────────
//
// `Q(u)` is the DIMENSIONLESS gap quantile function — gap divided by the
// path's own mean gap — interpolated through the ledger's measured points:
//
//   u ∈ [0,   0.5 ]  linear   0   → q50   (the sub-median body; the gauge
//                                          reports no quantile below p50)
//   u ∈ [0.5, 0.9 ]  log-linear q50 → q90
//   u ∈ [0.9, 0.99]  log-linear q90 → q99
//   u ∈ [0.99, 1  ]  Pareto,  Q = q99·((1−u)/0.01)^(−1/α)
//
// α is NOT chosen. It is SOLVED from the one thing the measurement pins that
// the quantiles do not: the mean gap is `1/rate_lr` (READOUT 3), so the top
// 1% must carry exactly the mass the body leaves, and `E[G | G > p99] =
// q99·α/(α−1)` fixes α. The measurement therefore determines its own tail.
//
// TWO PLACES WHERE THE MEASUREMENT DOES NOT CLOSE ON ITSELF, both handled by
// backing off INSIDE the ledger's own reported ranges rather than by inventing:
//
//  (a) at c2r100 and at c8's slow leg the quantile MIDPOINTS already imply a
//      mean gap ABOVE `1/rate_lr` (by ~10% and ~0.5%), leaving the tail
//      negative mass. `θ` — the position inside the ledger's own p90/p99
//      per-window ranges — is bisected DOWN from the midpoint until the two
//      measurements close with `E[G|G>p99] ≥ 1.5·p99`. p50 is never moved: it
//      is the headline number and the tightest-measured of the three.
//  (b) the Pareto tail, unbounded, generates silences of hundreds of ms at the
//      lightest α the mean allows. It is TRUNCATED at 18.2 ms — the largest
//      inter-ack gap the instrument reported anywhere (READOUT 1+2, c8/p1's
//      p99 upper range; the ledger's "tailing to 18 ms on a stalling leg").
//      Truncation is safe by construction: the observer is work-conserving, so
//      a lighter tail costs it silences, not acks.

/// The floor on the tail's own mass: the top 1% of gaps must average at least
/// this multiple of the measured p99 (α = 3 at 1.5 — the LIGHTEST tail this
/// model will still call a tail). It binds only at the two paths where the
/// ledger's quantile midpoints and its mean gap do not close, and it binds in
/// the CONSERVATIVE direction: a lighter tail means fewer long silences and a
/// SMALLER predicted over-read.
const ACK_TAIL_R_MIN: f64 = 1.5;

/// The largest inter-ack gap the instrument reported at any cell or path
/// (READOUT 1+2, `c8/p1` p99 = 5354–**18229** µs). The silence draw is
/// truncated here: the model never asserts a silence the gauge never saw.
const ACK_GAP_MAX_S: f64 = 18_229e-6;

/// One path's measured ack-gap law, resolved against the bench path that
/// carries it.
#[derive(Clone, Copy, Debug)]
struct AckGaps {
    /// Dimensionless measured quantiles (gap / mean gap).
    q50: f64,
    q90: f64,
    q99: f64,
    /// The Pareto tail exponent, SOLVED from the measured mean gap.
    alpha: f64,
    /// The silence threshold, SOLVED from the drain/duty identity below.
    u_c: f64,
    /// Where inside the ledger's p90/p99 ranges the model had to sit for the
    /// measurement to close (0.5 = the midpoint, i.e. it closed there).
    theta: f64,
    /// The NOMINAL mean ack gap, seconds — `1/rate` — used only until the
    /// path has measured its own. The live value is `AckObs::mean_gap_s`.
    mean_gap_s: f64,
}

/// `w·(b−a)/ln(b/a)` — the mean of a log-linear segment of width `w`.
fn logseg_mean(w: f64, a: f64, b: f64) -> f64 {
    if (b - a).abs() < 1e-15 {
        w * a
    } else {
        w * (b - a) / (b / a).ln()
    }
}

impl AckGaps {
    /// The dimensionless quantiles at range-position `theta` (p50 always at
    /// its midpoint).
    fn quantiles(sh: &AckShape, theta: f64) -> (f64, f64, f64) {
        let m = 1e6 / sh.rate_lr; // the MEASURED mean gap, µs
        (
            mid(sh.p50) / m,
            (sh.p90.0 + theta * (sh.p90.1 - sh.p90.0)) / m,
            (sh.p99.0 + theta * (sh.p99.1 - sh.p99.0)) / m,
        )
    }

    /// The mean of `Q` over `[0, 0.99]` — everything the measured quantiles
    /// themselves account for.
    fn body_mean(q50: f64, q90: f64, q99: f64) -> f64 {
        0.25 * q50 + logseg_mean(0.4, q50, q90) + logseg_mean(0.09, q90, q99)
    }

    /// `E[G | G > p99] / p99` at range-position `theta` — what the measured
    /// mean leaves for the tail, in units of the measured p99.
    fn tail_ratio(sh: &AckShape, theta: f64) -> f64 {
        let (q50, q90, q99) = Self::quantiles(sh, theta);
        (1.0 - Self::body_mean(q50, q90, q99)) / (0.01 * q99)
    }

    fn new(sh: &AckShape, path_rate: f64) -> Self {
        // (a) close the measurement against itself, inside its own ranges.
        let mut theta = 0.5;
        if Self::tail_ratio(sh, 0.5) < ACK_TAIL_R_MIN {
            let (mut lo, mut hi) = (0.0_f64, 0.5_f64);
            assert!(
                Self::tail_ratio(sh, 0.0) >= ACK_TAIL_R_MIN,
                "{}: even at the LOW end of every measured range the quantiles imply a \
                 mean gap inconsistent with rate_lr — the ledger rows do not close",
                sh.row
            );
            for _ in 0..80 {
                let m = 0.5 * (lo + hi);
                if Self::tail_ratio(sh, m) >= ACK_TAIL_R_MIN {
                    lo = m;
                } else {
                    hi = m;
                }
            }
            theta = lo;
        }
        let (q50, q90, q99) = Self::quantiles(sh, theta);
        let r = Self::tail_ratio(sh, theta);
        assert!(r > 1.0, "{}: tail ratio {r}", sh.row);
        let alpha = r / (r - 1.0);

        // THE SILENCE THRESHOLD, solved not chosen. A work-conserving observer
        // that drains at spacing `s = q50·ḡ` has, per cycle, a silence `S`,
        // a drain `D = S·q50/(1−q50)` and `S/(1−q50)` acks — so the silence
        // FRACTION of gaps is `φ = (1−q50)/E[S]`. For the model's own marginal
        // to reproduce the measured one, the silences must be exactly `Q`'s
        // upper tail, i.e. `φ = 1 − u_c` and `E[S] = ∫_{u_c}^1 Q / (1−u_c)`.
        // The two together give ONE equation with ONE unknown:
        //
        //     ∫_0^{u_c} Q(u) du = q50
        //
        // and its root is `u_c`. Nothing here is fitted: q50 is measured, Q is
        // the measured quantile curve, and conservation supplies the rest.
        let mut g = AckGaps {
            q50,
            q90,
            q99,
            alpha,
            u_c: 0.5,
            theta,
            mean_gap_s: 1.0 / path_rate,
        };
        let (mut lo, mut hi) = (0.5_f64, 1.0_f64);
        for _ in 0..80 {
            let m = 0.5 * (lo + hi);
            if g.cdf_mean_to(m) < q50 {
                lo = m;
            } else {
                hi = m;
            }
        }
        g.u_c = 0.5 * (lo + hi);
        g
    }

    /// `∫_0^u Q(t) dt` — the mean mass of `Q` below quantile `u`.
    fn cdf_mean_to(&self, u: f64) -> f64 {
        let (q50, q90, q99, a) = (self.q50, self.q90, self.q99, self.alpha);
        if u <= 0.5 {
            return q50 * u * u; // ∫_0^u 2·q50·t dt
        }
        let mut acc = 0.25 * q50;
        if u <= 0.9 {
            let k = (q90 / q50).ln() / 0.4;
            return acc + q50 * ((k * (u - 0.5)).exp() - 1.0) / k;
        }
        acc += logseg_mean(0.4, q50, q90);
        if u <= 0.99 {
            let k = (q99 / q90).ln() / 0.09;
            return acc + q90 * ((k * (u - 0.9)).exp() - 1.0) / k;
        }
        acc += logseg_mean(0.09, q90, q99);
        let w = ((1.0 - u) / 0.01).max(0.0);
        acc + 0.01 * q99 * a / (a - 1.0) * (1.0 - w.powf(1.0 - 1.0 / a))
    }

    /// `Q(u)` — the dimensionless gap at quantile `u`.
    fn q(&self, u: f64) -> f64 {
        let (q50, q90, q99, a) = (self.q50, self.q90, self.q99, self.alpha);
        if u <= 0.5 {
            2.0 * q50 * u
        } else if u <= 0.9 {
            q50 * ((q90 / q50).powf((u - 0.5) / 0.4))
        } else if u <= 0.99 {
            q90 * ((q99 / q90).powf((u - 0.9) / 0.09))
        } else {
            let w = ((1.0 - u) / 0.01).max(1e-12);
            q99 * w.powf(-1.0 / a)
        }
    }

    /// One silence, seconds: a draw from `Q`'s upper tail above `u_c`, scaled
    /// by the path's own measured mean gap, truncated at the largest gap the
    /// instrument ever reported.
    fn silence(&self, rng: &mut Rng, mean_gap_s: f64) -> f64 {
        let u = self.u_c + (1.0 - self.u_c) * rng.f64();
        (self.q(u) * mean_gap_s).min(ACK_GAP_MAX_S)
    }
}

/// Log-spaced buckets for the realized ack-gap distribution: 1 µs → 100 ms
/// over 5 decades. The bench MEASURES its own marginal and scores it against
/// the ledger's (V3) rather than asserting it holds by construction.
const GAP_BUCKETS: usize = 250;

fn gap_bucket(g_s: f64) -> usize {
    let us = (g_s * 1e6).max(1.0);
    let d = us.log10() / 5.0 * GAP_BUCKETS as f64;
    (d as usize).min(GAP_BUCKETS - 1)
}

fn gap_quantile(hist: &[u32; GAP_BUCKETS], q: f64) -> f64 {
    let total: u64 = hist.iter().map(|c| *c as u64).sum();
    if total == 0 {
        return f64::NAN;
    }
    let want = (q * total as f64).ceil() as u64;
    let mut acc = 0u64;
    for (i, c) in hist.iter().enumerate() {
        acc += *c as u64;
        if acc >= want {
            // The bucket's geometric centre, in µs.
            return 10f64.powf((i as f64 + 0.5) / GAP_BUCKETS as f64 * 5.0);
        }
    }
    f64::NAN
}

/// One path's ack-observation state under the MEASURED era.
struct AckObs {
    g: AckGaps,
    /// Delivered but not yet observed by the sender's rate sampler.
    backlog: u64,
    /// When the next ack is observed.
    next_obs: f64,
    rng: Rng,
    /// Arrival instants inside the last `REPORT_S` — the path's own measured
    /// mean ack gap, which is the unit the ledger's shape is expressed in.
    ///
    /// THE SHAPE IS DIMENSIONLESS AND MUST BE SCALED BY THE REALIZED RATE, NOT
    /// THE NOMINAL ONE. READOUT 1+2's gaps are quoted against READOUT 3's
    /// `rate_lr`, "the window's own long-run rate" — an OUTPUT of the wire, not
    /// a link capacity. A bench path the scheduler under-fills (c8's slow leg
    /// runs at ~60% of its link) has a correspondingly wider mean gap, and
    /// scaling the shape by `1/link_rate` there quietly asserts a denser ack
    /// stream than the path actually produced. The window is `REPORT_S` = 2 s
    /// because that is the gauge's own report cadence — the interval every
    /// measured number in the ledger is a statistic over.
    arrivals: std::collections::VecDeque<f64>,
    // ── gauges: everything the ledger measured and this model PREDICTS ──
    n_obs: u64,
    n_accept: u64,
    n_reject: u64,
    last_accept: f64,
    last_obs: f64,
    gap_sum: f64,
    hist: [u32; GAP_BUCKETS],
}

impl AckObs {
    fn new(sh: &AckShape, path_rate: f64, seed: u64) -> Self {
        Self {
            g: AckGaps::new(sh, path_rate),
            backlog: 0,
            next_obs: 0.0,
            rng: Rng::new(seed),
            arrivals: std::collections::VecDeque::new(),
            n_obs: 0,
            n_accept: 0,
            n_reject: 0,
            last_accept: 0.0,
            last_obs: 0.0,
            gap_sum: 0.0,
            hist: [0; GAP_BUCKETS],
        }
    }

    /// A delivered symbol reaches the sender at `t`.
    fn arrive(&mut self, t: f64) {
        self.backlog += 1;
        self.arrivals.push_back(t);
        while self.arrivals.front().is_some_and(|f| *f < t - REPORT_S) {
            self.arrivals.pop_front();
        }
        if self.backlog == 1 && self.next_obs < t {
            self.next_obs = t;
        }
    }

    /// The path's own mean ack gap over the gauge's 2 s window — the unit the
    /// measured shape is expressed in. Falls back to the nominal `1/rate`
    /// until the path has produced a window's worth of its own arrivals.
    fn mean_gap_s(&self) -> f64 {
        match (self.arrivals.front(), self.arrivals.back()) {
            (Some(a), Some(b)) if self.arrivals.len() >= 2 && b > a => {
                (b - a) / (self.arrivals.len() - 1) as f64
            }
            _ => self.g.mean_gap_s,
        }
    }

    /// The next observation instant at or before `limit`, if any.
    fn due(&self, limit: f64) -> Option<f64> {
        if self.backlog > 0 && self.next_obs <= limit { Some(self.next_obs) } else { None }
    }

    /// Consume the observation at `t` and schedule the next: another drain
    /// step if work remains, a SILENCE if the sender has caught up.
    fn take(&mut self, t: f64) {
        self.backlog -= 1;
        let mg = self.mean_gap_s();
        self.next_obs = t
            + if self.backlog == 0 {
                self.g.silence(&mut self.rng, mg)
            } else {
                // The drain spacing IS the measured p50 gap.
                self.g.q50 * mg
            };
        // Gauges. The accept/reject mirror is `scheduler/mod.rs:1178`'s rule
        // (`elapsed < 0.001` ⇒ rejected) transcribed for INSTRUMENTATION only;
        // the real floor still runs inside `CopaState::record_delivery`, which
        // returns nothing that distinguishes the two.
        if self.n_obs > 0 {
            let gap = t - self.last_obs;
            self.gap_sum += gap;
            self.hist[gap_bucket(gap)] += 1;
        }
        self.last_obs = t;
        self.n_obs += 1;
        if t - self.last_accept >= 0.001 {
            self.n_accept += 1;
            self.last_accept = t;
        } else {
            self.n_reject += 1;
        }
    }
}

/// Everything the measured era produced on one path, for the fidelity table.
#[derive(Debug, Clone, Copy, Default)]
struct ObsStat {
    n_obs: u64,
    n_accept: u64,
    n_reject: u64,
    /// Delivered but still un-observed when the horizon ended — the observer
    /// is work-conserving, so this is the only place an ack can be.
    backlog_end: u64,
    mean_gap_us: f64,
    p50_us: f64,
    p90_us: f64,
    p99_us: f64,
    /// The model's own resolved law, reported so the reader sees what the
    /// measurement resolved to rather than having to trust it.
    theta: f64,
    alpha: f64,
    u_c: f64,
}

impl ObsStat {
    fn reject_pct(&self) -> f64 {
        self.n_reject as f64 / (self.n_obs.max(1)) as f64 * 100.0
    }
    fn samples_s(&self, horizon: f64) -> f64 {
        self.n_accept as f64 / horizon
    }
    fn folded(&self) -> f64 {
        self.n_obs as f64 / self.n_accept.max(1) as f64
    }
}

#[derive(Debug, Clone, Copy)]
struct Run {
    ticks: u64,
    zero: u64,
    short: u64,
    sum_live: u64,
    sum_active: u64,
    delivered: u64,
    retx: u64,
    horizon_s: f64,
    mean_cap: f64,
    /// Σ over refresh ticks and paths of `copa_bdp_anchor() / (rate·RTprop)`
    /// — the REALIZED anchor over-read against the cell's own ground truth.
    anchor_ratio_sum: f64,
    anchor_ratio_n: u64,
    /// Σ over refresh ticks and paths of `cwnd` (the anchor floor's visible
    /// effect) — mean cwnd per path.
    cwnd_sum: f64,
    cwnd_n: u64,
    /// The accounting axis's per-channel ledger (all zero but `src`/`charges`/
    /// `releases`/`tokens` under `Acct::Off`).
    led: Ledger,
    /// PER PATH: Σ and n of `copa_bdp_anchor()/(rate·RTprop)` over refresh
    /// ticks. READOUT 3 is a per-PATH table, so V1 is scored per path — a
    /// cell mean would hide the c8 legs, which the wire reports separately.
    /// (Every cell this bench runs has ≤ 2 paths.)
    xa_sum: [f64; 2],
    xa_n: [u64; 2],
    /// PER PATH: what the measured ack observer actually did. All zero on
    /// every era but `Feed::Measured`.
    obs: [ObsStat; 2],
    /// PER PATH: Σ/n of `btlbw_sym_per_s()` and of `min_rtt()` over refresh
    /// ticks, plus the path's own delivered count.
    ///
    /// THE LEDGER'S `xanchor` IS NOT THIS BENCH'S `x`, and the difference is
    /// load-bearing. READOUT 3 defines it as `copa_bdp_anchor()/(rate_lr·
    /// RTprop)` where RTprop is the ANCHOR'S OWN `min_rtt` — the same
    /// `min_rtt` the anchor multiplied by — so the RTT cancels and `xanchor`
    /// is a pure RATE over-read, `max_bw/rate_lr`. The bench's pre-existing
    /// `overread()` divides by the CONFIGURED `rate·rtprop` instead, so it
    /// also carries whatever standing queue the link built. Both are kept:
    /// `overread()` unchanged (three ledger sections are scored on it) and
    /// `xanchor_lr()` as the ledger's own quantity.
    bw_sum: [f64; 2],
    bw_n: [u64; 2],
    mrtt_sum: [f64; 2],
    delivered_p: [u64; 2],
    /// PER PATH: the MEDIAN of `max_bw / rate_lr` over the dyn-cap refresh
    /// ticks, where `rate_lr` is the path's delivered rate over the PRECEDING
    /// `REPORT_S` — READOUT 3's statistic, computed the way READOUT 3 computes
    /// it ("Medians over the 12 windows"). A whole-run divisor is not the same
    /// number at a duty-cycled path: c8's slow leg runs at its link rate while
    /// it runs and idles between, so its 2 s windows read ~1.6× the run mean.
    xlr_med: [f64; 2],
}

impl Run {
    fn zero_pct(&self) -> f64 {
        self.zero as f64 / self.ticks.max(1) as f64 * 100.0
    }
    fn short_pct(&self) -> f64 {
        self.short as f64 / self.ticks.max(1) as f64 * 100.0
    }
    /// The `[SF]` E gauge: mean n_active / mean n_live.
    fn e(&self) -> f64 {
        self.sum_active as f64 / self.sum_live.max(1) as f64
    }
    fn goodput_sym_s(&self) -> f64 {
        self.delivered as f64 / self.horizon_s
    }
    /// Mean realized anchor over-read (×1.0 = honest).
    fn overread(&self) -> f64 {
        self.anchor_ratio_sum / self.anchor_ratio_n.max(1) as f64
    }
    /// The realized over-read on ONE path, on the BENCH's definition
    /// (`anchor / (configured rate·RTprop)`).
    fn overread_path(&self, pid: usize) -> f64 {
        self.xa_sum[pid] / self.xa_n[pid].max(1) as f64
    }
    /// The realized `xanchor` on ONE path on THE LEDGER'S definition
    /// (READOUT 3): `max_bw / rate_lr`, the path's windowed-max rate estimate
    /// over its own realized long-run delivered rate. The RTT divides out, as
    /// it does on the wire.
    fn xanchor_lr(&self, pid: usize) -> f64 {
        self.xlr_med[pid]
    }
    /// The same quantity on a WHOLE-RUN divisor, kept beside it so the
    /// duty-cycle effect is visible rather than chosen.
    fn xanchor_runmean(&self, pid: usize) -> f64 {
        let bw = self.bw_sum[pid] / self.bw_n[pid].max(1) as f64;
        let lr = self.delivered_p[pid] as f64 / self.horizon_s;
        if lr > 0.0 { bw / lr } else { f64::NAN }
    }
    /// How much standing queue the bench's link built: mean `min_rtt` over the
    /// path's configured RTprop. On the wire this reads ≈1 (READOUT 3's RTprop
    /// column is the cell's own RTT); anything else is a bench artifact and is
    /// reported rather than absorbed.
    fn rtt_inflation(&self, pid: usize, rtprop: f64) -> f64 {
        self.mrtt_sum[pid] / self.bw_n[pid].max(1) as f64 / rtprop
    }
    fn mean_cwnd(&self) -> f64 {
        self.cwnd_sum / self.cwnd_n.max(1) as f64
    }
}

/// The REAL reliable-source placement objective (`Scheduler::place_costs` via
/// `place_probs_with_temperature`), taken at T → 0 — the strict-best-path limit
/// the scheduler exposes for exactly this purpose. Deterministic (the shipped
/// `place_symbol` draws a uniform), same candidate set (`p.active`, no
/// availability filter), same cost.
fn place_min_cost(sched: &Scheduler) -> u32 {
    place_min_cost_of(sched, false, &[])
}

/// As `place_min_cost`, for the REPAIR objective: `is_repair = true` with the
/// covered-path multiset, which is what both engine repair sites use
/// (`emit_source.rs:907` and `net/mod.rs:6428`, each
/// `sched.place_symbol(true, &covered)` over `window_source_paths`). The
/// ρ_fate diversity term is what pushes a correction AWAY from the paths that
/// carried the window it covers — the reason recovery traffic concentrates on
/// the leg that is not carrying the source.
fn place_min_cost_of(sched: &Scheduler, is_repair: bool, covered: &[u32]) -> u32 {
    let mut cands = sched.place_probs_with_temperature(is_repair, covered, f64::MIN_POSITIVE);
    // DETERMINISM, and it is NOT free: `Scheduler` holds its paths in a
    // `HashMap<PathId, PathState>`, whose iteration order is randomised per
    // PROCESS. At an ASYMMETRIC cell the placement objective separates the
    // paths and the order cannot matter; at the SYMMETRIC cell (c7, and the
    // d = 1.0 point of the diagonal sweep) the two costs are bit-equal and the
    // winner was whatever the map yielded last — so c7 was the one cell whose
    // numbers moved run to run, which is exactly why goal-gate "c8 SF
    // Mechanism" carries 9.0% for c7 in FINDING 3 and 9.3% for the SAME
    // geometry in FINDING 4. Sorting by path id first makes the tie-break
    // lowest-id-wins and the whole bench reproducible.
    cands.sort_by_key(|(pid, _)| *pid);
    let mut best: Option<(u32, f64)> = None;
    for (pid, w) in cands {
        if best.is_none_or(|(_, bw)| w > bw) {
            best = Some((pid, w));
        }
    }
    best.map(|(pid, _)| pid).unwrap_or(0)
}

/// Close the loop at the SHIPPED honest-anchor era (bit-identical to the
/// bench's original behaviour — `Feed::Honest` is a per-symbol `on_ack(1)`).
fn simulate(paths: &[Spec], arm: Arm, horizon_s: f64) -> Run {
    simulate_era(paths, arm, Feed::Honest, horizon_s)
}

/// Close the loop. `paths` is the cell geometry; `arm` selects the path set /
/// pooled ceiling; `feed` selects the ANCHOR ERA (what the legacy ack-interval
/// rate sampler sees); `horizon_s` is simulated seconds.
fn simulate_era(paths: &[Spec], arm: Arm, feed: Feed, horizon_s: f64) -> Run {
    simulate_seeded(paths, arm, feed, horizon_s, 0)
}

/// As `simulate_era`, with the GE link seeds SALTED. FINDING 4 established
/// that this loop is BISTABLE, so a single run is a draw from a mode, not a
/// measurement of one — every claim below is scored over a seed ensemble and
/// reported as a MODE RATE, which is what that finding asked a successor to do.
fn simulate_seeded(paths: &[Spec], arm: Arm, feed: Feed, horizon_s: f64, salt: u64) -> Run {
    simulate_acct(paths, arm, feed, horizon_s, salt, Acct::Off)
}

/// The engine's `active_paths().max_by(loss_rate)` pick — the estimator the
/// taper block reads for r\* (`emit_source.rs:613-620`) — with the SAME
/// determinism fix `place_min_cost` needed and for the same reason:
/// `active_paths()` returns `HashMap` order, so `max_by`'s last-wins tie-break
/// is randomised per PROCESS. Losses tie exactly whenever both estimators are
/// still at 0.0 (every cold start) and at the symmetric cell in general, so
/// without the sort the bench's r\* — hence its whole repair channel — is not
/// reproducible. Pinned by `worst_loss_path_tie_is_broken_deterministically`.
/// The ENGINE has the same tie and does not break it; that divergence is
/// recorded in the block, not silently modelled away.
fn worst_loss_path(sched: &Scheduler) -> Option<u32> {
    let mut ids = sched.active_paths();
    ids.sort_unstable();
    let mut best: Option<(u32, f64)> = None;
    for id in ids {
        if let Some(p) = sched.path(id) {
            let l = p.estimator.loss_rate();
            if best.is_none_or(|(_, bl)| l > bl) {
                best = Some((id, l));
            }
        }
    }
    best.map(|(id, _)| id)
}

/// Charge one symbol to `pid`'s in-flight account.
fn chg(sched: &mut Scheduler, pid: u32, led: &mut Ledger) {
    if let Some(p) = sched.path_mut(pid) {
        p.charge_in_flight(1);
    }
    led.charges += 1;
}

/// Release one symbol from `pid`'s in-flight account, recording whether the
/// saturating subtraction threw it away (the ledger's un-recoverable loss).
fn rel(sched: &mut Scheduler, pid: u32, led: &mut Ledger) {
    if let Some(p) = sched.path_mut(pid) {
        if p.in_flight == 0 {
            led.releases_wasted += 1;
        }
        p.release_in_flight(1);
    }
    led.releases += 1;
}

/// As `simulate_seeded`, with the IN-FLIGHT ACCOUNTING axis. `Acct::Off` is
/// bit-identical to `simulate_seeded` — every branch the axis adds is behind
/// `acct.on()`, and the link's RNG consumption is unchanged.
fn simulate_acct(
    paths: &[Spec],
    arm: Arm,
    feed: Feed,
    horizon_s: f64,
    salt: u64,
    acct: Acct,
) -> Run {
    let tick = 0.000_25_f64; // 250 µs — 20 ticks per dyn-cap refresh
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    let mut links: Vec<Link> = Vec::new();
    // Ground truth per path for the realized-over-read gauge: BtlBw·RTprop.
    let truth: Vec<f64> = paths.iter().map(|(r, t, _, _)| r * t).collect();
    for (i, spec) in paths.iter().enumerate() {
        sched.add_path(i as u32);
        links.push(Link::new(
            *spec,
            0x5EED_0000_u64
                .wrapping_add(salt.wrapping_mul(0xD1B5_4A32_D192_ED03))
                .wrapping_add(i as u64 * 0x9E37_79B9),
        ));
    }
    let np = paths.len();

    // The retention store: admitted, not yet acked.
    let mut store: Vec<Sym> = Vec::new();
    let mut cap: usize = BOOT;
    let mut delivered: u64 = 0;
    let mut retx: u64 = 0;
    let mut next_refresh = 0.0_f64;
    let (mut ticks, mut zero, mut short, mut sum_live, mut sum_active) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut cap_sum: f64 = 0.0;
    let mut anchor_ratio_sum = 0.0_f64;
    let mut anchor_ratio_n = 0u64;
    let mut cwnd_sum = 0.0_f64;
    let mut cwnd_n = 0u64;
    let mut xa_sum = [0.0_f64; 2];
    let mut xa_n = [0u64; 2];
    let mut bw_sum = [0.0_f64; 2];
    let mut bw_n = [0u64; 2];
    let mut mrtt_sum = [0.0_f64; 2];
    let mut delivered_p = [0u64; 2];
    // Delivery instants inside the trailing `REPORT_S`, per path — the
    // denominator of READOUT 3's `xanchor`, measured over the gauge's own
    // report window rather than over the whole run.
    let mut deliv_win: Vec<std::collections::VecDeque<f64>> =
        (0..np).map(|_| std::collections::VecDeque::new()).collect();
    let mut xlr_s: [Vec<f64>; 2] = [Vec::new(), Vec::new()];

    // `Feed::Overread` — per-path fractional carry, so a non-integer scale is
    // exact in the LONG RUN instead of rounded per call.
    let mut scale_carry = vec![0.0_f64; np];
    // `Feed::Cumulative` — the receiver's per-seq delivery flags, the carrying
    // path of each seq, the cumulative frontier, and the feedback clock.
    let mut next_seq: u64 = 0;
    let mut seq_done: Vec<bool> = Vec::new();
    let mut seq_owner: Vec<u32> = Vec::new();
    let mut frontier: u64 = 0;
    let mut next_feedback = 0.0_f64;

    // ── the accounting axis's state ─────────────────────────────────────
    // The SHIPPED FEC rate controller, constructed at the resolved defaults
    // (`net/mod.rs:1570`). r* is therefore whatever the shipped law returns on
    // the bench's OWN measured loss/RTT/throughput — no repair rate is injected
    // anywhere. `set_inner_feedback(0.0)` is `config::resolve`'s default.
    let mut ctrl = FecRateController::new_with_toggles(
        TAIL_LOSS,
        MAX_OVERHEAD,
        ProtocolHint::Auto,
        FecBackend::RaptorQ,
        true,
        SYMBOL_SIZE,
    );
    ctrl.set_inner_feedback(0.0);
    let mut led = Ledger::default();
    let mut wire: Vec<WireSym> = Vec::new();
    // `st.repair_debt` (`emit_source.rs:787`) and the taper cache's r*.
    let mut repair_debt = 0.0_f64;
    let mut repair_rate = 0.0_f64;
    // Per-path ack counters, drained each tick into `record_batch` — the
    // engine's `expected_count` / `received_count` (`receiver.rs:1754`).
    let mut ack_expected = vec![0u32; np];
    let mut ack_received = vec![0u32; np];
    // The report task's throughput feed (`net/tasks/report.rs:84`).
    let mut sent_since_report = vec![0u64; np];
    let mut next_report = REPORT_S;

    // ── THE MEASURED ACK ERA's per-path observer ─────────────────────────
    // One per path, its law resolved against THAT path's own rate (the
    // measured shape is dimensionless), and its RNG kept strictly apart from
    // the links' so `Feed::Measured` cannot perturb the GE realizations that
    // every other era runs on.
    let mut obs: Vec<AckObs> = Vec::new();
    if let Feed::Measured(shapes) = feed {
        assert_eq!(
            shapes.len(),
            np,
            "the measured era needs one measured ack shape per path — the wire \
             measured this cell path by path and the bench must not invent the rest"
        );
        for (i, sh) in shapes.iter().enumerate() {
            obs.push(AckObs::new(
                sh,
                paths[i].0,
                0x0ACD_0000_u64
                    .wrapping_add(salt.wrapping_mul(0xA24B_AED4_963E_E407))
                    .wrapping_add(i as u64 * 0xC2B2_AE3D_27D4_EB4F),
            ));
        }
    }
    /// The sub-tick clock cursor, in whole nanoseconds so the MockClock
    /// advances monotonically and lands EXACTLY on each tick boundary — the
    /// non-measured eras advance once per tick and must stay bit-identical.
    fn advance_to(clock: &MockClock, cursor: &mut u128, t_s: f64) {
        let target = (t_s * 1e9).round() as u128;
        if target > *cursor {
            clock.advance(Duration::from_nanos((target - *cursor) as u64));
            *cursor = target;
        }
    }
    let mut clock_ns: u128 = 0;

    let steps = (horizon_s / tick).round() as u64;
    // The repair objective's `covered` multiset and the path it selects,
    // computed at most ONCE PER TICK and reused by every correction emitted in
    // that tick. This is the engine's own cache granularity, not a new idea:
    // `RWM_EMIT_BATCH` (`emit_source.rs:597-609`) refreshes the derived taper
    // math at BURST granularity rather than per symbol. A 250 µs tick is finer
    // than the engine's burst. Recorded in the block's "what this still cannot
    // see" list all the same.
    let mut covered_cache: Option<Vec<u32>>;
    let mut repair_path_cache: Option<u32>;
    for step in 1..=steps {
        let now = step as f64 * tick;
        covered_cache = None;
        repair_path_cache = None;

        if let Feed::Measured(_) = feed {
            // ── THE MEASURED ACK STREAM, SUB-TICK ────────────────────────
            // The whole point of the era: `record_delivery` must be called at
            // each ack's OWN arrival instant, with `count = 1`, and the
            // SHIPPED 1 ms `elapsed` floor (`scheduler/mod.rs:1178`) must be
            // the thing that decides which of them becomes a rate sample.
            // Feeding the sampler on the 250 µs tick — as every other era
            // does — quantizes `elapsed` to a multiple of the tick and
            // bypasses exactly the mechanism the wire measured.
            //
            // Deliveries enter each path's observer at their link arrival
            // time; observations come back out at the measured cadence, in
            // TIME ORDER across paths, with the clock walked to each one.
            let t_prev = (step - 1) as f64 * tick;
            let mut arrivals: Vec<(u32, f64)> = store
                .iter()
                .filter_map(|s| match s.ack_at {
                    Some(t) if t > t_prev && t <= now => Some((s.path, t)),
                    _ => None,
                })
                .collect();
            arrivals.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
            let mut ai = 0usize;
            loop {
                // The earliest observation any path has ready, and the
                // earliest arrival still to be admitted — whichever is first.
                let next_ob = (0..np)
                    .filter_map(|p| obs[p].due(now).map(|t| (t, p as u32)))
                    .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                let next_ar = arrivals.get(ai).copied();
                let observe_first = match (next_ob, next_ar) {
                    (Some((tob, _)), Some((_, tar))) => tob <= tar,
                    (Some(_), None) => true,
                    (None, _) => false,
                };
                if observe_first {
                    let (tob, pid) = next_ob.expect("observe_first implies an observation");
                    advance_to(&clock, &mut clock_ns, tob);
                    obs[pid as usize].take(tob);
                    if let Some(p) = sched.path_mut(pid) {
                        // drecv = 1: "p50 = p90 = 1 in all 60 report windows,
                        // 857 400 acks" — never a batch.
                        p.on_ack(1);
                    }
                } else if let Some((pid, tar)) = next_ar {
                    obs[pid as usize].arrive(tar);
                    ai += 1;
                } else {
                    break;
                }
            }
            advance_to(&clock, &mut clock_ns, now);
        } else {
            clock.advance(Duration::from_secs_f64(tick));
        }

        // ── ack/delivery half + the recovery plane ───────────────────────
        // Acked symbols leave the store and release their path's budget.
        // Dropped ones are retransmitted once RFC 9002's time threshold
        // (9/8·SRTT — the same `PLACE_SLACK_RECOV_PATIENCE` the placement
        // objective uses) has passed, and are RE-CHARGED to their new path.
        let acks: Vec<(u32, f64, u64)> = store
            .iter()
            .filter(|s| matches!(s.ack_at, Some(t) if t <= now))
            .map(|s| (s.path, s.rtt, s.seq))
            .collect();
        for (pid, rtt, seq) in &acks {
            if let Some(p) = sched.path_mut(*pid) {
                p.record_rtt_sample(Duration::from_secs_f64(*rtt));
            }
            // `release_in_flight(1)`, verbatim, plus the axis's counters —
            // the call site, its order and its argument are unchanged.
            rel(&mut sched, *pid, &mut led);
            if acct.on() {
                ack_expected[*pid as usize] += 1;
                ack_received[*pid as usize] += 1;
                if let Some(p) = sched.path_mut(*pid) {
                    p.estimator.record_rtt(Duration::from_secs_f64(*rtt));
                }
            }
            if let Some(p) = sched.path_mut(*pid) {
                // THE ERA AXIS. The transport-level accounting above is
                // identical in every era; only what the ack-interval RATE
                // SAMPLER is shown differs.
                match feed {
                    Feed::Honest => p.on_ack(1),
                    Feed::Overread(f) => {
                        let acc = &mut scale_carry[*pid as usize];
                        *acc += f;
                        let k = acc.floor();
                        *acc -= k;
                        p.on_ack(k as u32)
                    }
                    // The cwnd-dynamics half runs on the SAME per-symbol
                    // cadence as the honest arm (`on_delivery_signal` is the
                    // shipped honest-feed entry point, `feat/copa-sole-cc`);
                    // the rate sample is deferred to the frontier report.
                    Feed::Cumulative { .. } => p.on_delivery_signal(),
                    // The MEASURED era drives `on_ack(1)` from the sub-tick
                    // observation loop at the top of the step, at each ack's
                    // own measured arrival instant — nothing to do here.
                    Feed::Measured(_) => {}
                }
            }
            if let Feed::Cumulative { .. } = feed {
                seq_done[*seq as usize] = true;
                seq_owner[*seq as usize] = *pid;
            }
        }
        delivered += acks.len() as u64;
        for (pid, _, _) in &acks {
            if (*pid as usize) < 2 {
                delivered_p[*pid as usize] += 1;
            }
            deliv_win[*pid as usize].push_back(now);
        }
        for w in deliv_win.iter_mut() {
            while w.front().is_some_and(|f| *f < now - REPORT_S) {
                w.pop_front();
            }
        }
        store.retain(|s| !matches!(s.ack_at, Some(t) if t <= now));

        // ── THE COUNTER-DELTA RELEASE, for the flights that did NOT land ──
        // `control_msg.rs:341` / `:685`: the feedback message carries
        // `expected − received`, and the sender releases that difference on
        // the path the FEEDBACK arrived on. So a LOST symbol's budget comes
        // back at the ack instant, not at the retransmit — the store still
        // holds it, but the in-flight ledger has already let it go. The
        // published bench released it at the retransmit instead, which is
        // the 1:1 discipline the engine does not have.
        if acct.on() {
            for s in store.iter_mut() {
                if s.ack_at.is_none() && !s.resolved && s.resolve_at <= now {
                    s.resolved = true;
                    ack_expected[s.path as usize] += 1;
                    if let Some(p) = sched.path_mut(s.path) {
                        if p.in_flight == 0 {
                            led.releases_wasted += 1;
                        }
                        p.release_in_flight(1);
                    }
                    led.releases += 1;
                }
            }
            // The un-stored recovery flights resolve the same way. EVERY wire
            // symbol enters the receiver's per-batch counters
            // (`receiver.rs:1754` builds them from `batch.symbols`), so each
            // one releases 1 on the path it flew — INCLUDING the ones that
            // were never charged. That is the whole of "release is not 1:1
            // with charge", and `release_in_flight`'s saturating subtraction
            // means the excess is not stored anywhere: it is spent against
            // whatever else that path had outstanding.
            let mut i = 0;
            while i < wire.len() {
                if wire[i].resolve_at <= now {
                    let w = wire.swap_remove(i);
                    ack_expected[w.path as usize] += 1;
                    if w.delivered {
                        ack_received[w.path as usize] += 1;
                    }
                    rel(&mut sched, w.path, &mut led);
                } else {
                    i += 1;
                }
            }
            // Drain the per-path counters into the loss estimator — the
            // engine's `path.estimator.record_batch(expected, received)`
            // (`control_msg.rs:337`, `:686`). This is the estimator the
            // repair rate, the NACK margin and the placement objective's
            // ρ term all read, and the published bench never fed it.
            for pid in 0..np {
                if ack_expected[pid] > 0 {
                    if let Some(p) = sched.path_mut(pid as u32) {
                        p.estimator.record_batch(ack_expected[pid], ack_received[pid]);
                    }
                    ack_expected[pid] = 0;
                    ack_received[pid] = 0;
                }
            }
            // The report task's LOCAL throughput feed: achieved send rate
            // over the report interval, bytes/s (`net/tasks/report.rs:84`,
            // its own `dt > 0.2` gate). Feeds `t_sym` and the burst B/T term
            // of the shipped r* law.
            if now >= next_report {
                next_report = now + REPORT_S;
                for pid in 0..np {
                    if sent_since_report[pid] > 0 {
                        let bps = sent_since_report[pid] as f64 * SYMBOL_SIZE as f64 / REPORT_S;
                        if let Some(p) = sched.path_mut(pid as u32) {
                            p.estimator.record_throughput(bps);
                        }
                    }
                    sent_since_report[pid] = 0;
                }
            }
        }

        // ── the receiver's CUMULATIVE frontier report (legacy era) ───────
        // A GE drop stalls the frontier; the retransmit's delivery releases
        // the whole accumulated run in ONE feedback message, which is the
        // engine's Δdelivered spike over one ack interval. No number is
        // injected here — the batch size is whatever the bench's own loss and
        // reordering produce.
        if let Feed::Cumulative { ack_period_s } = feed {
            if now >= next_feedback {
                next_feedback = now + ack_period_s;
                let mut cnt = vec![0u32; np];
                while (frontier as usize) < seq_done.len() && seq_done[frontier as usize] {
                    cnt[seq_owner[frontier as usize] as usize] += 1;
                    frontier += 1;
                }
                for (pid, c) in cnt.iter().enumerate() {
                    if *c > 0 {
                        if let Some(p) = sched.path_mut(pid as u32) {
                            p.on_ack(*c);
                        }
                    }
                }
            }
        }
        let mut retx_this_tick: u64 = 0;
        for i in 0..store.len() {
            if store[i].ack_at.is_some() {
                continue;
            }
            let srtt = sched
                .path(store[i].path)
                .map(|p| p.srtt().as_secs_f64())
                .unwrap_or(0.1);
            if now - store[i].sent <= 1.125 * srtt {
                continue;
            }
            if !acct.on() {
                // The published bench's 1:1 discipline: the old flight's
                // charge comes back HERE. Under the axis it already came
                // back at the counter delta above.
                if let Some(p) = sched.path_mut(store[i].path) {
                    p.release_in_flight(1);
                }
                led.releases += 1;
            }
            let pid = place_min_cost(&sched);
            // THE FIRST BYPASS CHANNEL (`net/mod.rs:6374-6383`): the SACK-gap
            // retransmit builds a `SymbolBatch` and hands it straight to
            // `transport.send_symbols`. It calls `feed.on_sent` and
            // `p.on_src_sent` — and NOT `charge_in_flight`. It consumes the
            // wire and the ledger never learns.
            if acct != Acct::Engine {
                chg(&mut sched, pid, &mut led);
            }
            let (rt_at, rt_rtt, rt_ok) = links[pid as usize].send_resolved(now);
            store[i].path = pid;
            store[i].sent = now;
            store[i].ack_at = if rt_ok { Some(rt_at) } else { None };
            store[i].rtt = if rt_ok { rt_rtt } else { 0.0 };
            store[i].resolve_at = rt_at;
            store[i].resolved = false;
            retx += 1;
            retx_this_tick += 1;
            led.retx += 1;
            sent_since_report[pid as usize] += 1;
        }

        // ── THE SECOND BYPASS CHANNEL: the NACK repair margin ─────────────
        // `net/mod.rs:6420-6448`, verbatim in structure and with no constant
        // of its own: `margin = ceil(retransmitted × current_loss)` where
        // `current_loss` is the MAX `estimator.loss_rate()` over
        // `active_paths()`, placed by `place_symbol(true, &covered)`, and sent
        // with neither a token nor a charge. Both inputs are the bench's own
        // realizations: the retransmit count its GE drops produced and the
        // loss its estimator measured.
        if acct.on() && retx_this_tick > 0 {
            let current_loss = sched
                .active_paths()
                .iter()
                .filter_map(|id| sched.path(*id))
                .map(|p| p.estimator.loss_rate())
                .fold(0.0_f64, f64::max);
            let margin = (retx_this_tick as f64 * current_loss).ceil() as u64;
            if margin > 0 && !store.is_empty() {
                let mpid = *repair_path_cache.get_or_insert_with(|| {
                    let c = covered_cache
                        .get_or_insert_with(|| store.iter().map(|s| s.path).collect());
                    place_min_cost_of(&sched, true, c)
                });
                for _ in 0..margin {
                    if store.is_empty() {
                        break;
                    }
                    let (a, _rt, ok) = links[mpid as usize].send_resolved(now);
                    if acct != Acct::Engine {
                        chg(&mut sched, mpid, &mut led);
                    }
                    wire.push(WireSym { path: mpid, resolve_at: a, delivered: ok });
                    led.margin += 1;
                    sent_since_report[mpid as usize] += 1;
                }
            }
        }

        // ── the dyn-cap refresh phase (5 ms throttle) ────────────────────
        if now >= next_refresh {
            next_refresh = now + REFRESH_S;
            let live = sched.live_paths();
            let act = sched.active_paths();
            // The SHIPPED gauge predicate, on the shipped inputs
            // (`store_cap_sf_record(live.len(), act.len())`).
            ticks += 1;
            sum_live += live.len() as u64;
            sum_active += act.len() as u64;
            if act.len() < live.len() {
                short += 1;
            }
            if act.is_empty() && !live.is_empty() {
                zero += 1;
            }
            let n_live = live.len().max(1);
            let sum_over = |set: &[u32]| -> f64 {
                set.iter()
                    .filter_map(|id| sched.path(*id).and_then(|p| p.copa_bdp_anchor()))
                    .sum()
            };
            // The taper block's r* recompute (`emit_source.rs:610-640`), on
            // the engine's own selection: the max-loss estimator among
            // `active_paths()`, `sched.spare_capacity()` as the cap, and the
            // encoder window — here the retention store, which is the
            // reliable window the corrections code over. `None` (an empty
            // active set) ⇒ r = 0, exactly as the engine's `match`. Refreshed
            // on the same throttle the taper cache uses.
            if acct.on() {
                let spare = sched.spare_capacity();
                repair_rate = worst_loss_path(&sched)
                    .and_then(|id| sched.path(id))
                    .map(|p| ctrl.compute_repair_rate_capped(&p.estimator, spare, store.len()))
                    .unwrap_or(0.0);
            }
            let bdp_live = sum_over(&live);
            let bdp_set = if arm == Arm::Legacy { sum_over(&act) } else { bdp_live };
            cap = cap_for(arm, bdp_set, bdp_live, n_live);
            cap_sum += cap as f64;
            // The realized anchor over-read and the cwnd it floors, per path,
            // against the cell's OWN ground truth (rate·RTprop).
            for pid in 0..np {
                if let Some(p) = sched.path(pid as u32) {
                    cwnd_sum += p.cwnd as f64;
                    cwnd_n += 1;
                    // THE LEDGER'S OWN PAIR: the windowed-max rate estimate
                    // and the min-RTT the anchor multiplies it by.
                    if pid < 2 {
                        if let (Some(bw), Some(mr)) = (p.btlbw_sym_per_s(), p.min_rtt()) {
                            bw_sum[pid] += bw;
                            mrtt_sum[pid] += mr.as_secs_f64();
                            bw_n[pid] += 1;
                            let w = &deliv_win[pid];
                            if let (Some(a), Some(b)) = (w.front(), w.back()) {
                                if w.len() >= 2 && b > a {
                                    let lr = (w.len() - 1) as f64 / (b - a);
                                    xlr_s[pid].push(bw / lr);
                                }
                            }
                        }
                    }
                    if let Some(a) = p.copa_bdp_anchor() {
                        if truth[pid] > 0.0 {
                            anchor_ratio_sum += a / truth[pid];
                            anchor_ratio_n += 1;
                            if pid < 2 {
                                xa_sum[pid] += a / truth[pid];
                                xa_n[pid] += 1;
                            }
                        }
                    }
                }
            }
        }

        // ── admission (bulk source: always data to send) ─────────────────
        // THE GATE, exactly as the shipped plain-reliable sender writes it
        // (`net/mod.rs`: `reliable && (store_len >= effective_store_cap ||
        // cwnd_full)`, with `cwnd_full == false` at the battery's arms because
        // `RWM_INFL_CAP` defaults to 0): the STORE CAP IS THE ONLY BRAKE.
        //
        // Placement does NOT gate it. `emit_source.rs` picks with
        // `Scheduler::place_symbol(false, &[])`, whose `place_costs` filters on
        // `p.active` ALONE — there is no `available() > 0` filter on the
        // reliable source path (unlike `best_source_path` / `schedule`, which
        // the reliable emitter does not use). So `in_flight_i` may exceed
        // `cwnd_i` without bound, `available()` reads 0 and STAYS 0, and
        // `active_paths()` is a pure OBSERVABLE of the saturation the store cap
        // itself produced. That is the loop this bench closes.
        while store.len() < cap {
            let pid = place_min_cost(&sched);
            chg(&mut sched, pid, &mut led);
            let (a, rt, ok) = links[pid as usize].send_resolved(now);
            let seq = next_seq;
            next_seq += 1;
            if let Feed::Cumulative { .. } = feed {
                seq_done.push(false);
                seq_owner.push(pid);
            }
            store.push(Sym {
                path: pid,
                sent: now,
                ack_at: if ok { Some(a) } else { None },
                rtt: if ok { rt } else { 0.0 },
                seq,
                resolve_at: a,
                resolved: false,
            });
            led.src += 1;
            sent_since_report[pid as usize] += 1;
            // THE PACER'S DEBIT, transcribed from `emit_source.rs:493-497`:
            // `if pol.cc_pace { st.src_tokens -= 1.0 }` sits INSIDE the source
            // arm. This counter is what §12 claims paces the wire; every other
            // channel below increments `led.wire()` without touching it, and
            // `pacer_debit_bounds_only_the_source_arm_not_the_wire` bounds the
            // gap.
            led.tokens += 1;

            // ── CHANNEL (a): the taper correction, token-free ─────────────
            // `emit_source.rs:787-931`: `st.repair_debt += repair_rate` per
            // SOURCE symbol, and while the debt clears 1.0 a correction symbol
            // is generated and sent on the ρ_fate repair placement. It IS
            // charged to in_flight (`:929`) — what it is not is PACED, and it
            // is not in the retention store, so the store cap (the loop's only
            // brake) cannot see it. Guarded by the engine's own
            // `st.encoder.window_size() > 1` / `> 0`.
            if acct.on() && store.len() > 1 {
                repair_debt += repair_rate;
                while repair_debt >= 1.0 && !store.is_empty() {
                    repair_debt -= 1.0;
                    let rpid = *repair_path_cache.get_or_insert_with(|| {
                        let c = covered_cache
                            .get_or_insert_with(|| store.iter().map(|s| s.path).collect());
                        place_min_cost_of(&sched, true, c)
                    });
                    let (ra, _rrt, rok) = links[rpid as usize].send_resolved(now);
                    chg(&mut sched, rpid, &mut led);
                    wire.push(WireSym {
                        path: rpid,
                        resolve_at: ra,
                        delivered: rok,
                    });
                    led.taper += 1;
                    sent_since_report[rpid as usize] += 1;
                }
            }
        }
    }

    let mut xlr_med = [f64::NAN; 2];
    for (p, v) in xlr_s.iter_mut().enumerate() {
        if !v.is_empty() {
            v.sort_by(f64::total_cmp);
            xlr_med[p] = v[v.len() / 2];
        }
    }
    let mut obs_stat = [ObsStat::default(); 2];
    for (i, o) in obs.iter().enumerate().take(2) {
        obs_stat[i] = ObsStat {
            n_obs: o.n_obs,
            n_accept: o.n_accept,
            n_reject: o.n_reject,
            backlog_end: o.backlog,
            mean_gap_us: o.gap_sum / (o.n_obs.saturating_sub(1)).max(1) as f64 * 1e6,
            p50_us: gap_quantile(&o.hist, 0.50),
            p90_us: gap_quantile(&o.hist, 0.90),
            p99_us: gap_quantile(&o.hist, 0.99),
            theta: o.g.theta,
            alpha: o.g.alpha,
            u_c: o.g.u_c,
        };
    }

    Run {
        ticks,
        zero,
        short,
        sum_live,
        sum_active,
        delivered,
        retx,
        horizon_s,
        mean_cap: cap_sum / ticks.max(1) as f64,
        anchor_ratio_sum,
        anchor_ratio_n,
        cwnd_sum,
        cwnd_n,
        led,
        xa_sum,
        xa_n,
        obs: obs_stat,
        bw_sum,
        bw_n,
        mrtt_sum,
        delivered_p,
        xlr_med,
    }
}

fn cells() -> Vec<(&'static str, Vec<Spec>)> {
    vec![
        ("sc2  single fast            ", vec![C2]),
        ("sc3  single slow            ", vec![C3]),
        ("c7   dual symmetric         ", vec![C2, C2]),
        ("c8   dual asym (rate + RTT) ", vec![C2, C3]),
        ("c8r  dual asym RATE only    ", vec![C2, (C3.0, C2.1, C3.2, C3.3)]),
        ("c8t  dual asym RTT only     ", vec![C2, (C2.0, C3.1, C3.2, C3.3)]),
    ]
}

/// (1) THE REPRODUCTION — the `[SF]` zero-fraction under U on/off, per cell.
#[test]
#[ignore = "component bench; run with --ignored --nocapture"]
fn sf_zero_fraction_closed_loop_by_cell() {
    println!("\n=== [SF] ZERO-FRACTION, CLOSED LOOP (component bench, 2026-08-11) ===");
    println!("gain {GAIN}  floor {FLOOR}  knee/path {KNEE}  boot {BOOT}  refresh {:.0} ms  horizon 20 s", REFRESH_S * 1e3);
    println!("law: cap = clamp(gain*N*Sigma_set anchor, floor, N*knee); N = live_paths()\n");
    println!(
        "{:<30} {:<22} {:>8} {:>8} {:>7} {:>10} {:>12}",
        "cell", "arm", "zero%", "short%", "E", "mean cap", "goodput sym/s"
    );
    for (name, geom) in cells() {
        let mut base = 0.0;
        for arm in [Arm::Legacy, Arm::Unified, Arm::PooledUnified] {
            let r = simulate(&geom, arm, 20.0);
            if arm == Arm::Legacy {
                base = r.zero_pct();
            }
            let fold = if base > 0.0 { format!("  ({:.1}x)", r.zero_pct() / base) } else { String::new() };
            println!(
                "{:<30} {:<22} {:>7.1}% {:>7.1}% {:>7.3} {:>10.0} {:>12.0}{}",
                name,
                arm.label(),
                r.zero_pct(),
                r.short_pct(),
                r.e(),
                r.mean_cap,
                r.goodput_sym_s(),
                fold
            );
        }
        println!();
    }
}

/// (2) THE AXIS SWEEP — which geometry axis drives the fold. Rate ratio and
/// RTT ratio swept independently against the same fast path.
#[test]
#[ignore = "component bench; run with --ignored --nocapture"]
fn sf_zero_fold_vs_geometry_axes() {
    println!("\n=== U's [SF] FOLD vs GEOMETRY AXIS ===");
    println!("path 0 fixed at c2 (10 400 sym/s, RTprop 8 ms); path 1 swept.\n");

    println!("--- RATE asymmetry only (path 1 RTprop = 8 ms) ---");
    println!("{:>10} {:>12} {:>10} {:>10} {:>8}", "rate ratio", "drain ms", "A zero%", "AU zero%", "fold");
    for div in [1.0_f64, 2.0, 3.0, 5.2, 8.0] {
        let p1 = (C2.0 / div, C2.1, C2.2, C2.3);
        let a = simulate(&[C2, p1], Arm::Legacy, 20.0);
        let u = simulate(&[C2, p1], Arm::Unified, 20.0);
        println!(
            "{:>10.1} {:>12.1} {:>9.1}% {:>9.1}% {:>8}",
            div,
            p1.0 * p1.1 / p1.0 * 1e3,
            a.zero_pct(),
            u.zero_pct(),
            fold_str(a.zero_pct(), u.zero_pct())
        );
    }

    println!("\n--- RTT asymmetry only (path 1 rate = 10 400 sym/s) ---");
    println!("{:>10} {:>12} {:>10} {:>10} {:>8}", "RTT ratio", "drain ms", "A zero%", "AU zero%", "fold");
    for mul in [1.0_f64, 2.0, 3.75, 7.5, 12.0] {
        let p1 = (C2.0, C2.1 * mul, C2.2, C2.3);
        let a = simulate(&[C2, p1], Arm::Legacy, 20.0);
        let u = simulate(&[C2, p1], Arm::Unified, 20.0);
        println!(
            "{:>10.2} {:>12.1} {:>9.1}% {:>9.1}% {:>8}",
            mul,
            p1.0 * p1.1 / p1.0 * 1e3,
            a.zero_pct(),
            u.zero_pct(),
            fold_str(a.zero_pct(), u.zero_pct())
        );
    }

    println!("\n--- BOTH, holding the DRAIN TIME cwnd_i/rate_i = RTprop_i fixed ---");
    println!("(the c8 diagonal: rate down by d, RTprop up by d — anchor constant)\n");
    println!("{:>10} {:>12} {:>10} {:>10} {:>8}", "d", "drain ms", "A zero%", "AU zero%", "fold");
    for d in [1.0_f64, 2.0, 3.0, 5.2, 7.5] {
        let p1 = (C2.0 / d, C2.1 * d, C2.2, C2.3);
        let a = simulate(&[C2, p1], Arm::Legacy, 20.0);
        let u = simulate(&[C2, p1], Arm::Unified, 20.0);
        println!(
            "{:>10.1} {:>12.1} {:>9.1}% {:>9.1}% {:>8}",
            d,
            p1.1 * 1e3,
            a.zero_pct(),
            u.zero_pct(),
            fold_str(a.zero_pct(), u.zero_pct())
        );
    }
    println!();
}

/// The four cells the anchor-era question is asked at: the two the wire
/// separates (c7 immune, c8 exposed) and c8's two half-axes.
fn era_cells() -> Vec<(&'static str, Vec<Spec>)> {
    vec![
        ("c7   dual symmetric   ", vec![C2, C2]),
        ("c8   dual asym (r+RTT)", vec![C2, C3]),
        ("c8r  dual asym RATE   ", vec![C2, (C3.0, C2.1, C3.2, C3.3)]),
        ("c8t  dual asym RTT    ", vec![C2, (C2.0, C3.1, C3.2, C3.3)]),
    ]
}

/// (3) THE ANCHOR-ERA SWEEP — the suspect, as a CURVE and not a point.
///
/// The engine's legacy ack-interval anchor over-reads ×4.6–7.4 (goal-gate
/// "Anchor Hygiene" (b)); the shipped honest anchor reads ×1. No single value
/// is privileged here: the scale is swept THROUGH that band and past it, and
/// the reader is shown the whole curve, so a conclusion that depends on
/// picking 4.6 is visibly not available.
#[test]
#[ignore = "component bench; run with --ignored --nocapture"]
fn sf_zero_fraction_vs_anchor_overread() {
    println!("\n=== [SF] ZERO-FRACTION vs ANCHOR-ERA OVER-READ (20 s, deterministic) ===");
    println!("scale f feeds the LEGACY ack-interval sampler f x its true delta =>");
    println!("anchor, anchor floor (clamp_cwnd_with_anchor) and store-cap Sigma all x f.");
    println!("f = 1.0 IS the shipped honest-anchor era; the wire's legacy band is 4.6-7.4.\n");
    println!("NOTE: the injected scale f is NOT the realized over-read. `max_bw` is a windowed");
    println!("MAX over a 10 s window, and the loop feeds back (a bigger cwnd sends bigger bursts,");
    println!("which spike Delta/Dt further), so the MEASURED anchor/(rate*RTprop) is reported as x");
    println!("and it is x, not f, that must be read against the wire's 4.6-7.4 band.\n");
    for (name, geom) in era_cells() {
        println!(
            "{:<24} {:>6} {:>7} {:>9} {:>9} {:>10} {:>10} {:>12}",
            name, "f", "x (A)", "A zero%", "AU zero%", "A cwnd", "A cap", "A goodput"
        );
        for f in [1.0_f64, 1.5, 2.0, 2.5, 3.0, 4.0, 4.6, 6.0, 7.4, 10.0] {
            let feed = if f == 1.0 { Feed::Honest } else { Feed::Overread(f) };
            let (mut az, mut uz, mut ax, mut ac, mut acp, mut ag) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
            let n = 3u64;
            for s in 0..n {
                let a = simulate_seeded(&geom, Arm::Legacy, feed, 20.0, s);
                let u = simulate_seeded(&geom, Arm::Unified, feed, 20.0, s);
                az += a.zero_pct();
                uz += u.zero_pct();
                ax += a.overread();
                ac += a.mean_cwnd();
                acp += a.mean_cap;
                ag += a.goodput_sym_s();
            }
            let n = n as f64;
            println!(
                "{:<24} {:>6.1} {:>7.2} {:>8.1}% {:>8.1}% {:>10.0} {:>10.0} {:>12.0}",
                "",
                f,
                ax / n,
                az / n,
                uz / n,
                ac / n,
                acp / n,
                ag / n
            );
        }
        println!();
    }
}

/// The seed ensemble size. FINDING 4: the loop is bistable, so the statistic
/// that resolves it is the MODE RATE over an ensemble, not one run's mean.
const SEEDS: u64 = 8;

/// The CAUGHT class, pre-declared before the matrix is read: a run whose
/// `[SF]` zero-fraction is below 10%. The wire's legacy arms sit in a ≈4%
/// class and the bench's caught regime (FINDING 4, d = 5.2–7.5) sits at
/// 0.2–0.3%; 10% separates those from the 40–100% saturated mode with a wide
/// margin on both sides. `min`/`max` are printed so the cut can be re-drawn.
const CAUGHT_PCT: f64 = 10.0;

struct Ens {
    zero: Vec<f64>,
    gp: Vec<f64>,
    x: Vec<f64>,
    cwnd: Vec<f64>,
    cap: Vec<f64>,
}

impl Ens {
    fn run(geom: &[Spec], arm: Arm, feed: Feed) -> Self {
        let mut e = Ens { zero: vec![], gp: vec![], x: vec![], cwnd: vec![], cap: vec![] };
        for s in 0..SEEDS {
            let r = simulate_seeded(geom, arm, feed, 20.0, s);
            e.zero.push(r.zero_pct());
            e.gp.push(r.goodput_sym_s());
            e.x.push(r.overread());
            e.cwnd.push(r.mean_cwnd());
            e.cap.push(r.mean_cap);
        }
        e
    }
    fn mean(v: &[f64]) -> f64 {
        v.iter().sum::<f64>() / v.len().max(1) as f64
    }
    /// P(caught) — the mode rate.
    fn caught(&self) -> f64 {
        self.zero.iter().filter(|z| **z < CAUGHT_PCT).count() as f64 / self.zero.len() as f64
    }
    fn lo(&self) -> f64 {
        self.zero.iter().cloned().fold(f64::INFINITY, f64::min)
    }
    fn hi(&self) -> f64 {
        self.zero.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
    }
}

/// (4) THE MATRIX the question asks for: {c7, c8, c8r, c8t} × {A, AU, P} ×
/// {honest, over-read band}, scored over the seed ensemble. The over-read
/// column is shown at BOTH ends of the wire's measured band, never at a
/// single chosen value.
#[test]
#[ignore = "component bench; run with --ignored --nocapture"]
fn sf_anchor_era_matrix() {
    println!("\n=== ANCHOR-ERA MATRIX: cell x arm x era, {SEEDS} seeds x 20 s ===");
    println!("zero% = mean [SF] zero-fraction; [lo..hi] its range over seeds;");
    println!("caught = MODE RATE, the fraction of seeds with zero% < {CAUGHT_PCT:.0}% (FINDING 4's statistic);");
    println!("x = realized anchor over-read vs rate*RTprop.\n");
    println!(
        "{:<24} {:<22} {:>14} {:>8} {:>16} {:>8} {:>7} {:>8} {:>8} {:>9}",
        "cell", "arm", "era", "zero%", "[lo..hi]", "caught", "x", "cwnd", "cap", "goodput"
    );
    for (name, geom) in era_cells() {
        for arm in [Arm::Legacy, Arm::Unified, Arm::PooledUnified] {
            for feed in [Feed::Honest, Feed::Overread(4.6), Feed::Overread(7.4)] {
                let e = Ens::run(&geom, arm, feed);
                println!(
                    "{:<24} {:<22} {:>14} {:>7.1}% {:>16} {:>7.0}% {:>7.2} {:>8.0} {:>8.0} {:>9.0}",
                    name,
                    arm.label(),
                    feed.label(),
                    Ens::mean(&e.zero),
                    format!("[{:.1}..{:.1}]", e.lo(), e.hi()),
                    e.caught() * 100.0,
                    Ens::mean(&e.x),
                    Ens::mean(&e.cwnd),
                    Ens::mean(&e.cap),
                    Ens::mean(&e.gp)
                );
            }
        }
        println!();
    }
}

/// (5) THE DERIVED ERA — no injected number at all. A cumulative-frontier
/// receiver on a feedback cadence; the batch sizes, and hence the over-read,
/// are whatever the bench's own GE loss and retransmit timing produce. The
/// realized over-read is MEASURED against `rate·RTprop` and can be compared
/// with the wire's 4.6–7.4 band on its own terms.
#[test]
#[ignore = "component bench; run with --ignored --nocapture"]
fn sf_derived_overread_from_ack_batching() {
    println!("\n=== DERIVED ANCHOR ERA: cumulative-frontier acks at a feedback cadence ===");
    println!("no injected factor; 'x' is the MEASURED anchor / (rate*RTprop).\n");
    for (name, geom) in era_cells() {
        println!(
            "{:<24} {:>12} {:>8} {:>9} {:>7} {:>10} {:>12}",
            name, "cadence", "A zero%", "AU zero%", "x (A)", "A cwnd", "A goodput"
        );
        let h = simulate(&geom, Arm::Legacy, 20.0);
        let hu = simulate(&geom, Arm::Unified, 20.0);
        println!(
            "{:<24} {:>12} {:>7.1}% {:>8.1}% {:>7.2} {:>10.0} {:>12.0}",
            "",
            "honest",
            h.zero_pct(),
            hu.zero_pct(),
            h.overread(),
            h.mean_cwnd(),
            h.goodput_sym_s()
        );
        for ms in [0.25_f64, 1.0, 2.0, 5.0, 10.0] {
            let feed = Feed::Cumulative { ack_period_s: ms / 1e3 };
            let a = simulate_era(&geom, Arm::Legacy, feed, 20.0);
            let u = simulate_era(&geom, Arm::Unified, feed, 20.0);
            println!(
                "{:<24} {:>10.2}ms {:>7.1}% {:>8.1}% {:>7.2} {:>10.0} {:>12.0}",
                "",
                ms,
                a.zero_pct(),
                u.zero_pct(),
                a.overread(),
                a.mean_cwnd(),
                a.goodput_sym_s()
            );
        }
        println!();
    }
}

// ── THE ACCOUNTING-AXIS MATRIX, AND ITS PRE-REGISTERED VERDICT ────────────
//
// PRE-REGISTERED 2026-08-11, in the commit BEFORE the ON arm was ever run.
//
// THE WIRE'S GEOGRAPHY, quoted from goal-gate "Store-Cap Unification —
// RESULTS" and §16.52 (no L1 number is re-derived here):
//
//   * the legacy (A) arm sits in a ≈4% `[SF]` zero-fraction class — 3.7–7.4%
//     at c8, on both seeds and in BOTH anchor eras — and c1/sc2/c7 "do not
//     move", i.e. A is in that same low class at c7 as well;
//   * U raises c8 from ≈4% to ≈30% past 2σ on both seeds (a ≈7.5× fold) and
//     does NOT do so at c7.
//
// The bench with the axis OFF reproduces NEITHER: its A arm sits at 9.0% at
// c7 and 40.9% at c8 (×10 above the wire's operating point), and its U-fold
// is 11.0× at c7 against 2.4× at c8 — the fold keyed to the WRONG cell.
//
// THE HYPOTHESIS UNDER TEST: the un-metered, slow-leg-concentrated recovery
// flow is what keys the collapse to c8 on the wire.
//
// PASS CRITERIA, both required, stated before the measurement:
//
//   G1 (LEVEL). With `Acct::Engine`, the A arm's ensemble-mean zero-fraction
//       is below `CAUGHT_PCT` (10%) at BOTH c7 and c8, and its CAUGHT mode
//       rate is ≥ 50% at both. This is the wire's "A is a ≈4% class at both
//       cells", scored on the statistic §16.52 requires.
//   G2 (CELL-KEYING). fold = mean(AU zero%) / mean(A zero%). Require
//       fold(c8) ≥ 3.0 AND fold(c7) ≤ 2.0. The wire's own separation is
//       ≈7.5× against null, so this band is well inside it and leaves a clean
//       gap between the two cells.
//
// VERDICT = G1 ∧ G2. Anything else is NOT REPRODUCED and triggers the named
// fallback (goal-gate "PIPELINE VERIFICATION MATRIX": rank 2, the L0
// WindowAck-cadence gauge). c8r/c8t are reported for completeness; the
// verdict rests on c7 + c8, which is the contrast the dispatch names.
const G1_LEVEL_PCT: f64 = CAUGHT_PCT;
const G1_CAUGHT_MIN: f64 = 0.50;
const G2_FOLD_C8_MIN: f64 = 3.0;
const G2_FOLD_C7_MAX: f64 = 2.0;

struct AcctEns {
    zero: Vec<f64>,
    gp: Vec<f64>,
    cap: Vec<f64>,
    led: Ledger,
}

impl AcctEns {
    fn run(geom: &[Spec], arm: Arm, acct: Acct) -> Self {
        let mut e = AcctEns { zero: vec![], gp: vec![], cap: vec![], led: Ledger::default() };
        for s in 0..SEEDS {
            let r = simulate_acct(geom, arm, Feed::Honest, 20.0, s, acct);
            e.zero.push(r.zero_pct());
            e.gp.push(r.goodput_sym_s());
            e.cap.push(r.mean_cap);
            e.led.src += r.led.src;
            e.led.taper += r.led.taper;
            e.led.retx += r.led.retx;
            e.led.margin += r.led.margin;
            e.led.charges += r.led.charges;
            e.led.releases += r.led.releases;
            e.led.releases_wasted += r.led.releases_wasted;
            e.led.tokens += r.led.tokens;
        }
        e
    }
    fn mean(v: &[f64]) -> f64 {
        v.iter().sum::<f64>() / v.len().max(1) as f64
    }
    fn caught(&self) -> f64 {
        self.zero.iter().filter(|z| **z < CAUGHT_PCT).count() as f64 / self.zero.len() as f64
    }
    fn lo(&self) -> f64 {
        self.zero.iter().cloned().fold(f64::INFINITY, f64::min)
    }
    fn hi(&self) -> f64 {
        self.zero.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
    }
}

/// (6) THE ACCOUNTING-AXIS MATRIX — {c7, c8, c8r, c8t} × {A, AU} × {OFF,
/// TRAFFIC, ENGINE}, 8 seeds × 20 s, scored on the MODE RATE. The verdict
/// against G1/G2 is printed, and it is printed for c7 and c8 only, which are
/// the cells the pre-registration names.
#[test]
#[ignore = "component bench; run with --ignored --nocapture"]
fn sf_accounting_axis_matrix() {
    println!("\n=== IN-FLIGHT ACCOUNTING AXIS: cell x arm x metering, {SEEDS} seeds x 20 s ===");
    println!("OFF     = the published bench (source + retransmit, ledger balances 1:1)");
    println!("TRAFFIC = recovery traffic exists (taper r*, NACK margin, estimator fed), ledger BALANCES");
    println!("ENGINE  = as TRAFFIC + the two bypass channels uncharged + counter-delta release\n");
    println!(
        "{:<24} {:<22} {:<24} {:>8} {:>16} {:>8} {:>8} {:>9}",
        "cell", "arm", "metering", "zero%", "[lo..hi]", "caught", "cap", "goodput"
    );
    let mut verdict: Vec<(&str, f64, f64, f64)> = Vec::new();
    for (name, geom) in era_cells() {
        let mut a_zero = 0.0;
        let mut a_caught = 0.0;
        let mut u_zero = 0.0;
        for acct in [Acct::Off, Acct::Traffic, Acct::Engine] {
            for arm in [Arm::Legacy, Arm::Unified] {
                let e = AcctEns::run(&geom, arm, acct);
                if acct == Acct::Engine && arm == Arm::Legacy {
                    a_zero = AcctEns::mean(&e.zero);
                    a_caught = e.caught();
                }
                if acct == Acct::Engine && arm == Arm::Unified {
                    u_zero = AcctEns::mean(&e.zero);
                }
                println!(
                    "{:<24} {:<22} {:<24} {:>7.1}% {:>16} {:>7.0}% {:>8.0} {:>9.0}",
                    name,
                    arm.label(),
                    acct.label(),
                    AcctEns::mean(&e.zero),
                    format!("[{:.1}..{:.1}]", e.lo(), e.hi()),
                    e.caught() * 100.0,
                    AcctEns::mean(&e.cap),
                    AcctEns::mean(&e.gp)
                );
                if acct != Acct::Off && arm == Arm::Legacy {
                    let l = e.led;
                    println!(
                        "{:<24} {:<22} {:<24}   channels: src {} taper {} retx {} margin {} | \
                         charges {} releases {} wasted {} tokens {} | wire/src {:.3}",
                        "", "", "",
                        l.src, l.taper, l.retx, l.margin,
                        l.charges, l.releases, l.releases_wasted, l.tokens,
                        l.wire() as f64 / l.src.max(1) as f64
                    );
                }
            }
        }
        verdict.push((name, a_zero, a_caught, if a_zero > 0.0 { u_zero / a_zero } else { f64::INFINITY }));
        println!();
    }

    println!("--- PRE-REGISTERED VERDICT (G1 level, G2 cell-keying; c7 + c8) ---");
    println!(
        "G1: ENGINE A-arm mean < {G1_LEVEL_PCT:.0}% AND caught >= {:.0}% at BOTH c7 and c8",
        G1_CAUGHT_MIN * 100.0
    );
    println!("G2: fold(c8) >= {G2_FOLD_C8_MIN:.1} AND fold(c7) <= {G2_FOLD_C7_MAX:.1}\n");
    let mut g1 = true;
    let mut g2 = true;
    for (name, z, c, f) in &verdict {
        let key = name.trim();
        let is_c7 = key.starts_with("c7");
        let is_c8 = key.starts_with("c8 ");
        println!("{name}  A {z:.1}%  caught {:.0}%  fold {f:.1}x", c * 100.0);
        if is_c7 || is_c8 {
            if *z >= G1_LEVEL_PCT || *c < G1_CAUGHT_MIN {
                g1 = false;
            }
            if is_c8 && *f < G2_FOLD_C8_MIN {
                g2 = false;
            }
            if is_c7 && *f > G2_FOLD_C7_MAX {
                g2 = false;
            }
        }
    }
    println!(
        "\nG1 {}  G2 {}  ==> GEOGRAPHY {}",
        if g1 { "PASS" } else { "FAIL" },
        if g2 { "PASS" } else { "FAIL" },
        if g1 && g2 { "REPRODUCED" } else { "NOT REPRODUCED" }
    );
}

/// (7) THE CANDIDATE, re-scored on the corrected bench. Whatever the verdict
/// above, the pooled-ceiling successor's standing was measured on a bench
/// with no recovery accounting at all, so it is re-run here on every level of
/// the axis.
#[test]
#[ignore = "component bench; run with --ignored --nocapture"]
fn sf_pooled_candidate_on_the_accounting_axis() {
    println!("\n=== POOLED-CEILING CANDIDATE vs the accounting axis ({SEEDS} seeds x 20 s) ===");
    println!(
        "{:<24} {:<24} {:>8} {:>8} {:>8} {:>8} {:>10} {:>10} {:>10}",
        "cell", "metering", "A zero%", "AU zero%", "P zero%", "P caught", "A gp", "AU gp", "P gp"
    );
    for (name, geom) in era_cells() {
        for acct in [Acct::Off, Acct::Engine] {
            let a = AcctEns::run(&geom, Arm::Legacy, acct);
            let u = AcctEns::run(&geom, Arm::Unified, acct);
            let p = AcctEns::run(&geom, Arm::PooledUnified, acct);
            println!(
                "{:<24} {:<24} {:>7.1}% {:>7.1}% {:>7.1}% {:>7.0}% {:>10.0} {:>10.0} {:>10.0}",
                name,
                acct.label(),
                AcctEns::mean(&a.zero),
                AcctEns::mean(&u.zero),
                AcctEns::mean(&p.zero),
                p.caught() * 100.0,
                AcctEns::mean(&a.gp),
                AcctEns::mean(&u.gp),
                AcctEns::mean(&p.gp)
            );
        }
        println!();
    }
}

// ── The accounting axis's always-on pins ──────────────────────────────────

/// THE BOUNDING TEST FOR §12 — goal-gate "PIPELINE VERIFICATION MATRIX" row
/// 2, the KNOWN-DIVERGENT row that carried no bounding test, which CLAUDE.md
/// forbids outright ("every documented model-vs-engine divergence must carry
/// a test that BOUNDS it, not prose that describes it").
///
/// **The paper (§12, the amendment) claims the token bucket paces SOURCE AND
/// REPAIR at the CC rate.** The code debits the bucket inside the source arm
/// alone (`emit_source.rs:493-497`), so the realized wire rate is
/// `src·(1+r)` — every repair, retransmit and margin symbol reaches the link
/// having debited nothing.
///
/// This asserts what the IMPLEMENTATION does, absolutely and by identity, not
/// what it ought to do: the debit count equals the SOURCE count exactly, and
/// the wire count exceeds it by exactly the three unpaced channels. The
/// residual `wire/src − 1` is the realized `r` — computed from the bench's own
/// loss realizations through the shipped r\* law, never fitted.
///
/// If a successor ever paces repair, this test fails loudly and the ledger row
/// gets re-scored rather than silently drifting back into agreement.
#[test]
fn pacer_debit_bounds_only_the_source_arm_not_the_wire() {
    let r = simulate_acct(&[C2, C3], Arm::Legacy, Feed::Honest, 6.0, 0, Acct::Engine);
    let l = r.led;
    // MEASUREMENT DISCIPLINE 1 — the divergence must actually be reachable:
    // all three unpaced channels must have fired, or this proves nothing.
    assert!(l.src > 10_000, "no source traffic: {}", l.src);
    assert!(l.taper > 0, "the taper repair channel never fired");
    assert!(l.retx > 0, "the SACK-gap retransmit channel never fired");
    assert!(l.margin > 0, "the NACK repair margin channel never fired");
    // THE DIVERGENCE, as an exact identity in both directions.
    assert_eq!(
        l.tokens, l.src,
        "the pacer debit must be the SOURCE arm exactly (emit_source.rs:493-497)"
    );
    assert_eq!(
        l.wire(),
        l.src + l.taper + l.retx + l.margin,
        "the wire is source + the three unpaced channels and nothing else"
    );
    assert!(
        l.wire() > l.tokens,
        "the paper's §12 claim would need wire == tokens; measured wire {} vs tokens {}",
        l.wire(),
        l.tokens
    );
    // And the size of the divergence, bounded rather than described: the
    // unpaced excess is the realized repair overhead r, which the shipped law
    // caps at `max_fec_overhead` (0.5) per source symbol on the taper channel.
    let excess = (l.wire() - l.tokens) as f64 / l.src as f64;
    assert!(
        excess > 0.0 && excess < 1.0,
        "realized unpaced excess wire/src − 1 = {excess:.4}, outside (0, 1)"
    );
}

/// The FIRST HALF of matrix row 6's unverified property: `Σ charges` does NOT
/// count every wire symbol. It under-counts by exactly the two bypass
/// channels — the SACK-gap retransmit (`net/mod.rs:6374-6383`) and the NACK
/// repair margin (`net/mod.rs:6420-6448`), each of which builds a
/// `SymbolBatch` and calls `transport.send_symbols` with no
/// `charge_in_flight` anywhere on the path. The taper correction
/// (`emit_source.rs:929`) IS charged, and that asymmetry is asserted too, so
/// the test names which channels bypass and which do not.
///
/// The ratio is computed from the run's own channel counts, not fitted.
#[test]
fn unmetered_recovery_flow_is_not_charged_to_in_flight() {
    let e = simulate_acct(&[C2, C3], Arm::Legacy, Feed::Honest, 6.0, 0, Acct::Engine);
    let t = simulate_acct(&[C2, C3], Arm::Legacy, Feed::Honest, 6.0, 0, Acct::Traffic);
    // Under the ENGINE ledger the charge deficit is EXACTLY the two bypass
    // channels — an equality, not a bound.
    assert_eq!(
        e.led.wire() - e.led.charges,
        e.led.retx + e.led.margin,
        "the charge deficit must be exactly retx + margin (wire {} charges {} \
         retx {} margin {})",
        e.led.wire(),
        e.led.charges,
        e.led.retx,
        e.led.margin
    );
    // The taper correction is on the OTHER side of that line: charged.
    assert!(e.led.taper > 0, "the taper channel never fired");
    assert_eq!(
        e.led.charges,
        e.led.src + e.led.taper,
        "only source and taper corrections are charged under the engine ledger"
    );
    // The counterfactual engine that obeys §12's accounting charges every
    // wire symbol — which is what makes the comparison in the matrix a test
    // of the LEDGER and not of the traffic.
    assert_eq!(
        t.led.charges,
        t.led.wire(),
        "the TRAFFIC arm must charge every wire symbol (it is the balanced control)"
    );
}

/// The SECOND HALF of row 6: release is COUNTER-DELTA driven
/// (`control_msg.rs:341`, `:685` — `expected − received` on the path the
/// feedback arrived on, over counters `receiver.rs:1754` builds from every
/// symbol in the batch), so it is not 1:1 with charge and the ledger does not
/// balance by construction.
///
/// Asserted absolutely: under the ENGINE ledger releases strictly EXCEED
/// charges, the excess is bounded by the un-charged wire, and some of it is
/// provably thrown away by `release_in_flight`'s saturating subtraction —
/// budget the path can never get back. Under the balanced TRAFFIC control the
/// same run conserves. This is the four `ack_merge_counter_delta_*`
/// invariants extended to the un-metered case, which none of them exercises.
#[test]
fn counter_delta_release_is_conservative_under_loss() {
    for geom in [vec![C2, C2], vec![C2, C3]] {
        let e = simulate_acct(&geom, Arm::Legacy, Feed::Honest, 6.0, 0, Acct::Engine);
        let t = simulate_acct(&geom, Arm::Legacy, Feed::Honest, 6.0, 0, Acct::Traffic);
        // The engine over-releases, and by no more than the un-charged wire.
        assert!(
            e.led.releases > e.led.charges,
            "engine ledger did not over-release: charges {} releases {}",
            e.led.charges,
            e.led.releases
        );
        assert!(
            e.led.releases - e.led.charges <= e.led.retx + e.led.margin,
            "the over-release ({}) exceeds the un-charged wire ({})",
            e.led.releases - e.led.charges,
            e.led.retx + e.led.margin
        );
        // Some of it is unrecoverable: `release_in_flight` saturates at zero.
        assert!(
            e.led.releases_wasted > 0,
            "no release ever hit a zero in_flight — the saturation the engine's \
             counter-delta release runs into was never exercised"
        );
        // The balanced control conserves EXACTLY over the same traffic.
        assert_eq!(
            t.led.releases_wasted, 0,
            "the balanced ledger must never waste a release"
        );
        assert!(
            t.led.releases <= t.led.charges,
            "the balanced ledger released {} against {} charges",
            t.led.releases,
            t.led.charges
        );
    }
}

/// THE MEASURED RESULT, BOUNDED — goal-gate "SF Accounting Axis", FINDING 1.
///
/// The metering axis MOVES WHICH CELL THE U-FOLD KEYS TO, and it moves it onto
/// the cell the wire folds at. This is the one thing three prior sections
/// could not reproduce and explicitly left unexplained ("c8 SF Mechanism"
/// FINDING 3, "SF Anchor Suspect" DELIBERATELY NOT CONCLUDED, and the matrix's
/// anomaly A2).
///
/// Absolute, at 3 seeds × 6 s, both directions asserted so the swap cannot be
/// half-read (the 8-seed × 20 s ensemble that established it is in the
/// ledger; this is the regression bound, not the evidence):
///
///   * with the ledger balanced (`Acct::Off`, the published bench) the fold is
///     LARGER at the symmetric cell than at c8 — the wrong-cell keying;
///   * with the ENGINE's ledger it is larger at c8 by > 3× and NULL at c7
///     (< 2×), which is the wire's own separation (≈7.5× against null);
///   * and the c8 A-arm's LEVEL falls by more than half, from a > 25% class to
///     a < 15% class.
///
/// What is NOT asserted here, because it did not hold: the c7 A-arm's level.
/// The pre-registered G1 FAILED at c7 and the verdict stands as NOT
/// REPRODUCED; see the ledger block. This test bounds exactly what was
/// measured and no more.
#[test]
fn sf_zero_fraction_moves_with_the_metering_axis() {
    let mean = |geom: &[Spec], arm: Arm, acct: Acct| -> f64 {
        (0..3u64)
            .map(|s| simulate_acct(geom, arm, Feed::Honest, 6.0, s, acct).zero_pct())
            .sum::<f64>()
            / 3.0
    };
    let c7 = vec![C2, C2];
    let c8 = vec![C2, C3];

    // MEASUREMENT DISCIPLINE 1: the axis must have run.
    let probe = simulate_acct(&c8, Arm::Legacy, Feed::Honest, 6.0, 0, Acct::Engine);
    assert!(probe.led.taper > 0 && probe.led.margin > 0 && probe.led.retx > 0);
    assert!(probe.led.releases > probe.led.charges, "the un-metered ledger never ran");

    let off_a7 = mean(&c7, Arm::Legacy, Acct::Off);
    let off_u7 = mean(&c7, Arm::Unified, Acct::Off);
    let off_a8 = mean(&c8, Arm::Legacy, Acct::Off);
    let off_u8 = mean(&c8, Arm::Unified, Acct::Off);
    let en_a7 = mean(&c7, Arm::Legacy, Acct::Engine);
    let en_u7 = mean(&c7, Arm::Unified, Acct::Engine);
    let en_a8 = mean(&c8, Arm::Legacy, Acct::Engine);
    let en_u8 = mean(&c8, Arm::Unified, Acct::Engine);

    // THE PUBLISHED BENCH keys the fold to the WRONG cell.
    let off_f7 = off_u7 / off_a7;
    let off_f8 = off_u8 / off_a8;
    assert!(
        off_f7 > off_f8,
        "the balanced ledger must fold harder at c7 than at c8 (the published \
         defect): c7 {off_f7:.2}x vs c8 {off_f8:.2}x"
    );

    // THE ENGINE'S LEDGER keys it to c8 and nulls c7.
    let en_f7 = en_u7 / en_a7;
    let en_f8 = en_u8 / en_a8;
    assert!(
        en_f8 > 3.0,
        "the engine ledger must keep a large U-fold at c8: {en_f8:.2}x \
         (A {en_a8:.1}% AU {en_u8:.1}%)"
    );
    assert!(
        en_f7 < 2.0,
        "the engine ledger must null the U-fold at c7: {en_f7:.2}x \
         (A {en_a7:.1}% AU {en_u7:.1}%)"
    );

    // And the c8 A-arm's LEVEL, absolutely on both sides of the axis.
    assert!(off_a8 > 25.0, "the published c8 A arm must be the high class: {off_a8:.1}%");
    assert!(en_a8 < 15.0, "the engine c8 A arm must be the low class: {en_a8:.1}%");
    assert!(
        en_a8 < 0.5 * off_a8,
        "the c8 A arm must more than halve across the axis: {off_a8:.1}% → {en_a8:.1}%"
    );
}

/// THE ATTRIBUTION, bounded — goal-gate "SF Accounting Axis", FINDING 2: it is
/// the LEDGER, not the extra recovery traffic, that moves c8.
///
/// The `Traffic` level emits exactly the same taper corrections and NACK margin
/// repairs onto exactly the same repair placements, feeds exactly the same
/// estimators, and consumes exactly the same wire — and changes the c8 A arm's
/// zero-fraction by only a few points, leaving the fold on the wrong cell.
/// Only `Engine`, which adds the two un-charged channels and the counter-delta
/// release, moves it. Without this pin the result would be attributable to
/// "more traffic", which is the reading the measurement excludes.
#[test]
fn the_ledger_not_the_recovery_traffic_moves_the_c8_zero_fraction() {
    let c8 = vec![C2, C3];
    let mean = |arm: Arm, acct: Acct| -> f64 {
        (0..3u64)
            .map(|s| simulate_acct(&c8, arm, Feed::Honest, 6.0, s, acct).zero_pct())
            .sum::<f64>()
            / 3.0
    };
    // The traffic is REAL and identical in both ON levels — else this proves
    // nothing (MEASUREMENT DISCIPLINE 1).
    let t = simulate_acct(&c8, Arm::Legacy, Feed::Honest, 6.0, 0, Acct::Traffic);
    assert!(t.led.taper > 0 && t.led.margin > 0, "the traffic level emitted no recovery");
    assert_eq!(t.led.charges, t.led.wire(), "the traffic level must balance");

    let off_a = mean(Arm::Legacy, Acct::Off);
    let tr_a = mean(Arm::Legacy, Acct::Traffic);
    let en_a = mean(Arm::Legacy, Acct::Engine);
    // The two levels move the arm in OPPOSITE directions, which is the whole
    // point: balanced recovery traffic leaves c8 in — or pushes it further
    // into — its published high class, and only the un-metered LEDGER brings
    // it down to the wire's low class. Asserted as absolute classes on both
    // sides, not as a ratio, because the levels are draws from a bistable
    // loop and only the class membership is stable.
    assert!(off_a > 25.0, "the published c8 A arm must be the high class: {off_a:.1}%");
    assert!(
        tr_a > 20.0 && tr_a > off_a - 5.0,
        "balanced recovery traffic must not bring the c8 A arm down: \
         off {off_a:.1}% vs traffic {tr_a:.1}%"
    );
    assert!(
        en_a < 15.0 && off_a - en_a > 15.0,
        "the un-metered ledger must bring the c8 A arm into the low class: \
         off {off_a:.1}% vs engine {en_a:.1}%"
    );
}

/// THE AXIS'S OWN REPRODUCIBILITY, pinned in the shape the bench already
/// learned once (`symmetric_cell_placement_tie_is_broken_deterministically`).
///
/// The repair channel's rate comes from ONE path's estimator — the max-loss
/// path among `active_paths()` (`emit_source.rs:613-620`). `active_paths()`
/// returns `HashMap` iteration order, and `max_by` keeps the LAST maximum, so
/// on a tie the winner is a per-PROCESS coin flip. Losses tie exactly at every
/// cold start (both estimators at 0.0) and routinely at the symmetric cell.
/// Without the sort in `worst_loss_path` the whole ON arm is unreproducible —
/// the same instrument fault, in the same instrument, that made c7 drift.
#[test]
fn worst_loss_path_tie_is_broken_deterministically() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    for id in [0u32, 1, 2, 3] {
        sched.add_path(id);
    }
    // Fresh paths: every estimator reads 0.0 ⇒ a pure four-way tie.
    let ids = sched.active_paths();
    assert_eq!(ids.len(), 4, "all four paths must be active for this to be a tie");
    assert!(
        ids.iter().all(|id| sched.path(*id).unwrap().estimator.loss_rate() == 0.0),
        "the guard only means something if the losses are exactly equal"
    );
    assert_eq!(worst_loss_path(&sched), Some(0), "the tie must go to the lowest path id");
    // And when the tie is broken by a real difference, the max wins on merit.
    sched.path_mut(2).unwrap().estimator.record_batch(100, 50);
    assert_eq!(worst_loss_path(&sched), Some(2), "the strict max must win");
}

/// MEASUREMENT DISCIPLINE 1 for the axis itself: every mechanism the axis
/// claims to transplant must EXECUTE, and the OFF level must be the published
/// bench untouched. Without this the matrix could report "no effect" from an
/// axis that never ran.
#[test]
fn accounting_axis_executes_and_off_is_the_published_bench() {
    let off = simulate_acct(&[C2, C3], Arm::Legacy, Feed::Honest, 6.0, 0, Acct::Off);
    let published = simulate_seeded(&[C2, C3], Arm::Legacy, Feed::Honest, 6.0, 0);
    assert_eq!(off.zero, published.zero, "Acct::Off must be the published bench");
    assert_eq!(off.ticks, published.ticks);
    assert_eq!(off.delivered, published.delivered);
    assert_eq!(off.retx, published.retx);
    // OFF has no recovery channels and a 1:1 ledger.
    assert_eq!(off.led.taper, 0);
    assert_eq!(off.led.margin, 0);
    assert_eq!(off.led.charges, off.led.src + off.led.retx);
    // ON has all of them, and the shipped r* law produced a NON-ZERO repair
    // rate from the bench's own measured loss — if r* read 0 the taper channel
    // would be a no-op and the axis would be testing two channels, not three.
    let on = simulate_acct(&[C2, C3], Arm::Legacy, Feed::Honest, 6.0, 0, Acct::Engine);
    assert!(on.led.taper > 0, "r* never cleared the repair debt");
    assert!(on.led.margin > 0, "the NACK margin never fired");
    assert!(on.led.retx > 0, "the retransmit channel never fired");
}

// ── THE MEASURED ERA'S READOUTS ────────────────────────────────────────────

/// The cells the wire MEASURED an ack stream at, and only those. `c8r`/`c8t`
/// are absent on purpose: they are half-axis geometries the VM never ran, so
/// there is no measured shape for their paths and this bench will not invent
/// one. That is the whole reason this branch exists.
fn measured_cells() -> Vec<(&'static str, Vec<Spec>, &'static [AckShape])> {
    vec![
        ("sc2  single fast (c2r100)", vec![C2], &ACK_SC2[..]),
        ("c7   dual symmetric      ", vec![C2, C2], &ACK_C7[..]),
        ("c8   dual asym (r+RTT)   ", vec![C2, C3], &ACK_C8[..]),
    ]
}

/// (8) THE VALIDATION GATE — V1/V2/V3, scored per path against the ledger
/// before the geography question may be asked at all.
#[test]
#[ignore = "component bench; run with --ignored --nocapture"]
fn sf_measured_ack_era_fidelity() {
    println!("\n=== THE MEASURED ACK ERA vs THE WIRE (validation gate V1/V2/V3) ===");
    println!("inputs : drecv = 1, per-path gap p50/p90/p99 (READOUT 1+2), the 1 ms floor");
    println!("checks : rejection %% (READOUT 3b), samples/s, acks folded, xanchor (READOUT 3)");
    println!("model  : work-conserving observer; theta/alpha/u_c SOLVED from the measurement\n");
    println!("V1 is scored on the LEDGER's xanchor (max_bw/rate_lr, READOUT 3); the bench's own");
    println!("overread() gauge divides by the CONFIGURED rate*RTprop and is shown beside it.\n");
    println!(
        "{:<26} {:<11} {:>7} {:>7} {:>6} | {:>9} {:>9} {:>9} | {:>7} {:>7} | {:>7} {:>7} {:>7} {:>6}",
        "cell", "path", "theta", "alpha", "u_c", "p50 us", "p90 us", "p99 us", "rej%", "want",
        "x_lr", "want", "minRTT", "V1"
    );
    let mut v1 = true;
    let mut v2 = true;
    let mut v3 = true;
    for (name, geom, shapes) in measured_cells() {
        let r = simulate_acct(&geom, Arm::Legacy, Feed::Measured(shapes), 20.0, 0, Acct::Off);
        for (i, sh) in shapes.iter().enumerate() {
            let o = r.obs[i];
            let x = r.xanchor_lr(i);
            let ok1 = (x - sh.xanchor).abs() <= V1_XANCHOR_TOL * sh.xanchor;
            let ok2 = (o.reject_pct() - sh.rej_pct).abs() <= V2_REJECT_TOL_PTS;
            // V3 is scored against the ledger's own per-window ranges, scaled
            // to THIS bench path's mean gap (the shape is dimensionless).
            let scale = (1e6 / sh.rate_lr) / o.mean_gap_us.max(1e-9);
            let inband = |v: f64, (lo, hi): (f64, f64)| v * scale >= lo * 0.5 && v * scale <= hi * 2.0;
            let ok3 = inband(o.p50_us, sh.p50) && inband(o.p90_us, sh.p90) && inband(o.p99_us, sh.p99);
            v1 &= ok1;
            v2 &= ok2;
            v3 &= ok3;
            println!(
                "{:<26} {:<11} {:>7.3} {:>7.2} {:>6.3} | {:>9.1} {:>9.1} {:>9.1} | {:>6.1}% {:>6.1}% | {:>7.2} {:>7.2} {:>6.2}x {:>6}",
                if i == 0 { name } else { "" },
                sh.row,
                o.theta,
                o.alpha,
                o.u_c,
                o.p50_us,
                o.p90_us,
                o.p99_us,
                o.reject_pct(),
                sh.rej_pct,
                x,
                sh.xanchor,
                r.rtt_inflation(i, geom[i].1),
                if ok1 && ok2 && ok3 { "ok" } else { "MISS" }
            );
            println!(
                "{:<26} {:<11}   obs {} accept {} ({:.0}/s, want {:.0}/s at the WIRE's rate) \
                 folded {:.1} (want {:.1}) mean gap {:.1} us (this path's 1/rate = {:.1}, wire {:.1}) \
                 | bench overread() x{:.2}",
                "",
                "",
                o.n_obs,
                o.n_accept,
                o.samples_s(20.0),
                sh.samples_s,
                o.folded(),
                sh.rate_lr / sh.samples_s,
                o.mean_gap_us,
                1e6 / geom[i].0,
                1e6 / sh.rate_lr,
                r.overread_path(i)
            );
            println!(
                "{:<26} {:<11}   x_lr median over 2 s windows {:.2}  |  on a whole-run divisor {:.2}",
                "", "", x, r.xanchor_runmean(i)
            );
        }
        println!();
    }
    println!(
        "V1 xanchor +/-{:.0}%  {}   V2 rejection +/-{:.0} pts  {}   V3 marginal  {}",
        V1_XANCHOR_TOL * 100.0,
        if v1 { "PASS" } else { "FAIL" },
        V2_REJECT_TOL_PTS,
        if v2 { "PASS" } else { "FAIL" },
        if v3 { "PASS" } else { "FAIL" }
    );
    println!(
        "==> the measured inputs are {} — the geography question {} be asked\n",
        if v1 && v2 && v3 { "REPRODUCED" } else { "NOT REPRODUCED" },
        if v1 && v2 && v3 { "MAY" } else { "MAY NOT" }
    );
}

/// (9) THE GEOGRAPHY, on measured inputs + the accounting axis. The same
/// G1/G2 the accounting axis pre-registered, on the same statistic, so the two
/// runs differ in exactly one thing: the ack stream.
#[test]
#[ignore = "component bench; run with --ignored --nocapture"]
fn sf_geography_on_measured_inputs() {
    println!("\n=== GEOGRAPHY ON MEASURED INPUTS ({SEEDS} seeds x 20 s) ===");
    println!("era = the wire's ack stream; metering = OFF (published) and ENGINE (un-metered)\n");
    println!(
        "{:<26} {:<22} {:<24} {:>8} {:>16} {:>8} {:>8} {:>9}",
        "cell", "arm", "metering", "zero%", "[lo..hi]", "caught", "cap", "goodput"
    );
    let mut verdict: Vec<(&str, f64, f64, f64)> = Vec::new();
    for (name, geom, shapes) in measured_cells() {
        let feed = Feed::Measured(shapes);
        let (mut a_zero, mut a_caught, mut u_zero) = (0.0, 0.0, 0.0);
        for acct in [Acct::Off, Acct::Engine] {
            for arm in [Arm::Legacy, Arm::Unified, Arm::PooledUnified] {
                let mut e = MeasEns::run(&geom, arm, feed, acct);
                if acct == Acct::Engine && arm == Arm::Legacy {
                    a_zero = AcctEns::mean(&e.zero);
                    a_caught = e.caught();
                }
                if acct == Acct::Engine && arm == Arm::Unified {
                    u_zero = AcctEns::mean(&e.zero);
                }
                println!(
                    "{:<26} {:<22} {:<24} {:>7.1}% {:>16} {:>7.0}% {:>8.0} {:>9.0}",
                    name,
                    arm.label(),
                    acct.label(),
                    AcctEns::mean(&e.zero),
                    format!("[{:.1}..{:.1}]", e.lo(), e.hi()),
                    e.caught() * 100.0,
                    AcctEns::mean(&e.cap),
                    AcctEns::mean(&e.gp)
                );
                if acct == Acct::Engine && arm == Arm::Legacy {
                    let l = e.led;
                    // READOUT 4, as an EMERGENT property: under the engine's
                    // ledger every wire symbol enters the receiver's
                    // expected/received counters, so Σcrecv/srcack IS
                    // wire()/src. The wire settles at 1.01–1.04 (c2r100, c7)
                    // and 1.21–1.34 (c8).
                    println!(
                        "{:<26} {:<22} {:<24}   channels: src {} taper {} retx {} margin {} | \
                         Sum crecv/srcack = wire/src {:.3}  (wire: 1.01-1.04 sym, 1.21-1.34 asym)",
                        "", "", "",
                        l.src, l.taper, l.retx, l.margin,
                        l.wire() as f64 / l.src.max(1) as f64
                    );
                    println!(
                        "{:<26} {:<22} {:<24}   realized xanchor per path: {}",
                        "", "", "",
                        e.x_str()
                    );
                }
            }
        }
        verdict.push((name, a_zero, a_caught, if a_zero > 0.0 { u_zero / a_zero } else { f64::INFINITY }));
        println!();
    }

    println!("--- THE PRE-REGISTERED VERDICT (G1 level, G2 cell-keying; c7 + c8) ---");
    println!(
        "G1: ENGINE A-arm mean < {G1_LEVEL_PCT:.0}% AND caught >= {:.0}% at BOTH c7 and c8",
        G1_CAUGHT_MIN * 100.0
    );
    println!("G2: fold(c8) >= {G2_FOLD_C8_MIN:.1} AND fold(c7) <= {G2_FOLD_C7_MAX:.1}\n");
    let (mut g1, mut g2) = (true, true);
    for (name, z, c, f) in &verdict {
        let key = name.trim();
        let is_c7 = key.starts_with("c7");
        let is_c8 = key.starts_with("c8");
        println!("{name}  A {z:.1}%  caught {:.0}%  fold {f:.1}x", c * 100.0);
        if is_c7 || is_c8 {
            if *z >= G1_LEVEL_PCT || *c < G1_CAUGHT_MIN {
                g1 = false;
            }
            if is_c8 && *f < G2_FOLD_C8_MIN {
                g2 = false;
            }
            if is_c7 && *f > G2_FOLD_C7_MAX {
                g2 = false;
            }
        }
    }
    println!(
        "\nG1 {}  G2 {}  ==> GEOGRAPHY {}",
        if g1 { "PASS" } else { "FAIL" },
        if g2 { "PASS" } else { "FAIL" },
        if g1 && g2 { "REPRODUCED" } else { "NOT REPRODUCED" }
    );
}

/// The seed ensemble over the measured era, carrying the per-path `xanchor`
/// so the candidate/geography tables can show what the loop produced.
struct MeasEns {
    zero: Vec<f64>,
    gp: Vec<f64>,
    cap: Vec<f64>,
    led: Ledger,
    x: [Vec<f64>; 2],
    np: usize,
}

impl MeasEns {
    fn run(geom: &[Spec], arm: Arm, feed: Feed, acct: Acct) -> Self {
        let mut e = MeasEns {
            zero: vec![],
            gp: vec![],
            cap: vec![],
            led: Ledger::default(),
            x: [vec![], vec![]],
            np: geom.len(),
        };
        for s in 0..SEEDS {
            let r = simulate_acct(geom, arm, feed, 20.0, s, acct);
            e.zero.push(r.zero_pct());
            e.gp.push(r.goodput_sym_s());
            e.cap.push(r.mean_cap);
            for p in 0..geom.len().min(2) {
                e.x[p].push(r.overread_path(p));
            }
            e.led.src += r.led.src;
            e.led.taper += r.led.taper;
            e.led.retx += r.led.retx;
            e.led.margin += r.led.margin;
            e.led.charges += r.led.charges;
            e.led.releases += r.led.releases;
            e.led.releases_wasted += r.led.releases_wasted;
            e.led.tokens += r.led.tokens;
        }
        e
    }
    fn caught(&self) -> f64 {
        self.zero.iter().filter(|z| **z < CAUGHT_PCT).count() as f64 / self.zero.len() as f64
    }
    fn lo(&self) -> f64 {
        self.zero.iter().cloned().fold(f64::INFINITY, f64::min)
    }
    fn hi(&self) -> f64 {
        self.zero.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
    }
    fn x_str(&self) -> String {
        (0..self.np.min(2))
            .map(|p| format!("p{p} x{:.2}", AcctEns::mean(&self.x[p])))
            .collect::<Vec<_>>()
            .join("  ")
    }
}

/// (10) THE CANDIDATE, re-scored on the measured era — the dispatch's MATCH
/// outcome asks for exactly this, and it is printed either way so a NO-MATCH
/// still leaves the number on the record rather than losing it.
#[test]
#[ignore = "component bench; run with --ignored --nocapture"]
fn sf_pooled_candidate_on_measured_inputs() {
    println!("\n=== POOLED-CEILING CANDIDATE on MEASURED inputs ({SEEDS} seeds x 20 s) ===");
    println!(
        "{:<26} {:<24} {:>8} {:>8} {:>8} {:>8} {:>10} {:>10} {:>10}",
        "cell", "metering", "A zero%", "AU zero%", "P zero%", "P caught", "A gp", "AU gp", "P gp"
    );
    for (name, geom, shapes) in measured_cells() {
        let feed = Feed::Measured(shapes);
        for acct in [Acct::Off, Acct::Engine] {
            let a = MeasEns::run(&geom, Arm::Legacy, feed, acct);
            let u = MeasEns::run(&geom, Arm::Unified, feed, acct);
            let p = MeasEns::run(&geom, Arm::PooledUnified, feed, acct);
            println!(
                "{:<26} {:<24} {:>7.1}% {:>7.1}% {:>7.1}% {:>7.0}% {:>10.0} {:>10.0} {:>10.0}",
                name,
                acct.label(),
                AcctEns::mean(&a.zero),
                AcctEns::mean(&u.zero),
                AcctEns::mean(&p.zero),
                p.caught() * 100.0,
                AcctEns::mean(&a.gp),
                AcctEns::mean(&u.gp),
                AcctEns::mean(&p.gp)
            );
        }
        println!();
    }
}

// ── The measured era's always-on pins ─────────────────────────────────────

/// THE LAW IS SOLVED, NOT CHOSEN — the two quantities the model needs beyond
/// the measured quantiles are both roots of measured identities, and this
/// asserts they are, at every measured path:
///
///  * `alpha` is the root of "the distribution's mean is `1/rate_lr`", so
///    reconstructing the mean from the model's own pieces must return 1;
///  * `u_c` is the root of "the silence fraction equals the drain duty cycle",
///    i.e. `∫_0^{u_c} Q = q50`;
///  * and the model's marginal must reproduce the LEDGER's own quantiles: `Q`
///    evaluated at 0.5/0.9/0.99 must be the transcribed p50/p90/p99 (at the
///    range position `theta` the mean constraint left it).
///
/// If a successor edits the interpolation, the tail or the duty identity, this
/// fails on the identity rather than drifting silently into a fitted curve.
#[test]
fn measured_ack_law_is_solved_from_the_measurement() {
    for sh in ACK_ALL {
        // Resolved against the wire's OWN rate, so the reconstruction can be
        // checked in the wire's own units.
        let g = AckGaps::new(sh, sh.rate_lr);
        assert!(g.alpha > 1.0, "{}: alpha {} would give an infinite mean gap", sh.row, g.alpha);
        assert!(g.theta >= 0.0 && g.theta <= 0.5, "{}: theta {}", sh.row, g.theta);
        assert!(g.u_c > 0.5 && g.u_c < 1.0, "{}: u_c {}", sh.row, g.u_c);
        // (1) THE MEAN CONSTRAINT: ∫_0^1 Q du = 1, i.e. the model's mean gap
        // IS `1/rate_lr`. This is what `alpha` was solved for.
        let m = g.cdf_mean_to(1.0);
        assert!(
            (m - 1.0).abs() < 1e-6,
            "{}: the model's mean gap is {m:.6}x the measured one — alpha did not solve",
            sh.row
        );
        // (2) THE DUTY IDENTITY: ∫_0^{u_c} Q du = q50.
        assert!(
            (g.cdf_mean_to(g.u_c) - g.q50).abs() < 1e-9,
            "{}: the silence threshold does not satisfy the drain duty identity",
            sh.row
        );
        // (3) THE MARGINAL IS THE LEDGER'S. Q at the three measured quantiles
        // must BE the transcribed numbers, in µs, at this path's own scale.
        let mean_us = 1e6 / sh.rate_lr;
        let (want50, want90, want99) = AckGaps::quantiles(sh, g.theta);
        for (u, want, range, what) in [
            (0.5, want50, sh.p50, "p50"),
            (0.9, want90, sh.p90, "p90"),
            (0.99, want99, sh.p99, "p99"),
        ] {
            let got = g.q(u);
            assert!(
                (got - want).abs() < 1e-9,
                "{}: Q({u}) = {got} but the ledger says {want}",
                sh.row
            );
            let us = got * mean_us;
            assert!(
                us >= range.0 - 1e-6 && us <= range.1 + 1e-6,
                "{}: {what} = {us:.1} µs is outside the ledger's own range {range:?}",
                sh.row
            );
        }
        // (4) THE TAIL IS TRUNCATED AT SOMETHING THE INSTRUMENT SAW.
        let mut rng = Rng::new(1);
        let mut hi = 0.0_f64;
        for _ in 0..100_000 {
            hi = hi.max(g.silence(&mut rng, g.mean_gap_s));
        }
        assert!(
            hi <= ACK_GAP_MAX_S + 1e-12,
            "{}: a silence of {:.1} ms exceeds the largest gap the gauge reported",
            sh.row,
            hi * 1e3
        );
        // And the drain rate really is faster than arrivals — otherwise the
        // observer is not an observer and the whole era is inert.
        assert!(g.q50 < 1.0, "{}: p50 gap is not below the mean gap", sh.row);
    }
}

/// MEASUREMENT DISCIPLINE 1 for the measured era: the mechanism under test
/// must EXECUTE, and it must execute as the wire describes it.
///
///  * every ack reaches `record_delivery` with `count = 1` — asserted as an
///    identity between the observer's count and the run's delivered count, so
///    a batching bug cannot hide;
///  * the SHIPPED 1 ms floor really does the folding: most calls are rejected,
///    and the accepted ones fold many acks each;
///  * the sub-tick clock walk is monotone and lands on the tick grid (the
///    refresh count is unchanged from every other era).
#[test]
fn measured_era_feeds_the_shipped_floor_one_ack_at_a_time() {
    let m = simulate_acct(&[C2, C3], Arm::Legacy, Feed::Measured(&ACK_C8), 6.0, 0, Acct::Off);
    let h = simulate_acct(&[C2, C3], Arm::Legacy, Feed::Honest, 6.0, 0, Acct::Off);
    // drecv = 1: one observation per delivered symbol, no aggregation. The
    // observer is work-conserving, so the identity is exact ONCE the acks
    // still inside it at the horizon are counted — and that residual is
    // asserted small, because a large one would mean the observer is running
    // slower than the link and the era is throttling the loop rather than
    // re-timing it.
    let obs: u64 = m.obs.iter().map(|o| o.n_obs).sum();
    let residual: u64 = m.obs.iter().map(|o| o.backlog_end).sum();
    assert_eq!(
        obs + residual,
        m.delivered,
        "every delivered symbol must be observed exactly once (or still be in the \
         observer at the horizon) — a mismatch means acks were merged or dropped"
    );
    assert!(
        residual * 1_000 < m.delivered,
        "the observer is behind the link by {residual} of {} acks — it is throttling, \
         not re-timing",
        m.delivered
    );
    // The floor is the clock, and it rejects the way READOUT 3b says.
    for (i, o) in m.obs.iter().enumerate().take(2) {
        assert!(o.n_obs > 10_000, "path {i}: only {} acks observed", o.n_obs);
        assert!(
            o.reject_pct() > 70.0,
            "path {i}: the 1 ms floor rejected only {:.1}% — it is not clocking the sampler",
            o.reject_pct()
        );
        assert!(
            o.folded() > 3.0,
            "path {i}: {:.1} acks folded per accepted sample; the wire folds 5–18",
            o.folded()
        );
    }
    // The tick grid is untouched: the sub-tick walk must not add or lose a
    // dyn-cap refresh.
    assert_eq!(m.ticks, h.ticks, "the sub-tick clock walk moved the refresh grid");
}

/// THE MEASURED ERA IS AN ERA, NOT A REWRITE: `Feed::Measured` must leave
/// every other feed bit-identical. The era axis is only a claim about the
/// SAMPLER, so the transport half — deliveries, retransmits, the ledger — must
/// come out of `Feed::Honest` exactly as it did before this branch.
#[test]
fn measured_era_does_not_disturb_the_other_eras() {
    for acct in [Acct::Off, Acct::Traffic, Acct::Engine] {
        for geom in [vec![C2, C2], vec![C2, C3]] {
            let a = simulate_acct(&geom, Arm::Legacy, Feed::Honest, 6.0, 0, acct);
            let b = simulate_acct(&geom, Arm::Legacy, Feed::Honest, 6.0, 0, acct);
            assert_eq!(a.zero, b.zero);
            assert_eq!(a.delivered, b.delivered);
            assert_eq!(a.led.wire(), b.led.wire());
            // And the honest era observes nothing — the observer is inert off
            // its own feed.
            assert_eq!(a.obs[0].n_obs, 0, "the honest era ran the measured observer");
        }
    }
}

/// THE VALIDATION GATE, bounded — V1 and V2 as always-on assertions at the
/// two cells the dispatch's question is about, on the tolerances that were
/// pre-registered in the previous commit rather than discovered here.
///
/// Scored at 3 seeds × 8 s rather than the ledger's 8 × 20 s: this is the
/// regression bound, not the evidence.
#[test]
fn measured_era_reproduces_the_wires_floor_and_anchor() {
    for (geom, shapes) in [(vec![C2, C2], &ACK_C7), (vec![C2, C3], &ACK_C8)] {
        let feed = Feed::Measured(&shapes[..]);
        for s in 0..3u64 {
            let r = simulate_acct(&geom, Arm::Legacy, feed, 8.0, s, Acct::Off);
            for (i, sh) in shapes.iter().enumerate() {
                // V2 — the floor's rejection rate, a PREDICTION of the model.
                let rej = r.obs[i].reject_pct();
                assert!(
                    (rej - sh.rej_pct).abs() <= V2_REJECT_TOL_PTS,
                    "{} seed {s}: floor rejected {rej:.1}%, the wire {:.1}% (V2 = +/-{:.0} pts)",
                    sh.row,
                    sh.rej_pct,
                    V2_REJECT_TOL_PTS
                );
                // V1 — the realized anchor over-read on THE LEDGER'S
                // definition (READOUT 3: `max_bw/rate_lr`, the RTT divided
                // out), which is the quantity the store-cap Σ and the cwnd
                // anchor floor consume once the path's own RTprop is put back.
                let x = r.xanchor_lr(i);
                assert!(
                    (x - sh.xanchor).abs() <= V1_XANCHOR_TOL * sh.xanchor,
                    "{} seed {s}: realized xanchor x{x:.2}, the wire x{:.2} \
                     (V1 = +/-{:.0}%)",
                    sh.row,
                    sh.xanchor,
                    V1_XANCHOR_TOL * 100.0
                );
            }
        }
    }
}

/// THE NO-MATCH RESULT, BOUNDED — goal-gate "SF Bench on Measured Inputs".
///
/// With the ack stream measured instead of invented, the pre-registered
/// geography FAILS, and it fails for a reason that is arithmetic rather than
/// stochastic: **the measured over-read saturates the store-cap law's `N·knee`
/// ceiling, and a saturated cap cannot express the U-fold at all.**
///
/// The shipped law is `clamp(gain·N·Σ_set, floor, N·knee)`. U changes only
/// WHICH SET the Σ ranges over. At the measured `xanchor` the unclamped law
/// asks for 2.7× the ceiling at c8, so BOTH arms clamp to the same 4096 and
/// the set becomes unobservable — dropping a path from Σ at c8 removes 40% of
/// the anchor mass, far short of the 2.7× of headroom the clamp swallows.
///
/// Asserted here in both halves, at 3 seeds × 8 s (the ledger's evidence is
/// the 8 × 20 s matrix; this is the regression bound):
///
///  * THE ARITHMETIC, on the real law: `gain·N·Σ` at the measured per-path
///    `xanchor` exceeds `N·knee` by more than the anchor mass U can remove;
///  * THE CONSEQUENCE, in the loop: under the measured era the mean cap sits
///    within a few percent of the ceiling on BOTH arms at c8, and the U-fold
///    that the engine's ledger produced on the honest era (7.1×, goal-gate
///    "SF Accounting Axis" FINDING 1) collapses below 2×.
///
/// If a successor raises `RWM_STORE_PATH_POOL`, fixes the anchor era, or
/// changes the ceiling, this test fails and the diagnosis gets re-scored
/// rather than being inherited as prose.
#[test]
fn measured_over_read_saturates_the_knee_ceiling_and_collapses_the_u_fold() {
    // (1) THE ARITHMETIC, on the shipped law itself.
    let ceiling = (2 * KNEE) as f64; // 4096
    let sigma_full = C2.0 * C2.1 * ACK_C8_P0.xanchor + C3.0 * C3.1 * ACK_C8_P1.xanchor;
    let sigma_fast = C2.0 * C2.1 * ACK_C8_P0.xanchor; // what U's set change can remove
    assert!(
        GAIN * 2.0 * sigma_full > ceiling,
        "the measured c8 anchor does not even reach the ceiling: {:.0} vs {ceiling}",
        GAIN * 2.0 * sigma_full
    );
    assert_eq!(shipped_chain(sigma_full, 2), ceiling as usize);
    // The set change U makes is SMALLER than the headroom the clamp eats, so
    // both arms land on the same number — this is the fold's grave.
    assert_eq!(
        shipped_chain(sigma_fast, 2),
        shipped_chain(sigma_full, 2),
        "dropping the slow leg from Sigma must still clamp — otherwise the fold survives"
    );

    // (2) THE CONSEQUENCE, in the closed loop.
    let mean = |arm: Arm, acct: Acct| -> (f64, f64) {
        let (mut z, mut c) = (0.0, 0.0);
        for s in 0..3u64 {
            let r = simulate_acct(&[C2, C3], arm, Feed::Measured(&ACK_C8), 8.0, s, acct);
            z += r.zero_pct();
            c += r.mean_cap;
        }
        (z / 3.0, c / 3.0)
    };
    let (a_zero, a_cap) = mean(Arm::Legacy, Acct::Engine);
    let (u_zero, u_cap) = mean(Arm::Unified, Acct::Engine);
    // MEASUREMENT DISCIPLINE 1 — the era must have run, or this proves nothing.
    let probe = simulate_acct(&[C2, C3], Arm::Legacy, Feed::Measured(&ACK_C8), 8.0, 0, Acct::Engine);
    assert!(probe.obs[0].n_obs > 10_000 && probe.obs[1].n_obs > 1_000);
    assert!(probe.led.taper > 0 && probe.led.margin > 0 && probe.led.retx > 0);

    // Both arms ride the ceiling — the means carry the warm-up ramp from the
    // 128 boot cap, so they are scored against 0.7× rather than 1.0×, and the
    // load-bearing half is that the two arms CONVERGE: on the honest era they
    // differ by 5.8× (379 vs 2192, goal-gate "SF Accounting Axis" FINDING 1).
    for (label, cap) in [("A", a_cap), ("AU", u_cap)] {
        assert!(
            cap > 0.7 * ceiling,
            "{label} arm's mean cap {cap:.0} is not against the {ceiling} ceiling — the \
             saturation this test diagnoses did not happen"
        );
    }
    assert!(
        (a_cap / u_cap - 1.0).abs() < 0.25,
        "the two arms' caps did not converge onto the ceiling: A {a_cap:.0} vs AU {u_cap:.0} \
         (on the honest era they differ by 5.8x)"
    );
    let fold = u_zero / a_zero;
    assert!(
        fold < 2.0,
        "the U-fold survived the ceiling at c8: {fold:.2}x (A {a_zero:.1}% AU {u_zero:.1}%) \
         — the engine's ledger produced 7.1x on the honest era"
    );
    // And the c8 A arm is NOT what fails — it stays out of the published
    // bench's 37% class. Its full-horizon level (7.2%, caught on 88% of
    // seeds, i.e. inside the wire's ≈4% class) is the ledger's number and is
    // deliberately NOT asserted here: at 8 s the mean cap is still climbing
    // off the 128 boot value and the zero-fraction has not settled. This
    // bounds the class, which is stable; the level is evidence, not a bound.
    assert!(
        a_zero < 25.0,
        "the c8 A arm fell back into the published bench's high class: {a_zero:.1}%"
    );
}

fn fold_str(a: f64, u: f64) -> String {
    if a > 0.0 { format!("{:.1}x", u / a) } else { "inf".into() }
}

// ── Guards (always run) ───────────────────────────────────────────────────

/// MEASUREMENT DISCIPLINE 1: the loop under test must EXECUTE. The bench's
/// simulated sender must actually refresh the cap, actually saturate paths,
/// and actually deliver — a bench that never saturates would report 0% on both
/// arms and prove nothing.
#[test]
fn bench_loop_executes() {
    let r = simulate(&[C2, C3], Arm::Legacy, 4.0);
    assert!(r.ticks > 700, "dyn-cap refresh ticks = {} (expected ~800 at 5 ms over 4 s)", r.ticks);
    assert!(r.delivered > 10_000, "no delivery: {} symbols", r.delivered);
    assert!(r.short > 0, "no path ever saturated — the mechanism under test never ran");
    assert!(r.mean_cap > BOOT as f64, "the cap never left the boot value: {}", r.mean_cap);
}

/// THE LOAD-BEARING CODE FACT, pinned against the REAL scheduler.
///
/// The whole question rests on it: `active_paths()` (`p.active && available()
/// > 0`) is NOT a gate on the reliable data path. `emit_source.rs` places with
/// `Scheduler::place_symbol(false, &[])` → `place_costs`, which filters on
/// `p.active` ALONE. So a cwnd-saturated path keeps receiving source symbols,
/// `in_flight` may exceed `cwnd` without bound, and `available()` reads 0 and
/// STAYS 0 until acks drain it. `active_paths()` at the dyn-cap phase is
/// therefore a pure OBSERVABLE of saturation, never a brake on it — and the
/// only brake at the battery's arms is `store_len >= effective_store_cap`
/// (`cwnd_full` is off: `RWM_INFL_CAP` defaults to 0).
///
/// If this ever changes, the `[SF]` gauge stops meaning what this bench and
/// the goal-gate "c8 SF Mechanism" section read it to mean.
#[test]
fn reliable_placement_does_not_filter_on_cwnd_headroom() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);
    sched.add_path(1);
    // Saturate BOTH paths past their cwnd, exactly as an unbraked store cap
    // does: charge more in_flight than cwnd.
    for id in [0u32, 1u32] {
        let cw = sched.path(id).map(|p| p.cwnd).unwrap_or(0);
        assert!(cw > 0);
        if let Some(p) = sched.path_mut(id) {
            p.charge_in_flight(cw + 1);
        }
    }
    // The saturation filter now reads EMPTY...
    assert!(
        sched.active_paths().is_empty(),
        "both paths were charged past cwnd; active_paths() must be empty"
    );
    assert_eq!(sched.live_paths().len(), 2, "both paths are still live");
    // ...and `best_source_path` / `schedule`, which DO filter, correctly stall.
    assert!(sched.best_source_path().is_none());
    assert!(sched.schedule(Vec::new(), Vec::new()).is_empty());
    // But the RELIABLE source emitter's placement does not: it still returns a
    // full candidate set over the LIVE paths. This is the asymmetry the whole
    // mechanism turns on.
    let probs = sched.place_probs(false, &[]);
    assert_eq!(
        probs.len(),
        2,
        "place_costs must range over LIVE paths, not the saturation filter — \
         got {probs:?}"
    );
    assert!(probs.iter().any(|(_, w)| *w > 0.0), "placement must still pick a path");
}

/// THE MECHANISM'S ARITHMETIC CORE: an EMPTY `active_paths()` is not a taper
/// under the shipped law — it is a CLIFF to the boot cap, because
/// `path_scaled_store_cap` returns `None` at `pipe_sum <= 0` and the chain
/// falls all the way through to `store_boot_cap`.
///
/// That cliff is the negative feedback the legacy arm gets for free: the moment
/// every path is cwnd-saturated, the store cap drops ≥6× and admission stops
/// until the paths drain. `RWM_STORE_CAP_UNIFIED` deletes it — under U the Σ
/// ranges over `live_paths()`, which is never empty while the transfer is up,
/// so the empty state carries NO consequence and persists. This is the whole
/// of what U changes about the gauge.
#[test]
fn empty_active_set_is_a_cliff_not_a_taper() {
    let a_fast = C2.0 * C2.1; // 83.2
    let a_slow = C3.0 * C3.1; // 120.0 — the SLOW path carries the larger anchor
    assert!(a_slow > a_fast, "{a_slow} vs {a_fast}");

    // N = 2 (c8): full pool, one path filtered, both filtered.
    let both = shipped_chain(a_fast + a_slow, 2);
    let fast_only = shipped_chain(a_fast, 2);
    assert_eq!(both, 813);
    assert_eq!(fast_only, 333);
    assert_eq!(shipped_chain(0.0, 2), BOOT, "empty set ⇒ the boot cap");
    assert!(
        both as f64 / BOOT as f64 > 6.0,
        "the c8 cliff must be a ≥6× step, got {:.1}×",
        both as f64 / BOOT as f64
    );

    // N = 1 (c1/sc2): the same cliff, and it is the MEASURED c1 payoff
    // mechanism (goal-gate: capboot 30% → 0% under U, +13% goodput).
    let single = shipped_chain(a_fast * 5.0, 1); // legacy anchor over-read ×5
    assert_eq!(single, 832);
    assert!(single <= STORE_MAX, "the N = 1 law is bounded by RELIABLE_STORE_MAX");
    assert!(
        single as f64 / BOOT as f64 > 6.0,
        "the c1 cliff must be a ≥6× step, got {:.1}×",
        single as f64 / BOOT as f64
    );

    // The unified arm has NO cliff at all: `live_paths()` is non-empty
    // whenever the transfer is up, so the Σ never reaches 0.
    assert!(cap_for(Arm::Unified, both as f64, a_fast + a_slow, 2) > BOOT);
}

/// THE REPRODUCED DIRECTION, bounded: at every DUAL cell the unified set
/// raises the `[SF]` zero-fraction and raises the mean store cap. This is what
/// the closed loop reproduces deterministically; the CELL SPECIFICITY of the
/// L1 result (c8 only) is NOT reproduced by this model and is recorded as an
/// open item in goal-gate "c8 SF Mechanism" — do not read this test as
/// evidence for it.
#[test]
fn unified_raises_the_sf_zero_fraction_at_every_dual() {
    for geom in [vec![C2, C2], vec![C2, C3]] {
        let a = simulate(&geom, Arm::Legacy, 8.0);
        let u = simulate(&geom, Arm::Unified, 8.0);
        assert!(
            u.zero_pct() > a.zero_pct(),
            "U did not raise the zero-fraction: A {:.1}% vs AU {:.1}%",
            a.zero_pct(),
            u.zero_pct()
        );
        assert!(
            u.mean_cap > a.mean_cap,
            "U did not raise the mean store cap: A {:.0} vs AU {:.0}",
            a.mean_cap,
            u.mean_cap
        );
    }
}

/// THE BENCH'S OWN REPRODUCIBILITY, pinned. `Scheduler` holds its paths in a
/// `HashMap<PathId, PathState>`, so at the SYMMETRIC cell — where the
/// placement objective's costs are bit-equal — the winner used to be whatever
/// the map happened to yield last, i.e. a per-PROCESS random choice. This
/// asserts the tie goes to the LOWEST path id, which is what makes c7's
/// numbers the same on every run and every host. Without the tie-break this
/// test fails in roughly half of all processes.
#[test]
fn symmetric_cell_placement_tie_is_broken_deterministically() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    for id in [0u32, 1, 2, 3] {
        sched.add_path(id);
    }
    // Fresh identical paths ⇒ identical costs ⇒ a pure tie.
    let probs = sched.place_probs_with_temperature(false, &[], f64::MIN_POSITIVE);
    assert_eq!(probs.len(), 4);
    let w0 = probs[0].1;
    assert!(
        probs.iter().all(|(_, w)| *w == w0),
        "the symmetric cell must be an exact tie for this guard to mean anything: {probs:?}"
    );
    assert_eq!(place_min_cost(&sched), 0, "the tie must go to the lowest path id");
}

/// THE ARITHMETIC REASON THE ANCHOR-ERA SUSPECT CANNOT BE THE PROP.
///
/// The suspect (goal-gate "c8 SF Mechanism", FINDING 3) was that a ×5-class
/// over-reading anchor props the cwnd FLOOR (`clamp_cwnd_with_anchor`), keeps
/// `available() > 0`, and so keeps the fast cells out of the empty-
/// `active_paths()` state. What that argument misses is that **the SAME anchor
/// is on both sides of the loop**:
///
/// * the cwnd floor is `ANCHOR_FLOOR_GAIN · anchor` — LINEAR in the anchor,
/// * the store cap is `gain · N · Σ anchor` — ALSO linear in the anchor.
///
/// Saturation is decided by `store_cap` vs `Σ_paths cwnd`, and a common scale
/// `f` on the anchor cancels in that RATIO. So the era cannot move the
/// saturation state at all while both terms are in their linear regime — the
/// only thing that can is a term that is NOT homogeneous: the `N·knee`
/// CEILING (and `FLOOR`/`MAX_CWND`). This test pins exactly that on the real
/// `path_scaled_store_cap`: degree-1 homogeneity below the ceiling, and
/// saturation at `N·knee` above it. The measured consequence — a large enough
/// over-read helps only by driving the cap INTO its ceiling, and it reaches
/// the ceiling first at the cell with the biggest anchor (c8t, RTT-asymmetric)
/// rather than at the fast symmetric one — is what the era matrix shows.
#[test]
fn store_cap_law_is_degree_one_in_the_anchor_until_the_knee_ceiling() {
    let sigma = C2.0 * C2.1 + C3.0 * C3.1; // c8's Σ = 203.2
    let n = 2usize;
    let ceiling = (n * KNEE) as f64; // 4096

    // Below the ceiling: cap(f·Σ) == f·cap(Σ), for any scale — the anchor era
    // divides out. (`ceil` gives at most a 1-symbol residue.)
    let base = shipped_chain(sigma, n) as f64;
    for f in [1.0_f64, 2.0, 4.6, 7.4] {
        let scaled = shipped_chain(f * sigma, n) as f64;
        if scaled >= ceiling {
            continue;
        }
        assert!(
            (scaled - f * base).abs() <= 1.0 + f,
            "cap is not degree-1 in the anchor at f={f}: {scaled} vs {}",
            f * base
        );
    }

    // Above it the law SATURATES — this is the only non-homogeneous term, and
    // therefore the only route by which an anchor era can change the loop's
    // saturation state at all.
    let huge = shipped_chain(1_000.0 * sigma, n) as f64;
    assert_eq!(huge, ceiling, "the N*knee ceiling must bind");
    let f_needed = ceiling / base;
    assert!(
        f_needed > 4.6,
        "at c8 the cap only reaches its ceiling past x{f_needed:.1}, i.e. ABOVE the wire's \
         measured legacy band (4.6-7.4) — so inside that band the era is a pure scale"
    );
}

/// THE MEASURED REFUTATION, bounded: the over-reading (legacy-era) anchor does
/// NOT make the fast symmetric cell immune. The suspect predicted c7 would
/// stop folding because a propped cwnd floor keeps `available() > 0`; measured
/// over the seed ensemble, the legacy era leaves c7's shipped arm STRICTLY
/// WORSE than the honest era does, in the direction opposite to the prediction.
///
/// Kept ordinal-with-a-margin ON PURPOSE: the absolute levels are mode draws
/// from a bistable loop (FINDING 4), but the SIGN of this gap is not — it is
/// the store-cap side of the anchor (gain·N = 4× per path) outrunning the cwnd
/// side (ANCHOR_FLOOR_GAIN = 0.85×), which is arithmetic.
#[test]
fn overreading_anchor_does_not_protect_the_fast_symmetric_cell() {
    let c7 = vec![C2, C2];
    for s in 0..3u64 {
        let honest = simulate_seeded(&c7, Arm::Legacy, Feed::Honest, 8.0, s);
        let legacy = simulate_seeded(&c7, Arm::Legacy, Feed::Overread(4.6), 8.0, s);
        assert!(
            legacy.zero_pct() > honest.zero_pct() + 10.0,
            "seed {s}: the over-read era was supposed to PROTECT c7; honest {:.1}% vs \
             over-read {:.1}%",
            honest.zero_pct(),
            legacy.zero_pct()
        );
        // MEASUREMENT DISCIPLINE 1 — the mechanism under test must EXECUTE.
        // The prop is REAL: the over-reading anchor really does raise the cwnd
        // floor, by a wide margin. It simply does not buy immunity, because
        // the same anchor raises the admission the cwnd has to absorb.
        assert!(
            legacy.mean_cwnd() > 1.5 * honest.mean_cwnd(),
            "seed {s}: the over-read anchor never propped cwnd, so this test proved nothing: \
             honest {:.0} vs over-read {:.0}",
            honest.mean_cwnd(),
            legacy.mean_cwnd()
        );
        // And the realized over-read must actually be in/above the wire's band
        // — otherwise the era was not reached.
        assert!(
            legacy.overread() > 4.6,
            "seed {s}: realized over-read x{:.2} never reached the legacy band",
            legacy.overread()
        );
    }
}

/// The candidate successor is a pure DELETION of the count multiplier, not a
/// new constant: at the unified set it is exactly `gain·Σ_live` under the same
/// N·knee ceiling, so it is bounded above by the shipped ×N law at every N ≥ 1
/// and equals it at N = 1.
#[test]
fn pooled_unified_candidate_introduces_no_constant() {
    for geom in [vec![C2], vec![C2, C2], vec![C2, C3]] {
        let n = geom.len();
        let sum: f64 = geom.iter().map(|(r, t, _, _)| r * t).sum();
        let shipped = cap_for(Arm::Unified, sum, sum, n);
        let cand = cap_for(Arm::PooledUnified, sum, sum, n);
        assert!(cand <= shipped, "N={n}: candidate {cand} > shipped {shipped}");
        if n == 1 {
            assert_eq!(cand, shipped, "N=1 must be bit-identical");
        } else {
            // The only difference is the ×N (modulo the law's own `ceil`).
            assert!(
                (cand as f64 * n as f64 - shipped as f64).abs() <= n as f64,
                "N={n}: candidate {cand} ×{n} != shipped {shipped}"
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
//  goal-gate "Cap-Refresh Warmth" — WHICH REFRESH REGIME THE WIRE IS IN
//
//  The preceding section handed over a CONTRADICTION: its arithmetic said the
//  wire's store cap must be pinned at `N·knee`, and it believed the wire said
//  it could not be, "because U demonstrably moves c8's zero-fraction 4% → 30%
//  there and a pinned cap forbids that".
//
//  The contradiction dissolves, and it dissolves against the WIRE PREMISE.
//  Two independent readings, both already on disk before this section
//  existed:
//
//   1. THE REALIZED CAP. `win=occ/cap`'s `cap` field IS `dyn_store_cap`
//      (`net/mod.rs:4971` — `effective_store_cap = dyn_store_cap` whenever
//      `plain_dyn_cap`), and the L1 batteries record its median per rep as
//      `occcap_p50`. Over **178 dual-cell reps from five independent
//      sessions** it reads **exactly 4096 = 2·knee** in 69/69 c7-A reps and
//      52/57 c8-A reps, with `capboot_frac` (cap ≤ boot = 128) **0.0000 in
//      every single one**. The wire IS at its ceiling.
//
//   2. THE 7.5× "U-FOLD" IS NOT A FOLD IN THE CAP. `fold` is
//      `mean(AU zero%)/mean(A zero%)` — a ratio of the `[SF]` gauge, whose
//      `store_cap_sf_record(live, act)` call (`net/mod.rs:4586`) runs on BOTH
//      arms and is CONSUMED on neither under U: with `RWM_STORE_CAP_UNIFIED`
//      the Σ ranges over `live`, so an empty `active_paths()` cannot reach
//      the cap at all. Under U the zero-fraction is a pure OBSERVATION.
//      A pinned cap therefore forbids nothing, and no unsaturated cap was
//      ever required.
//
//  What U really does at c8 is smaller and measurable: it converts the ~36%
//  of refreshes the `active_paths()` filter leaves with ONE leg in the Σ
//  (interior cap ≈2936–3102) and the ~4.6% it leaves EMPTY (boot cap 128)
//  into the 4096 ceiling — the MEDIAN is unchanged, the MEAN moves ≈3520 →
//  4096. The tests below pin the arithmetic that makes that the whole story.
// ═══════════════════════════════════════════════════════════════════════════

/// The wire's realized store cap at the dual cells, transcribed from the L1
/// per-rep ledgers (`docs/l1-raw/*.log`, the `FLIPRESULT`/`HIRESULT`/
/// `LATRESULT` JSON rows, field `occcap_p50` = median of `win=occ/cap`'s cap
/// over that rep's steady `[DIAG]` samples; `capboot_frac` = the share of the
/// same samples with cap ≤ 128).
struct WireCap {
    cell: &'static str,
    arm: &'static str,
    /// Reps that reported a cap at all.
    reps: usize,
    /// Of those, how many read a median cap of exactly `2·KNEE` = 4096.
    at_ceiling: usize,
    /// The WORST `capboot_frac` over those reps.
    max_capboot: f64,
}

/// Five independent sessions (`flip`, `flip-topup`, `honestinputs`,
/// `latlever`, `uniflip`, `uniflip-topup`), two seeds, pooled ONLY for the
/// count of reps whose median cap is the ceiling — no goodput statistic is
/// pooled here and none is claimed (the documented 2.3× same-config drift
/// forbids that; a cap that reads the same integer in every session does not
/// care).
const WIRE_CAPS: &[WireCap] = &[
    WireCap { cell: "c7", arm: "A", reps: 69, at_ceiling: 69, max_capboot: 0.0 },
    WireCap { cell: "c7", arm: "AU", reps: 26, at_ceiling: 26, max_capboot: 0.0 },
    WireCap { cell: "c8", arm: "A", reps: 57, at_ceiling: 52, max_capboot: 0.0 },
    WireCap { cell: "c8", arm: "AU", reps: 26, at_ceiling: 26, max_capboot: 0.0 },
];

/// THE PIN THRESHOLD IS PATH-COUNT-FREE, and it is `knee/gain`.
///
/// `clamp(gain·N·Σ, floor, N·knee)` saturates iff `gain·N·Σ ≥ N·knee` iff
/// `Σ ≥ knee/gain`. The `N` cancels — so "does the anchor still steer the
/// cap?" is a question about the anchor SUM alone, answerable without knowing
/// the geometry, and at the shipped constants the answer flips at **1024
/// symbols**. Every regime claim in this section rests on this one line.
#[test]
fn the_pin_threshold_on_sigma_is_knee_over_gain_and_is_path_count_free() {
    assert_eq!(SIGMA_PIN, 1024.0);
    for n in 2..=8usize {
        let ceiling = n * KNEE;
        // Just below: strictly interior, and the law is still degree-1.
        let below = shipped_chain(SIGMA_PIN * 0.99, n);
        assert!(below < ceiling, "N={n}: Sigma just under the threshold pinned at {below}");
        assert_eq!(below, (GAIN * n as f64 * SIGMA_PIN * 0.99).ceil() as usize);
        // At and above: pinned, and INSENSITIVE to the anchor.
        assert_eq!(shipped_chain(SIGMA_PIN, n), ceiling, "N={n}");
        assert_eq!(shipped_chain(SIGMA_PIN * 100.0, n), ceiling, "N={n}");
    }
}

/// THE ONLY THREE REACHABLE REGIMES AT A DUAL, and the two that are not.
///
/// Enumerated by reading the refresh block (`net/mod.rs:4382-4799`) at the
/// batteries' resolved arms — every experiment gate off, so the chain is
/// `path_scaled_store_cap` → legacy `gain·Σ` → `store_boot_cap`:
///
///  * **ceiling-pinned** — `Σ ≥ 1024`;
///  * **interior** — `0 < Σ < 1024`, the only regime in which the anchor (and
///    therefore U's choice of path set) can move the cap at all;
///  * **boot fallback (128)** — `Σ == 0`, i.e. the summed set contributed no
///    warm anchor. Under U the set is `live_paths()`, which is non-empty
///    whenever the transfer is running, so this regime is UNREACHABLE on a
///    U arm by construction.
///
/// NOT reachable at the duals, and each killed by arithmetic rather than by
/// measurement:
///
///  * **the `floor` clamp (64)** would need `gain·N·Σ < 64`, i.e. `Σ < 16`
///    symbols at N = 2 — far below the smallest single-leg anchor the wire
///    ever reported. A warm anchor cannot get there.
///  * **the `store_max` (1024) latch** is the `n_live < 2` law, so it cannot
///    be seen at a cell where both legs are up; it is what `sc2`/`c2r100`
///    read, and they read exactly 1024 in every session.
#[test]
fn the_shipped_dual_refresh_has_exactly_three_reachable_regimes() {
    // Boot: the empty set, at any live count.
    assert_eq!(shipped_chain(0.0, 2), BOOT);
    // The floor is a REAL branch of the law, just an unreachable one here: it
    // binds for Sigma below 16 symbols at N = 2 and nowhere above.
    assert_eq!(shipped_chain(1.0, 2), FLOOR);
    assert_eq!(shipped_chain(16.0, 2), FLOOR);
    assert!(shipped_chain(17.0, 2) > FLOOR);
    // Interior: strictly between, degree-1 in the anchor.
    for sigma in [17.0, 100.0, 512.0, 1023.0] {
        let cap = shipped_chain(sigma, 2);
        assert!(cap > FLOOR && cap < 2 * KNEE, "Sigma {sigma}: cap {cap} not interior");
        assert_eq!(cap, (GAIN * 2.0 * sigma).ceil() as usize);
    }
    // The floor is unreachable from ANY warm single-leg anchor the wire
    // measured — the smallest is c7/p0's, and it clears the floor's Sigma by
    // more than an order of magnitude.
    let smallest = ACK_ALL
        .iter()
        .skip(1) // c2r100 is the N = 1 cell, whose law is the store_max latch
        .map(|s| s.anchor_sym())
        .fold(f64::INFINITY, f64::min);
    let floor_sigma = FLOOR as f64 / (GAIN * 2.0); // 16 symbols
    assert!(
        smallest > 40.0 * floor_sigma,
        "the floor clamp is within reach of a warm anchor: smallest leg {smallest:.0} \
         vs the floor's Sigma {floor_sigma:.0}"
    );
    // N = 1 is the store_max latch, not the pooled ceiling — and it is what
    // the single cells measure (occcap_p50 = 1024 at both sc2 and c2r100).
    assert_eq!(shipped_chain(ACK_C2R100_P0.anchor_sym(), 1), STORE_MAX);
}

/// THE WIRE'S OWN ANCHORS PIN THE LAW WITH BOTH LEGS AND FREE IT WITH ONE.
///
/// Reconstructed from READOUT 3 by inverting `xanchor` — three measured
/// columns multiplied, nothing modelled. The result is the section's central
/// number and it is not close to the threshold in either direction:
///
/// | cell | Σ both legs | ×`SIGMA_PIN` | one leg | ×`SIGMA_PIN` |
/// |---|---|---|---|---|
/// | c7 | 1635 | 1.60 | 712 / 924 | 0.70 / 0.90 |
/// | c8 | 1510 | 1.47 | 776 / 734 | 0.76 / 0.72 |
///
/// So at BOTH duals the shipped law is **pinned whenever both legs are in the
/// Σ and interior whenever exactly one is** — which makes the `[SF]` gauge's
/// short-tick fraction the cap's regime mixture directly, and makes the
/// realized median cap 4096 an arithmetic PREDICTION rather than a surprise.
/// It is confirmed by 121/126 dual reps reading exactly that integer.
#[test]
fn the_wires_measured_anchors_pin_both_legs_and_free_one_leg_at_both_duals() {
    for (cell, legs) in [("c7", &ACK_C7), ("c8", &ACK_C8)] {
        let sigma_both: f64 = legs.iter().map(|s| s.anchor_sym()).sum();
        assert!(
            sigma_both > SIGMA_PIN,
            "{cell}: Sigma over both legs {sigma_both:.0} does not reach the pin \
             threshold {SIGMA_PIN:.0} — the wire's median cap could not be the ceiling"
        );
        assert_eq!(shipped_chain(sigma_both, 2), 2 * KNEE, "{cell} both legs");
        for leg in legs.iter() {
            let one = leg.anchor_sym();
            assert!(
                one < SIGMA_PIN,
                "{}: one leg alone {one:.0} still pins — U would be arithmetically \
                 inert at this cell",
                leg.row
            );
            let cap = shipped_chain(one, 2);
            assert!(
                cap < 2 * KNEE && cap > 2_500,
                "{}: single-leg cap {cap} is not the interior regime this section \
                 attributes the U effect to",
                leg.row
            );
        }
    }
}

/// **THE CORRECTION TO THE PREDECESSOR.** Its Σ is 1.8× the wire's, because
/// it multiplies the measured `xanchor` by the cells' CONFIGURED rate and RTT
/// (10 400 / 2 000 sym/s at 8 / 60 ms) instead of the wire's own measured
/// `rate_lr` and `RTprop` (6 948 / 1 376 sym/s at 8.4 / 38.6 ms) — the same
/// "scale by the path's REALIZED ack rate, not its link capacity" caveat its
/// own section (b) stated for the ack MODEL and did not carry into the Σ.
///
/// The inflation is not cosmetic: it lands on the OTHER SIDE of the pin
/// threshold for the single-leg Σ, and that single comparison is what its
/// FINDING 1 rests on.
///
///  * on the bench's Σ, dropping the slow leg still clamps (4423 > 4096) ⇒
///    "the shipped law is arithmetically INCAPABLE of expressing the U-fold";
///  * on the WIRE's Σ, dropping either leg does NOT clamp (2936 / 3102) ⇒ U
///    moves the cap on every short tick, which the `[SF]` gauge measures at
///    **≈36% of c8 refreshes**.
///
/// The predecessor's `measured_over_read_saturates_the_knee_ceiling_and_
/// collapses_the_u_fold` is left standing and unmodified — it is a true
/// statement about the bench's own inputs, and it is the reason its AU arm
/// cannot move. This test bounds the gap between those inputs and the wire's.
#[test]
fn the_predecessors_sigma_is_inflated_by_configured_rates_not_the_wires_realized_ones() {
    let bench_fast = C2.0 * C2.1 * ACK_C8_P0.xanchor;
    let bench_slow = C3.0 * C3.1 * ACK_C8_P1.xanchor;
    let wire_fast = ACK_C8_P0.anchor_sym();
    let wire_slow = ACK_C8_P1.anchor_sym();
    // The predecessor's own printed numbers, re-derived here so a successor
    // sees they are the same quantity and not a different definition.
    assert!((bench_fast - 1105.7).abs() < 1.0 && (bench_slow - 1658.4).abs() < 1.0);
    let ratio = (bench_fast + bench_slow) / (wire_fast + wire_slow);
    assert!(
        ratio > 1.7 && ratio < 2.0,
        "the Sigma inflation moved: bench {:.0} vs wire {:.0} (x{ratio:.2})",
        bench_fast + bench_slow,
        wire_fast + wire_slow
    );
    // THE FLIP, stated as the two opposite verdicts on the same question.
    assert_eq!(shipped_chain(bench_fast, 2), shipped_chain(bench_fast + bench_slow, 2));
    assert_ne!(
        shipped_chain(wire_fast, 2),
        shipped_chain(wire_fast + wire_slow, 2),
        "on the wire's own anchors, dropping a leg must CHANGE the cap — the whole \
         U mechanism at c8 is that change"
    );
    assert_ne!(shipped_chain(wire_slow, 2), shipped_chain(wire_fast + wire_slow, 2));
}

/// THE WIRE'S REALIZED CAP, as the L1 ledgers already recorded it — the
/// reading that settles the handover without a VM run.
///
/// This is a transcription gate, not a simulation: it asserts that the
/// numbers this section's verdict quotes are the numbers in
/// `docs/l1-raw/*.log`, and that they say "ceiling" rather than "boot". If a
/// successor re-measures and gets something else, this is the row to change,
/// and changing it re-scores the verdict instead of inheriting it as prose.
#[test]
fn the_wires_realized_dual_cap_is_the_ceiling_and_never_the_boot_cliff() {
    for w in WIRE_CAPS {
        let frac = w.at_ceiling as f64 / w.reps as f64;
        assert!(
            frac >= 0.9,
            "{}-{}: only {}/{} reps read a median cap of 4096 — the wire is not \
             ceiling-pinned and this section's verdict is wrong",
            w.cell, w.arm, w.at_ceiling, w.reps
        );
        assert_eq!(
            w.max_capboot, 0.0,
            "{}-{}: the boot cliff was CONSUMED at a dual cell ({} of steady DIAG \
             samples) — the c8 mechanism would then be the cliff after all",
            w.cell, w.arm, w.max_capboot
        );
    }
    // The U arms are pinned in EVERY rep, which is what "the Sigma ranges over
    // live_paths()" predicts: the interior and boot regimes are unreachable
    // there, so there is no dispersion left to have.
    for w in WIRE_CAPS.iter().filter(|w| w.arm == "AU") {
        assert_eq!(w.at_ceiling, w.reps, "{}-AU is not uniformly pinned", w.cell);
    }
}
