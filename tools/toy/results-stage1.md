# Toy simulation, stage 1 (sender rules S0, S1, S1c, S2, S3 and the LP bound)

Spec: `docs/derivations/skip-rule-efficiency.md` §6 (458e3c53). Code: `tools/toy/` (`toy.py`, `lp.py`, `batch.py`, `summarize.py`, `test_toy.py`). Not U, not RX. The toy fixes no constant and none of these numbers is engine provenance.

Run set: 12 seeds (spec: 20) x 5 candidates x 15 configurations, one process, about 1 h 10 min. Base = N=1, RTT 10 ms, jitter 1 ms FIFO, H = H§8 (= 0 at N=1 FIFO), rho 0.99, 20 s (B60: 70 s). One run is about 2.5 s (B60 8 s, N=2 4.4 s) plus the LP solve (I1 1.9 s, B10 0.5 s, I2 57 s, G 27 s), so the 15 s fallback was not triggered. Seeds were cut to 12 only because the LP on the count-bound cells (I2, G) costs 30-60 s and the MILP up to its 90 s limit: LP is on 12 seeds for I1, B10, B60, I1H5; 6 for I2, G, B10H5 and the N=2 cells; 3 for I2H5, GH5. MILP is on seeds 0-2 only. Extra configurations beyond the five P5 cells: `*H5` (H = 5 ms, to make F1 visible, since H§8 = 0 at N=1), `*H5f` (floor as a separate datagram, see below), and three N=2 outage variants.

## Checks

- Unit tests (11, `python -m unittest test_toy` in `tools/toy`): determinism and CRN; P1 at every pass (with a positive control that trips it); P2 (rho = 1 and 1 - 1e-7: sender traces of S1, S1c, S2, S3 equal S0's); LP <= every candidate's HOL (I2, G, H = 0 and 5 ms); MILP == brute force on 40 tiny paths; LP == brute force when the count is slack; P4a.
- **LP is not equal to brute force when the count binds** (15 of 40 tiny cases have LP < brute force, e.g. 0.0680 against 0.0702): HOL is convex in pass time, so the LP splits a unit across holes. This deviates from the brief ("LP equals brute force") and was expected; MILP equals brute force. On the real cells the LP-MILP gap is large where the count binds (I2: LP 730 against MILP incumbent 1300; G: 134 against 297; the MILP is a 90 s incumbent, so an upper bound on the optimum) and zero elsewhere. The LP is a weak bound at I2 and G.
- P1: 0 violations in every run (column "P1 viol").
- P4a: S1 - S0 isolated-hole stall median = H +- 1 ms (test, H = 5 ms). P4b: S1c p50 = S0 p50 (0.00 ms), and G/S 0.00011 against the bound 1.5 eps h = 0.00019. P4d: see finding 7. P4c and P4e belong to U and are not applicable.

## Readings

**Run-atomic (S2 against S1c): no HOL gain anywhere; the strict P5 bar fails; recommend DROP.**
- Largest-episode spend falls where a run is long: B10 -191 [-192, -191] units (942 to 750), B10H5 -190, B10n2a -190, B10n2f -95 [-98, -91] (12 seeds each). Elsewhere identical (I1, G, B60) or within CI.
- HOL p99 never improves by more than 0.01 ms (I2: -0.008 ms [-0.022, 0.000]); it worsens by +0.025 ms [0.023, 0.027] at B10n2f and +0.026 ms [0.025, 0.027] at B10n2a: a CI excluding 0, but a negligible size. The strict reading of "worsens none by more than the CI" drops it; a size-aware reading would keep it for the spend reduction, which is bank preserved in outages at no latency gain. The reviewer should rule.
- Section 3.3's prediction "S2 = S1c at N=1 blackouts" does not hold exactly: S2 drains 20 % less (it declines a sweep piece that does not fit whole). It still drains the bank in pieces, as predicted.
- Cells that could discriminate: only B10, B60 and the N=2 outages. B60 shows S2 = S1c (bank 2000 > run: the same 1971 units are spent either way). I1, I2 and G have short runs.

**T_rho (S3 against S2): drop.**
- Inert where predicted: at I1 it sits at D (D-bind), and S3 = S2 at B10, B60, I1H5, I2H5 (identical, no CI).
- It acts only at I2 and G (h above 1 - rho): I2 p99 -3.41 ms [-3.74, -3.08], largest-episode spend -48, but repair sends double (0.030 to 0.060 per delivered) and total HOL +21 packet-seconds; **G p99 +0.37 ms [+0.32, +0.42]**, spend -35, total HOL +39. B60: zero difference. The pre-registered bar (p99 falls beyond CI in G or B60, none worsens) fails: G worsens and B60 does not move. With the floor as a separate datagram (GH5f) S3 - S2 is -0.11 ms [-0.32, 0.00], still no gain beyond CI.
- Gauges: T = D at every I1 evaluation; the `D-fail` column counts T_rho gate refusals (I2 9 937, G 4 455 per run, mean).

**Findings that change how the stage reads**
1. **D is degenerate at N=1** (spec prediction confirmed): `D-fail` = 0 for S1 to S2 everywhere at N=1; the age gate never refuses.
2. **The last copy costs the tail when the floor shares its datagram.** In the literal model (floor in the copy's datagram, same fate) S1c at H = 5 ms has HOL p99 20.5 ms against S0's 14.4 ms at I1H5 (G: 14.1 against 19.7, I2: 23.8 against 33.4 are better). Cause: a lost copy loses the floor too, and the floor refresh costs an extra round (resolution up to about 40 ms against S0's 24 ms). With the floor as an independent datagram (`*H5f`) S1c is 12.0 ms at I1 (better than S0's 14.4), 18.2 at I2 and 15.9 at G (S0: 19.7). The spec's "+H if the copy fails" assumes the floor survives the copy's loss. This needs an answer: are copy loss and floor loss one event under FEC residual loss?
3. **S1 as written is slower than S0** by H at isolated holes (I1H5: HOL p50 2.6 ms against 0, p99 21.4 against 14.4): F1 confirmed; it also spends the allowance (0.49 % of S_src at I1).
4. **The cumulative count does not bind at I1** (S1c spends 0.011 % = eps h), binds at I2 (S1 uses exactly 1.000 %, 15 000 refusals per run) and in G bursts.
5. **Outage cells carry almost no HOL signal** (B10, B60 p99 = 0.00 ms): on a dead link nothing arrives, so no packet waits; the damage is the late holes, whose own delay the metric excludes by spec. Only spend can rank run-atomic in outages.
6. The source pause fires at N=1 B10 (max outstanding 2 048, one pause per run): 2 000 outage seqs plus one RTT of flight exceed B = 2 048.
7. **P4d at N=2 as written is not reproducible.** "S1 spends > 0 for a gain within CI of 0 and S2 spends 0" holds only for a slow-path outage with phi_slow = 0.5 (B10n2s): S1 spends 595, S2 spends 0, but S1c also spends 0 there (copies ride the live fast path), so the zero comes from the last copy, not run-atomic. Fast-path outage (B10n2f): S1 595, S1c 464, S2 369. Both paths (B10n2a) behaves as N=1 (S2 750). No setting in the sweep gives a closed run longer than the bank: with phi_slow <= 0.5 consecutive missing seqs are short, and a long closed run appears only after data resumes, when the sweep has already drained the bank. N=1 side as predicted: S2 drains (750) like S1 (942).
8. LP `gap`: 0.99 at I1, 1.00 at B10, 0.85 (G) and 0.59 (I2) for S1c. It is near 1 where the LP's free pass at detect + H (no tell latency, about an RTT) is unreachable, so at H = 0 it bounds rather than ranks.

## Ambiguities in the spec and how they were settled

- Loss cells: v1's table gives I2 = 3 %/10 %, G = Gilbert-Elliott burst 8, bad-state loss 0.5, mean 2.5 %; I1 = h 0.5 %, eps 2.5 % (worked example; its 42-67 % overlap figure matches). Blackout cells carry no other loss. B at N=2 is "on one path" (v1), path unnamed, so slow-path, fast-path and both-path variants were run.
- D = RTprop/2 fixed (no delta dependence in stage 1); tail sweep period clamp(2 SRTT, 25, 100) ms (`hole_nack_refresh`); the hole law (9/8 x owd of the seq's path) applies only with two paths, as the spec says; the 9/8, the floor-refresh interval (RTT + 2 ticks + H + jitter) and the jitter law (uniform 0..J) are toy choices.
- Floor semantics: F = lowest seq that is neither verdicted nor known received; a floor is sent only while a verdict is pending below it (so rho = 1 traces equal S0's). It rides every copy of a verdict pass (same fate) or goes alone when no copy is sent; `*H5f` runs it as a separate datagram.
- Kaplan-Meier input: the sender must tell a pass from a fill when a verdicted hole leaves a report, which the (next_expected, G_rx) pair does not give per seq, so the toy report carries the seqs passed since the previous report (observer channel; ages use the report's generation time).
- Stall at N=2 with phi_slow > 0: skew keeps a hole outstanding at all times, so the whole run is one stall episode (p99 about the run length); the stall p99 column is meaningless there. HOL p99 is unaffected.
- HOL is summed over non-hole packets only (same set in every candidate and in the LP); hole packets' own delay is excluded as in the spec. The LP books the prefix budget at S_src(p_s) per the spec text, which can be earlier than a candidate's verdict; no case with LP above a candidate occurred.
- Isolated holes: no other lost original within 300 seqs (policy independent, so the same set across candidates).
- Not done: S-hat against S0's true survival, R0, 10 s windowing of the optima (the full 20 s was solved), jitter/delta/rho sweeps, U.

## Tables (12 seeds; gap = (HOL_cand - LP)/(HOL_S0 - LP); G/S is the final G_rx over S_src; time columns are means over seeds)

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
- B10n2s: 4.4
- B10n2f: 4.4
- B10n2a: 4.4
