# The give-up rule below ρ = 1: which holes, when, and where (derivation, no code)

**Question (owner).** Under ρ < 1 (Realtime declares ρ = 0.99) the machine may give up on a lost packet ("skip" it) so that in-order delivery does not stall. What is the correct give-up rule, and the one that costs the least latency and bandwidth? Re-derive T_cut, use a rule that is not time-based, or find the optimum by simulation?

**Short answer.** All three, each in its place:
- the count stays the guarantee (settled);
- a re-derived T_cut, in seconds and from measured repair times, decides *which* holes get the allowance when it is scarce;
- simulation ranks the candidates and bounds their gap to an offline optimum. It picks no constant.

Three findings change the design, whatever the simulation says:
- **(F1)** A sender-side skip as ADR-0084 writes it (shed = send nothing but the floor) is *slower* than simply resending, for every isolated hole (§3.1).
- **(F2)** Skipping part of a run of adjacent holes buys nothing and spends allowance (§3.3).
- **(F3)** Releasing order and losing a packet are two separate acts. Only stopping retries loses a packet. If a late packet delivered out of order counts as delivered (owner question **O3**, the deciding one), the receiver can release order on its own hold, uncounted and with no new wire field, while the sender's counted verdict stays the only loss (§4.1). If it does not count, the receiver has to spend counted allowance, and that needs one wire field (§4.3).

**H carries the headline numbers and is UNDERIVED** (rho-step-sites C11). F1's penalty is H, the receiver's saving is RTT − H, and the oracle passes at detect + H. Every time below is stated "if H = 5 ms" (SRTT/2 at a 10 ms RTT), and the simulation sweeps H (§6).

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

**A skip has no price inside the count.** The triangle has no price for a lost packet. ρ only bounds how many may be lost. So inside the allowance a skip is free, and the optimum spends the allowance wherever it saves the most priced stall. A loss price would be a new dial. That is owner choice O1 (§8).

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

The copy and the floor travel the same path, and the floor then waits H at the receiver. So (b) resolves the hole H later than (a), for every isolated hole whose resend arrives. (b) also spends one unit of allowance per hole, and saves one resend.

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
- **Latency.** The consumer's stall ends at `detect + H_o`, which is today's Realtime timer if H_o = SRTT/2. Every hole keeps being served until its fill or its verdict.
- **Allowance used:** only holes whose last copy also fails, about ε·ĥ (§3.4).
- **Bandwidth:** as serving, one copy per round until fill or verdict.
- **What U still needs: an ordering law H_o(δ), owed and not derived here.** Release is ordering, a δ question that does not involve ρ. Today's arms are SRTT/2 below ρ = 1 and never at ρ = 1. One δ-continuous H_o must reproduce "never" (or a hold long enough that Bulk TCP-in-tunnel sees no reorder) at Bulk and Auto, and a short hold at Realtime, with no threshold on δ. `min(b(δ), 2)·SRTT` (C11) is **not** enough: it gives 2·SRTT at Bulk, which changes the ρ = 1 Bulk default.
  - Until H_o is derived, U cannot ship.
  - The derivation is the receiver-side analogue of paper §7.6's request law: a level set of the δ-priced stall against the price of disorder. The price of disorder is part of O3.
- **Changes against rho-step-sites:** C9 and C11 keep one release arm, keyed by H_o(δ) and never by ρ, instead of deleting the local give-up. C12's marker term is unchanged.

### 4.2 Sender-only, no release (R1, the staging point under both branches)

The sender judges (with the last copy), and the receiver releases and passes only below the floor after H.
- Latency: fill time ≈ RTT + owd for an isolated hole, and + H when the last copy is lost.
- The promise is exact, with one spender. Wire: v11 as approved.
- R1 is U with H_o = ∞, so it is the staging point when O3 = yes.

### 4.3 Branch O3 = no: the receiver spends counted allowance (design RX)

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
| U (O3 = yes) | detect + H_o ≈ 10 ms | only failed last copies | ≈ ε·ĥ | none | H_o(δ) derivation |
| RX (O3 = no) | detect + H ≈ 10 ms | every pass | ≈ ĥ | Â | sim (P5), ADR on the wire |
| R1 + last copy | fill ≈ 16 ms; + H if the copy fails | only failed last copies | ≈ ε·ĥ | none | nothing (staging) |
| ADR-0084 as written | ≈ 21 ms | every verdict | ≈ ĥ | none | (dominated: F1) |

- **Under O3 = yes, U dominates RX.** It has the same release latency, loses fewer packets (retries continue until the sender's verdict) and needs no wire field.
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

## 6. Pre-registered simulation plan (toy model)

**Status.** A Python discrete-event **toy model**, built later by a separate worker. It ranks candidates and bounds their gap to an LP lower bound. It fixes no constant, and no number in it is provenance for the engine.

**Model.**
- Paths with fixed one-way delay and a bottleneck rate above the source rate, so copy arrival times do not depend on the policy up to a pass. Cells that saturate the link are labelled "bound approximate".
- A paced source at 10 000 packets/s (512 B).
- A per-copy loss process per path.
- A sender with a store, a 2 ms gap-report sampler, the RFC 9002 hole law at N = 2, and a tail sweep at `clamp(2·SRTT)`.
- A receiver with a reorder buffer, the floor pass and, for U, two frontiers.
- No FEC: the residual loss is set directly.
- Common random numbers across candidates in each seed.

**H is swept, because it is UNDERIVED:** H (and H_o for U) ∈ {0, 2.5, 5, 10 ms, 0.5·SRTT, 1·SRTT}. Every headline result is reported per H value, never at a chosen one.

**Candidates:**
- **R0 (reference, not a bar):** today's receiver timer. It passes at detect + SRTT/2 and its ack advances, so it is uncounted.
- **S0:** serve until filled.
- **S1:** ADR-0084 as written: `age > D`, per-seq first-come, no copy.
- **S1c:** S1 + last copy.
- **S2:** S1c + run-atomic.
- **S3:** S2 + the online T_ρ, with Ŝ estimated by Kaplan–Meier under S3's own verdicts (§3.4), applied to a run's oldest seq.
- **U:** R1 sender (S1c, or S2 if run-atomic survives P5), plus the receiver release at detect + H_o with the ack held.
- **RX:** §4.3, with T_rx and run-atomic.

**Oracle: an exact LP lower bound, not a greedy.**

Greedy is exact only when stalls do not overlap, and they overlap even in I1. Holes arrive at `λ = 10 000 × 0.5 % = 50 /s`, and a stall lasts `s ≈ 11 ms` (detect at 5 ms, fill at 16 ms). For Poisson holes:
- the fraction whose stall starts inside the previous one's is `1 − e^(−λs) = 1 − e^(−0.55) ≈ 42 %`;
- the fraction that overlap some other stall is `1 − e^(−2λs) ≈ 67 %`.

The LP. On each sample path, take every packet j's arrival `a_j` and every hole s's fill time `X_s` under S0 with last copies, and its earliest pass instant `p_s = t_detect(s) + H`:

```text
   minimise    Σ_{j ∈ P} ( y_j − a_j )                                   per-packet head-of-line delay, packet·seconds
   subject to  y_j ≥ a_j                         for every received j ∈ P
               y_j ≥ y_{j−1}                     for consecutive j ∈ P   (in-order delivery)
               y_s ≥ X_s − (X_s − p_s)⁺ · x_s    for every hole s        (a pass cuts s's resolution to p_s)
               Σ_{s : p_s ≤ t_k}  c_s · x_s  ≤  ⌊(1 − ρ)·S_src(t_k)⌋     at every pass instant t_k  (the prefix count)
               0 ≤ x_s ≤ 1
   c_s  =  1{X_s > p_s}   (O3 = yes: a pass that a copy beats costs nothing)        c_s = 1   (O3 = no)
```

- P is the set of packets whose arrival falls before the latest fill of an unresolved hole below them. Every other packet has `y_j = a_j` under any policy, so restricting to P is exact.
- Lost and late packets are excluded from the sum and reported separately.
- x is per seq, a superset of run-atomic and every candidate's choices, so the LP value is at or below the optimum of every passing policy on that path.
- Solve with HiGHS (`scipy.optimize.linprog`). On 3 seeds per cell, also solve the MILP (`scipy.optimize.milp`, x binary) and report the integrality gap.
- For the LP the cell duration is 10 s: about 500 holes and on the order of 10⁵ variables.
- **Scoring:** `gap = (HOL_cand − HOL_LP) / (HOL_S0 − HOL_LP)`, scored against the LP bound, never against a greedy. N_rep is reported beside it, unpriced.

**Cells:**
- RTT 10 ms and 50 ms; N = 1, and N = 2 with owd 5 and 25 ms;
- 20 seeds; 60 s (10 s for the LP).

| cell | loss | allowance |
|---|---|---|
| I1 | i.i.d., ĥ = 0.5 %, ε = 2.5 % | slack |
| I2 | i.i.d., ĥ = 3 %, ε = 10 % | binds without the last copy |
| G | Gilbert–Elliott, mean burst 8, bad-state loss 0.5, mean 2.5 % | binds in bursts |
| B10, B60 | one 200 ms blackout after 10 s and after 60 s clean (N = 1; at N = 2 on one path) | bank < run, bank > run |
| T | lost tail: the last 346 seqs dead | tail only |

Dial points: ρ ∈ {1, 1 − ε (ε·S_src < 1), 0.999, 0.99, 0.9898, 0.9902}; δ at Realtime ±2 %; a Bulk ρ = 1 control.

**Readouts, witnesses first:**
- verdicts, passes, releases and refusals by decider;
- G_rx / S_src against 1 − ρ at every instant;
- consumer HOL delay p50 and p99, frontier-stall p50 and p99, and late-delivered and lost counts;
- repair sends and control datagrams per delivered packet;
- the bind fractions (D, t_max, `n_R ≤ A`) and the largest single-episode spend;
- for S3: Ŝ against S0's true survival.

**Pass criteria, fixed now (decision bars, not law constants):**
- **P1, the promise (hard).** For every counted candidate (S1, S1c, S2, S3, U, RX), in every cell, seed and instant, `G_rx(t) ≤ ⌊(1 − ρ)S_src(t)⌋`. For U, also: no released hole is ever pruned at the sender without a verdict.
- **P2, the ρ = 1 identity.** At ρ = 1 and 1 − ε every candidate's sender trace equals S0's. U's receiver may release at the Realtime δ (§5), with zero loss.
- **P3, continuity.** At each nudge pair, every readout's preset value lies within the 95 % bootstrap CI of the mean of its two nudges.
- **P4, implementation checks of the toy.** These verify that the toy implements the mechanisms as specified. They are **not** evidence for the derivation. If one misses, the toy is debugged before any ranking is read.
  - **a.** I1: S1's isolated-hole stall exceeds S0's by H ± 1 ms.
  - **b.** I1: S1c's p50 equals S0's within 1 ms; S1c consumes ≤ 1.5·ε·ĥ.
  - **c.** I1: U's and RX's p50 sit ≈ RTT − H below S1c's.
  - **d.** B10 at N = 2: S1 spends > 0 for a gain within the CI of 0; S2 spends 0. B10 at N = 1: S2 drains like S1; RX spends 0.
- **P5, decisions:**
  - **Run-atomic** (S2 against S1c) is kept only if it improves HOL p99, or the largest-episode waste, by more than the CI in some cell, and worsens none by more than the CI. With a copy, only unfilled packets spend, so this may well fail. Then run-atomic is dropped.
  - **T_ρ** (S3 against S2, or against S1c if run-atomic is dropped) is kept only if p99 falls by more than the CI in G or B60 and no cell worsens by more than the CI.
  - **U and RX** are each reported per O3 branch. RX is recommended for a wire field only if O3 = no, its LP gap is ≤ 0.25 in I1, G and B60, and it passes P1.

## 7. What this blocks

From the commit-order table of `rho-step-sites.md` §5:

**Can proceed now under ADR-0084 (sender-side, or neutral):**
- brief steps 1–8, with **v11 exactly as approved**;
- C2, C5, C1, C7 (with §3.1's correction: subtract passed verdicts, not V) and C8 (after C9's floor pass).

**C3 and C4** need only C12's refresh term. Split C12 into "refresh at every ρ" (now) and the marker term (R1); C3 and C4 follow the refresh half.

**Proposed for step 7 before it ships** (each needs a superseding note, because ADR-0084 decision 1 is accepted):
- the last copy with every verdict (F1);
- the `(next_expected, G_rx)` pair-merge rule (§3.1).

Run-atomic and T_ρ wait for P5.

**Receiver side:**
- C9, C11 and C12's marker term **can proceed as R1**, the staging point of both branches.
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

## 8. For the owner

**The deciding question (O3).** When a lost packet's repair arrives after the packets behind it were already handed on, the engine delivers it anyway, out of order. That is what happens today (`reorder.rs`). **Does that late packet count as delivered for your 99 %?**
- **Yes:** the receiver can stop waiting on its own hold, as today's Realtime does, and nothing is lost by that. The only real losses are the sender's counted give-ups. This needs no new protocol field, but it does need one more derivation: how long to hold before releasing order, as a function of δ (the Bulk end must keep today's strict order).
- **No:** a release is a loss, so the receiver may release only within an allowance the sender sends it, as one new protocol field in a later version. Until then the receiver waits for the sender's give-up, about 5 ms slower per hole than today at a 10 ms round trip.

**In both branches, now.** Keep v11 as approved. Every give-up sends one last copy of the packet (F1). The sender merges the receiver's loss count safely across reordered acks.

**Worked example, estimated, if H = H_o = 5 ms.** One path, 10 ms round trip, 10 000 packets/s, Realtime ρ = 0.99, 0.5 % lost after FEC, 2.5 % of resends lost; times from the lost packet's send. H is UNDERIVED, and the simulation reports every row at H = 0, 2.5, 5 and 10 ms.

| rule | typical hole | resend also lost | allowance used | counted? |
|---|---|---|---|---|
| today's Realtime timer | ≈ 10 ms | ≈ 10 ms (packet lost) | none recorded | **no** |
| never skip (ρ = 1) | ≈ 16 ms | ≈ 40 ms | 0 | yes |
| ADR-0084 as written | ≈ 21 ms (= 16 + H) | ≈ 21 ms | ≈ 0.5 % | yes |
| + last copy (R1, now) | ≈ 16 ms | ≈ 21 ms | ≈ 0.01 % | yes |
| U, if O3 = yes | ≈ 10 ms (= 5 + H_o) | ≈ 10 ms, then sender give-up | ≈ 0.01 % | yes |
| RX, if O3 = no | ≈ 10 ms (= 5 + H) | ≈ 10 ms | ≈ 0.5 % | yes |

Bandwidth is about one resend per lost packet in every row except "ADR-0084 as written". The control messages add 50–100 small datagrams per second.

**Other choices that are yours:**
- **O1.** Is a skipped packet free inside the allowance? Default **yes**: the triangle has no loss price.
- **O2.** After a long clean stretch, one outage may skip about 2 000 consecutive packets (0.2 s) and still keep the 1 % promise, and repeated outages may each do so while the bank lasts. **Is that acceptable?** If not, the promise needs a window length, a number only you can declare, because the one window derivable from ρ alone forbids skipping any two adjacent packets.
- **O4.** Only under RX: may a full consumer drop within the allowance instead of backpressuring? Default **no**.
- **The ADR-0084 amendments.** The last copy, the pair-merge rule, and later the decision-2 store change (RX only) and any wire field are proposals against an accepted ADR.

**Open, not owner calls:**
- H, and under U H_o(δ), are UNDERIVED. Every headline number depends on them;
- the sender cannot see a run's end during a one-path outage (§3.3);
- D is predicted degenerate at Realtime N = 1 and is owed a ledger verdict (§3.1);
- C7's feed correction (§3.1).
