//! The power-seeking store law (`RWM_STORE_LAW=power`, default off; paper §6.1
//! (D), status §29). Pure core: no transport, no clock of its own, no hint.
//!
//! ```text
//!   Π(n) = D(n)^α / T(n)        α = b(δ),  q = q(b)
//!   one period = cap₊ = n̄(1 + ½q) half, then cap₋ = n̄(1 − ½q) half, n̄ fixed
//!   ê  = (ln Π₊ − ln Π₋) / ln((1 + ½q)/(1 − ½q))
//!   n̄' = clamp(n̄·(1 + q)^clamp(ê,−1,1), floor, N·knee)         once per period
//!   cap = cap± + Σᵢ ε̂ᵢ·Dᵢ·stall(δ, ρ, i)                        additive hole term
//! ```
//!
//! No predicate on the hint, δ, ρ or the path count: α and q arrive as numbers,
//! the dither is a square wave of the half index, and a path without loop-time
//! samples drops out of the T max by data availability.

use super::*;

/// One path's measurement of one round: the delivered count and the median of
/// its loop-time (echo-RTT) samples, `None` when the path had no sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathRound {
    pub id: u32,
    pub delivered: u64,
    pub loop_median_s: Option<f64>,
}

/// Per-path round accumulator, fed at `PathState::record_rtt_sample` and
/// `PathState::on_ack` (the `record_delivery` call). Engine state, not the
/// `[LOOPDIAG]` gauge; allocated only when the power law is on. Batch
/// granularity: one push per ack, never per symbol.
#[derive(Debug, Default)]
pub struct PowerAcc {
    rtts_us: Vec<u32>,
    dlv: u64,
}

/// Per-round sample cap: the median is over the first `POWER_RTT_CAP` samples
/// of a round (the overflow is dropped).
pub const POWER_RTT_CAP: usize = 8192;

impl PowerAcc {
    pub fn note_rtt(&mut self, us: u32) {
        if self.rtts_us.len() < POWER_RTT_CAP {
            self.rtts_us.push(us);
        }
    }

    pub fn note_delivered(&mut self, count: u32) {
        self.dlv += count as u64;
    }

    /// Summarise and reset the round (the allocation is kept).
    pub fn drain(&mut self, id: u32) -> PathRound {
        self.rtts_us.sort_unstable();
        let loop_median_s = (!self.rtts_us.is_empty())
            .then(|| crate::monitor::quantile::nearest_rank(&self.rtts_us, 0.5) as f64 / 1e6);
        let r = PathRound { id, delivered: self.dlv, loop_median_s };
        self.rtts_us.clear();
        self.dlv = 0;
        r
    }
}

/// Kleinrock power `Π = D^α / T`.
pub fn power_value(alpha: f64, d: f64, t: f64) -> f64 {
    d.powf(alpha) / t
}

/// The power elasticity from the held pair,
/// `ê = (ln Π₊ − ln Π₋) / ln((1 + ½q)/(1 − ½q))`, Δ ln cap ≈ q fixed.
pub fn power_elasticity(pi_plus: f64, pi_minus: f64, q: f64) -> f64 {
    (pi_plus.ln() - pi_minus.ln()) / ((1.0 + 0.5 * q) / (1.0 - 0.5 * q)).ln()
}

/// Which bound, if any, the n̄ update ran into (a gauge, not a branch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerBind {
    Free,
    Floor,
    Knee,
}

/// `n̄' = clamp(n̄·(1+q)^clamp(ê,−1,1), floor, ceil)`.
pub fn power_update(n_bar: f64, e_hat: f64, q: f64, floor: f64, ceil: f64) -> (f64, PowerBind) {
    let raw = n_bar * (1.0 + q).powf(e_hat.clamp(-1.0, 1.0));
    let n = raw.clamp(floor, ceil);
    let bind = if raw <= floor {
        PowerBind::Floor
    } else if raw >= ceil {
        PowerBind::Knee
    } else {
        PowerBind::Free
    };
    (n, bind)
}

/// One path's inputs to the additive hole term.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HolePath {
    /// The path's fed loss estimate (`PathState::estimator.loss_rate()`).
    pub eps: f64,
    /// The path's delivered rate over the last measured round (sym/s).
    pub rate: f64,
    pub rtprop_s: f64,
    pub srtt_s: f64,
}

/// `Σᵢ ε̂ᵢ·Dᵢ·stall(δ, ρ, i)` (symbols): the hole residency, outside the power
/// seeker. Linear in the stall and so continuous in ρ; both stall terms are
/// always computed ([`contract_stall_s`]).
pub fn power_hole_term(rho: f64, b_hint: f64, paths: &[HolePath]) -> f64 {
    paths
        .iter()
        .map(|p| {
            p.eps.clamp(0.0, 1.0)
                * p.rate.max(0.0)
                * contract_stall_s(rho, b_hint, p.rtprop_s, p.srtt_s)
        })
        .sum()
}

/// What one completed period reports (the `[PCAP]` line's fields).
#[derive(Debug, Clone, PartialEq)]
pub struct PcapLine {
    pub t_s: f64,
    pub period: u64,
    pub nbar: f64,
    pub next: f64,
    pub cap_p: f64,
    pub cap_m: f64,
    pub pi_p: f64,
    pub pi_m: f64,
    pub e_raw: f64,
    pub e_clamp: f64,
    pub bind: f64,
    pub floor_pin: f64,
    pub knee_pin: f64,
    pub hole: f64,
    pub t_loop_us: u64,
    pub d_p: f64,
    pub d_m: f64,
    pub ext: u64,
    pub sign: Option<f64>,
    pub conv: u64,
    pub conv_t_s: f64,
    pub out_pin: f64,
}

/// The `[PCAP]` line (byte-pinned by `pcap_line_is_byte_pinned`; the plan-29
/// parser reads exactly these keys). Fractions (`bind`, `floor_pin`,
/// `knee_pin`, `sign`, `out_pin`) are cumulative over the periods so far.
pub fn format_pcap_line(l: &PcapLine) -> String {
    let sign = l.sign.map_or_else(|| "-".to_string(), |s| format!("{s:.3}"));
    format!(
        "[PCAP] t={:.2}s period={} nbar={:.1} next={:.1} cap_p={:.1} cap_m={:.1} \
         pi_p={:.4e} pi_m={:.4e} e_raw={:.3} e_clamp={:.3} bind={:.3} floor_pin={:.3} \
         knee_pin={:.3} hole={:.1} t_loop_us={} d_p={:.0} d_m={:.0} ext={} sign={} \
         conv={} conv_t={:.2}s out_pin={:.3}",
        l.t_s, l.period, l.nbar, l.next, l.cap_p, l.cap_m, l.pi_p, l.pi_m, l.e_raw,
        l.e_clamp, l.bind, l.floor_pin, l.knee_pin, l.hole, l.t_loop_us, l.d_p, l.d_m,
        l.ext, sign, l.conv, l.conv_t_s, l.out_pin,
    )
}

/// The once-at-start liveness echo (`docs/measurement-discipline.md` rule 1).
pub fn power_liveness_line(alpha: f64, q: f64, floor: usize, knee: usize) -> String {
    format!(
        "power store law ACTIVE (RWM_STORE_LAW=power): alpha={alpha:.4} q={q:.4} floor={floor} knee={knee}"
    )
}

/// The power seeker's state: one period is two halves at fixed n̄ (cap₊ then
/// cap₋), each half two rounds of T (first round discarded, second measured);
/// n̄ updates once per period from the held pair.
#[derive(Debug)]
pub struct PowerSeeker {
    alpha: f64,
    q: f64,
    n_bar: f64,
    seeded: bool,
    /// Round index in the period: 0 settle₊, 1 measure₊, 2 settle₋, 3 measure₋.
    phase: u8,
    round_start_us: u64,
    round_len_us: u64,
    t_est_s: f64,
    t0_us: u64,
    pi_plus: f64,
    d_plus: f64,
    last_rates: Vec<(u32, f64)>,
    hole: f64,
    // gauges
    periods: u64,
    floor_pins: u64,
    knee_pins: u64,
    clamp_binds: u64,
    ext: u64,
    sign_pairs: u64,
    sign_agree: u64,
    prev_sign: Option<bool>,
    out_calls: u64,
    out_pins: u64,
    /// Per period: (n̄ after the update, wall time on the `[DIAG]` clock).
    n_hist: Vec<(f64, f64)>,
}

impl PowerSeeker {
    pub fn new(alpha: f64, q: f64) -> Self {
        Self {
            alpha,
            q,
            n_bar: 0.0,
            seeded: false,
            phase: 0,
            round_start_us: 0,
            round_len_us: 1,
            t_est_s: 0.0,
            t0_us: 0,
            pi_plus: 0.0,
            d_plus: 0.0,
            last_rates: Vec::new(),
            hole: 0.0,
            periods: 0,
            floor_pins: 0,
            knee_pins: 0,
            clamp_binds: 0,
            ext: 0,
            sign_pairs: 0,
            sign_agree: 0,
            prev_sign: None,
            out_calls: 0,
            out_pins: 0,
            n_hist: Vec::new(),
        }
    }

    pub fn alpha(&self) -> f64 {
        self.alpha
    }

    pub fn q(&self) -> f64 {
        self.q
    }

    pub fn is_seeded(&self) -> bool {
        self.seeded
    }

    pub fn n_bar(&self) -> f64 {
        self.n_bar
    }

    pub fn periods(&self) -> u64 {
        self.periods
    }

    /// Start at `n0` (the shipped pooled law's ask at the first refresh where
    /// it is defined) with `t_est_s` (Copa's srtt max over live paths) as the
    /// round length until the first measurement. `t0_us` is the `[DIAG]` clock
    /// origin, so `[PCAP] t=` shares `[LOOPDIAG] t=`'s clock.
    pub fn seed(&mut self, n0: f64, t_est_s: f64, now_us: u64, t0_us: u64) {
        self.n_bar = n0;
        self.seeded = true;
        self.phase = 0;
        self.t_est_s = t_est_s;
        self.t0_us = t0_us;
        self.start_round(now_us);
    }

    fn start_round(&mut self, now_us: u64) {
        self.round_start_us = now_us;
        self.round_len_us = ((self.t_est_s * 1e6) as u64).max(1);
    }

    /// The round has run its length of T.
    pub fn round_due(&self, now_us: u64) -> bool {
        self.seeded && now_us.saturating_sub(self.round_start_us) >= self.round_len_us
    }

    pub fn round_len_us(&self) -> u64 {
        self.round_len_us
    }

    /// The dither: a square wave of the half index, +½q then −½q in log-cap.
    fn dither(&self) -> f64 {
        let half = (self.phase / 2) as f64;
        1.0 - 2.0 * (half % 2.0)
    }

    /// The current half's cap: n̄·(1 ± ½q).
    pub fn cap_now(&self) -> f64 {
        self.n_bar * (1.0 + self.dither() * 0.5 * self.q)
    }

    /// Per-path delivered rate of the last measured round (the hole term's Dᵢ).
    pub fn last_rates(&self) -> &[(u32, f64)] {
        &self.last_rates
    }

    pub fn set_hole(&mut self, hole: f64) {
        self.hole = hole;
    }

    /// The output cap: the dithered value plus the hole term, ceiled to
    /// symbols, then clamped to `[floor, ceil]`. The final clamp is gauged
    /// separately (`out_pin=`) from the clamp on n̄.
    pub fn output_cap(&mut self, floor: usize, ceil: usize) -> usize {
        let raw = (self.cap_now() + self.hole).ceil() as usize;
        let out = raw.clamp(floor, ceil);
        self.out_calls += 1;
        self.out_pins += (out != raw) as u64;
        out
    }

    /// Close the round that started at `round_start_us`; `paths` is its
    /// per-path drain. Returns the period report when this round closed a
    /// period.
    pub fn on_round(
        &mut self,
        now_us: u64,
        paths: &[PathRound],
        floor: usize,
        ceil: usize,
    ) -> Option<PcapLine> {
        let wall_s = now_us.saturating_sub(self.round_start_us) as f64 / 1e6;
        if self.phase % 2 == 0 {
            // The settling round: discarded.
            self.phase += 1;
            self.start_round(now_us);
            return None;
        }
        let delivered: u64 = paths.iter().map(|p| p.delivered).sum();
        let d = if wall_s > 0.0 { delivered as f64 / wall_s } else { 0.0 };
        // Paths WITH samples only: data availability, not a path-count branch.
        let t_s = paths
            .iter()
            .filter_map(|p| p.loop_median_s)
            .fold(None, |m: Option<f64>, t| Some(m.map_or(t, |m| m.max(t))));
        let measured = t_s
            .map(|t| (t, power_value(self.alpha, d, t)))
            .filter(|(t, pi)| d > 0.0 && *t > 0.0 && pi.is_finite() && *pi > 0.0);
        let Some((t_s, pi)) = measured else {
            // A thin round: the half is extended by one round, no update.
            self.ext += 1;
            self.start_round(now_us);
            return None;
        };
        self.t_est_s = t_s;
        self.last_rates = paths
            .iter()
            .map(|p| (p.id, p.delivered as f64 / wall_s))
            .collect();
        if self.phase == 1 {
            self.pi_plus = pi;
            self.d_plus = d;
            self.phase = 2;
            self.start_round(now_us);
            return None;
        }
        // The − half is measured: the period closes, n̄ updates once.
        let e_raw = power_elasticity(self.pi_plus, pi, self.q);
        let held = self.n_bar;
        let (next, bind) = power_update(held, e_raw, self.q, floor as f64, ceil as f64);
        self.n_bar = next;
        let period = self.periods;
        self.periods += 1;
        let np = self.periods as f64;
        self.floor_pins += (bind == PowerBind::Floor) as u64;
        self.knee_pins += (bind == PowerBind::Knee) as u64;
        self.clamp_binds += (e_raw.abs() > 1.0) as u64;
        let sign = e_raw > 0.0;
        if let Some(prev) = self.prev_sign {
            self.sign_pairs += 1;
            self.sign_agree += (prev == sign) as u64;
        }
        self.prev_sign = Some(sign);
        let t_wall = now_us.saturating_sub(self.t0_us) as f64 / 1e6;
        self.n_hist.push((next, t_wall));
        let conv = self.conv_index();
        let line = PcapLine {
            t_s: t_wall,
            period,
            nbar: held,
            next,
            cap_p: held * (1.0 + 0.5 * self.q),
            cap_m: held * (1.0 - 0.5 * self.q),
            pi_p: self.pi_plus,
            pi_m: pi,
            e_raw,
            e_clamp: e_raw.clamp(-1.0, 1.0),
            bind: self.clamp_binds as f64 / np,
            floor_pin: self.floor_pins as f64 / np,
            knee_pin: self.knee_pins as f64 / np,
            hole: self.hole,
            t_loop_us: (t_s * 1e6).round() as u64,
            d_p: self.d_plus,
            d_m: d,
            ext: self.ext,
            sign: (self.sign_pairs > 0).then(|| self.sign_agree as f64 / self.sign_pairs as f64),
            conv: conv as u64,
            conv_t_s: self.n_hist[conv].1,
            out_pin: self.out_pins as f64 / self.out_calls.max(1) as f64,
        };
        self.phase = 0;
        self.start_round(now_us);
        Some(line)
    }

    /// The running `conv_period`: the first index of the suffix of per-period
    /// n̄ that stays within ±q of the current n̄.
    fn conv_index(&self) -> usize {
        let now = self.n_bar;
        let mut i = self.n_hist.len();
        while i > 0 && (self.n_hist[i - 1].0 / now - 1.0).abs() <= self.q {
            i -= 1;
        }
        i.min(self.n_hist.len() - 1)
    }
}
