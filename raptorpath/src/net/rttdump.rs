//! THE RAW RTT SAMPLE DUMP (`RWM_RTT_DUMP`, default OFF, observation only) —
//! the instrument that makes the estimator battery's CLAUSE `B` EXACT.
//!
//! WHY IT EXISTS — the defect it repairs, in the words of the record that
//! found it.
//!
//! The scored estimator battery (goal-gate, "THE SIGMA ESTIMATOR — THE SCORED
//! RESULT" §7) rejected three of four candidates on clause `B` and then
//! recorded that the rejection's own reference was the part of the bar most in
//! need of scrutiny:
//!
//! > *"A uniform 30–90× gap across ALL FOUR gauges, including the estimator
//! > this tree has shipped and trusted for its whole history, is not four
//! > independent biases; it is one property of the COMPARISON."*
//!
//! Clause `B`'s reference was `latt_probe.py`, a 20 Hz ICMP probe. Its own
//! docstring names the mismatch: the probe measures delivered round-trip time
//! *"through the WHOLE shaped path — netem's fixed delay, its jitter, its rate
//! serialization, ITS queue, and our own bytes sitting in front of the probe"*,
//! while `sig_us` measures the sender's smoothed estimate of its own ack path.
//! **These are different quantities**, so `B` was written REJECT-only —
//! it could convict and could never acquit. And for `msd_us`, the one
//! candidate that came near the bar, `B` could not be evaluated at all: the
//! probe samples at 20 Hz and the sender at kHz, and comparing a lag-dependent
//! statistic across a 500× sampling-rate gap is not a comparison. The battery
//! closed with the consequence stated plainly: *"nobody can currently say
//! whether `msd` is measuring the right quantity at all … `msd`'s 90–100 %
//! level gap against `sig_us` is unexplained, and 'unexplained' is not 'fine'."*
//!
//! WHAT THIS CHANGES — the question `B` asks, made answerable EXACTLY.
//!
//! Clause `B` exists to catch **an estimator that is stable because it
//! measures something smaller.** That question is about the estimator against
//! **the sample stream it consumes** — not about the sender's RTT against some
//! other path's latency. This gauge emits that sample stream. With it, each
//! candidate's online reading is compared against **the same functional
//! computed offline over the identical samples**, so the reference is exact by
//! construction, like-for-like by construction, and free of the lower-bound
//! asymmetry that made `B` REJECT-only. **The rebuilt `B` can therefore
//! ACQUIT.**
//!
//! It also settles the question the battery had to leave open, in one table:
//! every functional evaluated offline on ONE sample stream says exactly how
//! much of the 90–100× level gap between `msd_us` and `sig_us` is the
//! functionals differing and how much is anything else.
//!
//! **AND THIS IS A NARROWING, RECORDED AS ONE.** The rebuilt `B` asks whether
//! an estimator faithfully computes its functional over its own input. It does
//! **not** ask whether that input is the true delivered latency — the question
//! the probe was reaching for and answering badly. **The missing instrument
//! the battery named — a delivered-latency probe at the sender's own sample
//! rate — is still missing, and this module does not build it.**
//!
//! WHY THE DUMP PASS MUST BE SEPARATE FROM THE SCORED BATTERY.
//!
//! At a sender leg running tens of kHz this gauge writes megabytes of stderr
//! during the run. That is a CPU and I/O cost on the sender, and sender-side
//! dispersion is precisely the quantity clause `S` measures. **Running the
//! dump on the scored invocations would perturb the measurement it is there to
//! explain**, so the pre-registration amendment scores `S` and `C` on the
//! dump-OFF battery and `B` on a separate dump-ON pass, and discloses the
//! dump-ON pass's own `S` readings so the perturbation is visible rather than
//! assumed absent.
//!
//! THE FORMAT, and how truncation is made READABLE rather than silent.
//!
//! ```text
//!   [RTTDUMP] p=<path> t0=<µs since gauge epoch> n=<count> d=<dt,rtt;dt,rtt;…>
//! ```
//!
//! One line per `BATCH` samples per path, so the per-sample cost is a push
//! into a string rather than a write. Each batch is SELF-CONTAINED: `t0` is
//! the absolute stamp of its first sample and every `dt` is a delta from the
//! previous sample **within the batch**, with the first `dt` always 0. Both
//! `dt` and `rtt` are µs. Offline reconstruction is
//! `t_k = t0 + Σ_{i≤k} dt_i`, exactly, with no cross-batch state.
//!
//! Three bounded losses, each declared here and each detectable off the run's
//! own output:
//!
//!   1. **The tail partial batch.** Fewer than `BATCH` samples may be pending
//!      at end of run and are never written. Bounded by `BATCH − 1 = 255`
//!      samples per path per run.
//!   2. **The cap.** At most `RWM_RTT_DUMP_MAX` samples per path are dumped,
//!      as a contiguous PREFIX of the run. When it binds, one
//!      `[RTTDUMP-CAP]` line is printed, once, naming the count.
//!   3. Both are checkable against the gauges' own denominators: the final
//!      `[DIAG]` block's `sig_us=…/n<count>` is the number of samples the
//!      estimators saw, so `dumped / n` is the dump's own coverage and the
//!      parser reports it rather than assuming it is 1.
//!
//! **A PREFIX, AND THE CAVEAT IS STATED.** Truncation keeps the run's early
//! samples, not a random subset — a contiguous prefix is required because the
//! functionals under test are successive differences and a decimated sample
//! set would change the very lag the estimand is defined at. The consequence
//! is that a capped leg's `B` is scored over a time-prefix of the run. **`B`
//! stays like-for-like regardless**, because every functional is computed over
//! the same prefix.
//!
//! OBSERVATION ONLY. The gauge owns all its state, no engine decision can
//! reach it, and with the gate off every feed site is a null check on a
//! `OnceLock<Option<…>>` that resolved to `None`.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::time::Instant;

/// Samples per emitted line. Amortises the write over a batch so the
/// per-sample cost at a kHz leg is a `write!` into a `String`.
///
/// **DECLARED RESOURCE BOUND**, and it is also the bound on loss 1 above: at
/// most `BATCH − 1` samples per path per run are left unwritten in the tail
/// partial batch.
const BATCH: usize = 256;

/// Default cap on dumped samples per path (`RWM_RTT_DUMP_MAX`).
///
/// **DECLARED RESOURCE BOUND.** At ~10 B per sample on the wire format above
/// this is ~4 MB of stderr per path. The densest leg in the battery (`c1`,
/// 338 279 samples per rep) is covered whole; the cap exists so that a longer
/// or faster leg degrades to a declared, reported prefix instead of filling a
/// disk.
const DUMP_MAX_DEFAULT: usize = 400_000;

/// Lower/upper clamps on the override, in the shape `ackdiag::window_us` uses.
const DUMP_MAX_MIN: usize = 1_000;
const DUMP_MAX_MAX: usize = 20_000_000;

/// Resolved `RWM_RTT_DUMP_MAX`, once per process. A mistyped or out-of-domain
/// override resolves back to the default and is echoed as its RESOLVED value,
/// so "my arm did not take" is read rather than inferred.
pub fn dump_max() -> usize {
    static M: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *M.get_or_init(|| {
        std::env::var("RWM_RTT_DUMP_MAX")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .map(|v| v.clamp(DUMP_MAX_MIN, DUMP_MAX_MAX))
            .unwrap_or(DUMP_MAX_DEFAULT)
    })
}

/// Per-path dump state. Nothing here is read by any engine decision.
#[derive(Default)]
struct PathDump {
    /// Samples dumped so far (the numerator of the coverage ratio).
    emitted: usize,
    /// Samples offered so far, dumped or not (the gauge's own denominator).
    seen: u64,
    /// Absolute µs stamp of the current batch's first sample.
    batch_t0: u64,
    /// Stamp of the previous sample in the current batch.
    prev_us: u64,
    /// Samples buffered in the current batch.
    batch_n: usize,
    /// The current batch's `dt,rtt;` payload.
    buf: String,
    /// Whether the `[RTTDUMP-CAP]` notice has already been printed.
    capped: bool,
}

/// The gauge. One per process, behind `RWM_RTT_DUMP`.
pub struct RttDump {
    epoch: Instant,
    cap: usize,
    paths: Mutex<HashMap<u32, PathDump>>,
}

impl RttDump {
    fn new() -> Self {
        Self {
            epoch: Instant::now(),
            cap: dump_max(),
            paths: Mutex::new(HashMap::new()),
        }
    }

    /// µs since the gauge's own epoch. The gauge carries its own clock for the
    /// same reason `ackdiag` does: the dumped series must be self-consistent
    /// without depending on any engine clock's lifetime.
    fn now_us(&self) -> u64 {
        self.epoch.elapsed().as_micros() as u64
    }

    /// Offer one RTT sample. Called from the single delegate every ack path
    /// funnels through, so the dumped stream is EXACTLY the stream the five
    /// estimators consume — which is the whole point: clause `B` compares an
    /// estimator against its own input, and "its own input" has to be
    /// literally true or the comparison is the one this gauge replaces.
    pub fn note_rtt(&self, path_id: u32, rtt_us: u32) {
        let now = self.now_us();
        let mut m = self.paths.lock();
        let p = m.entry(path_id).or_default();
        p.seen += 1;

        if p.emitted >= self.cap {
            if !p.capped {
                p.capped = true;
                // Printed ONCE, at the moment the cap binds, so truncation is
                // a line in the log rather than a silent shortfall. The parser
                // also cross-checks `emitted` against the `[DIAG]` gauge `n`.
                eprintln!(
                    "[RTTDUMP-CAP] p={path_id} emitted={} seen={} \
                     — cap RWM_RTT_DUMP_MAX={} reached, later samples NOT dumped \
                     (clause B scored over a contiguous PREFIX of this leg)",
                    p.emitted, p.seen, self.cap
                );
            }
            return;
        }

        if p.batch_n == 0 {
            p.batch_t0 = now;
            p.prev_us = now;
            p.buf.clear();
        }
        let dt = now.saturating_sub(p.prev_us);
        p.prev_us = now;
        let _ = write!(p.buf, "{dt},{rtt_us};");
        p.batch_n += 1;
        p.emitted += 1;

        if p.batch_n == BATCH {
            eprintln!(
                "[RTTDUMP] p={path_id} t0={} n={} d={}",
                p.batch_t0, p.batch_n, p.buf
            );
            p.batch_n = 0;
            p.buf.clear();
        }
    }

    /// `(emitted, seen)` for a path — the machine-readable escape hatch, in
    /// the shape `ackdiag::totals` uses. Test-facing; no engine caller.
    pub fn totals(&self, path_id: u32) -> Option<(usize, u64)> {
        self.paths.lock().get(&path_id).map(|p| (p.emitted, p.seen))
    }
}

/// The process-global gauge, or `None` with the gate off.
///
/// Default OFF, resolved once. With it off every feed site is a null check —
/// the same zero-cost shape `ackdiag::gauge` and `cpuprof` use.
pub fn gauge() -> Option<&'static RttDump> {
    static G: std::sync::OnceLock<Option<RttDump>> = std::sync::OnceLock::new();
    G.get_or_init(|| {
        if crate::config::env_flag("RWM_RTT_DUMP", false) {
            Some(RttDump::new())
        } else {
            None
        }
    })
    .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gauge_is_absent_on_the_shipped_default() {
        // The gate is a process-global `OnceLock`, so this asserts the DEFAULT
        // resolution in a process where nothing set the variable. It is the
        // two-sided OFF-VALUE property (MEASUREMENT DISCIPLINE 15) at the
        // module level: an instrument that could be on by accident would put
        // megabytes of stderr and a per-sample lock into every shipped run.
        assert!(
            std::env::var("RWM_RTT_DUMP").is_err(),
            "this test asserts the DEFAULT; the environment set RWM_RTT_DUMP"
        );
        assert!(
            gauge().is_none(),
            "RWM_RTT_DUMP must ship default OFF — it is a raw-sample dump"
        );
    }

    #[test]
    fn the_dump_max_override_is_clamped_and_defaults() {
        // Resolved-value discipline: an out-of-domain override must resolve to
        // something inside the domain rather than to a wild value, and the
        // resolved number is what the `[GATES]` echo prints.
        assert_eq!(dump_max(), DUMP_MAX_DEFAULT);
        assert!(DUMP_MAX_MIN <= DUMP_MAX_DEFAULT && DUMP_MAX_DEFAULT <= DUMP_MAX_MAX);
    }

    #[test]
    fn a_batch_reconstructs_its_own_absolute_timeline_exactly() {
        // The format's one non-obvious property, asserted rather than
        // described: each batch is self-contained, so `t_k = t0 + Σ dt_i`
        // recovers absolute stamps with no cross-batch state. This is the
        // property the offline scorer depends on; if it were false, every
        // successive difference computed off the dump would be at the wrong
        // lag and clause B would be scored against a mis-timed series.
        let d = RttDump {
            epoch: Instant::now(),
            cap: 1_000,
            paths: Mutex::new(HashMap::new()),
        };
        for _ in 0..3 {
            d.note_rtt(0, 1234);
        }
        let (emitted, seen) = d.totals(0).expect("path 0 present after three samples");
        assert_eq!((emitted, seen), (3, 3));
        let m = d.paths.lock();
        let p = &m[&0];
        // First dt is 0 by construction, so t0 IS the first sample's stamp.
        assert!(
            p.buf.starts_with("0,1234;"),
            "the first entry of a batch must carry dt = 0 so that t0 is the \
             first sample's own stamp, got `{}`",
            p.buf
        );
        assert_eq!(p.buf.matches(';').count(), 3, "one entry per sample");
        assert!(
            p.prev_us >= p.batch_t0,
            "stamps must be non-decreasing within a batch"
        );
    }

    #[test]
    fn the_cap_is_reported_not_hidden() {
        // A truncated dump that looked complete would silently make clause B a
        // scoring over an unknown subset. The cap must announce itself, and it
        // must announce itself EXACTLY ONCE however long the run continues.
        let d = RttDump {
            epoch: Instant::now(),
            cap: 2,
            paths: Mutex::new(HashMap::new()),
        };
        for _ in 0..10 {
            d.note_rtt(7, 500);
        }
        let (emitted, seen) = d.totals(7).expect("path 7 present");
        assert_eq!(emitted, 2, "the cap bounds what is DUMPED");
        assert_eq!(seen, 10, "and `seen` still counts what was OFFERED");
        assert!(
            d.paths.lock()[&7].capped,
            "the cap must latch its notice so it prints once, not per sample"
        );
    }

    #[test]
    fn the_gauge_is_observation_only() {
        // Structural, in the shape `ackdiag_is_observation_only` uses: the
        // gauge exposes exactly one feed and one read, neither of which any
        // engine decision consumes. `note_rtt` returns `()` — there is no
        // value for a caller to branch on — and `totals` is test-facing.
        let d = RttDump {
            epoch: Instant::now(),
            cap: 10,
            paths: Mutex::new(HashMap::new()),
        };
        let ret: () = d.note_rtt(1, 42);
        assert_eq!(ret, ());
        assert_eq!(d.totals(2), None, "an unfed path has no state at all");
    }
}
