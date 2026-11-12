# W1 round 2: a δ-free ProbeRtt that keeps the refresh and drops most of its cost

Derivation only: no code, no cargo, no VM. This is plan W1 round 2, the first δ-free fix the §48b user decision asks for ("continue `bbr_rs`").

**Sources**
- `bbr_rs.rs` and `cc_sim.rs`: branch `bbr48cand` at `e5d7ffbd`. That is the CAND measured in §48b.
- Status: `raptorpath/docs/status.md` on main `1425acbd`, sections §47, §48 (+ Amendment 2), §48b (+ Result, Erratum and supplement, user decision).
- Supplement: `docs/l1-raw/bbr48b/{supplement.txt, supp48.py, c2c8-diag-qcc.tgz}`.
- Host-gate output: `docs/l1-raw/bbrfix48/green.txt` on `bbr48cand`.
- W1.2: `derivations/w1-who-limits.md` (`derive-w1`).
- W2: `derivations/w2-pacing-seam.md` (`adr68-amend`).

**External practice.** Fetched from draft-ietf-ccwg-bbr-07 (BBRv3) through a summarising fetch. Only these items are quoted verbatim:
- ProbeRTTCwndGain = 0.5;
- ProbeRTTDuration = 200 ms, "minimum duration for which ProbeRTT state holds C.inflight to at most ProbeRTTCwnd()";
- MinRTTFilterLen = 10 s;
- "ProbeRTT [should] sacrifice no more than roughly 1% of a flow's available bandwidth";
- "when restarting from idle and finding that the BBR.min_rtt has expired, BBR does not enter ProbeRTT";
- the min filter test `RS.rtt < BBR.min_rtt || BBR.min_rtt_expired`.

Everything else about v1, v2 or v3 is marked **to verify**. That includes the pseudocode ordering of `probe_rtt_cwnd` against the min_rtt reset, and whether a separate 5 s probe interval exists.

## 0. The formula first

ProbeRtt's goodput cost per filter period P ≈ 10 s, on one leg. This uses the loop identity of the §48b erratum: quinn ignores `pacing_rate`, so x = min(L, W/R̄).

```text
   C_R  =  (1/P) · ∫_R (1 − x(t)/L) dt
        =  (1/P) · [ t_q · 0  +  t_gap · 1  +  T_clean · (1 − f·(b̂/L)·m̂_R/R̄₀) ]
        =  (1/P) · (R̄₀ − f·(b̂/L)·m̂_R) · (1 + T_clean/R̄₀)                       (b̂ ≈ L; m̂_R = the m̂ the drained window is built on)
```

- **t_q.** The time for in-flight to fall from W to the pipe, L·R̄₀. The bottleneck queue is still emptying, so the link stays busy and **no goodput is lost**.
- **t_gap.** The time for in-flight to fall from L·R̄₀ to W_R = f·b̂·m̂_R, which is R̄₀ − f·(b̂/L)·m̂_R. The queue is empty and the sender is blocked, so **the link idles**. That is one round-equivalent of full loss.
- **T_clean.** The time held at W_R, measured from the drain moment (in-flight < W_R + MTU). The link runs at f·(b̂/L)·m̂_R/R̄₀ of L.
- **R̄₀.** The loop time at zero standing queue: path delay plus jitter plus host and ACK delay. R̄₀ ≥ m̂, because m̂ is a minimum.

**The shipped form** (`bbr_rs.rs:610–671`):
- f = 0.75 (`K_MODERATE_PROBE_RTT_MULTIPLIER`).
- T_clean = 200 ms + ≤ 1 round (`K_PROBE_RTT_TIME`; exit on the next round start after the timer).
- m̂_R is the **loaded** r_e. At expiry, `:839–841` sets m̂ := r_e, and r_e ≈ R̄ = W/x ≈ 2.8·m̂ at c2 (`qsr` 21.7 ms against m̂ 7.81 ms). The window then descends geometrically as lower samples arrive, by f per round. With this base, f < 1 is what makes the drain converge.

**Measured, and checked against the formula (c2):**
- The `[DIAG]`/`[QCC]` tick is 250 ms (`c2-100@def-CAND-s42-r1`).
- The row's R ticks fall at 10.3, 20.6, 30.8 and 51.1 s: one ProbeRtt per ≈ 10.2 s. A 0.2 s episode is caught by a 250 ms tick 4 times in 5.
- So the time share is 0.2/10.2 = 1.96 %, against the measured line share of 2.0 %.
- With R̄₀ = 10.2–11.8 ms (§43b `rtp_us` p50) and m̂ = 7.81 ms, the drained rate is x/L = 0.75·7.81/R̄₀ = 0.50–0.57, and R̄₀ − f·m̂ = 4.3–5.9 ms.
- **Predicted C_R = (4.3–5.9 ms)·(1 + 200/R̄₀)/10.2 s = 0.88–1.05 %; measured 0.84 %.**
- The line estimate reads low by O(T_R/tick), because a 250 ms `good` dilutes a 200 ms episode. The ≈ 0.8 % figure is therefore a lower edge, not a point.

**Two failure directions, and what guards each:**

| direction | mechanism | what guards it today |
|---|---|---|
| **latched low** (the §47 collapse) | one early stamp becomes a lifetime minimum, so W = 2·b̂·m̂ falls far below the pipe | the ack clock (r_e ≥ RTprop, proven, §48 design 1) and the 10 s window (§48 design 2). **Not** the drain. |
| **ratchet up** (the opposite) | at expiry, m̂ := loaded r_e. Without a clean sample, W = 2·b̂·m̂ + A feeds on its own queue: window-limited, R̄ = W/x, so m̂ ≈ doubles per period (W1.2 candidate B's self-feedback) | **the drain**: ProbeRtt holds in-flight below the pipe long enough to read the floor |

So the collapse fix does not depend on ProbeRtt. ProbeRtt is what stops the windowed filter from inflating. Every form below is judged on both directions.

## 1. The three candidates

### (a) Skip ProbeRtt when the filter refreshed naturally: already shipped; gain ≈ 0

- `:839` restamps t_m on `r_e ≤ m̂`, which is v1's natural refresh (to verify: v1 `bbr_update_min_rtt` uses `<`; v3-07 quotes `<`). Tests 8 and the tie test pin it.
- So the skip exists. It never fires at c2 or c8 because the loop is **window-limited**: r_e ≈ R̄ = W/x ≈ 2·m̂ + A/x > m̂ (§48b erratum identity). The 1.96 % = 2.0 % match above shows ProbeRtt fires every period.
- **Predicted saving: 0 % at c2 and c8.** It saves something only where the window does not bind, and there ProbeRtt already does not fire.
- **A loosened freshness test,** r_e ≤ (1 + ε)·m̂:
  - It is a tuned threshold (FORMULA-FIRST).
  - On window-bound lines it fires only for ε ≥ 1 + A/(b̂·m̂). It then installs a loaded sample, which is the ratchet up.
  - **Rejected.**
- **v3's idle-restart skip** (quoted): it needs a restart-from-idle event. `bbr_rs` has one in name only: `exiting_quiescence` is never set true (`:377`, `:421`, `:650`). The skip is inert, and the flag is dead code to remove with the fix.

### (b) A shallower drain, W_R = f·b̂·m̂_R: depth is second order; keep f = 0.75

**The admissible f** comes from the loop identity, on a base m̂_R that is **not** loaded (see the chosen form, item 1):
- Write R_floor·(1 + u) for the clean round's min when no queue forms. Here u is the shortfall of a finite-sample min above the floor (u > 0 under jitter).
- If W_R = f·b̂·m̂_k exceeds the pipe, the clean round is window-limited. Its loop time is W_R/L = f·(b̂/L)·m̂_k, and that is what its min reads.
- So, across filter periods k:

```text
   m̂_{k+1}  ≈  max( R_floor·(1+u),  f·(b̂/L)·m̂_k )
```

- This map contracts to R_floor·(1 + u) iff **f·(b̂/L) < 1**, whatever u is. u sets the fixed point, not stability.
- If f·(b̂/L) ≥ 1, m̂ grows by f·b̂/L per period: the ratchet up.

**Numbers (sim, `green.txt`):** b̂/L = 1.03–1.07 outside ProbeRtt (the max filter over-reads by up to 7 %), so **f_max = L/b̂ ≈ 0.93**.

**f = 1 is rejected.** f·b̂/L = 1.03–1.07 > 1: a slow ratchet at the measured over-read, and no contraction at all at b̂ = L.

**f = 0.75 (shipped) contracts at ×0.80 per period** at the measured top of b̂/L. Its margin is ≈ 20 %, and it tolerates a 33 % over-read.
- **The cost of the new base:** a high read decays geometrically, ×0.8 per 10 s period. The shipped loaded-reset form descends within one episode (≈ 20 rounds at ×0.75).
  - Anchor, sim: the first period's m̂ = 10.8 ms against the 200 ms min of 8.1 ms (`green.txt`). That read would take ≈ 1–2 periods to reach the fixed point, max(8.1·(1 + u), 0.8 × 10.8 = 8.6 ms).
  - The error is on the safe side (a larger window) and bounded. It is stated here and gauged (`qrt_us` per period).

**The cost difference** across the admissible band, at T_clean ≈ 2–3·R̄₀ (form (c)):

```text
   ΔC_R = Δf · (b̂/L)·m̂ · (1 + T_clean/R̄₀)/P  ≈  0.15 · 7.8 ms · 3 / 10.2 s  ≈  0.03 %      (f 0.75 → 0.90, c2)
```

- That is far below any guard resolution (c2 goodput d80 = 1 %).
- **Shallower is not upstream practice either:** v3-07's gain of 0.5 is deeper.
- **Rejected as a lever. f stays 0.75:** no new constant, a margin against both b̂ over-read and u.

### (c) Duration from the measured loop, not 200 ms: the lever

**The lower bound: one clean round.**
- m̂ must be a min over ACK events whose packets were all **sent** at in-flight ≤ W_R. Otherwise the queue the drain is emptying inflates them.
- Mark P_d = max sent packet number at the drain moment (in-flight first < W_R + MTU).
  - The first ACK of a packet > P_d arrives ≈ R̄₀ later. At that moment E = max sent packet number.
  - Every packet in (P_d, E] was sent at the drained window. Once all of them are acked, one full clean round has been sampled.
- **Evaluation order (fixed):**
  - E is set on one ACK event. The exit test runs only on later events.
  - The drain-moment event's own sample, whose packets were sent before the drain, does not enter m̃.
- So T_clean ≈ 2·R̄₀, at most ≈ 3·R̄₀ with ACK delay and jitter.

**The upper bound: 10 rounds.**
- b̂ is a 10-round windowed max (`:114`, `:295`). ProbeRtt's own delivery samples enter it: the sampler does not exclude R. Verified in sim: b̂/L dipped to **0.608** inside the 210 ms ProbeRtt (`green.txt`, c2-jitter).
- A hold longer than 10 rounds lets b̂ re-learn the drained rate. W_R = f·b̂·m̂ then compounds downward inside R. After exit, W = 2·b̂_decayed·m̂ + A must regrow round by round.
- 200 ms is ≈ 18–20 rounds at c2, ≈ 90 at c1 and ≈ 4–5 on the c3 leg. So at c1 and c2 the shipped hold breaks this bound by 2–9×.
- (v1 excludes ProbeRtt samples from the max filter? **to verify**; `bbr_rs` does not.)

**The duration: T_clean = the clean round.** It is inside [1, 10] rounds at every cell and has no millisecond constant. It scales with the leg's own loop: ≈ 5–8 ms at c1, ≈ 20–35 ms at c2, ≈ 90–145 ms on the c3 leg.

**What is lost:**
- **v1/v3's 200 ms floor** has a rationale beyond the floor read: holding long enough that competing BBR flows' ProbeRtts overlap on a shared bottleneck (**to verify**). No §48 cell has cross traffic. On a shared bottleneck, a one-round probe reads the queue other flows hold, and the result errs **high**: the safe side for the collapse, the queue side for delay. This is stated as a risk, not measured.
- **The min is noisier.** Fewer samples make the min read high: ≈ 26 ACK events against ≈ 500 at c2. Under the triangular tail E[min_N] − floor ≈ 7.5 ms/√N, so the read shifts by ≈ +1.1 ms at c2.
  - Prediction: c2 median `qrt_us` 7.81 → **7.8–9.2 ms** (the sim's 200 ms min read 8.1 ms);
  - c1 2.15–2.20 → 2.15–2.4 ms;
  - c3 39.4 → 39.4–41 ms.
  - W = 2·b̂·m̂ + A rises by the same ratio. That is a slightly larger margin and slightly more wire queue: c2 `wrtt` 21.8 → **21.8–25 ms**, still ≤ 0.3 × BASE.

## 2. The chosen form

A one-round ProbeRtt, built on the pre-expiry model, that installs a fresh min:

```text
   on expiry (age(t_m) > 10 s):  enter R;  m̂ is NOT reset to the loaded r_e;  m̃ := +∞
   in R:   W_R = max( f · b̂ · m̂ , min_cwnd ),   f = 0.75 (unchanged), m̂ = the pre-expiry model
           drain moment: the first event in R with in_flight < W_R + MTU;  P_d := max_sent   (its sample is NOT taken)
           every later event:  m̃ := min(m̃, r_e)
           E := max_sent at the first event after the drain moment with max_acked > P_d
           exit at the first event AFTER the one that set E with max_acked ≥ E:
               m̂ := m̃,  t_m := a_e,  → ProbeBw (or Startup if not at full bw)
   outside R: unchanged (r_e ≤ m̂ restamps; v1 natural refresh)
```

- In harness `H`, where every event acks everything it sent, an entry with in_flight = 0 is its own drain moment. E is set on event +1 and exit is on event +2, with m̃ = min(r_e at +1, +2).
- In a real network, E is set ≈ R̄₀ after the drain moment and exit comes ≈ R̄₀ later: T_clean ≈ 2–3·R̄₀.

Code deltas, specified for the worker, not written here:
- `:839` drops `|| min_rtt_expired`. Expiry now always enters R, since `exiting_quiescence` is dead (remove it).
- `maybe_enter_or_exit_probe_rtt` replaces `K_PROBE_RTT_TIME` with the (P_d, E) round key and installs m̃ on exit.
- New fields: `probe_rtt_min` (m̃), `probe_rtt_drain_pn` (P_d), `probe_rtt_end_pn` (E).
- New `[QCC]` counters per path, cumulative:
  - `qrn`: R entries;
  - `qrus`: µs in R, drain included;
  - `qrb`: bytes acked in R;
  - `qab`: bytes acked in total;
  - `qage_us`: the min filter's age a_e − t_m at the tick.

**Both failure directions:**
- **Latched low.** m̂ := m̃ on exit is a min over this period's clean round only. Whatever m̂ held before, low or high, is discarded every period. With r_e ≥ RTprop proven, m̃ ≥ RTprop. A stale-low m̂ costs one drained round at W_R = max(f·b̂·m̂_low, min_cwnd), then it is gone. The shipped reset-to-loaded already unlatched, so this guard is kept, not added.
- **Ratchet up.**
  - Bounded by the contraction f·(b̂/L) < 1 (section 1b): 0.80 at the measured top b̂/L of 1.07, a margin of ≈ 20 %.
  - The price is that a high read decays at ×0.8 per period instead of within one episode.
  - Spec T6 (section 4) measures it, with a mutation run that proves it can see a ratchet.

**Predicted cost.** From section 0: C_R = (R̄₀ − f·m̂)·(1 + k)/P, with k = T_clean/R̄₀ ∈ [2, 3]. R̄₀ comes from §43b and §48b R4; t_q costs 0, and t_gap is one round-equivalent.

| cell / leg | R̄₀ − f·m̂ | T_clean | C_R now (measured) | C_R predicted | saving |
|---|---|---|---|---|---|
| c2 | 10.2–11.8 − 5.86 = 4.3–5.9 ms | 20–35 ms | 0.84 % (lower edge; model 0.88–1.05 %) | **0.13–0.23 %** | ≈ 0.6–0.7 pp |
| c8 fast leg (share 0.823) | ≈ 10.5–12 − 6.24 = 4.3–5.8 ms | 21–36 ms | ≈ 0.66 (model split) | ≈ 0.10–0.19 % | |
| c8 slow c3 leg (share 0.177) | ≈ 42–48 − 29.6 = 12.5–18.5 ms | 84–144 ms (vs 200 + ≤ 48) | ≈ 0.12 (model split) | ≈ 0.07–0.13 % | |
| **c8 total** | | | 0.78 % | **0.17–0.32 % (bound ≤ 0.4 %)** | ≈ 0.45–0.6 pp |
| c1s / c1d | ≈ 0.55–0.85 ms | 4–8 ms | ProbeRtt 1.2–2.0 % of blocks, not costed | ≤ 0.05 % | — |

- The c8 leg split is a model: per-leg share × duty × depth, which sums to 0.85 % against the measured 0.78 %. The per-leg counters (`qrus`, `qrb`, `qab`) replace it in §48d.
- **Second-order saving (not credited):** with T_clean < 10 rounds, b̂ keeps its pre-probe max (no 0.608 dip), so the post-exit regrowth rounds vanish. These are a small part of §48b's unattributed rest.

**Time in R per 10 s** (drain included):
- c1 ≤ 0.1 %;
- c2 ≤ 0.5 % (drain ≈ (W − W_R)/L ≈ 15 ms + A/L, plus 20–35 ms);
- c3 leg ≤ 2.5 % (drain ≈ 50 ms + A/L, plus 84–144 ms; the leg's time barely shrinks, but its drain costs no goodput).

**What it leaves.**
- §48b's deficit was 1.48 % at c2 and 2.56 % at c8. After this fix: **c2 ≈ −0.8 % (−0.5 … −1.3 %), c8 ≈ −2.0 % (−1.5 … −2.6 %)** against BASE.
- The unattributed rest (≈ 0.6 % at c2, ≈ 1.8 % at c8) and the c8 CPU are untouched. This fix alone does not make `PROMOTE-ELIGIBLE` likely (see the §48d draft).

## 3. NO-MODE-SWITCH reading

- **ProbeRtt is a BBR state:** a time-multiplexed phase of one controller, entered on the filter's age. It is not a δ, ρ or hint mode.
  - The invariant forbids a threshold on δ or ρ that selects a law, and a hint branch.
  - Neither the shipped nor the chosen form reads δ, ρ or a hint. The controller stays δ-free (paper §8.4 "No mode switch" stands).
- **The change adds no state and removes none.** It changes R's window base (pre-expiry m̂), its exit key (a round key instead of 200 ms) and what R installs (m̃).
  - It removes one constant (200 ms), one branch (`|| min_rtt_expired` in the filter update) and one dead flag (`exiting_quiescence`).
  - It adds one variable (m̃) and two packet-number marks.
- **Continuity in the dial:** nothing in R depends on δ, so W is identical across hints. The spec in section 4 checks this.
- **Continuity in time:** R is still a periodic window dip, now ≈ 5–35 ms on c1 and c2 instead of ≈ 200 ms. That is the discontinuity the user was told about in §48 Amendment 2.
- **The state-free alternative** is W2's (d): a Copa-style continuous dither that drains by a ±v dip every few RTTs. It would remove R as a state. It is out of scope here: it is a law change, and its FEC protection gap on the shallow and jitter cells is open (W2 §2).

## 4. Red-first test specs (absolute)

"Red on `e5d7ffbd`" gives the shipped value that fails the assertion.

**Unit harness:** `H` (`bbr_rs.rs:1367–1404`). Every `event` is one round (each event's packets are acked at once), so in unit specs a "round" is an event.

**T1 `probe_rtt_window_is_f_times_the_pre_expiry_model`** (unit, **red first**)
- Setup:
  - plant m̂ = 10 ms at t = 1 s;
  - events every 10 ms, each `n` = 100 packets of 1 200 B, r_e = 25 ms (loaded; no refresh), in_flight = 10⁶;
  - so b̂ ≈ 12 MB/s.
- **Precondition, non-vacuity:** b̂ = `qcc().bw` ≥ 1 MB/s. Then 0.75·b̂·10 ms ≥ 7 500 B > min_cwnd = 4 800 B.
- At the first event past t_m + 10 s:
  - mode = R;
  - `window()` = ⌊0.75 · b̂ · 10 ms⌋ ± 1 byte, about 90 000 B, with b̂ read at the same event;
  - `qcc().rt_us` = 10 000 (m̂ not reset).
- Red on `e5d7ffbd`: m̂ := 25 ms at expiry, so `window()` ≈ 0.75·b̂·25 ms ≈ 225 000 B and `rt_us` = 25 000.

**T2 `probe_rtt_holds_one_clean_round_at_two_rtts`** (unit, **red first**)
- Two anchors, m̂ = 10 ms and m̂ = 40 ms, each planted and expired.
- in_flight = 0 from the entry event on, so the entry event is the drain moment.
- Then events every m̂ with r_e = m̂.
- Assert, by the evaluation order in section 2:
  - E is set on entry + 1 and exit is on entry + 2, **exactly**;
  - so the time in R is exactly 2·m̂: 20 ms and 80 ms.
- Red on `e5d7ffbd`: R lasts ≥ 200 ms at both anchors (≥ 20 and ≥ 5 events).

**T3 `probe_rtt_exit_installs_the_fresh_round_min`** (unit; both cases are guards, expected green on both trees; the red-first discriminator of the base change is T1)
- Case A:
  - planted m̂ = 10 ms; events every 100 ms;
  - the entry event carries a loaded r_e = 25 ms (it is the drain moment with in_flight = 0, so it is excluded from m̃);
  - entry + 1 carries 12.0 ms, entry + 2 carries 11.5 ms, and later events carry 11.8 ms (the path rose);
  - after exit, `rt_us` = 11 500 exactly.
  - `e5d7ffbd` reaches the same value by the `≤` ratchet from the loaded expiry sample.
- Case B, **"a stale min-RTT still gets refreshed"** at the unit level:
  - planted m̂ = 50 µs (the §47 latch scale); events every 100 ms of 100 × 1 200 B, so b̂ ≈ 1.2 MB/s; r_e = 2 ms throughout;
  - R is entered at the first event past t_m + 10 s;
  - `window()` in R = min_cwnd = 4 800 B, since 0.75·b̂·50 µs ≈ 45 B;
  - after exit (entry + 2), `rt_us` = 2 000 and `window()` ≥ 2·b̂·2 ms.

**T4 `probe_rtt_time_and_reduced_window_per_10s_at_c2`** (`cc_sim`, **red first**)
- Setup: the c2-jitter `Sc` (100 Mbit/s, 5 ms ± 3 ms per direction, in-order release), with `stop_after_probe_rtt: None`, 32 s, bulk.
- Over [1, 32) s, per 10 s window:
  - share(mode = R) ≤ 0.008;
  - share(`window()` < b̂·m̂) ≤ 0.008;
  - the ProbeRtt entry count is exactly 3, one per period.
- Red on `e5d7ffbd`: 210 ms per period, so share ≈ 0.021 (`green.txt`: R 10.012–10.222 s).
- Printed: T_clean per cycle in units of the cycle's mean r_e. It must sit in [1, 3].

**T5 `bbr_rs_c2_jitter_keeps_bhat_at_the_link_rate_inside_probe_rtt`** (`cc_sim`, **red first**)
- This extends the existing jitter test. Its "in ProbeRtt min" is now asserted: b̂/L ≥ 0.9 inside R.
- Red on `e5d7ffbd`: 0.608 (`green.txt`).

**T6 `probe_rtt_min_does_not_ratchet_up_under_jitter`** (`cc_sim`, a guard green on both trees, plus a mutation check)
- Same run as T4. At each of the 3 exits:
  - m̂_k < 10.0 ms (the nominal mean RTT 2 × 5 ms: a ratchet crosses it);
  - m̂_3 ≤ m̂_1 + 1.0 ms.
- Printed: m̂_k/m̂_{k−1}, against the predicted ≥ 0.8 decay toward the fixed point.
- **Mutation check** (measurement discipline rule 1: prove the test can see the mechanism):
  - The same run with f = 1.25 through a `#[cfg(test)]` override, so f·b̂/L ≈ 1.3 and m̂ grows ≈ ×1.3 per period, must be **red**. Its output is saved.
  - f = 1.0 is not used: at b̂/L = 1.03–1.07 it ratchets by only 3–7 % per period, which is too slow for 3 periods.
  - If the f = 1.25 run is green, T6 is blind: lengthen to 62 s (6 periods), never loosen the bound.

**T7 `w_is_identical_across_hints`** (`cc_sim`)
- T4's run under Bulk, Auto and Realtime, through `RWM_DELTA` or the hint, as the sim allows; if the sim has no hint input, this is a static check that `BbrRsConfig` holds no δ.
- `window()` traces are byte-identical.
- Green on both trees. This is the NO-MODE-SWITCH witness that R reads no dial.

**Stay green, unchanged:**
- §48 tests 1 and 2: the 3 s c1 stale runs never expire the filter.
- Test 3, the RTT rise: m̂ ≥ 36 ms by 12.5 s. R now ends ≈ 2–3 × 40 ms after the drain instead of 200 ms, so the bound holds with more slack.
- Test 4, quinn's collapse (pinned).
- Test 8, `probe_rtt_fires_only_on_an_expired_min_filter`: one entry. Exit is 2–3 events later; entry is unchanged.
- The tie test (`≤` keeps the filter fresh).
- The existing jitter assertions (pre and post b̂ ≥ 0.9 L).
- The gauge loopback test, extended to require numeric `qrn=`, `qrus=`, `qrb=`, `qab=` and `qage_us=` on `[QCC]` under `bbr_rs`.

## 5. Open

1. **The u model and the decay.**
   - Under the sim's in-order release, the one-round min shortfall u is unmeasured. T4 and T6 print it. u moves the fixed point R_floor·(1 + u), not stability (section 1b).
   - The prediction for c2 `qrt_us` (7.8–9.2 ms) rests on the triangular-tail stand-in.
   - A high read decays at ×f·b̂/L ≈ 0.8 per period. §48d's `RATCHET` flag reads the in-vivo result.
2. **To verify:**
   - v1's `<` against `bbr_rs`'s `≤`;
   - v1 or v3 excluding ProbeRtt samples from the bw max filter;
   - v3's `probe_rtt_cwnd` computed before or after the min_rtt reset at expiry. The quoted line `if (BBR.min_rtt_expired) BBR.probe_rtt_cwnd = BDPMultiple(...)` suggests before, i.e. on the pre-expiry model, as chosen here; ordering not checked;
   - the 200 ms multi-flow-overlap rationale.
3. **Shared bottlenecks.** A one-round probe under cross traffic reads high. No cell has cross traffic, so this is untested.
4. **The c8 per-leg split** of the 0.78 %. §48d's `qrus`/`qrb`/`qab` counters measure it.
