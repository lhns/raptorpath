# Three knobs, Revision 4: a sender-side wait limit under sustained overload (derivation, no runs)

**Status.** Design and derivation only. Nothing here has been run, and the offline simulation is unchanged. It amends `three-knobs.md` §13.2 (the dequeue rule) and withdraws that section's paragraph "What cannot be fixed: delay pinned near T − R", together with Q12's default ("no loss price"). L1, L2 (§2.1, §2.2 as revised in §17.1), the protected floor (§17.1), the brake and the store pause are unchanged. Reviewed with the advisor (Fable) before writing and again on the draft.

**Inputs read.**
- `docs/derivations/three-knobs.md` on `github/main` (`c326f489`): §1–§10, Revisions 1–3 (§13, §15, §17), the findings §18 (with its amendment) and §19.
- `docs/derivations/three-knobs-stage3e-registration.md` (main), §2.5, §2.6, §4 and §5.
- Branch `toy-stage3e` (`e434c44c`): `tools/toy/results-stage3e.md` (the OV reading of 2026-11-26 with its delay table, the CL-long DECIDE reading, TIME) and `tools/toy/toy2.py` (`emit`, `dequeue_expired`, `R_now`, `o_fwd`, `fwd`, `stage_repair`, `rate_hat`).
- Paper `docs/fec-arq-model.md` §4.1 (δ and ζ), §8.2 (Copa: the equilibrium standing queue is 1/δ packets), §8.3 (the BtlBw anchor; its sampler has no app-limited flag) and §8.7 (application back-pressure).

## Short answer

**The finding, re-read from the stored runs.** In OV (offered λ = 12 000 units/s, link C = 10 000/s, a 1 250-datagram link buffer, one path, repair RL), the stage 3e new-model rows fit one line in the intake hold `w = T − R`:

```text
   delivered(T)  ≈  0.772  +  0.07 · w          (T = 0.4, 0.6, 1, 2 give 0.7929, 0.8068, 0.8348, 0.9045)
```

- **0.772 is the steady share** `C / ((1 + r + ρ)·λ)`, with r ≈ 0.087 (the repair share of link sends, 0.080) and ρ ≈ 0 (units sent at age T − R are unprotected, so nothing is resent).
- **0.07·w is the end-of-run flush.** When hand-over stops at 10 s, the w seconds of backlog the intake held are still sent.
- **Every delivered unit waits about w in the intake,** plus the forward link queue the store pause leaves (about 88 ms), plus the 5 ms owd. So p50 = w + 0.093 s: 0.390 at T = 0.4 and 0.590 at T = 0.6. Within 0.4 s the share is 0.1175 at every T ≥ 0.6: only the first second of hand-overs, before the backlog binds.

**The recommended law (§3).** The dequeue rule's lifetime T becomes a wait limit τ:

```text
   expire the head u at the dequeue   ⇔   a(u) + R  ≥  τ   ∧   a younger unit is waiting
   τ  =  T  −  δ · t_ov · (λ̂/μ̂ − 1)⁺
```

- **λ̂** is the measured hand-over rate, **μ̂** the measured source capacity of the path (a windowed maximum held through outages), and **t_ov** how long λ̂ has exceeded μ̂.
- **Where it comes from.** It is the bet "dropping the head now will be refunded by a drop the overload would force anyway", taken when its odds, under the scale-free forecast of how long an overload lasts (Gott 1993), are at least 1/δ.
- **Inert by arithmetic at T = ∞:** ∞ minus a finite term is ∞.
- **Identical to Revision 3 wherever λ̂ ≤ μ̂:** the term is exactly 0.0, so τ = T bit for bit. That covers CL, CL-long, CLw-long and the K cells.
- **Continuous in δ, T, λ̂ and μ̂.** It has no branch, and its only clamp is `(·)⁺`, which is gauged.

**Predicted for OV, RL, δ = 50** (derived in §4.1):
- every finite T: delivered ≈ on time within 0.4 s ≈ 0.770 ± 0.005, delay p50 ≈ 0.095 s;
- Revision 3: 0.7929 / 0.7928 / 0.390 s at T = 0.4, and 0.8068 / 0.1175 / 0.590 s at T = 0.6;
- today's Realtime: 0.8165 / 0.814 / 0.172 s.

**OV still reads FAILS on the matched-share reading.** The gap shrinks from 0.70 to about 0.18. What is left is a **repair-displacement gap, not a wait-limit gap**:
- Today's Realtime sends every unit into the link, and its repairs decode the units the link buffer drops. Its share is therefore `C(1 − ε)/λ ≈ 0.81`.
- The new model drops in the intake, before a unit has a seq, so a repair can only recover wire loss. Any r above `ε/(1 − ε)` then displaces source units (registration §2.6).
- Closing that gap needs the spare-capacity limit on r (Q22), not a wait limit. With it, Revision 4's young units can be resent again, and the predicted OV row is ≈ 0.81 / 0.81 / 0.10 s (§4.1).

**What Revision 4 costs.** Under the bet, the intake gives up its reserve, the λ·w units that would be flushed when an overload ends.
- OV is a finite overload, so at T = 0.4 the new rule delivers about 0.023 less than Revision 3 (0.770 against 0.793), and at T = 2 about 0.13 less.
- That is the price δ = 50 declares. At δ = 0.005 the change in OV is below 0.002. Buffered video keeps its buffer.

**Candidates rejected:**
- (a) **A pure headroom factor on T** keeps a standing queue proportional to T, which only moves the cliff from T ≈ 0.41 to T ≈ 0.52, and gives δ no seat.
- (b) **Copa's 1/δ queue as an absolute intake cap** drops data at T = ∞.
- (c) **Newest-first service** loses the CL-long backlog and reorders a T = ∞ stream behind the receiver's back.

The recommended law is (a)'s measured forecast priced as (b) asks (§2).

**Open questions** Q24–Q28 (§6), each with a default. **Tests**, written to fail first, and **a reported-only sizing run** are in §5.

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

## 2. The derivation

**The bet.** At a dequeue under overload (x = λ̂/μ̂ > 1), compare two choices for the head u, of age a:
- **Keep u.** §13.2 sends it.
- **Drop u now.** That is free if the overload lasts until §13.2 would have forced a drop anyway, because one forced drop then disappears. The share is unchanged, and every unit sent in between is one service time younger.

With the queue wait growing at x − 1 seconds per second, that forced drop comes when the head would reach the margin, after

```text
   t_r  =  (T − R − a) / (x − 1)
```

If the overload ends before t_r, the backlog drains, u would have been delivered, and dropping it loses one unit.

**The forecast (cited).** An overload that has lasted t_ov is still running after a further s with probability

```text
   P(persist ≥ s | elapsed t_ov)  =  t_ov / (t_ov + s)
```

That is the scale-free (Jeffreys) prior on an unknown duration: J. R. Gott III, *Nature* 363, 315 (1993), the "delta-t" argument. It has no constant, and it is the only prior on a duration with no scale to choose. Its known weakness is that it is calibrated only on average (C. M. Caves, 2000), so it gets a calibration gauge (§5.3). The odds of a refund are therefore `t_ov / t_r`.

**The price (declared).** Take the bet when the odds are at least `1/δ`.
- δ is the delay price against throughput: Copa's weight in `U = log(throughput) − δ·log(delay)`, paper §4.1 and §8.2, dimensionless.
- At δ = 1 the bet is taken at even odds. That is the same bar L2 uses with `W_COST_RATIO = 1` (§2.2), and the point where U weighs a relative change in throughput and in delay alike.
- At the Realtime point (δ = 50) the bet is taken at 1:50, at Auto (0.5) at 2:1, and at Bulk (0.005) at 200:1.
- **This is the definition of worth, and it adds no dial.** A unit's worth is one unit of delivered share, and δ is the exchange rate against the delay it saves. It does make δ a loss price in expectation. That answers Q12 "yes", with the price being δ (Q24).

```text
   drop u now   ⇔   t_ov / t_r  ≥  1/δ   ⇔   T − R − a  ≤  δ · t_ov · (x − 1)
```

Rearranged with the existing rule as the case x ≤ 1, this is §3's law. The equilibrium intake wait is `a* = T − R − δ·t_ov·(x − 1)` while that is positive. Once it is not, the younger-waiting clause leaves the newest unit.

**Copa's 1/δ queue is not used as a floor.** Copa keeps 1/δ packets queued so that the link stays busy. Here the store pause already keeps about 880 datagrams in the link buffer, so the intake needs no standing part. Adding one (1/(δ·μ̂) seconds: 2 µs at δ = 50, 22 ms at δ = 0.005) would add a term that has no job.

## 3. The law and its symbols

```text
   at each dequeue (toy2.py emit, after L1):
      expire the head u   ⇔   a(u) + R  ≥  τ   ∧   a younger unit is waiting          (§13.2 with T → τ)
      τ  =  T  −  δ · t_ov · (λ̂/μ̂ − 1)⁺
```

| symbol | provenance |
|---|---|
| T, a(u), R | as in §2.1 and §13. R is the ack timeout `srtt + max(4·rttvar, g) + ack_delay_max` |
| δ | declared, δ ∈ (0, ∞). It is finite by the config domain (NaN is already a config error, the `RWM_DELTA` precedent cited in §3 of the derivation; this revision adds +∞ as one), so `δ · t_ov · 0` is never ∞·0 |
| λ̂ | measured: units handed over per second over the rate law's window (the offline simulation's `rate_hat` window, 8·srtt; the engine's `rate_hat`). It counts hand-overs, not first sends |
| μ̂ | measured: the source capacity of the path. It is BBR's bottleneck estimate (Cardwell et al., *ACM Queue* 14(5), 2016: the windowed max of the delivery rate over 10 round trips, with app-limited samples kept only if they raise the max), converted to source units by the first-send share of payload sends over the same window. Rounds advance only on acks, so the estimate **holds through an outage**. It changes only on flow-limited samples (the store pause bound in that round) or on app-limited ones that exceed it. Before any such sample it is +∞: no evidence of a limit, so x = 0. It is never 0. In the engine the anchor's sampler exists but has no app-limited flag (paper §8.3), which is a prerequisite (Q26) |
| t_ov | measured: `now − t₀`, where t₀ is the last instant at which λ̂ ≤ μ̂. It is a timestamp of measured state, like "the last instant the queue was empty", and selects no law: at x = 1 the product is 0 whatever t_ov is |
| (·)⁺ | a refund needs a growing queue. This is the law's one clamp, gauged as `ov_frac` (the fraction of dequeues with x > 1; the bind fraction is 1 − ov_frac) |
| 1/δ odds | declared, §2 (Q24) |
| Gott's forecast | cited, §2 (Q25) |

**Shape check.**
- **Units:** τ [s] = T [s] − δ [1] · t_ov [s] · (x − 1) [1].
- **Monotonicity.** τ is non-decreasing in T, and non-increasing in δ, t_ov and x. More overload or a higher delay price gives a shorter wait. τ ≤ T always.
- **T = ∞:** τ = ∞ − finite = ∞. `a + R ≥ ∞` is false, so the rule never expires and the trace is unchanged. No `isinf` test is used.
- **Zero headroom (x = 1) and any x ≤ 1:** `(x − 1)⁺ = 0.0` exactly and `T − 0.0 = T` exactly, so this is §13.2 **by identity**. Requirement 2 holds wherever λ̂ ≤ μ̂ at every dequeue (§4.2).
- **δ → 0:** τ → T, Revision 3. Buffered video, at δ ≈ 0, keeps its buffer.
- **δ → ∞** (a limit only; the domain is finite): at any finite T and x > 1, τ → −∞, so the intake holds only the newest unit. At T = ∞ it is still inert for every finite δ.
- **x → ∞** (λ̂ ≫ μ̂): a collapse at the first dequeue after t_ov > 0. μ̂ > 0 keeps x finite.
- **Continuity.** τ is continuous in δ, T and x, including through x = 1. A step in t_ov at a reset happens only where (x − 1)⁺ is 0, so τ does not step. A ±2 % nudge of δ moves the term by ±2 %. τ < R simply means the head is always expired, so negative τ needs no clamp.

**NO-MODE-SWITCH.** The law is one expression. Nothing compares δ, T or ρ with a constant. `λ̂ ≤ μ̂` sets a timestamp, not a branch: the law's value is continuous across it. The younger-waiting clause is §13.2's, a condition on queue state.

## 4. Predictions (derived, not run)

Common numbers, from stage 3e OV under RL:
- C = 10 000/s, λ = 12 000/s, r ≈ 0.087;
- μ̂ = C/(1 + r) ≈ 9 200 source units/s, and x ≈ 1.30;
- R ≈ 0.102 s (srtt 98 ms plus two ticks);
- forward queue plus owd ≈ 0.093 s.

With repair off (S5 and R0), μ̂ ≈ C/(1 + ρ) and x ≈ 1.24.

### 4.1 OV

The collapse instant is `t_ov* = (T − R)/(δ·(x − 1))`.
- **Before the collapse** the intake grows as in Revision 3. In the first ≈ 0.1 s μ̂ is still +∞, because there has been no flow-limited round yet.
- **After the collapse** each burst of acks sends the newest unit and the next few hand-overs. The intake waits about the 0.4 ms pause gap.

| δ | T | t_ov* | delivered | on time ≤ 0.4 s | delay p50 | Revision 3 (stage 3e) |
|---|---|---|---|---|---|---|
| 50 | 0.4 | 20 ms | 0.770 | 0.770 | 0.095 s | 0.7929 / 0.7928 / 0.390 |
| 50 | 0.6 | 33 ms | 0.770 | 0.770 | 0.095 s | 0.8068 / 0.1175 / 0.590 |
| 50 | 1 | 60 ms | 0.770 | 0.770 | 0.095 s | 0.8348 / 0.1175 / 0.990 |
| 50 | 2 | 127 ms | 0.770 | 0.770 | 0.095 s | 0.9045 / 0.1175 / 1.685 |
| 50 | ∞ | ∞ | 1.0000 | 0.1175 | 1.685 s | identical |
| 0.5 | 0.4 | 2.0 s (the wait peaks at 0.20 s at 0.67 s) | 0.770 | 0.770 | 0.095 s | — |
| 0.5 | 2 | 12.7 s (the wait peaks at 1.27 s at 4.2 s, then falls to 0.40 s at 10 s) | ≈ 0.80 | ≈ 0.10 | ≈ 1.0 s | — |
| 0.005 | any | > 200 s | Revision 3 ± 0.002 | Revision 3 ± 0.002 | Revision 3 | — |

Derivation of the δ = 50 rows:
- The sent source units are `μ̂ × 10 s` with no flush, so delivered ≈ 0.767–0.772. The two bounds are `C/((1 + r)λ)` and the fitted intercept.
- Nearly every delivered unit arrives at ≈ 0.093 s.
- A lost one is now **protected at birth**: `0.098 + 0.093 < 0.4`. So FEC or a resend fills it at about 0.2–0.3 s, still on time.
- On time within 0.4 s therefore equals delivered to within 0.002.

**Against today's Realtime (0.8165 / 0.814 / 0.172 s), on the registered matched-share reading.**
- The new curve at δ = 50 is flat at (0.770, 0.770) for every finite T, and jumps to (1.0, 0.1175) only at T = ∞.
- Interpolated at 0.8165 that gives about 0.64. So the comparison is shown worse by ≈ 0.18, against 0.70 today. **The cell still FAILS.**
- The 0.0465 of delivered share Revision 4 lacks is the repair-displacement gap of the short answer. It is `C(1 − ε)/λ − C/((1 + r)λ)` ≈ 0.808 − 0.767, matching the measured 0.8165 − 0.770 to within the flush term.
- **Conditional prediction, not part of this revision.** With r held at 0 under saturation (Q22's spare-capacity limit), the young units can be resent. The row becomes `C/((1 + ρ)λ)` with ρ = ε/(1 − ε), so ≈ 0.808 delivered and on time at p50 ≈ 0.10 s. That is equivalent in share to today's Realtime and 70 ms faster.

**Repair off (S5, R0) at δ = 50, T = 0.4:**
- **Revision 4:** ≈ 0.808 / 0.808 / 0.095 s. Resends now pass L2 and displace first sends one for one, so the share is C(1 − ε)/λ with no flush.
- **Revision 3:** ≈ 0.83. Stage 3d's 82.8 % within 0.4 s includes the flush, and its first-send losses are not recovered.

**Strict order (H = ∞), δ = 50, T = 0.4 (re-derived, not carried over from §15).**
- Young units are protected, so F lags each lost seq until FEC or a resend fills it. A seq stays protected through two resends: `0.198 + 0.093 < 0.4`, and `0.298 + 0.093 < 0.4` narrowly.
- **Holes FEC misses** arrive at about 3 % × 9 200 × (1 − P_dec) per second, with P_dec ≈ 0.9 in OV, so about 28/s. Each blocks for about one R. That is about 2.8 open at once, so the strict frontier always trails the oldest open hole by 0.1–0.2 s.
- **It does not collapse as in §15.2:** the holes are filled, not given up a round trip late.
- **Predicted:** on time within 0.4 s 0.74–0.77, p50 0.18–0.25 s.
- **Resends** run at about 30–280/s, against the 2 000/s budget.
- **Confidence is lower than for H = 0.** The second-resend margin is 9 ms, and §19 found ô undershooting a persistent queue.

### 4.2 CL-long, CL, CLw-long, RT and the K cells

**Identity, by arithmetic.** In these cells x ≤ 1 at every dequeue, so every row is bit-identical to Revision 3, with traces equal:
- **Before an outage** the sender is app-limited (λ = 10 000 < C), so μ̂ stays +∞.
- **Through an outage** μ̂ holds, because no round completes.
- **After CL-long's outage** the first flow-limited rounds read the drain rate: about 11 750/s repair-off (stage 3c) and about `12 500/(1 + 0.107 + ρ)` ≈ 10 700/s under RL, both above λ.
- **CLw-long:** `12 500/1.36` > 8 000 even at r = 0.36.

That covers S5, R0, RL and RS at every T, every T = ∞ arm, and the proof stage 1 recovery and on-time curves. The gauge `deq_early` (expiries with `a + R < T ≤ a + R + δ·t_ov·(x − 1)⁺`) must read 0 there.

**Arms that change, by design.**
- **RF3 in CL-long and CL:** r = 0.36 gives `μ̂ = 12 500/1.4` ≈ 8 900 < 10 000, so x ≈ 1.12 and the arm is overloaded by its own repair. At T = 0.4, t_ov* ≈ 65 ms, then it collapses as in OV. Predicted: delay p50 falls from about T − 10 ms to ≈ link + owd, and delivered and on time both fall by about the flush.
- **RE in CL-long:** ε̂ ≈ 0.3 raises r to its 0.5 cap after the outage, so `μ̂ = 12 500/1.53` ≈ 8 200 and the same applies while it lasts.

These are real overloads, and the rule treats them as such. Reported, not under the verdict.

### 4.3 Two paths

- μ̂ is the sum over paths of each path's held capacity, and x = λ̂/Σμ̂_p.
- **When one path is in an outage**, its held capacity still counts. That overstates the capacity, so x reads low and the rule stays at Revision 3: the safe side, with no gain.
- **A slow path whose ô exceeds T** also counts at full capacity. Again this is the safe side.
- The P2 cells run below capacity, so they are predicted identical.

## 5. Risks, tests and the sizing run

### 5.1 Risks

- **Oscillation.**
  - τ reads none of the quantities it moves. λ̂ is the application's rate. μ̂ changes only on flow-limited samples, and after a collapse the sender is still pause-bound in every round. t_ov follows λ̂ against μ̂, not the intake.
  - A collapse leaves srtt, and so R and the rate law's r, unchanged, because the link queue is set by B.
  - Noise in λ̂/μ̂ near 1 only resets t_ov, which moves τ toward T, the safe side.
- **The knife edge at x ≈ 1, amplified by δ.**
  - At δ = 50 a 1 % bias in x sustained over 0.8 s gives `50·0.8·0.01 = 0.4 s > T − R`.
  - **Engine:** without the app-limited exclusion, an app-limited sender reads μ̂ ≈ λ, so ordinary noise could collapse a call at headroom (Q26).
  - **Offline simulation:** μ̂ = +∞ until a flow-limited round, so the edge is reached only by a load genuinely at capacity. Cell OV-crit (§5.3) measures it.
- **The store pause.**
  - The rule runs only at a dequeue, as §13.2 does. During a pause, only L1 expires.
  - During an outage, μ̂ holds and the rule is §13.2's. The pause still sets O ≤ B and is not changed.
- **The floor law.**
  - Units sent young are protected, so the floor no longer sits at the send frontier under overload. Resends return, and with them ρ in μ̂ (§4.1).
  - The commitment test (no copy below an announced floor) and L1's test are unchanged and still absolute.
- **The bet's cost at the end of an overload.** It is about `μ̂·(T − R)` units per episode at δ = 50: 2 760 units at T = 0.4. Cell OV-end measures it.
- **Cold start.** Before the first sample R = 1.0 s, so at T = 0.4 §13.2 already expires all but the newest unit (§18's cold-start defect). Revision 4 adds nothing there: μ̂ = +∞ until the first flow-limited round.
- **Two paths:** §4.3, safe but without gain.

### 5.2 Tests (red first)

Each test is written before the code and fails until it lands. The FORMULA-FIRST rule also requires the law and §2's derivation in the paper (§5.6's neighbourhood, or a new §5.6b) before the code.

1. **`wait_limit` inert at T = ∞.** τ is +∞ for every δ ∈ {0.005, 0.5, 50, 10⁶}, x ∈ {0, 1, 1.3, 100} and t_ov ∈ {0, 10⁶}, and never NaN. A source deny test forbids `isinf`, `isfinite` and any comparison of δ, T or x with a literal in the function.
2. **The identity at headroom.** τ == T exactly for every x ≤ 1, at any t_ov and δ.
3. **Shape.** τ is non-increasing in δ, t_ov and x, and non-decreasing in T. A ±2 % continuity test holds at each δ preset and at T = 0.4, and so does continuity through x = 1 ± 10⁻⁹.
4. **The capacity estimate holds through an outage.** In CL-long, μ̂ at the first dequeue after the outage equals its value before the outage, or +∞ when there was none. It is never 0.
5. **CL-long identity with an execution witness.** S5, R0, RL and RS at T ∈ {0.4, 2, ∞} have trace digests equal to Revision 3's, `ov_frac` = 0 and `deq_early` = 0, in all 12 seeds. This test is red against a rule fed the instantaneous send rate, which collapses after the outage.
6. **OV executes.** At δ = 50 and T = 0.4, `deq_early` > 0. After `t_ov*` plus one round, every first send has an intake wait no greater than the longest pause gap. This is an absolute bound, read from `deq_log`.
7. **The burst control.** CL with a planted 200-unit hand-over burst at δ = 50 gives `deq_early` = 0, because the windowed λ̂ stays below μ̂. It is red against a Lindley-integral variant of the term.
8. **Starvation.** With τ < R under overload, first sends per round are greater than 0. The younger-waiting clause still binds.
9. **Unchanged absolutes.** `l1_viol` = 0, the commitment violations are 0, and the T = ∞ rows are identical to full delivery in every cell.

### 5.3 Sizing run (reported only, no verdict)

- **Cells:** OV, OV-long, CL-long and CLw-long, and two new cells:
  - **OV-crit:** λ = C/(1 + r) ≈ 9 200/s, the knife edge;
  - **OV-end:** λ = 12 000/s for 5 s, then 8 000/s to 15 s. It measures the cost of the bet when an overload ends.
- **Arms:** Revision 4's S5 and RL at T ∈ {0.4, 0.6, 1, 2, ∞} × δ ∈ {0.005, 0.5, 50}, H = 0, plus H = ∞ at T = 0.4 and δ = 50. Revision 3's rows reuse stage 3e where they exist.
- **Size:** 32 configurations per cell, 3 seeds, so 576 runs. At stage 3e's TIME rates for the new model (7.7–34 s per run) that is about 20–40 min on 9 workers.
- **Readouts:**
  - delivered, on time within 0.4 s and within T, and delay p50 and p90;
  - `deq_early`, `ov_frac`, the collapse instant, the fraction of time μ̂ = +∞, and the resend rate;
  - the Gott calibration: for each overload episode, the forecast `t_ov/(t_ov + s)` against whether it persisted for s, binned;
  - for OV-end, the units lost to the bet against the predicted `μ̂·(T − R)`.

## 6. Owner questions (numbering continues from Q23)

- **Q24, δ as the bet's price (answers Q12).** Accept that δ prices expected loss through the 1/δ odds bar? At the Realtime point this gives up the end-of-overload flush, 0.023 in OV at T = 0.4. **Default: yes.** The alternative is odds of `δ_auto/δ` (paper §4.1's ζ), which puts Auto at even odds, as L2 does, and halves the term.
- **Q25, the forecast.** Gott's scale-free prior (no constant, cited, calibration gauged), or a declared horizon. **Default: Gott.** Every measured horizon examined (R, the rate window) is a constant of the estimator, not of the overload.
- **Q26, the capacity estimate in the engine.** The BtlBw anchor's sampler has no app-limited flag (paper §8.3) and its window form is unread. **Default:** add BBR's app-limited rule and a round-counted window as a prerequisite of the engine step. Revision 4 does not ship at the Realtime point without them (§5.1, the knife edge).
- **Q27, the next law.** The spare-capacity limit on r under saturation (Q22). It closes the remaining OV gap (§4.1). **Default: register it next, ahead of an OV re-read.**
- **Q28, a standing part of the intake.** Copa's 1/δ queue as an intake floor. **Default: no** (§2): the store pause already keeps the link busy.

## 7. Effect on the earlier sections

- **§13.2:** the rule is `a + R ≥ τ`. Its paragraph "What cannot be fixed" is withdrawn, and the young-first order it mentions is rejected again (§8 item c), for a different reason.
- **§5's overload row:** under sustained overload the intake no longer expires from a head T old, except at δ → 0 or T = ∞.
- **§2.9, what δ sets,** gains one job: the sender's intake under overload. That is the seat the deleted D(δ) had, now priced as a bet and not as a give-up age.
- **§17.10's open item "delay pinned near T − R under overload":** answered, with the repair-displacement gap moved to Q27.
- **Not changed:** L1, L2, the protected floor, the brake, the store pause, the wire format and the receiver.

## 8. The candidates, evaluated

### (a) τ scaled by the measured headroom

**Plain form:** `τ = T · min(1, μ̂/λ̂)`. μ̂ is the held capacity, not the send rate, since otherwise an outage gives `T·0` and ∞·0 at T = ∞.
- **Shape.**
  - Inert at T = ∞, because the factor is at least μ̂/λ̂ > 0.
  - Identity at x ≤ 1.
  - Continuous, with no constant.
- **OV (x = 1.30):**
  - T = 0.4: hold `0.77·0.4 − R` = 0.21 s, p50 0.30 s, on time 0.786 (including a flush of 0.016).
  - T = 0.6: hold 0.36 s, p50 0.46 s, on time ≈ 0.12.
- So the cliff only moves from T ≈ 0.41 to T ≈ 0.52.
- **CL-long:** identity.
- **Risks:** none new, but the queue it leaves is standing and proportional to T, and δ has no seat.
- **Variants.**
  - Any power `(μ̂/λ̂)^k` is a tuned k.
  - "Drop the excess from the start" (a horizon-free fluid rule) drops at T = ∞.
  - The fluid forecast with a horizon proportional to T fails at T = ∞ (§1).
  - With a measured horizon it becomes §3's law at δ = 1. **Rejected as stated; its forecast is kept.**

### (b) A priced form

**Copa's standing queue as the intake's cap:** `b ≤ 1/δ` units, that is, `τ_q = 1/(δ·μ̂)`.
- **Worth, without a new dial:** one queued unit costs δ, from the equilibrium δ·Q = 1 (paper §8.2).
- **Shape.**
  - Units are consistent.
  - It is monotone in δ: 2 µs at δ = 50, 2 units at Auto, 22 ms at Bulk.
  - At T = ∞ it still drops under overload, so **requirement 1 fails**.
  - At headroom it drops CL-long's backlog after the outage, so **requirement 2 fails**.
- **OV:** p50 ≈ 0.093 s, delivered ≈ 0.770 at every T including ∞.
- **Combined with T** as `min(T, ·)` it fails at T = ∞ (§1).
- **Rejected as a cap.** What survives is δ as the price of a bet whose stake is set by T and whose odds come from the forecast (§2). That is how worth is defined without a new dial: a unit is worth one unit of share, and δ is the exchange rate.

### (c) Newest-first service within the lifetime under overload

- **OV:** p50 ≈ 0.093 s, delivered as in (b), with no flush at the end: the old backlog expires at T.
- **CL-long fails requirement 2.**
  - After the outage the surplus, about 1 750–2 500/s, serves the backlog from its newest end, while fresh hand-overs go first.
  - The oldest backlog units then wait `b/h` ≫ T − R instead of `b/μ̂` ≤ T − R.
  - Most of the λ·(T − R) ≈ 3 900 units FIFO delivers on time expire. That is −1.5 to −2 points on time, against leads of 1.3–2.7 points.
- **Order.**
  - `emit` assigns the seq at the dequeue, so newest-first leaves the wire in seq order. **H and the floor cannot see the reordering.**
  - The application receives units out of hand-over order. The transport can neither hold nor count that disorder.
  - A strict-order consumer (H = ∞) gets seq order, which is no longer its own order.
  - At T = ∞ a byte stream would be corrupted unless the service reduced to FIFO, which needs a selector: the mode bit §13.2 already refused.
  - Fixing it needs an application-order field on the wire, which breaks requirement 5.
- **Rejected.**
