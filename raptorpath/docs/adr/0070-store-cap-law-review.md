# ADR-0070: The store-cap law on trial — a term-by-term review of `clamp(gain·N·Σ, floor, N·knee)`, one defect, two fossils, one stale measurement, and the derived law that would replace the whole expression

> **AMENDMENT 2026-08-18 (`fix/cap-law-cluster`, paper §16.59, goal-gate "Mechanical Defect Sweep, items 1 / 2 / 4").** Finding 5's first half is DISCHARGED: `floor = 64` no longer exists. The bootstrap floor is DERIVED from its own stated job as `max(ANCHOR_MIN_SAMPLES · MERGED_ACK_SYMBOLS_PER_SAMPLE, RFC6928_INITIAL_WINDOW) = max(8·1, 10) = **10**`, with every input cited or measured. The composed law's `[CCAP]` floor-bind at loopback goes 81/81 → 0/61 and the realized cap drops from the floor's 64.8 to the LAW's own 60.0 — this ADR's mechanism 1 caught at the smallest scale in the tree. Finding 5's second half (`boot = 128`, ARGUED but never a battery arm) is UNCHANGED. Every other verdict below stands.
>
> **AMENDMENT 2026-08-18 — Deliverable 2's term 1.** §16.56's published `rateᵢ·RTpropᵢ` is corrected to `rateᵢ·Kᵢ·RTpropᵢ` (the form `net::three_term_store_cap` always computed), adjudicated on the term's job: it funds ONE ACK ROUND TRIP, and the ack takes `K·RTprop`. The agreement-test class this ADR's prevention kit implied but did not name now exists — `tests/formula_agreement.rs`.
>
> **AMENDMENT 2026-08-19 (`docs/tier0-corrections` — the literature cross-check's Tier-0 record corrections, `docs/research/literature-crosscheck.md`, paper §16.65).** Three provenance claims **in this ADR itself** were stronger than their sources support. No verdict, default or disposition is re-opened; the corrections weaken-to-accurate or claim what is ours:
>
> 1. **Finding 3's `9/8`-"cited not fitted" is withdrawn** (cross-check item 6(d)). RFC 9002 §6.1.2 RECOMMENDS `kTimeThreshold = 9/8` empirically — *"Experience with QUIC shows that 9/8 works well"* — it does not derive it, RACK (RFC 8985) uses **5/4** for the same purpose, and the RFC explicitly invites experiment. The `17/8` derivation this finding leans on therefore inherits a **tuned** constant.
> 2. **Finding 3's FOSSIL verdict is refined to: RIGHT VALUE, TWO PUBLISHED DERIVATIONS, WRONG LOCAL RATIONALE** (cross-check item 3, folklore item 1). The "≈1 BDP pipe + ≈1 BDP recovery-runway" prose appears in **no** primary source — BBR handles recovery by packet conservation and `prior_cwnd`, never via `cwnd_gain`. The published derivations of the value are RFC 6182 §5.3's `×2` (*"One BDP allows supporting reordering of segments by the network. The other BDP allows the connection to continue during fast retransmit"*) and BBR's `cwnd_gain = 2` (delayed/stretched/aggregated-ACK absorption in v1; *"the minimum gain value that allows the sending rate to double each round"* in draft-ietf-ccwg-bbr §2.5). The decl-site comment (`net/sender_policy.rs`) is corrected in this same pass; the KEEP-UNTOUCHED disposition and the no-re-fit rule (Decision item 4) are UNCHANGED — the literature agrees with the current value.
> 3. **Deliverable 2's "Zero fitted constants" is softened, and its span term acquires the novelty claim it was owed** (cross-check items 6(d) and 1(c)/Tier 0.6). The `9/8` inside the stall is cited AND tuned, per correction 1. And the span decomposition — a separable resequencing term (`2·rate_fast·skew`, the `Σ bwᵢ·(RTT_max − RTTᵢ)` shape) beside the window term — appears in **no publication** (checked against RFC 6182 §5.3, RFC 8684 §3.3.4, Barré 2011, Raiciu NSDI'12, DAPS/Kuhn ICC 2014): it is **ours** and must be presented as our derivation. What IS standard is the AGGREGATE `2·Σ bwᵢ·RTT_max` sizing with `RTT_max` outside the sum; our form is one step of algebra from it (and half its magnitude at N = 2), so that literature may be cited for the term's *magnitude*, never for its *shape*.

## Status: Accepted (2026-08-12) — **a REVIEW, not a change.** Every verdict below is a finding about provenance; no engine file, no gate, no default, no test is touched by this ADR. The replacement of Deliverable 2 is STATED, not shipped, and is gated on the validation path in the Decision.

**Date**: 2026-08-12

**Branch**: `docs/adr-0070-cap-law` from main@`631ed4c`. DOCS ONLY. No VM was contacted, no L1 number re-derived, no benchmark run.

## Context

### The challenge

The shipped outstanding-pool cap was challenged **term by term** — not "does it perform", which is the only question it has ever been asked, but "where did each symbol in it come from". The law:

```text
cap = clamp( gain · N · Σᵢ(max_bwᵢ · min_rttᵢ),  floor,  N · knee )

  gain  = 2.0            (RWM_STORE_GAIN)
  N     = live_paths().len()
  knee  = 2048 / path    (RWM_STORE_PATH_POOL)
  floor = 64             (store_cap_floor)
  boot  = 128            (RWM_STORE_BOOT), when the Σ has no warm term
  Σ-set = active_paths() (shipped) | live_paths() (RWM_STORE_CAP_UNIFIED, OFF)
```

`net::path_scaled_store_cap`, `net/mod.rs:2458-2487`:

```rust
    if !on || n_live < 2 || pipe_sum <= 0.0 {
        return None;
    }
    let ceiling = n_live.saturating_mul(pool).max(floor);
    Some(((gain * n_live as f64 * pipe_sum).ceil() as usize).clamp(floor, ceiling))
```

(One correction to the shorthand used everywhere else in the tree, including PIPELINE VERIFICATION MATRIX row 16 and paper §12.8's correction block: the upper clamp is `max(N·knee, floor)`, not bare `N·knee`. At the shipped `knee = 2048` the two are identical, and no cell has ever reached the difference.)

### Why this review had to happen, and why it did not happen earlier

**The formula had never been reviewed as a formula.** It has been measured relentlessly. Matrix row 16 (goal-gate.md:25016-25037) calls it *"the most thoroughly instrumented law in the pipeline"* — nine always-on absolute pins, a component bench (`tests/store_cap_bench.rs`), a closed-loop component bench (`tests/store_cap_sf_bench.rs`), an engine-equivalence pin, and an L1 mechanism gauge. That instrumentation measured **what the law does**. Not once did it ask **why each term is in it**.

The gap is structural, not accidental. Every store-cap section in the ledger for a month opens by naming a cell (c8) or an arm (`A`, `AU`, `P`) and asks whether the number moves. A formula-level read — five minutes, no VM, no bench — surfaces on the first pass that one of the five terms has *no provenance in the repository at all*, that a second is a frozen approximation of a quantity the tree now derives exactly, and that the whole expression exceeds its own ceiling by construction at every dual cell ever measured. That is the finding this ADR exists to record, and the prevention item at the end is the one that matters more than any verdict below.

### What was ALREADY on the record before this review

Honesty requires naming how much of this the ledger already knew, because it changes the character of the finding from *discovery* to *never assembled*:

- The set asymmetry is named as **"the defect"** in matrix row 16 (goal-gate.md:25019-25020) and in the paper's §12.8 correction (fec-arq-model.md:4109-4110).
- The `×N` deletion is named, arithmetically characterised (**zero new constants, bit-identical at N = 1**) and benched as `Arm::PooledUnified` (goal-gate.md:24276-24284).
- The pinning was measured — 121 of 126 dual reps at exactly `2·knee` (goal-gate.md:26616-26619).
- The reason none of it shipped is on the record twice (goal-gate.md:28770-28785, 29191-29199).

What was **not** on the record: a single document holding all five terms side by side with their provenance, and the observation that follows only from doing so — that the shipped multiplier makes the law **quadratic in N** where its own doc comment describes a **linear** quantity.

---

## The review, term by term

### 1. The Σ-set: `active_paths()` in a sizing law — **VERDICT: DEFECT** (already named; restated here for completeness)

`active_paths()` is `active AND available() > 0` (cwnd − in_flight). It is the **data-scheduling** filter. The engine's own decl comment says it is not a liveness predicate, and the code that reads it for the cap says so out loud (`net/mod.rs:4733-4749`):

> `active_paths()` … is the DATA-SCHEDULING filter; using it for a LAW is the documented cwnd-saturation trap … a wire-bound sender is cwnd-saturated by definition, so the filter drops exactly the paths that are carrying the transfer, mid-transfer.

Two consequences, both measured:

- **The count and the sum range over different sets.** `n_live` is taken from `live_paths()` at `net/mod.rs:4732` **regardless of the gate**; only the Σ-base follows `set` (`:4752`). So a cwnd-saturated path is counted in the `×N` and omitted from the Σ (`gates.rs:391-396`).
- **An empty `active_paths()` is a cliff, not a taper.** `path_scaled_store_cap` returns `None` at `pipe_sum <= 0`, the whole chain falls through to `store_boot_cap = 128`, and the drop is ≥ 6× — 813 with both paths in the Σ against 128 (goal-gate.md:24186-24193, pinned by `empty_active_set_is_a_cliff_not_a_taper`).

The Copa-sole seat of the same chain was already fixed to `live_paths()` unconditionally, with the measurement that forced it recorded inline (`net/mod.rs:4584-4591`: cap flapping 1024↔128, goodput dips to 20 Mbit). The plain seat was left on the scheduling filter behind an A/B.

**The honest counterweight — this defect is doing load-bearing work.** The cliff is *the loop's only stabiliser* (matrix row 17, goal-gate.md:25048-25051): a defect supplying a brake by accident. Removing it correctly cost c8 −19.6 % (paper §16.43). And the U bit's own record is worse than ambivalent — `RWM_STORE_CAP_UNIFIED` moved the c8 collapse mode from 1/16 to 8/11 (§16.53) before that harm was shown to be **length-scoped**, vanishing 0/24 at 8× the transfer (§16.54). This defect is real and it is not safely deletable alone.

### 2. `×N`, the count multiplier — **VERDICT: DEFECT, and its provenance is ABSENT**

**Where it came from.** The whole expression entered the tree at `5cace52` (2026-07-14, task #84, default OFF; flipped ON at `5ebbcda`, 2026-07-21, in the consolidation battery). The pre-existing law was `clamp(gain·Σ, floor, store_max)` — no count multiplier — and it is still in the tree as the legacy fallback (`net/mod.rs:4960-4961`).

The birth commit's own reasoning, verbatim (`net/mod.rs:2460-2467`, unchanged since):

> The plain-reliable OUTSTANDING ceiling was a per-transfer constant (`RELIABLE_STORE_MAX` = 1024) … so a MULTIPATH sender is store-starved — **the pool that must fund Σ per-path (BDP + one recovery round of runway) does not grow with the path count.**

That diagnosis is correct and it justifies **scaling the CEILING**. The quantity it describes — "Σ per-path (BDP + one recovery round of runway)" — is `Σᵢ(gain · anchorᵢ) = gain·Σ`. It is already linear in the path count, because the Σ is. The commit then shipped `gain · N · Σ`, which is **N times the quantity its own sentence names**, and no line of the commit message, the doc comment, the decl-site comment or the ledger section explains the multiplier.

**The evidence cited for it measures something else.** The birth commit's only measurement is a same-binary **static-store sweep** (`RWM_STORE`, which *disables the dynamic law entirely* — `store_env_set`, `net/sender_policy.rs:568`). It swept a constant pool: C7 1024→103 / 2048→122.7 / 4096→141.3 / 8192→143.7; C8 4096→71.5, 8192→31.8; sc2 8192→43.0 (goal-gate.md:10697-10706, mirrored at `net/sender_policy.rs:581-591`). That sweep measures **the ceiling**. It cannot see the multiplier, because with `RWM_STORE` set there is no multiplier running. **No A/B of `gain·Σ` against `gain·N·Σ` at a fixed ceiling has ever been run**, at L1, at a bench, or at L0.

**The arithmetic.** At a symmetric cell with per-path anchor `a` and N paths, `Σ = N·a`:

| law | value | order in N |
|---|---|---|
| clean generalization `Σᵢ(gain·anchorᵢ)` | `gain·N·a` | **linear** |
| shipped `gain·N·Σ` | `gain·N²·a` | **quadratic** |
| ceiling `N·knee` | `N·knee` | linear |

A quadratic value under a linear ceiling exceeds it for all N above a threshold — and the threshold does not depend on N at all. `gain·N·Σ ≥ N·knee ⟺ Σ ≥ knee/gain` — **the N cancels** (goal-gate.md:26629-26635, pinned by `the_pin_threshold_on_sigma_is_knee_over_gain_and_is_path_count_free`). At `knee/gain = 1024` symbols and the wire's own measured anchors — both legs summing to 1635 (c7) and 1510 (c8), i.e. 1.6× and 1.5× the threshold — the law is **always** pinned at both duals, while either leg alone (712–924 / 734–776) is always interior.

Measured consequence: `occcap_p50` reads **exactly 4096 = 2·knee** in **69/69** c7 reps and **52/57** c8 reps over 178 dual-cell reps from five independent sessions, with `capboot_frac = 0.0000` in every one (goal-gate.md:26616-26619). Per *refresh* rather than per rep the shipped law at c8 is at the ceiling ~59 % of the time, with 36.2 % one-leg-interior and 4.6 % at boot (goal-gate.md:26636-26639).

**So the ramp is decorative and the ceiling is the law.** The `×N` does not currently make the cap N× too large — while pinned, the cap is `N·knee` with or without it. Its cost is that it **removes the law's operating range**: the derived, measured, per-path term is never the number, and every store-cap result for a month has been a measurement of a constant that was swept as a static pool in a different era. The two arms differ exactly where the shipped one has been erased: at the wire's own anchors `gain·Σ` is **3270** (c7) and **3020** (c8) against the ceiling's 4096 — interior at both duals, for the first time.

**Three places in this repository already contradict the multiplier by name:**

1. The **honest-cap branch drops it, with the correct reasoning** (`net/mod.rs:4921-4927`): *"the Σ is already per-path-composed (each term carries its own K_i and runway), so no gain× multiplier here."* (Behind `RWM_PLAIN_RS`, umbrella default OFF, so it is not the shipped seat — `honest_cap_on = plain_dyn_cap && gates.plain_rs && gates.honest_cap`, `net/sender_policy.rs:746`.)
2. The **three-term pre-registration repudiates count-scaling by name** (goal-gate.md:20353-20358): *"the `active_paths()` vs `live_paths()` question — which path set a count-scaled law multiplies by — does not arise: this law reads `live_paths()` unconditionally and **never counts**."* Its sharpest prediction is exactly the one a count-keyed law cannot make: c7's span term is zero **at N = 2**, because c7's two paths are identical (goal-gate.md:20471-20475), and it measured 0.0000 over every rep (`:20920-20922`).
3. **PS6 priced the damage** (goal-gate.md:19819-19835): at c8 the predicted sender-retention span is 541 and the independently measured good pin is 508 (+6.5 %, inside the pre-registered ±10 %), while the arm that read **−19.6 %** at seed 7 was pinned at **4096 = 4096 ÷ 541 = ×7.57**, against the ×7.6 written down before looking. *The c8 collapse is a cell run at 7.6× its own resequencing span.*

### 3. `gain = 2.0` — **VERDICT: FOSSIL of a quantity this tree now derives**

**Provenance: argued in prose, never measured, and introduced somewhere else.** It entered at `ac3bc9d` (2026-07-07) with the **C2 single-path bufferbloat fix** — a week before the multipath law existed and with nothing multipath about it. The argument, `net/sender_policy.rs:569-571`:

> Window = gain × BDP. ≥2 keeps the pipe full (≈1 BDP) while leaving ≈1 BDP of headroom to keep sending fresh data during a one-RTT recovery round; 2.5 adds jitter/burst slack.

The argument is sound in shape: one BDP of pipe plus one BDP of recovery runway. The ledger repeats it (goal-gate.md:10882: *"~1 pipe full + ~1 recovery round of runway, per path"*). **[CORRECTED 2026-08-19: sound-sounding, but it appears in NO primary source — see head amendment 2 for the two published derivations of the value; the decl-site comment now carries them.]**

**Sweeps: exactly one, and it does not bear on the default.** `RWM_STORE_GAIN=1.25` was run at **one cell (sc2), on the Copa-sole passthrough arm — a different CC family from the plain/BBR default** — and read **−5 %** (goal-gate.md:10355-10359). The earlier +13/+23 % reading of the same knob was superseded in that same section, and the ledger says so explicitly at the time: *"RWM_STORE_GAIN's default is NOT done here (it also affects legacy plain)"* (`:10217`). It has **never** been swept above 2.0, never swept on the shipped stack, and never swept **inside** the path-scaled law.

**Why it is a fossil rather than merely un-swept.** The recovery-runway half of the argument is exactly what the three-term law derives, from cited constants and no fit: at ρ = 1, `stall = 9/8·srtt + srtt = 17/8·srtt = 2.125·srtt` — RFC 9002 §6.1.2 detection (`9/8`, cited not fitted) plus one retransmit round trip (goal-gate.md:20325-20343). **[CORRECTED 2026-08-19: "cited not fitted" overstates — RFC 9002's 9/8 is an empirical recommendation (*"works well"*); RACK uses 5/4. Head amendment 1.]** The frozen `1.0` is a 2× under-estimate of the quantity it approximates; the whole-expression multiplier the derivation implies is `1 + 17/8 = 25/8 = 3.125` in srtt units against the shipped `2.0`.

**Disposition: KEEP UNTOUCHED.** Re-fitting `gain` would be tuning a coefficient that the successor law deletes. It is not a defect; it is a placeholder that has outlived its derivation and should be replaced by the expression, not by a better number.

### 4. `knee = 2048/path` and the `N·knee` ceiling — **VERDICT: MEASURED, BUT STALE — and it is the actual operating point**

**This is the one term with a real measurement behind it.** The static-store sweep is a proper same-binary A/B across four pool values at four cells (goal-gate.md:10697-10706): a saturation at C7 (4096 → 141.3, 8192 → 143.7), a collapse at C8 (4096 → 71.5, 8192 → 31.8) and a collapse at the single sc2 (8192 → 43.0). The knee it establishes is a **total**: best dual total 4096 over 2 paths.

**"Per live path" is an INFERENCE, not the measurement.** The step from "4096 total at two paths" to "2048 per path" requires a third path count to distinguish `2048·N` from `4096` from `2048·√N`, and **no cell with three or more paths exists anywhere in the cell table**. The inference is stated as a fact in the decl comment ("the knee is ≈2048 outstanding symbols PER LIVE PATH at both C7 and C8", `net/mod.rs:2465-2466`) and has been carried unqualified ever since.

**The era is dead.** The sweep ran with (i) the **over-reading legacy anchor** (×4.6–7.4, floor-clocked — the honest fix shipped 2026-08-11, §16.51), (ii) **pre-SACK-clocked-release** (ADR-0060), (iii) pre-honest-inputs, and (iv) pre-`RWM_ACK_MERGE`. Every input to the number it measured has since changed. ADR-0066's own rule — *a refutation is only as good as the substrate it was measured on* — applies with equal force to a confirmation.

**And it is the number the engine actually runs on.** Because of finding 2, the ceiling is what every dual cell operates at (121/126 reps at exactly 4096). The ledger says so repeatedly (goal-gate.md:11099, 20480-20483). **The most-measured constant in the store chain is being used as the law, on an era that no longer exists.**

The successor treats it correctly: `three_term_store_cap` clamps to `WIN_STORE_MAX`, deliberately **not** the knee, because — `net/mod.rs:3100-3106` — *"the per-path 2048 knee the pooled laws clamp to is an empirical fit, and the whole point of this law is to DERIVE what that knee was approximating."*

**Date correction, on the record:** the sweep's owning ledger section is dated 2026-07-14, but its run timestamps are 2026-07-13 UTC (goal-gate.md:10632, 10667, 10728). Cite it as "the 2026-07-14 section", not "run on 2026-07-14".

### 5. `floor = 64` and `boot = 128` — **VERDICT: floor PROVENANCE ABSENT; boot ARGUED, never a battery arm**

**`floor = 64`.** One sentence of rationale, in source only — *"Floor so a transiently-tiny BDP estimate can't strangle the pipe"* (`net/sender_policy.rs:135`, `:578`). Never measured, never swept, no ledger entry, no derivation. Its magnitude has no argument at all.

It is nearly dead by arithmetic — reaching it needs `Σ < 16` symbols at N = 2, 46× below the smallest warm leg the wire ever reported (goal-gate.md:26674) — **but it is not dead**, and the record contains a clean miss: the three-term pre-registration asserted *"Never binds at any named cell (the smallest prediction is 163)"* (goal-gate.md:20625-20627), and then the battery measured the law landing exactly on it: `shal8 | 64 | 455/325 | 0.14 | NO — the law lands on store_cap_floor = 64` (`:20910`). A constant with no provenance binding a cell where the pre-registration said it could not is precisely the class of event discipline item 18 below exists to escalate.

**`boot = 128`.** Argued, and the argument is good (`net/sender_policy.rs:573-577`): tight so the startup burst cannot pre-bloat the queue and inflate the min-RTT floor, *which would then inflate the anchor itself* — a closed-loop argument, correctly identified. But the gate-forwarding audit records its measurement status in four words: `RWM_STORE_BOOT | n/a — never a battery arm` (goal-gate.md:18791).

Its **one recorded interaction is pathological**, and it is not a cold-start interaction at all: `boot` is the terminal `else` of both cap chains (`net/mod.rs:4652`, `:4963`), so the `active_paths()` cliff of finding 1 lands a **steady-state, mid-transfer** sender on the cold-start constant. Measured bimodal 128 ↔ 1024 across reps, priced at +15.8/+24.8 % goodput when removed (`net/mod.rs:7838-7843`); a ×6.4 cliff to 128 that is the loop's only stabiliser (matrix row 17). **A cold-start guard is a load-bearing steady-state brake.** That is a defect of finding 1, not of the constant — the constant is simply where the fall lands.

### 6. `max_bw · min_rtt` — **VERDICT: DEFENSIBLE SHAPE, MISUSED AS AN INPUT**

**The shape is right and the argument for it should be stated, because it is the one term whose derivation is real.** `max_bw · min_rtt` is the BBR-style **queue-free BDP**: a windowed *max* of delivered rate against a windowed *min* of RTT. Every alternative built from averages is worse for a structural reason, not an empirical one — an averaged RTT includes the standing queue the cap itself created, so `cap → queue → RTT → cap` closes a positive feedback loop and the estimator inflates without bound. The max/min pair breaks that loop by construction: the rate max cannot be inflated by queueing and the RTT min is the queue-free floor. This is also why the Copa-sole seat had to abandon the anchor entirely — under the honest send-interval sampler the `2×anchor` cap became **circular** in the other direction (`net/mod.rs:4571-4583`): *"samples can never read above the store-capped delivered rate, so the anchor could never grow toward the pipe (L0 MEASURED: stuck at ~3.2k of 10.4k sym/s, throughput 18 of 66 Mbit/s)"*.

**The misuse.** The anchor's own decl doc forbids the use this law makes of it (`scheduler/mod.rs:1873-1879`, on `bdp_anchor()`):

> `max_bw` is a windowed MAX of coarse ACK-batch rates with no per-packet/app-limited accounting … It **STRUCTURALLY underestimates** a warm-up/app-limited flow, which is exactly why it is only ever used to RAISE cwnd (recovery target + floor), **never as a cap** — an underestimate can only fail to help, never suppress.

*(The source reads lowercase "never as a cap"; only "STRUCTURALLY" is capitalized. The clause's second half — "an underestimate can only fail to help, never suppress" — is the whole argument and must not be truncated when quoted.)* The same distinction is drawn again at `scheduler/mod.rs:621-622`: "**floor, NOT cap**".

The store law consumes it as *exactly a cap input*. The contradiction was invisible for one reason: the legacy sampler **over-read by ×4.6–7.4**, so the structural underestimate never showed. The honest fix (§16.51, `RWM_HONEST_ANCHOR` default ON since 2026-08-11) removed the compensating error and left the misuse exposed. Note the direction: an honest anchor makes this term *smaller*, which under finding 2's pinning is currently invisible — the ceiling absorbs it — and would become immediately visible the moment the ramp is live.

**The estimator is not the axis; its consumer is.** The sampler is fixed and shipped. What is unfixed is that a floor-shaped quantity is doing ceiling-shaped work.

### 7. The late-stage brake, `RWM_INFL_CAP` / `cwnd_full` — **VERDICT: the correct architecture, DISABLED WITHOUT A DECISION**

The admission gate is `reliable && (store_len >= effective_store_cap || cwnd_full)` (`net/mod.rs:5220`). The second disjunct is a **per-path, late-stage, in-flight** brake — congestion state applied at emission, per placement, which is where congestion state belongs. It is permanently false:

- `RWM_INFL_CAP` defaults **0** (`gates.rs:327-328`, `:540`);
- `cwnd_full = eff_infl_cap > 0 && …` (`net/mod.rs:4541-4542`) short-circuits to `false` on the plain-reliable seat, where `infl_bdp_on` is unset. (Under `gen_pipe`, `RWM_INFL_BDP` resolves to 1.5 and the brake is live — the claim is scoped to the plain seat, not unconditional.)

Consequence, stated as a matrix finding: **the store cap is the SOLE brake on outstanding** (goal-gate.md:25047-25048, 24172).

**No decision record exists.** It was measured **once**, as a two-point null inside a 2026-07 diagnostic sweep — throughput ≈ 16.5 Mbit invariant across `infl_cap ∈ {100, 160}` and `store ∈ {96…1024}` at a C2 single (goal-gate.md:6758-6760) — at a cell whose ceiling that same section attributes to a frontier-bound recovery limit, i.e. a cell where *nothing* in this plane could have moved. There is no ADR, no register row and no ledger section recording when or why the default became 0, or that a null at one single-path cell was taken as a verdict for the multipath plane.

**This is the architecture the user's instinct names, and the architecture ADR-0058 half-defends.** ADR-0058's three-refinement chain (percap → guard+honest caps → bounded borrowing) is a genuine refutation and it defends **ONE POOL** — borrowing matters, lender-solvent slack cannot match pooled depth. It defends nothing about the pool's **size formula**, and it says nothing about a per-path brake applied *downstream* of a pooled account. Those are orthogonal questions and the chain answered only the first.

### 8. The ceiling zoo — four ceilings, one measured

Every ceiling in the plain chain (`net/mod.rs:4880-4964`), in evaluation order, with the gate that selects it and its provenance:

| # | arm | ceiling | selector | default | provenance of the VALUE |
|---|---|---|---|---|---|
| B1 | `three_term_store_cap` | `WIN_STORE_MAX` = 4096 | `RWM_THREE_TERM` | OFF | memory bound, stated as outside the law |
| B2 | `win_decouple_cap_ret` (N = 1) | `WIN_STORE_MAX` = 4096 | `RWM_WIN_DECOUPLE` | OFF | as B1 |
| B3 | `capw_store_cap` | `max(N·knee, floor)` | `RWM_STORE_CAPW` | OFF | inherits the knee |
| B4 | honest Σ, **no `gain×`** | `N·knee` if N ≥ 2 else `store_max` | `RWM_PLAIN_RS` ∧ `RWM_HONEST_CAP` | OFF (umbrella) | inherits the knee |
| B5 | `capw_store_cap` on send anchors | `max(N·knee, floor)` | `RWM_POOL_ANCHOR` (← `RWM_EST_CADENCE`) | OFF | inherits the knee |
| **B6** | **`path_scaled_store_cap`** | **`max(N·knee, floor)` = 4096 at N = 2** | **`RWM_STORE_PATHS`** | **ON** | **the 2026-07-14 static sweep — the only measured one** |
| B7 | legacy `gain·Σ` | `store_max` ← `RELIABLE_STORE_MAX` = 1024 | `bdp > 0` | fallback | **existence defended, value never** |
| B8 | `store_boot_cap` | `min(128, 1024)` | else | terminal | never a battery arm |

**B6 is the shipped seat**: B1–B5 are all default-OFF, so at N ≥ 2 with a warm Σ the live law is `path_scaled_store_cap`, which is what the 121/126 pinned reps measure. Above all of it, `RWM_STORE_PERCAP` can replace the result outright (`net/mod.rs:5131`, default OFF), and `RWM_STORE` disables the entire dynamic chain (`net/sender_policy.rs:568`).

The two non-knee ceilings:

- **`RELIABLE_STORE_MAX` = 1024** (`net/mod.rs:979-997`). Its **existence** is defended by a real measurement: relaxing it in out-of-order object mode slid the O(200) coding window away from un-received holes and **C8 collapsed to 2.5 Mbit** — worse than the 11.4 in-order baseline. Two caveats the citation must carry: the defence is scoped to **out-of-order object mode**, and it defends *having a cap*, not *1024*. The number itself ("a few BDPs … ≈ 10× the C2 BDP") is unjustified anywhere.
- **`WIN_STORE_MAX` = 4096** (`net/mod.rs:3211-3215`), a memory ceiling: 4096 × ~1.2 KB ≈ 5 MB. The three-term pre-registration lists it as *"a MEMORY bound, not law"* and then, in the same document, records it as **"the phase's largest un-derived quantity"** (goal-gate.md:20621-20624) — a self-description that lives in the ledger, not in the source. It **binds**: c2r200 clamped at exactly 4096 with no verdict possible (goal-gate.md:20908), and at c1 for RTprop ≥ 50 ms.
- **`RWM_INFL_CAP` = 0**, the disabled brake of finding 7 — the fifth ceiling, and it is off.

**One law, four live ceilings above it, and the only one anybody measured is the one measured in a dead era.**

---

## The N² postmortem — how a quadratic survived a month of the most instrumented law in the pipeline

The shipped law is quadratic in N at symmetric cells where its own doc comment describes a linear quantity. It was caught in **minutes**, on the first read that treated it as a formula rather than as an arm. Five mechanisms kept it alive, and each is a generalisable failure:

**1. The clamp ate the evidence.** `gain·N·Σ` saturates iff `Σ ≥ knee/gain` — a condition **independent of N**. Both duals sit 1.5–1.6× above that threshold at their own measured anchors, so the unclamped value never appeared in any output. Every measurement of the "law" was a measurement of the ceiling. **A clamp that always binds converts a law into a constant and hides its shape completely** — and the shape is the thing under test.

**2. `N ∈ {1, 2}` is the entire test universe.** A quadratic and a linear law differ by a factor of `N`, which is `1` at N = 1 and — because the ceiling absorbs it — unobservable at N = 2. The exponent is only *distinguishable* at N ≥ 3, and **no cell, bench geometry or L1 arm in the tree has ever run three paths**. The same gap is what made "per live path" an inference in finding 4: one dual measurement cannot separate `2048·N` from a constant 4096. Two independent errors, one missing axis.

**3. Two defects masked each other.** The legacy anchor over-read by ×4.6–7.4 while the multiplier over-scaled by N. Their product landed the cap where it needed to be, and the ledger noted the cancellation without reading it as one: at goal-gate.md:24467-24475 the store cap is recorded as `gain·N·Σ anchor` — *"ALSO LINEAR"* — precisely so that a common anchor scale **cancels** in a saturation ratio. Correct arithmetic, and it is exactly the property that let the over-read hide the multiplier. The honest-anchor flip (§16.51) removed one error a month before the other was named.

**4. The pinning was measured, and filed as a cell fact.** "Cap-Refresh Warmth" (2026-08-11) measured 121/126 dual reps at exactly `2·knee` and titled the result a **refutation of a downstream premise**. The finding was true, well-instrumented, and correctly reasoned — and the sentence *"the law never expresses itself at any dual cell"* was recorded as a property of c7 and c8 rather than as a property of the formula. **A degeneracy result was written up as a measurement outcome instead of a defect finding.** One day later, on a formula-level read, the same number is the primary evidence for finding 2.

**5. Nobody ever reviewed the formula as a formula.** This is the root, and the other four are its symptoms. The law acquired nine always-on absolute pins, two component benches, an engine-equivalence pin and an L1 gauge — every one of them asserting that the code computes what the model says, and none asking whether the model is right. Matrix row 16 says **VERIFIED**, and it is: *verified against a model that was never reviewed*. Design review presented diffs and arms. It never presented `gain · N · Σᵢ(max_bwᵢ·min_rttᵢ)` on a line by itself next to the sentence it was supposed to implement.

**The prevention kit** — five items, each landing as code or a binding rule rather than prose. Items 4 and 5 are recorded in this ADR's companion commits (CLAUDE.md "FORMULA-FIRST LAWS", MEASUREMENT DISCIPLINE 17 and 18); items 1–3 are the first step of the validation path below, deliberately built *before* the battery that would rely on them.

1. **Law-shape tests, always on.** Property tests of scaling structure on synthetic inputs, across axes the cells never exercise: `cap(N)/cap(1)` over N = 1…8 symmetric, and — decisively — **the UNCLAMPED formula tested separately from its clamp**. A clamp may never be the only thing making a law sane. Applied to `path_scaled_store_cap` first; the template documented for every future law.
2. **Bind-fraction gauges on every clamp.** The report itself flags any floor or ceiling binding above a threshold of evaluations, with the standing sentence *"this law operates as a constant."* The `capboot_frac` / `occcap_p50` pattern already exists — standardise it and add the report-side check to the L1 parsers, so mechanism 4 above cannot recur silently.
3. **N ≥ 3 coverage.** A 4-path symmetric geometry in the SF bench (deterministic, cheap, no VM) plus one quad-path L1 cell in the cell table. This is the axis mechanism 2 is made of.
4. **Formula-first review** (CLAUDE.md + discipline item): no law ships without its formula and derivation in the paper *before* the code; design review presents formulas, not diffs; the verification matrix gains a **"law-shape verified?"** column, distinct from "implementation matches model".
5. **Degeneracy = red alert** (discipline item 18): a measurement showing a law pinned or degenerate over its operating range is a **defect finding requiring a ledger verdict**, never an explanatory footnote.

---

## Deliverable 2 — the candidate replacement, STATED AND NOT SHIPPED

Every finding above is a symptom of one thing: the expression is a fitted constant wearing a law's clothes. The replacement is not a better coefficient; it is the deletion of the whole expression in favour of one already built, already validated at component level, and — critically — **never composed as a single arm**.

```text
cap = Σ_i [ rate_i · RTprop_i  +  rate_i · stall(δ, ρ, srtt_i) ]  +  2 · rate_fast · skew
      over live_paths(), on honest inputs

      stall = (1 − ρ)·D(δ) + ρ·(9/8·srtt + srtt),   D(δ) = min(b(δ)·RTprop, 2·RTprop)
      skew  = (max_i RTprop_i − min_i RTprop_i) / 2
      ceiling: NONE arbitrary — δ prices the queue as a latency budget;
               the memory bound is stated SEPARATELY as a resource limit, not a law term
      late-stage brake: per-path cwnd_full ENABLED, at emission, per placement
```

**Zero fitted constants** *[softened 2026-08-19 — head amendment 3: the `9/8` is cited AND tuned, so this claim overstates; and the span decomposition is OURS, no publication writes it]*. `9/8` is RFC 9002 §6.1.2 `kTimeThreshold`, cited as an empirical recommendation (*"Experience with QUIC shows that 9/8 works well"*; RACK uses 5/4). The span `2` is a definition boundary identified in §16.43 PS5 and measured at 2.00 ± 0.03 over 18/18 non-zero cells — identified, not fitted. `b(δ)` is the δ dial's named points (½ / 1 / 2) and ρ is the declared retention contract: **dials, not modes** — CLAUDE.md's invariant is satisfied by construction, not by inspection, and the law contains no `if n == 1`, no topology predicate and no δ/ρ threshold (goal-gate.md:20353-20358).

### What already exists, and what it has been measured to do

| piece | where | status |
|---|---|---|
| the three-term law | `net::three_term_store_cap`, `net/mod.rs:3107-3148`, `RWM_THREE_TERM` | **built and shipped-gated OFF.** Terms validated at the bench (§16.43/§16.44, no constants); engine↔bench equivalence pinned by `three_term_engine_law_is_the_bench_terms_at_the_anchors` — the one such pin in the tree |
| honest anchors | `RWM_HONEST_ANCHOR` | **DEFAULT ON since 2026-08-11** (§16.51); value-identical statistic, less work |
| the unified Σ-set | `RWM_STORE_CAP_UNIFIED` | built, OFF; correct in principle (finding 1), harm shown length-scoped (§16.54) |
| the late-stage brake | `RWM_INFL_CAP` / `cwnd_full` | **built and disabled**, one null measurement, no decision record (finding 7) |
| the `×N` deletion | `Arm::PooledUnified`, `tests/store_cap_sf_bench.rs` | bench arm only; **no engine gate exists** |
| δ as a latency budget | the Latency Lever battery (§16.47) | the cap **provably prices delay**, signed and predictable, 12/12 |
| memory bound, stated separately | `WIN_STORE_MAX` | already framed as "a MEMORY bound, not law" by the three-term pre-registration |

**This composition has NEVER been measured as one arm.** Not at L1, not at the SF bench, not at L0. Every row above was scored alone or in a pair; several were scored in compositions now known to be confounded. The strongest single reason to state the law here rather than build it is that the last two batteries to touch this plane both fired pre-registered STOP RULES (§16.53, §16.54), and the honest reading of that is that **the instruments, not the law, are the current bottleneck**.

### The known blockers, each with its named answer

| blocker | answer |
|---|---|
| `WIN_STORE_MAX` = 4096 binds and is un-derived | replaced by the **δ ceiling**: `cap − BDP` *is* the queue, and δ prices queue as a latency budget (§16.47's measured result). The memory bound survives beside the law as a resource limit that may abort, not as a term that shapes |
| the c8 **dead wall** | it belonged to the **arm**, not the cell (§16.53), and it is **length-scoped** — 0/24 at 8× the transfer (§16.54). The composed cap at c8 is ~500 (PS6's 541 prediction, 508 measured good pin) against the shipped 4096, which removes the queue that drives the recovery clamp's overshoot in the first place |
| the anchor **over-read** feeding every pooled law | fixed and shipped: honest inputs, default ON (§16.51) |
| `RWM_STORE_CAPW` cannot compose with U | true, and it is not this law — `capw_terms` read `live_paths()` unconditionally and `capw_store_cap` sits **above** `path_scaled_store_cap`, making the U bit a no-op wherever capw engages (goal-gate.md:28786-28796, paper §16.54). The composed law does not inherit this: it reads `live_paths()` by construction and has no set A/B at all |
| c8's statistic is bistable; a mean is the wrong statistic at any n | acknowledged, and it is why the dead-wall onset/duration instrument is step 2 of the validation path and not an afterthought (goal-gate.md:24266-24274) |

**Honest against-the-case, stated:** three-term arm A already read `cap = 264` at c1 against 488/163 predicted (goal-gate.md:20904) and **landed on `store_cap_floor` = 64 at shal8** where the pre-registration said the floor could not bind. A law with no arbitrary ceiling still has a floor, and that floor has no provenance either (finding 5). Removing the pooled ceiling does not automatically produce a law with no un-derived constants — it produces a law with **one**, and the derivation of the floor is owed.

---

## Decision

**1. The verdicts stand as findings of record**, one line each:

| term | verdict |
|---|---|
| Σ-set `active_paths()` | **DEFECT** — a scheduling filter in a sizing law; already named, and currently load-bearing as an accidental brake |
| `× N` | **DEFECT, provenance ABSENT** — quadratic where its own comment says linear; never A/B'd against `gain·Σ`; contradicted by name in three places in this repository. **DISPOSITION 2026-08-19: the deletion is EXECUTED by default flip** (`RWM_SUM_CAP` ON; see the annotation below the table) |
| `gain = 2.0` | **FOSSIL** — sound prose argument, one sweep at one cell under a different CC, superseded by a derivation the tree already contains (17/8 at ρ = 1) **[REFINED 2026-08-19: right value, two published derivations (RFC 6182 §5.3 ×2; BBR `cwnd_gain = 2`), wrong local rationale — the recovery-runway prose is in no primary source. Head amendment 2; disposition unchanged]** **[DISPOSITION 2026-08-19: the fossil is REPLACED, not re-derived — `RWM_DELTA_CAP` ON puts `1 + q(δ)` in this factor's seat in the POOLED law; see the annotation below the table]** |
| `knee = 2048/path`, ceiling `N·knee` | **MEASURED BUT STALE** — a real sweep in a dead era; "per path" is an inference no 3-path cell has ever tested; it is the actual operating point at every dual |
| `floor = 64` | **PROVENANCE ABSENT** — one sentence, never measured; bound at shal8 where the pre-registration said it could not |
| `boot = 128` | **ARGUED, NEVER A BATTERY ARM** — a cold-start guard doing steady-state braking through the finding-1 cliff |
| `max_bw · min_rtt` | **DEFENSIBLE SHAPE, MISUSED** — the right queue-free estimator, consumed as a cap input its own doc forbids; the over-read is what made the misuse invisible |
| `RWM_INFL_CAP` / `cwnd_full` | **CORRECT ARCHITECTURE, DISABLED WITHOUT A DECISION** — one null at a cell that could not move, no ADR, no register row |
| `RELIABLE_STORE_MAX` = 1024 | existence measured (C8 → 2.5 Mbit), scoped to out-of-order object mode; **the value un-derived** |
| `WIN_STORE_MAX` = 4096 | memory bound, self-described as the phase's largest un-derived quantity; **it binds** |

**FINDING 2's DISPOSITION, annotated 2026-08-19 (the ADR's verdicts are not re-opened; this records what the validation path below produced).** Step 4 of that path ran as the pre-registered ladder battery (goal-gate "Ladder Battery — RESULTS"; paper §16.63) and step 5's flip decision was taken in its own separate commit (paper §16.64):

- **The deletion is EXECUTED**, by default flip: `RWM_SUM_CAP` resolves ON, so the shipped pooled law is `clamp(gain·Σ, floor, N·knee)`. The A/B this finding said *"has never been run, at L1, at a bench, or at L0"* **has now been run**, and the corrected law landed interior at both scoreable duals (`pin` 0.000, `eng` 1.000, `chg_frac` 1.000) against a control that reproduced this finding's own premise — the shipped default pinned at 4096 on 19/19 c7, 20/21 c8, 25/25 c8L reps, and at 1024 on 17/18 sc2 reps.
- **The PROVENANCE verdict stands entirely untouched.** Nothing measured on the wire supplies the multiplier with the derivation it never had; the flip removes it rather than justifying it. This finding's reversal condition — *"an A/B of `gain·Σ` against `gain·N·Σ` at a fixed ceiling finding the multiplier ahead ≫σ"* — did NOT fire: the multiplier was behind or at parity everywhere, and at the risk cell the corrected law was ahead.
- **The PRACTICAL weight of the span this finding priced is RETIRED at these cells, and only at these cells.** The corrected law under-funds c8's own resequencing span (`W + S` = 4232) by **45.4 %** — worse than the 29 % the pre-registration predicted — and c8 goodput went **UP on both seeds**. The span was not load-bearing at c7 or c8 in this era. The bound is stated with it: the c8 noise floor (2σ = 27.07 Mbit/s on a 77–86 base, n = 21/24, bistable cell) excludes a **large** regression, not a small one, and no claim is made about cells or path counts this ladder did not visit. PS6's ×7.57 pricing of the c8 collapse remains the record of what the pinned law did; it is not re-derived here.
- **The displaced arm is preserved, not deprecated.** `RWM_SUM_CAP=0` re-runs the quadratic verbatim with no warning, its shape still pinned by `path_scaled_store_cap_value_is_quadratic_in_n_the_documented_defect` and its `=0` echo value still asserted. Register row in ADR-0066. Findings 1 and 3–8 are unchanged by this flip: `gain` is still a FOSSIL and the knee still MEASURED-BUT-STALE, carried identical on both arms by construction, and the "two things explicitly NOT licensed" clause below still forbids re-fitting either.

**FINDING 3's DISPOSITION, annotated 2026-08-19 (the ADR's verdicts are not re-opened; this records what the successor path produced).** ADR-0071 family 2 wrote the δ-priced replacement FORMULA-FIRST, paper §16.67 stated it as code-free arithmetic with its provenance table, the pre-registered candidates battery scored it (goal-gate "Candidates Battery — RESULTS"; paper §16.70), and the flip decision was taken in its own separate commit (paper §16.71):

- **The fossil is REPLACED in the POOLED law, not re-derived**, by default flip: `RWM_DELTA_CAP` resolves ON, so the pooled value multiplier is the CoDel-derived `1 + q(δ)` and — composed with finding 2's flip — the shipped pooled law is `clamp((1 + q(δ))·Σᵢ bwᵢ·RTpropᵢ, floor, N·knee)`. **`gain = 2.0` does not appear in the shipped pooled VALUE at all**; it survives on the `RWM_DELTA_CAP=0` arm and at the other cap seats this finding also covers, where it is untouched and still a FOSSIL.
- **The FOSSIL verdict stands entirely untouched, and so does head amendment 2.** Nothing measured supplies `2.0` with the local recovery-runway rationale it never had; the flip displaces the constant rather than justifying it, and the replacement's own provenance is of a different kind — RFC 8289 §3.2's band is CITED and DERIVED (Kleinrock power maximisation), both band endpoints are quoted, both dial endpoints are read from `net::delta_budget_b`, so the substitution introduces **no fitted constant and no new degree of freedom**. This is the "superseded by a derivation" half of the verdict arriving; the "right value, wrong rationale" half is what the supersession makes moot for this seat.
- **What the wire measured, and its bounds.** D-LAT six of six — goodput PARITY at every dual on both seeds with `q_p50` down 10–200 ms at every one — interior with the `N·knee` ceiling **provably inert** at c7 and c8 (`pin` = 0.0000, `eng` = `chg` = 1.00), `eng = 0/0` at the singles, and c8's paired dead wall shortened (p ≈ 0.011). §16.63's `×N` datum pointed this way and this is its confirmation on a second, independently derived multiplier: **the pool's gain was funding delay, not goodput.** Bounds carried: goodput is parity and not a win; **c8L is a PARTIAL delivery** (`pin` = 0.23, in the gap between the contract's two pre-declared branches, neither claimed — the within-run Σ series this ADR's finding 2 disposition already owed is the named instrument, and it is now owed twice); the probe disagrees in sign with `q_p50` on one of six rows; and the c8/seed-7 abort class is arm-correlated, so that cell's seed-7 exclusions are a selection on the treatment.
- **The displaced arm is preserved, not deprecated.** `RWM_DELTA_CAP=0` re-runs `gain = 2.0` verbatim with no warning, the substitution's shape still pinned two-sidedly by `the_delta_cap_substitutes_one_factor_and_reduces_to_candidate_d` and its `=0` echo value still asserted. Register row in ADR-0066. **Findings 1, 2 and 4–8 are unchanged by this flip**: the knee is still MEASURED-BUT-STALE (and is now measured INERT at the two duals rather than assumed inert — a reading about those cells, not a re-derivation), the floor's provenance is still ABSENT, and the "two things explicitly NOT licensed" clause below still forbids re-fitting either. **`WIN_STORE_MAX` is not touched**: the ceiling this flip leaves in place is the same one, and family 2's proposal to remove it entirely is NOT what shipped.

**2. Nothing flips in this ADR.** No default moves, no gate is added, no engine line is touched. The findings are provenance findings; a default that has been measured extensively is not made wrong by a bad derivation, and ADR-0067's rule cuts against flipping on inference in both directions.

**3. The validation path, approved, in order** — the instruments before the battery, deliberately:

1. **Prevention kit first** (items 1–3 above): law-shape property tests on `path_scaled_store_cap` including the unclamped formula; bind-fraction checks in the L1 report parsers; the 4-path symmetric SF-bench geometry. These are the instruments the validation itself needs in order to be trustworthy, and item 3 is the axis whose absence caused the defect.
2. **The dead-wall onset/duration instrument** (DIAG-side), the statistic-stability prerequisite recorded at the close of the mode-hunt work — so c8's statistic can resolve an effect at the cell where the decision is taken.
3. **Bench**: the composed law as an SF-bench arm at c1 / c7 / c8 / sc2 plus the new 4-path geometry; must not regress the validated pins.
4. **Pre-registered battery**: `{shipped, composed} × {c1, c7, c8@25MB, c8@200MB, sc2}` (plus a quad-path cell if it lands in time), `n` sized per the mode-rate lesson — **score against the arm that carries the mode** (§16.53/§16.54) — headroom written beside every target (discipline 16), abort ≠ DNF, finite-step VM protocol.
5. **Flip decisions per gate, separate commits, register rows.** The legacy-path deletion returns to the table **only if the composed law wins everywhere**; a per-cell split leaves both laws in the tree with the split recorded, per ADR-0069's shape.

**4. Two things are explicitly NOT licensed by this ADR**: re-fitting `gain` or `knee` to better numbers (that is tuning a formula this review finds structurally wrong, and it would burn the only clean comparison available), and shipping any part of the composed law without step 1 (the battery would be scored on instruments that cannot see the property under test — the exact failure this postmortem is about).

## Consequences

- **The most-instrumented law in the pipeline is now also the one with the worst-documented derivation**, and both statements are true at once. Matrix row 16 keeps its VERIFIED status — the code does compute the model — and gains the "law-shape" question as an open column.
- **A second knowingly-not-best-derived shipped default is recorded**, after ADR-0069's block-mode pipeline and ADR-0067's c8 WATCH. This one is a constant rather than a pipeline, and unlike those it is not known to be *worse* — it is known to be **unexplained**.
- **`RWM_INFL_CAP = 0` becomes a named decision instead of an unrecorded default.** It is the first item in the tree found to have reached its shipped value with no record of who set it or why; the register-mechanism gap ADR-0069 named for CLI-shaped defaults has a second shape.
- **Two standing rules land with this ADR** (companion commits): CLAUDE.md's **FORMULA-FIRST LAWS**, and MEASUREMENT DISCIPLINE **17** (law-shape) and **18** (degeneracy = red alert). Both are enforcement of the root cause, not of any verdict above.
- **No engine behaviour changed**; zero production lines touched.

**What would reverse or amend these verdicts:** (i) an A/B of `gain·Σ` against `gain·N·Σ` at a fixed ceiling finding the multiplier ahead ≫σ — the multiplier would cease to be a defect and become a measured, named, still-underived choice; (ii) a 3-or-more-path cell measuring the knee scaling as genuinely per-path — finding 4's inference would become a measurement; (iii) a record surfacing that dates and justifies `RWM_INFL_CAP = 0` — finding 7's "without a decision" would be withdrawn while its "never re-measured" stands.

## Evidence

- **Code** (this branch, main@`631ed4c`): `net/mod.rs:2458-2487` (the law), `:4570-4583` (Copa-sole circularity), `:4584-4591` (the `live_paths()` fix and its measurement), `:4732-4752` (the set selection and the count/Σ mismatch), `:4880-4964` (the full plain chain, B1–B8), `:4921-4933` (the honest branch that drops the multiplier), `:4960-4961` (legacy `gain·Σ`), `:3100-3106` + `:3107-3148` (the three-term law and why it refuses the knee), `:3211-3215` (`WIN_STORE_MAX`), `:979-997` (`RELIABLE_STORE_MAX` and the C8 2.5 Mbit defence), `:4541-4542` + `:5220` (the brake and the admission gate), `:5136-5144` (final selection), `:7838-7843` + `:8135-8141` + `:8232` (the cliff, priced and pinned); `net/sender_policy.rs:135` + `:569-577` + `:578` + `:581-591` + `:746` (gain, boot, floor rationales, the sweep mirror, the honest-cap gating); `scheduler/mod.rs:621-622` + `:1873-1879` (floor-not-cap); `gates.rs:327-328` + `:391-396` + `:488` + `:496-499` + `:504` + `:540` + `:552`.
- **Ledger** (`docs/goal-gate.md`): `:10697-10706` (the static sweep; run stamps `:10632`/`:10667`/`:10728`), `:10355-10359` + `:10217` (the single gain sweep and its own disclaimer), `:10882` (the gain argument), `:11099` + `:20480-20483` (pinned at `N·knee`), `:18791` (`RWM_STORE_BOOT` never a battery arm), `:6758-6760` (the `RWM_INFL_CAP` null), `:19819-19835` (PS6, ×7.57 and −19.6 %), `:20320-20345` (the three-term law verbatim with per-symbol provenance), `:20353-20358` + `:20471-20475` (count-scaling repudiated by name), `:20621-20627` + `:20904-20922` (`WIN_STORE_MAX` self-described; the floor's recorded miss at shal8), `:24186-24193` (the cliff), `:24266-24284` (bistability; the `×N` deletion candidate and its bench table), `:24467-24475` (the cancellation, recorded), `:25016-25058` (matrix rows 16 and 17), `:26609-26654` + `:26674` + `:26810-26824` (Cap-Refresh Warmth: 121/126 reps, the path-count-free pin threshold, the always-on pins), `:28770-28796` + `:29191-29199` (why the deletion has never been an arm).
- **Paper** (`docs/fec-arq-model.md`): `:4107-4113` (§12.8's correction naming the set asymmetry as the defect), §16.43/§16.44 (the three terms and the refuted fourth), §16.45/§16.46 (measured on the wire; the unsatisfiable criteria), §16.47 (the cap as a signed latency control), §16.51 (honest anchors, default ON), §16.52 (U's own c8 harm), §16.53/§16.54 (the two STOP RULEs; the harm is length-scoped), `:12408-12417` (capw makes the U bit a no-op), §16.55 (this ADR's pointer).
- **Commits**: `ac3bc9d` (2026-07-07, `gain = 2.0` and `boot = 128` arrive with the C2 bufferbloat fix), `5cace52` (2026-07-14, the whole path-scaled expression, default OFF), `5ebbcda` (2026-07-21, default ON in the consolidation battery).
- **Always-on pins relied on**: `path_scaled_store_cap_scales_value_and_ceiling_with_paths`, `store_cap_law_is_degree_one_in_the_anchor_until_the_knee_ceiling`, `empty_active_set_is_a_cliff_not_a_taper`, `pooled_unified_candidate_introduces_no_constant`, `the_pin_threshold_on_sigma_is_knee_over_gain_and_is_path_count_free`, `the_wires_measured_anchors_pin_both_legs_and_free_one_leg_at_both_duals`, `three_term_engine_law_is_the_bench_terms_at_the_anchors`, `three_term_span_vanishes_continuously_as_skew_goes_to_zero`.

## References

- **ADR-0058** — the pooled-vs-per-path decision this ADR does **not** re-open: its three-refinement chain defends ONE POOL and says nothing about the pool's size formula.
- **ADR-0052** (pre-registration shape of the battery in the Decision), **ADR-0060** (the release law that moved the c8 story and post-dates the knee sweep), **ADR-0061** (anchor hygiene — the sampler side of finding 6), **ADR-0066** (the register the `RWM_INFL_CAP` row would join; the "a refutation is only as good as its substrate" rule applied here to a confirmation), **ADR-0067** (the default-honesty rule, and why nothing flips here), **ADR-0069** (the shape this ADR copies: findings recorded, default pinned not flipped, deletion deferred to a named battery).
- **ADR-0064** / **CLAUDE.md, THE NO-MODE-SWITCH INVARIANT** — the composed law of Deliverable 2 satisfies it by construction (dials, no count keying, no topology predicate); this is a positive reason to prefer it, not merely a compliance note.
- **CLAUDE.md, FORMULA-FIRST LAWS**, and MEASUREMENT DISCIPLINE **17** and **18** — the rules this postmortem produced.
