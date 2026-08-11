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
//! No wall clock, no sockets, no tokio, no netem: same numbers every run.
//!
//! Run:
//!   cargo test --test store_cap_sf_bench --release -- --ignored --nocapture

use std::sync::Arc;
use std::time::Duration;

use raptorpath::net::path_scaled_store_cap;
use raptorpath::scheduler::{MockClock, Scheduler};

// ── Shipped policy constants at the battery's arms (sender_policy::resolve) ──
const GAIN: f64 = 2.0;
const FLOOR: usize = 64;
const KNEE: usize = 2048; // RWM_STORE_PATH_POOL
const STORE_MAX: usize = 1024; // RELIABLE_STORE_MAX
const BOOT: usize = 128; // RWM_STORE_BOOT
const REFRESH_S: f64 = 0.005; // the dyn-cap refresh throttle

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
    /// Serialise one symbol. `Some((ack_time, rtt))` if it will be delivered,
    /// `None` if the GE process drops it (it still consumes the bottleneck).
    fn send(&mut self, now: f64) -> Option<(f64, f64)> {
        let dep = self.busy_until.max(now) + 1.0 / self.rate;
        self.busy_until = dep;
        self.bad = if self.bad {
            self.rng.f64() < self.persist
        } else {
            self.rng.f64() < self.to_bad
        };
        if self.bad {
            return None;
        }
        let ack = dep + self.rtprop;
        Some((ack, ack - now))
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
}

/// The REAL reliable-source placement objective (`Scheduler::place_costs` via
/// `place_probs_with_temperature`), taken at T → 0 — the strict-best-path limit
/// the scheduler exposes for exactly this purpose. Deterministic (the shipped
/// `place_symbol` draws a uniform), same candidate set (`p.active`, no
/// availability filter), same cost.
fn place_min_cost(sched: &Scheduler) -> u32 {
    sched
        .place_probs_with_temperature(false, &[], f64::MIN_POSITIVE)
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(pid, _)| pid)
        .unwrap_or(0)
}

/// Close the loop. `paths` is the cell geometry; `arm` selects the path set /
/// pooled ceiling; `horizon_s` is simulated seconds.
fn simulate(paths: &[Spec], arm: Arm, horizon_s: f64) -> Run {
    let tick = 0.000_25_f64; // 250 µs — 20 ticks per dyn-cap refresh
    let clock = Arc::new(MockClock::new());
    let mut sched = Scheduler::new(clock.clone());
    let mut links: Vec<Link> = Vec::new();
    for (i, spec) in paths.iter().enumerate() {
        sched.add_path(i as u32);
        links.push(Link::new(*spec, 0x5EED_0000 + i as u64 * 0x9E37_79B9));
    }

    // The retention store: admitted, not yet acked.
    let mut store: Vec<Sym> = Vec::new();
    let mut cap: usize = BOOT;
    let mut delivered: u64 = 0;
    let mut retx: u64 = 0;
    let mut next_refresh = 0.0_f64;
    let (mut ticks, mut zero, mut short, mut sum_live, mut sum_active) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut cap_sum: f64 = 0.0;

    let steps = (horizon_s / tick).round() as u64;
    for step in 1..=steps {
        let now = step as f64 * tick;
        clock.advance(Duration::from_secs_f64(tick));

        // ── ack/delivery half + the recovery plane ───────────────────────
        // Acked symbols leave the store and release their path's budget.
        // Dropped ones are retransmitted once RFC 9002's time threshold
        // (9/8·SRTT — the same `PLACE_SLACK_RECOV_PATIENCE` the placement
        // objective uses) has passed, and are RE-CHARGED to their new path.
        let acks: Vec<(u32, f64)> = store
            .iter()
            .filter(|s| matches!(s.ack_at, Some(t) if t <= now))
            .map(|s| (s.path, s.rtt))
            .collect();
        for (pid, rtt) in &acks {
            if let Some(p) = sched.path_mut(*pid) {
                p.record_rtt_sample(Duration::from_secs_f64(*rtt));
                p.release_in_flight(1);
                p.on_ack(1);
            }
        }
        delivered += acks.len() as u64;
        store.retain(|s| !matches!(s.ack_at, Some(t) if t <= now));
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
            if let Some(p) = sched.path_mut(store[i].path) {
                p.release_in_flight(1);
            }
            let pid = place_min_cost(&sched);
            if let Some(p) = sched.path_mut(pid) {
                p.charge_in_flight(1);
            }
            let r = links[pid as usize].send(now);
            store[i].path = pid;
            store[i].sent = now;
            store[i].ack_at = r.map(|(a, _)| a);
            store[i].rtt = r.map(|(_, rt)| rt).unwrap_or(0.0);
            retx += 1;
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
            let bdp_live = sum_over(&live);
            let bdp_set = if arm == Arm::Legacy { sum_over(&act) } else { bdp_live };
            cap = cap_for(arm, bdp_set, bdp_live, n_live);
            cap_sum += cap as f64;
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
            if let Some(p) = sched.path_mut(pid) {
                p.charge_in_flight(1);
            }
            let r = links[pid as usize].send(now);
            store.push(Sym {
                path: pid,
                sent: now,
                ack_at: r.map(|(a, _)| a),
                rtt: r.map(|(_, rt)| rt).unwrap_or(0.0),
            });
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
