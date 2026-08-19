//! The SENDER CPU DECOMPOSITION instrument (`RWM_CPUPROF`), 2026-08-19.
//!
//! ## Why this exists: a ceiling that is measured and unexplained
//!
//! The c9 scored battery closed with a sender ceiling it established three
//! independent ways and then could not take apart:
//!
//! > CPU-per-payload-byte is INVARIANT across two very different cells: c9
//! > `CPUCLI` 27.38 s / 400 MB = **68.5 ms/MB**; c9h 10.38 s / 150 MB =
//! > **69.2 ms/MB** … 1.51 cores ÷ 68.5 ms/MB = 22.0 MB/s = **176.3 Mbit/s**,
//! > against the **176.4 Mbit/s measured**.
//!
//! The prediction lands within 1 %, so the ceiling is real and the sender's
//! CPU is the binding constraint at a 400 Mbit cell. **What no instrument in
//! this tree can say is where the 68.5 ms/MB GOES.** Every negative result at
//! c9 is confounded by it, and the named successor is "re-run c9 on a sender
//! that can fill four legs" — which nobody can build without knowing which
//! term to attack first.
//!
//! ## The measurand, stated before the code (CLAUDE.md FORMULA-FIRST)
//!
//! The quantity under decomposition is the one the ceiling is computed from:
//!
//! ```text
//!   ms_per_MB = CPUCLI_seconds × 1000 / payload_MB
//! ```
//!
//! `CPUCLI` is `/usr/bin/time -v`'s user+sys for the whole client process.
//! This gauge attributes part of it to named SEAMS. For each seam `s`, over
//! the gauge's own lifetime `[T_arm, T_end]`:
//!
//! ```text
//!   ns[s]     = Σ over entries of (t_exit − t_enter)      MONOTONIC WALL
//!   n[s]      = entry count
//!   cpu_ns    = process CPU consumed in [T_arm, T_end]    CLOCK_PROCESS_CPUTIME_ID
//!   share[s]  = ns[s] / cpu_ns
//!   attr      = Σ share[s]
//!   unattr    = 1 − attr
//! ```
//!
//! **`unattr` IS A FIRST-CLASS READING, NOT AN ERROR TERM.** It is the CPU
//! this process burned outside every instrumented seam, and at this engine
//! that is a NAMED place: quinn's endpoint driver task, where the actual
//! `sendmsg` happens and where rustls/ring applies AEAD packet protection to
//! every datagram. See "what this gauge structurally cannot see" below. A
//! decomposition that reported only the seams would be claiming the residual
//! is small, which is exactly the thing that has never been measured.
//!
//! ## THE THREE HONESTY CLAUSES, stated here rather than discovered later
//!
//! **1. THE SEAMS ARE WALL, THE DENOMINATOR IS CPU.** `Instant::now()` is a
//! ~20 ns vDSO read; `clock_gettime(CLOCK_THREAD_CPUTIME_ID)` is a real
//! syscall at ~0.5–1 µs, and at this engine's symbol rate (≈18 000 sym/s at
//! 176 Mbit/s with MTU-class symbols) two of those per seam per symbol would
//! cost 10–18 % of a core — the instrument would move the number it exists to
//! measure. So the seams are timed with the monotonic clock and the
//! denominator is process CPU. **`share[s]` is therefore "wall spent inside
//! seam s as a fraction of process CPU", and it is only a CPU share to the
//! extent the seam neither blocks nor sleeps.** All five seams are pure
//! compute or a non-blocking handoff; none of them awaits. The `cores` field
//! (`cpu_ns / run_ns`) is printed so a reader can see how far from 1.0 the
//! process is and judge the approximation instead of inheriting it.
//!
//! **2. THE GAUGE'S SPAN IS NOT THE PROCESS'S.** `T_arm` is the first
//! instrumented operation, not process start, so `cpu_ms` EXCLUDES startup
//! (cert generation, TLS setup, the `perf` warm-up object) and any teardown
//! after the sender loop's destructor. `cpu_ms ≤ CPUCLI` always, and the
//! difference is reported by subtraction at the parser rather than hidden:
//! the ledger carries both.
//!
//! **3. TOKIO CAN MIGRATE THE SENDER TASK.** That is why the denominator is
//! `CLOCK_PROCESS_CPUTIME_ID` and not `CLOCK_THREAD_CPUTIME_ID` — a
//! per-thread denominator would silently lose whatever fraction of the task
//! ran on another worker. Process CPU is also exactly what `CPUCLI` measures,
//! so the two are the same quantity and can be compared without a conversion.
//!
//! ## What this gauge structurally CANNOT see, named before the run
//!
//! The seams are all on the sender task. The following sender-side CPU is
//! REAL, is inside `CPUCLI`, and lands in `unattr` by construction:
//!
//!   * **rustls/ring AEAD packet protection** — every datagram is encrypted
//!     and authenticated, in quinn's driver, not here.
//!   * **the actual `sendmsg`/`sendmmsg` syscalls** — `send_datagram` hands a
//!     `Bytes` to quinn's queue and returns; the write to the socket happens
//!     on the endpoint driver task. **`hand` is the HANDOFF, not the
//!     syscall**, and calling it "the send syscall cost" would be wrong.
//!   * **quinn's own framing, pacing, loss detection and ack processing.**
//!   * **tokio's scheduler, and the receiver-side work of the reverse path.**
//!
//! **This is the reason the instrument pair exists.** `perf` sees all of it
//! and this gauge sees none of it; this gauge attributes exactly, and `perf`
//! attributes by sampling into symbols that whole-program LTO has inlined
//! together. Neither subsumes the other, and the pre-registration scores the
//! two against each other rather than promoting one.
//!
//! ## Observation only
//!
//! Structurally, not by promise: this module owns all of its state, its whole
//! input is a seam index and a duration, and the pin
//! `cpuprof_is_observation_only` scrapes this source for any write to an
//! engine handle — the same discipline, and the same forbidden list, as
//! `net::walldiag::tests::walldiag_is_observation_only`.
//!
//! Zero cost with the gate off: [`gauge`] is a `OnceLock<Option<…>>` that
//! resolves to `None`, so every feed site is a null check around a direct
//! call of the closure it wraps — no clock read, no atomic, no branch beyond
//! the null test.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// The instrumented seams, in the order they appear on the `[CPUPROF]` line.
///
/// **They are DISJOINT by construction** — no seam's extent contains
/// another's — which is what makes `attr = Σ share[s]` a sum rather than an
/// over-count. `enc` wraps the three GF coding entry points, which do not
/// call each other; `src` and `frm` are leaf copies; `ser` and `hand` are the
/// two halves of `Transport::send_symbols` and are adjacent, never nested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Seam {
    /// GF(256) coding: `code_generation`, `code_generation_full`,
    /// `generate_repair_range` — coefficient generation, the `mul_acc_slice`
    /// accumulation over the generation span, and the wire-header assembly.
    Enc = 0,
    /// Source admission: `GenerationEncoder::add_source` — the pad
    /// allocation, the payload copy, and the retention-store `insert` (which
    /// is a SECOND full copy of every source symbol).
    Src = 1,
    /// Framing: `framing::frame_window_packet` — the TUN packet's copy into a
    /// `symbol_size` buffer with its 2-byte length prefix.
    Frm = 2,
    /// Wire serialization inside `send_symbols`: `serialize_data_compact`
    /// (the shipped compact v5 path) or `WireMessage::serialize` (bincode).
    Ser = 3,
    /// The datagram HANDOFF to quinn: `send_datagram_shaped`. **Not the send
    /// syscall** — see the module docs.
    Hand = 4,
}

/// The number of seams; the array length every accumulator is sized to.
pub const SEAMS: usize = 5;

/// The seam names as they appear on the `[CPUPROF]` line, indexed by
/// [`Seam`]. An L1 parser is written against these five tokens.
pub const SEAM_NAMES: [&str; SEAMS] = ["enc", "src", "frm", "ser", "hand"];

/// Process CPU consumed so far, ns. `None` where the platform has no
/// process-CPU clock, which is reported as `-` rather than substituted.
#[cfg(target_os = "linux")]
fn proc_cpu_ns() -> Option<u64> {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `clock_gettime` writes only the `timespec` we own and borrow
    // mutably here; the clock id is a compile-time constant. Same shape as
    // `preflight`'s `libc::geteuid()`.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut ts) };
    if rc != 0 {
        return None;
    }
    Some((ts.tv_sec as u64).wrapping_mul(1_000_000_000) + ts.tv_nsec as u64)
}

#[cfg(not(target_os = "linux"))]
fn proc_cpu_ns() -> Option<u64> {
    None
}

/// One run's CPU decomposition. Every field is derived in
/// [`CpuProfGauge::report`]; nothing here is a threshold or a
/// classification — the pre-registration decides what "a top-two cost" means.
#[derive(Debug, Clone, PartialEq)]
pub struct CpuProfReading {
    /// Monotonic wall span of the gauge's life, ms (`T_end − T_arm`).
    pub run_ms: f64,
    /// Process CPU consumed over that span, ms. `None` where the platform
    /// has no process-CPU clock.
    pub cpu_ms: Option<f64>,
    /// `cpu_ms / run_ms` — the process's mean core occupancy over the span.
    /// The number the ceiling arithmetic calls "1.51 cores".
    pub cores: Option<f64>,
    /// Per-seam accumulated wall, ms.
    pub seam_ms: [f64; SEAMS],
    /// Per-seam entry count.
    pub seam_n: [u64; SEAMS],
    /// Per-seam share of process CPU. `None` with `cpu_ms`.
    pub seam_share: [Option<f64>; SEAMS],
    /// `Σ seam_share` — the fraction of process CPU this gauge attributes.
    pub attr: Option<f64>,
}

impl CpuProfReading {
    /// `1 − attr`: the process CPU spent outside every instrumented seam.
    /// **A reading, not an error term** — see the module docs.
    pub fn unattr(&self) -> Option<f64> {
        self.attr.map(|a| 1.0 - a)
    }
}

/// The CPU-decomposition gauge. One per process; the fields are atomics so
/// the teardown report can be taken from the same handle without threading a
/// `&mut` through every seam.
#[derive(Debug)]
pub struct CpuProfGauge {
    /// `T_arm` — the monotonic instant of gauge construction (the first
    /// instrumented operation, since construction is lazy at first touch).
    armed_at: Instant,
    /// Process CPU at `T_arm`, ns. `u64::MAX` = unavailable on this platform.
    armed_cpu_ns: u64,
    /// Σ wall inside each seam, ns.
    ns: [AtomicU64; SEAMS],
    /// Entries into each seam.
    n: [AtomicU64; SEAMS],
}

impl Default for CpuProfGauge {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuProfGauge {
    pub fn new() -> Self {
        CpuProfGauge {
            armed_at: Instant::now(),
            armed_cpu_ns: proc_cpu_ns().unwrap_or(u64::MAX),
            ns: std::array::from_fn(|_| AtomicU64::new(0)),
            n: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    /// Charge one completed seam entry. The ONLY mutation this module offers.
    pub fn charge(&self, seam: Seam, ns: u64) {
        let i = seam as usize;
        self.ns[i].fetch_add(ns, Ordering::Relaxed);
        self.n[i].fetch_add(1, Ordering::Relaxed);
    }

    /// The run's reading. `None` when the gauge has no wall-clock span at all
    /// — the honest answer rather than a 0/0.
    pub fn report(&self) -> Option<CpuProfReading> {
        let run_ns = self.armed_at.elapsed().as_nanos() as u64;
        if run_ns == 0 {
            return None;
        }
        let cpu_ns = if self.armed_cpu_ns == u64::MAX {
            None
        } else {
            proc_cpu_ns().map(|now| now.saturating_sub(self.armed_cpu_ns))
        };
        // A zero CPU delta is not a share denominator. It is reported as
        // "no CPU clock" rather than as an infinity.
        let cpu_ns = cpu_ns.filter(|c| *c > 0);

        let mut seam_ms = [0.0f64; SEAMS];
        let mut seam_n = [0u64; SEAMS];
        let mut seam_share: [Option<f64>; SEAMS] = [None; SEAMS];
        let mut attr = cpu_ns.map(|_| 0.0f64);
        for i in 0..SEAMS {
            let ns = self.ns[i].load(Ordering::Relaxed);
            seam_ms[i] = ns as f64 / 1e6;
            seam_n[i] = self.n[i].load(Ordering::Relaxed);
            if let Some(c) = cpu_ns {
                let s = ns as f64 / c as f64;
                seam_share[i] = Some(s);
                attr = attr.map(|a| a + s);
            }
        }
        Some(CpuProfReading {
            run_ms: run_ns as f64 / 1e6,
            cpu_ms: cpu_ns.map(|c| c as f64 / 1e6),
            cores: cpu_ns.map(|c| c as f64 / run_ns as f64),
            seam_ms,
            seam_n,
            seam_share,
            attr,
        })
    }
}

/// Process-global gauge, resolved once at first touch. `None` — and therefore
/// no state, no atomic and no clock read at any seam — unless
/// `RWM_CPUPROF=1`.
///
/// `RWM_CPUPROF` (default OFF, DIAG-surface, ADR-0052 class): the sender CPU
/// decomposition. Independent of `RWM_DIAG`, exactly as `RWM_WALLDIAG` and
/// `RWM_ACKDIAG` are — the cell whose ceiling this takes apart (c9, the
/// symmetric quad) is sender-CPU-bound, and adding the 250 ms `[DIAG]` report
/// to the arm under measurement would change the quantity being measured.
pub fn gauge() -> Option<&'static CpuProfGauge> {
    static G: std::sync::OnceLock<Option<CpuProfGauge>> = std::sync::OnceLock::new();
    G.get_or_init(|| {
        if crate::config::env_flag("RWM_CPUPROF", false) {
            Some(CpuProfGauge::new())
        } else {
            None
        }
    })
    .as_ref()
}

/// Time `f` into `seam`. **This is the only feed site shape in the tree**, so
/// a seam cannot be half-instrumented (an enter with no exit) by
/// construction: the extent is the closure's.
///
/// With the gate OFF this is `f()` behind one null check — no clock read, no
/// atomic, and `#[inline]` lets the branch fold at every call site.
#[inline]
pub fn timed<T>(seam: Seam, f: impl FnOnce() -> T) -> T {
    match gauge() {
        None => f(),
        Some(g) => {
            let t0 = Instant::now();
            let out = f();
            g.charge(seam, t0.elapsed().as_nanos() as u64);
            out
        }
    }
}

/// Render the run's ONE `[CPUPROF]` line. Split from the emitter so the unit
/// pins assert the STRING an L1 parser will scrape, not a side effect.
///
/// Seam tokens are `<name>=<ms>/n<count>/<share>`, following the `sig_us=`
/// convention — the value, then the evidence about it. An unavailable share
/// renders `-`, never `0.0000`: a parser must be able to tell "no CPU clock"
/// from "this seam cost nothing".
pub fn report_line(r: &CpuProfReading) -> String {
    let f1 = |v: Option<f64>| v.map_or("-".to_string(), |x| format!("{x:.1}"));
    let f3 = |v: Option<f64>| v.map_or("-".to_string(), |x| format!("{x:.3}"));
    let f4 = |v: Option<f64>| v.map_or("-".to_string(), |x| format!("{x:.4}"));
    let mut s = format!(
        "[CPUPROF] run_ms={:.1} cpu_ms={} cores={}",
        r.run_ms,
        f1(r.cpu_ms),
        f3(r.cores)
    );
    for i in 0..SEAMS {
        s.push_str(&format!(
            " {}={:.1}/n{}/{}",
            SEAM_NAMES[i],
            r.seam_ms[i],
            r.seam_n[i],
            f4(r.seam_share[i])
        ));
    }
    s.push_str(&format!(" attr={} unattr={}", f4(r.attr), f4(r.unattr())));
    s
}

/// Emit the run's ONE `[CPUPROF]` line, at sender teardown.
///
/// `eprintln!` rather than `tracing::info!`, matching the `[WALL]` and
/// `[ACKDIAG]` siblings: an instrument's line must not be filterable away by
/// a subscriber level the battery driver does not control.
pub(crate) fn report_at_teardown() {
    let Some(g) = gauge() else { return };
    let Some(r) = g.report() else { return };
    eprintln!("{}", report_line(&r));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gate ships OFF, so the gauge is absent and every seam is a null
    /// check. (Set-env semantics are `config::env_flag`'s.)
    #[test]
    fn the_gauge_is_absent_on_the_shipped_default() {
        if std::env::var("RWM_CPUPROF").is_ok() {
            return;
        }
        assert!(
            gauge().is_none(),
            "RWM_CPUPROF ships default OFF: the gauge must not exist"
        );
    }

    /// And with the gauge absent, `timed` is transparent: it returns the
    /// closure's value and charges nothing. This is the property that makes
    /// the instrument free on every shipped arm.
    #[test]
    fn timed_is_transparent_with_the_gate_off() {
        if std::env::var("RWM_CPUPROF").is_ok() {
            return;
        }
        let mut side = 0u32;
        let v = timed(Seam::Enc, || {
            side += 1;
            41 + 1
        });
        assert_eq!(v, 42, "timed must return the closure's value verbatim");
        assert_eq!(side, 1, "the closure runs exactly once");
    }

    /// The scrapeable line's SHAPE, pinned absolutely: an L1 parser is
    /// written against these tokens and their formats, and a silent rename
    /// here would leave the parser reading zeros.
    #[test]
    fn the_cpuprof_line_is_the_scrapeable_token_set() {
        let r = CpuProfReading {
            run_ms: 18_140.0,
            cpu_ms: Some(27_380.0),
            cores: Some(1.5094),
            seam_ms: [6845.0, 2738.0, 1369.0, 821.4, 547.6],
            seam_n: [333_333, 333_333, 333_333, 666_666, 666_666],
            seam_share: [Some(0.25), Some(0.1), Some(0.05), Some(0.03), Some(0.02)],
            attr: Some(0.45),
        };
        assert_eq!(
            report_line(&r),
            "[CPUPROF] run_ms=18140.0 cpu_ms=27380.0 cores=1.509 \
             enc=6845.0/n333333/0.2500 src=2738.0/n333333/0.1000 \
             frm=1369.0/n333333/0.0500 ser=821.4/n666666/0.0300 \
             hand=547.6/n666666/0.0200 attr=0.4500 unattr=0.5500"
        );
    }

    /// **THE `-` CLAUSE.** A platform with no process-CPU clock must render
    /// `-` for every derived share — never `0.0000`, which a parser would
    /// average into a results table as "this seam is free".
    #[test]
    fn an_absent_cpu_clock_renders_dashes_and_never_zeroes() {
        let r = CpuProfReading {
            run_ms: 1000.0,
            cpu_ms: None,
            cores: None,
            seam_ms: [1.0, 2.0, 3.0, 4.0, 5.0],
            seam_n: [1, 2, 3, 4, 5],
            seam_share: [None; SEAMS],
            attr: None,
        };
        let line = report_line(&r);
        assert_eq!(
            line,
            "[CPUPROF] run_ms=1000.0 cpu_ms=- cores=- enc=1.0/n1/- src=2.0/n2/- \
             frm=3.0/n3/- ser=4.0/n4/- hand=5.0/n5/- attr=- unattr=-"
        );
        assert!(
            !line.contains("=0.0000"),
            "an unavailable share must never render as a zero share: {line}"
        );
    }

    /// The accumulator is a SUM over entries and the count is its own
    /// denominator, so a mean ns/call is computable from the line alone.
    #[test]
    fn charges_accumulate_per_seam_with_their_own_counts() {
        let g = CpuProfGauge::new();
        for _ in 0..10 {
            g.charge(Seam::Enc, 1_000_000); // 1 ms
        }
        g.charge(Seam::Hand, 500_000); // 0.5 ms
        let r = g.report().expect("a live gauge has a span");
        assert!(
            (r.seam_ms[Seam::Enc as usize] - 10.0).abs() < 1e-9,
            "ten 1 ms charges are 10 ms: {}",
            r.seam_ms[Seam::Enc as usize]
        );
        assert_eq!(r.seam_n[Seam::Enc as usize], 10);
        assert!((r.seam_ms[Seam::Hand as usize] - 0.5).abs() < 1e-9);
        assert_eq!(r.seam_n[Seam::Hand as usize], 1);
        assert_eq!(r.seam_n[Seam::Src as usize], 0, "an unfed seam reads zero");
    }

    /// **THE DISJOINTNESS INVARIANT, as arithmetic.** `attr` is the SUM of
    /// the five shares, so a reader can add the printed columns and get the
    /// printed total. If a future seam were nested inside another this
    /// identity would still hold numerically while the shares over-counted —
    /// which is why disjointness is argued at [`Seam`] and pinned at the
    /// feed sites, not asserted here. What IS asserted here is that the
    /// printed `attr` is not independently computed.
    #[test]
    fn attr_is_exactly_the_sum_of_the_printed_shares() {
        let r = CpuProfReading {
            run_ms: 100.0,
            cpu_ms: Some(100.0),
            cores: Some(1.0),
            seam_ms: [10.0, 20.0, 5.0, 1.0, 4.0],
            seam_n: [1; SEAMS],
            seam_share: [Some(0.10), Some(0.20), Some(0.05), Some(0.01), Some(0.04)],
            attr: Some(0.40),
        };
        let sum: f64 = r.seam_share.iter().flatten().sum();
        assert!((sum - r.attr.unwrap()).abs() < 1e-12);
        assert!((r.unattr().unwrap() - 0.60).abs() < 1e-12);
    }

    /// Shares are computed against PROCESS CPU, not against the wall span —
    /// the distinction the module docs turn on. A gauge whose seams consumed
    /// half the CPU over a span twice as long must report share 0.5, not
    /// 0.25.
    #[test]
    fn shares_are_taken_against_cpu_and_not_against_the_wall() {
        let g = CpuProfGauge::new();
        if g.armed_cpu_ns == u64::MAX {
            return; // no process-CPU clock on this platform; nothing to assert
        }
        // Burn a little CPU so the denominator is nonzero and measurable.
        let mut acc = 0u64;
        let t0 = Instant::now();
        while t0.elapsed().as_millis() < 20 {
            acc = acc.wrapping_add(1);
        }
        assert!(acc > 0);
        let r = g.report().expect("a live gauge has a span");
        let cpu = r.cpu_ms.expect("the clock was available at arm time");
        assert!(cpu > 0.0, "a spinning process consumed CPU: {cpu}");
        assert!(
            r.cores.expect("cores accompanies cpu_ms") > 0.0,
            "cores must be positive on a spinning process"
        );
        // Nothing was charged, so the whole of it is unattributed — which is
        // the correct reading and the one the module insists is first-class.
        assert_eq!(r.attr, Some(0.0));
        assert_eq!(r.unattr(), Some(1.0));
    }

    /// **THE BEHAVIOUR-NEUTRALITY PIN**, structural rather than promised —
    /// the same scrape, and the same forbidden list, as
    /// `net::walldiag::tests::walldiag_is_observation_only`. The failure mode
    /// is someone LATER adding a convenient write here, and it has no runtime
    /// symptom to assert on.
    #[test]
    fn cpuprof_is_observation_only() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/net/cpuprof.rs"),
        )
        .expect("read src/net/cpuprof.rs");
        let src = &src[..src.find("#[cfg(test)]").expect("the test module marker")];
        let code: String = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//") && !l.trim_start().starts_with("//!"))
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
                "the CPU-decomposition gauge must not touch engine state: found `{forbidden}`"
            );
        }
        // Stronger than walldiag's pin: this gauge takes no engine value at
        // all — its whole input is a seam index and a duration.
        assert!(
            !code.contains("Arc<"),
            "the CPU gauge holds no engine handle; its input is a seam and a duration"
        );
    }

    /// The seam name table and the enum cannot drift apart: a seam added to
    /// one and not the other would silently shift every parser column.
    #[test]
    fn every_seam_has_a_name_and_the_indices_are_dense() {
        assert_eq!(SEAM_NAMES.len(), SEAMS);
        for (i, s) in [Seam::Enc, Seam::Src, Seam::Frm, Seam::Ser, Seam::Hand]
            .into_iter()
            .enumerate()
        {
            assert_eq!(s as usize, i, "seam discriminants must be dense from 0");
        }
    }
}
