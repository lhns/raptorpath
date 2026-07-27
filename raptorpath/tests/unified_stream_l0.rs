//! Roadmap item 3 (diag/unified-collapse): L0 sustained-stream rung for the
//! unified-realtime c3-1200B STREAM-COLLAPSE class (goal-gate "Unified
//! Decoder" L1 RESULTS battery 1: 3/10 unified reps with p50 in SECONDS at
//! the c3-1200B tail_matrix cell; absent in the stream arm, and legacy-rlc's
//! completed reps).
//!
//! The existing L0 arm (tests/unified_l0.rs) is OBJECT mode — sequential
//! objects with an app-level completion ack — and it never showed the class
//! (20/20 delivered, medians ≤ 0.55 s at the same cell). The L1 collapse is a
//! SUSTAINED-STREAM phenomenon: tools/l1/tail_matrix.sh drives 50 msg/s ×
//! 20 s of 1200-B messages through the tunnel and measures per-message
//! latency; a collapse rep backlogs the WHOLE stream (p50 seconds). This
//! harness reproduces that shape locally: a fixed-rate message stream over
//! the real engine (memory TUNs, real QUIC on 127.0.0.1) under the transport
//! L0 netem shim (`RWM_L0_NETEM`, default c3 — the L1 collapse cell's
//! params), one arm per process.
//!
//! Message = RWM_SL_MSG bytes (default 1200) split into chunks that fit one
//! realtime symbol (512 − 4 framing ⇒ ≤ 508 B packets — mirrors the L1 TUN
//! MTU clamp segmenting the inner TCP stream). Per-message latency = embedded
//! send timestamp → LAST chunk delivered in order at the server (the engine's
//! in-order frontier is the delivery point, as for the inner TCP byte
//! stream). The server acks each completed message (reverse-direction load ≈
//! the inner TCP ack stream). No app-level retransmit: a chunk the engine
//! force-delivers past the reorder horizon and never repairs is a LOST
//! message (counted; the L1 inner TCP would have retransmitted it — at L0 it
//! is a datum, not a stall).
//!
//! Env knobs (beyond every engine RWM_* knob — RWM_UNIFIED, RWM_DIAG,
//! RWM_FDIAG, ...):
//!   RWM_L0_NETEM    shim scenario (default c3 — the collapse cell)
//!   RWM_L0_SEED     GE/jitter RNG seed
//!   RWM_L0_HINT     protocol hint (default realtime)
//!   RWM_L0_BACKEND  explicit fec_backend ("rlc" = the legacy-RLC arm;
//!                   unset = shipped auto-selection: the unified RLC span
//!                   machine; RWM_UNIFIED=0 = the legacy-RLC windowed
//!                   machine (streaming retired 2026-07-28))
//!   RWM_SL_RATE     messages per second (default 50)
//!   RWM_SL_DUR     stream duration seconds (default 20)
//!   RWM_SL_MSG      message size bytes (default 1200)
//!
//! Output (stdout): one JSON line per completed message
//!   {"msg":id,"t_s":since-start,"lat_ms":latency}
//! and a final {"stream_summary":true,...} with p50/p90/p99/max and losses.
//!
//! `#[ignore]` — measurement instrument, not a CI gate.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use bytes::{BufMut, Bytes, BytesMut};
use raptorpath::{config, net, tun::TunInterface};

const MAGIC: u16 = 0x5253; // "RS"
const HDR_LEN: usize = 16; // magic(2) + msg_id(4) + chunk_idx(1) + total(1) + ts_us(8)
const ACK_IDX: u8 = 0xFF;
/// Realtime symbol_size 512 − 4 framing bytes = 508 usable; keep headroom.
const CHUNK_PAYLOAD_MAX: usize = 490;

fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

fn now_us() -> u64 {
    epoch().elapsed().as_micros() as u64
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn encode_chunk(msg_id: u32, chunk_idx: u8, total: u8, ts_us: u64, payload: &[u8]) -> Bytes {
    let mut b = BytesMut::with_capacity(HDR_LEN + payload.len());
    b.put_u16(MAGIC);
    b.put_u32(msg_id);
    b.put_u8(chunk_idx);
    b.put_u8(total);
    b.put_u64(ts_us);
    b.put_slice(payload);
    b.freeze()
}

/// (msg_id, chunk_idx, total, ts_us)
fn parse_hdr(pkt: &[u8]) -> Option<(u32, u8, u8, u64)> {
    if pkt.len() < HDR_LEN || u16::from_be_bytes([pkt[0], pkt[1]]) != MAGIC {
        return None;
    }
    Some((
        u32::from_be_bytes(pkt[2..6].try_into().unwrap()),
        pkt[6],
        pkt[7],
        u64::from_be_bytes(pkt[8..16].try_into().unwrap()),
    ))
}

fn pctl(sorted_ms: &[f64], p: f64) -> f64 {
    if sorted_ms.is_empty() {
        return f64::NAN;
    }
    let idx = ((sorted_ms.len() as f64 - 1.0) * p).round() as usize;
    sorted_ms[idx.min(sorted_ms.len() - 1)]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "measurement instrument (unified stream-collapse L0 rung), not a CI gate"]
async fn unified_stream_l0_arm() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .try_init();

    if std::env::var("RWM_L0_NETEM").is_err() {
        std::env::set_var("RWM_L0_NETEM", "c3");
    }
    let rate = env_usize("RWM_SL_RATE", 50) as f64;
    let dur_s = env_usize("RWM_SL_DUR", 20) as u64;
    let msg_bytes = env_usize("RWM_SL_MSG", 1200);
    let hint = std::env::var("RWM_L0_HINT").unwrap_or_else(|_| "realtime".into());
    let backend = std::env::var("RWM_L0_BACKEND").ok();

    eprintln!(
        "--- unified_stream_l0 arm: netem={:?} seed={:?} hint={hint} backend={backend:?} \
         rate={rate}/s dur={dur_s}s msg={msg_bytes}B RWM_UNIFIED={:?} RWM_TAPER_R={:?}",
        std::env::var("RWM_L0_NETEM").ok(),
        std::env::var("RWM_L0_SEED").ok(),
        std::env::var("RWM_UNIFIED").ok(),
        std::env::var("RWM_TAPER_R").ok(),
    );

    // ── server engine ──
    let srv_cfg = config::RaptorpathConfig {
        server: Some(true),
        bind: Some(vec!["127.0.0.1:47931".into()]),
        protocol_hint: Some(hint.clone()),
        fec_backend: backend.clone(),
        ..Default::default()
    };
    let (srv_pc, _) = config::resolve(&srv_cfg).unwrap();
    let (srv_tun, mut srv_mem) = TunInterface::memory(1500);
    let _srv_engine = tokio::spawn(net::run_with_tun(srv_pc, srv_tun));

    tokio::time::sleep(Duration::from_millis(500)).await;

    // ── client engine ──
    let cli_cfg = config::RaptorpathConfig {
        bind: Some(vec!["127.0.0.1:0".into()]),
        peer: Some(vec!["127.0.0.1:47931".into()]),
        protocol_hint: Some(hint),
        fec_backend: backend,
        ..Default::default()
    };
    let (cli_pc, _) = config::resolve(&cli_cfg).unwrap();
    let (cli_tun, mut cli_mem) = TunInterface::memory(1500);
    let _cli_engine = tokio::spawn(net::run_with_tun(cli_pc, cli_tun));

    // ── server task: reassemble messages, record latency, ack ──
    struct MsgState {
        got: u8,     // bitmask of chunk_idx (total ≤ 8)
        total: u8,
        ts_us: u64,
    }
    let (done_tx, mut done_rx) = tokio::sync::mpsc::unbounded_channel::<(u32, f64, f64)>();
    let srv_task = tokio::spawn(async move {
        let mut msgs: HashMap<u32, MsgState> = HashMap::new();
        let mut completed: u64 = 0;
        while let Some(pkt) = srv_mem.delivered.recv().await {
            let Some((msg_id, chunk_idx, total, ts_us)) = parse_hdr(&pkt) else {
                continue;
            };
            if chunk_idx == ACK_IDX {
                continue;
            }
            let st = msgs.entry(msg_id).or_insert(MsgState { got: 0, total, ts_us });
            st.got |= 1u8 << chunk_idx.min(7);
            let want: u8 = if st.total >= 8 { 0xFF } else { (1u8 << st.total) - 1 };
            if st.got & want == want {
                let st = msgs.remove(&msg_id).unwrap();
                completed += 1;
                let lat_ms = (now_us().saturating_sub(st.ts_us)) as f64 / 1000.0;
                let t_s = now_us() as f64 / 1e6;
                let _ = done_tx.send((msg_id, t_s, lat_ms));
                // ack (reverse-direction load, ~the inner TCP ack stream)
                let ack = encode_chunk(msg_id, ACK_IDX, 0, now_us(), &[]);
                if srv_mem.feed.send(ack).await.is_err() {
                    break;
                }
            }
        }
        completed
    });

    // ── client drain task (acks) ──
    let drain_task = tokio::spawn(async move {
        let mut acks: u64 = 0;
        // moved cli_mem.delivered in; feed handle returned for the sender
        while let Some(pkt) = cli_mem.delivered.recv().await {
            if let Some((_, idx, _, _)) = parse_hdr(&pkt) {
                if idx == ACK_IDX {
                    acks += 1;
                }
            }
        }
        acks
    });

    // ── warm-up: wait for the tunnel to pass traffic (msg id 0) ──
    let feed = cli_mem.feed.clone();
    let warm_deadline = Instant::now() + Duration::from_secs(30);
    let mut warmed = false;
    'warm: for attempt in 0..60u32 {
        let pkt = encode_chunk(0, 0, 1, now_us(), &[0xA5; 32]);
        if feed.send(pkt).await.is_err() {
            panic!("engine feed channel closed during warm-up");
        }
        let wait_until = Instant::now() + Duration::from_millis(500);
        while Instant::now() < wait_until {
            match tokio::time::timeout(Duration::from_millis(100), done_rx.recv()).await {
                Ok(Some((0, _, _))) => {
                    warmed = true;
                    break 'warm;
                }
                Ok(Some(_)) => {}
                Ok(None) => panic!("server task ended during warm-up"),
                Err(_) => {}
            }
        }
        if Instant::now() > warm_deadline {
            panic!("tunnel never passed warm-up traffic (attempt {attempt})");
        }
    }
    assert!(warmed, "tunnel warm-up failed");
    eprintln!("--- warm tunnel; stream start");

    // ── the stream: `rate` msg/s for dur_s seconds ──
    let n_msgs = (rate * dur_s as f64).round() as u32;
    let interval = Duration::from_secs_f64(1.0 / rate);
    let t0 = Instant::now();
    let stream_t0_us = now_us();
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);
    let total_chunks = msg_bytes.div_ceil(CHUNK_PAYLOAD_MAX) as u8;
    let payload = vec![0x5Au8; CHUNK_PAYLOAD_MAX];
    for m in 1..=n_msgs {
        ticker.tick().await;
        let ts = now_us();
        let mut left = msg_bytes;
        for c in 0..total_chunks {
            let k = left.min(CHUNK_PAYLOAD_MAX);
            left -= k;
            let pkt = encode_chunk(m, c, total_chunks, ts, &payload[..k]);
            if feed.send(pkt).await.is_err() {
                panic!("engine feed channel closed mid-stream (msg {m})");
            }
        }
    }
    let send_done_s = t0.elapsed().as_secs_f64();

    // ── grace drain: collect stragglers for 8 s past the stream end ──
    let mut lats: Vec<(u32, f64, f64)> = Vec::new();
    let drain_until = Instant::now() + Duration::from_secs(8);
    loop {
        let left = drain_until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        match tokio::time::timeout(left, done_rx.recv()).await {
            Ok(Some((id, t_s, lat))) if id > 0 => {
                lats.push((id, t_s - stream_t0_us as f64 / 1e6, lat));
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => break,
        }
    }

    for (id, t_s, lat) in &lats {
        println!(
            "{}",
            serde_json::json!({"msg": id, "t_s": (t_s * 1000.0).round() / 1000.0,
                               "lat_ms": (lat * 10.0).round() / 10.0})
        );
    }
    let mut ms: Vec<f64> = lats.iter().map(|(_, _, l)| *l).collect();
    ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let lost = n_msgs as i64 - ms.len() as i64;
    println!(
        "{}",
        serde_json::json!({
            "stream_summary": true,
            "netem": std::env::var("RWM_L0_NETEM").ok(),
            "seed": std::env::var("RWM_L0_SEED").ok(),
            "unified": std::env::var("RWM_UNIFIED").ok(),
            "backend": std::env::var("RWM_L0_BACKEND").ok(),
            "msgs_sent": n_msgs, "msgs_done": ms.len(), "lost": lost,
            "send_wall_s": (send_done_s * 100.0).round() / 100.0,
            "p50_ms": (pctl(&ms, 0.50) * 10.0).round() / 10.0,
            "p90_ms": (pctl(&ms, 0.90) * 10.0).round() / 10.0,
            "p99_ms": (pctl(&ms, 0.99) * 10.0).round() / 10.0,
            "max_ms": (pctl(&ms, 1.0) * 10.0).round() / 10.0,
        })
    );

    srv_task.abort();
    drain_task.abort();
}
