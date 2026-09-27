//! The receiver's successor-arrival gauge `[SUCC]` fires, and its three
//! outcomes (closed by the original, by a repair, abandoned) partition the
//! holes the engine opens. It times each hole from detection to resolution,
//! the distribution a recovery clock positioned on gap-driven fires would be
//! derived from. Clauses, in the order they can fail:
//!
//!   1. `succ_report_line`'s format and the `-`-iff-none convention.
//!   2. The line fires from the receiver over a lossy plain-window transfer,
//!      with `det > 0`.
//!   3. The accounting identity on the engine's output:
//!      `det = orig_n + rep_n + aban_n + open + over`.
//!   4. The holes are real against an independent witness: `[RFA] fires > 0`
//!      (`[RFA]` classifies arrivals, `[SUCC]` times holes).
//!   5. At least one hole closes, quantiles are ordered
//!      `p50 ≤ p90 ≤ p99 ≤ mx`, and `orig_frac` is a fraction.
//!   6. The line echoes `gen=`, reading `gen=0` on plain window.
//!   7. The raw dump is off by default and on when asked (a default-on dump
//!      would be a receiver cost on every scored arm).
//!
//! No quantile value, `orig_frac` or crossing point is asserted: loopback's
//! redundancy is the shim's and the host scheduler's. The readout rides
//! `RWM_DIAG`/`RWM_FDIAG`. Own test binary: `RWM_L0_NETEM` is process-global
//! in the child.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

use gauge::{opt_f64_field as opt_field_f64, opt_field, str_field, u64_field};

use raptorpath::net::succ::{succ_report_line, Hist};

// ── 1: the pure pin ─────────────────────────────────────────────────────

#[test]
fn the_succ_line_format_and_the_dash_iff_none_convention_are_pinned() {
    let mut orig = Hist::default();
    orig.add(1_000);
    orig.add(2_000);
    let mut rep = Hist::default();
    rep.add(40_000);
    let empty = Hist::default();

    // The same/cross exposure split occupies the end of the line, so every
    // assertion below keeps its meaning.
    let mut sp = Hist::default();
    sp.add(700);
    let xp = Hist::default();
    let l = succ_report_line(
        false, 7, &orig, &rep, &empty, &sp, &xp, 3, 1, Some(2048), false, 0,
    );
    assert!(l.starts_with("[SUCC] gen=0 det=7 res=3 "), "{l}");
    // `n` beside every value: no quantile is readable without its sample
    // count.
    for (k, v) in [
        ("orig_n=", "2"),
        ("rep_n=", "1"),
        ("aban_n=", "0"),
        ("open=", "3"),
        ("over=", "1"),
        ("cross_us=", "2048"),
        ("orig_frac=", "0.6667"),
    ] {
        assert!(l.contains(&format!("{k}{v}")), "`{k}{v}` missing from {l}");
    }
    // `-` iff none on every slot of the empty outcome — not a 0, which a
    // parser would read as a measured zero microseconds.
    for k in ["aban_p50_us", "aban_p90_us", "aban_p99_us", "aban_mx_us", "aban_mean_us"] {
        assert!(l.contains(&format!("{k}=-")), "`{k}` must render `-` when n=0: {l}");
    }
    // A quantile is its bucket's lower edge, so it never exceeds the exact
    // maximum printed beside it.
    assert!(l.contains("orig_mx_us=2000"), "{l}");
    assert!(l.contains("rep_mx_us=40000"), "{l}");

    // The other side of every convention on one line: nothing measured.
    let e =
        succ_report_line(true, 0, &empty, &empty, &empty, &empty, &empty, 0, 0, None, true, 12);
    assert!(e.starts_with("[SUCC] gen=1 det=0 res=0 "), "{e}");
    assert!(e.contains("orig_frac=- cross_us=- dump=1/12"), "{e}");
    assert!(!e.contains("orig_frac=0"), "an absent fraction is `-`, never 0: {e}");
    // The split, pinned on the same two sides: populated and absent.
    assert!(l.contains("sp_n=1 xp_n=0 xp_frac=0.0000"), "{l}");
    assert!(
        e.ends_with("sp_n=0 xp_n=0 xp_frac=- sp_p50_us=- sp_p90_us=- xp_p50_us=- xp_p90_us=-"),
        "the A0.3 split renders `-` iff none and sits at the END of the line: {e}"
    );
}

// ── The reachability run ────────────────────────────────────────────────

/// The arm. `RWM_DIAG` carries the periodic `[SUCC]` readout (the L1
/// harnesses SIGKILL the server, so a `Drop`-only emission is unreachable
/// there). No gate here changes a law.
const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

/// One lossy plain-window loopback. Returns `(client log, server log)`.
///
/// The dump is absent by default (clause 7) and set only on the dump arm
/// (server side); the harness clears inherited `RWM_*` vars. The `c3` cell
/// (20 Mbit, 20 ms one-way, 5 ms jitter, GE p = 2 % / q = 40 % ⇒ ε ≈ 4.8 %)
/// shapes client egress, seeded; loss is what makes holes exist. The server
/// log is taken once a `[SUCC]` readout post-dating the transfer landed.
fn lossy_run(dump: bool) -> (String, String) {
    let dump_env: &[(&str, &str)] =
        if dump { &[("RWM_SUCC_DUMP", "1"), ("RWM_SUCC_DUMP_MAX", "5000")] } else { &[] };
    loopback::transfer(loopback::Transfer {
        env: &ARM,
        server_env: dump_env,
        srv_tag: Some("[SUCC] "),
        ..Default::default()
    })
}

/// Clauses 2-6: the gauge fires, partitions its own denominator, and agrees
/// with an independent witness.
#[test]
fn the_receiver_reports_the_successor_arrival_distribution() {
    let (_cli, log) = lossy_run(false);

    // The gate: a missing `[SUCC]` must read as an unreached emission site,
    // never as an unset gate.
    assert!(
        log.contains("RWM_DIAG=1"),
        "the server's [GATES] echo does not carry RWM_DIAG=1 — the arm did not \
         arm:\n{log}"
    );
    assert!(
        !log.contains("RWM_DIAG=0"),
        "the server's [GATES] echo carries BOTH sides of RWM_DIAG:\n{log}"
    );
    // 7a: the dump is off by default, and the echo says so.
    assert!(
        log.contains("RWM_SUCC_DUMP=0"),
        "the [GATES] echo must carry RWM_SUCC_DUMP=0 on an unarmed run — a \
         dump whose state is not readable off the run is not a measurement \
         boundary:\n{log}"
    );
    assert!(
        log.contains("RWM_SUCC_DUMP_MAX="),
        "the dump CAP must be echoed as its RESOLVED value:\n{log}"
    );

    // 2. The line fires, nonzero.
    let succ: Vec<&str> = log.lines().filter(|l| l.contains("[SUCC] ")).collect();
    assert!(
        !succ.is_empty(),
        "no [SUCC] line from the RECEIVER over a lossy transfer — the \
         successor-arrival gauge is unreachable, and the measurand the \
         fire-cause pass named still has no producer:\n{log}"
    );
    // Cumulative counters: the last line is the reading.
    let last = *succ.last().expect("non-empty");
    println!("[succ-reach] {} lines; last: {last}", succ.len());

    // 6. The configuration contract.
    assert!(
        last.contains("gen=0"),
        "this run is PLAIN WINDOW — the configuration the fire-cause pass \
         measured `gap_data` in — and the line must say so: {last}"
    );

    let det = u64_field(last, "det=");
    let res = u64_field(last, "res=");
    let orig_n = u64_field(last, "orig_n=");
    let rep_n = u64_field(last, "rep_n=");
    let aban_n = u64_field(last, "aban_n=");
    let open = u64_field(last, "open=");
    let over = u64_field(last, "over=");

    assert!(
        det > 0,
        "[SUCC] det=0 over a c3-lossy PLAIN-WINDOW transfer — the receiver \
         detected no hole at all, which is the DEAD-GAUGE reading this test \
         exists to fail on:\n{last}"
    );

    // 3. The accounting identity, on the engine's own output.
    assert_eq!(
        det,
        orig_n + rep_n + aban_n + open + over,
        "[SUCC] det is not the sum of its outcomes, its census and its \
         declared overflow — the three outcomes do not partition the holes: \
         {last}"
    );
    assert_eq!(res, orig_n + rep_n, "[SUCC] res must be orig_n + rep_n: {last}");

    // 4. The independent witness: different code, different events, same
    //    underlying loss.
    let rfa = log
        .lines()
        .rev()
        .find(|l| l.contains("[RFA] "))
        .unwrap_or_else(|| panic!("no [RFA] line to witness [SUCC] against:\n{log}"));
    let fires = u64_field(rfa, "fires=");
    println!("[succ-reach] witness [RFA] fires={fires} against [SUCC] det={det}");
    assert!(
        fires > 0,
        "[SUCC] det={det} while the independent [RFA] witness reports \
         fires=0 — the two gauges disagree about whether this transfer had \
         holes at all:\n{rfa}\n{last}"
    );

    // 5. The outcomes are populated and in range.
    assert!(
        res > 0,
        "[SUCC] res=0 with det={det} — every hole the receiver detected is \
         still open, so not one time-to-resolution was measured: {last}"
    );
    for name in ["orig", "rep", "aban"] {
        let n = u64_field(last, &format!("{name}_n="));
        let q: Vec<Option<u64>> = ["p50", "p90", "p99", "mx"]
            .iter()
            .map(|s| opt_field(last, &format!("{name}_{s}_us=")))
            .collect();
        if n == 0 {
            assert!(
                q.iter().all(Option::is_none),
                "[SUCC] {name}_n=0 but a quantile rendered a number — an \
                 absent reading is `-`, never a measured zero: {last}"
            );
            continue;
        }
        let v: Vec<u64> = q
            .into_iter()
            .map(|x| x.unwrap_or_else(|| panic!("[SUCC] {name}_n={n} but a slot is `-`: {last}")))
            .collect();
        assert!(
            v[0] <= v[1] && v[1] <= v[2] && v[2] <= v[3],
            "[SUCC] {name} quantiles are not ordered p50<=p90<=p99<=mx \
             ({v:?}) — the histogram's bucketing is wrong: {last}"
        );
    }
    let of = opt_field_f64(last, "orig_frac=")
        .unwrap_or_else(|| panic!("[SUCC] res={res} but orig_frac is `-`: {last}"));
    assert!(
        (0.0..=1.0).contains(&of),
        "[SUCC] orig_frac={of} is not a fraction: {last}"
    );
    assert!(
        (of - orig_n as f64 / res as f64).abs() < 1e-3,
        "[SUCC] orig_frac={of} disagrees with {orig_n}/{res}: {last}"
    );
    // The crossing point may legally read `-` ("the original leads at every
    // horizon"); asserted in range when present.
    if let Some(c) = opt_field(last, "cross_us=") {
        println!("[succ-reach] crossing point {c} us");
        assert!(c <= 3_600_000_000, "[SUCC] cross_us={c} is not a plausible age");
    }
    // The dump is off on this arm, so it must have written nothing.
    assert!(last.contains("dump=0/0"), "an unarmed dump must be 0/0: {last}");
    assert!(
        !log.contains("[SUCCDUMP]"),
        "a [SUCCDUMP] line on an arm with RWM_SUCC_DUMP unset — the raw dump \
         is not default-OFF, and every scored arm would pay for it:\n{last}"
    );
}

/// Clause 7b: armed, the dump emits raw records in the pinned batch format,
/// and its cap binds loudly rather than silently truncating the stream.
#[test]
fn the_raw_dump_is_absent_by_default_and_emits_records_when_armed() {
    let (_cli, log) = lossy_run(true);
    assert!(
        log.contains("RWM_SUCC_DUMP=1"),
        "the [GATES] echo does not carry RWM_SUCC_DUMP=1 — the dump arm did \
         not arm, so its absence below would be unreadable:\n{log}"
    );
    let dumps: Vec<&str> = log.lines().filter(|l| l.contains("[SUCCDUMP] ")).collect();
    assert!(
        !dumps.is_empty(),
        "RWM_SUCC_DUMP=1 produced no [SUCCDUMP] line over a lossy transfer — \
         the raw record path is unreachable:\n{log}"
    );
    let first = dumps[0];
    let n = u64_field(first, "n=");
    assert!(n > 0, "[SUCCDUMP] n=0: {first}");
    let d = str_field(first, "d=");
    let recs: Vec<&str> = d.split(';').collect();
    assert_eq!(recs.len(), n as usize, "[SUCCDUMP] n= disagrees with its records: {first}");
    for r in &recs {
        let (tag, us) = r
            .split_once(',')
            .unwrap_or_else(|| panic!("[SUCCDUMP] record `{r}` is not `<tag>,<us>`: {first}"));
        assert!(
            // `x` = closed by the sender's copy (HoleOutcome::Retransmit).
            matches!(tag, "o" | "x" | "r" | "a"),
            "[SUCCDUMP] record tag `{tag}` is not one of the four outcomes: {first}"
        );
        us.parse::<u64>()
            .unwrap_or_else(|e| panic!("[SUCCDUMP] `{us}` is not µs: {e} in {first}"));
    }
    // The quantile line rides beside the dump and still says what the dump did.
    let last = log
        .lines()
        .rev()
        .find(|l| l.contains("[SUCC] "))
        .unwrap_or_else(|| panic!("no [SUCC] line on the dump arm:\n{log}"));
    assert!(last.contains(" dump=1/"), "the line must echo the armed dump: {last}");
    println!("[succ-reach] dump arm: {} SUCCDUMP lines; {last}", dumps.len());
}
