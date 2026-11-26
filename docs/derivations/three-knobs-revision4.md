# Three knobs, Revision 4: a sender-side wait limit under sustained overload, and the engine's repair clamp (derivation, no runs)

**Status.** Design and derivation. §1–§8 were written before any run. §9 (2026-11-26) reports the step 2 sizing run, reported only, and amends §3.3. §10 (2026-11-26, derivation) adds the floor-repeat addendum. §11 (2026-11-26, derivation) adds resend eligibility and the new model's gate. §12 (2026-11-26) is the confirmation registration, written before any data of its cells.
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

> **Superseded in part by §13 (2026-11-26).** Under the policy the refund premise below fails, so the odds t_ov/t_r do not apply. §13 re-derives the bet for the policy.

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

> **Superseded by §13.3 (2026-11-26)** for build B′. The law below stays as build B's record.

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

**Measured** (step 1b, coordinator, 2026-11-26; in_flight counts every unreported datagram, first sends, copies and repairs; **cwnd_sim = the cell's pause B = 1 000**; an earlier write-up said 2 048, a typo since corrected):

| | T = 0.4 | T = ∞ | RS at T = ∞ | RF3 |
|---|---|---|---|---|
| `spare_bind`, CL-long, RL (share of all first sends) | 0.12 | 0.67 | 0.77 | 0.97 |

- **Elsewhere:** about 0 in CL-long-v, Ec and RT.
- **In-flight in the post-outage drain peaks at 1 230–1 460, above B.** The store pause gates on outstanding seqs, not on datagrams in flight. Repairs, copies and unreported first sends sit above the seq count.

**The bind condition.**
- **At B = 1 000:** the clamp binds whenever `in_flight > B/(1 + r)`, about 900 at r = 0.107. That covers most of the drain, where in_flight is 1 230–1 460, so spare is negative and r_eff = 0 there.
- **Why the fraction rises with T:** it is measured over the whole run, and the drain takes a larger share of the run as T grows. At T = 0.4 the intake rule keeps only λ·(T − R) ≈ 3 850 units of backlog, against about 15 000 at T = ∞.
- **At cwnd_sim = 2 048** the threshold is about 1 850, above the drain's peak. So the clamp would not bind in CL-long at all.

**Predictions for CL-long with the clamp on, RL, at both values** (from the drain arithmetic, written before step 2 has data; both will run).

- **The arithmetic.** Recovery after the outage is the backlog over the drain headroom `h = μ − λ`, with μ the first-send capacity `C/(1/(1 − ε) + r_eff)` and λ = 10 000. The backlog is the same in every mode at a given T.
  - **Repair off:** μ = 12 124, h ≈ 2 120/s.
  - **RL:** r falls to ≈ 0.088 once the queue raises srtt (registration §2.6), so μ ≈ 11 230 and h ≈ 1 230/s. That predicts recovery 1.7× repair-off, against ADR-0084's measured 4.8 → 8.6 s (1.79×), which checks it.
  - **Clamp on at B = 1 000:** r_eff = 0 while in_flight > 900, which is most of the drain. So h ≈ 2 120/s, the repair-off value, except where in_flight dips below 900 early and late in the drain.
  - **Clamp on at 2 048:** no bind in the drain, so h is RL's.

| T | repair off | RL (stage 3e) | clamp on, B = 1 000 | clamp on, cwnd_sim = 2 048 |
|---|---|---|---|---|
| ∞, on time | 0.6500 | 0.4485 | 0.62–0.65 | RL ± A/A |
| ∞, recovery | (stage 3e repair-off value) | ≈ 1.7× repair-off | 1.0–1.1× repair-off | RL ± A/A |
| 2, on time | 0.6531 | 0.4586 | 0.62–0.65 | RL ± A/A |
| 2, recovery | | ≈ 1.7× repair-off | 1.0–1.1× repair-off | RL ± A/A |
| 0.4 | | | RL within the A/A floor; recovery within the 0.1 s resolution | the same |

- **Why the B = 1 000 on-time share sits slightly below repair-off.** Outside the drain the repair still runs at r, which costs nothing with headroom. Inside the drain, the dips below 900 let a little repair through.
- **At T = 0.4** the backlog is short, so neither value moves the reading.
- **Every clamp-on value must lie between repair-off and RL.** A value outside that interval is a finding. **The 2 048 rows must equal RL within the A/A floor,** with `spare_bind` ≈ 0 in CL-long.

**cwnd_sim = B is a modelling choice.** The engine's cwnd is BBR's under quinn by default, and the anchor of paper §8.3 holds it at about BtlBw·RTprop or more, so the bind fraction depends on that choice.
- **The conclusion** "the clamp binds in overload and drain windows, not elsewhere" holds for:

  ```text
     (1 + r)·in_flight_steady   <   cwnd   <   (1 + r)·in_flight_drain_peak
  ```

  - in_flight_steady is λ·RTT plus the outstanding holes, about 110–150 in CL before its outage.
  - in_flight_drain_peak is 1 230–1 460.
  - So the conclusion holds for **cwnd roughly between 200 and 1 400 datagrams** in CL-long, and B = 1 000 is inside the range.
- **Above it** (2 048, for example) the drain never binds, so the clamp buys nothing in CL-long but still binds in OV, where in_flight sits at the cap.
- **Near or below the BDP** (about 125–250 at 12 500/s and 10 ms RTprop), the clamp also binds in CL's steady state at 80 % load, which changes the call and CL rows.
  - If the engine read finds its cwnd there, the "≈ 0 outside overload and drain" claim fails.
  - The clamp model must then take the engine's cwnd law, and not B.
- **The gauge-first step reports in_flight quantiles per cell,** so this range can be checked against any cwnd without a rerun.
- **The CL-long RL rows change** at B = 1 000. They are re-baselined, not inherited.
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
   - Both cwnd_sim = B = 1 000 and 2 048 run. Each run states its cwnd_sim, because the bind fraction depends on it. At 2 048, CL-long's `spare_bind` is ≈ 0 and its rows equal RL.
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

## 9. Sizing results (reported only, 2026-11-26)

**Source.** Branch `toy-stage3e` at `d605afa2`: `tools/toy/results-stage3e.md`, the step 2 sizing run and items 3–8. 3 seeds, no verdict. The build sits behind config fields that default off, and with them off it reproduces the stage 3e identity sample. Readouts below are delivered / on time within 0.4 s / delay p50.

### 9.1 Predictions met and missed

**CL-long, clamp on, RL (§3.3; registered before the run):**

| T | repair off | RL, clamp off | clamp on, B = 1 000 | clamp on, 2 048 |
|---|---|---|---|---|
| 0.4 | 0.9406 / 0.9406 / recovery 0 | 0.9392 / 0.9392 / 0 | 0.9399 / 0.9399 / 0 | = clamp off |
| 2 | 0.9993 / 0.655 / 5.6 s | 0.9990 / 0.463 / 9.27 s | 0.9990 / 0.638 / 5.87 s | = clamp off |
| ∞ | 1.000 / 0.652 / 5.6 s | 1.000 / 0.453 / 9.47 s | 1.000 / 0.633 / 6.00 s | = clamp off |

- **Met:**
  - the clamp-on rows lie between repair-off and RL;
  - on time 0.633–0.638, inside the predicted 0.62–0.65;
  - recovery 1.05× and 1.07× repair-off, inside the predicted 1.0–1.1×;
  - at 2 048 the clamp never binds, and the rows equal RL;
  - T = 0.4 shows no change;
  - the wait limit is inert in CL-long (0 ticks with τ < T).
- **Missed:** the §3.3 version before the B correction had predicted 1.15–1.25× and on time 0.58–0.61. It was superseded before the data.

**OV, the 2×2 at T = 0.4, δ = 50, RL:**

| | measured | predicted |
|---|---|---|
| wait limit off, clamp off | 0.793 / 0.793 / 0.390 s | stored 0.7929 |
| wait limit off, clamp on | 0.833 / 0.833 / 0.390 s | 0.837 / 0.837 / 0.39 s, met |
| wait limit on, clamp off | 0.756 / 0.756 / 0.105 s | 0.770 / 0.770 / 0.095 s, **missed by −0.014** |
| both | 0.809 / 0.809 / 0.094 s; steady window 0.803 / 0.803 | 0.79–0.81 / 0.79–0.80 / 0.095–0.10 s, met |

- **The cause of the miss, inferred and not yet gauged.** The prediction for the wait limit alone took ρ ≈ 0. But the units it sends are young, so they are protected and get resent. Those resends add on top of the unclamped repair: `C/((1 + r + ρ)·λ)` = 10 000/(1.087 + 0.031)/12 000 ≈ 0.745, plus the start-up, which gives 0.756. The arithmetic matches.
- **Other OV rows:**
  - T = 2, δ = 50: 0.809 / 0.808 / 0.094 s. Met.
  - δ = 0.5 at T = 0.4: 0.809 / 0.809 / 0.094 s. Met.
  - δ = 0.5 at T = 2: 0.809 / 0.439 / 0.512 s, against 0.81 / 0.40 / 0.7 s. The delay is lower than predicted, the same direction as the OV-end miss below.
  - T = ∞: inert. Met.
- `collapse_t`: 1.34–1.41 s at T = 0.4 (δ = 50), 1.76 s at T = 2 (δ = 50), and 6.0 s at T = 2 (δ = 0.5).

**OV-end:**
- **T = 0.4:**
  - Revision 3 with the clamp: 0.929 / 0.929 / 0.156 s.
  - Revision 4 at δ = 50 and at δ = 25: 0.911 / 0.911 / 0.028 s, indistinguishable, as predicted.
  - The cost is 0.018, against a predicted 0.019. Met.
- **T = 2, δ = 0.25:** 1.000 / 0.310 / 0.697 s, against Revision 3's 1.000 / 0.308 / 0.701 s. The predicted gain of at most 0.01 holds. Met.
- **T = 2, δ = 0.5: missed.**
  - Measured: 0.956 / 0.522 / 0.365 s. In the [9, 15] s window on time is 0.999, against Revision 3's 0.426.
  - Predicted: a loss of 0.074 delivered and a gain of at most 0.01 on time. Measured: a loss of 0.044 and **a gain of +0.21**.
  - The cap cleared the backlog before the overload ended: there was no `collapse_t`, and τ < T on 50 030 ticks.
- **A δ sweep at T = 2 is smooth and monotone,** with no step at 0.5. The predicted "gain ≤ 0.01" holds only up to δ ≈ 0.3.

**OV-crit:**
- Revision 3 and Revision 4 with the clamp are identical: 1.000 / 1.000 / 0.093 s, with 0 ticks of τ < T. Met.
- Today's Realtime:
  - stored arm, unclamped: 0.999 / 0.999 / 0.131 s, higher than the predicted 0.91–0.95;
  - with the clamp, both arms: 0.999 / 0.999 / 0.091 s.

**K-OV (B = 8): missed for both revisions.**
- Revision 3: 0.539 / 0.539 / 0.307 s.
- Revision 4: 0.540 / 0.539 / 0.239 s.
- Predicted 0.72 delivered and 0.66–0.72 on time.
- **Repair is not the cause.** The clamp cut repairs from 503 to 20, and delivered returned to the repair-off value.
- **Superseded by §10.1:** the measured cause is the floor repeat, taking 25 % of the link. The inference that follows is withdrawn.
- **The miss is in the base arithmetic** `(C/λ)(1 − ε)`, which assumes the link is saturated. 0.54·50 = 27 units/s delivered, against 36 available. The inference is that the 8-seq store pause, with holes held in the outstanding count across an RTT of about 0.2 s, underfills the 40/s link. That is a B-sizing question for the cell, not a Revision 4 one. It is unconfirmed; `link_util_w` is the gauge to read.
- **Today's Realtime in K-OV:** the stored arm reads 0.740 / 0.007 / 1.52 s, and the ≤ 0.1 on-time prediction is met. The engine-queue arm reads 0.917 / 0.007 / 10.3 s: delivered met, on time near the predicted 0.03, p50 higher than the predicted ≈ 7 s.

**Today's Realtime, other cells (clamp on, B = 1 000):**
- **OV, stored arm:** 0.817 / 0.815 / 0.171 s; steady window 0.808. Met.
- **OV, engine-queue arm:** 0.873 / 0.160 / 0.724 s; steady window 0.815 / 0.
  - Delivered is inside the predicted range. On time (predicted 0.20–0.25) and p50 (predicted 0.65–0.70 s) are slightly outside.
- **OV-end, stored arm:** 0.916 overall, 0.808 in the overload window and 1.000 after.
- **OV-end, engine-queue arm:** 0.955 / 0.529 / 0.355 s; the overload window reads 0.837 / 0 and the window after 0.997 / 0.997.
  - On time 0 in the overload window: met. The share there (0.837 against a predicted 0.808) was missed.
- **The engine-queue arm with the clamp off is not run.** The engine has the clamp, so that arm has no engine counterpart (coordinator's amendment).
- **Also not run:** the strict-order (H = ∞) row of §4.1.

### 9.2 What the OV-end miss implies for Q24

The OV-end prediction and Q24's horizon step rest on the same arithmetic, `K = δ·(x − 1)·t_ov²`, with x and the onset of t_ov taken from §4's estimates.
- The miss: the cap fell below the on-time line well before predicted. Equivalently, the "≤ 0.01" boundary sat at δ ≈ 0.3 instead of above 0.5.
- That means the effective K was about 1.7 times the estimate at the same δ. The cause is not identified. Candidates:
  - x read higher than 1.25, for example D̂ under-reading by ε under unprotected sends (§3.2);
  - μ̂ became finite earlier than the assumed ≈ 1 s;
  - a different exponent on t_ov.
- **So the sizing run supports the law's shape** (continuous, monotone in δ, no step at a preset, inert where it should be) **but not its scale.**
- Q24 asks whether "horizon = the overload's age", that is, the t_ov² term, is right. A factor of about 1.7 in K that nobody has explained cannot be counted as evidence for it. It could equally come from the inputs or from the exponent.
- **Q24 therefore stays open, with its evidence status: shape confirmed, scale unexplained.**
- **The next step, registered before any rerun.** From the stored OV-end δ-sweep rows, fit the logged τ, K, x and t_ov per tick against `δ·(x − 1)·t_ov^k`. Read k and the input bias separately. Only a fit with k = 2 and an identified input bias would restore the prediction's standing.

### 9.3 Amendments to §3.3 (dated 2026-11-26)

1. **The send gate as written deadlocks after an outage.** "No payload staged while in_flight ≥ B" never reopens. Datagrams lost in the outage are never reported, so they stay counted in flight.
   - The frozen build uses **the engine's in-flight expiry** (`scheduler/path.rs:1169–1184`): an unreported datagram leaves in_flight after max(4·SRTT, 250 ms).
   - That is the engine's existing rule, not a new constant. Its 250 ms floor keeps whatever ledger standing it has in the engine.
   - The sizing run itself did not build the gate; the store pause on outstanding seqs gated alone.
2. **The clamp on each taper grant** (`fec_rate.rs:729`) has nothing to apply to in the offline simulation, which has no taper stage. Only the cadenced accrual is clamped. The engine's taper clamp is recorded as not modelled.

### 9.4 Dominance under overload against today's Realtime, engine-queue arm

The comparison uses the registered rule (§3.1, §4.3), in OV's steady window, Revision 4 with the clamp against the engine-queue arm with the clamp:

| readout | Revision 4 minus the engine-queue arm |
|---|---|
| on time within 0.4 s | +0.65 |
| delay p50 | −0.63 s |
| delivered | −0.012, against a bar of 0.009 |

- **As registered, this reads "not dominated": it is a trade, not a win.** Revision 4 gives up 1.2 points of steady delivered share, beyond the bar, for 0.65 of on-time share and 0.63 s of delay.
- The delivered deficit matches the engine-queue arm's steady share of 0.815 against Revision 4's 0.803.
- **The likely source of the 1.2 points** is the young units' resends plus residual repair (§9.1, OV). That is an inference and is not gauged here.
- **Against the stored arm** the steady delivered share is 0.803 against 0.808 (−0.005, inside the bar), and the delay is better.

## 10. Addendum: the floor repeat says only what the receiver may not know (2026-11-26; derivation, no runs)

### 10.1 The measured cause in K-OV

Stored rows, new model at T = 0.4, repair off, measured by the coordinator:
- **603 standalone floor datagrams in 60 s,** one per firing of the blocker timer. The timer fires every 100 ms, the sweep period `clamp(2·srtt, 25 ms, 100 ms)`, whose cap binds under a 200 ms RTT (Q17).
- **That is 25 % of the link's 2 400 slots** at 40/s.
- **The rest:** 3 resends, 0 repairs and 2 duplicates.
- **First sends:** about 1 794, delivered at 90 %, which gives the 0.54 delivered.

**So the floor repeat holds the new model at 0.54 against 0.72 carryable.** Repair and the sweep's resends do not. This replaces §9.1's inferred K-OV cause ("the 8-seq store pause underfills the link"), which is withdrawn.

**Why it fires.**
- The offline simulation's `on_blocker` calls `floor_repeat`. That sends F_ann whenever `F_ann > A_known` (`toy2.py`, `floor_repeat`; §17.3's "each fire also … repeats it while F_ann > A_known").
- Under overload at T = 0.4 the units are unprotected at birth (§15.3), so F_ann sits at the send frontier.
- A_known is the lowest seq not yet acknowledged, which includes one RTT of data still in flight. So `F_ann > A_known` holds at every fire, even when no hole lies below F.

### 10.2 When a standalone floor adds information

- **The floor already rides on every data datagram** (§15.3, header carrier Q15(a); `carry_floor`). A datagram sent at time t carries `F_ann(t)`.
- **The receiver uses the floor for one thing only:** to pass a hole below it (L6; §15.3).
- **So a standalone floor tells the receiver something new only if two things hold:**
  - the receiver may hold a hole at some seq s < F_ann;
  - and the sender has no evidence that any datagram carrying a floor above s has arrived.
- **Both are already measured** (§13.1):
  - the hole is "evidence of loss": a later datagram arrived, or s is overdue by R;
  - the arrival is the echoed largest send ordinal the receiver has received (§10.3 says why an ordinal and not a stamp).

### 10.3 The repeat law

```text
   at each repeat instant (the sweep or blocker fire, unchanged):
      send the floor   ⇔   ∃ s ∈ [A_known, F_ann) :   s unresolved   ∧   loss_ev(s, now)   ∧   rx_ord  <  carry_ord(s)
      carry_ord(s)     =   the send ordinal of the first datagram (data, copy, repair or floor) that carried a floor value > s     (+∞ if none yet)
      loss_ev(s, now)  ⇔   ord(last send of s) < rx_ord        (knowledge: a datagram sent later has arrived)
                        ∨   now ≥ t_newest_send + R             (silence: nothing sent has been followed by a report for R)
```

| symbol | provenance |
|---|---|
| F_ann, A_known, "unresolved" | sender state (§15.3, ADR-0084) |
| send ordinal | sender state: one counter per datagram sent, the offline simulation's `ps_ctr`. In the engine it is the QUIC packet number |
| rx_ord | measured: the largest send ordinal the receiver has received, echoed in its report. It is RFC 9002's "largest acknowledged" (§13.1), used in place of the send stamp τ_rx. In the engine it already comes with QUIC's acks; whether raptorpath's own report carries it is Q14 |
| carry_ord(s) | sender state: the floor is a running maximum, so carry_ord is non-decreasing in s. It is kept as a monotone list of (value, first carrying ordinal), pruned below A_known |
| t_newest_send, R | sender state, and the ack timeout of §13 |
| the repeat clock | unchanged: the sweep or blocker period. Its cap is Q17 and is not touched here |

**Why ordinals, not send stamps** (advisor review).
- The work-conserving source stages a whole burst at one instant. With stamps, an earlier member of the burst that carried a lower floor would count as "the carrier arrived".
- In the last burst before a pause, or at the end of a run, an early member can arrive while the later members are lost and nothing follows. The repeat would then be suppressed for ever, and the run would not finish. That is §18's S5v gap, "a hole sent in the same instant as the newest arrival".
- Ordinals are strictly ordered, so `rx_ord ≥ carry_ord(s)` holds exactly when the carrier itself, or a datagram sent after it, has arrived.
- **The ties go the conservative way:** any doubt keeps the repeat.

**Why the silence branch reads the newest send, not s** (§13.1's timeout reads s).
- §19 found R only about a tick above srtt. With a per-seq timeout, every in-flight datagram whose rtt exceeds R by queue jitter would become "evidenced" and would fire a repeat. Under flow that is many of them.
- The newest send is overdue only when the path has gone silent: an outage, a pause with nothing in reply, or a lost tail. Those are exactly the cases where no data can carry the floor.
- Under flow the newest send is always younger than R. So in-flight data that nobody has reported triggers nothing, which is the K-OV case.

**What the law is.**
- One predicate on sender state, with no rate, no dial and no constant. Nothing compares δ, T or ρ with anything, so there is no mode bit.
- **An efficient form.** carry_ord is non-decreasing in s, so it is enough to test the highest evidenced seq: `s* = max{ s < F_ann : s unresolved ∧ loss_ev(s, now) }`.
- **Scope.**
  - It holds under the header carrier only (Q15(a)).
  - Under the datagram carrier (Q15(b), arm S4b) every floor is standalone, and this law does not apply.
  - **Two paths are not predicted.** The standalone floor went on every live path (§17.7). A carried floor rides the one path its datagram takes. The P2 cells must be re-derived before any claim.
- **It supersedes:**
  - §15.3's liveness bullet (the sweep sends F every P while F > A_rx);
  - §17.3's bullet "each fire also … repeats it while F_ann > A_known".

**Shape against the sentence it implements** ("repeat the floor only when the receiver may not know it"):
- **A hole detected under flow** is known through a later arrival, which raises rx_ord past that datagram's ordinal.
  - If s was unprotected when that datagram was sent, the datagram carried F > s, so no repeat.
  - If the first carrier was lost and a later one arrived, rx_ord is at least the later ordinal, which is at least carry_ord(s). So again no repeat, correctly: the later one carried F > s too.
- **A hole that becomes unprotected after birth** (Revision 4's young units, after their resends fail) can repeat only in the gap between t_pass and the report of the next carrier, about one RTT.
- **After an outage, or with a lost tail,** nothing is sent or answered. The silence branch fires after R, and the floor repeats on the same clock as today.

### 10.4 Checks of the inertness premise, against the code

1. **Every datagram whose arrival can raise rx_ord carries the floor.** In `toy2.py`, `carry_floor` is called:
   - for first sends, in `emit`;
   - for copies, in `send_copy`'s `fin`;
   - for repairs, in `stage_repair`.

   Floor datagrams carry their own value. **Checked.**
2. **The guard `F_ann > lu` cannot be false while an unresolved s < F_ann exists.** lu is the lowest unresolved seq as of the last report, `min(M_known)` or `rhi_known + 1`. Any unresolved s satisfies s ≥ lu, so F_ann > s ≥ lu. **Checked.**

**Consequence.**
- A repeat is suppressed only when a datagram carrying a floor above every evidenced hole has arrived at the receiver. The receiver has then already passed those holes.
- So the instant at which each hole is passed is unchanged, and only floors that pass nothing new are removed.
- **What changes:**
  - the link slots those floors took, which is the gain;
  - the floor count;
  - and, through the freed slots, the trace of every link cell.
- **It is a model change of the new model, not an identity.** The new-model rows are re-baselined under it.
- **The readouts must not move beyond the A/A floor** where repeats were a negligible share of the link.
- **Today's designs are untouched.** ADR-0084's verdict floor keeps its own repeat, and today's Realtime sends no floor.

### 10.5 Predictions (written before any run) and the tests

**Repeat share of the link today.** At most one repeat per fire at period P = clamp(2·srtt, 25 ms, 100 ms), so the share is at most `1/(P·C)`:

| cell | P, C | share ≤ |
|---|---|---|
| K-OV | 0.1 s, 40/s | **25 %** (measured 603/2 400) |
| OV | 0.1 s, 10 000/s | 0.1 % |
| CL-long, CL | 25 ms, 12 500/s | 0.3 % |
| RT, the call cell | no link | 0 |

**K-OV, T = 0.4.**
- **Residual repeats, derived.**
  - **Revision 3:** units are unprotected at birth, so every known hole already has an arrived carrier. That leaves only the silence branch, which needs the newest send to be overdue by R while the store pause (B = 8) holds the sender. At 40/s with srtt ≈ 0.2 s the pause clears on every report, so silence is rare.
  - **Revision 4:** its young units are protected. Holes that become unprotected after a failed resend arrive at about 4/s × pc 0.1 ≈ 0.4/s, and each opens one window of about one RTT. At P = 0.1 s that is up to about 2 fires per hole.

  | | repeats predicted | `rep_fired_by_timeout` (registered) |
  |---|---|---|
  | Revision 3 | 0–30 in 60 s | 0–30 |
  | Revision 4 | 20–60 | ≤ 30 |

- **Delivered and on time.**
  - **Revision 3:** about 570–600 slots are freed, so first sends ≈ 2 360–2 390, delivered at 90 %. **Delivered and on time ≈ 0.70–0.72**, against `(C/λ)(1 − ε)` = 0.72. p50 is unchanged at about 0.31 s.
  - **Revision 4 with the clamp:** the same band. Its resends take ρ ≈ ε/(1 − ε) = 0.11, so `C/((1 + ρ)·λ)` ≈ 0.72. p50 is about 0.24 s.
- **If the timeout count exceeds its band,** R's honesty (§19) is the cause to read first.

**OV:** the floor count falls. delivered, on time and p50 stay within the A/A floor.

**CL-long:**
- During the outage nothing is sent or answered, so the silence branch repeats on every sweep, as today.
- After the outage, the first arrivals carry F, and the repeats stop.
- The hole-pass instants are unchanged, so recovery is the same within the 0.1 s resolution, and on time and delivered are within the A/A floor.

**CL, CLw-long, the K cells and the E cells:** within the A/A floor. The E cells' lost tail with nothing after it keeps its repeat by the silence branch.

**Tests (red first).**
1. **The receiver holds the floor at every suppressed repeat.** This is an absolute check on one clock in one run: whenever a repeat is suppressed, the receiver's own floor F_rx at that instant exceeds s*. Zero exceptions. It replaces the cross-run comparison of pass instants, which is ill-defined once traces diverge.
2. **A planted equal-stamp burst.** A burst staged at one instant: its first member arrives, the rest are lost, and nothing follows.
   - The repeat fires after R, the tail is passed, and the run finishes.
   - It is red against the stamp-based form (`τ_rx ≥ t_carry`), green with ordinals.
3. **Liveness.**
   - After any outage (CL-long, the K cells, E-I2), the receiver passes the post-outage holes within one repeat period plus one owd of the outage end.
   - In the E cells every run finishes (`finished`).
4. **Execution witnesses:**
   - `rep_suppressed` > 0 in K-OV and OV;
   - `rep_fired_by_timeout` > 0 through CL-long's outage window;
   - the floor share of link slots in K-OV is at most 3 %.

**Gauges:** `floor_rep` per second, `rep_suppressed`, `rep_fired_by_timeout`, `rep_fired_by_knowledge`, and the floor share of link slots.

### 10.6 Note on the engine-queue arm's steady share (§9.4)

The registered steady window [3, 9] s for today's Realtime, engine-queue arm, reads 0.815. **Second 3 of that window** falls while the 4 MiB queue is still filling (it fills at ≈ 4.1 s), and reads 0.850. **Seconds 4–9 read 0.809**, which is the link's carryable 0.808.

So the steady window as registered includes a filling transient, and the arm's true steady share is 0.809.
- Against that, Revision 4's 0.803 is −0.006, inside the 0.009 bar. The −0.012 of §9.4 is mostly the transient.
- **The dominance reading is not re-run here.** It stays as registered ("not dominated"). The transient is recorded so the next registration can start the window after the queue is full: from `fill time + 1 s`, computed from the arm's own queue-length gauge, not from a declared second.

### 10.7 Amendment: the repeat waits until the newest covering carrier is overdue (2026-11-26)

**Measured** (K-OV, Revision 4 with no gate): 94 of 95 repeats fired while a datagram carrying the covering floor was still in flight and not yet overdue, that is, before its send + R. §10.3's rule repeats as soon as the *first* carrier has not been reported. But a later carrier that is still in flight will tell the receiver the same thing.

**The law (replaces the predicate of §10.3; the ordinals are kept):**

```text
   send the floor   ⇔   ∃ s ∈ [A_known, F_ann) :   s unresolved   ∧   loss_ev(s, now)   ∧   rx_ord < carry_ord(s)   ∧   now  ≥  t_newest_carrier(s) + R
   t_newest_carrier(s)  =  the send instant of the newest DATA datagram (first send, copy or repair) that carried a floor value > s
                           (−∞ if none has; then the clause holds; standalone floors do not count, so silence repeats keep the clock P)
```

| symbol | provenance |
|---|---|
| R | the ack timeout already defined (§13; RFC 6298 §2 and RFC 9002 §6.2.1): the time after which an unacknowledged send is overdue. No new constant |
| t_newest_carrier(s) | sender state. It is the send instant of the newest data datagram whose carried floor exceeded s. Under flow it is the newest send, because the floor is a running maximum. **Standalone floors are excluded.** Counted as carriers, they would space silence repeats by R instead of P, which breaks "the same clock as today" wherever R > P (the K cells with the cap binding; cold start, R = 1 s). The waste measured in K-OV came from data carriers |

- **Why R, and not RFC 9002's 9/8 time threshold.** The 9/8 constant (RFC 9002 §6.1.2) is a reordering allowance for declaring a packet lost. Here the question is whether its report is overdue, which is R's job everywhere else in this design. It also adds no constant.
- **Under flow:** the newest carrier is always younger than R, so no repeat fires. That is the 94 suppressed in K-OV.
- **Silence** (an outage, or a lost tail): nothing is sent. After R the newest carrier is overdue, so the floor repeats on the same clock as today. **Liveness is kept.**
  - The clause also subsumes the silence branch of loss_ev, because the newest send is at least as new as the newest carrier. loss_ev keeps it for the case where nothing above s carried a floor.
  - With no carrier at all, t_newest_carrier is −∞ and the clause holds, so the floor is sent.
- **R's honesty (§19)** decides the residual. A carrier whose rtt exceeds R through queue jitter lets one repeat through: at most one per fire, and gauged as `rep_fired_carrier_overdue`.
- **Two paths (P2, 36 runs, Revision 3, measured with §10.3's rule):** every run finishes and the readouts are identical. The rule saves nothing there and breaks nothing. The amendment only removes repeats, never the silence case, so it is predicted the same there.
- **Predicted:**

  | cell | repeats | readouts |
  |---|---|---|
  | K-OV, Revision 4 | 95 → ≤ 10 in 60 s | +about 85 slots for first sends (§11.3) |
  | K-OV, Revision 3 | ≤ 10 | ≈ 0.715–0.72 |
  | OV, CL-long | fall further | within the A/A floor |

  CL-long's outage repeats are unchanged (silence).
- **Tests added:**
  - an absolute same-run check: every repeat fires at least R after the newest covering carrier was sent;
  - the planted lost-tail burst of §10.5 test 2 still finishes;
  - `rep_fired_by_timeout` > 0 through CL-long's outage.

## 11. Addendum: resend eligibility, and the gate the new model should use (2026-11-26; derivation, no runs)

### 11.1 The measured cause: resends of copies whose original can still arrive

**K-OV link slots per run** (measured; with the clamp and §10's rule, no gate):

| | first sends | resends | repairs | floors | delivered |
|---|---|---|---|---|---|
| Revision 4 | 2 077 | 188 (126 arrive twice: the original lands after the resend) | 46 | 95 | 0.644 |
| Revision 3 | 2 371 | 4 | — | — | 0.715 |

- **Under Revision 4 the units are young, so they are protected and resendable.** The engine-like blocker sweep resends the cumulative blocker after `max(last_send, last fire) + P`, with `P = clamp(2·srtt, 25 ms, 100 ms)`.
- Under K-OV's 200 ms RTT the 100 ms cap binds. **So P is below the round trip, and the sweep resends a seq whose original is still in flight.** That is Q17's defect ("sweep eligibility should be max(P, R)", §17.10), now measured as 126 wasted slots of 2 400.
- Revision 3 does not see it: its units are unprotected at birth, so they are never resent.

### 11.2 The law

"A timer may resend a seq only once its last send is overdue" (RACK's rule, RFC 8985 §6.2: a packet is lost only after the time threshold since its send; here the threshold is R, as in §13.1):

```text
   blocker deadline   =   max(  last_send(b) + max(P, R) ,   last_fire + P  )          b = A_known, the cumulative blocker
   P_reach(b)         =   max(P, R)                                                    (the same function feeds §17.1's trigger delay d(s) and the floor's protected())
```

| symbol | provenance |
|---|---|
| P | the engine's sweep period `clamp(2·srtt_pooled, 25 ms, 100 ms)` (`recovery_laws.rs::tail_sweep_timeout_us`). It is unchanged, and it stays the repeat clock for the floor |
| R | the ack timeout (§13). No new constant |
| last_send(b) | sender state |

**Report-driven resends are unchanged.** A report that lists s as missing carries knowledge that a later datagram arrived (§13.1's evidence). On one FIFO path the original is then lost, so the resend is never a duplicate. At N ≥ 2 the reorder window of §17.7 applies.

**Inert where resends are already late enough.**
- max(P, R) = P exactly when P ≥ R, that is, `clamp(2·srtt, 25 ms, 100 ms) ≥ srtt + max(4·rttvar, g) + ack_delay_max`.
- Below the cap that holds once srtt ≥ max(4·rttvar, g) + ack_delay_max, about 4–6 ms in the offline simulation.
- With the cap binding it holds only while srtt ≤ 100 ms − (R − srtt).
- **So the law is bit-identical wherever srtt stays below about 95 ms** and above a few ms. Gauge `elig_R_bind`: the fraction of blocker deadlines at which R > P.
- **In the engine** the same change also touches Bulk's resend timing. It is therefore its own guarded engine change, with off-Bulk stages (CLAUDE.md, `meas-guard-bulk-blind`). That is Q17's default.

### 11.3 Predictions (written before any run)

**K-OV, Revision 4 with the clamp and §10.7.**
- **P_reach also feeds `protected()`** (§17.1), so the new reach changes which seqs are resendable at all.
  - The intake wait is ≈ 0.239 − ô ≈ 0.09 s.
  - **Before**, the blocker's earliest resend was at 0.09 + P (0.1) + ô (0.15) = 0.34 < 0.4, so it was protected.
  - **After**, it is 0.09 + max(P, R) (≈ 0.21) + 0.15 ≈ 0.45 > 0.4, so it is **not protected**.
  - Non-blocker seqs already had d = srtt ≈ 0.2, so they were unprotected before too.
- **So, if the 188 measured resends were blocker-reach resends,** resends fall to a handful and the floor passes those holes at birth.
  - Registered band: **resends ≤ 20, duplicates ≤ 5.** The condition is checked from the stored trace's trigger field before the run is read.
- **Freed slots:** about 180 from resends and about 85 from floors.
- **The arithmetic:** first sends ≈ 2 077 + 265 ≈ 2 340, delivered at 90 %. Loss is no longer recovered by resend, and the clamp holds repair to about 0.
- **Predicted:**

  | | delivered | on time | p50 |
  |---|---|---|---|
  | Revision 4 | **0.70–0.71** | equal to delivered | ≈ 0.24 s |
  | Revision 3 | 0.715–0.72 | the same | unchanged |

  - **Revision 3 is unchanged by §11.2:** it never resends.
- **If the resends were mostly report-driven instead,** §11.2 does not touch them. Then the band is void, and the miss is read from the slot table before any further law.

**OV** (srtt ≈ 98 ms, P = 100 ms, R ≈ 102 ms):
- R exceeds P by about 2 ms, so `elig_R_bind` is ≈ 1 under overload and the sweep fires 2 ms later.
- Predicted within the A/A floor on every readout. Duplicates in OV are already rare, because the original's report arrives within srtt < R.

**CL-long:**
- Normal srtt is ≈ 11 ms, so P = 25 ms (the floor) > R ≈ 15 ms.
- In the drain, srtt ≈ B/C + RTprop ≈ 0.09 s, so P = 0.1 s (the cap). R ≈ 0.094 s only if rttvar ≈ 0.
  - At the outage end srtt steps from about 11 ms to about 90 ms, so 4·rttvar is briefly tens of ms, and R then exceeds the cap.
- **The registered prediction is therefore "within the A/A floor" on every readout.** Bit identity is claimed only if the witness reads 0.
- **The witness, `elig_R_bind`, is counted after the first RTT sample.** Before it, R is the 1.0 s seed against P ≤ 0.1 s in every cell, so the count would be non-zero by construction.

**Tests (red first):**
1. **No duplicate from the timer.** No timer-driven copy of s is staged before `last_send(s) + R`. This is absolute, from the trace.
2. **Identity where P ≥ R.** CL, CL-long and the call cells are trace-identical with and without §11.2, wherever `elig_R_bind` (counted after the first RTT sample) reads 0. Cells where it reads > 0 are compared within the A/A floor.
3. **K-OV executes.** `elig_R_bind` > 0, and the duplicates fall below 15.

### 11.4 The gate the new model should use

**Engine read** (coordinator, 2026-11-26):
- The engine's send gate on in-flight exists only when `reliable` (`net/mod.rs:2473`) and in generation mode.
- Its cap is `1.5 × Σ_p BtlBw_p·RTprop_p` (`RWM_INFL_BDP`, default 1.5; `sender_policy.rs:756–766`).
- At Realtime there is no gate.
- For K-OV the engine window would be about 12 datagrams, not the cell's B = 8.

**NO-MODE-SWITCH debt, recorded.** The gate is selected by `reliable`, a bit derived from ρ, so it is ρ-selected. It goes on the law ledger with `tx_paused` (§3.3), as engine sites to retire or bound with a test.

**What the new model's gate should use.** The new model has no ρ, so its gate cannot be selected by one. It applies at every T, as the store pause does. The default is the engine's existing law:

```text
   in_flight  <  W_gate ,      W_gate  =  1.5 · Σ_p BtlBw_p · RTprop_p          (RWM_INFL_BDP = 1.5, sender_policy.rs:756–766)
```

**Its provenance.**
- It is the engine's shipped window, with the same standing here as the spare clamp and the 250 ms in-flight expiry: the engine's law, carried as it is.
- **The 1.5 has no derivation in the paper.** It is recorded as ledger debt beside the ρ-selection, not adopted as derived. Shipping it under the three knobs needs a derivation (Q31).

**The link-busy check (the premise of §2 and Q28).** The gate must leave at least one report interval of drain in the link queue, or the link idles between reports.
- In OV the BDP is about 100 datagrams, and about 20 leave per 2 ms tick.
- At 1.5 × BDP the queue spare is 50 ≥ 20, so the link stays busy.
- At 1.05 × BDP it would be 5, so the link idles. D̂ then becomes gate-bound and reads low, which is §3.2's main risk.
- So the link-busy premise needs `(W/BDP − 1)·BDP ≥ C·g`, the drain in one report interval.

**The alternative under Q31:** paper §6.1's pooled-cap law, `(1 + q(δ))·Σ bw·RTprop` with q(δ) = (b(δ) + 1)/30.
- **For:** one bound with one provenance, shared with the store pause.
- **Against:** at δ ≥ 50, b(δ) is clamped at ½, so q = 0.05 and the window is 1.05 × BDP. That fails the link-busy check in OV and in the K cells, and B's knee is PINNED (F01).
- **Default: the engine's 1.5 × BDP,** with its debt recorded.

**Scope: the gate's cap only.**
- The repair clamp's spare (§3.3) reads the path's cwnd, which in the engine is the congestion window. The in-flight cap is `RWM_INFL_BDP`. They are two quantities unless an engine read shows them to be one.
- So the offline simulation keeps cwnd_sim for the clamp's spare (§3.3: B, and its 200–1 400 range in CL-long), and uses W_gate for the gate.
- Folding them together would put the clamp's cwnd at about 190 in CL-long, at the bottom edge of §3.3's range. The clamp would then bind in CL's steady state, and §9.1's met CL-long rows would be void.

**The offline simulation:**
- A cell's `pause_B` stays the seq bound.
- The datagram gate takes W_gate = 1.5 × D̂ × RTprop from the sender's measurements. It uses the engine's in-flight expiry of §9.3, max(4·SRTT, 250 ms), so it cannot deadlock.
- **K-OV:** W_gate ≈ 1.5 × 36/s × 0.1 s ≈ 5.4 datagrams on the propagation RTprop, below the cell's B = 8. So K-OV would be gate-bound.
  - The engine's 1.5 × BDP is about 12 on the coordinator's BDP. That implies RTprop ≈ 0.2 s, the queued RTT: a windowed minimum under a standing queue reads the queue (the RTprop limit recorded under Q18).
  - On the propagation RTprop of 0.1 s (2·owd) the engine's window would be about 6, which binds too.
  - Owner question **Q31:** whether the gate's window is the engine's 1.5 × BDP or the §6.1 law. **Default: the engine's 1.5 × BDP, with its debt recorded.** Either needs a derivation before it ships under the three knobs.
- **CL-long and OV:** W_gate ≈ 1.5 × 12 500 × 0.01 ≈ 190 and ≈ 1.5 × 10 000 × 0.01 ≈ 150 datagrams.
  - That is far below the in-flight levels of 880–1 460 measured with B = 1 000.
  - **Either window cuts the link cells' forward queue from about 88 ms to about 5 ms** (0.5 × BDP).
  - So §4's p50, R, ô and D̂ all move, and so do the readouts that depend on them.
  - **Not predicted here.** It needs its own registration before any run, which is the reason to decide Q31 first.

## 12. Confirmation registration (2026-11-26; written before any data of this part)

**Base.**
- The offline simulation on `toy-stage3e` at `8c743f0f`, the build that carries §3.3's clamp, §10.7 and §11.2 behind config fields.
- The sizing result it records: K-OV with §10.7 and §11.2 reads 0.716 for Revision 3 at p50 0.32 s, and 0.720 for Revision 4 with no gate at p50 0.166 s.
  - Revision 4 had 189 resends and 2 duplicates. The "≤ 20 resends" band of §11.3 was missed: the resends now come after R, and they recover real losses.
  - Every other cell was within the A/A floor, and P2 finished with identical readouts.
- **The cells below have not been read under any Revision 4 build,** except CL-long's new-model clamp rows (§12.4). This registration is written before any of their data.
- **Revised before any run** (2026-11-26, after the advisor's review of `70c976b2`):
  - the resend-eligibility fix goes on every design;
  - build B drops the send gate;
  - the scope is stated;
  - the video cells' delay is reported only;
  - comparisons use paired windows, and delay gets a bar;
  - CL-long's rows are labelled post-data;
  - the run time waits on a timing run.

**Scope, stated before data.**
- **Stage 3e's registered outcome for Revision 3 stands:** OV FAILS, and CL-long is UNDETERMINED under unclamped repair. §12 does not re-read it.
- **§12 is a new verdict,** for builds A and B, on its own cells, on one path.
- **Rows with unclamped repair are reported only.**

### 12.1 Builds and baselines

| code | build |
|---|---|
| **A** | Revision 3 (S5) + the repair clamp (§3.3, cwnd_sim = the cell's pause B) + the floor repeat of §10.3 with §10.7 + the resend eligibility of §11.2 (`elig = "rack"`) |
| **B** | A + the wait limit (§3.1, μ̂ = D̂ of §3.2). **No send gate.** |
| ADR-0084 at ρ ∈ {0.95, 0.98, 0.99, 0.995, 1} | with the clamp and `elig = "rack"` |
| today's Realtime, stored arm, at δ ∈ {0.5, 5, 50} plus the cell's δ | with the clamp and `elig = "rack"` |
| today's Realtime, engine-queue arm (§3.3), at the same δ | with the clamp and `elig = "rack"` |
| today's Bulk | with the clamp and `elig = "rack"` |

- **Shared machinery goes on every design.**
  - The repair clamp is the engine's, at every hint.
  - The resend eligibility of §11.2 is the sweep's timing, which every design's timer-driven resends use.
  - Without §11.2 on Bulk, the gate "S5 at T = ∞ ≡ Bulk" would fail by construction.
- **The floor-repeat rule (§10.3 with §10.7) applies to the new model only.**
  - It is part of the new model's floor law, the protected floor of §15.3.
  - ADR-0084's verdict floors are its own design, and they keep ADR-0084's repeat unchanged.
  - Today's Realtime sends no floor. Bulk sends none, because it never gives up.
- **No design has a send gate in §12.** With the gate, CL-long's repair-on rows would differ from A, so the "B ≡ A in headroom cells" stop would fire. Nor has the gate ever run together with §10.7 and §11.2. The gate (the §11.4 window, Q31) gets its own registration later.
- **Repair mode:** RL. At δ = 0.005, r = 0 exactly, so RL is the repair-off row by arithmetic and the clamp is inert.
- **The new builds sweep T as in registration §13.2:**
  - CL, CL-long, CLw-long: 15 points, plus H = ∞ at {0.4, 1.4, ∞};
  - OV-long: {0.4, 0.6, 1, 2, ∞}, plus H = ∞ at {0.4, 2, ∞};
  - the video cells: {1, 1.5, 2, 3, ∞}, plus H = ∞ at {1, 2, ∞};
  - CLc: {0.4, 1, ∞} × H ∈ {0, ∞}.
- **In the cells with headroom,** B differs from A only where λ̂ > μ̂ (§3.1). So B runs there at 3 seeds as an **identity check** against A at T ∈ {0.4, 2, ∞}: traces equal, `n_tau_lt_T` = 0. Without a gate this is an identity by arithmetic. It runs the full sweep only in the two overload cells.
- **Gates** (stop rules, as in registration §13.6):
  - S5 at T = ∞, H = ∞ (A and B) equals today's Bulk with the clamp and `elig = "rack"`, on every key and trace. Neither sends a floor at T = ∞, so the new model's floor rule is inert there;
  - every new-model run finishes. Today's Realtime is read with `unfinished` and `delivered_by_t_limit`, per the dated amendment;
  - §10.5's and §11.3's absolute tests hold in every run.

### 12.2 Readouts and the steady window

- **The readouts:**
  - the steady share delivered;
  - the steady share on time within the cell's τ (0.4 s, or 2 s in the video cells);
  - delay p50 from hand-over.
  - The outage cells also keep their registered recovery time.
- **The steady window, per arm, from its own queue gauge.** Its start is `max(fill, first RTT sample) + 1 s`. Its end is the last hand-over minus τ. "fill" is:
  - for the new builds, the first dequeue expiry (the intake at its cap);
  - for the engine-queue arm, the first instant its 4 MiB FIFO is full;
  - for the stored arm, the first link-queue drop;
  - for ADR-0084 and Bulk, the first store pause.

  If an arm's queue never fills (a cell with headroom), the window starts one second after the first RTT sample. That is the one rule, with no declared second.
- **One steady window per comparison.** Each comparison uses the later of the two arms' starts and the common end, so the per-seed differences stay paired.
- **In outage cells** the steady window excludes [outage start, the later recovery end]. The outage is read through the registered recovery time and the whole-run share by τ, as in stage 3e.
- **Bars.**
  - **For the two shares:** the registered form, `max(T_bar, NF)` with `T_bar = 0.05·(1 − share_ref)`.
  - **For delay p50:** `max(NF_p50, 0.05·p50_ref)`. The 0.05 is the same relative bar the registration already uses for shares, so no new constant is added.
- **A/A floors for the steady-window readouts.** The re-frozen A/A rows (§12.3) carry steady share, steady on time and steady p50, computed with the same paired-window rule (A against A′). NF for each is the p95 of |A − A′| over the comparison's seed count, frozen before any comparison at that count is read.

### 12.3 The reading rule

- **Matched share** (registration §13.3) applies where a build's T sweep spans a range of steady delivered shares: the outage and headroom cells.
- **Dominance** (§3.1) applies where the sweep collapses (`floor_bind` ≈ 1 at every finite T). It also applies wherever the steady delivered share does not vary with T, as in every overload cell, where it is the capacity share at every T.
  - Each baseline point is compared with the build's best finite-T point on the three readouts.
  - "Dominates" means not worse beyond the bar on any readout and better beyond it on at least one. Otherwise the result is "not dominated", a trade, and it is reported as such.
- **The video cells (δ = 0.005): delay p50 is reported only.**
  - Under the verdict there: delivered, and on time within 2 s.
  - The reason is the design record: at δ ≈ 0, delay inside the buffer is not priced (§4 of the derivation: buffered video is T = 2 s with δ ≈ 0), so delay cannot decide a comparison there.
  - So the OV-long-v prediction below is a comparison on share and on time only. Its delay difference is reported, and it feeds Q32.
- **Bars, NF and seeds** follow amendment 2 (§14.3):
  - 12 paired seeds in CL, CL-long, CLw-long, CLc, CLc-v and CL-long-v;
  - 6 in OV-long and OV-long-v, with the old senders there at 3 seeds at their dial ends (§14.4): ADR-0084 at ρ ∈ {0.95, 1}, today's Realtime at δ ∈ {0.5, 50} (or {0.005, 50} in OV-long-v), and Bulk;
  - one doubling.
- **A/A rows are re-frozen under build A,** one per cell at 12 seeds, before any comparison is read. The stage 3e floors were measured without the clamp, and are not reused.

### 12.4 Predictions (derived, with bands)

**The deciding case: CL-long against the old designs under the clamp** (λ 10 000, C 12 500, 1.5 s outage; δ = 50).
- **The clamp takes every design's drain-time repair to about 0** (§3.3: spare < r for most of the drain). So every row moves toward its repair-off reading, and the leads move toward the repair-off leads (stage 3e DECIDE, repair off):
  - **ADR-0084 at ρ ∈ {0.99, 0.995, 1}:** A's on-time lead is **+0.015 to +0.03** (repair off: +0.027) and its recovery lead **0.2 to 0.4 s** (repair off: 0.37). Read against bars of about 0.016 and 0.24, this is "shown better", or "undetermined" at the low end of the band. That is the registered risk.
  - **ADR-0084 at ρ ∈ {0.95, 0.98}:** **+0.04 to +0.06**, shown better.
  - **Today's Realtime** (both arms; the FIFO never fills at λ < C, so they are the same within A/A): **+0.035 to +0.05** on on-time share at matched share, shown better. Recovery is equivalent, as stored.
  - **Today's Bulk and the object API:** identity.
- **A's own rows are post-data,** and are labelled so: they were sized in step 2 (§9.1). They are T = 0.4 at 0.939–0.941 delivered and on time, and T = 2 and T = ∞ on time at 0.63–0.645, with recovery 5.7–6.1 s.
  - These are §9.1's measured clamp-on rows ± the A/A floor, because §10.7 and §11.2 are predicted within A/A here.
  - They confirm nothing. What is new in CL-long is the baselines under the clamp and `elig = "rack"`, which are unread.
- **The window bracket on the conclusion.** CL-long's reading under §12 holds at cwnd_sim = B = 1 000, where the clamp binds in the drain. At cwnd_sim = 2 048 the clamp never binds in CL-long (§9.1), so the stored unclamped reading, UNDETERMINED, holds there. The conclusion is stated with that bracket, and not as a property of the design alone.
- **B ≡ A** (identity check).

**CL** (the same as CL-long, but 10 s): the outage costs twice the share of a 20 s run.
- **A at T = 0.4:** delivered and on time ≈ 1 − (L − (T − R))·λ/N ≈ **0.88–0.90**.
- **Leads** over ADR-0084 and today's Realtime are in the same direction as CL-long, and **up to twice as large**, since the outage is the same and N is half. Bands: +0.02 to +0.06 against ADR-0084 at ρ ≥ 0.99, and +0.04 to +0.09 against today's Realtime.
- **B ≡ A.**

**CLw-long** (λ 8 000, C 12 500, 3 s outage):
- The headroom is 12 124 − 8 000 ≈ 4 100/s with the clamp, so x ≤ 1 and B ≡ A.
- **A's recovery at T = ∞:** the backlog λ·L = 24 000 over that headroom, on the on-time form of §19, gives **4.5–6.0 s**. At T = 0.4 it is within the 0.1 s resolution.
- **On-time leads** over ADR-0084 and today's Realtime: **+0.03 to +0.08**, shown better. Bulk is the identity.

**CLc** (CL with no outage):
- No queue builds, `spare_bind` ≈ 0 and x ≤ 1. So A equals stage 3e's RL rows within A/A, and B ≡ A.
- Every design delivers ≈ 1 − ε·(1 − P_dec) ≈ 0.995–1.000 on time at p50 ≈ 6–10 ms.
- **Predicted: equivalent to every baseline, on all three readouts.** This is the null cell.

**CLc-v, CL-long-v** (δ = 0.005, τ = 2 s): r = 0, so the clamp is inert for the new builds and for ADR-0084 and Bulk.
- **A** equals the repair-off rows; **B ≡ A**.
- **CLc-v:** equivalent across the board.
- **CL-long-v:** with τ = 2 s the backlog is delivered within τ at T ≥ 2, so the new model's lead over ADR-0084 comes from the floor's commitment, as in stage 3d. Band: **+0.00 to +0.04** on time within 2 s, "equivalent" or "shown better".
- **Today's Realtime at δ ∈ {0.5, 5, 50}** has r > 0, so the clamp moves those rows toward their repair-off readings.

**OV-long** (480 000 units, λ 12 000, C 10 000, δ = 50, τ = 0.4 s):
- **A:**
  - steady share **0.805–0.810**, which is `(C/λ)(1 − ε)` with r_eff ≈ 0: units sent at T − R are unprotected, so loss stays lost;
  - on time 0.805–0.810 at T = 0.4, and **< 0.02** at T ≥ 0.6, where the intake wait T − R exceeds 0.307 s;
  - p50 = T − R + 0.093 (0.39 s at T = 0.4).
- **B:**
  - it collapses at every finite T within about 0.6 s of μ̂ becoming finite (§4.1), so the sweep is degenerate and the cell is read by dominance;
  - steady share **0.800–0.810** (C/((1 + ρ)·λ); OV measured 0.803–0.809), on time equal to the share within 0.005, p50 **0.090–0.100 s** at every finite T.
- **Baselines:**
  - **the stored arm:** steady share 0.808, on time ≈ 0.806, p50 ≈ 0.17 s;
  - **the engine-queue arm:** steady 0.809 / 0 / 0.68–0.73 s;
  - **ADR-0084 and Bulk:** the intake grows without expiry, so on time is ≈ 0 and p50 is many seconds; share_any → 1 by t_limit.

| comparison | predicted reading |
|---|---|
| B against the stored arm | **B dominates**: share and on time within the bar, p50 better by about 0.075 s |
| B against the engine-queue arm | **B dominates**: share within the bar on the paired window, which starts at the engine-queue arm's later start, after its FIFO fills (§10.6), on time +0.8, p50 about 0.6 s better |
| B against ADR-0084 and Bulk | **not dominated**, a trade: on time +0.8 and p50 better by seconds, but share_any lower (0.81 against 1.0 by t_limit) |
| A against the stored arm | **not dominated**: equal share and on time, p50 worse by about 0.22 s |

**OV-long-v** (δ = 0.005, τ = 2 s): r = 0, and x ≈ 1.24.
- **A at T = 2:** steady share 0.805–0.810, on time within 2 s equal to the share (arrival at about 1.99 s), p50 ≈ 1.99 s. At T = 3, on time is ≈ 0.
- **B:**
  - K = 0.005·0.24·t_ov², against K_c = 3.78 at T = 2. So the cap falls slowly, from 1.90 s to about 0.99 s at 40 s, and **no collapse occurs within the run**.
  - Steady share and on time as A; **p50 1.6–1.8 s** at T = 2.
  - At T = 1 it collapses at about 27 s, and at T ≥ 1.5 not within the run.
- **Baselines:** the stored arm reads 0.808 / 0.808 / ≈ 0.17 s, and the engine-queue arm 0.809 / 0.809 / ≈ 0.69 s (its 0.68 s residence is within 2 s).
- **Predicted reading under the verdict (delivered and on time within 2 s): equivalent to both arms of today's Realtime.**
- **Reported only:** delay p50. The new builds read 1.6–2.0 s, against 0.17 s for the stored arm and 0.69 s for the engine-queue arm.
- **Q32 follows from that choice.** The verdict cannot see the standing queue at δ ≈ 0, by the design record's own pricing. So whether a standing queue that buys nothing should be shed at every δ is an owner question. It is not decided by this run.
- **ADR-0084 and Bulk:** on time ≈ 0 within 2 s, a trade as in OV-long.

### 12.5 Run count and time

**Configurations × seeds:**

| cell | build A | build B | baselines | runs |
|---|---|---|---|---|
| CL | 18 × 12 | 3 × 3 | 12 × 12 | 369 |
| CL-long | 18 × 12 | 3 × 3 | 12 × 12 | 369 |
| CLw-long | 18 × 12 | 3 × 3 | 12 × 12 | 369 |
| CLc | 6 × 12 | 3 × 3 | 12 × 12 | 225 |
| CLc-v | 8 × 12 | 3 × 3 | 14 × 12 | 273 |
| CL-long-v | 8 × 12 | 3 × 3 | 14 × 12 | 273 |
| OV-long | 8 × 6 | 8 × 6 | 7 × 3 | 117 |
| OV-long-v | 8 × 6 | 8 × 6 | 7 × 3 | 117 |
| A/A, re-frozen | | | 8 cells × 12 | 96 |
| **total** | | | | **2 208** |

- **The count is unchanged by the revision:** dropping the gate does not change B's rows, and §11.2 on the baselines does not add any.
- **No reuse.** §10.7 and §11.2 change every new-model trace, §11.2 changes the baselines' resend timing wherever R > P, and the clamp changes every repair-on row.
- **Where §11.2 changes the baselines:** wherever `elig_R_bind` > 0 after the first sample. That is the overload cells (srtt ≈ 0.1 s, with P at its cap) and the moments of CL-long's drain when rttvar spikes. Predicted within the A/A floor for every baseline readout, and gauged per row.
- **Time.** The time is not quoted until one timing run is done.
  - **The timing run:** one seed of ADR-0084 at ρ = 0.95 in OV-long-v, under the clamp and `elig = "rack"`. Stage 3e's TIME read 609 s for it without the clamp, which makes it the critical path.
  - **The estimate the timing run replaces** (stage 3e's TIME, one process each, unclamped): 7–15 s for most rows; 44 s for ADR-0084 in CL-long-v; 31–34 s, 160 s and 26 s for the new model, today's Realtime and Bulk in OV-long; 29 s, 609 s, 185 s and 24 s for the new model, ADR-0084, today's Realtime and Bulk in OV-long-v. That gives about 40 000 CPU-seconds, so about 75 min on 9 workers, or 1.5–2 h with the doubling.
  - **The schedule** is quoted from the timing run's measured value.
  - **If the run exceeds 30 min,** OV-long-v's ADR-0084 rows are reported before the batch continues, as in registration §4.
- **Order:**
  1. the A/A rows;
  2. CL-long, the deciding case;
  3. then CL, CLw-long, CLc, CLc-v and CL-long-v;
  4. then OV-long and OV-long-v.
  - Each cell is read when it finishes, and its doubling runs before the next cell.
- **Stop rules:** as §12.1's gates. A failed identity check of B against A in a headroom cell is a STOP, and sends the wait limit back to derivation.

## 13. Addendum: the wait limit's bet, re-derived for the policy (2026-11-26)

**Status.** Derivation. It uses the §12 confirmation's stored runs of OV-long and OV-long-v (6 seeds each), and one instrumented rerun of stored configurations (it logs readouts only and changes no decision). §13.5 and §13.6 are written before any run of build B′.

**Inputs.**
- `toy-stage3e` scratch build: `tools/toy/results-confirm3e.md` (OV-long, OV-long-v, CL-long), and the stored runs under `out3e/confirm/runs`, read with `confirm3e.py`'s registry.
- The per-window series below were recomputed from those runs. `reread-confirm-ov.md` is empty, so the coordinator's figures were checked against the recomputation, and they agree.
- **Instrumented reruns** (seed 0 of build A at T = 1; seeds 0–2 of build B in CL-long at T = 2). They log x̂, t_ov, R, ô, μ̂ and the head's age at each dequeue where x̂ > 1.
  - OV-long-v: x̂ = 1.226, μ̂ = 9 786/s, R = 0.105 s, ô = 0.094 s.
  - OV-long: x̂ = 1.232, μ̂ = 9 740/s, R = 0.104 s, ô = 0.095 s.
  - These are medians over 5–39 s. t_ov starts at t₀ ≈ 1.21–1.27 s, the first instant at which μ̂ is finite.

### 13.1 The refund premise, checked against the policy: refuted as stated

**§2's premise.** If the overload persists, the queue reaches §13.2's margin c = T − R and drops a unit anyway, so the head dropped now is refunded with odds t_ov/t_r.

**Under the policy that premise cannot hold.**
- The cap c_B = τ − R is non-increasing in t_ov at fixed x, so the queue never regrows to c while the overload lasts.
- The refund event of §2 ("the queue reaches c") is exactly what the policy forbids. The bet was priced against a future run by §13.2, but the future is run by the policy, so the bet is time-inconsistent.

**The measured mechanism: the bill is set by the cap when hand-over ends.** It is not set by what was shed on the way down. In a fluid model, with the link kept busy by the store pause (§1):

```text
   delivered  =  μ · (busy time) ,      busy time  =  hand-over span  +  the end drain  min( x·w_end , c )
   A − B      =  μ · ( c − x·w_end )⁺                                        w_end = B's queue-wait cap when the overload ends
```

**Why the end drain is `min(x·w_end, c)`.**
- When the overload ends, x̂ ≤ 1 resets t_ov, so B's cap returns to c.
- B's queue holds λ·w_end units at hand-over density λ. They drain at μ, so the last one is sent at age x·w_end. If x·w_end ≤ c, every one of them is delivered.
- A's queue holds λ·c units, and its head expires at age c. So A delivers only μ·c = λ·(c/x) of them; the rest expire.
- Every shed unit that the queue regains before the end is refunded: while the queue regrows, it drops nothing.

**Check against all four finite-T rows of OV-long-v** (δ = 0.005; μ̂ = 9 786, x̂ = 1.226, R = 0.105; w_end = B's `tau_min` − R, since the cap only falls):

| T | c (s) | w_end (s) | measured A − B (units) | the brief's bill with no refund, μ̂·(c − w_end) | the policy's bill μ̂·(c − x̂·w_end)⁺ |
|---|---|---|---|---|---|
| 1 | 0.895 | ≤ 0 (collapsed at 29.6 s) | 9 320 | 8 760 | 8 760 |
| 1.5 | 1.395 | 0.223 | 11 111 | 11 470 | 10 980 |
| 2 | 1.895 | 1.033 | 6 296 | 8 440 | 6 160 |
| 3 | 2.895 | 2.315 | 798 | 5 680 | 560 |

- The policy's bill fits every row to within 240–560 units. The no-refund bill (the coordinator's estimate, which is right at T = 1, where w_end = 0) fails at T = 2 and T = 3.
- **The residual, about +2–6 %, has no identified cause.** It is measured as an offset: after the collapse at T = 1, B's 4 s windows read 0.806 against A's 0.808, and in OV-long B's steady share is 0.803 against A's 0.805.
  - The inferred cause is B's young units: they are protected and resent (11 132 resends against A's 1 528 at T = 1).
  - Units per link send are equal, though (0.969 against 0.968), so the resends do not explain it by count. The offset is carried into §13.5's bands as measured, not as derived.
- **The end flush can be seen directly.** At T = 3, B delivers 1.000 of the units handed over in its last ≈ 2 s, against A's 0.808. That is λ·w_end units drained whole.

**The descent, read per window (T = 1).** B's delivered share per 4 s hand-over window, from [4, 8] s to [24, 28] s:

| | [4, 8] | [8, 12] | [12, 16] | [16, 20] | [20, 24] | [24, 28] |
|---|---|---|---|---|---|---|
| B | 0.801 | 0.790 | 0.782 | 0.777 | 0.768 | 0.763 |
| deficit against A's 0.808, measured | 0.007 | 0.018 | 0.026 | 0.031 | 0.040 | 0.045 |
| deficit predicted, `(μ̂/λ)·\|w′\|/(1 + \|w′\|)` with `\|w′\| = 2δ(x̂ − 1)·t_ov/(c + ô)` | 0.009 | 0.016 | 0.023 | 0.030 | 0.037 | 0.044 |

- After that, [28, 32] s reads 0.792 and the later windows 0.806. B never goes above A, because its cap never rises during the overload.
- The predicted collapse is at t₀ + √(c(c + ô)/(δ(x̂ − 1))) = 1.27 + 28.0 = 29.3 s, against 29.6 s measured.
- **So build B implements §3.1 as derived. The defect is in the derivation's premise, not in the code.**

**What survives of the refund, and what does not.**
1. **The free band.** The band of wait [c/x, c] is refunded for sure.
   - A cap at c/x leaves `x·w_end = c`, so the bill is 0.
   - The units A holds above c/x all expire at the end anyway: A's λc-unit reserve delivers only λ·(c/x).
   - **Shedding that band costs nothing at any δ and under any forecast.** It saves (c − c/x) of delay for every unit sent during the overload.
2. **The priced band.** Below c/x, each second of cap costs λ units, paid once and only if the overload ends while the cap is there: `bill = λ·(c/x − w)⁺`.
   - The odds t_ov/t_r of §2 describe neither band.
   - The factor in t_ov² that came from those odds has no counterpart in the policy (§13.4).

**Departures from the brief's expectations, stated.**
- **The no-refund bill is itself corrected.** The cap's end value is refunded up to `x·w_end` (the table above).
- **The new law is not inert at δ = 0.005.** The free band moves the cap from c to c/x̂ at the first overload dequeue, so p50 in OV-long-v at T = 2 goes from ≈ 1.99 s to ≈ 1.64 s, at no predicted cost in share. Only the priced band is inert there, with s_b ≈ 200–600 s, which is the brief's "≈ d₀/δ" scale.
- **The free band is §8(a)'s form.** It is τ = R + (T − R)/x̄, against §8(a)'s T·min(1, μ̂/λ̂). §8 rejected it as a whole law because it keeps a standing queue ∝ T. It returns here only as the zero-cost part, derived from the bill. The priced band removes the standing queue that §8 objected to, at δ-dependent speed.

**Premises of the bill, each with a gauge:**
- **The link stays busy.** The store pause holds about 880 datagrams in the link buffer (§1). Gauge: `link_util_w`.
- **The cap is restored when the overload ends.**
  - When hand-over stops or falls, λ̂ decays over its 8·srtt window, so x̂ falls continuously and the free-band cap c/x̂ rises faster than the head ages. With W = 0.8 s, x = 1.23 and c = 1.9, the cap is `1.55/(1 − t/W)` against a head age of `1.55 + 0.19·t`.
  - Then x̂ ≤ 1 resets t_ov, which removes the priced band.
  - Gauge: `reserve_end`, the queue wait at the last hand-over, and the delivered share of the units in it.
- **μ̂ is unbiased.** In OV-long-v, D̂ against the oracle reads 1.003–1.014 (q05–q95). Reading high is the safe side: the cap stays above c/x.

### 13.2 The bet for the policy

**The bet's objects.**
- **The cap's decision variable** is the queue wait w. Within [c/x, c] it is free (§13.1), so U prefers the lowest value, c/x, for every δ > 0.
- **Below c/x the stake is `λ·(c/x − w)`,** forfeited only if the overload ends while the cap is at w. Lowering the cap and raising it again before the end costs nothing, because the queue regrows without dropping. So the decision has no commitment and is taken afresh at each instant.

**The forecast (cited, not a choice).** Gott's scale-free prior (§2) gives, at overload age t_ov:

```text
   P(persist ≥ s | t_ov)  =  t_ov / (t_ov + s)      ⇒      hazard of ending now   h  =  1 / t_ov
```

**The flow form of U.** Per second of overload, compare the expected loss with the delay gain, both in U = log(throughput) − δ·log(delay).

```text
   expected throughput cost per second   =  h · λ·(c/x − w) / μ   =   x·(c/x − w) / t_ov          (the stake as a fraction of one second's units)
   delay gain per second                 =  δ · (c/x − w) / d₁ ,     d₁ = c/x + ô                     (relative delay, linearised at the status quo d₁, as §2 charges against d₀)
   shed below c/x   ⇔   δ/d₁  ≥  x/t_ov   ⇔   t_ov  ≥  s_b  =  (c + x·ô) / δ
```

- **Units.** Both rates are per second. s_b is in seconds.
- **The scale check** (the brief's rough scale was d₀/δ; here it is (c + x·ô)/δ, the same order):

  | cell | s_b | reading |
  |---|---|---|
  | OV-long-v, δ = 0.005, T = 1 | (0.895 + 0.115)/0.005 = 202 s | inert in a 40 s run |
  | OV-long-v, δ = 0.005, T = 3 | 603 s | inert in a 40 s run |
  | OV-long, δ = 50, T = 0.4 | 8.3 ms | at once |
  | OV-long, δ = 50, T = 2 | 40 ms | at once |

- **The median-horizon reading of §2 gives the same break-even.** Its throughput term is read over t_h = t_ov: δ/d₁ ≥ x/t_ov. So the hazard form and the median form agree on s_b.
- **The expected value over the horizon is ill-posed under Gott's prior.** The prior's remaining duration has infinite mean, so the answer depends on how the expectation is normalised:
  - normalised per horizon, E[1/S] = ∞ and the bet never pays;
  - added up per unit, E[S] = ∞ and it always pays.
  - The flow form above needs neither moment. **Reading U as a flow charged at the hazard rate** is the one step that is a choice (Q24, narrowed in §13.4).

**The optimum is a jump, so it cannot ship.**
- Linearised, the net per second is `(c/x − w)·(δ/d₁ − x/t_ov)`. That is linear in w, so the optimum is bang-bang: hold c/x until t_ov = s_b, then shed to the floor.
- Unlinearised, the gain `δ·log((c/x + ô)/(w + ô))` is convex in w, so the optimum is bang-bang too, and earlier: shed fully once t_ov ≥ t₁ = c/(δ·ln(1 + c/(x·ô))) ≤ s_b. For example t₁ = 82 s against s_b = 202 s at δ = 0.005 and T = 1.
- **The structural reason:** a cap removes the queue from the top, but the delay gain per second of wait is largest at the bottom. So once the first slice pays, every lower slice pays more.
- At fixed t_ov the jump is a step in δ at δ = (c + x·ô)/t_ov, which is a mode switch (CLAUDE.md). **So it cannot ship.**

**The continuous relaxation (a second choice, Q33).** Shed the priced band in proportion to the edge of the median bet.
- The median bet is "the overload outlasts s_b more", whose probability under Gott is P = t_ov/(t_ov + s_b).
- The shed fraction is `φ = (2P − 1)⁺ = ((t_ov − s_b)/(t_ov + s_b))⁺`.
- **Properties:**
  - it is exactly 0 until the bang-bang onset (P ≥ ½);
  - it is continuous in δ, t_ov, T and x;
  - it tends to 1 as t_ov → ∞;
  - it holds more than the U-optimum at every t_ov, so it is conservative in share.
- **Its form is the even-money Kelly fraction p − q** (J. L. Kelly, *Bell Syst. Tech. J.* 35, 917, 1956). Kelly's premise is a log-utility bet, which U's log(throughput) term shares. The mapping of our bet onto even money is not derived. So the citation gives the form, not a derivation, and it is recorded as a choice.

### 13.3 The law

**The sentence it implements.** Never hold a wait that the overload's end cannot drain (the free band). Below that, give the wait up as the δ-priced bet on the overload's persistence gains its edge (the priced band).

```text
   at each dequeue (toy2.py emit, after L1), with a younger unit waiting:
      expire the head u   ⇔   a(u) + R  ≥  T      ∨      a(u)  ≥  w
      w   =  ( c⁺ / x̄ ) · min( 1 ,  2 / (1 + y) )
      y   =  δ · t_ov / ( c⁺ + x̄·ô ) ,        c⁺ = max(T − R, 0) ,     x̄ = max(λ̂/μ̂, 1)
```

- **The first clause is §13.2 unchanged.** The second is the cap.
- In exact arithmetic the pair is one threshold, `a ≥ min(T − R, w)`. Both are always evaluated, and neither selects the other.
- For t_ov ≥ s_b, `min(1, 2/(1 + y))` equals 1 − φ, since y = t_ov/s_b.

| symbol | provenance |
|---|---|
| T, a(u), R | §2.1 and §13 of the derivation |
| ô | §17.1: `srtt − RTprop/2`. It is > 0 always (the RFC seed before a sample), so the denominator of y is > 0 |
| δ | declared, on (0, ∞); +∞ is a config error (§3.1) |
| λ̂, μ̂ = D̂, t_ov | measured, as in §3.1 and §3.2 (unchanged) |
| c/x̄ (the free band) | derived from the fluid bill, §13.1, with no choice |
| s_b = (c + x̄ô)/δ, so y = t_ov/s_b | derived: Gott's hazard (cited) and U read as a flow, linearised at d₁ (the choice of Q24) |
| `min(1, 2/(1 + y))` | the ramp (2P − 1)⁺ (the choice of Q33). It is a clamp, gauged as `onset_bind` |
| x̄ = max(x, 1) | the free band exists only under overload. It is a clamp, gauged as `ov_frac` (existing) |
| c⁺ = max(T − R, 0) | where T ≤ R, §13.2 already expires every head; w = 0 agrees. It is a clamp, gauged as `den_bind` (predicted 0 wherever T > R) |

**Shape (read before any number).**
- **Units:** w [s] = c⁺ [s] / x̄ [1] · [1]. y = δ [1] · t_ov [s] / [s].
- **In age:** w does not depend on a, so expiry is a threshold in age, and the oldest goes first.
- **In δ:** y is increasing, so w is non-increasing and continuous. dw/dδ = 0 before the onset, and there is a kink, not a step, at y = 1.
  - At ±2 % of any δ preset, |Δw|/w ≤ 2 %·y/(1 + y) < 2 %.
- **In t_ov:** non-increasing and continuous. The tail is `w ≈ 2·(c/x̄)·s_b/t_ov`, so ∝ 1/t_ov.
- **In T:** c rises, and y falls, so w rises.
- **In x:** non-increasing.
  - Before the onset, w = c/x̄.
  - After it, w = 2c/(x̄ + δ·t_ov·x̄/(c + x̄ô)), and both terms of the denominator rise with x̄.
  - At x → 1⁺, w → c·min(1, 2/(1 + y)), continuous in x.
- **T = ∞:** c⁺ = ∞ and y = δt_ov/∞ = 0.0, so min(1, 2) = 1 and w = ∞/x̄ = ∞. `a ≥ ∞` and `a + R ≥ ∞` are both false: **inert by arithmetic**, with no NaN, because x̄ is finite and ≥ 1.
- **Headroom, x̂ ≤ 1:** t_ov = 0 and x̄ = 1.0, so y = 0.0, the min is 1.0 and w = c⁺ = fl(T − R) exactly.
  - The second clause `a ≥ fl(T − R)` then differs from §13.2's `fl(a + R) ≥ T` only when a lies within one rounding of T − R.
  - So "identity" here means "identical except at one-ulp ties". The B′ ≡ A trace check is its witness. The form that is exact at ulps, `a + R ≥ T − (c − w)`, is ∞ − ∞ at T = ∞ and is rejected.
- **The x̂ = 1 edge (a regression against build B).**
  - **As a function,** w is continuous in (x, t_ov).
  - **Along a trajectory it is not.** t_ov is state, and it resets at x̂ ≤ 1. With x̂ = 1.0007 held past the onset, the cap is far below c; one tick at x̂ = 0.9999 gives c.
  - The old law did not step there in effect: its K carried (x − 1)⁺, so the cap was already at T − R whenever t_ov reset. The new priced band has no factor that vanishes as x → 1.
  - **A rise in the cap sheds nothing.** The harm runs the other way: a false x̂ > 1 that lasts past s_b in a headroom drain sheds the backlog.
    - At δ = 50 and T = 2, s_b ≈ 40 ms, and the identity margin in CL-long is t_ov ≤ 63 ms. The old law tolerated seconds. The one episode seen lasted 14 ms, over 3 seeds.
    - The free band is first order in x̂ − 1 as well: a live tick with the head at age in [c/x̂, c] expires it, and breaks the trace identity by a few units.
  - **So §1's "an outage must not read as overload" now rests on μ̂'s margin and on short false episodes,** not on the law.
  - **Gauges:**
    - `tov_resets`, the resets after the first overload dequeue;
    - `y_max` and `w_min_over_c` (the minimum of w/c⁺), reported in every identity cell.
  - **§5.2 test 3's "continuity through x = 1 ± 10⁻⁹"** is restated as continuity of w in (x, t_ov) jointly. The trajectory step is bounded by a test instead: in a planted drain with x̂ = 1 + 10⁻³ for t_ov ∈ {0.5, 1, 2}·s_b, the shed is reported against the backlog.
  - **Owner question Q34** (§13.4). It is not patched with a constant.
- **NO-MODE-SWITCH.** One expression. Three clamps, each continuous and each gauged. No dial is compared with a constant. The OR is a minimum of two thresholds, and both are always computed.
- **The floor.** w > 0 at every finite t_ov, so the old collapse threshold K_c has no counterpart. `floor_bind` is reported, and `collapse_t` is replaced by `onset_t`, the first instant with y ≥ 1, that is, t₀ + s_b.

**What it changes against build B.**
- The t_ov² is gone. w depends on t_ov linearly through y, and on x through c/x̄ and s_b.
- At δ = 0.005 only the free band acts within 40 s.
- At δ = 50 the cap reaches a few ms within a second and stays above the floor. The old law sat at the floor.

### 13.4 What this implies for Q24 and Q32 (and a new Q33)

**Q24, narrowed.**
- Of the two t_ov factors in t_ov², the **odds factor is refuted:** the policy has no refund at odds t_ov/t_r (§13.1).
- The **horizon factor** is replaced by Gott's hazard 1/t_ov. That is cited, and needs no median.
- What remains a choice is **reading U as a per-second flow,** charged at the expected loss rate and linearised at the status-quo delay d₁.
  - Its two alternatives, the expected value over the horizon normalised per horizon or per unit, are ill-posed under the prior (never, or always).
  - The median-horizon reading of §2 gives the same s_b.
- **Proposed for the owner:** the flow reading, with Gott's calibration gauged (Q25). No default is set.
- The free band takes no part in Q24: it ships on its derivation alone.

**Q32, answered in part.**
- "Whether a standing queue that buys nothing should be shed at every δ" now has a derived part: **the band [c/x, c] buys nothing at any δ, and is shed at every δ > 0.**
- The rest, c/x, buys exactly the reserve that the overload's end delivers. At δ = 0.005 U keeps it for s_b ≈ 200–600 s.
- So at video δ the new law still holds a standing queue of c/x (≈ 1.55 s at T = 2, p50 ≈ 1.64 s), against the 0.17 s of today's Realtime.
- **What Q32 still asks:** whether a reserve that U values should be held at δ ≈ 0, where the verdict cannot see delay (§12.3).

**Q34 (new; open): the priced band at x̂ → 1⁺.**
- The old law's (x − 1)⁺ came from refund odds that the policy refutes. So the new priced band has no factor that vanishes at x = 1, and a false overload reading longer than s_b sheds a headroom backlog (§13.3, the x̂ = 1 edge).
- **A candidate with provenance, not derived here:** charge the priced band only against the wait the overload itself built under A, `∫(x̂ − 1)⁺ dt`. That is the status-quo queue growth, and it is 0 in a drain whose backlog came from an outage.
- **Default: none.** The registered run measures the edge through `y_max` and `w_min_over_c` in the identity cells. §13.8 records the margin check.

**Open reading for the owner: OV-long-m against today's Realtime is predicted "equivalent" or "worse", not "dominates"** (§13.5).
- U accepts the descent's share cost for the delay it buys, and a barred dominance reading cannot price that.
- Whether the verdict at intermediate δ should read U, or dominance, is the owner's question. This registration reads dominance as §12 does.

**Q33 (new; it blocks shipping, with Q24).** The ramp `min(1, 2/(1 + y))`, the even-money Kelly form, relaxes the U-optimal jump.
- Alternatives: any continuous φ(t_ov/s_b) that is 0 below 1 and tends to 1.
- §13.5's OV-long-m rows read its shape: the onset instant, the post-onset deficit and the 1/t_ov tail.
- **Registered default: R1, the form above** (§13.8 compares the alternatives and gives the reasons). Q33 stays open for the owner.

### 13.5 Predictions (written before any run of B′)

**Build B′** = A + the law of §13.3 (no send gate), everything else as §12.1's A.

**The H = ∞ rows** (OV-long at T ∈ {0.4, 2, ∞}; OV-long-v at T ∈ {1, 2, ∞}) are **reported only**, with no band.
- The one expectation: their on time is ≤ the H = 0 row at the same T. B read 0.7908 against 0.8042 at T = 2 in OV-long.
- T = ∞, H = ∞ is §12.1's Bulk gate.

**Common numbers:**
- OV-long and OV-long-m (δ = 50, and 0.1): x̂ ≈ 1.24 (B's young units read μ̂ ≈ 9 690), R ≈ 0.104 s, ô ≈ 0.095 s, t₀ ≈ 1.2 s.
- OV-long-v: x̂ ≈ 1.226, μ̂ ≈ 9 786, R ≈ 0.105 s, ô ≈ 0.094 s.
- **The steady window:** start = the later arm's fill + 1 s, which is ≈ 2.5–3 s here (B′'s first expiry comes within ≈ 0.3 s of t₀); end = 39.6 s.

**OV-long (δ = 50, τ = 0.4 s; T ∈ {0.4, 0.6, 1, 2, ∞}, H = ∞ at {0.4, 2, ∞}):**
- s_b = 8.3 / 12.3 / 20.3 / 40.3 ms at T = 0.4 / 0.6 / 1 / 2, so `onset_bind` ≈ 0 after the first 40 ms of overload.
- The cap is `2·(c/x)·s_b/t_ov`: 3 ms → 0.1 ms over the window at T = 0.4, and 92 ms → 3 ms at T = 2.

| T | steady share | steady on time (0.4 s) | steady p50 | whole-run delivered |
|---|---|---|---|---|
| 0.4 | 0.801–0.805 | share − ≤ 0.003 | 0.090–0.105 s | 0.803–0.807 |
| 0.6, 1 | 0.800–0.805 | share − ≤ 0.003 | 0.092–0.108 s | 0.803–0.807 |
| 2 | 0.799–0.804 (descent ≈ −0.002 in the window) | share − ≤ 0.005 | 0.095–0.110 s | 0.803–0.807 |
| ∞ | = A (inert) | | | 1.000 |

- **The best point is T = 0.4.**
- **Dominance:**
  - against today's Realtime, stored arm, at δ ∈ {0.5, 50}: share −0.004 to −0.007 and on time the same, both within the ≈ 0.0096 bar; p50 better by 0.06–0.08 s, beyond the 0.0085 bar. **Dominates**, as B did.
  - against the engine-queue arm: **dominates** (on time +0.80, p50 −0.6 s).
  - against ADR-0084 and Bulk: **a trade** (share −0.19, on time +0.8).
- **Reported:** B′ against B. It should be equal within the A/A floor on every readout, except p50, which is higher by ≤ 0.01 s.

**OV-long-v (δ = 0.005, τ = 2 s; T ∈ {1, 1.5, 2, 3, ∞}, H = ∞ at {1, 2, ∞}):**
- s_b = 202 / 302 / 402 / 603 s, so **the priced band is inert: `onset_bind` = 1.000.**
- Only the free band acts. The cap is c/x̂ = 0.730 / 1.138 / 1.546 / 2.361 s, against A's c = 0.895 / 1.395 / 1.895 / 2.895 s.

| T | steady share | steady on time (2 s) | p50 (reported only) | whole-run, B′ − A |
|---|---|---|---|---|
| 1 | 0.804–0.809 | = share | 0.80–0.85 s | −0.004 to +0.001 |
| 1.5 | 0.804–0.809 | = share | 1.20–1.26 s | −0.004 to +0.001 |
| 2 | 0.804–0.809 | = share | 1.61–1.67 s | −0.004 to +0.001 |
| 3 | 0.804–0.809 | ≈ 0 (delay 2.46 s > 2) | 2.43–2.49 s | −0.004 to +0.001 |

- **The −0.002 expected in the share** is the young units' resend cost: a wait of c/x plus reach plus ô is below T at every T, so units are protected.
- **Bill check (a stop, §13.6):** A − B′ ≤ 0.004 × 480 000 ≈ 1 900 units at every finite T. B measured 9 320 at T = 1.
- **Dominance under the verdict** (delivered and on time within 2 s): against both of today's Realtime arms, at δ = 0.005 and at 50, **equivalent**, as A. B′'s best point is T = 1, the lowest p50 among the tied points.
  - It replaces B's measured "2.35 points worse".
  - It is read with the reader fix of §13.6.
- **Against ADR-0084 and Bulk:** a trade, as A.

**The headroom identity (3 seeds, traces equal: B′ ≡ A):**
- **Registered cells and lifetimes:**
  - CLc, CLc-v and CL-long-v at the default T set of §12 (B_T);
  - also CL-long, CL and CLw-long at T ∈ {0.4, 2, ∞}, because CL-long is where live ticks were seen.
- **Predicted: identical in all of them.**
  - Overload ticks appear only at CL and CL-long at T = 2, seed 2 (one trajectory: the two configs differ only in units and t_limit, and the episode lies at 3.69 s), and at CL-long-v, seed 11 (§13.8). Elsewhere x̂ ≤ 1 at every dequeue, and the identity holds by arithmetic (up to one-ulp ties).
  - CL's B rows were not read in §12: its reading stopped on an empty steady window. Its exposure comes from the coordinator's rerun.
  - **CL-long at T = 2, seed 2** (rerun): 151 ticks with x̂ ≤ 1.0007, t_ov ≤ 14 ms and head age ≤ 1.534 s, at 3.69–3.70 s in the drain.
    - There y = 50·0.014/(1.982 + 0.008) = 0.35 < 1, so w = c/x̄ = 1.981 s > 1.534 s, and nothing expires.
    - The identity would break only if x̂ > 1.29, or if t_ov > 63 ms at that x̂. **Predicted margin: kept.**
- `n_cap_lt_c` (ticks with w < c⁺) is predicted to read the same as B's `n_tau_lt_T`.

**The added overload row: OV-long-m.** OV-long's link, load and τ = 0.4 s, at **δ = 0.1**.
- **Why δ = 0.1:** s_b = (c + 0.118)/0.1 puts the onset inside the steady window at every finite T of OV-long's sweep. That reads the onset's linear dependence on (c + x·ô), so this row is MEASUREMENT DISCIPLINE 17's live witness.
- **The rate law there:** β = bulkness(0.1) = 0.349, and RL's r(0.1) = 0.046 at ε = 0.03 and srtt 0.1 s. The clamp takes it to ≈ 0 under overload (`spare_bind` ≈ 0.99 in OV-long), so μ̂ and x̂ are as in OV-long.
- **Sweep:** T ∈ {0.4, 0.6, 1, 2, ∞}, H = 0, for A and B′.

| T | s_b | `onset_t` (t₀ + s_b) | cap c/x before onset → at 39.6 s | per-window deficit right after onset | steady share | steady on time (0.4 s) | steady p50 | whole-run delivered |
|---|---|---|---|---|---|---|---|---|
| 0.4 | 4.14 s | 5.3 ± 0.4 s | 0.239 → 0.047 s | 0.023 | 0.796–0.803 | = share | 0.16–0.20 s | 0.803–0.808 |
| 0.6 | 6.14 s | 7.3 ± 0.5 s | 0.400 → 0.110 s | 0.026 | 0.792–0.802 | 0.50–0.70 (on time once the cap < 0.31 s, from t_ov ≈ 1.6·s_b) | 0.25–0.32 s | 0.806–0.812 |
| 1 | 10.1 s | 11.3 ± 0.7 s | 0.723 → 0.30 s | 0.029 | 0.789–0.799 | 0.00–0.05 (the cap reaches 0.31 s only at t_ov ≈ 37.6 s) | 0.50–0.65 s | 0.810–0.816 |
| 2 | 20.1 s | 21.3 ± 1.2 s | 1.529 → 1.05 s | 0.031 | 0.788–0.798 | ≈ 0 | 1.45–1.65 s (half the window is before the onset) | 0.828–0.836 |
| ∞ | — | — | inert | — | = A | | | 1.000 |

- **How the steady share is derived:** A's 0.805, minus the descent inside the window, `μ̂·(cap at the start − cap at the end)/(λ·window)` (−0.004 / −0.006 / −0.009 / −0.011 at T = 0.4 / 0.6 / 1 / 2), minus the protected units' resend cost (≈ −0.002 once the wait plus reach plus ô is below T).
- **Whole-run delivered** is A's row minus the bill `μ̂·(c − x̂·w_end)` (2 300 / 3 500 / 5 100 / 5 700 units).
- **A at δ = 0.1** is predicted equal to OV-long's A within the A/A floor (r is clamped, and A does not read δ otherwise).
- **The shape readouts** decide Q33. They are read from the per-window cap trace and the per-window shares, never from a pooled share:
  1. **`onset_t`** within the band at every T, and `onset_t − t₀` linear in (c + x̂ô): the T = 2 / T = 0.4 ratio is 4.86 ± 10 %.
  2. **Before the onset** the cap is c/x̂ within 3 %, and the per-window deficit against A is ≤ 0.003.
  3. **After the onset** the deficit peaks at `(μ/λ)·(c/x)/(2·s_b)` within ±30 %, then falls as `(s_b/(t_ov + s_b))²`. The old law's deficit rose linearly in t_ov instead.
  4. **The tail:** `w·t_ov` is constant within ±15 % over t_ov ∈ [2·s_b, 38 s].
- **Dominance at the best point** (T = 0.4) against today's Realtime at δ = 0.1, both arms run fresh:
  - **stored arm:** share −0.005 to −0.012 against a bar of ≈ 0.0096, on time the same, and p50 −0.01 to +0.03 against a bar of ≈ 0.0085.
    - **Predicted: "equivalent" in the upper half of the share band, "worse" in the lower half,** whichever way the share falls. It is not predicted to dominate.
    - The reason is that U accepts the descent's ≈ 0.4-point share cost for the delay it buys, while dominance with a bar cannot price it. This is a property of the reading, recorded here so that a "worse" cannot be read as a law failure.
  - **engine-queue arm:** **dominates** (on time +0.80, p50 −0.5 s).
- **Reported only:** B′ against A at every T. B′ − A in delivered equals −μ̂·(c − x̂·w_end)/N within 15 %, which is the bill check again on a live descent.

### 13.6 Registration (in the §12 style)

**Builds.**
- **A:** §12.1's A, unchanged. Its stored §12 rows are reused in OV-long and OV-long-v.
- **B′:** A + §13.3.
- **B:** stored, reported only.
- **Old senders:** reused from the stored §12 runs in OV-long and OV-long-v (the same machinery: the clamp and `elig = "rack"`).
  - In OV-long-m, today's Realtime runs fresh at δ = 0.1, both arms.
  - ADR-0084 at ρ ∈ {0.95, 1} and Bulk are reused from OV-long when their configuration fingerprint is equal (they do not read δ); otherwise they run 3 seeds.

**Cells and seeds:**

| cell | B′ | A | baselines | runs |
|---|---|---|---|---|
| OV-long | 8 × 6 | stored | stored | 48 |
| OV-long-v | 8 × 6 | stored | stored | 48 |
| OV-long-m (new) | 5 × 6 | 5 × 6 | rt and rteq at δ = 0.1, 2 × 3 (+ ADR-0084 and Bulk, 3 × 3, only if not reusable) | 66 (75) |
| identity: CLc, CLc-v, CL-long-v, CL-long, CL, CLw-long | 3 T × 3 seeds each (seeds 0–2, so CL and CL-long at T = 2 include the exposed seed 2, §13.8) | stored traces | — | 54 |
| identity, exposed seed: CL-long-v at T = 2, seed 11 (my addition; the coordinator asked for CL only) | 1 | 1, traced rerun (only seeds 0–2 carry traces) | — | 2 |
| A/A for OV-long-m | — | 2 × 12 | — | 24 |
| K-OV (reported only, §13.8) | T = 0.4 × 6 | T = 0.4 × 6 | — | 12 |
| **total** | | | | **254 (263)** |

- **Time:** about 7 000–8 000 CPU-s, so about 15 min on 9 workers at stage 3e's per-run times. The A/A floors of OV-long and OV-long-v are §12's frozen ones.

**The reading rule.**
- **Dominance in the three overload cells,** at B′'s best finite-T point (as §12.3, with its tie rule), on the paired steady window.
- The video cells' p50 is reported only (§12.3).
- B′ against A and B′ against B are reported only.
- The shape readouts of OV-long-m (§13.5, items 1–4) are read before any dominance outcome in that cell.
- **OV-long-m is in the verdict,** not reported-only: it is the only cell where the law acts mid-run.
- **The verdict sentence carries the limit of §13.8.** At δ = 0.1, every continuous relaxation puts the new model's steady share at the bar against today's stored Realtime arm at the best point: 0.008–0.010 behind it, against a bar of ≈ 0.0096. The relaxation (Q33) decides only whether the delay gain lands inside the 40 s window.
  - The registered default (R1, the Kelly ramp) is predicted "equivalent or worse". Its share sits ≈ 0.0004 inside the bar, with a ±0.002 band.
  - A "worse" there is read as that limit only if the decomposition test of §13.8 passes. Otherwise it is a law finding.
- **K-OV:** A and B′ are reported only (§13.8).

**The dominance reader fix (registered before any data of B′).** It also applies when §12's stored outcomes are re-read.

```text
   dominates     ⇔   worse beyond the bar on no readout     ∧   better beyond it on ≥ 1
   equivalent    ⇔   worse on none                           ∧   better on none
   worse         ⇔   better beyond the bar on no readout     ∧   worse beyond it on ≥ 1
   a trade       ⇔   better on ≥ 1                           ∧   worse on ≥ 1
   undetermined  ⇔   any readout undetermined (it takes precedence)
```

- Under this fix, §12's OV-long-v reading of B against today's Realtime ("not dominated (a trade)": share −0.0235, on time −0.0235, p50 reported only) is **worse**.

**Stop rules** (each STOP sends the law back to derivation):
1. **The headroom identity:** B′ ≢ A in any trace in the identity cells. One-ulp ties are reported and do not stop, but a tie must be shown to be one.
2. **The bill check in OV-long-v:** A − B′ > 1 900 units at any finite T. That would mean the free band is not free, and §13.1's mechanism is wrong.
3. **MEASUREMENT DISCIPLINE 17 in OV-long-m:** `onset_bind` ≥ 0.99 or ≤ 0.01 over the window at every finite T, so the law is inert or saturated where the onset was predicted inside. That is a defect finding and needs a ledger verdict.
4. **§12.1's gates:** every new-model run finishes, the absolute tests hold, and S5 at T = ∞, H = ∞ equals Bulk.

**Findings, not stops:**
- a shape readout of OV-long-m outside its band (it goes to Q33);
- `tov_resets` > 0 in an overload window;
- `den_bind` > 0 where T > R.

### 13.7 Config fields for `tools/toy/toy2.py` (branch `toy-stage3e`)

Names and semantics only. All default off, so every stored row reproduces.

**Config:**
- **`wait_limit_form: str = "k_t2"`.** It selects the wait-limit expression when `wait_limit = True`.
  - `"k_t2"` is §3.1's τ = T − K/(T − R + ô): build B, the current code.
  - `"rev4b"` is §13.3: the predicate `a + R ≥ T ∨ a ≥ w`, with w as §13.3.
  - It is a build selector of the offline simulation, like `wait_limit` itself, and not a dial.
- **The law as a function:** `wait_limit_w(T, R, o, delta, x, t_ov) -> w`, implementing §13.3 literally.
  - c⁺ = max(T − R, 0); x̄ = max(x, 1); y = delta·t_ov/(c⁺ + x̄·o); w = (c⁺/x̄)·min(1, 2/(1 + y)).
  - No `isinf` or `isfinite`, and no branch on δ, T or x.
  - It must return +∞ at T = ∞, and c⁺ exactly at x ≤ 1 with t_ov = 0.

**Gauges** (in `out["rev4"]`):
- **`onset_bind`:** the fraction of dequeues with x > 1 at which y ≤ 1.
- **`onset_t`:** the first instant with x > 1 and y ≥ 1, else None.
- **`n_cap_lt_c`:** dequeue ticks with w < c⁺. This is B′'s counterpart of `n_tau_lt_T`.
- **`cap_series`:** per 0.5 s window, the mean of w, of the head age at the dequeue, of x̂ and of t_ov, over dequeues with x > 1. Shape readouts 1–4 read from it.
- **`reserve_end`:** the head age at the last hand-over, and the delivered share of the units handed over after `last_hand − reserve_end`.
- **`tov_resets`:** the transitions from x > 1 to x ≤ 1 after the first dequeue with x > 1.
- **`y_max`, `w_min_over_c`:** the largest y, and the smallest w/c⁺, over the run. They are the x̂ = 1 edge's readouts (§13.3, Q34), reported in every identity cell.
- **Kept:** `den_bind`, which now counts dequeues with T < R; `ov_frac`; `floor_bind`; `deq_early_would`, counted against the §13.3 predicate.

**Cell and reader (in the confirmation driver, not in `toy2.py`):**
- **OV-long-m:** OV-long's link, load, units and τ = 0.4 s, at δ = 0.1, with the §13.5 sweep.
- **The fresh baselines:** today's Realtime at δ = 0.1, both arms.
- **The dominance reader fix of §13.6,** with a test that a row better on nothing and worse on one readout reads "worse".
- **The steady window** follows §12.2. For B′, "fill" is its first dequeue expiry.

**Tests (red first):**
- **Inert at T = ∞:** w = +∞ and never NaN over δ ∈ {0.005, 0.1, 0.5, 50, 10⁶}, x ∈ {0, 1, 1.3, 100} and t_ov ∈ {0, 10⁶}.
- **Exact at x ≤ 1:** w == max(T − R, 0) exactly.
- **Monotone in δ, t_ov and x (non-increasing) and in T (non-decreasing).** This includes ±2 % continuity at each δ preset and at y = 1.
- **The bill identity on a fluid toy:** the cap held at c/x leaves `x·w_end = c`.
- **The headroom trace identity** of §13.6's stop 1.

### 13.8 After Fable's review: Q33's default, OV-long-m under each relaxation, K-OV, and the Q34 margin (2026-11-26, before any run of B′)

**The relaxations of the bang-bang optimum.** Each is a function of y = t_ov/s_b only, multiplied into the free-band cap c/x̄. Each is continuous in δ, with no step and no threshold.

| code | shed form w/(c/x̄) | zero below the onset? | extra constant | provenance |
|---|---|---|---|---|
| R1 (§13.3) | `min(1, 2/(1 + y))` | yes | none | Kelly's even-money form p − q with P = Gott's P(S ≥ s_b) (cited form) |
| R2 | `1/(1 + y)` | no: it sheds from t_ov = 0⁺ | none | Gott's probability that the shed does not pay (probability matching) |
| R3 | `min(1, max(2 − y, 0))` | yes | the ramp width, one s_b, which has no provenance | the jump spread linearly over one s_b |
| BB | `1` for y < 1, `0` after | yes | none | the U optimum (linearised). A step in δ at fixed t_ov: **inadmissible**, and shown only as the bound |

**OV-long-m at its best point, T = 0.4** (s_b = 4.13 s; the steady window is t_ov ∈ [1.4, 38.4] s). The model:
- the base steady share is A's 0.805, or B's measured 0.803 once units are young enough to be protected (w + R + ô < T);
- minus the descent inside the window, `(μ̂/λ)·|w′|/(1 + |w′|)`;
- delay is w + 0.094 s.
- The bands are ±0.002 on shares and ±0.015 s on p50.
- The stored arm reads 0.808 / 0.806 / 0.17 s, with bars ≈ 0.0096 on shares and ≈ 0.0085 s on p50.

| law | steady share | on time (0.4 s) | p50 | whole-run | against the stored arm: share / p50 | predicted reading |
|---|---|---|---|---|---|---|
| R1 | 0.799 | 0.799 | 0.176 s | 0.806 | −0.009 / +0.006 s | **equivalent or worse** (the share is ≈ 0.0004 inside the bar) |
| R2 | 0.800 | 0.800 | 0.135 s | 0.805 | −0.008 / −0.035 s | dominates, or a trade at the low edge; **but it breaks the headroom identity** (below) |
| R3 | 0.798 | 0.798 | 0.094 s | 0.804 | −0.010 / −0.076 s | a trade, or dominates at the high edge |
| BB | ≈ 0.798 | ≈ 0.798 | 0.094 s | 0.804 | −0.010 / −0.076 s | a trade (bound only) |

**At the other lifetimes** (steady share / on time / p50):

| T | R1 | R2 | R3 |
|---|---|---|---|
| 0.6 | 0.797 / 0.61 / 0.28 s | 0.797 / 0.79 / 0.19 s | 0.795 / 0.66 / 0.09 s |
| 1 | 0.794 / 0.02 / 0.58 s | 0.793 / 0.53 / 0.34 s | 0.788 / 0.48 / 0.12 s |
| 2 | 0.793 / 0 / 1.62 s | 0.784 / 0 / 0.86 s | 0.775 / 0.04 / 1.62 s |

- **The registered bands are §13.5's** (for example 0.796–0.803 for the steady share at T = 0.4). This table is the comparison model behind them: central values, with ±0.002 inside those bands. R1's values fall inside §13.5's bands at every T.

**Fable's suspicion is confirmed.**
- R1's slow tail (w ∝ 1/t_ov) pays share across the whole window and delivers its delay gain late: p50 is 0.176 s against R3's 0.094 s. So Q33 sets the OV-long-m outcome against the stored arm, between "equivalent" (R1) and "a trade or dominates" (R3, R2).

**The limit, and its size** (carried by §13.6's verdict sentence).
- **No relaxation reads ahead of the stored arm on share.**
  - Every one reads 0.008–0.010 behind it at the best point.
  - That is made of: the new model's base deficit, measured in OV-long (A −0.0035; B −0.005 with young units' resends); plus the in-window descent, from 0.002 (R2: most of the shed lands before the window starts) to 0.005 (R3, BB: the whole band c/x = 0.239 s inside the window, `μ̂·0.239/(λ·37 s)`).
  - So the share readout sits at the bar whatever the relaxation. The relaxation decides only whether the delay gain lands inside the window.
- **R2's lead is not admissible.** It has no zero region below the onset, so it sheds a headroom backlog on a false overload reading of any length.
  - At CL and CL-long at T = 2, seed 2 (x̂ = 1.0007, t_ov = 14 ms, δ = 50), R2's cap is 1.981/(1 + 0.35) = 1.465 s, below the 1.534 s head age.
  - So B′ under R2 is predicted to fail stop 1 (the headroom identity). R1's cap there is 1.981 s.

**Q33's registered default: R1.** The reasons, in order:
1. **It adds no constant.** R3's width of one s_b has no provenance, and FORMULA-FIRST forbids shipping it; any k·s_b would do as well.
2. **It is zero below the onset,** which keeps Q34's margin (about 3× at δ = 50, below). R2 has none.
3. **It holds more than the U optimum at every t_ov,** so its error is in the conservative direction (share kept, delay paid).
4. **Its form is cited** (Kelly). The mapping onto even money is still a choice (Q33 stays open).
- **The cost of the choice, stated before data:**
  - OV-long-m is predicted "equivalent or worse" against the stored arm, not "dominates". R3 or R2 might dominate it, but R3 carries an unprovenanced width and R2 breaks the identity;
  - p50 is 0.176 s against 0.17 s;
  - the delay gain over A (p50 0.39 s → 0.18 s) is real, but it does not beat today's Realtime inside 40 s.
- **What would change the default:** an admissible relaxation with a faster tail and no added constant. None is known.

**"Worse" in OV-long-m, locked before any run.**
- It means §13.6's reader fix: better beyond the bar on no readout, and worse beyond it on at least one.
- **The cell stays in the verdict.**
- **A "worse" under R1 is read as the limit above only if all three hold.** Otherwise it is a law finding, and the law returns to derivation.
  1. The steady share against the stored arm is in [−0.012, −0.0096], and p50 is within +0.02 s.
  2. It decomposes as predicted, within ±0.002 each:
     - A against the stored arm, measured in this cell: −0.0035;
     - B′ against A: −0.0055;
     - the in-window descent, read from `cap_series`: −0.004.
  3. Shape readouts 1–4 of §13.5 pass.

**K-OV (reported only; δ = 50, τ = 0.4 s; λ 50/s, C 40/s, L10 loss, owd 50 ms, B = 8, 3 000 units; T = 0.4; 6 seeds).**
- **Inputs:** R ≈ 0.21 s and ô ≈ 0.15 s, so c ≈ 0.19 s. μ̂ = D̂ ≈ 36/s, so x̂ ≈ 1.39.
- **The law there:**
  - the free band is c/x̂ ≈ 0.137 s;
  - s_b = (0.19 + 1.39·0.15)/50 ≈ 8 ms;
  - so the cap is `2·(c/x̂)·s_b/t_ov` ≈ 2.2 ms·s / t_ov: about 2 ms after 1 s of overload, below one inter-arrival (20 ms). The queue therefore holds about the newest unit, as the old law's floor did.
- **The roadmap's "0.720 at half the delay"** is the old law's sizing row (§12 base: build B with no gate, 0.720 at p50 0.166 s, against Revision 3's 0.716 at 0.32 s). **B′ is predicted to reproduce it,** because at δ = 50 both laws reach a cap under one inter-arrival within about 0.1 s of μ̂ being finite. The reserve bill μ̂·c ≈ 7 units is negligible.

| build | delivered | on time (0.4 s) | p50 |
|---|---|---|---|
| A | 0.70–0.73 | delivered − ≤ 0.01 | 0.30–0.34 s |
| B′ | 0.70–0.73 | delivered − ≤ 0.02 (a resend arrives near 0.36 s) | 0.15–0.19 s |

- These readouts are whole-run, and K-OV has no registered steady window.
- K-OV's run time is not in §13.6's 15 min: it takes the time-limited run of §5.4, and its timing is reported when run.
- The figure is attributed to B′ only once these rows read inside their bands.

**The Q34 margin check** (the coordinator's reruns of build A with the `ana_cll` hook, at the seeds that saw overload ticks; the CL-long row is §13.5's rerun of build B).

| cell, T, seed | overload ticks | x̂ max | t_ov max | head age max | c/x̂ | R1's cap at that t_ov |
|---|---|---|---|---|---|---|
| CL, T = 2, seed 2 | 151 | 1.0007 | 14 ms | 1.534 s | 1.981 s | 1.981 s (y = 0.35) |
| CL-long, T = 2, seed 2 | 151 | 1.0007 | 14 ms | 1.534 s | 1.981 s | 1.981 s (y = 0.35) |
| CL-long-v, T = 2, seed 11 | 135 | 1.0092 | 12 ms | 1.532 s | 1.963 s | 1.963 s (δ = 0.005, y ≈ 3·10⁻⁵) |
| CL-long-v, T = 3, seed 11 | 135 | 1.0092 | 12 ms | 1.532 s | 2.954 s | 2.954 s |

- **In every headroom row with any overload tick, no tick would shed under R1.** No other headroom row of A in §12 has `ov_frac` > 0, and CLw-long has none.
- **The margin at δ = 50:** t_ov = 14 ms against s_b ≈ 40 ms, about 3×. Under R2 the same tick sheds, as above.
- **The CL and CL-long rows are one trajectory.** The two configs differ only in `n_total` and `t_limit` (checked), and the episode at 3.69 s comes before CL's 10 s end.
- **CL-long-v's live seed 11 lies outside the identity seeds 0–2.** So the 3-seed check executes no live tick there. §13.6 adds seed 11 at T = 2 as a traced pair.
- **CL at T = 2 is in the B′ identity set,** with 3 seeds including seed 2 (§13.6).
- **Q34 stays open, and blocks the engine step together with Q24.** This check bounds the edge only on the seeds seen.

### 13.9 The B′ confirmation, as registered, and the OV-long-m law finding read on stored gauges (2026-11-26; post-data, no new runs)

**Source.** `tools/toy/results-confirm13.md` (branch `toy-stage3e` at `cf22cb43`): the headroom identity table (its section "Headroom identity"), the OV-long-v, OV-long, OV-long-m and K-OV sections, and the runs under `out3e/confirm/runs`, read through `confirm13.py`'s jobs.
- The B′ rows ran on a Linux Docker host. Three stored Windows rows were rerun there and matched key for key, including one B′ row (the coordinator's check).
- The reads below use only the stored gauges: `cap_series`, the per-window series and the per-run counts. Scratch scripts: `ana13.py`, `ana13b.py`, `ana13c.py` and `fresh2.py` in the session scratchpad.

#### 13.9.1 The outcome as registered

| cell | registered prediction | outcome (`results-confirm13.md`) |
|---|---|---|
| headroom identity (every pair, including CL and CL-long at T = 2 seed 2, and CL-long-v at seed 11) | identical | identical in every pair |
| OV-long-v | equivalent to both Realtime arms; the bill A − B′ ≤ 1 900 units | equivalent to both; bill 601–776 units at T = 1–3 |
| OV-long | dominates both Realtime arms | dominates both |
| K-OV (reported only) | A 0.70–0.73 at p50 0.30–0.34 s; B′ 0.70–0.73 at 0.15–0.19 s | A 0.714 at 0.322 s; B′ 0.719 at 0.166 s. Both in band |
| OV-long-m, best point T = 0.4, against the stored arm | equivalent or worse | **worse**: share −0.0090 (bar ≈ 0.0096), p50 +0.026 s (bar ≈ 0.0086) |
| OV-long-m, against the engine-queue arm | dominates | **a trade**: share −0.0099, on time +0.80, p50 −0.54 s |

- **The engine-queue row is a prediction slip.** §13.5 predicted "dominates", but the share band (−0.008 to −0.010) already lay at that arm's bar.
- **§13.8's interpretation of "worse":**
  - condition 1 fails (p50 +0.026 s against ≤ +0.02 s);
  - condition 2 holds (−0.0034, −0.0055 and −0.0043 against −0.0035, −0.0055 and −0.004);
  - condition 3 fails: shape item 4 at T = 0.4 and 0.6 (19.1 % and 15.4 % against a 15 % bar), and item 2 at T = 2 (a pre-onset deficit of 0.0052 against a 0.003 bar).
- **By the locked rule this is a law finding.** The three reads below settle what it is a finding of. They do not change the registered word, which stays "worse".
- **A correction of reference.** §13.6's stop 3 (a law inert or saturated where it should act) is MEASUREMENT DISCIPLINE **18** in `docs/measurement-discipline.md` ("a law measured pinned or degenerate is a defect finding"). Rule 17 is the scaling-structure test.

#### 13.9.2 Read 1: the tail. The law executes as written; the 15 % bar was the error

- **The cap equals the law per window.** In every `cap_series` window with t ≥ 3 s, at every T and seed, the mean w equals `wait_limit_w(T, R = T − c⁺, ô = 0.095, δ, x̂, t_ov)` on that window's mean inputs: median relative error ≤ 10⁻⁴, and |error| ≤ 2·10⁻⁴ at p95.
- **The head sits at the cap.** The head's age at the dequeue is w within −1.0 % at T = 0.4 and within −0.07 % at T = 2, medians. It is just under the cap, as a head sent before reaching it should be.
- **§13.5's item 4 tested the wrong shape.** It assumed `w·t_ov` constant. The law gives
  ```text
     w·t_ov  =  2·(c/x̄)·s_b · y/(1 + y)
  ```
  which rises by y/(1 + y) over the tail windows.
  - Measured and law spreads are equal to three digits on the same windows: 0.140 / 0.121 / 0.084 against 0.140 / 0.121 / 0.084 at T = 0.4 / 0.6 / 1, as half-range over mean. The coordinator's metric gives 19.1 / 15.4 / 9.4 % measured against 19.7 / 15.9 / 9.7 % law.
- **Item 3 has the same kind of bar error.** Its readout is the highest 3-window mean after the onset. Most of the law's peak sits in the first window, so the 3-window mean is below the peak by an amount that grows as s_b shrinks.
  - The law's value of the statistic the reader actually computes is 0.0190 / 0.0244 / 0.0277 / 0.0303 at T = 0.4 / 0.6 / 1 / 2 (with the resend term below), against 0.0178 / 0.0231 / 0.0265 / 0.0296 measured. All are within 7 %.
  - The 0.76 to 0.96 trend in the reported ratio comes from comparing with the peak instead of with this statistic.

#### 13.9.3 Read 2: the deficits. About 7 % of the extra resends fill nothing, in both cells; the mechanism is not separated

**The counts, build B′ minus A, 6-seed means:**

| cell | T | resends (A → B′) | duplicates (A → B′) | repairs | first sends | link slots (first + resends + repairs) | delivered |
|---|---|---|---|---|---|---|---|
| OV-long-v (δ = 0.005, r = 0) | 1 | 1 528 → 12 074 | 89 → 89 | 0 | −10 578 | −32 | −758 |
| | 1.5 | 2 353 → 13 236 | 89 → 89 | 0 | −10 912 | −29 | −776 |
| | 2 | 3 192 → 13 484 | 89 → 89 | 0 | −10 304 | −12 | −704 |
| | 3 | 4 893 → 13 795 | 89 → 89 | 0 | −8 902 | 0 | −601 |
| OV-long-m (δ = 0.1) | 0.4 | 418 → 10 735 | 125 → 1 296 | −203 | −12 555 | −2 441 | −2 998 |
| | 0.6 | 727 → 12 488 | 163 → 1 519 | −267 | −15 157 | −3 663 | −4 217 |
| | 1 | 1 362 → 12 613 | 249 → 1 551 | −269 | −16 168 | −5 186 | −5 674 |
| | 2 | 2 972 → 13 106 | 444 → 1 682 | −273 | −15 730 | −5 869 | −6 308 |

**The account closes exactly from the stored fields.** For every row, `Δdelivered = Δ(first sends) − Δ(never_delivered)`. For example, OV-long-v at T = 1 gives −10 578 + 9 820 = −758, and OV-long-m at T = 0.4 gives −12 555 + 9 557 = −2 998. Each extra resend displaces a first send, or a link slot in the shorter end drain, and fills at most one hole. The fields read are `never_delivered` (= `passed_unfilled`), `pass_floor`, `late_floor`, `n_missing`, `dup` and `late`.

**Holes filled per extra resend** (Δ never_delivered / Δ resends):

| | T = 1 | 1.5 | 2 | 3 |
|---|---|---|---|---|
| OV-long-v | 0.931 | 0.931 | 0.933 | 0.932 |

| | T = 0.4 | 0.6 | 1 | 2 |
|---|---|---|---|---|
| OV-long-m | 0.926 | 0.930 | 0.933 | 0.930 |

- So **≈ 0.07 of each extra resend fills nothing**, in both cells and at every T.
- In OV-long-v that is 726 / 747 / 693 / 601 units, which is the cell's whole bill. The cap is constant there, r = 0, and the link slots balance to within 32.
- **Of the 0.07, ε ≈ 0.03 is the resend itself being lost.** The other ≈ 0.04 (≈ 410 per 10 500 resends) is not separated by the fields read: `dup` and `late_floor` do not move in OV-long-v.
  - §11.2 says a report-driven resend on one FIFO path is never a duplicate. So these ≈ 0.04 are an accounting hole, not yet a mechanism.
- **In OV-long-m the duplicate gauge rises by +1 171 to +1 356**, more than the 712–821 unfilled resends. So that gauge does not count wasted resends one for one; `late_floor` falls by 273–342 and the freed repair slots add 203–273.
  - The equal fill fraction in the two cells is therefore not shown to be one mechanism.
- **The per-window deficit B′ − A in OV-long-v** on the paired steady window is −0.0017 / −0.0019 / −0.0019 / −0.0018 at T = 1 / 1.5 / 2 / 3. It is flat in c/x̂ (0.73 → 2.36 s), and it equals the unfilled fraction at the measured extra-resend rate (≈ 280/s × 0.07 / 12 000 = 0.0016).

**What this explains:**
- **§13.1's 2–6 % residual on build B.** In OV-long-v at T = 1, B sent 9 604 extra resends (11 132 against 1 528, with duplicates 90 against 89). At 0.07 each that is ≈ 670 units, against the 240–560 measured. That is the order of the residual, and duplicates cannot carry it there.
- **The bill's excess in OV-long-m** (observed over predicted 1.29 / 1.20 / 1.11 / 1.08).
- **The post-onset "+0.002" of item 3's fall.**
- **The pre-onset deficit at T = 1** (0.0025 measured; 0.0018 from the cost at the run's own resend rate).

**What it does not explain (δ-specific, absent at r = 0):**
- **OV-long-m before the onset at T = 2:** 0.0049 measured against 0.0018, an excess of ≈ 0.003. The cap has no trend there (x̂ 1.2373 → 1.2381, w 1.5324 → 1.5310 over [11, 21.4] s), so the excess is not descent.
- **OV-long-m before the onset at T = 0.4:** 0.0013 measured against 0. The units there are unprotected (0.24 + 0.104 + 0.095 > 0.4), so they are not resent.
- OV-long-v shows no such growth with c/x̂. Both excesses therefore belong to δ = 0.1, which differs from δ = 0.005 by its repair (r = 0.046 before the clamp, ≈ 2 000 repairs sent) and by duplicates.

**Verdict on the read: it holds in part.**
- "The free band costs nothing" is **off by the unfilled fraction of the resends it causes**: about 0.07 per resend, because protected units are resent. That gives ≈ 0.0017 in share per window, and 600–780 units whole-run in OV-long-v (about 700 of OV-long-m's bill).
- The fluid bill of §13.1 leaves that cost out.
- A δ-specific pre-onset residual of up to 0.003 remains unexplained.

#### 13.9.4 Read 3: the delay. The failing condition was a prediction-convention error

- **The readings:** B′'s own steady p50 at T = 0.4 is **0.198 s**, inside §13.5's band of 0.16–0.20 s at its top edge. The stored arm's is **0.172 s**, against the 0.17 s assumed.
- **Why §13.8's condition 1 (≤ +0.02 s) missed.** It came from a central value of 0.176 s, which was the median of w + ô over *delivered* units.
- **The reader computes something else.** Its p50 is the median over *handed-over* units, with undelivered ones at +∞ (§12.2).
  - With 20 % of units undelivered, that is the 62.5th percentile of the delivered delays.
  - B′'s delay falls over the window, so that percentile sits at t_ov ≈ start + 0.375·L ≈ 15 s. There w ≈ 0.103 s, so p50 ≈ 0.197 s.
  - Recomputed with the reader's convention, the model gives 0.197 s, and a difference of +0.025 s against +0.026 s measured.
- **So condition 1 fails on the prediction, not on the law.** The stored arm's delay is flat, so the convention does not move its p50.
- **Superseded in §13.8.** Its p50 column (0.176 / 0.135 / 0.094 s for R1 / R2 / R3) used the delivered-unit median. With the reader's convention R1 reads ≈ 0.197 s.
  - So R1's predicted word against the stored arm was "worse" by the prediction's own numbers, and not "equivalent or worse".
  - R2 and R3 move up too, by less, because they shed earlier.

#### 13.9.5 What the finding is, the limit, and the corrected tests

**Reading.** The law executes as written (read 1), and the delay readout is what the law implies (read 3). The OV-long-m "worse" has two parts:
1. **The limit of R1, a Q33 matter.** R1's p50 stays above the stored arm's flat 0.172 s until the instantaneous cap falls below 0.078 s.
   ```text
      w + ô  <  0.172 s   ⇔   2·(c/x̄)/(1 + y) < 0.078 s   ⇔   y > 5.2   ⇔   t_ov  >  5.2·s_b  ≈  21.4 s  (≈ 22.7 s into the run)
   ```
   - With the reader's p50 convention, the window p50 crosses 0.172 s only in a run of ≳ 55 s.
   - **Another rerun of the same cell does not change this; only Q33 does.**
   - The share part of the limit is unchanged from §13.8: 0.009 behind, at the bar.
2. **A model gap that goes back to derivation:** the cost of the resends the free band causes (read 2).
   - **§13.1's bill becomes**
     ```text
        A − B′  =  μ̂·(c − x̂·w_end)⁺  +  κ_r·ΔN_resend ,       κ_r ≈ 0.065 units per extra resend (the unfilled fraction; measured in OV-long-v and OV-long-m, constant over T)
     ```
     - κ_r is measured, not derived, so it cannot ship as a law term.
     - It reproduces OV-long-m's four bills within 110 units (2 982 / 4 246 / 5 784 / 6 378 against 2 998 / 4 217 / 5 674 / 6 308).
   - **What is owed:**
     - a per-resend outcome gauge: of each resend, whether it filled a hole, was a duplicate, was lost, or arrived after its hole was passed. It should be read by rerunning the stored configurations, not as a new cell;
     - a derivation of κ_r from that gauge. Where r > 0 it must also cover duplicates and the freed repair slots;
     - a per-window resend, duplicate and repair series, to locate OV-long-m's δ-specific pre-onset excess (≈ 0.003 at T = 2, 0.0013 at T = 0.4).
   - Until then the excess is a recorded defect finding with no ledger verdict proposed.

**Corrected tests** (post-data on these rows; the fresh row below confirms them):
- **Item 2 (before the onset):**
  - cap·x̂/c within 3 % of 1 (unchanged);
  - deficit against A equal to `κ_r·(B′'s extra-resend rate)/λ`, computed from each run's own counts over the protected part of the run (≈ 0.0017 where units are protected, 0 where not);
  - the bar is the p95 of |A − A′| over the same windows from the cell's own A/A pair, at the comparison's seed count. That replaces any fixed number.
  - It is read only when at least 3 pre-onset windows lie inside the A-paired window; otherwise it is n/a.
  - On OV-long-m it fails at T = 2 (0.0049 against ≈ 0.0018) and is marginal at T = 0.4 (0.0013 against 0), which is the open δ-specific excess. It holds at T = 1 and in every OV-long-v row.
- **Item 3 (after the onset):** read only with 3 windows after the onset inside the window. The highest 3-window mean after the onset equals the law's value of that same statistic, `(μ/λ)·|w′|/(1 + |w′|)` averaged over 0.5 s windows plus the resend term, within ±15 %.
- **Item 4 (tail):** per tail window, `w·t_ov` equals `2·(c/x̄)·s_b·y/(1 + y)` on the window's own inputs, within ±3 %. This replaces "constant within ±15 %".
- **Delay (condition 1):** every p50 prediction uses the reader's convention (median over handed units, undelivered at +∞), on the window of the pairing it is read in.
  - The A-paired window starts at A's fill + 1 s, which differs by T. The stored-arm pairing starts at ≈ 2.7 s.
  - With those starts the model gives 0.196 / 0.318 / 0.634 / 1.63 s on OV-long-m's A pairing, against 0.198 / 0.319 / 0.626 / 1.561 s measured.
  - The T = 2 miss of 0.07 s comes from the 50 % cut falling within ≈ 0.2 s of the onset knee. There a small shift of the onset or of the window's delivered fraction moves the median along the knee.
  - Bands: ±0.015 s at T < 1, ±0.03 s at T = 1, ±0.10 s at T = 2.
- **The bill:** A − B′ equals μ̂·(c − x̂·w_end) + κ_r·ΔN_resend within ±250 units, with κ_r = 0.065 and ΔN_resend read from the run.

#### 13.9.6 The fresh middle-price row: OV-long-q at δ = 0.118 (written before any run)

**Why δ = 0.118.** The shape items read inside A's paired window, which starts at ≈ 2.9 / 3.9 / 6.0 / 11.0 s for T = 0.4 / 0.6 / 1 / 2 (A's fill + 1 s, independent of δ). Two constraints:
- **Items 2 and 3 need at least 3 windows (1.5 s) between the window start and the onset,** plus a margin over the onset's ±8 % band.
  - At T = 0.4 that means s_b ≥ 3.12 s plus the margin, so δ ≤ 0.133.
  - At δ = 0.118 the third pre-onset window ends 0.38 s before the predicted onset, outside its ±0.28 s band.
- **Item 4 at T = 2 needs tail windows:** t₀ + 2·s_b = 35.4 s ≤ 39.6 s, which gives 8 windows.
- **The earlier drafts are withdrawn:**
  - δ = 0.3 put every onset before the window start;
  - δ = 0.125 left only 0.09 s of margin at T = 0.4.

**The cell.** OV-long's link, load, units and τ = 0.4 s, at δ = 0.118.
- **The rate law there:** β = 0.314 and r(0.118) = 0.049. The clamp takes r to ≈ 0 under overload, so x̂ ≈ 1.235, μ̂ ≈ 9 695, R ≈ 0.104 s, ô ≈ 0.095 s and t₀ ≈ 1.28 s, as in OV-long-m.
- **Rows:**
  - A and B′ at T ∈ {0.4, 0.6, 1, 2, ∞}, H = 0, 6 seeds;
  - **its own A/A pair** (A at T = 0.4 against its phase variant, 2 × 12). Its per-window floor sets item 2's bar;
  - today's Realtime at δ = 0.118, both arms, 3 seeds, fresh;
  - ADR-0084 and Bulk reused by fingerprint (§13.6).

**B′ predictions.** They come from the corrected model, which reproduces OV-long-m's rows: the p50 values of §13.9.5, the item 3 statistic within 7 %, and share 0.799 against 0.799 at T = 0.4. Item 2 and the share use the resend term (≈ 0.0018 where protected). p50 is on the A pairing.

| T | s_b | `onset_t` | pre-onset windows; item 2 deficit | item 3 statistic | tail windows | steady share | on time (0.4 s) | p50, A pairing | A − B′ whole-run (units) |
|---|---|---|---|---|---|---|---|---|---|
| 0.4 | 3.50 s | 4.78 s | 3; 0 (unprotected) | 0.021 | 62 | 0.799 | = share | 0.184 s | 2 390 + 680 = 3 070 |
| 0.6 | 5.20 s | 6.48 s | 4; 0 (unprotected) | 0.028 | 55 | 0.797 | 0.67 | 0.292 s | 3 660 + 680 = 4 340 |
| 1 | 8.59 s | 9.87 s | 7; 0.0018 | 0.032 | 42 | 0.792 | 0.14 | 0.578 s | 5 510 + 680 = 6 190 |
| 2 | 17.1 s | 18.3 s | 14; 0.0018 | 0.035 | 8 | 0.787 | ≈ 0 | 1.51 s | 7 060 + 680 = 7 740 |
| ∞ | — | — | — | — | — | = A | | | 0 |

- **Bands:**
  - shares ±0.002;
  - p50 ±0.015 s at T < 1, ±0.03 s at T = 1, ±0.10 s at T = 2 (the knee, §13.9.5);
  - s_b ±8 %; item 3 ±15 %; item 4 ±3 % per window;
  - item 2 at the cell's own A/A floor; the bill ±250 units.
- **Item 1:** s_b(T = 2)/s_b(T = 0.4) = 4.87 ± 10 %.
- **The residual's tests are item 2 at all four T.**
  - At T = 1 and T = 2, a reading of 0.0018 within the floor would mean OV-long-m's excess was specific to that cell. A reading of ≈ 0.0035–0.005 would mean the δ-specific excess is real and grows with c/x, and it goes into the derivation owed in §13.9.5.
  - At T = 0.4 and 0.6 the prediction is 0. A reading like OV-long-m's 0.0013 would be the unprotected excess recurring.

**Reading (registered before any run).**
- **The best point is T = 0.4.**
- **Against the stored arm** (its pairing starts at ≈ 2.7 s; B′'s p50 there is 0.184 s): share −0.009 ± 0.002, at the ≈ 0.0096 bar; p50 +0.012 ± 0.015 s against a ≈ 0.0086 bar. **Predicted: "worse"** (or "equivalent" at the low end of both bands).
  - 5.2·s_b = 18 s here, so R1's delay gain lands too late to beat the stored arm's flat 0.172 s on the window p50.
  - This is the R1 limit of §13.9.5, at a second δ.
- **Against the engine-queue arm:** share −0.010, p50 −0.55 s, on time +0.80. **Predicted: "a trade."**
- **The cell confirms the corrected tests if all of these hold:**
  - items 1, 3 and 4 at every T;
  - item 2 within its floor at every T (the residual's test);
  - the bill and p50 in their bands.
- **Q33, Q34 and Q24 stay open.**
