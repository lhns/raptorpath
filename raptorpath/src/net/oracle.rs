//! The §43 diagnostic oracle store law (`RWM_STORE_LAW=oracle`) and its
//! sampler readout (`RWM_ORACLE_DIAG=1`). Default off, never merged, no law is
//! built from it: its L comes from the environment (paper §6.1, "The
//! closed-loop oracle arm"; status §43 and amendment 1).
//!
//! ```text
//!   n(t) = clamp( ceil( (1 + q(δ)) · Σᵢ Lᵢ · (RTpropᵢ(t) + Sᵢ(t)) ),  max(floor, h(t)),  N·knee )
//!   Sᵢ   = Tp50 − Tmin of the last closed 250 ms window of path i that held ≥ 1 sample (0 before)
//!   h(t) = H · 1{t < t_h}
//! ```
//!
//! * L: `RWM_ORACLE_L_DGPS`, one entry per configured path, in path-id order
//!   (entry i belongs to path id i).
//! * RTprop: Copa's 10 s windowed min, read live.
//! * S: from the sampler below, which keeps EVERY app-echo sample of the
//!   window (the `[LOOPDIAG]` instrument kept the first 8 192 and counted the
//!   rest in `ov=`, so its c1 windows described only their start).
//! * Windows close on the `[DIAG]` tick (`diag::report`, ≥ 250 ms), in the
//!   same loop iteration that ran the store-cap refresh, and `h(t)` reads the
//!   same clock origin (`dg.diag_start_us`). The law therefore needs
//!   `RWM_DIAG=1`; `OracleGates::check` refuses to start without it.
//! * Between closes only RTprop moves n. The refresh is the shipped one
//!   (≤ 5 ms); there is no step limit and no timer of its own.
//! * The hold h(t) is an exogenous step on the clamp's lower bound: it reads no
//!   δ, ρ, hint, path count or measurement.
//!
//! The gauge owns all its state; with both gates off [`gauge`] is `None` and
//! every feed site is a null check, so the shipped cap sequence is unchanged.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::store_cap::codel_setpoint_q;
use crate::monitor::quantile::nearest_rank;
use crate::scheduler::Scheduler;

/// The window length, declared in the paper block: 250 ms, tumbling.
pub const T_W_S: f64 = 0.25;

// ── gates ────────────────────────────────────────────────────────────────

/// The four env gates, resolved once. Parse errors are kept, not panicked, so
/// the one startup check ([`OracleGates::check`]) refuses with the reason.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OracleGates {
    /// `RWM_STORE_LAW=oracle`: the law decides the cap.
    pub law: bool,
    /// `RWM_ORACLE_DIAG=1`: the sampler and the counterfactual ask as an
    /// observation-only readout (MAIN).
    pub diag: bool,
    /// `RWM_ORACLE_L_DGPS`: datagrams/s per configured path, in path-id order.
    pub l_dgps: Vec<f64>,
    /// `RWM_ORACLE_HOLD=<H>:<t_h seconds>`: the start-high hold.
    pub hold: Option<(usize, f64)>,
    /// The first parse error, if any.
    pub err: Option<String>,
}

impl OracleGates {
    /// Parse the four raw env values. Pure: the tests drive it directly.
    pub fn parse(
        store_law: Option<&str>,
        l: Option<&str>,
        hold: Option<&str>,
        diag: Option<&str>,
    ) -> Self {
        let mut g = OracleGates::default();
        let bad = |g: &mut OracleGates, m: String| {
            g.err.get_or_insert(m);
        };
        match store_law {
            None | Some("pooled") => {}
            Some("oracle") => g.law = true,
            Some(o) => bad(
                &mut g,
                format!("RWM_STORE_LAW={o:?} is not a store law (expected `pooled` or `oracle`)"),
            ),
        }
        match diag {
            None | Some("0") | Some("false") => {}
            Some("1") | Some("true") => g.diag = true,
            Some(o) => bad(&mut g, format!("RWM_ORACLE_DIAG={o:?} is not 0 or 1")),
        }
        if let Some(raw) = l {
            let parsed: Result<Vec<f64>, _> = raw.split(',').map(|s| s.trim().parse::<f64>()).collect();
            match parsed {
                Ok(v) if !v.is_empty() && v.iter().all(|x| x.is_finite() && *x > 0.0) => g.l_dgps = v,
                _ => bad(
                    &mut g,
                    format!("RWM_ORACLE_L_DGPS={raw:?} is not a comma list of positive datagram rates"),
                ),
            }
        }
        if let Some(raw) = hold {
            let parsed = raw.split_once(':').and_then(|(h, t)| {
                let h = h.trim().parse::<usize>().ok().filter(|h| *h > 0)?;
                let t = t.trim().parse::<f64>().ok().filter(|t| t.is_finite() && *t >= 0.0)?;
                Some((h, t))
            });
            match parsed {
                Some(ht) => g.hold = Some(ht),
                None => bad(
                    &mut g,
                    format!("RWM_ORACLE_HOLD={raw:?} is not <H>:<t_h seconds> with H > 0"),
                ),
            }
        }
        g
    }

    pub fn resolve() -> Self {
        let env = |k: &str| std::env::var(k).ok();
        Self::parse(
            env("RWM_STORE_LAW").as_deref(),
            env("RWM_ORACLE_L_DGPS").as_deref(),
            env("RWM_ORACLE_HOLD").as_deref(),
            env("RWM_ORACLE_DIAG").as_deref(),
        )
    }

    /// Either the law or the readout is on.
    pub fn active(&self) -> bool {
        self.law || self.diag
    }

    /// The startup configuration checks. `Err` means the engine must refuse
    /// to start: a mis-sized L list, a hold above the ceiling, or a law that
    /// cannot run would each measure a different machine than the one named.
    ///
    /// * `n_paths`: the configured path count (`bind_addrs.len()`).
    /// * `knee`: the per-path pool (`RWM_STORE_PATH_POOL`, 2 048).
    /// * `diag_on`: `RWM_DIAG` (windows close on its tick).
    /// * `plain_dyn_cap`: the plain-reliable dynamic cap is the seat (a window
    ///   reliable run with `RWM_STORE` unset).
    pub fn check(
        &self,
        n_paths: usize,
        knee: usize,
        diag_on: bool,
        plain_dyn_cap: bool,
    ) -> Result<(), String> {
        if let Some(e) = &self.err {
            return Err(e.clone());
        }
        if self.hold.is_some() && !self.law {
            return Err("RWM_ORACLE_HOLD needs RWM_STORE_LAW=oracle (it is the law's lower bound)".into());
        }
        if !self.active() {
            return Ok(());
        }
        if self.l_dgps.len() != n_paths {
            return Err(format!(
                "RWM_ORACLE_L_DGPS has {} entries but {} paths are configured (one L per path, in path-id order)",
                self.l_dgps.len(),
                n_paths
            ));
        }
        if !diag_on {
            return Err("the oracle windows close on the [DIAG] tick: set RWM_DIAG=1".into());
        }
        if self.law && !plain_dyn_cap {
            return Err(
                "RWM_STORE_LAW=oracle replaces the plain dynamic store cap: it needs a window-reliable \
                 run with RWM_STORE unset"
                    .into(),
            );
        }
        if let Some((h, _)) = self.hold {
            let ceiling = n_paths.saturating_mul(knee);
            if h > ceiling {
                return Err(format!(
                    "RWM_ORACLE_HOLD H={h} exceeds N*knee = {n_paths}*{knee} = {ceiling}: the clamp's bounds would invert"
                ));
            }
        }
        Ok(())
    }

    /// The liveness echo of the law (rule 1). The readout-only arm prints a
    /// different line that never contains "oracle store law ACTIVE".
    pub fn echo(&self) -> Option<String> {
        if self.law {
            let hold = self
                .hold
                .map_or_else(|| "none".to_string(), |(h, t)| format!("H={h} t_h={t}s"));
            Some(format!(
                "oracle store law ACTIVE (RWM_STORE_LAW=oracle, diagnostic, never merged): L_dgps={:?} hold={} T_w={}s",
                self.l_dgps, hold, T_W_S
            ))
        } else if self.diag {
            Some(format!(
                "[OCAP] sampler readout ACTIVE (RWM_ORACLE_DIAG=1, observation only): L_dgps={:?} T_w={}s",
                self.l_dgps, T_W_S
            ))
        } else {
            None
        }
    }
}

// ── the law, pure ────────────────────────────────────────────────────────

/// One path's term of the law: Lᵢ · (RTpropᵢ + Sᵢ).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OracleTerm {
    pub l_dgps: f64,
    pub rtprop_s: f64,
    pub s_s: f64,
}

/// The unclamped ask: (1 + q) · Σᵢ Lᵢ · (RTpropᵢ + Sᵢ).
pub fn oracle_ask(q: f64, terms: &[OracleTerm]) -> f64 {
    (1.0 + q) * terms.iter().map(|t| t.l_dgps * (t.rtprop_s + t.s_s)).sum::<f64>()
}

/// h(t) = H · 1{t < t_h}; 0 without a hold.
pub fn hold_h(hold: Option<(usize, f64)>, t_s: f64) -> usize {
    match hold {
        Some((h, t_h)) if t_s < t_h => h,
        _ => 0,
    }
}

/// The clamp's outcome and which bound it hit. A bind needs the ask to lie
/// strictly outside the bound.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapDecision {
    pub n: usize,
    pub ask: f64,
    pub floor_bind: bool,
    pub pin_bind: bool,
    pub hold_bind: bool,
}

/// n = clamp(ceil(ask), max(floor, h), max(upper, lower)). The upper bound is
/// lifted to the lower one so the clamp can never invert (`check` refuses the
/// configuration that would need it).
pub fn oracle_cap(ask: f64, floor: usize, h: usize, upper: usize) -> CapDecision {
    let want = if ask.is_finite() && ask > 0.0 { ask.ceil() as usize } else { 0 };
    let lower = floor.max(h);
    let upper = upper.max(lower);
    CapDecision {
        n: want.clamp(lower, upper),
        ask,
        floor_bind: h <= floor && want < floor,
        pin_bind: want > upper,
        hold_bind: h > floor && want < h,
    }
}

/// The law at one refresh: the live paths' terms, δ through b, the clamp.
/// `t_s` is on the `[DIAG]` clock. Returns the decision and h(t).
pub(crate) fn decide(
    og: &OracleGates,
    g: &OracleGauge,
    sched: &Scheduler,
    delta_b: f64,
    floor: usize,
    pool: usize,
    t_s: f64,
) -> (CapDecision, usize) {
    let q = codel_setpoint_q(delta_b);
    let mut live: Vec<_> = sched.live_paths_iter().collect();
    live.sort_by_key(|(id, _)| *id);
    let terms: Vec<OracleTerm> = live
        .iter()
        .map(|(id, p)| OracleTerm {
            l_dgps: og.l_dgps.get(*id as usize).copied().unwrap_or(0.0),
            rtprop_s: p.min_rtt().map_or(0.0, |d| d.as_secs_f64()),
            s_s: g.s_s(*id),
        })
        .collect();
    let h = hold_h(og.hold, t_s);
    let upper = live.len().max(1).saturating_mul(pool);
    (oracle_cap(oracle_ask(q, &terms), floor, h, upper), h)
}

// ── the sampler ──────────────────────────────────────────────────────────

#[derive(Default)]
struct PathWin {
    /// Every sample of the open window (µs). Never capped.
    rtts: Vec<u32>,
    /// S of the last closed window that held a sample (µs); 0 before.
    s_us: u64,
    /// Samples offered / samples closed into windows, over the run.
    offered: u64,
    cum_nk: u64,
}

/// One drained window of one path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseRec {
    pub n_k: usize,
    pub tmin_us: Option<u32>,
    pub tp50_us: Option<u32>,
    /// S in effect after this close (µs).
    pub s_us: u64,
    pub offered: u64,
    pub cum_nk: u64,
}

/// The refresh-side accumulators of one window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RefreshWin {
    pub refreshes: u64,
    pub floor: u64,
    pub pin: u64,
    pub hold: u64,
    pub n_min: usize,
    pub n_max: usize,
    /// The last refresh's cap, ask and h (kept across windows).
    pub n: usize,
    pub ask: f64,
    pub h: usize,
}

struct Acc {
    refreshes: u64,
    floor: u64,
    pin: u64,
    hold: u64,
    n_min: usize,
    n_max: usize,
    n: usize,
    ask: f64,
    h: usize,
}

impl Default for Acc {
    fn default() -> Self {
        Acc { refreshes: 0, floor: 0, pin: 0, hold: 0, n_min: usize::MAX, n_max: 0, n: 0, ask: 0.0, h: 0 }
    }
}

#[derive(Default)]
struct Inner {
    paths: HashMap<u32, PathWin>,
    acc: Acc,
}

/// The gauge: per-path sample windows plus the refresh accumulators.
#[derive(Default)]
pub struct OracleGauge {
    inner: parking_lot::Mutex<Inner>,
}

impl OracleGauge {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every RTT sample `PathState::record_rtt_sample` receives.
    pub fn note_rtt(&self, path: u32, us: u32) {
        let mut g = self.inner.lock();
        let w = g.paths.entry(path).or_default();
        w.offered += 1;
        w.rtts.push(us);
    }

    /// Sᵢ in seconds (0 before the first closed non-empty window).
    pub fn s_s(&self, path: u32) -> f64 {
        self.inner.lock().paths.get(&path).map_or(0, |w| w.s_us) as f64 / 1e6
    }

    /// Close one path's window: n_k, the exact nearest-rank median, the min,
    /// and S for the next refreshes. An empty window leaves S as it was.
    pub fn close_path(&self, path: u32) -> CloseRec {
        let mut g = self.inner.lock();
        let w = g.paths.entry(path).or_default();
        let mut v = std::mem::take(&mut w.rtts);
        v.sort_unstable();
        let n_k = v.len();
        let tmin = v.first().copied();
        let tp50 = (n_k > 0).then(|| nearest_rank(&v, 0.50));
        if let (Some(lo), Some(p50)) = (tmin, tp50) {
            w.s_us = u64::from(p50 - lo);
        }
        w.cum_nk += n_k as u64;
        v.clear();
        w.rtts = v;
        CloseRec { n_k, tmin_us: tmin, tp50_us: tp50, s_us: w.s_us, offered: w.offered, cum_nk: w.cum_nk }
    }

    /// One refresh: `n` is the cap actually in force, `d` the law's (or the
    /// counterfactual's) decision, `h` the hold.
    pub fn note_refresh(&self, n: usize, d: &CapDecision, h: usize) {
        let mut g = self.inner.lock();
        let a = &mut g.acc;
        a.refreshes += 1;
        a.floor += d.floor_bind as u64;
        a.pin += d.pin_bind as u64;
        a.hold += d.hold_bind as u64;
        a.n_min = a.n_min.min(n);
        a.n_max = a.n_max.max(n);
        a.n = n;
        a.ask = d.ask;
        a.h = h;
    }

    /// Take the window's refresh accumulators and reset them.
    pub fn drain_refresh(&self) -> RefreshWin {
        let mut g = self.inner.lock();
        let a = std::mem::take(&mut g.acc);
        let (lo, hi) = if a.refreshes == 0 { (a.n, a.n) } else { (a.n_min, a.n_max) };
        g.acc = Acc { n: a.n, ask: a.ask, h: a.h, ..Acc::default() };
        RefreshWin {
            refreshes: a.refreshes,
            floor: a.floor,
            pin: a.pin,
            hold: a.hold,
            n_min: lo,
            n_max: hi,
            n: a.n,
            ask: a.ask,
            h: a.h,
        }
    }
}

fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "-".to_string(), |x| x.to_string())
}

/// The `[OCAP]` line of one path and one closed window. Fields:
///
/// * `n`, `ask`, `h`: the cap in force, the unclamped ask, the hold, at the
///   window's last refresh (`ask` is the counterfactual in MAIN rows);
/// * `rtprop_us`, `tmin_us`, `tp50_us`, `s_us`, `brake_us = tmin − rtprop`;
/// * `n_k`, `offered`: the window's sample count and the cumulative offered
///   count (Σ n_k equals it by construction; the scorer checks the sums);
/// * `r_k = T_w / Tp50`; `wbind = 1` iff `1/r_k > q`;
/// * `floor`, `pin`, `hold`: the share of the window's refreshes at which the
///   bound bound; `nmin`, `nmax`: n over the window's refreshes;
/// * `law`: `oracle` (the law decides n) or `shadow` (MAIN readout).
pub fn format_ocap(
    t_s: f64,
    path: u32,
    law: bool,
    q: f64,
    rf: &RefreshWin,
    w: &CloseRec,
    rtprop_us: Option<u64>,
) -> String {
    let frac = |c: u64| if rf.refreshes == 0 { 0.0 } else { c as f64 / rf.refreshes as f64 };
    let r_k = w.tp50_us.filter(|p| *p > 0).map(|p| T_W_S / (p as f64 / 1e6));
    let wbind = r_k.map_or(0, |r| (1.0 / r > q) as u8);
    let brake = match (w.tmin_us, rtprop_us) {
        (Some(t), Some(r)) => Some(t as i64 - r as i64),
        _ => None,
    };
    format!(
        "[OCAP] t={:.1}s p{} law={} n={} ask={:.2} h={} rtprop_us={} tmin_us={} tp50_us={} s_us={} brake_us={} \
         n_k={} offered={} r_k={} floor={:.3} pin={:.3} hold={:.3} wbind={} nmin={} nmax={} refreshes={} q={:.4}",
        t_s,
        path,
        if law { "oracle" } else { "shadow" },
        rf.n,
        rf.ask,
        rf.h,
        opt(rtprop_us),
        opt(w.tmin_us),
        opt(w.tp50_us),
        w.s_us,
        opt(brake),
        w.n_k,
        w.offered,
        r_k.map_or_else(|| "-".to_string(), |r| format!("{r:.2}")),
        frac(rf.floor),
        frac(rf.pin),
        frac(rf.hold),
        wbind,
        rf.n_min,
        rf.n_max,
        rf.refreshes,
        q,
    )
}

/// The `[DIAG]` tick: close every path's window and format the `[OCAP]`
/// lines. Called once per tick by `diag::report`, after the cap refresh of the
/// same loop iteration.
pub(crate) fn close_tick(
    g: &OracleGauge,
    sched: &Scheduler,
    t_s: f64,
    delta_b: f64,
    law: bool,
) -> Vec<String> {
    let q = codel_setpoint_q(delta_b);
    let rf = g.drain_refresh();
    let mut ids = sched.all_path_ids();
    ids.sort_unstable();
    ids.into_iter()
        .map(|id| {
            let rtprop_us = sched.path(id).and_then(|p| p.min_rtt()).map(|d| d.as_micros() as u64);
            let w = g.close_path(id);
            format_ocap(t_s, id, law, q, &rf, &w, rtprop_us)
        })
        .collect()
}

#[cfg(test)]
thread_local! {
    /// Test-only per-thread gauge, so the gate-on path runs without flipping
    /// the process-global `OnceLock`.
    static TEST_GAUGE: std::cell::Cell<Option<&'static OracleGauge>> =
        const { std::cell::Cell::new(None) };
}

/// The process-global gauge, or `None` with both gates off.
pub fn gauge() -> Option<&'static OracleGauge> {
    #[cfg(test)]
    if let Some(g) = TEST_GAUGE.with(|c| c.get()) {
        return Some(g);
    }
    static G: OnceLock<Option<OracleGauge>> = OnceLock::new();
    G.get_or_init(|| crate::gates::get().oracle.active().then(OracleGauge::new))
        .as_ref()
}

#[cfg(test)]
mod tests;
