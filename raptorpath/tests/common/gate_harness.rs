// The ADR-0051 L0 gate harness: the paper §2.3 Gilbert-Elliott channels, the
// SimRetx / SimQUIC baselines and the raptorpath FEC+ARQ model, with the cell
// runner. Textually `include!`d (not a module) by `gate_suite.rs` (the gates)
// and `ablation_bench.rs` (the ignored diagnostics), so both see one copy of
// the harness with no visibility plumbing. The includer supplies `mod common;`
// and the `use` lines.
const SYMBOL_SIZE: u16 = 1200;
const WIRE_BYTES: f64 = 1225.0; // symbol + per-symbol wire overhead
const N_SYMBOLS: u32 = 1500;
const BATCH: u32 = 10;
const TRIALS: usize = 10;
const TICK: Duration = Duration::from_micros(500);
const ENC_WINDOW: u64 = 64; // encoder sliding window (paper W)

// ---------------------------------------------------------------------------
// ADR-0051 channels — paper §2.3 GE parameterization (h_G=0, h_B=1, so
// epsilon = p/(p+q) exactly).
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
// SimQuic baseline (P3): loss-blind delay-based CC + SACK-timed ARQ, no FEC,
// in-order stream delivery, single path. The QUIC-class adversary.
// ---------------------------------------------------------------------------

/// SimQuic: models QUIC-as-deployed at L0 fidelity. Single path (no MPQUIC
/// in the wild), a modern loss-BLIND delay-based CC (Copa-lite, identical
/// structure to run_fec's — BBR-class behavior: loss does not shrink the
/// window), and sender-side SACK-timed ARQ with QUIC's time-threshold loss
/// detection (declared lost after 9/8 x SRTT without acknowledgment, RFC 9002
/// kTimeThreshold). No oracle: the channel is the same lossy, non-reliable
/// SimChannel run_fec uses, and the sender only learns about a loss by the
/// retransmit timer expiring. In-order stream delivery semantics (single
/// QUIC stream): head-of-line blocking on every hole until the retransmit
/// arrives — the structural cost FEC removes.
fn run_baseline_quic(paths: &[GateChannel], seed: u64) -> Outcome {
    assert_eq!(paths.len(), 1, "SimQuic models deployed QUIC: single path only");
    let ch = paths[0];
    let clock = Arc::new(MockClock::new());
    let t0 = clock.now();

    let mut chan = SimChannel::new(
        clock.clone(),
        seed,
        Duration::from_millis(ch.one_way_ms),
        ch.jitter_ms,
        mk_ge(&ch),
    );
    if let Some(bps) = ch.capacity_bps {
        chan = chan.with_link(bps, ch.queue);
    }

    // Copa-lite CC — same structure as run_fec: token-bucket pacing at
    // cwnd/SRTT with a small burst allowance, per-RTT window update driven
    // by the MIN RTT sample in the window vs the propagation floor.
    let base = ch.rtt().as_secs_f64();
    let cap = ch.bdp_cwnd() as f64 * 2.0;
    let mut cwnd = ch.bdp_cwnd() as f64 / 2.0;
    let mut srtt = base;
    let mut tokens = 10.0f64;
    let mut last_flush = t0;
    let mut min_rtt_win = f64::INFINITY;
    let mut ramping = true;

    // Sender-side reliability state (per SOURCE seq).
    let mut send_time: Vec<Instant> = Vec::with_capacity(N_SYMBOLS as usize); // first send
    let mut last_send: Vec<Instant> = Vec::with_capacity(N_SYMBOLS as usize);
    let mut deliver_time: Vec<Option<Instant>> = vec![None; N_SYMBOLS as usize];
    let mut outstanding: BTreeSet<u32> = BTreeSet::new(); // sent, not yet delivered
    // Per WIRE transmission: source seq + send instant (for SRTT samples).
    let mut wire_to_src: Vec<u32> = Vec::new();
    let mut wire_send_time: Vec<Instant> = Vec::new();

    let mut total_wire: u64 = 0;
    let mut sent: u32 = 0;
    let mut delivered: u32 = 0;
    let deadline = t0 + Duration::from_secs(600);

    while delivered < N_SYMBOLS {
        let now = clock.now();
        assert!(now < deadline, "simquic trial did not complete");

        // Replenish pacing tokens: rate = cwnd/SRTT, small burst allowance.
        tokens = (tokens + cwnd / srtt * TICK.as_secs_f64()).min((cwnd / 8.0).max(10.0));

        // 1) SACK-timed retransmissions first (oldest hole first). A packet
        //    is declared lost when 9/8 x SRTT has elapsed since its LAST
        //    transmission without a delivery; repeat until delivered.
        for &seq in &outstanding {
            if tokens < 1.0 {
                break;
            }
            if now.duration_since(last_send[seq as usize]).as_secs_f64() > 1.125 * srtt {
                let sym = make_wire_symbol_sized(seq, false, SYMBOL_SIZE as usize);
                wire_to_src.push(seq);
                wire_send_time.push(now);
                tokens -= 1.0;
                chan.send(sym);
                total_wire += 1;
                last_send[seq as usize] = now;
            }
        }

        // 2) New source symbols while pacing allows.
        while sent < N_SYMBOLS && tokens >= 1.0 {
            let sym = make_wire_symbol_sized(sent, false, SYMBOL_SIZE as usize);
            send_time.push(now);
            last_send.push(now);
            outstanding.insert(sent);
            wire_to_src.push(sent);
            wire_send_time.push(now);
            tokens -= 1.0;
            chan.send(sym);
            total_wire += 1;
            sent += 1;
        }

        // 3) Tick + deliveries (receiver simulated directly; ACK leg is
        //    folded into the SRTT sample like the other drivers).
        clock.advance(TICK);
        let now = clock.now();
        for pkt in chan.deliver() {
            let wire_seq = pkt.seq as usize;
            let src = wire_to_src[wire_seq] as usize;
            let sample = pkt
                .delivery_time
                .duration_since(wire_send_time[wire_seq])
                .as_secs_f64()
                + ch.one_way_ms as f64 / 1000.0; // + ACK return leg
            srtt = 0.875 * srtt + 0.125 * sample;
            min_rtt_win = min_rtt_win.min(sample);
            if deliver_time[src].is_none() {
                deliver_time[src] = Some(pkt.delivery_time);
                outstanding.remove(&(src as u32));
                delivered += 1;
            }
        }

        // 4) Copa-lite per-RTT window update: loss-blind. Ramp x1.5+1 while
        //    the min sample sits on the propagation floor; once the standing
        //    queue shows (min > 1.125 x base), oscillate +2 / x0.92.
        if now.duration_since(last_flush).as_secs_f64() >= srtt {
            if min_rtt_win.is_finite() {
                if min_rtt_win > base * 1.125 {
                    ramping = false;
                    cwnd = (cwnd * 0.92).max(4.0);
                } else if ramping {
                    cwnd = (cwnd * 1.5 + 1.0).min(cap);
                } else {
                    cwnd = (cwnd + 2.0).min(cap);
                }
                min_rtt_win = f64::INFINITY;
            }
            last_flush = now;
        }
    }

    // Single QUIC stream: the application sees IN-ORDER delivery.
    let mut latencies: Vec<f64> = Vec::with_capacity(N_SYMBOLS as usize);
    let mut buckets: Vec<u32> = Vec::new();
    let mut t_inorder = t0;
    for seq in 0..N_SYMBOLS as usize {
        let t = deliver_time[seq].expect("simquic delivers everything");
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

    Outcome {
        completion_s,
        p50_ms: percentile(&latencies, 0.50),
        p99_ms: percentile(&latencies, 0.99),
        wire_per_source: total_wire as f64 / N_SYMBOLS as f64,
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

/// Expected delivery time score for source-path selection (paper §5.7), with
/// the geometric retransmit chain: expected retries = eps/(1-eps).
fn path_score(srtt: f64, eps: f64) -> f64 {
    let chain = (eps / (1.0 - eps).max(1e-6)).min(50.0);
    srtt / 2.0 + chain * 1.5 * srtt
}

struct FecConfig {
    hint: ProtocolHint,
    /// Outage on path 0: (start, end) after t0.
    outage: Option<(Duration, Duration)>,
    /// P1: protocol hint also tightens the Copa queue target (paper §8.2).
    hint_delay_target: bool,
    /// P2: Copa floor is a running min estimate, not ground truth.
    estimated_floor: bool,
    /// P4a/P6: Bulk maps to the completion-exposure glide (paper §4.6):
    /// pure-ARQ steady state (χ = 0 ⇒ r* = 0), ramping to the tail budget
    /// over the final ~1.5 SRTT (χ is fed per tick).
    bulk_arq_delta: bool,
    /// P4b: burst of repairs covering the final window at end-of-stream.
    /// Under the Bulk χ glide (bulk_arq_delta) the burst is subsumed by
    /// the ramp and skipped; non-Bulk hints keep it.
    tail_fec: bool,
    /// P5: cap r at the p99(r) saturation point.
    saturation_cap: bool,
}

/// Default configuration: all improvements ON (each worker's flag gates its
/// own change; ablations flip individual flags off against the same seeds).
fn cfg(hint: ProtocolHint) -> FecConfig {
    FecConfig {
        hint,
        outage: None,
        hint_delay_target: true,
        estimated_floor: true,
        bulk_arq_delta: true,
        tail_fec: true,
        saturation_cap: true,
    }
}

fn run_fec(paths: &[GateChannel], seed: u64, cfg: &FecConfig) -> Outcome {
    let clock = Arc::new(MockClock::new());
    let t0 = clock.now();
    let n_paths = paths.len();

    let mut chans: Vec<SimChannel> = Vec::new();
    let mut ests: Vec<LossEstimator> = Vec::new();
    // Copa-lite congestion window (paper §8): delay-based — grow while
    // queueing delay is low, back off when SRTT rises above the propagation
    // floor. Keeps queues near-empty (low p99), unlike the baseline's
    // loss-based AIMD which fills the buffer.
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
    // Copa uses the minimum RTT sample per window: the min sees through
    // transient serialization bursts to the standing queue; an EWMA stays
    // inflated long after the queue drains and causes a backoff spiral.
    let mut min_rtt_win: Vec<f64> = Vec::new();
    // P2 (estimated_floor): running lifetime-min RTT sample = the endpoint's
    // estimate of the propagation floor. Copa uses a 10 s min window, which
    // exceeds any trial here, so lifetime min is the faithful analogue. The
    // ground-truth paths[i].rtt() is a simulation cheat a real endpoint
    // cannot make.
    let mut rtt_floor: Vec<f64> = Vec::new();
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
        rtt_floor.push(f64::INFINITY);
        ramping.push(true);
    }

    let mut ctrl = FecRateController::new(1e-5, 0.5, cfg.hint, FecBackend::Rlc, SYMBOL_SIZE);
    // P5: cap r at the p99(r) saturation point (paper §4.4).
    ctrl.set_saturation_cap(cfg.saturation_cap);
    // P4a/P6: Bulk maps δ to the completion-exposure glide
    // δ_eff = ε̂ + (0.05 − ε̂)·χ (paper §4.6): pure ARQ mid-stream (χ = 0,
    // r* = 0 identically), ramping to the tail budget over the final
    // ~1.5 SRTT. χ is fed per tick below (the driver knows N_SYMBOLS).
    // Flag-gated for ablation; no-op for non-Bulk hints.
    ctrl.set_bulk_pure_arq(cfg.bulk_arq_delta);
    // P10a: the inner-feedback weight stays at its default 0 — the gate's
    // payload is the transfer (file-transfer semantics), its delivery latency
    // does not feed back into its own throughput, so mid-stream ARQ recovery
    // is free and Bulk keeps the pure glide.
    // P6 rides P4a's flag: χ only matters under the Bulk glide.
    let chi_active = cfg.hint == ProtocolHint::Bulk && cfg.bulk_arq_delta;
    let mut encoder = RlcWindowEncoder::new(SYMBOL_SIZE);
    let mut decoder = RlcWindowDecoder::new(SYMBOL_SIZE);
    let max_delay = paths.iter().map(|c| c.one_way_ms + c.jitter_ms).max().unwrap();
    let mut reorder = ReorderBuffer::new(2 * max_delay + 10, 4000);

    // Encoder lag = jitter horizon in symbols. A repair covering symbols
    // that cannot yet have arrived (jitter lets a repair overtake up to
    // jitter x send_rate sources) carries no usable information at arrival
    // time — it parks as a deep pivot instead of decoding the actual loss.
    // Lagging the encoder behind the send stream by that horizon makes a
    // repair's unknowns true losses, decodable on arrival.
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
    // Path a source symbol was last sent on — the retransmit gate must use
    // that path's SRTT and loss rate. A global (srtt_min, eps_max) gate
    // declares slow-path in-flights lost at the fast path's timescale and
    // saturates P_lost for all in-flights during an outage (spurious-retx
    // storm once retransmits preempt source).
    let mut src_path: Vec<usize> = Vec::with_capacity(N_SYMBOLS as usize);
    // Retransmit round per symbol: the k-th round sends min(k, 3) copies. A
    // symbol whose retransmit was also lost is deep in the completion tail,
    // and each further single-copy round is another eps-coin-flip costing a
    // full RTT; escalating copies converts that multiplicative tail into an
    // additive one for eps² of holes, a negligible volume cost.
    let mut retx_round: Vec<u8> = Vec::with_capacity(N_SYMBOLS as usize);
    // Highest source seq arrived on each path (wire-level, pre-decode):
    // packet-threshold gap evidence (RFC 9002 kPacketThreshold). A hole with
    // ≥ enc_lag + 3 later seqs arrived on its own path is lost with
    // near-certainty — no need to wait out the time threshold.
    let mut max_arr_src_on_path: Vec<u64> = vec![0; n_paths];
    // Decode instant per source seq (diagnostics: separates decode time
    // from reorder-release time in the completion tail).
    let mut decode_time: Vec<Option<Instant>> = vec![None; N_SYMBOLS as usize];
    let mut recovered: BTreeSet<u64> = BTreeSet::new();
    // Decode-level receipt (pre-reorder) — the sender's SACK view. The
    // retransmit scan must use this, not the reorder-released set, or
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
    // P4b: end-of-stream tail FEC, fired exactly once. Repairs are
    // pre-generated (the encoder window moves at the flush) and drained under
    // pacing tokens on the best path.
    let mut tail_flushed = false;
    let mut tail_queue: VecDeque<WireSymbol> = VecDeque::new();
    let mut tail_best = 0usize;
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

    // Room = a send token is available. Tokens replenish at cwnd/SRTT per
    // second (Copa's sending rate) — the offered load is paced by sender
    // knowledge, independent of what the channel drops.
    macro_rules! has_room {
        ($i:expr, $now:expr) => {{
            let _ = $now;
            tokens[$i] >= 1.0
        }};
    }

    // Path-selection loss estimate: blend of the BOCD-informed median
    // (regime-aware, converges within a few batches) and the GE
    // state-conditional loss rate (eps_burst — flips on the first delivery
    // after an outage ends). The blend reacts within one symbol of a state
    // change while staying anchored to the posterior.
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

        // P6 — completion exposure (paper §4.6): the driver knows the
        // transfer length, so T_rem = (N − sent) / aggregate send rate.
        // SRTT is the slowest path's (its ARQ round is the one that can no
        // longer overlap sends); RTTVAR mirrors the retx gate's 0.125 × SRTT.
        // Unknown rate ⇒ T_rem = ∞ ⇒ χ = 0 (pure ARQ).
        if chi_active {
            let srtt_max = srtt.iter().cloned().fold(0.0f64, f64::max);
            let t_rem = if total_rate > 0.0 {
                (N_SYMBOLS - sent) as f64 / total_rate
            } else {
                f64::INFINITY
            };
            ctrl.set_completion_exposure(raptorpath_math::completion_exposure(
                t_rem,
                srtt_max,
                0.125 * srtt_max,
            ));
        }

        // Correction preemption: due corrections are sent before new source
        // symbols — they compete for the same wire budget and win when the
        // taper says they are due.
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

        // P4b: drain the end-of-stream tail burst under pacing tokens on the
        // best path — corrections preempt source (there is no source left by
        // then anyway).
        loop {
            let now2 = clock.now();
            if tail_queue.is_empty() || !has_room!(tail_best, now2) {
                break;
            }
            let rep = tail_queue.pop_front().unwrap();
            send_on!(tail_best, rep);
            n_repairs_sent += 1;
        }

        // P_lost-gated exact-source retransmit (correction symbols of the
        // retransmit kind, paper §3.4), cross-path via best E_i. Scanned every
        // tick — models per-ACK gap detection (SACK).
        //
        // The gate is path-aware: a symbol's loss evidence is measured on the
        // clock and loss rate of the path it was last sent on, with a
        // 9/8 × SRTT time-threshold floor (RFC 9002) plus a packet-threshold
        // detector when ARQ is the primary recovery. A global (srtt_min,
        // eps_max) gate declares slow-path in-flights lost at the fast path's
        // timescale and saturates P_lost for all in-flights during an outage.
        macro_rules! retx_scan {
            ($arq_primary:expr) => {{
                let now3 = clock.now();
                let mut budget = 20u32;
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
                    let sp = src_path[seq as usize];
                    let srtt_p = srtt[sp];
                    let age = now3.duration_since(encode_time[seq as usize]).as_secs_f64();
                    let pl = p_lost(age, eps_sel!(sp), srtt_p, 0.125 * srtt_p);
                    // Two detectors, QUIC-style (RFC 9002): packet threshold
                    // (later seqs arrived on the same path, beyond the
                    // jitter reorder horizon) and time threshold (9/8 × the
                    // path's own SRTT, weighted by P_lost). The packet
                    // threshold applies when ARQ is the primary recovery.
                    let gap_detected = $arq_primary
                        && max_arr_src_on_path[sp] >= seq + enc_lag as u64 + 3;
                    let time_detected =
                        age > 1.125 * srtt_p && pl > 0.9 && rng.gen::<f64>() < pl;
                    // End-of-stream cross-path tail reinjection: once nothing
                    // overlaps recovery, an undecoded symbol whose path is
                    // much slower than the best path is worth duplicating onto
                    // the fast path — the fast-path flight beats the slow
                    // path's residual queue + propagation wait, and the
                    // duplicate costs spare end-of-stream tokens only.
                    let tail_reinject =
                        sent == N_SYMBOLS && sp != best && srtt_p > 1.5 * srtt[best];
                    if (gap_detected || time_detected || tail_reinject)
                        && now3.duration_since(last_retx[seq as usize]).as_secs_f64() > srtt_p
                        && has_room!(best, now3)
                    {
                        let round = retx_round[seq as usize];
                        // Copy escalation only where rounds are serial
                        // (end-of-stream): mid-transfer a repeat-lost
                        // retransmit recovers in parallel with ongoing sends,
                        // so extra copies are pure overhead.
                        let copies = if sent == N_SYMBOLS {
                            (round as u32 + 1).min(3)
                        } else {
                            1
                        };
                        if std::env::var("RP_GATE_DEBUG").is_ok() {
                            println!(
                                "  [retx] t={:.1}ms seq={seq} age={:.1}ms pl={pl:.3} srtt_p={:.1}ms path={sp}->{best} copies={copies}",
                                now3.duration_since(t0).as_secs_f64() * 1000.0,
                                age * 1000.0, srtt_p * 1000.0
                            );
                        }
                        for _ in 0..copies {
                            if budget == 0 || !has_room!(best, now3) {
                                break;
                            }
                            send_on!(best, source_store[seq as usize].clone());
                            n_retx_sent += 1;
                            budget -= 1;
                        }
                        last_retx[seq as usize] = now3;
                        src_path[seq as usize] = best;
                        retx_round[seq as usize] = round.saturating_add(1);
                    }
                }
            }};
        }

        // FEC/ARQ budget coordination: when ambient repairs are flowing they
        // own the fast recovery path, and the retransmit kind is a backstop
        // that rides spare tokens after the send phase — a preempting
        // retransmit would double-spend wire on holes a repair is already in
        // flight for, and each duplicate displaces a source symbol onto the
        // slower path (stretching the multipath completion tail). When ARQ is
        // the primary recovery — the rate is ~0 (Bulk pure-ARQ steady state,
        // paper §4.5), or the hint is Bulk (mid-transfer lateness is free, so
        // a due retransmit preempts source like any correction) — it runs
        // before the send phase: starved behind it, all recovery serializes at
        // end-of-stream (hole-fill p50 ~110 ms instead of ~1.5 RTT at
        // C2-Bulk). The packet-threshold detector is enabled only in the true
        // pure-ARQ regime.
        let mut fec_rate_max = 0.0f64;
        for i in 0..n_paths {
            fec_rate_max =
                fec_rate_max.max(ctrl.compute_repair_rate(&ests[i], encoder.window_size()));
        }
        let pure_arq = fec_rate_max <= 0.02;
        let arq_primary =
            pure_arq || (cfg.hint == ProtocolHint::Bulk && cfg.bulk_arq_delta);
        if arq_primary {
            retx_scan!(pure_arq);
        }

        // Send phase: source symbols while a path has window room. Paths are
        // ranked by expected delivery time E_i (paper §5.7, with the
        // geometric retransmit chain). Overflow onto a worse path happens with
        // probability (1 - eps_i): a path's usefulness for source is its
        // delivery probability, so a dead path receives a vanishing (but never
        // zero) share, continuously.
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
            // Source symbols go on the wire immediately; the encoder sees
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
            src_path.push(i);
            retx_round.push(0);
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
                // to r per source symbol (paper §3.3). So the debt accumulates
                // at the flat aggregate rate — not at τ(t) with a global
                // ever-growing t, which decays to zero and starves repair
                // generation. Corrections share the path's wire budget with
                // source and preempt new source when due.
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
                    // Each path protects its share of the source stream —
                    // accruing the full rate on every path would double the
                    // correction budget on multipath.
                    debt[i] += rate * batch_src[i] as f64;
                    batch_src[i] = 0;
                }
            }
        }

        // P4b — completion-tail FEC: a transfer's completion is send duration
        // + recovery of the last window's losses. Mid-transfer losses recover
        // in parallel with ongoing sends; tail losses cost ~1.5 RTT of serial
        // ARQ each round. At end-of-stream, burst n_tail repairs covering the
        // final window: cost r_tail × W symbols (negligible), saving
        // P(≥1 tail loss) × ~1.5 RTT. r_tail from the exact transfer-matrix
        // computation (paper §4.7): smallest r with P_fail ≤ 0.05.
        //
        // Not under the Bulk χ glide (P6, paper §4.6): the ramp already raised
        // r continuously over the final ~1.5 SRTT — the one-shot burst is the
        // ramp's limiting case, and firing both would double-pay the tail
        // budget. Non-Bulk hints keep the burst.
        if cfg.tail_fec && !chi_active && !tail_flushed && sent == N_SYMBOLS {
            tail_flushed = true;
            let ge = ests[0].ge_estimator();
            // p_gb/q_bg = 0 are no-data sentinels (decayed counters on very
            // clean channels), not measurements — the transfer-matrix DP
            // would see infinite bursts and return its search ceiling.
            let r_tail = if ge.is_valid() && ge.p_gb() > 0.0 && ge.p_bg() > 0.0 {
                compute_r_star_exact(ge.p_gb(), ge.p_bg(), ENC_WINDOW as usize, 0.05)
            } else {
                let eps = ests[0].loss_rate().max(1e-3);
                compute_r_star_exact(eps, 0.5, ENC_WINDOW as usize, 0.05)
            };
            // Phase 1: repairs over the PRE-flush (lagged) window — this is
            // the only coverage the lag segment [N − lag − W, N − lag) will
            // ever get, since the flush below moves the window past it.
            let n_pre = ((r_tail * enc_lag as f64).ceil() as u32).min(8);
            if encoder.window_size() > 0 {
                for _ in 0..n_pre {
                    tail_queue.push_back(encoder.generate_repair());
                }
            }
            // The encoder trails the send stream by enc_lag; drain the lag
            // queue so repairs cover the final window.
            while let Some(d) = enc_queue.pop_front() {
                let es = encoder.add_source(&d);
                if es.block_id >= ENC_WINDOW {
                    encoder.advance(es.block_id - (ENC_WINDOW - 1));
                }
            }
            // Phase 2: repairs over the final window [N − W, N).
            let n_tail = ((r_tail * ENC_WINDOW as f64).ceil() as u32).min(24);
            if encoder.window_size() > 0 {
                for _ in 0..n_tail {
                    tail_queue.push_back(encoder.generate_repair());
                }
            }
            // Drained under pacing tokens on the best-scoring path (spread,
            // not a synchronized burst) — see the drain loop above.
            let mut best = 0usize;
            for i in 1..n_paths {
                if path_score(srtt[i], eps_sel!(i)) < path_score(srtt[best], eps_sel!(best)) {
                    best = i;
                }
            }
            tail_best = best;
            if std::env::var("RP_GATE_DEBUG").is_ok() {
                println!(
                    "  [tail] t={:.1}ms r_tail={:.3} n_pre={} n_tail={} best={} ge_valid={}",
                    now.duration_since(t0).as_secs_f64() * 1000.0,
                    r_tail, n_pre, n_tail, best, ge.is_valid()
                );
            }
        }

        // Path probing: one repair symbol per path per RTT. Probing with
        // repairs is free information — a repair is never wasted (paper
        // §3.4) — and its loss/arrival feedback is what lets a recovered path
        // come back into rotation.
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
                rtt_floor[i] = rtt_floor[i].min(sample);
                if i == 0 {
                    if let Some((_, end)) = cfg.outage {
                        if path0_recovery.is_none() && pkt.delivery_time >= t0 + end {
                            path0_recovery = Some(pkt.delivery_time);
                        }
                    }
                }
                if !pkt.symbol.is_repair {
                    max_arr_src_on_path[i] = max_arr_src_on_path[i].max(pkt.symbol.block_id);
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
        // Estimator feedback once per RTT per path (a batch = one ACK
        // feedback cycle, paper §2.6). Tiny per-10-symbol updates keep the
        // BOCD posterior artificially wide; RTT cadence matches the
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
                // min sample rises above the floor, back off. Continuous
                // oscillation, no phases, no loss reaction — channel loss is
                // FEC's job, not CC's (paper §8.1).
                // P2: with estimated_floor the propagation floor is the
                // running min RTT sample, not ground truth. The estimate is
                // slightly lower than rtt() (a min sample carries ~zero
                // jitter while rtt() includes the jitter term), so the
                // effective queue target tightens a little — honest, not
                // compensated.
                let base = if cfg.estimated_floor && rtt_floor[i].is_finite() {
                    rtt_floor[i]
                } else {
                    paths[i].rtt().as_secs_f64()
                };
                //
                // P1 (paper §8.2): the protocol hint sets the queue target,
                // not just the FEC rate. At high utilization the standing
                // queue sits at this target, so Realtime keeps it near-empty
                // while Bulk trades a deeper queue for utilization. A 3-level
                // mapping here; the continuous δ-based mapping is §8.2's.
                //
                // Calibration (with P2's estimated floor): `base` is the
                // jitter-free min-sample floor (~2×one_way), so the Realtime
                // multiplier sits just above 1: 1.08 = floor + ~0.8 ms of
                // queue on C2.
                let queue_mult = if cfg.hint_delay_target {
                    match cfg.hint {
                        ProtocolHint::Realtime => 1.08,
                        ProtocolHint::Auto => 1.125,
                        ProtocolHint::Bulk => 1.25,
                    }
                } else {
                    1.125
                };
                if min_rtt_win[i].is_finite() {
                    let cap = paths[i].bdp_cwnd() as f64 * 2.0;
                    if min_rtt_win[i] > base * queue_mult {
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
                decode_time[seq as usize] = Some(now);
                if let Some(lt) = loss_time.get(&seq) {
                    hole_fill_ms.push(now.duration_since(*lt).as_secs_f64() * 1000.0);
                }
                // Goodput buckets count decode-level delivery: the tunnel
                // forwards packets; ordering is a per-flow latency concern
                // (measured via the reorder buffer), not a throughput one.
                let b = (now.duration_since(t0).as_nanos() / BUCKET.as_nanos()) as usize;
                if buckets.len() <= b {
                    buckets.resize(b + 1, 0);
                }
                buckets[b] += 1;
            }
            for (rseq, _, _) in reorder.push_with_time(seq, data, now) {
                if recovered.insert(rseq) {
                    let lat = now.duration_since(encode_time[rseq as usize]);
                    lat_causes.push((lat.as_secs_f64() * 1000.0, 0, rseq));
                    latencies.push(lat.as_secs_f64() * 1000.0);
                    last_recovery = now;
                }
            }
        }
        for (rseq, _, _) in reorder.drain_expired(now) {
            if recovered.insert(rseq) {
                let lat = now.duration_since(encode_time[rseq as usize]);
                lat_causes.push((lat.as_secs_f64() * 1000.0, 1, rseq));
                latencies.push(lat.as_secs_f64() * 1000.0);
                last_recovery = now;
            }
        }

        // FEC-primary operating point: the retransmit backstop runs after
        // the send phase, on whatever tokens the send left (see the budget
        // coordination note above).
        if !arq_primary {
            retx_scan!(false);
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
        let mut by_recovery: Vec<(f64, f64, u8, u64)> = lat_causes
            .iter()
            .map(|&(l, c, seq)| {
                let enc_ms = encode_time[seq as usize].duration_since(t0).as_secs_f64() * 1000.0;
                (enc_ms + l, l, c, seq)
            })
            .collect();
        by_recovery.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        for (rt, l, c, seq) in by_recovery.iter().take(10) {
            let dec_ms = decode_time[*seq as usize]
                .map(|d| d.duration_since(t0).as_secs_f64() * 1000.0)
                .unwrap_or(-1.0);
            println!(
                "  [debug] recovered_at={rt:.1}ms decoded_at={dec_ms:.1}ms lat={l:.1}ms cause={} seq={seq} path={}",
                if *c == 1 { "drain" } else { "inorder" },
                src_path[*seq as usize]
            );
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
        let f = run_fec(fec_paths, seed, &cfg(hint));
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
