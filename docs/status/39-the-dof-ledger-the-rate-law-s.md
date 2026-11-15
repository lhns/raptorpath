## 39. The DoF ledger, the rate law's feedback leg — derivation, no VM — verdict

§37 rated the rate law "the maths holds, inert", and the user opened the
FlEC-style import. The import was derived on paper and bounded from
existing ledgers: no code, no build, no VM contact. The sources:

- FlEC (Michel et al., IEEE/ACM ToN 31(2) 2023, arXiv:2208.07741);
- AC-RLNC (Cohen et al., IEEE TCOM 68(7) 2020, arXiv:1905.02870) and its
  multipath form (arXiv:1910.13290);
- Hairpin (NSDI 2024) and Tambur (NSDI 2023), as contrast.

The derivation is paper §4.13, with rows in §11.2, §11.3 and §11.4. The
bound reads the r > 0 battery's receiver counters:
`git show db30e43:raptorpath/docs/l1-raw/rbattery-trunc/r-s42.log`, seed
42, reps 1–4, split by `docs/l1-raw/dof39/rfa_split.py`.

**Verdict: `CONTINUOUS-BUT-CANNOT-HELP-MEASURABLY`. No battery is
pre-registered.**

*The formula (continuous, no mode bit).*

```text
   Ω  =  Σ_c ( k_c − a_c )⁺   +   B
   B  ←  min(B + r(β), max(r(β)·W, 1)) per source or copy sent;  B ← B − 1 per proactive repair
```

| term | meaning | provenance |
|---|---|---|
| k_c | decoder component c's unknowns minus held rows | receiver |
| a_c | live corrections touching c | sender |
| B | the shipped `TaperBudget` bank plus the debt it grants into (schematic above), with copies accruing too | sender |
| r(β) | the blended rate law | §4.5, clamped |
| live | not yet reflected (ack/SACK, exact), and younger than the correction's own-path §7.1 threshold | shipped law |

A bank is needed. A stateless count over the live set, `(r·n_live − p)⁺`
(this branch's first form), loses every fraction below 1 as symbols age
out: at r = 0.01 on a 94-symbol live set it never emits.

- Ω is continuous in δ through r(β). It has no hint argument and no
  threshold on δ or ρ; ρ enters only through the shed law's hole set.
- FlEC's ds is derived: `ds = r(β) − l̂`. Bulk's −l̂ is the β = 1, χ = 0
  point.
- r* stays the a-priori term, spent through the existing bank. The ledger
  is the a-posteriori correction on top of it. It replaces the
  `ceil(retx·ε̂)` margin (by r(β) accrued per copy) and the `nacked·r` debt
  reduction (by counting live repairs in a_c).

*Why it cannot help measurably.* The double coverage the ledger removes is
`[RFA] preempt_src`: a copy arriving for a sequence the decoder had already
rebuilt (`net/report.rs`). On every row `preempt_src ≈ 0.85·fill_coded`.
At the singles nearly every decoded hole also got a copy; at c8 a
preempting arrival can be the original on the slower leg.

| 1.8 MB | CTL preempt | GLIDE preempt | MID preempt | completion spread, CTL |
|---|---|---|---|---|
| c3hg | 0.48 % of source | 1.15 % | 3.37 % | 5.4–7.2 s |
| sc2 | 0.14 % | 0.34 % | 1.33 % | 5.6–7.7 s |
| c8 | 0.50 % | 0.4–1.7 % | 0.35 % | 0.3–3.7 s |

- The larger waste classes do not move:
  - `dup_src` (re-fires and false alarms: 1.3–3.0 % at CTL, up to 25 % at
    c8 GLIDE) belongs to the recovery clock. The shipped cooldown is
    `max(pooled estimator RTT, 10 ms)`, and the ledger's liveness is
    `9/8·max(Copa SRTT_p, estimator RTT_p)`. If the two RTTs agree, that is
    a 9/8 stretch and `dup_src` is out of reach. If the Copa SRTT runs well
    above the estimator, part of `dup_src` is removable, and the corner
    bound widens to preempt + dup: ≤ 3.4 % of source at c3hg, ≤ 1.5 % at
    sc2. Which holds was not read.
  - `rep_redundant` is a-priori insurance.
- At the corner, dropping the margin also drops its decodes (fill_coded
  0.56 % at c3hg), which copies would have covered anyway. The net is
  ≤ about 1 % of source.
- By construction no cell is buffer-limited in FlEC's sense: SACK-clocked
  release (§6.3) frees every SACKed slot, so FlEC's one mid-stream bulk
  gain does not apply. This was not measured on these rows.
- The single-path throughput identity (§3.7) caps funded r at parity.
- The decode-closed latency (4–6×) is a max-of-k effect that a count does
  not change.

So the predicted effect is ≤ 0.5 % of wire at the shipped corner (≤ 3.4 %
if the re-fires are reachable) and ≤ 1.7 % at the only measurable funded
point. All are well under the completion spread (c3hg CTL ±15 % around its
median). The bound is also an upper bound: it credits the ledger with every
preempted copy.

The bound's own limits:

- The counters come from one seed and n = 4.
- They were taken on the September binary, which ran c3hg and sc2 at about
  2 Mbit/s against 20 and 100 Mbit/s links (`OUT-OF-BAND-RESULT` on 69 of
  72 rows; §37 records the sc2 regression as since fixed in `1b890e0`).
- A launch-day binary could carry different class shares. The verdict
  rests on their ratio to the completion spread, not on their level.

*Corrections to this branch's first draft (before any registration was
recorded).* The first draft attributed the funded arms' excess `dup_src`
to double coverage. It read "84–92 % of GLIDE's extra corrections arrived
redundant" and pre-registered `dup_src` as the primary measurand. By the
counter's definition `dup_src` is copies of sequences whose copy had
already arrived, a recovery-clock class that the ledger does not reach. The
removable class is `preempt_src`. Its excess over CTL is 4–30× smaller
than the `dup_src` excess. That pre-registration was committed on this
unpushed branch and withdrawn (reset) before any push or VM contact, so no
registered outcome was changed. The draft's stateless prior term (see the
formula above) was also replaced by the bank. The paper commit on this
branch carries only the corrected text.

**Premises found false or imprecise.**

1. *"The FEC half has no feedback leg."* Partly false:
   - The generation seat runs deficit-minus-in-flight (`GenerationDeficit`,
     `gen_want`).
   - The v8 `RepairRequest (a, m, k = holes − pivots)` already carries the
     rank deficit.
   - The plain window's reactive plane is FlEC's bulk rule.

   What is missing is one ledger across the planes, and §4.13 bounds its
   value.
2. *"Bulk uses ds = −l̂ (ARQ plus last-flight protection)."* Last-flight
   protection is FlEC's separate `FECPattern()` hook. r̂ = 1 − l̂ is a
   receive rate. χ is the counterpart: at T_rem = 0 it protects the last
   flight in free slots, and its pre-end ramp displaces source.
3. *"Repairs always stay inside cwnd."* There are three windows:
   - quinn's window bounds every datagram;
   - the scheduler's Copa-lite cwnd caps only proactive repair;
   - gap copies and margin repairs are uncharged to it
     (`RWM_CHARGE_RECOVERY`, default off).
4. *"Coded share ≤ 0.9 %."* CTL's non-NACK coded symbols are 0.50–1.14 %
   of source at the singles and 1.77–1.90 % at c8 (`(cod − retx)/src`).
5. *Unlisted.* The reactive margin `ceil(retx·ε̂)` adds at least one
   leading-window repair per firing report. It is now in §11.2.
6. *"Funded r > 0 made c3hg slower."* True. The ledger would not change
   that: the removable coordination waste at GLIDE is ≤ 1.2 % of source
   at c3hg.

**Test designs (owed only if the user opens a build; red-first,
absolute).**

Pure law: `raptorpath-math/src/dof.rs`, with tests in
`raptorpath-math/tests/dof_ledger.rs`. The signature is
`(components: &[(k_c, a_c)], bank)`, with the bank accrued by
`TaperBudget` and r(β). There is no hint, δ or ρ argument.

- `dof_ledger_bulk_corner_owes_one_per_uncovered_missing_dof`: r = 0,
  components {(3, 1), (2, 2), (0, 1)} ⇒ B = 0 and Ω = 2.0 exactly. It is
  finite when read through `controller_rate`'s clamped output at β = 1,
  χ = 0.
- `dof_ledger_without_feedback_is_the_taper_budget`: no components,
  r ∈ {0.01, 0.064, 0.2}, n = 10 000 sources, with symbols ageing out of the
  live set at a 94-symbol flight. Emission equals the shipped
  `TaperBudget::accrue` (spare = ∞) symbol for symbol, so it is ⌊r·n⌋ ± 1
  at r = 0.01 as well. The stateless form emits 0 there, and the test must
  fail on it.
- `dof_ledger_held_rows_reduce_what_is_owed`: 5 unknowns and 3 rows ⇒
  Ω = 2.0. The shipped count is 5 copies plus `ceil(5·ε̂) ≥ 1`.
- `dof_ledger_live_copy_is_not_refired`: (k 1, a 1) after the copy is
  sent gives Ω = 0 at r = 0 and Ω = B = 0.1 at r = 0.1. Past the path
  threshold, a = 0 and Ω = 1 (+ B).
- `dof_ledger_insurance_replaces_the_ceil_margin`: 10 copies sent for
  (k 10, a 10) give Ω = 0 at r = 0, against the shipped
  `ceil(10·0.05) = 1`, and Ω = B = 2.0 at r = 0.2.
- `dof_ledger_one_law_across_the_dial`: run the same symbol sequence at
  every preset and its ±2 % nudges. Cumulative Ω satisfies
  `|ΔΩ| ≤ (sources + copies)·|Δr(β)|`, which → 0. Ω is non-decreasing in
  r, including across the Bulk preset.

Wiring (measurement-discipline rule 1), in `src/net/tests.rs`:

- `ledger_reads_the_receiver_rank_deficit`: one row over two holes ⇒ one
  correction with the gate on, against 2 copies plus at least 1 margin
  repair with it off.
- `ledger_no_margin_at_bulk`.
- `ledger_reflection_gap_is_bounded`, which bounds the model/engine
  divergence in both directions:
  - A correction reflected after its threshold is double-sent at most once.
  - A copy acked or SACKed before its threshold leaves a_c on the exact
    signal, never by age, so it is counted once.
- Gate pins: `RWM_DOF_LEDGER=0` in `default_env_resolves_the_shipped_stack`
  and `gates_echo_default_is_byte_pinned`.
- The visualizer: the same Ω in the wasm model, with
  `test_continuum_one_law_across_the_dial` and both mjs gates extended.

**What would reopen it.** Any of these:

- a Copa δ echo, making the off-corner MID point measurable (preempt
  1.3–3.4 % at the singles);
- a pinned-store cell (c8L) read under the ledger;
- a Realtime (ρ < 1) cell.

A pre-registration for any of them starts from the tests above.

### 39. Addendum — the verdict's strength

The bound behind "cannot help measurably" comes from one seed, n = 4, on the
binary line that ran sc2 at about 2 Mbit/s. It reads as **not shown**, not as
refuted. The parity derivation (funded r on one saturated path is capped at
parity with ARQ) stands. The ledger would not make FEC act at Bulk; only an
off-Bulk cell on the wire can show a gain. Owed: §11.6 reference entries,
and the §9.8 and §10 wording.
