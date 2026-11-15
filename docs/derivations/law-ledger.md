# Law ledger: a correctness audit of every shipped law

Status: AUDIT. There are no engine changes and no VM. Base: `github/main` at `cbe2be3f`
(2026-11-13). Branch-only rows say so: `snapver-law` at `8191b3de` and `bbrrs-r3` at `d1fadd7c`.

**Why.** The owner wants the product fast, with the mathematical modelling correct first. Several
laws have been found pinned, degenerate or fed over-read inputs (status §3.10 items 1–9, §53
Erratum 1). This ledger puts every law in one place, gives each a verdict under FORMULA-FIRST and
NO-MODE-SWITCH (`CLAUDE.md`; measurement discipline rules 17–18), and orders the fixes.

**Files.**
- `law-ledger.yaml` is the source of truth, readable by a machine. Every row has `id, name,
  verdicts, priority, depends_on, evidence`. It also has `rank`, `ships_with`, `fix_class`,
  `measured`, `gauge` and the ten audit columns.
- `law-ledger/render.py` checks the YAML: unique ids, resolvable `depends_on`/`ships_with`, valid
  tags, and ranks 1..N with each used once. It then regenerates the marked blocks of this file.
  `python law-ledger/render.py --check` fails if this file is stale.

**Sources reused, not redone.** Each is cited where a row depends on it:
- the paper reconciliation (`paper-recon`, `w3-paper-reconciliation.md`, on `fdad664e`);
- the bounding tests (`bound-tests`, merged into main; this ledger records the bounds as landed);
- the wasm audit (`wasm-modeswitch-audit`, `docs/derivations/wasm-modeswitch-audit.md`);
- the §44 law inventory and its erratum;
- the §3.4 / paper §11.2 open-constants register;
- the W1.2 derivation (`derive-w1`, `w1-who-limits.md`);
- `p0-dwell-diag.md` and `dup-diag.md`.

**How the code was read.** Four read-only lookup passes covered store/emission/shed, recovery,
scheduler/CC/estimators and math/rate/wasm. They located each law with file:line on `cbe2be3f`.
I re-read these lines myself:
- `pooled_store_cap` and its doc comment;
- `bdp_anchor`, `record_delivery` vs `rs_on_delivered` and their callers, including
  `copa_feed_attribute`'s body;
- Copa's `min_rtt` deque;
- `RateSnapshot::rate`;
- the quinn CC factory;
- the `serve_gaps` ripe-gate branches;
- `recovery_laws.rs` in full;
- the receiver refresh's srtt source and the estimator seed;
- the NACK throttle;
- `TaperBudget`;
- `store_gate_released`;
- `place_costs` and `expected_delivery_load_at`;
- the `wire_rtt` field doc;
- `clamp_cwnd_with_anchor`.

Every other row's file:line and formula come from the lookup passes and were not re-read; those
rows are P2/P3. Every Appendix A function name maps to a row or to the exclusion list below.

**Excluded (not laws).** Instruments and resource bounds that no decision reads:
- `[ETA]` (`net/eta.rs`), `[HOLD]` (`net/holddown.rs`), `stall_threshold_us`, `LAG_TICK`;
- the I/O-owner channel depths, `VIEW_REFRESH`, `INBOUND_BATCH_MAX`, `CONTROL_SEND_TIMEOUT`;
- `qnative_window_n`, which has no engine caller;
- `contract_stall_s`, reached only from the off three-term cap, which is in F04;
- the removed arms (`archive/RWM_*`).

The shared pieces of two laws are folded into one row:
- `BULK_TAIL_BUDGET` is in R05;
- the knee 2048, boot 128 and floor 10 are in F01;
- `MAX_NACK_GAPS` is in C02.

## 1. Verdict criteria (advisor-reviewed before the audit)

| tag | meaning here |
|---|---|
| `OK` | Four conditions hold. (1) The formula is in the paper with per-symbol provenance. (2) The code has the same shape: order in N, units, monotonicity. (3) Its inputs measure what the law's sentence assumes. (4) No hint, `reliable`/ρ, or δ/ρ threshold selects a different formula. `OK` alone does not mean verified: the `measured` flag says whether the law was ever measured in its operating regime, and the distribution counts "OK but unverified" separately. |
| `DIVERGENT` | Code and paper differ in shape, input or constant. Since `bound-tests` merged, the evidence line says whether a test bounds the divergence. An unbounded divergence ranks higher. |
| `UNDERIVED` | A symbol or constant has no provenance (measured with its sweep, cited, a declared dial, or a resource bound stated outside the law). A "declared corner" and "the argmax of a failed sweep" count as underived. |
| `PINNED(cells)` | The output sits at a bound (floor, ceiling or search cap) over the operating range. The threshold is **a bind fraction ≥ 0.5 at a measured cell**. It is declared here, since rule 18 names no number, and the fraction is printed. A two-valued floor/cap law (T_cut) is `PINNED`, following §3.10's usage. |
| `DEGENERATE(cells)` | The law body or an input is constant, or the law sits on its trivial branch, over the regime: r ≡ 0, the P_fec = 0 side, an input that is a constructor seed. |
| `DISHONEST-INPUT` | An input measures a different quantity from the one the law's sentence names. The estimator row carries the tag. A consumer row carries it only where that input is live in that law, and it lists the estimator in `depends_on`. A cited constant applied to the wrong coordinate (RFC 9002's 9/8 on a dwell-inclusive age) is `DISHONEST-INPUT`, not `UNDERIVED`. |
| `MODE-SWITCH(live\|dead)` | Only a hint equality, the `reliable`/ρ boolean, or a δ/ρ threshold that selects a different law. Experiment gates, continuous clamps in the δ maps and path-count branches go in column 8 as notes. `live` means the default build or UI reaches it; `dead` means only tests do. |

**Column 6.** "No gauge" is itself a finding, recorded in the `gauge` flag. A clamp with no gauge
is never tagged `PINNED` on a guess.

**Priority.** Priority combines correctness impact with reach. Reach means what the default build
actually executes, at what the L1 cells actually run: `--protocol-hint bulk` with
`--window-reliable` (`tools/l1/perf_rwm_c.sh:38`). The default hint is Auto, and Auto has never been
measured.

| P | rule |
|---|---|
| P0 | The decision is wrong by a large factor on the default path at a measured cell, with a user-visible cost. |
| P1 | Default path, and the law is wrong or unverifiable, with a plausible material cost. |
| P2 | Bounded cost, or narrow reach: ρ < 1, Realtime, one cell, or a candidate branch. |
| P3 | Opt-in arm, model only, dead code, or cosmetic. |

**Rank** orders the fixes within and across priorities. The tie-break puts the row with the most
dependents first. **Fix class** is kept separate from rank. It is one of: input, law change needing a
battery, gauge, paper, owner call, delete, merge, model, measurement.

## 2. Distribution

<!-- BEGIN:dist -->
- rows: 70
- verdict tags (a row may carry several): OK 13, DIVERGENT 18, UNDERIVED 30, PINNED 4, DEGENERATE 10, DISHONEST-INPUT 17, MODE-SWITCH 7
- rows that are OK only: 13, of which not measured in regime (OK but unverified): 9 (D01, D02, D03, D04, D05, D06, R06, R08, C12)
- priorities: P0 5, P1 10, P2 19, P3 36
<!-- END:dist -->

## 3. Summary, in fix order (every row is ranked; rows with fix class "none" come last)

<!-- BEGIN:summary -->
| id | law | verdicts | priority | rank | depends on |
|---|---|---|---|---|---|
| [E04](#e04) | Copa max_bw / btlbw (delivery-rate max) | DISHONEST-INPUT | P0 | 1 | - |
| [E03](#e03) | Copa min_rtt (RTprop) | DISHONEST-INPUT | P0 | 2 | - |
| [F01](#f01) | pooled store cap | PINNED(c8,c1s), DISHONEST-INPUT | P0 | 3 | E03, E04 |
| [C02](#c02) | packet threshold 3 and the 20-gap report horizon | UNDERIVED, DEGENERATE(c8: the law sees only old holes) | P0 | 4 | - |
| [C03](#c03) | snapshot-order filter (branch github/snapver-law, not on main) | OK | P0 | 5 | - |
| [K09](#k09) | quinn BBR (the default substrate CC) | DISHONEST-INPUT | P1 | 6 | - |
| [E01](#e01) | app-echo RTT EWMA (est.rtt()) | DISHONEST-INPUT | P1 | 7 | - |
| [E11](#e11) | pooled recovery srtt (max over live paths of est.rtt()) | DISHONEST-INPUT | P1 | 8 | E01 |
| [E10](#e10) | receiver's srtt for the refresh clock (published TX view) | DEGENERATE(all one-way cells) | P1 | 9 | E01 |
| [C07](#c07) | receiver hole refresh (hole_nack_refresh) | DEGENERATE(all one-way cells), UNDERIVED | P1 | 10 | E10, E01 |
| [C05](#c05) | per-sequence retransmit cooldown | DISHONEST-INPUT, UNDERIVED | P1 | 11 | E11 |
| [C01](#c01) | per-path time threshold 9/8 * max(srtt, ewma) | DISHONEST-INPUT | P1 | 12 | E01, E11 |
| [C09](#c09) | NACK repair throttle (NackCongestionState x MAX_NACK_REPAIRS_PER_NACK, per-period nack cap) | UNDERIVED | P1 | 13 | E01 |
| [R01](#r01) | rate mix r(beta) | DEGENERATE(L1 cells: Bulk with chi = 0) | P1 | 14 | R02, R05 |
| [R02](#r02) | controller_rate (anchor term body) | UNDERIVED | P1 | 15 | E07, E08, E01 |
| [C04](#c04) | legacy age gate srtt/2 (the shipped N = 1 recovery law) | DIVERGENT, UNDERIVED, DISHONEST-INPUT | P2 | 16 | E11 |
| [C06](#c06) | tail-sweep timeout | PINNED(c8), UNDERIVED, DISHONEST-INPUT | P2 | 17 | E11 |
| [R07](#r07) | W* derived window and the keep-behind clip | DIVERGENT, UNDERIVED | P2 | 18 | E07, E01 |
| [R03](#r03) | P_fec normal approximation (truncated) | DIVERGENT, DEGENERATE(below r = eps/(1-eps)) | P2 | 19 | - |
| [R04](#r04) | r_saturation soft cap | UNDERIVED, DIVERGENT | P2 | 20 | R03, E01 |
| [E07](#e07) | BOCD predictive_loss_upper (eps upper quantile) | DIVERGENT | P2 | 21 | E06 |
| [E08](#e08) | Gilbert-Elliott estimator (sigma2_burst, mean burst, mass stats) | DIVERGENT | P2 | 22 | E06 |
| [E09](#e09) | RTTVAR stand-ins (0.1*SRTT for P_lost, 0.125*SRTT for chi) | UNDERIVED, DIVERGENT | P2 | 23 | E01 |
| [R05](#r05) | completion exposure chi and BULK_TAIL_BUDGET | UNDERIVED, DEGENERATE(tunnel) | P2 | 24 | E09 |
| [F02](#f02) | store gate scope (tx_paused / plain_dyn_cap require reliable) | MODE-SWITCH(live) | P2 | 25 | F01 |
| [F03](#f03) | emission burst bound (Law 0, RWM_EMIT_BURST = 64) | PINNED(c1s,c1d), UNDERIVED | P2 | 26 | - |
| [S01](#s01) | span deadline D | DIVERGENT, DISHONEST-INPUT | P2 | 27 | E01, E03 |
| [S04](#s04) | shed budget | DIVERGENT, DISHONEST-INPUT | P2 | 28 | R03, S01 |
| [S05](#s05) | receiver shed hold | DIVERGENT, UNDERIVED | P2 | 29 | E10 |
| [S06](#s06) | taper copy decision (P_lost) | DISHONEST-INPUT, DEGENERATE(L1: never fires) | P2 | 30 | E01, E09 |
| [S08](#s08) | Realtime duplicate source send | MODE-SWITCH(live), UNDERIVED | P2 | 31 | - |
| [S09](#s09) | symbol packing and emission-batching scope (use_packing, emit_batch_on) | MODE-SWITCH(live), UNDERIVED | P2 | 32 | - |
| [K05](#k05) | placement cost (place_costs) | DIVERGENT, UNDERIVED | P2 | 33 | - |
| [K10](#k10) | bbr_rs (branch github/bbrrs-r3, not default) | DIVERGENT | P2 | 34 | - |
| [S10](#s10) | proactive repair emission (taper density and TaperBudget accrual) | DIVERGENT, DEGENERATE(r = 0) | P3 | 35 | R01 |
| [C13](#c13) | T_cut give-up horizon (wasm model) | PINNED(every measured rho < 1 point) | P3 | 36 | R03 |
| [M04](#m04) | wasm give-up law (cartoon + sim; was V5/V6 and W14) | DEGENERATE(rho inert at Auto and Realtime under the shipped R), DIVERGENT | P3 | 37 | R03 |
| [M03](#m03) | wasm rate beta-guard and the dead classic-hint machine | MODE-SWITCH(live), MODE-SWITCH(dead) | P3 | 38 | - |
| [M01](#m01) | wasm store pool 512 per path | DIVERGENT, UNDERIVED | P3 | 39 | F01 |
| [M02](#m02) | wasm tail target interpolation (Bulk anchor 0.05) | DIVERGENT | P3 | 40 | - |
| [K01](#k01) | Copa app-echo cwnd law (queue target, P10 floor, anchor floor and pull) | MODE-SWITCH(live), UNDERIVED, DISHONEST-INPUT | P3 | 41 | E03, E04 |
| [K02](#k02) | Copa delta off the wire signal (constant 0.5) | DIVERGENT | P3 | 42 | K01 |
| [K03](#k03) | Copa wire-mode law and TCP-compete mode (opt-in) | MODE-SWITCH(live) | P3 | 43 | - |
| [K04](#k04) | spare capacity and the active_paths saturation filter | DISHONEST-INPUT | P3 | 44 | K01 |
| [E02](#e02) | Copa srtt / PathState::srtt() | DISHONEST-INPUT | P3 | 45 | E01 |
| [S02](#s02) | span width A* | DISHONEST-INPUT | P3 | 46 | S01, E12 |
| [S03](#s03) | trailing offset Delta | UNDERIVED | P3 | 47 | - |
| [S07](#s07) | taper-cache staleness 50 ms | UNDERIVED | P3 | 48 | - |
| [R06](#r06) | r*_mass burst-tail provisioning (RWM_RSTAR_TAIL) | OK | P3 | 49 | E08 |
| [C08](#c08) | gap-report minimum interval GAP_ACK_MIN_INTERVAL = 2 ms | UNDERIVED | P3 | 50 | - |
| [C10](#c10) | reactive repair margin ceil(retx*eps) | UNDERIVED | P3 | 51 | - |
| [C11](#c11) | repair debt reduction (debt -= nacked*r) | UNDERIVED, DEGENERATE(r = 0) | P3 | 52 | R01 |
| [C12](#c12) | receiver request law (l*, request_bar, request_m; off) | OK | P3 | 53 | - |
| [K06](#k06) | placement temperature T = 0.15 | UNDERIVED | P3 | 54 | K05 |
| [K07](#k07) | diversity weight w_div = 1.0 | UNDERIVED | P3 | 55 | K05 |
| [K08](#k08) | repair spare near-tie band 0.8 / floor 0.25 (opt-in) | UNDERIVED | P3 | 56 | K04 |
| [E12](#e12) | SendRateAnchor (the rate A* multiplies) | UNDERIVED | P3 | 57 | E01 |
| [F04](#f04) | store-cap control and off arms (RWM_STORE_PATHS=0, RWM_DELTA_CAP=0, RWM_SUM_CAP=0, RWM_THREE_TERM) | UNDERIVED | P3 | 58 | F01 |
| [F05](#f05) | generation pipeline depth M* (opt-in seat) | UNDERIVED | P3 | 59 | E03 |
| [R09](#r09) | generation repair floor 0.15 / 0.20 (opt-in seat) | UNDERIVED, DIVERGENT | P3 | 60 | - |
| [E05](#e05) | quinn wire SRTT (wire_rtt mirror) | OK | P3 | 61 | - |
| [E06](#e06) | per-path loss estimate eps-hat (tx EWMA) and its feed | OK | P3 | 62 | - |
| [F06](#f06) | SACK-clocked release (store_gate_released) | OK | P3 | 63 | - |
| [R08](#r08) | rate cadence min(5 ms, SRTT/4) | OK | P3 | 64 | - |
| [D01](#d01) | delta(hint) named points / delta_price | OK | P3 | 65 | - |
| [D02](#d02) | b(delta) span horizon | OK | P3 | 66 | - |
| [D03](#d03) | q(delta) CoDel setpoint | OK | P3 | 67 | - |
| [D04](#d04) | beta(delta) bulkness | OK | P3 | 68 | - |
| [D05](#d05) | w_bw(delta) placement loss weight | OK | P3 | 69 | - |
| [D06](#d06) | t_tail(delta) tail target | OK | P3 | 70 | - |
<!-- END:summary -->

## 4. Fix order

Correctness comes first. Each item names what it waits on. "Ships with" means the items go in the
same change and battery: one without the other is wrong, not merely incomplete.

1. **E04 + E03 + F01: the store law and both of its inputs, as one change.**
   - **What is wrong.** The pooled cap reads `max_bw` at 11–24× the link and `min_rtt` at 7–33× the
     propagation delay at c8. It asks 12–43 k against a ceiling of 4 096 and sits on the clamp for
     0.74–0.98 of every run, on main.
   - **Why the fixes cannot be split.**
     - With honest inputs, the shipped law (1 + q)·Σ bw·RTprop undershoots. It has no
       hole/dwell term. At c2 it collapses to its floor (41 → 10 in 3 s, 7.2 against 89.9 Mbit/s;
       W1 §2 H1, §43).
     - So the input fixes alone break c2.
     - The law fix alone keeps reading 11–33× inputs.
   - **E04 (rank 1): an open input design, not a rewire.**
     - The default `max_bw` is the ack-interval sampler. It over-reads through ack bunching
       (§23 Q1 `MECHANISM-CONFIRMED`, paper §2.7). The honest send-interval sampler runs only on
       the Copa-feed seat.
     - Rewiring it is no fix. §23 showed that on a store-bound sender every delivered-rate sampler
       reads the cap's own throughput: the send-interval anchor under-reads ×0.1–0.5, and the law on
       it collapses to its floor. The input is circular.
     - Every candidate so far has failed:
       - the min-filtered loop time `INFLATES` (§24);
       - quinn's rate `OVER-READ`s ×7.6 (§24);
       - `bbr_rs`'s rate is `CIRCULAR` at c2 (§27);
       - receiver dispersion is `NOT-YET` (§36, §40);
       - the derivation is `NONE-CLOSES` (§34);
       - a netem-oracle L converges only at c2 (§43b).
     - The next step is a derivation: a rate the store cap does not set, or a law that does not
       need one. More samplers are not the next step.
   - **E03 (rank 2).** RTprop from a wire clock in place of a 10 s minimum of the app-echo clock.
     There are two candidates: quinn's `rtt.min()` and `bbr_rs` m̂. Neither has been tested as the
     store's RTprop. §24 option 1, the min-filtered app-echo loop time, `INFLATES`.
   - **F01 (rank 3).** Write the term the paper's own Little's-law sentence omits:
     - the hole population (1 250–1 500 slots at c8 on main);
     - the no-queue offset b₀ (§44 erratum), which has no engine source yet.

     Derive it in the paper first (FORMULA-FIRST), and put the `[SUMCAP] pin=` and `ask=` gauges on
     the battery.
   - **Soft dependency.** §48b (K09/K10) decides whether `bbr_rs`'s b̂/m̂ become the honest source.
     Do not wait on it to build the gauges and the derivation.

2. **C02 + C03: the c8 recovery horizon and the snapshot-order filter, as one change.**
   - **What is wrong.**
     - Under the 20-gap report horizon, main's packet threshold sees only holes about 3.5 s old.
     - The c8 tail is 3.95 s, and the mean in-order delay is about 5.3 s against about 1.0 s under
       lever B.
   - **Why both, together.**
     - Lever B alone raises duplicates (1 546 → 4 004), because about 99 % of the slow leg's
       reports are stale.
     - The filter brings duplicates and retransmits/GB back to or below main.
     - The filter is already formula-first on its branch.
   - **What remains.**
     - The §41c guard of the CAND + filter build against main.
     - The owner's merge call, with the costs §53 lists: c8 `rtp_floor_p0` +29 %, which is itself a
       run-minimum artefact (§3.10 item 7), and c1d duplicates +13 %.
   - **Dependency.** Independent of item 1. But the freed store slots land in quinn's p0 send
     buffer (p0-dwell-diag §2), so item 1's cap fix bounds that cost.

3. **K09: the default substrate.**
   - quinn's BBR latches a lifetime-minimum RTT, which collapses the c1s window (§47). At c2 its
     window is 1.4–4.8× the BDP.
   - `bbr_rs` is in the §48b battery.
   - Its verdict also settles the source question in item 1.

4. **E01 → E11 → E10/C07, C05, C01, C04: one honest recovery clock.**
   - **The receiver's refresh (E10 + C07).**
     - On every one-way transfer, the receiving endpoint's own sender sends nothing and gets no
       echoes. So the srtt the receiver borrows is the 50 ms `LossEstimator` seed, and the refresh
       is a constant 100 ms (§3.10 item 6, now traced to its source).
     - Publishing the data sender's current srtt does not fix it alone. At c8 that srtt is
       257–542 ms, and 2·srtt still clamps to 100 ms, so the law goes from DEGENERATE to PINNED
       with the same output.
     - The output moves only where an honest srtt is below 50 ms, as at c1s and c2 propagation. So
       E10 waits on E01.
   - **The clocks today.** Every recovery clock reads the app-echo EWMA. It carries the sender's own
     send-buffer dwell (64–282 ms at c8 on a 10 ms leg). The pooled form takes the max across legs.
   - **The effects.**
     - **Cooldown (C05):** 257–542 ms per seq on a 10 ms leg.
     - **Time threshold (C01):** about 215–390 ms.
     - **N = 1 law (C04, P2):** the shipped N = 1 recovery law is the pre-RFC `srtt/2` age gate,
       not the RFC threshold. `RWM_RECOV_MP_LAW` needs N > 1 and `RWM_RECOV_SP` is off, so
       c1s/c2/c3 run it. On a FIFO single path it probably suppresses little, but nobody has read
       `mpd_supp_age` there.
   - **The fix.** A per-path wire clock and an age from the wire send. The paper's C04 text can be
     fixed now. C01 at N = 1 (the existing `RWM_RECOV_SP` arm) is one law across N, and it needs a
     battery.

5. **C09: the NACK repair throttle (new in this ledger).**
   - A default-path AIMD heuristic decides how many retransmits a gap report may carry: at most
     10 × a multiplier, halved on "rising loss and rising RTT". It also sets a per-period cap.
   - It has seven unprovenanced constants. It appears nowhere in the paper, whether searched by
     symbol or by prose.
   - Its rising-RTT input is dwell-inclusive.
   - It has no gauge.
   - First a gauge: the per-report cap and multiplier bind fractions, at c8. By inference, not
     measurement, about 1 400 old holes compete for at most 10 retransmits per report there. Then a
     paper entry or deletion.
   - It matters for item 2's liveness note: a "spent repair budget" drops a batch.

6. **R01 + R02: the rate law has never run off its corner.**
   - On every measured cell r ≡ 0: Bulk, with χ ≡ 0 on the tunnel.
   - At the default hint (Auto) the anchor term has never been measured. No engine gauge says which
     `max()` term or clamp wins.
   - Register the off-Bulk which-term-wins row (§44 item 5, §45) before any further rate-law work.
   - R03, R04, R06, R07 and the taper grant (S10) cannot be verified until this runs.

7. **C06: the tail sweep.** It is pinned at 100 ms (30/30 c8 rows). Derive it from the self-heal
   lateness after item 4, since its input is the pooled clock.

8. **R07, R03, R04: owner calls already on file.**
   - The W* budget: 1 RTT at every δ, against the paper's b(δ).
   - The 200 clip (paper-recon Q1, Q2).
   - The P_fec truncation (§3.10 item 2): the paper's continuous Φ against the engine pin.
   - r_sat's constants.
   - All of them are bounded by tests. They wait on item 6 to matter.

9. **The estimators' approximations: E07, E08, E09.**
   - The BOCD quantile approximation is bounded. Its clean-link floor of 0.0354 is unexplained.
   - The GE batch lumping is unbounded: paper-recon §7 specified a test for it, and that test was
     never landed.
   - Replace the two RTTVAR stand-ins with one measured RTTVAR per leg. The engine already computes
     four, and only gauges read them.

**Then the ρ < 1 and Realtime set (P2, ranks 25–34).**
- The ρ term of the store gate (F02). §6.1 derives it.
- D and the shed budget, which read the dwell-inclusive EWMA (S01, S04).
- The receiver hold, which should be b(δ)·SRTT (S05).
- The two live hint equalities (S08, S09).
- The paper text for placement (K05) and `bbr_rs` (K10).

**Then the P3 tail (ranks 35–60).**
- Paper-only fixes: the taper grant law (S10), C08, C10.
- The model, from the wasm audit's F1–F4: the cartoon shed (M04), T_cut (C13) and the dead classic
  machine (M03). The 512 pool waits until F01 settles (M01).
- Copa's legacy controller and its readers (K01–K04, E02), deleted or reduced as §44 item 4 asks.
- Gauges and derivations for the remaining constants.
- The opt-in seats (F05, R09) wait for their disposition.

Ranks 61–70 owe no fix: E05, E06, F06, R08 and the six δ maps.

**Dependency graph (only edges that gate a fix).**

```text
   E04 ─┐
   E03 ─┼─ ship together ─ F01 ─→ M01, F02, F04
        └─(soft) K09/K10 §48b may supply the RTprop candidate
   C02 ═ C03 (ship together) ; C09 gauge informs C03's liveness
   E01 ─→ E11 ─→ C05, C01, C04, C06 ;  E01 ─→ E10 ═ C07 (ship together)
   E01 ─→ S01 ─→ S02, S04 ;  E01 ─→ E09 ─→ S06, R05
   R01 measurement ─→ R02, R03, R04, R06, R07, S10, C11
   R03 ─→ S04, C13, M04
```

`ships_with` overrides the order that `depends_on` alone suggests. F01 depends on E03 and E04, but
all three go in one change.

## 5. Findings this ledger adds

These are new, or newly traced, beyond the cited audits. Each was checked against the code lines
listed in the header.

1. **The N = 1 recovery law is the legacy age gate (C04).**
   - `serve_gaps` takes the per-path branch only when `mp_n_paths > 1`
     (`net/sender_phases.rs:898`). It takes the single-path time threshold only under
     `RWM_RECOV_SP`, which is off.
   - So `legacy_age_ripe` (`srtt/2`) is the shipped recovery decision at every single-path cell.
   - Paper §11.2 says it runs only with per-path detection off. That is wrong.
   - Status §3.4's "inert at c1" may still hold in effect: a FIFO path reports a gap about one RTT
     after the send, so age > srtt/2. No gauge reading backs it.
   - It is also a path-count law step at N = 1 → 2, the shape ADR-0072 removed from the store.
2. **The receiver refresh's 50 ms is the `LossEstimator` constructor seed (E10).**
   - The seed (`estimator.rs:281`) is read through the receiving endpoint's own idle sender
     (`publish_tx_view`).
   - PathReport RTTs are deliberately not recorded under `RWM_MSTAR_ANCHOR`
     (`control_msg.rs:429-436`).
   - This closes §3.10 item 6's "source not yet traced".
   - The obvious fix (publish the data sender's srtt) changes nothing at c8 until E01 is honest.
3. **A default-path recovery law with no paper entry (C09).** This is `NackCongestionState` and the
   10-per-report cap.
4. **The store law's two inputs fail for one reason (E03, E04).** Both are measured on the clock or
   rate that the store's own queue sets:
   - the RTprop is a minimum of a dwell-inclusive echo;
   - the rate is a delivered rate on a store-bound sender (§23).

   The over-read is circular, not noise. Past audits list both defects; they had not been joined as
   one dependency, nor put beside W1's finding that the honest-input law collapses.
5. **Stale texts.**
   - The `pooled_store_cap` doc comment says the pool funds "BDP + one recovery round of runway".
     The expression funds q·RTprop, which is 5–10 % of RTprop. The paper's §6.1 text is right.
   - Paper §11.2's knee row says the knee is "inert at c7/c8 under the δ-cap". It is pinned
     0.74–0.98 at c8.
   - Paper A.4's placement E_i line is the pre-ADR-0073 echo price. §5.7 is current.
   - Paper A.2 prints the superseded raw taper density. The shipped grant law (S10) is only in a
     code comment.
   - The `wire_rtt` field doc says "written only while the arm is on". The arm was promoted and
     deleted (ADR-0073), and the mirror is written unconditionally.
6. **Clamp gauges missing on default-path laws.**
   - There is none for r_sat / `max_overhead`, the W* terms, the 200 clip, `r*_mass`'s ceiling of
     2.0, the Δ cap of 64, the taper cache, the taper grant's caps, or C09.
   - Copa's anchor floor and coupling cap have none either. `clamp_cwnd_with_anchor` raises cwnd to
     the anchor floor (0.85·bdp). The lookup reports that it runs after the wire-mode coupling cap,
     so the floor can exceed the cap; that order was not re-read.

## 6. Full rows

Each row gives the ten audit columns in order. Line numbers are on `cbe2be3f` unless the row names
a branch.

<!-- BEGIN:rows -->

### Area: estimator

#### E01

**app-echo RTT EWMA (est.rtt())** (default, every cell)

- *1. location:* control/estimator.rs:465 record_rtt, :523 rtt(); fed net/control_msg.rs:298 (Ack), :572 (WindowAck); seed 50 ms estimator.rs:281
- *2. formula (code):* 

  ```text
  ewma <- 0.125*(now - echo_send_ts) + 0.875*ewma ; first sample replaces the 50 ms seed (RWM_MSTAR_ANCHOR, on)
  ```

- *3. paper:* §2.6 / §3.2 call it SRTT; §11.3 row 'span deadline' notes it is used where the model says RTprop
- *4. derivation:* cited (RFC 6298 gain 1/8). The sample is stamped at the placement lock, before the owner channel and quinn's send buffer (net/lat.rs header), so it is RTprop + network queue + sender dwell
- *5. measured in regime?:* measured as a gauge (`rtt=` on [DIAG]); p0-dwell-diag §1: rtt0 - wrtt0 = quinn send-buffer dwell within 6 ms on 24/30 c8 rows (64 ms BASE, 222 ms CAND)
- *6. clamp bind:* none
- *7. inputs honest?:* honest as an app-echo round trip; DISHONEST wherever a law reads it as a path RTT or RTprop (S01, S02, S06, E11, C01, C04, C05, C06, R07)
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **DISHONEST-INPUT**: c8: app-echo excess over the wire RTT equals quinn's p0 send-buffer dwell (64-282 ms on a 10 ms leg), p0-dwell-diag §1
- *10. fix class, priority:* input; **P1**, rank 7
- *flags:* measured yes, gauge yes

#### E02

**Copa srtt / PathState::srtt()** (default, every cell)

- *1. location:* scheduler/copa.rs:942-949 (7/8 EWMA of cc_rtt); scheduler/path.rs:690 srtt() = copa.srtt else est.rtt(); Copa DEFAULT_SRTT 50 ms copa.rs:248
- *2. formula (code):* 

  ```text
  srtt <- 7/8*srtt + 1/8*cc_rtt ; cc_rtt = wire RTT if copa_wire_active() else app-echo (control_msg.rs:298-308, :572-582)
  ```

- *3. paper:* §8.2; §44 item 3 notes it equals est.rtt() bit for bit while RWM_COPA_WIRE is off
- *4. derivation:* cited (RFC 6298). A duplicate of E01 on the default path
- *5. measured in regime?:* as E01
- *6. clamp bind:* none
- *7. inputs honest?:* same samples as E01 off-wire, so the same dwell; when the wire gate is on it smooths quinn's already-smoothed srtt (double smoothing)
- *8. NO-MODE-SWITCH:* RWM_COPA_WIRE gate selects the sample source (an experiment gate, not a dial)
- *9. verdict:* **DISHONEST-INPUT**: duplicate of E01 off-wire (§44 item 3); C01 takes max(srtt, ewma) of two copies of one stream
- *10. fix class, priority:* delete (keep one clock, §44 item 3); **P3**, rank 45; depends on E01
- *flags:* measured yes, gauge yes

#### E03

**Copa min_rtt (RTprop)** (default, every cell)

- *1. location:* scheduler/copa.rs:1000-1008 (monotone min-deque), window 10 s copa.rs:641, expiry :1490; accessor scheduler/path.rs:1070
- *2. formula (code):* 

  ```text
  min_rtt = min{ cc_rtt(t) : t > now - 10 s } ; cc_rtt = app-echo while RWM_COPA_WIRE is off (default)
  ```

- *3. paper:* §6.1 symbol table: 'windowed-min RTT, measured anchor'; §8.3; A.7 BtlBw anchor
- *4. derivation:* cited (BBR's 10 s RTprop filter), but applied to the app-echo clock, which carries the sender's own standing queue
- *5. measured in regime?:* measured at c8 (§3.10 item 9, p0-dwell-diag §2.1): 95-129 ms on main, 163-319 ms under lever B, on a 10 ms leg, from t = 10.3 s (start-up sample leaves the window) on 30/30 rows; §44 erratum: c2 250 ms min sits 1.7-2.5 ms above the 10 s min under jitter
- *6. clamp bind:* window 10 s
- *7. inputs honest?:* DISHONEST: a min of a dwell-inclusive clock never reads propagation once the sender's queue persists > 10 s; 7-33x propagation at c8
- *8. NO-MODE-SWITCH:* none (gate selects sample source)
- *9. verdict:* **DISHONEST-INPUT**: reads 95-319 ms on a 10 ms path at c8, 30/30 rows (§3.10 item 9; p0-dwell-diag §2.1, §4)
- *10. fix class, priority:* input: candidates are quinn's wire rtt.min() or bbr_rs m-hat (neither tested as the store's RTprop; §24 option 1, the min-filtered app-echo loop time, INFLATES); the §44 erratum's b0 offset needs a source too; **P0**, rank 2; ships with F01, E04
- *flags:* measured yes, gauge yes

#### E04

**Copa max_bw / btlbw (delivery-rate max)** (default, every cell)

- *1. location:* scheduler/copa.rs:746 record_delivery (ack-interval, the DEFAULT feeder via scheduler/path.rs:484 on_ack when no CopaFeed); copa.rs:850 rs_on_delivered (send-interval sampler) fed only from net/copa_feed.rs:172, i.e. only under RWM_COPA_FEED/passthrough; max filter copa.rs:723; gate effective_btlbw :1472 (>= 8 samples)
- *2. formula (code):* 

  ```text
  max_bw = max over the window of (delta_delivered / delta_t_ack), delta_t >= 1 ms ; window 10 s (ack-interval feeder) ; bdp_anchor = max_bw * min_rtt (copa.rs:1458)
  ```

- *3. paper:* §2.7 (fec-arq-model.md:400-416) documents it: the default ack-interval sampler over-reads x4.6-7.4 and the over-read is 'partly load-bearing'; the honest send-interval sampler (RWM_PLAIN_RS, removed §49) read 1.02x the link but cost c2 20 %; §6.1 symbol table
- *4. derivation:* the send-interval sampler is cited (BBR delivery-rate sampling) and survives only on the Copa-feed seat; §23 showed that on a store-bound sender BOTH sampler families measure the rate the cap lets through, so neither can be the store law's input alone (paper §2.7)
- *5. measured in regime?:* measured at c8 (p0-dwell-diag §4): 110k-240k sym/s against a 9.8k/s link (11-24x); c1 x8-29 (§23 Q1 MECHANISM-CONFIRMED at c1s and c2: ack-interval samples over bunched acks); the send-interval anchor UNDER-reads x0.1-0.5 on a store-bound sender and the shipped law on it collapses to its floor (§23 Q2)
- *6. clamp bind:* window 10 s; >= 8 samples
- *7. inputs honest?:* DISHONEST and circular: the ack-interval rate measures ack bunching (§23 Q1), and any delivered-rate sampler on a store-bound sender reads the cap's own throughput (§23). The c8 11-24x is presumably the same mechanism (p0-dwell-diag §7 left it undiagnosed; not confirmed here)
- *8. NO-MODE-SWITCH:* RWM_COPA_FEED selects the sampler (an experiment gate)
- *9. verdict:* **DISHONEST-INPUT**: max_bw 11-24x the c8 link rate (p0-dwell-diag §4), x8-29 at c1 by the ack-bunching mechanism (§23 Q1); the honest send-interval sampler runs only on the Copa-feed seat (on_src_delivered_seq's one caller is copa_feed.rs:172) and under-reads a store-bound sender (§23 Q2)
- *10. fix class, priority:* open input design: no candidate has passed - send-interval sampler under-reads (§23), min-filtered loop time INFLATES and quinn's rate OVER-READs x7.6 (§24), bbr_rs reads the delivered rate, CIRCULAR at c2 (§27), receiver dispersion NOT-YET (§36, §40), derivation NONE-CLOSES (§34), netem-oracle L converges only at c2 (§43b); **P0**, rank 1; ships with F01, E03
- *flags:* measured yes, gauge yes

#### E05

**quinn wire SRTT (wire_rtt mirror)** (default, every cell (placement base))

- *1. location:* transport/io_owner.rs:344 (published view, Connection::rtt()); scheduler/mod.rs:196 mirror_wire_rtts -> scheduler/path.rs:679, called unconditionally from net/control_msg.rs:236/419/510
- *2. formula (code):* 

  ```text
  wire_rtt_i = quinn smoothed path RTT (packet-timed; includes the network queue, excludes quinn's send buffer)
  ```

- *3. paper:* §5.7 placement base (ADR-0073); §8.4
- *4. derivation:* cited (quinn / RFC 9002 smoothed_rtt)
- *5. measured in regime?:* measured (`wrtt=`): c8 p1 181-445 ms on a 40 ms leg is netem queue (p0-dwell-diag §1)
- *6. clamp bind:* none
- *7. inputs honest?:* honest for what placement prices (wire transit); blind to quinn's send buffer, so placement cannot see p0's 1.5-3k datagram backlog (p0-dwell-diag §4)
- *8. NO-MODE-SWITCH:* none. Code nit: the path.rs:28-35 field doc says 'written only while the arm is on', the code writes unconditionally
- *9. verdict:* **OK**: the honest wire clock; the candidate replacement source for E03 and E11
- *10. fix class, priority:* none (stale field doc); **P3**, rank 61
- *flags:* measured yes, gauge yes

#### E06

**per-path loss estimate eps-hat (tx EWMA) and its feed** (default, every cell)

- *1. location:* control/estimator.rs:310 record_batch, :498 loss_rate() (alpha 0.1); credit LossCredit estimator.rs:35-57; tracker net/mod.rs:3698
- *2. formula (code):* 

  ```text
  eps <- 0.1*(lost/sent)_batch + 0.9*eps ; fed = max(0, raw - c), c carried (late-arrival credit)
  ```

- *3. paper:* §2.6, §11.3 per-path loss feed rows (tracker and credit laws written out)
- *4. derivation:* declared EWMA gain; credit law derived in §11.3 and bounded by s10_tracker_true_loss_plus_reorder_reads_exactly_the_true_loss, loss_credit_law_anchor_points
- *5. measured in regime?:* measured against per-datagram truth: plc/truth 0.95-1.03 at c2, c3 and both c8 legs (§3.6, §3.8); reorder credit never exercised on L1 (netem FIFO, §3.8)
- *6. clamp bind:* none
- *7. inputs honest?:* honest (datagram-level truth match)
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **OK**: fed loss matches per-datagram truth 0.95-1.03x (§3.6, §3.8); reorder branch tested only in unit tests
- *10. fix class, priority:* none; **P3**, rank 62
- *flags:* measured yes, gauge yes

#### E07

**BOCD predictive_loss_upper (eps upper quantile)** (default (rate law, W*, NACK budget))

- *1. location:* raptorpath-math/src/changepoint.rs:203 predictive_quantile; control/estimator.rs:515 (Beta fallback < 5 updates); confidences 0.95 (fec_rate.rs:491, :543) and 1 - t_tail (net/mod.rs:3235)
- *2. formula (code):* 

  ```text
  q_c = sum_l w_l * Q_l(c) / sum_l w_l, Q_l = clamp(mean_l + Phi^-1(c)*sd_l, 0, 1) (run-length-weighted AVERAGE of quantiles)
  ```

- *3. paper:* §2.6; §11.3 BOCD row: model = mixture quantile, engine = average of quantiles
- *4. derivation:* cited (Adams & MacKay 2007) for BOCD; the averaging is an approximation with no derivation of its error
- *5. measured in regime?:* plu reads ~0.0354 on clean links, a floor from the prior and run-length mix, unverified (§3.7)
- *6. clamp bind:* [0,1]; MAX_RUN_LENGTH 200, hazard 0.01
- *7. inputs honest?:* fed by E06
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **DIVERGENT**: divergence bounded by bocd_quantile_is_the_weighted_average_of_quantiles_not_the_mixture_quantile (gap 0.003-0.0045); clean-link floor 0.0354 unexplained (§3.7)
- *10. fix class, priority:* paper or law (bounded); **P2**, rank 21; depends on E06
- *flags:* measured partial, gauge yes

#### E08

**Gilbert-Elliott estimator (sigma2_burst, mean burst, mass stats)** (default (rate law, W*, r_sat))

- *1. location:* raptorpath-math/src/gilbert_elliott.rs (decay 0.999, valid >= 30 transitions); fed by control/estimator.rs:310 record_batch (lost Bad then received Good)
- *2. formula (code):* 

  ```text
  p_gb, p_bg from decayed transition counts ; sigma2 = max(1, 1 + 2(1-p-q)/(p+q)) ; B = 1/p_bg
  ```

- *3. paper:* §2.3-2.4, A.1; §11.3 GE estimator row: model per-symbol sequence, engine per-batch counts with losses first
- *4. derivation:* cited (Gilbert-Elliott); the batch lumping is an undeclared approximation
- *5. measured in regime?:* GE adequacy against real traces checked offline (§2.5); burst length on L1 is GSO-inflated by the datagrams-per-skb factor (§3.8)
- *6. clamp bind:* sigma2 >= 1
- *7. inputs honest?:* lumped batch order biases burst length upward when a batch mixes losses and receives; window batches carry one symbol (§11.3), so the bias is small on the window path
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **DIVERGENT**: per-batch lumping is unbounded: the specified ge_batch_feed_orders_losses_before_receives test (paper-recon §7) was not landed by bound-tests
- *10. fix class, priority:* test (bound) then paper; **P2**, rank 22; depends on E06
- *flags:* measured partial, gauge no

#### E09

**RTTVAR stand-ins (0.1*SRTT for P_lost, 0.125*SRTT for chi)** (default (P_lost taper); chi opt-in)

- *1. location:* net/channel_set.rs:69 (rttvar = 0.1*srtt); net/sender_phases.rs:1316 (0.125*srtt); measured variants unused by any law: copa.rs:970 rtt_var_sq, :979 rtt_mdev, path.rs:798 qsp_us, :943 tlag_us
- *2. formula (code):* 

  ```text
  rttvar_Plost = 0.1*est.rtt() ; rttvar_chi = 0.125*est.rtt()
  ```

- *3. paper:* §11.3 P_lost row (0.1); paper-recon N2 (two stand-ins, undocumented in §11.3)
- *4. derivation:* none (two different constants for one symbol)
- *5. measured in regime?:* not measured; the measured dispersions exist only as [DIAG] gauges
- *6. clamp bind:* P_lost floors rttvar at 1 ms
- *7. inputs honest?:* a fixed fraction of a dwell-inclusive clock, not a variance
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **UNDERIVED, DIVERGENT**: bounded by p_lost_rttvar_is_a_tenth_of_srtt and chi_and_p_lost_rttvar_stand_ins_differ_by_the_declared_ratio; no provenance for either constant
- *10. fix class, priority:* input (one measured RTTVAR per leg); **P2**, rank 23; depends on E01
- *flags:* measured no, gauge n/a

#### E10

**receiver's srtt for the refresh clock (published TX view)** (default, every unidirectional reliable transfer (all L1 cells))

- *1. location:* net/receiver.rs:729-736 (max over live_tx_views(..).srtt), :2237 live_tx_views; published by scheduler/mod.rs:467-479 publish_tx_view = PathState::srtt() of the receiving endpoint's OWN sender
- *2. formula (code):* 

  ```text
  srtt_recv = max_i TxPublished_i.srtt = local sender's p.srtt(), else est.rtt(), else the 50 ms seed
  ```

- *3. paper:* §7.1 refresh row reads 'SRTT' with no source
- *4. derivation:* none: the receiver borrows its own sender's clock, which measures nothing when that sender sends no data
- *5. measured in regime?:* [QCLK] site=receiver: srtt = exactly 50 000 us with sigma n = 0 on 12/12 c8 rows (§3.10 item 6); source now traced (below)
- *6. clamp bind:* none
- *7. inputs honest?:* DEGENERATE: on a one-way transfer the receiving endpoint's sender gets no echoes (PathReport avg_rtt is not recorded under RWM_MSTAR_ANCHOR, control_msg.rs:429-436), so the value is the constructor seed estimator.rs:281
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **DEGENERATE(all one-way cells)**: 50 000 us, n = 0, 12/12 c8 rows (§3.10 item 6); traced here to the 50 ms LossEstimator seed read through the receiver's own idle sender
- *10. fix class, priority:* input (publish the data sender's measured RTT to the receiver, or measure at the receiver); only useful once that RTT is honest (E01); **P1**, rank 9; depends on E01; ships with C07
- *flags:* measured yes, gauge yes

#### E11

**pooled recovery srtt (max over live paths of est.rtt())** (default, multipath cells mostly)

- *1. location:* net/recovery_laws.rs:267 pooled_recovery_srtt_iter (fallback 10 ms), :251; used net/mod.rs:2719, net/sender_phases.rs:708-716
- *2. formula (code):* 

  ```text
  srtt_pool = max_{i in live} est_i.rtt()  (10 ms if no path)
  ```

- *3. paper:* A.6 / §7.1 write 'SRTT'; §11.2 legacy age row notes 'a max over live paths' and the dwell
- *4. derivation:* none for the max: a per-sequence clock is set by the slowest leg's dwell-inclusive RTT
- *5. measured in regime?:* [QCLK] srtt_us_mean 257-375 ms (20-gap rows), 375-542 ms (lever B) at c8 on 10/40 ms legs (p0-dwell-diag §4)
- *6. clamp bind:* none
- *7. inputs honest?:* DISHONEST: max of E01 across legs, so a hole on the 10 ms leg is clocked by the 40 ms leg's queue plus p0's own send-buffer dwell
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **DISHONEST-INPUT**: 257-542 ms at c8 where the legs' propagation RTTs are 10/40 ms (p0-dwell-diag §4)
- *10. fix class, priority:* input (per-path wire clock; per-seq clock of the seq's own path); **P1**, rank 8; depends on E01
- *flags:* measured yes, gauge yes

#### E12

**SendRateAnchor (the rate A* multiplies)** (default (unified span))

- *1. location:* control/anchor.rs:185-335; bucket clamp(srtt/2, 5, 250 ms) :197; window clamp(8*srtt, 0.5, 10 s) :201; read net/emit_source.rs:563
- *2. formula (code):* 

  ```text
  rate = windowed max of (count/dt) over send buckets
  ```

- *3. paper:* §5.3 (A* = ceil(rate*D))
- *4. derivation:* window and bucket constants declared, not derived
- *5. measured in regime?:* not measured on its own
- *6. clamp bind:* the two clamps above; no gauge
- *7. inputs honest?:* srtt argument is est.rtt() (E01)
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **UNDERIVED**: four window/bucket constants with no provenance; feeds only A* (S02), which acts only through repairs (r ~ 0 on the measured cells)
- *10. fix class, priority:* paper; **P3**, rank 57; depends on E01
- *flags:* measured no, gauge no


### Area: dial

#### D01

**delta(hint) named points / delta_price** (default)

- *1. location:* net/store_cap.rs:311 delta_price; scheduler/copa.rs:47 hint_delta_price; control/fec_rate.rs:70 tail_loss_scale
- *2. formula (code):* 

  ```text
  delta = RWM_DELTA if set else 0.5/zeta(hint), zeta in {0.01, 1, 100} -> {50, 0.5, 0.005}
  ```

- *3. paper:* A.3 line 1, §4.1
- *4. derivation:* declared dial: named points
- *5. measured in regime?:* only the Bulk point is measured on L1 (perf_rwm_c.sh HINT default bulk)
- *6. clamp bind:* none
- *7. inputs honest?:* n/a
- *8. NO-MODE-SWITCH:* the one sanctioned hint read; everything downstream reads delta
- *9. verdict:* **OK**: delta_budget_b_is_the_dial_not_a_mode pins 50/0.5/0.005 bit-exactly
- *10. fix class, priority:* none; **P3**, rank 65
- *flags:* measured partial, gauge yes

#### D02

**b(delta) span horizon** (default)

- *1. location:* raptorpath-math/src/lib.rs:183 span_horizon_b; net/store_cap.rs:294 delta_budget_b
- *2. formula (code):* 

  ```text
  b = clamp(2^(-0.5*log10(delta/0.5)), 0.5, 2)
  ```

- *3. paper:* A.4 line 1, §5.4; same shape
- *4. derivation:* declared dial map (§5.4)
- *5. measured in regime?:* b = 2 (Bulk) only, on L1
- *6. clamp bind:* [0.5, 2] = the dial's own range; binds outside the presets only
- *7. inputs honest?:* D01
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **OK**: published_span_horizon_b_equals_the_engine_over_the_whole_delta_dial (formula_agreement.rs:556)
- *10. fix class, priority:* none; **P3**, rank 66
- *flags:* measured partial, gauge yes

#### D03

**q(delta) CoDel setpoint** (default)

- *1. location:* net/store_cap.rs:130 codel_setpoint_q, constants :141/:146
- *2. formula (code):* 

  ```text
  q = 0.05 + 0.05*(clamp(b,0.5,2) - 0.5)/1.5 = (b+1)/30
  ```

- *3. paper:* A.5, §6.1 symbol table; same shape (affine in b, increasing in b, units: a ratio of RTprop)
- *4. derivation:* cited endpoints (RFC 8289 §3.2), declared affine map between them
- *5. measured in regime?:* [DCAP] q = 0.1 at Bulk; the law it scales (F01) is pinned at c8 and c1s, so q's effect is unmeasured; §44 erratum: knees need ~1.2 where q stops at 0.10
- *6. clamp bind:* b clamp = the dial's range
- *7. inputs honest?:* D02
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **OK**: formula provenanced and pinned (published_codel_setpoint_equals_the_engine_map_and_spans_the_derived_band); unverified because F01 is pinned
- *10. fix class, priority:* none (see F01); **P3**, rank 67
- *flags:* measured no, gauge yes

#### D04

**beta(delta) bulkness** (default (rate mix))

- *1. location:* raptorpath-math/src/lib.rs:196 bulkness_of_delta; resolved once control/fec_rate.rs:340
- *2. formula (code):* 

  ```text
  beta = clamp((log10 0.5 - log10 delta)/2, 0, 1)
  ```

- *3. paper:* A.3; same shape
- *4. derivation:* declared dial map
- *5. measured in regime?:* beta = 1 (Bulk) only, on L1
- *6. clamp bind:* [0,1]; flat (beta = 0) across the whole Realtime half (§44 item 1)
- *7. inputs honest?:* D01
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **OK**: the_bulkness_weight_is_exact_at_the_presets_and_the_ablation_zeroes_it; flat over [Auto, Realtime] by construction
- *10. fix class, priority:* none; **P3**, rank 68
- *flags:* measured partial, gauge no

#### D05

**w_bw(delta) placement loss weight** (default (placement))

- *1. location:* scheduler/mod.rs:90-93 SchedulingWeights::from_delta
- *2. formula (code):* 

  ```text
  w_bw = clamp(0.5 - 0.25*log10(delta/0.5), 0, 1)
  ```

- *3. paper:* A.4; same shape
- *4. derivation:* declared dial map
- *5. measured in regime?:* w_bw = 1 (Bulk) only; Track A placement battery INERT-AS-DERIVED on latency
- *6. clamp bind:* [0,1]
- *7. inputs honest?:* D01
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **OK**: scheduling_weights_are_the_dial_not_a_mode pins {0, 0.5, 1}
- *10. fix class, priority:* none; **P3**, rank 69
- *flags:* measured partial, gauge no

#### D06

**t_tail(delta) tail target** (default (rate law, W*))

- *1. location:* control/fec_rate.rs:335-337; base config.rs:337 (1e-5)
- *2. formula (code):* 

  ```text
  t_tail = clamp(1e-5 * zeta(delta), 1e-9, 0.1) -> 1e-7 / 1e-5 / 1e-3
  ```

- *3. paper:* A.3 line 2, §4.1; same shape
- *4. derivation:* declared dial (base 1e-5 declared)
- *5. measured in regime?:* Bulk point only; at Bulk with chi = 0 the anchor term is weighted 0 (R01), so t_tail is never exercised on L1
- *6. clamp bind:* [1e-9, 0.1]
- *7. inputs honest?:* D01
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **OK**: continuous and pinned; never exercised on a measured cell
- *10. fix class, priority:* none; **P3**, rank 70
- *flags:* measured no, gauge no


### Area: flow control

#### F01

**pooled store cap** (default, every reliable cell)

- *1. location:* net/store_cap.rs:76 pooled_store_cap, :177 unclamped, :162 multiplier; seat net/sender_phases.rs:191 (5 ms refresh :64); input net/channel_set.rs:125 store_cap_pool_inputs; gate net/mod.rs:2393
- *2. formula (code):* 

  ```text
  cap = clamp( ceil( (1 + q(delta)) * sum_{i in live} max_bw_i * min_rtt_i ), 10, N*2048 ) ; boot 128 before any anchor
  ```

- *3. paper:* A.5 line 1, §6.1. Shape check: units symbols (sym/s x s); order 1 in N; non-decreasing in every bw_i, RTprop_i; increasing in b. Paper and code agree. The code doc comment (store_cap.rs:55-57) says 'BDP + one recovery round of runway', which the expression does not compute (runway is q*RTprop = 5-10 % of RTprop): stale comment. §6.1 derives a hole-residence term + eps_i*bw_i*stall(delta,rho,i) and does not ship it. Stale: §11.2's knee row says the knee is 'inert at c7/c8 under the delta-cap', against pin = 0.74-0.98 at c8
- *4. derivation:* §6.1: q cited (RFC 8289), floor derived (resource bound), knee 2048 a resource bound with a stale measured value, boot 128 'a fit to one cell' (§11.2). The Little's-law sentence omits the hole population, which p0-dwell-diag §2.2 measures at 1 250-1 500 slots at c8 on main
- *5. measured in regime?:* c8: PINNED at N*pool = 4096 for 0.74-0.98 of each run, all five §53 arms (§3.10 item 8, §53 Erratum 1); c1s: knee binds in 70 % of windows (§19, paper §6.1); c2: reaches the knee only through the ack-interval over-read, and with an honest sampler contracts to its floor, 41 -> 10 in 3 s, 7.2 vs 89.9 Mbit/s (W1 §2 H1, §43)
- *6. clamp bind:* ceiling N*2048: gauge [SUMCAP] pin=, 0.74-0.98 (c8), ~0.7 (c1s); floor 10: [SUMCAP] floor=; boot 128: no gauge; law asks 12-43 k at c8 ([DCAP] ask=)
- *7. inputs honest?:* both inputs DISHONEST (E03 min_rtt 7-33x, E04 max_bw 11-24x), so ask = 3-10x the ceiling and a positive loop queue -> min_rtt -> ask is cut only by the clamp (p0-dwell-diag §4). With honest inputs the law undershoots (no hole/dwell term): W1 H1, §44 erratum (b0 offset has no engine source)
- *8. NO-MODE-SWITCH:* none on the default seat. Path-count: none (ADR-0072). rho: see F02
- *9. verdict:* **PINNED(c8,c1s), DISHONEST-INPUT**: pin = 0.74-0.98 at c8 on main (§53 Erratum 1); ask 12-43 k vs 4096; honest-input form collapses to the floor at c2 (W1 H1) - the law needs its hole/dwell term in the same change as honest inputs
- *10. fix class, priority:* law change needing a battery, shipped with the input fixes; **P0**, rank 3; depends on E03, E04; ships with E03, E04
- *flags:* measured yes, gauge yes

#### F02

**store gate scope (tx_paused / plain_dyn_cap require reliable)** (rho < 1 (Realtime default))

- *1. location:* net/mod.rs:2393 tx_paused = reliable && (...); net/sender_policy.rs:521 plain_dyn_cap = reliable && ...
- *2. formula (code):* 

  ```text
  gate(rho=1) = store_len >= cap ; gate(rho<1) = none
  ```

- *3. paper:* §6.1 'where rho enters' derives the rho term and states the engine has no rho < 1 seat; §3.3 row 'reliable boolean branches'
- *4. derivation:* the composing form is derived in §6.1 (stall = (1-rho)*D + rho*17/8*SRTT); not shipped
- *5. measured in regime?:* rho < 1 not measured except the crown spot (Realtime, §2 row)
- *6. clamp bind:* n/a
- *7. inputs honest?:* n/a
- *8. NO-MODE-SWITCH:* MODE-SWITCH(live): the reliable bool selects whether any outstanding gate exists
- *9. verdict:* **MODE-SWITCH(live)**: §3.3 last row; at rho < 1 there is no sent_store and no store gate (paper §6.1)
- *10. fix class, priority:* law change (rho term from §6.1); **P2**, rank 25; depends on F01
- *flags:* measured no, gauge no

#### F03

**emission burst bound (Law 0, RWM_EMIT_BURST = 64)** (default, rho = 1, non-Realtime)

- *1. location:* net/emit_burst.rs:28 emit_burst_bound(q) = q; gates.rs:363-365 (default 64, clamp [2,512]); loop net/mod.rs:3135
- *2. formula (code):* 

  ```text
  burst <= 64 symbols per pacer quantum
  ```

- *3. paper:* A.5 last line, §11.2 row
- *4. derivation:* none ('~64 KB', not swept; §11.2)
- *5. measured in regime?:* measured at the §8 re-run: binds 0.838 (c1s), 0.831 (c1d), 0.005 (c2), 0.118 (c8)
- *6. clamp bind:* the law is a constant; gauge eb_end cap: on [DIAG]
- *7. inputs honest?:* none
- *8. NO-MODE-SWITCH:* scope excludes rho < 1 and Realtime packing (S09)
- *9. verdict:* **PINNED(c1s,c1d), UNDERIVED**: the cap ends 0.83-0.84 of bursts at c1s/c1d (§11.2); 64 never swept
- *10. fix class, priority:* sweep or derive (a CPU/latency quantum, not a correctness law); **P2**, rank 26
- *flags:* measured yes, gauge yes

#### F04

**store-cap control and off arms (RWM_STORE_PATHS=0, RWM_DELTA_CAP=0, RWM_SUM_CAP=0, RWM_THREE_TERM)** (opt-in)

- *1. location:* net/sender_phases.rs:179 (three-term), :210 (control chain clamp(2*bdp,10,1024)); store_cap.rs:162/:190; three_term_store_cap store_cap.rs:552
- *2. formula (code):* 

  ```text
  control: clamp(ceil(2.0*bdp), 10, 1024) ; DELTA_CAP=0: gain 2.0 ; SUM_CAP=0: N*Sigma (quadratic) ; three-term: Sigma rate*K*RTprop + Sigma rate*stall + 2*rate_fast*skew, clamp to 4096
  ```

- *3. paper:* §6.2 (control), §6.4/§10 (three-term refuted)
- *4. derivation:* gain 2.0 and RELIABLE_STORE_MAX 1024 unprovenanced (§11.2)
- *5. measured in regime?:* control measured as the comparison arm (§18/§19)
- *6. clamp bind:* 1024 bound in every post-warm-up c1s window while it was the shipped single-path ceiling (§11.2)
- *7. inputs honest?:* as F01
- *8. NO-MODE-SWITCH:* experiment gates, not dials
- *9. verdict:* **UNDERIVED**: kept as A/B arms; the quadratic SUM_CAP=0 arm is pinned as a documented defect (path_scaled_store_cap_value_is_quadratic_in_n_the_documented_defect)
- *10. fix class, priority:* delete when the F01 fix lands (keep one control); **P3**, rank 58; depends on F01
- *flags:* measured partial, gauge partial

#### F05

**generation pipeline depth M* (opt-in seat)** (opt-in (--window-generation-coding))

- *1. location:* net/mod.rs:473 gen_pipe_depth, :448 GEN_PIPE_MAX_GENS = 32; refresh :2245-2271
- *2. formula (code):* 

  ```text
  M* = clamp(ceil(rate_max*2*RTprop/G) + 1, 2, 32) ; cap = min(M*G, store_max)
  ```

- *3. paper:* A.4 line 4; §11.2 M* row
- *4. derivation:* 2 = cold-start pipeline, 32 a declared memory backstop
- *5. measured in regime?:* not measured (seat opt-in, §3.2)
- *6. clamp bind:* [2,32]: no bind gauge ([GPIPE] prints changes only)
- *7. inputs honest?:* RTprop = max over live of Copa min_rtt (E03)
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **UNDERIVED**: opt-in seat with no keep-or-remove decision (§3.2)
- *10. fix class, priority:* decide the seat's disposition (§3.2) first; **P3**, rank 59; depends on E03
- *flags:* measured no, gauge no

#### F06

**SACK-clocked release (store_gate_released)** (default, every reliable cell (RWM_STORE_SACK_RELEASE on))

- *1. location:* net/sack.rs:397 store_gate_released, :361 received_above_released, :387 store_released_for_gate; store_len net/mod.rs:2223 (sack_release_outstanding sack.rs:461)
- *2. formula (code):* 

  ```text
  store_len = retained - released ; released = min(|S|, max(|M|, |S ∩ [0,F)| + (A - ((H+1-F) - |S ∩ [F,H]|))+))
  ```

- *3. paper:* A.5 lines 3-4, §6.3; same shape (counts, order 1, no constant)
- *4. derivation:* §6.3 derives it as a lower bound on |S ∩ received| from v9 wire fields; the outer min is a totality clamp that cannot bind
- *5. measured in regime?:* c8: on 20-gap rows the gate's released count is 28-75 k against 4-26 k SACK marks, so the received-above count does the work (p0-dwell-diag §1, srel= gauge)
- *6. clamp bind:* outer min cannot bind (derived)
- *7. inputs honest?:* honest (wire fields F, A); premise: the reliable receiver never evicts above its frontier (rho = 1 only)
- *8. NO-MODE-SWITCH:* none (rho < 1 has no store gate: F02)
- *9. verdict:* **OK**: bounded by store_gate_released_never_exceeds_the_retained_received_truth and stalled_frontier_store_gate_converges_past_the_sack_cap
- *10. fix class, priority:* none; **P3**, rank 63
- *flags:* measured yes, gauge yes


### Area: rate

#### R01

**rate mix r(beta)** (default, every cell)

- *1. location:* control/fec_rate.rs RateSnapshot::rate (~:130); evaluated via net/emit_source.rs:871 cadenced_repair_rate
- *2. formula (code):* 

  ```text
  r = (1 - beta)*r_anchor + beta*r_late-is-fine ; both terms always computed
  ```

- *3. paper:* A.3, §4.5; same shape; continuity bound |r(beta) - r_bulk| <= (1-beta)*r_max
- *4. derivation:* §4.5 (declared mix, beta from D04)
- *5. measured in regime?:* r ~ 0 on every measured cell (§37, §44 item 5, paper-recon §4). L1 runs --protocol-hint bulk (tools/l1/perf_rwm_c.sh:38), beta = 1, chi = 0 on the tunnel, so r = r_late = 0 identically. The default hint (Auto, beta = 0) has never been measured; the off-Bulk which-term-wins row (§44 item 5, §45) does not exist
- *6. clamp bind:* via R02
- *7. inputs honest?:* R02's inputs
- *8. NO-MODE-SWITCH:* none in the engine (no beta = 0 fast path); the wasm has one (M03)
- *9. verdict:* **DEGENERATE(L1 cells: Bulk with chi = 0)**: the_rate_mix_is_byte_identical_at_the_presets and the no-step test pin the shape; on every measured cell it computes exactly 0 (bulk_rate_is_exactly_zero_through_the_controller)
- *10. fix class, priority:* measurement (one off-Bulk row with a which-term-wins tally); **P1**, rank 14; depends on R02, R05
- *flags:* measured no, gauge partial

#### R02

**controller_rate (anchor term body)** (default (Auto anchor term))

- *1. location:* raptorpath-math/src/lib.rs:735 controller_rate; inputs control/fec_rate.rs:451-530 rate_snapshot
- *2. formula (code):* 

  ```text
  r = clamp( soft_sat( max(r_core + codec_eff, r_burst, r_mass), r_sat ), 0, 0.5 ) ; r_core = max(0, p/(1-p) + Phi^-1(1 - t/p)*sqrt(p*sigma2/(W(1-p)))) ; r_burst = (B/T_rtt)*(1 - t/p)+
  ```

- *3. paper:* A.3 lines 4-7, §4.2-4.4; same shape. Code drops codec_eff when r_core = 0; paper does not say
- *4. derivation:* r_core derived (§4.2); r_burst and the max of three terms declared (§4.4); max_overhead 0.5 declared
- *5. measured in regime?:* never off the corner on a measured cell (R01)
- *6. clamp bind:* max_overhead 0.5 and soft_sat: no engine gauge of either bind or of which max() term wins (the wasm has get_saturation_pressure)
- *7. inputs honest?:* p = E07 upper quantile, sigma2/B = E08, srtt = E01, t_symbols = srtt*tput/sym
- *8. NO-MODE-SWITCH:* bulk_late_is_fine bool selects the term inside R01's always-computed mix (a term selector, not a mode)
- *9. verdict:* **UNDERIVED**: the composition max(core, burst, mass) has no derivation of why max (§4.4 'partial' in §9.10); no gauge of which term or clamp binds
- *10. fix class, priority:* gauge (term-win tally) then measurement; **P1**, rank 15; depends on E07, E08, E01
- *flags:* measured no, gauge no

#### R03

**P_fec normal approximation (truncated)** (default via r_sat (R04) and rho < 1 via the shed budget (S04))

- *1. location:* raptorpath-math/src/lib.rs:885 p_fec_normal (cut :886-888); callers r_saturation lib.rs:917 and residual_loss_after_fec (net/emit_source.rs:588)
- *2. formula (code):* 

  ```text
  P_fec = 0 if r(1-eps) - eps <= 0 else Phi( sqrt(W)*(r(1-eps) - eps)/sqrt(eps(1-eps)(r + sigma2)) )
  ```

- *3. paper:* §4.2 (fec-arq-model.md:699) and A.3: continuous Phi with no cut. DIVERGENT: a 0 -> 1/2 step at r = eps/(1-eps)
- *4. derivation:* §4.2 derives the Phi form; the cut has none (since da837ee9)
- *5. measured in regime?:* whether the engine ever ran on the P_fec > 0 side is unsettled (§3.10 item 2: plan says r ~ 0; fec_rate/tests.rs:277 fixture says r ~ 0.34 at eps ~ 4.8 %)
- *6. clamp bind:* the cut; no gauge
- *7. inputs honest?:* eps = E06/E07, sigma2 = E08, W = A* (shed) or encoder window
- *8. NO-MODE-SWITCH:* a threshold on r that selects a branch (not a dial threshold; a step in the law)
- *9. verdict:* **DIVERGENT, DEGENERATE(below r = eps/(1-eps))**: 0 -> 1/2 step at r = eps/(1-eps) (wasm audit W22), bounded by test_p_fec_normal_vs_paper_phi_divergence_is_bounded; under the sliding bound premises the shipped form over-predicts recovery at A* 3-7 (20/20 cells)
- *10. fix class, priority:* derivation open (docs/derivations/pfec-continuous.md): the §4.2 event is the window event; block-exact C6 does not ship; next = an executed sliding oracle; **P2**, rank 19; ships with M04
- *flags:* measured no, gauge no

#### R04

**r_saturation soft cap** (default (saturation_cap on))

- *1. location:* raptorpath-math/src/lib.rs:917 r_saturation; soft_saturate lib.rs:1058 (s = 0.1*r_sat)
- *2. formula (code):* 

  ```text
  r_sat = argmin_{r in 0.01..1.0 step 0.005} (1 - P_fec(r))*1.5*srtt + B*t_sym*(1+r)/(r(1-eps)) + 0.5*(1+r)*W*t_sym
  ```

- *3. paper:* §4.4
- *4. derivation:* cost terms declared; 1.5*srtt, C_DILUTION 0.5, the grid and SAT_SOFTNESS 0.1 have no provenance
- *5. measured in regime?:* not measured (r ~ 0)
- *6. clamp bind:* grid [0.01, 1]; no engine gauge
- *7. inputs honest?:* inherits R03's step through P_fec; srtt = E01
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **UNDERIVED, DIVERGENT**: four constants with no provenance; its cost curve carries the P_fec cut (R03)
- *10. fix class, priority:* paper + gauge; **P2**, rank 20; depends on R03, E01
- *flags:* measured no, gauge no

#### R05

**completion exposure chi and BULK_TAIL_BUDGET** (default at Bulk (chi = 0 on the tunnel))

- *1. location:* raptorpath-math/src/lib.rs:564 completion_exposure, :123 BULK_TAIL_BUDGET = 0.05; engine net/sender_phases.rs:1316 (only under RWM_COMPLETION_EXPOSURE)
- *2. formula (code):* 

  ```text
  chi = PhiBar((T_rem - 1.5*srtt)/max(4*rttvar, srtt/4)) ; t_eff,bulk = p + (0.05 - p)*chi
  ```

- *3. paper:* A.3, §4.6; same shape
- *4. derivation:* 1.5, 4*RTTVAR, SRTT/4 and 0.05 unprovenanced (§11.2: '0.05 an e.g. promoted to a constant')
- *5. measured in regime?:* chi = 0 identically on the tunnel (paper-recon N1; tests/chi_reachability.rs), so the Bulk glide never runs
- *6. clamp bind:* [0,1]
- *7. inputs honest?:* T_rem unavailable on the tunnel; rttvar = 0.125*srtt (E09)
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **UNDERIVED, DEGENERATE(tunnel)**: chi never evaluated on the tunnel (without_the_arm_chi_is_never_evaluated_and_the_gauge_says_so)
- *10. fix class, priority:* owner call (paper-recon Q3: is chi = 0 the tunnel contract?); **P2**, rank 24; depends on E09
- *flags:* measured no, gauge yes

#### R06

**r*_mass burst-tail provisioning (RWM_RSTAR_TAIL)** (default anchor term (Auto))

- *1. location:* raptorpath-math/src/lib.rs:479 r_star_mass, :285 fit_burst_tail; mass stats gilbert_elliott.rs (block 64)
- *2. formula (code):* 

  ```text
  r*_mass = min{ r in [0,2] : (1-f)*T_lo(R) + f*T_hi(R) <= t/p } , S_m(x) = theta^(x^k)
  ```

- *3. paper:* A.3, §4.3; same shape; doc admits 'not a distribution-free bound'
- *4. derivation:* §4.3 (Weibull tail fit); w0 = 64, k in [0.2, 8], 8 scales declared
- *5. measured in regime?:* not measured (r ~ 0); needs >= 30 nonzero blocks
- *6. clamp bind:* ceiling 2.0: no gauge
- *7. inputs honest?:* E08 mass stats
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **OK**: derived and bit-pinned (rstar_bitpin_solver_is_bit_identical); never active on a measured cell
- *10. fix class, priority:* gauge (ceiling bind) with the R01 measurement; **P3**, rank 49; depends on E08
- *flags:* measured no, gauge no

#### R07

**W* derived window and the keep-behind clip** (default)

- *1. location:* raptorpath-math/src/lib.rs:1000 derive_window; control/fec_rate.rs:531-553 (budget 0.0); net/sender_phases.rs:1375 keep_behind = W*.clamp(16, win_cap=200)
- *2. formula (code):* 

  ```text
  W* = clamp( clamp(W_over, min(W_bur, W_lat), W_lat), 16, 512 ), W_lat = srtt*rate (budget 0 -> 1 RTT) ; keep_behind = clamp(W*, 16, 200)
  ```

- *3. paper:* A.3, §4.8: budgets b(delta) = 1/2, 1, 2 RTT. DIVERGENT: engine runs 1 RTT at every delta (paper-recon N3); the table's 512/429/214 are clipped to 200 on the seat (N4)
- *4. derivation:* W_over/W_bur derived (§4.8); alpha 0.25, [16, 512] and MAX_WINDOW_SIZE 200 declared (§11.2: 200 is below W_mp at the heterogeneous cell)
- *5. measured in regime?:* not measured; no gauge of which term or the 200 clip binds
- *6. clamp bind:* [16,512] then [16,200]: no gauge
- *7. inputs honest?:* eps = E07 upper (0.95), srtt = E01, rate = est tput
- *8. NO-MODE-SWITCH:* none (b(delta)*SRTT would be continuous; paper-recon Q1)
- *9. verdict:* **DIVERGENT, UNDERIVED**: bounded by derived_window_runs_one_rtt_budget_at_every_delta and keep_behind_is_w_star_clipped_to_the_seat_cap; 200 unprovenanced
- *10. fix class, priority:* owner call (paper-recon Q1/Q2), then gauge; **P2**, rank 18; depends on E07, E01
- *flags:* measured no, gauge no

#### R08

**rate cadence min(5 ms, SRTT/4)** (default)

- *1. location:* control/fec_rate.rs:181 RATE_CADENCE_MAX_US, :234 period_us
- *2. formula (code):* 

  ```text
  re-evaluate r when now - t_last >= min(5 ms, srtt/4)
  ```

- *3. paper:* §11.2 row
- *4. derivation:* 5 ms a resource bound (CPU), SRTT/4 declared
- *5. measured in regime?:* staleness cost not measured; gauge rce= exists
- *6. clamp bind:* 5 ms
- *7. inputs honest?:* srtt = E01
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **OK**: rate_cadence_is_one_period_stale_and_bounded; r ~ 0 so staleness is moot on L1
- *10. fix class, priority:* none; **P3**, rank 64
- *flags:* measured partial, gauge yes

#### R09

**generation repair floor 0.15 / 0.20 (opt-in seat)** (opt-in)

- *1. location:* net/sender_policy.rs:349 gen_repair_floor; net/emit_source.rs:686
- *2. formula (code):* 

  ```text
  r_gen = RWM_GEN_R or (0.15 systematic | 0.20 coded), clamp [0,2]
  ```

- *3. paper:* §11.2, §11.3 'r* to the generation encoder'
- *4. derivation:* none (a constant prior where r(beta) is derived)
- *5. measured in regime?:* not measured
- *6. clamp bind:* [0,2]
- *7. inputs honest?:* none
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **UNDERIVED, DIVERGENT**: generation_repair_floor_is_the_constant_prior bounds it; seat has no disposition
- *10. fix class, priority:* decide the seat (§3.2); **P3**, rank 60
- *flags:* measured no, gauge no


### Area: span

#### S01

**span deadline D** (default (feeds A*); shed at rho < 1)

- *1. location:* net/shed.rs:31 shed_deadline_us; net/emit_source.rs:567-583 (rtprop := est.rtt())
- *2. formula (code):* 

  ```text
  D = min(b*rtt, 2*rtt), rtt = est.rtt() of the worst-eps path
  ```

- *3. paper:* A.4 line 2, §5.3: min(b*RTprop, 2*RTprop). DIVERGENT input (§11.3 row 1)
- *4. derivation:* §5.3 (declared via b)
- *5. measured in regime?:* not measured; refreshed only on a taper-cache miss (S07)
- *6. clamp bind:* b capped at 2
- *7. inputs honest?:* DISHONEST: an EWMA of E01 named rtprop (§44 item 3)
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **DIVERGENT, DISHONEST-INPUT**: bounded by span_deadline_reads_the_ewma_rtt_not_rtprop; reads the dwell-inclusive EWMA, not RTprop
- *10. fix class, priority:* input (read RTprop from a wire clock); **P2**, rank 27; depends on E01, E03
- *flags:* measured no, gauge yes

#### S02

**span width A*** (default (trailing repair span))

- *1. location:* net/emit_source.rs:573
- *2. formula (code):* 

  ```text
  A* = clamp(ceil(rate_sym*D), 1, window_size)
  ```

- *3. paper:* A.4 line 3, §5.3; same shape
- *4. derivation:* §5.3
- *5. measured in regime?:* not measured; acts only through repairs (r ~ 0 on L1)
- *6. clamp bind:* [1, W]: [SPAN] a_star= gauge, no bind fraction
- *7. inputs honest?:* D from S01, rate from E12
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **DISHONEST-INPUT**: inherits S01's dwell-inclusive D
- *10. fix class, priority:* follows S01; **P3**, rank 46; depends on S01, E12
- *flags:* measured no, gauge partial

#### S03

**trailing offset Delta** (default (trailing repair span))

- *1. location:* net/emit_source.rs:575-577; jitter control/estimator.rs:550 (RFC 3550, receiver arrivals)
- *2. formula (code):* 

  ```text
  Delta = clamp(ceil(rate_sym*jitter), 1, 64)
  ```

- *3. paper:* A.4, §5.3, §11.2 'Delta cap 64'
- *4. derivation:* floor 1 structural; 64 none
- *5. measured in regime?:* not measured
- *6. clamp bind:* 64: no gauge
- *7. inputs honest?:* jitter of the worst-eps path (cited RFC 3550)
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **UNDERIVED**: the 64 has no provenance and no bind gauge (§11.2)
- *10. fix class, priority:* gauge; **P3**, rank 47
- *flags:* measured no, gauge no


### Area: shed

#### S04

**shed budget** (rho < 1 (Realtime default))

- *1. location:* net/emit_source.rs:587 shed_budget_frac = residual_loss_after_fec (control/fec_rate.rs:784); shed_allowed net/shed.rs:39
- *2. formula (code):* 

  ```text
  shed iff age > D and shed_total + 1 <= eps*(1 - P_fec(r, eps, A*, sigma2))*src_total
  ```

- *3. paper:* A.4, §5.6; same shape but inherits R03's cut
- *4. derivation:* §5.6
- *5. measured in regime?:* crown spot only (Realtime, §2 row)
- *6. clamp bind:* [0,1]
- *7. inputs honest?:* eps = E06, D = S01 (dwell), A* = S02; cached with the taper cache (S07)
- *8. NO-MODE-SWITCH:* armed only at rho < 1 (shed_armed); itself continuous
- *9. verdict:* **DIVERGENT, DISHONEST-INPUT**: budget halves where r* crosses eps/(1-eps) (wasm audit step table, W22)
- *10. fix class, priority:* follows R03 and S01; **P2**, rank 28; depends on R03, S01
- *flags:* measured partial, gauge yes

#### S05

**receiver shed hold** (rho < 1 receiver)

- *1. location:* net/shed.rs:58 shed_recv_hold; budget shed.rs:389; constants net/mod.rs:215-216
- *2. formula (code):* 

  ```text
  H = srtt/2 if shed_on and given_up < eps_recv*frontier else clamp(4*srtt, 60, 300 ms) ; srtt = max over live of the sender-published srtt
  ```

- *3. paper:* §5.6, §11.3 row 2: model b(delta)*SRTT, engine hard-codes 1/2; §11.2 receiver hold row
- *4. derivation:* 60, 300 and 4 unprovenanced; 1/2 equals b(Realtime) by seat, not by evaluation
- *5. measured in regime?:* [SHEDH] gauge exists (floor/cap fractions); bind not reported in a status section
- *6. clamp bind:* [60, 300] ms: [SHEDH] floor_frac / cap_frac
- *7. inputs honest?:* srtt from E10's published view (seed-prone on one-way flows)
- *8. NO-MODE-SWITCH:* the budget_ok test switches between two formulas (a loss-budget threshold, not delta/rho); 1/2 is a Realtime constant rather than b(delta)
- *9. verdict:* **DIVERGENT, UNDERIVED**: bounded by shed_recv_hold_delta_dial_and_legacy_fallback; three unprovenanced constants
- *10. fix class, priority:* law (b(delta)*SRTT) + read the [SHEDH] bind; **P2**, rank 29; depends on E10
- *flags:* measured no, gauge yes


### Area: span

#### S06

**taper copy decision (P_lost)** (default (correction slots))

- *1. location:* net/emit_source.rs:703-757; p_lost raptorpath-math/src/lib.rs:64; inputs net/channel_set.rs:65-73
- *2. formula (code):* 

  ```text
  copy oldest unacked with probability P = eps/(eps + (1-eps)*Q((age - srtt)/max(0.1*srtt, 1 ms)))
  ```

- *3. paper:* A.2 P_lost, §3.2-3.3; §11.3 P_lost row
- *4. derivation:* §3.2; rttvar stand-in E09
- *5. measured in regime?:* measured waste is zero; why it never fires is undecided (§11.2 taper row)
- *6. clamp bind:* [0,1]
- *7. inputs honest?:* srtt = E01 of the worst-eps path (dwell-inclusive), rttvar = 0.1*srtt
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **DISHONEST-INPUT, DEGENERATE(L1: never fires)**: taper_copy reads 0 on the measured cells; correction slots exist only when r > 0
- *10. fix class, priority:* diagnose (why zero) after E01/E09; **P2**, rank 30; depends on E01, E09
- *flags:* measured partial, gauge yes

#### S07

**taper-cache staleness 50 ms** (default, rho = 1)

- *1. location:* net/emit_source.rs:51 TAPER_CACHE_MAX_AGE_US; net/emit_burst.rs:38 taper_recompute_due
- *2. formula (code):* 

  ```text
  recompute iff !batching or !cached or syms >= burst bound or age > 50 ms
  ```

- *3. paper:* §11.2 row
- *4. derivation:* none
- *5. measured in regime?:* shipped inside the measured EB0 arm (§8); no refresh-cause tally
- *6. clamp bind:* 50 ms: no gauge
- *7. inputs honest?:* n/a. Note: D and the shed budget are cached with it, and the NACK path reads the cached values
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **UNDERIVED**: no derivation, no gauge (§11.2)
- *10. fix class, priority:* gauge; **P3**, rank 48
- *flags:* measured partial, gauge no


### Area: emission

#### S08

**Realtime duplicate source send** (Realtime only)

- *1. location:* net/emit_source.rs:461 if pol.protocol_hint == Realtime; scheduler/mod.rs:552 redundant_source_path
- *2. formula (code):* 

  ```text
  at Realtime: send every source symbol twice (second on the cheapest other active path)
  ```

- *3. paper:* §11.1 q13; §3.3
- *4. derivation:* none (a redundancy decision priced by nothing)
- *5. measured in regime?:* crown spot only
- *6. clamp bind:* n/a
- *7. inputs honest?:* n/a
- *8. NO-MODE-SWITCH:* MODE-SWITCH(live): hint equality
- *9. verdict:* **MODE-SWITCH(live), UNDERIVED**: §3.3 row 1; no test, no gauge
- *10. fix class, priority:* law (price the duplicate on the rate law) or delete; **P2**, rank 31
- *flags:* measured no, gauge no

#### S09

**symbol packing and emission-batching scope (use_packing, emit_batch_on)** (Realtime and rho < 1)

- *1. location:* net/sender_policy.rs:678 use_packing = hint == Realtime; :737 emit_batch_on = emit_batch && reliable && !coded_wire && !use_packing
- *2. formula (code):* 

  ```text
  packing on iff Realtime ; batching on iff reliable and not packing
  ```

- *3. paper:* §3.3 rows 2 and 5; §11.2 use_packing row
- *4. derivation:* none (declared corner)
- *5. measured in regime?:* not measured at Realtime
- *6. clamp bind:* n/a
- *7. inputs honest?:* n/a
- *8. NO-MODE-SWITCH:* MODE-SWITCH(live): hint equality and the reliable bool
- *9. verdict:* **MODE-SWITCH(live), UNDERIVED**: §3.3 rows 2 and 5; the batching echo prints 'out of scope' at rho < 1 and Realtime
- *10. fix class, priority:* law (a continuous packing/batching quantum) or owner call; **P2**, rank 32
- *flags:* measured no, gauge yes


### Area: span

#### S10

**proactive repair emission (taper density and TaperBudget accrual)** (default (RWM_TAPER_R on): how r reaches the wire)

- *1. location:* control/fec_rate.rs:600-640 TaperFunction (q = p_bg clamped [0.01,1], default 0.5), :678-740 TaperBudget::accrue; called from net/emit_source.rs (repair_rate block, ~:680)
- *2. formula (code):* 

  ```text
  owed <- min(owed + r, max(r*W, 1)) ; grant = min(owed, max(r*shape(t mod W), r), spare, 1) , shape(t) = W*q*(1-q)^t/(1-(1-q)^W) ; owed -= grant
  ```

- *3. paper:* A.2 prints the raw density tau(t) = r*q*(1-q)^t (TaperFunction), the superseded per-ack-cycle emitter; §3.3 'Taper budget' says only 'banks owed += r per source symbol'. The grant expression is in the code doc comment, not the paper
- *4. derivation:* doc comment argues conservation (sum of grants <= sum of rates up to the one-window expiry); q default 0.5 and the [0.01,1] clamp unprovenanced
- *5. measured in regime?:* r ~ 0 on every measured cell, so grant ~ 0
- *6. clamp bind:* spare and the 1.0-per-source cap: no gauge
- *7. inputs honest?:* r from R01, q from E08, spare from K04 (inflated cwnd)
- *8. NO-MODE-SWITCH:* RWM_TAPER_R experiment gate
- *9. verdict:* **DIVERGENT, DEGENERATE(r = 0)**: A.2's formula is the emitter §3.3 says was fixed; the shipped grant law appears only in code
- *10. fix class, priority:* paper (write the grant law on its own line; retire A.2's raw-density line); **P3**, rank 35; depends on R01
- *flags:* measured no, gauge no


### Area: recovery

#### C01

**per-path time threshold 9/8 * max(srtt, ewma)** (default, N >= 2)

- *1. location:* net/recovery_laws.rs:118 mp_time_threshold_split; net/sender_phases.rs:683-748 (mp_clocks = (p.srtt(), est.rtt()))
- *2. formula (code):* 

  ```text
  ripe iff now - t_flight >= max(9/8*max(srtt_i, ewma_i), 10 ms)
  ```

- *3. paper:* A.6 line 1, §7.1; same shape
- *4. derivation:* cited (RFC 9002 §6.1.2; 9/8 is empirical); floor 10 ms (10x kGranularity, §11.2)
- *5. measured in regime?:* c8 measured in regime only from §52 (lever B); p0 threshold ~390 ms (CAND) vs ~215 ms (BASE) on a 10 ms leg (p0-dwell-diag §4); it also fires on stale reports (tt_st 999 BASE, 2 481 CAND, §53)
- *6. clamp bind:* 10 ms floor: pf= gauge on [DIAG]
- *7. inputs honest?:* DISHONEST: srtt and ewma are both E01 copies (dwell-inclusive) and age runs from the original send, so the RFC constant is applied to a coordinate that includes sender dwell (§11.2 thresholds row)
- *8. NO-MODE-SWITCH:* path-count: applies at N >= 2 only; N = 1 runs C04 (a law step at N = 1 -> 2, column-8 note)
- *9. verdict:* **DISHONEST-INPUT**: threshold ~215-390 ms on a 10 ms leg at c8 (p0-dwell-diag §4); RFC 9002 assumes a wire-timed age
- *10. fix class, priority:* input (wire clock; age from the wire send); **P1**, rank 12; depends on E01, E11
- *flags:* measured yes, gauge yes

#### C02

**packet threshold 3 and the 20-gap report horizon** (default, N >= 2)

- *1. location:* net/recovery_laws.rs:142 MP_PACKET_THRESHOLD = 3, :166 mp_fast_lost; net/mod.rs:223 MAX_NACK_GAPS = 20; net/sack.rs:52-91 sack_to_gaps truncates to the 20 oldest runs
- *2. formula (code):* 

  ```text
  lost(s) iff #{delivered seqs on s's original path above s} >= 3 ; a report names only the 20 oldest gap runs
  ```

- *3. paper:* A.6 line 2, §7.1; MAX_NACK_GAPS is not in the paper
- *4. derivation:* 3 cited (RFC 9002 kPacketThreshold); 20 none (a report-size choice)
- *5. measured in regime?:* NOT measured in regime before §52 (§3.10 item 5): under the 20-gap horizon p0 holes reach the law at ~3.5 s age; the channel fires 0-755 times per run on main vs 3.7-6.6 k under lever B. Mean in-order delay at c8 ~5.3 s on main vs ~1.0 s under lever B (§53 Erratum 1); tail 3.95 s vs 0.36 s (§53)
- *6. clamp bind:* the 20-run horizon is a clamp on which holes the law can see: no gauge on main (the snapver branch's horizon test measures it)
- *7. inputs honest?:* fed gap reports; stale reports from the slow return leg feed it false evidence (C03)
- *8. NO-MODE-SWITCH:* N >= 2 only (as C01)
- *9. verdict:* **UNDERIVED, DEGENERATE(c8: the law sees only old holes)**: c8 on main: holes judged at ~3.5 s, tail 3.95 s, in-order delay ~5.3 s; lever B (64-gap) cuts the tail to 0.36 s (§53)
- *10. fix class, priority:* law change needing a battery (lever B horizon), shipped with C03; **P0**, rank 4; ships with C03
- *flags:* measured yes, gauge partial

#### C03

**snapshot-order filter (branch github/snapver-law, not on main)** (candidate (branch), default path once merged)

- *1. location:* github/snapver-law 8191b3de: net/snapver.rs snapver_judges, snapver_coalesce_keeps, SnapOrder; seam serve_gaps -> take_gap_batch / judged_gaps. Main: last-arrival-wins coalescing net/sender_phases.rs:658-666
- *2. formula (code):* 

  ```text
  judge(R) iff V(R) >= V*, V = (next_expected, received_above) lexicographic, V* = max V over every dequeued batch this run; tail sweep always judged
  ```

- *3. paper:* §7.1 'Snapshot order' and A.6 on the branch (formula-first, symbol table, safety and liveness argued)
- *4. derivation:* complete on the branch: every symbol is a wire field or the gap channel's own fold; no constant
- *5. measured in regime?:* measured at c8 and c1d in §53 as an env arm: stale/rep 0.984-0.997 on the slow leg; under lever B duplicates 4 004 -> 1 291, retransmits/GB 30 661 -> 25 157; c1d 7-16 % stale, lever B's +13 % duplicates not removed. rho < 1 untested
- *6. clamp bind:* none
- *7. inputs honest?:* honest (wire fields)
- *8. NO-MODE-SWITCH:* none: reads no dial, no gate
- *9. verdict:* **OK**: §53 CONFIRMED AND EFFECTIVE at c8; derivation docs/derivations/dup-diag.md on the branch
- *10. fix class, priority:* merge (with C02's horizon, §41c guard first); **P0**, rank 5; ships with C02
- *flags:* measured yes, gauge yes

#### C04

**legacy age gate srtt/2 (the shipped N = 1 recovery law)** (default, every single-path cell (c1s, c2, c3))

- *1. location:* net/recovery_laws.rs:222 legacy_age_ripe; net/sender_phases.rs:958-964 (the else branch when !(recov_mp_law && N > 1) and !(recov_sp && N <= 1); RWM_RECOV_SP default off)
- *2. formula (code):* 

  ```text
  ripe iff now - t_send_original >= srtt_pool/2
  ```

- *3. paper:* §11.2 says it 'runs only when per-path detection is off' and status §3.4 says 'inert at c1'. DIVERGENT: with RWM_RECOV_MP_LAW on, it is the law at every N = 1 cell, since the per-path branch needs N > 1
- *4. derivation:* none (pre-RFC; §3.4)
- *5. measured in regime?:* not measured as the N = 1 law. §3.4 calls it inert at c1, which is plausible (a single FIFO path reports a gap about one RTT after the send, so age > srtt/2 and it rarely suppresses) but unread: the mpd_supp_age gauge is not reported for a single-path cell
- *6. clamp bind:* none (10 ms only via the pool fallback)
- *7. inputs honest?:* DISHONEST: srtt_pool = E11 (dwell-inclusive) against an age from the original send
- *8. NO-MODE-SWITCH:* path-count switch: N = 1 runs this, N >= 2 runs C01/C02 (column-8 note; a step at N = 1 -> 2, like the guard ADR-0072 removed from the store)
- *9. verdict:* **DIVERGENT, UNDERIVED, DISHONEST-INPUT**: code: sender_phases.rs:898 needs mp_n_paths > 1 and :937 needs RWM_RECOV_SP (off), so :958 is the N = 1 law; paper §11.2 says it runs only with per-path detection off. Likely inert in effect, never gauged
- *10. fix class, priority:* paper correction now; law (C01 at N = 1, RWM_RECOV_SP exists as the off arm) with a battery; **P2**, rank 16; depends on E11
- *flags:* measured no, gauge yes

#### C05

**per-sequence retransmit cooldown** (default, every reliable cell (largest at N >= 2))

- *1. location:* net/recovery_laws.rs:273 retx_cooldown_us, :279 cooldown_elapsed; net/sender_phases.rs:716, :882
- *2. formula (code):* 

  ```text
  resend seq only if now - last_retx >= max(srtt_pool, 10 ms)
  ```

- *3. paper:* §7.1 ('once per SRTT'); §11.2 NACK_RETX_COOLDOWN_FLOOR_US row
- *4. derivation:* 'once per SRTT' declared; the 10 ms floor '10x kGranularity by choice', moves W by 6.8x across its range (§3.4)
- *5. measured in regime?:* [QCLK] srtt 257-542 ms at c8 (p0-dwell-diag §4): a retransmit on the 10 ms leg is held ~0.5 s
- *6. clamp bind:* 10 ms floor: mpd_supp_cool counts suppressions, no bind fraction
- *7. inputs honest?:* DISHONEST: E11, the slowest leg's dwell-inclusive clock, applied to every seq
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **DISHONEST-INPUT, UNDERIVED**: cooldown = 257-542 ms at c8 for seqs whose leg RTT is 10 ms (p0-dwell-diag §4)
- *10. fix class, priority:* input (per-path wire clock of the retransmit's leg); **P1**, rank 11; depends on E11
- *flags:* measured yes, gauge partial

#### C06

**tail-sweep timeout** (default, every reliable cell)

- *1. location:* net/recovery_laws.rs:287 tail_sweep_timeout_us; net/mod.rs:2708-2734, fire :2951-2963; TAIL_SWEEP_MIN/MAX net/mod.rs:248-249
- *2. formula (code):* 

  ```text
  T = clamp(2*srtt_pool, 25 ms, 100 ms)
  ```

- *3. paper:* A.6 refresh row covers the clamp; §7.1; §11.2 refresh clamp row
- *4. derivation:* none ('undefeated, not derived'; §3.4)
- *5. measured in regime?:* c8: w_us p05 = p95 = 100 000 us on 30/30 rows (§3.10 item 4; p0-dwell-diag §4); earlier 92.4-99.7 % bind was measured on the saturation-filtered set and not re-measured (§3.5 item 5)
- *6. clamp bind:* 100 ms upper: [QCLK] site=sender w_us distribution (no explicit bind counter)
- *7. inputs honest?:* E11 (dwell-inclusive); the clamp hides it
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **PINNED(c8), UNDERIVED, DISHONEST-INPUT**: 100 ms at p05 and p95 on 30/30 c8 rows; 3-8 fires per run (§3.10 item 4)
- *10. fix class, priority:* law (derive from self-heal lateness F, §11.2) after E11; **P2**, rank 17; depends on E11
- *flags:* measured yes, gauge partial

#### C07

**receiver hole refresh (hole_nack_refresh)** (default, every reliable one-way transfer)

- *1. location:* net/recovery_laws.rs:296 hole_nack_refresh; caller net/receiver.rs:741 (reliable only); HOLE_NACK_REFRESH_MIN/MAX net/mod.rs:239-240
- *2. formula (code):* 

  ```text
  refresh = clamp(2*srtt_recv, 25 ms, 100 ms), 100 ms if srtt_recv is None
  ```

- *3. paper:* A.6 line 4, §7.1; same shape
- *4. derivation:* none (§3.4 tail-sweep / refresh clamp row)
- *5. measured in regime?:* c8: srtt_recv = the 50 ms seed on 12/12 rows, so refresh = 100 ms constant (§3.10 item 6)
- *6. clamp bind:* [25,100] ms: [QCLK] site=receiver
- *7. inputs honest?:* DEGENERATE: E10 (seed); the seed gives exactly the None fallback
- *8. NO-MODE-SWITCH:* reliable only; rho < 1 uses S05
- *9. verdict:* **DEGENERATE(all one-way cells), UNDERIVED**: a constant 100 ms on every c8 row because its input is the seed (§3.10 item 6). Feeding it the data sender's current app-echo srtt (257-542 ms at c8) would still clamp to 100 ms (DEGENERATE -> PINNED, same output); the output moves only where an honest srtt < 50 ms (c1s, c2 propagation), so it needs E01 first
- *10. fix class, priority:* input (E10 after E01), then derive the cadence; **P1**, rank 10; depends on E10, E01; ships with E10
- *flags:* measured yes, gauge yes

#### C08

**gap-report minimum interval GAP_ACK_MIN_INTERVAL = 2 ms** (default)

- *1. location:* net/mod.rs:234; net/receiver.rs:1929-1932
- *2. formula (code):* 

  ```text
  advertise gaps iff new data above the frontier and >= 2 ms since the last gap report
  ```

- *3. paper:* §7.1 (fec-arq-model.md:2803), §11.2 row
- *4. derivation:* none (a rate limit that is also the hole sampler)
- *5. measured in regime?:* manufactures no holes on a lossless wire (holeclass_reachability.rs)
- *6. clamp bind:* 2 ms: [LATE] sampler_bind
- *7. inputs honest?:* n/a
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **UNDERIVED**: §11.2: 'none yet; it sets the alpha ~ 1 corner'
- *10. fix class, priority:* paper; **P3**, rank 50
- *flags:* measured partial, gauge yes

#### C09

**NACK repair throttle (NackCongestionState x MAX_NACK_REPAIRS_PER_NACK, per-period nack cap)** (default, every reliable cell)

- *1. location:* net/mod.rs:255-344 NackCongestionState, :225 MAX_NACK_REPAIRS_PER_NACK = 10, :350 IDLE_RECOVERY_FLOOR 0.1, :3190-3257 refresh every 5 ms; BudgetAllocator control/fec_rate.rs; enforced net/sender_phases.rs:672, :814
- *2. formula (code):* 

  ```text
  max retransmits per gap report = round(10*m) ; m halves when loss rose >10 %+0.001 AND rtt rose >1 ms for 2 periods, +0.1 when both flat, floor 0.1 when idle ; per-period cap = nack_cap(p_upper at 1 - t_tail)*source_symbols, at least 10
  ```

- *3. paper:* absent: no symbol and no prose (grepped NackCongestion, MAX_NACK_REPAIRS, BudgetAllocator, nack_cap, 'repair budget', 'per NACK', 'per gap report' in fec-arq-model.md); the snapver-law branch's §7.1 text mentions 'a spent repair budget' only as a liveness case
- *4. derivation:* none (thresholds 1.1, 0.001, 1 ms, 2, 0.5, 0.1, 10)
- *5. measured in regime?:* not measured; no gauge of how often the per-report cap or the multiplier binds. The snapver liveness note names 'a spent repair budget drops it' as a live case on main
- *6. clamp bind:* 10 per report; m in [0,1]: no gauge found
- *7. inputs honest?:* rising-RTT input is E01-class (dwell-inclusive), so a growing sender queue reads as congestion
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **UNDERIVED**: a default-path AIMD heuristic with seven constants and no paper entry. Inference, not measured: at c8 ~1 400 old holes (p0-dwell-diag §2.2) compete for <= 10 retransmits per report
- *10. fix class, priority:* gauge first, then paper or delete; **P1**, rank 13; depends on E01
- *flags:* measured no, gauge no

#### C10

**reactive repair margin ceil(retx*eps)** (default)

- *1. location:* net/sender_phases.rs:1108-1164
- *2. formula (code):* 

  ```text
  margin = ceil(retransmitted * max_i eps_i) coded repairs per gap report
  ```

- *3. paper:* §11.2 row ('a step, not a law'), §11.3 feedback-leg row
- *4. derivation:* none; the derived replacement is the DoF ledger (§4.13; §39 CONTINUOUS-BUT-CANNOT-HELP-MEASURABLY)
- *5. measured in regime?:* bounded by the r > 0 battery's [RFA] counters
- *6. clamp bind:* budget/window
- *7. inputs honest?:* E06 (max over channel paths)
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **UNDERIVED**: bounded by rate_law_feedback_leg_is_the_open_loop_budget
- *10. fix class, priority:* paper (record as declared) or ledger arm; **P3**, rank 51
- *flags:* measured partial, gauge partial

#### C11

**repair debt reduction (debt -= nacked*r)** (default)

- *1. location:* net/sender_phases.rs:1166-1176, :1331; accrual net/emit_source.rs:680-694
- *2. formula (code):* 

  ```text
  debt += max(r, floors) per source ; debt -= nacked*r per report ; debt -= newly_acked*r per ack
  ```

- *3. paper:* §11.2 row
- *4. derivation:* none ('a proxy'); subsumed by the ledger's a_c (§4.13)
- *5. measured in regime?:* r ~ 0 on L1, so debt ~ 0
- *6. clamp bind:* floor 0
- *7. inputs honest?:* nacked counts suppressed seqs too
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **UNDERIVED, DEGENERATE(r = 0)**: inert while r = 0; bounded by rate_law_feedback_leg_is_the_open_loop_budget
- *10. fix class, priority:* with the DoF ledger; **P3**, rank 52; depends on R01
- *flags:* measured no, gauge yes

#### C12

**receiver request law (l*, request_bar, request_m; off)** (opt-in (RWM_RECV_REQUEST_LAW, RWM_RANK_FEEDBACK off))

- *1. location:* net/late.rs:99 request_bar, :135 request_m, :241 pi0, :317-358 l*; net/succ.rs
- *2. formula (code):* 

  ```text
  request iff rho_heal(l) <= c/(1+c), c = delta/0.5 ; m = clamp(ceil(ln2/(-ln pi0)), 1, A*)
  ```

- *3. paper:* A.6 last lines, §7.6
- *4. derivation:* §7.6
- *5. measured in regime?:* refuted on every arm at both duals (receiver-law battery, Track C); pi0 estimator fixed since (§3.1 item 1), not re-run
- *6. clamp bind:* l* capped by (H - d)+
- *7. inputs honest?:* pi0 from sender-stamp order
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **OK**: REFUTED-WITH-RECORD on the pre-fix estimator; not re-run
- *10. fix class, priority:* none (off arm; re-run or retire); **P3**, rank 53
- *flags:* measured partial, gauge yes


### Area: model

#### C13

**T_cut give-up horizon (wasm model)** (model only (UI at rho < 1))

- *1. location:* raptorpath-math/src/lib.rs:1213 find_t_cut; raptorpath-wasm/src/lib.rs:1397-1420 (cut = max(t_cut, 2*RTT))
- *2. formula (code):* 

  ```text
  T_cut = min{t in [0, 10W] : 1 - eps(1-P_fec)(1-P_arq(t)) >= rho} ; cut = max(T_cut, 2*RTT)
  ```

- *3. paper:* A.3 'rho < 1' line, §4.10
- *4. derivation:* §4.10; the 2*RTT floor and the 10W search cap have none
- *5. measured in regime?:* at all ten measured rho < 1 points it is the floor or the cap (§3.10 item 1)
- *6. clamp bind:* 2*RTT floor and 10W cap
- *7. inputs honest?:* uses the TRUE q (agg_q), not an estimate
- *8. NO-MODE-SWITCH:* steps at non-preset rho* 0.983332 and 0.988175 (wasm audit F4)
- *9. verdict:* **PINNED(every measured rho < 1 point)**: two-valued over its range (§3.10 item 1); no engine caller
- *10. fix class, priority:* law (wasm audit F3+F4 proposal); **P3**, rank 36; depends on R03; ships with M04
- *flags:* measured yes, gauge yes


### Area: congestion

#### K01

**Copa app-echo cwnd law (queue target, P10 floor, anchor floor and pull)** (default, but the cwnd gates nothing (in-flight brake unarmed))

- *1. location:* scheduler/copa.rs:314 queue_target_mult, :1157 queue_floor (P10), :1227-1250 above-target, :1316-1338 update; anchor floor :1483 (0.85*bdp), clamp scheduler/path.rs:1092
- *2. formula (code):* 

  ```text
  above iff (winmin - P10floor) > (mult(hint) - 1)*floor + 2*jitter ; c <- 0.92c | 1.5c+1 | c + max(0.25(bdp - c), 2) ; c >= 0.85*bdp
  ```

- *3. paper:* A.7, §8.2; §11.2 rows (queue_target_mult, anchor 0.85/0.25/1.0)
- *4. derivation:* mult a declared corner; 0.85 set by one measurement; 0.25, 1.0 untested
- *5. measured in regime?:* c8: cwnd 10-20 k vs in-flight 0.1-1 k (p0-dwell-diag §4): inflated by the anchor floor from E03/E04
- *6. clamp bind:* [8, 10000], floor 0.85*bdp: no gauge
- *7. inputs honest?:* app-echo (E01-class) and bdp from E03 x E04
- *8. NO-MODE-SWITCH:* MODE-SWITCH(live): queue_target_mult 1.08/1.125/1.25 by hint (§3.3)
- *9. verdict:* **MODE-SWITCH(live), UNDERIVED, DISHONEST-INPUT**: cwnd reaches only placement's queue term and the active_paths filter; the hint table is a §3.3 debt
- *10. fix class, priority:* delete or reduce (§44 item 4: a controller that supplies one denominator); **P3**, rank 41; depends on E03, E04
- *flags:* measured partial, gauge partial

#### K02

**Copa delta off the wire signal (constant 0.5)** (default (inert, see K01))

- *1. location:* scheduler/copa.rs:611 delta = wire_mode ? copa_delta_for_hint(hint) : 0.5
- *2. formula (code):* 

  ```text
  delta_copa = 0.5 at every hint unless RWM_COPA_WIRE
  ```

- *3. paper:* A.7 / §8.2: delta(hint); §11.3 Copa price row
- *4. derivation:* n/a
- *5. measured in regime?:* inert by default
- *6. clamp bind:* n/a
- *7. inputs honest?:* n/a
- *8. NO-MODE-SWITCH:* experiment gate selects the delta source
- *9. verdict:* **DIVERGENT**: bounded by copa_delta_is_half_at_every_hint_off_the_wire_signal
- *10. fix class, priority:* follows K01; **P3**, rank 42; depends on K01
- *flags:* measured no, gauge yes

#### K03

**Copa wire-mode law and TCP-compete mode (opt-in)** (opt-in (RWM_COPA_WIRE, RWM_COPA_COMPETE, passthrough))

- *1. location:* scheduler/copa.rs:1191-1203 (dq, above), :1388-1428 (velocity), :1382 coupling cap, :1054-1140 compete
- *2. formula (code):* 

  ```text
  above iff cwnd*delta*dq > srtt ; step = max(v/delta, 1) ; cwnd <= bdp + 2/delta ; compete: 1/delta AIMD on quinn congestion events
  ```

- *3. paper:* A.7, §8.2
- *4. derivation:* cited (Copa, Arun & Balakrishnan 2018)
- *5. measured in regime?:* not on any default path
- *6. clamp bind:* velocity cap delta*10000; coupling cap: no gauges
- *7. inputs honest?:* quinn wire srtt (E05) smoothed again
- *8. NO-MODE-SWITCH:* MODE-SWITCH(live, opt-in): compete mode (§3.3 row 4)
- *9. verdict:* **MODE-SWITCH(live)**: §3.3 row 4; the anchor floor is applied after the coupling cap, so the floor can exceed the cap
- *10. fix class, priority:* delete with K01 or keep as arm; **P3**, rank 43
- *flags:* measured no, gauge partial


### Area: placement

#### K04

**spare capacity and the active_paths saturation filter** (default (repair cap, placement set))

- *1. location:* scheduler/path.rs:467 spare_capacity; scheduler/mod.rs:226 active_paths, :568; read net/emit_source.rs:526 (flat_rate = min(window_rate, spare))
- *2. formula (code):* 

  ```text
  spare_i = (cwnd_i - inflight_i)/inflight_i ; active iff cwnd_i > inflight_i
  ```

- *3. paper:* §5.7, §11.3 path-set rows
- *4. derivation:* declared
- *5. measured in regime?:* np_act = 2 on nearly every pinned c8 tick: the filter almost never excludes a leg (p0-dwell-diag §4)
- *6. clamp bind:* n/a
- *7. inputs honest?:* Copa cwnd inflated by the anchor floor (K01), so 'saturated' rarely fires; in-flight expiry max(4*srtt, 250 ms) on E02
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **DISHONEST-INPUT**: cwnd 10-20 k against in-flight 0.1-1 k at c8 (p0-dwell-diag §4)
- *10. fix class, priority:* follows K01; **P3**, rank 44; depends on K01
- *flags:* measured yes, gauge yes

#### K05

**placement cost (place_costs)** (default)

- *1. location:* scheduler/place.rs:288 place_costs; E_i scheduler/path.rs:441; cold r = 10.0 place.rs:347; ref floor place.rs:38
- *2. formula (code):* 

  ```text
  c_i = E_i/ref + w_bw*r_i + div_i , E_i = (I_i/W_i)*s_i + (0.5 + eps_i)*wire_i , r_i = eps/(1-eps) (10.0 when infinite)
  ```

- *3. paper:* §5.7 matches the code (its '(E_i - min(S, 9/8*srtt_i))+' with S = 0 is E_i). A.4's E_i line is stale: 'srtt_i/2 + eps_i*srtt_i' (the pre-ADR-0073 echo price) where the code prices (0.5+eps_i)*wire_i. DIVERGENT paper text (appendix only)
- *4. derivation:* base term derived (ADR-0073 wire price); 10.0 and the 50 ms cold seed unprovenanced (§11.2)
- *5. measured in regime?:* Track A battery: INERT-AS-DERIVED on latency; §41/§41b wire price cured the idle-leg lock-out
- *6. clamp bind:* 10.0 cold r: [ETA] cold_r = 0 on 31 807 lines (§11.2)
- *7. inputs honest?:* wire_i honest (E05), blind to quinn's send buffer; s_i = E02 times I/W ~ 0.01-0.1
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **DIVERGENT, UNDERIVED**: place_costs_match_the_pinned_wired_table pins the code; A.4's c_i and E_i lines describe superseded forms
- *10. fix class, priority:* paper (A.4 lines), then derive the cold price; **P2**, rank 33
- *flags:* measured yes, gauge yes

#### K06

**placement temperature T = 0.15** (default)

- *1. location:* scheduler/place.rs:18 PLACE_TEMPERATURE, :160 softmax
- *2. formula (code):* 

  ```text
  P_i proportional to exp(-(c_i - min c)/0.15)
  ```

- *3. paper:* A.4, §5.7, §11.2
- *4. derivation:* none: the argmax of a four-point sweep whose verdict failed; derived form (sqrt6/pi)*sigma_e/ref exists (removed arm)
- *5. measured in regime?:* Track A: '0.15 is not a stable quantile'
- *6. clamp bind:* n/a
- *7. inputs honest?:* c_i (K05)
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **UNDERIVED**: §11.2 PLACE_TEMPERATURE row
- *10. fix class, priority:* derive (sigma_e on the [ETA] stream); **P3**, rank 54; depends on K05
- *flags:* measured partial, gauge no

#### K07

**diversity weight w_div = 1.0** (default (repairs only; r ~ 0))

- *1. location:* scheduler/mod.rs:93; place.rs:366-371 derived arm RWM_PLACE_WDIV_DERIVED (off)
- *2. formula (code):* 

  ```text
  div_i = 1.0*fate_i for repairs
  ```

- *3. paper:* A.4, §5.7, §11.2
- *4. derivation:* none; derived form (p_BB - eps)+*srtt/ref gives 0.475 (c2), 0.552 (c3)
- *5. measured in regime?:* repairs are rare on L1
- *6. clamp bind:* n/a
- *7. inputs honest?:* n/a
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **UNDERIVED**: §11.2 w_div row
- *10. fix class, priority:* derive (flip the derived arm after a battery); **P3**, rank 55; depends on K05
- *flags:* measured no, gauge no

#### K08

**repair spare near-tie band 0.8 / floor 0.25 (opt-in)** (opt-in (RWM_XPATH_REPAIR))

- *1. location:* scheduler/place.rs:119 place_repair_spare_path
- *2. formula (code):* 

  ```text
  pick uniformly among s >= min(0.8*max, max - 0.25)
  ```

- *3. paper:* §11.2 row
- *4. derivation:* none ('a missing scale')
- *5. measured in regime?:* not measured
- *6. clamp bind:* n/a
- *7. inputs honest?:* spare (K04)
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **UNDERIVED**: §11.2 near-tie row
- *10. fix class, priority:* derive or delete; **P3**, rank 56; depends on K04
- *flags:* measured no, gauge no


### Area: substrate

#### K09

**quinn BBR (the default substrate CC)** (default, every cell)

- *1. location:* transport/quic.rs:79 quic_cc_mode, :192-196 BbrConfig (RWM_QUIC_CC unset = bbr); quinn-proto 0.11.14 (not in repo)
- *2. formula (code):* 

  ```text
  W = 2*b-hat*m-hat (ProbeBw) ; b-hat = 10-round max of min(send_rate, ack_rate) ; m-hat = lifetime rtt.min() ; quinn paces at 1.25*W/srtt and ignores pacing_rate
  ```

- *3. paper:* §8.4 (substrate is a controller), §47
- *4. derivation:* cited (BBR v1); quinn's sampler differs from BBR's
- *5. measured in regime?:* c1s: window collapse from a latched-low min_rtt (§47); c2: window over-reads ~180-600 KB against ~125 KB and never binds (W1 §2); §48 reproduces the collapse on a 10 us grid (share 0.81)
- *6. clamp bind:* none relevant
- *7. inputs honest?:* DISHONEST: m-hat never expires (lifetime min), b-hat over-reads under ack clusters (bbr_rs.rs module doc)
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **DISHONEST-INPUT**: latched min_rtt collapses the c1s window (§47); c2 window 1.4-4.8x the BDP (W1 §2)
- *10. fix class, priority:* replace (bbr_rs, §48b running); **P1**, rank 6
- *flags:* measured yes, gauge partial

#### K10

**bbr_rs (branch github/bbrrs-r3, not default)** (candidate (RWM_QUIC_CC=bbr_rs))

- *1. location:* github/bbrrs-r3 d1fadd7c transport/bbr_rs.rs: estimator :190-340, max filter 10 rounds :75-160, m-hat :906-915, ProbeRtt :658-720, target cwnd :723, pacing :750-773
- *2. formula (code):* 

  ```text
  W = max(4 mtu, 2*b-hat*m-hat) + max_ack_height (ProbeBw) ; b-hat = 10-round max of delivered/max(send_elapsed, ack_elapsed), interval >= m-hat ; m-hat <- r_e if r_e <= m-hat ; expiry 10 s -> ProbeRtt at 0.75*target
  ```

- *3. paper:* §8.4: law line says 'if r_e < m-hat' while the code uses <= (paper-recon N5, paper stale); §8.4 'reads no delta, rho or hint'
- *4. derivation:* cited (BBR v1); gains 2.885, 1.25, 0.75 cited; RWM_BBR_PROBERTT_ROUNDS = 1 declared
- *5. measured in regime?:* §48 host gate on cc_sim; §48b VM battery running (REHEARSAL-PASS)
- *6. clamp bind:* min_cwnd 4 mtu
- *7. inputs honest?:* honest by construction (send-interval sampler, own ack clock); qrt_us/qbw on [QCC]
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **DIVERGENT**: tied_min_rtt_samples_keep_the_filter_fresh_under_le_and_would_fire_probe_rtt_under_lt; paper §8.4 still prints <
- *10. fix class, priority:* paper (the <= sign) and the §48b verdict; **P2**, rank 34
- *flags:* measured partial, gauge yes


### Area: model

#### M01

**wasm store pool 512 per path** (model only)

- *1. location:* raptorpath-wasm/src/lib.rs:352 STORE_PER_PATH, :1303 stall
- *2. formula (code):* 

  ```text
  stall source iff occupancy >= 512*n_paths
  ```

- *3. paper:* §11.3 store-cap row (wasm clause), understated per paper-recon 4b
- *4. derivation:* none (the engine law is §6.1)
- *5. measured in regime?:* model
- *6. clamp bind:* the whole law is a constant
- *7. inputs honest?:* none
- *8. NO-MODE-SWITCH:* none
- *9. verdict:* **DIVERGENT, UNDERIVED**: bounded by wasm_pool_is_constant_per_path_whatever_the_dial_and_bdp
- *10. fix class, priority:* model (adopt §6.1 after F01 settles; golden re-capture); **P3**, rank 39; depends on F01
- *flags:* measured n/a, gauge yes

#### M02

**wasm tail target interpolation (Bulk anchor 0.05)** (model only (UI))

- *1. location:* raptorpath-wasm/src/lib.rs:247-261 sim_tail_target_of_delta
- *2. formula (code):* 

  ```text
  log t piecewise linear in log delta through (50,1e-7), (0.5,1e-5), (0.005,0.05)
  ```

- *3. paper:* §11.3 visualizer row: engine Bulk t_tail = 1e-3
- *4. derivation:* none for 0.05 (BULK_TAIL_BUDGET)
- *5. measured in regime?:* model
- *6. clamp bind:* flat outside [0.005, 50] (never reached from the UI)
- *7. inputs honest?:* n/a
- *8. NO-MODE-SWITCH:* continuous; slope kink at Auto (wasm audit W1, class A)
- *9. verdict:* **DIVERGENT**: bounded by test_sim_engine_tail_anchor_divergence_is_bounded (log10 50)
- *10. fix class, priority:* model; **P3**, rank 40
- *flags:* measured n/a, gauge n/a

#### M03

**wasm rate beta-guard and the dead classic-hint machine** (model only (W11 live with zero value step; W2/W4/W5/W6/W9/W17 dead))

- *1. location:* raptorpath-wasm/src/lib.rs:883 rate_now if bulkness > 0; :1022-1044 classic arms; :1689 get_completion_exposure
- *2. formula (code):* 

  ```text
  rate = beta > 0 ? mix : controller_rate(rate_inputs) (bulk_late_is_fine = hint_bulk)
  ```

- *3. paper:* §4.5: 'There is no if beta == 0 fast path; that would be the mode switch again'
- *4. derivation:* n/a
- *5. measured in regime?:* wasm audit: classic outputs equal the continuum at the presets on all six golden cells
- *6. clamp bind:* n/a
- *7. inputs honest?:* ρ dropped for auto/realtime on the classic path (W4)
- *8. NO-MODE-SWITCH:* MODE-SWITCH(live: W11 fast path, zero step; dead: classic machine; W17 steps at Auto in an unread accessor)
- *9. verdict:* **MODE-SWITCH(live), MODE-SWITCH(dead)**: §3.10 item 3; wasm audit site table
- *10. fix class, priority:* delete (wasm audit F1/F2); **P3**, rank 38
- *flags:* measured n/a, gauge n/a

#### M04

**wasm give-up law (cartoon + sim; was V5/V6 and W14)** (model only (UI))

- *1. location:* raptorpath-wasm/src/lib.rs giveup_budget (JS export) and the sim give-up loop; interactive-visualizer.html cartoon law (merge 56388aee)
- *2. formula (code):* 

  ```text
  give up iff age > D(delta) and given_up + 1 <= min(1 - rho, R)*src_total; R = eps*(1 - P_fec(r, eps, W_site, sigma2))
  ```

- *3. paper:* §5.6 (symbols separated: R residual, rho the declared dial) and the §11.3 give-up row
- *4. derivation:* docs/derivations/giveup-law-model.md (owner decision 2026-11-14 item 3, Fable round 1)
- *5. measured in regime?:* bind gauge, rho 0.900-0.999: Bulk 1-rho binds 50/100 (eps .05), 100/100 (eps >= .1); Auto and Realtime R binds 100/100 on every audit cell
- *6. clamp bind:* the min; bind fraction reported by giveup_law_tests and the UI gate
- *7. inputs honest?:* R inherits R03 (provisional until the sliding per-loss P_fec lands)
- *8. NO-MODE-SWITCH:* none (one formula; rho = 1 is the retain point; the engine still switches seats on reliable)
- *9. verdict:* **DEGENERATE(rho inert at Auto and Realtime under the shipped R), DIVERGENT**: giveup_law_tests + UI gate; sim gives up 4-10 (was 23-28) on the harsh cells, 0 at rho = 1; diverges from the engine only for 0 < 1 - rho < R
- *10. fix class, priority:* done (model); the engine seat switch stays (W3.2 if-reliable debt); rho-inert reading conditional on R03; **P3**, rank 37; depends on R03; ships with C13
- *flags:* measured n/a, gauge yes

<!-- END:rows -->
