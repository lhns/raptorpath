use wasm_bindgen::prelude::*;
use raptorpath_math as math;

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

// =========================================================================
// Three-variable solvers — return results as flat arrays for JS
// =========================================================================

/// Solve mode 1: given (delta, rho) -> r. Returns [r, delta, rho, t_cut, buffer_max].
#[wasm_bindgen]
pub fn solve_r_from_delta_rho(epsilon: f64, q: f64, window_size: f64, sigma2_burst: f64, delta: f64, rho: f64) -> Vec<f64> {
    let res = math::solve_r_from_delta_rho(epsilon, q, window_size, sigma2_burst, delta, rho);
    vec![res.r, res.delta, res.rho, res.t_cut, res.buffer_max]
}

/// Solve mode 2: given (r, rho) -> delta. Returns [r, delta, rho, t_cut, buffer_max].
#[wasm_bindgen]
pub fn solve_delta_from_r_rho(epsilon: f64, q: f64, window_size: f64, sigma2_burst: f64, r: f64, rho: f64) -> Vec<f64> {
    let res = math::solve_delta_from_r_rho(epsilon, q, window_size, sigma2_burst, r, rho);
    vec![res.r, res.delta, res.rho, res.t_cut, res.buffer_max]
}

/// Solve mode 3: given (r, delta) -> rho. Returns [r, delta, rho, t_cut, buffer_max].
#[wasm_bindgen]
pub fn solve_rho_from_r_delta(epsilon: f64, q: f64, window_size: f64, sigma2_burst: f64, r: f64, delta: f64) -> Vec<f64> {
    let res = math::solve_rho_from_r_delta(epsilon, q, window_size, sigma2_burst, r, delta);
    vec![res.r, res.delta, res.rho, res.t_cut, res.buffer_max]
}

// =========================================================================
// Simulation engine — runs the full per-tick GE channel simulation
// =========================================================================

#[wasm_bindgen]
pub struct Simulation {
    eps: f64,
    q: f64,
    p: f64,
    sigma2: f64,
    r_star: f64,        // current r (adaptive or static)
    r_star_auto: f64,   // auto-computed r* for reference
    r_static: f64,      // static r from constructor
    rtt_ticks: u32,
    srtt_secs: f64,
    rttvar_secs: f64,
    t_cut: f64,
    capacity: u32,
    channel_good: bool,
    tick: u32,
    source_done: bool,
    finished: bool,
    num_source: u32,

    // Triangle mode + adaptive r
    mode: math::TriangleMode,
    estimator: math::LossEstimator,
    fec_controller: math::FecRateController,
    w: u32,

    // Real RLC codec
    encoder: math::RlcEncoder,
    decoder: math::RlcDecoder,

    // Symbol tracking (for ARQ: which seqs are lost and un-recovered)
    symbols: Vec<Symbol>,
    next_seq: u32,

    // Counters
    total_src: u32, total_fec: u32, total_arq: u32, total_lost: u32,
    steady_src: u32, steady_fec: u32, steady_arq: u32,
    cum_sent: u32, cum_arrived: u32, cum_decoded: u32,
    fec_debt: f64,
    lost_pending: u32,

    // Per-tick output
    last_src: u32, last_fec: u32, last_arq: u32, last_lost: u32,

    // RNG
    rng_state: u64,
}

struct Symbol {
    tick: u32,
    seq: u64,
    lost: bool,
    recovered: bool,
    arq_tick: i32,
}

impl Simulation {
    fn rng(&mut self) -> f64 {
        let mut x = self.rng_state;
        x ^= x << 13; x ^= x >> 7; x ^= x << 17;
        self.rng_state = x;
        (x as f64) / (u64::MAX as f64)
    }
    fn channel_step(&mut self) {
        if self.channel_good {
            if self.rng() < self.p { self.channel_good = false; }
        } else {
            if self.rng() < self.q { self.channel_good = true; }
        }
    }
    fn is_lost(&self) -> bool { !self.channel_good }
}

#[wasm_bindgen]
impl Simulation {
    #[wasm_bindgen(constructor)]
    pub fn new(eps: f64, q: f64, rtt_ms: u32, w: u32,
               r: Option<f64>, delta: Option<f64>, rho: Option<f64>) -> Self {
        let p = eps * q / (1.0 - eps);
        let sigma2 = math::burst_variance_factor(p, q);
        let r_star_auto = math::compute_r_star(eps, sigma2, w as f64);

        // Determine triangle mode from which param is None
        let mode = match (r, delta, rho) {
            (None, Some(d), Some(p)) => math::TriangleMode::ComputeR { delta: d, rho: p },
            (Some(r), None, Some(p)) => math::TriangleMode::ComputeDelta { r, rho: p },
            (Some(r), Some(d), None) => math::TriangleMode::ComputeRho { r, delta: d },
            _ => math::TriangleMode::ComputeR { delta: 0.001, rho: 1.0 }, // fallback
        };

        // Initial r from triangle (using static channel params)
        let initial_r = match &mode {
            math::TriangleMode::ComputeR { delta, rho } =>
                math::solve_r_from_delta_rho(eps, q, w as f64, sigma2, *delta, *rho).r,
            math::TriangleMode::ComputeDelta { r, .. } => *r,
            math::TriangleMode::ComputeRho { r, .. } => *r,
        };

        let t_cut = match &mode {
            math::TriangleMode::ComputeR { rho, .. } | math::TriangleMode::ComputeDelta { rho, .. } =>
                math::find_t_cut(eps, q, initial_r, w as f64, sigma2, *rho),
            math::TriangleMode::ComputeRho { .. } => f64::INFINITY, // computed dynamically
        };

        let symbol_size = 8;
        Self {
            eps, q, p, sigma2,
            r_star: initial_r, r_star_auto, r_static: initial_r,
            rtt_ticks: rtt_ms.max(2),
            srtt_secs: rtt_ms as f64 / 1000.0,
            rttvar_secs: rtt_ms as f64 / 4000.0,
            t_cut, capacity: 4,
            channel_good: true, tick: 0,
            source_done: false, finished: false, num_source: 200,
            mode,
            estimator: math::LossEstimator::new(),
            fec_controller: math::FecRateController::new(0.5, 0.004),
            w,
            encoder: math::RlcEncoder::new(symbol_size),
            decoder: math::RlcDecoder::new(symbol_size),
            symbols: Vec::new(), next_seq: 0,
            total_src: 0, total_fec: 0, total_arq: 0, total_lost: 0,
            steady_src: 0, steady_fec: 0, steady_arq: 0,
            cum_sent: 0, cum_arrived: 0, cum_decoded: 0,
            fec_debt: 0.0, lost_pending: 0,
            last_src: 0, last_fec: 0, last_arq: 0, last_lost: 0,
            rng_state: 0xdeadbeef12345678,
        }
    }

    pub fn step(&mut self) {
        if self.finished { return; }
        self.channel_step();

        let mut src_n: u32 = 0;
        let mut fec_n: u32 = 0;
        let mut arq_n: u32 = 0;
        let mut lost_n: u32 = 0;
        let mut tick_sent: u32 = 0;
        let mut tick_survived: u32 = 0;

        // Compute r from triangle mode + live estimator (paper Section 1.4, 8.6)
        let w = self.encoder.window_size().max(1);
        let r = self.fec_controller.compute_repair_rate(&self.estimator, &self.mode, w);
        self.r_star = r;
        let c = self.capacity;

        for slot in 0..c {
            let force_source = slot == 0 && !self.source_done;
            self.fec_debt = (self.fec_debt + r).min(3.0);
            let has_unrecovered = self.symbols.iter().any(|s| s.lost && !s.recovered);

            if !force_source && self.fec_debt >= 1.0 && (!self.source_done || has_unrecovered) {
                // --- Correction slot ---
                self.fec_debt -= 1.0;
                let lost = self.is_lost();
                if lost { lost_n += 1; }
                tick_sent += 1;

                // P_lost weighted selection for ARQ vs FEC
                let taper = math::TaperFunction::new(r, self.q);
                let mut total_density: f64 = 0.0;
                for sym in &self.symbols {
                    if sym.recovered { continue; }
                    total_density += taper.density((self.tick.saturating_sub(sym.tick)) as f64);
                }
                let mut pick_idx: Option<usize> = None;
                if total_density > 0.0 {
                    let mut r2 = self.rng() * total_density;
                    for (i, sym) in self.symbols.iter().enumerate() {
                        if sym.recovered { continue; }
                        r2 -= taper.density((self.tick.saturating_sub(sym.tick)) as f64);
                        if r2 <= 0.0 { pick_idx = Some(i); break; }
                    }
                }

                let mut did_arq = false;
                if let Some(idx) = pick_idx {
                    let age_ticks = self.tick.saturating_sub(self.symbols[idx].tick);
                    let age_secs = age_ticks as f64 * 0.001;
                    let sym_lost = self.symbols[idx].lost;
                    let sym_arq_tick = self.symbols[idx].arq_tick;
                    let sym_seq = self.symbols[idx].seq;
                    let pl = math::p_lost(age_secs, self.eps, self.srtt_secs, self.rttvar_secs);
                    let rng_val = self.rng();
                    if rng_val < pl && sym_lost
                        && age_ticks >= self.rtt_ticks
                        && (self.tick as i32 - sym_arq_tick) >= self.rtt_ticks as i32
                    {
                        // ARQ: retransmit source symbol
                        self.symbols[idx].arq_tick = self.tick as i32;
                        self.total_arq += 1; arq_n += 1; self.cum_sent += 1;
                        if !lost {
                            tick_survived += 1;
                            self.cum_arrived += 1;
                            // Feed retransmitted source to decoder
                            if let Some(src_data) = self.encoder.get_source(sym_seq) {
                                let recovered = self.decoder.feed_source(sym_seq, src_data);
                                self.cum_decoded += recovered.len() as u32;
                            }
                            self.symbols[idx].recovered = true;
                            self.lost_pending -= 1;
                        }
                        did_arq = true;
                    }
                }
                if !did_arq {
                    // FEC: generate real repair symbol from encoder
                    let repair = self.encoder.generate_repair();
                    self.total_fec += 1; fec_n += 1; self.cum_sent += 1;
                    if !lost {
                        tick_survived += 1;
                        self.cum_arrived += 1;
                        // Feed repair to real RLC decoder
                        let recovered = self.decoder.feed_repair(
                            repair.window_start, repair.window_count,
                            repair.repair_index, &repair.coded_data,
                        );
                        // Mark recovered symbols
                        for rseq in &recovered {
                            for sym in &mut self.symbols {
                                if sym.seq == *rseq && sym.lost && !sym.recovered {
                                    sym.recovered = true;
                                    self.lost_pending -= 1;
                                }
                            }
                        }
                        self.cum_decoded += recovered.len() as u32;
                    }
                }
            } else if !self.source_done {
                // --- Source slot ---
                let dummy_data = vec![self.next_seq as u8; 8];
                let seq = self.encoder.add_source(&dummy_data);
                let lost = self.is_lost();
                if lost { lost_n += 1; }
                tick_sent += 1;
                self.symbols.push(Symbol {
                    tick: self.tick, seq, lost, recovered: false, arq_tick: -1000,
                });
                self.total_src += 1; src_n += 1; self.cum_sent += 1;
                if lost {
                    self.total_lost += 1; self.lost_pending += 1;
                } else {
                    tick_survived += 1;
                    self.cum_arrived += 1;
                    // Feed source directly to decoder
                    let recovered = self.decoder.feed_source(seq, &dummy_data);
                    self.cum_decoded += recovered.len() as u32;
                }
                self.next_seq += 1;
                if self.next_seq >= self.num_source { self.source_done = true; }
            }
        }

        // Update estimator with this tick's observations (adaptive r)
        self.estimator.record_batch(tick_sent, tick_survived, self.tick as u64);

        // ACK non-lost symbols after RTT
        for sym in &mut self.symbols {
            if !sym.lost && !sym.recovered && self.tick.saturating_sub(sym.tick) >= self.rtt_ticks {
                sym.recovered = true;
            }
        }

        // T_cut eviction (rho < 1.0)
        if self.source_done && self.t_cut.is_finite() {
            let t_cut_ticks = self.t_cut as u32;
            for sym in &mut self.symbols {
                if sym.lost && !sym.recovered && self.tick.saturating_sub(sym.tick) > t_cut_ticks {
                    sym.recovered = true;
                    self.lost_pending -= 1;
                }
            }
        }

        // Advance encoder window to prevent unbounded growth
        if self.encoder.window_size() > 100 {
            let oldest = self.encoder.next_seq().saturating_sub(80);
            self.encoder.advance(oldest);
        }

        // Finish condition
        if self.source_done && self.cum_decoded >= self.num_source as u32 { self.finished = true; }
        if self.source_done && self.lost_pending == 0
            && !self.symbols.iter().any(|s| s.lost && !s.recovered) { self.finished = true; }
        if self.tick > 5000 { self.finished = true; }

        // Steady-state tracking
        if !self.source_done {
            self.steady_src += src_n; self.steady_fec += fec_n; self.steady_arq += arq_n;
        }

        // Prune resolved symbols (keep lost unrecovered for ARQ)
        let tick = self.tick;
        self.symbols.retain(|s| (s.lost && !s.recovered) || tick.saturating_sub(s.tick) < 10);

        self.last_src = src_n; self.last_fec = fec_n; self.last_arq = arq_n; self.last_lost = lost_n;
        self.tick += 1;
    }

    // Accessors
    pub fn is_finished(&self) -> bool { self.finished }
    pub fn channel_is_good(&self) -> bool { self.channel_good }
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
    pub fn get_r_star(&self) -> f64 { self.r_star }
    pub fn get_r_star_auto(&self) -> f64 { self.r_star_auto }
    pub fn get_sigma2(&self) -> f64 { self.sigma2 }
    pub fn get_retx_buf_size(&self) -> u32 { self.lost_pending }
    pub fn get_num_source(&self) -> u32 { self.num_source }
    pub fn get_estimated_loss(&self) -> f64 { self.estimator.loss_rate() }
    pub fn get_overhead(&self) -> f64 {
        if self.steady_src > 0 { (self.steady_fec + self.steady_arq) as f64 / self.steady_src as f64 * 100.0 }
        else { 0.0 }
    }
    pub fn get_recovery(&self) -> f64 {
        if self.total_src > 0 { self.cum_decoded as f64 / self.total_src as f64 * 100.0 }
        else { 100.0 }
    }
}
