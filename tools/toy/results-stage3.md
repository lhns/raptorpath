# Offline simulation, stage 3: lifetime T, delay price, order hold H (one path)

Spec: `docs/derivations/three-knobs.md` §9 (registration), §11 (Amendment 1), laws in §2; on branch `github/three-knobs`. Code: `tools/toy/` (`toy2.py` extended, `batch3.py`, `summarize3.py`, `report_s3.py`, `report_o.py`, `diag3.py`, `test_toy2.py`); built on `toy-sim-stage2` `c0f33f8e`. The stage 1 and stage 2 code paths are unchanged. The offline simulation fixes no constant of the engine, and none of these numbers is engine provenance. 12 seeds per configuration (seeds 0 to 11, clock phases drawn per seed as in stage 2), one path, all runs deterministic.

## In plain words

- **The two rules the new design always keeps hold exactly.** No datagram carrying data is ever sent for a unit that is T or more old (L1, including units still waiting to be sent), and no resend is sent that cannot arrive before T (L2). Both are absolute tests that also prove the mechanism ran (units expired in the intake; resends refused).
- **The new design is a limit of today's receiver, exactly.** At T = infinity (and at T = 1e6 s) with the order hold at infinity it is bit-identical to today's receiver at rho = 1 on 20 readouts and on the sender trace, in all 6 cells and 12 seeds (P1). At H = infinity it equals the strict-order receiver on every readout, at H = 0 nothing is ever held (P2).
- **A call (T = 0.4 s, H = 0) against today's Realtime receiver:** in the call cells it delivers 98.2 % (5 ms path) and 98.0 % (50 ms path) of the frames at all, against 99.5 % and 99.1 % for today's receiver. All of the difference is the 1.5 s outage: today's receiver fills those frames seconds late. In exchange the mean wait beyond the loss-free arrival (w) falls from 17.2 to 2.8 ms and from 47.8 to 14.6 ms, the 99th percentile from 765 to 34 ms and from 959 to 214 ms, total consumer stall from 0.52 s to 0 and from 19.3 s to 0, and repair traffic falls from 0.65 to 0.36 and from 0.31 to 0.20 resends per delivered frame. Outside the outage window it delivers 100.0 % and 99.97 % within 0.4 s, against 99.7 % and 99.2 %. Cost: 3.0 % (5 ms path) and 10.4 % (50 ms path) of delivered frames arrive out of order, against 2.4 % and 6.3 % today.
- **The outage cells are where the design gives up on purpose.** At T = 0.4 s it delivers 97.1 % of the object in the end-outage cells (the 347 frames sent into the outage are given up) and finishes 0.26 s sooner at 50 ms (1.58 s against 1.83 s), but only 39 to 40 % in the mid-stream outage, against 99 to 100 % for every receiver that never gives up (Q10 in the spec).
- **Where the registration was wrong, it is reported as wrong** (P4, P5 bullets 3 and 4, P10 at 50 ms, P11), with the cause where it was found. Nothing was tuned.

## Readings (P1 to P11)

| | reading | key numbers |
|---|---|---|
| P1 | **PASS** | 144 of 144 (cell, seed, arm) identities float-equal on the 20 keys; sender traces identical (seeds 0 to 2, 6 cells, T = inf and T = 1e6) |
| P2 | **PASS** | RH(inf) = R1 on every readout and every gauge, 72 of 72; RH(0): hol_sum_nonhole = 0 and v_p50 = 0 exactly at T = 0.4, 2 and inf, 72 of 72 |
| P3 | **PASS as registered, with no demonstrated power (see P4)** | 0 shown, 0 undetermined of 144 K and P statistics. In the outage cells most readouts do not move at all over the nudges (K = P = 0 exactly), so the pass there is a flat line, not a measured curvature |
| P4 | **FAIL** | E-L10: K and P of share_T are exactly 0 (equivalent); E-I2: K = 0.0032 [-0.0012, 0.0075], P = 0.0026 [-0.0002, 0.0058] against m = 0.00133, undetermined. Neither reads shown. Cause: the dead-tail frames are delivered at age 1.53 s (E-I2) and 1.80 s (E-L10, range 1.69 to 1.89) under C3, so T = 1.5 s sits on the step only at 5 ms |
| P5 | **monotone PASS; below the first round PASS; the rise interval and the +30 ms shift FAIL** | see the P5 block: below the first round repair per delivered is exactly 0 in every seed and share_T = 0.8771 (L10, predicted 0.8775) and 0.9464 (I2, predicted 0.9458); the share rises in steps at about T* and moves with the path (mid-level crossing 0.1738 s at 50 ms and 0.2029 s at 60 ms, a shift of 29 ms, not registered), but the registered readings of the rise give 0 to 16 ms |
| P6 | **PASS** | T = 2 s against T = inf: share_any, v_mean, tail_time equivalent in all 4 outage cells |
| P7 | **PASS** | E: share_any 0.9715 (I2) and 0.9714 (L10) against 0.9714; tail time in E-L10 258 ms shorter than C3 (shown, bar 85 ms), E-I2 37 ms shorter (equivalent); M: 0.3999 and 0.3908 (bar 0.39 to 0.41); intake_expired 0 for S2, 5 242 (I2) and 5 352 (L10) for S2p. S2p expires nothing in the E cells, so the "above 4 000" bullet is read on the M cells only |
| P8 | **bullets 1 to 4 PASS; bullet 5 passes on its number, its stated reason is not seen** | outside the outage window share_by_0p4 S2 minus C1 = +0.0027 (I2), +0.0081 (L10), shown larger; v_mean S2 minus C1 = -14.4 ms and -33.2 ms (shown smaller); ooo H = 0 minus H = inf = +0.030 and +0.104 (shown larger); stall_total at H = 0 is 0 in all seeds; stall_p99 S2 H = inf is at most 0.213 s (I2) and 0.609 s (L10), but C2's stall_p99 is 0.039 s and 0.398 s on average, not at least the outage: the longest single stall is 1.53 s and 1.92 s for both C2 and S2 at H = inf |
| P9 | **PASS, one reading undetermined** | share_T equivalent in 11 of 12 (cell, H) rows, E-L10 at H = inf undetermined (+0.0017 [0.0003, 0.0041] against m = 0.0013: the copy resolves the hole ahead of the floor pass and the units queued behind it get through sooner); share_any not shown smaller anywhere; repair per delivered minus verdicts per delivered has residual below 6e-4 in every cell and seed (the readings are "equivalent" even at the tight outage-cell bars; the call-cell bar of 0.19 is vacuous, the residuals are the evidence) |
| P10 | **PASS in E and M; FAIL at K-L10 T = 0.10 as stated, a denominator effect** | E and M: floors per delivered below C2's by 0.018 to 0.039 (shown smaller); K-L10 H = 0: 0.228 at T = 0.10 and 0.1755 at T = 0.14 per delivered frame against the registered 0.2. Per frame handed over it is 0.2001 and 0.154: the registered number divides by handed over, the readout by delivered (87.7 % at that T) |
| P11 | **FAIL, and the cell does not test overload for this arm** | S2 (T = 0.4, H = 0) share_T = 0.975 (0.9747 to 0.9751 over seeds) against < 0.2. See the O-I2 block |

## Worker decisions (every reading of an ambiguity or a choice not fixed by the spec)

1. **`share_T` is a named readout function.** `within_lifetime(T, age)` in `toy2.py` is the third place T is compared, listed in the deny test's `EXEMPT` next to the two law functions. Spec §9.2 lists only the two law functions, but the readout needs `age <= T`; an inline comparison would fail the deny test and `np.less_equal` would hide it from it.
2. **A local variable called `T` in `summary` was renamed `Tr`** (the unpacking of a hole row). With `T` in `DIALS` the old name fails the deny scan; values are unchanged.
3. **The source-done cap `u_next >= units_total` applies to object mode only.** In streaming mode stage 1 emits one frame more than `int(dur*rate)` at the float edge (t = 6.0 s at 10 000 per second), so applying the cap there broke the stage 1 bridge tests; streaming mode keeps its `nxt >= dur` test (spec text: "streaming mode, unchanged"). The first full suite run caught this.
4. **The verdict timer is pushed at `max(t, t_h + T - owd)`.** After a pause the first unit sent can be older than `T - owd`, which would put the event in the past and run time backwards. At T = inf the value is +inf, as specified.
5. **`advance(t)` is called when the source finishes because the last units expired in the intake** (the completion test lives in `advance`; otherwise a run could end without setting `t_front`). Only reachable in object mode with expiry; no stage 1 or 2 run touches it.
6. **The pause is re-checked after every verdict**, in `act` (report-driven) and in the timer, through one `maybe_resume` also used by `on_report`.
7. **`l2_fail` counts verdicts taken, in both places**; `rel_by_floor` counts holes released by a floor pass while still unreleased.
8. **Tests 3 and 4 of §9.2 needed their own "mechanism ran" witnesses.** L1's test runs S2p (verdicted seqs stay in the outstanding count) with `pause_B = 200` and asserts `intake_expired > 0`; with S2 almost nothing expires (P7), so asserting expiry on S2 would be a false witness. L2's test uses T = 0.05 at 5 ms and T = 0.3 at 50 ms (at T = 0.05 and 50 ms every unit is verdicted, no resend exists) and asserts resends were sent and `l2_fail > 0`.
9. **Noise floors.** `nf_stage3.json` comes from the A/A of the two call cells (S2, T = 0.4, H = 0.02, phase variants 0 and 1). It was written once, then rewritten once after the K cells had run (but before any K reading was made) to add the three outside-the-window share keys, which P8 needs; no other value changed. One of 38 self-check readings read "shown" (K-L10 stall_total, -7.0 ms [-11.6, -2.6] against m = 1 ms); the frozen table was kept, and a replication with two fresh phase draws (variants 2 and 3) reads 0 of 38 shown, as in stage 2. The A/A is a two-arm A/A, so for K and P statistics NF_K = NF_P = the readout's own floor (the registration has no five-point A/A). For E and M cells the floor is stage 2's Part T floor where it has the readout (v_mean, v_p99, ooo, repair, tail_time), and the call-cell floor otherwise (shares, w, floors).
10. **Bars.** `T_bar` follows stage 2's table with C3 as reference: means max(5 % of C3, 0.01 ms), percentiles and tails 1 ms, ooo max(10 % of C3, 1/delivered), repair and floors max(5 % of C3, 1/delivered), shares max(1/units, 5 % of 1 - share_C3). w mean and w percentiles are treated like v. `share_T` is barred against C3's `share_any` (C3's own T is infinite).
11. **P5's "rise to 90 % of the T = 0.4 value" has two readings, both reported.** Reading A: 90 % of the T = 0.4 level (grid point and linear interpolation). Reading B: 90 % of the rise above the lowest-T level. At K-I2 reading A is degenerate (the lowest-T level 0.946 is already above 0.9 x 0.9815 = 0.883). A third, descriptive reading (mid-level crossing of the first step) is labelled "not registered".
12. **P8, P10 and P9 readings.** P8's outside-window shares come from the per-unit record (units handed over before 19.6 s or from 21.5 s on; the outage is 20.0 to 21.5 s). "C2's stall" in P8 bullet 5 was read as C2's stall_p99, with the longest single episode reported next to it from `diag3.py`. P10's K-L10 reading is on seed means (and the max over seeds is shown). P9's "higher by verdicts/delivered" is the per-seed residual `repair(S2c) - repair(S2) - verdicts/delivered` read against the bar.
13. **P7 and the S2 arm.** "S2 at T = 0.4" in E and M readings means H = infinity (the strict receiver); H = 0 and H = 0.02 are in the tables.
14. **Overload cell capacity.** The offline simulation has no link rate limit; the only throttle is the pause at B = 2048 outstanding. I took "the link's capacity" to be the sustained send rate that pause allows: 166 640 units/s for C3 at cell I2 (measured at an offered rate of 10^7 per second over frames 100 000 to 700 000). The offered load is 200 000 per second (1.2 x) for 10 s, 2 000 000 units. C1 costs about 1 ms per unit (the early-skip receiver rescans its buffer), so each C1 run took about 40 minutes; all 12 ran in parallel.
15. **Runs and storage.** 12 seeds ran as parallel processes over disjoint seed windows; run order was tests, P1/P2 identity arms in all cells, the call-cell A/A (frozen), the call cells, the outage cells, then O-I2. The per-run JSON drops the three long per-tail-seq lists of the stage 2 readouts; `tail_*` fields count seqs from `dead[0]`, which is not meaningful for arms that expire units. `finish` for a run that never completes is counted at the run limit.
16. **Unregistered arms and diagnostics, all labelled where used.** `REP` (A/A variants 2 and 3), `S2p-H0` in O-I2, and `diag3.py` (A/A replication, tail delivery ages, longest stall episodes, resend origin, floors per unit, A/A residuals). None changes a reading.
17. **Bit-identity of stages 1 and 2.** Both existing suites pass unchanged, and 34 stage 2 configurations (every sender and receiver of stages 1 and 2, three cells, streaming and object mode, traces included) give identical output between the stage 2 commit and this one on every key of the old output (checked once with a throwaway script, not committed).

## Things that looked wrong or surprising

1. **The repair noise floor in the call cells (0.189) is the size of the readout.** It comes from phase alignment: at 50 frames per second the 20 ms frame clock and the clamped 100 ms sweep (50 ms RTT) either do or do not put a still-unacknowledged frame at a sweep instant, so a run resends about 0.19 or about 0.39 per delivered frame depending on the draw (K-L10 variant 1, seed 4: 0.392 against 0.210). A/A repair residuals per seed at K-L10: 0.005 0.001 -0.003 -0.004 -0.182 -0.026 0.046 -0.099 0.014 0.039 -0.002 -0.198; at K-I2 they stay within 0.006. This is an effect of stage 2's sweep and affects every arm that shares the seed's phases; it makes "equivalent" on repair per delivered at the call-cell bar vacuous (P3, P9 there). The paired differences in P8 and P9 use common phases.
2. **Two ways to a resend before one round trip.** The early rise of share_T at K-L10 (T = 0.16 and 0.17) comes from the tail sweep, not from gap reports: resends sent exactly at sweep instants at age about 0.104 s (the sweep period is clamped to 100 ms, one round trip at 50 ms), admitted once T exceeds 0.104 + 0.05 = 0.154 s; report-driven resends (median age 0.121 s) first appear at T = 0.18. At seed 0: T = 0.16 and 0.17 have 85 sweep resends and 0 report resends; T = 0.18 has 104 and 203. The registered a1 analysis (§2.2) covers the report path only; the sweep makes an earlier, thinner rise, and that is why the registered shift between 50 and 60 ms path delay (3 x owd for the report path) is not seen by the 90 % readings.
3. **At sustained overload the new design does not overload.** See O-I2.
4. **A strict hold keeps the 1.5 s outage stall at any T.** In the call cells C2 and S2 at H = infinity have the same longest single stall (1.53 s at 5 ms and 1.92 s at 50 ms over the 12 seeds), because floors are lost in the outage as well and the hole is passed at the first floor after it. Only H = 0 removes the stall (0 s in every seed), and H = 0.02 cuts it to 0.99 s (5 ms) and 5.3 s (50 ms) in total.
5. **Mid-stream outage cell, repair per delivered.** At T = 0.4 it reads 14.0 (I2) against 9.4 for C3, but the denominator is the 40 % of the object that is delivered. Resends per run are 68 153 against 114 777 (I2) and 11 241 against 32 238 (L10): fewer, not more.
## Reading detail (generated by `python summarize3.py ident|K|EM|ALL|O`, then `tables`)

### P1 and P2: identities

#### P1: S2 at T = inf and T = 1e6 (H = inf) against C3 (S1c + R1, rho = 1), per cell and seed, float-equal on 20 keys

- E-I2 G-Tinf-Hinf: 12/12 seeds identical
- E-I2 S2L: 12/12 seeds identical
- E-L10 G-Tinf-Hinf: 12/12 seeds identical
- E-L10 S2L: 12/12 seeds identical
- M-I2 G-Tinf-Hinf: 12/12 seeds identical
- M-I2 S2L: 12/12 seeds identical
- M-L10 G-Tinf-Hinf: 12/12 seeds identical
- M-L10 S2L: 12/12 seeds identical
- K-I2 G-Tinf-Hinf: 12/12 seeds identical
- K-I2 S2L: 12/12 seeds identical
- K-L10 G-Tinf-Hinf: 12/12 seeds identical
- K-L10 S2L: 12/12 seeds identical

#### P1 trace identity (sender trace, seeds 0-2; reruns with want_trace)

- E-I2: traces identical (seeds 0-2, 33161 entries)
- E-L10: traces identical (seeds 0-2, 19100 entries)
- M-I2: traces identical (seeds 0-2, 127251 entries)
- M-L10: traces identical (seeds 0-2, 43415 entries)
- K-I2: traces identical (seeds 0-2, 5341 entries)
- K-L10: traces identical (seeds 0-2, 4026 entries)

#### P2: the two ends of L6

- E-I2: RH(inf) = R1 on every readout and gauge in 12/12 seeds; RH(0) has hol_sum_nonhole = 0 and v_p50 = 0 at T = 0.4, 2 and inf in 12/12
- E-L10: RH(inf) = R1 on every readout and gauge in 12/12 seeds; RH(0) has hol_sum_nonhole = 0 and v_p50 = 0 at T = 0.4, 2 and inf in 12/12
- M-I2: RH(inf) = R1 on every readout and gauge in 12/12 seeds; RH(0) has hol_sum_nonhole = 0 and v_p50 = 0 at T = 0.4, 2 and inf in 12/12
- M-L10: RH(inf) = R1 on every readout and gauge in 12/12 seeds; RH(0) has hol_sum_nonhole = 0 and v_p50 = 0 at T = 0.4, 2 and inf in 12/12
- K-I2: RH(inf) = R1 on every readout and gauge in 12/12 seeds; RH(0) has hol_sum_nonhole = 0 and v_p50 = 0 at T = 0.4, 2 and inf in 12/12
- K-L10: RH(inf) = R1 on every readout and gauge in 12/12 seeds; RH(0) has hol_sum_nonhole = 0 and v_p50 = 0 at T = 0.4, 2 and inf in 12/12

P1 seed-level diffs: 0 of 144; trace diffs: 0; P2 diffs: 0

### A/A self-check (call cells, S2 T = 0.4, H = 0.02, phase variants 0 and 1, 12 seeds)

38 readings: 26 equivalent, 11 undetermined, 1 shown.

- SHOWN: K-L10-stall_total: -6.991 [-11.63, -2.648] vs m=1: shown smaller

Frozen noise floors NF (95th percentile over seeds of |A - A'|, largest over the two call cells; ms for times):

| readout | NF |
|---|---|
| share_any | 0.001333 |
| share_T | 0.001333 |
| share_by_0p4 | 0.001333 |
| share_by_2 | 0.001333 |
| v_mean | 0.2871 |
| v_p50 | 0 |
| v_p99 | 15.45 |
| w_mean | 0.287 |
| w_p50 | 0.001638 |
| w_p99 | 15.45 |
| ooo_per_delivered | 0.003285 |
| repair_per_delivered | 0.1888 |
| floors_per_delivered | 0.001712 |
| stall_total | 80.93 |
| finish | 1.151 |
| tail_time | 1.151 |
| win_out_any | 0.0003442 |
| win_out_by0p4 | 0.0003442 |
| win_out_by2 | 0.0003442 |

### P5: the round boundary is measured, not declared (K cells, H = 0)

**K-L10** (owd 50 ms; m = 0.001333)

| T | share_T | repair per delivered | floors per delivered |
|---|---|---|---|
| 0.1 | 0.8771 | 0.0000 | 0.2281 |
| 0.14 | 0.8771 | 0.0000 | 0.1755 |
| 0.16 | 0.8882 | 0.0241 | 0.1467 |
| 0.17 | 0.8950 | 0.0306 | 0.1445 |
| 0.18 | 0.9602 | 0.1077 | 0.1176 |
| 0.2 | 0.9663 | 0.1183 | 0.1080 |
| 0.22 | 0.9673 | 0.1237 | 0.0965 |
| 0.25 | 0.9678 | 0.1299 | 0.0829 |
| 0.3 | 0.9768 | 0.1731 | 0.0223 |
| 0.4 | 0.9794 | 0.2012 | 0.0079 |

- non-decreasing (each adjacent mean difference >= -m): **yes**; adjacent readings: 0.14->0.16 shown larger, 0.16->0.17 shown larger, 0.17->0.18 shown larger, 0.18->0.2 shown larger, 0.25->0.3 shown larger, 0.3->0.4 shown larger
**K-L10b** (owd 60 ms; m = 0.001333)

| T | share_T | repair per delivered | floors per delivered |
|---|---|---|---|
| 0.1 | 0.8771 | 0.0000 | 0.2282 |
| 0.14 | 0.8771 | 0.0000 | 0.2282 |
| 0.16 | 0.8771 | 0.0000 | 0.2283 |
| 0.17 | 0.8882 | 0.1677 | 0.1670 |
| 0.18 | 0.8950 | 0.2235 | 0.1463 |
| 0.2 | 0.9127 | 0.2489 | 0.1336 |
| 0.22 | 0.9633 | 0.2987 | 0.1290 |
| 0.25 | 0.9677 | 0.3102 | 0.1147 |
| 0.3 | 0.9714 | 0.3517 | 0.0808 |
| 0.4 | 0.9790 | 0.4101 | 0.0170 |

- non-decreasing (each adjacent mean difference >= -m): **yes**; adjacent readings: 0.16->0.17 shown larger, 0.17->0.18 shown larger, 0.18->0.2 shown larger, 0.2->0.22 shown larger, 0.22->0.25 shown larger, 0.25->0.3 shown larger, 0.3->0.4 shown larger
**K-I2** (owd 5 ms; m = 0.001333)

| T | share_T | repair per delivered | floors per delivered |
|---|---|---|---|
| 0.01 | 0.9464 | 0.0000 | 0.3571 |
| 0.02 | 0.9464 | 0.0000 | 0.0546 |
| 0.03 | 0.9464 | 0.0000 | 0.0547 |
| 0.035 | 0.9511 | 0.0107 | 0.0492 |
| 0.04 | 0.9724 | 0.0394 | 0.0347 |
| 0.05 | 0.9728 | 0.0499 | 0.0235 |
| 0.1 | 0.9765 | 0.1023 | 0.0193 |

- non-decreasing (each adjacent mean difference >= -m): **yes**; adjacent readings: 0.03->0.035 shown larger, 0.035->0.04 shown larger, 0.05->0.1 shown larger

- K-L10 below the first round (T <= 0.14): repair per delivered exactly 0 for every seed: **yes**; share_T within 0.005 of 0.8775: **yes** (means 0.8771, 0.8771)
- K-I2 below the first round (T <= 0.02): repair per delivered exactly 0 for every seed: **yes**; share_T within 0.005 of 0.9457: **yes** (means 0.9464, 0.9464)

- K-L10: top (T = 0.4) 0.9794, lowest-T level 0.8771. Reading A (90 % of the T = 0.4 value = 0.8814): first sweep point at or above = 0.16, interpolated = 0.1478. Reading B (90 % of the rise = 0.9692): first sweep point = 0.3, interpolated = 0.2574. Largest adjacent jump: +0.0653 between T = 0.17 and 0.18.
- K-L10b: top (T = 0.4) 0.9790, lowest-T level 0.8771. Reading A (90 % of the T = 0.4 value = 0.8811): first sweep point at or above = 0.17, interpolated = 0.1636. Reading B (90 % of the rise = 0.9688): first sweep point = 0.3, interpolated = 0.2648. Largest adjacent jump: +0.0506 between T = 0.2 and 0.22.
- K-I2: top (T = 0.4) 0.9815, lowest-T level 0.9464. Reading A (90 % of the T = 0.4 value = 0.8834): first sweep point at or above = 0.01, interpolated = 0.0100. Reading B (90 % of the rise = 0.9780): first sweep point = None, interpolated = -. Largest adjacent jump: +0.0212 between T = 0.035 and 0.04.
- K-L10, registered interval [0.16, 0.2], hand prediction T* = 0.17 s: reading A grid point 0.16 (in), interpolated 0.1478 (OUT); reading B grid point 0.3 (OUT), interpolated 0.2574 (OUT)
- K-I2, registered interval [0.03, 0.05], hand prediction T* = 0.035 s: reading A grid point 0.01 (OUT), interpolated 0.0100 (OUT); reading B grid point None (OUT), interpolated - (OUT)
- shift K-L10b minus K-L10, reading A grid: **10.0 ms** (registered +30 +- 5): OUTSIDE
- shift K-L10b minus K-L10, reading A interpolated: **15.7 ms** (registered +30 +- 5): OUTSIDE
- shift K-L10b minus K-L10, reading B grid: **0.0 ms** (registered +30 +- 5): OUTSIDE
- shift K-L10b minus K-L10, reading B interpolated: **7.4 ms** (registered +30 +- 5): OUTSIDE

- (not registered, descriptive) K-L10: share_T crosses 0.92 at T = 0.1738 s
- (not registered, descriptive) K-L10b: share_T crosses 0.92 at T = 0.2029 s
- (not registered, descriptive) K-I2: share_T crosses 0.96 at T = 0.0371 s

### P8: the call against today (K cells)

**K-I2**

- outside the outage window (units handed over before 19.6 s or at 21.5 s and after): share_by_0p4 S2 (T=0.4,H=0) 1.0000, C1 0.9973, C2 0.9974, C3 1.0000
- share_by_0p4 outside the window, S2 minus C1 (prediction: >= -m): 0.002697 [0.00241, 0.002983] vs m=0.000344: shown larger
- over the whole run (no sign predicted): share_by_0p4 S2 0.9815, C1 0.9785; share_any S2 0.9815, C1 0.9948
- whole-run share_by_0p4, S2 minus C1: 0.003 [0.002667, 0.003361] vs m=0.00133: shown larger
- v_mean, S2 (H=0) minus C1 (prediction: shown smaller): -14.39 [-14.81, -13.93] vs m=1.04: shown smaller
- w_mean, S2 (H=0) minus C1 (reported): -14.39 [-14.81, -13.93] vs m=1.06: shown smaller
- ooo_per_delivered, H=0 minus H=inf (prediction: shown larger): 0.03 [0.02853, 0.03159] vs m=0.00329: shown larger
- stall_total at H = 0: max over seeds 0 s (prediction <= 1e-9 s): **yes**
- stall_p99 (s): S2 H=inf max over seeds 0.213 (prediction <= 1.6); C2 mean 0.039 (min 0.013; prediction >= the 1.5 s outage)
**K-L10**

- outside the outage window (units handed over before 19.6 s or at 21.5 s and after): share_by_0p4 S2 (T=0.4,H=0) 0.9997, C1 0.9916, C2 0.9923, C3 0.9992
- share_by_0p4 outside the window, S2 minus C1 (prediction: >= -m): 0.008061 [0.007458, 0.008635] vs m=0.000344: shown larger
- over the whole run (no sign predicted): share_by_0p4 S2 0.9794, C1 0.9705; share_any S2 0.9795, C1 0.9905
- whole-run share_by_0p4, S2 minus C1: 0.008889 [0.008222, 0.009528] vs m=0.00133: shown larger
- v_mean, S2 (H=0) minus C1 (prediction: shown smaller): -33.23 [-33.85, -32.53] vs m=3.38: shown smaller
- w_mean, S2 (H=0) minus C1 (reported): -33.23 [-33.85, -32.53] vs m=3.4: shown smaller
- ooo_per_delivered, H=0 minus H=inf (prediction: shown larger): 0.1042 [0.1018, 0.1067] vs m=0.00329: shown larger
- stall_total at H = 0: max over seeds 0 s (prediction <= 1e-9 s): **yes**
- stall_p99 (s): S2 H=inf max over seeds 0.609 (prediction <= 1.6); C2 mean 0.398 (min 0.329; prediction >= the 1.5 s outage)

### P4: positive control (E cells, T = 1.5 x nudges, H = inf, K of share_T)

| T | share_T mean |
|---|---|
| 1.47 | 0.97152 |
| 1.485 | 0.97180 |
| 1.5 | 0.97626 |
| 1.515 | 0.98377 |
| 1.53 | 0.99228 |
- E-I2 K of share_T: 0.00317 [-0.001167, 0.007505] vs m=0.00133: undetermined
- E-I2 P of share_T: 0.00258 [-0.0001683, 0.005753] vs m=0.00133: undetermined
| T | share_T mean |
|---|---|
| 1.47 | 0.97152 |
| 1.485 | 0.97152 |
| 1.5 | 0.97152 |
| 1.515 | 0.97152 |
| 1.53 | 0.97152 |
- E-L10 K of share_T: 0 [0, 0] vs m=0.00133: equivalent
- E-L10 P of share_T: 0 [0, 0] vs m=0.00133: equivalent

### P6: T = 2 s against T = inf (E and M cells, H = inf, paired difference S2 minus C3)

- E-I2 share_any: 0 [0, 0] vs m=0.00133: equivalent
- E-I2 v_mean: 0 [0, 0] vs m=3.03: equivalent
- E-I2 tail_time: 0 [0, 0] vs m=85.1: equivalent
- E-L10 share_any: -6.86e-06 [-2.058e-05, 0] vs m=0.00133: equivalent
- E-L10 v_mean: -0.01172 [-0.03515, 0] vs m=14.3: equivalent
- E-L10 tail_time: 0 [0, 0] vs m=85.1: equivalent
- M-I2 share_any: 0 [0, 0] vs m=0.00133: equivalent
- M-I2 v_mean: 0 [0, 0] vs m=13.1: equivalent
- M-I2 tail_time: 0 [0, 0] vs m=85.1: equivalent
- M-L10 share_any: -6.86e-05 [-0.0001303, -1.372e-05] vs m=0.00133: equivalent
- M-L10 v_mean: -9.903 [-25.54, -0.003623] vs m=42: equivalent
- M-L10 tail_time: -0.1049 [-0.3979, 0.06667] vs m=85.1: equivalent

### P7: T = 0.4 s in the outage cells (H = inf unless said)

- E-I2 share_any (S2 T=0.4, H=inf) = 0.9715 over seeds (min 0.9715, max 0.9715) against 1 - 347/12147 = 0.9714 +- 0.002
- E-I2 tail_time, S2 T=0.4 H=inf minus C3 (ms): -36.66 [-53.61, -23.97] vs m=85.1: equivalent
  - tail time means (ms): S2 1479.9, C3 1516.6, C1 1504.7, C2 1512.1
- E-L10 share_any (S2 T=0.4, H=inf) = 0.9714 over seeds (min 0.9713, max 0.9715) against 1 - 347/12147 = 0.9714 +- 0.002
- E-L10 tail_time, S2 T=0.4 H=inf minus C3 (ms): -258.4 [-316.6, -216.7] vs m=85.1: shown smaller
  - tail time means (ms): S2 1575.6, C3 1834.0, C1 1784.4, C2 1809.0
- M-I2 share_any (S2 T=0.4, H=inf) = 0.3999 (min 0.3996, max 0.4000) against [0.39, 0.41]
- M-L10 share_any (S2 T=0.4, H=inf) = 0.3908 (min 0.3901, max 0.3919) against [0.39, 0.41]
- E-I2 intake_expired: S2 (T=0.4 H=inf) 0.0, S2p 0.0 per run (units = 12147)
  - share_any: S2 0.9715 vs S2p 0.9715
- E-L10 intake_expired: S2 (T=0.4 H=inf) 0.0, S2p 0.0 per run (units = 12147)
  - share_any: S2 0.9714 vs S2p 0.9714
- M-I2 intake_expired: S2 (T=0.4 H=inf) 0.0, S2p 5241.8 per run (units = 12147)
  - share_any: S2 0.3999 vs S2p 0.3999
- M-L10 intake_expired: S2 (T=0.4 H=inf) 0.0, S2p 5352.2 per run (units = 12147)
  - share_any: S2 0.3908 vs S2p 0.3908

### P3: continuity at the named points (K and P of each readout over the five nudge points; both nudge sets, six cells)

Bar m = max(T bar, A/A noise floor of the readout); the stage-3 A/A is a two-arm A/A, so NF_K = NF_P = the readout's floor (worker decision).

| cell | set | statistic | share_any | share_T | v_mean | v_p99 | ooo_per_delivered | repair_per_delivered |
|---|---|---|---|---|---|---|---|---|
| K-I2 | T nudges | K | equivalent (0.000139, m=0.00133) | equivalent (0.000167, m=0.00133) | equivalent (0.0505, m=1.04) | equivalent (0.477, m=15.5) | equivalent (-2.87e-05, m=0.00329) | equivalent (3.31e-05, m=0.189) |
| K-I2 | T nudges | P | equivalent (4.17e-05, m=0.00133) | equivalent (2.78e-05, m=0.00133) | equivalent (0.0163, m=1.04) | equivalent (0.233, m=15.5) | equivalent (-1.43e-05, m=0.00329) | equivalent (-0.000284, m=0.189) |
| K-I2 | H nudges | K | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (1.09e-05, m=1.04) | equivalent (-1.78e-15, m=15.5) | equivalent (0, m=0.00329) | equivalent (0, m=0.189) |
| K-I2 | H nudges | P | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (8.66e-05, m=1.04) | equivalent (5.92e-16, m=15.5) | equivalent (0, m=0.00329) | equivalent (0, m=0.189) |
| K-L10 | T nudges | K | equivalent (-0.00025, m=0.00133) | equivalent (-0.000194, m=0.00133) | equivalent (-0.0831, m=3.38) | equivalent (-5.3, m=15.5) | equivalent (-8.74e-05, m=0.00329) | equivalent (-0.00236, m=0.189) |
| K-L10 | T nudges | P | equivalent (-6.94e-05, m=0.00133) | equivalent (-0.000125, m=0.00133) | equivalent (-0.0223, m=3.38) | equivalent (-2.31, m=15.5) | equivalent (-2.12e-05, m=0.00329) | equivalent (-0.000423, m=0.189) |
| K-L10 | H nudges | K | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (0.000212, m=3.38) | equivalent (-7.11e-15, m=15.5) | equivalent (0, m=0.00329) | equivalent (0, m=0.189) |
| K-L10 | H nudges | P | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (0.00257, m=3.38) | equivalent (0, m=15.5) | equivalent (0, m=0.00329) | equivalent (0, m=0.189) |
| E-I2 | T nudges | K | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (0, m=3.03) | equivalent (0, m=81.2) | equivalent (0, m=0.00174) | equivalent (6.36e-05, m=0.0859) |
| E-I2 | T nudges | P | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (0, m=3.03) | equivalent (0, m=81.2) | equivalent (0, m=0.00174) | equivalent (0.000568, m=0.0859) |
| E-I2 | H nudges | K | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (0.000126, m=3.03) | equivalent (-3.91e-14, m=81.2) | equivalent (0, m=0.00174) | equivalent (0, m=0.0859) |
| E-I2 | H nudges | P | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (0.000607, m=3.03) | equivalent (-1.89e-14, m=81.2) | equivalent (0, m=0.00174) | equivalent (0, m=0.0859) |
| E-L10 | T nudges | K | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (0, m=14.3) | equivalent (0, m=81.2) | equivalent (0, m=0.00174) | equivalent (2.12e-05, m=0.0291) |
| E-L10 | T nudges | P | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (0, m=14.3) | equivalent (0, m=81.2) | equivalent (0, m=0.00174) | equivalent (-0.000222, m=0.0291) |
| E-L10 | H nudges | K | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (-3.4e-14, m=14.3) | equivalent (0, m=81.2) | equivalent (0, m=0.00174) | equivalent (0, m=0.0291) |
| E-L10 | H nudges | P | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (-1.42e-14, m=14.3) | equivalent (0, m=81.2) | equivalent (0, m=0.00174) | equivalent (0, m=0.0291) |
| M-I2 | T nudges | K | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (0, m=13.1) | equivalent (0, m=81.2) | equivalent (0, m=0.00174) | equivalent (0.00118, m=0.472) |
| M-I2 | T nudges | P | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (0, m=13.1) | equivalent (0, m=81.2) | equivalent (0, m=0.00174) | equivalent (0.00107, m=0.472) |
| M-I2 | H nudges | K | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (0.000311, m=13.1) | equivalent (1.24e-14, m=81.2) | equivalent (0, m=0.00174) | equivalent (0, m=0.472) |
| M-I2 | H nudges | P | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (0.000598, m=13.1) | equivalent (1.48e-14, m=81.2) | equivalent (0, m=0.00174) | equivalent (0, m=0.472) |
| M-L10 | T nudges | K | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (0, m=42) | equivalent (0, m=81.2) | equivalent (0, m=0.00174) | equivalent (-0.000701, m=0.133) |
| M-L10 | T nudges | P | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (0, m=42) | equivalent (0, m=81.2) | equivalent (0, m=0.00174) | equivalent (-0.000508, m=0.133) |
| M-L10 | H nudges | K | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (-4.44e-15, m=42) | equivalent (0, m=81.2) | equivalent (0, m=0.00174) | equivalent (0, m=0.133) |
| M-L10 | H nudges | P | equivalent (0, m=0.00133) | equivalent (0, m=0.00133) | equivalent (1.21e-14, m=42) | equivalent (0, m=81.2) | equivalent (0, m=0.00174) | equivalent (0, m=0.133) |

**P3 reading: PASS.** Shown: 0; undetermined: 0 of 144 statistics.


### P9: the copy at the verdict (S2c against S2, T = 0.4; all six cells, both H)

- K-I2 H=0 share_T, S2c - S2: 0 [0, 0] vs m=0.00133: equivalent
- K-I2 H=0 share_any, S2c - S2 (prediction: not shown smaller): 5.556e-05 [0, 0.0001667] vs m=0.00133: equivalent
- K-I2 H=0 repair per delivered, (S2c - S2) - verdicts/delivered (prediction: equivalent to 0): -2.025e-05 [-6.075e-05, 4.626e-18] vs m=0.189: equivalent
- K-I2 H=inf share_T, S2c - S2: 0 [0, 0] vs m=0.00133: equivalent
- K-I2 H=inf share_any, S2c - S2 (prediction: not shown smaller): 5.556e-05 [0, 0.0001667] vs m=0.00133: equivalent
- K-I2 H=inf repair per delivered, (S2c - S2) - verdicts/delivered (prediction: equivalent to 0): -2.025e-05 [-6.075e-05, 4.626e-18] vs m=0.189: equivalent
- K-L10 H=0 share_T, S2c - S2: 0 [0, 0] vs m=0.00133: equivalent
- K-L10 H=0 share_any, S2c - S2 (prediction: not shown smaller): 0.001167 [0.0009167, 0.001389] vs m=0.00133: undetermined
- K-L10 H=0 repair per delivered, (S2c - S2) - verdicts/delivered (prediction: equivalent to 0): -0.0002422 [-0.000296, -0.000188] vs m=0.189: equivalent
- K-L10 H=inf share_T, S2c - S2: 0.0003611 [5.556e-05, 0.0008056] vs m=0.00133: equivalent
- K-L10 H=inf share_any, S2c - S2 (prediction: not shown smaller): 0.001167 [0.0009167, 0.001389] vs m=0.00133: undetermined
- K-L10 H=inf repair per delivered, (S2c - S2) - verdicts/delivered (prediction: equivalent to 0): -0.0002422 [-0.000296, -0.000188] vs m=0.189: equivalent
- E-I2 H=0 share_T, S2c - S2: 0 [0, 0] vs m=0.00133: equivalent
- E-I2 H=0 share_any, S2c - S2 (prediction: not shown smaller): 0 [0, 0] vs m=0.00133: equivalent
- E-I2 H=0 repair per delivered, (S2c - S2) - verdicts/delivered (prediction: equivalent to 0): -8.095e-18 [-1.735e-17, 5.782e-18] vs m=0.0859: equivalent
- E-I2 H=inf share_T, S2c - S2: 0 [0, 0] vs m=0.00133: equivalent
- E-I2 H=inf share_any, S2c - S2 (prediction: not shown smaller): 0 [0, 0] vs m=0.00133: equivalent
- E-I2 H=inf repair per delivered, (S2c - S2) - verdicts/delivered (prediction: equivalent to 0): -8.095e-18 [-1.735e-17, 5.782e-18] vs m=0.0859: equivalent
- E-L10 H=0 share_T, S2c - S2: 0 [0, 0] vs m=0.00133: equivalent
- E-L10 H=0 share_any, S2c - S2 (prediction: not shown smaller): 6.86e-05 [2.744e-05, 0.0001166] vs m=0.00133: equivalent
- E-L10 H=0 repair per delivered, (S2c - S2) - verdicts/delivered (prediction: equivalent to 0): -1.473e-05 [-2.536e-05, -5.402e-06] vs m=0.0291: equivalent
- E-L10 H=inf share_T, S2c - S2: 0.001722 [0.0002813, 0.004062] vs m=0.00133: undetermined
- E-L10 H=inf share_any, S2c - S2 (prediction: not shown smaller): 6.86e-05 [2.744e-05, 0.0001166] vs m=0.00133: equivalent
- E-L10 H=inf repair per delivered, (S2c - S2) - verdicts/delivered (prediction: equivalent to 0): -1.473e-05 [-2.536e-05, -5.402e-06] vs m=0.0291: equivalent
- M-I2 H=0 share_T, S2c - S2: 0 [0, 0] vs m=0.00133: equivalent
- M-I2 H=0 share_any, S2c - S2 (prediction: not shown smaller): 0 [0, 0] vs m=0.00133: equivalent
- M-I2 H=0 repair per delivered, (S2c - S2) - verdicts/delivered (prediction: equivalent to 0): -3.331e-16 [-7.772e-16, 1.3e-16] vs m=0.472: equivalent
- M-I2 H=inf share_T, S2c - S2: 0 [0, 0] vs m=0.00133: equivalent
- M-I2 H=inf share_any, S2c - S2 (prediction: not shown smaller): 0 [0, 0] vs m=0.00133: equivalent
- M-I2 H=inf repair per delivered, (S2c - S2) - verdicts/delivered (prediction: equivalent to 0): -3.331e-16 [-7.772e-16, 1.3e-16] vs m=0.472: equivalent
- M-L10 H=0 share_T, S2c - S2: 0 [0, 0] vs m=0.00133: equivalent
- M-L10 H=0 share_any, S2c - S2 (prediction: not shown smaller): 6.86e-06 [0, 2.058e-05] vs m=0.00133: equivalent
- M-L10 H=0 repair per delivered, (S2c - S2) - verdicts/delivered (prediction: equivalent to 0): -4.188e-05 [-0.0001256, 5.551e-17] vs m=0.133: equivalent
- M-L10 H=inf share_T, S2c - S2: 7.546e-05 [0, 0.0002264] vs m=0.00133: equivalent
- M-L10 H=inf share_any, S2c - S2 (prediction: not shown smaller): 6.86e-06 [0, 2.058e-05] vs m=0.00133: equivalent
- M-L10 H=inf repair per delivered, (S2c - S2) - verdicts/delivered (prediction: equivalent to 0): -4.188e-05 [-0.0001256, 5.551e-17] vs m=0.133: equivalent

### P10: floors

- E-I2 floors_per_delivered, S2 (T=0.4,H=inf) minus C2 (prediction: not shown larger): -0.02702 [-0.02747, -0.02647] vs m=0.00171: shown smaller
- E-L10 floors_per_delivered, S2 (T=0.4,H=inf) minus C2 (prediction: not shown larger): -0.03931 [-0.03965, -0.03892] vs m=0.00171: shown smaller
- M-I2 floors_per_delivered, S2 (T=0.4,H=inf) minus C2 (prediction: not shown larger): -0.01803 [-0.01865, -0.01728] vs m=0.00171: shown smaller
- M-L10 floors_per_delivered, S2 (T=0.4,H=inf) minus C2 (prediction: not shown larger): -0.0319 [-0.03266, -0.03122] vs m=0.00171: shown smaller
- K-L10 T = 0.1, H = 0: floors per delivered max over seeds 0.2301, mean 0.2281 (prediction <= 0.2): **NO**; verdicts per run 3000
- K-L10 T = 0.14, H = 0: floors per delivered max over seeds 0.2292, mean 0.1755 (prediction <= 0.2): **NO**; verdicts per run 3000


### O-I2: sustained overload (reported only; spec §11 item 5)

Cell I2, 200 000 units per second for 10 s (2 000 000 units; 1.2 x the 166 640 per second that the pause at B = 2048 allows a never-give-up sender, worker decision 14), T = 0.4 s, 12 seeds. The cell was registered to show that expiring from the head makes every sent unit arrive after T (P11: share_T below 0.2 for S2 at H = 0).

**P11 is a miss: share_T = 0.975 for S2 at H = 0 (0.9747 to 0.9751 over seeds), not below 0.2, and for this arm the cell does not test overload.**

- **Mechanism, seen in the gauges (seed 0: 1 662 345 pause events, 1 663 488 verdicts, 0 intake expiries) and in a 4 s probe of the send age.** The age of a unit when it is sent climbs from 0 to 0.385 s over the first 2.4 s and then stays at 0.385 s. At that age the per-seq verdict timer (at hand-over + T - owd = send age + 10 ms) fires before the frame's acknowledgement can return (one round trip is 10 ms plus the report delay), so the unit is verdicted while still in flight. With the L7 rule (verdicted seqs leave the outstanding count) the pause at 2048 outstanding then stops throttling and the sender runs at the full offered 200 000 per second, so the backlog never reaches T and nothing expires in the intake. This is the §2.2 "spurious verdict below about 1.5 round trips", seen from the other side: at overload it removes the only throttle the offline simulation has.
- **So for S2 there is no overload in this cell** (the spec's own "if the offline simulation cannot throttle, report not run" condition applies to that arm), and the 0.975 is the single-pass loss (3 % of originals lost, nearly all refilled) read at a 0.4 s bar. The cost is delay, not loss: w mean 353 ms, p50 385 ms, p99 386 ms, because every frame waits just under T in the intake.
- **H = infinity gives share_T = 0.16, but that comes from the receiver hold** (consumer stall 10.4 s in total), not from L1; share_any is 0.975 in both.
- **With the L7 rule off (S2p) the registered worry shows up.** 328 583 units (16.4 % of the object) expire in the intake and share_any falls to 0.816. The units that are sent leave at age about 0.4 s and arrive after T: share_T is 0.167 at H = 0 (unregistered row) and 0.159 at H = infinity. That is a value for an arm P11 does not register, so it is not counted as a P11 pass. It does say that the collapse the spec describes is real for S2p and absent only because the L7 rule lifts the window.
- **Consequence for Q11 (what to expire under overload):** with the L7 rule and the verdict timer, delay is pinned at about 0.385 s for every frame instead of share_T collapsing; whether that is wanted or a defect is for the owner. The coordinator should also rule whether this cell counts as "not run" for S2.

| arm | seeds | share any | share by own T | share by 0.4 | share by 2 | w mean ms | w p50 ms | w p99 ms | intake expired | finish s | stall total s |
|---|---|---|---|---|---|---|---|---|---|---|---|
| C1 today's receiver, rho = 0.99 | 0 | not run |
| S2 T=0.4 H=0 (P11 arm) | 12 | 0.9751 | 0.9749 | 0.9749 | 0.9751 | 353 | 385 | 386 | 0 | 0.42 | 0.00 |
| S2 T=0.4 H=inf | 12 | 0.9751 | 0.1606 | 0.1606 | 0.9751 | 375 | 402 | 434 | 0 | 0.42 | 10.41 |
| S2p T=0.4 H=inf (verdicted seqs stay in the outstanding count) | 12 | 0.8155 | 0.1588 | 0.1588 | 0.8155 | 376 | 411 | 453 | 328583 | 0.42 | 10.32 |
| S2p T=0.4 H=0 (unregistered) | 12 | 0.8155 | 0.1666 | 0.1666 | 0.8155 | 359 | 400 | 401 | 328583 | 0.42 | 0.00 |

- P11, S2 T=0.4 H=0 (the registered arm): share_T per seed min 0.9747, mean 0.9749, max 0.9751; prediction < 0.2: **FAIL**
- P11, S2p H=inf (not registered for P11): share_T per seed min 0.1567, mean 0.1588, max 0.1603; prediction < 0.2: below 0.2: yes (value only, not a reading)
- P11, S2p H=0 (unregistered): share_T per seed min 0.1646, mean 0.1666, max 0.1677; prediction < 0.2: below 0.2: yes (value only, not a reading)
- S2 H=0, seed 0: pause events 1662345, L2 verdicts 1663488, units 2000000, sent 2000000


## Summary tables per cell

C1 is today's receiver (S1c + R0e, rho = 0.99, global hold, engine loss estimate); C2 is S1c + R1 at rho = 0.99; C3 is S1c + R1 at rho = 1. "new T=a H=b" is S2 + RH. "share by 0.4" and "share by 2" count units delivered within 0.4 s and 2 s of hand-over, for every arm on the same footing; "w" is the delay from hand-over beyond the path delay (§11 item 2); "v" is the stage 2 delay from the loss-free first arrival; "ooo/deliv" is out-of-order deliveries per delivered unit; "stall total" is the consumer's total time without progress; "finish" is the time from the last unit's hand-over to completion.

#### K-I2 (means over 12 seeds; shares are fractions of units handed over; w, v in ms)

| arm | share any | share by 0.4 | share by 2 | w mean | w p50 | w p99 | v mean | v p50 | ooo/deliv | repair/deliv | floors/deliv | stall total s | finish s |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| C1 today's receiver, rho = 0.99 | 0.9948 | 0.9785 | 0.9948 | 17.2 | 0.5 | 765 | 16.7 | 0.0 | 0.0240 | 0.6471 | 0.0505 | 0.52 | 0.005 |
| C2 R1, rho = 0.99 | 0.9951 | 0.9786 | 0.9951 | 17.6 | 0.5 | 777 | 17.1 | 0.0 | 0.0000 | 0.6428 | 0.0595 | 1.07 | 0.005 |
| C3 R1, rho = 1 | 1.0000 | 0.9811 | 1.0000 | 21.3 | 0.5 | 919 | 20.8 | 0.0 | 0.0000 | 0.7766 | 0.0000 | 1.22 | 0.005 |
| new T=0.4 H=0 | 0.9815 | 0.9815 | 0.9815 | 2.8 | 0.5 | 34 | 2.3 | 0.0 | 0.0300 | 0.3613 | 0.0152 | 0.00 | 0.005 |
| new T=0.4 H=0.02 | 0.9815 | 0.9815 | 0.9815 | 3.2 | 0.5 | 35 | 2.7 | 0.0 | 0.0028 | 0.3613 | 0.0152 | 0.99 | 0.005 |
| new T=0.4 H=inf | 0.9815 | 0.9814 | 0.9815 | 3.3 | 0.5 | 37 | 2.8 | 0.0 | 0.0000 | 0.3613 | 0.0152 | 1.22 | 0.005 |
| new T=2 H=0 | 1.0000 | 0.9815 | 1.0000 | 20.4 | 0.5 | 897 | 19.9 | 0.0 | 0.0341 | 0.7766 | 0.0000 | 0.00 | 0.005 |
| new T=2 H=0.02 | 1.0000 | 0.9812 | 1.0000 | 21.0 | 0.5 | 912 | 20.5 | 0.0 | 0.0036 | 0.7766 | 0.0000 | 0.99 | 0.005 |
| new T=2 H=inf | 1.0000 | 0.9811 | 1.0000 | 21.3 | 0.5 | 919 | 20.8 | 0.0 | 0.0000 | 0.7766 | 0.0000 | 1.22 | 0.005 |
| new T=inf H=0 | 1.0000 | 0.9815 | 1.0000 | 20.4 | 0.5 | 897 | 19.9 | 0.0 | 0.0341 | 0.7766 | 0.0000 | 0.00 | 0.005 |
| new T=inf H=0.02 | 1.0000 | 0.9812 | 1.0000 | 21.0 | 0.5 | 912 | 20.5 | 0.0 | 0.0036 | 0.7766 | 0.0000 | 0.99 | 0.005 |
| new T=inf H=inf | 1.0000 | 0.9811 | 1.0000 | 21.3 | 0.5 | 919 | 20.8 | 0.0 | 0.0000 | 0.7766 | 0.0000 | 1.22 | 0.005 |

Reported gauges (mean per run) for the new arms:

| arm | intake expired | L2 verdicts | released by hold | released by floor | stalled runs |
|---|---|---|---|---|---|
| new T=0.4 H=0 | 0.0 | 56.3 | 107.1 | 36.7 | 0 |
| new T=0.4 H=0.02 | 0.0 | 56.3 | 8.4 | 55.3 | 0 |
| new T=0.4 H=inf | 0.0 | 56.3 | 0.0 | 55.4 | 0 |
| new T=2 H=0 | 0.0 | 0.0 | 102.2 | 0.0 | 0 |
| new T=2 H=0.02 | 0.0 | 0.0 | 10.8 | 0.0 | 0 |
| new T=2 H=inf | 0.0 | 0.0 | 0.0 | 0.0 | 0 |
| new T=inf H=0 | 0.0 | 0.0 | 102.2 | 0.0 | 0 |
| new T=inf H=0.02 | 0.0 | 0.0 | 10.8 | 0.0 | 0 |
| new T=inf H=inf | 0.0 | 0.0 | 0.0 | 0.0 | 0 |
| S2c T=0.4 H=0 | 0.0 | 56.3 | 107.0 | 36.7 | 0 |
| S2c T=0.4 H=inf | 0.0 | 56.3 | 0.0 | 55.2 | 0 |
| S2p T=0.4 H=inf | 0.0 | 56.3 | 0.0 | 55.4 | 0 |

#### K-L10 (means over 12 seeds; shares are fractions of units handed over; w, v in ms)

| arm | share any | share by 0.4 | share by 2 | w mean | w p50 | w p99 | v mean | v p50 | ooo/deliv | repair/deliv | floors/deliv | stall total s | finish s |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| C1 today's receiver, rho = 0.99 | 0.9905 | 0.9705 | 0.9905 | 47.8 | 0.9 | 959 | 47.3 | 0.0 | 0.0625 | 0.3102 | 0.0780 | 19.32 | 0.070 |
| C2 R1, rho = 0.99 | 0.9913 | 0.9699 | 0.9913 | 57.9 | 4.5 | 1004 | 57.4 | 3.9 | 0.0000 | 0.3135 | 0.1660 | 24.98 | 0.076 |
| C3 R1, rho = 1 | 1.0000 | 0.9766 | 1.0000 | 68.0 | 15.6 | 1090 | 67.5 | 15.1 | 0.0000 | 0.3534 | 0.0000 | 26.57 | 0.076 |
| new T=0.4 H=0 | 0.9795 | 0.9794 | 0.9795 | 14.6 | 0.6 | 214 | 14.1 | 0.0 | 0.1042 | 0.2012 | 0.0079 | 0.00 | 0.076 |
| new T=0.4 H=0.02 | 0.9795 | 0.9794 | 0.9795 | 16.5 | 0.6 | 216 | 15.9 | 0.0 | 0.1029 | 0.2012 | 0.0079 | 5.34 | 0.076 |
| new T=0.4 H=inf | 0.9795 | 0.9785 | 0.9795 | 46.9 | 12.4 | 246 | 46.4 | 11.8 | 0.0000 | 0.2012 | 0.0079 | 26.55 | 0.076 |
| new T=2 H=0 | 1.0000 | 0.9794 | 1.0000 | 33.9 | 0.6 | 957 | 33.4 | 0.0 | 0.1212 | 0.3534 | 0.0000 | 0.00 | 0.076 |
| new T=2 H=0.02 | 1.0000 | 0.9793 | 1.0000 | 35.8 | 0.7 | 962 | 35.3 | 0.0 | 0.1143 | 0.3534 | 0.0000 | 5.34 | 0.076 |
| new T=2 H=inf | 1.0000 | 0.9766 | 1.0000 | 68.0 | 15.6 | 1090 | 67.5 | 15.1 | 0.0000 | 0.3534 | 0.0000 | 26.57 | 0.076 |
| new T=inf H=0 | 1.0000 | 0.9794 | 1.0000 | 33.9 | 0.6 | 957 | 33.4 | 0.0 | 0.1212 | 0.3534 | 0.0000 | 0.00 | 0.076 |
| new T=inf H=0.02 | 1.0000 | 0.9793 | 1.0000 | 35.8 | 0.7 | 962 | 35.3 | 0.0 | 0.1143 | 0.3534 | 0.0000 | 5.34 | 0.076 |
| new T=inf H=inf | 1.0000 | 0.9766 | 1.0000 | 68.0 | 15.6 | 1090 | 67.5 | 15.1 | 0.0000 | 0.3534 | 0.0000 | 26.57 | 0.076 |

Reported gauges (mean per run) for the new arms:

| arm | intake expired | L2 verdicts | released by hold | released by floor | stalled runs |
|---|---|---|---|---|---|
| new T=0.4 H=0 | 0.0 | 69.7 | 362.8 | 4.8 | 0 |
| new T=0.4 H=0.02 | 0.0 | 69.7 | 339.8 | 24.3 | 0 |
| new T=0.4 H=inf | 0.0 | 69.7 | 0.0 | 61.6 | 0 |
| new T=2 H=0 | 0.0 | 0.0 | 363.5 | 0.0 | 0 |
| new T=2 H=0.02 | 0.0 | 0.0 | 342.8 | 0.0 | 0 |
| new T=2 H=inf | 0.0 | 0.0 | 0.0 | 0.0 | 0 |
| new T=inf H=0 | 0.0 | 0.0 | 363.5 | 0.0 | 0 |
| new T=inf H=0.02 | 0.0 | 0.0 | 342.8 | 0.0 | 0 |
| new T=inf H=inf | 0.0 | 0.0 | 0.0 | 0.0 | 0 |
| S2c T=0.4 H=0 | 0.0 | 69.7 | 366.6 | 0.0 | 0 |
| S2c T=0.4 H=inf | 0.0 | 69.7 | 0.0 | 58.1 | 0 |
| S2p T=0.4 H=inf | 0.0 | 69.7 | 0.0 | 61.6 | 0 |

#### E-I2 (means over 12 seeds; shares are fractions of units handed over; w, v in ms)

| arm | share any | share by 0.4 | share by 2 | w mean | w p50 | w p99 | v mean | v p50 | ooo/deliv | repair/deliv | floors/deliv | stall total s | finish s |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| C1 today's receiver, rho = 0.99 | 0.9901 | 0.9689 | 0.9901 | 39.0 | 7.2 | 1508 | 38.5 | 6.7 | 0.0208 | 1.3051 | 0.0305 | 1.16 | 1.505 |
| C2 R1, rho = 0.99 | 0.9900 | 0.9689 | 0.9900 | 41.1 | 9.9 | 1516 | 40.6 | 9.4 | 0.0000 | 1.2951 | 0.0308 | 1.18 | 1.512 |
| C3 R1, rho = 1 | 1.0000 | 0.9715 | 1.0000 | 55.4 | 11.1 | 1527 | 54.9 | 10.6 | 0.0000 | 1.7178 | 0.0000 | 1.20 | 1.517 |
| new T=0.4 H=0 | 0.9715 | 0.9715 | 0.9715 | 0.9 | 0.5 | 13 | 0.4 | 0.0 | 0.0294 | 0.4656 | 0.0038 | 0.00 | 1.480 |
| new T=0.4 H=0.02 | 0.9715 | 0.9715 | 0.9715 | 10.5 | 10.5 | 20 | 10.0 | 10.1 | 0.0027 | 0.4656 | 0.0038 | 1.16 | 1.480 |
| new T=0.4 H=inf | 0.9715 | 0.9715 | 0.9715 | 12.3 | 11.0 | 33 | 11.8 | 10.5 | 0.0000 | 0.4656 | 0.0038 | 1.16 | 1.480 |
| new T=2 H=0 | 1.0000 | 0.9715 | 1.0000 | 43.4 | 0.5 | 1497 | 42.9 | 0.0 | 0.0314 | 1.7178 | 0.0000 | 0.00 | 1.517 |
| new T=2 H=0.02 | 1.0000 | 0.9715 | 1.0000 | 53.2 | 10.6 | 1513 | 52.7 | 10.2 | 0.0034 | 1.7178 | 0.0000 | 1.18 | 1.517 |
| new T=2 H=inf | 1.0000 | 0.9715 | 1.0000 | 55.4 | 11.1 | 1527 | 54.9 | 10.6 | 0.0000 | 1.7178 | 0.0000 | 1.20 | 1.517 |
| new T=inf H=0 | 1.0000 | 0.9715 | 1.0000 | 43.4 | 0.5 | 1497 | 42.9 | 0.0 | 0.0314 | 1.7178 | 0.0000 | 0.00 | 1.517 |
| new T=inf H=0.02 | 1.0000 | 0.9715 | 1.0000 | 53.2 | 10.6 | 1513 | 52.7 | 10.2 | 0.0034 | 1.7178 | 0.0000 | 1.18 | 1.517 |
| new T=inf H=inf | 1.0000 | 0.9715 | 1.0000 | 55.4 | 11.1 | 1527 | 54.9 | 10.6 | 0.0000 | 1.7178 | 0.0000 | 1.20 | 1.517 |

Reported gauges (mean per run) for the new arms:

| arm | intake expired | L2 verdicts | released by hold | released by floor | stalled runs |
|---|---|---|---|---|---|
| new T=0.4 H=0 | 0.0 | 346.0 | 347.2 | 346.0 | 0 |
| new T=0.4 H=0.02 | 0.0 | 346.0 | 32.3 | 346.0 | 0 |
| new T=0.4 H=inf | 0.0 | 346.0 | 0.0 | 346.0 | 0 |
| new T=2 H=0 | 0.0 | 0.0 | 381.6 | 0.0 | 0 |
| new T=2 H=0.02 | 0.0 | 0.0 | 40.9 | 0.0 | 0 |
| new T=2 H=inf | 0.0 | 0.0 | 0.0 | 0.0 | 0 |
| new T=inf H=0 | 0.0 | 0.0 | 381.6 | 0.0 | 0 |
| new T=inf H=0.02 | 0.0 | 0.0 | 40.9 | 0.0 | 0 |
| new T=inf H=inf | 0.0 | 0.0 | 0.0 | 0.0 | 0 |
| S2c T=0.4 H=0 | 0.0 | 346.0 | 347.2 | 346.0 | 0 |
| S2c T=0.4 H=inf | 0.0 | 346.0 | 0.0 | 346.0 | 0 |
| S2p T=0.4 H=inf | 0.0 | 346.0 | 0.0 | 346.0 | 0 |

#### E-L10 (means over 12 seeds; shares are fractions of units handed over; w, v in ms)

| arm | share any | share by 0.4 | share by 2 | w mean | w p50 | w p99 | v mean | v p50 | ooo/deliv | repair/deliv | floors/deliv | stall total s | finish s |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| C1 today's receiver, rho = 0.99 | 0.9901 | 0.9691 | 0.9901 | 129.2 | 100.3 | 1720 | 128.7 | 99.9 | 0.0691 | 0.5455 | 0.0312 | 1.50 | 1.784 |
| C2 R1, rho = 0.99 | 0.9900 | 0.9696 | 0.9900 | 167.2 | 133.0 | 1752 | 166.7 | 132.5 | 0.0000 | 0.5425 | 0.0438 | 1.52 | 1.809 |
| C3 R1, rho = 1 | 1.0000 | 0.9239 | 0.9981 | 286.3 | 235.0 | 1783 | 285.8 | 234.5 | 0.0000 | 0.5821 | 0.0000 | 1.69 | 1.834 |
| new T=0.4 H=0 | 0.9714 | 0.9714 | 0.9714 | 11.8 | 0.5 | 137 | 11.3 | 0.0 | 0.1008 | 0.2064 | 0.0045 | 0.00 | 1.576 |
| new T=0.4 H=0.02 | 0.9714 | 0.9714 | 0.9714 | 29.0 | 20.0 | 137 | 28.5 | 19.5 | 0.1008 | 0.2064 | 0.0045 | 1.20 | 1.576 |
| new T=0.4 H=inf | 0.9714 | 0.9689 | 0.9714 | 238.7 | 229.7 | 432 | 238.2 | 229.2 | 0.0000 | 0.2064 | 0.0045 | 1.52 | 1.576 |
| new T=2 H=0 | 1.0000 | 0.9714 | 1.0000 | 55.5 | 0.6 | 1542 | 55.0 | 0.0 | 0.1009 | 0.5821 | 0.0000 | 0.00 | 1.834 |
| new T=2 H=0.02 | 1.0000 | 0.9714 | 1.0000 | 72.7 | 20.0 | 1561 | 72.2 | 19.5 | 0.1009 | 0.5821 | 0.0000 | 1.22 | 1.834 |
| new T=2 H=inf | 1.0000 | 0.9239 | 0.9981 | 286.3 | 235.0 | 1783 | 285.8 | 234.5 | 0.0000 | 0.5821 | 0.0000 | 1.69 | 1.834 |
| new T=inf H=0 | 1.0000 | 0.9714 | 1.0000 | 55.6 | 0.6 | 1542 | 55.1 | 0.0 | 0.1009 | 0.5821 | 0.0000 | 0.00 | 1.834 |
| new T=inf H=0.02 | 1.0000 | 0.9714 | 1.0000 | 72.7 | 20.0 | 1561 | 72.2 | 19.5 | 0.1009 | 0.5821 | 0.0000 | 1.22 | 1.834 |
| new T=inf H=inf | 1.0000 | 0.9239 | 0.9981 | 286.3 | 235.0 | 1783 | 285.8 | 234.5 | 0.0000 | 0.5821 | 0.0000 | 1.69 | 1.834 |

Reported gauges (mean per run) for the new arms:

| arm | intake expired | L2 verdicts | released by hold | released by floor | stalled runs |
|---|---|---|---|---|---|
| new T=0.4 H=0 | 0.0 | 357.4 | 1190.9 | 346.0 | 0 |
| new T=0.4 H=0.02 | 0.0 | 357.4 | 1190.9 | 346.0 | 0 |
| new T=0.4 H=inf | 0.0 | 357.4 | 0.0 | 346.9 | 0 |
| new T=2 H=0 | 0.0 | 0.2 | 1225.2 | 0.0 | 0 |
| new T=2 H=0.02 | 0.0 | 0.2 | 1225.2 | 0.0 | 0 |
| new T=2 H=inf | 0.0 | 0.2 | 0.0 | 0.1 | 0 |
| new T=inf H=0 | 0.0 | 0.0 | 1225.2 | 0.0 | 0 |
| new T=inf H=0.02 | 0.0 | 0.0 | 1225.2 | 0.0 | 0 |
| new T=inf H=inf | 0.0 | 0.0 | 0.0 | 0.0 | 0 |
| S2c T=0.4 H=0 | 0.0 | 357.4 | 1190.9 | 346.0 | 0 |
| S2c T=0.4 H=inf | 0.0 | 357.4 | 0.0 | 346.1 | 0 |
| S2p T=0.4 H=inf | 0.0 | 357.4 | 0.0 | 346.9 | 0 |

#### M-I2 (means over 12 seeds; shares are fractions of units handed over; w, v in ms)

| arm | share any | share by 0.4 | share by 2 | w mean | w p50 | w p99 | v mean | v p50 | ooo/deliv | repair/deliv | floors/deliv | stall total s | finish s |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| C1 today's receiver, rho = 0.99 | 0.9928 | 0.3988 | 0.9928 | 812.5 | 1322.1 | 1509 | 241.3 | 7.8 | 0.0183 | 9.2566 | 0.0252 | 1.03 | 1.330 |
| C2 R1, rho = 0.99 | 0.9932 | 0.3941 | 0.9932 | 822.3 | 1324.2 | 1513 | 251.1 | 10.0 | 0.0000 | 9.2438 | 0.0274 | 2.00 | 1.331 |
| C3 R1, rho = 1 | 1.0000 | 0.3933 | 1.0000 | 831.6 | 1325.4 | 1522 | 262.9 | 11.4 | 0.0000 | 9.4490 | 0.0000 | 2.14 | 1.337 |
| new T=0.4 H=0 | 0.3999 | 0.3999 | 0.3999 | 0.9 | 0.5 | 13 | 0.4 | 0.0 | 0.0285 | 14.0314 | 0.0093 | 0.00 | 0.786 |
| new T=0.4 H=0.02 | 0.3999 | 0.3999 | 0.3999 | 10.5 | 10.5 | 20 | 10.0 | 10.0 | 0.0026 | 14.0314 | 0.0093 | 0.48 | 0.786 |
| new T=0.4 H=inf | 0.3999 | 0.3933 | 0.3999 | 37.0 | 11.1 | 1022 | 36.5 | 10.6 | 0.0000 | 14.0314 | 0.0093 | 1.60 | 0.786 |
| new T=2 H=0 | 1.0000 | 0.3999 | 1.0000 | 806.5 | 1317.0 | 1499 | 237.8 | 0.0 | 0.0415 | 9.4490 | 0.0000 | 0.00 | 1.337 |
| new T=2 H=0.02 | 1.0000 | 0.3999 | 1.0000 | 817.6 | 1324.6 | 1514 | 248.9 | 10.7 | 0.0042 | 9.4490 | 0.0000 | 1.01 | 1.337 |
| new T=2 H=inf | 1.0000 | 0.3933 | 1.0000 | 831.6 | 1325.4 | 1522 | 262.9 | 11.4 | 0.0000 | 9.4490 | 0.0000 | 2.14 | 1.337 |
| new T=inf H=0 | 1.0000 | 0.3999 | 1.0000 | 806.5 | 1317.0 | 1499 | 237.8 | 0.0 | 0.0415 | 9.4490 | 0.0000 | 0.00 | 1.337 |
| new T=inf H=0.02 | 1.0000 | 0.3999 | 1.0000 | 817.6 | 1324.6 | 1514 | 248.9 | 10.7 | 0.0042 | 9.4490 | 0.0000 | 1.01 | 1.337 |
| new T=inf H=inf | 1.0000 | 0.3933 | 1.0000 | 831.6 | 1325.4 | 1522 | 262.9 | 11.4 | 0.0000 | 9.4490 | 0.0000 | 2.14 | 1.337 |

Reported gauges (mean per run) for the new arms:

| arm | intake expired | L2 verdicts | released by hold | released by floor | stalled runs |
|---|---|---|---|---|---|
| new T=0.4 H=0 | 0.0 | 7289.8 | 140.4 | 7288.0 | 0 |
| new T=0.4 H=0.02 | 0.0 | 7289.8 | 14.7 | 7288.0 | 0 |
| new T=0.4 H=inf | 0.0 | 7289.8 | 0.0 | 7289.8 | 0 |
| new T=2 H=0 | 0.0 | 0.0 | 504.3 | 0.0 | 0 |
| new T=2 H=0.02 | 0.0 | 0.0 | 51.2 | 0.0 | 0 |
| new T=2 H=inf | 0.0 | 0.0 | 0.0 | 0.0 | 0 |
| new T=inf H=0 | 0.0 | 0.0 | 504.3 | 0.0 | 0 |
| new T=inf H=0.02 | 0.0 | 0.0 | 51.2 | 0.0 | 0 |
| new T=inf H=inf | 0.0 | 0.0 | 0.0 | 0.0 | 0 |
| S2c T=0.4 H=0 | 0.0 | 7289.8 | 140.4 | 7288.0 | 0 |
| S2c T=0.4 H=inf | 0.0 | 7289.8 | 0.0 | 7289.8 | 0 |
| S2p T=0.4 H=inf | 5241.8 | 2048.0 | 0.0 | 2048.0 | 0 |

#### M-L10 (means over 12 seeds; shares are fractions of units handed over; w, v in ms)

| arm | share any | share by 0.4 | share by 2 | w mean | w p50 | w p99 | v mean | v p50 | ooo/deliv | repair/deliv | floors/deliv | stall total s | finish s |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| C1 today's receiver, rho = 0.99 | 0.9892 | 0.3895 | 0.9892 | 1056.6 | 1614.1 | 1788 | 422.6 | 178.4 | 0.0306 | 2.6980 | 0.0070 | 1.58 | 1.671 |
| C2 R1, rho = 0.99 | 0.9940 | 0.2698 | 0.9940 | 1243.8 | 1621.3 | 1776 | 596.8 | 172.1 | 0.0000 | 2.6669 | 0.0369 | 2.83 | 1.665 |
| C3 R1, rho = 1 | 1.0000 | 0.1761 | 0.9557 | 1484.3 | 1715.2 | 1945 | 839.7 | 406.7 | 0.0000 | 2.6540 | 0.0000 | 2.92 | 1.759 |
| new T=0.4 H=0 | 0.3908 | 0.3908 | 0.3908 | 9.1 | 0.5 | 103 | 8.6 | 0.0 | 0.0795 | 2.3682 | 0.0050 | 0.00 | 0.884 |
| new T=0.4 H=0.02 | 0.3908 | 0.3908 | 0.3908 | 26.7 | 20.0 | 103 | 26.2 | 19.5 | 0.0795 | 2.3682 | 0.0050 | 0.50 | 0.884 |
| new T=0.4 H=inf | 0.3908 | 0.1988 | 0.3900 | 941.7 | 821.5 | 1801 | 941.2 | 821.0 | 0.0000 | 2.3682 | 0.0050 | 2.05 | 0.884 |
| new T=2 H=0 | 0.9999 | 0.3908 | 0.9999 | 902.3 | 1461.8 | 1649 | 257.6 | 0.0 | 0.1003 | 2.6539 | 0.0003 | 0.00 | 1.759 |
| new T=2 H=0.02 | 0.9999 | 0.3908 | 0.9999 | 919.5 | 1479.9 | 1649 | 274.9 | 19.7 | 0.1003 | 2.6539 | 0.0003 | 1.08 | 1.759 |
| new T=2 H=inf | 0.9999 | 0.1761 | 0.9840 | 1474.4 | 1710.0 | 1910 | 829.8 | 401.9 | 0.0000 | 2.6539 | 0.0003 | 2.92 | 1.759 |
| new T=inf H=0 | 1.0000 | 0.3908 | 0.9999 | 902.4 | 1461.9 | 1649 | 257.8 | 0.0 | 0.1004 | 2.6540 | 0.0000 | 0.00 | 1.759 |
| new T=inf H=0.02 | 1.0000 | 0.3908 | 0.9999 | 919.6 | 1480.0 | 1649 | 275.1 | 19.7 | 0.1004 | 2.6540 | 0.0000 | 1.08 | 1.759 |
| new T=inf H=inf | 1.0000 | 0.1761 | 0.9557 | 1484.3 | 1715.2 | 1945 | 839.7 | 406.7 | 0.0000 | 2.6540 | 0.0000 | 2.92 | 1.759 |

Reported gauges (mean per run) for the new arms:

| arm | intake expired | L2 verdicts | released by hold | released by floor | stalled runs |
|---|---|---|---|---|---|
| new T=0.4 H=0 | 0.0 | 7401.5 | 489.7 | 7288.1 | 0 |
| new T=0.4 H=0.02 | 0.0 | 7401.5 | 489.7 | 7288.1 | 0 |
| new T=0.4 H=inf | 0.0 | 7401.5 | 0.0 | 7400.3 | 0 |
| new T=2 H=0 | 0.0 | 2.8 | 1219.5 | 0.0 | 0 |
| new T=2 H=0.02 | 0.0 | 2.8 | 1219.4 | 0.0 | 0 |
| new T=2 H=inf | 0.0 | 2.8 | 0.0 | 0.8 | 0 |
| new T=inf H=0 | 0.0 | 0.0 | 1219.4 | 0.0 | 0 |
| new T=inf H=0.02 | 0.0 | 0.0 | 1219.4 | 0.0 | 0 |
| new T=inf H=inf | 0.0 | 0.0 | 0.0 | 0.0 | 0 |
| S2c T=0.4 H=0 | 0.0 | 7401.5 | 489.7 | 7288.1 | 0 |
| S2c T=0.4 H=inf | 0.0 | 7401.5 | 0.0 | 7400.2 | 0 |
| S2p T=0.4 H=inf | 5352.2 | 2049.3 | 0.0 | 2048.2 | 0 |

## Reproduction

```
cd tools/toy
python -m unittest test_toy test_toy2                       # stage 1, 2 and 3 tests (55)
python batch3.py --part ID,AA,K,EM [--seed0 N --seeds M]    # resumable, one file per run in out3/ (ignored by git)
python batch3.py --part O --orate 200000                    # overload cell (C1 takes about 45 minutes per seed)
python summarize3.py ident | freeze | K | EM | ALL | O | tables
python diag3.py                                             # unregistered diagnostics
```
