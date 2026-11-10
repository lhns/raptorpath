//! In-process quinn-proto client/server simulator on a synthetic clock
//! (status §48, paper §8.4 "The substrate's window law, fixed").
//!
//! Real connections: a quinn-proto `Endpoint` pair built with the engine's own
//! config builders (`QuicTransport::{generate_self_signed_config,
//! make_client_config}`, MTU floor 1350), so both controllers see quinn's real
//! `RttEstimator`, ACK framing and pacer. Time is a synthetic `Instant`
//! timeline (`base + t`); nothing sleeps.
//!
//! The link is netem's release rule as stated in status §46: a packet sent at
//! t leaves at max(t + delay, t_tail) + t_pkt (in order; jitter becomes
//! head-of-line delay), and the qdisc holds at most `limit` packets.
//!
//! The client endpoint polls every 50 µs (`RECV_TIME_BOUND`). A normal poll
//! stamped τ hands every datagram that arrived by τ to the connection. A
//! *stale* poll (the §47 mechanism) drains for L more: datagrams arriving in
//! (τ, τ + L] are processed with the stamp τ, while the controller's injected
//! clock reads τ + L, the true time the drain ends.

use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use bytes::{Bytes, BytesMut};
use quinn_proto::congestion::{Bbr, BbrConfig, ControllerFactory};
use quinn_proto::{
    ClientConfig, Connection, ConnectionHandle, DatagramEvent, Endpoint, EndpointConfig, Event,
    ServerConfig,
};

use super::bbr_rs::{qcc_of, BbrRs, BbrRsConfig, Qcc};
use super::quic::QuicTransport;

const TICK: Duration = Duration::from_micros(50);
const MS: Duration = Duration::from_millis(1);
const DGRAM: usize = 1279;
const SEND_BUF: u64 = 4 * 1024 * 1024;

type CcFactory = Arc<dyn ControllerFactory + Send + Sync + 'static>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cc {
    BbrRs,
    QuinnBbr,
}

// ── the link ────────────────────────────────────────────────────────────

struct Pkt {
    data: Vec<u8>,
    /// Wire send time (sim time).
    wire_sent: Duration,
    /// Server -> client only: the latest client wire-send time the server had
    /// received when it built this datagram (an upper bound on the send time
    /// of the packet it acks).
    acks_up_to: Duration,
}

struct Netem {
    rate: f64,
    limit: usize,
    delay: Arc<dyn Fn(Duration) -> Duration + Send + Sync>,
    jitter: Duration,
    /// In flight (queued or propagating), in leave order: (arrival, packet).
    q: VecDeque<(Duration, Pkt)>,
    /// The previous packet's leave time (netem's `t_tail`).
    tail: Duration,
    rng: u64,
    tail_drops: u64,
}

impl Netem {
    fn new(
        rate: f64,
        limit: usize,
        delay: Arc<dyn Fn(Duration) -> Duration + Send + Sync>,
        jitter: Duration,
        seed: u64,
    ) -> Self {
        Self {
            rate,
            limit,
            delay,
            jitter,
            q: VecDeque::new(),
            tail: Duration::ZERO,
            rng: seed,
            tail_drops: 0,
        }
    }

    fn rand01(&mut self) -> f64 {
        // splitmix64
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Netem's release rule (status §46): a packet sent at t leaves at
    /// max(t + delay, t_tail) + t_pkt. Order is kept; jitter shows as
    /// head-of-line delay. The qdisc holds at most `limit` packets.
    fn enqueue(&mut self, t: Duration, pkt: Pkt) {
        if self.q.len() >= self.limit && self.q.front().is_some_and(|f| f.0 > t) {
            self.tail_drops += 1;
            return;
        }
        let mut d = (self.delay)(t);
        if !self.jitter.is_zero() {
            let u = self.rand01();
            d = d.saturating_sub(self.jitter) + self.jitter.mul_f64(2.0 * u);
        }
        let ser = Duration::from_secs_f64(pkt.data.len() as f64 / self.rate);
        let leave = (t + d).max(self.tail) + ser;
        self.tail = leave;
        self.q.push_back((leave, pkt));
    }

    /// Datagrams that have arrived by `t`, with their arrival times.
    fn take_arrived(&mut self, t: Duration) -> Vec<(Duration, Pkt)> {
        let mut v = Vec::new();
        while self.q.front().is_some_and(|f| f.0 <= t) {
            v.push(self.q.pop_front().unwrap());
        }
        v
    }
}

// ── one endpoint + its connection ───────────────────────────────────────

struct Peer {
    ep: Endpoint,
    ch: Option<ConnectionHandle>,
    conn: Option<Connection>,
    remote: SocketAddr,
    buf: Vec<u8>,
    connected: bool,
}

impl Peer {
    fn new(server: Option<ServerConfig>, remote: SocketAddr, seed: u8) -> Self {
        let ep = Endpoint::new(
            Arc::new(EndpointConfig::default()),
            server.map(Arc::new),
            false,
            Some([seed; 32]),
        );
        Self { ep, ch: None, conn: None, remote, buf: Vec::new(), connected: false }
    }

    /// Hand one datagram to the endpoint with the stamp `now`.
    fn rx(&mut self, now: Instant, data: &[u8], out: &mut Vec<Vec<u8>>) {
        self.buf.clear();
        match self.ep.handle(now, self.remote, None, None, BytesMut::from(data), &mut self.buf) {
            Some(DatagramEvent::ConnectionEvent(_, ev)) => {
                if let Some(c) = self.conn.as_mut() {
                    c.handle_event(ev);
                }
            }
            Some(DatagramEvent::NewConnection(incoming)) => {
                self.buf.clear();
                let (ch, conn) = self
                    .ep
                    .accept(incoming, now, &mut self.buf, None)
                    .unwrap_or_else(|e| panic!("accept: {:?}", e.cause));
                self.ch = Some(ch);
                self.conn = Some(conn);
            }
            Some(DatagramEvent::Response(t)) => out.push(self.buf[..t.size].to_vec()),
            None => {}
        }
    }

    /// Timers, endpoint/app events, then every transmit the connection allows.
    fn step(&mut self, now: Instant) {
        let Some(conn) = self.conn.as_mut() else { return };
        for _ in 0..4 {
            match conn.poll_timeout() {
                Some(t) if t <= now => conn.handle_timeout(now),
                _ => break,
            }
        }
        while let Some(ev) = conn.poll_endpoint_events() {
            if let Some(ce) = self.ep.handle_event(self.ch.unwrap(), ev) {
                conn.handle_event(ce);
            }
        }
        while let Some(e) = conn.poll() {
            match e {
                Event::Connected => self.connected = true,
                Event::ConnectionLost { reason } => panic!("connection lost: {reason}"),
                Event::DatagramReceived => while conn.datagrams().recv().is_some() {},
                _ => {}
            }
        }
    }

    fn transmit(&mut self, now: Instant, out: &mut Vec<Vec<u8>>) {
        let Some(conn) = self.conn.as_mut() else { return };
        loop {
            self.buf.clear();
            match conn.poll_transmit(now, 1, &mut self.buf) {
                Some(t) => out.push(self.buf[..t.size].to_vec()),
                None => break,
            }
        }
    }
}

// ── the scenario ────────────────────────────────────────────────────────

#[derive(Clone)]
struct Sc {
    rate: f64,
    limit: usize,
    delay: Arc<dyn Fn(Duration) -> Duration + Send + Sync>,
    jitter: Duration,
    dur: Duration,
    /// Phase A (bulk, >= 1 MB always queued) ends here; afterwards the source
    /// supplies `supply` bytes/s in 1 ms chunks (the store-braked regime).
    bulk_until: Duration,
    supply: f64,
    /// Drop `n` consecutive client datagrams from this wire time on.
    loss_burst: Option<(Duration, usize)>,
    /// Stale-stamp events: from `.0`, every `.1`, `.2` of them.
    stale: Option<(Duration, Duration, usize)>,
    /// End the run this long after the first ProbeRtt exit (`dur` is the cap).
    stop_after_probe_rtt: Option<Duration>,
    /// Poll grid of both endpoints (`TICK` = RECV_TIME_BOUND, 50 us).
    tick: Duration,
    cc: Cc,
}

/// L_j of the stale events (all below the 2 ms RTT of the c1 link).
const STALE_L_US: [u64; 7] = [500, 800, 1100, 1400, 1700, 1900, 1950];

// Phase of the stale events (+0.5 ms past the 1 ms source-chunk edges). The
// source sends in 1 ms chunks, so every send stamp sits on a chunk edge. An
// event ON an edge can then never read a quinn sample below one chunk spacing
// (the latest stamp whose ACK lands in the drain is the previous edge): a
// (0, 1 ms) minimum is unreachable by construction. Events at a phase inside
// the chunk can. The L_j grid is untouched.

#[derive(Clone, Debug)]
struct Sample {
    t: Duration,
    w: u64,
    qcc: Option<Qcc>,
}

#[derive(Debug, Default)]
struct Run {
    samples: Vec<Sample>,
    /// (t, quinn Bbr `min_rtt` from its Debug), every 10 ms (quinn arm).
    quinn_min_rtt: Vec<(Duration, Duration)>,
    /// (t, "mode / is_at_full_bandwidth" from quinn's Debug), every 10 ms.
    quinn_state: Vec<(Duration, String)>,
    /// quinn's whole Bbr Debug at the end of the run (evidence for the pins).
    quinn_final_dbg: String,
    /// quinn's whole Bbr Debug every 10 ms (characterisation dump).
    quinn_dbgs: Vec<(Duration, String)>,
    stale_events: usize,
    /// ACK-bearing datagrams that arrived after a stale stamp and whose
    /// stamp-based sample reads below the true RTT floor and above 0.
    witness: u64,
    /// Per stale event: (stamp τ, L, smallest stamp-based sample among the
    /// ACK-bearing datagrams that arrived in (τ, τ + L]).
    stale_min_sample: Vec<(Duration, Duration, Option<Duration>)>,
    burst_drops: u64,
    tail_drops: u64,
    modes: String,
    /// First ProbeRtt entry / exit (sampled at 1 ms).
    r_enter: Option<Duration>,
    r_exit: Option<Duration>,
    /// Host cost (ns) of one `[QCC]` read (`congestion_state()` clone + gauge),
    /// taken every 1 ms mid-run on the loaded connection.
    qcc_ns: Vec<u64>,
    /// The sampler's outstanding-record count (what the clone copies), every 10 ms.
    tracked: Vec<usize>,
}

fn parse_dur(s: &str) -> Duration {
    let split = s.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let v: f64 = num.parse().unwrap_or_else(|_| panic!("duration {s:?}"));
    let scale = match unit {
        "ns" => 1e-9,
        "µs" | "us" => 1e-6,
        "ms" => 1e-3,
        "s" => 1.0,
        u => panic!("duration unit {u:?} in {s:?}"),
    };
    Duration::from_secs_f64(v * scale)
}

fn dbg_field<'a>(dbg: &'a str, key: &str) -> &'a str {
    let i = dbg.find(key).unwrap_or_else(|| panic!("{key} in Debug")) + key.len();
    let rest = &dbg[i..];
    &rest[..rest.find([',', ' ', '}']).unwrap_or(rest.len())]
}

fn quinn_bbr_min_rtt(dbg: &str) -> Duration {
    let i = dbg.find("min_rtt: ").expect("min_rtt in quinn Bbr Debug") + "min_rtt: ".len();
    let rest = &dbg[i..];
    let end = rest.find([',', ' ', '}']).unwrap_or(rest.len());
    parse_dur(&rest[..end])
}

fn run(sc: &Sc) -> Run {
    // quinn-proto's rustls config needs a process-level provider (ring)
    let _ = rustls::crypto::ring::default_provider().install_default();
    let base = Instant::now();
    let clock = Arc::new(AtomicU64::new(0));
    let cc: CcFactory = match sc.cc {
        Cc::BbrRs => {
            let c = clock.clone();
            Arc::new(BbrRsConfig::with_clock(move || {
                base + Duration::from_nanos(c.load(Ordering::Relaxed))
            }))
        }
        Cc::QuinnBbr => Arc::new(BbrConfig::default()),
    };
    let (server_cfg, _) = QuicTransport::generate_self_signed_config(None).unwrap();
    let client_cfg: ClientConfig = QuicTransport::make_client_config(None, Some(cc));
    let caddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 40000);
    let saddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)), 4433);
    let mut client = Peer::new(None, saddr, 1);
    let mut server = Peer::new(Some(server_cfg), caddr, 2);
    let (ch, conn) = client.ep.connect(base, client_cfg, saddr, "raptorpath").expect("connect");
    client.ch = Some(ch);
    client.conn = Some(conn);

    let mut c2s = Netem::new(sc.rate, sc.limit, sc.delay.clone(), sc.jitter, 11);
    let mut s2c = Netem::new(sc.rate, sc.limit, sc.delay.clone(), sc.jitter, 22);
    let mut r = Run::default();
    let mut server_max_seen = Duration::ZERO;
    let mut stale: Option<(Duration, Duration)> = None;
    let mut stale_done = 0usize;
    let mut burst_left = sc.loss_burst.map_or(0, |b| b.1);
    let mut credit = 0.0f64;
    let mut out: Vec<Vec<u8>> = Vec::new();
    let payload = Bytes::from(vec![0xA5u8; DGRAM]);

    let mut k: u64 = 0;
    loop {
        let tau = sc.tick * k as u32;
        k += 1;
        if tau > sc.dur {
            break;
        }
        if let (Some(extra), Some(x)) = (sc.stop_after_probe_rtt, r.r_exit) {
            if tau > x + extra {
                break;
            }
        }
        let now = base + tau;

        // ── server: exact arrival stamps ──
        for (arr, pkt) in c2s.take_arrived(tau) {
            server_max_seen = server_max_seen.max(pkt.wire_sent);
            server.rx(base + arr, &pkt.data, &mut out);
        }
        server.step(now);
        server.transmit(now, &mut out);
        for d in out.drain(..) {
            s2c.enqueue(tau, Pkt { data: d, wire_sent: tau, acks_up_to: server_max_seen });
        }

        // ── client ──
        // A stale poll (stamp tau_s) drains until tau_s + L: the datagrams that
        // arrive in (tau_s, tau_s + L] are handled together, at the end of the
        // drain, with the stamp tau_s while the clock reads the drain's end.
        // The client does nothing else meanwhile (the driver is busy).
        if stale.is_none() {
            if let Some((t0, every, n)) = sc.stale {
                let due = t0 + every * stale_done as u32;
                if stale_done < n && tau >= due {
                    let l = Duration::from_micros(STALE_L_US[stale_done % STALE_L_US.len()]);
                    stale = Some((tau, tau + l));
                    stale_done += 1;
                    r.stale_events += 1;
                }
            }
        }
        if matches!(stale, Some((_, end)) if tau < end) {
            continue;
        }
        let mut cout: Vec<Vec<u8>> = Vec::new();
        if let Some((tau_s, _)) = stale.take() {
            clock.store(tau.as_nanos() as u64, Ordering::Relaxed);
            let floor = 2 * (sc.delay)(tau_s);
            let mut min_sample: Option<Duration> = None;
            for (arr, pkt) in s2c.take_arrived(tau) {
                if arr > tau_s {
                    let sample = tau_s.saturating_sub(pkt.acks_up_to);
                    if !sample.is_zero() && sample < floor {
                        r.witness += 1;
                        min_sample = Some(min_sample.map_or(sample, |m| m.min(sample)));
                    }
                }
                client.rx(base + tau_s, &pkt.data, &mut cout);
            }
            r.stale_min_sample.push((tau_s, tau - tau_s, min_sample));
            client.step(now);
        } else {
            clock.store(tau.as_nanos() as u64, Ordering::Relaxed);
            for (_, pkt) in s2c.take_arrived(tau) {
                client.rx(now, &pkt.data, &mut cout);
            }
            client.step(now);
        }
        {
            // the application source
            if client.connected {
                let conn = client.conn.as_mut().unwrap();
                let max = conn.datagrams().max_size().unwrap_or(DGRAM).min(DGRAM);
                let pl = payload.slice(..max);
                if tau < sc.bulk_until {
                    while conn.datagrams().send_buffer_space() as u64 > SEND_BUF - 1_500_000 {
                        if conn.datagrams().send(pl.clone(), true).is_err() {
                            break;
                        }
                    }
                } else if (tau + sc.tick).as_nanos() % MS.as_nanos() == 0 {
                    credit += sc.supply * MS.as_secs_f64();
                    while credit >= max as f64 {
                        credit -= max as f64;
                        let _ = conn.datagrams().send(pl.clone(), true);
                    }
                }
            }
            client.transmit(now, &mut cout);
        }
        for d in cout.drain(..) {
            if let Some((t0, _)) = sc.loss_burst {
                if tau >= t0 && burst_left > 0 {
                    burst_left -= 1;
                    r.burst_drops += 1;
                    continue;
                }
            }
            c2s.enqueue(tau, Pkt { data: d, wire_sent: tau, acks_up_to: Duration::ZERO });
        }

        // ── sampling ──
        if client.connected && tau.as_nanos() % MS.as_nanos() == 0 {
            let conn = client.conn.as_ref().unwrap();
            let w = conn.stats().path.cwnd;
            let qcc = match sc.cc {
                Cc::BbrRs => {
                    // the engine's `[QCC]` read, timed: clone + downcast + gauge
                    let t0 = std::time::Instant::now();
                    let q = qcc_of(conn.congestion_state().clone_box());
                    r.qcc_ns.push(t0.elapsed().as_nanos() as u64);
                    if tau.as_nanos() % (10 * MS).as_nanos() == 0 {
                        let b = conn.congestion_state().clone_box().into_any().downcast::<BbrRs>().unwrap();
                        r.tracked.push(b.tracked_records());
                    }
                    q
                }
                Cc::QuinnBbr => {
                    // another controller: the engine prints no [QCC] for it
                    assert!(qcc_of(conn.congestion_state().clone_box()).is_none());
                    None
                }
            };
            if let Some(q) = &qcc {
                if !r.modes.contains(q.mode) {
                    r.modes.push(q.mode);
                }
                if q.mode == 'R' && r.r_enter.is_none() {
                    r.r_enter = Some(tau);
                }
                if q.mode != 'R' && r.r_enter.is_some() && r.r_exit.is_none() {
                    r.r_exit = Some(tau);
                }
            }
            r.samples.push(Sample { t: tau, w, qcc });
            if sc.cc == Cc::QuinnBbr && tau.as_nanos() % (10 * MS).as_nanos() == 0 {
                let b = conn.congestion_state().clone_box().into_any().downcast::<Bbr>().unwrap();
                let dbg = format!("{:?}", *b);
                r.quinn_min_rtt.push((tau, quinn_bbr_min_rtt(&dbg)));
                r.quinn_dbgs.push((tau, dbg.clone()));
                r.quinn_state.push((tau, format!("{} full={}", dbg_field(&dbg, "mode: "), dbg_field(&dbg, "is_at_full_bandwidth: "))));
            }
        }
    }
    r.tail_drops = c2s.tail_drops + s2c.tail_drops;
    if sc.cc == Cc::QuinnBbr {
        let b = client.conn.as_ref().unwrap().congestion_state().clone_box().into_any().downcast::<Bbr>().unwrap();
        r.quinn_final_dbg = format!("{:?}", *b);
    }
    r
}

// ── the scenarios ───────────────────────────────────────────────────────

const C1_RATE: f64 = 125_000_000.0; // 1 Gbit/s
const BDP_TRUE: u64 = 250_000; // 1 Gbit/s x 2 ms

fn c1(cc: Cc, stale: bool) -> Sc {
    c1_ph(cc, stale, 1_200_500)
}

/// The c1 scenario with the first stale event at `t0_us` (phase sweep).
fn c1_ph(cc: Cc, stale: bool, t0_us: u64) -> Sc {
    Sc {
        rate: C1_RATE,
        limit: 1000,
        delay: Arc::new(|_| MS),
        jitter: Duration::ZERO,
        dur: Duration::from_secs(3),
        bulk_until: Duration::from_secs(1),
        supply: 0.9 * C1_RATE,
        loss_burst: Some((Duration::from_millis(1800), 28)),
        stale: stale.then(|| (Duration::from_micros(t0_us), Duration::from_millis(100), 18)),
        stop_after_probe_rtt: None,
        tick: TICK,
        cc,
    }
}

fn c1_run(cc: Cc, stale: bool) -> &'static Run {
    static RUNS: [OnceLock<Run>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    let slot = match (cc, stale) {
        (Cc::BbrRs, true) => 0,
        (Cc::QuinnBbr, true) => 1,
        (Cc::QuinnBbr, false) => 2,
        (Cc::BbrRs, false) => unreachable!("not a registered scenario"),
    };
    RUNS[slot].get_or_init(|| run(&c1(cc, stale)))
}

fn share_below(r: &Run, from: Duration, to: Duration, bound: u64) -> f64 {
    let v: Vec<_> = r.samples.iter().filter(|s| s.t >= from && s.t <= to).collect();
    assert!(!v.is_empty(), "no samples in {from:?}..{to:?}");
    v.iter().filter(|s| s.w < bound).count() as f64 / v.len() as f64
}

fn min_w(r: &Run, from: Duration, to: Duration) -> u64 {
    r.samples.iter().filter(|s| s.t >= from && s.t <= to).map(|s| s.w).min().unwrap()
}

/// The harness witnesses shared by tests 1, 2, 4 and 5: the loss burst was
/// applied, the stale events fired, and the stale stamp reached ACKs (the
/// mechanism witness: zero means `VOID-SIM`, a harness fault).
fn assert_c1_witnesses(r: &Run) {
    assert_eq!(r.burst_drops, 28, "the link dropped exactly 28 client datagrams");
    assert_eq!(r.stale_events, 18, "18 stale events in phase B");
    assert!(
        r.witness > 0,
        "VOID-SIM: no ACK-bearing datagram arrived after a stale stamp with a sample in (0, RTT)"
    );
}

/// Status §48 test 1.
#[test]
fn bbr_rs_holds_the_pipe_after_a_loss_burst_under_stale_stamps() {
    let r = c1_run(Cc::BbrRs, true);
    assert_c1_witnesses(r);
    let at = |t: Duration| r.samples.iter().find(|s| s.t >= t).unwrap();
    assert_ne!(at(Duration::from_millis(1200)).qcc.unwrap().mode, 'S', "left Startup in phase A");
    let (a, b) = (Duration::from_millis(1300), Duration::from_secs(3));
    let share = share_below(r, a, b, BDP_TRUE);
    let min = min_w(r, a, b);
    let m = r.samples.iter().filter(|s| s.t >= a).map(|s| s.qcc.unwrap().rt_us).min().unwrap();
    println!(
        "[cc_sim t1] share(W<BDP)={share:.4} minW={min} modes={} min m̂={m}us witness={} tail_drops={}",
        r.modes, r.witness, r.tail_drops
    );
    assert!(share <= 0.05, "share(W < BDP_true) = {share:.4} over [1.3, 3.0] s");
    assert!(min >= BDP_TRUE / 2, "min W = {min} < 0.5 x BDP_true over [1.3, 3.0] s");
}

/// Status §48 test 2.
#[test]
fn bbr_rs_min_rtt_never_reads_below_the_path() {
    let r = c1_run(Cc::BbrRs, true);
    assert_c1_witnesses(r);
    let first_event = Duration::from_millis(1200);
    let low = r
        .samples
        .iter()
        .filter(|s| s.t >= first_event)
        .map(|s| (s.qcc.unwrap().rt_us, s.t))
        .min()
        .unwrap();
    assert!(low.0 >= 2000, "m̂ read {} us at t={:?}: below the 2.000 ms path RTT", low.0, low.1);
}

/// Status §48 test 3: the model follows a path RTT rise (the lifetime min
/// would hold the old, lower RTT forever).
#[test]
fn bbr_rs_min_rtt_follows_a_path_rtt_rise() {
    let step = Duration::from_secs(2);
    let sc = Sc {
        rate: 1_250_000.0, // 10 Mbit/s
        limit: 1000,
        delay: Arc::new(move |t| if t < step { 5 * MS } else { 20 * MS }),
        jitter: Duration::ZERO,
        dur: Duration::from_secs(14),
        bulk_until: Duration::from_secs(100),
        supply: 0.0,
        loss_burst: None,
        stale: None,
        stop_after_probe_rtt: None,
        tick: TICK,
        cc: Cc::BbrRs,
    };
    let r = run(&sc);
    let at = |t: Duration| r.samples.iter().find(|s| s.t >= t).unwrap();
    let m = at(Duration::from_millis(12_500)).qcc.unwrap().rt_us;
    let share = share_below(&r, Duration::from_secs(13), Duration::from_secs(14), 50_000);
    println!("[cc_sim t3] m̂(12.5s)={m}us share(W<50kB)[13,14]={share:.4} modes={}", r.modes);
    assert!(m >= 36_000, "m̂ = {m} us at 12.5 s, expected >= 36 ms after the RTT rise to 40 ms");
    assert_eq!(share, 0.0, "W < 50 kB at some sample in [13, 14] s");
}

/// Status §48 test 4: pins quinn's own BBR defect (if a quinn bump turns it
/// red, ask again).
///
/// STOP-NOT-REPRODUCED (status §48 gate rule): the mechanism is realised
/// (witness > 0; test 5 green: quinn's `min_rtt` latches at 550 us on a 2.1 ms
/// path) but quinn's window does not collapse in this simulator. Its bandwidth
/// filter reads 26 MB/s (0.2 x the link), so its ack-aggregation term grows
/// without bound against that low model (max ack height 253 MB) and the
/// window stays at 253 MB, far above the BDP. Ignored until the user rules;
/// run with `--ignored` to see the failure and the dumped state.
#[test]
#[ignore = "STOP-NOT-REPRODUCED: quinn's window does not collapse in the simulator (ack-height term)"]
fn quinn_bbr_window_collapses_under_stale_stamps() {
    let r = c1_run(Cc::QuinnBbr, true);
    assert_c1_witnesses(r);
    let base = c1_run(Cc::QuinnBbr, false);
    let (a, b) = (Duration::from_millis(1300), Duration::from_secs(3));
    let share = share_below(r, a, b, BDP_TRUE);
    let share_no_stale = share_below(base, a, b, BDP_TRUE);
    let last = r.quinn_min_rtt.last().unwrap();
    let last_no = base.quinn_min_rtt.last().unwrap();
    println!(
        "[cc_sim t4] quinn share(W<BDP): stale={share:.4} no-stale={share_no_stale:.4}; \
         latched min_rtt: stale={:?} no-stale={:?}; minW stale={} no-stale={}
  quinn state (stale run) at 0.5/1.0/1.2/2.0/3.0 s: {:?}
  (no-stale run): {:?}",
        last.1,
        last_no.1,
        min_w(r, a, b),
        min_w(base, a, b),
        [500u64, 1000, 1200, 2000, 3000].map(|ms| r.quinn_state.iter().find(|x| x.0 >= Duration::from_millis(ms)).map(|x| x.1.clone())),
        [500u64, 1000, 1200, 2000, 3000].map(|ms| base.quinn_state.iter().find(|x| x.0 >= Duration::from_millis(ms)).map(|x| x.1.clone())),
    );
    assert!(
        share > 0.5,
        "share(W < BDP_true) = {share:.4}, expected > 0.5; quinn Bbr at the end of the stale run: {}",
        r.quinn_final_dbg
    );
}

/// Status §48 test 5: the mechanism realised in quinn's own estimator.
#[test]
fn quinn_bbr_min_rtt_under_reads_under_stale_stamps() {
    let r = c1_run(Cc::QuinnBbr, true);
    assert_c1_witnesses(r);
    let (t, m) = *r.quinn_min_rtt.last().unwrap();
    assert!(
        !m.is_zero() && m < MS,
        "quinn min_rtt = {m:?} at {t:?}, expected in (0, 1 ms); per-event (tau, L, min stamp-based sample): {:?}",
        r.stale_min_sample
    );
}

/// Amendment 2: c2's jitter (5 ms +- 3 ms per direction, netem release rule,
/// 100 Mbit/s) across one ProbeRtt. By the loop map, once 2 m̂ < R̄0 the
/// estimate settles at A / (R̄0 - 2 m̂): if b̂ decays below the link rate
/// here, the law needs a jitter allowance.
///
/// The run ends 3 s after the first ProbeRtt exit (cap 40 s). Tolerances
/// (this test's own choice): b̂ >= 0.9 L at every sample in [3 s, ProbeRtt
/// entry) and in [exit + 1 s, end). The dip inside the 200 ms drain is
/// reported, not asserted: the 10-round max filter spans less than 200 ms.
#[test]
fn bbr_rs_c2_jitter_keeps_bhat_at_the_link_rate_across_probe_rtt() {
    const RATE: f64 = 12_500_000.0; // 100 Mbit/s
    let sc = Sc {
        rate: RATE,
        limit: 1000,
        delay: Arc::new(|_| 5 * MS),
        jitter: 3 * MS,
        dur: Duration::from_secs(40),
        bulk_until: Duration::from_secs(1000),
        supply: 0.0,
        loss_burst: None,
        stale: None,
        stop_after_probe_rtt: Some(Duration::from_secs(3)),
        tick: TICK,
        cc: Cc::BbrRs,
    };
    let r = run(&sc);
    let (enter, exit) = match (r.r_enter, r.r_exit) {
        (Some(a), Some(b)) => (a, b),
        _ => panic!(
            "VOID: no complete ProbeRtt within {:?} (modes {}, entry {:?})",
            r.dur_hint(),
            r.modes,
            r.r_enter
        ),
    };
    let end = r.dur_hint();
    let bw = |from: Duration, to: Duration| -> (u64, u64) {
        let v: Vec<u64> = r
            .samples
            .iter()
            .filter(|s| s.t >= from && s.t < to)
            .map(|s| s.qcc.unwrap().bw)
            .collect();
        match v.iter().min() {
            Some(&lo) => (lo, v[v.len() / 2]),
            None => (u64::MAX, 0), // empty range: no constraint (reported as such)
        }
    };
    let at = |t: Duration| r.samples.iter().find(|s| s.t >= t).or(r.samples.last()).unwrap().qcc.unwrap();
    let f = |x: u64| (x.min(u64::MAX / 2)) as f64 / RATE;
    let s1 = Duration::from_secs(1);
    let (pre_lo, pre_med) = bw(Duration::from_secs(3), enter);
    let (in_lo, _) = bw(enter, exit);
    let (post_lo, post_med) = bw(exit + s1, end + MS);
    println!(
        "[cc_sim c2-jitter] ProbeRtt {enter:?}..{exit:?}; run end {end:?}; modes={}; tail_drops={}
           b̂/L: pre[3s,enter) min/med={:.3}/{:.3}; at enter-1ms={:.3}; in ProbeRtt min={:.3};          at exit={:.3}, +1s={:.3}, +2s={:.3}, +3s={:.3}; post[exit+1s,end) min/med={:.3}/{:.3}
           m̂(us): enter-1ms={} in-ProbeRtt={} exit+3s={}; sr(us) exit+3s={}",
        r.modes,
        r.tail_drops,
        f(pre_lo),
        f(pre_med),
        f(at(enter.saturating_sub(MS)).bw),
        f(in_lo),
        f(at(exit).bw),
        f(at(exit + s1).bw),
        f(at(exit + 2 * s1).bw),
        f(at(end).bw),
        f(post_lo),
        f(post_med),
        at(enter.saturating_sub(MS)).rt_us,
        at(exit.saturating_sub(MS)).rt_us,
        at(end).rt_us,
        at(end).sr_us,
    );
    assert!(f(pre_lo) >= 0.9, "b̂ decayed to {:.3} L before ProbeRtt", f(pre_lo));
    assert!(f(post_lo) >= 0.9, "b̂ decayed to {:.3} L after ProbeRtt", f(post_lo));
}

impl Run {
    fn dur_hint(&self) -> Duration {
        self.samples.last().map_or(Duration::ZERO, |s| s.t)
    }
}

/// Status §48 gauge wiring on a live loaded connection: the `[QCC]` read
/// returns the running controller's model with `qm` in {S, D, B, R} and numeric
/// fields (and, in the run above, `None` for quinn's own BBR), and its host
/// cost is measured: `congestion_state()` clones the controller, sampler record
/// map included, under the connection lock.
#[test]
fn qcc_gauge_reads_the_running_controller_and_its_cost_is_measured() {
    let r = c1_run(Cc::BbrRs, true);
    let at = |ms: u64| r.samples.iter().find(|s| s.t >= Duration::from_millis(ms)).unwrap().qcc.unwrap();
    for s in &r.samples {
        let q = s.qcc.unwrap();
        assert!("SDBR".contains(q.mode), "qm {:?} at {:?}", q.mode, s.t);
    }
    let q = at(1500);
    assert!(q.rt_us >= 2000 && q.sr_us > 0 && q.bw > 10_000_000 && q.quinn_min_us > 0, "{q:?}");
    let tok = q.token(0, 1234, 5, 1000);
    for f in ["p0:qm=", "qrt_us=", "qsr_us=", "qbw=", "qmin_us=", "qcns=1234/5/1000"] {
        assert!(tok.contains(f), "{f} in {tok}");
    }
    let mut ns = r.qcc_ns.clone();
    ns.sort_unstable();
    let mut tr = r.tracked.clone();
    tr.sort_unstable();
    println!(
        "[cc_sim qcc-cost] read ns: n={} p50={} p90={} p99={} max={}; sampler records outstanding: p50={} p99={} max={}",
        ns.len(),
        ns[ns.len() / 2],
        ns[ns.len() * 9 / 10],
        ns[ns.len() * 99 / 100],
        ns[ns.len() - 1],
        tr[tr.len() / 2],
        tr[tr.len() * 99 / 100],
        tr[tr.len() - 1]
    );
}

// ── offline characterisation of quinn's BBR and the stale-phase sweep ───

fn dbg_nested(dbg: &str, key: &str, then: &str) -> u64 {
    let i = dbg.find(key).unwrap_or_else(|| panic!("{key} in Debug"));
    let r = &dbg[i..];
    let j = r.find(then).unwrap() + then.len();
    let r = &r[j..];
    r[..r.find([',', ' ', '}']).unwrap()].parse().unwrap()
}

fn dbg_num(dbg: &str, key: &str) -> u64 {
    dbg_field(dbg, key).parse().unwrap()
}

/// Status §48 follow-up: quinn's BBR over time, stale run and no-stale control.
/// Writes `docs/l1-raw/bbrfix48/quinn-dump.txt`. Columns: t, mode, bandwidth
/// filter, max ack height, pacing rate, cwnd (quinn's own field), W (the
/// window quinn applies, `stats().path.cwnd`), delivered rate (bytes acked per
/// s over the 10 ms), min_rtt.
#[test]
#[ignore = "characterisation dump; writes docs/l1-raw/bbrfix48/quinn-dump.txt"]
fn quinn_characterisation_dump() {
    use std::fmt::Write as _;
    let mut out = String::new();
    for (label, stale) in [("stale", true), ("nostale", false)] {
        let r = c1_run(Cc::QuinnBbr, stale);
        writeln!(out, "## quinn BBR, {label} run (c1: 1 Gbit/s link = 125 MB/s, BDP 250 kB; phase A bulk to 1.0 s, phase B 0.9 L)").unwrap();
        writeln!(out, "# stale events (tau, L, smallest stamp-based sample): {:?}", r.stale_min_sample).unwrap();
        writeln!(out, "# t_ms mode filter_MBs ackheight_B pacing_MBs cwnd_B W_B delivered_MBs min_rtt_us recovery_state recovery_window note").unwrap();
        let (mut prev_acked, mut prev_mode) = (0u64, String::new());
        let (mut dsum_a, mut na, mut dsum_b, mut nb) = (0f64, 0, 0f64, 0);
        for (t, dbg) in &r.quinn_dbgs {
            let acked = dbg_num(dbg, "total_acked: ");
            let deliv = (acked - prev_acked) as f64 / 0.01 / 1e6;
            let mode = dbg_field(dbg, "mode: ").to_string();
            let w = r.samples.iter().find(|s| s.t == *t).map_or(0, |s| s.w);
            let note = if mode == "ProbeRtt" {
                "ProbeRtt"
            } else if prev_mode == "ProbeRtt" {
                "post-ProbeRtt-exit"
            } else {
                ""
            };
            writeln!(
                out,
                "{:>6} {:<8} {:>8.2} {:>11} {:>8.2} {:>11} {:>10} {:>8.2} {:>6} {:<12} {:>9} {}",
                t.as_millis(),
                mode,
                dbg_nested(dbg, "max_filter: MinMax", "value: ") as f64 / 1e6,
                dbg_nested(dbg, "max_ack_height: MinMax", "value: "),
                dbg_num(dbg, "pacing_rate: ") as f64 / 1e6,
                dbg_num(dbg, " cwnd: "),
                w,
                deliv,
                quinn_bbr_min_rtt(dbg).as_micros(),
                dbg_field(dbg, "recovery_state: "),
                dbg_num(dbg, "recovery_window: "),
                note
            )
            .unwrap();
            if t.as_millis() > 500 && t.as_millis() <= 1000 {
                dsum_a += deliv;
                na += 1;
            }
            if t.as_millis() > 1300 {
                dsum_b += deliv;
                nb += 1;
            }
            prev_acked = acked;
            prev_mode = mode;
        }
        writeln!(
            out,
            "# mean delivered: phase A (0.5-1.0 s) {:.1} MB/s; phase B (1.3-3.0 s) {:.1} MB/s (link 125, supply 112.5)\n",
            dsum_a / na as f64,
            dsum_b / nb as f64
        )
        .unwrap();
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/l1-raw/bbrfix48");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("quinn-dump.txt"), &out).unwrap();
    println!("{out}");
}

/// Status §48 follow-up: the stale-event phase swept over one chunk period
/// (first event at 1.2000 + k x 0.1 ms, k = 0..9), verdicts of tests 1, 2, 4,
/// 5 per phase. Prints `SWEEP` lines; `CC_SIM_TREE` labels the tree.
#[test]
#[ignore = "phase sweep; prints SWEEP lines"]
fn stale_phase_sweep() {
    let tree = std::env::var("CC_SIM_TREE").unwrap_or_else(|_| "?".into());
    let (a, b) = (Duration::from_millis(1300), Duration::from_secs(3));
    let v = |ok: bool| if ok { "green" } else { "RED" };
    for k in 0..10u64 {
        let t0 = 1_200_000 + 100 * k;
        let rb = run(&c1_ph(Cc::BbrRs, true, t0));
        let rq = run(&c1_ph(Cc::QuinnBbr, true, t0));
        let share = share_below(&rb, a, b, BDP_TRUE);
        let minw = min_w(&rb, a, b);
        let mmin = rb
            .samples
            .iter()
            .filter(|s| s.t >= Duration::from_millis(1200))
            .map(|s| s.qcc.unwrap().rt_us)
            .min()
            .unwrap();
        let qshare = share_below(&rq, a, b, BDP_TRUE);
        let qmin = rq.quinn_min_rtt.last().unwrap().1;
        println!(
            "SWEEP tree={tree} phase_us={k}00 witness={}/{} | t1={} (share {share:.4}, minW {minw}) | t2={} (min m-hat {mmin} us) | t4={} (quinn share {qshare:.4}) | t5={} (quinn min_rtt {qmin:?})",
            rb.witness,
            rq.witness,
            if rb.witness == 0 { "VOID" } else { v(share <= 0.05 && minw >= BDP_TRUE / 2) },
            v(mmin >= 2000),
            v(qshare > 0.5),
            v(!qmin.is_zero() && qmin < MS),
        );
    }
}

/// Status §48 follow-up: does the quinn-BBR bandwidth filter, and with it the
/// collapse, depend on the simulator's poll grid? Quinn's filter admits one
/// packet per ACK frame (the rest see a zero gap), so it reads about
/// 1 MTU / (gap between frames' stamps): 1312 B / 50 us = 26 MB/s. Runs the
/// quinn run at three poll grids. Prints `TICKVAR` lines.
#[test]
#[ignore = "poll-grid variants of the quinn run; prints TICKVAR lines"]
fn quinn_poll_grid_variants() {
    let (a, b) = (Duration::from_millis(1300), Duration::from_secs(3));
    for tick_us in [50u64, 20, 10] {
        for stale in [true, false] {
            let mut sc = c1(Cc::QuinnBbr, stale);
            sc.tick = Duration::from_micros(tick_us);
            let r = run(&sc);
            let at = |ms: u64| r.quinn_dbgs.iter().find(|x| x.0 >= Duration::from_millis(ms)).unwrap().1.clone();
            let fmt = |ms: u64| {
                let d = at(ms);
                format!(
                    "{ms}ms[{} filter {:.1}MB/s ackh {}B cwnd {}B rec {} recw {} minrtt {}us]",
                    dbg_field(&d, "mode: "),
                    dbg_nested(&d, "max_filter: MinMax", "value: ") as f64 / 1e6,
                    dbg_nested(&d, "max_ack_height: MinMax", "value: "),
                    dbg_num(&d, " cwnd: "),
                    dbg_field(&d, "recovery_state: "),
                    dbg_num(&d, "recovery_window: "),
                    quinn_bbr_min_rtt(&d).as_micros()
                )
            };
            println!(
                "TICKVAR tick={tick_us}us stale={stale} share(W<BDP)={:.4} minW={} {} {} {} {} {}",
                share_below(&r, a, b, BDP_TRUE),
                min_w(&r, a, b),
                fmt(900),
                fmt(1790),
                fmt(1830),
                fmt(2000),
                fmt(2990)
            );
        }
    }
}
