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

- Unit tests: `test_toy` and `test_toy2` 102 tests (including the 13 proof stage 1 tests and the extended deny test) and `test_proof_s1` 17 tests: 119,
  all OK. The 102 were last run in full before the amendments; `toy2.py` has not changed since.
- Identity (`check_ident_proof_s1.py`): the new fields at their defaults reproduce the base `e53eab45` on every key: 2139 old keys over 16
  configurations (seed 0: C1/R0e, C3, S5, link cells, the `@b` form, two paths), traces and per-seq arrays as digests. PASS. New keys: the gauges
  `dup_first`, `evict_skip`, `force_drains`, `l1_viol`.
- In the row: lifetime T = inf with hold inf equals today's Bulk on 37 keys x 12 seeds, 0 differences; no run failed a gate (unfinished,
  commitment, L1).
- A/A (frozen in `nf_proof_s1.json`, two pairs, larger floor per readout): `share_by_0p4` 0.0051, `recovery_time` 0.10 s, `share_any` and `share_T` 0.00126.
  Degenerate floors (0 by construction, or at or under the readout's resolution), listed in the file: `recovery_time` at the T = 0.4 pair (0),
  `share_any` and `share_T` at the T = inf pair (0). The reading bar `max(T_bar, NF)` never falls below the readout's resolution, so these
  have no effect on a reading.

## 3. Results

**Scope, stated plainly: this is one matched point, with repair off, on one path, in one cell (CL-long@b).** ADR-0084's sender delivers the same
share (0.9921) at every rho, so its "curve" is one point on the share axis and the row compares the new model with ADR-0084 at that one share.
Repair is off, so nothing here says anything about today's Realtime as shipped, and the repair-on half (`RL`) is not run.

**Outcome: ROW PASSES** (10 comparisons under the verdict: 0 worse, 0 undetermined, 10 shown better; no doubling was needed).
At the matched share 0.9921 the new model (hold 0, T about 1.4 s) recovers in 4.43 s against ADR-0084's 4.80 s (0.37 s ahead, bar 0.24 s) and is
0.027 ahead on on-time share (bar 0.016). Against ADR-0084 at rho = 0.95 the lead is 1.04 s and 0.060. Today's Realtime rows (repair-off controls) are
reported only: they deliver 0.8975 to 0.8984, below the new curve's lowest point (0.9412), so they are read against that lowest point (the new model is
better by 0.043 on-time share; recovery 0 against 0, equivalent). Their 62 220 evict-window skips and 5 604 sheds show the window and the silent shed executing.

### 3.1 The full table

#### Every row (repair off, CL-long@b, paired seeds, means over the first 12 seeds)

| row | share_any | on time at 0.4 s | on time at T | recovery (s) | intake expired (dequeue) | floor passes | unfinished / commit / L1 | evict-window skips | sheds | budget refusals | queue drops | p1 viol |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| lifetime T=0p4, hold 0 (S5) | 0.9412 | 0.9412 | 0.9412 | 0.00 | 375 | 1086 | 0 / 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| lifetime T=0p6, hold 0 (S5) | 0.9511 | 0.9053 | 0.9511 | 0.57 | 375 | 1081 | 0 / 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| lifetime T=0p8, hold 0 (S5) | 0.9615 | 0.8579 | 0.9615 | 1.51 | 373 | 1082 | 0 / 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| lifetime T=1, hold 0 (S5) | 0.9708 | 0.8097 | 0.9708 | 2.50 | 375 | 1082 | 0 / 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| lifetime T=1p2, hold 0 (S5) | 0.9814 | 0.7626 | 0.9814 | 3.41 | 375 | 1082 | 0 / 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| lifetime T=1p25, hold 0 (S5) | 0.9841 | 0.7509 | 0.9841 | 3.63 | 376 | 1082 | 0 / 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| lifetime T=1p3, hold 0 (S5) | 0.9868 | 0.7392 | 0.9868 | 3.89 | 375 | 1082 | 0 / 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| lifetime T=1p35, hold 0 (S5) | 0.9892 | 0.7273 | 0.9892 | 4.13 | 375 | 1082 | 0 / 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| lifetime T=1p4, hold 0 (S5) | 0.9915 | 0.7151 | 0.9915 | 4.38 | 375 | 1082 | 0 / 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| lifetime T=1p45, hold 0 (S5) | 0.9942 | 0.7022 | 0.9941 | 4.62 | 94 | 1063 | 0 / 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| lifetime T=1p5, hold 0 (S5) | 0.9953 | 0.6955 | 0.9949 | 4.74 | 0 | 942 | 0 / 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| lifetime T=1p55, hold 0 (S5) | 0.9956 | 0.6839 | 0.9951 | 4.99 | 0 | 879 | 0 / 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| lifetime T=1p6, hold 0 (S5) | 0.9960 | 0.6740 | 0.9954 | 5.20 | 0 | 801 | 0 / 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| lifetime T=2, hold 0 (S5) | 0.9993 | 0.6531 | 0.9991 | 5.63 | 0 | 148 | 0 / 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| lifetime T=inf, hold 0 (S5) | 1.0000 | 0.6500 | 1.0000 | 5.67 | 0 | 0 | 0 / 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| lifetime T=0p4, hold inf (S5) | 0.9412 | 0.9347 | 0.9347 | 0.00 | 375 | 1086 | 0 / 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| lifetime T=1p4, hold inf (S5) | 0.9915 | 0.6699 | 0.9851 | 5.39 | 375 | 1082 | 0 / 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| lifetime T=inf, hold inf (S5) | 1.0000 | 0.6093 | 1.0000 | 6.73 | 0 | 0 | 0 / 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| ADR-0084 sender at rho=0p95, today's receiver (C1) | 0.9921 | 0.6531 | 0.9921 | 5.47 | 0 | 23 | 0 / 0 / 0 | 0 | 6937 | 10 | 0 | 1 |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | 0.9921 | 0.6716 | 0.9921 | 5.09 | 0 | 24 | 0 / 0 / 0 | 0 | 5227 | 25396 | 0 | 447 |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | 0.9921 | 0.6858 | 0.9921 | 4.80 | 0 | 16 | 0 / 0 / 0 | 0 | 2477 | 57347 | 0 | 1964 |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | 0.9921 | 0.6858 | 0.9921 | 4.80 | 0 | 3 | 0 / 0 / 0 | 0 | 600 | 64146 | 0 | 3036 |
| ADR-0084 sender at rho=1, today's receiver (C1) | 0.9921 | 0.6861 | 0.9921 | 4.80 | 0 | 0 | 0 / 0 / 0 | 0 | 0 | 66250 | 0 | 3396 |
| today's Realtime, repair-off control, delta=0p5 (E-rt) | 0.8984 | 0.8984 | 0.8984 | 0.00 | 0 | 0 | 0 / 0 / 0 | 62220 | 5604 | 98 | 0 | 0 |
| today's Realtime, repair-off control, delta=5 (E-rt) | 0.8975 | 0.8975 | 0.8975 | 0.00 | 0 | 0 | 0 / 0 / 0 | 62220 | 5604 | 100 | 0 | 0 |
| today's Realtime, repair-off control, delta=50 (E-rt) | 0.8975 | 0.8975 | 0.8975 | 0.00 | 0 | 0 | 0 / 0 / 0 | 62220 | 5604 | 100 | 0 | 0 |
| never gives up, strict order = today's Bulk (C3) | 1.0000 | 0.6093 | 1.0000 | 6.73 | 0 | 0 | 0 / 0 / 0 | 0 | 0 | 311408 | 0 | 0 |

#### New model's H = 0 curve (T sweep) against each baseline point

Paired difference (the new curve interpolated per seed at that seed's baseline share, minus the baseline), signed so that positive favours the new model, against m = max(T_bar, A/A floor). A baseline below the curve's lowest delivered share is compared against the lowest point (amendment 1.6c). Today's Realtime rows are reported only, not under the verdict (1.6a).

| baseline | readout | difference | outcome | under the verdict |
|---|---|---|---|---|
| ADR-0084 sender at rho=0p95, today's receiver (C1) | share_by_0p4 | 0.0595 [0.0576, 0.0616] vs m=0.0173: shown better | shown better | yes |
| ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery_time | 1.04 [0.978, 1.1] vs m=0.273: shown better | shown better | yes |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | share_by_0p4 | 0.0409 [0.0389, 0.0433] vs m=0.0164: shown better | shown better | yes |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery_time | 0.659 [0.612, 0.711] vs m=0.255: shown better | shown better | yes |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | share_by_0p4 | 0.0268 [0.0248, 0.0289] vs m=0.0157: shown better | shown better | yes |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery_time | 0.369 [0.316, 0.425] vs m=0.24: shown better | shown better | yes |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | share_by_0p4 | 0.0268 [0.0248, 0.0289] vs m=0.0157: shown better | shown better | yes |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery_time | 0.369 [0.316, 0.425] vs m=0.24: shown better | shown better | yes |
| ADR-0084 sender at rho=1, today's receiver (C1) | share_by_0p4 | 0.0265 [0.0247, 0.0282] vs m=0.0157: shown better | shown better | yes |
| ADR-0084 sender at rho=1, today's receiver (C1) | recovery_time | 0.369 [0.322, 0.414] vs m=0.24: shown better | shown better | yes |
| today's Realtime, repair-off control, delta=0p5 (E-rt) | share_by_0p4 | 0.0428 [0.0424, 0.0432] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | no, reported only |
| today's Realtime, repair-off control, delta=0p5 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | no, reported only |
| today's Realtime, repair-off control, delta=5 (E-rt) | share_by_0p4 | 0.0436 [0.0432, 0.044] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | no, reported only |
| today's Realtime, repair-off control, delta=5 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | no, reported only |
| today's Realtime, repair-off control, delta=50 (E-rt) | share_by_0p4 | 0.0436 [0.0432, 0.044] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | no, reported only |
| today's Realtime, repair-off control, delta=50 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | no, reported only |

#### Curve against curve: the new model's points inside ADR-0084's range, against ADR-0084's interpolated rho curve

No curve-against-curve comparison was possible: the old curve's delivered share spans [0.9921, 0.9921] (width 2.2e-05); the old curve's points all lie within the merge noise (0.00126) of each other, so it is one point on the share axis. The matched-share comparison above is therefore a comparison at that one share.

#### Gates and identity

- runs failing a gate (unfinished, commitment violation, L1 violation): 0
- identity, lifetime T=inf with hold inf against today's Bulk (C3), 37 keys x 12 seeds: 0 differences

#### Outcome against the pass rule (this row, ADR-0084's rho curve only)

- comparisons under the verdict: 10; shown worse or outside the swept range: 0; undetermined: 0; shown better: 10
- rule 2 (above somewhere): yes
- **ROW PASSES**

### 3.2 Three readings, from the stored runs (no new runs)

**(1) The rho sweep: this is how the sender behaves in this cell; the builder is not miswired.**
- Delivered share is set by the receiver, not by rho. Today's receiver (R0e) gives up holes on its own early-skip budget, and that budget does not
  read rho: `pass_r0e` is 6 600 to 6 650 at every rho and 1 583 units are never delivered at every rho (`passed_unfilled` = `G_final` = 1 583, 0.79 % of
  200 000). The sender never resends a hole its receiver has passed, so rho cannot raise the share.
- What rho changes is who repairs the passed holes. About 5 000 passed holes are later filled at every rho (`late_r0e` 5 087, 5 079, 5 049, 5 026, 5 020), and
  total copies stay flat (`repair_sends` 5 958, 5 996, 5 969, 5 968, 6 020). At rho = 0.95 the repairs are verdict copies (`verdict_copies` 5 958, `serves` 0);
  at rho = 1 they are ordinary resends (`serves` 6 020, 0 verdicts); in between they mix. Most shed units do arrive: 5 363 of 6 937 sheds at 0.95
  (77 %), 4 393 of 5 227, 2 103 of 2 477, 507 of 600 (`vta`). `dup` is about 0, so a unit is repaired once, by its copy or by a resend. The stored runs do
  not record per sequence which copy filled which hole, so a late original cannot be told from a copy there.
- Wiring checks that hold: at rho = 1 there are exactly 0 verdicts and 0 floors; sheds fall monotonically with rho (6 937, 5 227, 2 477, 600, 0).
- Why 0.95 is worse (5.47 s, 0.653 on time): the verdict floor datagrams. It sends 4 465 floors at 0.95 against 16 at 0.99, and a floor datagram takes a
  link slot in the drain. Window-averaged link use is equal (0.821), so this is a reading from the counts, not a controlled ablation.
- For stage 3e: the builder is sound. A "rho curve" of ADR-0084 behind today's receiver in this cell is one share point; the reader now says so.

**(2) `p1 viol`: ADR-0084's row breaks its own contract at rho = 1, and nearly everywhere else; labelled.** `p1_viol` counts passes after which the
receiver's give-up count G exceeds the sender's allowance `floor((1 - rho) x sent)`; `p1_worst` is the largest excess (`toy2.py` `pass_below`). At rho = 1
every one of 3 396 passes violates (allowance 0, worst excess 1 586): the receiver passes about 6 600 holes and the run ends 1 583 units short, so the
row at rho = 1 delivers 99.21 %, not 100 %. At 0.995, 0.99 and 0.98 the worst excess is 956, 850 and 640; at 0.95 it is 10 over 1 check (the contract
nearly holds). So the rows "ADR-0084 sender at rho, today's receiver" with rho above 0.95 do not keep ADR-0084's retention contract in this cell: they
are ADR-0084's sender behind a receiver that gives up more than the sender allows. The row's name already says "today's receiver"; the verdict is a
comparison with that combination, which is what the plan registered (C1 with R0e).

**(3) The sign: a misprediction; the stored counts explain part of it.** The plan predicted the new model 0 to 0.4 s behind; it came out 0.37 s ahead.
- Copies and link use are not the cause. At matched share the new model sends more, not fewer: `repair_sends` 6 715 (T = 1.4) against 5 969 to 6 020
  (ADR-0084, any rho), and window link use is equal (0.822 against 0.819 to 0.821). Never giving up sends 8 039 and recovers in 5.67 s.
- ADR-0084's gain over never giving up is the receiver's give-up of 1 583 units (about 2 000 fewer sends than 8 039), as the plan said, and rho = 1 confirms
  it (the same 4.80 s with no sender give-up).
- What the new model has in addition is the dequeue rule: 375 intake units expire at the dequeue at every T up to 1.4 (94 at 1.45, 0 at 1.5), first at
  about 3.52 s, and they are never sent. Between T = 1.4 and 1.5 recovery moves from 4.38 to 4.74 s while those 375 expiries vanish and `repair_sends`
  rises from 6 715 to 6 906.
- The plan's prediction put the end of intake expiry at T about 1.34 s (drain wait 1.32 s plus R). In the run R is 0.039 s and the expiries last to T about
  1.45 to 1.5 s, so the matched point (T about 1.4 s) lies on the expiring segment, not on the plateau. That is a misprediction of where the segment ends. It
  accounts for the direction of the lead (units that are never sent), but no ablation isolated it, so the size of the 0.37 s is not attributed. Recorded as:
  misprediction, mechanism supported by the expiry counts, not proven.
- Registered bands (not re-centred): recovery at the matched share 4.43 s is below [4.6, 5.3]; the matched T of about 1.4 s is inside [1.25, 1.6]; the plateau share
  0.9953 to 0.9960 (T = 1.5 to 1.6) is inside [99.3 %, 99.6 %] at its upper edge.

### 3.3 What is not shown

- Nothing with repair on (`RL`), nothing on two paths, no other cell, no stage 3e rows. One cell, one matched share.
- The conditional cross was not run (no comparison read "shown worse").
- Today's Realtime rows are controls with no repair, so no claim about the shipped sender follows from them.
