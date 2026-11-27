# Toy simulation, stage 1 (sender rules S0, S1, S1c, S2, S3 and the LP bound)

Spec: `docs/derivations/skip-rule-efficiency.md` §6 (458e3c53). Code: `tools/toy/` (`toy.py`, `lp.py`, `batch.py`, `summarize.py`, `test_toy.py`). Not U, not RX. The toy fixes no constant and none of these numbers is engine provenance.

## Deviations from registration (all of them)

- **12 seeds instead of 20**, for every cell.
- **LP on fewer seeds in some cells:** 12 seeds for I1, B10, B60, I1H5; 6 for I2, G, B10H5 and the N=2 cells; 3 for I2H5 and GH5; none for the `*f` cells. The LP costs 27 s (G) and 57 s (I2) per solve where the count binds.
- **MILP on 3 seeds** (0-2), with a 90 s limit: on I2 and G the MILP value is an incumbent, so an upper bound on the optimum.
- The LP is solved over the full 20 s, not 10 s windows. Jitter, delta and rho sweeps, R0, U and S-hat against S0's true survival were not run.
- **Configurations added after registration (reported-only, no decision rests on them):** `*H5` (H = 5 ms, to make F1 visible, since H§8 = 0 at N=1), `*H5f` and `*f` (floor as its own datagram, see finding 2), and the three N=2 outage variants B10n2s/n2f/n2a. The five registered P5 cells are I1, I2, G, B10, B60 at base settings.

Base = N=1, RTT 10 ms, jitter 1 ms FIFO, H = H§8 (= 0 at N=1 FIFO), rho 0.99, 20 s (B60: 70 s), one process, about 1 h 20 min in total. One I1 run is about 2.5 s plus 1.9 s of LP (B60 8 s, N=2 4.4 s), so the 15 s fallback was not triggered.

## Checks

- Unit tests (11, `python -m unittest test_toy` in `tools/toy`): determinism and CRN; P1 at every pass (with a positive control that trips it); P2 (rho = 1 and 1 - 1e-7: sender traces of S1, S1c, S2, S3 equal S0's); LP <= every candidate's HOL (I2, G, H = 0 and 5 ms); MILP == brute force on 40 tiny paths; LP == brute force when the count is slack; P4a.
- **LP is not equal to brute force when the count binds** (15 of 40 tiny cases have LP < brute force, e.g. 0.0680 against 0.0702): HOL is convex in pass time, so the LP splits a unit across holes. MILP equals brute force. This deviates from the brief and was expected.
- P1: 0 violations in every run (column "P1 viol").
- P4a: S1 - S0 isolated-hole stall median = H +- 1 ms (test, H = 5 ms). P4b: S1c p50 = S0 p50 (0.00 ms), and G/S 0.00011 against the bound 1.5 eps h = 0.00019. P4d: see finding 8. P4c and P4e belong to U and are not applicable.

## Readings

**The `gap` column is not a distance to the optimum.** The LP is loose where the count binds (I2: LP 730 against a MILP incumbent of 1300; G: 134 against 297), and at H = 0 it bounds rather than ranks: it passes every hole at detect + H with no tell latency (about an RTT), which no sender rule can reach. Gap values (S1c): 0.99 at I1, 1.00 at B10, 0.85 at G, 0.59 at I2.

**Run-atomic (S2 against S1c): DROP, as registered.** Bar applied verbatim: kept only if it improves HOL p99 or largest-episode waste beyond the CI in some cell and worsens none beyond the CI.
- Largest-episode spend falls at B10 (-191 [-192, -191] units, 942 to 750), B10H5 -190, B10n2a -190, B10n2f -95 [-98, -91]; that is the gain. Elsewhere identical (I1, G, B60) or within CI.
- HOL p99 worsens beyond the CI at B10n2f (+0.025 ms [0.023, 0.027]) and B10n2a (+0.026 ms [0.025, 0.027]). So the bar fails, and the result is DROP as registered; no ruling is needed.
- **Instrument defect for later stages:** with common random numbers the paired CI collapses, so any deterministic one-tick difference "excludes 0" and the bar's "beyond CI" cannot act as a size threshold in this toy (here 0.025 ms of a 20.7 ms p99). The bar needs a stated minimum size.
- The right wording is **"not shown to help; its designed case (a closed run longer than the bank) was never produced"**, not "refuted": see finding 8. S2 does not equal S1c at N=1 blackouts as section 3.3 predicts: it drains 20 % less, as it declines a sweep piece that does not fit whole. B60 shows S2 = S1c (bank 2000 > run). I1, I2 and G have short runs.

**T_rho (S3 against S2): DROP.**
- Inert where predicted: at I1 it sits at D, and S3 = S2 at B10, B60, I1H5, I2H5 (identical).
- It acts only at I2 and G (h above 1 - rho), at H = 0: I2 p99 -3.41 ms [-3.74, -3.08], spend -48, but repair sends double (0.030 to 0.060 per delivered) and total HOL +21 packet-seconds; **G p99 +0.37 ms [+0.32, +0.42]**, spend -35, total HOL +39. B60: zero difference. The bar (p99 falls beyond CI in G or B60, none worsens) fails.
- **Why H = 5 ms switches it off (checked, not a bug).** T_rho = max(D, S^-1((1-rho)/h) - L) with L = owd + H. At I2 the measured T_rho is 14.2 ms at H = 0 and 9.2 ms at H = 5 ms (G: 14.0 and 8.9): it moves one-for-one with H. At N=1 the first report of a hole arrives at age about RTT + tick = 10-12 ms, so the gate (age >= T_rho) refuses only while S^-1 - owd - H exceeds that, i.e. only for H below about 3 ms (the sampler-tick slack). The gate is therefore dominated for any H above that: I2H5 and GH5 show 0 and 2 refusals. At N=2 under H§8 (H about the 20 ms skew) the same holds: a one-seed 8 s check (I2 and G, phi_slow 0.5) gives 0 refusals and S3 = S2 at H§8, against 9 457 and 5 496 refusals at H = 0. So T_rho is inert at N >= 2 under H§8, and active in the toy only at N=1 with H below about 3 ms.

**Findings**
1. **The age half of the verdict was never exercised.** The `D-fail` column counts `age_gate_fail`, the number of times the verdict gate refused a hole: for S1, S1c and S2 that is "age <= D", for S3 "age < T_rho". For S1, S1c and S2 it is 0 in every run of every cell, the N=2 ones included (the maximum over all result files is 0): at N=1 the first report of a hole is older than D = RTprop/2, and at N=2 a ripe slow-path hole is older than 9/8 x 25 ms. D is degenerate at N=1 (spec prediction confirmed), and the age test has never decided anything in stage 1. **Carry into the stage with skew and U:** the D gate must be exercised there (a cell where reports can be younger than D, e.g. reorder-allowed jitter or a fast path reporting a slow-path seq early) or its reading stays untested.
2. **Floor model. The engine-like model is the separate datagram (`*f`, reported-only); shared fate is a toy artefact.** The engine sends the floor as its own WindowFloor control datagram, staged right after the copy on the same path and repeated on the tail-sweep clock on every live path. The literal first model (floor in the copy's datagram, same fate: a lost copy loses the floor, and the refresh costs an extra round, resolution up to about 40 ms against S0's 24 ms) is therefore not the engine. HOL p99 for S1c (ms; S0 is 14.4 / 33.4 / 19.7 at I1 / I2 / G):

   | cell | shared | separate (`*f`) |
   |---|---|---|
   | H = 0: I1 | 13.57 | 11.51 |
   | H = 0: I2 | 18.58 | 13.13 |
   | H = 0: G | 11.96 | 12.47 |
   | H = 5 ms: I1 | 20.48 | 11.99 |
   | H = 5 ms: I2 | 23.76 | 18.20 |
   | H = 5 ms: G | 14.14 | 15.89 |

   - **H effect against datagram effect.** Separating the datagram at H = 0 alone gives 13.57 to 11.51 at I1 and 18.58 to 13.13 at I2, so the shared-fate refresh round is the bigger cost; H = 5 ms costs 0.5 ms (I1) to 5 ms (I2) on top of either model.
   - **Why I1H5f (11.99) beats the H = 0 shared cell (13.57):** in the shared model a lost copy also loses its floor, so the 2.5 % of holes with a lost copy wait out the refresh round; with a separate floor they are passed at floor arrival + H = 5 ms and beat S0's second resend. The saved round is worth more than H. I1f at H = 0 (11.51) confirms it.
   - **Why separate is worse than shared at G (15.89 against 14.14 at H = 5; 12.47 against 11.96 at H = 0):** not isolated. The data are consistent with redundancy: bursts verdict runs of about 3.7 holes per floor pass in G, and in the shared model the floor rides each of the run's copies, so one surviving copy delivers it, while the separate floor is a single datagram with one loss draw (50 % inside a burst); a lost floor is covered only by the next, higher floor. At I1 (about 1 copy per floor) the redundancy is absent, which is why shared is not better there. Floor refreshes are rare in both (under 0.3 per run), so the refresh round is not the cause at G.
   - At H = 0 the separate floor beats its copy in about half of the verdicts (own jitter draw), so the hole is passed and then filled late: transient passes are 487 per run against 65 at I1, and largest-episode spend 5 against 2. The late fills refund G_rx, but the order was given up for no gain; this is a cost of H = 0 with an independent floor.
   - Run-atomic and T_rho readings do not change with the floor model (`*f` cells: S2 - S1c within 0.2 ms at I2H5f/GH5f, CI containing 0; S3 - S2 at GH5f -0.11 ms [-0.32, 0.00]).
3. **S1 as written is slower than S0** by H at isolated holes (I1H5: HOL p50 2.6 ms against 0, p99 21.4 against 14.4): F1 confirmed; it also spends the allowance (0.49 % of S_src at I1).
4. **The cumulative count does not bind at I1** (S1c spends 0.011 % = eps h), binds at I2 (S1 uses exactly 1.000 %, 15 000 refusals per run) and in G bursts.
5. **Outage cells carry almost no HOL signal** (B10, B60 p99 = 0.00 ms): on a dead link nothing arrives, so no packet waits; the damage is the late holes, whose own delay the metric excludes by spec. Only spend can rank run-atomic in outages.
6. The source pause fires at N=1 B10 (max outstanding 2 048, one pause per run): 2 000 outage seqs plus one RTT of flight exceed B = 2 048.
7. At N=2 with phi_slow > 0 the stall metric is meaningless: skew keeps a hole outstanding at all times, so the whole run is one stall episode (the stall p99 column is about the run length there). HOL p99 is unaffected.
8. **P4d at N=2 as written is not reproducible.** "S1 spends > 0 for a gain within CI of 0 and S2 spends 0" holds only for a slow-path outage with phi_slow = 0.5 (B10n2s): S1 spends 595, S2 spends 0, but S1c also spends 0 there (copies ride the live fast path), so the zero comes from the last copy, not run-atomic. Fast-path outage (B10n2f): S1 595, S1c 464, S2 369. Both paths (B10n2a) behaves as N=1 (S2 750). No setting in the sweep gives a closed run longer than the bank: with phi_slow <= 0.5 consecutive missing seqs are short, and a long closed run appears only after data resumes, when the sweep has already drained the bank. N=1 side as predicted: S2 drains (750) like S1 (942).

## Ambiguities in the spec and how they were settled

- Loss cells: v1's table gives I2 = 3 %/10 %, G = Gilbert-Elliott burst 8, bad-state loss 0.5, mean 2.5 %; I1 = h 0.5 %, eps 2.5 % (worked example; its 42-67 % overlap figure matches). Blackout cells carry no other loss. B at N=2 is "on one path" (v1), path unnamed, so slow-path, fast-path and both-path variants were run.
- D = RTprop/2 fixed (no delta dependence in stage 1); tail sweep period clamp(2 SRTT, 25, 100) ms (`hole_nack_refresh`); the hole law (9/8 x owd of the seq's path) applies only with two paths, as the spec says; the 9/8, the floor-refresh interval (RTT + 2 ticks + H + jitter) and the jitter law (uniform 0..J) are toy choices.
- Floor semantics: F = lowest seq that is neither verdicted nor known received; a floor is sent only while a verdict is pending below it (so rho = 1 traces equal S0's).
- Kaplan-Meier input: the sender must tell a pass from a fill when a verdicted hole leaves a report, which the (next_expected, G_rx) pair does not give per seq, so the toy report carries the seqs passed since the previous report (observer channel; ages use the report's generation time).
- HOL is summed over non-hole packets only (same set in every candidate and in the LP); hole packets' own delay is excluded as in the spec. The LP books the prefix budget at S_src(p_s) per the spec text, which can be earlier than a candidate's verdict; no case with LP above a candidate occurred.
- Isolated holes: no other lost original within 300 seqs (policy independent, so the same set across candidates).

## Tables (12 seeds; gap = (HOL_cand - LP)/(HOL_S0 - LP), not a distance to the optimum; G/S is the final G_rx over S_src; `D-fail` = verdict-gate refusals: age <= D for S1, S1c, S2; age < T_rho for S3; `*f` cells have S0, S1c, S2 only)

| config | cand | seeds | G/S (1-rho=0.01) | HOL p50 ms | HOL p99 ms | stall p99 ms | repair/deliv | max-episode spend | gap | refusals | D-fail | P1 viol |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| I1 | S0 | 12 | 0.00000 | 0.00 | 14.44 | 42.2 | 0.00503 | 0 (max 0) | 1.00 | 0 | 0 | 0 |
| I1 | S1 | 12 | 0.00491 | 0.00 | 14.25 | 44.3 | 0.00000 | 8 (max 11) | 1.00 | 2 | 0 | 0 |
| I1 | S1c | 12 | 0.00011 | 0.00 | 13.57 | 42.6 | 0.00492 | 2 (max 3) | 0.99 | 0 | 0 | 0 |
| I1 | S2 | 12 | 0.00011 | 0.00 | 13.57 | 42.6 | 0.00492 | 2 (max 3) | 0.99 | 0 | 0 | 0 |
| I1 | S3 | 12 | 0.00011 | 0.00 | 13.57 | 42.6 | 0.00492 | 2 (max 3) | 0.99 | 0 | 0 | 0 |
| I2 | S0 | 12 | 0.00000 | 10.41 | 33.38 | 628.9 | 0.03319 | 0 (max 0) | 0.96 | 0 | 0 | 0 |
| I2 | S1 | 12 | 0.01000 | 9.75 | 21.81 | 537.3 | 0.02231 | 69 (max 101) | 0.70 | 15493 | 0 | 0 |
| I2 | S1c | 12 | 0.00296 | 9.27 | 18.58 | 463.5 | 0.03003 | 56 (max 76) | 0.59 | 35 | 0 | 0 |
| I2 | S2 | 12 | 0.00296 | 9.27 | 18.58 | 463.5 | 0.03003 | 56 (max 76) | 0.59 | 31 | 0 | 0 |
| I2 | S3 | 12 | 0.00031 | 9.44 | 15.16 | 467.8 | 0.05976 | 9 (max 12) | 0.61 | 30 | 9937 | 0 |
| G | S0 | 12 | 0.00000 | 0.12 | 19.65 | 49.8 | 0.02540 | 0 (max 0) | 1.00 | 0 | 0 | 0 |
| G | S1 | 12 | 0.00999 | 0.12 | 16.73 | 49.8 | 0.01530 | 15 (max 23) | 0.95 | 15604 | 0 | 0 |
| G | S1c | 12 | 0.00060 | 0.03 | 11.96 | 46.3 | 0.02485 | 43 (max 50) | 0.85 | 79 | 0 | 0 |
| G | S2 | 12 | 0.00060 | 0.03 | 11.96 | 46.3 | 0.02485 | 43 (max 50) | 0.85 | 37 | 0 | 0 |
| G | S3 | 12 | 0.00003 | 0.11 | 12.33 | 49.8 | 0.04949 | 8 (max 14) | 0.92 | 34 | 4455 | 0 |
| B10 | S0 | 12 | 0.00000 | 0.00 | 0.04 | 12.3 | 0.03919 | 0 (max 0) | 1.00 | 0 | 0 | 0 |
| B10 | S1 | 12 | 0.00510 | 0.00 | 0.00 | 12.3 | 0.00949 | 942 (max 945) | 1.00 | 2601 | 0 | 0 |
| B10 | S1c | 12 | 0.00470 | 0.00 | 0.00 | 12.3 | 0.01500 | 941 (max 944) | 1.00 | 2589 | 0 | 0 |
| B10 | S2 | 12 | 0.00375 | 0.00 | 0.00 | 11.3 | 0.01781 | 750 (max 750) | 0.85 | 4 | 0 | 0 |
| B10 | S3 | 12 | 0.00375 | 0.00 | 0.00 | 11.3 | 0.01781 | 750 (max 750) | 0.85 | 4 | 0 | 0 |
| B60 | S0 | 12 | 0.00000 | 0.00 | 0.00 | 12.4 | 0.01369 | 0 (max 0) | 1.00 | 0 | 0 | 0 |
| B60 | S1 | 12 | 0.00282 | 0.00 | 0.00 | 11.9 | 0.00000 | 1972 (max 1974) | 0.94 | 0 | 0 | 0 |
| B60 | S1c | 12 | 0.00250 | 0.00 | 0.00 | 11.4 | 0.00293 | 1971 (max 1973) | 0.88 | 0 | 0 | 0 |
| B60 | S2 | 12 | 0.00250 | 0.00 | 0.00 | 11.4 | 0.00293 | 1971 (max 1973) | 0.88 | 0 | 0 | 0 |
| B60 | S3 | 12 | 0.00250 | 0.00 | 0.00 | 11.4 | 0.00293 | 1971 (max 1973) | 0.88 | 0 | 0 | 0 |
| I1H5 | S0 | 12 | 0.00000 | 0.00 | 14.44 | 42.2 | 0.00503 | 0 (max 0) | 1.00 | 0 | 0 | 0 |
| I1H5 | S1 | 12 | 0.00491 | 2.63 | 21.39 | 76.9 | 0.00000 | 12 (max 18) | 2.09 | 2 | 0 | 0 |
| I1H5 | S1c | 12 | 0.00011 | 0.00 | 20.48 | 47.6 | 0.00492 | 2 (max 2) | 1.08 | 0 | 0 | 0 |
| I1H5 | S2 | 12 | 0.00011 | 0.00 | 20.48 | 47.6 | 0.00492 | 2 (max 2) | 1.08 | 0 | 0 | 0 |
| I1H5 | S3 | 12 | 0.00011 | 0.00 | 20.48 | 47.6 | 0.00492 | 2 (max 2) | 1.08 | 0 | 0 | 0 |
| I2H5 | S0 | 12 | 0.00000 | 10.41 | 33.38 | 628.9 | 0.03319 | 0 (max 0) | 0.93 | 0 | 0 | 0 |
| I2H5 | S1 | 12 | 0.01000 | 11.81 | 26.72 | 837.7 | 0.02231 | 106 (max 162) | 1.03 | 15492 | 0 | 0 |
| I2H5 | S1c | 12 | 0.00296 | 9.83 | 23.76 | 523.5 | 0.03003 | 25 (max 37) | 0.65 | 35 | 0 | 0 |
| I2H5 | S2 | 12 | 0.00296 | 9.83 | 23.76 | 523.5 | 0.03003 | 25 (max 37) | 0.65 | 31 | 0 | 0 |
| I2H5 | S3 | 12 | 0.00296 | 9.83 | 23.76 | 523.5 | 0.03003 | 25 (max 37) | 0.65 | 31 | 0 | 0 |
| GH5 | S0 | 12 | 0.00000 | 0.12 | 19.65 | 49.8 | 0.02540 | 0 (max 0) | 1.00 | 0 | 0 | 0 |
| GH5 | S1 | 12 | 0.00999 | 3.32 | 21.06 | 68.1 | 0.01530 | 18 (max 25) | 1.84 | 15606 | 0 | 0 |
| GH5 | S1c | 12 | 0.00060 | 0.12 | 14.14 | 50.5 | 0.02485 | 10 (max 15) | 0.95 | 79 | 0 | 0 |
| GH5 | S2 | 12 | 0.00060 | 0.12 | 14.15 | 50.5 | 0.02485 | 10 (max 15) | 0.95 | 37 | 0 | 0 |
| GH5 | S3 | 12 | 0.00060 | 0.12 | 14.15 | 50.5 | 0.02485 | 10 (max 15) | 0.95 | 37 | 2 | 0 |
| B10H5 | S0 | 12 | 0.00000 | 0.00 | 0.04 | 12.3 | 0.03919 | 0 (max 0) | 1.00 | 0 | 0 | 0 |
| B10H5 | S1 | 12 | 0.00510 | 0.00 | 0.00 | 19.1 | 0.00949 | 975 (max 1020) | 2.06 | 2601 | 0 | 0 |
| B10H5 | S1c | 12 | 0.00470 | 0.00 | 0.00 | 14.1 | 0.01500 | 940 (max 943) | 1.05 | 2589 | 0 | 0 |
| B10H5 | S2 | 12 | 0.00375 | 0.00 | 0.00 | 14.1 | 0.01781 | 750 (max 750) | 1.06 | 4 | 0 | 0 |
| B10H5 | S3 | 12 | 0.00375 | 0.00 | 0.00 | 14.1 | 0.01781 | 750 (max 750) | 1.06 | 4 | 0 | 0 |
| I1H5f | S0 | 12 | 0.00000 | 0.00 | 14.44 | 42.2 | 0.00503 | 0 (max 0) | - | 0 | 0 | 0 |
| I1H5f | S1 | 12 | 0.00491 | 2.63 | 21.39 | 76.9 | 0.00000 | 12 (max 18) | - | 2 | 0 | 0 |
| I1H5f | S1c | 12 | 0.00011 | 0.00 | 11.99 | 40.5 | 0.00492 | 2 (max 2) | - | 0 | 0 | 0 |
| I1H5f | S2 | 12 | 0.00011 | 0.00 | 11.99 | 40.5 | 0.00492 | 2 (max 2) | - | 0 | 0 | 0 |
| I1H5f | S3 | 12 | 0.00011 | 0.00 | 11.99 | 40.5 | 0.00492 | 2 (max 2) | - | 0 | 0 | 0 |
| I2H5f | S0 | 12 | 0.00000 | 10.41 | 33.38 | 628.9 | 0.03319 | 0 (max 0) | - | 0 | 0 | 0 |
| I2H5f | S1 | 12 | 0.01000 | 11.81 | 26.72 | 837.7 | 0.02231 | 106 (max 162) | - | 15492 | 0 | 0 |
| I2H5f | S1c | 12 | 0.00296 | 9.63 | 18.20 | 500.0 | 0.03003 | 25 (max 37) | - | 35 | 0 | 0 |
| I2H5f | S2 | 12 | 0.00296 | 9.63 | 17.98 | 495.3 | 0.03003 | 25 (max 37) | - | 31 | 0 | 0 |
| I2H5f | S3 | 12 | 0.00296 | 9.63 | 17.98 | 495.3 | 0.03003 | 25 (max 37) | - | 31 | 0 | 0 |
| GH5f | S0 | 12 | 0.00000 | 0.12 | 19.65 | 49.8 | 0.02540 | 0 (max 0) | - | 0 | 0 | 0 |
| GH5f | S1 | 12 | 0.00999 | 3.32 | 21.06 | 68.1 | 0.01530 | 18 (max 25) | - | 15606 | 0 | 0 |
| GH5f | S1c | 12 | 0.00060 | 0.13 | 15.89 | 51.2 | 0.02485 | 10 (max 15) | - | 79 | 0 | 0 |
| GH5f | S2 | 12 | 0.00060 | 0.13 | 15.84 | 50.7 | 0.02485 | 10 (max 15) | - | 37 | 0 | 0 |
| GH5f | S3 | 12 | 0.00060 | 0.13 | 15.74 | 50.7 | 0.02485 | 10 (max 15) | - | 37 | 2 | 0 |
| I1f | S0 | 12 | 0.00000 | 0.00 | 14.44 | 42.2 | 0.00503 | 0 (max 0) | - | 0 | 0 | 0 |
| I1f | S1c | 12 | 0.00011 | 0.00 | 11.51 | 38.9 | 0.00492 | 5 (max 7) | - | 0 | 0 | 0 |
| I1f | S2 | 12 | 0.00011 | 0.00 | 11.51 | 38.9 | 0.00492 | 5 (max 7) | - | 0 | 0 | 0 |
| I2f | S0 | 12 | 0.00000 | 10.41 | 33.38 | 628.9 | 0.03319 | 0 (max 0) | - | 0 | 0 | 0 |
| I2f | S1c | 12 | 0.00296 | 9.03 | 13.13 | 409.8 | 0.03003 | 88 (max 108) | - | 35 | 0 | 0 |
| I2f | S2 | 12 | 0.00296 | 9.03 | 13.02 | 404.3 | 0.03003 | 93 (max 108) | - | 31 | 0 | 0 |
| Gf | S0 | 12 | 0.00000 | 0.12 | 19.65 | 49.8 | 0.02540 | 0 (max 0) | - | 0 | 0 | 0 |
| Gf | S1c | 12 | 0.00060 | 0.06 | 12.47 | 47.4 | 0.02485 | 32 (max 40) | - | 79 | 0 | 0 |
| Gf | S2 | 12 | 0.00060 | 0.06 | 12.32 | 47.3 | 0.02485 | 32 (max 43) | - | 37 | 0 | 0 |
| B10n2s | S0 | 12 | 0.00000 | 8.20 | 20.59 | 20019.6 | 0.03867 | 0 (max 0) | 1.00 | 0 | 0 | 0 |
| B10n2s | S1 | 12 | 0.00297 | 18.92 | 20.74 | 20019.6 | 0.00874 | 595 (max 623) | 78.11 | 1742 | 0 | 0 |
| B10n2s | S1c | 12 | 0.00000 | 8.20 | 20.59 | 20019.6 | 0.01505 | 0 (max 0) | 1.00 | 1742 | 0 | 0 |
| B10n2s | S2 | 12 | 0.00000 | 8.20 | 20.59 | 20019.6 | 0.01788 | 0 (max 0) | 1.00 | 4 | 0 | 0 |
| B10n2s | S3 | 12 | 0.00000 | 8.20 | 20.59 | 20019.6 | 0.01788 | 0 (max 0) | 1.00 | 4 | 0 | 0 |
| B10n2f | S0 | 12 | 0.00000 | 4.84 | 20.57 | 10019.4 | 0.03871 | 0 (max 0) | 1.00 | 0 | 0 | 0 |
| B10n2f | S1 | 12 | 0.00298 | 18.95 | 38.92 | 10019.4 | 0.00874 | 595 (max 614) | 2.23 | 1742 | 0 | 0 |
| B10n2f | S1c | 12 | 0.00232 | 18.90 | 20.69 | 10019.4 | 0.01650 | 464 (max 486) | 1.86 | 1742 | 0 | 0 |
| B10n2f | S2 | 12 | 0.00185 | 18.91 | 20.71 | 10019.4 | 0.01980 | 369 (max 383) | 1.86 | 4 | 0 | 0 |
| B10n2f | S3 | 12 | 0.00185 | 18.91 | 20.71 | 10019.4 | 0.01980 | 369 (max 383) | 1.86 | 4 | 0 | 0 |
| B10n2a | S0 | 12 | 0.00000 | 0.51 | 20.52 | 10017.4 | 0.03931 | 0 (max 0) | - | 0 | 0 | 0 |
| B10n2a | S1 | 12 | 0.00510 | 18.86 | 20.64 | 10017.6 | 0.00962 | 1021 (max 1021) | - | 2755 | 0 | 0 |
| B10n2a | S1c | 12 | 0.00470 | 18.82 | 20.62 | 10017.6 | 0.01537 | 940 (max 944) | - | 2266 | 0 | 0 |
| B10n2a | S2 | 12 | 0.00375 | 18.87 | 20.64 | 10017.6 | 0.01795 | 750 (max 750) | - | 4 | 0 | 0 |
| B10n2a | S3 | 12 | 0.00375 | 18.87 | 20.64 | 10017.6 | 0.01795 | 750 (max 750) | - | 4 | 0 | 0 |

### P5 readings: paired per-seed differences, mean [95% bootstrap CI], seeds in brackets

| config | contrast | metric | diff | CI | n |
|---|---|---|---|---|---|
| I1 | S2 - S1c | hol_p99 | 0 | [0, 0] | 12 |
| I1 | S2 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| I1 | S2 - S1c | hol_sum_nonhole | 0 | [0, 0] | 12 |
| I1 | S3 - S2 | hol_p99 | 0.0002505 | [0, 0.0007514] | 12 |
| I1 | S3 - S2 | max_ep_spend | 0 | [0, 0] | 12 |
| I1 | S3 - S2 | hol_sum_nonhole | 0.01095 | [0, 0.03285] | 12 |
| I1 | S3 - S1c | hol_p99 | 0.0002505 | [0, 0.0007514] | 12 |
| I1 | S3 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| I1 | S3 - S1c | hol_sum_nonhole | 0.01095 | [0, 0.03285] | 12 |
| I2 | S2 - S1c | hol_p99 | -0.008321 | [-0.02196, -2.901e-06] | 12 |
| I2 | S2 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| I2 | S2 - S1c | hol_sum_nonhole | -0.07684 | [-0.197, 0.0218] | 12 |
| I2 | S3 - S2 | hol_p99 | -3.412 | [-3.742, -3.082] | 12 |
| I2 | S3 - S2 | max_ep_spend | -47.58 | [-52.75, -42.83] | 12 |
| I2 | S3 - S2 | hol_sum_nonhole | 21.2 | [17.52, 24.73] | 12 |
| I2 | S3 - S1c | hol_p99 | -3.421 | [-3.755, -3.088] | 12 |
| I2 | S3 - S1c | max_ep_spend | -47.58 | [-52.75, -42.83] | 12 |
| I2 | S3 - S1c | hol_sum_nonhole | 21.12 | [17.51, 24.62] | 12 |
| G | S2 - S1c | hol_p99 | -3.526e-06 | [-1.039e-05, 3.498e-07] | 12 |
| G | S2 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| G | S2 - S1c | hol_sum_nonhole | 0.02055 | [0.0002413, 0.04987] | 12 |
| G | S3 - S2 | hol_p99 | 0.3709 | [0.3161, 0.4208] | 12 |
| G | S3 - S2 | max_ep_spend | -35 | [-37.25, -32.67] | 12 |
| G | S3 - S2 | hol_sum_nonhole | 39.38 | [37.61, 41.07] | 12 |
| G | S3 - S1c | hol_p99 | 0.3709 | [0.3161, 0.4208] | 12 |
| G | S3 - S1c | max_ep_spend | -35 | [-37.25, -32.67] | 12 |
| G | S3 - S1c | hol_sum_nonhole | 39.4 | [37.62, 41.1] | 12 |
| B10 | S2 - S1c | hol_p99 | 0 | [0, 0] | 12 |
| B10 | S2 - S1c | max_ep_spend | -191.3 | [-192.2, -190.5] | 12 |
| B10 | S2 - S1c | hol_sum_nonhole | -0.1165 | [-0.1172, -0.1157] | 12 |
| B10 | S3 - S2 | hol_p99 | 0 | [0, 0] | 12 |
| B10 | S3 - S2 | max_ep_spend | 0 | [0, 0] | 12 |
| B10 | S3 - S2 | hol_sum_nonhole | 0 | [0, 0] | 12 |
| B10 | S3 - S1c | hol_p99 | 0 | [0, 0] | 12 |
| B10 | S3 - S1c | max_ep_spend | -191.3 | [-192.2, -190.5] | 12 |
| B10 | S3 - S1c | hol_sum_nonhole | -0.1165 | [-0.1172, -0.1157] | 12 |
| B60 | S2 - S1c | hol_p99 | 0 | [0, 0] | 12 |
| B60 | S2 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| B60 | S2 - S1c | hol_sum_nonhole | 0 | [0, 0] | 12 |
| B60 | S3 - S2 | hol_p99 | 0 | [0, 0] | 12 |
| B60 | S3 - S2 | max_ep_spend | 0 | [0, 0] | 12 |
| B60 | S3 - S2 | hol_sum_nonhole | 0 | [0, 0] | 12 |
| B60 | S3 - S1c | hol_p99 | 0 | [0, 0] | 12 |
| B60 | S3 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| B60 | S3 - S1c | hol_sum_nonhole | 0 | [0, 0] | 12 |
| I1H5 | S2 - S1c | hol_p99 | 0 | [0, 0] | 12 |
| I1H5 | S2 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| I1H5 | S2 - S1c | hol_sum_nonhole | 0 | [0, 0] | 12 |
| I1H5 | S3 - S2 | hol_p99 | 0 | [0, 0] | 12 |
| I1H5 | S3 - S2 | max_ep_spend | 0 | [0, 0] | 12 |
| I1H5 | S3 - S2 | hol_sum_nonhole | 0 | [0, 0] | 12 |
| I1H5 | S3 - S1c | hol_p99 | 0 | [0, 0] | 12 |
| I1H5 | S3 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| I1H5 | S3 - S1c | hol_sum_nonhole | 0 | [0, 0] | 12 |
| I2H5 | S2 - S1c | hol_p99 | 0.001897 | [-4.997e-06, 0.005697] | 12 |
| I2H5 | S2 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| I2H5 | S2 - S1c | hol_sum_nonhole | -0.0224 | [-0.1338, 0.0684] | 12 |
| I2H5 | S3 - S2 | hol_p99 | 0 | [0, 0] | 12 |
| I2H5 | S3 - S2 | max_ep_spend | 0 | [0, 0] | 12 |
| I2H5 | S3 - S2 | hol_sum_nonhole | 0 | [0, 0] | 12 |
| I2H5 | S3 - S1c | hol_p99 | 0.001897 | [-4.997e-06, 0.005697] | 12 |
| I2H5 | S3 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| I2H5 | S3 - S1c | hol_sum_nonhole | -0.0224 | [-0.1338, 0.0684] | 12 |
| GH5 | S2 - S1c | hol_p99 | 0.01866 | [0, 0.0504] | 12 |
| GH5 | S2 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| GH5 | S2 - S1c | hol_sum_nonhole | 0.2315 | [0, 0.5752] | 12 |
| GH5 | S3 - S2 | hol_p99 | 0 | [0, 0] | 12 |
| GH5 | S3 - S2 | max_ep_spend | 0 | [0, 0] | 12 |
| GH5 | S3 - S2 | hol_sum_nonhole | 0 | [0, 0] | 12 |
| GH5 | S3 - S1c | hol_p99 | 0.01866 | [0, 0.0504] | 12 |
| GH5 | S3 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| GH5 | S3 - S1c | hol_sum_nonhole | 0.2315 | [0, 0.5752] | 12 |
| B10H5 | S2 - S1c | hol_p99 | 0 | [0, 0] | 12 |
| B10H5 | S2 - S1c | max_ep_spend | -190.3 | [-191.2, -189.5] | 12 |
| B10H5 | S2 - S1c | hol_sum_nonhole | 0.002582 | [0.0008353, 0.004429] | 12 |
| B10H5 | S3 - S2 | hol_p99 | 0 | [0, 0] | 12 |
| B10H5 | S3 - S2 | max_ep_spend | 0 | [0, 0] | 12 |
| B10H5 | S3 - S2 | hol_sum_nonhole | 0 | [0, 0] | 12 |
| B10H5 | S3 - S1c | hol_p99 | 0 | [0, 0] | 12 |
| B10H5 | S3 - S1c | max_ep_spend | -190.3 | [-191.2, -189.5] | 12 |
| B10H5 | S3 - S1c | hol_sum_nonhole | 0.002582 | [0.0008353, 0.004429] | 12 |
| I1H5f | S2 - S1c | hol_p99 | 0 | [0, 0] | 12 |
| I1H5f | S2 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| I1H5f | S2 - S1c | hol_sum_nonhole | 0 | [0, 0] | 12 |
| I1H5f | S3 - S2 | hol_p99 | 0 | [0, 0] | 12 |
| I1H5f | S3 - S2 | max_ep_spend | 0 | [0, 0] | 12 |
| I1H5f | S3 - S2 | hol_sum_nonhole | 0 | [0, 0] | 12 |
| I1H5f | S3 - S1c | hol_p99 | 0 | [0, 0] | 12 |
| I1H5f | S3 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| I1H5f | S3 - S1c | hol_sum_nonhole | 0 | [0, 0] | 12 |
| I2H5f | S2 - S1c | hol_p99 | -0.2163 | [-0.673, 0.07877] | 12 |
| I2H5f | S2 - S1c | max_ep_spend | -0.4167 | [-1.75, 0.5] | 12 |
| I2H5f | S2 - S1c | hol_sum_nonhole | -2.723 | [-7.619, 0.7102] | 12 |
| I2H5f | S3 - S2 | hol_p99 | 0 | [0, 0] | 12 |
| I2H5f | S3 - S2 | max_ep_spend | 0 | [0, 0] | 12 |
| I2H5f | S3 - S2 | hol_sum_nonhole | 0 | [0, 0] | 12 |
| I2H5f | S3 - S1c | hol_p99 | -0.2163 | [-0.673, 0.07877] | 12 |
| I2H5f | S3 - S1c | max_ep_spend | -0.4167 | [-1.75, 0.5] | 12 |
| I2H5f | S3 - S1c | hol_sum_nonhole | -2.723 | [-7.619, 0.7102] | 12 |
| GH5f | S2 - S1c | hol_p99 | -0.05297 | [-0.385, 0.3186] | 12 |
| GH5f | S2 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| GH5f | S2 - S1c | hol_sum_nonhole | -0.008427 | [-1.986, 2.303] | 12 |
| GH5f | S3 - S2 | hol_p99 | -0.1054 | [-0.3163, 0] | 12 |
| GH5f | S3 - S2 | max_ep_spend | 0 | [0, 0] | 12 |
| GH5f | S3 - S2 | hol_sum_nonhole | -0.3879 | [-1.164, 0] | 12 |
| GH5f | S3 - S1c | hol_p99 | -0.1584 | [-0.4337, 0.1246] | 12 |
| GH5f | S3 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| GH5f | S3 - S1c | hol_sum_nonhole | -0.3964 | [-2.07, 1.453] | 12 |
| I1f | S2 - S1c | hol_p99 | 0 | [0, 0] | 12 |
| I1f | S2 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| I1f | S2 - S1c | hol_sum_nonhole | 0 | [0, 0] | 12 |
| I2f | S2 - S1c | hol_p99 | -0.105 | [-0.3004, 0.0519] | 12 |
| I2f | S2 - S1c | max_ep_spend | 4.167 | [-0.75, 9.917] | 12 |
| I2f | S2 - S1c | hol_sum_nonhole | -1.492 | [-3.546, 0.1424] | 12 |
| Gf | S2 - S1c | hol_p99 | -0.1549 | [-0.4923, 0.04755] | 12 |
| Gf | S2 - S1c | max_ep_spend | 0.6667 | [-0.5, 2] | 12 |
| Gf | S2 - S1c | hol_sum_nonhole | -0.7204 | [-2.928, 1.394] | 12 |
| B10n2s | S2 - S1c | hol_p99 | 0 | [0, 0] | 12 |
| B10n2s | S2 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| B10n2s | S2 - S1c | hol_sum_nonhole | 0 | [0, 0] | 12 |
| B10n2s | S3 - S2 | hol_p99 | 0 | [0, 0] | 12 |
| B10n2s | S3 - S2 | max_ep_spend | 0 | [0, 0] | 12 |
| B10n2s | S3 - S2 | hol_sum_nonhole | 0 | [0, 0] | 12 |
| B10n2s | S3 - S1c | hol_p99 | 0 | [0, 0] | 12 |
| B10n2s | S3 - S1c | max_ep_spend | 0 | [0, 0] | 12 |
| B10n2s | S3 - S1c | hol_sum_nonhole | 0 | [0, 0] | 12 |
| B10n2f | S2 - S1c | hol_p99 | 0.02494 | [0.02315, 0.02668] | 12 |
| B10n2f | S2 - S1c | max_ep_spend | -95 | [-98.25, -91.42] | 12 |
| B10n2f | S2 - S1c | hol_sum_nonhole | -0.0008563 | [-0.001758, -0.0001572] | 12 |
| B10n2f | S3 - S2 | hol_p99 | 0 | [0, 0] | 12 |
| B10n2f | S3 - S2 | max_ep_spend | 0 | [0, 0] | 12 |
| B10n2f | S3 - S2 | hol_sum_nonhole | 0 | [0, 0] | 12 |
| B10n2f | S3 - S1c | hol_p99 | 0.02494 | [0.02315, 0.02668] | 12 |
| B10n2f | S3 - S1c | max_ep_spend | -95 | [-98.25, -91.42] | 12 |
| B10n2f | S3 - S1c | hol_sum_nonhole | -0.0008563 | [-0.001758, -0.0001572] | 12 |
| B10n2a | S2 - S1c | hol_p99 | 0.02606 | [0.02487, 0.02726] | 12 |
| B10n2a | S2 - S1c | max_ep_spend | -190.5 | [-191.7, -189.4] | 12 |
| B10n2a | S2 - S1c | hol_sum_nonhole | 0.007405 | [-0.002098, 0.02051] | 12 |
| B10n2a | S3 - S2 | hol_p99 | 0 | [0, 0] | 12 |
| B10n2a | S3 - S2 | max_ep_spend | 0 | [0, 0] | 12 |
| B10n2a | S3 - S2 | hol_sum_nonhole | 0 | [0, 0] | 12 |
| B10n2a | S3 - S1c | hol_p99 | 0.02606 | [0.02487, 0.02726] | 12 |
| B10n2a | S3 - S1c | max_ep_spend | -190.5 | [-191.7, -189.4] | 12 |
| B10n2a | S3 - S1c | hol_sum_nonhole | 0.007405 | [-0.002098, 0.02051] | 12 |

run-atomic: better in [('I2', 'hol_p99'), ('B10', 'max_ep_spend'), ('B10H5', 'max_ep_spend'), ('B10n2f', 'max_ep_spend'), ('B10n2a', 'max_ep_spend')] ; worse in [('B10n2f', 'hol_p99'), ('B10n2a', 'hol_p99')]
T_rho: better in [('I2', 'hol_p99'), ('I2', 'max_ep_spend'), ('G', 'max_ep_spend')] ; worse in [('G', 'hol_p99')]

### LP / MILP

| config | seeds | LP s (mean) | LP hol | S0 hol | fractional x (mean) | MILP hol (3 seeds) | MILP-LP gap |
|---|---|---|---|---|---|---|---|
| I1 | 12 | 1.9 | 0.32 | 581.78 | 1.8 | 0.37 | -0.000 |
| I2 | 6 | 56.8 | 729.87 | 2383.58 | 4426.3 | 1299.57 | 568.143 |
| G | 6 | 27.1 | 133.83 | 698.53 | 4366.7 | 296.65 | 171.417 |
| B10 | 12 | 0.5 | 0.00 | 0.77 | 1361.4 | 0.01 | 0.006 |
| B60 | 12 | 0.5 | -0.00 | 0.65 | 0.0 | -0.00 | -0.000 |
| I1H5 | 12 | 2.5 | 113.28 | 581.78 | 0.8 | 113.29 | -0.000 |
| I2H5 | 3 | 10.0 | 1024.95 | 2426.25 | 2591.0 | 1276.24 | 251.293 |
| GH5 | 3 | 7.1 | 248.93 | 682.87 | 2956.0 | 338.59 | 89.662 |
| B10H5 | 6 | 0.1 | 0.13 | 0.77 | 0.0 | 0.13 | -0.000 |
| B10n2s | 6 | 3.1 | 1993.53 | 1995.05 | 0.0 | 1991.35 | 0.000 |
| B10n2f | 6 | 3.2 | 1990.66 | 2039.84 | 0.0 | 1988.47 | -0.000 |
| B10n2a | 6 | 2.9 | 1958.12 | 1958.12 | 0.0 | 1955.95 | 0.000 |

### Wall time per run (s, mean)

- I1: 2.5
- I2: 2.8
- G: 2.8
- B10: 2.4
- B60: 8.1
- I1H5: 2.3
- I2H5: 2.8
- GH5: 2.8
- B10H5: 2.5
- I1H5f: 2.3
- I2H5f: 2.6
- GH5f: 2.6
- I1f: 2.4
- I2f: 2.6
- Gf: 2.6
- B10n2s: 4.4
- B10n2f: 4.4
- B10n2a: 4.4
