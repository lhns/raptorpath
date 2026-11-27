# store-law-state: where the store law stands, what δ moves today, and the candidate on committed data

Status: STATE AND TEST, docs only. No engine change, no cargo, no VM. Branch `store-law-state` off main `e408e9fb`.
It is the first step of the owner's decision of 2026-11-19 ("we want the proper solution": the c8 delay gets a
derived store law, not lever B and not a smaller pool constant). Nothing here is applied to status, the paper, the
law ledger or the roadmap; §6 proposes that text.

**Answer in one paragraph.**
- **Derived and standing:** the store law is a *time* law. The update n′ = n·Σwᵢ·T*ᵢ/Σwᵢ·T̂ᵢ reads no rate, its fixed
  point is n* = Σᵢ λᵢ·T*ᵢ + H, H needs no term, the multi-leg target carries a coupled floor Φᵢ, and the gap
  horizon couples to the store as a transit condition (`store-law.md` §2, §3.1–§3.4, R6.2, R6.7).
- **Missing:** every input of T*. There is no per-window echo sampler on main, the floor F̂ and brake estimators
  were each convicted (E13, E14), and the allowance (F07) is UNDERIVED. Decision 8's g(δ)·S_F failed its
  second-cell test (§61a), and the sampler redesign waits on a measurement (M1) that has not run.
- **What δ moves today:** nothing. In the 152 sweep rows the cap sits at N·2048 on 97–100 % of LATE ticks at every
  δ (arm medians; single rows down to 0.86), and the only δ seat, (1 + q), multiplies an ask 3–34× above that ceiling.
  - So the in-order delay follows W/λ_src. At c2 it is 1.49–1.60 × W/λ_src on every arm, named Realtime included.
  - It *rises* with δ because repair lowers the source rate through a fixed store.
- **The candidate** (decision 8's allowance in the R6.7 line), evaluated unclamped on committed data:
  - **On the engine's logged inputs** (`btlbw`, `rtp`) it asks 6 300–58 000 symbols at c8 and 13 000–16 500 at c2,
    because `btlbw` over-reads the link 6–19× and `rtp` the floor 5–25×.
  - **On honest inputs** (netem rate and RTT, or §60's floors with the delivered source rate) it lands at
    c8 ≈ 510–540 in total (≈ 255–270 per path) and c2 ≈ 188–201 at Bulk.
  - Both are at or below §54b's working 640–1024 per path (W 1 280–2 048), above the §56 knees (c8 W ≈ 340, c2 168),
    and inside §56's flat regions.
  - **At c1s** it lands near W 300–350, below §56's smallest arm (W 384, 0.79 of control), so the cost is at least
    −21 % and not bracketed. c1d cannot be evaluated.
- **Units:** the (1 + r) wire-against-source mismatch is still on main (`d_received` counts every datagram). It
  reads 1.03 at Bulk and 1.22–1.32 at Auto in the sweep. A time law removes it, because the rate cancels.
- **Report fit:** the status entry's condition ("the holes outstanding under the law fit in one report") is two
  conditions.
  - (a) Every gap fits the 64-range SACK prefix.
  - (b) Every ripe loss hole fits the 20-gap horizon.
  - On §56's map, c8 carries 20 ranges per report at W 512 against 55–64 at W ≥ 1 280. At the candidate's point
    (a) holds about 3× and (b) about 7× on the measured κ (§7.3), so both constants become witnesses with two named bind gauges.
- **Owner question** (§5): one is live, the c1s/c1d price under decision 2.
  - The allowance's form (decision 8) waits on M1 by status's own record, so M1 is the critical-path item.
  - Whether a rate-free update meets the status entry's "formula in rate and delay" is a premise flag for the
    coordinator.

- **Follow-up (§7):** the c1s cost is real, ≈ −20 % at Bulk by the law's own fixed point (not an input artefact),
  and the receiver-bound term is open. τ_rel, one release interval, is a hypothesis only: as measured it is mostly
  an engine timer (LOOP_WAKE_US 1 ms, with the 2 ms gap-report floor behind it). Every cap is restated per
  path (c8 254–257 against the knee's 170; it sits at the lower edge of §56's flat region, 256–640 per path). κ is checked independently on §56's map
  (1.23 at W 512, rising to 1.9–3.1 at the edge). M1's plan is `m1-plan.md`.

Rules followed: FORMULA-FIRST (formulas stated with a provenance per symbol and a shape check before any number);
NO-MODE-SWITCH (every form below is one expression continuous in δ; no hint or threshold branch); every clamp named
gets a bind gauge.

Scripts: stdlib Python, run with `python -I`, inlined in the Appendix with their outputs (this branch adds one file
only). Inputs: `docs/l1-raw/s51b/full/b51c{2,8}-full.tgz`, `docs/l1-raw/s54/rep54b-diag-ld.tgz`,
`docs/l1-raw/s56/rep56-diag-ld.tgz` (extracted to a scratch directory), and the committed outputs
`store-law/little54b.out` and `store-law/r68allow.out`.

---

## 1. Where the store law stands

### 1.1 Derived (and where)

| result | statement | source |
|---|---|---|
| Little per leg | W − H = Σᵢ λᵢ·Tᵢ; on §54b T_W = W/Σλ and the rate-weighted echo loop agree within 1–3 % | `store-law.md` §1, `little54b.out` |
| no rate needed | any n′ = λ̂·T* fed by the store's own delivery is n′ = (n − H)·T*/T̄; the rate cancels, and the shipped law's collapse at c2 is T* < T₀, a time-target defect | `store-law.md` §2 |
| the update | n_{k+1} = clamp(n_k·Σwᵢ(Φᵢ + Aᵢ + Ŝᵢ)/ΣwᵢT̂ᵢ, floor, N·knee), G = 1 (derived: makes T̄ = T* the fixed point) | §3.1, R6.2, r67 §5 |
| uniqueness | one crossing iff d(T̂min − A)/dn > 0 on the legs carrying the weight | R6.2; checked in r68 §1.4 |
| holes | H is implicit: it changes the loop gain 1 − (1 − H′)·n/(n − H), not the fixed point | §3.4 |
| coupled floor | Φᵢ = F̂ᵢ + ½·max(0, maxⱼF̂ⱼ − F̂ᵢ): under ρ = 1 a faster leg may queue toward the slowest leg's floor; the ½ comes from symmetric shaping | §5.2, R3.3, §56 Addendum |
| horizon coupling | old holes accumulate above a transit P_h ∝ G_h/ε_run; c8 P_h(20) ∈ (1.9 k, 2.5 k) | §3.4 |
| δ's seat | q(δ) = (b(δ) + 1)/30, cited RFC 8289 §3.2, continuous and monotone | `store_cap.rs:130`, R6.3 |
| shape | order 0 in N for the update, order 1 for the fixed point; dimensionless, so in the store's own unit; no ρ input; one expression at every N | §3.2 |

### 1.2 Inputs with no engine source

The plan named four: L̂, the time term, b₀ and δ's seat. In the repo they appear as `store-engine-form-impl`'s
done-criterion ("reads an engine L̂ and time term from the per-leg state"), ledger F01 item 7 ("b0 offset has no
engine source") and W1's U2. Their state, and that of the terms the derivation added:

| input | plan's name | state on main | state of the derivation |
|---|---|---|---|
| a rate L̂ | L̂ (E04) | Copa `max_bw`, 6–19× the link (§3.2 below) | **removed by derivation, not sourced**: the update reads no rate (§2). A *static* readout of n* still needs the delivered source rate |
| T̂ᵢ, T̂min,ᵢ, wᵢ (250 ms windows) | the time term | **none.** `WinSampler` exists only on `slaw-gauge` `d416d59b` (`net/slaw.rs`), never merged | sampler-design `3375acd9` proposes the window *median* in place of the minimum; nothing adopted |
| the floor F̂ᵢ | (part of the time term) | Copa `min_rtt` (E03), 5–25× the floor at load | candidate C (r67 §5: median of the K lowest-n window minima) is the standing candidate; ℓ̂ (E13, DISHONEST-INPUT) is out |
| the brake b₀ | b₀ | none | b̂₀⁺ is E14 (UNDERIVED, DISHONEST-INPUT: latched in boot, one window sets F for a row). count-rule `2d557191` shows it is the law's own steady-state brake (3.4–8.4 ms at c3). No estimator stands |
| the allowance Aᵢ | (U2) | none | F07 UNDERIVED. Decision 8 set g(δ)·S_F + q(δ)·F̂, g(Bulk) = 1. §61a measured need/S_F = 0.46 at c3 against 0.80 at c2, so ρ DISAGREES. The re-derivation closed with decision 8 "neither confirmed nor refuted" |
| δ's seat | δ's seat | q(δ) in `codel_setpoint_q`, but it multiplies an ask the clamp discards (§2) | q(δ)·F̂ plus g(δ); g's Realtime end is unmeasured |
| Ŝᵢ = T̂ᵢ − T̂min,ᵢ | (spread) | none (`qsp_us` is P90 − P50 of 256 echoes, not a window statistic) | comes with the sampler |
| n₀ = 128, T_w = 250 ms, K | — | `RWM_STORE_BOOT` 128 (no provenance) | declared, open (Q7, Q8; R6.7 item 4) |
| the coupling's ½ | — | — | rests on symmetric shaping; its falsifier (asymmetric c8a/c8b) never ran; sampler-design's blocking item 2 deletes the coupling with no replacement |

### 1.3 Branches

| branch | head | content | on main? |
|---|---|---|---|
| `perleg-design` | `9e03d84a` | W3.2 design doc `w3-per-leg-state.md` (the per-leg window Ŵᵢ(δ) = (1 + q(δ))·b̂ᵢ·m̂ᵢ, the (1 + r) note, §6 questions) | no (listed under `infra-derivation-merges` as unmerged docs) |
| `perleg-phase-a` | `e5246d7f` | Phase A, A0–A4 (2026-11-16..17): `leg_state_trace_pin`, one `LegState.s`, m̂/b̂/sampler/J1–J3/wire RTT moved into `LegState`, the `[LEG]` gauge; zero behaviour change, 6 commits ahead of main | no; waits for the one stacked zero-behaviour guard (owner decision 5); node `place-per-leg-relocate` in-progress |
| (Phase B + W3.3) | — | consumers onto `LegState` and the store law's engine form | not started: `place-per-leg-impl` and `store-engine-form-impl` are blocked; `store-engine-form-impl` depends on `place-per-leg-impl` |
| `slaw-gauge` | `d416d59b` | `[SLAW]` gauge, `WinSampler` (ℓ̂, b̂₀⁺) | no; "reach main through a stacked guard, or be dropped" is still held |
| `derive-w1`, `w1r2` | `ed4e3405`, `22574061` | `w1-who-limits.md` (candidate A, U1–U3) | no (docs) |
| `store-law-rev6`, `store-law-r67`, `r67-verdicts` | `5058abf0`, `c49c0a77`, `6794780d` | `store-law.md`, `-r67.md`, `-r68.md` | yes |
| `knee-margin-rederive`, `count-rule`, `sampler-design` | `c6c55958`, `2d557191`, `3375acd9` | the post-§61 re-derivations (`knee-margin-rederive.md`, `count-rule.md`, `sampler-design.md`) | yes (all three heads are ancestors of main) |

### 1.4 What pushed the work out

1. **2026-11-13–14:** §54b fixes the c8 anchor, and the lever-B guard is paused for the derivation. Revisions 3–6 follow
   on §56's maps. Two fitted models are withdrawn (R5.2, R5.4), and the derivation is frozen until new data.
2. **2026-11-14–16:** §57a/§57b run. Both are UNDERPOWERED or UNSCOREABLE: c1s collapses, and c1d does not reach its
   falsifying point.
3. **2026-11-15–16:** the B-1 gauge build (`slaw-gauge`) is never merged. §60 runs on its own tree and convicts the
   law's own inputs: E13 (ℓ̂), E14 (b̂₀⁺) and F07 (the allowance, SHORT 1.69 ms at c2).
4. **2026-11-17:** decision 8 (g·S_F) is tested at c3 (§61). ρ DISAGREES, the re-derivation cannot settle it, and
   s62 at J = 0 ends at NO-GO-GATE. count-rule finds no count rule. sampler-design proposes a median law that
   waits on M1 (four dump rows, about 1 h, no build), and M1 has not run.
5. **2026-11-17–19:** the VM went to engine defect F2 (§63, §64, merged 2026-11-18), the §65 delay re-check, and §51
   block E and block 51. The liveness audit then put the repair rate first and the c8 delay second.

The work was pushed out by the law's inputs, not its form. Every estimator of the floor, the brake and the knee
margin that met data failed, or reads one cell only. Meanwhile the VM time went to F2 and §51.

### 1.5 §43b's line, as status carries it

The brief's paraphrase ("a store time term took c2 delivered delay from 208 ms to about 19 ms for about 2 %
goodput") **is in status**, in the verdict table (`docs/status.md` §2, row "Oracle store law, re-tuned (§43b)"):

> c2 `CONVERGES-ONE-POINT`: both starts in band (191 / 187), Tp50 18–19.6 ms vs main ≈ 208 ms, goodput ≈ 2 % lower;
> diagnostic only, nothing ships

The archived Result (`docs/status/43-the-oracle-store-law-in-closed-loop.md`, §43b Result) qualifies it:
"**About 2 % lower, not resolved at n = 3**" (−1.68 against a 2σ of 2.00), "Delay: Tp50 18–19.6 ms against MAIN's
≈ 208 ms (MAIN pinned at 2 048)". The form was G·ΣL·(RTprop + S + τ) with an **oracle L from netem**, G = 0.9 and
τ = 5 ms, neither provenanced. It is not the law, only its existence proof.

### 1.6 Ledger rows (`law-ledger.md`)

E04 `DISHONEST-INPUT` (rank 1) · E03 `DISHONEST-INPUT` (2) · F01 pooled store cap `PINNED(c8,c1s), DISHONEST-INPUT`
(3) · E13 (4) · E14 (5) · F07 (6) · C02 `UNDERIVED, DEGENERATE(c8)` (7). F01 ships with E03 and E04 as one change, and
F07 with F01.

---

## 2. What δ moves in the store cap today

### 2.1 The code

`refresh_store_cap` (`net/sender_phases.rs:60`) runs every 5 ms. With the default chain (no Copa feed, three-term
off) it calls `pooled_store_cap` (`net/store_cap.rs:76`) on `store_cap_pool_inputs` (`net/channel_set.rs:125`), which is
Σᵢ `copa_bdp_anchor` = Σᵢ max_bwᵢ·min_rttᵢ:

```text
   cap  =  clamp( (1 + q(δ)) · Σᵢ max_bwᵢ · min_rttᵢ ,  floor 10 ,  N · 2048 )        q(δ) = (b(δ) + 1)/30 ∈ [0.05, 0.10]
```

δ enters only as the factor 1 + q(δ), from 1.05 to 1.10: a ±2.3 % lever on the ask. `[DCAP]` echoes q and b, and
`[SUMCAP]`/`[DCAP]` print `pin=` (the share of refreshes at the ceiling), the mean cap and the mean unclamped ask.

### 2.2 The sweep's readings (`sl51.py`, `pinwhere.py`)

Arm medians over rows. pin, cap and ask come from the whole-row `[DCAP]` line; W and bind are LATE ticks
(t ≥ 15 s, src > 0).

| cell | arms | q | row `pin=` | row mean cap | row mean ask | LATE W | LATE bind | ask / ceiling |
|---|---|---|---|---|---|---|---|---|
| c2 | HB, BM, BP (Bulk) | 0.100 | 0.78–0.80 | 1 766–1 768 | 8 804–11 301 | 2 048 | 1.000 | 4.3–5.5 |
| c2 | I1, I2, I3 | 0.096–0.081 | 0.72–0.82 | 1 725–1 792 | 8 669–10 433 | 2 048 | 0.97–1.00 | 4.2–5.1 |
| c2 | AM, NA, HA, AP (Auto) | 0.067 | 0.83–0.84 | 1 805–1 818 | 11 112–12 451 | 2 048 | 1.000 | 5.4–6.1 |
| c2 | I4, RM, RP | 0.057–0.050 | 0.82–0.84 | 1 789–1 829 | 11 714–13 093 | 2 048 | 1.000 | 5.7–6.4 |
| c2 | HR (named Realtime) | 0.050 | 0.86 | 1 965 | 12 088 | 2 048 | 1.000 | 5.9 |
| c8 | HB, BM, BP | 0.100 | 0.74–0.75 | 3 497–3 580 | 12 358–13 845 | 4 096 | 1.000 | 3.0–3.4 |
| c8 | I1, I2, I3 | 0.096–0.081 | 0.75–0.80 | 3 503–3 646 | 13 640–33 507 | 4 096 | 1.000 | 3.3–8.2 |
| c8 | AM, NA, HA, AP | 0.067 | 0.80–0.82 | 3 650–3 700 | 26 759–36 397 | 4 096 | 1.000 | 6.5–8.9 |
| c8 | I4, RM, RP | 0.057–0.050 | 0.81–0.82 | 3 691–3 716 | 26 333–35 346 | 4 096 | 1.000 | 6.4–8.6 |
| c8 | HR | 0.050 | 0.93 | 4 026 | 138 216 | 4 096 | 1.000 | 33.7 |

- **Yes, `pin=` varies with δ, but only through the boot ramp.** The whole-row `pin=` (0.72–0.93) is below 1 only
  because it counts start-up: every row first reaches the ceiling at t ≈ 10.3 s (c2 10.3–15.3 s), about when Copa's
  10 s `min_rtt` window first turns over. After that it is pinned on 88–100 % of ticks, and on 97–100 % of LATE ticks.
  The remaining unpinned ticks sit in t < 15 s and the completion tail. That agrees with §54b (bind 0.997–1.000) and
  `p0-dwell-diag.md` §0 ("clamped at the ceiling from t ≈ 10.3 s on every row of every arm").
- **The cap is N·2048 at every δ point in both cells.** The realized store does not move with δ.
- **The ask rises as q falls,** and q is not why. From Bulk to Auto at c8, `btlbw₀` goes 109 k → 182 k /s, `rtp₀`
  127 → 206 ms and `rtp₁` 39 → 268 ms; at c2, `rtp` goes 191 → 211 ms.
  - Repair adds wire datagrams to the per-path received counter that feeds `max_bw`.
  - A fixed store at a lower source rate lengthens the loop that `min_rtt` takes in.
  - q's own effect is 1.05/1.10 = −4.5 %.

### 2.3 Does the store explain the in-order delay's shape along δ? Yes (`inorder.out`)

With the store pinned at a fixed symbol count, Little gives the loop as T ≈ W/λ_src.

| cell | arm | W/λ_src ms | in-order ms | in-order ÷ (W/λ_src) | W/λ_src rel. Bulk | in-order rel. Bulk |
|---|---|---|---|---|---|---|
| c2 | HB | 214 | 340 | 1.59 | 1.00 | 1.00 |
| c2 | I3 | 230 | 363 | 1.58 | 1.07 | 1.07 |
| c2 | HA | 254 | 403 | 1.59 | 1.19 | 1.19 |
| c2 | RP | 261 | 404 | 1.55 | 1.22 | 1.19 |
| c2 | **HR** (512 B symbols) | 119 | 187 | 1.57 | **0.56** | **0.55** |
| c8 | HB | 358 | 4 714 | 13.2 | 1.00 | 1.00 |
| c8 | HA | 457 | 5 470 | 12.0 | 1.28 | 1.16 |
| c8 | RP | 463 | 6 436 | 13.9 | 1.29 | 1.37 |
| c8 | HR | 190 | 1 704 | 8.9 | 0.53 | 0.36 |

- **c2:** in-order delay is 1.49–1.60 × W/λ_src on all 14 arms. It rises 16–19 % from Bulk to Auto because the source
  rate falls (repair r 0.22–0.25) through a fixed store, not because the store moves.
- **c2 HR, the only arm whose delay falls,** is the same law. The named hint halves the symbol to 512 B. The pool
  counts symbols, so λ_src in symbols doubles and the store's time halves (0.56 predicted, 0.55 measured). Status §51
  reads "only the named Realtime hint lowers delay". That is the pool constant counted in symbols, not a δ effect.
- **c8:** in-order delay is 10–14 × W/λ_src. The multiplier is waiting behind old holes (§54b: 1 289 at the shipped
  pool). That the 64-range prefix adds to it is a hypothesis (§4.3), not a reading. Its rise to Auto (+16 to +37 %) tracks W/λ_src (+28 %), inside c8's 50 % in-order resolution.
- So in-order delay does not fall with δ because the store does not move with δ. It cannot fall while the cap is
  pinned, and the pin is set by inputs that over-read by 37–435× in product (§3.2), not by δ.

### 2.4 The (1 + r) unit mismatch: still on main

- `max_bw` is fed by `record_delivery(d_received)` (`net/control_msg.rs:565` → `scheduler/mod.rs:362` →
  `scheduler/path.rs:485` → `scheduler/copa.rs:746`). `d_received` is the delta of `cum_received`, which is
  `PathBatchTracker::total_received`, every datagram the path delivered (`transport/protocol.rs:494`). That includes
  source, retransmits and coded repair.
- The cap bounds the store's count of retained source seqs.
- So the shipped cap compares a wire-unit Σ b̂·m̂ with a source-unit count, as W3 §2 item 8 found.
- **Size in the sweep** (wire tx ÷ source rate): c2 1.03 (Bulk), 1.22 (Auto), 1.25 (δ 51); c8 1.03 (Bulk), 1.32 (Auto).
- **In the candidate the mismatch cannot arise:** the update reads only times (§3.1). A static readout of n* must use
  the source rate λ_src, never `max_bw` or the wire tx rate.

---

## 3. The candidate, tested on committed data

### 3.1 Formula, provenance and shape

The best-supported candidate is R6.7's line (`store-law-r67.md` §5) with decision 8's allowance (r68 §1.4/§2.4,
owner decision 8). The update is per 250 ms window k:

```text
   n_{k+1}  =  clamp( n_k · Σᵢ wᵢ,ₖ·( Φᵢ + Aᵢ(δ) + Ŝᵢ,ₖ ) / Σᵢ wᵢ,ₖ·T̂ᵢ,ₖ ,  floor ,  N·knee )

   Φᵢ    =  F̂ᵢ + ½·max(0, maxⱼ F̂ⱼ − F̂ᵢ)                    coupled target floor
   Aᵢ(δ) =  g(δ)·S_F,ᵢ + q(δ)·F̂ᵢ                            the allowance (decision 8)
   Ŝᵢ,ₖ  =  T̂ᵢ,ₖ − T̂min,ᵢ,ₖ                                window spread
```

> *Sentence:* once per window, scale the store by the ratio of each leg's target loop to its measured loop,
> weighted by delivered share. A leg's target is its coupled queue-free floor, plus a knee margin, plus a δ-priced
> fraction of the floor, plus the window's own spread.

**Its fixed point is the rate × delay formula the status entry names** (owner decision 2026-11-19, status's
"how much each path may hold outstanding, as a formula in that path's measured rate and delay and in δ"; the owner's
own words quoted there are "we want the proper solution"). At R = 1, by Little:

```text
   n*  =  Σᵢ λᵢ · T*ᵢ(δ)  +  H ,        T*ᵢ(δ)  =  Φᵢ + g(δ)·S_F,ᵢ + q(δ)·F̂ᵢ + Ŝᵢ          (λᵢ: delivered SOURCE rate of leg i)
```

So per path, the store holds the path's measured delivered rate times a target loop built from the path's measured
floor and spread and the dial. The law never has to *read* λᵢ: the update is the ratio form, and λ cancels. Whether
that meets the status entry's sentence is flagged in §5 (item C).

| symbol | meaning | provenance | engine source |
|---|---|---|---|
| T̂ᵢ,ₖ, T̂min,ᵢ,ₖ, wᵢ,ₖ | median, minimum and count of leg i's echo samples in window k | measured | **missing** (`slaw-gauge` only) |
| F̂ᵢ | median of T̂min over the K lowest-n windows | measured, candidate C (r67 §5); K ∈ [3, 11] open | **missing** |
| Φᵢ | coupled floor | derived from ρ = 1 in-order delivery (§5.2); ½ from symmetric shaping (§56 Addendum) | from F̂ |
| S_F,ᵢ | floor windows' median − minimum | measured (r68 §1.4); refuted as *the knee's term* at c3 (§61a: 0.46 against 0.80) | **missing** |
| g(δ) | knee-margin multiplier, g(Bulk) = 1 | owner decision 8; Realtime end "set by measurement", not measured | none |
| q(δ) | (b(δ) + 1)/30 | cited, RFC 8289 §3.2 | `codel_setpoint_q` (present) |
| floor 10, N·knee | resource bounds | declared | present; gauges `[SUMCAP] floor=` / `pin=` |
| T_w = 250 ms, n₀ = 128 | window, boot | declared, no provenance (Q7, Q8) | n₀ present |

**Shape check (the sentence against the expression).**
- **Order in N:** the update is a weighted mean, order 0; the fixed point Σᵢλᵢ·T*ᵢ is order 1; the clamp is linear in
  N. No path-count branch.
- **Units:** R is time ÷ time; n stays in source seqs; T* is in ms.
- **Monotone in δ:** T* is non-decreasing in q (and in g), and q is continuous and monotone in b(δ).
- **ρ:** none. **Mode switch:** none; one expression, with δ only through q and g.
- **What the sentence does not yet say:** why S_F is the knee margin. §61a found it is not the same fraction of S_F
  at c3 as at c2. That gap is F07, and §5 item B.

### 3.2 The inputs: logged, netem truth and §60 (`cand2.py`, `little54b.out`, `sl51-*.out`)

netem truth from `tools/l1/lib.sh` `scenario_params`:
- c2 is `100mbit 5 3` (10 ms RTT, ±3 ms jitter each way);
- c3, c8's leg 1, is `20mbit 20 5` (40 ms);
- c1 is `1gbit 1 0`.
- At 1 270 B per datagram on the wire (from c2's saturated tx 9 840 /s), the links carry about 9 843, 1 969 and
  98 425 datagrams/s.

| input (c8 leg 0 / leg 1; c2) | §54b B640 | §54b B2048 | sweep HB (Bulk) | sweep HA (Auto) | netem truth | §60 measured |
|---|---|---|---|---|---|---|
| `btlbw` (Copa max_bw) /s | 75 228 / 21 026 | 114 713 / 22 272 | 108 506 / 24 062; c2 58 870 | 182 410 / 28 583; c2 70 324 | 9 843 / 1 969; c2 9 843 | delivered ≈ link |
| over-read ×link | **7.6 / 10.7** | 11.7 / 11.3 | 11.0 / 12.2; c2 6.0 | 18.5 / 14.5; c2 7.1 | 1 | — |
| `rtp` (Copa min_rtt) ms | 43 / 38 | 130 / 40 | 127 / 39; c2 191 | 206 / 268; c2 211 | prop 10 / 40; c2 10 | F̂ 8.79 / 37.36; c2 8.68 |
| over-read ×F̂ | **4.9 / 1.0** | 14.8 / 1.1 | 14.4 / 1.0; c2 22.0 | 23.5 / 7.2; c2 24.3 | — | 1 |
| echo `rtt` ms | 96 / 146 | 198 / 235 | 204 / 219; c2 207 | 292 / 643; c2 248 | — | — |
| quinn `wrtt` ms | 91 / 133 | 118 / 222 | 104 / 202; c2 132 | 127 / 626; c2 122 | — | — |
| `est=` | Y on 13 020 of 13 020 ticks | | Y on 33 052 / 18 259 of all c8 / c2 ticks | | | |

- **`btlbw` over-reads the link 6–19×** on every row (E04).
- **`rtp` over-reads the floor 5–25×** on the fast leg, and on the slow leg too once repair runs (7× at c8 Auto)
  (E03). It is a 10 s minimum of a loop that never drains.
- **`wrtt` includes the netem queue whenever W < quinn's window,** so it is no floor either. Quinn's lifetime
  `rtt.min()` reads propagation (6.2–6.9 ms at c8 p0, `store-law.md` §3.3), but it sits on the wire clock and needs
  an echo-minus-wire offset that has no source.
- `est=Y` holds everywhere, so the over-read is not a warm-up artefact.
- **The plan's flag is confirmed:** max-bandwidth and min-RTT over-read on every committed row, the product by
  10¹–10². No formula fed these two inputs can be the law.

### 3.3 The candidate's cap at each point, unclamped, against what works (`cand2.py`)

- n* = Σλᵢ·T*ᵢ is evaluated on three input sets:
  - **ENGINE:** λ = `btlbw`, F = `rtp`;
  - **NETEM:** λ = the link rate × 1/1.032 source share, F = the propagation RTT;
  - **S60:** λ = the delivered source rate, F = §60's F̂.
- S_F and Ŝ come from §60 in all three (ENGINE gets its best chance). Ŝ at the landing is c2 5.0, c8 leg 0 11.1 and
  leg 1 16 (10.0–23.5, not bracketed).
- The static Little form is exact only where λ is the delivered rate at the landing, i.e. above the knee. For
  landings below the knee the replay crossings of §60/§61b are quoted instead.
- Totals are for all paths; per-path values are total ÷ N.

| point | q | shipped ask (logged) | realized cap | n* ENGINE | n* NETEM | **n* S60, g = 1** | n* S60, g = 0 | what works (measured) |
|---|---|---|---|---|---|---|---|---|
| c8 §54b B640 | 0.10 | 4 612 | 1 280 | 6 282 | 539 | **508** (254 / path) | 437 | §54b: B640 and B1024 (W 1 280–2 048, **640–1 024 per path**) at 106.9 Mbit/s; §56 knee 0.95 at W ≈ 340, flat to W 512 (106.0 against 106.5) |
| c8 §54b B2048 | 0.10 | 17 589 | 4 096 | 20 865 | 539 | 511 | 440 | (the pinned control) |
| c8 sweep HB | 0.10 | 12 358 | 4 096 | 19 622 | 539 | **514** | 443 | as above |
| c8 sweep HA (Auto) | 0.0667 | 35 199 | 4 096 | 57 740 | 534 | **399** | 343 | knee rescaled by λ_src: ≈ 340 × 0.78 ≈ 265 |
| c8 sweep RP (δ 51) | 0.050 | 26 333 | 4 096 | 39 545 | 531 | **392** | 337 | ≈ 263 |
| c2 sweep HB | 0.10 | 8 804 | 2 048 | 12 949 | 201 | **188** (replay 198–203) | replay 70 | §60 W₉₅ 168; §56 flat to 192–256 (−0.7 %) |
| c2 sweep HA | 0.0667 | 11 815 | 2 048 | 16 522 | 198 | **156** (r68 replay at Auto's q on §60's Bulk-rate rows: 196, a different operating point) | — | knee rescaled ≈ 168 × 0.84 ≈ 142 |
| c2 sweep RP | 0.050 | 11 751 | 2 048 | 15 581 | 196 | **151** (same replay at q 0.05: 194) | replay 51 (§61b, Bulk rate) | ≈ 138 |
| c1s (link rate) | 0.10 | — | 2 048 | — | 308 | **349** | 307 | §56: W 384 → 0.79, W 512 → 0.89 of control, no plateau |
| c1d | — | — | 4 096 | — | — | **not evaluable** | — | no floor calibration; see §3.5 |

Reading:
- **On the engine's inputs the candidate is no better than the shipped law** (6 300–57 700). The defect is the
  inputs (E03, E04), not the form. This is the table's "no engine source" evidence.
- **On honest inputs it lands inside the measured working region at c2 and c8.**
  - c8: 508–539 in total, 254–270 per path. That is at or below §54b's working 640–1 024 per path, 1.5× above §56's
    knee, and at §56's W 512 point: goodput −0.5 %, in-order late mean 0.09 s against 0.37 s at W 1 280 and 4.4 s at
    the shipped pool.
  - c2: 188 by the static form, 198–203 by §60's replay. That is 1.1–1.2× the knee, inside §56's flat region, with
    app-echo rtt 19 ms and in-order 0.02 s at W 192, against 208 ms and 0.41 s at the shipped pool.
- **NETEM and S60 agree within 6 %** at both cells. The propagation RTT and the echo floor F̂ differ (10 against
  8.8 ms; 40 against 37.4 ms), but the coupled half-gap and S_F dominate T*.
- The r68 replay at c8 crosses above every arm up to 512, with R̃'s chord to the 1 280 control at 509–571. The
  static form's 508–514 agrees with it.
- **g = 0, the "δ-free margin removed" bracket:** c2 lands at 70 at Bulk and 51 at Realtime (§61b's landing table,
  u 0.45 / 0.34), far below the knee. c8 lands at 337–443 by the static form, near the knee. So whether g(Realtime)
  is near 1 or near 0 is a 3–4× difference in the c2 store (§5, item B).

### 3.4 Off Bulk: q's share against the rate's share

Compared in loop time, q moves the candidate's target very little:
- c2 Tmin*: 14.6 ms (Bulk) → 14.3 (Auto) → 14.2 (δ 51);
- c8 leg 0: 29.4 → 29.1 → 29.0 ms; leg 1: 51.1 → 49.9 → 49.2 ms.

In slots, n* falls 17–24 % from Bulk to Auto:

| cell | n* Auto / Bulk | λ_src Auto / Bulk (the rate's share) | T* Auto / Bulk (q's share) |
|---|---|---|---|
| c2 | 0.83 | 0.84 | 0.985 |
| c8 | 0.78 | 0.78 | 0.99 |

- Almost all of the drop is the source rate falling under repair, which a time law passes through correctly: the same
  delay with fewer source slots.
- q's own authority is ≈ 1.5 % of T*, about 0.3 ms at c2.
- That is W1's U2 and r68 §4 again: with g ≡ 1, δ prices about 3–5 slots at c2. With decision 8's g(δ) it prices up to
  the S_F term (≈ 5 ms, about 3×) once g's Realtime end is measured.

### 3.5 c1s and c1d: where the candidate costs

- **c1s** (1 Gbit/s, 1 ms, no jitter; §60 F̂ 2.38 ms, S_F 0.43 ms): T* − F̂ ≈ 0.67 ms, so n* ≈ 300–350.
  - §56's smallest c1s arm is W 384, at 0.79 of control (0.89 at W 512). The landing lies below the map, so it is
    not bracketed.
  - The cell is receiver-bound (R6.1), and its goodput keeps rising with standing queue up to the control.
  - **Expected cost: at least −21 % at Bulk, unbracketed below.** This is the price owner decision 2 named ("the
    price sits at c1s") without sizing it. The static form also uses the link rate as λ, which over-states λ
    below a knee, so the true landing could sit lower still.
- **c1d:** §60 has no c1d floor calibration, so the static form cannot be evaluated.
  - On §57b's q_ℓ axis, the candidate's ≈ 0.7 ms of queue above the floor sits below c1d's smallest store-bound
    pool (W 384, q_ℓ 2.6 ms, ratio 0.80–0.85).
  - It would land at or below W 384, and §57b's within-replicate ratio there is 0.79–0.86.
- **Neither is a knee match.** §57b found no c1d knee to match (T UNSCOREABLE), and §56 found no c1s plateau.

### 3.6 What this section does not establish

- Every number is a static map crossing. §43 showed static reasoning fails in closed loop (c2 BISTABLE, c1s
  UNSTABLE), and §43b's closed-loop c2 point (191/187 at τ = 5 ms) is the only closed-loop store-time-law reading
  on record.
- The S60 column uses §60's floors measured on near-empty pools, with no engine estimator behind them (E13, E14). It
  shows where the law *would* land with honest inputs, not that any estimator can supply them.
- S_F's role is refuted at c3 (§61a). Its magnitude at c2/c8 is used here because decision 8 stands.
- The coupling's ½ is untested off symmetric shaping.

---

## 4. The report-fit condition

### 4.1 What one report carries (code)

- **The SACK prefix, M_S = 64** (`net/sack.rs:522`, `MAX_SACK_RANGES` = ⌊(1 155 − 72 − 48)/16⌋, derived from one
  control datagram).
  - A report lists received runs in [next_expected, seen] and stops at 64.
  - Seqs above the last listed range are "not reported (may be in flight)" (`sack.rs:533–540`), and `sack_to_gaps`
    cannot name a gap there.
  - **The store gate does not depend on the prefix** (wire v9). The gate's released count is
    max(|SACK marks|, `received_above` − not-retained), clamped to the retained count (`store_released_for_gate`,
    `sack.rs:386`; seat `net/mod.rs:2300–2310`). So slots of received seqs past the 64th run *are* returned to the
    pool by count; what the prefix loses is per-seq visibility.
  - The number of runs in the window equals the number of gaps below `seen`.
- **The gap horizon, M_N = 20** (`net/mod.rs:225`, `MAX_NACK_GAPS`). `sack_to_gaps` (`sack.rs:79`) acts on the 20
  *oldest* gaps of a report, and the receiver's `RepairRequest` list is cut the same way.

### 4.2 Measured: ranges per report against the store (`sacki.py`)

This is the mean over each row of `[SACKI] ranges/reports` (boot included), as arm medians. A mean of a count capped
at 64 understates the binding share; no per-report distribution is logged.

| c8 W (total) | 160 | 224 | 320 | 512 | 768 | 1 280 | 2 048 | 4 096 |
|---|---|---|---|---|---|---|---|---|
| ranges / report | 2.9 | 4.2 | 7.2 | **20.2** | 39.6 | 53.2–54.6 | 58.5–61.8 | 62.9–63.6 (and every sweep arm but HR: 63.2–63.6) |
| source | §56 | §56 | §56 | §56 | §56 | §54b, §56 | §54b | §54b, §51 sweep |

c2 (one leg) reads 1.2–1.6 at W 128–384 and 4.1–4.6 at W 2 048 on every δ arm. **At c8 the reports are dominated by
cross-leg gaps, not losses,** and they fill the 64-range prefix from W ≈ 1 280 up. Lever B's 64-gap horizon would
have been set to a report that is itself already full.

### 4.3 The count, by Little

The gaps below `seen` in one report are:

```text
   G(n)  =  G_re + G_loss + H_old

   G_re    =  Σᵢ (λᵢ/βᵢ) · ( d_fwd,ᵢ − minⱼ d_fwd,ⱼ )⁺        seqs of slower legs still in flight above the fast leg's newest arrival
   G_loss  =  κ · Σᵢ ε_run,ᵢ · λᵢ · T̄ᵢ  =  κ · Σᵢ ε_run,ᵢ · n_ᵢ     open loss runs: arrival rate × lifetime κ·T̄ᵢ
   H_old   =  holes older than the repair loop (present only when (b) below fails)
```

| symbol | provenance |
|---|---|
| λᵢ, nᵢ = λᵢ·T̄ᵢ | measured (Little, §1) |
| d_fwd,ᵢ | forward one-way delay incl. queue. **No engine source:** the echo is a loop. On symmetric netem it is the loop minus the return propagation; returns travel on their own leg (5 and 20 ms), as `store-law.md` §6 reads |
| βᵢ | mean run of consecutive seqs placed on leg i; ≈ 1 fits the map below; not gauged |
| ε_run,ᵢ | loss runs per datagram: `lib.sh` calibration 0.027/8 (c8 leg 0), 0.0407/4 (leg 1), 0.026/10 (c2) |
| κ | hole lifetime in loops, **read per row, never a constant**: κ = `[SUCC] rep_mean` ÷ the LATE rate-weighted echo loop (§7.3; 1.1–2.5 below the edge, 3.1 and 13 at and past it). (b) is evaluated with each row's own κ |

**Shape check.**
- **Units:** counts (rate × time).
- **Order in the store:** G_loss is order 1 in n, so (b) is a linear bound on the store. G_re depends on n only
  through the forward-delay gap, which the law's target sets (§4.4).
- **Monotone:** non-decreasing in every λᵢ, εᵢ and in the delay gap.
- **N = 1:** the sum in G_re runs over a one-element set, so d_fwd − min d_fwd = 0 by arithmetic, with no path-count
  branch. At c2 the model gives G_loss ≈ 0.7 against 1.3 measured at W 192, and ≈ 8 against 4.1 at W 2 048. So
  the single-leg term is right in order and off by up to 2×, and κ·ε is not calibrated at c2.

The two conditions:

```text
   (a) visibility:  G(n*)                      ≤  M_S = 64      every gap, so every received run, fits one report
   (b) horizon:     κ·Σᵢ ε_run,ᵢ·n*ᵢ + H_old    ≤  M_N = 20      every ripe loss hole is acted on in one report
```

- **(b) is self-consistent.** When it holds, holes are repaired within κ loops and H_old = 0. When it fails, unrepaired
  holes age and H_old grows. That is `store-law.md` §3.4's "binds = old holes accumulate".
  - With ε_eff = 4.5·10⁻³ (c8, rate-weighted), the (b) edge is n_h = 20/(κ·ε_eff) ≈ 2 200 (κ = 2) to 3 000 (κ = 1.5).
  - §3.4 measured P_h(20) ∈ (1.9 k, 2.5 k), between B1024 (no old holes) and B2048 (1 289). Only κ ≈ 2 is consistent
    with that: at κ = 1.5 `gapfit.py` puts B2048's G_loss at 17.0, below 20, while B2048 has old holes. The
    measured κ settles this without a fit: B1024 reads 3.1, so G_loss ≈ 3.1 × 4.5·10⁻³ × 2 048 ≈ 29 > 20. That is
    the edge, consistent with §3.4. So (b) is read as a gauge with measured κ (§7.3), not with a fitted constant.
- **(a) is the half §3.4 left implicit.**
  - §3.4 noted that "reports are truncated at every arm (visibility 0.30 even at W = 1 280) while H′ ≈ 0.09": (a)
    failing while (b) holds.
  - **Read in code:** when (a) fails, the store gate still releases the received seqs past the prefix by count
    (`received_above`, §4.1). What fails is per-seq visibility. A loss hole past the 64th run cannot be named by
    `sack_to_gaps` until the prefix advances, and the per-seq marks stop growing.
  - **Hypothesis, not read:** that this delays repair of holes past the prefix, and so adds to c8's in-order wait
    (§2.3). No gauge separates it from the horizon's old holes.
- **The status entry's condition** ("the holes outstanding under the law fit in one report") **is (a) ∧ (b).** That
  is stronger than §3.4's binding definition.
- **The model against the map** (`gapfit.py`, with no fitted constant, but two choices made: the return-path split,
  and κ = 1.5):

  | c8 W | 160 | 224 | 320 | 512 | 768 | 1 280 |
  |---|---|---|---|---|---|---|
  | G_re + G_loss (own-leg returns) | 3.5 | 5.7 | 8.8 | 15.6 | 44.2 | ≥ 64 |
  | measured mean | 2.9 | 4.2 | 7.2 | 20.2 | 39.6 | 53–55 |

  - It tracks the unsaturated map within about 35 %.
  - Assuming all reports return on the fast leg over-predicts 2–3×.
  - **It is not validated:**
    - it misses the large-store arms (B2048, sweep HB and L2048 read 63 against a predicted 26–44), where H_old and
      quinn's backlog make the medians uninformative;
    - it compares a model with a mean of a capped count;
    - β, d_fwd and the return split are not gauged.

### 4.4 At the candidate's fixed point

- **c8, n* ≈ 510.** The coupled target sets the forward-delay gap:
  - Δd*_fwd = (Φ₁ + A₁) − (Φ₀ + A₀) − Δd_ret = 14.3 + 7.4 − 15 ≈ 6.7 ms, so G_re ≈ 1 824 × 0.0067 ≈ 12.
  - G_loss ≈ κ × 4.5·10⁻³ × 510 with the measured κ = 1.23 at W 512 (§7.3) ≈ 2.8, so G ≈ 15. The map at W 512 reads
    20.2.
  - **(a) holds about 3×, (b) about 7×.**
  - **The coupling term is what keeps the report small.** Without the ½ (a per-leg target) the fast leg would not
    queue toward the slow leg's floor, the forward gap would be ≈ 21 ms and G_re ≈ 38. That still fits, at 1.7×.
- **c2, n* ≈ 190:** G ≈ 1.3 measured, so (a) holds about 50× and (b) about 25× (G_loss ≈ 0.7).
- **Shipped pool, W 4 096:** (a) is pinned at 63.4–63.6 and (b) fails (1 289 old holes at Bulk).
- **So under the candidate neither 64 nor 20 is load-bearing at c2 or c8.**
  - The 64 is already derived from the datagram size.
  - The 20 needs only M_N ≥ κ·Σε·n*, about 2.8 at c8 on the measured κ, so it has a margin of about 7× rather than being tuned.
  - Both become witnesses: a cell where a gauge below binds is outside the law's domain, and it is reported as a
    defect finding, not fixed by raising the constant.

### 4.5 The two bind gauges (owed; neither exists)

1. **SACK-prefix bind fraction**, receiver side, in `received_sack_ranges` (`sack.rs:545`): the share of WindowAcks
   that stop at `MAX_SACK_RANGES`, plus the count of gaps above the last listed range (the holes the sender cannot
   name).
   - This is (a)'s witness.
   - Predicted at the candidate: ≈ 0 at c2, and a minority of reports at c8 (mean 20 of 64; the tail is unknown).
2. **Horizon ripe-overflow fraction**, sender side, in `sack_to_gaps` (`sack.rs:79`): the share of reports cut at
   `MAX_NACK_GAPS` with at least one *ripe* loss hole beyond the 20th gap.
   - This is (b)'s witness.
   - It refines `store-law.md` §8's "truncation share", which counts visibility, not binding.
   - *Prediction, not a reading:* at c8 W 512 the 20 cut happens on many reports (mean 20 gaps), while the cut gaps
     are young cross-leg gaps, which sit at the high-seq end of the window.
   - Predicted ≈ 0 at both cells. The existing server `[RANK]` old holes and the `[HOLD] ripe_frac` stay as its
     cross-checks.

---

## 5. Owner questions

All three of the plan's questions have an answer on record, so none is re-asked as written:
- **U1:** decision 1 (2026-11-16): the store limit is in charge of latency, and the congestion controller is the
  safety net.
- **U3:** decision 2 (2026-11-16): a small Bulk cost is accepted for about 10× lower delay.
- **U2:** decision 8 (2026-11-17): g(δ)·S_F + q(δ)·F̂, g(Bulk) = 1.

Decision 8 is unsettled since §61a, and decision 2 was priced at c2 and c8 only. Of what remains open:
- **A** is the one live owner question.
- **B** is not askable yet: status records that the owner's question "waits for M1". It is the critical-path item,
  not a question.
- **C** is a premise flag for the coordinator, not an owner question.

**§54b, the anchor every question rests on** (c8-100, 600 MB, n = 6 per arm, store bind 0.997–1.000; ranges per
report from `sacki.py`):

| arm | pool per path (W) | goodput Mbit/s | in-order run / late s | p0 echo RTT ms | old holes | ranges / report |
|---|---|---|---|---|---|---|
| B2048 (shipped) | 2 048 (4 096) | 98.5 | 4.44 / 5.63 | 198 | 1 289 | 63.4 |
| B1024 | 1 024 (2 048) | 106.9 | 0.92 / 0.84 | 152 | 0 | 61.8 |
| B640 | 640 (1 280) | 106.9 | 0.38 / 0.35 | 96 | 0 | 54.6 |
| L2048 (lever B) | 2 048 (4 096) | 108.0 | 0.90 / 0.93 | 354 | 0 | 62.9 |
| L1024 | 1 024 (2 048) | 107.6 | 0.52 / 0.49 | 162 | 0 | 58.5 |
| L640 | 640 (1 280) | 107.3 | 0.32 / 0.30 | 98 | 0 | 53.2 |
| *candidate (this doc)* | *≈ 255 (≈ 510)* | *≈ 106 (§56 W 512)* | *late ≈ 0.09 (§56 W 512)* | *≈ 39 (§56)* | *0 predicted* | *≈ 16–20* |

**B (critical path, not asked now). δ's term after §61a: which allowance does the law carry?**
- *History:*
  - W1 asked U2 (q's 5–10 % band, or A(c) with an unsourced c_B ≈ 2.7 ms).
  - §60/r68 measured the band 1.69 ms short at the c2 knee (F07), and r68 offered (a) a δ-free spread, (b) δ scaling a
    knee-sized term, or (c) a wider band.
  - Decision 8 took (b) with S_F, g(Bulk) = 1 and g's Realtime end to be measured.
  - §61a measured need/S_F = 0.46 at c3 against 0.80 at c2 (ρ DISAGREES), and the re-derivation closed with decision 8
    "neither confirmed nor refuted".
  - sampler-design then offered (a) the minimum law with a count-matched floor, (b) a median law with S = IQR
    (recommended, conditional on M1), or (c) the median with S_F as decided. That question waits for M1 (four
    RTT-dump rows, about 1 h, no build), which has not run.
- *What this doc adds:*
  - With g ≡ 1, δ's authority over the candidate is ≈ 1.5 % of T* (§3.4).
  - Whether g(Realtime) is near 1 or near 0 moves the c2 store 3–4× (194 against 51).
  - So the allowance's form *is* the δ dial, and nothing in this doc settles it.
- *Next step (no question):* M1 is the store law's next measurement: four RTT-dump rows on prebuilt `e410d640`,
  about 1 h, no build, needing a tools-only plan that widens the dump arm. The sampler-design question goes to the
  owner after it, as status already records.

**A (live owner question). The c1s/c1d price under decision 2.**
- *History:*
  - W1's U3 priced the store's cost at ≈ 2 % at c2 (§43b).
  - Decision 2 accepted "a small Bulk goodput cost … for about 10× lower delay", citing c2 W 256 (no cost) and
    c8 W 512 (≈ 1 %), and noted that "the price sits at c1s, where §56 found that every pool cut pays".
  - `store-law.md` §0 predicted c1s −10 to −20 % and c1d failing guard v2.
- *What this doc adds:*
  - On honest inputs the candidate lands near W 300–350 at c1s. That is below §56's smallest arm (W 384, 0.79 of
    control), so the cost is at least −21 % and unbracketed.
  - c1d lands at or below W 384, where §57b reads 0.79–0.86.
  - c1 is receiver-bound (R6.1): goodput keeps rising with standing queue, so there is no knee for the margin to sit
    on.
- *Asked:* does decision 2's "small" cost extend to a Bulk cost of −20 % or more on the 1 Gbit/s cells? If not, the
  receiver-bound regime needs its own term in the one formula. That is a derivation item, not a branch, and it comes
  before any engine form.

**C (premise flag for the coordinator). Does a rate-free update meet "a formula in that path's measured rate and
delay"?**
- *History:*
  - The status entry for the owner decision of 2026-11-19 phrases the fix as "a formula in that path's measured rate
    and delay and in δ". The owner's own words quoted there are only "we want the proper solution".
  - `store-law.md` §2 derived that the law needs no rate input: the update is a ratio of times, and its fixed point
    is n* = Σλᵢ·T*ᵢ + H.
  - Every rate source on record failed (§23, §24, §27, §34, §36, §40, §43b), and E04 over-reads 6–19× here (§3.2).
- *What this doc adds:*
  - The rate-free update and the rate × delay fixed point are one law. The rate appears in the fixed point, but the
    engine never reads it.
  - Reading it would bring back E04 and the (1 + r) units (§2.4).
- *Flag:* the derivation reads the time-law form (the rate only in the fixed point and its gauge) as meeting that
  phrasing. If the coordinator reads it as requiring the engine to read an explicit per-path rate, E04's open input
  design (an honest delivered-rate sampler on a store-bound sender) becomes a prerequisite. That premise should be
  settled before the engine form is designed.

---

## 6. Proposed text (not applied)

**Status** (a new entry after the 2026-11-19 owner decision):

> **Store-law state (2026-11-19; `store-law-state` `<sha>`; docs and logs only).** The derived law is a rate-free
> time law, fixed point n* = Σλᵢ·T*ᵢ + H; none of T*'s inputs has an engine source (sampler, F̂, brake, allowance:
> E13, E14, F07; decision 8 unsettled since §61a). In the 152 sweep rows the store sits at N·2048 on 97–100 % of
> LATE ticks (arm medians) at every δ, and q multiplies an ask 3–34× above it, so δ moves nothing; c2 in-order delay is
> 1.49–1.60 × W/λ_src on every arm, named Realtime included (512 B symbols halve the store's time). On honest
> inputs the candidate lands at c8 ≈ 510 (≈ 255 per path, inside §54b/§56's working region) and c2 ≈ 190–200; on
> the engine's logged inputs it asks 6 300–57 700 (`btlbw` 6–19× the link, `rtp` 5–25× the floor). The (1 + r)
> mismatch is still on main. Report fit is (a) all gaps in the 64-range prefix and (b) all ripe loss holes in the
> 20-gap horizon; at the candidate both hold (c8 about 3× and 4–6×). Next: M1 (the sampler-design measurement); one
> owner question (the c1s/c1d price, at least −21 % at c1s).

**Law ledger** (row edits):
- **F01:**
  - Item 5 (measured in regime): add "δ inert: at every §51 sweep point the LATE cap is the ceiling and (1 + q)
    scales an ask 3–34× above it (`store-law-state.md` §2); in-order delay tracks W/λ_src".
  - Item 6: the c8 `pin=` range 0.74–0.98 is the whole row. From the first pin at t ≈ 10.3 s it is 0.88–1.00.
  - Item 7: add "units: wire-unit Σ max_bw·min_rtt against a source-unit store, ×1.03 (Bulk) to ×1.32 (Auto)".
- **C02:**
  - Item 4 (derivation): add "20 is bounded below by report fit (b), κ·Σε_run·n* ≤ 20 (`store-law-state.md` §4); a
    witness under the store law".
  - Item 6 (clamp bind): add "the ripe-overflow gauge (§4.5 item 2)".
  - Item 6's pending SACK-cap paragraph ("needs its own derivation or a verdict"): answer it with clause (a) and the
    prefix-bind gauge. `MAX_SACK_RANGES` is derived from the datagram and its binding is the store's (§4.2: 20 ranges
    at W 512, 55–64 from W 1 280), so it is a witness, not a constant to tune.
- **E03, E04:** add the §3.2 over-read factors on the sweep rows.

**Paper `fec-arq-model.md` §6.1** (a paragraph after `store-law.md` §11's draft):

> *Reports.* A store of n symbols puts G(n) = Σᵢ(λᵢ/βᵢ)·(d_fwd,ᵢ − minⱼ d_fwd,ⱼ)⁺ + κ·Σᵢ ε_run,ᵢ·nᵢ gaps below the
> receiver's highest seq. The law's point must satisfy G(n*) ≤ M_S (the SACK ranges one control datagram carries)
> and κ·Σᵢ ε_run,ᵢ·n*ᵢ ≤ M_N (the per-report gap horizon). Both bounds are witnesses with bind gauges, not
> parameters of the law. The coupled floor Φ keeps the first term small, because it pulls the legs' forward delays
> together.

**Roadmap** `store-law-derivation` notes: append "2026-11-19: state doc `store-law-state.md` (branch
`store-law-state`): candidate evaluated on committed data, report-fit condition derived; next M1; one owner question
(c1s/c1d price)". Add
a node for the two report gauges (§4.5) under the store track, `kind: instrument`, depending on
`store-law-derivation`.

---

## 7. Follow-up (coordinator, 2026-11-19): out of sample, one unit, κ, M1

**M1** is planned in `docs/derivations/m1-plan.md` (same branch).
- It needs plan code; the worker spec is §B there.
- It runs on the prebuilt gauge binary `e410d640`, not main's: main has `RWM_RTT_DUMP` but not `RWM_SLAW`, which the
  harness gates on.

### 7.1 Out of sample: c1s and c1d (§57a, §57b, §56; `c1oos.py`, `wakes.py`)

**What §57a and §57b add.**
- **§57a (the emission burst at c1s) is UNDERPOWERED.** Its paired within-replicate reading is that a larger burst
  *lowers* c1s's rate (−2.7 %, n 6). So the sender's emission burst is not what c1s's standing queue buys.
- **§57b (the c1d pool sweep) finds T UNSCOREABLE.**
  - c1d's ratios at the same total W read like c1s's: 0.954 / 0.950 / 0.888 / 0.798 against 0.96 / 0.94 / 0.89 / 0.79,
    at q_ℓ 7.3 / 4.8 / 3.0 / 2.6 against 6.8 / 4.7 / 2.7 / 2.1 ms.
  - Release batch / W rises 0.05 → 0.16, and ack wakes per loop fall from 14 to 5–7.
- Both cells are receiver-bound (R6.1): goodput keeps rising with standing queue up to the control, with no capacity
  at which a curve diverges.

**netem truth** (c1: `1gbit 1 0`, so 2 ms RTT, no jitter, 98 425 datagrams/s per path):
- **The static form n* = λ·T* gives two answers** depending on which λ it is fed:
  - with the link rate: c1s 266–308 per path; c1d 266–308 per path (532–616 in total, between §57b's W 512 and 768);
  - with the delivered rate at the landing (c1s 73 k/s at W 384; c1d ≈ 64 k/s in total at W 384): c1s ≈ 260–290,
    c1d ≈ 113 per path (226 in total), below both maps.
- **The static form is therefore not the test on a receiver-bound cell:** the link rate is never reached (the c1s
  control runs at 93 k of 98.4 k).
- **The test is the law's own fixed point,** where λ cancels: T̂min(n*) = F̂ + A. On netem truth A is even smaller,
  because S_F → 0 with no jitter; the measured 0.43 ms is host-side.

**The fixed point on the c1s map.**
- §60 logs c1s window minima only at W 64/128/192/2048, so T̂min(W) for W 384–1 024 is *estimated* as
  F̂ + (mean loop − T₀), with T₀ = 4.0 ms and R6.1's q.
- This assumes the whole distribution shifts with load, as r68 §1.2 measured at c2. It is flagged as an assumption.

| form | q | c1s landing W | goodput / control (§56, interpolated) |
|---|---|---|---|
| candidate, A = S_F + q·F̂ (0.67 ms) | 0.10 | ≈ 398 | ≈ 0.80 |
| candidate | 0.05 | < 384 (T̂min(384) ≈ 2.99 > target 2.93) | ≤ 0.79 |
| candidate + τ_rel (hypothesis, below) | 0.10 / 0.05 | ≈ 630 / ≈ 610 | ≈ 0.91 |

- **c1d:** there is no floor calibration.
  - By §57b's c1s-matching q_ℓ relation, the candidate's ≈ 0.7 ms above the floor lands at or below c1d's smallest
    store-bound pool: W 384 in total, 192 per path, ratio 0.80–0.85.

**Verdict: the c1s cost is real, not an input artefact. It is ≈ −20 % at Bulk (W ≈ 400) and ≥ −21 % toward
Realtime.**
- The rate cancels in the law, so E04 plays no part.
- c1s's floor is honest: the cell has no jitter, the floor slope is flat (b = 0.0009 ms/slot) and ℓ̂ sits within
  0.1 ms of F̂.
- netem truth makes the margin smaller, not larger.
- The earlier "≥ 21 %, unbracketed" (§3.5) came from the static form; the fixed-point reading puts it at ≈ 0.80.

**A hypothesis for the receiver-bound term, not the missing term: one release interval, τ_rel** (amended after
review). It was added after the candidate failed at c1s, it still costs ≈ 9 % there, and its landing rests on a
window minimum that §60 never logged at c1s for W 384–1 024. It is recorded as a hypothesis with four checks below,
and it is not part of the candidate.

```text
   T*ᵢ  =  Φᵢ + g(δ)·S_F,ᵢ + q(δ)·F̂ᵢ + τ_rel,ᵢ + Ŝᵢ          τ_rel = 1 / (rate of sender wakes ended by an ack)
```

- *The hypothesis's sentence:* the sender refills the store only when a report releases slots. Between two
  releases the bottleneck is fed only from what the store already holds beyond the loop.
- *Measured* (`wakes.py`, sender-global; sweep HB 1.15 ms):

  | cell | τ_rel (ms) by store W (total) |
  |---|---|
  | c2 (§56) | 1.31 / 1.26 / 1.21 / 1.19 / 1.18 / 1.17 at W 128 / 160 / 192 / 256 / 384 / 2 048 |
  | c8 (§56) | 1.22 / 1.04 / 0.89 / 0.82 / 0.85 / 0.87 at W 160 / 224 / 320 / 512 / 768 / 1 280 |
  | c1s (§56) | 1.00 / 1.20 / 1.68 / 1.48 / 2.34 at W 384 / 512 / 768 / 1 024 / 2 048 |
  | **c1d (§57b, total W)** | **0.93 / 1.11 / 1.40 / 1.73 / 1.96 / 2.24 at W 384 / 512 / 768 / 1 024 / 1 536 / 4 096** |

**(a) What the counter is, and whether an engine constant sets it.**
- `[DIAG] wake[ack]` counts the **sender loop's wakes ended by an ack arriving on its input channel**, cumulative
  (`net/diag.rs` :116–134, rendered by `wake_line` :1033). It is not a count of reports: several queued acks can end
  one wake.
- What the receiver sends:
  - under the ack merge, one WindowAck per data message (`window_ack_emission`, `net/sack.rs:482–489`: emit =
    advertise ∨ ack_merge);
  - gap-advertising reports at most once per **`GAP_ACK_MIN_INTERVAL` = 2 ms** (`net/mod.rs:236`; the gate is
    `receiver.rs:1956–1957`).
- **That 2 ms floor binds on every report:** `[LATE] sampler_bind=1.0000` on c2 HB s42 r1, c1s P512 s42 r3 and
  c1d P256 (server logs).
- The receiver loop's own timer is **`LOOP_WAKE_US` = 1 000 µs** (`net/recovery_laws.rs:29`, used at
  `receiver.rs:453`).
- On c2 and c8, τ_rel reads 0.8–1.3 ms across a 16× change in the store. That flatness is a clock's signature, and its
  value sits at LOOP_WAKE_US. The receiver's wake path has not been traced end to end, so the identification is not
  proven by reading. It is the likely reading.
- **Consequence: as measured, τ_rel is mostly an engine timer (1 ms, with the 2 ms gap-report floor behind it).**
  A store law that adds it absorbs an engine constant into the formula. That is the workaround shape the owner
  rejected on 2026-11-19 (a constant carried into the law instead of derived).
  - The part that is not a clock is c1s's and c1d's growth above 1 ms with W (1.0 → 2.3 ms). That is the receiver
    taking longer per batch as batches grow.
  - The defensible form, if any, is that growth, read with the timer's floor separated out. The 2 ms floor itself
    is a ledger item to add beside C02: a threshold that binds on 100 % of reports (`[LATE]`'s own comment says it
    turns its law into a constant).

**(b) Stability of the fixed point when τ_rel grows with the store.** One leg, window update n′ = n·R(n), with
R(n) = (F̂ + A + τ(n) + Ŝ) / (T̂min(n) + Ŝ) and Ŝ taken locally flat. The fixed point is T̂min(n*) = F̂ + A + τ(n*), and

```text
   dn′/dn |_{n*}  =  1 − G ,      G  =  (1 − H′) · n*/(n* − H) · n* · ( T̂min′(n*) − τ′(n*) ) / T̂(n*)

   stable and unique   ⟺   0  <  G  <  2       (lower edge: T̂min′ > τ′, R6.2's uniqueness; upper edge: no period-2 overshoot)
```

- **At c1s, on the estimates of the table above:** T̂min′ ≈ 2.02 / 256 ms per slot and τ′ ≈ 0.48 / 256 (W 512–768);
  n* ≈ 630, T̂ ≈ 6 ms and H′ ≈ 0. That gives G ≈ 630 × 0.0060 / 6.1 ≈ 0.62, stable.
- G falls toward 0 as τ′ approaches T̂min′. A store law with τ in it at c1s is *closer* to losing its restoring force
  than one without: G ≈ 0.85 without τ.
- This, not the landing, is the gate τ_rel must pass on measured T̂min′, because c1s's τ′ is the receiver's batching
  response to the store itself.

**(c) c1d:** τ_rel = 0.93 ms at W 384 total (192 per path), rising to 2.24 ms at the control. This is sender-global;
per leg it is about 2×, if the legs' acks interleave. It has the same shape as c1s: a ≈ 1 ms floor, then growth with the
store.

**(d) The c1s arm set in M1's session.**
- **Not in M1's launch with no plan code.** `plan60.py` validates slots against a fixed grid (`VALID`), and its c1s
  arms are W 64 / 128 / 192 / 2 048 only. One launch also runs one `LD_PLAN`.
- **The same binary in the same session: yes,** as a second table in `plan66.py` (`66c`) launched right after M1 on the
  same staged `e410d640`:
  - cell `c1s-400`, `LATE_T` 5 s, bytes `plan60.BYTES_DEFAULT[c1s]` (5 GB), timeout 210 s, gauge on, no dump;
  - plan 60's c1s collapse flag reported per row, not voiding;
  - arms placed per the placement rule (item 3, §7.2): **W 384** (≈ the candidate's landing, 398), **W 640** (the
    working-constant reference, and also where the τ_rel hypothesis lands), plus **W 256 and W 1 024** to bracket
    T̂min′ for (b). c1s has no knee to place an arm at (§56: no plateau).
  - n = 3 each (a third of §56's c1s rows collapsed), so 12 rows at ≤ 210 s, about +45 min.
- The spec is `m1-plan.md` §C.

### 7.2 Every predicted cap in one unit: per path

"Per path" means the pool convention (total ÷ N, the unit of `RWM_STORE_PATH_POOL` and of §54b's 640–1 024). For c8
the per-leg Little split nᵢ = λᵢ·T*ᵢ is shown beside it, since the store is pooled and leg 0 carries about 83 % of the
rate.

| point | candidate n* per path (S60, g = 1) | c8 per-leg split (leg 0 / leg 1) | knee per path | flat region per path | working per path (§54b) | shipped |
|---|---|---|---|---|---|---|
| c8 Bulk (§54b B640, sweep HB) | **254–257** (NETEM 270) | 385 / 122 | §56 0.95 point W 340 total → **170** | W 512–1 280 total → **256–640** (lower edge 106.0 against 106.5 Mbit/s) | 640–1 024 | 2 048 |
| c8 Auto / δ 51 | 196–200 | ≈ 300 / ≈ 97 | ≈ 133 (rescaled by λ_src 0.78) | — | — | 2 048 |
| c2 Bulk | 188 (replay 198–203) | — | 168 | 192–256 | — | 2 048 |
| c2 Auto / δ 51 | 151–156 | — | ≈ 142 / 138 | — | — | 2 048 |
| c1s Bulk | ≈ 398 by the fixed point (§7.1); the τ_rel hypothesis ≈ 630 | — | none (no plateau) | — | — | 2 048 |
| c1d Bulk | ≤ 192 (§57b's smallest store-bound pool); the τ_rel hypothesis: not evaluated | — | none found (§57b) | — | — | 2 048 |

- **The c8 comparison, redone in one unit:** the candidate sits at 254–257 per path. That is 1.5× §56's knee of 170
  per path, at the **lower edge** of §56's flat region (256–640 per path; nothing below 256 is flat), and 2.5–4× below §54b's working 640–1 024.
- So the earlier statement holds, now in matching units: the candidate lands above the knee and below the working
  constants, and §56's W 512 point gives its outcome (goodput −0.5 %, in-order 0.09 s).
- The per-leg split shows the slow leg holding only ≈ 122. That is why §56's knee is leg 1 starving (r₁ falls from
  1 853 to 188 below W 320 in total): the coupled floor Φ₀ is what keeps leg 0 from taking the whole store.

**Placement of future pool arms** (amended after review): at the knee, the candidate and 640 per path.
- The candidate is at the flat region's lower edge, so its margin to the knee is the measured quantity.
- Off Bulk the knee is *rescaled* by λ_src here, not measured, so the knee arm off Bulk is a placement estimate whose
  measured knee is the readout.

| cell, dial point | knee arm (per path) | candidate arm | reference arm | source of the knee value |
|---|---|---|---|---|
| c8 Bulk | 170 (W 340 total) | 256 (W 512) | 640 (W 1 280) | §56, measured |
| c8 Auto | ≈ 133 (W 266), estimate | 200 (W 400) | 640 | rescaled, **to be measured** |
| c2 Bulk | 168 | 192 (replay 198–203; §56 has 192) | 640 | §60 W₉₅, measured |
| c2 Auto | ≈ 142, estimate | 160 | 640 | rescaled, **to be measured** |
| c1s Bulk | none (no plateau) | 384 (landing ≈ 398) | 640 | — (plus 256 and 1 024 to bracket T̂min′, §7.1 (d)) |
| c1d Bulk | none found (§57b) | 192 | 640 | — |

### 7.3 κ: an independent check exists, and it agrees (`kappa.py`)

κ ≈ 2 was first read from §54b's old-hole onset, a fit. It is now withdrawn as a constant: (b) uses κ as measured
per row. An independent reading comes from committed data:
- the hole lifetime itself, `[SUCC] rep_mean` (server, holes closed by repair, measured from receiver detection to
  fill);
- over the LATE rate-weighted echo loop, on §56's c8 map, which has no old holes at any arm and was never used for
  the onset.

| c8 W (total) | 160 | 224 | 320 | 512 | 768 | 1 280 | §54b B640 | §54b B1024 | §54b B2048 (old holes) |
|---|---|---|---|---|---|---|---|---|---|
| κ = rep_mean / loop | 2.50 | 1.89 | 1.51 | **1.23** | 1.49 | 1.98 | 1.90 | 3.08 | 13.3 |

- c2 (§56) reads 1.08–1.55 at W 128–384.
- **It agrees with κ ≈ 2 at the edge:** κ climbs to 1.9–3.1 at W 1 280–2 048, where the onset sits, because holes
  start to wait at the horizon there.
- **κ is not a constant.** It rises toward the edge, a feedback the condition has to carry: it is (b) evaluated with
  its own measured κ, not a fixed one.
- **At the candidate's point (W ≈ 512) κ = 1.23,** so (b)'s margin is 20 / (1.23 × 4.5·10⁻³ × 510) ≈ 7×, wider than
  the 4–6× in §4.4.
- The same counter in loops should be the (b) gauge's companion (§4.5): per row, rep_mean ÷ loop.

---

## Appendix: scripts and outputs

Extraction (any empty scratch directory `X`; the scripts live elsewhere and take paths as arguments):

```sh
tar -xzf docs/l1-raw/s51b/full/b51c8-full.tgz -C X/x51c8      # → X/x51c8/b51c8/run/rep51/diag-ld
tar -xzf docs/l1-raw/s51b/full/b51c2-full.tgz -C X/x51c2
tar -xzf docs/l1-raw/s54/rep54b-diag-ld.tgz   -C X/x54b       # → X/x54b/diag-ld
tar -xzf docs/l1-raw/s56/rep56-diag-ld.tgz    -C X/x56
python -I sl51.py    X/x51c8/b51c8/run/rep51/diag-ld c8-100 docs/l1-raw/s51b/c8/ld.log
python -I sl51.py    X/x51c2/b51c2/run/rep51/diag-ld c2-100 docs/l1-raw/s51b/c2/ld.log
python -I pinwhere.py <diag-ld> <cell>
python -I sacki.py   X/x54b/diag-ld <x56 diag-ld> X/x51c8/.../diag-ld X/x51c2/.../diag-ld
python -I gapfit.py  X/x54b/diag-ld <x56 diag-ld> X/x51c8/.../diag-ld
python -I cand2.py
```

### A.1 `sl51.py`

```python
"""Store cap across the δ sweep (status §51 block 51, docs/l1-raw/s51b/full/*.tgz, extracted).

Usage: python -I sl51.py <diag-ld dir> <cell prefix, e.g. c8-100> <ld.log>
Per row: [DCAP] (whole row: pin, mean cap, mean ask, q, b), and LATE [DIAG] ticks (t >= 15 s, src > 0): the tick
cap W (win=a/W), its bind fraction against N*2048, occupancy a, per leg btlbw, Copa rtp (min_rtt), echo rtt, quinn
wrtt, wire tx rate r (d dgq tx / dt). In-order and goodput from the ld.log P38ROW t51 record. Arm = median over rows.
"""
import glob, json, os, re, statistics as st, sys
D, CELL, LD = sys.argv[1], sys.argv[2], sys.argv[3]
m = st.median
PP = r' p%d:infl=\d+/sinfl=\d+/bdp(\d+)\S* sout=\d+ khr=\S+ btlbw=(\d+) est=(\w).*?rtt=(\d+)/wrtt=(\d+)/rtp\d+ms rtp_us=(\d+)'
t51 = {}
for L in open(LD, errors='replace'):
    if L.startswith('P38ROW '):
        d = json.loads(L[7:])
        t51[(d['arm'], str(d['seed']), int(d['rep']))] = d.get('t51') or {}


def row(f, nleg):
    o = {}
    for L in open(f, errors='replace'):
        mm = re.match(r'\[DCAP\].*? pin=([\d.]+) .*?cap=([\d.]+) ask=([\d.]+) q=([\d.]+) b=([\d.]+)', L)
        if mm:
            o['pin'], o['capm'], o['ask'], o['q'], o['b'] = (float(v) for v in mm.groups())
    ticks = []
    for L in open(f, errors='replace'):
        if not L.startswith('[DIAG]'):
            continue
        t = float(re.search(r't=([\d.]+)s', L).group(1))
        src = re.search(r' src=(\d+)sym', L)
        w = re.search(r'win=(\d+)/(\d+)', L)
        dq = [re.search(r'dgq%d\[hand=(\d+) tx=(\d+)' % i, L) for i in range(nleg)]
        pp = [re.search(PP % i, L) for i in range(nleg)]
        if t < 15 or not (src and w and all(dq) and all(pp)) or int(src.group(1)) == 0:
            continue
        x = dict(t=t, a=int(w.group(1)), W=int(w.group(2)), src=int(src.group(1)))
        for i in range(nleg):
            x['tx%d' % i] = int(dq[i].group(2))
            bdp, bw, est, rtt, wrtt, rtp = pp[i].groups()
            x.update({'bw%d' % i: int(bw), 'rtt%d' % i: int(rtt), 'wrtt%d' % i: int(wrtt), 'rtp%d' % i: int(rtp) / 1000})
        ticks.append(x)
    if len(ticks) < 4:
        return None
    ceil = 2048 * nleg
    o['n'] = len(ticks)
    o['W'] = m([x['W'] for x in ticks]); o['a'] = m([x['a'] for x in ticks])
    o['bindL'] = sum(x['W'] >= ceil for x in ticks) / len(ticks)
    o['src'] = m([x['src'] for x in ticks])
    dt = ticks[-1]['t'] - ticks[0]['t']
    for i in range(nleg):
        o['r%d' % i] = (ticks[-1]['tx%d' % i] - ticks[0]['tx%d' % i]) / dt
        for k in ('bw', 'rtt', 'wrtt', 'rtp'):
            o['%s%d' % (k, i)] = m([x['%s%d' % (k, i)] for x in ticks])
    return o


nleg = 2 if CELL.startswith('c8') else 1
arms = {}
for f in sorted(glob.glob(os.path.join(D, CELL + '@def-*-c.log'))):
    mm = re.match(re.escape(CELL) + r'@def-(\w+)-s(\d+)-r(\d+)-', os.path.basename(f))
    o = row(f, nleg)
    if not o:
        continue
    t = t51.get((mm.group(1), mm.group(2), int(mm.group(3))), {})
    o['inorder'] = t.get('inorder_ms'); o['gp'] = t.get('goodput_mbps'); o['rr'] = t.get('rr_mean')
    arms.setdefault(mm.group(1), []).append(o)
order = ['HB', 'BM', 'BP', 'I1', 'I2', 'I3', 'AM', 'NA', 'HA', 'AP', 'I4', 'RM', 'RP', 'HR']
cols = ['n', 'q', 'b', 'pin', 'capm', 'ask', 'W', 'bindL', 'a', 'inorder', 'gp', 'rr', 'src']
for i in range(nleg):
    cols += ['r%d' % i, 'bw%d' % i, 'rtp%d' % i, 'rtt%d' % i, 'wrtt%d' % i]
print('cell', CELL, 'arm medians over rows; LATE = t >= 15 s; pin/capm/ask/q/b are the whole-row [DCAP] line')
print('arm', *cols, sep='\t')
for a in order:
    rs = arms.get(a, [])
    if not rs:
        continue
    vals = []
    for c in cols:
        if c == 'n':
            vals.append(str(len(rs))); continue
        xs = [r[c] for r in rs if r.get(c) is not None]
        v = m(xs) if xs else None
        vals.append('-' if v is None else ('%.4f' % v if c in ('q', 'bindL', 'pin', 'rr') else '%.1f' % v))
    print(a, *vals, sep='\t')
```

Output (c8, then c2; columns as the header):

```text
arm n q b pin capm ask W bindL a inorder gp rr src r0 bw0 rtp0 rtt0 wrtt0 r1 bw1 rtp1 rtt1 wrtt1
HB 6 0.1000 2.0 0.7408 3579.6 12357.5 4096.0 1.0000 4096.0 4714.3 97.6 0.0000 11452.5 9824.8 108506.2 126.6 203.8 104.0 1948.6 24061.5 39.2 219.0 202.0
BM 6 0.1000 2.0 0.7473 3496.6 12504.5 4096.0 1.0000 4096.0 4502.1 97.6 0.0000 11470.8 9792.8 125360.0 111.3 193.8 132.5 1944.5 25638.5 40.0 257.0 236.5
BP 6 0.0998 2.0 0.7489 3572.6 13844.6 4096.0 1.0000 4096.0 4507.2 99.5 0.0010 11453.0 9822.4 122112.5 120.8 197.5 113.5 1967.2 23072.0 41.2 210.5 204.0
I1 4 0.0961 1.9 0.7515 3548.3 13639.9 4096.0 1.0000 4096.0 4318.1 98.7 0.0210 11327.0 9819.6 115132.0 122.0 207.0 133.0 1956.4 24220.0 39.9 267.0 254.2
I2 4 0.0894 1.7 0.7554 3503.2 18204.2 4096.0 1.0000 4096.0 4393.1 95.6 0.0634 10937.2 9830.4 119477.5 181.1 227.5 95.5 1990.5 26202.0 104.8 260.5 249.5
I3 4 0.0805 1.4 0.7961 3646.4 33506.8 4096.0 1.0000 4096.0 4091.3 93.2 0.1213 10238.5 9836.4 182353.5 219.6 265.2 117.5 1992.5 30338.0 237.4 441.5 438.0
AM 6 0.0668 1.0 0.8170 3683.2 26758.8 4096.0 1.0000 4096.0 6154.1 82.3 0.2603 9032.5 9832.9 165948.5 175.9 260.5 158.0 2011.2 25010.0 224.6 621.5 607.2
NA 6 0.0667 1.0 0.8146 3663.2 36397.0 4096.0 1.0000 4096.0 5836.0 82.8 0.2618 9028.0 9834.8 178778.2 204.4 280.2 130.5 2001.0 27593.5 323.8 644.8 618.0
HA 6 0.0667 1.0 0.8189 3700.4 35198.8 4096.0 1.0000 4096.0 5469.7 83.0 0.2634 8956.0 9830.4 182409.5 206.3 291.5 127.0 1997.7 28583.0 268.3 642.5 626.0
AP 6 0.0666 1.0 0.8024 3650.4 34923.8 4096.0 1.0000 4096.0 5470.5 82.9 0.2729 9026.0 9851.8 156857.5 215.2 287.0 150.5 2001.6 27138.0 284.1 655.8 631.8
I4 4 0.0569 0.7 0.8142 3692.3 34340.2 4096.0 1.0000 4096.0 5655.2 81.4 0.2798 8936.5 9849.4 168944.0 215.9 291.5 157.0 1999.1 29127.5 304.0 666.2 656.5
RM 6 0.0501 0.5 0.8209 3715.7 35345.7 4096.0 1.0000 4096.0 6145.1 81.5 0.2805 8875.2 9845.3 176697.0 191.1 278.0 149.0 2001.0 26869.5 247.6 633.8 611.8
RP 6 0.0500 0.5 0.8204 3690.9 26333.4 4096.0 1.0000 4096.0 6436.4 80.8 0.2841 8851.2 9846.0 130567.0 176.8 268.0 140.5 2001.8 25735.0 260.2 665.8 634.2
HR 6 0.0500 0.5 0.9281 4025.6 138216.1 4096.0 1.0000 4096.0 1703.6 82.8 0.0084 21503.0 22118.5 739202.5 123.8 181.5 175.0 4785.7 52870.5 1726.7 2138.8 1020.8
(c8 LATE bind per row: 0.992-1.000 on HB/BM/BP, 1.000 on every other row)
arm n q b pin capm ask W bindL a inorder gp rr src r0 bw0 rtp0 rtt0 wrtt0
HB 6 0.1000 2.0 0.7829 1766.3 8803.5 2048.0 1.0000 2048.0 340.0 90.0 0.0000 9555.8 9840.2 58870.0 190.8 207.0 132.0
BM 6 0.1000 2.0 0.8010 1768.3 9676.7 2048.0 1.0000 2048.0 343.0 90.2 0.0000 9553.5 9826.8 66569.5 191.7 207.0 136.5
BP 6 0.0998 2.0 0.8035 1767.0 11301.0 2048.0 1.0000 2048.0 343.2 90.1 0.0006 9550.0 9844.9 72112.0 191.0 207.0 113.0
I1 4 0.0961 1.9 0.7949 1746.5 8669.1 2048.0 1.0000 2048.0 337.7 90.0 0.0154 9523.0 9834.5 64324.5 191.8 208.0 135.5
I2 4 0.0894 1.7 0.7245 1724.8 10432.8 2048.0 0.9654 2048.0 341.0 87.7 0.0474 9314.5 9856.3 74088.5 194.1 213.0 119.5
I3 4 0.0805 1.4 0.8207 1792.4 10237.1 2048.0 1.0000 2048.0 363.3 84.0 0.0982 8903.0 9819.6 68540.5 198.1 223.0 129.0
AM 6 0.0668 1.0 0.8372 1805.2 12391.5 2048.0 1.0000 2048.0 394.3 76.2 0.2193 8033.8 9848.3 75742.5 212.4 248.0 157.0
NA 6 0.0667 1.0 0.8355 1817.8 12450.5 2048.0 1.0000 2048.0 394.4 76.1 0.2205 8015.0 9849.8 74181.2 210.2 248.5 142.0
HA 6 0.0667 1.0 0.8333 1811.6 11814.5 2048.0 1.0000 2048.0 403.0 76.1 0.2322 8058.5 9836.8 70324.0 210.8 247.5 122.0
AP 6 0.0666 1.0 0.8343 1810.8 11111.5 2048.0 1.0000 2048.0 390.4 76.0 0.2194 8018.0 9852.3 68535.5 208.1 248.5 154.0
I4 4 0.0569 0.7 0.8387 1828.5 13092.9 2048.0 1.0000 2048.0 394.8 75.1 0.2390 7920.5 9844.5 81682.0 213.5 252.0 193.2
RM 6 0.0501 0.5 0.8408 1821.1 11713.5 2048.0 1.0000 2048.0 387.9 74.7 0.2484 7865.0 9837.7 68094.0 218.6 254.0 164.5
RP 6 0.0500 0.5 0.8213 1788.6 11751.0 2048.0 1.0000 2048.0 404.1 74.6 0.2444 7851.2 9837.8 66156.5 214.7 254.0 180.5
HR 6 0.0500 0.5 0.8571 1965.0 12088.0 2048.0 1.0000 2045.8 186.9 68.0 0.2646 17153.5 21982.8 145333.0 94.9 116.0 115.0
(c2 LATE bind per row: 0.86-1.00 on HB, I1, I2, AM, RP; 1.000 on every other row)
```

### A.2 `pinwhere.py`

```python
"""Where the unpinned refreshes sit: per row, the share of ALL [DIAG] ticks with win denominator = N*2048, the
first pinned tick time, and the share pinned after it. Usage: python -I pinwhere.py <diag-ld dir> <cell prefix>"""
import glob, os, re, statistics as st, sys
D, CELL = sys.argv[1], sys.argv[2]
ceil = 4096 if CELL.startswith('c8') else 2048
arms = {}
for f in glob.glob(os.path.join(D, CELL + '@def-*-c.log')):
    arm = re.match(re.escape(CELL) + r'@def-(\w+)-', os.path.basename(f)).group(1)
    tk = []
    for L in open(f, errors='replace'):
        if L.startswith('[DIAG]'):
            t = float(re.search(r't=([\d.]+)s', L).group(1)); w = re.search(r'win=\d+/(\d+)', L)
            if w:
                tk.append((t, int(w.group(1)) >= ceil))
    if not tk:
        continue
    first = next((t for t, p in tk if p), None)
    after = [p for t, p in tk if first is not None and t >= first]
    arms.setdefault(arm, []).append((sum(p for _, p in tk) / len(tk), first, sum(after) / max(1, len(after))))
for a in sorted(arms):
    xs = arms[a]
    print('%s\t%s\tall-ticks pinned %.3f\tfirst pin t=%.1f s\tpinned after first %.3f' % (
        CELL, a, st.median(x[0] for x in xs), st.median(x[1] for x in xs if x[1] is not None), st.median(x[2] for x in xs)))
```

Output, condensed: c8 all-ticks pinned 0.70–0.83 (HR 0.93), first pin t = 10.3 s on every arm (HR 4.2 s), pinned
after the first pin 0.88–1.00; c2 all-ticks pinned 0.73–0.86, first pin t = 10.3–15.3 s, pinned after it 1.000 on
every arm.

### A.3 `sacki.py`

```python
"""SACK ranges per WindowAck report, per arm: the last client [SACKI] line of each row (cumulative over the row).
Usage: python -I sacki.py <diag-ld dir> [<diag-ld dir> ...]   Prints cell/arm, rows, median ranges/report [min..max]."""
import glob, os, re, statistics as st, sys
arms = {}
for D in sys.argv[1:]:
    for f in glob.glob(os.path.join(D, '*-c.log')):
        mm = re.match(r'(c\d+-\d+)@def-(\w+)-s\d+-r\d+', os.path.basename(f))
        if not mm:
            continue
        last = None
        for L in open(f, errors='replace'):
            if L.startswith('[SACKI]'):
                last = L
        if not last:
            continue
        rep, rng = (int(v) for v in re.search(r'reports=(\d+) ranges=(\d+)', last).groups())
        if rep:
            arms.setdefault((mm.group(1), mm.group(2)), []).append(rng / rep)
for k in sorted(arms):
    xs = arms[k]
    print('%s\t%s\tn=%d\tranges/report %.1f [%.1f..%.1f]' % (k[0], k[1], len(xs), st.median(xs), min(xs), max(xs)))
```

Output (c8; §56 arms are P<pool>, W = 2·pool; §54b are B/L; the rest are the §51 sweep):

```text
P80 2.9 [2.9..3.1]  P112 4.2 [3.9..4.3]  P160 7.2 [6.8..7.6]  P256 20.2 [19.1..21.2]  P384 39.6 [36.2..41.3]
P640 54.6 [52.6..55.6]  P640D 53.5  B640 54.6 [53.6..55.3]  L640 53.2 [52.4..53.6]  B1024 61.8 [60.4..62.5]
L1024 58.5 [54.9..60.1]  B2048 63.4 [63.2..63.6]  L2048 62.9 [62.6..63.5]
sweep HB BM BP I1 I2 I3 AM NA HA AP I4 RM RP: 63.2-63.6 each;  HR 11.6 [7.9..17.3]
c2: P128 1.2  P160 1.2  P192 1.3  P256 1.4  P384 1.6  P2048 4.1;  sweep arms 4.1-4.6 (HR 4.3)
```

### A.4 `gapfit.py`

```python
"""Gaps per report at c8 against a Little count (report-fit condition).

Usage: python -I gapfit.py <diag-ld dir> [...]
Per row (client log): LATE [DIAG] ticks t >= 15 s: r_i = d(dgq_i tx)/dt (datagrams/s), echo rtt_i medians,
store W; the row's ranges/report from its last [SACKI] line (cumulative, boot included).
Model: G = G_reorder + G_loss,
  G_reorder = r_1 * max(0, (rtt_1 - rtt_0) - dret)  slow-leg seqs in flight beyond the fast leg's newest arrival
              (dret = return-path delay difference: 15 ms if each leg's report returns on its own leg, 0 if on the fast leg)
  G_loss    = sum_i eps_run_i * r_i * kappa * rtt_i   open loss runs (Little; kappa = 1.5 loops to close)
  eps_run   = loss / burst per leg from tools/l1/lib.sh calibration: leg0 0.027/8, leg1 0.0407/4.
"""
import glob, os, re, statistics as st, sys
m = st.median
EPS = (0.027 / 8, 0.0407 / 4)
KAPPA = 1.5
arms = {}
for D in sys.argv[1:]:
    for f in glob.glob(os.path.join(D, 'c8-100@def-*-c.log')):
        arm = re.match(r'c8-100@def-(\w+)-', os.path.basename(f)).group(1)
        ticks, last = [], None
        for L in open(f, errors='replace'):
            if L.startswith('[SACKI]'):
                last = L
            if not L.startswith('[DIAG]'):
                continue
            t = float(re.search(r't=([\d.]+)s', L).group(1))
            w = re.search(r'win=(\d+)/(\d+)', L)
            dq = [re.search(r'dgq%d\[hand=\d+ tx=(\d+)' % i, L) for i in (0, 1)]
            pp = [re.search(r' p%d:infl=.*? rtt=(\d+)/wrtt=(\d+)/rtp' % i, L) for i in (0, 1)]
            src = re.search(r' src=(\d+)sym', L)
            if t < 15 or not (w and all(dq) and all(pp) and src) or int(src.group(1)) == 0:
                continue
            ticks.append((t, int(w.group(2)), [int(d.group(1)) for d in dq], [int(p.group(1)) for p in pp]))
        if len(ticks) < 4 or not last:
            continue
        rep, rng = (int(v) for v in re.search(r'reports=(\d+) ranges=(\d+)', last).groups())
        dt = ticks[-1][0] - ticks[0][0]
        r = [(ticks[-1][2][i] - ticks[0][2][i]) / dt for i in (0, 1)]
        rtt = [m([x[3][i] for x in ticks]) / 1000 for i in (0, 1)]
        W = m([x[1] for x in ticks])
        gl = sum(EPS[i] * r[i] * KAPPA * rtt[i] for i in (0, 1))
        g15 = r[1] * max(0.0, rtt[1] - rtt[0] - 0.015)
        g0 = r[1] * max(0.0, rtt[1] - rtt[0])
        arms.setdefault(arm, []).append((W, rng / rep, r[1], rtt[0] * 1e3, rtt[1] * 1e3, gl, g15, g0))
print('arm\tn\tW\tmeasured\tr1\trtt0\trtt1\tG_loss\tG_re(dret15)\tG_re(dret0)\tpred15\tpred0  (pred capped at 64)')
for a in sorted(arms, key=lambda k: m([x[0] for x in arms[k]])):
    xs = arms[a]
    c = [m([x[j] for x in xs]) for j in range(8)]
    print('%s\t%d\t%.0f\t%.1f\t%.0f\t%.0f\t%.0f\t%.1f\t%.1f\t%.1f\t%.1f\t%.1f' % (
        a, len(xs), c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7], min(64, c[5] + c[6]), min(64, c[5] + c[7])))
```

Output (selected; all sweep arms other than HB, BP and I2 predict ≥ 64):

```text
arm   n W    measured r1   rtt0 rtt1 G_loss G_re(15) G_re(0) pred15 pred0
P80   4 160  2.9      188  16   46   0.8    2.7      5.5     3.5    6.4
P112  4 224  4.2      405  20   46   1.3    4.4      10.5    5.7    11.7
P160  4 320  7.2      1113 26   47   2.1    6.7      23.4    8.8    25.5
P256  4 512  20.2     1718 39   61   3.5    12.1     37.9    15.6   41.4
P384  4 768  39.6     1831 57   92   5.4    38.8     66.1    44.2   64.0
B640  6 1280 54.6     1882 96   146  9.0    65.2     93.3    64.0   64.0
B1024 6 2048 61.8     1923 152  224  13.9   110.7    139.4   64.0   64.0
B2048 6 4096 63.4     1938 198  235  17.0   26.0     55.3    42.9   64.0    (misses: 1 289 old holes, not in the model)
L2048 6 4096 62.9     1982 354  294  26.5   0.0      0.0     26.5   26.5    (misses: quinn backlog on p0)
HB    6 4096 63.6     1949 204  219  16.5   12.7     27.3    29.1   43.7    (misses)
```

### A.5 `cand2.py`

```python
"""The decision-8 candidate's fixed point, unclamped, on three input sets (store-law-state.md sec. 3).

  n* = sum_i lam_i * ( Phi_i + g*S_F_i + q*F_i + Shat_i ),   Phi_i = F_i + 0.5*max(0, max_j F_j - F_i)

Input sets per leg:
  ENGINE  lam = logged btlbw (Copa max_bw, wire datagrams/s), F = logged rtp (Copa min_rtt)          -- what main would feed
  NETEM   lam = link datagram rate (lib.sh rate / 1270 B on the wire, x 1/1.032 source share at Bulk),
          F = propagation RTT 2 x one-way (lib.sh)
  S60     lam = delivered source rate (logged src= split by wire tx share), F = s60 floor F_c (r68allow.out)
S_F and Shat are s60 numbers in every set (no engine source exists; ENGINE is given its best chance).
Logged values: little54b.out (sec. 54b arm medians) and sl51-*.out (sweep arm medians), copied below.
"""
SF = {'c2': [5.077], 'c8': [5.454, 10.022], 'c1s': [0.430]}
SH = {'c2': [5.0], 'c8': [11.1, 16.0], 'c1s': [0.5]}          # leg-1 Shat: midpoint of 10.0-23.5 (not bracketed)
F60 = {'c2': [8.682], 'c8': [8.794, 37.357], 'c1s': [2.381]}
PROP = {'c2': [10.0], 'c8': [10.0, 40.0], 'c1s': [2.0]}
LINK = {'c2': [100e6 / 8 / 1270], 'c8': [100e6 / 8 / 1270, 20e6 / 8 / 1270], 'c1s': [1e9 / 8 / 1270]}
# name, cell, q, btlbw per leg, rtp per leg (ms), src total, wire tx per leg
PTS = [
    ('s54b B640 (Bulk)', 'c8', 0.10, [75228, 21026], [43, 38], 11693 / 1.032, [9811, 1882]),
    ('s54b B1024 (Bulk)', 'c8', 0.10, [119092, 23410], [94, 39], 11751 / 1.032, [9828, 1923]),
    ('s54b B2048 (Bulk)', 'c8', 0.10, [114713, 22272], [130, 40], 11749 / 1.032, [9811, 1938]),
    ('sweep c8 HB (Bulk)', 'c8', 0.10, [108506, 24062], [126.6, 39.2], 11452, [9825, 1949]),
    ('sweep c8 HA (Auto)', 'c8', 0.0667, [182410, 28583], [206.3, 268.3], 8956, [9830, 1998]),
    ('sweep c8 RP (d 51)', 'c8', 0.05, [130567, 25735], [176.8, 260.2], 8851, [9846, 2002]),
    ('sweep c2 HB (Bulk)', 'c2', 0.10, [58870], [190.8], 9556, [9840]),
    ('sweep c2 HA (Auto)', 'c2', 0.0667, [70324], [210.8], 8059, [9837]),
    ('sweep c2 RP (d 51)', 'c2', 0.05, [66157], [214.7], 7851, [9838]),
]


def nstar(c, q, g, lam, F):
    fm = max(F)
    return sum(l * (f + 0.5 * max(0.0, fm - f) + g * SF[c][i] + q * f + SH[c][i]) / 1000 for i, (l, f) in enumerate(zip(lam, F)))


print('point\tcell\tq\tover-read btlbw/link\trtp/F60\tn*(ENGINE)\tn*(NETEM)\tn*(S60) g=1\tn*(S60) g=0\tTmin* leg ms (S60, g=1)')
for name, c, q, bw, rtp, src, tx in PTS:
    share = [t / sum(tx) for t in tx]
    lam60 = [src * s for s in share]
    lamN = [x / 1.032 for x in LINK[c]]
    ov_bw = '/'.join('%.1f' % (b / l) for b, l in zip(bw, LINK[c]))
    ov_rtp = '/'.join('%.1f' % (r / f) for r, f in zip(rtp, F60[c]))
    fm = max(F60[c])
    tmin = '/'.join('%.1f' % (f + 0.5 * max(0, fm - f) + SF[c][i] + q * f) for i, f in enumerate(F60[c]))
    print('%s\t%s\t%.4f\t%s\t%s\t%.0f\t%.0f\t%.0f\t%.0f\t%s' % (
        name, c, q, ov_bw, ov_rtp, nstar(c, q, 1, bw, rtp), nstar(c, q, 1, lamN, PROP[c]),
        nstar(c, q, 1, lam60, F60[c]), nstar(c, q, 0, lam60, F60[c]), tmin))
for q in (0.10, 0.05):
    lam = [x / 1.001 for x in LINK['c1s']]
    print('c1s link-rate point (q %.2f)\tc1s\t%.4f\t-\t-\t-\t%.0f\t%.0f\t%.0f\t%.2f' % (
        q, q, nstar('c1s', q, 1, lam, PROP['c1s']), nstar('c1s', q, 1, lam, F60['c1s']), nstar('c1s', q, 0, lam, F60['c1s']),
        F60['c1s'][0] + SF['c1s'][0] + q * F60['c1s'][0]))
```

Output:

```text
point               q      btlbw/link rtp/F60   ENGINE NETEM S60g1 S60g0 Tmin*(g=1)
s54b B640 (Bulk)    0.1000 7.6/10.7   4.9/1.0   6282   539   508   437   29.4/51.1
s54b B1024 (Bulk)   0.1000 12.1/11.9  10.7/1.0  16543  539   511   440   29.4/51.1
s54b B2048 (Bulk)   0.1000 11.7/11.3  14.8/1.1  20865  539   511   440   29.4/51.1
sweep c8 HB (Bulk)  0.1000 11.0/12.2  14.4/1.0  19622  539   514   443   29.4/51.1
sweep c8 HA (Auto)  0.0667 18.5/14.5  23.5/7.2  57740  534   399   343   29.1/49.9
sweep c8 RP (d 51)  0.0500 13.3/13.1  20.1/7.0  39545  531   392   337   29.0/49.2
sweep c2 HB (Bulk)  0.1000 6.0        22.0      12949  201   188   139   14.6
sweep c2 HA (Auto)  0.0667 7.1        24.3      16522  198   156   115   14.3
sweep c2 RP (d 51)  0.0500 6.7        24.7      15581  196   151   111   14.2
c1s link rate       0.1000 -          -         -      308   349   307   3.05
c1s link rate       0.0500 -          -         -      298   337   295   2.93
```

(The g = 0 static values at c2 lie below the knee, where λ is not the link rate. §61b's replay gives 70 / 51.)

### A.6 In-order against W/λ_src (`sl51-*.out` columns)

Computed as in-order ÷ (1000·W/src) per arm. The c2 ratio is 1.49–1.60 on all 14 arms; c8 is 8.9–13.9 (§2.3).

### A.7 `c1oos.py`, `wakes.py`, `kappa.py` (§7)

```python
"""Out-of-sample c1s landing of the candidate, with and without a release-interval term (store-law-state.md sec. 7).
Fixed point of the ratio law at one leg: Tmin(n*) = F + A, A = S_F + q*F (+ tau_rel(n) in the extended form).
Tmin(W) is not logged at c1s for W >= 384 (s60 has W 64/128/192/2048 only), so it is estimated as F + q_W, with
q_W = mean echo loop - T0 (R6.1 / s56curve.out: T0 = 4.0 ms; q 0.61/1.16/3.18/5.29/14.19 at W 384/512/768/1024/2048):
the whole-distribution-shift reading r68 sec. 1.2 measured at c2 (assumption, flagged).
tau_rel(W): release (ack-wake) interval from wakes.py on the s56 c1s rows. Goodput/control: s56 (0.79/0.89/0.94/0.96/1)."""
F, SF = 2.381, 0.430
W = [384, 512, 768, 1024, 2048]
qW = [0.61, 1.16, 3.18, 5.29, 14.19]
tau = [1.00, 1.20, 1.68, 1.48, 2.34]
gp = [0.79, 0.89, 0.94, 0.96, 1.00]


def land(q, extra):
    f = [(F + qq) - (F + SF + q * F + e) for qq, e in zip(qW, extra)]   # Tmin - target; crossing where it turns positive
    if f[0] >= 0:
        return None
    for i in range(1, len(W)):
        if f[i] >= 0:
            x = f[i - 1] / (f[i - 1] - f[i])
            return W[i - 1] + x * (W[i] - W[i - 1]), gp[i - 1] + x * (gp[i] - gp[i - 1])
    return 'above', None


for q in (0.10, 0.05):
    print('q %.2f  candidate: %s   with tau_rel: %s' % (q, land(q, [0] * 5), land(q, tau)))
# below W 384: Tmin(384) estimate 2.99 vs target 3.05 at q 0.10 -> f(384) < 0 means the law still grows at 384
print('Tmin(384) est %.2f; target q0.10 %.2f; q0.05 %.2f' % (F + qW[0], F + SF + 0.10 * F, F + SF + 0.05 * F))
```

Output: `q 0.10 candidate (397.5, 0.80) with tau_rel (629.7, 0.91)`; `q 0.05 candidate None (below W 384) with tau_rel (609.9, 0.91)`; `Tmin(384) est 2.99; target 3.05 / 2.93`.

```python
"""Release (ack-wake) interval per arm: LATE [DIAG] ticks (t >= 15 s, src > 0), d(wake ack)/dt, as an interval in ms.
Usage: python -I wakes.py <diag-ld dir> <glob>"""
import glob, os, re, statistics as st, sys
arms = {}
for f in glob.glob(os.path.join(sys.argv[1], sys.argv[2])):
    arm = re.match(r'(c\w+-\d+)@def-(\w+)-', os.path.basename(f)).groups()
    tk = []
    for L in open(f, errors='replace'):
        if L.startswith('[DIAG]'):
            t = float(re.search(r't=([\d.]+)s', L).group(1)); m = re.search(r'wake\[.*? ack=(\d+)', L); s = re.search(r' src=(\d+)sym', L)
            if m and s and t >= 15 and int(s.group(1)) > 0:
                tk.append((t, int(m.group(1))))
    if len(tk) > 4:
        rate = (tk[-1][1] - tk[0][1]) / (tk[-1][0] - tk[0][0])
        arms.setdefault(arm, []).append(1000.0 / rate)
for k in sorted(arms):
    print('%s %s n=%d release interval %.2f ms [%.2f..%.2f]' % (k[0], k[1], len(arms[k]), st.median(arms[k]), min(arms[k]), max(arms[k])))
```

Output (median [min..max] ms): c1s P384 1.00, P512 1.20, P768 1.68, P1024 1.48, P2048 2.34; c2 P128 1.31, P160 1.26, P192 1.21, P256 1.19, P384 1.18, P2048 1.17, sweep HB 1.15; c8 P80 1.22, P112 1.04, P160 0.89, P256 0.82, P384 0.85, P640 0.87.

```python
"""Hole lifetime in loops, kappa, measured directly (independent of the old-hole onset kappa ~ 2 was read from).
Per row: server final [SUCC] rep_mean_us and rep_p50_us (holes closed by repair: detection at the receiver to fill),
over the client's LATE (t >= 15 s, src > 0) rate-weighted echo loop T = sum r_i*rtt_i / sum r_i (rtt_i medians,
r_i = d(dgq_i tx)/dt). kappa_mean = rep_mean / T, kappa_p50 = rep_p50 / T. Arm medians.
Usage: python -I kappa.py <diag-ld dir> [...]"""
import glob, os, re, statistics as st, sys
m = st.median
arms = {}
for D in sys.argv[1:]:
    for f in glob.glob(os.path.join(D, '*-c.log')):
        k = re.match(r'(c\w+-\d+)@def-(\w+)-', os.path.basename(f))
        if not k:
            continue
        nleg = 2 if k.group(1).startswith(('c8', 'c1d')) else 1
        tk = []
        for L in open(f, errors='replace'):
            if not L.startswith('[DIAG]'):
                continue
            t = float(re.search(r't=([\d.]+)s', L).group(1)); s = re.search(r' src=(\d+)sym', L)
            dq = [re.search(r'dgq%d\[hand=\d+ tx=(\d+)' % i, L) for i in range(nleg)]
            pp = [re.search(r' p%d:infl=.*? rtt=(\d+)/wrtt' % i, L) for i in range(nleg)]
            if t >= 15 and s and int(s.group(1)) > 0 and all(dq) and all(pp):
                tk.append((t, [int(d.group(1)) for d in dq], [int(p.group(1)) for p in pp]))
        sf = f[:-6] + '-s.log'
        su = [L for L in open(sf, errors='replace') if L.startswith('[SUCC]') and 'final=1' in L] if os.path.exists(sf) else []
        if len(tk) < 4 or not su:
            continue
        mm = re.search(r'rep_n=(\d+) rep_p50_us=(\d+) .*?rep_mean_us=(\d+)', su[-1])
        if not mm or int(mm.group(1)) < 50:
            continue
        dt = tk[-1][0] - tk[0][0]
        r = [(tk[-1][1][i] - tk[0][1][i]) / dt for i in range(nleg)]
        rtt = [m([x[2][i] for x in tk]) for i in range(nleg)]
        T = sum(r[i] * rtt[i] for i in range(nleg)) / sum(r) * 1000.0  # us
        arms.setdefault((k.group(1), k.group(2)), []).append((int(mm.group(3)) / T, int(mm.group(2)) / T, T / 1000))
for a in sorted(arms):
    xs = arms[a]
    print('%s %-6s n=%d loop %.0f ms  kappa_mean %.2f [%.2f..%.2f]  kappa_p50 %.2f' % (
        a[0], a[1], len(xs), m(x[2] for x in xs), m(x[0] for x in xs), min(x[0] for x in xs), max(x[0] for x in xs), m(x[1] for x in xs)))
```

Output (c8 arm: loop ms, kappa_mean): P80 17 2.50; P112 21 1.89; P160 28 1.51; P256 42 1.23; P384 62 1.49; P640 105 1.98; B640 104 1.90; B1024 162 3.08; B2048 205 13.31; L640 107 1.53; L1024 171 1.43; L2048 343 1.00; sweep Bulk arms 13.3-14.1, Auto/RT arms 9.7-12.8 (old holes), HR 0.53. c2: P128 1.48, P160 1.55, P192 1.38, P256 1.18, P384 1.08, P2048 0.85; sweep HB 0.84, Auto 0.23-0.25.
