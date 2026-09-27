//! The sender CPU decomposition `[CPUPROF]` is reached and fed. It is an
//! `eprintln!` from the destructor of a local of `run_window_sender`, so only
//! a `perf --client` run of the shipped binary can show the line fires and
//! the five seams sit on the path a real transfer takes (measurement-
//! discipline rule 1; the same exit shape `gauge_reachability.rs` covers).
//! Clauses, in the order they can fail:
//!
//!   1. The gate echo is two-sided: `RWM_CPUPROF=1` present, `=0` absent.
//!   2. The line fires exactly once per sender.
//!   3. Its token set parses as `net::cpuprof::report_line` renders it.
//!   4. Every one of the five seams is fed (`n > 0`): a seam wired into a
//!      path the window sender does not take would report a clean `0.0000`
//!      share that reads as "this cost nothing".
//!   5. The shares are coherent: each in `[0, 1]`, `attr` their sum,
//!      `unattr = 1 − attr`, and `attr ≤ 1` (the seams are disjoint).
//!   6. The gauge ships off: without the gate no `[CPUPROF]` line prints.
//!
//! No seam's share is asserted: loopback's decomposition is the host's. The
//! process CPU clock is read only on Linux and renders `-` elsewhere, so the
//! share clauses are conditional on a numeric reading and the `-` case is
//! asserted consistent across every derived field.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

/// The arm under test: the CPU decomposition on, nothing else changed.
/// `RWM_DIAG` is absent: the instrument is independent of the `[DIAG]`
/// surface.
const ARM: [(&str, &str); 2] = [("RWM_CPUPROF", "1"), ("RUST_LOG", "raptorpath=info")];

/// Run one `perf --client` transfer against a fresh server and return the
/// merged stdout+stderr the L1 drivers scrape.
fn run_transfer(env: &[(&str, &str)]) -> String {
    let srv = loopback::spawn_perf_server(
        &[loopback::free_addr()],
        env,
        &["--protocol-hint", "bulk", "--window-reliable"],
    );
    let mut args = loopback::perf_args("bulk", "8000000", "1").to_vec();
    // Generation coding on: the `enc` seam is the coded path, and without it
    // the headline column would be unfed for a harness reason. `perf_rwm_c.sh`
    // passes the same flag on every L1 battery arm.
    args.push("--window-generation-coding");
    loopback::run_perf_client(&srv.addrs, env, &args)
}

/// One parsed seam token: `<name>=<ms>/n<count>/<share|->`.
#[derive(Debug)]
struct SeamTok {
    name: String,
    ms: f64,
    n: u64,
    share: Option<f64>,
}

fn parse_seam(tok: &str) -> SeamTok {
    let (name, rest) = tok.split_once('=').expect("a seam token is name=value");
    let mut parts = rest.split('/');
    let ms = parts.next().expect("seam ms");
    let n = parts.next().expect("seam count");
    let share = parts.next().expect("seam share");
    assert!(
        parts.next().is_none(),
        "a seam token has exactly three fields: {tok}"
    );
    let n = n
        .strip_prefix('n')
        .unwrap_or_else(|| panic!("the seam count must render as `/n<count>`: {tok}"));
    SeamTok {
        name: name.to_string(),
        ms: ms
            .parse()
            .unwrap_or_else(|e| panic!("seam ms `{ms}` does not parse ({e}): {tok}")),
        n: n
            .parse()
            .unwrap_or_else(|e| panic!("seam count `{n}` does not parse ({e}): {tok}")),
        share: if share == "-" {
            None
        } else {
            Some(
                share
                    .parse()
                    .unwrap_or_else(|e| panic!("seam share `{share}` does not parse ({e}): {tok}")),
            )
        },
    }
}

fn parse_scalar(line: &str, key: &str) -> Option<f64> {
    gauge::opt_f64_field(line, &format!("{key}="))
}

#[test]
fn the_cpuprof_line_fires_and_every_seam_is_fed() {
    let log = run_transfer(&ARM);

    // 1. The gate, two-sided.
    assert!(
        log.contains("RWM_CPUPROF=1"),
        "the [GATES] echo does not carry RWM_CPUPROF=1 — the arm did not arm:\n{log}"
    );
    assert!(
        !log.contains("RWM_CPUPROF=0"),
        "the [GATES] echo carries BOTH sides of RWM_CPUPROF:\n{log}"
    );

    // 2. The line fires exactly once per sender — this fails if the
    //    destructor is the wrong site.
    let lines: Vec<&str> = log.lines().filter(|l| l.contains("[CPUPROF] ")).collect();
    assert_eq!(
        lines.len(),
        1,
        "expected exactly ONE [CPUPROF] line from one sender, got {}:\n{}",
        lines.len(),
        log
    );
    let line = lines[0];
    println!("[cpuprof-reach] {line}");

    // 3. The scalar fields parse.
    let run_ms = parse_scalar(line, "run_ms").expect("run_ms is never `-`");
    assert!(run_ms > 0.0, "a transfer has a positive wall span: {run_ms}");
    let cpu_ms = parse_scalar(line, "cpu_ms");
    let cores = parse_scalar(line, "cores");
    assert_eq!(
        cpu_ms.is_some(),
        cores.is_some(),
        "`cores` is derived from `cpu_ms`: the two must be available together: {line}"
    );

    // 4. Every seam is fed.
    let seams: Vec<SeamTok> = line
        .split_whitespace()
        .filter(|t| t.contains("/n"))
        .map(|t| parse_seam(t))
        .collect();
    let names: Vec<&str> = seams.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["enc", "src", "frm", "ser", "hand"],
        "the seam set and its ORDER are what an L1 parser indexes by: {line}"
    );
    for s in &seams {
        assert!(
            s.n > 0,
            "seam `{}` was never entered over a whole 8 MB transfer — it is \
             wired into a path the window sender does not take, and a results \
             table would print its 0 share as a measurement: {line}",
            s.name
        );
        assert!(
            s.ms >= 0.0,
            "seam `{}` reports negative time: {line}",
            s.name
        );
    }

    // 5. Arithmetic coherence, in particular `attr <= 1`: a nested seam added
    //    later shows up as an attribution above 100 %.
    let attr = parse_scalar(line, "attr");
    let unattr = parse_scalar(line, "unattr");
    match cpu_ms {
        None => {
            // The `-` case must be consistent: no derived field acquires a
            // number when its denominator has none.
            assert!(attr.is_none() && unattr.is_none(), "inconsistent `-`: {line}");
            for s in &seams {
                assert!(
                    s.share.is_none(),
                    "seam `{}` has a share with no CPU denominator: {line}",
                    s.name
                );
            }
            println!(
                "[cpuprof-reach] no process-CPU clock on this platform; \
                 share clauses skipped, `-` consistency asserted"
            );
        }
        Some(cpu) => {
            assert!(cpu > 0.0, "a transfer consumes CPU: {line}");
            let attr = attr.expect("attr accompanies cpu_ms");
            let unattr = unattr.expect("unattr accompanies cpu_ms");
            let mut sum = 0.0;
            for s in &seams {
                let sh = s.share.unwrap_or_else(|| {
                    panic!("seam `{}` has no share but cpu_ms is numeric: {line}", s.name)
                });
                assert!(
                    (0.0..=1.0).contains(&sh),
                    "seam `{}` share {sh} is outside [0, 1]: {line}",
                    s.name
                );
                sum += sh;
            }
            assert!(
                (sum - attr).abs() < 5e-4,
                "`attr` ({attr}) is not the sum of the printed shares ({sum}): {line}"
            );
            assert!(
                (attr + unattr - 1.0).abs() < 5e-4,
                "`unattr` must be 1 - `attr`: {line}"
            );
            assert!(
                attr <= 1.0 + 5e-4,
                "the seams attribute {attr} of process CPU — above 1.0 means they \
                 are NOT DISJOINT, which is the one structural assumption the \
                 decomposition rests on: {line}"
            );
            println!("[cpuprof-reach] attr={attr:.4} unattr={unattr:.4} cores={cores:?}");
        }
    }
}

/// The off side: the same transfer without the gate prints no `[CPUPROF]`,
/// asserted on the line and not only on the echo — the claim that the
/// instrument is free on every shipped arm rests on the gauge not existing.
#[test]
fn the_gauge_is_silent_on_the_shipped_default() {
    let log = run_transfer(&[("RUST_LOG", "raptorpath=info")]);
    assert!(
        log.contains("RWM_CPUPROF=0"),
        "the [GATES] echo must NAME the gate with its 0 value on the default arm:\n{log}"
    );
    assert!(
        !log.contains("[CPUPROF]"),
        "the CPU-decomposition gauge ships OFF and must print nothing:\n{log}"
    );
}
