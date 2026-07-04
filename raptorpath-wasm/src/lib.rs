use raptorpath_math as math;
use wasm_bindgen::prelude::*;

// =========================================================================
// Pure math functions — thin wasm-bindgen wrappers
// =========================================================================

#[wasm_bindgen]
pub fn normal_survival(z: f64) -> f64 {
    math::normal_survival(z)
}

#[wasm_bindgen]
pub fn p_lost(age_secs: f64, epsilon: f64, srtt_secs: f64, rttvar_secs: f64) -> f64 {
    math::p_lost(age_secs, epsilon, srtt_secs, rttvar_secs)
}

#[wasm_bindgen]
pub fn burst_variance_factor(p: f64, q: f64) -> f64 {
    math::burst_variance_factor(p, q)
}

#[wasm_bindgen]
pub fn compute_r_star(epsilon: f64, sigma2: f64, window_size: f64) -> f64 {
    math::compute_r_star(epsilon, sigma2, window_size)
}

/// Continuous r* (paper 8.4): quantile at 1 - delta/eps; glides to 0.
#[wasm_bindgen]
pub fn compute_r_star_continuous(epsilon: f64, sigma2: f64, window_size: f64, delta: f64) -> f64 {
    math::compute_r_star_with_z(epsilon, sigma2, window_size, math::z_for_tail_target(delta, epsilon))
}

#[wasm_bindgen]
pub fn z_for_tail_target(delta: f64, epsilon: f64) -> f64 {
    math::z_for_tail_target(delta, epsilon)
}

#[wasm_bindgen]
pub fn r_saturation(epsilon: f64, sigma2: f64, window: f64, srtt: f64, t_sym: f64) -> f64 {
    math::r_saturation(epsilon, sigma2, window, srtt, t_sym)
}

/// Derived encoder window W* (paper 8.8): balances overhead (1/sqrt(W)
/// margin), recovery latency (W/send_rate within budget), and burst
/// absorbency. Clamped to [16, 512]. See `math::derive_window`.
#[wasm_bindgen]
pub fn derive_window(
    delta: f64, epsilon: f64, sigma2: f64, srtt: f64, send_rate: f64, latency_budget: f64,
) -> f64 {
    math::derive_window(delta, epsilon, sigma2, srtt, send_rate, latency_budget)
}

#[wasm_bindgen]
pub fn p_fec_exact(p_gb: f64, q_bg: f64, r: f64, window_size: usize) -> f64 {
    math::p_fec_exact(p_gb, q_bg, r, window_size)
}

#[wasm_bindgen]
pub fn compute_r_star_exact(p_gb: f64, q_bg: f64, window_size: usize, delta: f64) -> f64 {
    math::compute_r_star_exact(p_gb, q_bg, window_size, delta)
}

#[wasm_bindgen]
pub fn taper_density(t: f64, amplitude: f64, decay: f64) -> f64 {
    amplitude * decay.powf(t)
}

#[wasm_bindgen]
pub fn p_fec_normal(r: f64, epsilon: f64, window_size: f64, sigma2_burst: f64) -> f64 {
    math::p_fec_normal(r, epsilon, window_size, sigma2_burst)
}

#[wasm_bindgen]
pub fn compute_delta(epsilon: f64, r: f64, rho: f64, window_size: f64, sigma2_burst: f64) -> f64 {
    math::compute_delta(epsilon, r, rho, window_size, sigma2_burst)
}

#[wasm_bindgen]
pub fn find_t_cut(epsilon: f64, q: f64, r: f64, window_size: f64, sigma2_burst: f64, target_rho: f64) -> f64 {
    math::find_t_cut(epsilon, q, r, window_size, sigma2_burst, target_rho)
}

#[wasm_bindgen]
pub fn b_max(q: f64) -> u64 {
    math::b_max(q)
}

/// The SHARED production rate formula (raptorpath-math::controller_rate) —
/// the exact code path the real transport runs. Flat-args wrapper for JS.
/// `completion_exposure` is the P6 chi in [0,1] (paper 14.26); pass 0 for
/// mid-stream / unknown T_rem.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn controller_rate(
    p_upper: f64, sigma2: f64, mean_burst: f64, window: f64,
    t_symbols: f64, srtt: f64, t_sym: f64, codec_overhead: f64,
    tail_target: f64, bulk_late_is_fine: bool, completion_exposure: f64,
    saturation_cap: bool, max_overhead: f64,
) -> f64 {
    math::controller_rate(&math::RateInputs {
        p_upper, sigma2, mean_burst, window, t_symbols, srtt, t_sym,
        codec_overhead, tail_target, bulk_late_is_fine, completion_exposure,
        // Paper 14.28: the visualizer models a raw transfer (file-transfer
        // semantics — no latency-feedback payload inside), so the
        // inner-feedback repair floor stays off (as does production by
        // default after the negative L1 C2/C3 ablation).
        inner_feedback: 0.0,
        saturation_cap, max_overhead,
    })
}

/// Completion-exposure kernel chi(T_rem) (paper 14.26): the probability a
/// loss NOW can no longer hide behind ongoing sends.
#[wasm_bindgen]
pub fn completion_exposure(t_rem_secs: f64, srtt_secs: f64, rttvar_secs: f64) -> f64 {
    math::completion_exposure(t_rem_secs, srtt_secs, rttvar_secs)
}

// =========================================================================
// Three-variable solvers — kept for the triangle explainer panel
// =========================================================================

#[wasm_bindgen]
pub fn solve_r_from_delta_rho(epsilon: f64, q: f64, window_size: f64, sigma2_burst: f64, delta: f64, rho: f64) -> Vec<f64> {
    let res = math::solve_r_from_delta_rho(epsilon, q, window_size, sigma2_burst, delta, rho);
    vec![res.r, res.delta, res.rho, res.t_cut, res.buffer_max]
}

#[wasm_bindgen]
pub fn solve_delta_from_r_rho(epsilon: f64, q: f64, window_size: f64, sigma2_burst: f64, r: f64, rho: f64) -> Vec<f64> {
    let res = math::solve_delta_from_r_rho(epsilon, q, window_size, sigma2_burst, r, rho);
    vec![res.r, res.delta, res.rho, res.t_cut, res.buffer_max]
}

#[wasm_bindgen]
pub fn solve_rho_from_r_delta(epsilon: f64, q: f64, window_size: f64, sigma2_burst: f64, r: f64, delta: f64) -> Vec<f64> {
    let res = math::solve_rho_from_r_delta(epsilon, q, window_size, sigma2_burst, r, delta);
    vec![res.r, res.delta, res.rho, res.t_cut, res.buffer_max]
}

// =========================================================================
// Simulation engine — runs the REAL improved algorithm:
//   - rate from the SHARED production formula (math::controller_rate):
//     continuous z_{delta/eps} margin, hint tail targets, Bulk pure-ARQ,
//     burst B/T term, saturation cap (paper 8.4 / 12.5 / 14.21)
//   - correction preemption over new source (paper C.1)
//   - per-slot repair/retransmit mix by P_lost (paper 5.4)
//   - honest feedback: the estimator learns outcomes one RTT after send,
//     fed with the TRUE loss pattern (record_counts + record_symbol)
//   - completion-tail FEC: for Bulk, the continuous completion-exposure
//     ramp (paper 14.26 — the sim KNOWS T_rem, so chi is fed per tick);
//     for the other hints, the one-shot end-of-stream burst (paper 14.25,
//     exact 8.7 DP), which the chi ramp subsumes as its limiting case
//
// Simplifications vs the L0 gate (documented): single path, fixed wire
// capacity (slots/tick) instead of a queue+Copa model, no jitter (so no
// encoder lag needed — the channel is FIFO).
// =========================================================================

const BASE_TAIL_TARGET: f64 = 1e-5;
const CODEC_OVERHEAD_RLC: f64 = 0.004;
const MAX_OVERHEAD: f64 = 0.5;
const TICK_SECS: f64 = 0.001;

#[wasm_bindgen]
pub struct Simulation {
    eps: f64,
    q: f64,
    sigma2_true: f64,
    hint_bulk: bool,
    tail_target: f64,
    fixed_r: Option<f64>,
    /// Ablation-only: revert Bulk to the pre-P6 mapping (delta_eff =
    /// min(0.1, p_hat) + one-shot tail burst). Never exposed to JS; set
    /// directly by native tests to isolate the completion-exposure change.
    legacy_bulk_delta: bool,
    /// Reliability target (rho). Below 1.0, losses older than T_cut are
    /// given up (paper 6.1 age eviction) — the third triangle corner.
    rho: f64,
    given_up: u32,
    /// Seqs the receiver has pruned (paper 6.2): late data for them is
    /// discarded, not delivered.
    given_up_seqs: std::collections::BTreeSet<u64>,
    /// Send tick per source seq (for delivery-latency measurement).
    send_tick: Vec<u32>,
    /// Delivery latency (ms) per delivered source symbol, in delivery order:
    /// (recovery_tick - send_tick) + one-way propagation.
    delivered_lat_ms: Vec<f64>,
    /// RFC 3550-style smoothed jitter over successive delivery latencies:
    /// J += (|D| - J) / 16.
    jitter_ms: f64,
    prev_lat_ms: f64,

    rtt_ticks: u32,
    srtt_secs: f64,
    rttvar_secs: f64,
    capacity: u32, // wire symbols per tick
    w: u32,        // encoder window

    /// Pre-generated per-WIRE-SYMBOL channel states: true = lost.
    channel_states: Vec<bool>,
    wire_idx: usize,

    tick: u32,
    source_done: bool,
    tail_flushed: bool,
    finished: bool,
    num_source: u32,

    estimator: math::LossEstimator,
    encoder: math::RlcEncoder,
    decoder: math::RlcDecoder,

    symbols: Vec<Symbol>,
    /// Source payloads by seq — retransmission source, independent of the
    /// encoder window (a retransmit long after eviction must still work).
    source_store: Vec<Vec<u8>>,
    next_seq: u32,

    /// Wire outcomes awaiting the ACK round-trip: (send_tick, arrived).
    feedback_queue: std::collections::VecDeque<(u32, bool)>,
    last_flush_tick: u32,
    pending_sent: u32,
    pending_ok: u32,

    // Rate state
    rate: f64,
    debt: f64,

    // Counters
    total_src: u32, total_fec: u32, total_arq: u32, total_lost: u32,
    cum_sent: u32, cum_arrived: u32, cum_decoded: u32,
    lost_pending: u32,
    last_src: u32, last_fec: u32, last_arq: u32, last_lost: u32,

    rng_state: u64,
}

struct Symbol {
    tick: u32,
    seq: u64,
    lost: bool,
    recovered: bool,
    last_retx_tick: i64,
}

fn xorshift64(state: &mut u64) -> f64 {
    let mut x = *state;
    x ^= x << 13; x ^= x >> 7; x ^= x << 17;
    *state = x;
    (x as f64) / (u64::MAX as f64)
}

/// Pre-generate channel states using the GE model (h_B = 1: Bad = lost),
/// then calibrate to the exact target epsilon for reproducible visuals.
fn generate_channel_states(p: f64, q: f64, target_eps: f64, n: usize, seed: u64) -> Vec<bool> {
    let mut rng = seed;
    let mut good = true;
    let mut states: Vec<bool> = (0..n).map(|_| {
        if good {
            if xorshift64(&mut rng) < p { good = false; }
        } else if xorshift64(&mut rng) < q {
            good = true;
        }
        !good
    }).collect();

    let actual = states.iter().filter(|&&l| l).count();
    let target = (target_eps * n as f64).round() as usize;
    if actual < target {
        let mut cands: Vec<(usize, u32)> = (0..n).filter(|&i| !states[i]).map(|i| {
            let adj = (i > 0 && states[i - 1]) as u32 + (i + 1 < n && states[i + 1]) as u32;
            (i, adj * 1000 + (xorshift64(&mut rng) * 999.0) as u32)
        }).collect();
        cands.sort_by(|a, b| b.1.cmp(&a.1));
        for &(i, _) in cands.iter().take(target - actual) { states[i] = true; }
    } else if actual > target {
        let mut cands: Vec<(usize, u32)> = (0..n).filter(|&i| states[i]).map(|i| {
            let adj = (i > 0 && states[i - 1]) as u32 + (i + 1 < n && states[i + 1]) as u32;
            (i, adj * 1000 + (xorshift64(&mut rng) * 999.0) as u32)
        }).collect();
        cands.sort_by(|a, b| a.1.cmp(&b.1));
        for &(i, _) in cands.iter().take(actual - target) { states[i] = false; }
    }
    states
}

impl Simulation {
    fn rng(&mut self) -> f64 {
        xorshift64(&mut self.rng_state)
    }

    /// Consume the next wire-slot channel outcome. true = lost.
    fn wire_lost(&mut self) -> bool {
        let lost = self.channel_states.get(self.wire_idx).copied().unwrap_or(false);
        self.wire_idx += 1;
        lost
    }

    /// Completion exposure chi (paper 14.26): the sim KNOWS the transfer
    /// length, so T_rem = remaining source symbols / send rate. The send
    /// rate is approximated by the wire capacity (Bulk mid-stream is
    /// nearly all source). Non-bulk hints keep chi = 0 (their tail target
    /// is a constant; the 14.25 burst covers their stream tail).
    fn completion_chi(&self) -> f64 {
        if !self.hint_bulk || self.legacy_bulk_delta {
            return 0.0;
        }
        let remaining = self.num_source.saturating_sub(self.next_seq) as f64;
        let t_rem_secs = remaining / self.capacity as f64 * TICK_SECS;
        math::completion_exposure(t_rem_secs, self.srtt_secs, self.rttvar_secs)
    }

    /// Rate from the SHARED production formula — identical code to the
    /// real transport's FecRateController (via raptorpath-math).
    fn controller_rate_now(&self) -> f64 {
        if let Some(r) = self.fixed_r {
            return r;
        }
        let ge = self.estimator.ge_estimator();
        let (sigma2, mean_burst) = if ge.is_valid() {
            (math::burst_variance_factor(ge.p_gb(), ge.p_bg()), ge.mean_burst_length())
        } else {
            (1.0, 1.0)
        };
        let t_symbols = if ge.is_valid() {
            (self.rtt_ticks as f64 * self.capacity as f64).max(1.0)
        } else {
            0.0
        };
        let p_upper = self.estimator.predictive_loss_upper(0.95);
        // Ablation arm: the pre-P6 Bulk mapping delta_eff = min(0.1, p_hat)
        // expressed through the plain tail_target path (equivalent by
        // construction; see test_ablation_p6_completion_exposure).
        let (tail_target, bulk_late_is_fine) = if self.legacy_bulk_delta && self.hint_bulk {
            ((0.1f64).min(p_upper), false)
        } else {
            (self.tail_target, self.hint_bulk)
        };
        math::controller_rate(&math::RateInputs {
            p_upper,
            sigma2,
            mean_burst,
            window: self.encoder.window_size().max(1) as f64,
            t_symbols,
            srtt: self.srtt_secs,
            t_sym: TICK_SECS / self.capacity as f64,
            codec_overhead: CODEC_OVERHEAD_RLC,
            tail_target,
            bulk_late_is_fine,
            completion_exposure: self.completion_chi(),
            // Paper 14.28: the sim's payload IS the transfer (file-transfer
            // semantics) — its delivery latency does not feed back into its
            // own throughput, so the inner-feedback repair floor stays off
            // and mid-stream Bulk remains pure ARQ (production defaults to
            // 0 too after the negative L1 C2/C3 ablation).
            inner_feedback: 0.0,
            saturation_cap: true,
            max_overhead: MAX_OVERHEAD,
        })
    }

    /// Send one repair symbol. Returns whether it survived the channel.
    fn send_repair(&mut self) -> bool {
        let repair = self.encoder.generate_repair();
        let lost = self.wire_lost();
        self.total_fec += 1;
        self.cum_sent += 1;
        self.feedback_queue.push_back((self.tick, !lost));
        if !lost {
            self.cum_arrived += 1;
            let recovered = self.decoder.feed_repair(
                repair.window_start, repair.window_count,
                repair.repair_index, &repair.coded_data,
            );
            self.mark_recovered(&recovered);
            self.record_delivery(&recovered);
            self.cum_decoded += recovered
                .iter()
                .filter(|q| !self.given_up_seqs.contains(q))
                .count() as u32;
        }
        !lost
    }

    /// Record delivery latencies for decoder outputs (excluding pruned
    /// seqs). Latency = time from send to decode plus one-way propagation
    /// (arrived symbols decode at their send tick, so their latency is the
    /// one-way delay; recovered symbols add the recovery wait).
    fn record_delivery(&mut self, seqs: &[u64]) {
        let one_way = self.srtt_secs * 500.0; // ms: RTT/2
        for q in seqs {
            if self.given_up_seqs.contains(q) {
                continue;
            }
            let st = self.send_tick.get(*q as usize).copied().unwrap_or(self.tick);
            let lat = (self.tick.saturating_sub(st)) as f64 + one_way;
            if self.prev_lat_ms >= 0.0 {
                let d = (lat - self.prev_lat_ms).abs();
                self.jitter_ms += (d - self.jitter_ms) / 16.0;
            }
            self.prev_lat_ms = lat;
            self.delivered_lat_ms.push(lat);
        }
    }

    fn mark_recovered(&mut self, seqs: &[u64]) {
        for rseq in seqs {
            for sym in &mut self.symbols {
                if sym.seq == *rseq && sym.lost && !sym.recovered {
                    sym.recovered = true;
                    self.lost_pending = self.lost_pending.saturating_sub(1);
                }
            }
        }
    }
}

#[wasm_bindgen]
impl Simulation {
    /// hint: "bulk" | "auto" | "realtime" | "fixed" (uses fixed_r as r) |
    /// "custom" (uses custom_delta + custom_rho — the full triangle).
    /// rho < 1.0 enables T_cut age eviction (losses older than T_cut are
    /// given up; reliability becomes the emergent third variable).
    #[wasm_bindgen(constructor)]
    pub fn new(
        eps: f64, q: f64, rtt_ms: u32, w: u32, hint: String,
        fixed_r: Option<f64>, custom_delta: Option<f64>, custom_rho: Option<f64>,
    ) -> Self {
        let p = if eps < 1.0 { eps * q / (1.0 - eps) } else { q };
        let sigma2_true = math::burst_variance_factor(p, q);

        // Hint -> tail target, mirroring the production constructor.
        let (tail_target, hint_bulk) = match hint.as_str() {
            "bulk" => ((BASE_TAIL_TARGET * 100.0).clamp(1e-9, 0.1), true),
            "realtime" => ((BASE_TAIL_TARGET * 0.01).clamp(1e-9, 0.1), false),
            "custom" => (custom_delta.unwrap_or(BASE_TAIL_TARGET).clamp(1e-9, 0.1), false),
            _ => (BASE_TAIL_TARGET, false), // auto / fixed
        };
        let fixed_r = if hint == "fixed" { Some(fixed_r.unwrap_or(0.1)) } else { None };
        let rho = if hint == "custom" {
            custom_rho.unwrap_or(1.0).clamp(0.9, 1.0)
        } else {
            1.0
        };

        let capacity = 4u32;
        let num_source = 2000u32;
        let seed = eps.to_bits() ^ q.to_bits().rotate_left(32)
            ^ (rtt_ms as u64).wrapping_mul(0x517cc1b727220a95);
        let num_wire = num_source as usize * 4; // generous margin incl. corrections
        let channel_states = generate_channel_states(p, q, eps, num_wire, seed);

        Self {
            eps, q, sigma2_true,
            hint_bulk, tail_target, fixed_r,
            legacy_bulk_delta: false,
            rho,
            given_up: 0,
            given_up_seqs: std::collections::BTreeSet::new(),
            send_tick: Vec::new(),
            delivered_lat_ms: Vec::new(),
            jitter_ms: 0.0,
            prev_lat_ms: -1.0,
            rtt_ticks: rtt_ms.max(2),
            srtt_secs: rtt_ms as f64 / 1000.0,
            rttvar_secs: rtt_ms as f64 / 8000.0,
            capacity,
            w,
            channel_states,
            wire_idx: 0,
            tick: 0,
            source_done: false,
            tail_flushed: false,
            finished: false,
            num_source,
            estimator: math::LossEstimator::new(),
            encoder: math::RlcEncoder::new(8),
            decoder: math::RlcDecoder::new(8),
            symbols: Vec::new(),
            source_store: Vec::new(),
            next_seq: 0,
            feedback_queue: std::collections::VecDeque::new(),
            last_flush_tick: 0,
            pending_sent: 0,
            pending_ok: 0,
            rate: 0.0,
            debt: 0.0,
            total_src: 0, total_fec: 0, total_arq: 0, total_lost: 0,
            cum_sent: 0, cum_arrived: 0, cum_decoded: 0,
            lost_pending: 0,
            last_src: 0, last_fec: 0, last_arq: 0, last_lost: 0,
            rng_state: 0xdeadbeef12345678,
        }
    }

    pub fn step(&mut self) {
        if self.finished {
            return;
        }
        let mut src_n = 0u32;
        let mut fec_n = 0u32;
        let mut arq_n = 0u32;
        let mut lost_n = 0u32;

        // --- Feedback path: outcomes become known one RTT after send. ---
        // Fed with the TRUE per-symbol pattern (paper 7.5); flushed once
        // per RTT — the estimator honestly lags reality.
        while let Some(&(t, ok)) = self.feedback_queue.front() {
            if self.tick.saturating_sub(t) < self.rtt_ticks {
                break;
            }
            self.feedback_queue.pop_front();
            self.estimator.record_symbol(ok);
            self.pending_sent += 1;
            if ok {
                self.pending_ok += 1;
            }
        }
        if self.tick.saturating_sub(self.last_flush_tick) >= self.rtt_ticks
            && self.pending_sent > 0
        {
            self.estimator
                .record_counts(self.pending_sent, self.pending_ok, self.tick as u64);
            self.pending_sent = 0;
            self.pending_ok = 0;
            self.last_flush_tick = self.tick;
        }

        // --- Rate from the shared production formula ---
        self.rate = self.controller_rate_now();

        // --- Wire slots ---
        // Per-slot priority (paper C.1 + 5.4): (1) a P_lost-confirmed
        // retransmit — ARQ is driven by loss confidence, INDEPENDENT of the
        // FEC budget (Bulk's r ~ 0 must not delay recovery to end of
        // stream); (2) a repair when the taper debt says one is due;
        // (3) new source. Retransmits scan ALL outstanding candidates so a
        // single symbol waiting out its retry timer cannot head-of-line
        // stall the drain.
        for _ in 0..self.capacity {
            // (1) P_lost-gated retransmit across all candidates.
            let mut did_retx = false;
            let mut cand: Option<usize> = None;
            for (i, sym) in self.symbols.iter().enumerate() {
                if sym.lost
                    && !sym.recovered
                    && (self.tick as i64 - sym.last_retx_tick) >= self.rtt_ticks as i64
                {
                    cand = Some(i);
                    break;
                }
            }
            if let Some(i) = cand {
                let age_ticks = self.tick.saturating_sub(self.symbols[i].tick);
                let pl = math::p_lost(
                    age_ticks as f64 * TICK_SECS,
                    self.estimator.loss_rate().clamp(1e-4, 0.99),
                    self.srtt_secs,
                    self.rttvar_secs,
                );
                if self.rng() < pl {
                    let seq = self.symbols[i].seq;
                    self.symbols[i].last_retx_tick = self.tick as i64;
                    let lost = self.wire_lost();
                    self.total_arq += 1;
                    arq_n += 1;
                    self.cum_sent += 1;
                    self.feedback_queue.push_back((self.tick, !lost));
                    if lost {
                        lost_n += 1;
                    } else {
                        self.cum_arrived += 1;
                        let data = self.source_store[seq as usize].clone();
                        let rec = self.decoder.feed_source(seq, &data);
                        // Cascade outputs may resolve OTHER lost symbols;
                        // pruned (given-up) seqs are discarded, not counted.
                        self.mark_recovered(&rec);
                        self.record_delivery(&rec);
                        self.cum_decoded += rec
                            .iter()
                            .filter(|q| !self.given_up_seqs.contains(q))
                            .count() as u32;
                        self.symbols[i].recovered = true;
                        self.lost_pending = self.lost_pending.saturating_sub(1);
                    }
                    did_retx = true;
                }
            }
            if did_retx {
                continue;
            }

            // (2) Repair when the taper debt says one is due.
            if self.debt >= 1.0 && self.encoder.window_size() > 0 {
                self.debt -= 1.0;
                let ok = self.send_repair();
                fec_n += 1;
                if !ok {
                    lost_n += 1;
                }
            } else if !self.source_done {
                // --- Source slot ---
                let data = vec![self.next_seq as u8; 8];
                let seq = self.encoder.add_source(&data);
                self.source_store.push(data.clone());
                self.send_tick.push(self.tick);
                // Slide the encoder window (paper W).
                if self.encoder.window_size() > self.w as usize {
                    let oldest = self.encoder.next_seq().saturating_sub(self.w as u64);
                    self.encoder.advance(oldest);
                }
                let lost = self.wire_lost();
                self.feedback_queue.push_back((self.tick, !lost));
                self.symbols.push(Symbol {
                    tick: self.tick,
                    seq,
                    lost,
                    recovered: false,
                    last_retx_tick: -1_000_000,
                });
                self.total_src += 1;
                src_n += 1;
                self.cum_sent += 1;
                // Steady-state debt accrual: r per SOURCE symbol — the
                // aggregate correction rate is taper-shape-invariant
                // (paper 4.2).
                self.debt = (self.debt + self.rate).min(8.0);
                if lost {
                    lost_n += 1;
                    self.total_lost += 1;
                    self.lost_pending += 1;
                } else {
                    self.cum_arrived += 1;
                    let rec = self.decoder.feed_source(seq, &data);
                    // Cascade outputs may resolve OTHER lost symbols.
                    self.mark_recovered(&rec);
                    self.record_delivery(&rec);
                    self.cum_decoded += rec
                        .iter()
                        .filter(|q| !self.given_up_seqs.contains(q))
                        .count() as u32;
                }
                self.next_seq += 1;
                if self.next_seq >= self.num_source {
                    self.source_done = true;
                    // Completion-tail FEC (paper 14.25): burst of repairs
                    // covering the final window, sized by the exact 8.7 DP.
                    // NOT for Bulk under P6 (paper 14.26): the continuous
                    // chi ramp already raised r over the final ~1.5 SRTT —
                    // the burst is the ramp's limiting case, so firing both
                    // would double-pay the tail budget.
                    let chi_replaces_burst = self.hint_bulk && !self.legacy_bulk_delta;
                    if !self.tail_flushed && !chi_replaces_burst {
                        self.tail_flushed = true;
                        let ge = self.estimator.ge_estimator();
                        let (pg, qg) = if ge.is_valid() && ge.p_gb() > 0.0 && ge.p_bg() > 0.0 {
                            (ge.p_gb(), ge.p_bg())
                        } else {
                            (self.eps * self.q / (1.0 - self.eps).max(1e-6), self.q)
                        };
                        let r_tail = math::compute_r_star_exact(pg, qg, self.w as usize, 0.05);
                        let n_tail = (r_tail * self.w as f64).ceil().min(24.0);
                        self.debt += n_tail;
                    }
                }
            }
        }

        // T_cut age eviction (paper 6.1): for rho < 1.0, losses older than
        // T_cut are given up — reliability bends instead of latency.
        if self.rho < 1.0 {
            let p_up = self.estimator.loss_rate().clamp(1e-4, 0.99);
            let sig = self.get_sigma2_est();
            let t_cut = math::find_t_cut(
                p_up, self.q, self.rate.max(1e-3),
                self.encoder.window_size().max(1) as f64, sig, self.rho,
            );
            if t_cut.is_finite() {
                let cut_ticks = (t_cut as u32).max(self.rtt_ticks * 2);
                let now = self.tick;
                for sym in &mut self.symbols {
                    if sym.lost && !sym.recovered && now.saturating_sub(sym.tick) > cut_ticks {
                        sym.recovered = true; // given up
                        self.given_up += 1;
                        self.given_up_seqs.insert(sym.seq);
                        self.lost_pending = self.lost_pending.saturating_sub(1);
                    }
                }
            }
        }

        // ACK arrived-but-unmarked symbols after one RTT (SACK view).
        let rtt = self.rtt_ticks;
        let tick = self.tick;
        for sym in &mut self.symbols {
            if !sym.lost && !sym.recovered && tick.saturating_sub(sym.tick) >= rtt {
                sym.recovered = true;
            }
        }

        // Finish conditions (rho = 100%: retransmit until everything is in;
        // rho < 100%: given-up symbols count as resolved, not delivered).
        if self.source_done && self.cum_decoded + self.given_up >= self.num_source {
            self.finished = true;
        }
        if self.source_done
            && self.lost_pending == 0
            && !self.symbols.iter().any(|s| s.lost && !s.recovered)
        {
            self.finished = true;
        }
        if self.tick > 20_000 {
            self.finished = true;
        }

        // Prune resolved symbols (keep lost unrecovered for ARQ).
        self.symbols
            .retain(|s| (s.lost && !s.recovered) || tick.saturating_sub(s.tick) < 10);

        self.last_src = src_n;
        self.last_fec = fec_n;
        self.last_arq = arq_n;
        self.last_lost = lost_n;
        self.tick += 1;
    }

    // --- Accessors (superset of the old API; UI-compatible names) ---
    pub fn is_finished(&self) -> bool { self.finished }
    pub fn channel_is_good(&self) -> bool {
        !self.channel_states.get(self.wire_idx.saturating_sub(1)).copied().unwrap_or(false)
    }
    pub fn get_tick(&self) -> u32 { self.tick }
    pub fn get_src(&self) -> u32 { self.last_src }
    pub fn get_fec(&self) -> u32 { self.last_fec }
    pub fn get_arq(&self) -> u32 { self.last_arq }
    pub fn get_lost(&self) -> u32 { self.last_lost }
    pub fn get_cum_sent(&self) -> u32 { self.cum_sent }
    pub fn get_cum_arrived(&self) -> u32 { self.cum_arrived }
    pub fn get_cum_decoded(&self) -> u32 { self.cum_decoded }
    pub fn get_total_src(&self) -> u32 { self.total_src }
    pub fn get_total_fec(&self) -> u32 { self.total_fec }
    pub fn get_total_arq(&self) -> u32 { self.total_arq }
    pub fn get_total_lost(&self) -> u32 { self.total_lost }
    /// Live controller rate (the shared production formula's output).
    pub fn get_r_star(&self) -> f64 { self.rate }
    /// Reference: the closed-form continuous r* at the TRUE channel params.
    pub fn get_r_star_auto(&self) -> f64 {
        math::compute_r_star_with_z(
            self.eps, self.sigma2_true, self.w as f64,
            math::z_for_tail_target(self.tail_target, self.eps),
        )
    }
    pub fn get_sigma2(&self) -> f64 { self.sigma2_true }
    pub fn get_retx_buf_size(&self) -> u32 { self.lost_pending }
    pub fn get_num_source(&self) -> u32 { self.num_source }
    pub fn get_estimated_loss(&self) -> f64 { self.estimator.loss_rate() }
    /// Conservative loss estimate the controller actually uses (BOCD 95%).
    pub fn get_p_upper(&self) -> f64 { self.estimator.predictive_loss_upper(0.95) }
    /// Estimated burst variance factor (from the live GE estimator).
    pub fn get_sigma2_est(&self) -> f64 {
        let ge = self.estimator.ge_estimator();
        if ge.is_valid() {
            math::burst_variance_factor(ge.p_gb(), ge.p_bg())
        } else {
            1.0
        }
    }
    /// Effective tail target after the hint mapping. Bulk (paper 14.26):
    /// the completion-exposure glide p̂ + (0.05 − p̂)·χ — equals p̂
    /// mid-stream (pure ARQ) and the 14.25 tail budget as χ → 1.
    pub fn get_delta_eff(&self) -> f64 {
        if self.hint_bulk {
            let p = self.get_p_upper();
            p + (math::BULK_TAIL_BUDGET - p) * self.completion_chi()
        } else {
            self.tail_target
        }
    }
    /// Live completion exposure chi (paper 14.26): 0 mid-stream, ramps to
    /// 1 over the final ~1.5 SRTT of the transfer (Bulk only).
    pub fn get_completion_exposure(&self) -> f64 {
        self.completion_chi()
    }
    /// Saturation cap for the current estimator state (paper 14.21).
    pub fn get_r_sat(&self) -> f64 {
        math::r_saturation(
            self.get_p_upper(),
            self.get_sigma2_est(),
            self.encoder.window_size().max(1) as f64,
            self.srtt_secs,
            TICK_SECS / self.capacity as f64,
        )
    }
    /// Derived encoder window W* (paper 8.8) for the current estimator state
    /// and hint tail target — the window the controller WOULD choose. The UI
    /// shows this next to the user's slider W so the tradeoff is visible:
    /// larger W thins the r* overhead margin (1/sqrt(W)) but stretches
    /// recovery latency (W/send_rate). send_rate = capacity / tick.
    pub fn get_derived_w(&self) -> f64 {
        math::derive_window(
            self.tail_target,
            self.get_p_upper().max(1e-6),
            self.get_sigma2_est(),
            self.srtt_secs,
            self.capacity as f64 / TICK_SECS, // source symbols per second
            0.0,                              // budget = 0 => align to ~1 RTT
        )
    }
    /// Symbols permanently given up (rho < 1.0 age eviction).
    pub fn get_given_up(&self) -> u32 { self.given_up }
    /// Latency (ms) of the most recently delivered source symbol.
    pub fn get_lat_last(&self) -> f64 {
        self.delivered_lat_ms.last().copied().unwrap_or(0.0)
    }
    /// Mean delivery latency (ms) over all delivered source symbols.
    pub fn get_lat_avg(&self) -> f64 {
        if self.delivered_lat_ms.is_empty() { return 0.0; }
        self.delivered_lat_ms.iter().sum::<f64>() / self.delivered_lat_ms.len() as f64
    }
    /// Delivery latency percentile (ms), pct in [0, 1].
    pub fn get_lat_percentile(&self, pct: f64) -> f64 {
        if self.delivered_lat_ms.is_empty() { return 0.0; }
        let mut v = self.delivered_lat_ms.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = ((v.len() as f64 - 1.0) * pct.clamp(0.0, 1.0)).round() as usize;
        v[idx.min(v.len() - 1)]
    }
    /// RFC 3550-style smoothed delivery jitter (ms).
    pub fn get_jitter(&self) -> f64 { self.jitter_ms }
    /// Achieved reliability so far: delivered / sent source symbols.
    pub fn get_reliability(&self) -> f64 {
        if self.total_src > 0 {
            self.cum_decoded as f64 / self.total_src as f64
        } else {
            1.0
        }
    }
    /// Configured reliability target rho.
    pub fn get_rho(&self) -> f64 { self.rho }
    /// The channel's information-theoretic overhead floor: with loss rate
    /// eps, EVERY reliable scheme must send at least 1/(1-eps) symbols per
    /// delivered symbol -- a floor of eps/(1-eps) extra, no matter how
    /// clever. As a percentage of source symbols.
    pub fn get_overhead_floor(&self) -> f64 {
        if self.eps < 1.0 {
            self.eps / (1.0 - self.eps) * 100.0
        } else {
            0.0
        }
    }
    /// The REALIZED channel floor for this specific run: over the wire slots
    /// actually consumed, the loss rate was (sent - arrived)/sent, which
    /// differs from the nominal eps because a finite run samples only a
    /// prefix of the (expectation-calibrated) channel. The nominal floor is
    /// unbeatable only in expectation; this one is what THIS channel forced.
    /// As a percentage of delivered symbols: lost/arrived = eps_real/(1-eps_real).
    pub fn get_overhead_floor_realized(&self) -> f64 {
        if self.cum_arrived > 0 {
            (self.cum_sent.saturating_sub(self.cum_arrived)) as f64 / self.cum_arrived as f64
                * 100.0
        } else {
            0.0
        }
    }
    /// Excess overhead: what we actually spent BEYOND the channel floor THIS
    /// run forced -- the honest inefficiency measure (0% = information-
    /// theoretically perfect; the floor itself is the channel's fault, not
    /// the protocol's). Measured against the REALIZED floor so it is a true
    /// invariant: overhead (all sends) >= realized floor always, because
    /// decoding N source symbols needs >= N arrived symbols, giving
    /// overhead - floor = sent*(arrived - N)/(N*arrived) >= 0.
    pub fn get_excess_overhead(&self) -> f64 {
        (self.get_overhead() - self.get_overhead_floor_realized()).max(0.0)
    }
    /// Total overhead over ALL sends (steady stream AND post-stream drain):
    /// (FEC + ARQ) / source symbols, in percent. Counting only the steady
    /// phase (as an earlier version did) undercounts, because corrections
    /// fired during the drain -- the tail-FEC burst and the final ARQ
    /// retransmits -- are real bandwidth and can push realized loss recovery
    /// below the floor's whole-transfer semantics.
    pub fn get_overhead(&self) -> f64 {
        if self.total_src > 0 {
            (self.total_fec + self.total_arq) as f64 / self.total_src as f64 * 100.0
        } else {
            0.0
        }
    }
    pub fn get_recovery(&self) -> f64 {
        if self.total_src > 0 {
            self.cum_decoded as f64 / self.total_src as f64 * 100.0
        } else {
            100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_generation_loss_rate() {
        let states = generate_channel_states(0.0556, 0.5, 0.10, 4000, 42);
        let losses = states.iter().filter(|&&l| l).count();
        let rate = losses as f64 / states.len() as f64;
        assert!((rate - 0.10).abs() < 0.01, "Expected ~10% loss, got {:.1}%", rate * 100.0);
    }

    #[test]
    fn test_simulation_runs_to_completion_auto() {
        let mut sim = Simulation::new(0.05, 0.5, 50, 64, "auto".into(), None, None, None);
        while !sim.is_finished() && sim.get_tick() < 20_000 {
            sim.step();
        }
        assert!(sim.is_finished());
        assert_eq!(sim.get_cum_decoded(), sim.get_num_source(),
            "all source symbols must be delivered");
        // Auto at 5% loss should carry meaningful FEC
        assert!(sim.get_total_fec() > 0, "Auto must send repairs");
    }

    // Paper 8.8 quality check: derived W vs a fixed W = 64 across the three
    // reference channels. Runs the SAME simulator both ways and reports
    // overhead (FEC/source) and p99 delivery latency. Prints a table with
    // `--nocapture`; asserts the derived window never loses on overhead where
    // the 1/sqrt(W) margin is fattest (Satellite) while staying complete.
    #[test]
    fn test_derived_window_quality_vs_fixed64() {
        struct Ch { name: &'static str, eps: f64, q: f64, rtt: u32 }
        let channels = [
            Ch { name: "DC",        eps: 0.001, q: 0.5, rtt: 5 },
            Ch { name: "WiFi",      eps: 0.025, q: 0.5, rtt: 13 },
            Ch { name: "Satellite", eps: 0.09,  q: 0.3, rtt: 210 },
        ];
        // Hint "auto" => tail_target = BASE_TAIL_TARGET. send_rate = capacity/tick.
        let delta = BASE_TAIL_TARGET;
        let send_rate = 4.0 / TICK_SECS;

        let run = |eps: f64, q: f64, rtt: u32, w: u32| -> (f64, f64, bool) {
            let mut sim = Simulation::new(eps, q, rtt, w, "auto".into(), None, None, None);
            while !sim.is_finished() && sim.get_tick() < 200_000 { sim.step(); }
            let overhead = sim.get_total_fec() as f64 / sim.get_total_src().max(1) as f64;
            let p99 = sim.get_lat_percentile(0.99);
            let complete = sim.get_cum_decoded() == sim.get_num_source();
            (overhead, p99, complete)
        };

        println!("\npaper 8.8 quality: derived W vs fixed W=64 (hint=auto, delta={delta:.0e})");
        println!("  {:<10} {:>4} {:>6} | {:>8} {:>8} | {:>8} {:>8}",
            "channel", "W64", "W*", "oh(64)", "oh(W*)", "p99(64)", "p99(W*)");
        for ch in &channels {
            let p = ch.eps * ch.q / (1.0 - ch.eps);
            let sigma2 = math::burst_variance_factor(p, ch.q);
            let srtt = ch.rtt as f64 / 1000.0;
            let wstar = math::derive_window(delta, ch.eps, sigma2, srtt, send_rate, 0.0).round() as u32;
            let (oh64, p99_64, c64) = run(ch.eps, ch.q, ch.rtt, 64);
            let (ohw, p99_w, cw) = run(ch.eps, ch.q, ch.rtt, wstar);
            println!("  {:<10} {:>4} {:>6} | {:>7.1}% {:>7.1}% | {:>6.1}ms {:>6.1}ms",
                ch.name, 64, wstar, oh64 * 100.0, ohw * 100.0, p99_64, p99_w);
            assert!(c64 && cw, "{}: both runs must deliver all source", ch.name);
            if ch.name == "Satellite" {
                // High-eps channel: the derived (larger) window must not cost
                // MORE overhead than fixed 64 — the 1/sqrt(W) margin is fattest here.
                assert!(ohw <= oh64 + 0.005,
                    "Satellite derived-W overhead {ohw:.3} should not exceed fixed-64 {oh64:.3}");
            }
        }
    }

    #[test]
    fn test_bulk_is_mostly_arq() {
        let mut sim_bulk = Simulation::new(0.05, 0.5, 50, 64, "bulk".into(), None, None, None);
        let mut sim_auto = Simulation::new(0.05, 0.5, 50, 64, "auto".into(), None, None, None);
        while !sim_bulk.is_finished() && sim_bulk.get_tick() < 20_000 { sim_bulk.step(); }
        while !sim_auto.is_finished() && sim_auto.get_tick() < 20_000 { sim_auto.step(); }
        assert!(sim_bulk.is_finished() && sim_auto.is_finished());
        // Bulk pure-ARQ: steady-state overhead well below Auto's
        assert!(
            sim_bulk.get_overhead() < sim_auto.get_overhead(),
            "bulk {}% should be below auto {}%",
            sim_bulk.get_overhead(), sim_auto.get_overhead()
        );
        assert_eq!(sim_bulk.get_cum_decoded(), sim_bulk.get_num_source());
    }

    #[test]
    fn test_bulk_completes_faster_than_realtime() {
        // Bulk sends ~no FEC (wire budget goes to source) and recovers via
        // P_lost-driven ARQ in parallel with the stream + tail FEC — its
        // completion must BEAT Realtime's (which pays r ~ 20%+ of the wire
        // for corrections). Mirrors the L0 gate result (0.163s vs 0.187s).
        let mut bulk = Simulation::new(0.05, 0.5, 50, 64, "bulk".into(), None, None, None);
        let mut rt = Simulation::new(0.05, 0.5, 50, 64, "realtime".into(), None, None, None);
        while !bulk.is_finished() && bulk.get_tick() < 20_000 { bulk.step(); }
        while !rt.is_finished() && rt.get_tick() < 20_000 { rt.step(); }
        assert!(bulk.is_finished() && rt.is_finished());
        assert_eq!(bulk.get_cum_decoded(), bulk.get_num_source());
        assert!(
            bulk.get_tick() < rt.get_tick(),
            "bulk ({} ticks) must complete before realtime ({} ticks)",
            bulk.get_tick(), rt.get_tick()
        );
    }

    #[test]
    fn test_latency_metrics_show_the_trade() {
        // Realtime buys a tighter latency tail with FEC; Bulk gives the
        // wire to source and pays the tail via ARQ waits. Both should have
        // avg latency >= one-way propagation.
        let mut rt = Simulation::new(0.05, 0.5, 50, 64, "realtime".into(), None, None, None);
        let mut bulk = Simulation::new(0.05, 0.5, 50, 64, "bulk".into(), None, None, None);
        while !rt.is_finished() && rt.get_tick() < 20_000 { rt.step(); }
        while !bulk.is_finished() && bulk.get_tick() < 20_000 { bulk.step(); }
        let one_way = 25.0;
        assert!(rt.get_lat_avg() >= one_way && bulk.get_lat_avg() >= one_way);
        let rt_p99 = rt.get_lat_percentile(0.99);
        let bulk_p99 = bulk.get_lat_percentile(0.99);
        assert!(
            rt_p99 < bulk_p99,
            "realtime p99 latency ({rt_p99:.1}ms) must beat bulk ({bulk_p99:.1}ms)"
        );
        assert!(rt.get_jitter() >= 0.0 && rt.get_jitter().is_finite());
        assert_eq!(
            rt.delivered_lat_ms.len(),
            rt.get_num_source() as usize,
            "every delivered symbol has a latency sample"
        );
    }

    #[test]
    fn test_excess_overhead_metric() {
        // Floor at eps=0.05: 5/95 = 5.26%. Bulk (mostly ARQ) should sit
        // close to the floor; excess = overhead - floor >= 0 always.
        let mut sim = Simulation::new(0.05, 0.5, 50, 64, "bulk".into(), None, None, None);
        while !sim.is_finished() && sim.get_tick() < 20_000 { sim.step(); }
        let floor = sim.get_overhead_floor();
        assert!((floor - 5.263).abs() < 0.01, "floor {floor}");
        assert!(sim.get_excess_overhead() >= 0.0);
        assert!(
            sim.get_excess_overhead() <= sim.get_overhead(),
            "excess cannot exceed total"
        );
    }

    #[test]
    fn test_custom_rho_gives_up_late_losses() {
        // Custom triangle mode: rho < 1 -> T_cut eviction; some symbols are
        // given up instead of retransmitted forever.
        let mut sim = Simulation::new(
            0.10, 0.3, 80, 64, "custom".into(), None, Some(0.05), Some(0.95),
        );
        while !sim.is_finished() && sim.get_tick() < 20_000 { sim.step(); }
        assert!(sim.is_finished());
        assert!(sim.get_reliability() >= 0.90, "reliability {}", sim.get_reliability());
        assert_eq!(
            sim.get_cum_decoded() + sim.get_given_up(),
            sim.get_num_source()
        );
    }

    #[test]
    fn test_realtime_more_fec_than_bulk() {
        let mut rt = Simulation::new(0.05, 0.5, 50, 64, "realtime".into(), None, None, None);
        let mut bulk = Simulation::new(0.05, 0.5, 50, 64, "bulk".into(), None, None, None);
        while !rt.is_finished() && rt.get_tick() < 20_000 { rt.step(); }
        while !bulk.is_finished() && bulk.get_tick() < 20_000 { bulk.step(); }
        assert!(rt.get_total_fec() > bulk.get_total_fec(),
            "realtime fec {} should exceed bulk fec {}",
            rt.get_total_fec(), bulk.get_total_fec());
    }

    #[test]
    fn test_fixed_r_mode() {
        let mut sim = Simulation::new(0.10, 0.5, 20, 64, "fixed".into(), Some(0.2), None, None);
        while !sim.is_finished() && sim.get_tick() < 20_000 { sim.step(); }
        assert!(sim.is_finished());
        let overhead = sim.get_overhead();
        assert!(overhead > 10.0 && overhead < 45.0,
            "fixed r=0.2 should give ~20-30% overhead incl. retx, got {overhead:.1}%");
    }

    fn run_to_end(sim: &mut Simulation) {
        while !sim.is_finished() && sim.get_tick() < 20_000 {
            sim.step();
        }
        assert!(sim.is_finished());
    }

    #[test]
    fn test_bulk_beats_fixed_001() {
        // P6 acceptance (paper 14.26): the old min(0.1, p_hat) mapping lost
        // to a fixed r = 0.01 floor on completion in 20/24 grid cells
        // (median +5%) and on overhead in 24/24 (excess overhead 2-14% vs
        // ~0-1%) — the M1 cold-start pin at max_overhead plus the M2
        // permanent-FEC leak. With the completion-exposure glide, Bulk must
        // match fixed(0.01) on completion (within 5%) and beat it on
        // overhead at the representative cell.
        let mut bulk = Simulation::new(0.05, 0.5, 50, 64, "bulk".into(), None, None, None);
        let mut fixed = Simulation::new(0.05, 0.5, 50, 64, "fixed".into(), Some(0.01), None, None);
        run_to_end(&mut bulk);
        run_to_end(&mut fixed);
        assert_eq!(bulk.get_cum_decoded(), bulk.get_num_source());
        println!(
            "eps=0.05 rtt=50: bulk {} ticks / {:.2}% overhead vs fixed(0.01) {} ticks / {:.2}%",
            bulk.get_tick(), bulk.get_overhead(), fixed.get_tick(), fixed.get_overhead()
        );
        assert!(
            (bulk.get_tick() as f64) <= fixed.get_tick() as f64 * 1.05,
            "bulk completion {} ticks must be within 5% of fixed(0.01) {} ticks",
            bulk.get_tick(), fixed.get_tick()
        );
        assert!(
            bulk.get_overhead() < fixed.get_overhead() + 1.0,
            "bulk overhead {:.2}% must be < fixed(0.01) {:.2}% + 1pt",
            bulk.get_overhead(), fixed.get_overhead()
        );

        // M2 cell (eps = 0.10 >= the old 0.1 clamp): the old mapping paid
        // permanent FEC ~ the IT floor on top of ARQ (excess overhead
        // 8-14%); the chi glide must keep excess overhead under 2%.
        let mut bulk10 = Simulation::new(0.10, 0.5, 50, 64, "bulk".into(), None, None, None);
        run_to_end(&mut bulk10);
        assert_eq!(bulk10.get_cum_decoded(), bulk10.get_num_source());
        assert!(
            bulk10.get_excess_overhead() < 2.0,
            "bulk excess overhead at eps=0.10 must be < 2%: {:.2}% (total {:.2}%, floor {:.2}%)",
            bulk10.get_excess_overhead(), bulk10.get_overhead(), bulk10.get_overhead_floor()
        );
    }

    #[test]
    fn test_bulk_chi_ramps_at_stream_tail() {
        // chi must be exactly 0 mid-stream (r* = 0 identically, even during
        // the estimator cold start) and ramp toward 1 by end of stream.
        let mut sim = Simulation::new(0.05, 0.5, 50, 64, "bulk".into(), None, None, None);
        // Cold start: first ticks have p_upper ~ 0.975 but chi = 0 -> r = 0.
        sim.step();
        assert_eq!(sim.get_completion_exposure(), 0.0);
        assert!(
            sim.get_p_upper() > 0.5,
            "cold-start upper quantile is pessimistic: {}",
            sim.get_p_upper()
        );
        assert_eq!(sim.get_r_star(), 0.0, "M1: cold-start Bulk rate must be 0");
        // Mid-stream: still 0.
        for _ in 0..200 { sim.step(); }
        assert_eq!(sim.get_completion_exposure(), 0.0);
        // Run until the source is done: chi must have ramped up.
        while !sim.source_done && sim.get_tick() < 20_000 { sim.step(); }
        assert!(
            sim.get_completion_exposure() > 0.95,
            "chi at end of stream: {}",
            sim.get_completion_exposure()
        );
        run_to_end(&mut sim);
        assert_eq!(sim.get_cum_decoded(), sim.get_num_source());
    }

    #[test]
    fn test_ablation_p6_completion_exposure() {
        // Old mapping (delta_eff = min(0.1, p_hat) + one-shot tail burst)
        // vs the P6 chi glide, same seeds (the channel seed derives from
        // eps/q/rtt only). Run with --nocapture to record the deltas.
        //
        // The rtt=150 cell is the documented horizon caveat (paper 14.26):
        // a 2000-symbol transfer (~0.5 s) fits entirely inside the chi
        // exposure horizon (~5.5 x SRTT at 150 ms), so chi > 0 from the
        // first tick and the cold-start estimator still pins the early
        // rate in BOTH arms — the glide's r* = 0 guarantee needs a genuine
        // mid-stream phase. Gate: improvement where mid-stream exists
        // (rtt=50 cells), no-regression elsewhere.
        for (eps, q, rtt) in [(0.05, 0.5, 50u32), (0.10, 0.5, 50u32), (0.05, 0.5, 150u32)] {
            let mut new = Simulation::new(eps, q, rtt, 64, "bulk".into(), None, None, None);
            let mut old = Simulation::new(eps, q, rtt, 64, "bulk".into(), None, None, None);
            old.legacy_bulk_delta = true;
            run_to_end(&mut new);
            run_to_end(&mut old);
            assert_eq!(new.get_cum_decoded(), new.get_num_source());
            assert_eq!(old.get_cum_decoded(), old.get_num_source());
            println!(
                "eps={eps} rtt={rtt}: completion {} -> {} ticks, overhead {:.2}% -> {:.2}% (excess {:.2}% -> {:.2}%)",
                old.get_tick(), new.get_tick(),
                old.get_overhead(), new.get_overhead(),
                old.get_excess_overhead(), new.get_excess_overhead()
            );
            // The glide must not regress completion anywhere...
            assert!(
                (new.get_tick() as f64) <= old.get_tick() as f64 * 1.05,
                "eps={eps} rtt={rtt}: completion regressed {} -> {}",
                old.get_tick(), new.get_tick()
            );
            // ...and must cut overhead wherever a mid-stream (chi = 0)
            // phase exists; at rtt=150 (horizon caveat above) the gate is
            // no-regression within noise.
            if rtt <= 50 {
                assert!(
                    new.get_overhead() < old.get_overhead(),
                    "eps={eps} rtt={rtt}: overhead must improve: {:.2}% -> {:.2}%",
                    old.get_overhead(), new.get_overhead()
                );
            } else {
                assert!(
                    new.get_overhead() <= old.get_overhead() + 1.0,
                    "eps={eps} rtt={rtt}: overhead regressed: {:.2}% -> {:.2}%",
                    old.get_overhead(), new.get_overhead()
                );
            }
        }
    }

    #[test]
    fn test_fixed_r_sets_realized_rate() {
        // "Fixed r" pins the CONTROLLER (correction) rate: the realized FEC
        // rate tracks the target within noise. Overhead and excess-overhead
        // are DIFFERENT metrics -- overhead adds the ARQ that recovers the
        // channel's losses (the floor); excess = overhead - realized floor,
        // which comes out near r only because at small r the FEC barely
        // displaces ARQ. This test asserts the realized FEC rate == target r,
        // NOT that overhead == r (the naming trap the UI must not imply).
        for &fr in &[0.01f64, 0.05, 0.10] {
            for &eps in &[0.02f64, 0.05, 0.10] {
                for &rtt in &[20u32, 50, 100] {
                    let mut sim = Simulation::new(
                        eps, 0.5, rtt, 64, "fixed".into(), Some(fr), None, None,
                    );
                    run_to_end(&mut sim);
                    let fec_rate = sim.total_fec as f64 / sim.total_src as f64;
                    // Realized FEC rate is r plus a small, bounded tail-flush
                    // constant (the end-of-stream burst, <= 24 repairs over
                    // 2000 source ~ +0.012); never a systematic multiple.
                    assert!(
                        fec_rate >= fr - 0.005 && fec_rate <= fr + 0.02,
                        "fixed r={fr} eps={eps} rtt={rtt}: realized FEC rate \
                         {fec_rate:.4} must track target (not diverge)"
                    );
                }
            }
        }
    }

    #[test]
    fn test_overhead_never_below_realized_floor() {
        // The reported user bug: "overhead below the channel floor
        // (theoretically impossible)". Two confirmed causes: (a) overhead
        // was measured over the steady phase only, dropping the drain-phase
        // corrections; (b) the nominal floor uses eps, but a finite run
        // samples only a prefix of the expectation-calibrated channel, so
        // realized loss can sit below eps. Fix: overhead counts ALL sends,
        // and the invariant is stated against the REALIZED floor -- which is
        // exact: decoding N source symbols requires >= N arrived symbols.
        for hint in ["bulk", "auto", "realtime", "fixed"] {
            for &eps in &[0.02f64, 0.05, 0.10, 0.15] {
                for &q in &[0.3f64, 0.5] {
                    for &rtt in &[20u32, 50, 100] {
                        let fr = if hint == "fixed" { Some(0.03) } else { None };
                        let mut sim = Simulation::new(eps, q, rtt, 64, hint.into(), fr, None, None);
                        run_to_end(&mut sim);
                        // rho = 1: every source symbol delivered.
                        assert_eq!(sim.get_cum_decoded(), sim.get_num_source());
                        let oh = sim.get_overhead();
                        let floor_real = sim.get_overhead_floor_realized();
                        assert!(
                            oh + 1e-9 >= floor_real,
                            "hint={hint} eps={eps} q={q} rtt={rtt}: overhead \
                             {oh:.4}% dropped below realized floor {floor_real:.4}%"
                        );
                        assert!(sim.get_excess_overhead() >= 0.0);
                    }
                }
            }
        }
    }

    #[test]
    fn test_estimator_converges_in_sim() {
        let mut sim = Simulation::new(0.10, 0.5, 20, 64, "auto".into(), None, None, None);
        for _ in 0..1500 {
            if sim.is_finished() { break; }
            sim.step();
        }
        let est = sim.get_estimated_loss();
        assert!((est - 0.10).abs() < 0.05,
            "estimator should converge to ~10%: got {est:.3}");
    }
}
