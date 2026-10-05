//! RTCP-style periodic PathReport + keepalive, and the local send-rate feed
//! that keeps the estimator's throughput term non-sentinel.
//!
//! Lock discipline: the per-path MTU query (a quinn call) runs first with
//! the scheduler released (threading P1, D2: no quinn call under the
//! scheduler); the rest of the per-tick scheduler work (send-rate feed,
//! dead-path check, MTU store, in_flight expiry/decay, report build) happens
//! inside one `report_scheduler.lock()` guard whose scope ends before the
//! `for (pid, report)` await loop — the report sends await on the reliable
//! stream and must not hold the scheduler lock. Control sends are wrapped in
//! 500 ms timeouts because this task also runs the dead-path checker and
//! must never wedge. `sent_prev` / `sent_prev_t` are task-local state that
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
    report_scheduler: Arc<crate::scheduler::SchedMutex>,
    report_stats: Arc<SharedStats>,
    report_symbol_size: u16,
    mut report_shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) {
    let mut interval = tokio::time::interval(REPORT_INTERVAL);
    // Local send-rate measurement state (per path): previous symbols_sent
    // counter and the last sample instant.
    let mut sent_prev: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
    let mut sent_prev_t = tokio::time::Instant::now();
    loop {
        tokio::select! {
            _ = interval.tick() => {}
            _ = report_shutdown_rx.recv() => break,
        }

        debug!("report tick");
        // Query the MTU per path first (threading P1, D2): it is a quinn
        // call (`Connection::max_datagram_size` takes the connection mutex),
        // so it runs with the scheduler released; the values are stored
        // under the guard below.
        let mtus: Vec<(u32, usize)> = {
            let ids = report_scheduler.lock().all_path_ids();
            ids.into_iter()
                .filter_map(|pid| report_transport.max_datagram_size(pid).map(|m| (pid, m)))
                .collect()
        };
        let reports: Vec<_> = {
        let mut sched = report_scheduler.lock();

        // Feed the estimator a local throughput measurement — the achieved
        // send rate over the report interval. The peer's PathReport value is
        // the peer's own estimator.throughput(), which is circular (both
        // sides would sit at 0.0), and every throughput-gated rate term
        // (t_sym: the inner-feedback floor, the saturation cap and the burst
        // B/T term, paper §4.4) would be sentinel-disabled. The send rate is
        // the right t_sym semantics anyway: T_arq counts wire slots of the
        // send process the repairs are interleaved into.
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
                    // `RWM_CLOCK_GAP`: a report tick inside a stall
                    // quarantine measures the release flood — skip the
                    // sample (the next tick's Δ/dt spans the disturbance
                    // and averages it out).
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

        // Store the MTU per path (queried above, outside the guard).
        for &(pid, mtu) in &mtus {
            if let Some(path) = sched.path_mut(pid) {
                path.max_datagram_size = Some(mtu);
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
                // The receiver-observed INCOMING loss, as the field's
                // meaning says (monitoring at the peer; never fed back).
                loss_rate: path.estimator.rx_loss_rate(),
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
            // load the datagram queue is saturated by symbol batches and
            // report datagrams get dropped, so the peer would declare the
            // path dead after DEAD_PATH_TIMEOUT and QUIC would idle out.
            // The reliable control stream has its own flow control, so
            // reports and pings survive saturation.
            // Hard deadline on control sends: this task also runs the
            // dead-path checker, so it must never wedge (open_uni can
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
