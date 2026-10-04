//! Multipath scheduler: distributes symbols across paths based on
//! throughput, loss, and latency measurements.
//!
//! Unlike round-robin MPTCP, we schedule symbols proportional to each path's
//! effective goodput and route repair symbols preferentially to better paths.
//!
//! Congestion control is Copa-lite (delay-based, paper §8.2):
//!
//!   - Propagation floor = min RTT sample in a sliding ~10s window.
//!   - Queuing-delay signal = min RTT sample since the last cwnd update
//!     (a windowed MIN, not an EWMA: the min sees through transient
//!     serialization bursts to the standing queue; an EWMA stays inflated
//!     long after the queue drains and causes a backoff spiral).
//!   - Hint-coupled queue target: back off when the windowed min
//!     exceeds floor × {1.08 Realtime, 1.125 Auto, 1.25 Bulk}.
//!   - Two-speed ramp: multiplicative ×1.5+1 per RTT until the first
//!     backoff, then additive +2 / multiplicative ×0.92.
//!   - Token-bucket pacing at cwnd/SRTT with burst allowance max(10, cwnd/8)
//!     (state lives here; the drain in net/mod.rs consumes the tokens).
//!
//! Loss alone does not reduce the window — only a standing queue does.
//! This prevents wireless random loss from collapsing throughput.
//! No ProbeRTT phase (natural oscillation refreshes the floor).
//!
//! Units: `cwnd`, `in_flight`, and pacing tokens are all in symbols.
//! Pacing rate = cwnd [symbols] / SRTT [s] = symbols/second.

pub mod clock;
pub use clock::*;

// The scheduler's process-global gate resolvers live beside the gate
// surface (`crate::gates::scheduler_gates`); re-exported at their old paths.
pub use crate::gates::scheduler_gates::*;
mod copa;
pub use copa::*;
pub(crate) mod place;
// Placement items are crate-internal; only the unit tests name them from here.
#[cfg(test)]
use place::*;
mod path;
pub use path::*;

use crate::control::fec_rate::ProtocolHint;
use crate::control::LossEstimator;
use crate::fec::{FecBackend, WireSymbol};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Identifies a network path (e.g., WiFi, LTE, Ethernet).
pub type PathId = u32;

/// Scheduling weights derived from protocol hint.
/// Controls the latency vs bandwidth trade-off in the interpolated objective
/// (paper §5.7).
#[derive(Debug, Clone, Copy)]
pub struct SchedulingWeights {
    /// Weight for latency cost: SUM(x_i × E_i)
    pub w_lat: f64,
    /// Weight for bandwidth overhead cost: SUM(x_i × r_i)
    pub w_bw: f64,
    /// Weight for the fate-diversity penalty ρ_fate (per-symbol placement,
    /// paper §5.7). Applies to repair symbols only: a repair placed on a path
    /// that already carried the window symbols it covers gains no diversity,
    /// so its marginal cost rises. Zero for source.
    pub w_div: f64,
}

impl SchedulingWeights {
    pub fn from_hint(hint: ProtocolHint) -> Self {
        Self::from_delta(crate::net::delta_price(hint))
    }

    /// The placement weights at any point on the δ dial (paper §5.7).
    ///
    /// ```text
    ///     w_bw(δ) = clamp(½ − ¼·log₁₀(δ/δ_Auto), 0, 1),  w_lat = 1 − w_bw
    /// ```
    ///
    /// The `¼` is the dial's own width: δ spans four decades (0.005 → 50)
    /// while the weights span 1. At the presets `log₁₀(δ/δ_Auto)` is exactly
    /// {2, 0, −2}, so `w_bw` is exactly {0, ½, 1} at Realtime / Auto / Bulk.
    /// Pinned by `scheduling_weights_are_the_dial_not_a_mode`.
    ///
    /// `w_div` is hint-independent: a repair correlated with its coverage is
    /// wasted regardless of the (δ, ρ, r) triangle. Its value 1.0 is
    /// underived (open-constants register, paper §11.2). See `place_symbol`.
    pub fn from_delta(delta_price: f64) -> Self {
        let w_bw = (0.5 - 0.25 * (delta_price.max(1e-12) / raptorpath_math::DELTA_AUTO).log10())
            .clamp(0.0, 1.0);
        Self { w_lat: 1.0 - w_bw, w_bw, w_div: 1.0 }
    }
}

/// Global correction deficit tracker.
///
/// Tracks `deficit = SUM(epsilon_s for un-ACKed symbols)` — the total expected
/// corrections still needed across all paths (paper §5.9).
///
/// Each sent symbol adds `epsilon_i` (loss rate of its path) to the deficit.
/// Each ACKed symbol removes its send-time `epsilon_s` (confirmed survived).
/// Lost corrections add to the deficit, creating the geometric chain that
/// produces `r = epsilon / (1 - epsilon)`.
#[derive(Debug)]
pub struct CorrectionDeficit {
    /// Per-symbol tracking: (seq, path_id, epsilon_at_send)
    pending: VecDeque<(u64, PathId, f64)>,
    /// Running sum of epsilon_s for all pending symbols.
    total: f64,
}

// on_ack / deficit / pending_count / path_deficit have only #[cfg(test)]
// consumers (the deficit-chain law tests in this file); the live path uses
// on_send + on_ack_cumulative.
impl CorrectionDeficit {
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            total: 0.0,
        }
    }

    /// Record a symbol sent on a path with loss rate epsilon.
    pub fn on_send(&mut self, seq: u64, path_id: PathId, epsilon: f64) {
        self.pending.push_back((seq, path_id, epsilon));
        self.total += epsilon;
    }

    /// Acknowledge a symbol (confirmed received). Removes its epsilon from deficit.
    /// Returns true if the symbol was found and removed.
    pub fn on_ack(&mut self, seq: u64) -> bool {
        if let Some(pos) = self.pending.iter().position(|(s, _, _)| *s == seq) {
            let (_, _, eps) = self.pending.remove(pos).unwrap();
            self.total -= eps;
            if self.total < 0.0 {
                self.total = 0.0; // floating point guard
            }
            true
        } else {
            false
        }
    }

    /// Acknowledge all symbols up to and including `up_to_seq` (cumulative ACK).
    pub fn on_ack_cumulative(&mut self, up_to_seq: u64) {
        while self.pending.front().is_some_and(|(s, _, _)| *s <= up_to_seq) {
            let (_, _, eps) = self.pending.pop_front().unwrap();
            self.total -= eps;
        }
        if self.total < 0.0 {
            self.total = 0.0;
        }
    }

    /// Current total correction deficit.
    pub fn deficit(&self) -> f64 {
        self.total
    }

    /// Number of un-ACKed symbols being tracked.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Per-path deficit: sum of epsilon_s for un-ACKed symbols on a specific path.
    pub fn path_deficit(&self, path_id: PathId) -> f64 {
        self.pending
            .iter()
            .filter(|(_, pid, _)| *pid == path_id)
            .map(|(_, _, eps)| eps)
            .sum()
    }
}

/// The multipath scheduler.
///
/// Uses the interpolated objective function (paper §5.7):
///
/// ```text
///   minimize: w_lat × SUM(x_i × E_i) + w_bw × SUM(x_i × r_i)
/// ```
///
/// where E_i is effective delivery time and r_i is correction rate per path.
/// (The block pipeline's block-granular source affinity was removed with it,
/// ADR-0069; the window sender places per symbol.)
pub struct Scheduler {
    paths: HashMap<PathId, PathState>,
    /// The sender-site `[ETA]` gauge (`net/eta.rs`). It lives here and not
    /// in the sender loop because its two feed sites -- the placement
    /// (`net/emit_source.rs`, which picks the path) and the ACK
    /// (`net/control_msg.rs`, which learns what actually happened) -- already
    /// hold this lock and would otherwise never see each other's state.
    ///
    /// Read by no law. `place_costs` writes only its bind
    /// counters; `place_symbol`'s probabilities do not depend on it.
    eta: crate::net::eta::SenderEta,
    /// `(cold_r, cold_ge, evaluations)` accumulated by `place_costs`, which is
    /// `&self` all the way up through `place_symbol` -- hence a `Cell`.
    /// Drained into `eta` at the report cadence by `drain_place_bind`; nothing
    /// on the placement path takes a lock or reads the gauge.
    place_bind: std::cell::Cell<(u64, u64, u64)>,
    /// The placement-arm gauges, same `Cell` discipline and for the same
    /// reason: `place_costs` takes `&self`, and an observation may not need a
    /// write lock the law itself does not need. Drained by
    /// `drain_place_bind`.
    ///
    /// `place_t_gauge` = `(last T_eff, t_cold, n)` - the temperature actually
    /// used, how many resolutions had no measured dispersion anywhere in the
    /// active set (and therefore fell back to the shipped `T`), and the
    /// denominator.
    place_t_gauge: std::cell::Cell<(f64, u64, u64)>,
    /// `place_hol_gauge` = `(s_i > H binds, source cost evaluations,
    /// argmin-moved count, place_costs calls, last W_live)` - the `kappa`
    /// bind fraction and the execution witness that the term actually
    /// changed a decision.
    place_hol_gauge: std::cell::Cell<(u64, u64, u64, u64, f64)>,
    clock: Arc<dyn Clock>,
    /// Global correction deficit tracker (paper §5.9).
    pub deficit: CorrectionDeficit,
    /// Scheduling weights from protocol hint.
    weights: SchedulingWeights,
    /// `RWM_PLACE_T_DERIVED` (placement arm 1) as resolved for this scheduler.
    /// Defaults to the process gate; settable so a unit test can drive both
    /// sides of the arm in one process.
    place_t_derived: bool,
    /// `RWM_PLACE_HOL` (placement arm 2), same discipline.
    place_hol: bool,
    /// `RWM_PLACE_WDIV_DERIVED` (placement arm 3), same discipline.
    place_wdiv_derived: bool,
    /// Protocol hint — also sets Copa-lite's queue target on each path
    /// (paper §8.2).
    hint: ProtocolHint,
    /// `RWM_COLD_PLACE` (anchor-hygiene rule 1 at the placement site) as a
    /// per-scheduler value rather than a hot-path env read: the process-wide
    /// gate resolution cannot hold both arms, so an A/B that measures both
    /// directions in one process (the SF bench's `Place` axis) sets it here.
    /// Resolved from `cold_place_active()` at construction; `set_cold_place`
    /// overrides. See `place_costs`.
    cold_place: bool,
}

impl Scheduler {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self::new_with_hint(clock, ProtocolHint::Auto)
    }

    /// Create scheduler with protocol hint for weight configuration and
    /// the per-path Copa-lite queue target.
    pub fn new_with_hint(clock: Arc<dyn Clock>, hint: ProtocolHint) -> Self {
        Self {
            paths: HashMap::new(),
            eta: Default::default(),
            place_bind: std::cell::Cell::new((0, 0, 0)),
            place_t_gauge: std::cell::Cell::new((0.0, 0, 0)),
            place_hol_gauge: std::cell::Cell::new((0, 0, 0, 0, 0.0)),
            place_t_derived: place_t_derived_active(),
            place_hol: place_hol_active(),
            place_wdiv_derived: place_wdiv_derived_active(),
            clock,
            deficit: CorrectionDeficit::new(),
            weights: SchedulingWeights::from_hint(hint),
            hint,
            cold_place: cold_place_active(),
        }
    }

    /// Override the cold-start placement price for this scheduler
    /// (`RWM_COLD_PLACE`; see the field docs). A/B hook: the process gate is
    /// resolved once, so a bench that scores both arms in one process sets
    /// this instead of racing the environment.
    pub fn set_cold_place(&mut self, enabled: bool) {
        self.cold_place = enabled;
    }

    /// `RWM_PLACE_T_DERIVED` (placement arm 1) for this scheduler.
    pub fn set_place_t_derived(&mut self, enabled: bool) {
        self.place_t_derived = enabled;
    }

    /// Is the derived temperature armed on this scheduler?
    pub fn place_t_derived(&self) -> bool {
        self.place_t_derived
    }

    /// `RWM_PLACE_HOL` (placement arm 2) for this scheduler.
    pub fn set_place_hol(&mut self, enabled: bool) {
        self.place_hol = enabled;
    }

    /// Is the frontier term armed on this scheduler?
    pub fn place_hol(&self) -> bool {
        self.place_hol
    }

    /// `RWM_PLACE_WDIV_DERIVED` (placement arm 3) for this scheduler.
    pub fn set_place_wdiv_derived(&mut self, enabled: bool) {
        self.place_wdiv_derived = enabled;
    }

    /// Is the derived diversity weight armed on this scheduler?
    pub fn place_wdiv_derived(&self) -> bool {
        self.place_wdiv_derived
    }

    /// The cold-start placement price setting in force for this scheduler.
    pub fn cold_place(&self) -> bool {
        self.cold_place
    }

    pub fn add_path(&mut self, id: PathId) {
        self.paths
            .insert(id, PathState::new_with_hint(id, self.clock.clone(), self.hint));
    }

    pub fn remove_path(&mut self, id: PathId) {
        self.paths.remove(&id);
    }

    pub fn path_mut(&mut self, id: PathId) -> Option<&mut PathState> {
        self.paths.get_mut(&id)
    }

    pub fn path(&self, id: PathId) -> Option<&PathState> {
        self.paths.get(&id)
    }

    pub fn active_paths(&self) -> Vec<PathId> {
        self.paths
            .iter()
            .filter(|(_, p)| p.active && p.available() > 0)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Paths that are up, regardless of remaining cwnd budget.
    ///
    /// Use for control-plane traffic (reports, pings, BlockStart) and
    /// congestion bookkeeping. `active_paths()` filters by spare capacity
    /// (for scheduling data); using it for liveness would make a saturated
    /// path invisible: no pings are sent while in_flight >= cwnd, so the peer
    /// would declare the path dead mid-transfer.
    pub fn live_paths(&self) -> Vec<PathId> {
        self.paths
            .iter()
            .filter(|(_, p)| p.active)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Schedule symbols across paths using the interpolated objective.
    ///
    /// Objective (paper §5.7):
    ///
    /// ```text
    ///   minimize: w_lat × SUM(x_i × E_i) + w_bw × SUM(x_i × r_i)
    /// ```
    ///
    /// Source symbols go to paths with lowest weighted cost.
    /// Repair symbols go to paths with highest effective goodput (maximize decode probability).
    ///
    /// Returns: Vec<(PathId, Vec<WireSymbol>)>
    pub fn schedule(
        &mut self,
        source_symbols: Vec<WireSymbol>,
        repair_symbols: Vec<WireSymbol>,
    ) -> Vec<(PathId, Vec<WireSymbol>)> {
        let mut assignments: HashMap<PathId, Vec<WireSymbol>> = HashMap::new();

        let active_paths: Vec<_> = self
            .paths
            .values()
            .filter(|p| p.active && p.available() > 0)
            .collect();

        if active_paths.is_empty() {
            return vec![];
        }

        // Compute per-path cost for source scheduling using interpolated objective.
        // cost_i = w_lat × E_i + w_bw × r_i
        // Lower cost = better path for source symbols.
        let mut path_costs: Vec<(PathId, f64, u32)> = active_paths
            .iter()
            .map(|p| {
                let e_i = p.effective_delivery_time();
                let r_i = p.correction_rate();
                let r_clamped = if r_i.is_infinite() { 10.0 } else { r_i };
                let cost = self.weights.w_lat * e_i + self.weights.w_bw * r_clamped;
                (p.id, cost, p.available())
            })
            .collect();
        path_costs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // Distribute source symbols: lowest-cost paths first, up to each
        // path's spare cwnd budget.
        let mut source_iter = source_symbols.into_iter();
        for &(pid, _, avail) in &path_costs {
            let batch: Vec<_> = source_iter.by_ref().take(avail as usize).collect();
            if batch.is_empty() {
                break;
            }
            assignments.entry(pid).or_default().extend(batch);
        }
        // Overflow to best path
        for sym in source_iter {
            if let Some(&(pid, _, _)) = path_costs.first() {
                assignments.entry(pid).or_default().push(sym);
            }
        }

        // Repair symbols: distribute proportional to effective goodput
        let mut paths_by_goodput: Vec<_> = self
            .paths
            .values()
            .filter(|p| p.active)
            .collect();
        paths_by_goodput.sort_by(|a, b| {
            b.effective_goodput()
                .partial_cmp(&a.effective_goodput())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if !paths_by_goodput.is_empty() {
            let total_goodput: f64 = paths_by_goodput.iter().map(|p| p.effective_goodput()).sum();
            let mut repair_iter = repair_symbols.into_iter().peekable();

            if total_goodput > 0.0 {
                for path in &paths_by_goodput {
                    let fraction = path.effective_goodput() / total_goodput;
                    let count = (fraction * repair_iter.len() as f64).ceil() as usize;
                    let batch: Vec<_> = repair_iter.by_ref().take(count).collect();
                    if !batch.is_empty() {
                        assignments.entry(path.id).or_default().extend(batch);
                    }
                }
            }
            // Remaining repair symbols to best goodput path
            for sym in repair_iter {
                if let Some(path) = paths_by_goodput.first() {
                    assignments.entry(path.id).or_default().push(sym);
                }
            }
        }

        // Charge the in_flight budget at schedule time (the single charge
        // point for symbols placed through this call).
        for (path_id, syms) in &assignments {
            if let Some(path) = self.paths.get_mut(path_id) {
                path.charge_in_flight(syms.len() as u32);
            }
        }

        assignments.into_iter().collect()
    }

    /// Acknowledge received symbols on a path.
    pub fn ack(&mut self, path_id: PathId, count: u32) {
        if let Some(path) = self.paths.get_mut(&path_id) {
            path.release_in_flight(count);
            path.on_ack(count);
        }
    }

    /// Notify the scheduler of a loss event on a path.
    ///
    /// `fec_recovered`: true if the FEC decoder recovered the block despite
    /// the loss (random/wireless loss), false if the block failed to decode
    /// (congestion signal).
    pub fn on_loss(&mut self, path_id: PathId, fec_recovered: bool) {
        if let Some(path) = self.paths.get_mut(&path_id) {
            path.on_loss(fec_recovered);
        }
    }

    /// Record that we received a report/data from a path (keepalive).
    pub fn touch_path(&mut self, path_id: PathId) {
        if let Some(path) = self.paths.get_mut(&path_id) {
            path.last_report = self.clock.now();
            if !path.active {
                tracing::info!(path_id, "path recovered — marking active");
                path.active = true;
                // Reset to startup on recovery (Copa reset keeps the hint's
                // queue target; pacing restarts at the initial burst; the
                // dead path's in-flight budget is gone with it).
                path.cwnd = PathState::INITIAL_CWND;
                path.ssthresh = 64;
                path.in_slow_start = true;
                path.copa.reset();
                path.pace_tokens = PathState::INITIAL_CWND as f64;
                path.last_pace_refill = path.last_report;
                path.in_flight = 0;
                path.in_flight_log.clear();
            }
        }
    }

    /// Check all paths for staleness and deactivate dead ones.
    /// Returns list of path IDs that were deactivated.
    pub fn check_dead_paths(&mut self, timeout: Duration) -> Vec<PathId> {
        let now = self.clock.now();
        let mut deactivated = vec![];
        for path in self.paths.values_mut() {
            if path.active && now.duration_since(path.last_report) > timeout {
                tracing::warn!(path_id = path.id, "path timed out — marking inactive");
                path.active = false;
                deactivated.push(path.id);
            }
        }
        deactivated
    }

    /// Get all path IDs (including inactive).
    pub fn all_path_ids(&self) -> Vec<PathId> {
        self.paths.keys().copied().collect()
    }

    /// Pick the best path for a source symbol: lowest interpolated cost.
    ///
    /// cost_i = w_lat × E_i + w_bw × r_i (paper §5.7)
    pub fn best_source_path(&self) -> Option<PathId> {
        self.paths
            .values()
            .filter(|p| p.active && p.available() > 0)
            .min_by(|a, b| {
                let cost_a = self.path_cost(a);
                let cost_b = self.path_cost(b);
                cost_a.partial_cmp(&cost_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| p.id)
    }

    /// Compute the interpolated scheduling cost for a path.
    fn path_cost(&self, path: &PathState) -> f64 {
        let e_i = path.effective_delivery_time();
        let r_i = path.correction_rate();
        let r_clamped = if r_i.is_infinite() { 10.0 } else { r_i };
        self.weights.w_lat * e_i + self.weights.w_bw * r_clamped
    }

    /// Pick the best path for a repair symbol: highest goodput with available capacity.
    pub fn best_repair_path(&self) -> Option<PathId> {
        self.paths
            .values()
            .filter(|p| p.active && p.available() > 0)
            .max_by(|a, b| {
                a.effective_goodput()
                    .partial_cmp(&b.effective_goodput())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| p.id)
    }

    /// Pick the best repair path, preferring to avoid `avoid` for cross-path diversity.
    /// Falls back to `best_repair_path()` if no alternative exists.
    pub fn best_repair_path_avoiding(&self, avoid: PathId) -> Option<PathId> {
        let alt = self
            .paths
            .values()
            .filter(|p| p.active && p.available() > 0 && p.id != avoid)
            .max_by(|a, b| {
                a.effective_goodput()
                    .partial_cmp(&b.effective_goodput())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| p.id);
        alt.or_else(|| self.best_repair_path())
    }

    /// The sender-site `[ETA]` gauge, mutably -- the placement stamp and the
    /// ack. Observation only; nothing downstream of it feeds a law.
    pub fn eta_mut(&mut self) -> &mut crate::net::eta::SenderEta {
        &mut self.eta
    }

    /// The sender-site `[ETA]` gauge, read-only.
    pub fn eta(&self) -> &crate::net::eta::SenderEta {
        &self.eta
    }

    /// Pick a secondary path for redundant source scheduling (different from primary).
    /// Returns None if only one usable path is available.
    pub fn redundant_source_path(&self, primary: PathId) -> Option<PathId> {
        self.paths
            .values()
            .filter(|p| p.active && p.available() > 0 && p.id != primary)
            .min_by(|a, b| {
                let cost_a = self.path_cost(a);
                let cost_b = self.path_cost(b);
                cost_a.partial_cmp(&cost_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| p.id)
    }

    /// Aggregate spare capacity across all active paths.
    ///
    /// Returns the minimum spare_capacity fraction across active paths,
    /// representing the tightest bottleneck. Used to cap FEC repair rate.
    pub fn spare_capacity(&self) -> f64 {
        self.paths
            .values()
            .filter(|p| p.active)
            .map(|p| p.spare_capacity())
            .fold(f64::INFINITY, f64::min)
    }

    /// Get the minimum max_datagram_size across all active paths that have
    /// reported an MTU. Returns None if no active path has a known MTU.
    pub fn min_mtu(&self) -> Option<usize> {
        self.paths
            .values()
            .filter(|p| p.active)
            .filter_map(|p| p.max_datagram_size)
            .min()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new(Arc::new(WallClock))
    }
}

impl Scheduler {
    /// Set protocol hint (updates scheduling weights and each path's
    /// Copa-lite queue target).
    pub fn set_protocol_hint(&mut self, hint: ProtocolHint) {
        self.weights = SchedulingWeights::from_hint(hint);
        self.hint = hint;
        for path in self.paths.values_mut() {
            path.set_hint(hint);
        }
    }
}

#[cfg(test)]
mod tests;

// ── The one-sided-clamp witness, process-wide (`[LCW]`) ───────────────
//
// `PathState::loss_clamp_witness` carries the per-path counters; these mirror
// them process-wide so a run reads one number off a teardown line instead of
// plumbing `PathState` into the diag renderer. Observation only — nothing
// here is read by a decision, and the clamp (`d_received.min(d_expected)`)
// is untouched.
//
// The hypothesis they score: the sender's symbol counter and the receiver's
// cumulative echo are two clocks, so `d_received > d_expected` whenever the
// receiver's cursor momentarily leads, and the clamp rectifies every such
// sample to zero loss instead of negative loss. Rectifying a zero-mean jitter
// is a positive bias at any path count, including N = 1. The statistic is
// `over_mass / loss_mass`: if rectification is the mechanism, the rectified
// mass is a large fraction of the loss mass the estimator was fed.
pub static LCW_OVER_N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static LCW_OVER_MASS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static LCW_LOSS_MASS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The process-wide one-sided-clamp witness line —
/// `[LCW] over_n=<n> over_mass=<m> loss_mass=<l> rect_frac=<m/l>`.
pub fn lcw_report_line() -> String {
    use std::sync::atomic::Ordering::Relaxed;
    let (n, m, l) = (
        LCW_OVER_N.load(Relaxed),
        LCW_OVER_MASS.load(Relaxed),
        LCW_LOSS_MASS.load(Relaxed),
    );
    let frac = if l == 0 { 0.0 } else { m as f64 / l as f64 };
    format!("[LCW] over_n={n} over_mass={m} loss_mass={l} rect_frac={frac:.4}")
}

/// Reset the process-wide witness — tests only, so one test's samples cannot
/// leak into another's assertion.
pub fn lcw_reset() {
    use std::sync::atomic::Ordering::Relaxed;
    LCW_OVER_N.store(0, Relaxed);
    LCW_OVER_MASS.store(0, Relaxed);
    LCW_LOSS_MASS.store(0, Relaxed);
}
