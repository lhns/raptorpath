# P_fec executed oracle: build spec (round 7, design only)

This spec is complete enough for a worker to build from without design
choices. It feeds `pfec-continuous.md` §8 (owner path (b): derive the sliding
per-loss law against an executed decoder).

- Base: main `59a65af8`. Every `file:line` below was re-grepped on it.
- Supporting numbers: `pfec-continuous/emission_witness.py` → `.out`.
- Advisor (Fable) round 7 settled: the deadline per caller, the extraction
  order, the emission readings E1 (confirmed) and E2 (cadence-dependent), the per-seat clock, and the registration tolerances.

**Header entry 8 (main-session Fable review of `2aa40a98`, round 8).** It
found three problems:
- m̄ decides the outcome, so a fixed m̄ = 1 pre-refuted every form.
- The copied debt and ack lines set W1, and string pins hold text, not
  behaviour.
- The refutation rule was never applied to the frozen predictions.

It also asked for E1 to be re-verified, and for the span seat to land once,
after phase 6.

Round 8 changes (my advisor pass, `pfec_decide.py` → `.out`):
- W1 is a known-answer test against engine output (K4). The engine already
  reads 0.67 at a realtime c1s row, so 0.055 is withdrawn as a prediction.
- m̄ is read per wake, and its gauge range is reported.
- A per-cell decidability table: A\* separates shipped/Φ from the block law;
  W 201 body separates nothing.
- A tail rule in miss counts.
- The asserted lower sides of K2 and K3 are dropped.
- The extraction lands after phase 6.
- A timing plan comes before the sample sizes.

§6 below holds every round-8 change. Where it conflicts with §0–§5, **§6
governs**.

## 0. Two findings the build must witness first

**E1. `TaperBudget`'s taper shape is inert as wired.**
- `emit_source.rs:537` passes `flat_rate = window_rate.min(spare)` as `rate`,
  and passes the same `spare`.
- Starting from `owed = 0`, `accrue` (`control/fec_rate.rs:704`) then gives
  `owed = rate` and `grant = min(rate, max(desire, rate), spare, 1) = rate`, so
  `owed` returns to 0, for any `offset` and any shape.
- Checked over 10⁵ random calls with rate ≤ spare: max |grant − rate| = 0 and
  max owed = 0 (`emission_witness.out`).
- So emission is uniform debt accrual, which is the Python bound's spacing
  premise. Round 6's "front-loaded after each ack" caveat is withdrawn.
- A law that is degenerate as wired is a **law-ledger finding**: the `taper`
  and `offset` inputs of the budget law are dead on this path.

**E2. The repair debt is reduced by acks.** This is a code reading plus a
**cadence-dependent prediction**, not a confirmed finding.
- `emit_source.rs:696–700` adds `grant` per source and emits while debt ≥ 1.
- `sender_phases.rs:1278–1328` subtracts `newly_acked × rate` on every
  cumulative-ack advance (clamped at 0). `:1162–1172` does the same per
  NACKed seq.
- After the emission loop the debt is < 1, so one ack advance of m sources
  cancels at most one repair's worth, out of the r·m accrued.
- With m̄ the mean sources per ack advance, the witness W1 = emitted/(r·sources)
  therefore obeys
  ```text
  W1 ≥ max(0, 1 − 1/(r·m̄))
  ```
  - m̄ = 1 makes the bound vacuous. That is the regime of my
    per-slot-ack model (`emission_witness.out`: 0.055 at the pin inputs,
    0.143 at A\* 7, r 0.5, 0.85–0.90 at the W 201 rows, all with give-up at
    D and no ARQ).
  - At m̄ ≥ 10 and r 0.34, W1 ≥ 0.71.
  - "Lossless ⇒ emission exactly 0 after start-up" holds only when m̄·r < 1.
- **m̄ is not settled by reading.**
  - `RWM_ACK_MERGE` emits one control datagram per data message
    (`sender_policy.rs:30–42`, `[CTLD]` ≈ 1.0), which points to m̄ ≈ 1.
  - But `TaperBudget`'s own doc calls an ack cycle "hundreds of symbols"
    (`fec_rate.rs:648–651`).
  - The pin comment's "consumed r ≈ 0.34" under `max_overhead` 0.5 would
    need W1 ≥ 0.68.
  - `taper_emission_l0.rs` (the "wire consumes r\*" rung) reads cod/src from
    DIAG and asserts nothing (`:16–18`).
  - So m̄ is an oracle input, swept {1, 4, 16, 64}. W1 is reported per cell.
- If W1 ≪ 1 on the engine, the r handed to `residual_loss_after_fec`
  (`flat_rate`, `emit_source.rs:593–598`) is not the wire rate. That would be
  an input-honesty finding for ledger R03/S04, whatever P_fec form is chosen.
- The oracle's x-axis is the **handed** r, because that is what the law is
  evaluated at. The consumed r is witness W1.

## 1. Called vs copied

The oracle is an **in-crate `#[cfg(test)]` module**,
`raptorpath/src/net/pfec_oracle.rs`, registered next to `decision_trace_pin`
(`net/mod.rs:3756`). It must be in-crate because several pieces are
`pub(crate)`. There is no visibility edit and no I/O.

| piece | status | where | notes |
|---|---|---|---|
| encoder | **called** | `fec::RlcWindowEncoder` (`fec/rlc_window.rs:31`): `add_source`, `generate_repair_range` (:142), `generate_repair` (:98), `advance` (:199), `window_span` (:192) | — |
| encoder window cap | copied | systematic `win_cap = MAX_WINDOW_SIZE` 200 (`net/mod.rs:205`, `sender_policy.rs:443–464`); eviction `if !generation && window_size > win_cap` (`mod.rs:3518–3520`); ack advance `advance(ack − keep_behind)` (`sender_phases.rs:1370–1374`) | a string pin (§1.1) |
| decoder | **called** | `fec::RlcWindowDecoder` (`fec/rlc_window.rs:248`) | — |
| decoder prune | **called** | `receiver::decode_inert_floor(next_expected, recv_win_cap)` (`receiver.rs:2215`), then `window_decoder.advance(floor)`, as `receiver.rs:669–675` does | — |
| coefficients | called (inside the codec) | `gf256::generate_window_coefficients` (`gf256/src/lib.rs:236`): nonzero pseudo-random GF(256) | — |
| `TaperBudget::accrue` | **called** | `control::TaperBudget` (`fec_rate.rs:704`) | `spare` = `rate + 1` (non-binding). By E1 a binding spare is just a lower r, so it gets no arm |
| `TaperFunction` | **called** | `TaperFunction::from_estimator(&LossEstimator::new(), rate)` (`fec_rate.rs:617`; an invalid GE gives q = 0.5) | inert by E1 |
| repair-debt loop | copied | `emit_source.rs:696–700` (`+= grant`, emit while ≥ 1, `−= 1`) | a string pin (§1.1) |
| ack and NACK debt cancellation | copied | `sender_phases.rs:1278–1328` (`newly_acked × rate`), `:1162–1172` (NACK) | a string pin. Switch `ack_cancel`: **on** in every registered reading; off only in K1 and K3. NACK cancellation is off (no NACKs without ARQ) |
| taper-copy slot | **out of the first build** | `emit_source.rs:702–757` (P_lost-gated copy of the oldest un-acked symbol, `rand::random` at `:722`) | Copies off: P_fec is coded recovery, the quantity R's formula claims. A copies-on arm needs `p_lost`'s srtt, rttvar, age in seconds and the copied clock's "oldest un-acked". It is specified in a later round |
| span seat A\*, Δ | **called after extraction** (§2) | today inline at `emit_source.rs:559–600` | — |
| span placement | **called after extraction** (§2) | today inline at `emit_source.rs:784–806` | — |
| D(δ) | **called** | `net::shed_deadline_us(b, rtprop_us)` (`shed.rs:31`), `net::delta_budget_b_of(δ)` (`store_cap.rs:318`) | — |
| receiver hold | **called** | `shed::shed_recv_hold(srtt, true, true)` (`shed.rs:58`, `pub(crate)`) = srtt/2, the evicting-seat resolution rule | — |
| GE channel | copied (6 lines) | loss iff Bad; p, q from (ε, σ²) by p + q = 2/(σ² + 1), p = ε(p+q); stationary start; stepped per wire symbol; **one chain** over sources and repairs | `tests/common/mod.rs:29` is not visible in-crate; the dual-path arm is out of the first build |
| ack clock | copied | the cumulative ack is the highest contiguous **resolved** seq (arrived, decoded, or resolved by the seat's rule below). It is reported every m̄ sources and delivered F slots later; `taper_offset = 0` on each advance (`sender_phases.rs:1444`) | F derived per cell (§3 table); m̄ ∈ {1, 4, 16, 64}, primary 1 |
| resolution rule (a simulation input, separate from the readout deadline) | copied | **Evicting seat (A\* rows):** a hole is given up at the executed receiver hold H = srtt/2 from its **detection** (the slot a higher seq arrives). **Reliable seat (W 201 rows):** the receiver never gives up; an unrecovered hole is resolved by ARQ landing L_arq after detection. That is a clock event only, never counted as FEC | without a rule, one hole stalls the ack for the rest of the run |
| receiver-request arm | **out** | `net/mod.rs:3319`; `RWM_RECV_REQUEST_LAW` and `RWM_RANK_FEEDBACK` default off (`gates.rs:376–377`) | — |

### 1.1 String pins for the copied engine lines

One `#[test]` reads `emit_source.rs` and `sender_phases.rs` with
`include_str!`. It asserts that each copied snippet is present verbatim, as
whitespace-normalised substrings:
- `st.repair_debt += repair_rate;`
- `while st.repair_debt >= 1.0 && st.encoder.window_size() > 0 {`
- `st.repair_debt -= 1.0;`
- `let debt_reduction = newly_acked as f64 * repair_rate;`
- `st.repair_debt = (st.repair_debt - debt_reduction).max(0.0);`
- the eviction condition.

An engine edit to them then fails the oracle's build instead of drifting
silently.

## 2. The extraction (shared-core D12, step 1.2's seat)

Two pure functions go **in the same file first**, `net/emit_source.rs`, as
`pub(crate)`:

```rust
/// Span seat (paper §5.3; shared-core D12). D = min(b·rtprop, 2·rtprop),
/// A* = clamp(ceil(rate·D), 1, W), Δ = clamp(ceil(rate·J), 1, 64).
/// NaN rate ⇒ A* = Δ = 1 (NaN as u64 = 0, then clamped); W = 0 panics
/// (Ord::clamp min > max), exactly as the inline code does.
#[inline]
pub(crate) fn span_seat(rate_sym: f64, rtprop_s: f64, b: f64, jitter_s: f64, window: usize) -> (u64, u64) {
    let d = (b * rtprop_s).min(2.0 * rtprop_s);
    let a_star = ((rate_sym * d).ceil() as u64).clamp(1, window as u64);
    let delta = ((rate_sym * jitter_s).ceil() as u64).clamp(1, 64);
    (a_star, delta)
}

pub(crate) enum SpanPlacement { Range { start: u64, count: u16 }, Leading }

/// Trailing solvable-span placement (paper §5.3): [max(ws, end − A*), end),
/// end = newest + 1 − Δ; Leading when the window is too young to trail.
#[inline]
pub(crate) fn trailing_span(ws: u64, we: u64, a_star: u64, delta: u64) -> SpanPlacement {
    let end = (we + 1).saturating_sub(delta);
    let start = end.saturating_sub(a_star).max(ws);
    if end > start {
        SpanPlacement::Range { start, count: (end - start).min(u16::MAX as u64) as u16 }
    } else {
        SpanPlacement::Leading
    }
}
```

**The rewritten call sites.**
- At `:577–582`:
  `let (a_star, delta) = span_seat(rate_sym, rtprop, b, est.jitter_us() / 1e6, st.encoder.window_size());`.
  `jitter_s` is passed as the same quotient the inline code forms, so the
  float operations are identical.
- The anchor feed (`:561–569`), the `b` read (`:576`) and the shed refresh
  (`:587–599`) stay at the call site. That refresh needs `d` only through
  `shed_deadline_us`, which recomputes it from `b`.
- At `:793–806`: `let (ws, we) = st.encoder.window_span();`, then
  `match trailing_span(ws, we, a_star, delta)`:
  - `Range { start, count }` → `generate_repair_range(start, count).unwrap_or_else(|| st.encoder.generate_repair())`;
  - `Leading` → `st.encoder.generate_repair()`.

**Proof, in order** (ADR-0077 §3; `shared-core-design.md` §4 ladder):
1. Build the release binary before and after. If the sha is identical,
   including the `.text`/`.rodata` hash of ladder 1, the guard is waived. If
   not, the step rides **guard batch α** (one §41c run for steps 1.2–2.5).
2. Then, on the extracted tree, add two digests to `decision_trace_pin`:
   - `pin_span_seat`: rate_sym ∈ {0, 1e-3, 3.7, 94, 1e5, NaN, ∞} ×
     b ∈ {span_horizon_b(50), (0.5), (0.005)} × rtprop ∈ {1e-3, 0.04} ×
     jitter_s ∈ {0, 5e-3} × W ∈ {1, 4, 200, 201}, digesting (A\*, Δ);
   - `pin_trailing_span`: ws ∈ {0, 10}, we ∈ {0, 3, 10, 300}, A\* ∈ {1, 4, 201},
     Δ ∈ {1, 2, 64}, digesting the variant and fields.

   The untouched tree has no function to digest, so these are new captures,
   not a pre/post comparison. The later move to `raptorpath-core` (D12) must
   keep them.
3. Remove the pin file's "not driveable" note for the A\*/Δ seat
   (`decision_trace_pin.rs:19–21`).

## 3. Deadlines: one per caller, tied to its use

Slots are source sends. rate = sources per second at the cell.

**Shed budget** (`residual_loss_after_fec`, `emit_source.rs:593`): R is
consulted when a hole's age exceeds D(δ) = `shed_deadline_us(b, rtprop)`,
measured from the original send. So P_fec here means "recovered by FEC
within D(δ) of its send". The registered deadline is d = rate·D(δ) slots.
- Because A\* = ⌈rate·D⌉, d ∈ (A\* − 1, A\*].
- A repair covering s is emitted Δ to Δ + A\* − 1 slots after s. So some
  covering repairs land after D.
- Convention: recovered-by-D iff `decode_slot − send_slot ≤ d`.
- Report three readouts side by side: d = A\* − 1, d = A\* and d = A\* + Δ.
- Secondary readout: d at the executed receiver hold `shed_recv_hold` =
  srtt/2 (`receiver.rs:791`).

**`r_saturation`** (`raptorpath-math/src/lib.rs:917`): tail_fec charges a miss
L_arq = 1.5·srtt (`:926`). So P_fec there means "recovered by FEC before an
ARQ round would land". The registered deadline is d = L_arq·rate.
- Its W is `encoder.window_size()` (201 on L1), but repairs span A\*(δ). The
  two coincide only where A\* is W-clamped, which is the Bulk case on L1
  (A\* 201–263 measured).
- The W 201 rows therefore set A\* = W = 201.
- Per-cell srtt and rate are cited from the nominal cells: c2 srtt 10 ms at
  100 Mbit/s and 1 200 B; c3 40 ms at 20 Mbit/s. Labelled nominal.

**Per-cell clock in slots** (rate = link/9 600 bit; F = rate·RTT;
H = srtt/2; Δ = ⌈rate·J⌉):

| cell | rate (sym/s) | RTT | F | Δ (J) | resolution rule | primary readout d |
|---|---|---|---|---|---|---|
| A\* rows (L0 c3, Realtime b = ½, measured A\* 4–7 at ≈ 95 sym/s, `8e77ba67`) | ≈ 95 | from A\* | **F = A\*/b = 2·A\***, because A\* = ⌈rate·b·RTT⌉ | 1 (J 5 ms, 0.48) | give-up at H = srtt/2 = F/2 ≈ A\* slots from detection | **d = A\* − 1** |
| W 201, c2 nominal | 10 417 | 10 ms | 104 | 32 (J 3 ms) | ARQ resolution at L_arq after detection | **d = L_arq = 156** |
| W 201, c3 nominal | 2 083 | 40 ms | 83 | 11 (J 5 ms) | as c2 | **d = L_arq = 125** |

**Why d = A\* − 1 on the shed rows.**
- Lags are integers and the convention is lag ≤ d.
- With rate·D ∈ (A\* − 1, A\*], ⌊rate·D⌋ = A\* − 1 unless rate·D is an integer.
  So d = A\* − 1 is the shed deadline in slots.
- A\* and A\* + Δ are reported alongside as readouts, never used for
  refutation.

**Recorded per recovered hole**, so any other deadline is a readout:
- lag from its send, in slots;
- lag from its detection, i.e. the slot the receiver first sees a higher seq.

## 4. Known-answer tests, in order (all must pass before any reading)

1. **K0, r = 0.** No repair is emitted, so recovered = 0 exactly; R = ε.
   Also assert grant == rate exactly on every `accrue` call (E1).
2. **K1, scripted single loss.** Lossless except source s; rate 1, spare 1,
   switch `ack_cancel = off`.
   - The first covering repair is emitted at send slot s + Δ (end > s ⇔
     t ≥ s + Δ).
   - Assert lag = Δ for Δ ∈ {1, 2, 5}, and that `trailing_span` returned
     `Range`.
3. **K2, block configuration.**
   - Placement is replaced by fixed blocks: after the block's last source,
     emit ⌊rW⌋ + Bernoulli(frac) repairs over [kW, (k+1)W) via
     `generate_repair_range`. One GE chain, starting Good. These are the
     `temporal_oracle.rs:1361–1423` premises.
   - Target: the exact C5 "after" value (`pfec_review.py`
     `palm_joint_exact(..., layout='after')`), at (ε, q, W, r) = the five
     `temporal_oracle.rs:1476` scenarios.
   - Tolerance: C5 − 3·SE − τ_GF ≤ P ≤ C5 + 3·SE. Here τ_GF = 1/255 is the
     singularity of a tight square system with nonzero pseudo-random GF(256)
     coefficients, an upper bound on the per-window rank deficit. One-sided,
     since the codec can only fall short of the generic rank.
4. **K3, P(∞) against the Python bound at matching premises.**
   - Switch `ack_cancel = off`, so emission is uniform debt accrual, the
     Python premise. Copies off, same Δ, decode without a deadline.
   - Compare per Δ (not T1's maximum) with `pfec_review.py` R1b, e.g. the pin
     cell: Δ 1 0.4022, Δ 2 0.4187.
   - Expected: P_rust(∞) ≤ P_py + 3·SE, because the real decoder prunes and
     its rank is GF(256), not generic.
   - At A\* 3–7, also P_rust(∞) ≥ P_py − 3·SE − τ_GF: the prune floor is
     2·W behind, far beyond the spans.
   - At W 201 only the upper side is asserted.

## 5. Registration draft (for status, before the run)

**Cells.**
- **A\* grid:** A\* ∈ {3..7} × r ∈ {0.2, 0.29, 0.34, 0.5}, ε 0.048, σ² 3.76.
  Plus the pin cell (A\* 4, r 0.34) and the r = 0.5·r_c rows. Clock per the
  §3 table.
- **W 201:** the c2/c3 nominal rows at 1.5 and 2·r_c, the two Auto-r\* tail
  rows, and the 0.5·r_c rows.
- **The single primary reading per cell, the only one the refutation rule
  reads:**
  - `ack_cancel` on; m̄ = 1 (ACK_MERGE: one ack per data message); copies
    off; one GE chain;
  - Δ per the §3 table;
  - d = A\* − 1 (A\* rows) or d = L_arq (W 201 rows).
- **Readouts reported, not used for refutation:** d ∈ {A\*, A\* + Δ, ∞},
  m̄ ∈ {4, 16, 64}, and W1.

**Frozen predictions.** Per form, per cell, the values in
`pfec-continuous.md` §8.3 and `pfec_bracket.out` / `pfec_delta.out`
(shipped, Φ, C6, C5, sliding bound), copied as constants into
`pfec_oracle.rs`.

The sliding-bound rows were computed at Δ 1–2 (and at Δ 8 for two cells).
At W 201 the engine's Δ is 11–32, so those bound rows are not frozen as
predictions there; only the forms are.

Witness W1 (consumed / handed r) is predicted to be in
[max(0, 1 − 1/(r·m̄)), 1]. At m̄ = 1 the per-slot model gives 0.055 at the
pin (`emission_witness.out`).

**Refutation rule.**
- A form is refuted at a cell iff |P_oracle(d) − P_form| > 3·SE_seeds + τ.
- SE is across ≥ 5 seeds; holes cluster in bursts, so seeds, not holes, are
  the unit.
- τ is the caller's resolution, with provenance:
  - **shed budget:** τ = 1e-3/ε, the ρ slider's step in R (0.021 at
    ε 0.048);
  - **r_sat:** τ = 0.005·L_r, one `r_saturation` grid step times the derived
    slope bound L_r = W·max ΔG (≈ 0.10 at W 201).

**Sample size.**
- Body: ≥ 10³ holes per cell per seed (SE ≈ 0.015).
- Auto tail at W 201: **10⁵ holes**, ≈ 2·10⁶ (c3) to 4·10⁶ (c2) sources.
  - That gives 20–40 misses at the target 2–4e-4, in ~10–20 bursts, so the
    relative SE is about 25–30 %.
  - It **separates C6 (≈ 1–2e-2) from shipped/Φ (≈ 3–5e-4)**.
  - It does **not** test "meets the target" to ± 20 %.
- **Unresolvable:** the Realtime tail (t/ε ≈ 2–4e-6 needs ≳ 10⁷ holes per
  ±30 %), and anything at Δ > 8 unless run.

**What each outcome means.**

| outcome | meaning |
|---|---|
| O1: P_oracle(d) at the A\* cells ≤ the sliding bound, near C5/C6 | the shipped form is refuted at the shed budget: R ≥ ≈ 2.6 % against 0.49 % (budget-starving). The sliding law is fitted to nothing; it is derived and then held to these values |
| O2: P_oracle(d) ≥ 0.71 at the pin (the L0 datum's band) | a premise of the Python bound fails (emission, ack cancellation, Δ, copies). The bound is withdrawn; the datum stands; the shipped form is not refuted there |
| O3: W1 ≪ 1 at the pin at m̄ = 1 | the proactive rate is ack-cancelled, and R is evaluated at a rate the wire does not carry. This is a ledger input-honesty finding, independent of the P_fec form. W1 near 1 instead means E2 is inert at the engine's cadence. Either way, the engine's actual m̄ must still be read from a gauge (sources per ack advance) |
| O4: W 201 Auto-tail miss ≥ 1e-3 | the closed-form r\* under-provisions on the executed decoder, and the rate-law finding reopens |
| O4′: W 201 Auto-tail miss ≤ a few e-4 | the block-law figure stays a paper erratum only |
| any K0–K3 failure | the harness is invalid; no reading is taken |

**Run cost.** Rust, no I/O. The body grid is ≈ 30 cells × 5 seeds × 10⁴
sources; the tail is ≈ 8·10⁶ sources. Both are within a CI-scale minute
budget, as an estimate. The tail runs `#[ignore]`d and on demand.

## 6. Round 8: revisions (these govern over §0–§5)

### 6.1 m̄: what the debt cancellation sees, and the gauges

**Reading.**
- The cancellation runs **per sender-loop iteration**, not per ack frame.
- `on_ack_advance` (`sender_phases.rs:1225`) is called once per iteration
  from `net/mod.rs:3482`. It reads `next_expected = window_ack_seq.load(..)`
  (`mod.rs:3481`) and acts if the cumulative ack moved since the last
  iteration (`cumulative_advance`, `sender_phases.rs:1220`).
- So m̄ = sources acked per **advance-observing iteration**. It lies between
  Δcum/Δ(all iterations) and the release batch Δcum/Δwake[ack]
  (`tools/l1/pool57_score.py:14`, `:160`).

**Gauge readings on existing rows.** The token semantics are as read; this is
not a calibration.

| rows | Δcum / Δwake[ack] | Δcum / Δ`wait n` | note |
|---|---|---|---|
| anchor-diag MAIN client rows (`docs/l1-raw/anchor-diag/diag-ad/*-MAIN-s42-r1-a1-c.log`): c1s / c1d / c2 / c8 | 13.9 / 3.6 / 0.7 / 0.7 | 38 / 20 / 6.5 / 7.3 | — |
| §57a and §57b c1 pool rows (`docs/status.md`, the §57a/§57b tables) | 60–195 | — | large pools |
| store-law-n1 realtime smoke, c1s (`.../diag-smoke/c1s-400-LAW-realtime-s42-r1-a1-c.log`) | 0.8 | — | — |

So m̄ is **cell- and era-dependent and not settled.**

**Registration consequence.**
- A cell is read only at an m̄ that a gauge has shown for that caller's seat.
- W 201 rows are read at the gauge range of their reliable-seat rows. A\*
  rows (the evicting seat) are a **reported m̄ sweep** {1, 4, 16, 64, 256}
  until an evicting-seat row exists.
- **The VM row that settles the evicting seat:**
  - c3 (or c2), `--protocol-hint realtime`, **no** `--window-reliable`
    (evicting), `RWM_UNIFIED=1`, `RWM_DIAG=1`;
  - tokens `[DIAG] cum=`, `wake[… ack=]`, `wait[… n=]`, `src=`, `cod=`, and
    `[SPAN] rr= debt= a_star= delta= shed= bud=`;
  - m̄ and engine W1 are both read from it.

### 6.2 W1 is a witness on engine output: known-answer test K4

**What the tokens count.**
- `cod=` is `total_repair_symbols`: all coded repair, i.e. proactive, margin
  and request-law equations (`diag.rs:220–223`). `src=` is source symbols.
  Both are DIAG rates.
- `[SPAN] rr=` is the handed `repair_rate` (the grant, `emit_source.rs:662–672`).

**Engine W1 on record.**
- **store-law-n1 realtime smoke, c1s client:** mean `rr` 0.119 (48 SPAN
  lines), cod/src 0.080 (94 DIAG lines with src > 1 000/s). That gives
  W1 ≲ 0.67. It is an upper bound, because `cod` includes margin and
  deficit coded repair.
- **Unified flip battery** (`c3a9d768`, L1 c3 realtime perf cell,
  `--window-reliable`): cod/src 0.38–0.50 consumed, under the same
  cancellation code. The code dates from `f2cba2b7`, 2026-03-20, so it was
  in place.
- **L0 evicting rep** (`8e77ba67`): "ru/rf ≈ 15.5 % (435/69)". If 435 is
  repairs fired over ≈ 95 sym/s × 20 s of sources, cod/src ≈ 0.23 at
  r 0.2–0.5, i.e. W1 ≳ 0.45. This is a reading, and the token meaning is
  unconfirmed.
- So the per-slot-ack model's 0.055 is **withdrawn as a prediction** for
  either seat. The copied clock at m̄ = 1 is what was wrong.

**K4.** Before any P reading, the oracle must reproduce engine W1 at a
recorded row's inputs (ε, σ², r = mean `rr`, A\*, Δ, m̄ from that row's
gauge): |W1_oracle − W1_engine| ≤ 3·SE_seeds. Here W1_engine = cod/(rr·src),
an upper bound. Until K4 passes, the copied debt and ack clock are
unvalidated and no P reading is taken.

Why a witness and not an extraction:
- Extracting the debt arithmetic (three lines) would not pin its call order
  or cadence (per iteration, against the ack atomic), and those set W1.
- The string pins (§1.1) stay as drift guards only.

### 6.3 E1, verified

- `TaperBudget::accrue` has one production call site, `emit_source.rs:543–549`.
  It is passed `rate = flat_rate = window_rate.min(spare.max(0.0))`
  (`:537`) and the same `spare`.
- Shaping needs owed > 0, which needs a call with spare < rate. The seat's
  `min` excludes that. With spare < 0, `flat_rate` = 0 and the grant is 0.
- `max_overhead` ≤ 0.5 < 1, so the `1.0` cap never binds. Hence
  grant ≡ rate at this seat, for any offset or shape.
- The shaped behaviour is exercised only in unit tests that drive
  spare = 0 and then ∞ (`control/fec_rate/tests.rs:957–1008`), and in
  `decision_trace_pin.rs:336`.
- `taper_emission_l0.rs` asserts nothing on emission. It is a measurement
  instrument that scrapes cod/src (`:16–20`).
- So E1 holds at the only production seat.

### 6.4 The refutation rule applied to the frozen predictions

Source: `pfec_decide.out`.
- A form f is refuted iff |P_oracle − P_f| > B, with B = 3·SE + τ.
- Forms a and b are **separable** iff |P_a − P_b| > 2B, i.e. every oracle
  value refutes at least one of them.
- SE comes from 5 seeds × 10³ holes, burst-inflated by the GE mean burst
  1/q.

| window | cells | shipped/Φ vs C6/C5 | C6 vs C5 | shipped vs Φ | verdict |
|---|---|---|---|---|---|
| A\* 3–7, r 0.2–0.5 (shed budget; τ 0.021, B ≈ 0.05) | 20 | gap 0.30–0.63 > 2B ≈ 0.10–0.11 at **20/20** | gap 0.03–0.07 < 2B at 20/20 | equal (above r_c) | **decidable:** the block law against shipped/Φ. **Undecidable:** C6 vs C5 |
| A\* 4, r = 0.5·r_c | 1 | shipped–block 0.02–0.03: no; Φ–block 0.43: yes | no | 0.45: yes | Φ decidable; shipped vs block not |
| W 201 body, c2/c3 at 0.5, 1.5, 2·r_c (τ 0.005·L_r = 0.064–0.10) | 6 | gap 0.08–0.17 < 2B ≈ 0.18–0.26 at 6/6 | no | separable only at c3 0.5·r_c | **reported only.** τ = 0.005·L_r was the wrong unit: it is the change of P over one grid step, not the P error that moves r_sat's argmin. The follow-up rule compares r_sat in grid steps and is sized after the §6.7 timing |

### 6.5 The tail rule, in miss counts

At W 201, the Auto r\*, d = L_arq and N = 10⁵ holes, there is one threshold
per cell at the geometric midpoint of the shipped and C6 expectations:
- **c2:** shipped/Φ μ 47, C6 μ 2 041. T = √(47·2 041) = **310**.
- **c3:** shipped/Φ μ 27, C6 μ 1 084. T = √(27·1 084) = **171**.

The rule, with no unassigned range:
- k < T refutes C6 and C5;
- k ≥ T refutes shipped and Φ.

**Clustering tolerance.** The threshold holds at 3σ for miss clusters up to
c ≈ 164 holes (c2) and 85 (c3): (μ ± 3√(μ·c)) stays on its side of T.
Observed cluster sizes are recorded as a witness. If the largest exceeds
that bound, the tail reading is void.

**Not separable by the tail:**
- shipped from the target, with μ 39 (c2) and 21 (c3);
- C6 from C5.

### 6.6 Known-answer tests and outcomes, restated

- **K2:** the asserted lower side is dropped. K2 instead reports the
  per-block **mismatch count**: blocks where the decoder's outcome differs
  from `surv ≥ nlost`. Each block knows both. The upper side, P ≤ C5 + 3·SE,
  is kept.
- **K3:** the asserted lower side is dropped. Only P_rust(∞) ≤ P_py + 3·SE
  is asserted.
- **K4** (§6.2) is added and runs before any P reading.
- **Outcomes replace O1 and O2 with per-window verdicts:**
  - **shed budget:** at each A\* cell the oracle refutes either shipped/Φ or
    both block forms. C6 vs C5 is not decided.
  - **r_sat body:** reported only.
  - **tail:** the §6.5 rule.
- If K3 passes and W1 ≈ 1 at the read m̄, the old O1 follows from K3. It is
  not a separate outcome.
- O3 is replaced by K4. O4 and O4′ are replaced by §6.5. The outcome "any
  K0–K4 failure ⇒ no reading" stands.

### 6.7 One extraction, after phase 6

- `span_seat` rewrites the same lines as shared-core step 1.2. It therefore
  lands **after phase 6** (branch `phase6-forms`, in progress; not on github
  at this writing, so the signatures below are **provisional**).
- It calls the moved `raptorpath-math` forms and re-clamps in u64, so NaN
  still gives 1:
  ```rust
  let a_star = (raptorpath_math::span_width_a_star(rate_sym, d, window as f64).ceil() as u64).clamp(1, window as u64);
  let delta  = (raptorpath_math::trailing_offset_delta(rate_sym, jitter_s) as u64).clamp(1, 64);
  ```
  - `span_width_a_star` clamps before the ceil while the inline code ceils
    first. The two agree for every finite input, because W is an integer.
  - `trailing_offset_delta` returns NaN for a NaN rate. `as u64` gives 0 and
    the u64 clamp gives 1, which preserves today's value.
- `trailing_span` is unchanged.
- **Step 1.2's other two seats** (`net/mod.rs:475`, `shed.rs:31`) change in
  the same commit and ride the same proof.
- **The sha proof is the VM's Linux release build of both trees.** The host
  comparison is evidence only.
  - Identical sha: the guard is waived.
  - Otherwise: guard batch α.
- The `decision_trace_pin` digests of §2 are captured on the extracted tree.

### 6.8 Timing, before any sample size is fixed

- Time one W 201 cell: c3 nominal, r 0.158, A\* = W = 201, Δ 11,
  10⁵ sources, primary arm.
- Use `cargo test` (debug profile) and `cargo test --release`.
- Use the smallest symbol size `RlcWindowEncoder::new` accepts. Codec cost is
  linear in symbol size and the decode does not read payload bytes.
- Report µs per source and per hole. Then set:
  - the body N, at 10³ holes per seed;
  - the tail N, at 10⁵ holes: ≈ 2·10⁶ (c3) to 4·10⁶ (c2) sources. It runs
    `#[ignore]` if it takes over a minute at the release profile.
