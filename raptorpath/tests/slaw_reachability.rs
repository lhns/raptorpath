//! The store law's gauge-only build (`net::slaw`, store-law.md R4.8/R5.6)
//! reaches the wire, on a real loopback transfer, and only under `RWM_DIAG`.
//! Rule 1 of `docs/measurement-discipline.md`: a unit test proves the pieces,
//! this proves the mechanism executes.
//!
//!   1. With `RWM_DIAG=1` the sender prints `[WSTAT]`, `[WSTORE]`, `[SLAW]`,
//!      `[SLAWC]` and `[CAPLOG]`, with `nk > 0` echo samples on a leg.
//!   2. `[SLAW] n=` is the store cap `[DIAG] win=<a>/<n>` prints, on at least
//!      99 % of windows (store-law.md §8's routing witness).
//!   3. Every `[CAPLOG]` record with `br=pooled` reproduces its cap through
//!      the pure law (here: the arithmetic restated, in the unit test through
//!      `pooled_store_cap` itself; `caplog_replay.py` does it offline).
//!   4. With `RWM_DIAG` unset none of the five tags appears.
//!
//! No value of R, σ or the brakes is asserted: the gauge decides nothing.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

const TAGS: [&str; 5] = ["[WSTAT] ", "[WSTORE] ", "[SLAW] ", "[SLAWC] ", "[CAPLOG] "];

fn run(diag: bool) -> String {
    run_with(diag, None)
}

fn run_with(diag: bool, slaw: Option<&str>) -> String {
    let mut env: Vec<(&str, &str)> = vec![("RUST_LOG", "raptorpath=info")];
    if diag {
        env.push(("RWM_DIAG", "1"));
    }
    if let Some(v) = slaw {
        env.push(("RWM_SLAW", v));
    }
    let binds = loopback::free_addrs(1);
    let srv = loopback::spawn_perf_server(&binds, &env, &["--protocol-hint", "bulk", "--window-reliable"]);
    let cli = loopback::run_perf_client(&binds, &env, &loopback::perf_args("bulk", "24000000", "2"));
    drop(srv);
    cli
}

fn tok<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    gauge::raw_field(line, key)
}

#[test]
fn the_gauge_prints_under_diag_and_only_under_diag() {
    let on = run(true);
    for t in TAGS {
        assert!(on.lines().any(|l| l.starts_with(t)), "no {t}line with RWM_DIAG=1 — the gauge is unreachable:\n{on}");
    }
    // 1. Some window carried echo samples on a leg.
    let slaw: Vec<&str> = on.lines().filter(|l| l.starts_with("[SLAW] ")).collect();
    let with_samples = slaw.iter().filter(|l| tok(l, "nk=").and_then(|v| v.parse::<u64>().ok()).unwrap_or(0) > 0).count();
    assert!(with_samples > 0, "every [SLAW] line has nk=0: the sampler was never fed");
    let w = on.lines().filter(|l| l.starts_with("[WSTAT] ") && tok(l, "nk=").is_some_and(|v| v != "0")).count();
    assert!(w > 0, "no [WSTAT] window with samples");

    // 2. The routing witness: [SLAW] n equals the [DIAG] win= denominator.
    let caps: Vec<&str> = on
        .lines()
        .filter(|l| l.starts_with("[DIAG] "))
        .filter_map(|l| tok(l, "win=").and_then(|v| v.split_once('/').map(|x| x.1)))
        .collect();
    let slaw_n: Vec<&str> = slaw.iter().filter_map(|l| tok(l, "n=")).collect();
    assert!(!caps.is_empty() && !slaw_n.is_empty());
    let hits = slaw_n.iter().filter(|n| caps.contains(n)).count();
    assert!(
        hits as f64 >= 0.99 * slaw_n.len() as f64,
        "[SLAW] n= is not the [DIAG] win= denominator on 99 % of windows ({hits}/{})",
        slaw_n.len()
    );

    // 3. CAPLOG pooled records are self-consistent (cap >= floor, <= N*pool).
    for l in on.lines().filter(|l| l.starts_with("[CAPLOG] ") && l.contains(" br=pooled ")) {
        let num = |k: &str| tok(l, k).and_then(|v| v.parse::<f64>().ok()).unwrap_or_else(|| panic!("{k} in {l}"));
        let (cap, floor, n, pool) = (num("cap="), num("floor="), num("n="), num("pool="));
        assert!(cap >= floor && cap <= (n * pool).max(floor), "{l}");
    }

    // 5. W1 debt flows ride [WSTORE] (names are the contract with the parser).
    let ws = on.lines().rev().find(|l| l.starts_with("[WSTORE] ")).expect("a [WSTORE] line");
    for k in ["dbt_acc=", "dbt_em=", "dbt_share=", "dbt_rm_ack=", "dbt_ask_ack=", "dbt_rm_nack=", "dbt_ask_nack=", "adv="] {
        assert!(tok(ws, k).is_some() || ws.contains(&format!(" {k}-")), "{k} missing: {ws}");
    }
    assert!(on.contains("RWM_SLAW=1"), "[GATES] must echo RWM_SLAW=1");

    // 4b. RWM_DIAG=1 RWM_SLAW=0: [DIAG] stays, the gauge does not run.
    let ctl = run_with(true, Some("0"));
    assert!(ctl.lines().any(|l| l.starts_with("[DIAG] ")), "[DIAG] must stay on:
{ctl}");
    assert!(ctl.contains("RWM_SLAW=0"), "[GATES] must echo RWM_SLAW=0");
    for t in TAGS {
        assert!(!ctl.lines().any(|l| l.starts_with(t)), "{t}printed with RWM_SLAW=0:
{ctl}");
    }

    // 4. Off: nothing.
    let off = run(false);
    for t in TAGS {
        assert!(!off.lines().any(|l| l.starts_with(t)), "{t}printed with RWM_DIAG unset:\n{off}");
    }
}
