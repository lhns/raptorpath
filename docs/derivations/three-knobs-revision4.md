# Three knobs, Revision 4: a sender-side wait limit under sustained overload, and the engine's repair clamp (derivation, no runs)

**Status.** Design and derivation only. Nothing here has been run, and the offline simulation is unchanged.
- **What it amends.** `three-knobs.md` §13.2 (the dequeue rule). It withdraws that section's paragraph "What cannot be fixed: delay pinned near T − R", and Q12's default ("no loss price").
- **What it adds to the model of every design** (§3.3): the engine's spare-capacity clamp on repair, and the engine's send gate.
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

**The revised law.** In the dequeue rule, T becomes τ, read per head:

```text
   expire the head u   ⇔   a(u) + R  ≥  τ(u)   ∧   a younger unit is waiting
   τ(u)  =  T  −  δ · (λ̂/μ̂ − 1)⁺ · t_ov²  /  ( a(u) + ô )
   μ̂     =  Ĉ / ( 1 + r_eff + ε̂/(1 − ε̂) )
```

- **Where it comes from.** It is derived from the paper's own utility `U = log(throughput) − δ·log(delay)`, applied to the bet "dropping the head now will be refunded by a drop the overload would force anyway". Gott's scale-free forecast (1993) gives both the odds and the horizon. The first draft's raw 1/δ odds bar is withdrawn (§2).
- **λ̂** counts hand-overs.
- **μ̂** is capacity for first sends:
  - Ĉ is the wire capacity of qualifying rounds, a windowed maximum.
  - The outage's one-off resend debt is excluded by construction (§3.2).
  - With a margin of 9.8 % under RL, CL-long's drain cannot read as overload: about 11 000/s against 10 000 (§4.2).
- **ô** is §17.1's one-way term.
- **t_ov** is how long λ̂ has exceeded μ̂.
- **Inert at T = ∞:** ∞ minus a finite term.
- **Identical to Revision 3 wherever λ̂ ≤ μ̂:** the subtracted term is exactly 0.0.
- **Continuous** in δ, T and the rates.
- **The floor of τ** is reached when every unit but the newest expires. It gets a bind gauge, `floor_bind` (§3.1).

**Predicted for OV at T = 0.4** (delivered / on time within 0.4 s / delay p50; derived in §4.1):

| design | prediction |
|---|---|
| Revision 3 (stored) | 0.793 / 0.793 / 0.390 s |
| the clamp alone | 0.837 / 0.837 / 0.39 s |
| the wait limit alone | 0.770 / 0.770 / 0.095 s |
| Revision 4, both | 0.79–0.81 / 0.79–0.80 / 0.095–0.10 s |
| today's Realtime behind the engine's gate, with a 500-packet front queue | about 0.808 / 0.80 / 0.15 s |

The last row assumes the Linux TUN default tail-drop queue of 500 packets (§3.3). With a front queue of depth 0 it would read 0.093 s, the link-queue floor both designs share. **It cannot fall below Revision 4's floor.** Without the gate, its 0.172 s is the 125 ms link buffer the arm fills.

**OV cannot confirm the law.**
- OV is post-data.
- At δ = 50 the intake collapses within about 50 ms of the overload being measured, so τ sits on its floor and every finite T gives one point.
- A collapsed sweep is read as a single point by dominance, never interpolated (§3.1).
- **The cells that can test it:**
  - OV-end (an overload that ends), which prices the bet;
  - OV-crit (load at capacity), the knife edge;
  - OV at δ = 0.5, where at T = 2 the collapse comes about 2.8 s after the overload is measured, so the shape is visible;
  - K-OV after a time-limit amendment.

**δ's scale is open, stated honestly (Q24).**
- The bar comes from U, with δ as the paper defines it: dimensionless, the marginal cost of a relative delay.
- **The one step without provenance** is that U's throughput term is read over the forecast horizon t_ov. That is Gott's median: cited, but a choice. Nothing ships on it until Q24 is answered.
- OV-end runs δ and δ/2 at T = 2 (δ = 0.5 and 0.25). The collapse instant is predicted to scale as δ^(−½).

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

**The bet.** At a dequeue under overload, x = λ̂/μ̂ > 1, drop the head u (age a) now instead of letting §13.2 send it.
- **If the overload persists** for `t_r = (T − R − a)/(x − 1)` more seconds, the queue would reach §13.2's margin and drop a unit anyway. The drop is then refunded: the throughput is unchanged, and every unit sent while the overload lasts is one service time, 1/μ̂, younger.
- **If it ends earlier,** u would have been delivered, so one unit is lost.

**The forecast (cited).** Gott's scale-free (Jeffreys) prior on an unknown duration (J. R. Gott III, *Nature* 363, 315, 1993; its limits in C. M. Caves, 2000):

```text
   P(persist ≥ s | elapsed t_ov)  =  t_ov / (t_ov + s) ,       so the odds of a refund are   t_ov / t_r
```

**The two outcomes in U.**
- **Refund.** The delay of the units sent from the head falls by 1/μ̂, relative to their delay `a + ô`: the intake wait plus the one-way delay, which includes the forward queue (§17.1). So ΔU = +δ·(1/μ̂)/(a + ô).
- **Loss.** One unit leaves the throughput term. That term is averaged over the horizon the forecast speaks for, N = μ̂·t_h units, so ΔU = −1/(μ̂·t_h).

**The horizon t_h.** Requirement 1 forces it to be independent of T.
- With the refund horizon t_h = t_r, t_r cancels: the condition becomes δ·t_ov ≥ a + ô, and T is gone. At T = ∞, P = 0 and t_r = ∞, so the expected-value form is 0·∞. It drops at T = ∞.
- Requirement 4 forces t_h to be measured. The only measured scale of an overload is its age.
- So **t_h = t_ov**, which is Gott's median remaining duration. This is the one step whose provenance is a choice and not a derivation (Q24).

**The result.** Drop the head iff the expected ΔU ≥ 0:

```text
   (t_ov/t_r) · δ/(a + ô)  ≥  1/t_ov      ⇔      (T − R − a)·(a + ô)  ≤  δ·(x − 1)·t_ov²      ⇔      a + R  ≥  T − δ·(x − 1)·t_ov² / (a + ô)
```

- **Units:** s² on both sides of the middle form.
- **δ enters linearly, as the paper's price.** Halving δ multiplies every collapse instant by √2 (§4.3, OV-end).
- **The factor is the overload's age squared:** once from the odds, and once from the horizon.

**Why the intake needs no standing part.** Copa's 1/δ queue exists to keep the link busy. Here the store pause already holds about `B − λ·RTprop` ≈ 880 datagrams in the link buffer. So the intake's own floor can be 0 (§3.1), and dropping in the intake changes which units are sent, not how many. The exception is the reserve flushed when an overload ends, and that is exactly what the bet prices.

## 3. The law

### 3.1 The wait limit, its floor, and how a collapsed sweep is read

```text
   at each dequeue (toy2.py emit, after L1):
      expire the head u   ⇔   a(u) + R  ≥  τ(u)   ∧   a younger unit is waiting                  (§13.2 with T → τ)
      τ(u)  =  T  −  K / ( a(u) + ô ) ,        K  =  δ · (λ̂/μ̂ − 1)⁺ · t_ov²
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
- **Units:** τ [s] = T [s] − K [s²] / (a + ô) [s].
- **Monotonicity.** τ is non-decreasing in T. It is non-increasing in δ, x and t_ov. It is non-decreasing in a, because an older head is charged a smaller relative delay saving.
- **T = ∞:** ∞ − finite = ∞, so the rule never expires and the trace is unchanged. No `isinf` test is used.
- **x ≤ 1:** K = 0.0 exactly, and T − 0.0/(a + ô) = T exactly, so this is §13.2 **by identity**.
- **δ → 0:** τ → T. At δ = 0.005 the collapse in OV comes after 5.5 s at T = 0.4 and after 28 s at T = 2 (§4.1).
- **δ → ∞** (a limit only): any x > 1 collapses at once at finite T. It is still inert at T = ∞.
- **Continuity.** τ is continuous in δ, T, x and a, including through x = 1. A step in t_ov at a reset happens only where K = 0. τ ≤ R simply expires the head, so no clamp is needed.

**The floor of τ.** τ has no lower bound. The **effective floor is the younger-waiting clause**: the intake keeps its newest unit, so the hold floors at 0, plus the wait until the next pause release. The law reaches that floor (it collapses) once K exceeds the largest value of `(T − R − a)(a + ô)`:

```text
   collapse  ⇔   K  ≥  f_max  =  ( (T − R + ô)/2 )²        (or  ≥ (T − R)·ô  when T − R < ô)
```

**Bind gauges.**
- **`floor_bind`:** the fraction of dequeues at which every unit but the newest is expired.
- **`collapse_t`:** the first instant at which K ≥ f_max.
- **`deq_early`:** expiries with `a + R < T ≤ a + R + K/(a + ô)`.

Between `(T − R)·ô` and f_max, a queue whose head is younger than the lower root of the parabola is also expired, so a collapsed intake stays collapsed while a standing one persists until f_max. That is history, not a mode bit. It is gauged as `band_hold`, the fraction of dequeues with K in that band.

**Reading a cell where the lifetime sweep collapses.** This rule is registered here, before any data. When `floor_bind ≥ 1 − (the A/A noise)` over the readable windows at every finite T of a sweep, T no longer reaches the decision.
- **The finite-T rows must agree** within the A/A floor. If they do not, that is a finding.
- **They are pooled into one point,** compared with each baseline by dominance: delivered and on time not worse beyond the bar, and delay p50 not worse.
- **No matched-share interpolation** across the jump to T = ∞. T = ∞ stays a point of its own.
- **The degeneracy is recorded as a pinned law** under MEASUREMENT DISCIPLINE 17, with a ledger verdict the owner gives (Q29). The proposed verdict is "binds by design: under sustained overload, past the δ-priced horizon, the lifetime does not bind".

**NO-MODE-SWITCH.** The law is one expression. No dial is compared with a constant. `λ̂ ≤ μ̂` sets a timestamp; the law is continuous across it.

### 3.2 μ̂: capacity for first sends (units checked)

λ̂ counts handed-over units, so μ̂ must count the same units: the first-send rate the path can sustain. It is **not** the wire rate, and **not** the current first-send rate. The current first-send rate falls during a resend burst, and an outage would make it 0.

```text
   μ̂  =  Ĉ / ( 1 + r_eff + ε̂/(1 − ε̂) )
```

| symbol | provenance |
|---|---|
| Ĉ | measured: wire capacity, the payload datagrams per second the path carries. It is the maximum over the last 10 completed round trips (BBR's window; Cardwell et al., *ACM Queue* 14(5), 2016) of each qualifying round's wire send rate |
| a qualifying round | the sender was flow-limited in it (the store pause or the send gate bound), and no single pause in it lasted longer than R. A pause longer than R means acks were overdue, which is §13's meaning of R: the path stalled, which is no evidence of its capacity. **Ĉ = +∞ unless all of the last 10 completed rounds qualified**, so x = 0 then. Ĉ > 0 always |
| r_eff | the repair rate actually staged, after the clamp of §3.3: sender state, read from the same accrual |
| ε̂/(1 − ε̂) | the steady resend share: expected copies per first send to cover first-send loss ε̂. ε̂ is the loss the rate law already reads (`eps_for_law`: the true p0 in the offline simulation, the BOCD upper quantile in the engine) |

**Why this cannot read CL-long's drain as overload** (item 2).
- **The wire is saturated** whenever the link holds a backlog. After the store pause releases, the sender puts up to B datagrams in the link at once, so a qualifying round reads Ĉ = C = 12 500/s whatever mix of first sends and resends it carried. The ramp the first draft worried about is a ramp in *first sends*, and μ̂ does not measure it.
- **The outage's resend debt is a finite amount of work, not a rate.** It is the protected outage holes, resent at up to the 2 000/s budget, so first sends fall to about 9 300/s for a while.
  - It delays the drain by about debt/C, and that delay is already priced per unit by §13.2's margin.
  - It does not make the hand-over rate exceed the capacity, and μ̂ excludes it by construction, because only the steady share ε̂/(1 − ε̂) enters.
  - Reading it as a rate, as "first sends per second" would, gives x = 10 000/9 300 = 1.075. At δ = 50 that collapses the backlog after about √(0.0382/(50·0.075)) ≈ 0.10 s at T = 0.4, and after 0.51 s at T = 2.
- **Margins in CL-long** (C = 12 500, ε = 0.03):

  | arm | μ̂ | margin over λ = 10 000 |
  |---|---|---|
  | repair off | 12 500/1.031 = 12 124 | 21 % |
  | RL, clamp off (r ≈ 0.107) | 12 500/1.138 = 10 984 | 9.8 % |
  | RL, clamp on (r_eff ≈ 0 in the drain, §3.3) | 12 124 | 21 % |

  The gauge-first step (§5.3) biases μ̂ by −2, −5 and −10 % in the drain. Predicted: 0 ticks with τ < T, except RL with the clamp off at −10 %, which exceeds its 9.8 % margin and is therefore predicted non-zero.
- **RE** (estimated loss) reads ε̂ ≈ 0.3 after the outage and r = 0.5, so μ̂ ≈ 12 500/1.93 ≈ 6 500 and x ≈ 1.5. The arm is overloaded by its own estimate, and the law treats it so. It is reported, not counted as headroom.
- **In the engine:** Ĉ is the BtlBw anchor's delivery rate (paper §8.3). Its sampler has no app-limited flag, and its window form is unread. Both are prerequisites (Q26).

### 3.3 The engine's repair clamp and send gate, applied to every design

**The engine's law, read.**

```text
   r_eff  =  min( r_law ,  max(spare, 0) ) ,       spare  =  (cwnd − in_flight) / in_flight      (+∞ at in_flight = 0)
```

- It applies to the cadenced rate (`emit_source.rs:579`, `flat_rate = window_rate.min(spare.max(0.0))`) and to each taper grant (`fec_rate.rs:729`).
- `spare` is `PathState::spare_capacity` (`path.rs:467–472`).
- **The send gate:** data goes only on paths with `available() = cwnd − in_flight > 0` (`scheduler/mod.rs:229`). Every design in the engine, today's included, sits behind it.

**What the offline simulation lacks.** It has no cwnd. The new model's only in-flight bound is the store pause (O ≥ B seqs). Today's Realtime arm has no in-flight bound at all: it fills the 125 ms link buffer and the link drops the excess, which is where its 0.172 s comes from.

**The offline simulation's form** (a deliberate model change of every design, with a `GOLDEN_CAPTURE` re-baseline of every repair row):
- **cwnd_sim = B,** in payload datagrams. B is the existing store cap, a resource bound, so no new constant is added. The engine's cwnd is Copa's and is anchored at about BtlBw·RTprop; that is not modelled (Q22).
- **in_flight:** payload datagrams sent and not yet acknowledged, or declared lost by a report. This is the engine's accounting at report granularity.
- **spare** = (B − in_flight)/in_flight, and +∞ at in_flight = 0, as in the engine.
- **The gate:** no payload datagram is staged while in_flight ≥ B. It applies to every design.
- **The clamp:** `owed += min(r, max(spare, 0))` per first send. At r = 0 nothing changes, so **repair-off rows stay bit-identical**.

**Where it binds.**
- **CL before an outage:** in_flight ≈ λ·RTT ≈ 110, so spare ≈ 8 ≫ r. Unchanged.
- **The CL-long drain and OV:** in_flight ≈ B, so spare ≈ 0 to 0.02 and r_eff falls to about 0–0.02.
- **Gauge `spare_bind`:** the fraction of first sends at which spare < r.
- **The CL-long RL rows change.** Repair stops during the drain, so recovery moves toward the repair-off rows: in stage 3e, RL lengthened ADR-0084's recovery from 4.8 to 8.6 s. Those rows are re-baselined, not inherited.

**What today's sender does behind the gate.**
- **Its link queue is bounded** by cwnd minus the BDP, about 880 datagrams or 88 ms here, the same as the new model's.
- **The excess waits in front of the transport,** where today's sender has no age expiry: in the TUN queue, or in the application's blocked `write()` (paper §8.7).
- So its delay is 0.093 s plus the front queue's depth over μ.
- **The offline simulation models the front queue twice:**
  - **depth 500 tail-drop:** the Linux TUN default `txqueuelen`, a cited resource bound. That reads about 0.093 + 500/9 700 ≈ 0.145 s.
  - **unbounded:** application back-pressure. Delay then grows at (x − 1) seconds per second, and on time reads about 0.11, like the ADR-0084 rows.
- **It cannot fall below the 0.093 s link-queue floor** that both designs share. A front queue of depth 0 would equal Revision 4's 0.095 s.
- **Its share:** front-queue drops are off the wire, so the clamp takes its repair to about 0 as well. Its units are young enough to resend, so share ≈ C/((1 + ε/(1 − ε))·λ) ≈ 0.808.

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
- **Today's sender behind the gate:** the clamp removes the repairs its queue was dropping, so its share stays ≈ 0.81 (§3.3).

**Revised prediction rows:** clamp alone 0.837 / 0.837 / 0.39 s; Revision 4 with both parts 0.80–0.81 / 0.79–0.80 / 0.095–0.10 s.

## 4. Predictions (derived, written before any run)

**Common numbers (OV).**
- C = 10 000/s, λ = 12 000/s, R ≈ 0.102 s, ô ≈ 0.093 s, and the forward queue plus owd ≈ 0.093 s.
- f_max = 0.0382 at T = 0.4, 0.246 at T = 1 and 0.991 at T = 2.
- **Clamp off:** μ̂ = 10 000/(1.087 + 0.031) ≈ 8 950, so x ≈ 1.34. **Clamp on:** μ̂ ≈ 10 000/(1.01 + 0.031) ≈ 9 600, so x ≈ 1.25.
- **μ̂ becomes finite after 10 qualifying rounds,** about 1 s at srtt ≈ 0.1 s. The collapse then follows after `t_ov* = √(f_max/(δ·(x − 1)))`.

### 4.1 OV (post-data: these rows cannot confirm the law)

**The 2×2 at δ = 50, T = 0.4** (delivered / on time ≤ 0.4 s / delay p50):

| | clamp off | clamp on |
|---|---|---|
| **wait limit off** (Revision 3) | 0.7929 / 0.7928 / 0.390 s (stored) | 0.837 / 0.837 / 0.39 s (§3.4) |
| **wait limit on** | 0.770 / 0.770 / 0.095 s | 0.79–0.81 / 0.79–0.80 / 0.095–0.10 s |

How each cell is derived:
- **Clamp alone.** r_eff ≈ 0, so there is no displacement. Units are still sent unprotected at T − R, so first-send loss stays lost: `(C/λ)(1 − ε)` = 0.808, plus a flush of 0.298·9 700·0.97/120 000 = 0.024. That gives 0.832 from capacity, and 0.837 from the measured counts (§3.4); the counts are used.
- **Wait limit alone.** `μ̂·10 s/N` with no flush, so 0.767–0.772. The collapse comes 47 ms after μ̂ becomes finite.
- **Both.** Units are young and so protected at birth (0.098 + 0.093 < 0.4). Loss is recovered by resend, at ρ ≈ ε/(1 − ε), and a resend arrives by about 0.3 s. The share is C/((1 + r_eff + ρ)·λ), which is 0.808 at r_eff = 0 and about 0.79 if r_eff stays near 0.02.

**Against today's Realtime.**
- **As stored, with no gate:** 0.8165 / 0.814 / 0.172 s.
- **Behind the gate:** about 0.808 / 0.80 / 0.145 s with the 500-packet front queue. With the unbounded front queue it reads about 0.11 on time.
- **Read by §3.1's dominance rule,** since the finite-T sweep is degenerate here: Revision 4 with both parts is equivalent in share and better in delay against the gated sender with the 500-packet front queue.

**The δ and T map with the clamp on** (x − 1 = 0.25):

| δ | T | collapse after μ̂ is finite | delivered | on time ≤ 0.4 s | p50 |
|---|---|---|---|---|---|
| 50 | 0.4 / 1 / 2 | 55 ms / 0.14 s / 0.28 s | ≈ 0.80 | ≈ 0.80 / 0.79 / 0.79 | ≈ 0.10 s |
| 0.5 | 0.4 | 0.55 s | ≈ 0.80 | ≈ 0.80 | ≈ 0.10 s |
| 0.5 | 1 | 1.4 s | ≈ 0.80 | ≈ 0.73 | ≈ 0.10 s |
| 0.5 | 2 | 2.8 s (the wait peaks at ≈ 0.95 s) | ≈ 0.80 | ≈ 0.62 | ≈ 0.10 s |
| 0.005 | 0.4 | 5.5 s | ≈ 0.81 | ≈ 0.81 | ≈ 0.39 s |
| 0.005 | 2 | 28 s (never in the run) | Revision 3 with the clamp | | |

- **Strict order (H = ∞) at δ = 50, T = 0.4,** re-derived:
  - Young units are protected, so the floor lags each lost seq until FEC or a resend fills it.
  - With repair clamped to about 0, the resends carry the loss: about 3 % × 9 600 ≈ 290/s, against the 2 000/s budget.
  - Each blocks the strict frontier for about one R, so about 29 holes are open at once. The frontier trails the oldest of them by 0.1–0.2 s.
  - Predicted: on time 0.72–0.79, p50 0.18–0.25 s. That is not §15.2's collapse, because the holes are filled, not given up a round trip late.
  - **Low confidence:** the second resend's margin is 9 ms, and §19 found ô undershooting a persistent queue.

### 4.2 CL-long, CL, CLw-long, RT, K-I2, K-L10

**The wait limit.** x ≤ 1 at every dequeue:
- before an outage, no round qualifies, so Ĉ = +∞;
- the round that straddles an outage fails the pause clause;
- after it, Ĉ reads the saturated link, giving the margins of §3.2 (21 % or 9.8 %).

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
  - at T = 2, the queue at 6 s, ≈ 0.25·5 s·9 600 ≈ 12 000 units, drained at the 1 600/s headroom, with every wait ≤ 1.25 s < T − R.
- **Revision 4 at T = 0.4:** δ = 50 and δ = 25 both collapse within 0.1 s. Both lose the 2 860-unit reserve, −0.019 on time against Revision 3. The arms are indistinguishable, which checks that the cost equals the reserve.
- **Revision 4 at T = 2, δ = 0.5 and δ = 0.25:**
  - The collapse comes 2.8 s and 4.0 s after μ̂ is finite, at about 3.8 s and 5.0 s. **The ratio is √2, predicted from δ's linear seat.**
  - Both collapse before 6 s, so both lose the reserve: −0.08 delivered within T against Revision 3.
  - On time within 0.4 s gains about +0.4 at δ = 0.5 and +0.3 at δ = 0.25, from the fresh units between the collapse and 6 s and after it.
- `collapse_t` is read directly. Its ratio between the δ and δ/2 arms is the test of the horizon step (Q24).

**OV-crit** (new): λ = 9 600/s, the first-send capacity with the clamp; otherwise OV, 10 s. The knife edge.
- **Predicted:** `deq_early` ≤ 1 % of hand-overs, and Revision 4 equals Revision 3 within the A/A floor on all three readouts.
- **Above that,** the knife edge bites: the law returns to derivation.

**OV at δ = 0.5:** the δ = 0.5 rows of §4.1. Here T still matters, and the collapse instants are 0.55, 1.4 and 2.8 s.

**K-OV** (λ = 50/s, C = 40/s, L10 loss, owd 50 ms, B = 8, 3 000 units). It was bounded at 10 s by TIME and never run, so it needs a time-limit amendment: one seed is timed first, and the bound is set from it.
- **Inputs.** R ≈ 0.21 s and ô ≈ 0.15 s, so f_max = 0.0289. With the clamp on, μ̂ = 40/1.111 = 36, so x ≈ 1.39. μ̂ becomes finite after about 2 s, at rounds of 0.2 s. The collapse then follows in 38 ms.
- **Revision 4 at T = 0.4:**
  - delay p50 ≈ 0.16 s: the 0.1 s pause queue, plus the 50 ms owd, plus up to one ack gap;
  - delivered ≈ 0.72, on time 0.66–0.72. A resend arrives near 0.36 s, at the edge.
- **Revision 3:** the registration's ≈ 0.72 on time at p50 ≈ 0.34 s.
- **Today's Realtime:**
  - without the gate, ≤ 0.1 on time;
  - behind the gate with the 500-packet front queue (12.5 s at 40/s), ≈ 0 on time.

### 4.4 Two paths

- **μ̂ pools the paths:** Ĉ = Σ Ĉ_p, and x = λ̂/μ̂.
- **A path in an outage** keeps its held Ĉ_p. That overstates the capacity, so x reads low: the safe side, at Revision 3.
- **A path whose ô exceeds T** also counts at full capacity, again the safe side.
- **The P2 cells** run below capacity, so they are predicted identical.

## 5. Risks, tests, the gauge-first step and the sizing run

### 5.1 Risks

- **The horizon step (Q24).** Every collapse time scales with √(t_h/t_ov). Nothing ships until the owner decides, and OV-end's δ against δ/2 rows read its consequence.
- **The knife edge at x ≈ 1, amplified by δ.**
  - K grows as δ·(x − 1)·t_ov². With μ̂ biased high it reads 0, the safe side. With μ̂ biased low by more than the margin, it collapses after `√(f_max/(δ·bias))`.
  - The engine prerequisite is the app-limited exclusion (Q26). OV-crit and the bias rows of §5.3 measure the edge.
- **Hysteresis in the parabola band.** Gauged as `band_hold` (§3.1).
- **Oscillation.**
  - τ reads none of the quantities it moves. λ̂ is the application's rate. Ĉ changes only in qualifying rounds, and the sender stays flow-limited after a collapse. r_eff and ε̂ do not depend on the intake.
  - A collapse leaves srtt and R unchanged, because the link queue is set by B.
  - Noise near x = 1 only resets t_ov, which moves τ toward T.
- **The store pause and the gate.** The rule runs only at a dequeue. Through an outage Ĉ holds, and the round that straddles it does not qualify.
- **The floor law.** Units sent young are protected, so resends return under overload (§4.1, strict order). The commitment and L1 tests are unchanged.
- **The cost of the bet** when an overload ends, `μ̂·(T − R)` per episode at high δ. OV-end measures it.
- **The clamp changes every repair row.** Every stage 3e repair baseline must be re-run under the clamp before it is compared. Repair-off rows are untouched by arithmetic.

### 5.2 Tests (red first)

FORMULA-FIRST: the law, §2's derivation and §3.3's clamp go into the paper first (near §5.6, and §5.3's rate clamp), and **only after Q24 is answered.**

1. **`wait_limit` inert at T = ∞.** τ is +∞, never NaN, for δ ∈ {0.005, 0.5, 50, 10⁶}, x ∈ {0, 1, 1.3, 100}, t_ov ∈ {0, 10⁶} and a ∈ {0, 1}.
   - The source deny test forbids `isinf`, `isfinite` and any literal comparison of δ, T or x.
2. **The identity at headroom.** τ == T exactly for every x ≤ 1.
3. **Shape.**
   - Monotone in T, δ, x, t_ov and a.
   - ±2 % continuity at each δ preset and at T = 0.4, and continuity through x = 1 ± 10⁻⁹.
   - The collapse threshold equals f_max to 10⁻¹² for a grid of T and ô.
4. **The units of μ̂.** In CL-long, with RL, clamp off, in the drain: μ̂ = Ĉ/(1 + r_eff + ε̂/(1 − ε̂)) with Ĉ = C to within one round's quantisation.
   - With resends forced to their budget, μ̂ does not move. It is red against μ̂ taken as "first sends per second".
5. **The sample rule.** A round with a pause longer than R never qualifies. Ĉ = +∞ until 10 rounds in a row have qualified, and Ĉ > 0 always.
6. **CL-long identity with an execution witness.** S5, R0, RL and RS at T ∈ {0.4, 2, ∞}, clamp on, are trace-identical to the clamp-only rows, with `ov_frac` = 0 and `deq_early` = 0 in all 12 seeds.
   - Red against the instantaneous-rate μ̂, and against the sample rule without its pause clause.
7. **OV executes.** At δ = 50 and T = 0.4, `deq_early` > 0 and `floor_bind` > 0.9 after `collapse_t` + one round.
   - The intake wait of every first send is then no greater than the longest pause gap (absolute, from `deq_log`).
8. **The burst control.** CL with a planted 200-unit burst at δ = 50 gives `deq_early` = 0.
9. **The clamp.**
   - r_eff ≤ max(spare, 0) at every accrual.
   - At r = 0 the trace equals the unclamped trace.
   - No payload datagram is staged while in_flight ≥ B.
   - In CL before the outage, `spare_bind` = 0.
10. **Starvation and the unchanged absolutes.**
    - With τ < R, first sends per round are greater than 0.
    - `l1_viol` = 0 and the commitment violations are 0.
    - The T = ∞ rows equal full delivery in every cell.

### 5.3 The gauge-first step (before any run of the law)

The law is added as **readouts only**: λ̂, μ̂ (with Ĉ, r_eff and ε̂), t_ov, K and τ at every dequeue, plus the count `n_tau_lt_T` of dequeue ticks with τ < T. **No decision reads them.**
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
- **Today's Realtime behind the gate,** with front queues of 500 and unbounded, at δ = 50: 2 configurations, in OV and OV-end.
- **OV-end:** Revision 4 with the clamp, T = 0.4 at δ ∈ {50, 25}, and T = 2 at δ ∈ {0.5, 0.25}, plus Revision 3 with the clamp at both T: 6 configurations.
- **OV-crit:** Revision 3 and Revision 4, both with the clamp, at T = 0.4 and δ = 50: 2 configurations.
- **K-OV**, after its time-limit amendment: Revision 3 and Revision 4 with the clamp, plus today's Realtime gated and ungated, at T = 0.4: 4 configurations.

**Size:** 3 seeds each, about 40 configurations and 120 runs. At TIME's rates (7.7–34 s per run) that is under 15 min on 9 workers, plus K-OV's measured time.

**Readouts:**
- §4's three readouts;
- the gauges `deq_early`, `floor_bind`, `collapse_t`, `band_hold`, `ov_frac` and `spare_bind`;
- the fraction of rounds with Ĉ = +∞;
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
- **Q27, the clamp in the offline simulation.** cwnd_sim = B and the send gate for every design (§3.3). **Default: yes**, as a deliberate model change with a re-baseline. It replaces the first draft's "register Q22 next": the clamp is the engine's law, so it is part of this revision.
- **Q28, a standing part of the intake** (Copa's 1/δ). **Default: no.** The store pause keeps the link busy.
- **Q29, the ledger verdict for a lifetime sweep that collapses under overload** (§3.1). **Proposed: "binds by design".**
- **Q30, today's sender's front queue** in the gated model. **Default:** report both the TUN default (500, tail-drop) and the unbounded case. Do not pick one for the verdict until the owner says which consumer the comparison stands for.

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
