//! The loop-time gauge (`RWM_LOOPDIAG`, default OFF, observation only).
//!
//! Status §23 / paper §6.1: the store law is a window controller fed by its
//! own output. The next diagnosis needs, per path and per `[DIAG]` window
//! (250 ms), side by side: the echo-RTT loop-time distribution, Copa's RTprop
//! / jitter / srtt, the delivered rate, and quinn's own BBR bandwidth
//! estimate. One `[LOOPDIAG]` line per path carries all of them.
//!
//! Feeds (both batch-granular, no per-symbol work):
//!   * every RTT sample `PathState::record_rtt_sample` receives (the single
//!     delegate every ack path funnels through: the per-batch `Ack` echo and
//!     the `WindowAck` echo, `net/control_msg.rs`) — the same series
//!     `CopaState::record_rtt` consumes (the wire RTT under `RWM_COPA_WIRE`);
//!   * the `count` passed to `CopaState::record_delivery` (accepted or
//!     rejected by its 1 ms floor), i.e. the per-ack delivered delta.
//!
//! Quinn's estimate is published by the io owner into the `PathView`
//! (`transport/io_owner.rs`), read here as atomics.
//!
//! Cost and behaviour. Gate off: [`gauge`] is a `OnceLock<Option<…>>`, so
//! every feed site is a null check. Gate on: the gauge owns all its state and
//! reads/writes no engine state, so no decision can observe it
//! (`loopdiag_is_observation_only`, `copa_state_is_bit_identical_gate_on_off`).
//!
//! Lock order: feeds run under the scheduler lock, so the gauge lock is
//! inner (`scheduler → gauge`); the report drains the gauge while it already
//! holds the scheduler, never the other way round.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::monitor::quantile::nearest_rank;

/// Per-path, per-window RTT sample cap. The overflow count is printed (`ov=`).
pub const RTT_CAP: usize = 8192;

/// One path's window: RTT samples (µs), the overflow past [`RTT_CAP`], and the
/// delivered count the rate sampler was fed.
#[derive(Debug, Default)]
pub struct WindowBuf {
    rtts: Vec<u32>,
    overflow: u64,
    dlv: u64,
}

/// A drained window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowSummary {
    /// Samples buffered (the quantiles' population).
    pub n: usize,
    /// Samples the cap refused.
    pub overflow: u64,
    pub min_us: Option<u32>,
    pub p10_us: Option<u32>,
    pub p50_us: Option<u32>,
    /// Σ delivered count this window (symbols).
    pub dlv: u64,
}

impl WindowBuf {
    pub fn push_rtt(&mut self, us: u32) {
        if self.rtts.len() < RTT_CAP {
            self.rtts.push(us);
        } else {
            self.overflow += 1;
        }
    }

    pub fn add_delivered(&mut self, count: u32) {
        self.dlv += count as u64;
    }

    /// Summarise and reset the window.
    pub fn drain(&mut self) -> WindowSummary {
        let mut v = std::mem::take(&mut self.rtts);
        v.sort_unstable();
        let s = WindowSummary {
            n: v.len(),
            overflow: self.overflow,
            min_us: v.first().copied(),
            p10_us: (!v.is_empty()).then(|| nearest_rank(&v, 0.10)),
            p50_us: (!v.is_empty()).then(|| nearest_rank(&v, 0.50)),
            dlv: self.dlv,
        };
        self.overflow = 0;
        self.dlv = 0;
        // Keep the allocation for the next window.
        v.clear();
        self.rtts = v;
        s
    }
}

/// Copa's side of the line, read under the scheduler lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopaReadout {
    /// `CopaState::min_rtt`, µs; `None` before the first sample.
    pub rtprop_us: Option<u64>,
    /// `CopaState::jitter_est`, µs.
    pub jit_us: u64,
    /// `CopaState::srtt`, µs; `None` before the first sample.
    pub srtt_us: Option<u64>,
}

/// Quinn's side of the line (the owner's last full snapshot).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuinnReadout {
    /// `metrics().pacing_rate`, bits/s; 0 = none.
    pub pace_bps: u64,
    /// `metrics().congestion_window`, bytes.
    pub cwnd: u64,
    /// quinn's smoothed RTT, µs; `None` before the first snapshot.
    pub srtt_us: Option<u64>,
}

fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "-".to_string(), |x| x.to_string())
}

/// The `[LOOPDIAG]` line (without the tag-free prefix: the caller passes it to
/// `readout!` verbatim). `t_s` is the `[DIAG] t=` clock; `dt_s` the window.
/// Empty-window RTT fields print `-`, never 0.
pub fn format_line(
    t_s: f64,
    path: u32,
    w: &WindowSummary,
    dt_s: f64,
    copa: &CopaReadout,
    q: Option<&QuinnReadout>,
) -> String {
    let dlv_rate = if dt_s > 0.0 { w.dlv as f64 / dt_s } else { 0.0 };
    let (pace, cwnd, qsrtt) = match q {
        Some(q) => (
            if q.pace_bps > 0 { q.pace_bps.to_string() } else { "-".to_string() },
            q.cwnd.to_string(),
            opt(q.srtt_us),
        ),
        None => ("-".to_string(), "-".to_string(), "-".to_string()),
    };
    format!(
        "[LOOPDIAG] t={:.1}s p{} n={} ov={} rtt_min_us={} rtt_p10_us={} rtt_p50_us={} \
         rtprop_us={} jit_us={} srtt_us={} dlv={} dlv_rate={:.0} q_pace_bps={} q_cwnd={} \
         q_srtt_us={}",
        t_s,
        path,
        w.n,
        w.overflow,
        opt(w.min_us),
        opt(w.p10_us),
        opt(w.p50_us),
        opt(copa.rtprop_us),
        copa.jit_us,
        opt(copa.srtt_us),
        w.dlv,
        dlv_rate,
        pace,
        cwnd,
        qsrtt,
    )
}

/// The gauge. One per process, keyed by path inside.
#[derive(Default)]
pub struct LoopDiag {
    paths: parking_lot::Mutex<HashMap<u32, WindowBuf>>,
    /// RTT samples offered over the run (cap or not) — the zero-cost witness.
    offered: AtomicU64,
}

impl LoopDiag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn note_rtt(&self, path: u32, us: u32) {
        self.offered.fetch_add(1, Ordering::Relaxed);
        self.paths.lock().entry(path).or_default().push_rtt(us);
    }

    pub fn note_delivered(&self, path: u32, count: u32) {
        self.paths.lock().entry(path).or_default().add_delivered(count);
    }

    /// Summarise and reset one path's window.
    pub fn drain(&self, path: u32) -> WindowSummary {
        self.paths.lock().entry(path).or_default().drain()
    }

    pub fn offered(&self) -> u64 {
        self.offered.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
thread_local! {
    /// Test-only per-thread gauge, so the gate-on path is exercised without
    /// flipping the process-global `OnceLock`.
    static TEST_GAUGE: std::cell::Cell<Option<&'static LoopDiag>> =
        const { std::cell::Cell::new(None) };
}

/// The process-global gauge, or `None` with the gate off.
pub fn gauge() -> Option<&'static LoopDiag> {
    #[cfg(test)]
    if let Some(g) = TEST_GAUGE.with(|c| c.get()) {
        return Some(g);
    }
    static G: std::sync::OnceLock<Option<LoopDiag>> = std::sync::OnceLock::new();
    G.get_or_init(|| crate::gates::get().loopdiag.then(LoopDiag::new))
        .as_ref()
}

#[cfg(test)]
mod tests;
