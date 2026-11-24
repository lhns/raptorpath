# Stage 3e registration: proactive repair in the three-knob model (no runs)

**Status.** Registration only, written before any run. Nothing here has been run in the offline simulation. Reviewed with the advisor (Fable) before it was written and again before the push (§12).

**Base.**
- Derivation `docs/derivations/three-knobs.md` on `three-knobs` at `93e29a22`. The laws are §1–§10 with Revisions 1–3 (§13, §15, §17); the findings are §18 (stage 3d) and §19 (the interactive simulator and the lifetime sweep).
- The offline simulation on `toy-stage3-knobs` at `e17b4c64`: `tools/toy/toy2.py`, `batch3d.py`, `check_ident3d.py`, `results-stage3d.md`.
- Repair laws as the engine and the paper define them: paper `docs/fec-arq-model.md` §3.3–§3.7, §4.1–§4.10 and §5.3–§5.4; `raptorpath-math/src/lib.rs` (`controller_rate`, `derive_window`, `r_saturation`, `soft_saturate`, `bulkness_of_delta`, `span_horizon_b`); `raptorpath/src/control/fec_rate.rs` (`rate_snapshot`, `derive_window`, `RepairRateCache`, `compute_repair_rate_capped`); `raptorpath/src/net/emit_source.rs:540–645` and `:780–900` (the cadenced rate, the spare-capacity clamp, A*, Δ, the trailing span).
- The interactive simulator on `viz-three-knobs`: `raptorpath-wasm/src/lib.rs` (`rate_now`, `rate_inputs_for`, `apply_age_edge`, `send_repair`, the slot loop), and `visualizer-three-knobs-plan.md`.

**What stage 3e adds.** The stage 3d sender S5 gains proactive repair, coded over the engine's trailing span at the rate law's r(δ), and the receiver gains a decoder. Everything else is S5 as stage 3d ran it.

## Short answer

- **Repair is already inside L1.** §2.1 lists "coded repair member" among the payload-bearing datagrams, so a repair may not carry any member of age ≥ T. A repair's age is the age of its oldest member, the lower end of its span (§2.2).
- **The span's lower edge comes from L2, not from the floor.**
  - A repair is a correction symbol (paper §3.4), so L2's worth test applies to each member it carries: a member s rides only if `a(s) + ô < T` (§2.3).
  - That is the cut §2.4 of the derivation already described ("V cuts the span's lower edge at T − ô"), stated directly now that V is empty (§17.2). It implies L1.
  - The brief's alternative, "the oldest protected source", switches repair off exactly where it is the only recovery. Units are unprotected at birth whenever no resend can arrive in time, which is §5's "T below one resend round". It is registered as an ablation arm and as owner question Q19.
- **The floor, the resend budget, the store pause and the dequeue rule are unchanged** (§2.4–§2.6).
  - Repair is paced by first sends (`owed += r` per first send, at most one repair per source), takes no store slot, is not charged to the resend budget, and carries the floor.
  - What changes is the link. Fresh data drains at `μ_eff = C / (1 + r + ρ)`.
- **Is repair overload self-limiting under L1?** In delay and memory, yes; in share, no (§2.6).
  - L1 and the dequeue rule bound the intake to `λ·(T − R)` and every sent unit's delay to about `T − R + d_fwd` at any r.
  - The worth edge never suspends repair, so under sustained overload repair displaces fresh data one for one. Repair at any r above the erasure minimum `ε/(1 − ε)` lowers the on-time share there, and the rate law at δ = 50 always sits above that minimum.
  - Two mechanisms do limit the load: the floor edge (the ablation), and the live rate law, whose window W grows with a queue's srtt and so lowers r.
- **Cells and arms.**
  - Three link cells at one tenth of stage 3d's rates (CLs, CLs-long, OVs), so a run takes a few seconds.
  - A call path where only repair can recover (RT: K-L10 without its outage, T = 0.12 s, about 1.2 round trips).
  - Arms: S5 without repair; S5 with repair at r = 0 (the identity); the live rate law; the law fixed at its unqueued value; r = 0.36 (the simulator's reading); and edge ablations.
- **Main predictions** (§8).
  - RT: the on-time share rises from 0.90 to about 0.98.
  - OVs: every repair arm loses on-time share to displacement (0.84 → 0.77 live, 0.71 fixed, 0.64 at r = 0.36).
  - CLs-long, never giving up: recovery after the outage goes from about 5.8 s to about 10 s with the live law, and does not happen within the run with the law fixed at its unqueued value.
  - At the call point (T = 0.4 s) the on-time share recovers within the readout's resolution in every arm except r = 0.36. With the law fixed, the delay stays pinned near T for the rest of the run.
- **Things in the existing laws that look wrong** are in §11. The rate law sizes r for a window W about twice the span A* that repairs are coded over, so it promises P_fec ≈ 0.997 where the span delivers about 0.80. W's floor of 16 sets r at call rates.

## 1. Symbols

New symbols only; §1 of the derivation holds for the rest.

| symbol | here | provenance |
|---|---|---|
| r | repair symbols per first send | the rate law (paper §4.4, §4.5), evaluated as in §2.1; or a declared constant in a fixed-r arm |
| W | the encoder window | `derive_window` (paper §4.8), capped by the sender's `MAX_WINDOW_SIZE = 200` |
| A*, D, Δ | the span width, the span deadline and the trailing offset | paper §5.3 |
| `rate_hat` | the measured first-send rate | §2.1 (the offline simulation's form of the paper §5.3 anchor) |
| owed | repair credit | `TaperBudget` (paper §3.3) |
| `[lo, end)` | a repair's span: the seqs coded into it | paper §5.3, plus the edge of §2.3 |
| e | the span's lower edge from the lifetime | §2.3 |
| ρ | resends per first send | measured |
| μ_s, μ_eff | the first-send drain rate of a saturated link | `C/(1 + r + ρ)` (§2.5) |
| `P_dec` | the fraction of lost first sends that the decoder recovers | measured; predicted by Appendix B |

## 2. How repair enters the three-knob laws

### 2.1 The repair laws as shipped, read as formulas

**Rate (paper §4.4, §4.5; `controller_rate` twice, mixed by `bulkness_of_delta`):**

```text
   r          =  (1 − β(δ))·r_anchor  +  β(δ)·r_late ,            β(50) = 0 exactly, so r = r_anchor at the call point
   r_anchor   =  clamp( soft_sat( max(r_core, r_burst) , r_sat ) , 0 , r_max )
   r_core     =  max( 0 , ε̂/(1−ε̂) + z·√( ε̂·σ² / (W·(1−ε̂)) ) )  +  ε_codec·(1 − (1−ε̂)^W)·1{core > 0}
   z          =  Φ⁻¹( 1 − t_tail(δ)/ε̂ ) ,      t_tail(δ) = clamp( 10⁻⁵·δ_auto/δ , 10⁻⁹ , 0.1 ) = 10⁻⁷ at δ = 50
   r_burst    =  (B̂ / t_symbols)·(1 − t_tail/ε̂)⁺ ,      r_sat = argmin of the p99 model (paper §4.4)
```

**Window (paper §4.8; `FecRateController::derive_window`, latency budget 0, so about one RTT):**

```text
   W  =  min( round( clamp( clamp( W_over , min(W_bur, W_lat) , W_lat ) , 16 , 512 ) ) , 200 ) ,    W_lat = srtt · rate_hat
```

**Span (paper §5.3; `emit_source.rs:600–645`, `:827–846`):**

```text
   D    =  min( b(δ)·srtt , 2·srtt ) ,          b(50) = ½    (the 2·srtt term never binds, §2.4 of the derivation)
   A*   =  clamp( ⌈rate_hat·D⌉ , 1 , W ) ,       Δ = clamp( ⌈rate_hat·J⌉ , 1 , 64 )
   end  =  n_sent − Δ ,     ws = max( A_known , n_sent − W )
   span =  [ max( ws , end − A* , e ) ,  end )                  e: the lifetime edge, §2.3
```

**Emission (paper §3.3, `TaperBudget`; §5.3, "repair grant ≤ 1 per source"):**

```text
   at every first send:   owed += r ;   if owed ≥ 1:  owed −= 1, stage one repair over the span
   cadence:               r and W are re-read once min(5 ms, srtt/4) has passed since the last reading      (RepairRateCache::period_us)
```

| symbol | provenance |
|---|---|
| ε̂ | **declared idealisation:** the cell's first-send loss `p0`. The engine reads the BOCD 95 % predictive upper quantile (`predictive_loss_upper(0.95)`), which is at least the mean. So the law arm's r is a **lower bound** on the engine's r for the same channel (owner question Q21) |
| σ², B̂ | 1 and 1: every cell is i.i.d. These are the GE values of an i.i.d. channel and also the engine's fallback when the GE estimate is invalid (`rate_snapshot`) |
| t_symbols, t_sym | `srtt·rate_hat` (at least 1) and `1/rate_hat`, as `rate_snapshot` forms them from throughput |
| ε_codec | 0.004, RLC (paper §3.6) |
| r_max | 0.5, the engine's `max_overhead` |
| the mass term | inert: no measured mass statistics (`MassStats::default()`), as at the engine's cold start |
| srtt | measured, RFC 6298, with the 333 ms seed before the first sample (stage 3b onwards). The engine's D reads the estimator's smoothed RTT, not RTprop, a divergence the paper already records (§5.3, §11.3). The offline simulation follows the engine |
| `rate_hat` | first sends in `(now − 8·srtt, now]` divided by `min(8·srtt, now − first send)`. The window length is paper §5.3's ("window ≈ 8 SRTT"). The engine takes a windowed **maximum** of srtt/2 buckets; at 50 units/s a bucket holds 2 or 3 sends, so the maximum reads 60/s and moves A* from 3 to 4. The mean is chosen and declared. Before the first RTT sample A* = 1, as the engine's cold start (`emit_source.rs:607`) |
| J | the cell's delay-process amplitude, 1 ms (declared). The engine measures jitter |
| b(δ), δ = 50 | paper §5.4. b's lower clamp ½ binds for every δ ≥ 50, so the call point sits on its edge |
| the cadence | `RATE_CADENCE_MAX_US = 5 ms` and srtt/4 (`fec_rate.rs:181`, `:234`). A reading is taken at the first first-send after the period has passed, so it schedules no event of its own |

**The reference values**, from the real `raptorpath-math` functions (Appendix A; a Python port agrees to 5 decimals):

| operating point | ε̂ | srtt | rate_hat | W | r | r_sat |
|---|---|---|---|---|---|---|
| CLs, CLs-long, OVs, unqueued | 0.03 | 0.010 | 1 000 or 1 200 | 16 (floor binds) | 0.22952 | 0.360 |
| CL at stage 3d's 10 000/s | 0.03 | 0.010 | 10 000 | 100 | 0.11234 | 0.145 |
| CLs draining behind the pause | 0.03 | 0.100 | 1 129 | 113 | 0.10744 | 0.135 |
| OVs in steady overload | 0.03 | 0.098 | 895 | 88 | 0.11763 | 0.155 |
| RT | 0.10 | 0.100 | 50 | 16 (floor binds) | 0.38355 | 0.385 (binds: raw 0.5105) |

Two clamps set r at the call point in the scaled cells: W's floor of 16, and in RT the saturation knee. Both are registered as bind gauges with expected fractions (§6, Q3e-2).

### 2.2 L1 covers repair, member by member

L1 (§2.1 of the derivation) is: no payload-bearing datagram for s is staged once `a(s) ≥ T`. Its absolute test already names "coded repair member".

A repair carries the payload of every member of its span. So:

```text
   stage a repair over [lo, end)   ⇒   a(s) < T  for every s in [lo, end)   ⇔   a(lo) < T
```

- **The equivalence holds because ages fall with seq.** Units are taken from the intake in hand-over order, so `t_h(s)` increases with s (`emit`).
- **The repair's age is therefore its oldest member's**, `a_rep = now − t_h(lo)`. There is no other age to give it: a repair has no hand-over of its own.
- **No new symbol.** The test extends the existing L1 gauge to repair members (`l1_viol_rep`, which must read 0).

### 2.3 The span's lower edge: L2 applied to a correction symbol

L1 says what a repair may carry. Whether a member is worth carrying is L2's question.
- L2 (§2.2 of the derivation, with ô from §17.1) admits a resend of s only if `a(s) + ô < T`.
- Paper §3.4 treats a resend and a repair as one thing, a correction symbol: the same slot, the same channel, the same cost.
- A repair sent now arrives about ô later. So a member with `a(s) + ô ≥ T` would be decoded after its lifetime even in the best case.
- Carrying it is not free. If that member is itself lost, it is one more unknown in the equation, and it takes rank that a fresh member could have used.

So the edge is:

```text
   e  =  min{ s ≥ ws :  a(s) + ô  <  T } ,      ô = srtt − RTprop/2     (§17.1)
```

- **It is the cut §2.4 of the derivation already stated.** "In effect V cuts the span's lower edge at T − ô." V has been empty since §17.2, so the cut must now be written as its own rule. It adds no symbol and no constant.
- **It implies L1,** because ô ≥ 0.
- **It is a per-member state comparison, like L2.** No knob is compared with a constant. At T = ∞ it never binds.
- **A repair whose span is empty after the edge is not sent.** Its credit is spent and counted (`rep_drop_edge`). Under the dequeue rule a unit is sent only while `a + R < T`, and R > ô, so the newest member nearly always passes and the edge shortens spans rather than emptying them. The prediction is `rep_drop_edge/credits ≤ 0.05` (Q3e-8).

**The two alternatives** (owner question Q19, both registered as ablation arms):
- **The age edge `a(s) < T`.** This is plain L1, the interactive simulator's `apply_age_edge`. It carries members that can only be decoded late, so it should deliver at least as much in the end but no more on time.
- **The floor edge `lo ≥ F_ann`.** This is the brief's "oldest protected source", and it is the literal reading of the paper's "F the oldest unresolved position" once the floor became the protected floor (§15.3).
  - `protected` reads the earliest **resend**: `max(t, last_send + d(s)) − t_h + ô < T`.
  - In RT, `d = min(srtt, P) = 0.1 s` and `ô = 0.051 s`, so every unit is unprotected at birth (0.151 ≥ 0.12). F_ann then sits at the send frontier and every span is empty.
  - So the floor edge switches repair off exactly in §5's "T below one resend round", where the derivation says repair is the only recovery. That is why it is not the default.
  - It does limit overload, though. In OVs every unit is sent unprotected after the first second, so repair stops there too (§2.6).

### 2.4 The floor and the commitment

- **The floor law is unchanged.**
  - `protected(s)` reads the resend trigger only, as §17.1 defines it. Repair does not make a seq "protected", because protection says when the sender can next *resend* s.
  - Whether the floor should also count "a repair sent now could still carry s" is owner question Q20. Default: no. That rule would hold the floor about T − ô behind, and strict order would wait for it (the §15.2 collapse).
- **The commitment ("no copy of s once s < F has been announced") stays a statement about copies.** `commit_viol` still counts copies only.
- **A repair may carry members below F_ann under the worth edge.**
  - If such a member is decoded after the receiver passed it, that is a late fill, delivered at once out of order (`reorder.rs:75–82`). It is counted as `late_floor_dec`.
  - At H = 0, every arm here, the hole was already released at detection, so this is an ordinary fill.
  - At H = ∞ it would be an out-of-order delivery. The stage 3c pin "`late_floor` = 0 on one FIFO path" therefore becomes `late_floor − late_floor_dec = 0`.
- **The floor rides on repair datagrams.** Under the header carrier (Q15's default), a repair datagram is a data datagram, so it carries F_ann like any other (`carry_floor`).

### 2.5 The resend budget, the store pause and the link

- **Resend budget: repair is exempt.**
  - In the engine, proactive repair is accrued by `taper_budget` in `emit_source.rs`. That file never touches `nack_repairs_this_period` or `cached_nack_budget`.
  - The NACK budget is decremented only at the gap serve, the margin repair and the request arm (§15.4's engine read).
  - The offline simulation's brake (10 copies per 5 ms) therefore counts copies only, as now.
- **Store pause: repairs hold no store slot.**
  - The pause `O ≥ B` counts seqs, and a repair has none. Repair load is bounded by the source clock instead: at most one repair per first send, and r per first send on average.
  - The link queue then holds about `(B − λ_s·RTprop_eff)·(1 + r)` datagrams behind a pause. With B = 100 and Q = 125 that is 111 at r = 0.23 and 122 at r = 0.36, within 3 of Q (Q3e-9).
- **The link.** Source, resends and repairs share one FIFO. With the link saturated, fresh data drains at

```text
   μ_eff  =  C / (1 + r + ρ) ,       ρ = resends per first send (measured)
```

  At r = 0 and I2's loss, ρ = p0/(1 − pc) = 0.033 and μ_eff = 0.968·C. That is §19's `μ(1 − ε)`, with the copy loss in place of the first-send loss.

### 2.6 The dequeue rule, and whether repair overload limits itself

**The dequeue rule is unchanged.** Its R reads the measured srtt, so the queueing that repairs add is priced automatically. Repair credit accrues at a first send, so an expired unit earns none.

**What L1 and the dequeue rule bound at any r.**
- The intake holds at most about `λ·(T − R)` units.
- Every unit sent leaves at age below `T − R` and arrives near `T − R + d_fwd`, which is about T − 10 ms under overload (stage 3d: p50 388–390 ms).
- So with repair the delay and the memory stay bounded, as without it. Only at T = ∞ is there no bound.

**What they do not bound is the share.** Under sustained overload the steady on-time share is:

```text
   share  ≈  (μ_s / λ) · ( 1 − ε·(1 − P_dec,T) ) ,      μ_s = C/(1 + r + ρ)
```

- At r = 0, units sent at `T − R` are unprotected, so no first-send loss is recovered: `share = (C/λ)·(1 − ε)`.
- So repair raises the steady share only if `(1 − ε(1 − P_dec))/(1 − ε) > 1 + r`. That needs `r < ε·P_dec/(1 − ε) ≤ ε/(1 − ε)`, which is the erasure minimum.
- At δ = 50, `t_tail = 10⁻⁷ < ε̂`, so z > 0 and `r_core > ε̂/(1 − ε̂)`. **The rate law never pays under sustained overload.** This is paper §3.7's displacement, `R_frontier = R_cc·(1 − φ_early)`, now in the lifetime setting.

**Three ways the load could fall, and which arm reads each:**
1. **The worth edge: no.** It shortens spans (in OVs to about `μ_s·(R − ô) ≈ 8` members, against A* ≈ 44) but almost never empties them, so repair keeps its share of the link.
2. **The floor edge: yes** (arm `RL-floor`). Once units are sent unprotected, spans are empty and repair stops. In OVs that happens within the first second. After an outage in CLs-long, repair stops until the intake wait falls below `T_p = T − d − ô ≈ 0.2 s`.
3. **The live rate law: partly** (arm `RL` against the fixed arm `RS`). A queue raises srtt, so `W_lat = srtt·rate_hat` grows, W grows and the margin term shrinks as `W^(−½)`. In OVs r falls from 0.23 to 0.12, and in the CLs drain to 0.11 (the table in §2.1).
   - **This feedback rests on a queue that the offline simulation builds only behind its store pause** (B/C = 80 ms), with no congestion control.
   - A delay-based controller holding its queue near its target would keep srtt, and so r, near the unqueued value. `RS` (r fixed at 0.2295) stands for that case.
   - The engine's spare-capacity clamp, `min(r, (cwnd − in_flight)/in_flight)` (`emit_source.rs:578`), is a fourth limit. The offline simulation has no cwnd, so it is not modelled (Q22). It can only lower r, so the offline simulation's repair load is an upper bound on what the clamp would allow.

**With headroom** (`λ(1 + r + ρ) < C`) repair costs no share but shrinks the drain after an outage.
- The time until fresh data is on time again, for a sender that never gives up (the brief's form, §19's law):

```text
   t_rec  =  ( L·λ/μ_eff − τ ) / ( 1 − λ/μ_eff )                       L = outage, τ = 0.4 s (the readout's reference)
   t_rec' =  ( (L·λ + B_lost)/μ_eff − (τ − d_fwd) ) / ( 1 − λ/μ_eff )   with §13.6's lost store and forward delay
```

- **At the call point the backlog does not live that long.** At T = 0.4 the dequeue rule keeps only the last `T − R` of hand-overs, so `L` is replaced by `min(L, T − R)`.
  - The on-time share then recovers within the readout's resolution, as in stage 3d (0.10 s in every T = 0.4 arm).
  - The **delay** recovers only when that backlog has drained, after `λ·(T − R)/(μ_eff − λ)`. With r fixed at 0.2295, `μ_eff − λ ≈ 16/s`, so the delay stays near T for the rest of the run.
  - That is what the 0.15 s readout of §6 reads. The 0.15 s is G.114's comfort level, which §4 of the derivation cites.

### 2.7 The receiver: generic-rank decoding

The decoder is modelled by its rank, not by GF(256) arithmetic:
- **The system.** Each received repair is an equation whose unknowns are its members not yet held.
- **What is decoded.** The unknowns the system determines, using the generic-rank rule for sparse systems. For generic coefficients the rank of a sparse system equals its maximum bipartite matching between unknowns and equations. The unknowns it determines are those outside the under-determined part of the Dulmage–Mendelsohn decomposition, that is, those no alternating path reaches from an unmatched unknown (Dulmage & Mendelsohn 1958; Murota, *Matrices and Matroids for Systems Analysis*, 2000, ch. 2).
- **Declared idealisation.** Random GF(256) coefficients are treated as generic, so singular draws are ignored. The engine's RLC decoder is incremental Gaussian elimination (`raptorpath-math/src/rlc.rs`). Its codec overhead, 0.004, is already added to r for exactly this (paper §3.6), so the offline simulation spends that overhead and gets nothing for it: the conservative side.
- **Decoding on time.** The span trails the frontier by Δ and starts at the edge, so on one FIFO path every member has arrived or been lost when the repair arrives. A determined unknown is decoded at that arrival, and a decode can cascade through older equations.

### 2.8 Continuity (NO-MODE-SWITCH)

- **The rate.** r enters only as `owed += r`. Nothing compares r with anything.
  - At r = 0, owed never reaches 1, so no repair is staged, no draw is taken and no event is pushed. The trace is that of stage 3d's S5 by arithmetic, not by a branch on r. That is the identity check, Q3e-1.
- **The edge.** A per-member comparison of state with T, like L2. At T = ∞ it never binds.
- **The law's inputs.** δ enters through t_tail and β (β = 0 exactly at δ = 50) and through b (clamped at ½ for δ ≥ 50).
  - At δ = 50·(1 ± 0.02), r moves by less than 3·10⁻⁵ and A* not at all (Appendix A). δ nudges would therefore have no power here, as K-L10's plants had none in stage 3d, so none are registered.
  - The law's own continuity is pinned in `raptorpath-math` (`the_rate_does_not_step_across_the_bulk_preset_or_anywhere_on_the_dial`).
- **The edge selector.** `repair["edge"]` selects an ablation arm, in the same way as `floor_law`. It is not a knob and no knob feeds it.

## 3. New code (exact)

Build on `toy-stage3-knobs` at `e17b4c64`, in `tools/toy/`. Extend `toy2.py`; add `batch3e.py`, `summarize3e.py`, `check_ident3e.py` and tests in `test_toy2.py` class `Stage3e`. **With every new field at its default the code reproduces stage 3d bit for bit.**

**New `Cfg` fields:**
- `repair: object = None`. Either None (stage 3d), or `dict(r=…, edge=…)` with:
  - `r`: `"law"` (live, §2.1) or a float (fixed);
  - `edge`: `"worth"` (default), `"age"` or `"floor"`.
- `repair_seam: object = None`. Test only: `"no_edge"` disables the edge, for the positive control. Named in the deny test's exemptions.

**New constants:** `SALT_REPAIR = 7`; `CODEC_RLC = 0.004`; `R_MAX = 0.5`; `W_CAP = 200`; `RATE_CADENCE_MAX = 0.005`. Each carries its source in a comment (§2.1).

**New module-level law functions.** Add each to the deny test's `EXEMPT`, by name only.
- `rate_law(p, srtt, rate, delta)`: the composition of Appendix A, both terms of the β mix computed, no branch on β.
- `derive_window(t_tail, eps, sigma2, srtt, rate)`: paper §4.8, as `raptorpath-math`.
- `span_edge_ok(edge, T, age, o)`: returns `age + o < T` for `"worth"`, `age < T` for `"age"`. (`"floor"` is applied as `lo ≥ F_ann` in `stage_repair`.)

**Sender** (`Sim`, all repair state initialised even when `repair` is None; nothing reads it then):
1. **State:** `owed = 0.0`, `n_rep = 0` (the repair index j), `t_eval = −inf`, `r_cur`, `W_cur`, and a deque of first-send times for `rate_hat`.
2. **In `emit`,** after the source datagram is staged, its floor carried and the floor updated, and only when `repair` is not None:
   - re-read `r_cur` and `W_cur` if `t − t_eval ≥ min(RATE_CADENCE_MAX, srtt_eff/4)`;
   - `owed += r_cur`;
   - if `owed ≥ 1.0`: `owed −= 1.0` and `stage_repair(t)`.
   - A fixed-r arm sets `r_cur = r` and still re-reads `W_cur` live: W is the window law, not the rate.
3. **`stage_repair(t)`:**
   - compute `A*`, `Δ`, `end`, `ws` and `lo = max(ws, end − A*)` by §2.1;
   - raise `lo` past every member failing `span_edge_ok` (`"worth"`, `"age"`), or to `max(lo, F_ann)` (`"floor"`);
   - if `end ≤ lo`, count `rep_drop_edge` when the edge moved lo, else `rep_drop_young`, and return;
   - otherwise `j = n_rep; n_rep += 1`;
   - count `l1_viol_rep` for members with `t − h_seq[s] ≥ T` and `worth_viol_rep` for members with `t − h_seq[s] + o_hat() ≥ T` (gauges on every arm);
   - `a, dep = fwd(t)`. Lost iff `a is None` (queue drop, `rep_qdrop`), or the outage covers it (`dead` set, `lo ≥ dead[0]` and `dep` in the window), or `u01(seed, j, 0, 0, SALT_REPAIR) < p0`;
   - if it arrives: `push(a, 11, (j, lo, end, t))`, then `carry_floor(a)`.
   - The repair draws its loss at `p0`, the cell's first-send rate: a repair is a fresh datagram, not a copy. I2's `pc = 0.10` models copies.
4. **RTT sampling:** reports gain `rhi_data`, the newest seq that arrived as a datagram rather than by decode, appended at the end of the tuple after the per-path field. The RFC 6298 sample is taken from `rhi_data` where stage 3d took it from `rhi`. With no decodes `rhi_data == rhi`, so stage 3d is unchanged.

**Receiver:**
5. **`on_repair(now, j, lo, end, tau)` (event kind 11):**
   - `tau_rx_recv = max(tau_rx_recv, tau)`: RACK counts every datagram;
   - `U = {s in [lo, end) : not got[s]}`. If U is empty, count `rep_redundant` and return;
   - otherwise count `rep_innov`, add the equation, and run `decode(now)` on its connected component.
6. **`decode(now)` on a component** (unknowns linked by shared equations):
   - take a maximum matching (augmenting paths);
   - the determined set is every unknown not reached by an alternating path (unknown → any equation containing it → that equation's matched unknown) from an unmatched unknown;
   - deliver each determined s in increasing seq order through the data path with copy index −1. That path: no `tau` update, no `est.feed`, no `rhi_data` change, opens holes up to s and raises `rhi` if `s > rhi`, as `on_data` does;
   - count `dec_units`, `dec_on_time` (`now − h_seq[s] ≤ T`), `dec_after_release` (s had been released) and `late_floor_dec` (s had been passed by the floor);
   - count every equation matched to a decoded unknown as `rep_used`;
   - remove decoded unknowns from every equation and drop empty equations.
7. **A source or copy arrival of a seq that sits in a pending equation** removes it, and `decode` runs again on that component (the cascade). A source or copy arriving for a decoded seq counts as `dup` and also as `dup_after_dec`.
8. **Memory:** equations are dropped only when empty. Gauge `dec_comp_max`, the largest component decoded.

**What must not change at r = 0:** no event is pushed, no draw is taken, `ctr` and `ps_ctr` do not advance, and the rate-law reading schedules nothing.

## 4. Cells

The link cells are stage 3d's at one tenth of the rate. The rate, the link capacity C, the queue Q, the store pause B and n_total are all multiplied by 0.1. owd, J, the loss rates, the outage times, T, the tick and the engine's resend brake (10 per 5 ms) are not scaled: they are times, probabilities or engine constants. Every arm uses the engine-like sweep (`sweep_form = "blocker"`, `rx_refresh`), because stage 3d found the T = 0.4 rows move by under 1 point between sweep forms and the blocker form is the engine's.

| cell | loss | owd | rate λ | link C, Q | B | units | outage | sweep | t_limit |
|---|---|---|---|---|---|---|---|---|---|
| **CLs** | I2 (p0 3 %, pc 10 %) | 5 ms | 1 000/s | 1 250/s, 125 | 100 | 10 000 (10 s) | 2.0 s to 3.5 s | blocker | 40 s |
| **CLs-long** | I2 | 5 ms | 1 000/s | 1 250/s, 125 | 100 | 20 000 (20 s) | 2.0 s to 3.5 s | blocker | 80 s |
| **OVs** | I2 | 5 ms | 1 200/s | 1 000/s, 125 | 100 | 12 000 (10 s) | none | blocker | 40 s |
| **RT** | L10 (10 %, 10 %) | 50 ms | 50/s | none | default | 3 000 (60 s) | none | tail | 80 s |

- **The link cells' other settings** are stage 3c's `LINK_KW` with `pause_B = 100`: work-conserving source, RFC 6298 RTT, the engine's cooldown gate, the engine's brake, J = 1 ms. `windows = True`.
- **RT** is K-L10 without its outage. Repair should help there:
  - the loss is independent;
  - the lifetime is 1.2 RTT;
  - no resend can arrive in time: the first report-driven resend would arrive at about 0.17 s, and the sweep's at `0.1 + ô ≥ 0.15` s, both past 0.12 s. So L2 refuses every resend and the share without repair is `1 − p0`.
  - Sweep and floor are as in the K cells.
- **What the scale changes** (owner question Q23):
  - at 1 000 units/s, `W_lat = srtt·rate ≈ 10`, so W sits on its floor of 16 and the unqueued law gives r = 0.2295;
  - at stage 3d's 10 000/s it gives W = 100 and r = 0.1123;
  - the scaled cells therefore test the law at call-like rates, where the floor binds, not at 3d's rates;
  - the resend brake (2 000/s) exceeds the scaled link, so it never binds (gauged).
- **Run time:**
  - The target is a few seconds per run. The worker times the first run of each cell and reports.
  - A cell above 15 s per run is reported before the batch continues; it is not rescaled silently.
- **Memory** (stage 3d's first batch died of a `MemoryError` with 15 workers):
  - at most 2 workers;
  - CLs-long with 1 worker;
  - `check_ident3e.py` alone, with no batch running.

## 5. Arms

All arms: δ = 50, H = 0. T = 0.4 s unless stated; in RT, T = 0.12 s unless stated. 12 seeds, phases per seed. r for each arm is the table of §2.1.

| arm | sender | r | edge | cells |
|---|---|---|---|---|
| `S5` | stage 3d's S5, `repair = None` | — | — | all |
| `R0` | S5 + repair | 0 | worth | all (identity with `S5`) |
| `RL` | S5 + repair | the live law | worth | all |
| `RS` | S5 + repair | fixed at the unqueued law value, 0.2295 | worth | CLs, CLs-long, OVs |
| `RF3` | S5 + repair | 0.36, the simulator's reading at the Realtime δ (§19) | worth | all |
| `RF1` | S5 + repair | 0.112, the law at stage 3d's 10 000/s (a low dose) | worth | RT |
| `RL-age` | as RL | live law | age (the simulator's) | OVs |
| `RL-floor` | as RL | live law | floor | OVs, CLs-long, RT |
| `S5-inf`, `RL-inf`, `RS-inf` | as S5, RL, RS at T = ∞ (never gives up, any order) | | | CLs-long |
| `S5-T0p4`, `RL-T0p4` | S5 and RL at the call point, T = 0.4 | | | RT |
| `AA` | S5, phase variant 1 | | | all (frozen as `nf_stage3e.json`) |

That is 33 configurations, or 396 runs. RS is not run in RT, because there r is constant (srtt does not move) and RS would equal RL.

## 6. Readouts and gauges

**Every stage 3d readout, unchanged.** That includes:
- `share_by_0p4`, `share_by_2`, `share_T` and `share_any`;
- the delay from hand-over p50/p90/p99 over all units handed over;
- the window series;
- `recovery_time` at 0.4 s;
- `commit_viol`, `skip_below_floor`, `late_floor`, `pass_floor`, `dup`;
- `finished`, `queue_drop_in`, `queue_drop_out`, `resend_braked`, `intake_expired` and `intake_expired_deq`.

No mean is taken over delivered units alone.

**New readouts:**
- `recovery_time_0p15`: the stage 3d function at `thr = 0.15` s (the delay recovery, §2.6);
- per 1 s window: repair datagrams sent, the measured r, W, A*, and decoded units;
- `mu_eff_hat`: `C · (first sends)/(all link datagrams)` over the saturated windows (link busy ≥ 0.98) after the outage end. It feeds the recovery formula evaluated per run (Q3e-7).

**Clamp bind fractions** (FORMULA-FIRST: every clamp):

| clamp | gauge | expected (Q3e-2) |
|---|---|---|
| W's floor 16 | `W_floor_bind`: readings whose pre-clamp W < 16 | RT 1.0; CLs, CLs-long ≥ 0.5 of readings (all unqueued time); OVs ≤ 0.2 (the ramp only) |
| W's 512 and the 200 cap | `W_cap_bind` | 0 |
| r's saturation knee | `sat_pressure`: mean of `saturation_pressure(raw, r_sat)` | RT ≥ 0.9; CLs unqueued ≤ 0.05; queued ≤ 0.25 |
| r_max = 0.5 | `rmax_bind` | 0 (in RT the knee binds first) |
| the burst term | `burst_bind`: r_burst > r_core | 0 |
| A*'s `[1, W]` | `astar_floor_bind`, `astar_W_bind` | the cold start only (< 0.01); 0 |
| Δ's `[1, 64]` | `delta_bind` | 0 (`⌈rate·J⌉` is 1 without the clamp) |
| D's 2·srtt | `D_cap_bind` | 0 (b = ½) |
| the span edge | `rep_cut_edge` (repairs whose lo the edge raised), `rep_drop_edge`, `rep_drop_young` | §8 |
| one repair per first send | `owed_max` | < 1.5 (r ≤ 0.5) |
| the existing clamps | the dequeue expiry, the brake, the pause, `hull_bind` | reported as in stage 3d |

**Repair gauges:**
- `rep_credits`, `rep_sent`, `rep_qdrop`, `rep_lost`;
- `rep_redundant` (arrived holding nothing unknown) and `rep_innov`;
- `rep_used`: repair symbols that repaired something;
- `dec_units`, `dec_on_time`, `dec_after_release`, `late_floor_dec`, `dup_after_dec`;
- `p_dec_meas = dec_units / lost first sends`, beside `p_fec_law`, the law's own `P_fec(r, W)` (paper §4.2) averaged over readings: the predicted mismatch of §11;
- `l1_viol_rep` (must be 0), `worth_viol_rep` (must be 0 under the worth edge; positive under the age edge), `dec_comp_max`;
- `rep_link_share`: repair datagrams over all link datagrams in saturated windows.

## 7. Tests (red first, then green)

1. **`check_ident3e.py`.** Stage 3d reproduced with `repair = None`, following `check_ident3d.py`'s pattern:
   - regenerate the references from a git archive of `e17b4c64` into `out3e/ref/`;
   - use every configuration of `check_ident3d.py`, plus one per `batch3d.py` JOBS name, at seeds 0 to 2;
   - compare every key, trace and per-seq digest.
2. **R0 equals S5:**
   - in every `batch3d.py` cell (21 cells, both sweep forms, P2), seeds 0 to 2;
   - in the four new cells, seeds 0 to 11;
   - every stage 3d key and the sender trace (§2.8).
3. **`rate_law` and `derive_window`:**
   - reproduce the table of §2.1 to 1e-5;
   - reproduce, to 1e-9, a grid computed by the Appendix A program against `raptorpath-math` (ε ∈ {0.01, 0.03, 0.1}, srtt ∈ {0.01, 0.1, 0.3}, rate ∈ {50, 1 000, 10 000}). The program is built in a scratch crate outside the repository, and its output is committed as `rate_ref3e.json`.
4. **The span, on constructed state:** no edge bind; a worth cut; an age cut; a floor cut; an empty span counted as `rep_drop_edge` against `rep_drop_young`; A* = 1 before the first RTT sample.
5. **The L1 control:**
   - RT at T = 0.04 s with `repair_seam = "no_edge"`: `l1_viol_rep > 0` (spans reach 0.06 s);
   - the same run with the edge: `l1_viol_rep = 0`.
6. **The decoder, on constructed systems:**
   - peeling;
   - a 2 × 2 block;
   - `{h1,h2}, {h1,h2}, {h1,h2,h3,h4}`, which decodes h1 and h2 only;
   - `{h1,h2}, {h2,h3}`, which decodes nothing;
   - a redundant equation;
   - a cascade through an older equation on a later source arrival;
   - a decode after an H = 0 release (a fill, `dec_after_release`);
   - a decode after a floor pass (`late_floor_dec`).
7. **Emission:**
   - over N first sends, repairs staged plus drops equal `floor(N·r)` or one less;
   - never more than one repair per first send;
   - none while paused;
   - at r = 0, no event pushed and `ctr` unchanged.
8. **The repair loss draw** uses `SALT_REPAIR` and `p0`, and the outage window drops repairs.
9. **`recovery_time_0p15`** on a synthetic run with a known answer.
10. **RTT sampling** ignores a decoded seq above `rhi_data`.
11. **The deny test** with the new law functions exempt by name, and a planted positive control.

## 8. Predictions (written before any run)

Seed means over 12 seeds unless stated. "S5 ± x" is a band around the same cell's S5 mean. A paired difference uses the stage 3d reading rule with the A/A floor m from `nf_stage3e.json` ("equivalent" = the paired 95 % interval lies within ±m).

**Q3e-1, identity and liveness.**
- `check_ident3e.py` PASS: stage 3d reproduced on every key.
- R0 equals S5 on every stage 3d key and trace in every cell, old and new.
- Every run of every arm is `finished`.
- FAIL: any difference or any unfinished run.

**Q3e-2, the rate law and its clamps.**
- `rate_law` reproduces §2.1's table and the reference grid.
- `RL`'s measured r:
  - CLs and CLs-long before the outage: [0.20, 0.235];
  - in the saturated windows after the outage: [0.09, 0.14];
  - OVs in windows after 2 s: [0.10, 0.14], with W in [70, 110];
  - RT: 0.3835 ± 0.002.
- Bind fractions as the table of §6. Two of them are defect findings by MEASUREMENT DISCIPLINE 17 and 18, each needing a ledger verdict:
  - `W_floor_bind` = 1.0 in RT and ≥ 0.5 in CLs: r is set by W's floor;
  - `sat_pressure` ≥ 0.9 in RT: r is set by the knee.

**Q3e-3, L1 and the commitment.**
- `l1_viol_rep = 0` in every registered run.
- `worth_viol_rep = 0` in every worth-edge and floor-edge run, and > 0 in `RL-age` (OVs). The age arm is the positive control of the worth gauge.
- `commit_viol = 0` in every run.
- `late_floor − late_floor_dec = 0` in every run (one FIFO path).

**Q3e-4, RT: where only repair recovers** (T = 0.12 s; on-time = `share_T`):
- S5 `share_T` in [0.885, 0.915] (1 − p0 = 0.90), with **0 copies in every run** (L2 refuses every resend, §4).
- Shares:

| arm | `share_T` band | central (Appendix B) | `share_any` |
|---|---|---|---|
| `S5` | [0.885, 0.915] | 0.900 | 0.900 |
| `RF1` (0.112) | [0.910, 0.940] | 0.924 | ≈ 0.924 |
| `RL` (0.3835) | [0.960, 0.990] | 0.978 | ≈ 0.980 |
| `RF3` (0.36) | [0.958, 0.988] | 0.976 | ≈ 0.977 |
| `RL-floor` | identical to S5 on every stage 3d key | 0.900 | 0.900 |

- `RL` against S5: shown larger by more than 0.05.
- `RL-floor`: `rep_drop_edge = rep_credits` (every unit unprotected at birth, §2.3).
- `RL`: `dec_on_time / dec_units ≥ 0.95`.
- `p_dec_meas` in [0.70, 0.88] against `p_fec_law` ≥ 0.99. The law overstates the decode probability by 0.11 to 0.29: a defect finding (§11 item 1).
- Datagrams per handed-over unit = 1 + r ± 0.01.

**At the call point on the same path** (T = 0.4):
- S5 and RL `share_by_0p4` ≥ 0.998, equivalent.
- Resends per delivered unit:
  - S5 in [0.10, 0.12];
  - RL within ±0.01 of S5. Repair saves no resends here: the first report lists the hole about 20 ms before the first covering repair can arrive (the span trails the frontier by Δ + 1 sources), and the sender resends on that report.
- `dup_after_dec / dec_units` in [0.80, 0.95], about `1 − pc`. A finding about the race, not a pass condition.

**Q3e-5, OVs: sustained overload** (`share_by_0p4`; delay p50 from hand-over):

| arm | `share_by_0p4` | from (§2.6, with the ramp) | delay p50 | expired at the intake, windows after 3 s |
|---|---|---|---|---|
| `S5` | [0.81, 0.86] | 0.842 (stage 3d at full rate: 0.838) | [0.37, 0.40] s | 1 − C/λ ≈ 0.17 ± 0.02 |
| `RL` | [0.74, 0.80] | 0.768 (r ≈ 0.118, μ_s ≈ 895) | [0.37, 0.40] s | ≈ 0.25 ± 0.03 |
| `RS` | [0.67, 0.74] | 0.706 (μ_s = 813) | [0.37, 0.40] s | ≈ 0.32 ± 0.03 |
| `RF3` | [0.61, 0.68] | 0.642 (μ_s = 735) | [0.37, 0.40] s | ≈ 0.39 ± 0.03 |
| `RL-age` | `RL` − 0.03 to `RL` + 0.005 | | [0.37, 0.40] s | as RL |
| `RL-floor` | `S5` − 0.02 to `S5` + 0.005 | | [0.37, 0.40] s | as S5 |

- **Every repair arm under the worth edge is shown smaller than S5.** RL by 0.04 to 0.10, RS by 0.10 to 0.17, RF3 by 0.16 to 0.23 (the displacement of §2.6).
- **The ramp formula used above.** With `t_r = λ(T − R)/(λ − μ_s)`, the share is `[λ·t_r·0.995 + μ_s·(D − t_r)·(1 − ε(1 − P_dec,T))]/(λ·D)`, where D is the 10 s run. For S5, `P_dec,T = 0`; for the repair arms, Appendix B at a span of about 8 members.
- `RL-age`:
  - `share_any` ≥ RL's;
  - `rep_cut_edge` ≤ 0.1, against RL's ≥ 0.9.
- `RL-floor`: `rep_drop_edge / rep_credits ≥ 0.85` (repair stops once the backlog binds, within about 1 s).
- The worth-edge arms: `rep_drop_edge / rep_credits ≤ 0.05`.
- Link busy over [1, 10] s ≥ 0.98 for every arm (validity, as stage 3d).

**Q3e-6, CLs and CLs-long at the call point:**

| | CLs `share_by_0p4` | CLs-long `share_by_0p4` | recovery at 0.4 s | delay recovery at 0.15 s (CLs-long) | delay p50, CLs-long |
|---|---|---|---|---|---|
| `S5` | [0.82, 0.87] (3d: 0.847) | [0.90, 0.94] (3d: 0.923) | [0, 0.3] s | [0.3, 1.6] s | [4, 9] ms |
| `RL` | S5 ± 0.02 | S5 ± 0.02 | [0, 0.3] s | [0.5, 3.0] s | [4, 15] ms |
| `RS` | S5 ± 0.02 | S5 ± 0.02 | [0, 0.3] s | ≥ 6 s or not within the run | [0.25, 0.40] s |
| `RF3` | S5 − 0.08 to S5 − 0.02 | S5 − 0.10 to S5 − 0.04 | not within the run | not within the run | [0.37, 0.40] s |
| `RL-floor` | — | S5 ± 0.02 | [0, 0.3] s | [0.5, 3.0] s | [4, 15] ms |

- **CLs (10 s):** the delay recovery at 0.15 s for RS and RF3 is "not within the run". The readable end is 4.5 s after the outage, against about 15 s predicted for RS.
- **RF3's on-time recovery is not within the run** because its pre-outage level is about 1.0 while its post-outage level is `μ_s/λ ≈ 0.92`. The link is overloaded by repair alone (`λ(1 + 0.36) = 1.09·C`), and the dequeue rule sheds about 8 % once the backlog binds. That backlog binds about 3.7 s after the start, so after the outage.
- **Link busy before the outage** ([0.5, 1.9] s):
  - S5 in [0.80, 0.85] (`λ(1 + ρ)/C`);
  - RL and RS ≥ 0.97 (`λ(1 + 0.2295 + ρ)/C`; the law's repair fills the link);
  - RF3 ≥ 0.98.
- **`rep_link_share` in saturated windows** = `r/(1 + r + ρ)` ± 0.01, from the run's own measured r and ρ. Repair load is tied to first sends.
- **`queue_drop_out`:** 0 for S5, R0, RL and RS. For RF3, at most 0.002 of the datagrams; the bound of §2.5 is 122 against Q = 125.

**Q3e-7, CLs-long with no lifetime** (T = ∞, H = 0; recovery at the 0.4 s readout):

| arm | a-priori μ_eff | brief's form t_rec | §13.6 form t_rec' | registered band |
|---|---|---|---|---|
| `S5-inf` | 1 250/1.033 = 1 210 | 4.84 s | 5.84 s | [4.6, 6.6] s (3d at full rate, blocker: 5.67 s) |
| `RL-inf` | 1 250/1.110 ≈ 1 126 (r ≈ 0.107 in the drain) | 8.3 s | 9.9 s | [7.0, 12.5] s |
| `RS-inf` | 1 250/1.2295 ≈ 1 017 | 66 s | — | not within the run |

- **On the run's own measured `mu_eff_hat`,** the residual `t_meas − t_rec` lies in [−0.3, +2.0] s and `t_meas − t_rec'` in [−1.0, +1.0] s, for S5-inf and RL-inf in every seed. This is the law check §19 asked for, with μ_eff reduced by the repair share.
- All three arms finish: after the source ends at 20 s the link drains the rest at μ_eff.

**Q3e-8, is repair overload self-limiting?**
1. In every finite-T arm, delivered units have delay p99 ≤ T. The intake never exceeds `λ·T`.
2. Under the worth edge the repair load does not fall: `rep_drop_edge/credits ≤ 0.05` and `rep_link_share` holds its `r/(1+r+ρ)` (Q3e-5, Q3e-6).
3. Under the floor edge it does: Q3e-5's `RL-floor` rows. The same edge costs RT all of its repair (Q3e-4).
4. The live law lowers r by about half once a queue forms (Q3e-2). RL drains in about the time of a 0.11 arm, while RS does not drain within the run (Q3e-6, Q3e-7).
5. With no lifetime there is no bound: RS-inf does not recover within the run.

**Q3e-9, other gauges, reported:**
- `rep_used`, `rep_redundant`, `dec_units`, `dec_after_release` and `late_floor_dec` per cell;
- `dec_comp_max` (expected ≤ 20);
- the brake: `resend_braked = 0` in the scaled cells (the brake exceeds the link);
- the A/A self-check as stage 3d.

## 9. Run order and stop rules

Each step writes a marker in `out3e/` when it passes. `batch3e.py` refuses to start a part whose markers are missing, and refuses everything while `out3e/STOP` exists.

0. **Build.** No runs.
1. **Tests.** Red on the stage 3d code, then green. `python -m unittest test_toy test_toy2`.
2. **`check_ident3e.py`**, alone on a quiet machine. PASS writes `PASS-ident`.
3. **Part ID:**
   - R0 against S5 in the stage 3d cells (3 seeds) and in the new cells (12 seeds);
   - every run of the part `finished`.
   - PASS writes `PASS-r0`.
4. **Part RATE:** test 3 against `rate_ref3e.json`. PASS writes `PASS-rate`.
5. **Part AA:** S5 variant 1 in the four cells; freeze `nf_stage3e.json`. Writes `PASS-aa`.
6. **Part MAIN:** the arms of §5. CLs-long with 1 worker, the others with at most 2.

**Stop rules. They are mechanical, and the worker does not continue past one.**
- In steps 2 to 4, any difference, any unfinished run or any rate mismatch: write `STOP` with the cause and return to the coordinator.
- During MAIN, after each run, write `STOP` and end the batch if:
  - `finished` is false;
  - `l1_viol_rep > 0`;
  - `commit_viol > 0`;
  - `worth_viol_rep > 0` in a worth-edge or floor-edge arm.
- After a STOP: no supplement run, no code change and no rerun without a registration amendment. Stage 3d's worker ran past a liveness failure. This rule exists for that reason.
- A run slower than 15 s is reported, not treated as a stop.

Report every reading, the A/A self-check, every deviation and the time per run, as stage 3d did.

**Optional, informative, not a gate.** Read the interactive simulator's gauges at its CL-long preset (p̂ upper, W, r at the end of the run). This tests whether its r ≈ 0.36 is the saturation knee at W = 16 (r_sat = 0.360 there) with p̂ ≈ 0.06 (§11 item 6).

## 10. Owner questions

Numbering continues from the derivation's Q18.

- **Q19, the span's lower edge under a lifetime.**
  - **Default: the worth edge `a + ô < T`** (§2.3), L2 applied to a correction symbol.
  - Alternatives: the age edge `a < T` (the simulator's; carries members that can only be decoded late) and the floor edge `lo ≥ F_ann` (limits overload, but switches repair off below one resend round).
  - All three are run.
- **Q20, should the floor count repair?**
  - **Default: no.** `protected` stays the resend trigger.
  - The alternative would hold the floor at about `T − ô`, and strict order would wait for it.
  - The default's cost is out-of-order late fills at H = ∞ (`late_floor_dec`).
- **Q21, ε̂ in the offline simulation's rate law.**
  - **Default: the cell's true first-send loss,** a declared idealisation. The law arm is then a lower bound on the engine's r.
  - The alternative is a port of the BOCD estimator (`raptorpath-math/src/estimator.rs`).
- **Q22, the engine's spare-capacity clamp** `min(r, (cwnd − in_flight)/in_flight)`.
  - **Default: not modelled.** The offline simulation has no cwnd, and its B is a store cap, not a congestion window.
  - Direction: the clamp can only lower r, so the results bound the engine's repair load from above.
  - If it is ever modelled, it needs a bind gauge.
- **Q23, the rate scale.**
  - **Default: one tenth,** with the window floor binding (r = 0.2295 against 0.1123 at 10 000/s). The arms RF1 and RS bracket the difference.
  - The alternative is the call point at full rate, at minutes per run.
- **Settled here, listed for review:**
  - a repair's loss is drawn at p0;
  - `rate_hat` is a windowed mean, not a windowed maximum;
  - the decoder is generic-rank;
  - the link cells use only the blocker sweep;
  - the repair draws its own `SALT_REPAIR`.

## 11. What looked wrong in the existing laws

1. **The rate law and the span law size different windows.**
   - The rate law sets r for a window W of about `srtt·rate` (latency budget 0), with W ≥ 16.
   - At δ = 50 the span law codes each repair over `A* = rate·b·srtt` with b = ½, about half of W.
   - At RT: `P_fec(r, W = 16) = 0.997` against about 0.80 at A* = 3 (Appendix B). At CLs: 0.99998 against 0.935 at A* = 6.
   - So the law promises a tail it does not buy. Q3e-4 measures the gap.
2. **W's floor of 16 sets r at call rates.**
   - Wherever `srtt·rate < 16` (a call at 50 units/s on any path below 320 ms, or 1 000 units/s below 16 ms), r is a function of ε alone through W = 16, not of the latency budget the window law names.
   - A clamp that binds over the operating range is a defect finding (FORMULA-FIRST).
3. **The saturation knee sets r in RT** (raw 0.51, knee 0.385, pressure about 0.96). The constants in the knee's model (c = 0.5, 1.5·srtt) are themselves unprovenanced (paper §4.4).
4. **Read literally, the paper's span edge contradicts "only repair helps".**
   - Paper §5.3's "F the oldest unresolved position", with the floor of Revision 2 substituted, would start every span at or above the protected floor.
   - Below one resend round every unit is unprotected at birth, so no repair would be sent: the opposite of §2.2's and §5's claim.
   - §2.4's T − ô cut is the consistent reading.
5. **The engine's empty-span fallback.** When the trailing span is empty, `emit_source.rs:839–846` falls back to `generate_repair()` over the leading window. Under a lifetime that fallback must respect the same edge, or it can code members past T. The L1 absolute test would catch it in the engine build.
6. **The simulator's r ≈ 0.36 is not reproduced by the closed form at ε = 0.03.**
   - The closed form gives 0.2295 at W = 16 and 0.158 at the simulator's CL-long operating point.
   - The knee at W = 16 is exactly 0.360, and the law reaches it only at a p̂ of about 0.06.
   - So the simulator's value is probably its BOCD upper quantile driving the raw rate into the knee. Unreconciled; the optional step in §9 reads it.
7. **The live law responds to a queue by lowering repair,** because the queue's delay enters W through `W_lat = srtt·rate`. That is a latency budget read from a delay that includes queueing. It limits overload here only because the offline simulation's only queue sits behind the store pause. Whether the engine sees it depends on its congestion controller.
8. **b's clamp at ½ binds for every δ ≥ 50,** so the Realtime point sits on the edge of b's range, and any δ above it sees a flat b. Continuous, but the span's response to δ above Realtime is nil.

## 12. Review

The advisor (Fable) reviewed the approach before writing. That review changed five things:
- the law arm reads r live from the measured srtt and send rate, as `rate_snapshot` does, instead of once per cell;
- ε̂ is pinned with its direction stated (Q21);
- the spare-capacity clamp became Q22;
- the age edge became an arm beside the floor edge;
- the T = ∞ arms were added, because the brief's recovery formula binds only for a sender that never gives up.

It also asked for the reference values from the real `raptorpath-math` functions (Appendix A), the mechanical stop markers (§9) and the queue margin (§2.5).

## Appendix A: rate-law reference program

A scratch crate outside the repository, with `raptorpath-math` as a path dependency. It produced §2.1's table.

```rust
use raptorpath_math as m;
fn r(p: f64, srtt: f64, rate: f64, delta: f64) -> (f64, f64, f64, f64) {
    let t_tail = (1e-5 * m::zeta_of_delta(delta)).clamp(1e-9, 0.1);
    let w = m::derive_window(t_tail, p, 1.0, srtt, rate, 0.0).round().min(200.0);
    let mk = |tail: f64, late: bool| m::RateInputs {
        p_upper: p, sigma2: 1.0, mean_burst: 1.0, mass: m::MassStats::default(), tail_provision: true,
        window: w, t_symbols: (srtt * rate).max(1.0), srtt, t_sym: 1.0 / rate, codec_overhead: 0.004,
        tail_target: tail, bulk_late_is_fine: late, completion_exposure: 0.0, inner_feedback: 0.0,
        saturation_cap: true, max_overhead: 0.5,
    };
    let beta = m::bulkness_of_delta(delta);
    let a = m::controller_rate(&mk(t_tail, false));
    let l = m::controller_rate(&mk((1e-5f64 * 100.0).clamp(1e-9, 0.1), true));
    (w, (1.0 - beta) * a + beta * l, m::r_saturation(p, 1.0, w, srtt, 1.0 / rate), m::span_horizon_b(delta))
}
```

Output:
- (0.03, 0.010, 1 000) → W 16, r 0.22952;
- (0.03, 0.010, 10 000) → W 100, r 0.11234;
- (0.03, 0.100, 1 129) → W 113, r 0.10744;
- (0.03, 0.098, 895) → W 88, r 0.11763;
- (0.10, 0.100, 50) → W 16, r 0.38355;
- at RT, δ ∈ {49, 49.5, 50, 50.5, 51} → r 0.383536 to 0.383561, b 0.50152 to 0.5.

## Appendix B: the decode estimate behind the predictions

A Monte Carlo of the coding structure alone. It is not the offline simulation. Assumptions:
- i.i.d. loss on sources and repairs at ε;
- `owed += r` per source, and one repair per credit over the trailing span `[s + 1 − Δ − A, s + 1 − Δ)` with Δ = 1;
- the generic-rank decode of §2.7;
- no resends.

| ε | span A | r | P_dec | P_dec within 3 sources (RT's deadline) |
|---|---|---|---|---|
| 0.10 | 3 | 0.3835 | 0.801 | 0.783 |
| 0.10 | 3 | 0.36 | 0.765 | 0.757 |
| 0.10 | 3 | 0.11 | 0.243 | 0.243 |
| 0.03 | 6 | 0.23 | 0.935 | — |
| 0.03 | 6 | 0.11 | 0.549 | — |
| 0.03 | 8 | 0.13 | 0.788 | — |
| 0.03 | 9 | 0.23 | 0.992 | — |
| 0.03 | 16 | 0.11 | 0.914 | — |

200 000 sources per row, seed 1.

```python
import random
def determined(unknowns, eqs):
    U = sorted(unknowns); adj = {u: [j for j, e in enumerate(eqs) if u in e] for u in U}
    meq, mu = {}, {}
    def aug(u, seen):
        for j in adj[u]:
            if j in seen: continue
            seen.add(j)
            if j not in meq or aug(meq[j], seen):
                meq[j] = u; mu[u] = j; return True
        return False
    for u in U: aug(u, set())
    bad = set(u for u in U if u not in mu); st = list(bad)
    while st:
        u = st.pop()
        for j in adj[u]:
            v = meq.get(j)
            if v is not None and v not in bad: bad.add(v); st.append(v)
    return set(U) - bad
def run(eps, A, r, L, n=200000, delta=1, seed=1):
    rnd = random.Random(seed); owed = 0.0; eqs = []; unk = set(); rec = ok = nl = 0
    for s in range(n):
        if rnd.random() < eps: unk.add(s); nl += 1
        owed += r
        if owed >= 1:
            owed -= 1; end = s + 1 - delta; lo = max(0, end - A)
            if end > lo and rnd.random() >= eps:
                e = set(x for x in range(lo, end) if x in unk)
                if e:
                    eqs.append(e)
                    dec = determined(set().union(*eqs), eqs) if len(eqs) < 60 else set()
                    for x in dec: unk.discard(x); rec += 1; ok += (s - x) <= L
                    if dec: eqs = [q - dec for q in eqs]; eqs = [q for q in eqs if q]
        if s % 200 == 0: eqs = [q for q in eqs if max(q) > s - 50]
    return rec / nl, ok / nl
```
