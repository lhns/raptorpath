//! Sliding window FEC traits.
//!
//! These traits define the interface for continuous, streaming FEC that operates
//! over a sliding window of source symbols rather than fixed blocks.
//! They coexist with the block-based `FecEncoder`/`FecDecoder` traits.

use bytes::Bytes;

use super::traits::WireSymbol;

/// Sliding window encoder — continuously accepts source symbols and emits repair.
pub trait WindowEncoder: Send {
    /// Add the next source symbol to the window. Returns it as a WireSymbol
    /// ready for transmission.
    fn add_source(&mut self, data: &[u8]) -> WireSymbol;

    /// Generate one repair symbol covering the current window.
    fn generate_repair(&mut self) -> WireSymbol;

    /// Generate one coded symbol for the SPECIFIC generation anchored at
    /// `anchor` (= its `window_start`), bypassing the encoder's proactive
    /// per-generation budget. This is the sender arm of the per-generation
    /// deficit-feedback loop (§16.3): the receiver reports how many more coded
    /// symbols each frontier generation needs, and the sender emits exactly that
    /// residual for the named generation — recovery that is bounded (the deficit)
    /// and targeted (the stalled generation), replacing the feedback-free cap.
    /// Returns `None` if the generation is not retained or not yet codeable
    /// (e.g. not sealed). Non-generation encoders have no stable anchor and
    /// return `None`.
    fn generate_repair_for(&mut self, anchor: u64) -> Option<WireSymbol> {
        let _ = anchor;
        None
    }

    /// Generate one repair symbol coded over the SPECIFIC seq range
    /// `[start, start + count)` (proactive-frontier repair). Unlike
    /// `generate_repair` — which codes over the whole current (leading) window
    /// and therefore entangles a frontier hole with not-yet-received in-flight
    /// symbols at the receiver — this codes over a small TRAILING window at the
    /// cumulative-ack frontier, whose members are all already received except
    /// the hole(s), so the receiver's incremental GE solves the hole the instant
    /// the repair arrives (no ARQ round-trip, no future-symbol entanglement).
    /// Returns `None` if the full range is not currently retained (so the
    /// equation would be inconsistent with the receiver's coefficients).
    /// Default `None` for encoders without a retained per-seq window.
    fn generate_repair_range(&mut self, start: u64, count: u16) -> Option<WireSymbol> {
        let _ = (start, count);
        None
    }

    /// Current window span: (oldest_seq, newest_seq).
    /// Returns (0, 0) if the window is empty.
    fn window_span(&self) -> (u64, u64);

    /// Advance window: drop symbols older than `oldest_seq`.
    /// Called when the receiver acknowledges receipt up to this point.
    fn advance(&mut self, oldest_seq: u64);

    /// Number of source symbols currently in the window.
    fn window_size(&self) -> usize;

    /// Fix 3 (transport-substrate): move the PROACTIVE CODING floor to the
    /// generation containing `anchor_seq`, DECOUPLED from the retention floor
    /// (`advance`). By default the generation coder anchors its proactive
    /// round-robin at the retention floor = the in-order cumulative ack, so when
    /// one generation stalls on a hole the coder keeps re-coding it and reaches
    /// only `pipeline` generations past the STALLED in-order frontier — the
    /// ∝1/RTT serialization that makes FEC no better than ARQ at high RTT. This
    /// lets the caller advance the coding floor to follow the SEND frontier
    /// instead, so freshly-sent generations receive their upfront proactive
    /// budget while the stalled generation is left to bounded reactive recovery
    /// (its sources stay RETAINED — `advance` still keys on the in-order ack, so
    /// reliability is unchanged). Clamped to `[retention_floor, top+1]`.
    /// Default no-op (non-generation encoders have no stable generation anchor).
    fn set_code_base(&mut self, anchor_seq: u64) {
        let _ = anchor_seq;
    }

    /// Retrieve an exact source symbol by sequence number.
    /// Returns `None` if the symbol has been evicted from the window.
    fn get_source(&self, seq: u64) -> Option<WireSymbol> {
        let _ = seq;
        None
    }

    /// Whether the encoder currently has a generation it can usefully code a
    /// symbol for (a not-yet-provisioned or recovery-eligible generation).
    /// When false, `generate_repair` would only return an inert placeholder, so
    /// the caller must NOT emit — emitting inert symbols would consume the send
    /// budget and stall (they dedup to one at the decoder). Default: always true
    /// (the sliding-window encoders always have the current window to code).
    fn wants_coding(&self) -> bool {
        true
    }

    /// Signal that source intake is idle (no new source symbols are arriving —
    /// e.g. the object's tail, all bytes handed to the encoder). Generation
    /// coding uses this to allow recovery coding of the final, partial
    /// generation (which is never "sealed" to its full size); other encoders
    /// ignore it. Default: no-op.
    fn set_intake_idle(&mut self, _idle: bool) {
        let _ = _idle;
    }
}

/// Sliding window decoder — processes symbols as they arrive.
pub trait WindowDecoder: Send + Sync {
    /// Feed a received symbol (source or repair).
    /// Returns newly decodable source symbols as (seq, data) pairs.
    fn add_symbol(&mut self, symbol: &WireSymbol) -> Vec<(u64, Bytes)>;

    /// Advance window: discard state for symbols older than `oldest_seq`.
    fn advance(&mut self, oldest_seq: u64);

    /// Independent rank the decoder currently holds over the span
    /// `[start, start + count)` — the number of independent degrees of freedom
    /// it has for that generation (solved sources + un-resolved pivot rows whose
    /// pivot lies in the span). When this reaches `K_g` (= `count`) the whole
    /// generation decodes. This is the receiver arm of the per-generation
    /// deficit-feedback loop (§16.3): `deficit_g = K_g − rank_in(anchor, K_g)`.
    /// Default `0` for decoders without per-generation structure.
    fn rank_in(&self, start: u64, count: u64) -> u64 {
        let _ = (start, count);
        0
    }

    /// Diagnostic probe over the in-order frontier window `[frontier, horizon]`
    /// (proactive-frontier diagnosis, RWM_FDIAG). Returns
    /// `(holes, buffered_equations)`:
    ///   * `holes` — source seqs in the span neither received nor decoded (the
    ///     un-recovered degrees of freedom the frontier is waiting on).
    ///   * `buffered_equations` — independent coded equations the decoder already
    ///     holds whose pivot lies in the span (repair present but not yet enough
    ///     rank to solve). `buffered_equations == 0` with `holes > 0` means NO
    ///     proactive repair covering the frontier hole is buffered — recovery can
    ///     only come from a source retransmit (reactive ARQ). `0 < B < holes`
    ///     means repair is present but insufficient. `B >= holes` would already
    ///     have decoded (GE solves at full rank), so it is never observed stuck.
    /// Default `(0, 0)` for decoders without incremental-GE structure.
    fn frontier_probe(&self, frontier: u64, horizon: u64) -> (u64, u64) {
        let _ = (frontier, horizon);
        (0, 0)
    }

    /// Total symbols fed to this decoder.
    fn total_fed(&self) -> u64;

    /// Number of repair symbols fed to this decoder.
    fn repairs_fed(&self) -> u64 { 0 }

    /// Number of repair symbols that contributed to recovery (useful repairs).
    fn repairs_useful(&self) -> u64 { 0 }
}
