//! The store law's gauge-only build (B-1; `docs/derivations/store-law.md`
//! R4.8, R5.6, R6.2, §8): raw window statistics and the law's counterfactual
//! update, PRINTED under `RWM_DIAG` and never applied.
//!
//! Nothing here is read by any engine decision. With `RWM_DIAG` off no state
//! in this module is constructed: the per-leg [`WinSampler`] is attached only
//! by `Scheduler::enable_slaw`, which `diag::report` calls, and the counters
//! are bumped only behind the caller's `diag` guard.
//!
//! Tags (one `readout!` line each, 250 ms window close, per live leg unless
//! noted):
//!
//! * `[WSTAT]`  per leg: the echo samples' raw statistics and quinn state;
//! * `[WSTORE]` store scope: release batches, emission bursts, ack wakes, a/W;
//! * `[SLAW]`   per leg: the law's inputs, terms, and the counterfactual R;
//! * `[SLAWC]`  store scope: the R4.8 counters (cumulative);
//! * `[CAPLOG]` per store-cap refresh whose inputs changed: (inputs, cap), so
//!   `docs/derivations/store-law/caplog_replay.py` can recompute every cap.
//!
//! **K and n₀ have no provenance (R6.2, 13.1).** K = [`K_GAUGE`] = 8 is a
//! GAUGE parameter only: the K raw brakes are printed so K can be chosen
//! offline. Nothing in the engine reads it.

use std::collections::HashMap;

/// The number of boot windows b̂₀⁺ is the maximum over. A GAUGE parameter, not
/// a law constant: R6.2 declares K = 8 (2 s) without provenance (13.1); the K
/// raw brakes are printed (`b0k=`) so a better K can be chosen offline.
pub const K_GAUGE: usize = 8;

/// Most echo samples a leg's window keeps for its quantiles. A declared
/// resource bound (a sort of this many `u32`, under `RWM_DIAG` only; the
/// measured cost is in `bench_close_23k`). Count, minimum and mean are exact
/// over ALL samples. When the bound binds the kept samples are a DETERMINISTIC
/// UNIFORM STRIDE over the whole window, never a prefix: the buffer holds every
/// `stride`-th sample (sample indices 0, s, 2s, ...); on filling, every other
/// kept sample is dropped and the stride doubles. So the quantiles are not
/// biased toward the window start. Its bind is printed: `nkq=` (quantile
/// sample count, between CAP/2 and CAP once bound) against `nk=` (all samples).
pub const WIN_SAMPLE_CAP: usize = 16_384;

// ── the per-leg window sampler ──────────────────────────────────────────

/// One closed window of one leg (µs; `None` where the quantity does not exist
/// yet).
#[derive(Clone, Debug, PartialEq)]
pub struct WinClose {
    /// Samples in the window (all of them) and how many fed the quantiles.
    pub nk: u64,
    pub nkq: u64,
    pub tmin: u32,
    pub p10: u32,
    pub p50: u32,
    pub p90: u32,
    pub mean: f64,
    /// Lifetime minimum of the echo series, this window included.
    pub life_min: u32,
    /// Copa's `min_rtt` at the close.
    pub m_hat: Option<u64>,
    /// This window's brake T̂min − m̂ (µs, signed).
    pub brake: Option<i64>,
    /// Windows with samples closed so far, this one included.
    pub k: usize,
    /// b̂₀⁺ candidate: the max brake over the first K windows, defined once K
    /// windows have closed. `b0run` is the same maximum while still filling.
    pub b0p: Option<i64>,
    pub b0run: Option<i64>,
    /// The K brakes and the K windows' sample counts, only at the window
    /// where the K-th brake arrived (otherwise `None`).
    pub b0k: Option<(Vec<i64>, Vec<u64>)>,
    /// `qsp_us` (P90 − P50 of the leg's last 256 echo samples) at the close:
    /// σ at load. `sigb` is its maximum over the first K closes (σ at boot).
    pub sig: Option<u64>,
    pub sigb: Option<u64>,
}

/// Observation-only state of one leg, fed from `record_rtt_sample`.
#[derive(Default, Debug)]
pub struct WinSampler {
    buf: Vec<u32>,
    n: u64,
    sum: u64,
    min: Option<u32>,
    life_min: Option<u32>,
    k: usize,
    brakes: Vec<i64>,
    knk: Vec<u64>,
    sigb: Option<u64>,
    /// Keep every `stride`-th sample of this window (0 reads as 1).
    stride: u64,
}

impl WinSampler {
    /// Feed one echo sample (µs). O(1), no allocation once `buf` has grown.
    #[inline]
    pub fn observe(&mut self, rtt_us: u32) {
        self.n += 1;
        self.sum += rtt_us as u64;
        self.min = Some(self.min.map_or(rtt_us, |m| m.min(rtt_us)));
        self.life_min = Some(self.life_min.map_or(rtt_us, |m| m.min(rtt_us)));
        let idx = self.n - 1;
        if self.stride == 0 {
            self.stride = 1;
        }
        if idx % self.stride == 0 {
            if self.buf.len() == WIN_SAMPLE_CAP {
                // Full: keep the even positions (indices that are multiples of
                // 2*stride) and double the stride.
                let mut w = 0;
                for i in (0..self.buf.len()).step_by(2) {
                    self.buf[w] = self.buf[i];
                    w += 1;
                }
                self.buf.truncate(w);
                self.stride *= 2;
            }
            if idx % self.stride == 0 {
                self.buf.push(rtt_us);
            }
        }
    }

    /// Samples fed since the last close.
    pub fn pending(&self) -> u64 {
        self.n
    }

    /// Lifetime minimum so far.
    pub fn life_min(&self) -> Option<u32> {
        self.life_min
    }

    /// Close the window. `None` for an empty window (a statistic of an empty
    /// set does not exist; the lifetime state is untouched). `m_hat_us` is
    /// Copa's `min_rtt` and `qsp_us` the leg's σ-at-load, both read by the
    /// caller at the close.
    pub fn close(&mut self, m_hat_us: Option<u64>, qsp_us: Option<u64>) -> Option<WinClose> {
        if self.n == 0 {
            return None;
        }
        self.buf.sort_unstable();
        let q = |p: f64| crate::monitor::quantile::nearest_rank(&self.buf, p);
        let (p10, p50, p90) = (q(0.10), q(0.50), q(0.90));
        let nk = self.n;
        let nkq = self.buf.len() as u64;
        let tmin = self.min.unwrap_or(0);
        let mean = self.sum as f64 / nk as f64;
        let life_min = self.life_min.unwrap_or(tmin);
        self.k += 1;
        let brake = m_hat_us.map(|m| tmin as i64 - m as i64);
        let mut b0k = None;
        if self.k <= K_GAUGE {
            if let Some(b) = brake {
                self.brakes.push(b);
                self.knk.push(nk);
            }
            if let Some(s) = qsp_us {
                self.sigb = Some(self.sigb.map_or(s, |x| x.max(s)));
            }
            if self.k == K_GAUGE {
                b0k = Some((self.brakes.clone(), self.knk.clone()));
            }
        }
        let b0run = self.brakes.iter().copied().max();
        let b0p = if self.k >= K_GAUGE { b0run } else { None };
        let out = WinClose {
            nk,
            nkq,
            tmin,
            p10,
            p50,
            p90,
            mean,
            life_min,
            m_hat: m_hat_us,
            brake,
            k: self.k,
            b0p,
            b0run,
            b0k,
            sig: qsp_us,
            sigb: if self.k >= K_GAUGE { self.sigb } else { None },
        };
        self.buf.clear();
        self.n = 0;
        self.sum = 0;
        self.min = None;
        self.stride = 1;
        Some(out)
    }
}

// ── the counterfactual R (pure; never applied) ──────────────────────────

/// One leg's inputs to [`slaw_r`], all in ms except the dimensionless weight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegIn {
    /// wᵢ: the leg's echo-sample count in the window.
    pub w: f64,
    /// T̂ᵢ, T̂minᵢ: median and minimum of the window's echo samples.
    pub t_hat: f64,
    pub t_min: f64,
    /// m̂ᵢ: Copa's `min_rtt`; ℓ̂ᵢ: the lifetime echo minimum.
    pub m_hat: f64,
    pub l_hat: f64,
    /// b̂₀ᵢ⁺ and σᵢ (the caller picks σ at load or at boot).
    pub b0: f64,
    pub sigma: f64,
}

/// What [`slaw_r`] returns: R, the legs that contributed, and per leg the
/// coupling term and the anchor-bind flag (`anc`).
#[derive(Clone, Debug, PartialEq)]
pub struct RResult {
    pub r: f64,
    pub used: usize,
    /// ½·max(0, maxⱼ(ℓ̂ⱼ + b̂₀ⱼ⁺) − (ℓ̂ᵢ + b̂₀ᵢ⁺)), per input leg.
    pub gap: Vec<f64>,
    /// m̂ᵢ > ℓ̂ᵢ + b̂₀ᵢ⁺, per input leg.
    pub anc: Vec<bool>,
}

/// The law's counterfactual update ratio, R6.2's line, per window.
///
/// ```text
///   R = Σᵢ wᵢ·( Fᵢ + σᵢ + q(δ)·ℓ̂ᵢ + Ŝᵢ ) / Σᵢ wᵢ·T̂ᵢ
///   Fᵢ = min(m̂ᵢ, ℓ̂ᵢ + b̂₀ᵢ⁺) + b̂₀ᵢ⁺ + ½·max(0, maxⱼ(ℓ̂ⱼ + b̂₀ⱼ⁺) − (ℓ̂ᵢ + b̂₀ᵢ⁺))
///   Ŝᵢ = T̂ᵢ − T̂minᵢ
/// ```
///
/// R6.2's identity, used by the test: `R − 1 = Σwᵢ(Fᵢ + σᵢ + q·ℓ̂ᵢ − T̂minᵢ) / ΣwᵢT̂ᵢ`.
/// Empty set (no leg with samples): R := 1 (store-law.md §3.1), `used = 0`.
/// A leg with `w = 0` drops out of both sums and of the max. It is a gauge:
/// no clamp, no n, no floor, and nothing reads it.
pub fn slaw_r(legs: &[LegIn], q: f64) -> RResult {
    let live: Vec<bool> = legs.iter().map(|l| l.w > 0.0).collect();
    let used = live.iter().filter(|&&b| b).count();
    let mut gap = vec![0.0; legs.len()];
    let mut anc = vec![false; legs.len()];
    if used == 0 {
        return RResult { r: 1.0, used, gap, anc };
    }
    let floor_all = legs
        .iter()
        .zip(&live)
        .filter(|(_, &l)| l)
        .map(|(l, _)| l.l_hat + l.b0)
        .fold(f64::NEG_INFINITY, f64::max);
    let (mut num, mut den) = (0.0, 0.0);
    for (i, l) in legs.iter().enumerate() {
        let own = l.l_hat + l.b0;
        gap[i] = 0.5 * (floor_all - own).max(0.0);
        anc[i] = l.m_hat > own;
        if !live[i] {
            continue;
        }
        let f = l.m_hat.min(own) + l.b0 + gap[i];
        let s = l.t_hat - l.t_min;
        num += l.w * (f + l.sigma + q * l.l_hat + s);
        den += l.w * l.t_hat;
    }
    RResult { r: num / den, used, gap, anc }
}

/// Counts crossings of R = 1 between consecutive defined windows (R6.2's
/// gauge, predicted 1 per row). A window with `used == 0` is skipped; R == 1
/// exactly counts as at-or-below.
#[derive(Default, Debug, Clone)]
pub struct RCross {
    prev_above: Option<bool>,
    pub crossings: u64,
    pub above: u64,
    pub below: u64,
}

impl RCross {
    pub fn update(&mut self, r: f64) {
        let above = r > 1.0;
        if above {
            self.above += 1;
        } else {
            self.below += 1;
        }
        if let Some(p) = self.prev_above {
            if p != above {
                self.crossings += 1;
            }
        }
        self.prev_above = Some(above);
    }
}

// ── counters (R4.8): process-global, bumped only behind `diag` ──────────

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// Gap reports built by `sack_to_gaps` and those truncated at
/// `MAX_NACK_GAPS` (more gaps remained than were kept): C02's visibility.
pub static GAP_REPORTS: AtomicU64 = AtomicU64::new(0);
pub static GAP_TRUNC: AtomicU64 = AtomicU64::new(0);
/// Gap reports the repair loop processed, and those on which it broke on
/// `cached_max_repairs` / the NACK budget (C09's per-report cap bind).
pub static C09_REPORTS: AtomicU64 = AtomicU64::new(0);
pub static C09_BIND: AtomicU64 = AtomicU64::new(0);

/// Count one gap report (call only with diag on).
pub fn note_gap_report(truncated: bool) {
    GAP_REPORTS.fetch_add(1, Relaxed);
    if truncated {
        GAP_TRUNC.fetch_add(1, Relaxed);
    }
}

/// W1 (repair-debt) accounting, micro-units (x1e6) so the sums are atomic
/// integers: debt accrued at emission (`repair_rate` per source symbol), debt
/// spent (one unit per correction slot of the `while repair_debt >= 1` loop),
/// and debt cancelled at the two cancellation lines, with what was ASKED
/// (`newly_acked * rate` / `nacked * rate`) beside what was actually removed
/// (the `.max(0.0)` floor makes them differ).
pub static DEBT_ACC: AtomicU64 = AtomicU64::new(0);
pub static DEBT_EM: AtomicU64 = AtomicU64::new(0);
pub static DEBT_RM_ACK: AtomicU64 = AtomicU64::new(0);
pub static DEBT_ASK_ACK: AtomicU64 = AtomicU64::new(0);
pub static DEBT_RM_NACK: AtomicU64 = AtomicU64::new(0);
pub static DEBT_ASK_NACK: AtomicU64 = AtomicU64::new(0);

fn micro(x: f64) -> u64 {
    (x.max(0.0) * 1e6) as u64
}

/// Debt accrued at one emission (call only with the gauge on).
pub fn note_debt_accrued(rate: f64) {
    DEBT_ACC.fetch_add(micro(rate), Relaxed);
}

/// One unit of debt spent on a correction slot (gauge on only).
pub fn note_debt_emitted() {
    DEBT_EM.fetch_add(1_000_000, Relaxed);
}

/// A cancellation line ran: `before`/`after` are `repair_debt` around it and
/// `asked` the unfloored reduction (gauge on only). `ack` picks the line.
pub fn note_debt_cancel(ack: bool, before: f64, after: f64, asked: f64) {
    let (rm, ask) = if ack { (&DEBT_RM_ACK, &DEBT_ASK_ACK) } else { (&DEBT_RM_NACK, &DEBT_ASK_NACK) };
    rm.fetch_add(micro(before - after), Relaxed);
    ask.fetch_add(micro(asked), Relaxed);
}

/// Count one processed repair report (call only with diag on).
pub fn note_c09_report() {
    C09_REPORTS.fetch_add(1, Relaxed);
}

/// Count one report whose loop broke on the per-report cap (diag only).
pub fn note_c09_bind() {
    C09_BIND.fetch_add(1, Relaxed);
}

// ── store-scope window state ────────────────────────────────────────────

/// Store-scope observation state, owned by `DiagState` as `Option<Box<_>>`:
/// `None` (nothing constructed) unless `RWM_DIAG` is on.
#[derive(Default, Debug)]
pub struct SlawState {
    /// Windows closed; the time of the previous close (µs).
    pub k: u64,
    pub last_close_us: u64,
    aw_sum: f64,
    aw_n: u64,
    aw_min: f64,
    rel: Vec<u32>,
    adv: Vec<u32>,
    debt_last: [u64; 6],
    qb: HashMap<u32, Vec<u64>>,
    last_aux_us: u64,
    ackw_last: u64,
    pub rc_load: RCross,
    pub rc_boot: RCross,
    qcol: HashMap<u32, (u64, u64)>,
    anc_bind: HashMap<u32, (u64, u64)>,
}

impl SlawState {
    pub fn new(now_us: u64) -> Self {
        Self { last_close_us: now_us, aw_min: f64::INFINITY, ..Default::default() }
    }

    /// One a/W sample (`store_len / effective_store_cap`), every loop pass.
    pub fn sample_aw(&mut self, a: usize, w: usize) {
        if w == 0 {
            return;
        }
        let x = a as f64 / w as f64;
        self.aw_sum += x;
        self.aw_n += 1;
        self.aw_min = self.aw_min.min(x);
    }

    /// One SACK-release event's size (symbols released).
    pub fn note_release(&mut self, n: usize) {
        if n > 0 {
            self.rel.push(n.min(u32::MAX as usize) as u32);
        }
    }

    /// One cumulative-ACK advance (`on_ack_advance`'s `newly_acked`, sources
    /// delivered in order by this advance): the P_fec oracle's m̄ distribution.
    pub fn note_adv(&mut self, newly_acked: u64) {
        if newly_acked > 0 {
            self.adv.push(newly_acked.min(u32::MAX as u64) as u32);
        }
    }

    /// One quinn backlog sample for a leg (`hand − tx`, datagrams); the
    /// caller rate-limits to [`AUX_PERIOD_US`].
    pub fn note_qb(&mut self, path: u32, backlog: u64) {
        self.qb.entry(path).or_default().push(backlog);
    }

    /// Whether the ≥ 5 ms auxiliary sampling is due; stamps the time if so.
    pub fn aux_due(&mut self, now_us: u64) -> bool {
        if now_us.saturating_sub(self.last_aux_us) >= AUX_PERIOD_US {
            self.last_aux_us = now_us;
            true
        } else {
            false
        }
    }
}

/// Auxiliary (quinn backlog) sampling period: the store-cap refresh cadence.
pub const AUX_PERIOD_US: u64 = 5_000;

fn q3(v: &mut [u32]) -> Option<(u32, u32, u32)> {
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    let q = |p: f64| crate::monitor::quantile::nearest_rank(v, p);
    Some((q(0.5), q(0.9), *v.last().unwrap()))
}

fn ms(us: u64) -> String {
    format!("{:.3}", us as f64 / 1000.0)
}
fn msi(us: i64) -> String {
    format!("{:.3}", us as f64 / 1000.0)
}
fn dash<T: std::fmt::Display>(o: Option<T>) -> String {
    o.map_or_else(|| "-".to_string(), |v| v.to_string())
}

/// Per-leg engine inputs read at the close (all engine quantities).
pub struct LegRead {
    pub id: u32,
    pub close: Option<WinClose>,
    pub qcwnd_bytes: u64,
    pub bdp_bytes: f64,
    pub btlbw_sym_s: f64,
}

/// Render and fold one window close. Pure over its arguments (the caller
/// does the engine reads), so a unit test drives it and the lines are the
/// same ones `diag::report` prints. Returns the lines in print order.
#[allow(clippy::too_many_arguments)]
pub fn render_window(
    s: &mut SlawState,
    legs: Vec<LegRead>,
    q: f64,
    t_s: f64,
    now_us: u64,
    store_len: usize,
    cap: usize,
    ackw_total: u64,
    emb: Vec<u32>,
) -> Vec<String> {
    let mut out = Vec::new();
    s.k += 1;
    let tw_ms = now_us.saturating_sub(s.last_close_us) as f64 / 1000.0;
    s.last_close_us = now_us;
    let aw_mean = if s.aw_n > 0 { s.aw_sum / s.aw_n as f64 } else { f64::NAN };
    let aw_min = if s.aw_n > 0 { s.aw_min } else { f64::NAN };
    let fmt_f = |x: f64| if x.is_nan() { "-".to_string() } else { format!("{x:.4}") };

    // R, twice: σ at load and σ at boot. Defined only when every leg that
    // has samples has completed its K boot windows (a partial b̂₀⁺ would
    // enter the coupling max); `used == 0` is the spec's R := 1.
    let sampled: Vec<&LegRead> = legs.iter().filter(|l| l.close.is_some()).collect();
    let ready = sampled
        .iter()
        .all(|l| l.close.as_ref().is_some_and(|c| c.b0p.is_some() && c.m_hat.is_some() && c.sigb.is_some()));
    let mk = |boot: bool| -> Vec<LegIn> {
        sampled
            .iter()
            .map(|l| {
                let c = l.close.as_ref().unwrap();
                LegIn {
                    w: c.nk as f64,
                    t_hat: c.p50 as f64 / 1000.0,
                    t_min: c.tmin as f64 / 1000.0,
                    m_hat: c.m_hat.unwrap_or(0) as f64 / 1000.0,
                    l_hat: c.life_min as f64 / 1000.0,
                    b0: c.b0p.unwrap_or(0) as f64 / 1000.0,
                    sigma: (if boot { c.sigb } else { c.sig }).unwrap_or(0) as f64 / 1000.0,
                }
            })
            .collect()
    };
    let (rl, rb) = if ready && !sampled.is_empty() {
        (Some(slaw_r(&mk(false), q)), Some(slaw_r(&mk(true), q)))
    } else {
        (None, None)
    };
    if let (Some(a), Some(b)) = (&rl, &rb) {
        if a.used > 0 {
            s.rc_load.update(a.r);
            s.rc_boot.update(b.r);
        }
    }

    // [WSTAT] and [SLAW], per leg.
    let mut si = 0usize;
    for l in &legs {
        let qbv = s.qb.remove(&l.id).unwrap_or_default();
        let mut qbs: Vec<u32> = qbv.iter().map(|&x| x.min(u32::MAX as u64) as u32).collect();
        let qb_last = qbv.last().copied();
        let (qb50, qbmax) = match q3(&mut qbs) {
            Some((p50, _, mx)) => (p50.to_string(), mx.to_string()),
            None => ("-".to_string(), "-".to_string()),
        };
        // The absolute quinn-collapse proxy (§56 Amendment 2): the registered
        // rule compares qcwnd with the CELL's BDP (a harness constant the
        // engine cannot know); the engine's own proxy is its Copa BDP anchor.
        let below = l.qcwnd_bytes > 0 && l.bdp_bytes > 0.0 && (l.qcwnd_bytes as f64) < l.bdp_bytes;
        if t_s >= 1.0 && l.qcwnd_bytes > 0 {
            let e = s.qcol.entry(l.id).or_default();
            e.1 += 1;
            e.0 += below as u64;
        }
        let (qcb, qct) = s.qcol.get(&l.id).copied().unwrap_or((0, 0));
        let qbt = match qb_last {
            Some(b) if l.btlbw_sym_s > 0.0 => format!("{:.3}", b as f64 / l.btlbw_sym_s * 1000.0),
            _ => "-".to_string(),
        };
        match &l.close {
            None => {
                out.push(format!(
                    "[WSTAT] t={t_s:.2} k={} p={} nk=0 qb50={qb50} qbmax={qbmax} qcwnd={} bdpb={:.0}",
                    s.k, l.id, l.qcwnd_bytes, l.bdp_bytes
                ));
                out.push(format!(
                    "[SLAW] t={t_s:.2} k={} p={} n={cap} W={store_len} nk=0 aw={} qb={} qbt={qbt} qcol={qcb}/{qct}",
                    s.k, l.id, fmt_f(aw_mean), dash(qb_last)
                ));
            }
            Some(c) => {
                out.push(format!(
                    "[WSTAT] t={t_s:.2} k={} p={} nk={} nkq={} tmin={} p10={} p50={} p90={} tmean={:.3} m={} l={} \
                     qb50={qb50} qbmax={qbmax} qcwnd={} bdpb={:.0}",
                    s.k, l.id, c.nk, c.nkq, ms(c.tmin as u64), ms(c.p10 as u64), ms(c.p50 as u64),
                    ms(c.p90 as u64), c.mean / 1000.0, dash(c.m_hat.map(ms)), ms(c.life_min as u64),
                    l.qcwnd_bytes, l.bdp_bytes
                ));
                let ri = if ready { Some(si) } else { None };
                let (gap, anc) = match (&rl, ri) {
                    (Some(r), Some(i)) => (Some(r.gap[i]), Some(r.anc[i])),
                    _ => (None, None),
                };
                if let Some(a) = anc {
                    let e = s.anc_bind.entry(l.id).or_default();
                    e.1 += 1;
                    e.0 += a as u64;
                }
                let (ab, at) = s.anc_bind.get(&l.id).copied().unwrap_or((0, 0));
                // §8's per-leg error: this window's brake − b̂₀⁺ − q·ℓ̂.
                let err = match (c.brake, c.b0p) {
                    (Some(b), Some(p)) => Some(format!(
                        "{:.3}",
                        (b - p) as f64 / 1000.0 - q * c.life_min as f64 / 1000.0
                    )),
                    _ => None,
                };
                let b0k = match &c.b0k {
                    Some((b, _)) => b.iter().map(|&x| msi(x)).collect::<Vec<_>>().join(","),
                    None => "-".to_string(),
                };
                let b0kn = match &c.b0k {
                    Some((_, n)) => n.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","),
                    None => "-".to_string(),
                };
                let rf = |r: &Option<RResult>| match r {
                    Some(x) => format!("{:.5}", x.r),
                    None => "-".to_string(),
                };
                out.push(format!(
                    "[SLAW] t={t_s:.2} k={} p={} n={cap} W={store_len} aw={} nk={} tmin={} tp50={} tmean={:.3} m={} l={} \
                     b0p={} b0run={} b0k={b0k} b0kn={b0kn} kw={} sig={} sigw={} sigb={} gap={} anc={} ancf={ab}/{at} err={} \
                     R={} Rb={} used={} xs={} xb={} ra={}/{} q={q:.6} qb={} qbt={qbt} qcol={qcb}/{qct}",
                    s.k, l.id, fmt_f(aw_mean), c.nk, ms(c.tmin as u64), ms(c.p50 as u64), c.mean / 1000.0,
                    dash(c.m_hat.map(ms)), ms(c.life_min as u64),
                    dash(c.b0p.map(msi)), dash(c.b0run.map(msi)), c.k,
                    dash(c.sig.map(ms)), ms(c.p90 as u64 - c.p50 as u64), dash(c.sigb.map(ms)),
                    dash(gap.map(|g| format!("{g:.3}"))), dash(anc.map(|a| a as u8)), dash(err),
                    rf(&rl), rf(&rb), dash(rl.as_ref().map(|r| r.used)),
                    s.rc_load.crossings, s.rc_boot.crossings, s.rc_load.above, s.rc_load.below,
                    dash(qb_last), 
                ));
                si += 1;
            }
        }
    }

    // W1: per-window debt flows (deltas of the cumulative atomics).
    let dn = [&DEBT_ACC, &DEBT_EM, &DEBT_RM_ACK, &DEBT_ASK_ACK, &DEBT_RM_NACK, &DEBT_ASK_NACK]
        .map(|a| a.load(Relaxed));
    let d: Vec<f64> = (0..6).map(|i| dn[i].saturating_sub(s.debt_last[i]) as f64 / 1e6).collect();
    s.debt_last = dn;
    let share = if d[0] > 0.0 { format!("{:.4}", d[1] / d[0]) } else { "-".to_string() };
    let debt_s = format!(
        "dbt_acc={:.2} dbt_em={:.0} dbt_share={share} dbt_rm_ack={:.2} dbt_ask_ack={:.2} dbt_rm_nack={:.2} dbt_ask_nack={:.2}",
        d[0], d[1], d[2], d[3], d[4], d[5]
    );

    // [WSTORE]: store scope, so the parser cannot double-count per-leg lines.
    let mut rel = std::mem::take(&mut s.rel);
    let nrel = rel.len();
    let (r50, r90, rmx) = q3(&mut rel).map_or(("-".into(), "-".into(), "-".into()), |(a, b, c)| {
        (a.to_string(), b.to_string(), c.to_string())
    });
    let mut adv = std::mem::take(&mut s.adv);
    let nadv = adv.len();
    let (a50, a90, amx) = q3(&mut adv).map_or(("-".into(), "-".into(), "-".into()), |(a, b, c)| {
        (a.to_string(), b.to_string(), c.to_string())
    });
    let mut emb = emb;
    let nemb = emb.len();
    let (e50, e90, emx) = q3(&mut emb).map_or(("-".into(), "-".into(), "-".into()), |(a, b, c)| {
        (a.to_string(), b.to_string(), c.to_string())
    });
    out.push(format!(
        "[WSTORE] t={t_s:.2} k={} tw={tw_ms:.1} n={cap} W={store_len} aw={} awmin={} relb={nrel}/{r50}/{r90}/{rmx} \
         emb={nemb}/{e50}/{e90}/{emx} adv={nadv}/{a50}/{a90}/{amx} ackw={} {debt_s}",
        s.k,
        fmt_f(aw_mean),
        fmt_f(aw_min),
        ackw_total.saturating_sub(s.ackw_last),
    ));
    s.ackw_last = ackw_total;
    s.aw_sum = 0.0;
    s.aw_n = 0;
    s.aw_min = f64::INFINITY;

    // [SLAWC]: the R4.8 counters, cumulative.
    let (gr, gt) = (GAP_REPORTS.load(Relaxed), GAP_TRUNC.load(Relaxed));
    let (cr, cb) = (C09_REPORTS.load(Relaxed), C09_BIND.load(Relaxed));
    let frac = |a: u64, b: u64| if b == 0 { "-".to_string() } else { format!("{:.4}", a as f64 / b as f64) };
    out.push(format!(
        "[SLAWC] t={t_s:.2} k={} gaprep={gr} gaptrunc={gt} gaptrunc_frac={} c09rep={cr} c09bind={cb} c09_frac={}",
        s.k, frac(gt, gr), frac(cb, cr)
    ));
    out
}

// ── the store-cap input log ─────────────────────────────────────────────

/// One store-cap refresh's inputs and result, as `[CAPLOG]` prints them.
#[derive(Clone, Debug, PartialEq)]
pub struct CapRec {
    /// `copa` (Σ cwnd seat) or `plain` (Σ anchor-BDP seat).
    pub seat: &'static str,
    /// `pooled` (`pooled_store_cap`), `chain` (the `RWM_STORE_PATHS=0` clamp),
    /// `boot` (no base yet) or `tt` (three-term law: not replayable here).
    pub br: &'static str,
    pub on: bool,
    pub sum_cap: bool,
    pub delta_cap: bool,
    pub b_hint: f64,
    pub n_live: usize,
    pub psum: f64,
    pub gain: f64,
    pub floor: usize,
    pub pool: usize,
    pub smax: usize,
    pub boot: usize,
    pub cap: usize,
}

/// The `[CAPLOG]` recorder, `Some` in `StoreCapState` only under `RWM_DIAG`.
/// Logs a refresh only when its inputs or result differ from the last logged
/// one, and counts the identical refreshes it skipped (`rep=`): the inputs
/// determine the cap, so the log is lossless.
#[derive(Default, Debug)]
pub struct CapLog {
    last: Option<CapRec>,
    pub idx: u64,
    pub refreshes: u64,
    rep: u64,
    /// Tests capture lines here instead of printing.
    pub sink: Option<Vec<String>>,
}

impl CapLog {
    pub fn line(&self, t_us: u64, r: &CapRec, rep: u64) -> String {
        format!(
            "[CAPLOG] i={} t={t_us} seat={} br={} on={} sum={} dc={} b={} q={} n={} psum={} gain={} floor={} pool={} \
             smax={} boot={} cap={} rep={rep}",
            self.idx,
            r.seat,
            r.br,
            r.on as u8,
            r.sum_cap as u8,
            r.delta_cap as u8,
            r.b_hint,
            super::codel_setpoint_q(r.b_hint),
            r.n_live,
            r.psum,
            r.gain,
            r.floor,
            r.pool,
            r.smax,
            r.boot,
            r.cap
        )
    }

    pub fn record(&mut self, t_us: u64, r: CapRec) {
        self.refreshes += 1;
        if self.last.as_ref() == Some(&r) {
            self.rep += 1;
            return;
        }
        self.idx += 1;
        let l = self.line(t_us, &r, self.rep);
        self.rep = 0;
        self.last = Some(r);
        match self.sink.as_mut() {
            Some(v) => v.push(l),
            None => crate::readout!("{l}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leg(w: f64, t: f64, tmin: f64, m: f64, l: f64, b0: f64, sig: f64) -> LegIn {
        LegIn { w, t_hat: t, t_min: tmin, m_hat: m, l_hat: l, b0, sigma: sig }
    }

    /// R at hand-computed anchors, absolute values.
    ///
    /// N = 1: w=100, T̂=20, T̂min=18, m̂=10, ℓ̂=9, b̂₀=2, σ=1.5, q=0.1.
    ///   own = ℓ̂+b̂₀ = 11; F = min(10, 11) + 2 + ½·max(0, 11−11) = 12
    ///   num = 100·(12 + 1.5 + 0.9 + 2) = 1640 ; den = 100·20 = 2000 ; R = 0.82.
    ///
    /// N = 2: leg A as above (w=100); leg B: w=50, T̂=40, T̂min=36, m̂=30,
    ///   ℓ̂=25, b̂₀=3, σ=2.0.
    ///   ownB = 28, floor_all = max(11, 28) = 28
    ///   A: gap = ½·(28−11) = 8.5; F_A = min(10,11)+2+8.5 = 20.5
    ///      term_A = 100·(20.5 + 1.5 + 0.9 + 2) = 2490
    ///   B: gap = 0; F_B = min(30,28)+3+0 = 31 ; Ŝ_B = 4
    ///      term_B = 50·(31 + 2.0 + 2.5 + 4) = 1975
    ///   den = 2000 + 50·40 = 4000 ; R = 4465/4000 = 1.11625.
    #[test]
    fn r_pinned_at_hand_computed_anchors() {
        let a = leg(100.0, 20.0, 18.0, 10.0, 9.0, 2.0, 1.5);
        let r1 = slaw_r(&[a], 0.1);
        assert_eq!(r1.used, 1);
        assert!((r1.r - 0.82).abs() < 1e-12, "{}", r1.r);
        assert_eq!(r1.gap, vec![0.0]);
        assert_eq!(r1.anc, vec![false]);

        let b = leg(50.0, 40.0, 36.0, 30.0, 25.0, 3.0, 2.0);
        let r2 = slaw_r(&[a, b], 0.1);
        assert!((r2.r - 1.11625).abs() < 1e-12, "{}", r2.r);
        assert_eq!(r2.gap, vec![8.5, 0.0]);
        // anchor bind: m̂ > ℓ̂+b̂₀ ?  A: 10 > 11 no.  B: 30 > 28 yes.
        assert_eq!(r2.anc, vec![false, true]);
    }

    /// R6.2's identity R − 1 = Σw(F+σ+qℓ−T̂min)/ΣwT̂, with F re-derived
    /// independently here, and the empty-set rule.
    #[test]
    fn r_identity_and_empty_set() {
        let legs = [
            leg(100.0, 20.0, 18.0, 10.0, 9.0, 2.0, 1.5),
            leg(50.0, 40.0, 36.0, 30.0, 25.0, 3.0, 2.0),
        ];
        let q = 0.0667;
        let fl = [20.5, 31.0];
        let num: f64 = legs.iter().zip(fl).map(|(l, f)| l.w * (f + l.sigma + q * l.l_hat - l.t_min)).sum();
        let den: f64 = legs.iter().map(|l| l.w * l.t_hat).sum();
        let r = slaw_r(&legs, q);
        assert!((r.r - 1.0 - num / den).abs() < 1e-12);
        let e = slaw_r(&[leg(0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0)], 0.1);
        assert_eq!((e.r, e.used), (1.0, 0));
        assert_eq!(slaw_r(&[], 0.1).r, 1.0);
        // A w = 0 leg drops out of the sums AND of the coupling max.
        let with_idle = slaw_r(&[legs[0], leg(0.0, 9.0, 9.0, 9.0, 99.0, 9.0, 9.0)], 0.1);
        assert!((with_idle.r - slaw_r(&[legs[0]], 0.1).r).abs() < 1e-12);
    }

    /// Known-answer R on the §56 maps (store-law.md R3.2, R5.5, R6.2;
    /// `docs/derivations/store-law/s56cls.py` CURVE / B0X). The derivation
    /// tabulates TARGETS and the W where the cell's arm-median queue curve q(W)
    /// meets them, not R itself, so R is rebuilt from the same inputs: a leg
    /// at no queue has T̂min = ℓ̂ + q(W), m̂ = ℓ̂, and R = 1 exactly when
    /// q(W) = b̂₀⁺ + σ + q(δ)·ℓ̂ (+ ½Δ at c8), the derivation's target. The
    /// test sweeps W over the cell's curve, finds the R = 1 crossing, and
    /// compares it with (a) the same curve read at the hand-added target and
    /// (b) the R3.2 table's stated W range. It also asserts the R6.2
    /// prediction: ONE crossing per row.
    ///
    /// Inputs per cell, all from the derivation: c2 b̂₀ 0.41 (ledger median),
    /// σ 1.8 (R3.2), ℓ̂ 10 ms (q·ℓ̂ 0.5→1.0); c8 leg 0 b̂₀ 1.0, σ 2.5, ℓ̂ 14 ms,
    /// ½Δ = 14.75 (the coupling term, here supplied by a second leg of
    /// negligible weight that sets the max); c1s b̂₀ 0.12, σ 0.5, ℓ̂ 2.5 ms.
    #[test]
    fn known_answer_r_on_the_s56_maps() {
        let curves: [(&str, &[(f64, f64)]); 3] = [
            ("c2", &[(128., 0.0), (160., 0.0), (192., 3.0), (256., 9.0), (384., 22.0), (2048., 192.0)]),
            ("c8", &[(160., 0.0), (224., 3.9), (320., 9.1), (512., 21.9), (768., 41.7), (1280., 84.2)]),
            ("c1s", &[(384., 0.0), (512., 1.0), (768., 3.0), (1024., 5.5), (2048., 15.0)]),
        ];
        let at = |c: &[(f64, f64)], w: f64| -> f64 {
            for p in c.windows(2) {
                if p[0].0 <= w && w <= p[1].0 {
                    return p[0].1 + (w - p[0].0) / (p[1].0 - p[0].0) * (p[1].1 - p[0].1);
                }
            }
            c.last().unwrap().1
        };
        // (cell, b0, sigma, l, extra half-gap, R3.2 W range at Realtime..Bulk)
        let cells = [
            ("c2", 0.41, 1.8, 10.0, 0.0, (187.0, 196.0)),
            ("c8", 1.0, 2.5, 14.0, 14.75, (460.0, 480.0)),
            ("c1s", 0.12, 0.5, 2.5, 0.0, (485.0, 500.0)),
        ];
        for (name, b0, sigma, l, half_gap, (lo_w, hi_w)) in cells {
            let curve = curves.iter().find(|c| c.0 == name).unwrap().1;
            for (hint_q, bulk) in [(0.05, false), (0.10, true)] {
                let r_at = |w: f64| -> f64 {
                    let tmin = l + at(curve, w);
                    let leg0 = LegIn { w: 1.0, t_hat: tmin + sigma, t_min: tmin, m_hat: l, l_hat: l, b0, sigma };
                    if half_gap > 0.0 {
                        // Leg 1 (weight 1e-9) only sets the coupling max so that
                        // leg 0's gap = ½Δ; its own terms are negligible.
                        let l1 = 2.0 * half_gap + l + b0 - b0;
                        let leg1 = LegIn { w: 1e-9, t_hat: 50.0, t_min: 49.0, m_hat: l1, l_hat: l1, b0, sigma: 0.0 };
                        slaw_r(&[leg0, leg1], hint_q).r
                    } else {
                        slaw_r(&[leg0], hint_q).r
                    }
                };
                // Bisect R(W) = 1 on the curve's range (R falls as W rises).
                let (mut a, mut b) = (curve[0].0, curve.last().unwrap().0);
                assert!(r_at(a) > 1.0 && r_at(b) < 1.0, "{name}: R must cross 1 inside the map");
                for _ in 0..60 {
                    let m = 0.5 * (a + b);
                    if r_at(m) > 1.0 { a = m } else { b = m }
                }
                let w_cross = 0.5 * (a + b);
                // The derivation's own arithmetic: target queue, then the curve.
                let target = b0 + sigma + hint_q * l + half_gap;
                let mut w_ref = curve.last().unwrap().0;
                for p in curve.windows(2) {
                    if p[0].1 <= target && target <= p[1].1 && p[1].1 > p[0].1 {
                        w_ref = p[0].0 + (target - p[0].1) * (p[1].0 - p[0].0) / (p[1].1 - p[0].1);
                        break;
                    }
                }
                println!("KA {name} q={hint_q} target={target:.3} ms  W_cross(R=1)={w_cross:.1}  W_derivation={w_ref:.1}");
                assert!((w_cross - w_ref).abs() < 0.5, "{name} q={hint_q}: R's crossing {w_cross} vs derivation {w_ref}");
                // ONE crossing per row (R6.2's gauge prediction), over a W sweep.
                let mut rc = RCross::default();
                let mut w = curve[0].0;
                while w <= curve.last().unwrap().0 {
                    rc.update(r_at(w));
                    w += 8.0;
                }
                assert_eq!(rc.crossings, 1, "{name}: R6.2 predicts one crossing");
                // R3.2's stated range (a table of hand-rounded values, so a
                // tolerance of 5 slots): reported, asserted at this tolerance.
                if bulk {
                    assert!(
                        w_cross >= lo_w - 5.0 && w_cross <= hi_w + 5.0,
                        "{name}: crossing {w_cross} outside R3.2's {lo_w}..{hi_w} (±5)"
                    );
                }
            }
        }
    }

    /// The crossing counter: sign changes only; ties count as at-or-below.
    #[test]
    fn crossing_count_per_row() {
        let mut c = RCross::default();
        for r in [0.8, 0.9, 1.2, 1.1, 0.95, 1.0, 1.3] {
            c.update(r);
        }
        // 0.8,0.9 below | 1.2,1.1 above (x1) | 0.95 below (x2) | 1.0 below | 1.3 above (x3)
        assert_eq!((c.crossings, c.above, c.below), (3, 3, 4));
        let mut one = RCross::default();
        for r in [0.7, 0.9, 1.05, 1.4, 2.0] {
            one.update(r);
        }
        assert_eq!(one.crossings, 1, "the predicted R6.2 shape: one crossing");
    }

    /// The sampler: exact count/min/mean, nearest-rank quantiles, the lifetime
    /// minimum across windows, K brakes, b̂₀⁺ defined only at K, empty window.
    #[test]
    fn sampler_window_statistics_and_boot_brakes() {
        let mut s = WinSampler::default();
        assert!(s.close(Some(1000), Some(50)).is_none(), "empty window has no statistics");
        let mut last = None;
        for k in 0..K_GAUGE {
            for i in 0..10u32 {
                s.observe(5000 + 100 * k as u32 + 10 * i);
            }
            // m̂ stays 4800 µs: brake_k = 5000 + 100k − 4800.
            last = s.close(Some(4800), Some(40 + k as u64));
            let c = last.as_ref().unwrap();
            assert_eq!(c.nk, 10);
            assert_eq!(c.tmin, 5000 + 100 * k as u32);
            assert_eq!(c.k, k + 1);
            assert_eq!(c.brake, Some(200 + 100 * k as i64));
            assert_eq!(c.b0p.is_some(), k + 1 == K_GAUGE, "b0p exists only once K windows closed");
            assert_eq!(c.b0run, Some(200 + 100 * k as i64));
            assert_eq!(c.life_min, 5000, "lifetime min is window 0's");
        }
        let c = last.unwrap();
        assert_eq!(c.b0p, Some(900));
        assert_eq!(c.b0k.as_ref().unwrap().0.len(), K_GAUGE);
        assert_eq!(c.sigb, Some(40 + K_GAUGE as u64 - 1));
        assert_eq!(c.mean, 5000.0 + 100.0 * 7.0 + 45.0);
        // After K: brakes frozen, b0p stays.
        s.observe(9000);
        let d = s.close(Some(4800), Some(999)).unwrap();
        assert_eq!(d.b0p, Some(900));
        assert!(d.b0k.is_none());
        assert_eq!(d.k, K_GAUGE + 1);
    }

    /// The quantile buffer's bind is visible (`nk` counts every sample, `nkq`
    /// only those that fed the quantiles) and its subsample is UNIFORM over the
    /// window, not a prefix: on a rising ramp the kept median is the full
    /// window's median (a first-CAP prefix would read ~40 % of the way up).
    #[test]
    fn sampler_cap_bind_is_visible_and_uniform() {
        let mut s = WinSampler::default();
        let n = WIN_SAMPLE_CAP as u32 * 5 / 2; // 40 960 samples, 2.5 x the cap
        for i in 0..n {
            s.observe(1000 + i);
        }
        let c = s.close(None, None).unwrap();
        assert_eq!(c.nk, n as u64);
        assert!(c.nkq <= WIN_SAMPLE_CAP as u64 && c.nkq > WIN_SAMPLE_CAP as u64 / 2, "{}", c.nkq);
        assert_eq!(c.tmin, 1000);
        let (true_p50, true_p10, true_p90) = (1000 + n / 2, 1000 + n / 10, 1000 + n * 9 / 10);
        // The stride ends at 4: quantiles are within one stride of the full window's.
        assert!(c.p50.abs_diff(true_p50) <= 4, "p50 {} vs {}", c.p50, true_p50);
        assert!(c.p10.abs_diff(true_p10) <= 4 && c.p90.abs_diff(true_p90) <= 4);
        assert!(c.brake.is_none() && c.b0p.is_none());
        // Deterministic: the same feed gives the same kept set.
        let mut s2 = WinSampler::default();
        for i in 0..n {
            s2.observe(1000 + i);
        }
        assert_eq!(s2.close(None, None).unwrap(), c);
        // The stride resets with the window: a small window keeps everything.
        for i in 0..100u32 {
            s.observe(5000 + i);
        }
        assert_eq!(s.close(None, None).unwrap().nkq, 100);
    }

    /// Cost of one window close at c1s volume (23 000 echo samples / 250 ms /
    /// leg): `cargo test --release -- --ignored --nocapture bench_close_23k`.
    #[test]
    #[ignore]
    fn bench_close_23k() {
        let mut lcg: u64 = 12345;
        let mut next = move || {
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (lcg >> 33) as u32
        };
        let mut t = Vec::new();
        let mut s = WinSampler::default();
        for _ in 0..21 {
            let t0 = std::time::Instant::now();
            for _ in 0..23_000 {
                s.observe(4000 + next() % 1500);
            }
            let t1 = std::time::Instant::now();
            let c = s.close(Some(3900), Some(600)).unwrap();
            let t2 = std::time::Instant::now();
            assert_eq!(c.nk, 23_000);
            t.push(((t1 - t0).as_micros() as u64, (t2 - t1).as_micros() as u64));
        }
        t.remove(0); // warm-up
        let mut obs: Vec<u64> = t.iter().map(|x| x.0).collect();
        let mut cl: Vec<u64> = t.iter().map(|x| x.1).collect();
        obs.sort();
        cl.sort();
        println!(
            "BENCH 23000 samples/window: observe total median {} us (min {}, {:.0} ns/sample); close() median {} us (min {}, max {})",
            obs[10], obs[0], obs[10] as f64 * 1000.0 / 23_000.0, cl[10], cl[0], cl[19]
        );
    }

    /// The CAPLOG line round-trips: the printed f64 inputs reproduce the cap
    /// through the pure `pooled_store_cap`, and an unchanged refresh is
    /// counted not printed.
    #[test]
    fn caplog_line_round_trips_through_the_pure_law() {
        let mut cl = CapLog { sink: Some(Vec::new()), ..Default::default() };
        let rec = |psum: f64, cap: usize| CapRec {
            seat: "plain",
            br: "pooled",
            on: true,
            sum_cap: true,
            delta_cap: true,
            b_hint: 2.0,
            n_live: 2,
            psum,
            gain: 2.0,
            floor: 10,
            pool: 2048,
            smax: 1024,
            boot: 128,
            cap,
        };
        let p = 1234.5678901234567_f64;
        let cap = super::super::pooled_store_cap(true, true, true, 2.0, 2, p, 2.0, 10, 2048).unwrap();
        cl.record(10, rec(p, cap));
        cl.record(20, rec(p, cap));
        cl.record(30, rec(p + 1.0, cap + 1));
        let v = cl.sink.as_ref().unwrap();
        assert_eq!(v.len(), 2, "the identical refresh is skipped");
        assert_eq!(cl.refreshes, 3);
        assert!(v[1].ends_with("rep=1"), "{}", v[1]);
        let field = |l: &str, k: &str| -> String {
            l.split_whitespace().find_map(|t| t.strip_prefix(&format!("{k}="))).unwrap().to_string()
        };
        let psum: f64 = field(&v[0], "psum").parse().unwrap();
        assert_eq!(psum.to_bits(), p.to_bits(), "{{}} formatting must round-trip an f64");
        let back = super::super::pooled_store_cap(
            true, true, true, field(&v[0], "b").parse().unwrap(), field(&v[0], "n").parse().unwrap(), psum,
            field(&v[0], "gain").parse().unwrap(), 10, 2048,
        )
        .unwrap();
        assert_eq!(back, field(&v[0], "cap").parse::<usize>().unwrap());
    }

    /// The window renderer end to end on two legs: fields present, R `-`
    /// before K windows and defined after, counters line present.
    #[test]
    fn render_window_two_legs() {
        let mut st = SlawState::new(0);
        let mut ss = [WinSampler::default(), WinSampler::default()];
        let mut lines = Vec::new();
        let mut all: Vec<String> = Vec::new();
        for k in 0..(K_GAUGE + 2) {
            let mut legs = Vec::new();
            for (i, s) in ss.iter_mut().enumerate() {
                for j in 0..20u32 {
                    s.observe(3000 * (i as u32 + 1) + 20 * j);
                }
                legs.push(LegRead {
                    id: i as u32,
                    close: s.close(Some(2900 * (i as u64 + 1)), Some(100)),
                    qcwnd_bytes: 5000,
                    bdp_bytes: 8000.0,
                    btlbw_sym_s: 1000.0,
                });
            }
            st.sample_aw(50, 100);
            st.note_release(12);
            st.note_adv(30);
            st.note_adv(10);
            st.note_qb(0, 7);
            lines = render_window(&mut st, legs, 0.1, 2.0 + k as f64 * 0.25, (k as u64 + 1) * 250_000, 50, 100, k as u64, vec![8, 16]);
            if k + 1 < K_GAUGE {
                assert!(lines.iter().filter(|l| l.starts_with("[SLAW]")).all(|l| l.contains(" R=- ")), "{lines:?}");
            }
            all.extend(lines.iter().cloned());
        }
        // `SLAW_SAMPLE=<file>` saves the rendered lines: tools/l1/testdata/slaw_sample.log
        // is the parser test's input (the engine's own format, not a hand copy).
        if let Ok(p) = std::env::var("SLAW_SAMPLE") {
            std::fs::write(p, all.join("\n") + "\n").unwrap();
        }
        let slaw: Vec<_> = lines.iter().filter(|l| l.starts_with("[SLAW]")).collect();
        assert_eq!(slaw.len(), 2);
        assert!(slaw[0].contains(" R=") && !slaw[0].contains(" R=- "), "{}", slaw[0]);
        assert!(slaw[0].contains("n=100 W=50"), "{}", slaw[0]);
        assert_eq!(lines.iter().filter(|l| l.starts_with("[WSTAT]")).count(), 2);
        assert!(lines.iter().any(|l| l.starts_with("[WSTORE]") && l.contains("relb=1/12/12/12 emb=2/16/16/16 adv=2/30/30/30")), "{lines:?}");
        assert!(lines.iter().any(|l| l.starts_with("[SLAWC]")));
        assert_eq!(st.rc_load.above + st.rc_load.below, 3, "windows K, K+1 and K+2 are the defined ones");
    }
}
