# Raptorpath Packet Flow Visualization

Visual guide to how packets traverse the raptorpath system on the **shipped
default stack** — the unified (δ, ρ, r) span machine (`RWM_UNIFIED`, default
ON since 2026-07-21) over the systematic RLC wire, with the consolidated
multipath laws (per-path recovery clocks, path-scaled pool, SACK-clocked
store release).

> **Honesty note.** This document is DESCRIPTIVE: hand-drawn diagrams of the
> mechanisms as shipped, refreshed 2026-07-28 against main `7a3aff6`. It is
> not a measurement record (that is `goal-gate.md`) and not normative (that
> is the paper, `fec-arq-model.md` §16.20/§16.26, and the code). The
> interactive companion is `raptorpath-visualizer/interactive-visualizer.html`
> — an L0 model of the same laws, with its own model-vs-engine table.

---

## 1. One Pipeline, No Mode Switch (§16.20)

There is ONE receive machine and ONE emission law, parameterized by the
hint's declared latency price δ(hint) = 0.5/ζ (§12.4: Bulk 0.005 / Auto 0.5
/ Realtime 50). Realtime and Bulk are the two ends of a continuum, not two
machines. (The legacy three-machine stack is the `RWM_UNIFIED=0` opt-out;
the streaming two-layer code was deleted 2026-07-27 after its crown re-test.)

```
  TUN packet (47–1400 bytes)
  ┌─────────────────────────┐
  │ IP header + payload     │
  └────────────┬────────────┘
               │ frame_window_packet(): u16 LE length prefix + zero pad
               ▼
  source symbol (symbol_size bytes, e.g. 1200)      ── the SYSTEMATIC wire:
  WireSymbol { block_id: seq, is_repair: false }       source symbols travel
               │                                       in the clear; only
               ▼                                       repairs are coded
  ┌──────────────────────────────────────────────────────────────┐
  │ Sender span machine (§16.20.3) — per source symbol:          │
  │   owed += r        (the TaperBudget quantity law: the wire   │
  │                     consumes r* exactly as computed, §8.4)   │
  │   if owed ≥ 1: emit ONE repair over the TRAILING span        │
  │                                                              │
  │        [F ────────── F+A* ]──Δ──▶ send frontier              │
  │         ▲                    ▲                               │
  │         │                    └ the span ENDS Δ behind the    │
  │         │                      frontier: every member has    │
  │         │                      LANDED when the repair does   │
  │         └ F = oldest unresolved position (cum-ack + 1)       │
  └──────────────────────────────────────────────────────────────┘
```

### The span law — every parameter derived, no mode bit

```
  D  = min(b·RTprop, 2·RTprop)     recovery deadline;  b(hint) = ½ / 1 / 2
                                   (Realtime / Auto / Bulk, §16.26)
  A* = clamp(rate·D, 1, W)         span width (coding quantum)
  M* = ceil(rate·2·RTprop/A*_q)+1  quanta in flight, clamped [2, 32]
                                   (A*_q = A* quantized to the retained grid)
  Δ  = clamp(⌈rate·J⌉, 1, 64)      trailing offset (J = jitter anchor)
```

`rate` is the windowed-MAX delivered rate and `RTprop` the min-filtered RTT —
measured anchors (§16.21 anchor hygiene), never live-SRTT. The two limits:

- **Realtime (δ = 50):** small fresh spans trailing the frontier, solvable
  at arrival, per-arrival incremental decode; depth term inert.
- **Bulk (δ = 0.005):** A* = 2·BDP clamped by W → the retained-grid quantum;
  M* = the §16.17 generation-pipeline depth; ρ = 1 RETAIN.

Between them δ moves everything smoothly — the oracle's continuity sweep
checks that no cliff appears at any δ.

---

## 2. Wire Format Byte Maps

All multi-byte integers **little-endian** unless noted.

### Source symbol (systematic — travels uncoded)

```
  ┌──────┬───────────────────────────┬───────────────┐
  │ len  │        packet data        │   zero pad    │
  │ u16  │                           │               │
  └──────┴───────────────────────────┴───────────────┘
  ◄─2B──►◄──── len bytes ──────────►◄── remainder ──►
```

### RLC repair symbol (14-byte self-describing header)

```
  REPAIR_HEADER_SIZE = 14
  ┌────────────────┬────────────┬──────────────┬─────────────────────┐
  │  window_start  │ win_count  │ repair_index │     coded data      │
  │    u64 LE      │  u16 LE    │   u32 LE     │                     │
  └────────────────┴────────────┴──────────────┴─────────────────────┘

  Every repair is a self-describing linear equation over the global
  source-seq variable space:
     Σ coeff(a,w,i)[c] · x_{a+c} = payload     (coeffs PRNG-seeded by
                                                (window_start, repair_index))
  FILL_FLAG variant (top bit of repair_index set): +2B coded_width — the
  sender summed only the prefix [a, a+cw); trailing columns are zero.
```

### SymbolBatch envelope

```
  ┌──────────┬─────────────┬──────────────────────────────────────────┐
  │  "RPTQ"  │  version    │          bincode payload                 │
  │  4 ASCII │  u32 BE (=4)│  WireMessage::Data(SymbolBatch { … })    │
  └──────────┴─────────────┴──────────────────────────────────────────┘
  SymbolBatch { symbols, send_timestamp_us, batch_seq (per-path), path_id }
```

---

## 3. Receiver: the Unified Decoder (§16.20.2, `src/fec/unified.rs`)

One decoder for every span policy the sender may derive — sliding spans and
pinned generations are the SAME input language.

```
  WireSymbol
      │
      ├─ source ──▶ a UNIT equation: deliver immediately, convert to a
      │             known column, back-eliminate payload-only through
      │             every covering row (worklist for the cascade)
      │
      └─ repair ──▶ ┌───────────────────────────────────────────────┐
                    │ Global incremental RREF, sparse-aware:         │
                    │ 1. KNOWN columns never enter the matrix —      │
                    │    eliminated payload-only (S bytes each)      │
                    │ 2. k = 0 fast path: span fully known ⇒         │
                    │    redundant in O(w), zero GF work             │
                    │ 3. forward-reduce vs existing pivots;          │
                    │    rows stay dense over their interval SPAN    │
                    │    (union of intervals is an interval — no     │
                    │    per-coefficient maps, no cascade allocs)    │
                    │ 4. unit rows deliver immediately               │
                    │                                                │
                    │ Cost: O(k·L·S + k²·(L+S)); block-diagonalizes  │
                    │ to the §16.18 bound on aligned (gen) wires     │
                    └───────────────────────────────────────────────┘
      │
      ▼
  delivery: the maximal determined subset — a property of the EQUATIONS,
  not the decoder. (Fixes the legacy sliding decoder's rank-loss defect on
  late sources; differential-tested against all legacy machines.)
```

Measured at the realtime cell (§16.20.8): the matrix is ~EMPTY at every
sample — trailing spans arrive solvable (k = 0 or deliver-now), per-arrival
decode 6–11 µs. Span freshness is what keeps the global closure cheap.

### Delivery contract past the decoder

```
  ρ = 1 (RETAIN, Bulk default) ──▶ reliable in-order delivery; the shed
                                   law is compiled out by construction
  EVICT (Realtime)             ──▶ in-order hold = the δ dial b·SRTT;
                                   δ-honest shedding (§16.26):
                                     shed a hole  iff  projected delivery
                                     exceeds D(δ)  AND  cumulative shed ≤
                                     1−ρ = ε̂·(1−P_fec(r, ε̂, A*, σ²))
                                   past-budget candidates are SERVED —
                                   ρ wins over δ
```

---

## 4. Multipath (§16, the consolidated laws)

One shared reliable window poured across N paths; a loss on path i may be
healed by an arrival on any path — the in-order frontier advances at ≈ Σgᵢ,
beating the per-path-affine E[max] ceiling (§16.2).

```
  ┌─ path A (e.g. 100 Mbit / 10 ms / 2.6%) ─────────────────────────┐
  │  sources striped ∝ estimated goodput gᵢ = capᵢ·(1−ε̂ᵢ)          │
  │  repairs/retransmits prefer the best available path (§13.8)     │
  └─────────────────────────────────────────────────────────────────┘
  ┌─ path B (e.g. 20 Mbit / 40 ms / 4.8%) ──────────────────────────┐
  │  its own RFC 9002 recovery clock, its own loss estimator        │
  └─────────────────────────────────────────────────────────────────┘
```

The three walls this configuration exposed, and their shipped laws:

1. **Per-path recovery clocks** (wall #8, §16.24, `RWM_RECOV_MP` default ON).
   Loss detection is RFC 9002 generalized per path: 9/8 time threshold on
   the LIVE flight + kPacketThreshold = 3 same-path fast channel;
   retransmits inherit their own clock. A cross-path striping gap does NOT
   fire a hole while the sending path's clock still runs — before this law,
   82% of c7 retransmits were such phantoms.

   ```
   path A arrivals:  …  s41  s43  s45   ← receiver sees gaps s42, s44
   path B (slower):        s42  s44 ── still in flight, inside B's clock
                            │
                            └─ NOT holes: B's own 9/8·RTT_B clock gates
                               them; a global clock would retransmit both
   ```

2. **The path-scaled pool** (wall #7, §16.19, `RWM_STORE_PATHS` default ON).
   The outstanding pool was a per-TRANSFER constant (1024 symbols) — a
   Little's-law ~100–128 Mbit wall, CPU-invariant. The shipped pool scales
   per path (knee ≈ 2048/path); removal re-opens the c7 collapse class.
   (The c8-aware refinement was derived, built and REFUTED 2026-07-27 —
   §16.29: the c8 binder is slow-path conversion, not pool sizing.)

3. **SACK-clocked store release** (wall #9, §16.25, `RWM_STORE_SACK_RELEASE`
   default ON). Retention-store slots free on the SELECTIVE ack — a
   SACKed-but-not-cumulative symbol no longer holds a flow-control slot a
   full frontier round. Payload + ARQ maps are retained until the frontier
   (slot release ≠ recoverability). Measured: c7 occupancy 3157 → 1460,
   composed 1.018–1.045×Σ.

---

## 5. The Block Pipeline (§15 — unchanged by the unification)

Bulk without `--window-reliable` still uses the block path: packets are
length-prefixed into blocks, sliced into k source symbols, encoded by a
block FEC backend (RaptorQ-class), and decoded per block_id at the
receiver. This is §15's other knob; the RLC-family unification (§16.20)
does not touch it.

---

## 6. Diagnostic Gauges (current names, `RWM_DIAG` and friends)

The live instrumentation the ledger's batteries read, as printed today:

| gauge | side | what it reports |
|---|---|---|
| `[SND]` | sender | ack frontier, coded totals, window span, want-lists, tx_paused |
| `[SPAN]` | sender | the live span law: `a_star`, `delta`, `owed`, realized repair rate `rr`, debt, retx_buf |
| `[GPIPE]` | sender | M* depth transitions: `M* a→b (rate_max, rtprop)` |
| `[PFRAC]` | sender | proactive vs recovery coded split (`proactive_fraction`) |
| `[RCV]` | receiver | decode frontier, tracked generations, deficits, horizon withholding |
| `[SHED-R]` | receiver | δ-honest shedding receiver arm: holes, frontier, budget_open |
| `[FDIAG]` / `[FDIAG-T]` | receiver | frontier/hole probe: decode vs source service times, `rf`/`ru` (repairs fed/useful — the liveness gate) |
| `[REASM]` | receiver | reassembly span/pending under `RWM_REASM_BDP` |
| `[RDIAG]` | receiver | engine saturation: busy %, msgs/s, queue depth/capacity |
| `[WIDLE]` | receiver | wire idle gaps (inter-arrival truth for the lossy-residual accounting) |
| `[WEDGE]` | receiver | frontier-stall forensics: blocker seq, stall age |

---

## Summary: End-to-End Flow (shipped default)

```
  ┌─────┐   ┌─────────┐   ┌──────────────────┐   ┌─────────────┐   ┌────────┐
  │ TUN │──▶│ Framing │──▶│ Span machine     │──▶│ Scheduler   │──▶│ path A │─┐
  │ in  │   │ u16+pad │   │ owed += r*       │   │ src ∝ gᵢ    │   └────────┘ │
  └─────┘   └─────────┘   │ trailing [F,F+A*]│   │ corr → best │   ┌────────┐ │
                          │ M* quanta, Δ gap │   │ per-path 9002│─▶│ path B │─┤
                          └──────────────────┘   └─────────────┘   └────────┘ │
                                   ▲                    ▲                     │
                          δ(hint) = 0.5/ζ      path-scaled pool +             │
                          (ONE dial: CC price  SACK-clocked release           │
                           AND span shape)                                    │
                                                                              │
  ┌─────┐   ┌─────────┐   ┌──────────────────┐   ┌────────────────┐          │
  │ TUN │◀──│ Extract │◀──│ ρ=1: in-order    │◀──│ UnifiedDecoder │◀─────────┘
  │ out │   │ packet  │   │ EVICT: b·SRTT    │   │ global sparse  │  (receiver)
  └─────┘   └─────────┘   │ hold + δ-honest  │   │ RREF, k=0 fast │
                          │ shedding (§16.26)│   │ path (§16.20.2)│
                          └──────────────────┘   └────────────────┘
```
