# Dead-wall mitigations — the metastability literature mapped onto OUR recovery plane

**A candidates document, not a decision record.** ERA LEDGER item 4. No ADR is
taken, no default moves, no gate is flipped, and — see §6 — **no mechanism is
built**. Companion to `literature-crosscheck.md` CD-3, which named the failure
class; this document asks what CD-3's published mitigation list actually costs
against the mechanisms this tree already has.

**Date**: 2026-08-19
**Branch**: `docs/mitigations-and-skeleton` from main@`b98d537`. **DOCS ONLY.**
No VM was contacted, no L1 number re-derived, no benchmark run, no engine line
touched.
**Related**: `literature-crosscheck.md` CD-3 and CD-6; goal-gate *"The
Latency-Feedback Source"*, *"B-WALL"*; paper §16.70, §16.71; ADR-0071.

**THE STRICT RULE APPLIED THROUGHOUT.** A candidate is admissible only if it is
**constant-free** — every symbol priced by δ, by ρ, by r, or by a signal this
tree already measures. A mitigation that needs a threshold nobody in this tree
can derive is recorded **REFUTED-FOR-NOW**, with the missing derivation named,
and it is not softened into a "future work" bullet.

---

## 0. FIRST: what today's default already does, and what residue is left

**This section exists because the honest answer to "how bad is the dead wall?"
changed eight days' worth of ledger ago, and any proposal written against the
old answer would be solving a problem the tree already shipped a fix for.**

`RWM_DELTA_CAP` went **DEFAULT ON on 2026-08-19** (`gates.rs:919`; paper §16.71;
flip commit `e9c6b24`). The shipped pooled cap is now

```text
  cap = clamp( (1 + q(δ)) · Σᵢ(bwᵢ · RTpropᵢ),  floor,  N · knee )
  q(δ) = 0.05 + 0.05·(clamp(b(δ), ½, 2) − ½)/(2 − ½)   ==   (b + 1)/30
```

(`net/mod.rs:2958-2964` `pool_value_multiplier`, `:2904-2909` `codel_setpoint_q`).
That is HotOS '21's *"reduce internal queue sizes"* — the one item on the
published mitigation list this tree has actually executed — with RFC 8289 §3.2's
derived 5–10 % standing-queue setpoint in place of the `gain = 2.0` fossil.

**And it moved the dead wall.** Goal-gate `:32819-32836`, B-WALL, the paired c8
contrast:

| pool | seed | paired | non-zero | D<A | D>A |
|---|---|---|---|---|---|
| main | 42 | 12 | 9 | 6 | 3 |
| main | 7 | 6 | 6 | 5 | 1 |
| topup | 7 | 2 | 2 | 2 | 0 |
| topup2 | 7 | 7 | 6 | 5 | 1 |

> **B-WALL RESOLVES: `dur_ms(D) < dur_ms(A)`.** … Pooled: **18 of 23 non-zero
> pairs favour D** (two-sided sign test p ≈ 0.011). **The δ-cap SHORTENS c8's
> dead wall.**

No predecessor's dead-wall contrast resolved at all. Two earlier measurands
inverted between pools collected minutes apart (goal-gate `:30098-30114`,
`:30141`); this one resolved on a **paired within-rep design fixed in advance**
for exactly that reason.

### 0.1 THE RESIDUE — what is demonstrably NOT fixed

Six things, each with its citation, because a proposal that ignores them is
proposing against a strawman.

1. **B-WALL is a DIRECTION, not a magnitude.** A sign test over 23 paired
   differences says D's wall is shorter. It does not say by how much, and the
   contract forbids reading one out: c8L's `[WALL]` is *"reported
   direction-only and scored on nothing, as pre-declared"* (goal-gate `:32836`).
   **The dead wall is shortened. It is not gone, and no number bounds what is
   left.**

2. **The clock that SETS the wall's quantum is untouched.** The dead time is an
   integer number of recovery rounds, and a round is
   `tail_sweep_timeout_us(srtt) = (2·srtt).clamp(25 ms, 100 ms)`
   (`net/mod.rs:570-572`; receiver twin `hole_nack_refresh`, `:579-582`). c8's own
   measured SRTT is `rtp_med + q_p50 = 38 + 338 = 376 ms`, which **overshoots the
   100 ms ceiling by 7.5×**, so *"a recovery round costs 100 ms of wall whatever
   the path does and ~1.1 s of dead wall is ~11 of them"* (goal-gate
   `:28014-28021`). The δ-cap changes the pool, not the clock.

3. **That clock is measured as a CONSTANT, and it violates RACK's own
   false-alarm budget.** Paper §16.71: the shipped
   `round = (2·srtt).clamp(25 ms, 100 ms)` *"binds 92.4–99.7 % of the time at
   every cell and violates RACK's own spurious budget by 1.7–12.0×. **Nothing
   about that clock is repaired here**"*. A clamp that binds 99.7 % of the time
   is not a law; it is `100 ms` wearing a law's clothes (CLAUDE.md FORMULA-FIRST,
   *"a clamp that always binds turns its law into a constant"*).

4. **The δ-cap's effect on the clock is real but unregistered.** Arming the
   δ-cap and touching no clock *"reduce[d] the SHIPPED recovery clock's
   false-alarm rate by **24 % at c8 and 50 % at c8L**, and by nothing at sc2
   where it does not engage"* (§16.71). §16.71 labels this *"a corroborating
   observation and not a bar — no clause pre-registered it"*. It is evidence the
   sustaining loop and the pool are coupled; it is not a result.

5. **No cap-shaped mitigation can reach the collapse at all.** Goal-gate
   `:28014`: `wait_paused` = 0 in **13 of 13** collapse reps — *"the gate a
   store-sizing law acts on is NEVER CLOSED while the collapse is happening."*
   Whatever the δ-cap did, it did **not** do it by braking on the store cap.
   **Any candidate below that acts through the store-cap gate is dead on
   arrival, and §3 is the one that fails this way.**

6. **The statistic is length-scoped, and 25 MB is where it lives.** The wall is
   a roughly fixed count of 100 ms rounds, so its share is inversely
   proportional to transfer length: *"c8's battery transfers are 25 MB / 2.44 s
   … so ~1.1 s of tail is a **30 % tax**; the same tail on c7's 200 MB / 9.23 s
   would be 12 % and on c1's 14.2 s, 8 %"* (goal-gate `:28112-28119`), and the
   harm measured **0/24 at 200 MB** (goal-gate `:29568-29569`). **The dead wall
   is a short-transfer phenomenon.** Any mitigation priced against it is priced
   against a 25 MB cell.

Plus two scope limits carried from §16.71 itself: **c8L is UNRESOLVED**
(`pin` = 0.2312 fell in the gap between the contract's two pre-declared
branches), and there is **no single-path support in either direction** — the
δ-cap returns before any multiplier is read at `n_live < 2`, so c1 and sc2 are
bit-identical to the control by construction.

**Summary of the residue.** The sustaining loop's *fuel* (a too-deep pool) has
been cut with a derived setpoint. Its *clock* — the 100 ms ceiling that turns
one recovery into 100 ms of wall at a 376 ms path — has not been touched, is
measured pinned at 92.4–99.7 %, and is the quantity every remaining candidate
below is really arguing about.

---

## 1. The published mitigation list, quoted

CD-3 §(e), from Bronson, Aghayev, Charapko & Zhu, HotOS '21 (**quoted
first-hand from the PDF** — verification ledger, `literature-crosscheck.md:2603`):

> "we might **disable failover and retries or set a retry budget**, switch to
> **LIFO scheduling**, **reduce internal queue sizes**, **enforce priorities
> during overload**, **shed load by rejecting a fraction of requests or
> clients**, or even use the **Circuit Breaker pattern** to block all requests."

Huang et al., OSDI '22 (**second-hand** — quoted by a research worker, not
independently re-verified; same ledger):

> "**By far, the most common sustaining effect is due to the retry policy,
> affecting more than 50% of the studied incidents.**"

> "**Theorem 2 (Stable region).** Define **Cstable = Cnorm /(w∗L ∗ w∗C)**. If
> Lnorm < Cstable, then the system will never have a metastable failure."

> "Load shedding was the most popular mitigation effort used in over 50% of the
> incidents. … **However, without a proper understanding of the metastability
> and feedback loops, it is hard to know just how much the load needs to be
> reduced.**"

And the warning that constrains every row below — Marc Brooker (AWS), via
CD-3 §(f):

> *"Circuit breakers are designed to turn partial failures into complete
> failures."* … *"The adaptive strategy isn't modal in the same way, and seems
> to perform better at lower failure rates."*

**The token bucket is the continuous formulation; the circuit breaker is the
mode switch.** Under THE NO-MODE-SWITCH INVARIANT that is not a preference in
this tree, it is a constraint: **no candidate in this document may be a
tripping brake.**

**ONE ITEM IN THE BRIEF HAS NO CITATION HERE, AND IT IS RECORDED RATHER THAN
INVENTED.** *Backoff-with-jitter* is named in this deliverable's own brief, but
it appears **nowhere** in HotOS '21's mitigation list, nowhere in CD-3, and
nowhere in `literature-crosscheck.md` — a full-text search for `jitter`,
`backoff`, `back-off` returns only our own measured-RTT-variance uses. §5 scores
it anyway, against the general practice, and says plainly that its citation is
**NOT IN THIS TREE'S EXTRACTION**.

---

## 2. THE MAP

One row per published mitigation. Every `file:line` is main@`b98d537`.

### Row A — LOAD SHEDDING → the δ-honest shed law

| | |
|---|---|
| **Published shape** | HotOS '21: *"shed load by rejecting a fraction of requests or clients"*. OSDI '22: most-used mitigation (>50 % of incidents), and its own indictment — *"hard to know just how much"*. |
| **Our nearest mechanism** | `RWM_UNIFIED_SHED`, **DEFAULT ON** (`gates.rs:828`, doc `:62-64`). Admission rule `net/mod.rs:1021-1031`; deadline `net/mod.rs:1013-1015`; budget refreshed live at `net/emit_source.rs:664-687`; decision sites `net/mod.rs:7792-7815` (sender ARQ) and `net/emit_source.rs:830-855` (correction slot); receiver twin `net/mod.rs:1041-1055`. |
| **The law** | `D(δ) = min(b(δ)·RTprop, 2·RTprop)`; shed iff `age > D(δ)` **and** `shed_total + 1 ≤ budget · src_total`, with `budget = ε̂·(1 − P_fec)` (`control/fec_rate.rs:673-679`) — the derived `1 − ρ`. |
| **Constant audit** | **CLEAN.** δ enters through `b(δ)` (`net/mod.rs:3420-3426`, the named points ½/1/2), ρ through the residual-loss budget, r and ε̂ through `residual_loss_after_fec`. No fitted constant. This is the tree's best existing example of the shape this document is asking for. |

**THE GAP, and it is the single most important finding in this document.**

```rust
// net/mod.rs:1005-1007
pub(crate) fn shed_armed(unified_on: bool, reliable: bool, gate: bool) -> bool {
    unified_on && !reliable && gate
}
```

`!reliable`. The shed law is armed **only on the EVICT path**. And
`contract_rho` is set at `net/sender_policy.rs:1027` as

```rust
let contract_rho: f64 = 1.0;   // constant by SCOPE — the plain dynamic cap
                               // exists only on RETAIN-UNTIL-ACKED
```

**So on the seat where the dead wall was measured, ρ = 1, `reliable` is true,
and the shed law is INERT BY CONSTRUCTION.** The one published mitigation this
tree has implemented, priced correctly and shipped default-ON, **does not exist
at the operating point where the failure it would mitigate occurs.** Goal-gate
`:13292` records the same fact from the other side: *"the shed law is inert
on…"* the reliable arms.

**CANDIDATE VERDICT: REFUTED — and refuted BY CONTRACT, not by a missing
derivation.** Shedding at ρ = 1 is not an unpriced threshold, it is a breach of
the retention contract. ρ = 1 means retain-until-acked; a mitigation that
discards a retained symbol has not mitigated the failure, it has changed the
product. **There is nothing to derive here and nothing to build.** The correct
record is that the dead wall lives on the one contract point where load
shedding is definitionally unavailable, and that is a property of ρ, not a
defect.

**ONE OBSERVATION RECORDED AGAINST CLAUDE.md, deliberately not acted on.**
`shed_armed` takes a **boolean** `reliable`, so the ρ axis enters this law as a
predicate rather than as a value — the shape THE NO-MODE-SWITCH INVARIANT
warns about. It is harmless *today* because `contract_rho` is a compile-time
1.0 on this seat, so no dial position is being routed. **It stops being
harmless the moment ρ becomes a live dial**, and a successor who makes ρ
continuous inherits this boolean as a step. Recorded here so that successor
finds it; no change proposed, because changing it today would be a change with
no reachable behaviour and no test that could bound it.

---

### Row B — CIRCUIT BREAKING → the NACK congestion throttle

| | |
|---|---|
| **Published shape** | HotOS '21: *"use the **Circuit Breaker pattern** to block all requests"*. Envoy: `base_ejection_time` 30 s × ejection count, capped — **multiplicative growth per re-trip with a cap**. Brooker, against: circuit breakers *"turn partial failures into complete failures"*; prefer token buckets. |
| **CD-3's own translation row** | `RWM_INFL_CAP` / `cwnd_full` (**built, disabled**) ↔ circuit breaker / admission control (`literature-crosscheck.md:1745`). |
| **Our nearest mechanism** | Two, and they are different things. **(i)** `NackCongestionState` (`net/mod.rs:1076-1140`) — an AIMD throttle on reactive repair. **(ii)** the in-flight brake `cwnd_full` (`net/mod.rs:5803-5838`), whose doors `RWM_LATE_BRAKE` (`gates.rs:908`) and `RWM_INFL_CAP` (`gates.rs:365`, default 0) are OFF, and whose BDP door rides `gen_pipe`, *"which is off on the plain seat"* (`gates.rs:585`). |

The throttle, in full:

```rust
// net/mod.rs:1076-1140, condensed
if loss_rate > prev_loss_rate * 1.1 + 0.001 { rising_loss += 1 } else { rising_loss = 0 }
if curr_rtt   > prev_rtt + 1 ms            { rising_rtt  += 1 } else { rising_rtt  = 0 }
let congested = rising_loss >= 2 && rising_rtt >= 2;
if congested { repair_multiplier = (repair_multiplier * 0.5).max(0.0) }
else if stable { repair_multiplier = (repair_multiplier + 0.1).min(1.0) }
```

**GOOD NEWS FIRST: the shape is already the one Brooker argues for.** This is
multiplicative-decrease / additive-increase on a *continuous* multiplier in
[0, 1], not a trip to zero. It composes into `cached_max_repairs = 10 ×
multiplier` (`net/mod.rs:7559-7561`) and can reach 0 — the rejected blanket
`.max(1)` floor is documented as rejected at `net/mod.rs:7562-7574`. **We are
not missing a circuit breaker; we have the adaptive strategy the literature
prefers to one.**

**THE GAP: five un-derived constants and one threshold-keyed regime.** `0.5`,
`0.1`, `2`, the `1.1` relative-rise factor and the `0.001` absolute floor have
no provenance in the repository. Worse for CLAUDE.md's purposes, `congested`
is a **conjunction of two counter thresholds** — a predicate that selects a
different multiplier law on each side. That is a behaviour step keyed on a
threshold, in the recovery plane, shipped.

**CANDIDATE VERDICT: REFUTED-FOR-NOW. The missing derivation is named, and it
is the one CD-3 already told us to measure.** A principled multiplicative
decrease is not free to pick: OSDI '22 Theorem 2 says recovery requires
dropping below `Cstable = Cnorm/(w*L · w*C)` — *below the amplification factor*,
not merely below the tipping point — and the Google SRE Book's worked instance
of that gap is **11×** (trips at 11 000 QPS, recovers below ~1 000). **This tree
has never measured `w*L` or `w*C`.** Until it has, replacing `0.5` with any
other number is swapping one un-derived constant for another, which is exactly
the defect ADR-0070 exists to prevent.

**The named prerequisite** is `literature-crosscheck.md` Tier 3 item 3.2
(`:2513`): *"The metastable amplification factor `w*L·w*C` at c8, instead of
tuning the cliff. Report the **collapse RATE** … not a mean."* See §4 — the
load half of it is computable from columns this tree already records.

---

### Row C — ADMISSION THROTTLING ON RECOVERY WORK → the NACK budget

| | |
|---|---|
| **Published shape** | HotOS '21: *"disable failover and retries or **set a retry budget**"*. OSDI '22: *"the most common sustaining effect is due to the retry policy, affecting more than 50% of the studied incidents."* DAGOR (SoCC '18, five years in production, **second-hand**): the overload signal is **queuing time, not utilisation**, with AIMD (α = 5 %, β = 1 %) on the admission threshold. |
| **Our nearest mechanism** | `BudgetAllocator` (`control/fec_rate.rs:687-718`), wired at `net/mod.rs:7576-7612`, enforced at `net/mod.rs:7645-7649` and `:7779-7781`, decremented at `:8035` and `:8097`. Plus the per-seq cooldown `retx_cooldown_us(srtt, floor) = srtt.max(floor)` (`net/mod.rs:544-546`, enforced `:7818-7825`). |
| **The law** | `total = p_upper/(1 − p_upper) + codec_overhead`; `nack_cap = total − proactive`; then `nack_cap_symbols = ⌊nack_cap · sources_this_period⌋`, spent down per retransmit. |

**This is a genuine retry budget, and its derivation is sound** — `p_upper` is
the estimator's predictive loss upper bound at the controller's own tail-loss
target (`net/mod.rs:7588`), and the split against proactive FEC conserves the
repair budget by construction.

**THE GAP.** `net/mod.rs:7610-7612`:

```rust
nack_cap_symbols
    .saturating_sub(nack_repairs_this_period)
    .max(MAX_NACK_REPAIRS_PER_NACK as u64)   // == .max(10)
```

The derived budget is floored at the constant **10** (`net/mod.rs:204`). The
comment at `:7594-7605` says why, and the reason is *arithmetic, not control*:
the period resets every 10 acked seqs, so `⌊nack_cap · sources_this_period⌋`
**truncated to 0 almost always**, silently suppressing the entire reactive
repair path. The floor is a patch on an integer truncation. Its consequence is
that the retry budget — the exact quantity OSDI '22 identifies as the most
common sustaining effect in the studied population — **can never be driven to
zero by its own law.**

**THE CANDIDATE, and it is the strongest one in this document: replace the
truncation with an accumulator.** Carry the fractional budget forward in `f64`
and spend integer tokens from it, instead of flooring the truncated integer.
That is Brooker's token bucket, it preserves the floor's actual intent
(quantisation must not starve wireless-loss repair), it lets genuine overload
zero the budget, and it is **constant-free with a net constant count of −1**:
`MAX_NACK_REPAIRS_PER_NACK`'s use as a *floor* disappears, priced instead by
`p_upper`, `codec_overhead` and the source count — all already measured.

**CANDIDATE VERDICT: ADMISSIBLE, NOT CLEAN, NOT BUILT.** Three reasons, in
descending weight:

1. **The zero it would unlock is already reachable by an adjacent mechanism.**
   The drain check is `if cached_max_repairs == 0 || cached_nack_budget == 0`
   (`net/mod.rs:7645-7649`), and `cached_max_repairs = ⌊10 · nack_multiplier⌉`
   **can already reach 0** through Row B's throttle. So the floor does not
   actually prevent the retry path from closing; it prevents *this one term*
   from closing it. The gap is real but it is much smaller than it looks, and
   "closes cleanly" is not an honest description of it.
2. **Its effect on the dead wall is unmeasurable in this deliverable, and the
   tree has already forbidden claiming it anyway.** B-WALL's pre-declared close
   (goal-gate `:31333-31355`, restated `:32525-32531`) says: *"No dead-wall
   claim of any kind is made from an unpaired contrast in this battery, at any
   n"*, and the paired design needs a c8 VM battery at n = 12/seed. This
   deliverable is `--doc`.
3. **The floor has a measured history that a doc-only change may not
   override.** `net/mod.rs:7598-7605`: *"(L1 C2: floor+sweep took 287 → 38 inner
   retransmits per 5×1.8MB.)"* Removing it is a change with a prior on the
   record, and the prior points the wrong way.

**What it is owed before it may be built**, in the tree's own order: the
formula first (§4's amplification instrument, so the budget's target has a
derivation rather than a shape argument), then a pre-registration naming the
paired c8 contrast and its power, then the gated arm. That is three commits and
a VM battery, and it is item 4's honest successor — not item 4.

---

### Row D — BACKOFF WITH JITTER → nothing, and the citation is missing too

| | |
|---|---|
| **Published shape** | **NOT IN THIS TREE'S EXTRACTION.** Absent from HotOS '21's mitigation list, from CD-3, and from `literature-crosscheck.md` entirely. Scored below against general practice only, with no verbatim quotation available. |
| **Our nearest mechanism** | **None.** There is no randomised jitter anywhere in the retransmit or NACK path. Every use of "jitter" in this tree is a *measured RTT-variation statistic* — `patience_floor_us(jitter, srtt) = 1 ms + min(jitter, srtt)` (`net/mod.rs:283-288`), `PathState::rtt_jitter_us()`. |

**THE GAP, stated precisely because it is a real structural property.** All
recovery clocks on all paths are deterministic multiples of a **single pooled**
SRTT: `pooled_recovery_srtt_us = max over active path RTTs`
(`net/mod.rs:536-538`), feeding both `retx_cooldown_us` and the sweep. So every
hole in a batch becomes eligible for retransmission at the same instant, on
every path, every round. **That is synchronised retry — precisely the pattern
randomised backoff exists to break** — and it is a plausible contributor to
c8's measured `retx` 1.412× on `tc_drop` 1.064× (goal-gate `:27819-27831`): 41 %
more retransmits on 6 % more link loss.

**CANDIDATE VERDICT: REFUTED-FOR-NOW, on three counts.**

1. **The spread has no derivation.** Randomised backoff needs a distribution
   width. Nothing in δ, ρ or r prices it.
2. **The natural constant-free width is a quantity this tree does not
   report.** The obvious candidate is the measured RTT standard deviation
   `rtt_sigma_us()` (`scheduler/mod.rs:3032-3037`) — which
   `research/cost-ratio-memo.md:591-595` records as **measured but NEVER
   REPORTED**: *"σ is estimated, not measured, everywhere in this memo, by
   inverting an inequality across two clocks."* **The named missing
   derivation is therefore a named missing MEASUREMENT, and it is already owed
   by another document.** Report σ, and this row becomes derivable.
3. **It would inject non-determinism into the measurement apparatus.** Every
   A/B arm in this tree, and the wasm sim's golden fingerprints, rest on
   deterministic replay. A randomness source in the recovery plane is a cost
   charged to the whole discipline stack, and it must be argued separately from
   the mitigation's merit.

---

### Row E — REDUCE INTERNAL QUEUE SIZES → the δ-cap. **ALREADY CLOSED.**

Recorded as a row so the list is complete and so no successor proposes it
twice. Published: on HotOS '21's list; CoDel's derived 5–10 % setpoint; and
RFC 896 (1984) explicitly rejecting the opposite — *"**Adding additional memory
to the gateways will not solve the problem** … the onset of congestion collapse
will be delayed but when collapse occurs an even larger fraction of the packets
in the net will be duplicates."* Ours: §0 above. **DELIVERED, default ON,
B-WALL 18/23 at p ≈ 0.011.** Residue per §0.1.

---

## 3. The one candidate that fails on §0.1 item 5, recorded so it is not re-proposed

**LIFO scheduling / priority-under-overload** (both on HotOS '21's list) act on
a *queue the system controls admission to*. Our analogue would be the store-cap
gate. Goal-gate `:28014`: `wait_paused` = **0 in 13 of 13** collapse reps — the
sender is not blocked on its cap while the collapse is happening. **Any
reordering or prioritisation of work behind that gate is provably inert on the
dead wall**, whatever its merits elsewhere. **REFUTED — on the measurement, not
on a missing derivation.** This is the same trap ADR-0070 finding 5 named when
it called the `active_paths()` cliff *"the loop's only stabiliser"*: a brake
that acts where the collapse is not.

---

## 4. The instrument every REFUTED-FOR-NOW row above is waiting on

Rows B and C both stall on the same missing quantity, and it is not a
mechanism — it is a **measurement**, which is why this document proposes it as
the successor rather than proposing code.

**`w*L`, the load amplification factor, is computable from columns this tree
already records.** OSDI '22 defines the sustaining effect as work amplification;
its load half is retransmit work per unit of real loss. Goal-gate `:27819-27831`
already prints both operands per class at c8:

```text
  collapse   normal    ratio
  retx      2120      1501.5    1.412
  tc_drop    182       171      1.064
                       w*L ≈ 1.412 / 1.064 ≈ 1.33   (c8, collapse vs normal)
```

**That arithmetic is one division on two committed columns, and it has never
been performed.** It is constant-free by construction (a ratio of measured
counters), it needs no VM, and it is exactly what `literature-crosscheck.md`
Tier 3 item 3.2 asks for. It is **not performed here either** — the numbers
above are the two-class medians from one section of one battery, and turning
them into a reported instrument means deciding the denominator (per rep? per
class? per pool?) and re-deriving it across cells, which is an L1 analysis pass
and not a docs deliverable.

**It is named, with its inputs, as item 4's owed successor.** With `w*L` on the
record, Row B's `0.5` acquires a target (`Cstable`) and Row C's budget acquires
a bound; without it, both are constant-swaps.

**The capacity half `w*C` has no instrument at all** and this document does not
invent one.

---

## 5. THE MITIGATION MAP, one line each

| # | published mitigation | our nearest mechanism (file:line) | the gap | verdict |
|---|---|---|---|---|
| A | load shedding (HotOS '21; OSDI '22 >50 %) | δ-honest shed law, `net/mod.rs:1021-1031` + `:1013-1015`, budget `fec_rate.rs:673-679`; **default ON** `gates.rs:828` | `shed_armed` requires `!reliable` (`net/mod.rs:1005`) and `contract_rho ≡ 1.0` (`sender_policy.rs:1027`) ⇒ **inert at the dead wall's operating point** | **REFUTED — by contract.** Shedding at ρ = 1 breaks retention. Nothing to derive. |
| B | circuit breaking (HotOS '21; Envoy; Brooker *against*) | `NackCongestionState` AIMD, `net/mod.rs:1076-1140`; in-flight brake `cwnd_full` `net/mod.rs:5803-5838` (doors OFF) | five un-derived constants (0.5 / 0.1 / 2 / 1.1 / 0.001) and a threshold-keyed `congested` predicate | **REFUTED-FOR-NOW.** Missing derivation: `Cstable = Cnorm/(w*L·w*C)`; `w*L` never measured. See §4. |
| C | admission throttling on recovery work / retry budget (HotOS '21; OSDI '22 >50 %; DAGOR) | `BudgetAllocator` `fec_rate.rs:687-718`, wired `net/mod.rs:7576-7612`; per-seq cooldown `net/mod.rs:544-546` | budget floored at the constant 10 (`net/mod.rs:7612`, `:204`) — a patch on integer truncation; the derived budget can never reach zero | **ADMISSIBLE, NOT CLEAN, NOT BUILT.** Token-bucket accumulator is constant-free (net −1), but the zero is reachable via Row B, the effect is unmeasurable under `--doc`, and the floor has a measured prior. |
| D | backoff with jitter | **none.** All clocks are deterministic multiples of one pooled SRTT (`net/mod.rs:536-538`) ⇒ synchronised retry | no derivable spread; the natural width `rtt_sigma_us()` is **measured but never reported** (`cost-ratio-memo.md:591-595`) | **REFUTED-FOR-NOW.** Also: the published citation is **absent from this tree's extraction**. Missing prerequisite: report σ. |
| E | reduce internal queue sizes (HotOS '21; CoDel; RFC 896 *against* "add memory") | the δ-cap, `net/mod.rs:2958-2964` + `:2904-2909`; **default ON** `gates.rs:919` | — | **ALREADY CLOSED.** B-WALL 18/23, p ≈ 0.011 (goal-gate `:32832`). Residue per §0.1. |
| F | LIFO / priorities under overload (HotOS '21) | the store-cap gate | `wait_paused` = 0 in **13/13** collapse reps (goal-gate `:28014`) | **REFUTED — on the measurement.** Provably inert on this failure. |

---

## 6. WHAT WAS BUILT: NOTHING, AND WHY THAT IS THE DELIVERABLE

Item 4's own rule was *"if exactly one candidate closes cleanly AND is small,
build it gated; otherwise the deliverable is the candidates doc alone."*

**No candidate closes cleanly.** Row C is the only admissible one and it fails
"cleanly" on its own §2 analysis: the zero it unlocks is already reachable, its
effect cannot be measured under this deliverable's gate, and the constant it
deletes has a measured prior pointing the other way. Rows A and F are refuted on
the record rather than on a missing derivation. Rows B and D are refuted-for-now
with their missing derivations named — the amplification factor `w*L`, and a
reported σ — and **both of those prerequisites are measurements, not
mechanisms.**

**So the deliverable is this document. Item 4: DELIVERED.**

The successor is not a mechanism either. It is §4's arithmetic — `w*L` from two
columns already committed — and after it, in this order: the formula, the
pre-registration naming the paired c8 contrast and its power, and only then a
gated arm. Anything that skips to the arm is the defect ADR-0070 was written
about.
