//! Mixed retention config, deterministically (ADR-0082): a Realtime client
//! (the evicting seat, ρ < 1) sends to a Bulk server (ρ = 1) over one path
//! with 2.5 % client-to-server loss, seed 42, 6 MB. Each end resolves its own
//! retention from its own config (`net/mod.rs` `recv_window_reliable =
//! window_reliable`), so the Bulk server's receiver runs the never-pass
//! reorder buffer on a stream whose sender evicts and sheds. Run with
//! `RUSTFLAGS="--cfg tokio_unstable" cargo test -p raptorpath --features sim,test-seams --test sim_mixed_rho`
//! (`-- --ignored` for the known-defect test).
//!
//! Liveness is read on the sender's final cumulative ack (`[DIAG] cum=`),
//! not on completion: at ρ < 1 the perf server needs every chunk, which the
//! contract does not promise.

mod sim_support;
use sim_support::{run, ChildRun, Scenario};

#[test]
fn child() {
    sim_support::child();
}

static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Client (data sender) at Realtime's preset, server (data receiver) at `srv_hint`'s preset.
fn pair(srv_hint: &str, port: u16) -> ChildRun {
    let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    run(&Scenario {
        seed: 42,
        paths: 1,
        bytes: 6_000_000,
        hint: "realtime",
        reliable: None,
        loss: 0.025,
        jitter_ms: 4,
        limit_s: 40,
        port,
        env: vec![("SIM_SRV_HINT", srv_hint.to_string())],
    })
}

/// The data sender's final `(src, cumulative ack)` from `[DIAG] cum=src/cod/ack`,
/// on the line with the largest src (as `sim_shed_reach::final_cum_ack`).
fn final_cum_ack(r: &ChildRun) -> (u64, u64) {
    let cum = |l: &str| -> Option<(u64, u64)> {
        let c = l.split(' ').find_map(|w| w.strip_prefix("cum="))?;
        let v: Vec<u64> = c.split('/').filter_map(|x| x.parse().ok()).collect();
        (v.len() == 3).then(|| (v[0], v[2]))
    };
    r.log
        .lines()
        .filter(|l| l.starts_with("[DIAG]"))
        .filter_map(cum)
        .fold(None, |best: Option<(u64, u64)>, c| match best {
            Some(b) if b.0 > c.0 => Some(b),
            _ => Some(c),
        })
        .expect("no [DIAG] cum= line")
}

/// KNOWN DEFECT (proposed roadmap node `rec-mixed-rho-stall`): a Realtime
/// sender against a Bulk receiver. The sender evicts and sheds; the
/// receiver's ρ = 1 buffer never passes a hole, so a hole the sender no
/// longer serves stalls the frontier for good.
#[test]
#[ignore = "rec-mixed-rho-stall: a Realtime (rho < 1) sender against a Bulk (rho = 1) receiver stalls the frontier, because each end reads its own retention flag"]
fn mixed_realtime_sender_bulk_receiver_ack_reaches_src() {
    let r = pair("bulk", 47_600);
    let (src, ack) = final_cum_ack(&r);
    println!("mixed (realtime -> bulk): outcome={} virt_s={:.2} src={src} ack={ack}", r.outcome, r.virt_s);
    assert!(src > 0, "the data sender sent nothing");
    assert_eq!(ack, src, "the receiver frontier stopped short (outcome={})", r.outcome);
}

/// Control: the same sender against a matched Realtime receiver. Its
/// frontier reaches the sender's src (its buffer gives up on holes), so the
/// stall above is the mismatch, not the link.
#[test]
fn control_matched_realtime_ack_reaches_src() {
    let r = pair("realtime", 47_610);
    let (src, ack) = final_cum_ack(&r);
    println!("matched (realtime -> realtime): outcome={} virt_s={:.2} src={src} ack={ack}", r.outcome, r.virt_s);
    assert!(src > 0, "the data sender sent nothing");
    assert_eq!(ack, src, "the matched receiver's frontier stopped short (outcome={})", r.outcome);
}
