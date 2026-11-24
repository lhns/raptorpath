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

The choice of the full ack timeout R as the dequeue margin (instead of R/2) is mine, made after that review.

A final review of the whole section then changed five things:
- the RTT sample is taken from the highest newly received seq, not the frontier;
- evidence is stated in sender terms (a datagram sent later has arrived), and the same gate applies to resends of every arm in link cells;
- the R timer sends one probe per timeout and does not back off;
- a narrow-tail cell is added for the positive control;
- queue drops are counted inside and outside the outage window.

One quantity is used throughout. It is the time after which an unacknowledged send is overdue, the ack timeout of RFC 6298 §2 and RFC 9002 §6.2.1:

```text
   R  =  srtt  +  max( 4 · rttvar ,  g )  +  ack_delay_max
```

| symbol | provenance |
|---|---|
| srtt, rttvar | measured, by RFC 6298 §2 (α = 1/8, β = 1/4; first sample: srtt = r, rttvar = r/2). Samples come from the largest newly received seq only (RFC 9002 §5.1), and never from a seq that was resent (Karn's rule, RFC 6298 §3). A sample from the cumulative frontier, as stage 3 took it, is a pre-outage send after a fill, inflated by the fill round |
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
   evidence(s)  ⇔   s unresolved  ∧  last_send(s)  <  τ_rx                        (knowledge: a datagram sent later has arrived)
                 ∨   now  ≥  last_send(s) + R                                    (timeout: overdue)
```

| symbol | provenance |
|---|---|
| a(s), T, ô | as in §2.1 and §2.2 |
| last_send(s) | measured, sender state |
| τ_rx | measured: the newest send stamp among datagrams the receiver has received, echoed in its report. This is RFC 8985's (RACK) time-based loss test, with the reordering window 0 that a FIFO path allows (§6.2 there; a reordering path needs RACK's window, as in the engine's C3 hole law). It is stated entirely in sender time, with no clock comparison across hosts. **Engine source: open.** Either the per-path batch acknowledgements already carry it, or it is one ack field (a wire version). Recorded as Q14 |
| R | as above |

- **The verdict instant is `max(t_h + T − ô, t_evidence(s))`**, a maximum of two continuous terms, so it is continuous in T. At T = ∞ it is ∞.
- **The limit of "never".** Only a later arrival is knowledge. For the open tail with nothing sent after it, the only evidence is the timeout. An ack slower than R gives a verdict on data that did arrive. That is gauged as `vta`, verdicted seqs the receiver ever received, and it must read ≈ 0 wherever R is honest (prediction Q3b-2).
- **The same predicate gates resends.** A copy is resent only on evidence that the previous one is lost. That replaces the toy's `gen ≥ last_send + owd + J`, which compares a receiver-side generation time with a sender time plus the base owd, and so ignores queueing: on a link with a 70–90 ms queue it resends every report interval.
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

**The law.** "When the oldest open-tail seq is R overdue, probe with it":

```text
   probe:   at  last_send(s_o) + R ,   s_o = the oldest unresolved, unverdicted seq above every reported one,
            send one datagram for s_o (serve or verdict by L2 and evidence);   re-arm for the new s_o      (replaces  now − first_send(s) ≥ P)
```

- One probe per timeout follows RFC 9002 §6.2.4, which sends one or two probe datagrams per PTO. The probe's arrival raises τ_rx, which gives evidence for every older open-tail seq at once (§13.1). So recovery after an outage is not serial, and the load is bounded at 1/R datagrams per second without a backoff.
- RFC 6298 §5.5's and RFC 9002 §6.2.1's doubling is not used. It would bound the load a second time, and after a long outage it would delay recovery by up to the last doubled interval. The cost is that a dead path keeps one probe per R in flight. Its rate is gauged as `probes`.

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
- `tail_elig: str = "P"`: `"R"` turns on the probe timer;
- `resend_gate: str = "owd"`: `"evidence"` gates every resend on evidence (link cells only);
- `deq_expiry: bool = False`;
- `evidence_verdict: bool = False`;
- `o_excl_verdict` (exists; S3 sets False, arm S3x sets True).

**Sender `"S3"`** is S2 (stage 3 code) with `evidence_verdict = True`, `deq_expiry = True`, `o_excl_verdict = False` and `rtt_est = "rfc"`. `tail_elig` is set per arm. S3 has no verdict copy.

1. **RTT (`rtt_est = "rfc"`).** In `on_report`, take a sample `r = now − sent_t[rhi]` only when the report's `rhi` exceeds the previous `rhi_known` and `copies[rhi] == 0` (largest newly received, RFC 9002 §5.1; Karn's rule). The report already carries `rhi`. Add `τ_rx` to the report tuple: the receiver's maximum `tau` over data arrivals, which `on_data` already receives. The sender keeps the maximum it has heard. On the first sample set `srtt = r, rttvar = r/2`; after that `rttvar = 0.75·rttvar + 0.25·|srtt − r|`, then `srtt = 0.875·srtt + 0.125·r` (RFC 6298 §2.3 order). Define `R_now() = srtt + max(4·rttvar, tick) + tick`, with `srtt = 0.333, rttvar = 0.1665` before the first sample. S3's ô in L2 is `srtt/2` (with the same seed), not the exact `rtt/2`. Arms with `rtt_est = "old"` are unchanged.
2. **Evidence-gated verdicts (`evidence_verdict`).**
   - Define `evidence(s, now) = (last_send[s] < self.tau_rx) or (now >= last_send[s] + R_now())`.
   - In `act`, an S3 candidate that fails L2 takes a verdict only if `evidence(s, now)` holds. One that passes L2 is served only if `copies[s] == 0` or `evidence(s, now)` holds. Otherwise it is left alone.
   - The per-seq verdict event (kind 8) is pushed at `max(t, h_seq[s] + T − srtt_seed/2)`, where `srtt_seed` is the RFC 9002 seed if no sample exists yet, else the current srtt. On firing, if s is unresolved (the stage 3 condition) and not verdicted:
     - if L2 now passes (`a + srtt/2 < T` with the current srtt), re-push at `h_seq[s] + T − srtt/2`;
     - else if `evidence(s, now)` holds, take the verdict (no floor of its own, as in stage 3);
     - else re-push at `last_send[s] + R_now()`.
   - **Resend gate selector `resend_gate: str = "owd"`.** With `"evidence"`, every arm's `can_resend(s, gen)` is replaced by `copies[s] == 0 or evidence(s, now)`. It is set for **every arm** in the link cells (CL, OV) and left at `"owd"` elsewhere, so stage 3 stays identical. The link cells therefore also need `rtt_est = "rfc"` for every arm.
3. **Brake.** With `o_excl_verdict = False`, `O_ctl = O` (stage 3's selector). For S3, `maybe_resume` is called only from `on_report`. S2, S2c and S3x (`o_excl_verdict = True`) keep stage 3's calls after verdicts, so S3x resumes on a verdict and measures the exclusion as stage 3 built it.
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
7. **The probe timer (`tail_elig = "R"`).** Keep exactly one pending event of kind 9, first pushed at the first send at `t + R_now()`. On firing:
   - find `s_o`, the smallest s with `s > rhi_known`, `s ≥ A_known`, `s < n_sent` and s not verdicted;
   - if none exists, re-push at `now + R_now()`;
   - else if `now ≥ last_send[s_o] + R_now()`, call `act(now, now, [s_o], open_tail=True)` (L2 and evidence decide serve or verdict; evidence holds by the timeout), count `probes += 1`, and re-push at `now + R_now()`;
   - else re-push at `last_send[s_o] + R_now()`.

   In `on_sweep`, with `tail_elig = "R"`, the open-tail list is not built. The object-mode re-serve uses `now − last_send[x] ≥ R_now()` in place of `≥ P`. `floor_repeat` is unchanged.
8. **Deny test.** Keep `DIALS ∋ T, Hk`; add `dequeue_expired` to `EXEMPT`.

**New readouts** (every arm; no mean is taken over delivered units alone):
- `share_by_0p4` and `share_by_2` (stage 3 definitions; denominator: units handed over);
- delay from hand-over `D_u = dlv − h(u)` over **all units handed over**, with never-delivered and expired units set to `+inf`: p50, p90 and p99 (a percentile is reported as `inf` when it falls on an undelivered unit);
- `vta` (verdicted seqs the receiver ever received: `|{s ∈ verdicted : got[s]}|`; S3 sends no copy at a verdict, so any such arrival came from a send made before it);
- `intake_expired`, `intake_expired_deq`, `queue_drop_in` and `queue_drop_out` (drops with departure attempt inside or outside the dead window), `probes`, `link_util_w` (link busy time inside [1 s, 10 s], divided by 9 s), `pause_events`, `pause_time` (total time paused);
- `throttle` (OV only): `"link"` if `link_util_w ≥ 0.98`, else `"pause"` if the pause time inside [1 s, 10 s] exceeds 4.5 s, else `"none"`;
- `R_mean`;
- `resend_by_R` and `resend_by_report` (resend counts by trigger);
- for the longest strict-order stall episode, the parts (i), (ii) and (iii) of §13.5, using the outage end from `dead`, the first floor arrival after it, and the last fill inside the episode.

**Tests to add first, red first:**
1. With every new field at its default, stage 3's outputs are reproduced (the identity script).
2. S3 with `tail_elig = "P"` at T = inf equals C3 on stage 3's 20 keys and on the trace, in the E, M and K cells (3 seeds).
3. S3 at T = 1e6 equals S3 at T = inf, for both `tail_elig` values.
4. The brake: a test seam records every **decrease** of `O_ctl()` and asserts that each happens inside `on_report` (with `o_excl_verdict = False`).
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
| **E-I2n** (narrow tail) | E-I2 with `tail = 50`: `dead = (n − 50, (n − 50)/10000, 1.5)`, so the dead tail spans 5 ms of hand-over, 0.3 % of its fill age | the positive control on K. Stage 3's 347-unit tail spans 34.7 ms, 2.3 % of 1.5 s, which is a ramp across the ±2 % window in both E cells |

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
- **Positive control (E-I2n, E-I2, E-L10), placed by a fixed procedure** (run after the identity arms, before any decision cell):
  - from the 12 runs of S3 (R) at T = inf, H = inf, take the delivery ages `dlv − h` of the dead-tail units (seq ≥ `dead[0]`);
  - let p be their median, rounded to 1 ms, and w their interquartile range;
  - run S3 (R) at T = p·{0.98, 0.99, 1, 1.01, 1.02} with H = inf;
  - stage 3 measured these ages at about 1.53 s (E-I2) and 1.80 s (E-L10, range 1.69 to 1.89) under C3. The procedure re-measures them under S3 (R).
- **A/A** (frozen as `nf_stage3b.json` before any decision cell): S3 (R) at T = inf, H = inf, phase variants 0 and 1, in every cell that carries a reading: E-I2, E-I2n, E-L10, M-I2, M-L10, K-I2, K-L10, K-L10b, CL, OV, X15 and X20, on every readout above. NF_K = NF_P = the readout's own floor, as stage 3 had it, since there is no five-point A/A.

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
  - `queue_drop_out = 0` for every arm. `queue_drop_in` is reported: during the outage the old-sweep arms resend about 1 000 outstanding seqs per 25 ms, which is above C, while S3 sends one probe per R.
- **Q3b-5, the crossover follows the path (S3 R, H = 0).**
  - Mid-level crossings: X15 at 0.047 to 0.055 s, X20 at 0.062 to 0.070 s, shift 15 ms ± 4 ms;
  - K-L10 at 0.150 to 0.165 s, K-L10b at 0.180 to 0.195 s, shift 30 ms ± 5 ms;
  - S2 (old sweep) at K-L10/K-L10b shows a shift below 10 ms (the clamp bind, reproducing stage 3);
  - each crossing also lies within ± 4 ms of the run's own `R_mean + srtt_mean/2`. That is the measured-delay form; the absolute bands above assume a small rttvar, and this one does not;
  - at each grid's lowest T, `resend_by_R = 0` and `resend_by_report = 0` in every seed (no resend can arrive in time).
- **Q3b-6, continuity.** K and P read equivalent at the T and H nudges in all stage 3 cells and CL, on `share_by_0p4`, `share_by_2`, delay p50 and p90, `ooo_per_delivered` and `repair_per_delivered`.
- **Q3b-7, positive control.**
  - In each E cell, if `w < 0.01·p`, K on `share_T` reads "shown". The prediction is that this holds in E-I2n.
  - If `w ≥ 0.01·p`, the step is a ramp across the nudge window. Then the prediction is that J(0.02) (the five-point outer difference, §7.9) reads "shown larger" and K reads equivalent. The prediction is that this holds in E-I2 and E-L10, whose tails span 2.3 % of the fill age.
  - Which case applies in each cell is fixed by the measured w before the nudges run, and written into the results first. If E-I2n's w comes out ≥ 0.01·p, the K control has no cell, which is a finding.
- **Q3b-8, brake cost (S3x against S3, M cells and CL):** `share_by_0p4` equivalent; the post-outage resume delay (outage end → first send after the pause) for S3 at most R + one sweep period + 2·RTT (probe, floor, ack), against S3x's earlier resume. In OV, S3x's `throttle` is not weaker than S3's because S3's evidence rule leaves almost nothing for the exclusion to remove: `pause_time` within 5 %.
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
  - Q14 new: the engine's source for τ_rx, RACK's newest-delivered send time (§13.1): per-path batch acks if they carry it, else one ack field. **Default: find out before the build.** The evidence rule needs it;
  - Q13 new: replace the tail-sweep eligibility `clamp(2·srtt, 25, 100 ms)` with R in the engine too. **Default: yes, as its own guarded change.** It moves Bulk resend timing, and the cap's 100 % bind at 50 and 60 ms owd is a defect finding (MEASUREMENT DISCIPLINE 17 and 18), owed a ledger verdict.
- **§10, the engine sketch:**
  - step 5 reads: evidence-gated verdicts; no O exclusion; dequeue expiry with R; the R probe timer for the open tail (its own commit, Q13); the τ_rx evidence (Q14);
  - the gauges add `vta`, `intake_expired_deq` and the R bind (time at the granularity floor);
  - the guard adds a Realtime datagram-stream cell with an outage, and a capacity-limited cell (§13.3, §13.6).
- **Not fixed, and why:**
  - delay pinned near T − R under sustained overload (loss is unpriced, Q12);
  - the strict-order stall beyond the outage (it is live holes, §13.5);
  - the open tail's timeout verdicts when an ack is slower than R (bounded by `vta`);
  - the toy still has no proactive repair, so T below a round trip is tested only in its ARQ half.

## 14. Findings from the offline simulation, stage 3b (2026-11-24, reviewed with the advisor)

Branch `toy-stage3-knobs` `d0ce4f9d`, `tools/toy/results-stage3b.md`.

**Confirmed.**
- Identities (Q3b-1): S3 with the old sweep at T = ∞ and 10⁶ equals full delivery; 432/432, traces identical.
- **D1 is fixed** (Q3b-2): 0 verdicts on delivered data in 1 326 S3 runs; S2 reproduces D1 (113 200 verdicts, 81 % on delivered data). The pause-time bullet failed; its readout (whole-run pause total) measures run length and is not a valid test.
- The new design meets its own predicted numbers: overload share within 0.4 s 0.815 (band 0.77–0.83) at H = 0; capacity cell 0.848 within 0.4 s at T = 0.4.
- The crossover under the R probe follows the path (Q3b-5): +15.1 ms and +30.2 ms, each within 3 ms of R + srtt/2.

**Not valid: every never-give-up comparison in the link cells.** Full delivery in CL read 19.9 % within 0.4 s and 20.9 % within 2 s (predicted 30–47 % and ≥ 97 %), with 684 k queue drops for 100 k units. Cause found in `toy2.py`: only fresh data is paused (`emit`, `o >= self.B`); `send_copy` passes no brake, so resends storm a bottleneck queue. The engine gates every send by `cwnd_full`. The old arms' collapse is therefore a simulation artefact, and the size of the lifetime's gain in CL and OV is not established. The new arm's absolute numbers stand.

**Overload at strict order.** The 0.815 holds only at H = 0; at H = ∞ it is 0.087, the same as never giving up (0.086). Cause not yet read from the results.

**Delay under overload** sits at p50 390 ms against T = 0.4 s: on time by about 10 ms, by construction (Q12).

**New defect D2 (Revision 1 breaks T = ∞).** The "last send + R" probe recovers an outage tail one seq per R. At T = ∞ with a 50 ms path it never finishes in 40 s (12/12 seeds); at 5 ms the tail takes a median 4.3 s against about 1.5 s with the old sweep. T = ∞ is Bulk and Auto, so "T = ∞ equals today" holds only with the old timer. The probe's motivation is also gone: the old sweep shifted +29.9 ms too (the "below 10 ms" bullet failed), so stage 3's reading that the sweep cap pinned the crossover was wrong. The positive-control placements (Q3b-7) are products of D2.

**Other readings.** Smoothness (Q3b-6): 0 shown, 4/148 undetermined; the control read "shown" on J only, so continuity is still not demonstrated on K and P. The stall decomposition's definition changed after the first read (post hoc).

**Register before the next run:** a resend brake on every arm (the engine's cwnd gate); a tail-recovery rule at T = ∞ that is not serial (or keep the old sweep and drop the probe); a valid pause readout; the missing T = ∞, H = 0 and T = 2 rows in both link cells; the cause of the strict-order overload collapse.

## 15. Revision 2 (2026-11-24, after stage 3b; owner: fix items 1, 2 and 4, register a rerun, "with fable")

This section supersedes earlier sections where it says so. Sections 1 to 14 are kept as the record.

The advisor (Fable) reviewed these, before writing:
- the diagnosis of the strict-order collapse and its evidence (§15.2);
- the floor law that fixes it, including its derivation from L2 at the earliest resend after the last send, its liveness at the lost tail, and its limit at T = 2 s (§15.3);
- the engine reads behind the resend brake, which overturned the brief's premise (§15.4);
- withdrawing the R probe, its own earlier review included, and pairing the identity check with a liveness check (§15.1);
- the per-seed placement of the continuity control, the pause readout and the extra stall part (§15.5);
- the prediction bands drawn from stage 3b's lightly-resending rows (§15.6).

It also reviewed the whole section before the push. That review changed six things:
- the floor is recomputed and sent at every decision instant, with no test on the verdicted-pending set, because §15.1's verdict is unreachable under §15.3;
- the floor-datagram carrier is coalesced to one per 2 ms;
- the continuity control is placed off the ramp;
- the `F_lag` prediction is dropped;
- the stall window is defined at the receiver;
- the brake model is checked against the engine's budget decrements, and the stage-3 and 3b references are regenerated because they are not committed.

### 15.1 D2: withdraw the probe; keep the old sweep; evidence from feedback only (supersedes §13.4 and Q13)

**What failed.** §13.4's probe resent only the oldest open-tail seq per R. Its arrival gave the *evidence* for every older seq, but no rule *acted* on the seqs above it, so an outage tail was recovered one seq per R. At T = ∞ on a 50 ms path the cell never finished in 40 s (§14). The advisor reviewed the probe in Revision 1 and missed this.

The probe's motivation was also wrong: the old sweep's crossover moved +29.9 ms with the path too (§14).

**Rules from here on:**
- **The open tail is served by the old sweep, unchanged.** It fires every `P = clamp(2·srtt, 25 ms, 100 ms)` and its eligibility reads the first-send age, as in `recovery_laws.rs::tail_sweep_timeout_us` and the toy's `on_sweep`.
  - The cap's bind at 50 and 60 ms owd stays a recorded finding, since it shapes only the thin early rise.
  - Q13 is withdrawn.
- **Resend gate, every arm.** Off the link cells the toy keeps its own gate (`gen ≥ last_send + owd + J`), so stages 1 to 3b stay identical. In the link cells every arm uses the engine's per-seq cooldown, `now − last_send(s) ≥ max(srtt, 10 ms)` (`recovery_laws.rs::retx_cooldown_us` and `cooldown_elapsed`, with `NACK_RETX_COOLDOWN_FLOOR_US` = 10 ms).
  - Revision 1's "resend only on evidence" gate is withdrawn. Without its timeout half, an open-tail seq whose copy was lost would never be resent.
- **The verdict is taken only on receiver feedback (amends §13.1).**

```text
   verdict(s)  ⇔   a(s) + ô  ≥  T   ∧   s listed missing in a report   ∧   last_send(s)  <  τ_rx
```

  - Here τ_rx is the newest send stamp the receiver has received (RACK's time test, RFC 8985 §6.2, with reorder window 0 on a FIFO path).
  - §13.1's timeout half is deleted, and the per-seq verdict timer (kind 8) is deleted with it.
  - Data nobody has reported missing is never judged. What gives it up is the floor's lifetime term (§15.3), which judges nothing in flight.
- **The brake is unchanged from §13.1:** the outstanding count falls only on receiver feedback.
- **This verdict is unreachable under §15.3, stated plainly.** A seq that fails L2 has `a + ô ≥ T`, so it is unprotected. Every older seq is unprotected too, because it is older and its earliest possible resend is no earlier. So the seq always lies below the floor F, and nothing ever judges it.
  - The gate the owner asked to keep is therefore kept as written but never binds. Giving up is done by the floor's commitment, which judges nothing in flight. The D1 property comes from the feedback-only brake.
  - The `verdicts` and `l2_fail` witnesses are replaced by floor passes (`pass_floor`) and the floor's lag behind the send frontier (`F_lag`).

**Identity is not enough; liveness is checked beside it.** An identity between two arms that both stall would still pass. So the T = ∞ checks are:
- S4 at T = ∞ equals C3 in every cell, including the link cells, with the cell's shared mechanisms (§15.4);
- every T = ∞ arm finishes in every cell (the frontier reaches the last unit within `t_limit`, in every seed).

### 15.2 Diagnosis: why strict order collapses under overload (item 2)

**Data:** stage 3b per-run files, `scratchpad\toy3\tools\toy\out3b\OV` and `CL`, 12 seeds per arm, read with `scratchpad\diag\ov.py`.

| OV arm | share by 0.4 s | share by 2 s | delay from hand-over, p50 | srtt mean | verdicts | floors | consumer stall |
|---|---|---|---|---|---|---|---|
| S3, T = 0.4, H = 0 | 0.815 | 0.815 | 0.390 s | 0.098 s | 2 509 | 3 025 | 0 |
| S3, T = 0.4, H = ∞ | 0.087 | 0.815 | 0.488 s | 0.098 s | 2 509 | 3 025 | one episode, 10.39 s |
| S3, T = 2, H = 0 | 0.165 | 0.961 | 1.203 s | 0.098 s | 586 (3 119 serves) | 754 | 0 |
| S3, T = 2, H = ∞ | 0.086 | 0.751 | 1.394 s | 0.098 s | 586 (3 119 serves) | 754 | one episode, 12.0 s |
| CL: S3, T = 0.4, H = 0 / H = ∞ | 0.848 / 0.847 | — | 0.006 / 0.017 s | 0.011 s | 1 000 | 52 | — |

**Reading.**
1. **At T = 0.4 the strict receiver delivers every unit one round trip late.** The median delay rises by 98.8 ms (0.488 − 0.390), and the mean srtt is 97.8 ms.
   - Every hole is a first send lost at age ≈ T − R. The dequeue rule sends at that age (§13.2), so a resend cannot arrive in time.
   - The hole is detected at the receiver. The report reaches the sender one reverse owd later (5 ms), the verdict and its floor go out **in the same call** (`toy2.py` `act`; floors ≥ verdicts; the floor hold H_f = 0 on the FIFO path), and the floor then waits in the forward bottleneck queue (≈ 87 ms) plus one owd.
   - So the receiver learns of the give-up one srtt after detection.
2. **The hypothesis is half right.** "Holes given up only at the lifetime hold everything behind them": yes. "The floor goes on the next sweep": no.
   - The sweep period there is 100 ms. A sweep-borne floor would add about 50 ms on top of the srtt (≈ 148 ms in all), against the 98.8 ms measured.
   - The cost is the tell's round trip, not the sweep.
3. **Why one hole becomes a collapse.** About 300 holes per second (3 % of 10 000 first sends per second) each block for about 98 ms, so about 30 holes are open at once. The strict frontier never catches up: one consumer stall lasts the whole 10.4 s run.
   - **Collapse condition:** units arrive within one round trip of T (arrival age > T − srtt), and holes come faster than one per round trip.
   - In CL the same arm does not collapse (0.847 against 0.848): arrivals have slack (median delay 6–17 ms), so the round trip of the tell still fits inside T.
4. **T = 2 s strict is a different mechanism.** The extra delay is 191 ms ≈ 2·srtt, with 3 119 resends against 586 verdicts. There the holes are *resent*, and the strict frontier waits for the slowest of the open holes. No floor law can shorten a wait for data that is still coming (§15.3, limit).

### 15.3 The fix: the floor is the lowest seq the sender still means to deliver (refines ADR-0084's floor, §2.2)

"The sender tells the receiver, with its data, below which seq it will not resend; the receiver passes any hole below that once a later datagram has arrived":

```text
   protected(s, t)  ⇔   max( t ,  last_send(s) + c(t) )  −  t_h(s)  +  ô(t)   <   T          (L2 at the earliest resend still possible)
   F(t)             =   running max of   min{ s ≥ A_known : s unresolved  ∧  s ∉ V  ∧  protected(s, t) }
   commitment:          no copy of s is sent once s < F has been announced
   carried:             on every data datagram while some unresolved seq lies below F (and on the sweep's floor repeat, ADR-0084 decision 1)
```

| symbol | provenance |
|---|---|
| c(t) = max(srtt, 10 ms) | the engine's per-seq resend cooldown (`retx_cooldown_us`): the earliest instant after the last send at which a resend may go. In the off-link cells the toy's own gate `owd + J` stands in for it |
| ô = srtt/2, T, t_h, a | as before |
| V, A_known, "unresolved" | ADR-0084, sender state |
| running max | the floor never decreases (ADR-0084 decision 1). Protection is monotone: t grows, and a resend moves last_send only to now. So the max only absorbs estimate noise, and its bind is gauged (`floor_hull_bind`, the fraction of updates where the new minimum is below the old F) |

- **Why it fixes the collapse.** At the overload operating point every unit is sent at age ≈ T − R, so it is unprotected from the moment it is sent. F therefore stays at the send frontier, and the datagram after a lost one already carries `F > s`. The receiver passes s at detection, plus H_f, and that is the "a later datagram arrived and s is missing" test, now run at the receiver. The tell's round trip disappears. Predicted: strict order ≈ H = 0 in OV at T = 0.4 (§15.6).
- **Why it does not give up data that could still be saved.** Protection is L2 evaluated at the earliest resend still possible, not at the current age. At K-L10 a hole whose earliest resend is at age ≈ 0.3 s still arrives by 0.4 s, so it stays protected and is resent as before. Predicted: the K cells' share is not lower than stage 3's (§15.6).
- **It is a commitment, not a verdict.**
  - It judges nothing in flight. On a FIFO path a later datagram arrives after every earlier one, so a seq missing when F arrives is lost. At N ≥ 2 the floor hold H_f covers reordering.
  - The brake stays feedback-only: the outstanding count still falls only on acks.
  - The "judged but arrived" gauge becomes the receiver's late fills of floor-passed holes (`late_floor`), which must read 0 on a FIFO path.
  - Absolute test: no copy of s is ever staged after an announced F > s.
- **Liveness at the lost tail.** When nothing follows the tail, there is no later datagram to carry F. So F is recomputed at **every decision instant, sweep fires included**, and the sweep sends it every P while `F > A_rx`, with no condition on the verdicted set. Floors lost in an outage repeat.
  - This matters because ADR-0084's repeat is guarded by "pending verdicts exist", and under §15.3 V stays empty, so that guard would never send. F would also go stale in an outage if it were recomputed only at sends and reports.
- **At T = ∞,** `protected` is always true, so F is ADR-0084's verdict floor. With no verdicts nothing lies below it and nothing is carried or sent, so the trace is unchanged.
- **Continuity:** F is a running max of a minimum over terms continuous in T (through t_h + T).
- **Limit, stated before the run:** it cannot help T = 2 s strict in OV. Those holes are protected and resent, and the frontier waits for them (§15.2 item 4). Predicted unchanged.
- **Wire (owner question Q15).**
  - Option (a): a field in the data header, 8 bytes or a smaller delta from the batch's own seq.
  - Option (b): a `WindowFloor` datagram at most once per 2 ms (the engine's report interval, `GAP_ACK_MIN_INTERVAL`) while F rises. One per emission would be one per data datagram under overload, because F sits at the send frontier there. At 500 per second against C = 10 000 per second, (b) takes 5 % of the link under overload.
  - **Default (a).**
  - Either way this changes ADR-0084's floor meaning, from "below this, every unresolved seq is verdicted" to "below this, the sender will not resend". That is an amendment of decision 1, and it needs a wire version.
  - The toy models (a) at zero link cost: F rides on the data event. A registered arm S4b models (b), so (b)'s link cost is measured.

### 15.4 Fair baselines: the resend brake (item 3; the brief's premise is wrong)

**What the engine does.** The coordinator's brief said the engine gates every send by `cwnd_full`. The code says otherwise:
- `gates/scheduler_gates.rs:280–284`: the SACK-gap retransmit and the NACK repair "neither read `available()` or `cwnd_full`; they are budgeted by `cached_nack_budget` and the NACK congestion multiplier".
- `net/mod.rs:2416–2436`: `cwnd_full = brake_armed && …`, armed only by the composed in-flight cap or `RWM_LATE_BRAKE`. ADR-0084 decision 7 keeps the cwnd brake unarmed.

So in the engine, fresh data is bounded by the store cap B (the pause the toy already has), and resends by a budget of their own:

```text
   resends allowed per refresh  =  max( nack_cap·sources_this_period − nack_repairs_this_period ,  MAX_NACK_REPAIRS_PER_NACK )      (net/mod.rs:3309–3333)
   per serving pass             ≤  round( MAX_NACK_REPAIRS_PER_NACK · multiplier )                                                    (net/mod.rs:3296, :3367)
   refresh every NACK_REPAIR_COOLDOWN_US = 5 ms;   MAX_NACK_REPAIRS_PER_NACK = 10;   multiplier ∈ [0, 1], halved on rising loss and RTT, +0.1 when stable, ≥ 0.1 when idle   (net/mod.rs:227, :229, :266–345)
```

**The toy's brake (selector `resend_brake = "engine"`, every arm in the link cells; default `"none"` keeps stages 1 to 3b identical):**
- at most **10 resends per 5 ms refresh window** (2 000 per second);
- windows are aligned to multiples of 5 ms from t = 0;
- every copy counts, from serves, sweeps and verdict copies.

The toy simplifies the engine in two ways:
- **The multiplier is held at 1.** It can only lower the budget, and modelling its loss-and-RTT detector is a second law. This favours the old arms, which resend more, so it is the conservative side for the comparison.
- **The `nack_cap·sources` term is omitted.** At 3 % loss and 50–60 source symbols per 5 ms it is below 1, so the floor of 10 binds.

Behaviour of the brake:
- **Retry:** a braked resend is not queued. It is retried at the next decision instant that lists it (a report or a sweep) and counted as `resend_braked`.
- **Who gets a slot:** fresh data and resends have separate budgets, as in the engine (store cap B against the NACK budget), so neither starves the other. Both share the link FIFO.
- **Exempt:** floors. They are control traffic, few and payload-free, and the engine's floor goes on its control lane.
- **The brake does not key on the outstanding count,** which sits at B after an outage, so it cannot wedge resends.

**Checked against the engine.**
- The budget is decremented once per resend at the gap serve and the margin repair (`sender_phases.rs:1097–1098`, `:1157–1158`) and at the request arm (`net/mod.rs:3509–3510`).
- The engine's tail sweep feeds `serve_gaps` with the cumulative blocker (`net/mod.rs:3036–3043`), so it draws on the same budget, and the toy counts sweep resends against it too.
- **Recorded divergence, not changed:** the engine's sweep resends only the cumulative blocker per fire, while the toy's sweep resends the whole open tail. The owner asked to keep the toy's old sweep. The divergence favours the old arms' tail recovery in the toy.

### 15.5 Readouts and controls (item 4)

- **Pause, by window.** For every 1 s window, report first sends, resends, braked resends, the time paused (O ≥ B) and link utilisation, plus counts of blocked send attempts by cause (pause, resend budget, cooldown).
  - `pause_time_w` is retired: it read 9 s of 9 for every OV arm, which measures the run length.
  - The OV validity witness stays `link_util_w ≥ 0.98` over [1 s, 10 s].
- **The missing link-cell rows.** T ∈ {0.4, 2, ∞} × H ∈ {0, ∞} for S4 in both CL and OV, plus C1 and C3 (C3 = S4 at T = ∞, H = ∞ by identity, run as both).
- **The continuity control, placed per seed from the same cell's T = ∞ run, off the ramp** (cell E-I2n, the 50-unit tail):
  1. run S4 at T = ∞, H = ∞ for each seed;
  2. take the delivery ages `D_u = dlv − h` of the dead-tail units;
  3. set `p_seed = ceil_1ms(max D_u + 2 ms)`;
  4. the seed is usable only if `min D_u > 0.99·p_seed + 2 ms`, that is, the whole ramp lies between the nudge points 0.99p and p. Otherwise it is reported as "ramp too wide" and excluded;
  5. from the same run, compute `F_seed(T) = |{u : D_u ≤ T}| / units`, and from it `K̂ = 2·[F(1.01p) − F(0.99p)] − [F(1.02p) − F(0.98p)]` and `P̂ = 2·[F(p) − ½(F(0.99p) + F(1.01p))] − [F(p) − ½(F(0.98p) + F(1.02p))]`. With the step between 0.99p and p and no other mass nearby, `K̂ = U − L` and `P̂ = (U − L)/2`, where U − L is the tail's share, both non-zero;
  6. then run that seed at T = p_seed·{0.98, 0.99, 1, 1.01, 1.02}, H = ∞.

  Placing the centre on the median would put p in the middle of the ramp, where 1 ms of rounding moves `share_T(p)` by about a fifth of the step. A pooled placement would smear the step by the sweep phase (up to 25 ms per seed). L2 gives up a fill at about `send age + ô` rather than at its T = ∞ delivery age; the 2 ms margins absorb that. K and P are then read as registered (§13.7.4).
- **Stall decomposition, registered up front.** The outage is taken **at the receiver**, as `[start + owd, end + owd]`, because `dead` is read at send or departure. For the consumer stall episode that overlaps it (K, E and CL cells, strict arms only), every part is clamped at 0:
  - (o) episode start → receiver-side outage start;
  - (oo) receiver-side outage start → receiver-side outage end;
  - (i) outage end → arrival of the first floor or F-carrying datagram after it;
  - (ii) → the last fill inside the episode;
  - (iii) the rest.

  An episode that does not span the outage is reported as "no outage episode".

### 15.6 Stage 3c registration

Build on `toy-stage3-knobs` `d0ce4f9d` in `tools/toy/`: extend `toy2.py`, and add `batch3c.py`, `summarize3c.py` and `check_ident3c.py`. **Stages 1, 2, 3 and 3b must reproduce bit-identically** with every new field at its default. `check_ident3c.py` compares against the committed stage 3 and 3b outputs on every key and on traces for 3 seeds, and runs first.

#### 15.6.1 New code (exact)

New `Cfg` fields (defaults reproduce 3b):
- `floor_law: str = "verdict"`: `"protect"` = §15.3;
- `floor_carrier: str = "header"`: `"datagram"` = §15.3 option (b);
- `resend_gate: str` gains `"cooldown"`;
- `resend_brake: str = "none"`: `"engine"` = §15.4;
- `verdict_evidence: str = "rev1"`: `"feedback"` = §15.1.

**Sender `"S4"`** is S3 with:
- `tail_elig = "P"` (no probe; kind 9 is never pushed);
- `verdict_evidence = "feedback"`: no kind-8 events are pushed; in `act`, a candidate failing L2 gets a verdict only if it is in the report's missing set, the report is current (`gen > last_send[s]`) and `last_send[s] < tau_rx`;
- `floor_law = "protect"`.

`rtt_est = "rfc"`, `deq_expiry = True` and the feedback-only brake are kept from S3.

1. **`protected(s, now)`** (module-level law function; add it to the deny test's `EXEMPT`):

   ```python
   max(now, last_send[s] + c) - h_seq[s] + srtt_eff / 2 < T
   ```

   - `c = max(srtt_eff, 0.010)` in link cells (`resend_gate == "cooldown"`), else `owd + J`;
   - `srtt_eff` is the RFC estimate, with the 0.333 s seed before the first sample.
2. **The floor F**, kept as `self.F_ann` (initially 0) and recomputed at every send and every report:
   - scan upward from `max(self.F_ann, self.A_known)` past every seq that is known received, verdicted, or not `protected`;
   - stop at the first unresolved, unverdicted, protected seq, or at `n_sent`;
   - set `F_ann` to the larger of the old value and the stop point.

   - Unresolved means `s ≥ A_known` and (`s ∈ M_known` or `s > rhi_known`), as stage 3 has it.
   - Recompute also at every sweep fire (`on_sweep`, before `floor_repeat`).
   - In `act` and `floor_repeat`, under `"protect"`, the floor sent is `F_ann`, and the send condition is `F_ann > A_known and F_ann > last_floor`, with no `pending` test. `floor_repeat` drops its `if not self.pending: return`, and repeats while `F_ann > A_known`.
   - At T = inf, `F_ann == A_known` always, so nothing is sent.
3. **The carrier:**
   - `"header"`: in `emit` and `send_copy`, when the datagram is delivered (not lost) and some unresolved seq lies below `F_ann` (`F_ann > lowest unresolved`), push a floor event of kind 1 with value `F_ann` at the **same arrival time**, after the data event (use the next counter value). It takes no link slot and no separate loss draw.
   - `"datagram"`: at most one `send_floor(now, F_ann)` per 2 ms window (`int(now / 0.002)`), when `F_ann` has risen since the last one sent. It takes a link slot as stage 3b's floors do.
   - `floor_repeat` on the sweep sends `F_ann` under both carriers.
4. **Commitment:** in `send_copy`, under `"protect"`, assert `s >= F_ann` (a test seam counts violations; the test expects 0). Callers skip a candidate below `F_ann`.
5. **Resend gate `"cooldown"`:** `can_resend(s, now)` is `copies[s] == 0 or now - last_send[s] >= max(srtt_eff, 0.010)`, for every arm.
6. **Resend brake `"engine"`:** a counter per 5 ms window (`int(now / 0.005)`). `send_copy` proceeds only if the window's count is below 10, else it increments `resend_braked` and returns without sending. Floors are not counted.
7. **Readouts** (all arms):
   - `share_by_0p4` and `share_by_2`;
   - delay from hand-over p50, p90 and p99 over all units handed over (undelivered and expired at +inf);
   - the per-window series of §15.5 (first sends, resends, braked resends, pause time and link utilisation per 1 s window, and blocked attempts by cause);
   - `queue_drop_in` and `queue_drop_out`, `late_floor`, `floor_hull_bind`, `resend_braked`;
   - `finished` (bool) and `finish`;
   - `pass_floor` (floor passes);
   - the stall parts (o), (oo), (i), (ii) and (iii);
   - `F_lag`: the mean over sends of `n_sent − F_ann`, how far the floor trails the send frontier.

   No mean is taken over delivered units alone.
8. **Tests, red first:**
   1. `check_ident3c.py` (stages 1 to 3b reproduced). The stage 3 and 3b per-run outputs are git-ignored (`out3/`, `out3b/`), so the references are regenerated first. Run every configuration of the check (stage 3 decision 17's 34, plus one per stage-3b arm and cell) at 3 seeds from commit `d0ce4f9d` in a separate worktree, store them under `out3c/ref/`, and compare the new code's outputs on every key and trace;
   2. S4 at T = inf equals C3 in every cell type, link cells included (3 seeds, every key, trace);
   3. S4 at T = 1e6 equals T = inf;
   4. the commitment seam reads 0;
   5. under `"protect"` at T = inf, no floor is ever carried or sent;
   6. the brake: no 5 ms window has more than 10 copies;
   7. the deny test, with `protected` exempt and a planted positive control.

#### 15.6.2 Cells

All cells have one path, 12 seeds and phases drawn per seed.
- **E-I2, E-L10, M-I2, M-L10, K-I2, K-L10:** stage 3's cells.
- **E-I2n:** §13.7.2.
- **CL and OV:** §13.7.2, unchanged. Every arm also runs `resend_gate = "cooldown"`, `resend_brake = "engine"`, `rtt_est = "rfc"` and `src_model = "wc"`.
- **X15, X20, K-L10, K-L10b:** the crossover cells, for S4 (old sweep) at H = 0, on §13.7.3's T grids.

#### 15.6.3 Arms per cell

- C1 = S1c + R0e at ρ = 0.99.
- C3 = S1c + R1 at ρ = 1.
- S4 at T ∈ {0.4, 2, ∞} × H ∈ {0, ∞}.
- S4 at T = 1e6, H = ∞.
- S4b (carrier `"datagram"`) at T = 0.4, H = ∞, in OV, CL and K-L10.
- The nudges: T = 0.4·{0.98, …, 1.02} at H = 0.02, and H = 0.02·{…} at T = 0.4, in every cell except the X cells.
- The per-seed positive control in E-I2n (§15.5).
- The A/A: S4 at T = inf, H = inf, phase variants 0 and 1, in every cell, frozen as `nf_stage3c.json` before any decision cell.

Run order:
1. tests;
2. identity and liveness;
3. A/A;
4. the control's placement;
5. everything else.

#### 15.6.4 Predictions (written before any run)

**Bands from stage 3b's T = ∞, H = 0 rows.** Those rows ran with the R probe and without the brake, but they had few resends (0.03 to 0.08 per delivered unit), so they are the nearest measured stand-in for a braked baseline. The bands below are widened for that reason.

- **Q3c-1, identity and liveness.**
  - Stages 1 to 3b are reproduced.
  - S4 at T = ∞ equals C3, and T = 1e6 equals ∞, in every cell and seed, the link cells included.
  - `finished` is true for **every arm at every T** in every cell and seed. A finite-T arm finishes when its frontier passes the last unit by fill or floor.
  - FAIL: any difference, or any unfinished run.
- **Q3c-2, the commitment and the brake.**
  - The commitment seam reads 0.
  - `late_floor = 0` in every one-path cell.
  - `queue_drop_out = 0` for every arm in CL and OV, and `queue_drop_in` is reported.
  - No 5 ms window exceeds 10 copies.
- **Q3c-3, the collapse fixed (OV, T = 0.4):**
  - S4 at H = 0: `share_by_0p4` ∈ [0.78, 0.83] (3b: 0.815);
  - S4 at H = ∞: ∈ [0.75, 0.83], and at least the H = 0 value − 0.03 (3b: 0.087);
  - S4b at H = ∞: `share_by_0p4` within ± 0.02 of `S4(H = ∞) × (1 − floor slots per second / C)`. The predicted factor is 1 − 500/10 000 = 0.95, so about 0.77. `queue_drop_out = 0`.
- **Q3c-4, the limit (OV, T = 2):**
  - H = ∞ `share_by_2` ∈ [0.65, 0.85] (3b: 0.751; not fixed, by §15.3's limit);
  - H = 0 ∈ [0.90, 0.99].
- **Q3c-5, fair baselines (OV):**
  - C3 and S4 at T = ∞, H = ∞: `share_by_0p4` ∈ [0.05, 0.20];
  - S4 at T = ∞, H = 0: ∈ [0.12, 0.21] (3b: 0.165);
  - C1 reported;
  - every arm's link utilisation in [1, 10] s ≥ 0.98, else the cell is invalid for that arm.
- **Q3c-6, the drain gain (CL):**
  - C3: `share_by_0p4` ∈ [0.25, 0.47] (§13.6 formula ≈ 0.37; 3b's T = ∞, H = 0 row: 0.318), and `share_by_2` ≥ 0.95;
  - S4 at T = 0.4: ∈ [0.78, 0.90] at both H, with H = ∞ within 0.02 of H = 0;
  - S4 at T = 2 against S4 at T = ∞ at the same H: equivalent on both shares;
  - S4 at T = 0.4 minus C3 on `share_by_0p4`: shown larger by more than 0.25.
- **Q3c-7, no loss of savable data (K cells):** S4 at T = 0.4, H = ∞ `share_by_0p4` ≥ stage 3's S2 at T = 0.4, H = ∞ value − m. `F_lag` is reported, with no prediction: with nothing given up the floor trails by the in-flight count.
- **Q3c-8, crossover (S4, old sweep, H = 0):**
  - mid-level crossing shifts: X15 → X20 +15 ± 4 ms, K-L10 → K-L10b +30 ± 5 ms;
  - each crossing within ± 5 ms of the run's own median report-driven resend age plus srtt/2;
  - at each grid's lowest T, zero resends in every seed.
- **Q3c-9, continuity:**
  - the T and H nudges read equivalent on K and P in every cell (shares, delay p50 and p90, ooo, repair per delivered);
  - in E-I2n, over the usable seeds, the per-seed control reads K and P "shown". Mean K lies within ± 50 % of the mean K̂ (≈ U − L ≈ 50/12 147 ≈ 0.0041) and mean P within ± 50 % of the mean P̂ (≈ 0.0021). At least 8 of 12 seeds must be usable, else the control is "not placed", which is a finding.
- **Q3c-10, stall parts (K cells, S4 at T = 0.4, H = ∞):**
  - (i) ≤ P + owd + 5 ms in every seed with an outage episode;
  - (o) ≤ owd + 1/rate + 2 ms where it is non-zero (a hole opened in the last owd before the outage);
  - the same parts for C3 are reported.

Report every reading, the A/A self-check and every deviation, as in stage 3b.

### 15.7 Effect on the earlier sections, and what stays unfixed

**Superseded or amended:**
- §13.4 and Q13 are withdrawn.
- §13.1's timeout evidence and the per-seq verdict timer are deleted; verdicts need feedback (§15.1).
- §2.2's verdict floor becomes §15.3's protected floor, amending ADR-0084 decision 1 (Q15, wire form).
- §13.7's resend gate "evidence" is withdrawn in favour of the engine's cooldown.
- §10's engine sketch:
  - step 5 reads: protected floor carried on data, feedback-only verdicts, dequeue expiry, no probe;
  - a gauge for `F_lag` and the hull bind is added;
  - the guard keeps §13.8's Realtime datagram cell and capacity cell.

**New owner questions:**
- **Q15:** the floor's wire form (header field or floor datagram). Default: header.
- **Q16:** model the NACK multiplier in the toy. Default: no. It only lowers the old arms' resends, which would make the comparison kinder to the new arm; the direction is stated.

**Unfixed:**
- strict order at T = 2 s under overload (§15.2 item 4: the frontier waits for holes that are being resent);
- delay pinned near T − R under overload (Q12);
- the strict stall through the outage itself (§13.5).

## 16. Findings from the offline simulation, stage 3c (2026-11-24, reviewed with the advisor)

Branch `toy-stage3-knobs` `02eda90b`, `tools/toy/results-stage3c.md`. One path, no proactive repair.

**Confirmed.**
- T = ∞ equals full delivery exactly in all 12 cells, link cells included (288/288, traces identical), and all 3 646 runs finish (Q3c-1). D2 is gone.
- No queue drops outside or inside the outage in any link-cell run (Q3c-2, bullets 2–4).
- The strict-order overload collapse is fixed: OV at T = 0.4, H = ∞ reads 0.801 on time against 0.087 in stage 3b; H = 0 reads 0.831 (band 0.78–0.83, missed by 0.0008; the strict margin passed by 0.0003).
- The crossover follows the path: +14.9 ms and +29.9 ms, each crossing within 0.7 ms of median resend age + srtt/2.

**What the capacity cell says, and what it does not.** Full delivery reads 19.9 % within 0.4 s in both 3b and 3c, although 3c removed the resend storm. By arithmetic (λ = 10 000/s, C = 12 500/s, a 1.5 s outage at 2 s): the backlog of about 15 000 units drains at C − λ = 2 500/s, so it is not on time again until about 9.5 s, near the end of the 10 s run, and 19.9 % is the 2 s before the outage. The lifetime arm's 84.8 % is 1 minus the outage's share. So the +65 points measure the part of this run after the outage and would shrink in a longer run; the registered readout to use is recovery time after the outage, per arm. Order alone moves 19.9 → 21.4 % (CL) and 6.7 → 13.7 % (OV); the lifetime does the rest. Its cost: 84.8 % ever delivered against 97.2 % within 2 s and 100 % ever for full delivery.

**Failed.**
- **The floor's commitment (no copy below it) failed:** 143 copies below the floor in 28 of 3 454 runs, all in link cells. Cause: §15 contradicts itself. Item 1's `protected` makes a freshly sent unit unprotected for one cooldown, while item 5's gate lets a seq with no copy go at once, and the floor moves inside the same `act` loop. A spec error the review missed. An unregistered supplement (cooldown on first resends too) reads 0 violations but moves the baselines by up to 4 points, so it is not a result. Check whether the same exemption explains the 598 resends per seed at K-L10b's lowest T.
- **The feedback-only verdict rule never fired** in any S4 run (candidates below the floor are filtered first). Either delete it or find a case that exercises it. Stage 3b's "D1 fixed" belongs to Revision 1's arm.
- **The 2 s lifetime under overload** read 0.908 by 2 s, above its band; only 6 % was ever dropped, so that lifetime barely bound in a 10 s run. Unread, not fixed.
- **Smoothness is still not demonstrated:** the control failed to place a third time (0/12, then 2/12 usable seeds). Use stage 2's planted STEP and KINK arms as the control instead of hunting for a natural step.
- **Baselines remain qualified:** about half the old arms' copies are duplicates of data still queued in the link (the simulation's 25 ms sweep resends the whole open tail; the engine's resends only the blocker), which costs them capacity. The simulation's sweep therefore likely disfavours the old arms, not favours them.

**Untested risk.** A floor on every data datagram tells the receiver to pass anything still in flight below it. Harmless in order on one path; on two paths it would pass slow-path data that is about to arrive. Two paths must be tested before any engine sketch.

**Register before the next run:** the item-5 fix (cooldown on first resends, re-check of the floor before each copy), with the link cells rerun; an engine-like sweep arm (blocker only); recovery time after the outage per arm; the planted STEP/KINK control; an ablation flipping `floor_law` alone in OV; a two-path cell with skew.

## 17. Revision 3 (2026-11-24, after stage 3c; owner: "yes Start the run", with Fable)

This section supersedes earlier sections where it says so. Sections 1 to 16 are kept as the record.

The advisor (Fable) reviewed these, before writing:
- the seam's cause and its law, including its own miss in reviewing §15;
- the K-L10b resend count as a separate cause;
- the engine-like sweep, which needs its period from measured srtt and a receiver refresh, and which recovers a tail with nothing after it one seq per fire;
- routing the planted control through T;
- the two-path model and its one-path assumptions;
- the window readouts for recovery time and for the 2 s lifetime;
- the verdict rule's fate.

The one-way term ô = srtt − RTprop/2 (§17.1) is my choice after that review. It was checked in the final review of the whole section, which also changed six things:
- per-path RTT samples, and the reorder rule applied to report-driven candidates only (§17.7);
- RTprop pinned to the exact propagation term;
- S5v given stage 3b's timeout for the open tail;
- CL-long sized from stage 3c's measured drain rate;
- each plant's K and P computed from the T_eff map;
- the blocker fire repeating the floor.

### 17.1 The commitment seam (§16 item 1): one trigger function, the right one-way term, and a re-check

**What §16 proposed is not the engine's gate.** The engine applies its per-seq cooldown only to a seq that already has a retransmit on record (`sender_phases.rs:873`: `if let Some(&(last, _)) = st.nack_retx_at.get(&seq)`). A first resend is exempt. So "cooldown on first resends too" would change the gate that every arm uses, and stage 3c's supplement moved the baselines by up to 4 points. It is not adopted.

**The cause, stated exactly.** §15's `protected` put the earliest resend of every seq at `last_send + cooldown`. For a never-resent seq the delay is not the cooldown but its **trigger**: the first instant the sender's own rules can make it a candidate. That is a report listing it, about one srtt after its send on its path, or the sweep reaching it. The law and the gate disagreed by exactly that difference.

**The law (supersedes §15.3's `protected`).** "A seq is protected while a resend at its earliest trigger could still arrive before T":

```text
   protected(s, t)  ⇔   max( t ,  last_send(s) + d(s) )  −  t_h(s)  +  ô   <   T
   d(s)  =  c                                        if s has been resent        (the cooldown: the engine's gate, sender_phases.rs:873)
         =  min( srtt_{path(s)} ,  P_reach(s) )      otherwise                    (its trigger: a report, or the sweep)
   ô     =  srtt_{resend path}  −  RTprop_{resend path} / 2
```

| symbol | provenance |
|---|---|
| c = max(srtt, 10 ms) | the engine's cooldown (`recovery_laws.rs::retx_cooldown_us`) |
| srtt_{path(s)} | measured, per path: the report path's trigger delay |
| P_reach(s) | the sweep's eligibility for s: the sweep period P under the whole-tail sweep; P for the cumulative blocker and ∞ for every other seq under the blocker-only sweep (§17.3) |
| RTprop | the propagation round trip. In the toy it is the exact `2·owd` of the resend path, the same idealisation as stage 2's D_RTprop arm. In the engine it is the windowed minimum RTT (paper §6.1). That windowed form has a known limit: under a standing queue longer than its window (10 s), the minimum reads the queue, and ô falls back towards srtt/2. Recorded under Q18 |
| ô | the one-way delay of a resend, assuming all queueing is on the data direction (declared; exact when the reverse path is unqueued, which holds for a sender's own bottleneck, since acks are small). It replaces srtt/2 in L2 and in `protected`. With no queue it equals srtt/2, so the call cells are unchanged |

- **Why ô moves.** In OV the forward queue is about 87 ms. The true one-way delay of a resend is then about 0.092 s, against srtt/2 = 0.049 s, so the symmetric split protected a unit whose resend could not arrive in time. `srtt − RTprop/2` reads 0.093 s there.
- **Why both changes are needed.** With the trigger term alone and the toy's 25 ms sweep, a fresh OV unit would read protected, the floor would leave the send frontier, and the stage 3b strict-order collapse would partly return. With ô corrected, the same unit is unprotected at birth (0.325 + 0.093 ≥ 0.4), whatever the sweep form.
- **The trigger is computed by the function that picks candidates.** `P_reach` is read from the same code that builds the sweep's list, so the law cannot drift from the gate again.
- **The re-check makes the commitment absolute.** Immediately before each copy is staged, test `s ≥ F_ann` against the current floor. A candidate that fails is skipped and counted (`skip_below_floor`).
  - The law makes skips rare: a report earlier than its srtt estimate can still produce one.
  - The re-check makes copies below the floor impossible. Absolute test: 0 copies below the floor, in every run.
- **A copy in flight below the floor.** After a copy, `last_send = now` and `d = c`, so a seq can become unprotected while its copy flies. On one FIFO path the floor rides a later datagram and arrives after the copy. On two paths the copy goes on the fast path (§17.7), so the same holds; §17.7 tests it.
- **Continuity:** d(s) branches on whether s was ever resent. That is a state, not a knob. ô is continuous. At T = ∞ the law is inert.

**K-L10b's 598 resends at its lowest T are a different cause.**
- About 300 frames are lost there (10 % of 3 000), yet 598 copies are sent, so at least half are copies of frames that arrived.
- The old sweep's eligibility is a first-send age of 100 ms (`clamp(2·srtt, 25, 100 ms)` with the cap binding), while a frame's ack returns at about 122 ms (2·60 ms + the report tick). So the sweep resends frames still awaiting their ack. Every sweep fire catches the frames whose age falls in [100, 122) ms, about 22 % of them.
- Some of those copies pass L2 and could arrive before T, which is why the "zero resends at the lowest T" bullet failed.
- This is the sweep cap's bind (§13.4, recorded), not the seam. It is confirmed by the `dup` gauge (Q3d-2), not fixed here. Q17 asks the owner whether the sweep's eligibility should be at least the ack timeout, `max(P, R)`, with no probe and the whole-tail list kept.

### 17.2 The feedback-only verdict (§16 item 2): deleted from the design, kept as the ablation's give-up

**It cannot fire under the protected floor, by construction.** L2 failing now implies unprotected (the earliest trigger is at or after now), and every older seq is unprotected too, so the seq is already below F. No cell can exercise it there.

So it is **deleted from the design** (S5 below). It survives as the give-up rule of the floor-law ablation (§17.6), the one arm where it is reachable. There it gets tests on real state, a verdict-count witness and `finished`. It has no evidence for a lost tail (nothing arrives after it), so the ablation runs only in OV, which has no tail.

### 17.3 An engine-like sweep for every arm (§16 item 3)

Selector `sweep_form ∈ {"tail", "blocker"}`, applied to every arm in a cell:
- **`"tail"`:** today's toy sweep. Every open-tail seq older than P is resent at each fire, with the period fixed from the base RTT (`sweep_P = clamp(2·rtt_base, 25, 100 ms)`, set once).
- **`"blocker"`:** the engine's (`net/mod.rs:2783–2811`, `:3029–3043`).
  - The deadline is `max(last_send(blocker), last sweep fire) + clamp(2·srtt_pooled, 25 ms, 100 ms)`, recomputed at every event from the **measured** pooled srtt.
  - On expiry, only the cumulative blocker (`A_known`) goes to the serve path. The cooldown, the resend budget, L2 and the floor re-check all apply.
  - Each fire also recomputes the floor and repeats it while `F_ann > A_known`, as the whole-tail sweep's `floor_repeat` does. Without this, a finite-T tail with nothing after it would never be passed.
  - **The receiver refresh is modelled too:** while the receiver holds holes, it re-sends its report every `clamp(2·srtt_rx, 25 ms, 100 ms)` (`recovery_laws.rs::hole_nack_refresh`, `HOLE_NACK_REFRESH_MIN` and `_MAX` = 25 and 100 ms), so reported holes are re-presented. The toy's receiver otherwise reports only on arrival. `srtt_rx` is the sender's current `srtt_pooled`, read directly. That is an idealisation: the engine's receiver keeps its own SRTT, the maximum over paths.

**Where it runs.** The blocker sweep recovers a tail with nothing after it one seq per fire: the blocker's copy arrives, the next seq becomes the blocker, and so on. The engine has that too. There coded repair (absent from the toy) also covers the tail. So `"blocker"` runs only in cells where data follows every hole: OV, CL, CL-long and the K cells. The E cells keep `"tail"`.

**Recorded finding for the engine:** a lost tail with no repair recovers one seq per tail-sweep timeout. This is already implied by ADR-0084's lost-tail precheck, and it is a question for the repair plane, not for this design.

### 17.4 Recovery time after the outage (§16 item 4), and a longer capacity cell

- **Windows** are indexed by hand-over time: width 0.5 s, step 0.1 s. Each window's on-time share counts the units handed over in it and delivered within 0.4 s of hand-over; expired and undelivered units count as late.
- **Pre-outage level `L0`:** the on-time share over hand-over [0.5 s, outage start − 0.1 s]. `sd0` is the standard deviation of the pre-outage window shares.
- **Threshold:** `θ = L0 − max(0.02, 3·sd0)`.
- **Recovery time:** `t_rec − outage_end`, where `t_rec` is the start of the first window at or after the outage end such that every window from it to the readable end has share ≥ θ. The readable end is the hand-over time of the last unit minus 2 s, so the object's tail is never read. If no window qualifies: "not within the run".
- **CL-long:** CL with `n_total = 200000` (20 s), outage [2.0, 3.5) s, `t_limit = 80`.
  - **Sized from stage 3c's measured drain, not from §13.6's formula.** In 3c's CL runs, C3's per-window series after the outage reads about 11 750 first sends and 750 resends per second at full link use. Its drain surplus is therefore about 1 750 units/s, not C − λ = 2 500.
  - The backlog at the outage end is about 15 000 units. Units are on time again once it falls to about `(0.4 − d_fwd)·11 750 ≈ 3 760`.
  - So recovery takes about (15 000 − 3 760)/1 750 ≈ 6.4 s under `"tail"`. Under `"blocker"` the duplicates leave, and with a surplus of about 2 200/s recovery takes about 5.1 s.
  - That matches stage 3c's CL reading: nothing was on time after the outage within 10 s.
  - The readable end at 18 s is more than 14 s after the outage end, above twice the slowest predicted recovery.
  - C1 ran 37 s per 100 000 units in 3c, growing faster than linearly, so a CL-long run is a few minutes.

### 17.5 Smoothness: planted controls through T (§16 item 5)

Stage 2's `plant=` scales design U's hold inside `ho_now`, which S5 never reaches. So the stage 2 shapes are kept but routed through the nudged knob, in one named, deny-exempt function `plant_T(cfg, T) → T_eff`, with p the planted point:
- **TSTEP:** `T_eff = 2·T` if T < p, else T;
- **TSPIKE:** `T_eff = 2·T` if T = p, else T;
- **TKINK:** `T_eff = T·(1 + 25·max(0, 1 − T/p))`.

These are the stage 2 §7.9 factors. `T_eff` enters every sender law (L1, L2, the dequeue rule, `protected`). Readouts use the declared T, so a plant that bites shows up as a share change against the stated lifetime.

**Cells.**
- **OV at p = 0.4.** The on-time share there is set by the dequeue threshold, so a plant bites: TSTEP doubles the threshold below p, and the units sent arrive after the declared T.
- **K-L10 at p = 0.4.** The share is flat in T there, so a plant does not bite. The prediction is "no power" (K and P equivalent for the plants too), and that is not counted as a pass.

**Readout:** `share_T` only. The delay p50 carries a curvature term of about 2 ms against a 1 ms bar, so it is not used for the plants.

**Expected K and P, computed from the T_eff map before the run (OV).**
- In steady overload a unit is on time only while `T_eff − R + d_fwd ≤ T`, that is `T_eff ≲ T + 10 ms`. So `share_T` is a cliff in T_eff: about 0.80 when `T_eff = T`, and about 0.15 (the run's first ≈ 1.5 s, before the backlog binds) when T_eff exceeds T by more than 10 ms.
- **TSTEP:** points at 0.98p and 0.99p read about 0.15; the rest read about 0.80. So `K ≈ 0.65` and `P ≈ 0.33`.
- **TKINK:** at 0.99p the factor is 1.25 and at 0.98p it is 1.5, both far past the cliff, so the same values as TSTEP: `K ≈ 0.65`, `P ≈ 0.33`. In OV a kink behaves as a step, because the readout is a cliff.
- **TSPIKE:** only p reads 0.15, so `K ≈ 0` and `P ≈ −0.65`.

### 17.6 The floor-law ablation in OV (§16 item 6)

Arm **S5v** is S5 with `floor_law = "verdict"` and nothing else changed. Its give-up rule is §15.1's feedback verdict: L2 fails, the report lists s, the report is current, and `last_send(s) < τ_rx`. The floor is ADR-0084's verdict floor, sent at the verdict (as in stage 3b).

OV has an end, and a lost last unit has no later arrival to give feedback evidence. So S5v also takes stage 3b's timeout evidence for the open tail only (`now ≥ last_send + R` for `s > rhi_known`), exactly as Revision 1 had it. That keeps every S5v run finishing. It is stated as part of the ablation arm, not of the design.

Cells: OV and OV-long. Arms: T = 0.4 × H ∈ {0, ∞}.

### 17.7 Two paths with skew (§16 item 7)

**Model.** It extends stage 2's unbuilt Part C (§7 on `toy-sim-stage2`: φ_slow placement, copies on the fast path). It is selected by `paths = [dict(owd=0.010, J=0.001), dict(owd=0.060, J=0.001)]`. With `paths = None` the code is unchanged.
- **Delay:** one `Delay` per path, its knots drawn with the path index in `u01`'s path slot (`u01(seed, k, 0, p, SALT_KNOT)`; path 0 keeps today's draw). Each path is FIFO; the two are not ordered against each other. `fifo_note` is kept per path.
- **Loss:** i.i.d. per datagram at the cell's rates, drawn as `u01(seed, seq, copy, p, SALT_LOSS)`.
- **Placement:**
  - a first send goes on path B iff `u01(seed, s, 0, 0, SALT_PLACE) < φ`, with φ = 0.5;
  - every copy goes on A (the fast path);
  - a floor datagram goes on both paths (ADR-0084: every live path);
  - a header-carried floor rides whatever data datagram carries it.
- **Reports** travel on A's reverse direction (owd 10 ms). τ_rx is the maximum send stamp over all arrivals on any path.
- **RTT:**
  - every report carries `rhi_p`, the newest received seq per path. Each path keeps its own RFC 6298 srtt and rttvar, sampled as `now − sent_t[rhi_p]` whenever `rhi_p` rises and that seq was never resent. RTprop is the exact `2·owd_p`.
  - The single `rhi` would never sample the slow path: at 10 000/s a slow-path unit arrives about 500 seqs behind the fast path, so it never raises `rhi`, and `srtt_B` would sit at the 0.333 s seed;
  - `srtt_pooled` is the maximum over paths (the receiver's SRTT and floor-hold read the maximum; ADR-0084 Am. 3 notes);
  - c, R and P read `srtt_pooled`;
  - `d(s)` reads `srtt_{path(s)}`;
  - ô reads path A (the resend path);
  - `sweep_P = clamp(2·max path rtt_base, 25, 100 ms)`.
- **Reorder rule, every arm:** a **report-driven** hole is a resend candidate only if `last_send(s) + skew_hat < τ_rx`, where `skew_hat = srtt_B − srtt_A` (measured). That is RACK's time test (RFC 8985 §6.2) with the reordering window set to the measured skew.
  - Sweep candidates keep their own eligibility (first-send age ≥ P and the gate). So a hole near the object's end, or one whose copy was lost with nothing sent after it, is still recovered by the sweep, which the time test alone could not do.
  - Without it, every slow-path unit is reported missing for about 50 ms and resent at once, a third baseline artefact.
  - On one path `skew_hat = 0`, so the rule is the plain RACK test, and it is applied only when `paths` is set.
- **Receiver floor hold H_f:** two arms. `HEst(strict=False, W = 10 s)`, the floor-hold estimator, non-strict across paths; and `H_force = 0`.

**Cell P2.** Cell I2 loss on both paths, rate 10 000/s, `n_total = 12147`, no outage, no link limit.

**T is chosen so that slow-path units are unprotected at birth and fast-path ones are not:**
- **T = 0.075 s.**
  - A B-unit arrives at about 0.061 s, inside T.
  - Its earliest trigger is about `srtt_B ≈ 0.07` s, and `0.07 + ô_A (0.010) ≥ 0.075`, so it is unprotected.
  - An A-unit's trigger is about 0.02 s, and `0.02 + 0.01 < 0.075`, so it is protected.
- **T = 0.4 s** as the vacuous case: everything is protected and the floor trails.
- **T = ∞** for the identity.

Arms: C3, and S5 at T ∈ {0.075, 0.4, ∞} × H ∈ {0, ∞} × H_f ∈ {est, 0}.

**Witnesses:**
- `pass_floor > 0`: the floor passed something;
- `late_floor`: late fills of floor-passed holes, the data in flight that the floor passed;
- `dup`;
- `skip_below_floor`.

### 17.8 The 2 s lifetime under overload, read where it binds (§16 item 8)

**OV-long:** OV with `n_total = 480000` (40 s).

The readout is the share delivered within 2 s of hand-over, in 0.5 s hand-over windows, read only in windows that start after the first dequeue expiry and end before the readable end (last hand-over − 2 s). Run length then does not matter. The prediction is the steady state `(C/λ)·(1 − p0)`.

Arms: C1, C3, S5 at T ∈ {0.4, 2} × H ∈ {0, ∞}, S5v at T = 0.4.

### 17.9 Stage 3d registration

**Build.**
- Base: `toy-stage3-knobs` `02eda90b`, in `tools/toy/`. Extend `toy2.py`, and add `batch3d.py`, `summarize3d.py` and `check_ident3d.py`.
- **References:** regenerate stage 1 to 3c references from `02eda90b` in a separate worktree, since the output folders are git-ignored. Use every configuration of `check_ident3c.py` plus one per 3c arm and cell, 3 seeds, all keys and traces.
- **Bit-identity:** with every new field at its default, the new code reproduces them.
- **Run order:**
  1. tests;
  2. identity and liveness (stop if red);
  3. A/A in every cell, frozen as `nf_stage3d.json`;
  4. everything else.

**New `Cfg` fields** (defaults reproduce 3c):
- `trigger_law: "cooldown"`: `"trigger"` = §17.1's d(s);
- `owd_est: "half"`: `"fwd"` = ô = srtt − RTprop/2;
- `recheck_floor: False`;
- `sweep_form: "tail"`;
- `rx_refresh: False`, set with `"blocker"`;
- `plant_T: None`;
- `paths: None`;
- `phi: 0.5`;
- `reorder_rack: False`, set with `paths`;
- `windows: False`, which computes §17.4's and §17.8's window readouts.

**Arm S5** = S4 with:
- `trigger_law = "trigger"`, `owd_est = "fwd"`, `recheck_floor = True`;
- the feedback verdict removed (`verdict_evidence = "none"`: a candidate failing L2 is skipped, never judged).

Every comparison arm in a cell shares that cell's `sweep_form`, `resend_gate`, `resend_brake`, `rtt_est`, `src_model`, `link`, `paths`, `reorder_rack` and `rx_refresh`.

**Cells** (12 seeds, phases per seed; I2 is 3 % loss at a 5 ms path, L10 is 10 % at 50 ms):
- **E-I2, E-L10, M-I2, M-L10, K-I2, K-L10, K-L10b:** as stage 3c, `sweep_form = "tail"`. The K cells also run `"blocker"`.
- **CL, OV:** as stage 3c, each under both sweep forms.
- **CL-long** (§17.4) and **OV-long** (§17.8): both sweep forms.
- **X15, X20:** as 3c (crossover, `"tail"`).
- **P2** (§17.7).

**Arms:**
- C1, C3, and S5 at T ∈ {0.4, 2, ∞} × H ∈ {0, ∞}, plus T = 1e6, in every cell except P2 (§17.7's arms) and X15/X20 (3c's grid);
- S5v in OV and OV-long (§17.6);
- plants TSTEP, TSPIKE and TKINK on S5 at T = 0.4·{0.98, …, 1.02}, H = 0, in OV and K-L10;
- the unplanted S5 nudges T = 0.4·{…} at H = 0.02, and H = 0.02·{…} at T = 0.4, in every cell except X and P2.

**Readouts** (all arms, no mean over delivered units alone):
- `share_by_0p4`, `share_by_2` and `share_T`;
- delay from hand-over p50, p90 and p99 over all units handed over;
- the window series (first sends, resends, braked, paused time, link use, blocked attempts by cause, per 1 s window);
- `recovery_time` (CL, CL-long, M cells) and the windowed by-2 s share (OV-long);
- `commit_viol` (copies staged below the floor; must be 0), `skip_below_floor`, `late_floor`, `pass_floor`, `dup` (arrivals of already-received seqs), `verdicts`;
- `finished`, `queue_drop_in` and `queue_drop_out`;
- the stall parts (§15.5).

**Tests, red first:**
1. `check_ident3d.py`.
2. S5 at T = inf equals C3, and T = 1e6 equals T = inf, in every cell type, including P2 and both sweep forms (3 seeds, every key, traces).
3. `commit_viol == 0`, with a planted positive control: with 3c's law fields (`trigger_law = "cooldown"`, `owd_est = "half"`) and the re-check disabled, OV seed 5 reproduces 3c's violation (> 0). Under the new ô those candidates fail L2, so the control must use the old fields.
4. `d(s)` returns c for a resent seq and `min(srtt_path, P_reach)` otherwise. `P_reach` comes from the candidate builder (one function, tested by calling both).
5. `owd_est = "fwd"` returns srtt/2 at zero queue, and ≈ owd + queue in OV.
6. The blocker sweep resends only `A_known`, and the receiver refresh fires at the clamped cadence.
7. The two-path model: path 0's draws equal the one-path draws; each path is FIFO; copies go on A; reorder_rack suppresses a resend of an in-flight B unit (fabricated state).
8. `plant_T`: STEP, SPIKE and KINK values at known points; deny-exempt by name only.
9. S5v takes verdicts on real state in a short OV run (verdicts > 0, floors sent at verdicts, `finished`).
10. The window readouts on a synthetic run with a known recovery time.

**Predictions (written before any run):**

- **Q3d-1, identity and liveness.**
  - Stages 1 to 3c are reproduced.
  - S5 at T = ∞ equals C3, and T = 1e6 equals T = ∞, in every cell, both sweep forms and P2.
  - Every run of every arm is `finished`. A blocker-form run that does not finish is a FAIL, not a finding.
- **Q3d-2, the commitment.**
  - `commit_viol = 0` in every run.
  - `skip_below_floor ≤ 0.001 × copies` in every one-path cell.
  - At K-L10b's lowest T, `dup ≥ 0.5 × copies` (the sweep-cap duplicates of §17.1). This confirms the separate cause; it is not a pass condition for the design.
- **Q3d-3, OV with the consistent law** (T = 0.4):
  - S5 H = 0 `share_by_0p4` ∈ [0.78, 0.84];
  - S5 H = ∞ ∈ [0.75, 0.84], at least the H = 0 value − 0.03, under **both** sweep forms (ô makes fresh units unprotected at birth whatever the trigger);
  - S5v H = ∞ ∈ [0.05, 0.25] (the tell's round trip returns: §15.2), with verdicts > 1 000 and every run `finished`;
  - S5v H = 0 within 0.02 of S5 H = 0.
- **Q3d-4, baselines and the sweep form** (OV and CL):
  - under `"blocker"`, C3's copies per delivered unit fall below its `"tail"` value, shown;
  - its `dup` share falls from about 0.5 to below 0.1;
  - the S5 rows move by less than 0.02 between forms.
- **Q3d-5, recovery time** (CL-long, both sweep forms; seconds after the outage end):
  - C3 under `"tail"` in [5.4, 7.6] (6.4 s from 3c's drain); under `"blocker"` in [4.1, 6.4] (5.1 s); C1 reported;
  - S5 at T = 2 equivalent to C3 (no expiry: the lifetime is above the backlog wait);
  - S5 at T = 0.4 in [0, 0.6] at H = 0 and [0, 1.0] at H = ∞.
  - CL (10 s): C3 "not within the run" or above 4 s; S5 at T = 0.4 ≤ 0.6 s.
- **Q3d-6, the 2 s lifetime where it binds** (OV-long, windowed by-2 s share):
  - S5 T = 2, H = 0 ∈ [0.78, 0.83], against (C/λ)·(1 − p0) = 0.808;
  - C3 and C1 ≤ 0.05 in those windows;
  - S5 T = 2, H = ∞ ∈ [0.40, 0.83], reported (strict order waits for holes still being resent, §15.2, not fixed).
- **Q3d-7, planted controls** (`share_T`, OV, values from §17.5):
  - TSTEP: K "shown" at 0.65 ± 0.15 and P "shown" at 0.33 ± 0.1;
  - TKINK: the same as TSTEP (a cliff readout);
  - TSPIKE: K equivalent, and P "shown" at −0.65 ± 0.15;
  - K-L10: all three read equivalent, reported as "no power";
  - the unplanted S5 nudges read equivalent on K and P in every cell. In K-L10 that pass has no shown power and is reported as such.
- **Q3d-8, two paths** (P2):
  - **T = 0.075, H = ∞:**
    - **H_f = 0:** `late_floor / units` ∈ [0.35, 0.50]. A fast-path datagram carrying F passes nearly every slow-path unit still in flight; those units then arrive at about 0.061 s, out of order, and still within T.
    - **H_f = est:**
      - `late_floor / units ≤ 0.03` over the whole run. The hold starts at 0, so floors pass the slow-path units of roughly the first skew window before any late sample exists, about 1.5–2.5 % of units;
      - `late_floor = 0` among units handed over after 1 s;
      - H_f ≈ the skew (45–55 ms) and `pass_floor > 0`.
    - **The dilemma itself, predicted:** with H_f = est, strict delivery waits for the slow path. The delay-from-hand-over p50 is in [0.055, 0.070] s, against [0.009, 0.020] s for the fast-path units at H_f = 0, which reorders instead.
    - **`share_T`:** a lost slow-path hole passes at about `t0 + 0.010 + H_f ≈ t0 + 0.061` s, inside T = 0.075, so `share_T` is within 0.02 between the two H_f arms. If the measured H_f exceeds 0.065 s, this bullet is expected to fail, and the cause is read from `H_f` and the pass ages.
  - **T = 0.4:** `late_floor = 0` and `pass_floor ≤ 0.002 × units` under both H_f (vacuous case, reported as such).
  - `dup / copies ≤ 0.1` for every arm under `reorder_rack`.
- **Q3d-9, continuity and the K cells:** the unplanted nudges are equivalent everywhere (above). The stall parts are as in Q3c-10, under both sweep forms in the K cells. In the K cells, S5 at T = 0.4, H = ∞ `share_by_0p4` is at least stage 3c's S4 value for the same arm minus m. There is no queue there, so ô is unchanged, and `d(s) ≤ c` protects at least as much.

Report every reading, the A/A self-check and every deviation, as stage 3c did.

### 17.10 Effect on the earlier sections, and what stays open

**Superseded or amended:**
- §15.3's `protected` gains the trigger term and ô = srtt − RTprop/2. ô also replaces srtt/2 in §2.2's L2.
- §15.1's verdict is deleted from the design and kept as the ablation's rule.
- §16's "cooldown on first resends" is not adopted (it is not the engine's gate).
- §13.2's dequeue margin R is unchanged.

**Owner questions:**
- **Q17:** sweep eligibility `max(P, R)`, so the capped sweep stops resending data whose ack cannot yet have returned. **Default: yes, as its own guarded engine change** with the sweep's 100 % cap bind. It is not part of this design.
- **Q18:** the declared assumption that queueing is on the data direction (ô). **Default: accept**, with a receiver-timestamp measurement replacing it when ADR-0078's field ships.

**Open:**
- delay pinned near T − R under overload (Q12);
- strict order at T = 2 under overload;
- the strict-order stall through the outage;
- the lost tail with no repair under an engine-like sweep (§17.3, an engine repair-plane question);
- the toy still has no proactive repair.

## 18. Findings from the offline simulation, stage 3d (2026-11-24, reviewed with the advisor)

Branch `toy-stage3-knobs` `e17b4c64`, `tools/toy/results-stage3d.md`.

**One path: established in simulation.**
- T = ∞ equals full delivery in all 21 cells, both sweep forms and P2 (432/432; traces identical).
- The floor's commitment held in all 5 628 runs (stage 3c: 143 violations). The re-check before each copy never fired; the trigger-delay law did the work (a control on 3c's law reproduces 7 violations).
- Recovery after the outage, CL-long (20 s, 1.5 s outage, offered 10 000/s, link 12 500/s), seconds after outage end, tail | engine-like sweep: full delivery, strict 7.79 | 6.73; ADR-0084's sender with today's receiver 5.68 | 4.80; never give up, any order 6.66 | 5.67; lifetime 2 s strict 7.78 | 6.59; lifetime 0.4 s, either order, within the readout's 0.1 s resolution. The never-give-up recovery follows outage × rate ÷ headroom (1.5 × 10 000 ÷ 2 500 ≈ 6 s). Any order alone saves about 1.1 s; a 2 s lifetime saves nothing here (the backlog wait is below 2 s per unit); 0.4 s saves nearly all of it. Cost: delivered at all 92.4 % against 100 %; within 0.4 s 92.4 % against 55.4 % (full delivery) and 64.2 % (ADR-0084's sender).
- Overload: the floor law is what fixes strict order (82.8 % within 0.4 s with it, 6.8 % in the ablation without it). The 2 s lifetime in OV-long reads 0.805 in the binding windows against 0.808 predicted. Never-give-up reads 0 there by construction (a fixed-rate source above capacity).
- Under overload delivery sits at about T − 10 ms (p50 390 ms at T = 0.4 s), so on-time delivery rests on the one-way estimate being right to about 10 ms, and on Q18. The planted step is a 2× change in the effective lifetime, so it does not measure that sensitivity.
- The never-give-up rows move by up to 13 points between sweep forms; the 0.4 s rows by under 1 point. Baselines must be quoted under both forms.

**Two paths (P2): an open design problem.** At T = 75 ms with strict order: with the measured floor hold, 2.0 % is on time (delay p50 121 ms); with no floor hold, 91.5 % is on time but 6.5 % of units are filled late after the floor passed them. So strict order and on-time are traded, not both met. The 6.5 % is probably masked: frequent fast-path losses keep the floor about 65 ms behind, so it rarely overtakes slow-path data; the registered 35–50 % is not refuted. Needed before any claim: a cell at the call rate or with a loss-free fast path. At T = 0.4 s it matches never giving up, except about 105 late fills in the first 0.1 s from the 333 ms RTT seed (new cold-start defect). The reorder window suppressed every resend at 75 ms (3 % lost) and makes about 30 % of the baseline's copies duplicates, so the P2 baseline is qualified.

**Spec errors and misses.**
- Liveness failed for the ablation S5v in OV-long (8 runs): a hole sent in the same instant as the newest arrival, below the highest received seq, has no timeout. The gap is in the tail timeout Revision 3's review added. The worker did not stop as the run order required; the fix is an unregistered supplement (shares unchanged). The same pattern may exist in the engine's verdict path.
- Smoothness: the planted control has power only in OV; the A/A itself is 21 % undetermined, so the 24 undetermined nudges are neither passes nor failures. Read OV's own nudge rows before claiming continuity.
- The CL-long baseline bands missed by 0.2–0.3 s (the drain surplus is about 15 % lower than 3c's estimate).

**Next, registered first:** a two-path cell at the call rate and one with a loss-free fast path; the structural question whether units behind a given-up gap are released at about lifetime + floor hold; the cold-start RTT seed; the S5v timeout gap; and repair (still untested).
