//! Per-path scheduler state (`PathState`): cwnd/in-flight accounting, the
//! loss/RTT estimators, the anchors and the per-path gauges. Moved verbatim
//! out of `scheduler/mod.rs` (cleanup Stage 3); re-exported from
//! `scheduler`.

use super::*;

/// Per-path state tracked by the scheduler.
pub struct PathState {
    pub id: PathId,
    pub estimator: LossEstimator,
    /// Congestion window in symbols
    pub cwnd: u32,
    /// Symbols currently in flight
    pub in_flight: u32,
    /// Per-path SOURCE outstanding gauge (BLEST in_flight_i, feat/per-path-
    /// estimator): source symbols whose DAPS placement committed them to THIS
    /// path (`source_path_map`) but which the receiver has not yet
    /// acked/decoded.  Charged at placement (`charge_src`) and released on
    /// per-path ack attribution (`on_src_delivered`), so it tracks TRUE
    /// sent-not-acked ON THIS PATH — the quantity the BLEST BDP cap bounds
    /// (`in_flight_i ≤ gain·BtlBw_i·RTprop_i`).  Distinct from `in_flight`
    /// (the coded-symbol budget released by time-expiry): the cap needs a
    /// source-unit outstanding that matches the source-unit BtlBw the ack
    /// attribution feeds `copa.record_delivery`.  Only driven under DAPS.
    pub src_inflight: u32,
    /// Whether the path is considered usable
    pub active: bool,
    /// Slow-start threshold (kept for legacy test compatibility)
    pub ssthresh: u32,
    /// Whether we are in slow-start phase (Copa startup)
    pub in_slow_start: bool,
    /// Last time we received an RTCP-style report or any data from this path
    pub last_report: Instant,
    /// Maximum datagram size discovered for this path
    pub max_datagram_size: Option<usize>,
    /// Copa delay-based congestion control state.
    pub(crate) copa: CopaState,
    /// Token-bucket pacing: symbols sendable right now. Replenished at
    /// cwnd/SRTT symbols per second, capped at the burst allowance
    /// max(10, cwnd/8). May go NEGATIVE: the drain in net/mod.rs is
    /// batch-granular and lets the final batch overdraft; the debt is
    /// repaid before the next drain, so the average rate stays cwnd/SRTT.
    pub(crate) pace_tokens: f64,
    /// Last time pacing tokens were replenished.
    pub(crate) last_pace_refill: Instant,
    /// FIFO log of in_flight charges (charge instant, symbols) backing the
    /// time-based release in `expire_in_flight`. Invariant (best-effort):
    /// sum of counts == in_flight; direct writes to `in_flight` (tests,
    /// the leak-guard backstop) break it temporarily and all helpers
    /// saturate rather than trust it.
    pub(crate) in_flight_log: VecDeque<(Instant, u32)>,
    /// Pool-anchor honest dual-store law (`RWM_POOL_ANCHOR`, goal-gate
    /// "Ship The Wins 1"): per-path hygiene-grade SEND-interval rate anchor
    /// (ADR-0061 `SendRateAnchor`: ≈SRTT/2 buckets, windowed-max ≈ 8·SRTT,
    /// clock-gap discard + quarantine), fed by this path's own send events
    /// at `charge_in_flight` — every wire send on the path (source,
    /// redundant, retransmit). Burst-immune by construction (Δt spans the
    /// SEND interval on the sender's clock), it is the N ≥ 2 pooled-store
    /// cap's rate input; it feeds NOTHING else (Copa cwnd dynamics keep the
    /// legacy `record_delivery` path byte-identically — the −22…−27 c7
    /// RS-composition price stays unreachable).
    send_anchor: crate::control::SendRateAnchor,
    /// Whether the send-anchor feed is on (resolved once at construction
    /// from `pool_anchor_active()`; test-forcible). OFF ⇒ `charge_in_flight`
    /// is byte-identical to the prior-default path (no clock read, no
    /// bucket work) — the A/B decomposition arm stays cost-honest.
    pool_anchor_feed: bool,
    /// ack-merge (`RWM_ACK_MERGE`, goal-gate "Unlock The Default 1"): the
    /// sender-side CURSOR for the v6 `WindowAck` cumulative counters. The
    /// merged ack carries the receiver's per-path running
    /// `(total_expected, total_received)`; the sender diffs them against
    /// these to recover exactly the `(expected_count, received_count)` pair
    /// the suppressed legacy `Ack` used to deliver per batch. Cumulative and
    /// diffed rather than per-ack sums so a DROPPED control datagram costs
    /// nothing — the next ack carries the whole outstanding delta, which is
    /// the property that makes merging safe on a lossy ack path.
    ack_cum_expected: u64,
    /// See [`Self::ack_cum_expected`].
    ack_cum_received: u64,
    /// `RWM_LOSS_SENT_TRUTH` (default OFF): the SENDER-side cursor over this
    /// path's own `PathStats::symbols_sent`. See
    /// [`Self::sender_truth_loss_delta`].
    loss_sent_cursor: u64,
    /// THE ONE-SIDED-CLAMP WITNESS (paper §16.63's successor hypothesis,
    /// observation only). Counts of samples where the receiver's cumulative
    /// cursor LED the sender's own symbol counter, their summed magnitude, and
    /// the positive loss mass actually fed — see [`Self::loss_clamp_witness`].
    loss_clamp_over_n: u64,
    loss_clamp_over_mass: u64,
    loss_clamp_loss_mass: u64,
    /// `RWM_LOSS_SENT_TRUTH`: the paired cursor over the receiver's clean
    /// per-path `total_received`. Separate from [`Self::ack_cum_received`]
    /// because the two arms advance independently (the legacy cursor pair
    /// keeps driving `release_in_flight` in BOTH arms — see
    /// [`Self::sender_truth_loss_delta`]).
    loss_recv_cursor: u64,
    /// THE REFUTED CANDIDATE's cursor pair (see
    /// [`Self::sender_truth_release_delta`]) — retained as the negative
    /// datum's only reproduction path, with no production call site.
    release_sent_cursor: u64,
    /// See [`Self::release_sent_cursor`].
    release_recv_cursor: u64,
    /// `RWM_RELEASE_1TO1` (default OFF, resolved once at construction): the
    /// lost-symbol release is the charge log's OWN RFC 9002 time-threshold
    /// sweep, and the contaminated `expected - received` term at the ack arms
    /// is not applied. See [`release_1to1_active`] and
    /// [`Self::expire_in_flight`].
    release_1to1: bool,
    /// Injectable clock
    clock: Arc<dyn Clock>,
}

impl PathState {
    /// Minimum congestion window in symbols (never go below this).
    /// 8 rather than the historical 2: an L1 run on a real emulated link
    /// showed the old collapse-to-target dynamics crawling at 2 symbols/RTT
    /// after the first burst; the floor guarantees a usable trickle that
    /// keeps RTT samples (and thus recovery) flowing.
    pub const MIN_CWND: u32 = 8;
    /// Initial congestion window.
    pub const INITIAL_CWND: u32 = 10;
    /// Maximum congestion window.
    pub const MAX_CWND: u32 = 10_000;
}

impl PathState {
    pub fn new(id: PathId, clock: Arc<dyn Clock>) -> Self {
        Self::new_with_hint(id, clock, ProtocolHint::Auto)
    }

    /// Create path state with a protocol hint (sets Copa-lite's
    /// hint-coupled queue target, paper Section 12.4 / P1).
    pub fn new_with_hint(id: PathId, clock: Arc<dyn Clock>, hint: ProtocolHint) -> Self {
        let now = clock.now();
        Self {
            id,
            estimator: LossEstimator::new(),
            cwnd: Self::INITIAL_CWND,
            in_flight: 0,
            src_inflight: 0,
            active: true,
            ssthresh: 64,
            in_slow_start: true,
            last_report: now,
            max_datagram_size: None,
            copa: {
                let mut c = CopaState::new(clock.clone(), hint);
                c.rs_trace_path = id; // DIAG label only (RWM_RS_TRACE prints)
                c
            },
            pace_tokens: Self::INITIAL_CWND as f64,
            last_pace_refill: now,
            in_flight_log: VecDeque::new(),
            send_anchor: crate::control::SendRateAnchor::new(),
            pool_anchor_feed: pool_anchor_active(),
            ack_cum_expected: 0,
            ack_cum_received: 0,
            loss_sent_cursor: 0,
            loss_clamp_over_n: 0,
            loss_clamp_over_mass: 0,
            loss_clamp_loss_mass: 0,
            loss_recv_cursor: 0,
            release_sent_cursor: 0,
            release_recv_cursor: 0,
            release_1to1: release_1to1_active(),
            clock,
        }
    }

    /// `RWM_LOSS_SENT_TRUTH` (default OFF) — the CROSS-PATH-CLEAN loss pair.
    ///
    /// THE LAW, on one line:
    ///
    /// ```text
    ///   eps_p  =  1  -  d(cum_received_p) / d(symbols_sent_p)
    /// ```
    ///
    /// Provenance of both operands: **measured, locally, per path.**
    /// `symbols_sent_p` is `PathStats::symbols_sent` — incremented once at
    /// every wire handoff on this path (source, repair and retransmit alike;
    /// `emit_source.rs:489/581/933`, `net/mod.rs:5735/5792/5906/7282/7722`),
    /// so it is the SENDER's own exact count of what it put on this path.
    /// `cum_received_p` is the receiver's `PathBatchTracker::total_received`,
    /// already on the wire in every v6 `WindowAck` — a pure count of arrivals
    /// on this path, with no sequence arithmetic in it.
    ///
    /// **What it replaces and why.** The shipped pair takes `expected` from
    /// `PathBatchTracker::total_expected` (`net/mod.rs:7576`), which estimates
    /// it as `gap × received` across a **GLOBAL** `batch_seq` gap
    /// (`batch_counter` is one connection-wide `AtomicU64`). At N ≥ 2 a single
    /// path's batch-seq sequence is mostly the OTHER path's symbols, so the
    /// gap is a SCHEDULING artefact and the ratio reads loss that never
    /// happened. Measured on the wire (goal-gate "Ack-Cadence Measurement
    /// (VM)" READOUT 4): `ce/cr` = 2.05 at c7 and 5.59 on c8's slow leg
    /// against realized packet loss of 0.55% and 1.96% — i.e. eps_hat 0.51
    /// and 0.82 against truth, **37–93x**. The same ledgers' `cr/s` column is
    /// exactly this law's reciprocal and reads 0.94–1.01 at those cells.
    ///
    /// **Why deltas of cumulatives and not a snapshot ratio.** Both operands
    /// are monotone cumulative counters, so a dropped ack costs nothing (the
    /// next one carries the whole outstanding delta) — the same property that
    /// makes [`Self::ack_merge_counter_delta`] safe. The cursors only ever
    /// move FORWARD, so a reordered/stale ack yields `(0, 0)`.
    ///
    /// **The named residual: in-flight lag.** `symbols_sent` counts a symbol
    /// at handoff, `cum_received` counts it ~RTT later, so the sent cursor
    /// leads by ≈ in_flight. The offset is CONSTANT in steady state, hence
    /// the DELTAS are unbiased; what it costs is a one-BDP over-read during
    /// the opening ramp (decaying, and in the same direction the legacy pair
    /// errs, so it is never a new over-read) and a matching under-read at the
    /// tail. Bounded by `sender_truth_loss_delta_is_unbiased_under_a_constant_
    /// in_flight_lag`. Subtracting `in_flight` here would remove the offset
    /// but couple this estimate to a gauge whose release is driven by the
    /// contaminated pair — the circularity is deliberately not taken.
    ///
    /// `cum_received == 0` is the same "no counter payload" sentinel the
    /// merged-ack cursor uses (the two timer-driven `WindowAck` sites
    /// broadcast to every live path and carry no per-path counter).
    /// `received` is clamped to `expected` so the derived loss count can
    /// never underflow when the lag runs the other way.
    pub fn sender_truth_loss_delta(
        &mut self,
        symbols_sent: u64,
        cum_received: u64,
    ) -> (u32, u32) {
        if cum_received == 0 {
            return (0, 0);
        }
        let d_expected = symbols_sent.saturating_sub(self.loss_sent_cursor);
        let d_received = cum_received.saturating_sub(self.loss_recv_cursor);
        if d_expected == 0 && d_received == 0 {
            return (0, 0);
        }
        self.loss_sent_cursor = self.loss_sent_cursor.max(symbols_sent);
        self.loss_recv_cursor = self.loss_recv_cursor.max(cum_received);
        // ── THE ONE-SIDED-CLAMP WITNESS (observation only) ───────────────
        // Goal-gate item 3c, REDIRECTED: the RFC 6675 denominator hypothesis
        // was refuted on the code (both operands count retransmits — a matched
        // pair). The labelled successor hypothesis for the T rung's 20 %
        // over-read, and for why it SURVIVES at N = 1 where the attribution
        // error it was built to repair cannot exist, is THIS `min`: two clocks
        // (the sender's own symbol counter and the receiver's cumulative echo)
        // jitter against each other, so `d_received > d_expected` whenever the
        // receiver's cursor momentarily leads. The clamp RECTIFIES every such
        // sample to zero loss rather than to negative loss — and rectifying a
        // zero-mean jitter is a POSITIVE BIAS at any path count, which is
        // exactly the shape of the surviving-at-N=1 result.
        //
        // Three counters, exactly what scoring the hypothesis needs:
        //   (a) how often the receiver led, (b) by how much summed,
        //   (c) the positive loss mass fed, for the ratio.
        // No behaviour change and no wire change: the clamp is untouched and
        // nothing here is read by a decision.
        if d_received > d_expected {
            LCW_OVER_N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            LCW_OVER_MASS.fetch_add(d_received - d_expected, std::sync::atomic::Ordering::Relaxed);
            self.loss_clamp_over_n = self.loss_clamp_over_n.saturating_add(1);
            self.loss_clamp_over_mass =
                self.loss_clamp_over_mass.saturating_add(d_received - d_expected);
        }
        LCW_LOSS_MASS.fetch_add(
            d_expected.saturating_sub(d_received.min(d_expected)),
            std::sync::atomic::Ordering::Relaxed,
        );
        self.loss_clamp_loss_mass = self
            .loss_clamp_loss_mass
            .saturating_add(d_expected.saturating_sub(d_received.min(d_expected)));
        let cap = u32::MAX as u64;
        (
            d_expected.min(cap) as u32,
            d_received.min(d_expected).min(cap) as u32,
        )
    }

    /// The one-sided-clamp witness, whole — `(samples where the receiver's
    /// cursor LED, their summed magnitude, the positive loss mass fed)`.
    ///
    /// The scoreable statistic is `over_mass / loss_mass`: if two-clock jitter
    /// rectification is the mechanism behind §16.63's 20×-and-survives-at-N=1
    /// result, the rectified mass is a large fraction of the loss the
    /// estimator was fed, at EVERY path count including N = 1. Surfaced on the
    /// DIAG/ACKDIAG line as `lcw=<n>/<over_mass>/<loss_mass>`.
    pub fn loss_clamp_witness(&self) -> (u64, u64, u64) {
        (self.loss_clamp_over_n, self.loss_clamp_over_mass, self.loss_clamp_loss_mass)
    }

    /// [`Self::sender_truth_loss_delta`] for the LEGACY per-batch `Ack` arm,
    /// whose `received_count` is a per-batch count rather than a cumulative
    /// counter. Same law, same cursors: the received side is accumulated
    /// here instead of arriving pre-summed on the wire. (`received_count` was
    /// never the contaminated operand — only `expected_count` was.)
    pub fn sender_truth_loss_batch(
        &mut self,
        symbols_sent: u64,
        received_in_batch: u32,
    ) -> (u32, u32) {
        let cum = self.loss_recv_cursor.saturating_add(received_in_batch as u64);
        self.sender_truth_loss_delta(symbols_sent, cum)
    }

    /// **THE REFUTED CANDIDATE — retained as the negative datum's only
    /// reproduction path, with NO production call site.**
    ///
    /// The shape the `fix/accounting-ledger` dispatch proposed for the
    /// lost-symbol release: the same clean operand pair
    /// `RWM_LOSS_SENT_TRUTH` gave the loss ESTIMATOR, applied to the LEDGER —
    ///
    /// ```text
    ///   released_lost_p  =  d(symbols_sent_p)  -  d(cum_received_p)
    /// ```
    ///
    /// **It is refuted ARITHMETICALLY, and the identity is short enough to
    /// state here.** Charge every send and release `d_received` (delivery arm)
    /// plus this term, and the sums telescope:
    ///
    /// ```text
    ///   in_flight  =  sent      -  [ recv + (sent - sent_0) - (recv - recv_0) ]
    ///              =  sent_0 - recv_0
    /// ```
    ///
    /// a CONSTANT — the outstanding at cursor init, which with cursors
    /// starting at zero is **zero**. The gauge is pinned on the floor exactly
    /// as the contaminated `expected - received` pins it, so the defect is
    /// reproduced rather than fixed, and lazily initialising the cursors only
    /// freezes the gauge at a different constant.
    ///
    /// The reason is structural, not a tuning failure: `d_sent - d_received`
    /// is `loss + delta(outstanding)`, so releasing on it releases the
    /// in-flight window itself. **Item 3's trick works for a RATIO** — where
    /// a constant lag cancels in the deltas and leaves the estimate unbiased —
    /// **and does not transfer to a LEDGER**, which needs the per-symbol
    /// identity that the striping destroyed (item 3's own candidate (b): "a
    /// seq that arrived NOWHERE cannot be attributed to a path").
    ///
    /// Reproduced and bounded by `sender_truth_release_pins_the_gauge_on_the_
    /// floor`; the shipped shape is [`release_1to1_active`].
    ///
    /// Cursor mechanics, for the reproduction: `cum_received == 0` is the "no
    /// counter payload" sentinel; cursors only move FORWARD, so a reordered or
    /// duplicated ack yields 0; and the lost count saturates at zero rather
    /// than going negative, so it can never invent budget.
    pub fn sender_truth_release_delta(
        &mut self,
        symbols_sent: u64,
        cum_received: u64,
    ) -> u32 {
        if cum_received == 0 {
            return 0;
        }
        let d_sent = symbols_sent.saturating_sub(self.release_sent_cursor);
        let d_received = cum_received.saturating_sub(self.release_recv_cursor);
        if d_sent == 0 && d_received == 0 {
            return 0;
        }
        self.release_sent_cursor = self.release_sent_cursor.max(symbols_sent);
        self.release_recv_cursor = self.release_recv_cursor.max(cum_received);
        d_sent.saturating_sub(d_received).min(u32::MAX as u64) as u32
    }

    /// [`Self::sender_truth_release_delta`] for the LEGACY per-batch `Ack`
    /// arm, whose `received_count` is a per-batch count rather than a
    /// cumulative counter. Same law, same cursors: the received side is
    /// accumulated here instead of arriving pre-summed on the wire.
    pub fn sender_truth_release_batch(
        &mut self,
        symbols_sent: u64,
        received_in_batch: u32,
    ) -> u32 {
        let cum = self
            .release_recv_cursor
            .saturating_add(received_in_batch as u64);
        self.sender_truth_release_delta(symbols_sent, cum)
    }

    /// ack-merge (`RWM_ACK_MERGE`): advance the v6 cumulative-counter cursor
    /// and return this ack's `(expected, received)` delta — exactly the pair
    /// the suppressed legacy `ControlMessage::Ack` carried per batch.
    ///
    /// `cum_received == 0` is the "no counter payload" sentinel (the two
    /// timer-driven `WindowAck` sites broadcast one message to every live
    /// path and cannot carry a per-path counter), and a reordered/stale ack
    /// yields `(0, 0)` because the cursor only ever moves FORWARD. Both cases
    /// are no-ops, which is what makes the re-homed consumers idempotent
    /// under ack loss, duplication and reordering.
    ///
    /// `received` is clamped to `expected`: the receiver's `expected` is a
    /// batch-gap ESTIMATE (`PathBatchTracker`), so a shrinking gap estimate
    /// must never make the derived loss count underflow.
    pub fn ack_merge_counter_delta(&mut self, cum_expected: u64, cum_received: u64) -> (u32, u32) {
        if cum_received == 0 {
            return (0, 0);
        }
        let d_expected = cum_expected.saturating_sub(self.ack_cum_expected);
        let d_received = cum_received.saturating_sub(self.ack_cum_received);
        if d_received == 0 && d_expected == 0 {
            return (0, 0);
        }
        self.ack_cum_expected = self.ack_cum_expected.max(cum_expected);
        self.ack_cum_received = self.ack_cum_received.max(cum_received);
        let cap = u32::MAX as u64;
        (
            d_expected.min(cap) as u32,
            d_received.min(d_expected).min(cap) as u32,
        )
    }

    /// Update the hint-coupled queue target when the protocol hint changes.
    pub fn set_hint(&mut self, hint: ProtocolHint) {
        self.copa.set_queue_mult(queue_target_mult(hint));
        self.copa.set_hint_delta(hint);
    }

    /// Correction rate r = epsilon / (1 - epsilon).
    /// The (1-epsilon) denominator accounts for corrections-of-corrections.
    /// See paper Section 13.4.
    pub fn correction_rate(&self) -> f64 {
        let eps = self.estimator.loss_rate();
        if eps >= 1.0 {
            return f64::INFINITY;
        }
        eps / (1.0 - eps)
    }

    /// Effective delivery time E_i = RTT_i/2 + epsilon_i × t_recovery_i.
    ///
    /// t_recovery is the expected time to recover a lost symbol. We approximate
    /// it as one RTT (ARQ round-trip) weighted by loss probability. When FEC
    /// is likely to recover (low loss), t_recovery is small. When ARQ is needed
    /// (high loss or aged symbol), t_recovery approaches one full RTT.
    ///
    /// See paper Section 13.5.
    pub fn effective_delivery_time(&self) -> f64 {
        let rtt_secs = self.estimator.rtt().as_secs_f64();
        let eps = self.estimator.loss_rate();
        // t_recovery ≈ RTT (one round-trip for ARQ recovery)
        let t_recovery = rtt_secs;
        rtt_secs / 2.0 + eps * t_recovery
    }

    /// Load-DEPENDENT expected frontier-completion-time `E_i(load)` (seconds) —
    /// the always-on load term of the RWM placement law (paper Section 16.3).
    /// The time a symbol handed to this path now takes to reach the receiver:
    ///
    ///   E_i(load) = in_flight_i / (cwnd_i/SRTT_i)   ← drain the current backlog
    ///             + SRTT_i / 2                        ← one-way propagation
    ///             + eps_i · RTT_i                     ← expected loss recovery
    ///
    /// The queue term uses the path's live PACING RATE (`cwnd/SRTT`), so a
    /// backlog on a low-capacity / high-RTT path costs proportionally MORE real
    /// time than the same backlog on the fast path — this is what makes the law
    /// water-fill by CAPACITY (arrival rate matches drain rate at equilibrium),
    /// not by equal window-fraction (which over-loads the slow path and, on a
    /// reliable in-order stream, collapses the frontier — MEASURED at C8:
    /// dimensionless fill gave 3.4 Mbit/s vs 15.4 fast-path-alone). It rises
    /// CONTINUOUSLY with `in_flight` (past cwnd under overdraft), so spillover
    /// is a smooth equilibrium, not a regime switch. Because it is the delivery
    /// latency of a reliable in-order stream (the completion cost itself), it
    /// carries UNIT weight independent of the protocol hint.
    pub fn expected_delivery_load(&self) -> f64 {
        self.expected_delivery_load_at(self.srtt().as_secs_f64())
    }

    /// `expected_delivery_load` with the path's latency anchor supplied by
    /// the caller, in seconds. Same formula, one free variable: `E_i` is
    /// linear in SRTT_i, and the ONLY thing `RWM_COLD_PLACE` changes is which
    /// measurement stands in for SRTT_i on a leg that has never had a sample.
    /// `expected_delivery_load()` is this at `srtt()`, so no caller of the
    /// nullary form can observe a difference.
    pub fn expected_delivery_load_at(&self, srtt: f64) -> f64 {
        let eps = self.estimator.loss_rate();
        let cwnd = self.cwnd.max(1) as f64;
        let queue_wait = (self.in_flight as f64 / cwnd) * srtt;
        queue_wait + srtt / 2.0 + eps * srtt
    }

    /// Effective goodput: throughput * (1 - loss_rate).
    /// This is what actually gets through to the receiver.
    pub fn effective_goodput(&self) -> f64 {
        let throughput = self.estimator.throughput();
        let loss = self.estimator.loss_rate();
        throughput * (1.0 - loss)
    }

    /// Available capacity: cwnd - in_flight.
    pub fn available(&self) -> u32 {
        self.cwnd.saturating_sub(self.in_flight)
    }

    /// Spare capacity as a fraction of in-flight traffic.
    ///
    /// Returns `(cwnd - in_flight) / in_flight` when in_flight > 0.
    /// Used by the FEC rate controller to ensure repairs don't exceed
    /// available link capacity (the "never hurts" guarantee).
    ///
    /// Returns f64::INFINITY when in_flight is 0 (unlimited spare capacity).
    pub fn spare_capacity(&self) -> f64 {
        if self.in_flight == 0 {
            return f64::INFINITY;
        }
        self.cwnd.saturating_sub(self.in_flight) as f64 / self.in_flight as f64
    }

    /// Copa-lite congestion control: handle acknowledgements.
    ///
    /// The cwnd update runs once per SRTT (gate driver cadence):
    ///   - windowed-min RTT above the queue target → ×0.92, end ramp
    ///   - ramping (before the first backoff) → ×1.5 + 1
    ///   - steady state → +2
    ///
    /// During the ramp the backoff check additionally runs per ACK, so the
    /// exponential phase ends within one feedback message of the first
    /// standing-queue evidence rather than waiting out the SRTT window.
    pub fn on_ack(&mut self, acked: u32) {
        let _rate = self.copa.record_delivery(acked);
        self.on_delivery_signal();
    }

    /// The cwnd-dynamics half of `on_ack`, WITHOUT the legacy ack-interval
    /// `record_delivery` sample (feat/copa-sole-cc). Callers that account
    /// delivery through the BBR-correct send-interval rate sampler
    /// (`on_src_delivered_seq`) use this so the windowed-max BtlBw filter is
    /// fed ONLY clean send-interval samples — the ack-interval Δt spikes
    /// (batched acks / frontier jumps) otherwise latch an over-read anchor
    /// that pins cwnd above BDP via the anchor floor (§16.13's ×145-class
    /// over-read, reproduced ×19 on the plain-mode L0 smoke). The update
    /// rules themselves are byte-identical to `on_ack`'s.
    pub fn on_delivery_signal(&mut self) {
        let now = self.clock.now();

        if self.copa.ramping
            && self.copa.samples_since_update >= 3
            && self.copa.queue_above_target(self.cwnd)
        {
            // Fast ramp exit: gentle ×0.92, NOT a collapse to a
            // rate-formula target (the pre-P7 bug: the initial burst
            // inflated its own RTT samples, dq exploded, and the target
            // dropped to the floor on the very first burst). Requires
            // ≥3 samples of evidence: a partial window's min can be a
            // single jittery sample, and one draw from the jitter tail
            // must not end the exponential ramp (L1 C2 finding).
            self.cwnd = self.copa.backoff(self.cwnd);
        } else if self.copa.should_update(now) {
            self.cwnd = self.copa.update_cwnd(self.cwnd);
        }

        self.clamp_cwnd_with_anchor();

        // Sync legacy fields
        self.in_slow_start = self.copa.ramping;
        if !self.in_slow_start && self.ssthresh > self.cwnd {
            self.ssthresh = self.cwnd;
        }
    }

    // --- Per-path send-interval rate sampling (the CopaFeed's anchor) ---
    //
    // The plain-mode Copa delivery feed (feat/copa-sole-cc / RWM_PLAIN_RS,
    // ADR-0061) attributes each newly-acked SOURCE seq to the path that
    // carried it and drives that path's BBR-correct send-interval sampler,
    // so BtlBw_i / the per-path BDP anchor establish per path.

    /// Charge `n` source symbols to this path's outstanding gauge at
    /// placement time (the seq→path commitment).  Pairs with
    /// `on_src_delivered_seq`.
    pub fn charge_src(&mut self, n: u32) {
        self.src_inflight = self.src_inflight.saturating_add(n);
    }

    /// BBR `SendPacket` for the rate-sample anchor: record this source seq's
    /// send-time state so its ack yields a send-interval rate sample.
    /// Called at placement/send time; pairs with `on_src_delivered_seq`.
    pub fn on_src_sent(&mut self, seq: u64, app_limited: bool) {
        self.copa.rs_on_sent(seq, app_limited);
    }

    /// Per-path ack attribution under the BBR rate-sample anchor: release the
    /// SOURCE outstanding gauge and feed the delivery-rate max-filter a
    /// SEND-INTERVAL sample (robust to ack-aggregation / a standing queue).
    pub fn on_src_delivered_seq(&mut self, seq: u64) {
        self.src_inflight = self.src_inflight.saturating_sub(1);
        self.copa.rs_on_delivered(seq);
    }

    /// Current per-path SOURCE outstanding (BLEST in_flight_i).
    pub fn src_inflight(&self) -> u32 {
        self.src_inflight
    }

    /// Whether the per-path Copa BtlBw/BDP anchor has established (≥
    /// ANCHOR_MIN_SAMPLES delivered-rate samples AND a min-RTT sample) — the
    /// per-path DIAG "established?" signal.
    pub fn anchor_established(&self) -> bool {
        self.copa.bdp_anchor().is_some()
    }

    /// (diag/slow-path-anchor) Per-path rate-sample anchor DIAG counters:
    /// (snapshotted-at-send, of-which-app-limited, acks-attributed-here,
    /// no-send-record, rejected[interval<MinRTT], rejected[zero-delivered],
    /// rejected[app-limited], samples-generated, windowed-max-fill).
    /// Read only under RWM_DIAG; never gates control.
    pub fn rs_diag(&self) -> (u64, u64, u64, u64, u64, u64, u64, u64, usize) {
        self.copa.rs_diag()
    }

    /// Copa-lite congestion control: handle loss events.
    ///
    /// Loss alone does NOT reduce cwnd — channel loss is FEC's job, not
    /// CC's (paper Section 12). The key insight:
    ///   - Loss + FEC recovered → wireless/random loss → ignore entirely
    ///   - Decode failure + standing queue above target → real congestion
    ///     → backoff ×0.92 (same speed as the delay backoff; a decode
    ///     failure adds no extra information beyond the delay signal)
    ///   - Decode failure + empty queue → borderline FEC under-provision,
    ///     not congestion → end the ramp and step down by 1
    pub fn on_loss(&mut self, fec_recovered: bool) {
        if fec_recovered {
            return;
        }
        if self.copa.queue_above_target(self.cwnd) {
            self.cwnd = self.copa.backoff(self.cwnd);
        } else {
            self.copa.ramping = false;
            self.cwnd = self.cwnd.saturating_sub(1);
        }
        self.clamp_cwnd_with_anchor();
        self.in_slow_start = false;
        if self.ssthresh > self.cwnd {
            self.ssthresh = self.cwnd;
        }
    }

    /// Feed an RTT measurement into Copa state.
    /// Call this when processing ACKs/reports that include RTT.
    pub fn record_rtt_sample(&mut self, rtt: Duration) {
        // THE RAW SAMPLE DUMP (`RWM_RTT_DUMP`, default OFF) — fed HERE, at the
        // single delegate every ack path funnels through, so the dumped series
        // is EXACTLY the series the five dispersion gauges consume. Clause `B`
        // compares an estimator against its own input; "its own input" has to
        // be literally true, or the comparison is the incommensurable one the
        // scored battery convicted (a 20 Hz ICMP probe against a kHz sender).
        // Null check with the gate off; no engine decision reads it.
        if let Some(d) = crate::net::rttdump::gauge() {
            d.note_rtt(self.id, (rtt.as_micros() as u64).min(u32::MAX as u64) as u32);
        }
        self.copa.record_rtt(rtt);
    }

    /// Wire-level loss evidence for the Copa competitive AIMD
    /// (feat/copa-compete): pass the pass-through shim's cumulative
    /// `congestion_events` counter for this path. No-op unless
    /// RWM_COPA_COMPETE is active.
    pub fn on_wire_congestion_events(&mut self, cumulative: u64) {
        self.copa.note_congestion_events(cumulative);
    }

    /// Copa competitive-mode DIAG snapshot (feat/copa-compete):
    /// (switching enabled, currently competitive, competitive entries,
    /// live δ, base δ). Observation only.
    pub fn copa_compete_diag(&self) -> (bool, bool, u64, f64, f64) {
        (
            self.copa.compete_on,
            self.copa.in_compete,
            self.copa.compete_switches,
            self.copa.delta,
            self.copa.delta_base,
        )
    }

    /// Test hook: force the wire-clocked δ-mapped update law with an
    /// explicit δ (bypasses the process-global env gate).
    #[cfg(test)]
    pub(crate) fn force_wire_for_test(&mut self, delta: f64) {
        self.copa.force_wire(delta);
    }

    /// Test hook: enable Copa §2.2 competitive mode switching (requires a
    /// prior `force_wire_for_test`).
    #[cfg(test)]
    pub(crate) fn force_compete_for_test(&mut self) {
        self.copa.force_compete();
    }

    /// Test hook (GOAL "HONEST INPUTS" phase 3, the c1 lock-blocking probe
    /// in `net::tests`): force the O(1) honest windowed-max deque ON for
    /// this path — the `RWM_HONEST_ANCHOR` (DH-arm) configuration — without
    /// touching the process-global env gate. Value-identical either way
    /// (`bw_mono_front_equals_full_window_fold`); this selects the DH arm's
    /// COST so the bench prices the fixed attribution path, not the fold.
    #[cfg(test)]
    pub(crate) fn force_honest_anchor_for_test(&mut self) {
        self.copa.force_bw_o1();
    }

    /// Test accessor (same probe): the cwnd anchor FLOOR
    /// (`CopaState::anchor_floor` — ANCHOR_FLOOR_GAIN × BtlBw × RTprop).
    /// The floor only ratchets cwnd UP, so it is the wire-bound sender's
    /// resting cwnd LOWER bound — the quantity the saturation predicate
    /// (`available() == 0`, the `active_paths()` filter) compares
    /// outstanding against.
    #[cfg(test)]
    pub(crate) fn anchor_floor_for_test(&self) -> Option<u32> {
        self.copa.anchor_floor()
    }


    /// Read Copa's current min_rtt estimate (for diagnostics/benchmarking).
    pub fn copa_min_rtt(&self) -> Option<Duration> {
        self.copa.min_rtt()
    }

    /// Smoothed RTT estimate (Copa's EWMA; the loss estimator's EWMA as a
    /// fallback before the first Copa sample).
    pub fn srtt(&self) -> Duration {
        match self.copa.srtt {
            Some(s) => s,
            None => self.estimator.rtt(),
        }
    }

    /// `srtt()` ONLY IF it is a MEASUREMENT — `None` for a path that has
    /// never had an RTT sample, where `srtt()` returns the 50-ms
    /// `DEFAULT_SRTT`-class seed instead (hygiene rule 1: `srtt()` cannot
    /// tell a consumer which of the two it just handed over).
    ///
    /// Structurally identical to `srtt()`, term for term — Copa's EWMA
    /// first, the loss estimator's as the pre-Copa fallback — so wherever
    /// this returns `Some(d)`, `srtt() == d` exactly. Consumers that must not
    /// price an unmeasured path with a constant read this
    /// (`Scheduler::place_costs`, `RWM_COLD_PLACE`).
    pub fn srtt_measured(&self) -> Option<Duration> {
        match self.copa.srtt {
            Some(s) => Some(s),
            None => self.estimator.rtt_measured(),
        }
    }

    /// The path's OWN measured RTT jitter, in microseconds — the derived
    /// patience floor's second term (goal-gate "Unlock The Default 2").
    ///
    /// Copa's consecutive-difference estimate (RFC 3550-style EWMA at gain
    /// 1/8, shift-robust: a standing queue shifts all samples and leaves the
    /// consecutive differences at jitter scale) widened by its window-level
    /// twin — `max(jitter_est, win_jitter_est)`, exactly the combination the
    /// Copa backoff threshold already uses, so patience and the CC read the
    /// SAME jitter. Before Copa has an RTT sample both are 0 and the loss
    /// estimator's RFC 3550 §A.8 interarrival jitter stands in.
    ///
    /// Measured, never configured: there is no env knob on this path.
    /// The RTT distribution's standard-deviation estimate (µs) — paper
    /// §16.69. `√(EWMA[(rtt − srtt)²])`, the SECOND moment Cantelli's
    /// distribution-free bound is stated in. `None` before any sample, so the
    /// derived clock gets an information-availability fallback, not a mode.
    pub fn rtt_sigma_us(&self) -> Option<u64> {
        if self.copa.rtt_var_sq <= 0.0 {
            return None;
        }
        Some((self.copa.rtt_var_sq.sqrt() * 1e6) as u64)
    }

    /// How many RTT samples have been folded into [`rtt_sigma_us`]'s EWMA —
    /// the σ gauge's WARM-UP EVIDENCE, reported beside it as
    /// `sig_us=<µs>/n<count>` in the `[DIAG]` line.
    ///
    /// It is NOT gated on `ANCHOR_MIN_SAMPLES`: that constant gates the
    /// delivered-rate anchor and has nothing to say about this statistic. See
    /// `CopaState::rtt_var_n` for what the count means and why it is reported
    /// rather than used as a threshold.
    ///
    /// [`rtt_sigma_us`]: Self::rtt_sigma_us
    pub fn rtt_sigma_samples(&self) -> u64 {
        self.copa.rtt_var_n
    }

    // ---------------------------------------------------------------------
    // THE THREE CANDIDATE DISPERSION GAUGES — goal #101 item 2, paper
    // §16.74.5's named successor. READ-ONLY, READ BY NOTHING but `[DIAG]`.
    //
    // **They are a DECOMPOSITION, not three guesses.** The shipped `sig_us`
    // carries three independent suspect properties at once, and the measured
    // 287× at `c8` cannot say which of them produced it. Each candidate moves
    // exactly one axis away from the shipped estimator, so the differences
    // between them identify the cause:
    //
    //   axis                shipped `sig_us`      `rvar`    `qsp`     `msd`
    //   ------------------  --------------------  --------  --------  --------
    //   memory              7 samples (β = 1/4)   7         L = 256   L = 256
    //   deviation enters    SQUARED               linear    rank      rank
    //   reference           lagging `srtt` EWMA   lagging   none      none
    //
    //   `rvar` vs `sig_us` : isolates the SQUARE   (memory + reference fixed)
    //   `qsp`  vs `rvar`   : isolates the MEMORY   (reference still absent)
    //   `msd`  vs `qsp`    : isolates the REFERENCE (memory + rank fixed)
    //
    // **All three render `-` before their first sample and carry their own
    // sample count beside their value**, the shipped `sig_us=<µs|->/n<count>`
    // convention — with one deliberate repair: `sig_us` returns `None` on a
    // non-positive `rtt_var_sq`, so it renders `-` for "no sample yet" AND for
    // "dispersion is exactly zero", which a parser cannot tell apart. These
    // return `None` **iff the sample set is empty** and report a genuine zero
    // as `0`. Neither is a threshold and neither gates anything: the warm-up
    // exclusions live in the battery's parser, pre-registered in goal-gate
    // "THE SIGMA ESTIMATOR — THE ACCEPTANCE BAR" clause `C3`.
    //
    // **No consumer. No gate. No default.** Nothing in the engine reads any of
    // these; the acceptance bar's battery is a later, VM-side pass.
    // ---------------------------------------------------------------------

    /// The `q`-quantile of an already-sorted slice, by the tree's own
    /// convention (`net::QuantileClockGauge::quantile`) — nearest-rank on
    /// `round((len − 1)·q)`, no interpolation, so two reads of one sample set
    /// always agree and the value is always a sample that actually occurred.
    fn cand_quantile(sorted: &[u32], q: f64) -> u64 {
        crate::monitor::quantile::nearest_rank(sorted, q) as u64
    }

    /// **CANDIDATE 1 — `qsp_us=`, WINDOWED QUANTILE DISPERSION, UNSCALED.**
    ///
    /// ```text
    ///     qsp  =  P90(rtt)  −  P50(rtt)      over the last L = 256 samples
    /// ```
    ///
    /// **UNSCALED, and that is a decision with a reason rather than an
    /// omission.** The obvious alternative is to divide by 1.2816 — the
    /// Gaussian value of `(P90 − P50)/σ` — and call the result a σ-equivalent.
    /// It is not done, for three reasons:
    ///
    /// 1. **The acceptance bar is scale-free.** `R_total = σ̂_p95/σ̂_p05` and
    ///    §16.74.5's `R_σ̂` are both RATIOS, so no fixed positive scaling
    ///    changes any clause of `S`. The constant would buy the bar nothing.
    /// 2. **The assumption it imports is refuted by the data it would be
    ///    applied to.** A Gaussian conversion is only meaningful on a Gaussian;
    ///    `c8` produced a σ reading of 54.836 ms at a cell whose measured `d`
    ///    is 3.298 ms and whose `RTprop` is 38 ms. That is not a Gaussian tail.
    ///    §16.69's one real virtue is being DISTRIBUTION-FREE, and scaling by a
    ///    Gaussian constant would spend exactly that.
    /// 3. **§16.69's own construction permits the quantile-native route over
    ///    this range.** Its construction line reads `W(α) = F⁻¹_X(1 − α)` — the
    ///    clock IS a quantile — and Cantelli is the distribution-free FALLBACK
    ///    for when only moments are available. An estimator that reports
    ///    quantiles directly does not need the fallback. §16.69 refuted the
    ///    direct route at the CONTRACT's `α = 10⁻⁵` (100 000 samples); over the
    ///    SWEPT range `[0.002, 0.400]` that arithmetic does not bind, and the
    ///    acceptance bar's clause `C2` records exactly where the line falls.
    ///
    /// The Gaussian constant is documented here and applied nowhere, so a
    /// future consumer that wants a σ-equivalent can multiply by `1/1.2816`
    /// with its assumption on the record: **for `X ~ N(µ, σ²)`,
    /// `P90 − P50 = 1.2816·σ`.**
    ///
    /// **Why it should beat the shipped EWMA, argued from the measured data.**
    /// It moves two axes at once. MEMORY: 256 samples against the EWMA's 7, so
    /// a reading is a property of the window and not of wherever the last
    /// seven samples happened to land. OUTLIER LEVERAGE: a quantile moves by
    /// one RANK regardless of an excursion's magnitude — `P90` over `L = 256`
    /// is unmoved by up to 25 arbitrarily large outliers, where the shipped
    /// EWMA admits one 200 ms excursion as `(200 ms)²` and needs ~16 samples
    /// to decay it below 1 %.
    ///
    /// `None` iff no sample has been recorded.
    pub fn rtt_qspread_us(&self) -> Option<u64> {
        if self.copa.rtt_win.is_empty() {
            return None;
        }
        let mut s: Vec<u32> = self.copa.rtt_win.iter().copied().collect();
        s.sort_unstable();
        Some(Self::cand_quantile(&s, 0.90) - Self::cand_quantile(&s, 0.50))
    }

    /// Samples in [`rtt_qspread_us`]'s window RIGHT NOW — **the window FILL,
    /// not the path's lifetime sample count**, and it saturates at
    /// `SIGMA_CAND_WINDOW`.
    ///
    /// That is deliberate and it follows the rule `diag.rs` already states for
    /// `sig_us`: the count must describe the sample set the value was computed
    /// from, so *"the denominator can never describe a different sample set
    /// than its numerator."* A lifetime count beside a windowed value would
    /// describe a different set. It also makes the pre-registered window-class
    /// warm-up test exact: **the window is warm iff `n == L`.**
    ///
    /// [`rtt_qspread_us`]: Self::rtt_qspread_us
    pub fn rtt_qspread_samples(&self) -> u64 {
        self.copa.rtt_win.len() as u64
    }

    /// **CANDIDATE 2 — `rvar_us=`, RFC 6298 §2's `RTTVAR`.**
    ///
    /// ```text
    ///     rvar  ←  (1 − β)·rvar  +  β·|rtt − srtt| ,     β = 1/4
    /// ```
    ///
    /// **Provenance: CITED.** β = 1/4 and the mean-deviation form are RFC 6298
    /// §2 verbatim, inherited by RFC 8985 §6.2 for RACK. Nothing here is
    /// fitted, and the shipped `rtt_var_sq` already cites the same RFC for the
    /// same gain on the second moment — so the two differ by the square alone.
    ///
    /// **It exists to be the CONTROL, not to win.** See `CopaState::rtt_mdev`:
    /// it holds memory and reference fixed against the shipped estimator and
    /// moves only the power the deviation enters at, which is the only way to
    /// attribute the 287× to outlier leverage or acquit it of that.
    ///
    /// Gaussian conversion, documented and not applied: for `X ~ N(µ, σ²)`,
    /// `E|X − µ| = √(2/π)·σ = 0.7979·σ`, so a consumer wanting a σ-equivalent
    /// multiplies by 1.2533. RFC 6298 itself does not: it uses `4·RTTVAR`
    /// directly, which is a mean-deviation multiplier and not a σ one.
    ///
    /// `None` iff no sample has been folded in.
    pub fn rtt_mdev_us(&self) -> Option<u64> {
        if self.copa.rtt_mdev_n == 0 {
            return None;
        }
        Some((self.copa.rtt_mdev * 1e6).max(0.0) as u64)
    }

    /// Samples folded into [`rtt_mdev_us`]'s EWMA. EWMA-class, so its
    /// pre-registered warm-up is `n ≥ 16` — identical to
    /// [`rtt_sigma_samples`], because it is the identical EWMA at the
    /// identical gain, fed at the identical site.
    ///
    /// [`rtt_mdev_us`]: Self::rtt_mdev_us
    /// [`rtt_sigma_samples`]: Self::rtt_sigma_samples
    pub fn rtt_mdev_samples(&self) -> u64 {
        self.copa.rtt_mdev_n
    }

    /// **CANDIDATE 3 — `msd_us=`, THE REFERENCE-FREE DISPERSION.** Median
    /// absolute SUCCESSIVE difference over the same window `L = 256`:
    ///
    /// ```text
    ///     msd  =  median( |rtt_i − rtt_{i−1}| )        over the window
    /// ```
    ///
    /// **THIS CANDIDATE IS ARGUED FROM THE MEASURED σ PROCESS, AND THE
    /// ARGUMENT IS THAT THE 287× IS NOT WHAT IT LOOKS LIKE.** The obvious
    /// reading of the `c8` spread is loss-burst contamination: a lossy cell
    /// produces RTT excursions and the estimator inhales them. **The committed
    /// ledgers refute that reading on their own numbers.** From the
    /// plain-window primitives table, per-cell loss `p` against the measured
    /// rep-to-rep σ spread:
    ///
    /// ```text
    ///     cell   p (per leg)        σ reps (ms)              sup/inf
    ///     c1     0.00015            0.013 / 0.035 / 0.046      3.5×
    ///     sc2    0.0040             0.335 / 0.492 / 1.113      3.3×
    ///     c7     0.0056 / 0.0053    0.480 / 0.499 / 2.321      4.8×
    ///     c8L    0.0039 / 0.0165    0.343 / 0.665 / 4.088     11.9×
    ///     c8     0.0040 / 0.0184    0.191 / 3.140 / 54.836   287×
    /// ```
    ///
    /// **`sc2` and `c8`'s fast leg carry the SAME loss rate (0.0040) and their
    /// σ spreads differ by 87×.** Loss rate does not predict the spread, so a
    /// loss-window-excluding estimator would be excluding the wrong thing —
    /// and it would also need a loss signal to key on, which is a coupling this
    /// gauge has no business introducing.
    ///
    /// **What the data points at instead is the REFERENCE.** The shipped
    /// estimator's deviation is `rtt − srtt`, and `srtt` is itself an EWMA at
    /// β = 1/8 chasing the same series. When the queue takes a LEVEL SHIFT,
    /// `srtt` lags it by ~8 samples and every deviation in that window is a
    /// full step height rather than a dispersion. Squared, that is the 54.836
    /// ms reading — **a "dispersion" 1.4× the cell's own `RTprop` of 38 ms and
    /// 17× its measured `d` of 3.298 ms, which no dispersion of a stationary
    /// RTT about a tracking mean can be.** It is `srtt`'s tracking error
    /// wearing σ's clothes. The two cells with the largest spreads (`c8`,
    /// `c8L`) are also the two with the largest per-leg loss ASYMMETRY (4.6×
    /// and 4.2×), which is a standing-queue-shift generator and not a
    /// loss-rate effect.
    ///
    /// **Successive differencing cancels a level shift exactly** — that is
    /// what the statistic is FOR, and it is the tree's own idea already:
    /// `CopaState::jitter_est` uses consecutive differences and its field doc
    /// gives this exact reason (*"a standing queue shifts ALL samples and
    /// leaves the consecutive differences at jitter scale"*). This candidate is
    /// that insight applied to the dispersion estimator, with the EWMA replaced
    /// by a median so an excursion moves it by one rank instead of by its
    /// magnitude.
    ///
    /// **Provenance: CITED.** The mean/median of absolute successive
    /// differences is the standard robust scale estimator under an unknown
    /// drifting mean (von Neumann's ratio, 1941; the successive-difference
    /// variance estimator of von Neumann, Kent, Bellinson & Hart 1941), and
    /// RFC 3550 §A.8's interarrival jitter is the same construction.
    ///
    /// Gaussian conversion, documented and not applied: for iid `X ~ N(µ, σ²)`
    /// the successive difference is `N(0, 2σ²)`, so
    /// `median|Δ| = 0.6745·√2·σ = 0.9539·σ` — within 5 % of unity, which is a
    /// convenience and not a licence to treat it as σ.
    ///
    /// `None` until at least two samples exist (one difference).
    pub fn rtt_msd_us(&self) -> Option<u64> {
        if self.copa.rtt_win.len() < 2 {
            return None;
        }
        let mut d: Vec<u32> = self
            .copa
            .rtt_win
            .iter()
            .zip(self.copa.rtt_win.iter().skip(1))
            .map(|(a, b)| a.abs_diff(*b))
            .collect();
        d.sort_unstable();
        Some(Self::cand_quantile(&d, 0.50))
    }

    /// SUCCESSIVE DIFFERENCES available to [`rtt_msd_us`] right now — the
    /// window fill minus one, saturating at `SIGMA_CAND_WINDOW − 1`.
    ///
    /// It is the difference count and not the sample count because the
    /// differences are what the median is taken over, and the count beside a
    /// value must describe that value's own sample set.
    ///
    /// [`rtt_msd_us`]: Self::rtt_msd_us
    pub fn rtt_msd_samples(&self) -> u64 {
        (self.copa.rtt_win.len() as u64).saturating_sub(1)
    }

    /// **CANDIDATE 4 — the τ-LAG PAIR SET. The one place the pairing rule
    /// exists**, so that [`rtt_tlag_us`] and [`rtt_tlag_samples`] can never
    /// describe different sample sets and the `-`-iff-`n == 0` biconditional
    /// holds BY CONSTRUCTION rather than by two functions agreeing.
    ///
    /// Paper §16.75.0, transcribed:
    ///
    /// ```text
    ///     P(τ) = { (i, j(i)) : j(i) = argmax { t_j : t_i − t_j ≥ τ },  j < i
    ///                          admitted iff  t_i − t_{j(i)} ≤ c·τ }
    ///
    ///     τ = RTprop  (MEASURED),   c = SIGMA_TLAG_BAND_C = 2
    /// ```
    ///
    /// Every anchor contributes at most one pair — its most recent admissible
    /// partner — so `|P(τ)|` is a count of anchors and is directly readable as
    /// the gauge's `n`.
    ///
    /// Empty (⇒ the gauge renders `-`) when `τ` is not yet established, when
    /// `τ` is zero, or when the leg's own spacing exceeds `c·τ` so that no
    /// partner is admissible anywhere in the ring. **That last case is the
    /// honest verdict at a leg too thin to hold a τ-lag pair, and it is
    /// reported rather than patched** (§16.75.6 F1).
    ///
    /// `O(L)`: the ring is ascending in time and `j` is non-decreasing in `i`,
    /// so the scan is a two-pointer sweep, at the `[DIAG]` cadence only.
    ///
    /// [`rtt_tlag_us`]: Self::rtt_tlag_us
    /// [`rtt_tlag_samples`]: Self::rtt_tlag_samples
    fn tlag_diffs(&self) -> Vec<u32> {
        let tau = match self.copa.min_rtt {
            Some(t) if !t.is_zero() => t,
            _ => return Vec::new(),
        };
        let hi = tau * SIGMA_TLAG_BAND_C;
        let ring = &self.copa.rtt_tlag;
        let n = ring.len();
        let mut out: Vec<u32> = Vec::with_capacity(n);
        // `j` tracks the LAST index whose lag to the current anchor is still
        // `≥ τ`. Lag decreases in `j` and anchors advance in time, so `j` only
        // ever moves forward.
        let mut j = 0usize;
        for i in 0..n {
            let (ti, vi) = ring[i];
            while j + 1 < i && ti.duration_since(ring[j + 1].0) >= tau {
                j += 1;
            }
            if j < i {
                let (tj, vj) = ring[j];
                let lag = ti.duration_since(tj);
                if lag >= tau && lag <= hi {
                    out.push(vi.abs_diff(vj));
                }
            }
        }
        out
    }

    /// **CANDIDATE 4 — `tlag_us=`, THE RATE-INVARIANT DISPERSION.** Median
    /// absolute difference at a fixed TIME lag `τ = RTprop`, paper §16.75:
    ///
    /// ```text
    ///     σ̂_Δ(τ)  =  median { |rtt(t_i) − rtt(t_j)| : (i, j) ∈ P(τ) }
    /// ```
    ///
    /// **WHY IT EXISTS, AND IT IS NOT AN IMPROVEMENT ON `msd_us` — IT IS A
    /// DIFFERENT ESTIMAND.** For a stationary series with autocorrelation `ρ`,
    /// the median absolute difference at lag `τ` is the structure function
    /// (variogram) evaluated there:
    ///
    /// ```text
    ///     median |X(t + τ) − X(t)|  =  0.6745 · √( 2·(1 − ρ(τ)) ) · σ
    /// ```
    ///
    /// A lag-1 successive difference — which is what [`rtt_msd_us`] computes —
    /// is therefore `V` evaluated at **whatever the inter-sample spacing
    /// happens to be**, and that spacing is set by the ack rate, i.e. by the
    /// traffic. It is not a property of the link and not a property of the
    /// estimator, so **two legs of one run estimate two different quantities.**
    ///
    /// **That is measured, not argued.** The scored VM battery (goal-gate, "THE
    /// SIGMA ESTIMATOR — THE SCORED RESULT" §6) found `R_total` tracking the
    /// sample rate at `rho = −0.548` across the eight sender legs, with the two
    /// thinnest legs (581 and 1 762 samples/s) the two worst readings in the
    /// battery (34.6 and 16.5) — and `msd_us`'s 8.667 on the data path is the
    /// nearest any candidate came to the pre-registered accept bar of 6.0.
    /// Selecting pairs by ELAPSED TIME fixes the estimand before any sample
    /// arrives: a sparse leg and a dense leg select the same τ-differences of
    /// the same process, the sparse leg simply finds fewer of them.
    ///
    /// **Provenance.** `median`, `|·|` — CITED, von Neumann, Kent, Bellinson &
    /// Hart 1941; RFC 3550 §A.8 is the same construction at lag 1. The
    /// extension to a STATED lag is Matheron's variogram (1963) and Allan's
    /// two-sample deviation at averaging time τ (1966). `τ = RTprop` —
    /// MEASURED, and the fraction is **1 and DERIVED**: one `RTprop` is the
    /// smallest lag at which two RTT samples observe distinct path traversals
    /// rather than the same queue occupancy on one traversal, which is the same
    /// ruling `control/anchor.rs` already enforces when it rejects sub-`RTprop`
    /// delivery-rate samples. `c` and `m` — DECLARED RESOURCE BOUNDS with their
    /// arithmetic, in their own const docs.
    ///
    /// **UNSCALED, exactly as `qsp_us` is.** Clause `S`'s `R_total` is a ratio
    /// so no fixed scaling changes it, and the rebuilt clause `B` compares this
    /// reading against the SAME functional computed offline over the same
    /// samples, so no scaling changes that either. The Gaussian conversion
    /// above needs `ρ(τ)` — the very thing being measured — so it is
    /// **documented here and applied nowhere.**
    ///
    /// **Reduces to `msd_us` in the limit** `τ → Δt`: the band collapses onto
    /// the lag-1 pair. The successor contains its predecessor, and the whole of
    /// the claimed improvement is a τ chosen by physics instead of by traffic.
    ///
    /// Read by nothing. `None` **iff** [`rtt_tlag_samples`] is 0.
    ///
    /// [`rtt_msd_us`]: Self::rtt_msd_us
    /// [`rtt_tlag_samples`]: Self::rtt_tlag_samples
    pub fn rtt_tlag_us(&self) -> Option<u64> {
        let mut d = self.tlag_diffs();
        if d.is_empty() {
            return None;
        }
        d.sort_unstable();
        Some(Self::cand_quantile(&d, 0.50))
    }

    /// τ-LAG PAIRS available to [`rtt_tlag_us`] right now — `|P(τ)|`.
    ///
    /// It is the PAIR count and not the sample count because the pairs are what
    /// the median is taken over, and the count beside a value must describe
    /// that value's own sample set. Both come from [`tlag_diffs`], so
    /// `rtt_tlag_us().is_none() ⇔ rtt_tlag_samples() == 0` holds by
    /// construction.
    ///
    /// **A reading resting on fewer than `L/8 = 32` pairs is scored
    /// `UNSCOREABLE-THIN` by the battery's PARSER** (paper §16.75.6 F1). That
    /// rule is declared in the paper, applied off-box, and is deliberately
    /// **not** a threshold anywhere in this crate: `diag.rs`'s standing reason
    /// applies verbatim — a field that disappears below a threshold cannot be
    /// told apart from a path that was never sampled.
    ///
    /// [`rtt_tlag_us`]: Self::rtt_tlag_us
    /// [`tlag_diffs`]: Self::tlag_diffs
    pub fn rtt_tlag_samples(&self) -> u64 {
        self.tlag_diffs().len() as u64
    }

    pub fn rtt_jitter_us(&self) -> u64 {
        let copa_j = self.copa.jitter_est.max(self.copa.win_jitter_est);
        if copa_j > 0.0 {
            (copa_j * 1e6) as u64
        } else {
            self.estimator.jitter_us().max(0.0) as u64
        }
    }

    /// Classic Copa equilibrium target, for diagnostics (see
    /// `CopaState::copa_target_cwnd` for the units derivation).
    pub fn copa_target_cwnd(&self) -> u32 {
        self.copa.copa_target_cwnd()
    }

    /// BtlBw×RTprop BDP anchor estimate in symbols, once established
    /// (paper Section 12.6). None during warm-up / before a min-RTT
    /// sample. Diagnostic/benchmarking accessor.
    pub fn copa_bdp_anchor(&self) -> Option<f64> {
        self.copa.bdp_anchor()
    }

    /// RTprop (Copa windowed-min RTT) for this path (None during warm-up).
    pub fn min_rtt(&self) -> Option<Duration> {
        self.copa.min_rtt()
    }

    /// goal-gate "Honest Inputs" (`RWM_HONEST_K`): the RAW-sample windowed-min
    /// echo-ratio for this path, or None with the gate off. `Some` values are
    /// what the honest-cap / three-term collectors substitute for the
    /// smoothed-at-refresh K (`k_raw.unwrap_or(legacy)` — ONE formula, the
    /// gate changes which measured series feeds it); None ⇒ every consumer
    /// byte-identical to the legacy feed.
    pub fn k_raw(&self) -> Option<f64> {
        self.copa.k_raw_ratio()
    }

    /// BtlBw (bottleneck rate) for this path in symbols/second — the path's
    /// own drain rate = anchor / RTprop = (BtlBw·RTprop)/RTprop.  This is the
    /// BBR-style per-path pacing rate: the slow path's future-offset data
    /// emitted at BtlBw_slow flows at the slow path's drain rate WITHOUT
    /// queuing, so no standing queue (bufferbloat) builds.  None during warm-up
    /// (same trustworthiness gate as `copa_bdp_anchor`).
    pub fn btlbw_sym_per_s(&self) -> Option<f64> {
        // Warm gate: an RTprop sample must exist (same trustworthiness gate as
        // `copa_bdp_anchor`).  The rate itself is `effective_btlbw` — the pure
        // windowed-MAX (byte-identical to the old `anchor/RTprop`).
        self.copa.min_rtt()?;
        self.copa.effective_btlbw()
    }

    /// Clamp cwnd to [MIN_CWND, MAX_CWND] and then raise it to the BtlBw
    /// anchor floor if one is established (paper Section 12.6). The floor
    /// only ratchets cwnd UP (never a cap) and is itself bounded by
    /// MAX_CWND, so an over-read BtlBw cannot exceed the hard ceiling.
    fn clamp_cwnd_with_anchor(&mut self) {
        self.cwnd = self.cwnd.clamp(Self::MIN_CWND, Self::MAX_CWND);
        if let Some(floor) = self.copa.anchor_floor() {
            self.cwnd = self.cwnd.max(floor.min(Self::MAX_CWND));
        }
    }

    // --- Token-bucket pacing (paper Section 12.5, gate driver P1) ---
    //
    // UNITS: tokens are SYMBOLS. Refill rate = cwnd [symbols] / SRTT [s]
    // = symbols/second; burst allowance = max(10, cwnd/8) symbols.

    /// Replenish pacing tokens for elapsed wall time.
    pub fn pace_refill(&mut self) {
        let now = self.clock.now();
        let elapsed = now.duration_since(self.last_pace_refill).as_secs_f64();
        self.last_pace_refill = now;
        let srtt = self.srtt().as_secs_f64().max(1e-3);
        let rate = self.cwnd as f64 / srtt; // symbols per second
        let burst = (self.cwnd as f64 / 8.0).max(10.0);
        self.pace_tokens = (self.pace_tokens + rate * elapsed).min(burst);
    }

    /// Current pacing token balance (symbols; may be negative — see field).
    pub fn pace_tokens(&self) -> f64 {
        self.pace_tokens
    }

    /// Consume tokens for `n` symbols just sent (may push balance negative).
    pub fn consume_pace_tokens(&mut self, n: u32) {
        self.pace_tokens -= n as f64;
    }

    /// Time until at least one pacing token is available at the current
    /// refill rate (zero if a token is already available).
    pub fn pace_delay(&self) -> Duration {
        if self.pace_tokens >= 1.0 {
            return Duration::ZERO;
        }
        let srtt = self.srtt().as_secs_f64().max(1e-3);
        let rate = (self.cwnd as f64 / srtt).max(1.0); // symbols per second
        Duration::from_secs_f64((1.0 - self.pace_tokens) / rate)
    }

    // --- in_flight budget accounting (P7 follow-up 2) ---
    //
    // in_flight is a BUDGET GAUGE (symbols committed: interleaver + pacing
    // carry + wire), charged exactly once per symbol at SCHEDULE time and
    // released by ACK feedback. ACKs are best-effort datagrams: a lost ACK
    // strands its release forever, and stranded budget compounds until the
    // TUN gate jams (L1 finding: the gate cycled at the 2s leak-guard
    // cadence instead of the RTT). The FIFO charge log makes releases
    // robust: budget older than max(4×SRTT, 250ms) is delivered-or-lost
    // either way (RFC 9002-style time-threshold, at gauge granularity) and
    // expires. Pacing (cwnd/SRTT tokens) remains the actual rate limiter,
    // so an early expiry can only let the encoder run ahead, never the
    // wire.

    /// Charge `n` symbols against the in_flight budget (at schedule time).
    pub fn charge_in_flight(&mut self, n: u32) {
        if n == 0 {
            return;
        }
        self.in_flight = self.in_flight.saturating_add(n);
        let now = self.clock.now();
        // Pool-anchor feed (RWM_POOL_ANCHOR): every wire send on this path
        // is a send-process sample for the honest dual-store anchor. O(1)
        // amortized; gate resolved once at construction — the =0 arm skips
        // entirely (cost-honest A/B).
        if self.pool_anchor_feed {
            let srtt = self.srtt();
            self.send_anchor.on_send(now, n as u64, srtt);
        }
        self.in_flight_log.push_back((now, n));
    }

    /// The pool-anchor SEND rate for this path (symbols/s) — the GAP-ROBUST
    /// WINDOWED MEAN (`SendRateAnchor::mean_rate`; the pre-battery
    /// amendment: an admission-gated sender's refill bursts latch a
    /// windowed-max, measured sr=53k vs ≈8.9k truth) — or None before the
    /// first surviving bucket / with the feed off. The N ≥ 2 pooled-store
    /// cap law's honest rate input (goal-gate "Ship The Wins 1").
    /// Read-only: consumes no sample, owns no cwnd dynamics.
    pub fn send_rate_anchor(&self) -> Option<f64> {
        if !self.pool_anchor_feed {
            return None;
        }
        self.send_anchor.mean_rate(self.clock.now(), self.srtt())
    }

    /// (gaps detected, buckets discarded) for the pool-anchor sampler — the
    /// DIAG hygiene gauges.
    pub fn send_anchor_stats(&self) -> (u64, u64) {
        self.send_anchor.stats()
    }

    /// Test hook: force the pool-anchor feed regardless of the process-global
    /// env cache (unit tests must not depend on it — the `force_wire`
    /// pattern).
    #[cfg(test)]
    pub fn force_pool_anchor_feed(&mut self, on: bool) {
        self.pool_anchor_feed = on;
    }

    /// Test hook: force the 1:1 release (`RWM_RELEASE_1TO1`). Unit tests must
    /// not depend on the process-global env cache — the `force_wire` pattern.
    #[cfg(test)]
    pub fn force_release_1to1(&mut self, on: bool) {
        self.release_1to1 = on;
    }

    /// Release `n` symbols of budget (ACK feedback: received or
    /// gap-inferred lost). Pops the OLDEST charges first.
    pub fn release_in_flight(&mut self, n: u32) {
        self.in_flight = self.in_flight.saturating_sub(n);
        let mut remaining = n;
        while remaining > 0 {
            match self.in_flight_log.front_mut() {
                Some((_, c)) if *c > remaining => {
                    *c -= remaining;
                    remaining = 0;
                }
                Some((_, c)) => {
                    remaining -= *c;
                    self.in_flight_log.pop_front();
                }
                None => break,
            }
        }
    }

    /// Expire budget charged longer than the horizon ago: its ACK (or the
    /// loss evidence) would have arrived by now — the datagram was delivered
    /// with the ACK lost, or lost with no later batch to reveal the gap.
    /// Either way it is no longer on the wire.
    ///
    /// This sweep is **1:1 with the charge by construction** — it pops the
    /// very `in_flight_log` entries `charge_in_flight` pushed.
    ///
    /// LEGACY horizon: `max(4 x SRTT, 250 ms)`, roughly a decade past the
    /// scale at which a symbol's fate is decided, which makes this a backstop
    /// and leaves the operative release to the contaminated
    /// `expected - received` term at the ack arms.
    ///
    /// `RWM_RELEASE_1TO1` horizon: RFC 9002 §6.1.2's kTimeThreshold,
    /// `9/8 x SRTT`, floored at the same kGranularity analog the recovery
    /// plane's own time threshold uses ([`crate::net::mp_time_threshold_split`],
    /// [`crate::net::NACK_RETX_COOLDOWN_FLOOR_US`]) — the engine's OWN
    /// judgement about when a symbol is lost, applied to the budget it charged
    /// for that symbol. No new constant; see [`release_1to1_active`].
    pub fn expire_in_flight(&mut self) {
        if self.in_flight_log.is_empty() {
            return;
        }
        let horizon = if self.release_1to1 {
            let srtt_us = self.srtt().as_micros() as u64;
            Duration::from_micros(
                crate::net::mp_time_threshold_split(
                    srtt_us,
                    srtt_us,
                    crate::net::NACK_RETX_COOLDOWN_FLOOR_US,
                )
                .0,
            )
        } else {
            (self.srtt() * 4).max(IN_FLIGHT_EXPIRY_MIN)
        };
        let now = self.clock.now();
        while let Some(&(t, c)) = self.in_flight_log.front() {
            if now.duration_since(t) < horizon {
                break;
            }
            self.in_flight = self.in_flight.saturating_sub(c);
            self.in_flight_log.pop_front();
        }
    }
}
