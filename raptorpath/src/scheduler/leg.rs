//! `LegState`: the per-leg estimators every law reads (W3.2, design
//! `docs/derivations/w3-per-leg-state.md` §2). `PathState` owns one.
//!
//! Phase A1 holds only the smoothed app-echo RTT `s`: it replaces both the
//! Copa `srtt` (R1) and the loss estimator's `ewma_rtt` (R2).

use super::*;

/// The smoothed app-echo RTT `s` (design §2, R1 = R2): EWMA 1/8 on one sample
/// stream, seeded from the first sample, in `Duration::mul_f64` form (Copa's
/// arithmetic, which the cadence and σ readings were captured against).
///
/// Deviation from the design's `s: Option<Duration>` sketch, forced by the
/// revival window (A0 `revived-window` row): today a revival clears Copa's
/// srtt but not the loss estimator's, so until the first post-revival sample
/// `srtt()`, `srtt_measured()` and the recovery clocks read the stale
/// pre-death value while Copa's cadence reads the 50 ms default. A bare
/// `Option` cleared on revival would move the first three. `reseed` carries
/// that: the value survives, and the next sample replaces it outright
/// instead of blending (the owner's decision: reseed on revival, as R1 does).
#[derive(Debug, Clone, Default)]
pub struct SmoothedRtt {
    s: Option<Duration>,
    reseed: bool,
}

impl SmoothedRtt {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one echo sample in.
    pub fn record(&mut self, rtt: Duration) {
        self.s = Some(match self.s {
            Some(s) if !self.reseed => s.mul_f64(0.875) + rtt.mul_f64(0.125),
            _ => rtt,
        });
        self.reseed = false;
    }

    /// The measurement, `None` until a sample has ever arrived.
    pub fn measured(&self) -> Option<Duration> {
        self.s
    }

    /// `measured()`, or the 50 ms `DEFAULT_SRTT` seed (what `srtt()` reads).
    pub fn or_default(&self) -> Duration {
        self.s.unwrap_or(DEFAULT_SRTT)
    }

    /// The controller's view: the default while the value awaits its reseed
    /// (Copa's cadence and σ reference never read a pre-death value).
    pub fn cadence(&self) -> Duration {
        if self.reseed { DEFAULT_SRTT } else { self.or_default() }
    }

    /// Revival: keep the stale value readable, reseed on the next sample.
    pub fn restart(&mut self) {
        self.reseed = true;
    }
}

/// Sliding window entry for bandwidth/RTT tracking.
#[derive(Clone, Debug)]
pub(crate) struct BwSample {
    /// Delivery rate in symbols per second.
    pub(crate) delivery_rate: f64,
    /// Timestamp when this sample was taken.
    pub(crate) timestamp: Instant,
}

#[derive(Clone, Debug)]
pub(crate) struct RttSample {
    pub(crate) rtt: Duration,
    pub(crate) timestamp: Instant,
}

/// Cap on rate-sample send records tracked per path (bounds the map when
/// symbols are lost / attributed without a matching send record). ~a few
/// aggregate BDPs; oldest are dropped past this.
pub(crate) const RS_MAX_TRACKED: usize = 8192;

/// A sent source symbol's BBR delivery-rate-sample state
/// (draft-cheng-iccrg-delivery-rate-estimation), snapshotted at send time and
/// consumed when the symbol is acked to produce one rate sample whose Δt is the
/// send interval — robust to ack-aggregation and a standing queue.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RsPacket {
    /// `C.delivered` (this path's delivered counter) at the moment of send.
    delivered: u64,
    /// `C.delivered_time` (time `delivered` last advanced) at send.
    delivered_time: Instant,
    /// `C.first_sent_time` (start of the current in-flight send burst) at send.
    first_sent_time: Instant,
    /// When this symbol was sent.
    sent_time: Instant,
    /// The sender was app-limited (starved, not cwnd/pace-limited) at send —
    /// the sample may only raise the max-filter, never be read as bw dropping.
    app_limited: bool,
}

/// The per-leg estimators every law reads (design §2). `PathState` owns one;
/// Copa's controller receives `&LegState` and keeps only its control state.
///
/// Phase A2 is a pure move: each field and method below is the one Copa held,
/// verbatim, and the A0 trace pin is unchanged by it. Flat, not the sketch's
/// nested `MinFilter` / `MaxFilter` / `RateSampler`: the nesting buys nothing
/// until a consumer needs one of them alone.
///
/// Idle: R1, R2, R4, J1-J3 and B1 freeze when a leg gets no echo and no
/// delivery (eviction runs on a feed only); `w` refreshes at idle (design
/// §1.2). A revival clears everything but `w` and the stale `s` (see
/// [`SmoothedRtt`]).
#[derive(Debug)]
pub struct LegState {
    /// The smoothed app-echo RTT (R1 = R2).
    pub(crate) s: SmoothedRtt,
    /// quinn's packet-timed smoothed RTT of this leg's connection, mirrored
    /// at every ack batch and path report (paper §5.7, R3). `None` until quinn
    /// has an RTT sample for the leg. Unlike `s` it does not need this leg's
    /// own symbols to be echoed, so an idle leg keeps it fresh, and it holds no
    /// local dwell. Survives a revival. Read only by `place_costs`.
    w: Option<Duration>,
    /// When the last echo sample / wire mirror / delivery-rate sample was
    /// folded in (design §2 `e_at`, `w_at`, `b_at`; observation only). A revival
    /// clears `e_at` and `b_at` and keeps `w_at` with `w`.
    e_at: Option<Instant>,
    w_at: Option<Instant>,
    b_at: Option<Instant>,
    /// Sliding window of bandwidth samples (symbols/sec).
    pub(crate) bw_samples: VecDeque<BwSample>,
    /// `RWM_HONEST_ANCHOR`: monotonic (non-increasing) max-deque maintained
    /// beside `bw_samples` — the mirror of the `rtt_samples` min-deque, on the
    /// max statistic. Fed and evicted in lockstep with `bw_samples`
    /// (`bw_push_sample` / `bw_evict_before`), so its front is always the
    /// full-window fold's
    /// value (unit-pinned). `max_bw` reads it only under `bw_o1`; the deque
    /// itself is maintained unconditionally (O(1) amortized, ≤ the size of
    /// `bw_samples`) so the equality is testable without env plumbing.
    pub(crate) bw_mono: VecDeque<BwSample>,
    /// `RWM_HONEST_ANCHOR` resolved at construction: `max_bw` = the mono
    /// deque's front (O(1)) instead of the per-sample full-window fold
    /// (O(window), a CPU tax under `RWM_PLAIN_RS` (removed, tag archive/RWM_PLAIN_RS)). Value-identical either way.
    pub(crate) bw_o1: bool,
    /// Sliding window of RTT samples.
    pub(crate) rtt_samples: VecDeque<RttSample>,
    /// How long to keep samples in sliding windows (10s).
    pub(crate) window_duration: Duration,
    /// Minimum RTT in the sliding window = estimated propagation floor.
    pub(crate) min_rtt: Option<Duration>,
    /// Maximum delivery rate seen in the current window.
    pub(crate) max_bw: f64,
    /// Consecutive-difference jitter estimate (seconds): EWMA of
    /// |rtt_i − rtt_{i−1}| at gain 1/8 (RFC 3550-style). Shift-robust by
    /// construction — a standing queue shifts all samples and leaves the
    /// consecutive differences at jitter scale, so this measures jitter,
    /// never queue. Widens the backoff threshold (JITTER_HEADROOM).
    pub(crate) jitter_est: f64,
    /// EWMA of the squared deviation from the smoothed RTT (seconds²) —
    /// `var = (1−β)·var + β·(rtt − srtt)²` at RFC 6298 §2's own smoothing
    /// gain `β = 1/4`, the gain that RFC uses for exactly this job on the
    /// first absolute moment (`RTTVAR = (1−β)·RTTVAR + β·|SRTT − R'|`).
    ///
    /// Exists for the derived recovery clock (paper §7.4), which needs a
    /// genuine second moment: Cantelli's distribution-free bound is stated in
    /// σ, and the other dispersion signals are mean-absolute statistics
    /// (`jitter_est` on consecutive differences, the estimator's RFC 3550
    /// interarrival jitter). Converting either to σ requires assuming a
    /// distribution, which would turn the clock's distribution-free guarantee
    /// into a fitted coefficient.
    ///
    /// Observation only: read by the `[DIAG] sig_us=` and `[QCLK]` gauges.
    pub(crate) rtt_var_sq: f64,
    /// How many samples have been folded into `rtt_var_sq` — the EWMA's own
    /// warm-up denominator, and the honest half of the `sig_us=` gauge.
    ///
    /// `ANCHOR_MIN_SAMPLES` gates the delivered-rate anchor (`bw_samples`),
    /// not this statistic, which is available from the first sample that has
    /// an `srtt` to deviate from. But it is not trustworthy from the first
    /// sample: the EWMA is seeded at 0 and runs at RFC 6298's β = 1/4, so it
    /// carries ≈ 0.75^n of its seed after n samples — 24 % at n = 5, 10 % at
    /// n = 8, 1 % at n = 16. A σ read at n = 2 is biased low by roughly half.
    ///
    /// So the count is reported alongside σ rather than used to gate it (`n`
    /// in `sig_us=<µs>/n<count>`). Emitting only-when-valid would require a
    /// threshold on n, and a threshold that selects whether a gauge exists is
    /// the same defect as a threshold that selects a law: the reader could not
    /// tell "σ suppressed" from "path never sampled". The parser gets the
    /// number and the evidence about it, and decides.
    pub(crate) rtt_var_n: u64,
    /// Candidate 2 — the `rvar_us=` gauge. RFC 6298 §2's `RTTVAR`, the
    /// mean-deviation EWMA: `rvar ← (1−β)·rvar + β·|rtt − srtt|` at the RFC's
    /// own β = 1/4 (`SIGMA_CAND_RVAR_GAIN`).
    ///
    /// It is the shipped `rtt_var_sq` with the square removed and nothing else
    /// changed — same feed site, same β, same lagging `srtt` reference, same
    /// 7-sample memory: the controlled comparison that isolates outlier
    /// leverage. If `rvar`'s dispersion lands near `√(dispersion of sig_us)`,
    /// the square is the culprit; if it stays near `sig_us`'s, the memory or
    /// the reference is.
    ///
    /// Observation only: read by nothing but `[DIAG]`.
    pub(crate) rtt_mdev: f64,
    /// Samples folded into [`CopaState::rtt_mdev`] — its warm-up denominator,
    /// on the line beside it. EWMA-class, so its warm-up `n_warm` is 16
    /// (`0.75^16` = 1.00 % seed retention), the same as `rtt_var_n`'s.
    pub(crate) rtt_mdev_n: u64,
    /// The raw RTT series, last `SIGMA_CAND_WINDOW` samples, µs, FIFO.
    /// Feeds candidates 1 (`qsp_us`) and 3 (`msd_us`).
    ///
    /// This cannot reuse `rtt_samples`: that deque is monotonic — it pops
    /// from the back on every sample that is not larger than the incoming one,
    /// because it serves a windowed minimum in O(1) — so it holds a lower
    /// envelope of the series, and a dispersion taken over an envelope would
    /// read the drift and not the spread. This is a plain FIFO in arrival
    /// order, which is also what makes candidate 3's successive differences
    /// meaningful.
    pub(crate) rtt_win: VecDeque<u32>,
    /// The timestamped, time-decimated RTT series for candidate 4
    /// (`tlag_us=`): `(arrival instant, rtt µs)`, ascending in time, at most
    /// `SIGMA_CAND_WINDOW` entries.
    ///
    /// This cannot reuse `rtt_win`: that FIFO carries no timestamps, so a lag
    /// can only be counted in samples there, and a lag-1 successive difference
    /// estimates the structure function `V` at whatever the inter-sample
    /// spacing happens to be — set by the ack rate rather than by the link.
    /// Nor `rtt_samples`, which holds a lower envelope, not the series.
    ///
    /// Entries are admitted at most one per `RTprop / SIGMA_TLAG_DECIM_M`, so
    /// the ring spans `32 · RTprop` at every sample rate (see the const doc).
    /// Fed unconditionally; read by nothing but `[DIAG]`.
    pub(crate) rtt_tlag: VecDeque<(Instant, u32)>,
    /// Previous raw RTT sample (for the consecutive difference).
    pub(crate) prev_rtt_sample: Option<Duration>,
    /// Delivered symbols counter for delivery rate calculation.
    pub(crate) delivered: u64,
    /// Timestamp of last delivery measurement.
    pub(crate) last_delivered_time: Instant,
    /// Delivered count at last measurement.
    pub(crate) last_delivered: u64,
    // --- BBR delivery-rate sampling (the send-interval anchor, ADR-0061) ---
    /// Total source symbols delivered on this path (BBR `C.delivered`). Separate
    /// from `delivered` so the ack-interval anchor stays unchanged.
    pub(crate) rs_delivered: u64,
    /// Time `rs_delivered` last advanced (BBR `C.delivered_time`).
    pub(crate) rs_delivered_time: Instant,
    /// Send time of the packet that started the current in-flight send burst
    /// (BBR `C.first_sent_time`); advances to each acked packet's send time.
    pub(crate) rs_first_sent_time: Instant,
    /// Outstanding rate-sample send records (seq → snapshot), consumed on ack.
    pub(crate) rs_sent: BTreeMap<u64, RsPacket>,
    // --- DIAG counters ---------------------------------------------------------
    // Pure observation — these never affect a control decision (read only at the
    // RWM_DIAG print).  They trace why the per-path BtlBw anchor does or does not
    // warm: how many source seqs were snapshotted at send, how many acks were
    // attributed here, and how each attributed ack was classified by the BBR
    // GenerateRateSample guards (interval<MinRTT / zero-delivered / app-limited)
    // vs accepted into the windowed-max filter.
    pub(crate) rs_sent_count: u64,
    pub(crate) rs_applimited_sent: u64,
    pub(crate) rs_attributions: u64,
    pub(crate) rs_no_record: u64,
    pub(crate) rs_rej_interval: u64,
    pub(crate) rs_rej_zero: u64,
    pub(crate) rs_rej_applimited: u64,
    pub(crate) rs_generated: u64,
    /// RWM_RS_TRACE: eprintln each accepted rate sample above the given
    /// symbols/s threshold with its (delivered, interval, send_elapsed,
    /// ack_elapsed) decomposition — the over-read forensics instrument.
    /// 0 = off (default, no cost on the sample path).
    pub(crate) rs_trace_thresh: f64,
    /// DIAG label for the RSTRACE prints: the owning path's id (u32::MAX
    /// until the owner stamps it). Never read by any control decision.
    pub(crate) rs_trace_path: u32,
    /// Injectable clock for time queries.
    pub(crate) clock: Arc<dyn Clock>,
}

impl LegState {
    pub(crate) fn new(clock: Arc<dyn Clock>) -> Self {
        let now = clock.now();
        Self {
            s: SmoothedRtt::new(),
            w: None,
            e_at: None,
            w_at: None,
            b_at: None,
            bw_samples: VecDeque::new(),
            bw_mono: VecDeque::new(),
            bw_o1: honest_anchor_active(),
            rtt_var_sq: 0.0,
            rtt_var_n: 0,
            rtt_mdev: 0.0,
            rtt_mdev_n: 0,
            rtt_win: VecDeque::with_capacity(SIGMA_CAND_WINDOW),
            rtt_tlag: VecDeque::with_capacity(SIGMA_CAND_WINDOW),
            rtt_samples: VecDeque::new(),
            window_duration: Duration::from_secs(10),
            min_rtt: None,
            max_bw: 0.0,
            jitter_est: 0.0,
            prev_rtt_sample: None,
            delivered: 0,
            last_delivered_time: now,
            last_delivered: 0,
            rs_delivered: 0,
            rs_delivered_time: now,
            rs_first_sent_time: now,
            rs_sent: BTreeMap::new(),
            rs_sent_count: 0,
            rs_applimited_sent: 0,
            rs_attributions: 0,
            rs_no_record: 0,
            rs_rej_interval: 0,
            rs_rej_zero: 0,
            rs_rej_applimited: 0,
            rs_generated: 0,
            rs_trace_thresh: crate::gates::get().rs_trace,
            rs_trace_path: u32::MAX,
            clock,
        }
    }

    /// Revival (`Scheduler::touch_path_at`): every estimator restarts except
    /// the wire RTT (quinn's, which a dead path's revival does not clear) and
    /// `s`, which keeps its stale value readable and reseeds on the next
    /// sample.
    pub(crate) fn reset(&mut self) {
        let (w, w_at) = (self.w, self.w_at);
        let mut s = self.s.clone();
        s.restart();
        *self = Self::new(self.clock.clone());
        self.w = w;
        self.w_at = w_at;
        self.s = s;
    }

    /// Mirror quinn's packet-timed RTT for this leg (placement base, §5.7).
    pub(crate) fn set_wire(&mut self, rtt: Duration) {
        self.w = Some(rtt);
        self.w_at = Some(self.clock.now());
    }

    /// The mirrored wire RTT, `None` if quinn has not measured the leg yet.
    pub(crate) fn wire(&self) -> Option<Duration> {
        self.w
    }

    /// Fold one RTT sample into RTprop, the jitter / σ / dispersion
    /// estimators and the raw-sample rings. `ref_srtt` is the measured smoothed
    /// RTT σ's deviation is taken against (`s`, or the wire arm's controller
    /// EWMA); `None` skips the σ and mean-deviation folds.
    pub(crate) fn record_sample(&mut self, rtt: Duration, ref_srtt: Option<Duration>) {
        let now = self.clock.now();
        self.e_at = Some(now);

        // Consecutive-difference jitter EWMA (shift-robust; see field doc).
        if let Some(prev) = self.prev_rtt_sample {
            let diff = if rtt > prev { rtt - prev } else { prev - rtt };
            self.jitter_est += (diff.as_secs_f64() - self.jitter_est) * JITTER_GAIN;
        }
        self.prev_rtt_sample = Some(rtt);

        // The derived clock's second moment, fed against the smoothed mean the
        // same way RFC 6298 §2 feeds RTTVAR and at the same β = 1/4. Fed
        // unconditionally; read by nothing on the default arm.
        if let Some(sr) = ref_srtt {
            let dev = rtt.as_secs_f64() - sr.as_secs_f64();
            self.rtt_var_sq = 0.75 * self.rtt_var_sq + 0.25 * dev * dev;
            // Counted at the same site that feeds the EWMA, so the `[DIAG]`
            // `sig_us=<µs>/n<count>` gauge's denominator can never describe a
            // different sample set than its numerator.
            self.rtt_var_n += 1;
            // Candidate 2 (`rvar_us=`): RFC 6298 §2's mean deviation, fed at
            // the same site, from the same `dev`, at the same β. Identical to
            // the line above except that the deviation enters linearly instead
            // of squared, which makes the pair a decomposition. Read by nothing.
            self.rtt_mdev += (dev.abs() - self.rtt_mdev) * SIGMA_CAND_RVAR_GAIN;
            self.rtt_mdev_n += 1;
        }
        // Candidates 1 and 3 (`qsp_us=`, `msd_us=`): the raw series, FIFO,
        // last `SIGMA_CAND_WINDOW`. Fed unconditionally and without an `srtt`
        // precondition — neither gauge takes a deviation against a reference.
        // O(1): one push, at most one pop. Read by nothing.
        if self.rtt_win.len() == SIGMA_CAND_WINDOW {
            self.rtt_win.pop_front();
        }
        self.rtt_win
            .push_back((rtt.as_micros() as u64).min(u32::MAX as u64) as u32);
        // Windowed minimum via a monotonic (non-decreasing) deque — O(1)
        // amortised instead of an O(n) rescan of the 10 s sample history on
        // every sample (with thousands of RTT samples/s that rescan dominates
        // sender CPU). A new sample evicts every pending candidate whose RTT
        // is >= its own: those can never be the window minimum while this
        // newer, smaller-or-equal sample is in the window (and it expires
        // strictly later), so the deque stays non-decreasing front→back with
        // strictly increasing timestamps. The front is always the current
        // windowed min, and time-based expiry pops from the (oldest) front.
        while self.rtt_samples.back().is_some_and(|s| s.rtt >= rtt) {
            self.rtt_samples.pop_back();
        }
        self.rtt_samples.push_back(RttSample {
            rtt,
            timestamp: now,
        });
        self.expire_old_samples(now);
        self.min_rtt = self.rtt_samples.front().map(|s| s.rtt);

        // Candidate 4 (`tlag_us=`): the timestamped, time-decimated series.
        // Fed after `min_rtt` is refreshed above, so the admission spacing
        // uses the freshest `τ = RTprop` this sample can see.
        //
        // Admission is a spacing cap applied identically at every rate — not a
        // branch on the rate. As the arrival spacing rises through `τ/m` the
        // admitted stream passes continuously from decimated to pass-through.
        // With `τ` unavailable nothing is admitted, the pair set stays empty
        // and the gauge renders `-`: there is no fallback constant, which
        // would be a free constant with no provenance and a second code path.
        //
        // O(1): one `duration_since`, at most one push and one pop.
        if let Some(tau) = self.min_rtt {
            let spacing = tau / SIGMA_TLAG_DECIM_M;
            let admit = match self.rtt_tlag.back() {
                Some((t_last, _)) => now.duration_since(*t_last) >= spacing,
                None => true,
            };
            if admit {
                if self.rtt_tlag.len() == SIGMA_CAND_WINDOW {
                    self.rtt_tlag.pop_front();
                }
                self.rtt_tlag
                    .push_back((now, (rtt.as_micros() as u64).min(u32::MAX as u64) as u32));
            }
        }
    }

    /// Admit one bandwidth sample into the sliding window: `bw_samples` gets
    /// it verbatim (its length is the anchor-establishment gate —
    /// `ANCHOR_MIN_SAMPLES` — and must not change meaning), and the
    /// monotonic max-deque `bw_mono` gets it with dominated-candidate
    /// eviction: a sample that is older and no
    /// larger than the new one can never again be the windowed max — any
    /// front-eviction cutoff that spares it spares the newer, larger sample
    /// too — so after eviction the mono deque is strictly decreasing
    /// front→back with increasing timestamps, and its front equals the
    /// full-window fold over `bw_samples` at all times (unit-pinned by
    /// `bw_mono_front_equals_full_window_fold`).
    fn bw_push_sample(&mut self, now: Instant, rate: f64) {
        self.b_at = Some(now);
        self.bw_samples.push_back(BwSample {
            delivery_rate: rate,
            timestamp: now,
        });
        while self
            .bw_mono
            .back()
            .is_some_and(|s| s.delivery_rate <= rate)
        {
            self.bw_mono.pop_back();
        }
        self.bw_mono.push_back(BwSample {
            delivery_rate: rate,
            timestamp: now,
        });
    }

    /// Evict bandwidth samples older than `cutoff` from both windows (the
    /// two structures see identical push and eviction sequences — that
    /// lockstep is what makes front == fold an invariant rather than a
    /// coincidence). Called with the 10 s cutoff from
    /// `expire_old_samples` and with the ≈10·RTprop [1 s, 10 s] cutoff from
    /// `rs_on_delivered`, exactly where `bw_samples` was already evicted.
    fn bw_evict_before(&mut self, cutoff: Instant) {
        while self.bw_samples.front().is_some_and(|s| s.timestamp < cutoff) {
            self.bw_samples.pop_front();
        }
        while self.bw_mono.front().is_some_and(|s| s.timestamp < cutoff) {
            self.bw_mono.pop_front();
        }
    }

    /// Recompute `max_bw` after a push/evict. `RWM_HONEST_ANCHOR` selects the
    /// cost of the same value: the mono deque's front (O(1) amortized) or
    /// the full-window fold (O(window) per accepted sample — a sender-CPU tax
    /// once `RWM_PLAIN_RS` (removed, tag archive/RWM_PLAIN_RS) feeds this per delivered symbol instead of per ack).
    fn bw_refresh_max(&mut self) {
        self.max_bw = if self.bw_o1 {
            self.bw_mono.front().map_or(0.0, |s| s.delivery_rate)
        } else {
            self.bw_samples
                .iter()
                .map(|s| s.delivery_rate)
                .fold(0.0f64, f64::max)
        };
    }

    /// The full-window fold, unconditionally — the equivalence
    /// oracle for `bw_mono` (test-only).
    #[cfg(test)]
    pub(crate) fn bw_fold(&self) -> f64 {
        self.bw_samples
            .iter()
            .map(|s| s.delivery_rate)
            .fold(0.0f64, f64::max)
    }

    /// Record delivery of `count` symbols.  Returns the computed delivery rate.
    pub(crate) fn record_delivery(&mut self, count: u32) -> f64 {
        self.delivered += count as u64;
        let now = self.clock.now();
        let elapsed = now.duration_since(self.last_delivered_time).as_secs_f64();

        // Need at least 1ms of elapsed time to compute a meaningful rate
        if elapsed < 0.001 {
            // Ack-cadence gauge (`RWM_ACKDIAG`, net/ackdiag.rs): a rejected
            // sample. It carries no rate (the filter is untouched)
            // but its `count` did arrive, so it feeds the over-read
            // denominator: the normalizer must see every delivered symbol the
            // sampler saw, or x is inflated by the rejection rate.
            if let Some(g) = crate::net::ackdiag::gauge() {
                g.note_rate_sample(self.rs_trace_path, count, 0.0, false);
            }
            return self.max_bw;
        }

        let delta_delivered = self.delivered - self.last_delivered;
        let rate = delta_delivered as f64 / elapsed;

        self.last_delivered_time = now;
        self.last_delivered = self.delivered;

        if self.rs_trace_thresh > 0.0 && rate >= self.rs_trace_thresh {
            crate::readout!(
                "[RSTRACE-LEGACY] path={} rate={:.0} delta={} elapsed_ms={:.2} max_bw={:.0}",
                self.rs_trace_path,
                rate,
                delta_delivered,
                elapsed * 1e3,
                self.max_bw,
            );
        }
        // Ack-cadence gauge (`RWM_ACKDIAG`, net/ackdiag.rs): an accepted
        // ack-interval sample, i.e. exactly one of the values the windowed max
        // below folds. The gauge normalizes it at print time by the window's
        // own long-run delivered rate to give the realized over-read x
        // directly (paper §2.7).
        if let Some(g) = crate::net::ackdiag::gauge() {
            g.note_rate_sample(self.rs_trace_path, count, rate, true);
        }
        // Add to sliding window
        self.bw_push_sample(now, rate);
        self.expire_old_samples(now);

        // Update max bandwidth
        self.bw_refresh_max();

        rate
    }

    // --- BBR delivery-rate sampling (paper §2.7) -------------------------------
    //
    // `record_delivery` above computes rate = Δdelivered / Δt where Δt is the
    // ack-arrival interval (`now − last_delivered_time`). When acks arrive
    // batched (ack-aggregation), a batch collapses Δt toward zero, so
    // Δdelivered/Δt spikes and the windowed max locks onto the spike. A rate
    // anchor that far too high makes every per-path pace bucket / BDP cap
    // inert (the bucket never binds; outstanding bloats to the deep
    // read-ahead).
    //
    // Cardwell/Cheng (draft-cheng-iccrg-delivery-rate-estimation): sample Δt
    // over the send interval — max(send_elapsed, ack_elapsed) — so a batched
    // ack (tiny ack_elapsed) is overridden by the true send spacing and the
    // sample is a correct delivery-rate lower bound. The max-filter then maxes
    // over correct samples and converges to the true BtlBw.

    /// BBR `SendPacket`: snapshot the rate-sample state for a sent source symbol.
    pub(crate) fn rs_on_sent(&mut self, seq: u64, app_limited: bool) {
        self.rs_sent_count += 1; // DIAG
        if app_limited {
            self.rs_applimited_sent += 1; // DIAG
        }
        let now = self.clock.now();
        // No packets in flight → (re)start the send burst window.
        if self.rs_sent.is_empty() {
            self.rs_first_sent_time = now;
            self.rs_delivered_time = now;
        }
        self.rs_sent.insert(
            seq,
            RsPacket {
                delivered: self.rs_delivered,
                delivered_time: self.rs_delivered_time,
                first_sent_time: self.rs_first_sent_time,
                sent_time: now,
                app_limited,
            },
        );
        // Bound the map: drop the oldest snapshots for symbols that were lost or
        // attributed cumulatively without ever matching a send record.
        while self.rs_sent.len() > RS_MAX_TRACKED {
            if let Some(&k) = self.rs_sent.keys().next() {
                self.rs_sent.remove(&k);
            } else {
                break;
            }
        }
    }

    /// BBR `UpdateRateSample` + `GenerateRateSample` for one acked source symbol.
    /// Feeds the windowed-max delivery-rate filter (`max_bw`) a sample whose Δt is
    /// the send interval, so batched acks / a standing queue cannot inflate it.
    pub(crate) fn rs_on_delivered(&mut self, seq: u64) {
        self.rs_attributions += 1; // DIAG
        let now = self.clock.now();
        let Some(p) = self.rs_sent.remove(&seq) else {
            // No send record (attributed cumulatively past a dropped record):
            // still advance the delivered cursor so later samples stay correct.
            self.rs_no_record += 1; // DIAG
            self.rs_delivered += 1;
            self.rs_delivered_time = now;
            return;
        };
        // Advance the connection delivered cursor (BBR: C.delivered += len).
        self.rs_delivered += 1;
        self.rs_delivered_time = now;
        // send_elapsed spans the send spacing of the packets from the burst start
        // to this packet; ack_elapsed spans the same deliveries in wall time.
        let send_elapsed = p.sent_time.saturating_duration_since(p.first_sent_time);
        let ack_elapsed = now.saturating_duration_since(p.delivered_time);
        // Advance the burst-window start (BBR: C.first_sent_time = P.sent_time).
        self.rs_first_sent_time = p.sent_time;
        // max() is what makes the sample ack-aggregation robust: a batched ack
        // shrinks ack_elapsed, but send_elapsed preserves the true spacing.
        let interval = send_elapsed.max(ack_elapsed).as_secs_f64();
        let delivered = self.rs_delivered.saturating_sub(p.delivered);
        // Reject samples spanning less than one RTprop (BBR GenerateRateSample:
        // `if interval < MinRTT: return`).  An interval below the propagation RTT
        // cannot reliably estimate the bottleneck rate: it is the classic
        // ack-aggregation / send-burst artefact (a batch of queued symbols acked
        // together over a tiny window), which otherwise reads many× the true
        // link.  Requiring interval ≥ RTprop forces the
        // sample to average over ≥ one pipe, so a drain burst reads the true
        // bottleneck.  Falls back to a 1 ms absolute floor before an RTprop sample.
        let min_interval = self
            .min_rtt
            .map(|r| r.as_secs_f64())
            .unwrap_or(0.001)
            .max(0.001);
        // DIAG: classify the rejection (split from the combined guard below for
        // per-cause counting; behaviour identical — same early return).
        if interval < min_interval {
            self.rs_rej_interval += 1; // DIAG
            return;
        }
        if delivered == 0 {
            self.rs_rej_zero += 1; // DIAG
            return;
        }
        let rate = delivered as f64 / interval;
        // App-limited samples underestimate bw (the pipe was starved, not full),
        // so they may only raise the max-filter, never be read as bw dropping.
        // In a pure windowed-max a low app-limited sample is simply not the max;
        // admit one only when it exceeds the current max (BBR §app-limited).
        if p.app_limited && rate <= self.max_bw {
            self.rs_rej_applimited += 1; // DIAG
            return;
        }
        self.rs_generated += 1; // DIAG
        if self.rs_trace_thresh > 0.0 && rate >= self.rs_trace_thresh {
            crate::readout!(
                "[RSTRACE] path={} seq={} rate={:.0} delivered={} interval_ms={:.2} send_ms={:.2} ack_ms={:.2} max_bw={:.0}",
                self.rs_trace_path,
                seq,
                rate,
                delivered,
                interval * 1e3,
                send_elapsed.as_secs_f64() * 1e3,
                ack_elapsed.as_secs_f64() * 1e3,
                self.max_bw,
            );
        }
        self.bw_push_sample(now, rate);
        // Max-filter window ≈ 10·RTprop (BBR's BtlBw filter), clamped to
        // [1s, 10s]: long enough to hold the true BtlBw between acks, short
        // enough that a genuine rate change is not pinned for the full 10s
        // sample window.  Falls back to 10s before a min-RTT sample exists.
        let win = self
            .min_rtt
            .map(|r| (r.as_secs_f64() * 10.0).clamp(1.0, 10.0))
            .unwrap_or(10.0);
        let cutoff = now
            .checked_sub(Duration::from_secs_f64(win))
            .unwrap_or(now);
        self.bw_evict_before(cutoff);
        // Under the removed RWM_PLAIN_RS (tag archive/RWM_PLAIN_RS) this runs once per delivered source symbol, so
        // the full-window fold here is O(window·rate) per second, a real
        // sender-CPU tax. RWM_HONEST_ANCHOR reads the same value off the mono
        // deque in O(1).
        self.bw_refresh_max();
    }


    /// BtlBw×RTprop BDP estimate in symbols — the active recovery anchor
    /// (paper §8.3), or None until it is trustworthy.
    ///
    /// Units: max_bw [symbols/s] × min_rtt [s] = symbols (in-flight the
    /// bottleneck rate keeps outstanding over one propagation RTT).
    ///
    /// Gated on ANCHOR_MIN_SAMPLES delivery samples and a min-RTT sample:
    /// `max_bw` is a windowed max of coarse ACK-batch rates with no
    /// per-packet/app-limited accounting, so a handful of samples (or no
    /// RTT floor yet) is not enough to steer cwnd. It structurally
    /// underestimates a warm-up/app-limited flow, which is exactly why it
    /// is only ever used to raise cwnd (recovery target + floor), never as
    /// a cap — an underestimate can only fail to help, never suppress.
    pub(crate) fn bdp_anchor(&self) -> Option<f64> {
        if self.bw_samples.len() < ANCHOR_MIN_SAMPLES || self.max_bw <= 0.0 {
            return None;
        }
        let rtprop = self.min_rtt?.as_secs_f64();
        Some(self.max_bw * rtprop)
    }

    /// The per-path bottleneck rate (symbols/s) for scheduler consumers (the
    /// percap store-cap law reads it via `btlbw_sym_per_s`): the pure
    /// windowed max `max_bw`, gated on ANCHOR_MIN_SAMPLES like `bdp_anchor`
    /// (equal to `bdp_anchor()/RTprop`). A max, not a sub-max quantile:
    /// decode-clocked samples are mostly low, so any sub-max quantile
    /// under-reads and throttles.
    pub(crate) fn effective_btlbw(&self) -> Option<f64> {
        if self.bw_samples.len() < ANCHOR_MIN_SAMPLES {
            return None;
        }
        if self.max_bw > 0.0 { Some(self.max_bw) } else { None }
    }

    /// The cwnd floor from the BtlBw anchor (symbols), or None if not yet
    /// established. A floor, not a cap — it only ratchets cwnd up toward the
    /// pipe, so a stale/underestimated BtlBw cannot suppress the window.
    /// Caller clamps against MAX_CWND.
    pub(crate) fn anchor_floor(&self) -> Option<u32> {
        self.bdp_anchor()
            .map(|bdp| (ANCHOR_FLOOR_GAIN * bdp).round() as u32)
    }

    /// Expire samples older than the sliding window.
    fn expire_old_samples(&mut self, now: Instant) {
        let cutoff = now.checked_sub(self.window_duration).unwrap_or(now);
        self.bw_evict_before(cutoff);
        while self.rtt_samples.front().is_some_and(|s| s.timestamp < cutoff) {
            self.rtt_samples.pop_front();
        }
    }


    /// Test hook: force the O(1) max-filter read (bypasses the
    /// process-global env cache).
    #[cfg(test)]
    pub(crate) fn force_bw_o1(&mut self) {
        self.bw_o1 = true;
    }


    /// Read the current min_rtt estimate (for diagnostics/benchmarking).
    pub fn min_rtt(&self) -> Option<Duration> {
        self.min_rtt
    }


    /// Snapshot of the rate-sample anchor DIAG counters:
    /// (sent, applimited_sent, attributions, no_record, rej_interval, rej_zero,
    /// rej_applimited, generated, bw_fill).  Observation only.
    pub(crate) fn rs_diag(&self) -> (u64, u64, u64, u64, u64, u64, u64, u64, usize) {
        (
            self.rs_sent_count,
            self.rs_applimited_sent,
            self.rs_attributions,
            self.rs_no_record,
            self.rs_rej_interval,
            self.rs_rej_zero,
            self.rs_rej_applimited,
            self.rs_generated,
            self.bw_samples.len(),
        )
    }

    // ---- Readouts (design §2): pure functions of the state and the dial --------

    /// `s` stamps: when the last echo sample, wire mirror and delivery-rate
    /// sample arrived. Observation only.
    pub fn echo_at(&self) -> Option<Instant> {
        self.e_at
    }
    pub fn wire_at(&self) -> Option<Instant> {
        self.w_at
    }
    pub fn rate_at(&self) -> Option<Instant> {
        self.b_at
    }

    /// L = b̂ (design §2 `rate()`): the 10 s windowed max of delivery samples,
    /// wire symbols per second, gated at `ANCHOR_MIN_SAMPLES`. Today's
    /// `effective_btlbw`: no app-limited rule (design §3 / §6 q2 are W3.3's).
    pub fn rate(&self) -> Option<f64> {
        self.effective_btlbw()
    }

    /// Ŵ_i(δ) = (1 + q(δ)) · b̂ · m̂ — the store law's per-leg term (design
    /// §2), in wire symbols. The multiplier is the store law's own
    /// ([`crate::net::pool_value_multiplier`] with `delta_cap` on, the shipped
    /// form), read at `b(δ)`, so the two cannot drift. `None` until the anchor
    /// is warm (the cold chains are Phase B's).
    pub fn window(&self, delta: f64) -> Option<f64> {
        self.bdp_anchor().map(|bdp| {
            crate::net::pool_value_multiplier(true, crate::net::delta_budget_b_of(delta), 1.0)
                * bdp
        })
    }

    /// The ring's minimum, in µs (`min(ring)`, design §3): over the ring's own
    /// span (`SIGMA_CAND_WINDOW` entries, decimated at m̂/8), not m̂'s 10 s.
    pub fn ring_min(&self) -> Option<u32> {
        self.rtt_tlag.iter().map(|&(_, v)| v).min()
    }

    /// The ring's median, in µs (`p50(ring)`), nearest-rank like the other
    /// dispersion readouts.
    pub fn ring_p50(&self) -> Option<u32> {
        if self.rtt_tlag.is_empty() {
            return None;
        }
        let mut v: Vec<u32> = self.rtt_tlag.iter().map(|&(_, x)| x).collect();
        v.sort_unstable();
        Some(crate::monitor::quantile::nearest_rank(&v, 0.50))
    }

    /// S = p50(ring) − min(ring), µs, both on the ring's horizon (design §3;
    /// whether the time term reads this or `p50(ring) − m̂`, and the idle
    /// horizon, are W3.3's). `None` on an empty ring; m̂ is [`Self::min_rtt`].
    pub fn spread(&self) -> Option<u32> {
        Some(self.ring_p50()? - self.ring_min()?)
    }
}

/// The `[LEG]` bind gauge's accumulators (design §4 A4; `RWM_DIAG` only,
/// observation only): per report tick, how Ŵ_i would have read against the
/// window the engine uses. Counted over ticks, so "fraction of time" is a
/// fraction of the 2 s ticks.
#[derive(Debug, Default, Clone)]
pub struct LegGauge {
    /// Ticks observed, and those where Ŵ was measured (warm anchor).
    pub n: u64,
    pub n_w: u64,
    /// Ticks with I > Ŵ (of `n_w`) and with I > cwnd (of `n`).
    pub i_gt_w: u64,
    pub i_gt_cwnd: u64,
    /// Ticks (of `n_w`) where `available() > 0` disagrees with I < Ŵ.
    pub avail_disagree: u64,
}

impl LegGauge {
    pub fn observe(&mut self, cwnd: u32, in_flight: u32, w: Option<f64>) {
        self.n += 1;
        if in_flight > cwnd {
            self.i_gt_cwnd += 1;
        }
        if let Some(w) = w {
            self.n_w += 1;
            let below_w = (in_flight as f64) < w;
            if in_flight as f64 > w {
                self.i_gt_w += 1;
            }
            if (cwnd > in_flight) != below_w {
                self.avail_disagree += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoothed_rtt_seeds_blends_and_reseeds() {
        let ms = Duration::from_millis;
        let mut s = SmoothedRtt::new();
        assert_eq!((s.measured(), s.or_default(), s.cadence()), (None, DEFAULT_SRTT, DEFAULT_SRTT));
        s.record(ms(200));
        assert_eq!(s.measured(), Some(ms(200)), "the first sample IS the estimate");
        s.record(ms(100));
        assert_eq!(s.measured(), Some(ms(187) + Duration::from_micros(500)), "7/8·200 + 1/8·100");
        // Revival: the stale value stays readable, the controller's view is
        // the default, and the next sample replaces rather than blends.
        s.restart();
        assert_eq!(s.measured(), Some(ms(187) + Duration::from_micros(500)));
        assert_eq!(s.or_default(), ms(187) + Duration::from_micros(500));
        assert_eq!(s.cadence(), DEFAULT_SRTT);
        s.record(ms(40));
        assert_eq!((s.measured(), s.cadence()), (Some(ms(40)), ms(40)));
    }

    // ---- A4 readouts ------------------------------------------------------------

    use crate::scheduler::MockClock;

    /// A warm leg: RTprop 20 ms, 9 delivery samples of exactly 1000 sym/s, so
    /// b̂ = 1000 and BDP = b̂·m̂ = 20 symbols.
    fn warm_leg() -> (Arc<MockClock>, LegState) {
        let clock = Arc::new(MockClock::new());
        let mut leg = LegState::new(clock.clone());
        leg.record_sample(Duration::from_millis(20), None);
        for _ in 0..9 {
            clock.advance(Duration::from_millis(10));
            leg.record_delivery(10);
        }
        (clock, leg)
    }

    #[test]
    fn window_is_one_plus_q_of_delta_times_the_bdp_at_the_presets() {
        use crate::control::fec_rate::ProtocolHint::*;
        let (_c, leg) = warm_leg();
        let bdp = leg.bdp_anchor().expect("warm");
        assert!((bdp - 20.0).abs() < 1e-6, "b̂·m̂ = 1000 × 20 ms: {bdp}");
        assert!((leg.rate().unwrap() - 1000.0).abs() < 1e-6);
        // Absolute values: Ŵ = (1 + q)·BDP, q = (b + 1)/30 at b = 1/2, 1, 2.
        for (hint, want) in [(Realtime, 21.0), (Auto, 20.0 * (1.0 + 2.0 / 30.0)), (Bulk, 22.0)] {
            let d = crate::scheduler::hint_delta_price(hint);
            let w = leg.window(d).unwrap();
            assert!((w - want).abs() < 1e-6, "{hint:?}: Ŵ = {w}, want {want}");
        }
        // The store law's own multiplier, not a transcription of it.
        let d = crate::scheduler::hint_delta_price(Auto);
        let m = crate::net::pool_value_multiplier(true, crate::net::delta_budget_b_of(d), 1.0);
        assert_eq!(leg.window(d).unwrap(), m * bdp);
    }

    /// Continuity and monotonicity at ±2 % around every preset (the
    /// `delta_budget_b_is_the_dial_not_a_mode` pattern): Ŵ never steps, and
    /// never rises as δ (the price of lateness) rises.
    #[test]
    fn window_is_continuous_and_monotone_through_every_preset() {
        use crate::control::fec_rate::ProtocolHint::*;
        let (_c, leg) = warm_leg();
        for hint in [Realtime, Auto, Bulk] {
            let d = crate::scheduler::hint_delta_price(hint);
            let (lo, mid, hi) = (
                leg.window(d * 0.98).unwrap(),
                leg.window(d).unwrap(),
                leg.window(d * 1.02).unwrap(),
            );
            assert!(lo >= mid && mid >= hi, "{hint:?}: non-increasing in δ: {lo} {mid} {hi}");
            assert!((lo - hi) / mid < 0.01, "{hint:?}: no step across the preset: {lo} {hi}");
        }
    }

    #[test]
    fn window_is_none_until_the_anchor_is_warm() {
        let clock = Arc::new(MockClock::new());
        let mut leg = LegState::new(clock.clone());
        assert_eq!((leg.window(0.5), leg.rate()), (None, None));
        leg.record_sample(Duration::from_millis(20), None);
        for _ in 0..7 {
            clock.advance(Duration::from_millis(10));
            leg.record_delivery(10);
        }
        assert_eq!(leg.window(0.5), None, "7 samples: below ANCHOR_MIN_SAMPLES");
        clock.advance(Duration::from_millis(10));
        leg.record_delivery(10);
        assert!(leg.window(0.5).is_some());
    }

    /// `spread` = p50(ring) − min(ring); the three parts are exposed
    /// separately, with m̂ (`min_rtt`) beside them.
    #[test]
    fn spread_is_p50_minus_min_of_the_ring() {
        let clock = Arc::new(MockClock::new());
        let mut leg = LegState::new(clock.clone());
        assert_eq!((leg.ring_min(), leg.ring_p50(), leg.spread()), (None, None, None));
        // RTprop 10 ms ⇒ one ring entry per 1.25 ms.
        for us in [10_000u64, 12_000, 18_000, 11_000, 30_000] {
            clock.advance(Duration::from_millis(2));
            leg.record_sample(Duration::from_micros(us), None);
        }
        assert_eq!(leg.ring_min(), Some(10_000));
        assert_eq!(leg.ring_p50(), Some(12_000), "sorted 10, 11, 12, 18, 30 ms");
        assert_eq!(leg.spread(), Some(2_000));
        assert_eq!(leg.min_rtt(), Some(Duration::from_millis(10)));
    }

    /// The `*_at` stamps: each is the clock at its feed; a revival clears the
    /// echo and rate stamps and keeps the wire's with the wire RTT.
    #[test]
    fn stamps_follow_their_feeds_and_the_wire_stamp_survives_revival() {
        let clock = Arc::new(MockClock::new());
        let mut leg = LegState::new(clock.clone());
        assert_eq!((leg.echo_at(), leg.wire_at(), leg.rate_at()), (None, None, None));
        let t0 = clock.now();
        clock.advance(Duration::from_millis(5));
        leg.record_sample(Duration::from_millis(20), None);
        assert_eq!(leg.echo_at(), Some(t0 + Duration::from_millis(5)));
        clock.advance(Duration::from_millis(5));
        leg.set_wire(Duration::from_millis(18));
        assert_eq!(leg.wire_at(), Some(t0 + Duration::from_millis(10)));
        clock.advance(Duration::from_millis(5));
        leg.record_delivery(10);
        assert_eq!(leg.rate_at(), Some(t0 + Duration::from_millis(15)));
        leg.reset();
        assert_eq!((leg.echo_at(), leg.rate_at()), (None, None));
        assert_eq!(leg.wire_at(), Some(t0 + Duration::from_millis(10)));
        assert_eq!(leg.wire(), Some(Duration::from_millis(18)));
    }

    /// The `[LEG]` accumulators at their anchor points.
    #[test]
    fn leg_gauge_counts_each_disagreement() {
        let mut g = LegGauge::default();
        g.observe(10, 5, None); // cold: only the cwnd side counts
        g.observe(10, 12, None); // I > cwnd
        g.observe(10, 5, Some(8.0)); // I < both: agree
        g.observe(10, 9, Some(8.0)); // cwnd has room, Ŵ does not: disagree, I > Ŵ
        g.observe(6, 9, Some(20.0)); // cwnd full, Ŵ has room: disagree, I > cwnd
        assert_eq!((g.n, g.n_w), (5, 3));
        assert_eq!((g.i_gt_cwnd, g.i_gt_w, g.avail_disagree), (2, 1, 2));
    }

    /// The `[LEG]` gauge is absent with `RWM_DIAG` off — no line and no
    /// observation, so the default path does no work for it — and present with
    /// it on, one line per leg in id order. (No gate was added: the `[GATES]`
    /// pin in `gates/tests.rs` is untouched.)
    #[test]
    fn leg_gauge_is_absent_with_the_gate_off_and_one_line_per_leg_with_it_on() {
        let clock = Arc::new(MockClock::new());
        let mut sched = crate::scheduler::Scheduler::new(clock);
        sched.add_path(1);
        sched.add_path(0);
        assert!(sched.leg_gauge_tick(false).is_empty());
        assert!(sched.path(0).unwrap().leg_line(0.5).contains(" n=0 "), "gate off: nothing observed");
        let lines = sched.leg_gauge_tick(true);
        assert_eq!(
            lines,
            vec![
                "[LEG] p0 cwnd=10 W=- I=0 n=1 nW=0 i_gt_W=- i_gt_cwnd=0.000 avail_x_W=- leak=0/0.00/0.00",
                "[LEG] p1 cwnd=10 W=- I=0 n=1 nW=0 i_gt_W=- i_gt_cwnd=0.000 avail_x_W=- leak=0/0.00/0.00",
            ]
        );
    }
}
