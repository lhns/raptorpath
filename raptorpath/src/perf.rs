//! rp-native object benchmark (`raptorpath perf`).
//!
//! Drives fixed-size objects over the REAL multipath engine through a
//! memory-backed TUN ([`TunInterface::memory`]): no inner TCP stack, no
//! kernel TUN device. This is the fair-geometry counterpart to
//! quinn-perf's warm-connection object benchmark (L2 claim table): the
//! engine sees exactly the packets we feed and delivers exactly the
//! packets the peer fed, so completion time measures the rp pipeline
//! itself (chunk -> encode -> schedule -> QUIC datagrams -> decode ->
//! deliver) plus block ARQ, with an application-level ack closing the
//! loop — the same delivery semantics as `transfer_bench.py`.
//!
//! Object protocol (opaque to the engine — it rides INSIDE the "TUN
//! packets" the engine treats as payload):
//!   data packet: [magic u16][obj_id u32][chunk_idx u32][total_chunks u32][payload]
//!   ack  packet: [magic u16][obj_id u32][ACK_IDX u32][0 u32]
//! All integers big-endian. The transport guarantees delivery via ARQ,
//! but cross-block ordering is only held within the in-order horizon, so
//! chunks can arrive out of order (and, defensively, duplicated); the
//! server reassembles by (obj_id, chunk_idx) and acks on completion.

use crate::control::fec_rate::ProtocolHint;
use crate::net::{self, PeerConfig};
use crate::tun::{MemTun, TunInterface};
use bytes::{BufMut, Bytes, BytesMut};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

const MAGIC: u16 = 0x5250; // "RP"
const HDR_LEN: usize = 14; // magic(2) + obj_id(4) + chunk_idx(4) + total_chunks(4)
const ACK_IDX: u32 = u32::MAX;
/// Per-run completion timeout; a run exceeding this is recorded as DNF.
const RUN_TIMEOUT: Duration = Duration::from_secs(300);

/// Max payload bytes per chunk for a protocol hint.
///
/// Bulk/auto under `--window-reliable` (RWM Phase A) ride the sliding-
/// window pipeline, which carries at most ONE packet per symbol and
/// silently TRUNCATES larger packets (see the TUN MTU clamp in net::run —
/// a memory TUN skips the clamp, so perf must size its own packets).
/// Bulk/auto use symbol_size=1200 → chunks must fit 1196 B total. The
/// same size is used for the block-mode arm (which length-prefixes any
/// MTU-ish packet into 64 KB blocks) so both A/B arms share identical
/// chunk geometry — the flag is the ONLY difference.
/// Realtime uses symbol_size=512 → 508 B total.
fn chunk_payload_len(hint: ProtocolHint) -> usize {
    match hint {
        ProtocolHint::Realtime => 508 - HDR_LEN,
        _ => 1196 - HDR_LEN,
    }
}

fn encode_chunk(obj_id: u32, chunk_idx: u32, total_chunks: u32, payload: &[u8]) -> Bytes {
    let mut b = BytesMut::with_capacity(HDR_LEN + payload.len());
    b.put_u16(MAGIC);
    b.put_u32(obj_id);
    b.put_u32(chunk_idx);
    b.put_u32(total_chunks);
    b.put_slice(payload);
    b.freeze()
}

fn encode_ack(obj_id: u32) -> Bytes {
    let mut b = BytesMut::with_capacity(HDR_LEN);
    b.put_u16(MAGIC);
    b.put_u32(obj_id);
    b.put_u32(ACK_IDX);
    b.put_u32(0);
    b.freeze()
}

/// Parse a perf packet: (obj_id, chunk_idx, total_chunks, payload).
fn parse_packet(pkt: &[u8]) -> Option<(u32, u32, u32, &[u8])> {
    if pkt.len() < HDR_LEN || u16::from_be_bytes([pkt[0], pkt[1]]) != MAGIC {
        return None;
    }
    let obj_id = u32::from_be_bytes(pkt[2..6].try_into().unwrap());
    let chunk_idx = u32::from_be_bytes(pkt[6..10].try_into().unwrap());
    let total = u32::from_be_bytes(pkt[10..14].try_into().unwrap());
    Some((obj_id, chunk_idx, total, &pkt[HDR_LEN..]))
}

fn round(x: f64, digits: i32) -> f64 {
    let m = 10f64.powi(digits);
    (x * m).round() / m
}

/// Reassembly state for one in-flight object (server side).
struct ObjState {
    got: HashSet<u32>,
    total: u32,
    bytes: usize,
    started: Instant,
}

/// Run the perf server: reassemble objects, ack each on completion.
pub async fn server(config: PeerConfig) -> anyhow::Result<()> {
    let (tun, mut mem) = TunInterface::memory(1500);
    let mut engine = tokio::spawn(net::run_with_tun(config, tun));
    println!("perf server ready (rp-native object mode)");

    let mut objs: HashMap<u32, ObjState> = HashMap::new();
    let mut done: HashSet<u32> = HashSet::new();
    loop {
        let pkt = tokio::select! {
            p = mem.delivered.recv() => match p {
                Some(p) => p,
                None => break, // engine dropped the delivery channel
            },
            r = &mut engine => {
                return match r {
                    Ok(inner) => inner,
                    Err(e) => Err(anyhow::anyhow!("engine task failed: {e}")),
                };
            }
        };
        let Some((obj_id, chunk_idx, total, payload)) = parse_packet(&pkt) else {
            continue;
        };
        if chunk_idx == ACK_IDX || done.contains(&obj_id) {
            continue;
        }
        let st = objs.entry(obj_id).or_insert_with(|| ObjState {
            got: HashSet::new(),
            total,
            bytes: 0,
            started: Instant::now(),
        });
        if st.got.insert(chunk_idx) {
            st.bytes += payload.len();
        }
        if st.got.len() as u32 == st.total {
            let st = objs.remove(&obj_id).unwrap();
            done.insert(obj_id);
            mem.feed
                .send(encode_ack(obj_id))
                .await
                .map_err(|_| anyhow::anyhow!("engine feed channel closed"))?;
            println!(
                "{}",
                serde_json::json!({
                    "server": true, "obj_id": obj_id, "bytes": st.bytes,
                    "chunks": st.total,
                    "assemble_s": round(st.started.elapsed().as_secs_f64(), 6),
                })
            );
        }
    }
    // engine shut down (Ctrl+C or task exit)
    match engine.await {
        Ok(inner) => inner,
        Err(e) => Err(anyhow::anyhow!("engine task failed: {e}")),
    }
}

enum RunOutcome {
    Acked(f64),
    Dnf,
}

/// Feed one object's chunks and await its ack. Stale packets (late acks
/// from previous objects, echoes) are skipped by obj_id matching.
async fn run_object(
    mem: &mut MemTun,
    obj_id: u32,
    nbytes: usize,
    payload_len: usize,
    deadline: Duration,
) -> anyhow::Result<RunOutcome> {
    let total = (nbytes.max(1)).div_ceil(payload_len) as u32;
    let payload = vec![0xA5u8; payload_len];
    let t0 = Instant::now();
    let mut left = nbytes.max(1);
    for idx in 0..total {
        let k = left.min(payload_len);
        left -= k;
        let pkt = encode_chunk(obj_id, idx, total, &payload[..k]);
        match tokio::time::timeout(
            deadline.saturating_sub(t0.elapsed()),
            mem.feed.send(pkt),
        )
        .await
        {
            Err(_) => return Ok(RunOutcome::Dnf), // backpressured past the deadline
            Ok(Err(_)) => anyhow::bail!("engine feed channel closed"),
            Ok(Ok(())) => {}
        }
    }
    loop {
        let remaining = deadline.saturating_sub(t0.elapsed());
        if remaining.is_zero() {
            return Ok(RunOutcome::Dnf);
        }
        match tokio::time::timeout(remaining, mem.delivered.recv()).await {
            Err(_) => return Ok(RunOutcome::Dnf),
            Ok(None) => anyhow::bail!("engine delivery channel closed"),
            Ok(Some(pkt)) => {
                if let Some((oid, idx, _, _)) = parse_packet(&pkt) {
                    if oid == obj_id && idx == ACK_IDX {
                        return Ok(RunOutcome::Acked(t0.elapsed().as_secs_f64()));
                    }
                }
            }
        }
    }
}

/// Run the perf client: warm-up object, then `runs` sequential timed
/// objects on the same warm engine (matching quinn-perf's warm
/// connection geometry). Emits one JSON line per run plus a summary
/// line in transfer_bench.py's schema.
pub async fn client(config: PeerConfig, nbytes: usize, runs: u32) -> anyhow::Result<()> {
    let hint = config.protocol_hint;
    let hint_str = format!("{hint:?}").to_lowercase();
    let payload_len = chunk_payload_len(hint);
    let (tun, mut mem) = TunInterface::memory(1500);
    let _engine = tokio::spawn(net::run_with_tun(config, tun));

    // Warm-up: a single-chunk object confirms both directions are up
    // before timing starts (the engine connects asynchronously).
    match run_object(&mut mem, 0, 64, payload_len, RUN_TIMEOUT).await? {
        RunOutcome::Acked(s) => {
            println!(
                "{}",
                serde_json::json!({"warmup": true, "seconds": round(s, 6)})
            );
        }
        RunOutcome::Dnf => anyhow::bail!("warm-up object never acked — tunnel not passing traffic"),
    }

    let mut times: Vec<f64> = Vec::new();
    let mut dnfs = 0u32;
    for run in 1..=runs {
        match run_object(&mut mem, run, nbytes, payload_len, RUN_TIMEOUT).await? {
            RunOutcome::Acked(secs) => {
                times.push(secs);
                println!(
                    "{}",
                    serde_json::json!({
                        "proto": "rp-native", "hint": hint_str, "bytes": nbytes,
                        "run": run, "seconds": round(secs, 6),
                        "mbps": round(nbytes as f64 * 8.0 / secs / 1e6, 3),
                    })
                );
            }
            RunOutcome::Dnf => {
                dnfs += 1;
                println!(
                    "{}",
                    serde_json::json!({
                        "proto": "rp-native", "hint": hint_str, "bytes": nbytes,
                        "run": run, "dnf": true,
                        "timeout_s": RUN_TIMEOUT.as_secs(),
                    })
                );
            }
        }
    }

    if times.is_empty() {
        println!(
            "{}",
            serde_json::json!({
                "summary": true, "proto": "rp-native", "hint": hint_str,
                "bytes": nbytes, "runs": runs, "dnf": dnfs,
            })
        );
        return Ok(());
    }
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    let mut sorted = times.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let stdev = if times.len() > 1 {
        let var = times.iter().map(|t| (t - mean).powi(2)).sum::<f64>()
            / (times.len() - 1) as f64;
        var.sqrt()
    } else {
        0.0
    };
    println!(
        "{}",
        serde_json::json!({
            "summary": true, "proto": "rp-native", "hint": hint_str,
            "bytes": nbytes, "runs": runs, "dnf": dnfs,
            "mean_s": round(mean, 4),
            "min_s": round(sorted[0], 4),
            "median_s": round(sorted[sorted.len() / 2], 4),
            "max_s": round(sorted[sorted.len() - 1], 4),
            "stdev_s": round(stdev, 4),
            "mean_mbps": round(nbytes as f64 * 8.0 / mean / 1e6, 3),
        })
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_roundtrip() {
        let pkt = encode_chunk(7, 3, 9, &[1, 2, 3]);
        let (o, i, t, p) = parse_packet(&pkt).unwrap();
        assert_eq!((o, i, t, p), (7, 3, 9, &[1u8, 2, 3][..]));
    }

    #[test]
    fn ack_roundtrip() {
        let pkt = encode_ack(42);
        let (o, i, _, p) = parse_packet(&pkt).unwrap();
        assert_eq!(o, 42);
        assert_eq!(i, ACK_IDX);
        assert!(p.is_empty());
    }

    #[test]
    fn non_magic_rejected() {
        assert!(parse_packet(&[0u8; 20]).is_none());
        assert!(parse_packet(&[0x52]).is_none());
    }

    #[test]
    fn realtime_chunks_fit_one_symbol() {
        // window mode: packet total (header + payload) must fit
        // symbol_size(512) - 4 framing bytes
        assert!(HDR_LEN + chunk_payload_len(ProtocolHint::Realtime) <= 508);
        // Bulk/auto may ride the window pipeline (--window-reliable,
        // symbol_size 1200 → usable 1196): packet total must fit one
        // symbol or frame_window_packet truncates it silently.
        assert!(HDR_LEN + chunk_payload_len(ProtocolHint::Bulk) <= 1196);
        assert!(HDR_LEN + chunk_payload_len(ProtocolHint::Auto) <= 1196);
    }
}
