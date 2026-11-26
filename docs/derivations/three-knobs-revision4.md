# Three knobs, Revision 4: a sender-side wait limit under sustained overload, and the engine's repair clamp (derivation, no runs)

**Status.** Design and derivation only. Nothing here has been run, and the offline simulation is unchanged.
- **What it amends.** `three-knobs.md` §13.2 (the dequeue rule). It withdraws that section's paragraph "What cannot be fixed: delay pinned near T − R", and Q12's default ("no loss price").
- **What it adds to the model of every design** (§3.3): the engine's spare-capacity clamp on repair. It also adds the engine's send gate to the designs that pause in the engine, and a second arm for today's Realtime that models the engine's real send queue, settled by an engine read (Q30).
- **What is unchanged:** L1, L2 (§2.1, and §2.2 as revised in §17.1), the protected floor (§17.1), the brake and the store pause.

**Review.**
- **First draft** (`2c8ced19`, `342f907e`): the advisor (Fable) reviewed it before it was written and again on the draft.
- **This revision** answers the coordinator's review of `342f907e`. Its six items are:
  1. repair is the other half of the gap;
  2. μ̂ in first-send units, and CL-long's drain;
  3. the floor of τ, and how a collapsed lifetime sweep is read;
  4. δ's scale;
  5. the sizing run;
  6. a gauge-first step.
- **The advisor's review of `ea56139c`** changed:
  - the law's shape: expiry is now monotone in age (§2, §3.1);
  - μ̂: it is now measured by the sender from its own reports, with no oracle (§3.2);
  - the clamp: it gets a bind gauge per cell and an explicit spare in the cells with no store pause, and every repair-on row re-runs, DECIDE included (§3.3);
  - the reading registered for the fresh cells (§4.3).
- **An independent engine read** settled Q30 and added today's Realtime engine-queue arm (§3.3).

**Inputs read.**
- `docs/derivations/three-knobs.md` on `github/main` (`c326f489`): §1–§10, Revisions 1–3 (§13, §15, §17), the findings §18 (with its amendment) and §19.
- `docs/derivations/three-knobs-stage3e-registration.md` (main): §2.5, §2.6, §4, §5, and the K-OV row of §13.
- Branch `toy-stage3e` (`e434c44c`):
  - `tools/toy/results-stage3e.md`: the OV reading of 2026-11-26 with its delay table, the CL-long DECIDE reading, and TIME;
  - `tools/toy/toy2.py`: `emit`, `dequeue_expired`, `R_now`, `o_fwd`, `fwd`, `stage_repair`, `rate_hat`.
- Paper `docs/fec-arq-model.md`:
  - §1's dial table (δ is "the marginal cost of delay, in Copa's utility `U = log(throughput) − δ·log(delay)`", on (0, ∞));
  - §4.1, §8.2 (1/δ packets), §8.3 (the BtlBw anchor, whose sampler has no app-limited flag) and §8.7.
- Engine:
  - `net/emit_source.rs:573–597` (the repair rate is `min(window_rate, spare)`);
  - `control/fec_rate.rs:729` (the taper budget's grant `.min(spare.max(0.0))`);
  - `scheduler/path.rs:458–472` (`available() = cwnd − in_flight`; `spare_capacity() = (cwnd − in_flight)/in_flight`, with +∞ at in_flight = 0);
  - `scheduler/mod.rs:229` (`active_paths()` keeps only paths with `available() > 0`).

## Short answer

**The finding, re-read from the stored runs.** In OV (offered λ = 12 000 units/s, link C = 10 000/s, a 1 250-datagram buffer, one path, repair RL), the stage 3e new-model rows fit one line in the intake hold `w = T − R`:

```text
   delivered(T)  ≈  0.772  +  0.07 · w          (T = 0.4, 0.6, 1, 2 give 0.7929, 0.8068, 0.8348, 0.9045)
```

- **The intercept is the steady share** `C/((1 + r)·λ)`, with r ≈ 0.087.
- **The slope term is the backlog flushed** when hand-over stops.
- **Every delivered unit waits w,** so p50 = w + 0.093 s, and within 0.4 s the share is 0.1175 for every T ≥ 0.6.

**The gap to today's Realtime (0.8165 / 0.814 / 0.172 s) has two halves.** Revision 4 takes both.
- **Delay: the intake's standing queue.** Fixed by the wait limit (§2, §3.1).
- **Share: repair that displaces source under saturation.**
  - **Measured** (stored runs plus two count-only gauges, `toy-stage3e` `fd77697a`; §3.4). Today's Realtime arm sends 10 507 repairs, and its own link queue drops 96 % of them. Of the 138 units it decodes, none were first sends the queue had dropped. So its share is set by the link buffer passing first sends: `(N − 19 187 queue-dropped first sends)·(1 − ε)/N` ≈ 0.815.
  - The new model's repair is effective: it decodes 2 821 of 2 826 lost first sends. But it takes 8.0 % of link sends, one for one from first sends, and 5 254 of its repairs are redundant.
  - The new model drops before a unit has a seq, so any r above ε/(1 − ε) displaces source.
  - The fix is the engine's own spare-capacity clamp, `r_eff = min(r, spare)`, applied to every design. The offline simulation lacks it because it has no cwnd (§3.3).

**The revised law.** In the dequeue rule, T becomes τ. τ is the same for every head, so expiry is monotone in age:

```text
   expire the head u   ⇔   a(u) + R  ≥  τ   ∧   a younger unit is waiting
   τ   =  T  −  δ · (λ̂/μ̂ − 1)⁺ · t_ov²  /  ( T − R + ô )
   μ̂   =  D̂ ,   the sender's own windowed-max rate of source datagrams the receiver reports newly received
```

**This head (2026-11-26, after the advisor's review of `ea56139c`) changes the law in one place.** The relative delay saving is now charged against `T − R + ô`, the delay §13.2 gives when the overload persists. Before, it was charged against `a(u) + ô`, the head's own delay. The old form made the expiry test `a + R + K/(a + ô) ≥ T` non-monotone in a: it falls in a until `a* = √K − ô`, where it takes its minimum `2√K − ô + R`, and rises after. So once √K > ô, a fresh head expired while a middle-aged one was held. That disagrees with the sentence the rule implements, §13.2's "give the oldest unit's slot to a younger one" (§2).

μ̂ is now a measurement the sender makes from its own reports (§3.2). The earlier form, built on a wire capacity Ĉ, looked like it could read the cell's true link rate.

- **Where it comes from.** It is derived from the paper's own utility `U = log(throughput) − δ·log(delay)`, applied to the bet "dropping the oldest unit now will be refunded by a drop the overload would force anyway". Gott's scale-free forecast (1993) gives both the odds and the horizon. The first draft's raw 1/δ odds bar is withdrawn (§2).
- **λ̂** counts hand-overs.
- **μ̂** is capacity for first sends, in the units λ̂ counts.
  - At the steady resend share it equals the delivered source rate, whatever the repair rate (§3.2).
  - Resends of the outage's one-off debt are source datagrams too, so they do not lower it.
  - CL-long's drain reads about 12 100/s with the clamp, a 21 % margin, and about 11 000/s without it, a 9.8 % margin, against 10 000 offered.
  - **A sustained under-read beyond the margin is the main risk** (§3.2).
- **ô** is §17.1's one-way term.
- **t_ov** is how long λ̂ has exceeded μ̂.
- **Inert at T = ∞:** ∞ minus a finite term.
- **Identical to Revision 3 wherever λ̂ ≤ μ̂:** the subtracted term is exactly 0.0.
- **Continuous** in δ, T and the rates.
- **The floor of τ** is reached when every unit but the newest expires, at `K ≥ (T − R)(T − R + ô)`. It gets a bind gauge, `floor_bind` (§3.1).

**Predicted for OV at T = 0.4** (delivered / on time within 0.4 s / delay p50; derived in §4.1):

| design | prediction |
|---|---|
| Revision 3 (stored) | 0.793 / 0.793 / 0.390 s |
| the clamp alone | 0.837 / 0.837 / 0.39 s |
| the wait limit alone | 0.770 / 0.770 / 0.095 s |
| Revision 4, both | 0.79–0.81 / 0.79–0.80 / 0.095–0.10 s |
| today's Realtime, the arm as stored (feeds the link's 125 ms buffer directly) | 0.8165 / 0.814 / 0.172 s |
| today's Realtime, engine queue (new arm, §3.3) | 0.84–0.88 / 0.20–0.25 / 0.65–0.70 s |

**The stored arm flatters the engine.** In the engine a Realtime first send is sent at once, with no source pause. It then waits in quinn's 4 MiB datagram queue, about 8 192 symbols, which evicts the oldest datagram when full and has no deadline. The new engine-queue arm models that, and both arms are run and reported (Fairness rule 4).

**OV cannot confirm the law.**
- OV is post-data.
- At δ = 50 the intake collapses within 0.1–0.55 s of the overload being measured, so τ sits on its floor and every finite T gives one point.
- A collapsed sweep is read as a single point by dominance, never interpolated (§3.1).
- **The cells that can test it:**
  - OV-end (an overload that ends), which prices the bet;
  - OV-crit (load at capacity), the knife edge;
  - OV at δ = 0.5, where the collapse comes 1.0, 2.7 and 5.5 s after the overload is measured at T = 0.4, 1 and 2, so the shape is visible;
  - K-OV after a time-limit amendment.

**δ's scale is open, stated honestly (Q24).**
- The bar comes from U, with δ as the paper defines it: dimensionless, the marginal cost of a relative delay.
- **The one step without provenance** is that U's throughput term is read over the forecast horizon t_ov. That is Gott's median: cited, but a choice. Nothing ships on it until Q24 is answered.
- OV-end runs δ and δ/2 at T = 2 (δ = 0.5 and 0.25). The reserve held when the overload ends is predicted short of Revision 3's by 1.57 s and 0.79 s: linear in δ (§4.3).

**Before any run of the law:** a gauge-first step (§5.3).
- It computes λ̂, μ̂, t_ov and τ as readouts only, on seed 0 of every stored new-model configuration, including μ̂ biased by −2, −5 and −10 % in CL-long's drain.
- A non-zero count of ticks with τ < T in a cell with headroom sends the law back to derivation.
- Predicted: 0 everywhere with headroom. The exception is CL-long under RL at −10 %, which exceeds its 9.8 % margin and is predicted non-zero, and so marks the edge.

**Candidates (a), (b) and (c)** are evaluated in §8 and rejected.

## 1. What the requirements force

Requirement 1, read with "τ small at zero headroom", restricts τ's form before any candidate is considered:
- **Forms that fail at T = ∞.** `min(T, x)`, `1/(1/T + 1/x)` and `(1 − h)·x + h·T`, each with x independent of T, give a finite τ at T = ∞ whenever x is finite. That means drops under overload at T = ∞, so requirement 1 fails.
- **Products need a factor bounded away from 0.** `T·φ` is inert only if φ > 0 by construction. If φ can reach 0, then ∞·0 is NaN.
- **The subtractive form is safe.** `T − y`, with y finite by construction, gives τ = ∞ at T = ∞ for any y ≥ 0. That needs δ < ∞, t_ov < ∞ and μ̂ > 0, each of which §3 secures.
- **A horizon proportional to T fails.** A forecast that compares the queue's growth over a horizon proportional to T with a deadline proportional to T reduces to a ratio test at T = ∞. For example, `c = T − R − (x − 1)·T` is −∞ at T = ∞ for x > 2, and 0·∞ at x = 2. So the forecast horizon must be measured, not T.
- **An outage must not read as overload.** With an instantaneous drain rate, an outage is overload with drain 0.
  - At the first dequeue after CL-long's store pause releases, such a rule would collapse the intake. At T = 0.4 it would lose the λ·(T − R) ≈ 3 850 units that currently survive, about 1.9 points of CL-long's 94.1 %, which is the order of the leads CL-long's PASS rests on. At T = 2 it would lose the whole 15 000-unit backlog.
  - So the drain side must be a **capacity** estimate, held through a pause, and never the current send rate.
- **Inputs that τ does not move.** τ should not read the intake length, which τ itself moves (a feedback loop). The hand-over rate and the capacity are set by the application and the path.

**Where the standing queue buys nothing.** Under saturation the link stays busy without any intake backlog. The store pause keeps about `B − λ·RTprop` ≈ 880 datagrams in the link buffer: srtt 98 ms against a 10 ms RTprop. Dropping a unit in the intake now instead of at T − R therefore changes which units are sent, not how many. The one exception is the reserve flushed when the overload ends, and that is what §2 prices.


## 2. The derivation, from the paper's δ

**What δ is.** The paper's dial table (§1) and §8.2 define δ as "the marginal cost of delay, in Copa's utility `U = log(throughput) − δ·log(delay)`", on (0, ∞).
- δ is **dimensionless**.
- It prices a **relative** change of delay against a relative change of throughput.
- "1/δ packets" is Copa's equilibrium queue under that utility. It is a consequence of δ, not its unit.

The first draft used δ as a raw odds bar (take the bet at odds ≥ 1/δ). That has no provenance and is **withdrawn**.

**The bet.** At a dequeue under overload, x = λ̂/μ̂ > 1, drop the head u now instead of letting §13.2 send it. The head is the oldest unit, of age a.
- **If the overload persists** for `t_r = (T − R − a)/(x − 1)` more seconds, the queue would reach §13.2's margin and drop a unit anyway. The drop is then refunded: the throughput is unchanged, and every unit sent while the overload lasts is one service time, 1/μ̂, younger.
- **If it ends earlier,** u would have been delivered, so one unit is lost.

**The forecast (cited).** Gott's scale-free (Jeffreys) prior on an unknown duration (J. R. Gott III, *Nature* 363, 315, 1993; its limits in C. M. Caves, 2000):

```text
   P(persist ≥ s | elapsed t_ov)  =  t_ov / (t_ov + s) ,       so the odds of a refund are   t_ov / t_r
```

**The two outcomes in U.**
- **Refund.** The drop is compared with the status quo under persistence. There, §13.2 holds the queue at its margin, and every unit is delivered at `d₀ = (T − R) + ô`: the intake wait at the margin plus the one-way delay, which includes the forward queue (§17.1). The drop makes every unit sent during the persistence 1/μ̂ younger, so ΔU = +δ·(1/μ̂)/d₀.
  - **Why d₀, and not the head's own delay `a + ô`.** The beneficiaries are the units behind the head, sent over the persistence. Their delay under the status quo is d₀, not the head's current age.
  - Charging against a + ô made the expiry test `g(a) = a + R + K/(a + ô) ≥ T` non-monotone. It has its minimum at `a* = √K − ô`, of value `2√K − ô + R`, so for √K > ô a fresh head expired while a middle-aged one was held. That contradicts "give the oldest unit's slot to a younger one", the sentence §13.2 implements.
  - With d₀ the test is a threshold in a, so expiry is monotone.
- **Loss.** One unit leaves the throughput term. That term is averaged over the horizon the forecast speaks for, N = μ̂·t_h units, so ΔU = −1/(μ̂·t_h).

**The horizon t_h.** Requirement 1 forces it to be independent of T.
- With the refund horizon t_h = t_r, t_r cancels: the condition becomes δ·t_ov ≥ d₀. At T = ∞, P = 0 and t_r = ∞, so the expected-value form is 0·∞. It drops at T = ∞.
- Requirement 4 forces t_h to be measured. The only measured scale of an overload is its age.
- So **t_h = t_ov**, which is Gott's median remaining duration. This is the one step whose provenance is a choice and not a derivation (Q24).

**The result.** Drop the head iff the expected ΔU ≥ 0:

```text
   (t_ov/t_r) · δ/d₀  ≥  1/t_ov      ⇔      T − R − a  ≤  δ·(x − 1)·t_ov² / (T − R + ô)      ⇔      a + R  ≥  T − δ·(x − 1)·t_ov² / (T − R + ô)
```

- **Units:** seconds on both sides of the middle form.
- **δ enters linearly, as the paper's price.**
  - The effective margin `T − R − K/(T − R + ô)` falls linearly in δ at a fixed t_ov, and halving δ multiplies every collapse instant by √2.
  - OV-end reads the linear part directly (§4.3).
- **Shape in a.** Expiry is a threshold on age: a ≥ `c − K/(c + ô)`, with c = T − R. So the oldest goes first, and the law is monotone in age.
- **The factor is the overload's age squared:** once from the odds, and once from the horizon.

**Why the intake needs no standing part.** Copa's 1/δ queue exists to keep the link busy. Here the store pause already holds about `B − λ·RTprop` ≈ 880 datagrams in the link buffer. So the intake's own floor can be 0 (§3.1), and dropping in the intake changes which units are sent, not how many. The exception is the reserve flushed when an overload ends, and that is exactly what the bet prices.

## 3. The law

### 3.1 The wait limit, its floor, and how a collapsed sweep is read

```text
   at each dequeue (toy2.py emit, after L1):
      expire the head u   ⇔   a(u) + R  ≥  τ   ∧   a younger unit is waiting                  (§13.2 with T → τ)
      τ  =  T  −  K / ( T − R + ô ) ,        K  =  δ · (λ̂/μ̂ − 1)⁺ · t_ov²
```

| symbol | provenance |
|---|---|
| T, a(u), R | as in §2.1 and §13 |
| ô | §17.1: `srtt − RTprop/2` on the resend path. It is > 0 once there is a sample, and the RFC seed before one |
| δ | declared, on (0, ∞). NaN is already a config error (the `RWM_DELTA` precedent, §3 of the derivation); +∞ becomes one too, so K is finite |
| λ̂ | measured: units **handed over** per second over the rate law's window (8·srtt in the offline simulation; the engine's `rate_hat` window) |
| μ̂ | measured: capacity **for first sends**, §3.2 |
| t_ov | measured: `now − t₀`, with t₀ the last instant at which λ̂ ≤ μ̂. It is a timestamp of state and selects no law: at x ≤ 1, K = 0 whatever t_ov is |
| (·)⁺ | a refund needs a growing queue. It is the law's one clamp, gauged as `ov_frac`, the fraction of dequeues with x > 1 |
| t_h = t_ov | Gott's median horizon (§2; Q24) |

**Shape.**
- **Units:** τ [s] = T [s] − K [s²] / (T − R + ô) [s].
- **Monotonicity.**
  - **In T:** dτ/dT = 1 + K/(T − R + ô)² > 0.
  - **In δ, x and t_ov:** non-increasing.
  - **In a:** τ does not depend on a, so expiry `a + R ≥ τ` is monotone in age.
- **The denominator is positive** once R and ô are measured: T − R + ô > 0 whenever T > R − ô, and §13.2 already expires every unit at T ≤ R.
  - Below that, τ = T − K/(non-positive) could misbehave. The test reads `K / max(T − R + ô, ô)`.
  - That max is a clamp with its own gauge, `den_bind`, which is predicted 0 in every cell with T > R. Where T ≤ R, every head expires by §13.2 alone.
- **T = ∞:** K/∞ = 0.0 and ∞ − 0.0 = ∞, so the rule never expires and the trace is unchanged. No `isinf` test is used, and no ∞·0 arises, because K is finite.
- **x ≤ 1:** K = 0.0 exactly, and T − 0.0/(T − R + ô) = T exactly, so this is §13.2 **by identity**.
- **δ → 0:** τ → T. At δ = 0.005 the collapse in OV comes 9.7 s after μ̂ is finite at T = 0.4, and 55 s after at T = 2, so not within the 10 s run (§4.1).
- **δ → ∞** (a limit only): any x > 1 collapses at once at finite T. It is still inert at T = ∞.
- **Continuity.** τ is continuous in δ, T and x, including through x = 1. A step in t_ov at a reset happens only where K = 0. τ ≤ R simply expires the head, so no clamp is needed.

**The floor of τ.** τ has no lower bound. The **effective floor is the younger-waiting clause**: the intake keeps its newest unit, so the hold floors at 0, plus the wait until the next pause release. The law reaches that floor (it collapses) once the effective margin `c − K/(c + ô)` reaches 0, with c = T − R:

```text
   collapse  ⇔   K  ≥  K_c  =  (T − R) · (T − R + ô)
```

**Bind gauges.**
- **`floor_bind`:** the fraction of dequeues at which every unit but the newest is expired.
- **`collapse_t`:** the first instant at which K ≥ K_c.
- **`deq_early`:** expiries with `a + R < T ≤ a + R + K/(T − R + ô)`.

The first draft's hysteresis band, an artefact of the non-monotone form, is gone.

**Reading a cell where the lifetime sweep collapses.** This rule is registered here, before any data. When `floor_bind ≥ 1 − (the A/A noise)` over the readable windows at every finite T of a sweep, T no longer reaches the decision.
- **The finite-T rows must agree** within the A/A floor. If they do not, that is a finding.
- **They are pooled into one point,** compared with each baseline by dominance: delivered and on time not worse beyond the bar, and delay p50 not worse.
- **No matched-share interpolation** across the jump to T = ∞. T = ∞ stays a point of its own.
- **The degeneracy is recorded as a pinned law** under MEASUREMENT DISCIPLINE 17, with a ledger verdict the owner gives (Q29). The proposed verdict is "binds by design: under sustained overload, past the δ-priced horizon, the lifetime does not bind".

**NO-MODE-SWITCH.** The law is one expression. No dial is compared with a constant. `λ̂ ≤ μ̂` sets a timestamp; the law is continuous across it.

### 3.2 μ̂: capacity for first sends, measured by the sender (units checked)

λ̂ counts handed-over units, so μ̂ must count the same units: the first-send rate the path can sustain. It is not the wire rate, not the current first-send rate (which falls during a resend burst and is 0 in an outage), and **never the cell's configured link rate**. An oracle would make the gauge-first step (§5.3) prove nothing.

```text
   μ̂  =  D̂  =  max over the last 10 qualifying round trips of  ( source datagrams newly reported received in the round ) / ( the round's duration )
```

| symbol | provenance |
|---|---|
| source datagrams newly reported received | measured by the sender from the receiver's reports: first sends and copies, counted once, from the report's frontier and hole list. It is BBR's delivery-rate sample (Cheng and Cardwell, IETF draft `draft-cheng-iccrg-delivery-rate-estimation`), restricted to source datagrams. Repairs are excluded because nothing acknowledges them individually |
| a round | from the send of a datagram to the first report that covers it (BBR's round count). Rounds advance only on reports |
| the window | the last 10 rounds (Cardwell et al., *ACM Queue* 14(5), 2016) |
| qualifying | the sender was flow-limited in the round (the store pause or the send gate bound), and no single pause in it lasted longer than R. A longer pause means acks were overdue: the path stalled, which is no evidence of its capacity. **D̂ = +∞ unless all of the last 10 completed rounds qualified**, so x = 0 then |

**Why the delivered source rate is the first-send capacity.** At the steady resend share, a first send costs `1/(1 − ε)` source datagrams on the wire, and each unit is delivered exactly once. So the delivered source rate is the first-send rate:

```text
   wire  =  μ·(1/(1 − ε) + r) ,     source datagrams delivered per second  =  μ·(1/(1 − ε))·(1 − ε)  =  μ
```

This holds whatever r is, so no mix correction is needed.
- **Under Revision 3's unprotected sends** there are no resends, and D̂ = μ(1 − ε) under-reads by ε (3 %). This affects only the gauge-first readouts of the stored rows. Under the law, units are young and resent, so the identity holds.
- **The outage's resend debt does not lower D̂.** Those resends are source datagrams, and they are delivered at the same capacity. Taking μ̂ as first sends per second would read the drain at about 9 300/s, so x = 1.075 against 10 000 offered. At δ = 50 that collapses the backlog about √(0.1165/(50·0.075)) ≈ 0.18 s into the drain at T = 0.4, and about 1.0 s in at T = 2.

**CL-long's drain** (C = 12 500, ε = 0.03):

| arm | D̂ | margin over λ = 10 000 |
|---|---|---|
| repair off, and RL with the clamp on (r_eff ≈ 0 in the drain) | 12 500/1.031 = 12 124 | 21 % |
| RL with the clamp off (r ≈ 0.107) | 12 500/(1.031 + 0.107) = 10 984 | 9.8 % |

**The main risk: a sustained under-read beyond the margin.**
- **A 10 % under-read trips the clamp-off RL arm, and §5.3's bias row is predicted to show it.** With the clamp on, which is part of this revision, the margin is 21 %.
- **What protects the estimate from reading low:**
  - It is a **maximum** over 10 rounds, so per-round noise biases it upward. Report quantisation is 2 ms against rounds of 11–100 ms, so per-round noise is up to ±18 % at srtt 11 ms.
  - Rounds where the link idled for longer than R are excluded.
  - Lost or late reports delay counts into the next round, which raises that round's sample.
- **What would make it read low:** the link idling in every one of 10 consecutive rounds while flow-limited. That happens only when the bound is the gate and not the link, so it is a risk only where cwnd_sim < BDP. That never happens here, since B ≫ λ·RTprop in every link cell.
- **The gauge-first step reads D̂ against the oracle** `C/(1/(1 − ε) + r)`, as a readout only, in every link cell, and reports the ratio's distribution.
- **RE** (estimated loss) is overloaded by its own repair after the outage (r = 0.5), so D̂ ≈ 12 500/1.531 ≈ 8 160 and x ≈ 1.23. It is reported, not counted as headroom.
- **In the engine:** D̂ is the BtlBw anchor's delivery rate (paper §8.3). Its sampler has no app-limited flag, and its window form is unread. Both are prerequisites (Q26).

### 3.3 The engine's repair clamp (every design), its send gate (the pausing designs), and today's Realtime queue

**The engine's law, read.**

```text
   r_eff  =  min( r_law ,  max(spare, 0) ) ,       spare  =  (cwnd − in_flight) / in_flight      (+∞ at in_flight = 0)
```

- It applies to the cadenced rate (`emit_source.rs:579`, `flat_rate = window_rate.min(spare.max(0.0))`) and to each taper grant (`fec_rate.rs:729`).
- `spare` is `PathState::spare_capacity` (`path.rs:467–472`).
- **The send gate:** data is placed only on paths with `available() = cwnd − in_flight > 0` (`scheduler/mod.rs:229`).
  - **This does not hold back a Realtime first send** (engine read, shipped default: quinn BBR; no `RWM_QUIC_CC`, `COPA_WIRE` or `CC_PACE`). With no path available, `best_source_path()` returns None and the send goes to path 0 (`net/mod.rs:1459`, `emit_source.rs:358–361`, `:432`).
  - **There is no source pause at Realtime either.** `tx_paused = reliable && …` (`net/mod.rs:2473`), and reliable = false at Realtime (`config.rs:293–295`).
  - The datagram then waits in quinn's send queue: 4 MiB (`quic.rs:944`), about 7 900–8 192 symbols of 512 B. When full it evicts the **oldest** queued datagrams silently (quinn-proto `connection/datagrams.rs:42–46`, documented at `io_owner.rs:135–139`). It has no deadline, sheds no first sends, and drains at BBR's rate.
  - Under passthrough or the Copa wire (both non-default), a CC-rate token bucket pauses TUN reads at every hint, and the excess waits in the 4 096-packet TUN channel.
  - **NO-MODE-SWITCH debt, recorded:** `tx_paused = reliable && …` is a ρ-selected branch (a bit derived from ρ selects whether the source pauses). It goes on the law ledger (paper §11.3) as an engine site to retire or bound with a test. Under the three knobs the store pause would apply at every T, and the intake rule does the shedding.

**What the offline simulation lacks.** It has no cwnd. The new model's only in-flight bound is the store pause (O ≥ B seqs). Today's Realtime arm has no in-flight bound at all: it fills the 125 ms link buffer, and the link drops the excess. That is where its 0.172 s comes from, and it is not the engine's queue.

**The offline simulation's form** (a deliberate model change of every design, with a `GOLDEN_CAPTURE` re-baseline of every repair row):
- **cwnd_sim = B,** in payload datagrams. B is the cell's existing store cap (`pause_B`), a resource bound, so no new constant is added. The engine's cwnd is Copa's and is anchored at about BtlBw·RTprop; that is not modelled (Q22).
  - **Cells with a configured pause:** the link cells, B = 1 000; K-OV, B = 8.
  - **Cells with no configured pause:** RT, the call cell; the file cells; the E, M and K-I2/K-L10 cells. These run with the default `pause_B` = 2 048. In_flight there is at most λ·RTT plus the outstanding holes, about 5–30 datagrams in RT (50/s, 0.1 s), so spare ≥ 60 ≫ r ≤ 0.5. **The clamp cannot bind there,** and the rows stay bit-identical.
  - That is a prediction, checked before any run: the gauge-first step reports, per cell, `in_flight_max` and `spare_bind`.
  - **RT's PASS cannot change by accident:** the identity is tested (§5.2 item 9), not assumed.
- **in_flight:** payload datagrams sent and not yet acknowledged, or declared lost by a report. This is the engine's accounting at report granularity.
- **spare** = (B − in_flight)/in_flight, and +∞ at in_flight = 0, as in the engine.
- **The gate:** no payload datagram is staged while in_flight ≥ B. It applies to the designs that pause in the engine: the new model, ADR-0084's sender and today's Bulk. It does not apply to today's Realtime, which has the two arms below.
- **The clamp:** `owed += min(r, max(spare, 0))` per first send. At r = 0 nothing changes, so **repair-off rows stay bit-identical**.

**Where it binds.**
- **CL before an outage:** in_flight ≈ λ·RTT ≈ 110, so spare ≈ 8 ≫ r. Unchanged.
- **The CL-long drain and OV:** in_flight ≈ B, so spare ≈ 0 to 0.02 and r_eff falls to about 0–0.02.
- **Gauge `spare_bind`,** per cell and arm: the fraction of first sends at which spare < r.
  - **It must read ≈ 0 outside overload and drain windows:** CL and CL-long before their outage, RT, the file, E, M and K-I2/K-L10 cells, and CLw-long before its outage.
  - It binds in OV, and in the post-outage drain of CL-long and CLw-long. That window has no spare capacity, which is consistent with the engine read: the engine's clamp exists exactly to stop repair there.

**Measured** (coordinator, 2026-11-26; in_flight counts every unreported datagram, first sends, copies and repairs; cwnd_sim = 2 048):

| | T = 0.4 | T = ∞ | RS at T = ∞ | RF3 |
|---|---|---|---|---|
| `spare_bind`, CL-long, RL | 0.12 | 0.67 | 0.77 | 0.97 |

- **Elsewhere:** about 0 in CL-long-v, Ec and RT.
- **in_flight in the post-outage drain:** 1 230–1 460.
- **The clamp binds when in_flight exceeds `cwnd/(1 + r)`,** 1 850 at r = 0.107, so the bind comes from the drain's in-flight peaks.
- **It rises with T** because the drain gets longer: at T = 0.4 the intake rule keeps only λ·(T − R) ≈ 3 850 units of backlog, against about 15 000 at T = ∞. So a larger share of the run is drain.

**Predictions for CL-long with the clamp on, RL** (derived from the drain arithmetic, written before the clamp-on runs):

- **The arithmetic.** Recovery after the outage is the backlog over the drain headroom `h = μ − λ`, with μ the first-send capacity `C/(1/(1 − ε) + r_eff)` and λ = 10 000. The backlog is the same in all three modes at a given T.
  - **Repair off:** μ = 12 124, h ≈ 2 120/s.
  - **RL:** r falls to ≈ 0.088 once the queue raises srtt (registration §2.6), so μ ≈ 11 230 and h ≈ 1 230/s. That predicts recovery 1.7× repair-off, against ADR-0084's measured 4.8 → 8.6 s (1.79×), which checks it.
  - **Clamp on:** in the drain, `r_eff ≤ r·(1 − spare_bind_drain)`. This is an upper bound on r_eff, because a binding grant is max(spare, 0), not 0. At the drain's bind fraction (≈ 0.6–0.7 at T ≥ 2), r_eff ≲ 0.03, so μ ≈ 11 790 and h ≈ 1 790/s.
- **Recovery time** scales as 1/h: about 1.15–1.25 × repair-off at T = 2 and ∞, against RL's 1.7–1.8×. So clamp-on sits about a quarter to a third of the way from repair-off to RL.
- **On-time share** loses the units late during the drain, which is proportional to the drain duration, so it interpolates the same way:

  | T | repair off | RL (stage 3e) | clamp on, predicted |
  |---|---|---|---|
  | ∞ | 0.6500 | 0.4485 | ≈ 0.59–0.61 |
  | 2 | 0.6531 | 0.4586 | ≈ 0.58–0.61 |
  | 0.4 | — | — | equal to RL within the A/A floor |

  - **Recovery at T = 0.4:** within the 0.1 s resolution in all three modes, as in stage 3d.
  - **Why T = 0.4 does not move.** The bind fraction is 0.12, so r_eff ≈ 0.09, h moves by under 5 %, and the backlog is short.
- **Every clamp-on value must lie between repair-off and RL.** A value outside that interval is a finding, not a pass.

**cwnd_sim = B is a modelling choice.** The engine's cwnd is BBR's under quinn by default, and the anchor of paper §8.3 holds it at about BtlBw·RTprop or more, so the bind fraction depends on that choice.
- **The conclusion** "the clamp binds in overload and drain windows, not elsewhere" holds for:

  ```text
     (1 + r)·in_flight_steady   <   cwnd   <   (1 + r)·in_flight_drain_peak
  ```

  - in_flight_steady is λ·RTT plus the outstanding holes, about 110–150 in CL before its outage.
  - in_flight_drain_peak is 1 850 or more here.
  - So the conclusion holds for **cwnd roughly between 300 and 2 000 datagrams** in CL-long.
- **Above that range** the drain never binds, so the clamp buys nothing in CL-long but still binds in OV, where in_flight sits at the cap.
- **Near the BDP** (about 125–250 at CL's 12 500/s and 10 ms RTprop), the clamp would also bind in CL's steady state at 80 % load, which would change the call and CL rows.
  - So if the engine read finds its cwnd close to the BDP, the "≈ 0 outside overload and drain" claim fails.
  - The clamp model must then take the engine's cwnd law, and not B.
- **The gauge-first step reports `in_flight` quantiles per cell,** so this range can be checked against any cwnd without a rerun.
- **The CL-long RL rows change.** They are re-baselined, not inherited.
- **Every repair-on row re-runs under the clamp, DECIDE included,** for every design and every cell. The DECIDE row is CL-long under RL. Its stage 3e reading (UNDETERMINED with repair, PASS with repair off) is void for the repair modes until the re-run. Repair-off rows are bit-identical by arithmetic, and are checked, not assumed.

**Today's Realtime: two arms, both run and reported (Fairness rule 4; Q30 settled by the engine read).**

1. **As stored:** first sends go straight into the link's 125 ms buffer, which tail-drops. It is kept for continuity with stage 3e, and it flatters the engine.
2. **Engine queue (new):**
   - **The queue:** a front FIFO of 4 MiB in symbols, so 8 192 symbols of 512 B, with every payload datagram counted as one symbol.
   - **Eviction:** when full, the **oldest** queued datagram is evicted, silently.
   - **No deadline.**
   - **The drain:** at the link rate, so BBR's pacing is modelled as a drain at C and the link buffer stays about empty.
   - **The same queue** takes first sends, copies and repairs.
   - **in_flight** for the clamp counts datagrams handed to the queue and not yet acknowledged, so under overload spare → 0 and r_eff → 0.
     - This assumes the engine counts in-flight at its own send, ahead of quinn's queue. That is not yet read, so it is flagged for the read-only engine check.
   - **The residence time under a sustained overload** is `Q_f/λ_wire`, the queue over its arrival rate, because the head leaves by service or by eviction.

**The predictions for both arms are in §4.1 and §4.3.**

### 3.4 What the clamp nets under saturation, from the measured mechanism

**Measured in OV under RL** (stored runs plus count-only gauges, `toy-stage3e` `fd77697a`; 120 000 units):

| design | repairs sent | repairs dropped by the link queue | units decoded | of them, queue-dropped first sends | redundant repairs | first sends dropped |
|---|---|---|---|---|---|---|
| today's Realtime, δ = 50 | 10 507 | 10 077 (96 %) | 138 | 0 | — | 19 187 by the link queue, of 29 263 queue drops |
| new model, T = 0.4 | 8 331 (8.0 % of link sends) | 0 | 2 821 (99.8 % of 2 826 lost first sends) | — | 5 254 | 24 853 at the intake (dequeue rule) |

- **Today's sender:** its repair is spent on its own queue. Its share is the link buffer passing first sends, `(120 000 − 19 187)·(1 − ε)/120 000 ≈ 0.815`, against 0.8165 measured. It is not a decode effect: the first draft's inference is refuted.
- **The new model:** its repair works, but under saturation every repair slot is a first send not made.

**The net of the clamp** (r_eff → 0 under saturation), per first send f:
- it gains r·f first sends, each delivered with probability 1 − ε;
- it loses the ε·P_dec·f units the repair decoded.

```text
   Δshare  =  (f/N) · ( r·(1 − ε)  −  ε·P_dec )
```

- **Inputs:** r = 8 331/f, with f ≈ 95 800 first sends, so r ≈ 0.087. ε ≈ 2 826/f ≈ 0.0295. P_dec = 0.998.
- **Result:** Δshare = 0.798·(0.0844 − 0.0294) = **+0.044**.
- **Check against the counts:** +8 331·0.97 − 2 821 = +5 260 units, which equals the 5 254 redundant repairs to within the loss on the freed slots.

**Applied to the 2×2 of §4.1:**
- **Wait limit off, clamp on:** 0.7929 + 0.044 = **0.837**, on time the same, p50 unchanged at 0.390 s. The units are still unprotected at T − R, so the lost ε stays lost.
- **Wait limit on, clamp on:** the units are young, so the lost ε is recovered by resend, costing ρ = ε/(1 − ε) ≈ 0.030 of link sends instead of r = 0.087. The steady share is C/((1 + ρ)·λ) = **0.808**, on time 0.80, p50 ≈ 0.10 s. That is +0.038 against the wait limit alone (0.770), with no flush.
- **Today's Realtime as stored:** the clamp removes the repairs its link queue was dropping, so its share stays ≈ 0.81. The engine-queue arm is in §4.1.

**Revised prediction rows:** clamp alone 0.837 / 0.837 / 0.39 s; Revision 4 with both parts 0.80–0.81 / 0.79–0.80 / 0.095–0.10 s.

## 4. Predictions (derived, written before any run)

**Common numbers (OV).**
- C = 10 000/s, λ = 12 000/s, R ≈ 0.102 s, ô ≈ 0.093 s, and the forward queue plus owd ≈ 0.093 s.
- The collapse threshold K_c = (T − R)(T − R + ô) is 0.1165 at T = 0.4, 0.890 at T = 1 and 3.78 at T = 2.
- **Clamp off:** μ̂ = D̂ ≈ 10 000/(1.031 + 0.087) ≈ 8 950, so x ≈ 1.34. **Clamp on:** μ̂ ≈ 10 000/(1.031 + 0.01) ≈ 9 600, so x ≈ 1.25.
- **μ̂ becomes finite after 10 qualifying rounds,** about 1 s at srtt ≈ 0.1 s. The collapse then follows after `t_ov* = √(K_c/(δ·(x − 1)))`. Before it, the queue wait is capped at `c − K/(c + ô)`, which falls from T − R.

### 4.1 OV (post-data: these rows cannot confirm the law)

**The 2×2 at δ = 50, T = 0.4** (delivered / on time ≤ 0.4 s / delay p50):

| | clamp off | clamp on |
|---|---|---|
| **wait limit off** (Revision 3) | 0.7929 / 0.7928 / 0.390 s (stored) | 0.837 / 0.837 / 0.39 s (§3.4) |
| **wait limit on** | 0.770 / 0.770 / 0.095 s | 0.79–0.81 / 0.79–0.80 / 0.095–0.10 s |

How each cell is derived:
- **Clamp alone.** r_eff ≈ 0, so there is no displacement. Units are still sent unprotected at T − R, so first-send loss stays lost: `(C/λ)(1 − ε)` = 0.808, plus a flush of 0.298·9 700·0.97/120 000 = 0.024. That gives 0.832 from capacity, and 0.837 from the measured counts (§3.4); the counts are used.
- **Wait limit alone.** `μ̂·10 s/N` with no flush, so 0.767–0.772. The collapse comes 83 ms after μ̂ becomes finite.
- **Both.** Units are young and so protected at birth (0.098 + 0.093 < 0.4). Loss is recovered by resend, at ρ ≈ ε/(1 − ε), and a resend arrives by about 0.3 s. The share is C/((1 + r_eff + ρ)·λ), which is 0.808 at r_eff = 0 and about 0.79 if r_eff stays near 0.02.

**Against today's Realtime.**
- **The stored arm:** 0.8165 / 0.814 / 0.172 s. In the steady windows its share is 0.8085, level with Revision 4's 0.808.
- **The engine-queue arm** (with the clamp, r_eff ≈ 0):
  - The FIFO fills in 8 192/(12 000 − 10 000) ≈ 4.1 s. After that, survivors wait `8 192/12 000` ≈ 0.68 s, so p50 ≈ 0.65–0.70 s.
  - On time within 0.4 s holds only while the queue is below about 3 950 symbols, the first ≈ 2.0 s, so ≈ 0.20–0.25.
  - Whole-run delivered is the link's 10 s plus the queue's 0.82 s flush, times 1 − ε: ≈ 0.84–0.88.
  - In the steady windows the share is 0.808 and on time is 0.
- **Read by §3.1's dominance rule,** since the finite-T sweep is degenerate here:
  - against the stored arm, Revision 4 with both parts is equivalent in steady share and better in delay;
  - against the engine-queue arm, it is better on steady on-time share (0.80 against 0) and on delay.

**The δ and T map with the clamp on** (x − 1 = 0.25):

| δ | T | collapse after μ̂ is finite | delivered | on time ≤ 0.4 s | p50 |
|---|---|---|---|---|---|
| 50 | 0.4 / 1 / 2 | 0.10 s / 0.27 s / 0.55 s | ≈ 0.80 | ≈ 0.80 / 0.80 / 0.79 | ≈ 0.10 s |
| 0.5 | 0.4 | 0.97 s (the wait stays at the §13.2 cap of 0.30 s until then, so on time) | ≈ 0.80 | ≈ 0.80 | ≈ 0.10 s |
| 0.5 | 1 | 2.7 s (the wait peaks at ≈ 0.6 s) | ≈ 0.80 | ≈ 0.63 | ≈ 0.10 s |
| 0.5 | 2 | 5.5 s (the wait peaks at ≈ 1.1 s at ≈ 3.5 s) | ≈ 0.81 | ≈ 0.40 | ≈ 0.7 s |
| 0.005 | 0.4 / 2 | 9.7 s / 55 s (neither within the run) | Revision 3 with the clamp: 0.837 / 0.837 / 0.39 s at T = 0.4 | | |

- **Strict order (H = ∞) at δ = 50, T = 0.4,** re-derived:
  - Young units are protected, so the floor lags each lost seq until FEC or a resend fills it.
  - With repair clamped to about 0, the resends carry the loss: about 3 % × 9 600 ≈ 290/s, against the 2 000/s budget.
  - Each blocks the strict frontier for about one R, so about 29 holes are open at once. The frontier trails the oldest of them by 0.1–0.2 s.
  - Predicted: on time 0.72–0.79, p50 0.18–0.25 s. That is not §15.2's collapse, because the holes are filled, not given up a round trip late.
  - **Low confidence:** the second resend's margin is 9 ms, and §19 found ô undershooting a persistent queue.

### 4.2 CL-long, CL, CLw-long, RT, K-I2, K-L10

**The wait limit.** x ≤ 1 at every dequeue:
- before an outage, no round qualifies, so D̂ = +∞;
- the round that straddles an outage fails the pause clause;
- after it, D̂ reads the saturated link's delivered source rate, giving the margins of §3.2 (21 % or 9.8 %).

So K = 0 and the rows are **identical to whatever the clamp makes them.** That covers:
- **repair-off rows:** bit-identical to the stored ones, for example CL-long at T = 0.4, 94.14 % delivered with on-time recovery within 0.1 s (§18 amendment);
- **repair rows:** identical to the clamp-only re-baseline.

**The clamp.** It changes the repair rows only where in_flight nears B: the CL-long and CLw-long drains, and K-OV.

**Arms overloaded by their own repair or estimate:**
- **RF3** (r = 0.36, if the clamp is off) and **RE** change, by design.
- With the clamp on, RF3's r is capped in the drain, so x ≤ 1. Predicted identical to the clamp-only row.

### 4.3 The cells that can test the law (registered before any data)

**OV-end** (new): λ = 12 000/s for 0–6 s, then 8 000/s for 6–16 s, so 152 000 units; otherwise OV's settings, clamp on. It prices the bet.
- **Revision 3** carries a reserve out of the overload, and delivers it on time:
  - μ̂·(T − R) ≈ 2 860 units at T = 0.4;
  - at T = 2, the queue at 6 s, a wait of ≈ 0.25·6 = 1.5 s or ≈ 14 400 units, drained at the 1 600/s headroom, with every wait ≤ 1.5 s < T − R.
- **Revision 4 at T = 0.4:** δ = 50 and δ = 25 collapse 0.10 s and 0.14 s after μ̂ is finite. Both lose the 2 860-unit reserve, −0.019 on time against Revision 3. The arms are indistinguishable, which checks that the cost equals the reserve.
- **Revision 4 at T = 2, δ = 0.5 and δ = 0.25.** Neither collapses before the overload ends, at 5.5 s and 7.8 s after μ̂ is finite.
  - At 6 s, with t_ov ≈ 5 s, the effective margin is `1.898 − δ·0.25·25/1.991`: 0.33 s at δ = 0.5 and 1.11 s at δ = 0.25.
  - So the reserves are about 3 170 and 10 660 units, against Revision 3's 14 400. Delivered within T falls by 0.074 and 0.025.
  - **The shortfall against Revision 3's margin, 1.57 s against 0.79 s, is linear in δ.** That ratio of 2, read from the reserve at the overload's end, is the direct test of δ's seat. The time at which each cap falls below 0.4 s, ∝ δ^(−½), tests the t_ov² horizon (Q24).
  - On time within 0.4 s: Revision 4 gains only where its cap is below 0.307 s. That is from about 5.9 s at δ = 0.5, and never within the overload at δ = 0.25, so the gain is ≤ 0.01. This row tests the cost, not the gain.
- `collapse_t`, the cap trace and the reserve at 6 s are read directly.
- **Today's Realtime in OV-end:**
  - **The stored arm:** about 0.81 share and on time in the overload windows [3, 5.5] s, at p50 ≈ 0.17 s, and ≈ 0.97–0.99 in [9, 15] s.
  - **The engine-queue arm:**
    - In [3, 5.5] s it is on time on ≈ 0: the wait passes 0.395 s at ≈ 2.0 s.
    - At 6 s the queue is full at 8 192 symbols. It drains at 2 000/s and is on time again from ≈ 8.1 s, so [9, 15] s reads ≈ 0.97–0.99.
    - Whole run: delivered ≈ 0.97 (about 3 800 evicted between 4.1 and 6 s), on time ≈ 0.57.
  - **Revision 4 with the clamp (T = 0.4, δ = 50):** 0.808 / 0.80 in [3, 5.5] s and ≈ 0.99 in [9, 15] s, so it beats the engine-queue arm in the overload windows and equals it after.

**OV-crit** (new): λ = 9 600/s, the first-send capacity with the clamp; otherwise OV, 10 s. The knife edge.
- **Predicted:** `deq_early` ≤ 1 % of hand-overs, and Revision 4 equals Revision 3 within the A/A floor on all three readouts.
- **Above that,** the knife edge bites: the law returns to derivation.
- **Today's Realtime in OV-crit.** Unclamped, its wire load is 9 600·1.087 plus resends ≈ 10 700/s > C, overloaded by its own repair.
  - **The stored arm:** the link buffer stays full and drops about 6 %. On time ≈ 0.91–0.95, p50 ≈ 0.13 s.
  - **The engine-queue arm, with the clamp:**
    - **The loop.** The queue grows, in_flight rises, spare falls and r falls; the wire then drops below C, the queue drains, spare rises and r rises again. So the clamp holds the wire near C.
    - **Predicted:** p50 ≤ 0.1 s, on time ≈ 0.95–0.97.
    - **Low confidence:** this is a feedback loop that may oscillate. `spare_bind` and the queue-length trace are read.

**OV at δ = 0.5:** the δ = 0.5 rows of §4.1. Here T still matters, and the collapse instants are 0.55, 1.4 and 2.8 s.

**K-OV** (λ = 50/s, C = 40/s, L10 loss, owd 50 ms, B = 8, 3 000 units). It was bounded at 10 s by TIME and never run, so it needs a time-limit amendment: one seed is timed first, and the bound is set from it.
- **Inputs.** R ≈ 0.21 s and ô ≈ 0.15 s, so K_c = 0.19·0.34 = 0.0646. With the clamp on, μ̂ = 40·0.9/(1 − 0.1)·… = D̂ ≈ 36, so x ≈ 1.39. μ̂ becomes finite after about 2 s, at rounds of 0.2 s. The collapse then follows in 58 ms.
- **Revision 4 at T = 0.4:**
  - delay p50 ≈ 0.16 s: the 0.1 s pause queue, plus the 50 ms owd, plus up to one ack gap;
  - delivered ≈ 0.72, on time 0.66–0.72. A resend arrives near 0.36 s, at the edge.
- **Revision 3:** the registration's ≈ 0.72 on time at p50 ≈ 0.34 s.
- **Today's Realtime:**
  - **the stored arm:** ≤ 0.1 on time, because it fills the 1 s link queue;
  - **the engine-queue arm:** 8 192 symbols at 40/s is 205 s, longer than the 60 s run, so nothing is evicted. The wait grows at (50 − 40)/40 = 0.25 s per s, giving p50 ≈ 7 s and on time ≈ 0.03 (the first ≈ 1.6 s). Delivered is ≈ 0.9–0.99 once the 15 s drain after hand-over is inside `t_limit`.

**How the fresh cells are read (registered before any data).** This applies to OV-end, OV-crit, K-OV and OV at δ = 0.5.
- **Primary readouts:** the steady-window share delivered and the steady-window share on time within 0.4 s. They are computed over hand-over windows (0.5 s wide, §17.4) inside a registered steady interval:

  | cell | steady interval |
  |---|---|
  | OV | [3, 9] s |
  | OV-end | [3, 5.5] s (overload) and [9, 15] s (after) |
  | OV-crit | [2, 9] s |
  | K-OV | [10, 55] s |

- **Readouts only:** delay p50 from hand-over over every unit, and the whole-run delivered share. The whole-run gap is the end-of-run backlog (§3.4, the flush), which the steady windows exclude.
- **A collapsed lifetime sweep** (`floor_bind` ≈ 1 at every finite T) is pooled and read by dominance (§3.1).
- **The steady prediction is level, not a lead.**
  - Revision 4 with the clamp predicts 0.808 in OV's steady windows. Today's Realtime's stored steady share is 0.8085: `(C − its carried repairs)·(1 − ε)/λ`.
  - So the share reading is predicted **equivalent**. The delay p50 (≈ 0.10 s against the stored arm's 0.172 s, and the engine-queue arm's ≈ 0.68 s) is the readout where they differ. Against the engine-queue arm, the steady on-time share differs too (≈ 0.80 against ≈ 0).
  - Today's higher whole-run delivered share (0.8165) is its own end-of-run flush of the 125 ms link buffer. It is not a steady-state difference.

### 4.4 Two paths

- **μ̂ pools the paths:** D̂ = Σ D̂_p, and x = λ̂/μ̂.
- **A path in an outage** keeps its held D̂_p. That overstates the capacity, so x reads low: the safe side, at Revision 3.
- **A path whose ô exceeds T** also counts at full capacity, again the safe side.
- **The P2 cells** run below capacity, so they are predicted identical.

## 5. Risks, tests, the gauge-first step and the sizing run

### 5.1 Risks

- **The horizon step (Q24).** Every collapse time scales with √(t_h/t_ov). Nothing ships until the owner decides, and OV-end's δ against δ/2 rows read its consequence.
- **The knife edge at x ≈ 1, amplified by δ.**
  - K grows as δ·(x − 1)·t_ov². With μ̂ biased high it reads 0, the safe side. With μ̂ biased low by more than the margin, it collapses after `√(K_c/(δ·bias))`.
  - The engine prerequisite is the app-limited exclusion (Q26). OV-crit and the bias rows of §5.3 measure the edge.
- **The capacity estimate reading low (main risk).** See §3.2.
- **Oscillation.**
  - τ reads none of the quantities it moves. λ̂ is the application's rate. D̂ changes only in qualifying rounds, and the sender stays flow-limited after a collapse. r_eff and ε̂ do not depend on the intake.
  - A collapse leaves srtt and R unchanged, because the link queue is set by B.
  - Noise near x = 1 only resets t_ov, which moves τ toward T.
- **The store pause and the gate.** The rule runs only at a dequeue. Through an outage D̂ holds, and the round that straddles it does not qualify.
- **The floor law.** Units sent young are protected, so resends return under overload (§4.1, strict order). The commitment and L1 tests are unchanged.
- **The cost of the bet** when an overload ends, `μ̂·(T − R)` per episode at high δ. OV-end measures it.
- **The clamp changes every repair row.** Every stage 3e repair baseline must be re-run under the clamp before it is compared. Repair-off rows are untouched by arithmetic.

### 5.2 Tests (red first)

FORMULA-FIRST: the law, §2's derivation and §3.3's clamp go into the paper first (near §5.6, and §5.3's rate clamp), and **only after Q24 is answered.**

1. **`wait_limit` inert at T = ∞.** τ is +∞, never NaN, for δ ∈ {0.005, 0.5, 50, 10⁶}, x ∈ {0, 1, 1.3, 100}, t_ov ∈ {0, 10⁶} and a ∈ {0, 1}.
   - The source deny test forbids `isinf`, `isfinite` and any literal comparison of δ, T or x.
2. **The identity at headroom.** τ == T exactly for every x ≤ 1.
3. **Shape.**
   - Monotone in T, δ, x and t_ov.
   - **The expiry predicate is monotone in age:** for every K ≥ 0 and every a₁ < a₂, if a₁ expires then so does a₂. This test is red against the first draft's `K/(a + ô)` form.
   - ±2 % continuity at each δ preset and at T = 0.4, and continuity through x = 1 ± 10⁻⁹.
   - The collapse threshold equals K_c = (T − R)(T − R + ô) to 10⁻¹² for a grid of T and ô. `den_bind` = 0 wherever T > R.
4. **The units of μ̂, and no oracle.**
   - μ̂ is computed only from the sender's reports: a source deny test forbids reading the cell's link rate, `link_sv` or `p0` in the estimator.
   - In CL-long, RL, clamp off, in the drain: D̂ ≈ C/(1/(1 − ε) + r) within the quantisation noise of §3.2.
   - With resends forced to their budget, D̂ does not move. It is red against μ̂ taken as "first sends per second".
5. **The sample rule.** A round with a pause longer than R never qualifies. D̂ = +∞ until 10 rounds in a row have qualified, and D̂ > 0 always.
6. **CL-long identity with an execution witness.** S5, R0, RL and RS at T ∈ {0.4, 2, ∞}, clamp on, are trace-identical to the clamp-only rows, with `ov_frac` = 0 and `deq_early` = 0 in all 12 seeds.
   - Red against the instantaneous-rate μ̂, and against the sample rule without its pause clause.
7. **OV executes.** At δ = 50 and T = 0.4, `deq_early` > 0 and `floor_bind` > 0.9 after `collapse_t` + one round.
   - The intake wait of every first send is then no greater than the longest pause gap (absolute, from `deq_log`).
8. **The burst control.** CL with a planted 200-unit burst at δ = 50 gives `deq_early` = 0.
9. **The clamp.**
   - r_eff ≤ max(spare, 0) at every accrual.
   - At r = 0 the trace equals the unclamped trace.
   - No payload datagram is staged while in_flight ≥ B.
   - `spare_bind` = 0 in CL before the outage, and in RT, the file cells and E, M, K-I2 and K-L10.
   - The RT, file and call-cell rows are trace-identical with the clamp on and off.
   - In CL-long under RL at T ∈ {0.4, 2, ∞}, the clamp-on on-time share and recovery lie between the repair-off and RL rows (§3.3).
   - The measurement used cwnd_sim = 2 048. The run must state the cwnd_sim it used, because the bind fraction depends on it.
10. **Starvation and the unchanged absolutes.**
    - With τ < R, first sends per round are greater than 0.
    - `l1_viol` = 0 and the commitment violations are 0.
    - The T = ∞ rows equal full delivery in every cell.

### 5.3 The gauge-first step (before any run of the law)

The law is added as **readouts only**: λ̂, μ̂ = D̂ (from the sender's own reports, §3.2), t_ov, K and τ at every dequeue, plus the count `n_tau_lt_T` of dequeue ticks with τ < T. **No decision reads them.**
- Also read: D̂ against the oracle `C/(1/(1 − ε) + r)`, and `in_flight_max` and `spare_bind` for the clamp. The clamp is not yet active either.
- **What it runs on:** seed 0 of every stored stage 3e new-model configuration, clamp off, as stored. The traces must equal the stored digests, which shows the readouts are inert.
- **The bias rows:** μ̂·(1 − b), for b ∈ {0.02, 0.05, 0.10}, in CL-long's drain (S5, R0, RL; T ∈ {0.4, 2}).

**Predicted.**
- **`n_tau_lt_T` = 0 in every cell with headroom:** CL, CL-long, CLw-long, RT, K-I2, K-L10, CLc, the E and M cells, and the video cells.
  - **Exceptions with headroom:** RF3 and RE, overloaded by their own repair or estimate, and RL at b = 0.10 (margin 9.8 %). These are predicted non-zero and marked in advance.
- **Non-zero:** OV, OV-long, OV-long-v and K-OV.
- **Stop rule:** a non-zero count in any other headroom cell, or at b ≤ 0.05, sends the law back to derivation before any run of the decision.

### 5.4 Sizing run (reported only, no verdict, predictions as in §4)

**Cells and arms:**
- **OV, the 2×2:** wait limit on or off × clamp on or off; S5 and RL; T ∈ {0.4, 2, ∞}; δ ∈ {0.5, 50}. That is 24 configurations. OV is post-data and confirms nothing; it is the calibration of the 2×2.
- **Today's Realtime,** both arms (as stored, and engine queue), each with the clamp on and off, at δ = 50, in OV, OV-end and OV-crit: 12 configurations.
- **OV-end:** Revision 4 with the clamp, T = 0.4 at δ ∈ {50, 25}, and T = 2 at δ ∈ {0.5, 0.25}, plus Revision 3 with the clamp at both T: 6 configurations.
- **OV-crit:** Revision 3 and Revision 4, both with the clamp, at T = 0.4 and δ = 50: 2 configurations.
- **K-OV**, after its time-limit amendment, which must cover the engine-queue arm's 15 s drain: Revision 3 and Revision 4 with the clamp, plus both of today's Realtime arms, at T = 0.4: 4 configurations.

**Size:** 3 seeds each, about 50 configurations and 150 runs. At TIME's rates (7.7–34 s per run) that is under 15 min on 9 workers, plus K-OV's measured time.

**Readouts:**
- §4's three readouts;
- the gauges `deq_early`, `floor_bind`, `collapse_t`, `den_bind`, `ov_frac` and `spare_bind`, and the reserve at the end of an overload;
- the fraction of rounds with D̂ = +∞, and D̂ against the oracle;
- r_eff and the resend rate;
- `dec_units_qdrop` and the queue drops of repairs, per design, to confirm §3.4's accounting under the clamp;
- the Gott calibration: forecast persistence against realised persistence, per episode.

## 6. Owner questions (numbering continues from Q23)

- **Q24, the horizon step (answers Q12; open, blocks shipping).** δ prices a relative delay, as the paper defines it. The bar follows from U only once the throughput term is read over a horizon.
  - Requirements 1 and 4 force that horizon to be measured and independent of T, and the overload's age t_ov is the only such scale.
  - "Horizon = age" is Gott's median, cited but a choice. **No default is proposed.** The owner decides after OV-end's δ against δ/2 rows.
  - The first draft's raw 1/δ odds bar is withdrawn.
- **Q25, the forecast.** Gott's scale-free prior, or a declared horizon. **Default: Gott**, with its calibration gauged.
- **Q26, the capacity estimate in the engine.** The BtlBw sampler needs BBR's app-limited rule, a round-counted window and the pause clause of §3.2. **Default: prerequisites of the engine step.**
- **Q27, the clamp in the offline simulation.** cwnd_sim = B and the clamp for every design, and the send gate for the designs that pause in the engine (§3.3). **Default: yes**, as a deliberate model change with a re-baseline. It replaces the first draft's "register Q22 next": the clamp is the engine's law, so it is part of this revision.
- **Q28, a standing part of the intake** (Copa's 1/δ). **Default: no.** The store pause keeps the link busy.
- **Q29, the ledger verdict for a lifetime sweep that collapses under overload** (§3.1). **Proposed: "binds by design".**
- **Q30, settled by an engine read:** today's Realtime first sends bypass the send gate and wait in quinn's 4 MiB evict-oldest datagram queue (§3.3).
  - The offline simulation gains the arm "today's Realtime, engine queue". The stored arm is kept, and both are run and reported (Fairness rule 4).
  - **Still open:** whether the engine counts in-flight ahead of quinn's queue, which the clamp in that arm assumes (read-only check).
  - **Recorded debt:** the ρ-selected `tx_paused = reliable && …` (`net/mod.rs:2473`), for the law ledger.

## 7. Effect on the earlier sections

- **§13.2:** the rule becomes `a + R ≥ τ(u)`. Its paragraph "What cannot be fixed" is withdrawn. The young-first order it mentions stays rejected (§8 item c).
- **§5's overload row:** the intake collapses once the δ-priced bet pays; the dequeue no longer expires at T − R for ever.
- **§2.9, what δ sets,** gains a job: the intake under overload, through U. That is the seat the deleted D(δ) had.
- **The registration's §2.5 and §2.6:** the repair clamp of §3.3 joins the model, so "the rate law never pays under sustained overload" now meets the engine's own limit.
- **§17.10's open item "delay pinned near T − R":** answered, subject to Q24.
- **Not changed:** L1, L2, the protected floor, the brake, the wire format and the receiver.

## 8. The candidates, evaluated

All candidates use the dequeue form `expire the head u ⇔ a(u) + R ≥ τ ∧ a younger unit is waiting`, unless stated. They share the OV numbers of §4 (x ≈ 1.30, μ̂ ≈ 9 200/s, R ≈ 0.102 s, forward queue plus owd ≈ 0.093 s) and §3's λ̂ and μ̂, sample rule included.

### (a) τ scaled by the measured headroom

```text
   τ_a  =  T · min( 1 , μ̂/λ̂ )
```

| symbol | provenance |
|---|---|
| μ̂, λ̂ | measured, as in §3. The held capacity is required. With the current send rate in place of μ̂, an outage gives `T·0`, which is NaN at T = ∞ |
| min(1, ·) | keeps τ ≤ T. It is a clamp, gauged as `ov_frac` |

- **Limits.**
  - T = ∞: ∞ × a factor in (0, 1] is ∞, so it is inert. μ̂ > 0, and λ̂ = 0 gives μ̂/0 = ∞, so the factor is 1.
  - δ → 0 and δ → ∞: no change, because δ has no seat.
  - Zero headroom (x ≤ 1): τ = T, the identity.
  - x → ∞: τ → 0⁺. That is still ∞ at T = ∞.
- **Shape.**
  - Units: seconds.
  - τ is non-decreasing in T and non-increasing in x.
  - A standing queue proportional to T remains: hold = `T/x − R`.
- **OV, RL:**

  | T | hold | p50 | delivered | on time |
  |---|---|---|---|---|
  | 0.4 | 0.21 s | 0.30 s | 0.783 (a flush of 0.016) | 0.783 |
  | 0.6 | 0.36 s | 0.45 s | 0.795 | ≈ 0.12 |
  | 2 | 1.44 s | 1.53 s | — | ≈ 0.11 |

  So the cliff only moves from T ≈ 0.41 to T ≈ 0.52.
- **CL-long:** the identity, under the same condition as §4.2.
- **Risks.**
  - **Oscillation:** none. Its inputs are not moved by τ.
  - **Store pause:** as in §5.1.
  - **Floor law.** At T = 0.4 a unit sent at hold 0.21 s is protected by 3 ms (0.21 + 0.098 + 0.093 = 0.401 against 0.4, so it sits at the margin). Strict order would sit on §15.2's knife edge.
  - **Two paths:** as in §4.3.
- **Variants.**
  - Any power `(μ̂/λ̂)^k` is a tuned k.
  - A fluid forecast over a horizon proportional to T fails at T = ∞ (§1).
  - With a measured horizon and U's price it becomes §3's law.
- **Rejected; its forecast is kept.**

### (b) A priced form: hold while δ × the queue is below the unit's worth

```text
   expire the head u   ⇔   a(u) + R ≥ T   ∨   δ · μ̂ · a(u)  ≥  1          i.e.   τ_b  =  min( T ,  R + 1/(δ·μ̂) )
```

| symbol | provenance |
|---|---|
| μ̂ · a(u) | the units handed over during u's wait: the intake length in units |
| 1/δ | Copa's equilibrium standing queue in packets (paper §8.2, cited) |
| worth | one queued unit costs δ, from Copa's equilibrium δ·Q = 1. That adds no dial, but it makes the worth absolute: it has no tie to T |

- **Limits.**
  - **T = ∞:** τ_b = R + 1/(δ·μ̂) is finite, so it drops under overload. **Requirement 1 fails.**
  - δ → 0: τ_b → T.
  - δ → ∞: τ_b → R, so the hold is 0.
  - **Zero headroom, with μ̂ = +∞ (no evidence):** 1/(δ·∞) = 0, so τ_b = R. That drops everything older than the newest unit even at headroom. With the current send rate in place of μ̂, it drops at drain rate 11 750/s, after 1/(50·11 750) ≈ 2 µs. **Requirement 2 fails** either way.
- **OV:** p50 ≈ 0.093 s and delivered ≈ 0.767 at every T, **including T = ∞** (Revision 3 there: 1.000).
- **CL-long:** after the outage the backlog is dropped.
  - At T = 0.4 that is about 3 850 units, −1.9 points on time.
  - At T = 2 it is about 15 000 units, −7.5 points.
- **Risks.**
  - **Oscillation:** none.
  - **Store pause:** every release after a pause drops the backlog.
  - **Floor law:** units sent young are protected (§4.1).
  - **Two paths:** pooled μ̂.
- **Rejected as a cap.** What survives is δ as the price of a bet whose stake is set by T and whose odds come from the forecast (§2). Worth is then one unit of share, and δ is the exchange rate.

### (c) Newest-first service within the lifetime

```text
   at the dequeue send   u*  =  argmin { a(u) : u in the intake ,  a(u) + R < T } ;     older units wait, and expire by §13.2
```

| symbol | provenance |
|---|---|
| a(u), R, T | as in §3 |
| argmin over ages | a service order, not a law. With one unit waiting it is FIFO |

- **Limits.**
  - **T = ∞:** under any backlog the order is newest-first for ever. It reduces to FIFO only through a selector, which is the mode bit §13.2 refused. **Requirement 1 fails**, because a T = ∞ byte stream is reordered.
  - δ: no seat.
  - Zero headroom: newest-first.
- **OV:**
  - p50 ≈ 0.093 s and delivered ≈ 0.767 in steady state.
  - At the end the old backlog still under T − R is served, so there is a Revision-3-like flush: ≈ 0.79 at T = 0.4.
  - The flushed units are late at T ≥ 0.6.
- **CL-long fails requirement 2.**
  - After the outage the surplus, about 1 750–2 500/s, serves the backlog from its newest end, while fresh hand-overs go first.
  - The oldest backlog units then wait `b/h` ≫ T − R instead of `b/μ̂` ≤ T − R.
  - Most of the λ·(T − R) ≈ 3 900 units FIFO delivers on time expire. That is −1.5 to −2 points on time, against leads of 1.3–2.7 points.
- **Order.**
  - `emit` assigns the seq at the dequeue, so the wire stays in seq order. **H and the floor cannot see the reordering.**
  - The application receives units out of hand-over order, which the transport can neither hold nor count.
  - A strict-order consumer (H = ∞) gets seq order, which is no longer its own order.
  - Fixing it needs an application-order field on the wire, which breaks requirement 5.
- **Risks.**
  - **Oscillation:** none.
  - **Store pause:** as in §5.1.
  - **Floor law:** units sent young are protected.
  - **Two paths:** unchanged.
- **Rejected.**


### (G) The recommended law: a bet on the overload persisting, priced in U (§2, §3)

- **Formula, symbols, floor and reading:** §3.1.
- **μ̂:** §3.2.
- **The clamp it ships with:** §3.3.
- **Predictions:** §4.
- **Risks:** §5.1.

It meets requirements 1 to 3 and 5:
- **requirement 1,** by arithmetic;
- **requirement 2,** by identity where λ̂ ≤ μ̂, with CL-long's drain at a 9.8–21 % margin;
- **requirement 3,** with no branch and one gauged `(·)⁺`;
- **requirement 5,** sender-only.

**Requirement 4 is met except for one step.** "The throughput term is read over the overload's age" (Q24) is the open step, and it blocks shipping.
