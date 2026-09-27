//! The scheduler's process-global gate resolvers (the Copa wire/compete
//! family, the anchor-hygiene and placement arms, and the ack/release/
//! charge gates), moved verbatim out of `scheduler/mod.rs` (cleanup
//! Stage 3). Re-exported from `crate::scheduler`, so every path is
//! unchanged.

// (every read below names its crate path in full)

// --- Wire-clocked Copa signal + hint→δ mapping (feat/copa-wire-signal) ---
//
// Task #80 named Copa-sole's bulk gap: the CC's delay term was fed the
// APP-LAYER ECHO RTT, which includes the sender's own store/reservoir dwell
// in quinn's datagram queue — Copa backed off against self-inflicted delay
// that is not in the network (arm D: shrinking the reservoir raised
// throughput +13–23% AND tightened the queue — the self-signal term proven).
// Under the wire signal the CC delay term is quinn's PACKET-TIMED path RTT
// (Connection::rtt — measured at the QUIC packet layer, excludes app store
// dwell), and Copa runs its ACTUAL update law around the target rate
// 1/(δ·d_q) with δ mapped continuously from the protocol hint's latency
// price (see `copa_delta`, paper §12.4). Gated: active only when the engine
// owns/feeds the substrate window (RWM_QUIC_CC=passthrough or
// RWM_COPA_FEED=1); RWM_COPA_WIRE=0 forces the legacy app-echo behavior
// (the #80 A/B arm), =1 forces on. Env fully unset ⇒ OFF ⇒ the shipped
// path is byte-identical.

/// Pure decision function for the wire-signal gate (unit-testable without
/// process-global env state): `qcc` = RWM_QUIC_CC, `feed` = RWM_COPA_FEED
/// as a flag, `wire` = RWM_COPA_WIRE raw value.
pub(crate) fn copa_wire_from_env(qcc: Option<&str>, feed: bool, wire: Option<&str>) -> bool {
    let feed_active = qcc
        .map(|v| v.trim().eq_ignore_ascii_case("passthrough"))
        .unwrap_or(false)
        || feed;
    match wire {
        Some(v) => {
            let v = v.trim();
            !(v.is_empty() || v == "0" || v.eq_ignore_ascii_case("false"))
        }
        None => feed_active,
    }
}

/// Whether the wire-clocked Copa queue signal (+ the δ-mapped update law) is
/// active for this process. Read once and cached — consulted on the ack hot
/// path.
pub fn copa_wire_active() -> bool {
    crate::gates::get().copa_wire
}

/// The resolve-time read behind [`copa_wire_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_copa_wire() -> bool {
    let qcc = std::env::var("RWM_QUIC_CC").ok();
    let wire = std::env::var("RWM_COPA_WIRE").ok();
    let on = copa_wire_from_env(
        qcc.as_deref(),
        crate::config::env_flag("RWM_COPA_FEED", false),
        wire.as_deref(),
    );
    // LIVENESS ECHO (goal-gate "Gate-Forwarding Audit", 2026-08-09):
    // two-sided and composed — this gate's value is DERIVED from three
    // knobs, so the echo prints the inputs beside the result. Resolved
    // once, cached; never on the hot path despite the hot-path readers.
    tracing::info!(
        copa_wire = on,
        quic_cc = qcc.as_deref().unwrap_or("unset"),
        copa_wire_env = wire.as_deref().unwrap_or("unset"),
        "Copa wire-clocked signal (RWM_COPA_WIRE / RWM_QUIC_CC / RWM_COPA_FEED)"
    );
    on
}

/// Pure decision function for the competitive-mode gate: requires BOTH the
/// env flag and the wire-clocked law (the δ adaptation composes with the
/// wire update law; the legacy app-echo dynamics do not consume δ).
pub(crate) fn copa_compete_from_env(compete_flag: bool, wire_active: bool) -> bool {
    compete_flag && wire_active
}

/// Whether Copa's TCP-competitive mode switching is active for this process.
/// Read once and cached (consulted at CopaState construction).
pub fn copa_compete_active() -> bool {
    crate::gates::get().copa_compete
}

/// The resolve-time read behind [`copa_compete_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_copa_compete(wire_active: bool) -> bool {
    let on = copa_compete_from_env(
        crate::config::env_flag("RWM_COPA_COMPETE", false),
        wire_active,
    );
    // LIVENESS ECHO (goal-gate "Gate-Forwarding Audit", 2026-08-09).
    // Two-sided: the "Copa Competitive Mode + Cross-Traffic" battery's
    // arms differ ONLY in this gate, and it composes with copa_wire —
    // so the echo must fire on the OFF arm too, or the control cannot
    // be shown to have been a control.
    tracing::info!(
        copa_compete = on,
        "Copa TCP-competitive mode (RWM_COPA_COMPETE, requires the wire signal)"
    );
    on
}

/// Whether the pool-anchor honest dual-store law is active for this process
/// (`RWM_POOL_ANCHOR`, goal-gate "Ship The Wins 1"): at N ≥ 2 live paths the
/// pooled-store cap's rate input comes from the per-path hygiene-grade
/// SEND-interval anchor ([`crate::control::SendRateAnchor`] — burst-immune
/// by construction, clock-gap discard) instead of the legacy ack-interval
/// windowed-max, whose burst-peak over-read under the est-cadence ack clock
/// was the §16.35 c7 blocker. ONE COMPOSED RESOLUTION: the unset default
/// rides `RWM_EST_CADENCE` (both OFF with everything unset — the measured
/// composed flip REVERTED on its pre-set c7 clause, 2026-08-07; the est=1
/// opt-in turns pool-anchor ON with it), while `RWM_POOL_ANCHOR=0` under
/// the est opt-in is the est-only decomposition arm (the blocker
/// reproduction). Consumers: the per-path send-event feed
/// (`PathState::charge_in_flight`) and the N ≥ 2 dyn-cap law in net/mod.rs.
/// The Copa cwnd feed (`record_delivery`/`on_ack`) is deliberately
/// UNTOUCHED — the measured −22…−27 c7 RS-composition price stays
/// unreachable. Read once and cached (consulted on the send hot path).
pub fn pool_anchor_active() -> bool {
    crate::gates::get().pool_anchor
}

/// The resolve-time read behind [`pool_anchor_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_pool_anchor(est_cadence: bool) -> bool {
    crate::config::env_flag(
        "RWM_POOL_ANCHOR",
        est_cadence,
    )
}

/// Whether the O(1) windowed-max rate filter is active for this process
/// (`RWM_HONEST_ANCHOR`, goal-gate "Honest Inputs" — anchor-hygiene family
/// member, **DEFAULT ON since 2026-08-11** per the flip battery's F7 and
/// paper §16.51; `=0` is the re-runnable legacy-fold A/B arm, and the
/// `RWM_ANCHOR_HYGIENE` umbrella still overrides in either direction).
///
/// THE MECHANISM IT REPAIRS (measured, not argued): `CopaState`'s BtlBw
/// windowed max (`max_bw`) is recomputed by a FULL-WINDOW FOLD over
/// `bw_samples` on every accepted sample. Fed per-ACK (the legacy
/// `record_delivery`) the fold is invisible; fed PER DELIVERED SOURCE
/// SYMBOL (`rs_on_delivered` under `RWM_PLAIN_RS`) it is O(window·rate)
/// work per second of transfer — a hidden O(n²), and the EXACT defect the
/// `rtt_samples` min-deque already fixed for min_rtt (see `record_rtt`'s
/// monotonic-deque comment: "~42% sender CPU ... MEASURED by perf"). The
/// latency-lever battery's CPU gauge convicts it at c1: `RWM_PLAIN_RS=1`
/// alone inflates sender CPU per delivered byte by +61…64% (CPUCLI
/// 15.0–16.6 s → 24.2–25.4 s for the same 400 MB, 16/16 reps, both seeds)
/// on a sender already at its ~1-core ceiling — which is the whole
/// −35% / D/A 0.64, and why the tax is rate-dependent (fold length ∝ rate)
/// and anti-correlated with store binding (it is not a store effect at
/// all).
///
/// ON ⇒ `max_bw` is read off a monotonic max-deque maintained beside
/// `bw_samples` — the SAME statistic to the bit (front of the deque ==
/// the fold; unit-pinned by `bw_mono_front_equals_full_window_fold`), the
/// same [1 s, 10 s] window, the same evictions, amortized O(1) per sample.
/// ZERO constants: nothing is sampled, subsetted, decayed or approximated.
/// OFF ⇒ the fold runs verbatim (value-identical either way; the gate
/// selects COST, not behavior). Read once and cached (consulted at
/// CopaState construction).
///
/// **DEFAULT ON since 2026-08-11** (goal-gate "Honest Inputs — FLIP
/// BATTERY", falsifier F7 swept: goodput within 2σ at every cell/seed,
/// CPU/byte 0.90–1.03×; value-identical by the unit-pinned equivalence, so
/// any behavioral movement is an instrument alarm, not a result). The
/// legacy fold remains reachable as `RWM_HONEST_ANCHOR=0` — the A/B arm
/// stays re-runnable per the deprecation register.
pub fn honest_anchor_active() -> bool {
    crate::gates::get().honest_anchor
}

/// The resolve-time read behind [`honest_anchor_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_honest_anchor() -> bool {
    crate::config::anchor_gate_default("RWM_HONEST_ANCHOR", true)
}

/// **`RWM_COLD_PLACE`** (anchor-hygiene family member, default OFF) — hygiene
/// rule 1 at the PLACEMENT site: an unmeasured leg's latency anchor is seeded
/// from MEASUREMENT, not from the 50-ms constant.
///
/// THE DEFECT IT REPAIRS: `place_costs`' load term reads
/// `PathState::srtt()`, which for a leg that has never had an RTT sample is
/// `estimator.rtt()` — still the 50-ms `DEFAULT_SRTT`-class constructor seed.
/// That prices a COLD leg's one-way propagation at 25 ms against a warm c2
/// leg's 4 ms, so the incumbents must reach `in_flight/cwnd ≈ 2.6` before the
/// cold leg can win the argmin. It draws nothing, so it takes no sample, so
/// it stays cold: a FIXED POINT of the estimator.
///
/// **WHERE IT BINDS, AND THE RETRACTION THAT ESTABLISHED THAT.** This was
/// first claimed at the SF bench's `c7x4` symmetric quad, and that claim is
/// RETRACTED (goal-gate "The Quad's Cold-Start Placement Lock-In —
/// RETRACTED", 2026-08-18): the quad's per-path gauges were truncated at
/// `pid < 2`, so the assertion that "measured" the lock-in could not fail,
/// and the quad in fact spreads evenly over all four legs. The reason is
/// mechanical and worth stating, because it bounds this gate's whole scope:
/// when every leg starts cold TOGETHER, the first admission burst runs before
/// any ack returns, all legs tie at the seed price, the `in_flight` term
/// round-robins them, and one RTT later they are all warm — the cold price
/// never gets a cold-vs-warm contrast to express.
///
/// The fixed point therefore forms only where a leg joins a set whose
/// incumbents are ALREADY warm — a LATE JOIN (path migration, a second
/// interface coming up mid-transfer). No SF-bench geometry and no L1 cell has
/// one, so this gate is bounded by
/// `a_late_joining_leg_is_locked_out_by_the_cold_price_and_admitted_without_it`
/// at synthetic states, and measured INERT at every bench cell by
/// `the_cold_start_placement_price_is_inert_wherever_every_leg_starts_cold`.
/// That is why it ships OFF and why no flip is recommended: the only regime
/// it changes has never been measured on a wire.
///
/// THE REPAIR, and why it costs no constant: the cold leg is priced at the
/// path set's own FASTEST MEASURED srtt. The price is another leg's
/// measurement, not a number — the same move `RWM_MSTAR_ANCHOR` makes inside
/// `LossEstimator::record_rtt` (seed from the first sample) and
/// `RWM_HONEST_K` makes for K (`k_raw.unwrap_or(legacy)`): ONE formula, the
/// gate only changes WHICH measurement seeds the unmeasured anchor. It is
/// the standard optimistic-exploration argument stated in the placement
/// objective's own units — exploration is free until measurement says
/// otherwise — and it is SELF-LIMITING without a threshold, because the
/// cold leg's `in_flight/cwnd` term starts charging the moment it is placed
/// on. No `if cold` beyond the `Option::None` the estimator already has, no
/// dial threshold, no round-robin counter.
///
/// OFF is bit-identical by construction: with the gate off the cold price IS
/// `p.srtt()`, i.e. the shipped expression verbatim at every leg.
/// A placement arm's flag: [`crate::config::env_flag`], default ABSENT. A
/// value outside the strict boolean dialect (`RWM_PLACE_HOL=of`) is a startup
/// error naming the gate, so a typo can never run a control row labelled as
/// a challenger.
pub(crate) fn place_arm_flag(name: &str) -> bool {
    crate::config::env_flag(name, false)
}

/// **`RWM_PLACE_T_DERIVED`** (Track A arm 1, ABSENT by default) - the
/// placement softmax temperature read as the Luce/Gumbel scale of the
/// scheduler's own ETA prediction error (paper 16.81.1):
///
/// ```text
///     T  =  (sqrt(6)/pi) * sigma_e / ref_srtt  =  0.77970 * sigma_e / ref
/// ```
///
/// The softmax IS `argmin` under i.i.d. Gumbel noise of scale `T` (the Luce
/// choice rule), so matching `Var(T*G) = pi^2 T^2/6` to `sigma_e^2` fixes `T`
/// with no free parameter. `sigma_e` is the dispersion of
/// `e = realized - predicted`, already maintained per path by the sender
/// `[ETA]` gauge's tau-lag estimator (`net::eta::SenderEta::sigma_us`), pooled
/// as an RMS over the ACTIVE candidate set.
///
/// **The shipped `0.15` is thereby a falsifiable claim about the wire**:
/// `T = 0.15 <=> sigma_e = 0.19238 * ref` at every cell. This gate does not
/// correct it - it makes the derived form runnable beside it. A `T_sigma` arm
/// that TIES with `CTL` licenses `sigma_e/ref`, never `0.15`.
///
/// OFF is byte-identical by construction: `place_temperature()` verbatim, which
/// the pinned cost/probability table asserts.
pub fn place_t_derived_active() -> bool {
    crate::gates::get().place_t_derived
}

/// The resolve-time read behind [`place_t_derived_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_place_t_derived() -> bool {
    let on = place_arm_flag("RWM_PLACE_T_DERIVED");
    // LIVENESS ECHO, TWO-SIDED (MEASUREMENT DISCIPLINE item 1): the OFF
    // value prints too, so "gate absent" is as checkable as "gate present".
    tracing::info!(
        place_t_derived = on,
        "placement temperature (RWM_PLACE_T_DERIVED, paper 16.81.1): \
         T = (sqrt6/pi)*sigma_e/ref from the sender's own ETA-error \
         dispersion when ON; the shipped place_temperature() when OFF"
    );
    on
}

/// **`RWM_PLACE_HOL`** (Track A arm 2, ABSENT by default) - the frontier
/// (head-of-line) term the placement law is missing, 16.80.3's `Phi` physics
/// read at the SENDER and added to `cost_i` for SOURCE symbols:
///
/// ```text
///     X_i  =  [ delta*s_i  +  kappa*(s_i - H)+ ] / ref
///     s_i  =  [ (now + E_i) - F_hat ]+          the frontier push this placement adds
///     F_hat  =  running max of the stamped ETAs of symbols already placed
///     H      =  the free headroom of the LIVE store-cap law
/// ```
///
/// plus 16.80.6(a2)'s wire-price ORDERING term at its derived `W`, which
/// prices the opposite sign of the same difference:
///
/// ```text
///     O_i  =  W * [ F_hat - (now + E_i) ]+ / ref
///     W    =  1 symbol / ( R_ref * tau_cool )        no literal; see place_hol_wire_price
/// ```
///
/// A placement that lands BEHIND the frontier pushes nothing and is FREE
/// (`s_i = 0` exactly) - the property that makes `X_i` a water-filling
/// incentive rather than a slow-path penalty. `delta` is read from
/// `net::delta_price`, so the term is continuous in the dial and no hint is
/// ever compared. `kappa = 1` is a DECLARED UPPER BOUND, not a value (D0's
/// own fits put it at 0.0048-0.067), chosen conservative in the direction that
/// DISCOURAGES frontier-pushing placements; its bind is gauged (`s_i > H`).
///
/// OFF is byte-identical by construction: the term is not merely zero, the
/// whole frontier read is skipped and the shipped sum is returned unchanged.
pub fn place_hol_active() -> bool {
    crate::gates::get().place_hol
}

/// The resolve-time read behind [`place_hol_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_place_hol() -> bool {
    let on = place_arm_flag("RWM_PLACE_HOL");
    tracing::info!(
        place_hol = on,
        "placement frontier term (RWM_PLACE_HOL, paper 16.81.2): \
         X_i = [delta*s_i + kappa*(s_i-H)+]/ref with s_i the frontier push \
         against the sender's own F_hat, plus the (a2) wire-price ordering \
         term at its derived W; ABSENT leaves the shipped cost untouched"
    );
    on
}

/// **`RWM_PLACE_WDIV_DERIVED`** (Track A arm 3, ABSENT by default) - the
/// repair diversity weight read off the channel's own burst persistence
/// instead of the `w_div = 1.0` literal (paper 16.81.3):
///
/// ```text
///     V_i  =  fate_i * ( p_BB,i - eps_i )+ * srtt_i / ref
///     p_BB  =  1 - p_bg      Gilbert-Elliott bad->bad persistence
///     eps   =  the path's marginal loss rate
/// ```
///
/// What a correlated repair costs is the EXCESS probability that one burst
/// takes both, and that excess VANISHES on a memoryless channel
/// (`p_BB = eps`) - which the shipped `1.0` does not. REPAIRS ONLY: `fate_i`
/// is identically zero for source symbols, so this arm cannot move a source
/// placement, and the pinned table's source rows are untouched even with it
/// armed. Cold rule: a path whose GE estimator is not yet valid keeps the
/// shipped `w_div * fate_i` and is already counted by the `cold_ge` bind
/// gauge.
pub fn place_wdiv_derived_active() -> bool {
    crate::gates::get().place_wdiv_derived
}

/// The resolve-time read behind [`place_wdiv_derived_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_place_wdiv_derived() -> bool {
    let on = place_arm_flag("RWM_PLACE_WDIV_DERIVED");
    tracing::info!(
        place_wdiv_derived = on,
        "placement diversity weight (RWM_PLACE_WDIV_DERIVED, paper 16.81.3): \
         fate*(p_BB - eps)+ * srtt/ref from the path's Gilbert-Elliott \
         burst persistence when ON; the shipped w_div*fate when OFF"
    );
    on
}

pub fn cold_place_active() -> bool {
    crate::gates::get().cold_place
}

/// The resolve-time read behind [`cold_place_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_cold_place() -> bool {
    let on = crate::config::anchor_gate("RWM_COLD_PLACE");
    // LIVENESS ECHO (MEASUREMENT DISCIPLINE item 1/15), two-sided: it
    // prints the OFF value too, so "gate absent" is as checkable as
    // "gate present". Resolved once and cached.
    tracing::info!(
        cold_place = on,
        "cold-start placement price (RWM_COLD_PLACE, anchor-hygiene rule 1): \
         an unmeasured leg's SRTT_i in the §16.3 cost is the active set's \
         fastest MEASURED srtt when ON, the 50-ms DEFAULT_SRTT-class seed \
         when OFF (shipped, bit-identical)"
    );
    on
}

/// Whether the RAW-sample echo-ratio floor is active for this process
/// (`RWM_HONEST_K`, goal-gate "Honest Inputs" — anchor-hygiene family
/// member, default OFF; `RWM_ANCHOR_HYGIENE=1` turns the family on).
///
/// THE MECHANISM IT REPAIRS: K_i (`EchoRatioMin`, the honest caps' and the
/// three-term law's residence-clock ratio) is documented as "the smallest
/// OBSERVED echoSRTT/RTprop" but is fed the SMOOTHED srtt series sampled at
/// the 5 ms dyn-cap refresh clock. The minimum of a smoothed series sits
/// near the MEAN of the underlying distribution, not its floor — the EWMA
/// (α = 1/8) filters out exactly the low excursions a windowed MIN exists
/// to catch — so K READS HIGH wherever the delay distribution is wide:
/// jit25's `[3T]` window term measured ×1.34/1.38 its pre-registered value,
/// the INVERSE of the pre-registered "min reads the low end" direction
/// (goal-gate "Latency Lever — BATTERY", banked as an `EchoRatioMin`
/// finding). RTprop, by contrast, is already the min over RAW samples —
/// the current K is min(smoothed)/min(raw), a statistic that rises with
/// jitter width by construction.
///
/// ON ⇒ the SAME `EchoRatioMin` tracker (same `PERCAP_K_HALF_WINDOW_US`
/// window, same ≥ 1 clamp, same seed-identity guard) is fed the RAW
/// per-sample echo/RTprop ratio at the SAMPLE clock (`record_rtt`), and
/// every K consumer reads that tracker's min — min(raw)/min(raw), the
/// floor the derivation assumed. ZERO constants: the fix changes which
/// measured series feeds the unchanged statistic. OFF ⇒ the smoothed
/// refresh-clock feed runs verbatim. Read once and cached (consulted at
/// CopaState construction).
pub fn honest_k_active() -> bool {
    crate::gates::get().honest_k
}

/// The resolve-time read behind [`honest_k_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_honest_k() -> bool {
    crate::config::anchor_gate("RWM_HONEST_K")
}

/// Whether the WINDOW-mode control-datagram MERGE is active for this process
/// (`RWM_ACK_MERGE`, goal-gate "Unlock The Default 1: ack-merge" →
/// "Ack-Merge Flip"; **default ON since 2026-08-08** — `RWM_ACK_MERGE=0` is
/// the opt-out A/B arm).
///
/// The receiver emits up to TWO control datagrams per data message: the SACK
/// `WindowAck` from the window arm, and the legacy per-batch
/// `ControlMessage::Ack` whose send site sits AFTER the window/block branch
/// and therefore fires in window mode too (the recorded code-fact correction
/// at `net/mod.rs`'s Ack arm). quinn-perf sends ~1 ack per ~24 packets.
///
/// **How much of a duplicate it is depends on the CELL, and that is the whole
/// measured story (§16.42).** The `Ack` fires once per symbol batch
/// unconditionally; the `WindowAck` it duplicates fires on FRONTIER ADVANCE.
/// So on a clean single path, where the in-order frontier advances on
/// essentially every batch, the two coincide and the receiver really does
/// send ≈2.0 control datagrams per data message — **measured 1.96 at c1**.
/// Under dual-path striping with GE loss the frontier advances in jumps of
/// ~20–25 seqs, the `WindowAck` rate collapses, and the "duplicate" is
/// ≈4% of the traffic — **measured 1.05 at c7**. §16.39 measured only the
/// dual cell and concluded the premise was refuted; it was refuted THERE and
/// exactly right at the clean cell.
///
/// ON ⇒ 1.000 per data message everywhere, and the goodput/CPU response
/// tracks the density REMOVED, cell by cell: c1 (1.96 → 1.00) +12.7% / +13.0%
/// on the two seeds with receiver CPU per bit −9.1% / −8.4%; c7 (1.05 → 1.00)
/// −0.7% / −0.2% with receiver CPU flat, i.e. within σ of its own control.
///
/// ON ⇒ in WINDOW MODE ONLY the legacy `Ack` is suppressed, the `WindowAck`
/// becomes unconditional (one per data message — exactly the cadence the
/// `Ack` had) and carries the `Ack`'s payload in its v6 cumulative counters,
/// and every consumer of the `Ack` arm is re-homed onto the counter DIFF.
/// BLOCK MODE IS BIT-EXACT: it keeps the legacy `Ack` in full, and
/// `block_arq` is already `None` in window mode so the dup-ack loss channel
/// is structurally out of scope.
///
/// **This gate changes the DATAGRAM COUNT and nothing else.** The delivery
/// statistic (`record_delivery`'s ack-interval windowed max), its cadence,
/// its counts and its consumers are all preserved — deliberately, because
/// with no `CopaFeed` constructed (the shipped default and every arm of the
/// ack-merge battery) that estimator IS the window-mode anchor, and removing
/// it is the measured catastrophic trap recorded at the Ack arm
/// (`max_bw = 0` ⇒ the anchor floor never establishes ⇒ the dynamic store cap
/// sticks at boot 128). Replacing the anchor is a DIFFERENT experiment; three
/// rate sources have already been measured against it (§16.35/§16.36/§16.37)
/// and the c7 ordering did not track anchor honesty.
///
/// Not a dial: it selects no law and no constructor argument on (δ, ρ, r),
/// and nothing keys on a threshold in the triangle (CLAUDE.md's
/// no-mode-switch invariant). The machine is bit-identical under both
/// settings; only the number of control frames differs.
pub fn ack_merge_active() -> bool {
    crate::gates::get().ack_merge
}

/// The resolve-time read behind [`ack_merge_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_ack_merge() -> bool {
    crate::config::env_flag("RWM_ACK_MERGE", true)
}

/// `RWM_LOSS_SENT_TRUTH` (**default OFF**) — feed the per-path loss estimator
/// the SENDER's own `symbols_sent` delta instead of the receiver's
/// gap-derived `total_expected`. The law, its provenance and its named
/// residual are on [`PathState::sender_truth_loss_delta`]; the defect it
/// removes is documented at the `PathBatchTracker` design note
/// (`net/mod.rs` header item (2)) and measured in goal-gate "Ack-Cadence
/// Measurement (VM)" READOUT 4.
///
/// **Behaviour-changing, hence gated.** The estimate feeds the NACK repair
/// margin (`net/mod.rs:6867`), the NACK congestion multiplier and budget cap
/// (`:6384`/`:6432`), the block-ARQ margins via `worst_loss_rate`
/// (`:7613`), the interleaver taper decay (`:7344`), the shed budget
/// (`emit_source.rs:682`, `receiver.rs:767`/`:1398`) and every placement /
/// scheduling cost that carries an `eps` term (`scheduler/mod.rs:2212`,
/// `:2229`, `:2256`, `:2266`, `:3111`). N = 1 is UNAFFECTED in shape — a
/// single path's batch-seq stream has no other path in it, so the legacy
/// pair is already honest there and this gate only removes its ~1 BDP of
/// startup lag.
///
/// **Not the refuted `RWM_RECOV_MP_SERIAL`.** That build gave each path its
/// own batch-seq NAMESPACE on the WIRE (sender-side, protocol-visible) and
/// was runtime-refuted on the clean substrate (dual-c1 181 → 134, sender CPU
/// x2.4 — goal-gate "Multipath Recovery Suppression", DEPRECATION REGISTER).
/// This changes NO wire format and adds no sender work: both operands
/// already exist and already ride the existing v6 counters. The refutation's
/// mechanism — honest loss re-heating every SRTT/loss-scaled recovery
/// cadence that the poisoned values were accidentally damping — applies to
/// ANY honest-loss build and is exactly why this one ships OFF pending the
/// named cadence re-derivation.
///
/// Not a dial: it selects no law on (delta, rho, r) and nothing keys on a
/// threshold in the triangle (CLAUDE.md's no-mode-switch invariant). It
/// changes which MEASUREMENT feeds one estimator; the laws downstream are
/// the same laws, evaluated at an honest argument.
pub fn loss_sent_truth_active() -> bool {
    crate::gates::get().loss_sent_truth
}

/// The resolve-time read behind [`loss_sent_truth_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_loss_sent_truth() -> bool {
    crate::config::env_flag("RWM_LOSS_SENT_TRUTH", false)
}

/// `RWM_RELEASE_1TO1` (**default OFF**) — MAKE THE RELEASE 1:1 WITH THE
/// CHARGE. One gate, one quantity: **what releases a LOST symbol's budget
/// slot.**
///
/// Today the answer is two mechanisms, and the first of them is contaminated:
///
/// 1. `control_msg.rs` releases `expected_count - received_count` in BOTH ack
///    arms, where `expected` is `PathBatchTracker`'s GLOBAL-`batch_seq` gap
///    estimate `gap x received` (`net/mod.rs`'s `PathBatchTracker::
///    record_batch`). At N >= 2 that gap is a SCHEDULING artefact — mostly the
///    OTHER path's symbols — so the release is inflated by the same 37-93x the
///    loss estimate was (goal-gate "Cross-Path Loss Contamination" READOUT 4:
///    `ce/cr` 2.05 at c7, 5.59 on c8's slow leg = **~1 and ~5 EXTRA slots
///    released per delivered symbol**). `release_in_flight` saturates at zero,
///    so the excess is spent, not stored: the gauge does not merely
///    mis-report, it **leaks OPEN**. Measured on the deterministic two-path
///    model, the gauge reads `in_flight == 0` on **> 90%** of acks at which
///    the path genuinely has symbols outstanding, which holds
///    `available() = cwnd - in_flight` wide open on evidence the path does not
///    have.
/// 2. [`PathState::expire_in_flight`], a time-based sweep of the charge log
///    itself. This one IS 1:1 by construction — it pops the very entries
///    `charge_in_flight` pushed — but its horizon is
///    `max(4 x SRTT, 250 ms)`, roughly an order of magnitude past the RTT
///    scale at which a symbol's fate is actually decided, so on the shipped
///    path it is a backstop and (1) is the operative release.
///
/// **Under the gate, (1) is DELETED and (2) becomes the whole answer, at the
/// scale the engine already uses to decide a symbol IS lost:** RFC 9002
/// §6.1.2's kTimeThreshold, `9/8 x SRTT`, floored at the same kGranularity
/// analog the recovery plane's own time threshold is floored at
/// (`net::mp_time_threshold_split`, `net::NACK_RETX_COOLDOWN_FLOOR_US`).
/// **No constant is introduced** — 9/8 and the floor are both already in the
/// tree, cited from the same RFC clause, and used for exactly this judgement
/// on the recovery plane.
///
/// THE LAW, on one line:
///
/// ```text
///   released(t)  =  delivered(t)  +  charges older than 9/8 x SRTT
/// ```
///
/// Both terms pop the SAME `in_flight_log` the charge pushed, so the ledger is
/// 1:1 by construction and cannot over-release however the paths are striped.
///
/// **WHY NOT the sender-truth pair**, which is the shape the dispatch that
/// opened this branch proposed and which
/// [`PathState::sender_truth_release_delta`] implements as the recorded
/// negative datum: it is refuted ARITHMETICALLY, not statistically. Charging
/// every send and releasing `d_received` plus `d_sent - d_received` telescopes
/// to `in_flight == outstanding_at_cursor_init`, a CONSTANT — with the cursors
/// starting at zero that constant is zero, so the gauge is pinned on the floor
/// exactly as the contaminated delta pins it. The reason is structural:
/// `d_sent - d_received` is `loss + delta(outstanding)`, so releasing on it
/// releases the in-flight window itself. Item 3's trick works for a RATIO and
/// does not transfer to a LEDGER, which needs the per-symbol identity.
/// Reproduced and bounded by
/// `sender_truth_release_pins_the_gauge_on_the_floor`.
///
/// **Composition with [`charge_recovery_active`].** This gate makes releases
/// 1:1 with CHARGES; that one makes charges equal the TRUE WIRE. Both are
/// needed for `in_flight` to be the wire's occupancy, and each is separately
/// meaningful, so they are separate gates and a battery can attribute.
///
/// Not a dial: it selects no law on (delta, rho, r) and keys on no threshold
/// in the triangle (CLAUDE.md's no-mode-switch invariant).
pub fn release_1to1_active() -> bool {
    crate::gates::get().release_1to1
}

/// The resolve-time read behind [`release_1to1_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_release_1to1() -> bool {
    crate::config::env_flag("RWM_RELEASE_1TO1", false)
}

/// `RWM_CHARGE_RECOVERY` (**default OFF**) — METER THE TWO RECOVERY CHANNELS
/// THAT ARE NOT METERED.
///
/// The SACK-gap retransmit (`net/mod.rs`, "SACK-gap retransmit") and the NACK
/// repair margin (`net/mod.rs`, "NACK repair margin") each build a
/// `SymbolBatch` and call `transport.send_symbols` with **no
/// `charge_in_flight`, no `consume_pace_tokens`, and no
/// `PathStats::symbols_sent` increment** anywhere on the path. Every OTHER
/// wire channel meters all three at the handoff — the source arm
/// (`emit_source.rs`), the taper correction (`emit_source.rs`), the three
/// generation-coding arms (`net/mod.rs`) and, most directly, the block-ARQ
/// repair batch, whose own comment states the norm this gate restores:
/// *"Charge like any correction: in_flight budget … + pacing tokens"*.
///
/// **The exemption that IS on the record is a different one.** Recovery is
/// deliberately exempt from the ACK-CLOCKED ADMISSION TARGET (deadlock
/// otherwise), and the reactive generation arm states its own position on the
/// congestion question explicitly — *"Recovery is NON-EXEMPT from
/// `cwnd_full`"*. No record anywhere in the tree exempts these two channels
/// from the in-flight ledger, the pacer or the sender's own wire count; the
/// provenance audit found none. Charging cannot deadlock them either, because
/// **neither send site reads `available()` or `cwnd_full`** — they are budgeted
/// by `cached_nack_budget` and the NACK congestion multiplier. The charge
/// therefore makes the SOURCE arm see the occupancy recovery created, which is
/// the whole purpose of the gauge, without gating recovery on it.
///
/// **One gate, one quantity: "are these two channels metered?"** The three
/// meters move together on purpose — they are one act at the peer site
/// (block-ARQ repair charges in_flight, pace tokens and `symbols_sent` in one
/// block), and splitting them would assert an accounting the engine has
/// nowhere else. A battery cannot attribute AMONG the three; that is stated as
/// a listed wire question rather than papered over.
///
/// Not a dial (CLAUDE.md's no-mode-switch invariant): no law on (delta, rho,
/// r) is selected and no threshold is keyed. It adds two counter increments on
/// a path that already exists.
pub fn charge_recovery_active() -> bool {
    crate::gates::get().charge_recovery
}

/// The resolve-time read behind [`charge_recovery_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_charge_recovery() -> bool {
    crate::config::env_flag("RWM_CHARGE_RECOVERY", false)
}

/// `RWM_SIDLE_DERIVED` (default OFF) — goal-gate "Unlock The Default 2".
/// DIAG-ONLY and behaviour-inert: the legacy `sidle=`/`[WIDLE] idle=` fields
/// are printed UNCHANGED; this gate adds a SECOND field (`sidle2=`,
/// `idle2=`) computed by `net::stall_threshold_us` over the same event
/// stream, so the fixed-3 ms-threshold artifact question is answered on the
/// SAME runs in every arm, controls included.
/// An INSTRUMENT whose verdict is a STANDING INSTRUCTION (*where
/// `evt ≫ LOOP_WAKE_US`, read `sidle2`, not `sidle`*), retained after its
/// three session-mates (`RWM_POOL_DELIV`, `RWM_FLOOR_BOUND`,
/// `RWM_PATIENCE_DERIVED`) were removed as refuted arms.
pub fn sidle_derived_active() -> bool {
    crate::gates::get().sidle_derived
}

/// The resolve-time read behind [`sidle_derived_active`] (called once, from
/// [`crate::gates::RuntimeGates::resolve`]).
pub(crate) fn resolve_sidle_derived() -> bool {
    crate::config::env_flag("RWM_SIDLE_DERIVED", false)
}
