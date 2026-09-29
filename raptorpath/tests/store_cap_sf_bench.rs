//! Closed-loop component bench for the store cap and the `[SF]` gauge (the
//! fraction of dyn-cap refreshes that see `active_paths()` empty), per
//! `docs/measurement-discipline.md` rule 14. A static cap comparison
//! (`store_cap_bench.rs`) cannot reproduce the gauge, because the effect is a
//! loop: cap → admission → in_flight → `available()` → `active_paths()` → cap.
//! This bench closes that loop with the real `Scheduler` / `PathState` (Copa-lite
//! cwnd, `copa_bdp_anchor()`, `active_paths()` / `live_paths()`,
//! `best_source_path()`), a `MockClock`, a deterministic bottleneck-link model
//! per path, and the shipped dyn-cap chain refreshed on its 5 ms cadence. It
//! pins the candidate cap laws (paper §6.1) against that loop. No wall clock,
//! sockets or tokio: the same numbers every run.
//!
//! Run:
//!   cargo test --test store_cap_sf_bench --release -- --ignored --nocapture

use std::sync::Arc;
use std::time::Duration;

use raptorpath::control::fec_rate::ProtocolHint;
use raptorpath::control::FecRateController;
use raptorpath::fec::FecBackend;
use raptorpath::net::{
    delta_budget_b, path_scaled_store_cap, three_term_store_cap, three_term_terms, EchoRatioMin,
    received_sack_ranges, ThreeTermPath, ThreeTermTerm, MAX_SACK_RANGES, WIN_STORE_MAX,
};
use raptorpath::scheduler::{MockClock, Scheduler};

/// The resolved `contract_rho` default at every arm (`sender_policy`).
const TT_RHO: f64 = 1.0;

// ── Shipped policy constants at the battery's arms (sender_policy::resolve) ──
const GAIN: f64 = 2.0;
/// Cited, not transcribed: the floor is a derived quantity
/// (`max(ANCHOR_MIN_SAMPLES·cadence, RFC 6928 IW)` = 10, paper §6.1), so the
/// bench tracks the shipped value.
const FLOOR: usize = raptorpath::net::sender_policy::STORE_CAP_FLOOR;
const KNEE: usize = 2048; // RWM_STORE_PATH_POOL
const STORE_MAX: usize = 1024; // RELIABLE_STORE_MAX
const BOOT: usize = 128; // RWM_STORE_BOOT
const REFRESH_S: f64 = 0.005; // the dyn-cap refresh throttle

// ── Shipped FEC-controller constants ────────────────────────────────────────
// The resolved defaults the arms run at (`config::resolve`): tail loss, max
// overhead, the Auto hint, the RaptorQ backend, the bulk profile's symbol size.
const TAIL_LOSS: f64 = 1e-5;
const MAX_OVERHEAD: f64 = 0.5;
const SYMBOL_SIZE: u16 = 1200;
/// The report task's cadence — the only production feed of
/// `LossEstimator::record_throughput`, gated on its own `dt > 0.2`.
const REPORT_S: f64 = 2.0;

// ── The cells, at the parameters store_cap_bench.rs already quotes ──────────
// c2 = 100 Mbit / 10 ms RTT, GE 1.3%/50% ⇒ 10 400 sym/s, RTprop 8 ms (anchor 83.2)
// c3 =  20 Mbit / 40 ms RTT, GE 2%/40%   ⇒  2 000 sym/s, RTprop 60 ms (anchor 120.0)
const C2: Spec = (10_400.0, 0.008, 0.013, 0.50);
const C3: Spec = (2_000.0, 0.060, 0.020, 0.40);

/// The widest geometry this bench's per-path gauges are sized for. N = 4 is
/// the axis on which the pooled law's N² value and its N ceiling separate.
const MAX_PATHS: usize = 4;

/// `c7x4` — the symmetric quad: c7's legs (`C2`) at N = 4. Not a wire cell;
/// it gives the store-cap laws N ≥ 3 coverage, and it is symmetric so the
/// law's own path-count scaling is the only thing that changes against `c7`.
///
/// A four-way symmetric cell ties at once in the three places determinism
/// depends on (`place_min_cost`'s path-id tie-break, `worst_loss_path`'s sort
/// before `max_by`, per-path link seeds derived from the path index), which
/// makes it the strongest determinism probe in the file
/// (`the_symmetric_quad_is_deterministic_and_all_four_legs_carry_and_warm`).
fn c7x4() -> Vec<Spec> {
    vec![C2, C2, C2, C2]
}

/// Which path set the dyn-cap phase's Σ-anchor base iterates, and which
/// pooled ceiling composes it — the two axes the shipped chain fixes and the
/// candidate successor varies.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Arm {
    /// `RWM_STORE_CAP_UNIFIED=0`: Σ over `active_paths()`, ×N pooled law.
    Legacy,
    /// `RWM_STORE_CAP_UNIFIED=1`: Σ over `live_paths()`, ×N pooled law.
    Unified,
    /// The pooled ceiling composed with the unified set: Σ over
    /// `live_paths()` without the ×N count multiplier, the N·knee ceiling
    /// kept. `cap = clamp(gain·Σ_live, floor, N·knee)`. No new constant.
    PooledUnified,
    /// The three-term law as the dual-cell cap (`RWM_THREE_TERM`): the shipped
    /// `net::three_term_store_cap` over `live_paths()`, at the engine's
    /// precedence — the law when every live path is warm, the configured
    /// pooled chain verbatim when any is cold. Inputs are the engine
    /// collector's (`btlbw_sym_per_s` / `srtt` / `min_rtt` / `k_raw` through
    /// `three_term_terms`), with `contract_rho = 1.0` and
    /// `delta_b = delta_budget_b(hint)`. Its term 3, `2·rate_fast·skew`, is
    /// the quantity the coupling axis makes the bench produce.
    ThreeTermCell,
    /// The composed cap law (`RWM_COMPOSED_CAP`, paper §10): the pool is
    /// [`Arm::ThreeTermCell`]'s, bit-identically; the only addition is the
    /// late-stage per-path brake `cwnd_full`, whose per-path cap is the path's
    /// own cwnd. The difference from `3T` is therefore a pure brake
    /// measurement.
    ///
    /// The set is load-bearing at the brake. The reliable source path has no
    /// `available() > 0` filter, so `in_flight_i` may exceed `cwnd_i` and
    /// `available()` stays 0; iterating `active_paths()` would ask a question
    /// false by construction. The brake reads `live_paths()`: every live path
    /// is at or above its own congestion window.
    Composed,
}

impl Arm {
    fn label(self) -> &'static str {
        match self {
            Arm::Legacy => "A   (U=0, shipped)",
            Arm::Unified => "AU  (U=1)         ",
            Arm::PooledUnified => "P   (pooled+unified)",
            Arm::ThreeTermCell => "3T  (three-term cap)",
            Arm::Composed => "C   (composed law) ",
        }
    }
    /// Does this arm arm the late-stage per-path brake? Only the composed one.
    fn brake_on(self) -> bool {
        self == Arm::Composed
    }
}

/// `cwnd_full` at the composed arm: every live path is at or above its own
/// congestion window (`available() == 0`). The engine's own predicate
/// (`net::infl_percap_full` over `live_paths()` with `cap_i = cwnd_i`),
/// evaluated on the bench's real `Scheduler`.
///
/// The `live_paths()` set is the load-bearing part — see [`Arm::Composed`].
fn composed_brake_closed(sched: &Scheduler, arm: Arm) -> bool {
    if !arm.brake_on() {
        return false;
    }
    let live = sched.live_paths();
    !live.is_empty()
        && live
            .iter()
            .all(|id| sched.path(*id).map(|p| p.available() == 0).unwrap_or(false))
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

fn cap_for(arm: Arm, bdp_over_set: f64, bdp_over_live: f64, n_live: usize, tt: Option<usize>) -> usize {
    match arm {
        Arm::Legacy | Arm::Unified => shipped_chain(bdp_over_set, n_live),
        // The engine's precedence: the law wins when it returns `Some`,
        // otherwise the configured chain runs verbatim.
        // The composed arm's POOL is bit-identically the three-term arm's —
        // one law, one implementation; the composition it adds is the BRAKE,
        // applied at admission rather than here.
        Arm::ThreeTermCell | Arm::Composed => {
            tt.unwrap_or_else(|| shipped_chain(bdp_over_live, n_live))
        }
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
/// queue, then a fixed one-way `rtprop`, with a Gilbert–Elliott loss process
/// (the cells are defined with GE loss, and retransmits ride the cap). A
/// symbol sent while the queue is backed up waits — which is how a
/// cwnd-saturating sender manufactures the delay signal Copa backs off on.
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
    /// `resolve_time` is when the sender learns this symbol's fate, the ack
    /// instant whether or not it survived: a loss is reported by the same
    /// feedback message (the receiver's expected/received counters), which is
    /// what the engine's counter-delta release reads. Dropped symbols still
    /// consume the bottleneck.
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
    /// and carried across retransmits — the number the receiver's cumulative
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

// ── The in-flight accounting axis ──────────────────────────────────────────
//
// The engine's recovery traffic is partly un-metered and its in-flight ledger
// does not balance by construction. Three divergences:
//
//   (a) Repair rides token-free. `emit_source` debits the CC token bucket in
//       the source arm only; the taper correction symbol consumes the wire and
//       no token, so the realized wire rate is src·(1+r). It is charged to
//       in_flight. (With `RWM_CC_PACE=0`, the default, the bucket does not run,
//       so the divergence is wire occupancy, not spacing; the token counter is
//       carried here to bound that.)
//   (b) Two channels bypass the charge: the SACK-gap retransmit and the NACK
//       repair margin (`margin = ceil(retransmitted × max_active_loss)`) call
//       `transport.send_symbols` without `charge_in_flight`.
//   (c) Release is counter-delta driven, not 1:1 with charges. `control_msg`
//       releases `expected − received` on the path the feedback arrived on;
//       the receiver builds those counters from per-batch symbol counts, so
//       every wire symbol enters them whether or not it was charged.
//       `release_in_flight` saturates at zero, so a wasted release is lost.
//
// Three levels, so that the recovery traffic existing (wire and queue
// occupancy) is not confounded with the ledger not balancing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Acct {
    /// The published bench: source + retransmit only, every wire symbol
    /// charged once and released once on its own path, no estimator feed.
    Off,
    /// The recovery traffic exists — taper repair at the shipped r*, the NACK
    /// repair margin, and the loss estimator/throughput feeds that produce
    /// them — but the ledger balances: every wire symbol is charged once, on
    /// the path it flies, and released once, on the same path.
    Traffic,
    /// The engine: as `Traffic`, plus (b) and (c) — retransmits and margin
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

// ── The coupling axis: what `store_len` counts ─────────────────────────────
//
// What a deeper retention pool does to `available()` depends on what the store
// counts. The bench's `Unacked` store is `sent − acked`, draining at path
// latency. The engine's is a frontier span that drains at cumulative-frontier
// latency, minus what the receiver has SACK-advertised (ADR-0060, paper §6.3):
//
// (1) The store is a dense span. Every reliable source symbol enters
//     `sent_store` keyed by its stream seq; the only removal is
//     `split_off(&(ack + 1))`, clocked by the cumulative ack. So
//     `|sent_store| = last_sent − ack`.
// (2) The cumulative ack is the receiver's in-order delivery point
//     (`received_up_to: highest_delivered_seq`), folded into the sender's
//     `window_ack_seq` with `fetch_max`. It advances only when in-order
//     delivery passes a hole.
// (3) SACK release uncounts, it does not remove:
//     `sack_release_outstanding(store_len, released) =
//     store_len.saturating_sub(released)` over the marks of currently
//     retained seqs, pruned at the cumulative twin.
// (4) The marks arrive on a rate-limited clock. SACK ranges ride only an ACK
//     with `advertise = cumulative_advanced || gap_report_due`, and
//     `gap_report_due` requires `GAP_ACK_MIN_INTERVAL` (2 ms) since the last
//     one. While the frontier is stalled the sender learns what lies above
//     the hole at most every 2 ms, plus the return flight.
// (5) The ranges are a PREFIX of the received runs in `(highest_delivered,
//     highest_seen]` — complete up to `MAX_SACK_RANGES` runs, cut before the
//     first unreportable hole beyond that (plan 2a). Below the cap a later
//     report subsumes an earlier one and the union is the newest snapshot;
//     past it the prefixes need not nest, so the sender holds the UNION of
//     every landed report, exactly as the engine's `sack_released` mark set
//     does (`sack_snapshots_subsume_and_the_union_is_the_newest`).
//
// The admission gate reads (3):
// `reliable && (store_len >= effective_store_cap || cwnd_full)`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Store {
    /// `store_len` = admitted − acked, each symbol leaving at its own ack
    /// instant.
    Unacked,
    /// The engine: `store_len = (last_sent − cum_frontier) − |SACK marks|`,
    /// the frontier span of ADR-0060 with the marks arriving on the
    /// receiver's gap-report clock.
    Span,
}

impl Store {
    fn label(self) -> &'static str {
        match self {
            Store::Unacked => "UNACKED (published)",
            Store::Span => "SPAN (frontier)    ",
        }
    }
}

// ── The source axis: what offers the load ──────────────────────────────────
//
// An "always data to send" source is not the only load. The L1 wire's source
// is: `perf --client` drives a memory-backed TUN and `perf.rs::run_object` is
// an open loop bounded only by the mpsc channel's capacity (pinned by
// `the_wires_offered_load_has_no_congestion_control`). The `Src::Reno` arm
// therefore models a deployed tunnel (a user's TCP over the TUN), not the L1
// battery.
//
// The model is Reno-class; every constant is a cited standard:
//
//   * One segment = one tunnel symbol (the window pipeline carries at most
//     one packet per symbol), so no conversion constant exists.
//   * IW = 10 segments (RFC 6928 §1).
//   * ssthresh starts arbitrarily high (RFC 5681 §3.1): slow start first.
//   * Slow start `cwnd += 1` per acked segment; congestion avoidance
//     `cwnd += 1/cwnd` per acked segment (RFC 5681 §3.1).
//   * Multiplicative decrease `ssthresh = max(FlightSize/2, 2)` (RFC 5681
//     §3.1 eq. 4).
//   * RTO from RFC 6298: α = 1/8, β = 1/4, K = 4 (§2.3); first sample
//     SRTT = R, RTTVAR = R/2 (§2.2); `RTO = max(SRTT + 4·RTTVAR, 1 s)` (§2.4)
//     capped at 60 s (§2.5); doubled on each expiry (§5.5). On expiry
//     `cwnd = 1` (RFC 5681 §3.1).
//
// The feedback path is the tunnel's own latency, not injected: the inner
// flow's in-flight is `next_seq − snd_frontier` (the receiver's cumulative
// in-order point), and its RTT sample is the wall between a segment's
// admission and the frontier passing it. Its throughput is therefore
// `w / (RTprop + tunnel queue + recovery stall)`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Src {
    /// An infinite offered load (the published bench).
    Bulk,
    /// A Reno-class inner flow over the tunnel, RTT-clocked on the tunnel's
    /// own delivered latency.
    Reno,
}

impl Src {
    fn label(self) -> &'static str {
        match self {
            Src::Bulk => "BULK (published)",
            Src::Reno => "RENO (closed loop)",
        }
    }
}

/// RFC 6928 §1 — the standard initial window, in segments.
const RENO_IW: f64 = 10.0;
/// RFC 5681 §3.1 eq (4) — the multiplicative-decrease factor, and its floor.
const RENO_BETA: f64 = 0.5;
const RENO_MIN_SSTHRESH: f64 = 2.0;
/// RFC 6298 §2.3 — the SRTT/RTTVAR gains and the variance multiplier.
const RFC6298_ALPHA: f64 = 1.0 / 8.0;
const RFC6298_BETA: f64 = 1.0 / 4.0;
const RFC6298_K: f64 = 4.0;
/// RFC 6298 §2.4 / §2.5 — the RTO bounds.
const RFC6298_RTO_MIN_S: f64 = 1.0;
const RFC6298_RTO_MAX_S: f64 = 60.0;

/// One Reno-class inner flow, clocked by the tunnel's delivered latency.
#[derive(Debug, Clone)]
struct RenoSource {
    /// Congestion window, in segments (= tunnel symbols).
    w: f64,
    ssthresh: f64,
    srtt: f64,
    rttvar: f64,
    rto: f64,
    have_sample: bool,
    /// `(seq, admission instant)` for every segment handed to the tunnel and
    /// not yet passed by the cumulative frontier — FIFO, because the frontier
    /// is monotone and in order.
    outstanding: std::collections::VecDeque<(u64, f64)>,
    /// Gauges. `rto_events` counts timeouts; `rtt_sum`/`rtt_n` is the
    /// delivered latency the inner flow experienced (the user-visible cost);
    /// `w_sum`/`w_n` is the window's time average over admission ticks.
    rto_events: u64,
    rtt_sum: f64,
    rtt_n: u64,
    w_sum: f64,
    w_n: u64,
    /// Admission opportunities at which the inner window was the binder (the
    /// bench's `wait_tun` analogue) and at which the store cap was
    /// (`wait_paused`). Sampled once per tick, before any admission.
    src_bound: u64,
    cap_bound: u64,
}

impl RenoSource {
    fn new() -> Self {
        Self {
            w: RENO_IW,
            ssthresh: f64::INFINITY,
            srtt: 0.0,
            rttvar: 0.0,
            rto: RFC6298_RTO_MIN_S,
            have_sample: false,
            outstanding: std::collections::VecDeque::new(),
            rto_events: 0,
            rtt_sum: 0.0,
            rtt_n: 0,
            w_sum: 0.0,
            w_n: 0,
            src_bound: 0,
            cap_bound: 0,
        }
    }

    /// RFC 6298 §2.2/§2.3 — one RTT measurement folded into SRTT/RTTVAR/RTO.
    fn sample_rtt(&mut self, r: f64) {
        if !self.have_sample {
            self.srtt = r;
            self.rttvar = r / 2.0;
            self.have_sample = true;
        } else {
            self.rttvar =
                (1.0 - RFC6298_BETA) * self.rttvar + RFC6298_BETA * (self.srtt - r).abs();
            self.srtt = (1.0 - RFC6298_ALPHA) * self.srtt + RFC6298_ALPHA * r;
        }
        self.rto = (self.srtt + RFC6298_K * self.rttvar).clamp(RFC6298_RTO_MIN_S, RFC6298_RTO_MAX_S);
        self.rtt_sum += r;
        self.rtt_n += 1;
    }

    /// The cumulative frontier advanced to `frontier`: retire every segment it
    /// passed, take their RTT samples, and grow the window one ACK at a time —
    /// RFC 5681 §3.1, slow start then congestion avoidance, with no branch on
    /// anything but `cwnd < ssthresh`.
    fn on_frontier(&mut self, frontier: u64, now: f64) {
        while let Some(&(seq, sent)) = self.outstanding.front() {
            if seq >= frontier {
                break;
            }
            self.outstanding.pop_front();
            self.sample_rtt(now - sent);
            if self.w < self.ssthresh {
                self.w += 1.0;
            } else {
                self.w += 1.0 / self.w;
            }
        }
    }

    /// RFC 6298 §5.5 + RFC 5681 §3.1 — the retransmission timer expired on the
    /// oldest outstanding segment. The tunnel is reliable, so the inner flow
    /// has nothing to retransmit that the tunnel is not already
    /// retransmitting; what the timeout does is collapse the offered load,
    /// which is the mechanism under test.
    fn check_rto(&mut self, now: f64) {
        let Some(&(_, sent)) = self.outstanding.front() else {
            return;
        };
        if now - sent <= self.rto {
            return;
        }
        let flight = self.outstanding.len() as f64;
        self.ssthresh = (flight * RENO_BETA).max(RENO_MIN_SSTHRESH);
        self.w = 1.0;
        self.rto = (self.rto * 2.0).min(RFC6298_RTO_MAX_S);
        self.rto_events += 1;
        // The timer restarts on the same segment (§5.5's "start the
        // retransmission timer"), which this model expresses by re-stamping
        // the head's send instant. Nothing else in the queue moves.
        if let Some(front) = self.outstanding.front_mut() {
            front.1 = now;
        }
    }

    /// The offered-load gate: how many segments the inner flow may have in the
    /// tunnel right now.
    fn window(&self) -> u64 {
        self.w.floor().max(1.0) as u64
    }

    fn admit(&mut self, seq: u64, now: f64) {
        self.outstanding.push_back((seq, now));
    }
}

/// `GAP_ACK_MIN_INTERVAL` — the receiver's gap-report rate limit and
/// therefore the SACK-release clock while the frontier is stalled.
const GAP_ACK_MIN_S: f64 = 0.002;

/// One receiver→sender feedback message carrying a cumulative point and a
/// SACK snapshot, in flight for the return half of the path it was emitted
/// on.
struct Report {
    arrive_at: f64,
    /// `received_up_to + 1`: the count of contiguously delivered seqs.
    frontier: u64,
    /// `received_sack_ranges(...)`, inclusive.
    ranges: Vec<(u64, u64)>,
}

/// `received_sack_ranges` on the bench's receiver set — the engine's own
/// encoder, so the snapshot carries its `MAX_SACK_RANGES` prefix cap.
/// `frontier` is the count of contiguously delivered seqs, so
/// `delivered = frontier − 1` and the scan starts at `frontier`. At
/// `frontier = 0` seq 0 is undelivered, hence not in `seen` (a received
/// seq 0 is delivered at once), so starting the engine's scan at 1 loses
/// nothing; `highest < frontier` ⇒ an empty SACK list.
fn sack_snapshot(seen: &std::collections::BTreeSet<u64>, frontier: u64, highest: u64) -> Vec<(u64, u64)> {
    received_sack_ranges(seen, frontier.saturating_sub(1), highest)
}

/// The sender's `sack_released` mark set as ranges: fold one landed report
/// into the union, dropping marks the cumulative twin has pruned (`< frontier`).
fn union_marks(marks: &mut Vec<(u64, u64)>, add: &[(u64, u64)], frontier: u64) {
    let mut all: Vec<(u64, u64)> = marks.iter().chain(add.iter()).copied().collect();
    all.sort_unstable();
    let mut out: Vec<(u64, u64)> = Vec::with_capacity(all.len());
    for (a, b) in all {
        if b < frontier {
            continue;
        }
        let a = a.max(frontier);
        match out.last_mut() {
            Some((_, e)) if a <= e.saturating_add(1) => *e = (*e).max(b),
            _ => out.push((a, b)),
        }
    }
    *marks = out;
}

/// `|ranges ∩ [frontier, next_seq)|` — the released-mark count the release
/// law subtracts. Marks exist only for currently retained seqs and are
/// pruned at the cumulative twin, which is exactly this clamp.
fn released_count(ranges: &[(u64, u64)], frontier: u64, next_seq: u64) -> usize {
    let mut n = 0usize;
    for &(a, b) in ranges {
        let lo = a.max(frontier);
        let hi = b.min(next_seq.saturating_sub(1));
        if hi >= lo {
            n += (hi - lo + 1) as usize;
        }
    }
    n
}

/// One un-stored recovery flight: a taper repair or a NACK margin repair. It
/// occupies the wire and (for the taper repair) the in-flight ledger, but
/// never the retention store — so the store cap cannot see it.
struct WireSym {
    path: u32,
    resolve_at: f64,
    delivered: bool,
}

/// The per-channel emission ledger, reported with every ON run so the
/// attribution is measured (`docs/measurement-discipline.md` rule 14).
#[derive(Debug, Clone, Copy, Default)]
struct Ledger {
    /// Source admissions — the only arm the pacer's token debit runs on.
    src: u64,
    /// Taper corrections: wire + charge, no token.
    taper: u64,
    /// SACK-gap retransmits: wire only under `Engine`.
    retx: u64,
    /// NACK repair margin: wire only under `Engine`.
    margin: u64,
    /// `charge_in_flight(1)` calls.
    charges: u64,
    /// `release_in_flight(1)` calls.
    releases: u64,
    /// Releases that landed on a path already at `in_flight == 0` — the
    /// budget the saturating subtraction threw away.
    releases_wasted: u64,
    /// The pacer's debit count (source arm only).
    tokens: u64,
}

impl Ledger {
    /// Every symbol that reached the link.
    fn wire(&self) -> u64 {
        self.src + self.taper + self.retx + self.margin
    }
}

// ── The anchor-era axis ────────────────────────────────────────────────────
//
// The bench acks per symbol at the true delivery instant, so
// `CopaState::record_delivery`'s Δdelivered/Δt reads the truth; the legacy
// ack-interval sampler over-reads because acks arrive batched and the
// cumulative frontier jumps. That anchor is the store-cap Σ and, via
// `clamp_cwnd_with_anchor`, the cwnd floor, so an over-reading anchor props
// `available() > 0` and can keep fast symmetric cells out of the
// empty-`active_paths()` state. The era is a bench variable, two ways:
//
//   * `Overread(f)` — a pure scale on the sampler's input, swept.
//     `record_delivery` uses `count` only for Δdelivered, so `f·count` scales
//     every rate sample (and `max_bw`, `bdp_anchor()`, the anchor floor, the
//     store-cap Σ) by exactly `f`, with every cadence unchanged. `f = 1.0` is
//     the honest arm.
//   * `Cumulative { ack_period_s }` — derived from the bench's own ack
//     batching: a receiver reporting a cumulative frontier on a feedback
//     cadence. A GE drop stalls the frontier; the retransmit makes it jump,
//     and the sampler sees the jump over one interval. The realized
//     over-read (`anchor / true BtlBw·RTprop`) is measured, not assumed.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Feed {
    /// Per-symbol `on_ack(1)` at the true delivery instant — the shipped
    /// honest-anchor era.
    Honest,
    /// The legacy ack-interval era as a swept scale on the sampler input.
    Overread(f64),
    /// The legacy era derived from cumulative-frontier acks at a feedback
    /// cadence (seconds).
    Cumulative { ack_period_s: f64 },
    /// The measured wire ack stream, one `AckShape` per path of the cell.
    /// Nothing about the anchor is injected: `record_delivery` is fed one
    /// delivered symbol per ack at modelled arrival instants and the shipped
    /// 1 ms `elapsed` floor does the folding.
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
// The measured ack stream
// ═══════════════════════════════════════════════════════════════════════════
//
// The ack stream is transcribed from a VM measurement of the wire (the
// `ackdiag` instrument), not invented. Four inputs:
//
// (1) Delivered count per ack = 1 (no ack aggregation at any cell or path)
//     ⇒ `record_delivery(1)` once per delivered symbol.
// (2) Arrival spacing is heavy-tailed, per cell and per path ⇒ the gap
//     quantiles below, as a distribution.
// (3) The 1 ms `elapsed` floor is the sampler's clock ⇒ the bench advances the
//     MockClock to each ack's arrival instant and calls the real `on_ack(1)`;
//     the real `elapsed < 0.001` branch rejects. The realized rejection rate
//     is measured and scored, not asserted.
// (4) `xanchor` is not an input but the check: the loop must produce it.
//
// The one structural choice. The measurement gives the marginal gap
// distribution, not its correlation, and the marginal alone cannot produce the
// over-read: an i.i.d. renewal stream puts ~10 acks in any 1 ms window, so
// `Δdelivered/Δt` reads ×1. A ×8 sample needs ~74 consecutive sub-p50 gaps.
// The over-read lives in the stream's run structure, modelled as a
// work-conserving observer:
//
//     the sender observes acks one at a time, spaced at the measured p50 gap,
//     while it has un-observed acks; when it runs out it goes silent for a
//     draw from the measured upper tail, and the acks that arrive during the
//     silence are observed in the burst that follows.
//
// It has no knobs: an observer draining at spacing `s` has duty cycle
// `s/ḡ = q50`, so the silence fraction is pinned at the value that makes the
// model's marginal reproduce the measured p50, p90 and p99 (`u_c` below,
// solved). What the model predicts, and the bench scores, is what was measured
// but not fed in: the floor-rejection rate, the accepted-sample rate, the acks
// folded per sample, and `xanchor`.

/// One measured path's ack stream. The `(lo, hi)` pairs are per-window ranges
/// over the measurement's 12 report windows — its uncertainty, carried rather
/// than averaged away. Microseconds; `rate_lr` is symbols/s.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct AckShape {
    /// The measurement row this is.
    row: &'static str,
    /// The window's long-run delivered rate, i.e. the mean ack gap is
    /// `1e6/rate_lr` µs.
    rate_lr: f64,
    /// Gap p50, µs.
    p50: (f64, f64),
    /// Gap p90, µs.
    p90: (f64, f64),
    /// Gap p99, µs.
    p99: (f64, f64),
    // ── the checks: measured, never fed in ──
    /// Floor-rejected samples, % — what the 1 ms floor did on the wire.
    rej_pct: f64,
    /// Accepted samples/s.
    samples_s: f64,
    /// Median `xanchor` — the quantity the store-cap Σ and the cwnd anchor
    /// floor consume, and the one this bench must produce.
    xanchor: f64,
    /// `xanchor` min/max over the 12 windows.
    xanchor_range: (f64, f64),
    /// Measured RTprop — the anchor's own `min_rtt`, i.e. exactly the
    /// `min_rtt` `copa_bdp_anchor()` multiplies by. Seconds. With it the
    /// wire's anchor is reconstructible in symbols:
    /// `xanchor := copa_bdp_anchor()/(rate_lr·RTprop)` inverts to
    /// `anchor = xanchor · rate_lr · RTprop` ([`AckShape::anchor_sym`]), the
    /// term the store-cap Σ adds up.
    rtprop_s: f64,
}

impl AckShape {
    /// The wire's own `copa_bdp_anchor()` for this path, in symbols — the
    /// store-cap Σ's per-path term, reconstructed by inverting the definition
    /// of `xanchor`: three measured columns multiplied.
    ///
    /// This is not `rate_configured · RTT_configured · xanchor`: the realized
    /// `rate_lr` is 0.67–0.69× the cells' nominal symbol rates and RTprop is
    /// 0.64–1.05× their configured RTTs, so the two differ by 1.4–2.3× per
    /// path.
    fn anchor_sym(&self) -> f64 {
        self.xanchor * self.rate_lr * self.rtprop_s
    }
}

/// The Σ at which the shipped pooled law stops responding to the anchor.
///
/// `cap = clamp(gain·N·Σ, floor, N·knee)` is ceiling-pinned exactly when
/// `gain·N·Σ ≥ N·knee`, i.e. when `Σ ≥ knee/gain` — the `N` cancels, so the
/// pin threshold on the anchor sum is path-count free: 1024 symbols at
/// `knee = 2048`, `gain = 2`. Pinned by
/// `the_pin_threshold_on_sigma_is_knee_over_gain_and_is_path_count_free`.
const SIGMA_PIN: f64 = KNEE as f64 / GAIN;

/// `c2r100/p0` — single 100 MB, the reference cell.
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
    rtprop_s: 0.1004, // measured RTprop 100.4 ms
};

/// `c7/p0` — c2/c2 dual 200 MB, leg 0.
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
    rtprop_s: 0.0077, // measured RTprop 7.7 ms
};

/// `c7/p1` — the symmetric dual's other leg.
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
    rtprop_s: 0.0097, // measured RTprop 9.7 ms
};

/// `c8/p0` — the asymmetric dual's fast (c2) leg.
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
    rtprop_s: 0.0084, // measured RTprop 8.4 ms
};

/// `c8/p1` — the asymmetric dual's slow (c3) leg, which stalls to 18.2 ms
/// and is the only path the floor rejects less than 90 % of.
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
    rtprop_s: 0.0386, // measured RTprop 38.6 ms
};

/// The measured cells, path by path. `sc2` is the bench's single-fast cell and
/// the wire's single cell is `c2r100`; `c7`/`c8` map leg for leg.
static ACK_SC2: [AckShape; 1] = [ACK_C2R100_P0];
static ACK_C7: [AckShape; 2] = [ACK_C7_P0, ACK_C7_P1];
static ACK_C8: [AckShape; 2] = [ACK_C8_P0, ACK_C8_P1];

// ── Which brake binds on the wire, and how much queue it builds ────────────
//
// Three columns every L1 per-rep summary record carries, measured over 26–101
// reps per cell/arm on the same arms as the `[SF]` numbers:
//
//   * `occ_p50 / occcap_p50` — the store-cap DIAG print `win={store_len}/{cap}`.
//     `store_len` is the admission gate's own operand, post-SACK release, so
//     `occ/cap` is how close the gate is to closing.
//   * `wait_paused` — the share of sender-loop wakeups spent in the store-cap
//     backpressure poll, i.e. the share of time the store cap was the brake.
//   * `q_p50` — `rtt − rtp` off the per-path DIAG field: SRTT minus Copa's
//     windowed-min RTT on the same app-echo clock, the standing queue in ms;
//     `rtp_med` is the RTprop it stands on.
//
// They refute the premise that the bench's link builds a 2.4–5.7× RTprop
// standing queue the wire does not.
#[derive(Clone, Copy, Debug)]
struct WireBrake {
    cell: &'static str,
    arm: &'static str,
    /// Reps behind the `occ`/`q`/`paused` columns.
    reps: usize,
    /// `occ_p50` median over reps — the gate operand.
    occ: f64,
    /// `occcap_p50` median over reps.
    cap: f64,
    /// `wait_paused` mean over reps, in percent of sender-loop wakeups.
    paused_pct: f64,
    /// `q_p50` median over reps, in milliseconds.
    q_ms: f64,
    /// `q_p99` median over reps, in milliseconds.
    q99_ms: f64,
    /// `rtp_med` median over reps, in milliseconds.
    rtprop_ms: f64,
}

impl WireBrake {
    /// The standing queue in units of the path's own RTprop.
    fn queue_over_rtprop(&self) -> f64 {
        self.q_ms / self.rtprop_ms
    }
    fn occ_over_cap(&self) -> f64 {
        self.occ / self.cap
    }
}

/// The wire transcription: medians over reps (means for `wait_paused`, which
/// is already a percentage per rep) from every L1 summary record carrying
/// `occ_p50`.
const WIRE_BRAKE: &[WireBrake] = &[
    WireBrake {
        cell: "sc2", arm: "A", reps: 77,
        occ: 1009.0, cap: 1024.0, paused_pct: 40.4,
        q_ms: 91.0, q99_ms: 98.0, rtprop_ms: 13.0,
    },
    WireBrake {
        cell: "sc2", arm: "AU", reps: 29,
        occ: 1008.0, cap: 1024.0, paused_pct: 40.3,
        q_ms: 90.0, q99_ms: 97.0, rtprop_ms: 14.0,
    },
    WireBrake {
        cell: "c7", arm: "A", reps: 69,
        occ: 1254.0, cap: 4096.0, paused_pct: 0.0,
        q_ms: 76.0, q99_ms: 189.0, rtprop_ms: 11.0,
    },
    WireBrake {
        cell: "c7", arm: "AU", reps: 26,
        occ: 1326.0, cap: 4096.0, paused_pct: 0.0,
        q_ms: 83.0, q99_ms: 207.0, rtprop_ms: 10.5,
    },
    WireBrake {
        cell: "c8", arm: "A", reps: 57,
        occ: 2271.0, cap: 4096.0, paused_pct: 7.8,
        q_ms: 338.0, q99_ms: 707.0, rtprop_ms: 38.0,
    },
    WireBrake {
        cell: "c8", arm: "AU", reps: 26,
        occ: 2026.0, cap: 4096.0, paused_pct: 3.1,
        q_ms: 424.5, q99_ms: 1095.5, rtprop_ms: 40.5,
    },
];

fn wire_brake(cell: &str, arm: &str) -> &'static WireBrake {
    WIRE_BRAKE
        .iter()
        .find(|w| w.cell == cell && w.arm == arm)
        .expect("every cell/arm this bench scores must have a transcribed wire row")
}

// ── The pre-registration ───────────────────────────────────────────────────
//
// Question: with the ack stream measured and the in-flight accounting axis at
// `Acct::Engine`, does the bench's geography match the wire at both cells?
//
//   * the legacy (A) arm is a ≈4 % `[SF]` zero-fraction class at c7 and c8,
//   * the U-fold is keyed to c8 (≈7.5×) and null at c7.
//
// The same G1/G2 pair and statistic as the accounting axis:
//   G1 (level)       A-arm ensemble mean < 10 % and caught ≥ 50 % at both cells.
//   G2 (cell-keying) fold(c8) ≥ 3.0 and fold(c7) ≤ 2.0.
//
// Three validation targets gate the question — a loop whose ack stream lands
// nowhere near the wire's cannot be asked whether its geography matches:
//
//   V1 `xanchor`. The realized per-path `copa_bdp_anchor()/(rate·RTprop)` is
//      within ±30 % of the measured median at each path (5.94 / 9.80 / 10.11 /
//      13.29 / 13.82). The measured quantity's own spread across windows of
//      one run is 2.3×–3.8×, so ±30 % on the median is already tight.
//   V2 Floor rejection. The realized `elapsed < 1 ms` rejection rate is within
//      ±5 points of the measured rate (91.5 / 94.3 / 94.3 / 94.0 / 81.5 %).
//      It is a prediction of the model, not an input.
//   V3 The marginal. The realized ack-gap p50/p90/p99 lie inside the measured
//      per-window ranges. The observer reproduces these by construction, so
//      V3 is a wiring check.
//
// Verdict = (V1 ∧ V2 ∧ V3) gating (G1 ∧ G2). If validation fails, the
// geography question is not asked and the run reports which produced quantity
// diverged first; if it passes and G1 ∧ G2 fails, the loop is wrong where the
// measured inputs do not reach.
//
// Repairs-in-counters is reported, not scored: the wire's Σ`crecv`/`srcack`
// is 1.01–1.04 at c2r100/c7 and 1.21–1.34 at c8. Under `Acct::Engine` every
// wire symbol enters the bench's counters, so the same ratio is `wire()/src`.

/// V1 — the fraction by which the bench's realized `xanchor` may differ from
/// the measured per-path median.
const V1_XANCHOR_TOL: f64 = 0.30;
/// V2 — the points by which the realized floor-rejection rate may differ from
/// the measured per-path percentage.
const V2_REJECT_TOL_PTS: f64 = 5.0;

/// Every measured path the bench consumes, for the transcription pin and the
/// fidelity readouts.
const ACK_ALL: &[&AckShape] =
    &[&ACK_C2R100_P0, &ACK_C7_P0, &ACK_C7_P1, &ACK_C8_P0, &ACK_C8_P1];

/// The transcription pin: the numbers this bench runs on are the recorded
/// ones, in the recorded shape, and the checks were not turned into inputs.
/// It asserts the measurement's internal identities, so a typo in any row
/// fails here:
///
///   * each quantile range is ordered and p50 < p90 < p99;
///   * the median `xanchor` lies inside its own min/max;
///   * accepted-sample rate, rejection rate and `rate_lr` are one measurement
///     three ways (`rejection = 1 − samples_s/rate_lr`), so they must agree;
///   * every path's p50 gap is far below its mean gap `1e6/rate_lr` — the heavy
///     tail is the finding, and a row without it is a transcription error.
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
        // Rejection %, accepted samples/s and rate_lr are one measurement;
        // they must close on each other.
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
        // The stream is heavy-tailed: the median gap is a small fraction of
        // the mean gap. If this ever reads ≈1 the row is not the wire's.
        assert!(
            q50 < 0.25 * mean_gap_us,
            "{}: p50 gap {q50:.1} µs is not far below the mean gap {mean_gap_us:.1} µs — \
             the measured stream is heavy-tailed and this row is not",
            s.row
        );
        // And the tail is a tail: p99 is at least 5× the mean gap.
        assert!(
            q99 > 5.0 * mean_gap_us,
            "{}: p99 gap {q99:.1} µs against mean {mean_gap_us:.1} µs",
            s.row
        );
    }
        // The tolerances are pre-registered: pin them so loosening one takes
        // a diff that says so.
    assert_eq!(V1_XANCHOR_TOL, 0.30);
    assert_eq!(V2_REJECT_TOL_PTS, 5.0);
}

/// The midpoint of a measured range — the point estimate, with the range kept
/// so the model can back off inside it when the quantiles and the mean do not
/// close (see `AckGaps::new`).
fn mid((lo, hi): (f64, f64)) -> f64 {
    0.5 * (lo + hi)
}

// ── The gap distribution, built from the measured quantiles ────────────────
//
// `Q(u)` is the dimensionless gap quantile function (gap / the path's mean
// gap), interpolated through the measured points:
//
//     u ∈ [0, 0.5 ]    linear       0 → q50  (no quantile is reported below p50)
//     u ∈ [0.5, 0.9 ]  log-linear   q50 → q90
//     u ∈ [0.9, 0.99]  log-linear   q90 → q99
//     u ∈ [0.99, 1 ]   Pareto,      Q = q99·((1−u)/0.01)^(−1/α)
//
// α is solved, not chosen: the mean gap is `1/rate_lr`, so the top 1 % carries
// exactly the mass the body leaves, and `E[G | G > p99] = q99·α/(α−1)` fixes α.
//
// Two places where the measurement does not close on itself, both handled by
// backing off inside the measured ranges:
//
// (a) At c2r100 and c8's slow leg the quantile midpoints already imply a mean
//     gap above `1/rate_lr` (by ~10 % and ~0.5 %), leaving the tail negative
//     mass. `θ` — the position inside the p90/p99 per-window ranges — is
//     bisected down from the midpoint until `E[G|G>p99] ≥ 1.5·p99`. p50, the
//     tightest-measured, is never moved.
// (b) The unbounded Pareto tail generates silences of hundreds of ms at the
//     lightest α the mean allows. It is truncated at 18.2 ms, the largest
//     inter-ack gap measured anywhere. The observer is work-conserving, so a
//     lighter tail costs it silences, not acks.

/// The floor on the tail's own mass: the top 1 % of gaps average at least this
/// multiple of the measured p99 (α = 3 at 1.5 — the lightest tail the model
/// still calls a tail). It binds only where the quantile midpoints and the
/// mean gap do not close, and in the conservative direction: a lighter tail
/// means fewer long silences and a smaller predicted over-read.
const ACK_TAIL_R_MIN: f64 = 1.5;

/// The largest inter-ack gap measured at any cell or path (`c8/p1` p99 upper
/// range, 18229 µs). The silence draw is truncated here.
const ACK_GAP_MAX_S: f64 = 18_229e-6;

/// One path's measured ack-gap law, resolved against the bench path that
/// carries it.
#[derive(Clone, Copy, Debug)]
struct AckGaps {
    /// Dimensionless measured quantiles (gap / mean gap).
    q50: f64,
    q90: f64,
    q99: f64,
    /// The Pareto tail exponent, solved from the measured mean gap.
    alpha: f64,
    /// The silence threshold, solved from the drain/duty identity below.
    u_c: f64,
    /// Where inside the measured p90/p99 ranges the model had to sit for the
    /// measurement to close (0.5 = the midpoint).
    theta: f64,
    /// The nominal mean ack gap, seconds — `1/rate` — used only until the
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

        // The silence threshold, solved. A work-conserving observer that
        // drains at spacing `s = q50·ḡ` has, per cycle, a silence `S`, a drain
        // `D = S·q50/(1−q50)` and `S/(1−q50)` acks — so the silence fraction of
        // gaps is `φ = (1−q50)/E[S]`. For the model's marginal to reproduce the
        // measured one, the silences are `Q`'s upper tail: `φ = 1 − u_c` and
        // `E[S] = ∫_{u_c}^1 Q / (1−u_c)`. Together, one equation in one
        // unknown:
        //
        //     ∫_0^{u_c} Q(u) du = q50
        //
        // whose root is `u_c`.
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
    /// by the path's own measured mean gap, truncated at the largest gap ever
    /// measured.
    fn silence(&self, rng: &mut Rng, mean_gap_s: f64) -> f64 {
        let u = self.u_c + (1.0 - self.u_c) * rng.f64();
        (self.q(u) * mean_gap_s).min(ACK_GAP_MAX_S)
    }
}

/// Log-spaced buckets for the realized ack-gap distribution: 1 µs → 100 ms
/// over 5 decades. The bench measures its own marginal and scores it (V3)
/// rather than assuming it holds by construction.
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

/// One path's ack-observation state under the measured era.
struct AckObs {
    g: AckGaps,
    /// Delivered but not yet observed by the sender's rate sampler.
    backlog: u64,
    /// When the next ack is observed.
    next_obs: f64,
    rng: Rng,
    /// Arrival instants inside the last `REPORT_S` — the path's own measured
    /// mean ack gap, the unit the measured shape is expressed in.
    ///
    /// The shape is dimensionless and must be scaled by the realized rate,
    /// not the nominal one: the measured gaps are quoted against the window's
    /// own long-run delivered rate, an output of the wire. A path the
    /// scheduler under-fills (c8's slow leg runs at ~60 % of its link) has a
    /// wider mean gap, and scaling by `1/link_rate` would assert a denser ack
    /// stream than it produced. The window is the gauge's 2 s report cadence.
    arrivals: std::collections::VecDeque<f64>,
    // ── gauges: everything measured on the wire that this model predicts ──
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
    /// step if work remains, a silence if the sender has caught up.
    fn take(&mut self, t: f64) {
        self.backlog -= 1;
        let mg = self.mean_gap_s();
        self.next_obs = t
            + if self.backlog == 0 {
                self.g.silence(&mut self.rng, mg)
            } else {
                // The drain spacing is the measured p50 gap.
                self.g.q50 * mg
            };
        // Gauges. The accept/reject mirror is the scheduler's rule
        // (`elapsed < 0.001` ⇒ rejected) transcribed for instrumentation only;
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
    /// — the realized anchor over-read against the cell's own ground truth.
    anchor_ratio_sum: f64,
    anchor_ratio_n: u64,
    /// Σ over refresh ticks and paths of `cwnd` (the anchor floor's visible
    /// effect) — mean cwnd per path.
    cwnd_sum: f64,
    cwnd_n: u64,
    /// The accounting axis's per-channel ledger (all zero but `src`/`charges`/
    /// `releases`/`tokens` under `Acct::Off`).
    led: Ledger,
    /// Per path: Σ and n of `copa_bdp_anchor()/(rate·RTprop)` over refresh
    /// ticks. V1 is scored per path; a cell mean would hide the c8 legs.
    xa_sum: [f64; MAX_PATHS],
    xa_n: [u64; MAX_PATHS],
    /// Per path: what the measured ack observer did. All zero on every era
    /// but `Feed::Measured`.
    obs: [ObsStat; MAX_PATHS],
    /// Per path: Σ/n of `btlbw_sym_per_s()` and of `min_rtt()` over refresh
    /// ticks, plus the path's own delivered count.
    ///
    /// The measured `xanchor` is not this bench's `x`. It is
    /// `copa_bdp_anchor()/(rate_lr·RTprop)` with RTprop the anchor's own
    /// `min_rtt`, so the RTT cancels and `xanchor` is a pure rate over-read,
    /// `max_bw/rate_lr`. `overread()` divides by the configured
    /// `rate·rtprop` instead, so it also carries whatever standing queue the
    /// link built. Both are kept: `overread()` and `xanchor_lr()`.
    bw_sum: [f64; MAX_PATHS],
    bw_n: [u64; MAX_PATHS],
    mrtt_sum: [f64; MAX_PATHS],
    delivered_p: [u64; MAX_PATHS],
    /// Per path: the median of `max_bw / rate_lr` over the dyn-cap refresh
    /// ticks, `rate_lr` being the path's delivered rate over the preceding
    /// `REPORT_S` — the wire statistic, computed as the wire computes it. A
    /// whole-run divisor differs at a duty-cycled path: c8's slow leg runs at
    /// its link rate while it runs and idles between, so its 2 s windows read
    /// ~1.6× the run mean.
    xlr_med: [f64; MAX_PATHS],
    /// The coupling axis's produced quantities (zero under `Store::Unacked`):
    /// the mean frontier span `last_sent − cum_ack`, the mean released-mark
    /// count the SACK law subtracts from it, and the fraction of ticks whose
    /// receiver frontier sat behind a hole. All three are outputs.
    span_mean: f64,
    released_mean: f64,
    stall_frac: f64,
    /// At the refresh tick, the quantities the `available()` predicate is
    /// made of: the flow-control operand the gate read (`store_len`), the
    /// unacked count beside it, and Σ`in_flight` / Σ`cwnd` over live paths.
    store_len_mean: f64,
    unacked_mean: f64,
    infl_mean: f64,
    cwnd_live_mean: f64,
    /// Per path: Σ of `srtt()` over the same refresh ticks as `mrtt_sum`, so
    /// the bench produces the wire's own queue statistic `q_p50 = rtt − rtp`
    /// (SRTT minus Copa's windowed-min RTT, same app-echo clock):
    /// `srtt_sum/bw_n − mrtt_sum/bw_n`.
    srtt_sum: [f64; MAX_PATHS],
    /// The fraction of admission opportunities at which the store-cap gate
    /// was already closed (`store_len >= cap`) — the bench's analogue of the
    /// engine's `paused` wait-arm share. It answers "is the store cap the
    /// brake?" directly; the wire reads 0.0 % at c7.
    gate_closed: u64,
    gate_ticks: u64,
    /// The composed arm's late-stage brake (`Arm::Composed` only; 0/0 at every
    /// other arm): admission opportunities at which `cwnd_full` was already
    /// closed. The engagement gauge: an arm bit-identical to control must read
    /// as a null result (`brake_closed_pct` = 0 with the brake armed), never
    /// as a null effect (the brake never armed).
    brake_closed: u64,
    brake_ticks: u64,
    /// The source axis's produced quantities (all zero under `Src::Bulk`): the
    /// inner flow's mean congestion window in segments, its mean delivered
    /// latency in seconds (admission → cumulative frontier), its RTO count,
    /// and the split of admission opportunities between "the inner window was
    /// the binder" and "the store cap was".
    src_w_mean: f64,
    src_rtt_mean: f64,
    src_rto: u64,
    src_bound: u64,
    cap_bound: u64,
    src_opps: u64,
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
    /// The realized over-read on one path, on the bench's definition
    /// (`anchor / (configured rate·RTprop)`).
    fn overread_path(&self, pid: usize) -> f64 {
        self.xa_sum[pid] / self.xa_n[pid].max(1) as f64
    }
    /// The realized `xanchor` on one path on the wire's definition:
    /// `max_bw / rate_lr`, the windowed-max rate estimate over the path's own
    /// realized long-run delivered rate. The RTT divides out.
    fn xanchor_lr(&self, pid: usize) -> f64 {
        self.xlr_med[pid]
    }
    /// The same quantity on a whole-run divisor, kept beside it so the
    /// duty-cycle effect is visible.
    fn xanchor_runmean(&self, pid: usize) -> f64 {
        let bw = self.bw_sum[pid] / self.bw_n[pid].max(1) as f64;
        let lr = self.delivered_p[pid] as f64 / self.horizon_s;
        if lr > 0.0 { bw / lr } else { f64::NAN }
    }
    /// How much standing queue the bench's link built: mean `min_rtt` over the
    /// path's configured RTprop. On the wire this reads ≈1; anything else is
    /// a bench artifact and is reported rather than absorbed.
    fn rtt_inflation(&self, pid: usize, rtprop: f64) -> f64 {
        self.mrtt_sum[pid] / self.bw_n[pid].max(1) as f64 / rtprop
    }
    /// The wire's queue statistic, produced by the bench: `srtt − min_rtt` in
    /// milliseconds, per path (the L1 `q_p50` column).
    fn queue_ms(&self, pid: usize) -> f64 {
        let n = self.bw_n[pid].max(1) as f64;
        (self.srtt_sum[pid] - self.mrtt_sum[pid]) / n * 1e3
    }
    /// `min_rtt` in milliseconds, per path (the L1 `rtp_med` column).
    fn min_rtt_ms(&self, pid: usize) -> f64 {
        self.mrtt_sum[pid] / self.bw_n[pid].max(1) as f64 * 1e3
    }
    /// The bench's analogue of the engine's `paused` wait-arm share: how often
    /// the store-cap gate was already closed when admission was offered.
    fn gate_closed_pct(&self) -> f64 {
        self.gate_closed as f64 / self.gate_ticks.max(1) as f64 * 100.0
    }
    /// The composed arm's late-stage brake share. 0.0 at every other arm
    /// because the brake was never armed there — read it beside
    /// `brake_ticks > 0`, never alone.
    fn brake_closed_pct(&self) -> f64 {
        self.brake_closed as f64 / self.brake_ticks.max(1) as f64 * 100.0
    }
    fn mean_cwnd(&self) -> f64 {
        self.cwnd_sum / self.cwnd_n.max(1) as f64
    }
    /// The bench's `wait_tun` analogue: the share of admission opportunities at
    /// which the offered load was the binder (inner window full, store cap
    /// not). Zero by construction under `Src::Bulk`.
    fn src_bound_pct(&self) -> f64 {
        self.src_bound as f64 / self.src_opps.max(1) as f64 * 100.0
    }
    /// The bench's `wait_paused` analogue under the source axis: the share at
    /// which the store cap was the binder.
    fn cap_bound_pct(&self) -> f64 {
        self.cap_bound as f64 / self.src_opps.max(1) as f64 * 100.0
    }
    /// The inner flow's delivered latency in milliseconds — the user-visible
    /// cost of whatever the tunnel does.
    fn src_rtt_ms(&self) -> f64 {
        self.src_rtt_mean * 1e3
    }
}

/// The real reliable-source placement objective (`Scheduler::place_costs` via
/// `place_probs_with_temperature`) at T → 0, the strict-best-path limit.
/// Deterministic (the shipped `place_symbol` draws a uniform), same candidate
/// set (`p.active`, no availability filter), same cost.
fn place_min_cost(sched: &Scheduler) -> u32 {
    place_min_cost_of(sched, false, &[])
}

/// As `place_min_cost`, for the repair objective: `is_repair = true` with the
/// covered-path multiset, as both engine repair sites call
/// `sched.place_symbol(true, &covered)`. The ρ_fate diversity term pushes a
/// correction away from the paths that carried the window it covers, which
/// is why recovery traffic concentrates on the leg not carrying the source.
fn place_min_cost_of(sched: &Scheduler, is_repair: bool, covered: &[u32]) -> u32 {
    let mut cands = sched.place_probs_with_temperature(is_repair, covered, f64::MIN_POSITIVE);
        // Determinism: `Scheduler` holds its paths in a `HashMap`, whose
        // iteration order is randomised per process. At a symmetric cell the
        // two costs are bit-equal and the winner would be whatever the map
        // yielded last. Sorting by path id makes the tie-break lowest-id-wins
        // and the bench reproducible.
    cands.sort_by_key(|(pid, _)| *pid);
    let mut best: Option<(u32, f64)> = None;
    for (pid, w) in cands {
        if best.is_none_or(|(_, bw)| w > bw) {
            best = Some((pid, w));
        }
    }
    best.map(|(pid, _)| pid).unwrap_or(0)
}

/// Close the loop at the shipped honest-anchor era (`Feed::Honest`, a
/// per-symbol `on_ack(1)`).
fn simulate(paths: &[Spec], arm: Arm, horizon_s: f64) -> Run {
    simulate_era(paths, arm, Feed::Honest, horizon_s)
}

/// Close the loop. `paths` is the cell geometry; `arm` selects the path set /
/// pooled ceiling; `feed` selects the anchor era (what the ack-interval rate
/// sampler sees); `horizon_s` is simulated seconds.
fn simulate_era(paths: &[Spec], arm: Arm, feed: Feed, horizon_s: f64) -> Run {
    simulate_seeded(paths, arm, feed, horizon_s, 0)
}

/// As `simulate_era`, with the GE link seeds salted. The loop is bistable, so
/// a single run is a draw from a mode, not a measurement of one: claims are
/// scored over a seed ensemble and reported as a mode rate.
fn simulate_seeded(paths: &[Spec], arm: Arm, feed: Feed, horizon_s: f64, salt: u64) -> Run {
    simulate_acct(paths, arm, feed, horizon_s, salt, Acct::Off)
}

/// The engine's `active_paths().max_by(loss_rate)` pick — the estimator the
/// taper block reads for r\* — with the same determinism fix as
/// `place_min_cost`: `active_paths()` returns `HashMap` order, so `max_by`'s
/// last-wins tie-break is randomised per process, and losses tie at every cold
/// start and at the symmetric cell. Pinned by
/// `worst_loss_path_tie_is_broken_deterministically`. The engine has the same
/// tie and does not break it.
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
/// saturating subtraction threw it away.
fn rel(sched: &mut Scheduler, pid: u32, led: &mut Ledger) {
    if let Some(p) = sched.path_mut(pid) {
        if p.in_flight == 0 {
            led.releases_wasted += 1;
        }
        p.release_in_flight(1);
    }
    led.releases += 1;
}

/// As `simulate_seeded`, with the in-flight accounting axis. `Acct::Off` is
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
    simulate_full(paths, arm, feed, horizon_s, salt, acct, Store::Unacked)
}

/// As `simulate_acct`, with the coupling axis. `Store::Unacked` is
/// bit-identical to `simulate_acct` — every branch the axis adds is behind
/// `store_mode == Store::Span`, and it consumes no RNG.
#[allow(clippy::too_many_arguments)]
fn simulate_full(
    paths: &[Spec],
    arm: Arm,
    feed: Feed,
    horizon_s: f64,
    salt: u64,
    acct: Acct,
    store_mode: Store,
) -> Run {
    simulate_src(paths, arm, feed, horizon_s, salt, acct, store_mode, Src::Bulk)
}

/// As `simulate_full`, with the source axis. `Src::Bulk` is bit-identical to
/// `simulate_full` — every branch the axis adds is behind `src == Src::Reno`,
/// and it consumes no RNG (the inner flow is deterministic given the tunnel).
#[allow(clippy::too_many_arguments)]
fn simulate_src(
    paths: &[Spec],
    arm: Arm,
    feed: Feed,
    horizon_s: f64,
    salt: u64,
    acct: Acct,
    store_mode: Store,
    src: Src,
) -> Run {
    simulate_place(paths, arm, feed, horizon_s, salt, acct, store_mode, src, Place::Shipped)
}

/// The placement axis — what an unmeasured leg's SRTT is worth in
/// `Scheduler::place_costs` (`RWM_COLD_PLACE`; see
/// `scheduler::cold_place_active`). `Place::Shipped` is bit-identical to
/// `simulate_src`: the scheduler is pinned to the shipped price rather than
/// left to read the process env, so no arm can be perturbed by the
/// environment it runs in.
///
/// An axis rather than a member of `Arm`: `Arm` selects a store-cap law, this
/// selects a placement price in a different layer, and the two compose.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Place {
    /// The shipped price: `PathState::srtt()` on a leg with no sample, i.e.
    /// the 50-ms `DEFAULT_SRTT`-class constructor seed.
    Shipped,
    /// `RWM_COLD_PLACE`: the active set's fastest MEASURED srtt.
    ColdMeasured,
}

#[allow(clippy::too_many_arguments)]
fn simulate_place(
    paths: &[Spec],
    arm: Arm,
    feed: Feed,
    horizon_s: f64,
    salt: u64,
    acct: Acct,
    store_mode: Store,
    src: Src,
    place: Place,
) -> Run {
    // The inner flow's ack is the sender's own cumulative frontier, which only
    // the coupling axis maintains. A closed-loop source without it would
    // silently run an open loop, so it is refused.
    assert!(
        src == Src::Bulk || store_mode == Store::Span,
        "Src::Reno needs Store::Span: the inner flow's ack IS the sender's \
         cumulative frontier (`snd_frontier`), and nothing else advances it"
    );
    let tick = 0.000_25_f64; // 250 µs — 20 ticks per dyn-cap refresh
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    // Pin the placement axis explicitly — never inherit the process env, or
    // an `RWM_COLD_PLACE=1` in the shell silently moves every arm at once.
    sched.set_cold_place(place == Place::ColdMeasured);
    assert_eq!(
        sched.cold_place(),
        place == Place::ColdMeasured,
        "the placement axis did not take — the arm would be unfalsifiable"
    );
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
    assert!(
        np <= MAX_PATHS,
        "the per-path gauges are [_; MAX_PATHS = {MAX_PATHS}] arrays; widen MAX_PATHS \
         before running a {np}-path geometry"
    );

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
    let mut xa_sum = [0.0_f64; MAX_PATHS];
    let mut xa_n = [0u64; MAX_PATHS];
    let mut bw_sum = [0.0_f64; MAX_PATHS];
    let mut bw_n = [0u64; MAX_PATHS];
    let mut mrtt_sum = [0.0_f64; MAX_PATHS];
    let mut srtt_sum = [0.0_f64; MAX_PATHS];
    let mut delivered_p = [0u64; MAX_PATHS];
    // The `paused`-arm analogue: counted at the admission gate below, once per
    // tick, BEFORE any symbol is admitted.
    let mut gate_closed = 0u64;
    let mut brake_closed = 0u64;
    let mut brake_ticks = 0u64;
    let mut gate_ticks = 0u64;
    // Delivery instants inside the trailing `REPORT_S`, per path — the
    // denominator of the wire's `xanchor`, over the gauge's own report window
    // rather than the whole run.
    let mut deliv_win: Vec<std::collections::VecDeque<f64>> =
        (0..np).map(|_| std::collections::VecDeque::new()).collect();
    let mut xlr_s: [Vec<f64>; MAX_PATHS] = std::array::from_fn(|_| Vec::new());

    // `Feed::Overread` — per-path fractional carry, so a non-integer scale is
    // exact in the long run instead of rounded per call.
    let mut scale_carry = vec![0.0_f64; np];
    // `Feed::Cumulative` — the receiver's per-seq delivery flags, the carrying
    // path of each seq, the cumulative frontier, and the feedback clock.
    let mut next_seq: u64 = 0;
    let mut seq_done: Vec<bool> = Vec::new();
    let mut seq_owner: Vec<u32> = Vec::new();
    let mut frontier: u64 = 0;
    let mut next_feedback = 0.0_f64;

    // ── The coupling axis's state (`Store::Span` only) ──────────────────
    // Receiver side: `received_seqs` / `highest_delivered_seq` /
    // `highest_seen_seq` and the gap-report rate limit, all connection-wide
    // locals of the single receiver task.
    let mut recv_seen: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
    let mut recv_frontier: u64 = 0; // = highest_delivered_seq + 1
    let mut recv_highest: u64 = 0;
    let mut last_gap_ack_seen: u64 = 0;
    let mut last_gap_ack_s: f64 = -GAP_ACK_MIN_S; // `Instant::now() - GAP_ACK_MIN_INTERVAL`
    let mut last_advertised_ack: u64 = 0;
    let mut reports: Vec<Report> = Vec::new();
    // Sender side: `window_ack_seq` (folded with `fetch_max`) and the
    // `sack_released` mark set, held as the union of every landed report
    // pruned at the frontier (5), O(#ranges) to count.
    let mut snd_frontier: u64 = 0;
    let mut snd_marks: Vec<(u64, u64)> = Vec::new();
    // ── The source axis's state (`Src::Reno` only) ──────────────────────
    let mut reno = RenoSource::new();
    // Gauges: the span the model produces, and what the marks release.
    let mut span_sum = 0.0_f64;
    let mut span_n = 0u64;
    let mut rel_sum = 0.0_f64;
    let mut stall_ticks = 0u64;
    // The `available()` decomposition, sampled at the refresh tick.
    let mut sl_sum = 0.0_f64;
    let mut un_sum = 0.0_f64;
    let mut infl_sum = 0.0_f64;
    let mut cwl_sum = 0.0_f64;
    let mut dec_n = 0u64;

    // ── the accounting axis's state ─────────────────────────────────────
    // The shipped FEC rate controller at the resolved defaults: r* is whatever
    // the shipped law returns on the bench's own measured loss/RTT/throughput
    // — no repair rate is injected. `set_inner_feedback(0.0)` is
    // `config::resolve`'s default.
    let mut ctrl = FecRateController::new_with_toggles(
        TAIL_LOSS,
        MAX_OVERHEAD,
        ProtocolHint::Auto,
        FecBackend::RaptorQ,
        true,
        SYMBOL_SIZE,
    );
    ctrl.set_inner_feedback(0.0);
    // The three-term candidate's K tracker and δ-budget, both the engine's:
    // `percap_k` is the same `EchoRatioMin` map every honest cap uses, and
    // `b` is `delta_budget_b(hint)` at the bench's hint.
    let mut percap_k: std::collections::HashMap<u32, EchoRatioMin> =
        std::collections::HashMap::new();
    let tt_b = delta_budget_b(ProtocolHint::Auto);
    let mut led = Ledger::default();
    let mut wire: Vec<WireSym> = Vec::new();
    // `st.repair_debt` and the taper cache's r*.
    let mut repair_debt = 0.0_f64;
    let mut repair_rate = 0.0_f64;
    // Per-path ack counters, drained each tick into `record_batch` — the
    // engine's `expected_count` / `received_count`.
    let mut ack_expected = vec![0u32; np];
    let mut ack_received = vec![0u32; np];
    // The report task's throughput feed.
    let mut sent_since_report = vec![0u64; np];
    let mut next_report = REPORT_S;

    // ── The measured ack era's per-path observer ─────────────────────────
    // One per path, its law resolved against that path's own rate (the
    // measured shape is dimensionless), and its RNG kept apart from the
    // links' so `Feed::Measured` cannot perturb the GE realizations every
    // other era runs on.
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
    /// advances monotonically and lands exactly on each tick boundary — the
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
    // computed at most once per tick and reused by every correction emitted in
    // that tick — the engine refreshes the derived taper math at burst
    // granularity (`RWM_EMIT_BATCH`), and a 250 µs tick is finer than a burst.
    let mut covered_cache: Option<Vec<u32>>;
    let mut repair_path_cache: Option<u32>;
    for step in 1..=steps {
        let now = step as f64 * tick;
        covered_cache = None;
        repair_path_cache = None;

        if let Feed::Measured(_) = feed {
            // ── The measured ack stream, sub-tick ────────────────────────
            // `record_delivery` is called at each ack's own arrival instant
            // with `count = 1`, and the shipped 1 ms `elapsed` floor decides
            // which becomes a rate sample. Feeding the sampler on the 250 µs
            // tick would quantize `elapsed` and bypass that mechanism.
            //
            // Deliveries enter each path's observer at their link arrival
            // time; observations come out at the measured cadence, in time
            // order across paths, with the clock walked to each one.
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
                        // drecv = 1: the wire never batches acks.
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

        // ── The coupling axis: the receiver's frontier and its SACK clock ──
        if store_mode == Store::Span {
            let t_prev = (step - 1) as f64 * tick;
            // (a) Sender: apply every feedback message that has landed —
            //     `window_ack_seq.fetch_max` and the snapshot union (5). The
            //     mark set prunes on the cumulative twin, which is
            //     `released_count`'s `frontier` clamp.
            let mut i = 0;
            while i < reports.len() {
                if reports[i].arrive_at <= now {
                    let r = reports.swap_remove(i);
                    if r.frontier > snd_frontier {
                        snd_frontier = r.frontier;
                    }
                    union_marks(&mut snd_marks, &r.ranges, snd_frontier);
                } else {
                    i += 1;
                }
            }
            // (a2) The inner flow's ack clock. The sender's cumulative
            //      frontier is the only thing that retires an inner segment,
            //      and the RTT it measures is the tunnel's whole delivered
            //      latency — RTprop, standing queue and any recovery stall.
            //      No-ops under `Src::Bulk` (nothing was ever admitted into
            //      `reno.outstanding`).
            if src == Src::Reno {
                reno.on_frontier(snd_frontier, now);
                reno.check_rto(now);
            }
            // (b) Receiver: in-order delivery, in time order, with the
            //     gap-report clock evaluated per arrival (the engine evaluates
            //     `window_ack_emission` once per received data message). A
            //     symbol reaches the receiver half a round trip before its ack
            //     reaches the sender; the ack returns on the arm it arrived
            //     on, so the return leg is that path's RTprop/2.
            let mut recvs: Vec<(f64, u32, u64)> = store
                .iter()
                .filter_map(|s| {
                    let t = s.ack_at? - paths[s.path as usize].1 * 0.5;
                    (t > t_prev && t <= now).then_some((t, s.path, s.seq))
                })
                .collect();
            recvs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
            for (t_r, pid, seq) in recvs {
                recv_seen.insert(seq);
                if seq > recv_highest {
                    recv_highest = seq;
                }
                while recv_seen.contains(&recv_frontier) {
                    recv_frontier += 1;
                }
                // The receiver's ack-emission rule, in the same structure.
                let cumulative_advanced = recv_frontier > last_advertised_ack;
                let gap_report_due = recv_highest >= recv_frontier
                    && recv_highest > last_gap_ack_seen
                    && t_r - last_gap_ack_s >= GAP_ACK_MIN_S;
                if cumulative_advanced || gap_report_due {
                    if cumulative_advanced {
                        last_advertised_ack = recv_frontier;
                    }
                    last_gap_ack_seen = recv_highest;
                    last_gap_ack_s = t_r;
                    reports.push(Report {
                        arrive_at: t_r + paths[pid as usize].1 * 0.5,
                        frontier: recv_frontier,
                        ranges: sack_snapshot(&recv_seen, recv_frontier, recv_highest),
                    });
                }
            }
            // The receiver's own retention: seqs at or below the delivered
            // frontier are gone (it retains `> received_up_to`).
            if recv_frontier > 0 {
                recv_seen = recv_seen.split_off(&recv_frontier);
            }
        }

        // ── ack/delivery half + the recovery plane ───────────────────────
        // Acked symbols leave the store and release their path's budget.
        // Dropped ones are retransmitted once the time threshold (9/8·SRTT,
        // RFC 9002 kTimeThreshold) has passed, and are re-charged to their
        // new path.
        let acks: Vec<(u32, f64, u64)> = store
            .iter()
            .filter(|s| matches!(s.ack_at, Some(t) if t <= now))
            .map(|s| (s.path, s.rtt, s.seq))
            .collect();
        for (pid, rtt, seq) in &acks {
            if let Some(p) = sched.path_mut(*pid) {
                p.record_rtt_sample(Duration::from_secs_f64(*rtt));
            }
            // `release_in_flight(1)`, verbatim, plus the axis's counters.
            rel(&mut sched, *pid, &mut led);
            if acct.on() {
                ack_expected[*pid as usize] += 1;
                ack_received[*pid as usize] += 1;
                if let Some(p) = sched.path_mut(*pid) {
                    p.estimator.record_rtt(Duration::from_secs_f64(*rtt));
                }
            }
            if let Some(p) = sched.path_mut(*pid) {
                // The era axis: the transport-level accounting above is
                // identical in every era; only what the rate sampler is
                // shown differs.
                match feed {
                    Feed::Honest => p.on_ack(1),
                    Feed::Overread(f) => {
                        let acc = &mut scale_carry[*pid as usize];
                        *acc += f;
                        let k = acc.floor();
                        *acc -= k;
                        p.on_ack(k as u32)
                    }
                    // The cwnd-dynamics half runs on the same per-symbol
                    // cadence as the honest arm (`on_delivery_signal`); the
                    // rate sample is deferred to the frontier report.
                    Feed::Cumulative { .. } => p.on_delivery_signal(),
                    // The measured era drives `on_ack(1)` from the sub-tick
                    // observation loop at the top of the step.
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
            // `np`, not a hard-coded 2: with fixed-size `MAX_PATHS` arrays a
            // narrower guard would leave legs ≥ 2 reading 0 at `c7x4` whatever
            // placement did (see
            // `the_quad_spreads_across_all_four_legs_and_all_four_warm`).
            if (*pid as usize) < np {
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

        // ── The counter-delta release, for the flights that did not land ──
        // The feedback message carries `expected − received`, and the
        // sender releases that difference on the path the feedback arrived
        // on. So a lost symbol's budget comes back at the ack instant, not at
        // the retransmit — the store still holds it, but the in-flight ledger
        // has let it go.
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
            // The un-stored recovery flights resolve the same way. Every wire
            // symbol enters the receiver's per-batch counters, so each one
            // releases 1 on the path it flew — including the ones never
            // charged. `release_in_flight` saturates, so the excess is spent
            // against whatever else that path had outstanding.
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
            // Drain the per-path counters into the loss estimator (the
            // engine's `path.estimator.record_batch(expected, received)`),
            // which the repair rate, the NACK margin and the placement
            // objective's ρ term all read.
            for pid in 0..np {
                if ack_expected[pid] > 0 {
                    if let Some(p) = sched.path_mut(pid as u32) {
                        p.estimator.record_batch(ack_expected[pid], ack_received[pid]);
                    }
                    ack_expected[pid] = 0;
                    ack_received[pid] = 0;
                }
            }
            // The report task's local throughput feed: achieved send rate
            // over the report interval, bytes/s (its own `dt > 0.2` gate).
            // Feeds `t_sym` and the burst B/T term of the shipped r* law.
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

        // ── the receiver's cumulative frontier report (legacy era) ───────
        // A GE drop stalls the frontier; the retransmit's delivery releases
        // the whole accumulated run in one feedback message — the engine's
        // Δdelivered spike over one ack interval. The batch size is whatever
        // the bench's own loss and reordering produce.
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
                // The 1:1 discipline: the old flight's charge comes back
                // here. Under the axis it already came back at the counter
                // delta above.
                if let Some(p) = sched.path_mut(store[i].path) {
                    p.release_in_flight(1);
                }
                led.releases += 1;
            }
            let pid = place_min_cost(&sched);
            // First bypass channel: the SACK-gap retransmit hands its
            // `SymbolBatch` straight to `transport.send_symbols`, calling
            // `feed.on_sent` and `p.on_src_sent` but not `charge_in_flight`.
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

        // ── Second bypass channel: the NACK repair margin ─────────────────
        // `margin = ceil(retransmitted × current_loss)`, `current_loss` being
        // the max `estimator.loss_rate()` over `active_paths()`, placed by
        // `place_symbol(true, &covered)`, sent with neither a token nor a
        // charge. Both inputs are the bench's own realizations.
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
            // The shipped gauge predicate on the shipped inputs
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
            // The `available()` decomposition at the tick the gauge is
            // recorded on. `store_len` is what the axis says the gate reads;
            // `unacked` is carried beside it so the two are comparable.
            dec_n += 1;
            un_sum += store.len() as f64;
            sl_sum += match store_mode {
                Store::Unacked => store.len() as f64,
                Store::Span => (next_seq - snd_frontier) as f64
                    - released_count(&snd_marks, snd_frontier, next_seq) as f64,
            };
            for id in live.iter() {
                if let Some(p) = sched.path(*id) {
                    infl_sum += p.in_flight as f64;
                    cwl_sum += p.cwnd as f64;
                }
            }
            let n_live = live.len().max(1);
            let sum_over = |set: &[u32]| -> f64 {
                set.iter()
                    .filter_map(|id| sched.path(*id).and_then(|p| p.copa_bdp_anchor()))
                    .sum()
            };
            // The taper block's r* recompute, on the engine's own selection:
            // the max-loss estimator among `active_paths()`,
            // `sched.spare_capacity()` as the cap, and the retention store as
            // the encoder window. An empty active set ⇒ r = 0, as in the
            // engine. Refreshed on the taper cache's throttle.
            if acct.on() {
                let spare = sched.spare_capacity();
                repair_rate = worst_loss_path(&sched)
                    .and_then(|id| sched.path(id))
                    .map(|p| ctrl.compute_repair_rate_capped(&p.estimator, spare, store.len()))
                    .unwrap_or(0.0);
            }
            let bdp_live = sum_over(&live);
            let bdp_set = if arm == Arm::Legacy { sum_over(&act) } else { bdp_live };
            // The three-term candidate's inputs, collected as the engine
            // collects them: over `live_paths()`, off the same `PathState`
            // accessors, through the shipped `three_term_terms` and
            // `three_term_store_cap`. `rho` and `b` are the resolved defaults.
            let tt: Option<usize> = if matches!(arm, Arm::ThreeTermCell | Arm::Composed) {
                let slots: Vec<Option<ThreeTermPath>> = live
                    .iter()
                    .map(|id| {
                        sched.path(*id).map(|p| ThreeTermPath {
                            id: *id,
                            rate: p.btlbw_sym_per_s(),
                            srtt: p.srtt(),
                            rtprop: p.min_rtt(),
                            k_raw: p.k_raw(),
                        })
                    })
                    .collect();
                let terms = three_term_terms(&mut percap_k, &slots, (now * 1e6) as u64);
                three_term_store_cap(true, &terms, TT_RHO, tt_b, FLOOR).map(|(c, ..)| c)
            } else {
                None
            };
            cap = cap_for(arm, bdp_set, bdp_live, n_live, tt);
            cap_sum += cap as f64;
            // The realized anchor over-read and the cwnd it floors, per path,
            // against the cell's own ground truth (rate·RTprop).
            for pid in 0..np {
                if let Some(p) = sched.path(pid as u32) {
                    cwnd_sum += p.cwnd as f64;
                    cwnd_n += 1;
                    // The wire statistic's pair: the windowed-max rate
                    // estimate and the min-RTT the anchor multiplies it by.
                    if pid < np {
                        if let (Some(bw), Some(mr)) = (p.btlbw_sym_per_s(), p.min_rtt()) {
                            bw_sum[pid] += bw;
                            mrtt_sum[pid] += mr.as_secs_f64();
                            srtt_sum[pid] += p.srtt().as_secs_f64();
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
                            if pid < np {
                                xa_sum[pid] += a / truth[pid];
                                xa_n[pid] += 1;
                            }
                        }
                    }
                }
            }
        }

        // ── admission (bulk source: always data to send) ─────────────────
        // The gate, as the shipped plain-reliable sender writes it:
        // `reliable && (store_len >= effective_store_cap || cwnd_full)`, with
        // `cwnd_full == false` at the battery's arms (`RWM_INFL_CAP` = 0):
        // the store cap is the only brake.
        //
        // Placement does not gate it. `emit_source` picks with
        // `Scheduler::place_symbol(false, &[])`, whose `place_costs` filters on
        // `p.active` alone — no `available() > 0` filter on the reliable
        // source path. So `in_flight_i` may exceed `cwnd_i`, `available()`
        // stays 0, and `active_paths()` is a pure observable of the saturation
        // the store cap produced. That is the loop this bench closes.
        // The coupling axis reads the gate's operand from the release law
        // instead of the unacked count; under `Store::Unacked` it is inert.
        let mut store_len = match store_mode {
            Store::Unacked => store.len(),
            Store::Span => (next_seq - snd_frontier) as usize
                - released_count(&snd_marks, snd_frontier, next_seq),
        };
        if store_mode == Store::Span {
            span_sum += (next_seq - snd_frontier) as f64;
            rel_sum += released_count(&snd_marks, snd_frontier, next_seq) as f64;
            span_n += 1;
            if recv_highest >= recv_frontier {
                stall_ticks += 1;
            }
        }
        // The `paused` analogue, sampled before admission: was the store-cap
        // gate already closed at this tick? (The wire reads 0.0 % at c7-A.)
        gate_ticks += 1;
        if store_len >= cap {
            gate_closed += 1;
        }
        // The composed arm's late-stage brake, sampled at the same instant as
        // the store-cap gate so the two brakes are comparable per tick. Zero
        // by construction at every other arm (`Arm::brake_on`).
        if arm.brake_on() {
            brake_ticks += 1;
            if composed_brake_closed(&sched, arm) {
                brake_closed += 1;
            }
        }
        // ── The source axis: what the offered load allows ─────────────────
        // Under `Src::Bulk` this is `u64::MAX` and every branch below is
        // inert. Under `Src::Reno` it is the inner flow's congestion window,
        // and the sender is offered-load-bound exactly when the window is full
        // and the cap is not — the analogue of the wire's `wait_tun` /
        // `wait_paused` split, attributed once per tick before admission.
        let src_room: u64 = match src {
            Src::Bulk => u64::MAX,
            Src::Reno => reno.window().saturating_sub(next_seq - snd_frontier),
        };
        if src == Src::Reno {
            reno.w_sum += reno.w;
            reno.w_n += 1;
            if store_len >= cap {
                reno.cap_bound += 1;
            } else if src_room == 0 {
                reno.src_bound += 1;
            }
        }
        let mut src_left = src_room;
        // The admission gate with the composed arm's second disjunct live:
        // `reliable && (store_len >= cap || cwnd_full)`, `cwnd_full` false at
        // every other arm. Re-evaluated per symbol because each placement
        // moves `in_flight` on the path it chose — a late-stage,
        // per-placement brake.
        while store_len < cap && src_left > 0 && !composed_brake_closed(&sched, arm) {
            src_left -= 1;
            store_len += 1;
            let pid = place_min_cost(&sched);
            chg(&mut sched, pid, &mut led);
            let (a, rt, ok) = links[pid as usize].send_resolved(now);
            let seq = next_seq;
            next_seq += 1;
            if src == Src::Reno {
                reno.admit(seq, now);
            }
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
            // The pacer's debit: `if pol.cc_pace { st.src_tokens -= 1.0 }`
            // sits inside the source arm only. Every other channel increments
            // `led.wire()` without touching it;
            // `pacer_debit_bounds_only_the_source_arm_not_the_wire` bounds
            // the gap.
            led.tokens += 1;

            // ── Channel (a): the taper correction, token-free ─────────────
            // `st.repair_debt += repair_rate` per source symbol; while the
            // debt clears 1.0 a correction is sent on the ρ_fate repair
            // placement. It is charged to in_flight but not paced, and it is
            // not in the retention store, so the store cap cannot see it.
            // Guarded by the engine's `encoder.window_size() > 1`.
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

    let mut xlr_med = [f64::NAN; MAX_PATHS];
    for (p, v) in xlr_s.iter_mut().enumerate() {
        if !v.is_empty() {
            v.sort_by(f64::total_cmp);
            xlr_med[p] = v[v.len() / 2];
        }
    }
    let mut obs_stat = [ObsStat::default(); MAX_PATHS];
    for (i, o) in obs.iter().enumerate().take(MAX_PATHS) {
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
        span_mean: span_sum / span_n.max(1) as f64,
        released_mean: rel_sum / span_n.max(1) as f64,
        stall_frac: stall_ticks as f64 / span_n.max(1) as f64,
        store_len_mean: sl_sum / dec_n.max(1) as f64,
        unacked_mean: un_sum / dec_n.max(1) as f64,
        infl_mean: infl_sum / dec_n.max(1) as f64,
        cwnd_live_mean: cwl_sum / dec_n.max(1) as f64,
        srtt_sum,
        gate_closed,
        gate_ticks,
        brake_closed,
        brake_ticks,
        src_w_mean: reno.w_sum / reno.w_n.max(1) as f64,
        src_rtt_mean: reno.rtt_sum / reno.rtt_n.max(1) as f64,
        src_rto: reno.rto_events,
        src_bound: reno.src_bound,
        cap_bound: reno.cap_bound,
        src_opps: reno.w_n,
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

/// (1) The reproduction — the `[SF]` zero-fraction under U on/off, per cell.
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

/// (2) The axis sweep — which geometry axis drives the fold. Rate ratio and
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

/// (3) The anchor-era sweep, as a curve and not a point.
///
/// The legacy ack-interval anchor over-reads ×4.6–7.4 on the wire; the
/// shipped honest anchor reads ×1. The scale is swept through that band and
/// past it, so no conclusion can depend on picking one value.
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

/// The seed ensemble size. The loop is bistable, so the statistic that
/// resolves it is the mode rate over an ensemble, not one run's mean.
const SEEDS: u64 = 8;

/// The caught class, declared before the matrix is read: a run whose `[SF]`
/// zero-fraction is below 10 %. The wire's legacy arms sit in a ≈4 % class
/// and the bench's caught regime at 0.2–0.3 %; 10 % separates those from the
/// 40–100 % saturated mode with a wide margin. `min`/`max` are printed so the
/// cut can be re-drawn.
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

/// (4) The matrix: {c7, c8, c8r, c8t} × {A, AU, P} × {honest, over-read
/// band}, scored over the seed ensemble. The over-read column is shown at
/// both ends of the wire's measured band.
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

/// (5) The derived era — no injected number. A cumulative-frontier receiver on
/// a feedback cadence; the batch sizes, and hence the over-read, are whatever
/// the bench's own GE loss and retransmit timing produce. The realized
/// over-read is measured against `rate·RTprop` and compared with the wire's
/// 4.6–7.4 band.
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

// ── The accounting-axis matrix and its pre-registered verdict ─────────────
//
// The wire's geography:
//
//   * the legacy (A) arm sits in a ≈4 % `[SF]` zero-fraction class (3.7–7.4 %
//     at c8, both seeds, both anchor eras) and is in the same low class at c7;
//   * U raises c8 from ≈4 % to ≈30 % past 2σ on both seeds (≈7.5× fold) and
//     does not do so at c7.
//
// With the axis off the bench reproduces neither: A sits at 9.0 % at c7 and
// 40.9 % at c8, and the U-fold is 11.0× at c7 against 2.4× at c8 — keyed to
// the wrong cell.
//
// Hypothesis: the un-metered, slow-leg-concentrated recovery flow keys the
// collapse to c8 on the wire.
//
// Pass criteria, both required:
//
//   G1 (level). With `Acct::Engine`, the A arm's ensemble-mean zero-fraction
//      is below `CAUGHT_PCT` (10 %) at both c7 and c8, and its caught mode
//      rate is ≥ 50 % at both.
//   G2 (cell-keying). fold = mean(AU zero%) / mean(A zero%). Require
//      fold(c8) ≥ 3.0 and fold(c7) ≤ 2.0 — well inside the wire's ≈7.5×
//      against null.
//
// Verdict = G1 ∧ G2. c8r/c8t are reported for completeness; the verdict rests
// on the c7 + c8 contrast.
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

/// (6) The accounting-axis matrix — {c7, c8, c8r, c8t} × {A, AU} × {OFF,
/// TRAFFIC, ENGINE}, 8 seeds × 20 s, scored on the mode rate. The G1/G2
/// verdict is printed for c7 and c8 only.
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

/// (7) The pooled-ceiling candidate, re-run on every level of the accounting
/// axis.
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

/// Bounds the pacing divergence (CLAUDE.md: every documented model-vs-engine
/// divergence carries a test that bounds it). The token bucket is debited
/// inside the source arm alone, so the realized wire rate is `src·(1+r)` —
/// every repair, retransmit and margin symbol reaches the link having debited
/// nothing.
///
/// This asserts what the implementation does, by identity: the debit count
/// equals the source count exactly, and the wire count exceeds it by exactly
/// the three unpaced channels. The residual `wire/src − 1` is the realized
/// `r`, from the bench's own loss realizations through the shipped r\* law.
/// If repair is ever paced, this test fails loudly.
#[test]
fn pacer_debit_bounds_only_the_source_arm_not_the_wire() {
    let r = simulate_acct(&[C2, C3], Arm::Legacy, Feed::Honest, 6.0, 0, Acct::Engine);
    let l = r.led;
    // Measurement discipline rule 1: all three unpaced channels must have
    // fired, or this proves nothing.
    assert!(l.src > 10_000, "no source traffic: {}", l.src);
    assert!(l.taper > 0, "the taper repair channel never fired");
    assert!(l.retx > 0, "the SACK-gap retransmit channel never fired");
    assert!(l.margin > 0, "the NACK repair margin channel never fired");
    // The divergence, as an exact identity in both directions.
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
        "a token-exact wire would need wire == tokens; measured wire {} vs tokens {}",
        l.wire(),
        l.tokens
    );
    // The size of the divergence, bounded: the unpaced excess is the realized
    // repair overhead r, which the shipped law caps at `max_fec_overhead`
    // (0.5) per source symbol on the taper channel.
    let excess = (l.wire() - l.tokens) as f64 / l.src as f64;
    assert!(
        excess > 0.0 && excess < 1.0,
        "realized unpaced excess wire/src − 1 = {excess:.4}, outside (0, 1)"
    );
}

/// `Σ charges` does not count every wire symbol: it under-counts by exactly
/// the two bypass channels — the SACK-gap retransmit and the NACK repair
/// margin, each of which calls `transport.send_symbols` with no
/// `charge_in_flight`. The taper correction is charged, and that asymmetry is
/// asserted too. The ratio comes from the run's own channel counts.
#[test]
fn unmetered_recovery_flow_is_not_charged_to_in_flight() {
    let e = simulate_acct(&[C2, C3], Arm::Legacy, Feed::Honest, 6.0, 0, Acct::Engine);
    let t = simulate_acct(&[C2, C3], Arm::Legacy, Feed::Honest, 6.0, 0, Acct::Traffic);
    // Under the engine ledger the charge deficit is exactly the two bypass
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
    // The taper correction is on the other side of that line: charged.
    assert!(e.led.taper > 0, "the taper channel never fired");
    assert_eq!(
        e.led.charges,
        e.led.src + e.led.taper,
        "only source and taper corrections are charged under the engine ledger"
    );
    // The balanced counterfactual charges every wire symbol, which makes the
    // matrix comparison a test of the ledger and not of the traffic.
    assert_eq!(
        t.led.charges,
        t.led.wire(),
        "the TRAFFIC arm must charge every wire symbol (it is the balanced control)"
    );
}

/// Release is counter-delta driven (`expected − received` on the path the
/// feedback arrived on, over counters built from every symbol in the batch),
/// so it is not 1:1 with charge and the ledger does not balance by
/// construction.
///
/// Under the engine ledger releases strictly exceed charges, the excess is
/// bounded by the un-charged wire, and some of it is thrown away by
/// `release_in_flight`'s saturating subtraction. Under the balanced `Traffic`
/// control the same run conserves. This extends the
/// `ack_merge_counter_delta_*` invariants to the un-metered case.
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
        // The balanced control conserves exactly over the same traffic.
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

/// The metering axis moves which cell the U-fold keys to, onto the cell the
/// wire folds at. Regression bound at 3 seeds × 6 s (the 8-seed × 20 s
/// ensemble is the evidence), both directions asserted:
///
///   * with the ledger balanced (`Acct::Off`) the fold is larger at the
///     symmetric cell than at c8 — the wrong-cell keying;
///   * with the engine's ledger it is larger at c8 by > 3× and null at c7
///     (< 2×), the wire's own separation;
///   * the c8 A-arm's level falls by more than half, from a > 25 % class to a
///     < 15 % class.
///
/// Not asserted, because it did not hold: the c7 A-arm's level (G1 failed at
/// c7; the verdict is not reproduced).
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

    // Measurement discipline rule 1: the axis must have run.
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

    // The balanced ledger keys the fold to the wrong cell.
    let off_f7 = off_u7 / off_a7;
    let off_f8 = off_u8 / off_a8;
    assert!(
        off_f7 > off_f8,
        "the balanced ledger must fold harder at c7 than at c8 (the published \
         defect): c7 {off_f7:.2}x vs c8 {off_f8:.2}x"
    );

    // The engine's ledger keys it to c8 and nulls c7.
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

    // And the c8 A-arm's level, absolutely on both sides of the axis.
    assert!(off_a8 > 25.0, "the published c8 A arm must be the high class: {off_a8:.1}%");
    assert!(en_a8 < 15.0, "the engine c8 A arm must be the low class: {en_a8:.1}%");
    assert!(
        en_a8 < 0.5 * off_a8,
        "the c8 A arm must more than halve across the axis: {off_a8:.1}% → {en_a8:.1}%"
    );
}

/// It is the ledger, not the extra recovery traffic, that moves c8.
///
/// The `Traffic` level emits the same taper corrections and NACK margin
/// repairs onto the same placements, feeds the same estimators and consumes
/// the same wire — and leaves the c8 A arm's zero-fraction in its high class.
/// Only `Engine`, which adds the two un-charged channels and the counter-delta
/// release, moves it. This pin excludes the "more traffic" reading.
#[test]
fn the_ledger_not_the_recovery_traffic_moves_the_c8_zero_fraction() {
    let c8 = vec![C2, C3];
    let mean = |arm: Arm, acct: Acct| -> f64 {
        (0..3u64)
            .map(|s| simulate_acct(&c8, arm, Feed::Honest, 6.0, s, acct).zero_pct())
            .sum::<f64>()
            / 3.0
    };
    // The traffic is real and identical in both ON levels, else this proves
    // nothing (measurement discipline rule 1).
    let t = simulate_acct(&c8, Arm::Legacy, Feed::Honest, 6.0, 0, Acct::Traffic);
    assert!(t.led.taper > 0 && t.led.margin > 0, "the traffic level emitted no recovery");
    assert_eq!(t.led.charges, t.led.wire(), "the traffic level must balance");

    let off_a = mean(Arm::Legacy, Acct::Off);
    let tr_a = mean(Arm::Legacy, Acct::Traffic);
    let en_a = mean(Arm::Legacy, Acct::Engine);
    // The two levels move the arm in opposite directions: balanced recovery
    // traffic leaves c8 in (or pushes it further into) its high class, and
    // only the un-metered ledger brings it down to the wire's low class.
    // Asserted as absolute classes, not a ratio, because the levels are draws
    // from a bistable loop and only class membership is stable.
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

/// The axis's own reproducibility.
///
/// The repair channel's rate comes from one path's estimator — the max-loss
/// path among `active_paths()`. That returns `HashMap` iteration order and
/// `max_by` keeps the last maximum, so a tie would be a per-process coin flip.
/// Losses tie at every cold start and routinely at the symmetric cell;
/// without the sort in `worst_loss_path` the ON arm is unreproducible.
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

/// Measurement discipline rule 1 for the axis: every mechanism it transplants
/// executes, and the OFF level is the published bench untouched. Without this
/// the matrix could report "no effect" from an axis that never ran.
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
    // ON has all of them, and the shipped r* law produced a non-zero repair
    // rate from the bench's own measured loss — if r* read 0 the axis would
    // test two channels, not three.
    let on = simulate_acct(&[C2, C3], Arm::Legacy, Feed::Honest, 6.0, 0, Acct::Engine);
    assert!(on.led.taper > 0, "r* never cleared the repair debt");
    assert!(on.led.margin > 0, "the NACK margin never fired");
    assert!(on.led.retx > 0, "the retransmit channel never fired");
}

// ── The measured era's readouts ────────────────────────────────────────────

/// The cells the wire measured an ack stream at, and only those. `c8r`/`c8t`
/// are half-axis geometries the VM never ran, so there is no measured shape
/// for their paths and this bench will not invent one.
fn measured_cells() -> Vec<(&'static str, Vec<Spec>, &'static [AckShape])> {
    vec![
        ("sc2  single fast (c2r100)", vec![C2], &ACK_SC2[..]),
        ("c7   dual symmetric      ", vec![C2, C2], &ACK_C7[..]),
        ("c8   dual asym (r+RTT)   ", vec![C2, C3], &ACK_C8[..]),
    ]
}

// ── The coupling model's pre-registration (measurement discipline rule 11) ─
//
// Every number below is a wire number and every tolerance is stated before
// `sf_geography_on_the_coupling_model` ran. The wire (`[SF]` fractions pooled
// over every L1 rep carrying the gauge):
//
//     | cell | arm | zero% | short% |
//     |------|-----|-------|--------|
//     | c7   | A   |   0.3 |    3.7 |
//     | c7   | AU  |   1.2 |    6.3 |
//     | c8   | A   |   4.6 |   40.8 |
//     | c8   | AU  |  29.9 |   51.1 |
//
// All four criteria must hold; the test prints the verdict.
//
//   C1 (the c8 contrast): fold(c8) = mean(AU zero%) / mean(A zero%) >= 3.0
//      and mean(c8 AU zero%) >= 20.0 points.  [wire: fold 6.5x, AU 29.9%]
//   C2 (the c8 level): mean(c8 A zero%) <= 10.0 and caught >= 50 % of seeds.
//      [wire: 4.6 %, class 3.7–7.4]
//   C3 (c7 quiet): mean(c7 A zero%) <= 10.0 and fold(c7) <= 2.0.
//      [wire: 0.3 % and 4.0x on a 0.3 → 1.2 base, both arms in the noise]
//   C4 (the short-set fractions, the regime mixture the cap arithmetic is
//      expressed in): mean(c8 A short%) >= 25.0 and mean(c7 A short%) <= 15.0.
//      [wire: 40.8 % and 3.7 %]
//
// Verdict = C1 & C2 & C3 & C4. If it holds, the brake candidates are scored by
// `coupling_candidate_rule`. If not, the deliverable is which produced quantity
// diverges first along `span -> released -> store_len -> in_flight ->
// available()`, and no design conclusion is drawn.
const K1_FOLD_C8_MIN: f64 = 3.0;
const K1_AU_C8_MIN: f64 = 20.0;
const K2_LEVEL_C8_MAX: f64 = 10.0;
const K2_CAUGHT_MIN: f64 = 0.50;
const K3_LEVEL_C7_MAX: f64 = 10.0;
const K3_FOLD_C7_MAX: f64 = 2.0;
const K4_SHORT_C8_MIN: f64 = 25.0;
const K4_SHORT_C7_MAX: f64 = 15.0;

/// The candidate scoring rule, pre-stated; scored only if the verdict above
/// holds. Three candidates, one control:
///
///   (a) `Arm::PooledUnified` — the pooled-ceiling successor.
///   (b) `Arm::ThreeTermCell` — the three-term law as the dual-cell cap.
///   (c) `Arm::Unified` — U alone, the control that must reproduce the harm.
///
/// A candidate wins iff, against the shipped `Arm::Legacy` baseline:
///   * c8 stays stable at U's depth: mean cap within 10 % of the U arm's, and
///     c8 zero% <= the shipped arm's + 2.0 points;
///   * c1-class geometry keeps its throughput: goodput >= 0.98× the shipped
///     arm's at the c1-class cell;
///   * and the control (c) fails the first clause, or the bench has not
///     reproduced the harm it is scored against.
const CAND_CAP_TOL: f64 = 0.10;
const CAND_ZERO_TOL_PTS: f64 = 2.0;
const CAND_GP_MIN: f64 = 0.98;

// ── The coupling model's always-on pins ────────────────────────────────────

/// The release law's two pure helpers against the engine's definitions:
/// `sack_snapshot` is `received_sack_ranges` and `released_count` is the
/// cardinality the mark set contributes to `sack_release_outstanding` after
/// the cumulative prune.
#[test]
fn sack_snapshots_subsume_and_the_union_is_the_newest() {
    use std::collections::BTreeSet;
    // The engine's own unit fixture: delivered = 10, seen = 20,
    // received {11,12,15,18,19,20} ⇒ [(11,12),(15,15),(18,20)].
    let seen: BTreeSet<u64> = [11u64, 12, 15, 18, 19, 20].into_iter().collect();
    assert_eq!(
        sack_snapshot(&seen, 11, 20),
        vec![(11, 12), (15, 15), (18, 20)],
        "the snapshot must be received_sack_ranges' own encoding"
    );
    // Empty above the cumulative point ⇒ no ranges.
    assert!(sack_snapshot(&seen, 21, 20).is_empty());

    // The mark set only covers retained seqs: below the frontier and at or
    // above the sent edge are both excluded, which makes the count a subset
    // of `sent_store`.
    let r = sack_snapshot(&seen, 11, 20);
    assert_eq!(released_count(&r, 11, 21), 6, "all six retained marks count");
    assert_eq!(released_count(&r, 16, 21), 3, "the prune drops 11,12,15");
    assert_eq!(released_count(&r, 11, 19), 4, "the sent edge clips 19,20");
    assert_eq!(released_count(&r, 25, 30), 0, "a passed frontier releases nothing");

    // Subsumption (5): a later snapshot over a superset of arrivals covers
    // every seq an earlier one did, above the later cumulative point.
    let later: BTreeSet<u64> = [11u64, 12, 13, 14, 15, 18, 19, 20, 21].into_iter().collect();
    let a = sack_snapshot(&seen, 11, 20);
    let b = sack_snapshot(&later, 11, 21);
    for &(lo, hi) in &a {
        for s in lo..=hi {
            assert!(
                released_count(&b, s, s + 1) == 1,
                "seq {s} released by the earlier snapshot must be in the later one"
            );
        }
    }
    assert!(
        released_count(&b, 11, 22) > released_count(&a, 11, 22),
        "the later snapshot is strictly larger here, so the union IS the newest"
    );
    let mut u = Vec::new();
    union_marks(&mut u, &a, 11);
    union_marks(&mut u, &b, 11);
    assert_eq!(u, b, "below the cap the union of snapshots is the newest");

    // Past the cap (plan 2a) the snapshot is an honest PREFIX: with
    // MAX_SACK_RANGES + 5 isolated received seqs above a hole, only the first
    // MAX_SACK_RANGES are claimed and no missing seq is.
    let n = MAX_SACK_RANGES as u64 + 5;
    let many: BTreeSet<u64> = (0..n).map(|i| 12 + 4 * i).collect();
    let top = 12 + 4 * (n - 1);
    let capped = sack_snapshot(&many, 11, top);
    assert_eq!(capped.len(), MAX_SACK_RANGES);
    for &(lo, hi) in &capped {
        assert!((lo..=hi).all(|s| many.contains(&s)), "({lo},{hi}) claims a missing seq");
    }
    // A later isolated arrival low down (14) adds a run and pushes the
    // earlier prefix's last run past the cap: the later prefix does NOT
    // subsume the earlier one, and the union keeps what the earlier prefix
    // proved received.
    let mut more = many.clone();
    more.insert(14);
    let later_capped = sack_snapshot(&more, 11, top);
    let mut u = Vec::new();
    union_marks(&mut u, &capped, 11);
    union_marks(&mut u, &later_capped, 11);
    let last = *capped.last().unwrap();
    assert!(later_capped.iter().all(|&(lo, hi)| !(lo <= last.0 && last.0 <= hi)));
    assert_eq!(released_count(&u, 11, top + 1), MAX_SACK_RANGES + 1);
    for &(lo, hi) in &u {
        assert!((lo..=hi).all(|s| more.contains(&s)), "union claims a missing seq");
    }
}

/// Measurement discipline rule 1: the axis executes, `Store::Unacked` is the
/// published bench bit-for-bit, and `Store::Span` produces the three
/// quantities the model exists to produce.
#[test]
fn coupling_axis_executes_and_unacked_is_the_published_bench() {
    let geom = [C2, C3];
    let feed = Feed::Measured(&ACK_C8[..]);
    let base = simulate_acct(&geom, Arm::Legacy, feed, 4.0, 0, Acct::Engine);
    let off = simulate_full(&geom, Arm::Legacy, feed, 4.0, 0, Acct::Engine, Store::Unacked);
    assert_eq!(base.zero, off.zero, "Store::Unacked must be bit-identical");
    assert_eq!(base.short, off.short);
    assert_eq!(base.delivered, off.delivered);
    assert_eq!(base.retx, off.retx);
    assert_eq!(base.led.wire(), off.led.wire());
    assert_eq!(off.span_mean, 0.0, "the gauges are inert on the published arm");

    let on = simulate_full(&geom, Arm::Legacy, feed, 4.0, 0, Acct::Engine, Store::Span);
    assert!(on.span_mean > 0.0, "the frontier span must be produced");
    assert!(on.released_mean > 0.0, "the SACK marks must actually arrive");
    assert!(
        on.span_mean > on.released_mean,
        "span {} must exceed the marks {} — the difference IS store_len",
        on.span_mean,
        on.released_mean
    );
    assert!(
        on.stall_frac > 0.0,
        "the receiver frontier must sit behind a hole at least sometimes, or \
         the 2 ms gap-report clock (net/mod.rs:213) never binds and the model \
         is untested"
    );
    assert!(on.delivered > 0 && on.retx > 0, "the loop must still run");
}

/// The model's cell-keyed claim, bounded: the frontier span is what a skewed
/// cell parks and a symmetric one does not. The c8 legs differ by 4.6× in
/// RTprop and the c7 legs do not, so the skewed cell's frontier stalls at
/// least as often.
#[test]
fn the_frontier_span_is_parked_at_the_skewed_cell_and_not_at_the_symmetric_one() {
    let sym = simulate_full(&[C2, C2], Arm::Legacy, Feed::Measured(&ACK_C7[..]), 4.0, 0, Acct::Engine, Store::Span);
    let asym = simulate_full(&[C2, C3], Arm::Legacy, Feed::Measured(&ACK_C8[..]), 4.0, 0, Acct::Engine, Store::Span);
    assert!(sym.span_mean > 0.0 && asym.span_mean > 0.0);
    // The fraction of the span the release law has not yet uncounted.
    let held_sym = 1.0 - sym.released_mean / sym.span_mean;
    let held_asym = 1.0 - asym.released_mean / asym.span_mean;
    assert!(
        (0.0..=1.0).contains(&held_sym) && (0.0..=1.0).contains(&held_asym),
        "the marks are a subset of the span by construction: {held_sym} {held_asym}"
    );
    assert!(
        asym.stall_frac >= sym.stall_frac,
        "the skewed cell's in-order frontier cannot stall LESS often than the \
         symmetric cell's: c8 {:.3} vs c7 {:.3}",
        asym.stall_frac,
        sym.stall_frac
    );
}

/// The three-term candidate is the shipped law and introduces no constant:
/// same function, same resolved arguments, and the span term is identically
/// zero at N = 1 by arithmetic (no path-count predicate).
#[test]
fn the_three_term_cell_cap_is_the_shipped_law_and_introduces_no_constant() {
    let tt = |rate: f64, rtp_ms: f64, k: f64| ThreeTermTerm { rate, rtprop_s: rtp_ms / 1e3, k };
    let b = delta_budget_b(ProtocolHint::Auto);
    // N = 1: the span term vanishes, and it does so without a branch.
    let (_, _, _, sp1) =
        three_term_store_cap(true, &[Some(tt(10_400.0, 8.0, 1.5))], TT_RHO, b, FLOOR).unwrap();
    assert_eq!(sp1, 0.0, "one path has zero RTprop spread");
    // The bench's c8 geometry: the span term is 2·rate_fast·skew with
    // skew = (60−8)/2 ms, i.e. rate_fast × the round-trip difference.
    let c8 = [Some(tt(C2.0, C2.1 * 1e3, 1.5)), Some(tt(C3.0, C3.1 * 1e3, 1.5))];
    let (_, _, _, sp2) = three_term_store_cap(true, &c8, TT_RHO, b, FLOOR).unwrap();
    let want = C2.0 * (C3.1 - C2.1);
    assert!(
        (sp2 - want).abs() < 1e-6,
        "term 3 must be rate_fast × (RTprop_max − RTprop_min): {sp2} vs {want}"
    );
    // Cold ⇒ None ⇒ the arm falls back to the configured chain verbatim,
    // which is what `cap_for` does.
    assert!(three_term_store_cap(true, &[None, Some(tt(C2.0, 8.0, 1.5))], TT_RHO, b, FLOOR).is_none());
    assert_eq!(
        cap_for(Arm::ThreeTermCell, 0.0, 900.0, 2, None),
        shipped_chain(900.0, 2),
        "a cold three-term tick must read the shipped chain, not a substitute"
    );
}

/// The seed ensemble over the coupling axis, carrying the short-set fraction
/// (C4) and the model's produced span/release gauges beside the `[SF]`
/// zero-fraction.
struct CoupEns {
    zero: Vec<f64>,
    short: Vec<f64>,
    gp: Vec<f64>,
    cap: Vec<f64>,
    span: Vec<f64>,
    rel: Vec<f64>,
    stall: Vec<f64>,
}

impl CoupEns {
    fn run(geom: &[Spec], arm: Arm, feed: Feed, acct: Acct, sm: Store) -> Self {
        let mut e = CoupEns {
            zero: vec![], short: vec![], gp: vec![], cap: vec![],
            span: vec![], rel: vec![], stall: vec![],
        };
        for s in 0..SEEDS {
            let r = simulate_full(geom, arm, feed, 20.0, s, acct, sm);
            e.zero.push(r.zero_pct());
            e.short.push(r.short_pct());
            e.gp.push(r.goodput_sym_s());
            e.cap.push(r.mean_cap);
            e.span.push(r.span_mean);
            e.rel.push(r.released_mean);
            e.stall.push(r.stall_frac * 100.0);
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
}

/// (11) The coupling model — the pre-registered C1..C4 verdict, and the
/// candidate scoring rule it unlocks.
#[test]
#[ignore = "component bench; run with --ignored --nocapture"]
fn sf_geography_on_the_coupling_model() {
    println!("\n=== THE COUPLING MODEL ({SEEDS} seeds x 20 s) ===");
    println!(
        "era = the wire's measured ack stream; metering = ENGINE (un-metered); \
         store = UNACKED (published) vs SPAN (frontier)\n"
    );
    println!(
        "{:<26} {:<22} {:<20} {:>7} {:>14} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>8}",
        "cell", "arm", "store", "zero%", "[lo..hi]", "caught", "short%", "cap",
        "span", "rel", "stall%", "goodput"
    );
    // (cell, A zero, A caught, A short, AU zero, AU short)
    let mut rows: Vec<(&str, f64, f64, f64, f64, f64)> = Vec::new();
    // (cell, arm, zero, cap, goodput) for the candidate rule.
    let mut cand: Vec<(&str, Arm, f64, f64, f64)> = Vec::new();
    for (name, geom, shapes) in measured_cells() {
        let feed = Feed::Measured(shapes);
        let (mut az, mut ac, mut as_, mut uz, mut us) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for sm in [Store::Unacked, Store::Span] {
            for arm in [Arm::Legacy, Arm::Unified, Arm::PooledUnified, Arm::ThreeTermCell] {
                let e = CoupEns::run(&geom, arm, feed, Acct::Engine, sm);
                let (z, sh) = (AcctEns::mean(&e.zero), AcctEns::mean(&e.short));
                if sm == Store::Span {
                    cand.push((name, arm, z, AcctEns::mean(&e.cap), AcctEns::mean(&e.gp)));
                    match arm {
                        Arm::Legacy => {
                            az = z;
                            ac = e.caught();
                            as_ = sh;
                        }
                        Arm::Unified => {
                            uz = z;
                            us = sh;
                        }
                        _ => {}
                    }
                }
                println!(
                    "{:<26} {:<22} {:<20} {:>6.1}% {:>14} {:>6.0}% {:>6.1}% {:>7.0} \
                     {:>7.0} {:>7.0} {:>6.1}% {:>8.0}",
                    name, arm.label(), sm.label(), z,
                    format!("[{:.1}..{:.1}]", e.lo(), e.hi()),
                    e.caught() * 100.0, sh,
                    AcctEns::mean(&e.cap), AcctEns::mean(&e.span),
                    AcctEns::mean(&e.rel), AcctEns::mean(&e.stall),
                    AcctEns::mean(&e.gp)
                );
            }
            println!();
        }
        rows.push((name, az, ac, as_, uz, us));
    }

    println!("--- THE PRE-REGISTERED VERDICT (C1 contrast, C2 level, C3 c7-quiet, C4 short set) ---");
    println!("wire: c7 A 0.3/3.7  c7 AU 1.2/6.3  c8 A 4.6/40.8  c8 AU 29.9/51.1  (zero%/short%)\n");
    let (mut c1, mut c2, mut c3, mut c4) = (true, true, true, true);
    for (name, az, ac, as_, uz, us) in &rows {
        let key = name.trim();
        let fold = if *az > 0.0 { uz / az } else { f64::INFINITY };
        println!(
            "{name}  A {az:.1}%/{as_:.1}%  AU {uz:.1}%/{us:.1}%  caught {:.0}%  fold {fold:.1}x",
            ac * 100.0
        );
        if key.starts_with("c8") {
            if fold < K1_FOLD_C8_MIN || *uz < K1_AU_C8_MIN {
                c1 = false;
            }
            if *az > K2_LEVEL_C8_MAX || *ac < K2_CAUGHT_MIN {
                c2 = false;
            }
            if *as_ < K4_SHORT_C8_MIN {
                c4 = false;
            }
        }
        if key.starts_with("c7") {
            if *az > K3_LEVEL_C7_MAX || fold > K3_FOLD_C7_MAX {
                c3 = false;
            }
            if *as_ > K4_SHORT_C7_MAX {
                c4 = false;
            }
        }
    }
    let ok = c1 && c2 && c3 && c4;
    let f = |b: bool| if b { "PASS" } else { "FAIL" };
    println!(
        "\nC1 {}  C2 {}  C3 {}  C4 {}  ==> THE COUPLING {}",
        f(c1), f(c2), f(c3), f(c4),
        if ok { "MODEL VALIDATES; candidates scored below" } else { "MODEL DOES NOT VALIDATE" }
    );

    println!("\n--- THE CANDIDATES (pre-stated rule; scored only on a validating model) ---");
    let get = |cell: &str, arm: Arm| -> Option<(f64, f64, f64)> {
        cand.iter()
            .find(|(n, a, ..)| n.trim().starts_with(cell) && *a == arm)
            .map(|(_, _, z, c, g)| (*z, *c, *g))
    };
    for arm in [Arm::PooledUnified, Arm::ThreeTermCell, Arm::Unified] {
        let (Some((z8, c8cap, _)), Some((zb, _, _)), Some((_, ucap, _)), Some((g1, _, _))) = (
            get("c8", arm),
            get("c8", Arm::Legacy),
            get("c8", Arm::Unified),
            get("sc2", Arm::Legacy),
        ) else {
            continue;
        };
        let g_arm = get("sc2", arm).map(|(_, _, g)| g).unwrap_or(0.0);
        let gp_base = get("sc2", Arm::Legacy).map(|(_, _, g)| g).unwrap_or(1.0);
        let _ = g1;
        let depth_ok = (c8cap - ucap).abs() <= CAND_CAP_TOL * ucap;
        let harm_ok = z8 <= zb + CAND_ZERO_TOL_PTS;
        let c1_ok = g_arm >= CAND_GP_MIN * gp_base;
        println!(
            "{:<22} c8 zero {z8:.1}% (shipped {zb:.1}%, +{:.1} pts)  cap {c8cap:.0} vs U {ucap:.0} \
             ({:+.0}%)  c1-class gp {:.3}x  ==> depth {} harm {} c1 {}",
            arm.label(), z8 - zb, (c8cap / ucap - 1.0) * 100.0, g_arm / gp_base,
            f(depth_ok), f(harm_ok), f(c1_ok)
        );
    }
    if !ok {
        println!(
            "\nThe model does not validate, so NO candidate conclusion is drawn from the rows \
             above; they are data. The deliverable is the first divergence along \
             span -> released -> store_len -> in_flight -> available()."
        );
    }
}

/// (12) Which produced quantity diverges first, walked along
/// `span → released → store_len → in_flight → available()` with every column
/// a produced number.
#[test]
#[ignore = "component bench; run with --ignored --nocapture"]
fn the_coupling_chain_walked_quantity_by_quantity() {
    println!("\n=== span -> released -> store_len -> in_flight -> available() ===");
    println!(
        "every column is PRODUCED. store_len is the operand the admission gate read;\n\
         unacked is the published bench's operand, carried beside it at the same tick.\n"
    );
    println!(
        "{:<26} {:<22} {:<20} {:>8} {:>8} {:>10} {:>9} {:>9} {:>9} {:>7} {:>7}",
        "cell", "arm", "store", "span", "rel", "store_len", "unacked", "S in_fl",
        "S cwnd", "zero%", "short%"
    );
    for (name, geom, shapes) in measured_cells() {
        let feed = Feed::Measured(shapes);
        for sm in [Store::Unacked, Store::Span] {
            for arm in [Arm::Legacy, Arm::Unified] {
                let (mut sp, mut re, mut sl, mut un, mut inf, mut cw, mut z, mut sh) =
                    (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
                for s in 0..SEEDS {
                    let r = simulate_full(&geom, arm, feed, 20.0, s, Acct::Engine, sm);
                    sp += r.span_mean;
                    re += r.released_mean;
                    sl += r.store_len_mean;
                    un += r.unacked_mean;
                    inf += r.infl_mean;
                    cw += r.cwnd_live_mean;
                    z += r.zero_pct();
                    sh += r.short_pct();
                }
                let n = SEEDS as f64;
                println!(
                    "{:<26} {:<22} {:<20} {:>8.0} {:>8.0} {:>10.0} {:>9.0} {:>9.0} \
                     {:>9.0} {:>6.1}% {:>6.1}%",
                    name, arm.label(), sm.label(),
                    sp / n, re / n, sl / n, un / n, inf / n, cw / n, z / n, sh / n
                );
            }
        }
        println!();
    }
    println!(
        "READ: the release law uncounts exactly what the frontier retains, so\n\
         `store_len = span - released` lands back on the UNACKED count up to the\n\
         2 ms gap-report lag; the admission loop then pins it at `cap` in BOTH\n\
         store models, and S in_flight follows the cap in both. The store law is\n\
         therefore an identity at the gate, not a new coupling."
    );
}

/// The frontier-span store does not change what the admission gate sees: the
/// SACK release law uncounts precisely the delivered part of the span, so
/// `store_len` returns to the unacked count up to the gap-report lag. Asserted
/// at the asymmetric cell, where the span is largest (3.5× the cap), so a
/// change to the release clock, `GAP_ACK_MIN_INTERVAL` or placement re-scores
/// this row.
#[test]
fn the_frontier_span_returns_to_the_unacked_count_through_the_release_law() {
    let geom = [C2, C3];
    let feed = Feed::Measured(&ACK_C8[..]);
    let r = simulate_full(&geom, Arm::Unified, feed, 8.0, 0, Acct::Engine, Store::Span);
    // The span is much larger than the gate's operand: the un-SACKed frontier
    // span the fast path parks (paper §6.3).
    assert!(
        r.span_mean > 2.0 * r.store_len_mean,
        "the span must dwarf the flow-control operand: span {:.0} vs store_len {:.0}",
        r.span_mean,
        r.store_len_mean
    );
    // …and yet the operand is the unacked count again, within a few percent —
    // the release law's job.
    let rel_err = (r.store_len_mean - r.unacked_mean).abs() / r.unacked_mean.max(1.0);
    assert!(
        rel_err < 0.10,
        "store_len must return to the unacked count through the release law: \
         store_len {:.0} vs unacked {:.0} ({:.1}% apart)",
        r.store_len_mean,
        r.unacked_mean,
        rel_err * 100.0
    );
    // The gate pins the operand at the cap, which is why extra pool depth
    // arrives as the same extra in-flight and no more.
    assert!(
        (r.store_len_mean - r.mean_cap).abs() < 0.15 * r.mean_cap,
        "the bulk source pins store_len at the cap: {:.0} vs cap {:.0}",
        r.store_len_mean,
        r.mean_cap
    );
}

/// The first divergence is not the store law, it is Σ`cwnd`.
///
/// `available() = cwnd − in_flight`, and Σ`in_flight` tracks the cap in both
/// store models, so what decides whether `active_paths()` empties is the other
/// operand. The bench's Σ`cwnd` over live paths at the duals is a large
/// multiple of the wire's measured Σ-anchor (the cwnd floor), so the bench's
/// `available()` has structural headroom the wire's does not. The owner is
/// the bench's link artifact: 2.4–5.7× RTprop of standing queue at every fast
/// path, which lands in `min_rtt`, hence in the anchor and the cwnd floor.
/// Bounded so a fix to the link model re-scores this row.
#[test]
fn the_benchs_live_cwnd_is_a_multiple_of_the_wires_measured_anchor_at_both_duals() {
    for (cell, geom, shapes) in [
        ("c7", vec![C2, C2], &ACK_C7[..]),
        ("c8", vec![C2, C3], &ACK_C8[..]),
    ] {
        let sigma_wire: f64 = shapes.iter().map(|s| s.anchor_sym()).sum();
            // The same 20 s horizon as the readout: Copa is still ramping at
            // 8 s (c7 reads Σ cwnd 1100 there, below the wire's anchor sum),
            // so a shorter horizon would assert the warm-up, not the steady
            // state.
        let r = simulate_full(
            &geom,
            Arm::Unified,
            Feed::Measured(shapes),
            20.0,
            0,
            Acct::Engine,
            Store::Span,
        );
        assert!(r.cwnd_live_mean > 0.0 && r.infl_mean > 0.0, "{cell}: the loop must run");
        let ratio = r.cwnd_live_mean / sigma_wire;
        assert!(
            ratio > 2.0,
            "{cell}: the bench's Sigma cwnd {:.0} is not above 2x the wire's \
             Sigma anchor {sigma_wire:.0} — if this ever drops, the headroom \
             attribution of the coupling model is void and must \
             be re-taken",
            r.cwnd_live_mean
        );
        assert!(
            ratio < 12.0,
            "{cell}: Sigma cwnd {:.0} is {ratio:.1}x the wire's anchor sum \
             {sigma_wire:.0}, outside the band this section measured",
            r.cwnd_live_mean
        );
            // …and the headroom is what keeps `available()` open: the
            // in-flight the store cap can produce is a fraction of it.
        assert!(
            r.infl_mean < r.cwnd_live_mean,
            "{cell}: Sigma in_flight {:.0} must sit under Sigma cwnd {:.0} on \
             average — that gap IS the unclosed available()",
            r.infl_mean,
            r.cwnd_live_mean
        );
    }
}

/// Does the balanced ledger move Σ`cwnd`/Σ-anchor toward 1?
///
/// The counter-delta over-release was a candidate owner of the bench's Σ`cwnd`
/// at 3.6–6.6× the wire's measured Σ-anchor, on the argument that an
/// over-release keeps `available() > 0`. The engine fix (`RWM_CHARGE_RECOVERY`
/// + `RWM_RELEASE_1TO1`) is this bench's `Acct::Traffic` level by construction,
/// so the question needs no new arm.
///
/// The answer is no, and at c8 the sign is the opposite: balancing moves the
/// ratio 4.59× → 4.33× (−5.6 %) at c7 and 4.99× → 6.25× (+25.3 %) at c8.
/// Charging the recovery channels and releasing 1:1 raises Σ`in_flight`
/// (2 884 → 4 181 at c7, 3 050 → 4 237 at c8) — an honest gauge is a fuller
/// gauge — and Copa answers the tighter `available()` by growing `cwnd`. The
/// divergence belongs to the bench horizon (see
/// `the_anchors_windowed_extremes_expire_at_ten_seconds_and_the_wire_never_reaches_it`).
/// Bounded in both directions and by sign.
#[test]
fn balancing_the_ledger_does_not_move_sigma_cwnd_toward_the_wires_anchor() {
    for (cell, geom, shapes) in [
        ("c7", vec![C2, C2], &ACK_C7[..]),
        ("c8", vec![C2, C3], &ACK_C8[..]),
    ] {
        let sigma_wire: f64 = shapes.iter().map(|s| s.anchor_sym()).sum();
        let run = |acct| {
            simulate_full(
                &geom,
                Arm::Unified,
                Feed::Measured(shapes),
                20.0,
                0,
                acct,
                Store::Span,
            )
        };
        let e = run(Acct::Engine);
        let t = run(Acct::Traffic);
        // Measurement discipline rule 1: the axis must have run — the
        // un-metered ledger over-releases and the balanced one does not.
        assert!(
            e.led.releases > e.led.charges && t.led.charges == t.led.wire(),
            "{cell}: the accounting axis did not execute"
        );
        let (re, rt) = (e.cwnd_live_mean / sigma_wire, t.cwnd_live_mean / sigma_wire);
        println!(
            "[LEDGER-SIGMA] {cell}: sigma_wire {sigma_wire:.0}  ENGINE cwnd \
             {:.0} ({re:.2}x) infl {:.0}  |  BALANCED cwnd {:.0} ({rt:.2}x) \
             infl {:.0}  |  move {:+.1}%",
            e.cwnd_live_mean,
            e.infl_mean,
            t.cwnd_live_mean,
            t.infl_mean,
            (rt / re - 1.0) * 100.0
        );
        assert!(
            re > 0.0 && rt > 0.0,
            "{cell}: the loop must run in both arms"
        );
        // The balanced ledger does not bring the ratio near 1 at either dual;
        // it stays in the engine arm's multiple-of-the-anchor class.
        assert!(
            rt > 3.0,
            "{cell}: the balanced ledger left Sigma cwnd at {rt:.2}x the wire's \
             anchor — if this ever approaches 1, the release fix DOES own \
             'The Coupling Model' FINDING 4 and that must be claimed, not \
             assumed away"
        );
        // An honest gauge is a fuller gauge, at both cells — the mechanism
        // behind the sign.
        assert!(
            t.infl_mean > e.infl_mean,
            "{cell}: balancing the ledger must RAISE Sigma in_flight ({:.0} vs \
             {:.0}) — that is what stops the leak",
            t.infl_mean,
            e.infl_mean
        );
        // And the sign, per cell.
        let move_pct = (rt / re - 1.0) * 100.0;
        let band = if cell == "c7" { (-20.0, 0.0) } else { (10.0, 45.0) };
        assert!(
            move_pct > band.0 && move_pct < band.1,
            "{cell}: Sigma cwnd/Sigma anchor moved {move_pct:+.1}% \
             ({re:.2}x -> {rt:.2}x), outside the measured band \
             [{:+.0}%, {:+.0}%]",
            band.0,
            band.1
        );
    }
}

/// Transcription pin for the wire's brake/queue columns, and the bound on the
/// claim that the bench's link builds a 2.4–5.7× RTprop standing queue the
/// wire does not.
///
/// A "≈1×" wire reading comes from its RTprop column, which is the wire's
/// `min_rtt` — comparing a bench `min_rtt` inflation against it compares a
/// quantity with itself. The wire's standing queue is the `q_p50 = rtt − rtp`
/// column, and it reads 5.8–10.5× the same `rtp`: the wire queues more than
/// the bench, not less.
#[test]
fn the_wires_own_columns_say_the_queue_is_many_rtprops_and_the_cap_is_not_the_brake() {
    for w in WIRE_BRAKE {
        let tag = format!("{}/{}", w.cell, w.arm);
        // Internal identities: a typo fails here rather than surviving.
        assert!(w.reps >= 26, "{tag}: too few reps behind the transcription");
        assert!(w.occ <= w.cap, "{tag}: occupancy above its own cap");
        assert!(w.q99_ms >= w.q_ms, "{tag}: p99 below p50");
        assert!(w.rtprop_ms > 0.0 && w.q_ms > 0.0, "{tag}: degenerate row");

        // Every transcribed row — single cell and both duals, both arms —
        // carries a standing queue of many RTprops.
        assert!(
            w.queue_over_rtprop() > 5.0,
            "{tag}: the wire's standing queue is {:.1} ms on a {:.1} ms RTprop \
             ({:.1}x). The dispatch's premise was that the wire reads ~1x. If \
             this row ever drops below 5x, the queue-attribution finding \
             is void and must be re-taken.",
            w.q_ms,
            w.rtprop_ms,
            w.queue_over_rtprop()
        );
    }

    // Which brake binds is cell-keyed: at the single cell the store cap is the
    // brake (occupancy at its ceiling, the backpressure arm taking 40 % of the
    // sender loop's wakeups); at both duals it is not (occupancy at a third
    // to a half of a 4× larger cap, backpressure 0.0–7.8 %).
    for w in WIRE_BRAKE.iter().filter(|w| w.cell == "sc2") {
        assert!(
            w.occ_over_cap() > 0.95 && w.paused_pct > 30.0,
            "sc2/{}: the single cell must be store-cap-bound (occ/cap {:.2}, paused {:.1}%)",
            w.arm,
            w.occ_over_cap(),
            w.paused_pct
        );
    }
    for w in WIRE_BRAKE.iter().filter(|w| w.cell != "sc2") {
        assert!(
            w.occ_over_cap() < 0.60 && w.paused_pct < 10.0,
            "{}/{}: the dual cells must NOT be store-cap-bound — the bench's \
             admission model assumes they are (occ/cap {:.2}, paused {:.1}%)",
            w.cell,
            w.arm,
            w.occ_over_cap(),
            w.paused_pct
        );
    }
    // c7: the store-cap backpressure arm never fires, over 69 reps.
    assert_eq!(
        wire_brake("c7", "A").paused_pct,
        0.0,
        "c7-A: the store-cap gate is measured never to close on the wire"
    );
}

/// The engine's windowed extremes expire at 10 s, and every wire transfer is
/// shorter (measurement discipline rule 1: the mechanism executes).
///
/// `CopaState::window_duration` (10 s) is the cutoff for both deques the BDP
/// anchor is built from: the `min_rtt` sample deque (`expire_old_samples`) and
/// the `max_bw` max-filter (`bw_evict_before`, same cutoff). Inside that
/// window nothing expires, so `min_rtt` and `max_bw` are whole-transfer
/// extrema.
///
/// Drives the real `PathState` on a `MockClock`: a low RTT sample taken at t=0
/// still floors `min_rtt` at t = 9 s and no longer does at t = 11 s. The
/// bench's published horizon is 20 s; the wire's transfers are 2.4–9.7 s. That
/// regime difference owns the Σ`cwnd` divergence.
#[test]
fn the_anchors_windowed_extremes_expire_at_ten_seconds_and_the_wire_never_reaches_it() {
    // Every cell this bench is scored against, with the median L1 transfer
    // duration. All reps at all four are under the window.
    const WIRE_SECONDS: &[(&str, f64)] =
        &[("sc2", 9.06), ("c2r100", 9.64), ("c7", 9.23), ("c8", 2.44)];
    for (cell, secs) in WIRE_SECONDS {
        assert!(
            *secs < 10.0,
            "{cell}: transfer {secs}s must sit inside the engine's own 10 s \
             filter window for this attribution to hold"
        );
    }
    // …and the bench's published horizon is not.
    assert!(
        20.0 > 10.0,
        "the published readouts run 20 s — twice the window — which is the point"
    );

    // The switch, on the real PathState.
    for (label, elapsed_s, still_floored) in
        [("inside the window", 9.0_f64, true), ("outside it", 11.0_f64, false)]
    {
        let clock = Arc::new(MockClock::new());
        let mut sched = Scheduler::new(clock.clone());
        sched.add_path(0);
        // One short rtt sample at t = 0 — the pre-queue floor.
        let low = Duration::from_millis(10);
        if let Some(p) = sched.path_mut(0) {
            p.record_rtt_sample(low);
        }
        let floor0 = sched.path(0).and_then(|p| p.min_rtt());
        assert_eq!(floor0, Some(low), "{label}: the floor must be taken");
        // Then a long steady standing-queue RTT, sampled every 100 ms until
        // `elapsed_s` — exactly what a backed-up link produces.
        let high = Duration::from_millis(110);
        let mut t = 0.0_f64;
        while t < elapsed_s {
            clock.advance(Duration::from_millis(100));
            t += 0.1;
            if let Some(p) = sched.path_mut(0) {
                p.record_rtt_sample(high);
            }
        }
        let now = sched.path(0).and_then(|p| p.min_rtt()).expect("a floor exists");
        if still_floored {
            assert_eq!(
                now, low,
                "{label}: inside the 10 s window the t=0 floor still wins — this \
                 is why the wire's RTprop column reads its configured RTT"
            );
        } else {
            assert_eq!(
                now, high,
                "{label}: past the window the floor expires onto the standing \
                 queue — this is why the bench's 20 s min_rtt is inflated"
            );
        }
    }
}

/// The queue readout — the bench's own standing queue and brake share against
/// the wire's, in the wire's units and off the wire's columns. It shows the
/// ordering is the other way round: the wire queues more than the bench.
#[test]
#[ignore = "component bench; run with --ignored --nocapture"]
fn sf_queue_and_brake_against_the_wire() {
    println!("\n=== WHICH BRAKE BINDS, AND HOW MUCH QUEUE IT BUILDS ===");
    println!("wire columns: occ_p50/occcap_p50 (win=store_len/cap, net/diag.rs:845),");
    println!("              wait_paused (the store-cap backpressure wait arm, net/mod.rs:5663),");
    println!("              q_p50 = rtt - rtp (net/diag.rs:617), rtp_med = min_rtt.");
    println!("bench columns: the SAME quantities off the SAME PathState accessors.\n");
    println!(
        "{:<28} {:<4} | {:>7} {:>7} {:>6} {:>8} | {:>7} {:>7} {:>6} {:>8}",
        "cell", "arm", "occ", "cap", "o/c", "paused%", "q ms", "rtp ms", "q/rtp", "gate%"
    );
    for (name, cell, geom, shapes) in [
        ("sc2  single fast (c2r100)  ", "sc2", vec![C2], &ACK_SC2[..]),
        ("c7   dual symmetric        ", "c7", vec![C2, C2], &ACK_C7[..]),
        ("c8   dual asym (r+RTT)     ", "c8", vec![C2, C3], &ACK_C8[..]),
    ] {
        for (arm, label) in [(Arm::Legacy, "A"), (Arm::Unified, "AU")] {
            let w = wire_brake(cell, label);
            println!(
                "{:<28} {:<4} | {:>7.0} {:>7.0} {:>6.2} {:>7.1}% | {:>7.1} {:>7.1} {:>6.1}x {:>8}   WIRE (n={})",
                name, label, w.occ, w.cap, w.occ_over_cap(), w.paused_pct,
                w.q_ms, w.rtprop_ms, w.queue_over_rtprop(), "-", w.reps
            );
            let mut occ = 0.0;
            let mut cap = 0.0;
            let mut gate = 0.0;
            let mut q = 0.0;
            let mut rtp = 0.0;
            let mut n = 0.0;
            for salt in 0..8u64 {
                let r = simulate_full(
                    &geom, arm, Feed::Measured(shapes), 20.0, salt, Acct::Engine, Store::Span,
                );
                occ += r.store_len_mean;
                cap += r.mean_cap;
                gate += r.gate_closed_pct();
                // Pool the paths the way the wire's per-path DIAG regex does:
                // every path's sample lands in the same q/rtp population.
                let np = geom.len();
                for p in 0..np {
                    q += r.queue_ms(p);
                    rtp += r.min_rtt_ms(p);
                }
                n += np as f64;
            }
            let (occ, cap, gate) = (occ / 8.0, cap / 8.0, gate / 8.0);
            let (q, rtp) = (q / n, rtp / n);
            println!(
                "{:<28} {:<4} | {:>7.0} {:>7.0} {:>6.2} {:>8} | {:>7.1} {:>7.1} {:>6.1}x {:>7.1}%   BENCH (8 seeds)",
                "", label, occ, cap, occ / cap.max(1e-9), "-", q, rtp, q / rtp.max(1e-9), gate
            );
        }
        println!();
    }
}

/// The divergence is in `min_rtt`, not in the queue, and the bench's brake is
/// not the wire's. Scored at c7 on `Arm::Legacy` (the arm the wire's A-arm
/// columns were taken on), at 20 s like the Σ`cwnd` pin, so the two rows read
/// together:
///
///   1. The bench's standing queue, in RTprops, is smaller than the wire's.
///   2. The bench's `min_rtt` is a large multiple of the wire's — and that is
///      what reaches the anchor, the cwnd floor and `available()`.
///   3. The bench's admission gate is closed most of the time and the wire's
///      is never closed at this cell (`wait_paused` = 0.0 % over 69 reps).
#[test]
fn the_benchs_queue_is_smaller_than_the_wires_and_its_min_rtt_is_where_the_gap_is() {
    let w = wire_brake("c7", "A");
    let r = simulate_full(
        &[C2, C2], Arm::Legacy, Feed::Measured(&ACK_C7[..]), 20.0, 0,
        Acct::Engine, Store::Span,
    );
    let q: f64 = (r.queue_ms(0) + r.queue_ms(1)) / 2.0;
    let rtp: f64 = (r.min_rtt_ms(0) + r.min_rtt_ms(1)) / 2.0;
    assert!(q > 0.0 && rtp > 0.0, "the loop must run and produce both columns");

    // (1) The bench queues less than the wire.
    assert!(
        q / rtp < w.queue_over_rtprop(),
        "c7: the bench's standing queue is {:.1}x RTprop and the wire's is \
         {:.1}x — the dispatch assumed the reverse. If this ever flips, \
         the queue-attribution finding must be re-taken.",
        q / rtp,
        w.queue_over_rtprop()
    );

    // (2) Where the gap is: the bench's windowed-min RTT sits at a large
    // multiple of the wire's, because the bench's 20 s horizon is twice
    // `CopaState::window_duration` and the wire's 9.23 s transfer is inside it.
    let mrtt_x = rtp / w.rtprop_ms;
    assert!(
        mrtt_x > 2.0,
        "c7: the bench's min_rtt is {rtp:.1} ms against the wire's {:.1} ms \
         ({mrtt_x:.1}x) — this is the quantity that reaches the anchor floor",
        w.rtprop_ms
    );
    assert!(
        mrtt_x < 8.0,
        "c7: min_rtt inflation {mrtt_x:.1}x is outside the band this section \
         measured"
    );

    // (3) And the brake is not the same brake.
    assert!(
        r.gate_closed_pct() > 50.0,
        "c7: the bench's store-cap gate is closed {:.1}% of admission \
         opportunities — its whole loop assumes the cap is the brake",
        r.gate_closed_pct()
    );
    assert_eq!(
        w.paused_pct, 0.0,
        "c7: …while the wire's store-cap backpressure arm never fires at all"
    );
}

/// Running the bench inside the engine's own filter window puts `min_rtt` back
/// on the floor and Σ`cwnd` back on the wire's anchor.
///
/// The Σ`cwnd` divergence (3.6× at c7, 6.6× at c8) comes from the bench's 20 s
/// horizon, twice `CopaState::window_duration`. At the wire's own median
/// transfer durations (c8 2.44 s, c7 9.23 s; all reps under 10 s):
///
///   * `min_rtt` returns to the configured RTprop (×1.01, both cells), because
///     the t≈0 floor sample never expires;
///   * Σ`cwnd`/Σ-anchor moves from 3.55× → 0.67× at c7 and lands at 1.28× at
///     c8, inside the ±0.3 band stated in advance;
///   * at c8 the standing queue lands on the wire's within 2 % (343.9 ms
///     modelled vs 338 ms measured, on a 34.3 vs 38 ms RTprop).
///
/// The residual is at c7 and is the brake regime, not the queue: the bench is
/// store-cap-bound there and the wire is measured never to be.
#[test]
fn matching_the_horizon_to_the_wires_own_transfer_puts_sigma_cwnd_on_the_wires_anchor() {
    for (cell, geom, shapes, horizon, lo, hi) in [
        // (cell, geometry, ack shapes, the wire's own median transfer, band)
        ("c8", vec![C2, C3], &ACK_C8[..], 2.5_f64, 0.7_f64, 1.6_f64),
        ("c7", vec![C2, C2], &ACK_C7[..], 9.0_f64, 0.4_f64, 1.0_f64),
    ] {
        assert!(
            horizon < 10.0,
            "{cell}: the whole point is to sit inside CopaState::window_duration"
        );
        let sigma_wire: f64 = shapes.iter().map(|s| s.anchor_sym()).sum();
        let mut cwnd = 0.0;
        let mut infl_x = 0.0;
        let seeds = 4u64;
        for salt in 0..seeds {
            let r = simulate_full(
                &geom, Arm::Legacy, Feed::Measured(shapes), horizon, salt,
                Acct::Engine, Store::Span,
            );
            cwnd += r.cwnd_live_mean;
            infl_x += (0..geom.len())
                .map(|p| r.rtt_inflation(p, geom[p].1))
                .sum::<f64>()
                / geom.len() as f64;
        }
        let cwnd = cwnd / seeds as f64;
        let infl_x = infl_x / seeds as f64;
        let ratio = cwnd / sigma_wire;

        // (a) The anchor's own RTprop is honest inside the window — the
        // mechanism; the Σ below is its consequence.
        assert!(
            infl_x < 1.15,
            "{cell}: min_rtt is {infl_x:.2}x the configured RTprop at a \
             {horizon}s horizon — inside the 10 s window the t=0 floor must \
             still win, or the min_rtt-floor attribution \
             is wrong"
        );
        // (b) And Σ cwnd lands on the wire's measured anchor sum.
        assert!(
            ratio > lo && ratio < hi,
            "{cell}: Sigma cwnd {cwnd:.0} is {ratio:.2}x the wire's measured \
             Sigma-anchor {sigma_wire:.0}, outside the [{lo}, {hi}] band this \
             section measured at a {horizon}s horizon (it is 3.55x/6.6x at the \
             published 20 s)"
        );
    }
}

/// The horizon axis — the single-axis experiment that isolates the first
/// diverging quantity.
///
/// `CopaState::window_duration` (10 s) governs both windowed extremes the
/// anchor is built from: the `min_rtt` sample deque and the `max_bw`
/// max-filter (`bw_evict_before`, same cutoff). Every wire transfer this bench
/// is scored against is shorter than that (c7 9.23 s, sc2 9.11 s, c2r100
/// 9.66 s, c8 2.56 s), so on the wire neither filter expires a sample:
/// `min_rtt` and `max_bw` are whole-transfer extrema, latched before the
/// standing queue builds.
///
/// The bench runs 20 s, so both filters roll, `min_rtt` climbs onto the
/// standing queue, and the anchor (`max_bw · min_rtt`) — the cwnd floor via
/// `clamp_cwnd_with_anchor` — climbs with it, and Σ`cwnd` too. This sweep
/// reports, per horizon, the quantities the attribution chain is made of.
#[test]
#[ignore = "component bench; run with --ignored --nocapture"]
fn sf_horizon_against_the_engines_own_filter_window() {
    println!("\n=== THE HORIZON AXIS vs CopaState::window_duration = 10 s ===");
    println!("wire transfer durations (the `seconds` column of the L1 records): c7 9.23s, sc2 9.11s, c8 2.56s.");
    println!("Below 10 s NEITHER windowed extreme expires — min_rtt and max_bw are whole-transfer.\n");
    for (name, cell, geom, shapes) in [
        ("sc2  single fast (c2r100)  ", "sc2", vec![C2], &ACK_SC2[..]),
        ("c7   dual symmetric        ", "c7", vec![C2, C2], &ACK_C7[..]),
        ("c8   dual asym (r+RTT)     ", "c8", vec![C2, C3], &ACK_C8[..]),
    ] {
        let sigma_wire: f64 = shapes.iter().map(|s| s.anchor_sym()).sum();
        let w = wire_brake(cell, "A");
        println!(
            "{name}  wire: Sigma-anchor {sigma_wire:.0} sym  q {:.0} ms  rtp {:.0} ms  q/rtp {:.1}x  occ/cap {:.2}",
            w.q_ms, w.rtprop_ms, w.queue_over_rtprop(), w.occ_over_cap()
        );
        println!(
            "  {:>6} | {:>8} {:>8} {:>7} | {:>7} {:>7} {:>6} | {:>7} {:>7} {:>7}",
            "horiz", "S cwnd", "S/Sw", "S infl", "q ms", "rtp ms", "q/rtp", "minRTTx", "zero%", "gp"
        );
        for horizon in [2.5_f64, 5.0, 9.0, 20.0] {
            let mut acc = [0.0_f64; 7];
            let seeds = 6u64;
            for salt in 0..seeds {
                let r = simulate_full(
                    &geom, Arm::Legacy, Feed::Measured(shapes), horizon, salt,
                    Acct::Engine, Store::Span,
                );
                let np = geom.len();
                let q: f64 = (0..np).map(|p| r.queue_ms(p)).sum::<f64>() / np as f64;
                let rtp: f64 = (0..np).map(|p| r.min_rtt_ms(p)).sum::<f64>() / np as f64;
                let infl_x: f64 =
                    (0..np).map(|p| r.rtt_inflation(p, geom[p].1)).sum::<f64>() / np as f64;
                acc[0] += r.cwnd_live_mean;
                acc[1] += r.infl_mean;
                acc[2] += q;
                acc[3] += rtp;
                acc[4] += infl_x;
                acc[5] += r.zero_pct();
                acc[6] += r.goodput_sym_s();
            }
            for a in acc.iter_mut() {
                *a /= seeds as f64;
            }
            println!(
                "  {:>5.1}s | {:>8.0} {:>7.2}x {:>7.0} | {:>7.1} {:>7.1} {:>5.1}x | {:>6.2}x {:>6.1}% {:>7.0}",
                horizon, acc[0], acc[0] / sigma_wire, acc[1], acc[2], acc[3],
                acc[2] / acc[3].max(1e-9), acc[4], acc[5], acc[6]
            );
        }
        println!();
    }
}

/// (8) The validation gate — V1/V2/V3, scored per path before the geography
/// question may be asked.
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
                // V3 is scored against the measured per-window ranges, scaled
                // to this bench path's mean gap (the shape is dimensionless).
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

/// (9) The geography, on measured inputs + the accounting axis. The same
/// G1/G2 and statistic as the accounting axis, so the two runs differ in
/// exactly one thing: the ack stream.
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
                    // Repairs-in-counters, as an emergent property: under the
                    // engine's ledger every wire symbol enters the receiver's
                    // expected/received counters, so Σcrecv/srcack is
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

/// (17) The composed cap law, scored as one arm (paper §10).
///
/// Reports, per geometry, against the shipped arm and against the pool law
/// alone, so the brake's contribution is separable from the pool's:
///
///   * cap — and whether it is interior. The predecessor operated at its
///     ceiling (121/126 dual reps at exactly 4096), so every measurement
///     through it measured a constant; the composed law's only remaining
///     bound is a memory bound outside the law, and a composed cap landing
///     on it is a stop, not a result.
///   * zero% — the `[SF]` zero-fraction.
///   * gp — the goodput class, with the fold against the shipped arm.
///   * brake% — the late-stage brake's liveness. Read it beside the 3T
///     column: where goodput agrees and `brake%` is non-zero, the brake bound
///     and changed nothing (a null result); where `brake%` is zero, the brake
///     never bound and the arms are the same law.
#[test]
#[ignore = "component bench; run with --ignored --nocapture"]
fn sf_composed_cap_law_as_one_arm() {
    println!("\n=== THE COMPOSED CAP LAW as ONE ARM (paper §10) — 8 s ===");
    println!(
        "law: cap = SUM_live [ rate_i*RTprop_i + rate_i*stall(delta,rho,srtt_i) ] \
         + 2*rate_fast*skew"
    );
    println!(
        "     brake: cwnd_full over live_paths(), cap_i = the path's OWN cwnd \
         (no new constant)"
    );
    println!("     bounds OUTSIDE the law: memory {WIN_STORE_MAX}, paroled floor {FLOOR}");
    println!(
        "\n{:<28} {:>9} {:>9} {:>9} {:>7} {:>7} {:>9} {:>9} {:>7} {:>7}",
        "geometry", "A cap", "3T cap", "C cap", "interior", "brake%", "A gp", "C gp", "C/A", "zero%"
    );
    for (name, geom) in composed_geometries() {
        let a = simulate(&geom, Arm::Legacy, 8.0);
        let t = simulate(&geom, Arm::ThreeTermCell, 8.0);
        let c = simulate(&geom, Arm::Composed, 8.0);
        let interior = c.mean_cap > FLOOR as f64 && c.mean_cap < WIN_STORE_MAX as f64;
        println!(
            "{:<28} {:>9.1} {:>9.1} {:>9.1} {:>8} {:>6.1}% {:>9.0} {:>9.0} {:>6.2}x {:>6.1}%",
            name,
            a.mean_cap,
            t.mean_cap,
            c.mean_cap,
            if interior { "YES" } else { "NO-STOP" },
            c.brake_closed_pct(),
            a.goodput_sym_s(),
            c.goodput_sym_s(),
            c.goodput_sym_s() / a.goodput_sym_s().max(1e-9),
            c.zero_pct(),
        );
    }
    println!(
        "\nNOTE: `c1` (the 1 Gbit single) has NO geometry in this bench and one was \
         NOT invented for this table — the fast single above is the single-path \
         class. The 1 Gbit cell is a VM question."
    );
    println!(
        "NOTE: the quad's \"cold-start placement lock-in\" is RETRACTED (2026-08-18): \
         it was three `pid < 2` gauge guards left behind when MAX_PATHS went 2 -> 4, \
         not a placement defect. The quad spreads evenly over all four legs. See \
         `the_symmetric_quad_is_deterministic_and_all_four_legs_carry_and_warm`; the \
         composed arm's four-way split is bounded by \
         `the_composed_law_does_not_starve_a_leg_of_the_quad`."
    );
}

/// (10) The pooled-ceiling candidate, re-scored on the measured era and
/// printed either way so the number stays on the record.
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

/// The law is solved, not chosen: the two quantities the model needs beyond
/// the measured quantiles are roots of measured identities, at every path:
///
///   * `alpha` is the root of "the distribution's mean is `1/rate_lr`", so
///     reconstructing the mean from the model's pieces returns 1;
///   * `u_c` is the root of "the silence fraction equals the drain duty
///     cycle", i.e. `∫_0^{u_c} Q = q50`;
///   * the model's marginal reproduces the measured quantiles: `Q` at
///     0.5/0.9/0.99 is the transcribed p50/p90/p99 (at the range position
///     `theta` the mean constraint left it).
///
/// An edit to the interpolation, the tail or the duty identity fails on the
/// identity rather than drifting into a fitted curve.
#[test]
fn measured_ack_law_is_solved_from_the_measurement() {
    for sh in ACK_ALL {
        // Resolved against the wire's own rate, so the reconstruction can be
        // checked in the wire's units.
        let g = AckGaps::new(sh, sh.rate_lr);
        assert!(g.alpha > 1.0, "{}: alpha {} would give an infinite mean gap", sh.row, g.alpha);
        assert!(g.theta >= 0.0 && g.theta <= 0.5, "{}: theta {}", sh.row, g.theta);
        assert!(g.u_c > 0.5 && g.u_c < 1.0, "{}: u_c {}", sh.row, g.u_c);
        // (1) The mean constraint: ∫_0^1 Q du = 1, i.e. the model's mean gap
        // is `1/rate_lr`. This is what `alpha` was solved for.
        let m = g.cdf_mean_to(1.0);
        assert!(
            (m - 1.0).abs() < 1e-6,
            "{}: the model's mean gap is {m:.6}x the measured one — alpha did not solve",
            sh.row
        );
        // (2) The duty identity: ∫_0^{u_c} Q du = q50.
        assert!(
            (g.cdf_mean_to(g.u_c) - g.q50).abs() < 1e-9,
            "{}: the silence threshold does not satisfy the drain duty identity",
            sh.row
        );
        // (3) The marginal is the measurement's: Q at the three measured
        // quantiles is the transcribed numbers, in µs, at this path's scale.
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
        // (4) The tail is truncated at a gap the instrument saw.
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
        // And the drain is faster than arrivals — otherwise the observer is
        // not an observer and the era is inert.
        assert!(g.q50 < 1.0, "{}: p50 gap is not below the mean gap", sh.row);
    }
}

/// Measurement discipline rule 1 for the measured era: the mechanism executes,
/// as the wire describes it.
///
///   * every ack reaches `record_delivery` with `count = 1` — an identity
///     between the observer's count and the delivered count, so a batching
///     bug cannot hide;
///   * the shipped 1 ms floor does the folding: most calls are rejected, and
///     the accepted ones fold many acks each;
///   * the sub-tick clock walk is monotone and lands on the tick grid (the
///     refresh count is unchanged from every other era).
#[test]
fn measured_era_feeds_the_shipped_floor_one_ack_at_a_time() {
    let m = simulate_acct(&[C2, C3], Arm::Legacy, Feed::Measured(&ACK_C8), 6.0, 0, Acct::Off);
    let h = simulate_acct(&[C2, C3], Arm::Legacy, Feed::Honest, 6.0, 0, Acct::Off);
    // drecv = 1: one observation per delivered symbol. The observer is
    // work-conserving, so the identity is exact once the acks still inside it
    // at the horizon are counted; that residual must be small, or the observer
    // is throttling the loop rather than re-timing it.
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
    // The floor is the clock, and it rejects as measured.
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
    // The tick grid is untouched: the sub-tick walk adds or loses no dyn-cap
    // refresh.
    assert_eq!(m.ticks, h.ticks, "the sub-tick clock walk moved the refresh grid");
}

/// The measured era is an era, not a rewrite: `Feed::Measured` leaves every
/// other feed bit-identical. The era axis is a claim about the sampler only,
/// so the transport half — deliveries, retransmits, the ledger — must come out
/// of `Feed::Honest` unchanged.
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

/// The validation gate, bounded — V1 and V2 as always-on assertions at c7 and
/// c8, on the pre-registered tolerances. 3 seeds × 8 s: the regression bound,
/// not the evidence.
#[test]
fn measured_era_reproduces_the_wires_floor_and_anchor() {
    for (geom, shapes) in [(vec![C2, C2], &ACK_C7), (vec![C2, C3], &ACK_C8)] {
        let feed = Feed::Measured(&shapes[..]);
        for s in 0..3u64 {
            let r = simulate_acct(&geom, Arm::Legacy, feed, 8.0, s, Acct::Off);
            for (i, sh) in shapes.iter().enumerate() {
                // V2 — the floor's rejection rate, a prediction of the model.
                let rej = r.obs[i].reject_pct();
                assert!(
                    (rej - sh.rej_pct).abs() <= V2_REJECT_TOL_PTS,
                    "{} seed {s}: floor rejected {rej:.1}%, the wire {:.1}% (V2 = +/-{:.0} pts)",
                    sh.row,
                    sh.rej_pct,
                    V2_REJECT_TOL_PTS
                );
                // V1 — the realized anchor over-read on the wire's definition
                // (`max_bw/rate_lr`, the RTT divided out), the quantity the
                // store-cap Σ and the cwnd anchor floor consume once the
                // path's own RTprop is put back.
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

/// With the ack stream measured, the pre-registered geography fails, for an
/// arithmetic reason: the measured over-read saturates the store-cap law's
/// `N·knee` ceiling, and a saturated cap cannot express the U-fold.
///
/// The shipped law is `clamp(gain·N·Σ_set, floor, N·knee)`; U changes only
/// which set the Σ ranges over. At the measured `xanchor` the unclamped law
/// asks for 2.7× the ceiling at c8, so both arms clamp to 4096 and the set is
/// unobservable — dropping a path from Σ removes 40 % of the anchor mass, far
/// short of the 2.7× the clamp swallows.
///
/// Both halves, at 3 seeds × 8 s (regression bound):
///
///   * the arithmetic, on the real law: `gain·N·Σ` at the measured per-path
///     `xanchor` exceeds `N·knee` by more than the mass U can remove;
///   * the consequence, in the loop: the mean cap sits near the ceiling on
///     both arms at c8, and the U-fold the engine's ledger produced on the
///     honest era (7.1×) collapses below 2×.
///
/// Raising `RWM_STORE_PATH_POOL`, fixing the anchor era or changing the
/// ceiling fails this test and re-scores the diagnosis.
#[test]
fn measured_over_read_saturates_the_knee_ceiling_and_collapses_the_u_fold() {
    // (1) The arithmetic, on the shipped law.
    let ceiling = (2 * KNEE) as f64; // 4096
    let sigma_full = C2.0 * C2.1 * ACK_C8_P0.xanchor + C3.0 * C3.1 * ACK_C8_P1.xanchor;
    let sigma_fast = C2.0 * C2.1 * ACK_C8_P0.xanchor; // what U's set change can remove
    assert!(
        GAIN * 2.0 * sigma_full > ceiling,
        "the measured c8 anchor does not even reach the ceiling: {:.0} vs {ceiling}",
        GAIN * 2.0 * sigma_full
    );
    assert_eq!(shipped_chain(sigma_full, 2), ceiling as usize);
    // The set change U makes is smaller than the headroom the clamp eats, so
    // both arms land on the same number.
    assert_eq!(
        shipped_chain(sigma_fast, 2),
        shipped_chain(sigma_full, 2),
        "dropping the slow leg from Sigma must still clamp — otherwise the fold survives"
    );

    // (2) The consequence, in the closed loop.
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
    // Measurement discipline rule 1: the era must have run.
    let probe = simulate_acct(&[C2, C3], Arm::Legacy, Feed::Measured(&ACK_C8), 8.0, 0, Acct::Engine);
    assert!(probe.obs[0].n_obs > 10_000 && probe.obs[1].n_obs > 1_000);
    assert!(probe.led.taper > 0 && probe.led.margin > 0 && probe.led.retx > 0);

    // Both arms ride the ceiling. The means carry the warm-up ramp from the
    // 128 boot cap, so they are scored against 0.7× rather than 1.0×; the
    // load-bearing half is that the two arms converge (on the honest era they
    // differ by 5.8×, 379 vs 2192).
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
    // The c8 A arm stays out of the published bench's 37 % class. Its
    // full-horizon level (7.2 %, caught on 88 % of seeds, inside the wire's
    // ≈4 % class) is deliberately not asserted: at 8 s the mean cap is still
    // climbing off the boot value. This bounds the class, which is stable.
    assert!(
        a_zero < 25.0,
        "the c8 A arm fell back into the published bench's high class: {a_zero:.1}%"
    );
}

fn fold_str(a: f64, u: f64) -> String {
    if a > 0.0 { format!("{:.1}x", u / a) } else { "inf".into() }
}

// ── Guards (always run) ───────────────────────────────────────────────────

/// Measurement discipline rule 1: the loop executes. The simulated sender
/// refreshes the cap, saturates paths and delivers — a bench that never
/// saturates would report 0 % on both arms and prove nothing.
#[test]
fn bench_loop_executes() {
    let r = simulate(&[C2, C3], Arm::Legacy, 4.0);
    assert!(r.ticks > 700, "dyn-cap refresh ticks = {} (expected ~800 at 5 ms over 4 s)", r.ticks);
    assert!(r.delivered > 10_000, "no delivery: {} symbols", r.delivered);
    assert!(r.short > 0, "no path ever saturated — the mechanism under test never ran");
    assert!(r.mean_cap > BOOT as f64, "the cap never left the boot value: {}", r.mean_cap);
}

/// The load-bearing code fact, pinned against the real scheduler.
///
/// `active_paths()` (`p.active && available() > 0`) is not a gate on the
/// reliable data path. `emit_source` places with
/// `Scheduler::place_symbol(false, &[])` → `place_costs`, which filters on
/// `p.active` alone. So a cwnd-saturated path keeps receiving source symbols,
/// `in_flight` may exceed `cwnd`, and `available()` stays 0 until acks drain
/// it. `active_paths()` at the dyn-cap phase is a pure observable of
/// saturation, never a brake on it; the only brake at the battery's arms is
/// `store_len >= effective_store_cap` (`cwnd_full` is off: `RWM_INFL_CAP`
/// defaults to 0). If this changes, the `[SF]` gauge changes meaning.
#[test]
fn reliable_placement_does_not_filter_on_cwnd_headroom() {
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    sched.add_path(0);
    sched.add_path(1);
    // Saturate both paths past their cwnd, as an unbraked store cap does.
    for id in [0u32, 1u32] {
        let cw = sched.path(id).map(|p| p.cwnd).unwrap_or(0);
        assert!(cw > 0);
        if let Some(p) = sched.path_mut(id) {
            p.charge_in_flight(cw + 1);
        }
    }
    // The saturation filter now reads empty...
    assert!(
        sched.active_paths().is_empty(),
        "both paths were charged past cwnd; active_paths() must be empty"
    );
    assert_eq!(sched.live_paths().len(), 2, "both paths are still live");
    // ...and `best_source_path` / `schedule`, which do filter, stall.
    assert!(sched.best_source_path().is_none());
    assert!(sched.schedule(Vec::new(), Vec::new()).is_empty());
    // But the reliable source emitter's placement does not: it still returns
    // a full candidate set over the live paths.
    let probs = sched.place_probs(false, &[]);
    assert_eq!(
        probs.len(),
        2,
        "place_costs must range over LIVE paths, not the saturation filter — \
         got {probs:?}"
    );
    assert!(probs.iter().any(|(_, w)| *w > 0.0), "placement must still pick a path");
}

/// An empty `active_paths()` is not a taper under the shipped law but a cliff
/// to the boot cap: `path_scaled_store_cap` returns `None` at `pipe_sum <= 0`
/// and the chain falls through to `store_boot_cap`.
///
/// That cliff is the negative feedback the legacy arm gets for free: the
/// moment every path is cwnd-saturated, the store cap drops ≥6× and admission
/// stops until the paths drain. Under `RWM_STORE_CAP_UNIFIED` the Σ ranges
/// over `live_paths()`, never empty while the transfer is up, so the empty
/// state has no consequence and persists. That is all U changes about the
/// gauge.
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

    // N = 1 (c1/sc2): the same cliff.
    let single = shipped_chain(a_fast * 5.0, 1); // legacy anchor over-read ×5
    assert_eq!(single, 832);
    assert!(single <= STORE_MAX, "the N = 1 law is bounded by RELIABLE_STORE_MAX");
    assert!(
        single as f64 / BOOT as f64 > 6.0,
        "the c1 cliff must be a ≥6× step, got {:.1}×",
        single as f64 / BOOT as f64
    );

    // The unified arm has no cliff: `live_paths()` is non-empty whenever the
    // transfer is up, so the Σ never reaches 0.
    assert!(cap_for(Arm::Unified, both as f64, a_fast + a_slow, 2, None) > BOOT);
}

/// The reproduced direction, bounded: at every dual cell the unified set
/// raises the `[SF]` zero-fraction and the mean store cap. The cell
/// specificity of the L1 result (c8 only) is not reproduced by this model;
/// do not read this test as evidence for it.
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

/// The bench's own reproducibility. `Scheduler` holds its paths in a
/// `HashMap`, so at the symmetric cell (bit-equal placement costs) the winner
/// would be a per-process random choice. The tie goes to the lowest path id,
/// which makes c7's numbers the same on every run and host.
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

/// Why the over-reading anchor cannot be the prop.
///
/// The suspect was that a ×5-class over-reading anchor props the cwnd floor
/// (`clamp_cwnd_with_anchor`), keeps `available() > 0`, and keeps fast cells
/// out of the empty-`active_paths()` state. But the same anchor is on both
/// sides of the loop:
///
///   * the cwnd floor is `ANCHOR_FLOOR_GAIN · anchor` — linear in the anchor,
///   * the store cap is `gain · N · Σ anchor` — also linear.
///
/// Saturation is decided by `store_cap` vs `Σ_paths cwnd`, and a common scale
/// `f` cancels in that ratio. The era cannot move the saturation state while
/// both terms are linear; only a non-homogeneous term can — the `N·knee`
/// ceiling (and `FLOOR`/`MAX_CWND`). Pinned on the real
/// `path_scaled_store_cap`: degree-1 homogeneity below the ceiling,
/// saturation at `N·knee` above it.
#[test]
fn store_cap_law_is_degree_one_in_the_anchor_until_the_knee_ceiling() {
    let sigma = C2.0 * C2.1 + C3.0 * C3.1; // c8's Σ = 203.2
    let n = 2usize;
    let ceiling = (n * KNEE) as f64; // 4096

    // Below the ceiling: cap(f·Σ) == f·cap(Σ) — the anchor era divides out.
    // (`ceil` gives at most a 1-symbol residue.)
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

    // Above it the law saturates — the only non-homogeneous term, and so the
    // only route by which an anchor era can change the saturation state.
    let huge = shipped_chain(1_000.0 * sigma, n) as f64;
    assert_eq!(huge, ceiling, "the N*knee ceiling must bind");
    let f_needed = ceiling / base;
    assert!(
        f_needed > 4.6,
        "at c8 the cap only reaches its ceiling past x{f_needed:.1}, i.e. ABOVE the wire's \
         measured legacy band (4.6-7.4) — so inside that band the era is a pure scale"
    );
}

/// The over-reading (legacy-era) anchor does not make the fast symmetric cell
/// immune: over the seed ensemble the legacy era leaves c7's shipped arm
/// strictly worse than the honest era, opposite to the prediction.
///
/// Ordinal with a margin on purpose: the absolute levels are mode draws from
/// a bistable loop, but the sign is arithmetic — the store-cap side of the
/// anchor (gain·N = 4× per path) outruns the cwnd side
/// (ANCHOR_FLOOR_GAIN = 0.85×).
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
        // Measurement discipline rule 1: the prop is real — the over-reading
        // anchor raises the cwnd floor by a wide margin. It does not buy
        // immunity, because the same anchor raises the admission the cwnd
        // has to absorb.
        assert!(
            legacy.mean_cwnd() > 1.5 * honest.mean_cwnd(),
            "seed {s}: the over-read anchor never propped cwnd, so this test proved nothing: \
             honest {:.0} vs over-read {:.0}",
            honest.mean_cwnd(),
            legacy.mean_cwnd()
        );
        // And the realized over-read must reach the wire's band, or the era
        // was not reached.
        assert!(
            legacy.overread() > 4.6,
            "seed {s}: realized over-read x{:.2} never reached the legacy band",
            legacy.overread()
        );
    }
}

/// The candidate successor is a pure deletion of the count multiplier: at the
/// unified set it is exactly `gain·Σ_live` under the same N·knee ceiling, so
/// it is bounded above by the shipped ×N law at every N ≥ 1 and equals it at
/// N = 1.
#[test]
fn pooled_unified_candidate_introduces_no_constant() {
    for geom in [vec![C2], vec![C2, C2], vec![C2, C3]] {
        let n = geom.len();
        let sum: f64 = geom.iter().map(|(r, t, _, _)| r * t).sum();
        let shipped = cap_for(Arm::Unified, sum, sum, n, None);
        let cand = cap_for(Arm::PooledUnified, sum, sum, n, None);
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
// Which refresh regime the wire is in
//
// The arithmetic says the wire's store cap is pinned at `N·knee`. Two readings
// confirm it:
//
// 1. The realized cap. `win=occ/cap`'s `cap` field is `dyn_store_cap`
//    whenever `plain_dyn_cap`, and L1 records its median per rep as
//    `occcap_p50`. Over 178 dual-cell reps from five sessions it reads exactly
//    4096 = 2·knee in 69/69 c7-A reps and 52/57 c8-A reps, with
//    `capboot_frac` (cap ≤ boot = 128) 0.0000 in every one.
//
// 2. The 7.5× "U-fold" is not a fold in the cap. `fold` is
//    `mean(AU zero%)/mean(A zero%)`, a ratio of the `[SF]` gauge, whose
//    `store_cap_sf_record(live, act)` runs on both arms and under U is
//    consumed by neither: the Σ ranges over `live`, so an empty
//    `active_paths()` cannot reach the cap. A pinned cap forbids nothing.
//
// What U does at c8 is smaller: it converts the ~36 % of refreshes the
// `active_paths()` filter leaves with one leg in the Σ (interior cap
// ≈2936–3102) and the ~4.6 % it leaves empty (boot cap 128) into the 4096
// ceiling — the median is unchanged, the mean moves ≈3520 → 4096.
// ═══════════════════════════════════════════════════════════════════════════

/// The wire's realized store cap at the dual cells, from the L1 per-rep
/// result rows: `occcap_p50` = median of `win=occ/cap`'s cap over the rep's
/// steady `[DIAG]` samples; `capboot_frac` = the share with cap ≤ 128.
struct WireCap {
    cell: &'static str,
    arm: &'static str,
    /// Reps that reported a cap at all.
    reps: usize,
    /// Of those, how many read a median cap of exactly `2·KNEE` = 4096.
    at_ceiling: usize,
    /// The worst `capboot_frac` over those reps.
    max_capboot: f64,
}

/// Five independent sessions, two seeds, pooled only for the count of reps
/// whose median cap is the ceiling — no goodput statistic is pooled (the
/// documented 2.3× same-config drift forbids that; a cap that reads the same
/// integer in every session does not care).
const WIRE_CAPS: &[WireCap] = &[
    WireCap { cell: "c7", arm: "A", reps: 69, at_ceiling: 69, max_capboot: 0.0 },
    WireCap { cell: "c7", arm: "AU", reps: 26, at_ceiling: 26, max_capboot: 0.0 },
    WireCap { cell: "c8", arm: "A", reps: 57, at_ceiling: 52, max_capboot: 0.0 },
    WireCap { cell: "c8", arm: "AU", reps: 26, at_ceiling: 26, max_capboot: 0.0 },
];

/// The pin threshold is path-count free, and it is `knee/gain`.
///
/// `clamp(gain·N·Σ, floor, N·knee)` saturates iff `gain·N·Σ ≥ N·knee` iff
/// `Σ ≥ knee/gain`. The `N` cancels, so "does the anchor still steer the
/// cap?" is a question about the anchor sum alone, and at the shipped
/// constants the answer flips at 1024 symbols.
#[test]
fn the_pin_threshold_on_sigma_is_knee_over_gain_and_is_path_count_free() {
    assert_eq!(SIGMA_PIN, 1024.0);
    for n in 2..=8usize {
        let ceiling = n * KNEE;
        // Just below: strictly interior, and the law is still degree-1.
        let below = shipped_chain(SIGMA_PIN * 0.99, n);
        assert!(below < ceiling, "N={n}: Sigma just under the threshold pinned at {below}");
        assert_eq!(below, (GAIN * n as f64 * SIGMA_PIN * 0.99).ceil() as usize);
        // At and above: pinned, and insensitive to the anchor.
        assert_eq!(shipped_chain(SIGMA_PIN, n), ceiling, "N={n}");
        assert_eq!(shipped_chain(SIGMA_PIN * 100.0, n), ceiling, "N={n}");
    }
}

/// N ≥ 3 coverage: the symmetric quad against the symmetric dual, and the
/// value-vs-ceiling distinction that hid a quadratic (paper §6.1).
///
/// The shipped law is `clamp(gain·N·Σ, floor, N·knee)`, and its two halves
/// scale differently in the path count. At a symmetric cell `Σ = N·A`, so:
///
///   * the value `gain·N·Σ = gain·A·N²` is quadratic — c7x4 gets 4× c7's pool
///     where the summed derivation `Σᵢ gain·anchorᵢ` asks for 2×;
///   * the ceiling `N·knee` is linear — c7x4's ceiling is exactly 2× c7's.
///
/// Which one a measurement reads depends only on whether `Σ ≥ knee/gain`, and
/// on the wire, where the legacy anchor over-reads ×4.6–7.4, every dual-cell
/// rep read the ceiling (`WIRE_CAPS`). So every measurement of the shipped law
/// reports the linear ceiling's ratio 2 and cannot see the ratio 4 underneath.
/// Both regimes are asserted here, on the same two geometries.
#[test]
fn the_shipped_cap_at_the_symmetric_quad_separates_a_linear_ceiling_from_a_quadratic_value() {
    // One c7 leg's honest anchor-BDP; the quad is the same leg, four times.
    let a = C2.0 * C2.1; // 83.2
    assert_eq!(c7x4().len(), 4, "the quad geometry must be N = 4");
    assert!(c7x4().iter().all(|s| *s == C2), "the quad must be c7's leg, verbatim");
    let sigma = |n: usize| n as f64 * a;

    // ── Regime 1: honest anchors ⇒ Σ < knee/gain ⇒ the value rules ────────
    assert!(sigma(4) < SIGMA_PIN, "the quad must be INTERIOR at honest anchors");
    let v2 = shipped_chain(sigma(2), 2);
    let v4 = shipped_chain(sigma(4), 4);
    assert_eq!(v2, 666, "c7 interior value = ceil(2·2·166.4)");
    assert_eq!(v4, 2_663, "c7x4 interior value = ceil(2·4·332.8)");
    // The clamp is provably inert on both, so this is a reading of the law.
    assert!(v2 < 2 * KNEE && v4 < 4 * KNEE, "a ceiling bound: {v2} / {v4}");
    let r_value = v4 as f64 / v2 as f64;
    assert!(
        (r_value - 4.0).abs() < 0.01,
        "N=4 vs N=2 value ratio is {r_value}, i.e. not the shipped ×N² — the summed \
         derivation would read 2.0"
    );

    // ── Regime 2: the over-read era ⇒ pinned ⇒ the ceiling rules ─────────
    // ×7.4 is the top of the measured legacy band
    // (`store_cap_law_is_degree_one_in_the_anchor_until_the_knee_ceiling`).
    const OVERREAD_HI: f64 = 7.4;
    let sig = |n: usize| OVERREAD_HI * sigma(n);
    assert!(sig(2) >= SIGMA_PIN && sig(4) >= SIGMA_PIN, "both must PIN in this era");
    let c2_ = shipped_chain(sig(2), 2);
    let c4_ = shipped_chain(sig(4), 4);
    assert_eq!(c2_, 2 * KNEE, "c7 pinned at N·knee");
    assert_eq!(c4_, 4 * KNEE, "c7x4 pinned at N·knee");
    assert_eq!(
        c4_ as f64 / c2_ as f64,
        2.0,
        "a PINNED cap can only ever report the ceiling's LINEAR ratio, whatever the \
         value underneath is doing — this is the reading the wire took"
    );

    // The two regimes disagree: the same pair of geometries reports 4 or 2
    // depending only on the anchor era.
    assert_ne!(r_value.round(), c4_ as f64 / c2_ as f64);
}

/// The quad's determinism, and that all four legs carry and warm.
///
/// A four-way symmetric cell ties in every place this bench's determinism
/// depends on at once — the placement objective's exact-cost tie
/// (`place_min_cost`), the worst-loss pick's 0.0-vs-0.0 tie
/// (`worst_loss_path`), and the per-path link seeding — and `Scheduler` holds
/// its paths in a `HashMap`, so any of them resolving by map order would make
/// c7x4's numbers a per-process draw.
///
/// All four legs carry within 3 % of each other, all four warm on the same
/// tick count, and the cell delivers 2.1× c7. The cold-start placement price
/// (an unmeasured leg priced at `DEFAULT_SRTT`/2 = 25 ms against a warm c2
/// leg's 4 ms) never binds here: every leg is unmeasured at once, so the first
/// admission burst ties all four at the seed price and the `in_flight` term
/// round-robins them; one sample later every leg is warm. The cold price can
/// only close into a fixed point on a late join to already-warm incumbents,
/// which no geometry here contains and which the scheduler bounds directly
/// (`a_late_joining_leg_is_locked_out_by_the_cold_price_and_admitted_without_it`).
#[test]
fn the_symmetric_quad_is_deterministic_and_all_four_legs_carry_and_warm() {
    let g = c7x4();
    let a = simulate(&g, Arm::Legacy, 4.0);
    let b = simulate(&g, Arm::Legacy, 4.0);
    assert_eq!((a.delivered, a.retx, a.ticks, a.zero, a.short), (b.delivered, b.retx, b.ticks, b.zero, b.short));
    assert_eq!(a.mean_cap.to_bits(), b.mean_cap.to_bits(), "mean cap is not reproducible");
    assert_eq!(a.sum_live, b.sum_live);
    assert_eq!(a.sum_active, b.sum_active);
    assert_eq!(a.delivered_p, b.delivered_p, "the per-leg split is not reproducible");

    // Measurement discipline rule 1: the dyn-cap refresh really sees four live
    // paths, since `n_live` is the multiplier the law-shape question is about.
    assert!(a.ticks > 0 && a.delivered > 0, "the quad never ran");
    assert_eq!(a.sum_live, 4 * a.ticks, "the refresh never saw four live paths");
    assert!(a.mean_cap <= (4 * KNEE) as f64, "the realized cap exceeded N·knee");

    // ── All four legs carry, and the split is even ────────────────────────
    // Absolute, not ordinal: with four identical `C2` legs an even split is
    // the placement objective's own prediction (paper §5.7 water-fills by
    // capacity, and the capacities are equal). The 10 % band is loose enough
    // for the four GE realizations to differ and tight enough that a gauge
    // truncation or a placement change that starves a leg fails here.
    for pid in 0..4 {
        assert!(
            a.delivered_p[pid] > 0,
            "leg {pid} carried nothing — either placement starved it or a \
             per-path gauge is truncated again (the `pid < 2` defect)"
        );
    }
    let lo = *a.delivered_p[..4].iter().min().unwrap() as f64;
    let hi = *a.delivered_p[..4].iter().max().unwrap() as f64;
    assert!(
        hi / lo < 1.10,
        "the symmetric quad's legs delivered {:?} — a >10% spread at a cell \
         whose four legs are the same spec is a placement defect, not noise",
        a.delivered_p
    );
    // N = 4 really moves N× the traffic: had the legs locked onto two, this
    // would read like c7's total.
    let c7_total = simulate(&[C2, C2], Arm::Legacy, 4.0).delivered as f64;
    assert!(
        a.delivered as f64 > 1.8 * c7_total,
        "the quad delivered {} against c7's {c7_total} — two legs' worth, i.e. \
         the lock-in this test used to assert would be REAL after all",
        a.delivered
    );

    // ── And all four warm ─────────────────────────────────────────────────
    // `bw_n[pid]` counts refresh ticks at which leg `pid` had both a BtlBw
    // estimate and a min-RTT — a warm anchor. Every leg must reach it as often
    // as any other, or "carries traffic" and "has an anchor" have come apart.
    for pid in 0..4 {
        assert!(a.bw_n[pid] > 0, "leg {pid} carried traffic but never warmed an anchor");
    }
    let wlo = *a.bw_n[..4].iter().min().unwrap();
    let whi = *a.bw_n[..4].iter().max().unwrap();
    assert_eq!(
        wlo, whi,
        "the four legs warmed at different tick counts ({:?}) — at a symmetric \
         cell every leg warms on the same refresh",
        a.bw_n
    );
}

/// The placement axis is inert at every geometry this bench has — a result,
/// not a null effect: the axis is proven live first (`simulate_place` asserts
/// `sched.cold_place()` took and refuses to run otherwise).
///
/// `RWM_COLD_PLACE` changes what an unmeasured leg's SRTT_i is worth in
/// `place_costs`. Every cell here starts with all legs unmeasured at once, so
/// the first admission burst places on all of them before any ack returns;
/// from the second tick on there is no unmeasured leg for the price to apply
/// to, and the two arms are the same law on the same inputs. Bit-identity is
/// the prediction, asserted as one — including at `c7x4` and at the duals.
#[test]
fn the_cold_start_placement_price_is_inert_wherever_every_leg_starts_cold() {
    for (name, g) in [
        ("c7  ", vec![C2, C2]),
        ("c8  ", vec![C2, C3]),
        ("c7x4", c7x4()),
    ] {
        let off = simulate_place(
            &g, Arm::Legacy, Feed::Honest, 4.0, 0, Acct::Off, Store::Unacked, Src::Bulk,
            Place::Shipped,
        );
        let on = simulate_place(
            &g, Arm::Legacy, Feed::Honest, 4.0, 0, Acct::Off, Store::Unacked, Src::Bulk,
            Place::ColdMeasured,
        );
        assert!(off.ticks > 0 && on.ticks > 0, "{name}: the geometry never ran");
        assert_eq!(
            (on.delivered, on.retx, on.ticks, on.zero, on.short),
            (off.delivered, off.retx, off.ticks, off.zero, off.short),
            "{name}: the cold-start price moved a cell where no leg is ever \
             cold-while-another-is-warm — it must be inert here"
        );
        assert_eq!(on.delivered_p, off.delivered_p, "{name}: the per-leg split moved");
        assert_eq!(on.bw_n, off.bw_n, "{name}: the per-leg warm-tick count moved");
        assert_eq!(
            on.mean_cap.to_bits(),
            off.mean_cap.to_bits(),
            "{name}: the store cap moved — placement and the cap law are \
             different layers and this axis must not reach the cap"
        );
    }
}

// ── The composed cap law (paper §10) ───────────────────────────────────────

/// The geometries the composed arm is scored at. `c1`-class is deliberately
/// absent: the L1 `c1` cell is a 1 Gbit single this bench has no
/// transcription of, and inventing one would manufacture a wire number. The
/// fast single below (`C2`, 100 Mbit / 10 ms) is the single-path class; the
/// 1 Gbit cell stays a VM question.
fn composed_geometries() -> Vec<(&'static str, Vec<Spec>)> {
    vec![
        ("sc2  single fast (c1-class)", vec![C2]),
        ("sc3  single slow           ", vec![C3]),
        ("c7   dual symmetric        ", vec![C2, C2]),
        ("c8   dual asym (rate + RTT)", vec![C2, C3]),
        ("c7x4 symmetric quad        ", c7x4()),
    ]
}

/// The composed law is the three-term law plus a brake, and nothing else.
///
/// The composed arm's pool is bit-identically the three-term arm's, because
/// the composed law is `net::three_term_store_cap` (paper §10 — one
/// implementation). A second cap expression for the composed arm would turn
/// the A/B from a brake measurement into a two-factor experiment.
///
/// So at a geometry where the brake never closes, the two arms agree in every
/// produced quantity. A single path whose cwnd is never saturated is that
/// geometry, and the precondition is asserted (`brake_closed` = 0 with
/// `brake_ticks` > 0 — armed, and null).
#[test]
fn the_composed_arm_is_the_three_term_pool_plus_a_brake_and_nothing_else() {
    // Pick the geometry by its measured brake behaviour, not by assumption.
    for geom in [vec![C2], vec![C3]] {
        let t = simulate(&geom, Arm::ThreeTermCell, 4.0);
        let c = simulate(&geom, Arm::Composed, 4.0);

        // Mechanism liveness (rule 1): the brake is armed at the composed arm
        // and never at the three-term one, or the comparison proves nothing.
        assert!(c.brake_ticks > 0, "the composed arm never armed its brake");
        assert_eq!(t.brake_ticks, 0, "the three-term arm must not arm the brake");

        if c.brake_closed == 0 {
            // The brake was armed and never bound ⇒ the arms are the same law,
            // and every produced quantity must agree exactly.
            assert_eq!(
                (c.delivered, c.retx, c.ticks, c.zero, c.short),
                (t.delivered, t.retx, t.ticks, t.zero, t.short),
                "the composed pool diverged from the three-term pool with the \
                 brake never binding — the composition grew a second law"
            );
            assert_eq!(
                c.mean_cap.to_bits(),
                t.mean_cap.to_bits(),
                "the composed cap is not bit-identical to the three-term cap"
            );
        }
    }
}

/// The composed cap lands interior at the duals — the law's prediction and
/// its stop condition, as an absolute pin (paper §10).
///
/// The predecessor operated at its ceiling (`occcap_p50` = exactly 4096 in 121
/// of 126 dual reps), so every measurement through it measured a constant.
/// The composed law's only remaining bound above is `WIN_STORE_MAX`, a memory
/// bound outside the law; a composed cap landing on it would mean the memory
/// bound has become the law — the predecessor's defect reproduced, and a stop
/// rather than a result. The other side is pinned too: above the floor.
#[test]
fn the_composed_cap_lands_interior_at_both_duals_and_neither_bound_is_the_law() {
    let mut caps: Vec<(&str, f64)> = Vec::new();
    for (name, geom) in [
        ("c7", vec![C2, C2]),
        ("c8", vec![C2, C3]),
        ("c7x4", c7x4()),
    ] {
        let r = simulate(&geom, Arm::Composed, 8.0);
        assert!(r.ticks > 0 && r.delivered > 0, "{name}: the geometry never ran");
        assert!(
            r.mean_cap < WIN_STORE_MAX as f64,
            "{name}: the composed cap reads {:.1} at the MEMORY bound {WIN_STORE_MAX} — \
             the resource limit has become the law (stop condition)",
            r.mean_cap
        );
        assert!(
            r.mean_cap > FLOOR as f64,
            "{name}: the composed cap reads {:.1} at the paroled floor {FLOOR} — \
             the law's operating range is a constant with no provenance",
            r.mean_cap
        );
        // And not the boot cliff either: `store_boot_cap` is where both cap
        // chains terminate, so a law that fell through would read here.
        assert!(
            (r.mean_cap - BOOT as f64).abs() > 1.0,
            "{name}: the composed cap reads {:.1}, i.e. the boot cap {BOOT} — \
             the law fell through to the terminal `else` at every refresh",
            r.mean_cap
        );
        caps.push((name, r.mean_cap));
    }

    // The law has an operating range (measurement discipline rule 18). The
    // predecessor's defect was that its ceiling was the only value the law
    // ever took, so every measurement through it measured a constant. A
    // successor reading the same number at three structurally different
    // geometries would reproduce the defect.
    //
    // Not asserted: a comparison against the shipped arm's mean cap. The
    // shipped mean at c7 is ~352, far below its 4096 ceiling, because the
    // `active_paths()` cliff drops it to `store_boot_cap` on many refreshes. A
    // mean-vs-mean ordinal between a pinned-with-cliffs law and an interior
    // one is meaningless; the shipped numbers are reported by
    // `sf_composed_cap_law_as_one_arm`.
    for i in 0..caps.len() {
        for j in (i + 1)..caps.len() {
            assert!(
                (caps[i].1 - caps[j].1).abs() > 1.0,
                "the composed cap reads {:.1} at {} and {:.1} at {} — a law that \
                 returns the same value at structurally different geometries is \
                 operating as a constant (MEASUREMENT DISCIPLINE 18)",
                caps[i].1,
                caps[i].0,
                caps[j].1,
                caps[j].0
            );
        }
    }
}

/// The composed law must not starve a leg of the quad.
///
/// The composed arm adds a per-path brake, and a brake can starve a leg by
/// holding the others saturated. This pins that the four-way split survives
/// the composition, against the shipped arm's own split as the baseline, with
/// the brake proven armed — a falsifiable null.
#[test]
fn the_composed_law_does_not_starve_a_leg_of_the_quad() {
    let g = c7x4();
    let base = simulate(&g, Arm::Legacy, 4.0);
    let c = simulate(&g, Arm::Composed, 4.0);

    // Determinism first — the quad ties in three places at once.
    let c2 = simulate(&g, Arm::Composed, 4.0);
    assert_eq!(
        (c.delivered, c.retx, c.ticks, c.zero, c.short),
        (c2.delivered, c2.retx, c2.ticks, c2.zero, c2.short),
        "the composed arm is not reproducible at the quad"
    );

    // Mechanism liveness: the brake was armed, so a null below is a result.
    assert!(c.brake_ticks > 0, "the composed arm never armed its brake at the quad");
    // The refresh saw four live paths — `n_live` is the axis.
    assert_eq!(c.sum_live, 4 * c.ticks, "the refresh never saw four live paths");

    // The baseline is measured, not assumed: the shipped arm spreads over all
    // four legs. If that stops being true the comparison below is
    // meaningless, so it is asserted first.
    for pid in 0..4 {
        assert!(
            base.delivered_p[pid] > 0,
            "the shipped arm starved leg {pid} — the baseline this test \
             compares to is gone"
        );
    }

    // The split survives the brake. Every leg still carries and warms, and no
    // leg's share collapses relative to the shipped arm's share of its own
    // total (a ratio, so the brake may move the total without that reading as
    // starvation).
    let (bt, ct) = (base.delivered as f64, c.delivered as f64);
    assert!(bt > 0.0 && ct > 0.0, "a zero-delivery arm at the quad");
    for pid in 0..4 {
        assert!(
            c.delivered_p[pid] > 0,
            "the composed law STARVED leg {pid} of the quad: {:?} against the \
             shipped arm's {:?}",
            c.delivered_p,
            base.delivered_p
        );
        assert!(
            c.bw_n[pid] > 0,
            "the composed law left leg {pid} without a warm anchor"
        );
        let (bs, cs) = (base.delivered_p[pid] as f64 / bt, c.delivered_p[pid] as f64 / ct);
        assert!(
            (cs - bs).abs() < 0.02,
            "leg {pid}'s share moved {bs:.4} → {cs:.4} under the composition — \
             the brake is redistributing placement, which is a different layer \
             from the cap law it is supposed to be"
        );
    }
}

/// The only three reachable regimes at a dual, and the two that are not.
///
/// At the batteries' resolved arms (every experiment gate off) the refresh
/// chain is `path_scaled_store_cap` → legacy `gain·Σ` → `store_boot_cap`:
///
///   * ceiling-pinned — `Σ ≥ 1024`;
///   * interior — `0 < Σ < 1024`, the only regime in which the anchor (and so
///     U's choice of path set) can move the cap;
///   * boot fallback (128) — `Σ == 0`, the summed set contributed no warm
///     anchor. Under U the set is `live_paths()`, non-empty whenever the
///     transfer runs, so this regime is unreachable on a U arm.
///
/// Not reachable at the duals, each by arithmetic:
///
///   * the `floor` clamp would need `Σ < floor/(gain·N)` — 2.5 symbols at
///     N = 2 under the derived floor of 10 (paper §6.1), far below the
///     smallest single-leg anchor the wire reported. The crossover is computed
///     from `FLOOR`, so this tracks the shipped constant.
///   * the `store_max` (1024) latch is the `n_live < 2` law, so it cannot be
///     seen with both legs up; it is what `sc2`/`c2r100` read (exactly 1024 in
///     every session).
#[test]
fn the_shipped_dual_refresh_has_exactly_three_reachable_regimes() {
    // Boot: the empty set, at any live count.
    assert_eq!(shipped_chain(0.0, 2), BOOT);
    // The floor is a real branch of the law, just unreachable here: it binds
    // for Σ at or below `floor/(gain·N)`. The crossover is computed from the
    // shipped floor, so what this asserts is the shape of the branch.
    let floor_sigma_n2 = FLOOR as f64 / (GAIN * 2.0);
    assert_eq!(shipped_chain(floor_sigma_n2 * 0.5, 2), FLOOR);
    assert_eq!(shipped_chain(floor_sigma_n2, 2), FLOOR);
    assert!(shipped_chain(floor_sigma_n2 + 1.0, 2) > FLOOR);
    // Interior: strictly between, degree-1 in the anchor.
    for sigma in [17.0, 100.0, 512.0, 1023.0] {
        let cap = shipped_chain(sigma, 2);
        assert!(cap > FLOOR && cap < 2 * KNEE, "Sigma {sigma}: cap {cap} not interior");
        assert_eq!(cap, (GAIN * 2.0 * sigma).ceil() as usize);
    }
    // The floor is unreachable from any warm single-leg anchor the wire
    // measured — the smallest is c7/p0's, and it clears the floor's Σ by more
    // than an order of magnitude.
    let smallest = ACK_ALL
        .iter()
        .skip(1) // c2r100 is the N = 1 cell, whose law is the store_max latch
        .map(|s| s.anchor_sym())
        .fold(f64::INFINITY, f64::min);
    // 16 symbols under the legacy bare floor; 2.5 under the derived one.
    let floor_sigma = FLOOR as f64 / (GAIN * 2.0);
    assert!(
        smallest > 40.0 * floor_sigma,
        "the floor clamp is within reach of a warm anchor: smallest leg {smallest:.0} \
         vs the floor's Sigma {floor_sigma:.0}"
    );
    // N = 1 is the store_max latch, not the pooled ceiling — what the single
    // cells measure (occcap_p50 = 1024 at both sc2 and c2r100).
    assert_eq!(shipped_chain(ACK_C2R100_P0.anchor_sym(), 1), STORE_MAX);
}

/// The wire's own anchors pin the law with both legs and free it with one.
///
/// Reconstructed by inverting `xanchor` — three measured columns multiplied,
/// nothing modelled — and not close to the threshold in either direction:
///
/// | cell | Σ both legs | ×`SIGMA_PIN` | one leg | ×`SIGMA_PIN` |
/// |---|---|---|---|---|
/// | c7 | 1635 | 1.60 | 712 / 924 | 0.70 / 0.90 |
/// | c8 | 1510 | 1.47 | 776 / 734 | 0.76 / 0.72 |
///
/// At both duals the shipped law is pinned whenever both legs are in the Σ
/// and interior whenever exactly one is — so the `[SF]` gauge's short-tick
/// fraction is the cap's regime mixture, and the realized median cap 4096 is
/// an arithmetic prediction (confirmed by 121/126 dual reps).
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

/// A Σ built from configured rates is 1.8× the wire's: multiplying the
/// measured `xanchor` by the cells' configured rate and RTT (10 400 / 2 000
/// sym/s at 8 / 60 ms) instead of the wire's measured `rate_lr` and RTprop
/// (6 948 / 1 376 sym/s at 8.4 / 38.6 ms).
///
/// The inflation lands on the other side of the pin threshold for the
/// single-leg Σ:
///
///   * on the configured Σ, dropping the slow leg still clamps (4423 > 4096),
///     so the shipped law could not express the U-fold;
///   * on the wire's Σ, dropping either leg does not clamp (2936 / 3102), so U
///     moves the cap on every short tick — ≈36 % of c8 refreshes.
///
/// `measured_over_read_saturates_the_knee_ceiling_and_collapses_the_u_fold`
/// stays as a true statement about the bench's own inputs; this test bounds
/// the gap between those inputs and the wire's.
#[test]
fn the_predecessors_sigma_is_inflated_by_configured_rates_not_the_wires_realized_ones() {
    let bench_fast = C2.0 * C2.1 * ACK_C8_P0.xanchor;
    let bench_slow = C3.0 * C3.1 * ACK_C8_P1.xanchor;
    let wire_fast = ACK_C8_P0.anchor_sym();
    let wire_slow = ACK_C8_P1.anchor_sym();
    // The configured-rate numbers, re-derived so they are visibly the same
    // quantity and not a different definition.
    assert!((bench_fast - 1105.7).abs() < 1.0 && (bench_slow - 1658.4).abs() < 1.0);
    let ratio = (bench_fast + bench_slow) / (wire_fast + wire_slow);
    assert!(
        ratio > 1.7 && ratio < 2.0,
        "the Sigma inflation moved: bench {:.0} vs wire {:.0} (x{ratio:.2})",
        bench_fast + bench_slow,
        wire_fast + wire_slow
    );
    // The flip: two opposite verdicts on the same question.
    assert_eq!(shipped_chain(bench_fast, 2), shipped_chain(bench_fast + bench_slow, 2));
    assert_ne!(
        shipped_chain(wire_fast, 2),
        shipped_chain(wire_fast + wire_slow, 2),
        "on the wire's own anchors, dropping a leg must CHANGE the cap — the whole \
         U mechanism at c8 is that change"
    );
    assert_ne!(shipped_chain(wire_slow, 2), shipped_chain(wire_fast + wire_slow, 2));
}

/// The wire's realized cap, as the L1 per-rep records report it — a
/// transcription gate, not a simulation: the numbers the verdict quotes say
/// "ceiling" rather than "boot". A re-measurement that disagrees changes this
/// row and re-scores the verdict.
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
    // The U arms are pinned in every rep, as "the Σ ranges over
    // live_paths()" predicts: the interior and boot regimes are unreachable
    // there, so there is no dispersion left.
    for w in WIRE_CAPS.iter().filter(|w| w.arm == "AU") {
        assert_eq!(w.at_ceiling, w.reps, "{}-AU is not uniformly pinned", w.cell);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// The latency-feedback source
//
// Hypothesis: the offered load is an inner TCP whose congestion control reacts
// to the tunnel's own inflated RTT. Two readings of existing evidence settle
// what the L1 wire's source is, before any model is built.
// ═══════════════════════════════════════════════════════════════════════════

/// The wire's offered load has no congestion control (measurement discipline
/// rule 1: asserted against the source text).
///
/// The offered load at every L1 arm is `raptorpath perf --client`, which
/// drives a memory-backed TUN — `MemTun` runs the real transport "without a
/// kernel TUN or an inner TCP stack" — and `perf.rs::run_object` is a bare
/// `for idx in 0..total` over `mem.feed.send(pkt)`. No window, cwnd, RTT
/// estimator, retransmit timer or loss signal exists on the app side; the
/// only backpressure is the bounded mpsc channel. So the offered load cannot
/// be reacting to the tunnel's latency. Adding an inner stack to `perf` fails
/// this test.
#[test]
fn the_wires_offered_load_has_no_congestion_control() {
    let perf = include_str!("../src/perf.rs");
    let tun = include_str!("../src/tun/mod.rs");

    assert!(
        tun.contains("without a kernel TUN or an inner TCP stack"),
        "tun/mod.rs no longer states that the perf vehicle has no inner TCP \
         stack — the premise of the latency-feedback attribution \
         must be re-read against whatever replaced it"
    );
    // The feed loop is open: one bare iteration per chunk, no gate but the
    // channel's own capacity.
    assert!(
        perf.contains("for idx in 0..total"),
        "perf.rs::run_object's open feed loop is gone; re-read the source model"
    );
    // Nothing that could implement a congestion response exists on the app
    // side. Each name is checked separately so a failure says which appeared.
    let lower = perf.to_ascii_lowercase();
    for banned in [
        "cwnd", "ssthresh", "congestion", "rto", "retransmit", "sack", "in_flight",
        "inflight", "rtt",
    ] {
        assert!(
            !lower.contains(banned),
            "perf.rs now mentions `{banned}` — the offered load may have grown \
             a congestion response, which would REOPEN the latency-feedback \
             hypothesis this section refuted"
        );
    }
}

/// The wire's sender-loop wait attribution, per cell and arm: each
/// sender-loop iteration's wall time is charged to the `select!` arm that woke
/// it, and every L1 per-rep summary carries the median over the rep's DIAG
/// windows for all eight buckets. Medians over reps; transcription only.
#[derive(Debug, Clone, Copy)]
struct WireWait {
    cell: &'static str,
    arm: &'static str,
    reps: usize,
    tun: f64,
    paused: f64,
    nack: f64,
    tail: f64,
}

const WIRE_WAIT: &[WireWait] = &[
    WireWait { cell: "c7", arm: "A", reps: 101, tun: 98.0, paused: 0.0, nack: 2.0, tail: 0.0 },
    WireWait { cell: "c7", arm: "AU", reps: 36, tun: 98.0, paused: 0.0, nack: 2.0, tail: 0.0 },
    WireWait { cell: "c8", arm: "A", reps: 77, tun: 34.0, paused: 6.0, nack: 31.0, tail: 1.0 },
    WireWait { cell: "c8", arm: "AU", reps: 37, tun: 26.5, paused: 2.0, nack: 29.0, tail: 1.0 },
    WireWait { cell: "sc2", arm: "A", reps: 77, tun: 29.0, paused: 40.0, nack: 31.0, tail: 1.0 },
    WireWait { cell: "sc3", arm: "A", reps: 20, tun: 7.0, paused: 75.0, nack: 16.0, tail: 1.0 },
    WireWait { cell: "c1", arm: "A", reps: 83, tun: 67.0, paused: 33.0, nack: 0.0, tail: 0.0 },
];

fn wire_wait(cell: &str, arm: &str) -> &'static WireWait {
    WIRE_WAIT
        .iter()
        .find(|w| w.cell == cell && w.arm == arm)
        .unwrap_or_else(|| panic!("no transcribed wait row for {cell}-{arm}"))
}

/// "At c7/c8 the sender is offered-load-bound (`wait_tun` 97.7 %,
/// `wait_paused` ~0)" is true at c7 and false at c8. At c8 the
/// productive-intake arm is a minority of the loop's wall (34 %) and the
/// largest bucket beside it is the gap-report arm at 31 % — the recovery
/// plane, not the source and not the store cap.
#[test]
fn the_wire_is_tun_bound_at_c7_and_recovery_bound_at_c8() {
    // Internal identity: no bucket is a percentage outside [0, 100].
    for w in WIRE_WAIT {
        for (n, v) in [("tun", w.tun), ("paused", w.paused), ("nack", w.nack), ("tail", w.tail)] {
            assert!(
                (0.0..=100.0).contains(&v),
                "{}-{} {n} = {v} is not a percentage",
                w.cell, w.arm
            );
        }
        assert!(w.reps >= 20, "{}-{}: n = {} is too thin to transcribe", w.cell, w.arm, w.reps);
    }
    // c7 is offered-load-bound and its store cap is inert, on both arms.
    for arm in ["A", "AU"] {
        let w = wire_wait("c7", arm);
        assert!(w.tun >= 95.0, "c7-{arm}: tun = {} — c7 is no longer tun-bound", w.tun);
        assert_eq!(w.paused, 0.0, "c7-{arm}: the store-cap arm is no longer exactly 0%");
        assert!(w.nack <= 5.0, "c7-{arm}: the recovery arm has grown to {}%", w.nack);
    }
    // c8 is not.
    for arm in ["A", "AU"] {
        let w = wire_wait("c8", arm);
        assert!(
            w.tun <= 40.0,
            "c8-{arm}: tun = {} — if the productive-intake arm really dominates \
             at c8 then the dispatch's premise stands and this section is wrong",
            w.tun
        );
        assert!(
            w.nack >= 25.0,
            "c8-{arm}: the gap-report arm is only {}% — the recovery plane is \
             not where c8's sender loop lives",
            w.nack
        );
        assert!(
            w.nack >= 0.7 * w.tun,
            "c8-{arm}: the recovery arm ({}%) no longer rivals the intake arm ({}%)",
            w.nack, w.tun
        );
    }
    // And the store cap is the brake at the single cells, which is why the
    // bench's `while store_len < cap` models sc2/sc3 and not the duals.
    assert!(wire_wait("sc2", "A").paused >= 30.0);
    assert!(wire_wait("sc3", "A").paused >= 60.0);
    assert!(wire_wait("c7", "A").paused < 1.0);
}

/// The c8 collapse mode has a sender-loop signature, and it is a perfect
/// separator.
///
/// Over the 131 c8 reps of the A/AU/AL/ALU arms that carry the gauge, sorted
/// slowest first, the 19 slowest all read `wait_tun` = 0 % and
/// `wait_paused` = 0 % — an unbroken prefix — against 5 such reps in the
/// remaining 112. All 13 reps below the battery's own 60 Mbit/s collapse
/// threshold are in it.
#[derive(Debug, Clone, Copy)]
struct C8Class {
    label: &'static str,
    n: usize,
    mbps: f64,
    seconds: f64,
    wait_tun: f64,
    wait_paused: f64,
    wait_nack: f64,
    wait_tail: f64,
    occ_p50: f64,
    retx: f64,
    tc_drop: f64,
    tc_pkts: f64,
    sf_ticks: f64,
}

const C8_COLLAPSE: C8Class = C8Class {
    label: "collapse (< 60 Mbit/s)",
    n: 13,
    mbps: 54.3,
    seconds: 3.70,
    wait_tun: 0.0,
    wait_paused: 0.0,
    wait_nack: 51.0,
    wait_tail: 4.0,
    occ_p50: 0.0,
    retx: 2120.0,
    tc_drop: 182.0,
    tc_pkts: 23935.0,
    sf_ticks: 324.5,
};
const C8_NORMAL: C8Class = C8Class {
    label: "normal",
    n: 118,
    mbps: 81.1,
    seconds: 2.49,
    wait_tun: 33.0,
    wait_paused: 5.0,
    wait_nack: 30.0,
    wait_tail: 1.0,
    occ_p50: 2372.0,
    retx: 1501.5,
    tc_drop: 171.0,
    tc_pkts: 23552.5,
    sf_ticks: 315.0,
};
/// The unbroken prefix of slowest reps carrying the signature, and the pool.
const C8_DEAD_PREFIX: usize = 19;
const C8_REPS: usize = 131;
/// How many of the remaining reps carry it.
const C8_DEAD_ELSEWHERE: usize = 5;

/// Each dual's own wire transfer duration — the horizon at which the bench's
/// anchor is the wire's (all reps under `CopaState::window_duration`).
const WIRE_HORIZON: &[(&str, f64)] = &[("c7", 9.23), ("c8", 2.44)];

fn wire_horizon(cell: &str) -> f64 {
    WIRE_HORIZON
        .iter()
        .find(|(c, _)| *c == cell)
        .map(|(_, h)| *h)
        .unwrap_or_else(|| panic!("no wire horizon for {cell}"))
}

// ── The source axis's pre-registration (measurement discipline rule 11) ───
//
// Every tolerance is fixed here, before the scored run, against wire numbers
// already published in this file.
//
// V1 — the standing queue, mean over legs, against the wire's `q_p50`.
const V_Q_WIRE_MS: &[(&str, f64)] = &[("c7", 76.0), ("c8", 338.0)];
const V_Q_LO: f64 = 0.5;
const V_Q_HI: f64 = 2.0;
// V2 — the regime, against the wire's own wait attribution.
const V_SRC_BOUND_C7_MIN: f64 = 80.0; // wire `wait_tun` = 98%
const V_CAP_BOUND_C7_MAX: f64 = 10.0; // wire `wait_paused` = 0%
const V_CAP_BOUND_C8_MAX: f64 = 15.0; // wire `wait_paused` = 6%
// V3 — occupancy over cap at the refresh tick.
const V_OCC_WIRE: &[(&str, f64)] = &[("c7", 0.31), ("c8", 0.55)];
const V_OCC_TOL: f64 = 0.20;
// V4 — goodput class against the `Src::Bulk` arm at the same cell.
const V_GP_LO: f64 = 0.5;
const V_GP_HI: f64 = 2.0;
/// The matrix's collapse threshold, transcribed: the battery's own 60 Mbit/s
/// over its own normal-class median of 81.1 Mbit/s.
const MATRIX_COLLAPSE_RATIO: f64 = 60.0 / 81.1;
/// Seeds per arm per cell (at least 5).
const MATRIX_SEEDS: u64 = 8;

/// The validation gate, scored — V1–V4 against the pre-registration, with its
/// stop rule.
///
/// The gate fails, at c7, on V2 and V3, and the failure is the result. A
/// Reno-class flow over a reliable tunnel has no congestion signal: the
/// tunnel hides every loss, so the flow never leaves slow start, its window
/// runs away, and it becomes the bulk source again. The offered load is not a
/// latency control — the same conclusion as for the wire, from the opposite
/// direction. Pinned so a change to the source model re-scores the gate.
#[test]
fn the_closed_loop_source_cannot_reproduce_the_a_arm_and_the_reason_is_the_reliable_tunnel() {
    let mut c7_cap_bound = f64::NAN;
    let mut c7_src_bound = f64::NAN;
    let mut c7_occ = f64::NAN;
    for (label, specs, acks) in measured_cells() {
        let cell = label.split_whitespace().next().expect("cell name");
        if cell == "sc2" {
            continue;
        }
        let h = wire_horizon(cell);
        let r = simulate_src(
            &specs, Arm::Legacy, Feed::Measured(acks), h, 0, Acct::Engine, Store::Span, Src::Reno,
        );
        let bulk = simulate_src(
            &specs, Arm::Legacy, Feed::Measured(acks), h, 0, Acct::Engine, Store::Span, Src::Bulk,
        );
        assert!(r.src_opps > 0, "{cell}: the source axis never sampled an admission tick");
        assert!(r.goodput_sym_s() > 0.0, "{cell}: the closed loop delivered nothing");

        // V4 — the closed loop must not have destroyed the transfer. It passes
        // at both duals, which makes the V2/V3 failure a statement about the
        // regime and not about a broken instrument.
        let gp = r.goodput_sym_s() / bulk.goodput_sym_s();
        assert!(
            (V_GP_LO..=V_GP_HI).contains(&gp),
            "{cell}: V4 — the closed loop's goodput is {gp:.2}x the bulk arm's, \
             outside the pre-registered [{V_GP_LO}, {V_GP_HI}]; the instrument \
             is broken and nothing else here can be read"
        );

        // Every criterion is produced at both duals and printed. Only c7's are
        // asserted below, because c7 is where the gate fails.
        let n = specs.len() as f64;
        let q: f64 = (0..specs.len()).map(|p| r.queue_ms(p)).sum::<f64>() / n;
        let occ = r.store_len_mean / r.mean_cap.max(1e-9);
        let (_, q_wire) = V_Q_WIRE_MS.iter().find(|(c, _)| *c == cell).expect("wire q");
        let (_, occ_wire) = V_OCC_WIRE.iter().find(|(c, _)| *c == cell).expect("wire occ");
        let vq = (V_Q_LO..=V_Q_HI).contains(&(q / q_wire));
        let vocc = (occ - occ_wire).abs() <= V_OCC_TOL;
        let vcap = r.cap_bound_pct()
            <= if cell == "c7" { V_CAP_BOUND_C7_MAX } else { V_CAP_BOUND_C8_MAX };
        println!(
            "[V-GATE] {cell:4} q {q:7.1} ms vs wire {q_wire:5.1} ({:4.2}x) V1 {} | \
             src-bound {:5.1}% cap-bound {:5.1}% (wire tun/paused {}) V2 {} | \
             occ/cap {occ:4.2} vs {occ_wire:4.2} V3 {} | gp {gp:4.2}x bulk V4 PASS | \
             inner w {:8.0} rto {}",
            q / q_wire,
            if vq { "PASS" } else { "FAIL" },
            r.src_bound_pct(),
            r.cap_bound_pct(),
            if cell == "c7" { "98/0" } else { "34/6" },
            if vcap && (cell != "c7" || r.src_bound_pct() >= V_SRC_BOUND_C7_MIN) {
                "PASS"
            } else {
                "FAIL"
            },
            if vocc { "PASS" } else { "FAIL" },
            r.src_w_mean,
            r.src_rto,
        );
        if cell == "c7" {
            c7_cap_bound = r.cap_bound_pct();
            c7_src_bound = r.src_bound_pct();
            c7_occ = occ;
        }
    }

    // V2a/V2b fail at c7, in the same direction: the model puts the store cap
    // in charge where the wire measures it never to fire.
    assert!(
        c7_src_bound < V_SRC_BOUND_C7_MIN,
        "c7: V2a now PASSES ({c7_src_bound:.1}% >= {V_SRC_BOUND_C7_MIN}%) — the \
         closed-loop source has become offered-load-bound at c7 and the STOP \
         RULE no longer fires. Re-score the whole matrix."
    );
    assert!(
        c7_cap_bound > V_CAP_BOUND_C7_MAX,
        "c7: V2b now PASSES ({c7_cap_bound:.1}% <= {V_CAP_BOUND_C7_MAX}%) — \
         re-score the gate"
    );
    // …and the mechanism: with no loss signal the window leaves the tunnel's
    // own cap far behind, so the flow offers strictly more than the store can
    // hold and the gate closes almost always.
    assert!(
        c7_cap_bound > 80.0,
        "c7: the store cap binds only {c7_cap_bound:.1}% of admission \
         opportunities — the runaway-window mechanism this section names is gone"
    );
    // V3 fails with it, on the same arithmetic: a full store is not an
    // occupancy of 0.31.
    let (_, w_occ) = V_OCC_WIRE.iter().find(|(c, _)| *c == "c7").expect("c7 row");
    assert!(
        (c7_occ - w_occ).abs() > V_OCC_TOL,
        "c7: V3 now PASSES (occ/cap {c7_occ:.2} vs the wire's {w_occ:.2}) — \
         re-score the gate"
    );
}

/// The matrix, run but not scored — the pre-registration's stop rule fired at
/// the validation gate, so every row is labelled UNSCORED.
///
/// {A, AU, U+3T, P} × {c7, c8} × 8 seeds, at each cell's own wire horizon.
#[test]
#[ignore = "component bench; run with --ignored --nocapture"]
fn sf_source_matrix_unscored() {
    println!(
        "\nTHE MATRIX — **UNSCORED**: the pre-registered STOP RULE fired at the\n\
         validation gate (V2a/V2b/V3 fail at c7). Numbers for the record only.\n\
         Src::Reno, Acct::Engine, Store::Span, the measured ack era, {MATRIX_SEEDS} seeds,\n\
         each cell at its own wire transfer duration. `collapse` = goodput below\n\
         {:.3}x the A arm's own median at that cell (the uniflip battery's 60/81.1).\n",
        MATRIX_COLLAPSE_RATIO
    );
    println!(
        "{:<26} {:<21} | {:>8} {:>8} {:>9} | {:>8} {:>7} | {:>8} {:>7} {:>5}",
        "cell", "arm", "gp med", "gp min", "collapse", "cap mean", "occ/cap", "innRTT", "inner w", "rto"
    );
    for (label, specs, acks) in measured_cells() {
        let cell = label.split_whitespace().next().expect("cell name");
        if cell == "sc2" {
            continue;
        }
        let h = wire_horizon(cell);
        let feed = Feed::Measured(acks);
        // The A arm's own median is the collapse denominator, per the
        // pre-registration, so it is computed first and reused.
        let mut a_gp: Vec<f64> = (0..MATRIX_SEEDS)
            .map(|s| {
                simulate_src(&specs, Arm::Legacy, feed, h, s, Acct::Engine, Store::Span, Src::Reno)
                    .goodput_sym_s()
            })
            .collect();
        a_gp.sort_by(f64::total_cmp);
        let a_med = a_gp[a_gp.len() / 2];
        let floor = MATRIX_COLLAPSE_RATIO * a_med;

        for (arm, name) in [
            (Arm::Legacy, "A    (shipped)"),
            (Arm::Unified, "AU   (deeper pool)"),
            (Arm::ThreeTermCell, "U+3T (three-term)"),
            (Arm::PooledUnified, "P    (pooled+unified)"),
        ] {
            let runs: Vec<Run> = (0..MATRIX_SEEDS)
                .map(|s| simulate_src(&specs, arm, feed, h, s, Acct::Engine, Store::Span, Src::Reno))
                .collect();
            let mut gp: Vec<f64> = runs.iter().map(|r| r.goodput_sym_s()).collect();
            gp.sort_by(f64::total_cmp);
            let collapse = gp.iter().filter(|g| **g < floor).count();
            let mean = |f: fn(&Run) -> f64| runs.iter().map(f).sum::<f64>() / runs.len() as f64;
            let cap = mean(|r| r.mean_cap);
            println!(
                "{:<26} {:<21} | {:>8.0} {:>8.0} {:>6}/{:<2} | {:>8.0} {:>7.2} | {:>8.1} {:>7.0} {:>5}   UNSCORED",
                label,
                name,
                gp[gp.len() / 2],
                gp[0],
                collapse,
                MATRIX_SEEDS,
                cap,
                mean(|r| r.store_len_mean) / cap.max(1e-9),
                mean(|r| r.src_rtt_ms()),
                mean(|r| r.src_w_mean),
                runs.iter().map(|r| r.src_rto).sum::<u64>(),
            );
        }
        println!();
    }
}

/// The source axis's smoke — one seed, both duals, both source arms, printed
/// so the instrument's behaviour is visible before anything is scored against
/// it. A readout, not a pin.
#[test]
#[ignore = "component bench; run with --ignored --nocapture"]
fn sf_source_axis_smoke() {
    println!(
        "\nTHE SOURCE AXIS — Reno-class inner flow (RFC 5681 AIMD + RFC 6298 RTO,\n\
         IW = 10 per RFC 6928) clocked on the tunnel's OWN delivered latency.\n\
         Horizon = each cell's own wire transfer duration. Acct::Engine, Store::Span,\n\
         the measured ack era. seed 0.\n"
    );
    println!(
        "{:<26} {:<19} | {:>7} {:>7} {:>6} | {:>7} {:>7} | {:>7} {:>7} {:>5} | {:>8}",
        "cell", "src", "occ", "cap", "o/c", "q ms", "rtp ms", "inner w", "innRTT", "rto", "gp sym/s"
    );
    for (label, specs, acks) in measured_cells().into_iter().filter(|(l, ..)| !l.starts_with("sc2"))
    {
        let cell = label.split_whitespace().next().expect("cell name");
        let h = wire_horizon(cell);
        for src in [Src::Bulk, Src::Reno] {
            let r = simulate_src(
                &specs, Arm::Legacy, Feed::Measured(acks), h, 0, Acct::Engine, Store::Span, src,
            );
            let n = specs.len() as f64;
            let q: f64 = (0..specs.len()).map(|p| r.queue_ms(p)).sum::<f64>() / n;
            let rtp: f64 = (0..specs.len()).map(|p| r.min_rtt_ms(p)).sum::<f64>() / n;
            println!(
                "{:<26} {:<19} | {:>7.0} {:>7.0} {:>6.2} | {:>7.1} {:>7.1} | {:>7.0} {:>7.1} {:>5} | {:>8.0}",
                label,
                src.label(),
                r.store_len_mean,
                r.mean_cap,
                r.store_len_mean / r.mean_cap.max(1e-9),
                q,
                rtp,
                r.src_w_mean,
                r.src_rtt_ms(),
                r.src_rto,
                r.goodput_sym_s()
            );
            if src == Src::Reno {
                println!(
                    "{:<26} {:<19} | offered-load-bound {:.1}%  store-cap-bound {:.1}%  \
                     (WIRE: c7 tun 98 / paused 0, c8 tun 34 / paused 6)",
                    "", "", r.src_bound_pct(), r.cap_bound_pct()
                );
            }
        }
        println!();
    }
}

/// The c8 collapse is appended dead wall, not a degraded transfer.
///
/// Three of the wire's columns say so together, none a goodput statistic:
///
///   * `tc_pkts` — packets the shaper counted, i.e. what reached the wire — is
///     1.02× between the classes: the collapse rep sends the same traffic.
///   * `sf_ticks` — the dyn-cap refresh count, which only increments inside
///     the emission path — is 1.03×: the emission work is the same.
///   * `seconds` is 1.49×. So ~30 % of a collapse rep's wall is time in which
///     the sender is neither taking source in (`wait_tun` = 0), nor blocked on
///     its store cap (`wait_paused` = 0), nor putting packets on the wire.
///
/// A store-sizing law cannot reach this: `wait_paused` = 0 in 13 of 13
/// collapse reps, so the gate a cap acts on is never closed during the
/// collapse.
#[test]
fn the_c8_collapse_is_appended_dead_wall_and_the_store_cap_gate_is_never_closed() {
    let (c, n) = (C8_COLLAPSE, C8_NORMAL);
    assert_eq!(c.n + n.n, C8_REPS, "the class counts must partition the pool");
    assert!(c.mbps < 60.0 && n.mbps >= 60.0, "the classes are the wrong side of the threshold");
    assert!(c.wait_nack > n.wait_nack, "{}: the recovery arm did not grow", c.label);
    assert!(c.wait_tail >= 2.0 * n.wait_tail, "{}: the tail-sweep arm did not grow", c.label);

    // (a) The separator. Both loop-attribution buckets are exactly zero in the
    // collapse class and neither is in the normal class.
    assert_eq!(c.wait_tun, 0.0, "{}: the intake arm is no longer dead", c.label);
    assert_eq!(c.wait_paused, 0.0, "{}: the store-cap arm is no longer dead", c.label);
    assert!(n.wait_tun >= 20.0 && n.wait_paused > 0.0, "the normal class lost its contrast");
    assert!(
        C8_DEAD_PREFIX >= 15 && C8_DEAD_ELSEWHERE <= C8_DEAD_PREFIX / 3,
        "the signature is no longer a clean prefix of the slowest reps \
         ({C8_DEAD_PREFIX} prefix vs {C8_DEAD_ELSEWHERE} elsewhere)"
    );

    // (b) The wire volume and the emission work are unchanged.
    let pkts = c.tc_pkts / n.tc_pkts;
    let ticks = c.sf_ticks / n.sf_ticks;
    let drop = c.tc_drop / n.tc_drop;
    assert!(
        (0.95..=1.10).contains(&pkts),
        "tc_pkts moved {pkts:.3}x between the classes — the collapse IS a \
         throughput loss after all and this section's mechanism is wrong"
    );
    assert!((0.95..=1.10).contains(&ticks), "sf_ticks moved {ticks:.3}x");
    assert!((0.90..=1.15).contains(&drop), "tc_drop moved {drop:.3}x — link loss is not equal");

    // (c) So the wall is where it went, and the residual is real.
    let wall = c.seconds / n.seconds;
    assert!(
        wall >= 1.35,
        "the collapse class is only {wall:.3}x the wall — the dead time is gone"
    );
    // The dead share, computed the way the tool computes it: hold the normal
    // class's refresh duty fixed and ask how much of the collapse rep's wall
    // its own refresh count can account for.
    let duty = n.sf_ticks / n.seconds;
    let dead = (c.seconds - c.sf_ticks / duty) / c.seconds;
    assert!(
        (0.20..0.50).contains(&dead),
        "the non-emission share of a collapse rep's wall is {:.1}%, not the \
         measured ~30%",
        dead * 100.0
    );
    // The normal class must have essentially none, or the statistic is
    // measuring the method rather than the mode.
    let dead_n = (n.seconds - n.sf_ticks / duty) / n.seconds;
    assert!(dead_n.abs() < 0.05, "the normal class shows {:.1}% dead wall too", dead_n * 100.0);

    // (d) And the store is empty while it happens: the median DIAG window of
    // a collapse rep holds nothing in the retention store.
    assert_eq!(c.occ_p50, 0.0, "the collapse class's median occupancy is no longer 0");
    assert!(n.occ_p50 > 1000.0, "the normal class's occupancy contrast is gone");

    // (e) The extra work that is there is recovery, and it is spurious: 1.41×
    // the retransmits on 1.06× the link drops.
    let spurious = (c.retx / n.retx) / (c.tc_drop / n.tc_drop);
    assert!(
        spurious >= 1.20,
        "retransmits are only {spurious:.2}x per unit of link loss — the extra \
         recovery traffic is explained by extra loss and is not spurious"
    );
}

/// The dead time's quantum, on the shipped laws and the wire's own SRTT.
///
/// Both timers that can end a c8 recovery stall are `2·SRTT` clamped to a
/// 100 ms ceiling — `net::tail_sweep_timeout_us` and `net::hole_nack_refresh`
/// (both `[25 ms, 100 ms]`). At c8 the wire's SRTT is `rtp_med + q_p50` =
/// 38 + 338 = 376 ms, so `2·SRTT` = 752 ms and both timers sit at their
/// ceiling, 7.5× below the round trip they are meant to be a multiple of.
/// Each recovery round costs 100 ms of wall whatever the path does, and
/// ~1.1 s of dead wall is ~11 of them. Arithmetic on shipped functions; not a
/// claim that changing the clamp would help.
#[test]
fn the_recovery_timers_are_clamp_bound_at_c8_and_free_at_the_single_cells() {
    use raptorpath::net::{hole_nack_refresh, tail_sweep_timeout_us};
    // The wire's measured SRTT per cell = rtp_med + q_p50, both from the same
    // per-rep records the WIRE_BRAKE table transcribes.
    let cells: &[(&str, u64, u64)] = &[("c7", 11, 76), ("c8", 38, 338), ("sc2", 13, 91)];
    let mut clamped = 0usize;
    for (cell, rtp_ms, q_ms) in cells {
        let srtt_us = (rtp_ms + q_ms) * 1000;
        let sweep = tail_sweep_timeout_us(srtt_us);
        let refresh = hole_nack_refresh(Some(Duration::from_micros(srtt_us))).as_micros() as u64;
        assert_eq!(
            sweep, refresh,
            "{cell}: the two recovery clocks have diverged; this section reads them as one"
        );
        if sweep == 100_000 {
            clamped += 1;
            let under = srtt_us as f64 * 2.0 / sweep as f64;
            if *cell == "c8" {
                assert!(
                    under >= 5.0,
                    "c8's 2*SRTT is only {under:.1}x the clamp — the clamp is no \
                     longer badly bound there and the dead-time arithmetic changes"
                );
            }
        }
    }
    assert_eq!(clamped, 3, "a transcribed cell stopped reaching the 100 ms clamp");
    // c1, the cell with no queue, is not clamp-bound at the ceiling — the
    // floor holds it instead, the control that says the ceiling reading is
    // about c8's queue and not about the law.
    assert_eq!(
        tail_sweep_timeout_us(9 * 1000),
        25_000,
        "c1's 2*SRTT = 18 ms should sit on the 25 ms FLOOR, not the ceiling"
    );
}

