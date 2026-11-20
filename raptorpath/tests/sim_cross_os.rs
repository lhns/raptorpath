//! Cross-OS determinism (ADR-0082, `docs/derivations/sim-engine.md` "What a
//! seed fixes"): this test writes the socket trace and filtered engine log
//! of a fixed scenario set to `SIM_XOS_OUT`; CI runs it on Linux and Windows
//! and diffs the two directories (`ci.yml` job `sim-cross-os`). Without
//! `SIM_XOS_OUT` it only checks that every scenario runs. Run with
//! `RUSTFLAGS="--cfg tokio_unstable" cargo test -p raptorpath --features sim,test-seams --test sim_cross_os`.
//!
//! The scenarios cover 1 and 2 paths, the Auto and Realtime hints, loss with
//! and without retransmission (rho = 1 and rho < 1) and the shed path, which
//! is where a reordered `select!` first showed (a 24-byte DATAGRAM frame
//! coalesced into a different packet).

mod sim_support;
use sim_support::{run, Scenario};

#[test]
fn child() {
    sim_support::child();
}

fn scenarios() -> Vec<(&'static str, Scenario)> {
    let realtime = |paths: u16, bytes: usize, loss: f64, port: u16| Scenario {
        bytes,
        hint: "realtime",
        reliable: Some(false),
        loss,
        limit_s: 40,
        ..Scenario::basic(42, paths, port)
    };
    let shed = vec![("RWM_TEST_TS_SHED_ALL", "1".to_string()), ("RWM_TEST_TS_BUDGET_FRAC", "0.05".to_string())];
    vec![
        ("auto_1path", Scenario::basic(5, 1, 47_500)),
        ("auto_2path", Scenario::basic(5, 2, 47_510)),
        ("realtime_2path_5pct", Scenario { env: shed.clone(), ..realtime(2, 6_000_000, 0.05, 47_520) }),
        ("realtime_1path_5pct_30mb", Scenario { env: shed.clone(), ..realtime(1, 30_000_000, 0.05, 47_530) }),
        // Two 1-path cells that go red without the one-body shutdown_signal
        // (the other 1-path cells happen not to: CI 44737688400).
        ("realtime_1path_5pct_s1", Scenario { seed: 1, env: shed.clone(), ..realtime(1, 6_000_000, 0.05, 47_550) }),
        ("realtime_1path_5pct_s2", Scenario { seed: 2, env: shed.clone(), ..realtime(1, 6_000_000, 0.05, 47_551) }),
        (
            "realtime_1path_blackout",
            Scenario {
                env: [
                    shed,
                    vec![
                        ("RWM_TEST_TS_SEQ", "12087".to_string()),
                        ("RWM_TEST_TS_DROP", "dead".to_string()),
                        ("RWM_TEST_TS_DEAD_MS", "1500".to_string()),
                    ],
                ]
                .concat(),
                ..realtime(1, 6_000_000, 0.025, 47_540)
            },
        ),
    ]
}

#[test]
fn write_traces() {
    let out = std::env::var_os("SIM_XOS_OUT").map(std::path::PathBuf::from);
    if let Some(d) = &out {
        std::fs::create_dir_all(d).unwrap();
    }
    for (name, sc) in scenarios() {
        let r = run(&sc);
        assert!(!r.trace.is_empty(), "{name}: empty trace");
        println!("{name}: outcome={} virt_s={:.3} events={}", r.outcome, r.virt_s, r.trace.lines().count());
        if let Some(d) = &out {
            std::fs::write(d.join(format!("{name}.trace")), &r.trace).unwrap();
            std::fs::write(d.join(format!("{name}.log")), &r.log).unwrap();
        }
    }
}
