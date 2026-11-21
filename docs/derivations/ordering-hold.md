# The ordering hold H_o(δ) for design U (derivation, no code)

**Question.** Design U (`skip-rule-efficiency.md` §4.1) splits today's receiver give-up into *release* (deliver past a hole: an ordering act, no loss) and *stop* (the sender's counted verdict). U needs a law for when the receiver releases: pass hole s for delivery at `t_detect(s) + H_o`. §4.1 asked for an H_o that is "never" at Bulk and Auto and short at Realtime. Stated that way it is a mode switch: a function that is infinite on a whole stretch of δ is a threshold renamed.

**Restated requirement.** At Bulk and Auto, release must not fire before a fill or a verdict *in the operating range*. That is shown by a gauge (§6), not by H_o = ∞. Where a finite hold does change Bulk or Auto, this document says so (§4.3, §5).

**Short answer.**
- There is no price of disorder in the paper. It is a new declared quantity ω, to be measured (§1).
- skip-rule §1 prices frontier seconds, which is the paper's own §7.3 frontier term. Under U that leaves disorder unpriced. The objective keeps the paper's pricing and adds ω per released-then-filled hole (§2).
- The exact optimal rule is not usable. It reads a fill hazard that is 0 before the first repair round trip, so it either releases at detect for every δ, or it jumps at one δ (§3.1).
- The rule proposed is rent-or-buy, on the paper's frontier pricing. "Release a hole when the δ-priced stall it has caused reaches the disorder price" (§3.2):

```text
   release s at the first t with      (δ/d) · ( t − t_detect(s) )   ≥   ω          ⇔        H_o(δ)  =  ω · d / δ
```

- H_o is continuous and strictly decreasing in δ, exactly ∝ 1/δ, finite for every δ > 0, and 0 at ω = 0 (the out-of-order end). It reads no ρ and no hint.
- Per-held-packet delay pricing would be a second new law next to ω, and the paper does not have it. It is carried only as a tabulated alternative (§3.4), and adopting it is a paper question, not an owner one.
- **Bulk and Auto keep order at N = 1 if ω ≳ 3.** In the example (d = 11 ms), ω = 1 gives an Auto hold of 22 ms, which releases the 2.5 % of holes whose first repair fails.
- **At N ≥ 2 with path skew Δ, every slow-path packet starts a hole.** Unless `ω ≥ δ·Δ/d`, it is released past and arrives out of order *without any loss*. At Realtime that is about the whole slow-path share. It also reaches Auto when the measured d is small (§5).

**Inputs read** (`github/main` `ca555715`):
- `skip-rule-efficiency.md` §1, §3.1, §3.4, §4 and §6;
- `rho-step-sites.md` C1, C2, C3, C9, C10, C11 and C12;
- paper `docs/fec-arq-model.md` §6.4, §7.3, §7.6 and §7.7;
- `net/late.rs:64–101` (`W_COST_RATIO`, `request_bar`) and `net/store_cap.rs:316` (`delta_price`);
- `net/reorder.rs:60–130`;
- `net/receiver.rs`: `:158–170` (the reorder-buffer arms, `recv_window_ooo`), `:265–271` (`abandoned_seqs`), `:654–672` (prune at `decode_inert_floor(next_expected)`), `:1012–1024` (the expiry moves `next_expected`) and `:1570–1590` (`late_after_aban`);
- `config.rs:78–92` (`window_out_of_order`);
- `fec/unified.rs:183` and `:380–400`, `fec/rlc_window.rs:456–475`, `fec/window_traits.rs:151–160` (decoder dedup and its return-once contract);
- `net/seq_bits.rs:1–32` (`MAX_SPAN` = 2²⁴);
- `net/report.rs:25–29` (`RACK_SPURIOUS_BUDGET`);
- RFC 5681 §3.1–§3.2 and RFC 8985 §1.1, §3.3.1 and §6.2 (texts checked).

**Words.**
- A hole's *fill time* R is measured from `t_detect(s)`, the first arrival above s. R = ∞ if the hole is never filled (a verdict whose last copy is also lost).
- *rbf* is the release-before-fill fraction, `P(H_o < R < ∞)`.
- *Displacement* is the number of successors delivered before a released-then-filled packet, and the time between its release and its fill.

## 1. The price of disorder (new)

**What release buys.** The frontier, and everything behind it, stops waiting on the hole.

**What release costs.** If the hole is filled after the release, the consumer gets that packet after its successors. The consumer is the TUN inject channel (`net/delivery.rs`, `receiver.rs:172`). It receives IP packets, so this is reordering seen by the inner flows, not byte-stream corruption. Each tunnel seq carries one or more IP packets (`extract_window_packets`, `receiver.rs:1014`), so one late seq displaces a bundle.
- **Inner TCP, classic.** Three duplicate ACKs trigger fast retransmit with `ssthresh = max(FlightSize/2, 2·SMSS)` (RFC 5681 §3.2, eq. 4). A displacement of three or more segments of that flow costs one spurious retransmission and one cwnd halving.
- **Inner TCP, RACK-TLP.** RFC 8985 §6.2 marks a segment lost once a later-sent one is delivered and `RTT + reo_wnd` has elapsed, with `reo_wnd = min(reo_wnd_mult·min_RTT/4, SRTT)`. reo_wnd is 0 while no reordering has been seen and the sender is in recovery or has ≥ DupThresh (3) segments SACKed. Each round with a DSACK raises reo_wnd_mult, and the raise persists for 16 recoveries. With DSACK-based undo, a spurious retransmission's cwnd cut is reverted. **So a modern inner stack tolerates reordering, and the measured ω may be small.** The tunnel cannot see the inner min_RTT, so it cannot use the time tolerance itself.
- **Inner UDP (realtime media, games).** Usually harmless: a jitter buffer reorders.

**What the paper has.** The paper prices delay (δ, `net::delta_price`, §4.1), a wasted copy (w = 1, `W_COST_RATIO`, §7.3 and §7.6), and frontier stall (Φ, §7.3). It has no price for disorder delivered to the consumer.
- `RACK_SPURIOUS_BUDGET = 1/16` (`net/report.rs:29`) is the bar for the tunnel's *own* spurious repairs. It is not a consumer price.
- §6.4's RFC 9002 and RACK factors set a stall duration, not a disorder cost.
- `window_out_of_order` is the ω = 0 end. See below.

```text
   ω  =  price of one released-then-filled hole, in repair symbols (the unit of δ and w)
```

The simplest form is one constant per hole, charged when the late fill arrives.

**Provenance: measured.** Run a TCP-in-tunnel battery with releases forced by a test seam. Read the inner goodput deficit and inner retransmissions (`iperf3` retr) per released-then-filled hole, against the matched no-release run. Report it per inner stack: Linux Cubic with RACK and DSACK undo by default, and RACK off as the classic case.
- **A reference scale only, not provenance:** with no undo, one Reno halving at window W costs `W²/8` packets of send deficit (RFC 5681 eq. 4, plus additive increase of 1 SMSS per RTT).
- Back-solving ω from today's Realtime SRTT/2 would be calibration, not provenance.

**`window_out_of_order` (`config.rs:92`, `receiver.rs:161`).** It is a declared consumer contract: "Out-of-order object delivery … the H → ∞ corner (paper §4.11)". The receiver then runs no reorder buffer and hands each decoded symbol over at once. The object API reassembles by offset, and the sender's retention backpressure is relaxed.
- **Its delivery half is exactly ω = 0.** At ω = 0 the rule releases every hole at detect, so each symbol is delivered on arrival. ω would subsume that half, with the in-order and out-of-order consumers as two values of one price.
- **It is not a dial-derived bit.** It is the consumer's declared contract, set by config, not by δ or ρ. So it is not the forbidden "bit from a dial". It does select a code path (`reorder_buf = None`), though, and once U ships that path is the ω = 0 point of the one law. Retiring the delivery half into ω = 0 is recommended, with its own ADR.
- **Its other half is not covered here.** The relaxed sender backpressure, and the `cumulative_received` arm at `receiver.rs:2022`, are sender and ack behaviour. ω does not replace them.

## 2. The objective under U: the paper's pricing plus ω

skip-rule §1 prices T_stall, the frontier-blocked seconds, at δ/d. That is the paper's own form: §7.3 prices the lost packet's delay at `δ·(T + d)/d` and the frontier separately (Φ). Nothing in the paper charges δ to each packet held behind a hole.

Under U, two things are missing from §1's J:
- **The late fill's own delay.** After a release the frontier resumes, so the packet's remaining wait is in no term. But it is delivered at its fill time R under *any* hold, so its delay does not depend on the release decision. It belongs in J as §7.3's lost-packet delay term, which is policy-invariant here. It does not move H_o, and U is not flattered once that term is present.
- **Its disorder.** It enters only as `ω·1{h < R < ∞}`. A hole never filled costs no disorder: its loss is the sender's counted verdict, not the release.

```text
   J_U  =  ( N_rep  +  (δ/d) · T_stall  +  (δ/d) · Σ_{holes} R_s  +  ω · N_dis ) / S_del          N_dis = released holes later filled
```

| symbol | provenance |
|---|---|
| T_stall, N_rep, S_del, d | measured, as in skip-rule §1 |
| (δ/d)·Σ R_s | paper §7.3, the lost packet's own delay. Policy-invariant under U |
| ω | declared, new, measured (§1) |

skip-rule §4.4's "U ≈ 10 ms against R1 ≈ 16 ms" compares T_stall only. The late packet itself arrives at ≈ 16 ms under both. §4.4 is amended on this branch.

## 3. Deriving H_o

### 3.1 The exact rule, and why it cannot ship

For one hole with hold h, the release-dependent part of J_U, with n ≡ 1 on the paper's frontier pricing (n(t) for the alternative of §3.4), is

```text
   C(h)  =  (δ/d) · E[ ∫₀^{min(h,R)} n(t) dt ]  +  ω · P(h < R < ∞)
   dC/dh =  S(h) · [ (δ/d)·n(h)  −  ω·λ(h) ]                     S = P(R > h),  λ = fill hazard
   hold at age h   ⇔   ω · λ(h)  ≥  (δ/d) · n(h)
```

- **At N = 1, λ = 0 on [0, t_min).** No fill can arrive before the repair round trip. So the first crossing is h = 0 at **every** δ, and the myopic rule releases Bulk at detect. Refuted.
- **The global argmin jumps.** C rises on [0, t_min), falls across the fill mass near one RTT, and rises again. Its global minimum switches between h = 0 and "just past the first repair" at `δ* ≈ ω·d·(1 − ε)/τ₁`. That behaves like `release_now = δ > ω`, a threshold renamed. Unlike skip-rule §3.4's T_ρ step, release is a timer, so "no decision event falls in the gap" does not apply.
- **The hazard is self-censored once releases happen**, the same problem as skip-rule §3.4's Ŝ.
- `request_bar`'s `c/(w + c)` does not transplant. As a quantile level it is ½ at Auto, which would release about half the holes before their fill.

### 3.2 The rule proposed: rent or buy

Holding is renting: it costs δ/d per stalled second. Releasing is buying: it costs ω if the fill later comes. The deterministic rent-or-buy rule buys when the rent paid equals the purchase price. This is the ski-rental problem of competitive analysis, from Karlin, Manasse, Rudolph and Sleator, *Competitive snoopy caching*, Algorithmica 3:79–119 (1988), doi 10.1007/BF01762111; the citation is confirmed.

"Release a hole when the δ-priced stall it has caused reaches the price of the disorder it may cause."

```text
   B_s(t)  =  (δ/d) · ( t − t_detect(s) )                release s at the first t with  B_s(t) ≥ ω
   H_o(δ)  =  ω · d / δ
```

| symbol | provenance |
|---|---|
| t_detect(s) | measured: the first arrival above s |
| δ | declared dial, `net::delta_price` |
| d | measured, paper §7.3 (ARQ resolution delay) |
| ω | declared, new, measured (§1) |

- **Order is automatic.** Any arrival above s is also above every hole s' < s, so `t_detect(s') ≤ t_detect(s)`. A lower hole always reaches ω first, and the holes of a run detected together release together. One deadline is needed, the lowest hole's, like C12's.
- **Guarantee, stated self-contained (isolated hole).** Let OPT be the offline cost that knows R. Then `cost ≤ 2·OPT + ω·1{R = ∞}`:
  - if R ≤ H_o, the rent paid is ≤ ω, and that is OPT;
  - if H_o < R < ∞, the cost is ω of rent plus ω of disorder, and OPT = min(rent to R, ω) = ω;
  - if R = ∞, the cost is ω and OPT = 0. That term is bounded by the verdict rate, ≈ ε·ĥ (skip-rule §3.4).

  The bound is proven for an isolated hole. For overlapping stalls (42–67 % of holes in I1, skip-rule §6) it is a sketch, and the toy scores the rule against §6's LP bound. The rule needs no hazard estimate, so it has no self-censoring problem.
- **What it gives up.** It does not exploit a known reorder spike. At N ≥ 2 that matters (§5).

### 3.3 Shape

- `H_o = ω·d/δ` is exactly inversely proportional to δ (hypothesis confirmed on the paper's pricing). It is linear in ω and in the measured d, and does not depend on the rate.
- In a blackout at N = 1 no hole is detected until data resumes, and the fill follows one RTT later. So a blackout does not trigger a release. A one-path outage at N = 2 does (§4.3).

### 3.4 Alternative pricing, not adopted: per held packet

Charging δ/d per packet-second to every packet held behind the hole gives `B_s = (δ/d)·Σ_{j held} (t − max(a_j, t_detect))`, and in steady state `H_o = (√(n₀² + 2gωd/δ) − n₀)/g`, which is ∝ δ^(−1/2) at high rate.
- That is a second new law: a per-packet delay price the paper does not have. Adopting it needs a paper extension of §7.3 first, with its own derivation.
- It is carried here only so that the effect is visible: it releases Auto far earlier (§4.1).

## 4. Behaviour at the presets (N = 1)

### 4.1 Numbers

**Example** (skip-rule §8): d = 11 ms. The fill arrives τ₁ = 11 ms after detect with probability 1 − ε (ε = 2.5 %), and each further round adds about 25 ms (the refresh floor, C12).

rbf ≈ ε^k, where k is the number of rounds that complete before H_o. With fills at exact round times this column is a staircase. Real fills are jittered, and behaviour continuity is the toy's nudge check (§9), not shown here. Brackets give H_o at δ × 0.98 and δ × 1.02.

**Primary: frontier pricing, `H_o = ω·d/δ`.**

| ω | Bulk δ = 0.005 | Auto δ = 0.5 | Realtime δ = 50 |
|---|---|---|---|
| 1 | 2.2 s [2.244, 2.156], rbf ≈ 0 | 22 ms [22.4, 21.6], rbf **0.025** | 0.22 ms [0.224, 0.216], released before first repair |
| 3 | 6.6 s, rbf ≈ 0 | 66 ms [67.3, 64.7], rbf 1.6·10⁻⁵ | 0.66 ms |
| 10 | 22 s, rbf ≈ 0 | 220 ms, rbf ≈ 0 | 2.2 ms |
| 100 | 220 s, rbf ≈ 0 | 2.2 s, rbf ≈ 0 | 22 ms, rbf 0.025 (holds through the first repair) |

**Alternative (§3.4): per-packet pricing, g = 10 000 packets/s, n₀ = 1.**

| ω | Bulk | Auto | Realtime |
|---|---|---|---|
| 1 | 20.9 ms, rbf 0.025 | 2.0 ms, rbf **1** | 0.13 ms |
| 10 | 66 ms, rbf 1.6·10⁻⁵ | 6.5 ms, rbf **1** | 0.57 ms |
| 100 | 210 ms, rbf ≈ 0 | 20.9 ms, rbf 0.025 | 2.0 ms |
| 1 250 (= W²/8 at W = 100) | 742 ms | 74 ms, rbf 1.6·10⁻⁵ | 7.3 ms |

Reading:
- **The law is continuous.** Each ±2 % nudge moves H_o by ∓2 % (frontier pricing). There is no step at or between presets.
- **Realtime's release latency is whatever ω gives:** 0.2–22 ms on frontier pricing over ω = 1–100, and 0.1–7 ms on the alternative. That is not today's ≈ 10 ms by construction. Any earlier statement that U keeps Realtime at today's latency is withdrawn.

### 4.2 The requirement as a condition on ω

"Auto holds through every fill up to age τ_op" (frontier pricing):

```text
   ω  ≥  δ_Auto · τ_op / d
```

In the example: ω ≥ 1.6 for two rounds (τ_op = 36 ms), 2.8 for three (61 ms) and 9.1 for 200 ms. Bulk needs 1/100 of that. So at N = 1 Bulk keeps order at any ω ≥ 0.03, and Auto needs ω ≳ 3. Whether the measured ω clears that is the question of §10.

### 4.3 Where a finite hold changes Bulk

- **A one-path outage at N ≥ 2.** The live path keeps delivering and the dead path's seqs are holes. Bulk releases them after `H_o = ω·d/0.005` (2.2 s at ω = 1, d = 11 ms). Today it holds until the path returns or the sender re-places the seqs. The inner flows then get those seqs late and out of order instead of after a stall.
- **Persistent repair loss on one hole** (k rounds lost, ε^k): the same, after H_o.
- **Nothing is lost.** The ack keeps reporting the hole and the sender keeps serving it (U).

## 5. N ≥ 2: reordering without loss

**Mechanism.** A hole is detected on any arrival above it. With per-path one-way delays differing by Δ, a packet placed on the slow path is "missing" from the moment a later seq arrives on the fast path, until its own original arrives Δ later. No loss is involved. Under U it is released past, and then delivered out of order, iff

```text
   H_o  <  Δ_j            (Δ_j = that packet's lateness behind its first faster successor)
   slow-path originals delivered out of order  ≈  φ_slow · P( Δ_j > ω·d/δ )
```

Here φ_slow is the placement share on the slow path, measured (the receiver's per-path first-copy counts, `c8r_first`).

**At the presets, with skew Δ = 20 ms** (skip-rule §6's N = 2 cell, owd 5 and 25 ms). Holding every skewed packet needs `ω ≥ δ·Δ/d`:

| measured d | Bulk | Auto | Realtime |
|---|---|---|---|
| 11 ms | ω ≥ 0.009 | ω ≥ 0.91 | ω ≥ 91 |
| 0.78 ms (paper §7.3, c7) | ω ≥ 0.13 | ω ≥ 12.8 | ω ≥ 1 282 |

- **Realtime at N = 2** with any plausible ω delivers essentially its whole slow-path share out of order, all the time, not only on loss.
- **Auto at N = 2** keeps order only if ω clears δ_Auto·Δ/d. At a dual cell's small measured d that is ω ≳ 13. A small measured ω (§1, RACK and DSACK undo) would make Auto reorder in normal operation, a default change for Auto.
- **Bulk** keeps order at N = 2 for any ω above about 0.1.

**How this touches the pending owner decision on Realtime at N ≥ 2 (rho-step-sites C1 and C3).**
- C1 moves Realtime from best-path to striping. Today Realtime's slow path carries almost nothing (φ_slow ≈ 0, apart from the hint-keyed duplicate send), so skew causes no holes.
- With C1 and U together, φ_slow > 0, and every slow-path packet is released past and arrives out of order. The owner's confirmation of C1's default change must therefore be taken *with* this consequence, and with the measured ω, not separately.
- C3 (the per-path hole law at the sender) suppresses spurious *resends* for skewed seqs. It does nothing for the receiver's release, because the receiver detects a hole on any later arrival. It does not know which path an unarrived seq was sent on, so it cannot run C3's per-path clock without a wire field.
- **Open, not derived here:** detection on a per-path basis, which removes skew-induced holes, would need the sender's path map at the receiver: one wire field, or a per-path seq. It is recorded as an option, not proposed.

**Simulation readout (added to skip-rule §6).** Per N = 2 cell and preset, report the out-of-order deliveries per delivered packet, split into *skew* (released, then filled by its own original, with no copy sent) and *loss* (filled by a copy). Also report φ_slow and the Δ_j distribution.

## 6. Gauges

Each gauge is printed per preset and per stage (FORMULA-FIRST: every term that can bind is gauged):
- **rbf, the release-before-fill fraction** = released-then-filled ÷ holes detected. At Bulk and Auto it must lie within the CI of 0 in the operating-range cells (skip-rule §6's I1, I2, G). It may be nonzero in outage cells (B10, B60 at N = 2), and must be reported there. This gauge, not H_o = ∞, is the demonstration the restated requirement asks for.
- **Released-then-filled count** (today's `late_after_aban`, `receiver.rs:1583`, same event).
- **Released holes outstanding**: level and maximum of the released, unfilled, unverdicted set. Also the count of evictions from its cap (§7).
- **Released, never filled**: must equal the released holes later passed by a floor (a cross-check against G_rx).
- **Displacement of late fills**, in seqs (p50, p99, and the fraction ≥ 3, RFC 5681's DupThresh) and in time (fill − release). This is the input to ω's measurement (§1).
- **Skew releases (N ≥ 2)**: released holes later filled by their own original with no copy sent, per path, per delivered packet (§5).
- **Which term resolved each hole**: fill before H_o, release at H_o, or floor pass at t_F + H (§8). This is the bind fraction of H_o.

## 7. Receiver state for released holes

- **Two frontiers, and every floor keyed on the ack.** Today the expiry moves `next_expected` with delivery (`receiver.rs:1024`). Under U, `next_expected`, which is A_rx in skip-rule §4.1, must not move on a release. It also feeds `decode_inert_floor` (`receiver.rs:666`), the `received_seqs` prune and `window_decoder.advance`, and every one of these must stay keyed on A_rx, never on the delivery frontier `reorder.next_deliver_seq()`. If any followed delivery, the sender would prune the hole (an uncounted loss, today's defect), and a late fill more than 2·W below delivery would be refused by the decoder.
- **Dedup of late copies.** `ReorderBuffer::push_with_time` hands out *any* seq below `next_deliver_seq` at once, with no membership test (`reorder.rs:75–82`). After a release, a duplicate copy of an already-delivered seq would be delivered twice if it reached the buffer. The dedup that prevents this is the decoder's: `src_seen` refuses a repeated source seq (`unified.rs:390`, `rlc_window.rs:468`), and the trait's contract returns each seq at most once (`window_traits.rs:155`).
  - That holds on `[floor, floor + 2²⁴)` (`seq_bits.rs` `MAX_SPAN`). Under U the floor stays at or below `A_rx − 1 − 2W`, and the received span above it is rate × (time to the hole's fill or verdict). B does not bound it, because SACKed seqs leave O (the open item below). At 10 000 packets/s, 2²⁴ seqs is about 28 minutes.
  - Past `MAX_SPAN`, `SeqBits::insert` refuses and the decoder treats the seq as fresh (`|| !contains`). Under U the reorder buffer would then hand a duplicate out again. So `SeqBits::refused` becomes a **correctness gauge that must read 0**, not only a SACK under-claim.
  - Owed test: a duplicate of a seq delivered past a release is not delivered twice, at N = 2.
- **The released set.** It is `abandoned_seqs` (`receiver.rs:271`). Entries leave it on fill (`:1583`). Under U they must also leave it when A_rx passes them on a floor (the counted verdict: lost, in G_rx).
  - **Bound.** A released, unfilled hole is neither acked nor SACKed, so it stays in the sender's O, and O ≤ B = N·2048. So |released| ≤ B, which is below `ABANDONED_TRACK_MAX` = 65 536 for N ≤ 31.
  - The oldest-first eviction at the cap is not a correctness mechanism: delivery of a late fill does not read the set. An eviction only undercounts the gauges. It gets an eviction counter, which must read 0.
- **SACK release (C2).** Seqs received above a released hole are SACKed and leave O. The hole stays in O and in the store, and is served. The sender keeps the SACKed payloads until the cumulative frontier passes them (ADR-0060, paper §6.3), so U frees no sender memory. It frees little receiver memory: released payloads leave the reorder buffer's `pending`, but the decoder keeps received payloads above `decode_inert_floor` (`unified.rs:183` `recovered`, `receiver.rs:655–672`) until A_rx moves, that is until fill or verdict. rho-step-sites' open item (nothing derived bounds the receiver's state above a stuck hole under SACK release) is therefore **not** closed by U, at any δ.
- **A full consumer (C10).** The release must deliver through the holding path (`WindowDelivery::offer`). Today's expiry path calls `try_send` and drops on Full (`receiver.rs:1023`). Under U a dropped released packet would be an uncounted loss.

## 8. Interaction with H, the floor-pass hold (C11, UNDERIVED)

**What H guards under U.** The floor F arrives with the verdict's last copy. After t_F, the only possible fills are that copy, earlier copies still in flight, or the original on a slower path.
- **The ack and G_rx side.** A_rx may pass the hole once no fill can still arrive. Passing early is conservative: G_rx is a level that falls on a late fill (skip-rule §3.1), so an early pass only overstates Ab briefly.
- **The delivery side, only where H_o has not yet fired** (long H_o with ρ < 1, where the floor can come first). After the fill window the fill probability is 0, so §2's objective releases at once: holding longer is pure rent with nothing to buy.

**What this gives H.** H falls out of the same objective, but not out of the δ law. It is the end of the support of the remaining fills relative to the floor, a path quantity:

```text
   H  =  max over paths p still carrying a copy or the original of s ( owd_p − owd(first floor path) )⁺  +  the in-path reorder spread
```

- **At N = 1**, with the copy and the floor sent on one path at one instant (skip-rule §3.1), H ≈ the in-path reorder ≈ 0. Today's SRTT/2 overstates it.
- **At N ≥ 2**, H ≈ the path skew Δ (§5) when the floor and a copy or original travel on different paths.
- Which measured quantile bounds the spread is open. The candidate is the observed maximum, like §3.4's t_max, with a bind gauge.
- **Proposed** answer to C11's open question of H's kind: H is a reorder cover, a path quantity, not a δ quantity. `min(b(δ), 2)·SRTT` has no derivation here.

**Reconciled with skip-rule-efficiency on this branch.** Its tables assumed H = 5 ms.
- At N = 1 with H ≈ 0, F1's latency penalty (a copy-less shed resolves H later than a resend) vanishes.
- The last copy then rests on saved allowance alone. A copy-less verdict always spends a unit, about ĥ in all; with the copy, only the failed copies spend, about ε·ĥ, 40× less in the example.
- At N ≥ 2, H ≈ Δ > 0 and F1's latency penalty stands.
- skip-rule's short answer, §3.1, §4.2, §4.4 and §8 now state F1 per H value.

**One hold cannot serve both.** H_o is a price ratio (δ against ω); H is a support bound (paths). They compose in one deadline, with both terms always computed and absent = ∞, as C12's deadline does:

```text
   release_deadline(s)  =  min( t_detect(s) + ω·d/δ ,   t_F + H )          t_F = ∞ while no floor covers s (always at ρ = 1)
```

## 9. Continuity (NO-MODE-SWITCH)

| quantity | in δ | in ρ | hint | at the edges |
|---|---|---|---|---|
| H_o = ω·d/δ | continuous, strictly decreasing, ∝ 1/δ | not read | not read | ω = 0: release at detect (`window_out_of_order`'s delivery). δ → 0: H_o → ∞ only in the limit, never on a stretch |
| H | not read | through t_F only (no floor at ρ = 1) | not read | — |
| min(·, ·) | both terms always computed | | | |

- **The law is continuous. The behaviour, rbf = S(H_o(δ)), is continuous only where the fill-time survival S is.** With fixed path delays S is a staircase. So the toy must jitter delays (skip-rule §6, amended), and behaviour continuity is the toy's ±2 % nudge check (P3), reported, not claimed.
- The protocol hints are named δ points on one curve. Explicit ρ = 1 with the Realtime δ releases, with no loss (skip-rule §5).
- **Owed tests**, in the style of `test_continuum_one_law_across_the_dial`:
  - H_o at each preset against its ±2 % nudges, monotone with no step;
  - an absolute H_o at the anchors for fixed (ω, d);
  - rbf continuity across every preset in the toy.
- **The toy** (skip-rule §6) sweeps ω ∈ {0, 0.3, 1, 3, 10, 100} instead of H_o, adds the gauges of §6, the skew readout of §5, and a cell with inner TCP displacement scored against DupThresh.

## 10. What this blocks

- **Paper first.** Before any U code, paper §5.1 (two frontiers) and a new §7 subsection owe this law: `B_s ≥ ω` on §7.3's frontier pricing, its derivation, ω's provenance, the 2× bound, and §3.1's refutation of the hazard rule.
- **The ω measurement** (roadmap node `meas-disorder-price`) comes after VM batch 1.
- **C9, C11 and C12's release arm** waits on the measured ω and on the toy's rbf and skew readouts at Bulk and Auto.
- **R1 (sender-only, no release)** is unaffected. It is the ω = ∞ point of this law, and it stays the staging point.

## 11. For the owner

Under U, waiting less never loses a packet. The packet still comes, just after the ones behind it. So how long the receiver waits is set by one new number: **ω, the price of one packet delivered out of order**. Realtime's wait is whatever ω gives, somewhere between about 0.1 and 22 ms in the tables. It is not held at today's ≈ 10 ms.

**Two questions are yours.**
1. **May ω be set by measurement?** The proposal is a TCP-in-tunnel test that forces late, out-of-order deliveries and measures what they cost the inner TCP. It gets its own roadmap node and runs after VM batch 1.
   - **Warning.** Modern TCP (RACK reordering detection, DSACK undo) tolerates reordering well, so the measured ω may come out small.
   - If it does, Auto and Bulk would deliver out of order in normal operation: at one path when a repair takes a second round, at Auto at two paths for every slow-path packet. That is contrary to rho-step-sites' "no Bulk or Auto default changes".
   - You would then choose between that change and a larger declared ω, which keeps today's order.
2. **At Bulk, after a long outage** (for example one of two paths dead for longer than the hold, about 2 s if ω = 1), **is late-and-out-of-order delivery acceptable instead of a stall?** Nothing is lost either way.
