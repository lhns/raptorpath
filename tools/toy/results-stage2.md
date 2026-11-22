# Toy simulation, stage 2, one path (Parts A, T and B)

Spec: `docs/derivations/skip-rule-efficiency.md` §7 (toy plan v3, merged from toy-plan-v3 af156da7, which adds R0c to Part T and main's 50 ms seed to the D_EWMA arm). Code: `tools/toy/` (`toy2.py`, `ustar.py`, `batch2.py`, `summarize2.py`, `report2.py`, `test_toy2.py`); the stage 1 files are untouched. The toy fixes no constant and none of these numbers is engine provenance. Not built, by instruction: Parts C and D, two-path cells, in-path reorder cells, the W sweep, §7.14.

**D arm used.** D_RTprop is the default in every cell except where AG1 names an arm (the bridge P4h and the P3 blocks were registered on the paper's form). The D_EWMA arm uses main's 50 ms SRTT seed before its first sample, as the plan now says.

## Deviations from registration (all of them)

1. **12 seeds, no 24-seed reruns.** Every "undetermined" decision clause below was left undetermined; the reserve was not spent.
2. **A/A self-check: 7 of 118 readings read "shown" in the first A/A, and did not replicate.** The 7 are R0e's tail metrics in the I2 tail cell (v and e at p99 and p99.9, tail time, tail-only mean and median v): the mean A/A difference was about -8 to -9 ms against T = 1 ms, CI excluding 0. Both arms draw phases from the same distribution, so the true mean difference is zero; 7 of 118 is about what a 95 % CI gives with no multiplicity allowance, and all 7 are one quantity in one cell. Replication with two fresh phase draws (variants 2 and 3, 12 seeds) for R0e (engine and cumulative eps-hat) and R1: **0 of 7 readings are shown again** (all undetermined, differences of +2 to +12 ms with CIs spanning 0). So this is recorded as a chance result of the self-check, not a toy defect. The frozen NF stays, because the per-seed phase scatter it measures is real: the SD of the per-seed tail-time difference between two phase sets is 9.5 to 10 ms for R0e and 41 to 42 ms for R1. Phase-set means of the I2 tail time (ms, 12 seeds): R0e 1488.0, 1495.5, 1495.2, 1489.6, 1490.3, 1485.9 (variants 0 to 5); R1 1512.1, 1522.6, 1524.0, 1511.7, 1512.3, 1526.2. Effect: Part T's bars for tail time and p99 metrics are large (NF about 80 ms). The table was frozen with `--force` because the first reading looked like a failure; no code was changed.
3. **P4a is registered for S1c and cannot hold as written.** S1c's last copy rides the verdict and fills an isolated hole at the instant S0's first resend would, so the median stall equals S0's (measured S1c - S0 = 0 within 1 ms). The +H appears on S1 (a verdict without the copy): S1 - S0 = H within 1 ms (test). The hold itself is checked absolutely: every floor pass in an I1 run with H forced to 5 ms happens exactly 5 ms after that floor's arrival (to 1e-9 s). The copy-less S1 sender exists in `toy2.py` for this test only.
4. **The early-skip receivers use the engine's eps-hat** (owner's coordinator, after review): a per-batch EWMA with alpha = 0.1 (`raptorpath/src/control/estimator.rs:92` for alpha, `:103-110` for the update `rx_ewma_loss <- 0.1 (e - r)/e + 0.9 rx_ewma_loss`, read as the maximum over live paths, `receiver.rs:2266`; one path here). One arrival is one batch: for an arrival above rhi e = s - rhi and r = 1, otherwise e = r = 1. Both R0e and R0c use it, so they differ only in the allowance operand. Part T was rerun for R0e and R0c (all cells, 12 seeds) and these are the reported rows. The registered cumulative eps-hat (which can never make the hold fall back) and R0x (per-seq EWMA, alpha 1/256, my own toy choice) stay as history rows, labelled. **Witness:** the hold falls back about 1 546 (I1), 7 650 (I2) and 8 690 (L10) times per run under the engine eps-hat, for both R0e and R0c, so the I2 witness is met and R0e's I2 reading stands.
5. **Part T additions and choices.** R0c was added after Part A had run and before Part T. Dead window: every datagram (original, copy or floor) for a tail seq (floor value at or above the first tail seq) sent in [1.18 s, 2.68 s) is lost. In object mode the sweep also re-serves reported holes: without it a finished stream sends no more gap reports, so holes whose copies were lost are never served again, and R1 stalled at 40 s in I2 (seen in the first A/A run; the AAT runs were rerun after the fix, which touches no non-object cell). Runs end 1 s after the ack frontier reaches S_src (or at 40 s). The EWMA arm's seed-era change and R0c were merged from the plan during the work; Part A's frozen noise floors were computed before either and neither changes a Part A cell.
6. **S0 reference runs** (36 for Parts A and B, 36 for Part T, one per cell and seed) were added; the relative bars need them. They are not in the registered counts.
7. **c_H rent arm: skipped.** The plan's B (sender backlog) and g (delivery rate) are not defined enough to build. The d_rep arm ran at the three presets (P4g witness).
8. **ρ nudges:** ω in {1, 10} x 4 non-centre points = 8 configurations (centres are in the I2 block); the registered "9" cannot be derived from the text. The ρ block reads against the Realtime noise floor.
9. **KINK follow-up** (registered curvature check): two more points ran for KINK only.
10. **HZ calibration:** one S0 run per cell at seed 1000 gives tau1 (median first-repair fill time), q99 and d; delta* is placed at Auto exactly (omega derived), so at Auto itself HZ holds.
11. **P4h:** S0 is bit-identical (trace and readouts) and S1c's verdict count is equal in 12 of 12 seeds; S1c's HOL sum differs in 8 of 12 seeds and floors sent differ (940 against 1276 per run: v3 repeats floors on the sweep clock, stage 1 refreshed on a timer). That is the floor-timing difference the plan expects.
12. **Wall time:** Part A 1416 s plus 15 s (AAT rerun), Part T 68 s, Part B 5561 s plus 50 s (KINK follow-up): about 2.1 h against 3.3 h estimated. One base run costs 2.2 to 2.8 s. No fallback cut was applied; nothing was cut.
13. **Tests:** `python -m unittest test_toy2` (32 tests) and stage 1's `test_toy` (11) pass. P2 excludes R0e (its own passes move the ack, so its sender trace differs by design).

## Checks

- **P1:** 0 violations in every Part B run and every Part T run of R1 and U; the positive control (a sender that ignores the count) trips it. **R0e and R0c violate P1** (today's receiver ignores the count): about 5 (I1), 27 (I2) and 160 to 250 (L10) pass events above the allowance per run (engine eps-hat), worst excess 5 to 54 seqs.
- **P2** (ρ = 1 and 1 - 1e-7): sender traces equal S0's for S1c with R1 and with U (unit tests); in Part B 0 verdicts, floors and passes at both.
- **P4a** (deviation 3), **P4b** (S1c's G_rx/S_src within 1.5 ε ĥ), **P4c** (U's consumer stall per isolated hole equals min(H_o, R - t_detect, t_F + H - t_detect), observed to within 1e-5 s in four settings), **P4e** (sweep == TU LP == brute force on 200 random instances, LP integral every time), **P4f** (n_dec = 0 at Realtime and Auto under D_RTprop in I1, I2 and Q; above 0 at b = 1.3 and Bulk), **P4g** (d_all == d_rep exactly), **P4h** (deviation 11), **P4i** (e over non-hole packets equals stage 1's HOL sum), **P4j** (below), the **H estimator tests** (FIFO gives 0; a 7 ms late packet gives 6 ms until the window passes; empty window and cold start 0; strict and non-strict; two-path synthetic skew; a later floor never delays an earlier marker), the **deny test** with a planted positive control, and **FIFO** (no datagram overtook another in any run; H is 0 and the estimator counted no late arrival in any Part B run). All pass.
- **P4j:** on isolated holes the bound ratio_U <= 2 + omega N_{R=inf} / cost_U* holds in every seed at omega in {1, 10} and the three presets (worst slack 5.8e-11). The isolated-hole ratio is 2.02 at Realtime, 1.02 at Auto omega = 1, and 1.00 elsewhere.

## Readings, Part T first (lost tail; Realtime, rho = 0.99, one path)

Times in ms, 12 seeds. Bars m = max(T, NF), NF from the I2 A/A. The early-skip rows use the engine eps-hat (deviation 4); the hold falls back, so the witness is met.

- **Nothing stalled.** Every receiver in every cell got the ack to S_src. Tail time (last send to ack frontier) is 1.49 s in I1 and I2 and 1.67 to 1.81 s in L10. The dead window is itself 1.5 s, so nearly all of this is the window; receivers differ in how fast the denied tail seqs are served after it.
- **R0e against R1.** I1: equivalent on every wait readout; R0e makes 183 passes against 121. I2: R0e's median v is 4.7 ms lower (shown), mean v 5.1 ms lower (shown), largest-episode spend 22.8 higher (shown), tail time 24 ms shorter (equivalent under the 85 ms bar). L10: R0e is shown better on every wait readout: median -36 ms, p99 -120 ms, mean -44 ms, tail time -141 ms. So the early skip buys a few ms in I2 and tens of ms at RTT 100 ms with 10 % loss, and costs spend.
- **R0c against R0e.** R0c's passes come no later and its floor share G_floor/S_src is 0.0090 to 0.0100 against the 0.01 allowance. In I2 R0c is shown better than R0e: mean v 4.1 ms lower, repair sends per delivered 0.161 lower, tail seqs with a verdict 121 against 89 (R0e's allowance is eaten by early-skip passes). R0c's G_rx/S_src exceeds R0e's by 0.0027 in I2 and 0.0028 in L10, which is at most the 0.01 the plan predicts. **The plan's prediction holds on all three counts:** passes no later, total abandonment above R0e's by at most G_floor <= (1 - rho) S_src, and the tail passed by the floor where R0e's is refused (R0c: all 121 verdicts used in I1 and I2 and 117.5 in L10; R0e: 89 in I2, 81 in L10).
- **Against U (the target).** U has R1's sender trace and is shown better on wait: mean v 8.5 ms lower than R1 in I2, 99 ms lower in L10. R0e and R0c sit between R1 and U in I2 and L10.
- **Tail-only wait (the 347 seqs).** I2: R0e 1487, R0c 1484, R1 1509, U 1481 ms. L10: about 1624, 1620, 1727, 1536 ms (engine eps-hat rows).
- **In words.** The interim receiver costs nothing at the tail (no stall in any cell), buys 5 to 45 ms of mean wait, breaks the promise (P1) by up to 60 seqs (about 5 events per run in I1, 27 in I2, 160 to 250 in L10), and R0c keeps that benefit while admitting verdicts up to the allowance on floor passes alone. Neither interim receiver beats U. Nothing is adopted.

## Readings, Part A (noise floors)

The frozen one-path noise-floor table is `tools/toy/nf_stage2.json`, committed before Parts T and B ran. Largest 95th percentile of |A - A'| over the six A/A cells: v and e p50 0.11 ms, p99 1.5 ms, p99.9 0.4 ms, means 0.13 ms, rbf 0.0075, G_rx/S_src 2.2e-4. Part T cells: p99 and p99.9 about 81 ms, mean 3.0 ms, tail time 85 ms (sweep phase, deviation 2). NF_K and NF_P are in the tables. The A/A self-check is 95 equivalent, 16 undetermined, 7 shown (deviation 2).

## Readings, Part B (U at one path)

- **P3 has power.** HZ fails on rbf (K 0.97, shown) and on mean e (K -2.7 ms, shown). STEP fails on K for rbf (shown) but not on mean e or e p99 (equivalent): the registered requirement is met on rbf only. SPIKE fails on P for rbf (shown) with K passing, as required. **KINK fails P on rbf** (P 0.0049 shown, K undetermined) and passes on mean e; the registered curvature follow-up gives intercept a = 0.0049 [0.0038, 0.0061] against m = 0.0038: undetermined. By the plan's rule an rbf fail of P on this curved readout is read as undetermined.
- **U passes P3 at all three presets for every omega.** For omega in {0, 0.3, 1, 3, 10, 100} at Bulk, Auto and Realtime in I1 (18 of 18 configurations) all five readouts (e mean, e p99, rbf, G_rx/S_src, repair sends) read equivalent on both K and P. The rho nudge at Realtime in I2 (omega 1 and 10) passes everywhere. No step at any preset.
- **Order kept (rbf below eps^2).** Bulk keeps order for every omega >= 0.3 in all three cells (H_o of 0.7 s or more: every hole is filled first), and releases everything at omega = 0. Auto keeps order for omega >= 3 (rbf 0 to 4e-5); at omega = 1 it breaks it in I1 and G (rbf 0.010 and 0.012 against 6.3e-4) and is undetermined in I2 (0.010 against 0.010); at omega <= 0.3 it releases nearly everything. Realtime releases nearly every hole for omega <= 10 (H_o of 0.02 to 2.3 ms is shorter than one repair round) and keeps order only at omega = 100 in I2. **The band where Bulk and Auto both keep order is omega >= 3 in I1, I2 and G.** This is the threshold the measured omega is read against, a toy result.
- **ratio_U** (d moves, early holes pay no H_o). Over all holes: 1.00 at Bulk; at Auto 1.00 to 1.14 for omega >= 1 but 1.9 to 4.4 at omega = 0.3; at Realtime 1.6 to 2.2 for omega <= 10 and 1.01 at omega = 100. The 2x bound is proven only on isolated holes with d frozen (P4j, passes); in decision runs the isolated-hole bound is exceeded in 5 to 9 of 12 seeds at Auto omega = 0.3 and at Realtime (the omega N_{R=inf} term and the moving d), so it stays "sketch, measured".
- **d_all against d_rep** at one path: identical, as the plan says.
- **AG1 (the age gate).** Witnesses met in I1, I2 and Q. Under D_RTprop n_dec is 0 at Realtime and Auto and rises with b (I1, summed over 12 seeds: 5 325 at b = 1.15, 14 146 at b = 1.3, 55 501 at Bulk). Under D_EWMA at Auto n_dec is 3 668 (I1), 316 210 (I2) and 7 434 (Q), and 6, 48 and 22 at Realtime (reported; the prediction of 0 does not hold). Seed-era verdicts (before the first RTT sample) are about 0 per run under D_EWMA at every point. The n_dec at Realtime split by the first RTT sample: I1 6 of 6 in the seed era (D = 25 ms), I2 13 of 48, Q 6 of 22; so the 50 ms seed explains the whole miss in I1 and part in I2 and Q, and the rest (35 in I2, 16 in Q) are refusals after the first sample, where a first report is younger than half the EWMA (sampler wait and jitter in the sample). Reading: at Realtime D is live but decides only timing (equivalent on every readout). From Auto down, **D is live and changes a readout by a shown amount**: under D_EWMA in I2 mean e is 1.6 to 2.7 ms higher and e p99 8 to 12 ms higher than under D_RTprop (shown), and repair sends move by 0.003 to 0.03 per delivered; in I1 and Q the effect is on repair sends only (shown, 0.001 to 0.005) with e equivalent. D = 0 changes spend and repair sends below Auto in I1 and Q, and lowers mean e by 1.9 ms at Bulk in I2.
- Several AG1 e p99 and e mean readings at Bulk and the KINK rbf anomaly are undetermined and were not rerun at 24 seeds.

## Open problems

- A/A phase noise on the tail cell (deviation 2, a chance result).
- STEP does not fail on the continuous readouts at Auto omega = 1; KINK's rbf curvature leaves P undetermined.
- c_H not built; no 24-seed reruns.

## Tables

### Part A tables

### Noise floors NF (95th percentile over 12 seeds of |A - A'|, largest over the six A/A cells; ms for times)

| metric | NF | | metric | NF (Part T cells) |
|---|---|---|---|---|
| v_p50 | 0.1096 | | v_p50 | 0.2189 |
| v_p99 | 1.488 | | v_p99 | 81.18 |
| v_p999 | 0.4041 | | v_p999 | 81.99 |
| v_mean | 0.1284 | | v_mean | 3.026 |
| e_p50 | 0.1096 | | e_p50 | 0.2189 |
| e_p99 | 1.488 | | e_p99 | 81.18 |
| e_p999 | 0.4041 | | e_p999 | 81.99 |
| e_mean | 0.1284 | | e_mean | 3.026 |
| G_ratio | 0.0002207 | | G_ratio | 0.001832 |
| max_ep_spend | 3 | | max_ep_spend | 9.35 |
| repair_per_delivered | 0.0002217 | | repair_per_delivered | 0.01585 |
| rbf | 0.007502 | | rbf | 0 |
| fp_rel_pd | 2.251e-06 | | fp_rel_pd | 0.0007797 |
| ooo_per_delivered | 0.0002155 | | ooo_per_delivered | 0.001742 |
| | | | tail_time | 85.12 |
| | | | tail_v_mean | 73.22 |
| | | | tail_v_p50 | 80.89 |

### NF_K and NF_P (A/A on the five-point sets)

| preset | readout | NF_K | NF_P |
|---|---|---|---|
| Auto | e_mean | 0.001195 | 0.0004607 |
| Auto | e_p99 | 0.04417 | 0.03722 |
| Auto | rbf | 0.008293 | 0.003821 |
| Auto | G_ratio | 0 | 0 |
| Auto | repair_per_delivered | 0 | 0 |
| Realtime | e_mean | 7.112e-08 | 4.575e-08 |
| Realtime | e_p99 | 0.0001885 | 9.456e-05 |
| Realtime | rbf | 0 | 0 |
| Realtime | G_ratio | 0 | 0 |
| Realtime | repair_per_delivered | 0 | 0 |
| Bulk (max of the two) | e_mean | 0.001195 | 0.0004607 |
| Bulk (max of the two) | e_p99 | 0.04417 | 0.03722 |
| Bulk (max of the two) | rbf | 0.008293 | 0.003821 |
| Bulk (max of the two) | G_ratio | 0 | 0 |
| Bulk (max of the two) | repair_per_delivered | 0 | 0 |

### A/A self-check (mean A/A difference against T)

95 readings equivalent, 16 undetermined, 7 shown.
- undetermined: I1-AU-rbf: -0.001473 [-0.003546, 0.0005946] vs m=0.000625: undetermined
- undetermined: I1-AU-ooo_per_delivered: -7.083e-06 [-1.708e-05, 2.917e-06] vs m=5e-06: undetermined
- undetermined: I2-AU-ooo_per_delivered: 2.083e-06 [-2.917e-06, 8.333e-06] vs m=5e-06: undetermined
- undetermined: G-RT-ooo_per_delivered: 1.958e-05 [-4.875e-05, 8.916e-05] vs m=5e-06: undetermined
- undetermined: G-AU-rbf: 0.0004045 [-0.0005722, 0.00138] vs m=0.000625: undetermined
- undetermined: G-AU-ooo_per_delivered: 1.042e-05 [-1.375e-05, 3.417e-05] vs m=5e-06: undetermined
- undetermined: T-R0e-v_mean: -0.261 [-1.132, 0.5597] vs m=1: undetermined
- undetermined: T-R0e-e_mean: -0.261 [-1.132, 0.5597] vs m=1: undetermined
- undetermined: T-R0e-max_ep_spend: -0.25 [-3.167, 2.25] vs m=1: undetermined
- undetermined: T-R1-v_p99: -12.12 [-33.83, 9.223] vs m=1: undetermined
- undetermined: T-R1-v_p999: -12.75 [-34.8, 9.177] vs m=1: undetermined
- undetermined: T-R1-e_p99: -12.12 [-33.83, 9.223] vs m=1: undetermined
- undetermined: T-R1-e_p999: -12.75 [-34.8, 9.177] vs m=1: undetermined
- undetermined: T-R1-tail_time: -10.41 [-33.27, 11.48] vs m=1: undetermined
- undetermined: T-R1-tail_v_mean: -10.77 [-30.5, 9.337] vs m=1: undetermined
- undetermined: T-R1-tail_v_p50: -12.07 [-33.74, 9.178] vs m=1: undetermined
- SHOWN: T-R0e-v_p99: -9.106 [-13.21, -4.198] vs m=1: shown smaller
- SHOWN: T-R0e-v_p999: -9.335 [-13.6, -4.306] vs m=1: shown smaller
- SHOWN: T-R0e-e_p99: -9.106 [-13.21, -4.198] vs m=1: shown smaller
- SHOWN: T-R0e-e_p999: -9.335 [-13.6, -4.306] vs m=1: shown smaller
- SHOWN: T-R0e-tail_time: -7.538 [-12.32, -2.257] vs m=1: shown smaller
- SHOWN: T-R0e-tail_v_mean: -9.113 [-13.28, -4.131] vs m=1: shown smaller
- SHOWN: T-R0e-tail_v_p50: -9.057 [-13.21, -4.083] vs m=1: shown smaller

### P4j: U against U*, d frozen at RTprop, I1 (12 seeds each)

| omega | preset | isolated holes (mean) | worst slack over seeds (<= 1e-9 required) | ratio_U isolated (mean) | ratio_U all holes (mean) | pass |
|---|---|---|---|---|---|---|
| 1 | Bulk | 308 | -1.61 | 1.000 | 1.000 | pass |
| 1 | Auto | 308 | -159 | 1.023 | 1.047 | pass |
| 1 | Realtime | 315 | -2.62e-10 | 2.023 | 2.017 | pass |
| 10 | Bulk | 308 | -1.61 | 1.000 | 1.000 | pass |
| 10 | Auto | 308 | -161 | 1.000 | 1.000 | pass |
| 10 | Realtime | 315 | 5.82e-11 | 2.023 | 1.976 | pass |

### P4h bridge to the stage 1 toy (I1, N = 1, Realtime, J = 0, 20 s, 12 seeds; stage 1 at commit with floor_with_copy = False)

- S0: seeds with identical n_src 12/12, delivered 12/12, n_holes 12/12, hol_sum_nonhole 12/12, iso_p50 12/12, G_final 12/12, max_ep_spend 12/12; verdict count equal in 12 of 12 seeds; floors sent stage 1 0 (of which refresh 0.0) against stage 2 0 (of which repeats 0.0), mean per run; hol p99 stage 1 13.967 ms against e p99 stage 2 14.125 ms
- S1c: seeds with identical n_src 12/12, delivered 12/12, n_holes 12/12, hol_sum_nonhole 4/12, iso_p50 12/12, G_final 12/12, max_ep_spend 12/12; verdict count equal in 12 of 12 seeds; floors sent stage 1 940 (of which refresh 0.3) against stage 2 1276 (of which repeats 74.5), mean per run; hol p99 stage 1 11.117 ms against e p99 stage 2 11.225 ms

### Wall time per run (s, mean)

- AA- 2.79 s over 144 runs
- AAT- 0.30 s over 48 runs
- KP- 2.54 s over 240 runs
- P4J- 2.53 s over 72 runs
- BR- 4.53 s over 24 runs
- REF- 2.63 s over 36 runs

### Part T tables

### Part T: per-config means over 12 seeds (times in ms)

| cell | receiver | v p50 | v p99 | v p99.9 | v mean | e mean | G_rx/S_src | passes (floor / R0e) | P1 violations (worst excess) | tail time | stalled | tail seqs with a verdict | filled after the window | R0e hold fell back | repair sends / delivered | G_floor/S_src | floors / delivered | last copies / delivered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| I1 | R0e (today's early-skip, engine eps-hat) | 0.00 | 1488.1 | 1496.2 | 28.71 | 28.71 | 0.00995 | 183 (109 / 74) | 4.8 (4.9) | 1485.0 | 0/12 | 120.0 | 226.1 | 1546.2 | 1.12477 | 0.00901 | 0.01071 | 0.01486 |
| I1 | R0c (early skip, allowance counts floor passes only) | 0.00 | 1487.9 | 1495.7 | 28.47 | 28.47 | 0.01011 | 185 (121 / 64) | 6.2 (6.6) | 1485.0 | 0/12 | 121.0 | 224.2 | 1545.1 | 1.11687 | 0.00996 | 0.01067 | 0.01495 |
| I1 | R1 (floor only) | 0.00 | 1499.3 | 1508.5 | 30.93 | 30.93 | 0.00996 | 121 (121 / 0) | 0.0 (0.0) | 1498.1 | 0/12 | 120.0 | 226.0 | 0.0 | 1.12160 | 0.00996 | 0.01153 | 0.01486 |
| I1 | U, omega = 1 | 0.00 | 1483.3 | 1494.1 | 28.03 | 28.03 | 0.00996 | 121 (121 / 0) | 0.0 (0.0) | 1498.1 | 0/12 | 120.0 | 226.0 | 0.0 | 1.12160 | 0.00996 | 0.01153 | 0.01486 |
| I1 | R0e, cumulative eps-hat (history, reported only) | 0.00 | 1488.1 | 1496.2 | 28.71 | 28.71 | 0.00995 | 183 (109 / 74) | 4.8 (4.9) | 1485.0 | 0/12 | 120.0 | 226.1 | 0.0 | 1.12477 | 0.00901 | 0.01071 | 0.01486 |
| I1 | R0c, cumulative eps-hat (history, reported only) | 0.00 | 1487.9 | 1495.7 | 28.47 | 28.47 | 0.01011 | 185 (121 / 64) | 6.2 (6.6) | 1485.0 | 0/12 | 121.0 | 224.2 | 0.0 | 1.11687 | 0.00996 | 0.01067 | 0.01495 |
| I1 | R0e, per-seq EWMA eps-hat alpha 1/256 (history, reported only) | 0.00 | 1488.1 | 1496.2 | 28.79 | 28.79 | 0.00996 | 182 (110 / 72) | 4.8 (5.0) | 1485.0 | 0/12 | 120.0 | 226.0 | 331.8 | 1.12437 | 0.00902 | 0.01075 | 0.01486 |
| I1 | S0 reference | 0.06 | 1512.2 | 1521.2 | 45.81 | 45.81 | 0.00000 | 0 (0 / 0) | 0.0 (0.0) | 1499.1 | 0/12 | 0.0 | 346.0 | 0.0 | 1.68875 | 0.00000 | 0.00000 | 0.00000 |
| I2 | R0e (today's early-skip, engine eps-hat) | 4.70 | 1493.3 | 1503.4 | 35.51 | 35.51 | 0.01030 | 459 (80 / 379) | 27.8 (23.4) | 1488.0 | 0/12 | 88.8 | 252.8 | 7652.7 | 1.30671 | 0.00659 | 0.02999 | 0.03553 |
| I2 | R0c (early skip, allowance counts floor passes only) | 4.69 | 1489.9 | 1500.2 | 31.42 | 31.42 | 0.01304 | 490 (111 / 379) | 27.2 (54.0) | 1488.0 | 0/12 | 121.0 | 219.4 | 7642.7 | 1.14573 | 0.00913 | 0.03040 | 0.03893 |
| I2 | R1 (floor only) | 9.38 | 1515.1 | 1523.6 | 40.60 | 40.60 | 0.00996 | 121 (121 / 0) | 0.0 (0.0) | 1512.1 | 0/12 | 89.7 | 256.3 | 0.0 | 1.29514 | 0.00996 | 0.03084 | 0.03630 |
| I2 | U, omega = 1 | 0.00 | 1487.5 | 1503.1 | 32.14 | 32.14 | 0.00996 | 121 (121 / 0) | 0.0 (0.0) | 1512.1 | 0/12 | 89.7 | 256.3 | 0.0 | 1.29514 | 0.00996 | 0.03084 | 0.03630 |
| I2 | R0e, cumulative eps-hat (history, reported only) | 4.70 | 1493.3 | 1503.4 | 35.51 | 35.51 | 0.01030 | 459 (80 / 379) | 27.8 (23.4) | 1488.0 | 0/12 | 88.8 | 252.8 | 0.0 | 1.30671 | 0.00659 | 0.02999 | 0.03553 |
| I2 | R0c, cumulative eps-hat (history, reported only) | 4.69 | 1489.9 | 1500.2 | 31.42 | 31.42 | 0.01304 | 490 (111 / 379) | 27.2 (54.0) | 1488.0 | 0/12 | 121.0 | 219.4 | 0.0 | 1.14573 | 0.00913 | 0.03040 | 0.03893 |
| I2 | R0e, per-seq EWMA eps-hat alpha 1/256 (history, reported only) | 5.49 | 1493.6 | 1503.8 | 36.36 | 36.36 | 0.01032 | 430 (84 / 346) | 27.2 (23.7) | 1488.3 | 0/12 | 88.8 | 252.5 | 3151.2 | 1.30523 | 0.00692 | 0.03011 | 0.03560 |
| I2 | S0 reference | 10.63 | 1526.5 | 1535.3 | 54.86 | 54.86 | 0.00000 | 0 (0 / 0) | 0.0 (0.0) | 1516.6 | 0/12 | 0.0 | 346.0 | 0.0 | 1.71776 | 0.00000 | 0.00000 | 0.00000 |
| L10 | R0e (today's early-skip, engine eps-hat) | 96.66 | 1631.3 | 1642.1 | 123.16 | 123.16 | 0.00973 | 1195 (71 / 1124) | 251.5 (48.7) | 1667.6 | 0/12 | 81.4 | 267.3 | 8692.4 | 0.55120 | 0.00582 | 0.02538 | 0.08022 |
| L10 | R0c (early skip, allowance counts floor passes only) | 96.66 | 1626.9 | 1637.8 | 117.96 | 117.96 | 0.01257 | 1231 (111 / 1119) | 157.0 (53.0) | 1667.4 | 0/12 | 117.5 | 228.1 | 8655.2 | 0.50838 | 0.00916 | 0.03803 | 0.11107 |
| L10 | R1 (floor only) | 132.48 | 1751.4 | 1775.1 | 166.67 | 166.67 | 0.00996 | 121 (121 / 0) | 0.0 (0.0) | 1809.0 | 0/12 | 98.2 | 247.8 | 0.0 | 0.54254 | 0.00996 | 0.04381 | 0.10786 |
| L10 | U, omega = 1 | 1.90 | 1533.0 | 1629.1 | 67.27 | 67.27 | 0.00996 | 121 (121 / 0) | 0.0 (0.0) | 1809.0 | 0/12 | 98.2 | 247.8 | 0.0 | 0.54254 | 0.00996 | 0.04381 | 0.10786 |
| L10 | R0e, cumulative eps-hat (history, reported only) | 95.59 | 1630.6 | 1641.4 | 121.99 | 121.99 | 0.00965 | 1298 (72 / 1226) | 318.2 (48.7) | 1666.8 | 0/12 | 83.6 | 265.9 | 0.0 | 0.55125 | 0.00591 | 0.03115 | 0.08250 |
| L10 | R0c, cumulative eps-hat (history, reported only) | 95.55 | 1626.1 | 1637.0 | 116.56 | 116.56 | 0.01299 | 1335 (111 / 1224) | 193.7 (60.4) | 1666.8 | 0/12 | 121.0 | 224.6 | 0.0 | 0.50304 | 0.00913 | 0.04465 | 0.11157 |
| L10 | R0e, per-seq EWMA eps-hat alpha 1/256 (history, reported only) | 95.79 | 1630.7 | 1641.5 | 122.24 | 122.24 | 0.00973 | 1252 (74 / 1178) | 315.8 (48.7) | 1667.1 | 0/12 | 83.8 | 264.9 | 1159.2 | 0.55010 | 0.00608 | 0.03054 | 0.08247 |
| L10 | S0 reference | 234.53 | 1782.6 | 1805.6 | 285.80 | 285.80 | 0.00000 | 0 (0 / 0) | 0.0 (0.0) | 1834.0 | 0/12 | 0.0 | 346.0 | 0.0 | 0.58212 | 0.00000 | 0.00000 | 0.00000 |

Tail-only waits (the 347 tail seqs; ms): mean v / median v
- I1 R0e: 1481.7 / 1488.8
- I1 R0c: 1481.5 / 1488.6
- I1 R1: 1492.6 / 1499.9
- I1 U: 1477.7 / 1484.1
- I1 R0cum: 1481.7 / 1488.8
- I1 R0ccum: 1481.5 / 1488.6
- I1 R0x: 1481.8 / 1488.8
- I2 R0e: 1486.6 / 1492.7
- I2 R0c: 1484.0 / 1491.0
- I2 R1: 1508.6 / 1514.7
- I2 U: 1481.3 / 1486.7
- I2 R0cum: 1486.6 / 1492.7
- I2 R0ccum: 1484.0 / 1491.0
- I2 R0x: 1486.9 / 1493.0
- L10 R0e: 1623.2 / 1629.8
- L10 R0c: 1619.4 / 1627.5
- L10 R1: 1726.8 / 1743.9
- L10 U: 1536.2 / 1532.7
- L10 R0cum: 1622.8 / 1629.2
- L10 R0ccum: 1618.6 / 1626.9
- L10 R0x: 1622.9 / 1629.4

### Part T readings: paired per-seed differences, 95 percent bootstrap CI against m = max(T, NF)

| cell | contrast | metric | difference [CI] | m | reading |
|---|---|---|---|---|---|
| I1 | R0e - R0c | v_p50 | 0 [0, 0] | 1 | equivalent |
| I1 | R0e - R0c | v_p99 | 0.2395 [0.1366, 0.383] | 81.2 | equivalent |
| I1 | R0e - R0c | v_p999 | 0.485 [0.03978, 1.302] | 82 | equivalent |
| I1 | R0e - R0c | v_mean | 0.2365 [0.1756, 0.3147] | 3.03 | equivalent |
| I1 | R0e - R0c | e_mean | 0.2365 [0.1756, 0.3147] | 3.03 | equivalent |
| I1 | R0e - R0c | G_ratio | -0.0001578 [-0.0002127, -0.0001166] | 0.00183 | equivalent |
| I1 | R0e - R0c | max_ep_spend | -1.667 [-2, -1.333] | 9.35 | equivalent |
| I1 | R0e - R0c | repair_per_delivered | 0.007902 [0.006319, 0.00949] | 0.0844 | equivalent |
| I1 | R0e - R0c | tail_time | 0 [0, 0] | 85.1 | equivalent |
| I1 | R0e - R1 | v_p50 | 0 [0, 0] | 1 | equivalent |
| I1 | R0e - R1 | v_p99 | -11.21 [-19.51, -4.818] | 81.2 | equivalent |
| I1 | R0e - R1 | v_p999 | -12.29 [-21.48, -5.323] | 82 | equivalent |
| I1 | R0e - R1 | v_mean | -2.218 [-2.381, -2.074] | 3.03 | equivalent |
| I1 | R0e - R1 | e_mean | -2.218 [-2.381, -2.074] | 3.03 | equivalent |
| I1 | R0e - R1 | G_ratio | -6.86e-06 [-5.488e-05, 4.116e-05] | 0.00183 | equivalent |
| I1 | R0e - R1 | max_ep_spend | 4.917 [3.75, 6.167] | 9.35 | equivalent |
| I1 | R0e - R1 | repair_per_delivered | 0.003173 [0.001195, 0.005162] | 0.0844 | equivalent |
| I1 | R0e - R1 | tail_time | -13.08 [-22.04, -5.805] | 85.1 | equivalent |
| I1 | R0c - R1 | v_p50 | 0 [0, 0] | 1 | equivalent |
| I1 | R0c - R1 | v_p99 | -11.45 [-19.77, -5.033] | 81.2 | equivalent |
| I1 | R0c - R1 | v_p999 | -12.78 [-21.94, -5.786] | 82 | equivalent |
| I1 | R0c - R1 | v_mean | -2.455 [-2.665, -2.276] | 3.03 | equivalent |
| I1 | R0c - R1 | e_mean | -2.455 [-2.665, -2.276] | 3.03 | equivalent |
| I1 | R0c - R1 | G_ratio | 0.0001509 [8.232e-05, 0.0002333] | 0.00183 | equivalent |
| I1 | R0c - R1 | max_ep_spend | 6.583 [5.5, 7.75] | 9.35 | equivalent |
| I1 | R0c - R1 | repair_per_delivered | -0.004729 [-0.006696, -0.002759] | 0.0844 | equivalent |
| I1 | R0c - R1 | tail_time | -13.08 [-22.04, -5.805] | 85.1 | equivalent |
| I1 | R0e - U | v_p50 | 0 [0, 0] | 1 | equivalent |
| I1 | R0e - U | v_p99 | 4.806 [4.088, 5.219] | 81.2 | equivalent |
| I1 | R0e - U | v_p999 | 2.088 [1.183, 3.071] | 82 | equivalent |
| I1 | R0e - U | v_mean | 0.678 [0.592, 0.7666] | 3.03 | equivalent |
| I1 | R0e - U | e_mean | 0.678 [0.592, 0.7666] | 3.03 | equivalent |
| I1 | R0e - U | G_ratio | -6.86e-06 [-5.488e-05, 4.116e-05] | 0.00183 | equivalent |
| I1 | R0e - U | max_ep_spend | 14.83 [3.75, 35.75] | 9.35 | undetermined |
| I1 | R0e - U | repair_per_delivered | 0.003173 [0.001195, 0.005162] | 0.0844 | equivalent |
| I1 | R0e - U | tail_time | -13.08 [-22.04, -5.805] | 85.1 | equivalent |
| I1 | R0c - U | v_p50 | 0 [0, 0] | 1 | equivalent |
| I1 | R0c - U | v_p99 | 4.567 [3.86, 5.003] | 81.2 | equivalent |
| I1 | R0c - U | v_p999 | 1.603 [0.7116, 2.628] | 82 | equivalent |
| I1 | R0c - U | v_mean | 0.4415 [0.3084, 0.5622] | 3.03 | equivalent |
| I1 | R0c - U | e_mean | 0.4415 [0.3084, 0.5622] | 3.03 | equivalent |
| I1 | R0c - U | G_ratio | 0.0001509 [8.232e-05, 0.0002333] | 0.00183 | equivalent |
| I1 | R0c - U | max_ep_spend | 16.5 [5.5, 37.5] | 9.35 | undetermined |
| I1 | R0c - U | repair_per_delivered | -0.004729 [-0.006696, -0.002759] | 0.0844 | equivalent |
| I1 | R0c - U | tail_time | -13.08 [-22.04, -5.805] | 85.1 | equivalent |
| I1 | R1 - U | v_p50 | 0 [0, 0] | 1 | equivalent |
| I1 | R1 - U | v_p99 | 16.02 [9.378, 24.47] | 81.2 | equivalent |
| I1 | R1 - U | v_p999 | 14.38 [7.353, 23.26] | 82 | equivalent |
| I1 | R1 - U | v_mean | 2.896 [2.784, 3.004] | 3.03 | equivalent |
| I1 | R1 - U | e_mean | 2.896 [2.784, 3.004] | 3.03 | equivalent |
| I1 | R1 - U | G_ratio | 0 [0, 0] | 0.00183 | equivalent |
| I1 | R1 - U | max_ep_spend | 9.917 [0, 29.75] | 9.35 | undetermined |
| I1 | R1 - U | repair_per_delivered | 0 [0, 0] | 0.0844 | equivalent |
| I1 | R1 - U | tail_time | 0 [0, 0] | 85.1 | equivalent |
| I2 | R0e - R0c | v_p50 | 0.009626 [0.008398, 0.01101] | 1 | equivalent |
| I2 | R0e - R0c | v_p99 | 3.439 [3.223, 3.641] | 81.2 | equivalent |
| I2 | R0e - R0c | v_p999 | 3.251 [2.583, 3.906] | 82 | equivalent |
| I2 | R0e - R0c | v_mean | 4.091 [3.864, 4.329] | 3.03 | shown larger |
| I2 | R0e - R0c | e_mean | 4.091 [3.864, 4.329] | 3.03 | shown larger |
| I2 | R0e - R0c | G_ratio | -0.002737 [-0.002895, -0.002586] | 0.00183 | shown smaller |
| I2 | R0e - R0c | max_ep_spend | -30.75 [-32.83, -28.92] | 9.35 | shown smaller |
| I2 | R0e - R0c | repair_per_delivered | 0.161 [0.1527, 0.1698] | 0.0859 | shown larger |
| I2 | R0e - R0c | tail_time | 0 [0, 0] | 85.1 | equivalent |
| I2 | R0e - R1 | v_p50 | -4.682 [-4.767, -4.593] | 1 | shown smaller |
| I2 | R0e - R1 | v_p99 | -21.82 [-35.67, -12.97] | 81.2 | equivalent |
| I2 | R0e - R1 | v_p999 | -20.2 [-34.6, -10.91] | 82 | equivalent |
| I2 | R0e - R1 | v_mean | -5.087 [-6.006, -4.402] | 3.03 | shown smaller |
| I2 | R0e - R1 | e_mean | -5.087 [-6.006, -4.402] | 3.03 | shown smaller |
| I2 | R0e - R1 | G_ratio | 0.000343 [1.372e-05, 0.0007823] | 0.00183 | equivalent |
| I2 | R0e - R1 | max_ep_spend | 22.75 [19.83, 25.5] | 9.35 | shown larger |
| I2 | R0e - R1 | repair_per_delivered | 0.01158 [0.007159, 0.01611] | 0.0859 | equivalent |
| I2 | R0e - R1 | tail_time | -24.14 [-38.37, -14.3] | 85.1 | equivalent |
| I2 | R0c - R1 | v_p50 | -4.692 [-4.776, -4.603] | 1 | shown smaller |
| I2 | R0c - R1 | v_p99 | -25.26 [-39.01, -16.56] | 81.2 | equivalent |
| I2 | R0c - R1 | v_p999 | -23.45 [-38.06, -14.13] | 82 | equivalent |
| I2 | R0c - R1 | v_mean | -9.179 [-9.987, -8.529] | 3.03 | shown smaller |
| I2 | R0c - R1 | e_mean | -9.179 [-9.987, -8.529] | 3.03 | shown smaller |
| I2 | R0c - R1 | G_ratio | 0.00308 [0.002765, 0.003451] | 0.00183 | shown larger |
| I2 | R0c - R1 | max_ep_spend | 53.5 [50.83, 56.75] | 9.35 | shown larger |
| I2 | R0c - R1 | repair_per_delivered | -0.1494 [-0.1572, -0.1421] | 0.0859 | shown smaller |
| I2 | R0c - R1 | tail_time | -24.14 [-38.37, -14.3] | 85.1 | equivalent |
| I2 | R0e - U | v_p50 | 4.702 [4.569, 4.835] | 1 | shown larger |
| I2 | R0e - U | v_p99 | 5.783 [5.246, 6.291] | 81.2 | equivalent |
| I2 | R0e - U | v_p999 | 0.3103 [-6.343, 4.935] | 82 | equivalent |
| I2 | R0e - U | v_mean | 3.365 [2.661, 3.9] | 3.03 | undetermined |
| I2 | R0e - U | e_mean | 3.365 [2.661, 3.9] | 3.03 | undetermined |
| I2 | R0e - U | G_ratio | 0.000343 [1.372e-05, 0.0007823] | 0.00183 | equivalent |
| I2 | R0e - U | max_ep_spend | 30.5 [20, 47.75] | 9.35 | shown larger |
| I2 | R0e - U | repair_per_delivered | 0.01158 [0.007159, 0.01611] | 0.0859 | equivalent |
| I2 | R0e - U | tail_time | -24.14 [-38.37, -14.3] | 85.1 | equivalent |
| I2 | R0c - U | v_p50 | 4.693 [4.559, 4.825] | 1 | shown larger |
| I2 | R0c - U | v_p99 | 2.344 [1.819, 2.868] | 81.2 | equivalent |
| I2 | R0c - U | v_p999 | -2.941 [-9.746, 1.736] | 82 | equivalent |
| I2 | R0c - U | v_mean | -0.7268 [-1.335, -0.2054] | 3.03 | equivalent |
| I2 | R0c - U | e_mean | -0.7268 [-1.335, -0.2054] | 3.03 | equivalent |
| I2 | R0c - U | G_ratio | 0.00308 [0.002765, 0.003451] | 0.00183 | shown larger |
| I2 | R0c - U | max_ep_spend | 61.25 [51.08, 77.75] | 9.35 | shown larger |
| I2 | R0c - U | repair_per_delivered | -0.1494 [-0.1572, -0.1421] | 0.0859 | shown smaller |
| I2 | R0c - U | tail_time | -24.14 [-38.37, -14.3] | 85.1 | equivalent |
| I2 | R1 - U | v_p50 | 9.384 [9.315, 9.445] | 1 | shown larger |
| I2 | R1 - U | v_p99 | 27.61 [18.98, 41.26] | 81.2 | equivalent |
| I2 | R1 - U | v_p999 | 20.51 [14.43, 29.32] | 82 | equivalent |
| I2 | R1 - U | v_mean | 8.452 [8.237, 8.761] | 3.03 | shown larger |
| I2 | R1 - U | e_mean | 8.452 [8.237, 8.761] | 3.03 | shown larger |
| I2 | R1 - U | G_ratio | 0 [0, 0] | 0.00183 | equivalent |
| I2 | R1 - U | max_ep_spend | 7.75 [0, 23.25] | 9.35 | undetermined |
| I2 | R1 - U | repair_per_delivered | 0 [0, 0] | 0.0859 | equivalent |
| I2 | R1 - U | tail_time | 0 [0, 0] | 85.1 | equivalent |
| L10 | R0e - R0c | v_p50 | 0.001641 [-0.03807, 0.0317] | 1 | equivalent |
| L10 | R0e - R0c | v_p99 | 4.356 [3.282, 5.532] | 81.2 | equivalent |
| L10 | R0e - R0c | v_p999 | 4.256 [3.27, 5.309] | 82 | equivalent |
| L10 | R0e - R0c | v_mean | 5.202 [4.158, 6.435] | 14.3 | equivalent |
| L10 | R0e - R0c | e_mean | 5.202 [4.158, 6.435] | 14.3 | equivalent |
| L10 | R0e - R0c | G_ratio | -0.002833 [-0.003327, -0.002442] | 0.00183 | shown smaller |
| L10 | R0e - R0c | max_ep_spend | 1 [-0.4167, 2.25] | 9.35 | equivalent |
| L10 | R0e - R0c | repair_per_delivered | 0.04281 [0.03525, 0.05145] | 0.0291 | shown larger |
| L10 | R0e - R0c | tail_time | 0.2349 [-0.0005639, 0.6702] | 85.1 | equivalent |
| L10 | R0e - R1 | v_p50 | -35.81 [-38.72, -32.66] | 1 | shown smaller |
| L10 | R0e - R1 | v_p99 | -120.1 [-151, -88.58] | 81.2 | shown smaller |
| L10 | R0e - R1 | v_p999 | -133 [-159.1, -107.6] | 82 | shown smaller |
| L10 | R0e - R1 | v_mean | -43.51 [-44.8, -42.19] | 14.3 | shown smaller |
| L10 | R0e - R1 | e_mean | -43.51 [-44.8, -42.19] | 14.3 | shown smaller |
| L10 | R0e - R1 | G_ratio | -0.0002264 [-0.0003567, -9.605e-05] | 0.00183 | equivalent |
| L10 | R0e - R1 | max_ep_spend | 1004 [988.1, 1019] | 9.35 | shown larger |
| L10 | R0e - R1 | repair_per_delivered | 0.008658 [0.00468, 0.0126] | 0.0291 | equivalent |
| L10 | R0e - R1 | tail_time | -141.3 [-166.6, -116.4] | 85.1 | shown smaller |
| L10 | R0c - R1 | v_p50 | -35.81 [-38.75, -32.64] | 1 | shown smaller |
| L10 | R0c - R1 | v_p99 | -124.5 [-155.9, -92.6] | 81.2 | shown smaller |
| L10 | R0c - R1 | v_p999 | -137.2 [-163.7, -111.6] | 82 | shown smaller |
| L10 | R0c - R1 | v_mean | -48.71 [-50.56, -46.83] | 14.3 | shown smaller |
| L10 | R0c - R1 | e_mean | -48.71 [-50.56, -46.83] | 14.3 | shown smaller |
| L10 | R0c - R1 | G_ratio | 0.002607 [0.002285, 0.003005] | 0.00183 | shown larger |
| L10 | R0c - R1 | max_ep_spend | 1003 [987.9, 1017] | 9.35 | shown larger |
| L10 | R0c - R1 | repair_per_delivered | -0.03416 [-0.04015, -0.02902] | 0.0291 | undetermined |
| L10 | R0c - R1 | tail_time | -141.6 [-166.6, -116.6] | 85.1 | shown smaller |
| L10 | R0e - U | v_p50 | 94.76 [94.58, 94.93] | 1 | shown larger |
| L10 | R0e - U | v_p99 | 98.24 [97.37, 99.25] | 81.2 | shown larger |
| L10 | R0e - U | v_p999 | 12.96 [10.65, 15.06] | 82 | equivalent |
| L10 | R0e - U | v_mean | 55.89 [54.95, 57.06] | 14.3 | shown larger |
| L10 | R0e - U | e_mean | 55.89 [54.95, 57.06] | 14.3 | shown larger |
| L10 | R0e - U | G_ratio | -0.0002264 [-0.0003567, -9.605e-05] | 0.00183 | equivalent |
| L10 | R0e - U | max_ep_spend | 1004 [988.1, 1019] | 9.35 | shown larger |
| L10 | R0e - U | repair_per_delivered | 0.008658 [0.00468, 0.0126] | 0.0291 | equivalent |
| L10 | R0e - U | tail_time | -141.3 [-166.6, -116.4] | 85.1 | shown smaller |
| L10 | R0c - U | v_p50 | 94.76 [94.6, 94.92] | 1 | shown larger |
| L10 | R0c - U | v_p99 | 93.88 [93.33, 94.38] | 81.2 | shown larger |
| L10 | R0c - U | v_p999 | 8.707 [6.686, 10.63] | 82 | equivalent |
| L10 | R0c - U | v_mean | 50.69 [50.1, 51.29] | 14.3 | shown larger |
| L10 | R0c - U | e_mean | 50.69 [50.1, 51.29] | 14.3 | shown larger |
| L10 | R0c - U | G_ratio | 0.002607 [0.002285, 0.003005] | 0.00183 | shown larger |
| L10 | R0c - U | max_ep_spend | 1003 [987.9, 1017] | 9.35 | shown larger |
| L10 | R0c - U | repair_per_delivered | -0.03416 [-0.04015, -0.02902] | 0.0291 | undetermined |
| L10 | R0c - U | tail_time | -141.6 [-166.6, -116.6] | 85.1 | shown smaller |
| L10 | R1 - U | v_p50 | 130.6 [127.4, 133.5] | 1 | shown larger |
| L10 | R1 - U | v_p99 | 218.4 [186.4, 250] | 81.2 | shown larger |
| L10 | R1 - U | v_p999 | 146 [120.2, 172.6] | 82 | shown larger |
| L10 | R1 - U | v_mean | 99.4 [97.78, 101.1] | 14.3 | shown larger |
| L10 | R1 - U | e_mean | 99.4 [97.78, 101.1] | 14.3 | shown larger |
| L10 | R1 - U | G_ratio | 0 [0, 0] | 0.00183 | equivalent |
| L10 | R1 - U | max_ep_spend | 0 [0, 0] | 9.35 | equivalent |
| L10 | R1 - U | repair_per_delivered | 0 [0, 0] | 0.0291 | equivalent |
| L10 | R1 - U | tail_time | 0 [0, 0] | 85.1 | equivalent |

NF for Part T comes from the A/A on the I2 cell (R0e and R1) and is applied to all three cells.

### Replication, phase scatter and the AG1 n_dec split

### Replication of the I2 tail A/A with fresh phase draws (variants 2 and 3, 12 seeds)

| receiver, eps-hat | metric | C - D mean [CI] | T | reading |
|---|---|---|---|---|
| R0e engine | v_p99 | 2.36 [-2.134, 7.163] | 1 | undetermined |
| R0e engine | v_p999 | 2.306 [-2.248, 7.13] | 1 | undetermined |
| R0e engine | e_p99 | 2.36 [-2.134, 7.163] | 1 | undetermined |
| R0e engine | e_p999 | 2.306 [-2.248, 7.13] | 1 | undetermined |
| R0e engine | tail_time | 5.585 [0.374, 11.25] | 1 | undetermined |
| R0e engine | tail_v_mean | 2.326 [-2.145, 7.089] | 1 | undetermined |
| R0e engine | tail_v_p50 | 2.487 [-2.005, 7.307] | 1 | undetermined |
| R0e cum | v_p99 | 2.36 [-2.134, 7.163] | 1 | undetermined |
| R0e cum | v_p999 | 2.306 [-2.248, 7.13] | 1 | undetermined |
| R0e cum | e_p99 | 2.36 [-2.134, 7.163] | 1 | undetermined |
| R0e cum | e_p999 | 2.306 [-2.248, 7.13] | 1 | undetermined |
| R0e cum | tail_time | 5.585 [0.374, 11.25] | 1 | undetermined |
| R0e cum | tail_v_mean | 2.326 [-2.145, 7.089] | 1 | undetermined |
| R0e cum | tail_v_p50 | 2.487 [-2.005, 7.307] | 1 | undetermined |
| R1 engine | v_p99 | 10.56 [-12.71, 32.57] | 1 | undetermined |
| R1 engine | v_p999 | 10.37 [-12.88, 32.37] | 1 | undetermined |
| R1 engine | e_p99 | 10.56 [-12.71, 32.57] | 1 | undetermined |
| R1 engine | e_p999 | 10.37 [-12.88, 32.37] | 1 | undetermined |
| R1 engine | tail_time | 12.32 [-11.19, 34.3] | 1 | undetermined |
| R1 engine | tail_v_mean | 10.34 [-12.03, 31.53] | 1 | undetermined |
| R1 engine | tail_v_p50 | 10.54 [-12.63, 32.46] | 1 | undetermined |

Readings shown across the replication: 0

### Phase scatter of tail time (ms): mean over 12 seeds per phase set, and per-seed SD of the difference between two sets

- R0e engine: variant 2 mean 1495.2, variant 3 mean 1489.6; SD of per-seed difference 10.0
- R0e cum: variant 2 mean 1495.2, variant 3 mean 1489.6; SD of per-seed difference 10.0
- R1 engine: variant 2 mean 1524.0, variant 3 mean 1511.7; SD of per-seed difference 41.3
- R0e (frozen A/A, cumulative eps-hat for R0e): variant 0 mean 1488.0, variant 1 mean 1495.5; SD of per-seed difference 9.5
- R1 (frozen A/A, cumulative eps-hat for R0e): variant 0 mean 1512.1, variant 1 mean 1522.6; SD of per-seed difference 41.8

### AG1 at Realtime under D_EWMA: n_dec before and after the first RTT sample (summed over 12 seeds)

- I1: n_dec 6, of which in the seed era (D = 25 ms) 6, after the first sample 0
- I2: n_dec 48, of which in the seed era (D = 25 ms) 13, after the first sample 35
- Q: n_dec 22, of which in the seed era (D = 25 ms) 6, after the first sample 16

### Part B tables

### P3 power: HZ and the planted controls (U at Auto, I1, omega = 1, rho = 1)

K and P are per-seed statistics of the five points at 0.98, 0.99, 1, 1.01, 1.02 times Auto. Reading against m = max(T, NF).

| control | readout | K (mean [CI]) | K reading | P (mean [CI]) | P reading | outcome | required |
|---|---|---|---|---|---|---|---|
| HZ | rbf | 0.966 [0.961, 0.97] | shown larger | -0.483 [-0.485, -0.481] | shown smaller | fail | fail on rbf and e mean |
| HZ | e_mean | -2.74 [-2.77, -2.71] | shown smaller | 1.37 [1.35, 1.39] | shown larger | fail | fail on rbf and e mean |
| HZ | e_p99 | -11.8 [-11.8, -11.8] | shown smaller | 5.91 [5.9, 5.92] | shown larger | fail | fail on rbf and e mean |
| STEP | rbf | 0.0106 [0.00866, 0.0125] | shown larger | 0.00519 [0.00451, 0.00591] | shown larger | fail | fail on K |
| STEP | e_mean | -0.0136 [-0.0195, -0.00796] | equivalent | -0.00725 [-0.0101, -0.00453] | equivalent | pass | fail on K |
| STEP | e_p99 | -0.467 [-0.67, -0.275] | equivalent | -0.243 [-0.344, -0.149] | equivalent | pass | fail on K |
| SPIKE | rbf | 0.000339 [-0.00152, 0.00223] | equivalent | -0.0104 [-0.012, -0.00887] | shown smaller | fail | fail on P, K passes |
| SPIKE | e_mean | -9.04e-06 [-0.000187, 0.000178] | equivalent | 0.0136 [0.00798, 0.0195] | equivalent | pass | fail on P, K passes |
| SPIKE | e_p99 | -0.00915 [-0.0167, -0.000481] | equivalent | 0.463 [0.27, 0.667] | equivalent | pass | fail on P, K passes |
| KINK | rbf | 0.00988 [0.00811, 0.0118] | undetermined | 0.00485 [0.00395, 0.00571] | shown larger | fail | pass both |
| KINK | e_mean | -0.00448 [-0.00561, -0.00334] | equivalent | -0.00269 [-0.00328, -0.00209] | equivalent | pass | pass both |
| KINK | e_p99 | -0.215 [-0.291, -0.142] | equivalent | -0.117 [-0.157, -0.08] | equivalent | pass | pass both |

- HZ fails P3 on rbf and mean e: **yes**. P3 has power.
- STEP fails on K (rbf or mean e): **True**; SPIKE fails on P with K passing: **True**; KINK passes both K and P on rbf and mean e: **False**.
KINK curvature follow-up: intercept a of Delta(h) = a + c h + k h^2 from widths 0.005, 0.01, 0.02, per seed
- rbf: a 0.00486 [0.0038, 0.00605] vs m=0.00382: undetermined
- e_mean: a -0.00273 [-0.00334, -0.00214] vs m=0.149: equivalent

### P3 for U at the three presets, I1 (12 seeds; K and P each read against m = max(T, NF))

Cells show K/P readings: E equivalent, S shown, ? undetermined. Readouts: e mean, e p99, rbf, G_rx/S_src, repair sends per delivered.

| preset | omega | e mean | e p99 | rbf | G_rx/S_src | repair/delivered | verdict |
|---|---|---|---|---|---|---|---|
| Bulk | 0 | E/E | E/E | E/E | E/E | E/E | pass |
| Bulk | 0.3 | E/E | E/E | E/E | E/E | E/E | pass |
| Bulk | 1 | E/E | E/E | E/E | E/E | E/E | pass |
| Bulk | 3 | E/E | E/E | E/E | E/E | E/E | pass |
| Bulk | 10 | E/E | E/E | E/E | E/E | E/E | pass |
| Bulk | 100 | E/E | E/E | E/E | E/E | E/E | pass |
| Auto | 0 | E/E | E/E | E/E | E/E | E/E | pass |
| Auto | 0.3 | E/E | E/E | E/E | E/E | E/E | pass |
| Auto | 1 | E/E | E/E | E/E | E/E | E/E | pass |
| Auto | 3 | E/E | E/E | E/E | E/E | E/E | pass |
| Auto | 10 | E/E | E/E | E/E | E/E | E/E | pass |
| Auto | 100 | E/E | E/E | E/E | E/E | E/E | pass |
| Realtime | 0 | E/E | E/E | E/E | E/E | E/E | pass |
| Realtime | 0.3 | E/E | E/E | E/E | E/E | E/E | pass |
| Realtime | 1 | E/E | E/E | E/E | E/E | E/E | pass |
| Realtime | 3 | E/E | E/E | E/E | E/E | E/E | pass |
| Realtime | 10 | E/E | E/E | E/E | E/E | E/E | pass |
| Realtime | 100 | E/E | E/E | E/E | E/E | E/E | pass |

Readouts that did not pass (K and P mean, CI, bar):


### P3 on rho (nudges on 1 - rho, Realtime delta, I2; bars as the Realtime preset)

- omega 1, e_mean: K -1.13e-05 [-2.58e-05, -1.41e-06] vs m=0.604: equivalent; P -5.64e-06 [-1.29e-05, -7.03e-07] vs m=0.604: equivalent; **pass**
- omega 1, e_p99: K -2.72e-05 [-8.17e-05, 0] vs m=1: equivalent; P -1.36e-05 [-4.09e-05, 0] vs m=1: equivalent; **pass**
- omega 1, rbf: K 0 [0, 0] vs m=0.01: equivalent; P 0 [0, 0] vs m=0.01: equivalent; **pass**
- omega 1, G_ratio: K 4.17e-07 [0, 1.25e-06] vs m=0.0005: equivalent; P 2.08e-07 [0, 6.25e-07] vs m=0.0005: equivalent; **pass**
- omega 1, repair_per_delivered: K -4.05e-07 [-3.34e-06, 2.1e-06] vs m=0.00166: equivalent; P -6.2e-07 [-2.08e-06, 6.4e-07] vs m=0.00166: equivalent; **pass**
- omega 10, e_mean: K -0.000116 [-0.000209, -2.33e-05] vs m=0.604: equivalent; P -5.79e-05 [-0.000105, -1.16e-05] vs m=0.604: equivalent; **pass**
- omega 10, e_p99: K -2.72e-05 [-8.17e-05, 0] vs m=1: equivalent; P -1.36e-05 [-4.09e-05, 0] vs m=1: equivalent; **pass**
- omega 10, rbf: K 0 [0, 0] vs m=0.01: equivalent; P 0 [0, 0] vs m=0.01: equivalent; **pass**
- omega 10, G_ratio: K 4.17e-07 [0, 1.25e-06] vs m=0.0005: equivalent; P 2.08e-07 [0, 6.25e-07] vs m=0.0005: equivalent; **pass**
- omega 10, repair_per_delivered: K -4.05e-07 [-3.34e-06, 2.1e-06] vs m=0.00166: equivalent; P -6.2e-07 [-2.08e-06, 6.4e-07] vs m=0.00166: equivalent; **pass**

### rbf (released then filled, per hole) per omega against the bar epsilon_cell^2, level reading per seed

| cell | preset | w=0 | w=0.3 | w=1 | w=3 | w=10 | w=100 |
|---|---|---|---|---|---|---|---|
| I1 (bar 0.00063) | Bulk | 1 [1, 1] breaks | 0 [0, 0] KEEPS ORDER | 0 [0, 0] KEEPS ORDER | 0 [0, 0] KEEPS ORDER | 0 [0, 0] KEEPS ORDER | 0 [0, 0] KEEPS ORDER |
| I1 (bar 0.00063) | Auto | 1 [1, 1] breaks | 1 [1, 1] breaks | 0.01 [0.0091, 0.012] breaks | 0 [0, 0] KEEPS ORDER | 0 [0, 0] KEEPS ORDER | 0 [0, 0] KEEPS ORDER |
| I1 (bar 0.00063) | Realtime | 1 [1, 1] breaks | 1 [1, 1] breaks | 1 [1, 1] breaks | 1 [1, 1] breaks | 1 [1, 1] breaks | 0.00034 [0, 0.00086] ? |
| I2 (bar 0.01) | Bulk | 1 [1, 1] breaks | 4.2e-05 [0, 8.4e-05] KEEPS ORDER | 0 [0, 0] KEEPS ORDER | 0 [0, 0] KEEPS ORDER | 0 [0, 0] KEEPS ORDER | 0 [0, 0] KEEPS ORDER |
| I2 (bar 0.01) | Auto | 1 [1, 1] breaks | 1 [1, 1] breaks | 0.01 [0.0099, 0.011] ? | 4.2e-05 [0, 8.4e-05] KEEPS ORDER | 4.2e-05 [0, 8.4e-05] KEEPS ORDER | 0 [0, 0] KEEPS ORDER |
| I2 (bar 0.01) | Realtime | 1 [1, 1] breaks | 1 [1, 1] breaks | 1 [1, 1] breaks | 1 [1, 1] breaks | 1 [1, 1] breaks | 0.00042 [0.00032, 0.00053] KEEPS ORDER |
| G (bar 0.00063) | Bulk | 1 [1, 1] breaks | 0 [0, 0] KEEPS ORDER | 0 [0, 0] KEEPS ORDER | 0 [0, 0] KEEPS ORDER | 0 [0, 0] KEEPS ORDER | 0 [0, 0] KEEPS ORDER |
| G (bar 0.00063) | Auto | 1 [1, 1] breaks | 1 [1, 1] breaks | 0.012 [0.011, 0.013] breaks | 0 [0, 0] KEEPS ORDER | 0 [0, 0] KEEPS ORDER | 0 [0, 0] KEEPS ORDER |
| G (bar 0.00063) | Realtime | 1 [1, 1] breaks | 1 [1, 1] breaks | 1 [1, 1] breaks | 1 [1, 1] breaks | 1 [1, 1] breaks | 0.0015 [0.0011, 0.0019] breaks |

### ratio_U = cost_U / cost_U* (d moves in these runs, holes before the first d sample pay no H_o)

| cell | preset | omega | ratio_U mean (min..max) | isolated holes: cost_U / cost_U* | isolated runs above 2 + omega N_inf / cost_U* |
|---|---|---|---|---|---|
| I1 | Bulk | 0 | n/a (no cost) | n/a | 8 of 12 |
| I1 | Bulk | 0.3 | 1.000 (1.000..1.000) | 1.000 | 0 of 12 |
| I1 | Bulk | 1 | 1.000 (1.000..1.000) | 1.000 | 0 of 12 |
| I1 | Bulk | 3 | 1.000 (1.000..1.000) | 1.000 | 0 of 12 |
| I1 | Bulk | 10 | 1.000 (1.000..1.000) | 1.000 | 0 of 12 |
| I1 | Bulk | 100 | 1.000 (1.000..1.000) | 1.000 | 0 of 12 |
| I1 | Auto | 0 | n/a (no cost) | n/a | 8 of 12 |
| I1 | Auto | 0.3 | 1.945 (1.934..1.953) | 1.999 | 5 of 12 |
| I1 | Auto | 1 | 1.027 (1.016..1.037) | 1.014 | 0 of 12 |
| I1 | Auto | 3 | 1.000 (1.000..1.000) | 1.000 | 0 of 12 |
| I1 | Auto | 10 | 1.000 (1.000..1.000) | 1.000 | 0 of 12 |
| I1 | Auto | 100 | 1.000 (1.000..1.000) | 1.000 | 0 of 12 |
| I1 | Realtime | 0 | n/a (no cost) | n/a | 8 of 12 |
| I1 | Realtime | 0.3 | 2.249 (2.190..2.378) | 2.386 | 9 of 12 |
| I1 | Realtime | 1 | 2.083 (2.064..2.118) | 2.128 | 9 of 12 |
| I1 | Realtime | 3 | 2.026 (2.013..2.034) | 2.055 | 9 of 12 |
| I1 | Realtime | 10 | 1.972 (1.953..1.983) | 2.029 | 9 of 12 |
| I1 | Realtime | 100 | 1.018 (1.010..1.024) | 1.023 | 0 of 12 |
| I2 | Bulk | 0 | n/a (no cost) | n/a | 0 of 12 |
| I2 | Bulk | 0.3 | 1.010 (1.000..1.039) | 1.000 | 0 of 12 |
| I2 | Bulk | 1 | 1.013 (1.000..1.050) | 1.000 | 0 of 12 |
| I2 | Bulk | 3 | 1.013 (1.000..1.050) | 1.000 | 0 of 12 |
| I2 | Bulk | 10 | 1.013 (1.000..1.050) | 1.000 | 0 of 12 |
| I2 | Bulk | 100 | 1.013 (1.000..1.050) | 1.000 | 0 of 12 |
| I2 | Auto | 0 | n/a (no cost) | n/a | 0 of 12 |
| I2 | Auto | 0.3 | 3.280 (3.249..3.311) | 2.002 | 7 of 12 |
| I2 | Auto | 1 | 1.081 (1.069..1.099) | 1.000 | 0 of 12 |
| I2 | Auto | 3 | 1.001 (1.000..1.004) | 1.000 | 0 of 12 |
| I2 | Auto | 10 | 1.003 (1.000..1.013) | 1.000 | 0 of 12 |
| I2 | Auto | 100 | 1.013 (1.000..1.050) | 1.000 | 0 of 12 |
| I2 | Realtime | 0 | n/a (no cost) | n/a | 0 of 12 |
| I2 | Realtime | 0.3 | 2.132 (2.099..2.168) | 2.104 | 8 of 12 |
| I2 | Realtime | 1 | 2.088 (2.069..2.106) | 2.104 | 8 of 12 |
| I2 | Realtime | 3 | 2.014 (2.000..2.023) | 2.104 | 8 of 12 |
| I2 | Realtime | 10 | 1.817 (1.803..1.826) | 2.104 | 8 of 12 |
| I2 | Realtime | 100 | 1.013 (1.011..1.016) | 1.099 | 0 of 12 |
| G | Bulk | 0 | n/a (no cost) | n/a | 3 of 12 |
| G | Bulk | 0.3 | 1.000 (1.000..1.000) | 1.000 | 0 of 12 |
| G | Bulk | 1 | 1.000 (1.000..1.000) | 1.000 | 0 of 12 |
| G | Bulk | 3 | 1.000 (1.000..1.000) | 1.000 | 0 of 12 |
| G | Bulk | 10 | 1.000 (1.000..1.000) | 1.000 | 0 of 12 |
| G | Bulk | 100 | 1.000 (1.000..1.000) | 1.000 | 0 of 12 |
| G | Auto | 0 | n/a (no cost) | n/a | 3 of 12 |
| G | Auto | 0.3 | 4.368 (4.152..4.551) | 2.000 | 6 of 12 |
| G | Auto | 1 | 1.140 (1.088..1.178) | 1.000 | 0 of 12 |
| G | Auto | 3 | 1.000 (1.000..1.000) | 1.000 | 0 of 12 |
| G | Auto | 10 | 1.000 (1.000..1.000) | 1.000 | 0 of 12 |
| G | Auto | 100 | 1.000 (1.000..1.000) | 1.000 | 0 of 12 |
| G | Realtime | 0 | n/a (no cost) | n/a | 3 of 12 |
| G | Realtime | 0.3 | 1.619 (1.602..1.637) | 2.596 | 6 of 12 |
| G | Realtime | 1 | 1.571 (1.560..1.584) | 2.180 | 6 of 12 |
| G | Realtime | 3 | 1.412 (1.402..1.423) | 2.061 | 6 of 12 |
| G | Realtime | 10 | 1.867 (1.826..1.904) | 2.020 | 6 of 12 |
| G | Realtime | 100 | 1.006 (1.003..1.011) | 1.009 | 0 of 12 |

### d_all against d_rep at one path (P4g witness), I1, omega = 1

- Bulk: d_all == d_rep in every seed: True; the d_rep arm gives identical e mean and rbf: True
- Auto: d_all == d_rep in every seed: True; the d_rep arm gives identical e mean and rbf: True
- Realtime: d_all == d_rep in every seed: True; the d_rep arm gives identical e mean and rbf: True

### P2: rho = 1 and 1 - 1e-7, Realtime delta, I1, omega = 1

- P2-rho1: verdicts 0, floors 0, passes 0, G_rx at the end 0 (all seeds, summed)
- P2-rho1m: verdicts 0, floors 0, passes 0, G_rx at the end 0 (all seeds, summed)

P1 violations summed over every Part B run: 0
H estimator: largest H over every Part B run 0 s, late arrivals counted by the estimator 0, FIFO violations 0

### AG1: where the age gate can refuse (U at omega = 1, rho = 0.99; counts are sums over 12 seeds)

n_dec counts (hole, report) pairs where age <= D and the count would have admitted the verdict.

| cell | delta point | arm | n_dec | verdicts (seed-era) | gate refused, count full | cold-start refusals | e mean ms | e p99 ms | G_rx/S_src | repair/delivered |
|---|---|---|---|---|---|---|---|---|---|---|
| I1 | Realtime | RT | 0 | 983 (0.1) | 0 | 0 | 0.062 | 0.22 | 0.00011 | 0.00492 |
| I1 | Realtime | EWMA | 6 | 983 (0.0) | 0 | 0 | 0.062 | 0.22 | 0.00011 | 0.00492 |
| I1 | Realtime | ZERO | 0 | 983 (0.1) | 0 | 0 | 0.062 | 0.22 | 0.00011 | 0.00492 |
| I1 | Auto | RT | 0 | 983 (0.1) | 0 | 0 | 2.785 | 11.77 | 0.00011 | 0.00492 |
| I1 | Auto | EWMA | 3668 | 983 (0.0) | 306 | 0 | 2.790 | 11.79 | 0.00008 | 0.00634 |
| I1 | Auto | ZERO | 0 | 983 (0.1) | 0 | 0 | 2.785 | 11.77 | 0.00011 | 0.00492 |
| I1 | b115 | RT | 5325 | 983 (0.1) | 444 | 0 | 2.795 | 11.80 | 0.00006 | 0.00713 |
| I1 | b115 | EWMA | 13901 | 983 (0.0) | 1159 | 0 | 2.809 | 11.87 | 0.00001 | 0.00954 |
| I1 | b115 | ZERO | 0 | 983 (0.1) | 0 | 0 | 2.788 | 11.78 | 0.00011 | 0.00492 |
| I1 | b13 | RT | 14146 | 983 (0.1) | 1179 | 0 | 2.810 | 11.88 | 0.00000 | 0.00983 |
| I1 | b13 | EWMA | 24445 | 982 (0.0) | 2037 | 0 | 2.832 | 11.98 | 0.00000 | 0.00983 |
| I1 | b13 | ZERO | 0 | 983 (0.1) | 0 | 0 | 2.788 | 11.78 | 0.00011 | 0.00492 |
| I1 | Bulk | RT | 55501 | 982 (0.1) | 4625 | 0 | 2.924 | 12.78 | 0.00000 | 0.00983 |
| I1 | Bulk | EWMA | 68520 | 215 (0.0) | 5710 | 0 | 2.971 | 14.09 | 0.00000 | 0.00603 |
| I1 | Bulk | ZERO | 0 | 983 (0.1) | 0 | 0 | 2.788 | 11.78 | 0.00011 | 0.00492 |
| I2 | Realtime | RT | 0 | 5977 (0.2) | 0 | 0 | 0.333 | 11.87 | 0.00296 | 0.03003 |
| I2 | Realtime | EWMA | 48 | 5977 (0.0) | 4 | 0 | 0.333 | 11.87 | 0.00296 | 0.03005 |
| I2 | Realtime | ZERO | 0 | 5977 (0.2) | 0 | 0 | 0.333 | 11.87 | 0.00296 | 0.03003 |
| I2 | Auto | RT | 0 | 5977 (0.2) | 0 | 0 | 8.517 | 12.75 | 0.00296 | 0.03003 |
| I2 | Auto | EWMA | 316210 | 5706 (0.0) | 26362 | 0 | 10.280 | 21.02 | 0.00030 | 0.05843 |
| I2 | Auto | ZERO | 0 | 5977 (0.2) | 0 | 0 | 8.517 | 12.75 | 0.00296 | 0.03003 |
| I2 | b115 | RT | 32136 | 5977 (0.2) | 2685 | 0 | 8.581 | 12.80 | 0.00177 | 0.04341 |
| I2 | b115 | EWMA | 412129 | 1762 (0.0) | 34359 | 0 | 11.157 | 23.45 | 0.00021 | 0.03966 |
| I2 | b115 | ZERO | 0 | 5977 (0.2) | 0 | 0 | 8.520 | 12.78 | 0.00296 | 0.03003 |
| I2 | b13 | RT | 85932 | 5977 (0.2) | 7173 | 0 | 8.766 | 13.98 | 0.00030 | 0.05977 |
| I2 | b13 | EWMA | 439133 | 759 (0.0) | 36607 | 0 | 11.431 | 24.30 | 0.00006 | 0.03632 |
| I2 | b13 | ZERO | 0 | 5977 (0.2) | 0 | 0 | 8.520 | 12.78 | 0.00296 | 0.03003 |
| I2 | Bulk | RT | 336798 | 5976 (0.2) | 28072 | 0 | 10.435 | 20.97 | 0.00030 | 0.05978 |
| I2 | Bulk | EWMA | 474839 | 72 (0.0) | 39570 | 0 | 12.006 | 32.85 | 0.00000 | 0.03349 |
| I2 | Bulk | ZERO | 0 | 5977 (0.2) | 0 | 0 | 8.520 | 12.78 | 0.00296 | 0.03003 |
| Q | Realtime | RT | 0 | 983 (0.1) | 0 | 0 | 0.088 | 0.31 | 0.00011 | 0.00492 |
| Q | Realtime | EWMA | 22 | 983 (0.0) | 2 | 0 | 0.088 | 0.31 | 0.00011 | 0.00492 |
| Q | Realtime | ZERO | 0 | 983 (0.1) | 0 | 0 | 0.088 | 0.31 | 0.00011 | 0.00492 |
| Q | Auto | RT | 0 | 983 (0.1) | 0 | 0 | 5.346 | 19.72 | 0.00011 | 0.00492 |
| Q | Auto | EWMA | 7434 | 980 (0.0) | 620 | 0 | 5.359 | 19.76 | 0.00007 | 0.00688 |
| Q | Auto | ZERO | 0 | 983 (0.1) | 0 | 0 | 5.346 | 19.72 | 0.00011 | 0.00492 |
| Q | b115 | RT | 264 | 983 (0.1) | 22 | 0 | 5.348 | 19.72 | 0.00011 | 0.00503 |
| Q | b115 | EWMA | 22683 | 969 (0.0) | 1890 | 0 | 5.393 | 19.89 | 0.00000 | 0.00971 |
| Q | b115 | ZERO | 0 | 983 (0.1) | 0 | 0 | 5.348 | 19.72 | 0.00011 | 0.00492 |
| Q | b13 | RT | 1743 | 983 (0.1) | 145 | 0 | 5.352 | 19.72 | 0.00009 | 0.00561 |
| Q | b13 | EWMA | 38227 | 955 (0.0) | 3186 | 0 | 5.442 | 20.13 | 0.00000 | 0.00981 |
| Q | b13 | ZERO | 0 | 983 (0.1) | 0 | 0 | 5.348 | 19.72 | 0.00011 | 0.00492 |
| Q | Bulk | RT | 26190 | 982 (0.1) | 2183 | 0 | 5.408 | 19.84 | 0.00003 | 0.00903 |
| Q | Bulk | EWMA | 95749 | 157 (0.0) | 7979 | 0 | 5.575 | 21.15 | 0.00000 | 0.00967 |
| Q | Bulk | ZERO | 0 | 983 (0.1) | 0 | 0 | 5.348 | 19.72 | 0.00011 | 0.00492 |

Witnesses: n_dec > 0 at b = 1.3 and at Bulk under D_RTprop, and at Auto under D_EWMA, or the cell is void.

- I1: D_RTprop n_dec {'Realtime': 0, 'Auto': 0, 'b115': 5325, 'b13': 14146, 'Bulk': 55501}; D_EWMA at Auto n_dec 3668; witness met
- I2: D_RTprop n_dec {'Realtime': 0, 'Auto': 0, 'b115': 32136, 'b13': 85932, 'Bulk': 336798}; D_EWMA at Auto n_dec 316210; witness met
- Q: D_RTprop n_dec {'Realtime': 0, 'Auto': 0, 'b115': 264, 'b13': 1743, 'Bulk': 26190}; D_EWMA at Auto n_dec 7434; witness met

### AG1 arm comparisons (paired differences against D_RTprop, bars m = max(T, NF))

| cell | delta point | arm - D_RTprop | e mean | e p99 | G_rx/S_src | repair/delivered | reading |
|---|---|---|---|---|---|---|---|
| I1 | Realtime | EWMA | 0 (equivalent) | 0 (equivalent) | 0 (equivalent) | 0 (equivalent) | decides only timing: equivalent on every readout |
| I1 | Realtime | ZERO | 0 (equivalent) | 0 (equivalent) | 0 (equivalent) | 0 (equivalent) | decides only timing: equivalent on every readout |
| I1 | Auto | EWMA | 0.00479 (equivalent) | 0.0163 (equivalent) | -2.87e-05 (equivalent) | 0.00143 (shown larger) | D is live and changes a readout by a shown amount |
| I1 | Auto | ZERO | 0 (equivalent) | 0 (equivalent) | 0 (equivalent) | 0 (equivalent) | decides only timing: equivalent on every readout |
| I1 | b115 | EWMA | 0.0139 (equivalent) | 0.0708 (equivalent) | -5.54e-05 (equivalent) | 0.00241 (shown larger) | D is live and changes a readout by a shown amount |
| I1 | b115 | ZERO | -0.0071 (equivalent) | -0.0247 (equivalent) | 4.62e-05 (equivalent) | -0.00222 (shown smaller) | D is live and changes a readout by a shown amount |
| I1 | b13 | EWMA | 0.0221 (equivalent) | 0.104 (equivalent) | 0 (equivalent) | -1.25e-06 (equivalent) | decides only timing: equivalent on every readout |
| I1 | b13 | ZERO | -0.0221 (equivalent) | -0.102 (equivalent) | 0.000108 (equivalent) | -0.00491 (shown smaller) | D is live and changes a readout by a shown amount |
| I1 | Bulk | EWMA | 0.047 (equivalent) | 1.31 (undetermined) | -4.17e-07 (equivalent) | -0.0038 (shown smaller) | D is live and changes a readout by a shown amount |
| I1 | Bulk | ZERO | -0.136 (undetermined) | -1 (undetermined) | 0.000108 (equivalent) | -0.00491 (shown smaller) | D is live and changes a readout by a shown amount |
| I2 | Realtime | EWMA | 2.57e-05 (equivalent) | 0.000115 (equivalent) | -2.08e-06 (equivalent) | 1.41e-05 (equivalent) | decides only timing: equivalent on every readout |
| I2 | Realtime | ZERO | 0 (equivalent) | 0 (equivalent) | 0 (equivalent) | 0 (equivalent) | decides only timing: equivalent on every readout |
| I2 | Auto | EWMA | 1.76 (shown larger) | 8.27 (shown larger) | -0.00266 (shown smaller) | 0.0284 (shown larger) | D is live and changes a readout by a shown amount |
| I2 | Auto | ZERO | 0 (equivalent) | 0 (equivalent) | 0 (equivalent) | 0 (equivalent) | decides only timing: equivalent on every readout |
| I2 | b115 | EWMA | 2.58 (shown larger) | 10.6 (shown larger) | -0.00156 (shown smaller) | -0.00375 (shown smaller) | D is live and changes a readout by a shown amount |
| I2 | b115 | ZERO | -0.061 (equivalent) | -0.0279 (equivalent) | 0.00119 (shown larger) | -0.0134 (shown smaller) | D is live and changes a readout by a shown amount |
| I2 | b13 | EWMA | 2.67 (shown larger) | 10.3 (shown larger) | -0.000241 (equivalent) | -0.0235 (shown smaller) | D is live and changes a readout by a shown amount |
| I2 | b13 | ZERO | -0.246 (equivalent) | -1.21 (equivalent) | 0.00266 (shown larger) | -0.0297 (shown smaller) | D is live and changes a readout by a shown amount |
| I2 | Bulk | EWMA | 1.57 (shown larger) | 11.9 (shown larger) | -0.000297 (equivalent) | -0.0263 (shown smaller) | D is live and changes a readout by a shown amount |
| I2 | Bulk | ZERO | -1.91 (shown smaller) | -8.19 (shown smaller) | 0.00266 (shown larger) | -0.0297 (shown smaller) | D is live and changes a readout by a shown amount |
| Q | Realtime | EWMA | 0 (equivalent) | 0 (equivalent) | 0 (equivalent) | 6.67e-06 (equivalent) | decides only timing: equivalent on every readout |
| Q | Realtime | ZERO | 0 (equivalent) | 0 (equivalent) | 0 (equivalent) | 0 (equivalent) | decides only timing: equivalent on every readout |
| Q | Auto | EWMA | 0.0125 (equivalent) | 0.0409 (equivalent) | -3.79e-05 (equivalent) | 0.00197 (shown larger) | D is live and changes a readout by a shown amount |
| Q | Auto | ZERO | 0 (equivalent) | 0 (equivalent) | 0 (equivalent) | 0 (equivalent) | decides only timing: equivalent on every readout |
| Q | b115 | EWMA | 0.0452 (equivalent) | 0.166 (equivalent) | -0.000104 (equivalent) | 0.00468 (shown larger) | D is live and changes a readout by a shown amount |
| Q | b115 | ZERO | -0.00044 (equivalent) | 5.97e-06 (equivalent) | 2.5e-06 (equivalent) | -0.00011 (equivalent) | decides only timing: equivalent on every readout |
| Q | b13 | EWMA | 0.0898 (equivalent) | 0.404 (equivalent) | -9e-05 (equivalent) | 0.0042 (shown larger) | D is live and changes a readout by a shown amount |
| Q | b13 | ZERO | -0.00449 (equivalent) | 2.77e-05 (equivalent) | 1.83e-05 (equivalent) | -0.000698 (shown smaller) | D is live and changes a readout by a shown amount |
| Q | Bulk | EWMA | 0.166 (undetermined) | 1.31 (undetermined) | -2.5e-05 (equivalent) | 0.000644 (shown larger) | D is live and changes a readout by a shown amount |
| Q | Bulk | ZERO | -0.0607 (equivalent) | -0.12 (equivalent) | 8.5e-05 (equivalent) | -0.00412 (shown smaller) | D is live and changes a readout by a shown amount |

### Wall time per run (s, mean)

- U-I1- 2.16 s over 1080 runs
- U-I2- 2.46 s over 216 runs
- U-G- 2.53 s over 216 runs
- HZ- 2.12 s over 60 runs
- PL- 2.13 s over 204 runs
- AG1- 2.31 s over 540 runs
- RHO- 2.47 s over 96 runs
