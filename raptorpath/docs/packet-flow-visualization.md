# Raptorpath Packet Flow Visualization

Visual guide to how packets traverse the raptorpath system — from TUN ingress
through FEC encoding, multipath scheduling, and receiver-side recovery.

---

## 1. Packet-to-Symbol Pipeline

### Window Mode (Realtime)

One TUN packet becomes one source symbol, padded to `symbol_size`:

```
  TUN packet (47–1400 bytes)
  ┌─────────────────────────┐
  │ IP header + payload     │
  └────────────┬────────────┘
               │ frame_window_packet()
               ▼
  ┌──┬─────────────────┬────────────┐
  │LE│  original data   │  zero pad  │  ← symbol_size bytes total (e.g. 1200)
  │u16                  │            │
  └──┴─────────────────┴────────────┘
   2B     47–1198 B       remainder

               │ wrap in WireSymbol
               ▼
  WireSymbol { block_id: seq, payload_id: 0, is_repair: false,
               data: [padded], backend: Rlc/Mettle/Streaming }

               │ feed to window encoder
               ▼
  ┌──────────────────────────────────┐
  │ Window FEC Encoder               │
  │  • maintains sliding window      │
  │  • generates repair on demand    │
  └──────────┬───────────────────────┘
             │
     ┌───────┴───────┐
     ▼               ▼
  source sym      repair sym(s)
  (original)      (coded from window)
```

### Block Mode (Bulk / Auto)

Multiple TUN packets are length-prefixed and packed into one block:

```
  TUN packets
  ┌──────┐ ┌──────────┐ ┌────┐
  │ pkt1 │ │   pkt2   │ │pkt3│
  └──┬───┘ └────┬─────┘ └──┬─┘
     │          │           │  length-prefix each (2B BE)
     ▼          ▼           ▼
  ┌──┬──────┬──┬──────────┬──┬────┬──────────────┐
  │L1│ pkt1 │L2│   pkt2   │L3│pkt3│  zero fill   │  ← block buffer
  └──┴──────┴──┴──────────┴──┴────┴──────────────┘
     │                          0x0000 sentinel ──┘
     │ slice into k source symbols
     ▼
  ┌────────┐ ┌────────┐ ┌────────┐
  │ src[0] │ │ src[1] │ │ src[2] │  ... src[k-1]
  └───┬────┘ └───┬────┘ └───┬────┘
      │          │           │  FEC encode (RaptorQ / RS / RLC)
      ▼          ▼           ▼
  ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐
  │ src[0] │ │ src[1] │ │ src[2] │ │ rep[0] │ ... rep[r-1]
  └────────┘ └────────┘ └────────┘ └────────┘
      k source symbols        r repair symbols
```

---

## 2. Wire Format Byte Maps

All multi-byte integers are **little-endian** unless noted.

### Source Symbol (Window Mode)

```
  symbol_size bytes (e.g. 1200)
  ┌──────┬───────────────────────────┬───────────────┐
  │ len  │        packet data        │   zero pad    │
  │ u16  │                           │               │
  │ LE   │                           │               │
  └──────┴───────────────────────────┴───────────────┘
  ◄─2B──►◄──── len bytes ──────────►◄── remainder ──►
         max payload = symbol_size − 2
```

### RLC Repair Symbol (14B header)

```
  REPAIR_HEADER_SIZE = 14
  ┌────────────────┬────────────┬──────────────┬─────────────────────┐
  │  window_start  │ win_count  │ repair_index │     coded data      │
  │    u64 LE      │  u16 LE    │   u32 LE     │                     │
  └────────────────┴────────────┴──────────────┴─────────────────────┘
  ◄──── 8B ───────►◄─── 2B ───►◄──── 4B ─────►◄── symbol_size B ───►

  Coefficients derived from PRNG seeded by (window_start, repair_index).
  Coded data = Σ coeff[i] × source[window_start + i]  over GF(2^8),
               for i in 0..win_count.
```

### Streaming Repair Symbol (15B header)

```
  STREAMING_REPAIR_HEADER = 15
  ┌────────────────┬────────────┬──────────────┬───────┬─────────────┐
  │  window_start  │ win_count  │ repair_index │ layer │ coded data  │
  │    u64 LE      │  u16 LE    │   u32 LE     │  u8   │             │
  └────────────────┴────────────┴──────────────┴───────┴─────────────┘
  ◄──── 8B ───────►◄─── 2B ───►◄──── 4B ─────►◄─1B──►◄─sym_size B─►

  Layer 0: diagonal XOR (burst protection)
  Layer 1: random GF(256) (erasure recovery)
```

### METTLE Repair Symbol (variable header)

```
  REPAIR_HEADER_FIXED = 10
  ┌────────────────┬──────────────┬──────────────────────────┬───────────┐
  │  window_start  │ num_members  │  member_offsets[]        │ xor_data  │
  │    u64 LE      │   u16 LE     │  u16 LE × num_members   │           │
  └────────────────┴──────────────┴──────────────────────────┴───────────┘
  ◄──── 8B ───────►◄──── 2B ────►◄── 2 × N bytes ─────────►◄─sym_size─►

  Header size = 10 + 2 × num_members (variable)
  Each offset is relative to window_start.
  xor_data = XOR of all member source symbols (no GF multiply).
```

### SymbolBatch Envelope (on the wire)

```
  ┌──────────┬─────────────┬──────────────────────────────────────────┐
  │  "RPTQ"  │  version    │          bincode payload                 │
  │  4 ASCII │  u32 BE     │                                          │
  │          │  (= 3)      │                                          │
  └──────────┴─────────────┴──────────────────────────────────────────┘
  ◄── 4B ───►◄──── 4B ────►◄──────── variable ──────────────────────►

  bincode payload = WireMessage::Data(SymbolBatch {
      symbols:          Vec<WireSymbol>,   // source + repair symbols
      send_timestamp_us: u64,              // μs since connection epoch
      batch_seq:         u64,              // per-path monotonic counter
      path_id:           u32,              // which network path
  })

  Max message size: 2 MB     Max symbols/batch: 1000
```

---

## 3. Multipath Scheduling

### Example: 10 source + 4 repair across WiFi and LTE

```
  Path Properties
  ┌─────────────────────────┐    ┌─────────────────────────┐
  │ WiFi (path 0)           │    │ LTE  (path 1)           │
  │  RTT:      5 ms    ◄──best   │  RTT:     25 ms         │
  │  Loss:     2.0%         │    │  Loss:     0.5%         │
  │  cwnd:     80 pkts      │    │  cwnd:     40 pkts      │
  │  goodput:  15.2 Mbps ───┼──► │  goodput:   9.7 Mbps   │
  │  jitter:   1.2 ms       │    │  jitter:    4.0 ms      │
  └─────────────────────────┘    └─────────────────────────┘
```

**Scheduling rules:**
- Sources → **lowest RTT** path first (minimize time-to-first-byte)
- Repairs → **proportional to effective goodput**
- Realtime mode adds redundant source copy on secondary path

```
  Symbols to schedule:  S0 S1 S2 S3 S4 S5 S6 S7 S8 S9  R0 R1 R2 R3
                        ├── 10 source ─────────────────┤  ├─4 repair─┤

  ┌─ WiFi (lowest RTT, higher goodput) ───────────────────────────────┐
  │                                                                    │
  │  Sources (all 10, WiFi has capacity):                              │
  │  ┌────┬────┬────┬────┬────┬────┬────┬────┬────┬────┐              │
  │  │ S0 │ S1 │ S2 │ S3 │ S4 │ S5 │ S6 │ S7 │ S8 │ S9 │             │
  │  └────┴────┴────┴────┴────┴────┴────┴────┴────┴────┘              │
  │                                                                    │
  │  Repairs (61% of 4 ≈ 2, proportional to goodput share):           │
  │  ┌────┬────┐                                                       │
  │  │ R0 │ R1 │                                                       │
  │  └────┴────┘                                                       │
  └────────────────────────────────────────────────────────────────────┘

  ┌─ LTE (higher RTT, lower goodput) ─────────────────────────────────┐
  │                                                                    │
  │  Repairs (39% of 4 ≈ 2):                                          │
  │  ┌────┬────┐                                                       │
  │  │ R2 │ R3 │                                                       │
  │  └────┴────┘                                                       │
  │                                                                    │
  │  [Realtime mode only] Redundant source copies:                     │
  │  ┌────┬────┬────┬────┬────┬────┬────┬────┬────┬────┐              │
  │  │ S0'│ S1'│ S2'│ S3'│ S4'│ S5'│ S6'│ S7'│ S8'│ S9'│             │
  │  └────┴────┴────┴────┴────┴────┴────┴────┴────┴────┘              │
  └────────────────────────────────────────────────────────────────────┘

  Goodput fractions:
    WiFi: 15.2 / (15.2 + 9.7) = 61%  →  ceil(0.61 × 4) = 3 → clamped to 2
    LTE:   9.7 / (15.2 + 9.7) = 39%  →  floor(0.39 × 4) = 1 → adjusted to 2
```

---

## 4. Block Interleaving (depth = 2)

Interleaving weaves symbols from multiple blocks so that a burst loss
hits different blocks instead of destroying one.

### Before Interleaving

```
  Block A (4 src + 2 rep):  A0  A1  A2  A3  Ar0  Ar1
  Block B (4 src + 2 rep):  B0  B1  B2  B3  Br0  Br1

  Transmission order (no interleave):
  ┌────┬────┬────┬────┬─────┬─────┬────┬────┬────┬────┬─────┬─────┐
  │ A0 │ A1 │ A2 │ A3 │ Ar0 │ Ar1 │ B0 │ B1 │ B2 │ B3 │ Br0 │ Br1 │
  └────┴────┴────┴────┴─────┴─────┴────┴────┴────┴────┴─────┴─────┘

  Burst loss of 4 consecutive symbols:
           ╳    ╳    ╳     ╳
  │ A0 │ A1 │ A2 │ A3 │ Ar0 │ Ar1 │ ...
         ^^^^^^^^^^^^^^^^^^^^^^^^
         All 4 losses in Block A → likely unrecoverable
         (lost 4 of 6 symbols, need 4 sources)
```

### After Interleaving (depth = 2, round-robin)

```
  Interleaved order (alternating A, B):
  ┌────┬────┬────┬────┬────┬────┬─────┬─────┬─────┬─────┬────┬────┐
  │ A0 │ B0 │ A1 │ B1 │ A2 │ B2 │ A3  │ B3  │ Ar0 │ Br0 │Ar1 │Br1 │
  └────┴────┴────┴────┴────┴────┴─────┴─────┴─────┴─────┴────┴────┘

  Same burst loss of 4 consecutive symbols:
           ╳    ╳    ╳     ╳
  │ A0 │ B0 │ A1 │ B1 │ A2 │ B2 │ ...
         ^^^^^^^^^^^^^^^^^^^^^^^^
         2 losses in Block A (B0, A1) + 2 losses in Block B (B1, A2)
         wait — round-robin means:
           lost: B0, A1, B1, A2
           Block A lost 2 of 6 → recoverable (has 2 repairs)
           Block B lost 2 of 6 → recoverable (has 2 repairs)
```

### Drain Triggers

```
  Whichever fires first:
  ┌────────────────────────────────────────────────────┐
  │ 1. slots.len() ≥ depth         (depth reached)    │
  │ 2. oldest slot > 2 × flush_timeout  (time limit)  │
  │ 3. total buffered ≥ 1024       (memory pressure)  │
  └────────────────────────────────────────────────────┘

  Latency overhead = (depth − 1) × flush_timeout
  ┌──────────┬───────┬──────────────────┐
  │ Protocol │ Depth │ Max added delay   │
  ├──────────┼───────┼──────────────────┤
  │ Realtime │   2   │  1 × 2ms  =  2ms │
  │ Auto     │   3   │  2 × 10ms = 20ms │
  │ Bulk     │   4   │  3 × 10ms = 30ms │
  └──────────┴───────┴──────────────────┘
```

---

## 5. Tapered Repair Interleaving

Previous block's repairs are spread across the next block's sources using
an exponential decay schedule: more repairs up front, fewer at the tail.

### Decay Formula

```
  weight(i) = exp(−λ × i / k)

  where:
    i = source position (0 to k−1)
    k = source_count
    λ = 4.605 / (1 + 10 × loss_rate)        [4.605 ≈ −ln(0.01)]
```

### Example: 6 repairs distributed across 8 sources

```
  ── Loss = 0% (λ ≈ 4.6, steep decay) ──────────────────────────────

  Position:    0      1      2      3      4      5      6      7
  Weight:    1.000  0.562  0.316  0.178  0.100  0.056  0.032  0.018
  Repairs:     3      1      1      1      0      0      0      0
               ▓▓▓    ▓      ▓      ▓
  Drain:     R R R  S  R  S  R  S  R  S  S  S  S  S
              ↑ front-loaded: most repairs emitted early

  ── Loss = 12% (λ ≈ 2.1, gentler decay) ───────────────────────────

  Position:    0      1      2      3      4      5      6      7
  Weight:    1.000  0.769  0.591  0.455  0.349  0.269  0.207  0.159
  Repairs:     2      1      1      1      1      0      0      0
               ▓▓     ▓      ▓      ▓      ▓
  Drain:     R R  S  R  S  R  S  R  S  R  S  S  S  S
              ↑ more spread out: repairs cover a wider span
```

### Block-Mode Integration

```
  Time ──────────────────────────────────────────────────────►

  Block B₀ arrives:
  ┌─────────────────────────────────────┐
  │ B₀: S₀ S₁ S₂ S₃ S₄  R₀ R₁ R₂ R₃  │  ← B₀ repairs held as pending
  └───────────────────────┬─────────────┘
                          │ hold repairs
                          ▼
  Block B₁ arrives:
  ┌─────────────────────────────────────────────────────────────────┐
  │ taper_schedule = compute_taper_schedule(4 repairs, 5 sources,   │
  │                                         loss_rate)              │
  │                                                                 │
  │ Emit:  R₀ R₁  S₀  R₂  S₁  R₃  S₂  S₃  S₄                    │
  │        ├─B₀ repairs interleaved into B₁ sources──┤              │
  │                                                                 │
  │ B₁'s own repairs → held as pending for B₂                      │
  └─────────────────────────────────────────────────────────────────┘
```

### Window-Mode Burst Repairs

```
  In window mode, burst repairs cover new sources immediately:

  burst_count = ceil(loss_rate × BURST_FACTOR)     [BURST_FACTOR = 4.0]

  Loss = 0%:   burst = 0   →  no extra repairs
  Loss = 5%:   burst = 1   →  1 extra repair per source
  Loss = 12%:  burst = 1   →  1 extra repair per source
  Loss = 30%:  burst = 2   →  2 extra repairs per source

  Source arrives:  S₇
                     ↓ immediately generate burst repairs
                   S₇  R₇ₐ  [R₇ᵦ]
                        └── covers S₇ in current encoder window
```

---

## 6. Path Properties — WiFi vs LTE Side-by-Side

```
  ┌──────────────────────────────┬──────────────────────────────────┐
  │         WiFi Path            │           LTE Path               │
  ├──────────────────────────────┼──────────────────────────────────┤
  │                              │                                  │
  │  RTT:     5 ms (typ)        │  RTT:     25 ms (typ)            │
  │  Loss:    1–3%              │  Loss:    0.3–1%                 │
  │  Jitter:  0.5–2 ms         │  Jitter:  2–8 ms                 │
  │  BW:      20–80 Mbps       │  BW:      10–50 Mbps             │
  │  cwnd:    60–120 pkts      │  cwnd:    30–60 pkts             │
  │                              │                                  │
  │  ┌─ Burst Model ──────┐    │  ┌─ Burst Model ──────┐         │
  │  │ Gilbert-Elliott     │    │  │ Gilbert-Elliott     │         │
  │  │ mean burst: 2–4 pkt │    │  │ mean burst: 1–2 pkt │         │
  │  │ burst prob: 5–15%   │    │  │ burst prob: 2–5%    │         │
  │  └─────────────────────┘    │  └─────────────────────┘         │
  │                              │                                  │
  │  Best for: source symbols   │  Best for: repair diversity      │
  │  (lowest RTT → first byte)  │  (independent loss events)       │
  │                              │  + redundant src (Realtime)      │
  └──────────────────────────────┴──────────────────────────────────┘
```

---

## 7. Receiver Pipeline

### Full Receive Path (Window Mode)

```
  Network (QUIC)
       │
       ▼
  ┌──────────────────────────────────────────────────────────┐
  │  Envelope Parsing                                        │
  │  ┌──────┬─────────┬──────────────────────────────┐       │
  │  │"RPTQ"│ ver = 3 │ bincode → WireMessage::Data  │       │
  │  └──────┴─────────┴──────────────────────────────┘       │
  │       │                                                   │
  │       ▼  SymbolBatch { symbols, timestamp, seq, path }   │
  └───────┬──────────────────────────────────────────────────┘
          │
          │ for each WireSymbol in batch
          ▼
  ┌──────────────────────────────────────────────────────────┐
  │  Window Decoder (stateful, long-lived)                   │
  │                                                          │
  │  ┌─ Source symbol ──────────────────────────────────┐    │
  │  │ Record as known; substitute into pivot rows      │    │
  │  │ Cascade: check if any pivot reduces to degree 1  │    │
  │  └──────────────────────────────────────────────────┘    │
  │                                                          │
  │  ┌─ Repair symbol ─────────────────────────────────────┐ │
  │  │ Parse header → extract window range + coefficients   │ │
  │  │                                                      │ │
  │  │ ┌─ Gaussian Elimination (RLC) ───────────────────┐  │ │
  │  │ │ 1. Substitute known sources (XOR + GF multiply)│  │ │
  │  │ │ 2. Eliminate against existing pivot rows       │  │ │
  │  │ │ 3. If single unknown → RECOVER source          │  │ │
  │  │ │ 4. If multiple unknowns → store as new pivot   │  │ │
  │  │ │ 5. Cascade: propagate recovery through pivots  │  │ │
  │  │ └───────────────────────────────────────────────-┘  │ │
  │  │                                                      │ │
  │  │ ┌─ Peeling (METTLE) ────────────────────────────┐   │ │
  │  │ │ 1. XOR out all known members from bin data    │   │ │
  │  │ │ 2. If degree 1 → PEEL (recover immediately)  │   │ │
  │  │ │ 3. If degree > 1 → store in pending_bins     │   │ │
  │  │ │ 4. Propagate: check all bins referencing      │   │ │
  │  │ │    recovered seq, peel if degree drops to 1   │   │ │
  │  │ └──────────────────────────────────────────────-┘   │ │
  │  └─────────────────────────────────────────────────────┘ │
  │                                                          │
  │  Output: Vec<(seq, decoded_data)>                        │
  └──────────┬───────────────────────────────────────────────┘
             │
             ▼
  ┌──────────────────────────────────────────────────────────┐
  │  Reorder Buffer                                          │
  │                                                          │
  │  BTreeMap<u64, (Bytes, Instant)>                         │
  │  key = sequence number                                   │
  │                                                          │
  │  ┌─────┬─────┬─────┬─────┬─────┬─────┬─────┐            │
  │  │  42 │  43 │  45 │  47 │  48 │  49 │  51 │  ...       │
  │  └──┬──┴──┬──┴─────┴─────┴──┬──┴──┬──┴─────┘            │
  │     │     │                  │     │                      │
  │     ▼     ▼                  ▼     ▼                      │
  │  next_deliver_seq = 42                                    │
  │  drain_contiguous() → [42, 43]  (gap at 44 → stop)      │
  │                                                          │
  │  Later: seq 44 arrives or timeout expires                │
  │  drain_contiguous() → [44, 45] → then gap at 46 → stop  │
  │  drain_expired()    → force-deliver entries > 20ms old   │
  │                                                          │
  │  Config: timeout = 20ms, max_buffered = 500              │
  └──────────┬───────────────────────────────────────────────┘
             │ contiguous (seq, data) pairs
             ▼
  ┌──────────────────────────────────────────────────────────┐
  │  Packet Extraction                                       │
  │                                                          │
  │  extract_window_packet(symbol_data):                     │
  │    read u16 LE length prefix → slice [2..2+len]          │
  │    validate len ≤ symbol_size − 2                        │
  │    return original TUN packet                            │
  └──────────┬───────────────────────────────────────────────┘
             │
             ▼
  ┌──────────────────────────────────────────────────────────┐
  │  TUN Inject                                              │
  │  recv_tun_tx.try_send(Bytes)                             │
  │  → non-blocking; drops packet if channel full            │
  └──────────────────────────────────────────────────────────┘
```

### Receive Path (Block Mode, simplified)

```
  SymbolBatch
       │
       │ for each WireSymbol
       ▼
  ┌─────────────────────────────────────────────────────────┐
  │  Per-Block Decoder (one decoder per block_id)           │
  │  active_decoders: DashMap<u64, Box<dyn FecDecoder>>     │
  │                                                         │
  │  decoder.add_symbol(symbol) → Option<Bytes>             │
  │  When k symbols received (any mix of src + rep):        │
  │    → Some(decoded_block)                                │
  └──────────┬──────────────────────────────────────────────┘
             │
             ▼
  ┌─────────────────────────────────────────────────────────┐
  │  extract_packets(decoded_block):                        │
  │    loop: read 2B BE length, slice data, repeat          │
  │    stop at 0x0000 sentinel                              │
  │    → Vec<original_packets>                              │
  └──────────┬──────────────────────────────────────────────┘
             │
             ▼
  TUN Inject (same as window mode)
```

---

## Summary: End-to-End Flow

```
  ┌─────┐    ┌────────┐    ┌─────────┐    ┌────────────┐    ┌──────┐
  │ TUN │───►│Framing │───►│FEC Enc  │───►│ Scheduler  │───►│ WiFi │──┐
  │ in  │    │        │    │         │    │            │    └──────┘  │
  └─────┘    │ window:│    │ RLC     │    │ src→minRTT │    ┌──────┐  │
             │  pad   │    │ METTLE  │    │ rep→maxGP  │───►│ LTE  │──┤
             │ block: │    │ Stream  │    │            │    └──────┘  │
             │  pack  │    │ RaptorQ │    └────────────┘              │
             └────────┘    └─────────┘          │                     │
                                          ┌─────┴──────┐             │
                                          │Interleaver │             │
                                          │ block/taper│             │
                                          └────────────┘             │
                                                                     │
  ┌─────┐    ┌────────┐    ┌──────────┐    ┌──────────┐             │
  │ TUN │◄───│Extract │◄───│Reorder   │◄───│ Window   │◄────────────┘
  │ out │    │ packet │    │ Buffer   │    │ Decoder  │   (receiver)
  └─────┘    └────────┘    │BTreeMap  │    │ GE/Peel  │
                           │ by seq   │    │ cascade  │
                           └──────────┘    └──────────┘
```
