use super::*;

/// Behaviour pin: the default-environment `[GATES]` echo, byte for byte. Moving
/// where gates are resolved must not move a character a battery parses.
#[test]
fn gates_echo_default_is_byte_pinned() {
    let line = RuntimeGates::resolve().echo_line();
    assert_eq!(line, PINNED_DEFAULT_GATES_ECHO);
}

/// `RWM_COPA_DELTA` echoes the CC's δ override as the resolved NUMBER, so a
/// battery that pins the CC (the r > 0 battery's MID arm) can witness the
/// pin off `[GATES]` — and an out-of-domain value echoes `unset`, the way
/// `scheduler::copa_delta` ignores it. No env is set: the field is written
/// on a resolved copy, so parallel resolves cannot race.
#[test]
fn the_copa_delta_override_echoes_its_resolved_value() {
    let mut g = RuntimeGates::resolve();
    g.copa_delta = Some(0.005);
    let line = g.echo_line();
    assert!(line.contains(" RWM_COPA_DELTA=0.005 "), "{line}");
    // `RWM_DELTA=` must not be read inside `RWM_COPA_DELTA=`: a token scrape
    // anchors at a token start (`l1common.field`), and so does this check.
    assert!(line.contains(" RWM_DELTA=unset "), "{line}");
    g.copa_delta = None;
    assert!(g.echo_line().contains(" RWM_COPA_DELTA=unset "));
    // The CC reads the same resolved field `[GATES]` prints.
    use crate::scheduler::copa_delta;
    use crate::control::fec_rate::ProtocolHint;
    assert_eq!(copa_delta(ProtocolHint::Bulk, Some(0.005)), 0.005);
    assert_eq!(
        copa_delta(ProtocolHint::Bulk, None),
        copa_delta(ProtocolHint::Bulk, Some(-1.0)),
        "an out-of-domain override is the unset one"
    );
}

const PINNED_DEFAULT_GATES_ECHO: &str = concat!(
    "[GATES] RWM_UNIFIED=1 RWM_UNIFIED_SHED=1 RWM_TAPER_R=1 ",
    "RWM_ASTAR_ANCHOR=1 RWM_MSTAR_ANCHOR=1 RWM_PLAIN_RS=0 ",
    "RWM_HONEST_ANCHOR=1 RWM_HONEST_K=0 RWM_STORE_SACK_RELEASE=1 ",
    "RWM_STORE_PATHS=1 RWM_STORE_PATH_POOL=2048 RWM_STORE=unset ",
    "RWM_STORE_GAIN=2 RWM_STORE_BOOT=128 ",
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
    "RWM_INFL_BDP=unset RWM_COPA_FEED=0 RWM_RS_ATTR=1 RWM_EMIT_BATCH=1 ",
    "RWM_EMIT_BURST=64 RWM_RECOV_MP=1 RWM_RECOV_MP_LAW=1 ",
    "RWM_RECOV_SP=0 RWM_DERIVED_SWEEP=0 ",
    "RWM_HOLDDOWN_Q=unset RWM_REFRESH_FLOOR_US=unset RWM_DELTA=unset ",
    "RWM_COPA_DELTA=unset ",
    "RWM_COMPLETION_EXPOSURE=0 RWM_RECV_REQUEST_LAW=0 RWM_RANK_FEEDBACK=0 ",
    "RWM_DIAG=0 RWM_ACKDIAG=0 RWM_ACKDIAG_WINDOW_US=2000000 RWM_RTT_DUMP=0 ",
    "RWM_RTT_DUMP_MAX=400000 RWM_SUCC_DUMP=0 RWM_SUCC_DUMP_MAX=200000 ",
    "RWM_WALLDIAG=0 RWM_CPUPROF=0 RWM_RDIAG=0 RWM_RTOBS=0 RWM_FDIAG=0 RWM_TRACE=0 ",
    "RWM_PFRAC=0",
);

/// Gates removed as refuted experiment arms are unknown names: a stale script
/// that still exports one, even with a value the strict boolean parser would
/// reject for a known gate, must be ignored, never panic, and never appear on
/// the `[GATES]` echo. Nothing reads these names, so setting them cannot race
/// another test's resolve.
#[test]
fn removed_gates_in_the_environment_are_ignored() {
    // Suffixes, not quoted full-name literals: `forwarding_audit` scrapes
    // every quoted RWM_ literal in `src/` as an engine read, and these
    // are not.
    const REMOVED: [&str; 17] = [
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
        // Plan 2b: the channel's path set is membership, unconditionally.
        "STORE_CAP_UNIFIED",
        "RECOV_MP_LIVE",
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

/// Default-env resolution reproduces the shipped stack (`docs/status.md` §1):
/// the core laws on, every experiment gate off. Set-env semantics are
/// `config::env_flag`'s and are tested there.
#[test]
fn default_env_resolves_the_shipped_stack() {
    // Assumes the test environment exports no RWM_* overrides.
    let g = RuntimeGates::resolve();
    // CORE (default ON)
    assert!(g.unified && g.unified_shed && g.taper_r);
    assert!(g.astar_anchor && g.mstar_anchor);
    assert!(g.store_sack_release && g.store_paths);
    assert!(g.recov_mp && g.recov_mp_law);
    assert!(!g.recov_sp, "RWM_RECOV_SP ships default OFF (A/B arm)");
    assert!(
        !g.derived_sweep,
        "RWM_DERIVED_SWEEP ships default OFF (A/B arm)"
    );
    // The receiver-law arms (paper §7.6) ship absent.
    assert!(
        !g.recv_request_law,
        "RWM_RECV_REQUEST_LAW ships ABSENT (paper §7.6, arm A)"
    );
    assert!(
        !g.rank_feedback,
        "RWM_RANK_FEEDBACK ships ABSENT (paper §7.6, arm B)"
    );
    // The CoDel-derived setpoint ships on (paper §6.1); the `=0` arm's off value
    // is asserted below on an explicit arm.
    assert!(
        g.delta_cap,
        "RWM_DELTA_CAP ships default ON (paper §6.1); `=0` remains the \
         re-runnable A/B arm - the displaced gain = 2.0 law."
    );
    // The refresh-band floor is absent by default, which is the shipped 25 ms
    // (paper §7.4); both the field and the echo token are asserted.
    assert!(
        g.refresh_floor_us.is_none(),
        "RWM_REFRESH_FLOOR_US is ABSENT by default - absent resolves to \
         HOLE_NACK_REFRESH_MIN and the hole-refresh cadence is the \
         shipped one byte-identically (paper §7.4)"
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
    // The floor's domain, pinned on both sides: below the receiver loop's wake
    // granularity the cadence cannot be expressed, and above the shipped upper
    // rail the band leaves the shipped band. Every sweep arm must lie inside it.
    for bad in [0u64, crate::net::LOOP_WAKE_US - 1, 100_001, u64::MAX] {
        assert!(
            bad < crate::net::LOOP_WAKE_US
                || bad > crate::net::HOLE_NACK_REFRESH_MAX.as_micros() as u64,
            "`{bad}` us must be OUTSIDE the refresh floor's domain (paper §7.4)"
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
            "`{good}` us is an arm the (q, refresh) sweep commands and must \
             be INSIDE the refresh floor's domain (paper §7.4)"
        );
    }
    // The echo is what a battery parses; assert these names with their resolved
    // values.
    let line = g.echo_line();
    for tok in [
        // Shipped on.
        "RWM_DELTA_CAP=1",
        // The contract's δ is absent on every shipped arm (the hint names it).
        "RWM_DELTA=unset",
        // The χ arm is off on every shipped arm.
        "RWM_COMPLETION_EXPOSURE=0",
    ] {
        assert!(line.contains(tok), "the [GATES] echo is missing {tok}: {line}");
    }
    assert!(
        g.delta.is_none(),
        "RWM_DELTA is an EXPERIMENT knob and is ABSENT by default — the \
         shipped δ is the one the contract's hint names, and nothing \
         shipped may set a δ between the presets (paper §4.1)"
    );
    // An armed δ must echo its resolved value. Set by field: env mutation is
    // process-global in a parallel runner, and the resolve is a `OnceLock`.
    let mut dialed = g.clone();
    dialed.delta = Some(0.05);
    assert!(
        dialed.echo_line().contains("RWM_DELTA=0.05"),
        "the armed arm's echo must NAME the RESOLVED δ, not a flag: {}",
        dialed.echo_line()
    );
    // The `=0` arm must echo the gate with its 0 value. Set by field, not env.
    let mut off_arm = g.clone();
    off_arm.delta_cap = false;
    assert!(
        off_arm.echo_line().contains("RWM_DELTA_CAP=0"),
        "the `=0` arm's echo must NAME the delta-cap gate with its 0 value \
         - the displaced gain = 2.0 law stays re-runnable and \
         scrapeable: {}",
        off_arm.echo_line()
    );
    assert!(g.gen_pipe, "gen_pipe default rides unified_active()");
    // The estimator cadence ships on (Stage 3 (d), status.md §5) and the pool
    // anchor, decoupled from it, ships off: the default IS the measured arm
    // `RWM_EST_CADENCE=1 RWM_POOL_ANCHOR=0`.
    assert!(
        g.est_cadence,
        "RWM_EST_CADENCE ships default ON (Stage 3 (d) FLIP-RECOMMENDED)"
    );
    assert!(
        !g.pool_anchor,
        "RWM_POOL_ANCHOR ships default OFF, independent of RWM_EST_CADENCE"
    );
    // The window-mode control-datagram merge ships on (paper §9.5).
    assert!(
        g.ack_merge,
        "RWM_ACK_MERGE ships default ON (paper §9.5); \
         RWM_ACK_MERGE=0 is the opt-out arm"
    );
    // The three honest-accounting gates ship off: honest loss re-heats the
    // loss-scaled recovery cadences, and the two ledger gates change the
    // admission gauge's operand.
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
    // The derived stall gauge is an instrument and ships off.
    assert!(
        !g.sidle_derived,
        "RWM_SIDLE_DERIVED ships default OFF (DIAG-only A/B gauge)"
    );
    // Experiments / instruments (default OFF)
    assert!(!g.plain_rs);
    // Honest anchor ships on, honest K off; both are named on the echo so a
    // control arm can assert them (`docs/measurement-discipline.md` rule 15).
    assert!(
        g.honest_anchor,
        "RWM_HONEST_ANCHOR ships default ON (value-identical by the \
         unit-pinned equivalence); `=0` remains the re-runnable \
         legacy-fold A/B arm."
    );
    assert!(
        !g.honest_k,
        "RWM_HONEST_K ships default OFF (A/B arm)"
    );
    assert!(
        g.echo_line().contains("RWM_HONEST_ANCHOR=1")
            && g.echo_line().contains("RWM_HONEST_K=0"),
        "the default echo must NAME both Honest-Inputs gates with their shipped \
         values (anchor=1, K=0): {}",
        g.echo_line()
    );
    assert!(
        g.emit_batch,
        "RWM_EMIT_BATCH ships default ON (status §8 Result (re-run):          FLIP-RECOMMENDED, Law 0); `=0` is the per-symbol control arm"
    );
    assert_eq!(
        g.emit_burst, 64,
        "the shipped form is the measured one: the burst bound stays 64"
    );
    assert!(
        g.echo_line().contains("RWM_EMIT_BATCH=1 RWM_EMIT_BURST=64"),
        "the default echo must NAME emission batching ON at burst 64: {}",
        g.echo_line()
    );
    assert!(
        !g.cold_place,
        "RWM_COLD_PLACE ships default OFF (A/B arm) — the cold-start \
         placement repair must be opted into"
    );
    // ── The three placement arms (paper §5.7) ──
    // All absent by default and named with `0` on the echo, so a control arm can
    // assert them absent; the pinned cost table is an oracle only while this holds.
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
        !g.three_term,
        "RWM_THREE_TERM ships default OFF (A/B arm)"
    );
    // Off value named on the echo, two-sided (`docs/measurement-discipline.md`
    // rule 15).
    assert!(
        g.echo_line().contains("RWM_THREE_TERM=0"),
        "the default echo must NAME the three-term gate with its 0 value: {}",
        g.echo_line()
    );
    // The composed cap (paper §10) ships off, same two-sided property.
    assert!(
        !g.composed_cap,
        "RWM_COMPOSED_CAP ships default OFF (A/B arm — paper §10)"
    );
    assert!(
        g.echo_line().contains("RWM_COMPOSED_CAP=0"),
        "the default echo must NAME the composed-cap gate with its 0 value: {}",
        g.echo_line()
    );
    // The `×N` deletion ships on (paper §6.1); the `=0` arm's off value is
    // asserted below on an explicit arm.
    assert!(
        g.sum_cap,
        "RWM_SUM_CAP ships default ON (paper §6.1); `=0` remains the \
         re-runnable A/B arm — the displaced quadratic."
    );
    assert!(
        g.echo_line().contains("RWM_SUM_CAP=1"),
        "the default echo must NAME the sum-cap gate with its shipped 1 \
         value: {}",
        g.echo_line()
    );
    // The `=0` arm must echo the gate with its 0 value. Set by field, not env.
    let mut off_arm = g.clone();
    off_arm.sum_cap = false;
    assert!(
        off_arm.echo_line().contains("RWM_SUM_CAP=0"),
        "the `=0` arm's echo must NAME the sum-cap gate with its 0 value — \
         the displaced quadratic stays re-runnable and scrapeable: {}",
        off_arm.echo_line()
    );
    // The extracted late-stage brake is still an experiment arm and ships off.
    assert!(
        !g.late_brake,
        "RWM_LATE_BRAKE ships default OFF (A/B arm; its effect is unresolved)"
    );
    assert!(
        g.echo_line().contains("RWM_LATE_BRAKE=0"),
        "the default echo must NAME the late-brake gate with its 0 value: {}",
        g.echo_line()
    );
    assert!(!g.proactive_pacer && !g.xpath_repair && !g.no_reactive);
    assert!(!g.diag && !g.rdiag && !g.rtobs && !g.fdiag && !g.trace && !g.pfrac);
    // Instruments ship off, each named with its 0 value on the echo.
    assert!(
        !g.ackdiag,
        "RWM_ACKDIAG ships default OFF (DIAG-surface instrument)"
    );
    assert!(
        g.echo_line().contains("RWM_ACKDIAG=0"),
        "the default echo must NAME the ack-cadence gauge with its 0 value: {}",
        g.echo_line()
    );
    // The raw RTT dump writes megabytes of stderr and locks per sample when on.
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
    // The successor raw dump costs receiver-side stderr when on; its quantile
    // line is ungated.
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
    assert!(
        !g.walldiag,
        "RWM_WALLDIAG ships default OFF (DIAG-surface instrument)"
    );
    assert!(
        g.echo_line().contains("RWM_WALLDIAG=0"),
        "the default echo must NAME the dead-wall gauge with its 0 value: {}",
        g.echo_line()
    );
    // The CPU decomposition runs on sender-CPU-bound cells, so an arm that
    // silently carried it would pay in the measured quantity.
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

/// Every accessor reads the process's single [`get`] resolution, so the value
/// behaviour reads and the value `[GATES]` prints cannot diverge. Asserted as
/// identity at every accessor.
#[test]
fn every_gate_accessor_reads_the_one_resolution() {
    let g = get();
    assert!(std::ptr::eq(g, get()), "one resolution per process");
    assert_eq!(crate::scheduler::copa_wire_active(), g.copa_wire);
    assert_eq!(crate::scheduler::copa_compete_active(), g.copa_compete);
    assert_eq!(crate::scheduler::pool_anchor_active(), g.pool_anchor);
    assert_eq!(crate::scheduler::honest_anchor_active(), g.honest_anchor);
    assert_eq!(crate::scheduler::honest_k_active(), g.honest_k);
    assert_eq!(crate::scheduler::cold_place_active(), g.cold_place);
    assert_eq!(crate::scheduler::place_t_derived_active(), g.place_t_derived);
    assert_eq!(crate::scheduler::place_hol_active(), g.place_hol);
    assert_eq!(crate::scheduler::place_wdiv_derived_active(), g.place_wdiv_derived);
    assert_eq!(crate::scheduler::ack_merge_active(), g.ack_merge);
    assert_eq!(crate::scheduler::loss_sent_truth_active(), g.loss_sent_truth);
    assert_eq!(crate::scheduler::release_1to1_active(), g.release_1to1);
    assert_eq!(crate::scheduler::charge_recovery_active(), g.charge_recovery);
    assert_eq!(crate::scheduler::sidle_derived_active(), g.sidle_derived);
    assert_eq!(crate::control::estimator::est_cadence_active(), g.est_cadence);
    assert_eq!(crate::transport::wire_compact_active(), g.wire_compact);
    assert_eq!(crate::net::unified_active(), g.unified);
    assert_eq!(delta_override(), g.delta);
    assert_eq!(crate::net::ackdiag::window_us(), g.ackdiag_window_us);
    assert_eq!(crate::net::rttdump::dump_max(), g.rtt_dump_max);
    assert_eq!(crate::net::succ::dump_max(), g.succ_dump_max);
    assert_eq!(
        crate::scheduler::place::place_temperature().to_bits(),
        g.place_t.to_bits()
    );
    // The process resolution IS a resolve of the same environment: its echo
    // is the pinned default line.
    assert_eq!(g.echo_line(), RuntimeGates::resolve().echo_line());
    assert_eq!(g.echo_line(), PINNED_DEFAULT_GATES_ECHO);
}

/// Stage 3 (d) measured `RWM_EST_CADENCE=1 RWM_POOL_ANCHOR=0` and nothing
/// else: the pool anchor is an independent experiment arm with its own
/// shipped default (OFF), never a passenger of the estimator cadence. With
/// only the cadence set, the pool anchor must resolve OFF. Setting the
/// cadence to `1` is the shipped default value, so this env write cannot
/// change what a concurrently resolving test sees.
#[test]
fn pool_anchor_does_not_follow_the_estimator_cadence() {
    std::env::remove_var("RWM_POOL_ANCHOR");
    std::env::set_var("RWM_EST_CADENCE", "1");
    let g = RuntimeGates::resolve();
    std::env::remove_var("RWM_EST_CADENCE");
    assert!(g.est_cadence, "RWM_EST_CADENCE=1 must resolve the cadence on");
    assert!(
        !g.pool_anchor,
        "RWM_POOL_ANCHOR must resolve OFF when only RWM_EST_CADENCE=1 is set \
         (Stage 3 (d) measured the cadence with the pool anchor off)"
    );
    assert!(
        g.echo_line().contains(" RWM_POOL_ANCHOR=0 "),
        "the [GATES] echo must witness the decoupled pool anchor: {}",
        g.echo_line()
    );
}
