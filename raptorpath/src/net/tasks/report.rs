//! RTCP-style periodic PathReport + keepalive, and the local send-rate feed
//! that keeps the estimator's throughput term non-sentinel (P10a).
//!
//! Behavior contract: the body is the former inline `async move` block from
//! `run_impl` verbatim. The lock discipline is the load-bearing part and is
//! preserved exactly: the whole per-tick scheduler work (send-rate feed,
//! dead-path check, MTU query, in_flight expiry/decay, report build) happens
//! inside ONE `report_scheduler.lock()` guard whose scope ends before the
//! `for (pid, report)` await loop — the report sends await on the reliable
//! stream and must not hold the scheduler lock. The two 500 ms `timeout`
//! wrappers around `send_control` (PathReport then Ping) and their three-way
//! match arms are unchanged; this task also runs the dead-path checker, so it
//! must never wedge. `sent_prev` / `sent_prev_t` remain task-local state that
//! survives across ticks.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tracing::{debug, warn};

use super::super::{DEAD_PATH_TIMEOUT, REPORT_INTERVAL, now_us};
use crate::monitor::stats::SharedStats;
use crate::scheduler::Scheduler;
use crate::transport::{ControlMessage, QuicTransport};

/// RTCP-style periodic report + keepalive task.
pub(crate) async fn run_report(
    report_transport: Arc<QuicTransport>,
    report_scheduler: Arc<parking_lot::Mutex<Scheduler>>,
    report_stats: Arc<SharedStats>,
    report_symbol_size: u16,
    mut report_shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) {
    let mut interval = tokio::time::interval(REPORT_INTERVAL);
    // P10a: local send-rate measurement state (per path): previous
    // symbols_sent counter and the last sample instant.
    let mut sent_prev: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut sent_prev_t = tokio::time::Instant::now();
    loop {
        tokio::select! {
            _ = interval.tick() => {}
            _ = report_shutdown_rx.recv() => break,
        }

        debug!("report tick");
        let reports: Vec<_> = {
        let mut sched = report_scheduler.lock();

        // P10a (paper 14.28): feed the estimator a LOCAL throughput
        // measurement — the achieved send rate over the report
        // interval. Production previously had NO local feed: the only
        // record_throughput call took the peer's PathReport value,
        // which is the peer's estimator.throughput() — circular, so
        // both sides sat at 0.0 forever and every throughput-gated
        // model term (t_sym: the 14.28 inner-feedback floor, the
        // 14.21 saturation cap, the 8.4 burst B/T term) was silently
        // sentinel-disabled on real links. The send rate is the right
        // t_sym semantics anyway: T_arq counts wire slots of the send
        // process the repairs are interleaved into.
        {
            let now_t = tokio::time::Instant::now();
            let dt = now_t.duration_since(sent_prev_t).as_secs_f64();
            if dt > 0.2 {
                for pid in sched.all_path_ids() {
                    let sent = report_stats
                        .path(pid)
                        .map(|ps| ps.symbols_sent.load(Ordering::Relaxed))
                        .unwrap_or(0);
                    let prev = sent_prev.insert(pid, sent).unwrap_or(sent);
                    let delta = sent.saturating_sub(prev);
                    // Only feed while actually sending: an idle tunnel
                    // must not decay the operating-rate estimate to 0
                    // (t_sym would blow up and re-disable the floor).
                    // feat/anchor-hygiene (`RWM_CLOCK_GAP`): a report
                    // tick inside a stall quarantine measures the
                    // release flood — skip the sample (the next tick's
                    // Δ/dt spans the disturbance and averages it out).
                    let gap_q = crate::control::anchor::stall_witness()
                        .is_some_and(|w| w.quarantined_now());
                    if delta > 0 && !gap_q {
                        if let Some(path) = sched.path_mut(pid) {
                            let bps = delta as f64 * report_symbol_size as f64 / dt;
                            path.estimator.record_throughput(bps);
                        }
                    }
                }
                sent_prev_t = now_t;
            }
        }

        // Check for dead paths
        let deactivated = sched.check_dead_paths(DEAD_PATH_TIMEOUT);
        for pid in &deactivated {
            if let Some(ps) = report_stats.path(*pid) {
                ps.active.store(false, Ordering::Relaxed);
            }
        }

        // Query and store MTU per path
        for pid in sched.all_path_ids() {
            if let Some(mtu) = report_transport.max_datagram_size(pid) {
                if let Some(path) = sched.path_mut(pid) {
                    path.max_datagram_size = Some(mtu);
                }
            }
        }

        // in_flight leak guard (backstop): time-based expiry
        // (PathState::expire_in_flight, RTT-timescale) is the primary
        // release for stranded budget; the 25% decay remains as a
        // last-resort backstop for anything the expiry can't see
        // (e.g. direct in_flight writes that bypassed the charge log).
        for pid in sched.all_path_ids() {
            if let Some(path) = sched.path_mut(pid) {
                path.expire_in_flight();
                if path.in_flight > path.cwnd {
                    path.in_flight -= path.in_flight / 4;
                }
            }
        }

        // Send PathReport + Ping on each LIVE path (not active_paths:
        // that filters by spare cwnd, and a saturated path still needs
        // its liveness heartbeats — see Scheduler::live_paths).
        let path_ids = sched.live_paths();
        path_ids.iter().filter_map(|&pid| {
            let path = sched.path(pid)?;
            let ps = report_stats.path(pid)?;
            Some((pid, ControlMessage::PathReport {
                path_id: pid,
                loss_rate: path.estimator.loss_rate(),
                avg_rtt_us: path.estimator.rtt().as_micros() as u64,
                throughput_bps: path.estimator.throughput(),
                jitter_us: path.estimator.jitter_us() as u64,
                symbols_sent: ps.symbols_sent.load(Ordering::Relaxed),
                symbols_received: ps.symbols_received.load(Ordering::Relaxed),
            }))
        }).collect()
        // guard dropped by scope end: the report sends below await on
        // the reliable stream and must not hold the scheduler lock
        };

        for (pid, report) in reports {
            // Liveness must not share fate with the data flood: under
            // load the datagram queue is saturated by symbol batches
            // and report datagrams get dropped, so the peer declares
            // the path dead after DEAD_PATH_TIMEOUT and QUIC idles out
            // (L1 finding: every bulk transfer killed the tunnel in
            // ~6 s). The reliable control stream has its own flow
            // control, so reports and pings survive saturation.
            // Hard deadline on control sends: this task also runs the
            // dead-path checker, so it must NEVER wedge (open_uni can
            // block indefinitely once stream credit is exhausted).
            match tokio::time::timeout(
                Duration::from_millis(500),
                report_transport.send_control(pid, report),
            )
            .await
            {
                Err(_) => warn!(pid, "PathReport send timed out (stream credit?)"),
                Ok(Err(e)) => warn!(pid, ?e, "failed to send PathReport on control stream"),
                Ok(Ok(())) => {}
            }
            match tokio::time::timeout(
                Duration::from_millis(500),
                report_transport.send_control(pid, ControlMessage::Ping { timestamp_us: now_us() }),
            )
            .await
            {
                Err(_) => warn!(pid, "Ping send timed out (stream credit?)"),
                Ok(Err(e)) => warn!(pid, ?e, "failed to send Ping on control stream"),
                Ok(Ok(())) => debug!(pid, "ping sent on control stream"),
            }
        }
    }
}
