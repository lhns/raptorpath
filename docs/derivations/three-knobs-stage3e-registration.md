# Stage 3e registration: proactive repair in the three-knob model (no runs)

**Status.** Registration only, written before any run. Nothing here has been run in the offline simulation. Reviewed with the advisor (Fable) before it was written and again after the first push (`d500454b`); it then carries the coordinator's review of `f50401f8` (full-rate cells, the estimated-loss arm, the power cell, the RTT gate; §12). Still no run.

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
  - The alternative, "the oldest protected source", switches repair off exactly where it is the only recovery. It is registered as an ablation arm and as owner question Q19.
- **The floor, the resend budget, the store pause and the dequeue rule are unchanged** (§2.4–§2.6).
  - Repair is paced by first sends (`owed += r` per first send, at most one repair per source), takes no store slot, is not charged to the resend budget, and carries the floor.
  - What changes is the link. Fresh data drains at `μ_eff = C / (1 + r + ρ)`.
- **Is repair overload self-limiting?** In delay and memory, yes, through L1 and the dequeue rule. In share, no: the worth edge never suspends repair, and under sustained overload any r above `ε/(1 − ε)` lowers the on-time share (§2.6).
  - **The load itself falls only under Q21's true-loss default.** There a queue raises srtt, W grows to its cap, and r falls.
  - With the engine's estimated loss (arm `RE`), an outage or queue drops raise ε̂, and so r, at the moment the link has a backlog to drain. That is the opposite feedback, and RE measures it.
  - The floor-edge ablation also limits the load.
- **Cells.**
  - Primary cells at stage 3d's full rate: CL, CL-long and OV, comparable with stage 3d's stored rows, plus CLw-long, a 3 s outage at a load where every arm has room for repair.
  - RT: K-L10 without its outage, at T = 0.12 s, where only repair can recover.
  - One one-tenth cell (CLs-long), labelled clamp-bound: W's floor sets r there.
- **Arms.**
  - S5 without repair; R0 (r = 0, the identity);
  - the live law with true loss (RL) and with estimated loss (RE);
  - the law fixed at its unqueued value (RS); r = 0.36, the simulator's reading (RF3); r = 0.112 (RF1);
  - the edge ablations;
  - T = ∞ arms for the recovery formula.
- **Main predictions** (§8).
  - **RT:** the on-time share rises from 0.90 to 0.978. That is `0.90 + 0.10 · 0.783`, with 0.783 the span's decode probability within the deadline; the rate law's own promise, 0.997, would give 0.9997.
  - **OV:** every repair arm loses on-time share to displacement (0.84 → 0.79 with the live law, 0.79 fixed, 0.64 at r = 0.36).
  - **CLw-long, never giving up:** recovery after the outage goes from 4.7–5.2 s to 5.6–6.8 s with the live law, with r = 0.36 not recovering within the run.
  - **CL-long at the call point:** the on-time share recovers within resolution, and the delay recovers in about 1–2 s in every arm except r = 0.36. At full rate the law leaves headroom.
- **Things in the existing laws that look wrong** are in §11.
  - At call rates the rate law sizes r for a window W about twice the span A* that repairs are coded over, so it promises P_fec ≈ 0.997 where the span delivers about 0.80.
  - W's floor of 16 sets r at call rates, and its cap of 200 sets r once a full-rate link queues.
  - Stage 3d's RTT sample can come from a seq that never arrived. It is now gated (§9).

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
   end  =  n_sent − Δ ,     ws = max( 0 , A_known − W )
   span =  [ max( ws , end − A* , e ) ,  end )                  e: the lifetime edge, §2.3
```

**Emission (paper §3.3, `TaperBudget`; §5.3, "repair grant ≤ 1 per source"):**

```text
   at every first send:   owed += r ;   if owed ≥ 1:  owed −= 1, stage one repair over the span
   cadence:               r and W are re-read once min(5 ms, srtt/4) has passed since the last reading      (RepairRateCache::period_us)
```

| symbol | provenance |
|---|---|
| ε̂ | **declared idealisation:** the cell's first-send loss `p0`. The engine reads the BOCD 95 % predictive upper quantile (`predictive_loss_upper(0.95)`), which is at least the mean. So the law arm's r is a **lower bound** on the engine's r for the same channel (owner question Q21). **Arm `RE` uses an estimate instead:** the engine's 95 % upper-quantile form `LossEstimator::loss_rate_upper(0.95)` = `beta_quantile(b, a, 0.95)` (normal approximation, `estimator.rs:505`), with `a, b` starting at 1 and updated per report as `a ← 0.995·a + received`, `b ← 0.995·b + lost` (`estimator.rs:279`, `:353–357`). A report's batch is the first sends whose fate it newly reveals: newly acknowledged, or newly listed missing, which includes queue drops and outage losses. Dequeue expiries are never sent, so they never enter the estimate. The BOCD predictive quantile that the engine reads after 5 updates is not ported; the Beta form is its own pre-BOCD fallback (declared, Q21) |
| σ², B̂ | 1 and 1: every cell is i.i.d. These are the GE values of an i.i.d. channel and also the engine's fallback when the GE estimate is invalid (`rate_snapshot`) |
| t_symbols, t_sym | `srtt·rate_hat` (at least 1) and `1/rate_hat`, as `rate_snapshot` forms them from throughput |
| ε_codec | 0.004, RLC (paper §3.6) |
| r_max | 0.5, the engine's `max_overhead` |
| the mass term | inert: no measured mass statistics (`MassStats::default()`), as at the engine's cold start |
| srtt | measured, RFC 6298, with the 333 ms seed before the first sample (stage 3b onwards). The engine's D reads the estimator's smoothed RTT, not RTprop, a divergence the paper already records (§5.3, §11.3). The offline simulation follows the engine |
| `rate_hat` | first sends in `(now − 8·srtt, now]` divided by `min(8·srtt, now − first send)`. The window length is paper §5.3's ("window ≈ 8 SRTT"). The engine takes a windowed **maximum** of srtt/2 buckets; at 50 units/s a bucket holds 2 or 3 sends, so the maximum reads 60/s and moves A* from 3 to 4. The mean is chosen and declared. Before the first RTT sample A* = 1, as the engine's cold start (`emit_source.rs:607`) |
| J | the cell's delay-process amplitude, 1 ms (declared). The engine measures jitter |
| ws | the encoder window's start. Outside generation mode the engine keeps W seqs behind the cumulative ack (`sender_phases.rs:1370–1373`, `advance(ack − clamp(W, 16, win_cap))`; `rlc_window.rs::window_span`). The offline simulation reads the acknowledged frontier as `A_known`, and any capacity bound of the encoder itself is not modelled (declared). Since `A* ≤ W`, ws rarely binds |
| b(δ), δ = 50 | paper §5.4. b's lower clamp ½ binds for every δ ≥ 50, so the call point sits on its edge |
| the cadence | `RATE_CADENCE_MAX_US = 5 ms` and srtt/4 (`fec_rate.rs:181`, `:234`). A reading is taken at the first first-send after the period has passed, so it schedules no event of its own |

**The reference values**, from the real `raptorpath-math` functions (Appendix A; a Python port agrees to 5 decimals):

| operating point | ε̂ | srtt | rate_hat | W | r | r_sat |
|---|---|---|---|---|---|---|
| CL, CL-long unqueued (full rate) | 0.03 | 0.0115 | 10 000 | 115 | 0.10687 | 0.135 |
| CLw-long unqueued | 0.03 | 0.0115 | 8 000 | 92 | 0.11566 | 0.150 |
| any full-rate cell queued behind the pause (CL drain, OV) | 0.03 | 0.098–0.100 | 9 000–11 250 | 200 (cap binds) | 0.08754 | 0.100 |
| CLs-long, one tenth, unqueued (clamp-bound) | 0.03 | 0.0115 | 1 000 | 16 (floor binds) | 0.22952 | 0.360 |
| RT | 0.10 | 0.101 | 50 | 16 (floor binds) | 0.38355 | 0.385 (binds: raw 0.5105) |
| `RE`, CL steady (ε̂ ≈ 0.0344) | 0.0344 | 0.0115 | 10 000 | 115 | 0.11566 | 0.135 |
| `RE`, CL drain after the outage (ε̂ 0.05 / 0.1 / 0.3) | 0.05–0.3 | 0.100 | 11 250 | 200 | 0.116 / 0.195 / 0.500 | 0.120 / 0.200 / 0.610 |
| `RE`, OV steady | 0.0344 | 0.098 | 9 000 | 200 | 0.09313 | 0.100 |
| `RE`, CLw-long steady | 0.0344 | 0.0115 | 8 000 | 92 | 0.12543 | 0.150 |
| `RE`, RT (ε̂ ≈ 0.135) | 0.135 | 0.101 | 50 | 16 | 0.42470 | 0.425 |

**Where ε̂ sits for `RE`.** In steady state ε̂ ≈ ε + 1.645·√(ε(1 − ε)/n_eff), with `n_eff = (sends per batch)/(1 − 0.995)`.
- At full rate a 2 ms report batch holds about 20 sends, so n_eff ≈ 4 000 and ε̂ ≈ 0.034.
- At 50/s a batch holds about 1 send, so n_eff ≈ 200 and ε̂ ≈ 0.135.
- After an outage the first reports reveal the whole lost store (B = 1 000 seqs) at once. ε̂ then jumps to about 0.2 and decays over about 200 batches (0.4 s).

**Which clamps set r.**
- At full rate the clamps do not set r while there is no queue. Queued, W's 200 cap binds and the knee binds partly.
- Call rates are clamp-bound by construction. In RT, W's floor of 16 binds and the knee binds. In the kept one-tenth cell (CLs-long), W's floor binds.
- Every one of these clamps is a bind gauge with an expected fraction (§6, Q3e-2).

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
  - It does limit overload, though. In OV every unit is sent unprotected after the first second, so repair stops there too (§2.6).

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
  - The link queue then holds about `(B − λ_s·RTprop_eff)·(1 + r)` datagrams behind a pause. At full rate (B = 1 000, Q = 1 250, `λ·RTprop_eff` ≈ 120) that is 974 at r = 0.107 and 1 197 at r = 0.36, within 53 of Q (Q3e-6). In the one-tenth cell it is 111 and 122 against Q = 125.
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
1. **The worth edge: no.** It shortens spans (in OV to about `μ_s·(R − ô) ≈ 80` members, against A* = W = 200) but almost never empties them, so repair keeps its share of the link.
2. **The floor edge: yes** (arm `RL-floor`). Once units are sent unprotected, spans are empty and repair stops. In OV that happens within the first second. After an outage in CL-long, repair stops until the intake wait falls below `T_p = T − d − ô ≈ 0.2 s`.
3. **The live rate law: partly, and only under Q21's true-loss default** (arm `RL` against the fixed arm `RS`). A queue raises srtt, so `W_lat = srtt·rate_hat` grows, W grows (to its 200 cap) and the margin term shrinks as `W^(−½)`. At full rate r falls from 0.107 to 0.088; in the one-tenth cell, from 0.23 to 0.11.
   - **This feedback rests on a queue that the offline simulation builds only behind its store pause** (B/C = 80 ms), with no congestion control.
   - A delay-based controller holding its queue near its target would keep srtt, and so r, near the unqueued value. `RS` (r fixed at the unqueued law value) stands for that case.
   - **With an estimated loss the feedback runs the other way** (arm `RE`). An outage, or queue drops, shows up as a burst of first sends newly listed missing. That raises ε̂, and through it r, at exactly the moment the link has a backlog to drain: in the CL drain, ε̂ ≈ 0.1 gives r ≈ 0.20, and ε̂ ≈ 0.3 gives the 0.5 cap. Dequeue expiries never enter the estimate, because they are never sent. So "the live law limits its own overload" holds only for the true-loss input; whether it survives the engine's estimator is what `RE` reads.
   - The engine's spare-capacity clamp, `min(r, (cwnd − in_flight)/in_flight)` (`emit_source.rs:578`), is a fourth limit. The offline simulation has no cwnd, so it is not modelled (Q22). It can only lower r, so the offline simulation's repair load is an upper bound on what the clamp would allow.

**With headroom** (`λ(1 + r + ρ) < C`) repair costs no share but shrinks the drain after an outage.
- The time until fresh data is on time again, for a sender that never gives up (the brief's form, §19's law):

```text
   t_rec  =  ( L·λ/μ_eff − τ ) / ( 1 − λ/μ_eff )                       L = outage, τ = 0.4 s (the readout's reference)
   t_rec' =  ( (L·λ + B_lost)/μ_eff − (τ − d_fwd) ) / ( 1 − λ/μ_eff )   with §13.6's lost store and forward delay
```

- **At the call point the backlog does not live that long.** At T = 0.4 the dequeue rule keeps only the last `T − R` of hand-overs, so `L` is replaced by `min(L, T − R)`.
  - The on-time share then recovers within the readout's resolution, as in stage 3d (0.10 s in every T = 0.4 arm).
  - The **delay** recovers only when that backlog has drained, after `λ·(T − R)/(μ_eff − λ)`. In the one-tenth cell with r fixed at 0.2295, `μ_eff − λ ≈ 16/s`, so the delay stays near T for the rest of the run. At full rate the same arm drains at about 1 200/s.
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
  - `r`: `"law"` (live, true loss, §2.1), `"est"` (live, with ε̂ from the Beta upper quantile of §2.1, updated in `on_report`), or a float (fixed);
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
4. **RTT sampling:** the receiver keeps `rhi_smp`, which follows every raise of `rhi` exactly as stage 3d makes them (data arrivals in `on_data`, and floor passes in `apply_floor`, which sets `rhi = f − 1` with no arrival) **except raises by a decode**.
   - Reports carry `rhi_smp`, appended at the end of the tuple after the per-path field, and the sender takes its RFC 6298 sample from `rhi_smp` where stage 3d took it from `rhi`.
   - With no decodes `rhi_smp == rhi` at every report, so stage 3d is unchanged, including its sampling of floor-passed seqs (§11 item 9).

**Receiver:**
5. **`on_repair(now, j, lo, end, tau)` (event kind 11):**
   - `tau_rx_recv = max(tau_rx_recv, tau)`: RACK counts every datagram;
   - `U = {s in [lo, end) : not got[s]}`. If U is empty, count `rep_redundant` and return;
   - otherwise count `rep_innov`, add the equation, and run `decode(now)` on its connected component.
6. **`decode(now)` on a component** (unknowns linked by shared equations):
   - take a maximum matching (augmenting paths);
   - the determined set is every unknown not reached by an alternating path (unknown → any equation containing it → that equation's matched unknown) from an unmatched unknown;
   - deliver each determined s in increasing seq order through the data path with copy index −1. That path: no `tau` update, no `est.feed`, no `rhi_smp` change, opens holes up to s and raises `rhi` if `s > rhi`, as `on_data` does;
   - count `dec_units`, `dec_on_time` (`now − h_seq[s] ≤ T`), `dec_after_release` (s had been released) and `late_floor_dec` (s had been passed by the floor);
   - count every equation matched to a decoded unknown as `rep_used`;
   - remove decoded unknowns from every equation and drop empty equations.
7. **A source or copy arrival of a seq that sits in a pending equation** removes it, and `decode` runs again on that component (the cascade). A source or copy arriving for a decoded seq counts as `dup` and also as `dup_after_dec`.
8. **Memory:** equations are dropped only when empty. Gauge `dec_comp_max`, the largest component decoded.

**What must not change at r = 0:** no event is pushed, no draw is taken, `ctr` and `ps_ctr` do not advance, and the rate-law reading schedules nothing.

## 4. Cells

**The primary cells run at stage 3d's full rate.**
- A T sweep at that rate took about 3 s per run.
- At full rate the rate law is not set by W's floor.
- R0 and S5 can be compared with stage 3d's stored results, because the configurations are stage 3d's own cells under the engine-like sweep.

Every link cell uses the engine-like sweep (`sweep_form = "blocker"`, `rx_refresh`). Stage 3d found that the T = 0.4 rows move by under 1 point between sweep forms, and the blocker form is the engine's.

| cell | loss | owd | rate λ | link C, Q | B | units | outage | sweep | t_limit | stage 3d counterpart |
|---|---|---|---|---|---|---|---|---|---|---|
| **CL** | I2 (p0 3 %, pc 10 %) | 5 ms | 10 000/s | 12 500/s, 1 250 | 1 000 | 100 000 (10 s) | 2.0 s to 3.5 s | blocker | 40 s | `CL@b` |
| **CL-long** | I2 | 5 ms | 10 000/s | 12 500/s, 1 250 | 1 000 | 200 000 (20 s) | 2.0 s to 3.5 s | blocker | 80 s | `CL-long@b` |
| **OV** | I2 | 5 ms | 12 000/s | 10 000/s, 1 250 | 1 000 | 120 000 (10 s) | none | blocker | 40 s | `OV@b` |
| **CLw-long** (new: room for repair) | I2 | 5 ms | 8 000/s | 12 500/s, 1 250 | 1 000 | 160 000 (20 s) | 2.0 s to 5.0 s | blocker | 80 s | none |
| **RT** | L10 (10 %, 10 %) | 50 ms | 50/s | none | default | 3 000 (60 s) | none | tail | 80 s | K-L10 without its outage |
| **CLs-long** (kept, clamp-bound) | I2 | 5 ms | 1 000/s | 1 250/s, 125 | 100 | 20 000 (20 s) | 2.0 s to 3.5 s | blocker | 80 s | none |

- **The link cells' other settings** are stage 3c's `LINK_KW`: work-conserving source, RFC 6298 RTT, the engine's cooldown gate, the engine's brake, J = 1 ms. `windows = True`.
- **CLw-long gives repair room.** The registered call-point test is `(1 + r)·λ < μ·(1 − ε)` with margin, with μ = C.
  - `μ(1 − ε) = 12 125`.
  - The law's r = 0.116 needs 8 925 (26 % margin), and r = 0.36 needs 10 880 (10 % margin).
  - So every arm can recover there, and the recovery formula has power. In CL-long at a load of 0.8, r = 0.36 overloads the link by arithmetic (`1.36·10 000 > 12 500·0.97`), and before this revision so did the law's r at one tenth.
  - The 3 s outage makes the backlog `λ·L = 24 000` units, large against the store pause (B = 1 000).
- **RT** is K-L10 without its outage. Repair should help there:
  - the loss is independent;
  - the lifetime is 1.2 RTT;
  - no resend can arrive in time: the first report-driven resend would arrive at about 0.17 s, and the sweep's at `0.1 + ô ≥ 0.15` s, both past 0.12 s. So L2 refuses every resend and the share without repair is `1 − p0`.
  - At 50 units/s, `W_lat = 5`, so W sits on its floor of 16. Call rates are clamp-bound by construction, and the readings say so.
- **CLs-long is kept as the one scaled cell, labelled clamp-bound.**
  - It is CL-long with λ, C, Q, B and the units divided by 10.
  - W's floor binds on every arm there (r = 0.2295 against 0.107 at full rate), and it has no stage 3d counterpart.
  - It reads what the law does when its window clamp sets r. None of its readings is evidence about the full-rate law.
- **Run time:**
  - The worker times the first run of each cell and reports.
  - A cell above 30 s per run is reported before the batch continues; it is not rescaled silently.
- **Memory** (stage 3d's first batch died of a `MemoryError` with 15 workers):
  - at most 2 workers;
  - the long cells with 1 worker;
  - `check_ident3e.py` alone, with no batch running.

## 5. Arms

All arms: δ = 50, H = 0. T = 0.4 s unless stated; in RT, T = 0.12 s unless stated. 12 seeds, phases per seed. Each arm's r is given by the table of §2.1.

| arm | sender | r | edge | cells |
|---|---|---|---|---|
| `S5` | stage 3d's S5, `repair = None` | — | — | all |
| `R0` | S5 + repair | 0 | worth | all (identity with `S5`, and with stage 3d's stored rows where they exist, §9) |
| `RL` | S5 + repair | the live law, true loss | worth | all |
| `RE` | S5 + repair | the live law, estimated loss (Beta 95 % upper quantile, §2.1) | worth | CL-long, OV, CLw-long, RT |
| `RS` | S5 + repair | fixed at the cell's unqueued law value (CL, CL-long 0.107; OV 0.0998; CLs-long 0.2295) | worth | CL, CL-long, OV, CLs-long |
| `RF3` | S5 + repair | 0.36, the simulator's reading at the Realtime δ (§19) | worth | all |
| `RF1` | S5 + repair | 0.112, the law at stage 3d's 10 000/s and RTprop (a low dose) | worth | RT, CLs-long |
| `RL-age` | as RL | live law | age (the simulator's) | OV |
| `RL-floor` | as RL | live law | floor | OV, CL-long, RT |
| `S5-inf`, `RL-inf`, `RS-inf` | as S5, RL, RS at T = ∞ (never gives up, any order) | | | CL-long, CLs-long |
| `RF3-inf`, `RE-inf` | as RF3, RE at T = ∞ | | | CLw-long (with `S5-inf`, `RL-inf`, `RS-inf` at r = 0.116) |
| `S5-T0p4`, `RL-T0p4` | S5 and RL at the call point, T = 0.4 | | | RT |
| `AA` | S5, phase variant 1 | | | all (frozen as `nf_stage3e.json`) |

Per cell:

| cell | configurations |
|---|---|
| CL | S5, R0, RL, RS, RF3 |
| CL-long | S5, R0, RL, RE, RS, RF3, RL-floor, S5-inf, RL-inf, RS-inf |
| OV | S5, R0, RL, RE, RS, RF3, RL-age, RL-floor |
| CLw-long | S5, R0, RL, RE, RF3, S5-inf, RL-inf, RS-inf, RF3-inf, RE-inf |
| RT | S5, R0, RL, RE, RF1, RF3, RL-floor, S5-T0p4, RL-T0p4 |
| CLs-long | S5, R0, RL, RS, RF1, RF3, S5-inf, RL-inf, RS-inf |
| A/A | one per cell |

That is 57 configurations, or 684 runs. RS is not run in RT: there r is constant (srtt does not move), so RS would equal RL.

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
| W's floor 16 | `W_floor_bind`: readings whose pre-clamp W < 16 | RT 1.0; CLs-long (one tenth) ≥ 0.5 of readings (all unqueued time); full-rate cells 0 |
| W's 512 and the 200 cap | `W_cap_bind` | full-rate cells: ≥ 0.9 of readings while queued (OV after 2 s, the CL drain), 0 unqueued; RT and CLs-long 0 |
| r's saturation knee | `sat_pressure`: mean of `saturation_pressure(raw, r_sat)` | RT ≥ 0.9; full rate unqueued (raw 0.110, knee 0.135) 0.1 to 0.25; queued (raw 0.091, knee 0.100) 0.25 to 0.55; CLs-long unqueued ≤ 0.05 |
| r_max = 0.5 | `rmax_bind` | 0 (in RT the knee binds first) |
| the burst term | `burst_bind`: r_burst > r_core | 0 |
| A*'s `[1, W]` | `astar_floor_bind`, `astar_W_bind` | the cold start only (< 0.01); 0 |
| Δ's `[1, 64]` | `delta_bind` | 0 (`⌈rate·J⌉` is 10 at full rate and 1 at call and one-tenth rates, inside the clamp) |
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
   - in the new cells, seeds 0 to 11;
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
10. **RTT sampling** ignores a decoded seq above `rhi_smp`, and still follows a floor raise exactly as stage 3d does (constructed state: a floor passing a lost tail).
11. **The deny test** with the new law functions exempt by name, and a planted positive control.
12. **The loss estimate for `RE`** (`"est"`), on constructed report sequences:
    - matches `LossEstimator::loss_rate_upper(0.95)` (the Beta update with decay 0.995 and the normal-approximation quantile) to 1e-12;
    - a batch that newly lists 1 000 seqs missing raises ε̂ above 0.15 at the full-rate batch size;
    - a dequeue expiry changes nothing.

## 8. Predictions (written before any run)

Seed means over 12 seeds unless stated. "S5 ± x" is a band around the same cell's S5 mean. A paired difference uses the stage 3d reading rule with the A/A floor m from `nf_stage3e.json` ("equivalent" = the paired 95 % interval lies within ±m).

Where a band quotes a stage 3d number, it assumes the RTT-sample measurement of §9 finds stage 3d unaffected. If the baseline is re-run with a fix, those bands are re-read against the re-run. The formula bands stand either way.

**Q3e-1, identity and liveness.**
- `check_ident3e.py` PASS: stage 3d's code reproduced on every key.
- R0 equals S5 on every stage 3d key and trace, in every cell old and new.
- After the RTT gate of §9: S5 and R0 in CL, CL-long and OV equal stage 3d's stored `CL@b`, `CL-long@b` and `OV@b` rows (`G-T0p4-H0`), and S5-inf in CL-long equals `CL-long@b` `G-Tinf-H0`, on every key at seeds 0 to 11.
- Every run of every arm is `finished`.
- FAIL: any difference or any unfinished run.

**Q3e-2, the rate law and its clamps.**
- `rate_law` reproduces §2.1's table and the reference grid.
- `RL`'s measured r:
  - CL and CL-long before the outage: [0.095, 0.115];
  - queued windows (the CL drain, OV after 2 s): [0.08, 0.10], with W at its 200 cap;
  - CLw-long before the outage: [0.105, 0.125];
  - RT: 0.3835 ± 0.002;
  - CLs-long: [0.20, 0.235] unqueued and [0.09, 0.14] in the drain.
- `RE`'s measured r:
  - CL-long before the outage: [0.105, 0.13];
  - in the first 0.5 s after the outage end: ≥ 0.15, and at least 1.5 × RL's r in the same windows. This is the opposite feedback of §2.6;
  - OV: [0.085, 0.10];
  - RT: [0.39, 0.43].
- Bind fractions as the table of §6. These are defect findings by MEASUREMENT DISCIPLINE 17 and 18, each needing a ledger verdict:
  - `W_floor_bind` = 1.0 in RT and ≥ 0.5 in CLs-long;
  - `sat_pressure` ≥ 0.9 in RT;
  - `W_cap_bind` ≥ 0.9 while queued at full rate. The sender's `MAX_WINDOW_SIZE = 200` then sets W, and through it r.

**Q3e-3, L1 and the commitment.**
- `l1_viol_rep = 0` in every registered run.
- `worth_viol_rep = 0` in every worth-edge and floor-edge run, and > 0 in `RL-age` (OV). The age arm is the positive control of the worth gauge.
- `commit_viol = 0` in every run.
- `late_floor − late_floor_dec = 0` in every run (one FIFO path).

**Q3e-4, RT: where only repair recovers** (T = 0.12 s; on-time = `share_T`).

**Which P_fec the prediction uses.** The span's delivery, not the rate law's promise.
- A lost unit is on time only if a repair decodes it before `T − owd − J`. That deadline is 0.069 s, or 3 sources at 50/s, after hand-over.
- So `share_T = (1 − ε) + ε · P_dec,T`, where `P_dec,T` is the fraction of lost first sends decoded within 3 sources.
- The decoding has to come from repairs coded over the engine's span: `A* = ⌈50 · ½ · 0.101⌉ = 3` members, trailing the frontier by Δ = 1.
- Appendix B gives `P_dec,T = 0.783` at r = 0.3835. So `share_T = 0.90 + 0.10 · 0.783 = 0.978`.
- **The rate law's own promise** is `P_fec(r = 0.3835, W = 16, ε = 0.1, σ² = 1) = 0.9973` (paper §4.2's normal form, at the window the law sized r for). That would give `0.90 + 0.10 · 0.9973 = 0.9997`.
- The band below excludes 0.9997. A reading inside the band therefore also confirms §11 item 1, that the law's window is not the span's.

| arm | r | `P_dec,T` (Appendix B) | `share_T` = 0.90 + 0.10·P_dec,T | band | `share_any` |
|---|---|---|---|---|---|
| `S5` | 0 | 0 | 0.900 | [0.885, 0.915] | 0.900 |
| `RF1` | 0.112 | 0.243 | 0.924 | [0.910, 0.940] | ≈ 0.924 |
| `RF3` | 0.36 | 0.757 | 0.976 | [0.958, 0.988] | ≈ 0.977 |
| `RL` | 0.3835 | 0.783 | 0.978 | [0.960, 0.990] | ≈ 0.980 |
| `RE` | about 0.42 | about 0.80 (interpolated) | about 0.980 | [0.960, 0.992] | ≈ 0.982 |
| `RL-floor` | 0.3835, but every span empty | 0 | 0.900 | identical to S5 on every stage 3d key | 0.900 |

- S5 sends 0 copies in every run (L2 refuses every resend, §4).
- `RL` against S5: shown larger by more than 0.05.
- `RL-floor`: `rep_drop_edge = rep_credits` (every unit unprotected at birth, §2.3).
- `RL`: `dec_on_time / dec_units ≥ 0.95`.
- `p_dec_meas` in [0.70, 0.88] against `p_fec_law` ≥ 0.99.
- Datagrams per handed-over unit = 1 + r ± 0.01.

**At the call point on the same path** (T = 0.4):
- S5 and RL `share_by_0p4` ≥ 0.998, equivalent.
- Resends per delivered unit:
  - S5 in [0.10, 0.12];
  - RL within ±0.01 of S5. The first report lists the hole about 20 ms before the first covering repair can arrive, and the sender resends on that report.
- `dup_after_dec / dec_units` in [0.80, 0.95], about `1 − pc`. A finding about the race, not a pass condition.

**Q3e-5, OV: sustained overload at full rate** (`share_by_0p4`; delay p50 from hand-over).

The steady share is `share = [λ·t_r·0.995 + μ_s·(D − t_r)·(1 − ε(1 − P_dec,T))]/(λ·D)`, where:
- `t_r = λ(T − R)/(λ − μ_s)` is the ramp before the backlog binds;
- `μ_s = C/(1 + r)` (ρ ≈ 0, because units sent at `T − R` are never resent);
- D is the 10 s run;
- `P_dec,T` is 0 for S5 and about 0.95 for the repair arms (spans of about 80 members, Appendix B).

| arm | r | μ_s | `share_by_0p4` from the formula | band | delay p50 | expired at the intake, windows after 3 s |
|---|---|---|---|---|---|---|
| `S5` | 0 | 10 000 | 0.841 (stage 3d stored: 0.838) | [0.82, 0.86] | [0.37, 0.40] s | 0.167 ± 0.02 |
| `RL` | 0.0875 queued | 9 195 | 0.794 | [0.77, 0.82] | [0.37, 0.40] s | 0.234 ± 0.03 |
| `RE` | 0.093 queued | 9 150 | 0.790 | [0.765, 0.815] | [0.37, 0.40] s | 0.24 ± 0.03 |
| `RS` | 0.0998 | 9 093 | 0.786 | [0.76, 0.81] | [0.37, 0.40] s | 0.242 ± 0.03 |
| `RF3` | 0.36 | 7 353 | 0.643 | [0.61, 0.68] | [0.37, 0.40] s | 0.387 ± 0.03 |
| `RL-age` | as RL | | | RL − 0.03 to RL + 0.005 | [0.37, 0.40] s | as RL |
| `RL-floor` | as RL | | | S5 − 0.02 to S5 + 0.005 | [0.37, 0.40] s | as S5 |

- Every worth-edge repair arm is shown smaller than S5: RL and RE by 0.02 to 0.07, RS by 0.03 to 0.08, RF3 by 0.16 to 0.23 (the displacement of §2.6).
- `RE` against `RL`: equivalent within ±0.015. OV has no queue drops and no outage, so ε̂ stays near 0.034 and the opposite feedback does not arise.
- `RL-age`:
  - `share_any − share_by_0p4` ≥ RL's;
  - `rep_cut_edge` ≤ 0.1, against RL's ≥ 0.9.
- `RL-floor`: `rep_drop_edge / rep_credits ≥ 0.85`.
- The worth-edge arms: `rep_drop_edge / rep_credits ≤ 0.05`.
- Link busy over [1, 10] s ≥ 0.98 for every arm.
- `dec_on_time / dec_units` in OV and in the CL drain is reported, as a reading on Q18. The worth edge leaves only `R − ô` ≈ 9 ms of margin, and §19 found ô undershooting a persistent queue. If ô were exact the ratio would be ≥ 0.9.

**Q3e-6, CL and CL-long at the call point (full rate).**

The delay recovery at 0.15 s is computed as `t ≈ ((T − R)·λ/μ_eff − (0.15 − d_fwd))/(1 − λ/μ_eff)`, with:
- `T − R` = 0.296;
- d_fwd = 0.09 behind the pause;
- `μ_eff = C/(1 + r + ρ)`, using the drain value of r;
- the simple form, with `− 0.15` and no d_fwd term, as the low end of each band.

| arm | CL `share_by_0p4` | CL-long `share_by_0p4` | recovery at 0.4 s | μ_eff in the drain | delay recovery at 0.15 s (CL-long) | delay p50, CL-long |
|---|---|---|---|---|---|---|
| `S5` | [0.83, 0.86] (3d: 0.847) | [0.91, 0.94] (3d: 0.923) | [0, 0.3] s | 12 100 | 0.54 to 1.06 → [0.3, 1.6] s | [4, 9] ms |
| `RL` | S5 ± 0.02 | S5 ± 0.02 | [0, 0.3] s | 11 440 (r 0.0875) | 0.86 to 1.58 → [0.5, 2.3] s | [4, 12] ms |
| `RE` | — | S5 ± 0.02 | [0, 0.3] s | 8 300 to 9 600 for about 0.4 s, then as RL | [0.5, 3.5] s, and ≥ RL's | [4, 12] ms |
| `RS` | S5 ± 0.02 | S5 ± 0.02 | [0, 0.3] s | 11 240 (r 0.107) | 1.03 to 1.85 → [0.6, 2.6] s | [4, 12] ms |
| `RF3` | S5 − 0.08 to S5 − 0.03 | S5 − 0.10 to S5 − 0.04 | not within the run | 9 158 < λ | not within the run | [0.37, 0.40] s |
| `RL-floor` | — | S5 ± 0.02 | [0, 0.3] s | as S5, then as RL | [0.3, 2.3] s | [4, 12] ms |

- **At full rate the law leaves headroom.** Before the outage the load is `λ(1 + 0.107 + ρ)/C ≈ 0.91`. So RL and RS differ little: RS minus RL on the delay recovery is in [0, +1.0] s.
  - Compare the clamp-bound CLs-long (Q3e-9), where the window floor raises r to 0.23 and RS does not recover within the run. The difference between the two cells is the scale bracket of Q23.
- **RF3 is overloaded by repair alone** (`λ(1.36 + ρ) > C`). The dequeue rule sheds `1 − 9 158/10 000 = 8.4 %` once the backlog binds, about 3.5 s after the start, so after the outage. Its on-time level never returns to the pre-outage level.
- **The delivered share.** For every worth-edge and floor-edge arm, `share_any − share_by_0p4` ∈ [0, 0.005]. So `share_any` carries the bands above, widened by 0.005.
- **Link busy before the outage** ([0.5, 1.9] s):
  - S5 in [0.80, 0.85];
  - RL, RS and RE in [0.87, 0.95];
  - RF3 ≥ 0.98.
- **`rep_link_share` in saturated windows** = `r/(1 + r + ρ)` ± 0.01, from the run's measured r and ρ.
- **`queue_drop_out`:** 0 for S5, R0, RL, RS and RE. For RF3, at most 0.002 of the datagrams; the bound `(B − λ·RTprop_eff)(1 + r) = 880 · 1.36 = 1 197` is below Q = 1 250.

**Q3e-7, recovery with no lifetime** (T = ∞, H = 0; recovery at the 0.4 s readout).

Each row gives the brief's form `t_rec = (L·λ/μ_eff − τ)/(1 − λ/μ_eff)` and §13.6's form `t_rec'`, which adds `B_lost = 1 000` and d_fwd = 0.09. The resend share ρ is taken over [0.005, 0.035].

| cell | arm | μ_eff | t_rec | t_rec' | band |
|---|---|---|---|---|---|
| CL-long (L = 1.5 s, λ = 10 000) | `S5-inf` | 12 100 | 4.83 s | 5.83 s | [4.6, 6.6] s (stored stage 3d: 5.67 s) |
| | `RL-inf` | 11 140 to 11 440 (r 0.0875) | 7.2 to 9.3 s | 8.8 to 10.9 s | [6.5, 12.5] s |
| | `RS-inf` | 10 860 to 11 150 (r 0.107) | 8.6 to 11.1 s | 10.5 to 13.0 s | [8.0, 14.5] s or not within the run; RS-inf minus RL-inf in [0, +4] s |
| CLw-long (L = 3 s, λ = 8 000) | `S5-inf` | 12 100 | 4.67 s | 5.18 s | [4.0, 6.0] s |
| | `RL-inf` | 11 140 to 11 440 (r 0.0875 in the drain) | 5.6 to 6.2 s | 6.3 to 6.8 s | [5.0, 7.5] s |
| | `RS-inf` | 10 860 to 11 150 (r 0.116) | 6.2 to 6.9 s | 6.9 to 7.5 s | [5.5, 8.2] s |
| | `RE-inf` | about 8 300 for about 0.4 s, then as RL | | | RL-inf + [0, 2.0] s |
| | `RF3-inf` | 8 960 to 9 160 | 17.6 to 21.3 s | | not within the run (readable for 13 s) |

- **CLw-long is the cell with power.** Every arm there has room (§4), and the predicted ordering is S5-inf < RL-inf ≤ RS-inf < RF3-inf.
  - RL-inf minus S5-inf is shown positive, from 0.5 to 2.5 s: the repair share's cost in drain time.
- **On each run's own measured `mu_eff_hat`,** the residual `t_meas − t_rec` lies in [−0.3, +2.0] s and `t_meas − t_rec'` in [−1.0, +1.0] s, for every T = ∞ arm with a finite prediction, in every seed. This is §19's law, with μ_eff reduced by the repair share.
- Every T = ∞ arm finishes: after the source ends, the link drains the rest at μ_eff.
- **CLw-long at the call point** (S5, R0, RL, RE, RF3 at T = 0.4):
  - `share_by_0p4` S5 in [0.82, 0.88], the others S5 ± 0.02. Even RF3 does not overload this link: `8 000 · 1.39 < 12 500`;
  - delay recovery at 0.15 s: S5 [0.1, 1.0] s, RL [0.1, 1.2] s, RE [0.1, 2.5] s, RF3 [0.8, 3.0] s.

**Q3e-8, is repair overload self-limiting?** Conditional on Q21's true-loss default:
1. In every finite-T arm, delivered units have delay p99 ≤ T. The intake never exceeds `λ·T`. This holds for RE as well: it rests on L1 and the dequeue rule, not on r.
2. Under the worth edge the repair load does not fall: `rep_drop_edge/credits ≤ 0.05`, and `rep_link_share` holds its `r/(1 + r + ρ)`.
3. Under the floor edge it does (Q3e-5's `RL-floor`). The same edge costs RT all of its repair (Q3e-4).
4. The live law with true loss lowers r once a queue forms: 0.107 → 0.088 at full rate, and 0.23 → 0.11 in CLs-long.
5. **With the estimated loss, the law raises r after an outage** (RE, Q3e-2), and RE drains no faster than RL (Q3e-6, Q3e-7). If RE's post-outage r is not above RL's, the opposite-feedback claim of §2.6 is refuted.
6. With no lifetime there is no bound (RF3-inf in CLw-long, RS-inf in CLs-long).

**Q3e-9, CLs-long: clamp-bound, one tenth of the rate** (W's floor binds on every arm; r = 0.2295 unqueued):
- `share_by_0p4`:
  - RL, RS and RF1: S5 ± 0.02;
  - RF3: S5 − 0.10 to S5 − 0.04.
- Delay recovery at 0.15 s: S5 [0.3, 1.6] s, RL [0.5, 3.0] s, RF1 [0.8, 3.5] s, RS ≥ 6 s or not within the run, RF3 not within the run.
- Delay p50:
  - RS [0.20, 0.40] s;
  - RF3 [0.37, 0.40] s;
  - RL [4, 25] ms;
  - RF1 [4, 12] ms.
- T = ∞: S5-inf [4.6, 6.6] s, RL-inf [7.0, 12.5] s, RS-inf not within the run (law: 66 s).
- **The bracket (Q23).** RS fails to recover here, while full-rate CL-long's RS recovers in [0.6, 2.6] s (Q3e-6). If so, that failure is the window floor at one tenth, not the rate law.

**Q3e-10, the simulator's r ≈ 0.36** (the mandatory reading of §9 step 6).
- **The expectation.** At the CL-long preset the simulator reads W = 16, a p̂ upper of 0.05 to 0.07, and a raw rate at or above its knee of about 0.36, with saturation pressure ≥ 0.5.
- **If it reads otherwise,** §11 item 6 is wrong. The reading is reported with its inputs and is not a stop.
- **The question it answers.** Whether RF3's r = 0.36 stands for "the engine's estimator at a call-like window" or for a simulator defect.

**Q3e-11, other gauges, reported:**
- `rep_used`, `rep_redundant`, `dec_units`, `dec_after_release` and `late_floor_dec` per cell;
- `dec_comp_max` (expected ≤ 60 at full rate);
- the brake: `resend_braked` in CL-long and CLw-long after the outage, reported (2 000/s against about 1 000 lost seqs);
- the A/A self-check, as in stage 3d.

## 9. Run order and stop rules

Each step writes a marker in `out3e/` when it passes. `batch3e.py` refuses to start a part whose markers are missing, and refuses everything while `out3e/STOP` exists.

0. **Build.** No runs.
1. **Tests.** Red on the stage 3d code, then green. `python -m unittest test_toy test_toy2`.
2. **`check_ident3e.py`**, alone on a quiet machine: this code reproduces stage 3d's code. PASS writes `PASS-ident`.
3. **Part ID:**
   - R0 against S5 in the stage 3d cells (3 seeds) and in the new cells (12 seeds);
   - every run of the part `finished`.
   - PASS writes `PASS-r0`. This step compares the new code with itself and with stage 3d's code. It does not compare against stage 3d's stored results.
4. **The RTT gate.** The floor-raised RTT sample (§11 item 9) is being measured separately.
   - Q3e-1's comparison against stage 3d's **stored results** waits for that measurement.
   - If the measurement shows the bug moves no stage 3d reading beyond its A/A floor, write `PASS-rttbug` with the measurement's reference, then run that comparison.
   - If it does move a reading, write `STOP`. The stage 3d baseline is re-run with the fix first, under its own amendment. Stage 3e is then rebuilt on the fixed code: steps 1 to 3 again, with `check_ident3e.py` against the fixed code. Only then does its comparison against the re-run take this step's place.
5. **Part RATE:** test 3 against `rate_ref3e.json`. PASS writes `PASS-rate`.
6. **Part SIMREAD (mandatory).** Read the interactive simulator at its CL-long preset (`viz-three-knobs`):
   - the p̂ upper, W, the raw rate, r_sat, r and the saturation pressure, at the end of the run and averaged over its last 5 s;
   - record them in the results, as Q3e-10 asks. Writes `PASS-simread`.
   - This is a reading, not a pass condition on its values. MAIN does not start without it.
7. **Part AA:** S5 variant 1 in every cell; freeze `nf_stage3e.json`. Writes `PASS-aa`.
8. **Part MAIN:** the arms of §5. The long cells run with 1 worker, the others with at most 2.

**Stop rules. They are mechanical, and the worker does not continue past one.**
- In steps 2 to 5, any difference, any unfinished run or any rate mismatch: write `STOP` with the cause and return to the coordinator.
- During MAIN, after each run, write `STOP` and end the batch if:
  - `finished` is false;
  - `l1_viol_rep > 0`;
  - `commit_viol > 0`;
  - `worth_viol_rep > 0` in a worth-edge or floor-edge arm.
- After a STOP: no supplement run, no code change and no rerun without a registration amendment. Stage 3d's worker ran past a liveness failure. This rule exists for that reason.
- A run slower than 30 s is reported, not treated as a stop.

Report every reading, the A/A self-check, every deviation and the time per run, as stage 3d did.

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
  - **The self-limiting reading of §2.6 rests on this default.** With an estimate, an outage or queue drops raise ε̂ and so r, which is the opposite feedback. Arm `RE` runs the engine's 95 % upper-quantile form (the Beta quantile with 0.995 decay) to measure it.
  - The remaining gap is the BOCD predictive quantile, which the engine reads after 5 updates. Default: not ported. A port (`raptorpath-math/src/estimator.rs`) would follow if RE's post-outage reading matters for the verdict.
- **Q22, the engine's spare-capacity clamp** `min(r, (cwnd − in_flight)/in_flight)`.
  - **Default: not modelled.** The offline simulation has no cwnd, and its B is a store cap, not a congestion window.
  - Direction: the clamp can only lower r, so the results bound the engine's repair load from above.
  - If it is ever modelled, it needs a bind gauge.
- **Q23, the rate scale.**
  - **Decided (coordinator's review): full rate** for the primary cells. Runs take about 3 s, the law is not window-floor-bound there, and S5 and R0 can be compared with stage 3d's stored rows.
  - One one-tenth cell is kept and labelled clamp-bound (CLs-long). With RF1 and RS there and RS at full rate, it brackets what W's floor does to r (Q3e-9).
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
   - At RT: `P_fec(r, W = 16) = 0.997` against about 0.80 at A* = 3 (Appendix B). At CLs-long: 0.99998 against 0.935 at A* = 6.
   - So the law promises a tail it does not buy at call rates. Q3e-4 measures the gap.
   - At full rate the span is about 58 members and decodes about 0.999 (Appendix B), so the gap closes there. The mismatch matters where A* is small, that is, at call rates.
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
   - So the simulator's value is probably its BOCD upper quantile driving the raw rate into the knee. Unreconciled; the mandatory SIMREAD step of §9 reads it (Q3e-10).
7. **The live law responds to a queue by lowering repair,** because the queue's delay enters W through `W_lat = srtt·rate`. That is a latency budget read from a delay that includes queueing. It limits overload here only because the offline simulation's only queue sits behind the store pause. Whether the engine sees it depends on its congestion controller.
8. **b's clamp at ½ binds for every δ ≥ 50,** so the Realtime point sits on the edge of b's range, and any δ above it sees a flat b. Continuous, but the span's response to δ above Realtime is nil.
9. **Stage 3d's RTT sample can come from a seq that never arrived.**
   - `apply_floor` raises the receiver's `rhi` to `f − 1` without an arrival. The report carries that `rhi`, and `on_report` samples `now − sent_t[rhi]` when that seq was never resent (`toy2.py`, `apply_floor` and `on_report`).
   - So a floor that passes a lost tail yields an RTT sample from a datagram the receiver never got. RFC 9002 §5.1 asks for the largest newly *acknowledged* packet.
   - Stage 3e keeps the behaviour for the identity (§3 item 4) and does not fix it. It is being measured separately. §9's RTT gate holds back the comparison with stage 3d's stored results until that measurement is in, and if stage 3d is affected, the baseline is re-run with the fix first.
10. **W's cap of 200 sets r whenever a full-rate link queues.** `W_lat = srtt·rate` reaches 1 000 or more behind the pause. W is then the sender's `MAX_WINDOW_SIZE = 200`, which paper §4.8 already calls "below the multipath lower bound". The queued r (0.0875) is the cap's value, not the window law's.

## 12. Review

The advisor (Fable) reviewed the approach before writing. That review changed five things:
- the law arm reads r live from the measured srtt and send rate, as `rate_snapshot` does, instead of once per cell;
- ε̂ is pinned with its direction stated (Q21);
- the spare-capacity clamp became Q22;
- the age edge became an arm beside the floor edge;
- the T = ∞ arms were added, because the brief's recovery formula binds only for a sender that never gives up.

It also asked for the reference values from the real `raptorpath-math` functions (Appendix A), the mechanical stop markers (§9) and the queue margin (§2.5).

The second review read the version first pushed (`d500454b`) and changed these:
- **The RTT sampling field.** Floors raise the receiver's `rhi` without an arrival, so the first version's `rhi_data` would have broken the r = 0 identity. The field now follows stage 3d except for raises by decode (§3 item 4, §11 item 9).
- **RF1 in CLs-long.** Q23's scale bracket was claimed but not run, so RF1 now runs there with a band.
- **The delivered share** is now stated for the link cells.
- **The delay p50 bands** for RS and RL in CLs-long were re-derived over a range of ρ and widened.
- **`dec_on_time/dec_units`** is now a reported reading on Q18.
- **ws** has its engine provenance (`A_known − W`, not `n_sent − W`).
- **The status line** no longer claims a review before the first push.

The coordinator's review of `f50401f8` changed these:
- **Rates.** The primary cells now run at stage 3d's full rate, with the rate-law values and every prediction recomputed through `raptorpath-math`. One one-tenth cell is kept and labelled clamp-bound.
- **Self-limiting** is now stated as conditional on Q21's true-loss default. Arm `RE` uses the engine's 95 % upper-quantile form, where outage losses and queue drops raise r.
- **The simulator read** is now a mandatory step that gates MAIN.
- **Power.** The cell CLw-long has room for repair: `(1 + r)·λ < μ(1 − ε)` holds with 10–26 % margin, and the outage is 3 s.
- **The RT prediction** now states that it uses the span's decode probability, not the law's P_fec, with the arithmetic.
- **The RTT gate** is now a stop rule (§9 step 4).

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

Output, as (ε̂, srtt, rate) → W, r:
- (0.03, 0.0115, 10 000) → W 115, r 0.10687;
- (0.03, 0.0115, 12 000) → W 138, r 0.09982;
- (0.03, 0.0115, 8 000) → W 92, r 0.11566;
- (0.03, 0.010, 10 000) → W 100, r 0.11234;
- (0.03, 0.100, 11 250) and (0.03, 0.098, 9 000) → W 200, r 0.08754;
- (0.03, 0.0115, 1 000) → W 16, r 0.22952;
- (0.10, 0.101, 50) → W 16, r 0.38355;
- estimated loss: (0.0344, 0.0115, 10 000) → 0.11566; (0.05 / 0.1 / 0.3, 0.100, 11 250) → 0.11609 / 0.19542 / 0.50000; (0.0344, 0.098, 9 000) → 0.09313; (0.0344, 0.0115, 8 000) → 0.12543; (0.135, 0.101, 50) → 0.42470;
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
| 0.03 | 46 | 0.1157 | 1.000 | — |
| 0.03 | 58 | 0.107 | 0.999 | — |
| 0.03 | 58 | 0.36 | 1.000 | — |
| 0.03 | 80 | 0.0875 | 1.000 | — |
| 0.03 | 100 | 0.0875 | 1.000 | — |

The last five rows (the full-rate spans) use 60 000 sources and decode only the connected component of each new equation, which is the same rule restricted to where it can change.

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
