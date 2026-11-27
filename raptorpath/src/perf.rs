//! rp-native object benchmark (`raptorpath perf`).
//!
//! Drives fixed-size objects over the real multipath engine through a
//! memory-backed TUN ([`TunInterface::memory`]): no inner TCP stack, no
//! kernel TUN device. This is the fair-geometry counterpart to
//! quinn-perf's warm-connection object benchmark (L2 claim table): the
//! engine sees exactly the packets we feed and delivers exactly the
//! packets the peer fed, so completion time measures the rp pipeline
//! itself (chunk -> encode -> schedule -> QUIC datagrams -> decode ->
//! deliver) plus the window ARQ, with an application-level ack closing the
//! loop — the same delivery semantics as `transfer_bench.py`.
//!
//! Object protocol (opaque to the engine — it rides inside the "TUN
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
/// RWM_PERF_TIMEOUT_S overrides (harness knob for reliability batteries
/// where a DNF is an expected datum — e.g. the realtime profile, where a
/// hole force-delivered past the reorder horizon can never complete the
/// object, and 300 s per expected miss is impractical).
fn run_timeout() -> Duration {
    crate::gates::get()
        .perf_timeout_raw
        .as_deref()
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(300))
}

/// `RWM_PERF_RATE_MBIT`: the app-side offered-load pacer (default off), in
/// payload Mbit/s. Unset is `None`: the feed loop then takes no sleep and no
/// extra clock read. A value that is not a finite number > 0 is an error,
/// never silently unpaced.
fn parse_rate_mbit(raw: Option<&str>) -> anyhow::Result<Option<f64>> {
    let Some(raw) = raw else { return Ok(None) };
    match raw.trim().parse::<f64>() {
        Ok(v) if v.is_finite() && v > 0.0 => Ok(Some(v)),
        _ => anyhow::bail!(
            "RWM_PERF_RATE_MBIT={raw:?} is not a finite number > 0 (Mbit/s); \
             unset it to run unpaced"
        ),
    }
}

/// The knob's token for a burst source, as echoed: `10/33`, or `20/33+126000` with a lead-in.
fn burst_token(dgrams: u32, period_ms: u64, lead: u64) -> String {
    if lead > 0 { format!("{dgrams}/{period_ms}+{lead}") } else { format!("{dgrams}/{period_ms}") }
}

/// The offered load of a timed object: a constant payload rate or bursty frames.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Offered {
    /// Payload Mbit/s (`RWM_PERF_RATE_MBIT`).
    Rate(f64),
    /// `dgrams` datagrams released together every `period_ms` (`RWM_PERF_BURST`).
    Burst { dgrams: u32, period_ms: u64, lead: u64 },
}

/// `RWM_PERF_BURST=<datagrams>/<interval_ms>[+<lead_datagrams>]`: the bursty
/// offered-load source (default off): `datagrams` chunks released together every
/// `interval_ms` (a video-frame shape: 10/33), after an optional unpaced lead-in
/// of `lead_datagrams` chunks (a saturating start, so the controller leaves its
/// startup before the frames begin). Unset is `None`. Anything that is not
/// positive integers in that shape is an error, never silently unpaced.
fn parse_burst(raw: Option<&str>) -> anyhow::Result<Option<(u32, u64, u64)>> {
    let Some(raw) = raw else { return Ok(None) };
    let bad = || {
        anyhow::anyhow!(
            "RWM_PERF_BURST={raw:?} is not <datagrams>/<interval_ms> with both positive integers \
             (e.g. 10/33); unset it to run unpaced"
        )
    };
    let (d, rest) = raw.trim().split_once('/').ok_or_else(bad)?;
    let (p, lead) = match rest.split_once('+') {
        Some((p, l)) => (p, l.parse::<u64>().ok().filter(|l| *l > 0).ok_or_else(bad)?),
        None => (rest, 0),
    };
    match (d.parse::<u32>(), p.parse::<u64>()) {
        (Ok(d), Ok(p)) if d > 0 && p > 0 => Ok(Some((d, p, lead))),
        _ => Err(bad()),
    }
}

/// The offered load from the two knobs. Both set is an error: a silent pick of
/// one would run a different source than the one the row says.
fn parse_offered(rate_raw: Option<&str>, burst_raw: Option<&str>) -> anyhow::Result<Option<Offered>> {
    match (parse_rate_mbit(rate_raw)?, parse_burst(burst_raw)?) {
        (Some(_), Some(_)) => anyhow::bail!("RWM_PERF_RATE_MBIT and RWM_PERF_BURST are both set; use one"),
        (Some(m), None) => Ok(Some(Offered::Rate(m))),
        (None, Some((dgrams, period_ms, lead))) => Ok(Some(Offered::Burst { dgrams, period_ms, lead })),
        (None, None) => Ok(None),
    }
}

/// When chunk number `chunk` (0-based) of a burst source is due, as an offset
/// from the object's t0: its frame (`chunk / dgrams`) times the interval.
/// Absolute, like [`due_offset`]: a late frame does not push the later ones back.
fn burst_due(chunk: u64, dgrams: u32, period_ms: u64) -> Duration {
    Duration::from_millis(period_ms * (chunk / dgrams as u64))
}

/// Absolute schedule: when a chunk is due, as an offset from the object's
/// t0, given the payload bytes of the chunks before it and `lambda_bps`
/// (payload bits/s): `8 · bytes_before / λ`. Absolute, so a late send does
/// not push the later ones back.
fn due_offset(bytes_before: u64, lambda_bps: f64) -> Duration {
    Duration::from_secs_f64(8.0 * bytes_before as f64 / lambda_bps)
}

/// Completion more than this after `due` counts a chunk as late (the TUN
/// backpressure share, the pacer's own bind gauge).
const PACE_LATE: Duration = Duration::from_millis(2);

/// Pacer state for one timed object; no rate means no pacer at all.
struct Pacer {
    offered: Offered,
    sent: u64,
    chunks: u32,
    late: u32,
    late_max: Duration,
    /// Burst source with a lead-in: when the last lead-in chunk finished (frame 0 starts then).
    lead_end: Option<Duration>,
}

impl Pacer {
    /// The selector: no rate, no pacer.
    fn new(offered: Option<Offered>) -> Option<Pacer> {
        offered.map(|o| Pacer {
            offered: o,
            sent: 0,
            chunks: 0,
            late: 0,
            late_max: Duration::ZERO,
            lead_end: None,
        })
    }

    /// Due offset of the next chunk.
    fn next_due(&self) -> Duration {
        match self.offered {
            Offered::Rate(m) => due_offset(self.sent, m * 1e6),
            Offered::Burst { dgrams, period_ms, lead } => {
                let c = self.chunks as u64;
                if c < lead {
                    Duration::ZERO // the lead-in is unpaced: backpressure alone limits it
                } else {
                    self.lead_end.unwrap_or(Duration::ZERO) + burst_due(c - lead, dgrams, period_ms)
                }
            }
        }
    }

    /// Account a chunk of `k` payload bytes whose send finished at `done`
    /// (offset from t0) against its `due`.
    fn note_sent(&mut self, k: usize, due: Duration, done: Duration) {
        self.sent += k as u64;
        self.chunks += 1;
        if let Offered::Burst { lead, .. } = self.offered {
            if lead > 0 && self.chunks as u64 == lead {
                self.lead_end = Some(done);
            }
        }
        let over = done.saturating_sub(due);
        if over > PACE_LATE {
            self.late += 1;
        }
        self.late_max = self.late_max.max(over);
    }

    fn line(&self, obj_id: u32, feed_s: f64) -> serde_json::Value {
        let achieved = if feed_s > 0.0 { 8.0 * self.sent as f64 / feed_s / 1e6 } else { 0.0 };
        let mut v = serde_json::json!({
            "pace": true, "obj": obj_id,
            "chunks": self.chunks, "feed_s": round(feed_s, 6),
            "achieved_mbit": round(achieved, 6), "late": self.late,
            "late_max_ms": round(self.late_max.as_secs_f64() * 1e3, 3),
        });
        match self.offered {
            Offered::Rate(m) => v["rate_mbit"] = serde_json::json!(round(m, 6)),
            Offered::Burst { dgrams, period_ms, lead } => v["burst"] = serde_json::json!(burst_token(dgrams, period_ms, lead)),
        }
        v
    }
}

/// Max payload bytes per chunk for a protocol hint.
///
/// Every hint rides the sliding-window pipeline (ADR-0069), which carries
/// at most one packet per symbol and silently truncates
/// larger packets (see the TUN MTU clamp in net::run —
/// a memory TUN skips the clamp, so perf must size its own packets).
/// Bulk/auto use symbol_size=1200 → chunks must fit 1196 B total.
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
    /// `[THR]`/`[LAG]` window start (runtime_obs; `None` when not armed).
    obs: Option<crate::runtime_obs::Snapshot>,
}

/// Run the perf server: reassemble objects, ack each on completion.
pub async fn server(config: PeerConfig) -> anyhow::Result<()> {
    crate::runtime_obs::set_side("server");
    let (tun, mut mem) = TunInterface::memory(1500);
    let mut engine = tokio::spawn(crate::task_obs::timed("engine", net::run_with_tun(config, tun)));
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
            started: crate::clock::now(),
            obs: crate::runtime_obs::snapshot(),
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
                    "assemble_s": round(st.started.clock_elapsed().as_secs_f64(), 6),
                })
            );
            // The object's own window, first packet → completion (after the
            // ack is handed over, so the /proc read never delays it).
            if let Some(s0) = &st.obs {
                crate::runtime_obs::emit_window(s0, &format!(" obj={obj_id}"));
            }
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
    // Paper §4.6: the one driver in this tree that knows how much of the
    // transfer is left. `None` unless `RWM_COMPLETION_EXPOSURE` armed the
    // feed, in which case the engine's rate site reads it as `T_rem` and
    // runs the completion-exposure glide.
    feed: Option<&std::sync::Arc<net::CompletionFeed>>,
    // `RWM_PERF_RATE_MBIT` / `RWM_PERF_BURST` for a timed object; `None` = today's loop.
    offered: Option<Offered>,
) -> anyhow::Result<RunOutcome> {
    let mut pacer = Pacer::new(offered);
    let total = (nbytes.max(1)).div_ceil(payload_len) as u32;
    let payload = vec![0xA5u8; payload_len];
    let t0 = crate::clock::now();
    let mut left = nbytes.max(1);
    // The whole object is ahead of us, in the bytes the engine will read
    // (every chunk carries `HDR_LEN`). The ENGINE drains it as its sender
    // admits each packet — not this loop: the memory TUN queues 8192 chunks,
    // so "handed over" is not "sent" (see `CompletionFeed`).
    if let Some(f) = feed {
        f.set_remaining((left + total as usize * HDR_LEN) as u64);
    }
    for idx in 0..total {
        let k = left.min(payload_len);
        left -= k;
        let pkt = encode_chunk(obj_id, idx, total, &payload[..k]);
        let due = pacer.as_ref().map(Pacer::next_due);
        if let Some(due) = due {
            // A due at or past the deadline is a DNF at the deadline, as for a
            // backpressured send: wait it out, so the next object starts then.
            let wake = due.min(deadline);
            tokio::time::sleep_until(tokio::time::Instant::from_std(t0 + wake)).await;
            if due >= deadline {
                return Ok(RunOutcome::Dnf);
            }
        }
        match tokio::time::timeout(
            deadline.saturating_sub(t0.clock_elapsed()),
            mem.feed.send(pkt),
        )
        .await
        {
            Err(_) => return Ok(RunOutcome::Dnf), // backpressured past the deadline
            Ok(Err(_)) => anyhow::bail!("engine feed channel closed"),
            Ok(Ok(())) => {}
        }
        if let (Some(p), Some(due)) = (pacer.as_mut(), due) {
            p.note_sent(k, due, t0.elapsed());
        }
    }
    if let Some(p) = &pacer {
        println!("{}", p.line(obj_id, t0.elapsed().as_secs_f64()));
    }
    loop {
        let remaining = deadline.saturating_sub(t0.clock_elapsed());
        if remaining.is_zero() {
            return Ok(RunOutcome::Dnf);
        }
        match tokio::time::timeout(remaining, mem.delivered.recv()).await {
            Err(_) => return Ok(RunOutcome::Dnf),
            Ok(None) => anyhow::bail!("engine delivery channel closed"),
            Ok(Some(pkt)) => {
                if let Some((oid, idx, _, _)) = parse_packet(&pkt) {
                    if oid == obj_id && idx == ACK_IDX {
                        // Acked: nothing is left, so χ falls back to 0 and the
                        // next object sets its own remaining from scratch.
                        if let Some(f) = feed {
                            f.clear();
                        }
                        return Ok(RunOutcome::Acked(t0.clock_elapsed().as_secs_f64()));
                    }
                }
            }
        }
    }
}

/// `run_object`, but ends with the engine's own error if the engine task
/// ends first (e.g. "peer lost"), like the server: a dead tunnel must not
/// sit out the run timeout.
#[allow(clippy::too_many_arguments)]
async fn run_watched(
    engine: &mut tokio::task::JoinHandle<anyhow::Result<()>>,
    mem: &mut MemTun,
    obj_id: u32,
    nbytes: usize,
    payload_len: usize,
    deadline: Duration,
    feed: Option<&std::sync::Arc<net::CompletionFeed>>,
    offered: Option<Offered>,
) -> anyhow::Result<RunOutcome> {
    tokio::select! {
        r = run_object(mem, obj_id, nbytes, payload_len, deadline, feed, offered) => r,
        r = &mut *engine => match r {
            Ok(Ok(())) => anyhow::bail!("engine exited during object {obj_id}"),
            Ok(Err(e)) => Err(e.context(format!("engine ended during object {obj_id}"))),
            Err(e) => Err(anyhow::anyhow!("engine task failed: {e}")),
        },
    }
}

/// Run the perf client: warm-up object, then `runs` sequential timed
/// objects on the same warm engine (matching quinn-perf's warm
/// connection geometry). Emits one JSON line per run plus a summary
/// line in transfer_bench.py's schema.
pub async fn client(mut config: PeerConfig, nbytes: usize, runs: u32) -> anyhow::Result<()> {
    let hint = config.protocol_hint;
    let hint_str = format!("{hint:?}").to_lowercase();
    let payload_len = chunk_payload_len(hint);
    // Offered-load pacing (default off): an invalid value fails here, before
    // the engine starts. The warm-up object stays unpaced.
    let g = crate::gates::get();
    let offered = parse_offered(g.perf_rate_raw.as_deref(), g.perf_burst_raw.as_deref())?;
    // Mechanism-liveness echo (docs/measurement-discipline.md rule 1).
    match offered {
        Some(Offered::Rate(v)) => tracing::info!("perf offered-load pacing ACTIVE (RWM_PERF_RATE_MBIT={v})"),
        Some(Offered::Burst { dgrams, period_ms, lead }) => {
            tracing::info!("perf offered-load pacing ACTIVE (RWM_PERF_BURST={})", burst_token(dgrams, period_ms, lead))
        }
        None => {}
    }
    // Paper §4.6: publish the completion feed only when the arm
    // is set. Absent by default ⇒ `config.completion_feed` stays `None` and
    // the engine's rate site never reads it — byte-identical.
    let feed = if crate::gates::get().completion_exposure {
        let f = std::sync::Arc::new(net::CompletionFeed::new());
        config.completion_feed = Some(f.clone());
        // Mechanism-liveness echo (docs/measurement-discipline.md rule 1).
        tracing::info!(
            "completion-exposure feed ACTIVE (RWM_COMPLETION_EXPOSURE: the perf client \
             publishes remaining bytes; the 14.26 glide is fed for the first time)"
        );
        Some(f)
    } else {
        None
    };
    crate::runtime_obs::set_side("client");
    let (tun, mut mem) = TunInterface::memory(1500);
    let mut engine = tokio::spawn(crate::task_obs::timed("engine", net::run_with_tun(config, tun)));

    // Warm-up: a single-chunk object confirms both directions are up
    // before timing starts (the engine connects asynchronously).
    match run_watched(&mut engine, &mut mem, 0, 64, payload_len, run_timeout(), feed.as_ref(), None).await? {
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
        // `[THR]`/`[LAG]` over this timed object only (runtime_obs).
        let obs0 = crate::runtime_obs::snapshot();
        let outcome =
            run_watched(&mut engine, &mut mem, run, nbytes, payload_len, run_timeout(), feed.as_ref(), offered).await?;
        if let Some(s0) = &obs0 {
            crate::runtime_obs::emit_window(s0, &format!(" run={run}"));
        }
        match outcome {
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
                        "timeout_s": run_timeout().as_secs(),
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
    fn rate_parse_rejects_invalid_never_silently_unpaced() {
        assert_eq!(parse_rate_mbit(None).unwrap(), None);
        assert_eq!(parse_rate_mbit(Some("12.5")).unwrap(), Some(12.5));
        for bad in ["", "0", "-1", "nan", "inf", "abc", "1e999"] {
            assert!(parse_rate_mbit(Some(bad)).is_err(), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn burst_parse_rejects_invalid_never_silently_unpaced() {
        assert_eq!(parse_burst(None).unwrap(), None);
        assert_eq!(parse_burst(Some("10/33")).unwrap(), Some((10, 33, 0)));
        assert_eq!(parse_burst(Some(" 1/1 ")).unwrap(), Some((1, 1, 0)));
        assert_eq!(parse_burst(Some("20/33+126000")).unwrap(), Some((20, 33, 126_000)));
        for bad in ["", "10", "0/33", "10/0", "a/b", "10/33/1", "-1/33", "10/-3", "1.5/33", "10/", "/33", "10/33ms", "4294967296/33", "nan/inf", "20/33+", "20/33+x", "20/33+-1", "20/33+0", "20/33+1+2", "+5"] {
            assert!(parse_burst(Some(bad)).is_err(), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn rate_and_burst_together_is_an_error_not_a_silent_pick() {
        assert_eq!(parse_offered(None, None).unwrap(), None);
        assert_eq!(parse_offered(Some("8"), None).unwrap(), Some(Offered::Rate(8.0)));
        assert_eq!(parse_offered(None, Some("10/33")).unwrap(), Some(Offered::Burst { dgrams: 10, period_ms: 33, lead: 0 }));
        assert!(parse_offered(Some("8"), Some("10/33")).is_err());
        assert!(parse_offered(Some("0"), None).is_err());
        assert!(parse_offered(None, Some("0/33")).is_err());
    }

    #[test]
    fn burst_schedule_releases_whole_frames_on_absolute_instants() {
        // 10 datagrams per 33 ms: the first frame at 0, the second at 33 ms, the third at 66 ms.
        for c in 0..10 {
            assert_eq!(burst_due(c, 10, 33), Duration::ZERO, "chunk {c}");
        }
        for c in 10..20 {
            assert_eq!(burst_due(c, 10, 33), Duration::from_millis(33), "chunk {c}");
        }
        assert_eq!(burst_due(20, 10, 33), Duration::from_millis(66));
        // 1818 frames over 60 s: the last frame is due at 1817 x 33 ms
        assert_eq!(burst_due(18_179, 10, 33), Duration::from_millis(1817 * 33));
    }

    #[test]
    fn pacer_selector_burst_follows_the_frame_schedule_and_reports_it() {
        let mut p = Pacer::new(Some(Offered::Burst { dgrams: 10, period_ms: 33, lead: 0 })).unwrap();
        for _ in 0..10 {
            assert_eq!(p.next_due(), Duration::ZERO);
            p.note_sent(494, Duration::ZERO, Duration::from_micros(300));
        }
        assert_eq!(p.next_due(), Duration::from_millis(33));
        p.note_sent(494, Duration::from_millis(33), Duration::from_millis(40));
        assert_eq!((p.chunks, p.late, p.sent), (11, 1, 5434));
        let v = p.line(5, 0.5);
        assert_eq!(v["pace"], true);
        assert_eq!(v["burst"], "10/33");
        assert!(v.get("rate_mbit").is_none(), "a burst source has no constant rate: {v}");
    }

    #[test]
    fn lead_in_is_unpaced_and_the_frames_start_when_it_ends() {
        // 2 datagrams per 33 ms after a 3-chunk lead-in that finished at t = 5 s
        let mut p = Pacer::new(Some(Offered::Burst { dgrams: 2, period_ms: 33, lead: 3 })).unwrap();
        for c in 0..3u64 {
            assert_eq!(p.next_due(), Duration::ZERO, "lead chunk {c}");
            p.note_sent(494, Duration::ZERO, Duration::from_millis(1700 * (c + 1)));
        }
        assert_eq!(p.next_due(), Duration::from_millis(5100), "frame 0 starts when the lead-in ended");
        p.note_sent(494, Duration::from_millis(5100), Duration::from_millis(5100));
        assert_eq!(p.next_due(), Duration::from_millis(5100));
        p.note_sent(494, Duration::from_millis(5100), Duration::from_millis(5100));
        assert_eq!(p.next_due(), Duration::from_millis(5133), "frame 1");
        assert_eq!(p.line(1, 6.0)["burst"], "2/33+3");
    }

    #[test]
    fn due_offset_is_absolute_and_exact() {
        // 8 Mbit/s = 1 byte per microsecond.
        let lam = 8e6;
        assert_eq!(due_offset(0, lam), Duration::ZERO);
        // One full chunk (1182 B) before: 1182 us; two: 2364 us.
        assert_eq!(due_offset(1182, lam), Duration::from_micros(1182));
        assert_eq!(due_offset(2 * 1182, lam), Duration::from_micros(2364));
        // A short last chunk (100 B) after two full ones is due at 2364 us;
        // what follows it is due at 2464 us.
        assert_eq!(due_offset(2 * 1182 + 100, lam), Duration::from_micros(2464));
    }

    #[test]
    fn pacer_selector_unset_has_no_pacer_and_set_follows_schedule() {
        assert!(Pacer::new(None).is_none());
        let mut p = Pacer::new(Some(Offered::Rate(8.0))).unwrap();
        assert_eq!(p.next_due(), Duration::ZERO);
        p.note_sent(1182, Duration::ZERO, Duration::from_micros(500));
        assert_eq!(p.next_due(), Duration::from_micros(1182));
        // 1 ms late: under the 2 ms threshold, not counted.
        p.note_sent(1182, Duration::from_micros(1182), Duration::from_micros(2182));
        assert_eq!((p.chunks, p.late), (2, 0));
        // 5 ms late: counted, and the max tracks it.
        p.note_sent(100, Duration::from_micros(2364), Duration::from_micros(7364));
        assert_eq!((p.chunks, p.late, p.sent), (3, 1, 2464));
        assert_eq!(p.late_max, Duration::from_millis(5));
        let v = p.line(3, 0.002464);
        assert_eq!(v["pace"], true);
        assert_eq!(v["obj"], 3);
        assert_eq!(v["rate_mbit"], 8.0);
        assert_eq!(v["chunks"], 3);
        assert_eq!(v["late"], 1);
        assert_eq!(v["late_max_ms"], 5.0);
        assert_eq!(v["achieved_mbit"], 8.0); // 8 * 2464 B / 2.464 ms
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
        // Bulk/auto ride the window pipeline too (symbol_size 1200 →
        // usable 1196): packet total must fit one
        // symbol or frame_window_packet truncates it silently.
        assert!(HDR_LEN + chunk_payload_len(ProtocolHint::Bulk) <= 1196);
        assert!(HDR_LEN + chunk_payload_len(ProtocolHint::Auto) <= 1196);
    }
}

// Appended last so no existing line moves (release panic locations, sim-engine).
use crate::clock::ClockElapsed as _;
