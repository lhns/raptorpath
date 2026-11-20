//! The lost tail of `sim_lost_tail.rs` (source seq k swallowed by the
//! `RWM_TEST_TS_DROP=dead` seam for 1.5 s, Realtime, 2.5 % loss) with the
//! source PACED (`SIM_PACE_MBIT`, `sim_support::paced_client`) instead of
//! handed over at once, and the NATURAL budget (no `RWM_TEST_TS_BUDGET_FRAC`).
//! Question (roadmap node `rec-evict-blocker-unresolved`): does the rho < 1
//! stall need an over-driven source? Run with
//! `RUSTFLAGS="--cfg tokio_unstable" cargo test -p raptorpath --features sim,test-seams --test sim_lost_tail_paced -- --nocapture`.

mod sim_support;
use sim_support::{run, ChildRun, Scenario};

const K: u64 = 11_800;
const SEEDS: [u64; 3] = [42, 7, 2024];
/// Below the 40 Mbit link, and near it (the wire adds headers and repair).
const RATES: [f64; 2] = [20.0, 35.0];

static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn child() {
    sim_support::child();
}

fn paced_lost_tail(reliable: bool, mbit: f64, seed: u64) -> ChildRun {
    let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    run(&Scenario {
        seed,
        paths: 1,
        bytes: 6_000_000,
        hint: "realtime",
        reliable: Some(reliable),
        loss: 0.025,
        jitter_ms: 4,
        limit_s: 40,
        port: 47_300,
        env: vec![
            ("SIM_PACE_MBIT", mbit.to_string()),
            ("RWM_TEST_TS_SEQ", K.to_string()),
            ("RWM_TEST_TS_DROP", "dead".into()),
            ("RWM_TEST_TS_DEAD_MS", "1500".into()),
        ],
    })
}

/// The data sender's final cumulative ack: third field of `[DIAG] cum=src/cod/ack`,
/// from the line with the largest src (the peer's sender prints `[DIAG]` too).
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

struct Row {
    sends: usize,
    first_dropped: bool,
    fills: usize,
    src: u64,
    ack: u64,
    queue_drops: usize,
    o_strand: u64,
    inert_sweeps: u64,
    line: String,
}

fn row(r: &ChildRun) -> Row {
    let tag = format!(" seq={K} ");
    let ks = |ev: &str| r.tseam(ev).into_iter().filter(|l| l.contains(&tag)).collect::<Vec<_>>();
    let sends = ks("send");
    let (src, ack) = final_cum_ack(r);
    let a = r.acct();
    let queue_drops = r.trace.lines().filter(|l| l.ends_with(" drop_queue")).count();
    let line = format!(
        "outcome={} virt_s={:.2} k_sends={} k_fill={} k_pass={} cum_src={src} final_cum_ack={ack} (k={K}, passed={}) queue_drops={queue_drops} | ACCT O={} O_strand={} inert_sweeps={} oldest_unchanged_max={} H={} E={} bud={}",
        r.outcome,
        r.virt_s,
        sends.len(),
        ks("fill").len(),
        ks("pass").len() + ks("pass_force").len(),
        ack > K,
        a.u("O_serv") + a.u("O_strand"),
        a.u("O_strand"),
        a.u("inert_sweeps"),
        a.u("oldest_unchanged_max"),
        a.u("H"),
        a.u("E"),
        a.f("bud"),
    );
    Row { sends: sends.len(), first_dropped: sends.first().is_some_and(|l| l.contains(" dropped=1 ")), fills: ks("fill").len(), src, ack, queue_drops, o_strand: a.u("O_strand"), inert_sweeps: a.u("inert_sweeps"), line }
}

/// Observed (CI run 44757846199, 3 seeds each, natural budget): at rho < 1 the
/// lost tail stalls at BOTH 20 and 35 Mbit/s although the source is paced and
/// the link's queue never overflows (0 drop_queue; unpaced `perf::client`
/// overflows about 1 100): final cumulative ack stuck at k = 11 800 of 12 147,
/// O = 347, O_strand = 147, inert_sweeps 386 to 735, outcome virtual_limit.
/// k is resent 2 to 4 times at 20 Mbit/s (never at 35) and still never fills.
/// So the defect does NOT need an over-driven source; a rate bound alone is
/// not the fix. This test pins that, with the witness that the run was paced.
#[test]
fn paced_lost_tail_still_stalls_at_rho_below_1() {
    for mbit in RATES {
        for seed in SEEDS {
            let r = row(&paced_lost_tail(false, mbit, seed));
            let w = format!("pace={mbit} seed={seed}: {}", r.line);
            println!("rho<1 {w}");
            assert!(r.first_dropped, "seam did not swallow k's first send: {w}");
            assert_eq!(r.queue_drops, 0, "the source over-drove the link (not a paced run): {w}");
            assert!(r.ack <= K && r.fills == 0, "k resolved (defect fixed? flip this test): {w}");
            assert!(r.o_strand > 0 && r.inert_sweeps > 0, "account shows no stall (defect fixed? flip): {w}");
        }
    }
}

/// KNOWN DEFECT `rec-evict-blocker-unresolved`: once fixed, a paced lost tail
/// at rho < 1 resolves: the cumulative ack passes k and the account is clean.
#[test]
#[ignore = "rec-evict-blocker-unresolved: at rho < 1 the lost tail stalls even with a paced source (no queue overflow): final cum ack stuck at k, O_strand=147, inert sweeps"]
fn paced_lost_tail_resolves_at_rho_below_1() {
    for mbit in RATES {
        for seed in SEEDS {
            let r = row(&paced_lost_tail(false, mbit, seed));
            let w = format!("pace={mbit} seed={seed}: {}", r.line);
            assert!(r.ack > K, "final cumulative ack did not pass k: {w}");
            assert_eq!(r.o_strand, 0, "O_strand: {w}");
            assert_eq!(r.inert_sweeps, 0, "inert sweeps: {w}");
        }
    }
}

/// Control: the same paced lost tail at rho = 1 recovers: k resent and filled,
/// the cumulative ack passes k, the transfer completes, the account is clean.
#[test]
fn control_paced_lost_tail_recovers_at_rho_1() {
    for mbit in RATES {
        for seed in SEEDS {
            let r = paced_lost_tail(true, mbit, seed);
            let w = row(&r);
            println!("rho=1 pace={mbit} seed={seed}: {}", w.line);
            assert!(w.first_dropped, "seam did not swallow k's first send: {}", w.line);
            assert!(w.sends >= 2 && w.fills >= 1, "k not resent and filled: {}", w.line);
            assert!(w.ack > K, "final cumulative ack did not pass k: {}", w.line);
            assert_eq!(r.outcome, "client_ok", "{}", w.line);
            assert!(r.acct().violations().is_empty(), "account: {:?}", r.acct().violations());
        }
    }
}
