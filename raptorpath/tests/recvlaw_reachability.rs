//! The receiver-seat request law (paper §7.6) is reachable at both
//! endpoints, closes the collision seam, and its absent arm is the shipped
//! machine. Arm (A) `RWM_RECV_REQUEST_LAW` moves the repair request to the
//! receiver; arm (B) `RWM_RANK_FEEDBACK` widens it to a span of
//! `m = clamp(⌈k_½(π̂₀)⌉, 1, A*)`, `k_½ = ln 2 / (−ln π₀)`. Clauses:
//! 1. the gates echo at both seats;
//! 2. the control builds and serves nothing, and the per-seq SACK→gap
//!    producer recovers holes (`[FCAUSE] gap_data > 0`);
//! 3. (A) builds (`[REQ] sent > 0`) and serves (`[REQS] served > 0`);
//! 4. (A) suppresses the gap producer — the identifiability condition that
//!    no copy flies inside `[0, ℓ*)`;
//! 5. SACK-clocked release is untouched (the transfer completes);
//! 6. (A) alone keeps `m ≡ 1`;
//! 7. (B) obeys `π̂₀ > ½ ⇒ m ≥ 2` against the same run's `rho_heal0`;
//! 8. `[LATE]` echoes `lstar_us` and `knee_bind`;
//! 9. at one path the acting threshold is 0.
//!
//! No value of ℓ*, the knee or goodput is asserted: on loopback `d` exceeds
//! the observed knee, so `lstar_us = 0`, and loopback's `π₀` is not a cell's.
//! Both gates default off.

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

/// One loopback transfer under `extra`. Returns `(sender log, receiver log)`:
/// the arm has a seat at each end, and a one-sided reading cannot tell
/// "never built" from "never served". Clause 5 is `run_perf_client`'s
/// success assertion — a request law that touched `sack_tx` would wedge the
/// sender's flow control. The receiver log is taken once a `[REQ]` readout
/// post-dating the transfer landed.
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

/// Clause 1: the arm read at both seats. The perf client installs no
/// `tracing` subscriber, so the receiver's arm is read off `[GATES]` (what
/// was asked for) and the sender's off `[REQS] on=` (the predicate the
/// serving loop is gated on), asserting producer and consumer agree.
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

// ── The control ─────────────────────────────────────────────────────────

/// Clauses 1, 2: with both arms absent nothing is built, nothing is served,
/// and the shipped gap machinery recovers holes.
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

    // Clause 2: the per-seq SACK→gap producer is the shipped recovery path;
    // a control with `gap_data = 0` would make clause 4 vacuous.
    let fc = last_with(&cli, "[FCAUSE] ");
    println!("[recvlaw-reach] CTL {fc}");
    assert!(
        u64_field(fc, "gap_data=") > 0,
        "[FCAUSE] gap_data = 0 on the CONTROL over a c2,c3-lossy dual transfer \
         — the shipped gap producer never fired, so the treatment arm's \
         `gap_data → 0` would prove nothing:\n{fc}"
    );
}

// ── Arm (A): the timing lever ───────────────────────────────────────────

/// Clauses 1, 3, 4, 5, 6, 8: arm (A) builds, serves, closes the seam, keeps
/// the copy, and completes the transfer.
#[test]
fn arm_a_requests_are_built_served_and_the_gap_producer_is_suppressed() {
    let (cli, srv) = run(2, Some("c2,c3"), "24000000", &[("RWM_RECV_REQUEST_LAW", "1")]);
    assert_gates(&cli, &srv, 1, 0);

    // Mechanism-liveness echo (measurement-discipline rule 1), read at the
    // seat whose subscriber exists in this harness — see `assert_gates`.
    assert!(
        srv.contains("receiver-seat repair request ACTIVE"),
        "the arm never echoed its own activation — MEASUREMENT DISCIPLINE \
         rule 1"
    );

    // Clause 3, producer half.
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

    // Clause 6: (A) alone serves the copy; `m ≡ 1` isolates the timing lever.
    assert_eq!(
        u64_field(req, "m_max="),
        1,
        "[REQ] m_max > 1 with RWM_RANK_FEEDBACK ABSENT — arm (A) changed the \
         VOCABULARY as well as the timing, so the two levers are confounded \
         and neither is measurable:\n{req}"
    );

    // Clause 3, server half.
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

    // Clause 4: the seam. `[FCAUSE]` is emitted only if the gap loop
    // classified at least one fire. Under (A) the SACK→gap producer is
    // suppressed and the tail sweep may fire zero times, so the line may be
    // absent; the control proves the emission site is reachable. Either way:
    //   * line present ⇒ `gap_data = gap_refresh = 0` on the line itself;
    //   * line absent  ⇒ `n = 0` ⇒ `gap_data = gap_refresh = 0` exactly.
    match gauge::last_line(&cli, "[FCAUSE] ") {
        Some(fc) => {
            println!("[recvlaw-reach] A {fc}");
            assert_eq!(
                u64_field(fc, "gap_data="),
                0,
                "[FCAUSE] gap_data > 0 with the request law armed — the collision seam \
                 did NOT close, so a copy still flies inside `[0, ℓ*)`, so \
                 `ρ̂_heal` is censored in the direction paper §7.4 names and the arm is \
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

    // Clause 8: the threshold and the bind gauge are echoed; no value is
    // asserted (see the header).
    let late = last_with(&srv, "[LATE] ");
    println!("[recvlaw-reach] A {late}");
    assert!(late.contains("lstar_us="), "[LATE] must echo the threshold: {late}");
    assert!(late.contains("knee_bind="), "[LATE] must echo the bind gauge: {late}");
    assert!(late.contains("delta=") && late.contains("bar="), "{late}");
    let _ = opt_field(late, "lstar_us=");
}

// ── Arm (B): the vocabulary lever ───────────────────────────────────────

/// Clauses 1, 3, 7, 8: (A)+(B) composes, the `m` law agrees with the
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

    // Clause 7: `π₀ > ½ ⇒ k_½ > 1 ⇒ ⌈k_½⌉ ≥ 2`, checked against the same
    // run's `π̂₀` — an implication of the law, not a claim about loopback's
    // heal share.
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
    // Every `m > 1` answer is either a coded equation or a counted refusal
    // that fell back to a copy (the sender's `WA1` split); no third outcome.
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

    // Clause 8: the false measurand under coded answers is present by name.
    let rfa = last_with(&srv, "[RFA] ");
    assert!(
        rfa.contains("rep_redundant="),
        "`rep_redundant` missing from [RFA] — the false measurand under coded \
         answers has no producer: {rfa}"
    );
}

// ── Arm (B) alone: the vocabulary-only wiring test ──────────────────────

/// (B) without (A) speaks the deficit vocabulary with the shipped trigger:
/// the seam keys on (A) alone, so `[FCAUSE] gap_data` must stay above zero.
/// This keeps the four arms {CTL, A, B, A+B} separable.
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

// ── The single-path control ─────────────────────────────────────────────

/// Clause 9: at `N = 1` the law's corner is the shipped machine — the acting
/// threshold is 0, request immediately (paper §7.6).
///
/// Arm (A) alone: the `m = 1` half of the corner is a claim about `π₀`, and
/// loopback's single path measures `rho_heal0 ≈ 0.5`, where `m > 1` is the
/// law reading its input correctly. That control belongs at L1. What is
/// mechanical here: with (B) absent `request_m` returns 1, cross-path
/// resolution is impossible, and the threshold is the shipped corner.
#[test]
fn the_single_path_corner_is_todays_machine() {
    let (cli, srv) = run(1, Some("c3"), "12000000", &[("RWM_RECV_REQUEST_LAW", "1")]);
    assert_gates(&cli, &srv, 1, 0);

    let req = last_with(&srv, "[REQ] ");
    let late = last_with(&srv, "[LATE] ");
    println!("[recvlaw-reach] N=1 {req}");
    println!("[recvlaw-reach] N=1 {late}");

    // On this shim the knee takes the threshold to 0 (`d` exceeds the
    // observed `H`, so `(H − d)⁺ = 0`) rather than `π₀ → 0`; `knee_bind`
    // says which term bound.
    assert_eq!(
        u64_field(req, "lstar_us="),
        0,
        "the acting threshold is not 0 at ONE PATH -- the shipped machine \
         requests immediately there:\n{req}\n{late}"
    );
    // (B) absent ⇒ `request_m` returns 1 at every input.
    assert_eq!(
        u64_field(req, "m_max="),
        1,
        "m > 1 with RWM_RANK_FEEDBACK ABSENT -- arm (A) changed the VOCABULARY \
         as well as the timing, so the two levers are confounded:\n{req}"
    );
    // The request law still reaches the wire at one path: the corner is the
    // law running, not the arm going silent.
    assert!(
        u64_field(req, "sent=") > 0,
        "[REQ] sent=0 at ONE PATH -- the corner must be the law RUNNING, not \
         the arm falling silent:\n{req}"
    );
    // Cross-path resolution is impossible at one path.
    assert_eq!(field(late, "xp_frac="), "0.0000", "{late}");
    // Both bind gauges are on the line, so WHICH term bound is a reading.
    assert!(late.contains("knee_bind="), "{late}");
    assert!(late.contains("sampler_bind="), "{late}");
}
