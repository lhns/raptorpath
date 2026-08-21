//! THE SUCCESSOR-ARRIVAL GAUGE FIRES — `[SUCC]` — AND ITS THREE OUTCOMES ARE
//! A PARTITION OF THE HOLES THE ENGINE ACTUALLY OPENS.
//!
//! **The measurand, and why it is owed.** The fire-cause pass (goal-gate, "THE
//! FIRE-CAUSE PASS — THE SCORED RESULT") measured **0.59 % of 107 597 recovery
//! fires timer-driven and 98.99 % `gap_data`** — the receiver's SACK report,
//! emitted when a higher seq arrives while a hole is outstanding. It named the
//! successor measurand from that count and then named, in its own closing
//! paragraph, the reading it had NOT taken:
//!
//! > *"the successor-arrival distribution has never been measured on this
//! > engine … A derivation written against an uncharacterized distribution
//! > would repeat the exact defect just corrected."*
//!
//! `[SUCC]` is that reading's instrument. **This binary is the gate that must
//! pass before the measurement pass is worth making**, and it is the assertion
//! that FAILS on the shipped-before engine: no `[SUCC]` line exists there, no
//! hole was ever timed, and the quantity the next derivation is supposed to be
//! positioned on had no producer at all.
//!
//! **What is asserted, in the order it can fail.**
//!
//!   1. `succ_report_line`'s FORMAT and the `-`-iff-none convention, so an L1
//!      parser has a pin and a measured zero is never confusable with an
//!      absent reading. Pure, fails locally.
//!   2. The line FIRES from the receiver over a lossy plain-window transfer,
//!      with `det > 0`. THE DEAD-GAUGE READING this test exists to fail on.
//!   3. **THE ACCOUNTING IDENTITY, ON THE WIRE**: `det = orig_n + rep_n +
//!      aban_n + open + over`, checked on the engine's own output and not only
//!      in the unit test's synthetic feed. A gauge whose classes do not
//!      partition its own denominator is caught here rather than in a results
//!      table.
//!   4. **THE HOLES ARE REAL, against an INDEPENDENT witness.** `[SUCC] det`
//!      must be > 0 exactly where `[RFA] fires` is, and the two are bumped by
//!      different code from different events (`[RFA]` classifies ARRIVALS,
//!      `[SUCC]` times HOLES). A `det` that moves while `[RFA]` reads zero
//!      would mean this gauge is inventing its own denominator.
//!   5. **THE OUTCOMES ARE POPULATED AND IN RANGE.** At least one hole closes,
//!      every quantile is ordered `p50 ≤ p90 ≤ p99 ≤ mx`, and `orig_frac` is a
//!      fraction. A histogram that reports an unordered quantile triple is
//!      reporting a bucketing bug.
//!   6. **THE CONFIGURATION CONTRACT**, the `[RFA]` convention: the line
//!      echoes `gen=` so no row is ever read out of its scope, and it reads
//!      `gen=0` on the plain window this pass measures.
//!   7. **THE RAW DUMP IS OFF BY DEFAULT AND ON WHEN ASKED** — both sides,
//!      measured. A default-ON dump would be a receiver-side cost on every
//!      scored arm, which is the `[RTTDUMP]` defect this design copied the fix
//!      for.
//!
//! **What this deliberately does NOT assert.** Any particular VALUE of any
//! quantile, of `orig_frac`, or of the crossing point. Loopback's redundancy is
//! the shim's Gilbert-Elliott process and the host scheduler's, not a network's;
//! the numbers that characterize the measurand come off an L1 run scored against
//! a pre-registration. This is the INSTRUMENT gate, not the measurement.
//!
//! **No new gate on the readout.** The periodic `[SUCC]` line rides the
//! EXISTING `RWM_DIAG` / `RWM_FDIAG` gates, exactly as `[RFA]` and `[QCLK]` do,
//! so a missing line can only be read as an unreached emission site.
//! `RWM_DIAG=1` IS asserted present in the `[GATES]` echo below.
//!
//! Own test binary, for `rfa_reachability.rs`'s reason: `RWM_L0_NETEM` is
//! process-global in the child and the spawned pair must not contend with the
//! in-process loopback tests.

use std::io::Read;
use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use raptorpath::net::succ::{succ_report_line, Hist};

// ── 1: THE PURE PIN ─────────────────────────────────────────────────────

#[test]
fn the_succ_line_format_and_the_dash_iff_none_convention_are_pinned() {
    let mut orig = Hist::default();
    orig.add(1_000);
    orig.add(2_000);
    let mut rep = Hist::default();
    rep.add(40_000);
    let empty = Hist::default();

    let l = succ_report_line(false, 7, &orig, &rep, &empty, 3, 1, Some(2048), false, 0);
    assert!(l.starts_with("[SUCC] gen=0 det=7 res=3 "), "{l}");
    // n BESIDE every value — no quantile is ever readable without its own
    // sample count.
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
    // `-` IFF NONE, on every slot of the empty outcome — and NOT a 0, which a
    // parser would read as a measured zero microseconds.
    for k in ["aban_p50_us", "aban_p90_us", "aban_p99_us", "aban_mx_us", "aban_mean_us"] {
        assert!(l.contains(&format!("{k}=-")), "`{k}` must render `-` when n=0: {l}");
    }
    // A quantile is its bucket's LOWER edge, so it never exceeds the exact
    // maximum printed beside it.
    assert!(l.contains("orig_mx_us=2000"), "{l}");
    assert!(l.contains("rep_mx_us=40000"), "{l}");

    // THE OTHER SIDE of every convention on one line: nothing measured at all.
    let e = succ_report_line(true, 0, &empty, &empty, &empty, 0, 0, None, true, 12);
    assert!(e.starts_with("[SUCC] gen=1 det=0 res=0 "), "{e}");
    assert!(e.contains("orig_frac=- cross_us=- dump=1/12"), "{e}");
    assert!(!e.contains("orig_frac=0"), "an absent fraction is `-`, never 0: {e}");
}

// ── THE REACHABILITY RUN ────────────────────────────────────────────────

/// The arm. `RWM_DIAG` carries the periodic `[SUCC]` readout (the L1 harnesses
/// SIGKILL the server, so a `Drop`-only emission is unreachable there and this
/// test must not depend on one either). No gate here changes a law.
const ARM: [(&str, &str); 3] = [
    ("RWM_DIAG", "1"),
    ("RWM_PLAIN_RS", "1"),
    ("RUST_LOG", "raptorpath=info"),
];

fn free_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("probe bind");
    l.local_addr().expect("probe addr").port()
}

struct Reaper(Child);
impl Drop for Reaper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Spawn the perf SERVER — the receiver of the bulk direction, and the site
/// whose `[SUCC]` this test is about. Its stdout AND stderr land in one sink so
/// a missing `[SUCC]` can never be confused with an unset gate.
fn spawn_perf_server(dump: bool) -> (SocketAddr, Reaper, Arc<Mutex<String>>) {
    let bin = env!("CARGO_BIN_EXE_raptorpath");
    let addr: SocketAddr = format!("127.0.0.1:{}", free_port()).parse().unwrap();
    let mut cmd = Command::new(bin);
    cmd.args([
        "perf",
        "--server",
        "--bind",
        &addr.to_string(),
        "--protocol-hint",
        "bulk",
        "--window-reliable",
    ]);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in ARM {
        cmd.env(k, v);
    }
    // THE DUMP'S BOTH SIDES. Absent by default (7); set only on the dump arm,
    // and unset explicitly so an inherited value cannot arm the control.
    cmd.env_remove("RWM_SUCC_DUMP");
    cmd.env_remove("RWM_SUCC_DUMP_MAX");
    if dump {
        cmd.env("RWM_SUCC_DUMP", "1");
        cmd.env("RWM_SUCC_DUMP_MAX", "5000");
    }
    // The shim shapes the CLIENT's egress; the server's own datagram path (the
    // ack direction) is left clean so acks are not the thing under test.
    cmd.env_remove("RWM_L0_NETEM");
    let mut srv = Reaper(cmd.spawn().expect("spawn perf server"));

    let log = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&log);
    let mut err = srv.0.stderr.take().expect("server stderr");
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match err.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => sink
                    .lock()
                    .expect("stderr sink")
                    .push_str(&String::from_utf8_lossy(&buf[..n])),
            }
        }
    });

    let mut out = srv.0.stdout.take().expect("server stdout");
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut seen = String::new();
    let mut buf = [0u8; 256];
    while Instant::now() < deadline && !seen.contains("perf server ready") {
        match out.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => seen.push_str(&String::from_utf8_lossy(&buf[..n])),
            Err(e) => panic!("reading perf server stdout: {e}"),
        }
    }
    assert!(
        seen.contains("perf server ready"),
        "perf server never became ready; it said: {seen}"
    );
    {
        let sink = Arc::clone(&log);
        sink.lock().expect("stderr sink").push_str(&seen);
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match out.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => sink
                        .lock()
                        .expect("stderr sink")
                        .push_str(&String::from_utf8_lossy(&buf[..n])),
                }
            }
        });
    }
    (addr, srv, log)
}

/// One lossy PLAIN-WINDOW loopback. Returns `(client log, server log)`.
fn lossy_run(dump: bool) -> (String, String) {
    let bin = env!("CARGO_BIN_EXE_raptorpath");
    let (addr, _srv, srv_log) = spawn_perf_server(dump);

    let mut cli = Command::new(bin);
    cli.args([
        "perf",
        "--client",
        "--peer",
        &addr.to_string(),
        "--bytes",
        "4000000",
        "--runs",
        "2",
        "--protocol-hint",
        "bulk",
        "--window-reliable",
    ]);
    cli.stdout(Stdio::piped()).stderr(Stdio::piped());
    for (k, v) in ARM {
        cli.env(k, v);
    }
    // The L1 `c3` cell (LTE-class: 20 Mbit, 20 ms one-way, 5 ms jitter,
    // GE p = 2 % / q = 40 % ⇒ ε ≈ 4.8 %) on client egress, seeded. LOSS IS WHAT
    // MAKES HOLES EXIST AT ALL — with no loss this gauge has nothing to time
    // and `det = 0` would be a configuration fact rather than a dead gauge.
    cli.env("RWM_L0_NETEM", "c3");
    cli.env("RWM_L0_SEED", "42");

    let out = cli.output().expect("run perf client");
    let cli_stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let cli_stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "perf client failed (dump={dump}, {:?})\n--- stdout ---\n{cli_stdout}\n\
         --- stderr ---\n{cli_stderr}",
        out.status
    );
    // Let the receiver's last periodic readout land.
    std::thread::sleep(Duration::from_millis(1500));
    let srv = srv_log.lock().expect("stderr sink").clone();
    (format!("{cli_stdout}\n{cli_stderr}"), srv)
}

fn field<'a>(line: &'a str, key: &str) -> &'a str {
    line.split_whitespace()
        .find_map(|t| t.strip_prefix(key))
        .unwrap_or_else(|| panic!("`{key}` missing from gauge line: {line}"))
}

fn u64_field(line: &str, key: &str) -> u64 {
    let v = field(line, key);
    v.parse()
        .unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e} in {line}"))
}

/// A `-`-or-number slot. `None` is the ABSENT reading and is never 0.
fn opt_field(line: &str, key: &str) -> Option<u64> {
    let v = field(line, key);
    if v == "-" {
        return None;
    }
    Some(
        v.parse()
            .unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e} in {line}")),
    )
}

/// 2-6: THE GAUGE FIRES, PARTITIONS ITS OWN DENOMINATOR, AND AGREES WITH AN
/// INDEPENDENT WITNESS.
#[test]
fn the_receiver_reports_the_successor_arrival_distribution() {
    let (_cli, log) = lossy_run(false);

    // THE GATE. A missing `[SUCC]` must be readable as an unreached emission
    // site and never as an unset gate.
    assert!(
        log.contains("RWM_DIAG=1"),
        "the server's [GATES] echo does not carry RWM_DIAG=1 — the arm did not \
         arm:\n{log}"
    );
    assert!(
        !log.contains("RWM_DIAG=0"),
        "the server's [GATES] echo carries BOTH sides of RWM_DIAG:\n{log}"
    );
    // 7a: THE DUMP IS OFF BY DEFAULT, and the echo says so on its own line.
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

    // 2. THE LINE FIRES, AND IT FIRES NONZERO. This is what fails on the
    //    shipped-before engine: no [SUCC] at all, and no hole was ever timed.
    let succ: Vec<&str> = log.lines().filter(|l| l.contains("[SUCC] ")).collect();
    assert!(
        !succ.is_empty(),
        "no [SUCC] line from the RECEIVER over a lossy transfer — the \
         successor-arrival gauge is unreachable, and the measurand the \
         fire-cause pass named still has no producer:\n{log}"
    );
    // Cumulative counters: the LAST line is the reading.
    let last = *succ.last().expect("non-empty");
    println!("[succ-reach] {} lines; last: {last}", succ.len());

    // 6. THE CONFIGURATION CONTRACT.
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

    // 3. THE ACCOUNTING IDENTITY, ON THE ENGINE'S OWN OUTPUT.
    assert_eq!(
        det,
        orig_n + rep_n + aban_n + open + over,
        "[SUCC] det is not the sum of its outcomes, its census and its \
         declared overflow — the three outcomes do not partition the holes: \
         {last}"
    );
    assert_eq!(res, orig_n + rep_n, "[SUCC] res must be orig_n + rep_n: {last}");

    // 4. THE INDEPENDENT WITNESS. `[RFA]` classifies ARRIVALS; `[SUCC]` times
    //    HOLES. Different code, different events, same underlying loss — so a
    //    `det` that moves while `[RFA]` reads zero means this gauge is
    //    inventing its own denominator.
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

    // 5. THE OUTCOMES ARE POPULATED AND IN RANGE.
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
    // The crossing point is a LEGAL `-`: it reads "the original leads at every
    // horizon". Asserted as in-range when present, never asserted to exist.
    if let Some(c) = opt_field(last, "cross_us=") {
        println!("[succ-reach] crossing point {c} us");
        assert!(c <= 3_600_000_000, "[SUCC] cross_us={c} is not a plausible age");
    }
    // The dump is OFF on this arm, so it must have written nothing.
    assert!(last.contains("dump=0/0"), "an unarmed dump must be 0/0: {last}");
    assert!(
        !log.contains("[SUCCDUMP]"),
        "a [SUCCDUMP] line on an arm with RWM_SUCC_DUMP unset — the raw dump \
         is not default-OFF, and every scored arm would pay for it:\n{last}"
    );
}

fn opt_field_f64(line: &str, key: &str) -> Option<f64> {
    let v = field(line, key);
    if v == "-" {
        return None;
    }
    Some(v.parse().unwrap_or_else(|e| panic!("`{key}` `{v}` does not parse: {e}")))
}

/// 7b: THE DUMP'S OTHER SIDE. Armed, it emits raw records in the pinned batch
/// format, and its cap binds LOUDLY rather than silently truncating the stream
/// the next derivation will read.
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
    let d = field(first, "d=");
    let recs: Vec<&str> = d.split(';').collect();
    assert_eq!(recs.len(), n as usize, "[SUCCDUMP] n= disagrees with its records: {first}");
    for r in &recs {
        let (tag, us) = r
            .split_once(',')
            .unwrap_or_else(|| panic!("[SUCCDUMP] record `{r}` is not `<tag>,<us>`: {first}"));
        assert!(
            matches!(tag, "o" | "r" | "a"),
            "[SUCCDUMP] record tag `{tag}` is not one of the three outcomes: {first}"
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
