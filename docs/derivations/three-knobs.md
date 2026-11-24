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
   stage any payload-bearing datagram for s   ⇒   a(s)  <  T ,          a(s) = now − t_h(s)
```

| symbol | provenance |
|---|---|
| T | declared; T ∈ (0, ∞], with ∞ a value and not a flag (§3) |
| t_h(s) | measured: stamped once, when the intake admits s. This is new; the engine stamps first send today (`emit_source.rs`, `src_send_ts_us`) |
| now | measured |

- **Unsent data is covered too.** A unit still in the intake when `a ≥ T` is dropped there, counted as `intake_expired`, and never gets a seq. This is the same rule at a different seat. Without it a source pause (ADR-0084 decision 3) turns T into "T after the pause ends", which is not what an application means by "stop trying after T".
- **This is an absolute invariant**, not a target, so it gets an absolute test: no payload-bearing datagram (source, resend, coded repair member) carries a seq whose age at staging is at least T. Floors carry no payload and repeat past T by design (ADR-0084 decision 1). It holds whatever the link does, which answers "what if we cap it but cannot fulfil it?". The engine keeps the rule, and the share it achieves is reported (L3).
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
| ô = SRTT/2 | measured SRTT (`est.rtt()`; before the first sample it is main's 50 ms seed, so ô = 25 ms then, the seed defect skip-rule and tail-and-hold already record), with the even split between the two directions as a **declared assumption** (a symmetric path). The measured replacement is a receiver timestamp (ADR-0078's planned field, not yet on the wire). On an asymmetric path the error is half the asymmetry, and a gauge reports it once receiver timestamps exist |
| "unresolved" | measured: in a reported gap or the open tail, and not acknowledged (ADR-0084 decision 1) |
| decision instant | a gap report, a tail-sweep fire, or the per-seq verdict timer at `t_h(s) + T − ô` (below) |

- **Why `a + ô < T` and not a probability.** The full form is "send if P(this resend arrives before T) is worth one wire symbol", that is `P(owd ≤ T − a) ≥ w/(1 + w)`, with `w = W_COST_RATIO = 1` (`net/late.rs:64`, a register row). With the normal RTT model of §3.2 and an even split, that probability reaches ½ exactly at `T − a = SRTT/2`, so the full form reduces to the line above. The reduction adds no constant; it uses one that already exists. If the owner wants a different cost ratio, the bar moves to a quantile of the RTT spread instead of the median. Recommended: keep w = 1 (§7, Q7).
- **The verdict has its own timer.** Reports and sweeps are coarse: in a 50-packet/s call at 100 ms RTT the sweep fires every 100 ms. A timer at `t_h(s) + T − ô`, re-checked when it fires against the current ô, gives the verdict at its own instant. ADR-0084's tail sweep stays the repeat clock for the floor.
- **Below about 1.5·RTT the timer fires before the sender can know a seq arrived.** The timer fires at age `T − ô`, while a delivered seq's acknowledgement returns at about one RTT plus the report delay. When `T − ô` is below that, every seq still looks unresolved when its timer fires, so it gets a verdict whether or not it was lost.
  - Example: a 100 ms RTT call at T = 0.14 s fires at age 0.09 s, against an ack at about 0.10 s. The same holds at the 0.4 s call point once the RTT exceeds about 270 ms.
  - Such a spurious verdict is harmless to data. Nothing more could have been sent anyway (L2 fails), the store entry would have left on the ack a moment later, and the receiver ignores a floor below data it holds.
  - Its one cost is floors. Rule: **a verdict taken by the timer sends no floor of its own; the floor goes on the next tail-sweep fire** (ADR-0084's repeat clock, period `clamp(2·RTT, 25 ms, 100 ms)`) or with the next report-driven verdict. That bounds floors to about one per sweep period, and delays a floor pass by at most one sweep period, which matters only to strict-order streams (H = ∞).
  - `floors_per_delivered` is reported, with a prediction (§9, P10).
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

- **T reaches the span only through V.** A verdicted seq has left the store and is no longer unresolved, so the span's lower edge F can never be older than about `T − ô`. A repair therefore never codes a member that L2 has given up. No T term enters the formula. In effect, though, V cuts the span's lower edge at `T − ô`, which is an implicit `min(b(δ)·RTT, T − ô)` on the span's reach. So it gets a bind gauge: `span_cut_by_V`, the fraction of granted repairs whose span would have started below F had V been empty.
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
- **Why H = 0 for a call.** Real-time media carries its own sequence numbers, which let the receiving application restore order (RFC 3550 §5.1). That the application's playout buffer reorders is common practice, not checked here against a specific text. A transport hold only adds delay in front of that buffer. Today's Realtime receiver holds SRTT/2 in order, so H = 0 is a Realtime behaviour change: at N ≥ 2 every slow-path packet is released past and arrives out of order without loss (`ordering-hold.md` §5; Q5).
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
- The existing `done` tests become `u_next ≥ units_total` (object mode) and `nxt ≥ dur` (streaming mode, unchanged). `src_done` is set where `src_next = inf` is set for completion.
- The `t_front` completion test in `advance` becomes, in full: `if self.t_front is None and self.src_done and not self.holes and self.rhi + 1 >= self.n_sent and self.cfg.n_total:`. The old test, `self.n_sent >= self.nmax - 16 and not self.holes and self.rhi + 1 >= self.nmax - 16`, can never hold once units expire in the intake (`n_sent < n_total`), and S2p would then read as stalled. For every existing arm (no expiry, `n_sent == n_total == nmax − 16` when done) the two tests are the same condition, so stage 2 stays bit-identical.
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
- On firing: if `s ≥ self.A_known` and (`s in self.M_known` or `s > self.rhi_known`) and `s not in self.verdicted`, take the verdict **without a floor**: `verdicted.add(s); pending.add(s); g["verdicts"] += 1; g["l2_fail"] += 1`, plus one `send_copy(now, s)` for S2c. The floor goes on the next `on_sweep` (its `floor_repeat`) or with the next report-driven `act` (§2.2's timer rule).
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
- `l2_fail` (the count of verdicts L2 took), `rel_by_hold`, `rel_by_floor`, and the existing `floors_per_delivered`.

Existing readouts keep their definitions: `v_mean`, `v_p50`, `v_p99` (wait beyond the loss-free arrival), `ooo_per_delivered` (the out-of-order count per delivered), `repair_per_delivered`, `stall_n`, `stall_total`, `stall_p99`, `tail_time` and `verdicts`.

**Tests to add to `test_toy2.py`, red first:**
1. T = inf S2 + RH(inf) equals S0 + R1, trace and readouts, on I1 for 3 seeds.
2. T = 1e6 equals T = inf (same).
3. L1 absolute: in a traced S2 run at T = 0.05, no data trace entry has `t − h_seq[s] ≥ T`. Data entries are kinds 0 and 1 in the trace's last field. Floor entries (kind 2) carry the floor value in the seq slot and are excluded.
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

- **P1, T → ∞ is a limit (identity).** In every cell and seed, S2 T = inf + RH(inf) and S2 T = 1e6 + RH(inf) equal C3 exactly (float-equal) on these keys: `n_src, delivered, never_delivered, open_end, v_mean, v_p50, v_p99, v_p999, e_mean, e_p99, hol_sum_nonhole, n_holes, stall_n, stall_total, stall_p99, repair_per_delivered, floors_per_delivered, ooo_per_delivered, tail_time, t_last`. They also equal it on the sender trace for seeds 0–2. The config echo and the `gauges` dict differ by arm (`refusals`, `age_gate_fail`, `p1_checks`) and are not compared. PASS: all equal. FAIL: any difference.
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
  - outside the outage window (units handed over before 19.6 s or after 21.5 s): `share_T`(S2 T = 0.4, H = 0) ≥ `share_any`(C1) − m. Over the whole run no sign is predicted: S2 loses every outage unit (`share_T ≈ 0.975`, P5), while C1 fills some of them late. Both are reported;
  - `v_mean`(S2, H = 0) < `v_mean`(C1), shown;
  - `ooo_per_delivered`(H = 0) > (H = inf), shown;
  - `stall_total`(H = 0) ≤ 1e-9 s;
  - `stall_p99`(S2, H = inf) ≤ 1.6 s, against C2's: C2 serves the 45 outage units beyond its 30-unit allowance until filled, so its stall is at least the outage.
- **P9, the copy at the verdict (S2c against S2, T = 0.4):**
  - `share_T` reads equivalent (the copy arrives after T);
  - `share_any`(S2c) ≥ `share_any`(S2);
  - `repair_per_delivered` is higher by `verdicts/delivered` ± m.

- **P10, floors.**
  - At T = 0.4, `floors_per_delivered` is at most C2's + m in the E and M cells.
  - In K-L10 below 1.5·RTT (T ≤ 0.14), where the timer gives spurious verdicts (§2.2), it is at most 1/(P·rate) = 0.2 per delivered (one floor per 100 ms sweep against 50 units/s), not about 1.

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

## 11. Amendment 1 to the registration (2026-11-23, before any run; reviewed with the advisor)

Every change here is made before a single stage-3 run. It supersedes §9 where it says so.

1. **Common-footing readouts (amends §9.2 and P8).** `share_T` reads each arm's own T, so for an arm at T = ∞ it equals `share_any`, and P8 would compare "by 0.4 s" against "ever". Every arm, C1–C3 included, also reports two fixed-reference shares, measured from hand-over: `share_by_0p4 = |{s : dlv[s] ≥ 0 ∧ dlv[s] − h(s) ≤ 0.4}| / units` and `share_by_2 = …≤ 2.0…`. Hand-over h(s) is the unit's application time `u·dt`, defined for the comparison arms too (their seq s carries unit s, since nothing expires at T = ∞). P8's first bullet is restated on `share_by_0p4` for both arms over the whole run and outside the outage window, with no sign predicted over the whole run; both are reported.
2. **Delay from hand-over (amends §9.2).** The existing `v` is measured from the loss-free arrival of the *first send* (`toy2.py` `emit`: `atilde[s] = t + delay`), so the time a unit waits unsent while the source is paused is invisible to it. That flatters every never-give-up arm. Every arm also reports `w = dlv − h − owd` (mean, p50, p99) over delivered units. `v` keeps its definition, for continuity with stage 2.
3. **Events at time ∞ (worker note).** The per-seq verdict timer and the RH release are pushed at `+∞` when T or H is ∞. The event loop, `t_last` and any drain-to-empty must stop at the run's time limit or completion and never at an event time; the fix goes in the loop bound, never as an `isinf` on a knob (the deny test forbids that).
4. **Run order.** The new tests and the P1/P2 identities run first; if any is red, the worker stops and reports. Then the K (call) cells, then the E and M cells.
5. **Overload cell (adds to §5, §8 and §9.3).** Under sustained overload the intake expires from the head at age T, so every unit that is sent leaves at age ≈ T and arrives after T: `share_T` falls towards 0 instead of degrading in proportion to the excess. This is a defect of L1 as written, not a property of the link. Added as a reported-only cell `O-I2`: cell I2 at rate 1.2× the toy link's capacity for 10 s, T = 0.4, H = 0, against C1. Prediction P11: `share_T` < 0.2 for S2. If the toy cannot throttle the link below the source rate, the cell is reported as not run. The fix (expire the newest instead of the oldest, or admit at the intake only what can be sent before T − ô) is an owner question (Q11), not designed here.
6. **Owner questions added to §7.**
   - **Q10, Realtime's T.** At T = 0.4 s an object sent at Realtime no longer completes across an outage longer than 0.4 s. P7 predicts a share of about 40 % in the mid-stream outage, against 99 % today. Every existing Realtime test and harness cell is an object transfer. **Default: T = 0.4 s for the call point, with object transfers at the Realtime hint taking T = ∞ unless they state a T**, to be confirmed by the owner.
   - **Q11, overload at finite T** (item 5).
7. **Correction to §4 and §6 ("Object API: no change").** `window_out_of_order` also has a sender-side reader: it turns off the plain in-order Copa feed (`net/mod.rs:936–941`, `plain_inorder`). Mapping it to a receiver-only H = 0 therefore changes the sender at that point. The mapping row now reads "becomes H = 0 at the receiver; the sender reader at `net/mod.rs:941` needs its own form (open)".
8. **Not testable here (adds to §9.1).** The toy has no proactive repair, so it cannot show how a call fares when T is below about 1.5 round trips. The G.114 values were read from the ITU summary page only.

## 12. Findings from the offline simulation, stage 3 (2026-11-23, reviewed with the advisor)

Branch `toy-stage3-knobs` `39efa6a3`, `tools/toy/results-stage3.md`. One path, no proactive repair, no link capacity.

**What holds.**
- T = ∞ (and T = 10⁶ s) is float-identical to ρ = 1 on every registered readout and on the sender trace (P1, 144/144). RH(∞) equals R1 and RH(0) has no hold (P2).
- The outage shares came out as predicted (P7): about 97.1 % with an end outage, about 40 % with a mid-stream outage at T = 0.4 s (Q10).
- Below one resend round nothing is resent and the share equals what the originals deliver (P5 bullet 2).

**What the lifetime buys is not shown.** On the same-footing readout (share delivered within 0.4 s of hand-over), call cells, 3 % | 10 % loss:
- today (C1) 0.9785 | 0.9705; the sender-only receiver at 0.99 (C2) reads the same; full delivery (C3) 0.9811 | 0.9766;
- T = 0.4 s with H = 0 is identical to T = ∞ with H = 0 (0.9815 | 0.9794); with strict order T = 0.4 s adds +0.0003 | +0.0019.

So the gain over today comes from the sender not giving up early, not from the lifetime. With no link capacity, giving up frees nothing for other data, so T's expected benefit is untested here, not disproved. The lower mean waits of the T = 0.4 s arms are over delivered units only and leave out the given-up outage units (about 75 × ~1 s ÷ 3000 ≈ 25 ms of the difference); they are not a finding.

**Failed or unsupported checks.**
- **Continuity is not demonstrated.** The positive control failed (P4): the nudges at T = 1.5 s missed the fill step (fills at 1.53 s and 1.80 s), so P3's pass is flat lines with no shown power. Worker decision 9 also changed the share bar in the outage cells from the registration; P4 is to be re-read under the registered bar.
- **The resend crossover's shift with the path is not confirmed** (P5 bullets 3 and 4): the registered shift read 0–16 ms against +30 ms. The early rise comes from the tail sweep at its clamped 100 ms period, a constant. A mid-level crossing that moves 29 ms was found after the run and is not registered.
- **Overload is unmeasured** (P11). The cell is not a valid overload for S2 (share 0.975 against < 0.2 predicted), by §11 item 5's own "cannot throttle" clause. The unregistered 3 s repeat throttles only the old arm and is not used.
- **A lifetime does not shorten the strict-order stall.** P8's last bullet passed on its number, but the longest stall is identical for C2 and S2 at H = ∞ (1.53 s, 1.92 s): floors are lost in the outage too.
- **No bandwidth claim** on the call cells: resends there are sweep-driven and bimodal across phases (about 0.19 or 0.39 per delivered).

**New defect D1: the verdict timer fires on data in flight, and with L7 the pause stops braking.** In the overload cell the send age pins at 0.385 s; the timer fires at send age + 10 ms, before the ack returns, so 1.66 M verdicts are taken on data that was delivered, and since verdicted seqs leave the outstanding count (L7, Q6) the pause at B is released by the timer, not by acks (mean delay pinned at 353 ms, zero intake expiries). L2's timer needs a resolved-or-not test it cannot make before one round trip, and L7 needs a different brake (or no exclusion while the seq is within one RTT of its last send). Open; to be derived before any engine sketch.

**Deviations.** `nf_stage3.json` was rewritten once after the call runs and before any reading (three keys added); a third exempt comparison `within_lifetime` was added for `share_T`; the verdict timer is pushed at `max(t, h + T − ô)`.

**Follow-ups, to be registered before running:** a positive control at the measured fill age; a capacity-limited link cell, where giving up can free capacity (the case T is meant for); D1's fix.

## 13. Revision 1 (2026-11-24, after stage 3; owner: "let fable make a revised design that improves these things")

This section supersedes earlier sections where it says so. Sections 1 to 12 are kept as the record. The advisor (Fable) reviewed these choices before they were written:
- the verdict rule and the brake (§13.1), including withdrawing its own earlier advice to take verdicted seqs out of the outstanding count;
- the overload intake rule, and the finding that delay pinned near T cannot be fixed (§13.2);
- assigning T by what is handed over (§13.3);
- the crossover timer and the unclamped shift cells (§13.4);
- the strict-order stall: decompose it, and expect no cheap law (§13.5);
- the backlog-drain gain formula (§13.6);
- the stage 3b traps (§13.7): the source model, selecting the link by a flag, pinning B, the positive control's placement procedure, the noise floors, the cell size against C1's cost, and quantile-only readouts.

The choice of the full ack timeout R as the dequeue margin (instead of R/2) is mine, made after that review. It was checked in the final review of this section.

One quantity is used throughout. It is the time after which an unacknowledged send is overdue, the ack timeout of RFC 6298 §2 and RFC 9002 §6.2.1:

```text
   R  =  srtt  +  max( 4 · rttvar ,  g )  +  ack_delay_max
```

| symbol | provenance |
|---|---|
| srtt, rttvar | measured, by RFC 6298 §2 (α = 1/8, β = 1/4; first sample: srtt = r, rttvar = r/2), with Karn's rule: no sample from a seq that was ever resent (RFC 6298 §3) |
| 4 | cited: RFC 6298's K |
| g | resource bound: timer granularity (RFC 9002 kGranularity = 1 ms; the toy's tick, 2 ms) |
| ack_delay_max | measured or declared: the receiver's report coalescing (engine `GAP_ACK_MIN_INTERVAL` = 2 ms; the toy's tick) |
| before the first sample | cited: RFC 9002 §6.2.2, kInitialRtt = 333 ms, so srtt = 0.333 s and rttvar = 0.1665 s, giving R ≈ 1.0 s |

In the engine R is the per-path `srtt_p + 4·σ_p` that `tail-and-hold.md` E0 already reads (`PathState::srtt_measured`, `rtt_sigma_us`). This is not a second timeout law: the floor's silence timer and the two uses below are the same quantity.

### 13.1 D1: judge only what is known lost, and brake only on feedback

**The defect (§12).** §2.2's verdict timer at `t_h + T − ô` judged data whose acknowledgement could not yet have returned. With §2.8's rule that verdicted seqs leave the outstanding count, those verdicts released the pause instead of acknowledgements, so the source stopped braking: 1.66 M verdicts on delivered data in the overload cell.

**The verdict law (supersedes §2.2's verdict line and its timer).** "A seq is judged only on evidence of loss newer than its last send":

```text
   verdict(s)  at the first instant where   a(s) + ô  ≥  T   ∧   evidence(s)
   evidence(s)  ⇔   a gap report generated at  ≥ last_send(s) + ô  lists s missing        (knowledge)
                 ∨   now  ≥  last_send(s) + R                                         (timeout: overdue)
```

| symbol | provenance |
|---|---|
| a(s), T, ô | as in §2.1 and §2.2 |
| last_send(s) | measured, sender state |
| report generation time | measured; the report carries it (the toy's `gen`; in the engine, the ack's own send stamp) |
| R | as above |

- **The verdict instant is `max(t_h + T − ô, t_evidence(s))`**, a maximum of two continuous terms, so it is continuous in T. At T = ∞ it is ∞.
- **The limit of "never".** Only a gap report is knowledge. For the open tail (nothing received above s) the only evidence is the timeout. An ack slower than R gives a verdict on data that did arrive. That is gauged as `vta`, verdicts later acknowledged as received, and it must read ≈ 0 wherever R is honest (prediction Q3b-2).
- **At T below about 1.5·RTT,** §2.2's spurious verdicts on in-flight data are gone. A delivered seq's ack returns before R, so the seq never gets evidence. The floor-cost finding P10 no longer applies.

**The brake (supersedes §2.8's `− |V ∩ [A_rx, hi]|` term, answering Q6 "no").** "The outstanding count falls only on receiver feedback":

```text
   pause  ⇔  ( hi + 1 − A_rx − |released| )  ≥  B   ∨   cwnd_full          (ADR-0084 decision 3 as accepted, unchanged)
```

- A verdicted seq stays counted until the ack passes it, after its floor arrives. This is now an absolute invariant with an absolute test: O changes only in the handlers of receiver feedback (`on_report` in the toy; the ack path in the engine).
- **The exclusion bought nothing measured.** In stage 3's M cells S2 and S2p delivered the same share (0.3999 and 0.3908 for both, `results-stage3.md` P7 block). Its only measured effect was D1.
- **Its cost** is about one sweep period plus one RTT of pause after an outage longer than T, while the first floor travels and is acknowledged. §13.7 registers the pair (S3 against S3x, the exclusion arm) on the outage cells to bound that cost.
- ADR-0084 decision 3 is therefore not amended (§6's row for the source-rate bound reverts to "kept").

**Continuity.** Evidence is state, not a knob, and the brake has no T input. At T = ∞ both laws are inert, so the stage 3 identity P1 must hold again for the revised arm with the old resend clock (§13.7, Q3b-1).

### 13.2 Overload: the intake rule (answers Q11; supersedes §11 item 5's open question)

**What any first-in, first-out (FIFO) rule does.** Under sustained overload (offered rate λ above the drain rate C), the intake backlog grows until its head reaches whatever age the expiry rule drops at. Every unit sent then leaves at that age. So:

```text
   share_on_time  ≈  (C / λ) · P( a unit sent at the threshold age arrives by T )
```

- §2.1's L1 drops at `a ≥ T`, so every sent unit arrives after T: S2p read 0.167 in the 10 s overload cell. That number was missing only the owd term; head-drop as such was not the cause.
- With the threshold at `a + ô ≥ T` (the median one-way delay), half the sent units would be late.
- The threshold must therefore be an upper bound on the one-way delay, so that the units that are sent are on time.

**The intake law (refines §2.1 at the intake; L1 stays as the memory bound).** "Give a unit's slot to a younger one only when it can no longer arrive in time":

```text
   at dequeue (the instant the unit would be sent):
      expire the head u   ⇔   a(u) + R  ≥  T   ∧   a younger unit is waiting
      otherwise send u
```

| symbol | provenance |
|---|---|
| R | as above, used as an upper bound on the one-way delay: one-way ≤ RTT ≤ srtt + 4·rttvar. No symmetric-path split is needed, which matters because a forward-only queue makes srtt/2 an underestimate (seen in the drain arithmetic of §13.6) |
| "a younger unit is waiting" | state: the intake holds more than one unit |

- **Why not use ô here.** At the intake the decision is whether a unit is sent at all. A unit sent at the median threshold is late half the time, and under sustained overload every sent unit sits at the threshold.
- **The second clause removes the starvation case.** When `R ≥ T` on a slow path, the head is still sent if it is the only unit waiting, so the path keeps carrying data and keeps being measured. That is §5's "first sends still go", unchanged.
- **The clause is a condition on queue state, not on a knob.** There is no rate estimate, so a pause cannot starve the rule: units that age out during a pause expire at the next dequeue, or by L1's `a ≥ T` memory bound.
- **At T = ∞ nothing expires.** At no overload the head is sent at age ≈ 0 and nothing expires either.
- **Proportional degradation:** `share_on_time ≈ (C/λ)·(1 − ε_unrecovered)`. Units sent at age about `T − R` cannot be resent in time (L2 fails), so their first-send losses stay lost.

**What cannot be fixed: delay pinned near T − R under sustained overload.** Every sent unit waits about `T − R` in the intake. δ prices delay but not loss (owner question O1, skip-rule §1), so δ cannot justify dropping data earlier than T requires.

A young-first service order would deliver the same share at low delay. It does not reduce to FIFO at T = ∞ without a selector, so it is not adopted. Recorded as owner question Q12: should a loss price exist, so that δ could buy a shorter intake under overload? **Default: no.** The transport reports `intake_expired` and the pinned delay, and the application sheds load, for example by lowering a codec rate.

### 13.3 Which T an object transfer gets at the Realtime point (answers Q10)

- **T belongs to what is handed over, not to the hint.** Paper §5.11 already puts a contract per stream. A whole object (a file, the perf object, the native object API) has no value delivered in part. A call frame or a tunnel datagram does.
- **Named points therefore resolve T from (hint, kind of hand-over)**, once, in `config::resolve`, the same place where the hint names δ today (`hint_delta_price`):

| hand-over | Bulk | Auto | Realtime |
|---|---|---|---|
| object (perf, native object API) | ∞ | ∞ | **∞** |
| datagram / tunnel packet | ∞ | ∞ | 0.4 s |

- **This is not a mode bit.** The engine receives one number T per stream and never learns which row produced it, exactly as δ today. The deny test (§2.10) still forbids any comparison of T with a literal downstream. An application that wants a deadline on an object states T explicitly.
- **Consequence:** stage 3's M-cell share of 40 % at Realtime applies only to a stream that states T = 0.4 s. Every existing Realtime test and harness cell is an object transfer, so all of them keep T = ∞ and their current behaviour.
- **This also means no existing Realtime cell exercises a finite T.** The engine guard for this change needs one: a tunnel datagram stream at Realtime with an outage, the engine analogue of the toy's K cells.

### 13.4 The resend crossover follows measured delay (supersedes the tail-sweep eligibility used in stage 3)

**The constant.** The tail sweep resends an open-tail seq once it is older than `P = clamp(2·srtt, 25 ms, 100 ms)` (`recovery_laws.rs::tail_sweep_timeout_us`; the toy's `sweep_P`).
- At owd 50 and 60 ms, `2·srtt` = 200 and 240 ms, so the 100 ms cap binds in both cells. The early rise of the share sat at age ≈ 0.104 s in both, and the registered +30 ms shift could not appear.
- That cap is a defect finding in its own right: a bind of 100 % over those cells. It is recorded in §13.8.

**The law.** "An open-tail seq is overdue, and eligible to be resent, R after its last send":

```text
   open-tail resend eligibility:   now  ≥  last_send(s) + R          (replaces  now − first_send(s) ≥ P)
   checked by a per-seq timer at  last_send(s) + R ,  re-armed if R has grown when it fires
```

- **The sweep clock keeps only its other jobs:** the floor repeat, and in object mode re-serving reported holes, now with the same R eligibility.
- **The crossover:** the first resend of a seq lost alone arrives at age about `R + owd ≈ 3·owd + 4·rttvar + 2g`. L2 admits it iff that is below T. So the crossover is `T* ≈ R + ô`, made of measured terms only, and it moves by 3·Δowd when the path moves.
- **This changes resend timing at T = ∞ too.** Split the identity: the revised arm with the old sweep eligibility must be bit-identical to C3, and the R-timer is compared against C3 as a change of its own (C3R in §13.7). In the engine the same change touches Bulk resend timing, so it is a separate guarded change, not part of the knobs build.
- **The shift is tested where no clamp binds** (owd 15 and 20 ms; `2·srtt` = 60 and 80 ms, inside [25, 100]). The clamped pair at 50 and 60 ms is kept as the bind finding.

### 13.5 The strict-order stall after an outage (no cheap law; decomposition registered)

In stage 3 the longest strict-order stall exceeded the outage by 0.03 s (5 ms path) and 0.42 s (50 ms path), at any T. It has three parts:
- (i) outage end → arrival of the first floor: the floor's repeat clock plus one owd, ≤ P + owd;
- (ii) first floor → the last fill of holes still alive (handed over less than `T − ô` before the outage ended, so strict order must wait for their resends);
- (iii) the rest.

What each part allows:
- **No law can shorten the outage itself.**
- **(i) is already short.** With the brake of §13.1 the paused sender's first post-outage datagrams are its floor repeats and resends, so carrying the floor on data would not arrive earlier. A floor repeat on the R clock instead of P changes (i) by `R − P`, which is ≈ +5 ms at a 100 ms RTT, so it is not adopted.
- **(ii) is what strict order is.** T shortens it only for holes that were already older than `T − ô` when the outage ended; those pass at the first floor.

So no cheap law is proposed. The toy decomposes the longest episode into (i), (ii) and (iii) per run (§13.7 readouts), to confirm that (ii) carries the 50 ms excess.

### 13.6 What T should buy, and where (the gain mechanism)

- **T frees a resource only where one is scarce.** The stage 3 toy had none: links of unlimited rate and an outstanding cap that only throttled the overload cell. Giving up therefore freed nothing, and T could not win there.
- **The scarce resources are link capacity and the sender queue.** After an outage of length L, on a link of capacity C fed at λ < C, a sender that never gives up must drain the whole backlog before fresh data is on time again. A sender with lifetime T drains only what is still alive:

```text
   backlog at outage end:         never give up:  λ·L + B_lost        with T:  ≈ λ·(T − R)                (the rest expired or verdicted)
   time until fresh data is on time again at a reference deadline τ:
      never give up:   L + max(0, (λ·L + B_lost)/C − (τ − d_fwd)) · C/(C − λ)
      with T:          L + max(0,  λ·(T − R)/C      − (τ − d_fwd)) · C/(C − λ)
```

| symbol | provenance |
|---|---|
| λ, C, L | the cell (declared for the simulation) |
| B_lost | outstanding seqs lost in the outage, ≤ B |
| d_fwd | the forward delay during the drain: owd + link queue |
| τ | the readout's reference (0.4 s) |

- **The gain is the on-time units handed over during the extra drain time**, `λ·(B_lost + λ·(L − T + R))/(C − λ)` units, and it exists only when `λ·L/C` exceeds τ.
- **T buys nothing when T is above the backlog wait.** At T = 2 s in a cell whose drain wait is 1.3 s, T changes nothing, and that is predicted too (Q3b-4).
- **The sender queue is the second mechanism, under overload (§13.2).** Without T the backlog grows without bound and nothing is on time after about `τ·C/(λ − C)` seconds. With T the on-time share settles at C/λ.
- **The third would be the receiver's strict-order stall, and §13.5 shows T does not move it.**

### 13.7 Registration: toy stage 3b

Build on branch `toy-stage3-knobs` `39efa6a3` in `tools/toy/`: extend `toy2.py`, and add `batch3b.py` and `summarize3b.py`, patterned on `batch3.py` and `summarize3.py`. Stage 1, 2 and 3 code paths and results must stay bit-identical. Gate every new behaviour behind new `Cfg` fields whose defaults reproduce stage 3, and keep the existing test suites green. Re-run the 34-configuration identity check of stage 3's worker decision 17 against stage 3's outputs, and commit the check script this time (`check_ident3b.py`).

#### 13.7.1 New code (exact)

**Config fields** (defaults reproduce stage 3):
- `rtt_est: str = "old"`: `"rfc"` turns on Karn's rule and RFC 6298 srtt and rttvar, and also defines `R_now()`;
- `src_model: str = "old"`: `"wc"` is the work-conserving source;
- `link: object = None`: `dict(C=…, Q=…)` turns on the bottleneck;
- `tail_elig: str = "P"`: `"R"` turns on the per-seq R timer;
- `deq_expiry: bool = False`;
- `evidence_verdict: bool = False`;
- `o_excl_verdict` (exists; S3 sets False, arm S3x sets True).

**Sender `"S3"`** is S2 (stage 3 code) with `evidence_verdict = True`, `deq_expiry = True`, `o_excl_verdict = False` and `rtt_est = "rfc"`. `tail_elig` is set per arm. S3 has no verdict copy.

1. **RTT (`rtt_est = "rfc"`).** In `on_report`, take a sample `r = now − sent_t[A − 1]` only when `copies[A − 1] == 0` (Karn's rule). On the first sample set `srtt = r, rttvar = r/2`; after that `rttvar = 0.75·rttvar + 0.25·|srtt − r|`, then `srtt = 0.875·srtt + 0.125·r` (RFC 6298 §2.3 order). Define `R_now() = srtt + max(4·rttvar, tick) + tick`, with `srtt = 0.333, rttvar = 0.1665` before the first sample. S3's ô in L2 is `srtt/2` (with the same seed), not the exact `rtt/2`. Arms with `rtt_est = "old"` are unchanged.
2. **Evidence-gated verdicts (`evidence_verdict`).**
   - In `act`, an S3 candidate that fails L2 takes a verdict only if `can_resend(s, gen)` holds, the existing test "this report was generated after the last copy could have arrived", or if `now ≥ last_send[s] + R_now()`. Otherwise it is left alone: no serve and no verdict.
   - The per-seq verdict event (kind 8) is pushed at `max(t, h_seq[s] + T − srtt_seed/2)`, where `srtt_seed` is the RFC 9002 seed if no sample exists yet, else the current srtt. On firing, if s is unresolved (the stage 3 condition) and not verdicted:
     - if `now ≥ last_send[s] + R_now()`, take the verdict (no floor of its own, as in stage 3);
     - else re-push it at `last_send[s] + R_now()`.
   - This is the timeout half of evidence. Reported holes are judged by `act` on the report path.
3. **Brake.** With `o_excl_verdict = False`, `O_ctl = O` (stage 3's selector). `maybe_resume` is called only from `on_report`. Remove the S3 calls after verdicts (stage 3's worker decision 6 added them for S2), leaving S2 and S2c as they were.
4. **Dequeue expiry (`deq_expiry`, in `emit`).** Before assigning a seq, loop:

   ```python
   while (self.u_next + 1 < self.units_total
          and (self.u_next + 1) * self.dt <= t
          and dequeue_expired(c.T, t - self.u_next * self.dt, self.R_now())):
       self.intake_expired_deq += 1
       self.u_next += 1
   ```

   The first two conditions say that a younger unit is waiting. `dequeue_expired(T, age, m)` returns `age + m >= T`. It is a module-level law function, added to the deny test's `EXEMPT` with a pointer to §13.2. Stage 3's L1 loop stays, and runs first.
5. **Work-conserving source (`src_model = "wc"`).** At the end of `emit`, if not done and not paused, set `src_next = max(t, self.u_next·dt)` instead of `t + dt`. A waiting backlog is then sent at once, limited by the pause and the link queue; with no backlog the next unit is sent at its own hand-over time. In `maybe_resume`, set `src_next = max(now, self.u_next·dt)`. The `"old"` model is unchanged.
6. **Bottleneck (`link`).** Add state `link_free = 0.0` and `link_busy = 0.0`. In every forward send (`emit`, `send_copy`, `send_floor`):
   - first compute the queue in seconds, `qs = max(0, link_free − t)`;
   - if `qs ≥ Q/C`, the datagram is dropped at the queue (`queue_drop += 1`; no loss draw and no event);
   - else set `d = max(t, link_free) + 1/C`, `link_busy += 1/C`, `link_free = d`, take the arrival as `d + delay.at(d)`, and make the loss draw with `lost(…, t = d)`. The dead window is therefore read at departure, so datagrams queued when the outage starts are lost.
   - Reports and the reverse path are not limited. The FIFO check (`fifo_note`) applies to arrivals as today.
   - With `link = None` none of this runs.
7. **R timer for the open tail (`tail_elig = "R"`).** At every send of s (first send and copies), push an event of kind 9 at `t + R_now()` with payload `(s, t)`. On firing, if `last_send[s] == t` (no later send), `s ≥ A_known`, `s > rhi_known` (no report covers s) and s is not verdicted:
   - if `now ≥ t + R_now()`, call `act(now, now, [s], open_tail=True)`, which serves or takes the verdict by L2 and evidence;
   - else re-push at `t + R_now()`.

   In `on_sweep`, with `tail_elig = "R"`, the open-tail list is not built. The object-mode re-serve uses `now − last_send[x] ≥ R_now()` in place of `≥ P`. `floor_repeat` is unchanged.
8. **Deny test.** Keep `DIALS ∋ T, Hk`; add `dequeue_expired` to `EXEMPT`.

**New readouts** (every arm; no mean is taken over delivered units alone):
- `share_by_0p4` and `share_by_2` (stage 3 definitions; denominator: units handed over);
- delay from hand-over `D_u = dlv − h(u)` over **all units handed over**, with never-delivered and expired units set to `+inf`: p50, p90 and p99 (a percentile is reported as `inf` when it falls on an undelivered unit);
- `vta` (verdicts on seqs the receiver had received before the verdict instant: `got[s]` with `arr[s] ≤ verdict time`);
- `intake_expired`, `intake_expired_deq`, `queue_drop`, `link_util` (`link_busy` over the run's active time, from the first send to the last departure), `pause_events`, `pause_time` (total time paused);
- `throttle`: `"link"` if `link_util ≥ 0.98` over the overload window, else `"pause"` if `pause_time` exceeds half the window, else `"none"`;
- `R_mean`;
- `resend_by_R` and `resend_by_report` (resend counts by trigger);
- for the longest strict-order stall episode, the parts (i), (ii) and (iii) of §13.5, using the outage end from `dead`, the first floor arrival after it, and the last fill inside the episode.

**Tests to add first, red first:**
1. With every new field at its default, stage 3's outputs are reproduced (the identity script).
2. S3 with `tail_elig = "P"` at T = inf equals C3 on stage 3's 20 keys and on the trace, in the E, M and K cells (3 seeds).
3. S3 at T = 1e6 equals S3 at T = inf, for both `tail_elig` values.
4. The brake: a test seam records every change of `O_ctl()` and asserts that each happens inside `on_report` (with `o_excl_verdict = False`).
5. `vta == 0` in K-I2 at T = 0.14 for S3, against stage 3's spurious verdicts there.
6. The dequeue rule: in OV (below), no sent unit has `a + R_now() ≥ T` at its dequeue unless it was the only unit waiting (checked from a trace of dequeue decisions).
7. The link with `Q = 0` drops everything, and with large C and Q reproduces arrival = `t + 1/C + delay.at(·)`.
8. The deny test, with a positive control `dequeue_expiredish(c)`.

#### 13.7.2 Cells

All cells have one path, 12 seeds, phases drawn per seed, and δ = 50.

| cell | definition | why |
|---|---|---|
| E-I2, E-L10, M-I2, M-L10, K-I2, K-L10 | stage 3's cells, unchanged | identity, continuity and the positive control |
| **CL** (capacity, the T benefit) | cell I2 loss; owd 5 ms, J 1 ms; `link = dict(C=12500, Q=1250)`; λ = 10 000/s (`rate`); `n_total = 100000` (10 s); `src_model = "wc"`; `pause_B = 1000` (below C·RTT + Q = 125 + 1250, so the pause, not queue overflow, brakes; witness `queue_drop = 0`); `dead = (0, 2.0, 1.5)` read at departure; `t_limit = 40` | §13.6's drain |
| **OV** (overload, valid for every arm) | cell I2 loss; owd 5 ms; `link = dict(C=10000, Q=1250)`; λ = 12 000/s; `n_total = 120000` (10 s); `src_model = "wc"`; `pause_B = 1000`; no outage; `t_limit = 40` | §13.2; the link throttles every arm. A run whose `throttle` is not "link" over [1 s, 10 s] makes the cell invalid for that arm, reported as such |
| **X15, X20** (crossover, unclamped) | cell L10 loss; rate 50; `n_total = 3000`; no outage; owd 15 ms and 20 ms | §13.4 |
| K-L10, K-L10b | stage 3's call cells at owd 50 and 60 ms (the clamp binds) | the bind finding, and the shift under the R timer |

Cost check: CL and OV are 100 000 and 120 000 units, so C1, at about 1.2 ms per unit (stage 3 decision 14), takes about 2 to 2.5 min per run. Every arm finishes.

#### 13.7.3 Arms

- **Comparisons (every cell):**
  - C1 = S1c + R0e at ρ = 0.99;
  - C3 = S1c + R1 at ρ = 1;
  - C3R = S3 with `tail_elig = "R"` at T = inf, H = inf (the sweep change alone).
  - CL and OV also run `src_model = "wc"` and `rtt_est = "rfc"` for C1 and C3, so every arm shares the source and the RTT estimator.
- **Revised arm:** S3 with `tail_elig = "R"`, T ∈ {0.4, 2.0, inf} × H ∈ {0, inf}.
- **Identity arms:** S3 with `tail_elig = "P"` at T = inf, H = inf; and S3 (R) at T = 1e6, H = inf.
- **Brake pair:** S3x (`o_excl_verdict = True`) at T = 0.4, H = inf, in M-I2, M-L10, CL and OV.
- **Stage 3 arm, for D1:** S2 at T = 0.4, H = 0 in OV and CL, to show D1 on a real link.
- **Crossover sweeps** (H = 0, S3 R):
  - X15: T = 0.040 to 0.070 in 2 ms steps (16 points);
  - X20: T = 0.055 to 0.085 in 2 ms steps (16 points);
  - K-L10: T = 0.150 to 0.215 in 5 ms steps (14 points);
  - K-L10b: T = 0.180 to 0.245 in 5 ms steps (14 points);
  - and S2 (stage 3 arm, old sweep) on the same K-L10 and K-L10b grids.
- **Nudges** (S3 R, every stage 3 cell plus CL): T = 0.4·{0.98, 0.99, 1, 1.01, 1.02} at H = 0.02, and H = 0.02·{…} at T = 0.4.
- **Positive control (E-I2, E-L10), placed by a fixed procedure** (run after the identity arms, before any decision cell):
  - from the 12 runs of S3 (R) at T = inf, H = inf, take the delivery ages `dlv − h` of the dead-tail units (seq ≥ `dead[0]`);
  - let p be their median, rounded to 1 ms, and w their interquartile range;
  - run S3 (R) at T = p·{0.98, 0.99, 1, 1.01, 1.02} with H = inf;
  - stage 3 measured these ages at about 1.53 s (E-I2) and 1.80 s (E-L10, range 1.69 to 1.89) under C3. The procedure re-measures them under S3 (R).
- **A/A** (frozen as `nf_stage3b.json` before any decision cell): S3 (R) at T = inf, H = inf, phase variants 0 and 1, in E-I2, E-L10, CL, OV and X15, on every readout above.

#### 13.7.4 Readings

- Reading rules are as stage 3 (`summarize2.read`, paired per seed, `m = max(T_bar, NF)`, NF from `nf_stage3b.json`).
- Share bars are `T_bar = max(1/units, 0.05·(1 − share_any_C3))` against the cell's own C3. This is the registered bar of §9.5; no worker substitution is allowed. Delay quantile bars are 1 ms.
- K and P are as `summarize2.kp_stats`.
- **The crossover's primary reading is the mid-level crossing:** the smallest T on the grid, linearly interpolated, at which `share_T` reaches the midpoint between its value at the grid's lowest T and its value at T = 0.4 (run once per cell at T = 0.4 for that purpose). The 90 % reading is reported as secondary.

#### 13.7.5 Predictions (written before any run)

- **Q3b-1, identities.**
  - S3 (P) at T = inf equals C3, and S3 at T = 1e6 equals T = inf, on stage 3's 20 keys and on the trace, in every cell, seed and `tail_elig` value.
  - With every new field at its default, the identity script reproduces stage 3's outputs.
  - FAIL: any difference.
- **Q3b-2, D1 fixed.**
  - `vta ≤ 0.001 × verdicts` for S3 in every cell where verdicts > 100.
  - In OV, S3's `pause_time` is within 5 % of C3's and its `throttle` is "link".
  - S2 in OV shows D1 again: verdicts above 10 × S3's, and `vta` above 0.5 × its own verdicts.
- **Q3b-3, overload degrades in proportion (OV):**
  - S3 T = 0.4 (H = 0): `share_by_0p4` ∈ [0.77, 0.83], against `(C/λ)·(1 − p0) = 0.808`;
  - delay-from-hand-over p50 ∈ [0.25, 0.40] s (pinned near T − R; the unfixed cost of §13.2);
  - C3 and C1: `share_by_0p4` ∈ [0.10, 0.22] (the intake wait grows by 0.2 s per s, and passes 0.4 − d_fwd ≈ 0.3 s at about 1.5 s); `share_any` ≥ 0.99 for C3;
  - S3 T = 2: `share_by_2` ∈ [0.77, 0.83];
  - `throttle = "link"` for every arm. Otherwise the cell is invalid for that arm.
- **Q3b-4, the drain gain (CL), from §13.6** (with d_fwd ≈ owd + B/C − RTT ≈ 0.08 s):
  - C3: `share_by_0p4` ∈ [0.30, 0.47] (late window ≈ 1.5 + (1.28 − 0.32)·5 ≈ 6.3 s) and `share_by_2` ≥ 0.97;
  - S3 T = 0.4: `share_by_0p4` ∈ [0.75, 0.90];
  - S3 T = 0.4 minus C3 on `share_by_0p4` reads "shown larger" by more than 0.25;
  - S3 T = 2 against C3 on `share_by_0p4` and `share_by_2` reads equivalent (T above the backlog wait buys nothing);
  - `queue_drop = 0` for every arm.
- **Q3b-5, the crossover follows the path (S3 R, H = 0).**
  - Mid-level crossings: X15 at 0.047 to 0.055 s, X20 at 0.062 to 0.070 s, shift 15 ms ± 4 ms;
  - K-L10 at 0.150 to 0.165 s, K-L10b at 0.180 to 0.195 s, shift 30 ms ± 5 ms;
  - S2 (old sweep) at K-L10/K-L10b shows a shift below 10 ms (the clamp bind, reproducing stage 3);
  - below each crossing, `resend_by_R = 0` and `resend_by_report = 0` (no resend can arrive in time).
- **Q3b-6, continuity.** K and P read equivalent at the T and H nudges in all stage 3 cells and CL, on `share_by_0p4`, `share_by_2`, delay p50 and p90, `ooo_per_delivered` and `repair_per_delivered`.
- **Q3b-7, positive control.**
  - In each E cell, if `w < 0.01·p`, K on `share_T` reads "shown". Stage 3's E-I2 ages suggest this holds there.
  - If `w ≥ 0.01·p`, the step is a ramp across the nudge window. Then the prediction is that J(0.02) (the five-point outer difference, §7.9) reads "shown larger" and K reads equivalent. Stage 3's E-L10 range suggests this case.
  - Which case applies is fixed by the measured w before the nudges run, and written into the results first.
- **Q3b-8, brake cost (S3x against S3, M cells and CL):** `share_by_0p4` equivalent; the post-outage resume delay (outage end → first send after the pause) for S3 at most R + one sweep period + RTT, against S3x's earlier resume. In OV, S3x's `throttle` is not weaker than S3's because S3's evidence rule leaves almost nothing for the exclusion to remove: `pause_time` within 5 %.
- **Q3b-9, strict stall decomposition (K cells, S3 R, H = inf, T = 0.4):** part (i) ≤ P + owd + 5 ms in every seed; at K-L10, part (ii) ≥ 0.5 × (longest stall − 1.5 s), i.e. most of the excess is live-hole fills, not floor wait.

Run order:
1. tests;
2. the identity arms (stop if red);
3. A/A and its freeze;
4. the positive-control placement runs;
5. the X cells and K sweeps;
6. CL and OV;
7. the rest.

Report every reading, the A/A self-check and every deviation, as in stage 3.

### 13.8 Effect on the earlier sections

- **§2.2:** the verdict needs evidence (§13.1); the open-tail resend is eligible after R (§13.4). The physical step is now at `R + ô`.
- **§2.1:** the intake gains the dequeue rule (§13.2). L1's `a ≥ T` stays as the memory bound and the absolute test.
- **§2.8 and §6:** the outstanding count is ADR-0084 decision 3 as accepted, with no exclusion (Q6 answered "no"). L7's memory statement is unchanged.
- **§4:** T is resolved from (hint, kind of hand-over) (§13.3). The Realtime row reads "0.4 s for datagrams and tunnel packets, ∞ for objects".
- **§5:** the overload row now reads "share ≈ C/λ; delay pinned near T − R; `intake_expired` reported".
- **§7:**
  - Q6 answered no;
  - Q10 and Q11 answered as above, defaults awaiting the owner;
  - Q12 new: a loss price, default no;
  - Q13 new: replace the tail-sweep eligibility `clamp(2·srtt, 25, 100 ms)` with R in the engine too. **Default: yes, as its own guarded change.** It moves Bulk resend timing, and the cap's 100 % bind at 50 and 60 ms owd is a defect finding (MEASUREMENT DISCIPLINE 17 and 18), owed a ledger verdict.
- **§10, the engine sketch:**
  - step 5 reads: evidence-gated verdicts; no O exclusion; dequeue expiry with R; the per-path R timer for the open tail (its own commit, Q13);
  - the gauges add `vta`, `intake_expired_deq` and the R bind (time at the granularity floor);
  - the guard adds a Realtime datagram-stream cell with an outage, and a capacity-limited cell (§13.3, §13.6).
- **Not fixed, and why:**
  - delay pinned near T − R under sustained overload (loss is unpriced, Q12);
  - the strict-order stall beyond the outage (it is live holes, §13.5);
  - the open tail's timeout verdicts when an ack is slower than R (bounded by `vta`);
  - the toy still has no proactive repair, so T below a round trip is tested only in its ARQ half.
