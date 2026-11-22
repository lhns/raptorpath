# The give-up rule below ρ = 1: which holes, when, and where (derivation, no code)

**Question (owner).** Under ρ < 1 (Realtime declares ρ = 0.99) the machine may give up on a lost packet ("skip" it) so that in-order delivery does not stall. What is the correct give-up rule, and the one that costs the least latency and bandwidth? Re-derive T_cut, use a rule that is not time-based, or find the optimum by simulation?

**Short answer.** All three, each in its place:
- the count stays the guarantee (settled);
- a re-derived T_cut, in seconds and from measured repair times, decides *which* holes get the allowance when it is scarce;
- simulation ranks the candidates and bounds their gap to an offline optimum. It picks no constant.

Three findings change the design, whatever the simulation says:
- **(F1)** A sender-side skip as ADR-0084 writes it (shed = send nothing but the floor) is *slower* than simply resending, by H, for every isolated hole (§3.1). This holds per H value: at N ≥ 2, H ≈ the path skew and the penalty stands. At N = 1, `ordering-hold.md` §8 proposes H ≈ 0 (copy and floor on one path at one instant). There the latency penalty vanishes, and the last copy rests on saved allowance alone (≈ ε·ĥ spent instead of ≈ ĥ).
- **(F2)** Skipping part of a run of adjacent holes buys nothing and spends allowance (§3.3).
- **(F3)** Releasing order and losing a packet are two separate acts. Only stopping retries loses a packet. If a late packet delivered out of order counts as delivered (owner question **O3**, the deciding one), the receiver can release order on its own hold, uncounted and with no new wire field, while the sender's counted verdict stays the only loss (§4.1). If it does not count, the receiver has to spend counted allowance, and that needs one wire field (§4.3).

**H carries the headline numbers and is UNDERIVED** (rho-step-sites C11). F1's penalty is H, the receiver's saving is RTT − H, and the oracle passes at detect + H. Every time below is stated "if H = 5 ms" (SRTT/2 at a 10 ms RTT), and the simulation sweeps H (§6). `ordering-hold.md` §8 proposes H as a path quantity: ≈ 0 at N = 1 and ≈ the path skew at N ≥ 2. Rows below that depend on H are given at both values where they differ.

**Inputs read** (`github/main` `79d264aa`):
- ADR-0084 and its Amendment 1;
- `rho-step-sites.md` §1, §5 and §8;
- `evict-verdict-law.md` §1–§4 and §9;
- paper `docs/fec-arq-model.md` §3.3, §4.1, §4.10, §5.6, §7.3, §7.5 and §7.6;
- `raptorpath-math/src/lib.rs:1239–1270` (`p_recovered_within`, `find_t_cut`);
- `net/store_cap.rs:316` (`delta_price`) and `net/late.rs:64–101` (`W_COST_RATIO`, `request_bar`);
- `net/reorder.rs:60–92` (late fills), `net/delivery.rs` (the consumer hand-off) and `net/receiver.rs:1560–1590` (`abandoned_seqs`, `late_after_aban`);
- `net/mod.rs:236` (`GAP_ACK_MIN_INTERVAL` = 2 ms);
- ledger row C13.

**Words.**
- A *hole* is a source seq the receiver is missing while it holds later data, or a lost tail.
- A *run* is a maximal block of consecutive missing seqs with no received seq inside.
- *Resolution age* X is the time from a seq's original send to the moment it is filled, under the machine that never gives up.
- *Tell latency* L is the time from a give-up decision to the receiver passing the hole.
- *Release* means delivering later data past a hole (order given up). *Stop* means the sender stops retrying (a possible loss). Today's Realtime receiver does both at once: its expiry moves the ack, so the sender stops.
- *Allowance* is the unspent part of the count.

## 1. The objective

"Minimise the delay that losses add to in-order delivery, plus the repair traffic sent to cure them, both in the paper's one unit, subject to the count."

```text
   minimise   J  =  ( N_rep  +  (δ/d) · T_stall )  /  S_del          subject to   Ab(t) ≤ (1 − ρ)·S_src(t)  for all t
```

| symbol | meaning | provenance |
|---|---|---|
| N_rep | repair and resent symbols sent | measured (sender counters) |
| T_stall | frontier-blocked seconds: Σ over stall episodes of (frontier resume − frontier freeze) | measured (`[WIDLE]` onset and frontier writers, paper §7.3) |
| S_del | source symbols delivered | measured |
| δ | the latency price, `net::delta_price` (50 Realtime, 0.5 Auto, 0.005 Bulk) | declared dial (paper §4.1) |
| d | the ARQ resolution delay | measured (paper §7.3) |
| w = 1 | price of one wasted copy against `c_L` (`W_COST_RATIO`) | declared register row (`net/late.rs:64`) |
| Ab, S_src, ρ | the count of ADR-0084 decision 5 | measured, measured, declared |

**The weight needs no new owner input.** Paper §7.3 already prices one ARQ resolution delay d of a genuine loss at δ repair symbols ("π₁·P_arq·δ·(T + d)/d"), and `request_bar` reads the same `delta_price` against `w = 1`. So `δ/d` converts stall seconds into repair symbols. At Realtime one round trip of stall costs about 50 resends, so stall dominates. At Bulk the repair term dominates, but ρ = 1 there, so no allowance exists.

**A skip has no price inside the count.** The triangle has no price for a lost packet. ρ only bounds how many may be lost. So inside the allowance a skip is free, and the optimum spends the allowance wherever it saves the most priced stall. A loss price would be a new dial. That is owner choice O1 (§9).

**Readouts beside J.**
- Per-packet head-of-line delay p50 and p99. A stall of length s delays about `rate·s` packets by `s/2` on average, so this metric weighs long stalls quadratically. That strengthens §3's ranking rather than reversing it.
- The skipped fraction G_rx / S_src.

§7.5's overrun weight λ is not used: the paper records it as unmeasured.

## 2. The horizon of the promise

**What S_src is.** The sender counter `src_total` counts "source seqs sent" (paper §5.6; `net/shed.rs:43–48`). It is cumulative from connection start and never reset. The rule checks `Ab + 1 ≤ (1 − ρ)·S_src(t)` at every verdict, and the contract is read at the end of the run, `G_rx ≤ (1 − ρ)·src_total`. ADR-0084 decision 7 keeps the budget cumulative (evict-verdict-law question 10, owner-accepted). So the promise is a **prefix promise**: every prefix of the stream loses at most 1 − ρ of what it sent.

```text
   A(t)  =  ⌊ (1 − ρ) · S_src(t) ⌋  −  Ab(t)                 allowance available at t
```

**What that means in numbers (Realtime, ρ = 0.99, 10 000 packets/s):**
- **Start.** A = 0 until `S_src ≥ 1/(1 − ρ)` = 100 source symbols, about 10 ms. Every hole in that span is served until filled.
- **After clean runs, the bank grows without bound.** It grows by 100 units per second: after 10 s the allowance is 1 000 packets, and after an hour it is 360 000 packets, or 36 s of stream. One episode, though, is capped near B by the source pause (§3.3).

**Is banking acceptable? Derived, not picked.**
1. **The promise as declared allows it.** ρ is a per-packet delivery fraction, and the cumulative count is its estimator. A window of W symbols would promise something stronger, "at most (1 − ρ)·W in any W". That bounds the length of a loss run, which is a different contract and needs W as a new declared dial.
2. **Only one window comes from the triangle without a new dial: W = 1/(1 − ρ).** It is the smallest window that holds one whole unit. It admits one skip per 100 consecutive symbols, so no run of two adjacent holes can ever be skipped. Under bursts ρ < 1 would then buy nothing. So it is refuted, and every other W needs provenance the repository does not have.
3. **The bank is harmless if the spending rule does not read the bank's size.** §3's rule spends by a threshold set from *rates*: allowance growth against the measured loss rate. The bank level does not enter it. In steady state, if losses use less than 1 − ρ, the threshold is at its floor and the bank only absorbs variance. If they use more, the bank stays near zero. A bank spent in one episode is spent on runs that §3's ratio admits.
4. **First-come spending is the policy that turns the bank into a defect.** That is ADR-0084's per-seq rule at age > D: the bank goes to the first holes after a clean run, including part of a long run (F2), for no gain.

**In plain words (owner question O2).** After a long clean stretch, at least 20 s at 10 000 packets/s, one outage may skip about 2 000 consecutive packets (0.2 s, bounded by the source pause near B) and still keep the 1 % promise. Repeated outages can each do so while the bank lasts. This is what the cumulative promise allows, and it should be confirmed rather than assumed.

**Verdict: keep the cumulative horizon, with no window.** Gauge the largest allowance spent in one episode (ADR-0084 question 10's gauge). A run-length bound is owner choice O2.

## 3. Which holes get the allowance

### 3.1 The race a skip runs against (finding F1)

Take an isolated hole at N = 1. When the sender acts on the first gap report it can:
- (a) serve: resend it, and it is filled about one owd later;
- (b) shed as ADR-0084 decision 1 writes it: send the floor and no copy. The receiver passes the hole one owd later plus H;
- (c) shed with a last copy: send one copy and the floor together.

The copy and the floor travel the same path, and the floor then waits H at the receiver. So (b) resolves the hole H later than (a), for every isolated hole whose resend arrives. (b) also spends one unit of allowance per hole, and saves one resend. At H ≈ 0 (N = 1, `ordering-hold.md` §8) (b) is as fast as (a), and the case for (c) is the allowance alone: (b) spends ≈ ĥ, (c) ≈ ε·ĥ.

(c) resolves at `min(fill, owd + H)`. It never resolves later than (a), and it spends allowance only when the copy is lost: a filled verdict seq is delivered in order and leaves Ab's pending term without entering G_rx (ADR-0084 §4, "Ab converges").

"A verdict stops retrying; it does not withhold the last copy":

```text
   verdict(s)  ⇒  send one copy of s  and the floor F,   at the same instant
```

There is no branch on δ or ρ here: the copy goes with every verdict. Bandwidth is the same as serving once.

**A forced verdict has no copy.** A verdict forced by eviction, when the last servable copy leaves the sender, has no data to send. It sends the floor alone and always consumes one unit. With a store at every ρ, forced verdicts are 0 by data (ADR-0084 decision 2), and their gauge must read 0. If it does not, those verdicts cost F1's full penalty.

**G_rx is a level, not a counter.** It falls on a late fill. evict-verdict-law §4's no-double-count argument says "both fields are cumulative", and that holds for `next_expected` only. Nothing in ADR-0084's budget text assumes G_rx is non-decreasing, and the `[ACCT-RX]` lines print the frontier and, at the end, `holes_given_up` only (`receiver.rs:141`, `:2229`). The merge rule at the sender still matters once N ≥ 2 reorders acks:
- taking G_rx from the latest-*arriving* ack can pair a stale, lower G_rx with a newer A_rx, which under-counts Ab and overshoots;
- the safe rule takes the pair `(next_expected, G_rx)` from the ack with the greatest `next_expected`, and at a tie the larger G_rx. That is conservative, and it converges as the frontier moves.

The engine brief's step for `passed_undelivered` must state this rule.

**Finding (predicted, owed a ledger verdict): D is degenerate at Realtime N = 1.** The first report for a hole arrives at age ≈ RTprop + reorder + sampler interval, which is more than `D = RTprop/2`. So at N = 1 Realtime the age test is predicted true at every report, and D decides nothing.
- Under MEASUREMENT DISCIPLINE 17/18 a term pinned over its operating range is a defect finding, not a footnote. Its D-bind gauge (§3.4) must be read in the guard's ρ < 1 stage, and the ledger row for the shed law gets the verdict.
- D does bind at N ≥ 2, where a fast path reports a slow-path seq early. That is its real job, a reorder guard, so the expected verdict is "degenerate at N = 1, live at N ≥ 2". It needs a reading.

**Side effects of the last copy.** Most verdict seqs now end delivered, not passed.
- **C7's feed** (`newly = … \ V`, rho-step-sites C7) would then under-credit every filled verdict. The correction term must be the passed set, not V: subtract `V ∩ passed`, known from the floor's ack and `G_rx`, or drop a seq from the feed's exclusion when it is filled.
- **Witnesses.** The rho-step-sites §3 acceptance test (`shed ≥ 1`, `G_floor ≥ 1` at 0.99) still holds where its `dead` seam kills every copy of k. Any test that assumes "verdict ⇒ passed" without such a seam must assert on `G_rx`, not on `ev=shed`.

### 3.2 Isolated holes: rank by expected remaining resolution time

For an isolated hole at verdict age a, with last copy:

```text
   saving        s(a)  =  E[ (X − a − L)⁺  |  X > a ]                          stall seconds saved
   consumption   c(a)  =  P( X > a + L  |  X > a )                              allowance units spent
   s(a) / c(a)   =  E[ X − (a + L)  |  X > a + L ]   =   m(a + L)               saving per unit actually spent
```

m(t) is the mean residual resolution time at age t.

**Theorem sketch (isolated holes; assumptions at the end of §3.3).** Take independent holes with separable savings and the prefix count. The policy that minimises J subject to the count gives a verdict iff m(a + L) ≥ θ, where θ ≥ 0 is the count's shadow price in stall seconds. Two parts:
1. **Offline.** With the X_s known, the feasible skip sets under a prefix count (a nested chain of capacities) form a laminar matroid. Greedy by saving is therefore exact: skip the holes with the largest `(X_s − a_s − L)⁺` that the prefix capacity admits.
2. **Online.** It is the Lagrangian stopping rule.

So the owner's hypothesis holds in a precise form. **Spend the allowance on the holes with the longest expected remaining resolution time, measured past the tell latency, per unit actually spent.**

**When does age stand in for that?** When m is non-decreasing in age beyond D (increasing mean residual life). Then a hole that has waited long is likely in a blackout or a burst of correlated loss, and `m(a + L) ≥ θ ⇔ a ≥ T_ρ`, an age threshold. When m is decreasing (repairs bunched at one round trip), the optimum is "give up at the first chance or never". With identical holes the count gate's first-come order is then an optimal tie-break. That is ADR-0084's rule, plus the last copy. Both cases are one rule: the threshold below falls to D in the second case, by arithmetic.

### 3.3 Runs: rank by saving per unit (finding F2)

Stall is not additive. The frontier waits for the **latest** resolution in a run, so passing a strict prefix of a run delivers nothing: there is no data between missing seqs. Savings are complementary, the single-hole exchange argument fails, and "longest remaining first, per seq" is refuted for runs. The correct item is the run:

```text
   item R = (n_R, X_R = max_{s∈R} X_s);      saving per unit  =  m_R(a + L) / n_R;      spend on R  ⇔  n_R ≤ A(t)  ∧  m_R(a + L) / n_R ≥ θ
```

- Offline this is a knapsack under the prefix count. Greedy by ratio is within one item of the optimum, and the simulation's oracle (§6) reports both greedy and the LP bound.
- **Run-atomic spending is certain, not a tuning:** "spend n_R units only if all n_R are affordable".
- For a 2 000-packet blackout run with 1 000 units banked, first-come per seq spends the whole bank and the frontier still waits on the upper half. Run-atomic spends nothing.

**A limit of the sender: n_R is defined there only for a closed run.** A closed run is one bounded above by a received seq, as a gap report shows it.

An *open* run has nothing received above it. It is a lost tail, or a blackout at N = 1, and the sender cannot tell the two apart until feedback returns. The tail sweep can only judge the open run as it stands, `[A_rx, hi]`. It does that at every fire while the run grows, and each fire's run fits the allowance that is left. So in a blackout the sender drains the bank in pieces, exactly as first-come does, and no definition of n_R for an open run both keeps the bank in a blackout and passes a lost tail. Run-atomic spending therefore helps at the sender only for closed runs: every run at N ≥ 2 while a live path reports, and every burst that ends before the blackout does. The receiver sees a blackout run closed, with its exact extent, as soon as data resumes. This is one reason for §4.

**The source pause caps one episode.** The source-rate bound (ADR-0084 decision 3) pauses at `O ≥ B`, and B is pinned at N·2048 (ledger F01). A run is part of O, so one episode can spend at most about B units: 2 048 at N = 1, about 0.2 s of stream in the example. The bank can still be drained by consecutive episodes, but no single run is longer than B.

**The theorem is a sketch.** It assumes:
- stationary losses with independent resolution ages;
- the Lagrangian form, which treats the count as a long-run rate. The hard prefix count is that rate plus the bank as slack;
- for runs, greedy by ratio under nested capacities. Its "within one item" bound is not proven, which is why the oracle reports the LP bound.

The exact statements are the offline unit-weight case (the laminar matroid) and run-atomic certainty.

### 3.4 T_cut re-derived, in seconds and from measurements

Under increasing mean residual life, θ is set by rate balance: the long-run consumption equals the allowance's growth.

```text
   ĥ · Ŝ(T_ρ + L)  =  (1 − ρ)       ⇒       T_ρ  =  max( D(δ) ,  Ŝ⁻¹( (1 − ρ)/ĥ )  −  L )          verdict needs  age ≥ T_ρ
   Ŝ⁻¹(p)  =  inf{ t : Ŝ(t) ≤ p },     = 0 for p ≥ 1,     ≤ t_max (the largest resolution age observed)
```

| symbol | provenance |
|---|---|
| Ŝ | measured: the survival of the resolution age X over reported holes, Kaplan–Meier (cited: Kaplan & Meier 1958) with verdicts as right-censoring at their pass age. Linear interpolation between sample points makes Ŝ⁻¹ continuous where Ŝ decreases |
| ĥ | measured: reported holes per source symbol |
| L | measured: owd of the floor's path + H (H = `min(b(δ_rx), 2)·SRTT_rx`, rho-step-sites C11) |
| D(δ) | derived, paper §5.6 |
| ρ | declared |

**Its relation to today's T_cut.** `find_t_cut` (`raptorpath-math/src/lib.rs:1254`) solves `ε(1 − P_fec)·(1 − P_arq(t)) = 1 − ρ`. That is the same equation with the model in place of the measurement: `ĥ = ε(1 − P_fec)` and `S(t) = 1 − P_arq(t)`. It is unusable as it stands, for four reasons:
- **Its unit is taper offsets in symbols, not seconds.** t indexes `(1 − q)^t` against `window_size`.
- **It models ARQ as the taper's correction count.** It has no round trips.
- **Its search is capped at 10·W.** Ledger C13: the cap or the wasm 2·RTT floor binds at all ten measured ρ < 1 points, so it is PINNED.
- **It branches `target_rho >= 1.0 → ∞`.** That is recorded debt, and is not to be copied.

The re-derived form is in seconds and needs no model parameter. Its only upper bound is the data's t_max, and that bound gets a bind gauge.

**ρ = 1 comes from the count, not from T_ρ.** At ρ = 1, `Ŝ⁻¹(0) = t_max` is finite, while A ≡ 0 admits no verdict. As ρ → 1⁻, A → 0 and T_ρ → t_max together. Both shrink the skip set continuously.

**Clamps and gauges (FORMULA-FIRST):**
- `max(D, ·)`: the D-bind fraction;
- `Ŝ⁻¹ ≤ t_max`: the t_max-bind fraction;
- `n_R ≤ A`: refusals (`shed_denied`).

**Self-censoring: Ŝ is estimated online, under the policy's own give-ups.** Once verdicts stop retries at age T, a passed hole's true resolution age is never observed. S3 must therefore estimate Ŝ from its own run, not from S0's never-give-up distribution:
- **Kaplan–Meier** treats a passed hole as right-censored at its pass age, and a filled hole as an event at its fill age. That is consistent on `[0, T + L]`, because censoring there depends only on age, which is known (non-informative given age).
- **The last copy keeps the operating point observed.** A verdict's copy that arrives is a fill at an age up to T + L. So `Ŝ(T + L)`, the one value the rate balance reads at its fixed point, is measured, not extrapolated.
- **Beyond T + L the tail is seen only through refusals.** When the count binds, refused holes are served until filled, and their fills are events past T. That is exactly the case in which T must move up. When the count is slack the tail beyond T + L is unseen, but then `T_ρ = D` and the tail is not read.
- **Residual risk.** A regime change to longer outages while the count is slack is seen only after the first refusal. The simulation reports Ŝ against S0's true survival as a diagnostic, and the t_max-bind gauge flags it.

**Prediction.** With the last copy, consumption under i.i.d. loss is ε·ĥ, which is 0.0125 % at ε = 2.5 % and ĥ = 0.5 %. That is far below 1 − ρ = 1 %, so T_ρ = D at almost every i.i.d. point and the D clamp binds. T_ρ moves only under correlated loss (GE bad state, blackouts), where copies fail together. If the simulation shows no p99 gain there, T_ρ is dropped (pre-registered, §6).

**Is T_ρ continuous when Ŝ is flat?** No hole resolves before about one round trip, so Ŝ = 1 on [0, t_min). As (1 − ρ)/ĥ crosses 1, T_ρ steps from D to `t_min − L`. Verdicts are taken only at report ages, though, and none falls in that interval at N = 1. So the realised skip set does not step, and the aggregate behaviour is continuous. The simulation's continuity readout checks this (§6).

## 4. Where the decision is taken, and why O3 decides it

**What the consumer receives today for a late fill (read, not assumed).**
- `ReorderBuffer::push_with_time` returns a seq below `next_deliver_seq` at once: "Holding a late fill buys no ordering … Deliver immediately" (`reorder.rs:75–82`).
- The receiver counts it as `late_after_aban` ("the EVICT seat's repair waste", `receiver.rs:1570–1583`) and hands it to `WindowDelivery::offer` like any other symbol (`delivery.rs:45–80`). So the consumer, the TUN inject channel, receives the IP packet **out of order, after its successors**.
- It does reach the consumer today, but only when a copy was already in flight. Today's expiry also advances the ack, so the sender prunes the seq on the cumulative ack and sends nothing more. That coupling is why today's timer is an uncounted loss.

**Release is not loss.** Two acts hide in one "skip":
- **release:** deliver later data past the hole. It costs only order;
- **stop:** stop retrying the hole. That is the only act that can lose it.

Whether release alone needs the count depends on O3: **does a packet delivered after its successors count as delivered for ρ?**
- **O3 = yes.** Release never loses a packet, so it needs no count. Only *stop* spends allowance, and the sender already counts that (ADR-0084).
- **O3 = no.** A released hole is worth nothing when it arrives late, so release is a loss and must be counted wherever it happens.

### 4.1 Branch O3 = yes: the receiver releases on its hold, the sender alone stops (design U)

"The receiver releases order on its own hold and keeps acknowledging the hole as missing. Only the sender's counted verdict, with its last copy and floor, lets the ack pass it."

```text
   receiver delivery frontier:   pass hole s for delivery  at  t_detect(s) + H_o                     (uncounted; no allowance read)
   receiver ack frontier:        A_rx  =  min{ s : s ∉ received  ∧  s ≥ F_rx }                       (exactly R1's next_expected)
   late fill of a released s:    delivered at once (reorder.rs:75–82), removed from the released set
   sender:                       verdict(s) ⇔ age > T_ρ ∧ n_R ≤ A(t);  verdict ⇒ last copy + floor     (ADR-0084 + §3)
```

- **The promise holds by construction.** The ack never covers a released hole, so the sender keeps it in U and serves it until it fills or gets a counted verdict. G_rx keeps ADR-0084's meaning: holes below A_rx never received, minus late fills.
- **No new wire field, and no new ack meaning.** `A_rx` is exactly R1's `next_expected`, because under R1 the receiver passes only below floors. v11 as approved carries U unchanged.
- **Memory.** Released holes are a seq set, the existing `abandoned_seqs` (`receiver.rs:1581`). Received data above a stuck ack is bounded as at ρ = 1: the source pauses at `O ≥ B`, and SACK release (C2) keeps received-above seqs out of O.
- **Latency.** The consumer's stall ends at `detect + H_o`, with `H_o = ω·d/δ` (`ordering-hold.md`), whatever the disorder price ω gives: 0.2–22 ms at Realtime over ω = 1–100 in its example. It is not today's timer by construction. Every hole keeps being served until its fill or its verdict.
- **Allowance used:** only holes whose last copy also fails, about ε·ĥ (§3.4).
- **Bandwidth:** as serving, one copy per round until fill or verdict.
- **What U still needs: an ordering law H_o(δ), derived in `ordering-hold.md`** (rent-or-buy against a new disorder price ω, the owner's call; the "never" below is restated there as "release-before-fill ≈ 0 at Bulk and Auto in the operating range, shown by a gauge", since an H_o infinite on a stretch of δ is a mode switch). Original text: Release is ordering, a δ question that does not involve ρ. Today's arms are SRTT/2 below ρ = 1 and never at ρ = 1. One δ-continuous H_o must reproduce "never" (or a hold long enough that Bulk TCP-in-tunnel sees no reorder) at Bulk and Auto, and a short hold at Realtime, with no threshold on δ. `min(b(δ), 2)·SRTT` (C11) is **not** enough: it gives 2·SRTT at Bulk, which changes the ρ = 1 Bulk default.
  - Until H_o is derived, U cannot ship.
  - The derivation is the receiver-side analogue of paper §7.6's request law: a level set of the δ-priced stall against the price of disorder. The price of disorder is part of O3.
- **Changes against rho-step-sites:** C9 and C11 keep one release arm, keyed by H_o(δ) and never by ρ, instead of deleting the local give-up. C12's marker term is unchanged.

### 4.2 Sender-only, no release (R1, the staging point under both branches)

(2026-11-21: U chosen. R1 stays the staging point only; it is not accepted as the final receiver law.)

The sender judges (with the last copy), and the receiver releases and passes only below the floor after H.
- Latency: fill time ≈ RTT + owd for an isolated hole, and + H when the last copy is lost.
- The promise is exact, with one spender. Wire: v11 as approved.
- R1 is U with H_o = ∞, so it is the staging point when O3 = yes.

### 4.3 Branch O3 = no: the receiver spends counted allowance (design RX)

**Not pursued (owner, 2026-11-21: O3 = yes, design U).** Kept as the record of the alternative.

"The receiver is the one spender; the sender tells it the cumulative allowance it may spend."

```text
   sender → receiver:   Â  =  ⌊ (1 − ρ) · S_src ⌋                     cumulative, non-decreasing
   receiver passes run R  ⇔  [ t − t_detect(R) ≥ max(H, T_rx)   ∨   R ⊂ [A_rx, F_rx) ]   ∧   G_rx + n_R  ≤  Â_heard
   T_rx  =  Ŝ_rx⁻¹( ĝ / ĥ_rx )                                         ĝ = growth rate of Â heard,  ĥ_rx = holes per second
```

- **One ledger, no overshoot.**
  - Every pass is checked at the receiver against the newest Â it heard. Â only grows, so stale is safe: `G_rx ≤ Â_heard ≤ (1 − ρ)·S_src` holds at every instant.
  - Sender verdicts become proposals (copy + floor). The sender keeps a proposed seq until A_rx passes it, and resumes serving if the floor is not acknowledged within RTT + H. That amends ADR-0084 decision 2.
- **The receiver reads no ρ**, only a count and its growth. Ŝ_rx is measured from detection to fill. The receiver sees this directly (paper §7.6; `LateGauge`).
- **Under O3 = no, every pass consumes a unit** (≈ ĥ = 0.5 % in the example, inside 1 %). No late-fill refund applies, because a late packet does not count.
- **Latency:** detect + H, the same as U.
- **The lost tail** still needs the sender's proposal (rho-step-sites §1.1).
- **Wire: one field, Â,** on `WindowFloor` and once per SRTT while it grew (staleness ≤ `(1 − ρ)·rate·SRTT` units).
- **Floor cadence:** sweep-only suffices again (evict-verdict-law question 6's premise returns).
- **C10:** a full channel could drop within Â (O4); the default is to hold.

### 4.4 Ranking

| design | consumer stall, isolated hole (if H = H_o = 5 ms) | loss | allowance used | wire beyond v11 | owed before it can ship |
|---|---|---|---|---|---|
| U (O3 = yes) | detect + H_o(ω): 5.2–27 ms at Realtime over ω = 1–100 (`ordering-hold.md` §4) | only failed last copies | ≈ ε·ĥ | none | H_o(δ) derivation |
| RX (O3 = no) | detect + H: 10 ms at H = 5; ≈ 5 ms at H ≈ 0 (N = 1) | every pass | ≈ ĥ | Â | sim (P5), ADR on the wire |
| R1 + last copy | fill ≈ 16 ms; + H if the copy fails | only failed last copies | ≈ ε·ĥ | none | nothing (staging) |
| ADR-0084 as written | ≈ 21 ms | every verdict | ≈ ĥ | none | (dominated: F1) |

- **At H ≈ 0 (N = 1, `ordering-hold.md` §8)** "ADR-0084 as written" falls to ≈ 16 ms, equal to R1 + last copy. F1 then separates them by allowance only (≈ ĥ against ≈ ε·ĥ), not by latency.
- **Under O3 = yes, U dominates RX** in loss and wire; its latency is set by ω, not by H. It has the same release latency, loses fewer packets (retries continue until the sender's verdict) and needs no wire field.
- **Under O3 = no, U is not available.** A release is a loss, so RX is the fast design, and R1 is the slow, field-free one.
- **Wire recommendation: keep v11 exactly as approved.** Decide Â only after O3 and the simulation. U never needs it.

## 5. Continuity (NO-MODE-SWITCH)

Every formula above is one expression, with every term always computed:

| formula | in ρ | in δ | at ρ = 1 |
|---|---|---|---|
| `A = ⌊(1 − ρ)S_src⌋ − Ab` | linear (integer part on measured S_src, not at a preset) | none | 0: no verdict, no pass |
| copy with every verdict | none | none | no verdict exists |
| run-atomic `n_R ≤ A` | through A | none | A = 0 |
| `T_ρ = max(D, Ŝ⁻¹((1 − ρ)/ĥ) − L)` | continuous where Ŝ decreases; a step only across ages with no decision event (§3.4) | through D(δ) and H | finite (t_max); inert because A = 0 |
| U's release at `detect + H_o(δ)` | **none**: release reads no ρ | H_o(δ), owed, must be continuous | releases at Realtime δ even at ρ = 1, with no loss: the ack never passes |
| RX's rule | through Â only | through H | Â = 0: no pass |

**U at ρ = 1 is a behaviour change at the Realtime δ only.** At an explicit ρ = 1 with the Realtime hint, U releases order while losing nothing. That is ρ composing with δ, as CLAUDE.md requires. At the Bulk and Auto presets H_o must reproduce today's no-release, and that is H_o's derivation obligation, not a branch.

**Nudges.**
- The ±2 % nudge of ρ = 0.99 is taken on 1 − ρ: 0.9898 and 0.9902.
- At ρ = 1 the nudge is one-sided. ρ = 1 − ε with ε·S_src < 1 must be byte-identical (rho-step-sites §3 clause 5), and the readouts must converge monotonically as ε → 0.
- δ is nudged ±2 % around Realtime.

## 6. Pre-registered simulation plan (toy model, v2)

**Superseded for stage 2 by §7 (toy plan v3).** This section is kept intact as the stage 1 registration that `tools/toy/results-stage1.md` reports against.

**Status.** A Python discrete-event **toy model**, built later by a separate worker. It ranks candidates and bounds their gap to two offline optima:
- an LP lower bound for the rules that pass holes against the count;
- an exact optimum of U's own cost for the release rule.

It fixes no constant, and no number in it is provenance for the engine. **RX (§4.3) is not simulated**: the owner chose U (§9, owner answers).

### 6.1 Model

- **Paths.** Each path has a base one-way delay plus per-packet jitter, and a bottleneck rate above the source rate, so copy arrival times do not depend on the policy up to a pass. Cells that saturate the link are labelled "bound approximate".
  - Without jitter, fills bunch at exact round times, and every hold-dependent readout is a staircase in δ (`ordering-hold.md` §9).
  - **Jitter.** Measured where a VM battery supplies it, as the per-path owd distribution from `[LAT]`/`rttdump`. Otherwise swept i.i.d. per packet over {0, 1, 4 ms}; 4 ms is `sim_lost_tail`'s value.
  - Each jitter value runs with in-path FIFO kept and with in-path reorder allowed.
- **Source:** paced at 10 000 packets/s (512 B).
- **Loss:** a per-copy loss process per path. No FEC: the residual loss is set directly.
- **Sender:** a store, a 2 ms gap-report sampler, the RFC 9002 hole law at N = 2, and a tail sweep at `clamp(2·SRTT)`.
- **Placement (no placement rule in the toy):** at N = 2 each source seq goes to the slow path with probability φ_slow, swept over {0, 0.25, 0.5}. Copies go on the fast path.
- **Receiver:** a reorder buffer, the floor pass and, for U, two frontiers.
- **Common random numbers.** Every candidate in a seed sees the same loss, jitter and placement draws, keyed by (seed, seq, copy index, path).

**How the toy computes d.** The paper calls d the ARQ resolution delay (§7.3). The engine's sample is `note_source_resolution` (`receiver.rs:1446–1457`): the detect-to-close time of every hole closed by its own source symbol, **original or retransmit** (`HoleOutcome::Original | Retransmit`), averaged (`net/late.rs:290`). The toy computes both, as running means over the run:
- **d_all**, the engine's definition (self-heals included);
- **d_rep**, holes closed by a copy only.

U is run with each.

**Finding for `ordering-hold.md`: d shrinks where reordering is highest.** At N = 2, d_all counts skew self-heals. Paper §7.3's c7 row (d = 0.78 ms against π₀ = 0.96, so 96 % of holes heal by their own original) is that case. `H_o = ω·d/δ` then shortens exactly where skew reordering is most frequent, and releases more slow-path packets out of order. Paper §7.3 does not say which class d contains. The ordering-hold law must state whether its d is d_rep (the repair round, the quantity rent-or-buy waits for) or d_all. The toy reads both: rbf and the skew out-of-order rate under each, per φ_slow.

**Out-of-order classes.** Every late delivery is classed by what filled it:
- **loss:** filled by a copy;
- **skew:** N ≥ 2, filled by its own original via a slower path;
- **jitter:** filled by its own original on the same path after a later seq overtook it. In-path reorder only; this is the N = 1 class.

### 6.2 Candidates

**Sender rules, scored against the LP:**
- **R0 (reference, not a bar):** today's receiver timer: pass at detect + SRTT/2, the ack advances, uncounted.
- **S0:** serve until filled.
- **S1:** ADR-0084 as written: `age > D`, per-seq first-come, no copy.
- **S1c:** S1 + last copy (ADR-0084 Amendment 2).
- **S2:** S1c + run-atomic.
- **S3:** S2 + the online T_ρ (Kaplan–Meier under its own verdicts, §3.4), applied to a run's oldest seq.

**The release rule, scored against U\*:**
- **U:** the R1 sender (S1c, or S2 if run-atomic survives P5), plus the receiver release at `min(t_detect + H_o, t_F + H)` with the ack held (`ordering-hold.md` §8).
  - `H_o = ω·d/δ`, with ω ∈ {0, 0.3, 1, 3, 10, 100} and d ∈ {d_all, d_rep}.
  - **The rent is a pluggable function** `rent(t)`, with release at the first t where `rent(t − t_detect) ≥ ω`. Two arms are registered now:
    - `rent(t) = (δ/d)·t`, ordering-hold §3.2;
    - `rent(t) = (δ/d)·t + c_H(t)`, `c_H(t) = (t·g − B)⁺·MSS/S`, the candidate of disorder-price-registration §2. B is the sender backlog and g the delivery rate, both toy state, with MSS/S = 1.2 as registered there.

    A rent measured later by `meas-disorder-price` plugs in without a plan change.
- **P3's positive control, HZ:** ordering-hold §3.1's global-argmin hazard rule, with the true fill-time survival of the cell (no estimation). ω is chosen so that its jump `δ* = ω·d·(1 − ε)/τ₁` sits at the Auto preset. It exists only to show that P3 can fail.

**H is swept, because it is UNDERIVED:** H ∈ {0, 2.5, 5, 10 ms, 0.5·SRTT, 1·SRTT, **H§8**}. H§8 is ordering-hold §8's proposal, `max over paths still carrying a copy or the original (owd_p − owd_floor path)⁺ + in-path reorder spread`: ≈ 0 at N = 1 and ≈ the skew at N = 2, with the spread read as the observed maximum. Every headline is reported per H.

### 6.3 Offline optima

**(a) The LP lower bound for passing rules (S-candidates).** This is unchanged from v1. On each sample path, take arrivals a_j and the fill times X_s under S0 with last copies, and pass instants `p_s = t_detect(s) + H`:

```text
   minimise    Σ_{j ∈ P} ( y_j − a_j )
   subject to  y_j ≥ a_j  (received j),   y_j ≥ y_{j−1},   y_s ≥ X_s − (X_s − p_s)⁺ · x_s  (holes),
               Σ_{s : p_s ≤ t_k} c_s · x_s ≤ ⌊(1 − ρ)·S_src(t_k)⌋  (prefix count),   0 ≤ x_s ≤ 1,   c_s = 1{X_s > p_s}
```

- Solve with HiGHS; on 3 seeds per cell also solve the MILP for the integrality gap.
- `gap = (HOL_cand − HOL_LP)/(HOL_S0 − HOL_LP)`.
- **It bounds only rules that pass a hole in order against the count.** U spends no count and delivers out of order, so U can score below this "bound". U is never scored against it.

**(b) U\*, the exact offline optimum of U's own cost.** The cost is the release-dependent part of J_U (ordering-hold §2). The late packet's own delay and N_rep do not depend on the release, so they are excluded. On each sample path, every hole s has a detect time t_s and a fill time R_s (∞ if never filled before its floor pass), all known.
- Offline, a partial hold is dominated: holding and then releasing before the fill pays rent *and* ω. So each hole is either released at detect (z_s = 0) or held to its fill or floor pass (z_s = 1).
- The frontier is blocked exactly on the union of the held intervals `[t_s, R_s ∧ (t_F + H))`.

Linear rent:

```text
   minimise    (δ/d) · Σ_k ℓ_k · u_k   +   ω · Σ_s (1 − z_s) · 1{R_s < ∞}
   subject to  u_k ≥ z_s     for every elementary segment k ⊂ [t_s, R_s)      (segments between all interval endpoints, lengths ℓ_k)
               0 ≤ z_s, u_k ≤ 1
```

- Every constraint is a difference `u_k − z_s ≥ 0`, so the constraint matrix is totally unimodular and the LP optimum is integral: **U\* is exact, not a bound.**
- With a pluggable non-linear rent (`c_H` charged per stall episode, an episode being a connected component of the held union), solve instead by exact dynamic programming:
  - first split into independent clusters, the connected components of the union of *all* hole intervals;
  - within a cluster, scan holes by t_s, with state (episode start, episode end) over interval endpoints;
  - that is O(n³) per cluster, and clusters in I1 hold a few holes.
- **Score:** `ratio_U = cost_U / cost_U*` per seed and cell.
  - **The 2× claim is tested where it is proven:** on isolated holes (clusters of size 1), `ratio_U ≤ 2 + ω·N_{R=∞}/cost_U*`.
  - On overlapping clusters (42–67 % of I1's holes) the ratio is reported, and the 2× claim is marked "sketch, measured".

### 6.4 Cells, dial points and the run budget

**Base cell.**

| factor | base |
|---|---|
| loss | I1 |
| topology | N = 1, RTT 10 ms |
| jitter | 1 ms, FIFO kept |
| H | H§8 |
| δ | Realtime |
| ρ | 0.99 |
| U | ω = 1, d_all, linear rent |
| seeds | 20 |
| duration | 20 s (allowance after 20 s = 2 000; 10 s windows for the optima) |

B60 runs 70 s.

**One-factor sweeps from the base.** Each row varies one factor; the extra configurations exclude the base.

| factor | values | extra |
|---|---|---|
| loss cell | I2, G, B10, B60, T (as v1: i.i.d. 3 %/10 %; Gilbert–Elliott, burst 8; 200 ms blackout after 10 s and 60 s; last 346 seqs dead) | 5 |
| topology | N = 1 RTT 50; N = 2 (owd 5 and 25 ms) × φ_slow ∈ {0, 0.25, 0.5} | 4 |
| jitter × reorder | {0, 1, 4 ms} × {FIFO, reorder allowed} | 5 |
| H | the 7 arms, plus `floor-hold.md`'s H_ℓ(W) at W ∈ {1, 10, 60 s} (readouts: `fh_miss` rate and the floor-pass stall) | 9 |
| δ | Bulk, Auto, Realtime, each × {0.98, 1, 1.02} | 8 |
| ρ | 1, 1 − ε (ε·S_src < 1), 0.999, 0.9898, 0.9902 | 5 |

**Crossed for U only**, because its questions sit at Auto and Bulk: ω (6) × δ points (9) × {N = 1; N = 2 at φ_slow = 0.25} (2) × d ∈ {d_all, d_rep} (2) = 216 configurations. Two more, the c_H rent arm at Bulk and Auto, bring it to 218. Each U configuration also runs the U\* oracle.

**HZ:** Auto × {0.98, 1, 1.02}, N = 1 and N = 2 at φ_slow = 0.25: 6 configurations.

**Run count.** One run is one candidate × configuration × seed. Seeds run one at a time, because memory is low.

| block | configurations | × seeds | runs |
|---|---|---|---|
| R0, S0, S1, S1c, S2, S3: (1 + 36) each | 222 | 20 | 4 440 |
| U: 37 one-factor + 218 crossed | 255 | 20 | 5 100 |
| HZ | 6 | 20 | 120 |
| **total simulation runs** | | | **9 660** |
| LP solves (S0 traces of the 37 one-factor configurations) | 37 | 20 | 740, plus 3 × 37 MILP |
| U\* solves (TU LP or DP, per U run) | | | 5 100, each ≪ 1 s |

**Estimated wall time, sequential.** A 20 s cell is about 2·10⁵ packets and on the order of 10⁶ events, about 5–10 s in pure-Python `heapq`.
- Simulation: 9 660 × ~8 s ≈ 21.5 h.
- LP: 740 × ~5 s ≈ 1 h, MILP ≈ 1 h.
- U\*: under 1 h.
- **Total ≈ 25 h** (the three H_ℓ(W) arms add about 1 h).

**Pre-registered fallback.** First time the base cell. If one run takes more than 15 s, the one-factor sweeps drop to 10 seeds; the crossed U block, the presets and P3 keep 20.

### 6.5 Readouts, witnesses first

- verdicts, passes, releases and refusals by decider;
- G_rx / S_src against 1 − ρ at every instant;
- consumer HOL delay p50 and p99, frontier-stall p50 and p99, and late and lost counts;
- the out-of-order count per class (loss, skew, jitter) per delivered packet;
- repair sends and control datagrams per delivered packet;
- the bind fractions (D, t_max, `n_R ≤ A`, and which of H_o and t_F + H resolved each release);
- the largest single-episode spend;
- for S3: Ŝ against S0's true survival;
- for U: rbf per preset; d_all, d_rep and their ratio; φ_slow and the skew distribution; `gap` (S-rules) or `ratio_U` (U), per cell.

### 6.6 Pass criteria, fixed now (decision bars, not law constants)

- **P1, the promise (hard).** For S1, S1c, S2, S3 and U, in every cell, seed and instant, `G_rx(t) ≤ ⌊(1 − ρ)S_src(t)⌋`. For U, also: no released hole is ever pruned at the sender without a verdict.
- **P2, the ρ = 1 identity.** At ρ = 1 and 1 − ε every candidate's sender trace equals S0's. U's receiver may release, with zero loss.
- **P3, continuity, on paired differences.** For each preset p, readout m and seed k (common random numbers), take the midpoint deviation:

  ```text
     Δ_k  =  m_k(p)  −  ½·( m_k(0.98·p) + m_k(1.02·p) )        slope_k  =  m_k(1.02·p) − m_k(0.98·p)
     pass  ⇔  the 95 % paired-bootstrap CI of mean(Δ) contains 0   ∨   |mean(Δ)| ≤ ¼·|mean(slope)|
  ```

  - A smooth readout has a midpoint deviation that is second order in the nudge. A step located at the preset has `|Δ| = ½·|slope|`. The ¼ separates them.
  - For ρ the nudge is taken on 1 − ρ.
  - **Positive control:** HZ must **fail** P3 at Auto on rbf and on frontier-stall p50. If HZ passes, P3 has no power, and no P3 pass of any candidate is reported as evidence.
- **P4, implementation checks of the toy.** These are not evidence for the derivation; the toy is debugged before any ranking is read.
  - **a.** I1, N = 1, H = 5 ms: S1's isolated-hole stall exceeds S0's by H ± 1 ms.
  - **b.** I1: S1c's p50 equals S0's within 1 ms; S1c consumes ≤ 1.5·ε·ĥ.
  - **c.** I1: U's consumer-stall per isolated hole equals `min(H_o, R − t_detect)` within 0.5 ms.
  - **d.** B10 at N = 2: S1 spends > 0 for a gain within the CI of 0, and S2 spends 0. B10 at N = 1: S2 drains like S1.
  - **e.** U\*'s LP returns an integral z on every solve (the TU check).
- **P5, decisions:**
  - **Run-atomic** (S2 against S1c) is kept only if it improves HOL p99, or the largest-episode waste, by more than the CI in some cell and worsens none by more than the CI. Otherwise it is dropped.
  - **T_ρ** (S3 against S2, or against S1c) is kept only if p99 falls by more than the CI in G or B60 and no cell worsens by more than the CI.
  - **U** is reported, not adopted, here. Adoption waits on ω (`meas-disorder-price`) per ordering-hold §10. The toy reports:
    - `ratio_U` against the 2× claim (proven on isolated holes, measured on clusters);
    - rbf at Bulk and Auto, which must lie within the CI of 0 in I1, I2 and G at N = 1 (ordering-hold §6), per ω and per d definition;
    - the skew out-of-order rate at N = 2 per φ_slow and d definition;
    - whether d_all against d_rep changes Auto's rbf by more than the CI (the d finding above).

## 7. Toy plan v3 (stage 2)

**Status.** Pre-registration for stage 2 of the toy simulation, written for review before anything is built. No code and no runs in this section. It supersedes §6 for stage 2; §6 stays as the stage 1 registration, and `tools/toy/results-stage1.md` (stage 1, "the stage 1 results") is the evidence every change below cites. The toy fixes no constant. Every number in this section is either a provenance-carrying input or a decision bar for the toy; **no toy choice becomes engine provenance.**

**Review outcome (2026-11-22).** The one-path parts may be built after the edits of this revision: the engine's EWMA form of D (§7.2, §7.5), P3's residuals and planted controls (§7.9), and a new first decision part on the lost tail (Part T, §7.13), from the owner's answer of 2026-11-22, "fix the lost tail first". **The two-path parts (C) and the floor and W parts (D) wait on the hold re-derivation** (branch `tail-fix-derivation`, `docs/derivations/tail-and-hold.md`, in progress). They are not built until it lands, and they may be re-registered then.

**Settled before stage 2, not revisited.** Run-atomic spending (S2) and the time-based give-up deadline T_ρ (S3) are dropped (stage 1, P5). The sender rule is ADR-0084's verdict, `age > D ∧ Ab + 1 ≤ (1 − ρ)·S_src`, plus one last copy with the floor (Amendment 2). Stage 1 called this rule S1c, and so does this section.

**What stage 2 asks, most important first.**
0. With the sender build's interim receivers, today's early-skip arm (Amendment 3's fallback) and the floor-only receiver, what does the lost tail cost in measured milliseconds, and does it stall? (Part T.)
1. Does design U's release rule (§4.1, `ordering-hold.md`) behave as derived at one path: continuous through the three presets, order kept at Bulk and Auto where the derivation says it is, and within its proven factor of its own offline optimum U\*?
2. At two paths with skew: how much out-of-order delivery does U cause per slow-path share φ_slow, does the definition of d (with or without skew self-heals) change that, and what does the user actually wait?
3. Does the derived floor hold H (`floor-hold.md`) cover the late fills, and which memory W does the evidence favour?
4. Does the age half of the verdict (`age > D`) ever decide anything, and what does it change when it does?
5. When the floor overtakes its own copy, does that harm U?

### 7.1 What v3 changes, item by item

| item | change | stage 1 evidence it rests on | where |
|---|---|---|---|
| 1 | every keep/drop bar and every pass/fail clause gets a minimum effect size in the metric's units; an A/A calibration step sets the noise floor | the paired CI collapsed under common random numbers, so a deterministic 0.025 ms difference in a 20.7 ms p99 "excluded 0" (run-atomic reading) | §7.7, §7.9 |
| 2 | registered cells where the age gate refuses, with a refusal-count witness; the toy's D follows paper §5.6 instead of a fixed RTprop/2 | `D-fail` = 0 in every run of every cell, N = 2 included (finding 1) | §7.5 |
| 3 | the floor is its own datagram, the only floor model; FIFO paths are FIFO for every datagram; the overtaking case is kept in cells that allow in-path reorder, with a harm reading under U | shared fate was a toy artefact; the separate floor overtook its copy in about half of the verdicts, 487 transient passes per run at I1 (finding 2) | §7.6 |
| 4 | P4d at N = 2 is dropped; an absolute two-path implementation check replaces it | no setting produced a closed run longer than the bank at N = 2; the one zero came from the last copy (finding 8) | §7.8 |
| 5 | the two-path stall-episode readout is replaced by a per-packet wait measured from a loss-free arrival, with quantiles | with φ_slow > 0 skew keeps a hole outstanding at all times, so the run was one stall episode (finding 7) | §7.4 |
| 6 | each comparison names its bound and what it may claim; the passing-rule LP is bracketed by the MILP's dual bound and incumbent | LP 730 against a MILP incumbent of 1 300 at I2 (134 against 297 at G); the gap column is not a distance to the optimum | §7.3 |
| 7 | a wall-clock budget of 9 h, in four parts, each with the question it decides; U\* by an exact sweep that scales to two paths | 2.5–4.4 s per run, LP 0.5–57 s, MILP capped at 90 s; v2's per-cluster DP is O(n³) on one cluster of ≈ 5·10⁴ skew holes at N = 2 | §7.10, §7.3 |
| 8 | the claim "T_ρ is inert at N ≥ 2 under the derived H" is withdrawn, not checked | it rests on one seed over 8 s, and no stage 1 decision depends on it | §7.11 |

### 7.2 Model

**Cells and dial values used below** (as in stage 1):
- **I1:** i.i.d. loss, 0.5 % of originals and 2.5 % of copies (ε = 2.5 %).
- **I2:** i.i.d. loss, 3 % and 10 %.
- **G:** Gilbert–Elliott loss, mean 2.5 %, mean burst 8, loss 0.5 in the bad state.
- **B10:** a 200 ms blackout at 10 s.
- **The presets:** δ = 0.005 (Bulk), 0.5 (Auto), 50 (Realtime), the `net::delta_price` values. Bulk and Auto ship at ρ = 1, and Realtime at ρ = 0.99.
- ε_cell is a cell's mean per-copy loss: 2.5 % in I1, 10 % in I2, 2.5 % in G.
- **Q (light standing queue):** a queue delay added to the path's delay process, a sawtooth that rises from 0 to 10 ms over 0.98 s and falls back over 20 ms (slope −½, so the path stays FIFO), period 1 s, mean ≈ 5 ms. The windowed-min RTprop stays the propagation round trip, and the EWMA follows the queue. Toy choice.

Everything not listed here is as in §6.1 and stage 1 (source 10 000 packets/s, the 2 ms gap-report sampler, the tail sweep at `clamp(2·SRTT, 25, 100) ms`, the RFC 9002 hole law of 9/8 of the seq's path one-way delay at N = 2, B = N·2 048, common random numbers keyed by seed, seq, copy index and path).

**Paths and the FIFO property.** Each path p has a base one-way delay `owd_p` and a delay process `J_p(t)` sampled at the send instant:

```text
   arrival  =  τ  +  owd_p  +  J_p(τ)  +  r          (r = 0 on a FIFO path; r ~ U[0, J_r] i.i.d. per datagram where in-path reorder is allowed)
   J_p(t)   =  piecewise-linear through i.i.d. U[0, J] knots spaced κ = 2·J apart          slope ≤ ½  ⇒  FIFO for every datagram
```

- A path is FIFO exactly when the delay process has slope above −1, so `κ > J` is the condition. κ = 2·J is a toy choice.
- Arrival depends only on the path and the send instant, so it does not depend on the candidate. That keeps common random numbers without stage 1's work-around.
- Stage 1 enforced FIFO on originals only, and drew independent jitter for copies and floors. On a nominally FIFO path the floor could then overtake its copy. That is the source of the 487 transient passes (`toy.py` `send_copy`, `send_floor_only`, `last_orig_arr`). The engine's sim links are FIFO for every datagram (`floor-hold.md` §1, `sim.rs:30`), and so is v3's FIFO cell.
- Jitter J is swept over {0, 1, 4 ms}: 1 ms is the base, 4 ms is `sim_lost_tail`'s value. Reorder cells use J_r ∈ {1, 4 ms}, a toy choice matching the jitter values.
- **Clock phases.** The phases of the sampler and the tail sweep are drawn per seed and shared by every candidate, so they are common random numbers too. Only the A/A arm A′ draws its own (§7.7).

**D, the verdict's age bar, follows paper §5.6.** Stage 1 used `D = RTprop/2` with RTprop the fast path's round trip, at every δ.

```text
   D(δ)  =  min( b(δ)·RTprop , 2·RTprop )        b(δ) = clamp( 2^(−½·log₁₀(δ/δ_auto)), ½, 2 )        RTprop = max over live paths of 2·owd_p
```

- b(δ): derived, paper §5.4. It is ½ at Realtime, 1 at Auto and 2 at Bulk.
- RTprop: paper §5.6, "the largest over live paths". The toy's path round trip is `2·owd_p`, which is a toy choice: symmetric paths.
- At N = 1 Realtime this gives stage 1's D exactly, so stage 1's base cells are a bridge (§7.8).
- **This form is the arm D_RTprop.** RTprop is the exact propagation round trip, `2·owd_p` with no jitter or queue.
- **The engine computes the arm D_EWMA:** `D = min(b, 2)·SRTT` (`shed.rs:31` `shed_deadline_us`, fed `est.rtt()` at `emit_source.rs:631`), where SRTT is the EWMA RTT of `estimator.rs:465–481`: α = 1/8, and the first measured sample replaces the 50 ms seed. Before the first sample D = 0, so nothing is sheddable (`shed_allowed`).
  - **The toy's RTT sample** is one per gap report: the report's arrival at the sender minus the send time of the highest seq it newly acknowledges. That includes the jitter, the sampler wait and any queue, as an RTT sample without an ack-delay correction does. This is a toy choice.
  - SRTT exceeds RTprop by the mean jitter, the mean sampler wait and the mean queue, and it lags any change of the queue by about 8 samples (16 ms at one report per 2 ms tick).
- **Which path:** main today (the ADR-0084 build is pending) reads the EWMA RTT of the path with the highest loss estimate: `emit_source.rs:631` takes `est.rtt()` from `worst_eps_estimator`, which is the `max_by` over channel paths on `loss_rate()` (`channel_set.rs:40–53`). At N = 2 that is either path, depending on which loss estimate is higher at the moment. So the two-path age-gate cells run both ends as arms: D from the slow path (the paper's form) and D from the fast path (stage 1's form). The engine lies between them. That is a model-against-engine divergence, reported in §7.12.

**The floor (item 3), one model, as the engine sends it** (ADR-0084 decision 1, `adr84-engine-brief.md` step 8):
- At each verdict, the sender stages the last copy, then the floor `F = min{s ≥ A_rx : s ∉ V}`, on the same path at the same instant.
- While `F > A_rx`, the floor is sent again on every live path at each tail-sweep fire. Stage 1's refresh rule (RTT + 2 ticks + H + jitter) is gone.
- Each floor datagram has its own loss and its own delay draw, keyed by (seed, path, F, repeat index), so the same floor meets the same fate in every candidate.
- "Live paths" in the toy are all configured paths (toy choice). A floor sent into a blackout is lost by the loss process.

**H, the floor hold, is the derived estimator of `floor-hold.md` §2,** not stage 1's configured H§8 and not v2's fixed sweep:

```text
   H(t)  =  max{ ℓ_i : t − W ≤ a_i ≤ t }        ℓ_i  =  ( a_i − min{ a_j : j arrived before i, j sent after i } )⁺
```

- It is fed by every data arrival, source and repair, with the order rule strict on one path and non-strict across paths (`floor-hold.md` §1).
- W ∈ {1, 10, 60 s}; the base is 10 s (`floor-hold.md` §2, borrowed, owner to accept).
- Two fixed arms, and only these: H forced to 0 in the overtake cells (§7.6), and H forced to 5 ms in check P4a (§7.8).
- A marker is `(F, t_F + H(t_F))` per floor rise, and a hole passes at the earliest expiry among the markers that cover it.

**d for U.** As in §6.1: the running means d_all (holes closed by their own original or by a copy, the engine's definition) and d_rep (closed by a copy only).
- A hole's release deadline is fixed at its detection, `t_detect + ω·d(t_detect)/δ`. It is not re-evaluated as d moves.
- Before the first sample the H_o term is absent, so it counts as ∞, the C12 convention. No cold-start constant is needed.

### 7.3 Candidates, offline optima and what each comparison may claim (items 6 and 7)

**Candidates.**
- **S0:** serve until filled. The reference.
- **S1c:** the settled sender rule.
- **U:** the S1c sender plus the receiver release `min(t_detect + H_o, t_F + H)`, with the ack held (§4.1, `ordering-hold.md` §8):
  - `H_o = ω·d/δ`, ω ∈ {0, 0.3, 1, 3, 10, 100}, d ∈ {d_all, d_rep};
  - the rent arms are as in §6.2: linear, and the c_H arm at N = 1 only (its optimum needs the episode state, below).
- **R1:** U with the H_o term absent, the floor-only receiver: it passes only below the sender's floor, after H. It is the ω → ∞ end of the same law, not a separate rule. It runs in Part T and in the W sweep, where the floor pass is the only release.
- **R0e, today's early-skip receiver (Part T only):** the receiver arm Amendment 3 keeps until the release rule. It is modelled on `shed.rs:389` and `:44–78` and `reorder.rs:112–186`:
  - each buffered entry expires `hold` after it is buffered, and an expiry passes every hole below the largest expired entry;
  - `hold = SRTT/2` while its own budget is open, `holes_given_up < ε̂_recv·frontier`, and `(4·SRTT).clamp(60, 300) ms` otherwise;
  - ε̂_recv is the fraction of originals the receiver saw missing (a toy choice for the engine's incoming-loss EWMA), and SRTT is the sender's EWMA (a toy simplification);
  - its passes move the ack, so the sender stops serving them. They count in G_rx, and so in the sender's allowance, and a late fill refunds them.
  - It is today's code, and it carries a bit (the budget test selects the hold). It is a reference, not a candidate, and the deny test names it as an exception.
- **HZ:** P3's positive control, as in §6.2. It is a deliberate jump, not a candidate, and exists to show that P3 can fail.
- **Ablations, not candidates, in the age-gate cells only:** S1c with D = 0, and S1c with D from the fast path. They are configurations, not branches on a dial.

Dropped: S1, S2, S3 (stage 1 decided them) and R0 (today's timer). R0 ranked nothing in stage 1 and U replaces its role as the receiver that does not wait.

**NO-MODE-SWITCH in the toy.** No candidate reads a threshold on δ or ρ:
- H_o = ω·d/δ is one expression at every δ;
- R1 is the absent term, not a branch;
- D(δ) is the paper's clamp, continuous, with the kinks P3 must tolerate (§7.9);
- at ρ = 1 the count admits no verdict, so no floor exists, and nothing branches on ρ.

A toy source test pins this: no comparison on `rho`, `delta` or a hint name may select a branch outside the cell table. HZ is the one named exception.

**U\*: exact, by a sweep (item 7).** Per hole s, the offline data are the detect time t_s, the fill time R_s and the end of the hold `r_s = min(R_s, t_F(s) + H(t_F(s)))`. Releasing costs ω only if the hole would otherwise have been filled in order:

```text
   cost(Z)  =  (δ/d_end) · | ∪_{s ∈ Z} [t_s, r_s) |   +   ω · Σ_{s ∉ Z} 1{ R_s < t_F(s) + H }          Z = the holes held to r_s
```

- As in §6.3, a partial hold is dominated, so every hole is either released at detect or held to r_s.
- **The sweep.** Take the holes in order of t_s. Every interval held so far starts at or before t_s, so the held union to the right of t_s is one interval `[t_s, e)`, where e is the largest r among the holes held so far. So the state is e alone:
  - hold s: e ← max(e, r_s), and the cost rises by (δ/d_end)·(r_s − max(t_s, e))⁺;
  - release s: e is unchanged, and the cost rises by ω·1{R_s < t_F + H}.
  - All states with e ≤ t_s merge into one.
  - A state is dropped if another has a larger e at no higher cost, or a smaller e at a cost lower by at least (δ/d_end) times the difference in e.
- The sweep is exact, the same optimum as §6.3's totally unimodular LP. It costs O(n·k), where k is the number of holes whose hold is still open at t_s: about 10² at N = 2 with φ_slow = 0.5, against v2's O(n³) on one cluster of about 5·10⁴ skew holes.
- d_end is the run's final d, and U's own cost is computed with the same d_end, so the two are compared like with like.
- **The 2× bound is a proof check only where its premises hold:** one fixed d, and an H_o for every hole. In decision runs d moves and the first holes have no H_o, so a hole can pay `ω·d(t_detect)/d_end` in rent, or more. There ratio_U is reported against the 2× line, with holes detected before the first d sample excluded. The pass clause runs in check P4j (§7.8), with d frozen.
- The c_H rent depends on the episode's length, so its state is (episode start, e). It runs at N = 1 only, where clusters hold a few holes, by v2's DP.
- **Check P4e (rewritten):** the sweep, the TU LP and brute force agree on 200 small random instances, N = 1 and N = 2 shapes.

**What each comparison may claim (item 6).**

| comparison | bound | it may claim | it may not claim |
|---|---|---|---|
| U against U\* | exact optimum of U's release cost, given the run's own sender trace | `ratio_U = cost_U / cost_U*` is a distance to the optimum. The 2× bound is tested on isolated holes, and the ratio is reported on clusters | anything about the sender rule, which U\* takes as fixed |
| S1c against the passing-rule oracle (reported only, Part D) | `[MILP dual bound, MILP incumbent]` from HiGHS at the 90 s limit, on the S0 trace with last copies (§6.3 (a)); the LP is the weaker dual bound and is kept only as a cross-check | the distance to the oracle lies in `[HOL_S1c − incumbent, HOL_S1c − dual]`. "Near" needs the upper end below the bar; "far" needs the lower end above it; otherwise "not determined by this bound" | a distance to the best reachable sender rule. The oracle passes at `t_detect + H` with no tell latency, about one RTT earlier than any sender verdict can act, so even "far" is far from an unreachable bound |
| any candidate against S0 | none: a paired difference | an effect under §7.7's bars | optimality |

The stage 1 column `gap = (HOL − LP)/(HOL_S0 − LP)` is not reported again. Where the bracket is computed, both ends are printed.

### 7.4 What the user waits for (item 5)

At two paths with skew, the frontier is almost always blocked, so "frontier-stall episode" readouts measure the run length (stage 1 finding 7). The readout that replaces them measures, per delivered packet, how long after its *loss-free* arrival the consumer gets it. The loss-free arrival is the same instant in every candidate:

```text
   ã_j   =  τ_j + owd_{p(j)} + J_{p(j)}(τ_j) + r_j               loss-free arrival of seq j's original (from the draws, whether or not it was lost)
   y_j   =  delivery instant of j to the consumer (in order, released past, or a late fill)
   v_j   =  y_j − ã_j                                             wait beyond its own loss-free arrival
   y⁰_j  =  max_{i ≤ j} ã_i                                       loss-free in-order delivery (computed from the draws, no run needed)
   e_j   =  y_j − y⁰_j                                            wait caused by loss and by the release policy
```

- **Population:** every delivered seq, hole packets included. A hole packet's v is its repair time, which stage 1 excluded. Seqs passed and never filled are not in the quantiles; they are counted in G_rx and reported beside the quantiles.
- **Readouts:** p50, p99 and p99.9 of v and of e, and their means, per run.
  - v is what the user waits, including the skew's own in-order wait.
  - e is the part a policy can change. For S0 and S1c, e ≥ 0 except where a pass precedes a loss-free arrival.
  - Under U, e is negative where a release delivers past a skew hole before the loss-free in-order instant. That is U's gain, signed.
- **Little's law form:** Σ_j (y_j − a_j) over non-hole packets is the time-integral of the reorder buffer's occupancy. It stays the LP's objective (§7.3) and nothing else.
- **Always beside the out-of-order rate.** A wait metric alone rewards ω = 0, which releases everything. So every v and e readout is printed next to the out-of-order deliveries per delivered packet, split into loss, skew and jitter (§6.1 classes), and next to rbf.
- **Bridge (P4i):** at N = 1 on a FIFO path, `y⁰_j − ã_j = 0`. So e over non-hole packets equals stage 1's HOL per packet exactly. Checked on the I1 base.
- The frontier-blocked seconds T_stall stay as the priced term of J and of U\*'s cost (paper pricing, §1). They are an objective, not a user-wait readout, and are not reported as stall quantiles at N = 2.

### 7.5 Cells where the age gate refuses (item 2)

**The age of the first report.** A hole s is reported to the sender at the first sampler tick after a later seq arrives, and the report travels back on the fast path:

```text
   age₁(s)  =  max( ripe(s) ,   (a_det(s) − τ_s)  +  w  +  owd_fb )
   a_det(s) =  first arrival of a seq above s;   w ∈ [0, tick) the wait for the sampler;   owd_fb ≥ owd_fast;   ripe(s) = (9/8)·owd_{p(s)} at N = 2, 0 at N = 1
```

The gate decides something at a report only when `age ≤ D(δ)` and the count would have admitted the verdict.

**N = 1, arm D_RTprop: the gate can refuse only below Auto.** `a_det − τ_s ≥ owd + 1/rate` and `owd_fb ≥ owd`, so `age₁ ≥ RTprop + 1/rate > RTprop`. This is a property of the exact-RTprop arm only:
- **Realtime (b = ½, D = RTprop/2):** never refuses. This is a proof, not a prediction, and it explains stage 1's zero.
- **Auto (b = 1, D = RTprop):** never refuses, strictly.
- **Below Auto:** a first report is refused while `(b(δ) − 1)·RTprop > j + w + 1/rate`, with j the jitter draw. The refused share rises continuously from 0 at Auto to about 1 at `b ≈ 1 + (J + tick)/RTprop`, which is 1.3 at the base cell (δ ≈ 0.087). At Bulk (b = 2, D = 20 ms against an age of 10–13 ms) it is about 1. Then the hole is served, and its verdict, if any, waits until age > D.
**N = 1, arm D_EWMA (the engine's form): the gate can refuse at Auto.** Write `SRTT ≈ RTprop + J̄ + w̄ + q̄`, where J̄ is the mean jitter (J/2), w̄ the mean sampler wait (tick/2) and q̄ the mean queue, all lagged by the EWMA. The first report's age is `RTprop + j + w + q(t) + 1/rate`.
- **Auto (D = SRTT):** a first report is refused when its own jitter, wait and queue fall below the EWMA's mean, `j + w + q(t) < J̄ + w̄ + q̄`. That is about half of the first reports in the base cells (J = 1 ms, tick 2 ms), and more in Q's falling phase, where SRTT lags above the current RTT.
- **Realtime (D = SRTT/2):** a refusal needs `SRTT > 2·age₁`, so an EWMA above twice the current RTT. In the base cells that cannot happen: `J̄ + w̄ = 1.5 ms`, far below RTprop = 10 ms. In Q it needs the EWMA to lag a queue drop larger than about RTprop, but Q's drop is 10 ms at most. **Predicted: 0 in every one-path cell.** It is reported, not required, because it is a prediction, not a proof.
- **Below Auto** the EWMA arm refuses more than the RTprop arm at every δ, since SRTT > RTprop.

- Bulk and Auto run at ρ = 1, so there the count refuses first and the gate decides nothing. The gate decides only where ρ < 1 is composed with a δ below Auto. NO-MODE-SWITCH makes that a legal point, since ρ is an independent dial.

**N = 2 (owd 5 and 25 ms): the gate refuses fast-path holes when D is sized by the slow path.** With RTprop = 50 ms, D is 25 ms at Realtime:
- **Fast-path holes:** `age₁ ≈ 2·owd_fast + j + w ≈ 10–13 ms < 25 ms`. Every first report is refused, and the hole is served until its age passes 25 ms. Its first resend has usually filled it by then.
- **Slow-path holes:** `age₁ ≥ ripe = 28.1 ms > 25 ms`. Not refused at Realtime. At Auto (D = 50 ms) they are refused too.
- **With stage 1's D from the fast path** (5 ms), nothing is refused. That is stage 1's N = 2 zero.

**Two natural guesses point the wrong way.** A short D against a long RTT, or a slower report cadence, make reports *older* relative to D, so they refuse less. The gate refuses when D is long against the round trip that carries the report: b(δ) > 1, or D sized by a slower path than the one that reports.

**Registered cells.** 12 seeds, U at ω = 1 as the receiver:

| cell | dial point | arms | witness required | predicted |
|---|---|---|---|---|
| AG1 | N = 1, ρ = 0.99, δ ∈ {Realtime, Auto, δ at b = 1.15 (≈ 0.2, a toy choice inside the predicted rise), δ at b = 1.3, Bulk}; I1, I2 and Q (Q on I1 loss) | D_RTprop; D_EWMA; D = 0 | n_dec > 0 at b = 1.3 and at Bulk under D_RTprop, and at Auto under D_EWMA, or the cell is void | D_RTprop: n_dec = 0 at Realtime and Auto (check P4f), rising with b, about every first report at Bulk. D_EWMA: about half the first reports at Auto, more in Q; 0 at Realtime (reported) |
| AG2 | N = 2, Realtime, ρ = 0.99, φ_slow ∈ {0, 0.25, 0.5} | D from the slow path (paper); D from the fast path (stage 1); D = 0 | n_dec > 0 under the paper's D, or the cell is void | fast-path holes refused at first report; slow-path holes not refused; n_dec = 0 under the fast-path D |

- **The witness.** n_dec counts the (hole, report) pairs where `age ≤ D` and `Ab + 1 ≤ (1 − ρ)·S_src`, so the gate alone refused. Stage 1's `D-fail` column counted refusals without the count condition; v3 prints both.
- **Readouts:** the verdict-age distribution against D; G_rx/S_src; verdicts; v and e quantiles; repair sends. Each is compared between arms under §7.7's bars.
- **Reading (for the ledger verdict owed in §3.1):**
  - "D is live and changes X by a shown amount", or
  - "D is live but decides only timing: equivalent on every readout", or
  - void (no witness).
- No arm is adopted. D is accepted ADR-0084 law; the toy reports what it does.

### 7.6 The floor overtaking its copy, under U (item 3)

**Where it can happen.** On a FIFO path the floor arrives after its copy by construction (§7.2), so it cannot, and P4d′ checks that the count is 0. It can happen in cells with in-path reorder, where every datagram has its own draw r. There the derived H normally covers it, because data reordering raises ℓ. It still happens at cold start, and wherever the floor's draw exceeds the spread H has seen.

**Cells (Part D).** N = 1, J_r ∈ {1, 4 ms}, H ∈ {derived (W = 10 s), forced 0}, and three U points:
- Realtime δ at ρ = 0.99 with ω = 1 (H_o ≈ 0.2 ms, short);
- Realtime δ with ω = 100 (≈ 22 ms, long);
- Bulk δ composed with ρ = 0.99, ω = 1 (≈ 2.2 s).

The forced-H = 0 arm is the stage 1 case and must show overtakes > 0, or the cell is void.

**The harm reading.** Classify each floor pass of a hole that is later filled (a transient pass) by which term of U's `min(t_detect + H_o, t_F + H)` fired first:
- **(i) H_o had already released it.** Order is unaffected: the late fill would have been out of order anyway. The cost is G_rx one unit too high until the fill, so `Ab` is overstated. Readout: verdicts refused while Ab was overstated that the true Ab would have admitted ("attributable denials").
- **(ii) The floor pass was the release.** One extra out-of-order delivery, priced ω. Readout: these per delivered packet, as an added part of rbf.
- **(iii) Refused by the decoder** (`fh_miss_refused`, `floor-hold.md` §4): must be 0 in every cell, a hard check.

**Pre-stated criterion.** Overtaking is harmful under U if class (ii) per delivered packet is shown above the rbf bar, or attributable denials are shown above their bar (§7.7). It is harmless if both are equivalent to 0.

**Prediction.** Class (i) only at Realtime ω = 1, since H_o < RTT < t_F − t_detect. Class (ii) appears where a long H_o meets ρ < 1, at Realtime ω = 100 and Bulk at ρ = 0.99. At Bulk and Auto with ρ = 1 there is no floor, so this case cannot arise.

### 7.7 Minimum effect sizes and the A/A calibration (item 1)

**The reading rule, for every bar.** For a paired per-seed difference Δ_k (common random numbers), with mean Δ̄ and 95 % paired-bootstrap CI [lo, hi], and a bar of size m in the metric's units:
- **shown larger** if lo > m, and **shown smaller** if hi < −m;
- **equivalent** if −m ≤ lo and hi ≤ m;
- **undetermined** otherwise.

For a level clause (rbf, `fh_miss`, the overtake classes, the denials, n_dec), the per-seed value is the level itself, read against the bar the same way.

An undetermined decision clause is rerun once at 24 seeds, registered now and paid from the reserve (§7.10). If it is still undetermined, it is reported so. Under a collapsed CI a deterministic difference smaller than m now reads "equivalent", not "shown".

**The size of each bar:**

```text
   m_metric  =  max( T_metric ,  NF_metric )          NF = the 95th percentile over seeds of | m_k(A) − m_k(A′) |, the largest over the A/A cells
```

Every T has an absolute floor, so T > 0 in every row, ρ = 1 included, and a zero noise floor cannot produce a zero bar. A relative T is taken against S0's value in the same cell; in P3, against S0 at the preset point itself.

**The A/A calibration (Part A, before any decision cell runs).**
- **A and A′ are the same candidate, configuration and seed, except for the phases of the sender's two clocks.** The gap-report sampler's phase is drawn from U[0, tick) and the tail sweep's from U[0, P), separately for A and A′.
- **Why that is inert by design:** no candidate's law reads a clock phase, and the engine's phases are set by when the connection started. It is not bit-inert, though. It moves decision instants by less than one tick, and that is exactly the micro-timing difference that read as "shown" in stage 1. The same seed with the same phases would be bit-identical and give NF = 0.
- **Cells (one path, frozen in Part A):** I1, I2 and G at N = 1. Each runs U at Realtime with ρ = 0.99 and U at Auto with ρ = 1, all at ω = 1, 12 seeds, two arms: 6 cells. Plus the five-point K and P sets of §7.9, and R0e and R1 in Part T's I2 tail cell. **Two-path noise floors** are frozen at the start of Part C, from {I1, G} at φ_slow = 0.25, and do not apply to the one-path parts.
- **Output:** NF for every metric in the table below, frozen in a committed file before Part B starts.
- **A/A self-check:** the mean A/A difference must read "equivalent" against T on every metric. If it reads "shown", the toy depends on clock phase, which is a defect. Part B does not start until that is fixed.

**The bars** (T is the declared tolerance; every T is a toy choice unless cited):

| clause | metric | unit | T | provenance of T |
|---|---|---|---|---|
| any latency comparison | v and e at p50, p99 and p99.9 | ms | 1 ms | the base jitter amplitude: a shift smaller than one jitter amplitude is inside the per-packet delay noise the consumer already sees (toy choice) |
| any latency comparison | mean v, mean e | ms | max(5 % of S0's value, 0.01 ms) | toy choice |
| spend comparisons | G_rx / S_src at the end of the run | fraction of S_src | max(0.05·(1 − ρ), 1/S_src) | one twentieth of the declared allowance, and at least one packet (toy choice) |
| spend comparisons | largest single-episode spend | units | max(0.05·⌊(1 − ρ)·S_src⌋, 1) at the end of the run | as above |
| bandwidth comparisons | repair sends per delivered packet | ratio | max(5 % of S0's value, 1/S_del) | toy choice; the floor is one send per run |
| rbf at Bulk and Auto "keeps order" (replaces "within the CI of 0") | rbf, released-then-filled ÷ holes | fraction | ε_cell² | two consecutive lost repairs at the cell's mean per-copy loss ε_cell: "order kept through the second repair round" (`ordering-hold.md` §4.2's τ_op of two rounds) |
| skew out-of-order rate and d_all against d_rep | skew out-of-order deliveries per delivered packet | fraction | max(10 % of the larger of the two values, 1/S_del) | toy choice; the floor is one event per run |
| overtake harm, class (ii) | floor-pass releases later filled, per delivered packet | fraction | ε_cell² | as rbf |
| overtake harm, attributable denials | count per run | units | 1 | one verdict per run (toy choice) |
| P3 step and point statistics (§7.9) | the readout's own | its unit | that readout's m | as the readout |
| P3 positive control (HZ must fail) | K on rbf and on mean e at Auto | as above | as above | HZ fails only if K is "shown" beyond m |
| `fh_miss`, FIFO cells (N = 1) | fills after t_F + H | count | 0, hard | `floor-hold.md` §6; H ≡ 0 is exact only on a FIFO path |
| `fh_miss`, N ≥ 2 and N = 1 reorder cells | one-sided 95 % upper bound of fh_miss ÷ floor-passed holes | fraction | 1 − ρ | `floor-hold.md` §6: misses at the allowance's own rate would spend it twice |
| P1, P2, P4 | as stated in §7.8 | | exact, or the stated tolerance | §7.8 |
| age-gate witness | n_dec | count | > 0 required | item 2 |
| P4j: ratio_U on isolated holes, d frozen | `ratio_U − (2 + ω·N_{R=∞}/cost_U*)` per seed | dimensionless | ≤ 10⁻⁹ | proven bound (`ordering-hold.md` §3.2), whose premises hold only with d frozen; floating-point slack only |

### 7.8 Implementation checks, the two-path control and bridges (item 4)

These debug the toy. They are not evidence for the derivation, and no ranking is read until they pass.
- **P1** (unchanged): `G_rx(t) ≤ ⌊(1 − ρ)·S_src(t)⌋` at every instant, and no hole released by U is pruned at the sender without a verdict.
- **P2** (unchanged): at ρ = 1 and at 1 − ε with ε·S_src < 1, every sender trace equals S0's. U may release, with zero loss.
- **P4a:** at I1, N = 1, with H forced to 5 ms, S1c's isolated-hole stall exceeds S0's by H within ±1 ms. That is ±½ tick, because verdicts are taken on the 2 ms sampler.
- **P4b:** S1c's G_rx/S_src at I1 is ≤ 1.5·ε·ĥ.
- **P4c:** U's consumer stall per isolated hole equals `min(H_o, R − t_detect, t_F + H − t_detect)` within 10 µs. The release is a receiver timer with no tick, so the slack covers event-time rounding only.
- **P4d (dropped at N = 2 and at N = 1).** P4d checked run-atomic spending (S2), which stage 2 no longer runs. Stage 1 also showed why its N = 2 case does not arise under independent per-path loss:
  - consecutive missing seqs on one path are short, geometric in φ_slow;
  - a long closed run appears only after data resumes, and by then the sweep has drained the bank.

  The construction that would produce one is a loss on both paths at once, shorter than the sweep period minus the report delay, at a time when the bank is smaller than the run. That is a one-path cell in disguise. It is recorded here and not run.
- **P4d′ (replaces P4d at N = 2, absolute):** N = 2, φ_slow = 0.5, no loss, J = 1 ms FIFO.
  - The skew hole count equals the number of slow-path seqs overtaken by a later fast-path seq, computed from the draws.
  - S1c sends 0 verdicts and 0 serves, because skew holes fill before they ripen.
  - Under U, the skew out-of-order count equals the number with `Δ_j > H_o`, computed from the draws.
  - On every FIFO cell the overtake count is 0.
- **P4e:** U\*'s sweep equals the TU LP and brute force on small instances (§7.3).
- **P4f:** under D_RTprop only, n_dec = 0 at N = 1 at Realtime and at Auto, in every cell (§7.5's proof). Under D_EWMA nothing is required at Auto, and Realtime is a reported prediction.
- **P4g:** at N = 1 on a FIFO path, d_all = d_rep exactly, since there are no self-heals. So the d arm is not run there.
- **P4h (bridge, D and floor):** at N = 1 Realtime I1 with J = 0, the v3 toy and the stage 1 toy (commit `c9ab6e9a`, `floor_with_copy = False`) give bit-identical S0 runs, and S1c runs with the same verdict count. Any other difference must lie in floor timing alone, and is reported.
- **P4i (bridge, wait metric):** §7.4.
- **P4j (U's 2× bound):** I1, N = 1, with d frozen at RTprop from the start, so every hole has H_o. This is a check configuration, and RTprop as the frozen value is a toy choice. On isolated holes, every seed must satisfy `ratio_U ≤ 2 + ω·N_{R=∞}/cost_U*`, at ω ∈ {1, 10} and the three presets.
- **Counts:** P4e's 200 instances and the seed counts (12, and 24 on rerun) are toy choices.

### 7.9 P3, continuity, with two nudge widths

v2's midpoint test cannot tell a kink from a step. b(δ) is clamped exactly at the Bulk and Realtime presets (b = 2 and ½), so D(δ) is continuous but flat on one side there. A kink with one flat side gives `|Δ| = ½·|slope|`, the same signature as a step at the preset. And under a collapsed CI, a flat readout failed v2's ¼·slope clause on any tiny deterministic Δ. v3 uses five points per preset p, at p·{0.98, 0.99, 1, 1.01, 1.02}, and two statistics per seed. The ±1 % width is a toy choice, half of the registered ±2 %:

```text
   J_k(h)  =  m_k(p·(1+h)) − m_k(p·(1−h))                    Δ_k(h)  =  m_k(p) − ½·( m_k(p·(1−h)) + m_k(p·(1+h)) )
   K_k     =  2·J_k(0.01) − J_k(0.02)            step at p:   K = jump size;      smooth or kinked:  K = 0 to third order
   P_k     =  2·Δ_k(0.01) − Δ_k(0.02)            anomaly at p (the preset alone differs):  P = its size;   smooth or kinked:  P = 0 to second order
   pass  ⇔  K̄ and P̄ both read "equivalent" against the readout's bar m;      fail  ⇔  either reads "shown"
```

- Both statistics cancel a kink: J(h) and Δ(h) are linear in h on a kink, and the combinations remove the linear term.
- **What is left over.** K is exact to third order: J is odd in h, so a smooth readout leaves only `O(h³)`. **P is not zero to second order.** A smooth readout has `Δ(h) = −½·m″·(h·p)²`, so curvature leaves

  ```text
     P_smooth  =  m″(p) · (0.01·p)²
  ```

  - For a readout ∝ 1/δ (H_o = ω·d/δ and everything linear in it), `m″ = 2·m/p²`, so `P_smooth = 2·10⁻⁴·m(p)`. That is 250 times below a 5 % relative bar and below every absolute floor at the presets. It is under the bar.
  - Other readouts can curve more, rbf for example, which follows a survival function of H_o. A P that reads "shown" on such a readout triggers one registered follow-up: two more points at p·(1 ± 0.005). Three widths fit `Δ(h) = a + c·h + k·h²` exactly, and only a ≠ 0 (shown against the bar) is a preset anomaly.
- **Clock phases in the decision runs.** A 1 % nudge moves D by 0.05–0.25 ms, against a 2 ms sampler tick. With one phase for every seed, the readouts would be staircases in δ at the scale of the nudge. So the sampler and sweep phases are drawn per seed (shared across candidates and nudge points, §7.2). That is required in every P3 run.
- **Noise floor on K and P themselves.** The A/A of §7.7 also runs whole five-point sets: arm A′ redraws the phases for all five points. `NF_K` and `NF_P` are the 95th percentiles over seeds of `|K_k(A) − K_k(A′)|` and `|P_k(A) − P_k(A′)|`, and they enter K's and P's bars as `max(T, NF)`. The sets run on U at Auto and Realtime, I1, N = 1, ω = 1.
- K catches a step located at p, and P catches the visualizer-style defect where the preset point itself behaves differently.
- For ρ the nudges are taken on 1 − ρ: ρ ∈ {0.9902, 0.9901, 0.99, 0.9899, 0.9898}.
- **Which ρ each preset runs at:** Bulk and Auto at ρ = 1, as shipped, and Realtime at ρ = 0.99. The ρ-nudge block runs at the Realtime δ in I2, where the count binds.
- **Readouts under P3:** mean e, e at p99, rbf, the skew out-of-order rate (N = 2), G_rx/S_src, and repair sends per delivered packet.
- **Positive control:** HZ at Auto must **fail** on rbf and on mean e, at N = 1 and at N = 2 (φ_slow = 0.25). If it passes, P3 has no power, every P3 reading is void, and the remaining P3 blocks are skipped to save time.
- **Planted controls**, U at Auto, I1, N = 1, ω = 1. Each is a deliberate defect, not a candidate, and is named in the deny test:
  - **STEP:** H_o doubled for δ < p. It must **fail** on K.
  - **SPIKE:** H_o doubled at δ = p exactly, the preset-only override. This is the only test of P. It must **fail** on P, and K must pass, since K does not read m(p).
  - **KINK:** H_o times `1 + 25·max(0, 1 − δ/p)`, continuous at p and steep on one side (×1.5 at 0.98·p). Like the b(δ) clamp, but large enough to see. It must **pass** both K and P. If it fails P, the curvature follow-up above decides; if it still fails, P3 is too sensitive and its fails are reported as undetermined.
  - Readouts: rbf and mean e, as for HZ.

### 7.10 Parts, runs and the wall-clock budget (item 7)

**Per-run cost, estimated from stage 1.**
- About 3.5 s per 20 s run at N = 1, and 6 s at N = 2: stage 1's 2.5 and 4.4 s, plus 40 % for the release state, the ℓ sampler and the delivery-time record. The 40 % is an estimate, and Part A measures it.
- U\* about 0.2 s per run at N = 1 and about 1.5 s at N = 2.
- LP up to 57 s and MILP 90 s (capped).
- One process, seeds in sequence (low memory); 12 seeds per configuration unless stated.

**Budget: 9 h sequential in all**, with 1 h held in reserve for the registered 24-seed reruns. **Built now: A, T and B, about 3.3 h, capped at 4 h.** C and D (about 4.75 h) wait on the hold re-derivation, and their rows are kept as the current registration.

| part | runs (configurations × 12 seeds) | estimated | cap | what it decides |
|---|---|---|---|---|
| **A. Calibration and checks** | A/A 6 cells × 2 arms (144 runs); K and P sets, 2 presets × 5 points × 2 arms (240 runs); P4j, 6 configurations (72 runs); P4h bridge on the stage 1 toy and the v3 toy (≈ 144 runs); unit tests; timing | ≈ 0.6 h | 0.75 h | the one-path bar sizes (frozen and committed before T and B); instrument validity (P1, P2, P4, A/A self-check); the measured cost per run, which drives the fallback below |
| **T. The lost tail (question 0, first decision part)** | §7.13: 3 cells × 4 receivers (R0e, R0c, R1, U at ω = 1) × 12 seeds = 144 runs, short objects | ≈ 0.25 h | 0.5 h | the cost of Amendment 3's fallback in measured ms: v and e quantiles, spend, tail time and stalls, R0e against R0c against R1 |
| **B. U at one path (question 1)** | I1: ω (6) × δ (15: 5 points × 3 presets) = 90; I2 and G: ω (6) × 3 presets = 36; c_H arm at 3 presets; HZ at Auto, 5 points; ρ nudges at Realtime in I2, 9; d_rep at 3 presets (P4g witness), 3; P2 at ρ = 1 and 1 − ε, 2; planted controls STEP, SPIKE and KINK, 5 points each, 15; AG1 (moved here from D: 5 δ × 3 cells × 3 arms), 45. In all 208 configurations, 2 496 runs | ≈ 2.5 h | 2.75 h | P3 at all three presets for U (a fail is a NO-MODE-SWITCH defect that blocks the release-arm build); the ω band in which Bulk and Auto keep order (rbf bar), handed to `meas-disorder-price` as the threshold the measured ω is read against; ratio_U against the 2× bound; P3's power (HZ) |
| **C. Two paths with skew (question 2, and question 4 at N = 2). Waiting on `tail-and-hold.md`** | φ_slow = 0.25: ω ∈ {1, 10} × 15 δ points × d (2) = 60, plus ω ∈ {0, 0.3, 3, 100} × 3 presets × d (2) = 24; φ_slow = 0.5: ω ∈ {1, 10, 100} × 3 × 2 = 18; φ_slow = 0: 6 × 3 with d_all only = 18; HZ, 5; slow-path outage (B10 on the slow path, φ_slow = 0.5) ω ∈ {1, 10} × 3, 6; AG2 ablation arms, 6. In all 137 configurations, 1 644 runs | ≈ 3.4 h | 3.75 h | d_all against d_rep (the finding owed to `ordering-hold.md`: which d its law reads); the skew out-of-order rate and the v and e quantiles per φ_slow and preset (the input to the owner's C1 decision, `ordering-hold.md` §5); P3 at two paths; the AG2 witness and the D reading at N = 2 |
| **D. Floor, H and the rest (questions 3–5). Waiting on `tail-and-hold.md`** | W spike cell, 75 s: N = 2, φ_slow = 0.25, slow-path delay +40 ms for 200 ms at t = 5 s, ramped down at slope −½ so the path stays FIFO; W ∈ {1, 10, 60 s} × {U at ω = 1, R1} at Realtime (72 runs); W steady cells: N = 1 with J_r = 4 ms, and N = 2 at φ_slow = 0.5, W ∈ {1, 10} × 2 (96 runs); overtake cells, 12; jitter J ∈ {0, 4 ms} FIFO × 3 presets, 6; S1c bracket at I1 and G, 6 seeds | ≈ 1.35 h | 1.5 h | the W recommendation to the owner; the overtake harm reading; jitter sensitivity; S1c's bracket (reported only) |

**The W sweep needs a spike that can age out.** At 20 s, W = 10 s and W = 60 s are indistinguishable. So the spike cell runs 75 s: the spike at 5 s ages out of W = 60 s at 65 s.
- **Witness, per arm:** H returns to its level before the spike within W + 1 s after the spike ends. fh_age at the floor passes reaches W.
- **Rule:** the recommended W is the one with the smallest floor-pass e at p99 (R1 arm) among those that pass the fh_miss bar in every W cell. If the passing values are equivalent on e, the recommendation is 10 s (`floor-hold.md` §11's default). The owner accepts W; the toy only informs.
- The spike (+40 ms for 200 ms, then down at slope −½) is a toy choice: a queue excursion on the slow path. A sudden drop would reorder the path, so the ramp keeps it FIFO. The spike sets H through cross-path lateness only.

**Fallback, pre-registered.** If Part A's measured cost projects B + C + D above 7.25 h, cut in this order until it fits:
1. the S1c bracket;
2. Part C's ω ∈ {0, 0.3} at φ_slow = 0.25;
3. ω = 100 at φ_slow = 0.5;
4. the jitter block.

Part A, Part B, the witness cells AG1 and AG2, the overtake cells and every P3 block are never cut. After any cut, the stages still answer their questions, at coarser ω resolution.

**Order and stopping.** Run A, then T, then B, then (once the hold re-derivation lands) C and D, and read each part before the next starts.
- If A fails, fix the toy and rerun A.
- If HZ passes P3 in B, P3 is void, and the remaining P3 blocks of C are skipped.
- No part's result changes a later part's registration. A later part may only be cut by the fallback.

### 7.11 Dropped and withdrawn (item 8 and housekeeping)

- **Item 8, withdrawn.** Stage 1's sentence "T_ρ is inert at N ≥ 2 under H§8" rests on one seed over 8 s. It is withdrawn as a claim and stands only as an observation. No stage 1 decision rests on it: T_ρ was dropped on the five registered one-path cells. Checking it would mean running S3 again, a candidate stage 2 does not have, and the result could change no decision. Its premises have also moved: v3's D at two paths comes from the slow path, and H is the derived estimator.
- **B60 and T** (the 60 s blackout and the dead tail) are not run. They ranked run-atomic, which is dropped. The lost tail is covered by the engine's T1 and T1′.
- **v2's fixed H sweep** {0, 2.5, 5, 10 ms, ½ and 1 SRTT, H§8} is replaced by the derived H, W and the forced-0 overtake arm. A fixed H is not a law any build will ship.
- **v2's P5 decisions** for run-atomic and T_ρ are gone. U is reported, not adopted: adoption waits on the measured ω (`ordering-hold.md` §10).

### 7.12 Open questions this plan does not settle

1. **The engine's D at N = 2 is not the paper's.** The engine reads the EWMA RTT of the path with the worst loss estimate (`channel_set.rs:51`); paper §5.6 says the largest windowed-min RTprop over live paths. AG2 brackets the two, but which path the engine picks depends on loss estimates the toy does not model. This is a model-against-engine divergence, and it needs its own bounding test (CLAUDE.md, testing discipline). Recorded, not fixed here.
2. **The tolerances T are toy choices** (1 ms, 5 %, 1/20 of the allowance, ε², one verdict). They are decision bars, not law constants, and the reviewer may want different ones. They must be fixed before Part A runs.
3. **Is clock phase a fair A/A perturbation?** If the reviewer prefers a second nuisance, say an independent jitter salt with loss and placement draws kept, the noise floor grows. The table's `max(T, NF)` form accepts either.
4. **Path round trip in the toy.** `2·owd_p` assumes symmetric paths, while the toy's reports return on the fast path, which would make the slow path's round trip 30 ms rather than 50. AG2's prediction holds for both (fast-path report age 10–13 ms against D = 15 or 25 ms), but the D reading differs in size.
5. **The EWMA arm's RTT sample** (one per report, including the sampler wait) is a toy choice. The engine's sample site and its ack-delay handling were not read for this plan, and they set the size of SRTT − RTprop, which is what makes the gate live at Auto.
6. **R0e's ε̂_recv** is a cumulative missing fraction in the toy, not the engine's per-path EWMA. That sets when its hold falls back from SRTT/2 to 60–300 ms.
7. **d's cold start.** The H_o term is absent until the first d sample. Whether the engine does the same is for the release-rule build.
8. **The c_H rent at two paths** has no scalable exact optimum here: the episode state does not collapse the way e does. It runs at one path only.
9. **The 40 % per-run overhead is a guess.** Part A measures it; if it is much larger, the fallback cuts the ω resolution first.
### 7.13 Part T: the lost tail under the interim receivers (owner, 2026-11-22)

**Why first.** The owner answered "fix the lost tail first", before the sender build. Amendment 3 ships the sender with today's receiver arm (R0e) kept below ρ = 1, with the floor pass beside it. Its fallback is the floor-only receiver (R1). Part T prices that choice in measured milliseconds.

**Cells.** Realtime, ρ = 0.99, N = 1, FIFO, J = 1 ms, sender S1c with the floor:
- **The short object** is S_src = 12 147 seqs (`sim_lost_tail.rs`'s object). Every copy of the last 347 seqs is lost during a `dead` window of 1.5 s from the send of the first of them, T1's shape (`adr84-engine-brief.md` T1); after the window, copies arrive. The run continues until every seq is delivered or passed, or until 40 s (T1's `limit_s`).
- **Loss before the tail:** I1; I2, which spends the allowance before the tail; and **L10**, i.i.d. 10 % on originals and on copies at RTT 100 ms, with the tail sweep at its 100 ms cap. Toy choice, the high-loss, long-RTT corner.
- At the tail the allowance is at most ⌊0.01·12 147⌋ = 121, less what loss already spent. So at least 226 tail seqs are refused and served, in every cell.

**Receivers:** 12 seeds each.
- **R0e, the interim:** early skip, and the sender's allowance counts every pass (`Ab = G_rx + pending`).
- **R0c, option (c)** of `tail-and-hold.md` §1.4: the same early-skip receiver, but the sender's allowance counts only the holes the floor passed, `Ab_c = G_floor + |{v ∈ V : v ≥ A_rx}|`. G_floor is the floor pass's own passes net of their late fills, and a late fill refunds the writer that passed the hole. The early skip's passes still move the ack and still count in G_rx, which is printed against (1 − ρ)·S_src but admits nothing.
- **R1:** the floor-only receiver.
- **U** at ω = 1 as the target.

**Readouts:**
- v and e at p50, p99 and p99.9, and their means, over the whole object (§7.4);
- G_rx/S_src against 1 − ρ (for R0c also G_floor/S_src, the share its sender budgets), and the passes by decider (the early skip's expiry, the floor pass);
- floor datagrams and last copies sent, each per delivered packet;
- **the tail time:** from the send of the last seq to the first instant the ack frontier reaches S_src;
- **stalled:** the frontier has not reached S_src at 40 s, counted per seed;
- the share of tail seqs that got a verdict, and how many filled after the dead window.

**Witnesses:** the dead window covers ≥ 1 copy of every tail seq; R0e's hold falls back at least once in I2 (its budget test bound), or that reading is void; P1 holds at every instant.

**Reading.** Each readout is compared pairwise between R0e, R0c and R1 under §7.7's bars, against U as the target. Nothing is adopted. The result is the measured cost of keeping the early skip in the interim, with and without option (c), and whether any receiver leaves the tail stalled. `tail-and-hold.md` §1.4 predicts that R0c's passes come no later than R0e's, that its total abandonment exceeds R0e's by at most G_floor ≤ (1 − ρ)·S_src, and that its tail is passed by the floor where R0e's is refused.

### 7.14 The floor-pass hold: two candidate rules (Parts C and D, registered now, waiting)

From `tail-and-hold.md` (branch `tail-fix-derivation`, f1851c4c) §2.3. The floor pass's hold gets two candidates, run with every receiver that has a floor pass (R1 and U):
- **HW, the windowed maximum:** `floor-hold.md` §2's H, W = 10 s (§7.2).
- **E0, the event rule in its form E0:** pass the holes below F once, on every path the floor was sent on, the floor itself or a later-sent datagram has arrived, or else at `t_F + PTO_p`:

  ```text
     p resolved for F   ⇔   τ_max_rx(p) > τ_F   ∨   floor_rx(p) ≥ F   ∨   τ_max_rx(any) ≥ τ_F + PTO_p   ∨   now ≥ t_F + PTO_p
     pass [next_deliver, F)   ⇔   every path that carried this floor is resolved for F
     PTO_p  =  srtt_p + max( 4·rttvar_p , 1 ms )
  ```

  - σ_p = τ_F, the floor's own send stamp (form E0; form E1 carries per-path copy stamps and is not run).
  - srtt_p and rttvar_p: RFC 6298 §2 estimators per path over that path's RTT samples, as the toy defines them for D_EWMA (§7.2). 4 and 1 ms: cited, RFC 6298 §2 and RFC 9002 §6.2.1.
  - E0 has no memory, and it reads no dial.

**Cells.**
- The two-path cells of Part C at φ_slow ∈ {0.25, 0.5} (I1), and the slow-path outage (B10 on the slow path).
- The W spike cell of Part D (E0 forgets the spike by construction, HW holds it for W).
- **R (in-path reorder):** N = 1 and N = 2 at φ_slow = 0.25, each with per-datagram i.i.d. jitter J_r = 4 ms on every datagram and no rate limit, the netem `delay … jitter` shape without `rate`. Here a later-sent datagram can arrive before an earlier one on the same path, so E0's per-path witness can fire while a copy is still in flight. This is the cell that can show E0 missing. Witness: the overtake count of §7.6 > 0, or the cell is void.

**Readouts, per rule:**
- **the wait** `t_pass − t_F` per floor-passed hole: p50, p99 and the maximum;
- **the miss rate:** fills arriving after the pass, per floor-passed hole, read against the `fh_miss` bars of §7.7 (hard 0 on FIFO cells; the one-sided 95 % upper bound against 1 − ρ on two-path and reorder cells);
- which term of E0 resolved each path (per-path witness, floor, send-anchored silence bound, local silence bound), as its bind gauge;
- v and e over the stream (§7.4), so the wait is read for what it costs the user.

**Reading.** HW against E0 on wait and miss rate under §7.7's bars, per cell. Predicted (`tail-and-hold.md` §2.4): equal waits of about the skew at two paths when both legs carry repair, E0 shorter after a spike and when the slow leg carries only old originals, E0 at the silence bound on a dead copy-carrying path, and E0's misses above HW's only in R. Nothing is adopted; the owner decides the hold with `tail-and-hold.md`. Budget: about 0.5 h, added to C and D, re-sized when those parts are re-registered.

## 8. What this blocks

From the commit-order table of `rho-step-sites.md` §5:

**Can proceed now under ADR-0084 (sender-side, or neutral):**
- brief steps 1–8, with **v11 exactly as approved**;
- C2, C5, C1, C7 (with §3.1's correction: subtract passed verdicts, not V) and C8 (after C9's floor pass).

**C3 and C4** need only C12's refresh term. Split C12 into "refresh at every ρ" (now) and the marker term (R1); C3 and C4 follow the refresh half.

**Accepted for step 7 (owner, 2026-11-21, ADR-0084 Amendment 2):**
- the last copy with every verdict (F1);
- the `(next_expected, G_rx)` pair-merge rule (§3.1).

Run-atomic and T_ρ wait for P5. *(Stage 1: both dropped, `tools/toy/results-stage1.md`.)*

**Receiver side:**
- C9, C11 and C12's marker term **can proceed as R1**, the staging point of both branches. *(2026-11-21, owner: not shipped as R1. ADR-0084 Amendment 3 builds the sender side first and keeps the old receiver arm until U's release rule; the floor pass and its marker are in the sender build, C9–C11 and C12's expiry wait.)*
- After O3:
  - **yes:** C9 and C11 gain U's release arm, keyed by H_o(δ). That waits on the H_o derivation, not on a wire change;
  - **no:** RX's local pass and Â wait on P5 and a wire ADR (v12 or later).
- C10 is independent.

**Paper first (FORMULA-FIRST).** Before code, paper §5.6 owes:
- the last copy;
- the pair-merge rule;
- for U, §5.1's two frontiers and H_o's derivation;
- for RX, the receiver rule;
- T_ρ and run-atomic, only if P5 keeps them.

Ledger C13 points here for the time-domain T_ρ.

## 9. For the owner

**The deciding question (O3).** When a lost packet's repair arrives after the packets behind it were already handed on, the engine delivers it anyway, out of order. That is what happens today (`reorder.rs`). **Does that late packet count as delivered for your 99 %?**
- **Yes:** the receiver can stop waiting on its own hold, as today's Realtime does, and nothing is lost by that. The only real losses are the sender's counted give-ups. This needs no new protocol field, but it does need one more derivation: how long to hold before releasing order, as a function of δ (the Bulk end must keep today's strict order).
- **No:** a release is a loss, so the receiver may release only within an allowance the sender sends it, as one new protocol field in a later version. Until then the receiver waits for the sender's give-up, about 5 ms slower per hole than today at a 10 ms round trip.

**In both branches, now.** Keep v11 as approved. Every give-up sends one last copy of the packet (F1). The sender merges the receiver's loss count safely across reordered acks.

**Worked example, estimated, if H = H_o = 5 ms.** One path, 10 ms round trip, 10 000 packets/s, Realtime ρ = 0.99, 0.5 % lost after FEC, 2.5 % of resends lost; times from the lost packet's send. H is UNDERIVED, and the simulation reports every row at H = 0, 2.5, 5 and 10 ms.

| rule | typical hole | resend also lost | allowance used | counted? |
|---|---|---|---|---|
| today's Realtime timer | ≈ 10 ms | ≈ 10 ms (packet lost) | none recorded | **no** |
| never skip (ρ = 1) | ≈ 16 ms | ≈ 40 ms | 0 | yes |
| ADR-0084 as written | ≈ 21 ms (= 16 + H); ≈ 16 ms at H ≈ 0 (N = 1) | ≈ 21 ms | ≈ 0.5 % | yes |
| + last copy (R1, now) | ≈ 16 ms | ≈ 21 ms | ≈ 0.01 % | yes |
| U, if O3 = yes | 5 + H_o(ω): 5.2–27 ms over ω = 1–100 | ≈ 10 ms, then sender give-up | ≈ 0.01 % | yes |
| RX, if O3 = no | ≈ 10 ms (= 5 + H); ≈ 5 ms at H ≈ 0 | ≈ 10 ms | ≈ 0.5 % | yes |

Bandwidth is about one resend per lost packet in every row except "ADR-0084 as written". The control messages add 50–100 small datagrams per second.

**Other choices that are yours:**
- **O1.** Is a skipped packet free inside the allowance? Default **yes**: the triangle has no loss price.
- **O2.** After a long clean stretch, one outage may skip about 2 000 consecutive packets (0.2 s) and still keep the 1 % promise, and repeated outages may each do so while the bank lasts. **Is that acceptable?** If not, the promise needs a window length, a number only you can declare, because the one window derivable from ρ alone forbids skipping any two adjacent packets.
- **O4.** Only under RX: may a full consumer drop within the allowance instead of backpressuring? Default **no**.
- **The ADR-0084 amendments.** The last copy, the pair-merge rule, and later the decision-2 store change (RX only) and any wire field are proposals against an accepted ADR.

**Open, not owner calls:**
- H is UNDERIVED (`ordering-hold.md` §8 proposes a path quantity: ≈ 0 at N = 1). Under U, H_o is derived as `ω·d/δ`, and ω, the disorder price, is to be measured. Every headline number depends on them;
- the sender cannot see a run's end during a one-path outage (§3.3);
- D is predicted degenerate at Realtime N = 1 and is owed a ledger verdict (§3.1);
- C7's feed correction (§3.1).

### Owner answers (2026-11-21)

**Decided:**
- **O3 = yes: design U.** A late out-of-order fill counts as delivered. RX (§4.3) is not pursued; O4 is moot.
- **O2: the cumulative count is accepted,** with no run-length window. The largest allowance spent in one episode is gauged.
- **ADR-0084 Amendment 2 (accepted):** every verdict sends one last copy with the floor, and a forced verdict sends the floor alone; the sender takes `(next_expected, G_rx)` from the ack with the greatest `next_expected`, the larger `G_rx` on a tie. Consequences: C7's feed subtracts passed verdicts, not V; tests assert on `G_rx`, not `ev=shed`.
- **Bulk after a long outage:** late and out of order instead of a stall is acceptable once the release rule ships. This is the first Bulk default change (`rho-step-sites.md` §8).
- **ω** is measured on a TCP-in-tunnel test, and its result is followed (roadmap `meas-disorder-price`).

**Not decided:** O1; run-atomic spending; the time threshold T_ρ; the release arm; the new H; retiring `window_out_of_order`. These wait on the toy simulation (§6, stage 2 in §7) or the ω measurement. R1 is the staging point only. The receiver sites C9, C11 and C12 get the release arm once, with H_o(δ), after ω is measured.
