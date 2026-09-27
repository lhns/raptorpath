use super::*;

/// Structure-cleanup behaviour pin: the default-environment `[GATES]`
/// echo, byte for byte. A refactor of where gates are resolved must not
/// move a single character of what a battery parses.
#[test]
fn gates_echo_default_is_byte_pinned() {
    let line = RuntimeGates::resolve().echo_line();
    assert_eq!(line, PINNED_DEFAULT_GATES_ECHO);
}

const PINNED_DEFAULT_GATES_ECHO: &str = concat!(
    "[GATES] RWM_UNIFIED=1 RWM_UNIFIED_SHED=1 RWM_TAPER_R=1 ",
    "RWM_ASTAR_ANCHOR=1 RWM_MSTAR_ANCHOR=1 RWM_PLAIN_RS=0 ",
    "RWM_HONEST_ANCHOR=1 RWM_HONEST_K=0 RWM_STORE_SACK_RELEASE=1 ",
    "RWM_STORE_PATHS=1 RWM_STORE_PATH_POOL=2048 RWM_STORE=unset ",
    "RWM_STORE_GAIN=2 RWM_STORE_BOOT=128 RWM_STORE_CAP_UNIFIED=0 ",
    "RWM_THREE_TERM=0 RWM_COMPOSED_CAP=0 RWM_SUM_CAP=1 RWM_LATE_BRAKE=0 ",
    "RWM_DELTA_CAP=1 RWM_HONEST_CAP=0 RWM_POOL_ANCHOR=0 RWM_ACK_MERGE=1 ",
    "RWM_LOSS_SENT_TRUTH=0 RWM_RELEASE_1TO1=0 RWM_CHARGE_RECOVERY=0 ",
    "RWM_SIDLE_DERIVED=0 RWM_COLD_PLACE=0 RWM_PLACE_T_DERIVED=0 ",
    "RWM_PLACE_HOL=0 RWM_PLACE_WDIV_DERIVED=0 RWM_GEN=384 RWM_PIPELINE=2 ",
    "RWM_GEN_PIPE=1 RWM_GEN_R=unset RWM_GEN_RATE=9000 ",
    "RWM_GEN_RATE_FLOOR=2000 RWM_GEN_INFLIGHT=unset RWM_OOO_RETAIN=0/16 ",
    "RWM_WINDOW=unset RWM_REPORT_GENS=unset RWM_REPAIR_WAIT=unset ",
    "RWM_CODED_SRC=0 RWM_NO_REACTIVE=0 RWM_XPATH_REPAIR=0 ",
    "RWM_PROACTIVE_PACER=0 RWM_REASM_BDP=0 RWM_MIN_R=0 RWM_CC_PACE=0 ",
    "RWM_CC_PACE_HR=1.1 RWM_REACT_CAP=unset RWM_INFL_CAP=0 ",
    "RWM_INFL_BDP=unset RWM_COPA_FEED=0 RWM_RS_ATTR=1 RWM_EMIT_BATCH=0 ",
    "RWM_EMIT_BURST=64 RWM_RECOV_MP=1 RWM_RECOV_MP_LAW=1 ",
    "RWM_RECOV_MP_LIVE=0 RWM_RECOV_SP=0 RWM_DERIVED_SWEEP=0 ",
    "RWM_HOLDDOWN_Q=unset RWM_REFRESH_FLOOR_US=unset RWM_DELTA=unset ",
    "RWM_COMPLETION_EXPOSURE=0 RWM_RECV_REQUEST_LAW=0 RWM_RANK_FEEDBACK=0 ",
    "RWM_DIAG=0 RWM_ACKDIAG=0 RWM_ACKDIAG_WINDOW_US=2000000 RWM_RTT_DUMP=0 ",
    "RWM_RTT_DUMP_MAX=400000 RWM_SUCC_DUMP=0 RWM_SUCC_DUMP_MAX=200000 ",
    "RWM_WALLDIAG=0 RWM_CPUPROF=0 RWM_RDIAG=0 RWM_FDIAG=0 RWM_TRACE=0 ",
    "RWM_PFRAC=0",
);

/// The gates removed as refuted experiment arms (cleanup Stage 2) are
/// UNKNOWN names now: an operator or a stale battery script that still
/// exports one — even with a value the strict boolean parser would
/// reject for a known gate — must be ignored, never panic, and never
/// reappear on the `[GATES]` echo. Every name below is read by nothing,
/// so setting it cannot race another test's resolve.
#[test]
fn removed_gates_in_the_environment_are_ignored() {
    // Suffixes, not quoted full-name literals: `forwarding_audit` scrapes
    // every quoted RWM_ literal in `src/` as an engine read, and these
    // are not.
    const REMOVED: [&str; 15] = [
        "POOL_DELIV",
        "FLOOR_BOUND",
        "PATIENCE_DERIVED",
        "STORE_CAPW",
        "STORE_PERCAP",
        "PERCAP_GUARD",
        "STORE_BORROW",
        "WIN_DECOUPLE",
        "PLACE_SLACK",
        "RACK_CLOCKS",
        "RACK_REO_MULT",
        "QUANTILE_CLOCKS",
        "W_FORM",
        "ALPHA_OVERRIDE",
        // Not a removed gate: a name no version of the engine ever read.
        "NEVER_A_GATE",
    ];
    for suffix in REMOVED {
        // `maybe` is malformed for every boolean gate the parser knows.
        std::env::set_var(format!("RWM_{suffix}"), "maybe");
    }
    let line = RuntimeGates::resolve().echo_line();
    for suffix in REMOVED {
        let name = format!("RWM_{suffix}");
        assert!(
            !line.contains(&format!("{name}=")),
            "{name} is no longer a gate but is still echoed: {line}"
        );
    }
}

/// The `[GATES]` echo prints the EFFECTIVE honest-cap law: it is inert
/// unless `RWM_PLAIN_RS` is on, so the echo must not read `1` then.
#[test]
fn honest_cap_echo_is_the_effective_value() {
    let mut g = RuntimeGates::resolve();
    for (honest, plain, want) in
        [(true, false, "0"), (true, true, "1"), (false, true, "0"), (false, false, "0")]
    {
        g.honest_cap = honest;
        g.plain_rs = plain;
        let e = g.echo_line();
        assert!(
            e.contains(&format!("RWM_HONEST_CAP={want} RWM_POOL_ANCHOR=")),
            "honest_cap={honest} plain_rs={plain} must echo {want}: {e}"
        );
    }
}

// Unique env var names per test: test threads share one environment.

#[test]
fn env_parse_rejects_non_finite_floats() {
    for (var, val) in [
        ("RWM_TEST_EP_NAN", "NaN"),
        ("RWM_TEST_EP_INF", "inf"),
        ("RWM_TEST_EP_NINF", "-inf"),
        ("RWM_TEST_EP_OVF", "1e400"),
    ] {
        std::env::set_var(var, val);
        assert_eq!(env_parse::<f64>(var), None, "{var}={val:?} must be rejected");
        std::env::remove_var(var);
    }
    std::env::set_var("RWM_TEST_EP_OK", "2.5");
    assert_eq!(env_parse::<f64>("RWM_TEST_EP_OK"), Some(2.5));
    std::env::remove_var("RWM_TEST_EP_OK");
}

#[test]
fn gen_rate_floor_cannot_panic_on_a_small_or_nan_ceiling() {
    for (raw, ceil) in [
        (None, 0.5),
        (Some(5.0), 0.0),
        (Some(5.0), -3.0),
        (None, f64::NAN),
        (Some(f64::NAN), 100.0),
    ] {
        let f = std::panic::catch_unwind(|| gen_rate_floor(raw, ceil));
        let f = f.unwrap_or_else(|_| panic!("gen_rate_floor({raw:?}, {ceil}) panicked"));
        assert!(f >= 1.0 && f.is_finite(), "floor {f} out of range for ({raw:?}, {ceil})");
    }
    assert_eq!(gen_rate_floor(None, 9000.0), 2000.0, "default unchanged");
    assert_eq!(gen_rate_floor(Some(50_000.0), 9000.0), 9000.0, "bounded by the ceiling");
    assert_eq!(gen_rate_floor(Some(0.1), 9000.0), 1.0, "bounded below by 1");
}

#[test]
fn ooo_retain_accepts_a_depth_and_the_strict_booleans() {
    for (var, val, want) in [
        ("RWM_TEST_OOO_DEPTH", "16", true),
        ("RWM_TEST_OOO_ONE", "1", true),
        ("RWM_TEST_OOO_ZERO", "0", false),
        ("RWM_TEST_OOO_OFF", "off", false),
        ("RWM_TEST_OOO_NO", "no", false),
    ] {
        std::env::set_var(var, val);
        assert_eq!(flag_or_depth(var), want, "{var}={val:?}");
        std::env::remove_var(var);
    }
}

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
    assert!(!g.recov_sp, "RWM_RECOV_SP ships default OFF (A/B arm)");
    assert!(
        !g.derived_sweep,
        "RWM_DERIVED_SWEEP ships default OFF (A/B arm — goal-gate \
         \"The Derived Recovery Clamp\")"
    );
    // Paper 16.83.6 -- THE RECEIVER-LAW ARMS SHIP ABSENT. Both are
    // EXPERIMENTS: (A) moves the repair DECISION to the receiver and
    // suppresses the per-seq gap producer, (B) changes the request's
    // VOCABULARY. Neither is a default in waiting, and 16.80's own value
    // bound caps what either could win at < 1.54 % of transfer at c7.
    assert!(
        !g.recv_request_law,
        "RWM_RECV_REQUEST_LAW ships ABSENT (16.83.6 arm A)"
    );
    assert!(
        !g.rank_feedback,
        "RWM_RANK_FEEDBACK ships ABSENT (16.83.6 arm B)"
    );
    // THE CoDel-DERIVED SETPOINT (paper 16.67/16.70/16.71, ADR-0071
    // family 2) - FLIPPED DEFAULT ON 2026-08-19. The battery that scored
    // it (goal-gate "Candidates Battery - RESULTS", rung D) measured
    // D-LAT six of six: goodput parity at every dual on both seeds with
    // q_p50 down 10-200 ms at every one; interior with the ceiling
    // provably inert at c7 and c8 (pin 0.0000); bit-identical at N = 1
    // (eng 0/0 at c1 and sc2); and c8's paired dead wall shortened
    // (p = 0.011). Pinned ON here so the flip cannot drift back silently;
    // the OFF-value property now belongs to the `=0` arm, asserted below
    // on an explicit arm rather than on the default.
    assert!(
        g.delta_cap,
        "RWM_DELTA_CAP ships DEFAULT ON since 2026-08-19 (candidates \
         battery rung D DELIVERED: D-LAT 6/6 - goodput parity at every \
         dual both seeds with q_p50 down 10-200 ms; interior with the \
         ceiling inert at c7/c8; bit-identical at N = 1). `=0` remains \
         the re-runnable A/B arm - the displaced gain = 2.0 fossil."
    );
    // THE REFRESH-BAND FLOOR IS ABSENT BY DEFAULT, AND ABSENT IS THE
    // SHIPPED 25 ms (paper 16.78). Both halves asserted: the resolved
    // field AND the echo token, so "the control was really a control" is
    // read off the run's own output rather than inferred.
    assert!(
        g.refresh_floor_us.is_none(),
        "RWM_REFRESH_FLOOR_US is ABSENT by default - absent resolves to              HOLE_NACK_REFRESH_MIN and the hole-refresh cadence is the              shipped one byte-identically (paper 16.78)"
    );
    assert!(
        g.echo_line().contains("RWM_REFRESH_FLOOR_US=unset"),
        "the absent floor must echo `unset`: {}",
        g.echo_line()
    );
    let mut armed = g.clone();
    armed.refresh_floor_us = Some(6_150);
    assert!(
        armed.echo_line().contains("RWM_REFRESH_FLOOR_US=6150"),
        "the armed floor must echo its RESOLVED us: {}",
        armed.echo_line()
    );
    // THE DOMAIN IS THE LAW'S OWN, NOT A TASTE, AND IT IS PINNED ON BOTH
    // SIDES. Below the receiver loop's wake granularity the cadence cannot
    // be expressed by the loop that has to emit it; above the shipped upper
    // rail the band's LOWER rail leaves the shipped band entirely. The arms
    // paper 16.78.3 derives must all be INSIDE it - a pre-registration
    // whose own grid is out of its gate's domain is unsatisfiable when
    // written, and this tree has that failure on the record once already.
    for bad in [0u64, crate::net::LOOP_WAKE_US - 1, 100_001, u64::MAX] {
        assert!(
            bad < crate::net::LOOP_WAKE_US
                || bad > crate::net::HOLE_NACK_REFRESH_MAX.as_micros() as u64,
            "`{bad}` us must be OUTSIDE the refresh floor's domain (paper 16.78)"
        );
    }
    for good in [
        crate::net::LOOP_WAKE_US,
        1_919,
        3_838,
        6_144,
        6_150,
        12_288,
        12_300,
        crate::net::HOLE_NACK_REFRESH_MIN.as_micros() as u64,
        crate::net::HOLE_NACK_REFRESH_MAX.as_micros() as u64,
    ] {
        assert!(
            good >= crate::net::LOOP_WAKE_US
                && good <= crate::net::HOLE_NACK_REFRESH_MAX.as_micros() as u64,
            "`{good}` us is an arm the (q, refresh) sweep commands and must                  be INSIDE the refresh floor's domain (paper 16.78.3)"
        );
    }
    // The gates echo is what a battery parses; assert the three new names
    // are on it with their resolved values, two-sided.
    let line = g.echo_line();
    for tok in [
        // Flipped 2026-08-19: the echo must name the SHIPPED value.
        "RWM_DELTA_CAP=1",
        // THE CONTRACT'S δ is ABSENT on every shipped arm: the hint names
        // the point on the dial, and the echo says so (§16.81).
        "RWM_DELTA=unset",
        // The χ arm is OFF on every shipped arm: §14.26's glide stays
        // inert and `r*` stays at the corner unless a battery arms it.
        "RWM_COMPLETION_EXPOSURE=0",
    ] {
        assert!(line.contains(tok), "the [GATES] echo is missing {tok}: {line}");
    }
    assert!(
        g.delta.is_none(),
        "RWM_DELTA is an EXPERIMENT knob and is ABSENT by default — the \
         shipped δ is the one the contract's hint names, and nothing \
         shipped may set a δ between the presets (paper §16.81/§16.82)"
    );
    // THE ARMED ARM'S ECHO, two-sided: a row standing between the presets
    // must be able to state its own δ off its own log. Set by FIELD —
    // env mutation is process-global in a parallel runner, and the resolve
    // is a `OnceLock` besides.
    let mut dialed = g.clone();
    dialed.delta = Some(0.05);
    assert!(
        dialed.echo_line().contains("RWM_DELTA=0.05"),
        "the armed arm's echo must NAME the RESOLVED δ, not a flag: {}",
        dialed.echo_line()
    );
    // THE `=0` ARM'S OFF-VALUE PROPERTY, which the default assertion above
    // used to carry (MEASUREMENT DISCIPLINE 15, two-sided): a battery
    // re-running the displaced `gain = 2.0` fossil must be able to assert
    // the gate ABSENT on both endpoints, not merely unmentioned. RE-HOMED
    // onto an EXPLICIT arm now that the default is ON, so the property
    // survives the flip instead of being retired by it. Set by field
    // rather than through the environment: env mutation is process-global
    // state in a parallel runner.
    let mut off_arm = g.clone();
    off_arm.delta_cap = false;
    assert!(
        off_arm.echo_line().contains("RWM_DELTA_CAP=0"),
        "the `=0` arm's echo must NAME the delta-cap gate with its 0 value \
         - the displaced gain = 2.0 fossil stays re-runnable and \
         scrapeable: {}",
        off_arm.echo_line()
    );
    assert!(g.gen_pipe, "gen_pipe default rides unified_active()");
    // The est×honest-anchor composed flip (goal-gate "Ship The Wins 1",
    // 2026-08-07) was measured and REVERTED by its pre-set c7 clause:
    // everything unset ⇒ est-cadence OFF (estimator's own default test)
    // ⇒ pool-anchor OFF (it rides the est resolution), emit-batch OFF.
    // The composed opt-in (est=1 ⇒ pa on, + eb=1) stays the documented
    // fast single-path configuration (c1 446–508).
    assert!(
        !g.pool_anchor,
        "RWM_POOL_ANCHOR default rides the RWM_EST_CADENCE resolution (OFF unset)"
    );
    // "Ack-Merge Flip" (2026-08-08): the window-mode control-datagram
    // merge PASSED its own pre-registered gate set at full scope (×8,
    // both seeds, + sustained + crown) and is now part of the shipped
    // stack. c1 +12.7%/+13.0% with receiver CPU per bit −9.1%/−8.4%;
    // control-datagram density 1.96 → 1.00 at c1 and 1.05 → 1.00 at c7,
    // and the response tracks the density removed cell by cell. Every
    // no-regression gate held within σ of its own same-session control.
    assert!(
        g.ack_merge,
        "RWM_ACK_MERGE ships default ON since 2026-08-08 (paper §16.42); \
         RWM_ACK_MERGE=0 is the opt-out arm"
    );
    // "Cross-Path Loss Contamination" (2026-08-18) and its successor
    // "The Accounting Ledger" (fix/accounting-ledger): the three honest-
    // accounting gates all ship OFF. The loss estimator's re-heats every
    // SRTT/loss-scaled recovery cadence (the named follow-up); the two
    // ledger gates change the ADMISSION gauge's operand and must be
    // measured on the wire before any flip.
    assert!(
        !g.loss_sent_truth,
        "RWM_LOSS_SENT_TRUTH ships default OFF pending the cadence re-derivation"
    );
    assert!(
        !g.release_1to1,
        "RWM_RELEASE_1TO1 ships default OFF (A/B arm)"
    );
    assert!(
        !g.charge_recovery,
        "RWM_CHARGE_RECOVERY ships default OFF (A/B arm)"
    );
    // "Unlock The Default 2: derived patience" (2026-08-07): the derived
    // recovery-patience floor is a pure A/B arm and must not reach the
    // shipped default stack until its pre-registered gate set passes;
    // the derived stall gauge is DIAG-only and also ships OFF.
    assert!(
        !g.sidle_derived,
        "RWM_SIDLE_DERIVED ships default OFF (DIAG-only A/B gauge)"
    );
    // Experiments / instruments (default OFF)
    assert!(!g.plain_rs);
    // "Honest Inputs" (2026-08-10): both fixes ship default OFF (A/B
    // arms; anchor-hygiene umbrella members). The OFF-VALUE PROPERTY,
    // two-sided on the echo (MEASUREMENT DISCIPLINE 15): a battery must
    // be able to assert the gates ABSENT on the control arm.
    assert!(
        g.honest_anchor,
        "RWM_HONEST_ANCHOR ships DEFAULT ON since 2026-08-11 (flip-battery F7 \
         swept: goodput within 2σ every cell/seed, CPU/byte 0.90–1.03×; \
         value-identical by the unit-pinned equivalence). `=0` remains the \
         re-runnable legacy-fold A/B arm."
    );
    assert!(
        !g.honest_k,
        "RWM_HONEST_K ships default OFF (A/B arm — goal-gate \"Honest Inputs\"; \
         flip battery: rode only the failed BHU composition, khr−kraw ≈ 0 in-cell)"
    );
    assert!(
        g.echo_line().contains("RWM_HONEST_ANCHOR=1")
            && g.echo_line().contains("RWM_HONEST_K=0"),
        "the default echo must NAME both Honest-Inputs gates with their shipped \
         values (anchor=1 since the 2026-08-11 flip, K=0): {}",
        g.echo_line()
    );
    assert!(!g.emit_batch, "emission batching ships OFF (the composed flip reverted)");
    assert_eq!(g.emit_burst, 64);
    assert!(
        !g.cold_place,
        "RWM_COLD_PLACE ships default OFF (A/B arm) — the cold-start \
         placement repair must be opted into"
    );
    // ── TRACK A's THREE PLACEMENT ARMS (paper §16.81) ─────────────
    // All three ABSENT by default, and all three NAMED with their `0`
    // value on the echo: the placement battery's CTL arm must be able to
    // assert the arms ABSENT rather than merely unmentioned, and the
    // pinned cost table is an oracle only while this holds.
    assert!(
        !g.place_t_derived,
        "RWM_PLACE_T_DERIVED ships ABSENT (Track A arm 1 — the derived \
         softmax temperature must be opted into)"
    );
    assert!(
        !g.place_hol,
        "RWM_PLACE_HOL ships ABSENT (Track A arm 2 — the frontier term \
         must be opted into)"
    );
    assert!(
        !g.place_wdiv_derived,
        "RWM_PLACE_WDIV_DERIVED ships ABSENT (Track A arm 3 — the derived \
         diversity weight must be opted into)"
    );
    for k in ["RWM_PLACE_T_DERIVED=0", "RWM_PLACE_HOL=0", "RWM_PLACE_WDIV_DERIVED=0"] {
        assert!(
            g.echo_line().contains(k),
            "the default echo must NAME `{k}` two-sided: {}",
            g.echo_line()
        );
    }
    assert!(
        !g.recov_mp_live,
        "RWM_RECOV_MP_LIVE ships default OFF (A/B arm)"
    );
    assert!(
        !g.store_cap_unified,
        "RWM_STORE_CAP_UNIFIED ships default OFF (A/B arm)"
    );
    assert!(
        !g.three_term,
        "RWM_THREE_TERM ships default OFF (A/B arm — goal-gate \"Three-Term Law\")"
    );
    // The gate's OFF-VALUE PROPERTY, asserted on the echo itself
    // (MEASUREMENT DISCIPLINE 15, two-sided): a battery must be able to
    // assert the gate ABSENT in the control arm, not merely unmentioned.
    assert!(
        g.echo_line().contains("RWM_THREE_TERM=0"),
        "the default echo must NAME the three-term gate with its 0 value: {}",
        g.echo_line()
    );
    // The COMPOSED CAP LAW (paper §16.56, ADR-0070 Deliverable 2) is an
    // A/B arm and ships OFF, with the same two-sided OFF-value property.
    assert!(
        !g.composed_cap,
        "RWM_COMPOSED_CAP ships default OFF (A/B arm — paper §16.56)"
    );
    assert!(
        g.echo_line().contains("RWM_COMPOSED_CAP=0"),
        "the default echo must NAME the composed-cap gate with its 0 value: {}",
        g.echo_line()
    );
    // THE `×N` DELETION (paper §16.60/§16.64, ADR-0070 finding 2) —
    // **FLIPPED DEFAULT ON 2026-08-19**. The A/B that ADR-0070 said had
    // never been run was run (goal-gate "Ladder Battery — RESULTS", rung
    // N): interior at both scoreable duals (`pin` 0.000, `eng` 1.000,
    // `chg_frac` 1.000), the control reproducing the shipped 4096 pin,
    // goodput UP at c8 on both seeds, CPU 0.937–1.005×, all guards green.
    // Pinned ON here so the flip cannot drift back silently; the OFF-value
    // property now belongs to the `=0` arm, asserted below on an explicit
    // arm rather than on the default.
    assert!(
        g.sum_cap,
        "RWM_SUM_CAP ships DEFAULT ON since 2026-08-19 (ladder battery rung \
         N DELIVERED: interior at both duals, pin 0.000 / eng 1.000 / \
         chg_frac 1.000, goodput ≥ control at c8 both seeds). `=0` remains \
         the re-runnable A/B arm — the displaced quadratic."
    );
    assert!(
        g.echo_line().contains("RWM_SUM_CAP=1"),
        "the default echo must NAME the sum-cap gate with its shipped 1 \
         value (flipped 2026-08-19): {}",
        g.echo_line()
    );
    // THE `=0` ARM'S OFF-VALUE PROPERTY, which the default assertion above
    // used to carry (MEASUREMENT DISCIPLINE 15, two-sided): a battery
    // re-running the displaced quadratic must be able to assert the gate
    // ABSENT on both endpoints, not merely unmentioned. Asserted on an
    // EXPLICIT arm now that the default is ON, so the property survives the
    // flip instead of being retired by it. Set by field rather than through
    // the environment: env mutation is process-global state in a parallel
    // runner.
    let mut off_arm = g.clone();
    off_arm.sum_cap = false;
    assert!(
        off_arm.echo_line().contains("RWM_SUM_CAP=0"),
        "the `=0` arm's echo must NAME the sum-cap gate with its 0 value — \
         the displaced quadratic stays re-runnable and scrapeable: {}",
        off_arm.echo_line()
    );
    // THE EXTRACTED LATE-STAGE BRAKE (§16.60.1, ADR-0070 finding 7) is
    // still an A/B arm and still ships OFF: the ladder scored it
    // DELIVERED-AS-ARMED but NEEDS-MORE for effect (B-WALL closed on
    // power), so it is NOT flipped by the same program that flipped
    // `RWM_SUM_CAP`.
    assert!(
        !g.late_brake,
        "RWM_LATE_BRAKE ships default OFF (A/B arm — paper §16.60.1; ladder \
         battery: armed on 110/110 FULL reps but its EFFECT is unresolved)"
    );
    assert!(
        g.echo_line().contains("RWM_LATE_BRAKE=0"),
        "the default echo must NAME the late-brake gate with its 0 value: {}",
        g.echo_line()
    );
    assert!(!g.proactive_pacer && !g.xpath_repair && !g.no_reactive);
    assert!(!g.diag && !g.rdiag && !g.fdiag && !g.trace && !g.pfrac);
    // The ack-cadence gauge (goal-gate "Ack-Cadence Gauge", 2026-08-11)
    // is a DIAG-surface instrument and ships OFF, with the two-sided
    // OFF-VALUE property asserted on the echo (MEASUREMENT DISCIPLINE 15).
    assert!(
        !g.ackdiag,
        "RWM_ACKDIAG ships default OFF (DIAG-surface instrument)"
    );
    assert!(
        g.echo_line().contains("RWM_ACKDIAG=0"),
        "the default echo must NAME the ack-cadence gauge with its 0 value: {}",
        g.echo_line()
    );
    // The raw RTT sample dump (clause `B`'s exact reference, 2026-08-21)
    // is the same class and ships the same way — and it matters more here
    // than for its siblings, because ON it writes megabytes of stderr per
    // path and takes a lock on every RTT sample.
    assert!(
        !g.rtt_dump,
        "RWM_RTT_DUMP ships default OFF (raw-sample dump: megabytes of \
         stderr and a per-sample lock)"
    );
    assert!(
        g.echo_line().contains("RWM_RTT_DUMP=0"),
        "the default echo must NAME the raw-sample dump with its 0 value: {}",
        g.echo_line()
    );
    assert!(
        g.echo_line().contains("RWM_RTT_DUMP_MAX=400000"),
        "the default echo must carry the dump cap's RESOLVED value, so a \
         truncated leg's clause B is readable off its own run: {}",
        g.echo_line()
    );
    // The successor-arrival RAW dump (2026-08-21) is the same class and
    // ships the same way — at the RECEIVER, where the cost is directly
    // goodput-visible. Its QUANTILE line is ungated and always emitted;
    // only the raw record stream is behind this flag, which is what lets a
    // scored pass read the distribution without paying for the dump.
    assert!(
        !g.succ_dump,
        "RWM_SUCC_DUMP ships default OFF (raw per-hole records: megabytes \
         of receiver-side stderr on a lossy cell)"
    );
    assert!(
        g.echo_line().contains("RWM_SUCC_DUMP=0"),
        "the default echo must NAME the successor dump with its 0 value: {}",
        g.echo_line()
    );
    assert!(
        g.echo_line().contains("RWM_SUCC_DUMP_MAX=200000"),
        "the default echo must carry the successor dump cap's RESOLVED \
         value, so a truncated record stream is readable off its own run \
         rather than inferred by whoever derives against it: {}",
        g.echo_line()
    );
    // The dead-wall onset/duration instrument (ADR-0070 validation path
    // step 2, 2026-08-12) is the same class and ships the same way.
    assert!(
        !g.walldiag,
        "RWM_WALLDIAG ships default OFF (DIAG-surface instrument)"
    );
    assert!(
        g.echo_line().contains("RWM_WALLDIAG=0"),
        "the default echo must NAME the dead-wall gauge with its 0 value: {}",
        g.echo_line()
    );
    // The sender CPU decomposition (goal-gate "MEASUREMENT TRUTH item 2 —
    // THE SENDER CPU CEILING", 2026-08-19) is the same class and ships the
    // same way. The two-sided property matters more here than for its
    // siblings: the cell this instrument is built for is sender-CPU-bound,
    // so an arm that silently carried the gauge would be paying for it in
    // exactly the quantity under measurement, and "the gate did not take"
    // must be readable from the run's own output rather than inferred.
    assert!(
        !g.cpuprof,
        "RWM_CPUPROF ships default OFF (DIAG-surface instrument)"
    );
    assert!(
        g.echo_line().contains("RWM_CPUPROF=0"),
        "the default echo must NAME the CPU-decomposition gauge with its 0 value: {}",
        g.echo_line()
    );
    // Numeric defaults
    assert_eq!(g.gen_size, 384);
    assert_eq!(g.pipeline, 2);
    assert_eq!(g.store_path_pool, 2048);
    assert_eq!(g.store_boot, 128);
    assert!((g.store_gain - 2.0).abs() < 1e-12);
    assert!((g.cc_pace_headroom - 1.1).abs() < 1e-12);
    assert!(g.store_override.is_none());
}
