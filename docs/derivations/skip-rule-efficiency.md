# The give-up rule below ρ = 1: which holes, when, and where (derivation, no code)

**Question (owner).** Under ρ < 1 (Realtime declares ρ = 0.99) the machine may give up on a lost packet ("skip" it) so that in-order delivery does not stall. What is the correct give-up rule, and the one that costs the least latency and bandwidth? Re-derive T_cut, use a rule that is not time-based, or find the optimum by simulation?

**Short answer.** All three, each in its place:
- the count stays the guarantee (settled);
- a re-derived T_cut, in seconds and from measured repair times, decides *which* holes get the allowance when it is scarce;
- simulation ranks the candidates and bounds their gap to an offline optimum. It picks no constant.

Three findings change the design, whatever the simulation says:
- **(F1)** A sender-side skip as ADR-0084 writes it (shed = send nothing but the floor) is *slower* than simply resending, for every isolated hole (§3.1).
- **(F2)** Skipping part of a run of adjacent holes buys nothing and spends allowance (§3.3).
- **(F3)** The receiver is the place where a skip is fastest and where a run's extent is known, so the end state has the receiver spend the allowance and the sender supply it as a number (§4).

**Inputs read** (`github/main` `79d264aa`):
- ADR-0084 and its Amendment 1;
- `rho-step-sites.md` §1, §5 and §8;
- `evict-verdict-law.md` §1–§4 and §9;
- paper `docs/fec-arq-model.md` §3.3, §4.1, §4.10, §5.6, §7.3, §7.5 and §7.6;
- `raptorpath-math/src/lib.rs:1239–1270` (`p_recovered_within`, `find_t_cut`);
- `net/store_cap.rs:316` (`delta_price`) and `net/late.rs:64–101` (`W_COST_RATIO`, `request_bar`);
- `net/reorder.rs:60–92` (late fills);
- `net/mod.rs:236` (`GAP_ACK_MIN_INTERVAL` = 2 ms);
- ledger row C13.

**Words.**
- A *hole* is a source seq the receiver is missing while it holds later data, or a lost tail.
- A *run* is a maximal block of consecutive missing seqs with no received seq inside.
- *Resolution age* X is the time from a seq's original send to the moment it is filled, under the machine that never gives up.
- *Tell latency* L is the time from a give-up decision to the receiver passing the hole.
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

**Prediction.** With the last copy, consumption under i.i.d. loss is ε·ĥ, which is 0.0125 % at ε = 2.5 % and ĥ = 0.5 %. That is far below 1 − ρ = 1 %, so T_ρ = D at almost every i.i.d. point and the D clamp binds. T_ρ moves only under correlated loss (GE bad state, blackouts), where copies fail together. If the simulation shows no p99 gain there, T_ρ is dropped (pre-registered, §6).

**Is T_ρ continuous when Ŝ is flat?** No hole resolves before about one round trip, so Ŝ = 1 on [0, t_min). As (1 − ρ)/ĥ crosses 1, T_ρ steps from D to `t_min − L`. Verdicts are taken only at report ages, though, and none falls in that interval at N = 1. So the realised skip set does not step, and the aggregate behaviour is continuous. The simulation's continuity readout checks this (§6).

## 4. Where the decision is taken

### 4.1 Sender-only (ADR-0084 and R1)

The sender judges, sends copy + floor, and the receiver passes below the floor after H.
- **Latency of a skip:** report delay + owd + H ≈ RTT + owd + H after the loss (rho-step-sites §1.2). With the last copy, an isolated hole resolves at fill time ≈ RTT + owd, and at + H only when the copy is lost.
- **Promise:** exact by construction; one spender.
- **Wire:** v11 as approved (`WindowFloor{floor}`, `WindowAck.passed_undelivered`).
- **Weaknesses:**
  - it is never faster than the resend for a hole whose copy arrives;
  - it cannot see a run's end during a blackout (§3.3).

### 4.2 The receiver spends, the sender supplies the number

"The receiver is the one spender; the sender tells it the cumulative allowance it may spend."

```text
   sender → receiver:   Â  =  ⌊ (1 − ρ) · S_src ⌋                     cumulative, non-decreasing
   receiver passes run R  ⇔  [ t − t_detect(R) ≥ max(H, T_rx)   ∨   R ⊂ [A_rx, F_rx) ]   ∧   G_rx + n_R  ≤  Â_heard
   T_rx  =  Ŝ_rx⁻¹( ĝ / ĥ_rx )                                         ĝ = growth rate of Â heard,  ĥ_rx = holes per second
```

- **One ledger, no overshoot.** Every pass, local or below a floor, is checked at the receiver against the newest Â it heard. Â only grows, so a stale or lost Â is always lower than the true one, and stale is safe: `G_rx ≤ Â_heard ≤ (1 − ρ)·S_src` holds at every instant by construction. The "two deciders across one RTT" overshoot (settled) does not arise, because the receiver is the only one who spends. A sender verdict becomes a proposal: copy + floor, which the receiver passes only within Â.
- **The receiver reads no ρ.** It reads a count and its growth. T_rx is fully receiver-observable:
  - Ŝ_rx is the resolution time from detection to fill, which the receiver sees directly (the paper §7.6 argument: it holds the frontier and the lateness distribution; `LateGauge` already measures heal lateness);
  - ĝ/ĥ_rx is the same rate balance as §3.4, in seconds at the receiver.
- **The lost tail still needs the sender.** The receiver has no data above a lost tail (rho-step-sites §1.1). The sender's copy + floor is the proposal, and the receiver passes it within Â.
- **Store change.** A proposal can be refused when Â is exhausted, so the sender must keep a verdict seq until A_rx passes it, and resume serving if the floor is not acknowledged within one RTT + H of sending it. That amends ADR-0084 decision 2 ("an entry leaves on ack or on verdict").
- **Latency of a skip:** detection + H ≈ owd + H after the loss. That is about today's uncounted Realtime timer, now counted. It saves ≈ RTT − H (RTT/2 at Realtime) per skipped hole against §4.1.
- **Late fills.** The receiver's gap report has already triggered a resend, which arrives after the pass as a late fill. That fill is delivered (out of order, `reorder.rs:75–82`) and is subtracted from G_rx, so allowance is consumed only by holes that are never filled. Whether a packet delivered after its successors counts as delivered is owner question O3. If not, every pass consumes a unit (0.5 % at the example's loss, still inside 1 %).
- **Floor cadence.** Holes with data above pass on the receiver's own hold again, so evict-verdict-law question 6's premise returns. The floor-on-raise cadence that R1 added becomes unnecessary, and the sweep clock suffices.
- **C10.** With a ledger at the receiver, a full consumer channel *could* drop within Â instead of backpressuring. Default: keep the hold (rho-step-sites C10). That is owner choice O4.

**Wire.** One new field: the cumulative allowance Â, on `WindowFloor` and on a cadence of once per SRTT while it grew. The staleness bound is `(1 − ρ)·rate·SRTT` units, which is 1 unit in the example, with no new constant. The ack already carries `passed_undelivered` (G_rx).
- **It does not fit v11 as approved.** ADR-0084 decision 4 fixes v11's contents to `WindowFloor{floor}` + `passed_undelivered`.
- Main is still at `PROTOCOL_VERSION = 10` (`transport/protocol.rs:105`) and no `WindowFloor` exists in code, so the field *can* join v11 if a new ADR supersedes decision 4 before v11 ships.
- Otherwise it is v12. ADR-0078's receiver timestamp also wants a version after v11.
- The numbering is the coordinator's call.

### 4.3 Recommendation on "where"

**End state: §4.2.** It is the faster decider, the one that sees runs, and the one that measures fills directly, with one spender and no overshoot.

**Staging.** Build R1 (§4.1) first, as planned, with the last copy. It is the §4.2 machine with no local pass, and nothing in it is thrown away:
- the floor, `G_rx`, the store and the sender proposal are all reused;
- §4.2 then adds the Â field, the receiver's local pass and the store-until-ack change, behind the simulation of §6 and its own guard.

## 5. Continuity (NO-MODE-SWITCH)

Every formula above is one expression, with every term always computed:

| formula | in ρ | in δ | at ρ = 1 |
|---|---|---|---|
| `A = ⌊(1 − ρ)S_src⌋ − Ab` | linear (integer part on measured S_src, not at a preset) | none | 0: no verdict, no pass |
| copy with every verdict | none | none | no verdict exists |
| run-atomic `n_R ≤ A` | through A | none | A = 0 |
| `T_ρ = max(D, Ŝ⁻¹((1 − ρ)/ĥ) − L)` | continuous where Ŝ decreases; a step only across ages with no decision event (§3.4) | through D(δ) = b(δ)·RTprop and H(δ) | finite (t_max); inert because A = 0 |
| §4.2 receiver rule | through Â only | through H | Â = 0: no local pass, no floor pass (none sent) |

- **The nudges.** The ±2 % nudge of ρ = 0.99 is taken on 1 − ρ: ρ = 1 − 0.01·(1 ± 0.02) = 0.9898 and 0.9902. A +2 % nudge of ρ itself would leave the domain. At ρ = 1 the nudge is one-sided: ρ = 1 − ε with ε·S_src < 1 must give a byte-identical trace (rho-step-sites §3 clause 5), and the readouts must converge monotonically to the ρ = 1 values as ε → 0.
- **δ nudges** of ±2 % around Realtime move D and H continuously through `b(δ)`.
- **No branch reads a hint, δ or ρ.** The receiver reads neither ρ nor the sender's hint.

## 6. Pre-registered simulation plan (toy model)

**Status.** A Python discrete-event **toy model**, built later by a separate worker. It ranks candidates and bounds their gap to an oracle. It fixes no constant, and no number in it is provenance for the engine.

**Model.**
- Paths with fixed one-way delay and a bottleneck rate, one queue each.
- A paced source at 10 000 packets/s (512 B).
- A per-copy loss process per path.
- A sender with a store at every ρ, the gap report at a 2 ms sampler, the RFC 9002 hole law at N = 2, and a tail sweep at `clamp(2·SRTT)`.
- A receiver with a reorder buffer and the floor pass.
- No FEC: residual loss is set directly, and ĥ is the per-copy loss on the original.
- Common random numbers across candidates in each seed.

**Candidates:**
- **R0 (reference, not a bar):** today's receiver timer: pass at detect + SRTT/2, uncounted. It is expected to break the promise in the correlated cells, and that is reported.
- **S0:** serve until filled (the ρ = 1 machine). Upper reference for stall, lower for loss.
- **S1:** sender-only, ADR-0084 as written: `age > D`, per-seq first-come, no last copy.
- **S2:** S1 + last copy + run-atomic.
- **S3:** S2 + the measured age threshold T_ρ (§3.4), applied to a run's oldest seq, with the run-atomic count. The ratio `m_R/n_R` of §3.3 is used by the oracle only: an online ratio needs a per-run survival, which no candidate estimates.
- **RX:** §4.2, the receiver spends, with T_rx and run-atomic (exact at the receiver, which sees runs closed); sender proposals for tails.
- **O (oracle), offline:**
  - it sees every copy's arrival time under S0 with last copies;
  - it passes runs at the receiver's earliest possible instant (detect + H);
  - it chooses runs greedily by stall saved per unit under the prefix count, and reports the LP relaxation bound beside it;
  - in the i.i.d. cells, where runs are single holes, the greedy is exact (laminar matroid).

**Cells:**
- RTT 10 ms and 50 ms, N = 1 and N = 2 (owd 5 and 25 ms, skewed);
- 20 seeds;
- duration 60 s unless stated.

| cell | loss | allowance |
|---|---|---|
| I1 | i.i.d., ĥ = 0.5 %, ε = 2.5 % | slack |
| I2 | i.i.d., ĥ = 3 %, ε = 10 % | binds without the last copy |
| G | Gilbert–Elliott, mean burst 8, bad-state loss 0.5, mean 2.5 % | binds in bursts |
| B10, B60 | one 200 ms blackout after 10 s and after 60 s of clean run (N = 1; at N = 2 on one path) | bank < run, bank > run |
| T | lost tail: the last 346 seqs dead (T1's shape) | tail only |

Dial points: ρ ∈ {1, 1 − ε (ε·S_src < 1), 0.999, 0.99, 0.9898, 0.9902}; δ at Realtime and ±2 %; a Bulk ρ = 1 control.

**Readouts, witnesses first:**
- verdicts, passes and refusals by decider (each candidate's mechanism executed);
- G_rx / S_src against 1 − ρ, at every instant;
- per-packet HOL delay p50 and p99, and frontier-stall p50 and p99 per episode;
- repair sends and control datagrams per delivered packet;
- J (§1);
- the bind fractions (D, t_max, `n_R ≤ A`);
- the largest single-episode spend.

**Pass criteria, fixed now.** These are decision bars, not law constants.
- **P1, the promise (hard).** For S1, S2, S3 and RX, in every cell, seed and instant: `G_rx(t) ≤ ⌊(1 − ρ)S_src(t)⌋`. One violation fails the candidate.
- **P2, the ρ = 1 identity.** At ρ = 1 and at 1 − ε, every candidate's event trace equals S0's.
- **P3, continuity.** At each nudge pair, every readout's preset value lies within the 95 % bootstrap CI of the mean of its two nudges.
- **P4, the derivation's predictions.** The derivation is refuted where these miss.
  - **a.** I1: S1's isolated-hole stall exceeds S0's by H ± 1 ms (F1).
  - **b.** I1: S2's p50 equals S0's within 1 ms, and S2 consumes ≤ 1.5·ε·ĥ of source.
  - **c.** I1 at 10 ms: RX's p50 is at least 0.4·RTT below S2's (predicted ≈ RTT − H = 0.5·RTT).
  - **d.** B10 at N = 2 (the live path reports, so runs are closed): S1 spends > 0 and its stall gain over S0 is within the CI of 0 (waste), while S2 spends 0. B10 at N = 1 (open run): S2 drains the bank like S1 (§3.3, predicted), and RX spends 0.
- **P5, gap to the oracle.** `gap = (J_cand − J_O)/(J_S0 − J_O)`, per cell.
  - S3 is adopted over S2 only if, in G and B60, p99 stall falls by more than the CI and no cell worsens p50 or p99 by more than the CI. Otherwise T_ρ is dropped (simpler wins).
  - RX is recommended for the v-next wire if its gap is ≤ 0.25 in I1, G and B60 and it passes P1.

## 7. What this blocks

From the commit-order table of `rho-step-sites.md` §5:

**Can proceed now under ADR-0084 (all sender-side, or neutral):**
- brief steps 1–8: store at every ρ, the rate bound, the verdict with the `(1 − ρ)·S_src` count, v11 and the floor;
- C2 (SACK release) and C5 (batching);
- C1 (placement), C7 (Copa feed), C6 (owner answer pending, unrelated) and C8 (after C9's floor pass).

**C3 and C4** depend only on C12's **refresh** term, not on its expiry or marker term. C12 can split into "refresh at every ρ" (lands now) and "marker term" (R1). C3 and C4 then proceed after the refresh half.

**Two amendments, proposed for step 7 before it ships** (ADR-0084 decision 1 is accepted, so a superseding note is needed):
- the last copy with every verdict (F1);
- run-atomic spending (F2).

Both are sender-side and change no wire. Without them, step 7 ships a Realtime rule that is slower than serving for isolated holes and wastes the bank on partial runs. T_ρ (§3.4) waits for the simulation (P5) and lands later, on its own guard.

**Receiver side:**
- C9, C11 and C12's marker term implement R1, which is the staging point of §4.3. They **can proceed as R1**.
- **Waits for this derivation, the simulation and a wire decision:** only the §4.2 additions:
  - the Â field (a new ADR superseding v11's contents, or v12);
  - the receiver's local pass within Â;
  - the store-until-ack change to ADR-0084 decision 2;
  - the floor-cadence reversion.
- **C10** is independent: hold at every ρ, unless the owner takes O4.

**Paper first (FORMULA-FIRST).** Before any code of this lands, paper §5.6 owes:
- the last-copy sentence;
- the run-atomic spend;
- T_ρ with its provenance;
- under §4.2, the receiver rule.

Ledger C13 gets a pointer: the time-domain T_ρ is its replacement, and the model's `find_t_cut` stays PINNED debt until the model moves.

## 8. For the owner

**Recommendation.** Keep the count and its cumulative horizon. Change *how* the allowance is spent, in three steps:
1. **Now, sender-side, no wire change.**
   - Every give-up still sends one last copy of the packet with the "skip it" message.
   - Allowance is spent only on a whole run of missing packets, never on part of one.
2. **After the simulation.** Among holes, spend the allowance on those still unfilled after a measured time T_ρ (the re-derived T_cut, in seconds), but only if the simulation shows it pays under bursts.
3. **Next protocol version.** Let the receiver spend the allowance itself: the sender sends one number, "you may skip up to Â in total", and the receiver skips locally within it.

**Worked example (estimated, not measured).** One path, 10 ms round trip, 10 000 packets/s, Realtime (ρ = 0.99, D = 5 ms, receiver hold H = 5 ms), 0.5 % of packets lost after FEC and 2.5 % of resends lost. Times are from the lost packet's send.

| rule | typical hole resolved | when the resend is also lost | allowance used | counted? |
|---|---|---|---|---|
| today's Realtime receiver timer | ≈ 10 ms (skipped) | ≈ 10 ms | none recorded | **no**: breaks the promise |
| never skip (ρ = 1 behaviour) | ≈ 16 ms (resend) | ≈ 40 ms (next refresh) | 0 | yes |
| ADR-0084 as written (skip, no copy) | ≈ 21 ms (skip) | ≈ 21 ms | 0.5 % of packets | yes |
| + last copy, run-atomic (step 1) | ≈ 16 ms (resend) | ≈ 21 ms (skip) | ≈ 0.01 % | yes |
| receiver spends (step 3) | ≈ 10 ms (skip; the late resend still arrives) | ≈ 10 ms | ≈ 0.01 % (0.5 % if a late packet does not count, O3) | yes |

Notes on the table:
- **Bandwidth:** every row except "ADR-0084 as written" sends about one resend per lost packet (≈ 0.5 % extra). The skip messages add about 50–100 small control datagrams per second. Priced by δ, 5 ms of stall per hole costs about 25 resends' worth, so latency dominates at Realtime.
- **Bursts.** For a 200 ms outage (2 000 packets lost in a row) after 10 s of clean stream (1 000 units banked), today's first-come rule spends the whole bank and the stream still waits. The run rule spends nothing and keeps the bank, but only where the run is closed: a burst that ends, or an outage on one of two paths. On a single path the sender cannot see the end of an outage and still drains the bank piecemeal. Only the receiver design (step 3) sees the run whole. After 60 s (6 000 banked) the receiver skips the whole outage, which saves roughly the resend time of 2 000 packets.

**Choices that are yours:**
- **O1.** Is a skipped packet free inside the allowance? Default **yes**: the triangle has no loss price, and a price would be a new dial.
- **O2.** Should the promise also bound the length of a loss run (a window)? Default **no**. The only window derivable from ρ alone forbids skipping any two adjacent packets; any other window length needs a declared number from you.
- **O3.** Does a packet that arrives after its successors were delivered count as delivered for ρ? Default **yes**, as the engine already delivers it (`reorder.rs`).
- **O4.** With the receiver holding the allowance (step 3), should a full consumer drop within it instead of backpressuring? Default **no**: keep the hold.
- **The wire.** Put Â into v11 before v11 ships (a new ADR superseding ADR-0084 decision 4) or into v12 (coordinator's numbering).
- **The amendments.** Accept the last-copy and run-atomic amendments to ADR-0084 decision 1 before the verdict step ships. These, the store-until-ack change to decision 2 and the Â field against decision 4 are proposals; each needs a superseding note, because the ADR is accepted.

**Open, not owner calls:**
- the sender cannot see a run's end during a blackout at N = 1 (§3.3). Only the receiver design fixes it;
- D is predicted degenerate at Realtime N = 1 (§3.1) and is owed a ledger verdict after its gauge is read;
- C7's feed must subtract passed verdicts, not V, once verdicts carry a copy (§3.1);
- the H kind stays UNDERIVED (rho-step-sites C11);
- T_ρ's flat-survival step is argued harmless at N = 1 and must be checked at N = 2 (P3).
