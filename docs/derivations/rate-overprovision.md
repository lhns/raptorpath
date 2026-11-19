# rate-overprovision: is the proactive rate r over-provisioned off Bulk, and by which term?

Status: DIAGNOSIS and derivation direction. No engine change, no VM, no cargo. The status, paper
and ledger texts are proposals (§8), not edits. Code read at `github/main` `3f07a4cd`. Evidence: the
152 rows of the off-Bulk sweep (status "§51 block 51 Result" and Addenda 1–2), read from the full
per-row logs (`s51b-ledger/x/b51c2|b51c8/run/rep51/diag-ld`, not committed) with the scripts in
`rate-overprovision/` (outputs committed beside them as `*.out`).

## 0. Answer in plain words

- **Against its own target, r is not over-provisioned; it is short.** At Auto, the law's own target
  t_tail/ε = 4·10⁻⁴ per window needs r ≈ 0.32 at c2 and 0.39 at c8's slow leg on the honest
  channel. The law gives 0.22 and 0.27.
- **Against what the δ dial needs at ρ = 1, it is over-provisioned, at Auto entirely.** At ρ = 1
  delivery needs no proactive repair at all (§3.5: P_arq = 1), so r buys only lateness. The NACK
  path still resends every loss about once on every arm, so each coded symbol is pure extra
  bandwidth that buys an earlier close for about 0.14 holes. Priced the way paper §4.9 prices a
  late symbol, the critical fractile at Auto is ≥ 1, so r = 0 is the optimum there.
- **The term that makes it so is the z·S margin** of the core term, z = Φ⁻¹(1 − t_tail/ε̂), with
  t_tail = 10⁻⁵·ζ(δ). Its base 10⁻⁵ has no price provenance. The margin is about 85–90 % of r;
  the information floor ε̂/(1−ε̂) is about 0.02. Nothing in the law credits the NACK channel.
- **The fix** replaces the per-window allowance t_tail/ε̂ with a critical fractile
  m = min(1, 1/c_hole(δ, ρ)), where c_hole is the price of one hole falling to ARQ. It is continuous
  in δ and ρ, has no branch, keeps Bulk at exactly 0, and gives r = 0 at Auto and roughly today's r
  at Realtime. Predicted goodput at Auto is 90 Mbit/s at c2 (76 today) and about 97.6 at c8 (83
  today).

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
| worst path | argmax of `loss_rate()` (`pl=`, the per-ack EWMA) | declared (§4.4) | `pl` prints 0.0000 on both legs on 55–59 % of c8 ticks; the pick is p1 on 86–92 %, which is the true worst leg. Single path at c2. |
| σ², B̂ | GE estimator, per-batch lumped feed | cited (GE), lumping undeclared (E08) | not printed. The netem truth gives an honest dispersion g·σ²_skb = 13.0 (c2) / 12.0 (c8 p1); the per-datagram GE fit of that channel gives 16.5 / 14.2. |
| W | `encoder.window_size()` | derived, clipped at 200 (R07) | 201 on every off-Bulk tick |
| t_tail base 10⁻⁵ | `config.rs:337` | **none**: a config default | it sets z |
| ζ(δ) = δ_A/δ | the dial | declared (§4.1) | yes |
| srtt | `estimator.rtt()` (EWMA, dwell-inclusive) | E01 | **no**: 206–251 ms at c2 and 264–600 ms on c8's p1, against 10 / 40 ms of netem delay |
| 1.5, C_DILUTION 0.5, grid, SAT_SOFTNESS 0.1 | `r_saturation` / `soft_saturate` | none (R04) | — |
| r_max = 0.5 | `config.rs:338` | declared | — |
| r_mass | `r_star_mass(mass, W, t_tail/ε̂, ε̂/ε_mass)` | derived (§4.3) | mass stats not printed |

**Wiring check.** `TaperBudget::accrue` is the identity: `owed=0.00` on all 16 843 `[SPAN]` lines
of both cells, so the grant is min(rate, spare) and `rr` is the law r after the spare clamp. Its
bank, taper shape and expiry cap never bind (its sentence promises a re-timing; it does none at
this cadence). Mean `rr` tracks the consumed r per arm (c2 Auto 0.22 / 0.195, c8 Auto 0.26 /
0.24, `inputs-*.out`). Spare is slack off Bulk: c2 infl ≈ 100 against cwnd 10 000.

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
- **c8 is reproduced by the core term at about the per-datagram GE σ²** (within 7 % at 14.2).
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
   K_N > R over N = W + R. At Auto (m = t_tail/ε = 4·10⁻⁴) it needs r = 0.32 at c2 and 0.39 on
   c8's p1. The law gives 0.22 and 0.27. This is the §4.2 "known limitation" (the closed form
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

- **Resends do not fall.** They stay at 0.026–0.0275 per source on every arm, about 1.0–1.09 per
  source loss. So a decode close is an earlier close of a hole that is resent anyway, never a saved
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
  So the fix replaces t_tail/ε̂ with m(δ, ρ) in both, and **deletes the base 10⁻⁵**.
- **The A1 check is measured.** Under A1 the marginal yield above *is* P(K > R). The shipped r is
  optimal only if c_hole ≥ 1/0.14 ≈ 7 at Auto and ≥ 12.5 at R±.

| symbol | provenance |
|---|---|
| D_arq | measured: one resend round, e.g. `[SUCC] orig_p50`, or 1.5·srtt on an honest srtt (E01) |
| d | Copa's delay normaliser, already the dial's currency (§4.9); not echoed by any gauge today |
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
| R02 (anchor body) | UNDERIVED | **UNDERIVED(t_tail base 10⁻⁵ has no price; no NACK credit; why max), DISHONEST-INPUT(ε̂ per E07, σ² per E08, srtt per E01 via r_burst and r_sat)**. Core wins at every off-Bulk point (inferred; no gauge). The z·S margin is 85–90 % of r. Over-provisioned against the δ dial at ρ = 1, short of its own target. max_overhead binds on 6–16 % of ticks. |
| R03 | unchanged | Measured on the P_fec > 0 side from I2 up in both cells. Read only by r_sat at ρ = 1. |
| R04 (r_sat) | UNDERIVED, DIVERGENT | **+ DISHONEST-INPUT(srtt = E01)**. Measured: slack at c2 (pressure ≤ 0.06, inferred), pressure 0.45–0.62 at c8 R± (inferred). Live r_sat ≈ 0.32 at c8, not the design's 0.245; no gauge, so not PINNED. |
| R05 | UNDERIVED, DEGENERATE(tunnel) | unchanged (χ = 0 on every row) |
| R06 (mass) | OK (unverified) | unchanged; **cannot be excluded as the c2 winner** (a 0.04 gap at Auto). Gauge needed before any verdict. |
| E07 (plu) | DIVERGENT | **+ DISHONEST-INPUT**: an "upper 95 % quantile" below the wire truth on 54–65 % of source-phase ticks and above 2× on 15–25 % (`plu` vs `[TRUTH]`). The cumulative `plc` matches truth, so the losses reach the feed. Inferred cause: the feed arrives in lumps (one ack delta carries a burst), and `pl` sits below print resolution on 82–86 % of c2 ticks. |
| E08 (GE) | DIVERGENT | Evidence for the unbounded lumping: c2 needs σ² ≈ 30 against the honest 13, if core wins. |

No MODE-SWITCH: no hint or δ/ρ threshold selects a law here.

## 6. Fix direction (a law change, formula first)

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
    δ_k = d/D_arq, which is near Auto (0.45–0.97 on the paper's two D_arq/d readings). It needs a
    bind-fraction gauge (CLAUDE.md FORMULA-FIRST), and the ±2 % nudge gates must include δ_k.
  - **Continuous in ρ** through P_arq(ρ). There is no `reliable` branch.
  - **Bulk.** β = 1 and m = 1 both give exactly 0, so the H-B bytes are unchanged.
  - **The presets move at Auto and Realtime** (a deliberate model change).
  - **ε → 0:** P(K_N > 0) → 0 < m, so r → 0. It is exactly 0 at ε = 0.
  - **N = 1** gives §4.2. **N = 2 with identical legs** equals N = 1 at the combined rate.
  - **Units:** m is a probability, and c_hole is in symbol-equivalents by construction.
- **Predictions.** Exact tail at honest inputs (`need.py`), two readings of D_arq/d (1.03 / 2.2,
  §4.9). Goodput from G = G_Bulk·(1 + x_B)/(1 + x_B + r), with x_B = resends + margin at H-B
  (c2 0.031, c8 0.035).
  - The identity reproduces today's c2 rows to within 1 % (Auto 75.8 against 76.1 measured, R−
    74.2 against 74.7, I3 83.5 against 84.0).
  - At c8 it reads 5 % low (Auto 79.1 against 82.9), because c8 is not purely wire-bound. Treat
    the c8 goodput numbers as ±5 %.

| point | m | c2 r (today 0.195–0.22) | c2 goodput (today) | c8 r (today 0.24–0.26) | c8 goodput (today) |
|---|---|---|---|---|---|
| I1–I3 | 1 | 0 | 90.0 (90.0 / 87.7 / 84.0) | 0 | 97.6 (98.7 / 95.6 / 93.2) |
| Auto | 1 / 0.91 | 0 / 0 | 90.0 (76.1) | 0 / ≈ 0.005 | 97.6 (82.9) |
| I4 (δ 5) | 0.19 / 0.09 | 0.05 / 0.09 | 85.8 / 82.8 (75.1) | 0.095 / 0.135 | 89.4 / 86.3 (81.4) |
| R± (δ 50) | 0.019 / 0.009 | 0.155 / 0.19 | 78.2 / 76.0 (74.6) | 0.215 / 0.25 | 80.8 / 78.6 (81.5 / 80.8) |

**Over-provision factor** (consumed r ÷ fixed r):
- **Auto:** unbounded in both cells (0.195 / 0.24 against 0).
- **I4:** c2 2.4–4.3×, c8 1.8–2.6×.
- **R±:** c2 1.16–1.43×, c8 1.04–1.2×.

Against the law's own target, the factor is 0.6–0.7 (short).

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
1. **Which late-hole price?**
   - **(a) §4.9 per symbol, δ·D_arq/d.** This makes Auto a corner, r = 0.
   - **(b) Head-of-line weighted.** Auto becomes interior.
   - And what is c_loss at ρ < 1?
2. **Is t_tail = 10⁻⁵·ζ(δ) a user-facing contract to keep?** If yes, r is short at Auto, not
   over, and the exact tail needs 0.32 / 0.39. That is the opposite fix.
3. **Should a decode close cancel the pending NACK resend?** That would give c_hole one resend
   symbol of bandwidth credit. Today it never does.

## 8. Proposed text (not written to those files)

**Paper, §4.4 (new paragraph "What the margin buys at ρ = 1").**
- At ρ = 1 a hole the margin misses is delivered one ARQ round late, and the resend is sent whatever
  the decoder did. Measured at s51b c2, resends stay at 1.0–1.09 per loss from H-B to R±, while
  coded closes 0.14 holes per symbol at Auto.
- The per-window allowance should therefore be the critical fractile m = min(1, 1/c_hole), with
  c_hole = P_arq(ρ)·δ·D_arq/d + (1−P_arq(ρ))·c_loss, not t_tail/ε̂. The base 10⁻⁵ then has no role.
- Then the provenance table of §3.3 above.

**Ledger.** The §5 table rows, as written there.

**status.md (§51 Addendum 3, proposed).**
- "The off-Bulk r is not over-provisioned against its own target (exact need 0.32 / 0.39 against
  0.22 / 0.27 at Auto). It is over-provisioned against the δ dial at ρ = 1: the z·S margin on an
  unpriced base 10⁻⁵ is 85–90 % of r, and the law credits no NACK capacity. Resends stay at about
  one per loss on every arm, and coded closes 0.14 holes per symbol."
- "The S06 fix does not change the slot count or this law's inputs."
- "Fix direction: the hole-price fractile (`docs/derivations/rate-overprovision.md` §6), pending
  owner questions 1–3."
- "Gauges missing: term-win, r_sat bind, mass stats."
