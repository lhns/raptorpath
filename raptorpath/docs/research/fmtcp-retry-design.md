# FMTCP-Class Retry — Pure Fountain-Coded Multipath Aggregation: Design + Oracle Confirmation

**Status:** DESIGN + ORACLE-CONFIRMED. No production code; no VM. The temporal
oracle (`raptorpath-math/tests/temporal_oracle.rs`, PART 5 / 5b) confirms the
design reaches **×1.19 at C8** and escapes the in-order-frontier recovery-latency
serialization that capped every prior production attempt at ~parity (×0.97).

**Recommendation up front: BUILD.** The pure combination the arc never tested —
*total-in-flight flow control* + *fountain-redundancy loss absorption (no per-hole
ARQ)* + *decode-on-total, out of order* — reaches the Σg goodput ceiling in the
honest temporal model, with a bounded in-flight store and zero ARQ. The minimal
production change list is in §5. This is the FMTCP escape the literature-map
identified, now modeled honestly and confirmed.

---

## 1. The FMTCP-class mechanisms (the key finding), cited

The fountain-coded multipath designs all share one architecture: **the coded
block, not the byte stream, is the unit of reliability, and reliability is
decoupled from ordering.** For each design, the two load-bearing answers:

### 1.1 FMTCP — Cui, Wang, Wang, Wang & Wang, IEEE/ACM ToN 23(2):465–478, 2015

The principal missed solution. FMTCP's abstract states our exact C8 pathology: "a
subflow experiencing high delay and loss … becomes the bottleneck … significantly
degrading the aggregate goodput."

- **FLOW CONTROL — total in-flight / per-block decoding demand, NOT an in-order
  cumulative-ack frontier.** FMTCP fountain-encodes each data block and transmits
  encoded symbols whose *only* job is to supply decoding degrees of freedom for a
  block. The source sends symbols "according to the decoding need of the
  destination" — i.e. it keeps a block in flight until the receiver has enough
  independent symbols to decode it, not until a specific in-order byte is ACKed.
  There is no cumulative-ack byte frontier that a hole can freeze: an encoded
  symbol "is a combination over a block," so it carries no fixed sequence
  position. Multiple blocks are in flight concurrently and complete **out of
  order**.
- **LOSS ABSORPTION — fountain redundancy, NO per-hole retransmission.** "Lost
  packets do not need to be retransmitted." When a path's quality drops, "the
  source only needs to transmit new encoded symbols" — a *per-block deficit*
  top-up (send more any-of-N symbols until the block decodes), never a per-hole
  round-trip ARQ. The redundancy ε is provisioned to the block's expected loss;
  the rateless top-up is deficit-driven (how many more DoF the block still
  needs), not hole-driven.
- **SCHEDULING — data allocation by expected arrival time + decoding demand.**
  FMTCP's "data allocation algorithm based on the expected packet arriving time
  and decoding demand" stripes symbols across heterogeneous subflows by predicting
  when each subflow's symbol will *arrive and be useful*, and allocates each
  subflow the number of symbols that keeps every block's decode deadline met. A
  slow subflow is simply given fewer / more-useful symbols; because symbols are
  fungible it never owns a position the fast subflow must wait for.
- **DECODE-ON-TOTAL.** Each block decodes on any K independent symbols
  (fountain/rateless), from any mix of subflows, the instant the K-th arrives —
  a K-of-N order statistic, not a fork-join max over per-path shares.
- **SLOW-PATH FIX.** Fungibility + expected-arrival allocation: the slow path
  does real non-redundant work proportional to its goodput, and any shortfall is
  covered by *any other* path's next encoded symbol. The slow path can never be a
  long pole because no block is waiting on a *specific* slow-path symbol.

*Source:* Cui et al., FMTCP, IEEE/ACM ToN 23(2), 2015 (mechanism confirmed from
the paper abstract/summary and the SUNY-Stony-Brook author copy of the ToN
version).

### 1.2 SCDP — Alasmar, Parisis et al., IEEE/ACM ToN 2021 (arXiv 1909.08928)

Systematic rateless coding for data-centre transport; the cleanest modern
statement of the same architecture.

- **FLOW CONTROL — receiver-driven, per-object.** "Receiver-driven flow control in
  combination with in-network packet trimming" — the receiver pulls encoded
  symbols until the object decodes; there is no sender-side in-order ack gate.
  Multipath by **packet spraying** across all paths for every transport mode.
- **LOSS ABSORPTION — RaptorQ rateless, NO retransmission.** SCDP "eliminates
  Incast by … not relying on retransmissions of lost packets (given the rateless
  nature of RaptorQ codes)." A trimmed/lost symbol is replaced by *any* next
  encoded symbol.
- **DECODE.** Systematic RaptorQ → short flows complete decode-free
  (source symbols pass straight through); losses are filled from repair symbols;
  **decode-on-arrival**, out of order.

### 1.3 "MPTCP meets FEC" — Ferlin, Kučera, Claussen & Alay, IEEE/ACM ToN 26(5), 2018

The bandwidth-knob realization (keeps in-order TCP semantics, spends redundancy).

- **FLOW CONTROL — still MPTCP in-order** (it augments MPTCP rather than replacing
  its stream). This is why it is the *r*-knob, not the *H*-knob escape.
- **LOSS ABSORPTION — proactive coded redundancy for zero-RTT recovery.** It adds
  FEC so a loss is recovered *without* the "at least one extra RTT" that fast-
  retransmit / RTO cost — "TCP recovery mechanisms further escalate head-of-line
  blocking in multipath." The redundancy is proactive (provisioned ahead of loss),
  bought with bandwidth — exactly our (H, r) surface's r knob (paper §16.7). The
  companion systematic-coding line reports average MPTCP buffer delay cut by ≥80%.

### 1.4 Common architecture (the one-line synthesis)

All three **decouple reliability from ordering**: reliability = *this block has K
independent DoF* (fungible, any-path, no per-hole round trip); ordering = a
*separate, optional* delivery policy (reassemble-by-offset for objects). Flow
control is gated on **decode progress / total in-flight**, never on an in-order
cumulative-ack byte frontier. This is precisely the two-knob escape our own paper
§16.7 derived independently (H → decode-on-total; r → rateless fungibility), and
the position the literature-map names as the unbuilt fix.

---

## 2. The specific gap vs our attempts

**Hypothesis (task Part 2), CONFIRMED.** Every production attempt in the arc kept
**one foot in the in-order world**. Nobody flipped *both* the flow-control model
and the loss-recovery model at once. Cross-checked against the goal-gate record:

| Arc attempt | Flow control | Loss recovery | Result | Which foot stayed in-order |
|---|---|---|---|---|
| RWM Phase B/C, generation coding (§16.7) | **IN-ORDER frontier** (store pruned on cumulative ack) | fungible-ish, but same-path/suppressed at bulk r | ×0.76–0.85 | **flow control** — frontier serialization |
| Coded moving-window (temporal L1) | in-order, store ≈ W (stop-and-wait) | per-seq ARQ, ADR-0046-throttled | ×0.26 | both (moving anchor + throttle) |
| SACK + BDP decoupling (`feat/sack-flow-control`) | tried to decouple, but **SUMMED-across-paths store cap (#64)** | **PER-HOLE ARQ** (hole walks at ≈1 ARQ round/RTT) | single FLAT, C8 REGRESSES (bufferbloat) | **loss recovery** + the #64 summed-BDP bug |
| Aligned generations, stable anchor (oracle) | in-order metric | fungible cross-path | ×1.19 **in oracle**, unbuilt in production | — (this is the target) |

The two capping levers, stated exactly:

1. **IN-ORDER FLOW CONTROL** — the sender's outstanding/store is measured against
   the *in-order delivered frontier* (`window_ack_seq` / `df`). A hole freezes the
   frontier; the store fills; the fast path idles behind a hole that walks at ≈1
   ARQ round/RTT. (Generation/coded designs kept this.)
2. **PER-HOLE ARQ RECOVERY** — a lost symbol is a *specific position* recovered by
   a targeted round-trip retransmit, congestion-throttled, on its own path. (SACK
   designs kept this; plus the #64 summed-across-paths store cap, which defeats
   even a decoupled sender on heterogeneous RTT.)

**The untested pure combination:** *total-in-flight flow control* (outstanding
measured against total decode progress, not the in-order frontier) **+**
*fountain-redundancy loss absorption* (per-block deficit top-up, no per-hole ARQ)
**+** *decode-on-total, out of order*, **all at once**, with a **per-path (not
summed) BDP** in-flight cap. That is the FMTCP config, and it is the empty
quadrant.

---

## 3. The pure architecture for raptorpath (spec, not code)

**Unit.** Block-fountain per object. Partition the object into blocks/generations
of K source symbols; each block is fountain-coded (dense RLC over a small field,
or RaptorQ) with a **stable per-generation anchor** (the coding target never
moves — the temporal-oracle lesson that a moving anchor anti-aggregates). Source
symbols may be systematic (pass straight through, zero decode) for cheapness.

**Redundancy.** Send K·(1+ε) coded symbols per block, ε = expected aggregate loss
over the block's paths + a tail margin from r* (§8.4/§8.8). A block still short of
K after its proactive symbols triggers a **bounded rateless top-up sized to the
per-block DEFICIT** (K − received-DoF), NOT per-hole, NOT re-flooded every
sub-RTT (the #59/#60 lesson — deficit feedback rides one RTT, coalesced).

**Scheduling.** Stripe coded symbols across paths by **expected arrival**
(§16.3 marginal-cost / by-goodput placement): each path gets work ∝ its goodput
so the slow path does real non-redundant work, and the residual deficit is placed
on the best (soonest-useful) path. Fungible: any path's symbol covers any block's
deficit.

**Flow control (the crux).** Total in-flight ≤ **aggregate BDP = Σ_i BtlBw_i ·
RTprop_i** (per-path summed capacity, **NOT** the summed-*anchor* #64 formula that
double-counts the slow path's RTT-inflated window). Outstanding is measured
against **total decode progress**, not the in-order delivered frontier. Blocks
pipeline; **no in-order cumulative-ack gate on the sender.**

**Delivery.** Decode each block on any K independent symbols, **out of order**;
the object completes when all blocks decode (decode-on-total). Reassemble by
offset. No in-order delivery frontier anywhere in the reliability path (ordering,
if a stream ever needs it, is a separate optional receiver policy — §16.7).

**No systematic-only source with per-hole ARQ; no in-order frontier.** Those are
the two feet the arc kept in the in-order world; this design removes both.

---

## 4. Oracle confirmation (the gate)

Modeled in `raptorpath-math/tests/temporal_oracle.rs`. The existing `Sys` model
already carries the two levers as flags: `in_order` selects the flow-control gate
(`df` = in-order frontier vs `d` = total delivered = total-in-flight);
`cross_path`+`arq` select fungible fountain redundancy vs per-hole ARQ. Two new
tests (PART 5, PART 5b) plus an `idle_slots` / `max_outstanding` sender-stall
probe isolate the flow-control lever honestly.

### 4.1 The 2×2 lever matrix at C8 het (`fmtcp_pure_flow_control_and_redundancy`)

K ≈ 25 MB, finite store = fungible horizon, seed 0xF00D. Goodput ceiling Σg/g_fast
= **×1.195**.

```
  config (FC × LR)                                factor   arq   idle  maxOut
  in-order-frontier FC  ×  per-hole ARQ            0.932x    9   4394    494   <- PRODUCTION cap
  TOTAL-in-flight FC    ×  per-hole ARQ            1.171x   44      0    169   <- flip FC only
  in-order-frontier FC  ×  fountain redundancy     1.188x    0      0    296   <- flip LR only
  TOTAL-in-flight FC    ×  fountain redundancy     1.188x    0      0    156   <- PURE FMTCP
```

### 4.2 The flow-control lever isolated (`fmtcp_flow_control_lever_isolated`)

Recovery held at the CAPPING setting (per-hole ARQ) + finite store; flip ONLY the
flow-control model:

```
  flow control              factor   idle_slots   max_inflight
  in-order-frontier (df)     0.932x        4394            494   (pinned at store cap)
  total-in-flight (d)        1.171x           0            169   (≈ aggregate BDP)
```

### 4.3 What the oracle says (the answers to Part 4)

- **(a) Reaches ×1.19 at C8?** YES. The pure FMTCP config (total-in-flight FC +
  fountain redundancy) reaches **×1.188**, essentially the Σg ceiling (×1.195),
  with **zero ARQ** and in-flight bounded at **156** symbols ≈ aggregate BDP
  (145) — no #64 bufferbloat (156 ≪ K = 16 667).

- **(b) Does honest flow-control modeling escape the frontier serialization?**
  YES, and the mechanism is now explicit. The **×0.97 production cap is the
  CONJUNCTION** of in-order-frontier flow control AND per-hole ARQ (0.932× in the
  model, 4394 wasted sender slots, in-flight pinned at the store cap — the sender
  idles behind a hole walking at ≈1 ARQ round/RTT). **Flipping the flow-control
  model alone** — in-order → total-in-flight, holding recovery FIXED at per-hole
  ARQ — lifts it **0.932 → 1.171** and collapses the sender idle drag to **zero**.
  Flipping the recovery model alone (fountain) lifts it **0.932 → 1.188**. The
  pure config flips both and reaches the ceiling.

- **The critical honest nuance (report this):** in this model the two escapes are
  *each individually sufficient* — total-in-flight FC escapes even with per-hole
  ARQ, and fungible fountain repair escapes even with in-order FC. The production
  ×0.97 persisted because **every prior attempt kept BOTH capping levers set at
  once** (generation designs: in-order FC + effectively-throttled same-path
  recovery; SACK designs: per-hole ARQ + summed-BDP store). The oracle does NOT
  contradict the literature: total-in-flight + fountain-redundancy does **not**
  cap at parity — it reaches ×1.19. There is no critical refutation; the build is
  greenlit by the model.

- **Regression control.** C7 symmetric aligned generations ×1.96 (ideal ~2.0), no
  drag; the fidelity test still reproduces the L1 ×0.26/×0.36 refutation of the
  naive moving window. `cargo test -p raptorpath-math` green (13 temporal_oracle
  tests incl. the 2 new).

---

## 5. BUILD-or-REFUTE recommendation + minimal production change list

**BUILD.** The oracle confirms the pure FMTCP config reaches ×1.19 at C8 and
escapes the frontier serialization with a bounded store and no ARQ. This is a
scoped, mechanism-level greenlight — subject to coordinator review and VM
availability for the L1 measurement.

**Minimal production change list** (the delta from today's stack):

1. **Flow control: gate on total in-flight, not the in-order frontier.** Measure
   sender outstanding against *total decode progress* (count of decoded/covered
   symbols) instead of `window_ack_seq`. Remove the cumulative-ack byte frontier
   from the sender's store-prune / TUN-backpressure path for the bulk/object
   profile.

2. **In-flight cap: per-path aggregate BDP, fix #64.** Cap outstanding at
   **Σ_i BtlBw_i · RTprop_i** (per-path capacity summed), NOT the current
   summed-*anchor* `gain · Σ BtlBw×RTprop` that double-counts the slow path's
   RTT-inflated window. This is the single line that made SACK+BDP regress C8.

3. **Loss recovery: fungible per-block deficit top-up, delete per-hole ARQ for
   the bulk profile.** Replace targeted per-seq retransmit with a rateless top-up
   sized to the per-block DEFICIT (K − received DoF), placed on the best path,
   coalesced to one deficit feedback per RTT (no per-sub-RTT re-flood — #59/#60).
   Provision proactive ε ≥ expected block loss so most blocks never need the
   top-up (oracle: r ≈ 0.05–0.06 sufficient at C8).

4. **Delivery: decode-on-total, out of order, reassemble by offset.** Keep the
   stable per-generation anchor (never a moving window). Object completes when all
   blocks decode; no in-order delivery frontier in the reliability path. (This
   capability — RWM Phase C's unordered flag — largely exists; the change is
   making flow control and recovery consume it, per items 1–3.)

**What NOT to change:** the codec (decoder-revival fix already took repairs_useful
0.15%→66–72%); loss-blind Copa CC; the r* controller. The realtime/in-order
profiles keep today's semantics — this is the bulk/object profile only.

**Risk / open items to validate at L1:**
- Shared-bottleneck path *correlation* is not modeled (independent GE chains); the
  §16.7 caveat. Real-trace validation needed before claiming the number.
- The oracle's ε (r≈0.05–0.06) is provisioned against GE; real cellular loss
  under-provisions r* by 2–4× (Finding 5 / task #46) — size the proactive ε
  against the empirical window-loss quantile, not the GE tail.

---

## 6. References (verified titles/venues; per the desk-research rule)

- Y. Cui, L. Wang, X. Wang, H. Wang, Y. Wang, "FMTCP: A Fountain Code-Based
  Multipath Transmission Control Protocol," IEEE/ACM Trans. Networking 23(2),
  pp. 465–478, 2015. — the principal design.
- S. Ferlin, S. Kučera, H. Claussen, Ö. Alay, "MPTCP meets FEC: Supporting
  Latency-Sensitive Applications over Heterogeneous Networks," IEEE/ACM Trans.
  Networking 26(5), 2018 (arXiv:1807.11059). — the r-knob (bandwidth) realization.
- M. Alasmar, G. Parisis, et al., "SCDP: Systematic Rateless Coding for Efficient
  Data Transport in Data Centres," IEEE/ACM Trans. Networking, 2021
  (arXiv:1909.08928). — receiver-driven rateless, no-retransmission, packet spray.
- N. Kuhn, E. Lochin, F. Michel, M. Welzl, "Forward Erasure Correction (FEC)
  Coding and Congestion Control in Transport," IRTF RFC 9265, 2022. — the
  presence⊥throughput bound the escape must respect (spare bandwidth = the ε).
- Internal: goal-gate.md "FINAL CONSOLIDATED VERDICT (2026-07-08)"; paper
  §16.7 (H, r surface), §16.8 (final status); `temporal_oracle.rs` PART 5/5b;
  literature-map.md Finding 3 + "Known solutions to the … bound".
