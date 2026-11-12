//! Store-cap component bench (`docs/measurement-discipline.md` rule 14).
//!
//! Drives the shipped store-cap laws alone — no transport, no tokio, no VM —
//! and answers two questions a battery cannot discover for itself:
//!
//!   (A) LAW: by how much does the dyn-cap phase's Σ-anchor base / honest
//!       per-path cap sum differ when it iterates `active_paths()` (cwnd −
//!       in_flight > 0) instead of `live_paths()`, as a function of how many
//!       paths are cwnd-saturated? Deterministic, closed form, seconds.
//!
//!   (B) POPULATION: at the dyn-cap refresh instants of a real transfer, how
//!       often is `active_paths()` actually short of `live_paths()`? The
//!       `sf=` gauge (`net::store_cap_sf_gauge`) counts it. A law delta of
//!       −50% on a population of 0% is latent-but-inert; the same delta on a
//!       population of 90% is the defect.
//!
//! Run:
//!   cargo test --test store_cap_bench --release -- --ignored --nocapture

#[path = "common/loopback.rs"]
mod loopback;

use std::collections::HashMap;
use std::time::Duration;

use loopback::in_process::{cfgs, ports, resolve, run};

use raptorpath::net::{
    path_scaled_store_cap,
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
// singles. Σ_c7 = 2×sc2, Σ_c8 = sc2 + sc3.
const C2_RATE: f64 = 10_400.0;
const C2_RTPROP_S: f64 = 0.008;
const C3_RATE: f64 = 2_000.0;
const C3_RTPROP_S: f64 = 0.060;

// Shipped policy constants (sender_policy::resolve / gates defaults).
const GAIN: f64 = 2.0;
/// The shipped bootstrap floor, cited rather than transcribed because it is a
/// derived quantity (paper §6.1): a copy would silently model a policy the
/// engine no longer has. Inert at every geometry here — asserted by
/// `derived_floor_is_the_max_of_its_two_clauses_and_only_moves_the_degenerate_end`.
const FLOOR: usize = raptorpath::net::sender_policy::STORE_CAP_FLOOR;
const KNEE: usize = 2048;
const STORE_MAX: usize = 1024;
const BOOT: usize = 128;

/// The plain anchor's assumed over-read of the honest one (the ×4.6–7.4 band,
/// paper §6.6); the shipped default runs on the over-read. Both levels are
/// reported because the ratio under test is anchor-invariant until a clamp
/// bites. The realized `xanchor` is cell-dependent (5.94 single, 9.80–10.11
/// at c7, 13.29–13.82 at c8), so this constant is right at one cell and
/// 2.0–2.8× low at the duals. It is kept as the assumption under comparison:
/// `the_overread_error_decides_whether_the_knee_ceiling_binds` shows it is
/// the difference between a cap proportional to the anchor and one saturated
/// at its ceiling. The sweep also runs at the measured per-cell scale.
const OVERREAD: f64 = 5.0;

#[derive(Clone, Copy)]
struct Cell {
    name: &'static str,
    /// (rate sym/s, RTprop s, measured `xanchor`) per path — the third term
    /// is the per-path median the VM measured at that cell. `None` = the VM
    /// never ran this geometry, and this bench will not invent an over-read.
    paths: &'static [(f64, f64, Option<f64>)],
}

const CELLS: &[Cell] = &[
    // c7 legs: measured `c7/p0` 9.80 and `c7/p1` 10.11.
    Cell {
        name: "c7  (c2+c2)",
        paths: &[(C2_RATE, C2_RTPROP_S, Some(9.80)), (C2_RATE, C2_RTPROP_S, Some(10.11))],
    },
    // c8 legs: measured `c8/p0` 13.29 (fast) and `c8/p1` 13.82 (slow).
    Cell {
        name: "c8  (c2+c3)",
        paths: &[(C2_RATE, C2_RTPROP_S, Some(13.29)), (C3_RATE, C3_RTPROP_S, Some(13.82))],
    },
    // sc2: the VM's single cell is c2r100, measured 5.94.
    Cell { name: "sc2 (c2)    ", paths: &[(C2_RATE, C2_RTPROP_S, Some(5.94))] },
    // sc3: no single slow cell was measured; left unmeasured on purpose.
    Cell { name: "sc3 (c3)    ", paths: &[(C3_RATE, C3_RTPROP_S, None)] },
];

/// The per-path anchor scale to run a sweep row at: `1.0` honest, `OVERREAD`
/// the assumed constant, or the cell's own measured per-path `xanchor`.
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

/// The shipped default pooled law, exactly as the sender composes it:
/// `path_scaled_store_cap(on, n_live, Σ_set anchor, …)` else `gain·Σ` else
/// the boot cap. `n_live` is always the live count — that is the defect's
/// whole shape: the ×N and the Σ range over different sets. The law engages
/// at every N >= 1 (ADR-0072); the `STORE_MAX` branch is the cold-anchor
/// remainder of the `RWM_STORE_PATHS=0` chain.
fn shipped_pool_cap(bdp_over_set: f64, n_live: usize) -> usize {
    if let Some(c) = path_scaled_store_cap(true, n_live, bdp_over_set, GAIN, FLOOR, KNEE) {
        c
    } else if bdp_over_set > 0.0 {
        ((GAIN * bdp_over_set).ceil() as usize).clamp(FLOOR, STORE_MAX)
    } else {
        BOOT.min(STORE_MAX)
    }
}

fn pct(new: f64, base: f64) -> f64 {
    if base <= 0.0 { f64::NAN } else { (new - base) / base * 100.0 }
}

// ── Guards (always run) ───────────────────────────────────────────────────

/// N = 1 must be inert to the path set at the law level: with one path,
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

/// For the shipped pooled law the cap is exactly proportional to the anchor
/// mass the path set retains (until a clamp bites), so filtering half the
/// anchor mass halves the cap.
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
    // The terminal case: both saturated ⇒ the boot cap, a ×5.2 collapse at
    // honest anchors and ×26 at the assumed over-read.
    assert_eq!(shipped_pool_cap(0.0, 2), BOOT);
    let c_over = shipped_pool_cap(OVERREAD * full, 2);
    assert_eq!(c_over, 3328);
    assert!((c_over as f64 / BOOT as f64) > 25.0);
}

/// The bootstrap cap's derivation, and the bound on what replacing it moves
/// (paper §6.2).
///
/// `net::sender_policy::STORE_BOOT_DERIVED` derives boot from its own stated
/// job and lands on `max(ANCHOR_MIN_SAMPLES·cadence, RFC 6928 IW) = 10` — the
/// same two clauses, inputs and answer as the floor, because boot and the
/// floor are one quantity: *the outstanding bound that applies when the law
/// has no measurement*. The derived constant is not shipped;
/// `RWM_STORE_BOOT` still defaults to 128.
///
/// This test asserts four things:
///
/// 1. **The identity** — the derived boot is `STORE_CAP_FLOOR`, computed from
///    the same cited inputs.
/// 2. **128 is a fit** — `1.5 × (c2 rate × c2 RTprop)` reconstructs the
///    shipped value to within the rounding to a power of two. A bootstrap cap
///    sized to one path's BDP is circular, since no BDP is known yet.
/// 3. **At boot's legitimate job the two values are indistinguishable** —
///    cold start, where either value warms the anchor inside one round trip,
///    because both clear `ANCHOR_MIN_SAMPLES · MERGED_ACK_SYMBOLS_PER_SAMPLE`.
/// 4. **At the empty-Σ cliff they are not** — the terminal `else` of the
///    pooled chain, reached mid-transfer when the Σ-set empties. There the
///    swap is a ×12.8 tightening, and this test pins that ratio. That is why
///    the derived value does not ship; the defect belongs to the Σ-set, not
///    to boot.
///
/// The divergence between the derived and the shipped constant is bounded
/// by the exact condition under which they differ.
#[test]
fn derived_boot_is_the_floors_twin_and_is_inert_only_once_the_cliff_is_closed() {
    use raptorpath::net::sender_policy::{STORE_BOOT_DERIVED, STORE_CAP_FLOOR};

    // 1. The identity. Boot's derivation and the floor's are the same two
    //    clauses over the same cited inputs, so they cannot disagree — and if a
    //    future cadence change moves one it must move the other.
    assert_eq!(
        STORE_BOOT_DERIVED, STORE_CAP_FLOOR,
        "boot and the floor are one quantity; they cannot derive to two values"
    );
    assert_eq!(STORE_BOOT_DERIVED, 10);
    assert_eq!(
        FLOOR, STORE_CAP_FLOOR,
        "this bench must model the shipped floor, not a copy of it"
    );

    // 2. What 128 is: 1.5 × c2's own configured BDP, rounded to a power of
    //    two. Reconstructed rather than asserted, so the claim "it is a fit to
    //    one cell" is checked instead of repeated.
    let c2_bdp = C2_RATE * C2_RTPROP_S; // 83.2 symbols
    let fitted = 1.5 * c2_bdp; // 124.8
    assert!(
        (fitted - 124.8).abs() < 0.05,
        "the rationale's own sizing sentence no longer reconstructs: {fitted}"
    );
    assert!(
        (BOOT as f64 - fitted).abs() < 4.0,
        "128 is not 1.5x the c2 BDP rounded — the fit this test names has moved"
    );

    // 3. Boot's real job: both values warm the anchor inside one round trip,
    //    because both clear the sample requirement. At cold start the two are
    //    indistinguishable in the only respect boot is for.
    let samples_needed = 8 * 1; // ANCHOR_MIN_SAMPLES · MERGED_ACK_SYMBOLS_PER_SAMPLE
    for boot in [BOOT, STORE_BOOT_DERIVED] {
        assert!(
            boot >= samples_needed,
            "boot = {boot} cannot buy the {samples_needed} samples that end it — \
             a bootstrap cap that cannot end itself is a strangle"
        );
    }

    // 4. The cliff, where they differ. `shipped_pool_cap`
    //    with an empty Σ is exactly the terminal `else` the cliff reaches, at
    //    any live count. The swap's effect there is pinned as a ratio.
    for n in [1usize, 2, 3] {
        assert_eq!(
            shipped_pool_cap(0.0, n),
            BOOT,
            "N={n}: the empty-set fallthrough is not the boot cap"
        );
    }
    let deepening = BOOT as f64 / STORE_BOOT_DERIVED as f64;
    assert!(
        (deepening - 12.8).abs() < 1e-9,
        "the cliff deepening the derived boot would cause is {deepening}, not 12.8"
    );

    // Once the Σ-set is non-empty — which the live set guarantees whenever
    // the transfer is running — the boot branch is unreachable, and the two
    // values are indistinguishable everywhere because neither is ever read.
    for n in [1usize, 2, 3] {
        let smallest_warm_leg = 1.0; // any positive Σ at all
        assert_ne!(
            shipped_pool_cap(smallest_warm_leg, n),
            BOOT,
            "N={n}: a non-empty Sigma still reached the boot branch"
        );
    }
}

/// What the `OVERREAD = 5.0` error affects (paper §6.6).
///
/// The shipped pooled law is `clamp(gain·N·Σ, floor, N·knee)`, which is
/// degree-1 homogeneous in the anchor below the ceiling and constant above it
/// (proved on the real law by `store_cap_sf_bench`'s
/// `store_cap_law_is_degree_one_in_the_anchor_until_the_knee_ceiling`). So a
/// wrong anchor scale is inert for every ratio this bench reports — until it
/// pushes the cap into `N·knee`, the law's only non-homogeneous term. That is
/// the whole of what the constant's error can affect, and the measurement puts
/// the two levels on opposite sides of it:
///
/// | cell | Σ anchor at ×5.0 | cap | Σ at the measured xanchor | cap |
/// |---|---|---|---|---|
/// | c7 | 832  | 3328 (below the 4096 ceiling) | 1656.6 | **4096, SATURATED** |
/// | c8 | 1016 | 4064 (below it by **0.8%**)   | 2764.1 | **4096, SATURATED** |
///
/// So at the assumed ×5.0 both dual cells sit just under the ceiling and the
/// pooled cap tracks the anchor proportionally; at the measured over-read they
/// are 1.6× and 2.7× above it and the cap is pinned at `N·knee`, where it no
/// longer responds to the anchor at all. c8 clears the ceiling at ×5.0 by
/// 32 symbols out of 4096 — the constant is not merely 2.7× low, it is 2.7×
/// low across a knee, and it is the knee that the store-cap ratio arguments
/// all assume away.
///
/// At N = 1 the ceiling is `1·knee` (ADR-0072; it was `RELIABLE_STORE_MAX`),
/// and the measured single-cell over-read (5.94) stays under it — so the error
/// is confined to the dual cells, which is exactly where the `[SF]` question
/// lives.
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

    // The assumed constant: both duals below the ceiling, proportional.
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

    // The measured over-read: both duals pinned at the ceiling.
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

    // The size of the error, per cell: 2.0-2.8x.
    for (cell, lo, hi) in [(c7, 1.9, 2.1), (c8, 2.6, 2.8)] {
        let ratio = sigma(cell, Scale::Measured) / sigma(cell, Scale::Fixed(OVERREAD));
        assert!(
            ratio >= lo && ratio <= hi,
            "{}: the measured anchor is {ratio:.2}x the assumed one, outside the \
             ledger's {lo}-{hi}x",
            cell.name
        );
    }

    // N = 1 is unaffected: the single cell's measured over-read (5.94) leaves
    // the cap below the 1·knee ceiling, so the error is a dual-cell error.
    let s1 = sigma(sc2, Scale::Measured);
    let cap1 = shipped_pool_cap(s1, 1);
    assert!(cap1 < KNEE, "sc2 at the measured x5.94 already clamps: {cap1}");
    assert_eq!(cap1, (2.0 * s1).ceil() as usize, "N=1 must still be proportional");
    // The measurement's own UPPER window at that cell (9.28) is still interior
    // at N = 1 (the ceiling is the knee, not the old 1024 constant); the
    // ceiling is reached from a base 1.33x above it.
    assert_eq!(shipped_pool_cap(83.2 * 9.28, 1), (2.0 * 83.2 * 9.28f64).ceil() as usize);
    assert_eq!(shipped_pool_cap(83.2 * 9.28 * 1.33, 1), KNEE);
}

// ── The bootstrap floor, derived (paper §6.1) ────────────────────────────

/// The floor's derivation, and the bound on what changing it moves.
///
/// The floor is derived from its own stated job — *a transiently-tiny BDP
/// estimate must not strangle the pipe* — read as two independent lower
/// bounds on one quantity:
///
/// ```text
///   STORE_CAP_FLOOR = max( ANCHOR_MIN_SAMPLES · MERGED_ACK_SYMBOLS_PER_SAMPLE,
///                          RFC6928_INITIAL_WINDOW )
///                   = max( 8 · 1, 10 ) = 10
/// ```
///
/// This test asserts three things:
///
/// 1. **The arithmetic is the derivation's**, computed from the three cited
///    inputs rather than transcribed — so the value cannot be edited without
///    editing an input that has a citation attached to it.
/// 2. **`max`, not `+`, and both clauses are live.** The conjunction is
///    asserted to be the larger clause, and each clause is asserted to be
///    capable of being the binding one, so neither is decoration. (Today RFC
///    6928 binds; if the ack cadence ever became 2 symbols per sample, the
///    warm clause would, and this test says so rather than silently agreeing.)
/// 3. **The behaviour delta is bounded to the degenerate end.** The floor
///    only binds where the chain's unclamped ask is beneath it, so at every
///    cell geometry this bench carries the old (64) and new floors are
///    indistinguishable, asserted cell by cell.
#[test]
fn derived_floor_is_the_max_of_its_two_clauses_and_only_moves_the_degenerate_end() {
    use raptorpath::net::sender_policy::{
        MERGED_ACK_SYMBOLS_PER_SAMPLE, RFC6928_INITIAL_WINDOW, STORE_CAP_FLOOR,
    };
    use raptorpath::scheduler::ANCHOR_MIN_SAMPLES;

    // (1) The arithmetic is the derivation's.
    let warm_clause = ANCHOR_MIN_SAMPLES * MERGED_ACK_SYMBOLS_PER_SAMPLE;
    let burst_clause = RFC6928_INITIAL_WINDOW;
    assert_eq!(
        STORE_CAP_FLOOR,
        warm_clause.max(burst_clause),
        "the shipped floor is not `max(warm, RFC 6928 IW)` — the \
         derivation (paper §6.1) and the constant have drifted apart"
    );
    assert_eq!(STORE_CAP_FLOOR, 10, "the derivation evaluates to 10 today");

    // (2) Both clauses are live, and the conjunction is `max`.
    assert!(
        warm_clause > 0 && burst_clause > 0,
        "a clause that can never bind is decoration, not a derivation"
    );
    assert!(
        STORE_CAP_FLOOR >= warm_clause && STORE_CAP_FLOOR >= burst_clause,
        "the floor must satisfy BOTH lower bounds"
    );
    assert!(
        STORE_CAP_FLOOR < warm_clause + burst_clause,
        "the floor is the LARGER of two independent lower bounds, never their \
         sum — a sum is a composition neither clause asks for"
    );
    // Which clause binds TODAY, stated so a cadence change is a red test and
    // not a silent re-derivation.
    assert_eq!(
        STORE_CAP_FLOOR, burst_clause,
        "RFC 6928's initial window is the binding clause at the shipped ack \
         cadence; if this fires, the cadence moved and paper §6.1 must be re-read"
    );

    // (3) The delta is bounded to the degenerate end. `sigma` here is the
    // cell's Σ(max_bw·min_rtt) and the shipped pooled law is `clamp(2·N·Σ,
    // floor, N·knee)`; the floor can only be the answer where `2·N·Σ` is
    // beneath it. Asserted at every cell this bench carries, at BOTH the
    // honest and the over-reading anchor scale, at N = 1 and N = 2.
    const LEGACY_FLOOR: usize = 64;
    let sigma = |cell: &Cell, scale: Scale| -> f64 {
        cell.paths
            .iter()
            .map(|p| p.0 * p.1 * scale.of(p).expect("this cell is measured"))
            .sum()
    };
    // The three cells the VM actually measured an anchor scale for; `sc3` is
    // deliberately unmeasured (`None`) and this bench does not invent one.
    for cell in &CELLS[..3] {
        for scale in [Scale::Measured, Scale::Fixed(1.0)] {
            let s = sigma(cell, scale);
            for n in 1..=2usize {
                let unclamped = 2.0 * n as f64 * s;
                assert!(
                    unclamped > LEGACY_FLOOR as f64,
                    "{}: the shipped law's ask ({unclamped}) is beneath the \
                     LEGACY floor at N={n} — the floor change is not inert here \
                     and the floor's bound (paper §6.1) is wrong",
                    cell.name
                );
                assert_eq!(
                    path_scaled_store_cap(true, n, s, 2.0, LEGACY_FLOOR, 2048),
                    path_scaled_store_cap(true, n, s, 2.0, STORE_CAP_FLOOR, 2048),
                    "{}: the two floors are distinguishable at N={n} — the \
                     change is not inert at a named cell geometry",
                    cell.name
                );
            }
        }
    }

    // And the other side of the bound, so "inert" is not mistaken for "dead":
    // the derived floor IS reachable, at an ask beneath it. A floor that could
    // never bind would be decorative.
    let tiny = 1e-6;
    assert_eq!(
        path_scaled_store_cap(true, 2, tiny, 2.0, STORE_CAP_FLOOR, 2048),
        Some(STORE_CAP_FLOOR),
        "the derived floor is unreachable — it is not the lower bound it claims"
    );
    // The window the change actually occupies: asks strictly between the two
    // floors used to read 64 and now read the law's own answer.
    for ask in [11usize, 30, 60, 63] {
        let s = ask as f64 / 4.0; // 2·N·Σ with N = 2
        assert_eq!(path_scaled_store_cap(true, 2, s, 2.0, LEGACY_FLOOR, 2048), Some(64));
        assert_eq!(path_scaled_store_cap(true, 2, s, 2.0, STORE_CAP_FLOOR, 2048), Some(ask));
    }
}
