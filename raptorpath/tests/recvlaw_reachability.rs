//! **THE RECEIVER-SEAT REQUEST LAW IS REACHABLE, THE COLLISION SEAM CLOSES,
//! AND THE ABSENT ARM IS THE SHIPPED MACHINE.**
//!
//! Paper §16.83, arms **(A)** `RWM_RECV_REQUEST_LAW` and **(B)**
//! `RWM_RANK_FEEDBACK`. Every recovery clock this tree has written lived at
//! the SENDER, which cannot observe the quantity the decision needs. These
//! arms move the decision to the RECEIVER — which holds the frontier, the
//! lateness distribution and the rank — and make `α` a thing to READ
//! (`α = S(ℓ*)`) rather than a thing to declare.
//!
//! **What is asserted, in the order it can fail.**
//!
//!   1. **THE GATES ECHO, TWO-SIDED, AT BOTH ENDPOINTS.** The request law is
//!      consumed at the receiver (which builds the message) AND at the sender
//!      (which serves it, and whose gap producer the seam suppresses), so the
//!      CONTROL's absence must be as mechanically assertable as the arm's
//!      presence. A row whose gates are not readable off its own log is VOID.
//!   2. **THE CONTROL IS THE SHIPPED MACHINE.** `[REQ] on=0 sent=0`,
//!      `[REQS] on=0 served=0`, and `[FCAUSE] gap_data > 0` — the per-seq
//!      SACK→gap producer is what recovers holes today, and this is the
//!      reading the treatment is contrasted against.
//!   3. **ARM (A) REACHES THE WIRE AT BOTH ENDS.** `[REQ] sent > 0` at the
//!      receiver (**WL2's producer half**) and `[REQS] served > 0` at the
//!      sender (**WL2**) — MEASUREMENT DISCIPLINE rule 1: prove the mechanism
//!      under test executes, at every seat it has to execute at.
//!   4. **THE COLLISION SEAM CLOSED, AND `[FCAUSE] gap_data → 0` IS THE
//!      PROOF.** §16.83.4's identifiability argument is CONDITIONAL on the
//!      receiver being the single authority: `ρ̂_heal(ℓ) = π₀·f(ℓ)` holds
//!      exactly on `[0, ℓ*)` only because no copy has flown there. If the gap
//!      producer kept firing, the estimate would be censored in §16.77.8a's
//!      own direction and the arm would be measuring a different law. This is
//!      the clause that makes the seam a fact rather than an intention.
//!   5. **`sack_tx` WAS NOT TOUCHED.** The SACK still flows: store release is
//!      ADR-0060's, pruning `sent_store` on SACK was refuted structurally
//!      UNSAFE on 2026-07-07, and a request law that touched it would be
//!      re-running a refuted experiment. Asserted as *the transfer completes
//!      under arm (A)* — a broken release wedges it.
//!   6. **`m ≡ 1` UNDER (A) ALONE.** Arm (A) isolates the TIMING lever: the
//!      bytes on the wire are today's per-seq copy (`[REQS] copy > 0`,
//!      `coded = 0`), which is what keeps `[RFA] dup_src` comparable to CTL.
//!   7. **(B) CHANGES THE VOCABULARY, AND THE `m` LAW IS CHECKED AGAINST THE
//!      RECEIVER'S OWN `π̂₀`.** `m = clamp(⌈k_½(π̂₀)⌉, 1, A*)` with
//!      `k_½ = ln 2 / (−ln π₀)`, so `π̂₀ > ½ ⇒ m ≥ 2` — an implication of the
//!      law itself, checked against the `rho_heal0` the same run printed. A
//!      hard `m > 1` assertion would be a claim about LOOPBACK's heal share,
//!      which no cell reading may be taken from.
//!   8. **`[LATE]` ECHOES `lstar_us` AND `knee_bind`** (**WL1 / WK**), and
//!      `[RFA] rep_redundant` is present under (B).
//!   9. **THE SINGLE-PATH CORNER.** At `N = 1` the acting threshold is 0 —
//!      request immediately, which IS the shipped machine — and with (B)
//!      absent `m ≡ 1`. **The `m = 1` half of §16.83.2's corner is a claim
//!      about `π₀`, and LOOPBACK'S `π₀` IS NOT `c1`'s** (this shim measures
//!      `rho_heal0 = 0.5`; D0 measured 0.0077 at `c1`), so that control
//!      belongs to the L1 battery and is deliberately not asserted here.
//!
//! **THIS BINARY FAILS ON THE PRE-CHANGE ENGINE.** `RWM_RECV_REQUEST_LAW`,
//! `RWM_RANK_FEEDBACK`, `[REQ]` and `[REQS]` do not exist there, so clauses 1
//! and 3 read missing fields.
//!
//! **THE LOOPBACK FINDING THIS TEST IS WRITTEN AROUND, AND DOES NOT HIDE.**
//! `[LATE]` on L0 loopback reports `lstar_us = 0` with `knee_bind = 1.0` on
//! BOTH topologies, because `d` (the mean ARQ resolution) exceeds the observed
//! knee `H` there, so `(H − d)⁺ = 0`. That is a property of the loopback shim
//! and NOT of any cell. **WL1 (`lstar_us > 2000` at the duals) is therefore an
//! L1 witness and is NOT asserted here** — this binary asserts only that the
//! field EXISTS and is echoed, which is what the pre-registration needs to be
//! able to read.
//!
//! **What this deliberately does NOT assert.** Any VALUE of the realized false
//! fraction, of goodput, of `ℓ*`, or of the knee. Loopback's dispersion is the
//! host scheduler's and its loss is the shim's GE process. This is the
//! INSTRUMENT gate that must pass before the L1 battery is worth running.
//!
//! **Nothing here flips a default.** Both gates ship ABSENT.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

use gauge::{field, opt_field, u64_field};

/// The base arm. `RWM_DIAG` carries `[REQ]`, `[REQS]`, `[LATE]`, `[FCAUSE]`
/// and `[RFA]`; no gate here changes a law.
const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// One loopback transfer under `extra`. Returns `(sender log, receiver log)` —
/// BOTH, because the arm has a seat at each end and a one-sided reading cannot
/// tell "never built" from "never served".
///
/// Both arms ride BOTH endpoints; the harness clears every inherited `RWM_*`
/// var, so an absent arm is absent rather than inherited. CLAUSE 5, in its
/// operational form, is `run_perf_client`'s success assertion: the SACK still
/// clocks store release, so the transfer completes — a request law that
/// touched `sack_tx` would wedge the sender's flow control and show there.
/// The receiver log is taken once a `[REQ]` readout post-dating the transfer
/// landed (with its cadence siblings `[LATE]`/`[RFA]`).
fn run(paths: usize, netem: Option<&str>, bytes: &str, extra: &[(&str, &str)]) -> (String, String) {
    let mut env = ARM.to_vec();
    env.extend_from_slice(extra);
    loopback::transfer(loopback::Transfer {
        paths,
        env: &env,
        client_env: &loopback::shaped(netem),
        bytes,
        srv_tag: Some("[REQ] "),
        ..Default::default()
    })
}

// ── READERS ─────────────────────────────────────────────────────────────

fn last_with<'a>(log: &'a str, pat: &str) -> &'a str {
    gauge::require(
        log,
        pat,
        "the gauge is unreachable, which is the DEAD-GAUGE reading this test \
         exists to fail on",
    )
}

/// CLAUSE 1: the arm's own axis, READ AT BOTH SEATS.
///
/// **WHY THE TWO SEATS ARE READ OFF DIFFERENT LINES HERE.** `[GATES]` is a
/// `tracing` record, and the perf CLIENT in this harness installs no
/// subscriber — its stderr carries the `eprintln!` gauges and nothing
/// else. (On the L1 driver both endpoint logs carry `[GATES]`, and
/// `recvlaw_battery.sh` checks both.) So the RECEIVER's arm is read off
/// `[GATES]` — what was ASKED FOR — and the SENDER's off
/// `[REQS] on=`, the RESOLVED arm at the seat that consumes it. That is the
/// STRONGER reading of the two, not a weaker substitute: `on=` IS the
/// predicate the serving loop is gated on, so the producer and the consumer
/// are asserted to AGREE about whether the arm is live.
fn assert_gates(cli: &str, srv: &str, law: u8, rank: u8) {
    let g = last_with(srv, "[GATES] ");
    assert!(
        g.contains(&format!("RWM_RECV_REQUEST_LAW={law}")),
        "receiver: the request-law gate must echo its RESOLVED value — a row \
         whose arm is not readable off its own log is VOID:\n{g}"
    );
    assert!(
        g.contains(&format!("RWM_RANK_FEEDBACK={rank}")),
        "receiver: the rank-feedback gate must echo its RESOLVED value:\n{g}"
    );
    let r = last_with(cli, "[REQS] ");
    let want = u8::from(law == 1 || rank == 1);
    assert_eq!(
        field(r, "on="),
        want.to_string(),
        "sender: the serving seat resolved the wrong arm — the producer \
         and the consumer disagree about whether the arm is live:\n{r}"
    );
}

// ── THE CONTROL ─────────────────────────────────────────────────────────

/// CLAUSES 1, 2: with both arms absent nothing is built, nothing is served,
/// and the shipped gap machinery is what recovers holes.
#[test]
fn the_control_builds_no_request_and_the_gap_producer_is_what_recovers() {
    let (cli, srv) = run(2, Some("c2,c3"), "24000000", &[]);
    assert_gates(&cli, &srv, 0, 0);

    let req = last_with(&srv, "[REQ] ");
    println!("[recvlaw-reach] CTL receiver {req}");
    assert_eq!(field(req, "on="), "0", "{req}");
    assert_eq!(field(req, "rank="), "0", "{req}");
    assert_eq!(
        u64_field(req, "sent="),
        0,
        "the CONTROL built a RepairRequest — the arm is not absent: {req}"
    );
    assert_eq!(u64_field(req, "spans="), 0, "{req}");

    let reqs = last_with(&cli, "[REQS] ");
    println!("[recvlaw-reach] CTL sender {reqs}");
    assert_eq!(field(reqs, "on="), "0", "{reqs}");
    assert_eq!(
        u64_field(reqs, "served="),
        0,
        "the CONTROL served a request — the consumer is not disarmed: {reqs}"
    );
    assert_eq!(
        field(reqs, "wa1_none_frac="),
        "-",
        "an absent fraction renders `-`, never 0: {reqs}"
    );

    // CLAUSE 2: the per-seq SACK→gap producer IS the shipped recovery path,
    // and this is the reading the treatment's `gap_data = 0` is contrasted
    // against. A control with `gap_data = 0` would make clause 4 vacuous.
    let fc = last_with(&cli, "[FCAUSE] ");
    println!("[recvlaw-reach] CTL {fc}");
    assert!(
        u64_field(fc, "gap_data=") > 0,
        "[FCAUSE] gap_data = 0 on the CONTROL over a c2,c3-lossy dual transfer \
         — the shipped gap producer never fired, so the treatment arm's \
         `gap_data → 0` would prove nothing:\n{fc}"
    );
}

// ── ARM (A): THE TIMING LEVER ───────────────────────────────────────────

/// CLAUSES 1, 3, 4, 5, 6, 8: arm (A) builds, serves, closes the seam, keeps
/// the copy, and completes the transfer.
#[test]
fn arm_a_requests_are_built_served_and_the_gap_producer_is_suppressed() {
    let (cli, srv) = run(2, Some("c2,c3"), "24000000", &[("RWM_RECV_REQUEST_LAW", "1")]);
    assert_gates(&cli, &srv, 1, 0);

    // The mechanism-liveness echo. A `tracing` record, so it is read at the
    // seat whose subscriber exists in this harness — see `assert_gates`.
    assert!(
        srv.contains("receiver-seat repair request ACTIVE"),
        "the arm never echoed its own activation — MEASUREMENT DISCIPLINE \
         rule 1"
    );

    // CLAUSE 3, producer half.
    let req = last_with(&srv, "[REQ] ");
    println!("[recvlaw-reach] A receiver {req}");
    assert_eq!(field(req, "on="), "1", "{req}");
    let sent = u64_field(req, "sent=");
    assert!(
        sent > 0,
        "[REQ] sent=0 — the receiver never put a RepairRequest on the wire, so \
         with the gap producer suppressed nothing at all requested a repair. \
         The arm is unreachable:\n{req}"
    );
    assert!(u64_field(req, "spans=") > 0, "{req}");

    // CLAUSE 6: (A) ALONE IS THE COPY. `m ≡ 1` isolates the TIMING lever.
    assert_eq!(
        u64_field(req, "m_max="),
        1,
        "[REQ] m_max > 1 with RWM_RANK_FEEDBACK ABSENT — arm (A) changed the \
         VOCABULARY as well as the timing, so the two levers are confounded \
         and neither is measurable:\n{req}"
    );

    // CLAUSE 3, server half — WL2.
    let reqs = last_with(&cli, "[REQS] ");
    println!("[recvlaw-reach] A sender {reqs}");
    assert_eq!(field(reqs, "on="), "1", "{reqs}");
    let served = u64_field(reqs, "served=");
    assert!(
        served > 0,
        "[REQS] served=0 — the sender received requests and served none (WL2). \
         The receiver's authority has no server:\n{reqs}"
    );
    assert_eq!(
        u64_field(reqs, "copy="),
        served,
        "[REQS] a coded answer under `m = 1` — the m=1 corner must serve the \
         per-seq copy out of `sent_store`, which is what keeps `[RFA] dup_src` \
         comparable to the control:\n{reqs}"
    );
    assert_eq!(u64_field(reqs, "coded="), 0, "{reqs}");

    // CLAUSE 4: THE SEAM. This is the whole identifiability argument.
    //
    // `[FCAUSE]` is emitted at the sender gauge's teardown IFF the gap loop
    // classified at least one fire (`is_fire_cause_site`: `n > 0`). Under (A)
    // the SACK→gap producer is suppressed, so the only producer left is the
    // tail sweep (`timer`/`other`), which can legitimately fire ZERO times on
    // a loopback run — and then there is no line at all. That absence is not
    // a dead gauge: the emission site's reachability in this harness is
    // proven by the CONTROL (`[FCAUSE] gap_data > 0` there), and this run's
    // sender diag surface is live (`[REQS] on=1 served > 0` above). So the
    // two readings are:
    //   * line present ⇒ `gap_data = gap_refresh = 0` on the line itself;
    //   * line absent  ⇒ `n = 0` ⇒ `gap_data = gap_refresh = 0` exactly.
    // Either way the seam claim is asserted, and the repair traffic this
    // lossy transfer needed provably went through the request path
    // (`served > 0`), not through a silent third producer.
    match gauge::last_line(&cli, "[FCAUSE] ") {
        Some(fc) => {
            println!("[recvlaw-reach] A {fc}");
            assert_eq!(
                u64_field(fc, "gap_data="),
                0,
                "[FCAUSE] gap_data > 0 with the request law armed — the collision seam \
                 did NOT close, so a copy still flies inside `[0, ℓ*)`, so \
                 `ρ̂_heal` is censored in §16.77.8a's own direction and the arm is \
                 measuring a different law than the one it names:\n{fc}"
            );
            assert_eq!(
                u64_field(fc, "gap_refresh="),
                0,
                "[FCAUSE] gap_refresh > 0 with the request law armed — the SACK→gap \
                 producer is armed at ONE site and both its arms must go with it:\n{fc}"
            );
        }
        None => {
            println!(
                "[recvlaw-reach] A: no [FCAUSE] line — the gap loop classified 0 \
                 fires (tail sweep never fired); gap_data = gap_refresh = 0 exactly"
            );
            assert!(
                !cli.contains("gap_data="),
                "a `gap_data=` token without an `[FCAUSE]` tag — the line was \
                 mangled, not absent:\n{cli}"
            );
        }
    }

    // CLAUSE 8: the threshold and the bind gauge are echoed, so the
    // pre-registration can read them. NO VALUE is asserted — see the header's
    // loopback finding.
    let late = last_with(&srv, "[LATE] ");
    println!("[recvlaw-reach] A {late}");
    assert!(late.contains("lstar_us="), "[LATE] must echo the threshold: {late}");
    assert!(late.contains("knee_bind="), "[LATE] must echo the bind gauge: {late}");
    assert!(late.contains("delta=") && late.contains("bar="), "{late}");
    let _ = opt_field(late, "lstar_us=");
}

// ── ARM (B): THE VOCABULARY LEVER ───────────────────────────────────────

/// CLAUSES 1, 3, 7, 8: (A)+(B) composes, the `m` law agrees with the
/// receiver's own `π̂₀`, and `rep_redundant` is present.
#[test]
fn arm_b_composes_and_the_m_law_agrees_with_the_receivers_own_pi0() {
    let (cli, srv) = run(
        2,
        Some("c2,c3"),
        "24000000",
        &[("RWM_RECV_REQUEST_LAW", "1"), ("RWM_RANK_FEEDBACK", "1")],
    );
    assert_gates(&cli, &srv, 1, 1);

    let req = last_with(&srv, "[REQ] ");
    let late = last_with(&srv, "[LATE] ");
    let reqs = last_with(&cli, "[REQS] ");
    println!("[recvlaw-reach] A+B receiver {req}");
    println!("[recvlaw-reach] A+B receiver {late}");
    println!("[recvlaw-reach] A+B sender {reqs}");

    assert_eq!(field(req, "rank="), "1", "{req}");
    assert!(u64_field(req, "sent=") > 0, "the composed arm built nothing: {req}");
    assert!(u64_field(reqs, "served=") > 0, "the composed arm served nothing: {reqs}");

    // CLAUSE 7: THE `m` LAW, CHECKED AGAINST THE SAME RUN'S OWN `π̂₀`.
    // `k_½ = ln 2 / (−ln π₀)`, so `π₀ > ½ ⇒ k_½ > 1 ⇒ ⌈k_½⌉ ≥ 2`. This is an
    // implication of the law, not a claim about loopback's heal share — which
    // is why it is written as an implication.
    let rho0 = field(late, "rho_heal0=");
    let m_max = u64_field(req, "m_max=");
    println!("[recvlaw-reach] A+B rho_heal0={rho0} m_max={m_max}");
    assert!(m_max >= 1, "{req}");
    if rho0 != "-" {
        let p: f64 = rho0.parse().expect("rho_heal0 is a fraction");
        if p > 0.5 {
            assert!(
                m_max >= 2,
                "the receiver's own π̂₀ = {p} gives k_½ = ln2/(−ln π₀) > 1, so \
                 ⌈k_½⌉ ≥ 2, yet the widest span asked for was m = {m_max}. The \
                 `m` law is not reading π̂₀:\n{req}\n{late}"
            );
        }
    }
    // Whenever a span WAS wider than one seq, the sender's `WA1` split must
    // account for it: every `m > 1` answer is either a coded equation or a
    // COUNTED refusal that fell back to a copy. A silent third outcome is what
    // this pins out.
    if m_max > 1 {
        let some = u64_field(reqs, "wa1_some=");
        let none = u64_field(reqs, "wa1_none=");
        assert!(
            some + none > 0,
            "[REQ] asked for m={m_max} spans but the sender's WA1 split is \
             empty — the span request never reached `generate_repair_range` at \
             all:\n{reqs}"
        );
        assert_eq!(
            u64_field(reqs, "coded="),
            some,
            "[REQS] coded answers must equal the WA1 `Some` count — a coded \
             answer with no accepted span is unaccounted:\n{reqs}"
        );
    }

    // CLAUSE 8: the false measurand under coded answers is present by NAME.
    let rfa = last_with(&srv, "[RFA] ");
    assert!(
        rfa.contains("rep_redundant="),
        "`rep_redundant` missing from [RFA] — the false measurand under coded \
         answers has no producer: {rfa}"
    );
}

// ── ARM (B) ALONE: THE VOCABULARY-ONLY WIRING TEST ──────────────────────

/// (B) without (A) is the shipped 2 ms trigger spoken in the deficit
/// vocabulary: the gap producer stays ARMED (the seam keys on (A) alone), so
/// `[FCAUSE] gap_data` must NOT go to zero. This is what makes the battery's
/// four arms separable — without it, (B)'s effect and the seam's are one
/// treatment.
#[test]
fn arm_b_alone_changes_the_vocabulary_and_leaves_the_seam_open() {
    let (cli, srv) = run(2, Some("c2,c3"), "24000000", &[("RWM_RANK_FEEDBACK", "1")]);
    assert_gates(&cli, &srv, 0, 1);

    let req = last_with(&srv, "[REQ] ");
    let fc = last_with(&cli, "[FCAUSE] ");
    println!("[recvlaw-reach] B-alone receiver {req}");
    println!("[recvlaw-reach] B-alone {fc}");

    assert_eq!(field(req, "on="), "0", "the request law must stay absent: {req}");
    assert_eq!(field(req, "rank="), "1", "{req}");
    assert!(
        u64_field(req, "sent=") > 0,
        "[REQ] sent=0 with (B) armed alone — the vocabulary-only arm has no \
         producer, so `{{CTL, A, B, A+B}}` collapses to `{{CTL, A}}`:\n{req}"
    );
    assert!(
        u64_field(fc, "gap_data=") > 0,
        "[FCAUSE] gap_data = 0 with (A) ABSENT — the seam keys on the request \
         law alone, so arm (B) must leave the shipped gap producer armed. If \
         it does not, (B) is not the vocabulary-only lever it is scored \
         as:\n{fc}"
    );
}

// ── THE SINGLE-PATH CONTROL ─────────────────────────────────────────────

/// CLAUSE 9: at `N = 1` the law's own corner is the SHIPPED MACHINE -- the
/// acting threshold is 0, i.e. request immediately. 16.83.2: the corner is
/// not an approximation of the law, it IS the law at those inputs.
///
/// **WHAT THIS RUNS, AND WHY IT IS ARM (A) ALONE.** The `m = 1` half of the
/// corner is a claim about `pi0`, and **LOOPBACK'S `pi0` IS NOT `c1`'s.** D0
/// measured `pi0 = 0.0077` at `c1`; this shim's single-path topology measures
/// `rho_heal0 = 0.5000` (485 of 970 holes closed by their own original),
/// which by the law's own arithmetic gives `k_half = ln2/(-ln 0.5) = 1.0` at
/// the readout and ABOVE 1 earlier in the run. **So `m > 1` at one path on
/// loopback is the `m` law reading its input CORRECTLY, not a defect** -- and
/// asserting `m = 1` here would be taking a cell reading off a shim, which
/// this file's header forbids. The `m = 1` control at `c1`/`sc2` belongs to
/// the L1 battery's MUST-NOT-MOVE clause, where `pi0` is the cell's own.
///
/// What IS mechanical at one path, and is asserted here: with (B) ABSENT
/// `request_m` returns 1 unconditionally, cross-path resolution is
/// structurally impossible, and the acting threshold is the shipped corner.
#[test]
fn the_single_path_corner_is_todays_machine() {
    let (cli, srv) = run(1, Some("c3"), "12000000", &[("RWM_RECV_REQUEST_LAW", "1")]);
    assert_gates(&cli, &srv, 1, 0);

    let req = last_with(&srv, "[REQ] ");
    let late = last_with(&srv, "[LATE] ");
    println!("[recvlaw-reach] N=1 {req}");
    println!("[recvlaw-reach] N=1 {late}");

    // THE ACTING THRESHOLD IS THE SHIPPED CORNER. On this shim it is the KNEE
    // that takes it there (`d` exceeds the observed `H`, so `(H - d)+ = 0`)
    // rather than `pi0 -> 0`; the law reaches the same answer from either
    // limit -- which is the whole content of 16.83.2 -- and `knee_bind` on the
    // same line says WHICH term bound.
    assert_eq!(
        u64_field(req, "lstar_us="),
        0,
        "the acting threshold is not 0 at ONE PATH -- the shipped machine \
         requests immediately there:\n{req}\n{late}"
    );
    // (B) ABSENT ==> `request_m` returns 1 at every input. MECHANICAL.
    assert_eq!(
        u64_field(req, "m_max="),
        1,
        "m > 1 with RWM_RANK_FEEDBACK ABSENT -- arm (A) changed the VOCABULARY \
         as well as the timing, so the two levers are confounded:\n{req}"
    );
    // The request law still REACHED THE WIRE at one path: the corner is the
    // law running, not the arm going silent.
    assert!(
        u64_field(req, "sent=") > 0,
        "[REQ] sent=0 at ONE PATH -- the corner must be the law RUNNING, not \
         the arm falling silent:\n{req}"
    );
    // Cross-path resolution is STRUCTURALLY impossible at one path.
    assert_eq!(field(late, "xp_frac="), "0.0000", "{late}");
    // Both bind gauges are on the line, so WHICH term bound is a reading.
    assert!(late.contains("knee_bind="), "{late}");
    assert!(late.contains("sampler_bind="), "{late}");
}
