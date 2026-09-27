//! The receiver-seat gauges `[LATE]`, `[RANK]` and the `[RFA]` counters
//! `rep_redundant` and `late_after_aban` fire. The request law (paper §7.6)
//! decides at the receiver, which needs: the hole's lateness, bracketed
//! (`[LATE] lo_*`/`hi_*`); the rank deficit `holes − pivots` over the
//! frontier span (`[RANK]`); and the false measurand under coded answers,
//! `repairs_fed − repairs_useful` (`rep_redundant`), plus a copy that landed
//! after the frontier gave up (`late_after_aban`). Clauses:
//!
//!   1. The fields exist and the lines fire.
//!   2. `[LATE] n > 0` over a lossy transfer, with `[SUCC] det > 0` as an
//!      independent witness.
//!   3. `[LATE] n = orig + rep + aban` and `n = xp_n + sp_n`.
//!   4. `xp_frac ≡ 0` on one path, `> 0` on two.
//!   5. `knee_us` is non-null on two paths: `H` is observed as the
//!      arrival-stall onset during a frontier freeze, and caps `ℓ*_recv`.
//!   6. `[RANK]`'s `deficit = holes − pivots` holds, with the tail over-count
//!      reported beside it.
//!   7. `[RFA]` carries both counters by name (`tools/l1/tail_matrix.sh`
//!      greps `late_after_aban`), and under the reliable window
//!      `late_after_aban` reads 0: the reorder buffer never delivers past a
//!      hole.
//!
//! No value of `ℓ*_recv`, the knee or either bind fraction is asserted, in
//! particular not whether the knee binds. The readouts ride `RWM_DIAG`.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

use gauge::{field, opt_field, u64_field};

const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// One loopback transfer. Returns the server (receiver) log, taken once a
/// `[LATE]` readout post-dating the transfer arrived.
fn run(paths: usize, netem: Option<&str>, bytes: &str) -> String {
    let (_cli, srv) = loopback::transfer(loopback::Transfer {
        paths,
        env: &ARM,
        client_env: &loopback::shaped(netem),
        bytes,
        srv_tag: Some("[LATE] "),
        ..Default::default()
    });
    srv
}

// ── Readers ─────────────────────────────────────────────────────────────

fn last_with<'a>(log: &'a str, pat: &str) -> &'a str {
    gauge::require(
        log,
        pat,
        "the RECEIVER's gauge is unreachable, which is the DEAD-GAUGE reading \
         this test exists to fail on",
    )
}

/// Clauses 1, 3, 6, 7 — everything that must hold on any topology.
fn assert_common(log: &str) -> (String, String, String) {
    let late = last_with(log, "[LATE] ").to_string();
    let rank = last_with(log, "[RANK] ").to_string();
    let rfa = last_with(log, "[RFA] ").to_string();

    // 3. The `[LATE]` identities, on the engine's own output.
    let n = u64_field(&late, "n=");
    assert_eq!(
        n,
        u64_field(&late, "orig=") + u64_field(&late, "rep=") + u64_field(&late, "aban="),
        "[LATE] n is not the sum of its three outcome classes: {late}"
    );
    assert_eq!(
        n,
        u64_field(&late, "xp_n=") + u64_field(&late, "sp_n="),
        "[LATE] n is not the sum of the same/cross split: {late}"
    );
    // Both ends of the bracket exist, and the upper never sits below the
    // lower.
    for p in ["p50", "p90", "p99"] {
        if let (Some(lo), Some(hi)) =
            (opt_field(&late, &format!("lo_{p}=")), opt_field(&late, &format!("hi_{p}=")))
        {
            assert!(hi >= lo, "[LATE] hi_{p} < lo_{p} — the bracket inverted: {late}");
        }
    }
    // The declared cost ratio is on the line, so a different one is a
    // rescale of a printed number rather than a hidden constant.
    assert!(late.contains("w=1.00"), "[LATE] must print its declared ratio: {late}");
    // Both bind fractions are present and are fractions.
    for k in ["knee_bind=", "sampler_bind="] {
        let v = field(&late, k);
        assert!(
            v == "-" || (0.0..=1.0).contains(&v.parse::<f64>().expect("fraction")),
            "`{k}` must be a fraction or `-`: {late}"
        );
    }

    // 6. `[RANK]`'s identity.
    assert_eq!(
        u64_field(&rank, "deficit="),
        u64_field(&rank, "holes=").saturating_sub(u64_field(&rank, "pivots=")),
        "[RANK] deficit is not holes − pivots: {rank}"
    );
    assert!(u64_field(&rank, "reports=") > 0, "[RANK] never took a reading: {rank}");
    assert!(rank.contains("tail_overcount="), "[RANK] must report its tail correction: {rank}");

    // 7. Both `[RFA]` counters, by name.
    for k in ["rep_redundant=", "late_after_aban="] {
        assert!(
            rfa.contains(k),
            "`{k}` missing from the [RFA] line — its NAME is part of the \
             contract the Track B scrape greps for: {rfa}"
        );
    }
    // Under the reliable window the reorder buffer never delivers past a
    // hole, so a copy can never land below the frontier.
    assert_eq!(
        u64_field(&rfa, "late_after_aban="),
        0,
        "[RFA] late_after_aban > 0 under the RELIABLE window — the reorder \
         buffer delivered past a hole, which it is built not to do. A FINDING \
         about the engine, caught here: {rfa}"
    );
    (late, rank, rfa)
}

// ── The two topologies ──────────────────────────────────────────────────

/// Clauses 1-4, 6, 7 at one path: everything fires, and `xp_frac ≡ 0`.
#[test]
fn the_receiver_seat_gauges_fire_and_cross_path_is_zero_on_one_path() {
    let log = run(1, Some("c3"), "12000000");
    assert!(log.contains("RWM_DIAG=1"), "the receiver's [GATES] echo lacks RWM_DIAG=1:\n{log}");
    let (late, rank, rfa) = assert_common(&log);
    println!("[late-reach] N=1 {late}");
    println!("[late-reach] N=1 {rank}");

    // 2. The gauge fires, and an independent witness agrees there were holes.
    let n = u64_field(&late, "n=");
    assert!(
        n > 0,
        "[LATE] n=0 over a c3-lossy transfer — no hole was ever bracketed:\n{late}"
    );
    let succ = last_with(&log, "[SUCC] ");
    let det = u64_field(succ, "det=");
    println!("[late-reach] witness [SUCC] det={det} against [LATE] n={n}");
    assert!(
        det > 0,
        "[LATE] n={n} while the independent [SUCC] witness reports det=0 — the \
         two gauges disagree about whether this transfer had holes:\n{succ}"
    );

    // 4. The control: cross-path is structurally impossible at one path.
    assert_eq!(
        u64_field(&late, "xp_n="),
        0,
        "[LATE] xp_n > 0 at ONE PATH — a cross-path resolution is structurally \
         impossible there, so the class is measuring something other than what \
         it is named for:\n{late}"
    );
    assert_eq!(field(&late, "xp_frac="), "0.0000", "{late}");
    let _ = rfa;
}

/// Clauses 4, 5 at two paths: the cross-path class is populated and the knee
/// is observable.
#[test]
fn cross_path_and_the_knee_are_populated_on_two_paths() {
    let log = run(2, Some("c2,c3"), "24000000");
    let (late, rank, _rfa) = assert_common(&log);
    println!("[late-reach] N=2 {late}");
    println!("[late-reach] N=2 {rank}");

    assert!(u64_field(&late, "n=") > 0, "[LATE] n=0 on the dual topology: {late}");
    assert!(
        u64_field(&late, "xp_n=") > 0,
        "[LATE] xp_n = 0 on the dual c2,c3 topology — the cross-path class is \
         unreachable, so no hole can ever be attributed to the scheduler:\n{late}"
    );
    let xf: f64 = field(&late, "xp_frac=").parse().expect("fraction");
    assert!(xf > 0.0 && xf <= 1.0, "xp_frac out of range: {late}");

    // 5. The knee is observable: `H` caps `ℓ*_recv`.
    let knee = opt_field(&late, "knee_us=");
    println!(
        "[late-reach] knee_us={knee:?} knee_n={} d_us={:?} lstar_us={:?} \
         knee_bind={} sampler_bind={}",
        u64_field(&late, "knee_n="),
        opt_field(&late, "d_us="),
        opt_field(&late, "lstar_us="),
        field(&late, "knee_bind="),
        field(&late, "sampler_bind="),
    );
    assert!(
        knee.is_some(),
        "[LATE] knee_us=- on the dual topology — the arrival-stall onset was \
         never observed, so `H` has no producer and `ℓ*_recv`'s cap cannot be \
         evaluated at all:\n{late}"
    );
    assert_eq!(
        knee.is_none(),
        u64_field(&late, "knee_n=") == 0,
        "`knee_us` must render `-` IFF `knee_n = 0`: {late}"
    );
    // `ℓ*_recv` exists once either term does, and is `-`, never 0, when
    // neither does (0 would read "request immediately", the shipped corner).
    assert!(
        opt_field(&late, "lstar_us=").is_some(),
        "[LATE] lstar_us=- with a knee present — the hypothetical threshold is \
         not being computed: {late}"
    );
}
