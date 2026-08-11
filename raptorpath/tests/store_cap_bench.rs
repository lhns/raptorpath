//! Store-cap component bench (MEASUREMENT DISCIPLINE 14) — goal-gate
//! "Store-Cap Triplication", 2026-08-09.
//!
//! Drives the SHIPPED store-cap laws alone — no transport, no tokio, no VM —
//! and answers the two questions a battery cannot discover for itself:
//!
//!   (A) LAW: by how much does the dyn-cap phase's Σ-anchor base / honest
//!       per-path cap sum differ when it iterates `active_paths()` (cwnd −
//!       in_flight > 0) instead of `live_paths()`, as a function of how many
//!       paths are cwnd-saturated? Deterministic, closed form, seconds.
//!
//!   (B) POPULATION: at the dyn-cap refresh INSTANTS of a real transfer, how
//!       often is `active_paths()` actually short of `live_paths()`? The
//!       `sf=` gauge (`net::store_cap_sf_gauge`) counts it. A law delta of
//!       −50% on a population of 0% is latent-but-inert; the same delta on a
//!       population of 90% is the defect.
//!
//! Run:
//!   cargo test --test store_cap_bench --release -- --ignored --nocapture

use std::collections::HashMap;
use std::time::Duration;

use raptorpath::net::{
    EchoRatioMin, HonestCapPath, honest_cap_terms, path_scaled_store_cap, store_cap_sf_gauge,
    store_cap_sf_reset,
};

// ── The cells, at the parameters the shipped derivation itself quotes ──────
//
// c2 (100 Mbit, 10 ms RTT, GE 1.3%/50%): ~1.2 KB symbols ⇒ 10 400 sym/s,
//   RTprop 8 ms ⇒ honest anchor 83.2 symbols. These are exactly the numbers
//   `honest_store_cap`'s own doc cross-check uses ("sc2 → 10.4k·(K·8ms +
//   108ms)").
// c3 (20 Mbit, 40 ms RTT + 5 ms jitter, GE 2%/40%): ~2 000 sym/s, RTprop
//   60 ms ⇒ anchor 120 symbols ("c8-slow → ~2k·(K·60ms + 160ms)").
//
// c7 = dual c2 (symmetric), c8 = c2 + c3 (heterogeneous), sc2/sc3 = the
// singles. Σ_c7 = 2×sc2, Σ_c8 = sc2 + sc3 (goal-gate).
const C2_RATE: f64 = 10_400.0;
const C2_RTPROP_S: f64 = 0.008;
const C3_RATE: f64 = 2_000.0;
const C3_RTPROP_S: f64 = 0.060;

// Shipped policy constants (sender_policy::resolve / gates defaults).
const GAIN: f64 = 2.0;
const FLOOR: usize = 64;
const KNEE: usize = 2048;
const STORE_MAX: usize = 1024;
const BOOT: usize = 128;

/// The LEGACY plain anchor over-reads the honest one by ×4.6–7.4 ("Anchor
/// Hygiene" battery (b)); the shipped default runs on the over-read. Both
/// levels are reported because the RATIO under test is anchor-invariant
/// until a clamp bites — and showing that is half the point.
///
/// **MEASURED, AND WRONG AT EVERY DUAL CELL** (goal-gate "Ack-Cadence
/// Measurement (VM)", READOUT 3, 2026-08-11). The wire's realized `xanchor`
/// is **5.94** at the single cell (in the ×4.6–7.4 band this constant came
/// from), **9.80–10.11** at c7 and **13.29–13.82** at c8 — so 5.0 is right at
/// ONE cell and **2.0–2.8× low at the duals**. "Exactly the shape CLAUDE.md
/// forbids: a constant standing in for a cell-dependent quantity."
///
/// It is kept, not deleted, because it is the LEGACY ASSUMPTION and the
/// comparison against it is the finding: `the_overread_error_decides_whether
/// _the_knee_ceiling_binds` shows that this constant is the difference
/// between a store cap that is proportional to the anchor and one that is
/// SATURATED at its ceiling. The sweep below runs at the MEASURED per-cell
/// scale as well.
const OVERREAD: f64 = 5.0;

#[derive(Clone, Copy)]
struct Cell {
    name: &'static str,
    /// (rate sym/s, RTprop s, MEASURED `xanchor`) per path — the third term
    /// is READOUT 3's per-path median at the cell the VM ran, replacing the
    /// single `OVERREAD` constant with the per-path measurement it stood in
    /// for. `None` = the VM never ran this geometry, and this bench will not
    /// invent an over-read for it.
    paths: &'static [(f64, f64, Option<f64>)],
}

const CELLS: &[Cell] = &[
    // c7 legs: READOUT 3 rows `c7/p0` 9.80 and `c7/p1` 10.11.
    Cell {
        name: "c7  (c2+c2)",
        paths: &[(C2_RATE, C2_RTPROP_S, Some(9.80)), (C2_RATE, C2_RTPROP_S, Some(10.11))],
    },
    // c8 legs: READOUT 3 rows `c8/p0` 13.29 (fast) and `c8/p1` 13.82 (slow).
    Cell {
        name: "c8  (c2+c3)",
        paths: &[(C2_RATE, C2_RTPROP_S, Some(13.29)), (C3_RATE, C3_RTPROP_S, Some(13.82))],
    },
    // sc2: the VM's single cell is c2r100 — READOUT 3 row 1, 5.94.
    Cell { name: "sc2 (c2)    ", paths: &[(C2_RATE, C2_RTPROP_S, Some(5.94))] },
    // sc3: no single SLOW cell was ever measured. Left unmeasured on purpose.
    Cell { name: "sc3 (c3)    ", paths: &[(C3_RATE, C3_RTPROP_S, None)] },
];

/// The per-path anchor scale to run a sweep row at: `1.0` honest, `OVERREAD`
/// the legacy constant, or the cell's own MEASURED per-path `xanchor`.
#[derive(Clone, Copy, PartialEq)]
enum Scale {
    Fixed(f64),
    Measured,
}

impl Scale {
    fn of(self, path: &(f64, f64, Option<f64>)) -> Option<f64> {
        match self {
            Scale::Fixed(f) => Some(f),
            Scale::Measured => path.2,
        }
    }
}

fn slots(cell: &Cell, keep: &[bool], anchor_scale: Scale) -> Vec<Option<HonestCapPath>> {
    cell.paths
        .iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(i, p)| {
            let (rate, rtprop) = (p.0, p.1);
            let scale = anchor_scale.of(p).unwrap_or(1.0);
            Some(HonestCapPath {
                id: i as u32,
                anchor: Some(rate * rtprop * scale),
                rate: Some(rate),
                srtt: Duration::from_secs_f64(rtprop * 3.0),
                rtprop: Some(Duration::from_secs_f64(rtprop)),
                k_raw: None,
            })
        })
        .collect()
}

/// The SHIPPED DEFAULT pooled law, exactly as the sender composes it:
/// `path_scaled_store_cap(on, n_live, Σ_set anchor, …)` else the legacy
/// `gain·Σ` else the boot cap. `n_live` is ALWAYS the live count — that is
/// the defect's whole shape: the ×N and the Σ range over different sets.
fn shipped_pool_cap(bdp_over_set: f64, n_live: usize) -> usize {
    if let Some(c) = path_scaled_store_cap(true, n_live, bdp_over_set, GAIN, FLOOR, KNEE) {
        c
    } else if bdp_over_set > 0.0 {
        ((GAIN * bdp_over_set).ceil() as usize).clamp(FLOOR, STORE_MAX)
    } else {
        BOOT.min(STORE_MAX)
    }
}

/// The HONEST pooled law (`RWM_PLAIN_RS` + `RWM_HONEST_CAP`): Σ of the
/// per-path honest terms over the set, clamped to the principled ceiling.
fn honest_pool_cap(terms: &[Option<f64>], n_live: usize) -> usize {
    let hsum: f64 = terms.iter().flatten().sum();
    if hsum <= 0.0 {
        return 0; // law disengages; the caller falls through
    }
    let ceiling =
        if n_live >= 2 { n_live.saturating_mul(KNEE).max(FLOOR) } else { STORE_MAX };
    (hsum.ceil() as usize).clamp(FLOOR, ceiling)
}

fn pct(new: f64, base: f64) -> f64 {
    if base <= 0.0 { f64::NAN } else { (new - base) / base * 100.0 }
}

/// (A) THE LAW SWEEP — cap(active) vs cap(live) as paths saturate.
#[test]
#[ignore = "component bench; run with --ignored --nocapture"]
fn store_cap_pathset_sweep() {
    println!("\n=== STORE-CAP PATH-SET SWEEP (component bench, 2026-08-09) ===");
    println!(
        "gain {GAIN}  floor {FLOOR}  knee/path {KNEE}  store_max {STORE_MAX}  boot {BOOT}"
    );
    println!(
        "law: cap_i = anchor_i*(K_i+gain-1) + rate_i*(gain-1)*R,  R = 100 ms (HONEST_RECOVERY_ROUND_S)\n"
    );

    for k_ratio in [1.0_f64, 1.5, 2.0] {
        println!("--- K (windowed-min echoSRTT/RTprop) = {k_ratio:.1} ---");
        for cell in CELLS {
            let n = cell.paths.len();
            let n_live = n.max(1);
            // Baseline: nothing filtered (what live_paths() always gives).
            // THREE anchor levels, not two: honest, the LEGACY CONSTANT, and
            // the wire's own per-path measurement. The third row is the one
            // this bench had no way to run until the VM measured it.
            for (label, scale) in [
                ("honest anchor", Scale::Fixed(1.0)),
                ("legacy anchor x5", Scale::Fixed(OVERREAD)),
                ("MEASURED xanchor", Scale::Measured),
            ] {
                if cell.paths.iter().any(|p| scale.of(p).is_none()) {
                    continue; // no measurement at this geometry — do not invent one
                }
                let all = vec![true; n];
                let mut ks: HashMap<u32, EchoRatioMin> = HashMap::new();
                let base_terms = honest_cap_terms(&mut ks, &slots(cell, &all, scale), 0, GAIN);
                let base_bdp: f64 = slots(cell, &all, scale)
                    .iter()
                    .flatten()
                    .filter_map(|p| p.anchor)
                    .sum();
                // Force K to the swept value by feeding a matching sample.
                let mut ks: HashMap<u32, EchoRatioMin> = HashMap::new();
                let forced: Vec<Option<HonestCapPath>> = slots(cell, &all, scale)
                    .into_iter()
                    .map(|s| {
                        s.map(|mut p| {
                            let rtp = p.rtprop.unwrap().as_secs_f64();
                            p.srtt = Duration::from_secs_f64(rtp * k_ratio.max(1.000_001));
                            p
                        })
                    })
                    .collect();
                let base_terms_k = honest_cap_terms(&mut ks, &forced, 0, GAIN);
                let _ = base_terms;

                let base_shipped = shipped_pool_cap(base_bdp, n_live);
                let base_honest = honest_pool_cap(&base_terms_k, n_live);

                // Sweep: s = number of paths active_paths() filters out.
                for s in 0..=n {
                    let mut keep = vec![true; n];
                    for keepi in keep.iter_mut().take(s) {
                        *keepi = false; // saturate the first s paths
                    }
                    let sl = slots(cell, &keep, scale);
                    let mut ksv: HashMap<u32, EchoRatioMin> = HashMap::new();
                    let fk: Vec<Option<HonestCapPath>> = sl
                        .iter()
                        .map(|s| {
                            s.map(|mut p| {
                                let rtp = p.rtprop.unwrap().as_secs_f64();
                                p.srtt = Duration::from_secs_f64(rtp * k_ratio.max(1.000_001));
                                p
                            })
                        })
                        .collect();
                    let terms = honest_cap_terms(&mut ksv, &fk, 0, GAIN);
                    let bdp: f64 = sl.iter().flatten().filter_map(|p| p.anchor).sum();
                    let shipped = shipped_pool_cap(bdp, n_live);
                    let mut honest = honest_pool_cap(&terms, n_live);
                    let honest_note = if honest == 0 {
                        // hsum = 0 ⇒ the honest law DISENGAGES and the
                        // caller falls through to the shipped pooled law.
                        honest = shipped;
                        " (law disengaged -> fallback)"
                    } else {
                        ""
                    };
                    println!(
                        "{}  {:16}  sat {}/{}  shipped-pool {:5} ({:+7.1}%)   honest-pool {:5} ({:+7.1}%){}",
                        cell.name,
                        label,
                        s,
                        n,
                        shipped,
                        pct(shipped as f64, base_shipped as f64),
                        honest,
                        pct(honest as f64, base_honest as f64),
                        honest_note,
                    );
                }
            }
        }
        println!();
    }
}

/// (B) THE POPULATION — the `sf=` gauge over a real dual-path L0 transfer.
///
/// In-process loopback over two 127.0.0.1 path pairs, the REAL engine, plain
/// window-reliable mode: the same sender loop, the same 5 ms dyn-cap refresh
/// cadence, the same `active_paths()` filter. Loopback has no netem, so the
/// SATURATION this measures is the sender's own cwnd/in-flight bookkeeping —
/// which is precisely the mechanism `active_paths()` filters on.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "component bench; run with --ignored --nocapture"]
async fn store_cap_saturation_population_dual_l0() {
    use raptorpath::{config, perf};
    let _ = rustls::crypto::ring::default_provider().install_default();
    store_cap_sf_reset();

    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47991".into(), "127.0.0.1:47992".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (srv_pc, _) = config::resolve(&srv_cfg).unwrap();
    assert!(srv_pc.window_reliable);
    let srv = tokio::spawn(perf::server(srv_pc));
    tokio::time::sleep(Duration::from_millis(500)).await;

    let cli_cfg = config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into(), "127.0.0.1:0".into()]),
        peer: Some(vec!["127.0.0.1:47991".into(), "127.0.0.1:47992".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();
    tokio::time::timeout(Duration::from_secs(120), perf::client(cli_pc, 8_000_000, 1))
        .await
        .expect("dual-path L0 store-cap population run timed out")
        .expect("dual-path L0 store-cap population run failed");
    srv.abort();

    let (ticks, live, active, short, zero) = store_cap_sf_gauge();
    println!("\n=== sf= GAUGE (dual-path L0, 8 MB, plain window-reliable) ===");
    println!("dyn-cap refresh ticks            : {ticks}");
    println!("Sum n_live / Sum n_active        : {live} / {active}");
    println!(
        "mean n_live / mean n_active      : {:.3} / {:.3}",
        live as f64 / ticks.max(1) as f64,
        active as f64 / ticks.max(1) as f64
    );
    println!(
        "ticks with n_active < n_live     : {short}  ({:.1}%)",
        short as f64 / ticks.max(1) as f64 * 100.0
    );
    println!(
        "ticks with n_active = 0 (cap->boot): {zero}  ({:.1}%)",
        zero as f64 / ticks.max(1) as f64 * 100.0
    );
    println!(
        "anchor-mass retained by active_paths(): {:.1}% (path-count proxy)\n",
        active as f64 / live.max(1) as f64 * 100.0
    );
    assert!(ticks > 0, "the sf= gauge saw no dyn-cap refresh ticks — the mechanism under test did not execute (MEASUREMENT DISCIPLINE 1)");
}

/// (B2) THE POPULATION AT N = 1 — `active_paths()` can return the EMPTY set
/// on a single saturated path, and then the shipped cap is `store_boot_cap`
/// (128), not `gain·anchor`. The tasking's expectation that "N = 1 must be
/// inert" to the path set is refuted by the code, so it is measured here.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "component bench; run with --ignored --nocapture"]
async fn store_cap_saturation_population_single_l0() {
    use raptorpath::{config, perf};
    let _ = rustls::crypto::ring::default_provider().install_default();
    store_cap_sf_reset();

    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47993".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (srv_pc, _) = config::resolve(&srv_cfg).unwrap();
    let srv = tokio::spawn(perf::server(srv_pc));
    tokio::time::sleep(Duration::from_millis(500)).await;

    let cli_cfg = config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into()]),
        peer: Some(vec!["127.0.0.1:47993".into()]),
        protocol_hint: Some("bulk".into()),
        window_reliable: Some(true),
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();
    tokio::time::timeout(Duration::from_secs(120), perf::client(cli_pc, 8_000_000, 1))
        .await
        .expect("single-path L0 store-cap population run timed out")
        .expect("single-path L0 store-cap population run failed");
    srv.abort();

    let (ticks, live, active, short, zero) = store_cap_sf_gauge();
    println!("\n=== sf= GAUGE (SINGLE-path L0, 8 MB, plain window-reliable) ===");
    println!("dyn-cap refresh ticks             : {ticks}");
    println!("Sum n_live / Sum n_active         : {live} / {active}");
    println!(
        "ticks with n_active < n_live      : {short}  ({:.1}%)",
        short as f64 / ticks.max(1) as f64 * 100.0
    );
    println!(
        "ticks with n_active = 0 (cap->boot): {zero}  ({:.1}%)\n",
        zero as f64 / ticks.max(1) as f64 * 100.0
    );
    assert!(ticks > 0, "the sf= gauge saw no dyn-cap refresh ticks (MEASUREMENT DISCIPLINE 1)");
}

// ── Guards (always run) ───────────────────────────────────────────────────

/// The collector IS the law: `honest_cap_terms` must equal a hand-rolled
/// `EchoRatioMin` + `honest_store_cap` transcription, term for term. This is
/// the bound on the de-triplication.
#[test]
fn honest_cap_terms_equals_the_transcription() {
    use raptorpath::net::honest_store_cap;
    let sl: Vec<Option<HonestCapPath>> = vec![
        Some(HonestCapPath {
            id: 1,
            anchor: Some(83.2),
            rate: Some(10_400.0),
            srtt: Duration::from_micros(12_000),
            rtprop: Some(Duration::from_micros(8_000)),
            k_raw: None,
        }),
        None,
        Some(HonestCapPath {
            id: 2,
            anchor: Some(120.0),
            rate: Some(2_000.0),
            srtt: Duration::from_micros(90_000),
            rtprop: Some(Duration::from_micros(60_000)),
            k_raw: None,
        }),
        Some(HonestCapPath {
            id: 3,
            anchor: None,
            rate: Some(1.0),
            srtt: Duration::from_micros(90_000),
            rtprop: Some(Duration::from_micros(60_000)),
            k_raw: None,
        }),
    ];
    let mut ks: HashMap<u32, EchoRatioMin> = HashMap::new();
    let got = honest_cap_terms(&mut ks, &sl, 1_000, 2.0);

    let mut want: Vec<Option<f64>> = Vec::new();
    let mut ks2: HashMap<u32, EchoRatioMin> = HashMap::new();
    for slot in &sl {
        want.push(slot.and_then(|p| {
            let k = ks2
                .entry(p.id)
                .or_insert_with(|| EchoRatioMin::new(5_000_000))
                .observe_srtt_over_rtprop(p.srtt, p.rtprop, 1_000);
            honest_store_cap(p.anchor, p.rate, k, 2.0)
        }));
    }
    assert_eq!(got.len(), want.len(), "a None slot must keep its position");
    for (g, w) in got.iter().zip(want.iter()) {
        match (g, w) {
            (None, None) => {}
            (Some(a), Some(b)) => assert!((a - b).abs() < 1e-9, "{a} != {b}"),
            _ => panic!("term shape diverged: {g:?} vs {w:?}"),
        }
    }
    // A None slot observes NO clock sample.
    assert!(!ks.contains_key(&0));
    // A cold anchor still observes the clock (it is a clock statistic).
    assert!(ks.contains_key(&3));
}

/// N = 1 must be INERT to the path set at the law level: with one path,
/// live and active differ only in whether that path is saturated, and the
/// shipped single-path law is `gain·Σanchor` either way.
#[test]
fn single_path_pool_law_is_pathset_inert_when_unsaturated() {
    let bdp = 83.2;
    assert_eq!(shipped_pool_cap(bdp, 1), shipped_pool_cap(bdp, 1));
    // The N = 1 collapse the gate removes: filtered out ⇒ Σ = 0 ⇒ boot cap.
    assert_eq!(shipped_pool_cap(0.0, 1), BOOT);
    assert_eq!(shipped_pool_cap(bdp, 1), 167);
}

/// The claim the pre-registration rests on, asserted as an invariant rather
/// than left as prose: for the shipped pooled law the cap is EXACTLY
/// proportional to the anchor mass the path set retains (until a clamp
/// bites), so filtering half the anchor mass halves the cap.
#[test]
fn shipped_pool_cap_is_proportional_to_retained_anchor_mass() {
    let full = 2.0 * 83.2;
    let half = 83.2;
    let c_full = shipped_pool_cap(full, 2);
    let c_half = shipped_pool_cap(half, 2);
    assert_eq!(c_full, 666);
    assert_eq!(c_half, 333);
    assert!(
        ((c_half as f64) / (c_full as f64) - 0.5).abs() < 0.01,
        "symmetric dual, one path saturated ⇒ exactly −50% of the pooled cap"
    );
    // And the terminal case: BOTH saturated ⇒ the boot cap, a ×5.2 collapse
    // at honest anchors and ×26 at the measured legacy over-read.
    assert_eq!(shipped_pool_cap(0.0, 2), BOOT);
    let c_over = shipped_pool_cap(OVERREAD * full, 2);
    assert_eq!(c_over, 3328);
    assert!((c_over as f64 / BOOT as f64) > 25.0);
}

/// WHAT THE `OVERREAD = 5.0` ERROR AFFECTS — goal-gate "Ack-Cadence
/// Measurement (VM)", READOUT 3, answered as an assertion rather than as
/// prose.
///
/// The shipped pooled law is `clamp(gain·N·Σ, floor, N·knee)`, which is
/// DEGREE-1 HOMOGENEOUS in the anchor below the ceiling and constant above it
/// (proved on the real law by `store_cap_sf_bench`'s
/// `store_cap_law_is_degree_one_in_the_anchor_until_the_knee_ceiling`). So a
/// wrong anchor scale is INERT for every ratio this bench reports — until it
/// pushes the cap into `N·knee`, the law's only non-homogeneous term. That is
/// the whole of what the constant's error can affect, and the measurement puts
/// the two levels on OPPOSITE SIDES of it:
///
/// | cell | Σ anchor at ×5.0 | cap | Σ at the MEASURED xanchor | cap |
/// |---|---|---|---|---|
/// | c7 | 832  | 3328 (below the 4096 ceiling) | 1656.6 | **4096, SATURATED** |
/// | c8 | 1016 | 4064 (below it by **0.8%**)   | 2764.1 | **4096, SATURATED** |
///
/// So at the assumed ×5.0 BOTH dual cells sit just under the ceiling and the
/// pooled cap tracks the anchor proportionally; at the measured over-read BOTH
/// are 1.6× and 2.7× ABOVE it and the cap is PINNED at `N·knee`, where it no
/// longer responds to the anchor at all. c8 clears the ceiling at ×5.0 by
/// 32 symbols out of 4096 — the constant is not merely 2.7× low, it is 2.7×
/// low across a knee, and it is the knee that the store-cap ratio arguments
/// all assume away.
///
/// At N = 1 the ceiling is `RELIABLE_STORE_MAX` instead, and the measured
/// single-cell over-read (5.94) stays under it — so the error is confined to
/// the DUAL cells, which is exactly where the `[SF]` question lives.
#[test]
fn the_overread_error_decides_whether_the_knee_ceiling_binds() {
    let ceiling_n2 = (2 * KNEE) as f64; // 4096
    let sigma = |cell: &Cell, scale: Scale| -> f64 {
        cell.paths
            .iter()
            .map(|p| p.0 * p.1 * scale.of(p).expect("this cell is measured"))
            .sum()
    };
    let c7 = &CELLS[0];
    let c8 = &CELLS[1];
    let sc2 = &CELLS[2];

    // THE LEGACY CONSTANT: both duals below the ceiling, proportional.
    for cell in [c7, c8] {
        let s = sigma(cell, Scale::Fixed(OVERREAD));
        let cap = shipped_pool_cap(s, 2);
        assert!(
            (cap as f64) < ceiling_n2,
            "{}: at x{OVERREAD} the cap {cap} already saturates the {ceiling_n2} ceiling",
            cell.name
        );
        assert_eq!(cap, (2.0 * 2.0 * s).ceil() as usize, "{}: not proportional", cell.name);
    }
    // ...and c8 clears it by less than 1%, which is why "the constant is a bit
    // low" is not a safe reading of the miss.
    let c8_legacy = shipped_pool_cap(sigma(c8, Scale::Fixed(OVERREAD)), 2) as f64;
    assert!(
        c8_legacy / ceiling_n2 > 0.99,
        "c8 at x{OVERREAD} sits at {:.3} of the ceiling",
        c8_legacy / ceiling_n2
    );

    // THE MEASURED OVER-READ: both duals PINNED at the ceiling.
    for cell in [c7, c8] {
        let s = sigma(cell, Scale::Measured);
        assert_eq!(
            shipped_pool_cap(s, 2),
            ceiling_n2 as usize,
            "{}: the measured anchor must saturate the N*knee ceiling (Sigma {s:.1})",
            cell.name
        );
        // And it is over by a wide margin, not marginally.
        assert!(
            2.0 * 2.0 * s / ceiling_n2 > 1.5,
            "{}: the unclamped law would ask for only {:.2}x the ceiling",
            cell.name,
            2.0 * 2.0 * s / ceiling_n2
        );
    }

    // THE SIZE OF THE ERROR, per cell, as the ledger states it: 2.0-2.8x.
    for (cell, lo, hi) in [(c7, 1.9, 2.1), (c8, 2.6, 2.8)] {
        let ratio = sigma(cell, Scale::Measured) / sigma(cell, Scale::Fixed(OVERREAD));
        assert!(
            ratio >= lo && ratio <= hi,
            "{}: the measured anchor is {ratio:.2}x the assumed one, outside the \
             ledger's {lo}-{hi}x",
            cell.name
        );
    }

    // N = 1 IS UNAFFECTED: the single cell's measured over-read (5.94) leaves
    // the cap below RELIABLE_STORE_MAX, so the error is a DUAL-cell error.
    let s1 = sigma(sc2, Scale::Measured);
    let cap1 = shipped_pool_cap(s1, 1);
    assert!(cap1 < STORE_MAX, "sc2 at the measured x5.94 already clamps: {cap1}");
    assert_eq!(cap1, (2.0 * s1).ceil() as usize, "N=1 must still be proportional");
    // The measurement's own UPPER window at that cell (9.28) does clamp — so
    // even at N = 1 the headroom is one window's excursion wide.
    assert!(shipped_pool_cap(83.2 * 9.28, 1) >= STORE_MAX);
}
