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
    r_star: f64,
    r_star_auto: f64,
    amplitude: f64,
    decay: f64,
    rtt_ticks: u32,
    srtt_secs: f64,
    rttvar_secs: f64,
    t_cut: f64,
    capacity: u32,

    // GE channel state
    channel_good: bool,

    // Simulation state
    tick: u32,
    symbols: Vec<Symbol>,
    next_seq: u32,
    source_done: bool,
    finished: bool,
    num_source: u32,

    // Counters
    total_src: u32,
    total_fec: u32,
    total_arq: u32,
    total_lost: u32,
    steady_src: u32,
    steady_fec: u32,
    steady_arq: u32,
    cum_sent: u32,
    cum_arrived: u32,
    cum_decoded: u32,
    fec_debt: f64,
    fec_pool: u32,
    lost_pending: u32,

    // Per-tick output (read after each step)
    last_src: u32,
    last_fec: u32,
    last_arq: u32,
    last_lost: u32,

    // RNG state (simple xorshift64)
    rng_state: u64,
}

struct Symbol {
    tick: u32,
    lost: bool,
    recovered: bool,
    arq_tick: i32,
}

impl Simulation {
    fn rng(&mut self) -> f64 {
        // xorshift64
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
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
    pub fn new(eps: f64, q: f64, rtt_ms: u32, w: u32, r: f64, rho: f64) -> Self {
        let p = eps * q / (1.0 - eps);
        let sigma2 = math::burst_variance_factor(p, q);
        let r_star_auto = math::compute_r_star(eps, sigma2, w as f64);
        let t_cut = math::find_t_cut(eps, q, r, w as f64, sigma2, rho);
        Self {
            eps, q, p, sigma2,
            r_star: r,
            r_star_auto,
            amplitude: r * q.clamp(0.01, 1.0),
            decay: 1.0 - q.clamp(0.01, 1.0),
            rtt_ticks: rtt_ms.max(2),
            srtt_secs: rtt_ms as f64 / 1000.0,
            rttvar_secs: rtt_ms as f64 / 4000.0,
            t_cut,
            capacity: 4,
            channel_good: true,
            tick: 0,
            symbols: Vec::new(),
            next_seq: 0,
            source_done: false,
            finished: false,
            num_source: 200,
            total_src: 0, total_fec: 0, total_arq: 0, total_lost: 0,
            steady_src: 0, steady_fec: 0, steady_arq: 0,
            cum_sent: 0, cum_arrived: 0, cum_decoded: 0,
            fec_debt: 0.0, fec_pool: 0, lost_pending: 0,
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
        let r = self.r_star;
        let c = self.capacity;

        for slot in 0..c {
            let force_source = slot == 0 && !self.source_done;
            self.fec_debt = (self.fec_debt + r).min(3.0);

            let has_unrecovered = self.symbols.iter().any(|s| s.lost && !s.recovered);

            if !force_source && self.fec_debt >= 1.0 && (!self.source_done || has_unrecovered) {
                self.fec_debt -= 1.0;

                // Weighted random selection by taper density
                let mut total_density: f64 = 0.0;
                for sym in &self.symbols {
                    if sym.recovered { continue; }
                    let age = self.tick.saturating_sub(sym.tick);
                    total_density += math::TaperFunction::new(self.r_star, self.q).density(age as f64);
                }

                let mut pick_idx: Option<usize> = None;
                if total_density > 0.0 {
                    let mut r2 = self.rng() * total_density;
                    let taper = math::TaperFunction::new(self.r_star, self.q);
                    for (i, sym) in self.symbols.iter().enumerate() {
                        if sym.recovered { continue; }
                        let age = self.tick.saturating_sub(sym.tick);
                        r2 -= taper.density(age as f64);
                        if r2 <= 0.0 { pick_idx = Some(i); break; }
                    }
                }

                let lost = self.is_lost();
                if lost { lost_n += 1; }

                let mut did_arq = false;
                if let Some(idx) = pick_idx {
                    let age_ticks = self.tick.saturating_sub(self.symbols[idx].tick);
                    let age_secs = age_ticks as f64 * 0.001;
                    let sym_lost = self.symbols[idx].lost;
                    let sym_arq_tick = self.symbols[idx].arq_tick;
                    let pl = math::p_lost(age_secs, self.eps, self.srtt_secs, self.rttvar_secs);
                    let rng_val = self.rng();
                    if rng_val < pl && sym_lost
                        && age_ticks >= self.rtt_ticks
                        && (self.tick as i32 - sym_arq_tick) >= self.rtt_ticks as i32
                    {
                        self.symbols[idx].arq_tick = self.tick as i32;
                        self.total_arq += 1; arq_n += 1; self.cum_sent += 1;
                        if !lost {
                            self.cum_arrived += 1;
                            self.symbols[idx].recovered = true;
                            self.cum_decoded += 1;
                            self.lost_pending -= 1;
                        }
                        did_arq = true;
                    }
                }
                if !did_arq {
                    self.total_fec += 1; fec_n += 1; self.cum_sent += 1;
                    if !lost {
                        self.cum_arrived += 1;
                        if self.lost_pending > 0 { self.fec_pool += 1; }
                    }
                }
            } else if !self.source_done {
                let lost = self.is_lost();
                if lost { lost_n += 1; }
                self.symbols.push(Symbol {
                    tick: self.tick, lost, recovered: false, arq_tick: -1000,
                });
                self.total_src += 1; src_n += 1; self.cum_sent += 1;
                if lost { self.total_lost += 1; self.lost_pending += 1; }
                else { self.cum_arrived += 1; self.cum_decoded += 1; }
                self.next_seq += 1;
                if self.next_seq >= self.num_source { self.source_done = true; }
            }
        }

        // Incremental FEC decode
        while self.fec_pool > 0 && self.lost_pending > 0 {
            let mut found = false;
            for sym in &mut self.symbols {
                if sym.lost && !sym.recovered {
                    sym.recovered = true;
                    self.cum_decoded += 1;
                    self.lost_pending -= 1;
                    self.fec_pool -= 1;
                    found = true;
                    break;
                }
            }
            if !found { break; }
        }

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

        // Finish condition
        if self.source_done && self.cum_decoded >= self.num_source as u32 {
            self.finished = true;
        }
        if self.source_done && self.lost_pending == 0 && !self.symbols.iter().any(|s| s.lost && !s.recovered) {
            self.finished = true;
        }
        if self.tick > 5000 { self.finished = true; }

        // Steady-state tracking
        if !self.source_done {
            self.steady_src += src_n;
            self.steady_fec += fec_n;
            self.steady_arq += arq_n;
        }

        // Prune resolved symbols
        let tick = self.tick;
        self.symbols.retain(|s| (s.lost && !s.recovered) || tick.saturating_sub(s.tick) < 10);

        self.last_src = src_n;
        self.last_fec = fec_n;
        self.last_arq = arq_n;
        self.last_lost = lost_n;
        self.tick += 1;
    }

    // Accessors for JS
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

    pub fn get_overhead(&self) -> f64 {
        if self.steady_src > 0 {
            (self.steady_fec + self.steady_arq) as f64 / self.steady_src as f64 * 100.0
        } else { 0.0 }
    }

    pub fn get_recovery(&self) -> f64 {
        if self.total_src > 0 {
            self.cum_decoded as f64 / self.total_src as f64 * 100.0
        } else { 100.0 }
    }
}
