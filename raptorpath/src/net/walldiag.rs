//! The dead-wall onset/duration instrument (`RWM_WALLDIAG`).
//!
//! It measures the sender's terminal dead wall by its onset and duration,
//! not by a tick-share. A tick-share (the fraction of sender-loop wakeups on
//! the TUN or paused arm) is unstable: the loop's wakeup rate is an output of
//! the mechanism under test, so two runs with the same 400 ms terminal wall
//! read very different shares if one woke 50× more often before the wall
//! began. And a median over the whole run cannot distinguish one long
//! terminal wall from a hundred scattered micro-gaps totalling the same time.
//!
//! Onset and duration have neither defect: they are wall-clock quantities of
//! one named event (the contiguous terminal window), defined per run rather
//! than per tick, and a run with no wall reports duration ≈ 0.
//!
//! ## The measurand
//!
//! Let `T_start` be the first sender-loop iteration and `T_end` the sender's
//! teardown. Call an instant PRODUCTIVE when the sender is doing any of the
//! three things whose ABSENCE defines the wall:
//!
//! ```text
//!   productive(t)  ⟺  the loop woke on the TUN arm      (reading source)
//!                  ∨   the loop woke on the PAUSED arm   (cap-paused)
//!                  ∨   `last_source_send_us` advanced    (NEW data on the wire)
//! ```
//!
//! Then, with `T_prod` = the last productive instant,
//!
//! ```text
//!   duration_ms = (T_end − T_prod) / 1000
//!   onset       = (T_prod − T_start) / (T_end − T_start)     ∈ [0, 1]
//!   retx        = retransmits fired in [T_prod, T_end]
//! ```
//!
//! `onset` is a fraction of the transfer wall so it is comparable across
//! cells whose absolute durations differ by an order of magnitude.
//! `duration_ms` is absolute because a wall is bad in milliseconds, not in
//! percent.
//!
//! The third disjunct is new source only. A retransmit is not new data, which
//! is why retransmits are counted inside the window: a sender that spends its
//! last 400 ms serving holes and nothing else is walled by this definition,
//! and `retx` tells a recovery tail from a hang.
//!
//! Contiguity is free: `T_prod` is a running maximum, so the reported window
//! is by construction the maximal contiguous suffix containing no productive
//! instant. No histogram, no threshold, no scan.
//!
//! ## Resolution
//!
//! The instrument samples once per sender-loop iteration, so `T_prod` is
//! quantized to the loop's wakeup granularity. The two poll arms that can
//! idle the loop (`paused`, `pace`) both sleep 1 ms, so a reported duration
//! over-states the true wall by at most one loop period. The `it_ms` field
//! carries the observed mean iteration period so that bound is read off every
//! run.
//!
//! ## Observation only
//!
//! This module owns all of its state, and `walldiag_is_observation_only`
//! scrapes this source for any write to an engine handle (the same forbidden
//! list as `net::ackdiag::tests::ackdiag_is_observation_only`).
//!
//! Zero cost with the gate off: [`gauge`] is a `OnceLock<Option<…>>` that
//! resolves to `None`, so the single feed site is a null check.

use std::sync::atomic::{AtomicU64, Ordering};

/// The sender-loop wait arm that carries productive source intake
/// (`net/mod.rs`: `wait_arm = 0`, the `tun.read_packet()` arm).
pub const ARM_TUN: usize = 0;
/// The sender-loop wait arm that carries store-cap backpressure
/// (`net/mod.rs`: `wait_arm = 1`, the 1 ms `tx_paused` poll).
pub const ARM_PAUSED: usize = 1;

/// One run's dead-wall reading. All fields are derived in [`DeadWallGauge::report`];
/// nothing here is a threshold or a classification — the caller (and the L1
/// parser) decides what counts as a wall.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallReading {
    /// Whole-transfer wall time, ms (`T_end − T_start`).
    pub total_ms: f64,
    /// The terminal window's onset as a fraction of the transfer wall.
    /// 1.0 = the sender was productive right up to teardown (no wall).
    pub onset: f64,
    /// The terminal window's duration, ms.
    pub duration_ms: f64,
    /// Retransmits fired inside the terminal window.
    pub retx: u64,
    /// Mean sender-loop iteration period over the run, ms — the resolution
    /// bound on `duration_ms`, reported rather than assumed.
    pub it_ms: f64,
}

/// The dead-wall gauge. One per process; the sender loop is its only writer,
/// but the fields are atomics so the teardown report can be taken from the
/// same handle without threading a `&mut` through the loop's exit arms.
#[derive(Debug)]
pub struct DeadWallGauge {
    /// First observed iteration (µs). 0 = the gauge has never been fed.
    start_us: AtomicU64,
    /// Last productive instant (µs) — the running maximum of the definition.
    last_prod_us: AtomicU64,
    /// The retransmit total as of `last_prod_us`.
    retx_at_prod: AtomicU64,
    /// The live retransmit total, transcribed from the engine's own counter.
    retx_total: AtomicU64,
    /// The highest `last_source_send_us` seen, for the "advanced" test.
    src_seen_us: AtomicU64,
    /// Iterations observed, for the resolution bound.
    iters: AtomicU64,
    /// Last observed instant (µs) — the report's `T_end` when teardown does
    /// not carry its own clock read.
    last_us: AtomicU64,
}

impl Default for DeadWallGauge {
    fn default() -> Self {
        Self::new()
    }
}

impl DeadWallGauge {
    pub fn new() -> Self {
        DeadWallGauge {
            start_us: AtomicU64::new(0),
            last_prod_us: AtomicU64::new(0),
            retx_at_prod: AtomicU64::new(0),
            retx_total: AtomicU64::new(0),
            src_seen_us: AtomicU64::new(0),
            iters: AtomicU64::new(0),
            last_us: AtomicU64::new(0),
        }
    }

    /// Feed one sender-loop iteration. `wait_arm` is the arm that woke the
    /// loop (`usize::MAX` when no arm did), `last_source_send_us` is
    /// `SenderState::last_source_send_us` (the wall clock of the last new
    /// source symbol), `retx_total` the engine's monotone retransmit counter.
    ///
    /// The first call establishes `T_start` and seeds the productive stamp:
    /// a transfer is productive at its own start by definition, so a run that
    /// never sends anything reports a wall covering the whole run rather than
    /// a division by zero.
    pub fn observe(
        &self,
        now_us: u64,
        wait_arm: usize,
        last_source_send_us: u64,
        retx_total: u64,
    ) {
        self.retx_total.store(retx_total, Ordering::Relaxed);
        self.last_us.store(now_us, Ordering::Relaxed);
        self.iters.fetch_add(1, Ordering::Relaxed);
        if self.start_us.load(Ordering::Relaxed) == 0 {
            self.start_us.store(now_us.max(1), Ordering::Relaxed);
            self.src_seen_us.store(last_source_send_us, Ordering::Relaxed);
            self.mark_productive(now_us, retx_total);
            return;
        }
        // The three disjuncts of `productive(t)`, all evaluated — no early
        // exit, so the source stamp advances even on a productive wait arm.
        let src_advanced = last_source_send_us
            > self.src_seen_us.fetch_max(last_source_send_us, Ordering::Relaxed);
        if wait_arm == ARM_TUN || wait_arm == ARM_PAUSED || src_advanced {
            self.mark_productive(now_us, retx_total);
        }
    }

    fn mark_productive(&self, now_us: u64, retx_total: u64) {
        self.last_prod_us.store(now_us, Ordering::Relaxed);
        self.retx_at_prod.store(retx_total, Ordering::Relaxed);
    }

    /// The run's reading. `end_us` is teardown's own clock read; `None`
    /// before the gauge has been fed, or when the run has no wall-clock span
    /// (`T_end == T_start`), rather than a 0/0.
    pub fn report(&self, end_us: u64) -> Option<WallReading> {
        let start = self.start_us.load(Ordering::Relaxed);
        if start == 0 {
            return None;
        }
        let end = end_us.max(self.last_us.load(Ordering::Relaxed));
        let total_us = end.saturating_sub(start);
        if total_us == 0 {
            return None;
        }
        let prod = self.last_prod_us.load(Ordering::Relaxed).clamp(start, end);
        let iters = self.iters.load(Ordering::Relaxed).max(1);
        Some(WallReading {
            total_ms: total_us as f64 / 1000.0,
            onset: prod.saturating_sub(start) as f64 / total_us as f64,
            duration_ms: end.saturating_sub(prod) as f64 / 1000.0,
            retx: self
                .retx_total
                .load(Ordering::Relaxed)
                .saturating_sub(self.retx_at_prod.load(Ordering::Relaxed)),
            it_ms: total_us as f64 / iters as f64 / 1000.0,
        })
    }
}

/// Process-global gauge, resolved once at first touch. `None` — and therefore
/// no state, no atomic and no clock read at the feed site — unless
/// `RWM_WALLDIAG=1`.
///
/// `RWM_WALLDIAG` (default OFF, instrument): independent of `RWM_DIAG`, as
/// `RWM_ACKDIAG` is, so it can run on arms that cannot afford the 250 ms
/// `[DIAG]` report; the `[WALL]` line is separately scrapeable.
pub fn gauge() -> Option<&'static DeadWallGauge> {
    static G: std::sync::OnceLock<Option<DeadWallGauge>> = std::sync::OnceLock::new();
    G.get_or_init(|| {
        if crate::gates::get().walldiag {
            Some(DeadWallGauge::new())
        } else {
            None
        }
    })
    .as_ref()
}

/// Render the run's one `[WALL]` line. Split from the emitter so the unit
/// pins assert the string an L1 parser will scrape, not a side effect.
pub fn report_line(r: WallReading) -> String {
    format!(
        "[WALL] onset={:.4} dur_ms={:.1} retx={} total_ms={:.1} it_ms={:.3}",
        r.onset, r.duration_ms, r.retx, r.total_ms, r.it_ms
    )
}

/// Emit the run's one `[WALL]` line, at sender teardown. Idempotent by the
/// caller's construction (each teardown arm returns immediately after).
///
/// `eprintln!` rather than `tracing::info!`, matching the `[ACKDIAG]` sibling:
/// an instrument's line must not be filterable away by a subscriber level the
/// battery driver does not control, and the L1 drivers scrape the merged
/// stream either way.
pub(crate) fn report_at_teardown(end_us: u64) {
    let Some(g) = gauge() else { return };
    let Some(r) = g.report(end_us) else { return };
    eprintln!("{}", report_line(r));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gate ships OFF, so the gauge is absent and the feed site is a null
    /// check. (Set-env semantics are `config::env_flag`'s.)
    #[test]
    fn the_gauge_is_absent_on_the_shipped_default() {
        // NOTE: relies on the test env not exporting RWM_WALLDIAG — the same
        // assumption every engine-default test in this crate makes.
        if std::env::var("RWM_WALLDIAG").is_ok() {
            return;
        }
        assert!(
            gauge().is_none(),
            "RWM_WALLDIAG ships default OFF: the gauge must not exist"
        );
    }

    /// The scrapeable line's shape, pinned absolutely: an L1 parser is written
    /// against these five keys and their formats, and a silent rename here
    /// would leave the parser reading zeros.
    #[test]
    fn the_wall_line_is_the_five_scrapeable_keys() {
        let line = report_line(WallReading {
            total_ms: 1234.5,
            onset: 0.5,
            duration_ms: 617.25,
            retx: 7,
            it_ms: 1.0,
        });
        assert_eq!(
            line,
            "[WALL] onset=0.5000 dur_ms=617.2 retx=7 total_ms=1234.5 it_ms=1.000"
        );
    }

    /// The behaviour-neutrality pin, structural rather than promised — the
    /// same scrape, and the same forbidden list, as
    /// `net::ackdiag::tests::ackdiag_is_observation_only`. The failure mode is
    /// a later convenient write here, which has no runtime symptom.
    #[test]
    fn walldiag_is_observation_only() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/net/walldiag.rs"),
        )
        .expect("read src/net/walldiag.rs");
        let src = &src[..src.find("#[cfg(test)]").expect("the test module marker")];
        let code: String = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in [
            "path_mut",
            "&mut Scheduler",
            "scheduler",
            "set_cc_window_bytes",
            "release_in_flight",
            "charge_in_flight",
            "record_delivery(",
            "on_delivery_signal",
        ] {
            assert!(
                !code.contains(forbidden),
                "the dead-wall gauge must not touch engine state: found `{forbidden}`"
            );
        }
        // Stronger than ackdiag's pin: this gauge takes no engine handle at
        // all — its whole input is four scalars passed by value.
        assert!(
            !code.contains("Arc<"),
            "the dead-wall gauge holds no engine handle; its input is four scalars"
        );
    }

    /// A clean run — productive right up to teardown — reports duration ≈ 0
    /// and onset ≈ 1. Pinned on injected instants so the loopback's job is
    /// routing only.
    #[test]
    fn a_clean_run_reports_no_terminal_wall() {
        let g = DeadWallGauge::new();
        let mut src = 1_000u64;
        for i in 0..100u64 {
            let t = 1_000_000 + i * 1_000;
            src = t; // new data every iteration
            g.observe(t, ARM_TUN, src, 0);
        }
        let r = g.report(1_000_000 + 99_000).expect("a fed gauge reports");
        assert_eq!(r.duration_ms, 0.0, "a productive-to-teardown run has no wall");
        assert!((r.onset - 1.0).abs() < 1e-12, "onset must be 1.0: {}", r.onset);
        assert_eq!(r.retx, 0);
        assert!((r.it_ms - 1.0).abs() < 0.05, "iteration period ≈ 1 ms: {}", r.it_ms);
    }

    /// A terminal wall: productive for the first half, then 500 ms of arms
    /// that are neither intake nor cap-pause, with no new source and four
    /// retransmits fired inside the window.
    #[test]
    fn a_terminal_wall_reports_its_onset_duration_and_retransmits() {
        let g = DeadWallGauge::new();
        let mut src;
        for i in 0..500u64 {
            let t = 1_000_000 + i * 1_000;
            src = t;
            g.observe(t, ARM_TUN, src, 0);
        }
        let frozen = 1_000_000 + 499_000; // last new source send
        for i in 500..1_000u64 {
            let t = 1_000_000 + i * 1_000;
            // arm 4 = a gap report arrived: not intake, not cap-pause.
            // One retransmit per 100 ms of the wall, four in all.
            let retx = if i >= 600 { ((i - 600) / 100 + 1).min(4) } else { 0 };
            g.observe(t, 4, frozen, retx);
        }
        let r = g.report(1_000_000 + 999_000).expect("a fed gauge reports");
        assert!(
            (r.duration_ms - 500.0).abs() < 1.5,
            "the terminal window is ~500 ms: {}",
            r.duration_ms
        );
        assert!(
            (r.onset - 0.5).abs() < 0.01,
            "the wall opens at half the transfer: {}",
            r.onset
        );
        assert_eq!(r.retx, 4, "all four retransmits fired inside the window");
        assert!((r.total_ms - 999.0).abs() < 1.5);
    }

    /// Contiguity: scattered micro-gaps are not a terminal wall — the
    /// discrimination a tick-share statistic cannot make. The same total
    /// non-productive time as the test above, chopped into 1-iteration gaps,
    /// must report duration ≈ 0.
    #[test]
    fn scattered_gaps_are_not_a_terminal_wall() {
        let g = DeadWallGauge::new();
        for i in 0..1_000u64 {
            let t = 1_000_000 + i * 1_000;
            if i % 2 == 0 {
                g.observe(t, 4, 1_000, 0); // idle arm, no new source
            } else {
                g.observe(t, ARM_TUN, 1_000 + t, 0);
            }
        }
        let r = g.report(1_000_000 + 999_000).expect("a fed gauge reports");
        assert!(
            r.duration_ms <= 1.5,
            "half the ticks were idle but none of them terminal: {}",
            r.duration_ms
        );
        assert!(r.onset > 0.99, "onset ≈ 1: {}", r.onset);
    }

    /// The cap-paused arm is productive by definition — a sender blocked on
    /// its own store cap is capped, not walled, and the two are kept apart.
    #[test]
    fn cap_paused_is_not_a_wall() {
        let g = DeadWallGauge::new();
        for i in 0..200u64 {
            g.observe(1_000_000 + i * 1_000, ARM_PAUSED, 1_000, 0);
        }
        let r = g.report(1_000_000 + 199_000).expect("a fed gauge reports");
        assert_eq!(r.duration_ms, 0.0, "cap-paused is not the dead wall");
    }

    /// An unfed gauge has no reading, and a zero-span run has none either,
    /// rather than a 0/0.
    #[test]
    fn an_unfed_or_zero_span_gauge_reports_nothing() {
        let g = DeadWallGauge::new();
        assert_eq!(g.report(1_000_000), None);
        g.observe(1_000_000, ARM_TUN, 0, 0);
        assert_eq!(g.report(1_000_000), None, "a zero-span run has no reading");
    }
}
