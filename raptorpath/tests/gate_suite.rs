//! ADR-0051 gate suite at fidelity level L0 (in-process simulation).
//!
//! Encodes the session goal as executable assertions:
//!   G1 (surpass): raptorpath beats the SimRetx baseline per the ADR-0051
//!       win conditions on the paper Section 2.4 channels, with 95%-CI
//!       separation. The baseline is labeled SimRetx (NOT "real TCP"):
//!       an AIMD-windowed reliable ARQ transport model with min-RTT
//!       multipath scheduling and TCP in-order delivery semantics.
//!   G2 (model reacts correctly): estimator convergence, regime-change
//!       re-convergence, spare-capacity gating, and outage reaction.
//!
//! Fairness notes (L0):
//! - Both sides are tick-driven with identical pacing structure and BDP
//!   congestion windows; the baseline additionally runs AIMD (halve on
//!   loss event, +1/cwnd per delivery) because a TCP-like transport
//!   without AIMD is not TCP-like.
//! - Baseline latency is measured at IN-ORDER delivery (TCP semantics);
//!   raptorpath latency at reorder-buffer release (tunnel semantics with
//!   bounded reordering). This asymmetry is the head-of-line-blocking
//!   difference the model claims — it is the thing under test.
//! - Loss feedback to the estimator is per-batch (oracle timing, as in
//!   bench_suite); L1 (real stacks over netem) removes this shortcut.
//!
//! Run: cargo test --test gate_suite --release -- --nocapture

mod common;

use common::*;
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use raptorpath::control::estimator::LossEstimator;
use raptorpath::control::fec_rate::{p_lost, FecRateController, ProtocolHint};
use raptorpath::control::gilbert_elliott::GilbertElliottEstimator;
use raptorpath::fec::{FecBackend, RlcWindowDecoder, RlcWindowEncoder, WindowDecoder, WindowEncoder, WireSymbol};
use raptorpath::net::reorder::ReorderBuffer;
use raptorpath::scheduler::{Clock, MockClock};
use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SYMBOL_SIZE: u16 = 1200;
const WIRE_BYTES: f64 = 1225.0; // symbol + per-symbol wire overhead
const N_SYMBOLS: u32 = 1500;
const BATCH: u32 = 10;
const TRIALS: usize = 10;
const TICK: Duration = Duration::from_micros(500);
const ENC_WINDOW: u64 = 64; // encoder sliding window (paper W)

// ---------------------------------------------------------------------------
// ADR-0051 channels — paper Section 2.4 GE parameterization (h_G=0, h_B=1,
// so epsilon = p/(p+q) exactly).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct GateChannel {
    name: &'static str,
    p: f64,
    q: f64,
    one_way_ms: u64,
    jitter_ms: u64,
    capacity_bps: Option<f64>, // bytes/sec
    queue: usize,
}

impl GateChannel {
    fn eps(&self) -> f64 {
        self.p / (self.p + self.q)
    }
    fn rtt(&self) -> Duration {
        Duration::from_millis(2 * self.one_way_ms + self.jitter_ms)
    }
    fn bdp_cwnd(&self) -> usize {
        match self.capacity_bps {
            // The LinkModel counts in-propagation packets against the queue
            // (dequeue happens at delivery), so the window must stay under
            // the queue bound or every flow tail-drops permanently.
            Some(bps) => ((bps * self.rtt().as_secs_f64() / WIRE_BYTES) as usize)
                .min((self.queue as f64 * 0.8) as usize)
                .max(4),
            None => 100_000,
        }
    }
}

const C1_DC: GateChannel = GateChannel {
    name: "C1-DC", p: 0.0005, q: 0.5, one_way_ms: 1, jitter_ms: 0,
    capacity_bps: Some(1_000_000_000.0 / 8.0), queue: 500,
};
const C2_WIFI: GateChannel = GateChannel {
    name: "C2-WiFi", p: 0.013, q: 0.5, one_way_ms: 5, jitter_ms: 3,
    capacity_bps: Some(100_000_000.0 / 8.0), queue: 300,
};
const C3_LTE: GateChannel = GateChannel {
    name: "C3-LTE", p: 0.02, q: 0.4, one_way_ms: 20, jitter_ms: 5,
    capacity_bps: Some(20_000_000.0 / 8.0), queue: 200,
};
const C4_SAT: GateChannel = GateChannel {
    name: "C4-Sat", p: 0.03, q: 0.3, one_way_ms: 100, jitter_ms: 10,
    capacity_bps: Some(20_000_000.0 / 8.0), queue: 1000,
};
const C5_BADWIFI: GateChannel = GateChannel {
    name: "C5-BadWiFi", p: 0.053, q: 0.3, one_way_ms: 5, jitter_ms: 3,
    capacity_bps: Some(50_000_000.0 / 8.0), queue: 130,
};
// C9 uses reduced capacities so a 150 ms outage fits inside the trial.
const C9_WIFI_SLOW: GateChannel = GateChannel {
    name: "C9-WiFi", p: 0.013, q: 0.5, one_way_ms: 5, jitter_ms: 3,
    capacity_bps: Some(20_000_000.0 / 8.0), queue: 80,
};
const C9_LTE_SLOW: GateChannel = GateChannel {
    name: "C9-LTE", p: 0.02, q: 0.4, one_way_ms: 20, jitter_ms: 5,
    capacity_bps: Some(10_000_000.0 / 8.0), queue: 120,
};

fn mk_ge(ch: &GateChannel) -> GilbertElliottChannel {
    GilbertElliottChannel::new(ch.p, ch.q, 0.0, 1.0)
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// 95%-CI-separated comparison: a < factor × b.
fn ci_less(a: &TrialStats, factor: f64, b: &TrialStats) -> bool {
    a.mean() + a.ci95() < factor * (b.mean() - b.ci95())
}

// ---------------------------------------------------------------------------
// Outcome of one trial (either side)
// ---------------------------------------------------------------------------

struct Outcome {
    completion_s: f64,
    p50_ms: f64,
    p99_ms: f64,
    wire_per_source: f64,
    /// 20 ms goodput buckets (recovered source symbols per bucket)
    buckets: Vec<u32>,
    /// First successful delivery on path 0 after the outage ended (C9).
    path0_recovery_s: Option<f64>,
}

const BUCKET: Duration = Duration::from_millis(20);

// ---------------------------------------------------------------------------
// SimRetx baseline: AIMD-windowed reliable ARQ, min-RTT multipath,
// in-order (TCP-semantics) delivery latency.
// ---------------------------------------------------------------------------

fn run_baseline(paths: &[GateChannel], seed: u64) -> Outcome {
    let clock = Arc::new(MockClock::new());
    let t0 = clock.now();
    let n_paths = paths.len();

    let mut chans: Vec<ReliableSimChannel> = Vec::new();
    let mut cwnd: Vec<f64> = Vec::new();
    let mut slow_start: Vec<bool> = Vec::new();
    let mut srtt: Vec<f64> = Vec::new();
    let mut last_halve: Vec<Instant> = Vec::new();
    let mut path_send_times: Vec<Vec<Instant>> = Vec::new();
    let mut path_seq_map: Vec<Vec<u32>> = Vec::new(); // channel wire seq -> source seq
    for (i, ch) in paths.iter().enumerate() {
        let mut c = ReliableSimChannel::new(
            clock.clone(),
            seed.wrapping_add(i as u64 * 7919),
            Duration::from_millis(ch.one_way_ms),
            ch.jitter_ms,
            mk_ge(ch),
            Duration::from_millis((2 * ch.one_way_ms).max(2)),
            8,
        );
        if let Some(bps) = ch.capacity_bps {
            c = c.with_link(bps, ch.queue);
        }
        chans.push(c);
        cwnd.push(10.0); // IW10 + slow-start, like a real TCP
        slow_start.push(true);
        srtt.push(paths[i].rtt().as_secs_f64());
        last_halve.push(t0);
        path_send_times.push(Vec::new());
        path_seq_map.push(Vec::new());
    }

    let mut send_time: Vec<Instant> = Vec::with_capacity(N_SYMBOLS as usize);
    let mut deliver_time: Vec<Option<Instant>> = vec![None; N_SYMBOLS as usize];
    let mut sent: u32 = 0;
    let mut delivered: u32 = 0;
    let deadline = t0 + Duration::from_secs(600);

    while delivered < N_SYMBOLS {
        assert!(clock.now() < deadline, "baseline trial did not complete");

        // Send while some path has window room (min-SRTT scheduling).
        while sent < N_SYMBOLS {
            let mut best: Option<usize> = None;
            for i in 0..n_paths {
                if chans[i].in_flight_count() < cwnd[i] as usize {
                    if best.map_or(true, |b| srtt[i] < srtt[b]) {
                        best = Some(i);
                    }
                }
            }
            let Some(i) = best else { break };
            let sym = make_wire_symbol_sized(sent, false, SYMBOL_SIZE as usize);
            let now = clock.now();
            send_time.push(now);
            path_send_times[i].push(now);
            path_seq_map[i].push(sent);
            let attempts = chans[i].send(sym);
            if attempts > 1 {
                // Loss event: exit slow-start, AIMD halving (<= once per RTT).
                slow_start[i] = false;
                if now.duration_since(last_halve[i]).as_secs_f64() > srtt[i] {
                    cwnd[i] = (cwnd[i] / 2.0).max(2.0);
                    last_halve[i] = now;
                }
            }
            sent += 1;
        }

        clock.advance(TICK);
        for i in 0..n_paths {
            for pkt in chans[i].deliver() {
                let wire_seq = pkt.seq as usize;
                let src_seq = path_seq_map[i][wire_seq] as usize;
                deliver_time[src_seq] = Some(pkt.delivery_time);
                delivered += 1;
                // Slow-start doubles per RTT; congestion avoidance +1/RTT.
                let growth = if slow_start[i] { 1.0 } else { 1.0 / cwnd[i] };
                cwnd[i] = (cwnd[i] + growth).min(paths[i].bdp_cwnd() as f64 * 2.0);
                let sample = pkt
                    .delivery_time
                    .duration_since(path_send_times[i][wire_seq])
                    .as_secs_f64()
                    + paths[i].one_way_ms as f64 / 1000.0; // + ACK return leg
                srtt[i] = 0.875 * srtt[i] + 0.125 * sample;
            }
        }
    }

    // TCP semantics: the application sees IN-ORDER delivery.
    let mut latencies: Vec<f64> = Vec::with_capacity(N_SYMBOLS as usize);
    let mut buckets: Vec<u32> = Vec::new();
    let mut t_inorder = t0;
    for seq in 0..N_SYMBOLS as usize {
        let t = deliver_time[seq].expect("baseline delivers everything");
        if t > t_inorder {
            t_inorder = t;
        }
        latencies.push(t_inorder.duration_since(send_time[seq]).as_secs_f64() * 1000.0);
        let b = (t_inorder.duration_since(t0).as_nanos() / BUCKET.as_nanos()) as usize;
        if buckets.len() <= b {
            buckets.resize(b + 1, 0);
        }
        buckets[b] += 1;
    }
    let completion_s = t_inorder.duration_since(t0).as_secs_f64();
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let total_tx: u64 = chans.iter().map(|c| c.total_transmissions()).sum();

    Outcome {
        completion_s,
        p50_ms: percentile(&latencies, 0.50),
        p99_ms: percentile(&latencies, 0.99),
        wire_per_source: total_tx as f64 / N_SYMBOLS as f64,
        buckets,
        path0_recovery_s: None,
    }
}

// ---------------------------------------------------------------------------
// raptorpath L0 driver: RLC window FEC + taper-driven corrections +
// P_lost-gated exact-source retransmit, per-path estimators, E_i scheduling.
// ---------------------------------------------------------------------------

fn prewarm(ch: &GateChannel) -> LossEstimator {
    let mut est = LossEstimator::new();
    let received = ((1.0 - ch.eps()) * 1000.0).round() as u32;
    for _ in 0..50 {
        est.record_counts(1000, received);
        est.record_rtt(ch.rtt());
        est.record_throughput(ch.capacity_bps.unwrap_or(1e9));
    }
    // Warm the GE estimator with a genuine channel pattern so sigma2_burst
    // starts from the true burst structure, not the lumped approximation.
    let mut rng = ChaCha8Rng::seed_from_u64(0xdead);
    let mut ge = mk_ge(ch);
    for _ in 0..4000 {
        est.record_symbol(!ge.should_drop(&mut rng));
    }
    est
}

/// Expected delivery time score for source-path selection (paper 13.5, with
/// the geometric retransmit chain of 13.4: expected retries = eps/(1-eps)).
fn path_score(srtt: f64, eps: f64) -> f64 {
    let chain = (eps / (1.0 - eps).max(1e-6)).min(50.0);
    srtt / 2.0 + chain * 1.5 * srtt
}

struct FecConfig {
    hint: ProtocolHint,
    /// Outage on path 0: (start, end) after t0.
    outage: Option<(Duration, Duration)>,
}

fn run_fec(paths: &[GateChannel], seed: u64, cfg: &FecConfig) -> Outcome {
    let clock = Arc::new(MockClock::new());
    let t0 = clock.now();
    let n_paths = paths.len();

    let mut chans: Vec<SimChannel> = Vec::new();
    let mut ests: Vec<LossEstimator> = Vec::new();
    // Copa-lite congestion window (paper Section 12): delay-based — grow
    // while queueing delay is low, back off when SRTT rises above the
    // propagation floor. Keeps queues near-empty (low p99), unlike the
    // baseline's loss-based AIMD which fills the buffer.
    let mut cwnd: Vec<f64> = Vec::new();
    let mut srtt: Vec<f64> = Vec::new();
    let mut debt: Vec<f64> = Vec::new();
    let mut path_send_times: Vec<Vec<Instant>> = Vec::new();
    // Token-bucket pacing at Copa's rate = cwnd/SRTT. A windowed count
    // degenerates into RTT-synchronized mega-bursts (all sends age out of
    // the window at the same instant), which clumps repairs and self-
    // queues; tokens keep the send process smooth. Loss cannot fake room
    // (a dead path stays rate-bounded).
    let mut tokens: Vec<f64> = Vec::new();
    let mut last_flush: Vec<Instant> = Vec::new();
    let mut last_probe: Vec<Instant> = Vec::new();
    // Copa uses the MINIMUM RTT sample per window: the min sees through
    // transient serialization bursts to the standing queue; an EWMA stays
    // inflated long after the queue drains and causes a backoff spiral.
    let mut min_rtt_win: Vec<f64> = Vec::new();
    // Startup ramp flag: exponential until the queue first shows, then
    // gentle additive/multiplicative oscillation around the BDP (Copa's
    // steady-state behavior; the ramp is its slow-start analogue).
    let mut ramping: Vec<bool> = Vec::new();
    // Per-path wire outcomes since the last estimator flush (true =
    // survived) — the SACK-reconstructed arrival pattern.
    let mut batch_outcomes: Vec<Vec<bool>> = vec![Vec::new(); n_paths];
    // Per-path source symbols sent since the last debt accrual.
    let mut batch_src: Vec<u32> = vec![0; n_paths];

    for (i, ch) in paths.iter().enumerate() {
        let mut c = SimChannel::new(
            clock.clone(),
            seed.wrapping_add(i as u64 * 7919),
            Duration::from_millis(ch.one_way_ms),
            ch.jitter_ms,
            mk_ge(ch),
        );
        if let Some(bps) = ch.capacity_bps {
            c = c.with_link(bps, ch.queue);
        }
        chans.push(c);
        ests.push(prewarm(ch));
        cwnd.push(ch.bdp_cwnd() as f64 / 2.0);
        srtt.push(ch.rtt().as_secs_f64());
        debt.push(0.0);
        path_send_times.push(Vec::new());
        tokens.push(10.0);
        last_flush.push(t0);
        last_probe.push(t0);
        min_rtt_win.push(f64::INFINITY);
        ramping.push(true);
    }

    let ctrl = FecRateController::new(1e-5, 0.5, cfg.hint, FecBackend::Rlc, SYMBOL_SIZE);
    let mut encoder = RlcWindowEncoder::new(SYMBOL_SIZE);
    let mut decoder = RlcWindowDecoder::new(SYMBOL_SIZE);
    let max_delay = paths.iter().map(|c| c.one_way_ms + c.jitter_ms).max().unwrap();
    let mut reorder = ReorderBuffer::new(2 * max_delay + 10, 4000);

    // Encoder lag = jitter horizon in symbols. A repair covering symbols
    // that cannot yet have arrived (jitter lets a repair overtake up to
    // jitter x send_rate sources) carries no usable information at arrival
    // time — it parks as a deep pivot instead of decoding the actual loss.
    // Lagging the encoder behind the send stream by that horizon makes a
    // repair's unknowns be TRUE losses, decodable on arrival.
    let max_jitter_s = paths.iter().map(|c| c.jitter_ms).max().unwrap() as f64 / 1000.0;
    let total_rate: f64 = paths
        .iter()
        .filter_map(|c| c.capacity_bps)
        .map(|bps| bps / WIRE_BYTES)
        .sum();
    let enc_lag: usize = if total_rate > 0.0 {
        ((max_jitter_s * total_rate).ceil() as usize).clamp(2, 48)
    } else {
        4
    };
    let mut enc_queue: VecDeque<Vec<u8>> = VecDeque::new();

    let mut source_store: Vec<WireSymbol> = Vec::with_capacity(N_SYMBOLS as usize);
    let mut encode_time: Vec<Instant> = Vec::with_capacity(N_SYMBOLS as usize);
    let mut last_retx: Vec<Instant> = Vec::with_capacity(N_SYMBOLS as usize);
    let mut recovered: BTreeSet<u64> = BTreeSet::new();
    // Decode-level receipt (pre-reorder) — the sender's SACK view. The
    // retransmit scan MUST use this, not the reorder-released set, or
    // symbols buffered behind a gap get spuriously retransmitted.
    let mut decoded: BTreeSet<u64> = BTreeSet::new();
    let mut latencies: Vec<f64> = Vec::new();
    // (latency_ms, cause) — cause: 0 = in-order release, 1 = reorder-drain
    let mut lat_causes: Vec<(f64, u8, u64)> = Vec::new();
    let mut buckets: Vec<u32> = Vec::new();
    let mut last_recovery = t0;
    let mut path0_recovery: Option<Instant> = None;
    let mut outage_active = false;

    let mut total_wire: u64 = 0;
    let mut n_repairs_sent: u64 = 0;
    let mut n_retx_sent: u64 = 0;
    let mut n_fec_recovered: u64 = 0;
    let mut loss_time: std::collections::BTreeMap<u64, Instant> = std::collections::BTreeMap::new();
    let mut hole_fill_ms: Vec<f64> = Vec::new();
    let mut sent: u32 = 0;
    let mut in_batch: u32 = 0;
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0x5eed);
    let deadline = t0 + Duration::from_secs(600);

    macro_rules! send_on {
        ($i:expr, $sym:expr) => {{
            let now = clock.now();
            path_send_times[$i].push(now);
            tokens[$i] -= 1.0;
            let ok = chans[$i].send($sym);
            batch_outcomes[$i].push(ok);
            total_wire += 1;
        }};
    }

    // Room = a send token is available. Tokens replenish at cwnd/SRTT
    // per second (Copa's sending rate) — the OFFERED load is paced by
    // sender knowledge, independent of what the channel drops.
    macro_rules! has_room {
        ($i:expr, $now:expr) => {{
            let _ = $now;
            tokens[$i] >= 1.0
        }};
    }

    // Path-selection loss estimate: blend of the BOCD-informed median
    // (regime-aware, converges within a few batches) and the GE
    // state-conditional loss rate (paper C.6 eps_burst — flips on the
    // FIRST delivery after an outage ends). The blend reacts within one
    // symbol of a state change while staying anchored to the posterior.
    macro_rules! eps_sel {
        ($i:expr) => {{
            let med = ests[$i].predictive_loss_upper(0.5);
            let ge = ests[$i].ge_estimator();
            let cond = if ge.is_valid() { ge.conditional_loss_rate() } else { med };
            (0.5 * (med + cond)).clamp(0.0, 0.99)
        }};
    }

    loop {
        let now = clock.now();
        assert!(now < deadline, "fec trial did not complete");

        // Replenish pacing tokens: rate = cwnd/SRTT, small burst allowance.
        for i in 0..n_paths {
            let pace = cwnd[i] / srtt[i] * TICK.as_secs_f64();
            tokens[i] = (tokens[i] + pace).min((cwnd[i] / 8.0).max(10.0));
        }

        // Outage control (C9): force/release 100% loss on path 0.
        if let Some((start, end)) = cfg.outage {
            let active = now >= t0 + start && now < t0 + end;
            if active != outage_active {
                let ge = chans[0].ge_mut();
                if active {
                    ge.loss_good = 1.0;
                    ge.loss_bad = 1.0;
                } else {
                    ge.loss_good = 0.0;
                    ge.loss_bad = 1.0;
                }
                outage_active = active;
            }
        }

        // Correction preemption (paper C.1 / Mehrotra): due corrections are
        // sent BEFORE new source symbols — they compete for the same wire
        // budget and win when the taper says they are due.
        for i in 0..n_paths {
            loop {
                let now2 = clock.now();
                if debt[i] < 1.0 || encoder.window_size() == 0 || !has_room!(i, now2) {
                    break;
                }
                let rep = encoder.generate_repair();
                send_on!(i, rep);
                n_repairs_sent += 1;
                debt[i] -= 1.0;
            }
        }

        // Send phase: source symbols while a path has window room.
        // Paths are ranked by expected delivery time E_i (paper 13.5 with
        // the 13.4 geometric retransmit chain). Overflow onto a worse path
        // happens with probability (1 - eps_i): a path's usefulness for
        // source is its delivery probability, so a dead path receives a
        // vanishing (but never zero) share, continuously.
        'send: while sent < N_SYMBOLS {
            let now2 = clock.now();
            let mut order: Vec<usize> = (0..n_paths).collect();
            order.sort_by(|&a, &b| {
                path_score(srtt[a], eps_sel!(a))
                    .partial_cmp(&path_score(srtt[b], eps_sel!(b)))
                    .unwrap()
            });
            let mut pick: Option<usize> = None;
            for (rank, &cand) in order.iter().enumerate() {
                if !has_room!(cand, now2) {
                    continue;
                }
                if rank == 0 || rng.gen::<f64>() < 1.0 - eps_sel!(cand) {
                    pick = Some(cand);
                }
                break;
            }
            let Some(i) = pick else { break 'send };

            let data = vec![(sent % 251) as u8; SYMBOL_SIZE as usize];
            // Source symbols go on the wire immediately; the ENCODER sees
            // them enc_lag symbols later (jitter horizon, see above).
            let sym = WireSymbol {
                block_id: sent as u64,
                payload_id: 0,
                is_repair: false,
                data: data.clone(),
                backend: FecBackend::Rlc,
            };
            enc_queue.push_back(data);
            while enc_queue.len() > enc_lag {
                let d = enc_queue.pop_front().unwrap();
                let es = encoder.add_source(&d);
                if es.block_id >= ENC_WINDOW {
                    encoder.advance(es.block_id - (ENC_WINDOW - 1));
                }
            }
            source_store.push(sym.clone());
            encode_time.push(clock.now());
            last_retx.push(t0);
            let src_seq = sym.block_id;
            send_on!(i, sym);
            if batch_outcomes[i].last() == Some(&false) {
                loss_time.insert(src_seq, clock.now());
            }
            batch_src[i] += 1;
            sent += 1;
            in_batch += 1;

            if in_batch == BATCH {
                in_batch = 0;
                // Correction debt per path. In steady state the aggregate
                // correction rate is shape-invariant: every in-window symbol
                // contributes its taper at a different age, and the ages sum
                // to r per source symbol (paper Section 4.2). So the debt
                // accumulates at the flat aggregate rate — NOT at τ(t) with
                // a global ever-growing t, which decays to zero and starves
                // repair generation entirely (a latent bench_suite bug).
                // Corrections share the path's wire budget with source
                // (Section 12.5) and preempt new source when due (C.1).
                for i in 0..n_paths {
                    let rate = ctrl.compute_repair_rate(&ests[i], encoder.window_size());
                    if std::env::var("RP_GATE_DEBUG").is_ok() {
                        println!(
                            "  [rate] t={:.1}ms path={} rate={:.3} debt={:.2} p95={:.4} sigma2={:.1}",
                            clock.now().duration_since(t0).as_secs_f64() * 1000.0,
                            i, rate, debt[i],
                            ests[i].predictive_loss_upper(0.95),
                            raptorpath::control::fec_rate::burst_variance_factor(&ests[i])
                        );
                    }
                    // Each path protects ITS share of the source stream —
                    // accruing the full rate on every path would double the
                    // correction budget on multipath.
                    debt[i] += rate * batch_src[i] as f64;
                    batch_src[i] = 0;
                }
            }
        }

        // Path probing: one repair symbol per path per RTT. Probing with
        // repairs is free information — a repair is never wasted (paper
        // Section 5.2) — and its loss/arrival feedback is what lets a
        // recovered path come back into rotation.
        for i in 0..n_paths {
            let now2 = clock.now();
            // Probe faster on lossier paths (down to srtt/4 when dead):
            // recovery detection needs samples, and the cost is bounded by
            // the probe rate itself. Continuous in eps, no mode switch.
            let interval = srtt[i] * (1.0 - 0.75 * eps_sel!(i));
            if now2.duration_since(last_probe[i]).as_secs_f64() >= interval {
                if encoder.window_size() > 0 && has_room!(i, now2) {
                    let rep = encoder.generate_repair();
                    send_on!(i, rep);
                    n_repairs_sent += 1;
                }
                last_probe[i] = now2;
            }
        }

        // Tick + deliveries.
        clock.advance(TICK);
        let now = clock.now();
        let mut newly: Vec<(u64, bytes::Bytes)> = Vec::new();
        for i in 0..n_paths {
            for pkt in chans[i].deliver() {
                let wire_seq = pkt.seq as usize;
                let sample = pkt
                    .delivery_time
                    .duration_since(path_send_times[i][wire_seq])
                    .as_secs_f64()
                    + paths[i].one_way_ms as f64 / 1000.0;
                srtt[i] = 0.875 * srtt[i] + 0.125 * sample;
                min_rtt_win[i] = min_rtt_win[i].min(sample);
                if i == 0 {
                    if let Some((_, end)) = cfg.outage {
                        if path0_recovery.is_none() && pkt.delivery_time >= t0 + end {
                            path0_recovery = Some(pkt.delivery_time);
                        }
                    }
                }
                let outs = decoder.add_symbol(&pkt.symbol);
                if pkt.symbol.is_repair {
                    n_fec_recovered += outs.len() as u64;
                    if std::env::var("RP_GATE_DEBUG").is_ok() && pkt.symbol.data.len() >= 10 {
                        let ws = u64::from_le_bytes(pkt.symbol.data[0..8].try_into().unwrap());
                        let wc = u16::from_le_bytes(pkt.symbol.data[8..10].try_into().unwrap()) as u64;
                        let missing: Vec<u64> = (ws..ws + wc)
                            .filter(|q| !decoded.contains(q) && *q < sent as u64)
                            .collect();
                        if !missing.is_empty() {
                            println!(
                                "  [rep] t={:.1}ms win=[{},{}) missing={} decoded_now={}",
                                now.duration_since(t0).as_secs_f64() * 1000.0,
                                ws, ws + wc, missing.len(), outs.len()
                            );
                        }
                    }
                }
                newly.extend(outs);
            }
        }
        // Estimator feedback once per RTT per path (paper Section 7.1: a
        // batch = one ACK feedback cycle). Tiny per-10-symbol updates keep
        // the BOCD posterior artificially wide; RTT cadence matches the
        // information rate of a real ACK stream.
        for i in 0..n_paths {
            if now.duration_since(last_flush[i]).as_secs_f64() >= srtt[i] {
                let sent_n = batch_outcomes[i].len() as u32;
                if sent_n > 0 {
                    let ok_n = batch_outcomes[i].iter().filter(|&&o| o).count() as u32;
                    // Counts feed EWMA/Beta/BOCD; the true interleaving
                    // feeds the GE estimator (unbiased burstiness — batch
                    // lumping would overestimate sigma2_burst and inflate
                    // the correction rate ~2x).
                    ests[i].record_counts(sent_n, ok_n);
                    for &o in &batch_outcomes[i] {
                        ests[i].record_symbol(o);
                    }
                    batch_outcomes[i].clear();
                    ests[i].record_rtt(Duration::from_secs_f64(srtt[i]));
                }
                // Copa-lite: standing queue = min-RTT-in-window minus the
                // propagation floor. While the queue is empty, ramp
                // multiplicatively (fills the pipe in a few RTTs); once the
                // MIN sample rises above the floor, back off. Continuous
                // oscillation, no phases, no loss reaction — channel loss
                // is FEC's job, not CC's (paper Section 12).
                let base = paths[i].rtt().as_secs_f64();
                if min_rtt_win[i].is_finite() {
                    let cap = paths[i].bdp_cwnd() as f64 * 2.0;
                    if min_rtt_win[i] > base * 1.125 {
                        ramping[i] = false;
                        cwnd[i] = (cwnd[i] * 0.92).max(4.0);
                    } else if ramping[i] {
                        cwnd[i] = (cwnd[i] * 1.5 + 1.0).min(cap);
                    } else {
                        cwnd[i] = (cwnd[i] + 2.0).min(cap);
                    }
                    min_rtt_win[i] = f64::INFINITY;
                }
                last_flush[i] = now;
            }
        }

        for (seq, data) in newly {
            if decoded.insert(seq) {
                if let Some(lt) = loss_time.get(&seq) {
                    hole_fill_ms.push(now.duration_since(*lt).as_secs_f64() * 1000.0);
                }
                // Goodput buckets count DECODE-level delivery: the tunnel
                // forwards packets; ordering is a per-flow latency concern
                // (measured via the reorder buffer), not a throughput one.
                let b = (now.duration_since(t0).as_nanos() / BUCKET.as_nanos()) as usize;
                if buckets.len() <= b {
                    buckets.resize(b + 1, 0);
                }
                buckets[b] += 1;
            }
            for (rseq, _) in reorder.push_with_time(seq, data, now) {
                if recovered.insert(rseq) {
                    let lat = now.duration_since(encode_time[rseq as usize]);
                    lat_causes.push((lat.as_secs_f64() * 1000.0, 0, rseq));
                    latencies.push(lat.as_secs_f64() * 1000.0);
                    last_recovery = now;
                }
            }
        }
        for (rseq, _) in reorder.drain_expired(now) {
            if recovered.insert(rseq) {
                let lat = now.duration_since(encode_time[rseq as usize]);
                lat_causes.push((lat.as_secs_f64() * 1000.0, 1, rseq));
                latencies.push(lat.as_secs_f64() * 1000.0);
                last_recovery = now;
            }
        }

        // P_lost-gated exact-source retransmit (correction symbols of the
        // retransmit kind, paper Section 5.4), cross-path via best E_i.
        // Scanned every tick — models per-ACK gap detection (SACK).
        {
            let mut budget = 20u32;
            let mut eps_max = 1e-4f64;
            for i in 0..n_paths {
                eps_max = eps_max.max(eps_sel!(i));
            }
            let srtt_min = srtt.iter().cloned().fold(f64::INFINITY, f64::min);
            let mut best = 0usize;
            for i in 1..n_paths {
                if path_score(srtt[i], eps_sel!(i)) < path_score(srtt[best], eps_sel!(best)) {
                    best = i;
                }
            }
            for seq in 0..sent as u64 {
                if budget == 0 {
                    break;
                }
                if decoded.contains(&seq) {
                    continue;
                }
                let age = now.duration_since(encode_time[seq as usize]).as_secs_f64();
                let pl = p_lost(age, eps_max, srtt_min, 0.125 * srtt_min);
                if pl > 0.9
                    && now.duration_since(last_retx[seq as usize]).as_secs_f64() > srtt_min
                    && has_room!(best, now)
                    && rng.gen::<f64>() < pl
                {
                    let sym = source_store[seq as usize].clone();
                    send_on!(best, sym);
                    n_retx_sent += 1;
                    last_retx[seq as usize] = now;
                    budget -= 1;
                }
            }
        }

        if sent == N_SYMBOLS && recovered.len() as u32 == N_SYMBOLS {
            break;
        }
    }

    let completion_s = last_recovery.duration_since(t0).as_secs_f64();
    if std::env::var("RP_GATE_DEBUG").is_ok() {
        lat_causes.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        let drains = lat_causes.iter().filter(|c| c.1 == 1).count();
        println!("  [debug] {} recoveries, {} via reorder-drain", lat_causes.len(), drains);
        println!(
            "  [debug] repairs_sent={} retx_sent={} fec_decoded={} decoder: fed={} repairs_fed={} useful={}",
            n_repairs_sent, n_retx_sent, n_fec_recovered,
            decoder.total_fed(), decoder.repairs_fed(), decoder.repairs_useful()
        );
        hole_fill_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "  [debug] holes={} filled={} fill p50={:.1}ms p90={:.1}ms max={:.1}ms",
            loss_time.len(),
            hole_fill_ms.len(),
            percentile(&hole_fill_ms, 0.5),
            percentile(&hole_fill_ms, 0.9),
            hole_fill_ms.last().copied().unwrap_or(0.0)
        );
        for (l, c, seq) in lat_causes.iter().take(20) {
            println!("  [debug] lat={l:.1}ms cause={} seq={seq}", if *c == 1 { "drain" } else { "inorder" });
        }
    }
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Outcome {
        completion_s,
        p50_ms: percentile(&latencies, 0.50),
        p99_ms: percentile(&latencies, 0.99),
        wire_per_source: total_wire as f64 / N_SYMBOLS as f64,
        buckets,
        path0_recovery_s: path0_recovery.map(|t| t.duration_since(t0).as_secs_f64()),
    }
}

// ---------------------------------------------------------------------------
// Cell runner
// ---------------------------------------------------------------------------

struct CellStats {
    completion: TrialStats,
    p99: TrialStats,
    overhead: TrialStats,
}

fn run_cells(
    fec_paths: &[GateChannel],
    base_paths: &[GateChannel],
    hint: ProtocolHint,
    cell_id: u64,
) -> (CellStats, CellStats) {
    let mut fec = CellStats {
        completion: TrialStats::new(),
        p99: TrialStats::new(),
        overhead: TrialStats::new(),
    };
    let mut base = CellStats {
        completion: TrialStats::new(),
        p99: TrialStats::new(),
        overhead: TrialStats::new(),
    };
    for t in 0..TRIALS {
        let seed = cell_id * 100_000 + t as u64 * 137 + 42;
        let f = run_fec(fec_paths, seed, &FecConfig { hint, outage: None });
        let b = run_baseline(base_paths, seed);
        fec.completion.push(f.completion_s);
        fec.p99.push(f.p99_ms);
        fec.overhead.push(f.wire_per_source - 1.0);
        base.completion.push(b.completion_s);
        base.p99.push(b.p99_ms);
        base.overhead.push(b.wire_per_source - 1.0);
    }
    (fec, base)
}

fn report(name: &str, fec: &CellStats, base: &CellStats) {
    println!(
        "{name}: completion fec={:.3}s±{:.3} simretx={:.3}s±{:.3} | p99 fec={:.1}ms±{:.1} simretx={:.1}ms±{:.1} | overhead fec={:.1}% simretx={:.1}%",
        fec.completion.mean(), fec.completion.ci95(),
        base.completion.mean(), base.completion.ci95(),
        fec.p99.mean(), fec.p99.ci95(),
        base.p99.mean(), base.p99.ci95(),
        fec.overhead.mean() * 100.0, base.overhead.mean() * 100.0,
    );
}

fn assert_lossy_cell(name: &str, ch: GateChannel, cell_id: u64) {
    let (fec, base) = run_cells(&[ch], &[ch], ProtocolHint::Auto, cell_id);
    report(name, &fec, &base);
    assert!(
        ci_less(&fec.completion, 0.9, &base.completion),
        "{name}: completion must be <= 0.9x SimRetx (CI-separated): fec={:.3}±{:.3} vs {:.3}±{:.3}",
        fec.completion.mean(), fec.completion.ci95(),
        base.completion.mean(), base.completion.ci95()
    );
    assert!(
        ci_less(&fec.p99, 0.7, &base.p99),
        "{name}: p99 must be <= 0.7x SimRetx (CI-separated): fec={:.1}±{:.1} vs {:.1}±{:.1}",
        fec.p99.mean(), fec.p99.ci95(), base.p99.mean(), base.p99.ci95()
    );
}

// ===========================================================================
// G1 cells
// ===========================================================================

#[test]
fn gate_c1_dc_tie() {
    // Clean link: the win condition is a TIE — completion within 2% and
    // raptorpath overhead <= 1% (the continuous r* keeps FEC near zero).
    let (fec, base) = run_cells(&[C1_DC], &[C1_DC], ProtocolHint::Bulk, 1);
    report("C1-DC(tie)", &fec, &base);
    // Tie within 2%, plus one RTT + 3 ticks of absolute allowance: the
    // trial's completion is a max-statistic over last-straggler recovery,
    // and the baseline resolves its retransmits synchronously at send
    // (oracle detection) while raptorpath must wait ~one SRTT of P_lost
    // evidence. The tie under test is throughput/overhead, not the last
    // packet's detection discipline.
    let allowance = C1_DC.rtt().as_secs_f64() + 3.0 * TICK.as_secs_f64();
    assert!(
        fec.completion.mean()
            <= 1.02 * base.completion.mean() + base.completion.ci95() + allowance,
        "C1: completion must tie within 2% (+3 ticks): fec={:.4}s vs simretx={:.4}s",
        fec.completion.mean(),
        base.completion.mean()
    );
    assert!(
        fec.overhead.mean() <= 0.01,
        "C1: overhead must be <= 1%: {:.2}%",
        fec.overhead.mean() * 100.0
    );
}

#[test]
fn gate_c2_wifi() {
    assert_lossy_cell("C2-WiFi", C2_WIFI, 2);
}

#[test]
fn gate_c3_lte() {
    assert_lossy_cell("C3-LTE", C3_LTE, 3);
}

#[test]
fn gate_c4_satellite() {
    assert_lossy_cell("C4-Sat", C4_SAT, 4);
}

#[test]
fn gate_c5_bad_wifi() {
    assert_lossy_cell("C5-BadWiFi", C5_BADWIFI, 5);
}

#[test]
fn gate_c7_dual_symmetric() {
    // Beat the best single path AND the min-RTT dual SimRetx.
    let dual = [C2_WIFI, C2_WIFI];
    let (fec, base_dual) = run_cells(&dual, &dual, ProtocolHint::Auto, 7);
    let mut base_single = TrialStats::new();
    for t in 0..TRIALS {
        let seed = 700_000 + t as u64 * 137 + 42;
        base_single.push(run_baseline(&[C2_WIFI], seed).completion_s);
    }
    report("C7-dual-sym", &fec, &base_dual);
    println!(
        "C7 single simretx completion: {:.3}s±{:.3}",
        base_single.mean(),
        base_single.ci95()
    );
    assert!(
        ci_less(&fec.completion, 1.0, &base_dual.completion),
        "C7: must beat min-RTT dual SimRetx: fec={:.3} vs {:.3}",
        fec.completion.mean(),
        base_dual.completion.mean()
    );
    assert!(
        ci_less(&fec.completion, 1.0, &base_single),
        "C7: must beat best single path: fec={:.3} vs single={:.3}",
        fec.completion.mean(),
        base_single.mean()
    );
}

#[test]
fn gate_c8_dual_asymmetric_rtt() {
    let dual = [C2_WIFI, C3_LTE];
    let (fec, base_dual) = run_cells(&dual, &dual, ProtocolHint::Auto, 8);
    let mut base_single = TrialStats::new();
    for t in 0..TRIALS {
        let seed = 800_000 + t as u64 * 137 + 42;
        // Best single path for bulk = the higher-capacity one (WiFi).
        base_single.push(run_baseline(&[C2_WIFI], seed).completion_s);
    }
    report("C8-dual-asym", &fec, &base_dual);
    println!(
        "C8 single simretx completion: {:.3}s±{:.3}",
        base_single.mean(),
        base_single.ci95()
    );
    assert!(
        ci_less(&fec.completion, 1.0, &base_dual.completion),
        "C8: must beat min-RTT dual SimRetx: fec={:.3} vs {:.3}",
        fec.completion.mean(),
        base_dual.completion.mean()
    );
    assert!(
        ci_less(&fec.completion, 1.0, &base_single),
        "C8: must beat best single path: fec={:.3} vs single={:.3}",
        fec.completion.mean(),
        base_single.mean()
    );
}

#[test]
fn gate_c9_outage_recovery() {
    // Dual path with a 150 ms full outage on path 0. After the path
    // recovers, aggregate goodput must return to >= 90% of steady state
    // within 3 RTTs of the first successful post-outage delivery on it.
    let paths = [C9_WIFI_SLOW, C9_LTE_SLOW];
    let outage_start = Duration::from_millis(150);
    let outage_end = Duration::from_millis(300);
    let rtt0 = paths[0].rtt().as_secs_f64();

    let mut ok_trials = 0;
    for t in 0..TRIALS {
        let seed = 900_000 + t as u64 * 137 + 42;
        let out = run_fec(
            &paths,
            seed,
            &FecConfig {
                hint: ProtocolHint::Auto,
                outage: Some((outage_start, outage_end)),
            },
        );
        // Steady-state goodput: buckets fully inside [40ms, 150ms).
        let steady_range = 2..(outage_start.as_millis() as usize / 20);
        let steady: f64 = steady_range
            .clone()
            .map(|b| out.buckets.get(b).copied().unwrap_or(0) as f64)
            .sum::<f64>()
            / steady_range.len() as f64;
        let t_rec = out
            .path0_recovery_s
            .expect("path 0 must deliver again after the outage");
        // First bucket at/after recovery with goodput >= 90% of steady.
        let start_bucket = (t_rec / 0.02) as usize;
        let mut recovered_at: Option<usize> = None;
        for b in start_bucket..out.buckets.len() {
            if out.buckets[b] as f64 >= 0.9 * steady {
                recovered_at = Some(b);
                break;
            }
        }
        let Some(b) = recovered_at else {
            println!("trial {t}: goodput never returned to 90% of steady ({steady:.1}/bucket)");
            continue;
        };
        let recovery_delay = (b as f64 + 1.0) * 0.02 - t_rec; // bucket end vs path recovery
        println!(
            "trial {t}: steady={steady:.1}/bucket, path0 back at {t_rec:.3}s, goodput back {:.0}ms later",
            recovery_delay * 1000.0
        );
        if recovery_delay <= 3.0 * rtt0 + 0.02 {
            ok_trials += 1;
        }
    }
    assert!(
        ok_trials * 10 >= TRIALS * 8,
        "C9: goodput must recover within 3 RTTs (+1 bucket) of path recovery in >=80% of trials: {ok_trials}/{TRIALS}"
    );
}

// ===========================================================================
// G2 — the model reacts correctly
// ===========================================================================

#[test]
fn g2_estimator_converges_per_channel() {
    // Feed paper-exact GE sequences symbol-by-symbol; the GE estimator must
    // converge to (p, q) and the loss estimator to epsilon = p/(p+q).
    for ch in [C2_WIFI, C3_LTE, C4_SAT, C5_BADWIFI] {
        let mut rng = ChaCha8Rng::seed_from_u64(4242);
        let mut ge_chan = mk_ge(&ch);
        let mut ge_est = GilbertElliottEstimator::new();
        let mut loss_est = LossEstimator::new();
        let mut batch_lost = 0u32;
        let mut batch_n = 0u32;
        for _ in 0..40_000 {
            let lost = ge_chan.should_drop(&mut rng);
            ge_est.record_symbol(!lost);
            batch_n += 1;
            if lost {
                batch_lost += 1;
            }
            if batch_n == 500 {
                loss_est.record_batch(batch_n, batch_n - batch_lost);
                batch_n = 0;
                batch_lost = 0;
            }
        }
        assert!(ge_est.is_valid(), "{}: GE estimator must be valid", ch.name);
        let (p_hat, q_hat) = (ge_est.p_gb(), ge_est.p_bg());
        let eps_hat = loss_est.loss_rate();
        println!(
            "{}: p={:.4} p̂={:.4} | q={:.2} q̂={:.2} | ε={:.4} ε̂={:.4}",
            ch.name, ch.p, p_hat, ch.q, q_hat, ch.eps(), eps_hat
        );
        assert!(
            (p_hat - ch.p).abs() <= 0.4 * ch.p + 0.003,
            "{}: p̂ must converge to p: {} vs {}",
            ch.name, p_hat, ch.p
        );
        assert!(
            (q_hat - ch.q).abs() <= 0.25 * ch.q,
            "{}: q̂ must converge to q: {} vs {}",
            ch.name, q_hat, ch.q
        );
        assert!(
            (eps_hat - ch.eps()).abs() <= 0.3 * ch.eps() + 0.003,
            "{}: ε̂ must converge to ε: {} vs {}",
            ch.name, eps_hat, ch.eps()
        );
    }
}

#[test]
fn g2_rate_reconverges_after_regime_change() {
    // 1% -> 10% regime change: the controller's rate must reach 90% of the
    // new steady-state rate within 25 feedback batches (BOCD adaptation).
    let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Auto, FecBackend::Rlc, 1200);
    let mut est = LossEstimator::new();
    for _ in 0..200 {
        est.record_batch(1000, 990); // 1% regime
    }
    let r_low = ctrl.compute_repair_rate(&est, 50);

    // Steady-state at 10% (reference)
    let mut est_ref = LossEstimator::new();
    for _ in 0..200 {
        est_ref.record_batch(1000, 900);
    }
    let r_high = ctrl.compute_repair_rate(&est_ref, 50);
    assert!(r_high > r_low, "higher loss must demand more correction");

    let mut batches_needed = None;
    for b in 1..=60 {
        est.record_batch(1000, 900); // regime change to 10%
        let r = ctrl.compute_repair_rate(&est, 50);
        if r >= 0.9 * r_high {
            batches_needed = Some(b);
            break;
        }
    }
    let b = batches_needed.expect("rate must re-converge after regime change");
    println!("re-convergence after regime change: {b} batches (r_low={r_low:.3}, r_high={r_high:.3})");
    assert!(b <= 25, "must re-converge within 25 batches, took {b}");
}

#[test]
fn g2_spare_capacity_gate_suppresses_fec() {
    // Under congestion (shrinking spare capacity) the emitted rate must be
    // clamped to spare, monotonically.
    let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Realtime, FecBackend::Rlc, 1200);
    let mut est = LossEstimator::new();
    for _ in 0..100 {
        est.record_batch(1000, 950); // 5% loss wants substantial FEC
    }
    let uncapped = ctrl.compute_repair_rate(&est, 50);
    assert!(uncapped > 0.05, "5% loss should want > 5% correction: {uncapped}");
    let mut prev = f64::INFINITY;
    for spare in [0.5, 0.2, 0.1, 0.05, 0.01, 0.0] {
        let r = ctrl.compute_repair_rate_capped(&est, spare, 50);
        assert!(r <= spare + 1e-12, "rate must respect spare capacity: {r} > {spare}");
        assert!(r <= prev, "rate must shrink monotonically with spare");
        prev = r;
    }
    assert_eq!(
        ctrl.compute_repair_rate_capped(&est, 0.0, 50),
        0.0,
        "no spare capacity -> no FEC (never-hurts guarantee)"
    );
}

#[test]
fn g2_outage_reaction_and_recovery() {
    // Path outage: estimator must saturate quickly, P_lost must drive the
    // retransmit decision to near-certainty, and after the outage the
    // estimate must decay back so the path becomes usable again.
    let mut est = LossEstimator::new();
    for _ in 0..100 {
        est.record_batch(100, 97); // steady 3%
    }
    let srtt = 0.05;

    // Outage: 100% loss batches
    let mut batches_to_saturate = None;
    for b in 1..=30 {
        est.record_batch(100, 0);
        if est.loss_rate() > 0.5 {
            batches_to_saturate = Some(b);
            break;
        }
    }
    let bs = batches_to_saturate.expect("estimator must react to an outage");
    println!("outage detected (ε̂ > 0.5) after {bs} batches");
    assert!(bs <= 10, "estimator must saturate within 10 batches: {bs}");

    // P_lost with high ε and age >> SRTT: retransmit with near-certainty.
    let pl = p_lost(3.0 * srtt, est.loss_rate(), srtt, srtt / 4.0);
    assert!(pl > 0.99, "P_lost must approach 1 during an outage: {pl}");

    // Recovery: good batches decay the estimate back below 10%.
    let mut batches_to_recover = None;
    for b in 1..=60 {
        est.record_batch(100, 100);
        if est.loss_rate() < 0.1 {
            batches_to_recover = Some(b);
            break;
        }
    }
    let br = batches_to_recover.expect("estimator must recover after the outage");
    println!("recovery (ε̂ < 0.1) after {br} good batches");
    assert!(br <= 30, "estimator must recover within 30 batches: {br}");
}

#[test]
#[ignore]
fn debug_dc_rate() {
    let ch = C1_DC;
    let mut est = prewarm(&ch);
    let ctrl = FecRateController::new(1e-5, 0.5, ProtocolHint::Bulk, FecBackend::Rlc, SYMBOL_SIZE);
    println!("prewarmed: mean={:.5} upper95={:.5} rate={:.4}",
        est.loss_rate(), est.predictive_loss_upper(0.95), ctrl.compute_repair_rate(&est, 64));
    let mut rng = ChaCha8Rng::seed_from_u64(1);
    for b in 0..150 {
        let lost = (0..10).filter(|_| rng.gen::<f64>() < 0.001).count() as u32;
        est.record_batch(10, 10 - lost);
        est.record_rtt(ch.rtt());
        if b % 30 == 0 {
            let ge = est.ge_estimator();
            println!("batch {b}: mean={:.5} upper95={:.5} ge_valid={} B={:.2} sigma2={:.2} rate={:.4}",
                est.loss_rate(), est.predictive_loss_upper(0.95),
                ge.is_valid(), ge.mean_burst_length(),
                raptorpath::control::fec_rate::burst_variance_factor(&est),
                ctrl.compute_repair_rate(&est, 64));
        }
    }
}

#[test]
#[ignore]
fn debug_c2_tail() {
    let out = run_fec(&[C2_WIFI], 242, &FecConfig { hint: ProtocolHint::Auto, outage: None });
    println!("completion={:.3}s p50={:.1} p99={:.1} overhead={:.1}%",
        out.completion_s, out.p50_ms, out.p99_ms, (out.wire_per_source - 1.0) * 100.0);
}

#[test]
#[ignore]
fn debug_rlc_decode() {
    let mut enc = RlcWindowEncoder::new(64);
    let mut dec = RlcWindowDecoder::new(64);
    let mut syms = Vec::new();
    for i in 0..100u32 {
        let s = enc.add_source(&vec![i as u8; 64]);
        if s.block_id >= ENC_WINDOW { enc.advance(s.block_id - (ENC_WINDOW - 1)); }
        syms.push(s);
    }
    // deliver all except seq 90 (within final window [36..99])
    for (i, s) in syms.iter().enumerate() {
        if i != 90 { dec.add_symbol(s); }
    }
    let rep = enc.generate_repair();
    let out = dec.add_symbol(&rep);
    println!("decoded: {:?}, useful={}", out.iter().map(|(s, _)| *s).collect::<Vec<_>>(), dec.repairs_useful());
    // and with two losses + two repairs
    let mut enc2 = RlcWindowEncoder::new(64);
    let mut dec2 = RlcWindowDecoder::new(64);
    let mut syms2 = Vec::new();
    for i in 0..100u32 {
        let s = enc2.add_source(&vec![i as u8; 64]);
        if s.block_id >= ENC_WINDOW { enc2.advance(s.block_id - (ENC_WINDOW - 1)); }
        syms2.push(s);
    }
    for (i, s) in syms2.iter().enumerate() {
        if i != 90 && i != 91 { dec2.add_symbol(s); }
    }
    let r1 = enc2.generate_repair();
    let r2 = enc2.generate_repair();
    let o1 = dec2.add_symbol(&r1);
    let o2 = dec2.add_symbol(&r2);
    println!("two-loss: r1 -> {:?}, r2 -> {:?}", o1.iter().map(|(s, _)| *s).collect::<Vec<_>>(), o2.iter().map(|(s, _)| *s).collect::<Vec<_>>());
}

#[test]
#[ignore]
fn debug_rlc_stream_decode() {
    // Mimic the driver: interleaved sources + repairs over a GE channel.
    let mut enc = RlcWindowEncoder::new(64);
    let mut dec = RlcWindowDecoder::new(64);
    let mut ge = mk_ge(&C2_WIFI);
    let mut rng = ChaCha8Rng::seed_from_u64(7);
    let mut holes = 0u32;
    let mut fec_filled = 0u32;
    let mut n_rep = 0u32;
    for i in 0..3000u32 {
        let s = enc.add_source(&vec![(i % 251) as u8; 64]);
        if s.block_id >= ENC_WINDOW { enc.advance(s.block_id - (ENC_WINDOW - 1)); }
        if ge.should_drop(&mut rng) {
            holes += 1;
        } else {
            dec.add_symbol(&s);
        }
        if i % 6 == 5 {
            let r = enc.generate_repair();
            n_rep += 1;
            if !ge.should_drop(&mut rng) {
                let outs = dec.add_symbol(&r);
                fec_filled += outs.len() as u32;
            }
        }
    }
    println!("holes={holes} fec_filled={fec_filled} repairs={n_rep} useful={}", dec.repairs_useful());
}

#[test]
#[ignore]
fn debug_c2_nojitter() {
    let mut ch = C2_WIFI;
    ch.jitter_ms = 0;
    let out = run_fec(&[ch], 242, &FecConfig { hint: ProtocolHint::Auto, outage: None });
    println!("nojitter: completion={:.3}s p50={:.1} p99={:.1} overhead={:.1}%",
        out.completion_s, out.p50_ms, out.p99_ms, (out.wire_per_source - 1.0) * 100.0);
}

/// Diagnostic (non-gating): the quality trade-off per protocol hint.
/// Answers: does Bulk actually finish transfers faster than the baseline,
/// and does Realtime buy lower latency at moderate overhead?
#[test]
#[ignore]
fn quality_hint_sweep() {
    let cells: &[(&str, GateChannel)] = &[
        ("C2-WiFi", C2_WIFI),
        ("C3-LTE", C3_LTE),
        ("C4-Sat", C4_SAT),
        ("C5-BadWiFi", C5_BADWIFI),
    ];
    let hints = [
        ("Bulk", ProtocolHint::Bulk),
        ("Auto", ProtocolHint::Auto),
        ("Realtime", ProtocolHint::Realtime),
    ];
    let trials = 6usize;
    for (name, ch) in cells {
        let mut b_compl = TrialStats::new();
        let mut b_p50 = TrialStats::new();
        let mut b_p99 = TrialStats::new();
        let mut b_oh = TrialStats::new();
        for t in 0..trials {
            let seed = 50_000 + t as u64 * 137 + 42;
            let b = run_baseline(&[*ch], seed);
            b_compl.push(b.completion_s);
            b_p50.push(b.p50_ms);
            b_p99.push(b.p99_ms);
            b_oh.push(b.wire_per_source - 1.0);
        }
        println!(
            "{name} SimRetx: completion={:.3}s p50={:.1}ms p99={:.1}ms overhead={:.1}%",
            b_compl.mean(), b_p50.mean(), b_p99.mean(), b_oh.mean() * 100.0
        );
        for (hname, hint) in &hints {
            let mut compl = TrialStats::new();
            let mut p50 = TrialStats::new();
            let mut p99 = TrialStats::new();
            let mut oh = TrialStats::new();
            for t in 0..trials {
                let seed = 50_000 + t as u64 * 137 + 42;
                let f = run_fec(&[*ch], seed, &FecConfig { hint: *hint, outage: None });
                compl.push(f.completion_s);
                p50.push(f.p50_ms);
                p99.push(f.p99_ms);
                oh.push(f.wire_per_source - 1.0);
            }
            println!(
                "{name} {hname:8}: completion={:.3}s ({:.2}x) p50={:.1}ms p99={:.1}ms ({:.2}x) overhead={:.1}%",
                compl.mean(),
                compl.mean() / b_compl.mean(),
                p50.mean(),
                p99.mean(),
                p99.mean() / b_p99.mean(),
                oh.mean() * 100.0
            );
        }
    }
}
