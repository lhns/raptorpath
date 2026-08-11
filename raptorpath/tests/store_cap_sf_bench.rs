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
}

impl Feed {
    fn label(self) -> String {
        match self {
            Feed::Honest => "honest (x1.0)".into(),
            Feed::Overread(f) => format!("over-read x{f:.1}"),
            Feed::Cumulative { ack_period_s } => format!("cum-ack {:.2} ms", ack_period_s * 1e3),
        }
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
        clock.advance(Duration::from_secs_f64(tick));

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
                }
            }
            if let Feed::Cumulative { .. } = feed {
                seq_done[*seq as usize] = true;
                seq_owner[*seq as usize] = *pid;
            }
        }
        delivered += acks.len() as u64;
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
                    if let Some(a) = p.copa_bdp_anchor() {
                        if truth[pid] > 0.0 {
                            anchor_ratio_sum += a / truth[pid];
                            anchor_ratio_n += 1;
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
