# ADR-0078: One framing for every source symbol, sized by its sender

## Status: Proposed (format decided by the owner, 2026-11-17: option A, one framing; points 4–5 await the owner). Design: `docs/derivations/f2-framing-design.md`.

## Context

Engine defect F2 (status): a source symbol is written in one of two framings, and the receiver learns which one only from `WindowStart.packed`.
- The two framings are packed (BE lengths, several packets) and one-packet (an LE length).
- `WindowStart.packed` is a one-shot, unreliable datagram. If it is lost, every Realtime symbol is misread for the whole connection.
- The receiver still acks what it could not unframe, so ARQ never notices.
- The receive-side choice is also a hint-keyed mode bit (NO-MODE-SWITCH).

Each side also decodes with its own hint's symbol size. The handshake sends a constant 1200, so peers with different hints cut each other's symbols.

The rejected option (B) was a per-symbol "packed" bit. It keeps two formats and moves the mode bit into every frame.

## Decision

1. **Every source symbol, at every hint, is one format:** `[u16 LE len][bytes]`, repeated. A zero length or the symbol's end terminates it, and zero padding is the terminator.
   - `framing::put_packet` is the only writer and `framing::read_packets` the only reader.
   - A one-packet symbol is byte-identical to the v9 one-packet frame.
2. **`window_packed` and `WindowStart.packed` are removed.** Packing becomes a sender-only choice. Its hint keying stays as listed sender debt (status §3.3).
3. **Wire v10.** A v9 peer is refused at the handshake. The planned receiver-timestamp field moves to v11.
4. **The symbol size belongs to the sender, per direction.**
   - `Handshake.symbol_size` carries the sender's real size, and the receiver decodes with it.
   - A peer whose paths disagree on the size, or whose size is outside [72, 1200], is refused at connect.
   - There is no joint constraint between the sides, so a future S(δ) stays continuous.
   - Option (i), refusing any mismatch, is the fallback.
5. **A symbol the receiver cannot unframe** (a length overrun, or no packet from non-zero bytes) is a protocol violation.
   - It is counted in `UNDELIVERABLE_SYMBOLS`, logged as `[UNFRAMABLE]`, never delivered, and it closes the tunnel.
   - That is one behaviour at every δ and ρ.
   - After point 4, only an engine bug can produce one.

## Consequences

- **Bulk and Auto data bytes are unchanged.** `hotpath_pin` passes without re-capture at the v9 framing commit; the v10 commit then re-captures it for the version field only. The two commits are never pinned, launched or merged apart, and never squashed.
- **Realtime packed symbols** keep their size; only the byte order of each length field flips.
- **The receiver no longer depends on any control message** to read data. A lost `WindowStart` is harmless.
- **Size is no longer a per-side assumption.** Mixed-hint pairs still differ in the receiver's default ρ, which nothing on the wire checks (audit §6). That is out of scope here.
- **The release binary changes, so F2 rides the stacked §41c guard** (owner decision 5, 2026-11-16).
- **L1 rows with `undeliv` > 0 or an `[UNFRAMABLE]` line are voided** (`VOID-F2`), never scored as DNF.
- `extract_window_packet`, the BE `extract_packets` and `frame_end` are deleted, and integration tests migrate to `read_packets`.
