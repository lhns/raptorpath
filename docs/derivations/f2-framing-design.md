# F2 fix design: one framing for every symbol (owner decision A, 2026-11-17)

Design only; nothing here is built. Defect: status "Engine defect F2". Audit:
`f2-oneshot-audit.md`. ADR: `docs/adr/0078-one-symbol-framing.md`. Line numbers
are in `raptorpath/src/` at `f2-design` (main `d2cae698` + `f2-prep` `a8c7debe`).
Round 2 (after review): §5 is new (symbol size per direction), §1.1 the false-positive
audit, §7 the guard, §8.5 the executes witness, §9 the harness witness and VM order.

## 1. The one framing

A source symbol is `symbol_size` bytes:

```text
symbol  = packet* terminator? zero-pad
packet  = len:u16 little-endian (1..=symbol_size-2)  bytes[len]
end     = a length field of 0, or fewer than 2 bytes left in the symbol
```

- **Zero padding is the terminator.** Every writer zero-fills the symbol, and
  every decoder hands out a full zero-padded symbol (§1.1). So no separate
  sentinel write is needed, and `frame_end` goes.
- **One writer:** `framing::put_packet(dst: &mut [u8], pkt: &[u8]) -> usize`
  writes `[len LE][pkt]` and returns `2 + len`. Both producers call it:
  - `frame_window_packet` (one packet; keeps its `vec![0; S]` allocation, its
    `S-2` truncation and its `Frm` seam);
  - `SymbolPacker` (several packets; keeps its capacity `S-2`, its `S-4`
    oversize truncation, its append-only `Vec` buffer, and the fresh
    `vec![0u8; size]` symbol it copies into at `emit`).
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

### 1.1 False positives, by reading (fail-closed makes each one fatal)

A false `Err` needs a symbol shorter than `2 + len` for some written `len`, or
stale non-zero bytes where a terminator should be. Neither is produced.

- **Every decoder hands the reader a full, zero-padded `symbol_size` buffer.**
  Nothing in them trims.
  - `unified`: sources go into `vec![0u8; S]` (`unified.rs:402-405`), recovered
    rows into `vec![0u8; S]` (`:456-458`), and the output is the `Known` buffer
    (`:254-257`).
  - `rlc_window`: sources go into `vec![0u8; S]` (`rlc_window.rs:480-483`). A
    single-unknown recovery writes into `vec![0u8; S]` (`:359-366`). A pivot
    recovery outputs `row.data` (`:437-442`), whose payload was built as
    `vec![0u8; S]` (`:516-518`).
  - `generation`: sources go into `vec![0u8; S]` (`generation.rs:1111-1113`).
    A recovered row is `width + S` bytes. `drain(..width)` then `truncate(ss)`
    (`:956-957`) leaves exactly S bytes, and `truncate` there only cuts back to S.
  - On the wire, the compact frame copies all of `sym.data` (`protocol.rs:171`,
    `:235`) and the parser takes the payload to the datagram end (`:274`). The
    decoders pad a short payload to S and truncate a long one to S (the
    symbol-size hazard of §5).
- **`SymbolPacker` cannot leak stale bytes.**
  - Its buffer is a `Vec` that only grows by appending (`frame_packet` →
    `extend_from_slice`) and is `clear()`ed at `emit` (`framing.rs:205-211`).
  - Each symbol is a fresh `vec![0u8; size]` that the buffer is copied into.
  - The design keeps this. `put_packet` appends by `resize(at + 2 + len, 0)`
    and then writes in place, so it never writes into a reused fixed array.
  - §8.3 adds a dirty-buffer test.
- **No other source-symbol producer exists.**
  - The six `emit_source` calls take either a `SymbolPacker` output
    (`net/mod.rs:2990`, `:3020`, `:3097`, `:3118`) or a `frame_window_packet`
    output (`:3133`, `:3183`).
  - `emit_source` feeds `encoder.add_source_owned` (`emit_source.rs:283`). That
    pads or truncates to S (`rlc_window.rs:73-84`).
  - Retransmits re-send the encoder's own stored bytes.
  - Every other `add_source(` call is in `#[cfg(test)]` code (grep:
    `fec/unified.rs`, `fec/window_traits.rs`).
- **The remaining real cause is a symbol-size disagreement between peers** (§5).
  Option (ii) removes it.

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
  history above it. The entry covers:
  - the one LE framing;
  - `WindowStart` without `packed`;
  - `Handshake.symbol_size` now meaning the sender's real size (§5);
  - the fact that v9 Realtime symbols are BE-packed.
- **Coordination:**
  - None of the 360 local and `github` refs scanned has `PROTOCOL_VERSION` ≥ 10
    in code. `perleg-design` (`9e03d84a`) docs claim no wire version.
  - F2 takes v10, and the receiver-timestamp field becomes v11.
  - The texts that still call the timestamp "v10" are
    `docs/status/42-the-engine-path-for-dispersion-and-the.md:21` ("wire v9 → v10")
    and `:76` ("wire v10 and the ADR"), and
    `docs/status/43-the-oracle-store-law-in-closed-loop.md:213` ("socket
    timestamps, wire v10, an ADR").
  - The W3.3 roadmap nodes (`docs/roadmap/tracks/store.md:296`, `:364`;
    `nodes.yaml`) name no version. The plan text for W3.3 is not in this repo,
    so it was not checked.
  - The coordinator updates the status pages. This branch does not write
    `docs/status.md`.
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

## 5. Symbol size per direction (in v10)

### 5.1 How each side gets its size today

- **Each process picks its own hint, so the two sides can differ in a shipped setup.**
  - The CLI takes `--protocol-hint` and `--profile` per process (`main.rs:66-71`).
    A config file and profile overlay can also set them (`config.rs:185`). The
    Home profile gives Auto and Datacenter gives Bulk (`config.rs:139-149`).
  - The default is Auto (`config.rs:235-239`).
  - The size then follows from the hint (`symbol_size_for_hint`,
    `net/mod.rs:181-186`): Realtime is 512, Bulk and Auto are 1200.
  - Nothing forbids `--protocol-hint realtime` on one side and the default on
    the other.
- **No tested setup differs.** Every harness passes one hint to both sides:
  - `tools/l1/perf_rwm_c.sh` passes one `$HINT` to the server (`:204`) and the client (`:365`);
  - `tail_matrix.sh:67`/`:72` and `tun_bulk.sh:73`/`:79` do the same;
  - the in-process loopback does the same (`tests/common/loopback.rs:630-642`);
  - `lib.sh` sets no hint.
- **There is no Android client in this repo.**
- **The handshake carries neither the hint nor the real size.**
  `local_handshake` sends a constant `symbol_size: 1200` (`io_owner.rs:793-795`),
  and the peer's handshake is discarded (`_peer_hs`, `:953`, `:962`).
- **The receiver decodes with its own local size** (`recv_symbol_size = symbol_size`,
  `net/mod.rs:1171` → `create_window_decoder`, `receiver.rs:117`). That is the
  only receive-side use of the size. `report_symbol_size` (`tasks/report.rs:85`)
  times *sent* symbols and stays local.
- **Effect today, Bulk sender into Realtime receiver:** the 1200-byte sources
  are cut to 512 (`unified.rs:403`). Packets of up to 510 B arrive. Larger ones
  overrun, are dropped and are still acked.
- **Under fail-closed without a fix:** the first such packet closes the tunnel.
  So fail-closed is safe only together with one of the options below.

### 5.2 Options

**(i) Refuse a mismatch at connect.**
- `Handshake.symbol_size` becomes the real local size, and both sides compare
  it with their own.
- On a mismatch, connect fails with "symbol_size mismatch: local 512, peer 1200
  (check --protocol-hint on both ends)". The server bails before replying, as for
  the version.
- Cost: a few lines.
- It makes the two sides' hint choices a *joint* constraint.
- Under NO-MODE-SWITCH it has the wrong shape. Once the size is a function of
  δ, as the hint table's debt implies it will be, any two peers at different δ
  are refused. That is a step at "equal", where the dial should be continuous.

**(ii) The receiver decodes with the sender's size, per direction (recommended).**
- `Handshake.symbol_size` is the sender's real size, the local
  `symbol_size_for_hint` value. Each side's decoder is built with the **peer's**
  announced size.
- Each direction's size then belongs to its sender alone, like packing (§4):
  - no joint constraint;
  - a future continuous S(δ) needs nothing more;
  - no receive-side mode bit.
- The handshake is reliable (a stream), so this is not a new one-shot dependency.
- **Mechanics:**
  - `local_handshake` takes the local size (pass it in `OwnerArgs` from
    `QuicTransport::new`; the size is known at `net/mod.rs:623`, before the
    transport is built at `:727`).
  - `io_owner::connect`/`accept` return the peer `Handshake` up through
    `QuicTransport::connect`/`accept`.
  - `run_impl` collects the peer sizes over the initial paths. They are all
    handshaken before the receiver is built (`:739-748` against `:1171`).
  - Then `recv_symbol_size = reconcile_peer_symbol_size(&sizes)?`.
- **Refused (connect fails with a clear error):**
  - paths of one peer that announce different sizes;
  - a runtime-added path (`quic.rs:622`) whose size differs from the established
    one, which refuses that path;
  - a size outside **[72, 1200]**. The floor is the IPv4 minimum MTU of 68
    (RFC 791) plus the 4-byte framing allowance that sets the TUN MTU
    (`net/mod.rs:657`). The ceiling is the largest size this build's table
    emits, which keeps the compact frame inside ADR-0055's 1350-byte floor; a
    future table that goes larger moves the bound with it.

  These refusals are integrity checks on one peer, not a mode.
- **Wire cost:** none. `Handshake.symbol_size: u16` already exists. Only its
  meaning changes, which is covered by v10.
- **Risk, unverified:** a Realtime-side OS TUN (MTU 508) receives packets of up
  to 1196 B from a Bulk peer. A Linux TUN write is not limited by the device MTU
  on ingress, as far as I know; I have not checked it. Inner TCP clamps its MSS
  to the smaller side anyway. The memory TUN of the tests has no such limit.

**Recommendation: (ii).** It removes §1.1's last real false-positive cause, so
fail-closed then fires only on an engine bug. It is the only option that stays
continuous in δ. (i) is the fallback if the owner wants mixed-hint pairs refused
rather than supported.

### 5.3 `backend` has the same shape and is single-valued

- Each side sets `fec_backend` from its own config.
- Only `rlc` is accepted (`config.rs:272-285`; `pipeline_backend`,
  `net/mod.rs:385-394`).
- It is announced only in `WindowStart` (log only) and in every `WireSymbol`'s
  `backend`. The handshake does not carry it.
- Every decoder drops a non-Rlc symbol (`unified.rs:381`, `rlc_window.rs:457`,
  `generation.rs:1086`).

So a backend mismatch is unreachable today. If a second backend returns, the
per-symbol tag already carries it, and that decision goes into that ADR, not
F2's. No change.

**Out of scope, same shape:** ρ (`window_reliable`) and the generation wire are
also per-side configuration that nothing on the wire checks (audit §6). Under
`RWM_UNIFIED`, the default, one decoder reads both wires.

## 6. Defect (b): a symbol the receiver cannot unframe

**The brief's premise ("no ack, so ARQ repairs it") does not hold in this code:**
- A seq enters `received_seqs`, and with it the SACK and the cumulative-ack
  input, at decoder output (`receiver.rs:1466-1478`). That is before any extract
  site, so by the time a symbol is unframed it is already SACK-visible.
- All three decoders dedup a source by seq (`unified.rs:384-400`,
  `rlc_window.rs:463-472`, `generation.rs:1091-1100`). A retransmit of an
  absorbed seq never re-emerges.
- After (A), §1.1 and §5 (ii), the only remaining cause is an encode or decode
  bug. A retransmit carries the same bytes, so ARQ cannot repair it.

Not acking would mean validating before `:1477`, so before the decoder. Even
then, the hole stays permanent at ρ = 1 and becomes a silent loss at ρ < 1.

**Decision (recommended, part of the one owner question in §10): fail closed,
one behaviour at every ρ and δ.** At each of the four extract sites, a
`read_packets` error:
1. adds one to `UNDELIVERABLE_SYMBOLS` (already printed as `undeliv=` on `[CTLD]`);
2. logs one `error!` that **begins with the token `[UNFRAMABLE]`** and gives the
   seq, the symbol length and the first 4 bytes. It is always on, not gated by
   `RWM_DIAG`, so the harness witness of §9 can key on it;
3. `break 'recv`. That is the receiver's existing fatal exit (`receiver.rs:1060`,
   `:1529`, `:1626`): the exit flush, then the receiver task ends, and the
   tunnel shuts down (`net/mod.rs:1341`).

The symbol is never delivered and never counted as delivered:
- the in-order site reads before `blk.lat.note_delivery` (moved above it,
  `:1610`);
- `next_expected` is not advanced past it;
- no further ack is sent.

Acks already sent for it are moot, because the connection ends.

The rejected alternative is to keep the tunnel up, skip the seq and leave it out
of `received_seqs`. That gives a permanent stall at ρ = 1 and a silent loss at
ρ < 1, which is two outcomes and no repair.

## 7. Costs, from code

- **Bytes per symbol, Bulk and Auto: +0.** `symbol_size` is 1200
  (`net/mod.rs:181-186`) and the TUN MTU is `symbol_size - 4` = 1196
  (`net/mod.rs:657`). A one-packet symbol is `[len LE][≤1196 B][zeros]`, which
  is today's `frame_window_packet` output byte for byte. Repair symbols are
  combinations of identical sources, so they are identical too. Two witnesses:
  - **`hotpath_pin` at v9.** It digests `frame_window_packet` output at T = 1200
    over lengths 0…1500, truncation included, through the encoder and both
    framings. The bincode framing carries `PROTOCOL_VERSION` (`hotpath_pin.rs:78`,
    `protocol.rs:573`), so the pin can witness the framing only **before** the
    bump. The framing commit therefore lands at v9 with `hotpath_pin` unchanged.
    The v10 commit follows and re-captures the two digests for the version
    field alone (`pin_frame_layout_and_add_source_identity` and the compact
    arena test carry no version and stay green).
  - **A permanent byte-equality test:** `frame_window_packet` against an inlined
    copy of v9's `frame_window_packet_inner`, over `hotpath_pin::LENS` at
    T = 1200 and 512.
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
  - the 4-byte version field reads 10 (same width);
  - `Handshake.symbol_size` keeps its width; at Realtime its value changes from
    1200 to 512.
- **The guard: F2 rides the stacked §41c guard.**
  - Owner decision 5 (2026-11-16, status "Binary identity") and CLAUDE.md limit
    the waiver to an identical release sha. F2 changes the binary, so there is
    no waiver question.
  - F2's Bulk cells are zero-behaviour: `hotpath_pin` is unchanged at the framing
    commit, `decision_trace_pin` is unchanged, and no law is touched. So F2
    stacks into the one guard run against main with the other zero-behaviour
    steps, and is bisected only on failure.
  - The guard is 12 + 12 rows at c1s, c1d, c2 and c8
    (`docs/derivations/shared-core-design.md:302`).
  - **VM cost:**
    - Inside a stacked run, F2 adds no rows: one more candidate commit, and the
      run's builds are shared.
    - Run alone, it would be about 48 min of rows plus overheads, two release
      builds of about 6 min each, and the suites at about 50 min, so roughly
      2 h. These are the §41d figures in status, "Expected length".
    - A guard failure costs one bisection run of the same size.
- **No guard covers Realtime.** That is where behaviour does change (it is the
  fix). The field check is §9's first VM contact, then §51's E block.
- **CPU per symbol, expected:**
  - Sender: ±2 ns. Same allocation, a `put_packet` call instead of an inline
    copy, inlined in release.
  - Receiver, Bulk: +1–3 ns. One more u16 load and compare, to read the zero
    terminator. The outer `Vec` allocation is the same as today's
    `Option::into_iter().collect()`.
  - At c1s (≈ 90 k symbols/s, status §41c readout), +5 ns ≈ 0.45 ms of CPU per
    second, about 0.05 % of a core. The guard's c1s CPU resolution is ≈ 2 %.
  - The handshake change runs once per path.
- **Host ns bench to specify** (the `nsbench41c` pattern): an ignored release
  test `framing_ns`.
  - BASE inlines main's `frame_window_packet_inner` and `extract_window_packet`.
    CAND calls `frame_window_packet` and `read_packets`.
  - Points: S = 1200 at lengths 64, 600 and 1196; S = 512 with 3 × 160 packed
    (BASE = main's BE `extract_packets`, CAND = `read_packets`).
  - 2 000 000 calls per point, 5 alternating BASE/CAND pairs, `taskset -c 2`.
  - Report median ns/call with [min–max].

## 8. Test plan

Red first (each fails on `f2-prep`, except where noted):
1. **Endianness, by byte literal:** `read_packets(&[3,0, 0xAA,0xBB,0xCC, 1,0, 0xDD, 0,0, …zeros to 16])`
   gives `[[AA,BB,CC],[DD]]`, and `put_packet` writes exactly `[3,0,AA,BB,CC]`.
   On main the BE reader reads 768 and yields nothing.
2. **Round trips, absolute, at S = 1200 and S = 512:**
   - a one-packet symbol: `frame_window_packet` → `read_packets` = `[pkt]` for
     lengths 1, 508, 1196 and `S-2`;
   - a packed symbol: the `SymbolPacker` output of 3 × 160 → 3 packets in order;
   - `SymbolPacker` with one 100-byte packet then `flush` is byte-equal to
     `frame_window_packet` of it. The two writers agree;
   - the v9 byte-equality test (§7). It is green from the start and stays green.
3. **`Unframable` and the packer's dirty buffer:**
   - a length overrun after a valid first packet gives `Err`, and the wrapper
     counts it once;
   - a non-zero symbol with no packet gives `Err`;
   - all zeros gives `Ok([])` and is not counted;
   - **dirty buffer:** push 3 × 160, emit, then push one 10-byte packet and
     `flush`. The second symbol is `[10,0][10 B]` followed by zeros only, and
     `read_packets` gives exactly one packet. Repeat after an oversize
     (truncated) push;
   - **decoder output is full size:** for each of `UnifiedDecoder`,
     `RlcWindowDecoder` and `GenerationDecoder`, a source fed with a short
     payload, and a seq recovered from repair, both come out as exactly
     `symbol_size` bytes and read back with `read_packets` to the packet that
     was sent. Use a payload whose last bytes are zero.
   - These replace `net/tests.rs:6563-6586`.
4. **The repro, un-ignored and tightened:** `tests/f2_window_start_lost.rs` also
   asserts `raptorpath::net::undeliverable_symbols() == 0` after the run. App
   delivery is already asserted: the client returns Ok only after the object is
   acked and fully received.
5. **Fail closed executes, with a witness** (a new own-binary test,
   `tests/f2_unframable_closes.rs`, at Bulk and at Realtime).
   - **The seam.** `TEST_CORRUPT_SOURCE: AtomicU32` sits beside
     `TEST_DROP_WINDOW_START` (`io_owner.rs:61`). On the next non-repair compact
     Data datagram once the count has passed a warm-up of N = 20 source
     datagrams, it:
     - saves the original symbol payload in
       `TEST_CORRUPTED_PAYLOAD: Mutex<Option<Vec<u8>>>`;
     - rewrites the payload's first u16 to 0xFFFF.
   - **The drive.** Drive each side's engine directly with `net::run_with_tun`
     and memory TUNs, not `perf`.
     - The client TUN injects M = 200 distinct packets, each with its index in
       its first 4 bytes.
     - The test collects every packet the server's memory TUN delivers until
       the server engine returns.
   - **Assertions:**
     - `undeliverable_symbols() == 1` exactly;
     - the seam was spent;
     - an `[UNFRAMABLE]` line was logged (`tracing-test` or a capture layer;
       otherwise, assert the counter and the task end only);
     - the server engine's future completes within 5 s of the corruption. The
       outcome is `Ok(())`, the engine's existing convention for a fatal
       receiver exit: `break 'recv` returns the task normally, `net/mod.rs:1341`
       logs at info and `r?` passes;
     - **not delivered:** every packet in `read_packets(saved_original)` is
       absent from the server's delivered set;
     - **it executed:** the delivered set is non-empty (the warm-up got
       through), so the close is not a clean shutdown before any data.
6. **Version and handshake:**
   - `protocol_version_is_nine` → `_is_ten`;
   - `handshake_refuses_an_old_peer_at_v9` → `_at_v10`, with olds `[9, 8]`, plus
     data-path refusal;
   - a `WindowStart { symbol_size, backend }` round trip;
   - `local_handshake` carries the local size: 512 at Realtime, 1200 at Bulk;
   - `reconcile_peer_symbol_size`:
     - `[1200, 1200]` gives 1200;
     - `[1200, 512]` gives `Err`;
     - `[71]` and `[1201]` give `Err`;
     - `[72]` and `[1200]` are accepted, the absolute edges of the range.
7. **Mixed hints, the (ii) witness:** an in-process loopback with the **client
   at Bulk and the server at Realtime**. Add a `cfgs` variant that takes one hint
   per side.
   - The client sends full-size chunks (≥ 1000 B).
   - Assert a full round trip and `undeliverable_symbols() == 0`.
   - Red on `f2-prep`: the server's 512-byte decoder truncates the 1200-byte
     sources, the chunks are lost, and the run times out.
   - Add the reverse pair (client Realtime, server Bulk) as a second case.

Pins:
- `hotpath_pin`: **unchanged at the framing commit**, at v9. It is the Bulk
  byte-identity witness there, and a re-capture at that commit is a design
  failure. It is re-captured at the v10 commit for the version field only, and
  the commit message says so.
- `decision_trace_pin`: unchanged (no framing input).
- wasm goldens: untouched. `raptorpath-wasm`, `-core`, `-math` and the
  visualizer have no framing or packing code (grep).

Existing tests to migrate (not new behaviour):
- `net/framing/tests.rs`: BE literals become LE (`:85`); the window and packer
  tests read through `read_packets`.
- `tests/fec_window_test.rs`, `tests/fec_backend_switching_test.rs`: they import
  `extract_window_packet`; use `read_packets(..).ok()` and take the first packet.
- `tests/f2_undeliverable_counter.rs`: header cross-reference only.

## 9. Implementation brief (Sonnet worker)

**Branch:** off `f2-design` in its own worktree. Commit there, and do not push
unless told.

**Decided:** everything in §1–§8, including §5 option (ii) and the fail-closed
policy of §6. If the owner picks (i) instead, only step 5's handshake part
changes: compare instead of adopt.

**Do not:**
- touch `use_packing`'s keying, `symbol_size_for_hint`, the TUN clamp, or
  `SymbolPacker`'s capacity, truncation or buffer discipline;
- touch the wasm, core or math crates;
- re-capture `decision_trace_pin`, or `hotpath_pin` anywhere but the v10 commit.

**Commit rule.**
- Two engine commits, in order. Commit 1 is steps 1–4 and 6–8, at v9.
  `hotpath_pin` must pass unchanged at its end. Commit 2 is step 5: the version
  bump, `WindowStart` without `packed`, the handshake size (ii), the version and
  handshake tests, and a `hotpath_pin` re-capture whose message says "version
  field only".
- **Commit 1 is not wire-valid alone.** It labels itself v9 but writes LE-packed
  Realtime symbols that a real v9 peer misreads. Never pin, launch, tag or merge
  commit 1 alone, and never squash the two. Merge the branch with `--no-ff`.
- The harness witness (step 9) is a third, tools-only commit.

**Steps, red first:**
1. **Tests that fail on the base.**
   - Write §8 tests 1–3 in `net/framing/tests.rs`, `net/tests.rs` and the decoder
     test modules, against the new names; they fail to compile, or fail.
   - Un-ignore and tighten the repro (§8.4) and run it: red.
   - Add the seam and test §8.5: red (today the tunnel stays up).
   - Add the v9 byte-equality test (§7); it stays green throughout.
   - In commit 2 only, before the bump: edit the version pins and write §8.6–7,
     all red.
2. **`net/framing.rs`.**
   - Add `put_packet` and `read_packets` / `Unframable`. Remove `frame_end`,
     `extract_packets` and `extract_window_packet`.
   - `frame_window_packet_inner` keeps `vec![0u8; size]` and the `S-2` clamp, and
     calls `put_packet`.
   - `SymbolPacker::push` appends by `resize`, then `put_packet`. `emit` still
     copies into a fresh zeroed symbol and `clear()`s.
   - Update the module and `SymbolPacker` docs (LE, zero padding as terminator).
3. **`net/mod.rs`.** `extract_window_packets(data) -> Result<Vec<Vec<u8>>, Unframable>`:
   one `read_packets` call, and `fetch_add` on `Err`. Fix the comment at `:650`
   and the `UNDELIVERABLE_SYMBOLS` doc (`:1384-1390`, now "the receiver closes the
   tunnel").
4. **`net/receiver.rs`.**
   - Delete `window_packed`.
   - At `:1016`, `:1524`, `:1621` and `:1756`: `Ok(p)` goes on as today. `Err`
     logs one `error!("[UNFRAMABLE] …")` (§6) and does `break 'recv`.
   - At the in-order site, call the reader before `blk.lat.note_delivery`.
   - At the hold-expiry sites, break before `next_expected` moves.
   - Delete the `WindowStart` arm at `:2152-2155`, and update the module doc's
     list of fatal exits (`:21-24`, `:2189`).
5. **Commit 2: `transport/protocol.rs`, `io_owner.rs`, `quic.rs`, `net/mod.rs`.**
   - Drop `packed`, set v10 with its history entry, and edit the pins. In
     `net/control_msg.rs:122`, drop `packed` from the pattern. In
     `net/mod.rs:1884`, drop it from the send.
   - The handshake size (§5.2 ii):
     - the local size goes into `OwnerArgs`, and `local_handshake` sends it;
     - `connect`/`accept` return the peer `Handshake`;
     - `QuicTransport::connect`/`accept` return the peer's size;
     - `reconcile_peer_symbol_size` is a pure function next to
       `symbol_size_for_hint`;
     - `recv_symbol_size` is the reconciled peer size;
     - `add_path` refuses a mismatch;
     - `info!` the local and peer sizes once per path.
6. **`transport/io_owner.rs`.** Add `TEST_CORRUPT_SOURCE` and
   `TEST_CORRUPTED_PAYLOAD` (one relaxed load when unarmed, the
   `test_drop_window_start` pattern).
7. **Migrate the integration tests** listed in §8.
8. **Add the `framing_ns` bench** (§7), ignored. Run it once on the host,
   release, and report the numbers; do not tune.
9. **Harness row witness (tools only, third commit).** A fail-closed tunnel must
   void its row, never read as a DNF datum.
   - `tools/l1/l1common.py` gets `f2_void(lines) -> str | None`. It returns a
     reason when:
     - any `[CTLD]` line has `undeliv=` > 0 (`field(line, "undeliv")`), or
     - any line contains `[UNFRAMABLE]`.

     The error line is always on, so rows run without `RWM_DIAG` are covered.
   - `tools/l1/perf_rwm_c.sh` adds one line after the client exits, beside the
     `--- RWM-C perf` (`:286`) and DNF (`:385`) lines:
     `--- F2 undeliv=<max over /tmp/rwm-s.log and /tmp/rwm-c.log> unframable=<count of [UNFRAMABLE] lines>`.
     `f2_void` also reads that line.
   - Every scorer that labels DNF calls `f2_void` before its DNF test. The row
     is then labelled `VOID-F2`, excluded from DNF counts, from scored medians
     and from goodput medians, and counted and printed in the session summary.
   - Required at once:
     - the guard scorer `place41b_score.py`;
     - the E-block and §51 scorers;
     - the parsers `place_parse.py` and `r_parse.py`;
     - the scorer of any session queued next (`s61_score.py` on `harness61` is
       already launched; its owner decides).

     The other `dnf` users (grep) take it at their next touch.
   - Tests in `test_l1common.py` (and a stub-log case for `perf_rwm_c.sh` if a
     stub harness exists):
     - `undeliv=0` with no token gives `None`;
     - `undeliv=3` gives a reason;
     - a bare `[UNFRAMABLE]` line without `[CTLD]` gives a reason;
     - a DNF row that also has the token is labelled `VOID-F2`, not DNF.

**Done:**
- `cargo test -p raptorpath` is green at commit 2, with the lib and every
  integration test, including the un-ignored repro, `f2_undeliverable_counter`,
  the fail-closed test and the mixed-hint test;
- at commit 1, everything except §8.6–7 is green;
- `decision_trace_pin` passes unchanged at both commits, and `hotpath_pin`
  passes unchanged at commit 1;
- `cargo clippy -p raptorpath` adds no new warnings;
- `grep -rn "window_packed\|extract_window_packet\|to_be_bytes" raptorpath/src/net/framing.rs raptorpath/src/net/receiver.rs`
  finds nothing framing-related;
- the tools tests pass (`python3 -m pytest tools/l1/test_l1common.py`, plus the
  suites `build_visualizer.sh`/CI already runs, if any).

Host cargo runs under the mutex
(`L=/c/Users/pierr/AppData/Local/Temp/claude/cargo-host.lock; until mkdir "$L" 2>/dev/null; do sleep 20; done; …; rmdir "$L"`),
with `CARGO_BUILD_JOBS=1` and your own `CARGO_TARGET_DIR`.

**Report:**
- the three shas;
- the red→green list;
- the `framing_ns` table;
- any site where the `Err` arm could not sit before a delivery record;
- the scorers that got `f2_void`.

**VM order (for the coordinator; not the worker's job):**
1. **Suites first.** The VM suites run on the candidate, fail-closed ON as
   built, before any pin or row is registered. A suite that trips
   `[UNFRAMABLE]` is a false positive to fix before anything else.
2. **First VM contact: E's failing paced Realtime rows**, the 2 of 3 E-rehearsal
   rows that failed (seed 42, paced), before the full E-pre.
   - Run them on the `f2-prep` binary (the control: the failure should
     reproduce, now with `undeliv` > 0) and on the candidate (delivery,
     `undeliv` = 0, no `[UNFRAMABLE]`).
   - The trigger (GE loss at connect) was inferred, not observed. If the control
     does not reproduce, the trigger reading is wrong, and the owner hears that
     before E-pre.
3. **Then** the stacked §41c guard (§7) and E-pre.

## 10. Owner question (one)

**Approve fail-closed (§6) together with symbol size per direction, option (ii) (§5.2)?**
- (ii) makes each direction's symbol size its sender's choice, announced in the
  reliable v10 handshake. Mixed-hint pairs then work, and fail-closed fires only
  on an engine bug.
- Alternatives:
  - fail-closed with (i): mixed-hint pairs are refused at connect with a clear
    error;
  - keep the tunnel up with a permanent hole (not recommended: a stall at ρ = 1
    and a silent loss at ρ < 1).
- No tested setup mixes hints today (§5.1). A shipped one can.
