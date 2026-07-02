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
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn controller_rate(
    p_upper: f64, sigma2: f64, mean_burst: f64, window: f64,
    t_symbols: f64, srtt: f64, t_sym: f64, codec_overhead: f64,
    tail_target: f64, bulk_late_is_fine: bool, saturation_cap: bool,
    max_overhead: f64,
) -> f64 {
    math::controller_rate(&math::RateInputs {
        p_upper, sigma2, mean_burst, window, t_symbols, srtt, t_sym,
        codec_overhead, tail_target, bulk_late_is_fine, saturation_cap,
        max_overhead,
    })
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
//   - completion-tail FEC burst at end of stream (paper 14.25, exact 8.7 DP)
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
    /// Reliability target (rho). Below 1.0, losses older than T_cut are
    /// given up (paper 6.1 age eviction) — the third triangle corner.
    rho: f64,
    given_up: u32,
    /// Seqs the receiver has pruned (paper 6.2): late data for them is
    /// discarded, not delivered.
    given_up_seqs: std::collections::BTreeSet<u64>,

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
    steady_src: u32, steady_fec: u32, steady_arq: u32,
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
        math::controller_rate(&math::RateInputs {
            p_upper: self.estimator.predictive_loss_upper(0.95),
            sigma2,
            mean_burst,
            window: self.encoder.window_size().max(1) as f64,
            t_symbols,
            srtt: self.srtt_secs,
            t_sym: TICK_SECS / self.capacity as f64,
            codec_overhead: CODEC_OVERHEAD_RLC,
            tail_target: self.tail_target,
            bulk_late_is_fine: self.hint_bulk,
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
            self.cum_decoded += recovered
                .iter()
                .filter(|q| !self.given_up_seqs.contains(q))
                .count() as u32;
        }
        !lost
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
            rho,
            given_up: 0,
            given_up_seqs: std::collections::BTreeSet::new(),
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
            steady_src: 0, steady_fec: 0, steady_arq: 0,
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
                    if !self.tail_flushed {
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

        if !self.source_done {
            self.steady_src += src_n;
            self.steady_fec += fec_n;
            self.steady_arq += arq_n;
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
    /// Effective tail target after the hint mapping (Bulk: min(0.1, p̂)).
    pub fn get_delta_eff(&self) -> f64 {
        if self.hint_bulk {
            (0.1f64).min(self.get_p_upper())
        } else {
            self.tail_target
        }
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
    /// Symbols permanently given up (rho < 1.0 age eviction).
    pub fn get_given_up(&self) -> u32 { self.given_up }
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
    pub fn get_overhead(&self) -> f64 {
        if self.steady_src > 0 {
            (self.steady_fec + self.steady_arq) as f64 / self.steady_src as f64 * 100.0
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
