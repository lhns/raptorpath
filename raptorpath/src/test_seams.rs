//! Per-seq fault injection and event trace for the S06 reactive-recovery
//! tests (`tests/s06_reactive.rs`). Compiled only with `--features test-seams`
//! (never in the release binary); inert unless `RWM_TEST_TS_SEQ` is set.
//!
//! Env (read once per process):
//! * `RWM_TEST_TS_SEQ=<k>`: the seq under test (a source symbol's `block_id`).
//! * `RWM_TEST_TS_DROP=first:<n>` swallow the first n wire sends of source seq k;
//!   `all` swallow every wire send of source seq k; `dead` swallow every data
//!   send (source and repair) from the first send of k on.
//! * `RWM_TEST_TS_DEAD_MS=<ms>`: `dead` ends that long after k's first send
//!   (default: never).
//! * `RWM_TEST_TS_BUDGET_CLOSED=1`: the sender's shed budget fraction is forced to 0.
//!
//! Every event is printed on stderr as `[TSEAM] ev=<name> seq=<k> t_us=<µs>
//! …` so a test reads it from the process log. Times are one process's own
//! clock: a test only subtracts times from the same process.

use crate::fec::WireSymbol;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Off,
    First(u64),
    All,
    Dead,
}

struct Cfg {
    k: u64,
    drop: Mode,
    budget_closed: bool,
    dead_us: u64,
}

fn cfg() -> &'static Cfg {
    static C: OnceLock<Cfg> = OnceLock::new();
    C.get_or_init(|| {
        let k = std::env::var("RWM_TEST_TS_SEQ").ok().and_then(|v| v.parse().ok()).unwrap_or(u64::MAX);
        let drop = match std::env::var("RWM_TEST_TS_DROP").ok().as_deref() {
            Some("all") => Mode::All,
            Some("dead") => Mode::Dead,
            Some(s) => s
                .strip_prefix("first:")
                .and_then(|n| n.parse().ok())
                .map_or(Mode::Off, Mode::First),
            None => Mode::Off,
        };
        let budget_closed = std::env::var("RWM_TEST_TS_BUDGET_CLOSED").is_ok_and(|v| v == "1");
        let dead_us = std::env::var("RWM_TEST_TS_DEAD_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map_or(u64::MAX, |ms| ms * 1000);
        Cfg { k, drop, budget_closed, dead_us }
    })
}

fn t_us() -> u64 {
    crate::net::now_us()
}

/// Sends of source seq k seen so far (including swallowed ones).
static K_SENDS: AtomicU64 = AtomicU64::new(0);
/// Set once k's first send happened (arms `Dead`).
static K_SENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// `t_us` of k's first send.
static K_T0: AtomicU64 = AtomicU64::new(0);

fn dead_now(c: &Cfg) -> bool {
    c.dead_us == u64::MAX || t_us().saturating_sub(K_T0.load(Relaxed)) < c.dead_us
}

/// A wire send of `syms` through `site` (`single` = `send_symbol`, `batch` =
/// `send_symbols`). Returns true when the send must be swallowed.
pub fn on_send(syms: &[WireSymbol], site: &str) -> bool {
    let c = cfg();
    if c.k == u64::MAX {
        return false;
    }
    let is_k = syms.iter().any(|s| !s.is_repair && s.block_id == c.k);
    let mut swallow = false;
    if is_k {
        let n = K_SENDS.fetch_add(1, Relaxed) + 1;
        if n == 1 {
            K_T0.store(t_us(), Relaxed);
            K_SENT.store(true, Relaxed);
        }
        swallow = match c.drop {
            Mode::Off => false,
            Mode::First(m) => n <= m,
            Mode::All => true,
            Mode::Dead => dead_now(c),
        };
        crate::readout!("[TSEAM] ev=send seq={} n={n} site={site} dropped={} t_us={}", c.k, swallow as u8, t_us());
    } else if c.drop == Mode::Dead && K_SENT.load(Relaxed) && dead_now(c) {
        swallow = true;
    }
    if K_SENT.load(Relaxed) {
        tick_send(syms, swallow);
    }
    swallow
}

/// Liveness witness (sender): after k's first send, every 100 ms, the wire
/// sends that passed / were swallowed and the highest source seq sent.
fn tick_send(syms: &[WireSymbol], swallowed: bool) {
    static PASS: AtomicU64 = AtomicU64::new(0);
    static SWAL: AtomicU64 = AtomicU64::new(0);
    static MAXSRC: AtomicU64 = AtomicU64::new(0);
    static REP: AtomicU64 = AtomicU64::new(0);
    static LAST: AtomicU64 = AtomicU64::new(0);
    let n = syms.len() as u64;
    if swallowed { SWAL.fetch_add(n, Relaxed); } else { PASS.fetch_add(n, Relaxed); }
    for s in syms {
        if s.is_repair { REP.fetch_add(1, Relaxed); } else { MAXSRC.fetch_max(s.block_id, Relaxed); }
    }
    let now = t_us();
    let last = LAST.load(Relaxed);
    if now.saturating_sub(last) >= 100_000 && LAST.compare_exchange(last, now, Relaxed, Relaxed).is_ok() {
        crate::readout!(
            "[TSEAM] ev=txtick rel_us={} pass={} swallowed={} repair={} maxsrc={} t_us={now}",
            now.saturating_sub(K_T0.load(Relaxed)),
            PASS.load(Relaxed), SWAL.load(Relaxed), REP.load(Relaxed), MAXSRC.load(Relaxed)
        );
    }
}

/// Liveness witness (receiver): a data symbol reached the window decoder.
/// Every 100 ms after the first arrival above k: arrivals, highest source
/// seq, and whether k itself arrived as a source symbol.
pub fn on_recv(block_id: u64, is_repair: bool) {
    static N: AtomicU64 = AtomicU64::new(0);
    static REP: AtomicU64 = AtomicU64::new(0);
    static MAXSRC: AtomicU64 = AtomicU64::new(0);
    static KSRC: AtomicU64 = AtomicU64::new(0);
    static LAST: AtomicU64 = AtomicU64::new(0);
    let c = cfg();
    if c.k == u64::MAX {
        return;
    }
    N.fetch_add(1, Relaxed);
    if is_repair { REP.fetch_add(1, Relaxed); } else {
        MAXSRC.fetch_max(block_id, Relaxed);
        if block_id == c.k { KSRC.fetch_add(1, Relaxed); }
    }
    let now = t_us();
    let last = LAST.load(Relaxed);
    if now.saturating_sub(last) >= 100_000 && LAST.compare_exchange(last, now, Relaxed, Relaxed).is_ok() {
        crate::readout!(
            "[TSEAM] ev=rxtick n={} repair={} maxsrc={} k_src={} t_us={now}",
            N.load(Relaxed), REP.load(Relaxed), MAXSRC.load(Relaxed), KSRC.load(Relaxed)
        );
    }
}

/// The receiver's in-order frontier, printed when it changes by a force
/// drain (capacity) past k: the one give-up `on_give_up` does not see.
pub fn on_force_drain(before: u64, after: u64, drained: &[u64]) {
    let c = cfg();
    if c.k != u64::MAX && c.k >= before && c.k < after && !drained.contains(&c.k) {
        crate::readout!("[TSEAM] ev=pass_force seq={} t_us={}", c.k, t_us());
    }
}

/// The NACK-site shed of seq (`age_us` against deadline `d_us`).
pub fn on_shed(seq: u64, age_us: u64, d_us: u64) {
    let c = cfg();
    if seq == c.k {
        crate::readout!("[TSEAM] ev=shed seq={seq} age_us={age_us} d_us={d_us} t_us={}", t_us());
    }
}

/// `shed_budget_frac` override (`RWM_TEST_TS_BUDGET_CLOSED=1` ⇒ 0).
pub fn budget_frac(frac: f64) -> f64 {
    if cfg().budget_closed { 0.0 } else { frac }
}

/// A shed-denied seq (`shed_denied += 1`) at the NACK site.
pub fn on_denied(seq: u64) {
    if seq == cfg().k {
        crate::readout!("[TSEAM] ev=denied seq={seq} t_us={}", t_us());
    }
}

/// Receiver in-order frontier witness, every 100 ms while it is fed:
/// `next_deliver_seq`, pending entries, and the hold timeout in force.
pub fn on_frontier(next_deliver: u64, pending: usize, hold_us: u64) {
    static LAST: AtomicU64 = AtomicU64::new(0);
    if cfg().k == u64::MAX {
        return;
    }
    let now = t_us();
    let last = LAST.load(Relaxed);
    if now.saturating_sub(last) >= 100_000 && LAST.compare_exchange(last, now, Relaxed, Relaxed).is_ok() {
        crate::readout!("[TSEAM] ev=frontier next={next_deliver} pending={pending} hold_us={hold_us} t_us={now}");
    }
}

/// The reorder buffer was offered `seq` (an arrival or a decode fill).
/// Printed for k itself and for the first seq above k.
pub fn on_arrive(seq: u64) {
    static ABOVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let c = cfg();
    if c.k == u64::MAX {
        return;
    }
    if seq == c.k {
        crate::readout!("[TSEAM] ev=fill seq={seq} t_us={}", t_us());
    } else if seq > c.k && !ABOVE.swap(true, Relaxed) {
        crate::readout!("[TSEAM] ev=above seq={} first={seq} t_us={}", c.k, t_us());
    }
}

/// The in-order frontier moved from `before` to `after` by a give-up; `held`
/// are the seqs it delivered. k passed when it lies in the jump undelivered.
pub fn on_give_up(before: u64, after: u64, held: &[u64]) {
    let c = cfg();
    if c.k != u64::MAX && c.k >= before && c.k < after && !held.contains(&c.k) {
        crate::readout!("[TSEAM] ev=pass seq={} t_us={}", c.k, t_us());
    }
}

/// A slot-site shed (the deleted `P_lost` branch's shed; only the pre-ADR-0079
/// tree calls it). Kept so one test file reads both trees.
pub fn on_shed_slot(seq: u64, age_us: u64, d_us: u64) {
    if seq == cfg().k {
        crate::readout!("[TSEAM] ev=shed_slot seq={seq} age_us={age_us} d_us={d_us} t_us={}", t_us());
    }
}

/// A slot-site shed refusal (past D, budget closed) of k.
pub fn on_denied_slot(seq: u64) {
    if seq == cfg().k {
        crate::readout!("[TSEAM] ev=denied_slot seq={seq} t_us={}", t_us());
    }
}

/// Evicting NACK site: a gap wholly outside the coding window was dropped
/// before any per-seq judgement (resend or shed). Every 100 ms.
pub fn on_gap_clamped(gap_start: u64, gap_end: u64, win_start: u64, win_end: u64) {
    static N: AtomicU64 = AtomicU64::new(0);
    static LAST: AtomicU64 = AtomicU64::new(0);
    if cfg().k == u64::MAX {
        return;
    }
    let n = N.fetch_add(1, Relaxed) + 1;
    let now = t_us();
    let last = LAST.load(Relaxed);
    if now.saturating_sub(last) >= 100_000 && LAST.compare_exchange(last, now, Relaxed, Relaxed).is_ok() {
        crate::readout!(
            "[TSEAM] ev=gap_clamped n={n} gap=[{gap_start},{gap_end}] win=[{win_start},{win_end}] t_us={now}"
        );
    }
}
