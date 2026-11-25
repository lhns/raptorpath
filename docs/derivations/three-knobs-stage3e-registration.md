# Stage 3e registration: proactive repair in the three-knob model (no runs)

**Status.** Registration only, written before any run. Amendment 1 (§13, 2026-11-25) adds the proof plan's rows, Amendment 2 (§14, the same day, before any stage 3e data) sets seeds, reuse, order and workers from measured rates, and Amendment 3 (§15, before any MAIN data) records the simulator reading and adds the bracket arm RB; the build is specified in `three-knobs-stage3e-build.md`. Nothing here has been run in the offline simulation. Reviewed with the advisor (Fable) before it was written and again after the first push (`d500454b`); it then carries the coordinator's review of `f50401f8` (full-rate cells, the estimated-loss arm, the power cell, the RTT gate) and its final review of `abe2981c` (built on the RTT-fixed code; §12). Still no run.

**Base.**
- Derivation `docs/derivations/three-knobs.md` on `three-knobs` at `93e29a22`. The laws are §1–§10 with Revisions 1–3 (§13, §15, §17); the findings are §18 (stage 3d) and §19 (the interactive simulator and the lifetime sweep).
- The offline simulation on `toy-stage3-knobs` at `e17b4c64`: `tools/toy/toy2.py`, `batch3d.py`, `check_ident3d.py`, `results-stage3d.md`.
- **Stage 3e builds on the RTT-sample fix and its re-baseline** (branch `toy-stage3d-rttfix`, head per the re-baseline). It does not build on `e17b4c64` as it stands, because the bug of §11 item 9 is confirmed. Every stage 3d baseline quoted here is either the re-baseline's number or marked "per the re-baseline".
- Repair laws as the engine and the paper define them: paper `docs/fec-arq-model.md` §3.3–§3.7, §4.1–§4.10 and §5.3–§5.4; `raptorpath-math/src/lib.rs` (`controller_rate`, `derive_window`, `r_saturation`, `soft_saturate`, `bulkness_of_delta`, `span_horizon_b`); `raptorpath/src/control/fec_rate.rs` (`rate_snapshot`, `derive_window`, `RepairRateCache`, `compute_repair_rate_capped`); `raptorpath/src/net/emit_source.rs:540–645` and `:780–900` (the cadenced rate, the spare-capacity clamp, A*, Δ, the trailing span).
- The interactive simulator on `viz-three-knobs`: `raptorpath-wasm/src/lib.rs` (`rate_now`, `rate_inputs_for`, `apply_age_edge`, `send_repair`, the slot loop), and `visualizer-three-knobs-plan.md`.

**What stage 3e adds.** The stage 3d sender S5 gains proactive repair, coded over the engine's trailing span at the rate law's r(δ), and the receiver gains a decoder. Everything else is S5 as the re-baselined stage 3d code runs it.

## Short answer

- **Amendment 1 (§13) adds the proof plan's rows.**
  - The old senders (ADR-0084 at ρ, today's Realtime, today's Bulk) run with repair off, RL and RE, in every one-path cell of the plan. Each design sweeps its own dial.
  - Old against new is read only on matched-share curves, with a dominance rule below the new curve's range and "undetermined" as its own outcome, with one doubling of seeds.
  - 989 configurations and 11 382 runs (11 058 with proof stage 1's repair-off rows reused), plus the conditional cross and the doubling.
  - Order: §13.6.
- **Amendment 3 (§15) records the simulator reading.**
  - r ≈ 0.36 is a run average of a steady 0.29 and a 0.5 cap held for about 3 s after the outage; §11 item 6 is withdrawn.
  - RF3 is relabelled.
  - RE does not cover the aftermath, and DECIDE's and EARLY's repair rows are labelled "law fed the true loss".
  - A bracket arm RB (r at its cap from the outage start to A = 3 s after the end, every design, CL-long, CLw-long and CL-long-v) adds 72 configurations and 864 runs. That brings the count before the verdict to 738 configurations and 6 828 runs.
- **Amendment 2 (§14) cuts the cost on measured rates.**
  - It reuses the re-baselined stage 3d repair-off rows.
  - It keeps 12 seeds on the deciding row and on the near-tie cells, and runs 6 elsewhere with one doubling. The long overload cells' old senders run at 3 seeds and only at their dial ends.
  - ROBUST and SMOOTH run only after a passing verdict.
  - The order gives an early answer: TIME, then DECIDE, then one cell per use case.
  - Before the verdict: 666 configurations and 5 964 runs, about 5–7 h of wall time from the measured rates (up to about 12 h with margin). A measured timing step gates the run at about 24 h.
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
  - With an estimated loss (arm `RE`: normal-approximation upper quantile, BOCD not ported), an outage or queue drops raise ε̂, and so r, at the moment the link has a backlog to drain. That is the opposite feedback, and RE measures it.
  - The floor-edge ablation also limits the load.
- **Cells.**
  - Primary cells at stage 3d's full rate: CL, CL-long and OV, comparable with the re-baselined stage 3d rows, plus CLw-long, a 3 s outage at a load where every arm has room for repair.
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
| ε̂ | **declared idealisation:** the cell's first-send loss `p0`. The engine reads the BOCD 95 % predictive upper quantile (`predictive_loss_upper(0.95)`), which is at least the mean. So the law arm's r is a **lower bound** on the engine's r for the same channel (owner question Q21). **Arm `RE` is the estimated-loss arm (normal-approximation upper quantile, BOCD not ported).** It is not the engine's estimator. It takes the form of the engine's pre-BOCD fallback `LossEstimator::loss_rate_upper(0.95)` = `beta_quantile(b, a, 0.95)` (normal approximation, `estimator.rs:505`), with `a, b` starting at 1 and updated per report as `a ← 0.995·a + received`, `b ← 0.995·b + lost` (`estimator.rs:279`, `:353–357`). A report's batch is the first sends whose fate it newly reveals: newly acknowledged, or newly listed missing, which includes queue drops and outage losses. Dequeue expiries are never sent, so they never enter the estimate. The BOCD predictive quantile, which the engine reads after 5 updates, is not ported (declared, Q21).

  **Its time constant.** The decay is 0.995 per report, so the estimate forgets with a time constant of `report interval / (1 − 0.995)` = 200 reports. In this simulation a report goes out at each 2 ms tick that saw an arrival. At full rate that is every tick, so the time constant is 0.4 s; at RT's 50 units/s it is about one report per arrival, so about 4 s. The engine's batch cadence differs, and so would its time constant |
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
- After an outage the first reports reveal the whole lost store (B = 1 000 seqs) at once. ε̂ then jumps to about 0.2 and decays with the 0.4 s time constant.

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
- **A repair whose span is empty after the edge is not sent, and nothing waits for it.** The credit is spent, not carried to the next first send, and the drop is counted (`rep_drop_edge`). No fallback span is coded either.
  - The engine instead falls back to `generate_repair()` over the leading window (`emit_source.rs:839–846`; §11 item 5). That fallback is not modelled, because under a lifetime it can code members past T, which L1 forbids. Whether the engine should drop the slot or give it to fresh data is for its build. Here the slot simply goes unused by repair, and the next datagram is whatever the sender sends next. Under the dequeue rule a unit is sent only while `a + R < T`, and R > ô, so the newest member nearly always passes and the edge shortens spans rather than emptying them. The prediction is `rep_drop_edge/credits ≤ 0.05` (Q3e-8).

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

**Superseded for code by Amendment 1.** `three-knobs-stage3e-build.md` is the authoritative code specification: base `proof-s1`, the edge list including `"none"` and `"evict"`, RTT sampling through `dec_flag` in place of a `rhi_smp` report field, and the decoder's event signature. Where this section and the build brief differ, the build brief holds for code. This section stays as the record of the derivation's first form.

Build on the RTT-fixed stage 3d code (`toy-stage3d-rttfix`, at the re-baseline's head), in `tools/toy/`. Extend `toy2.py`; add `batch3e.py`, `summarize3e.py`, `check_ident3e.py` and tests in `test_toy2.py` class `Stage3e`. **With every new field at its default the code reproduces stage 3d bit for bit.**

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
4. **RTT sampling:** the receiver keeps `rhi_smp`, the seq the **fixed** stage 3d code (`toy-stage3d-rttfix`) samples from, updated by the same events as there **except raises by a decode**.
   - Reports carry `rhi_smp`, appended at the end of the tuple after the per-path field, and the sender takes its RFC 6298 sample from `rhi_smp` where stage 3d took it from `rhi`.
   - With no decodes `rhi_smp` equals the fixed code's sampling seq at every report, so the re-baselined stage 3d is unchanged.

**Receiver:**
5. **`on_repair(now, j, lo, end, tau)` (event kind 11):**
   - `tau_rx_recv = max(tau_rx_recv, tau)`: RACK counts every datagram;
   - `U = {s in [lo, end) : not got[s]}`. If U is empty, count `rep_redundant` and return;
   - otherwise count `rep_innov`, add the equation, and run `decode(now)` on its connected component.
6. **`decode(now)` on a component** (unknowns linked by shared equations):
   - take a maximum matching (augmenting paths);
   - the determined set is every unknown not reached by an alternating path (unknown → any equation containing it → that equation's matched unknown) from an unmatched unknown;
   - deliver each determined s in increasing seq order through the data path with copy index −1. That path: no `tau` update, no `est.feed`, no `rhi_smp` change, opens holes up to s and raises `rhi` if `s > rhi`, as `on_data` does;
   - count `dec_units`, `dec_on_time` (`now − h_seq[s] ≤ T`), `dec_late` (`now − h_seq[s] > T`: a decode after the lifetime), `dec_after_release` (s had been released) and `late_floor_dec` (s had been passed by the floor);
   - count every repair whose decodes were all late as `rep_late_only`. Under the worth edge that happens only when ô undershot the true one-way delay;
   - count every equation matched to a decoded unknown as `rep_used`;
   - remove decoded unknowns from every equation and drop empty equations.
7. **A source or copy arrival of a seq that sits in a pending equation** removes it, and `decode` runs again on that component (the cascade). A source or copy arriving for a decoded seq counts as `dup` and also as `dup_after_dec`.
8. **Memory:** equations are dropped only when empty. Gauge `dec_comp_max`, the largest component decoded.

**What must not change at r = 0:** no event is pushed, no draw is taken, `ctr` and `ps_ctr` do not advance, and the rate-law reading schedules nothing.

## 4. Cells

**The primary cells run at stage 3d's full rate.**
- A T sweep at that rate took about 3 s per run.
- At full rate the rate law is not set by W's floor.
- R0 and S5 can be compared with the re-baselined stage 3d results, because the configurations are stage 3d's own cells under the engine-like sweep.

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
| `R0` | S5 + repair | 0 | worth | all (identity with `S5`, and with the re-baselined stage 3d rows where they exist, §9) |
| `RL` | S5 + repair | the live law, true loss | worth | all |
| `RE` | S5 + repair | the live law, estimated-loss arm (normal-approximation upper quantile, BOCD not ported, no burst terms; §2.1, §15.2) | worth | CL-long, OV, CLw-long, RT |
| `RS` | S5 + repair | fixed at the cell's unqueued law value (CL, CL-long 0.107; OV 0.0998; CLs-long 0.2295) | worth | CL, CL-long, OV, CLs-long |
| `RF3` | S5 + repair | 0.36: a run average the simulator's estimator never holds, not a steady rate (§15.2) | worth | all |
| `RF1` | S5 + repair | 0.112, the law at stage 3d's 10 000/s and RTprop (a low dose) | worth | RT, CLs-long |
| `RL-age` | as RL | live law | age (the simulator's) | OV |
| `RL-floor` | as RL | live law | floor | OV, CL-long, RT |
| `RL-Hinf` | as RL, receiver strict order (H = ∞) | live law | worth | OV, CL-long (measures Q20's late fills) |
| `S5-inf`, `RL-inf`, `RS-inf` | as S5, RL, RS at T = ∞ (never gives up, any order) | | | CL-long, CLs-long |
| `RF3-inf`, `RE-inf` | as RF3, RE at T = ∞ | | | CLw-long (with `S5-inf`, `RL-inf`, `RS-inf` at r = 0.116) |
| `S5-T0p4`, `RL-T0p4` | S5 and RL at the call point, T = 0.4 | | | RT |
| `AA` | S5, phase variant 1 | | | all (frozen as `nf_stage3e.json`) |

Per cell:

| cell | configurations |
|---|---|
| CL | S5, R0, RL, RS, RF3 |
| CL-long | S5, R0, RL, RE, RS, RF3, RL-floor, RL-Hinf, S5-inf, RL-inf, RS-inf |
| OV | S5, R0, RL, RE, RS, RF3, RL-age, RL-floor, RL-Hinf |
| CLw-long | S5, R0, RL, RE, RF3, S5-inf, RL-inf, RS-inf, RF3-inf, RE-inf |
| RT | S5, R0, RL, RE, RF1, RF3, RL-floor, S5-T0p4, RL-T0p4 |
| CLs-long | S5, R0, RL, RS, RF1, RF3, S5-inf, RL-inf, RS-inf |
| A/A | one per cell |

That is 59 configurations, or 708 runs. RS is not run in RT: there r is constant (srtt does not move), so RS would equal RL.

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
- **`dec_late` and `rep_late_only`: repairs that decode after T.** The worth edge admits a member if `a + ô < T`, and §19 shows ô undershoots a persistent queue. So members whose repair arrives after their lifetime are expected wherever the link queues (OV, the CL drain). `dec_late / dec_units` is the direct measure of that undershoot;
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
10. **RTT sampling** ignores a decoded seq, and otherwise samples exactly as the fixed stage 3d code does (constructed state: a floor passing a lost tail, and a decode above the newest arrival).
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
- After the RTT gate of §9: S5 and R0 in CL, CL-long and OV equal the **re-baselined** stage 3d `CL@b`, `CL-long@b` and `OV@b` rows (`G-T0p4-H0`), and S5-inf in CL-long equals the re-baselined `CL-long@b` `G-Tinf-H0`, on every key at seeds 0 to 11.
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
| `RE` | about 0.425 | 0.812 (Appendix B, r = 0.4247) | 0.981 | [0.963, 0.993] | ≈ 0.984 |
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
| `S5` | 0 | 10 000 | 0.841 (stage 3d before the fix: 0.838; per the re-baseline) | [0.82, 0.86] | [0.37, 0.40] s | 0.167 ± 0.02 |
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
- **Decodes after T** (`dec_late / dec_units`, a reading on Q18). Under the worth edge with exact ô this would be about 0. Predicted:
  - OV, RL, RE and RS: [0.02, 0.30]. The worth margin `R − ô` is about 9 ms, and §19 measured an undershoot of about 5 ticks (10 ms) under a persistent queue.
  - RT: ≤ 0.01 (no queue).
  - CL and CL-long before the outage: ≤ 0.01.
  - A value above 0.30 in OV means the worth edge is not doing its job there. It is then a finding on Q18, not a stop.
- **`RL-Hinf`, strict order with repair (Q20)**, in OV:
  - every unit is unprotected at birth, so F_ann sits at the send frontier and a lost unit is passed by the floor on the next datagram, before any repair covering it arrives;
  - so `late_floor_dec / dec_units` ≥ 0.9: almost every decode is an out-of-order late fill;
  - `share_by_0p4` within ±0.01 of RL's (a late fill is still delivered and still on time);
  - `ooo` per delivered ≥ RL's `dec_units`/units × 0.9.
- `dec_on_time / dec_units` in OV and in the CL drain is reported, as a reading on Q18. The worth edge leaves only `R − ô` ≈ 9 ms of margin, and §19 found ô undershooting a persistent queue. If ô were exact the ratio would be ≥ 0.9.

**Q3e-6, CL and CL-long at the call point (full rate).**

The delay recovery at 0.15 s is computed as `t ≈ ((T − R)·λ/μ_eff − (0.15 − d_fwd))/(1 − λ/μ_eff)`, with:
- `T − R` = 0.296;
- d_fwd = 0.09 behind the pause;
- `μ_eff = C/(1 + r + ρ)`, using the drain value of r;
- the simple form, with `− 0.15` and no d_fwd term, as the low end of each band.

| arm | CL `share_by_0p4` | CL-long `share_by_0p4` | recovery at 0.4 s | μ_eff in the drain | delay recovery at 0.15 s (CL-long) | delay p50, CL-long |
|---|---|---|---|---|---|---|
| `S5` | per the re-baseline (before the fix: 0.847) | [0.93, 0.955] (re-baseline: 0.9414) | [0, 0.1] s (re-baseline: 0.00 s) | 12 100 | 0.54 to 1.06 → [0.3, 1.6] s | [4, 9] ms |
| `RL` | S5 ± 0.02 | S5 ± 0.02 | [0, 0.3] s | 11 440 (r 0.0875) | 0.86 to 1.58 → [0.5, 2.3] s | [4, 12] ms |
| `RE` | — | S5 ± 0.02 | [0, 0.3] s | 8 300 to 9 600 for about 0.4 s, then as RL | [0.5, 3.5] s, and ≥ RL's | [4, 12] ms |
| `RS` | S5 ± 0.02 | S5 ± 0.02 | [0, 0.3] s | 11 240 (r 0.107) | 1.03 to 1.85 → [0.6, 2.6] s | [4, 12] ms |
| `RF3` | S5 − 0.08 to S5 − 0.03 | S5 − 0.10 to S5 − 0.04 | not within the run | 9 158 < λ | not within the run | [0.37, 0.40] s |
| `RL-floor` | — | S5 ± 0.02 | [0, 0.3] s | as S5, then as RL | [0.3, 2.3] s | [4, 12] ms |

- **At full rate the law leaves headroom.** Before the outage the load is `λ(1 + 0.107 + ρ)/C ≈ 0.91`. So RL and RS differ little: RS minus RL on the delay recovery is in [0, +1.0] s.
  - Compare the clamp-bound CLs-long (Q3e-9), where the window floor raises r to 0.23 and RS does not recover within the run. The difference between the two cells is the scale bracket of Q23.
- **RF3 is overloaded by repair alone** (`λ(1.36 + ρ) > C`). The dequeue rule sheds `1 − 9 158/10 000 = 8.4 %` once the backlog binds, about 3.5 s after the start, so after the outage. Its on-time level never returns to the pre-outage level.
- **`RL-Hinf` in CL-long (Q20).**
  - Before the outage the floor trails the span by about `T − ô − d` of data (≈ 0.3 s, against a span of about 6 ms), so a decode below the floor is rare: `late_floor_dec / dec_units` ≤ 0.05 over [0.5, 1.9] s.
  - In the drain, units go out unprotected, and the ratio rises towards OV's. It is reported per window.
  - `share_by_0p4` is RL's ± 0.02.
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
| CL-long (L = 1.5 s, λ = 10 000) | `S5-inf` | 12 100 | 4.83 s | 5.83 s | [4.6, 6.6] s (unchanged by the fix: 5.67 s) |
| | `RL-inf` | 11 140 to 11 440 (r 0.0875) | 7.2 to 9.3 s | 8.8 to 10.9 s | **reported only**; the claims are the residual on measured μ_eff, r in the drain in [0.083, 0.092], and the paired difference below |
| | `RS-inf` | 10 860 to 11 150 (r 0.107) | 8.6 to 11.1 s | 10.5 to 13.0 s | **reported only**; RS-inf minus RL-inf, paired per seed: [+1.0, +3.0] s |
| CLw-long (L = 3 s, λ = 8 000) | `S5-inf` | 12 100 | 4.67 s | 5.18 s | [4.0, 6.0] s |
| | `RL-inf` | 11 140 to 11 440 (r 0.0875 in the drain) | 5.6 to 6.2 s | 6.3 to 6.8 s | [5.0, 7.5] s |
| | `RS-inf` | 10 860 to 11 150 (r 0.116) | 6.2 to 6.9 s | 6.9 to 7.5 s | [5.5, 8.2] s |
| | `RE-inf` | about 8 300 for about 0.4 s, then as RL | | | RL-inf + [0, 2.0] s |
| | `RF3-inf` | 8 960 to 9 160 | 17.6 to 21.3 s | | not within the run (readable for 13 s) |

- **Why the CL-long T = ∞ rows are reported only.**
  - The link is queued for the whole drain, so r sits at the cap value 0.0875 (Q3e-2) and is not uncertain.
  - What is uncertain is ρ (0.005 to 0.035). It moves the absolute recovery by about 2 s, and a band wide enough to cover it cannot be falsified.
  - The paired difference is falsifiable, because ρ enters both arms alike. From the §13.6 form, RS-inf minus RL-inf is 1.7 s at ρ = 0.005, 2.0 s at ρ = 0.02 and 2.1 s at ρ = 0.035.
  - So is the residual against each run's measured μ_eff (below). The absolute claim is carried by CLw-long.
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
- `dec_comp_max` (expected ≤ 60 under the worth edge at full rate; reported only for the edge-`"none"` arms, whose components after an outage can hold hundreds of unknowns);
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
   - PASS writes `PASS-r0`. This step compares the new code with itself and with stage 3d's code. It does not compare against the re-baselined results.
4. **The RTT gate.** The floor-raised RTT sample (§11 item 9) is confirmed: fixing it moves CL-long at T = 0.4 from 92.38 % to 94.14 % delivered, and its recovery from 0.10 s to 0.00 s. Never-give-up, the 2 s lifetime and P2 are unchanged.
   - So stage 3e builds on the fixed code (`toy-stage3d-rttfix`). Steps 1 to 3 run against that code, and `check_ident3e.py` reproduces it, not `e17b4c64`.
   - Q3e-1's comparison with stored results waits until the re-baseline has finished and is committed. Its target is the re-baseline's results, never the pre-fix stage 3d outputs.
   - When the re-baseline is in, write `PASS-rttbug` with its head and run that comparison. Until then MAIN does not start.
   - Any band quoted "per the re-baseline" is filled in from it before MAIN starts, with the band's width kept as registered: the band is centred on the re-baselined value. That substitution is recorded in the results.
5. **Part RATE:** test 3 against `rate_ref3e.json`. PASS writes `PASS-rate`.
6. **Part SIMREAD (mandatory).** Read the interactive simulator at its CL-long preset (`viz-three-knobs`):
   - the p̂ upper, W, the raw rate, r_sat, r and the saturation pressure, at the end of the run and averaged over its last 5 s;
   - record them in the results, as Q3e-10 asks. Writes `PASS-simread`.
   - This is a reading, not a pass condition on its values. MAIN does not start without it.
7. **Part AA:** S5 variant 1 in every cell; freeze `nf_stage3e.json`. Writes `PASS-aa`.
8. **Part MAIN:** the arms of §5. The long cells run with 1 worker, the others with at most 2. Amendment 1 puts DECIDE before MAIN, and AMEND, ROBUST and SMOOTH after it (§13.6).

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
  - The default's cost is out-of-order late fills at H = ∞ (`late_floor_dec`). Arm `RL-Hinf` (OV, CL-long) measures it (Q3e-5, Q3e-6).
- **Q21, ε̂ in the offline simulation's rate law.**
  - **Default: the cell's true first-send loss,** a declared idealisation. The law arm is then a lower bound on the engine's r.
  - **The self-limiting reading of §2.6 rests on this default.** With an estimate, an outage or queue drops raise ε̂ and so r, which is the opposite feedback. Arm `RE`, the estimated-loss arm (normal-approximation upper quantile, BOCD not ported), measures it.
  - The remaining gap is the BOCD predictive quantile, which the engine reads after 5 updates. Default: not ported. A port (`raptorpath-math/src/estimator.rs`) would follow if RE's post-outage reading matters for the verdict.
- **Q22, the engine's spare-capacity clamp** `min(r, (cwnd − in_flight)/in_flight)`.
  - **Default: not modelled.** The offline simulation has no cwnd, and its B is a store cap, not a congestion window.
  - Direction: the clamp can only lower r, so the results bound the engine's repair load from above.
  - If it is ever modelled, it needs a bind gauge.
- **Q23, the rate scale.**
  - **Decided (coordinator's review): full rate** for the primary cells. Runs take about 3 s, the law is not window-floor-bound there, and S5 and R0 can be compared with the re-baselined stage 3d rows.
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
6. **[Withdrawn by Amendment 3, §15.2: the reading shows 0.36 is a run average, not the knee.]** The simulator's r ≈ 0.36 is not reproduced by the closed form at ε = 0.03.
   - The closed form gives 0.2295 at W = 16 and 0.158 at the simulator's CL-long operating point.
   - The knee at W = 16 is exactly 0.360, and the law reaches it only at a p̂ of about 0.06.
   - So the simulator's value is probably its BOCD upper quantile driving the raw rate into the knee. Unreconciled; the mandatory SIMREAD step of §9 reads it (Q3e-10).
7. **The live law responds to a queue by lowering repair,** because the queue's delay enters W through `W_lat = srtt·rate`. That is a latency budget read from a delay that includes queueing. It limits overload here only because the offline simulation's only queue sits behind the store pause. Whether the engine sees it depends on its congestion controller.
8. **b's clamp at ½ binds for every δ ≥ 50,** so the Realtime point sits on the edge of b's range, and any δ above it sees a flat b. Continuous, but the span's response to δ above Realtime is nil.
9. **Stage 3d's RTT sample can come from a seq that never arrived.**
   - `apply_floor` raises the receiver's `rhi` to `f − 1` without an arrival. The report carries that `rhi`, and `on_report` samples `now − sent_t[rhi]` when that seq was never resent (`toy2.py`, `apply_floor` and `on_report`).
   - So a floor that passes a lost tail yields an RTT sample from a datagram the receiver never got. RFC 9002 §5.1 asks for the largest newly *acknowledged* packet.
   - **Confirmed by a separate measurement.** Fixed, CL-long at T = 0.4 delivers 94.14 % against 92.38 %, with recovery 0.00 s against 0.10 s. Never-give-up, the 2 s lifetime and P2 are unchanged.
   - A re-baseline is running on `toy-stage3d-rttfix`, and stage 3e builds on that fixed code (§9 step 4).
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
- **Self-limiting** is now stated as conditional on Q21's true-loss default. Arm `RE`, an estimated-loss arm (normal-approximation upper quantile, BOCD not ported), is a case where outage losses and queue drops raise r.
- **The simulator read** is now a mandatory step that gates MAIN.
- **Power.** The cell CLw-long has room for repair: `(1 + r)·λ < μ(1 − ε)` holds with 10–26 % margin, and the outage is 3 s.
- **The RT prediction** now states that it uses the span's decode probability, not the law's P_fec, with the arithmetic.
- **The RTT gate** is now a stop rule (§9 step 4).

The coordinator's final review of `abe2981c` changed these:
- **The RTT bug is confirmed.** Stage 3e now builds on `toy-stage3d-rttfix`. R0's identity target is the re-baseline, and the S5 baselines are its numbers or "per the re-baseline": CL-long at T = 0.4 is 94.14 % with recovery 0.00 s.
- **CL-long at T = ∞.** RL-inf and RS-inf are reported only, because the uncertainty in ρ makes an absolute band unfalsifiable. The claims there are the paired RS − RL difference [+1.0, +3.0] s, r in the drain in [0.083, 0.092], and the residual on measured μ_eff. CLw-long carries the absolute claim.
- **RE** is relabelled as an estimated-loss arm, not the engine's estimator, and its decay is stated as a time constant (0.4 s at full rate, about 4 s at 50/s). Its RT decode probability is now computed (0.812), not interpolated.
- **A gauge for repairs that decode after T** (`dec_late`, `rep_late_only`), with predicted bands.
- **The empty span** is stated: no repair, no waiting, no fallback span. And `RL-Hinf`, a strict-order arm with repair, measures Q20's late fills.

The proof plan (`c2d005f8`) added Amendment 1 (§13). The owner delegated every decision to the defaults and the advisor. The reading rules are proof stage 1's §1.6 (a)–(d), so both stages read the same way.

The advisor's review of `ffa11dcc` changed these:
- the build brief is authoritative for code, and §3 is marked superseded;
- the verdict scope is aligned in both documents;
- CROSS is 2 new configurations (24 runs), not 5;
- the identity stop rules use `test_toy2.id_diff`'s key set, with stage 3e's readouts excluded;
- K-OV's pause is B = 8, with its prediction re-derived;
- RT gains its 0.12 s readout and a prediction against each baseline;
- the one-sided features are stated.

## 13. Amendment 1: the proof plan's stage 3e rows (2026-11-25, before any run)

**Source.** `docs/derivations/three-knobs-proof-plan.md` on `three-knobs-proof-plan` at `c2d005f8`: §0 (fairness rules F1–F6), §1 (readouts, curves, pass rule, cells), §2.3 (the arm "today's Realtime"), §3 (the deciding row), §4 (amendments A1–A7), §6 (robustness and smoothness) and §9 (order).

**The owner delegated every open decision to the defaults and the advisor.** Every plan owner question OQ1–OQ10 therefore takes its default. That includes signing §1 as written (OQ1) and 12 paired seeds (OQ4).

**What this section adds, and what stays.**
- Sections 1–12 stay registered as written: the MAIN arms, their point predictions and their stop rules.
- This section adds:
  - the old senders, with and without repair;
  - each design's sweep over its own dial;
  - the plan's new cells;
  - robustness and smoothness;
  - a reading rule that compares curves, not points.
- **No old-against-new conclusion is read from a point comparison.** MAIN's point predictions remain checks of the mechanism, and the verdict comes only from §13.3.

### 13.1 Base and code

- **The code base** is the local branch `proof-s1` at `b2addffb`, which is built on the RTT-sampling fix `toy-stage3d-rttfix`. It already holds the arm "today's Realtime" (`E-rt`, plan §2.3), the L1 gauge, the matched-share reader and their tests.
- **Stage 3e's code** (§3, plus §13.2's additions) is built on top of it, as specified in `docs/derivations/three-knobs-stage3e-build.md` on this branch.
- **Identity, extended** (`check_ident3e.py`):
  - with every new field at its default, the 3e code reproduces `proof-s1` on every configuration of `check_ident_proof_s1.py` and `check_ident3d.py`, on every key and trace;
  - R0 (repair at r = 0) equals repair off on the same set;
  - under §9 step 4, the comparison target for stored results is the re-baseline.
- **Reuse of proof stage 1's repair-off rows.** Proof stage 1 runs the repair-off half of DECIDE on `proof-s1`.
  - If `check_ident3e.py` passes on exactly those 27 configurations at seeds 0 to 11, the 3e code computes the same runs bit for bit, and those results are used as-is (F3 is met by identity).
  - Otherwise they are rerun in DECIDE.

### 13.2 Arms, edges and cells

**Designs**, named by behaviour (plan §0.1, F6):

| row | sender | receiver | code |
|---|---|---|---|
| lifetime T, hold H | S5: gives up what cannot arrive by T, announces a floor | releases a hole H after detection, or at the floor | `S5` |
| ADR-0084 at ρ | gives up a hole older than b(δ)·RTprop while `(1 − ρ)·sent` allows, with a copy and a floor | today's Realtime receiver | `C1` (S1c, R0e) |
| today's Realtime | silent shed within `ε̂_s·(1 − P_fec(r, A*, ε̂_s))`; resends only inside the 200 window; no store pause | SRTT/2 hold while the budget is open, else 4·SRTT in [60, 300] ms; force drain at 500 | `E-rt` (proof-s1) |
| today's Bulk | never gives up | strict order | `C3` |
| today's object API | never gives up | any order | S5 at T = ∞, H = 0 (identity) |

**Repair modes**, the same machinery for every design in a row (F2):
- **OFF:** no repair.
- **RL:** the live rate law with true loss (§2.1).
- **RE:** the live law with the estimated-loss arm (normal-approximation upper quantile, BOCD not ported), only in CL-long, OV, CLw-long and RT, as 3e runs it.

In each mode every design uses the same rate law r(δ) at its own δ, the same span width A*, the same decoder and the same loss draw. In RE, today's Realtime's budget reads the same ε̂.

**The span's lower edge** is part of each design (plan A1):
- the new model: the worth edge `a + ô < T` (§2.3);
- ADR-0084 and today's Bulk: edge `"none"`, the engine's span `[max(ws, end − A*), end)` with no lifetime term. That is paper §5.3's oldest unresolved, read through ws;
- today's Realtime: edge `"evict"`. The span starts at `max(ws, end − A*, n_sent − 200)` and is then raised past any shed seq at its low end, so it never codes a seq that is out of its window or already shed.

At T = ∞ the worth edge never binds, so S5 at T = ∞, H = ∞ with repair must still equal today's Bulk with repair, on every key and trace. That is a gate (§13.6).

**Today's Realtime with repair.** Its budget uses the same r as the row's mode. P_fec is ported as shipped, including its cut, at window A* (`residual_loss_after_fec(ε̂, r, A*, σ²)`, `emit_source.rs:629–642`). Its repair-off rows remain machinery-matched controls, reported but not under the verdict (proof stage 1 §1.6 (a)). Its RL and RE rows are under the verdict.

**One-sided features (F2).** The dequeue rule is on for the new model only, in every row: plan §0.2 treats it as the design difference. The duplicate first send and the reorder window are off everywhere, because every cell here has one path.

**Cells** (plan §1.4; one path; 12 paired seeds 0–11, phases per seed):

| cell | definition | δ | τ | new model's T sweep (H = 0) | H = ∞ at |
|---|---|---|---|---|---|
| RT | §4 | 50 | 0.4 s and 0.12 s (the plan's two readouts; 0.12 s is where only repair recovers) | {0.08, 0.10, 0.12, 0.14, 0.16, 0.20, 0.40, ∞} | {0.12, 0.4, ∞} |
| K-I2, K-L10 | stage 3d's (50/s, 60 s, 1.5 s outage at 20 s) | 50 | 0.4 s | as RT | as RT |
| K-OV (new) | L10, owd 50 ms, 50/s offered, link 40/s, Q = 40 (1 s of data), B = 8, 3 000 units, link settings of §4. B must exceed the path's bandwidth-delay product (λ·RTprop = 5 units) for the link to saturate. B/C = 0.2 s keeps the pause queue below τ/2, as CL's and OV's B/C = 0.08 s does on their 10 ms path | 50 | 0.4 s | {0.2, 0.3, 0.4, 0.6, 1.0, ∞} | {0.4, 1.0, ∞} |
| CLc (new) | CL with no outage | 50 | 0.4 s | {0.4, 1.0, ∞} | {0.4, 1.0, ∞} |
| CL, CL-long, CLw-long | §4 | 50 | 0.4 s | {0.4, 0.6, 0.8, 1.0, 1.2, 1.25, …, 1.6 (0.05 steps), 2.0, ∞}, 15 points | {0.4, 1.4, ∞} |
| OV, OV-long | §4, and stage 3d's OV-long (480 000 units) | 50 | 0.4 s | {0.4, 0.6, 1.0, 2.0, ∞} | {0.4, 2.0, ∞} |
| CLc-v, CL-long-v, OV-long-v (new) | CLc, CL-long and OV-long at the video point | 0.005 | 2 s | {1.0, 1.5, 2.0, 3.0, ∞} | {1.0, 2.0, ∞} |
| Ec (new), E-I2, M-I2 | E-I2 with no outage; stage 3d's E-I2 and M-I2 | 0.005 | completion time, longest stall | T = ∞ only | T = ∞ |

**Baseline points in every cell except the file cells:**
- ADR-0084 at ρ ∈ {0.95, 0.98, 0.99, 0.995, 1}, at the cell's δ;
- today's Realtime at δ ∈ {0.5, 5, 50}, plus the cell's δ where that differs (0.005 in the video cells);
- today's Bulk at the cell's δ.

**The file cells** run only the identity: S5 at T = ∞ with H ∈ {0, ∞}, against today's Bulk. An object has no deadline, so ADR-0084's and today's Realtime's partial delivery is not a file outcome.

**At δ = 0.005 the rate law gives r = 0 exactly** (β = 1, and the late-is-fine term with χ = 0). So every RL row at δ = 0.005 is the repair-off row by arithmetic. Those rows run at 3 seeds as an identity check against OFF, every key and trace, not at 12. Today's Realtime at δ ∈ {0.5, 5, 50} inside the video cells has r > 0 and runs at 12 seeds.

**New A/A rows** (frozen into `nf_stage3e.json` before any decision part): K-I2, K-L10, K-OV, CLc, OV-long, CLc-v, CL-long-v, OV-long-v, Ec, E-I2, M-I2.

**Robustness** (plan §6), repair RL, no pass rule:
- the one-way estimate biased by `o_bias` ∈ {−20, −10, −5, +5, +10, +20} ms, for S5 at T = 0.4, H = 0, in OV and CL-long;
- the dequeue margin biased by `R_bias` over the same six values, in OV;
- the cell RQ: CL-long with a standing reverse queue of 0.1 s (`rev_q`, the size of the forward queue, Q/C), with S5 at T ∈ {0.4, ∞}, ADR-0084 at its 5 ρ, today's Realtime at its 3 δ and today's Bulk, plus an A/A row;
- the relative one-way delay `owd_est = "rel"` for S5 at T = 0.4, H = 0, in RQ and OV.

**Smoothness** (plan §6), repair RL, read by `kp_stats` slid over every 5 consecutive points:
- T ∈ {0.360, 0.365, …, 0.440} (17 points), H = 0, in OV, CL-long and K-L10;
- T ∈ {1.800, 1.825, …, 2.200} (17 points), H = 0, δ = 0.005, in CL-long-v;
- T ∈ {5, 10, 20, 40} in OV, CL-long and K-L10, converging to T = ∞;
- H ∈ {0, 1, …, 10} ms and H ∈ {19.6, 19.7, …, 20.4} ms at T = 0.4 in OV, CL-long and K-L10;
- δ = p·{0.90, 0.92, …, 1.10} for p ∈ {0.005, 0.5, 50}, for S5 at T = 0.4, H = 0 and for today's Realtime, in RT and CL-long.

### 13.3 The reading rule: matched-share curves (plan §1.2, with proof stage 1 §1.6)

**The axes.**
- x is delivered at all (`share_any`).
- In RT, `share_by_0p12` is a second on-time readout, read the same way as `share_by_0p4`.
- y is the on-time share at τ (`share_by_τ`, higher is better) and, in outage cells, the recovery time at τ (lower is better).
- A recovery that is "not within the run" is censored at the readable end and counted, never dropped.
- In the file cells, y is the completion time and the longest stall, read for identity only.

**Curves.**
- Per seed, each design's sweep points are sorted by that seed's x and joined by straight lines.
- Neighbouring points whose mean x differ by no more than `max(NF on share_any, 1/units)` keep the one with the better mean y.

**A comparison at a baseline point (x_b, y_b).**
- **Inside the range:** for each seed, interpolate the new curve at that seed's x_b and take the paired difference, signed so that positive favours the new model.
- **The dominance rule:** if x_b lies below the new curve's lowest delivered share in a seed, the new curve's value in that seed is its own lowest point.
  - The new model wins there if it is no worse on the on-time share and on recovery, since it already delivers more.
  - The usual reading applies, and "shown worse" is a loss.
- **Above the range:** a baseline point above the new curve's highest share reads "outside the swept range" and counts as not passed. This should not occur, because every sweep includes T = ∞.

**Curve against curve.** The new curve is compared with ADR-0084's ρ curve both ways: each ADR-0084 point against the new curve, and each new point inside ADR-0084's range against ADR-0084's curve, on both y readouts.

**The reading.**
- `summarize2.read` on the bootstrap 95 % interval of the mean paired difference, against `m = max(T_bar, NF)`.
- For shares, `T_bar = max(1/units, 0.05·(1 − share_ref))`. For recovery and completion, `T_bar = max(5 % of the reference, 0.1 s)`.
- NF is this stage's frozen A/A floor.
- The outcomes are "shown better", "equivalent", "shown worse" and "undetermined".

**Undetermined is its own outcome, with one doubling.**
- A comparison under the verdict that reads undetermined at 12 seeds triggers one registered doubling, of seeds 12 to 23, for only the arms in that comparison: the baseline point and the two new-curve points that bracket it, or the single lowest point under the dominance rule.
- The comparison is then read on 24 seeds.
- No second doubling.

**A cell's outcome.**
- FAILS if any comparison under the verdict reads "shown worse" or "outside the swept range", or if any gate fails.
- PASSES if every comparison reads "equivalent" or "shown better".
- UNDETERMINED otherwise.

**The stage verdict** (plan §1.3):
- **PASSES** if no cell FAILS and at least one comparison outside the identity cells reads "shown better".
- **FAILS** if any cell fails.
- **UNDETERMINED** otherwise, listing the cells.

**Under the verdict:**
- in every mode, the new curve against ADR-0084's curve and points;
- in RL and RE, the new curve against today's Realtime's points (at its named δ = 50, and at the other δ as reported rows);
- today's Bulk and the object API as single points, by the same rule.

Repair-off today's Realtime rows are reported only. Rows in different repair modes are never compared with each other.

### 13.4 Predictions (written before any run)

**Per cell, the expected outcome against each baseline family**, at 12 seeds. The derivations are the formulas of §2.6 and plan §3.3. "BETTER" means "shown better", "EQUIV" means "equivalent", "UND" means "undetermined" is expected before the doubling.

| cell | vs ADR-0084's curve | vs today's Realtime (RL, RE) | vs today's Bulk / object API | why |
|---|---|---|---|---|
| RT | EQUIV or UND | EQUIV or UND | EQUIV or UND | at τ = 0.4 every design delivers ≥ 0.998 on a 100 ms path, and the share bar `0.05·(1 − share)` ≈ 10⁻⁴ is below the seed spread of 3 000 units, so UND is likely and the doubling is expected |
| K-I2, K-L10 | EQUIV | EQUIV | EQUIV on time; recovery EQUIV | no link limit, so the outage's units are lost in time by every design, and the drain is immediate |
| K-OV | BETTER | BETTER | BETTER | with B = 8 the pause queue is about 0.1 s, so srtt ≈ 0.2 s and R ≈ 0.21 s. A unit is sent at age below `T − R` ≈ 0.19 s and arrives near 0.19 + 0.15 = 0.34 s, before τ. So on time is about `(C/λ)(1 − ε) = 0.8 · 0.9 = 0.72`, band [0.66, 0.76] at T = 0.4 repair off. The never-give-up and ADR-0084 rows wait behind an intake growing at 10/s, so they are on time only for about `τ·C/(λ − C) ≈ 1.6 s` of the 60 s, and read ≤ 0.08. Today's Realtime fills the 1 s link queue, which is past τ, and reads ≤ 0.1 |
| RT at τ = 0.12 s, RL | EQUIV, or new BETTER by up to 0.03 | EQUIV, or new BETTER by up to 0.03 | new BETTER by ≥ 0.05 | the same decoder recovers about 0.78 of the lost units within 0.12 s for every design (every edge is inert on a 3-member span), so the new curve is flat near 0.978, band [0.96, 0.99], over T. ADR-0084 and today's Realtime release in order with an SRTT/2 ≈ 50 ms hold, which delays the units behind an undecoded hole (about 2 % of units are such holes) by up to that hold. Today's Bulk holds them until the resend at about 0.17 s, past τ |
| CLc | EQUIV or UND | EQUIV or UND | EQUIV or UND | no outage, load 0.8 (0.91 with repair), so every design is on time |
| CL-long (DECIDE), CL, CLw-long | two-sided: EQUIV, or worse by up to about 0.4 s on recovery (plan §3.3) | BETTER on recovery (today's Realtime has no source pause, so the outage burst and the drain fill and drop at the link queue) | BETTER on recovery at the lowest point (dominance) | plan §3.2–§3.3 |
| OV, OV-long | BETTER | BETTER or EQUIV | BETTER | stage 3d: the new model at 0.4 s is about 0.84 on time against 0.07–0.16 for the never-give-up rows. Today's Realtime sends fresh units into a 0.125 s queue that drops the excess, so it is about `(C/λ)(1 − ε) = 0.81`. Under the dominance rule the new lowest point (about 0.84 delivered) is compared against it |
| CLc-v | EQUIV or UND | EQUIV or UND | EQUIV or UND | clean |
| CL-long-v | EQUIV | EQUIV | EQUIV | stage 3d: a 2 s lifetime saves nothing when the backlog wait is below 2 s |
| OV-long-v | BETTER | BETTER or EQUIV | BETTER | stage 3d: 0.805 within 2 s against 0 |
| Ec, E-I2, M-I2 | — | — | identity on every key and trace | T = ∞ |

**Predicted stage verdict:**
- PASSES, unless the deciding row reads "shown worse" (plan §3.3's live branch), in which case FAILS and §3.5 of the plan applies.
- "Shown better" is expected in K-OV, OV, OV-long and OV-long-v.

**DECIDE with repair (RL), derived from the repair-off prediction.**
- Both designs drain at `μ_eff = C/(1 + r + ρ)` with the same r = 0.0875 while queued.
- Every recovery time on the law's scale grows by the factor `(1 − λ/μ_eff,off)/(1 − λ/μ_eff,RL)` ≈ 0.175/0.126 ≈ 1.4.
- So the new-minus-ADR-0084 recovery difference at matched share under RL has the same sign as repair off. Its size is the repair-off difference × [1.2, 1.6].
- The plateau prediction moves to recovery [6.4, 7.4] s at matched T [1.25, 1.6] s. These are plan §3.3's [4.6, 5.3] s × 1.4, re-centred on the re-baseline with their widths scaled.

**Today's Realtime with repair:**
- `queue_drop_out > 0` in CL, CL-long, CLw-long and OV (no source pause), against 0 for every paused design;
- its shed count is at most `ε̂(1 − P_fec)·sent` (gate-like witness, reported);
- `evict_skip > 0` in the outage cells (the 200 window binds), its execution witness.

**RE against RL inside a design:**
- r higher after an outage (Q3e-2), so recovery no faster;
- the outcome class of each cell's comparisons unchanged from RL. A changed class is a finding on Q21.

**Robustness (no pass rule; reported against these formulas):**
- a bias of +x on ô gives up about x earlier, so `share_by_0p4` falls by about `λ·x/units` per outage in CL-long;
- −x in OV sends units that arrive about x late, so `share_by_0p4` falls towards the dequeue-margin slack once x exceeds `R − d_fwd` (about 10 ms): predicted collapse between −10 and −20 ms;
- `R_bias` acts the same way through the dequeue rule;
- in RQ, ô overestimates by about the reverse queue (0.1 s), so the new model gives up early. `share_by_0p4` at T = 0.4 drops below CL-long's by [0.02, 0.15], and `rel` recovers CL-long's value within ±0.01;
- with no reverse queue, `owd_est = "rel"` equals `"fwd"` within ±0.005 in OV.

**Smoothness:** no K or P statistic "shown" anywhere, in any slid window of 5 points, with each fine sweep's own A/A spread reported. A "shown" K or P is a step or a kink, a defect finding.

### 13.5 Run count

Counts are configurations × seeds, before the runner removes exact duplicates. The runner fingerprints every Cfg, runs each fingerprint once and reports the final count, which is lower by about 10 configurations where sweeps share a point.

| part | what | configurations | seeds | runs |
|---|---|---|---|---|
| MAIN | §5 as registered | 59 | 12 | 708 |
| DECIDE | CL-long: 18 new and 9 old rows × {OFF, RL} | 54 | 12 | 648 (324 if the repair-off half is reused, §13.1) |
| CROSS | plan §3.4, run only if DECIDE's comparison against ADR-0084 reads "shown worse"; repair off. The 2 × 2 cross has 5 cells; 3 of them are DECIDE rows already | 2 | 12 | 24 |
| AA2 | the new A/A rows | 11 | 12 | 132 |
| AMEND | the rows of §13.2, beyond DECIDE and MAIN | 507 at full seeds + 54 at δ = 0.005 under RL/RE (identity) | 12 / 3 | 6 084 + 162 = 6 246 |
| ROBUST | §13.2 robustness | 32 | 12 | 384 |
| SMOOTH | §13.2 smoothness | 272 | 12 | 3 264 |
| DOUBLE | seeds 12–23 for the arms of undetermined comparisons, once | as needed | 12 | at most 12 × the arms involved |
| **total, unconditional** | | **989** | | **11 382**, or **11 058** with the reuse |

AMEND per cell (configurations): CLw-long 75, RT 55, CL 52, OV 47, K-I2 40, K-L10 40, K-OV 36, OV-long 34, CLc 30, CL-long 26 (its RE rows), CLc-v, CL-long-v and OV-long-v 21 each at 12 seeds, Ec, E-I2 and M-I2 3 each.

**Machine time.**
- S5-family runs in short full-rate cells take about 3 s, and in the 20 s cells tens of seconds.
- The old senders in the long overload cells take about 2–5 min per run (stage 3c: 37 s per 100 000 units for ADR-0084's sender, growing faster than linearly). OV-long and OV-long-v dominate.
- Estimate: 5–6 days with 2 workers, the long cells on 1.
- The worker times the first run of each cell, and reports any cell above twice its estimate before going on.

### 13.6 Run order and stop rules (supersedes §9 step 8)

Steps 0–7 of §9 stay, with these extensions:
- step 1's tests include the build's tests (`three-knobs-stage3e-build.md` §7);
- step 3's identity covers `proof-s1`'s configurations;
- step 7's A/A includes AA2.

Then, one batch at a time:
8. **DECIDE.** Read it at once by §13.3. If it FAILS, write `STOP`. CROSS runs (if its condition holds) and everything else waits for the plan's §3.5 decision. A design that will change is not run across every cell.
9. **DOUBLE for DECIDE**, if any comparison reads undetermined.
10. **MAIN**, as registered.
11. **AMEND**, in this order: K-I2, K-L10, K-OV, RT, CLc, CL, CLw-long, CL-long (RE), OV, the file cells, CLc-v, CL-long-v, then OV-long and OV-long-v last. Each cell is read when it finishes, and its DOUBLE runs before the next cell.
12. **ROBUST**, then **SMOOTH**, each with its own marker.

**Stop rules, in addition to §9's.**
- During any part, write `STOP` and end the batch if:
  - a run is not `finished`;
  - `l1_viol > 0` or `l1_viol_rep > 0`;
  - `commit_viol > 0`;
  - `worth_viol_rep > 0` under the worth or floor edge;
  - the T = ∞ identity (S5 at T = ∞, H = ∞ against today's Bulk in the same mode) differs in its sender trace or in any key that `test_toy2.id_diff` compares (every top-level key outside `ID_EXCL` and the `rep3e` block, plus `ID_GAUGES`; the key set of `Stage3d.test_2`);
  - an RL row at δ = 0.005 differs from its OFF row in the same keys or trace. Stage 3e's own readouts sit in the `rep3e` block and are not compared, because only the arm with repair reads the law.
- No point, cell, readout or bar is added, dropped or moved after data is seen (F5). A reading-code fix is allowed and reported.
- A cell that FAILS does not stop later cells. It is reported, and a fix goes through the plan's §8, a derived fix and a rerun of that row only, under a new amendment.

## 14. Amendment 2: cost from measured rates (2026-11-25, before any stage 3e data exists)

**Why.** §13.5's 5–6 day estimate was never measured. The owner judged it unreasonable, and the advisor agreed.

This amendment replaces §13.5's seeds, reuse and timing, and §13.6's order, with rules drawn from measured rates. It changes no arm's law, no cell's physics, no readout and no reading rule. Where it changes a seed count, the reading rule's bars and floor follow the count used. No stage 3e run exists as this is written.

### 14.1 Measured rates (one process, today's re-baseline on the fixed code)

| measurement | value |
|---|---|
| 816 runs | 4 056 s, so 5.0 s per run |
| 1 001 runs | 6 970 s, so 7.0 s per run |
| long overload cells (OV-long) | about 31 s per run |
| all 5 628 stage 3d runs | about 8.5 h |
| one stage 3e run with repair | 11.9 s, 111 MB peak |
| machine | 16 logical cores, 20 GB, about 5 GB free |
| earlier memory errors | 15 workers on long-cell old-sender runs, about 1.5 GB each |

### 14.2 Reuse

The re-baseline re-ran every stage 3d repair-off row on the fixed code, which is the same code as `proof-s1` with the new fields at their defaults.

**A stage 3e repair-off configuration that equals a re-baselined stage 3d configuration is not re-run; its stored result is used.** "Equals" means the same `Cfg` fingerprint (§8.3 of the build brief), with every stage 3e field at its default.

The rule is the same bit-identity rule as the `proof-s1` reuse (§13.1). `check_ident3e.py` must reproduce a 3-seed sample of every reused fingerprint on the 3e code, every key under `id3e` and every trace, before any reuse counts. If any sample differs, nothing is reused, and the difference is a STOP.

**Reused, repair off:**
- S5 at T ∈ {0.4, 2, ∞} × H ∈ {0, ∞}, ADR-0084 at ρ = 0.99 and today's Bulk, in CL, CL-long, OV, OV-long, K-I2 and K-L10, wherever those configurations are sweep points;
- DECIDE's repair-off half, from proof stage 1 (§13.1).

### 14.3 Seeds

- **12 paired seeds** on the deciding row (CL-long, DECIDE and its AMEND rows), and on the cells predicted as near ties in §13.4: CL, CLw-long, RT, CLc, CLc-v, CL-long-v, K-I2 and K-L10.
- **6 paired seeds elsewhere:** K-OV, OV, OV-long, OV-long-v, the file cells, and MAIN's OV and CLs-long rows.
- **The doubling.** An undetermined comparison doubles once:
  - a 6-seed cell from 6 to 12 (seeds 6–11);
  - a 12-seed cell from 12 to 24 (seeds 12–23), as before.

  No second doubling.
- **The A/A floor matches the count.** The A/A rows run at 12 seeds in every cell. NF for a comparison is the p95 of |A − A'| over the first n seeds, where n is that comparison's seed count (6, 12 or 24). It is frozen at each count before any comparison at that count is read. `T_bar`'s `1/units` term is unchanged.
- **3-seed rows:**
  - the δ = 0.005 RL and RE rows stay 3-seed identity checks (§13.2);
  - the long overload cells follow §14.4.

### 14.4 Long overload cells (OV-long, OV-long-v)

- **The old senders read about 0 on time there by construction.** Stage 3d: 0.0000 within 2 s for ADR-0084 at 0.99 and today's Bulk in OV-long, with a fixed-rate source above capacity.
- **So the old senders run at 3 seeds, and only at the ends of their dials:**
  - ADR-0084 at ρ ∈ {0.95, 1};
  - today's Realtime at δ ∈ {0.5, 50} (OV-long) and {0.005, 50} (OV-long-v);
  - today's Bulk.
- **The new model's sweep runs at 6 seeds.**
- **The comparison rule is unchanged.**
  - A comparison against such a point that reads undetermined gets the one doubling, to 6 seeds.
  - An old point that does not read about 0 was mispredicted, and is reported as such.

### 14.5 ROBUST and SMOOTH after the verdict

- These parts do not answer "is the new model better than the old". So they run only after the stage verdict, and only if it reads PASSES.
- SMOOTH runs at 3 paired seeds. Its K and P statistics are read against its own 3-seed A/A spread, and the sliding window of 5 points is unchanged.
- ROBUST runs at 6 seeds.
- Neither part enters the verdict.

### 14.6 Order for an early answer (supersedes §13.6's order; its stop rules stay)

0. §9's steps 0–7, with step 7's A/A at 12 seeds in every cell.
1. **TIME** (§14.8). Gate the whole run on its total.
2. **DECIDE** (CL-long, RL half; the repair-off half reused), read at once. If it FAILS: `STOP`, run CROSS if its condition holds, then wait for the plan's §3.5 decision.
3. **EARLY: one cell per use case at the call point, every design, with repair (RL, the law fed the true loss).**
   - **file:** Ec, the T = ∞ identity against today's Bulk;
   - **call:** RT at T = 0.4 and τ ∈ {0.4, 0.12}, the new model's sweep and every baseline point;
   - **tunnel:** OV, the new sweep and every baseline point (the predicted "shown better");
   - **video:** CL-long-v at T = 2, the new sweep and every baseline point.

   These are the AMEND rows of those four cells under RL, read when they finish.
3a. **RB** (§15.3), read by §13.3 within its own rows, with status per `ENGINE-CAP`.
4. **MAIN**, as registered.
5. **REST:** the remaining AMEND rows, in §13.6's cell order (long overload cells last), each cell's doubling before the next cell.
6. **The stage verdict** (§13.3).
7. **Only if it PASSES:** ROBUST, then SMOOTH.

### 14.7 Workers by memory, and the time formula

**Workers.**
- Light runs, every run other than an old sender in a long cell (111 MB): 6–8 workers. Together they use under 1 GB.
- Old senders (ADR-0084, today's Realtime, today's Bulk) in the long cells (CL-long, CLw-long, CL-long-v, OV-long, OV-long-v): 1–2 workers, at about 1.5 GB each.
- The two pools may run at once while the machine holds at least 1 GB free. The runner reads free memory before it starts a run and waits otherwise.

**Wall time.** With `t_i` the measured single-process time of configuration class i (cell × design × repair mode, from TIME) and `n_i` its runs:

```text
   T_wall  ≈  max(  Σ_light n_i·t_i / w_light ,   Σ_heavy n_i·t_i / w_heavy  )          two pools at once
          ≤   Σ_light n_i·t_i / w_light  +  Σ_heavy n_i·t_i / w_heavy                     one pool at a time
```

Here `w_light` ∈ [6, 8] and `w_heavy` ∈ [1, 2].

**The estimate before TIME**, from §14.1:
- **Base times:** 2 s for the 3 000-unit call cells (RT, K-*), 6 s for the 10 s full-rate cells, 12 s for the 20 s cells, 31 s for the long overload cells.
- **Factors:** ×2 with repair (11.9 s measured, against 5–7 s without), and ×2 again for old senders in the long cells (stage 3c: ADR-0084's sender 37 s per 100 000 units, growing faster than linearly).

| part | runs | CPU hours |
|---|---|---|
| MAIN | 528 | |
| DECIDE (RL half) | 324 | |
| AMEND | 4 920 | |
| A/A | 192 | |
| light total | | ≈ 16.4 |
| heavy total (long-cell old senders) | | ≈ 9.5 |

- **Wall time:** 16.4/7 + 9.5/2 ≈ 7.1 h with one pool at a time, or about 4.8 h with both at once.
- **With a 1.7× margin** for the unmeasured classes: up to about 12 h.
- **The doublings** add up to about 30 % of the runs involved.
- **ROBUST and SMOOTH, after a pass:** 32 × 6 = 192 and 272 × 3 = 816 runs, about 1–2 h more.

### 14.8 The timing step (TIME, before DECIDE)

- **What runs:** one seed (seed 0) of each cell × design under repair RL. The designs are the new model at the cell's named point, ADR-0084 at ρ = 0.99, today's Realtime at δ = 50 (δ = 0.005 in the video cells), and today's Bulk. About 68 runs, slowest first, one at a time.
- **What it records per run:** wall time and peak resident memory, in `results-stage3e.md` (the table of `t_i`).
- **The total:** computed from that table by §14.7's one-pool-at-a-time formula. Each configuration class takes the time of its TIME row (the repair-off classes at half of it, the ratio measured in §14.1), with the registered run counts of §14.3–§14.4 and the worker counts that the measured peak memory allows within 4 GB.
- **The gate:**
  - If the total is at most about 24 h, the run proceeds.
  - Otherwise the runner writes `STOP` with the table and the total, and reports.
  - Whole cells then move to the VM: every arm and seed of a cell runs on one machine and one commit, so no compared pair is split across machines. The moved cells and the commit are recorded before they run.
- **TIME runs are timing only.** They do not enter any reading. They are not stored as seed 0 of their configurations; seed 0 runs again in its part.

### 14.9 Run count after this amendment

| part | configurations | runs to execute |
|---|---|---|
| TIME | about 68 | about 68 (timing only) |
| DECIDE | 54 | 324 (the repair-off half reused) |
| EARLY and REST (AMEND) | 543 | 4 920 (297 more reused from the re-baseline) |
| MAIN | 53 | 528 |
| A/A | 16 | 192 |
| **before the verdict** | **666** | **5 964**, plus TIME |
| CROSS (conditional) | 2 | 24 |
| doublings (conditional) | — | up to about 30 % of the runs involved |
| ROBUST, SMOOTH (after a pass only) | 32 + 272 | 192 + 816 |

This replaces §13.5's 989 configurations and 11 382 runs. The AMEND configuration count falls from 561 to 543, because the long overload cells drop the inner ρ and δ points (§14.4).

## 15. Amendment 3: the simulator reading and the bracket arm RB (2026-11-25, before any MAIN data)

**Why.** The mandatory simulator reading (SIMREAD, §9 step 6, Q3e-10) came out against its registered expectation. This amendment records the reading and withdraws the explanation it refutes. It relabels the arms that rested on that explanation, and registers one bracket arm. No MAIN run exists as this is written, and no reading of DECIDE or EARLY is changed by it.

### 15.1 The reading (interactive simulator, CL-long preset, Realtime δ)

| phase | p̂ upper (BOCD 95 %) | W | raw r | which term binds | saturation pressure | r |
|---|---|---|---|---|---|---|
| steady state | 0.044–0.048 | 16 | ≈ 0.30 | the random-loss term on 100 % of ticks; the knee on 3.8 % | 0.10–0.21 | ≈ 0.29 |
| during the outage | 1 | — | — | — | — | r_max = 0.5 |
| about 3 s after the outage | — | — | — | the Gilbert–Elliott σ² peaks at 50 and B̂ at 135, which keeps the knee above 1 until about 4.5 s | — | 0.5 until about 6.5 s |
| whole run | — | — | — | r at r_max on 25–32 % of ticks | — | mean 0.356 (T = ∞), 0.385 (T = 0.4) |

**So 0.36 is a run average, not the knee.**
- Q3e-10's expectation was W = 16, p̂ 0.05–0.07, a raw rate at or above the knee and pressure ≥ 0.5. It reads p̂ 0.044–0.048 with pressure 0.10–0.21, so it fails.
- The steady value agrees with the closed form: at W = 16 and p̂ = 0.045 the law gives raw 0.298 and r 0.292 (§2.1's port, the 0.045 row).

### 15.2 Withdrawals and relabels

1. **§11 item 6 is withdrawn,** by the rule that a miss is reported, not explained away.
   - Its claim was that the simulator's r ≈ 0.36 is the estimator's upper quantile driving the raw rate into the knee at W = 16, with p̂ ≈ 0.06. The reading refutes it.
   - The value is the run average of a steady r ≈ 0.29 and a saturated r = 0.5 held through the outage and about 3 s after it.
   - The item stays in §11 as the record, marked withdrawn by this section.
2. **RF3 is relabelled** "fixed r = 0.36: a run average the simulator's estimator never holds, not a steady rate".
   - Its rows and predictions stand as the registered dose they are. Its overload results, including CL-long's "overloaded by repair alone", read as a dose that no steady state produces, never as "the engine's r".
   - In §10 and Q3e-10 the question RF3 was to answer, the engine's estimator at a call-like window, is answered by §15.1, not by RF3.
3. **RE does not cover the aftermath.** RE (§2.1) is the normal-approximation Beta upper quantile with a 0.4 s time constant. It predicts r ≥ 0.15 for about 0.4 s after an outage (Q3e-2).
   - The engine-like estimator holds r = 0.5 for about 3 s after the outage, through the Gilbert–Elliott σ² and B̂, which RE has no counterpart for (σ² = 1 and B̂ = 1 in every cell, §2.1).
   - So RE is not a lower bound on the engine's post-outage repair load either. Its rows stay as registered, labelled "estimated-loss arm without burst terms". The aftermath is bracketed by RB (§15.3) instead.
4. **DECIDE's and EARLY's repair rows are labelled "law fed the true loss".** Their RL label means r(δ) evaluated on the cell's true first-send loss, with σ² = 1. That is not what an engine estimator would feed the law during and after an outage.

### 15.3 The bracket arm RB

**The rule** (a declared bracket, not a law):

```text
   r(t)  =  r_max                                        for  t ∈ [outage start, outage end + A)
         =  the law fed the true loss (RL)               otherwise
```

| symbol | provenance |
|---|---|
| r_max = 0.5 | the engine's `max_overhead`, the law's cap (§2.1) |
| outage start and end | the cell's declared `dead` window. An oracle, which is why RB is a bracket and not a design |
| A | **a named parameter, default 3 s**: §15.1's measured aftermath (r = 0.5 until about 6.5 s after an outage ending at about 3.5 s). A scaling check now running decides whether A is a time or a symbol count. The parameter is `rb_A` with unit `rb_unit` ∈ {`"s"`, `"sym"`}: in seconds after the outage end, or in first sends after it. If the check gives a different value, it is set by a new amendment before RB runs |

- **Every design gets the same RB in a row** (F2): the new model, ADR-0084, today's Realtime and today's Bulk.
- **Repair emission keeps its form.** RB only changes r. During the outage a paused sender makes no first sends, so it emits no repairs. Today's Realtime, which has no pause, sends both into the outage.
- **What RB brackets.** RL holds the law fed the true loss through the aftermath. RB holds the cap. The engine lies between them, at or near RB if §15.1 carries over.
- **Its status depends on the engine, and both checks are running now:**
  - if the engine's own estimator holds the cap after an outage, RB counts under the verdict, with the same scope as RL (§13.3);
  - if the engine's spare-capacity clamp (Q22) removes the cap, RB is reported only.
  - The coordinator writes the marker `ENGINE-CAP` with `holds` or `clamped` before RB is read. Until it exists, RB is read as reported only.
- **At δ = 0.005 the engine never reaches the cap.** The Bulk-end law gives r = 0 even at p̂ = 1 (β = 1, and the late-is-fine term has `z = −∞` when its target equals p̂). So RB in CL-long-v has no engine counterpart for the arms at δ = 0.005, and is reported only for them, whatever `ENGINE-CAP` says. Today's Realtime at δ ∈ {0.5, 5, 50} in that cell follows the marker.

**Cells, designs and seeds** (amendment 2's rules: 12 seeds in these cells, which are the deciding row and near-tie cells):

| cell | configurations | runs |
|---|---|---|
| CL-long | new T sweep (15 at H = 0, 3 at H = ∞), ADR-0084 at 5 ρ, today's Realtime at 3 δ, today's Bulk: 27 | 324 |
| CLw-long | the same 27 | 324 |
| CL-long-v | new (5 + 3), ADR-0084 at 5 ρ, today's Realtime at 4 δ, today's Bulk: 18 | 216 |
| **RB total** | **72** | **864** |

**Order:** RB runs after EARLY and before MAIN (§14.6 step 3a). It has its own `DONE-RB` marker and is read by §13.3's rule within RB rows only.

**Cost** (§14.7's rates): about 3.5 CPU-hours light and 4.7 heavy, so about 2.8 h of wall time.

**The new count before the verdict:** 738 configurations and **6 828 runs**, plus TIME.

### 15.4 Predictions for RB (written before any RB run)

Under RB, while queued, μ_eff = C/(1 + 0.5 + ρ) ≈ 12 500/1.53 ≈ 8 170/s.
- **CL-long:** that is below λ = 10 000, so for A = 3 s after the outage the backlog grows by about 1 830/s, instead of draining.
- **CLw-long:** μ_eff ≈ 8 170 is about λ = 8 000, so the backlog holds for A and then drains.

| cell, arm | prediction | derivation |
|---|---|---|
| CL-long, new model at T = 0.4 | `share_by_0p4` = RL's − [0.015, 0.04]; recovery at 0.4 s in [A, A + 1.5] = [3.0, 4.5] s | during the aftermath the dequeue rule sheds `1 − μ_eff/λ ≈ 18 %` of hand-overs: 0.18 × 10 000 × 3 / 200 000 ≈ 0.027. The window share sits near 0.82 < θ until the cap lifts |
| CL-long, never gives up (S5-inf, ADR-0084 at ρ = 1, today's Bulk) | recovery not within the run | backlog 16 000 + 5 500 at 6.5 s, then a drain at about 12 500/1.1 − 10 000 ≈ 1 360/s: about 16 s, past the 14.5 s readable end |
| CL-long, ADR-0084 at ρ < 1 | recovery not within the run for ρ ≥ 0.99; reported for 0.95 and 0.98 | the receiver's give-up of B_lost (plan §3.2) saves about 0.5 s against a drain of about 16 s |
| CL-long, today's Realtime | `queue_drop_out` above its RL value; `share_by_0p4` below the new model's at matched share | with no pause, 1.5 × the sends hit a 1 250 queue that drops the excess |
| CL-long, comparison under the verdict | new BETTER on recovery against every never-give-up and ADR-0084 point (dominance or matched); on time: BETTER or EQUIV | the new model sheds the aftermath's excess at the intake, the old ones queue it |
| CLw-long, never gives up | recovery ≈ A + RL-inf's: [8.0, 11.5] s | the backlog of about 25 000 holds through A, then drains as under RL (RL-inf [5.0, 7.5] s, §13.4) |
| CLw-long, new model at T = 0.4 | `share_by_0p4` = RL's ± 0.02; recovery [0, 0.5] s | μ_eff ≈ λ, so the dequeue rule sheds little and the intake stays at about `λ(T − R)` |
| CL-long-v | the δ = 0.005 arms: reported, and under RB they differ from OFF only by the cap's repairs. Today's Realtime at δ ≥ 0.5: as CL-long's today's Realtime row | §15.3 |

**What RB settles.** If the new model stays at least equal to every old design under RB as well as under RL, the conclusion holds across the bracket. If RB reverses a cell's outcome, that cell's verdict is UNDETERMINED until `ENGINE-CAP` decides which end the engine sits at, and that cell is reported as such.

### 15.5 Ledger finding: the cap binds on 25–32 % of ticks

The cap r_max = 0.5 binds on 25–32 % of the simulator's ticks in this preset. A clamp that binds that often over its operating range is a defect finding (FORMULA-FIRST, MEASUREMENT DISCIPLINE 17–18) and needs a ledger verdict. It is recorded as one, with the measured fraction, the preset and this section as its source.

**The preset is overloaded before the outage, at its own steady r.** The arithmetic:
- offered λ = 800/s against 1 000 slots per second;
- at the steady r ≈ 0.29, first sends plus repairs alone are 800 × 1.29 = 1 032/s;
- resends add more: with 3 % loss and FEC recovering most, about 800 × 0.03 × (1 − P_fec) ≲ 24/s;
- so about 1 030–1 056 sends contend for 1 000 slots.

The preset is overloaded by repair at its steady r before any outage. Its recovery readings are therefore readings of an overloaded link, not of an outage drain. The simulator's plan sized its bottleneck preset as `μ = 1.25·λ·(1 + r_est)` (visualizer plan §7). A link of 1 000/s at 800/s offered satisfies that only with r_est = 0, so the preset's link and the estimator's steady r disagree. That is recorded beside the ledger finding.

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
| 0.10 | 3 | 0.4247 (RE in RT) | 0.840 | 0.812 |
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
