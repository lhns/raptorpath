//! S2c: config round trip, per entry point (test detection plan). `#[cfg(test)]`
//! only; no engine bytes.
//!
//! For every `RaptorpathConfig` key, a TOML file setting only that key to a
//! non-default value is resolved through each entry point's resolution fn
//! (`config::resolve_tunnel`, `config::resolve_perf` with the client and server
//! baselines). The cell in [`KEYS`] says what must happen. The table is owned
//! here: a key an entry point does not read is written down with its reason,
//! and a key it should read but does not is `Owed`, not hidden.
//!
//! What the cells assert, and the one-call-later risk behind them:
//! * `Moves` / `MovesToController`: the resolved `PeerConfig` differs from the
//!   baseline. `MovesToController` also builds the controller the way
//!   `net::run` does and requires its output to move, because a value can
//!   reach `PeerConfig` and die one call later (`enable_pi_feedback`, whose
//!   consumer ignores it: `DiesAtController`).
//! * `ForcedOff` (the tunnel's four object modes): setting the key must not
//!   move the resolved value. An `Err` is accepted only if its message names
//!   the key, so an unrelated failure cannot pass for the guard.
//! * `Rejected`: resolution must fail, naming the key.
//! * `Owed`: expected to move (or be rejected) and does not, today. The
//!   default test pins that it still does not, so fixing it forces a table
//!   edit; `owed_cells_are_red_when_demanded` (ignored) fails on every one.
//!
//! The perf entry point's `--config` drop cannot be seen by feeding it a file
//! (its resolution fn takes none), so the claim is carried by
//! `main_rs_wiring_pins`: `cmd_perf` is called without the config path.

use crate::config::{self, RaptorpathConfig};
use crate::control::estimator::LossEstimator;
use crate::control::fec_rate::FecRateController;
use crate::fec::FecBackend;
use crate::net::PeerConfig;

#[derive(Clone, Copy, Debug)]
enum Cell {
    /// Moves `PeerConfig`; the field is read by `net::run` (pinned in source).
    Moves(&'static str),
    /// Moves `PeerConfig` and the controller built from it.
    MovesToController(&'static str),
    /// OWED: reaches `PeerConfig`, dies at the controller.
    DiesAtController(&'static str),
    /// Moves nothing, by design (reason).
    NotRead(&'static str),
    /// The entry point forces the value: file input must not move it.
    ForcedOff,
    /// Resolution must fail naming the key.
    Rejected,
    /// OWED: should move (or be rejected) and does not (reason).
    Owed(&'static str),
}
use Cell::*;

struct Key {
    name: &'static str,
    /// The TOML value (right of `=`), non-default.
    toml: &'static str,
    tunnel: Cell,
    perf: Cell,
}

const PERF_DROPS_CONFIG: &str = "perf drops --config";
const PERF_INJECTED_TUN: &str = "perf runs on an injected memory TUN: no device, route or DNS setup";
const DEPRECATED: &str = "deprecated: parsed, warned, ignored (paper 5.10)";
const NO_CONSUMER: &str = "no PeerConfig field and no consumer: resolve drops a documented key";

const KEYS: &[Key] = &[
    Key { name: "server", toml: "true", tunnel: Moves("is_server"), perf: Owed(PERF_DROPS_CONFIG) },
    Key { name: "bind", toml: "[\"127.0.0.1:19001\"]", tunnel: Moves("bind_addrs"), perf: Owed(PERF_DROPS_CONFIG) },
    Key { name: "peer", toml: "[\"127.0.0.1:19002\"]", tunnel: Moves("peer_addrs"), perf: Owed(PERF_DROPS_CONFIG) },
    Key { name: "tun_name", toml: "\"tuntest0\"", tunnel: Moves("tun_name"), perf: NotRead(PERF_INJECTED_TUN) },
    Key { name: "tun_addr", toml: "\"10.77.0.1/24\"", tunnel: Moves("tun_addr"), perf: NotRead(PERF_INJECTED_TUN) },
    Key { name: "target_tail_loss", toml: "0.001", tunnel: MovesToController("target_tail_loss"), perf: Owed("the tail target: no --target-tail-loss flag and --config dropped") },
    Key { name: "max_fec_overhead", toml: "0.2", tunnel: MovesToController("max_fec_overhead"), perf: Owed("no --max-fec-overhead flag and --config dropped") },
    Key { name: "protocol_hint", toml: "\"realtime\"", tunnel: MovesToController("protocol_hint"), perf: Owed(PERF_DROPS_CONFIG) },
    Key { name: "status_addr", toml: "\"127.0.0.1:9821\"", tunnel: Moves("status_addr"), perf: Owed("no flag; triage whether perf serves the monitor") },
    Key { name: "route", toml: "[\"192.0.2.0/24\"]", tunnel: Moves("routes"), perf: NotRead(PERF_INJECTED_TUN) },
    Key { name: "dns", toml: "\"192.0.2.53\"", tunnel: Moves("dns"), perf: NotRead(PERF_INJECTED_TUN) },
    Key { name: "interleave_depth", toml: "4", tunnel: Rejected, perf: Owed("perf drops --config: once read, resolve rejects it") },
    Key { name: "pin_cert", toml: "\"pinned.pem\"", tunnel: Moves("pin_cert"), perf: Owed(PERF_DROPS_CONFIG) },
    Key { name: "fec_backend", toml: "\"rlc\"", tunnel: NotRead("rlc is the only backend; every other value is an error"), perf: NotRead("rlc is the only backend; every other value is an error") },
    Key { name: "fec_switch_threshold_low", toml: "0.01", tunnel: NotRead(DEPRECATED), perf: NotRead(DEPRECATED) },
    Key { name: "fec_switch_threshold_high", toml: "0.2", tunnel: NotRead(DEPRECATED), perf: NotRead(DEPRECATED) },
    Key { name: "fec_switch_interval", toml: "5", tunnel: NotRead(DEPRECATED), perf: NotRead(DEPRECATED) },
    Key { name: "fec_auto_switch", toml: "true", tunnel: NotRead(DEPRECATED), perf: NotRead(DEPRECATED) },
    Key { name: "window_reliable", toml: "false", tunnel: Moves("window_reliable"), perf: Owed(PERF_DROPS_CONFIG) },
    Key {
        name: "enable_pi_feedback",
        toml: "false",
        tunnel: DiesAtController("FecRateController::new_with_toggles ignores it (`_enable_pi_feedback`, the S2b finding)"),
        perf: Owed(PERF_DROPS_CONFIG),
    },
    Key { name: "ge_burst_factor", toml: "0.5", tunnel: Owed(NO_CONSUMER), perf: Owed(NO_CONSUMER) },
    Key { name: "realtime_burst_extra", toml: "0.5", tunnel: Owed(NO_CONSUMER), perf: Owed(NO_CONSUMER) },
    Key { name: "reorder_timeout_ms", toml: "50", tunnel: Moves("reorder_timeout_ms"), perf: Owed(PERF_DROPS_CONFIG) },
    Key { name: "reorder_max_size", toml: "100", tunnel: Moves("reorder_max_size"), perf: Owed(PERF_DROPS_CONFIG) },
    Key { name: "inner_feedback_weight", toml: "0.5", tunnel: MovesToController("inner_feedback_weight"), perf: Owed(PERF_DROPS_CONFIG) },
    Key { name: "mp_block_affinity", toml: "true", tunnel: Rejected, perf: Owed("perf drops --config: once read, resolve rejects it") },
    Key { name: "window_out_of_order", toml: "true", tunnel: ForcedOff, perf: Owed("perf takes the CLI flag only") },
    Key { name: "window_coded_only", toml: "true", tunnel: ForcedOff, perf: Owed("perf takes the CLI flag only") },
    Key { name: "window_generation_coding", toml: "true", tunnel: ForcedOff, perf: Owed("perf takes the CLI flag only") },
    Key { name: "window_systematic_repair", toml: "true", tunnel: ForcedOff, perf: Owed("perf takes the CLI flag only") },
];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Entry {
    Tunnel,
    PerfClient,
    PerfServer,
}

fn cli_baseline(e: Entry) -> RaptorpathConfig {
    match e {
        Entry::Tunnel => RaptorpathConfig::default(),
        Entry::PerfClient => RaptorpathConfig { peer: Some(vec!["192.0.2.1:4433".into()]), ..Default::default() },
        Entry::PerfServer => RaptorpathConfig {
            server: Some(true),
            bind: Some(vec!["192.0.2.2:4433".into()]),
            ..Default::default()
        },
    }
}

fn cli_sets(e: Entry, key: &str) -> bool {
    !serde_json::to_value(cli_baseline(e)).unwrap()[key].is_null()
}

/// Resolve through the entry point's own lib fn, with `toml_line` (or nothing)
/// as the config file the tunnel reads.
fn resolve_entry(e: Entry, toml_line: Option<&str>, tag: &str) -> Result<PeerConfig, String> {
    match e {
        Entry::Tunnel => {
            let path = toml_line.map(|l| {
                let p = std::env::temp_dir().join(format!("rp_s2c_{}_{tag}.toml", std::process::id()));
                std::fs::write(&p, format!("{l}\n")).expect("write temp config");
                p
            });
            let r = config::resolve_tunnel(None, path.as_deref(), cli_baseline(e))
                .map(|(p, _)| p)
                .map_err(|err| err.to_string());
            if let Some(p) = path {
                let _ = std::fs::remove_file(p);
            }
            r
        }
        // The perf resolution takes the CLI overlay alone (see the module doc),
        // so a file has nowhere to go: the same call is made for both arms.
        Entry::PerfClient | Entry::PerfServer => config::resolve_perf(&cli_baseline(e)).map_err(|err| err.to_string()),
    }
}

/// The controller as `net::run` builds it (`wiring_of_the_controller_is_pinned`
/// holds this to the source), probed over several loss, window and hint points
/// so a regime-limited key still moves it.
fn controller_probe(pc: &PeerConfig) -> Vec<u64> {
    use crate::control::fec_rate::ProtocolHint;
    let mut v = Vec::new();
    // The configured hint (so `protocol_hint` moves it) and Bulk, where the
    // mid-stream glide that `inner_feedback_weight` acts on lives.
    for (hint, pure_arq) in [
        (pc.protocol_hint, None),
        (ProtocolHint::Bulk, Some(true)),
        (ProtocolHint::Bulk, Some(false)),
    ] {
        let mut c = FecRateController::new_with_toggles(
            pc.target_tail_loss,
            pc.max_fec_overhead,
            hint,
            FecBackend::Rlc,
            pc.enable_pi_feedback,
            1200,
        );
        c.set_inner_feedback(pc.inner_feedback_weight);
        if let Some(p) = pure_arq {
            c.set_bulk_pure_arq(p);
        }
        v.push(c.target_tail_loss().to_bits());
        for lost in [1u32, 5, 20, 50, 300, 500] {
            let mut est = LossEstimator::new();
            for _ in 0..150 {
                est.record_batch(1000, 1000 - lost);
                est.record_rtt(std::time::Duration::from_millis(20));
                est.record_throughput(5e6);
            }
            for w in [16usize, 64, 256] {
                v.push(c.compute_repair_rate(&est, w).to_bits());
            }
        }
    }
    v
}

#[derive(Debug)]
struct Obs {
    moved: bool,
    err: Option<String>,
    ctl_moved: bool,
}

fn observe(e: Entry, k: &Key) -> Obs {
    let base = resolve_entry(e, None, "base").expect("the baseline must resolve");
    match resolve_entry(e, Some(&format!("{} = {}", k.name, k.toml)), k.name) {
        Ok(p) => Obs {
            moved: format!("{p:?}") != format!("{base:?}"),
            err: None,
            ctl_moved: controller_probe(&p) != controller_probe(&base),
        },
        Err(err) => Obs { moved: false, err: Some(err), ctl_moved: false },
    }
}

/// `strict`: an `Owed` cell must meet the behaviour it is owed.
fn judge(cell: Cell, o: &Obs, name: &str, strict: bool) -> Result<(), String> {
    let names_key = o.err.as_deref().is_some_and(|m| m.contains(name));
    let fixed = o.moved || names_key;
    let stray = |m: &str| Err(format!("{m} (err={:?})", o.err));
    match cell {
        Moves(_) if o.moved && o.err.is_none() => Ok(()),
        Moves(f) => stray(&format!("moves nothing: `{f}` stays at its baseline")),
        MovesToController(_) if o.moved && o.ctl_moved => Ok(()),
        MovesToController(f) if o.moved => stray(&format!("reaches PeerConfig.{f} and dies at the controller")),
        MovesToController(f) => stray(&format!("does not reach PeerConfig.{f}")),
        DiesAtController(why) => match (o.moved, o.ctl_moved, strict) {
            (true, false, false) => Ok(()),
            (true, true, false) => Err(format!("stale OWED, the controller now takes it: update the table ({why})")),
            (true, false, true) => Err(format!("OWED: reaches PeerConfig and dies at the controller: {why}")),
            (true, true, true) => Ok(()),
            (false, ..) => stray("does not even reach PeerConfig"),
        },
        NotRead(_) if !o.moved && o.err.is_none() => Ok(()),
        NotRead(why) => stray(&format!("moved, but the table says it is not read ({why})")),
        ForcedOff if names_key => Ok(()),
        ForcedOff if o.err.is_none() && !o.moved => Ok(()),
        ForcedOff if o.err.is_none() => stray("the entry point's forced value moved: the file leaks through"),
        ForcedOff => stray("rejected by an error that does not name the key"),
        Rejected if names_key => Ok(()),
        Rejected => stray("not rejected naming the key"),
        Owed(why) => match (fixed, strict) {
            (false, false) => Ok(()),
            (true, false) => Err(format!("stale OWED, it now works: update the table ({why})")),
            (false, true) => Err(format!("OWED: {why}")),
            (true, true) => Ok(()),
        },
    }
}

fn entries() -> [(Entry, &'static str); 3] {
    [(Entry::Tunnel, "tunnel run"), (Entry::PerfClient, "perf client"), (Entry::PerfServer, "perf server")]
}

fn run_table(strict: bool) -> Vec<String> {
    let mut bad = Vec::new();
    for (e, label) in entries() {
        for k in KEYS {
            let cell = match e {
                Entry::Tunnel => k.tunnel,
                // The role's own CLI flag sets this key and wins over any file.
                _ if cli_sets(e, k.name) => NotRead("the role's own CLI flag sets it, and the CLI wins over a file"),
                _ => k.perf,
            };
            let o = observe(e, k);
            if let Err(m) = judge(cell, &o, k.name, strict) {
                bad.push(format!("{label}: {}: {m}", k.name));
            }
        }
    }
    bad
}

#[test]
fn every_config_key_is_in_the_table_and_parses() {
    let fields = serde_json::to_value(RaptorpathConfig::default()).unwrap();
    let mut want: Vec<&str> = fields.as_object().unwrap().keys().map(String::as_str).collect();
    let mut have: Vec<&str> = KEYS.iter().map(|k| k.name).collect();
    want.sort_unstable();
    have.sort_unstable();
    assert_eq!(have, want, "KEYS must list exactly the RaptorpathConfig fields");
    for k in KEYS {
        let line = format!("{} = {}", k.name, k.toml);
        let parsed: RaptorpathConfig = toml::from_str(&line).unwrap_or_else(|e| panic!("`{line}` does not parse: {e}"));
        let v = serde_json::to_value(parsed).unwrap();
        assert!(!v[k.name].is_null(), "`{line}` did not set the key (non-default value expected)");
    }
}

#[test]
fn config_round_trip_matches_the_table_at_every_entry_point() {
    let bad = run_table(false);
    assert!(bad.is_empty(), "config round trip:\n  {}", bad.join("\n  "));
}

/// Run with `--ignored`: every OWED cell, listed. Red until the table is empty of `Owed`.
#[test]
#[ignore = "red by design: lists every OWED gap (perf --config, the tail target, enable_pi_feedback, dead keys)"]
fn owed_cells_are_red_when_demanded() {
    let bad = run_table(true);
    assert!(bad.is_empty(), "{} OWED gaps:\n  {}", bad.len(), bad.join("\n  "));
}

/// Every `Moves` cell's `PeerConfig` field is read by `net::run`, so a value
/// does not die one call after `resolve`.
#[test]
fn moved_peer_config_fields_are_read_by_the_engine() {
    let src = include_str!("net/mod.rs");
    for k in KEYS {
        for cell in [k.tunnel, k.perf] {
            if let Moves(f) | MovesToController(f) = cell {
                assert!(
                    src.contains(&format!("config.{f}")) || src.contains(&format!("&config.{f}")),
                    "net/mod.rs never reads config.{f} (key `{}`)",
                    k.name
                );
            }
        }
    }
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `controller_probe` mirrors `net::run`'s construction; this holds it to the source.
#[test]
fn wiring_of_the_controller_is_pinned() {
    let src = squash(include_str!("net/mod.rs"));
    assert!(
        src.contains(
            "FecRateController::new_with_toggles( config.target_tail_loss, config.max_fec_overhead, config.protocol_hint, effective_fec_backend, config.enable_pi_feedback, symbol_size, );"
        ),
        "net::run no longer builds the controller as controller_probe does: update both"
    );
    assert!(
        src.contains("ctrl.set_inner_feedback(config.inner_feedback_weight);"),
        "net::run no longer feeds inner_feedback_weight to the controller: update controller_probe"
    );
}

/// `main.rs` routes each command through the lib fn the test exercises, and
/// perf still gets no config path (the OWED claim).
#[test]
fn main_rs_wiring_pins() {
    let src = squash(include_str!("main.rs"));
    assert!(src.contains("Commands::Run(args) => cmd_run(cli.config, args).await"), "run must receive the config path");
    assert!(src.contains("config::resolve_tunnel("), "cmd_run must resolve through config::resolve_tunnel");
    assert!(src.contains("config::resolve_perf("), "cmd_perf must resolve through config::resolve_perf");
    assert_eq!(
        src.matches("config::resolve(").count(),
        1,
        "only cmd_check may call config::resolve directly; run and perf go through their lib fns"
    );
    assert!(
        src.contains("Commands::Perf(args) => cmd_perf(args).await"),
        "OWED: perf takes no config path; if this pin fails, perf reads --config: replace the Owed perf cells and give resolve_perf a file"
    );
}
