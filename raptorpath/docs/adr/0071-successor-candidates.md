# ADR-0071: The two conceptual successors to the cap law, written FORMULA-FIRST as CANDIDATES — the slack magnitude (what replaces `17/8`-as-permanent) and the δ-priced queue bound (what replaces `knee`/`N·2048` and `WIN_STORE_MAX`-as-law)

## Status: **PROPOSED — NO DECISION TAKEN, AND NONE IS SOUGHT HERE.**

This document enumerates, derives and prices candidates. It picks no winner
and ships nothing: no engine file, no gate, no default, no test, no paper
claim beyond the pointer at §16.61. The adjudication is the user's, and a
"recommendation" section is deliberately absent — ADR-0068's `Proposed`
precedent is the shape copied.

**Date**: 2026-08-18
**Branch**: `docs/successor-candidates` from main@`1d83547`. **DOCS ONLY.** No
VM was contacted, no L1 number re-derived, no benchmark run. Every number in
every predictions table below is **arithmetic on already-published means**,
and the arithmetic is shown.

> **AMENDMENT 2026-08-19 (`docs/tier0-corrections` — literature cross-check
> Tier 0, `docs/research/literature-crosscheck.md`, paper §16.65).** Three
> record corrections land on claims this ADR inherits; no candidate, verdict
> or ranking changes.
>
> 1. **The span term every candidate below carries (`2·rate_fast·skew`) is
>    OURS — a NOVELTY CLAIM, recorded where the term is proposed** (cross-check
>    item 1(c) honesty note; Tier 0.6). No publication writes a separable
>    resequencing term of the shape `Σ bwᵢ·(RTT_max − RTTᵢ)` beside a window
>    term — checked against RFC 6182 §5.3, RFC 8684 §3.3.4, Barré 2011,
>    Raiciu NSDI'12 and DAPS (Kuhn ICC 2014). The published multipath sizings
>    are AGGREGATES with `RTT_max` outside the sum (`2·Σ bwᵢ·RTT_max`); our
>    decomposition is one step of algebra from them and **half** their
>    magnitude at N = 2. The paper may cite that literature for the term's
>    *magnitude*, never for its *shape*: the decomposition must always be
>    presented as our derivation.
> 2. **The `17/8` at the centre of family 1 inherits a TUNED constant**
>    (cross-check item 6(d)): RFC 9002 §6.1.2 recommends `kTimeThreshold = 9/8`
>    empirically — *"Experience with QUIC shows that 9/8 works well"* — and
>    RACK (RFC 8985) uses 5/4 for the same purpose. Earlier "cited, not
>    fitted" descriptions overstate the source (candidate (d)'s deletion note
>    below is unaffected — it removes the constant rather than re-deriving it).
> 3. **The displaced predecessor's `gain = 2.0` is the right value with two
>    published derivations and a wrong local rationale** (cross-check item 3,
>    folklore item 1): RFC 6182 §5.3's `×2` and BBR's `cwnd_gain = 2`
>    (ACK-aggregation absorption / minimum per-round rate-doubling gain) — the
>    "recovery runway" prose appears in no primary BBR source. Recorded here
>    because ADR-0070's FOSSIL verdict is cited throughout this document.

---

## Why these two, and why now

§16.57 measured the composed cap law on the wire and reached a verdict that
neither of its predecessors could have produced: **the SHAPE is right and the
MAGNITUDE is wrong.** The law is linear in the path count, carries no mode
bit, no δ/ρ threshold and no topology predicate, its span term vanishes by
arithmetic at N = 1 in all 340 single-path evaluations — and it asks for
`3.125 · Σ(rate·K·RTprop) + span`, which exceeds the 4096 memory bound at
every dual cell and, where it is interior, buys **2.4× the standing queue for
zero goodput at parity** and **1.43–1.48× worse delivered latency** (sc2, both
seeds, far outside 2σ).

§16.59 then eliminated the one available explanation that was not the
magnitude itself: the queue-free stall clock is **REFUTED** — it sheds 1.7 %
of a 90 % overshoot at c8, because at c8 `K` = 1.04 and there was nothing in
the clock to remove. *The magnitude owns the overshoot, not the argument.*

That leaves exactly two questions, and they are the two families below.

**One formula-level fact governs both, and it has not been stated in these
terms anywhere in the tree.** At the shipped scope ρ = 1 (`contract_rho = 1.0`,
`sender_policy.rs:767` — plain dyn cap ⇒ reliable ⇒ retain-until-acked), the
stall term is

```text
stall(δ, ρ=1, srtt) = (1 − 1)·D(δ) + 1·(9/8·srtt + srtt) = 17/8·srtt
```

— **δ is multiplied by zero.** The shipped composed law contains no δ at all
at the scope it ships in. §16.56's design sentence, *"`cap − BDP` IS the
standing queue, δ prices queue as a latency budget"*, is therefore not merely
unmet on the wire (§16.57's finding); it is **unmeetable by construction**,
and §16.57's observation that "δ priced nothing" at sc2 has this as its
mechanism. Family 2 exists because of this line. Family 1 exists because
`17/8·srtt` is what stands in δ's place.

**A second arithmetic fact, from the published numbers, sharpens it.** The
largest delay allowance the δ dial can express is `D(δ) = min(b(δ)·RTprop,
2·RTprop)` at `b(Bulk) = 2` (`net::delta_budget_b`, `net::shed_deadline_us`),
i.e. **2·RTprop**. The shipped slack is `17/8·srtt = 2.125·K·RTprop`, and `K`
is floored at 1.0, so

```text
shipped slack ≥ D(δ)|Bulk   ⟺   2.125·K ≥ 2   ⟺   K ≥ 0.941   — ALWAYS TRUE
```

At the session's own measured `K` (1.04 … 1.505) the shipped slack runs
**2.21 … 3.20 RTprop** against a Bulk allowance of 2. **The composed law
charges more than the Bulk delay allowance at every point of the δ dial,
including Realtime, where the allowance is ½·RTprop and the law charges 4.4×
it.** That single line is the strongest available statement of the indictment
and it needs no new measurement.

---

## The measured inputs every candidate is scored on

All five cells, from the **833 `[3T]` evaluations** of the composed battery
(goal-gate "Composed-Cap Battery — RESULTS", `[3T]` decomposition; `K` from
the same session; identities re-checked below):

| cell | `W` = Σ rate·K·RTprop | `S` = span | `K` | `BDP = W/K` | shipped composed `3.125·W + S` | published | arm A realized |
|---|---|---|---|---|---|---|---|
| c1 | 201 | 0 | 1.15 | 174.8 | 628.1 | 629 | 541 (legacy `2·BDP`, interior) |
| sc2 | 374 | 0 | 1.14 | 328.1 | 1168.8 | 1 168 | 1024 (`RELIABLE_STORE_MAX` latch) |
| c7 | 1 261 | 118 | 1.14 | 1 106.1 | 4059.1 | 4 059 | 4096 (pin) |
| c8 | 1 669 | 2 563 | 1.04 | 1 604.8 | 7778.6 | 7 778 | 4096 (pin) |
| c8L | 7 489 | 2 552 | 1.505 | 4 976.1 | 25 955.6 | 25 956 | 4096 (pin) |

The `3.125·W + S` column reproduces the published "mean unclamped" to the last
digit at all five cells, which is the check that the decomposition below is
the law and not a paraphrase of it.

**Rate, for the two candidates that need one.** Symbol ≈ 1.2 KB (the memory
bound's own arithmetic, `net/mod.rs:3442-3445`), so
`rate ≈ goodput_Mbit · 10⁶ / 9600` sym/s: **c1 22 708, c7 18 104, c8 8 302,
c8L 7 479, sc2 9 146.** Stated as an assumption, not a measurement.

**The cap→delay conversion, and it is measured, not assumed.** By Little's law
delivered residence is `occupancy / rate`. At sc2 the composed battery
measured arm A cap 1024 / `occ_p50` 1012 / `q_p50` 91 ms and arm C cap 2291 /
`occ` 2214 / `q_p50` 218 ms: `Δocc / rate = 1202 / 9146 = 131 ms` against a
measured `Δq` of **127 ms** — Little's law closing to 3 %. The independent
ICMP probe reads `1024/9146 = 112 ms` predicted against **97.7 ms** measured
(§16.47, arm A, seed 42) and, at §16.47's arm-B cap of ≈ 471, **44.6 ms**
against 51 ms predicted. **The cap is convertible to delivered latency to
within ≈ 20 % at sc2 across a 4.9× range of cap, in two independent sessions
and two eras.** That conversion is what makes every predictions table below a
falsifiable latency claim rather than a symbol count.

> **One caveat recorded against my own conversion.** Between arm A and arm C
> of the composed battery the engine-side `q_p50` moved with the FULL Little
> slope (0.106 ms/symbol ≈ 1/rate) while the independent probe moved at only
> **one third** of it (0.036 ms/symbol). In §16.47 the probe moved at the full
> slope. The likely mechanism is that above the bottleneck's own buffer the
> extra backlog is held in the sender's store rather than on the wire, so the
> probe stops seeing it while the DATA's delivered latency still pays — but
> that is a hypothesis, and it is a named prerequisite for family 2's
> falsifier (below), not a settled fact.

**Two findings about these inputs, both new, both consequential, both from
arithmetic alone:**

1. **c8L cannot fund one BDP.** `W(c8L) = 7 489` symbols is **1.83×
   `WIN_STORE_MAX` = 4096 before any slack, any span and any δ**. Term 1
   alone — the network window, the one term nobody disputes — exceeds the
   memory bound. **No cap law of any magnitude can be interior at c8L**, and
   c8L must be pre-declared MEMORY-STARVED and UNSCORABLE for cap-law purposes
   rather than reported as a law that pinned. §16.57 reports c8L's `mem` bind
   at 1.000 and its overshoot at 6.3× as a property of the law; at least 1.83×
   of it is a property of the *resource limit*.
2. **c8 and c8L are the same geometry and their term 1 differs by 4.5×.**
   c8 = 25 MB / 2.54 s transfer, c8L = 200 MB / 20.5 s, identical netem, and
   `W` reads 1 669 against 7 489 while goodput reads 79.7 against 71.8 Mbit/s.
   Delivered rate is flat; the LAW's own rate×RTprop input is not. Whatever
   the mechanism (a `max_bw` windowed max still warming at 2.5 s, an inflating
   `min_rtt`, or both), **c8 measures the cap law's inputs during estimator
   warm-up**, and every cap verdict ever taken at c8 inherits it. The test is
   cheap and needs no VM arm: read the `[3T]` `window=` time series *within*
   one c8L run at t ≈ 2.5 s and at t ≈ 20 s. If it reproduces the 4.5×, c8 is
   a warm-up cell and should be labelled one.

---

# CANDIDATE FAMILY 1 — the slack magnitude

**The indictment, restated as a formula.** The slack term
`rate · 17/8 · srtt` provisions the WORST-CASE recovery backlog —
RFC 9002 detection plus one full retransmit round trip — **permanently, at
every instant, whether or not anything is stalled**. It is a standing
reservation for a transient event. At a saturated ρ = 1 cell its measured
payout was **zero** (sc2: 2.24× the cap, 2.19× the outstanding, goodput 0.993
/ 1.003 — parity within 2σ) and its measured premium was **constant queue**
(2.4×, 218 ms against 91 ms, and 43–48 % worse delivered latency).

All four candidates below change **only** the second argument of
`net::three_term_store_cap`'s `slack` accumulator (`contract_stall_s`). Term 1
(`rate·K·RTprop`) and term 3 (the span) are untouched, and the shape
properties §16.57 confirmed on the wire — linear in N, span ≡ 0 at N = 1, no
mode bit, no topology predicate — are preserved by construction in every one.

---

## (a) TRANSIENT slack — standing slack zero, a reserve drawn only while a stall is DETECTED

```text
cap = Σᵢ [ rateᵢ·Kᵢ·RTpropᵢ ]  +  ARMED · Σᵢ [ rateᵢ · stall(δ, ρ, srttᵢ) ]  +  2·rate_fast·skew

  ARMED  =  tx_paused  ∧  (retransmit_buffer is non-empty)          ← the arming law
  release:  ARMED falls on the cumulative-ack advance that retires the blocking hole
```

**and its continuous form (a′), which is the one that survives CLAUDE.md:**

```text
cap = Σᵢ [ rateᵢ·Kᵢ·RTpropᵢ ]  +  p_lost(age_oldest, ε̂, srtt, rttvar) · Σᵢ [ rateᵢ · stall(δ, ρ, srttᵢ) ]  +  2·rate_fast·skew
```

### Provenance

| symbol | provenance |
|---|---|
| `tx_paused` | **measured, already computed** — `net/mod.rs:5452`, `outstanding ≥ store_cap`; the store-cap backpressure edge, DIAG field `diag.rs:321`, wakes `wait_arm = 1` |
| `retransmit_buffer` non-empty | **measured, already computed** — `emit_source.rs:118`; the tail sweep already reads exactly this predicate (`mod.rs:6102-6106`) |
| the release edge | **measured, already computed** — cumulative-ack advance, the same event the frontier-stall attribution is charged on (`mod.rs:7037-7049`) |
| `p_lost(...)` (form a′) | **measured, already computed** — `control::fec_rate::p_lost`, called every emission at `emit_source.rs:818`; a scalar ∈ [0,1] on the oldest un-acked symbol's age, already load-bearing for the ARQ/FEC branch |
| `stall(δ, ρ, srtt)` | **unchanged** — `contract_stall_s`, `net/mod.rs:3001-3009` |

**Zero new constants in either form.** `ARMED` is a conjunction of two
existing booleans; `p_lost` is an existing scalar. Nothing is swept, nothing
is fitted, and the arming condition is *literally the dead-wall condition* —
the store is full **and** a hole is outstanding — which is the event the slack
was provisioned for.

### Reduction check

Form (a) reduces to the shipped composed law **exactly, while ARMED**, and to
candidate (d) while not. Form (a′) reduces to the shipped law as
`p_lost → 1` (the oldest un-acked symbol is certainly lost) and to (d) as
`p_lost → 0` (nothing at risk), **continuously, with both terms always
computed** — the shipped rate law's own shape.

### Predictions at the five cells

`ARMED = 0` is the standing state; the armed state is bit-identical to the
shipped composed law.

| cell | standing `W + S` | armed (= shipped) | vs `WIN_STORE_MAX` standing |
|---|---|---|---|
| c1 | **201** | 629 | interior |
| sc2 | **374** | 1 168 | interior |
| c7 | **1 379** | 4 059 | interior |
| c8 | **4 232** | 7 778 | **STILL PINS — by 3.3 %** |
| c8L | **10 041** | 25 956 | **STILL PINS — 2.45×** |

**The finding that matters here is the c8 row and it is against the whole
family.** Deleting the slack ENTIRELY at c8 sheds 45.6 % of the ask, and
§16.59 measured that c8 must shed **47 %** to clear the memory bound. *Zero
slack is not enough at c8.* The residue is the SPAN term: at c8 `S` = 2 563 >
`W` = 1 669. The slack is not the c8 binder; the span is, jointly with the
memory bound. Any successor scored on "does c8 go interior" will fail for a
reason that has nothing to do with family 1.

### Falsification plan

1. **The lead-time falsifier (cheapest, no VM, kills (a) outright).** A
   reserve that arms *after* the wire has already gone idle funds nothing.
   Measure `T_arm − T_prod` where `T_prod` is `[WALL]`'s own productive-suffix
   boundary (`walldiag.rs:199-221`), median over reps at c7 and c8. **If the
   median is ≥ 0 — the reserve arrives at or after the idle it was to
   prevent — (a) is dead, and there is no coefficient to tune.** The
   instrument exists and reported on 199/199 live reps.
2. **The step falsifier (a) must answer and (a′) is built to dodge.** (a) puts
   a STEP in the cap — in time, not across a dial, so it is not the
   §16.20/ADR-0064 mode switch on its face. But a cap that jumps from `W + S`
   to `3.125·W + S` on an edge is a burst-admission event, and the shipped
   `boot = 128` argument (`sender_policy.rs:573-577`) says exactly why that is
   dangerous: a burst pre-bloats the queue, inflates the `min_rtt` floor, and
   therefore inflates the anchor that sizes the cap. **Falsifier: the
   `min_rtt` of each live path must not fall in the 2 RTprop following an arm
   edge**, measured in-run. This question is the entire reason (a′) exists,
   and I do not claim it is settled by writing (a′) down.
3. **The payout falsifier.** Wire-idle at the standing backlog, at every cell,
   on `slack_bench.rs`'s existing idle-vs-backlog replay (§16.43, 576 cells in
   13 s, no VM). If idle at `S_standing = W + S` exceeds the pre-registered
   coverage point at any cell, the standing reservation was buying something.

---

## (b) δ-PRICED slack — slack can never buy more queue than the contract permits

```text
cap = Σᵢ [ rateᵢ·Kᵢ·RTpropᵢ  +  rateᵢ · min( stall(δ, ρ, srttᵢ),  D(δ, RTpropᵢ) ) ]  +  2·rate_fast·skew

  D(δ, RTprop) = min( b(δ)·RTprop,  2·RTprop )        ← net::shed_deadline_us, UNCHANGED
```

### Provenance

| symbol | provenance |
|---|---|
| `D(δ, RTprop)` | **shipped code, reused not restated** — `net::shed_deadline_us`, `net/mod.rs:762-768`; already the span law's own deadline and already the `(1−ρ)` half of `contract_stall_s` |
| `b(δ)` | **declared DIAL** — `net::delta_budget_b`, `mod.rs:2946-2952`: Realtime ½, Auto 1, Bulk 2 **round trips of RTprop**; pinned as a dial by `delta_budget_b_is_the_dial_not_a_mode` |
| `min(·,·)` | not a branch — a continuous, monotone, non-expansive function of both arguments; no threshold selects a code path |
| everything else | unchanged from §16.56 |

**Zero new constants.** Note what this does structurally: it puts `D(δ)` back
into the law **at ρ = 1**, where the shipped form multiplies it by `(1−ρ) = 0`.
That is the whole point of the candidate.

### Reduction check — and this is where the candidate is ugly, stated plainly

`min(2.125·K·RTprop, b·RTprop)` with `b ≤ 2` (D's own inner `min`) and
`K ≥ 1.0` (the floor) means **the min is `b·RTprop` ALWAYS, at every measured
cell, at every dial point**. So:

- the candidate **never** reduces to the shipped composed law, at any δ;
- **at δ → ∞ (Bulk) it reduces to `cap = Σ rate·RTprop·(K + 2) + span`**, i.e.
  ≈ 3·W at K ≈ 1 — which lands 4 % below the shipped 3.125·W by coincidence
  of arithmetic, not by derivation. **The shipped `17/8` is, to within 4 %,
  the BULK corner of this candidate applied at every point of the dial.**
- the `stall` argument of the `min` is therefore **inert** at every cell in
  the record. A term that never binds is exactly ADR-0070 mechanism 1 in the
  other direction, and honesty requires saying so: as written, (b) is not
  "slack capped by δ", it **is** the δ ceiling of family 2 with a dead
  argument attached. If one wanted the `stall` argument to be live, one would
  have to remove D's inner `2·RTprop` cap — and that cap is what makes the
  bound a bound. **This candidate and family 2 are one formula written
  twice**, and the composition section below is where that is settled rather
  than hidden.

### Predictions at the five cells

With the `min` resolving to `b·RTprop`, `cap = W·(1 + b/K) + S`:

| cell | b = ½ (Realtime) | b = 1 (Auto) | b = 2 (Bulk) | shipped composed | arm A |
|---|---|---|---|---|---|
| c1 | **288** | **376** | **551** | 629 | 541 |
| sc2 | **538** | **702** | **1 030** | 1 168 | 1024 |
| c7 | **1 932** | **2 485** | **3 591** | 4 059 | 4096 |
| c8 | **5 035** | **5 837** | **7 442** | 7 778 | 4096 |
| c8L | **12 528** | **15 018** | **19 993** | 25 956 | 4096 |

Interior at c1, sc2, c7 at every dial point; **still pinned at c8 and c8L at
every dial point**, for the span/memory reason named under (a). At Bulk, sc2
lands at 1 030 against the shipped latch of 1 024 — a 0.6 % agreement that is
a coincidence worth noting and not evidence.

### Falsification plan

1. **The conversion falsifier (the load-bearing one).** By the measured
   cap→delay conversion, this law promises delivered residence
   `RTprop·(K + b)` per path. Pre-register that band per cell per dial point
   and score the **independent ICMP probe** (§16.47's instrument, not the
   engine's `rtt=`). **If the probe median at Realtime exceeds `RTprop·(K+½)`
   by more than 2σ at any cell where the cap is provably interior and the
   brake provably engaged, δ is not pricing the queue and the candidate is
   refuted** — the same clause §16.57's own falsifier failed to key correctly
   (it keyed to goodput; that defect is on the record and is not repeated
   here).
2. **The dial-continuity gate, and it needs no VM.** A property test over
   `b ∈ [0.4, 2.2]` in fine steps asserting the cap is continuous and monotone
   in `b`, plus ±2 % nudges through each of the three named points — the
   `test_visualizer.mjs` pattern applied to the engine law. A step at a named
   point is a defect **even if each side is individually correct**.
3. **The goodput falsifier.** At the three cells with permitted headroom (c1
   75.9 %, c8 18.7 %, c8L 21.8 %; c7 3.1 % and sc2 1.6 % carry NO throughput
   target — discipline 16), a > 2σ goodput regression at Realtime means the
   contract's own allowance is below what the wire needs, which refutes
   δ-as-queue-budget rather than the arithmetic.

---

## (c) UTILIZATION-ARGUED slack — provision what measured idleness justifies

```text
cap = Σᵢ [ rateᵢ·Kᵢ·RTpropᵢ ]  +  ( Σᵢ rateᵢ ) · T_idle_measured  +  2·rate_fast·skew

  T_idle_measured = the recovery-idle time the sender directly observes,
                    per recovery episode, from the [WALL] / wait instruments
```

### Provenance

| symbol | provenance |
|---|---|
| `T_idle_measured` | **measured** — `[WALL]`'s `duration_ms`, `walldiag.rs:99-221`: the terminal window in which the loop woke on neither the TUN arm nor the PAUSED arm and `last_source_send_us` did not advance. Resolution `it_ms` = 0.04–0.15 ms, three to four orders below the walls it measures |
| the alternative reading | `wait_us[1]` (`paused`) / `wait_n` — the tick-share of sender wall time woken by store-full backpressure, `diag.rs:120-151`; §16.47 measured it moving 40 % → 7 % and 65 % → 5 % exactly where the law raises the cap |
| Little's law | the same law terms 1 and 2 already are |

**Zero new constants — and one unresolved DEFINITION**, which is the honest
weakness. `duration_ms` is the duration of the *terminal* dead window, not an
idle time *per recovery episode*; converting it to a backlog requires an
episode count that `[WALL]` does not report. The predictions below use
`duration_ms` directly and are therefore an **upper reading**, labelled as
such.

### Reduction check

Reduces to (d) when the measured idle is zero, and to a value near the shipped
slack when the measured idle equals the derived stall — which is the
self-consistency check §16.43's slack bench was built to run.

### Predictions at the five cells

Using arm-A `[WALL] dur_ms` medians (626.2 c8, 219.2 c8L, 144.7 c7, 20.2 sc2,
1.5 c1) and the rates above:

| cell | `rate · dur_ms` | `cap = W + that + S` | shipped composed | comment |
|---|---|---|---|---|
| c1 | 34 | **235** | 629 | interior |
| sc2 | 185 | **559** | 1 168 | interior; lands near §16.47's winning cap (≈ 471) |
| c7 | 2 620 | **3 999** | 4 059 | reproduces the shipped ask to 1.5 % |
| c8 | 5 199 | **9 431** | 7 778 | **WORSE than shipped** |
| c8L | 1 639 | **11 680** | 25 956 | better, still pins |

The c8 row is the candidate's own indictment: at the one cell with a large
measured wall, a feedback law that funds the measured idleness asks for **21 %
more** than the law already refuted for asking too much.

### The loop-stability question, stated

This is the only candidate that closes a **feedback loop from an outcome back
into the law**: `slack → cap → less idle → less slack → more idle`. It has no
stable interior fixed point by construction — if the provisioning works, the
measurand it is provisioned against goes to zero, which withdraws the
provisioning. The expected behaviour is a **limit cycle** whose period is
≈ 2× the measurement window, not convergence. This is the same species of
circularity the tree has already refuted twice: the `2×anchor` Copa-sole cap
(`net/mod.rs:4571-4583`, *"samples can never read above the store-capped
delivered rate"*, L0-measured stuck at 3.2k of 10.4k sym/s), and the
`cap → wireQ → srtt → K → slack → cap` loop §16.59 measured at 1.505 on c8L.
A candidate that closes a third one owes the argument that this one is
different, and **I do not have that argument.**

### Falsification plan

1. **The stability falsifier, and it fires FIRST.** Pre-register a maximum
   within-rep oscillation ratio on the realized cap (`p95/p05` from `[CCAP]`),
   and a requirement that the time-averaged idle be no worse than the constant
   slack arm's. Cheap at the SF bench, no VM.
2. **The BLOCKING prerequisite, which is not a falsifier but a gate.** `[WALL]`
   **failed its own stability trial** at c8: `sign(median dur_ms(C) − median
   dur_ms(A))` read −1 / +1 / −1 across three pools collected minutes apart on
   one binary (S-WALL, INVERTED), which is the exact event that voided §16.54,
   reproduced on the measurand built to replace the one that did it. **A law
   whose INPUT is that statistic cannot be scored at c8 today.** Per the
   battery's own recommendation 5, the fix is a DESIGN change (a paired
   within-rep contrast, or a cell whose statistic is not bistable), not a
   third measurand — and it is a prerequisite for (c) specifically.

---

## (d) ZERO — the null candidate

```text
cap = Σᵢ [ rateᵢ·Kᵢ·RTpropᵢ  +  rateᵢ · (1 − ρ)·D(δ, RTpropᵢ) ]  +  2·rate_fast·skew

  i.e.  stall(δ, ρ) = (1 − ρ)·D(δ)  —  the ρ·(9/8·srtt + srtt) term DELETED
```

### Provenance

Every symbol is already provenanced in §16.56; this candidate **removes** a
term and adds nothing. It deletes the only place `9/8` appears — which also
discharges §16.59's `9/8` provenance regression by removing its subject rather
than re-citing it.

### Reduction check

**Exact at ρ → 0**: identical to the shipped composed law, since there the
shipped `ρ·(…)` term is itself zero. Maximally divergent at ρ = 1, which is
the shipped scope. It remains continuous in ρ and in δ, with both terms always
computed — the invariant is satisfied by construction, and the candidate is
*more* δ-live than the shipped law, which has no δ at ρ = 1 at all.

### Predictions at the five cells

At ρ = 1 the remaining stall is zero, so `cap = W + S`: **c1 201, sc2 374,
c7 1 379, c8 4 232, c8L 10 041** — the "standing" column of (a).

### The against-the-case, stated

The measured payout at sc2 was zero, but the argument that it *should* be zero
is not free: **a retransmit occupies a store slot too.** If the store is
exactly `W + S` and every slot is held by an un-acked symbol, the retransmit
that would clear the blocking hole has nowhere to go — the very starvation the
slack was invented to prevent, arriving through the front door. The counter is
that a retransmit re-sends an ALREADY-STORED symbol and needs no new slot; I
believe that is true of this engine's retransmit path but **I did not verify
it in the code, and it is the first thing to check before (d) is taken
seriously.**

### Falsification plan — the cheapest in the document

1. **No VM, no wire.** `slack_bench.rs` already replays each cell's measured
   store residence against a backlog `S` and reports wire-idle against `S`
   (§16.43/§16.44, 576 cells in 13 s). Read the idle at `S = W + S_span` at
   every cell. **If idle exceeds the pre-registered coverage point anywhere,
   the payout was not zero and (d) is refuted** — on a component bench, in
   seconds, with no session, no seed and no abort class.
2. **On the wire, if it survives the bench.** A > 2σ goodput regression at any
   cell with permitted headroom (c1, c8, c8L). Note the LADDER battery may
   answer this without a dedicated arm — see the last section.

---

# CANDIDATE FAMILY 2 — the δ-priced queue bound

## The formula

```text
cap  =  min( demand,  ceiling )

ceiling = Σᵢ over live_paths [ rateᵢ · ( baselineᵢ  +  δ_headroomᵢ ) ]

  δ_headroomᵢ = D(δ, RTpropᵢ) = min( b(δ)·RTpropᵢ, 2·RTpropᵢ )     ← net::shed_deadline_us
  baselineᵢ   = RTpropᵢ            (reading ii — the queue-free clock)
              | Kᵢ·RTpropᵢ         (reading i  — the ack-round-trip clock)
  demand      = the three-term law, with whichever family-1 slack the user picks

  NO knee. NO N·2048. NO swept pool.
  WIN_STORE_MAX survives OUTSIDE the law as a resource limit that may ABORT,
  never as a term that shapes — and its bind fraction is reported.
```

## What `δ_headroom` IS at each named hint — and why this is not a mode switch

`δ_headroom` is a **time**, expressed in round trips of the path's own RTprop,
and it is read from the shipped dial:

| hint | `b(δ)` | `δ_headroom` | the promise, in words |
|---|---|---|---|
| Realtime | ½ | ½·RTprop | *"you may queue half a round trip"* |
| Auto | 1 | 1·RTprop | *"you may queue one round trip"* |
| Bulk | 2 | 2·RTprop | *"you may queue two round trips"* |

The hints are **named points**, `net::delta_budget_b` is a lookup of a NUMBER
on the dial (`delta_b: f64`, `sender_policy.rs:245`, doc: *"a NUMBER on a
dial… Not a mode selector"*), and the law reads only that number. There is no
`if hint ==`, no threshold on δ, and the cap is continuous and strictly
monotone in `b` on the whole interval — the ceiling is affine in `b` up to D's
own `min` at 2, where it is continuous with a slope change (a corner, not a
step). **A corner in the derivative at the Bulk endpoint is worth flagging to
the user explicitly**: it is not a behaviour step and it does not violate the
invariant as written, but it is the only non-smooth point in the whole family
and the user has rejected three near-misses in this area.

**The one substantive provenance question, and I am not deciding it.** `D(δ)`
is the *shed deadline* — the age past which a retransmit is not worth sending.
Reusing it as the *queue budget* is the ZERO-CONSTANT choice and it is the
reason to prefer it. But they are two jobs: one is a retransmit-worthiness
horizon, the other a standing-queue allowance. The alternative is a second
dial point for queue, which is a new constant and would have to be derived.
**Reuse is not free and I am recording the conflation rather than assuming
past it.**

## Predictions at the five cells

`ceiling = W·(1 + b/K)` under reading (i); `ceiling = W·(1 + b)/K` under
reading (ii). Span is **inside** the budget in both (see the composition
below), so it is not added on top:

| cell | (ii) b=½ | (ii) b=1 | (ii) b=2 | (i) b=1 | current `N·knee` / latch | shipped composed |
|---|---|---|---|---|---|---|
| c1 | **262** | **350** | **524** | 376 | 1024 (`RELIABLE_STORE_MAX`, N=1) | 629 |
| sc2 | **492** | **656** | **984** | 702 | 1024 | 1 168 |
| c7 | **1 659** | **2 212** | **3 318** | 2 485 | **4096** | 4 059 |
| c8 | **2 407** | **3 210** | **4 815** | 3 274 | **4096** | 7 778 |
| c8L | **7 464** | **9 952** | **14 928** | 12 466 | **4096** | 25 956 |

**Read this table for its shape, not its digits.** At c7 and c8 the δ-priced
ceiling lands at **0.54× and 0.78× the shipped 4096** at Auto — for the first
time the operating point would be a *law* at a dual cell, not the clamp.
At c8L it does not, and cannot: `W/K` = 4 976 alone exceeds the memory bound
(finding 1 above). At c1 and sc2 it is well inside the legacy latch.

**Reading (ii) additionally closes the `K` loop that §16.59 could not.** Under
(ii) the ack-path overhead `(K−1)·RTprop` is itself charged against the δ
budget rather than granted free — which is correct if `K = 1 + wireQ/RTprop`
(§16.59's own algebra), because that overhead IS delay. Under (i) it is
granted, on §16.56's amendment that term 1 must fund one *ack* round trip.
**The two readings differ by exactly `(K−1)/(1+b)` and that difference is 4 %
at c8 and 50 % at c8L.** Choosing between them is a derivation question about
what the contract's baseline is, and it is a question for the user.

## The memory bound's remaining role

`WIN_STORE_MAX` = 4096 stops being a term and becomes what §16.56 already
called it: a **resource limit stated outside the law**, 4096 × ~1.2 KB ≈ 5 MB,
which may ABORT or refuse but never shapes. Three consequences that must be
written into any battery that scores this:

1. Its bind fraction is REPORTED (`[CCAP] mem=`), per the FORMULA-FIRST clamp
   rule, and **a non-zero bind is a STOP, not a datum** — §16.56 wrote that
   condition and §16.57 measured it firing.
2. **c8L is pre-declared UNSCORABLE** on the arithmetic above: term 1 alone is
   1.83× the bound, so the bound binds under every candidate at every dial
   point, and reporting c8L as "the law pinned" would repeat exactly the
   discipline-18 error this arc exists to prevent.
3. If the δ-priced ceiling is right, the memory bound's *value* becomes a
   capacity-planning question (how much delay may a 5 MB budget fund at a
   given rate) rather than a law question — and the answer at c8L is *less
   than one BDP*, which is a statement about the product, not the formula.

## The composition with family 1 — the double-charge, shown rather than asserted

δ appears in the demand (through any family-1 candidate that uses `D(δ)`) and
in the ceiling. **They do not compose additively and they must not.** Written
out per path:

```text
demand_i  = rate_i·(baseline_i + stall_i(δ, ρ))
ceiling_i = rate_i·(baseline_i + D(δ, RTprop_i))

cap_i = min(demand_i, ceiling_i)
      = rate_i·( baseline_i + min( stall_i(δ,ρ), D(δ,RTprop_i) ) )
```

The `min` distributes over the shared `baseline`, so **δ is charged exactly
once**: the ceiling can only ever REMOVE time from the demand, never add it.
If the demand's own δ term already respects the budget the ceiling is inert
and the composition is a no-op; if it does not, the ceiling binds and the
demand's δ term is what is discarded. **There is no configuration in which
both charge.**

And the identity that follows is the honest headline of this document:

```text
family 1 candidate (b)   ≡   family 2 ceiling, composed with the shipped demand
```

They are **the same formula written twice**. Candidate (b)'s `min` and family
2's `min` are the same `min`. That is why (b)'s "reduces to what at δ → ∞"
question has an ugly answer, and why the user is being shown one composition
diagram rather than two independent knobs: the real choice is *which
family-1 demand* sits inside the `min`, and (b) is what you get when you
choose the shipped one.

**The span term is inside the budget, and that is a claim, not a convention.**
`cap − BDP` is residence above the network window; resequencing span is
residence above the network window; therefore span is delay and δ budgets it.
The alternative — span added outside the ceiling — is defensible on the
grounds that resequencing delay is not *queueing* delay and is not the
sender's to shed, and it is what the c8 rows would need in order not to be
clipped. **I state both and choose neither.** The arithmetic consequence is
sharp and belongs in any pre-registration: at c8 the span is 2 563 against a
b=1 ceiling of 3 210, so where the span sits decides whether c8's law is
interior at all.

## Falsification plan

1. **P-INTERIOR, rebooted honestly.** `mem` bind fraction = 0 at c1, sc2, c7
   and c8; **c8L excluded in advance with the 1.83× arithmetic on the record.**
   §16.57's version of this clause was written without that exclusion and
   three of five cells went UNSCORED as a result.
2. **The conversion clause (load-bearing).** Delivered ICMP-probe p50 must
   land within a pre-registered band of `RTpropᵢ·(baseline_factor + b)` at
   each dial point, at every cell where the cap is provably interior AND the
   brake provably engaged. **The falsifier must be keyed to LATENCY, not
   goodput** — §16.57's was keyed to goodput and "could not fire where it was
   most needed", which that section records as a defect in its own bar.
3. **The prerequisite the conversion clause needs.** The engine-side `q_p50`
   and the independent probe disagreed by 3× on the cap→delay slope in the
   composed battery (caveat above). **Resolve that before scoring, not
   after**: it is the difference between "δ priced the queue" and "δ priced a
   queue the probe cannot see."
4. **The dial-continuity property test.** No VM: cap continuous and monotone
   in `b` over the whole interval, ±2 % nudges through Realtime / Auto / Bulk,
   plus the explicit assertion that the Bulk corner is a slope change and not
   a step. This is the CLAUDE.md gate expressed for the engine law.
5. **The refutation.** A cell where the δ-priced ceiling binds below the
   shipped cap and costs > 2σ goodput **at a cell with permitted headroom**.
   That would mean the contract's stated allowance is smaller than the wire's
   requirement — which refutes δ-as-queue-budget at the level of the idea, not
   the arithmetic, and is the single most valuable measurement in this
   document.

---

# What the LADDER battery already answers, and what needs a dedicated arm

The ladder (running separately under the SHIPPED-LAW CLEANUP goal) sweeps the
outstanding cap as a **magnitude** at the cells and reads goodput, delivered
latency and queue against it. Everything below is stated as a dependency, and
if the ladder's actual design differs from that reading, the split changes and
this section is what should be corrected first.

## Already answered by the ladder — NO dedicated arm needed

- **(d) ZERO's price, directly.** `W + S` is a magnitude: c1 201, sc2 374,
  c7 1 379, c8 4 232, c8L 10 041. Read it off the ladder's own curve. This is
  the cheapest question in the document and the ladder answers it for free.
- **Every family-2 prediction, at every dial point.** The ceiling values in
  the family-2 table are magnitudes too — 262 … 14 928. If the ladder covers
  that span, the goodput/latency cost of each dial point is already measured
  before any δ-priced law exists in code.
- **Whether c7 and c8 have a goodput knee below 4096 at all.** This is the
  question that decides whether ANY of these candidates can win, and it is
  purely a magnitude question.
- **The cap→delay conversion, at more than two points.** The conversion is
  currently anchored on sc2 (two arms, two eras, ≈ 20 %). A ladder gives it a
  slope per cell, which every latency falsifier above depends on.
- **c8L's memory starvation, confirmed or refuted.** If the ladder's highest
  rung at c8L is still improving goodput, the 4096 bound is the binder and
  finding 1 is confirmed on the wire.

## NEEDS a dedicated arm — a static ladder cannot express these

| question | why the ladder cannot answer it |
|---|---|
| **(a) TRANSIENT** — arming lead time, and whether a time-varying cap beats any fixed one | a ladder is a set of CONSTANT caps; the entire claim of (a) is that no constant is right at both instants |
| **(a′)** — the `p_lost`-weighted cap | same; the weight is a runtime scalar |
| **(c) UTILIZATION** — loop stability | a ladder has no loop. Also BLOCKED on the c8 `[WALL]` statistic (S-WALL INVERTED) regardless |
| **δ-continuity** | the ladder sweeps a MAGNITUDE, the invariant is about the DIAL. A property test, not a battery, and it needs no VM |
| **The composition / double-charge** | the `min` identity is algebra and is proven above; what needs measuring is that the SHIPPED code computes it, which is a `tests/formula_agreement.rs` entry, not an arm |
| **The (i)-vs-(ii) baseline choice** | they differ by 4 % (c8) and 50 % (c8L) — inside the ladder's own rung spacing at c8, and a derivation question at c8L |
| **The c8 warm-up finding** | a within-run `[3T]` time series at c8L, not a between-arm contrast; cheap, no VM, and it should precede any c8 verdict |
| **The min-RTT-inflation falsifier for (a)** | needs an arm edge to exist |

---

## FAMILY 2's DISPOSITION, annotated 2026-08-19 — the candidate is ADOPTED VIA THE DERIVED BAND, in the pool's VALUE MULTIPLIER, and NOT as the ceiling replacement this family's own formula proposes

**This annotation records what happened to family 2 downstream. It re-opens no
verdict above, prefers no candidate retroactively, and the Status line stays
`PROPOSED` — the adjudication was the user's and it was taken elsewhere.**

The path was: paper §16.67 restated family 2's δ pricing as a formula with its
provenance table before any code; the pre-registered candidates battery scored
it on the wire (goal-gate "Candidates Battery — RESULTS"; paper §16.70); and
the flip was executed in its own separate commit (paper §16.71).

- **WHAT WAS ADOPTED IS THE DERIVED BAND, NOT THIS SECTION'S CEILING.** What
  ships is `RWM_DELTA_CAP` ON: the POOLED law's VALUE multiplier becomes
  `1 + q(δ)` with `q(δ) = (b+1)/30` over RFC 8289 §3.2's cited 5–10 % band —

  ```text
  cap = clamp( (1 + q(δ)) · Σᵢ(bwᵢ · RTpropᵢ),  floor,  N · knee )
  ```

  — which is family 2's **idea** (δ prices the standing queue, as a time, read
  off the shipped dial with no new constant) landing in a different SEAT than
  family 2's formula puts it. **The `N·knee` ceiling and `WIN_STORE_MAX` are
  BOTH still in the law.** This section's *"NO knee. NO N·2048. NO swept pool"*
  is **not what shipped**, and no part of it is claimed as delivered.
- **THE CONFLATION THIS SECTION REFUSED TO DECIDE IS STILL UNDECIDED.** The
  *"one substantive provenance question"* above — reusing `D(δ)`, the shed
  deadline, as the queue budget — is **not what the shipped law does**: the
  shipped setpoint reads `b(δ)` and maps it onto CoDel's derived band, so the
  δ dial supplies the *dial position* and RFC 8289 supplies the *allowance*.
  That is a third answer to the question this section posed, and the section's
  own recorded worry is neither resolved nor inherited by what shipped.
- **THE DIAL-CONTINUITY REQUIREMENT (falsification plan item 4) IS MET AND
  ASSERTED**, without a VM: `codel_setpoint_q` is continuous and strictly
  monotone in `b` across Realtime/Auto/Bulk with ±2 % nudges through each,
  never leaves the band, and is pinned by
  `formula_agreement::published_codel_setpoint_equals_the_engine_map_and_spans_the_derived_band`.
  On the wire the dial ROUTED: `[DCAP]` echoed `q=0.100000 b=2.0000` on every
  engaged rep at every dual, one formula evaluated at one named point.
- **THE REFUTATION CLAUSE (item 5) DID NOT FIRE, and stays live after the
  flip.** No cell was found where the δ-priced bound binds below the shipped
  cap and costs > 2σ goodput at a cell with permitted headroom — measured at
  all five cells, with goodput PARITY at every dual on both seeds and no
  reading outside 2σ_pooled in either direction. Restated so it remains
  falsifiable now that the law ships by default.
- **WHAT IS NOT SETTLED.** Family 2's own P-INTERIOR clause (item 1) is
  **partially** answered: interior with the ceiling **provably inert** at c7
  and c8 (`pin` = 0.0000), and **UNRESOLVED at c8L**, where `pin` = 0.23 falls
  in the gap between the two branches the battery's contract pre-declared and
  neither is claimed after the fact. The named instrument is the **within-run
  Σ series**, which this ADR already owed and which needs no VM arm. Family 2's
  conversion clause (item 2) was **not** scored — the shipped law is not the
  ceiling replacement it is written against — and the `q_p50`-vs-probe
  prerequisite (item 3) is **still open**: the two disagreed in sign on one of
  the six scored rows, and the flip's latency claim rests on `q_p50` with the
  probe reported beside it.
- **FAMILY 1 IS UNTOUCHED.** Nothing here adopts, prefers or refutes (a), (b),
  (c) or (d), and the `17/8` question is not re-opened. The one connection
  worth recording is the reduction already stated in the paper: as `q → 0` the
  shipped multiplier becomes exactly one BDP per path, which **is** candidate
  (d) ZERO — so what ships is (d) plus the power-point allowance rather than a
  rival to it.

## What this document deliberately does NOT conclude

- **No candidate is preferred, ranked, or recommended.** The review is the
  user's.
- **No default moves and nothing is built.** No engine file, gate, test or
  default is touched by this ADR.
- **The (i)/(ii) baseline question is left open**, as is whether the span
  belongs inside the δ budget — and both are shown to be load-bearing at c8
  rather than filed as details.
- **Nothing here re-opens ADR-0058** (ONE POOL) or ADR-0064 (the unified span
  machine). Every candidate is a Σ over `live_paths()` that never counts
  paths.
- **No claim is made that any candidate clears c8.** Three of the five
  candidates provably do not, by arithmetic, and the reason is the span term
  and the memory bound rather than the slack — which is a finding *against*
  the framing of the goal item this document executes.

## Evidence

- **Paper**: §16.43 (the three terms and the stall law), §16.44 (the closed
  dwell loop), §16.47 (the cap as a signed latency control, 12/12), §16.51
  (honest anchors), §16.55 (ADR-0070's pointer), §16.56 (the composed law's
  formula and provenance table, with the 2026-08-18 term-1 amendment), §16.57
  (SHAPE confirmed / MAGNITUDE refuted, the 2.125 identity, the sc2 inversion),
  §16.59 (the queue-free clock REFUTED, the 1.7 %-of-90 % table; and the
  derived floor = 10 — now §16.60), §16.61 (this ADR's pointer).
- **Ledger** (`docs/goal-gate.md`): "The Cap Law On Trial" (`:29466`),
  "Composed-Cap Battery — RESULTS" (`:29966` — the `[3T]` decomposition, the
  headroom table, S-WALL, the `[WALL]` table, the defect in its own bar),
  "Latency Lever — BATTERY" (`:22031` — the 12/12 direction table and the
  headroom discipline), "Mechanical Defect Sweep, items 1 / 2 / 4" (`:30439`).
- **Code** (main@`1d83547`): `net/mod.rs:3001-3009` (`contract_stall_s` — the
  ρ = 1 collapse), `:762-768` (`shed_deadline_us` = D(δ)), `:2946-2952`
  (`delta_budget_b` — the dial), `:3190-3231` (`three_term_store_cap`),
  `:3442-3446` (`WIN_STORE_MAX`), `:1019` (`RELIABLE_STORE_MAX`),
  `:5452` + `diag.rs:321` (`tx_paused`), `:6479-6480` (`sender_idle`),
  `:7037-7049` (frontier-stall attribution / the cumulative-ack advance),
  `emit_source.rs:118` (the retransmit buffer), `:818` (`p_lost`),
  `sender_policy.rs:128-131` (the derived floor = 10), `:245` + `:763`
  (`delta_b`, resolved once), `:767` (`contract_rho = 1.0`),
  `walldiag.rs:35-47` + `:99-221` (the `[WALL]` measurand),
  `diag.rs:120-151` (the wait-arm histogram).
- **Tests relied on as existing instruments**: `slack_bench.rs` (the
  idle-vs-backlog replay and
  `the_queue_free_slack_clock_is_refuted_on_the_wire_measured_inputs`),
  `store_cap_bench.rs::derived_floor_is_the_max_of_its_two_clauses_and_only_moves_the_degenerate_end`,
  `formula_agreement.rs` (the agreement-test class any successor must join),
  `three_term_store_cap_value_is_linear_in_n_the_template_applied`,
  `delta_budget_b_is_the_dial_not_a_mode`.

## References

- **ADR-0070** — the trial this document is the successor enumeration for. All
  of its verdicts stand; §16.57 strengthened them and refuted its stated
  replacement.
- **ADR-0064** / **CLAUDE.md, THE NO-MODE-SWITCH INVARIANT** — every candidate
  is continuous in the dials by construction; the two places where that is
  *nearly* violated (family 1(a)'s time step, family 2's Bulk corner) are
  flagged in the open rather than argued past.
- **CLAUDE.md, FORMULA-FIRST LAWS** — the rule this document obeys: formula on
  its own line, per-symbol provenance, shape checked before any number.
- **ADR-0052** (pre-registration shape), **ADR-0066** (era honesty — applied
  here to §16.47's sc2 cap ratio, which is a pre-honest-anchor datum and is
  cited as a conversion slope rather than as a cap value), **ADR-0067** (why
  nothing flips), **ADR-0068** (the `Proposed` status this ADR copies).
