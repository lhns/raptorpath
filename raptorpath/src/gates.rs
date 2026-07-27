//! Runtime experiment/feature gates — the `RWM_*` environment surface of the
//! window/generation engine, resolved ONCE at engine start.
//!
//! History (code-consolidation pass, 2026-07-27): `net/mod.rs` grew a
//! ~70-env-var gate block read inline mid-function across `run_impl`, the
//! receiver task and `run_window_sender`. This module centralizes the ENV
//! resolution: every gate is read exactly once per engine start
//! (`RuntimeGates::resolve()`), documented in one place with its default and
//! its decision record (ADR / goal-gate section), and the resolved struct is
//! passed to the tasks that consume it. Deprecation warnings
//! (`config::deprecated_env_flag`) fire here, once.
//!
//! Behavior contract: `resolve()` reproduces the exact per-site semantics the
//! scattered reads had (same defaults, same parse/clamp rules, same chaining
//! through `unified_active()` / `copa_wire_active()` / the
//! `RWM_ANCHOR_HYGIENE` umbrella). Fields whose EFFECTIVE default depends on
//! the runtime MODE (generation / fmtcp / systematic — e.g. `RWM_GEN_R`,
//! `RWM_REACT_CAP`, `RWM_INFL_BDP`, `RWM_REPORT_GENS`) store the raw override
//! (`Option<_>`) and the mode-dependent default stays at the use site.
//!
//! NOT covered here (deliberately — each is already a resolve-once site in
//! its own module): the substrate-CC policy `RWM_QUIC_CC` (transport/quic.rs,
//! ADR-0054), the MTU floor `RWM_MTU_FLOOR` (transport, ADR-0055), the Copa
//! wire/δ family `RWM_COPA_WIRE`/`RWM_COPA_DELTA`/`RWM_COPA_COMPETE`
//! (scheduler, cached `OnceLock`, ADR-0062), the stall-witness umbrella
//! member `RWM_CLOCK_GAP` (control/anchor.rs, ADR-0061), the RS trace knob
//! `RWM_RS_TRACE` (scheduler CopaState), and the harness/bench-only knobs
//! (`RWM_L0_*`, `RWM_B_*`, `RWM_SL_*`, `RWM_PERF_TIMEOUT_S`, …).

use crate::config::{anchor_gate, anchor_gate_default, deprecated_env_flag, env_flag};

fn env_parse<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok().and_then(|s| s.parse::<T>().ok())
}

/// The engine's env-gate surface, resolved once at engine start.
///
/// Grouping mirrors the regime map: the unified machine, anchor hygiene,
/// store/flow-control laws, the generation stack, CC/pacing, the recovery
/// plane, and the instruments.
#[derive(Debug, Clone)]
pub struct RuntimeGates {
    // ── The unified machine (ADR-0064) ───────────────────────────────────
    /// `RWM_UNIFIED` (default ON): the one-span-machine default; `=0` is the
    /// legacy three-machine opt-out arm (streaming retirement: register).
    pub unified: bool,
    /// `RWM_UNIFIED_SHED` (default ON): δ-honest overload shedding on the
    /// EVICT path within the derived 1−ρ budget; `=0` = serializing arm.
    pub unified_shed: bool,
    /// `RWM_TAPER_R` (default = `unified`): budget-conserving taper emission
    /// (#85 quantity fix); `=0` = legacy per-ack-cycle accrual (ADR-0063/64).
    pub taper_r: bool,

    // ── Anchor hygiene (ADR-0061; `RWM_ANCHOR_HYGIENE` umbrella) ─────────
    /// `RWM_ASTAR_ANCHOR` (umbrella default ON): windowed-max send-rate A*
    /// anchor with clock-gap discard; engages only under the unified span.
    pub astar_anchor: bool,
    /// `RWM_MSTAR_ANCHOR` (umbrella default ON): measured RTprop floor +
    /// fast-seed rate filter + derived (M*+2)·G win backstop; the plain-live
    /// subset (peer-report RTT-feed suppression) is not generation-gated.
    pub mstar_anchor: bool,
    /// `RWM_PLAIN_RS` (umbrella default OFF): plain-mode BBR send-interval
    /// sampler (sampling-only CopaFeed); the honest-cap law's anchor input.
    pub plain_rs: bool,

    // ── Store / flow-control laws ─────────────────────────────────────────
    /// `RWM_STORE_SACK_RELEASE` (default ON): SACK-clocked slot release —
    /// slot uncounted, recoverability retained (ADR-0060; supersedes the
    /// removed `RWM_SACK_PRUNE`).
    pub store_sack_release: bool,
    /// `RWM_STORE_PATHS` (default ON): path-scaled outstanding pool for
    /// N ≥ 2 live paths; N = 1 keeps the legacy law bit-exactly (ADR-0058).
    pub store_paths: bool,
    /// `RWM_STORE_PATH_POOL` (default 2048): per-live-path pool knee.
    pub store_path_pool: usize,
    /// `RWM_STORE` (unset = mode default): STATIC store/retention override —
    /// setting it disables the plain-mode dynamic BDP cap (the sweep knob).
    pub store_override: Option<usize>,
    /// Whether `RWM_STORE` was SET at all (even unparsable) — the dynamic-cap
    /// disable keys on presence, the value on parse (legacy semantics kept).
    pub store_env_set: bool,
    /// `RWM_STORE_GAIN` (default 2.0, clamped [1, 64]): window = gain × BDP.
    pub store_gain: f64,
    /// `RWM_STORE_BOOT` (default 128): outstanding cap before the BtlBw
    /// anchor warms.
    pub store_boot: usize,
    /// `RWM_STORE_CAPW` (default OFF): capacity-weighted SHARED outstanding
    /// pool — pool = Σ_i honest per-path cap over live paths (each path earns
    /// depth for its OWN pipe + recovery round; one pool, so borrowing stays
    /// free — ADR-0058's pooled verdict kept). The c8-aware pool law
    /// (the ADR-0058 "c8 WATCH" follow-up). Engaged N ≥ 2 with warm anchors;
    /// falls back to the configured pooled law until anchors live. Reads
    /// honestly only with the `RWM_PLAIN_RS` sampler (the battery arm
    /// composes it); with the over-reading legacy anchor it clamps to the
    /// N×knee ceiling ≡ the path-scaled law.
    pub store_capw: bool,
    /// `RWM_STORE_PERCAP` (default OFF): per-path outstanding accounts
    /// (task #86; symmetric-cell tool, c8 successor named — ADR-0058).
    pub store_percap: bool,
    /// `RWM_PERCAP_GUARD` (default ON under percap): delay-aware redirect
    /// bound; `=0` = the measured c8-regression control arm.
    pub percap_guard: bool,
    /// `RWM_STORE_BORROW` (default OFF): bounded account borrowing
    /// (§16.22; loans ≡ 0 at symmetric cells by theorem).
    pub store_borrow: bool,
    /// `RWM_HONEST_CAP` (default ON where `plain_rs` is live): honest
    /// floor-clock store caps on the send-interval anchor (§16.23).
    pub honest_cap: bool,

    // ── Generation stack ──────────────────────────────────────────────────
    /// `RWM_GEN` (default 384, min 1): generation size G.
    pub gen_size: usize,
    /// `RWM_PIPELINE` (default 2, min 1): legacy fixed pipeline depth M.
    pub pipeline: usize,
    /// `RWM_GEN_PIPE` (default = `unified`): derived pipeline depth M* +
    /// dynamic intake cap (ADR-0064 §16.20(d)); `=0` = fixed legacy M arm.
    pub gen_pipe: bool,
    /// `RWM_FMTCP` (default OFF, DEPRECATED-warned): the FMTCP-class
    /// decode-on-total composite. Register: strongest re-test case — retained
    /// pending the piggybacked c8-pool-session arm (ADR-0066).
    pub fmtcp: bool,
    /// `RWM_FMTCP_WIN` (unset = derived/static backstop): explicit win
    /// backstop override; part of the deprecated FMTCP surface (warned).
    pub fmtcp_win: Option<usize>,
    /// `RWM_GEN_R` (unset = mode default 0.10 fmtcp / 0.15 systematic /
    /// 0.20 coded-only; clamped [0, 2] at the use site): proactive overhead r.
    pub gen_r: Option<f64>,
    /// `RWM_GEN_RATE` (default 9000 sym/s): coded-emission pace ceiling.
    pub gen_rate: f64,
    /// `RWM_GEN_RATE_FLOOR` (default 2000, clamped [1, gen_rate]): bootstrap
    /// pacing floor before the ack-rate estimator has a sample.
    pub gen_rate_floor: f64,
    /// `RWM_GEN_INFLIGHT` (unset = 2·M·G): in-flight coded allowance W.
    pub gen_inflight: Option<f64>,
    /// `RWM_OOO_RETAIN` set at all (flag semantics): out-of-order retention
    /// decouple (Fix 3); forced under FMTCP.
    pub ooo_retain: bool,
    /// `RWM_OOO_RETAIN` numeric value (≥ 2, default 16): retention depth in
    /// generations for the decouple.
    pub ooo_gens: usize,
    /// `RWM_WINDOW` (default 640, clamped [MAX_WINDOW_SIZE, 4096] at use):
    /// coded-only W_mp coding-window override (§16.5).
    pub window_override: Option<usize>,
    /// `RWM_REPORT_GENS` (unset = M*+1 under gen_pipe, else 6; clamped
    /// [1, 2000] at use): generations reported per deficit round.
    pub report_gens: Option<usize>,
    /// `RWM_REPAIR_WAIT` (ms; unset/0 = report immediately): repair-coverage
    /// horizon before a frontier hole may fire a reactive NACK.
    pub repair_wait_ms: Option<u64>,
    /// `RWM_CODED_SRC` (default OFF): clock the coded budget on the SENT
    /// frontier instead of the acked frontier (small-G wedge demonstrator).
    pub coded_src: bool,
    /// `RWM_NO_REACTIVE` (default OFF): pure-proactive demonstrator — the
    /// deficit-driven reactive loop disabled entirely.
    pub no_reactive: bool,
    /// `RWM_XPATH_REPAIR` (default OFF; forced under FMTCP): route repair to
    /// the max-spare-capacity path (the C8 fungibility realization).
    pub xpath_repair: bool,
    /// `RWM_PROACTIVE_PACER` (default OFF): present-at-stall filling-repair
    /// pacer — the documented resolution of the removed frontier/inline
    /// family (presence⊥throughput evidence arm; ADR-0066).
    pub proactive_pacer: bool,
    /// `RWM_REASM_BDP` (default OFF; forced under FMTCP): receiver
    /// reassembly clamp — never evict an undelivered above-frontier symbol.
    pub reasm_bdp: bool,
    /// `RWM_MIN_R` (default 0, clamped [0, 2]): per-symbol repair-rate floor
    /// (raise-r test instrument, not a shipped control law).
    pub min_r: f64,

    // ── CC / pacing ───────────────────────────────────────────────────────
    /// `RWM_CC_PACE` (default = `copa_wire_active()`): CC-rate pacing of the
    /// systematic source (paced wire assumption of the Copa model).
    pub cc_pace: bool,
    /// `RWM_CC_PACE_HR` (default 1.1, clamped [1, 2]): pace headroom.
    pub cc_pace_headroom: f64,
    /// `RWM_REACT_CAP` (unset = 1.0 under fmtcp/gen_pipe else OFF; <1 =
    /// fraction of SRTT, ≥1 = absolute µs): bounded-reactive spacing.
    pub react_cap: Option<f64>,
    /// `RWM_INFL_CAP` (default 0 = off): static total in-flight cap.
    pub infl_cap: u64,
    /// `RWM_INFL_BDP` (unset = 1.5 under fmtcp/gen_pipe else off): BDP-derived
    /// in-flight cap gain.
    pub infl_bdp: Option<f64>,
    /// `RWM_COPA_FEED` (default OFF): standalone plain-mode Copa delivery
    /// feed (also implied by `RWM_QUIC_CC=passthrough`) — ADR-0062.
    pub copa_feed: bool,
    /// `RWM_RS_ATTR` (default ON): flight-time witness for cross-path ack
    /// attribution in the sampling-only feed; `=0` = last-sent-path arm.
    pub rs_attr: bool,

    // ── Emission (goal-gate "Emission Batching", 2026-07-27) ──────────────
    /// `RWM_EMIT_BATCH` (default OFF — the A/B arm): pacer-quantum emission
    /// batching on the plain window-reliable sender. Burst TUN intake
    /// (≤ `emit_burst` symbols per loop iteration, inside the flow-control
    /// store headroom and the pacing bucket) + per-burst taper/span-math
    /// refresh (per-symbol when OFF — bit-identical shipped path). Perf-only:
    /// ordering/pacing contracts and the delivered set unchanged.
    pub emit_batch: bool,
    /// `RWM_EMIT_BURST` (default 64 symbols ≈ 64 KB payload — the BBR-style
    /// pacer quantum; clamped [2, 512]): emission burst quantum.
    ///
    /// SENDER-ONLY by measurement: the receiver-arm variants (engine-loop
    /// burst drain ± per-burst ack coalescing, gates `RWM_EMIT_BATCH_RECV`/
    /// `RWM_EMIT_ACK`) were built and REFUTED 2026-07-27 — any engine-
    /// receiver drain collapsed c1 227.6 → 136–144 Mbit/s (echo-RTT
    /// inflation → store-cap growth → spurious retx flood); removed, see
    /// goal-gate "Emission Batching".
    pub emit_burst: usize,

    // ── Recovery plane (ADR-0059) ─────────────────────────────────────────
    /// `RWM_RECOV_MP` (default ON): multipath recovery suppression — per-
    /// flight RFC 9002-style hole law on the flight path's smoothed clocks.
    pub recov_mp: bool,
    /// `RWM_RECOV_MP_LAW` (default ON under the umbrella): the per-flight
    /// hole-law sub-gate (trace attribution).
    pub recov_mp_law: bool,

    // ── Instruments (ADR-0052; no behavior) ───────────────────────────────
    /// `RWM_DIAG` (default OFF): the transport-ceiling / recovery-plane DIAG.
    pub diag: bool,
    /// `RWM_RDIAG` (default OFF): engine-receiver saturation probe.
    pub rdiag: bool,
    /// `RWM_FDIAG` (default OFF): proactive-frontier diagnosis instrument
    /// (retained after the frontier mechanism's removal — ADR-0066).
    pub fdiag: bool,
    /// `RWM_TRACE` (default OFF): generation-lifecycle trace prints.
    pub trace: bool,
    /// `RWM_PFRAC` (default OFF): proactive-vs-reactive recovery fraction.
    pub pfrac: bool,
}

impl RuntimeGates {
    /// Read the whole gate surface from the environment — call once at engine
    /// start. Deprecation warnings (register Class-C gates) fire here.
    pub fn resolve() -> Self {
        let unified = crate::net::unified_active();
        let gen_rate: f64 = env_parse::<f64>("RWM_GEN_RATE").unwrap_or(9000.0);
        RuntimeGates {
            unified,
            unified_shed: env_flag("RWM_UNIFIED_SHED", true),
            taper_r: env_flag("RWM_TAPER_R", unified),
            astar_anchor: anchor_gate_default("RWM_ASTAR_ANCHOR", true),
            mstar_anchor: anchor_gate_default("RWM_MSTAR_ANCHOR", true),
            plain_rs: anchor_gate("RWM_PLAIN_RS"),
            store_sack_release: env_flag("RWM_STORE_SACK_RELEASE", true),
            store_paths: env_flag("RWM_STORE_PATHS", true),
            store_path_pool: env_parse::<usize>("RWM_STORE_PATH_POOL").unwrap_or(2048),
            store_override: env_parse::<usize>("RWM_STORE"),
            store_env_set: std::env::var("RWM_STORE").is_ok(),
            store_gain: env_parse::<f64>("RWM_STORE_GAIN")
                .unwrap_or(2.0)
                .clamp(1.0, 64.0),
            store_boot: env_parse::<usize>("RWM_STORE_BOOT").unwrap_or(128),
            store_capw: env_flag("RWM_STORE_CAPW", false),
            store_percap: env_flag("RWM_STORE_PERCAP", false),
            percap_guard: env_flag("RWM_PERCAP_GUARD", true),
            store_borrow: env_flag("RWM_STORE_BORROW", false),
            honest_cap: env_flag("RWM_HONEST_CAP", true),
            gen_size: env_parse::<usize>("RWM_GEN").unwrap_or(384).max(1),
            pipeline: env_parse::<usize>("RWM_PIPELINE").unwrap_or(2).max(1),
            gen_pipe: env_flag("RWM_GEN_PIPE", unified),
            fmtcp: deprecated_env_flag(
                "RWM_FMTCP",
                false,
                "FMTCP Aggregation Build (2026-07-08) — refuted PRE-wedge-fix/PRE-recov-mp/PRE-divide; re-test REQUIRED before removal",
            ),
            fmtcp_win: {
                let win = env_parse::<usize>("RWM_FMTCP_WIN");
                if win.is_some() {
                    tracing::warn!(
                        "RWM_FMTCP_WIN is deprecated: part of the RWM_FMTCP experiment surface, refuted in \
                         goal-gate \"FMTCP Aggregation Build\" (2026-07-08); removal scheduled pending the \
                         DEPRECATION REGISTER re-test clause"
                    );
                }
                win
            },
            gen_r: env_parse::<f64>("RWM_GEN_R"),
            gen_rate,
            gen_rate_floor: env_parse::<f64>("RWM_GEN_RATE_FLOOR")
                .unwrap_or(2000.0)
                .clamp(1.0, gen_rate),
            gen_inflight: env_parse::<f64>("RWM_GEN_INFLIGHT"),
            ooo_retain: env_flag("RWM_OOO_RETAIN", false),
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
            cc_pace: env_flag("RWM_CC_PACE", crate::scheduler::copa_wire_active()),
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
            diag: env_flag("RWM_DIAG", false),
            rdiag: env_flag("RWM_RDIAG", false),
            fdiag: env_flag("RWM_FDIAG", false),
            trace: env_flag("RWM_TRACE", false),
            pfrac: env_flag("RWM_PFRAC", false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Default-env resolution reproduces the shipped defaults (the ADR-0067
    /// consolidated stack): the CORE laws ON, every experiment gate OFF.
    /// (Set-env semantics are `config::env_flag`'s and are tested there;
    /// integration tests exercise gate activation per feature.)
    #[test]
    fn default_env_resolves_the_shipped_stack() {
        // NOTE: relies on the test env not exporting RWM_* overrides — same
        // assumption every engine-default test in this crate makes.
        let g = RuntimeGates::resolve();
        // CORE (default ON)
        assert!(g.unified && g.unified_shed && g.taper_r);
        assert!(g.astar_anchor && g.mstar_anchor);
        assert!(g.store_sack_release && g.store_paths);
        assert!(g.recov_mp && g.recov_mp_law);
        assert!(g.gen_pipe, "gen_pipe default rides unified_active()");
        // Experiments / instruments (default OFF)
        assert!(!g.fmtcp && !g.store_percap && !g.store_borrow && !g.plain_rs);
        assert!(!g.emit_batch, "emission batching ships OFF (A/B gate)");
        assert_eq!(g.emit_burst, 64);
        assert!(!g.store_capw, "RWM_STORE_CAPW ships default OFF (A/B arm)");
        assert!(!g.proactive_pacer && !g.xpath_repair && !g.no_reactive);
        assert!(!g.diag && !g.rdiag && !g.fdiag && !g.trace && !g.pfrac);
        // Numeric defaults
        assert_eq!(g.gen_size, 384);
        assert_eq!(g.pipeline, 2);
        assert_eq!(g.store_path_pool, 2048);
        assert_eq!(g.store_boot, 128);
        assert!((g.store_gain - 2.0).abs() < 1e-12);
        assert!((g.cc_pace_headroom - 1.1).abs() < 1e-12);
        assert!(g.store_override.is_none() && g.fmtcp_win.is_none());
    }
}
