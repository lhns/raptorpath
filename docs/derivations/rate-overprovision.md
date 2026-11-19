# rate-overprovision: is the proactive rate r over-provisioned off Bulk, and by which term?

Status: DIAGNOSIS and derivation direction. No engine change, no VM, no cargo. The status, paper
and ledger texts are proposals (§8), not edits. Code read at `github/main` `3f07a4cd`, the base of
this branch; the scripts and their outputs are committed beside this note. Evidence: the
152 rows of the off-Bulk sweep (status "§51 block 51 Result" and Addenda 1–2), read from the full
per-row logs (`s51b-ledger/x/b51c2|b51c8/run/rep51/diag-ld`, not committed) with the scripts in
`rate-overprovision/` (outputs committed beside them as `*.out`).

## 0. Answer in plain words

> **Round 2 (§9) supersedes this section and §5–§6 where they differ.** The tail base is a declared
> dial (ADR-0050), not an underived default. The third D_arq/d reading is replaced by the in-session
> close times. The fractile is a candidate, **not adopted**.


- **Against its own target, r is not over-provisioned; it is short.** At Auto, the law's own target
  t_tail/ε = 4·10⁻⁴ per window needs r ≈ 0.32 at c2 and 0.36 at c8 (two-leg window) on the
  honest channel. The law gives 0.22 and 0.27.
- **Against what the δ dial needs at ρ = 1, it is over-provisioned at Auto on all three of the
  paper's readings of the late price. How much depends on the reading.**
  - At ρ = 1 delivery needs no proactive repair at all (§3.5: P_arq = 1), so r buys only lateness.
  - The NACK path still resends every source loss about once on every arm. So each coded symbol is
    pure extra bandwidth that buys an earlier close for about 0.14 holes.
  - Priced as paper §4.9 prices a late symbol, with D_arq/d = 1.03 or 2.2, the critical fractile
    at Auto is ≥ 0.9, and r = 0 is the optimum there.
  - With §4.9's third reading, D_arq = the measured repair delivery delay (D_arq/d ≈ 94 at c2, 24
    at c8), Auto is interior at r ≈ 0.155 (c2) and 0.105 (c8). The shipped r is 1.3× and 2.3× that,
    but short at Realtime.
  - The break-even for any r > 0 at Auto is D_arq/d ≈ 6. §4.9's own words apply: "one gauge field
    decides it" (d is echoed nowhere).
- **The term that makes it so is the z·S margin** of the core term, z = Φ⁻¹(1 − t_tail/ε̂), with
  t_tail = 10⁻⁵·ζ(δ). Its base is the declared `target_tail_loss` dial (ADR-0050; §9 item 0), but no derivation ties the dial to a price for a hole falling to ARQ. The margin is about 85–90 % of r;
  the information floor ε̂/(1−ε̂) is about 0.02. Nothing in the law credits the NACK channel.
- **The fix** replaces the per-window allowance t_tail/ε̂ with a critical fractile
  m = min(1, 1/c_hole(δ, ρ)), where c_hole is the price of one hole falling to ARQ. It is continuous
  in δ and ρ, has no branch, and keeps Bulk at exactly 0. Predicted goodput at Auto:
  - c2: 90.0 Mbit/s on readings 1–2, or 78.2 on reading 3 (76.1 today).
  - c8: 97.6 or 88.6 (82.9 today).

## 1. The shipped law, with provenance

r(δ) = (1 − β(δ))·r_anchor + β(δ)·r_late,  β(δ) = clamp((log δ_A − log δ)/(log δ_A − log δ_B), 0, 1)
(`fec_rate.rs` `RateSnapshot::rate`, paper §4.5). On the tunnel χ = 0, so r_late ≡ 0, and **off Bulk
r = (1 − β)·r_anchor**:

```text
r_anchor = clamp( soft_sat( max(r_core, r_burst, r_mass), r_sat ), 0, r_max )
r_core   = max(0, ε̂/(1−ε̂) + z·S) + ε_codec·(1 − (1−ε̂)^W)·1{core>0},   S = √(ε̂·σ²/(W(1−ε̂)))
z        = Φ⁻¹(1 − t_tail/ε̂),     t_tail(δ) = clamp(10⁻⁵·δ_A/δ, 10⁻⁹, 0.1)
r_burst  = (B̂/T_rtt)·(1 − t_tail/ε̂)⁺,   T_rtt = srtt·tput/1200
r_sat    = argmin_{r∈[0.01,1], step .005} (1−P_fec(r))·1.5·srtt + B̂·t_sym(1+r)/(r(1−ε̂)) + 0.5·(1+r)·W·t_sym
```

| symbol | read from | provenance | honest at s51b? |
|---|---|---|---|
| ε̂ | `predictive_loss_upper(0.95)` (`plu=`) of the worst path | cited (BOCD) as an upper quantile | **no**: below the wire truth on 54–65 % of source-phase ticks and above 2× truth on 15–25 %; plu/truth p10–p90 = 0.12–2.9 (c2), 0.15–4.2 (c8). An upper 95 % quantile should sit above truth on ~95 % of ticks. |
| worst path | argmax of `loss_rate()` (`pl=`, the per-ack EWMA, fed per WindowAck counter delta into `record_batch(d_expected, d_received)`, `control_msg.rs:626–636`) | declared (§4.4) | c8 source-phase ticks: strictly p0 10–12 %, strictly p1 30–34 %, **printed tie (both < 5·10⁻⁵) 54–59 %**. The logs cannot resolve ties (S06 doc §2.1), so §2 brackets both picks. Single path at c2, where `pl` prints 0 on 82–86 % of ticks. |
| σ², B̂ | GE estimator, per-batch lumped feed | cited (GE), lumping undeclared (E08) | not printed. The netem truth gives an honest dispersion g·σ²_skb = 13.0 (c2) / 12.0 (c8 p1); the per-datagram GE fit of that channel gives 16.5 / 14.2. |
| W | `encoder.window_size()` | derived, clipped at 200 (R07) | 201 on every off-Bulk tick |
| t_tail base 10⁻⁵ | `target_tail_loss`: `--target-tail-loss` (`main.rs:59`, `:362`), `config.rs:337` (default 10⁻⁵), Home 10⁻⁴ / Datacenter 10⁻⁶ (`config.rs:130–150`) | **a declared dial** (ADR-0050, Accepted) | it sets z. The perf path does not read it: 10⁻⁵ on all 152 rows (§9 item 1) |
| ζ(δ) = δ_A/δ | the dial | declared (§4.1) | yes |
| srtt | `estimator.rtt()` (EWMA, dwell-inclusive) | E01 | **no**: 206–251 ms at c2 and 264–600 ms on c8's p1, against 10 / 40 ms of netem delay |
| 1.5, C_DILUTION 0.5, grid, SAT_SOFTNESS 0.1 | `r_saturation` / `soft_saturate` | none (R04) | — |
| r_max = 0.5 | `config.rs:338` | declared | — |
| r_mass | `r_star_mass(mass, W, t_tail/ε̂, ε̂/ε_mass)` | derived (§4.3) | mass stats not printed |

**Wiring check.** `TaperBudget::accrue` is the identity, **by construction**: owed starts at 0, is
raised by `rate`, and then pays out min(owed, max(desire, rate), spare, 1). Since
max(desire, rate) ≥ rate = owed, the grant is min(rate, spare) and owed returns to 0. The re-timing
its sentence promises can act only on a tick where spare binds. It is confirmed measured:
`owed=0.00` on 16 843 of 16 843 `[SPAN]` lines in both cells. So `rr` is the law r after the spare
clamp. Mean `rr` tracks the consumed r per arm (c2 Auto 0.22 / 0.195, c8 Auto 0.26 /
0.24, `inputs-*.out`). Spare is slack off Bulk: c2 infl ≈ 100 against cwnd 10 000.

**Which loss estimate the law reads.**
- **The sample is the per-path WindowAck counter delta.** Each ack's (d_expected, d_received)
  goes to `record_batch` (`control_msg.rs:626–636`). That call updates the per-ack EWMA
  `loss_rate()` (α = 0.1), the Beta posterior, the BOCD and the lumped GE feed.
- **The rate law never reads `loss_rate()` as ε̂.** It reads it only to choose the worst path. Its
  ε̂ is plu, the BOCD from the same feed.
- **Two other readers of `loss_rate()`**, outside this law:
  - the shed budget `residual_loss_after_fec`, inert at ρ = 1;
  - S06's `eps_at_send`.
- **The cumulative `plc` matches the wire truth** (0.0242–0.0246 at c2 against 0.0245), so the
  losses do reach the feed.
- **Why `pl` prints 0 and plu misreads (inference, not counted):** the losses arrive in lumps, a
  few ack deltas each carrying a burst, so a 10-ack memory spends most ticks below print
  resolution and the BOCD alternates between "no loss" and a changepoint.

**The formula against its sentence.** The sentence is "provision so that a symbol is late (falls to
ARQ) with probability ≤ t_tail" (§4.2). The shape agrees (r rises in 1/δ and in ε̂, is continuous,
and reaches 0 as t_tail → ε̂). But the sentence prices lateness at a fixed 10⁻⁵ per symbol at Auto,
independent of what a late symbol costs (D_arq = 1.5 srtt) and of the bandwidth one repair costs.
It does not mention that a late symbol is still delivered at ρ = 1, or that ARQ resends it anyway.
It is a reliability target written for a channel without ARQ.

## 2. Evaluated at the measured inputs: which term wins

Port of `controller_rate` and the mix (`law.py`; mass term excluded because it is not printed),
evaluated tick by tick at each `[SPAN]` tick's nearest `[DIAG]` worst-path plu and srtt
(`tick.py`), time mean over the source phase:

| cell, arms | mean rr | law at honest σ² | law at GE-fit σ² | σ² that reproduces rr |
|---|---|---|---|---|
| c2 Auto (AM/HA/NA/AP) | 0.223 | 0.165 (13) | 0.180 (16.5) | ≈ 30 |
| c2 R± | 0.247 | 0.196 | 0.214 | ≈ 25 |
| c2 I3 | 0.100 | 0.069 | 0.075 | ≈ 33 |
| c8 Auto | 0.271 | 0.239 (12) | 0.251 (14.2) | ≈ 18 |
| c8 R± | 0.287 | 0.263 | 0.277 | ≈ 16 |
| c8 I3 | 0.128 | 0.111 | 0.116 | ≈ 19 |

Per-arm terms at the median inputs (`law.out`):

- **The core term wins at every off-Bulk point in both cells.** The burst term is 0.003–0.018,
  far below it. The IT floor ε̂/(1−ε̂) is 0.015–0.04. **The margin z·S is 0.13–0.25, i.e.
  85–90 % of r_anchor.**
- **c8 is reproduced by the core term at about the per-datagram GE σ².**
  - At σ² = 14.2 the law's Auto mean is 0.252 with printed ties sent to p1 (the code's tie rule),
    and 0.237 with ties sent to p0. Measured is 0.271, so the law is 7–13 % low.
  - At R± it is 0.278 / 0.264 against 0.287.
- **c2 is not.** It needs σ² ≈ 30, about 2.3× the honest dispersion. Either the lumped batch feed
  inflates the GE fit (E08, consistent with "overestimates burstiness" in `record_batch`'s own
  comment), or the mass term wins by about 0.04 at Auto. **Which one is not decidable without the
  term-win gauge (R02).**
- **max_overhead (0.5) binds on 6–8 % of `[SPAN]` ticks at c2 Auto and above, and 12–16 % at c8**
  (`rr_at_cap`, measured). These are plu spikes: plu reads 0.70 at c2 HA s42 r1 t = 13 s, and
  cold start sits at t = 0. Below the 0.5 PINNED threshold.
- **r_sat: no gauge.** Inferred soft-cap pressure is ≤ 0.02 at c2 through I4, 0.04–0.06 at c2 R±,
  0.10–0.19 at c8 Auto and 0.45–0.62 at c8 R±. The c8 "above the predicted r_sat 0.245–0.250"
  (§51 Result) is in r_sat's inputs, not a breach. The live srtt of c8's p1 (564–600 ms against
  the design's 250 ms) raises the 1.5·srtt FEC-miss cost, so live r_sat is about 0.32. `rr` reads
  0.5 on 12–16 % of ticks, so live r_sat reached ≥ 0.5 on those ticks.
- **A weak loop exists through r_sat.** More r means more wire load, more dwell, a larger srtt and
  so a looser cap: srtt rises 206 → 245 ms at c2 and 236 → 576 ms on c8's p1 from Bulk to Auto.
  The core term does not read srtt, so the loop acts only where the cap has pressure (c8 R±).

## 3. What r the δ dial actually needs

### 3.1 Three baselines

1. **Delivery at ρ = 1: r = 0.** P_arq = 1 (§3.5). Every loss is delivered by ARQ.
2. **The law's own lateness target, on the honest channel: r is short.** `need.py` computes the
   exact window loss-mass tail of the netem process: GE per skb (p, q from the qdisc line), each
   lost skb taking its g datagrams (g = the measured GSO mean). It applies §4.3's exact criterion
   K_N > R over N = W + R.
   - At Auto (m = t_tail/ε = 4·10⁻⁴) it needs r = 0.32 at c2, and 0.36 on c8's two-leg window
     (0.39 on p1 alone).
   - The law gives 0.22 and 0.27. This is the §4.2 "known limitation" (the closed form
   under-provisions on bursty channels), here measured at 0.65–0.70× of the requirement.
3. **A target that prices the late hole and credits ARQ.** See §3.3.

### 3.2 What the coded channel buys at ρ = 1 (the NACK channel is never counted)

Counter: `[RFA] fill_coded` (`net/report.rs:51–69`, fed at `receiver.rs:1474`): a seq the decoder
rebuilt from coded repair, not one received as source. Sender coded is `cum=` cod; resends are
`[DIAG] retx=`; losses are the driver's `[TRUTH] lost=` (all wire datagrams). These are per-arm
medians (`recov-c2.out`, `recov-c8.out`). The S06 agent's `fill.py` reports the same counter, and
the re-check of c2 HA s42 r1 gives fill_coded 6 335 on cod 48 592 (0.13 per coded).

| c2 arm | coded/src | resends/src | closed by decode/src | per coded symbol | useful-rank share of repairs |
|---|---|---|---|---|---|
| H-B | 0.0038 | 0.0273 | 0.0013 | 0.34 | 0.36 |
| I3 | 0.029 | 0.0275 | 0.0068 | 0.23 | 0.24 |
| Auto (4 arms) | 0.072–0.082 | 0.026 | 0.0107–0.0117 | 0.14 | 0.15 |
| R± | 0.087–0.102 | 0.026 | 0.011–0.013 | 0.13 | 0.14 |
| c8 Auto | 0.004 | 0.027 | 0.0017 | 0.42 | 0.45 |

- **Resends do not fall.** They stay at 0.026–0.0275 per source symbol on every arm.
  - Per *source* loss (ε ≈ 0.025 of source symbols, the netem rate) that is 1.04–1.10.
  - Per `[TRUTH] lost=` it is 0.83 at Auto. That counter also counts lost copies and lost coded
    datagrams, which rise with r (0.026 → 0.032 per source). So a decode close is an earlier close of a hole that is resent anyway, never a saved
  resend.
- **The earlier close is real per hole.** On c2 HA s42 r1 (`[SUCC]`), decode-resolved holes close
  at p50 9 ms, against 229 ms for resend-resolved ones.
- **The marginal yield is 0.14 holes per extra coded symbol from H-B to Auto, and about 0.08 from
  Auto to R±.**
- **The law has no NACK term.** `BudgetAllocator` (`fec_rate.rs:794–845`, ADR-0050) sizes only the
  NACK cap (`net/mod.rs:3308`). Its `proactive_rate()` has test callers only. So the module
  header's "coordinated to avoid double-spending" has no formula behind it on the proactive leg,
  and the measurement shows the double spend.

### 3.3 The price form (paper §4.9 and §4.12, made per hole)

- **Premise A1: the repair is a fungible degree of freedom.** The sliding RLC decoder banks rank,
  and ARQ resends keep filling holes, so a repair beyond the window's deficit stays usable.
  Then the (R+1)-th repair closes one hole earlier iff the window's loss mass exceeds R. Its
  expected saving is P(K_N > R)·c_hole, and its cost is 1 symbol.
- **The newsvendor condition** gives the least R with

```text
P(K_N > R) ≤ m(δ, ρ),     m = min(1, 1/c_hole(δ, ρ))
c_hole(δ, ρ) = P_arq(ρ)·δ·D_arq/d  +  (1 − P_arq(ρ))·c_loss
```

  with c_hole in symbol-equivalents, Copa's currency (§4.9: a delay enters as δ·Δd/d).
- **No branch is needed.** The same statistic and solver exist today: `r_star_mass` takes exactly
  this allowance as `delta_wf`, and its normal form is `compute_r_star_with_z(ε̂, σ², W, Φ⁻¹(1−m))`.
  So the candidate replaces t_tail/ε̂ with m(δ, ρ) in both. It must also say what the declared
  `target_tail_loss` dial then does (§9 item 0).
- **The A1 check is measured, at the coded count.**
  - Under A1 the marginal yield above *is* P(K > R) at the coded count actually sent, about
    0.075–0.10 per source (copies take the rest of the slots).
  - At that count it demands c_hole ≥ 1/0.14 ≈ 7 at Auto and ≥ 12.5 at R±.
- **At the slot count the bar is far higher.**
  - The law's output is the slot count, 0.20–0.26.
  - Once S06 turns the copies into coded symbols, the marginal slot sits at P(K_N > R) for
    r ≈ 0.22. That is about 4·10⁻³ at c2 (`need.out`) and about 2.5·10⁻³ on c8's two-leg window.
  - **So the shipped slot count is optimal only if c_hole ≳ 250 (c2) or 400 (c8).**
  - The §4.9 readings give c_hole(Auto) = 0.5–1.1 (readings 1–2), or 47 at c2 and 12 at c8
    (reading 3).

| symbol | provenance |
|---|---|
| D_arq | measured: one resend round, e.g. `[SUCC] orig_p50`, or 1.5·srtt on an honest srtt (E01) |
| d | Copa's delay normaliser, already the dial's currency (§4.9); not echoed by any gauge today. §4.9 gives three readings of D_arq/d: 1.03, 2.2, and "at measured". Its δ_exit column implies ≈ 94 (c2) / 24 (c8) for the last, as 1.03 × δ_exit(1.03)/δ_exit(measured) |
| δ | the declared dial |
| P_arq(ρ) | §3.5 (1 at ρ = 1) |
| c_loss | declared at ρ < 1 (owner question 1) |
| 1 | one repair symbol: the bandwidth unit of r |

## 4. S06 interaction

- **The slot count is this law's alone, and the S06 fix does not change it.** `s06-plost-resend.md`
  on `github/s06-derivation` keeps the slot count and moves copies to coded repair (its lines 20
  and 703).
- **It leaves this law's inputs unchanged to first order.**
  - The slot count, and so the wire load, are the same.
  - ε̂ (plu) comes from the ack counter feed, which copies do not bias in rate: netem drops per skb
    whatever the payload.
  - srtt follows the wire load, which does not change.
- **It changes what a slot buys.** Copies (0.12–0.25 per source) become coded symbols, whose
  measured yield at the trailing span is 0.13–0.14 holes per symbol at c2, and which the S06 doc
  says the span can mostly not use.
- **So the S06 fix cannot return goodput** (Addendum 2's arithmetic holds). Only a lower r does.
- **On the low ε̂:** plu reads below truth on most ticks, but the law r is still high because z·S
  dominates. A more honest (higher) ε̂ would raise r under today's target. Under the fix, ε̂ enters
  only through K_N's distribution.
- **There is no feedback from copies into ε̂.**

## 5. Verdicts (the ledger's words)

| row | verdict now | proposed |
|---|---|---|
| R01 (mix) | DEGENERATE(L1: Bulk, χ = 0) | **OK (shape), measured**: r > 0 off Bulk, continuous across all three presets (T1 PASS ×6); carries R02's defects. DEGENERATE withdrawn for the off-Bulk cells. |
| R02 (anchor body) | UNDERIVED | **UNDERIVED(no derivation ties the declared tail dial to the delivered quantity; why max), DIVERGENT(ADR-0050 Phase 3 credits the NACK channel, `proactive_budget = total − nack_expected`; the code never wires it into r), DISHONEST-INPUT(ε̂ per E07, σ² per E08, srtt per E01 via r_burst and r_sat)**. Core wins at every off-Bulk point (inferred; no gauge). The z·S margin is 85–90 % of r. Over-provisioned against the δ dial at ρ = 1, short of its own target. max_overhead binds on 6–16 % of ticks. |
| R03 | unchanged | Measured on the P_fec > 0 side from I2 up in both cells. Read only by r_sat at ρ = 1. |
| S10 (TaperBudget) | DIVERGENT, DEGENERATE(r = 0) | **DIVERGENT, DEGENERATE(identity: the grant is min(r, spare) by construction; the re-timing never acts unless spare binds)**. r = 0 is withdrawn off Bulk. Measured: owed = 0.00 on 16 843 / 16 843 `[SPAN]` lines. |
| R04 (r_sat) | UNDERIVED, DIVERGENT | **+ DISHONEST-INPUT(srtt = E01)**. Measured: slack at c2 (pressure ≤ 0.06, inferred), pressure 0.45–0.62 at c8 R± (inferred). Live r_sat ≈ 0.32 at c8, not the design's 0.245; no gauge, so not PINNED. |
| R05 | UNDERIVED, DEGENERATE(tunnel) | unchanged (χ = 0 on every row) |
| R06 (mass) | OK (unverified) | unchanged; **cannot be excluded as the c2 winner** (a 0.04 gap at Auto). Gauge needed before any verdict. |
| E07 (plu) | DIVERGENT | **+ DISHONEST-INPUT**: an "upper 95 % quantile" below the wire truth on 54–65 % of source-phase ticks and above 2× on 15–25 % (`plu` vs `[TRUTH]`). The cumulative `plc` matches truth, so the losses reach the feed. Inferred cause: the feed arrives in lumps (one ack delta carries a burst), and `pl` sits below print resolution on 82–86 % of c2 ticks. |
| E08 (GE) | DIVERGENT | Evidence for the unbounded lumping: c2 needs σ² ≈ 30 against the honest 13, if core wins. |

No MODE-SWITCH: no hint or δ/ρ threshold selects a law here.

## 6. Fix direction: the fractile candidate (NOT ADOPTED; superseded in part by §9)

```text
r_anchor(δ, ρ) = clamp( soft_sat( max(r_core(m), r_burst(m), r_mass(m)), r_sat ), 0, r_max )
m(δ, ρ)        = min(1, 1/c_hole(δ, ρ)),    c_hole = P_arq(ρ)·δ·D_arq/d + (1 − P_arq(ρ))·c_loss
r_core(m)      = max(0, ε̂/(1−ε̂) + Φ⁻¹(1−m)·S) [+codec],  r_burst ∝ (1 − m)⁺,  r_mass = r_star_mass(.., m, ..)
```

- **N = 2.** K_N is the window's total loss mass over the paths that carry it, by share:
  `K_N = Σ_i K_i(g_i·N)`, and c_hole is the share- and loss-weighted mean of the per-path c_hole,i
  (D_arq,i). This replaces "the worst path's ε̂ applied to the whole window", which over-provisions
  at N = 2, and its argmax-EWMA pick. At N = 1 it is exactly §4.2/§4.3.
- **Shape checks.**
  - **Continuous in δ.** c_hole is linear in δ. The min(1, ·) clamp is a kink, not a step, at
    δ_k = d/D_arq: 0.45–0.97 on readings 1–2, and 0.011 (c2) / 0.04 (c8) on reading 3. It needs a
    bind-fraction gauge (CLAUDE.md FORMULA-FIRST), and the ±2 % nudge gates must include δ_k.
  - **The proposed law is itself clamp-bound over part of the dial.**
    - On readings 1–2, m = 1 from Bulk through Auto. The default hint is then pure ARQ, and the
      clamp's bind fraction is 1 there.
    - That is the pinned-over-range pattern CLAUDE.md flags. Here it is the law's answer, not a
      hidden constant. But it is a consequence the owner chooses with question 1, and the gauge
      must report it.
  - **Continuous in ρ** through P_arq(ρ). There is no `reliable` branch.
  - **Bulk.** β = 1 and m = 1 both give exactly 0, so the H-B bytes are unchanged.
  - **The presets move at Auto and Realtime** (a deliberate model change).
  - **ε → 0:** P(K_N > 0) → 0 < m, so r → 0. It is exactly 0 at ε = 0.
  - **N = 1** gives §4.2. **N = 2 with identical legs** equals N = 1 at the combined rate.
  - **Units:** m is a probability, and c_hole is in symbol-equivalents by construction.
- **Predictions.**
  - **The tail.** Exact window-mass tail at honest inputs (`need.py`). c8 uses the two-leg window
    convolved at the measured 0.82 / 0.18 egress split (the N = 2 formula), not the worst leg
    alone.
  - **The price.** Three readings of D_arq/d (§4.9): (1) 1.03, (2) 2.2, (3) measured, 94 at c2 and
    24 at c8.
  - **Goodput.** From G = G_Bulk·(1 + x_B)/(1 + x_B + r), with x_B = resends + margin at H-B (c2
    0.031, c8 0.035).
    - It reproduces today's c2 rows to within 1 %: Auto 75.8 against 76.1 measured, R− 74.2 against
      74.7, I3 83.5 against 84.0.
    - At c8 it reads 5 % low (Auto 79.1 against 82.9), because c8 is not purely wire-bound. Treat
      the c8 goodput numbers as ±5 %.
  - **`need.py` self-check.** At the paper's WiFi channel (per symbol, W = 50, m = 10⁻²) it gives
    0.180, against §4.7's exact 0.170. That is within one repair of its grid step (0.02 at W = 50).
    The {⌊g⌋, ⌈g⌉} skb-size mix is a model of GSO, not a measurement.

| point | c2 r, readings 1 / 2 / 3 (today) | c2 goodput 1 / 2 / 3 (today) | c8 r, 1 / 2 / 3 (today) | c8 goodput 1 / 2 / 3 (today) |
|---|---|---|---|---|
| I1, I2 | 0 / 0 / 0 (0.008, 0.032) | 90.0 (90.0, 87.7) | 0 / 0 / 0 (0.017, 0.054) | 97.6 (98.7, 95.6) |
| I3 (β ½) | 0 / 0 / 0.022 (0.080) | 90.0 / 90.0 / 88.1 (84.0) | 0 / 0 / 0 (0.115) | 97.6 (93.2) |
| Auto | 0 / 0 / 0.155 (0.195) | 90.0 / 90.0 / 78.2 (76.1) | 0 / 0 / 0.105 (0.242) | 97.6 / 97.6 / 88.6 (82.9) |
| I4 (δ 5) | 0.05 / 0.09 / 0.255 (0.214) | 85.8 / 82.8 / 72.2 (75.1) | 0.06 / 0.10 / 0.215 (0.249) | 92.3 / 89.0 / 80.8 (81.4) |
| R± (δ 50) | 0.155 / 0.19 / 0.35 (0.221) | 78.2 / 76.0 / 67.2 (74.6) | 0.18 / 0.215 / 0.33 (0.26) | 83.1 / 80.8 / 74.0 (81.2) |

**Over-provision factor** (consumed r ÷ fixed r, readings 1 / 2 / 3):

| cell | Auto | I4 | R± |
|---|---|---|---|
| c2 | ∞ / ∞ / 1.26 | 4.3 / 2.4 / 0.84 | 1.43 / 1.16 / 0.63 |
| c8 | ∞ / ∞ / 2.3 | 4.2 / 2.5 / 1.16 | 1.44 / 1.21 / 0.79 |

- **Against the law's own target** the factor is 0.69 at c2 and 0.67–0.75 at c8 (worst leg / two-leg
  window: need 0.39 / 0.36 against 0.27). That is short.
- **Against the dial, r is over-provisioned at Auto on every reading.** The price question decides
  by how much, and whether Realtime is short.

**This sits on two premises an owner must accept:**
- **A1** (fungible repair). It is checked by the marginal yield.
- **The per-symbol late price of §4.9.** A head-of-line-weighted price, where a stalled hole delays
  every symbol behind it, would raise c_hole by roughly rate·D_arq/2 and put Auto in the interior
  again (owner question 1).

## 7. Tests (red first), coupling, owner questions

**Tests.**
1. **`rate_fractile_is_the_hole_price` (math, absolute).**
   - Fixture: ε = 0.025, σ² = 13, W = 200, S = 0.04082.
   - m = 1 gives r = 0. m = 0.5 gives the IT floor 0.0256 (+codec). m = 0.0194 gives 0.1100 (+codec).
   - At the Auto fixture with δ·D_arq/d ≤ 1, r is exactly 0.
   - Red today: today's law reads t_tail, and gives 0.1625 at the same fixture (m = 4·10⁻⁴).
2. **`rate_fractile_continuity`.**
   - Nudge ±2 % through Bulk, Auto, Realtime and δ_k: assert no step above the f64 slope bound.
   - Sweep ρ ∈ [0.9, 1] the same way.
   - N = 1 and N = 2 absolutes (N = 2 with identical legs equals N = 1).
   - ε = 0 gives exactly 0.
3. **The term-win and bind gauge (R02).**
   - `[DIAG] rlaw=` prints r_anchor, r_late, β, m, the winning term, the soft-cap pressure, and the
     max_overhead and spare bind flags.
   - An "executes" witness asserts that the gauge counts ticks on an off-Bulk host loopback run.
   - This is the prerequisite for any PINNED or DEGENERATE call on R04 or R06.
4. **Bound tests for the divergences this note measures.**
   - Closed form against the exact mass tail at the c2 fixture: 0.16 against 0.32 at m = 4·10⁻⁴.
     This bounds R02 under its own target.
   - plu against a synthetic lumpy feed (E07): the share of ticks with plu < true ε, red today.
5. **`decode_close_does_not_cancel_the_resend` (host loopback).** It counts resends of seqs already
   decode-filled. It pins the "no bandwidth credit" premise behind c_hole.

**wasm coupling.**
- `raptorpath-wasm` evaluates the same `math::controller_rate` and `z_for_tail_target`
  (`lib.rs:33–38`, `:111–125`), so the change lands in `raptorpath-math` (scope class (a), with its
  paper derivation first).
- It is a deliberate model change. The golden fingerprints need a `GOLDEN_CAPTURE=1` re-capture.
- `test_continuum_one_law_across_the_dial` and the two mjs gates must add δ_k to the nudge points.
- The model must hold every c_hole input, D_arq and d, before the law is shared, with an "executes"
  witness. Today d is echoed nowhere.
- M03's wasm β-guard (`if bulkness > 0`) stays a separate debt.

**Owner questions (at most three).**
1. **Which late-hole price, and which d?**
   - **(a) §4.9 per symbol, δ·D_arq/d.**
     - With d = Copa's operating delay (D_arq/d 1–2), Auto is a corner: r = 0.
     - With D_arq = the measured repair delay (D_arq/d 24–94), Auto is interior: r ≈ 0.1–0.16.
   - **(b) Head-of-line weighted.**
   - The break-even for r > 0 at Auto is D_arq/d ≈ 6.
   - Also: what is c_loss at ρ < 1?
2. **Is t_tail = 10⁻⁵·ζ(δ) a user-facing contract to keep?** If yes, r is short at Auto, not
   over, and the exact tail needs 0.32 / 0.36. That is the opposite fix.
3. **Should a decode close cancel the pending NACK resend?** That would give c_hole one resend
   symbol of bandwidth credit. Today it never does.

## 8. Proposed text (not written to those files)

**Paper, §4.4 (new paragraph "What the margin buys at ρ = 1").**
- At ρ = 1 a hole the margin misses is delivered one ARQ round late, and the resend is sent whatever
  the decoder did. Measured at s51b c2, resends stay at 1.04–1.10 per source loss from H-B to R±, while
  coded closes 0.14 holes per symbol at Auto.
- The per-window allowance should therefore be the critical fractile m = min(1, 1/c_hole), with
  c_hole = P_arq(ρ)·δ·D_arq/d + (1−P_arq(ρ))·c_loss, not t_tail/ε̂. The base 10⁻⁵ then has no role.
- Then the provenance table of §3.3 above.

**Ledger.** The §5 table rows, as written there.

**status.md (§51 Addendum 3, proposed).**
- "The off-Bulk r is not over-provisioned against its own target (exact need 0.32 / 0.36 against
  0.22 / 0.27 at Auto). It is over-provisioned against the δ dial at ρ = 1: the z·S margin on an
  unpriced base 10⁻⁵ is 85–90 % of r, and the law credits no NACK capacity. Resends stay at about
  one per loss on every arm, and coded closes 0.14 holes per symbol."
- "The S06 fix does not change the slot count or this law's inputs."
- "Fix direction: the hole-price fractile (`docs/derivations/rate-overprovision.md` §6), pending
  owner questions 1–3."
- "Gauges missing: term-win, r_sat bind, mass stats."

## 9. Round 2 (coordinator review): the tail dial, units, profiles, close times, registration

### 9.0 Correction: `target_tail_loss` is a declared dial

- **Where it is declared:**
  - the `--target-tail-loss` flag (`main.rs:59`, `:362`);
  - `config.rs:337` (default 10⁻⁵);
  - two profiles (`config.rs:130–150`): Home is 10⁻⁴ with max_fec_overhead 0.3 and hint auto;
    Datacenter is 10⁻⁶ with 0.5 and bulk;
  - ADR-0050 (Accepted), "Protocol Hint → Tail Reliability".
- **What this changes in §0, §1 and §5.** "Base with no provenance" is withdrawn. The defect is that
  no derivation ties the dial to what the engine delivers (§9.2), plus a code-against-ADR divergence:
  ADR-0050 Phase 3 decides `proactive_budget = total_budget − nack_expected`, and the code never
  feeds that into r (`BudgetAllocator::proactive_rate` has test callers only).
- **The fractile (§6) is a candidate, NOT ADOPTED.** If it were adopted, the flag needs a meaning.
  Two options:
  - **(A) The flag stays a lateness contract:** r = max(r_price(m), r_flag(t/ε̂)), one continuous
    max with no branch. The default 10⁻⁵ then keeps today's r, so the over-provision against the
    dial persists unless the default becomes "no contract" (t = ε̂, r_flag ≡ 0).
  - **(B) The flag is superseded into the price unit:** for example `--target-tail-loss` maps to
    c_hole = 1/m. That needs a new ADR superseding ADR-0050's hint → tail mapping.
  - §9.6's registration decides between them.

### 9.1 Does the flag reach the sweep's path? No. All 152 rows ran at 10⁻⁵.

- **The perf path ignores the flag.** `cmd_perf` (`main.rs:268–300`) builds its `RaptorpathConfig`
  from `PerfArgs` with `..Default::default()`. `PerfArgs` has no `target_tail_loss` (or
  `max_fec_overhead`, or profile), and `cmd_perf` reads no config file, so `config::resolve`
  applies `unwrap_or(1e-5)` (`config.rs:337`).
- **Confirmed in the logs.** The `perf configuration` line reads `target_tail_loss: 1e-5,
  max_fec_overhead: 0.5` on 152 of 152 client and server logs in each cell.
- **So the flag-sweep arms need a new binary.** That means a `--target-tail-loss` (and
  `--max-fec-overhead`) pass-through on `perf`, and the harness passing it on both ends. This is a
  tools/CLI change, not engine behaviour, but it changes the binary sha.

### 9.2 The unit, and the delivered residual in that unit

- **What the target bounds.** Paper §4.2 and §3.5: target_tail_loss bounds the **per-source-symbol
  probability of being late**, P(late) = ε·(1 − P_fec) ≤ t_tail(δ). "Late" means delivered by ARQ
  rather than by in-window FEC; at ρ = 1 nothing is lost. The law enforces it through the
  **per-window** allowance t_tail/ε̂ = P(window deficit). It does not bound anything per object.
- **The delivered residual at c2 Auto, per source symbol.** Source losses ≈ 0.025 (netem rate; the
  resends, 0.026, agree). Decode closes are 0.0107–0.0117 (`recov-c2.out`). So
  **P(late) ≈ 0.013–0.014**. With `[TRUTH] lost=` (0.0315, which includes lost copies and coded
  datagrams) the upper end is 0.021, matching main's 0.014–0.021.
- **Against the target:** 10⁻⁵, so the miss is **1 300–2 100×**.
- **Per hole:** 54–58 % of holes fall to ARQ, against the law's assumed 1 − P_fec of about 4·10⁻⁴.
- **Per window** (W = 200): 2.7–4.2 late holes per window.
- **Per 600 MB object** (507 616 source symbols): about 6 600–10 700 late holes, against 5.
- **R± (target 10⁻⁷):** P(late) ≈ 0.012–0.014. The delivered lateness barely moves with the dial.

### 9.3 The profiles, and whether the target is meetable

- **The exact requirement.** `need.py` gives the exact requirement at honest inputs, with
  m = t_tail(δ)/ε (c2 ε 0.025; c8's two-leg window at the worst-leg ε 0.048).

| case | t_tail at the law | per-window m (c2 / c8) | exact r needed c2 / c8 | cap | law today | meetable? |
|---|---|---|---|---|---|---|
| Datacenter: 10⁻⁶, bulk | 10⁻⁴ (×ζ = 100) | — | — | 0.5 | **0 exactly** (β = 1; the anchor term is weighted 0) | **The target is inert in the rate law.** P(late) = ε by design (late is fine). The sweep's 0 reading is this, not a miss. |
| Home: 10⁻⁴, auto | 10⁻⁴ | 4·10⁻³ / 2.1·10⁻³ | 0.225 / 0.285 | 0.3 | ≈ 0.16 / 0.20 | Feasible under the cap. The law gives about 0.7× of it. |
| default 10⁻⁵, auto | 10⁻⁵ | 4·10⁻⁴ / 2.1·10⁻⁴ | 0.32 / 0.395 | 0.5 | 0.20–0.22 / 0.25–0.27 | Feasible under the cap. The law gives 0.65–0.7×. |
| default, Realtime (×0.01) | 10⁻⁷ | 4·10⁻⁶ / 2.1·10⁻⁶ | 0.505 / > 0.58 | 0.5 (r_sat ≈ 0.3–0.4 soft) | 0.25–0.29 | **No.** Infeasible in-window under max_overhead at both cells, and below r_sat's soft cap well before that. |

- **So the dial's target is met nowhere it is live.**
  - At Bulk it is inert by design.
  - At Auto the law under-provisions its own target (0.65–0.7×), and the realized decode yield then
    misses it by 10³.
  - At Realtime it is infeasible under the declared caps on these channels.
- **In the delivered unit the dial is close to flat:** P(late) is 0.013–0.014 at Auto and
  0.012–0.014 at R±.

### 9.4 Where the base enters, other than base·ζ(δ)

It enters nowhere else. All three readers read the one clamped product
`ctrl.target_tail_loss() = clamp(base·ζ(δ), 10⁻⁹, 0.1)` (`fec_rate.rs:331–333`):
1. **The rate law:** z = Φ⁻¹(1 − t/ε̂), the burst fraction (1 − t/ε̂)⁺, and the mass solve's
   δ_wf = t/ε̂ (`fec_rate.rs:504`). This is live.
2. **W\*:** `derive_window(t, …)` (`fec_rate.rs:546`, called at `sender_phases.rs:1316`). W_over
   reads z(t/ε̂). It is **inert on these rows**: the keep-behind clamp to 200 binds (`wsize=201` on
   every off-Bulk tick, R07).
3. **The NACK budget's confidence:** `predictive_loss_upper(1 − t)` (`net/mod.rs:3315`). It is
   **inert on these rows**: the per-pass budget sits at its floor `MAX_NACK_REPAIRS_PER_NACK`,
   `nbud=10`, on 29 287 of 34 785 `[DIAG]` ticks (84 %). This is the one place ADR-0050's "the BOCD
   quantile at the adjusted confidence level IS the entire control mechanism" survives, and it is
   floor-bound.

### 9.5 Items owed from the review

**(a) What d is, and the in-session D_arq/d.**
- **What d is.** d is Copa's delay normaliser: the operating delay that a delay increment Δd is
  priced against, in L = bandwidth fraction + δ·Δd/d (§4.9). The paper's three readings are d =
  Copa's operating delay (D_arq/d ≈ 1.03), a propagation-scale reference (2.2), and a third whose
  δ_exit column back-solves to D_arq/d ≈ 94 / 24. Those are the paper's cells and binary, not this
  session.
- **In this session.** The quantity a decode close saves is D_arq − t_fec, not D_arq. `[SUCC]` (6
  rows per arm, server logs):
  - **c2 Auto:** resend-resolved p50 229–246 ms, decode-resolved p50 9–13 ms, saving 220–235 ms.
  - **c2 H-B:** both are 197 ms; the margin's coded repair is not faster.
  - **c2 R±:** the same as Auto.
  - **c8 Auto and R±:** resend p50 0.25–0.46 s, **decode p50 0.65–6.3 s**. At c8 a decode close is
    *slower* than a resend (the "present or isolating" race of §3.7, and coded symbols there are
    only 0.004 per source).
- **So the in-session ratio (D_arq − t_fec)/d is:**
  - about **0.9–1.0** with d = the operating delay (srtt 245 ms, dwell-inclusive): reading 1;
  - about **22–24** with d = the propagation RTT (netem 2 × 5 ms): reading 3′, replacing 94;
  - **≤ 0 at c8** on any d.
- **The candidate's c_hole uses (D_arq − t_fec)⁺**, a continuous clamp, and that changes §6's table.

| point | c2 r, readings 1 / 2 / 3′ (today) | c2 goodput 1 / 2 / 3′ (today) | c8 r, any reading (today) | c8 goodput (today) |
|---|---|---|---|---|
| I1–I3 | 0 / 0 / 0 (0.008–0.080) | 90.0 (84.0–90.0) | 0 (0.017–0.115) | 97.6 (93.2–98.7) |
| Auto | 0 / 0 / 0.09 (0.195) | 90.0 / 90.0 / 82.8 (76.1) | 0 (0.242) | 97.6 (82.9) |
| I4 | 0.05 / 0.09 / 0.19 (0.214) | 85.8 / 82.8 / 76.0 (75.1) | 0 (0.249) | 97.6 (81.4) |
| R± | 0.155 / 0.19 / 0.29 (0.221) | 78.2 / 76.0 / 70.2 (74.6) | 0 (0.26) | 97.6 (81.2) |

- **c8 is conditional.** t_fec was measured with copies starving the coded channel (S06). After the
  no-copy fix it must be re-read before any c8 number stands. c8 goodput is ±5 % (§6).

**(b) Why the resend still fires after a decode close (code read).**
- **Nothing cancels it.**
  - The receiver sends a gap report as soon as a higher seq arrives past a hole, rate-limited to one
    per `GAP_ACK_MIN_INTERVAL` = 2 ms (`receiver.rs:1954–1957`, the dupack analog). The trailing-span
    repair covering the hole lands after that report, at a p50 of 9 ms.
  - The sender serves every seq of a judged gap list (`sender_phases.rs:789–803`).
  - At N = 1 its only gates are the per-seq cooldown (`:871–879`, which applies only after a first
    resend) and the age gate, original age ≥ srtt/2 (`recovery_laws.rs:266–268`).
    `RWM_RECOV_SP=0` on these rows, so there is no RFC 9002 time threshold at N = 1 (`gates.rs:371`).
  - There is no decoder or "since received" check between the report and the fire. The report
    predates the decode, so the resend fires about srtt/2 later anyway, stamped at `:1091`.
- **A decode close only suppresses later re-resends.** The receiver inserts the decoded seq into
  `received_seqs` (`receiver.rs:1482`), so later SACKs omit it.
- **Consistent with the counters:**
  - `preempt_src` (a source arrival for an already decoded seq) is about 0.0036 per source at c2
    Auto, about a third of decode closes. Some of those are copies, not resends.
  - Resends per source stay flat.

**(c) The loss feed, and why `pl=` prints 0 (replacing "lumps").**
- **An empty delta never reaches the estimator.**
  - Timer and hold-expiry acks carry `cum_expected: 0` (`receiver.rs:945`, `:1042`, `:1086`).
  - `ack_merge_counter_delta` returns (0, 0) for those and for an unchanged counter
    (`path.rs:370–378`).
  - `record_batch` is called only `if le > 0` (`control_msg.rs:635–637`).
  - So an empty delta does not decay the EWMA.
- **The counters ride every data ack.** Under `RWM_ACK_MERGE=1` (on, per `[GATES]`) the WindowAck
  "must go out once per data message" (`receiver.rs:1966–1975`). The (expected, received) totals
  ride every one (`receiver.rs:1304–1312`), and a window batch carries about one symbol. So the
  estimator gets about one sample per received datagram: about 9.3 k/s at c2.
- **What that does to `pl`.** A netem burst (two skbs, about 9 datagrams) is revealed in one delta,
  for example (10 expected, 1 received). That gives a batch loss of 0.9, so the EWMA jumps to
  0.1·0.9 = 0.09. It then falls below the print resolution 5·10⁻⁵ after ln(5.6·10⁻⁴)/ln 0.9 ≈
  71 samples, about 7.6 ms.
- **The arithmetic matches the reading.** Bursts arrive at 1 750 per 62 s, about 28/s, one per
  36 ms. So `pl` is nonzero about 7.6/36 ≈ 21 % of the time: **predicted 79 % printed zero,
  measured 82–86 % at c2.** A 10-sample memory at 9 k samples/s is a ≈ 1 ms memory on a channel
  whose losses are 36 ms apart.
- **plu is fed the same samples.** `record_counts_at` feeds the BOCD per delta. That is consistent
  with its low median and 10× spikes, but the BOCD trace is not printed, so that part is inference.
- **c8:** per-path samples at about 9 k/s (p0) and 2 k/s (p1). `pl` reads 0 on both legs on 55–59 %
  of ticks.

### 9.6 Registration input: a two-sided flag sweep at fixed δ (no-copy VM session)

- **Question:** does a tighter `target_tail_loss` buy less resend latency (ADR-0050), at a bandwidth
  cost, on the measured cells?
- **Binary:** ADR-0079's no-copy engine plus the §9.1 perf pass-through. The term-win and bind gauge
  `rlaw=` (§7 test 3) is strongly wanted, since without it the r movement is read only from `rr`.
- **Arms.**
  - The cells are c2-100 and c8-100, hint auto (δ = 0.5, β = 0), and `RWM_DELTA` is unset.
  - The flag takes 10⁻³, 10⁻⁴, 10⁻⁵ (control, equal to the default; an A/A against an
    unset-flag arm), 10⁻⁶ and 10⁻⁷.
  - max_fec_overhead stays 0.5 on all arms, so the cap is not confounded. Home's 0.3 is a separate
    arm if wanted.
  - n = 6 per arm, in interleaved blocks as in §51.
- **Witnesses:**
  - The `perf configuration` line shows each arm's value on both ends, or the row is VOID.
  - `rr` must move with the flag. If it does not, the session reads NO VERDICT (wiring).
- **Readouts:**
  - consumed r and coded per source;
  - P(late) per source, measured as (source losses − `fill_coded`)/src, with the resend count beside it;
  - `[SUCC]` resend and decode p50;
  - in-order delay;
  - goodput;
  - CPU/GB.
- **Predicted under ADR-0050** (the law ported at the fitted σ²; arithmetic, with goodput from the
  §6 identity):

| flag | c2 r | c8 r | c2 goodput | c8 goodput |
|---|---|---|---|---|
| 10⁻³ | 0.11 | 0.15 | 81.2 | 85.3 |
| 10⁻⁴ | 0.16 | 0.20 | 77.8 | 82.2 |
| 10⁻⁵ | 0.20 | 0.23 | 75.3 | 80.0 |
| 10⁻⁶ | 0.23 | 0.25 | 73.4 | 78.6 |
| 10⁻⁷ | 0.26 | 0.27 | 71.8 | 77.7 |

- **ADR-0050's direction:**
  - as the flag tightens, P(late) and in-order delay fall, and goodput falls;
  - P(late) should track the flag value by orders of magnitude: 10⁻³ to 10⁻⁷ is a factor 10⁴.
- **Outcomes (two-sided, resolution at the in-session A/A: c2 in-order d80 about 12 %; c8 about
  50 %, so c8 is secondary):**
  1. **The dial works.** P(late) falls monotonically with the flag by at least an order of magnitude
     across the sweep, and in-order delay falls beyond resolution at c2.
     - Keep the dial (option A in §9.0).
     - Make the law deliver its own target: exact mass tail, honest ε̂ and σ².
     - Price the default, not the dial.
  2. **The dial is inert.** P(late) moves by less than 2× and delay is flat (or rises) while goodput
     falls.
     - On these cells the dial buys bandwidth loss and no latency.
     - Supersede ADR-0050's hint → tail mapping with a priced form (option B, or the §6 candidate
       after re-measuring t_fec without copies).
     - The flag is remapped or deprecated in the same ADR.
  3. **Mixed.** P(late) falls but delay does not, which is the §3.7 race: decode is faster per hole,
     but the in-order frontier is held elsewhere (dwell or store).
     - Keep the dial's semantics, and report the delay holder as its own finding.
     - The dial is not the lever for delay on these cells.
  4. **r does not move with the flag:** NO VERDICT (wiring).
- **What it cannot test:** ρ < 1 (that is block E's seat), and Realtime's infeasibility (§9.3 says
  10⁻⁷ is infeasible by arithmetic, so the 10⁻⁷ arm tests only the cap regime).
