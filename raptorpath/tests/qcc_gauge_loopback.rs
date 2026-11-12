//! The `[QCC]` substrate-controller gauge (status §48, amendment 2) reads the
//! controller that is running (`docs/measurement-discipline.md` rule 1).
//!
//! Under `RWM_QUIC_CC=bbr_rs` the sender's `[QCC]` line carries, per path,
//! `qm=` in {S, D, B, R} and numeric `qrt_us=`, `qsr_us=`, `qbw=`, `qmin_us=`
//! the read-cost `qcns=<last>/<reads>/<mean>` and the ProbeRtt counters `qrn=`, `qrus=`, `qrb=`, `qab=`, `qage_us=`; under the default
//! substrate (quinn's BBR) there is no `[QCC]` line at all, while `[DIAG]` is
//! still printing, so an absent line cannot be an unset gate. The `[DIAG]`
//! per-path format is pinned and untouched. Own test binary: the spawned
//! pair gets its env explicitly.

#[path = "common/loopback.rs"]
mod loopback;

use std::collections::HashMap;

/// Client (sender) log of one shaped loopback transfer under `cc`.
fn client_log(cc: Option<&str>) -> String {
    let binds = loopback::free_addrs(1);
    let mut env: Vec<(&str, &str)> = vec![("RWM_DIAG", "1"), ("RUST_LOG", "raptorpath=info")];
    if let Some(cc) = cc {
        env.push(("RWM_QUIC_CC", cc));
    }
    let srv = loopback::spawn_perf_server(&binds, &env, &["--protocol-hint", "bulk", "--window-reliable"]);
    let mut cli_env = env.clone();
    cli_env.extend(loopback::CLEAN_RATE);
    // 40 Mbit: ~2 s of 250 ms DIAG ticks (the first tick requests the read,
    // the owner's next snapshot takes it)
    let log = loopback::run_perf_client(&binds, &cli_env, &loopback::perf_args("bulk", "10000000", "1"));
    drop(srv);
    log
}

/// `key=value` fields of one `p<id>:qm=..` block of a `[QCC]` line.
fn blocks(line: &str) -> Vec<HashMap<String, String>> {
    let mut out: Vec<HashMap<String, String>> = Vec::new();
    for tok in line.split_whitespace().skip(2) {
        let tok = tok.strip_prefix('p').and_then(|t| t.split_once(':')).map_or(tok, |(_, kv)| {
            out.push(HashMap::new());
            kv
        });
        if let (Some((k, v)), Some(b)) = (tok.split_once('='), out.last_mut()) {
            b.insert(k.to_string(), v.to_string());
        }
    }
    out
}

#[test]
fn qcc_line_reads_bbr_rs_and_is_absent_under_the_default_substrate() {
    // ── bbr_rs: the line is there and reads the running controller ──
    let log = client_log(Some("bbr_rs"));
    assert!(log.contains("burst-robust BBR"), "the bbr_rs liveness echo is missing:\n{log}");
    let lines: Vec<&str> = log.lines().filter(|l| l.starts_with("[QCC] t=")).collect();
    assert!(!lines.is_empty(), "no [QCC] line under RWM_QUIC_CC=bbr_rs ({} [DIAG] lines)", log.matches("[DIAG] t=").count());
    let mut modes = String::new();
    let mut loaded = false;
    for l in &lines {
        let bs = blocks(l);
        assert!(!bs.is_empty(), "no path block in {l}");
        for b in bs {
            let qm = b.get("qm").unwrap_or_else(|| panic!("qm missing in {l}"));
            assert!(matches!(qm.as_str(), "S" | "D" | "B" | "R"), "qm={qm} in {l}");
            for k in ["qrt_us", "qsr_us", "qbw", "qmin_us", "qrn", "qrus", "qrb", "qab", "qage_us"] {
                b.get(k)
                    .unwrap_or_else(|| panic!("{k} missing in {l}"))
                    .parse::<u64>()
                    .unwrap_or_else(|_| panic!("{k} not numeric in {l}"));
            }
            let c: Vec<u64> = b["qcns"].split('/').map(|x| x.parse().expect("qcns numeric")).collect();
            assert!(c.len() == 3 && c[1] >= 1, "qcns=last/reads/mean with >=1 read: {l}");
            if !modes.contains(qm.as_str()) {
                modes.push_str(qm);
            }
            loaded |= b["qbw"].parse::<u64>().unwrap() > 0 && b["qrt_us"].parse::<u64>().unwrap() > 0;
        }
    }
    assert!(loaded, "no [QCC] line ever carried a bandwidth and a min-RTT model:\n{}", lines.join("\n"));
    println!("[qcc-loopback] {} [QCC] lines; modes seen: {modes}; last: {}", lines.len(), lines.last().unwrap());

    // ── default substrate: no [QCC], [DIAG] still printing ──
    let log = client_log(None);
    assert!(log.contains("[DIAG] t="), "the control run printed no [DIAG]; the absence below would mean nothing");
    assert!(
        !log.lines().any(|l| l.starts_with("[QCC]")),
        "a [QCC] line under the default substrate:\n{log}"
    );
}
