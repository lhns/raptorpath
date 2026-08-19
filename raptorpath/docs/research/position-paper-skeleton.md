# Position-paper skeleton — a publishable paper from this tree's record

**FOR THE USER'S REVIEW. This is an outline, not a draft, and not a decision.**
ERA LEDGER item 6.

- **No submission target is chosen.** Section 0 lists candidate venues with what
  each would force, and picks none.
- **No claim beyond the tree's record.** Every assertion below is either
  citable to a ledger section / paper section / `file:line`, or is explicitly
  marked **[NOT ON THE RECORD]** with what it would take to put it there.
- **Every number is citable.** Where a number is load-bearing and its
  provenance is weak, §7 says so by name rather than leaving it to a reviewer.

**Date**: 2026-08-19. **Branch**: `docs/mitigations-and-skeleton` from
main@`b98d537`. **DOCS ONLY** — no VM, no L1 number re-derived, no engine line.

---

## 0. Framing, and the honest positioning question

**The thesis, one sentence.** *A transport whose every load-bearing constant is
required to carry a derivation can be built, and the discipline that enforces
that requirement produces more publishable results by refuting its own
proposals than by shipping them.*

**Why this is a position paper and not a systems paper.** The tree has no
end-to-end performance win to report. Its two shipped flips are **parity**
results — §16.71 states it in its own words: *"the honest claim is 'free'
rather than 'faster'"*. What it has instead is a *method* and a *refutation
record*. A systems venue would ask for the win; a position/experience venue
asks for the lesson. **Choosing otherwise would require claims this tree's
record does not support, and the skeleton is built so that no such claim is
available to the author.**

Candidate venues, and what each would force (**not chosen**):

| venue | what it wants | what it would force on us |
|---|---|---|
| HotNets / HotOS | a provocative position, 6 pp | drop §1 almost entirely; lead with §3 |
| CoNEXT / IMC | measurement rigour | §5 must have run; the c9 battery becomes a gate |
| SIGCOMM CCR editorial | community lesson | §4 leads; §1 becomes an appendix |
| A "negative results" track | refutations as contribution | §3 leads; strongest fit for the record as it stands |

---

## 1. THE VALIDATED-LAWS TABLE

**The section's job:** show that the laws are laws — each an expression, with a
provenance for every symbol, and a measurement that could have refuted it.

**Structure:** one row per shipped law. Columns: **formula** · **each symbol's
provenance** (measured / cited / declared dial / resource bound) · **literature
anchor OR novelty status** · **the measurement that validated it, with its
falsifier**.

**Source of the provenance discipline:** `CLAUDE.md` "FORMULA-FIRST LAWS
(ADR-0070)" — *"Each symbol gets a one-line provenance … 'Argued in a commit
message' is not provenance, and a constant with none does not ship."*

### Draft rows

| # | law | formula | anchor / novelty | validation |
|---|---|---|---|---|
| 1.1 | **The pooled outstanding cap** (shipped, both 2026-08-19 flips composed) | `cap = clamp((1 + q(δ))·Σᵢ(bwᵢ·RTpropᵢ), floor, N·knee)`, `q(δ) = (b+1)/30` | **CITED** — RFC 6182 §5.3 for the `Σ bwᵢ·RTTᵢ` shape; **RFC 8289 §3.2** (CoDel, Kleinrock power) for the 5–10 % setpoint | §16.71 rung D: **6/6** pre-registered clauses; `q_p50` −16/−10 ms (c7), −113.5/−117.0 (c8), −200/−130 (c8L); goodput inside 2σ_pooled in **both** directions at every dual |
| 1.2 | **The `×N` deletion** (`RWM_SUM_CAP`, flipped §16.64) | the count multiplier deleted: `Σ`, not `N·Σ` | **DERIVED** — the law's own doc comment named a linear quantity | §16.63 ladder rung N: changes the cap on **100 %** of engaged refreshes at both duals; costs nothing measurable |
| 1.3 | **The span term** | `span = rate_fast·(RTT_max − RTT_min)` | **NOVEL DECOMPOSITION** — see §2.1. Magnitude anchored to RFC 6182 / BLEST / Raiciu / Barré / Kuhn; the *subtracted form* is ours | Raiciu NSDI '12 *"For equal delay paths, MPTCP's receiver memory consumption is also close to zero"* reproduced on our wire: span **0.0000** over every c7 rep, and identically 0.000 in all **340** single-path evaluations (§16.57) |
| 1.4 | **The three-term stall law** | `stall = (1−ρ)·D(δ) + ρ·(9/8·srtt + srtt)` | **CITED, WITH A CORRECTION** — `9/8` is RFC 9002's `kTimeThreshold`, but *"Experience with QUIC shows that 9/8 works well"* is an empirical recommendation. Corrected tree-wide in Tier 0 (commit `46c3e6a`) | `slack/window ≡ 2.125` in **833 of 833** evaluations at ρ = 1 (§16.57) — and see §7, this is a *degeneracy*, not a confirmation |
| 1.5 | **The δ-honest shed law** | `D(δ) = min(b(δ)·RTprop, 2·RTprop)`; shed iff `age > D(δ)` ∧ `shed_total+1 ≤ (1−ρ)·src_total`, with `1−ρ = ε̂·(1−P_fec)` | **NOVEL** as an expression; the *shape* is admission control | **[WEAK — see §7]** default ON, but structurally inert at ρ = 1 |
| 1.6 | **The rate law** (the no-mode-switch exemplar) | `r(β) = (1−β)·r_anchor + β·r_late-is-fine`, `β = bulkness_of_delta(δ)` | **NOVEL** — the architecture's central claim | continuity + monotonicity through **every preset** (±2 % nudges), `test_visualizer.mjs`; `test_continuum_one_law_across_the_dial` |

**A row that must be in the table and is not a win** — because leaving it out
would make the table a highlight reel:

| 1.7 | **The recovery clock** | `round = (2·srtt).clamp(25 ms, 100 ms)` | base is **RFC 8985 §7.2's TLP PTO verbatim**; the clamp has **no counterpart** — every published bound in this family is RTT-relative | **DEFECT FINDING, MEASURED**: binds **92.4–99.7 %** at every cell; violates RACK's own <7 % spurious budget by **1.7–12.0×** (§16.70). *This law operates as a constant.* |

**Row 1.7 is the section's honesty anchor.** It is the one shipped law the
tree's own instrument convicted, and it was convicted by the instrument built
to judge its *successor*. A validated-laws table that omits it is the highlight
reel MEASUREMENT DISCIPLINE 18 exists to forbid.

---

## 2. THE THREE NOVELTY CLAIMS, WITH THEIR NEGATIVE SEARCHES

**The section's job:** state three claims of priority, each backed by a
*documented* negative search, and state plainly that the three searches are
**not of equal strength**. §2.4 is not optional.

### 2.1 The span decomposition

**Claim.** Every published multipath buffer formula places `RTT_max` *outside*
the sum — RFC 6182 §5.3, RFC 6824/8684 §3.3.4, Barré IFIP 2011 §4.1, Raiciu
NSDI '12 §4.2, Kuhn ICC 2014 §II.D Eq. 3, unanimously. The heterogeneity
allowance written as a per-path **difference**,

```text
    2·Σᵢ bwᵢ·RTT_max − 2·Σᵢ bwᵢ·RTTᵢ  =  2·Σᵢ bwᵢ·(RTT_max − RTTᵢ)
```

is ours. **Negative search** (`literature-crosscheck.md:206-211`, verbatim):

> **HONESTY NOTE, and it is important.** *No published source writes the
> subtracted form* `Σ bwᵢ·(RTT_max − RTTᵢ)` *as a named resequencing term.*
> This was checked against all five sources above. The decomposition is one step
> of algebra from their formula, but **it is our formulation and must be
> presented as our derivation, never as a quotation.**

**Strength: MEDIUM.** The exact source set checked is named (five papers/RFCs);
**no query strings are recorded**.

**The claim must be stated at its true size, and the paper must do this
explicitly:** the novelty is *algebraic/notational*, "one step of algebra from
their formula" — and DAPS Eq. (2) already uses the RTT *difference*, for
blocking **time**; *"no source multiplies it by a bandwidth to get a buffer.
Our span term is that unwritten product"* (`:364`).

**And it must carry its own counter-evidence.** CD-2: the reorder-buffer
literature's balanced design scales `W ∝ D²`, contradicting our linear form;
our own wire measured the term **45.4 % over-funded at c8 with goodput going
UP** (§16.63); Eyerman et al. drop the analogous term outright (*"we assume this
term is zero"*). **We are claiming priority on a decomposition three
literatures independently call negligible.** That is a defensible position —
*we found the honest form of a term everyone else deletes* — but only if stated
that way.

### 2.2 Nested delay-CC stability as an open problem we can state precisely

**Claim.** ADR-0068 puts Copa's δ-priced delay control inside ADR-0071 family
2's outer cap, which is *itself* a delay budget: two delay-regulating loops,
nested, on the same path delay. **No published stability analysis of that
topology exists.**

**Negative search — the strongest of the three, because it records its method**
(`literature-crosscheck.md:1224-1239`): arXiv `abs:"nested control loops" AND
abs:"congestion"` → 0; `all:"congestion control" AND "flow control" AND
"nested"` → 0; `abs:"congestion control" AND abs:"cascaded"` → 0;
`abs:"congestion control" AND abs:"inner loop"` → 0; `abs:"congestion control"
AND abs:"two-level"` → 1 (irrelevant); DBLP `nested loops congestion control` →
0; full-text search of the Copa paper for `nested`, `receive window`, `rwnd`,
`flow control`, `cascade` → **zero hits**.

**Its own hedge, quoted in the paper:** *"no such result is prominent or
well-cited"*, **not** *"provably does not exist"* — the paywalled full-text
indexes were not queryable.

**What we can state precisely, and this is the contribution.** The general
control literature supplies the constraint the networking literature has not
applied: Skogestad & Postlethwaite §10.2 p. 387 (time-scale separation *"a
factor of five or more"*), Hollot et al. TAC 47(6) 2002 (`ω_g·R₀ < 0.85`;
closed-loop time constants bounded by `R₀/2`), and the unanimous `gain ∝ 1/delay`
(Vinnicombe; Low/Paganini/Doyle), against Copa's own proved `C·Σ 1/δᵢ < BDP`.
**Our separation appears to hold — and holds BY ACCIDENT**: it has never been
stated as a requirement, and the `WIN_STORE_MAX`/knee clamps are exactly the
nonlinearity a linear cascade argument does not cover
(`literature-crosscheck.md:1332-1336`).

**So the claim is: "here is an open problem, here is why it is well-posed, and
here is the one measured instance in the literature of it going wrong"** —
Huang et al. IMC 2012's *downward spiral effect*, an outer receive-window loop
destabilising an inner CC loop. **Not: "we solved it."**

### 2.3 Sequential detection for transport timeouts

**Claim.** *"How long should the sender wait before declaring the ack failed?"*
is quickest-change-detection, a problem with a mature optimality theory —
Lorden 1971 Theorem 1 (`Ē_θN*(γ) ~ log γ / I(θ)` subject to `E₀N ≥ γ`),
Moustakides 1986 (Page's CUSUM exactly optimal), Wald & Wolfowitz 1948 — and
**transport does not use it.**

**Negative search** (`literature-crosscheck.md:2195-2200`): *"no published
application of SPRT or quickest-change detection to TCP timeouts or transport
loss detection `[searched; not found]`. CUSUM appears in network security
anomaly detection, not in loss recovery."*

**Strength: WEAKEST OF THE THREE.** No query strings, no venues, no index
names — just `[searched; not found]`. **§2.4 and §7 both flag this; the paper
must re-run and document this search at claim 2.2's standard before submission.**

**The bridge that makes it concrete, and it is already half-published.** RACK's
own design budget — *"bound such spurious recoveries to approximately once
every 16 recoveries (**less than 7 %**)"* — **is a false-alarm rate, i.e.
Lorden's γ, chosen by hand.** RFC 8985 is already reasoning in this framework
without naming it.

**The concrete instantiation — `cost-ratio-memo.md` option (d).** The memo
derives the recovery clock `W(α) = srtt + k(α)·σ`, `k(α) = √((1−α)/α)`, and
prices α by equating marginal costs:

```text
    L(α) = α·ν + δ·p·k(α)·σ/d      ⇒      α^{3/2}·(1−α)^{1/2} = δ·p·σ / (2·ν·d)
```

*"No fitted coefficient appears anywhere. The equation closes."*
(`cost-ratio-memo.md:383`.) And the closure condition `ν ≥ δ·p·σ/(0.6495·d)`
(from `max_α α^{3/2}(1−α)^{1/2} = 3√3/16 = 0.3248` at α = 0.75) makes options
(a), (b) and (c) **three points on (d)'s curve** rather than alternatives to it.

> **⚠ A FRAMING WARNING THE PAPER MUST HONOUR.** *"(d) is the concrete
> instantiation of the sequential-detection claim"* **is a synthesis this
> skeleton is proposing across two documents. It is asserted nowhere in the
> record.** `cost-ratio-memo.md` contains **zero** occurrences of Lorden,
> CUSUM, SPRT, quickest-change or CD-6. And structurally, (d) minimises an
> expected-loss functional — it is **not a sequential stopping rule in
> Lorden's sense**: no CUSUM statistic, no `log γ / I` detection-delay bound.
> The paper must present the link as **new framing it introduces**, not as a
> result it inherits.

**Honest scope, from CD-6 itself:** *"the least immediately actionable. Nothing
to adopt today."* The claim is conditional — *"a derived recovery clock in this
framework **would be** a genuine contribution rather than a re-derivation."*

### 2.4 The negative searches are not of equal strength, and the paper says so

A short subsection, because a reviewer will find this in ten minutes if we do
not:

| claim | query strings recorded? | source set named? | rating |
|---|---|---|---|
| 2.2 nested delay-CC | **yes** — 6 queries, 2 indexes, 1 full-text | yes | **STRONG** (self-rated *"not prominent or well-cited"*) |
| 2.1 span decomposition | no | **yes** — five named sources | **MEDIUM** |
| 2.3 sequential detection | **no** | no | **WEAK — must be re-run before submission** |

**And a methodological footnote that belongs in the paper, not in a
`.gitignore`d note** (`literature-crosscheck.md:2542-2548`): the fetch
summarisers used by two research passes *"refused RFC 6675 outright and
silently paraphrased elsewhere … A literature cross-check conducted through a
summariser would have produced plausible, wrong quotations for at least three
of the constants above."* Every quotation that matters was obtained by direct
text extraction of a primary PDF or RFC plaintext. **That is a reproducibility
statement about doing literature review with LLM tooling, and it is worth
publishing on its own.**

---

## 3. THE REFUTATION RECORD AS A CONTRIBUTION

**"Negative results from disciplined transfer."** Three refutations, each a
different *failure mode of borrowing*, and the section's argument is that the
three together are worth more than the two flips.

### 3.1 The N² postmortem — *a law can be exhaustively tested and still be wrong*

**The defect.** `path_scaled_store_cap = clamp(gain·N·Σ, floor, N·knee)`
(`net/mod.rs:2458-2487`). At a symmetric cell `Σ = N·a`, so the value is
**`gain·N²·a` — quadratic — under a linear ceiling.** Its own doc comment
described a **linear** quantity. The `× N` term had **no provenance in the
repository** and had **never** been swept.

**Why every test passed.** The saturation condition `gain·N·Σ ≥ N·knee ⟺
Σ ≥ knee/gain` — **the N cancels.** `knee/gain = 1024` against measured anchors
**1635 (c7)** and **1510 (c8)**, i.e. **1.6× and 1.5×** the threshold. So the
wire read `occcap_p50` = exactly **4096** in **121 of 126 dual reps across five
independent sessions**. *The clamp ate the evidence.* Nine always-on absolute
pins, two component benches, an engine-equivalence pin and an L1 gauge, for a
month; the verification matrix called it *"the most thoroughly instrumented law
in the pipeline"*. **Every pin passed. They were all asserting that the code
computes the model; none asked whether the model was right.**

**How it was found:** *"in minutes, on the first read that treated it as a
formula rather than as an arm."*

**The five mechanisms, one root** (ADR-0070:212-220): the clamp ate the
evidence; `N ∈ {1,2}` was the entire test universe (the exponent is
distinguishable only at N ≥ 3, and **no cell, bench geometry or L1 arm had ever
run three paths**); two defects masked each other (the anchor over-read
**×4.6–7.4** while the multiplier over-scaled by N); the pinning was measured
and **misfiled** as a fact about c7/c8 rather than about the formula; and — the
root — **the formula was never reviewed as a formula.**

**THE PREVENTION KIT IS THE CONTRIBUTION, and it landed as code and binding
rules, not prose** (ADR-0070:222-228): (1) always-on law-shape property tests,
sweeping `N = 1…8` synthetically, **with the unclamped formula tested
separately from its clamp** — *"a clamp may never be the only thing making a
law sane"* (`src/net/mod.rs:10550`); (2) a bind-fraction gauge on every clamp,
with the standing sentence *"this law operates as a constant"*
(`tools/l1/ladder_report.py:455`); (3) N ≥ 3 coverage — the `c7x4` symmetric
quad (`tests/store_cap_sf_bench.rs:89`); (4) formula-first review; (5)
degeneracy is a red alert. Items 4–5 became MEASUREMENT DISCIPLINE 17 and 18.

**Damage priced:** the pinned 4096 arm is **×7.57** the cell's own resequencing
span, read **−19.6 %**. **Repair:** deleting `×N` moved the law INTERIOR at both
duals for the first time — `gain·Σ` = **3270 (c7)**, **3020 (c8)** against 4096.

**The transferable lesson:** *exhaustive pinning and law verification are
different activities, and the first can run for a month while the second has
never happened.*

### 3.2 The RACK transplant — *the construction the backlog named does not exist in the source*

**What was attempted.** Tier-2 item 2.1: replace `round = (2·srtt).clamp(25 ms,
100 ms)` with RACK's relative bounds.

**The specification failure** (§16.68, `fec-arq-model.md:14055-14076`). Our two
clocks are **re-probe cadences**: neither decides that a symbol is lost. Their
RACK counterpart is therefore §7.2's PTO — and **§7.2's PTO has no RTT-relative
ceiling.** Its only bound is `TCP_RTO_expiration()`, whose RFC 6298 definition
carries a **1-second minimum**: an absolute constant an *order of magnitude
larger* than the 100 ms one we were replacing, from a different RFC. And the one
relatively-bounded expression RACK does publish (§6.2 Step 4) is bounded by
`SRTT` only *because its base is `min_RTT/4`*; transplanting that bound onto a
`2·SRTT` base is **arithmetically vacuous** — `min(2·srtt, srtt) ≡ srtt`.

> *"The cross-check's item 2.1 asked for a construction its own cited source
> does not contain, and this section records that as an error in the backlog
> rather than closing it."*

**The faithful transplant was built anyway, and measured, so the refutation is
reproducible.** It runs **8–46× tighter** than the clamp it replaces (1–10 ms
against 25–100 ms at `mult = 1`). RACK's own upper bound is **unreachable
within RACK's own multiplier range at four of five cells** (the ceiling binds
only at `mult ≥ 18, 32, 32, 40, 47`, against RACK's maximum of **17**). And the
adaptive half is **structurally inert**: `mult` advances on DSACK-detected
spurious recoveries, and *"this transport has no DSACK and no spurious-recovery
detector."* Measured: no arm clears RACK's own `α_class` = 6.25 % at any cell
(**0.17–0.78**), and at `mult = 1` the `SRTT` ceiling bound **0 times in 108 847
evaluations**.

> **The general lesson, verbatim:** *"transplanting its static half means
> shipping one third of RACK under RACK's name."*

**AND THE INSTRUMENT CONVICTED THE INCUMBENT.** The same battery measured the
*shipped* clamp binding **92.4–99.7 %** of the time at every cell and violating
RACK's own spurious budget by **1.7–12.0×** — *"neither number existed in this
tree before."* **A refuted successor produced the strongest defect finding of
the era, about the law it failed to replace.** That is the section's best
single argument for publishing refutations.

### 3.3 The quantile clock — *a category error in the units of a probability*

**The construction.** `W(α) = srtt + k(α)·σ`, `k(α) = √((1−α)/α)` from
Cantelli's inequality, with `α = target_tail_loss × ζ(hint)`.

**The arithmetic refutation:** with `target_tail_loss = 1e-5`, `k` is **316 at
Auto** and **3 162 at Realtime** — `W` = **3.24 s** and **31.7 s** against a
100 ms shipped clamp. And the empirical route fails on the same number: a
`1 − 10⁻⁵` quantile needs of order **10⁵** independent samples, while the Copa
RTT store is a MIN-deque that *"does not retain the upper tail at all, by
construction."*

**THE CATEGORY ERROR, which is the actual contribution** (`:14357-14367`):

> `target_tail_loss` is the probability that a symbol is **never delivered**.
> `α` is the probability that a **retransmit is wasted**. **These are not the
> same failure and they do not cost the same** … Pricing the RATE of the second
> from the tolerated rate of the first asserts that they cost the same.

**And then the record corrected itself, which is the part worth publishing.**
§16.69 concluded the required price ratio *"exists nowhere in this repository
and has no published value."* The cost-ratio memo (2026-08-19) showed that
**too strong**: it does not exist on the **r** leg, but exists on the **δ** leg
twice — Copa's `U = log(throughput) − δ·log(delay)` and `D(δ)`. *"The category
error §16.69 identified is real; its conclusion that no price exists anywhere
is not."* **A refutation that was itself partially refuted, on the record, by
the same discipline.**

### 3.4 The through-line

Three different failure modes of *disciplined transfer*: **3.1** — we
transferred nothing and got it wrong anyway, because a formula went unread;
**3.2** — we transferred a construction the source does not contain, and the
error was in the *backlog item*, not the implementation; **3.3** — we
transferred a number across a unit boundary it does not cross, and then found
our own denial of a price was wrong. **The claim of the section is that a tree
that records these in a ledger, with verdicts and scopes, learns faster than
one that quietly reverts them.**

---

## 4. THE METHOD — the discipline stack as a reproducibility story

**The section's job:** describe the machine that produced §3, in enough detail
that another group could run it. Five layers, bottom-up.

**4.1 MEASUREMENT DISCIPLINE — 18 numbered rules** (`goal-gate.md:15-338`;
ADR-0052), each dated and each traceable to the battery that forced it. Origin
(`:19-23`): *"six mechanism verdicts were merged on measurements in which the
mechanism under test never executed, because nobody checked."* Rule 1 is
**mechanism-liveness proof** — *"Dead code measures noise."* Rules 17 and 18
were both added on 2026-08-12 by the N² postmortem. **The rules are not a style
guide; they are a merge gate.**

**4.2 FORMULA-FIRST** (`CLAUDE.md`; ADR-0070). *"No law ships without its
formula and its derivation IN THE PAPER, before the code."* Design review
presents the **formula, not the diff**, and checks agreement in **SHAPE** —
order in N, units, monotonicity — before any number. Every clamp gets a
bind-fraction gauge. Sections that did this before their code existed: §16.56,
§16.58, §16.59, §16.62, §16.67, §16.68, §16.69.

**4.3 PRE-REGISTRATION** (MEASUREMENT DISCIPLINE 11, `goal-gate.md:74-81`).
The contract — mechanism, predicted effect size and cells, falsification
condition — is committed **in its own commit, before the drivers exist and
before any VM contact**; results are then *"stated against the criteria
pre-registered at `<sha>`, never against a number chosen after the fact"*, and
the contract is never edited. Two standing rules make it bite: **a build whose
prediction fails defaults to the deprecation register, not to iteration**; and
**no battery flips its own default** — a flip is a separate trivial commit
citing the results block.

**The strongest evidence that it is real, and the paper should lead with it:
pre-registration has produced UNSCORED results twice.** The Dead-Wall battery
(`:28586`) and the Mode-Hunt battery (`:29077`) both fired their pre-written
stop rules and were recorded as **UNSCORED** rather than re-cut. Mode-Hunt
missed its own bar by **9/26 = 0.346 against 0.40**. A discipline that never
produces a null is not a discipline.

**A second piece of evidence, and it is unusual enough to name:** B-WALL's
reporter was **corrected against itself in the direction of a weaker claim**.
It had applied the ≥ 8 power test per (pool, seed) — *stricter* than the
contract wrote — which *"would have closed this clause NEEDS-MORE"*; the
reporter was corrected *"to transcribe [the contract] rather than to invent a
stricter one"*, and **both readings are printed** (goal-gate `:32834`).

**4.4 COMPOSITION LADDERS.** A battery is not an A/B; it is a ladder of rungs
on **one binary** — control → one factor changed → another → their composition
→ all. The composition rung is a *factorisation test*, not a third law: *"the
cap axis and the recovery-clock axis are disjoint seats … so **DR is the
factorisation test, not a third law**"* — and it **factorised**: DR's cap is
D's to **0.1 % at c7 and 4.5 % at c8**. Arms are interleaved **round-robin per
rep**, which is what made the paired within-rep design of §5 possible at all.

**4.5 ERA BATTERIES.** *"ONE binary for every arm, every cell and both seeds …
sha256 recorded in every ledger header. **An arm built from a different tree is
not a rung of this ladder.**"* Every ledger line carries its era:

```text
=== CONTRACT goal-gate "Candidates Battery — PRE-REGISTRATION" (commit 6bd5299),
    era main@0055c5d, ONE binary all arms
```

Two eras have run: the **Ladder Battery** (720 L1 invocations / 529 live, era
main@`5ddf7f6`) and the **Candidates Battery** (596 / 452 live across four
pools, era main@`0055c5d`).

**4.6 The honest cost.** This stack produced, in one era, **two parity flips and
three refutations**. A paper claiming the method is cheap would be lying; the
claim is that it is *cheaper than the N² defect it exists to prevent*, and the
N² defect ran undetected for a month under nine always-on pins.

---

## 5. WHAT THE ERA BATTERY WILL ADD — **PENDING ITS RUN**

**MARKED PENDING THROUGHOUT. This section has no results and the paper cannot
be submitted with it as written.**

**The measurement:** the cumulative effect of an era's flips, measured
together rather than one gate at a time. Every rung so far scored **one factor
against a same-session control**; nothing has scored *the machine you get when
all of an era's derived laws are on at once* against the machine at the era's
start.

**The pre-registration is already written, before the cell it needs exists** —
`goal-gate.md:33117-33153`, *"c9 PRE-REGISTRATION (written 2026-08-19, BEFORE
c9 exists)"*, standing under ERA LEDGER item 5. Its four clauses, with the
predicted numbers already committed:

| clause | prediction |
|---|---|
| C9-1 | ρ̄ **negative**, point **−0.27**, band **[−0.34, −0.15]** |
| C9-2 | `B(4, −0.27) = 0.782` against c7's measured `B(2, −0.814) = 0.695` |
| C9-3 | heterogeneous quad ρ̄ **≥ +0.3**; `B(4, +0.3) = 0.311` against the √N law's 0.500 |
| C9-4 | the exogeneity bar — *"the single most informative measurement in this pre-registration and it costs one extra arm"* |

with `B(N, ρ̄) = 1 − √((1 + (N−1)·ρ̄)/N)`, floor `ρ̄ = −1/(N−1)`, and c7 measured
at **−0.814** — **81.4 % of the way to its own floor.**

**TWO BLOCKING DEPENDENCIES, and the paper must state them as blockers.**

1. **`c9` does not exist.** The only N = 4 geometry in the tree is `c7x4`, a
   bench cell.
2. **A named harness defect.** `topo_dual.sh` takes ONE `--seed` and passes it
   to both legs, so **at every symmetric cell in this tree the two paths' loss
   processes are the same realization** — ρ_loss is **+1 by construction**, and
   **no arm ever run can measure loss-side correlation in either direction.**
   Plus the `[ACKDIAG]` cadence must go to 250 ms (8× finer) — *"recorded here
   as a blocking dependency, not as a nice-to-have."*

**What this section will claim if it runs:** that an era's derived laws compose
without interaction, at N ≥ 4, on a cell no rung has seen. **What it claims
today: nothing.**

---

## 6. WHAT THE PAPER MUST NOT CLAIM

A standing list, so a later draft cannot quietly acquire these.

1. **Not "faster."** Both shipped flips are parity. §16.71's own word is *"free"*.
2. **Not "zero fitted constants."** Corrected tree-wide (Tier 0, `46c3e6a`):
   `9/8` is a *cited empirical recommendation*, not a derivation, and RACK uses
   5/4 for the same job.
3. **Not "the MPTCP literature supports our span term's shape."** It supports
   its magnitude only, at **2× ours** (§2.1).
4. **Not "the dead wall is fixed."** B-WALL is a **direction** (18/23,
   p ≈ 0.011); c8L is direction-only and scored on nothing; the clock that sets
   the wall's quantum is untouched and binds 92.4–99.7 % of the time. See
   `dead-wall-mitigations.md` §0.1.
5. **Not "tagging trivialises data association."** *"No published sentence
   asserting that was found"* — the strong claim is folklore.
6. **Not "the pooled cap beats per-path accounts because of Eppen."** §16.72 is
   **PARTIAL**: the ordering is real (p = 0.009) but CD-5 *"named the WRONG
   SERIES and guessed the WRONG DIRECTION"*, and the exogeneity Eppen's theorem
   requires is **unverified** — at c8 the shared pool is a candidate *cause* of
   the correlation rather than a victim of it.
7. **Not "(d) is a sequential-detection result."** See §2.3's framing warning.

---

## 7. THE NUMBERS I WOULD MOST WANT CHECKED

Ranked. **#1 is the one to check first.**

1. **`δ = 0.4838` — ~~the claimed 3 % agreement with the contract's
   `δ_auto = 0.5`~~ — SUPERSEDED BY MEASUREMENT (2026-08-19). The measured
   value is `δ = 45`.**

   **THE MEASUREMENT, AND IT IS NOW THE ROW'S HEADLINE.** σ has been read off
   the field at c8 — `sig_us=` in the per-path `[DIAG]` line, last emission per
   path, 3 reps, cell **c8 = `c2 c3` dual**, **25 MB**, **seed 42**, shipped
   defaults, **0 aborts**, binary sha256 `330ebfcc…` (goal-gate *"THE MEASURED
   σ AT c8 — THE SCORED RESULT"*). The **data path** (identified by sample
   count, `btlbw` and symbols handed — it carries 96–99.8 % of the bytes) reads

   ```text
       σ(c8, data path)  =  620 / 853 / 8 550 µs   at n = 24 661 / 24 445 / 24 421
       median  σ  =  0.853 ms
   ```

   which, with the measured `ν(c8) = 0.0438`, gives

   ```text
       δ(σ = 0.853 ms, ν = 0.0438)  =  38.4 / 0.853  =  45.0     vs  δ_auto = 0.5
   ```

   **The prediction written below — that the measurement would KILL the
   agreement rather than confirm it — is CONFIRMED**, against a band
   pre-committed before the run (`δ ∈ [0.4, 0.6]`, i.e. `σ ∈ [64, 96] ms`).
   Every rep fails independently: even the most generous single reading
   (8.55 ms) gives `δ = 4.49`, still 7.5× above the band, and the slow leg's
   19.7 ms gives `δ = 1.95`. **No path in the cell restores the agreement.**

   **NEITHER `0.4838` NOR `2.12` MAY BE QUOTED.** The quotable number is `δ =
   45` at c8, with the provenance above attached to it, and with the standing
   caveat that this is **one cell, one seed, three reps** — the full-cell σ pass
   is the named successor. The prose below is preserved as the derivation that
   the measurement was written against, and its ν and σ discussion now reads as
   history rather than as an open question.

   *What the number was.* `cost-ratio-memo.md:420-435` calls it *"the most
   interesting number in this document"*, and it was the paper's single most
   quotable result: two independently-derived recovery-clock options agreeing
   at Auto to 3 %, apparently validating the transfer of Copa's δ to the
   recovery plane.

   *Why it is no longer quotable as stated.* It was never a measurement. It is
   an INVERSION of option (d)'s stationarity condition, and it therefore
   carries every input of that condition. Written out — which the memo did not
   do, and which is the repair — invert
   `α^{3/2}(1−α)^{1/2} = δ·p·σ/(2·ν·d)` at option (b)'s Auto point `α_b`:

   ```text
                2 · ν · d · α_b^{3/2} · (1 − α_b)^{1/2}
       δ(σ, ν) = ───────────────────────────────────────
                                p · σ
   ```

   **δ IS INVERSELY PROPORTIONAL TO σ AND LINEAR IN ν.** Every symbol on the
   right except σ and ν is on the record: `α_b = 0.1843` (option (b)'s Auto
   point at c8), `p = 0.0126` (realized per-path loss), `d = 77 ms`
   (`srtt_wire` at c8; only the ratio `d/σ` matters, so the unit cancels).
   Collecting them:

   ```text
       δ(σ, ν)  =  0.484 · (18.1 ms / σ) · (ν / 0.01)
   ```

   (The expression evaluates to **0.483** at the memo's own inputs, against the
   memo's **0.4838** — a 0.3 % residual from `α_b`'s rounding, which is the
   check that this really is the memo's number re-derived and not a new one.)

   The `0.4838` in the memo is that expression evaluated at **σ = 18.1 ms** —
   itself an estimate obtained by inverting Cantelli across two different
   clocks, reported by the memo as a point value while being a lower bound —
   and at **ν = 0.01**, which the memo picked precisely BECAUSE it reproduced
   the agreement. *"A 3 % agreement computed from a σ that may be 1.8× wrong is
   not a 3 % agreement"*, and one computed at a ν chosen to produce it is not
   an agreement at all until ν is measured.

   *What has changed since.* **ν IS NOW MEASURED**, off committed ledgers and
   with no VM run: `tools/l1/nu_measure.py` over the Candidates Battery's 477
   usable records reads **ν(c8) = 0.0438** — `fired` (the `[RACK]` line's
   `fa=` denominator) over `dgq_hand` (symbols handed to the wire), both from
   the same record and the same site. That is **4.5× the ν = 0.01 the
   agreement was computed at**, and it moves δ the same way:

   ```text
       δ(σ = 18.1 ms, ν = 0.0438)  =  2.12        vs  δ_auto = 0.5
   ```

   *σ REMAINS A SYMBOL, and no value is invented here.* What can be stated is
   the FALSIFIABLE PREDICTION the measurement will settle. Solving `δ(σ, ν) =
   δ_auto = 0.5` at the measured ν gives the σ that would restore the
   agreement:

   ```text
       σ_required  =  18.1 ms · (0.484 / 0.5) · 4.38  ≈  77 ms   ≈  d
   ```

   **A σ equal to the srtt it is the dispersion of is not a plausible reading**
   — so the honest expectation is that the measurement kills the agreement
   rather than confirming it. That is a prediction, it is written before the
   measurement, and it is the reason the row is marked SUPERSEDED rather than
   deleted: the number's FATE is now decided by one field.

   *The prerequisite is discharged.* `rtt_sigma_us()` — the engine's own
   comment described it as *"Fed unconditionally; read by nothing"* — is now
   reported per path in the `[DIAG]` line as `sig_us=<µs>/n<count>`, with the
   EWMA's sample count beside it as warm-up evidence
   (`tests/sigma_diag_reachability.rs`). ~~**What remains is one L1 run to read
   σ at c8, and this row is rewritten from a measurement instead of an
   inversion.**~~ **THAT RUN HAS BEEN TAKEN** — see the measurement at the head
   of this row. Neither 0.4838 nor 2.12 may be quoted: both are `δ(σ, ν)` at a
   σ that has now been measured and is neither of the values they assume.

2. **`slack/window ≡ 2.125` in 833 of 833 evaluations** (§16.57, row 1.4). By
   MEASUREMENT DISCIPLINE 18 this reads as a **degeneracy**, not a validation —
   `833/833` at ρ = 1 is what a law that reduces to a constant looks like.
   Confirm it is reported as a bind fraction with the standing sentence, or the
   row is the N² defect in a second law.

3. **`w*L ≈ 1.33` at c8** (`dead-wall-mitigations.md` §4). **[NOT ON THE
   RECORD]** — my own division of two committed medians (retx 1.412 / tc_drop
   1.064), performed nowhere in the tree. Do not cite it until an L1 pass
   defines the denominator and re-derives it across cells.

4. **`knee/gain = 1024` against anchors 1635 / 1510** (§3.1). The whole N²
   postmortem turns on the clamp being reached at both duals. It is well
   sourced; it is also the load-bearing arithmetic of the paper's best story,
   so it should be re-derived from the anchor table independently.

5. **`108 847` evaluations with the SRTT ceiling binding 0 times** (§3.2).
   Cheap to re-check, and it is the sentence a hostile reviewer will pick.

---

## 8. Assembly order (suggested)

§3 → §4 → §1 → §2 → §5 → §0. The refutation record is the strongest material
and should be drafted first; §1 is the easiest and should not be, because
drafting it first tempts the author into a highlight reel that omits row 1.7.
