//! SACK plumbing: the repair-request batch, gap computation from SACK
//! ranges, the per-path outstanding accounts (the `[DIAG] sout=`
//! attribution of the pooled outstanding to placement paths, kept under
//! `RWM_DIAG`; the `RWM_STORE_PATHS` cap itself scales by the live-path
//! count and does not read them), the SACK-clocked release and the
//! window-ack emission.

use super::*;

// ---------------------------------------------------------------------------
// WindowNack gap computation
// ---------------------------------------------------------------------------

/// The receiver-seat repair request as it crosses the task seam (paper
/// §7.6, wire v8). `(cause, spans)` where each span is
/// `(start, count, deficit)`: over `[start, start+count)` I need `deficit`
/// more independent equations.
///
/// `count = 1, deficit = 1` is the per-seq copy request, so the shipped
/// machine is this message's `m = 1` corner and not a different message.
/// `cause` is the `[FCAUSE]` class vocabulary carried as a plain `u8` — a
/// label for the counters, as `FireCause` is on the gap channel; nothing in
/// the serving loop branches on it.
pub type RepairRequestBatch = (u8, Vec<(u64, u16, u32)>);

/// The collision seam's predicate (paper §7.6), a pure function so it can be
/// asserted.
///
/// The request law lives on the plain reliable window and nowhere else:
/// generation coding has no per-seq layer to suppress and answers a
/// different vocabulary already (`GenerationDeficit`), and block mode has no
/// window at all. `false` means the gap producer keeps its shipped arming
/// and no `RepairRequest` is ever constructed.
pub fn request_law_armed(
    window_mode: bool,
    reliable: bool,
    generation: bool,
    gate: bool,
) -> bool {
    window_mode && reliable && !generation && gate
}

/// Compute gap ranges from a set of received sequences in a window.
/// Returns Vec<(start, end)> of inclusive ranges of missing sequences.
pub fn compute_gap_ranges(
    received: &BTreeSet<u64>,
    window_start: u64,
    window_end: u64,
) -> Vec<(u64, u64)> {
    let mut gaps = Vec::new();
    let mut expected = window_start;

    for &seq in received.range(window_start..=window_end) {
        if seq > expected {
            gaps.push((expected, seq - 1));
            if gaps.len() >= MAX_NACK_GAPS {
                return gaps;
            }
        }
        expected = seq + 1;
    }

    // Trailing gap
    if expected <= window_end && gaps.len() < MAX_NACK_GAPS {
        gaps.push((expected, window_end));
    }

    gaps
}

/// Invert SACK ranges into missing-seq gaps.
///
/// `sack_ranges` are inclusive, ascending, disjoint ranges of seqs the
/// receiver has beyond the cumulative point `received_up_to`. Every seq
/// between the cumulative point and a sacked range that is not itself
/// sacked is missing at the receiver. (Seqs above the last sacked range
/// are not reported — they may simply still be in flight.)
pub fn sack_to_gaps(received_up_to: u64, sack_ranges: &[(u64, u64)]) -> Vec<(u64, u64)> {
    let mut gaps = Vec::new();
    // Seq 0: `received_up_to = 0` is advertised both after seq 0 was
    // delivered and while nothing is delivered (the receiver's frontier
    // starts at 0). Under prefix delivery a received-but-undelivered seq 1
    // (a SACK range starting at 1) is only possible if seq 0 is missing, so
    // that report also names seq 0 — no wire change. (Seq 0 lost together
    // with seq 1 is reported one round later, once seq 1 arrives; the tail
    // sweep still backstops it.)
    let seq0_missing =
        received_up_to == 0 && sack_ranges.first().is_some_and(|&(start, _)| start == 1);
    let mut expected = if seq0_missing { 0 } else { received_up_to + 1 };
    for &(start, end) in sack_ranges {
        if start > expected {
            gaps.push((expected, start - 1));
            if gaps.len() >= MAX_NACK_GAPS {
                return gaps;
            }
        }
        expected = expected.max(end.saturating_add(1));
    }
    gaps
}

/// Charge one retained seq to its placement path's outstanding account —
/// the per-path store-attribution gauge behind `[DIAG] sout=`. Called in
/// lockstep with the `sent_store` insert; paired with
/// [`percap_release_seq`] (SACK/OOO removal) and
/// [`percap_release_cumulative`] (frontier advance) — release is by ack
/// only, the retention contract.
pub fn percap_charge(
    acct: &mut BTreeMap<u64, u32>,
    out: &mut std::collections::HashMap<u32, usize>,
    seq: u64,
    path: u32,
) {
    if acct.insert(seq, path).is_none() {
        *out.entry(path).or_insert(0) += 1;
    }
}

/// Release one seq from its account on OOO (SACK-range) removal from the
/// retention store. Idempotent: a seq not (or no longer) in the account
/// map releases nothing — so SACK + cumulative can never double-release.
pub fn percap_release_seq(
    acct: &mut BTreeMap<u64, u32>,
    out: &mut std::collections::HashMap<u32, usize>,
    seq: u64,
) {
    if let Some(pid) = acct.remove(&seq) {
        if let Some(o) = out.get_mut(&pid) {
            *o = o.saturating_sub(1);
        }
    }
}

/// Release every account entry at or below the cumulative ack (the
/// in-order frontier advance — the `sent_store.split_off(ack+1)` twin).
pub fn percap_release_cumulative(
    acct: &mut BTreeMap<u64, u32>,
    out: &mut std::collections::HashMap<u32, usize>,
    ack: u64,
) {
    let keep = acct.split_off(&(ack + 1));
    for pid in acct.values() {
        if let Some(o) = out.get_mut(pid) {
            *o = o.saturating_sub(1);
        }
    }
    *acct = keep;
}

/// SACK-clocked store release (`RWM_STORE_SACK_RELEASE`, ADR-0060, paper
/// §6.3): mark every seq of the SACK range that is currently retained as
/// released — uncounted from the flow-control outstanding, so the send
/// window opens at path rate instead of frontier latency — while the
/// `sent_store` entry (the only payload copy; `retransmit_buffer` is
/// metadata-only) and every ARQ/recovery map stay untouched until the
/// cumulative frontier passes the seq.
///
/// This law never removes anything: pruning `sent_store` on SACK would
/// destroy the only copy of a received-then-evicted symbol and wedge the
/// in-order stream. A released symbol remains retransmittable (the NACK path
/// serves from `sent_store.get`); worst case under receiver eviction is a
/// wasted retransmit, not a wedge. The race-ahead is bounded because
/// never-received/evicted seqs are never SACKed and still count.
///
/// Returns the seqs newly released by this call (for per-path account
/// release); already-released seqs are skipped — no double-release.
pub fn sack_release_mark<V>(
    sent_store: &BTreeMap<u64, V>,
    released: &mut BTreeSet<u64>,
    start: u64,
    end: u64,
) -> Vec<u64> {
    let mut newly = Vec::new();
    for (&seq, _) in sent_store.range(start..=end) {
        if released.insert(seq) {
            newly.push(seq);
        }
    }
    newly
}

/// The cumulative-frontier twin of [`sack_release_mark`] (the
/// `sent_store.split_off(&(ack+1))` pattern): drop released marks at or
/// below the ack — those slots are now fully freed (payload gone from the
/// store, mark gone from the released set).
pub fn sack_release_prune(released: &mut BTreeSet<u64>, ack: u64) {
    *released = released.split_off(&(ack + 1));
}

/// Effective outstanding under the release law: retained minus released.
/// With the gate off the released set is empty and this is exactly
/// `store_len`.
pub fn sack_release_outstanding(store_len: usize, released: usize) -> usize {
    store_len.saturating_sub(released)
}

/// Ack-merge (`RWM_ACK_MERGE`, default ON): the receiver data arm's
/// WindowAck emission decision, as a pure function of the two predicates and
/// the gate. Returns `(emit, advertise)`.
///
/// The separation is the safety argument of the merge:
///
/// - `advertise` is `cumulative_advanced || gap_report_due`, and it alone
///   decides whether the ack carries SACK ranges and pushes the gap/hole
///   timers. So `GAP_ACK_MIN_INTERVAL` still rate-limits gap reports at its
///   own cadence and the depth-16 nack/sack `try_send` channels see no new
///   pressure — a merge-only ack carries counters and an echo, never a gap
///   report.
/// - `emit` decides only whether a datagram goes out. Under the merge it is
///   unconditional, because this ack also carries the suppressed `Ack`'s
///   payload and must keep the `Ack`'s once-per-data-message cadence.
///
/// With the gate off, `emit == advertise`.
pub fn window_ack_emission(
    cumulative_advanced: bool,
    gap_report_due: bool,
    ack_merge: bool,
) -> (bool, bool) {
    let advertise = cumulative_advanced || gap_report_due;
    (advertise || ack_merge, advertise)
}

/// Receiver-side SACK encoding: the inclusive, ascending, disjoint ranges
/// of seqs the receiver has in (`delivered`, `seen`] — the inverse of
/// [`sack_to_gaps`]. Shared by the data-arm WindowAck and the reliable
/// window's stalled-hole re-advertisement.
pub fn received_sack_ranges(
    received: &BTreeSet<u64>,
    delivered: u64,
    seen: u64,
) -> Vec<(u64, u64)> {
    let gaps = compute_gap_ranges(received, delivered, seen);
    let mut sack_ranges = Vec::new();
    let mut cursor = delivered + 1;
    for &(gap_start, gap_end) in &gaps {
        if cursor < gap_start {
            sack_ranges.push((cursor, gap_start - 1));
        }
        cursor = gap_end + 1;
    }
    if cursor <= seen {
        sack_ranges.push((cursor, seen));
    }
    sack_ranges
}
