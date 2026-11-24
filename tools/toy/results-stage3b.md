# Offline simulation, stage 3b: the revised design on a capacity-limited link (RFC 6298 timeout, evidence-gated verdicts, dequeue expiry, R probe)

Spec: `docs/derivations/three-knobs.md` Â§13 "Revision 1", registration Â§13.7 (branch `github/three-knobs`, head `b502ca8e`). Code: `tools/toy/` (`toy2.py` extended; `batch3b.py`, `summarize3b.py`, `report_s3b.py`, `tables3b.py`, `pc_place.py`, `check_ident3b.py`, tests in `test_toy2.py`); built on `toy-stage3-knobs` `39efa6a3`. Stage 1, 2 and 3 code paths and stored outputs are bit-identical (below). The offline simulation fixes no constant of the engine, and none of these numbers is engine provenance. 12 seeds per configuration (seeds 0 to 11, clock phases drawn per seed), one path, all runs deterministic. Data lives in `out3b/` (not tracked, like `out3/`); the frozen A/A floor `nf_stage3b.json` and the placement `pc_stage3b.json` are tracked.

## In plain words

- **Everything that was registered as an exact identity holds.** With the new fields at their defaults the code reproduces stage 3 on 34 configurations (every old key, traces and per-sequence arrays), stage 2 on 29, and 8 stored stage 3 runs. The revised sender with the old tail sweep at T = infinity (and at T = 10^6 s) equals today's receiver at rho = 1 on 20 readouts in all 12 cells, 12 seeds each (432 of 432), and on the sender trace (seeds 0 to 2, all cells). The suites pass.
- **D1 is fixed.** On the revised sender no delivered datagram is ever judged lost (1 326 runs with more than 100 verdicts, 0 with delivered-verdicts above 0.1 %, worst 0), and on the capacity-limited overload cell the stage 3 sender still shows the defect (113 000 verdicts, 81 % of them on data that arrived, against 2 500 and none for the revised sender).
- **The crossover now follows the path.** With the R timer the point where a resend can arrive in time sits at 52 ms (15 ms path) and 67 ms (20 ms path), a shift of 15.1 ms (predicted 15 +- 4), and at 159 ms (50 ms path) and 190 ms (60 ms path), a shift of 30.2 ms (predicted 30 +- 5); each lies within 3 ms of R + (smoothed round trip)/2 measured in the same runs. The old sweep also moves by 29.9 ms at the mid-level reading, which the registration expected to stay under 10 ms (it was registered against stage 3's 90 % readings, which did read 0 to 16 ms).
- **What the lifetime buys, at last, is shown.** When the link is the scarce resource the revised sender with T = 0.4 s delivers 84.8 % of the units within 0.4 s of hand-over in the outage-and-drain cell (CL) against 19.9 % for today's sender (C1), 19.9 % for never giving up (C3) and 23.9 % for never giving up with the R timer (C3R); under sustained 20 % overload (OV) it delivers 81.5 % within 0.4 s against 6.0 %, 2.5 % and 8.6 %. The price is the units given up on purpose (15.2 % in CL, the 1.5 s outage; 18.5 % in OV, the excess), and under overload a pinned delay of 390 ms (median).
- **But the registered picture of the comparison arms was wrong, and one design property fails.** In both link cells today's and the stage 3 resend rules flood the queue (C3: 684 000 queue drops outside the outage in CL, 200 000 in OV; the predicted value was 0) and collapse, so C3 and C1 read far below the registered bands (CL: 19.9 % within 0.4 s against 30 to 47 %; OV: 2.5 % and 6.0 % against 10 to 22 %), and T = 2 s is not equivalent to never giving up (it delivers +79 points more within 2 s). More important for the design: **the R probe timer recovers an outage tail serially, one sequence number per timeout.** After an end outage the 347 lost tail units take about 347 x R to recover; at 50 ms path delay (R about 105 ms) the arm with T = infinity does not finish within 40 s in 12 of 12 seeds (99.79 % delivered), and at 5 ms path delay the tail needs 4.3 s (median age at delivery) against 1.5 s for the old sweep. Â§13.4 says recovery is not serial; for T = infinity it is, because the probe's evidence is a gate and nothing sends the other tail units. A finite T hides it (they get verdicts). This also makes the registered positive control (K on `share_T` at the fill age of the narrow tail) impossible: the placement procedure measures an inter-quartile range of 22 to 92 % of the median in all three end-outage cells, so every cell is a ramp (Q3b-7).

## Readings (Q3b-1 to Q3b-9)

| | reading | key numbers |
|---|---|---|
| Q3b-1 | **PASS** | S3 (old sweep) at T = inf and at T = 1e6 equal C3, and S3 (R) at T = 1e6 equals C3R, on the 20 keys: 432 of 432 (12 cells x 3 comparisons x 12 seeds); sender traces identical in all 12 cells x 3 seeds x 2 identities; `check_ident3b.py` PASS (34 + 29 + 8) |
| Q3b-2 | **PARTIAL: bullets 1 and 3 PASS, the pause-time bullet FAILS** | (1) vta <= 0.001 x verdicts in 1 326 of 1 326 S3 runs with verdicts > 100 (10 cells), worst 0. (2) OV throttle = "link" for every arm (12 of 12 each). pause_time: S3 T = 0.4 is 9.91 s against C3's 17.07 s (ratio 0.58, registered within 5 %); S3 at T = inf is 11.92 s (0.70). Over the window [1 s, 10 s] every arm is paused 9.000 s (the source is paused for the whole window), so the whole-run total differs only by how long each arm takes to finish. (3) S2 in OV: 113 200 verdicts against S3's 2 509 (45 x, bar 10 x), vta = 91 700 = 81 % of its own verdicts (bar 50 %) |
| Q3b-3 | **FAIL on the comparison arms and T = 2; PASS on the S3 T = 0.4 arm** | S3 T = 0.4 (H = 0): share_by_0p4 = 0.8147 (band 0.77 to 0.83; range over seeds 0.8139 to 0.8153), delay p50 = 389.6 ms (band 250 to 400). C3 share_by_0p4 = 0.0245 and C1 = 0.0596, both below the band 0.10 to 0.22; share_any(C3) = 1.000 PASS (C1 0.958). S3 T = 2: share_by_2 = 0.9607 (H = 0) and 0.7512 (H = inf), outside 0.77 to 0.83 either way (the 10 s run ends before the backlog reaches age T - R = 1.9 s; the expiry only works in the last 1.5 s). throttle = "link" for all arms |
| Q3b-4 | **FAIL: 5 of 7 bullets** | C3 share_by_0p4 = 0.1989 (band 0.30 to 0.47) and share_by_2 = 0.2088 (>= 0.97): FAIL. S3 T = 0.4 share_by_0p4 = 0.8476 (H = 0) and 0.8465 (H = inf), band 0.75 to 0.90: PASS. S3 minus C3 on share_by_0p4 = +0.648 [0.647, 0.648], shown larger by more than 0.25: PASS. S3 T = 2 against C3: share_by_0p4 +0.040 [0.036, 0.045] vs m = 0.040, undetermined (H = inf; +0.119 shown at H = 0), share_by_2 +0.79 shown larger: not equivalent, FAIL. queue_drop_out = 0 for S3 T = 0.4 only; C1 3.2e5, C3 6.8e5, C3R 2 235, S3 T = 2 and T = inf 2 235, S3x 1 557, S2 1.9e5: FAIL. queue_drop_in (reported): C1 29 600, C3 38 100, S2 38 300, S3 0 |
| Q3b-5 | **PASS on the revised arm; the old-sweep bullet FAILS** | mid-level crossings: X15 0.0519 s (band 0.047 to 0.055), X20 0.0670 (0.062 to 0.070), shift 15.1 ms (15 +- 4); K-L10 0.1593 (0.150 to 0.165), K-L10b 0.1895 (0.180 to 0.195), shift 30.2 ms (30 +- 5); within 4 ms of R_mean + srtt_mean/2: 0.0528, 0.0667, 0.1570, 0.1869 (differences -0.9, +0.3, +2.3, +2.6 ms); at each grid's lowest T resend_by_R = resend_by_report = 0 in all 12 seeds of all four cells. 90 % readings (secondary): 0.0500 and 0.0650 (shift 15 ms), 0.1553 and 0.1852 (shift 29.9 ms). S2 (old sweep) at K-L10 and K-L10b: mid crossings 0.1726 and 0.2025 s, shift 29.9 ms (registered: below 10 ms): FAIL; 90 % readings 0.1531 and 0.1700 (16.9 ms), also not below 10 ms |
| Q3b-6 | **PASS on "nothing shown"; 4 of 148 statistics undetermined, not equivalent as registered** | K and P over the T nudges and the H nudges, 7 cells (E-I2, E-L10, M-I2, M-L10, K-I2, K-L10, CL), 6 readouts, bar max(T, A/A floor): 0 shown, 4 undetermined, 144 equivalent. The four are within a bar of equivalent: K-I2 share_by_2 K (-2.8e-5 [-5.0e-4, 4.7e-4] vs 3.3e-4) and P, K-L10 share_by_2 K (-3.9e-4 [-6.9e-4, -1.4e-4]), K-L10 ooo_per_delivered K (-3.6e-4 [-6.4e-4, -1.3e-4]) |
| Q3b-7 | **FINDING: the K control has no cell; the ramp predictions hold in 2 of 3 cells** | placement (from C3R, 12 runs pooled): E-I2n p = 1.904 s, w = 0.410 s (21.5 % of p, not below 1 %); E-I2 p = 4.328, w = 2.818 (65 %); E-L10 p = 20.249, w = 18.702 (92 %), with 301 dead-tail units never delivered. All three are the ramp case, so there is no cell where K is predicted shown. Ramp case: J(0.02) on share_T is shown larger in all three (3.7e-4, 8.9e-4, 5.6e-4, against bars 8.2e-5, 8.2e-5, 1.7e-4); K equivalent in E-I2n and E-L10, **undetermined in E-I2** (-1.4e-5 [-1.1e-4, 7.5e-5] vs 8.2e-5): FAIL there. The stage 3 fill ages (1.53 s and 1.80 s) came from the old sweep; the R timer's serial recovery moves and widens them |
| Q3b-8 | **PASS** | share_by_0p4 S3x minus S3 equivalent in M-I2 (0), M-L10 (0), CL (-0.012 [-0.018, -0.007] vs m = 0.040) and OV (-0.0002); S3's resume delay in CL is 23 ms on average (0.016 to 0.036 s), bound R + one sweep + 2 RTT = 62.6 ms; S3x resumes after 0.05 ms (earlier); in OV pause_time 9.912 vs 9.910 s (within 5 %), throttle link for both. In M-I2 and M-L10 the resume delay is undefined: S3 expires every remaining unit in the intake, so nothing is sent after the outage. Cost of S3x's exclusion: pause time in CL is 1.14 s longer (2.56 s against 1.42 s) |
| Q3b-9 | **bullet 1 PASS, bullet 2 FAIL** | the parts exist in 6 (K-I2) and 10 (K-L10) of 12 seeds (an episode open at the first floor after the outage). (i): at most 0.018 s at K-I2 (bound 0.035) and 0.147 s at K-L10 (bound 0.155): PASS. (ii) >= 0.5 x (longest stall - 1.5 s) at K-L10: holds in 6 of 10 seeds; fails in 4 (stalls 1.523, 1.559, 1.583 s with (ii) = 0, and 1.920 s with (ii) = 0.142 s, half-excess 0.21 s). The excess in those seeds is not live-hole fills: the episode began at about 20.05 s (a hole opened by a loss in the last one-way delay before the outage, detected when the last pre-outage data arrived) and the strict receiver held it through the outage; the parts after the outage end are all that parts (i) to (iii) cover |

## Per-cell tables in plain words

Seed means over 12 seeds. "Delivered at all", "within 0.4 s" and "within 2 s" are fractions of the units handed over (a unit never delivered, or given up, counts as not delivered). Delay is measured from hand-over at the sender (so a unit that waits unsent in the intake or while the source is paused counts that wait); a percentile is "never" when it falls on an undelivered unit (in n of 12 seeds when only some seeds). No mean over delivered units alone is a headline. Arm labels:

- `C1`: today's engine. Sender gives up on a hole when the 1 % allowance (1 - rho at rho = 0.99) permits; receiver passes holes early (the early-skip receiver). Link cells: the same source and RTT estimator as every arm.
- `C3`: sender never gives up (rho = 1, old tail sweep at clamp(2 srtt, 25, 100 ms)); receiver releases only in order (a floor passes a hole).
- `C3R`: sender never gives up, but resends the open tail by the R probe timer, with RFC 6298 RTT (this is S3 at T = inf, H = inf).
- `G-T0p4-H0`, `G-T0p4-Hinf`, `G-T2-H0`, `G-T2-Hinf`, `G-Tinf-H0`: the revised sender S3 (RFC 6298 R timeout; verdicts and resends only on evidence of loss; dequeue expiry; R probe; brake released by receiver feedback only) with lifetime T = 0.4 s, 2 s or never, and receiver order hold H = 0 (release at once, any order) or strict order (H = inf).
- `G-T0p4-H0p02`: S3 at T = 0.4 s with a 20 ms hold.
- `S3x`: S3 at T = 0.4 s, strict order, with stage 3's brake (verdicted sequence numbers leave the outstanding count).
- `S2-T0p4-H0`: stage 3's sender (verdict timer without the evidence rule, exact one-way delay, old sweep, no dequeue expiry) at T = 0.4 s, receiver any order.

Cells: E = outage at the end of a 12 147-unit object; M = outage in the middle; K = a call (50 units per second, 60 s, 1.5 s outage at 20 s); I2 = 3 % loss, 5 ms path; L10 = 10 % loss, 50 ms path; CL = capacity-limited link (12 500 units/s, queue 1 250, offered 10 000/s for 10 s, 2 s to 3.5 s outage); OV = overload (link 10 000/s, offered 12 000/s for 10 s, no outage).

#### E-I2

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip) | 99.0 % | 96.9 % | 99.0 % | 12 / 16 / 1522 | 1.31 | 0.0208 |
| `C3`: never gives up (rho = 1), old tail sweep; receiver strict order | 100.0 % | 97.2 % | 100.0 % | 16 / 28 / 1532 | 1.72 | 0 |
| `C3R`: never gives up, R-timer probe for the open tail, RFC 6298 RTT; receiver strict order (S3 at T = inf, H = inf) | 100.0 % | 97.2 % | 97.4 % | 16 / 28 / 5165 | 0.0715 | 0 |
| `G-T0p4-H0`: sender gives up on a unit 0.4 s after hand-over; receiver releases at once, any order (H = 0) | 97.2 % | 97.2 % | 97.2 % | 5.5 / 5.9 / never (inf) | 0.0348 | 0.0294 |
| `G-T0p4-Hinf`: sender gives up at 0.4 s; receiver strict order (H = inf) | 97.2 % | 97.2 % | 97.2 % | 16 / 28 / never (inf) | 0.0348 | 0 |
| `G-T2-H0`: sender gives up at 2 s; receiver any order | 97.4 % | 97.2 % | 97.4 % | 5.5 / 5.9 / never (inf) | 0.0439 | 0.0293 |
| `G-T2-Hinf`: sender gives up at 2 s; receiver strict order | 97.4 % | 97.2 % | 97.4 % | 16 / 28 / never (inf) | 0.0439 | 0 |
| `G-Tinf-H0`: sender never gives up (R-timer probe); receiver any order | 100.0 % | 97.2 % | 97.4 % | 5.5 / 5.9 / 5165 | 0.0715 | 0.0286 |
| `G-T0p4-H0p02`: sender gives up at 0.4 s; receiver holds a hole 20 ms | 97.2 % | 97.2 % | 97.2 % | 16 / 23 / never (inf) | 0.0348 | 0.00274 |

#### E-L10

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip) | 99.0 % | 96.9 % | 99.0 % | 150 / 153 / 1802 | 0.546 | 0.0691 |
| `C3`: never gives up (rho = 1), old tail sweep; receiver strict order | 100.0 % | 92.4 % | 99.8 % | 285 / 380 / 1833 | 0.582 | 0 |
| `C3R`: never gives up, R-timer probe for the open tail, RFC 6298 RTT; receiver strict order (S3 at T = inf, H = inf) | 99.8 % | 92.4 % | 97.2 % | 285 / 380 / 27691 | 0.139 | 0 |
| `G-T0p4-H0`: sender gives up on a unit 0.4 s after hand-over; receiver releases at once, any order (H = 0) | 97.1 % | 97.1 % | 97.1 % | 51 / 152 / never (inf) | 0.112 | 0.101 |
| `G-T0p4-Hinf`: sender gives up at 0.4 s; receiver strict order (H = inf) | 97.1 % | 93.0 % | 97.1 % | 285 / 372 / never (inf) | 0.112 | 0 |
| `G-T2-H0`: sender gives up at 2 s; receiver any order | 97.2 % | 97.1 % | 97.2 % | 51 / 152 / never (inf) | 0.113 | 0.101 |
| `G-T2-Hinf`: sender gives up at 2 s; receiver strict order | 97.2 % | 92.4 % | 97.2 % | 285 / 380 / never (inf) | 0.113 | 0 |
| `G-Tinf-H0`: sender never gives up (R-timer probe); receiver any order | 99.8 % | 97.1 % | 97.2 % | 51 / 152 / 27691 | 0.139 | 0.0982 |
| `G-T0p4-H0p02`: sender gives up at 0.4 s; receiver holds a hole 20 ms | 97.1 % | 97.1 % | 97.1 % | 70 / 152 / never (inf) | 0.112 | 0.101 |

#### M-I2

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip) | 99.3 % | 39.9 % | 99.3 % | 1327 / 1422 / 1520 | 9.26 | 0.0183 |
| `C3`: never gives up (rho = 1), old tail sweep; receiver strict order | 100.0 % | 39.3 % | 100.0 % | 1330 / 1439 / 1527 | 9.45 | 0 |
| `C3R`: never gives up, R-timer probe for the open tail, RFC 6298 RTT; receiver strict order (S3 at T = inf, H = inf) | 100.0 % | 39.3 % | 100.0 % | 1348 / 1453 / 1542 | 0.228 | 0 |
| `G-T0p4-H0`: sender gives up on a unit 0.4 s after hand-over; receiver releases at once, any order (H = 0) | 40.0 % | 40.0 % | 40.0 % | never (inf) / never (inf) / never (inf) | 0.038 | 0.0285 |
| `G-T0p4-Hinf`: sender gives up at 0.4 s; receiver strict order (H = inf) | 40.0 % | 39.3 % | 40.0 % | never (inf) / never (inf) / never (inf) | 0.038 | 0 |
| `G-T2-H0`: sender gives up at 2 s; receiver any order | 100.0 % | 40.0 % | 100.0 % | 1340 / 1413 / 1522 | 0.228 | 0.193 |
| `G-T2-Hinf`: sender gives up at 2 s; receiver strict order | 100.0 % | 39.3 % | 100.0 % | 1348 / 1453 / 1542 | 0.228 | 0 |
| `G-Tinf-H0`: sender never gives up (R-timer probe); receiver any order | 100.0 % | 40.0 % | 100.0 % | 1340 / 1413 / 1522 | 0.228 | 0.193 |
| `S3x`: as 'gives up at 0.4 s, strict order' but the pause counts verdicted seqs as gone (stage 3's brake) | 40.0 % | 39.3 % | 40.0 % | never (inf) / never (inf) / never (inf) | 0.0417 | 0 |
| `G-T0p4-H0p02`: sender gives up at 0.4 s; receiver holds a hole 20 ms | 40.0 % | 40.0 % | 40.0 % | never (inf) / never (inf) / never (inf) | 0.038 | 0.00264 |

#### M-L10

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip) | 98.9 % | 38.9 % | 98.9 % | 1665 / 1776 / 1840 (never in 10 of 12 seeds) | 2.7 | 0.0306 |
| `C3`: never gives up (rho = 1), old tail sweep; receiver strict order | 100.0 % | 17.6 % | 95.6 % | 1765 / 1922 / 1995 | 2.65 | 0 |
| `C3R`: never gives up, R-timer probe for the open tail, RFC 6298 RTT; receiver strict order (S3 at T = inf, H = inf) | 100.0 % | 17.6 % | 89.0 % | 1911 / 2010 / 2077 | 0.346 | 0 |
| `G-T0p4-H0`: sender gives up on a unit 0.4 s after hand-over; receiver releases at once, any order (H = 0) | 39.1 % | 39.1 % | 39.1 % | never (inf) / never (inf) / never (inf) | 0.134 | 0.0795 |
| `G-T0p4-Hinf`: sender gives up at 0.4 s; receiver strict order (H = inf) | 39.1 % | 18.4 % | 39.1 % | never (inf) / never (inf) / never (inf) | 0.134 | 0 |
| `G-T2-H0`: sender gives up at 2 s; receiver any order | 100.0 % | 39.1 % | 100.0 % | 1696 / 1748 / 1854 | 0.345 | 0.244 |
| `G-T2-Hinf`: sender gives up at 2 s; receiver strict order | 100.0 % | 17.6 % | 91.0 % | 1909 / 1996 / 2045 | 0.345 | 0 |
| `G-Tinf-H0`: sender never gives up (R-timer probe); receiver any order | 100.0 % | 39.1 % | 100.0 % | 1696 / 1748 / 1854 | 0.346 | 0.244 |
| `S3x`: as 'gives up at 0.4 s, strict order' but the pause counts verdicted seqs as gone (stage 3's brake) | 39.1 % | 18.4 % | 39.0 % | never (inf) / never (inf) / never (inf) | 0.134 | 0 |
| `G-T0p4-H0p02`: sender gives up at 0.4 s; receiver holds a hole 20 ms | 39.1 % | 39.1 % | 39.1 % | never (inf) / never (inf) / never (inf) | 0.134 | 0.0795 |

#### K-I2

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip) | 99.5 % | 97.9 % | 99.5 % | 5.5 / 6.0 / 1084 | 0.647 | 0.024 |
| `C3`: never gives up (rho = 1), old tail sweep; receiver strict order | 100.0 % | 98.1 % | 100.0 % | 5.5 / 6.0 / 924 | 0.777 | 0 |
| `C3R`: never gives up, R-timer probe for the open tail, RFC 6298 RTT; receiver strict order (S3 at T = inf, H = inf) | 100.0 % | 98.1 % | 100.0 % | 5.5 / 6.0 / 935 | 0.0896 | 0 |
| `G-T0p4-H0`: sender gives up on a unit 0.4 s after hand-over; receiver releases at once, any order (H = 0) | 98.1 % | 98.1 % | 98.1 % | 5.5 / 6.0 / never (inf) | 0.0603 | 0.00965 |
| `G-T0p4-Hinf`: sender gives up at 0.4 s; receiver strict order (H = inf) | 98.1 % | 98.1 % | 98.1 % | 5.5 / 6.0 / never (inf) | 0.0603 | 0 |
| `G-T2-H0`: sender gives up at 2 s; receiver any order | 100.0 % | 98.1 % | 100.0 % | 5.5 / 6.0 / 922 | 0.0896 | 0.0279 |
| `G-T2-Hinf`: sender gives up at 2 s; receiver strict order | 100.0 % | 98.1 % | 100.0 % | 5.5 / 6.0 / 935 | 0.0896 | 0 |
| `G-Tinf-H0`: sender never gives up (R-timer probe); receiver any order | 100.0 % | 98.1 % | 100.0 % | 5.5 / 6.0 / 922 | 0.0896 | 0.0279 |
| `G-T0p4-H0p02`: sender gives up at 0.4 s; receiver holds a hole 20 ms | 98.1 % | 98.1 % | 98.1 % | 5.5 / 6.0 / never (inf) | 0.0603 | 0.000849 |

#### K-L10

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip) | 99.0 % | 97.0 % | 99.0 % | 51 / 171 / 1529 (never in 4 of 12 seeds) | 0.31 | 0.0625 |
| `C3`: never gives up (rho = 1), old tail sweep; receiver strict order | 100.0 % | 97.7 % | 100.0 % | 66 / 174 / 1140 | 0.353 | 0 |
| `C3R`: never gives up, R-timer probe for the open tail, RFC 6298 RTT; receiver strict order (S3 at T = inf, H = inf) | 100.0 % | 97.5 % | 100.0 % | 57 / 171 / 1239 | 0.142 | 0 |
| `G-T0p4-H0`: sender gives up on a unit 0.4 s after hand-over; receiver releases at once, any order (H = 0) | 97.8 % | 97.8 % | 97.8 % | 51 / 156 / never (inf) | 0.117 | 0.103 |
| `G-T0p4-Hinf`: sender gives up at 0.4 s; receiver strict order (H = inf) | 97.8 % | 97.6 % | 97.8 % | 57 / 171 / never (inf) | 0.117 | 0 |
| `G-T2-H0`: sender gives up at 2 s; receiver any order | 100.0 % | 97.8 % | 100.0 % | 51 / 156 / 1086 | 0.142 | 0.123 |
| `G-T2-Hinf`: sender gives up at 2 s; receiver strict order | 100.0 % | 97.5 % | 100.0 % | 57 / 171 / 1239 | 0.142 | 0 |
| `G-Tinf-H0`: sender never gives up (R-timer probe); receiver any order | 100.0 % | 97.8 % | 100.0 % | 51 / 156 / 1086 | 0.142 | 0.123 |
| `G-T0p4-H0p02`: sender gives up at 0.4 s; receiver holds a hole 20 ms | 97.8 % | 97.8 % | 97.8 % | 51 / 156 / never (inf) | 0.117 | 0.103 |

#### CL

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip) | 91.5 % | 19.9 % | 33.8 % | 2583 / 3972 / never (inf) | 4.65 | 0.252 |
| `C3`: never gives up (rho = 1), old tail sweep; receiver strict order | 100.0 % | 19.9 % | 20.9 % | 5759 / 10849 / 11866 | 8.91 | 0 |
| `C3R`: never gives up, R-timer probe for the open tail, RFC 6298 RTT; receiver strict order (S3 at T = inf, H = inf) | 100.0 % | 23.9 % | 100.0 % | 866 / 1565 / 1783 | 0.0751 | 0 |
| `G-T0p4-H0`: sender gives up on a unit 0.4 s after hand-over; receiver releases at once, any order (H = 0) | 84.8 % | 84.8 % | 84.8 % | 5.7 / never (inf) / never (inf) | 0.0341 | 0.0297 |
| `G-T0p4-Hinf`: sender gives up at 0.4 s; receiver strict order (H = inf) | 84.8 % | 84.7 % | 84.8 % | 17 / never (inf) / never (inf) | 0.0341 | 0 |
| `G-T2-H0`: sender gives up at 2 s; receiver any order | 100.0 % | 31.8 % | 100.0 % | 716 / 1410 / 1536 | 0.0751 | 0.0405 |
| `G-T2-Hinf`: sender gives up at 2 s; receiver strict order | 100.0 % | 23.9 % | 100.0 % | 866 / 1565 / 1783 | 0.0751 | 0 |
| `G-Tinf-H0`: sender never gives up (R-timer probe); receiver any order | 100.0 % | 31.8 % | 100.0 % | 716 / 1410 / 1536 | 0.0751 | 0.0405 |
| `S3x`: as 'gives up at 0.4 s, strict order' but the pause counts verdicted seqs as gone (stage 3's brake) | 87.8 % | 83.4 % | 87.8 % | 20 / never (inf) / never (inf) | 0.0659 | 0 |
| `S2-T0p4-H0`: stage 3 sender (timer verdict without evidence rule, old sweep) at 0.4 s; receiver any order | 87.5 % | 79.4 % | 87.5 % | 398 / never (inf) / never (inf) | 2.88 | 0.0161 |
| `G-T0p4-H0p02`: sender gives up at 0.4 s; receiver holds a hole 20 ms | 84.8 % | 84.8 % | 84.8 % | 16 / never (inf) / never (inf) | 0.0341 | 0.00293 |

#### OV

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip) | 95.8 % | 6.0 % | 31.4 % | 3162 / 5668 / never (inf) | 2.06 | 0.0429 |
| `C3`: never gives up (rho = 1), old tail sweep; receiver strict order | 100.0 % | 2.5 % | 24.4 % | 3921 / 6925 / 7573 | 2.13 | 0 |
| `C3R`: never gives up, R-timer probe for the open tail, RFC 6298 RTT; receiver strict order (S3 at T = inf, H = inf) | 100.0 % | 8.6 % | 75.1 % | 1394 / 2342 / 2569 | 0.0332 | 0 |
| `G-T0p4-H0`: sender gives up on a unit 0.4 s after hand-over; receiver releases at once, any order (H = 0) | 81.5 % | 81.5 % | 81.5 % | 390 / never (inf) / never (inf) | 0.00546 | 0.00494 |
| `G-T0p4-Hinf`: sender gives up at 0.4 s; receiver strict order (H = inf) | 81.5 % | 8.7 % | 81.5 % | 488 / never (inf) / never (inf) | 0.00546 | 0 |
| `G-T2-H0`: sender gives up at 2 s; receiver any order | 96.1 % | 16.5 % | 96.1 % | 1203 / 1990 / never (inf) | 0.0277 | 0.025 |
| `G-T2-Hinf`: sender gives up at 2 s; receiver strict order | 96.1 % | 8.6 % | 75.1 % | 1394 / 2089 / never (inf) | 0.0277 | 0 |
| `G-Tinf-H0`: sender never gives up (R-timer probe); receiver any order | 100.0 % | 16.5 % | 83.2 % | 1203 / 2163 / 2379 | 0.0332 | 0.0299 |
| `S3x`: as 'gives up at 0.4 s, strict order' but the pause counts verdicted seqs as gone (stage 3's brake) | 81.7 % | 8.6 % | 81.7 % | 491 / never (inf) / never (inf) | 0.00545 | 0 |
| `S2-T0p4-H0`: stage 3 sender (timer verdict without evidence rule, old sweep) at 0.4 s; receiver any order | 82.1 % | 5.8 % | 82.1 % | 442 / never (inf) / never (inf) | 1.27 | 0.00643 |


## A/A self-check (frozen before any decision cell)

`nf_stage3b.json` was written once, after the A/A runs (S3 with the R timer at T = inf, H = inf, phase variant 0 = arm C3R and variant 1 = arm AA-1, 12 seeds, 12 cells) and before any other arm was read; it was not rewritten. NF = p95 of |A - A'| per cell and readout, NF_K = NF_P = that floor. Self-check (each A/A difference read against the T bar only): 312 readings over 12 cells, 237 equivalent, 67 undetermined, 2 shown (M-I2 `dh_p50` -4.6 ms [-8.2, -1.1] vs 1 ms; CL `dh_p99` +17.3 ms [6.1, 31.2] vs 1 ms). The frozen table was kept. An unregistered replication with two fresh phase draws (variants 2 and 3, all 12 cells) reads 1 of 234 shown (M-I2 `dh_p90` -4.3 ms [-7.3, -1.4]); both flagged readings are post-outage delay quantiles of the mid-stream cell, which depend on the sweep and report phase by a few ms (M-I2) or on queue state (CL). The bars of 1 ms on delay quantiles are therefore tighter than the phase noise in M-I2; the readings that use them there are marked in the detail.

## Worker decisions (every reading of an ambiguity or a choice not fixed by the spec)

1. **The serve gate of S3 is `can_resend`, not a literal "copies == 0 or evidence".** Â§13.7.1 item 2 says an S3 candidate that passes L2 is served only if `copies[s] == 0` or evidence holds, and item 2's last bullet says `resend_gate = "evidence"` replaces `can_resend` in the link cells only. Applying the evidence form everywhere would break Q3b-1 (S3 with the old sweep at T = inf equals C3 in the E, M and K cells, which resend on the old gate). So S3's serve goes through `can_resend`, which is the evidence form when `resend_gate = "evidence"` and the stage 3 form otherwise. Verdicts are always evidence-gated for S3.
2. **S3 and S3x are sender names selecting the S2 family; the stage 3b behaviour comes from the `Cfg` fields.** `batch3b.s3_cfg` sets `evidence_verdict`, `deq_expiry`, `o_excl_verdict` (False for S3, True for S3x), `rtt_est = "rfc"` and `tail_elig`. With the fields at their defaults S3 would equal S2 except for the RTT-dependent Ã´; the arms are always built by `s3_cfg`. `maybe_resume` after verdicts is skipped for sender "S3" only (S3x, S2, S2c keep stage 3's calls).
3. **R, before the first sample, uses the RFC 9002 seed:** srtt 0.333 s, rttvar 0.1665 s, giving R = 0.333 + 0.666 + 0.002 + 0.002 = 1.003 s. g and ack_delay_max are both the 2 ms tick. Ã´ for S3 is srtt/2 with the same seed (0.1665 s before the first sample); S2 keeps the exact owd/2.
4. **The RTT sample** is taken inside the `gen > last_gen` branch of `on_report`, before `rhi_known` is updated, from the report's `rhi` when `copies[rhi] == 0` at that moment (Karn). With `rtt_est = "rfc"` the stage 2 estimator (highest newly acknowledged seq, EWMA 1/8) is not run. `self.srtt` stays `None` until the first sample, so the early-skip receiver in C1 keeps its 50 ms seed before it. In the link cells C1 and C3 also run the RFC estimator (the spec: every arm shares it).
5. **Ï„_rx** is the receiver's maximum send stamp over every data arrival, duplicates included, floors excluded; the report tuple grows an eighth field (outputs unaffected).
6. **Timers that must move forward.** The verdict timer re-pushes at `max(h + T - Ã´, now + 1e-9)` (L2 passes) or `max(last_send + R, now + 1e-9)` (no evidence yet), and the probe timer at `max(last_send[s_o] + R, now + 1e-9)`: float rounding could otherwise re-fire an event at the same instant forever. The first probe event is pushed when seq 0 is sent.
7. **The probe timer's `s_o`** skips verdicted seqs from `max(rhi_known + 1, A_known)`; the probe calls `act` with that one seq, so L2 and the gate decide as for any candidate (resend_by_R counts its copy).
8. **Resend triggers** (`resend_by_report`, `resend_by_sweep`, `resend_by_R`) count copies sent from a report-driven `act`, a sweep with the old eligibility, and the R timer (probes and the R-eligible re-serve of reported holes in object mode).
9. **The bottleneck.** Floors and resends go through the same queue as data; reports are unlimited. A datagram dropped at the queue gets its path sequence number (the early-skip receiver sees the gap) and, for an original, `atilde = t + delay.at(t)` (no queue wait; it only enters the stage 2 `v` readouts); it is appended to `orig_lost`. `queue_drop_in/out` classify by the attempt time against `dead`. `link_util_w` is busy time in [1 s, 10 s] over 9 s.
10. **Pause readouts.** `pause_time` is the sum of the intervals from `paused = True` to the next `maybe_resume`, closed at `t_last` if the run ends paused; `pause_time_w` the part inside [1, 10]. In OV every arm is paused over the whole window (9.000 s), so the "pause" branch of `throttle` is never the discriminator there, `link_util_w` is. In CL `pause_time` is reported as is.
11. **Delay quantiles** use the nearest-rank percentile over `units_total` units (for streaming runs, which send one unit more than `int(dur x rate)`, over the larger count). The paired differences set a pair of equal infinities to 0 and read a one-sided infinity as shown (a seed-level bootstrap is not defined on infinity); where only some seeds are infinite the word is "undetermined (infinite seeds)" or "shown larger (some seeds infinite)".
12. **`resume_delay`** = the first send at or after the outage end minus the outage end; undefined (None) when nothing is left to send (the M cells at T = 0.4).
13. **Stall decomposition.** The registered text decomposes "the longest stall episode". The longest episode of a run is not always the one that spans the outage. The parts are computed for the longest episode that is still open at the first floor arrival after the outage end; (i) = that arrival minus the outage end, (ii) = the last fill inside the episode after it, (iii) = the rest of the time from the outage end to the episode end. Where no episode is open at that floor the parts are None and the seed is not counted (6 of 12 seeds at K-I2, 10 of 12 at K-L10). See deviation 1.
14. **Crossings** are read on the seed-mean `share_T` curve (the registered reading), linearly interpolated; per-seed crossings are shown next to them. The T = 0.4 level of the S2 crossing comes from S2 at T = 0.4 in the same cell (an arm added; deviation 2).
15. **Q3b-2 and S3x.** "S3" in the vta bullet covers every S3 arm (grid, nudges, sweeps, positive control), not S3x; S3x is reported separately (CL: 228 to 259 delivered verdicts of about 11 500, 2.2 %, the exclusion arm resumes on a verdict).
16. **Q3b-3 and Q3b-4 H.** Where the registration does not name H for T = 2 and the comparison reads, both H = 0 and H = inf are shown; the verdict words use the one that matches C3's receiver (H = inf) first, and say so.
17. **Cell definitions.** CL and OV set `J = 0.001` (cell I2, as the spec) and `pause_B = 1000`; X15 and X20 use `t_limit = 80` (60 s of units). `K-L10b` carries the sweeps and the S2 sweep only; E-I2n carries C3, C3R, the identity arms, A/A and the positive control only. The A/A variant 0 is the arm C3R itself (same configuration).
18. **Placement.** `pc_place.py` pools the dead-tail delivery ages of the 12 C3R runs (a unit never delivered is not in the pool and is counted), rounds the median to 1 ms and runs once; the case (step or ramp) follows `w < 0.01 p`.
19. **Tests.** The registered tests are in `test_toy2.py` (class `Stage3b`, `test_stage3b_positive_controls`); test 1 re-runs one stored stage 3 run if `out3/` exists; the vta test uses K-I2 at T = 0.14 as registered, plus K-L10 at T = 0.14 as the witness that stage 3 verdicts delivered data (K-I2's round trip is 10 ms, so at T = 0.14 stage 3 has nothing spurious there); the brake test wraps every handler and requires the only decreases of `O_ctl()` to happen in `on_report` for S3, with S3x as the positive control (a decrease in `on_verdict_timer`); the dequeue test checks the trace of dequeue decisions in a shortened OV; the link test checks Q = 0 drops everything and a large link adds 1/C + delay.at(t + 1/C).

## Deviations (timing as required)

1. **The stall-decomposition definition was changed after the first read.** The first read of Q3b-9 (after all runs) showed part (ii) values of 9 to 34 s in seeds where the run's longest stall was a short episode unrelated to the outage. The definition in `toy2.py` (`summary3b`) was changed to the episode open at the first post-outage floor (decision 13) and a 1e-9 tolerance added on a second pass; the 24 K-cell runs of the arms K-I2 and K-L10 at T = 0.4, H = inf were re-run twice (the first superseded outputs were kept outside the tracked tree). The tolerance changed no reading; the first definition changed (ii) and the number of seeds with defined parts. No other arm reads these keys. This is a readout definition, not a prediction, bar or cell, but it came after seeing data.
2. **An S2 T = 0.4 arm was added to K-L10** after the first read of Q3b-5 (the registered text asks for the crossing of S2 on the K-L10 grid but only runs the T = 0.4 level for the S3 arms; the K-L10b reference had been added with the cell, K-L10's had not). First read, with the S3 level as reference: mid crossing 0.1725 s; final, with S2's own level: 0.1726 s. The S2 shift (29.9 ms) is unchanged.
3. **Unregistered runs.** The A/A replication (arms AA-2 and AA-3), the S2 T = 0.4 level arm, the `dh` fix below; all labelled where used, none changes a registered reading.
4. **`dh` for streaming runs** (units sent above `units_total`) was fixed after the identity arms ran (the first full test run caught an index error in the stage 1 and 2 suites; object-mode outputs are unaffected, and the identity arms were not re-run: their keys do not contain `dh`).
5. **Report-script fixes** (a seconds-to-milliseconds slip in the pause-time print, S3x separated from S3 in the vta count) were made after the first print of those tables; printing only.
6. **No change** to any prediction, bar, cell or arm list after seeing data, and `nf_stage3b.json` and `pc_stage3b.json` were written once.

## Things that looked wrong or surprising

1. **Serial tail recovery under the R timer** (see "In plain words"). Â§13.4 and the sentence in Â§13.1 about evidence "for every older open-tail seq at once" describe a gate; the only sender of the open tail at T = inf is the probe, one datagram per R. E-L10: C3R delivers 99.79 % in 40 s, never finishes (stalled in 12 of 12 seeds); E-I2: tail median age 4.3 s. It is hidden at a finite T because the verdict timer judges the older tail units on the probe's evidence. A repair would be to let a report-driven pass resend (or judge) every open-tail unit that has evidence, which is a design change, not a code fix, and is not run here.
2. **The comparison arms collapse on the link for a reason the registration did not model.** With `resend_gate = "evidence"` a resent copy is judged lost as soon as any later datagram arrives, and a shallow queue drops copies, so C3, C1 and S2 resend the same units again and again: C3 in CL sends 8.9 resends per delivered unit and loses 684 000 datagrams to the queue outside the outage (S3 at T = inf: 0.075 and 2 235). The registered C3 bands (0.30 to 0.47 within 0.4 s in CL) assumed the drain arithmetic of Â§13.6 without that flooding.
3. **At T = 2 s the revised sender gives up on nothing it needs to in a 10 s overload**: the backlog reaches age T - R = 1.9 s only after 11 s, so T = 2 behaves like T = infinity for most of the run (share within 2 s 96 % at H = 0), and the registered 77 to 83 % only appears for a longer run.
4. **S3 at T = 0.4 in OV reads within the registered band, but the delay quantiles p90 and p99 are "never"** (18.5 % of the units are not delivered, so any quantile above the 81st percentile falls on a given-up unit). The p50 of 390 ms is pinned near T - R, as Â§13.2 says.
5. **M-cell post-outage quantiles are phase-sensitive by a few ms** (two A/A readings shown, one replication shown). The 1 ms bar on `dh` quantiles is below that.
6. **`pause_time` is a whole-run total and depends on the run's length**, so "within 5 % of C3" compares arms that finish at different times; over the common window every arm is paused 9.000 s. A different readout (pause time in the window, or pause time per delivered unit) would have been a better registration.
7. **S3x pauses longer than S3 in CL** (2.56 s against 1.42 s) although it resumes sooner after the outage; not investigated. Not predicted; bounded by Q3b-8 (share equivalent).
8. **C3R is slightly below C3 within 0.4 s in K-L10** (0.9746 against 0.9766) with a lower median delay (57 against 66 ms): not investigated; the R timer fires later than the 100 ms sweep for some units, which is one candidate.

## Recommended next steps (not built)

- Decide the open-tail law for T = infinity: either keep one probe per R and accept serial recovery, or add evidence-driven resend of the whole open tail on the report path (then re-run E and CL with C3R).
- Replace the pause-time readout and the C3 bands for link cells in the registration before any further run; C3's flood needs its own explanation (an RFC 8985-style reordering window, or a pacing of resends).
- Keep the E-I2n narrow-tail cell, but place it on a sender whose tail recovery is not serial; the K control cannot be run until then.

## Reading detail (generated: `python summarize3b.py ident|ident-trace|freeze|q2..q9|tables|aa|rep`)

### Q3b-1 stored runs

#### Q3b-1: S3 (P) at T = inf and T = 1e6 against C3; S3 (R) at T = 1e6 against C3R; float-equal on 20 keys, 12 seeds

- E-I2: ID-P-inf vs C3: 12/12; ID-P-1e6 vs C3: 12/12; ID-R-1e6 vs C3R: 12/12
- E-I2n: ID-P-inf vs C3: 12/12; ID-P-1e6 vs C3: 12/12; ID-R-1e6 vs C3R: 12/12
- E-L10: ID-P-inf vs C3: 12/12; ID-P-1e6 vs C3: 12/12; ID-R-1e6 vs C3R: 12/12
- M-I2: ID-P-inf vs C3: 12/12; ID-P-1e6 vs C3: 12/12; ID-R-1e6 vs C3R: 12/12
- M-L10: ID-P-inf vs C3: 12/12; ID-P-1e6 vs C3: 12/12; ID-R-1e6 vs C3R: 12/12
- K-I2: ID-P-inf vs C3: 12/12; ID-P-1e6 vs C3: 12/12; ID-R-1e6 vs C3R: 12/12
- K-L10: ID-P-inf vs C3: 12/12; ID-P-1e6 vs C3: 12/12; ID-R-1e6 vs C3R: 12/12
- K-L10b: ID-P-inf vs C3: 12/12; ID-P-1e6 vs C3: 12/12; ID-R-1e6 vs C3R: 12/12
- CL: ID-P-inf vs C3: 12/12; ID-P-1e6 vs C3: 12/12; ID-R-1e6 vs C3R: 12/12
- OV: ID-P-inf vs C3: 12/12; ID-P-1e6 vs C3: 12/12; ID-R-1e6 vs C3R: 12/12
- X15: ID-P-inf vs C3: 12/12; ID-P-1e6 vs C3: 12/12; ID-R-1e6 vs C3R: 12/12
- X20: ID-P-inf vs C3: 12/12; ID-P-1e6 vs C3: 12/12; ID-R-1e6 vs C3R: 12/12

seed-level diffs: 0 of 432

#### Q3b-1 sender traces (reruns, final code)

- CL: traces identical/identical/identical (seeds [0, 1, 2]; 1005990 entries C3, 107609 entries C3R)
- E-I2: traces identical/identical/identical (seeds [0, 1, 2]; 33161 entries C3, 12995 entries C3R)
- E-I2n: traces identical/identical/identical (seeds [0, 1, 2]; 15462 entries C3, 12669 entries C3R)
- E-L10: traces identical/identical/identical (seeds [0, 1, 2]; 19100 entries C3, 13825 entries C3R)
- K-I2: traces identical/identical/identical (seeds [0, 1, 2]; 5341 entries C3, 3252 entries C3R)
- K-L10: traces identical/identical/identical (seeds [0, 1, 2]; 4026 entries C3, 3403 entries C3R)
- K-L10b: traces identical/identical/identical (seeds [0, 1, 2]; 4681 entries C3, 3400 entries C3R)
- M-I2: traces identical/identical/identical (seeds [0, 1, 2]; 127251 entries C3, 14845 entries C3R)
- M-L10: traces identical/identical/identical (seeds [0, 1, 2]; 43415 entries C3, 16296 entries C3R)
- OV: traces identical/identical/identical (seeds [0, 1, 2]; 379254 entries C3, 123960 entries C3R)
- X15: traces identical/identical/identical (seeds [0, 1, 2]; 3308 entries C3, 3308 entries C3R)
- X20: traces identical/identical/identical (seeds [0, 1, 2]; 3308 entries C3, 3308 entries C3R)

#### check_ident3b.py

```text
A1: 34 configurations against the stage 3 commit 39efa6a3, every old key, traces and per-seq arrays: identical
A2: 29 stage 1 and 2 configurations against the stage 2 commit c0f33f8e, every key of the old output: identical
B: 8 stored stage 3 runs re-run and compared on every stored key: identical
IDENTITY CHECK: PASS
```



### q2
#### Q3b-2 D1 fixed

vta <= 0.001 x verdicts for S3 in every (cell, arm, seed) with verdicts > 100:
  S3 (every S3 arm): 1326 (cell, arm, seed) runs with verdicts > 100 in 10 cells; 0 violate; worst vta/verdicts = 0.00e+00
  VIOLATION CL S3x 0 244 11560
  VIOLATION CL S3x 1 242 11566
  VIOLATION CL S3x 2 252 11546
  VIOLATION CL S3x 3 238 11536
  VIOLATION CL S3x 4 259 11561
  VIOLATION CL S3x 5 254 11541
  VIOLATION CL S3x 6 245 11533
  VIOLATION CL S3x 7 239 11547
  VIOLATION CL S3x 8 252 11556
  VIOLATION CL S3x 9 241 11536
  VIOLATION CL S3x 10 229 11546
  VIOLATION CL S3x 11 228 11530
  S3x (the exclusion arm, not part of the prediction): 48 (cell, arm, seed) runs with verdicts > 100 in 4 cells; 12 violate; worst vta/verdicts = 2.24e-02

OV: S3 pause_time against C3's (within 5 %), throttle, S2 against S3 (verdicts > 10x, vta > 0.5 x own verdicts):
  C3             pause_time 17.073 s (1.000 of C3's), in [1,10] s 9.000 s (1.000 of C3's), throttle link/pause/none = 12/0/0, verdicts 0, vta 0, util_w 1.000, queue_drop 2.007e+05
  C1             pause_time 15.639 s (0.916 of C3's), in [1,10] s 9.000 s (1.000 of C3's), throttle link/pause/none = 12/0/0, verdicts 161.5, vta 157, util_w 1.000, queue_drop 1.962e+05
  C3R            pause_time 11.922 s (0.698 of C3's), in [1,10] s 9.000 s (1.000 of C3's), throttle link/pause/none = 12/0/0, verdicts 0, vta 0, util_w 1.000, queue_drop 0
  G-T0p4-H0      pause_time 9.912 s (0.581 of C3's), in [1,10] s 9.000 s (1.000 of C3's), throttle link/pause/none = 12/0/0, verdicts 2509, vta 0, util_w 1.000, queue_drop 0
  G-T0p4-Hinf    pause_time 9.912 s (0.581 of C3's), in [1,10] s 9.000 s (1.000 of C3's), throttle link/pause/none = 12/0/0, verdicts 2509, vta 0, util_w 1.000, queue_drop 0
  G-T2-H0        pause_time 11.511 s (0.674 of C3's), in [1,10] s 9.000 s (1.000 of C3's), throttle link/pause/none = 12/0/0, verdicts 586.1, vta 0, util_w 1.000, queue_drop 0
  G-T2-Hinf      pause_time 11.511 s (0.674 of C3's), in [1,10] s 9.000 s (1.000 of C3's), throttle link/pause/none = 12/0/0, verdicts 586.1, vta 0, util_w 1.000, queue_drop 0
  G-Tinf-H0      pause_time 11.922 s (0.698 of C3's), in [1,10] s 9.000 s (1.000 of C3's), throttle link/pause/none = 12/0/0, verdicts 0, vta 0, util_w 1.000, queue_drop 0
  S3x            pause_time 9.910 s (0.580 of C3's), in [1,10] s 9.000 s (1.000 of C3's), throttle link/pause/none = 12/0/0, verdicts 2515, vta 0, util_w 1.000, queue_drop 0
  S2-T0p4-H0     pause_time 10.066 s (0.590 of C3's), in [1,10] s 9.000 s (1.000 of C3's), throttle link/pause/none = 12/0/0, verdicts 1.132e+05, vta 9.17e+04, util_w 1.000, queue_drop 1.404e+05


### q3
#### Q3b-3 overload (OV)

| arm | share_any | share_by_0p4 | share_by_2 | dh_p50 | dh_p90 | dh_p99 | intake_expired | intake_expired_deq | link_util_w | pause_time | queue_drop |
|---|---|---|---|---|---|---|---|---|---|---|---|
| C1 | 0.9582 | 0.05963 | 0.3142 | 3162 | 5668 | inf | 0 | 0 | 1 | 15.64 | 1.962e+05 |
| C3 | 1 | 0.02454 | 0.2436 | 3921 | 6925 | 7573 | 0 | 0 | 1 | 17.07 | 2.007e+05 |
| C3R | 1 | 0.08587 | 0.7514 | 1394 | 2342 | 2569 | 0 | 0 | 1 | 11.92 | 0 |
| G-T0p4-H0 | 0.8152 | 0.8147 | 0.8152 | 389.6 | inf | inf | 0 | 1.967e+04 | 1 | 9.912 | 0 |
| G-T0p4-Hinf | 0.8152 | 0.08655 | 0.8152 | 488.4 | inf | inf | 0 | 1.967e+04 | 1 | 9.912 | 0 |
| G-T2-H0 | 0.9612 | 0.1649 | 0.9607 | 1203 | 1990 | inf | 0 | 4070 | 1 | 11.51 | 0 |
| G-T2-Hinf | 0.9612 | 0.08587 | 0.7512 | 1394 | 2089 | inf | 0 | 4070 | 1 | 11.51 | 0 |
| G-Tinf-H0 | 1 | 0.1649 | 0.832 | 1203 | 2163 | 2379 | 0 | 0 | 1 | 11.92 | 0 |
| S3x | 0.8167 | 0.08639 | 0.8167 | 491.2 | inf | inf | 0 | 1.948e+04 | 1 | 9.91 | 0 |
| S2-T0p4-H0 | 0.8207 | 0.05829 | 0.8207 | 442.3 | inf | inf | 0 | 0 | 1 | 10.07 | 1.404e+05 |
  C1 share_by_0p4 per seed min/max 0.0589 0.0611; share_by_2 0.3110 0.3181; dh_p50 ms min/max 3128 3192
  C3 share_by_0p4 per seed min/max 0.0175 0.0358; share_by_2 0.2076 0.2588; dh_p50 ms min/max 3597 4564
  G-T0p4-H0 share_by_0p4 per seed min/max 0.8139 0.8153; share_by_2 0.8145 0.8158; dh_p50 ms min/max 389.5 389.8
  G-T2-Hinf share_by_0p4 per seed min/max 0.0749 0.0936; share_by_2 0.7306 0.7663; dh_p50 ms min/max 1351 1448
  G-T2-H0 share_by_0p4 per seed min/max 0.1629 0.1670; share_by_2 0.9600 0.9613; dh_p50 ms min/max 1198 1209


### q4
#### Q3b-4 drain gain (CL)

| arm | share_any | share_by_0p4 | share_by_2 | dh_p50 | dh_p90 | dh_p99 | queue_drop_in | queue_drop_out | intake_expired | intake_expired_deq | link_util_w | pause_time | probes | verdicts | vta |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| C1 | 0.9146 | 0.1994 | 0.3377 | 2583 | 3972 | inf | 2.963e+04 | 3.241e+05 | 0 | 0 | 0.982 | 11.73 | 0 | 731.8 | 522.8 |
| C3 | 1 | 0.1989 | 0.2088 | 5759 | 1.085e+04 | 1.187e+04 | 3.813e+04 | 6.845e+05 | 0 | 0 | 0.9799 | 19.38 | 0 | 0 | 0 |
| C3R | 1 | 0.239 | 1 | 866.4 | 1565 | 1783 | 0 | 2235 | 0 | 0 | 0.8228 | 8.02 | 106.9 | 0 | 0 |
| G-T0p4-H0 | 0.8476 | 0.8476 | 0.8476 | 5.666 | inf | inf | 0 | 0 | 1.024e+04 | 3999 | 0.6967 | 1.424 | 32.92 | 1000 | 0 |
| G-T0p4-Hinf | 0.8476 | 0.8465 | 0.8476 | 17 | inf | inf | 0 | 0 | 1.024e+04 | 3999 | 0.6967 | 1.424 | 32.92 | 1000 | 0 |
| G-T2-H0 | 1 | 0.3176 | 1 | 716.3 | 1410 | 1536 | 0 | 2235 | 0 | 0 | 0.8228 | 8.02 | 106.9 | 0 | 0 |
| G-T2-Hinf | 1 | 0.239 | 1 | 866.4 | 1565 | 1783 | 0 | 2235 | 0 | 0 | 0.8228 | 8.02 | 106.9 | 0 | 0 |
| G-Tinf-H0 | 1 | 0.3176 | 1 | 716.3 | 1410 | 1536 | 0 | 2235 | 0 | 0 | 0.8228 | 8.02 | 106.9 | 0 | 0 |
| S3x | 0.8779 | 0.8344 | 0.8779 | 19.66 | inf | inf | 0 | 1557 | 0 | 911.1 | 0.8282 | 2.564 | 105.5 | 1.155e+04 | 243.6 |
| S2-T0p4-H0 | 0.8754 | 0.7945 | 0.8754 | 397.8 | inf | inf | 3.827e+04 | 1.881e+05 | 0 | 0 | 0.9799 | 8.195 | 0 | 8e+04 | 6.754e+04 |
  S3 T=0.4 H=inf - C3    share_by_0p4  0.6477 [0.6473, 0.648] vs m=0.0401: shown larger
  S3 T=0.4 H=0 - C3      share_by_0p4  0.6488 [0.6483, 0.6492] vs m=0.0401: shown larger
  S3 T=2 H=inf - C3      share_by_0p4  0.04017 [0.03596, 0.04463] vs m=0.0401: undetermined
  S3 T=2 H=0 - C3        share_by_0p4  0.1187 [0.1169, 0.1206] vs m=0.0401: shown larger
  S3 T=0.4 H=inf - C3    share_by_2    0.6388 [0.6383, 0.6393] vs m=0.0396: shown larger
  S3 T=0.4 H=0 - C3      share_by_2    0.6388 [0.6383, 0.6393] vs m=0.0396: shown larger
  S3 T=2 H=inf - C3      share_by_2    0.7912 [0.7909, 0.7914] vs m=0.0396: shown larger
  S3 T=2 H=0 - C3        share_by_2    0.7912 [0.7909, 0.7914] vs m=0.0396: shown larger
  S3 T=0.4 H=inf - C3    dh_p50        -5742 [-5839, -5642] vs m=12.2: shown smaller
  S3 T=0.4 H=0 - C3      dh_p50        -5753 [-5850, -5653] vs m=12.2: shown smaller
  S3 T=2 H=inf - C3      dh_p50        -4892 [-4987, -4794] vs m=12.2: shown smaller
  S3 T=2 H=0 - C3        dh_p50        -5042 [-5139, -4943] vs m=12.2: shown smaller
  S3 T=0.4 H=inf - C3    dh_p90        inf [inf-seeds] vs m=12: shown larger
  S3 T=0.4 H=0 - C3      dh_p90        inf [inf-seeds] vs m=12: shown larger
  S3 T=2 H=inf - C3      dh_p90        -9283 [-9409, -9162] vs m=12: shown smaller
  S3 T=2 H=0 - C3        dh_p90        -9439 [-9564, -9318] vs m=12: shown smaller
  S3 T=0.4 H=inf - C3    dh_p99        inf [inf-seeds] vs m=58.1: shown larger
  S3 T=0.4 H=0 - C3      dh_p99        inf [inf-seeds] vs m=58.1: shown larger
  S3 T=2 H=inf - C3      dh_p99        -1.008e+04 [-1.025e+04, -9936] vs m=58.1: shown smaller
  S3 T=2 H=0 - C3        dh_p99        -1.033e+04 [-1.05e+04, -1.018e+04] vs m=58.1: shown smaller
  C1           queue_drop_out per seed: [320352, 318996, 313860, 324720, 320135, 328948, 338405, 328684, 329686, 330011, 321095, 313764] ; queue_drop_in mean 29632.3
  C3           queue_drop_out per seed: [682873, 687877, 695433, 717713, 678660, 682525, 682980, 682685, 687949, 687666, 671492, 655559] ; queue_drop_in mean 38129.2
  C3R          queue_drop_out per seed: [2390, 1939, 2426, 2416, 2406, 2053, 2405, 2064, 2414, 2155, 2058, 2090] ; queue_drop_in mean 0.0
  G-T0p4-H0    queue_drop_out per seed: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] ; queue_drop_in mean 0.0
  G-T0p4-Hinf  queue_drop_out per seed: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] ; queue_drop_in mean 0.0
  G-T2-Hinf    queue_drop_out per seed: [2390, 1939, 2426, 2416, 2406, 2053, 2405, 2064, 2414, 2155, 2058, 2090] ; queue_drop_in mean 0.0
  S3x          queue_drop_out per seed: [1570, 1548, 1542, 1580, 1587, 1539, 1558, 1546, 1582, 1552, 1547, 1532] ; queue_drop_in mean 0.0
  S2-T0p4-H0   queue_drop_out per seed: [188260, 188096, 187758, 188063, 188260, 188176, 187877, 188084, 188104, 188268, 188062, 188019] ; queue_drop_in mean 38265.7


### q5
#### Q3b-5 crossover

- X15: share_T at lowest T 0.8992, at T=0.4 1.0000; mid-level crossing 0.0519 s (per-seed 0.0512 0.0532 0.0527 0.0512 0.0512 0.0512 0.0512 0.0532 0.0513 0.0512 0.0527 0.0532); 90% crossing 0.05003; R_mean 0.0370, srtt_mean/2 0.0159, R+srtt/2 = 0.0528
  curve: 0.040:0.8992 0.042:0.8992 0.044:0.8992 0.046:0.8992 0.048:0.8992 0.050:0.8992 0.052:0.9535 0.054:0.9811 0.056:0.9811 0.058:0.9811 0.060:0.9811 0.062:0.9811 0.064:0.9811 0.066:0.9816 0.068:0.9878 0.070:0.9902
  at the lowest T: resend_by_R per seed [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], resend_by_report [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
- X20: share_T at lowest T 0.8992, at T=0.4 1.0000; mid-level crossing 0.0670 s (per-seed 0.0682 0.0663 0.0663 0.0683 0.0682 0.0664 0.0668 0.0663 0.0684 0.0664 0.0663 0.0664); 90% crossing 0.06503; R_mean 0.0461, srtt_mean/2 0.0207, R+srtt/2 = 0.0667
  curve: 0.055:0.8992 0.057:0.8992 0.059:0.8992 0.061:0.8992 0.063:0.8992 0.065:0.8992 0.067:0.9498 0.069:0.9739 0.071:0.9760 0.073:0.9780 0.075:0.9792 0.077:0.9809 0.079:0.9809 0.081:0.9810 0.083:0.9878 0.085:0.9897
  at the lowest T: resend_by_R per seed [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], resend_by_report [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
- K-L10: share_T at lowest T 0.8771, at T=0.4 0.9782; mid-level crossing 0.1593 s (per-seed 0.159 0.159 0.159 0.159 0.159 0.159 0.159 0.159 0.159 0.159 0.159 0.159); 90% crossing 0.1553; R_mean 0.1063, srtt_mean/2 0.0507, R+srtt/2 = 0.1570
  curve: 0.150:0.8771 0.155:0.8771 0.160:0.9359 0.165:0.9385 0.170:0.9396 0.175:0.9634 0.180:0.9635 0.185:0.9639 0.190:0.9640 0.195:0.9658 0.200:0.9658 0.205:0.9660 0.210:0.9660 0.215:0.9663
  at the lowest T: resend_by_R per seed [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], resend_by_report [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
- K-L10b: share_T at lowest T 0.8771, at T=0.4 0.9771; mid-level crossing 0.1895 s (per-seed 0.19 0.189 0.19 0.19 0.189 0.189 0.19 0.19 0.19 0.189 0.19 0.19); 90% crossing 0.1852; R_mean 0.1263, srtt_mean/2 0.0607, R+srtt/2 = 0.1869
  curve: 0.180:0.8771 0.185:0.8771 0.190:0.9326 0.195:0.9351 0.200:0.9358 0.205:0.9634 0.210:0.9635 0.215:0.9637 0.220:0.9638 0.225:0.9659 0.230:0.9659 0.235:0.9659 0.240:0.9659 0.245:0.9664
  at the lowest T: resend_by_R per seed [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], resend_by_report [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
- shift X15 -> X20 (mid): 15.1 ms; 90%: 15 ms
- shift K-L10 -> K-L10b (mid): 30.2 ms; 90%: 29.93 ms

Stage 3 arm S2 (old sweep) on the K-L10 and K-L10b grids:
- K-L10: S2 mid crossing 0.1726 s; 90% 0.1531; curve 0.150:0.8771 0.155:0.8842 0.160:0.8882 0.165:0.8910 0.170:0.8950 0.175:0.9597 0.180:0.9602 0.185:0.9606 0.190:0.9610 0.195:0.9663 0.200:0.9663 0.205:0.9664 0.210:0.9665 0.215:0.9672
- K-L10b: S2 mid crossing 0.2025 s; 90% 0.17; curve 0.180:0.8950 0.185:0.9019 0.190:0.9061 0.195:0.9087 0.200:0.9127 0.205:0.9620 0.210:0.9624 0.215:0.9628 0.220:0.9633 0.225:0.9668 0.230:0.9670 0.235:0.9671 0.240:0.9672 0.245:0.9676
- S2 shift K-L10 -> K-L10b (mid): 29.9 ms


### q6
#### Q3b-6 continuity (K and P at the nudges; m = max(T bar, A/A floor))

  undetermined K-I2 T-nudge share_by_2 K: -2.778e-05 [-0.0005, 0.0004729] vs m=0.000333: undetermined
  undetermined K-I2 T-nudge share_by_2 P: -1.389e-05 [-0.000375, 0.0003333] vs m=0.000333: undetermined
  undetermined K-L10 T-nudge share_by_2 K: -0.0003889 [-0.0006944, -0.0001389] vs m=0.000333: undetermined
  undetermined K-L10 T-nudge ooo_per_delivered K: -0.0003573 [-0.0006383, -0.0001273] vs m=0.000333: undetermined
  148 K and P statistics: 0 shown, 4 undetermined, 144 equivalent


### q7
#### Q3b-7 positive control

- E-I2n: p = 1.904 s, IQR w = 0.4098 s (21.52 % of p) -> case ramp; share_T at the five points [0.99778, 0.99786, 0.99794, 0.99807, 0.99815]
    K on share_T: 4.116e-05 [-6.86e-06, 8.232e-05] vs m=8.23e-05: equivalent
    P on share_T: -1.372e-05 [-4.459e-05, 1.372e-05] vs m=8.23e-05: equivalent
    J on share_T: 0.0003705 [0.0003293, 0.0004116] vs m=8.23e-05: shown larger
- E-I2: p = 4.328 s, IQR w = 2.8178 s (65.11 % of p) -> case ramp; share_T at the five points [0.98531, 0.98552, 0.98577, 0.98596, 0.9862]
    K on share_T: -1.372e-05 [-0.0001098, 7.546e-05] vs m=8.23e-05: undetermined
    P on share_T: 4.802e-05 [-3.43e-06, 9.948e-05] vs m=8.23e-05: undetermined
    J on share_T: 0.0008919 [0.0008301, 0.0009536] vs m=8.23e-05: shown larger
- E-L10: p = 20.249 s, IQR w = 18.7023 s (92.36 % of p) -> case ramp; share_T at the five points [0.98442, 0.98454, 0.98468, 0.98481, 0.98498]
    K on share_T: -1.372e-05 [-6.174e-05, 2.744e-05] vs m=0.000165: equivalent
    P on share_T: 3.43e-05 [1.715e-05, 5.488e-05] vs m=0.000165: equivalent
    J on share_T: 0.0005626 [0.0005283, 0.0005969] vs m=0.000165: shown larger


### q8
#### Q3b-8 brake cost (S3x against S3, T = 0.4, H = inf)

  M-I2   S3x - S3 share_by_0p4     0 [0, 0] vs m=0.0303: equivalent | S3 0.3933 S3x 0.3933
  M-I2   S3x - S3 share_by_2       0 [0, 0] vs m=8.23e-05: equivalent | S3 0.3999 S3x 0.3999
  M-I2   S3x - S3 dh_p50           0 [0, 0] vs m=12.7: equivalent | S3 inf S3x inf
  M-I2   S3x - S3 dh_p99           0 [0, 0] vs m=12.6: equivalent | S3 inf S3x inf
  M-I2   S3x - S3 pause_time       -0.609 [-0.6184, -0.6007] vs m=1: equivalent | S3 1.323 S3x 0.7141
  M-I2   S3x - S3 resume_delay     nan [inf-seeds] vs m=12.8: n/a | S3 nan S3x nan
  M-I2   S3x - S3 intake_expired   -5242 [-5243, -5241] vs m=1: shown smaller | S3 5242 S3x 0
  M-I2   S3x - S3 intake_expired_deq 0 [0, 0] vs m=1: equivalent | S3 0 S3x 0
    bound for S3's resume delay: R_mean 0.0208 + P 0.025 + 2 RTT 0.020 = 0.0658 s ; S3 resume_delay per seed (s): ['nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan']
    S3x resume_delay per seed (s): ['nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan']
  M-L10  S3x - S3 share_by_0p4     0 [0, 0] vs m=0.0412: equivalent | S3 0.1845 S3x 0.1845
  M-L10  S3x - S3 share_by_2       -0.0007341 [-0.002202, 0] vs m=0.134: equivalent | S3 0.3908 S3x 0.39
  M-L10  S3x - S3 dh_p50           0 [0, 0] vs m=56.9: equivalent | S3 inf S3x inf
  M-L10  S3x - S3 dh_p99           0 [0, 0] vs m=44.3: equivalent | S3 inf S3x inf
  M-L10  S3x - S3 pause_time       -0.7987 [-0.8251, -0.7757] vs m=1: equivalent | S3 1.478 S3x 0.6798
  M-L10  S3x - S3 resume_delay     nan [inf-seeds] vs m=48.7: n/a | S3 nan S3x nan
  M-L10  S3x - S3 intake_expired   -5352 [-5356, -5348] vs m=1: shown smaller | S3 5352 S3x 0
  M-L10  S3x - S3 intake_expired_deq 0 [0, 0] vs m=1: equivalent | S3 0 S3x 0
    bound for S3's resume delay: R_mean 0.1141 + P 0.100 + 2 RTT 0.200 = 0.4141 s ; S3 resume_delay per seed (s): ['nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan']
    S3x resume_delay per seed (s): ['nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan', 'nan']
  CL     S3x - S3 share_by_0p4     -0.01211 [-0.0178, -0.006658] vs m=0.0401: equivalent | S3 0.8465 S3x 0.8344
  CL     S3x - S3 share_by_2       0.03024 [0.02991, 0.03064] vs m=0.0396: equivalent | S3 0.8476 S3x 0.8779
  CL     S3x - S3 dh_p50           2.66 [2.218, 3.086] vs m=12.2: equivalent | S3 17 S3x 19.66
  CL     S3x - S3 dh_p99           0 [0, 0] vs m=58.1: equivalent | S3 inf S3x inf
  CL     S3x - S3 pause_time       1.141 [1.135, 1.147] vs m=1: shown larger | S3 1.424 S3x 2.564
  CL     S3x - S3 resume_delay     -23.33 [-26.84, -20.35] vs m=10.3: shown smaller | S3 23.37 S3x 0.04693
  CL     S3x - S3 intake_expired   -1.024e+04 [-1.027e+04, -1.021e+04] vs m=1: shown smaller | S3 1.024e+04 S3x 0
  CL     S3x - S3 intake_expired_deq -3088 [-3094, -3080] vs m=1: shown smaller | S3 3999 S3x 911.1
    bound for S3's resume delay: R_mean 0.0176 + P 0.025 + 2 RTT 0.020 = 0.0626 s ; S3 resume_delay per seed (s): ['0.0177', '0.0222', '0.0361', '0.0233', '0.0172', '0.02', '0.0318', '0.0222', '0.0234', '0.016', '0.024', '0.0265']
    S3x resume_delay per seed (s): ['6.38e-05', '1.48e-05', '5.62e-05', '5.82e-05', '9.37e-05', '3.7e-05', '2.76e-05', '8.37e-06', '7.22e-05', '7.64e-05', '5.92e-06', '4.89e-05']
  OV     S3x - S3 share_by_0p4     -0.0001549 [-0.000341, 3.472e-05] vs m=0.0488: equivalent | S3 0.08655 S3x 0.08639
  OV     S3x - S3 share_by_2       0.001499 [0.00137, 0.001631] vs m=0.0378: equivalent | S3 0.8152 S3x 0.8167
  OV     S3x - S3 dh_p50           2.734 [2.616, 2.842] vs m=1.22: shown larger | S3 488.4 S3x 491.2
  OV     S3x - S3 dh_p99           0 [0, 0] vs m=2.21: equivalent | S3 inf S3x inf
  OV     S3x - S3 pause_time       -0.002 [-0.002667, -0.001333] vs m=1: equivalent | S3 9.912 S3x 9.91
  OV     S3x - S3 resume_delay     nan [inf-seeds] vs m=1: n/a | S3 nan S3x nan
  OV     S3x - S3 intake_expired   0 [0, 0] vs m=1: equivalent | S3 0 S3x 0
  OV     S3x - S3 intake_expired_deq -186.3 [-202, -170.9] vs m=1: shown smaller | S3 1.967e+04 S3x 1.948e+04


### q9
#### Q3b-9 strict stall decomposition (K cells, S3 R, T = 0.4, H = inf)

- K-I2: P = 0.025, owd = 0.005, bound for (i) = P + owd + 5 ms = 0.035 s
   seed  0: longest stall 0.024 s; parts (i, ii, iii) = None
   seed  1: longest stall 0.012 s; parts (i, ii, iii) = (0.01, 0.0, 0.0)
   seed  2: longest stall 0.022 s; parts (i, ii, iii) = None
   seed  3: longest stall 0.023 s; parts (i, ii, iii) = (0.018, 0.0, 0.0)
   seed  4: longest stall 0.032 s; parts (i, ii, iii) = None
   seed  5: longest stall 0.024 s; parts (i, ii, iii) = (0.009, 0.0, 0.0)
   seed  6: longest stall 0.050 s; parts (i, ii, iii) = None
   seed  7: longest stall 0.052 s; parts (i, ii, iii) = (0.011, 0.0, 0.0)
   seed  8: longest stall 0.032 s; parts (i, ii, iii) = (0.007, 0.0, 0.0)
   seed  9: longest stall 0.031 s; parts (i, ii, iii) = (0.009, 0.0, 0.0)
   seed 10: longest stall 0.024 s; parts (i, ii, iii) = None
   seed 11: longest stall 0.031 s; parts (i, ii, iii) = None
   parts defined (an episode open at the first floor after the outage) in 6 of 12 seeds; (i) <= bound in all of them: True (max (i) 0.018 s)
- K-L10: P = 0.100, owd = 0.050, bound for (i) = P + owd + 5 ms = 0.155 s
   seed  0: longest stall 1.523 s; parts (i, ii, iii) = (0.074, 0.0, 0.0)
   seed  1: longest stall 1.559 s; parts (i, ii, iii) = (0.069, 0.0, 0.0)
   seed  2: longest stall 0.501 s; parts (i, ii, iii) = (0.147, 0.127, 0.0)
   seed  3: longest stall 0.540 s; parts (i, ii, iii) = None
   seed  4: longest stall 1.583 s; parts (i, ii, iii) = (0.053, 0.0, 0.0)
   seed  5: longest stall 0.481 s; parts (i, ii, iii) = (0.085, 0.25, 0.0)
   seed  6: longest stall 1.920 s; parts (i, ii, iii) = (0.133, 0.142, 0.016)
   seed  7: longest stall 1.723 s; parts (i, ii, iii) = (0.097, 0.176, 0.0)
   seed  8: longest stall 0.787 s; parts (i, ii, iii) = None
   seed  9: longest stall 1.901 s; parts (i, ii, iii) = (0.073, 0.359, 0.0)
   seed 10: longest stall 1.741 s; parts (i, ii, iii) = (0.101, 0.173, 0.018)
   seed 11: longest stall 1.921 s; parts (i, ii, iii) = (0.093, 0.359, 0.0)
   parts defined (an episode open at the first floor after the outage) in 10 of 12 seeds; (i) <= bound in all of them: True (max (i) 0.147 s)
   (ii) >= 0.5 x (longest stall - 1.5 s): 6 of 10 seeds with parts; failing seeds' (mx, ii): [(1.523, 0.0), (1.559, 0.0), (1.583, 0.0), (1.92, 0.142)]


### aa
#### A/A self-check (frozen nf_stage3b.json)

312 readings over 12 cells: 237 equivalent, 67 undetermined, 2 shown
  shown: M-I2-dh_p50 -4.646 [-8.204, -1.071] vs m=1: shown smaller
  shown: CL-dh_p99 17.25 [6.071, 31.23] vs m=1: shown larger

undetermined (cell-key: mean [lo, hi] vs m):
  E-I2-dh_p99: 17.55 [-7.658, 45.66] vs m=1: undetermined
  E-I2-v_p99: 17.5 [-7.626, 45.55] vs m=1: undetermined
  E-I2-stall_total: -0.2072 [-1.343, 0.9018] vs m=1: undetermined
  E-I2-tail_time: 27.36 [-10.7, 69.99] vs m=1: undetermined
  E-I2-finish: 27.36 [-10.7, 69.99] vs m=1: undetermined
  E-I2-probes: -0.5 [-1.25, 0.25] vs m=1: undetermined
  E-I2n-stall_total: -0.2457 [-2.151, 1.938] vs m=1: undetermined
  E-I2n-tail_time: 2.127 [-7.224, 11.72] vs m=1: undetermined
  E-I2n-finish: 2.127 [-7.224, 11.72] vs m=1: undetermined
  E-I2n-probes: -0.25 [-1.333, 0.8333] vs m=1: undetermined
  E-L10-dh_p99: -3.24 [-35.68, 29.46] vs m=1: undetermined
  E-L10-v_p99: 10.64 [-36.64, 61.04] vs m=1: undetermined
  E-L10-stall_total: 1.342 [-0.2309, 3.049] vs m=1: undetermined
  M-I2-dh_p90: -4.067 [-8.005, 0.2463] vs m=1: undetermined
  M-I2-dh_p99: -4.55 [-8.047, -0.9547] vs m=1: undetermined
  M-I2-v_p99: -4.524 [-8.035, -0.9176] vs m=1: undetermined
  M-I2-stall_total: -1.869 [-7.651, 4.861] vs m=1: undetermined
  M-I2-tail_time: 0.6633 [-2.312, 4.755] vs m=1: undetermined
  M-I2-finish: -3.956 [-9.215, 2.236] vs m=1: undetermined
  M-I2-probes: -1.417 [-2.167, -0.6667] vs m=1: undetermined
  M-I2-resume_delay: -4.02 [-7.495, -0.624] vs m=1: undetermined
  M-L10-share_by_2: 0.001969 [-0.04372, 0.05031] vs m=0.00221: undetermined
  M-L10-dh_p50: 5.998 [-12.71, 23.03] vs m=1: undetermined
  M-L10-dh_p90: 8.641 [-7.694, 24.62] vs m=1: undetermined
  M-L10-dh_p99: 5.257 [-10.84, 20.72] vs m=1: undetermined
  M-L10-v_p50: 3.232 [-1.641, 11.03] vs m=1: undetermined
  M-L10-v_p99: 4.437 [-12.73, 21.15] vs m=1: undetermined
  M-L10-stall_total: 9.154 [-7.676, 25.38] vs m=1: undetermined
  M-L10-tail_time: -0.05942 [-3.023, 2.109] vs m=1: undetermined
  M-L10-finish: 9.154 [-7.676, 25.38] vs m=1: undetermined
  M-L10-resume_delay: 10.15 [-7.088, 27.15] vs m=1: undetermined
  K-I2-stall_total: 2.357 [-1.053, 5.449] vs m=1: undetermined
  K-I2-probes: 2.333 [-4.25, 9] vs m=1: undetermined
  K-L10-dh_p50: -0.4249 [-1.041, 0.2107] vs m=1: undetermined
  K-L10-dh_p99: 7.984 [-0.5696, 24.27] vs m=1: undetermined
  K-L10-v_p50: -0.4868 [-1.083, 0.1368] vs m=1: undetermined
  K-L10-v_p99: 7.983 [-0.5697, 24.27] vs m=1: undetermined
  K-L10-stall_total: -54.79 [-147.9, 36.55] vs m=1: undetermined
  K-L10-probes: 0.5 [-0.08333, 1.333] vs m=1: undetermined
  K-L10-R_mean: -0.3413 [-1.069, 0.3654] vs m=1: undetermined
  K-L10b-dh_p50: -0.4807 [-1.38, 0.6428] vs m=1: undetermined
  K-L10b-dh_p99: -1.696 [-4.953, 0.3113] vs m=1: undetermined
  K-L10b-v_p50: -0.6455 [-1.344, 0.1191] vs m=1: undetermined
  K-L10b-v_p99: -1.68 [-4.904, 0.311] vs m=1: undetermined
  K-L10b-stall_total: -59.32 [-136.7, 14.14] vs m=1: undetermined
  K-L10b-tail_time: -0.3414 [-1.205, 0.2701] vs m=1: undetermined
  K-L10b-finish: -0.3414 [-1.205, 0.2701] vs m=1: undetermined
  K-L10b-R_mean: -0.34 [-1.068, 0.3652] vs m=1: undetermined
  CL-dh_p50: 1.343 [-2.003, 4.481] vs m=1: undetermined
  CL-dh_p90: 1.662 [-1.677, 4.845] vs m=1: undetermined
  CL-v_p99: -6.721 [-24.65, 5.045] vs m=1: undetermined
  CL-stall_total: -5.691 [-27.09, 6.827] vs m=1: undetermined
  CL-tail_time: -6.905 [-24.92, 4.027] vs m=1: undetermined
  CL-finish: -5.425 [-26.45, 7.28] vs m=1: undetermined
  CL-probes: 0.5833 [-0.9167, 2.167] vs m=1: undetermined
  CL-resume_delay: 1.314 [-1.842, 4.169] vs m=1: undetermined
  OV-dh_p99: -0.6515 [-1.234, -0.1735] vs m=1: undetermined
  OV-stall_total: -0.9747 [-3.907, 1.943] vs m=1: undetermined
  OV-tail_time: -1.122 [-4.118, 1.687] vs m=1: undetermined
  OV-finish: -0.9747 [-3.907, 1.943] vs m=1: undetermined
  X15-stall_total: 16.06 [-124.5, 154] vs m=1: undetermined
  X15-probes: 1.917 [-5.752, 9.417] vs m=1: undetermined
  X20-dh_p90: -0.385 [-1.021, 0.2244] vs m=1: undetermined
  X20-dh_p99: 0.129 [-0.9179, 1.812] vs m=1: undetermined
  X20-v_p99: 0.2059 [-0.7887, 1.828] vs m=1: undetermined
  X20-stall_total: -92.76 [-267.5, 78.18] vs m=1: undetermined
  X20-R_mean: -0.3413 [-1.068, 0.3618] vs m=1: undetermined

floors (cell: readout = p95 |A - A'|, ms for times):
  E-I2    share_by_2=0.000119; dh_p50=0.114; dh_p90=0.313; dh_p99=91; v_mean=1.99; v_p50=0.1; v_p99=90.8; w_mean=1.99; repair_per_delivered=0.000202; stall_total=3.92; tail_time=140; finish=140; probes=2.45; R_mean=0.123
  E-I2n   share_by_2=0.000119; dh_p50=0.103; dh_p90=0.254; dh_p99=1.19; v_mean=0.136; v_p50=0.103; v_p99=1.16; w_mean=0.136; repair_per_delivered=0.000284; stall_total=6.64; tail_time=27; finish=27; probes=3.45; R_mean=0.044
  E-L10   share_any=0.000165; share_T=0.000165; share_by_0p4=0.00494; dh_p50=0.524; dh_p90=1.69; dh_p99=104; v_mean=4.67; v_p50=0.578; v_p99=164; w_mean=4.67; repair_per_delivered=7.62e-05; stall_total=6.12; probes=1; R_mean=0.133
  M-I2    share_by_0p4=0.0027; dh_p50=12.7; dh_p90=12.7; dh_p99=12.6; v_mean=5.58; v_p50=0.167; v_p99=12.5; w_mean=9.47; repair_per_delivered=0.00246; stall_total=20.4; tail_time=13.5; finish=17.9; pause_time=0.0127; probes=3.45; R_mean=0.0316; resume_delay=12.8
  M-L10   share_by_0p4=0.000741; share_by_2=0.134; dh_p50=56.9; dh_p90=46.3; dh_p99=44.3; v_mean=25.5; v_p50=23.8; v_p99=48.8; w_mean=42.9; repair_per_delivered=0.00079; stall_total=45.8; tail_time=9.4; finish=45.8; pause_time=0.0433; probes=1; R_mean=0.127; resume_delay=48.7
  K-I2    dh_p99=1.56; v_mean=0.0577; v_p99=1.56; w_mean=0.0577; repair_per_delivered=0.004; stall_total=9.54; probes=16; R_mean=1.92
  K-L10   share_by_0p4=0.00112; dh_p50=2.05; dh_p90=1.85; dh_p99=44.4; v_mean=1.99; v_p50=2.03; v_p99=44.4; w_mean=1.99; repair_per_delivered=0.000633; stall_total=288; tail_time=1.36; finish=1.36; probes=2.35; R_mean=2.27
  K-L10b  share_by_0p4=0.0013; dh_p50=3.28; dh_p90=1.2; dh_p99=9.52; v_mean=0.718; v_p50=2.31; v_p99=9.43; w_mean=0.718; repair_per_delivered=0.000483; stall_total=273; tail_time=2.9; finish=2.9; probes=1.9; R_mean=2.26
  CL      share_by_0p4=0.0035; dh_p50=12.2; dh_p90=12; dh_p99=58.1; v_mean=2.48; v_p50=1.02; v_p99=54.2; w_mean=10.8; repair_per_delivered=0.00437; stall_total=58; tail_time=51; finish=57.4; pause_time=0.0132; probes=4.8; R_mean=0.196; link_util_w=0.00122; queue_drop=438; resume_delay=10.3
  OV      share_by_0p4=0.000605; share_by_2=0.00034; dh_p50=1.22; dh_p90=0.544; dh_p99=2.21; v_mean=0.239; v_p50=0.219; v_p99=1.5; w_mean=0.239; stall_total=11.9; tail_time=11.7; finish=11.9; pause_time=0.00136; R_mean=0.105
  X15     dh_p50=0.000921; dh_p90=1.92; dh_p99=1.12; v_mean=0.288; v_p99=1.12; w_mean=0.288; stall_total=408; tail_time=1.23; finish=1.23; probes=22.4; R_mean=1.92
  X20     dh_p50=0.00566; dh_p90=2.15; dh_p99=4.67; v_mean=0.67; v_p99=4.53; w_mean=0.67; stall_total=549; tail_time=1.27; finish=1.27; probes=2.45; R_mean=2.26


### rep
#### A/A replication, phase variants 2 and 3 (unregistered), read against T_bar only

  SHOWN M-I2 dh_p90: -4.331 [-7.325, -1.422] vs m=1: shown smaller
  312 readings, 1 shown
