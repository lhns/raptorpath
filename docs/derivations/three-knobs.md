# Three knobs: lifetime T, delay price δ, order hold H (derivation, no code)

**Proposal (owner, 2026-11-23).** Replace the declared dials (δ, ρ; r derived) with three numbers the application states:

1. **Lifetime T, in seconds.** "Stop trying after T", measured from when the application handed the data over. This is a rule the engine can always keep. The delivered share becomes a reported outcome, never a promise. T = ∞ means "never give up" (today's ρ = 1), reached as a limit and not handled as a special case. There is no round-trip cap.
2. **Delay price δ.** Within T, how much each millisecond costs against throughput (Copa's utility weight, paper §4.1). The repair rate r stays derived from δ and the channel. δ is independent of T: buffered video is T = 2 s with δ ≈ 0, a call is T ≈ 400 ms with δ high.
3. **Order hold H, in seconds.** How long the receiver holds later data behind a gap. H = 0 is any order, H = ∞ is strict order, and values between give mostly-in-order delivery.

Reported, not set: the delivered share, the repair bandwidth r and the achieved delay.

Owner statements this draft is held to:
- "we need as few essential dials as possible but i understnad that for a call a fixed latency deadline can be important";
- "the overall deadline shouldnt have any roundtrip limit";
- "we have a valid 'mode switch' when a retransmit is as fast as a fec packet, thats okay";
- "what if we cap it but cannot fullfill it?";
- on the order knob: "having to set order to T is kinda annoying because T differs sometimes. so maybe it is the time in percent between 0 and T ?"

**Short answer.**
- **The proposal works and removes more than it adds.** The count budget `(1 − ρ)·S_src` and its operand Ab, the shed deadline D(δ) as a give-up age, T_cut and `find_t_cut`, the re-derived T_ρ, run-atomic spending and the bank question O2, the disorder price ω as a law input, `shed_recv_hold`, `recv_shed_on` with its exception rows, `reorder_timeout_ms` and `window_out_of_order` all go. What comes in is two declared numbers and one measured one-way term.
- **The laws (§2), one line each:**
  - L1, the sending bound: no datagram for s is staged once its age a(s) ≥ T, and that includes data still waiting in the intake.
  - L2, resend worth: a resend is staged only if `a(s) + ô < T`, with ô the one-way delay estimate SRTT/2. When no further resend can pass L2, the sender takes the verdict and sends the floor alone.
  - L3, the reported share: §3.7's race read at the declared T, `share(T) = 1 − ĥ·Ŝ(T)`.
  - L4, the span: b(δ)·RTT keeps its job as the span width. T reaches the span only through the verdict set, with no new term and no clamp.
  - L5, the rate: the engine's r(β(δ)) is unchanged. In the model, §4.10's triangle becomes (t_tail, T) → r, with T in place of ρ.
  - L6, the receiver: release a hole at `min(t_detect + H, t_F + H_f)`. H = 0 and H = ∞ are the two ends of this one expression, and the floor pass ends every hold.
  - L7, memory: at finite T the store and the intake are each bounded by about rate·T with no clamp. At T = ∞ the bound is the pooled cap B, which the ledger has PINNED (F01), so that case carries a defect verdict.
- **"T < one round trip: only repair helps" comes out of L2** with no threshold. A hole can first be resent at age a₁ ≈ RTT plus the detection delay. L2 admits that resend only if `a₁ + ô < T`, which means `T ≳ 1.5·RTT`, the paper's own ARQ latency (§3.7). Below that, every hole gets its verdict at the first report, and the only recovery left is the proactive repair r already sends. The share steps there because the link steps there: this is the owner's accepted "retransmit as fast as a FEC packet". L2 is continuous in T, and the aggregate share moves through the spread of a₁ and of the RTT.
- **The order knob: form (b), H in seconds on [0, ∞], with no min(H, T) (§2.6, §2.7).** The receiver does not know T, nor the hand-over time of a missing seq. So both `h·T` (form a) and `min(H, T)` would need T at the receiver, either as a wire field or as a matched config, and that is the mixed-configuration pairing defect again. The min is also unnecessary: the sender's floor, sent when the verdict is taken, ends every hold, so strict order (H = ∞) already follows T. **Form (a) fails at T = ∞**: any h > 0 gives H = ∞, and h = 0 gives 0·∞. A never-give-up stream that wants a finite hold therefore cannot be stated, and the owner has already accepted exactly that behaviour for Bulk ("late, out-of-order delivery instead of a stall after a long outage", `rho-step-sites.md` §8 answers). So the owner's annoyance is answered by H = ∞ meaning "strict, whatever T is", not by a fraction.
- **Things in the proposal that do not hold as stated (§8):**
  - "stop trying at age T" and "keep the last copy" contradict each other: a copy sent at age T always arrives after T;
  - "a resend must arrive before T" needs a one-way delay, which the sender does not measure;
  - "H = T, strict order, forced for a byte stream / TCP tunnel": the tunnel carries IP packets, so order is not forced there. A true byte stream forces T = ∞ as well, and strict order is exact only at T = ∞;
  - hand-over is not first send, because the source pause separates the two;
  - the round-trip cap the owner rejected is already dead code in the span law.

**Inputs read** (`github/main` `e337e328`; branch `toy-sim-stage2` `c0f33f8e`):
- paper `docs/fec-arq-model.md` §1.2, §3.2, §3.3, §3.5, §3.7, §4.1, §4.10, §4.11, §5.3, §5.4, §5.6, §6.1 (B, q(δ)) and §7.3;
- ADR-0084 and its Amendments 1–3, including the `rho1-debt-exception` block;
- `docs/derivations/skip-rule-efficiency.md` §1–§5 (main) and §7.9 (toy-sim-stage2), `rho-step-sites.md` §8 with the owner answers, `floor-hold.md`, `ordering-hold.md` (short answer), and `tail-and-hold.md` on branch `tail-fix-derivation` `07752704` (short answer: form E0);
- code:
  - `raptorpath/src/config.rs:60–100` and `:285–370` (`window_reliable`, `reorder_timeout_ms` default 20, `reorder_max_size`, `window_out_of_order`, `inner_feedback_weight`);
  - `net/receiver.rs:150–190` (the reorder-buffer arms, `recv_shed_on`), `:785–795` and `:1752–1762` (`set_timeout(shed_recv_hold(..))`);
  - `net/shed.rs:23–80` (`shed_armed`, `shed_deadline_us`, `shed_allowed`, `shed_recv_hold`);
  - `net/emit_source.rs:615–645` (D, A*, Δ, the budget refresh);
  - `net/store_cap.rs:299–335` (`delta_budget_b`, `delta_price`, `stall(δ, ρ)`);
  - `net/late.rs:62–100` (`W_COST_RATIO`, `request_bar`);
  - `net/reorder.rs:1–60`;
- toy, read with `git show`: `tools/toy/toy2.py`, `batch2.py`, `summarize2.py` (`kp_stats`, `read`, `T_of`), `test_toy2.py` (the deny test) and `results-stage2.md` (Part T, the mid-stream outage cell);
- ITU-T G.114 (05/2003), summary page only (§4).

## 1. Symbols

Several letters already carry other meanings in the repository. This table fixes them for this document.

| symbol | here | not to be confused with |
|---|---|---|
| **T** | the lifetime knob, in seconds, measured from hand-over | §7.3's lateness T; ADR-0084's tests T1–T8; toy "T" bars (`T_of`) |
| **δ** | the delay price, unchanged (paper §4.1) | — |
| **H** | the order hold knob, in seconds, measured from hole detection at the receiver | §4.11's H (the reorder share of a deadline, with the opposite end meaning out of order, §6); §7.3's H (store headroom); `floor-hold.md`'s H (below: H_f); design U's H_o |
| **H_f** | the floor-pass hold: the wait after a floor arrives before the holes below it pass. `floor-hold.md` (main: a windowed maximum of RFC 4737's late time offset) or `tail-and-hold.md` form E0 (branch: a per-path event count with a silence timer), whichever the owner confirms. This document does not re-derive it | H |
| t_h(s) | hand-over instant of s: when the application gave it to the engine (sender clock) | `src_send_ts_us`, the first-send stamp |
| a(s) | age, `now − t_h(s)`, at the sender | the toy's and the engine's age from first send |
| ô | one-way delay estimate of a datagram sent now: SRTT/2 | RTprop |
| V | the verdict set (ADR-0084) | — |
| F, t_F(s) | the sender's floor; arrival of the first floor above s at the receiver | F_rx |
| t_detect(s) | first arrival above s at the receiver | — |
| B | the pooled outstanding cap (paper §6.1; ADR-0084 decision 3) | — |
| ĥ, Ŝ | holes per source symbol after FEC; Kaplan–Meier survival of the resolution age (skip-rule §3.4) | — |

## 2. The laws

Every law is written once, with each symbol's provenance. "Declared" means an application input, "measured" means engine state, "cited" means an external reference, and "derived" means a function of the others.

### 2.1 L1, the sending bound (the rule the engine always keeps)

"Nothing for s is staged once s has outlived T":

```text
   stage any datagram for s   ⇒   a(s)  <  T ,          a(s) = now − t_h(s)
```

| symbol | provenance |
|---|---|
| T | declared; T ∈ (0, ∞], with ∞ a value and not a flag (§3) |
| t_h(s) | measured: stamped once, when the intake admits s. This is new; the engine stamps first send today (`emit_source.rs`, `src_send_ts_us`) |
| now | measured |

- **Unsent data is covered too.** A unit still in the intake when `a ≥ T` is dropped there, counted as `intake_expired`, and never gets a seq. This is the same rule at a different seat. Without it a source pause (ADR-0084 decision 3) turns T into "T after the pause ends", which is not what an application means by "stop trying after T".
- **This is an absolute invariant**, not a target, so it gets an absolute test: no datagram carries a seq whose age at staging is at least T. It holds whatever the link does, which answers "what if we cap it but cannot fulfil it?". The engine keeps the rule, and the share it achieves is reported (L3).
- **First sends are not gated by ô.** L1 alone decides whether fresh data is sent. A first send is the data itself, and it is also the probe that keeps ô measured. If L2 gated it, a path with ô > T would stop sending and stop measuring, and stay stuck there (§5).

### 2.2 L2, resend worth and the verdict

"A resend is worth sending only if it can arrive before s outlives T":

```text
   resend s now   ⇔   s unresolved  ∧  a(s) + ô  <  T
   verdict(s)     at the first decision instant where s is unresolved and  a(s) + ô  ≥  T
   verdict(s)  ⇒  s leaves the store;  the floor F is sent (and repeated on the sweep clock, ADR-0084 decision 1);  no copy
```

| symbol | provenance |
|---|---|
| ô = SRTT/2 | measured SRTT (`est.rtt()`), with the even split between the two directions as a **declared assumption** (a symmetric path). The measured replacement is a receiver timestamp (ADR-0078's planned field, not yet on the wire). On an asymmetric path the error is half the asymmetry, and a gauge reports it once receiver timestamps exist |
| "unresolved" | measured: in a reported gap or the open tail, and not acknowledged (ADR-0084 decision 1) |
| decision instant | a gap report, a tail-sweep fire, or the per-seq verdict timer at `t_h(s) + T − ô` (below) |

- **Why `a + ô < T` and not a probability.** The full form is "send if P(this resend arrives before T) is worth one wire symbol", that is `P(owd ≤ T − a) ≥ w/(1 + w)`, with `w = W_COST_RATIO = 1` (`net/late.rs:64`, a register row). With the normal RTT model of §3.2 and an even split, that probability reaches ½ exactly at `T − a = SRTT/2`, so the full form reduces to the line above. The reduction adds no constant; it uses one that already exists. If the owner wants a different cost ratio, the bar moves to a quantile of the RTT spread instead of the median. Recommended: keep w = 1 (§7, Q7).
- **The verdict has its own timer.** Reports and sweeps are coarse: in a 50-packet/s call at 100 ms RTT the sweep fires every 100 ms. A timer at `t_h(s) + T − ô`, re-checked when it fires against the current ô, gives the verdict at its own instant. ADR-0084's tail sweep stays the repeat clock for the floor.
- **No last copy.** ADR-0084 Amendment 2 sends one copy with every verdict, for two reasons: it saves allowance (gone with the count) and it can resolve a hole sooner than the floor alone (finding F1). Under L2 the verdict is taken exactly when no resend can arrive in time, so the copy fails its own worth test. The "last copy" becomes simply the last resend L2 admitted, sent one decision earlier. This amends Amendment 2 (§6; owner question Q4, with the copy kept as a registered arm S2c, §9).
- **"Only repair helps" falls out.** The first decision instant at which a hole can be resent is the first gap report, at age `a₁ ≈ t_detect − t_h + owd_back`. That is about one RTT plus the gap to the next arrival on one path. L2 admits that resend iff `a₁ + ô < T`, that is `T > a₁ + ô ≈ 1.5·RTT + 1/rate`: the paper's ARQ latency (§3.7, "L_arq ≈ 1.5·RTT"). Below that, every reported hole gets its verdict at the first report, nothing is resent, and only proactive repair (r) recovers anything. No term compares T with a constant or with "one RTT". The comparison is per seq and on measured state, and the aggregate share moves continuously in T through the spread of a₁ (report timing, jitter, the per-seq phase).
- **The step that remains is physical.** With zero jitter and a fixed report phase, the share as a function of T has a real step at `T = a₁ + ô`, because below it no resend can arrive and above it one can. That is the "valid mode switch" the owner accepted. It is located by measured delays, never by a declared constant, and it moves when the path moves. §9 registers a check that it does (prediction P5).
- **At N ≥ 2 the early-report problem stays where it is.** A fast path reports a slow-path seq as missing early. Today D(δ) guards the shed against that (skip-rule §3.1: "live at N ≥ 2"). Under L2 the early report causes a resend, which is the existing §5.5/C3 per-flight hole law's job, not the verdict's. The verdict no longer reads reports for its timing, so it needs no reorder guard.

### 2.3 L3, the reported share (prediction and measurement)

T is declared, so the share is a prediction and a measurement, both printed, and never an operand. §3.7's race, read at T:

```text
   P(delivered by T)  =  1 − ε · P(t_fec > T) · P(L_arq > T)
   share_pred(T)      =  1 − ĥ · Ŝ(T)                              (skip-rule §3.4's balance, read the other way)
```

| symbol | provenance |
|---|---|
| ĥ | measured: reported holes per source symbol, after FEC |
| Ŝ(t) | measured: Kaplan–Meier survival of the resolution age, with verdicts right-censored at their verdict age (skip-rule §3.4; Kaplan & Meier 1958, cited there) |
| ε, P_fec, L_arq | measured, derived (§3.5, §3.7) |

- skip-rule §3.4 solved `ĥ·Ŝ(T_ρ + L) = 1 − ρ` for T_ρ, given a declared ρ. Here T is declared and the same balance yields the share. The self-censoring argument carries over unchanged: Ŝ is observed up to T, which is the only value read.
- **Two shares are reported**, because a late fill still reaches the consumer (O3 = yes, owner 2026-11-21):
  - `share_any`: source units delivered at any time, divided by source units handed over (intake expiries included in the denominator). It is exact at the receiver once the sender's count of handed-over units arrives (one counter).
  - `share_T`: delivered with `t_deliver − t_h ≤ T`. This needs the two clocks related. The engine has no clock sync, so it is an estimate: the receiver's delivery instant minus the send stamp, offset by the minimum of (arrival − send stamp), with ô as the one-way term. That has the same drift caveat as `lat.rs`'s A_x (floor-hold §1), and it is printed as an estimate. In the toy (one clock) it is exact.
- `G_rx` and `passed_undelivered` (wire v11) stay, as gauges of what the receiver passed undelivered. `Ab` and the `[BUDGET] breach=` line have nothing left to bound.

### 2.4 L4, the span and where b(δ) keeps its jobs

The span law (§5.3) is unchanged:

```text
   A*  =  clamp( ⌈rate · b(δ) · RTT⌉ , 1 , W )                          span width, trailing [F, F + A*),  F = oldest unresolved
```

- **T reaches the span only through V.** A verdicted seq has left the store and is no longer unresolved, so the span's lower edge F can never be older than about `T − ô`. A repair therefore never codes a member that L2 has given up. No T term and no clamp enter the span, so no new bind gauge is needed.
- **The 2·RTT cap is already dead.** §5.3 writes `D = min(b(δ)·RTT, 2·RTT)`, and §5.4 clamps b to [½, 2], so `b·RTT ≤ 2·RTT` always and the min never binds. The owner's rejection of the round-trip cap therefore costs the span law nothing: it removes an inert term (`emit_source.rs:620`, `.min(2.0 * rtprop)`), which needs a zero-behaviour-change commit. The cap that did act was D used as a **give-up age** (§5.6, `shed_deadline_us`), and L2 replaces that.
- **b(δ) keeps its other jobs:** the span width above, the pool setpoint `q(δ) = (b(δ) + 1)/30` (§6.1) and the contract stall (§6.4). It loses one job, the shed deadline.

### 2.5 L5, the repair rate

**Engine: unchanged.** `r(β) = (1 − β)·r_anchor + β·r_late-is-fine, β = bulkness(δ)` (paper §4.5) reads δ and the channel. It reads neither ρ nor T_cut today (§4.10: "T_cut is not a runtime input"), and ADR-0079 deleted the age-keyed copy branch, the one site where an age horizon touched the proactive plane. So "r stays derived from δ and the channel" is literally true of the engine.

**Model: §4.10's triangle survives with T in ρ's seat.** §4.10 truncates the taper at the give-up age, and "any two of (t_tail, ρ, r) fix the third" found T_cut from ρ first. With T declared, the bisection step falls away:

```text
   (t_tail(δ), T)  →  r :      P_arq(T)  =  Σ_{k≥1} ε_c^(k−1)·(1 − ε_c) · Φ( (T − t_k) / σ_k ) ,     t_k  ≈  a₁ + (k − 1)·RTT + ô
                               solve   ε·(1 − P_fec(r))·(1 − P_arq(T))  ≤  t_tail(δ)   for the least r
```

| symbol | provenance |
|---|---|
| t_k, σ_k | measured: the k-th resend's arrival age and its spread (RTT samples; §3.2's normal model) |
| ε_c | measured: resend loss |
| t_tail(δ) | derived, §4.1 |

- This is where "T below a round trip needs repair" would move r. §4.10's 33 ms video example moves r from the Auto row toward the Realtime row on exactly this ground.
- **Whether the engine's r should read T is an owner question (Q3), recommended "not in the first build".** The reason is that it is a rate-law change that rides a battery of its own, and the toy has no proactive repair to test it (§9.1). Until it lands, an application that states T below one round trip and a low δ gets `share ≈ 1 − ε·(1 − P_fec(r(δ)))`, and that is reported.

### 2.6 L6, the receiver's release rule

Design U's release (skip-rule §4.1), with the application's H in place of `H_o = ω·d/δ`:

```text
   release hole s to the consumer at   t_rel(s)  =  min( t_detect(s) + H ,  t_F(s) + H_f )
   ack frontier A_rx  =  min{ s : s ∉ received ∧ s ≥ F_rx }          (R1's next_expected; unchanged)
   a late fill of a released s:  delivered at once, out of order (reorder.rs:75–82), counted
```

| symbol | provenance |
|---|---|
| H | declared, H ∈ [0, ∞], read at the receiver only |
| t_detect(s) | measured, the receiver clock |
| t_F(s), H_f | measured; H_f as §1 says |

- **One expression, two ends.**
  - **H = 0:** release at detection, so any order. Every non-hole unit is delivered on arrival. This is today's `window_out_of_order` corner, and §4.11's "out-of-order corner".
  - **H = ∞:** `t_rel = t_F + H_f`, floor-only, which is R1 (skip-rule §4.2: "R1 is U with H_o = ∞").
  - **H = ∞ and T = ∞:** no floor is ever sent, so `t_rel = ∞`, today's ρ = 1 reorder buffer.
  - Every H in between is mostly in order. t_rel is continuous and non-decreasing in H.
- **The min is two events, not a clamp on the knob.** Both terms are always computed. A bind gauge counts which one fired (`rel_by_hold` and `rel_by_floor`), which FORMULA-FIRST asks of every min.
- **Strict order is exact only at T = ∞.** At finite T a floor pass can come before a late fill, when the last resend L2 admitted was delayed past `t_F + H_f`. That fill is then delivered out of order. It is counted (`ooo`), and its rate is bounded by H_f's miss gauge (floor-hold §6 and tail-and-hold §2.6).
- **The receiver reads neither T nor δ.** T reaches it only as the sender's floor, so a mismatched configuration cannot select a different machine: the receiver never compares its own T with anything.
- **Receiver memory.** Received data above a stuck ack is bounded as at ρ = 1: by the source pause at B, and by SACK release (C2) keeping received-above seqs out of O. With H finite, released data leaves the buffer at `t_detect + H`. The force-drain at `reorder_max_size = 500` has no job left (§6).

### 2.7 The order knob's form: fraction or seconds

The coordinator posed the owner's suggestion as a fork: (a) `H = h·T`, h ∈ [0, 1]; (b) H in seconds, with an effective hold of min(H, T). §2.6 adopts (b) **without** the min. Here is the case for and against each.

| | (a) `H = h·T` | (b) H in seconds, no min |
|---|---|---|
| strict order | h = 1 | H = ∞ |
| any order | h = 0 | H = 0 |
| follows T when T changes | yes, by construction | yes, at the strict end, through the floor; between the ends, no |
| T = ∞ with a finite hold (Bulk tunnel releasing after an outage, owner-accepted) | **cannot be stated**: any h > 0 gives H = ∞ | H = 1 s, say |
| h = 0 at T = ∞ | 0·∞ is undefined; needs a convention, which is a special case at ∞ | H = 0 |
| what the receiver needs | T (a wire field or a matched config) and each hole's hand-over age (it does not have the missing seq's stamp) | nothing beyond its own H |
| unit an application has | a fraction of a deadline | seconds: a jitter-buffer depth, an inner RACK window |
| at finite T, as a number | `h = H/T`: the same release instant, so the two forms agree wherever (a) is defined | the same |

- **What decides it is the T = ∞ row.** A stream that never gives up but still wants order released after some seconds has no (a) form. The owner has accepted that behaviour for Bulk after a long outage, and the TCP-in-tunnel measurement (`meas-disorder-price`) may yet ask for it in normal operation.
- **The second deciding fact is the receiver row.** (a) puts T at the receiver, and the receiver cannot evaluate `h·T` against a hole's age without the missing seq's hand-over stamp.
- **Where (a) is better:** an application that wants "hold a quarter of my deadline" and changes T often keeps its proportion with one number. That application can compute `H = 0.25·T` itself, so nothing is lost by putting seconds on the wire of the API.
- **Why the min is dropped.** `min(H, T)` would bound the hold by T at the receiver, but the floor already does it: under L2 the floor is sent at about `t_h + T − ô`, arrives at about `t_h + T`, and passes the hole after H_f. A strict stream (H = ∞) therefore releases at about T + H_f with no T at the receiver. The owner's annoyance ("having to set order to T") is answered by H = ∞ meaning "strict, whatever T is".
- **Recommended: (b), H in seconds on [0, ∞].** Neither form is registered as a separate arm: at finite T they are numerically identical, and the case that separates them, T = ∞, cannot be run under (a) at all (§9.2).

**The disorder price ω (design U's form) against seconds.** U wrote the hold as `H_o = ω·d/δ`: a price, not a time. That form scales the hold with the measured ARQ delay d and with δ. An application that moves from a 10 ms path to a 300 ms one keeps the same ω and gets a hold 30 times longer. In seconds it keeps the same H and releases far more holes before their fill.
- That is the honest cost of seconds: the hold does not follow the path.
- Seconds win for three reasons:
  - applications state holds in seconds (a de-jitter buffer, a playout deadline), while nobody states ω, and ω is unmeasured (`meas-disorder-price` waits on a TCP-in-tunnel test);
  - ω adds a third dial;
  - H = 0 and H = ∞ are plain ends, while ω reaches "never release" only as ω → ∞.
- The implied price `ω̂ = δ·H/d` is printed, so the two forms can be compared on any run.
- The ω measurement does not go away: it is how the tunnel's named H is chosen (§4).

### 2.8 L7, memory and the outstanding count

```text
   resident store   ≤   rate_in · (T − ô)  +  (seqs acknowledged within one RTT)          finite T: no clamp; every entry leaves by verdict
   intake           ≤   rate_app · T                                                     L1 at the intake
   T = ∞:           store ≤ B  (paper §6.1, ADR-0084 decision 3),   intake ≤ application back-pressure (§8.7)
   pause  ⇔  ( hi + 1 − A_rx − |released| − |V ∩ [A_rx, hi]| )  ≥  B   ∨   cwnd_full
```

| symbol | provenance |
|---|---|
| rate_in, rate_app | measured |
| B | `clamp((1 + q(δ))·Σ bwᵢ·RTpropᵢ, floor, N·knee)`, paper §6.1. **PINNED at N·2048 on real links (ledger F01)** |
| V ∩ [A_rx, hi] | measured: verdicted seqs not yet passed by the ack |

- **At finite T the store needs no cap:** its age bound is a bound on its size.
- **At T = ∞ the bound is B, and B is a constant in practice** (F01, PINNED, with its knee unprovenanced). That is a defect finding under MEASUREMENT DISCIPLINE 17 and 18, carried as a ledger verdict and not a footnote. This proposal does not make it worse: it is the same B as ρ = 1 has today. But T = ∞ is the Bulk and Auto named point, so the memory law of the most-used point stays a pinned constant until the store law's time form (`store-law.md`) lands.
- **Verdicted seqs leave the outstanding count** (the `− |V ∩ [A_rx, hi]|` term). This amends ADR-0084 decision 3. Without it, an outage longer than T keeps the source paused on seqs the sender has already given up, until the first floor after the outage is acknowledged. That costs about one RTT after the outage, and for a call it means stale data in the intake ages out (L1) when it could have been sent fresh.
  - The term is 0 at T = ∞ (V = ∅), so the T = ∞ trace is unchanged.
  - It is continuous in T through V.
  - Cost: a verdicted seq whose floor is lost keeps no flow-control weight. That is safe, because it has left the store and nothing more is sent for it except floors.
  - Owner question Q6; arm S2p in §9 runs without it.

### 2.9 What δ still sets

δ keeps every job that prices delay within the lifetime:
- the rate law (r(β(δ)), the tail target t_tail(δ), §4.1–§4.5);
- Copa's delay weight in congestion control (§8.2);
- the span width through b(δ) (L4);
- the pool setpoint q(δ) (§6.1);
- the receiver's request bar `request_bar(δ)` (§7.6);
- the contract stall (§6.4).

δ loses the shed deadline D(δ) (L2 replaces it) and design U's ordering hold (H replaces it).

Two δ-and-ρ laws need their own rewrite, which this draft does not design:
- `stall(δ, ρ) = (1 − ρ)·D(δ) + ρ·(9/8·srtt + srtt)` (`store_cap.rs:331`, the three-term store law, `RWM_THREE_TERM`, default off) reads ρ. Under the proposal ρ is a reported outcome, and feeding a reported outcome back into a law makes a loop. The likely form replaces the give-up share with what L2 gives up, but that is a store-law question.
- §7.3's value of waiting, `R(T; δ, ρ, r)`, carries `P_arq(ρ, r)`. That becomes `P_arq(T)` of L5. It is a model edit only, since `request_bar` reads `P_arq = 1` on every shipped seat.

### 2.10 Continuity and limits (NO-MODE-SWITCH)

| law | in T | in H | in δ | at T = ∞ | at H = 0 / H = ∞ |
|---|---|---|---|---|---|
| L1 `a < T` | per-event state comparison; the aggregate share is continuous through the spread of ages | — | — | never true, so nothing is dropped | — |
| L2 `a + ô < T` | the same; the physical step at `a₁ + ô` is located by measured delays | — | — | always true: serve until filled, no verdict, no floor (the ρ = 1 trace) | — |
| L3 | continuous where Ŝ decreases | — | through r(δ) | share = 1 | — |
| L4 | through V only | — | b(δ), unchanged | today's span | — |
| L5 (engine) | none | — | unchanged | unchanged | — |
| L6 `min(t_det + H, t_F + H_f)` | through t_F | continuous, non-decreasing | none | t_F = ∞ | the two ends of one expression |
| L7 | through V | — | through B's q(δ) | `−\|V\| = 0`; B as today | — |

- **T = ∞ is a value.** The engine holds T as `f64::INFINITY` and evaluates `a + ô < T` and `t_h + T − ô`. IEEE arithmetic gives `true` and `+∞` with no `is_infinite()` test. A timer at +∞ never fires. The deny test (§10) forbids `is_infinite`, `is_finite` and any comparison of T or H with a literal.
- **The limit is reached at a finite T, which is a stronger check.** Once T exceeds `max_s(a_resolved(s)) + ô` in a run, every decision is the same as at T = ∞, so the trace is bit-identical, not merely close (§9, P1).
- **A per-event comparison is not a mode bit.** `age > D(δ)` in today's accepted shed law has the same shape as L2. The test CLAUDE.md asks for is that no comparison of a **knob against a constant** (`T < 1 s`, `H == 0`, `T == ∞`) selects a branch, and that the aggregate behaviour is continuous through ±2 % nudges at every named point. §9 runs those nudges.
- **Presets are points.** Every named point in §4 is a triple of numbers, and nothing downstream reads which preset produced it.

## 3. Representation and validation

- Config: `lifetime_s: Option<f64>` (T) and `hold_s: Option<f64>` (H), with "inf" accepted.
- Domains: T ∈ (0, ∞] and H ∈ [0, ∞]. NaN, negative values and T = 0 are config errors, each with a test, following the `RWM_DELTA` NaN precedent (F1, `9b8b42ca`).
- The engine never receives "absent": `config::resolve` fills the hint's named point (§4), as it does for δ today.
- **No wire field.** T is read only by the sender and H only by the receiver. The floor (v11 `WindowFloor`) carries T's effect. δ is read by both ends, as today. Each end's knob governs only its own behaviour, so a mismatched pair is two well-defined halves and not a stall. That is unlike today's ρ, where a ρ = 1 sender paired with a ρ < 1 receiver loses packets (`rec-mixed-rho-stall`).

## 4. Named points

| point | T | δ | H | source for the values | what changes against today |
|---|---|---|---|---|---|
| **Bulk** (tunnel, perf) | ∞ | 0.005 | ∞ | today's ρ = 1 and no-release | none (bit-identical, P1) |
| **Auto** | ∞ | 0.5 | ∞ | as Bulk | none |
| **Object API** (`window_out_of_order` today) | ∞ | per hint | 0 | the corner config.rs already names | none: the same corner, now H = 0 |
| **Realtime / call** | **0.4 s** | 50 | **0** | ITU-T G.114 (below); RTP's sequence numbers let the application reorder (RFC 3550) | ρ = 0.99 and the 60–300 ms / SRTT/2 holds go; skips happen at T, not on the receiver's own budget; slow-path packets are released past at N ≥ 2 |
| **buffered video** (owner's example; not a shipped hint) | 2 s | 0.005 | 0 | owner's example; not cited | — |

- **Why T = 400 ms for a call, not 150 ms.** ITU-T G.114 (05/2003) states that a one-way delay of 400 ms should not be exceeded for general network planning. The earlier editions' 150 ms ("most applications not significantly affected") is replaced in the current edition by an E-model curve (G.107). Both readings come from the ITU summary page; the full text was not checked here.
  - T is a give-up rule, so it belongs at the limit beyond which the data is useless (400 ms), not at the comfort level.
  - Keeping the typical delay near 150 ms is δ's job (δ = 50 prices every millisecond). That is the owner's "for a call a fixed latency deadline can be important": the deadline is T, and the comfort level is δ.
  - G.114 measures mouth-to-ear delay, which includes codec, packetisation and playout. T counts from hand-over to the transport, so a call application that knows its own codec delay should state less than 400 ms; 0.4 s is the transport's upper bound.
- **Why H = 0 for a call.** Real-time media carries its own sequence numbers and a jitter buffer that reorders (RFC 3550 §5.1, §6.4.1). A transport hold only adds delay in front of that buffer. Today's Realtime receiver holds SRTT/2 in order, so H = 0 is a Realtime behaviour change: at N ≥ 2 every slow-path packet is released past and arrives out of order without loss (`ordering-hold.md` §5; Q5).
- **The tunnel at Bulk and Auto: H = ∞ now.** The owner accepted "late, out-of-order delivery instead of a stall after a long outage" once the release rule ships. That corresponds to a finite H such as 1 s (outage scale) at T = ∞, and only form (b) can state it. The value is not chosen here: the inner TCP's tolerance is RFC 8985's reordering window, which starts at min_RTT/4 (§6.2 there), and the `meas-disorder-price` TCP-in-tunnel test is what tells whether a finite H costs inner throughput. Until then H = ∞ keeps Bulk and Auto bit-identical (Q2).
- **Nudge points** for the continuity checks (§9): T = 0.4·{0.98, 0.99, 1, 1.01, 1.02} and H = 0.02·{…}. H = 0 and T = ∞ are one-sided ends, checked by identity (P1, P2), not by nudges.

## 5. When the link cannot meet T

The rule is always kept (L1). What is given up is reported.

| situation | what the machine does | given up | reported |
|---|---|---|---|
| **path delay above T** (ô > T) | first sends still go (L1 passes them; L2 gates only resends); every resend fails L2; every hole gets its verdict at its first decision instant | every unit, in the T sense; units still reach the consumer late | `share_T ≈ 0`, `share_any` ≈ 1 − ε; achieved delay; a gauge `ô > T` (the fraction of decisions with `ô ≥ T`, printed, not acted on) |
| **T below one resend round** (`a₁ + ô ≥ T`) | no resend passes L2; verdict at the first report; proactive repair r(δ) is the only recovery | the FEC residue `ε·(1 − P_fec(r))` | `share_T ≈ 1 − ε·(1 − P_fec(r))`; the L2 fail count; and, as Q3 asks, the r that §4.10 would have chosen for this T (a gauge, not an operand) |
| **outage longer than T** | units sent in the outage get verdicts at `T − ô`; their floors are lost and repeat on the sweep; the first floor after the outage passes them all; unsent units age out in the intake (L1); with the L7 term the source is not held paused on given-up seqs | everything sent or produced more than T before the outage ends | `share_any`, `share_T`, `intake_expired`, the largest single-episode loss |
| **overload** (application rate above capacity) | the intake grows until its head is T old and then expires from the head (oldest first, since it is the stalest); the source is paced by cwnd and B as today | the expired head | `intake_expired` rate; achieved delay |
| **T = ∞ and any of the above** | serve until filled; the source pauses at B; nothing expires | nothing; delay instead | stall time, as today |

- **The owner's "what if we cap it but cannot fulfil it?":** the engine never sends past T, and it reports how much arrived by T and how much arrived at all. No promise is broken, because none is made.
- **The deadlock first sends avoid:** if L2 also gated first sends, a path with ô > T would send nothing, take no RTT samples and keep its stale ô. Gating first sends by L1 alone keeps the path measured. An application that would rather not send at all on such a path sees `share_T ≈ 0` and can act on it.

## 6. Mapping: what stays, what goes

| existing concept | under the proposal | ADR / paper text it amends |
|---|---|---|
| ρ, `window_rho` (Am. 1; engine pending) | **deleted as an input**; returns as `share_any`, `share_T` (L3) | ADR-0064 (the triangle; ρ a dial), ADR-0084 decision 6 and Am. 1 item 1; paper §1.2, §3.5, §5.1; CLAUDE.md "ρ is the retention contract — an INDEPENDENT dial" |
| the 1 − ρ count budget `Ab + 1 ≤ (1 − ρ)·S_src` | **deleted**; L2 decides the verdict | ADR-0084 decision 5; paper §5.6 |
| Ab, G_rx, `passed_undelivered` (v11) | **kept as gauges** (L3), no longer an operand | ADR-0084 decisions 4 and 5 (their role) |
| the cumulative prefix promise and the bank (O2), run-atomic spending (F2) | **moot**: no allowance exists | skip-rule §2, §3.3 |
| D(δ) as the shed deadline (`shed_deadline_us`, §5.6) | **deleted**; replaced by `a + ô ≥ T` | paper §5.6; ADR-0064 decision 3's form "age > D(δ) within a 1 − ρ budget" |
| D(δ) in the span law | **kept** as `b(δ)·RTT`; its `min(·, 2·RTT)` is inert and goes as a zero-behaviour-change commit | paper §5.3's constant table ("D's cap 2") |
| b(δ) | **kept**: span width, q(δ), contract stall | — |
| T_cut, `find_t_cut` (its 10·W cap is PINNED, ledger C13) | **deleted**; T is declared in seconds; §4.10 keeps the model form with T in ρ's seat (L5) | paper §3.5, §4.10 |
| T_ρ (skip-rule §3.4) | **deleted** as a rule; Ŝ and ĥ stay as the share predictor (L3) | skip-rule §3.4 (not accepted, so no ADR) |
| `shed_recv_hold` (SRTT/2, or 4·SRTT clamped to [60, 300] ms) | **deleted**; L6 | ADR-0084 Am. 3 exception row C11; paper §5.6, §11.3 |
| `recv_shed_on`, `rho1_debt_bit`, the `rho1-debt-exception` block (C9–C11, C8) | **deleted** at the receiver build; the exception's removal trigger becomes "L6 lands" instead of "H_o(δ) lands" | ADR-0084 Am. 3 decision 1 and its fallback (which is keyed on the ω Result, no longer needed for the law) |
| `window_out_of_order` | **becomes H = 0**; the flag is removed with a startup error naming `hold_s = 0` | config.rs; paper §4.11 |
| `reorder_timeout_ms` (default 20) | **deleted**. It is live only on the ρ < 1 in-order buffer, and only until the first SRTT sample: `set_timeout(shed_recv_hold(..))` overrides it before every drain (`receiver.rs:792`, `:1758`) | config.rs |
| `reorder_max_size` (force-drain at 500) | **deleted**; receiver memory is bounded by B (L6) | config.rs; ADR-0084 Am. 3 row C9 |
| `window_reliable` (already removed by ADR-0084 Am. 1; engine pending) | its meaning is T = ∞ | — |
| the hint presets Bulk/Auto/Realtime | **named points (T, δ, H)**, §4 | paper §1.2's table; ADR-0084 decision 6 (Realtime ρ = 0.99) |
| design U's ω and `H_o = ω·d/δ` | **replaced by H**; ω̂ = δ·H/d is printed; the ω measurement only informs the tunnel's H | `ordering-hold.md`; skip-rule §4.1; roadmap `model-ordering-hold`, `meas-disorder-price` |
| ADR-0084's last copy with every verdict (Am. 2) | **amended**: the verdict sends the floor alone; the last copy is the last resend L2 admitted (Q4) | ADR-0084 Am. 2 decision 1 |
| the floor tell, its repeat, the pair-merge rule | **kept** | ADR-0084 decision 1, Am. 2 decision 3 |
| the source-rate bound `pause ⇔ O ≥ B` | **kept, with verdicted seqs out of O** (L7; Q6) | ADR-0084 decision 3 |
| the floor-pass hold (floor-hold W, or tail-and-hold E0) | **kept as H_f**, whichever the owner confirms | — |
| F1 (a copy-less shed is slower than serving) | moot under L2: no resend could arrive in time when the verdict is taken | skip-rule §3.1 |
| §4.11's H (the reorder share of a deadline; "H → ∞ is the out-of-order corner") | **notation clash**: that H is a lateness tolerance, so its out-of-order end is ∞, while the owner's H is a hold, so its out-of-order end is 0. §4.11 should rename its H when the paper is edited | paper §4.11; config.rs's comment on `window_out_of_order` ("the H → ∞ corner") |
| `inner_feedback_weight`, the Realtime duplicate send, `use_packing` | unaffected; the last two stay recorded hint debts | — |

## 7. Open questions for the owner

Each question has a recommended default.

- **Q1, T's origin.** Count T from hand-over (intake admission) or from the first send? **Default: hand-over**, as proposed. It needs one new stamp per source unit, and it makes the intake obey T (L1).
- **Q2, the tunnel's H at Bulk and Auto.** **Default H = ∞** (no change) until `meas-disorder-price` reads a TCP-in-tunnel result. Then a finite H (outage scale, about 1 s, or the inner RACK window) may be chosen on that result.
- **Q3, should the engine's r read T?** **Default: no, in the first build.** r(β(δ)) stays, and §4.10's (t_tail, T) → r is printed as a gauge. Reading T in the rate law is a separate rate-law change with its own battery.
- **Q4, the last copy.** **Default: no copy at the verdict** (amends ADR-0084 Am. 2): under L2 it would arrive after T. The alternative keeps the copy for late delivery (`share_any`) at the cost of one wire symbol per verdict. The toy measures it (S2c).
- **Q5, Realtime's H.** **Default H = 0** (calls reorder in their own jitter buffer). It changes Realtime at N ≥ 2 (slow-path packets are released past); a small H such as the path skew is the alternative. Either way the value is a declared number, not a law.
- **Q6, verdicted seqs out of the outstanding count** (L7). **Default: yes.**
- **Q7, the resend cost ratio.** **Default: keep `W_COST_RATIO = 1`**, which makes L2's bar the median one-way delay.
- **Q8, ô's symmetric split.** **Default: SRTT/2 until receiver timestamps ship**, with the asymmetry gauged then.
- **Q9, order of work against the pending ADR-0084 sender build.** That build implements the 1 − ρ budget this proposal deletes. **Default: decide this proposal before the build's shed step (brief steps 2–5) is written**, so the budget is not built and then removed. The verdict's tell, the store at every ρ, the floor and the pair merge carry over unchanged.

## 8. What in the proposal does not hold as stated

1. **"Stop trying at age T" together with "keep the last copy".** A copy sent at age T arrives at about T + owd, which is useless under a deadline. And "a resend is worth sending only if it can arrive before T" needs a one-way delay the sender does not measure. Resolution: L1 is the exact rule (no sending at age ≥ T); L2 adds the arrival refinement with ô = SRTT/2 (a declared split); the last copy goes (Q4).
2. **"H = T: strict order, forced for a byte stream / TCP tunnel".**
   - The tunnel carries IP packets, and the inner TCP tolerates reordering, so order is not forced there; only the current default keeps it.
   - A true byte-stream consumer cannot pass a gap at all, so it forces **T = ∞** as well as strict order. No such consumer ships today: the object API reassembles by offset, and the only byte-stream mention is a comment (`receiver.rs:157`).
   - H = T at the receiver needs T at the receiver, which form (b) avoids.
   - Strict order is exact only at T = ∞: at finite T a late fill after the floor pass arrives out of order (L6).
3. **Form (a), `H = h·T`, is unworkable at T = ∞** (§2.7). Form (b) also needs no min(H, T).
4. **Hand-over is not first send.** The source pause separates them. So T needs an intake stamp and an intake expiry (L1), and the pause must not count given-up seqs (L7). Without the stamp, T silently becomes "T from first send", and an outage turns it into "T after the pause".
5. **"r stays derived from δ and the channel" is true of the engine but not of the model.** §4.10's own example raises r when the deadline is below a round trip. Under the proposal as stated, an application with T below a round trip and a low δ gets the FEC residue as its loss. That is reported, and whether it should move r is Q3.
6. **The rejected round-trip cap was already inert in the span law** (b ≤ 2). The cap that did act was D as a give-up age, and L2 removes it.
7. **At T = ∞ the memory bound is the pinned constant B** (F01). That is not introduced here, but it is now the memory law of every T = ∞ point, and it is carried as a defect.

## 9. Registration for the offline simulation (toy stage 3)

Built on `tools/toy/` at branch `toy-sim-stage2` `c0f33f8e`. Add the new arms to `toy2.py` and a new runner `batch3.py` with the same structure as `batch2.py`. The stage 1 and stage 2 code paths must stay bit-identical (their tests must pass unchanged).

### 9.1 What the toy cannot test

The toy has one path and no proactive repair. "Repair" in `repair_per_delivered` is resends only. So:
- L5 and "only repair helps" are tested only in their ARQ half: below one round the share must equal what the originals deliver (P5);
- nothing here tests r, N ≥ 2, H_f other than the FIFO value 0, or ô against an asymmetric path.

### 9.2 New code (exact)

**Config fields** (`Cfg`):
- `T: float = math.inf`;
- `Hk: float = math.inf` (the order hold H);
- `o_excl_verdict: bool = True` (the L7 term; arm S2p sets it False).

`sender` gains the values `"S2"` (L1 and L2, no copy) and `"S2c"` (as S2 plus one copy with every verdict). `recv` gains `"RH"`. The new arms run with `rho = 1.0` and `check_p1 = False`. Infinity is `math.inf` everywhere, with no `is None`, `isinf` or `isfinite` test.

**Hand-over and the intake (L1).**
- New state: `u_next = 0` (the next application unit), `h_seq = [0.0]*nmax` (the hand-over time of each seq), `intake_expired = 0`. Unit u is handed over at `h(u) = u·dt`.
- In `emit(t)`, before assigning the seq: while `u_next < units_total` and `intake_expired_fn(c.T, t − u_next·dt)` holds, do `intake_expired += 1; u_next += 1`.
- If `u_next ≥ units_total`, the source is done: `src_next = inf`, and nothing is emitted.
- Otherwise seq `s = n_sent` takes unit `u_next`: `h_seq[s] = u_next·dt; u_next += 1`, and the rest of `emit` runs as today.
- `units_total = n_total` in object mode, and `int(dur·rate)` in streaming mode.
- The existing `done` tests become `u_next ≥ units_total` (object mode) and `nxt ≥ dur` (streaming mode, unchanged). The `t_front` completion test `self.n_sent >= self.nmax - 16` becomes `self.src_done and …`, where `src_done` is set when the source finishes. For every existing arm (no expiry, `u_next == n_sent`) these are the same conditions, so stage 2 stays bit-identical.
- Apply this intake logic for every sender. At `T = inf`, `intake_expired_fn` is always false, so S0 and S1c are unchanged.

**The two law functions** (module level; the only places T is compared; both added to the deny test's `EXEMPT` with a comment citing this section):

```python
def intake_expired_fn(T, age):      # L1
    return age >= T

def resend_admissible(T, age, x):   # L2
    return age + x < T
```

Add `"T"` and `"Hk"` to `DIALS` in `test_toy2.py`, so any other comparison on them fails the deny test.

**Sender S2 / S2c (in `act`).**
- Where `c.sender != "S0"` today, add an arm for `c.sender in ("S2", "S2c")`: for each candidate s, if `resend_admissible(c.T, now − h_seq[s], self.rtt/2)` it goes to `serve_seqs`, otherwise to `verdict_seqs`. There is no allowance, no D, and no `A_avail`.
- `x = self.rtt/2` is the exact owd, the same idealisation as the D_RTprop arm.
- The verdict copy block runs for `c.sender in ("S1c", "S2c")`.
- At `T = inf` every candidate is served, so the calls and their order equal S0's.

**The per-seq verdict timer (S2, S2c).**
- In `emit`, push an event of a new kind 8 at `h_seq[s] + c.T − self.rtt/2` with payload s. At `T = inf` this time is inf and the event never fires.
- On firing: if `s ≥ self.A_known` and (`s in self.M_known` or `s > self.rhi_known`) and `s not in self.verdicted`, call `self.act(now, now, [s], open_tail=True)`. `act` takes the verdict, since L2 fails.
- After any verdict under S2 with `o_excl_verdict`, re-check the pause, as `on_report` does: if `self.paused and self.O_eff() < self.B`, resume.

**The outstanding count.** `O_eff() = O() − (len(self.pending) if o_excl_verdict else 0)`, used in place of `O()` at both pause sites for S2 and S2c. Write it so that S0 and S1c call the unchanged `O()`, for example as a method selected once in `__init__`. The `len(pending)` term is 0 at T = inf.

**Receiver RH.**
- `ho_now()` returns `self.cfg.Hk` for `recv == "RH"`. The existing `new_hole` then pushes the release event at `now + Hk`; at `Hk = inf` that event never fires.
- The floor pass is unchanged (`HEst`, W = 10 s; 0 on the toy's FIFO path).
- Add gauges `rel_by_hold` (the existing `rel_ho`) and `rel_by_floor` (floor passes of holes not yet released).

**New readouts** (in `summary`):
- `units = units_total`, `intake_expired`;
- `share_any = delivered / units`;
- `share_T = |{s : dlv[s] ≥ 0 ∧ dlv[s] − h_seq[s] ≤ T}| / units`;
- `finish = t_front − h(units − 1)` in object mode;
- `l2_fail` (the count of verdicts L2 took), `rel_by_hold` and `rel_by_floor`.

Existing readouts keep their definitions: `v_mean`, `v_p50`, `v_p99` (wait beyond the loss-free arrival), `ooo_per_delivered` (the out-of-order count per delivered), `repair_per_delivered`, `stall_n`, `stall_total`, `stall_p99`, `tail_time` and `verdicts`.

**Tests to add to `test_toy2.py`, red first:**
1. T = inf S2 + RH(inf) equals S0 + R1, trace and readouts, on I1 for 3 seeds.
2. T = 1e6 equals T = inf (same).
3. L1 absolute: in a traced S2 run at T = 0.05, no trace send has `t − h_seq[s] ≥ T`.
4. L2 absolute: no copy is sent at `age + owd ≥ T`.
5. RH(Hk = 0): `hol_sum_nonhole == 0`.
6. The deny test, with a positive control `"def f(c):\n    return c.T < 1\n"` flagged.

### 9.3 Cells (all one path, 12 seeds, phases drawn per seed as stage 2, δ = 50 unless stated)

| cell | definition |
|---|---|
| E-I2 | `batch2.tail_cfg("I2", …)`: object of 12 147 units at 10 000/s, owd 5 ms, end outage `dead = (n − 347, (n − 347)/10 000, 1.5)` |
| E-L10 | the same at L10, owd 50 ms |
| M-I2 | `n_total = 12147`, owd 5 ms, mid outage `dead = (0, 0.4·12147/10000, 1.5)`, as batch2's MID |
| M-L10 | the same at L10, owd 50 ms |
| K-I2 | the call case: cell I2, `rate = 50`, `n_total = 3000` (60 s of 20 ms frames), owd 5 ms, `dead = (0, 20.0, 1.5)`, `t_limit = 80` |
| K-L10 | the same at L10, owd 50 ms |
| K-L10b | K-L10 with owd 60 ms (the P5 shift check only) |

### 9.4 Arms per cell

- **Comparison:**
  - C1 = S1c + R0e at ρ = 0.99 (stage 2 defaults: global hold, engine ε̂);
  - C2 = S1c + R1 at ρ = 0.99;
  - C3 = S1c + R1 at ρ = 1.
- **Grid:** S2 with T ∈ {0.4, 2.0, inf} × RH with H ∈ {0, 0.02, inf}: 9 arms.
- **Limit and identity arms:**
  - S2 T = 1e6 + RH(inf);
  - S2 T = 0.4 + R1 (for P2);
  - S2c T = 0.4 + RH(0) and RH(inf);
  - S2p (`o_excl_verdict = False`) T = 0.4 + RH(inf).
- **Nudges** (every cell): T = 0.4·{0.98, 0.99, 1, 1.01, 1.02} with H = 0.02, and H = 0.02·{…} with T = 0.4.
- **Positive control** (E-I2, E-L10): T = 1.5·{0.98, 0.99, 1, 1.01, 1.02} with H = inf.
- **Round sweep** (H = 0):
  - K-L10 and K-L10b: T ∈ {0.10, 0.14, 0.16, 0.17, 0.18, 0.20, 0.22, 0.25, 0.30, 0.40};
  - K-I2: T ∈ {0.010, 0.020, 0.030, 0.035, 0.040, 0.050, 0.10}.
- **A/A for the call cells:** S2 T = 0.4 + RH(0.02) at variant 0 and variant 1, frozen as `nf_stage3.json` before any decision cell runs. E and M cells reuse `nf_stage2.json`'s Part T floors.

Rough count: 6 cells × 24 arms, plus the sweeps, controls and A/A, comes to about 190 configurations, or about 2 300 runs. That is under two hours at stage 2's 1–3 s per run.

### 9.5 Readings

Reading rules:
- Every comparison uses `summarize2.read` on paired per-seed differences, with bar `m = max(T_bar, NF)`.
- `T_bar` is `summarize2.T_of` with C3 as the reference. For `share_any` and `share_T`, `T_bar = max(1/units, 0.05·(1 − share_C3))`.
- K and P are `summarize2.kp_stats` (skip-rule §7.9 on `toy-sim-stage2`).

### 9.6 Predictions (stated before running)

A miss is a finding to explain, never a reason to tune.

- **P1, T → ∞ is a limit (identity).** In every cell and seed, S2 T = inf + RH(inf) and S2 T = 1e6 + RH(inf) equal C3 on every readout exactly (float-equal), and on the sender trace for seeds 0–2. PASS: all equal. FAIL: any difference.
- **P2, the two ends of L6.**
  - S2 T = 0.4 + RH(inf) equals S2 T = 0.4 + R1 on every readout exactly.
  - S2 + RH(0) has `hol_sum_nonhole = 0` and `v_p50 = 0` exactly in every cell.
- **P3, continuity at the named points.** On `share_any`, `share_T`, `v_mean`, `v_p99`, `ooo_per_delivered` and `repair_per_delivered`, K and P read "equivalent" for both nudge sets in all six cells. FAIL: any "shown". A "shown" P on a curved readout triggers the registered two-point follow-up (§7.9 there).
- **P4, positive control.** K on `share_T` across T = 1.5 s reads **"shown"** in E-L10 and E-I2. That T lies on the tail seqs' fill age just after the 1.5 s outage. If K reads "equivalent", the statistic has no power on this readout, which is a finding.
- **P5, the round boundary is measured, not declared** (K cells, H = 0):
  - `share_T(T)` is non-decreasing over the sweep (each adjacent difference ≥ −m);
  - below the first round (K-L10 T ≤ 0.14; K-I2 T ≤ 0.020), `repair_per_delivered = 0` exactly, and `share_T = (1 − p0)·(1 − 75/3000)` ± 0.005, that is 0.8775 at L10 and 0.9458 at I2;
  - the rise to 90 % of the T = 0.4 value lies in T ∈ [0.16, 0.20] at K-L10 and in [0.03, 0.05] at K-I2, against the hand prediction `T* = 3·owd + 1/rate` (+ ≤ 3 ms of tick and jitter): 0.170 s and 0.035 s;
  - **in K-L10b the rise moves by +30 ms ± 5 ms** (3·Δowd). The step follows the path, not a constant.
- **P6, T = 2 s against T = ∞** (E and M cells, H = inf): `share_any`, `v_mean` and `tail_time` read equivalent. Every outage fill lands before age 2 s.
- **P7, T = 0.4 s in the outage cells:**
  - E cells: `share_any = 1 − 347/12147 = 0.9714` ± 0.002. In E-L10, `tail_time` is shorter than C3's by at least 0.1 s (shown): the floor passes the dead tail at the first sweep after the window, at about 1.52–1.62 s, against C3's 1.72–1.81 s. In E-I2 it is not longer than C3's (equivalent or smaller).
  - M cells: `share_any ∈ [0.39, 0.41]`. Everything after the 4 860 units sent before the outage was sent into it, or handed over more than T before it ended.
  - `intake_expired` is near 0 for S2 and above 4 000 for S2p. Same share, different reason: the L7 term at work.
- **P8, the call against today** (K cells):
  - `share_T`(S2 T = 0.4, H = 0) ≥ `share_any`(C1) − m;
  - `v_mean`(S2, H = 0) < `v_mean`(C1), shown;
  - `ooo_per_delivered`(H = 0) > (H = inf), shown;
  - `stall_total`(H = 0) ≤ 1e-9 s;
  - `stall_p99`(S2, H = inf) ≤ 1.6 s, against C2's: C2 serves the 45 outage units beyond its 30-unit allowance until filled, so its stall is at least the outage.
- **P9, the copy at the verdict (S2c against S2, T = 0.4):**
  - `share_T` reads equivalent (the copy arrives after T);
  - `share_any`(S2c) ≥ `share_any`(S2);
  - `repair_per_delivered` is higher by `verdicts/delivered` ± m.

Report every reading, the A/A self-check as in stage 2, and every deviation.

## 10. Engine build sketch (not implemented)

1. **ADR first:** a new ADR, "three application knobs", superseding in part ADR-0064 and ADR-0084 (decisions 3, 5 and 6, Am. 1 item 1, Am. 2 decision 1, Am. 3's exception and fallback). Then paper §1.2, §3.5, §4.10, §4.11 (notation), §5.1, §5.3 (the inert cap), §5.6, §7.3 and A.4. The CLAUDE.md amendment is the owner's.
2. **Zero-behaviour-change commits first:**
   - delete `.min(2.0 * rtprop)` in `emit_source.rs` (the binid workflow must show an identical `.text`, or a `binid: expected-change` line);
   - add an intake hand-over stamp `t_h` carried in the store entry, still unused.
3. **`raptorpath-core`** (I/O-free, wasm32-clean, ADR-0077): `fn resend_admissible(age_s, one_way_s, lifetime_s) -> bool` and `fn intake_expired(age_s, lifetime_s) -> bool`, each with the ±2 % continuity test at T = 0.4, an absolute test at N = 1 and N = 2, and the T = ∞ identity.
4. **Config** (§3): `lifetime_s`, `hold_s`, named points, domain tests; `window_out_of_order`, `reorder_timeout_ms` and `reorder_max_size` become startup errors naming their replacement.
5. **Sender:**
   - L1 at the intake;
   - L2 at `serve_gaps`, the tail sweep and the request server (C6 (a));
   - the per-seq verdict timer;
   - the verdict sends the floor alone;
   - O excludes V (L7);
   - `shed_allowed`, `shed_deadline_us` and the budget refresh at `emit_source.rs:629–640` are deleted.
6. **Receiver:** L6 replaces C9–C11. `shed_recv_hold`, `recv_shed_on`, `rho1_debt_bit` and every `rho1-debt-exception` row go, and `rho1_debt_is_gone` is un-ignored.
7. **Gauges:** `share_any`, the `share_T` estimate, `intake_expired`, `l2_fail`, `ô ≥ T`, `rel_by_hold` and `rel_by_floor`, the resident store maximum against rate·T, and B's bind fraction at T = ∞.
8. **Tests:**
   - a sim test that T = ∞ (and T = 1e6) is trace-identical to today's ρ = 1 on the `sim_lost_tail` and `sim_shed_reach` cells;
   - L1's absolute test (no staged datagram at `a ≥ T`);
   - an engine deny test extending ADR-0084 Am. 1 item 2's, which fails on any comparison of `lifetime_s` or `hold_s` with a literal and on `is_infinite` or `is_finite` of either, outside the two core functions.
9. **Guard:** a behaviour change at every point except T = ∞, so it rides a §41c guard with off-Bulk stages and execution witnesses (`meas-guard-bulk-blind`). Bulk and Auto are covered by the trace identity.

Owed with the merge of this derivation: a roadmap node (`model-three-knobs`, waiting on the owner's answers in §7 and on toy stage 3) in `docs/roadmap/nodes.yaml`. It is not added on this branch, so the coordinator can place it against `model-ordering-hold`, `meas-disorder-price` and the ADR-0084 sender build, which it would reshape.
