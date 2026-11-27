# Toy simulation, stage 2, one path (Parts A, T and B)

Spec: `docs/derivations/skip-rule-efficiency.md` §7 (toy plan v3, merged from toy-plan-v3 af156da7, which adds R0c to Part T and main's 50 ms seed to the D_EWMA arm). Code: `tools/toy/` (`toy2.py`, `ustar.py`, `batch2.py`, `summarize2.py`, `report2.py`, `test_toy2.py`); the stage 1 files are untouched. The toy fixes no constant and none of these numbers is engine provenance. Not built, by instruction: Parts C and D, two-path cells, in-path reorder cells, the W sweep, §7.14.

**D arm used.** D_RTprop is the default in every cell except where AG1 names an arm (the bridge P4h and the P3 blocks were registered on the paper's form). The D_EWMA arm uses main's 50 ms SRTT seed before its first sample, as the plan now says.

## Deviations from registration (all of them)

1. **12 seeds, no 24-seed reruns.** Every "undetermined" decision clause below was left undetermined; the reserve was not spent.
2. **A/A self-check: 7 of 118 readings read "shown" in the first A/A, and did not replicate.** The 7 are R0e's tail metrics in the I2 tail cell (v and e at p99 and p99.9, tail time, tail-only mean and median v): the mean A/A difference was about -8 to -9 ms against T = 1 ms, CI excluding 0. Both arms draw phases from the same distribution, so the true mean difference is zero; 7 of 118 is about what a 95 % CI gives with no multiplicity allowance, and all 7 are one quantity in one cell. Replication with two fresh phase draws (variants 2 and 3, 12 seeds) for R0e (engine and cumulative eps-hat) and R1: **0 of 7 readings are shown again** (all undetermined, differences of +2 to +12 ms with CIs spanning 0). So this is recorded as a chance result of the self-check, not a toy defect. The frozen NF stays, because the per-seed phase scatter it measures is real: the SD of the per-seed tail-time difference between two phase sets is 9.5 to 10 ms for R0e and 41 to 42 ms for R1. Phase-set means of the I2 tail time (ms, 12 seeds): R0e 1488.0, 1495.5, 1495.2, 1489.6, 1490.3, 1485.9 (variants 0 to 5); R1 1512.1, 1522.6, 1524.0, 1511.7, 1512.3, 1526.2. Effect: Part T's bars for tail time and p99 metrics are large (NF about 80 ms). The table was frozen with `--force` because the first reading looked like a failure; no code was changed.
3. **P4a is registered for S1c and cannot hold as written.** S1c's last copy rides the verdict and fills an isolated hole at the instant S0's first resend would, so the median stall equals S0's (measured S1c - S0 = 0 within 1 ms). The +H appears on S1 (a verdict without the copy): S1 - S0 = H within 1 ms (test). The hold itself is checked absolutely: every floor pass in an I1 run with H forced to 5 ms happens exactly 5 ms after that floor's arrival (to 1e-9 s). The copy-less S1 sender exists in `toy2.py` for this test only.
4. **The early-skip receivers model the engine's global hold and an engine-like eps-hat** (coordinator's requests, after review; rerun three times, each time all of Part T's R0e and R0c rows).
   - **Hold.** The engine has one reorder timeout, re-read at every drain: `ReorderBuffer` keeps one `timeout` (`net/reorder.rs:18`), `drain_expired` passes an entry when `now - buffered_at >= timeout` (`:126`), `oldest_deadline` is `buffered_at + timeout` (`:187`), and `set_timeout` (`:174`) is called with `shed_recv_hold(srtt, on, budget_open)` before every drain, with `budget_open = shed_recv_budget_ok(holes_given_up, next_deliver_seq, eps_recv)` (`shed.rs:389`, `holes < eps x frontier`) recomputed each time. The toy now does the same: one current hold; an entry expires at the first evaluation (each arrival, and a timer at the oldest entry's deadline) where its age reaches the current hold; the expiry passes every hole below the largest expired entry; closing the budget lengthens every buffered entry's hold at once and reopening shortens it. Frontier = the delivery frontier, holes given up = the receiver's cumulative early-skip passes. The earlier model (hold fixed per entry at buffering) stays as labelled history rows ("per-entry hold").
   - **eps-hat.** The engine's incoming-loss EWMA: alpha = 0.1 (`raptorpath/src/control/estimator.rs:92`), update `rx_ewma_loss <- 0.1 (e - r)/e + 0.9 rx_ewma_loss` per batch (`:103-110`), a batch being one datagram keyed on a per-path path_seq (`net/mod.rs:3755-3800`, `PathBatchTracker::record_batch`): expected = the gap to the highest path_seq seen, received = 1; a late arrival has expected 0, skips the update and becomes a loss credit paying down a later loss (`estimator.rs:41-50`). In the toy every datagram takes the next path_seq at send, lost ones included: originals, every copy (verdict last copies, serves and resends all go through the one copy-send function), and floors; every arrival is recorded in send order. On a FIFO path the credit never fires (tested on a synthetic late arrival). Differences from the engine: one path, so the maximum over paths is trivial; floors count as batches here.
   - **Does the fallback bind? Yes, now.** In the global-hold model the brake decides passes: per run, 175 of R0e's 279 early-skip passes in I2 (11 of 36 in I1, 609 of 860 in L10) were taken by an entry whose hold had been lengthened while it was buffered, and 3 263 entries (I2) were lengthened. P1 breaches fall from 28 to 2.8 events per run in I2. In the per-entry history model the fallback fired thousands of times and decided nothing (rows identical to eps-hat forced to 1).
   - **Planted controls (I2, 12 seeds, per run).** eps-hat forced to 0 (budget always spent): R0e and R0c both become R1's output exactly (0 early-skip passes, mean v 40.60 ms, median 9.38 ms, tail time 1512 ms, 0 P1 breaches). eps-hat forced to 1 (budget never spent): 368 early-skip passes (R0c 361) and 6.8 P1 breaches against 279 and 2.8 under the engine-like eps-hat, mean v 36.4 ms against 38.5 ms; so the engine-like eps-hat's brake removes about 90 early-skip passes per run and lengthens 175 passes' holds. In the per-entry history model, eps-hat = 0 left 0.2 passes per run and eps-hat = 1 reproduced the engine-like rows.
   - The cumulative-eps-hat rows and R0x (per-seq EWMA, alpha 1/256, my own toy choice), both per-entry, are history rows.
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

- **P1:** 0 violations in every Part B run and every Part T run of R1 and U; the positive control (a sender that ignores the count) trips it. **R0e and R0c violate P1** (today's receiver ignores the count): 0.6 to 0.8 (I1), 3 to 4 (I2) and 43 to 59 (L10) pass events above the allowance per run (global hold), worst excess 2 to 44 seqs.
- **P2** (ρ = 1 and 1 - 1e-7): sender traces equal S0's for S1c with R1 and with U (unit tests); in Part B 0 verdicts, floors and passes at both.
- **P4a** (deviation 3), **P4b** (S1c's G_rx/S_src within 1.5 ε ĥ), **P4c** (U's consumer stall per isolated hole equals min(H_o, R - t_detect, t_F + H - t_detect), observed to within 1e-5 s in four settings), **P4e** (sweep == TU LP == brute force on 200 random instances, LP integral every time), **P4f** (n_dec = 0 at Realtime and Auto under D_RTprop in I1, I2 and Q; above 0 at b = 1.3 and Bulk), **P4g** (d_all == d_rep exactly), **P4h** (deviation 11), **P4i** (e over non-hole packets equals stage 1's HOL sum), **P4j** (below), the **H estimator tests** (FIFO gives 0; a 7 ms late packet gives 6 ms until the window passes; empty window and cold start 0; strict and non-strict; two-path synthetic skew; a later floor never delays an earlier marker), the **deny test** with a planted positive control, and **FIFO** (no datagram overtook another in any run; H is 0 and the estimator counted no late arrival in any Part B run). All pass.
- **P4j:** on isolated holes the bound ratio_U <= 2 + omega N_{R=inf} / cost_U* holds in every seed at omega in {1, 10} and the three presets (worst slack 5.8e-11). The isolated-hole ratio is 2.02 at Realtime, 1.02 at Auto omega = 1, and 1.00 elsewhere.

## Readings, Part T first (lost tail; Realtime, rho = 0.99, one path)

Times in ms, 12 seeds. Bars m = max(T, NF), NF from the I2 A/A (the frozen table; its R0e arm used the registered per-entry hold). R0e and R0c are the global-hold, engine-like-eps-hat receivers (deviation 4).

- **Nothing stalled.** Every receiver in every cell got the ack to S_src. Tail time (last send to the ack frontier) is 1.49 to 1.51 s in I1 and I2 and 1.72 to 1.81 s in L10; the dead window is itself 1.5 s.
- **tail_time cannot separate option (c) from today in these cells.** 347 lost tail seqs against a 121 allowance leave at least 219 resent under every design, and the last fill sets the time. R0c minus R0e in tail time is -7 ms in I2 [-20, 2] and +1 ms in L10 [-23, 26], equivalent under the 85 ms bar; these are the tail time, not a per-seq wait.
- **R0e against R1.** I1: equivalent on every readout (R0e makes 36 early-skip passes per run). I2: median wait 2.7 ms lower (shown), mean wait 2.1 ms lower (equivalent under the bar), tail time equivalent. L10: shown better on wait: median -33 ms, mean -38 ms; tail time -25 ms (equivalent). So the early skip buys under 3 ms in I2 and about 35 ms at RTT 100 ms with 10 % loss.
- **P1.** R0e and R0c break the promise: R0e 0.6 (I1), 2.8 (I2) and 59 (L10) pass events per run above the allowance, worst excess 3, 2 and 41 seqs; R0c 0.8, 3.9 and 43 (worst 3, 30, 44). R1 and U never do.
- **R0c against R0e, stated plainly.** R0c's total abandonment G_rx/S_src is **0.0123 (I2) and 0.0124 (L10)** against the 0.01 promise, and R0e's is 0.0099 and 0.0099; only R0c's floor share stays within the promise (G_floor/S_src 0.0092 and 0.0100). R0c gets 117 (I2) and 116 (L10) tail verdicts against 90 and 92 for R0e (R1: 90 and 98). What option (c) buys: 3.4 ms of mean wait in I2 (shown; equivalent in L10, 3.3 ms against a 14 ms bar) and 0.141 (I2) and 0.034 (L10) fewer repair sends per delivered (shown), by giving up the refused tail within the floor allowance instead of serving it. Its passes come no later than R0e's.
- **Tail-only wait, R0c - R0e, paired per seed** (wait to release: delivery, or the pass if the seq was passed; the lost tail seqs only): I1 +1.3 ms [0.0, 4.0]; I2 +3.7 ms [-1.9, 11.3]; L10 +2.8 ms [-15.3, 18.8]. These do not separate the designs: the sampler and sweep phases scatter the resent tail by about 10 ms per seed. The split shows where the difference is: the seqs that get a verdict under R0c but are re-sent under R0e (28 per run in I2, 24 in L10) are released **7.9 ms earlier in I2 [-10.6, -5.4] and 42.6 ms earlier in L10 [-67.5, -16.5]** per seq (mean wait 1497 against 1505 ms in I2, 1558 against 1600 ms in L10), against the derivation's expectation of about one sweep round (25 ms in I2, 100 ms in L10): below it in both cells, since the saved round overlaps the resends' own wait. The other tail seqs differ by +4.7 ms [-1.3, 13.0] (I2) and +6.5 ms [-12.4, 23.2] (L10), undetermined.
- **Short-window cell (reported only; 70 lost tail seqs, chosen as 0.58 of the allowance and 0.78 of the about 90 I2 leaves free).** The whole lost tail fits the allowance. Tail time is 1513 ms for every receiver, and all four receivers verdict 69 of the 70 tail seqs; R0e and R0c are identical (G_rx/S_src 0.0084, below the 0.01 promise, so R0e's allowance was never exhausted and (c) has nothing to change). Mean wait is 6.3 ms (R0e, R0c), 8.5 ms (R1) and 0.45 ms (U); R0e against R1 is -2.3 ms (equivalent), median -2.7 ms (shown).
- **Accounting.** R0c's extra G_rx over R0e is 29.4 packets per object in I2 (0.0024 of S_src) and 30.4 in L10 (0.0025). They are the seqs that get a verdict under R0c only: 28.2 per run in I2 and 24.4 in L10, and every one of those is among the extra passed seqs (overlap 28.2 and 24.4); the other 1.2 (I2) extra packets are further tail seqs in G under R0c only (verdicted under both, passed in one run and filled in the other), and of L10's other 6.0, 0.8 are tail seqs and about 5 are non-tail early-skip passes never refilled. Tail-seq verdicts per run: I2 R0e 90.3, R0c 117.2, R1 89.7 (R1 - R0e -0.7 seqs, undetermined); L10 R0e 92.3, R0c 115.8, R1 98.2 (R1 - R0e +5.9 seqs [3.7, 8.3], shown).
- **A/A on the global-hold model** (I2 tail cell, variants 0 and 1, 12 seeds): the 95th percentile of |A - A'| on mean wait is 1.0 ms (R0e) and 1.05 ms (R0c), under the frozen 3.03 ms. On repair sends per delivered it is 0.0159 (R0e) and 0.0227 (R0c) against the frozen NF of 0.0159, over it by 0.0001 (R0e) and 0.0069 (R0c), but under the bar actually used in Part T, max(T, NF) = 0.0859 (T is 5 % of S0's value); the frozen table is not changed. The p99 and tail-time scatter (50 to 82 ms) is also near the 81 and 85 ms floors.
- **Against U (the target).** U has R1's sender trace and is shown better on wait: mean v 8.5 ms below R1 in I2, 99 ms below in L10. R0e and R0c sit between them.
- **In words.** The interim receiver costs nothing at the tail (no stall anywhere), buys about 2 ms of mean wait in I2 and 38 ms in L10 against R1, and breaks the promise by a few seqs (I1, I2) to 40 seqs (L10) at its worst. Option (c) adds a few ms of mean wait and one tenth of a repair send per delivered by passing, through the floor, the tail that today refuses and serves, at a total abandonment of 0.0123 against the 0.01 promise. Neither beats U. Nothing is adopted.

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

| cell | receiver | v p50 | v p99 | v p99.9 | v mean | G_rx/S_src | passes (floor / early skip) | P1 violations (worst excess) | tail time | stalled | tail seqs with a verdict | filled after the window | hold evaluations in the fallen-back state | repair sends / delivered | G_floor/S_src | floors / delivered | last copies / delivered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| I1 | R0e (today's early-skip; engine-like eps-hat, one global hold) | 0.00 | 1494.7 | 1503.4 | 30.44 | 0.00995 | 146 (111 / 36) | 0.6 (3.1) | 1493.3 | 0/12 | 120.1 | 226.2 | 3558 | 1.12239 | 0.00911 | 0.01152 | 0.01486 |
| I1 | R0c (same receiver; allowance counts floor passes only) | 0.00 | 1497.1 | 1506.1 | 30.43 | 0.00998 | 147 (111 / 36) | 0.8 (3.3) | 1495.6 | 0/12 | 120.2 | 225.8 | 3558 | 1.12040 | 0.00914 | 0.01149 | 0.01488 |
| I1 | R1 (floor only) | 0.00 | 1499.3 | 1508.5 | 30.93 | 0.00996 | 121 (121 / 0) | 0.0 (0.0) | 1498.1 | 0/12 | 120.0 | 226.0 | 0 | 1.12160 | 0.00996 | 0.01153 | 0.01486 |
| I1 | U, omega = 1 | 0.00 | 1483.3 | 1494.1 | 28.03 | 0.00996 | 121 (121 / 0) | 0.0 (0.0) | 1498.1 | 0/12 | 120.0 | 226.0 | 0 | 1.12160 | 0.00996 | 0.01153 | 0.01486 |
| I1 | R0e, hold fixed per entry at buffering (history) | 0.00 | 1488.1 | 1496.2 | 28.71 | 0.00995 | 183 (109 / 74) | 4.8 (4.9) | 1485.0 | 0/12 | 120.0 | 226.1 | 1539 | 1.12477 | 0.00901 | 0.01071 | 0.01486 |
| I1 | R0c, hold fixed per entry at buffering (history) | 0.00 | 1487.9 | 1495.7 | 28.47 | 0.01011 | 185 (121 / 64) | 6.2 (6.6) | 1485.0 | 0/12 | 121.0 | 224.2 | 1538 | 1.11687 | 0.00996 | 0.01067 | 0.01495 |
| I1 | R0e, cumulative eps-hat, per-entry hold (history) | 0.00 | 1488.1 | 1496.2 | 28.71 | 0.00995 | 183 (109 / 74) | 4.8 (4.9) | 1485.0 | 0/12 | 120.0 | 226.1 | 0 | 1.12477 | 0.00901 | 0.01071 | 0.01486 |
| I1 | R0c, cumulative eps-hat, per-entry hold (history) | 0.00 | 1487.9 | 1495.7 | 28.47 | 0.01011 | 185 (121 / 64) | 6.2 (6.6) | 1485.0 | 0/12 | 121.0 | 224.2 | 0 | 1.11687 | 0.00996 | 0.01067 | 0.01495 |
| I1 | R0e, per-seq EWMA eps-hat alpha 1/256, per-entry hold (history) | 0.00 | 1488.1 | 1496.2 | 28.79 | 0.00996 | 182 (110 / 72) | 4.8 (5.0) | 1485.0 | 0/12 | 120.0 | 226.0 | 332 | 1.12437 | 0.00902 | 0.01075 | 0.01486 |
| I1 | S0 reference | 0.06 | 1512.2 | 1521.2 | 45.81 | 0.00000 | 0 (0 / 0) | 0.0 (0.0) | 1499.1 | 0/12 | 0.0 | 346.0 | 0 | 1.68875 | 0.00000 | 0.00000 | 0.00000 |
| I2 | R0e (today's early-skip; engine-like eps-hat, one global hold) | 6.66 | 1507.4 | 1515.3 | 38.47 | 0.00987 | 370 (92 / 279) | 2.8 (1.9) | 1504.7 | 0/12 | 90.3 | 258.1 | 9789 | 1.30515 | 0.00755 | 0.03051 | 0.03575 |
| I2 | R0c (same receiver; allowance counts floor passes only) | 6.67 | 1513.6 | 1522.5 | 35.02 | 0.01229 | 401 (111 / 290) | 3.9 (29.8) | 1512.2 | 0/12 | 117.2 | 228.8 | 9796 | 1.16399 | 0.00916 | 0.03078 | 0.03863 |
| I2 | R1 (floor only) | 9.38 | 1515.1 | 1523.6 | 40.60 | 0.00996 | 121 (121 / 0) | 0.0 (0.0) | 1512.1 | 0/12 | 89.7 | 256.3 | 0 | 1.29514 | 0.00996 | 0.03084 | 0.03630 |
| I2 | U, omega = 1 | 0.00 | 1487.5 | 1503.1 | 32.14 | 0.00996 | 121 (121 / 0) | 0.0 (0.0) | 1512.1 | 0/12 | 89.7 | 256.3 | 0 | 1.29514 | 0.00996 | 0.03084 | 0.03630 |
| I2 | R0e, hold fixed per entry at buffering (history) | 4.70 | 1493.3 | 1503.4 | 35.51 | 0.01030 | 459 (80 / 379) | 27.8 (23.4) | 1488.0 | 0/12 | 88.8 | 252.8 | 7322 | 1.30671 | 0.00659 | 0.02999 | 0.03553 |
| I2 | R0c, hold fixed per entry at buffering (history) | 4.69 | 1489.9 | 1500.2 | 31.42 | 0.01304 | 490 (111 / 379) | 27.2 (54.0) | 1488.0 | 0/12 | 121.0 | 219.4 | 7312 | 1.14573 | 0.00913 | 0.03040 | 0.03893 |
| I2 | R0e, cumulative eps-hat, per-entry hold (history) | 4.70 | 1493.3 | 1503.4 | 35.51 | 0.01030 | 459 (80 / 379) | 27.8 (23.4) | 1488.0 | 0/12 | 88.8 | 252.8 | 0 | 1.30671 | 0.00659 | 0.02999 | 0.03553 |
| I2 | R0c, cumulative eps-hat, per-entry hold (history) | 4.69 | 1489.9 | 1500.2 | 31.42 | 0.01304 | 490 (111 / 379) | 27.2 (54.0) | 1488.0 | 0/12 | 121.0 | 219.4 | 0 | 1.14573 | 0.00913 | 0.03040 | 0.03893 |
| I2 | R0e, per-seq EWMA eps-hat alpha 1/256, per-entry hold (history) | 5.49 | 1493.6 | 1503.8 | 36.36 | 0.01032 | 430 (84 / 346) | 27.2 (23.7) | 1488.3 | 0/12 | 88.8 | 252.5 | 3151 | 1.30523 | 0.00692 | 0.03011 | 0.03560 |
| I2 | S0 reference | 10.63 | 1526.5 | 1535.3 | 54.86 | 0.00000 | 0 (0 / 0) | 0.0 (0.0) | 1516.6 | 0/12 | 0.0 | 346.0 | 0 | 1.71776 | 0.00000 | 0.00000 | 0.00000 |
| L10 | R0e (today's early-skip; engine-like eps-hat, one global hold) | 99.86 | 1719.6 | 1750.0 | 128.67 | 0.00988 | 951 (91 / 860) | 58.7 (41.4) | 1784.4 | 0/12 | 92.3 | 255.3 | 10959 | 0.54555 | 0.00750 | 0.03117 | 0.09116 |
| L10 | R0c (same receiver; allowance counts floor passes only) | 99.86 | 1732.5 | 1749.9 | 125.41 | 0.01238 | 974 (121 / 853) | 42.8 (43.6) | 1783.0 | 0/12 | 115.8 | 230.1 | 10939 | 0.51107 | 0.00996 | 0.03668 | 0.10950 |
| L10 | R1 (floor only) | 132.48 | 1751.4 | 1775.1 | 166.67 | 0.00996 | 121 (121 / 0) | 0.0 (0.0) | 1809.0 | 0/12 | 98.2 | 247.8 | 0 | 0.54254 | 0.00996 | 0.04381 | 0.10786 |
| L10 | U, omega = 1 | 1.90 | 1533.0 | 1629.1 | 67.27 | 0.00996 | 121 (121 / 0) | 0.0 (0.0) | 1809.0 | 0/12 | 98.2 | 247.8 | 0 | 0.54254 | 0.00996 | 0.04381 | 0.10786 |
| L10 | R0e, hold fixed per entry at buffering (history) | 96.39 | 1631.2 | 1642.1 | 122.81 | 0.00978 | 1218 (72 / 1147) | 258.9 (48.7) | 1667.6 | 0/12 | 82.0 | 266.2 | 8405 | 0.55031 | 0.00591 | 0.02599 | 0.08067 |
| L10 | R0c, hold fixed per entry at buffering (history) | 96.47 | 1626.7 | 1637.6 | 117.69 | 0.01261 | 1252 (121 / 1131) | 161.6 (53.5) | 1667.4 | 0/12 | 117.9 | 227.7 | 8347 | 0.50772 | 0.00996 | 0.03912 | 0.11117 |
| L10 | R0e, cumulative eps-hat, per-entry hold (history) | 95.59 | 1630.6 | 1641.4 | 121.99 | 0.00965 | 1298 (72 / 1226) | 318.2 (48.7) | 1666.8 | 0/12 | 83.6 | 265.9 | 0 | 0.55125 | 0.00591 | 0.03115 | 0.08250 |
| L10 | R0c, cumulative eps-hat, per-entry hold (history) | 95.55 | 1626.1 | 1637.0 | 116.56 | 0.01299 | 1335 (111 / 1224) | 193.7 (60.4) | 1666.8 | 0/12 | 121.0 | 224.6 | 0 | 0.50304 | 0.00913 | 0.04465 | 0.11157 |
| L10 | R0e, per-seq EWMA eps-hat alpha 1/256, per-entry hold (history) | 95.79 | 1630.7 | 1641.5 | 122.24 | 0.00973 | 1252 (74 / 1178) | 315.8 (48.7) | 1667.1 | 0/12 | 83.8 | 264.9 | 1159 | 0.55010 | 0.00608 | 0.03054 | 0.08247 |
| L10 | S0 reference | 234.53 | 1782.6 | 1805.6 | 285.80 | 0.00000 | 0 (0 / 0) | 0.0 (0.0) | 1834.0 | 0/12 | 0.0 | 346.0 | 0 | 0.58212 | 0.00000 | 0.00000 | 0.00000 |

### Part T: the early-skip hold. Passes decided while the budget had lengthened an entry's hold (global-hold model, per run)

| cell | receiver | entries whose hold was lengthened while buffered | early-skip passes | passes decided by a lengthened hold | hold evaluations, budget open | hold evaluations, fallen back |
|---|---|---|---|---|---|---|
| I1 | R0e | 2336 | 35.8 | 11.4 | 1959 | 3558 |
| I1 | R0c | 2336 | 35.8 | 11.4 | 1959 | 3558 |
| I2 | R0e | 3263 | 278.8 | 175.2 | 4635 | 9789 |
| I2 | R0c | 3278 | 290.2 | 175.8 | 4648 | 9796 |
| L10 | R0e | 5198 | 859.8 | 608.8 | 4752 | 10959 |
| L10 | R0c | 5146 | 853.2 | 600.3 | 5664 | 10939 |

Tail-only waits (the lost tail seqs; ms): mean v / median v
- I1 R0e: 1488.3 / 1495.4
- I1 R0c: 1490.3 / 1497.8
- I1 R1: 1492.6 / 1499.9
- I1 U: 1477.7 / 1484.1
- I2 R0e: 1501.0 / 1506.9
- I2 R0c: 1506.8 / 1514.0
- I2 R1: 1508.6 / 1514.7
- I2 U: 1481.3 / 1486.7
- L10 R0e: 1698.9 / 1718.9
- L10 R0c: 1711.1 / 1733.0
- L10 R1: 1726.8 / 1743.9
- L10 U: 1536.2 / 1532.7

### Part T readings: paired per-seed differences against m = max(T, NF) (NF from the I2 A/A, frozen)

| cell | contrast | metric | difference [CI] | m | reading |
|---|---|---|---|---|---|
| I1 | R0e - R0c | v_p50 | 0 [0, 0] | 1 | equivalent |
| I1 | R0e - R0c | v_p99 | -2.419 [-7.302, 0.03912] | 81.2 | equivalent |
| I1 | R0e - R0c | v_p999 | -2.672 [-8.017, -8.951e-06] | 82 | equivalent |
| I1 | R0e - R0c | v_mean | 0.01231 [-0.04196, 0.06076] | 3.03 | equivalent |
| I1 | R0e - R0c | G_ratio | -3.43e-05 [-6.86e-05, -6.86e-06] | 0.00183 | equivalent |
| I1 | R0e - R0c | max_ep_spend | -0.4167 [-0.8333, -0.08333] | 9.35 | equivalent |
| I1 | R0e - R0c | repair_per_delivered | 0.001985 [0.0004011, 0.003956] | 0.0844 | equivalent |
| I1 | R0e - R0c | tail_time | -2.264 [-6.793, 0] | 85.1 | equivalent |
| I1 | R0e - R1 | v_p50 | 0 [0, 0] | 1 | equivalent |
| I1 | R0e - R1 | v_p99 | -4.625 [-9.641, -1.468] | 81.2 | equivalent |
| I1 | R0e - R1 | v_p999 | -5.122 [-10.54, -1.477] | 82 | equivalent |
| I1 | R0e - R1 | v_mean | -0.4852 [-0.5526, -0.4273] | 3.03 | equivalent |
| I1 | R0e - R1 | G_ratio | -1.372e-05 [-3.43e-05, 0] | 0.00183 | equivalent |
| I1 | R0e - R1 | max_ep_spend | 2.917 [1, 4.917] | 9.35 | equivalent |
| I1 | R0e - R1 | repair_per_delivered | 0.0007882 [-1.386e-05, 0.001984] | 0.0844 | equivalent |
| I1 | R0e - R1 | tail_time | -4.753 [-9.332, -1.477] | 85.1 | equivalent |
| I1 | R0c - R1 | v_p50 | 0 [0, 0] | 1 | equivalent |
| I1 | R0c - R1 | v_p99 | -2.206 [-3.57, -0.8931] | 81.2 | equivalent |
| I1 | R0c - R1 | v_p999 | -2.45 [-4.026, -0.9507] | 82 | equivalent |
| I1 | R0c - R1 | v_mean | -0.4975 [-0.5559, -0.4436] | 3.03 | equivalent |
| I1 | R0c - R1 | G_ratio | 2.058e-05 [0, 4.116e-05] | 0.00183 | equivalent |
| I1 | R0c - R1 | max_ep_spend | 3.333 [1.583, 5.25] | 9.35 | equivalent |
| I1 | R0c - R1 | repair_per_delivered | -0.001196 [-0.002407, 0] | 0.0844 | equivalent |
| I1 | R0c - R1 | tail_time | -2.488 [-4.092, -0.9613] | 85.1 | equivalent |
| I1 | R0e - U | v_p50 | 0 [0, 0] | 1 | equivalent |
| I1 | R0e - U | v_p99 | 11.39 [6.167, 18.56] | 81.2 | equivalent |
| I1 | R0e - U | v_p999 | 9.257 [3.905, 16.73] | 82 | equivalent |
| I1 | R0e - U | v_mean | 2.411 [2.312, 2.524] | 3.03 | equivalent |
| I1 | R0e - U | G_ratio | -1.372e-05 [-3.43e-05, 0] | 0.00183 | equivalent |
| I1 | R0e - U | max_ep_spend | 12.83 [1.083, 34.33] | 9.35 | undetermined |
| I1 | R0e - U | repair_per_delivered | 0.0007882 [-1.386e-05, 0.001984] | 0.0844 | equivalent |
| I1 | R0e - U | tail_time | -4.753 [-9.332, -1.477] | 85.1 | equivalent |
| I1 | R0c - U | v_p50 | 0 [0, 0] | 1 | equivalent |
| I1 | R0c - U | v_p99 | 13.81 [6.429, 22.79] | 81.2 | equivalent |
| I1 | R0c - U | v_p999 | 11.93 [4.35, 21.47] | 82 | equivalent |
| I1 | R0c - U | v_mean | 2.399 [2.297, 2.511] | 3.03 | equivalent |
| I1 | R0c - U | G_ratio | 2.058e-05 [0, 4.116e-05] | 0.00183 | equivalent |
| I1 | R0c - U | max_ep_spend | 13.25 [1.667, 34.67] | 9.35 | undetermined |
| I1 | R0c - U | repair_per_delivered | -0.001196 [-0.002407, 0] | 0.0844 | equivalent |
| I1 | R0c - U | tail_time | -2.488 [-4.092, -0.9613] | 85.1 | equivalent |
| I1 | R1 - U | v_p50 | 0 [0, 0] | 1 | equivalent |
| I1 | R1 - U | v_p99 | 16.02 [9.378, 24.47] | 81.2 | equivalent |
| I1 | R1 - U | v_p999 | 14.38 [7.353, 23.26] | 82 | equivalent |
| I1 | R1 - U | v_mean | 2.896 [2.784, 3.004] | 3.03 | equivalent |
| I1 | R1 - U | G_ratio | 0 [0, 0] | 0.00183 | equivalent |
| I1 | R1 - U | max_ep_spend | 9.917 [0, 29.75] | 9.35 | undetermined |
| I1 | R1 - U | repair_per_delivered | 0 [0, 0] | 0.0844 | equivalent |
| I1 | R1 - U | tail_time | 0 [0, 0] | 85.1 | equivalent |
| cell | contrast | metric | difference [CI] | m | reading |
|---|---|---|---|---|---|
| I2 | R0e - R0c | v_p50 | -0.008105 [-0.03782, 0.01047] | 1 | equivalent |
| I2 | R0e - R0c | v_p99 | -6.191 [-19.11, 3.093] | 81.2 | equivalent |
| I2 | R0e - R0c | v_p999 | -7.11 [-19.67, 2.15] | 82 | equivalent |
| I2 | R0e - R0c | v_mean | 3.449 [3.08, 3.79] | 3.03 | shown larger |
| I2 | R0e - R0c | G_ratio | -0.002422 [-0.002621, -0.00223] | 0.00183 | shown smaller |
| I2 | R0e - R0c | max_ep_spend | -26.5 [-33.5, -18.92] | 9.35 | shown smaller |
| I2 | R0e - R0c | repair_per_delivered | 0.1412 [0.1299, 0.1518] | 0.0859 | shown larger |
| I2 | R0e - R0c | tail_time | -7.407 [-20.34, 1.743] | 85.1 | equivalent |
| I2 | R0e - R1 | v_p50 | -2.72 [-2.86, -2.585] | 1 | shown smaller |
| I2 | R0e - R1 | v_p99 | -7.771 [-19.74, -0.6713] | 81.2 | equivalent |
| I2 | R0e - R1 | v_p999 | -8.297 [-20.54, -0.8935] | 82 | equivalent |
| I2 | R0e - R1 | v_mean | -2.129 [-2.39, -1.945] | 3.03 | equivalent |
| I2 | R0e - R1 | G_ratio | -9.605e-05 [-0.0001441, -5.488e-05] | 0.00183 | equivalent |
| I2 | R0e - R1 | max_ep_spend | 5.917 [-2.333, 19.09] | 9.35 | undetermined |
| I2 | R0e - R1 | repair_per_delivered | 0.01001 [0.005678, 0.01487] | 0.0859 | equivalent |
| I2 | R0e - R1 | tail_time | -7.395 [-19.22, 0] | 85.1 | equivalent |
| I2 | R0c - R1 | v_p50 | -2.712 [-2.85, -2.58] | 1 | shown smaller |
| I2 | R0c - R1 | v_p99 | -1.58 [-6.021, 3.661] | 81.2 | equivalent |
| I2 | R0c - R1 | v_p999 | -1.187 [-5.421, 3.596] | 82 | equivalent |
| I2 | R0c - R1 | v_mean | -5.578 [-5.936, -5.208] | 3.03 | shown smaller |
| I2 | R0c - R1 | G_ratio | 0.002326 [0.002154, 0.002518] | 0.00183 | shown larger |
| I2 | R0c - R1 | max_ep_spend | 32.42 [26.92, 39.92] | 9.35 | shown larger |
| I2 | R0c - R1 | repair_per_delivered | -0.1311 [-0.1413, -0.1217] | 0.0859 | shown smaller |
| I2 | R0c - R1 | tail_time | 0.01199 [-5.028, 6.069] | 85.1 | equivalent |
| I2 | R0e - U | v_p50 | 6.664 [6.482, 6.834] | 1 | shown larger |
| I2 | R0e - U | v_p99 | 19.84 [16.96, 22.6] | 81.2 | equivalent |
| I2 | R0e - U | v_p999 | 12.21 [7.736, 16.19] | 82 | equivalent |
| I2 | R0e - U | v_mean | 6.323 [6.181, 6.451] | 3.03 | shown larger |
| I2 | R0e - U | G_ratio | -9.605e-05 [-0.0001441, -5.488e-05] | 0.00183 | equivalent |
| I2 | R0e - U | max_ep_spend | 13.67 [-2, 32] | 9.35 | undetermined |
| I2 | R0e - U | repair_per_delivered | 0.01001 [0.005678, 0.01487] | 0.0859 | equivalent |
| I2 | R0e - U | tail_time | -7.395 [-19.22, 0] | 85.1 | equivalent |
| I2 | R0c - U | v_p50 | 6.672 [6.49, 6.838] | 1 | shown larger |
| I2 | R0c - U | v_p99 | 26.03 [15.52, 41.01] | 81.2 | equivalent |
| I2 | R0c - U | v_p999 | 19.32 [12.9, 27.93] | 82 | equivalent |
| I2 | R0c - U | v_mean | 2.874 [2.472, 3.289] | 3.03 | undetermined |
| I2 | R0c - U | G_ratio | 0.002326 [0.002154, 0.002518] | 0.00183 | shown larger |
| I2 | R0c - U | max_ep_spend | 40.17 [28.83, 55.09] | 9.35 | shown larger |
| I2 | R0c - U | repair_per_delivered | -0.1311 [-0.1413, -0.1217] | 0.0859 | shown smaller |
| I2 | R0c - U | tail_time | 0.01199 [-5.028, 6.069] | 85.1 | equivalent |
| I2 | R1 - U | v_p50 | 9.384 [9.315, 9.445] | 1 | shown larger |
| I2 | R1 - U | v_p99 | 27.61 [18.98, 41.26] | 81.2 | equivalent |
| I2 | R1 - U | v_p999 | 20.51 [14.43, 29.32] | 82 | equivalent |
| I2 | R1 - U | v_mean | 8.452 [8.237, 8.761] | 3.03 | shown larger |
| I2 | R1 - U | G_ratio | 0 [0, 0] | 0.00183 | equivalent |
| I2 | R1 - U | max_ep_spend | 7.75 [0, 23.25] | 9.35 | undetermined |
| I2 | R1 - U | repair_per_delivered | 0 [0, 0] | 0.0859 | equivalent |
| I2 | R1 - U | tail_time | 0 [0, 0] | 85.1 | equivalent |
| cell | contrast | metric | difference [CI] | m | reading |
|---|---|---|---|---|---|
| L10 | R0e - R0c | v_p50 | 0.002492 [-0.0313, 0.03786] | 1 | equivalent |
| L10 | R0e - R0c | v_p99 | -12.91 [-42.05, 18.02] | 81.2 | equivalent |
| L10 | R0e - R0c | v_p999 | 0.0875 [-24.42, 25.34] | 82 | equivalent |
| L10 | R0e - R0c | v_mean | 3.259 [2.691, 3.831] | 14.3 | equivalent |
| L10 | R0e - R0c | G_ratio | -0.002504 [-0.002957, -0.002106] | 0.00183 | shown smaller |
| L10 | R0e - R0c | max_ep_spend | 2 [-7.669, 11.5] | 9.35 | undetermined |
| L10 | R0e - R0c | repair_per_delivered | 0.03448 [0.02911, 0.04061] | 0.0291 | shown larger |
| L10 | R0e - R0c | tail_time | 1.409 [-23.35, 26.42] | 85.1 | equivalent |
| L10 | R0e - R1 | v_p50 | -32.61 [-35.53, -29.47] | 1 | shown smaller |
| L10 | R0e - R1 | v_p99 | -31.79 [-56.59, -7.88] | 81.2 | equivalent |
| L10 | R0e - R1 | v_p999 | -25.08 [-49.65, -1.076] | 82 | equivalent |
| L10 | R0e - R1 | v_mean | -38 [-39.64, -36.34] | 14.3 | shown smaller |
| L10 | R0e - R1 | G_ratio | -8.232e-05 [-0.0001578, -2.744e-05] | 0.00183 | equivalent |
| L10 | R0e - R1 | max_ep_spend | 761.9 [742.5, 783.1] | 9.35 | shown larger |
| L10 | R0e - R1 | repair_per_delivered | 0.00301 [0.0003863, 0.005396] | 0.0291 | equivalent |
| L10 | R0e - R1 | tail_time | -24.54 [-49.18, 0] | 85.1 | equivalent |
| L10 | R0c - R1 | v_p50 | -32.62 [-35.54, -29.46] | 1 | shown smaller |
| L10 | R0c - R1 | v_p99 | -18.88 [-53.28, -0.3931] | 81.2 | equivalent |
| L10 | R0c - R1 | v_p999 | -25.17 [-66.55, 0.002838] | 82 | equivalent |
| L10 | R0c - R1 | v_mean | -41.26 [-43.19, -39.31] | 14.3 | shown smaller |
| L10 | R0c - R1 | G_ratio | 0.002422 [0.002031, 0.00284] | 0.00183 | shown larger |
| L10 | R0c - R1 | max_ep_spend | 759.9 [741.7, 778.3] | 9.35 | shown larger |
| L10 | R0c - R1 | repair_per_delivered | -0.03147 [-0.03778, -0.02616] | 0.0291 | undetermined |
| L10 | R0c - R1 | tail_time | -25.95 [-66.74, 0] | 85.1 | equivalent |
| L10 | R0e - U | v_p50 | 97.96 [97.9, 98.02] | 1 | shown larger |
| L10 | R0e - U | v_p99 | 186.6 [162.5, 210.5] | 81.2 | shown larger |
| L10 | R0e - U | v_p999 | 120.9 [104.1, 143.4] | 82 | shown larger |
| L10 | R0e - U | v_mean | 61.4 [60.71, 62.08] | 14.3 | shown larger |
| L10 | R0e - U | G_ratio | -8.232e-05 [-0.0001578, -2.744e-05] | 0.00183 | equivalent |
| L10 | R0e - U | max_ep_spend | 761.9 [742.5, 783.1] | 9.35 | shown larger |
| L10 | R0e - U | repair_per_delivered | 0.00301 [0.0003863, 0.005396] | 0.0291 | equivalent |
| L10 | R0e - U | tail_time | -24.54 [-49.18, 0] | 85.1 | equivalent |
| L10 | R0c - U | v_p50 | 97.96 [97.91, 98] | 1 | shown larger |
| L10 | R0c - U | v_p99 | 199.5 [164.6, 233] | 81.2 | shown larger |
| L10 | R0c - U | v_p999 | 120.8 [88.73, 152.6] | 82 | shown larger |
| L10 | R0c - U | v_mean | 58.14 [57.22, 58.95] | 14.3 | shown larger |
| L10 | R0c - U | G_ratio | 0.002422 [0.002031, 0.00284] | 0.00183 | shown larger |
| L10 | R0c - U | max_ep_spend | 759.9 [741.7, 778.3] | 9.35 | shown larger |
| L10 | R0c - U | repair_per_delivered | -0.03147 [-0.03778, -0.02616] | 0.0291 | undetermined |
| L10 | R0c - U | tail_time | -25.95 [-66.74, 0] | 85.1 | equivalent |
| L10 | R1 - U | v_p50 | 130.6 [127.4, 133.5] | 1 | shown larger |
| L10 | R1 - U | v_p99 | 218.4 [186.4, 250] | 81.2 | shown larger |
| L10 | R1 - U | v_p999 | 146 [120.2, 172.6] | 82 | shown larger |
| L10 | R1 - U | v_mean | 99.4 [97.78, 101.1] | 14.3 | shown larger |
| L10 | R1 - U | G_ratio | 0 [0, 0] | 0.00183 | equivalent |
| L10 | R1 - U | max_ep_spend | 0 [0, 0] | 9.35 | equivalent |
| L10 | R1 - U | repair_per_delivered | 0 [0, 0] | 0.0291 | equivalent |
| L10 | R1 - U | tail_time | 0 [0, 0] | 85.1 | equivalent |

### Tail-only wait, R0c - R0e, paired per seed (ms): split into the seqs that get a verdict under R0c but not under R0e, and the rest

tail_time is the time to the last fill and is set by the resent tail; it cannot separate (c) from today here, because 347 lost tail seqs against a 121 allowance leave at least 219 resent under every design. The tail-only wait is read per seq.

| cell | R0c - R0e tail-only mean wait [CI] | seqs with a verdict under R0c only (per run) | their mean wait under R0c | their mean wait under R0e | difference per such seq [CI] | difference over the other tail seqs [CI] | overall [CI] |
|---|---|---|---|---|---|---|---|
| I1 | 1.33 [0.00, 3.99] | 0.4 | 1493.6 | 1493.6 | 0.00 [0.00, 0.00] | 1.34 [0.00, 4.01] | 1.33 [0.00, 3.99] |
| I2 | 3.69 [-1.94, 11.31] | 28.2 | 1497.3 | 1505.3 | -7.91 [-10.64, -5.43] | 4.68 [-1.31, 12.96] | 3.69 [-1.94, 11.31] |
| L10 | 2.78 [-15.27, 18.77] | 24.4 | 1557.7 | 1600.4 | -42.64 [-67.48, -16.48] | 6.47 [-12.44, 23.22] | 2.78 [-15.27, 18.77] |
| short window (70 lost), I2 | -0.00 [-0.00, 0.00] | 0.0 | n/a | n/a | n/a | -0.00 [-0.00, 0.00] | -0.00 [-0.00, 0.00] |

### Short-window cell (reported only): I2 conditions, the last 70 seqs lost for 1.5 s

70 lost tail seqs is 0.58 of the 121-seq allowance and 0.78 of the roughly 90 that I2's earlier losses leave free (R1 gets 89.7 tail verdicts in the full cell), so the whole lost tail fits the allowance where the floor can pass it. Chosen by hand from that arithmetic; other windows were not tried.

| cell | receiver | v p50 | v p99 | v p99.9 | v mean | G_rx/S_src | passes (floor / early skip) | P1 violations (worst excess) | tail time | stalled | tail seqs with a verdict | filled after the window | hold evaluations in the fallen-back state | repair sends / delivered | G_floor/S_src | floors / delivered | last copies / delivered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| S70 | R0e (today's early-skip; engine-like eps-hat, one global hold) | 6.58 | 12.1 | 12.7 | 6.26 | 0.00835 | 358 (73 / 285) | 2.8 (1.9) | 1513.3 | 0/12 | 69.0 | 0.0 | 9973 | 0.03582 | 0.00600 | 0.03084 | 0.03455 |
| S70 | R0c (same receiver; allowance counts floor passes only) | 6.60 | 12.1 | 12.7 | 6.27 | 0.00836 | 358 (73 / 285) | 2.9 (1.9) | 1513.3 | 0/12 | 69.0 | 0.0 | 9992 | 0.03559 | 0.00600 | 0.03125 | 0.03510 |
| S70 | R1 (floor only) | 9.31 | 14.6 | 19.8 | 8.51 | 0.00830 | 101 (101 / 0) | 0.0 (0.0) | 1513.3 | 0/12 | 69.0 | 0.0 | 0 | 0.03617 | 0.00830 | 0.03138 | 0.03515 |
| S70 | U, omega = 1 | 0.00 | 12.0 | 14.1 | 0.45 | 0.00830 | 101 (101 / 0) | 0.0 (0.0) | 1513.3 | 0/12 | 69.0 | 0.0 | 0 | 0.03617 | 0.00830 | 0.03138 | 0.03515 |
| S70 | S0 reference | 10.54 | 35.3 | 1537.4 | 20.43 | 0.00000 | 0 (0 / 0) | 0.0 (0.0) | 1539.9 | 0/12 | 0.0 | 69.0 | 0 | 0.37211 | 0.00000 | 0.00000 | 0.00000 |

| cell | contrast | metric | difference [CI] | m | reading |
|---|---|---|---|---|---|
| S70 | R0e - R0c | v_p50 | -0.01684 [-0.04615, 0.0006977] | 1 | equivalent |
| S70 | R0e - R0c | v_p99 | -0.000768 [-0.005259, 0.003693] | 81.2 | equivalent |
| S70 | R0e - R0c | v_p999 | 0.002522 [-0.009679, 0.01524] | 82 | equivalent |
| S70 | R0e - R0c | v_mean | -0.01424 [-0.03752, 0.0002872] | 3.03 | equivalent |
| S70 | R0e - R0c | G_ratio | -6.86e-06 [-5.488e-05, 5.488e-05] | 0.00183 | equivalent |
| S70 | R0e - R0c | max_ep_spend | -1.667 [-4.917, 0] | 9.35 | equivalent |
| S70 | R0e - R0c | repair_per_delivered | 0.0002349 [-8.144e-05, 0.0005391] | 0.0186 | equivalent |
| S70 | R0e - R0c | tail_time | 0 [0, 0] | 85.1 | equivalent |
| S70 | R0e - R1 | v_p50 | -2.728 [-2.867, -2.594] | 1 | shown smaller |
| S70 | R0e - R1 | v_p99 | -2.513 [-3.731, -1.497] | 81.2 | equivalent |
| S70 | R0e - R1 | v_p999 | -7.074 [-9.531, -4.921] | 82 | equivalent |
| S70 | R0e - R1 | v_mean | -2.251 [-2.371, -2.141] | 3.03 | equivalent |
| S70 | R0e - R1 | G_ratio | 4.802e-05 [0, 0.0001029] | 0.00183 | equivalent |
| S70 | R0e - R1 | max_ep_spend | 57.92 [38.41, 79.92] | 9.35 | shown larger |
| S70 | R0e - R1 | repair_per_delivered | -0.0003442 [-0.0005659, -0.0001082] | 0.0186 | equivalent |
| S70 | R0e - R1 | tail_time | 0 [0, 0] | 85.1 | equivalent |
| S70 | R0c - R1 | v_p50 | -2.711 [-2.849, -2.581] | 1 | shown smaller |
| S70 | R0c - R1 | v_p99 | -2.512 [-3.731, -1.497] | 81.2 | equivalent |
| S70 | R0c - R1 | v_p999 | -7.077 [-9.544, -4.918] | 82 | equivalent |
| S70 | R0c - R1 | v_mean | -2.237 [-2.346, -2.137] | 3.03 | equivalent |
| S70 | R0c - R1 | G_ratio | 5.488e-05 [2.058e-05, 9.605e-05] | 0.00183 | equivalent |
| S70 | R0c - R1 | max_ep_spend | 59.58 [40.42, 81.17] | 9.35 | shown larger |
| S70 | R0c - R1 | repair_per_delivered | -0.0005791 [-0.00075, -0.0004] | 0.0186 | equivalent |
| S70 | R0c - R1 | tail_time | 0 [0, 0] | 85.1 | equivalent |
| S70 | R0e - U | v_p50 | 6.58 [6.398, 6.749] | 1 | shown larger |
| S70 | R0e - U | v_p99 | 0.107 [-0.07526, 0.2471] | 81.2 | equivalent |
| S70 | R0e - U | v_p999 | -1.367 [-3.33, 0.01798] | 82 | equivalent |
| S70 | R0e - U | v_mean | 5.806 [5.653, 5.954] | 3.03 | shown larger |
| S70 | R0e - U | G_ratio | 4.802e-05 [0, 0.0001029] | 0.00183 | equivalent |
| S70 | R0e - U | max_ep_spend | 71.08 [55.25, 91.67] | 9.35 | shown larger |
| S70 | R0e - U | repair_per_delivered | -0.0003442 [-0.0005659, -0.0001082] | 0.0186 | equivalent |
| S70 | R0e - U | tail_time | 0 [0, 0] | 85.1 | equivalent |
| S70 | R0c - U | v_p50 | 6.597 [6.414, 6.762] | 1 | shown larger |
| S70 | R0c - U | v_p99 | 0.1077 [-0.07078, 0.2447] | 81.2 | equivalent |
| S70 | R0c - U | v_p999 | -1.369 [-3.329, 0.01037] | 82 | equivalent |
| S70 | R0c - U | v_mean | 5.82 [5.67, 5.963] | 3.03 | shown larger |
| S70 | R0c - U | G_ratio | 5.488e-05 [2.058e-05, 9.605e-05] | 0.00183 | equivalent |
| S70 | R0c - U | max_ep_spend | 72.75 [58, 92.75] | 9.35 | shown larger |
| S70 | R0c - U | repair_per_delivered | -0.0005791 [-0.00075, -0.0004] | 0.0186 | equivalent |
| S70 | R0c - U | tail_time | 0 [0, 0] | 85.1 | equivalent |
| S70 | R1 - U | v_p50 | 9.307 [9.237, 9.369] | 1 | shown larger |
| S70 | R1 - U | v_p99 | 2.62 [1.637, 3.834] | 81.2 | equivalent |
| S70 | R1 - U | v_p999 | 5.708 [3.295, 8.506] | 82 | equivalent |
| S70 | R1 - U | v_mean | 8.057 [7.973, 8.142] | 3.03 | shown larger |
| S70 | R1 - U | G_ratio | 0 [0, 0] | 0.00183 | equivalent |
| S70 | R1 - U | max_ep_spend | 13.17 [6.917, 24] | 9.35 | undetermined |
| S70 | R1 - U | repair_per_delivered | 0 [0, 0] | 0.0186 | equivalent |
| S70 | R1 - U | tail_time | 0 [0, 0] | 85.1 | equivalent |

### Replication, phase scatter and the AG1 n_dec split

### Fallback control (I2 tail cell, 12 seeds): eps-hat forced to 0 (budget always spent) and to 1 (never spent), per run

| row | early-skip passes | floor passes | passes decided by a lengthened hold | entries lengthened while buffered | hold evaluations fallen back | P1 violations | v mean ms | v p50 ms | tail time ms |
|---|---|---|---|---|---|---|---|---|---|
| R0e, global hold, engine-like eps-hat | 278.8 | 91.7 | 175.2 | 3263 | 9789 | 2.8 | 38.47 | 6.66 | 1504.7 |
| R0c, global hold, engine-like eps-hat | 290.2 | 111.2 | 175.8 | 3278 | 9796 | 3.9 | 35.02 | 6.67 | 1512.2 |
| R0e, global hold, eps-hat = 0 | 0.0 | 121.0 | 0.0 | 5440 | 11733 | 0.0 | 40.60 | 9.38 | 1512.1 |
| R0c, global hold, eps-hat = 0 | 0.0 | 121.0 | 0.0 | 5440 | 11733 | 0.0 | 40.60 | 9.38 | 1512.1 |
| R0e, global hold, eps-hat = 1 | 367.7 | 73.1 | 0.0 | 0 | 10 | 6.8 | 36.39 | 4.74 | 1499.4 |
| R0c, global hold, eps-hat = 1 | 361.3 | 110.9 | 0.0 | 0 | 10 | 7.6 | 32.15 | 4.73 | 1506.4 |
| R0e, per-entry hold (history), engine-like eps-hat | 378.7 | 80.1 | 0.0 | 0 | 7322 | 27.8 | 35.51 | 4.70 | 1488.0 |
| R0e, per-entry hold (history), eps-hat = 0 | 0.2 | 121.0 | 0.0 | 0 | 11316 | 0.2 | 40.54 | 9.38 | 1510.4 |
| R0e, per-entry hold (history), eps-hat = 1 | 378.7 | 80.1 | 0.0 | 0 | 0 | 27.8 | 35.51 | 4.70 | 1488.0 |
| R1 | 0.0 | 121.0 | 0.0 | 0 | 0 | 0.0 | 40.60 | 9.38 | 1512.1 |

### Replication of the I2 tail A/A with fresh phase draws (variants 2 and 3, 12 seeds)

| receiver, eps-hat | metric | C - D mean [CI] | T | reading |
|---|---|---|---|---|
| R0e engine | v_p99 | -2.791 [-18.58, 8.161] | 1 | undetermined |
| R0e engine | v_p999 | -2.839 [-18.48, 8.267] | 1 | undetermined |
| R0e engine | e_p99 | -2.791 [-18.58, 8.161] | 1 | undetermined |
| R0e engine | e_p999 | -2.839 [-18.48, 8.267] | 1 | undetermined |
| R0e engine | tail_time | -0.7262 [-17.09, 10.65] | 1 | undetermined |
| R0e engine | tail_v_mean | -2.298 [-17.68, 8.506] | 1 | undetermined |
| R0e engine | tail_v_p50 | -2.759 [-18.49, 8.181] | 1 | undetermined |
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

- R0e engine: variant 2 mean 1510.0, variant 3 mean 1510.7; SD of per-seed difference 26.3
- R0e cum: variant 2 mean 1495.2, variant 3 mean 1489.6; SD of per-seed difference 10.0
- R1 engine: variant 2 mean 1524.0, variant 3 mean 1511.7; SD of per-seed difference 41.3
- R0e (frozen A/A, cumulative eps-hat for R0e): variant 0 mean 1488.0, variant 1 mean 1495.5; SD of per-seed difference 9.5
- R1 (frozen A/A, cumulative eps-hat for R0e): variant 0 mean 1512.1, variant 1 mean 1522.6; SD of per-seed difference 41.8

### AG1 at Realtime under D_EWMA: n_dec before and after the first RTT sample (summed over 12 seeds)

- I1: n_dec 6, of which in the seed era (D = 25 ms) 6, after the first sample 0
- I2: n_dec 48, of which in the seed era (D = 25 ms) 13, after the first sample 35
- Q: n_dec 22, of which in the seed era (D = 25 ms) 6, after the first sample 16

### Reading labels, A/A on the global-hold model, and the accounting check

### Paired readings on the global-hold model (difference [CI] | m | label)

| cell | contrast | metric | difference [CI] | m | label |
|---|---|---|---|---|---|
| I2 | R0e - R1 | v_mean | -2.129 [-2.39, -1.945] | 3.03 | equivalent |
| I2 | R0e - R1 | v_p99 | -7.771 [-19.74, -0.6713] | 81.2 | equivalent |
| I2 | R0e - R1 | G_ratio | -9.605e-05 [-0.0001441, -5.488e-05] | 0.00183 | equivalent |
| I2 | R0e - R1 | repair_per_delivered | 0.01001 [0.005678, 0.01487] | 0.0859 | equivalent |
| I2 | R0e - R1 | tail_time | -7.395 [-19.22, 0] | 85.1 | equivalent |
| I2 | R0c - R0e | v_mean | -3.449 [-3.79, -3.08] | 3.03 | shown smaller |
| I2 | R0c - R0e | v_p99 | 6.191 [-3.093, 19.11] | 81.2 | equivalent |
| I2 | R0c - R0e | G_ratio | 0.002422 [0.00223, 0.002621] | 0.00183 | shown larger |
| I2 | R0c - R0e | repair_per_delivered | -0.1412 [-0.1518, -0.1299] | 0.0859 | shown smaller |
| I2 | R0c - R0e | tail_time | 7.407 [-1.743, 20.34] | 85.1 | equivalent |
| I2 | R1 - R0e | tail seqs with a verdict | -0.6667 [-1.667, 0.25] | 1 | undetermined |
| L10 | R0e - R1 | v_mean | -38 [-39.64, -36.34] | 14.3 | shown smaller |
| L10 | R0e - R1 | v_p99 | -31.79 [-56.59, -7.88] | 81.2 | equivalent |
| L10 | R0e - R1 | G_ratio | -8.232e-05 [-0.0001578, -2.744e-05] | 0.00183 | equivalent |
| L10 | R0e - R1 | repair_per_delivered | 0.00301 [0.0003863, 0.005396] | 0.0291 | equivalent |
| L10 | R0e - R1 | tail_time | -24.54 [-49.18, 0] | 85.1 | equivalent |
| L10 | R0c - R0e | v_mean | -3.259 [-3.831, -2.691] | 14.3 | equivalent |
| L10 | R0c - R0e | v_p99 | 12.91 [-18.02, 42.05] | 81.2 | equivalent |
| L10 | R0c - R0e | G_ratio | 0.002504 [0.002106, 0.002957] | 0.00183 | shown larger |
| L10 | R0c - R0e | repair_per_delivered | -0.03448 [-0.04061, -0.02911] | 0.0291 | shown smaller |
| L10 | R0c - R0e | tail_time | -1.409 [-26.42, 23.35] | 85.1 | equivalent |
| L10 | R1 - R0e | tail seqs with a verdict | 5.917 [3.667, 8.25] | 1 | shown larger |

Tail-seq verdict counts per run (mean over 12 seeds):

- I2: R0e 90.3, R0c 117.2, R1 89.7
- L10: R0e 92.3, R0c 115.8, R1 98.2

### A/A on the global-hold R0e and R0c (I2 tail cell, variants 0 and 1, 12 seeds): 95th percentile of |A - A'| per metric against the frozen floors

| receiver | metric | p95 abs(A - A') | frozen NF (tail cells) | bar m = max(T, NF) used in Part T | under NF? | under m? |
|---|---|---|---|---|---|---|
| R0e | v_mean | 0.9901 | 3.026 | 3.026 | yes | yes |
| R0e | repair_per_delivered | 0.01594 | 0.01585 | 0.08589 | NO, over by 9.336e-05 | yes |
| R0e | v_p99 | 49.58 | 81.18 | 81.18 | yes | yes |
| R0e | tail_time | 55.08 | 85.12 | 85.12 | yes | yes |
| R0e | G_ratio | 0.000284 | 0.001832 | 0.001832 | yes | yes |
| R0c | v_mean | 1.05 | 3.026 | 3.026 | yes | yes |
| R0c | repair_per_delivered | 0.02274 | 0.01585 | 0.08589 | NO, over by 0.006894 | yes |
| R0c | v_p99 | 78.08 | 81.18 | 81.18 | yes | yes |
| R0c | tail_time | 82.08 | 85.12 | 85.12 | yes | yes |
| R0c | G_ratio | 0.000284 | 0.001832 | 0.001832 | yes | yes |

### Accounting: R0c's extra G_rx over R0e, in packets per object, and its overlap with the seqs that get a verdict under R0c only

- I2: G_rx(R0c) - G_rx(R0e) = 29.4 packets per object (0.0024 of S_src); seqs with a verdict under R0c only 28.2; tail seqs in G under R0c but not under R0e 29.4; overlap of the two sets 28.2
- L10: G_rx(R0c) - G_rx(R0e) = 30.4 packets per object (0.0025 of S_src); seqs with a verdict under R0c only 24.4; tail seqs in G under R0c but not under R0e 25.2; overlap of the two sets 24.4

### What the passed packets do afterwards (Part T, per run, 12 seeds)

A passed packet that is later filled is delivered after the frontier has passed it, which is out of order in the engine (`reorder.rs:75-82`); one that never arrives stays in G_rx. Early-skip passes are the receiver's own; floor passes are the floor's.

| cell | receiver | early-skip passes | filled later (out of order) | never arrive | floor passes | floor-passed, filled later | floor-passed, never arrive |
|---|---|---|---|---|---|---|---|
| I2 | R0e | 278.8 | 250.7 | 28.2 | 91.7 | 0.0 | 91.7 |
| I2 | R0c | 290.2 | 252.2 | 38.0 | 111.2 | 0.0 | 111.2 |
| I2 | R1 | 0.0 | 0.0 | 0.0 | 121.0 | 0.0 | 121.0 |
| L10 | R0e | 859.8 | 830.8 | 28.9 | 91.1 | 0.0 | 91.1 |
| L10 | R0c | 853.2 | 823.8 | 29.4 | 121.0 | 0.0 | 121.0 |
| L10 | R1 | 0.0 | 0.0 | 0.0 | 121.0 | 0.0 | 121.0 |

In words: about 90 % (I2) and 97 % (L10) of the early-skip passes are later filled, so each is one out-of-order delivery; the floor passes are all of the dead window's tail, which no copy fills before the run ends, so none is delivered late.

### Mid-stream outage cell (reported only)

Part T conditions (I2 and L10 loss, Realtime, rho = 0.99, one path, S_src = 12 147 seqs per object), but every datagram, original, copy and floor, sent in a 1.5 s window starting at 40 % of the object's send time (0.486 s, ending 1.986 s) is lost, instead of the window at the tail. The source pauses at 2 048 outstanding (once per run), so the sends resume after the window ends: the last send is at about 2.54 s (I2) and 2.66 s (L10), well after it. 12 seeds, per run:

| cell | receiver | G_rx/S_src (abandoned at the end) | P1 breach events (worst excess) | early-skip passes | filled later | never arrived | floor passes | v mean ms | v median ms | tail time ms | stalled |
|---|---|---|---|---|---|---|---|---|---|---|---|
| I2 | R0e | 0.00722 | 9.8 (53.7) | 252.2 | 221.1 | 31.2 | 56.5 | 241.26 | 7.84 | 13.0 | 0/12 |
| I2 | R0c | 0.00814 | 19.2 (48.2) | 235.2 | 205.5 | 29.8 | 69.2 | 240.40 | 7.81 | 12.7 | 0/12 |
| I2 | R1 | 0.00679 | 0.0 (0.0) | 0.0 | 0.0 | 0.0 | 82.5 | 251.13 | 9.99 | 14.6 | 0/12 |
| I2 | U | 0.00679 | 0.0 (0.0) | 0.0 | 0.0 | 0.0 | 82.5 | 232.38 | 0.00 | 14.6 | 0/12 |
| L10 | R0e | 0.01079 | 132.4 (61.8) | 497.5 | 367.2 | 130.3 | 0.8 | 422.61 | 178.39 | 220.1 | 0/12 |
| L10 | R0c | 0.01520 | 108.4 (110.2) | 499.8 | 386.2 | 113.5 | 71.2 | 386.12 | 141.19 | 173.7 | 0/12 |
| L10 | R1 | 0.00600 | 0.0 (0.0) | 0.0 | 0.0 | 0.0 | 72.9 | 596.84 | 172.12 | 204.0 | 0/12 |
| L10 | U | 0.00600 | 0.0 (0.0) | 0.0 | 0.0 | 0.0 | 72.9 | 276.19 | 3.95 | 204.0 | 0/12 |

How v and e are defined in `toy2.py` (the same in every cell): v_j is, for every seq j the consumer was handed (in order, released past a hole, or by a late fill after a pass; a seq passed and never filled is not in v and is counted separately), the time from its loss-free arrival (the instant its original would have arrived, tau_j + owd + jitter + queue, whether or not the original was lost) to its delivery instant to the consumer; e_j is the same delivery instant minus the loss-free in-order delivery instant, the running maximum of those loss-free arrivals, which on this FIFO one-path model equals the loss-free arrival, so e equals v here. S_src is 12 147 seqs per object (about 12 074 delivered per run in this cell, the rest passed and never filled).

In words: the outage cell shows the same ordering as the tail cell with the cost moved to the middle: R0e and R0c finish the object about as fast as R1 (tail time under 15 ms in I2) because the outage is over long before the last send, and they break the promise more (about 10 to 19 pass events per run in I2, 108 to 132 in L10, worst excess 48 to 110 seqs); R0c's abandonment (0.0081 in I2, 0.0152 in L10) exceeds R0e's (0.0072, 0.0108) and, in L10, the promise (0.01); U has the lowest mean wait (232 and 276 ms) and a near-zero median, since it releases past the holes the outage leaves. Nothing stalled.

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
