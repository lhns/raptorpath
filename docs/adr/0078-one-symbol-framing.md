# ADR-0078: One framing for every source symbol

## Status: Proposed (format decided by the owner, 2026-11-17: option A, one framing). Design: `docs/derivations/f2-framing-design.md`.

## Context

Engine defect F2 (status): a source symbol is written in one of two framings, and the receiver learns which one only from `WindowStart.packed`.
- The two framings are packed (BE lengths, several packets) and one-packet (an LE length).
- `WindowStart.packed` is a one-shot, unreliable datagram. If it is lost, every Realtime symbol is misread for the whole connection.
- The receiver still acks what it could not unframe, so ARQ never notices.
- The receive-side choice is also a hint-keyed mode bit (NO-MODE-SWITCH).

The rejected option (B) was a per-symbol "packed" bit. It keeps two formats and moves the mode bit into every frame.

## Decision

1. **Every source symbol, at every hint, is one format:** `[u16 LE len][bytes]`, repeated. A zero length or the symbol's end terminates it, and zero padding is the terminator.
   - `framing::put_packet` is the only writer and `framing::read_packets` the only reader.
   - A one-packet symbol is byte-identical to the v9 one-packet frame.
2. **`window_packed` and `WindowStart.packed` are removed.** Packing becomes a sender-only choice. Its hint keying stays as listed sender debt (status §3.3).
3. **Wire v10.** A v9 peer is refused at the handshake. The planned receiver-timestamp field moves to v11.
4. **A symbol the receiver cannot unframe** (a length overrun, or no packet from non-zero bytes) is a protocol violation. It is counted in `UNDELIVERABLE_SYMBOLS`, logged, never delivered, and it closes the tunnel. That is one behaviour at every δ and ρ. Owner to confirm.

## Consequences

- **Bulk and Auto data bytes are unchanged.** `hotpath_pin` passes without re-capture at the v9 framing commit; the v10 bump then re-captures it for the version field only.
- **Realtime packed symbols** keep their size; only the byte order of each length field flips.
- **The receiver no longer depends on any control message** to read data. A lost `WindowStart` is harmless.
- **The release binary changes.** The §41c guard or an F1-style waiver is the owner's call.
- **Peers configured with different hints** (different `symbol_size`, which the handshake does not check) now fail closed instead of misreading silently.
- `extract_window_packet`, the BE `extract_packets` and `frame_end` are deleted, and integration tests migrate to `read_packets`.
