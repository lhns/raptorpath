# The ordering hold H_o(δ) for design U (derivation, no code)

**Question.** Design U (`skip-rule-efficiency.md` §4.1) splits today's receiver give-up into *release* (deliver past a hole; an ordering act, no loss) and *stop* (the sender's counted verdict). U needs a law for when the receiver releases: pass hole s for delivery at `t_detect(s) + H_o`. §4.1 asked for an H_o that is "never" at Bulk and Auto and short at Realtime. Stated that way it is a mode switch: a function that is infinite on a whole stretch of δ is a threshold renamed.

**Restated requirement.** At Bulk and Auto, release must not fire before a fill or a verdict *in the operating range*. That is shown by a gauge (§5), not by H_o = ∞. Where a finite hold does change Bulk, this document says so (§4.3).

**Short answer.**
- There is no price of disorder in the paper. It is a new declared quantity ω, and its value is the owner's call (§1).
- skip-rule §1 prices frontier seconds. Under U that treats a released packet's late fill as free and leaves disorder unpriced. The objective is restated on per-packet delay plus ω per released-then-filled hole (§2).
- The exact optimal release rule is not usable. It reads a fill hazard that is 0 before the first repair round trip, so it either releases at detect for every δ, or it jumps between "release now" and "hold through the repair" at one δ (§3.1).
- The rule proposed is rent-or-buy. Release a hole when the δ-priced delay it has imposed on the packets held behind it reaches the disorder price (§3.2):

```text
   release s at the first t with      (δ/d) · Σ_{j ∈ P_s(t)} ( t − max(a_j, t_detect(s)) )   ≥   ω
```

- In steady state (arrival rate g, n₀ packets already held at detect) that is

```text
   H_o(δ)  =  ( √( n₀² + 2·g·ω·d/δ ) − n₀ ) / g
```

- H_o is continuous and strictly decreasing in δ, finite for every δ > 0 and every ω < ∞, and H_o → 0 as ω → 0 (the out-of-order arm). It reads no ρ and no hint. The hypothesis "H_o ∝ 1/δ" holds only at low rate. At high rate H_o ∝ δ^(−1/2) (§3.3).
- Whether Bulk and Auto keep strict order depends only on ω. With ω = 1 (one wasted packet, the `W_COST_RATIO` analogy), Bulk at 10 000 packets/s releases 2.5 % of holes before their fill, and Auto releases every hole. With the Reno cwnd-halving price ω = W²/8, Bulk and Auto hold through every operating-range fill from 10 000 packets/s up. Because that price shrinks as the square of the rate, slow flows release early: at 1 000 packets/s Auto releases the 2.5 % of holes whose first repair fails, at 100 packets/s Auto releases every hole, and at 10 packets/s Bulk releases at about 3 ms (§4).

**Inputs read** (`github/main` `ca555715`):
- `skip-rule-efficiency.md` §1, §3.4, §4.1 and §6;
- `rho-step-sites.md` C2, C9, C10, C11 and C12;
- paper `docs/fec-arq-model.md` §6.4, §7.3, §7.6 and §7.7;
- `net/late.rs:64–101` (`W_COST_RATIO`, `request_bar`) and `net/store_cap.rs:316` (`delta_price`);
- `net/reorder.rs:60–130`;
- `net/receiver.rs`: `:265–271` (`abandoned_seqs`, `ABANDONED_TRACK_MAX`), `:654–672` (prune at `decode_inert_floor(next_expected)`), `:1012–1024` (the expiry moves `next_expected`) and `:1570–1590` (`late_after_aban`);
- `fec/unified.rs:380–400`, `fec/rlc_window.rs:456–475` and `fec/window_traits.rs:151–160` (decoder dedup and its return-once contract);
- `net/seq_bits.rs:1–32` (`MAX_SPAN` = 2²⁴);
- `net/report.rs:25–29` (`RACK_SPURIOUS_BUDGET`);
- RFC 5681 §3.1–§3.2 and RFC 8985 §1.1, §3.3.1 and §6.2 (texts checked).

**Words.**
- A hole's *fill time* R is measured from `t_detect(s)`. R = ∞ if the hole is never filled (a verdict whose last copy is also lost).
- `P_s(t)` is the set of received, undelivered packets above s at t.
- `a_j` is packet j's arrival.
- *Displacement* is the number of successors delivered before a released-then-filled packet, and the time between its release and its fill.

## 1. The objective, and the missing price of disorder

**What release buys.** Every packet held behind a hole waits until the hole resolves. Releasing at hold h ends that wait for all of them.

**What release costs.** If the hole is filled after h, the consumer gets that packet after its successors. The consumer is the TUN inject channel (`net/delivery.rs`, `receiver.rs:172`). It receives IP packets, so this is reordering seen by the inner flows, not byte-stream corruption. Each tunnel seq carries one or more IP packets (`extract_window_packets`, `receiver.rs:1014`), so one late seq displaces a bundle.
- **Inner TCP, classic.** Three duplicate ACKs trigger fast retransmit with `ssthresh = max(FlightSize/2, 2·SMSS)` (RFC 5681 §3.2, eq. 4). Any displacement of three or more segments of that flow costs one spurious retransmission and one cwnd halving.
- **Inner TCP, RACK-TLP.** RFC 8985 §6.2 marks a segment lost once a later-sent one is delivered and `RTT + reo_wnd` has elapsed, with `reo_wnd = min(reo_wnd_mult·min_RTT/4, SRTT)`. reo_wnd is 0 when no reordering has been seen and the sender is in recovery or has ≥ DupThresh (3) segments SACKed. Each round with a DSACK raises reo_wnd_mult, which persists for 16 recoveries. So a displacement shorter than about min_RTT/4 of the *inner* flow is tolerated once reordering has been seen. The tunnel cannot observe the inner min_RTT, so this time tolerance cannot be used by the tunnel. The count tolerance (DupThresh = 3) can be.
- **Inner UDP (realtime media, games).** Usually harmless: a jitter buffer reorders. A packet later than its playout point is lost to the application whether or not it was released.

**What the paper has.** The paper prices delay (δ, `net::delta_price`, §4.1) and a wasted copy (w = 1, `W_COST_RATIO`, `net/late.rs:64`, §7.3 and §7.6). It has no price for disorder delivered to the consumer.
- `RACK_SPURIOUS_BUDGET = 1/16` (`net/report.rs:29`) is the bar for the tunnel's *own* spurious repairs. It is not a consumer price.
- `window_out_of_order` (`config.rs:92`) is a declared boolean that turns ordering off entirely. In the language below it is the ω = 0 end, not a price.
- §6.4's RFC 9002 and RACK factors set a stall duration, not a disorder cost.

**So ω is a new declared quantity, and its value is the owner's call** (§7):

```text
   ω  =  price of one released-then-filled hole, in repair symbols (the unit of δ and w)
```

The simplest form is one constant per hole, charged when the late fill arrives. Three provenance options:

| option | value | provenance | weakness |
|---|---|---|---|
| (a) unit | ω = 1 | analogy to `W_COST_RATIO`: one spurious inner retransmission is one wasted packet | ignores the cwnd halving; §4 shows it changes Bulk at ≥ 10 000 packets/s and Auto everywhere |
| (b) cited Reno halving | ω = W²/8, with W = g·SRTT in packets | RFC 5681 eq. 4 (halving) plus additive increase of 1 SMSS per RTT: the send deficit of one halving at window W is W/2 packets over W/2 RTTs, W²/8 in total | assumes one inner Reno flow fills the tunnel and no DSACK undo. With k flows the halving hits one flow of window W/k: W²/(8k²). W uses the tunnel SRTT, a lower bound on the inner RTT, so this is not a clean upper end either way |
| (c) measured | ω = inner goodput deficit per released-then-filled hole, in packets | a TCP-in-tunnel battery with releases forced by a test seam, counting inner retransmissions and goodput deficit (`iperf3` retr) per released-then-filled hole | needs the battery; depends on the inner stack (Linux RACK with DSACK undo, Cubic) |

**Recommendation.** Use (c), with (b) as its stated upper bound. (a) is listed only to show what a unit price does. Do not back-solve ω from today's Realtime SRTT/2. That would be calibration, not provenance.

## 2. skip-rule §1's objective under U

skip-rule §1 prices `T_stall`, the frontier-blocked seconds. Under U the frontier resumes at release, so:
- the late fill's own wait after release is in no term of J, so the late fill counts as **free**;
- its disorder is in no term of J;
- and a stall of length s blocks about `g·s` packets, yet is priced as one second per second.

§4.4's "U ≈ 10 ms against R1 ≈ 16 ms" compares frontier stall only. The late packet itself still arrives at ≈ 16 ms under both.

**Restated objective (per packet, with disorder).**

```text
   J_U  =  ( N_rep  +  (δ/d) · Σ_{j delivered} D_j  +  ω · N_dis ) / S_del
   D_j  =  delivery_j − ( send_j + owd_min )                        per-packet excess delay, late fills included
   N_dis = released holes later filled
```

| symbol | provenance |
|---|---|
| D_j | measured. For a late fill it contains its whole resolution, so it is never free |
| δ/d | paper §7.3: one ARQ resolution delay d of a genuine loss costs δ repair symbols, the price of the *lost packet's own* delay. §7.3 prices the frontier separately (Φ). Charging δ/d per packet-second to the successors too is **this document's extension**, not the paper's. The frontier-priced alternative (n ≡ 1) is carried beside it in §4.1 |
| ω | declared, new (§1) |
| N_rep, S_del, d | measured, as in skip-rule §1 |

**How the late fill enters.**
- **Its delay.** s is delivered at its fill time R under any hold. Its D_s does not depend on the release decision. It is in J, so U is not flattered, but it does not move H_o.
- **Its disorder.** It enters only as `ω·1{h < R < ∞}`. A hole that is never filled costs no disorder: its loss was the sender's counted verdict, not the release.

The count constraint is unchanged and does not involve release (U, O3 = yes).

## 3. Deriving H_o

### 3.1 The exact rule, and why it cannot ship

For one hole with hold h, the release-dependent part of J_U is

```text
   C(h)  =  (δ/d) · E[ ∫₀^{min(h,R)} n(t) dt ]  +  ω · P(h < R < ∞)          n(t) = |P_s(t_detect + t)|
   dC/dh =  S(h) · [ (δ/d)·n(h)  −  ω·λ(h) ]                                   S = P(R > h),  λ = fill hazard
```

"Hold while the fill hazard, priced at ω, exceeds the priced growth of the held set":

```text
   hold at age h   ⇔   ω · λ(h)  ≥  (δ/d) · n(h)                    n(h) = n₀ + g·h in steady state
```

- With a flat hazard λ, this gives `H_o = (ωλd/δ − n₀)/g`, which is continuous and ∝ 1/(δg).
- It is the receiver-side analogue of §7.6 (a hazard level set at a price-set height). But it does not survive the real hazard:
  - **At N = 1, λ = 0 on [0, t_min).** No fill can arrive before the repair round trip. The first crossing of the level set is h = 0 at **every** δ, so the myopic rule releases Bulk at detect. Refuted.
  - **The global argmin jumps.** C rises on [0, t_min), falls across the fill mass near one RTT, and rises again. Its two local minima are h = 0 and h just past the first repair. The global minimum switches between them at `δ* ≈ ω·d·(1 − ε)/∫₀^{τ₁} n`, which depends on measured quantities. With d ≈ τ₁ and frontier pricing (n ≡ 1), δ* ≈ ω(1 − ε): the rule behaves like `release_now = δ > ω`. That is a threshold renamed, a behaviour step in δ, which CLAUDE.md forbids. Unlike skip-rule §3.4's T_ρ step, release is a timer, so "no decision event falls in the gap" does not apply.
  - **The hazard is self-censored once releases happen.** That is the same problem as §3.4's Ŝ.
- `request_bar`'s `c/(w + c)` does not transplant. As a quantile level it is ½ at Auto, which would release about half the holes before their fill.

### 3.2 The rule proposed: rent or buy

Holding is renting: it costs `(δ/d)·n(t)` per second. Releasing is buying: it costs ω if the fill later comes. The deterministic rent-or-buy rule is "buy when the rent paid equals the purchase price" (ski rental; Karlin, Manasse, Rudolph and Sleator, *Competitive snoopy caching*, Algorithmica 3, 1988; cited from memory, not re-fetched here, so the reference is to be confirmed).

"Release a hole when the δ-priced delay it has already imposed on the packets held behind it reaches the disorder price."

```text
   B_s(t)   =  (δ/d) · Σ_{j ∈ P_s(t)} ( t − max(a_j, t_detect(s)) )          accrued priced hold, packet·seconds × δ/d
   release s at the first t with  B_s(t) ≥ ω        (and every hole below s first, see below)
```

| symbol | provenance |
|---|---|
| P_s(t), a_j | measured: the reorder buffer already stores each pending entry's arrival instant (`reorder.rs:83`, `pending.insert(seq, (data, now))`) |
| t_detect(s) | measured: the first arrival above s |
| δ | declared dial, `net::delta_price` |
| d | measured, paper §7.3 (ARQ resolution delay) |
| ω | declared, new, owner (§1) |

- **Sharing the hold.** Between arrivals, B_s is linear in t, so the deadline is closed-form. With k held packets and Σ the sum of their `max(a_j, t_detect)`, the deadline is `t* = (ω·d/δ + Σ)/k`. It is recomputed at each arrival, one timer, like C12's deadline.
- **Order is automatic, with no rule.** Any arrival above s is also above every hole s' < s. So `t_detect(s') ≤ t_detect(s)` and `P_{s'} ⊇ P_s`, hence `B_{s'} ≥ B_s`. A lower hole always reaches ω first, and the holes of a run detected together release together. Run-atomicity at the receiver falls out of the formula.
- **Guarantee, for any fill distribution, for an isolated hole (a sketch).** Per hole, `cost(rent-or-buy) ≤ 2·OPT + ω·1{R = ∞}`, with OPT the offline cost knowing R:
  - if R ≤ H_o, the cost is the rent to R, which is ≤ ω, and that is OPT;
  - if H_o < R < ∞, the cost is 2ω, and OPT = min(rent to R, ω) = ω;
  - if R = ∞, the cost is ω and OPT = 0.

  The additive term is bounded by the verdict rate, ≈ ε·ĥ (skip-rule §3.4). The rule needs no hazard estimate, so it has no self-censoring problem.

  The bound is proven only for an isolated hole. B_s counts every packet above s, including packets that a higher hole still holds after s is released. Under overlapping stalls (42–67 % of holes in I1, skip-rule §6) rent is charged twice and the bound is unproven. The toy scores the rule against skip-rule §6's LP bound.
- **What it gives up.** It does not exploit a known reorder spike. At N ≥ 2, a slow-path original arriving shortly after detect (high self-heal, π₀ up to 0.96 at c7) is released if H_o is shorter than the path skew. The 2× bound still holds. A distribution-aware refinement must itself be continuous in δ (§3.1 shows the naive one is not), and it is open.

### 3.3 Closed form and shape

With steady arrivals at rate g and n₀ packets held at detect, `B(h) = (δ/d)(n₀h + g·h²/2)`, so

```text
   H_o(δ)  =  ( √( n₀² + 2·g·ω·d/δ ) − n₀ ) / g
```

- **Low rate** (g·H_o ≪ n₀): `H_o ≈ ω·d/(δ·n₀)`, inversely proportional to δ. Hypothesis confirmed in this limit, and also in frontier pricing (n ≡ 1), where `H_o = ω·d/δ` exactly.
- **High rate**: `H_o ≈ √(2ω·d/(g·δ))`. The hypothesis is refuted here: H_o falls as δ^(−1/2) and as g^(−1/2). A fast Bulk link shortens the hold unless ω grows with the rate, as option (b) does (W ∝ g gives `H_o ≈ SRTT·√(g·d/(4δ))`, which grows with √g).
- **In a blackout** (N = 1, nothing arrives) no hole is detected and B does not grow. Holes are detected when data resumes and filled about one RTT later, so a blackout does not trigger a release. A one-path outage at N = 2 does: arrivals continue on the live path (§4.3).
- **Bound.** `H_o ≤ ω·d/(δ·n₀)` with n₀ ≥ 1, because a detected hole has at least its exposer held. So H_o is finite for every δ > 0 and every ω < ∞.

## 4. Behaviour at the presets

### 4.1 Numbers

**Example** (the same one as skip-rule §8): d = 11 ms, SRTT = 10 ms and n₀ = 1. The fill arrives τ₁ = 11 ms after detect with probability 1 − ε (ε = 2.5 %), and each further round adds about 25 ms (the refresh floor, C12).

The release-before-fill fraction is `P(H_o < R < ∞) ≈ ε^k`, where k is the number of rounds that complete before H_o. Brackets give H_o at δ × 0.98 and δ × 1.02. Computed by a scratch script, not committed.

| g (packets/s) | ω | Bulk δ = 0.005 | Auto δ = 0.5 | Realtime δ = 50 |
|---|---|---|---|---|
| 1 000 | 1 | 65.3 ms [66.0, 64.7], rbf 1.6·10⁻⁵ | 5.71 ms [5.78, 5.64], rbf **1** | 0.20 ms, rbf 1 |
| 1 000 | W²/8 = 12.5 | 234 ms [236, 231], rbf ≈ 0 | 22.5 ms [22.7, 22.2], rbf **0.025** | 1.55 ms, rbf 1 |
| 10 000 | 1 | 20.9 ms [21.1, 20.7], rbf **0.025** | 2.0 ms, rbf **1** | 0.13 ms, rbf 1 |
| 10 000 | W²/8 = 1 250 | 742 ms [749, 734], rbf ≈ 0 | 74.1 ms [74.8, 73.3], rbf 1.6·10⁻⁵ | 7.3 ms [7.39, 7.24], rbf 1 |
| 100 000 | 1 | 6.6 ms, rbf **1** | 0.65 ms, rbf **1** | 0.06 ms, rbf 1 |
| 100 000 | W²/8 = 125 000 | 2.35 s, rbf ≈ 0 | 235 ms, rbf ≈ 0 | 23.4 ms, rbf 0.025 |

Reading:
- **Continuity of the law.** Each ±2 % nudge moves H_o by about ∓1 % (the δ^(−1/2) regime) to ∓2 % (the 1/δ regime). H_o has no step at any preset or between presets: it is a smooth, strictly monotone function of δ.
- **Continuity of the behaviour is owed, not shown.** The realised behaviour is rbf = S(H_o(δ)). It steps wherever fill times are bunched, and the ε^k column above is exactly that staircase in this idealised example (fills at exact round times). With real jitter S is smooth. Behaviour continuity at each preset is the toy's ±2 % nudge check (skip-rule §6 P3), and it is reported, not assumed.
- **Frontier pricing, for comparison** (n ≡ 1, `H_o = ω·d/δ`, rate-free): at ω = 1, 2.2 s at Bulk, 22 ms at Auto and 0.22 ms at Realtime. The "ω = 1 breaks Auto" conclusion holds under per-packet pricing only. Under frontier pricing Auto at ω = 1 holds through one round (rbf ≈ ε).
- **Slow flows under option (b)** (ω = W²/8 shrinks as g²): at g = 100, H_o = 65 ms at Bulk, 2.5 ms at Auto (rbf 1) and 0.03 ms at Realtime. At g = 10, H_o = 2.7 ms at Bulk (rbf 1).
- **Realtime** (rbf = 1 means "released before the first repair, by design"). At ω = 1, Realtime releases at detect. At ω = W²/8 and 10 000 packets/s, it releases at 7.3 ms, close to today's SRTT/2 = 5 ms. At 100 000 packets/s it holds through the first repair (23 ms), slower than today, because one Reno halving of a 1 000-packet window is priced above 23 ms of delay to the held packets. That is the price speaking, not a defect, but the owner must know it.

### 4.2 The requirement as a condition on ω

"Auto holds through every fill up to age τ_op" is equivalent to

```text
   ω  ≥  (δ_Auto/d) · ( n₀·τ_op + g·τ_op²/2 )
```

In the example (Bulk needs 1/100 of this):

| g | τ_op = 36 ms (two rounds) | 61 ms (three rounds) | 200 ms |
|---|---|---|---|
| 1 000 | 31 | 87 | 918 |
| 10 000 | 296 | 848 | 9 100 |
| 100 000 | 2 947 | 8 460 | 90 918 |

- Option (b)'s W²/8 (12.5, 1 250, 125 000) clears two to three rounds at Auto from 10 000 packets/s up. At 1 000 packets/s it clears one round.
- ω = 1 clears no round at Auto at any rate.
- **So "Bulk and Auto keep strict order" is not a property of the law. It is the owner's choice of ω, plus the gauge (§5) that shows it holds.**

### 4.3 Where any finite hold changes Bulk (for the owner)

- **A one-path outage at N ≥ 2.** The live path keeps delivering and the dead path's seqs are holes. Bulk releases them after H_o (742 ms at 10 000 packets/s with ω = W²/8). Today it holds until the path returns or the sender re-places the seqs. Any inner TCP then sees those seqs late and out of order, instead of seeing a stall.
- **Persistent repair loss on one hole** (k rounds lost, ε^k): the same, after H_o.
- **A slow flow under option (b).** ω = W²/8 → 0 as the rate falls, so Bulk releases every repaired hole below about 100 packets/s (2.7 ms hold at 10 packets/s). A tiny inner window cannot collect three duplicate ACKs anyway, so the disorder there is mostly cheap, but it is a change from today's strict order.
- **A fast link with a rate-independent ω** (options (a) or (c) measured at low rate): `H_o ∝ g^(−1/2)` shrinks to operating-range fill times. That is the 100 000 packets/s, ω = 1 row.
- **Nothing is lost by any of these.** The ack keeps reporting the hole and the sender keeps serving it (U).

## 5. Gauges

Each gauge is printed per preset and per stage (FORMULA-FIRST: every term that can bind is gauged):
- **rbf, the release-before-fill fraction** = released-then-filled ÷ holes detected. At Bulk and Auto it must lie within the CI of 0 in the operating-range cells (skip-rule §6's I1, I2, G). It may be nonzero in outage cells (B10, B60 at N = 2), and must be reported there. This gauge, not H_o = ∞, is the demonstration the restated requirement asks for.
- **Released-then-filled count** (today's `late_after_aban`, `receiver.rs:1583`, same event).
- **Released holes outstanding**: level and maximum of the released, unfilled, unverdicted set. Also the count of evictions from its cap (§6).
- **Released, never filled**: must equal the released holes later passed by a floor (a cross-check against G_rx).
- **Displacement of late fills**, in seqs (p50, p99, and the fraction ≥ 3, RFC 5681's DupThresh) and in time (fill − release). This is the input to ω's option (c) and to any RACK refinement.
- **Which term resolved each hole**: fill before H_o, release at H_o, or floor pass at t_F + H (§6). This is the bind fraction of H_o.

## 6. Receiver state for released holes

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

## 7. Interaction with H, the floor-pass hold (C11, UNDERIVED)

**What H guards under U.** The floor F arrives with the verdict's last copy. After t_F the only possible fill is that copy.
- **The ack and G_rx side.** A_rx may pass the hole once the copy cannot still arrive. Passing early is conservative: G_rx is a level that falls on a late fill (skip-rule §3.1), so an early pass only overstates Ab briefly.
- **The delivery side, only where H_o has not yet fired** (Bulk and Auto δ with ρ < 1, where the floor can come first). Here the fill probability after the copy's arrival window is 0, so §2's objective releases immediately: holding longer is pure rent with nothing to buy.

**What this gives H.** H falls out of the same objective, but not out of the δ law. It is the end of the support of the last copy's arrival relative to the floor's, a path quantity:

```text
   H  =  max over paths p still carrying a copy or the original of s ( owd_p − owd(first floor path) )⁺  +  the in-path reorder spread
```

- At N = 1, with the copy and the floor sent on one path at one instant (skip-rule §3.1), H ≈ the in-path reorder ≈ 0. Today's SRTT/2 overstates it.
- Which measured quantile bounds the spread is open. The candidate is the observed maximum, like §3.4's t_max, with a bind gauge.
- Earlier copies, or the original on a slower path, can also fill after the floor, hence the max over paths.
- **Proposed** answer to C11's open question of H's kind: H is a reorder cover, a path quantity, not a δ quantity. `min(b(δ), 2)·SRTT` has no derivation here.

**One hold cannot serve both.** H_o is a price ratio (δ against ω); H is a support bound (paths). They compose in one deadline, both terms always computed, absent = ∞, as C12's deadline does:

```text
   release_deadline(s)  =  min( t : B_s(t) ≥ ω ,   t_F + H )          t_F = ∞ while no floor covers s (always at ρ = 1)
```

## 8. Continuity (NO-MODE-SWITCH)

| quantity | in δ | in ρ | hint | at the edges |
|---|---|---|---|---|
| B_s(t) ≥ ω | continuous and linear in δ, so H_o is strictly decreasing and smooth | not read | not read | ω → 0: H_o → 0, out of order. δ → 0: H_o → ∞ only in the limit, never on a stretch |
| H | not read | through t_F only (no floor at ρ = 1) | not read | — |
| min(·, ·) | both terms always computed | | | |

- The law is continuous. The realised behaviour rbf = S(H_o(δ)) is continuous only where the fill-time survival S is, so its continuity is owed to the toy's nudge check (§4.1), not claimed here.
- The protocol hints are named δ points on one curve (§4.1). Explicit ρ = 1 with the Realtime δ releases, with no loss (skip-rule §5).
- **Owed tests**, in the style of `test_continuum_one_law_across_the_dial`: H_o at each preset against its ±2 % nudges, monotone and with no step; an absolute H_o at the anchors for a fixed (g, n₀, d, ω); and rbf continuity across every preset in the toy (the behaviour check of §4.1).
- **The toy simulation** (skip-rule §6) replaces its H_o sweep with an ω sweep: ω ∈ {0, 1, W²/8, 10·W²/8}. It adds the gauges of §5, and a cell with inner TCP displacement scored by DupThresh.

## 9. What this blocks

- **Paper first.** Before any U code, paper §5.1 (two frontiers) and a new §7 subsection owe this law: B_s ≥ ω, its derivation, ω's provenance, the 2× bound, and §3.1's refutation of the hazard rule.
- **C9, C11 and C12's release arm** waits on ω (the owner) and on the toy's rbf readout at Bulk and Auto.
- **R1 (sender-only, no release)** is unaffected. It is the ω = ∞ point of this law, and it stays the staging point.

## 10. For the owner

**What changes, in plain words.** Today Bulk and Auto never deliver past a missing packet, and Realtime stops waiting after half a round trip (and, today, silently loses the packet). Under U nothing is lost by waiting less. The packet still comes, just after the ones behind it. So the only question is how much you mind packets arriving out of order. That is a number the engine has never had: the **price of one out-of-order delivery, ω**.
- **Realtime:** stops waiting after a few milliseconds or less (7 ms at 10 000 packets/s with the TCP-based price, at once with a unit price). It no longer loses the packet.
- **Auto and Bulk:** keep strict order in normal operation **only if ω is large**. With the TCP-based price (one halving of an inner TCP's window), Auto waits about 74 ms and Bulk about 0.7 s at 10 000 packets/s, longer than any normal repair. With a unit price, Auto would reorder almost every repaired packet, and Bulk would at high rates.
- **What Bulk can still see:** a hole older than its hold, for example a dead path at two paths lasting about 0.7 s or more. Its packets then arrive late and out of order instead of after a stall. Nothing is lost.

**The cost.** No new protocol field. One timer per stalled hole. A released-packet gauge that must read about zero at Bulk and Auto.

**Your decisions.**
1. **ω, the price of disorder.** One out-of-order delivery costs as much as how many resent packets? The options: (a) 1; (b) one TCP window halving, W²/8, computed from the measured rate and RTT; (c) measured on a TCP-in-tunnel test. The recommendation is (c), with (b) as the ceiling.
2. **Should ω follow the rate?** (b) grows with the link rate and keeps Bulk's hold long on fast links, but it shrinks on slow ones (Bulk reorders below about 100 packets/s). A fixed ω shortens the hold as the rate grows.
3. **Is reordering after a long outage at Bulk acceptable** (§4.3), in exchange for no stall? If not, ω must be very large there, which is the same as R1, U's no-release staging point.
4. **How to price delay:** per delayed packet (this document's extension, the stricter for Auto) or per stalled second (skip-rule §1, which lets ω = 1 keep Auto in order through one repair).
