# P_fec executed oracle: build spec (round 7, design only)

This spec is complete enough for a worker to build from without design
choices. It feeds `pfec-continuous.md` §8 (owner path (b): derive the sliding
per-loss law against an executed decoder).

- Base: main `59a65af8`. Every `file:line` below was re-grepped on it.
- Supporting numbers: `pfec-continuous/emission_witness.py` → `.out`.
- Advisor (Fable) round 7 settled: the deadline per caller, the extraction
  order, the emission readings E1 (confirmed) and E2 (cadence-dependent), the per-seat clock, and the registration tolerances.

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
