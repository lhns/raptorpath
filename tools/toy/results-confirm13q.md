
## OV-long-q (13.9.6) (2026-11-26): the A/A floor

```
{
 "3": {
  "share": 0.00010182648401821925,
  "ot": 0.00010182648401821925,
  "p50": 0.0,
  "recovery": 0.0,
  "wshare": 9.083333333332888e-05,
  "wot": 9.270833333329121e-05
 },
 "6": {
  "share": 9.931506849311655e-05,
  "ot": 9.931506849311655e-05,
  "p50": 0.0,
  "recovery": 0.0,
  "wshare": 8.69791666666675e-05,
  "wot": 8.906249999998672e-05
 },
 "12": {
  "share": 0.0001091324200913013,
  "ot": 0.00010810502283098433,
  "p50": 0.0,
  "recovery": 0.0,
  "wshare": 0.00010312500000001499,
  "wot": 0.00010239583333331302
 }
}
```

## OV-long-q: build B' at delta = 0.118 (13.9.6)


### OV-long-q (build B', 13.3)

Builds: A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + the old wait limit (stored, reported only); B' = A + the 13.3 wait limit (Bp), no send gate. Dominance on the paired steady window (12.2), best finite-T point per build, on the fixed reader (13.6). 

| row | seeds | delivered | on time (whole run) | resends | ov_frac | n_cap_lt_c | onset_bind | onset_t | tov_resets | y_max | w_min_over_c | den_bind | floor_bind | deq_early_would |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=0p4, hold 0 (S5) | 6 | 0.8122 | 0.8121 | 416 | 0.964 | - | - | - | - | - | - | 141 | 0.9132 | 381783 |
| A; lifetime T=0p6, hold 0 (S5) | 6 | 0.8161 | 0.0390 | 725 | 0.959 | - | - | - | - | - | - | 141 | 0.8799 | 374610 |
| A; lifetime T=1, hold 0 (S5) | 6 | 0.8241 | 0.0389 | 1360 | 0.950 | - | - | - | - | - | - | 141 | 0.8134 | 360623 |
| A; lifetime T=2, hold 0 (S5) | 6 | 0.8440 | 0.0389 | 2972 | 0.927 | - | - | - | - | - | - | 0 | 0.6509 | 325815 |
| A; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.0389 | 15782 | 0.781 | - | - | - | - | - | - | 0 | 0.0000 | 0 |
| Bp; lifetime T=0p4, hold 0 (S5) | 6 | 0.8057 | 0.8057 | 11051 | 0.967 | 375335 | 0.093 | 4.76 | 0.0 | 11.23 | 0.134 | 141 | 0.0004 | 0 |
| Bp; lifetime T=0p6, hold 0 (S5) | 6 | 0.8070 | 0.6453 | 12524 | 0.967 | 374579 | 0.134 | 6.46 | 0.0 | 7.67 | 0.191 | 141 | 0.0004 | 0 |
| Bp; lifetime T=1, hold 0 (S5) | 6 | 0.8113 | 0.1744 | 12635 | 0.963 | 375066 | 0.221 | 9.84 | 1.0 | 4.63 | 0.293 | 141 | 0.0004 | 0 |
| Bp; lifetime T=2, hold 0 (S5) | 6 | 0.8281 | 0.0389 | 13071 | 0.943 | 374912 | 0.439 | 18.32 | 1.0 | 2.31 | 0.494 | 0 | 0.0000 | 0 |
| Bp; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.0389 | 15782 | 0.781 | 0 | 1.000 | - | 1.0 | 0.00 | 0.800 | 0 | 0.0000 | 0 |
| rt; today's Realtime, delta=0p118 (E-rt) | 3 | 0.8107 | 0.8100 | 44 | 0.000 | - | - | - | - | - | - | 0 | 0.0000 | 0 |
| rteq; today's Realtime, delta=0p118 (E-rt) (engine queue) | 3 | 0.8246 | 0.0412 | 13 | 0.000 | - | - | - | - | - | - | 0 | 0.0000 | 0 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 3 | 0.9973 | 0.0315 | 14471 | 0.000 | - | - | - | - | - | - | 0 | 0.0000 | 0 |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 3 | 0.9974 | 0.0348 | 14469 | 0.000 | - | - | - | - | - | - | 0 | 0.0000 | 0 |
| bulk; never gives up, strict order = today's Bulk (C3) | 3 | 1.0000 | 0.0273 | 15761 | 0.000 | - | - | - | - | - | - | 0 | 0.0000 | 0 |

Dominance (12.3). The verdict is B'; A and B are reported only.

| build | baseline | best point | share | on time | p50 | outcome |
|---|---|---|---|---|---|---|
| A | rt; today's Realtime, delta=0p118 (E-rt) | T=0p4 | -0.00346 [-0.00359, -0.00332] | -0.00346 [-0.0036, -0.00332] | -0.217 [-0.217, -0.217] | worse |
| A | rteq; today's Realtime, delta=0p118 (E-rt) (engine queue) | T=0p4 | -0.00434 [-0.00444, -0.00428] | 0.805 [0.805, 0.805] | 0.34 [0.34, 0.34] | dominates |
| A | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=0p4 | -0.193 [-0.193, -0.192] | 0.805 [0.805, 0.805] | 5.33 [5.33, 5.33] | not dominated (a trade) |
| A | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=0p4 | -0.193 [-0.193, -0.192] | 0.805 [0.805, 0.805] | 4.74 [4.74, 4.74] | not dominated (a trade) |
| A | bulk; never gives up, strict order = today's Bulk (C3) | T=0p4 | -0.195 [-0.195, -0.195] | 0.805 [0.805, 0.805] | 4.96 [4.95, 5] | not dominated (a trade) |
| B' | rt; today's Realtime, delta=0p118 (E-rt) | T=0p4 | -0.00917 [-0.00928, -0.009] | -0.00919 [-0.00928, -0.00902] | -0.0124 [-0.0124, -0.0124] | worse |
| B' | rteq; today's Realtime, delta=0p118 (E-rt) (engine queue) | T=0p4 | -0.00993 [-0.00998, -0.00986] | 0.799 [0.799, 0.799] | 0.55 [0.55, 0.55] | not dominated (a trade) |
| B' | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=0p4 | -0.198 [-0.199, -0.198] | 0.799 [0.799, 0.799] | 5.54 [5.54, 5.54] | not dominated (a trade) |
| B' | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=0p4 | -0.198 [-0.199, -0.198] | 0.799 [0.799, 0.799] | 4.94 [4.94, 4.94] | not dominated (a trade) |
| B' | bulk; never gives up, strict order = today's Bulk (C3) | T=0p4 | -0.201 [-0.201, -0.201] | 0.799 [0.799, 0.799] | 5.17 [5.15, 5.2] | not dominated (a trade) |

B' against A, paired steady window (reported only):

| T | readout | B' - A [95 %] | bar |
|---|---|---|---|
| 0p4 | steady share | -0.00579 [-0.00587, -0.00571] (positive favours B') | 0.00977 |
| 0p4 | steady ot | -0.00581 [-0.00589, -0.00573] (positive favours B') | 0.00977 |
| 0p4 | steady p50 | 0.205 [0.205, 0.205] (positive favours B') | 0.0195 |
| 0p6 | steady share | -0.00842 [-0.00849, -0.00837] (positive favours B') | 0.00977 |
| 0p6 | steady ot | 0.672 [0.671, 0.673] (positive favours B') | 0.05 |
| 0p6 | steady p50 | 0.297 [0.297, 0.298] (positive favours B') | 0.0296 |
| 1 | steady share | -0.0126 [-0.0128, -0.0125] (positive favours B') | 0.00977 |
| 1 | steady ot | 0.151 [0.149, 0.153] (positive favours B') | 0.05 |
| 1 | steady p50 | 0.423 [0.421, 0.424] (positive favours B') | 0.0496 |
| 2 | steady share | -0.0156 [-0.0159, -0.0153] (positive favours B') | 0.00977 |
| 2 | steady ot | 0 [0, 0] (positive favours B') | 0.05 |
| 2 | steady p50 | 0.553 [0.548, 0.558] (positive favours B') | 0.0996 |

The bill (whole-run delivered units; 13.1's fluid bill mu_hat (c - x_hat w_end)+ with w_end the last overload window's cap):

| T | A - B' (units) | B' - B (units) | predicted mu_hat (c - x_hat w_end)+ | observed / predicted |
|---|---|---|---|---|
| 0p4 | 3072 | - | 2399 | 1.28 |
| 0p6 | 4394 | - | 3683 | 1.19 |
| 1 | 6137 | - | 5556 | 1.10 |
| 2 | 7626 | - | 7159 | 1.07 |

Registered predictions of B' (13.9.6, written before any run):

| T | s_b | onset_t | pre-onset windows | item 2 deficit | item 3 statistic | tail windows | steady share | on time (0.4 s) | p50, A pairing | A - B' whole run |
|---|---|---|---|---|---|---|---|---|---|---|
| 0p4 | 3.50 | 4.78 | 3 | 0.0 | 0.021 | 62 | 0.799 | = share | 0.184 s | 3070 |
| 0p6 | 5.20 | 6.48 | 4 | 0.0 | 0.028 | 55 | 0.797 | 0.67 | 0.292 s | 4340 |
| 1 | 8.59 | 9.87 | 7 | 0.0018 | 0.032 | 42 | 0.792 | 0.14 | 0.578 s | 6190 |
| 2 | 17.10 | 18.30 | 14 | 0.0018 | 0.035 | 8 | 0.787 | ~ 0 | 1.510 s | 7740 |

The corrected tests (13.9.5) and the registered predictions, per lifetime (item 2's bar is the p95 of |A - A'| over the same windows from the cell's own A/A pair, 12 seeds, at the comparison's seed count):

T=0p4:
- item 1, onset: s_b = 3.50 s (seed mean of 6; registered 3.50; law on the registered inputs 3.50), onset_t = 4.76 s (registered 4.78): inside +-8 %
- item 2, before the onset (3 windows from 2.9 s, registered 3): cap x_hat / c within 0.0 % of 1 (bar 3 %); deficit against A 0.0010, kappa_r x extra resends / handed 0.0000 (per-window gauge), excess +0.0010 against the cell's own A/A floor 0.0004 (registered deficit 0.0): OUTSIDE
- item 3, after the onset: highest 3-window mean of the deficit 0.0214 against the law's value of that statistic 0.0213 (law + resend term; mu 9701, lambda 12000, x_hat 1.237, c 0.296, t0 1.26), ratio 1.00 (bar +-15 %), registered 0.021: holds
- item 4, tail: w x t_ov against 2 (c / x_bar) s_b y / (1 + y) over 62 windows with t_ov >= 2 s_b (registered 62): median |error| 0.01 %, max 0.01 % (bar 3 %): holds
- steady share (A pairing) 0.7989 (registered 0.799 +- 0.002: inside); on time 0.7989 (registered = share); p50 0.185 s (reader's convention; registered 0.184 +- 0.015: inside; the model on the measured share 0.184 s)
- the bill: A - B' = 3072 units; mu_hat (c - x_hat w_end)+ = 2399, kappa_r x extra resends (10635) = 691, sum 3090 (registered 3070), difference -18 (bar +-250): holds
T=0p6:
- item 1, onset: s_b = 5.20 s (seed mean of 6; registered 5.20; law on the registered inputs 5.20), onset_t = 6.46 s (registered 6.48): inside +-8 %
- item 2, before the onset (4 windows from 3.9 s, registered 4): cap x_hat / c within 0.0 % of 1 (bar 3 %); deficit against A 0.0008, kappa_r x extra resends / handed 0.0016 (per-window gauge), excess -0.0008 against the cell's own A/A floor 0.0003 (registered deficit 0.0): OUTSIDE
- item 3, after the onset: highest 3-window mean of the deficit 0.0262 against the law's value of that statistic 0.0277 (law + resend term; mu 9692, lambda 12000, x_hat 1.238, c 0.496, t0 1.26), ratio 0.95 (bar +-15 %), registered 0.028: holds
- item 4, tail: w x t_ov against 2 (c / x_bar) s_b y / (1 + y) over 56 windows with t_ov >= 2 s_b (registered 55): median |error| 0.00 %, max 0.01 % (bar 3 %): holds
- steady share (A pairing) 0.7962 (registered 0.797 +- 0.002: inside); on time 0.6716 (registered 0.67); p50 0.294 s (reader's convention; registered 0.292 +- 0.015: inside; the model on the measured share 0.293 s)
- the bill: A - B' = 4394 units; mu_hat (c - x_hat w_end)+ = 3683, kappa_r x extra resends (11798) = 767, sum 4450 (registered 4340), difference -56 (bar +-250): holds
T=1:
- item 1, onset: s_b = 8.59 s (seed mean of 6; registered 8.59; law on the registered inputs 8.59), onset_t = 9.84 s (registered 9.87): inside +-8 %
- item 2, before the onset (7 windows from 6.0 s, registered 7): cap x_hat / c within 0.0 % of 1 (bar 3 %); deficit against A 0.0047, kappa_r x extra resends / handed 0.0016 (per-window gauge), excess +0.0031 against the cell's own A/A floor 0.0002 (registered deficit 0.0018): OUTSIDE
- item 3, after the onset: highest 3-window mean of the deficit 0.0297 against the law's value of that statistic 0.0313 (law + resend term; mu 9692, lambda 12000, x_hat 1.238, c 0.896, t0 1.25), ratio 0.95 (bar +-15 %), registered 0.032: holds
- item 4, tail: w x t_ov against 2 (c / x_bar) s_b y / (1 + y) over 42 windows with t_ov >= 2 s_b (registered 42): median |error| 0.00 %, max 0.00 % (bar 3 %): holds
- steady share (A pairing) 0.7920 (registered 0.792 +- 0.002: inside); on time 0.1510 (registered 0.14); p50 0.570 s (reader's convention; registered 0.578 +- 0.03: inside; the model on the measured share 0.583 s)
- the bill: A - B' = 6137 units; mu_hat (c - x_hat w_end)+ = 5556, kappa_r x extra resends (11275) = 733, sum 6289 (registered 6190), difference -152 (bar +-250): holds
T=2:
- item 1, onset: s_b = 17.07 s (seed mean of 6; registered 17.10; law on the registered inputs 17.06), onset_t = 18.32 s (registered 18.30): inside +-8 %
- item 2, before the onset (13 windows from 11.0 s, registered 14): cap x_hat / c within 0.0 % of 1 (bar 3 %); deficit against A 0.0077, kappa_r x extra resends / handed 0.0017 (per-window gauge), excess +0.0060 against the cell's own A/A floor 0.0001 (registered deficit 0.0018): OUTSIDE
- item 3, after the onset: highest 3-window mean of the deficit 0.0333 against the law's value of that statistic 0.0347 (law + resend term; mu 9691, lambda 12000, x_hat 1.238, c 1.896, t0 1.25), ratio 0.96 (bar +-15 %), registered 0.035: holds
- item 4, tail: w x t_ov against 2 (c / x_bar) s_b y / (1 + y) over 8 windows with t_ov >= 2 s_b (registered 8): median |error| 0.00 %, max 0.00 % (bar 3 %): holds
- steady share (A pairing) 0.7891 (registered 0.787 +- 0.002: OUTSIDE); on time 0.0000 (registered ~ 0); p50 1.439 s (reader's convention; registered 1.51 +- 0.1: inside; the model on the measured share 1.508 s)
- the bill: A - B' = 7626 units; mu_hat (c - x_hat w_end)+ = 7159, kappa_r x extra resends (10100) = 656, sum 7816 (registered 7740), difference -190 (bar +-250): holds
item 1, linearity: s_b(T = 2) / s_b(T = 0.4) = 4.88 against 4.87 +- 10 %: holds

T = inf (registered: B' = A, bill 0): 6 of 6 seeds identical to A on every id3e key; whole-run A - B' = 0 units

The cell confirms the corrected tests if items 1, 3 and 4 hold at every T, item 2 is within its floor at every T, and the bill and p50 are in their bands: OUTSIDE: item2@0p4, item2@0p6, item2@1, item2@2
The residual's test is item 2 at all four T: a reading of 0.0018 within the floor at T = 1 and 2 means OV-long-m's excess was specific to that cell; about 0.0035-0.005 means the delta-specific excess is real and grows with c / x (it goes into the derivation owed in 13.9.5); at T = 0.4 and 0.6 a reading like OV-long-m's 0.0013 is the unprotected excess recurring.

Registered reading (13.9.6) against the outcome:

- against the stored arm: best point T=0p4 (registered 0.4); share -0.0092, on time -0.0092, p50 +0.012 s; word: worse. Registered: share -0.009 +- 0.002, p50 +0.012 +- 0.015 s; predicted 'worse' (or 'equivalent' at the low end of both bands)
- against the engine-queue arm: best point T=0p4 (registered 0.4); share -0.0099, on time +0.7991, p50 -0.550 s; word: not dominated (a trade). Registered: share -0.010, p50 -0.55 s, on time +0.80; predicted 'a trade'

## RESEND-GAUGE (reported only): the stored A and B' rows of OV-long-v and OV-long-m rerun with the resend gauge on, 3 seeds

Every resend is classed once: filled (a hole), dup (the unit was already received), lost (random or outage loss, or a bottleneck-queue drop: qdrop is the queue's share), late (arrived after its hole was passed; floor is the floor's share), other (the unit was not yet a hole: ahead of the receiver's highest seq), open (still in flight at the end). The classes sum to the resend count (the gauge's own n, equal to the stored `repair_sends`).

Identity of each rerun with its stored run (every id3e key; the gauge adds only its own block): 48 of 48 runs compared (a stored run missing from the store is not compared): identical

| cell | T | build | seeds | resends | filled | dup | lost (qdrop) | late (floor) | other | open | filled / n | dup / n | lost / n | late / n | repairs sent | never delivered |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| OV-long-v | 1 | A | 3 | 1519 | 1292 | 85 | 142 (0) | 0 (0) | 0 | 0.0 | 0.850 | 0.056 | 0.094 | 0.000 | 0 | 10995 |
| OV-long-v | 1 | B' | 3 | 12038 | 10797 | 85 | 1156 (0) | 0 (0) | 0 | 0.0 | 0.897 | 0.007 | 0.096 | 0.000 | 0 | 1189 |
| OV-long-v | 1p5 | A | 3 | 2346 | 2030 | 85 | 231 (0) | 0 (0) | 0 | 0.0 | 0.865 | 0.036 | 0.099 | 0.000 | 0 | 10377 |
| OV-long-v | 1p5 | B' | 3 | 13184 | 11825 | 85 | 1273 (0) | 0 (0) | 0 | 0.0 | 0.897 | 0.006 | 0.097 | 0.000 | 0 | 279 |
| OV-long-v | 2 | A | 3 | 3173 | 2781 | 85 | 307 (0) | 0 (0) | 0 | 0.0 | 0.876 | 0.027 | 0.097 | 0.000 | 0 | 9760 |
| OV-long-v | 2 | B' | 3 | 13439 | 12057 | 85 | 1297 (0) | 0 (0) | 0 | 0.0 | 0.897 | 0.006 | 0.097 | 0.000 | 0 | 171 |
| OV-long-v | 3 | A | 3 | 4871 | 4317 | 85 | 468 (0) | 0 (0) | 0 | 0.0 | 0.886 | 0.017 | 0.096 | 0.000 | 0 | 8487 |
| OV-long-v | 3 | B' | 3 | 13733 | 12320 | 85 | 1328 (0) | 0 (0) | 0 | 0.0 | 0.897 | 0.006 | 0.097 | 0.000 | 0 | 201 |
| OV-long-m | 0p4 | A | 3 | 413 | 256 | 122 | 36 (0) | 0 (0) | 0 | 0.0 | 0.619 | 0.294 | 0.086 | 0.000 | 2281 | 11293 |
| OV-long-m | 0p4 | B' | 3 | 10679 | 8356 | 1299 | 1024 (0) | 0 (0) | 0 | 0.0 | 0.782 | 0.122 | 0.096 | 0.000 | 2074 | 1746 |
| OV-long-m | 0p6 | A | 3 | 707 | 481 | 159 | 66 (0) | 0 (0) | 0 | 0.0 | 0.681 | 0.225 | 0.094 | 0.000 | 2284 | 11080 |
| OV-long-m | 0p6 | B' | 3 | 12436 | 9709 | 1523 | 1204 (0) | 0 (0) | 0 | 0.0 | 0.781 | 0.122 | 0.097 | 0.000 | 2017 | 131 |
| OV-long-m | 1 | A | 3 | 1356 | 986 | 245 | 125 (0) | 0 (0) | 0 | 0.0 | 0.727 | 0.180 | 0.092 | 0.000 | 2288 | 10572 |
| OV-long-m | 1 | B' | 3 | 12557 | 9793 | 1551 | 1213 (0) | 0 (0) | 0 | 0.0 | 0.780 | 0.124 | 0.097 | 0.000 | 2020 | 104 |
| OV-long-m | 2 | A | 3 | 2956 | 2225 | 444 | 287 (0) | 0 (0) | 0 | 0.0 | 0.753 | 0.150 | 0.097 | 0.000 | 2302 | 9400 |
| OV-long-m | 2 | B' | 3 | 13074 | 10124 | 1689 | 1260 (0) | 0 (0) | 0 | 0.0 | 0.774 | 0.129 | 0.096 | 0.000 | 2027 | 1 |

B' minus A (seed means over the seeds both have), per extra resend; the fill fraction is what 13.9.3 reads as 0.93 (Delta never_delivered / Delta resends):

| cell | T | extra resends | extra filled | extra dup | extra lost | extra late | extra other | extra repairs | unfilled per extra resend | A - B' never_delivered / extra resends |
|---|---|---|---|---|---|---|---|---|---|---|
| OV-long-m | 0p4 | 10266 | 8100 | 1177 | 989 | 0 | 0 | -206 | 0.211 | 0.930 |
| OV-long-m | 0p6 | 11729 | 9228 | 1363 | 1137 | 0 | 0 | -267 | 0.213 | 0.933 |
| OV-long-m | 1 | 11201 | 8807 | 1307 | 1088 | 0 | 0 | -268 | 0.214 | 0.934 |
| OV-long-m | 2 | 10117 | 7899 | 1245 | 973 | 0 | 0 | -276 | 0.219 | 0.929 |
| OV-long-v | 1 | 10519 | 9505 | 0 | 1013 | 0 | 0 | 0 | 0.096 | 0.932 |
| OV-long-v | 1p5 | 10837 | 9795 | 0 | 1042 | 0 | 0 | 0 | 0.096 | 0.932 |
| OV-long-v | 2 | 10266 | 9277 | 0 | 990 | 0 | 0 | 0 | 0.096 | 0.934 |
| OV-long-v | 3 | 8862 | 8003 | 0 | 859 | 0 | 0 | 0 | 0.097 | 0.935 |

Per-second series of OV-long-m at T=0p4, A and B' side by side, seed means over 1 s (resends sent, duplicate arrivals, repairs sent): locates 13.9.3's pre-onset excess (registered onset_t 5.3 s)

| t (s) | resends A | resends B' | dup A | dup B' | repairs A | repairs B' |
|---|---|---|---|---|---|---|
| 0 | 278 | 278 | 101 | 101 | 198 | 198 |
| 1 | 135 | 135 | 20 | 20 | 61 | 61 |
| 2 | 0 | 0 | 0 | 0 | 53 | 53 |
| 3 | 0 | 0 | 0 | 0 | 53 | 53 |
| 4 | 0 | 0 | 0 | 0 | 53 | 53 |
| 5 | 0 | 0 | 0 | 0 | 53 | 53 |
| 6 | 0 | 9 | 0 | 0 | 53 | 59 |
| 7 | 0 | 297 | 0 | 31 | 53 | 53 |
| 8 | 0 | 280 | 0 | 30 | 53 | 49 |
| 9 | 0 | 281 | 0 | 29 | 53 | 50 |
| 10 | 0 | 304 | 0 | 26 | 53 | 50 |
| 11 | 0 | 294 | 0 | 28 | 53 | 49 |
| 12 | 0 | 295 | 0 | 26 | 53 | 50 |
| 13 | 0 | 303 | 0 | 31 | 53 | 50 |
| 14 | 0 | 299 | 0 | 27 | 53 | 49 |
| 15 | 0 | 285 | 0 | 26 | 53 | 50 |
| 16 | 0 | 311 | 0 | 33 | 53 | 47 |
| 17 | 0 | 321 | 0 | 38 | 53 | 45 |
| 18 | 0 | 341 | 0 | 38 | 53 | 46 |
| 19 | 0 | 324 | 0 | 40 | 53 | 46 |
| 20 | 0 | 304 | 0 | 37 | 53 | 45 |
| 21 | 0 | 344 | 0 | 37 | 53 | 45 |
| 22 | 0 | 307 | 0 | 37 | 53 | 46 |
| 23 | 0 | 322 | 0 | 39 | 53 | 46 |
| 24 | 0 | 321 | 0 | 35 | 53 | 45 |
| 25 | 0 | 317 | 0 | 40 | 53 | 46 |
| 26 | 0 | 315 | 0 | 39 | 53 | 45 |
| 27 | 0 | 297 | 0 | 40 | 53 | 46 |
| 28 | 0 | 300 | 0 | 39 | 53 | 46 |
| 29 | 0 | 324 | 0 | 37 | 53 | 46 |
| 30 | 0 | 320 | 0 | 40 | 53 | 46 |
| 31 | 0 | 317 | 0 | 37 | 53 | 45 |
| 32 | 0 | 315 | 0 | 39 | 53 | 46 |
| 33 | 0 | 319 | 0 | 35 | 53 | 45 |
| 34 | 0 | 298 | 0 | 41 | 53 | 47 |
| 35 | 0 | 304 | 0 | 38 | 53 | 46 |
| 36 | 0 | 313 | 0 | 39 | 53 | 45 |
| 37 | 0 | 322 | 0 | 39 | 53 | 46 |
| 38 | 0 | 317 | 0 | 37 | 53 | 45 |
| 39 | 0 | 298 | 0 | 37 | 53 | 45 |

Per-second series of OV-long-m at T=2, A and B' side by side, seed means over 1 s (resends sent, duplicate arrivals, repairs sent): locates 13.9.3's pre-onset excess (registered onset_t 21.3 s)

| t (s) | resends A | resends B' | dup A | dup B' | repairs A | repairs B' |
|---|---|---|---|---|---|---|
| 0 | 290 | 290 | 109 | 109 | 196 | 196 |
| 1 | 300 | 300 | 40 | 40 | 46 | 46 |
| 2 | 289 | 289 | 38 | 38 | 45 | 45 |
| 3 | 317 | 317 | 38 | 38 | 45 | 45 |
| 4 | 326 | 326 | 37 | 37 | 46 | 46 |
| 5 | 318 | 318 | 40 | 40 | 45 | 45 |
| 6 | 307 | 307 | 41 | 41 | 46 | 46 |
| 7 | 323 | 323 | 39 | 39 | 45 | 45 |
| 8 | 298 | 298 | 40 | 40 | 45 | 45 |
| 9 | 188 | 315 | 23 | 42 | 59 | 47 |
| 10 | 0 | 322 | 0 | 39 | 53 | 46 |
| 11 | 0 | 318 | 0 | 40 | 53 | 46 |
| 12 | 0 | 319 | 0 | 40 | 53 | 45 |
| 13 | 0 | 327 | 0 | 39 | 53 | 45 |
| 14 | 0 | 333 | 0 | 40 | 53 | 46 |
| 15 | 0 | 304 | 0 | 40 | 53 | 45 |
| 16 | 0 | 334 | 0 | 37 | 53 | 45 |
| 17 | 0 | 314 | 0 | 35 | 53 | 46 |
| 18 | 0 | 334 | 0 | 40 | 53 | 46 |
| 19 | 0 | 327 | 0 | 36 | 53 | 45 |
| 20 | 0 | 298 | 0 | 37 | 53 | 45 |
| 21 | 0 | 350 | 0 | 39 | 53 | 45 |
| 22 | 0 | 302 | 0 | 37 | 53 | 46 |
| 23 | 0 | 330 | 0 | 39 | 53 | 45 |
| 24 | 0 | 328 | 0 | 37 | 53 | 44 |
| 25 | 0 | 308 | 0 | 40 | 53 | 46 |
| 26 | 0 | 320 | 0 | 38 | 53 | 45 |
| 27 | 0 | 308 | 0 | 39 | 53 | 44 |
| 28 | 0 | 304 | 0 | 43 | 53 | 46 |
| 29 | 0 | 316 | 0 | 37 | 53 | 46 |
| 30 | 0 | 321 | 0 | 38 | 53 | 45 |
| 31 | 0 | 316 | 0 | 41 | 53 | 46 |
| 32 | 0 | 316 | 0 | 38 | 53 | 45 |
| 33 | 0 | 327 | 0 | 37 | 53 | 44 |
| 34 | 0 | 304 | 0 | 43 | 53 | 46 |
| 35 | 0 | 306 | 0 | 42 | 53 | 47 |
| 36 | 0 | 304 | 0 | 39 | 52 | 46 |
| 37 | 0 | 316 | 0 | 40 | 53 | 45 |
| 38 | 0 | 331 | 0 | 38 | 53 | 44 |
| 39 | 0 | 305 | 0 | 38 | 53 | 45 |
