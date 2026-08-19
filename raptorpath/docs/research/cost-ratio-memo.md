# The cost ratio: a decision memo on the one number that unlocks the quantile clock

**Status: DECISION REQUESTED. No recommendation is made in this document.**
Date 2026-08-19. Branch `docs/cost-ratio-memo` from main@`6ad964d`. Docs only —
no code, no gate, no default is touched.

**What this is.** Paper §16.69 wrote the derived recovery clock
`W(α) = srtt + k(α)·σ`, `k(α) = √((1−α)/α)`, and refuted it three ways. Two of
the three refutations are arithmetic and follow from the value of one input.
The third is a category error, and §16.69 says so:

> "`target_tail_loss` is the probability that a symbol is never delivered. `α`
> is the probability that a retransmit is wasted. **These are not the same
> failure and they do not cost the same** … Making the mapping honest requires
> the RATIO of the two costs — *how many wasted symbols is one unrecovered
> symbol worth?* — and **that ratio exists nowhere in this repository and has
> no published value.** It is exactly the invented constant this section is
> forbidden to ship."

That ratio is a product decision about what the protocol values. This memo
lays out the options, computes what each one does to the shipped numbers, and
stops. It also reports one finding that narrows the decision considerably, and
one that widens it.

---

## 0. THE TWO FINDINGS, up front

**FINDING A — the ratio is not, in fact, absent from the repository. The
contract already declares a bandwidth-versus-latency price, twice, and both
declarations are continuous in the dial.**

* `scheduler/mod.rs:119-132` — Copa's utility is `U = log(throughput) −
  δ·log(delay)`, and the doc comment on that seat says it in these words:
  *"**δ IS the marginal latency price**"*. `δ(hint) = COPA_DELTA / ζ(hint)` =
  **50 / 0.5 / 0.005** at Realtime / Auto / Bulk (`COPA_DELTA = 0.5` at
  `scheduler/mod.rs:47`).
* `net/mod.rs:1013-1015` + `net/mod.rs:3420-3426` — the δ leg declares a
  latency BUDGET in round trips: `D(δ) = min(b(δ)·RTprop, 2·RTprop)` with
  `b` = **½ / 1 / 2** at Realtime / Auto / Bulk.

§16.69's sentence is exactly right that the ratio does not exist **on the r
leg**, where it went looking. It exists on the **δ leg**, which is where a
price on latency belongs. What the user must decide is therefore *not* "invent
a number" but the far narrower question: **does the price the contract already
pays for latency in the congestion controller also govern the recovery clock?**
That is a transfer claim, not a measurement, and it is the decision this memo
presents.

**FINDING B — σ is already computed on every arm and reported on none, and the
shipped record can be inverted to estimate it for free.**

`scheduler/mod.rs:1148,1657-1660` maintains `rtt_var_sq = 0.75·rtt_var_sq +
0.25·dev²` — RFC 6298's own `β = 1/4` on the squared deviation — and
`rtt_sigma_us()` (`scheduler/mod.rs:3032-3037`) reads it. Its only consumers
sit behind `RWM_QUANTILE_CLOCKS`, default OFF. The engine's own comment says
it: *"Fed unconditionally; read by nothing on the default arm."* **The one
measured input every option below needs already exists in the shipped binary
and has never been printed.** §16.69's working value `σ ≈ 10 ms` at c8 was an
assumption; it did not have to be.

---

## 1. THE QUESTION, in one paragraph and in plain language

When a recovery clock expires, the transport is making a guess. It has not
seen the acknowledgement, and it does not know whether that is because the
data was lost or because the acknowledgement is merely slow. If it guesses
"lost" and retransmits, and the original was in fact about to arrive, it has
spent one symbol of bandwidth for nothing — that symbol came out of the same
wire the repair plane is already spending on redundancy (the **r** leg). If it
guesses "slow" and waits longer, and the data really was lost, then every
byte behind it in the stream is delayed by exactly how long it waited — that
delay comes out of the delivery latency the contract is written to protect
(the **δ** leg). **How should the protocol weigh those two mistakes against
each other?** Once that weight is a number, the clock is fully determined:
the false-alarm rate `α` follows from it, `k(α) = √((1−α)/α)` follows from
Cantelli, and `W = srtt + k(α)·σ` follows from that, with no fitted constant
anywhere in the chain.

---

## 2. WHAT THE MEASURED RECORD ALREADY SAYS

Everything in this section is transcribed from the Candidates Battery RESULTS
(`goal-gate.md`, "Candidates Battery — RESULTS", 596 L1 invocations / 452
live, one binary `0f6069da…9dd395e7`) and from §16.68's component bench
(`raptorpath/tests/recovery_bench.rs:535-542`). Nothing here is new
measurement.

### 2.1 The shipped clock, measured

| cell | shipped `legacy_pin` | `legacy_ms` | arm-A `fa_frac` | × RACK's `α_class` = 6.25 % |
|---|---|---|---|---|
| c1 | 0.9242 | 25.39 | 0.1066 | **1.7×** |
| c7 | 0.9592 | 99.09 | 0.2385 | **3.8×** |
| c8 | 0.9660 | 99.06 | 0.4014 | **6.4×** |
| c8L | 0.9968 | 99.89 | 0.2591 | **4.1×** |
| sc2 | 0.9948 | 99.81 | 0.7516 | **12.0×** |

`round = (2·srtt).clamp(25 ms, 100 ms)` binds 92.4–99.7 % of the time: the
shipped law is, in practice, the constant 25 ms at c1 and the constant 100 ms
everywhere else, and it violates RACK's own published spurious budget at all
five cells.

### 2.2 The two clocks, and which one any of this is about

The false-alarm statistic is a **sender-site** quantity by construction: the
receiver's `[RACK]` gauge never calls `record_fire`
(`net/receiver.rs:209,771-780`; pre-registration instrument fact 5). But the
sender's clock is the **app-echo** RTT, which includes the store dwell, and
the receiver's is the **wire** RTT. They are different numbers:

| cell | `srtt_app` (sender) | `srtt_wire` (receiver) | shipped cadence | cadence − `srtt_app` | cadence − `srtt_wire` |
|---|---|---|---|---|---|
| c1 | 9 | 2 | 25.39 | **+16.4** | +23.4 |
| c7 | 87 | 72 | 99.09 | **+12.1** | +27.1 |
| c8 | 376 | 77 | 99.06 | **−277.0** | +22.1 |
| c8L | 464 | 82 | 99.89 | **−364.1** | +17.9 |
| sc2 | 104 | 101 | 99.81 | **−4.2** | **−1.2** |

(ms; `srtt_app = RTprop + standing queue` and `srtt_wire = ` loaded ICMP p50,
both from `recovery_bench.rs`'s `MEASURED` table.)

**This table dominates every option below and it must be read before any of
them.** `W(α) = srtt + k(α)·σ` is `srtt` plus a non-negative margin, so
`W ≥ srtt` for every α. At the **sender** site at c8 that means `W ≥ 376 ms`
under *any* cost ratio whatsoever — 3.8× the shipped clamp — because the
quantity the sender's clock waits on is not a network quantity, it is the
sender's own store dwell. **No choice of α makes the sender's quantile clock
resemble the shipped one.** The cost-ratio decision governs the *margin* term
and therefore governs the **receiver** site cleanly; at the sender site the
dominant term is the dwell, which is the δ-cap's territory, not the clock's.
§16.68 said this in prose (*"the clamp is not the disease; the dwell-inclusive
clock is"*) and §16.70 measured it. It is stated here because a reader could
otherwise take the c8 numbers below as promises about the sender.

**And it explains the worst cell without reference to any cost ratio.** At sc2
the shipped 100 ms ceiling sits **1.2 ms below sc2's own wire `srtt`** and
4.2 ms below its app-echo `srtt`. A clock set below the mean of the thing it
is waiting for is expected to be wrong more than half the time; sc2 measures
0.7516. **Any option in this memo strictly improves sc2, at any α, because
`W ≥ srtt` is structural.** sc2's 12× violation — the worst reading in the
battery — is not a cost-ratio question at all.

### 2.3 σ, estimated by inverting the shipped record

Cantelli says `P(X − μ ≥ k·σ) ≤ 1/(1+k²)`. Running that backwards on the
measured `fa_frac` at the measured cadence gives the σ that makes the bound
exactly reproduce what the wire returned — i.e. the smallest σ consistent with
the measurement:

| cell | `srtt_wire` | cadence | margin `M` | `k` = √(1/fa − 1) | **σ = M/k** |
|---|---|---|---|---|---|
| c1 | 2 | 25.39 | 23.39 | 2.895 | **8.1 ms** |
| c7 | 72 | 99.09 | 27.09 | 1.787 | **15.2 ms** |
| c8 | 77 | 99.06 | 22.06 | 1.221 | **18.1 ms** |
| c8L | 82 | 99.89 | 17.89 | 1.691 | **10.6 ms** |
| sc2 | 101 | 99.81 | **−1.19** | — | **vacuous** (§2.2) |

**THIS IS AN ESTIMATE AND NOT A MEASUREMENT, and three things are wrong with
it, named rather than buried.** (i) It pairs a receiver-site clock with a
sender-site statistic — the repair is to report `fa` and σ at the same site,
which is a print statement. (ii) `fa_frac` is `spurious/fired`, not
`P(X > W)` per evaluation; the denominators differ and the two coincide only
if every fire is a fresh independent trial. (iii) Cantelli is an upper bound,
so this inverts an inequality and yields a *lower* bound on σ, reported here as
a point value. **It is used below only to put the options on a common scale
and to show their ORDERING; it is not evidence about any option's absolute
correctness, and FINDING B says the real number is one print line away.**
§16.69's assumed `σ ≈ 10 ms` at c8 sits ~45 % below this estimate.

For orientation, `W` at c8 (`srtt_wire = 77 ms`) across α:

| α | `k(α)` | `W` at σ = 10 ms (§16.69's value) | `W` at σ = 18.1 ms (§2.3's estimate) |
|---|---|---|---|
| 1e-7 (Realtime, §16.69) | 3 162 | 31.7 s | 57.2 s |
| 1e-5 (Auto, §16.69) | 316 | 3.24 s | 5.79 s |
| 1e-3 (Bulk, §16.69) | 31.6 | 393 ms | 648 ms |
| 0.01 | 9.95 | 176 ms | 257 ms |
| **0.0625 (RACK)** | **3.873** | **116 ms** | **147 ms** |
| 0.10 | 3.000 | 107 ms | 131 ms |
| 0.20 | 2.000 | 97 ms | 113 ms |
| 0.40 | 1.225 | 89 ms | 99 ms |
| 0.50 | 1.000 | 87 ms | 95 ms |

The first three rows are §16.69's refutation, reproduced. Everything below
them is what a *priced* α buys.

---

## 3. THE OPTIONS

Four constructions. Each states its `α(r, δ)` mapping, what `W` it yields,
what it costs, and where it breaks. **All four are continuous in the dials by
construction** — no branch on a hint, no threshold on δ or ρ, per CLAUDE.md's
no-mode-switch invariant. Where an option's shipped input is currently a
three-arm `match` on the hint (`delta_budget_b`, `net/mod.rs:3420-3426`), that
is a named-points table and its consumer law is continuous and monotone in it;
a build would express it through `bulkness_of_delta(δ)` the way the wasm rate
law already does (`raptorpath-wasm/src/lib.rs:234-238`), and that is noted in
each option's build path rather than repeated four times.

### (a) BANDWIDTH-CONSERVATIVE — α tied to the repair budget's headroom

**The mapping.** The contract already declares that it will spend a fraction
`r` of the wire on symbols that will most likely turn out to be unnecessary.
A spurious retransmit is the same kind of spend. So let the clock waste at the
same rate the code already wastes, and no faster:

```text
  α_bw = r
```

where `r` is the rate law's own live output — continuous in the dial by
construction, since `r` is continuous in δ through the shipped rate law
(`raptorpath-math/src/lib.rs:618-661`, and the `r(β)` blend at
`raptorpath-wasm/src/lib.rs:864-888`). **No new constant.** A wasted
retransmit costs exactly one symbol on the wire — confirmed at
`net/mod.rs:7930-7944`, which clones one symbol from `sent_store` or generates
one repair — so `α_bw = r` really is "one wasted symbol per r source symbols",
the same unit the repair budget is already denominated in.

**What it yields at c8** (σ = 18.1 ms, `srtt_wire` = 77 ms):

| live `r` | `k` | `W` at c8 | comment |
|---|---|---|---|
| 0.05 | 4.359 | **156 ms** | thrifty; +57 ms over the shipped clamp |
| 0.10 | 3.000 | **131 ms** | +32 ms |
| 0.20 | 2.000 | **113 ms** | +14 ms |
| 0.50 (`max_fec_overhead`) | 1.000 | **95 ms** | −4 ms; the hard ceiling |

**Direction on the dial.** Realtime prices a late symbol 100× dearer
(ζ = 0.01), which tightens `effective_tail_loss` and *raises* r; a larger r
means a larger α, a smaller margin and a **faster, wastier** clock. Bulk does
the reverse. **That is the correct direction** and it comes out of the
existing law with nothing added.

**Consequences, and the defect.** The bandwidth cost is bounded by
construction — spurious retransmits can never exceed what FEC already spends,
and never exceed `max_fec_overhead = 0.5` (`config.rs:318`). The latency cost
is the margin: **+14 to +57 ms of added detection delay at c8** across the
plausible r range, and proportionally more at cells with larger σ.

**It degenerates at Bulk, and the degeneracy is total.** Bulk mid-stream sets
`delta_eff = ε̂` and therefore `r* = 0 identically` — pure ARQ, stated at
`raptorpath-math/src/lib.rs:576-581`. At `r = 0`, `α = 0`, `k = ∞`, and
**`W = ∞`: the clock never fires.** Bulk is precisely the hint that still
needs a retransmit clock, since it has no FEC to fall back on. Substituting
the ceiling (`α = max_fec_overhead = 0.5`) removes the degeneracy but also
removes every trace of the dial — α becomes a constant 0.5, 8× RACK's budget,
and neither δ nor r reaches the clock. **Option (a) has no constant-free
instantiation that survives the Bulk end of its own dial.** That is stated
here as the finding it is.

### (b) LATENCY-CONSERVATIVE — α tied to δ's budget

**The mapping.** The δ leg already declares, in milliseconds of the path's own
propagation delay, how much added latency it will tolerate:
`D(δ) = min(b(δ)·RTprop, 2·RTprop)` with `b` = ½ / 1 / 2
(`net/mod.rs:1013-1015`, `:3420-3426`). Spend exactly that, and no more, on
the clock's margin:

```text
  k(α)·σ  =  D(δ)     ⇒     k = D(δ)/σ ,     α = σ² / (σ² + D(δ)²)

  and therefore    W = srtt + D(δ) = srtt + b(δ)·RTprop
```

**No new constant, and note what happened to the law: σ cancels out of `W`
entirely.** Under option (b) the clock is `srtt + b(δ)·RTprop` — two measured
quantities and one contract dial. **α is not an input at all; it is a
reported consequence**, and σ enters only to say what α came out to be. This
is the simplest of the four laws by a wide margin.

**What it yields, at every cell and every named point** (σ from §2.3):

| cell | RTprop | σ | Realtime `D`/α/`W` | Auto `D`/α/`W` | Bulk `D`/α/`W` |
|---|---|---|---|---|---|
| c1 | 2 | 8.1 | 1 ms / 0.985 / 3 ms | 2 ms / 0.942 / 4 ms | 4 ms / 0.803 / 6 ms |
| c7 | 11 | 15.2 | 5.5 ms / 0.884 / 77.5 ms | 11 ms / 0.655 / 83 ms | 22 ms / 0.322 / 94 ms |
| **c8** | 38 | 18.1 | 19 ms / 0.475 / **96 ms** | 38 ms / 0.184 / **115 ms** | 76 ms / **0.054** / **153 ms** |
| c8L | 40 | 10.6 | 20 ms / 0.219 / 102 ms | 40 ms / 0.065 / 122 ms | 80 ms / **0.017** / 162 ms |
| sc2 | 13 | (15, assumed) | 6.5 ms / 0.842 / 107.5 ms | 13 ms / 0.571 / 114 ms | 26 ms / 0.250 / 127 ms |

**Consequences.** At Bulk this is the only option in the memo that **clears
RACK's 6.25 % budget from its own arithmetic** — α = 0.054 at c8 and 0.017 at
c8L — and it does so at a bandwidth cost of nothing beyond what the δ leg has
already declared it will pay in latency. The latency cost at c8 Bulk is
+54 ms of margin over the shipped clamp.

**Where it breaks, and it breaks at two of the three named points.** `D(δ)` is
proportional to `RTprop`, and at short-`RTprop` cells `D` is small relative to
σ: at c1 (`RTprop = 2 ms`) the margin is 1–4 ms against a σ of 8.1 ms, and
α lands at **0.80–0.98** — the clock is told to add essentially nothing and
fires almost always. The same happens at **Realtime everywhere**
(α = 0.22–0.99). Directionally that is correct — Realtime declares latency is
what matters and waste is acceptable — but numerically it means the clock
degenerates to "fire immediately" at Realtime, which is a real behaviour the
decision would be choosing, not a bug to be patched with a scaling constant.
**Introducing a fraction `φ` so that `k·σ = φ·D(δ)` fixes the numbers and is
exactly the invented constant the rules forbid; it is named here and not
taken.**

### (c) THE RACK POINT — α = 6.25 % adopted as a cited engineering default

**The mapping.** `α = α_class = 1/16 = 6.25 %`, RFC 8985 §6.2 Step 4's own
published budget (*"bound such spurious recoveries to approximately once every
16 recoveries (less than 7 %)"*). `k = √15 = 3.873`, flat.

This is the **weakest justification this tree's rules accept** — a cited
empirical default — and §16.67.1 already ruled on what a cited default costs:
it *"ships only with a falsifiable validation of the thing it is empirical
ABOUT."* **That validation now exists as a shipped instrument.** `fa=` /
`fa_frac=` / `fa_class=0.0625` are already printed (`net/mod.rs:4139-4160`,
`RACK_SPURIOUS_BUDGET = 1.0/16.0` at `:4167`) and the battery has already run
them on the control at all five cells. Option (c) is the only option whose
validation bar is not owed.

**What it yields** (σ from §2.3):

| cell | σ | `W` = srtt + 3.873·σ | shipped cadence | Δ vs shipped |
|---|---|---|---|---|
| c1 | 8.1 | **33.3 ms** | 25.39 | +7.9 ms |
| c7 | 15.2 | **130.7 ms** | 99.09 | +31.6 ms |
| c8 | 18.1 | **147.0 ms** | 99.06 | +47.9 ms |
| c8L | 10.6 | **123.0 ms** | 99.89 | +23.1 ms |
| sc2 | (15) | **159.1 ms** | 99.81 | +59.3 ms |

**Consequences.** A uniform +8 to +59 ms of detection delay across the cells,
bought against a claimed spurious rate of 6.25 % where the shipped clock
measures 10.7–75.2 %. Bandwidth cost: whatever a false-alarm rate of 6.25 %
costs, i.e. one wasted symbol per sixteen fires, against the shipped clock's
one per 1.3 (sc2) to one per 9.4 (c1).

**Where it breaks.** α is a constant. It is not a mode switch — there is no
branch, no threshold, and the same number is evaluated at every dial position
— but **neither δ nor r nor ρ reaches the clock at all.** The recovery plane
would be the one plane of the machine that the triangle does not parameterise.
Realtime and Bulk would wait the same number of standard deviations. That is a
coherent position (the false-alarm budget is a property of the *mechanism
class*, not of the application's preferences) and it is TCP's own position, but
it should be chosen knowingly rather than arrived at.

### (d) SYMMETRIC / POWER — α where the marginal costs are equal

**Does the equation close without a new constant? It closes, subject to one
transfer claim and one measurement.** Here is the derivation in full, in the
contract's own currency.

Copa's utility is `U = log(throughput) − δ·log(delay)` (`scheduler/mod.rs:111`),
so per recovery decision the loss in utility is

```text
  L(α)  =  α·ν              wasted-bandwidth term: ν = fires per delivered
                            symbol, each false alarm costs exactly ONE symbol
                            (net/mod.rs:7930-7944), so α·ν is the fraction of
                            wire spent spuriously — a pure fraction, symbol
                            size cancels

        +  δ · p · k(α)·σ / d      latency term: the margin k(α)·σ is added to
                            the detection of genuinely lost data, which is a
                            fraction p of symbols; δ·Δd/d is Copa's own price
                            for a fractional delay increase; d = srtt

  dL/dα = 0,  with  k'(α) = −1 / (2·α^{3/2}·(1−α)^{1/2})  :

  ┌──────────────────────────────────────────────┐
  │   α^{3/2} · (1−α)^{1/2}  =  δ·p·σ / (2·ν·d)  │
  └──────────────────────────────────────────────┘
```

**Provenance of every symbol on the right:**

| symbol | provenance |
|---|---|
| `δ` | **CONTRACT-DECLARED** — `COPA_DELTA/ζ(hint)`, 50 / 0.5 / 0.005, `scheduler/mod.rs:129-132`. Continuous in the dial; the same ζ the rate law already consumes |
| `p` | **measured** — realized per-path loss (c8: 0.0055 / 0.0196, `tools/l1/xpath_loss_replay.py:29-33`) |
| `σ` | **measured, exists today, never reported** — `rtt_sigma_us()`, `scheduler/mod.rs:3032-3037` (FINDING B) |
| `d` | **measured** — `srtt`, unchanged |
| `ν` | **derivable, NOT currently reported** — fires per delivered symbol. `fired` is already counted (`RackClockGauge`, `net/mod.rs:4192-4193`); the delivered-symbol count already exists; their ratio is not printed |

**No fitted coefficient appears anywhere.** The equation closes.

**But it closes on a TRANSFER CLAIM, and that claim is the decision.** δ is
the marginal latency price the contract declares *for the congestion
controller's* throughput-versus-queueing tradeoff. Using it for the recovery
plane's retransmit-versus-detection-delay tradeoff asserts that the protocol
values a millisecond of delivery latency the same on both legs. **That is a
product decision — but it is a much smaller one than §16.69's framing
suggests, because the alternative is not "no number" but "a second, different
latency price", and the burden then falls on justifying why the two differ.**

**What it yields at c8** (δ per hint; `p = 0.0126`, `σ = 18.1 ms`, `d = 77 ms`):

| hint | δ | ν = 0.001 | ν = 0.01 | ν = 0.1 | ν = 1.0 |
|---|---|---|---|---|---|
| Realtime | 50 | corner | corner | corner | α 0.189 / W 114 ms |
| **Auto** | **0.5** | corner | **α 0.189 / W 114 ms** | α 0.038 / W 167 ms | α 0.008 / W 276 ms |
| Bulk | 0.005 | α 0.038 / W 167 ms | α 0.008 / W 276 ms | α 0.0018 / W 507 ms | α 0.0004 / W 1004 ms |

"corner" = no interior stationary point; `L` is monotone decreasing in α and
the optimum is **α → 1: fire immediately, never wait.**

**The closure condition, in closed form.** `max_α α^{3/2}(1−α)^{1/2} =
3√3/16 = 0.3248` at α = 0.75, so an interior optimum exists iff

```text
   ν  ≥  δ·p·σ / (0.6495 · d)
```

At c8 that is **ν ≥ 0.228** (Realtime), **ν ≥ 0.0023** (Auto), **ν ≥ 0.00002**
(Bulk). Realtime's threshold is implausibly high — a clock would have to fire
once per four delivered symbols — so **(d) reduces to a corner at Realtime for
any realistic fire rate, and the corner is "fire immediately", where the
binding constraint becomes option (a)'s bandwidth ceiling.** That is the
reduction the brief asked to be checked, and it lands: **(d) is (a)'s corner at
Realtime, interior at Auto, and the slowest of the four options at Bulk.**

**AND (d) AGREES WITH (b) AT AUTO, WHICH IS THE MOST INTERESTING NUMBER IN
THIS DOCUMENT.** Option (b) at c8 Auto gives α = 0.1843 from the deadline law
`D(δ) = 1·RTprop = 38 ms`. Option (d) at c8 Auto gives α = 0.1889 at
ν = 0.01. Inverting: the δ that makes (d) reproduce (b)'s Auto point at c8 is
**δ = 0.4838** against the contract's actual **δ_auto = 0.5** — a 3 %
disagreement. The tree's two independent declarations of the latency
price — the deadline budget `b(δ)` on the shed law and the Copa price δ in the
utility — **turn out to be mutually consistent at the Auto point**, provided
`ν ≈ 0.0097` fires per delivered symbol.

**That is a falsifiable prediction, and it is the memo's chief technical
residue.** ν is not on the record. If a measurement of ν at c8 lands near
0.01, then (b) and (d) are the same option and there is materially less to
decide than §16.69 implied. If ν lands an order of magnitude away, they
diverge and the choice between them is real. **ν requires no VM arm and no new
law: it is a ratio of two counters that already exist.**

The same inversion for the other options, at c8 and ν = 0.01:

| option | its α at c8 | the δ that reproduces it under (d) | vs. the contract's δ |
|---|---|---|---|
| (c) RACK 6.25 % | 0.0625 | δ = 0.102 | between Auto (0.5) and Bulk (0.005) |
| (a) at r = 0.10 | 0.100 | δ = 0.203 | between Auto and Bulk |
| **(b) at Auto** | **0.184** | **δ = 0.484** | **Auto (0.5), to 3 %** |

**(a), (b) and (c) are not alternatives to (d): they are three points on (d)'s
curve.** Choosing any of them is choosing a cost ratio implicitly. This table
says which one.

---

## 4. WHAT EACH OPTION IMPLIES AT THE THREE NAMED DIAL POINTS

All at **c8**, `srtt_wire` = 77 ms, RTprop = 38 ms, σ = 18.1 ms (§2.3), against
the shipped cadence of **99.06 ms at `fa` = 0.4014**. `α` continuous in the
dials throughout; no branch, no threshold.

| option | Realtime (δ=50, b=½, ζ=0.01) | Auto (δ=0.5, b=1, ζ=1) | Bulk (δ=0.005, b=2, ζ=100) |
|---|---|---|---|
| **(a) α = r** | α = r (largest); fastest, wastiest | α = r; W 113–156 ms over r ∈ [0.05, 0.2] | **α = r\* = 0 ⇒ W = ∞. DEGENERATE** |
| **(b) W = srtt + b·RTprop** | α 0.475, **W 96 ms** | α 0.184, **W 115 ms** | α **0.054**, **W 153 ms** |
| **(c) α = 1/16 flat** | α 0.0625, W 147 ms | α 0.0625, W 147 ms | α 0.0625, W 147 ms |
| **(d) marginal-cost equality** | corner: α→1, **W → 77 ms** (fire at once) | α 0.189, **W 114 ms** (at ν=0.01) | α 0.008, **W 276 ms** (at ν=0.01) |

Reading the rows: **(b) and (d) both move the clock monotonically with the
dial** in the direction the contract declares — Realtime fast and wasteful,
Bulk slow and thrifty — and they agree closely at Auto. **(c) is flat: the
triangle does not reach the recovery plane.** **(a) has the right direction
and an unusable Bulk end.**

Two properties hold across every cell and every option and are worth stating
once: `W ≥ srtt` always, so sc2 improves under all four (§2.2); and none of
the four can rescue the **sender** site at c8/c8L, where `srtt_app` alone
(376 / 464 ms) exceeds every `W` in this table (§2.2).

---

## 5. WHAT HAPPENS NEXT, PER OPTION

**If any of (a)–(d) is decided:** the path is the one this tree already runs.

1. **The law, formula-first**, in the paper before it is code, with a
   per-symbol provenance table — the §16.67/§16.68 pattern. `α(r, δ)` written
   as one expression continuous in the dial, with `b(δ)` routed through
   `bulkness_of_delta(δ)` rather than the three-arm `match` at
   `net/mod.rs:3420-3426`, so the no-mode-switch invariant is structural and
   not merely observed.
2. **The instruments first, and two of the three are free.**
   * **σ**: one print line — `rtt_sigma_us()` already exists
     (`scheduler/mod.rs:3032-3037`) and is computed on every arm.
   * **ν**: one ratio of counters that already exist (`fired` at
     `net/mod.rs:4192`, delivered-symbol count). Required by (d), and it is
     what decides whether (b) and (d) are the same option.
   * **`fa` and σ reported at the SAME SITE** — the receiver gauge does not
     call `record_fire` (`net/receiver.rs:209`), which is why §2.3 has to
     cross clocks. This is the one instrument change with any surface area.
3. **`RWM_QUANTILE_CLOCKS` re-armed, default OFF**, with `α` fed from the
   decided mapping instead of `contract_alpha` (`net/mod.rs:767-769`) — which
   is the seat §16.69 refuted, and whose refutation stands untouched: the
   change is *what feeds α*, not the Cantelli construction, which was never
   the defective part.
4. **Pre-registration in its own commit before any VM contact**, scored on
   `fa_frac` against `α_class` at all five cells with goodput parity as a
   condition — the Candidates Battery's own design, which is the only design
   in this tree whose paired contrast has ever resolved.
5. **Flip only in a separate commit citing the results block**, per §16.71.

Per-option specifics: **(c) is the cheapest to execute** — its validation
instrument is already shipped and already run, so it needs only steps 1, 3, 4,
5. **(b) is the cheapest to specify** — `W = srtt + b(δ)·RTprop` needs no σ at
all in the law, only in the reporting. **(d) needs ν before it can be
specified at all.** **(a) needs a decision about its Bulk degeneracy before it
can be written down.**

**If nothing is decided (the do-nothing path).** `round = (2·srtt).clamp(25,
100) ms` stays. It is not neutral, and the cost is on the record rather than
hypothetical:

* It binds **92.4–99.7 %** of the time at every cell — a shipped clamp whose
  law is a constant, which is the exact defect CLAUDE.md's bind-fraction rule
  exists to catch, now caught on a shipped law by measurement.
* It violates RACK's own published spurious budget at **all five cells, by
  1.7× to 12.0×**.
* At sc2 it sits **below** the mean of the quantity it waits on (§2.2), which
  no amount of α-tuning is needed to see and no successor is needed to fix.
* `RWM_QUANTILE_CLOCKS` stays **REFUTED-STANDING** and `RWM_RACK_CLOCKS`
  **REFUTED-WITH-RECORD** (ADR-0066 deprecation register, §16.70.1), leaving
  the recovery-clock family with a convicted default and no successor —
  §16.70.1's own words: *"No flip is proposed, because the only successor
  written is the one refuted above — and that gap is the section's honest
  residue."*

The do-nothing path is a defensible choice. It is a choice to keep a measured
defect because no successor has cleared the bar, which is a position this tree
has taken deliberately before. It should be recorded as a decision if it is
taken.

---

## 6. THE OBSERVATION THAT MAY CHANGE THE URGENCY, IN BOTH DIRECTIONS

**No recommendation is made. This is the one thing the record says that bears
on whether the decision is urgent, and it cuts both ways.**

§16.71 flipped `RWM_DELTA_CAP` to default ON. §16.70.1 measured what that does
to the shipped clock's false-alarm rate, with no clock touched at all:

| cell | `fa_frac` before (arm A) | after (arm D) | change | still × 6.25 % |
|---|---|---|---|---|
| c8 | 0.4014 | **0.3050** | **−24 %** | **4.9×** |
| c8L | 0.2591 | **0.1294** | **−50 %** | **2.1×** |
| c7 | 0.2385 | 0.2273 | −5 % | 3.6× |
| c1 | 0.1066 | 0.1066 | 0 (N=1, cap inert) | 1.7× |
| sc2 | 0.7516 | 0.7760 | +3 % (N=1, cap inert) | 12.4× |

**The argument that this REDUCES urgency.** The largest single improvement to
the recovery clock's false-alarm rate ever measured in this tree came from a
law that is not a clock. The δ-cap halved it at c8L and cut it a quarter at
c8, for free, and it has already shipped. Running the same estimate as §2.3 on
the post-flip numbers, the implied σ at c8 falls **18.1 → 14.6 ms** and at c8L
**10.6 → 6.9 ms** — the cap did not merely move the clock's operating point,
it shrank the variance the clock has to absorb, which is the input every
option in this memo divides by. A second pool-side law might do the same
again, at no cost to the recovery plane's specification and with no product
decision required at all. **On this reading the cost ratio is not the binding
constraint; the store dwell is, and §2.2's sender-site table says the same
thing from the other direction — at c8 the sender's `srtt_app` is 376 ms
against a 38 ms RTprop, and no α can touch that.**

**The argument that this INCREASES urgency.** After the flip, **not one cell
clears RACK's budget.** The violation is 1.7× to 12.4×, and the two cells the
cap does not reach at all (c1 and sc2, both N = 1 — `RWM_DELTA_CAP` returns
before any multiplier is read at `n_live < 2`) are unchanged and unchangeable
by that route: sc2 remains the worst cell in the battery at 12.4×, and §16.70.2
records that *"the δ-cap's support does not extend to single-path cells at
all."* The cap bought a 24–50 % improvement on a quantity that needed a
94 % improvement. **On this reading the cap demonstrated that the mechanism is
real and moveable, and then ran out of reach at exactly the cells where the
defect is worst.**

**One fact is common to both readings and is not in dispute.** sc2's 12×
violation is structural — the shipped cadence sits 1.2 ms below sc2's own wire
`srtt` (§2.2) — and it is fixed by the *shape* of `W = srtt + margin`, at any
α, under any cost ratio. **The worst reading in the battery does not depend on
this decision.**

---

## 7. WHAT IS NOT CLAIMED

* **No option is recommended.** The decision is the user's.
* **σ is estimated, not measured, everywhere in this memo** (§2.3), by
  inverting an inequality across two clocks. Every `W` and every α computed
  here inherits that. FINDING B says the real number is one print statement
  away and no VM arm is required to get it; **every number in §3 and §4 should
  be recomputed against a reported σ before any of it is built.**
* **ν is not on the record at all**, so option (d)'s column is
  parameterised rather than evaluated, and the (b)≡(d) agreement at Auto is a
  prediction rather than a result.
* **§16.69's refutation is not re-opened.** Reasons 1 and 2 (Cantelli needs
  316σ at α = 1e-5; the min-deque cannot hold 10⁵ samples) stand exactly as
  written — they are consequences of α = 1e-5, and every α in this memo is
  between 0.008 and 0.99, where `k` is between 11 and 1 and neither reason
  applies. Reason 3 stands as a category error and is what this memo answers.
  `RWM_QUANTILE_CLOCKS` remains default OFF and REFUTED-STANDING; nothing here
  changes that until a decision is taken and a battery is run.
* **Finding A is a location claim, not a validation.** That δ is *a* declared
  latency price does not establish that it is the *right* price for the
  recovery plane. It establishes that the choice is a transfer decision rather
  than an invention, which is a different and smaller thing.
