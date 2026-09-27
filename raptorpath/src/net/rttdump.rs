//! The raw RTT sample dump (`RWM_RTT_DUMP`, default OFF, observation only).
//!
//! It emits the exact RTT sample stream the dispersion estimators consume, so
//! each estimator's online reading can be compared against the same
//! functional computed offline over the identical samples. That reference is
//! exact and like-for-like by construction, unlike an external latency probe
//! (e.g. `latt_probe.py`'s 20 Hz ICMP), which measures delivered round-trip
//! time through the whole shaped path — a different quantity, sampled at a
//! rate hundreds of times lower — and so can only ever reject an estimator,
//! never acquit one.
//!
//! This is a narrowing: the comparison asks whether an estimator faithfully
//! computes its functional over its own input, not whether that input is the
//! true delivered latency. A delivered-latency probe at the sender's own
//! sample rate does not exist.
//!
//! At a sender leg running tens of kHz this gauge writes megabytes of stderr,
//! a CPU and I/O cost on the sender that perturbs sender-side dispersion. A
//! dump-on pass is therefore run separately from the scored dump-off
//! invocations, and its own dispersion readings are reported so the
//! perturbation is visible.
//!
//! ## Format
//!
//! ```text
//!   [RTTDUMP] p=<path> t0=<µs since gauge epoch> n=<count> d=<dt,rtt;dt,rtt;…>
//! ```
//!
//! One line per `BATCH` samples per path, so the per-sample cost is a push
//! into a string rather than a write. Each batch is self-contained: `t0` is
//! the absolute stamp of its first sample and every `dt` is a delta from the
//! previous sample within the batch, with the first `dt` always 0. Both
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
//!      as a contiguous prefix of the run. When it binds, one
//!      `[RTTDUMP-CAP]` line is printed, once, naming the count.
//!   3. Both are checkable against the gauges' own denominators: the final
//!      `[DIAG]` block's `sig_us=…/n<count>` is the number of samples the
//!      estimators saw, so `dumped / n` is the dump's own coverage and the
//!      parser reports it rather than assuming it is 1.
//!
//! Truncation keeps a prefix. It keeps the run's early samples, not a random
//! subset — a contiguous prefix is required because the functionals under
//! test are successive differences and a decimated sample set would change
//! the lag the estimand is defined at. A capped leg is therefore scored over
//! a time-prefix of the run, and stays like-for-like because every functional
//! is computed over the same prefix.
//!
//! Observation only. The gauge owns all its state, no engine decision can
//! reach it, and with the gate off every feed site is a null check on a
//! `OnceLock<Option<…>>` that resolved to `None`.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::time::Instant;

/// Samples per emitted line. Amortises the write over a batch so the
/// per-sample cost at a kHz leg is a `write!` into a `String`.
///
/// Declared resource bound, and the bound on loss 1 above: at
/// most `BATCH − 1` samples per path per run are left unwritten in the tail
/// partial batch.
const BATCH: usize = 256;

/// Default cap on dumped samples per path (`RWM_RTT_DUMP_MAX`).
///
/// Declared resource bound. At ~10 B per sample on the wire format above
/// this is ~4 MB of stderr per path, which covers a dense single-path leg
/// whole; a longer or faster leg degrades to a declared, reported prefix
/// instead of filling a disk.
const DUMP_MAX_DEFAULT: usize = 400_000;

/// Lower/upper clamps on the override, in the shape `ackdiag::window_us` uses.
const DUMP_MAX_MIN: usize = 1_000;
const DUMP_MAX_MAX: usize = 20_000_000;

/// Resolved `RWM_RTT_DUMP_MAX`, once per process. A mistyped or out-of-domain
/// override resolves back to the default and is echoed as its resolved value,
/// so "my arm did not take" is read rather than inferred.
pub fn dump_max() -> usize {
    crate::gates::get().rtt_dump_max
}

/// The resolve-time read behind [`dump_max`].
pub(crate) fn resolve_dump_max() -> usize {
    std::env::var("RWM_RTT_DUMP_MAX")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|v| v.clamp(DUMP_MAX_MIN, DUMP_MAX_MAX))
        .unwrap_or(DUMP_MAX_DEFAULT)
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
    /// funnels through, so the dumped stream is exactly the stream the
    /// estimators consume — the comparison is only valid if "its own input"
    /// is literally true.
    pub fn note_rtt(&self, path_id: u32, rtt_us: u32) {
        let now = self.now_us();
        let mut m = self.paths.lock();
        let p = m.entry(path_id).or_default();
        p.seen += 1;

        if p.emitted >= self.cap {
            if !p.capped {
                p.capped = true;
                // Printed once, at the moment the cap binds, so truncation is
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
        if crate::gates::get().rtt_dump {
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
        // The gate is a process-global `OnceLock`, so this asserts the default
        // resolution in a process where nothing set the variable — the
        // two-sided off-value property (`docs/measurement-discipline.md`
        // rule 15). An instrument that could be on by accident would put
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
        // lag.
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
        // First dt is 0 by construction, so t0 is the first sample's stamp.
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
        // A truncated dump that looked complete would make the comparison a
        // scoring over an unknown subset. The cap must announce itself, and
        // exactly once however long the run continues.
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
