//! SACK plumbing: the repair-request batch, gap computation from SACK
//! ranges, the per-path outstanding accounts (the `[DIAG] sout=`
//! attribution of the pooled outstanding to placement paths, kept under
//! `RWM_DIAG`; the `RWM_STORE_PATHS` cap itself scales by the live-path
//! count and does not read them), the SACK-clocked release and the
//! window-ack emission.

use super::*;

// ---------------------------------------------------------------------------
// Gap computation and the SACK encoding (no WindowNack exists on this wire)
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
/// different vocabulary already (`GenerationDeficit`). `false` means the
/// gap producer keeps its shipped arming and no `RepairRequest` is ever
/// constructed.
pub fn request_law_armed(reliable: bool, generation: bool, gate: bool) -> bool {
    reliable && !generation && gate
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
/// receiver has at or above the cumulative point `next_expected` (wire v9:
/// the count of the delivered prefix, so `next_expected` itself is the first
/// seq not delivered, and `0` means nothing delivered). Every seq from the
/// cumulative point up to a sacked range that is not itself sacked is
/// missing at the receiver. (Seqs above the last sacked range are not
/// reported — they may simply still be in flight.) A lost seq 0 needs no
/// special case in v9: `next_expected = 0` names it like any other hole.
pub fn sack_to_gaps(next_expected: u64, sack_ranges: &[(u64, u64)]) -> Vec<(u64, u64)> {
    let mut gaps = Vec::new();
    let mut expected = next_expected;
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

/// Release every account entry below the cumulative point `next_expected`
/// (the in-order frontier advance — the `sent_store.split_off(&next_expected)`
/// twin; wire v9 count semantics).
pub fn percap_release_cumulative(
    acct: &mut BTreeMap<u64, u32>,
    out: &mut std::collections::HashMap<u32, usize>,
    next_expected: u64,
) {
    let keep = acct.split_off(&next_expected);
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
pub fn sack_release_mark<S: super::seq_ring::RetainedSeqs>(
    sent_store: &S,
    released: &mut BTreeSet<u64>,
    start: u64,
    end: u64,
) -> Vec<u64> {
    let mut newly = Vec::new();
    sack_release_mark_into(sent_store, released, start, end, &mut newly);
    newly
}

/// [`sack_release_mark`] into the caller's scratch `newly` (cleared first),
/// so the per-SACK-range drain allocates nothing in steady state.
pub fn sack_release_mark_into<S: super::seq_ring::RetainedSeqs>(
    sent_store: &S,
    released: &mut BTreeSet<u64>,
    start: u64,
    end: u64,
    newly: &mut Vec<u64>,
) {
    newly.clear();
    sent_store.for_each_seq_in(start, end, |seq| {
        if released.insert(seq) {
            newly.push(seq);
        }
    });
}

/// The cumulative-frontier twin of [`sack_release_mark`] (the
/// `sent_store.split_off(&next_expected)` pattern): drop released marks below
/// the cumulative point — those slots are now fully freed (payload gone from
/// the store, mark gone from the released set).
pub fn sack_release_prune(released: &mut BTreeSet<u64>, next_expected: u64) {
    *released = released.split_off(&next_expected);
}

// ---------------------------------------------------------------------------
// Wire v9: the received-above-frontier count (plan 2c)
// ---------------------------------------------------------------------------

/// What one SACK-bearing WindowAck carries to the local window sender
/// across the `sack_tx` seam: the ranges (per-seq: the release marks, Copa
/// attribution and per-path release) and the v9 `(next_expected,
/// received_above)` pair the store gate reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SackReport {
    pub next_expected: u64,
    pub received_above: u32,
    pub ranges: Vec<(u64, u64)>,
}

/// The v9 received-above-frontier evidence the sender holds: the newest
/// `(next_expected, received_above)` pair it has seen.
///
/// "Newest" is lexicographic: acks from different paths land out of order,
/// the receiver's frontier only advances, and at a fixed frontier the
/// received set above it only grows (the receiver never prunes above its
/// frontier) -- so the larger pair is always the later reading. `Default`
/// is the no-report state and yields no evidence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct AboveReport {
    pub next_expected: u64,
    pub received_above: u32,
}

impl AboveReport {
    /// Fold one landed report in (lexicographic max).
    pub fn fold(&mut self, next_expected: u64, received_above: u32) {
        *self = (*self).max(AboveReport { next_expected, received_above });
    }
}

/// How many RETAINED seqs one `AboveReport` proves received -- a lower bound
/// on `|S ∩ received|`, where `S` is the retained store.
///
/// Derivation. The report says: every seq `< F` (= `next_expected`) was
/// delivered, and the receiver holds exactly `A` (= `received_above`)
/// distinct seqs in `[F, H]`, where `H` is the highest seq ever sent (the
/// receiver cannot hold a seq never sent). Received stays received (the
/// reliable receiver never evicts above its frontier). So:
///
/// - every retained seq `< F` is received: `retained_below = |S ∩ [0, F)|`;
/// - of the `A` received seqs in `[F, H]`, at most `not_retained = |[F, H]
///   \ S| = (H + 1 − F) − |S ∩ [F, H]|` can lie outside the store, so at
///   least `A − not_retained` of them are retained.
///
/// Hence `|S ∩ received| ≥ retained_below + max(0, A − not_retained)`. The
/// bound holds for ANY report, stale or not: a later cumulative prune moves
/// seqs out of `S`, which raises `not_retained` by exactly the seqs it
/// removed from `[F, H]`, and new sends raise `H` and `|S ∩ [F, H]|`
/// together. It never exceeds `|S|` (`A − not_retained ≤ |S ∩ [F, H]|`).
///
/// Operands: `retained_below = |S ∩ [0, F)|`, `retained_from = |S ∩ [F,
/// H]|`, `span_from = H + 1 − F` (0 when `H < F` or `S` is empty).
pub fn received_above_released(
    retained_below: usize,
    retained_from: usize,
    span_from: u64,
    received_above: u32,
) -> usize {
    let not_retained = span_from.saturating_sub(retained_from as u64);
    retained_below + (received_above as u64).saturating_sub(not_retained) as usize
}

/// **The store gate's released count (wire v9)** -- ONE always-computed
/// expression:
///
/// `released_for_gate = min(retained, max(sack_released, above_released))`
///
/// Both terms are lower bounds on the retained seqs the receiver provably
/// has: `sack_released` is the per-seq mark set (the union of every SACK
/// prefix the sender has seen, exact per seq), `above_released` is
/// [`received_above_released`] off the newest [`AboveReport`]. The max is
/// therefore still a lower bound, so the gate can never uncount a seq the
/// receiver lacks by more than the marks already could -- and past the
/// `MAX_SACK_RANGES` prefix, where the marks stop growing at a stalled
/// frontier, the count carries the rest and the gate converges to the
/// unreceived count. The clamp is the retained count (both terms are
/// already ≤ it; the clamp keeps the subtraction in
/// [`sack_release_outstanding`] total).
pub fn store_released_for_gate(retained: usize, sack_released: usize, above_released: usize) -> usize {
    sack_released.max(above_released).min(retained)
}

/// [`store_released_for_gate`] on the sender's own retained store. `S` is
/// pruned only as a prefix (`split_off(&next_expected)` at the cumulative
/// ack) and every reliable source seq enters it at send, so whenever `S` is
/// non-empty its last key is the highest seq ever sent -- the `H` of the
/// derivation. O(log n + |S ∩ [0, F)|); the second term is ~0 because the
/// cumulative prune runs every loop iteration.
pub fn store_gate_released<S: super::seq_ring::RetainedSeqs>(
    sent_store: &S,
    sack_released: usize,
    report: AboveReport,
) -> usize {
    let retained = sent_store.retained_len();
    let above = match sent_store.last_seq() {
        Some(h) => {
            let f = report.next_expected;
            let retained_below = sent_store.retained_below(f);
            let retained_from = retained - retained_below;
            let span_from = (h + 1).saturating_sub(f);
            received_above_released(retained_below, retained_from, span_from, report.received_above)
        }
        None => 0,
    };
    store_released_for_gate(retained, sack_released, above)
}

/// The receiver's `received_above` counter: the number of distinct seqs in
/// its received set at or above its cumulative point, maintained
/// incrementally so every WindowAck (one per data message under the ack
/// merge) pays O(frontier advance), not a scan of everything parked above a
/// stalled hole.
///
/// Contract: call [`Self::on_insert`] for every insert into the received
/// set, and [`Self::sync`] with the current cumulative point before reading
/// the count and before any prune of the set (the receiver prunes only below
/// its frontier, so a synced counter never loses a counted seq to a prune).
#[derive(Debug, Default)]
pub struct ReceivedAbove {
    frontier: u64,
    count: u64,
}

impl ReceivedAbove {
    pub fn new() -> Self {
        Self::default()
    }

    /// A seq was inserted into the received set (`newly` = it was not there).
    pub fn on_insert(&mut self, seq: u64, newly: bool) {
        if newly && seq >= self.frontier {
            self.count += 1;
        }
    }

    /// Move to the current cumulative point and return the count, saturated
    /// to the wire's `u32`.
    pub fn sync(&mut self, received: &BTreeSet<u64>, next_expected: u64) -> u32 {
        if next_expected > self.frontier {
            let passed = received.range(self.frontier..next_expected).count() as u64;
            self.count = self.count.saturating_sub(passed);
        } else if next_expected < self.frontier {
            self.count += received.range(next_expected..self.frontier).count() as u64;
        }
        self.frontier = next_expected;
        self.count.min(u32::MAX as u64) as u32
    }
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

/// The byte budget one SACK-bearing control datagram may use. A
/// `WindowAck` rides a single QUIC DATAGRAM (`send_control_datagram`: no
/// fragmentation; an oversize send is an `Err` its callers discard), so the
/// report must fit the smallest datagram any path can carry. That is
/// QUIC's guaranteed minimum 1200-byte UDP payload (RFC 9000 §14) — the
/// value quinn falls back to under `RWM_MTU_FLOOR=0` or a black-hole reset
/// without the floor — minus the same conservative 45-byte QUIC short-header
/// + PN + AEAD tag + DATAGRAM frame overhead `mtu_floor_covers_symbol_batch`
/// budgets against (quinn's own figure is ~33). With the ADR-0055 floor
/// (1350) the real room is 150 bytes larger still.
pub const SACK_DATAGRAM_BUDGET: usize = 1200 - 45;

/// Serialized size of a `WireMessage::Control(WindowAck)` with zero SACK
/// ranges: 4 magic + 4 version + 4 + 4 enum tags + 8 `next_expected` + 8
/// vec length + 8 echo + 4 jitter + 3×8 counters + 4 `received_above` (v9)
/// (bincode fixint). Pinned by
/// `window_ack_sack_size_fits_the_control_datagram`.
pub const WINDOW_ACK_BASE_BYTES: usize = 72;

/// Serialized size of one SACK range `(u64, u64)` (bincode fixint). Pinned
/// by the same test.
pub const WINDOW_ACK_BYTES_PER_RANGE: usize = 16;

/// Slack kept below [`SACK_DATAGRAM_BUDGET`] for header-length variation
/// (CID/PN length) and future fields.
pub const SACK_RANGE_MARGIN_BYTES: usize = 48;

/// Maximum SACK ranges one `WindowAck` carries: the largest `n` with
/// `72 + 16·n ≤ 1155 − 48`, i.e. ⌊(1155 − 72 − 48) / 16⌋ = 64 (1096 bytes on
/// the wire; the v9 `received_above` field did not move the cap). Derived from what one control datagram can carry, not from the
/// sender's per-report NACK cap ([`MAX_NACK_GAPS`]).
pub const MAX_SACK_RANGES: usize = (SACK_DATAGRAM_BUDGET
    - WINDOW_ACK_BASE_BYTES
    - SACK_RANGE_MARGIN_BYTES)
    / WINDOW_ACK_BYTES_PER_RANGE;

/// Receiver-side SACK encoding: the inclusive, ascending, disjoint ranges
/// of seqs the receiver has in [`next_expected`, `seen`] (`seen` = the
/// highest seq seen, inclusive; wire v9 count semantics for the cumulative
/// point) — the inverse of [`sack_to_gaps`]. Shared by the data-arm WindowAck and the reliable
/// window's stalled-hole re-advertisement.
///
/// At most [`MAX_SACK_RANGES`] ranges, and a capped report is a PREFIX of
/// the true received runs: the last range ends where the receiver's run
/// really ends, before the first hole the report cannot name. The wire's
/// only honest truncation is a prefix — [`sack_to_gaps`] reads everything
/// above the last range as "not reported (may be in flight)". Ending the
/// list at `seen` instead would claim every unreported hole as received:
/// the sender would release those slots, Copa would count them delivered
/// and nothing would ever NACK them (plan 2a). Below the cap the output is
/// the full received-run list, unchanged.
pub fn received_sack_ranges(
    received: &BTreeSet<u64>,
    next_expected: u64,
    seen: u64,
) -> Vec<(u64, u64)> {
    let mut sack_ranges: Vec<(u64, u64)> = Vec::new();
    if seen < next_expected {
        return sack_ranges;
    }
    for &s in received.range(next_expected..=seen) {
        match sack_ranges.last_mut() {
            Some((_, e)) if *e + 1 == s => *e = s,
            _ => {
                if sack_ranges.len() >= MAX_SACK_RANGES {
                    break;
                }
                sack_ranges.push((s, s));
            }
        }
    }
    sack_ranges
}
