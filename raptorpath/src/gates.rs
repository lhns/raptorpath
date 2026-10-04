//! The `RWM_*` environment surface, resolved once per process by [`get`] and
//! read by every consumer; `[GATES]` ([`RuntimeGates::echo_line`]) prints it.
//! Each gate is shipped behaviour, an experiment arm or an instrument; the
//! shipped stack is in `docs/status.md` §1. Fields whose default depends on the
//! generation configuration hold the raw override; the use site applies it.

use crate::config::{anchor_gate, anchor_gate_default, env_flag};

pub mod scheduler_gates;

/// A numeric env value, or `None` when unset, unparseable or non-finite (a NaN
/// would poison every law it reaches).
fn env_parse<T: std::str::FromStr + EnvFinite>(name: &str) -> Option<T> {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse::<T>().ok())
        .filter(|v| v.is_finite_value())
}

/// Finiteness for [`env_parse`]; integers are always finite.
trait EnvFinite {
    fn is_finite_value(&self) -> bool {
        true
    }
}
impl EnvFinite for f64 {
    fn is_finite_value(&self) -> bool {
        self.is_finite()
    }
}
impl EnvFinite for u16 {}
impl EnvFinite for u32 {}
impl EnvFinite for u64 {}
impl EnvFinite for usize {}

/// `RWM_GEN_RATE_FLOOR` resolved against the pace ceiling `gen_rate`
/// (default 2000, bounded to `[1, gen_rate]`).
fn gen_rate_floor(raw: Option<f64>, gen_rate: f64) -> f64 {
    // `f64::max` returns the non-NaN operand, so the upper bound is always a
    // number >= the lower one and `clamp` cannot panic.
    let ceil = gen_rate.max(1.0);
    let floor = raw.unwrap_or(2000.0);
    if floor.is_nan() { 1.0 } else { floor.clamp(1.0, ceil) }
}

/// `RWM_OOO_RETAIN`'s flag half: the variable doubles as the retention depth
/// (`RWM_OOO_RETAIN=16`), so a depth value also arms the decouple.
fn flag_or_depth(name: &str) -> bool {
    match std::env::var(name).ok().and_then(|v| v.trim().parse::<u64>().ok()) {
        Some(depth) => depth > 0,
        None => env_flag(name, false),
    }
}

/// `RWM_DELTA`, the contract's δ override (paper §4.1); see
/// [`RuntimeGates::delta`]. A free function for [`crate::net::delta_price`]'s
/// hot-path callers, which have no `gates` in scope; it reads [`get`].
pub fn delta_override() -> Option<f64> {
    get().delta
}

/// The resolve-time read behind [`delta_override`].
fn resolve_delta() -> Option<f64> {
    env_parse::<f64>("RWM_DELTA").filter(|d| d.is_finite() && *d > 0.0)
}

/// `RWM_COPA_DELTA`: the congestion controller's own δ override, with the
/// domain filter `scheduler::copa_delta` applies (finite, > 0) — out of
/// domain resolves to absent and echoes `unset`, as the CC ignores it.
fn resolve_copa_delta() -> Option<f64> {
    env_parse::<f64>("RWM_COPA_DELTA").filter(|d| d.is_finite() && *d > 0.0)
}

/// The process's [`RuntimeGates`], resolved on first use; the only gate cache,
/// so `[GATES]` prints what behaviour read.
pub fn get() -> &'static RuntimeGates {
    static GATES: std::sync::OnceLock<RuntimeGates> = std::sync::OnceLock::new();
    GATES.get_or_init(RuntimeGates::resolve)
}

/// The engine's env-gate surface, grouped by the part of the machine it gates.
#[derive(Debug, Clone)]
pub struct RuntimeGates {
    // ── The unified machine (ADR-0064) ──
    /// `RWM_UNIFIED` (on, shipped): the unified span machine; `=0` + Realtime = legacy RLC.
    pub unified: bool,
    /// `RWM_UNIFIED_SHED` (on, shipped): δ-honest EVICT shedding within 1 − ρ (paper §5.6).
    pub unified_shed: bool,
    /// `RWM_TAPER_R` (= `unified`, shipped): budget-conserving taper emission (paper §3.3).
    pub taper_r: bool,

    // ── Anchor hygiene (ADR-0061; `RWM_ANCHOR_HYGIENE` umbrella) ──
    /// `RWM_ASTAR_ANCHOR` (on, shipped): windowed-max send-rate A* anchor, clock-gap discard.
    pub astar_anchor: bool,
    /// `RWM_MSTAR_ANCHOR` (on, shipped): measured RTprop floor, fast-seed filter, (M*+2)·G
    /// backstop.
    pub mstar_anchor: bool,
    /// `RWM_PLAIN_RS` (off, arm): plain-mode BBR send-interval sampler (paper §8.3).
    pub plain_rs: bool,
    /// `RWM_HONEST_ANCHOR` (on, shipped): BtlBw windowed max on an O(1) max-deque; `=0` is the same
    /// statistic by an O(window·rate) full fold.
    pub honest_anchor: bool,
    /// `RWM_HONEST_K` (off, arm): `EchoRatioMin` fed the raw echo/RTprop ratio, not SRTT.
    pub honest_k: bool,

    // ── Store / flow control (paper §6) ──
    /// `RWM_STORE_SACK_RELEASE` (on, shipped): SACK frees the slot, keeps the payload (ADR-0060).
    pub store_sack_release: bool,
    /// `RWM_STORE_PATHS` (on, shipped): path-scaled outstanding pool at N ≥ 2 live paths.
    pub store_paths: bool,
    /// `RWM_STORE_PATH_POOL` (default 2048): per-live-path pool knee.
    pub store_path_pool: usize,
    /// `RWM_STORE`: static store override; setting it disables the plain dynamic cap.
    pub store_override: Option<usize>,
    /// Whether `RWM_STORE` is set at all (the dynamic-cap disable keys on presence).
    pub store_env_set: bool,
    /// `RWM_STORE_GAIN` (default 2.0, clamped [1, 64]): window = gain × BDP.
    pub store_gain: f64,
    /// `RWM_STORE_BOOT` (default 128): outstanding cap before the BtlBw anchor warms.
    pub store_boot: usize,
    /// `RWM_HONEST_CAP` (on, shipped; needs `plain_rs`): floor-clock caps on the send anchor.
    pub honest_cap: bool,
    /// `RWM_POOL_ANCHOR` (= `est_cadence`, arm): at N ≥ 2 the pooled cap is Σᵢ honest_store_cap on
    /// each path's send-interval anchor, not the ack-interval max.
    pub pool_anchor: bool,
    /// `RWM_ACK_MERGE` (on, shipped): window mode drops the per-batch `Ack`; its payload rides
    /// `WindowAck`'s cumulative counters.
    pub ack_merge: bool,
    /// `RWM_LOSS_SENT_TRUTH` (off, arm): loss estimator fed the sender's `symbols_sent` delta.
    pub loss_sent_truth: bool,
    /// `RWM_RELEASE_1TO1` (off, arm): lost charges expire at 9/8·SRTT, not by `expected − received`.
    pub release_1to1: bool,
    /// `RWM_CHARGE_RECOVERY` (off, arm): meter SACK-gap retx and NACK repair at wire handoff.
    pub charge_recovery: bool,
    /// `RWM_SIDLE_DERIVED` (off, instrument): `sidle2=`/`idle2=` at `net::stall_threshold_us`.
    pub sidle_derived: bool,

    // ── Placement (paper §5.7) ──
    /// `RWM_COLD_PLACE` (off, arm): price an unmeasured leg at the fastest measured srtt.
    pub cold_place: bool,
    /// `RWM_PLACE_T_DERIVED` (absent, arm): temperature `T = (√6/π)·σ_e/ref_srtt`.
    pub place_t_derived: bool,
    /// `RWM_PLACE_HOL` (absent, arm): source head-of-line cost `[δ·s_i + κ·(s_i − H)+]/ref`.
    pub place_hol: bool,
    /// `RWM_PLACE_WDIV_DERIVED` (absent, arm): repair weight `fate_i·(p_BB,i − ε_i)+·srtt_i/ref`.
    pub place_wdiv_derived: bool,

    // ── Generation stack (paper §5.8) ──
    /// `RWM_GEN` (default 384, min 1): generation size G.
    pub gen_size: usize,
    /// `RWM_PIPELINE` (default 2, min 1): fixed pipeline depth M.
    pub pipeline: usize,
    /// `RWM_GEN_PIPE` (= `unified`, shipped): derived depth M* and dynamic intake cap.
    pub gen_pipe: bool,
    /// `RWM_GEN_R` (unset = 0.15 systematic / 0.20 coded-only): proactive overhead r.
    pub gen_r: Option<f64>,
    /// `RWM_GEN_RATE` (default 9000 sym/s): coded-emission pace ceiling.
    pub gen_rate: f64,
    /// `RWM_GEN_RATE_FLOOR` (default 2000): pacing floor before the first ack-rate sample.
    pub gen_rate_floor: f64,
    /// `RWM_GEN_INFLIGHT` (unset = 2·M·G): in-flight coded allowance W.
    pub gen_inflight: Option<f64>,
    /// `RWM_OOO_RETAIN` set at all: the out-of-order retention decouple.
    pub ooo_retain: bool,
    /// `RWM_OOO_RETAIN` value (≥ 2, default 16): retention depth in generations.
    pub ooo_gens: usize,
    /// `RWM_WINDOW` (default 640): coded-only multipath coding window.
    pub window_override: Option<usize>,
    /// `RWM_REPORT_GENS` (unset = M*+1 under gen_pipe, else 6): generations per report.
    pub report_gens: Option<usize>,
    /// `RWM_REPAIR_WAIT` (ms, unset = 0): wait before a frontier hole fires a NACK.
    pub repair_wait_ms: Option<u64>,
    /// `RWM_CODED_SRC` (off, arm): clock the coded budget on the sent frontier.
    pub coded_src: bool,
    /// `RWM_NO_REACTIVE` (off, arm): disable the reactive loop.
    pub no_reactive: bool,
    /// `RWM_XPATH_REPAIR` (off, arm): route repair to the path with most spare capacity.
    pub xpath_repair: bool,
    /// `RWM_PROACTIVE_PACER` (off, arm): present-at-stall filling-repair pacer.
    pub proactive_pacer: bool,
    /// `RWM_REASM_BDP` (off, arm): never evict an undelivered above-frontier symbol.
    pub reasm_bdp: bool,
    /// `RWM_MIN_R` (default 0, clamped [0, 2]): repair-rate floor (test instrument).
    pub min_r: f64,

    // ── CC / pacing ──
    /// `RWM_CC_PACE` (= `copa_wire`): CC-rate pacing of the systematic source.
    pub cc_pace: bool,
    /// `RWM_CC_PACE_HR` (default 1.1, clamped [1, 2]): pace headroom.
    pub cc_pace_headroom: f64,
    /// `RWM_REACT_CAP` (unset = 1.0 under gen_pipe; < 1 SRTT fraction, ≥ 1 µs): reactive spacing.
    pub react_cap: Option<f64>,
    /// `RWM_INFL_CAP` (default 0 = off): static total in-flight cap.
    pub infl_cap: u64,
    /// `RWM_INFL_BDP` (unset = 1.5 under gen_pipe): BDP in-flight cap gain.
    pub infl_bdp: Option<f64>,
    /// `RWM_COPA_FEED` (off, arm; implied by `RWM_QUIC_CC=passthrough`): Copa feed (ADR-0062).
    pub copa_feed: bool,
    /// `RWM_RS_ATTR` (on, shipped): flight-time witness for cross-path ack attribution.
    pub rs_attr: bool,

    // ── Emission ──
    /// `RWM_EMIT_BATCH` (off, arm): pacer-quantum emission batching, single live path only.
    pub emit_batch: bool,
    /// `RWM_EMIT_BURST` (default 64, clamped [2, 512]): burst quantum in symbols.
    pub emit_burst: usize,

    // ── Recovery plane and store-cap arms (ADR-0059, paper §7) ──
    /// `RWM_RECOV_MP` (on, shipped): per-flight hole law on the flight path's clocks (paper §7.1).
    pub recov_mp: bool,
    /// `RWM_RECOV_MP_LAW` (on, shipped): the per-flight hole-law sub-gate.
    pub recov_mp_law: bool,
    /// `RWM_THREE_TERM` (off, arm; paper §10): `net::three_term_store_cap` as the plain cap.
    pub three_term: bool,
    /// `RWM_COMPOSED_CAP` (off, arm; paper §10): three-term cap plus the per-path cwnd brake.
    pub composed_cap: bool,
    /// `RWM_SUM_CAP` (on, shipped; paper §6.1): pooled cap `gain·Σ`; `=0` is `gain·N·Σ`.
    pub sum_cap: bool,
    /// `RWM_LATE_BRAKE` (off, arm): brake when every live path has `in_flightᵢ ≥ cwndᵢ`.
    pub late_brake: bool,
    /// `RWM_RECOV_SP` (off, arm): single-path RFC 9002 §6.1.2 time-threshold suppression.
    pub recov_sp: bool,
    /// `RWM_DERIVED_SWEEP` (off, arm): recovery clocks at `net::derived_recovery_round_us`.
    pub derived_sweep: bool,
    /// `RWM_DELTA_CAP` (on, shipped; paper §6.1): pooled value multiplier `1 + q(δ)`, q on RFC 8289
    /// §3.2's 5–10 % band; `=0` uses `gain`.
    pub delta_cap: bool,
    /// `RWM_HOLDDOWN_Q` (absent, arm): hold a reported hole `T(q) = W_q(1 − q)` (paper §7.2).
    pub holddown_q: Option<f64>,
    /// `RWM_REFRESH_FLOOR_US` (absent = 25 ms, arm): hole-refresh band floor (paper §7.4).
    pub refresh_floor_us: Option<u64>,
    /// `RWM_DELTA` (absent, arm): the contract's δ as a number (paper §4.1); `RWM_COPA_DELTA`
    /// outranks it for the CC.
    pub delta: Option<f64>,
    /// `RWM_COPA_DELTA` (absent, arm): the CC's own δ, resolved (`RWM_COPA_DELTA` ▸
    /// `RWM_DELTA` ▸ the hint's map, `scheduler::copa_delta`); consumed by the
    /// wire-clocked update law only (`RWM_COPA_WIRE`), whose echo says whether it is on.
    pub copa_delta: Option<f64>,
    /// `RWM_COMPLETION_EXPOSURE` (off, arm): feed χ from a `CompletionFeed` (paper §4.6).
    pub completion_exposure: bool,
    /// `RWM_RECV_REQUEST_LAW` (absent, arm): the receiver requests holes past `l*_recv`; the
    /// SACK→gap producer is suppressed (paper §7.6).
    pub recv_request_law: bool,
    /// `RWM_RANK_FEEDBACK` (absent, arm): requests at `m = clamp(⌈ln 2/−ln π₀⌉, 1, A*)`.
    pub rank_feedback: bool,

    // ── Instruments (ADR-0052; observation only) ──
    /// `RWM_DIAG` (off): the transport-ceiling / recovery-plane `[DIAG]`.
    pub diag: bool,
    /// `RWM_ACKDIAG` (off): per-path ack-cadence gauge (`[ACKDIAG]`).
    pub ackdiag: bool,
    /// `RWM_RTT_DUMP` (off): raw RTT sample stream (`[RTTDUMP]`).
    pub rtt_dump: bool,
    /// `RWM_SUCC_DUMP` (off): raw successor-arrival records beside `[SUCC]`.
    pub succ_dump: bool,
    /// `RWM_WALLDIAG` (off): dead-wall onset and duration (`[WALL]`).
    pub walldiag: bool,
    /// `RWM_CPUPROF` (off): sender CPU split across five seams (`[CPUPROF]`).
    pub cpuprof: bool,
    /// `RWM_RDIAG` (off): engine-receiver saturation probe.
    pub rdiag: bool,
    /// `RWM_FDIAG` (off): proactive-frontier diagnosis.
    pub fdiag: bool,
    /// `RWM_TRACE` (default OFF): generation-lifecycle trace prints.
    pub trace: bool,
    /// `RWM_PFRAC` (default OFF): proactive-vs-reactive recovery fraction.
    pub pfrac: bool,

    // ── Resolved here, not on the `[GATES]` line (own echoes; `EXTERNALLY_ECHOED`) ──
    /// `RWM_COPA_WIRE` / `RWM_QUIC_CC` / `RWM_COPA_FEED`: the wire-clocked
    /// Copa signal (`scheduler::copa_wire_active`).
    pub copa_wire: bool,
    /// `RWM_COPA_COMPETE` (requires `copa_wire`; `scheduler::copa_compete_active`).
    pub copa_compete: bool,
    /// `RWM_EST_CADENCE` (`control::estimator::est_cadence_active`).
    pub est_cadence: bool,
    /// `RWM_WIRE_COMPACT` (`transport::wire_compact_active`).
    pub wire_compact: bool,
    /// `RWM_PLACE_T`: the effective placement temperature.
    pub place_t: f64,
    /// `RWM_CLOCK_GAP`: the process stall witness (`control::anchor`).
    pub clock_gap: bool,
    /// `RWM_RSTAR_TAIL`: r* tail provisioning (`FecRateController`).
    pub rstar_tail: bool,
    /// `RWM_RS_TRACE`: the Copa rate-sample trace threshold (0 = off).
    pub rs_trace: f64,
    /// `RWM_QUIC_CC`, raw (the substrate controller choice; parsed in quic.rs).
    pub quic_cc: Option<String>,
    /// `RWM_MTU_FLOOR`, raw (parsed in quic.rs).
    pub mtu_floor_raw: Option<String>,
    /// `RWM_L0_NETEM`, raw (the L0 shim's scenario list).
    pub l0_netem: Option<String>,
    /// `RWM_L0_SEED`, raw (the L0 shim's RNG seed).
    pub l0_seed_raw: Option<String>,
    /// `RWM_PERF_TIMEOUT_S`, raw (the perf harness's per-run timeout).
    pub perf_timeout_raw: Option<String>,
    /// `RWM_ACKDIAG_WINDOW_US`, resolved (echoed on the `[GATES]` line).
    pub ackdiag_window_us: u64,
    /// `RWM_RTT_DUMP_MAX`, resolved (echoed on the `[GATES]` line).
    pub rtt_dump_max: usize,
    /// `RWM_SUCC_DUMP_MAX`, resolved (echoed on the `[GATES]` line).
    pub succ_dump_max: u64,
}

impl RuntimeGates {
    /// Read the whole gate surface from the environment.
    pub fn resolve() -> Self {
        let unified = crate::config::env_flag("RWM_UNIFIED", true);
        let gen_rate: f64 = env_parse::<f64>("RWM_GEN_RATE").unwrap_or(9000.0);
        // The gates that print their own liveness echo at resolve time, in
        // the order the echoes have always come out.
        let honest_anchor = crate::gates::scheduler_gates::resolve_honest_anchor();
        let honest_k = crate::gates::scheduler_gates::resolve_honest_k();
        let est_cadence = crate::control::estimator::resolve_est_cadence();
        let pool_anchor = crate::gates::scheduler_gates::resolve_pool_anchor(est_cadence);
        let cold_place = crate::gates::scheduler_gates::resolve_cold_place();
        let place_t_derived = crate::gates::scheduler_gates::resolve_place_t_derived();
        let place_hol = crate::gates::scheduler_gates::resolve_place_hol();
        let place_wdiv_derived = crate::gates::scheduler_gates::resolve_place_wdiv_derived();
        let copa_wire = crate::gates::scheduler_gates::resolve_copa_wire();
        let copa_compete = crate::gates::scheduler_gates::resolve_copa_compete(copa_wire);
        RuntimeGates {
            unified,
            unified_shed: env_flag("RWM_UNIFIED_SHED", true),
            taper_r: env_flag("RWM_TAPER_R", unified),
            astar_anchor: anchor_gate_default("RWM_ASTAR_ANCHOR", true),
            mstar_anchor: anchor_gate_default("RWM_MSTAR_ANCHOR", true),
            plain_rs: anchor_gate("RWM_PLAIN_RS"),
            honest_anchor,
            honest_k,
            store_sack_release: env_flag("RWM_STORE_SACK_RELEASE", true),
            store_paths: env_flag("RWM_STORE_PATHS", true),
            store_path_pool: env_parse::<usize>("RWM_STORE_PATH_POOL").unwrap_or(2048),
            store_override: env_parse::<usize>("RWM_STORE"),
            store_env_set: std::env::var("RWM_STORE").is_ok(),
            store_gain: env_parse::<f64>("RWM_STORE_GAIN")
                .unwrap_or(2.0)
                .clamp(1.0, 64.0),
            store_boot: env_parse::<usize>("RWM_STORE_BOOT").unwrap_or(128),
            honest_cap: env_flag("RWM_HONEST_CAP", true),
            pool_anchor,
            ack_merge: crate::gates::scheduler_gates::resolve_ack_merge(),
            loss_sent_truth: crate::gates::scheduler_gates::resolve_loss_sent_truth(),
            release_1to1: crate::gates::scheduler_gates::resolve_release_1to1(),
            charge_recovery: crate::gates::scheduler_gates::resolve_charge_recovery(),
            sidle_derived: crate::gates::scheduler_gates::resolve_sidle_derived(),
            cold_place,
            place_t_derived,
            place_hol,
            place_wdiv_derived,
            gen_size: env_parse::<usize>("RWM_GEN").unwrap_or(384).max(1),
            pipeline: env_parse::<usize>("RWM_PIPELINE").unwrap_or(2).max(1),
            gen_pipe: env_flag("RWM_GEN_PIPE", unified),
            gen_r: env_parse::<f64>("RWM_GEN_R"),
            gen_rate,
            gen_rate_floor: gen_rate_floor(env_parse::<f64>("RWM_GEN_RATE_FLOOR"), gen_rate),
            gen_inflight: env_parse::<f64>("RWM_GEN_INFLIGHT"),
            ooo_retain: flag_or_depth("RWM_OOO_RETAIN"),
            ooo_gens: env_parse::<usize>("RWM_OOO_RETAIN")
                .filter(|&n| n >= 2)
                .unwrap_or(16),
            window_override: env_parse::<usize>("RWM_WINDOW"),
            report_gens: env_parse::<usize>("RWM_REPORT_GENS"),
            repair_wait_ms: env_parse::<u64>("RWM_REPAIR_WAIT"),
            coded_src: env_flag("RWM_CODED_SRC", false),
            no_reactive: env_flag("RWM_NO_REACTIVE", false),
            xpath_repair: env_flag("RWM_XPATH_REPAIR", false),
            proactive_pacer: env_flag("RWM_PROACTIVE_PACER", false),
            reasm_bdp: env_flag("RWM_REASM_BDP", false),
            min_r: env_parse::<f64>("RWM_MIN_R").unwrap_or(0.0).clamp(0.0, 2.0),
            cc_pace: env_flag("RWM_CC_PACE", copa_wire),
            cc_pace_headroom: env_parse::<f64>("RWM_CC_PACE_HR")
                .unwrap_or(1.1)
                .clamp(1.0, 2.0),
            react_cap: env_parse::<f64>("RWM_REACT_CAP"),
            infl_cap: env_parse::<u64>("RWM_INFL_CAP").unwrap_or(0),
            infl_bdp: env_parse::<f64>("RWM_INFL_BDP"),
            copa_feed: env_flag("RWM_COPA_FEED", false),
            rs_attr: env_flag("RWM_RS_ATTR", true),
            emit_batch: env_flag("RWM_EMIT_BATCH", false),
            emit_burst: env_parse::<usize>("RWM_EMIT_BURST")
                .unwrap_or(64)
                .clamp(2, 512),
            recov_mp: env_flag("RWM_RECOV_MP", true),
            recov_mp_law: env_flag("RWM_RECOV_MP_LAW", true),
            three_term: env_flag("RWM_THREE_TERM", false),
            composed_cap: env_flag("RWM_COMPOSED_CAP", false),
            sum_cap: env_flag("RWM_SUM_CAP", true),
            late_brake: env_flag("RWM_LATE_BRAKE", false),
            recov_sp: env_flag("RWM_RECOV_SP", false),
            derived_sweep: env_flag("RWM_DERIVED_SWEEP", false),
            delta_cap: env_flag("RWM_DELTA_CAP", true),
            // Out-of-domain values resolve to absent (echo `unset`): q ≤ 0 is the
            // shipped machine and the window law diverges at q ≥ 1.
            holddown_q: env_parse::<f64>("RWM_HOLDDOWN_Q")
                .filter(|q| q.is_finite() && *q > 0.0 && *q < 1.0),
            // Below the receiver loop's wake granularity the cadence cannot be
            // expressed; above the shipped upper rail it leaves the band.
            refresh_floor_us: env_parse::<u64>("RWM_REFRESH_FLOOR_US").filter(|f| {
                *f >= crate::net::LOOP_WAKE_US
                    && *f <= crate::net::HOLE_NACK_REFRESH_MAX.as_micros() as u64
            }),
            delta: resolve_delta(),
            copa_delta: resolve_copa_delta(),
            completion_exposure: env_flag("RWM_COMPLETION_EXPOSURE", false),
            recv_request_law: env_flag("RWM_RECV_REQUEST_LAW", false),
            rank_feedback: env_flag("RWM_RANK_FEEDBACK", false),
            diag: env_flag("RWM_DIAG", false),
            ackdiag: env_flag("RWM_ACKDIAG", false),
            rtt_dump: env_flag("RWM_RTT_DUMP", false),
            succ_dump: env_flag("RWM_SUCC_DUMP", false),
            walldiag: env_flag("RWM_WALLDIAG", false),
            cpuprof: env_flag("RWM_CPUPROF", false),
            rdiag: env_flag("RWM_RDIAG", false),
            fdiag: env_flag("RWM_FDIAG", false),
            trace: env_flag("RWM_TRACE", false),
            pfrac: env_flag("RWM_PFRAC", false),
            copa_wire,
            copa_compete,
            est_cadence,
            wire_compact: env_flag("RWM_WIRE_COMPACT", true),
            place_t: crate::scheduler::place::resolve_place_temperature(),
            clock_gap: anchor_gate_default("RWM_CLOCK_GAP", true),
            rstar_tail: env_flag("RWM_RSTAR_TAIL", true),
            rs_trace: std::env::var("RWM_RS_TRACE")
                .ok()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0),
            quic_cc: std::env::var("RWM_QUIC_CC").ok(),
            mtu_floor_raw: std::env::var("RWM_MTU_FLOOR").ok(),
            l0_netem: std::env::var("RWM_L0_NETEM").ok(),
            l0_seed_raw: std::env::var("RWM_L0_SEED").ok(),
            perf_timeout_raw: std::env::var("RWM_PERF_TIMEOUT_S").ok(),
            ackdiag_window_us: crate::net::ackdiag::resolve_window_us(
                std::env::var("RWM_ACKDIAG_WINDOW_US").ok().as_deref(),
            ),
            rtt_dump_max: crate::net::rttdump::resolve_dump_max(),
            succ_dump_max: crate::net::succ::resolve_dump_max(),
        }
    }

    /// The `[GATES]` liveness echo: one line naming every gate resolved here
    /// with its resolved value, off values included, so a run's own output
    /// proves which arm it ran (`docs/measurement-discipline.md` rule 15).
    /// Gates resolved elsewhere keep their own echoes and are listed in
    /// `EXTERNALLY_ECHOED`.
    pub fn echo_line(&self) -> String {
        let b = |v: bool| if v { "1" } else { "0" };
        let o = |v: &Option<f64>| v.map_or("unset".to_string(), |x| x.to_string());
        let ou = |v: &Option<usize>| v.map_or("unset".to_string(), |x| x.to_string());
        let o64 = |v: &Option<u64>| v.map_or("unset".to_string(), |x| x.to_string());
        format!(
            "[GATES] RWM_UNIFIED={} RWM_UNIFIED_SHED={} RWM_TAPER_R={} \
             RWM_ASTAR_ANCHOR={} RWM_MSTAR_ANCHOR={} RWM_PLAIN_RS={} \
             RWM_HONEST_ANCHOR={} RWM_HONEST_K={} \
             RWM_STORE_SACK_RELEASE={} RWM_STORE_PATHS={} RWM_STORE_PATH_POOL={} \
             RWM_STORE={} RWM_STORE_GAIN={} RWM_STORE_BOOT={} \
             RWM_THREE_TERM={} RWM_COMPOSED_CAP={} \
             RWM_SUM_CAP={} RWM_LATE_BRAKE={} RWM_DELTA_CAP={} \
             RWM_HONEST_CAP={} RWM_POOL_ANCHOR={} \
             RWM_ACK_MERGE={} RWM_LOSS_SENT_TRUTH={} \
             RWM_RELEASE_1TO1={} RWM_CHARGE_RECOVERY={} \
             RWM_SIDLE_DERIVED={} \
             RWM_COLD_PLACE={} RWM_PLACE_T_DERIVED={} RWM_PLACE_HOL={} \
             RWM_PLACE_WDIV_DERIVED={} \
             RWM_GEN={} RWM_PIPELINE={} RWM_GEN_PIPE={} RWM_GEN_R={} \
             RWM_GEN_RATE={} RWM_GEN_RATE_FLOOR={} RWM_GEN_INFLIGHT={} \
             RWM_OOO_RETAIN={}/{} RWM_WINDOW={} RWM_REPORT_GENS={} \
             RWM_REPAIR_WAIT={} RWM_CODED_SRC={} RWM_NO_REACTIVE={} \
             RWM_XPATH_REPAIR={} RWM_PROACTIVE_PACER={} RWM_REASM_BDP={} \
             RWM_MIN_R={} RWM_CC_PACE={} RWM_CC_PACE_HR={} RWM_REACT_CAP={} \
             RWM_INFL_CAP={} RWM_INFL_BDP={} RWM_COPA_FEED={} RWM_RS_ATTR={} \
             RWM_EMIT_BATCH={} RWM_EMIT_BURST={} RWM_RECOV_MP={} \
             RWM_RECOV_MP_LAW={} RWM_RECOV_SP={} \
             RWM_DERIVED_SWEEP={} RWM_HOLDDOWN_Q={} \
             RWM_REFRESH_FLOOR_US={} RWM_DELTA={} RWM_COPA_DELTA={} \
             RWM_COMPLETION_EXPOSURE={} \
             RWM_RECV_REQUEST_LAW={} RWM_RANK_FEEDBACK={} \
             RWM_DIAG={} RWM_ACKDIAG={} RWM_ACKDIAG_WINDOW_US={} \
             RWM_RTT_DUMP={} RWM_RTT_DUMP_MAX={} \
             RWM_SUCC_DUMP={} RWM_SUCC_DUMP_MAX={} \
             RWM_WALLDIAG={} RWM_CPUPROF={} RWM_RDIAG={} \
             RWM_FDIAG={} RWM_TRACE={} RWM_PFRAC={}",
            b(self.unified), b(self.unified_shed), b(self.taper_r),
            b(self.astar_anchor), b(self.mstar_anchor), b(self.plain_rs),
            b(self.honest_anchor), b(self.honest_k),
            b(self.store_sack_release), b(self.store_paths), self.store_path_pool,
            ou(&self.store_override), self.store_gain, self.store_boot,
            b(self.three_term), b(self.composed_cap),
            b(self.sum_cap), b(self.late_brake), b(self.delta_cap),
            // EFFECTIVE value: the honest-cap law only runs with plain_rs.
            b(self.honest_cap && self.plain_rs), b(self.pool_anchor),
            b(self.ack_merge), b(self.loss_sent_truth),
            b(self.release_1to1), b(self.charge_recovery),
            b(self.sidle_derived),
            b(self.cold_place), b(self.place_t_derived), b(self.place_hol),
            b(self.place_wdiv_derived),
            self.gen_size, self.pipeline, b(self.gen_pipe), o(&self.gen_r),
            self.gen_rate, self.gen_rate_floor, o(&self.gen_inflight),
            b(self.ooo_retain), self.ooo_gens, ou(&self.window_override),
            ou(&self.report_gens),
            self.repair_wait_ms.map_or("unset".to_string(), |v| v.to_string()),
            b(self.coded_src), b(self.no_reactive),
            b(self.xpath_repair), b(self.proactive_pacer), b(self.reasm_bdp),
            self.min_r, b(self.cc_pace), self.cc_pace_headroom, o(&self.react_cap),
            self.infl_cap, o(&self.infl_bdp), b(self.copa_feed), b(self.rs_attr),
            b(self.emit_batch), self.emit_burst, b(self.recov_mp),
            b(self.recov_mp_law), b(self.recov_sp),
            b(self.derived_sweep),
            // Levels and bounds print their resolved value, not a flag: a run's
            // setting must be readable off its own output.
            o(&self.holddown_q),
            o64(&self.refresh_floor_us),
            o(&self.delta),
            o(&self.copa_delta),
            b(self.completion_exposure),
            // Consumed at both endpoints, so echoed at both.
            b(self.recv_request_law), b(self.rank_feedback),
            b(self.diag), b(self.ackdiag), self.ackdiag_window_us,
            b(self.rtt_dump), self.rtt_dump_max,
            b(self.succ_dump), self.succ_dump_max,
            b(self.walldiag), b(self.cpuprof), b(self.rdiag),
            b(self.fdiag), b(self.trace), b(self.pfrac),
        )
    }

    /// Emit the `[GATES]` echo. Call ONCE, right after [`Self::resolve`].
    pub fn echo(&self) {
        tracing::info!("{}", self.echo_line());
    }
}

/// Forwarded `RWM_*` knobs absent from `[GATES]`: each has its own echo or is a
/// harness/L0-sim knob, with the reason the coverage test accepts it.
#[cfg(test)]
const EXTERNALLY_ECHOED: &[(&str, &str)] = &[
    ("RWM_ANCHOR_HYGIENE", "umbrella; folded into the astar/mstar/plain_rs values this line prints"),
    ("RWM_CLOCK_GAP", "own echo: 'clock-gap estimator hygiene ACTIVE' (control/anchor.rs wiring, net/mod.rs)"),
    ("RWM_COPA_WIRE", "own echo: scheduler Copa family resolve"),
    ("RWM_COPA_COMPETE", "own echo: scheduler Copa family resolve"),
    ("RWM_EST_CADENCE", "own echo: 'estimator heavy-math cadence ACTIVE' (control/estimator.rs)"),
    ("RWM_MTU_FLOOR", "own echo: 'MTU floor: …' / 'MTU floor OFF' (transport/quic.rs)"),
    ("RWM_QUIC_CC", "own echo: 'quinn congestion controller: …' (transport/quic.rs)"),
    ("RWM_WIRE_COMPACT", "own echo: compact v5 DATA framing (transport/quic.rs part-2 echo)"),
    ("RWM_RS_TRACE", "instrument: its own [RSTRACE] output IS the echo"),
    ("RWM_RSTAR_TAIL", "r* provisioning knob, read at the tail-provisioning site"),
    ("RWM_PLACE_T", "placement temperature, read by the perf harness path"),
    ("RWM_PERF_TIMEOUT_S", "harness-only: per-run completion timeout (src/perf.rs)"),
    ("RWM_L0_NETEM", "L0 sim harness knob, no engine gate"),
    ("RWM_L0_SEED", "L0 sim harness knob, no engine gate"),
];

#[cfg(test)]
mod forwarding_audit {
    use super::EXTERNALLY_ECHOED;
    use std::collections::BTreeSet;

    /// Every `RWM_*` string literal in the crate source, so an inline
    /// `env::var` at a new site is caught too.
    fn engine_gate_surface() -> BTreeSet<String> {
        fn walk(dir: &std::path::Path, out: &mut String) {
            for e in std::fs::read_dir(dir).expect("read src dir").flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push_str(&std::fs::read_to_string(&p).unwrap_or_default());
                }
            }
        }
        let mut src = String::new();
        walk(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut src,
        );
        let mut out = BTreeSet::new();
        // Match the `"RWM_..."` literal form every gate read uses.
        let bytes = src.as_bytes();
        let mut i = 0;
        while let Some(off) = src[i..].find("\"RWM_") {
            let start = i + off + 1;
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_uppercase() || bytes[end].is_ascii_digit() || bytes[end] == b'_') {
                end += 1;
            }
            if end < bytes.len() && bytes[end] == b'"' {
                let name = &src[start..end];
                // `len > 4` drops the bare `"RWM_"` prefix literal this
                // scraper itself contains; RWM_TEST_* are `config::env_flag`'s
                // own unit-test fixtures, not gates.
                if name.len() > 4 && !name.starts_with("RWM_TEST_") {
                    out.insert(name.to_string());
                }
            }
            i = start;
        }
        assert!(out.len() > 60, "gate scrape found only {} names", out.len());
        out
    }

    /// The harness's `RWM_FORWARD` array in `tools/l1/lib.sh`.
    fn harness_forward_list() -> BTreeSet<String> {
        let lib = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/l1/lib.sh"),
        )
        .expect("tools/l1/lib.sh must exist — it is the single forwarding list");
        let body = lib
            .split_once("RWM_FORWARD=(")
            .expect("lib.sh must define RWM_FORWARD=(")
            .1
            .split_once(')')
            .expect("unterminated RWM_FORWARD array")
            .0;
        body.split_whitespace().map(str::to_string).collect()
    }

    /// An `RWM_*` gate read by the engine but missing from `tools/l1/lib.sh`'s
    /// `RWM_FORWARD` fails here instead of producing a battery arm whose knob
    /// never reaches the wire.
    #[test]
    fn gate_forwarding_list_covers_the_engine_surface() {
        let engine = engine_gate_surface();
        let fwd = harness_forward_list();
        let missing: Vec<_> = engine.difference(&fwd).collect();
        assert!(
            missing.is_empty(),
            "these RWM_* gates are read by the engine but are NOT in \
             tools/l1/lib.sh's RWM_FORWARD, so no L1 driver forwards them \
             explicitly: {missing:?}"
        );
        // The reverse direction keeps the list free of dead knobs.
        let stale: Vec<_> = fwd.difference(&engine).collect();
        assert!(
            stale.is_empty(),
            "RWM_FORWARD names knobs the engine no longer reads: {stale:?}"
        );
    }

    /// Every forwarded gate has a liveness echo, in `[GATES]` or registered in
    /// `EXTERNALLY_ECHOED` (`docs/measurement-discipline.md` rules 1 and 15).
    #[test]
    fn every_forwarded_gate_has_a_liveness_echo() {
        let line = super::RuntimeGates::resolve().echo_line();
        let known: BTreeSet<&str> = EXTERNALLY_ECHOED.iter().map(|(n, _)| *n).collect();
        let unechoed: Vec<_> = harness_forward_list()
            .into_iter()
            .filter(|g| !line.contains(g.as_str()) && !known.contains(g.as_str()))
            .collect();
        assert!(
            unechoed.is_empty(),
            "these gates have NO liveness echo — add them to \
             RuntimeGates::echo_line() or register them in EXTERNALLY_ECHOED \
             with the echo they already own: {unechoed:?}"
        );
    }

    /// The echo is two-sided: it prints off values too, so a battery can
    /// assert both "gate present in the arm" and "gate absent in the control".
    #[test]
    fn the_gates_echo_is_two_sided() {
        let line = super::RuntimeGates::resolve().echo_line();
        assert!(line.starts_with("[GATES] "));
        assert!(
            line.contains("RWM_RECOV_SP=0"),
            "a default-OFF gate must still be NAMED with its 0 value: {line}"
        );
        assert!(
            line.contains("RWM_ACK_MERGE=1"),
            "a default-ON gate must be named with its 1 value: {line}"
        );
        assert!(
            line.contains("RWM_DERIVED_SWEEP=0"),
            "RWM_DERIVED_SWEEP must print its OFF value: {line}"
        );
        // A receiver-law control row is void unless both arms read `=0`.
        assert!(
            line.contains("RWM_RECV_REQUEST_LAW=0"),
            "RWM_RECV_REQUEST_LAW must print its OFF value: {line}"
        );
        assert!(
            line.contains("RWM_RANK_FEEDBACK=0"),
            "RWM_RANK_FEEDBACK must print its OFF value: {line}"
        );
    }
}

#[cfg(test)]
mod tests;
