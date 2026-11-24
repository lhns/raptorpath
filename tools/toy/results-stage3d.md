# Offline simulation, stage 3d: the consistent floor law, the engine-like sweep, recovery time, plants through T, the two-path cell

Spec: `docs/derivations/three-knobs.md` section 17 "Revision 3", registration 17.9 (branch `github/three-knobs`, head `bd64f394`). Code: `tools/toy/` (`toy2.py` extended; `batch3d.py`, `summarize3d.py`, `report_s3d.py`, `tables3d.py`, `check_ident3d.py`, tests in `test_toy2.py` class `Stage3d`); built on `toy-stage3-knobs` `02eda90b`. With every new field at its default the code reproduces stages 1 to 3c bit for bit (below). The offline simulation fixes no constant of the engine, and none of these numbers is engine provenance. 12 seeds per configuration (seeds 0 to 11, clock phases drawn per seed), one path except P2, all runs deterministic. Data lives in `out3d/` (not tracked); the frozen A/A floor `nf_stage3d.json` is tracked. Tests: `python -m unittest test_toy test_toy2` 88 tests OK on the final code (77 existing, 11 new in class `Stage3d`; the new tests were red on the stage 3c code, 10 of 10 erroring on the missing fields, before they passed). Identity check: `check_ident3d.py` PASS on the final code (1 984 reference runs, 253 075 old keys, 0 differences).

## In plain words

- **The identities hold; liveness fails in one ablation arm.** With the new fields at their defaults the code reproduces the stage 3c code (and through it stages 1 to 3b) on 1 984 reference runs. The new sender at lifetime T = infinity equals the never-give-up sender on 37 readouts, and at T = 10^6 s equals T = infinity, in all 21 cells (traces identical, seeds 0 to 2). 5 620 of the 5 628 stored runs finish. The 8 that do not are all the floor-law ablation (S5v, "ADR-0084's verdict floor") in the 40 s overload cell: its registered give-up rule cannot judge a hole that was sent in the same instant as the newest datagram that arrived and has nothing sent after it. That is a gap in the registered ablation rule, not in the design; an unregistered supplement with one extra timeout finishes all 96 runs of that arm (48 in the long cells) and reads the same shares.
- **The commitment is now absolute, and the re-check never had to act.** No copy was ever staged below an announced floor in 5 628 runs (stage 3c: 143 in 28 runs). `skip_below_floor` is 0 in every run: the corrected law (a never-resent unit is protected until its own trigger, with the forward one-way delay in place of half the RTT) leaves nothing for the re-check to catch, so the re-check is exercised only on constructed state in the tests. The separate cause of the 598 resends at K-L10b's lowest lifetime is confirmed: 79 % of those copies arrive as duplicates.
- **Overload with the consistent law.** At a 0.4 s lifetime the overload cell delivers 83.1 % within 0.4 s with the receiver releasing at once and 82.8 % with strict order (blocker sweep: 83.8 % both). The ablation with the old verdict floor keeps 80.9 % at any order and collapses to 6.8 % at strict order, as registered; at any order it reads 2.2 points below the design, a miss of the 2-point bar by 0.16 point.
- **Recovery time after the outage, the headline of the long capacity cell** (outage 2.0 to 3.5 s, 20 s of offered load): never giving up needs 7.8 s to be on time again (6.7 s with the engine-like sweep), ADR-0084's sender with today's receiver 5.7 s (4.8 s), the lifetime sender at 2 s with any-order receiver 6.6 s (5.6 s), and the lifetime sender at 0.4 s 0.1 s, which is the resolution of the window series. The registered bands for the never-give-up sender are missed above by 0.19 s and 0.33 s.
- **The engine-like sweep removes the duplicates** (blocker only, receiver repeats its report): the never-give-up sender's duplicate arrivals fall from 49 % to 3 % of its copies in the overload cell, and its copies per delivered unit halve. The lifetime sender at 0.4 s moves by less than 1 point between the two sweeps; its never-expiring variants move by up to 13 points because they inherit the baseline's duplicates.
- **The 2 s lifetime where it binds** (40 s overload): 80.5 % (tail) and 80.8 % (blocker) of the units are delivered within 2 s of hand-over in the windows after the first dequeue expiry, against the registered steady state 80.8 %, and 0 % for the never-give-up and ADR-0084 senders. Strict order does not collapse (also 80.5 %).
- **Planted controls through T have power in the overload cell and none in the call cell.** The step and the kink read K 0.69 / P 0.35 (registered 0.65 / 0.33); the spike reads P -0.69. In the call cell (K-L10) all three read no effect (share within 0.0004 over the whole range), but K reads "undetermined" rather than "equivalent" against the 1/units bar. The unplanted nudges read equivalent on 368 of 392 K and P statistics, 24 undetermined, none shown.
- **Two paths: the registered mechanism is not the one that operates.** At the 0.075 s lifetime the floor does pass slow-path units that are still in flight, but only 6.5 % of the units (registered 35 to 50 %), because the floor trails behind lost fast-path holes, which stay protected for about 65 ms. With the estimated floor hold the late fills fall to 1.0 % and none occur after the first second, as registered, but the strict-order delay p50 reads 121 ms (registered 55 to 70 ms): the hold adds to the 65 ms trail instead of replacing it, and only 2.0 % of the units arrive within the lifetime (registered: equal to the H_f = 0 arm within 2 points). At 0.4 s the "vacuous" case is not vacuous: the 333 ms RTT seed leaves the first 65 ms of slow-path units unprotected, 105 late fills per run, all in the first 0.1 s.

## Readings (Q3d-1 to Q3d-9)

| | reading | key numbers |
|---|---|---|
| Q3d-1 | **identity PASS; liveness FAIL** | stages 1 to 3c reproduced: `check_ident3d.py` PASS on the final code (1 984 reference runs regenerated from `02eda90b`; 253 075 old keys, traces and per-sequence arrays as digests, 0 differences). S5 at T = inf equals C3 and S5 at T = 1e6 equals T = inf on 37 keys: 432 of 432 seed-level comparisons (21 cells, both sweep forms, P2; 12 seeds, 3 in the four long cells); sender traces identical in all 21 cells x seeds 0 to 2 x 3 arms. `finished`: 5 620 of 5 628 stored runs. **Unfinished: 8, all the ablation S5v in the 40 s overload cell**: OV-long seed 0 (H = 0 and H = inf), OV-long@b seeds 3, 6, 9 (both H). Cause: the stuck hole was sent at the same instant as the newest arrived datagram (a burst after the pause released), nothing was sent later, so `last_send < tau_rx` never holds, and it lies below the newest received seq, so the registered timeout for the open tail does not apply; the 480 000-unit source then ends at the intake and the hole is never judged (seed 0: hole 389 469, sent 40.29 s, `A_known` frozen, run limit 120 s). Registered rule implemented as written; the supplement `SUP-S5v` (timeout also for a hole no later send has overtaken) finishes 96 of 96 runs in OV, OV@b, OV-long, OV-long@b (the 8 unfinished ones among them) and reads identical shares |
| Q3d-2 | **PASS on the design; the dup bullet confirms** | `commit_viol` = 0 in 5 628 of 5 628 runs. `skip_below_floor` = 0 in every run, so the bar 0.001 x copies holds in every one-path cell (20 one-path cells including X15 and X20; the largest cell, M-I2, has 14 996 155 copies). K-L10b, S5 at T = 0.180 s, H = 0: copies 598 or 599 per seed, duplicate arrivals 449 to 486, 0.792 of copies >= 0.5: confirmed (at T = 0.4: 0.533; C3: 0.422). The re-check is exercised only on constructed state (test 3): it never bound in a registered run |
| Q3d-3 | **PASS on 5 of 6 bullets; the S5v H = 0 bar is missed by 0.0016** | OV, tail form: S5 H = 0 `share_by_0p4` 0.8309 (band 0.78 to 0.84): PASS; S5 H = inf 0.8283 (0.75 to 0.84): PASS, at least H = 0 minus 0.03 = 0.8009: PASS (paired -0.0026 [-0.0032, -0.0021]). Blocker form: 0.8376 and 0.8376, both PASS. S5v H = inf 0.0677 (0.05 to 0.25): PASS (blocker 0.0813), verdicts mean 2 632 (min 2 564) > 1 000: PASS (2 600, min 2 523), all 12 runs finished in OV: PASS. **S5v H = 0 0.8093 against S5 H = 0 0.8309: -0.0216, bar 0.02: FAIL** (blocker -0.0211: FAIL) |
| Q3d-4 | **PASS on bullets 1 and 2; FAIL on bullet 3 read over every S5 row, PASS on the T = 0.4 rows** | copies per delivered unit, tail to blocker: C3 OV 0.0723 to 0.0343, CL 0.0725 to 0.0471; C1 OV 0.0627 to 0.0301, CL 0.0604 to 0.0301: falls in all four. Duplicate share (arrivals of already-received seqs per copy), C3: OV 0.487 to 0.030, CL 0.282 to 0.015 (below 0.1): PASS; C1 OV 0.571 to 0.003, CL 0.360 to 0.002. S5 rows between forms (`share_by_0p4`, `share_by_2`): the T = 0.4 rows move by at most 0.0093 (OV) and 0.0007 (CL): PASS; the T = 2 and T = inf rows move by up to **0.1306 (OV, T = inf, H = 0, `share_by_2`) and 0.0879 (CL, T = 2, H = 0, `share_by_0p4`)**: FAIL on the reading "every S5 row < 0.02" |
| Q3d-5 | **FAIL on the C3 bands and on S5 T = 2 H = 0 equivalence; PASS on the rest** | CL-long, seconds after the outage end (mean over 12 seeds). Tail sweep: C3 **7.79** (band 5.4 to 7.6): FAIL by 0.19; C1 5.68 (reported); S5 T = 2 H = inf 7.78 (against C3 -0.008 [-0.025, 0], bar 0.39: equivalent): PASS; S5 T = 2 H = 0 6.61 (against C3 **-1.18 [-1.29, -1.05]: shown smaller**: FAIL; against S5 at T = inf H = 0, 6.66, the same receiver: equivalent); S5 T = 0.4 0.10 at H = 0 (band 0 to 0.6): PASS and 0.10 at H = inf (0 to 1.0): PASS. Blocker sweep: C3 **6.73** (band 4.1 to 6.4): FAIL by 0.33; C1 4.80; S5 T = 2 H = inf 6.59 (-0.13, equivalent), H = 0 5.63 (-1.09, shown smaller); S5 T = 0.4 0.10 and 0.10: PASS. CL (10 s): C3 "not within the run" in 12 of 12 seeds in both forms: PASS; S5 T = 0.4 0.10 s <= 0.6 s: PASS. M cells: no recovery time (the pre-outage interval is empty), see worker decisions |
| Q3d-6 | **PASS** | OV-long, windowed share within 2 s of hand-over, windows after the first dequeue expiry (8.8 to 9.0 s) and before 38 s. Tail: S5 T = 2 H = 0 **0.8051** (band 0.78 to 0.83; steady state 0.8083): PASS; C3 0.0000 and C1 0.0000 (<= 0.05): PASS; S5 T = 2 H = inf 0.8051 (0.40 to 0.83): in band. Blocker: 0.8075, 0.0000, 0.0000, 0.8075 |
| Q3d-7 | **OV step and kink PASS; the spike misses on K (undetermined); K-L10 "no power" with K undetermined; unplanted nudges 368 of 392 equivalent, 24 undetermined** | OV `share_T` at 0.98 to 1.02 p: step 0.1346 0.1360 0.8309 0.8312 0.8315, K 0.6935 [0.6925, 0.6945], P 0.3468 [0.3463, 0.3472] (0.65 +- 0.15, 0.33 +- 0.10): PASS. Kink 0.1342 0.1354 0.8309 0.8312 0.8315, K 0.6943, P 0.3472: PASS. Spike 0.8304 0.8306 0.1374 0.8312 0.8315: P -0.6935 [-0.6944, -0.6925] (-0.65 +- 0.15): PASS; K -1.6e-5 [-7.2e-5, 4.2e-5] against m = 8.3e-6 (= 1/units): **undetermined, registered "equivalent": FAIL** by the reading rule (the interval is 4 orders below the shown effect). K-L10 (share 0.9791 to 0.9795 over the range): step K -1.9e-4 [-4.4e-4, 5.6e-5] and P -1.8e-4 (equivalent); kink the same; spike K -1.7e-4 [-4.7e-4, 1.1e-4], P -1.1e-4: **K undetermined against m = 3.3e-4 in all three** (registered: all three equivalent). Unplanted nudges, 18 cells (every cell but X15, X20 and P2) x 2 knobs x 6 readouts: 0 shown, 24 undetermined, 368 equivalent. The undetermined, all within about one bar of equivalent: K-L10 `share_by_2` K; CL `share_by_2` K and P and `ooo_per_delivered` K and P; OV `ooo_per_delivered` K and P; the blocker-form cells K-I2, K-L10, K-L10b `share_by_2` and `ooo_per_delivered` K (5), OV `ooo_per_delivered` K and P; CL-long `share_by_2`, `dh_p90` and `ooo_per_delivered` K and P (6); OV-long and OV-long@b `ooo_per_delivered` K and P (4) |
| Q3d-8 | **6 of 11 registered bullets FAIL, 5 PASS** | P2 (seed means). T = 0.075, H = inf, H_f = 0: late fills / units **0.0651** (band 0.35 to 0.50): FAIL. H_f = est: late fills / units 0.0102 (<= 0.03): PASS; after 1 s: 0 (must be 0): PASS; H_f 50.8 ms (45 to 55): PASS; `pass_floor` 491 > 0: PASS; strict-order delay p50 **121.4 ms** (55 to 70): FAIL; at H_f = 0 strict 70.6 ms, fast-path units at any order 10.5 ms (9 to 20): PASS on that reading; `share_T` est **0.0203** against H_f = 0 **0.9149** (within 0.02): FAIL. T = 0.4: late fills 105.2 per run (must be 0), `pass_floor` / units 0.0093 (<= 0.002): FAIL (cold start, below). Duplicates per copy (<= 0.1): **0.297 for C3 and 0.298 for the T = 0.4 arms**: FAIL (the 0.075 arms send no copies). `pause_events` 0 and `resend_braked` at most 8 over every P2 run: the engine-like settings are inert |
| Q3d-9 | **PASS; the stall parts reproduce stage 3c** | S5 T = 0.4, H = inf against 3c's S4 (the 3c code path, 12 seeds), `share_by_0p4`: K-I2 0.98147 against 0.98150, K-L10 0.97889 against 0.97894, K-L10b 0.97847 against 0.97850; paired -2.8e-5, -5.6e-5, -2.8e-5, all equivalent (m 0.00095, 0.0061, 0.0064): PASS. Stall parts in both sweep forms: (i) within bound in every episode (K-I2 1 of 1, K-L10 8 of 8, K-L10b 8 of 8; largest 0.041 s); (o) at K-L10 the non-zero values 0.0191, 0.0195, 0.0397, 0.0796, 0.1791 s are the values of stage 3c, 3 of 5 within the 0.072 s bound (seeds 4 and 6 exceed it, as in stage 3c); the unplanted nudges: Q3d-7 |

## Headline: recovery time after the outage in the long capacity cell (CL-long)

The link carries 12 500 units/s, the source offers 10 000/s for 20 s, the outage is 2.0 to 3.5 s. Recovery time = seconds after the outage end until the share delivered within 0.4 s of hand-over, in 0.5 s windows stepped by 0.1 s, is back at the pre-outage level and stays there to 2 s before the last hand-over. 12 seeds; 0.1 s is the resolution.

| arm (sender; receiver) | tail sweep (resends the whole open tail every period) | engine-like sweep (only the blocking unit; receiver repeats its report) |
|---|---|---|
| ADR-0084's sender (gives up on a hole when the 1 % allowance permits) with today's early-skip receiver (`C1`) | 5.68 s | 4.80 s |
| never gives up, strict order (`C3`) | 7.79 s | 6.73 s |
| S5, lifetime 2 s, strict order | 7.78 s | 6.59 s |
| S5, lifetime 2 s, any order | 6.61 s | 5.63 s |
| S5, never gives up, any order | 6.66 s | 5.67 s |
| S5, lifetime 0.4 s, any order | 0.10 s | 0.10 s |
| S5, lifetime 0.4 s, strict order | 0.10 s | 0.10 s |

The same arms in the 10 s capacity cell (CL, outage 2.0 to 3.5 s): the never-give-up, ADR-0084 and 2 s arms are "not within the run" in 12 of 12 seeds in both forms; the 0.4 s arms recover in 0.10 s.

## Per-cell tables in plain words

Seed means over 12 seeds. "Delivered at all", "within 0.4 s" and "within 2 s" are fractions of the units handed over (a unit never delivered, or given up, counts as not delivered). Delay is measured from hand-over at the sender; a percentile is "never" when it falls on an undelivered unit. No mean over delivered units alone is a headline. Cells: E = outage at the end of a 12 147-unit object; M = outage in the middle; K = a call (50 units per second, 60 s, 1.5 s outage at 20 s); I2 = 3 % loss, 5 ms path; L10 = 10 % loss, 50 ms path (K-L10b 60 ms); CL = capacity-limited link (12 500 units/s, queue 1 250, offered 10 000/s for 10 s, outage 2 to 3.5 s); OV = overload (link 10 000/s, offered 12 000/s for 10 s); CL-long and OV-long = 20 s and 40 s of the same; "@b" = the same cell with the engine-like sweep and receiver refresh for every arm; P2 = two paths. The `S5` sender used below is the stage 3d sender: a floor that is the lowest unit it still means to deliver, the trigger-delay law and the forward one-way delay in the protection test, no feedback verdict, re-check before every copy.

#### E-I2

Sweep for every arm in this cell: resends the whole open tail every period (period fixed from the base RTT: 25 to 100 ms).

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early) | 99.0 % | 96.9 % | 99.0 % | 12 / 16 / 1522 | 1.31 | 0.0208 |
| `C3`: sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole) | 100.0 % | 97.2 % | 100.0 % | 16 / 28 / 1532 | 1.72 | 0 |
| `C3S`: sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order | 100.0 % | 97.2 % | 100.0 % | 16 / 28 / 1532 | 1.72 | 0 |
| `G-T0p4-H0`: sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0) | 97.2 % | 97.2 % | 97.2 % | 5.5 / 5.9 / never (inf) | 0.465 | 0.0294 |
| `G-T0p4-Hinf`: sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it | 97.2 % | 97.2 % | 97.2 % | 16 / 28 / never (inf) | 0.465 | 0 |
| `G-T2-H0`: sender: S5, lifetime 2 s; receiver: any order (H = 0) | 100.0 % | 97.2 % | 100.0 % | 5.5 / 5.9 / 1502 | 1.72 | 0.0314 |
| `G-T2-Hinf`: sender: S5, lifetime 2 s; receiver: strict order | 100.0 % | 97.2 % | 100.0 % | 16 / 28 / 1532 | 1.72 | 0 |
| `G-Tinf-H0`: sender: S5 with no lifetime (never gives up, no floor); receiver: any order | 100.0 % | 97.2 % | 100.0 % | 5.5 / 5.9 / 1502 | 1.72 | 0.0314 |
| `G-T0p4-H0p02`: sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing | 97.2 % | 97.2 % | 97.2 % | 16 / 23 / never (inf) | 0.465 | 0.00274 |

#### E-L10

Sweep for every arm in this cell: resends the whole open tail every period (period fixed from the base RTT: 25 to 100 ms).

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early) | 99.0 % | 96.9 % | 99.0 % | 150 / 153 / 1802 | 0.546 | 0.0691 |
| `C3`: sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole) | 100.0 % | 92.4 % | 99.8 % | 285 / 380 / 1833 | 0.582 | 0 |
| `C3S`: sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order | 100.0 % | 92.4 % | 99.8 % | 285 / 380 / 1833 | 0.582 | 0 |
| `G-T0p4-H0`: sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0) | 97.1 % | 97.1 % | 97.1 % | 51 / 152 / never (inf) | 0.206 | 0.101 |
| `G-T0p4-Hinf`: sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it | 97.1 % | 97.0 % | 97.1 % | 280 / 347 / never (inf) | 0.206 | 0 |
| `G-T2-H0`: sender: S5, lifetime 2 s; receiver: any order (H = 0) | 100.0 % | 97.1 % | 100.0 % | 51 / 152 / 1592 | 0.582 | 0.101 |
| `G-T2-Hinf`: sender: S5, lifetime 2 s; receiver: strict order | 100.0 % | 92.4 % | 99.8 % | 285 / 380 / 1833 | 0.582 | 0 |
| `G-Tinf-H0`: sender: S5 with no lifetime (never gives up, no floor); receiver: any order | 100.0 % | 97.1 % | 100.0 % | 51 / 152 / 1592 | 0.582 | 0.101 |
| `G-T0p4-H0p02`: sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing | 97.1 % | 97.1 % | 97.1 % | 70 / 152 / never (inf) | 0.206 | 0.101 |

#### M-I2

Sweep for every arm in this cell: resends the whole open tail every period (period fixed from the base RTT: 25 to 100 ms).

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early) | 99.3 % | 39.9 % | 99.3 % | 1327 / 1422 / 1520 | 9.26 | 0.0183 |
| `C3`: sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole) | 100.0 % | 39.3 % | 100.0 % | 1330 / 1439 / 1527 | 9.45 | 0 |
| `C3S`: sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order | 100.0 % | 39.3 % | 100.0 % | 1330 / 1439 / 1527 | 9.45 | 0 |
| `G-T0p4-H0`: sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0) | 40.0 % | 40.0 % | 40.0 % | never (inf) / never (inf) / never (inf) | 6.26 | 0.0285 |
| `G-T0p4-Hinf`: sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it | 40.0 % | 39.3 % | 40.0 % | never (inf) / never (inf) / never (inf) | 6.26 | 0 |
| `G-T2-H0`: sender: S5, lifetime 2 s; receiver: any order (H = 0) | 100.0 % | 40.0 % | 100.0 % | 1322 / 1395 / 1504 | 9.45 | 0.0415 |
| `G-T2-Hinf`: sender: S5, lifetime 2 s; receiver: strict order | 100.0 % | 39.3 % | 100.0 % | 1330 / 1439 / 1527 | 9.45 | 0 |
| `G-Tinf-H0`: sender: S5 with no lifetime (never gives up, no floor); receiver: any order | 100.0 % | 40.0 % | 100.0 % | 1322 / 1395 / 1504 | 9.45 | 0.0415 |
| `G-T0p4-H0p02`: sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing | 40.0 % | 40.0 % | 40.0 % | never (inf) / never (inf) / never (inf) | 6.26 | 0.00264 |

#### M-L10

Sweep for every arm in this cell: resends the whole open tail every period (period fixed from the base RTT: 25 to 100 ms).

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early) | 98.9 % | 38.9 % | 98.9 % | 1665 / 1776 / 1840 (never in 10 of 12 seeds) | 2.7 | 0.0306 |
| `C3`: sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole) | 100.0 % | 17.6 % | 95.6 % | 1765 / 1922 / 1995 | 2.65 | 0 |
| `C3S`: sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order | 100.0 % | 17.6 % | 95.6 % | 1765 / 1922 / 1995 | 2.65 | 0 |
| `G-T0p4-H0`: sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0) | 39.1 % | 39.1 % | 39.1 % | never (inf) / never (inf) / never (inf) | 1.17 | 0.0795 |
| `G-T0p4-Hinf`: sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it | 39.1 % | 19.9 % | 39.1 % | never (inf) / never (inf) / never (inf) | 1.17 | 0 |
| `G-T2-H0`: sender: S5, lifetime 2 s; receiver: any order (H = 0) | 100.0 % | 39.1 % | 100.0 % | 1512 / 1559 / 1699 | 2.65 | 0.1 |
| `G-T2-Hinf`: sender: S5, lifetime 2 s; receiver: strict order | 100.0 % | 17.6 % | 98.7 % | 1760 / 1889 / 1959 | 2.65 | 0 |
| `G-Tinf-H0`: sender: S5 with no lifetime (never gives up, no floor); receiver: any order | 100.0 % | 39.1 % | 100.0 % | 1512 / 1559 / 1699 | 2.65 | 0.1 |
| `G-T0p4-H0p02`: sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing | 39.1 % | 39.1 % | 39.1 % | never (inf) / never (inf) / never (inf) | 1.17 | 0.0795 |

#### K-I2

Sweep for every arm in this cell: resends the whole open tail every period (period fixed from the base RTT: 25 to 100 ms).

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early) | 99.5 % | 97.9 % | 99.5 % | 5.5 / 6.0 / 1084 | 0.647 | 0.024 |
| `C3`: sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole) | 100.0 % | 98.1 % | 100.0 % | 5.5 / 6.0 / 924 | 0.777 | 0 |
| `C3S`: sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order | 100.0 % | 98.1 % | 100.0 % | 5.5 / 6.0 / 924 | 0.777 | 0 |
| `G-T0p4-H0`: sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0) | 98.2 % | 98.2 % | 98.2 % | 5.5 / 6.0 / never (inf) | 0.36 | 0.03 |
| `G-T0p4-Hinf`: sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it | 98.2 % | 98.1 % | 98.2 % | 5.5 / 6.0 / never (inf) | 0.36 | 0 |
| `G-T2-H0`: sender: S5, lifetime 2 s; receiver: any order (H = 0) | 100.0 % | 98.2 % | 100.0 % | 5.5 / 6.0 / 902 | 0.777 | 0.0341 |
| `G-T2-Hinf`: sender: S5, lifetime 2 s; receiver: strict order | 100.0 % | 98.1 % | 100.0 % | 5.5 / 6.0 / 924 | 0.777 | 0 |
| `G-Tinf-H0`: sender: S5 with no lifetime (never gives up, no floor); receiver: any order | 100.0 % | 98.2 % | 100.0 % | 5.5 / 6.0 / 902 | 0.777 | 0.0341 |
| `G-T0p4-H0p02`: sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing | 98.2 % | 98.2 % | 98.2 % | 5.5 / 6.0 / never (inf) | 0.36 | 0.00283 |

#### K-L10

Sweep for every arm in this cell: resends the whole open tail every period (period fixed from the base RTT: 25 to 100 ms).

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early) | 99.0 % | 97.0 % | 99.0 % | 51 / 171 / 1529 (never in 4 of 12 seeds) | 0.31 | 0.0625 |
| `C3`: sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole) | 100.0 % | 97.7 % | 100.0 % | 66 / 174 / 1140 | 0.353 | 0 |
| `C3S`: sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order | 100.0 % | 97.7 % | 100.0 % | 66 / 174 / 1140 | 0.353 | 0 |
| `G-T0p4-H0`: sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0) | 97.9 % | 97.9 % | 97.9 % | 51 / 170 / never (inf) | 0.2 | 0.104 |
| `G-T0p4-Hinf`: sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it | 97.9 % | 97.9 % | 97.9 % | 66 / 174 / never (inf) | 0.2 | 0 |
| `G-T2-H0`: sender: S5, lifetime 2 s; receiver: any order (H = 0) | 100.0 % | 97.9 % | 100.0 % | 51 / 170 / 1007 | 0.353 | 0.121 |
| `G-T2-Hinf`: sender: S5, lifetime 2 s; receiver: strict order | 100.0 % | 97.7 % | 100.0 % | 66 / 174 / 1140 | 0.353 | 0 |
| `G-Tinf-H0`: sender: S5 with no lifetime (never gives up, no floor); receiver: any order | 100.0 % | 97.9 % | 100.0 % | 51 / 170 / 1007 | 0.353 | 0.121 |
| `G-T0p4-H0p02`: sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing | 97.9 % | 97.9 % | 97.9 % | 51 / 170 / never (inf) | 0.2 | 0.103 |

#### K-L10b

Sweep for every arm in this cell: resends the whole open tail every period (period fixed from the base RTT: 25 to 100 ms).

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early) | 99.1 % | 97.0 % | 99.1 % | 62 / 189 / 1519 (never in 1 of 12 seeds) | 0.496 | 0.06 |
| `C3`: sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole) | 100.0 % | 97.5 % | 100.0 % | 90 / 202 / 1150 | 0.566 | 0 |
| `C3S`: sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order | 100.0 % | 97.5 % | 100.0 % | 90 / 202 / 1150 | 0.566 | 0 |
| `G-T0p4-H0`: sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0) | 97.9 % | 97.9 % | 97.9 % | 61 / 187 / never (inf) | 0.41 | 0.104 |
| `G-T0p4-Hinf`: sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it | 97.9 % | 97.8 % | 97.9 % | 90 / 202 / never (inf) | 0.41 | 0 |
| `G-T2-H0`: sender: S5, lifetime 2 s; receiver: any order (H = 0) | 100.0 % | 97.9 % | 100.0 % | 61 / 187 / 1017 | 0.566 | 0.121 |
| `G-T2-Hinf`: sender: S5, lifetime 2 s; receiver: strict order | 100.0 % | 97.5 % | 100.0 % | 90 / 202 / 1150 | 0.566 | 0 |
| `G-Tinf-H0`: sender: S5 with no lifetime (never gives up, no floor); receiver: any order | 100.0 % | 97.9 % | 100.0 % | 61 / 187 / 1017 | 0.566 | 0.121 |
| `G-T0p4-H0p02`: sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing | 97.9 % | 97.9 % | 97.9 % | 61 / 187 / never (inf) | 0.41 | 0.103 |

#### K-I2@b

Sweep for every arm in this cell: resends only the unit blocking the cumulative ack, when its deadline (about 2 smoothed RTTs, 25 to 100 ms, from its last send) expires; the receiver repeats its report every 2 smoothed RTTs while it holds holes.

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early) | 99.5 % | 97.8 % | 99.5 % | 5.5 / 6.0 / 1194 | 0.0539 | 0.0235 |
| `C3`: sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole) | 100.0 % | 98.1 % | 100.0 % | 5.5 / 6.0 / 939 | 0.0789 | 0 |
| `C3S`: sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order | 100.0 % | 98.1 % | 100.0 % | 5.5 / 6.0 / 939 | 0.0789 | 0 |
| `G-T0p4-H0`: sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0) | 98.1 % | 98.1 % | 98.1 % | 5.5 / 6.0 / never (inf) | 0.0441 | 0.0345 |
| `G-T0p4-Hinf`: sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it | 98.1 % | 98.1 % | 98.1 % | 5.5 / 6.0 / never (inf) | 0.0441 | 0 |
| `G-T2-H0`: sender: S5, lifetime 2 s; receiver: any order (H = 0) | 100.0 % | 98.1 % | 100.0 % | 5.5 / 6.0 / 924 | 0.0789 | 0.0526 |
| `G-T2-Hinf`: sender: S5, lifetime 2 s; receiver: strict order | 100.0 % | 98.1 % | 100.0 % | 5.5 / 6.0 / 939 | 0.0789 | 0 |
| `G-Tinf-H0`: sender: S5 with no lifetime (never gives up, no floor); receiver: any order | 100.0 % | 98.1 % | 100.0 % | 5.5 / 6.0 / 924 | 0.0789 | 0.0526 |
| `G-T0p4-H0p02`: sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing | 98.1 % | 98.1 % | 98.1 % | 5.5 / 6.0 / never (inf) | 0.0441 | 0.00328 |

#### K-L10@b

Sweep for every arm in this cell: resends only the unit blocking the cumulative ack, when its deadline (about 2 smoothed RTTs, 25 to 100 ms, from its last send) expires; the receiver repeats its report every 2 smoothed RTTs while it holds holes.

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early) | 99.0 % | 97.1 % | 99.0 % | 51 / 171 / 1590 (never in 2 of 12 seeds) | 0.252 | 0.0705 |
| `C3`: sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole) | 100.0 % | 97.5 % | 100.0 % | 65 / 172 / 1223 | 0.283 | 0 |
| `C3S`: sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order | 100.0 % | 97.5 % | 100.0 % | 65 / 172 / 1223 | 0.283 | 0 |
| `G-T0p4-H0`: sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0) | 97.8 % | 97.8 % | 97.8 % | 51 / 171 / never (inf) | 0.259 | 0.103 |
| `G-T0p4-Hinf`: sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it | 97.8 % | 97.8 % | 97.8 % | 65 / 172 / never (inf) | 0.259 | 0 |
| `G-T2-H0`: sender: S5, lifetime 2 s; receiver: any order (H = 0) | 100.0 % | 97.8 % | 100.0 % | 51 / 171 / 1082 | 0.283 | 0.123 |
| `G-T2-Hinf`: sender: S5, lifetime 2 s; receiver: strict order | 100.0 % | 97.5 % | 100.0 % | 65 / 172 / 1223 | 0.283 | 0 |
| `G-Tinf-H0`: sender: S5 with no lifetime (never gives up, no floor); receiver: any order | 100.0 % | 97.8 % | 100.0 % | 51 / 171 / 1082 | 0.283 | 0.123 |
| `G-T0p4-H0p02`: sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing | 97.8 % | 97.8 % | 97.8 % | 51 / 171 / never (inf) | 0.259 | 0.103 |

#### K-L10b@b

Sweep for every arm in this cell: resends only the unit blocking the cumulative ack, when its deadline (about 2 smoothed RTTs, 25 to 100 ms, from its last send) expires; the receiver repeats its report every 2 smoothed RTTs while it holds holes.

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early) | 99.1 % | 97.1 % | 99.1 % | 67 / 201 / 1608 (never in 1 of 12 seeds) | 0.246 | 0.0706 |
| `C3`: sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole) | 100.0 % | 97.3 % | 100.0 % | 95 / 204 / 1296 | 0.285 | 0 |
| `C3S`: sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order | 100.0 % | 97.3 % | 100.0 % | 95 / 204 / 1296 | 0.285 | 0 |
| `G-T0p4-H0`: sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0) | 97.6 % | 97.6 % | 97.6 % | 61 / 201 / never (inf) | 0.252 | 0.102 |
| `G-T0p4-Hinf`: sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it | 97.6 % | 97.6 % | 97.6 % | 93 / 202 / never (inf) | 0.252 | 0 |
| `G-T2-H0`: sender: S5, lifetime 2 s; receiver: any order (H = 0) | 100.0 % | 97.7 % | 100.0 % | 61 / 201 / 1116 | 0.285 | 0.123 |
| `G-T2-Hinf`: sender: S5, lifetime 2 s; receiver: strict order | 100.0 % | 97.3 % | 100.0 % | 95 / 204 / 1296 | 0.285 | 0 |
| `G-Tinf-H0`: sender: S5 with no lifetime (never gives up, no floor); receiver: any order | 100.0 % | 97.7 % | 100.0 % | 61 / 201 / 1116 | 0.285 | 0.123 |
| `G-T0p4-H0p02`: sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing | 97.6 % | 97.6 % | 97.6 % | 61 / 201 / never (inf) | 0.252 | 0.102 |

#### CL

Sweep for every arm in this cell: resends the whole open tail every period (period fixed from the base RTT: 25 to 100 ms).

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered | link busy over 1-10 s | queue drops outside / inside outage | resends refused by the budget | duplicate arrivals per copy | recovery time after the outage (s) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| `C1`: sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early) | 98.7 % | 28.6 % | 98.7 % | 724 / 1337 / never (inf) | 0.0604 | 0.0265 | 82.8 % | 0 / 0 | 233298 | 0.360 | not within the run |
| `C3`: sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole) | 100.0 % | 19.9 % | 97.2 % | 982 / 1595 / 2132 | 0.0725 | 0 | 82.0 % | 0 / 0 | 393203 | 0.282 | not within the run |
| `C3S`: sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order | 100.0 % | 19.9 % | 97.2 % | 982 / 1595 / 2132 | 0.0725 | 0 | 82.0 % | 0 / 0 | 393203 | 0.282 | not within the run |
| `G-T0p4-H0`: sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0) | 84.8 % | 84.8 % | 84.8 % | 5.7 / never (inf) / never (inf) | 0.0353 | 0.0297 | 69.8 % | 0 / 0 | 14548 | 0.000 | 0.10 s |
| `G-T0p4-Hinf`: sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it | 84.8 % | 84.7 % | 84.8 % | 17 / never (inf) / never (inf) | 0.0353 | 0 | 69.8 % | 0 / 0 | 14548 | 0.000 | 0.10 s |
| `G-T2-H0`: sender: S5, lifetime 2 s; receiver: any order (H = 0) | 99.9 % | 21.8 % | 99.8 % | 822 / 1418 / 1615 | 0.0717 | 0.0382 | 82.0 % | 0 / 0 | 383519 | 0.292 | not within the run |
| `G-T2-Hinf`: sender: S5, lifetime 2 s; receiver: strict order | 99.9 % | 19.9 % | 99.7 % | 975 / 1582 / 1972 | 0.0717 | 0 | 82.0 % | 0 / 0 | 383519 | 0.292 | not within the run |
| `G-Tinf-H0`: sender: S5 with no lifetime (never gives up, no floor); receiver: any order | 100.0 % | 21.4 % | 99.8 % | 829 / 1425 / 1626 | 0.0725 | 0.0394 | 82.0 % | 0 / 0 | 393203 | 0.282 | not within the run |
| `G-T0p4-H0p02`: sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing | 84.8 % | 84.8 % | 84.8 % | 16 / never (inf) / never (inf) | 0.0353 | 0.00293 | 69.8 % | 0 / 0 | 14548 | 0.000 | 0.10 s |

#### CL@b

Sweep for every arm in this cell: resends only the unit blocking the cumulative ack, when its deadline (about 2 smoothed RTTs, 25 to 100 ms, from its last send) expires; the receiver repeats its report every 2 smoothed RTTs while it holds holes.

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered | link busy over 1-10 s | queue drops outside / inside outage | resends refused by the budget | duplicate arrivals per copy | recovery time after the outage (s) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| `C1`: sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early) | 98.7 % | 37.5 % | 98.7 % | 621 / 1333 / never (inf) | 0.0301 | 0.0264 | 82.1 % | 0 / 0 | 989 | 0.002 | not within the run |
| `C3`: sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole) | 100.0 % | 22.6 % | 97.0 % | 895 / 1603 / 2146 | 0.0471 | 0 | 81.4 % | 0 / 0 | 153458 | 0.015 | not within the run |
| `C3S`: sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order | 100.0 % | 22.6 % | 97.0 % | 895 / 1603 / 2146 | 0.0471 | 0 | 81.4 % | 0 / 0 | 153458 | 0.015 | not within the run |
| `G-T0p4-H0`: sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0) | 84.7 % | 84.7 % | 84.7 % | 5.7 / never (inf) / never (inf) | 0.0337 | 0.0297 | 69.6 % | 0 / 0 | 0 | 0.000 | 0.10 s |
| `G-T0p4-Hinf`: sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it | 84.7 % | 84.6 % | 84.7 % | 17 / never (inf) / never (inf) | 0.0337 | 0 | 69.6 % | 0 / 0 | 0 | 0.000 | 0.10 s |
| `G-T2-H0`: sender: S5, lifetime 2 s; receiver: any order (H = 0) | 99.9 % | 30.6 % | 99.8 % | 736 / 1429 / 1631 | 0.0458 | 0.0381 | 81.4 % | 0 / 0 | 142356 | 0.021 | not within the run |
| `G-T2-Hinf`: sender: S5, lifetime 2 s; receiver: strict order | 99.9 % | 22.9 % | 99.6 % | 885 / 1587 / 1975 | 0.0458 | 0 | 81.4 % | 0 / 0 | 142356 | 0.021 | not within the run |
| `G-Tinf-H0`: sender: S5 with no lifetime (never gives up, no floor); receiver: any order | 100.0 % | 30.0 % | 99.8 % | 746 / 1440 / 1644 | 0.0471 | 0.0395 | 81.4 % | 0 / 0 | 153458 | 0.015 | not within the run |
| `G-T0p4-H0p02`: sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing | 84.7 % | 84.7 % | 84.7 % | 16 / never (inf) / never (inf) | 0.0337 | 0.00293 | 69.6 % | 0 / 0 | 0 | 0.000 | 0.10 s |

#### OV

Sweep for every arm in this cell: resends the whole open tail every period (period fixed from the base RTT: 25 to 100 ms).

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered | link busy over 1-10 s | queue drops outside / inside outage | resends refused by the budget | duplicate arrivals per copy |
|---|---|---|---|---|---|---|---|---|---|---|
| `C1`: sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early) | 99.1 % | 11.3 % | 64.4 % | 1568 / 2775 / 3036 | 0.0627 | 0.0207 | 100.0 % | 0 / 0 | 379280 | 0.571 |
| `C3`: sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole) | 100.0 % | 6.7 % | 62.7 % | 1634 / 2767 / 3030 | 0.0723 | 0 | 100.0 % | 0 / 0 | 366504 | 0.487 |
| `C3S`: sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order | 100.0 % | 6.7 % | 62.7 % | 1634 / 2767 / 3030 | 0.0723 | 0 | 100.0 % | 0 / 0 | 366504 | 0.487 |
| `G-T0p4-H0`: sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0) | 83.1 % | 83.1 % | 83.1 % | 390 / never (inf) / never (inf) | 0.0103 | 0.00349 | 100.0 % | 0 / 0 | 36244 | 0.565 |
| `G-T0p4-Hinf`: sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it | 83.1 % | 82.8 % | 83.1 % | 390 / never (inf) / never (inf) | 0.0103 | 0 | 100.0 % | 0 / 0 | 36244 | 0.565 |
| `G-T2-H0`: sender: S5, lifetime 2 s; receiver: any order (H = 0) | 94.0 % | 13.7 % | 94.0 % | 1438 / 1990 / never (inf) | 0.0519 | 0.0208 | 100.0 % | 0 / 0 | 245152 | 0.500 |
| `G-T2-Hinf`: sender: S5, lifetime 2 s; receiver: strict order | 94.0 % | 6.7 % | 93.7 % | 1634 / 1990 / never (inf) | 0.0519 | 0 | 100.0 % | 0 / 0 | 245152 | 0.500 |
| `G-Tinf-H0`: sender: S5 with no lifetime (never gives up, no floor); receiver: any order | 100.0 % | 13.7 % | 69.6 % | 1438 / 2583 / 2841 | 0.0723 | 0.0299 | 100.0 % | 0 / 0 | 366504 | 0.487 |
| `G-T0p4-H0p02`: sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing | 83.1 % | 83.1 % | 83.1 % | 390 / never (inf) / never (inf) | 0.0103 | 0.00341 | 100.0 % | 0 / 0 | 36244 | 0.565 |
| `S5v-H0`: ABLATION: S5's lifetime 0.4 s with ADR-0084's verdict floor (a floor datagram sent at a verdict, the feedback give-up rule) instead of the protected floor; receiver: any order | 80.9 % | 80.9 % | 80.9 % | 390 / never (inf) / never (inf) | 0.0105 | 0.00355 | 100.0 % | 0 / 0 | 35425 | 0.564 |
| `S5v-Hinf`: ABLATION: as S5v with H = 0, receiver: strict order | 80.9 % | 6.8 % | 80.9 % | 488 / never (inf) / never (inf) | 0.0105 | 0 | 100.0 % | 0 / 0 | 35425 | 0.564 |

#### OV@b

Sweep for every arm in this cell: resends only the unit blocking the cumulative ack, when its deadline (about 2 smoothed RTTs, 25 to 100 ms, from its last send) expires; the receiver repeats its report every 2 smoothed RTTs while it holds holes.

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered | link busy over 1-10 s | queue drops outside / inside outage | resends refused by the budget | duplicate arrivals per copy |
|---|---|---|---|---|---|---|---|---|---|---|
| `C1`: sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early) | 99.7 % | 13.2 % | 73.8 % | 1372 / 2431 / 2662 | 0.0301 | 0.0269 | 100.0 % | 0 / 0 | 0 | 0.003 |
| `C3`: sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole) | 100.0 % | 8.2 % | 74.6 % | 1403 / 2354 / 2587 | 0.0343 | 0 | 100.0 % | 0 / 0 | 0 | 0.030 |
| `C3S`: sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order | 100.0 % | 8.2 % | 74.6 % | 1403 / 2354 / 2587 | 0.0343 | 0 | 100.0 % | 0 / 0 | 0 | 0.030 |
| `G-T0p4-H0`: sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0) | 83.8 % | 83.8 % | 83.8 % | 388 / never (inf) / never (inf) | 0.00543 | 0.00405 | 100.0 % | 0 / 0 | 0 | 0.161 |
| `G-T0p4-Hinf`: sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it | 83.8 % | 83.8 % | 83.8 % | 388 / never (inf) / never (inf) | 0.00543 | 0 | 100.0 % | 0 / 0 | 0 | 0.161 |
| `G-T2-H0`: sender: S5, lifetime 2 s; receiver: any order (H = 0) | 96.6 % | 16.0 % | 96.6 % | 1214 / 1988 / never (inf) | 0.0278 | 0.0241 | 100.0 % | 0 / 0 | 0 | 0.032 |
| `G-T2-Hinf`: sender: S5, lifetime 2 s; receiver: strict order | 96.6 % | 8.2 % | 96.6 % | 1403 / 1988 / never (inf) | 0.0278 | 0 | 100.0 % | 0 / 0 | 0 | 0.032 |
| `G-Tinf-H0`: sender: S5 with no lifetime (never gives up, no floor); receiver: any order | 100.0 % | 16.0 % | 82.7 % | 1214 / 2176 / 2392 | 0.0343 | 0.0299 | 100.0 % | 0 / 0 | 0 | 0.030 |
| `G-T0p4-H0p02`: sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing | 83.8 % | 83.8 % | 83.8 % | 388 / never (inf) / never (inf) | 0.00543 | 0.004 | 100.0 % | 0 / 0 | 0 | 0.161 |
| `S5v-H0`: ABLATION: S5's lifetime 0.4 s with ADR-0084's verdict floor (a floor datagram sent at a verdict, the feedback give-up rule) instead of the protected floor; receiver: any order | 81.6 % | 81.6 % | 81.6 % | 389 / never (inf) / never (inf) | 0.00553 | 0.00409 | 100.0 % | 0 / 0 | 0 | 0.168 |
| `S5v-Hinf`: ABLATION: as S5v with H = 0, receiver: strict order | 81.6 % | 8.1 % | 81.6 % | 487 / never (inf) / never (inf) | 0.00553 | 0 | 100.0 % | 0 / 0 | 0 | 0.168 |

#### CL-long

Sweep for every arm in this cell: resends the whole open tail every period (period fixed from the base RTT: 25 to 100 ms).

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered | link busy over 1-10 s | queue drops outside / inside outage | resends refused by the budget | duplicate arrivals per copy | recovery time after the outage (s) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| `C1`: sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early) | 99.2 % | 64.2 % | 99.2 % | 16 / 1186 / 1461 | 0.0473 | 0.0258 | 82.8 % | 0 / 0 | 260555 | 0.269 | 5.68 s |
| `C3`: sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole) | 100.0 % | 55.4 % | 98.6 % | 209 / 1435 / 2058 | 0.0566 | 0 | 82.0 % | 0 / 0 | 443742 | 0.239 | 7.79 s |
| `C3S`: sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order | 100.0 % | 56.1 % | 98.5 % | 192 / 1433 / 2071 | 0.0562 | 0 | 82.0 % | 0 / 0 | 436479 | 0.233 | 7.43 s |
| `G-T0p4-H0`: sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0) | 92.4 % | 92.4 % | 92.4 % | 5.6 / 17 / never (inf) | 0.0342 | 0.0299 | 69.8 % | 0 / 0 | 14548 | 0.000 | 0.10 s |
| `G-T0p4-Hinf`: sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it | 92.4 % | 92.3 % | 92.4 % | 17 / 34 / never (inf) | 0.0342 | 0 | 69.8 % | 0 / 0 | 14548 | 0.000 | 0.10 s |
| `G-T2-H0`: sender: S5, lifetime 2 s; receiver: any order (H = 0) | 99.9 % | 60.8 % | 99.9 % | 76 / 1269 / 1484 | 0.0561 | 0.0341 | 82.0 % | 0 / 0 | 433104 | 0.245 | 6.61 s |
| `G-T2-Hinf`: sender: S5, lifetime 2 s; receiver: strict order | 99.9 % | 55.6 % | 99.8 % | 197 / 1428 / 1936 | 0.0561 | 0 | 82.0 % | 0 / 0 | 433104 | 0.245 | 7.78 s |
| `G-Tinf-H0`: sender: S5 with no lifetime (never gives up, no floor); receiver: any order | 100.0 % | 60.6 % | 99.9 % | 82 / 1276 / 1484 | 0.0566 | 0.0347 | 82.0 % | 0 / 0 | 443742 | 0.239 | 6.66 s |
| `G-T0p4-H0p02`: sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing | 92.4 % | 92.4 % | 92.4 % | 16 / 25 / never (inf) | 0.0342 | 0.00296 | 69.8 % | 0 / 0 | 14548 | 0.000 | 0.10 s |

#### CL-long@b

Sweep for every arm in this cell: resends only the unit blocking the cumulative ack, when its deadline (about 2 smoothed RTTs, 25 to 100 ms, from its last send) expires; the receiver repeats its report every 2 smoothed RTTs while it holds holes.

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered | link busy over 1-10 s | queue drops outside / inside outage | resends refused by the budget | duplicate arrivals per copy | recovery time after the outage (s) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| `C1`: sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early) | 99.2 % | 68.6 % | 99.2 % | 14 / 1159 / 1479 | 0.0301 | 0.0254 | 82.1 % | 0 / 0 | 989 | 0.002 | 4.80 s |
| `C3`: sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole) | 100.0 % | 60.9 % | 98.5 % | 28 / 1421 / 2070 | 0.0402 | 0 | 81.4 % | 0 / 0 | 153458 | 0.009 | 6.73 s |
| `C3S`: sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order | 100.0 % | 61.2 % | 98.4 % | 28 / 1419 / 2093 | 0.0403 | 0 | 81.5 % | 0 / 0 | 151567 | 0.010 | 6.60 s |
| `G-T0p4-H0`: sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0) | 92.3 % | 92.3 % | 92.3 % | 5.6 / 17 / never (inf) | 0.0335 | 0.0298 | 69.6 % | 0 / 0 | 0 | 0.000 | 0.10 s |
| `G-T0p4-Hinf`: sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it | 92.3 % | 92.3 % | 92.3 % | 17 / 34 / never (inf) | 0.0335 | 0 | 69.6 % | 0 / 0 | 0 | 0.000 | 0.10 s |
| `G-T2-H0`: sender: S5, lifetime 2 s; receiver: any order (H = 0) | 99.9 % | 65.3 % | 99.9 % | 6.0 / 1256 / 1502 | 0.0396 | 0.034 | 81.4 % | 0 / 0 | 142356 | 0.012 | 5.63 s |
| `G-T2-Hinf`: sender: S5, lifetime 2 s; receiver: strict order | 99.9 % | 61.2 % | 99.8 % | 28 / 1410 / 1937 | 0.0396 | 0 | 81.4 % | 0 / 0 | 142356 | 0.012 | 6.59 s |
| `G-Tinf-H0`: sender: S5 with no lifetime (never gives up, no floor); receiver: any order | 100.0 % | 65.0 % | 99.9 % | 6.0 / 1267 / 1502 | 0.0402 | 0.0347 | 81.4 % | 0 / 0 | 153458 | 0.009 | 5.67 s |
| `G-T0p4-H0p02`: sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing | 92.3 % | 92.3 % | 92.3 % | 16 / 25 / never (inf) | 0.0335 | 0.00296 | 69.6 % | 0 / 0 | 0 | 0.000 | 0.10 s |

#### OV-long

Sweep for every arm in this cell: resends the whole open tail every period (period fixed from the base RTT: 25 to 100 ms).

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered | link busy over 1-10 s | queue drops outside / inside outage | resends refused by the budget | duplicate arrivals per copy |
|---|---|---|---|---|---|---|---|---|---|---|
| `C1`: sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early) | 99.0 % | 2.8 % | 16.1 % | 6090 / 10916 / 11992 | 0.0624 | 0.0204 | 100.0 % | 0 / 0 | 1538877 | 0.574 |
| `C3`: sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole) | 100.0 % | 1.7 % | 15.7 % | 5932 / 10508 / 11541 | 0.0721 | 0 | 100.0 % | 0 / 0 | 1487912 | 0.484 |
| `C3S`: sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order | 100.0 % | 1.8 % | 15.7 % | 5934 / 10484 / 11490 | 0.0714 | 0 | 100.0 % | 0 / 0 | 1486449 | 0.479 |
| `G-T0p4-H0`: sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0) | 81.2 % | 81.2 % | 81.2 % | 390 / never (inf) / never (inf) | 0.00265 | 0.000893 | 100.0 % | 0 / 0 | 36244 | 0.565 |
| `G-T0p4-Hinf`: sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it | 81.2 % | 81.1 % | 81.2 % | 390 / never (inf) / never (inf) | 0.00265 | 0 | 100.0 % | 0 / 0 | 36244 | 0.565 |
| `G-T2-H0`: sender: S5, lifetime 2 s; receiver: any order (H = 0) | 83.9 % | 3.4 % | 83.9 % | 1990 / never (inf) / never (inf) | 0.0145 | 0.00583 | 100.0 % | 0 / 0 | 245152 | 0.500 |
| `G-T2-Hinf`: sender: S5, lifetime 2 s; receiver: strict order | 83.9 % | 1.7 % | 83.8 % | 1990 / never (inf) / never (inf) | 0.0145 | 0 | 100.0 % | 0 / 0 | 245152 | 0.500 |
| `G-T0p4-H0p02`: sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing | 81.2 % | 81.2 % | 81.2 % | 390 / never (inf) / never (inf) | 0.00265 | 0.000873 | 100.0 % | 0 / 0 | 36244 | 0.565 |
| `S5v-H0`: ABLATION: S5's lifetime 0.4 s with ADR-0084's verdict floor (a floor datagram sent at a verdict, the feedback give-up rule) instead of the protected floor; receiver: any order | 78.8 % | 78.8 % | 78.8 % | 390 / never (inf) / never (inf) | 0.00269 | 0.000911 | 100.0 % | 0 / 0 | 35425 | 0.564 |
| `S5v-Hinf`: ABLATION: as S5v with H = 0, receiver: strict order | 78.8 % | 1.7 % | 78.8 % | 489 / never (inf) / never (inf) | 0.00269 | 0 | 100.0 % | 0 / 0 | 35425 | 0.564 |

#### OV-long@b

Sweep for every arm in this cell: resends only the unit blocking the cumulative ack, when its deadline (about 2 smoothed RTTs, 25 to 100 ms, from its last send) expires; the receiver repeats its report every 2 smoothed RTTs while it holds holes.

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered | link busy over 1-10 s | queue drops outside / inside outage | resends refused by the budget | duplicate arrivals per copy |
|---|---|---|---|---|---|---|---|---|---|---|
| `C1`: sender: ADR-0084's (gives up on a hole when the 1 % allowance permits, sends a verdict copy and a floor); receiver: today's early-skip receiver (passes holes early) | 99.7 % | 3.3 % | 18.4 % | 5339 / 9572 / 10524 | 0.0301 | 0.0271 | 100.0 % | 0 / 0 | 0 | 0.001 |
| `C3`: sender: never gives up (rho = 1) and resends by the cooldown gate; receiver: strict order (only a floor passes a hole) | 100.0 % | 2.1 % | 18.7 % | 5014 / 8851 / 9719 | 0.0339 | 0 | 100.0 % | 0 / 0 | 0 | 0.015 |
| `C3S`: sender: S5 at lifetime T = infinity (never gives up, never sends a floor; bit-identical to C3); receiver: strict order | 100.0 % | 2.2 % | 18.7 % | 5030 / 8858 / 9704 | 0.0339 | 0 | 100.0 % | 0 / 0 | 0 | 0.014 |
| `G-T0p4-H0`: sender: S5, gives up on a unit that cannot arrive by 0.4 s and announces a floor (carried on its data); receiver: releases at once, any order (H = 0) | 81.5 % | 81.5 % | 81.5 % | 388 / never (inf) / never (inf) | 0.00139 | 0.00104 | 100.0 % | 0 / 0 | 0 | 0.161 |
| `G-T0p4-Hinf`: sender: S5 as above, 0.4 s; receiver: strict order (H = inf), passes a hole only when the floor says the sender will not resend it | 81.5 % | 81.5 % | 81.5 % | 388 / never (inf) / never (inf) | 0.00139 | 0 | 100.0 % | 0 / 0 | 0 | 0.161 |
| `G-T2-H0`: sender: S5, lifetime 2 s; receiver: any order (H = 0) | 84.7 % | 4.0 % | 84.7 % | 1988 / never (inf) / never (inf) | 0.00792 | 0.00688 | 100.0 % | 0 / 0 | 0 | 0.032 |
| `G-T2-Hinf`: sender: S5, lifetime 2 s; receiver: strict order | 84.7 % | 2.1 % | 84.7 % | 1988 / never (inf) / never (inf) | 0.00792 | 0 | 100.0 % | 0 / 0 | 0 | 0.032 |
| `G-T0p4-H0p02`: sender: S5, lifetime 0.4 s; receiver: holds a hole 20 ms before releasing | 81.5 % | 81.5 % | 81.5 % | 388 / never (inf) / never (inf) | 0.00139 | 0.00103 | 100.0 % | 0 / 0 | 0 | 0.161 |
| `S5v-H0`: ABLATION: S5's lifetime 0.4 s with ADR-0084's verdict floor (a floor datagram sent at a verdict, the feedback give-up rule) instead of the protected floor; receiver: any order | 79.2 % | 79.2 % | 79.2 % | 389 / never (inf) / never (inf) | 0.00142 | 0.00105 | 100.0 % | 0 / 0 | 0 | 0.168 |
| `S5v-Hinf`: ABLATION: as S5v with H = 0, receiver: strict order | 79.2 % | 2.0 % | 79.2 % | 488 / never (inf) / never (inf) | 0.00142 | 0 | 100.0 % | 0 / 0 | 0 | 0.168 |

#### P2

Two paths (fast 10 ms, slow 60 ms, half the first sends on each, every copy and the reports on the fast path, 3 % loss on each), 10 000 units per second, 12 147 units, no outage, no link limit. Every arm resends only a report-listed hole whose last send is older than the measured skew before the newest received send (RACK's time test).

| arm (sender; receiver) | delivered at all | within T | within 0.4 s | delay p50 / p90 / p99 from hand-over (ms) | out of order per delivered | late fills of floor-passed holes per unit | floor passes per unit | late fills among units handed over after 1 s | H_f measured (ms) | duplicates per copy |
|---|---|---|---|---|---|---|---|---|---|---|
| `C3`: sender: never gives up (rho = 1); receiver: strict order | 100.0 % | 100.0 % | 100.0 % | 139 / 200 / 261 | 0 | 0.0000 | 0.0000 | 0.00 | 50.8 | 0.297 |
| `C3S`: sender: S5 at T = infinity (never gives up); receiver: strict order (bit-identical to C3) | 100.0 % | 100.0 % | 100.0 % | 139 / 200 / 261 | 0 | 0.0000 | 0.0000 | 0.00 | 50.8 | 0.297 |
| `G-T0p075-H0-Fest`: sender: S5 gives up at 0.075 s; receiver: any order, receiver estimates the floor hold from measured reordering (H_f, window 10 s) | 97.0 % | 97.0 % | 97.0 % | 60 / 61 / never (inf) | 0.504 | 0.0102 | 0.0404 | 0.00 | 50.8 | nan |
| `G-T0p075-H0-F0`: sender: S5 gives up at 0.075 s; receiver: any order, receiver passes at the floor at once (H_f = 0) | 97.0 % | 97.0 % | 97.0 % | 60 / 61 / never (inf) | 0.504 | 0.0651 | 0.0953 | 92.67 | 50.8 | nan |
| `G-T0p075-Hinf-Fest`: sender: S5 gives up at 0.075 s; receiver: strict order, receiver estimates the floor hold from measured reordering (H_f, window 10 s) | 97.0 % | 2.0 % | 97.0 % | 121 / 126 / never (inf) | 0.0105 | 0.0102 | 0.0404 | 0.00 | 50.8 | nan |
| `G-T0p075-Hinf-F0`: sender: S5 gives up at 0.075 s; receiver: strict order, receiver passes at the floor at once (H_f = 0) | 97.0 % | 91.5 % | 97.0 % | 71 / 75 / never (inf) | 0.0671 | 0.0651 | 0.0953 | 92.67 | 50.8 | nan |
| `G-T0p4-H0-Fest`: sender: S5 gives up at 0.4 s; receiver: any order, receiver estimates the floor hold from measured reordering (H_f, window 10 s) | 99.9 % | 99.9 % | 99.9 % | 60 / 61 / 83 | 0.519 | 0.0087 | 0.0093 | 0.00 | 50.8 | 0.298 |
| `G-T0p4-H0-F0`: sender: S5 gives up at 0.4 s; receiver: any order, receiver passes at the floor at once (H_f = 0) | 99.9 % | 99.9 % | 99.9 % | 60 / 61 / 83 | 0.519 | 0.0087 | 0.0093 | 0.00 | 50.8 | 0.298 |
| `G-T0p4-Hinf-Fest`: sender: S5 gives up at 0.4 s; receiver: strict order, receiver estimates the floor hold from measured reordering (H_f, window 10 s) | 99.9 % | 99.9 % | 99.9 % | 138 / 200 / 262 | 0.00867 | 0.0087 | 0.0093 | 0.00 | 50.8 | 0.298 |
| `G-T0p4-Hinf-F0`: sender: S5 gives up at 0.4 s; receiver: strict order, receiver passes at the floor at once (H_f = 0) | 99.9 % | 99.9 % | 99.9 % | 138 / 200 / 262 | 0.00867 | 0.0087 | 0.0093 | 0.00 | 50.8 | 0.298 |
| `G-Tinf-H0-Fest`: sender: S5 never gives up; receiver: any order, receiver estimates the floor hold from measured reordering (H_f, window 10 s) | 100.0 % | 100.0 % | 100.0 % | 60 / 61 / 83 | 0.519 | 0.0000 | 0.0000 | 0.00 | 50.8 | 0.297 |
| `G-Tinf-H0-F0`: sender: S5 never gives up; receiver: any order, receiver passes at the floor at once (H_f = 0) | 100.0 % | 100.0 % | 100.0 % | 60 / 61 / 83 | 0.519 | 0.0000 | 0.0000 | 0.00 | 50.8 | 0.297 |
| `G-Tinf-Hinf-Fest`: sender: S5 never gives up; receiver: strict order, receiver estimates the floor hold from measured reordering (H_f, window 10 s) | 100.0 % | 100.0 % | 100.0 % | 139 / 200 / 261 | 0 | 0.0000 | 0.0000 | 0.00 | 50.8 | 0.297 |
| `G-Tinf-Hinf-F0`: sender: S5 never gives up; receiver: strict order, receiver passes at the floor at once (H_f = 0) | 100.0 % | 100.0 % | 100.0 % | 139 / 200 / 261 | 0 | 0.0000 | 0.0000 | 0.00 | 50.8 | 0.297 |



## A/A self-check

Frozen before any decision cell was read (`nf_stage3d.json`): A = S5 at T = infinity, H = infinity with clock-phase variant 0 (`C3S`; `C3` in the four long cells, where `C3S` has 3 seeds and is bit-equal to `C3`), A' = the same with variant 1, per cell and readout (the 3c list plus `recovery_time`), 12 seeds, read against the cell's T bar.

560 readings over 21 cells: 439 equivalent, 117 undetermined, 4 shown
  shown: K-L10@b-stall_total -32.77 [-66.6, -1.734] vs m=1: shown smaller
  shown: K-L10b@b-stall_total -91.35 [-118.5, -61.59] vs m=1: shown smaller
  shown: X15-stall_total 59.28 [13.28, 108.9] vs m=1: shown larger
  shown: X20-stall_total -53.85 [-104.5, -6.299] vs m=1: shown smaller

undetermined (cell-key: mean [lo, hi] vs m):

The 117 undetermined readings and all 21 cells' floors are in `nf_stage3d.json` (the 4 shown ones are the stall totals above: phase-variant noise in the strict-order stall, not a decision cell).

## Worker decisions (code-level ambiguities, the reading closest to the text)

1. **References.** Regenerated from `git archive 02eda90b` into a temporary directory, not a git worktree (the archive is a snapshot of the same tree): 1 984 runs, 3 seeds, every key with traces and per-seq arrays as digests: the 34 stage 3 configurations (102), every stage 3b configuration (963) and every stage 3c configuration (919; the E-I2n control runs only for the seeds that placed). `check_ident3d.py` also checks that `batch3.py`, `batch3b.py`, `batch3c.py` and the placement files equal their `02eda90b` versions.
2. **S5 and S5v.** Sender name `S5`, in the S2 family with S4's fields plus `trigger_law = "trigger"`, `owd_est = "fwd"`, `recheck_floor`, `verdict_evidence = "none"`. S5v = `verdict_evidence = "feedback_tail"` and `floor_law = "verdict"`, everything else S5's. The `none` evidence adds no verdict timer and counts the skipped L2 failures in `l2_skip`.
3. **The floor law and the re-check.** `protected()` is the one law function; with the forward term it receives twice the one-way delay as its "srtt" argument so the law reads srtt_eff/2 = ô exactly. The re-check recomputes the floor (`update_floor`) first and then tests `s >= F_ann`; it runs before the budget test.
4. **P_reach.** One method, `sweep_reach(s)`, used by the floor law and by the sweep. Whole-tail: the period for every seq. Blocker: the blocker period for the cumulative blocker (`A_known`), infinity for every other seq. The blocker period is clamp(2 srtt_pooled, 25, 100 ms) of the measured srtt (333 ms seed before the first sample, so 100 ms).
5. **Blocker sweep.** Scheduled as deadline events (kind 10) re-armed whenever the blocker, its last send or a report may have changed; a stale event is ignored by its payload. The periodic whole-tail sweep is not scheduled in that form, and the object-mode re-serve of reported holes is not used (only the blocker goes to the serve path). The fire also recomputes the floor and repeats it while `F_ann > A_known`.
6. **Receiver refresh.** At 2 ms ticks, while the receiver holds holes at the ack level, a report is re-sent when the last report is older than clamp(2 srtt, 25, 100 ms), srtt being the sender's `srtt_eff()` (idealised, as registered).
7. **RTT estimator in the engine-like cells.** `rtt_est = "rfc"` for every arm in the `@b` cells (the K cells' comparison arms otherwise use stage 3's estimator, which would give the blocker sweep a different deadline and break the identity). In the tail-form cells every arm keeps its stage 3c setting.
8. **P2.** Link-cell kwargs without the link (work-conserving source, RFC 6298 RTT, the engine's cooldown, the 10-per-5-ms resend budget, `pause_B` 1000, J = 1 ms on both paths) for every arm; a witness shows them inert (pause 0, budget refusals <= 8). Per-path RFC 6298 estimators sampled from each path's newest never-resent seq; pooled srtt = maximum over paths, pooled rttvar = maximum; `skew_hat = srtt_B - srtt_A` unclamped; the reorder rule filters report-driven holes only. The floor-hold estimator is non-strict across paths.
9. **Recovery time.** L0 over units handed over in [0.5 s, outage start - 0.1 s]; sd0 = population standard deviation of the windows lying wholly inside that interval; the readable end is the last hand-over minus 2 s and a window must end by it; the first window start at or after the outage end. **M cells: not computed**: the outage starts at 0.4859 s, so [0.5 s, 0.3859 s] is empty (impossible cell, reported as such). The CL-long outage is the CL outage (2.0 to 3.5 s) with 200 000 units, `t_limit` 80.
10. **OV-long.** 480 000 units at 12 000/s (40 s), `t_limit` 120 s (the registration gives none; the never-give-up arms need about 48 s of link time). The windowed within-2-s readout uses, for every arm, the first dequeue expiry of the same seed's S5 T = 2, H = 0 run as "the first dequeue expiry" (the arms without a lifetime have none), windows starting after it and ending by 38 s.
11. **Arms.** Stage 3c's E-I2n is not in the 3d cell list and is not run. X15 and X20 run S5 (T = 0.4 and the crossover grid, H = 0) in place of S4. The unplanted nudges are run in the long cells too (added after the identity and A/A runs began, before any nudge run); the planted controls only in OV and K-L10 (tail sweep, H = 0). Identity arms in the four long cells: 3 seeds (the cost of a long run), A/A at 12 seeds. S5v runs in OV and OV-long under both sweep forms.
12. **Readings.** Q3d-3: bullets 1, 3, 4 are read on the tail form and also reported on the blocker form; bullet 2 on both as registered. "Verdicts > 1 000" on the seed mean (the minimum is reported). Q3d-4 bullet 3 read over every S5 row (stricter reading) with the T = 0.4 rows reported separately. Q3d-5 equivalence bar max(5 % of C3, 0.1 s, frozen A/A floor), a worker decision (the registration gives none). Q3d-2's skip bar on the sum over every run of a cell, the worst run as a supplement.
13. **Per-path delay quantiles** (`dhA_*`, `dhB_*`) were added to the output for Q3d-8's "fast-path units" reading.
14. **Reference arm for Q3d-9.** Stage 3c's S4 is re-run with the 3c code path (`batch3c.s4_cfg`) in the same 3d output, 12 seeds; its bit-identity to the 3c code is `check_ident3d.py`'s subject.

## Deviations (timing)

- **Order of work.** Identity and liveness on the ID arms (636 runs) came before the A/A; I then added the per-path delay quantiles to the output and the unplanted nudges in the long cells (both after the ID and A/A runs began, before any P2 or nudge run), and fixed the reading code (Q3d-2 over every one-path cell including X, the recovery-time bar with the A/A floor; the `Te` name added to the deny scan; test seeds raised from 2 to 3) before the A/A freeze and the main batch. The ID and A/A runs were made with the code before the per-path quantiles (an output-only addition); `check_ident3d.py` and the full unit suite were rerun on the final code.
- **Reading-code bug after seeing data.** `report_s3d.py` compared P2's delay quantiles (stored in ms) with bands in seconds in its first version; found on reading Q3d-8 (the first P2 reading), fixed by converting, no prediction, bar, cell or arm changed.
- **Reading coverage.** The unplanted-nudge reading (`report_s3d.py q7`) first covered 14 of the 18 cells (the long cells were omitted by mistake); after seeing it I added them. The readings of the original 14 cells did not change (14 of the 24 undetermined). The long-cell nudge runs themselves were registered and run before any reading.
- **No registered prediction, bar, cell, placement or noise-floor file was changed after any data was seen.** The A/A floor was frozen once.
- **Operational.** The first main batch (15 workers) died with a `MemoryError` in a long-cell C1 run; no stored file was affected (files are written atomically); the batch was resumed with fewer workers in two groups. The first final identity run (`check_ident3d.py`) also hit a `MemoryError` while the long runs were in progress and was rerun on a quiet machine.
- **Unregistered supplement `SUP-S5v`** (the S5v ablation with the timeout also for a hole no later send has overtaken, `verdict_evidence = "feedback_tail_any"`), the fix attempt for Q3d-1's liveness failure; it does not replace the registered arm in any reading above.

## Things that looked wrong

1. **The ablation arm cannot finish in 8 of 48 long runs.** See Q3d-1. The registered open-tail timeout covers `s > rhi_known` only; a lost unit sent in the same instant as the last datagrams of the run is neither "open tail" nor judged by feedback (no later send). Probably also reachable in the engine's verdict path.
2. **The floor re-check never fired** (`skip_below_floor` = 0 in all runs): either the law is now exact for these cells or the re-check is untested outside the constructed state; the registered control (3c's law fields and no re-check reproduce 7 violations in OV seed 5, T = 0.4) proves the cell can produce the violation.
3. **P2: the reorder rule suppresses every report-driven resend at the 0.075 s lifetime.** A hole is a candidate only once a datagram sent `skew_hat` (50 ms) later has been reported, which is at about 63.5 ms of age on the fast path; by then the floor has passed it (candidates skipped below the floor: 2 109 per run, against 66 776 without the rule, which resends 184 units), so no copy is sent. All 3 % lost units are therefore given up: 0 copies in the six 0.075 s arms, delivered 97.0 % (98.5 % without the rule). And the rule has no margin: C3 and the 0.4 s arms resend 30 % of their copies for data that then arrives (duplicates 0.297 per copy) because the report leaves 1 ms before the slow-path datagram lands.
4. **P2: the floor trails 65 ms behind lost fast-path holes**, which the registered prediction did not model: a lost fast-path unit stays protected until age T - ô = 65 ms, and 3 % loss at 10 000/s puts a new such hole every 6.6 ms, so the floor sits at that age and passes the slow-path units still in flight (age under 60 ms) only rarely. This is why the late fills are 6.5 % and not 35 to 50 %, and why the estimated hold, added on top, doubles the strict-order delay.
5. **P2 cold start.** Before the first RTT sample of each path the sender uses the 333 ms seed; with it every slow-path unit is unprotected at birth (trigger 100 ms + ô 323 ms), so the floor passes units in flight. At T = 0.4 all 97 to 105 late fills per run are among the units handed over in the first 0.1 s.
6. **The 0.10 s recovery time of the 0.4 s arms** is the window series' resolution: the 0.1 s step with 0.5 s windows cannot read less. The registered upper bands (0.6 s and 1.0 s) are met by a wide margin; the lower end of the band is not informative.
7. **CL-long C3 recovers 0.2 to 0.3 s slower than the 3c-drain-based bands.** The registered 6.4 s and 5.1 s were derived from 3c's 11 750 first sends per second and a 1 750/s surplus; the measured recoveries are 7.8 s and 6.7 s, consistent with a surplus about 15 % lower.
8. **A/A readings.** 4 of 560 A/A readings are "shown" (stall total in K-L10@b, K-L10b@b, X15, X20), 117 undetermined: phase-variant noise as in stage 3c; the frozen floors cover them.
9. **Stage 3c's Q3c-10 bullet 2 miss at K-L10 persists unchanged** (the same five non-zero (o) values), as registered ("as in Q3c-10").

## Data

`out3d/` (not tracked): 5 628 runs. The run log and reading outputs were produced by `batch3d.py` and `summarize3d.py q1 ... q9 aa`; `tables3d.py` prints the per-cell tables above.
