//! δ-honest overload shedding, the completion feed, and the `[CHI]` /
//! `[SHEDH]` gauges.

use super::*;

// ── δ-honest overload shedding (paper §5.6) ──────────────────────────────
//
// Part of the unified machine's realtime semantics under `RWM_UNIFIED`;
// `RWM_UNIFIED_SHED=0` is the serializing control arm.
//
// At small δ overload must be shed, not serialized. A symbol is sheddable
// iff both (1) its projected delivery exceeds the deadline D(δ) — a
// retransmit fired at age > D arrives after the receiver's own δ-horizon
// give-up, and a hole held past D only serializes successors past their
// deadlines — and (2) its loss stays within the 1−ρ budget
// (`residual_loss_after_fec`, the ε̂·(1−P_fec) allowance the (δ, ρ, r) design
// already concedes). Beyond the budget the machine serializes (ρ wins over
// δ). The reliable-transfer contract (retain-until-acked, ρ = 1) is excluded
// by construction: the law is armed only on the EVICT path (`!reliable`).

/// Is the shed law armed at all? Realtime-EVICT under the unified machine
/// only — never the reliable (ρ = 1) contract.
pub(crate) fn shed_armed(unified_on: bool, reliable: bool, gate: bool) -> bool {
    unified_on && !reliable && gate
}

/// The δ deadline D in µs: min(b(hint)·RTprop, 2·RTprop) — the span law's
/// own D (paper §5.3), measured from the symbol's original send. b(Realtime)
/// = ½, so on the realtime path D = RTprop/2: a retransmit older than that
/// lands after the receiver's δ-horizon give-up (send + owd + D) — waste.
pub fn shed_deadline_us(b_hint: f64, rtprop_us: u64) -> u64 {
    ((b_hint.min(2.0) * rtprop_us as f64) as u64).min(2 * rtprop_us)
}

/// The per-decision shed admission: past-deadline AND within the ρ budget.
/// `budget_frac` = the derived 1−ρ (`residual_loss_after_fec`); the
/// cumulative shed count may never exceed budget_frac × the stream's
/// source count. Cold start (budget 0, no ε̂/r sample) sheds nothing.
pub fn shed_allowed(
    age_us: u64,
    deadline_us: u64,
    shed_total: u64,
    src_total: u64,
    budget_frac: f64,
) -> bool {
    deadline_us > 0 // no derived deadline yet ⇒ nothing is sheddable
        && age_us > deadline_us
        && ((shed_total + 1) as f64) <= budget_frac * (src_total as f64)
}

/// Receiver-side in-order hold for the window EVICT path. Default: 4×SRTT
/// clamped [60, 300] ms (two ARQ repair rounds). Under the shed law (unified
/// realtime, budget open): the δ-derived H = b·SRTT with b(Realtime) = ½ —
/// the reorder timeout is the δ dial (the EVICT in-order window path exists
/// only for the Realtime hint, so b = ½ structurally). When the receiver's
/// give-up budget (holes ≤ ε̂_recv × frontier — the loss-class bound) is
/// spent, the hold reverts to the clamped form: serialize, don't shed below ρ.
pub(crate) fn shed_recv_hold(srtt: Duration, shed_on: bool, budget_ok: bool) -> Duration {
    let h = if shed_on && budget_ok {
        SHEDH.evals.fetch_add(1, Ordering::Relaxed);
        SHEDH.dial.fetch_add(1, Ordering::Relaxed);
        srtt / 2
    } else {
        SHEDH.evals.fetch_add(1, Ordering::Relaxed);
        SHEDH.legacy.fetch_add(1, Ordering::Relaxed);
        let raw = srtt * 4;
        if raw < BLOCK_REORDER_MIN_HOLD {
            SHEDH.at_floor.fetch_add(1, Ordering::Relaxed);
        } else if raw > BLOCK_REORDER_MAX_HOLD {
            SHEDH.at_cap.fetch_add(1, Ordering::Relaxed);
        } else {
            SHEDH.interior.fetch_add(1, Ordering::Relaxed);
        }
        raw.clamp(BLOCK_REORDER_MIN_HOLD, BLOCK_REORDER_MAX_HOLD)
    };
    SHEDH.hold_us_sum.fetch_add(h.as_micros() as u64, Ordering::Relaxed);
    h
}

/// The completion feed — `RWM_COMPLETION_EXPOSURE`, absent by default
/// (paper §4.6).
///
/// The input the completion-exposure glide needs: how much of this transfer
/// is left. The production tunnel is an endless stream with no `T_rem`, so χ
/// stays 0 there (and with it δ_eff = ε̂ at the Bulk end). A driver that does
/// know the remaining bytes (the perf client, feeding a sized object)
/// publishes them here.
///
/// One `AtomicU64` and nothing else: the writer is the object feeder, the
/// reader is the rate site, and a stale read is a slightly stale χ — never a
/// correctness question. `Relaxed` for the same reason.
#[derive(Debug)]
pub struct CompletionFeed {
    remaining_bytes: AtomicU64,
}

impl Default for CompletionFeed {
    fn default() -> Self {
        Self::new()
    }
}

impl CompletionFeed {
    pub fn new() -> Self {
        Self { remaining_bytes: AtomicU64::new(0) }
    }
    /// The whole object is ahead of us: set at the start of a transfer.
    pub fn set_remaining(&self, bytes: u64) {
        self.remaining_bytes.store(bytes, Ordering::Relaxed);
    }
    /// One chunk handed to the engine: saturating, so a feeder that
    /// over-counts by a partial chunk cannot wrap the counter.
    pub fn consume(&self, bytes: u64) {
        let _ = self.remaining_bytes.fetch_update(
            Ordering::Relaxed,
            Ordering::Relaxed,
            |v| Some(v.saturating_sub(bytes)),
        );
    }
    /// The object is acked: nothing is left, and χ has no more work to do.
    pub fn clear(&self) {
        self.remaining_bytes.store(0, Ordering::Relaxed);
    }
    pub fn remaining_bytes(&self) -> u64 {
        self.remaining_bytes.load(Ordering::Relaxed)
    }
}

/// `[CHI]` — the completion-exposure gauge (paper §4.6).
///
/// `docs/measurement-discipline.md` rule 1: prove the mechanism under test
/// executes. An arm whose χ never left 0 ran the shipped machine under a
/// different label. `max` and `frac_gt_half` distinguish "the glide ramped"
/// from "the gate was set and nothing happened". Reported on the 1 s
/// cadence, cumulative, last line wins.
///
/// Two-sided: the line is printed on the control arm too (where it must read
/// `max=0.0000 n=0`), so gate-off is as mechanically assertable as gate-on.
pub(crate) struct ChiGauge {
    n: AtomicU64,
    /// max χ × 1e6, as a u64 so the whole gauge is lock-free.
    max_ppm: AtomicU64,
    /// Observations with χ > ½ — the region where δ_eff has actually left ε̂.
    gt_half: AtomicU64,
    sum_ppm: AtomicU64,
}

pub(crate) static CHI: ChiGauge = ChiGauge {
    n: AtomicU64::new(0),
    max_ppm: AtomicU64::new(0),
    gt_half: AtomicU64::new(0),
    sum_ppm: AtomicU64::new(0),
};

impl ChiGauge {
    pub(crate) fn observe(&self, chi: f64) {
        let ppm = (chi.clamp(0.0, 1.0) * 1e6) as u64;
        self.n.fetch_add(1, Ordering::Relaxed);
        self.sum_ppm.fetch_add(ppm, Ordering::Relaxed);
        self.max_ppm.fetch_max(ppm, Ordering::Relaxed);
        if chi > 0.5 {
            self.gt_half.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub(crate) fn chi_report_line() -> String {
    let n = CHI.n.load(Ordering::Relaxed);
    let mean = if n == 0 {
        "-".to_string()
    } else {
        format!("{:.4}", CHI.sum_ppm.load(Ordering::Relaxed) as f64 / n as f64 / 1e6)
    };
    format!(
        "[CHI] n={} max={:.4} frac_gt_half={:.4} mean={} rttvar_src=srtt_eighth",
        n,
        CHI.max_ppm.load(Ordering::Relaxed) as f64 / 1e6,
        if n == 0 {
            0.0
        } else {
            CHI.gt_half.load(Ordering::Relaxed) as f64 / n as f64
        },
        mean,
    )
}

/// `[SHEDH]` — the receiver hold's bind gauge (every clamp gets a
/// bind-fraction gauge, reported).
///
/// `shed_recv_hold` is two laws behind one signature. The clamped branch is
/// `(4·SRTT).clamp(60 ms, 300 ms)`, and 60, 300 and the 4 have no
/// provenance. A clamp that always binds turns its law into a constant: at a
/// 13 ms SRTT the 60 ms floor binds (4·SRTT ≈ 52 ms), and under an inflated
/// stalled SRTT the 300 ms cap binds.
///
/// Observation only. Nothing reads these counters; they are fed inside
/// `shed_recv_hold` itself rather than at its call sites, so every branch is
/// counted exactly once by construction.
///
/// `dial + legacy = evals` and `at_floor + at_cap + interior = legacy`, both
/// asserted by `shedh_partitions_every_evaluation`.
pub(crate) struct ShedHoldGauge {
    /// Every evaluation of the hold, on both branches.
    pub(crate) evals: AtomicU64,
    /// The δ-derived branch (`b·SRTT`, shed law armed and budget open).
    pub(crate) dial: AtomicU64,
    /// The clamped `4·SRTT` branch.
    pub(crate) legacy: AtomicU64,
    /// Legacy evaluations pinned at the 60 ms floor.
    pub(crate) at_floor: AtomicU64,
    /// Legacy evaluations pinned at the 300 ms cap.
    pub(crate) at_cap: AtomicU64,
    /// Legacy evaluations where `4·SRTT` decided the hold — the only regime
    /// in which there is a "4·SRTT law" to speak of.
    pub(crate) interior: AtomicU64,
    /// Σ of the holds returned, µs, so the gauge reports the mean hold beside
    /// the bind fractions.
    hold_us_sum: AtomicU64,
}

pub(crate) static SHEDH: ShedHoldGauge = ShedHoldGauge {
    evals: AtomicU64::new(0),
    dial: AtomicU64::new(0),
    legacy: AtomicU64::new(0),
    at_floor: AtomicU64::new(0),
    at_cap: AtomicU64::new(0),
    interior: AtomicU64::new(0),
    hold_us_sum: AtomicU64::new(0),
};

/// The `[SHEDH]` line. Cumulative, last line wins — the `[RFA]` convention,
/// because the harness SIGKILLs the server and a `Drop` never reaches its log.
/// `mean_us` is `-` when `n = 0`: a dash iff there is no datum, never a zero
/// standing in for one.
pub(crate) fn shedh_report_line() -> String {
    let g = &SHEDH;
    let (evals, legacy) = (
        g.evals.load(Ordering::Relaxed),
        g.legacy.load(Ordering::Relaxed),
    );
    let frac = |x: u64, d: u64| if d == 0 { 0.0 } else { x as f64 / d as f64 };
    let mean = if evals == 0 {
        "-".to_string()
    } else {
        format!("{}", g.hold_us_sum.load(Ordering::Relaxed) / evals)
    };
    format!(
        "[SHEDH] evals={} dial={} legacy={} dial_frac={:.4} floor_n={} cap_n={} \
         interior_n={} floor_frac={:.4} cap_frac={:.4} interior_frac={:.4} \
         mean_us={} floor_ms={} cap_ms={}",
        evals,
        g.dial.load(Ordering::Relaxed),
        legacy,
        frac(g.dial.load(Ordering::Relaxed), evals),
        g.at_floor.load(Ordering::Relaxed),
        g.at_cap.load(Ordering::Relaxed),
        g.interior.load(Ordering::Relaxed),
        frac(g.at_floor.load(Ordering::Relaxed), legacy),
        frac(g.at_cap.load(Ordering::Relaxed), legacy),
        frac(g.interior.load(Ordering::Relaxed), legacy),
        mean,
        BLOCK_REORDER_MIN_HOLD.as_millis(),
        BLOCK_REORDER_MAX_HOLD.as_millis(),
    )
}

/// Receiver give-up budget: holes given up so far vs ε̂_recv × frontier.
/// (The receiver owns no r/A*, so its bound is the loss class, not the FEC
/// residual; give-up is holes-only, which keeps the realized fraction in the
/// residual class anyway.)
pub(crate) fn shed_recv_budget_ok(holes_given_up: u64, frontier_seqs: u64, eps_recv: f64) -> bool {
    (holes_given_up as f64) < eps_recv * (frontier_seqs as f64)
}
