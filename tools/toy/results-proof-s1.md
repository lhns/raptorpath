# Proof stage 1: the deciding row (CL-long at matched share), repair off

Plan: `docs/derivations/three-knobs-proof-plan.md` on `three-knobs-proof-plan` (sections 0, 2.3, 3, 8, 9). Code: `tools/toy/` on
`proof-s1`, built on `toy-stage3d-rttfix` (RTT-sampling fix). Reader and builders: `proof_s1.py`; tests: `test_toy2.py`
(`ProofS1TodaysRealtime`, `DenyTest`) and `test_proof_s1.py`; identity: `check_ident_proof_s1.py`.

## 1. Fixed before the run (F5)

Everything in this section was committed before the first run of the row.

### 1.1 What runs, and what does not

- **Cell:** CL-long@b (200 000 units at 10 000/s, link 12 500/s with a queue of 1 250, a 1.5 s outage at 2 s, the engine-like blocker
  sweep, RFC 6298 RTT), repair off, 12 paired seeds 0 to 11 with phases per seed, one commit, one batch.
- **Repair on (arm `RL`) is not run.** It needs stage 3e's rate law and decoder, which do not exist on this base
  (`git log --all -- tools/toy/batch3e.py` is empty). The plan's 54 configurations are 27 repair off plus 27 `RL`; only the 27 repair-off
  configurations (324 runs) and 12 A/A runs are possible here. The `RL` half is owed to stage 3e.
- **Today's Realtime rows are repair-off controls** (plan 2.3: machinery-matched for F2), never quoted as "today". Today's Realtime never
  ships with repair off.

### 1.2 Rows (named by what the sender and the receiver do, F6)

| row set | rows | what it is |
|---|---|---|
| lifetime T, hold 0 | T in {0.4, 0.6, 0.8, 1.0, 1.2, 1.25, 1.3, 1.35, 1.4, 1.45, 1.5, 1.55, 1.6, 2.0, inf} | the new model's curve, any order (S5). T = inf is today's object API by identity |
| lifetime T, hold inf | T in {0.4, 1.4, inf} | the same sender with strict order. T = inf, hold inf must equal today's Bulk on every key |
| ADR-0084 sender at rho, today's receiver | rho in {0.95, 0.98, 0.99, 0.995, 1} | ADR-0084's curve (C1). rho = 1 never gives up |
| today's Realtime, repair-off control | delta in {0.5, 5, 50} | arm `E-rt` (plan 2.3), see 1.3 |
| never gives up, strict order = today's Bulk | one | C3 |
| A/A | lifetime T = inf, hold 0, phase variant 1 | the noise floor, frozen to `nf_proof_s1.json` before any row is read |
| conditional 2 x 2 cross | ADR-0084 sender at 0.99 with an any-order receiver; ADR-0084 sender at 0.99 without its copy | run only if the ADR-0084 matched-share comparison reads "shown worse" (plan 3.4). The other cross cells exist already: ADR-0084 at 0.99 (C1), never gives up with today's receiver (ADR rho = 1), never gives up any order (T = inf, hold 0) |

### 1.3 Today's Realtime (`E-rt`): the differences from ADR-0084's sender, each with a red-first test

ADR-0084's sender (`S1c`, C1) becomes today's Realtime by these changes, each pinned in `test_toy2.py` `ProofS1TodaysRealtime`
(committed failing before the code, `2a3b80a0`):

1. **Budget** `budget = "resid"`: a shed is allowed iff `shed_total + 1 <= eps_hat (1 - P_fec) x source count`, with the sender's own cumulative
   shed count (shed.rs `shed_allowed`), refreshed at each source emission. `eps_hat` is the cell's first-send loss (0.03, Q21's true-loss
   default, a declared idealisation) and is 0 until the first RTT sample. Repair is off, so r = 0 and `P_fec = 0` by the shipped cut
   (`p_fec_normal` ported with its cut, debt W22, named in the deny test).
2. **Deadline** `d_arm = "EWMA"`: `D = min(b(delta), 2) x srtt` with the 50 ms seed.
3. **Silent shed:** no verdict copy (sender S1) and no floor, repeated or not (`floor_send = False`).
4. **Window** `evict_window = 200`: a hole more than 200 seqs behind the send frontier is neither served nor shed.
5. **No source pause** (`pause_B` infinite): the link queue drops instead.
6. **Two paths:** `place = "dup"` duplicates every first send on the second path. Built and tested; not exercised in this one-path row.
7. **Receiver:** R0e with the reorder buffer's force drain at 500 entries (`r0e_max`), draining the oldest to half the cap as `reorder.rs` does.

Taking it apart gives ADR-0084's sender at the derived allowance (test `test_taken_apart_it_equals_adr_0084s_sender_at_the_derived_allowance`,
on a cell where every resend is lost, so that a shed hole is never filled: where a shed hole is filled later, ADR-0084's outstanding count and
the shipped cumulative count differ by design).

Pinned shipped behaviour, not patched: with no floor, an object whose last units are shed never completes (the receiver never learns of the
tail). A stream that continues past its outage is unaffected.

Not modelled, a recorded divergence (plan 2.3): the probabilistic correction-slot retransmit.

### 1.4 Readouts, bars and the reading rule

- **Readouts under the pass rule:** on time at 0.4 s (`share_by_0p4`, higher is better) and recovery time after the outage (`recovery_time`,
  window rule of 17.4 at tau = 0.4 s, lower is better). A recovery that is "not within the run" is censored at 14.5 s (the readable end) and
  counted, never dropped. Delivered at all (`share_any`) is the x axis.
- **Gates, each 0 or true in every run:** `finished`, `commit_viol`, `l1_viol` (new gauge: a payload staged, first send or copy, at age >= T), and
  the T = inf, hold inf identity with today's Bulk on every key. (The worth-edge gate belongs to stage 3e.)
- **Curves and matching (plan 1.2):** per seed, each design's points sorted by that seed's x; the new curve is interpolated at that seed's
  baseline x; no extrapolation. A baseline point outside the new curve's range for any seed reads "outside the swept range" and counts as not
  passed. Neighbouring points whose mean x differ by no more than max(A/A floor on `share_any`, 1/units) keep the one with the better mean y.
- **Reading:** `summarize2.read`'s rule on the paired difference (bootstrap 95 % interval of the mean), signed so that positive favours the new
  model, against `m = max(T_bar, NF)`. `T_bar` for shares: `max(1/units, 0.05 (1 - share_ref))`; for recovery: `max(5 % of the reference, 0.1 s)`.
  `NF` = p95 over the 12 seeds of |A - A'| per readout, A = lifetime T = inf, hold 0, A' = the same with phase variant 1.
- **Comparisons:** the new H = 0 curve against every ADR-0084 point and every today's Realtime point, and curve against curve both ways
  (each ADR-0084 point inside the new range against the new curve; each new point inside the ADR-0084 range against ADR-0084's curve),
  on both readouts.
- **Pass rule for this row:** (1) no comparison reads "shown worse" or "outside the swept range"; (2) at least one reads "shown better" (reported;
  rule 2 is judged over the whole table in the plan); (3) every gate holds; (4) the identity holds.
- **Stop rule (plan, and the brief):** if the row fails, stop and report. No Revision 4, no extra T point, no moved bar. The cross runs
  only if the matched-share comparison against ADR-0084 reads "shown worse".
- **Registered predictions (plan 3.3, quoted, not re-centred: the re-baseline values for these rows do not exist yet):** a plateau for T in
  about (1.34, 1.6) s with recovery 4.6 to 5.3 s at the matched share, matched T in [1.25, 1.6] s, plateau delivered share in [99.3 %, 99.6 %];
  a straight line below the plateau down to the 0.4 s point.

### 1.5 Deviations from the plan, stated before the run

- Repair off only (1.1).
- The A/A arm is the T = inf, hold 0 row (plan 3's identity row), not stage 3d's strict `C3S`; same noise definition.
- `l1_viol` is a new gauge added for the L1 gate (the plan lists the gate; no gauge existed).
- Today's Realtime's ε̂ is the cell's declared loss (Q21), since there is no estimator on this base; it is 0 before the first RTT sample.

### 1.6 Amendment of 2026-11-25, committed before any run exists (owner's delegated decisions, advisor review)

Where this section differs from 1.4 it supersedes it. No data had been generated when it was written.

- **(a) Today's Realtime rows are reported only, not under the verdict.** Repair is off, so the arm is not today's sender; its rows count
  once stage 3e's repair exists. The check asked for: with no repair sent, `p_fec_normal(r = 0, ...)` returns 0 by the shipped function's own
  first guard (`r <= 0`, the engine's formula at r = 0), so the give-up budget is `eps_hat`. That assumes no repair that is not there. The
  engine's budget with its shipped repair is smaller (`eps_hat (1 - P_fec)`), so the repair-off arm is allowed more give-ups than the shipped
  one, not fewer. It is not handicapped by the P_fec term and nothing is changed. It is a control and is labelled so.
- **(b) The verdict rests on ADR-0084's rho curve only**, repair off on both sides: the five point comparisons and the curve-against-curve
  comparisons in both directions, on both readouts. Today's Realtime points and Bulk are reported in the table.
- **(c) Out of range.** A baseline point below the new curve's lowest delivered share is compared against that lowest point (per seed, the
  lowest point's own value). The new model wins if it is no worse on on-time share and on recovery, since it already delivers more; the usual
  reading rule applies and "shown worse" is a loss. A baseline above the new curve's highest share still reads "outside the swept range" and
  counts as not passed. No lifetime point is added after data is seen.
- **(d) "Undetermined" is its own outcome**, neither pass nor fail. Any comparison under the verdict that reads undetermined at 12 seeds
  triggers one registered doubling: seeds 12 to 23 of the arms in that comparison only (the baseline and the two points bracketing it on the
  other curve), then the comparison is read again on 24 seeds with those arms. If it is still undetermined, the row's outcome is
  UNDETERMINED (shown worse anywhere is FAILS; all comparisons equivalent or shown better is PASSES). No second doubling.
- **Outcome labels:** PASSES, FAILS, UNDETERMINED. A gate failure or an identity difference is FAILS.
- **Reader:** `proof_s1.py` (`classify`, `curve_at(clamp_low)`, `bracket`, `cmd_read`), with tests on synthetic curves in `test_proof_s1.py`.
- **Run gating:** the batch starts only on a marker file created by the coordinator after the re-baseline has been checked and pushed.

### 1.7 Amendment of 2026-11-25, committed before any run exists

- **Estimator.** Every arm of the row already runs `rtt_est = "rfc"` (RFC 6298 srtt/rttvar, Karn's rule): the cell CL-long@b sets it, and the
  S5, ADR-0084, today's Realtime, Bulk and A/A configurations all inherit it. A test pins this for every job
  (`test_every_arm_uses_the_rfc_estimator_and_todays_realtime_keeps_its_deadline_seed`). No alignment was needed, so the code is unchanged
  and the 16-configuration identity sample (PASS, base `e53eab45`) still stands. Stage 3e can reuse these runs on this point.
- **Today's Realtime keeps its 50 ms seed on its deadline.** `D_now` with `d_arm = "EWMA"` reads 50 ms until the first RTT sample and the
  sample after it, whatever the estimator; the same test pins both values. (The cold 333 ms seed of the rfc estimator feeds the cooldown, the
  floor and the receiver's R, not this deadline.)
- **A/A, two pairs.** S5 at T = inf, hold 0 against its phase variant 1, and S5 at T = 0.4, hold 0 against its phase variant 1 (12 more runs).
  Per readout (`share_any`, `share_T`, `share_by_0p4`, `recovery_time`) the noise floor NF is the larger of the two pairs' p95 over 12 seeds
  of |A - A'|, frozen to `nf_proof_s1.json` before any row is read.
- **Degenerate floors.** A floor of 0 or at or below the readout's resolution (1/units for shares, 0.1 s for recovery) is listed in the
  frozen file and in the report. The reader needs no special case: the bar is `m = max(T_bar, NF)` and T_bar is never below that resolution, and
  the x-merge noise is `max(NF on share_any, 1/units)`.
- **Outcome rule, confirmed.** Any comparison under the verdict that is still undetermined after the one registered doubling makes the row
  UNDETERMINED (unless something reads shown worse or outside the range, which is FAILS).

## 2. Tests and identity

(filled after the run)

## 3. Results

(filled after the run)
