# Successor candidates to the cap law

Research memo (formerly ADR-0071). Two conceptual successors to the composed
cap law, written formula-first as candidates: the slack magnitude (what
replaces `17/8` as a permanent term) and the δ-priced queue bound (what
replaces `knee`/`N·2048` and `WIN_STORE_MAX` as a law).

## Status: proposed; no decision taken here

This memo enumerates, derives and prices candidates. It picks no winner and
ships nothing (no engine file, gate, default, test or paper claim), and it has
no recommendation section. Every number in the prediction tables is arithmetic
on already-published means, and the arithmetic is shown. Code line numbers are
as of main@`1d83547`.

Three corrections from the literature cross-check
([`literature-crosscheck.md`](literature-crosscheck.md); paper §11.4) apply to
claims this memo inherits; no candidate or verdict changes:

1. **The span term every candidate carries (`2·rate_fast·skew`) is ours, a
   novelty claim.** No publication writes a separable resequencing term of the
   shape `Σ bwᵢ·(RTT_max − RTTᵢ)` beside a window term (checked against
   RFC 6182 §5.3, RFC 8684 §3.3.4, Barré 2011, Raiciu NSDI'12 and DAPS, Kuhn
   ICC 2014). The published multipath sizings are aggregates with `RTT_max`
   outside the sum (`2·Σ bwᵢ·RTT_max`); our decomposition is one step of
   algebra from them and half their magnitude at N = 2. The literature may be
   cited for the term's magnitude, never for its shape.
2. **The `17/8` at the centre of family 1 inherits a tuned constant.**
   RFC 9002 §6.1.2 recommends `kTimeThreshold = 9/8` empirically ("Experience
   with QUIC shows that 9/8 works well"), and RACK (RFC 8985) uses 5/4 for the
   same purpose. Earlier "cited, not fitted" descriptions overstate the source.
3. **The displaced predecessor's `gain = 2.0` is the right value with two
   published derivations and a wrong local rationale**: RFC 6182 §5.3's `×2`
   and BBR's `cwnd_gain = 2` (ACK-aggregation absorption / minimum per-round
   rate-doubling gain). The "recovery runway" prose appears in no primary BBR
   source.

---

## Why these two

The composed cap law, measured on the wire, has the right shape and the wrong
magnitude (paper §6.1, §10). It is linear in the path count, carries no mode
bit, no δ/ρ threshold and no topology predicate, and its span term vanishes by
arithmetic at N = 1 in all 340 single-path evaluations. But it asks for
`3.125 · Σ(rate·K·RTprop) + span`, which exceeds the 4096 memory bound at
every dual cell and, where it is interior, buys 2.4× the standing queue for
zero goodput at parity and 1.43–1.48× worse delivered latency (sc2, both
seeds, far outside 2σ).

The one alternative explanation, the queue-free stall clock, was refuted
(paper §10): it sheds 1.7 % of a 90 % overshoot at c8, because at c8 `K` =
1.04 and there was nothing in the clock to remove. The magnitude owns the
overshoot.

**One formula-level fact governs both families.** At the shipped scope ρ = 1
(`contract_rho = 1.0`, `sender_policy.rs:767`: plain dyn cap ⇒ reliable ⇒
retain-until-acked), the stall term (paper §6.4) is

```text
stall(δ, ρ=1, srtt) = (1 − 1)·D(δ) + 1·(9/8·srtt + srtt) = 17/8·srtt
```

δ is multiplied by zero. The shipped composed law contains no δ at all at the
scope it ships in, so its design sentence ("`cap − BDP` is the standing queue;
δ prices queue as a latency budget") is unmeetable by construction, which is
the mechanism behind the measured "δ priced nothing" at sc2. Family 2 exists
because of this line; family 1 exists because `17/8·srtt` stands in δ's place.

**A second arithmetic fact sharpens it.** The largest delay allowance the δ
dial can express is `D(δ) = min(b(δ)·RTprop, 2·RTprop)` at `b(Bulk) = 2`
(`net::delta_budget_b`, `net::shed_deadline_us`), i.e. 2·RTprop. The shipped
slack is `17/8·srtt = 2.125·K·RTprop`, and `K` is floored at 1.0, so

```text
shipped slack ≥ D(δ)|Bulk   ⟺   2.125·K ≥ 2   ⟺   K ≥ 0.941   — always true
```

At the measured `K` (1.04 … 1.505) the shipped slack runs 2.21 … 3.20 RTprop
against a Bulk allowance of 2. The composed law charges more than the Bulk
delay allowance at every point of the δ dial, including Realtime, where the
allowance is ½·RTprop and the law charges 4.4× it.

---

## The measured inputs every candidate is scored on

All five cells, from the 833 `[3T]` evaluations of the composed battery
(ledger at ac1aed1, "Composed-Cap Battery — RESULTS", `[3T]` decomposition;
`K` from the same session):

| cell | `W` = Σ rate·K·RTprop | `S` = span | `K` | `BDP = W/K` | shipped composed `3.125·W + S` | published | arm A realized |
|---|---|---|---|---|---|---|---|
| c1 | 201 | 0 | 1.15 | 174.8 | 628.1 | 629 | 541 (legacy `2·BDP`, interior) |
| sc2 | 374 | 0 | 1.14 | 328.1 | 1168.8 | 1 168 | 1024 (`RELIABLE_STORE_MAX` latch) |
| c7 | 1 261 | 118 | 1.14 | 1 106.1 | 4059.1 | 4 059 | 4096 (pin) |
| c8 | 1 669 | 2 563 | 1.04 | 1 604.8 | 7778.6 | 7 778 | 4096 (pin) |
| c8L | 7 489 | 2 552 | 1.505 | 4 976.1 | 25 955.6 | 25 956 | 4096 (pin) |

The `3.125·W + S` column reproduces the published "mean unclamped" to the last
digit at all five cells, which checks that the decomposition is the law.

**Rate, for the candidates that need one.** Symbol ≈ 1.2 KB (the memory
bound's own arithmetic, `net/mod.rs:3442-3445`), so
`rate ≈ goodput_Mbit · 10⁶ / 9600` sym/s: c1 22 708, c7 18 104, c8 8 302,
c8L 7 479, sc2 9 146. This is an assumption, not a measurement.

**The cap→delay conversion is measured.** By Little's law delivered residence
is `occupancy / rate`. At sc2 the composed battery measured arm A cap 1024 /
`occ_p50` 1012 / `q_p50` 91 ms and arm C cap 2291 / `occ` 2214 / `q_p50`
218 ms: `Δocc / rate = 1202 / 9146 = 131 ms` against a measured `Δq` of
127 ms, closing to 3 %. The independent ICMP probe reads `1024/9146 = 112 ms`
predicted against 97.7 ms measured (latency-lever battery, arm A, seed 42),
and at that battery's arm-B cap of ≈ 471, 44.6 ms against 51 ms predicted.
The cap converts to delivered latency to within ≈ 20 % at sc2 across a 4.9×
range of cap, in two sessions and two eras, which makes every prediction
table below a falsifiable latency claim.

> **Caveat on the conversion.** Between arm A and arm C of the composed
> battery the engine-side `q_p50` moved with the full Little slope
> (0.106 ms/symbol ≈ 1/rate) while the independent probe moved at one third
> of it (0.036 ms/symbol). In the latency-lever battery the probe moved at the
> full slope. A likely mechanism is that above the bottleneck's buffer the
> extra backlog is held in the sender's store rather than on the wire, so the
> probe stops seeing it while the data's delivered latency still pays. This
> is a hypothesis and a prerequisite for family 2's falsifier.

**Two findings about these inputs, from arithmetic alone:**

1. **c8L cannot fund one BDP.** `W(c8L) = 7 489` symbols is 1.83×
   `WIN_STORE_MAX` = 4096 before any slack, span or δ. Term 1 alone exceeds
   the memory bound, so no cap law of any magnitude can be interior at c8L,
   and c8L must be pre-declared memory-starved and unscoreable for cap-law
   purposes. At least 1.83× of c8L's measured 6.3× overshoot (with `mem`
   bind 1.000) is a property of the resource limit, not of the law.
2. **c8 and c8L are the same geometry and their term 1 differs by 4.5×.**
   c8 = 25 MB / 2.54 s transfer, c8L = 200 MB / 20.5 s, identical netem;
   `W` reads 1 669 against 7 489 while goodput reads 79.7 against
   71.8 Mbit/s. Delivered rate is flat; the law's rate×RTprop input is not.
   Whatever the mechanism (a `max_bw` windowed max still warming at 2.5 s, an
   inflating `min_rtt`, or both), c8 measures the cap law's inputs during
   estimator warm-up, and every cap verdict taken at c8 inherits it. The test
   needs no VM arm: read the `[3T]` `window=` series within one c8L run at
   t ≈ 2.5 s and t ≈ 20 s. If it reproduces the 4.5×, c8 is a warm-up cell.

---

# Candidate family 1 — the slack magnitude

The slack term `rate · 17/8 · srtt` provisions the worst-case recovery
backlog (RFC 9002 detection plus one full retransmit round trip) permanently,
whether or not anything is stalled: a standing reservation for a transient
event. At a saturated ρ = 1 cell its measured payout was zero (sc2: 2.24× the
cap, 2.19× the outstanding, goodput 0.993 / 1.003, parity within 2σ) and its
measured premium was constant queue (2.4×, 218 ms against 91 ms, and 43–48 %
worse delivered latency).

All four candidates change only the second argument of
`net::three_term_store_cap`'s `slack` accumulator (`contract_stall_s`). Term 1
(`rate·K·RTprop`) and term 3 (the span) are untouched, and the shape
properties confirmed on the wire (linear in N, span ≡ 0 at N = 1, no mode bit,
no topology predicate) are preserved by construction in every one.

---

## (a) Transient slack — zero standing slack, a reserve drawn only while a stall is detected

```text
cap = Σᵢ [ rateᵢ·Kᵢ·RTpropᵢ ]  +  ARMED · Σᵢ [ rateᵢ · stall(δ, ρ, srttᵢ) ]  +  2·rate_fast·skew

  ARMED  =  tx_paused  ∧  (retransmit_buffer is non-empty)          ← the arming law
  release:  ARMED falls on the cumulative-ack advance that retires the blocking hole
```

and its continuous form (a′), the one compatible with the no-mode-switch
invariant:

```text
cap = Σᵢ [ rateᵢ·Kᵢ·RTpropᵢ ]  +  p_lost(age_oldest, ε̂, srtt, rttvar) · Σᵢ [ rateᵢ · stall(δ, ρ, srttᵢ) ]  +  2·rate_fast·skew
```

### Provenance

| symbol | provenance |
|---|---|
| `tx_paused` | measured, already computed: `net/mod.rs:5452`, `outstanding ≥ store_cap`; the store-cap backpressure edge, DIAG field `diag.rs:321`, wakes `wait_arm = 1` |
| `retransmit_buffer` non-empty | measured, already computed: `emit_source.rs:118`; the tail sweep reads exactly this predicate (`mod.rs:6102-6106`) |
| the release edge | measured, already computed: cumulative-ack advance, the event the frontier-stall attribution is charged on (`mod.rs:7037-7049`) |
| `p_lost(...)` (form a′) | measured, already computed: `control::fec_rate::p_lost`, called every emission at `emit_source.rs:818`; a scalar ∈ [0,1] on the oldest un-acked symbol's age, already load-bearing for the ARQ/FEC branch |
| `stall(δ, ρ, srtt)` | unchanged: `contract_stall_s`, `net/mod.rs:3001-3009` |

Zero new constants in either form. `ARMED` is a conjunction of two existing
booleans; `p_lost` is an existing scalar. The arming condition is the
dead-wall condition itself (store full and a hole outstanding), which is the
event the slack was provisioned for.

### Reduction check

Form (a) reduces to the shipped composed law exactly while armed, and to
candidate (d) while not. Form (a′) reduces to the shipped law as `p_lost → 1`
and to (d) as `p_lost → 0`, continuously, with both terms always computed
(the shipped rate law's shape).

### Predictions at the five cells

`ARMED = 0` is the standing state; the armed state equals the shipped law.

| cell | standing `W + S` | armed (= shipped) | vs `WIN_STORE_MAX` standing |
|---|---|---|---|
| c1 | **201** | 629 | interior |
| sc2 | **374** | 1 168 | interior |
| c7 | **1 379** | 4 059 | interior |
| c8 | **4 232** | 7 778 | still pins, by 3.3 % |
| c8L | **10 041** | 25 956 | still pins, 2.45× |

The c8 row counts against the whole family. Deleting the slack entirely at c8
sheds 45.6 % of the ask, and c8 must shed 47 % to clear the memory bound, so
zero slack is not enough at c8. The residue is the span term: at c8 `S` =
2 563 > `W` = 1 669. The span, jointly with the memory bound, is the c8
binder, not the slack; any successor scored on "does c8 go interior" fails
for a reason unrelated to family 1.

### Falsification plan

1. **Lead time (cheapest, no VM, kills (a) outright).** A reserve that arms
   after the wire has gone idle funds nothing. Measure `T_arm − T_prod`, where
   `T_prod` is `[WALL]`'s productive-suffix boundary (`walldiag.rs:199-221`),
   median over reps at c7 and c8. If the median is ≥ 0, (a) is dead and there
   is no coefficient to tune. The instrument reported on 199/199 live reps.
2. **The step.** (a) puts a step in the cap in time, not across a dial, so it
   is not the §5.10 / ADR-0064 mode switch on its face. But a cap that jumps
   from `W + S` to `3.125·W + S` on an edge is a burst-admission event, and the
   shipped `boot = 128` argument (`sender_policy.rs:573-577`) says why that is
   dangerous: a burst pre-bloats the queue, inflates the `min_rtt` floor, and
   so inflates the anchor that sizes the cap. Falsifier: the `min_rtt` of each
   live path must not fall in the 2 RTprop following an arm edge, measured
   in-run. (a′) exists to avoid this; writing it down does not settle it.
3. **Payout.** Wire-idle at the standing backlog, at every cell, on
   `slack_bench.rs`'s idle-vs-backlog replay (576 cells in 13 s, no VM). If
   idle at `S_standing = W + S` exceeds the pre-registered coverage point at
   any cell, the standing reservation was buying something.

---

## (b) δ-priced slack — slack can never buy more queue than the contract permits

```text
cap = Σᵢ [ rateᵢ·Kᵢ·RTpropᵢ  +  rateᵢ · min( stall(δ, ρ, srttᵢ),  D(δ, RTpropᵢ) ) ]  +  2·rate_fast·skew

  D(δ, RTprop) = min( b(δ)·RTprop,  2·RTprop )        ← net::shed_deadline_us, unchanged
```

### Provenance

| symbol | provenance |
|---|---|
| `D(δ, RTprop)` | shipped code, reused: `net::shed_deadline_us`, `net/mod.rs:762-768`; already the span law's deadline and the `(1−ρ)` half of `contract_stall_s` |
| `b(δ)` | declared dial: `net::delta_budget_b`, `mod.rs:2946-2952`: Realtime ½, Auto 1, Bulk 2 round trips of RTprop; pinned as a dial by `delta_budget_b_is_the_dial_not_a_mode` (paper §5.4) |
| `min(·,·)` | not a branch: continuous, monotone and non-expansive in both arguments; no threshold selects a code path |
| everything else | unchanged from the composed law (paper §6.4) |

Zero new constants. Structurally it puts `D(δ)` back into the law at ρ = 1,
where the shipped form multiplies it by `(1−ρ) = 0`.

### Reduction check — where the candidate is weak

`min(2.125·K·RTprop, b·RTprop)` with `b ≤ 2` (D's inner `min`) and `K ≥ 1.0`
means the min is `b·RTprop` always, at every measured cell and dial point. So:

- the candidate never reduces to the shipped composed law, at any δ;
- at Bulk it reduces to `cap = Σ rate·RTprop·(K + 2) + span`, ≈ 3·W at
  K ≈ 1, which lands 4 % below the shipped 3.125·W by coincidence of
  arithmetic. The shipped `17/8` is, to within 4 %, the Bulk corner of this
  candidate applied at every dial point;
- the `stall` argument of the `min` is inert at every cell in the record. A
  term that never binds is the pinned-law defect in the other direction: as
  written, (b) is the δ ceiling of family 2 with a dead argument attached.
  Making `stall` live would require removing D's inner `2·RTprop` cap, which
  is what makes the bound a bound. This candidate and family 2 are one
  formula written twice (see the composition section).

### Predictions at the five cells

With the `min` resolving to `b·RTprop`, `cap = W·(1 + b/K) + S`:

| cell | b = ½ (Realtime) | b = 1 (Auto) | b = 2 (Bulk) | shipped composed | arm A |
|---|---|---|---|---|---|
| c1 | **288** | **376** | **551** | 629 | 541 |
| sc2 | **538** | **702** | **1 030** | 1 168 | 1024 |
| c7 | **1 932** | **2 485** | **3 591** | 4 059 | 4096 |
| c8 | **5 035** | **5 837** | **7 442** | 7 778 | 4096 |
| c8L | **12 528** | **15 018** | **19 993** | 25 956 | 4096 |

Interior at c1, sc2 and c7 at every dial point; still pinned at c8 and c8L at
every dial point, for the span/memory reason under (a). At Bulk, sc2 lands at
1 030 against the shipped latch of 1 024, a 0.6 % coincidence and not
evidence.

### Falsification plan

1. **Conversion (load-bearing).** By the measured cap→delay conversion this
   law promises delivered residence `RTprop·(K + b)` per path. Pre-register
   that band per cell per dial point and score the independent ICMP probe,
   not the engine's `rtt=`. If the probe median at Realtime exceeds
   `RTprop·(K+½)` by more than 2σ at any cell where the cap is provably
   interior and the brake provably engaged, δ is not pricing the queue and the
   candidate is refuted. The composed battery's own falsifier was keyed to
   goodput; this one is keyed to latency.
2. **Dial continuity (no VM).** A property test over `b ∈ [0.4, 2.2]` in fine
   steps asserting the cap is continuous and monotone in `b`, plus ±2 %
   nudges through each named point (the `test_visualizer.mjs` pattern applied
   to the engine law). A step at a named point is a defect even if each side
   is individually correct.
3. **Goodput.** At the cells with permitted headroom (c1 75.9 %, c8 18.7 %,
   c8L 21.8 %; c7 3.1 % and sc2 1.6 % carry no throughput target,
   MEASUREMENT DISCIPLINE 16), a > 2σ goodput regression at Realtime means the
   contract's allowance is below what the wire needs, which refutes
   δ-as-queue-budget rather than the arithmetic.

---

## (c) Utilization-argued slack — provision what measured idleness justifies

```text
cap = Σᵢ [ rateᵢ·Kᵢ·RTpropᵢ ]  +  ( Σᵢ rateᵢ ) · T_idle_measured  +  2·rate_fast·skew

  T_idle_measured = the recovery-idle time the sender directly observes,
                    per recovery episode, from the [WALL] / wait instruments
```

### Provenance

| symbol | provenance |
|---|---|
| `T_idle_measured` | measured: `[WALL]`'s `duration_ms`, `walldiag.rs:99-221`: the terminal window in which the loop woke on neither the TUN arm nor the PAUSED arm and `last_source_send_us` did not advance. Resolution `it_ms` = 0.04–0.15 ms, three to four orders below the walls it measures |
| the alternative reading | `wait_us[1]` (`paused`) / `wait_n`: the tick-share of sender wall time woken by store-full backpressure, `diag.rs:120-151`; the latency-lever battery measured it moving 40 % → 7 % and 65 % → 5 % exactly where the law raises the cap |
| Little's law | the same law terms 1 and 2 already are |

Zero new constants, and one unresolved definition. `duration_ms` is the
duration of the terminal dead window, not an idle time per recovery episode;
converting it to a backlog needs an episode count that `[WALL]` does not
report. The predictions below use `duration_ms` directly and are an upper
reading.

### Reduction check

Reduces to (d) when the measured idle is zero, and to a value near the shipped
slack when the measured idle equals the derived stall, which is the
self-consistency check `slack_bench.rs` was built to run.

### Predictions at the five cells

Using arm-A `[WALL] dur_ms` medians (626.2 c8, 219.2 c8L, 144.7 c7, 20.2 sc2,
1.5 c1) and the rates above:

| cell | `rate · dur_ms` | `cap = W + that + S` | shipped composed | comment |
|---|---|---|---|---|
| c1 | 34 | **235** | 629 | interior |
| sc2 | 185 | **559** | 1 168 | interior; near the latency-lever battery's winning cap (≈ 471) |
| c7 | 2 620 | **3 999** | 4 059 | reproduces the shipped ask to 1.5 % |
| c8 | 5 199 | **9 431** | 7 778 | worse than shipped |
| c8L | 1 639 | **11 680** | 25 956 | better, still pins |

At the one cell with a large measured wall, a feedback law that funds the
measured idleness asks for 21 % more than the law already refuted for asking
too much.

### Loop stability

This is the only candidate that closes a feedback loop from an outcome back
into the law: `slack → cap → less idle → less slack → more idle`. It has no
stable interior fixed point by construction: if the provisioning works, the
measurand goes to zero and withdraws the provisioning. The expected behaviour
is a limit cycle with period ≈ 2× the measurement window. The tree has
refuted this species of circularity twice: the `2×anchor` Copa-sole cap
(`net/mod.rs:4571-4583`, "samples can never read above the store-capped
delivered rate", L0-measured stuck at 3.2k of 10.4k sym/s), and the
`cap → wireQ → srtt → K → slack → cap` loop measured at 1.505 on c8L. A
candidate that closes a third owes an argument that this one is different;
none is available.

### Falsification plan

1. **Stability (fires first).** Pre-register a maximum within-rep oscillation
   ratio on the realized cap (`p95/p05` from `[CCAP]`), and require that the
   time-averaged idle be no worse than the constant-slack arm's. Cheap at the
   SF bench, no VM.
2. **Blocking prerequisite.** `[WALL]` failed its own stability trial at c8:
   `sign(median dur_ms(C) − median dur_ms(A))` read −1 / +1 / −1 across three
   pools collected minutes apart on one binary (S-WALL, inverted), the same
   event that voided an earlier measurand. A law whose input is that statistic
   cannot be scored at c8. The fix is a design change (a paired within-rep
   contrast, or a cell whose statistic is not bistable), not a third
   measurand.

---

## (d) Zero — the null candidate

```text
cap = Σᵢ [ rateᵢ·Kᵢ·RTpropᵢ  +  rateᵢ · (1 − ρ)·D(δ, RTpropᵢ) ]  +  2·rate_fast·skew

  i.e.  stall(δ, ρ) = (1 − ρ)·D(δ)  —  the ρ·(9/8·srtt + srtt) term deleted
```

### Provenance

Every symbol is already provenanced (paper §6.4); this candidate removes a
term and adds nothing. It deletes the only place `9/8` appears, which
discharges the `9/8` provenance question by removing its subject.

### Reduction check

Exact at ρ → 0, where the shipped `ρ·(…)` term is itself zero. Maximally
divergent at ρ = 1, the shipped scope. It remains continuous in ρ and δ with
both terms always computed, and is more δ-live than the shipped law, which
has no δ at ρ = 1.

### Predictions at the five cells

At ρ = 1 the remaining stall is zero, so `cap = W + S`: c1 201, sc2 374,
c7 1 379, c8 4 232, c8L 10 041 (the standing column of (a)).

### The case against

The measured payout at sc2 was zero, but the argument that it should be zero
is not free: a retransmit occupies a store slot too. If the store is exactly
`W + S` and every slot is held by an un-acked symbol, the retransmit that
would clear the blocking hole has nowhere to go. The counter is that a
retransmit re-sends an already-stored symbol and needs no new slot; that is
believed true of this engine's retransmit path but is unverified in the code,
and it is the first thing to check before (d) is taken seriously.

### Falsification plan — the cheapest here

1. **No VM, no wire.** `slack_bench.rs` replays each cell's measured store
   residence against a backlog `S` and reports wire-idle against `S` (576
   cells in 13 s). Read the idle at `S = W + S_span` at every cell. If idle
   exceeds the pre-registered coverage point anywhere, the payout was not zero
   and (d) is refuted.
2. **On the wire, if it survives the bench.** A > 2σ goodput regression at
   any cell with permitted headroom (c1, c8, c8L). The ladder battery may
   answer this without a dedicated arm (last section).

---

# Candidate family 2 — the δ-priced queue bound

## The formula

```text
cap  =  min( demand,  ceiling )

ceiling = Σᵢ over live_paths [ rateᵢ · ( baselineᵢ  +  δ_headroomᵢ ) ]

  δ_headroomᵢ = D(δ, RTpropᵢ) = min( b(δ)·RTpropᵢ, 2·RTpropᵢ )     ← net::shed_deadline_us
  baselineᵢ   = RTpropᵢ            (reading ii — the queue-free clock)
              | Kᵢ·RTpropᵢ         (reading i  — the ack-round-trip clock)
  demand      = the three-term law, with whichever family-1 slack is chosen

  No knee. No N·2048. No swept pool.
  WIN_STORE_MAX survives outside the law as a resource limit that may abort,
  never as a term that shapes, and its bind fraction is reported.
```

## What `δ_headroom` is at each named hint, and why this is not a mode switch

`δ_headroom` is a time, in round trips of the path's own RTprop, read from the
shipped dial:

| hint | `b(δ)` | `δ_headroom` | the promise |
|---|---|---|---|
| Realtime | ½ | ½·RTprop | queue at most half a round trip |
| Auto | 1 | 1·RTprop | queue at most one round trip |
| Bulk | 2 | 2·RTprop | queue at most two round trips |

The hints are named points: `net::delta_budget_b` looks up a number on the
dial (`delta_b: f64`, `sender_policy.rs:245`, doc: "a NUMBER on a dial… Not a
mode selector"), and the law reads only that number. There is no
`if hint ==`, no threshold on δ, and the cap is continuous and strictly
monotone in `b` on the whole interval: affine in `b` up to D's own `min` at
2, where it has a slope change (a corner, not a step). That corner at the Bulk
endpoint is the only non-smooth point in the family and should be flagged in
review, since it is close to the patterns the invariant forbids.

**The open provenance question.** `D(δ)` is the shed deadline, the age past
which a retransmit is not worth sending. Reusing it as the queue budget is the
zero-constant choice and the reason to prefer it, but the two are different
jobs: a retransmit-worthiness horizon versus a standing-queue allowance. The
alternative is a second dial point for queue, which is a new constant and
would need a derivation. The conflation is recorded, not resolved.

## Predictions at the five cells

`ceiling = W·(1 + b/K)` under reading (i); `ceiling = W·(1 + b)/K` under
reading (ii). Span is inside the budget in both (see the composition below),
so it is not added on top:

| cell | (ii) b=½ | (ii) b=1 | (ii) b=2 | (i) b=1 | current `N·knee` / latch | shipped composed |
|---|---|---|---|---|---|---|
| c1 | **262** | **350** | **524** | 376 | 1024 (`RELIABLE_STORE_MAX`, N=1) | 629 |
| sc2 | **492** | **656** | **984** | 702 | 1024 | 1 168 |
| c7 | **1 659** | **2 212** | **3 318** | 2 485 | **4096** | 4 059 |
| c8 | **2 407** | **3 210** | **4 815** | 3 274 | **4096** | 7 778 |
| c8L | **7 464** | **9 952** | **14 928** | 12 466 | **4096** | 25 956 |

Read the table for its shape. At c7 and c8 the δ-priced ceiling lands at
0.54× and 0.78× the shipped 4096 at Auto: the operating point would be a law
at a dual cell, not the clamp. At c8L it cannot be: `W/K` = 4 976 alone
exceeds the memory bound (finding 1). At c1 and sc2 it is well inside the
legacy latch.

**Reading (ii) also closes the `K` loop.** Under (ii) the ack-path overhead
`(K−1)·RTprop` is charged against the δ budget rather than granted free,
which is correct if `K = 1 + wireQ/RTprop`, because that overhead is delay.
Under (i) it is granted, on the amendment that term 1 must fund one ack round
trip. The two readings differ by exactly `(K−1)/(1+b)`: 4 % at c8 and 50 % at
c8L. Choosing between them is a derivation question about the contract's
baseline.

## The memory bound's remaining role

`WIN_STORE_MAX` = 4096 stops being a term and becomes a resource limit stated
outside the law, 4096 × ~1.2 KB ≈ 5 MB, which may abort or refuse but never
shapes. Any battery that scores this must state:

1. Its bind fraction is reported (`[CCAP] mem=`), per the formula-first clamp
   rule, and a non-zero bind is a stop, not a datum (the composed battery
   measured that condition firing).
2. c8L is pre-declared unscoreable: term 1 alone is 1.83× the bound, so the
   bound binds under every candidate at every dial point, and reporting c8L as
   "the law pinned" repeats the MEASUREMENT DISCIPLINE 18 error.
3. If the δ-priced ceiling is right, the memory bound's value becomes a
   capacity-planning question (how much delay a 5 MB budget funds at a given
   rate), and the answer at c8L is less than one BDP: a statement about the
   product, not the formula.

## Composition with family 1 — δ is charged once

δ appears in the demand (through any family-1 candidate that uses `D(δ)`) and
in the ceiling. They must not compose additively. Per path:

```text
demand_i  = rate_i·(baseline_i + stall_i(δ, ρ))
ceiling_i = rate_i·(baseline_i + D(δ, RTprop_i))

cap_i = min(demand_i, ceiling_i)
      = rate_i·( baseline_i + min( stall_i(δ,ρ), D(δ,RTprop_i) ) )
```

The `min` distributes over the shared `baseline`, so δ is charged exactly
once: the ceiling can only remove time from the demand. If the demand's δ
term respects the budget the ceiling is inert; if not, the ceiling binds and
the demand's δ term is discarded. No configuration charges both.

The identity that follows:

```text
family 1 candidate (b)   ≡   family 2 ceiling, composed with the shipped demand
```

Candidate (b)'s `min` and family 2's `min` are the same `min`. The real choice
is which family-1 demand sits inside it; (b) is what the shipped demand gives.

**The span term is inside the budget, and that is a claim.** `cap − BDP` is
residence above the network window; resequencing span is residence above the
network window; therefore span is delay and δ budgets it. The alternative,
span outside the ceiling, is defensible because resequencing delay is not
queueing delay and is not the sender's to shed, and it is what the c8 rows
would need to avoid clipping. Both are stated; neither is chosen. The
consequence belongs in any pre-registration: at c8 the span is 2 563 against
a b=1 ceiling of 3 210, so where the span sits decides whether c8's law is
interior at all.

## Falsification plan

1. **P-INTERIOR.** `mem` bind fraction = 0 at c1, sc2, c7 and c8; c8L
   excluded in advance with the 1.83× arithmetic on the record. The composed
   battery's version of this clause lacked that exclusion, and three of five
   cells went unscored.
2. **Conversion (load-bearing).** Delivered ICMP-probe p50 must land within a
   pre-registered band of `RTpropᵢ·(baseline_factor + b)` at each dial point,
   at every cell where the cap is provably interior and the brake provably
   engaged. The falsifier must be keyed to latency, not goodput; the composed
   battery's goodput-keyed falsifier could not fire where it was most needed.
3. **Prerequisite for 2.** The engine-side `q_p50` and the independent probe
   disagreed by 3× on the cap→delay slope in the composed battery (caveat
   above). Resolve that before scoring: it separates "δ priced the queue" from
   "δ priced a queue the probe cannot see".
4. **Dial continuity (no VM).** Cap continuous and monotone in `b` over the
   whole interval, ±2 % nudges through Realtime / Auto / Bulk, and an explicit
   assertion that the Bulk corner is a slope change and not a step.
5. **Refutation.** A cell where the δ-priced ceiling binds below the shipped
   cap and costs > 2σ goodput at a cell with permitted headroom. That would
   mean the contract's stated allowance is below the wire's requirement,
   refuting δ-as-queue-budget as an idea; it is the most valuable measurement
   here.

---

# What the ladder battery answers, and what needs a dedicated arm

The ladder sweeps the outstanding cap as a magnitude at the cells and reads
goodput, delivered latency and queue against it. The split below depends on
that design; if the ladder differs, correct this section first.

## Answered by the ladder, no dedicated arm

- **(d)'s price, directly.** `W + S` is a magnitude (c1 201, sc2 374,
  c7 1 379, c8 4 232, c8L 10 041); read it off the ladder's curve.
- **Every family-2 prediction at every dial point.** The ceiling values are
  magnitudes too (262 … 14 928). If the ladder covers that span, the cost of
  each dial point is measured before any δ-priced law exists in code.
- **Whether c7 and c8 have a goodput knee below 4096 at all**, which decides
  whether any candidate can win.
- **The cap→delay conversion at more than two points.** It is anchored on sc2
  (two arms, two eras, ≈ 20 %); a ladder gives a slope per cell, which every
  latency falsifier depends on.
- **c8L's memory starvation.** If the ladder's highest rung at c8L still
  improves goodput, the 4096 bound is the binder and finding 1 is confirmed.

## Needs a dedicated arm

| question | why the ladder cannot answer it |
|---|---|
| **(a) transient**: arming lead time, and whether a time-varying cap beats any fixed one | a ladder is a set of constant caps; (a)'s claim is that no constant is right at both instants |
| **(a′)**: the `p_lost`-weighted cap | the weight is a runtime scalar |
| **(c) utilization**: loop stability | a ladder has no loop; also blocked on the c8 `[WALL]` statistic (S-WALL inverted) |
| **δ-continuity** | the ladder sweeps a magnitude, the invariant is about the dial; a property test, no VM |
| **The composition / double charge** | the `min` identity is algebra; what needs checking is that the shipped code computes it, a `tests/formula_agreement.rs` entry |
| **The (i)-vs-(ii) baseline choice** | they differ by 4 % (c8) and 50 % (c8L): inside the ladder's rung spacing at c8, a derivation question at c8L |
| **The c8 warm-up finding** | a within-run `[3T]` time series at c8L, not a between-arm contrast; no VM, and it should precede any c8 verdict |
| **The min-RTT-inflation falsifier for (a)** | needs an arm edge to exist |

---

## Family 2's disposition: adopted via the derived band in the pool's value multiplier, not as this ceiling

This records what happened to family 2 downstream. It re-opens no verdict
above and ranks no candidate; the status line stays "proposed".

The δ pricing was restated as a formula with its provenance table before any
code (paper §6.1), scored on the wire by the pre-registered candidates battery
(ledger at ac1aed1, "Candidates Battery — RESULTS"; paper §6.1, §9.4), and
flipped in its own commit.

- **What shipped is the derived band, not this ceiling.** `RWM_DELTA_CAP` is
  on: the pooled law's value multiplier is `1 + q(δ)` with
  `q(δ) = (b+1)/30` over RFC 8289 §3.2's cited 5–10 % band:

  ```text
  cap = clamp( (1 + q(δ)) · Σᵢ(bwᵢ · RTpropᵢ),  floor,  N · knee )
  ```

  This is family 2's idea (δ prices the standing queue, as a time, read off
  the shipped dial with no new constant) in a different seat from family 2's
  formula. The `N·knee` ceiling and `WIN_STORE_MAX` are both still in the
  law; "no knee, no N·2048, no swept pool" is not what shipped.
- **The conflation above is still undecided.** The shipped law does not reuse
  `D(δ)` as the queue budget: the setpoint reads `b(δ)` and maps it onto
  CoDel's derived band, so the δ dial supplies the dial position and RFC 8289
  supplies the allowance. That is a third answer to the question posed above.
- **Dial continuity (falsification item 4) is met and asserted** without a
  VM: `codel_setpoint_q` is continuous and strictly monotone in `b` across
  Realtime/Auto/Bulk with ±2 % nudges through each, never leaves the band, and
  is pinned by
  `formula_agreement::published_codel_setpoint_equals_the_engine_map_and_spans_the_derived_band`.
  On the wire `[DCAP]` echoed `q=0.100000 b=2.0000` on every engaged rep at
  every dual.
- **The refutation clause (item 5) did not fire and stays live.** No cell was
  found where the δ-priced bound binds below the shipped cap and costs > 2σ
  goodput at a cell with permitted headroom: goodput parity at every dual on
  both seeds, no reading outside 2σ_pooled in either direction.
- **Not settled.** P-INTERIOR (item 1) is partially answered: interior with
  the ceiling provably inert at c7 and c8 (`pin` = 0.0000), and unresolved at
  c8L, where `pin` = 0.23 falls between the two pre-declared branches. The
  named instrument is the within-run Σ series, which needs no VM arm. The
  conversion clause (item 2) was not scored, since the shipped law is not the
  ceiling it is written against, and the `q_p50`-vs-probe prerequisite
  (item 3) is still open: the two disagreed in sign on one of the six scored
  rows, and the flip's latency claim rests on `q_p50` with the probe reported
  beside it.
- **Family 1 is untouched.** Nothing here adopts, prefers or refutes (a)–(d).
  As `q → 0` the shipped multiplier becomes exactly one BDP per path, which is
  candidate (d); what ships is (d) plus the power-point allowance.

## What this memo does not conclude

- No candidate is preferred, ranked or recommended.
- No default moves and nothing is built.
- The (i)/(ii) baseline question is open, as is whether the span belongs
  inside the δ budget; both are load-bearing at c8.
- Nothing here re-opens the single shared pool (paper §6.1, §6.7) or ADR-0064
  (the unified span machine). Every candidate is a Σ over `live_paths()` that
  never counts paths.
- No candidate is claimed to clear c8. Three of the five provably do not, by
  arithmetic, and the reason is the span term and the memory bound rather than
  the slack.

## Evidence

- **Paper**: §6.1 (the pooled cap, the ×N defect and the δ-cap result), §6.2
  (the single-path cap and the sc2 481-symbol measurement), §6.4 (the
  contract stall), §6.6 (why the pins passed), §9.4 (the derived laws), §10
  (the three-term law, the composed cap and the queue-free slack clock,
  refuted), §11.4 (the span-term novelty and the 9/8 and gain = 2.0 sources).
- **Ledger at ac1aed1**: "The Cap Law On Trial" (`:29466`), "Composed-Cap
  Battery — RESULTS" (`:29966`: the `[3T]` decomposition, the headroom table,
  S-WALL, the `[WALL]` table), "Latency Lever — BATTERY" (`:22031`: the 12/12
  direction table and the headroom discipline), "Mechanical Defect Sweep,
  items 1 / 2 / 4" (`:30439`).
- **Code** (main@`1d83547`): `net/mod.rs:3001-3009` (`contract_stall_s`, the
  ρ = 1 collapse), `:762-768` (`shed_deadline_us` = D(δ)), `:2946-2952`
  (`delta_budget_b`, the dial), `:3190-3231` (`three_term_store_cap`),
  `:3442-3446` (`WIN_STORE_MAX`), `:1019` (`RELIABLE_STORE_MAX`),
  `:5452` + `diag.rs:321` (`tx_paused`), `:6479-6480` (`sender_idle`),
  `:7037-7049` (frontier-stall attribution / the cumulative-ack advance),
  `emit_source.rs:118` (the retransmit buffer), `:818` (`p_lost`),
  `sender_policy.rs:128-131` (the derived floor = 10), `:245` + `:763`
  (`delta_b`, resolved once), `:767` (`contract_rho = 1.0`),
  `walldiag.rs:35-47` + `:99-221` (the `[WALL]` measurand),
  `diag.rs:120-151` (the wait-arm histogram).
- **Tests used as instruments**: `slack_bench.rs` (the idle-vs-backlog replay
  and `the_queue_free_slack_clock_is_refuted_on_the_wire_measured_inputs`),
  `store_cap_bench.rs::derived_floor_is_the_max_of_its_two_clauses_and_only_moves_the_degenerate_end`,
  `formula_agreement.rs` (the agreement-test class any successor must join),
  `three_term_store_cap_value_is_linear_in_n_the_template_applied`,
  `delta_budget_b_is_the_dial_not_a_mode`.

## References

- Paper §6.6 and CLAUDE.md, FORMULA-FIRST LAWS: the store-cap review this memo
  is the successor enumeration for (formerly ADR-0070, deleted in 94bf58d).
  Its verdicts stand; the composed-cap battery strengthened them and refuted
  its stated replacement.
- ADR-0064 and CLAUDE.md, THE NO-MODE-SWITCH INVARIANT: every candidate is
  continuous in the dials by construction; the two near-violations (family
  1(a)'s time step, family 2's Bulk corner) are flagged above.
- ADR-0052 (pre-registration shape); MEASUREMENT DISCIPLINE 9 (era
  comparison, applied here to the latency-lever battery's sc2 cap ratio, a
  pre-honest-anchor datum cited as a conversion slope rather than a cap
  value); ADR-0068 (the "proposed" status this memo copies).
