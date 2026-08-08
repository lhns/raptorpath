//! The window sender's RESOLVE-ONCE policy: the derived constants that fix
//! `run_window_sender`'s behaviour for the lifetime of a tunnel.
//!
//! History (net seam pass 2, 2026-08-09): `run_window_sender` opened with
//! ~1,300 lines of setup that resolved the `RuntimeGates` env surface, the
//! protocol hint and the four pipeline booleans (`reliable`, `coded_only`,
//! `generation`, `systematic`) into ~50 locals which are then never
//! reassigned. Because they were locals, every function extracted out of the
//! sender had to take them as parameters. They are now the fields of
//! [`SenderPolicy`], resolved once, in the `RuntimeGates::resolve()` shape
//! (`src/gates.rs`).
//!
//! BEHAVIOUR CONTRACT: this is a change of WHERE a value is stored, not of
//! what it is. Every field's initializer is the original expression, in the
//! original order, with the original clamp/`unwrap_or` chain and the original
//! gate composition — `RuntimeGates` is still the sole env reader (nothing
//! here calls `std::env`), and the mode-dependent defaults that `gates.rs`
//! deliberately left at the use site (`RWM_GEN_R`, `RWM_REACT_CAP`,
//! `RWM_INFL_BDP`) are resolved here against the SAME mode inputs they were
//! resolved against inline. The mechanism-liveness `info!` echoes stay in
//! `run_window_sender` — they are startup side effects, not policy, and
//! moving them would reorder the log against the `WindowStart` broadcast.
//!
//! NOT covered here (deliberately): anything `run_window_sender` REASSIGNS
//! after setup — the pacing token buckets and their refresh stamps, the
//! derived-depth / dynamic-store-cap caches (`gen_pipe_m`,
//! `gen_pipe_store_cap`, `dyn_store_cap`, `wd_*`, `pa_*`, `ps_*`), the
//! per-path account caps (`percap_caps` / `percap_bounds` / `percap_rr`,
//! refreshed at the dyn-cap cadence), `emit_batch_live` (re-scoped per loop
//! iteration on the live-path count) and the whole DIAG counter set. Those
//! stay locals; the mutable EMISSION state lives in
//! [`SenderState`](super::emit_source::SenderState).

use crate::control::fec_rate::ProtocolHint;

/// Everything `run_window_sender` decides once and then only reads.
///
/// Grouped as the sender itself is: the pipeline shape, the retention/flow
/// -control laws, the generation stack, CC/pacing, the recovery plane, the
/// unified span/shed laws, and the instruments.
#[derive(Debug, Clone)]
pub(crate) struct SenderPolicy {
    // ── Pipeline shape (the caller's mode selection) ─────────────────────
    /// Symbol payload size in bytes.
    pub symbol_size: u16,
    /// The (δ, ρ, r) named point the tunnel was opened at.
    pub protocol_hint: ProtocolHint,
    /// RWM Phase A: RETAIN-UNTIL-ACKED retention at the ARQ layer.
    pub reliable: bool,
    /// Generation-based coding (§16.3): fixed generations of `gen_size`
    /// source symbols, `pipeline` generations concurrently in flight. Turns
    /// per-seq ARQ OFF.
    pub generation: bool,
    /// Systematic + deficit-repair (§16.3 oracle): a submode of `generation`
    /// in which the RAW source rides the wire as PRIMARY.
    pub systematic: bool,
    /// Generation coding emits coded wire symbols exactly like coded-only;
    /// the difference is the coding UNIT and that per-seq ARQ is disabled.
    pub coded_wire: bool,

    // ── Generation stack ─────────────────────────────────────────────────
    /// `RWM_GEN` generation width in source symbols.
    pub gen_size: usize,
    /// Generation-coding proactive overhead r (coded per generation beyond
    /// K_G). Systematic's natural default is smaller than coded-only's
    /// (which must also fund the K base). `RWM_GEN_R` overrides.
    pub gen_repair_floor: f64,

    // ── CC / pacing ──────────────────────────────────────────────────────
    /// Fix 1 (transport-substrate): CC-RATE PACING of the systematic source
    /// against the link-rate token bucket. `RWM_CC_PACE`.
    pub cc_pace: bool,

    // ── Retention / per-path accounting ──────────────────────────────────
    /// feat/store-borrowing (§16.22): a cap-full pick may fly on its picked
    /// pipe while being charged to a sibling account.
    pub percap_borrow_on: bool,
    /// `percap_on || (diag_on && plain_dyn_cap)` — under `RWM_DIAG` the
    /// account maps are maintained as a GAUGE even when the law is off.
    /// Behavior-inert by construction: every percap decision site keys on
    /// `percap_caps` NON-EMPTY (the "law engaged" signal).
    pub percap_track: bool,

    // ── Emission batching (`RWM_EMIT_BATCH`) ─────────────────────────────
    /// Pacer-quantum TUN intake burst size (symbols).
    pub emit_burst: usize,

    // ── The unified span / shed laws (§16.20.3, ADR-0064) ────────────────
    /// `RWM_UNIFIED`: trailing solvable-span placement for plain-mode
    /// proactive repair.
    pub unified_span: bool,
    /// feat/anchor-hygiene (`RWM_ASTAR_ANCHOR`): the windowed-max send-rate
    /// A* anchor instead of the poisonable `est.throughput()` EWMA.
    pub astar_anchor_on: bool,
    /// δ-honest overload shedding (`RWM_UNIFIED_SHED`), armed only on the
    /// EVICT path.
    pub shed_on: bool,
    /// #85 budget-conserving taper emission (`RWM_TAPER_R`).
    pub taper_r_budget: bool,
    /// RWM Phase C (§16.5, the BANDWIDTH knob r): per-symbol repair-rate
    /// FLOOR. 0 = production default (unchanged glide).
    pub repair_rate_floor: f64,

    // ── Instruments ──────────────────────────────────────────────────────
    /// `RWM_DIAG` master gate.
    pub diag_on: bool,
    /// diag/unified-collapse: the span-law trace's own t0.
    pub span_diag_start_us: u64,
}
