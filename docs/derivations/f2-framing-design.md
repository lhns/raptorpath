# F2 fix design: one framing for every symbol (owner decision A, 2026-11-17)

Design only; nothing here is built. Defect: status "Engine defect F2". Audit:
`f2-oneshot-audit.md`. ADR: `docs/adr/0078-one-symbol-framing.md`. Line numbers
are in `raptorpath/src/` at `f2-design` (main `d2cae698` + `f2-prep` `a8c7debe`).

## 1. The one framing

A source symbol is `symbol_size` bytes:

```text
symbol  = packet* terminator? zero-pad
packet  = len:u16 little-endian (1..=symbol_size-2)  bytes[len]
end     = a length field of 0, or fewer than 2 bytes left in the symbol
```

- **Zero padding is the terminator.** Every writer zero-fills the symbol, and
  the decoder zero-pads a short source (`fec/unified.rs:402`). So no separate
  sentinel write is needed, and `frame_end` goes.
- **One writer:** `framing::put_packet(dst: &mut [u8], pkt: &[u8]) -> usize`
  writes `[len LE][pkt]` and returns `2 + len`. Both producers call it:
  - `frame_window_packet` (one packet; keeps its `vec![0; S]` allocation, its
    `S-2` truncation and its `Frm` seam);
  - `SymbolPacker` (several packets; keeps its capacity `S-2` and its `S-4`
    oversize truncation).
- **One reader:** `framing::read_packets(sym: &[u8]) -> Result<Vec<Vec<u8>>, Unframable>`
  walks the chain (the old `extract_packets` loop, read LE). It returns `Err` when:
  - a length overruns the symbol (today's `extract_packets` silently drops that tail), or
  - no packet comes out of a symbol that has a non-zero byte (today's counter test).

  An all-zero symbol is `Ok(vec![])`, the frame of an empty TUN packet. Non-zero
  bytes after the terminator are not checked, because that would be a pad scan per
  symbol for a case no writer produces.
- **Removed:** `extract_window_packet`, the BE `extract_packets`, `frame_end`, and
  the `packed: bool` argument of `net::extract_window_packets` (`net/mod.rs:1372`).
  The latter becomes a thin wrapper that counts `Err` in `UNDELIVERABLE_SYMBOLS`.
- **Why little-endian:**
  - LE keeps today's one-packet symbol **byte-identical** (`framing.rs:75`
    writes LE). That symbol is every Bulk and Auto source symbol, and all the
    guarded ledgers.
  - Symbol-level fields are already LE (repair header, `unified.rs:414-416`).
    Only the message envelope (magic, version) is BE.
  - Realtime's packed symbols are the broken ones, so they change either way.
- **A one-packet symbol is a packed symbol with one packet.** So "packed" stops
  being a format. It is only how many packets a sender puts in a symbol.

## 2. `window_packed` and `WindowStart.packed`

- Delete `window_packed` (`receiver.rs:138`), its setter (`:2152-2155`) and its four
  reads (`:1016`, `:1524`, `:1621`, `:1756`). The receiver has one code path.
- Remove the field `packed` from `ControlMessage::WindowStart`
  (`transport/protocol.rs:442-449`). Keep the variant, `{ symbol_size, backend }`
  (log only, `control_msg.rs:122`):
  - a removed variant would renumber every later fixint tag for no gain;
  - the repro's seam (`io_owner.rs:1325-1338`) needs a `WindowStart` to drop.

  After the fix, a lost `WindowStart` costs nothing.
- Old peers: the wire bumps to v10, and a v9 peer is refused at the handshake
  (§3). A v9 peer never sees a v10 `WindowStart`.

## 3. Protocol version v10

- `PROTOCOL_VERSION` 9 → 10 (`protocol.rs:92`), with a "v10:" entry in the doc
  history above it. The entry covers the one LE framing, `WindowStart` without
  `packed`, and the fact that v9 Realtime symbols are BE-packed.
- **Coordination:** no ref (local or `github`) has v10 in code. The receiver
  timestamp is planned only in status §42 (a) and §43, not in ADR-0072. F2 takes
  v10, and the timestamp field becomes v11.
- **Refusal is already in place.** Both `Handshake::deserialize` (`protocol.rs:310-313`)
  and `WireMessage::deserialize` (`:595-600`) bail on any version mismatch:
  - a v10 client against a v9 server: the server bails in `accept_handshake`
    before replying (`io_owner.rs:778`), so the client sees a stream error, not
    the mismatch text;
  - a v9 client against a v10 server: the same, mirrored;
  - either way the connect fails and no data symbol is exchanged. Compact DATA
    frames carry no version, and they follow only a successful handshake.

## 4. Packing becomes a sender-side choice only

- **Yes.** With one framing the receiver never needs to know whether a sender
  packed. `use_packing` (`sender_policy.rs:678`) no longer crosses the wire.
- **It stays hint-keyed sender debt:** `protocol_hint == Realtime`, as listed in
  status §3.3. That includes `emit_batch_on`'s `!use_packing` (`:737`). This fix
  removes the receive-side mode bit and the one-shot dependency. It does not
  make the sender continuous.
- **The continuous form, named and not designed:** a δ-priced flush deadline
  τ(δ) for `SymbolPacker`. τ = 0 is one packet per symbol, the Bulk end; that is
  the same writer and the same format under (A). It needs a paper derivation
  first (FORMULA-FIRST), so it is not trivial and not part of F2.

## 5. Defect (b): a symbol the receiver cannot unframe

**The brief's premise ("no ack, so ARQ repairs it") does not hold in this code:**
- A seq enters `received_seqs`, and with it the SACK and the cumulative-ack
  input, at decoder output (`receiver.rs:1466-1478`). That is before any extract
  site, so by the time a symbol is unframed it is already SACK-visible.
- All three decoders dedup a source by seq (`unified.rs:384-400`,
  `rlc_window.rs:463-472`, `generation.rs:1091-1100`). A retransmit of an
  absorbed seq never re-emerges.
- After (A), the only remaining causes are deterministic:
  - an encode or decode bug;
  - peers with different `symbol_size`. Each side derives it from its own local
    hint (`net/mod.rs:181-186`, `:1171`). The handshake sends a constant 1200
    (`io_owner.rs:794`) and nobody checks it, so a Bulk peer against a Realtime
    peer truncates 1200-byte sources to 512 (`unified.rs:403`) and every length
    overruns.

  A retransmit carries the same bytes, so ARQ cannot repair either cause.

Not acking would mean validating before `:1477`, so before the decoder. Even
then, the hole stays permanent at ρ = 1 and becomes a silent loss at ρ < 1.

**Decision (recommended; owner to confirm): fail closed, one behaviour at every
ρ and δ.** At each of the four extract sites, a `read_packets` error:
1. adds one to `UNDELIVERABLE_SYMBOLS` (already printed as `undeliv=` on `[CTLD]`);
2. logs one `error!` with seq, symbol length and the first 4 bytes ("unframable
   symbol: peer framing or symbol_size disagrees; closing tunnel");
3. `break 'recv`. That is the receiver's existing fatal exit (`receiver.rs:1060`,
   `:1529`, `:1626`): the exit flush, then the receiver task ends, and the
   tunnel shuts down (`net/mod.rs:1341`).

The symbol is never delivered and never counted as delivered:
- the in-order site reads before `blk.lat.note_delivery` (moved above it,
  `:1610`);
- `next_expected` is not advanced past it;
- no further ack is sent.

Acks already sent for it are moot, because the connection ends. A smoke that
reads `undeliv=` > 0 now also sees a dead tunnel with an error line, instead of
a live one with healthy counters.

The rejected alternative is to keep the tunnel up, skip the seq and leave it out
of `received_seqs`. That gives a permanent stall at ρ = 1 and a silent loss at
ρ < 1, which is two outcomes and no repair.

## 6. Costs, from code

- **Bytes per symbol, Bulk and Auto: +0.** `symbol_size` is 1200
  (`net/mod.rs:181-186`) and the TUN MTU is `symbol_size - 4` = 1196
  (`net/mod.rs:657`). A one-packet symbol is `[len LE][≤1196 B][zeros]`, which
  is today's `frame_window_packet` output byte for byte. `hotpath_pin` digests
  that output at T = 1200 over lengths 0…1500, truncation included, and it
  must pass **without re-capture**. Repair symbols are combinations of
  identical sources, so they are identical too.
- **Bytes per symbol, Realtime: +0.** S = 512 and MTU 508. Packer capacity
  (`S-2`), packing decisions and the zero tail are unchanged. Only each 2-byte
  length field's byte order flips (BE → LE).
- **Largest packet fits:**
  - Bulk/Auto: 2 + 1196 = 1198 ≤ 1200, with the 2-byte zero terminator inside.
  - Realtime: 2 + 508 = 510 = packer capacity, then the terminator, 512.
  - The compact frame and the 1350 floor (ADR-0055) are untouched.
- **Control bytes, all hints:**
  - `WindowStart` loses its 1-byte bincode bool; it is one datagram per path per
    connection;
  - the 4-byte version field reads 10 (same width).
- **The §41c guard (owner question).**
  - F2's status entry keys the guard on Bulk wire bytes. Those are unchanged on
    the data plane, and the proof is `hotpath_pin` unchanged.
  - ADR-0076 (`:45`) and the F1 precedent (status, F1 merge rule) key it on any
    engine-byte change. The release binary changes, so the standing rule asks
    for a guard.
  - The case for an F1-style waiver: `hotpath_pin` and `decision_trace_pin` are
    unchanged, no law is touched, and Bulk's code path changes only in the
    reader (below).
- **No guard covers Realtime.** That is where behaviour does change (it is the
  fix). §51's E block is the field check.
- **CPU per symbol, expected:**
  - Sender: ±2 ns. Same allocation, a `put_packet` call instead of an inline
    copy, inlined in release.
  - Receiver, Bulk: +1–3 ns. One more u16 load and compare, to read the zero
    terminator. The outer `Vec` allocation is the same as today's
    `Option::into_iter().collect()`.
  - At c1s (≈ 90 k symbols/s, status §41c readout), +5 ns ≈ 0.45 ms of CPU per
    second, about 0.05 % of a core. The guard's c1s CPU resolution is ≈ 2 %.
- **Host ns bench to specify** (the `nsbench41c` pattern): an ignored release
  test `framing_ns`.
  - BASE inlines main's `frame_window_packet_inner` and `extract_window_packet`.
    CAND calls `frame_window_packet` and `read_packets`.
  - Points: S = 1200 at lengths 64, 600 and 1196; S = 512 with 3 × 160 packed
    (CAND only, BE vs LE).
  - 2 000 000 calls per point, 5 alternating BASE/CAND pairs, `taskset -c 2`.
  - Report median ns/call with [min–max].

## 7. Test plan

Red first (each fails on `f2-prep`):
1. **Endianness, by byte literal:** `read_packets(&[3,0, 0xAA,0xBB,0xCC, 1,0, 0xDD, 0,0, …zeros to 16])`
   gives `[[AA,BB,CC],[DD]]`, and `put_packet` writes exactly `[3,0,AA,BB,CC]`.
   On main the BE reader reads 768 and yields nothing.
2. **Round trips, absolute, at S = 1200 and S = 512:**
   - a one-packet symbol: `frame_window_packet` → `read_packets` = `[pkt]` for
     lengths 1, 508, 1196 and `S-2`;
   - a packed symbol: the `SymbolPacker` output of 3 × 160 → 3 packets in order;
   - `SymbolPacker` with one 100-byte packet then `flush` is byte-equal to
     `frame_window_packet` of it. The two writers agree.
3. **`Unframable`:**
   - a length overrun after a valid first packet gives `Err`, and the wrapper
     counts it once;
   - a non-zero symbol with no packet gives `Err`;
   - all zeros gives `Ok([])` and is not counted.
   - This replaces `net/tests.rs:6563-6586`.
4. **The repro, un-ignored and tightened:** `tests/f2_window_start_lost.rs` also
   asserts `raptorpath::net::undeliverable_symbols() == 0` after the run. App
   delivery is already asserted: the client returns Ok only after the object is
   acked and fully received.
5. **Fail closed executes** (a new own-binary test, `tests/f2_unframable_closes.rs`).
   A seam `TEST_CORRUPT_SOURCE` beside `TEST_DROP_WINDOW_START` (`io_owner.rs:61`)
   rewrites the first payload u16 of the next non-repair compact Data datagram to
   0xFFFF. Then, at Bulk and at Realtime:
   - `undeliverable_symbols() >= 1`, and the seam was spent;
   - the server's receive task ends within 5 s. The run returns, or the client
     fails, rather than timing out against a live tunnel.
6. **Version:**
   - `protocol_version_is_nine` → `_is_ten`;
   - `handshake_refuses_an_old_peer_at_v9` → `_at_v10`, with olds `[9, 8]`, plus
     data-path refusal;
   - a `WindowStart { symbol_size, backend }` round trip.

Pins:
- `hotpath_pin`: **unchanged** (this is the Bulk byte-identity witness; a
  re-capture is a design failure, not a fix).
- `decision_trace_pin`: unchanged (no framing input).
- wasm goldens: untouched. `raptorpath-wasm`, `-core`, `-math` and the
  visualizer have no framing or packing code (grep).

Existing tests to migrate (not new behaviour):
- `net/framing/tests.rs`: BE literals become LE (`:85`); the window and packer
  tests read through `read_packets`.
- `tests/fec_window_test.rs`, `tests/fec_backend_switching_test.rs`: they import
  `extract_window_packet`; use `read_packets(..).ok()` and take the first packet.
- `tests/f2_undeliverable_counter.rs`: header cross-reference only.

## 8. Implementation brief (Sonnet worker)

**Branch:** off `f2-design` in its own worktree. Commit there, and do not push
unless told.

**Decided:** everything in §1–§7, including the fail-closed policy of §5. If the
owner reverses §5, only step 4's error arm changes.

**Do not:**
- touch `use_packing`'s keying, `symbol_size_for_hint`, the TUN clamp,
  `SymbolPacker`'s capacity or truncation rules, or the handshake's `symbol_size`;
- touch the wasm, core or math crates;
- re-capture `hotpath_pin` or `decision_trace_pin`.

**Steps, red first:**
1. **Tests that fail on the base.**
   - Write §7 tests 1–3 in `net/framing/tests.rs` and `net/tests.rs` against the
     new names; they fail to compile, or fail.
   - Un-ignore and tighten the repro (§7.4) and run it: red.
   - Add the seam and test §7.5: red (today the tunnel stays up).
   - Edit the version pins (§7.6): red.
2. **`net/framing.rs`.**
   - Add `put_packet` and `read_packets` / `Unframable`. Remove `frame_end`,
     `extract_packets` and `extract_window_packet`.
   - `frame_window_packet_inner` keeps `vec![0u8; size]` and the `S-2` clamp, and
     calls `put_packet`. `SymbolPacker::push`/`emit` append via `put_packet` and
     zero-fill.
   - Update the module and `SymbolPacker` docs (LE, zero padding as terminator).
3. **`net/mod.rs`.** `extract_window_packets(data) -> Result<Vec<Vec<u8>>, Unframable>`:
   one `read_packets` call, and `fetch_add` on `Err`. Fix the comment at `:650`
   and the `UNDELIVERABLE_SYMBOLS` doc (`:1384-1390`, now "the receiver closes the
   tunnel").
4. **`net/receiver.rs`.**
   - Delete `window_packed`.
   - At `:1016`, `:1524`, `:1621` and `:1756`: `Ok(p)` goes on as today. `Err`
     logs one `error!` (§5) and does `break 'recv`.
   - At the in-order site, call the reader before `blk.lat.note_delivery`.
   - At the hold-expiry sites, break before `next_expected` moves.
   - Delete the `WindowStart` arm at `:2152-2155`, and update the module doc's
     list of fatal exits (`:21-24`, `:2189`).
5. **`transport/protocol.rs`.** Drop `packed`, set v10 with its history entry,
   and edit the pins. In `net/control_msg.rs:122`, drop `packed` from the pattern.
   In `net/mod.rs:1884`, drop it from the send.
6. **`transport/io_owner.rs`.** Add `TEST_CORRUPT_SOURCE` (one relaxed load when
   unarmed, the `test_drop_window_start` pattern).
7. **Migrate the integration tests** listed in §7.
8. **Add the `framing_ns` bench** (§6), ignored. Run it once on the host,
   release, and report the numbers; do not tune.

**Done:**
- `cargo test -p raptorpath` is green, with the lib and every integration test,
  including the un-ignored repro, `f2_undeliverable_counter` and the new
  fail-closed test;
- `hotpath_pin` and `decision_trace_pin` pass unchanged;
- `cargo clippy -p raptorpath` adds no new warnings;
- `grep -rn "window_packed\|extract_window_packet\|to_be_bytes" raptorpath/src/net/framing.rs raptorpath/src/net/receiver.rs`
  finds nothing framing-related.

Host cargo runs under the mutex
(`L=/c/Users/pierr/AppData/Local/Temp/claude/cargo-host.lock; until mkdir "$L" 2>/dev/null; do sleep 20; done; …; rmdir "$L"`),
with `CARGO_BUILD_JOBS=1` and your own `CARGO_TARGET_DIR`.

**Report:**
- the sha;
- the red→green list;
- the `framing_ns` table;
- any site where the `Err` arm could not sit before a delivery record.

## 9. Owner questions

1. **Defect (b) policy:** fail closed (recommended, §5), or a permanent hole.
2. **The guard:** waive it F1-style on `hotpath_pin` and `decision_trace_pin`
   unchanged, or run a §41c guard. The binary changes, but Bulk data bytes do not.
3. **Recommended, not designed here:** carry the real `symbol_size` in the v10
   handshake and refuse a mismatch. It is the remaining reachable cause of an
   unframable symbol (§5), and it costs nothing extra if it rides the same bump.
   Today `local_handshake` has no config (`io_owner.rs:793-795`).
