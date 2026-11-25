# Offline simulation, stage 3c: the protected floor, feedback-only verdicts and the engine's resend budget

> **Amendment 2026-11-25 (superseded stage): RTT sample from a never-arrived seq.** The stage 3c code carried the same bug as stage 3d (see the amendment at the top of `results-stage3d.md`). This stage is superseded and was not re-run at its registered seeds. Below: the runs of the identity check (stage 3c configurations, seeds 0 to 2, new code against the code that produced this file) that took a skipped RTT sample, from the `rtt_unarrived` counter. Every run that differs from the old code is such a run; none with counter 0 differs. This file's numbers for those cells are not exact and were not recomputed.
>
> | cell | runs checked (3 seeds) | runs with a skipped RTT sample | skipped samples |
> |---|---|---|---|
> | CL | 90 | 45 | 45 |
> | E-I2 | 63 | 0 | 0 |
> | E-I2n | 73 | 0 | 0 |
> | E-L10 | 63 | 0 | 0 |
> | K-I2 | 63 | 0 | 0 |
> | K-L10 | 108 | 1 | 1 |
> | K-L10b | 105 | 0 | 0 |
> | M-I2 | 63 | 0 | 0 |
> | M-L10 | 63 | 0 | 0 |
> | OV | 90 | 16 | 56 |
> | X15 | 69 | 23 | 98 |
> | X20 | 69 | 1 | 7 |
> | total | 919 | 86 | 207 |
>
> S4 commit_viol under the fix, on the configurations at hand: the 3 reference runs of stage 3d (K-I2, K-L10, K-L10b, S4 at T = 0.4, H = inf, 12 seeds each) read 0 old and 0 new (no skipped sample in any of them). The unit-test configuration (CL, small, seed 1, S4 at T = 0.4, H = inf) reads 0 old and 1 new, so Q3c-2's 143 violations in 28 runs is not a ceiling for S4 under a correct srtt in the link cells; S5, which re-checks the floor, reads 0 on the same cell and seed. Stage 3c's CL (45 of 90 identity runs) and OV (16 of 90) rows are affected; Q3c-3, Q3c-4 and Q3c-6 are the readings to re-check.

Spec: `docs/derivations/three-knobs.md` section 15 "Revision 2", registration 15.6 (branch `github/three-knobs`, head `c3616d96`). Code: `tools/toy/` (`toy2.py` extended; `batch3c.py`, `summarize3c.py`, `report_s3c.py`, `tables3c.py`, `pc_place3c.py`, `check_ident3c.py`, tests in `test_toy2.py` class `Stage3c`); built on `toy-stage3-knobs` `d0ce4f9d`. Stage 1, 2, 3 and 3b code paths are bit-identical with every new field at its default (below). The offline simulation fixes no constant of the engine, and none of these numbers is engine provenance. 12 seeds per configuration (seeds 0 to 11, clock phases drawn per seed), one path, all runs deterministic. Data lives in `out3c/` (not tracked, like `out3b/`); the frozen A/A floor `nf_stage3c.json` and the two placement files `pc_stage3c.json`, `pc_stage3c_dev.json` are tracked.

## In plain words

- **Everything registered as an identity holds, and every run finishes.** With the new fields at their defaults the code reproduces the stage 3b code (and through it stages 1 to 3) on 102 + 963 runs (3 seeds each): 114 585 old keys, traces and per-sequence arrays as digests, 0 differences. The new sender at lifetime T = infinity equals today's receiver at rho = 1 on 37 readouts in all 12 cells, 12 seeds each (288 of 288 comparisons), and on the sender trace (seeds 0 to 2, all cells); at T = 10^6 s it equals T = infinity. The frontier reaches the last unit in all 3 646 stored runs, at every T, including the link cells.
- **The strict-order collapse under overload is fixed.** In the overload cell (OV) with a lifetime of 0.4 s, the strict receiver now delivers 80.1 % of the units within 0.4 s of hand-over, against 8.7 % for the stage 3b sender (and 83.1 % for the receiver that releases at once). The floor rides on the data; as a datagram of its own (one per 2 ms) it costs 487 link slots per second and 3.4 points (76.7 %), as predicted (5 %). The tell's round trip is gone.
- **But the registered limit at T = 2 s did not hold, in the good direction.** The registration said the floor cannot help strict order at T = 2 s in OV (stage 3b: 75.1 % within 2 s, predicted 65 to 85 %). It reads 90.8 %. The registered band is missed above.
- **The commitment is not absolute.** The registered seam, "no copy of a sequence number is ever sent after the floor passed it", read 143 violations in 28 runs (all in the link cells, 13 of 14 arm groups in OV and one in CL). The cause is an inconsistency inside the registration: the resend gate lets a sequence number that has never been resent go again at once (15.6.1 item 5), while the floor law assumes its earliest resend is one cooldown after its last send (item 1). A supplementary run, labelled unregistered, applies the cooldown to first resends too: violations 0 in all 192 runs; the S4 arms at T = 0.4 move by at most 0.001, but the baselines that resend more move more (C3 in OV `share_by_2` 0.627 to 0.667, C1 in CL `share_by_0p4` +0.036, S4 at T = 2 H = 0 in CL +0.034).
- **The design properties that mattered hold elsewhere.** No savable data is lost in the call cells (shares equal stage 3's within 0.001). With the floor the draining gain is kept in the outage-and-drain cell (CL: 84.8 % within 0.4 s against 19.9 % for never giving up and 28.6 % for today's engine), the engine resend budget holds (at most 10 copies in any 5 ms window, in every one of 528 link-cell runs), and no queue drop falls outside the outage for any arm in CL and OV.
- **Two registered bands for the comparison arms still miss.** Never giving up in CL reads 19.9 % within 0.4 s (band 25 to 47 %), the same as stage 3b: the comparison arm collapses on the link for a reason the band's arithmetic does not model, with or without the resend budget. The positive control for the nudge statistic has no cell: 0 of 12 seeds are usable on the registered placement and 2 of 12 on a corrected one (8 required).

## Readings (Q3c-1 to Q3c-10)

| | reading | key numbers |
|---|---|---|
| Q3c-1 | **PASS** | stages 1 to 3b reproduced: `check_ident3c.py` PASS on the final code (1 065 reference runs regenerated from `d0ce4f9d`; 114 585 old keys, traces and per-sequence arrays as digests). S4 at T = inf equals C3 and S4 at T = 1e6 equals T = inf on 37 keys: 288 of 288 (12 cells x 2 identities x 12 seeds), link cells included; sender traces identical in all 12 cells x 3 seeds x 3 arms. `finished` true in 3 646 of 3 646 runs (3 166 registered, 288 unregistered A/A replication, 192 supplement), every arm and every T, every cell and seed |
| Q3c-2 | **FAIL on the commitment seam; the other three bullets PASS** | `commit_viol` = 143 in 28 of 3 454 runs (OV 9 seeds, CL 1 seed; arms `G-T0p4-*`, `G-T2-*` and the T nudges of OV, and `NT-0p98` of CL); expected 0. `late_floor` = 4 in three OV runs, all of them runs with violations (expected 0 in every one-path cell). `queue_drop_out` = 0 and `queue_drop_in` = 0 in all 528 CL and OV runs (44 arm groups, 12 seeds each). Maximum copies in any 5 ms window: 10 over all CL and OV runs. See "Things that looked wrong", item 1 |
| Q3c-3 | **PASS on 3 of 4 bullets; the H = 0 band is missed by 0.0008** | OV, T = 0.4. S4 H = 0: 0.8308 (band 0.78 to 0.83): FAIL by 0.0008. S4 H = inf: 0.8011 (band 0.75 to 0.83): PASS; at least H = 0 minus 0.03 = 0.8008: PASS by 0.0003 (paired difference -0.0298 [-0.0311, -0.0284]). S4b H = inf: 0.7671, against 0.8011 x 0.95 = 0.7610 +- 0.02 (measured factor 1 - 487.5/10 000 gives 0.7620): PASS; `queue_drop_out` 0. Floor slots on the link per second: S4 38.6 (sweep repeats only), S4b 487.5 (one per 2 ms window); header-carried floors on S4: 9 257 per second |
| Q3c-4 | **FAIL on H = inf, PASS on H = 0** | OV, T = 2. H = inf `share_by_2` 0.9077 (band 0.65 to 0.85; stage 3b 0.751): FAIL, better than predicted. H = 0 0.9396 (band 0.90 to 0.99): PASS. dh p50 1 634 ms (H = inf) and 1 438 ms (H = 0) |
| Q3c-5 | **PASS** | OV. C3 and S4 at T = inf, H = inf: `share_by_0p4` 0.0674 (band 0.05 to 0.20). S4 T = inf, H = 0: 0.1374 (band 0.12 to 0.21). C1: 0.1134 (reported). `link_util_w` over [1 s, 10 s]: 1.0000 minimum over seeds, for every arm: the cell is valid for every arm |
| Q3c-6 | **FAIL: 3 of 5 bullets PASS** | CL. C3 `share_by_0p4` 0.1990 (band 0.25 to 0.47): FAIL; `share_by_2` 0.9716 (>= 0.95): PASS. S4 T = 0.4: 0.8476 (H = 0) and 0.8465 (H = inf), both in [0.78, 0.90], differing by 0.0011 (<= 0.02): PASS. S4 T = 2 against T = inf: equivalent on both shares at H = 0 (+0.0027, +0.0002); at H = inf `share_by_0p4` is equivalent (+0.00002) but `share_by_2` is **shown larger by +0.0194 [0.0170, 0.0220] against a bar of 0.0032**: FAIL. S4 T = 0.4 minus C3 on `share_by_0p4`: +0.6486 (H = 0) and +0.6475 (H = inf), shown larger by more than 0.25: PASS |
| Q3c-7 | **PASS** | S4 T = 0.4, H = inf against stage 3's S2 at the same point, K cells, `share_by_0p4`: K-I2 0.9815 against 0.9814 (paired +0.00006, m = 0.00095); K-L10 0.9789 against 0.9785 (+0.00044, m = 0.00607). `F_lag` (mean over sends of sent minus floor): 1.505 (K-I2) and 8.320 (K-L10); `floor_hull_bind` 0 |
| Q3c-8 | **PASS on the shifts and the crossing form; FAIL on the lowest-T bullet at K-L10b** | mid-level crossings: X15 0.0677 s, X20 0.0825, shift **14.9 ms** (band 15 +- 4); K-L10 0.1726, K-L10b 0.2025, shift **29.9 ms** (band 30 +- 5). Each crossing against the run's own median first-resend age (report-driven, T = 0.4, H = 0) plus srtt/2: X15 0.0678 (-0.1 ms), X20 0.0818 (+0.7), K-L10 0.1718 (+0.7), K-L10b 0.2018 (+0.6): all within 5 ms. At each grid's lowest T, resends are 0 in every seed in X15, X20 and K-L10, but **598 or 599 per seed in K-L10b** (T = 0.180 s): FAIL |
| Q3c-9 | **the control is a finding (not placed); the nudge statistics read equivalent except 7 of 216 undetermined** | T and H nudges, 10 cells (every cell but X15 and X20), 6 readouts (24 of the 240 statistics are skipped because the quantile is infinite at every nudge point), bar max(T bar, A/A floor): 0 shown, 7 undetermined, 209 equivalent. The undetermined: K-L10 `share_by_2` K; CL `share_by_2` K and P, `ooo_per_delivered` K and P; OV `ooo_per_delivered` K and P (all within about one bar of equivalent, listed below). Control in E-I2n: the registered placement gives 0 of 12 usable seeds and the corrected one 2 of 12 (at least 8 required): "not placed", a finding as registered. On the 2 usable seeds (descriptive only) mean K = 0.00181 against K-hat 0.00403 (ratio 0.45) and mean P = 0.00091 against 0.00202 (ratio 0.45), both undetermined against m = 0.00008 |
| Q3c-10 | **bullet 1 PASS, bullet 2 FAIL at K-L10** | K cells, S4 T = 0.4, H = inf. (i) at most 0.013 s at K-I2 (bound 0.035; 1 seed with an outage episode) and 0.024 s at K-L10 (bound 0.155; 8 seeds): PASS. (o) where non-zero: K-I2 none; K-L10 five non-zero values 0.019, 0.020, 0.040, 0.080, 0.179 s against the bound 0.072 s: 3 of 5 within, **seeds 4 (0.0796) and 6 (0.1791) exceed it**: FAIL. C3 reported: no floor is sent, so (i) is the rest of the episode after the outage: 0.12 to 0.40 s in K-L10 (8 seeds) |

## Per-cell tables in plain words

Seed means over 12 seeds. "Delivered at all", "within 0.4 s" and "within 2 s" are fractions of the units handed over (a unit never delivered, or given up, counts as not delivered). Delay is measured from hand-over at the sender (so a unit that waits unsent in the intake or while the source is paused counts that wait); a percentile is "never" when it falls on an undelivered unit (in n of 12 seeds when only some seeds). No mean over delivered units alone is a headline; "resends per delivered" is a count ratio. Arm labels, each with its sender, its receiver and the code name:

- `C1`: today's engine. Sender gives up on a hole when the 1 % allowance (1 - rho at rho = 0.99) permits; receiver passes holes early (the early-skip receiver). In CL and OV it shares the source model, the RFC 6298 RTT estimator, the per-sequence resend cooldown and the 10-copies-per-5-ms resend budget with every arm.
- `C3`: sender never gives up (rho = 1), tail sweep at clamp(2 srtt, 25, 100 ms), resends by its own gate (CL and OV: the cooldown and the budget); receiver releases only in order (a floor passes a hole).
- `C3S`: the new sender S4 with lifetime T = infinity: never gives up, the same old tail sweep, no floor is ever sent; receiver strict. Equal to `C3` by identity in every cell.
- `G-T0p4-H0`, `G-T0p4-Hinf`, `G-T2-H0`, `G-T2-Hinf`, `G-Tinf-H0`: the new sender S4 with lifetime T = 0.4 s, 2 s or never, and receiver order hold H = 0 (release at once, any order) or H = infinity (strict order). S4 gives a unit up when it cannot arrive by T even if resent at the earliest allowed instant (cooldown), announces a floor = the lowest unit it still means to deliver, carried on every data datagram while some unit lies below it (and repeated by the sweep), judges a unit lost only on receiver feedback (never on a timer), expires a unit at the dequeue when it cannot arrive in time and a younger one waits, releases its pause on receiver feedback only, and resends the open tail by the old sweep. The receiver passes a hole once the floor says the sender will not resend it.
- `S4b`: as `G-T0p4-Hinf`, but the floor travels as a datagram of its own, at most one per 2 ms, so it takes link capacity.
- `G-T0p4-H0p02`: S4 at T = 0.4 s, receiver holds a hole 20 ms.

Cells: E = outage at the end of a 12 147-unit object; M = outage in the middle; K = a call (50 units per second, 60 s, 1.5 s outage at 20 s); I2 = 3 % loss, 5 ms path; L10 = 10 % loss, 50 ms path (K-L10b 60 ms); E-I2n = E-I2 with a 50-unit tail; CL = capacity-limited link (12 500 units/s, queue 1 250, offered 10 000/s for 10 s, 2 s to 3.5 s outage); OV = overload (link 10 000/s, offered 12 000/s for 10 s, no outage).

#### E-I2

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip) | 99.0 % | 96.9 % | 99.0 % | 12 / 16 / 1522 | 1.31 | 0.0208 |
| `C3`: never gives up (rho = 1), old tail sweep, resends only per its own gate; receiver strict order | 100.0 % | 97.2 % | 100.0 % | 16 / 28 / 1532 | 1.72 | 0 |
| `C3S`: S4 with lifetime T = infinity: never gives up, old tail sweep, no floor ever sent; receiver strict order (equal to C3 by identity) | 100.0 % | 97.2 % | 100.0 % | 16 / 28 / 1532 | 1.72 | 0 |
| `G-T0p4-H0`: sender gives up on a unit it cannot deliver by 0.4 s (floor = lowest unit it still means to deliver, carried on its data); receiver releases at once, any order (H = 0) | 97.2 % | 97.2 % | 97.2 % | 5.5 / 5.9 / never (inf) | 0.465 | 0.0294 |
| `G-T0p4-Hinf`: sender gives up as above at 0.4 s; receiver strict order (H = inf), so the floor lets it pass holes | 97.2 % | 97.2 % | 97.2 % | 16 / 28 / never (inf) | 0.465 | 0 |
| `G-T2-H0`: sender gives up at 2 s; receiver any order | 100.0 % | 97.2 % | 100.0 % | 5.5 / 5.9 / 1502 | 1.72 | 0.0314 |
| `G-T2-Hinf`: sender gives up at 2 s; receiver strict order | 100.0 % | 97.2 % | 100.0 % | 16 / 28 / 1532 | 1.72 | 0 |
| `G-Tinf-H0`: sender never gives up (old tail sweep); receiver any order | 100.0 % | 97.2 % | 100.0 % | 5.5 / 5.9 / 1502 | 1.72 | 0.0314 |
| `G-T0p4-H0p02`: sender gives up at 0.4 s; receiver holds a hole 20 ms | 97.2 % | 97.2 % | 97.2 % | 16 / 23 / never (inf) | 0.465 | 0.00274 |

#### E-L10

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip) | 99.0 % | 96.9 % | 99.0 % | 150 / 153 / 1802 | 0.546 | 0.0691 |
| `C3`: never gives up (rho = 1), old tail sweep, resends only per its own gate; receiver strict order | 100.0 % | 92.4 % | 99.8 % | 285 / 380 / 1833 | 0.582 | 0 |
| `C3S`: S4 with lifetime T = infinity: never gives up, old tail sweep, no floor ever sent; receiver strict order (equal to C3 by identity) | 100.0 % | 92.4 % | 99.8 % | 285 / 380 / 1833 | 0.582 | 0 |
| `G-T0p4-H0`: sender gives up on a unit it cannot deliver by 0.4 s (floor = lowest unit it still means to deliver, carried on its data); receiver releases at once, any order (H = 0) | 97.1 % | 97.1 % | 97.1 % | 51 / 152 / never (inf) | 0.206 | 0.101 |
| `G-T0p4-Hinf`: sender gives up as above at 0.4 s; receiver strict order (H = inf), so the floor lets it pass holes | 97.1 % | 97.0 % | 97.1 % | 280 / 347 / never (inf) | 0.206 | 0 |
| `G-T2-H0`: sender gives up at 2 s; receiver any order | 100.0 % | 97.1 % | 100.0 % | 51 / 152 / 1592 | 0.582 | 0.101 |
| `G-T2-Hinf`: sender gives up at 2 s; receiver strict order | 100.0 % | 92.4 % | 99.8 % | 285 / 380 / 1833 | 0.582 | 0 |
| `G-Tinf-H0`: sender never gives up (old tail sweep); receiver any order | 100.0 % | 97.1 % | 100.0 % | 51 / 152 / 1592 | 0.582 | 0.101 |
| `G-T0p4-H0p02`: sender gives up at 0.4 s; receiver holds a hole 20 ms | 97.1 % | 97.1 % | 97.1 % | 70 / 152 / never (inf) | 0.206 | 0.101 |

#### M-I2

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip) | 99.3 % | 39.9 % | 99.3 % | 1327 / 1422 / 1520 | 9.26 | 0.0183 |
| `C3`: never gives up (rho = 1), old tail sweep, resends only per its own gate; receiver strict order | 100.0 % | 39.3 % | 100.0 % | 1330 / 1439 / 1527 | 9.45 | 0 |
| `C3S`: S4 with lifetime T = infinity: never gives up, old tail sweep, no floor ever sent; receiver strict order (equal to C3 by identity) | 100.0 % | 39.3 % | 100.0 % | 1330 / 1439 / 1527 | 9.45 | 0 |
| `G-T0p4-H0`: sender gives up on a unit it cannot deliver by 0.4 s (floor = lowest unit it still means to deliver, carried on its data); receiver releases at once, any order (H = 0) | 40.0 % | 40.0 % | 40.0 % | never (inf) / never (inf) / never (inf) | 6.26 | 0.0285 |
| `G-T0p4-Hinf`: sender gives up as above at 0.4 s; receiver strict order (H = inf), so the floor lets it pass holes | 40.0 % | 39.3 % | 40.0 % | never (inf) / never (inf) / never (inf) | 6.26 | 0 |
| `G-T2-H0`: sender gives up at 2 s; receiver any order | 100.0 % | 40.0 % | 100.0 % | 1322 / 1395 / 1504 | 9.45 | 0.0415 |
| `G-T2-Hinf`: sender gives up at 2 s; receiver strict order | 100.0 % | 39.3 % | 100.0 % | 1330 / 1439 / 1527 | 9.45 | 0 |
| `G-Tinf-H0`: sender never gives up (old tail sweep); receiver any order | 100.0 % | 40.0 % | 100.0 % | 1322 / 1395 / 1504 | 9.45 | 0.0415 |
| `G-T0p4-H0p02`: sender gives up at 0.4 s; receiver holds a hole 20 ms | 40.0 % | 40.0 % | 40.0 % | never (inf) / never (inf) / never (inf) | 6.26 | 0.00264 |

#### M-L10

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip) | 98.9 % | 38.9 % | 98.9 % | 1665 / 1776 / 1840 (never in 10 of 12 seeds) | 2.7 | 0.0306 |
| `C3`: never gives up (rho = 1), old tail sweep, resends only per its own gate; receiver strict order | 100.0 % | 17.6 % | 95.6 % | 1765 / 1922 / 1995 | 2.65 | 0 |
| `C3S`: S4 with lifetime T = infinity: never gives up, old tail sweep, no floor ever sent; receiver strict order (equal to C3 by identity) | 100.0 % | 17.6 % | 95.6 % | 1765 / 1922 / 1995 | 2.65 | 0 |
| `G-T0p4-H0`: sender gives up on a unit it cannot deliver by 0.4 s (floor = lowest unit it still means to deliver, carried on its data); receiver releases at once, any order (H = 0) | 39.1 % | 39.1 % | 39.1 % | never (inf) / never (inf) / never (inf) | 1.17 | 0.0795 |
| `G-T0p4-Hinf`: sender gives up as above at 0.4 s; receiver strict order (H = inf), so the floor lets it pass holes | 39.1 % | 19.9 % | 39.1 % | never (inf) / never (inf) / never (inf) | 1.17 | 0 |
| `G-T2-H0`: sender gives up at 2 s; receiver any order | 100.0 % | 39.1 % | 100.0 % | 1512 / 1559 / 1699 | 2.65 | 0.1 |
| `G-T2-Hinf`: sender gives up at 2 s; receiver strict order | 100.0 % | 17.6 % | 98.7 % | 1760 / 1889 / 1959 | 2.65 | 0 |
| `G-Tinf-H0`: sender never gives up (old tail sweep); receiver any order | 100.0 % | 39.1 % | 100.0 % | 1512 / 1559 / 1699 | 2.65 | 0.1 |
| `G-T0p4-H0p02`: sender gives up at 0.4 s; receiver holds a hole 20 ms | 39.1 % | 39.1 % | 39.1 % | never (inf) / never (inf) / never (inf) | 1.17 | 0.0795 |

#### K-I2

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip) | 99.5 % | 97.9 % | 99.5 % | 5.5 / 6.0 / 1084 | 0.647 | 0.024 |
| `C3`: never gives up (rho = 1), old tail sweep, resends only per its own gate; receiver strict order | 100.0 % | 98.1 % | 100.0 % | 5.5 / 6.0 / 924 | 0.777 | 0 |
| `C3S`: S4 with lifetime T = infinity: never gives up, old tail sweep, no floor ever sent; receiver strict order (equal to C3 by identity) | 100.0 % | 98.1 % | 100.0 % | 5.5 / 6.0 / 924 | 0.777 | 0 |
| `G-T0p4-H0`: sender gives up on a unit it cannot deliver by 0.4 s (floor = lowest unit it still means to deliver, carried on its data); receiver releases at once, any order (H = 0) | 98.2 % | 98.2 % | 98.2 % | 5.5 / 6.0 / never (inf) | 0.361 | 0.03 |
| `G-T0p4-Hinf`: sender gives up as above at 0.4 s; receiver strict order (H = inf), so the floor lets it pass holes | 98.2 % | 98.2 % | 98.2 % | 5.5 / 6.0 / never (inf) | 0.361 | 0 |
| `G-T2-H0`: sender gives up at 2 s; receiver any order | 100.0 % | 98.2 % | 100.0 % | 5.5 / 6.0 / 902 | 0.777 | 0.0341 |
| `G-T2-Hinf`: sender gives up at 2 s; receiver strict order | 100.0 % | 98.1 % | 100.0 % | 5.5 / 6.0 / 924 | 0.777 | 0 |
| `G-Tinf-H0`: sender never gives up (old tail sweep); receiver any order | 100.0 % | 98.2 % | 100.0 % | 5.5 / 6.0 / 902 | 0.777 | 0.0341 |
| `G-T0p4-H0p02`: sender gives up at 0.4 s; receiver holds a hole 20 ms | 98.2 % | 98.2 % | 98.2 % | 5.5 / 6.0 / never (inf) | 0.361 | 0.00283 |

#### K-L10

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip) | 99.0 % | 97.0 % | 99.0 % | 51 / 171 / 1529 (never in 4 of 12 seeds) | 0.31 | 0.0625 |
| `C3`: never gives up (rho = 1), old tail sweep, resends only per its own gate; receiver strict order | 100.0 % | 97.7 % | 100.0 % | 66 / 174 / 1140 | 0.353 | 0 |
| `C3S`: S4 with lifetime T = infinity: never gives up, old tail sweep, no floor ever sent; receiver strict order (equal to C3 by identity) | 100.0 % | 97.7 % | 100.0 % | 66 / 174 / 1140 | 0.353 | 0 |
| `G-T0p4-H0`: sender gives up on a unit it cannot deliver by 0.4 s (floor = lowest unit it still means to deliver, carried on its data); receiver releases at once, any order (H = 0) | 97.9 % | 97.9 % | 97.9 % | 51 / 170 / never (inf) | 0.2 | 0.104 |
| `G-T0p4-Hinf`: sender gives up as above at 0.4 s; receiver strict order (H = inf), so the floor lets it pass holes | 97.9 % | 97.9 % | 97.9 % | 66 / 174 / never (inf) | 0.2 | 0 |
| `G-T2-H0`: sender gives up at 2 s; receiver any order | 100.0 % | 97.9 % | 100.0 % | 51 / 170 / 1007 | 0.353 | 0.121 |
| `G-T2-Hinf`: sender gives up at 2 s; receiver strict order | 100.0 % | 97.7 % | 100.0 % | 66 / 174 / 1140 | 0.353 | 0 |
| `G-Tinf-H0`: sender never gives up (old tail sweep); receiver any order | 100.0 % | 97.9 % | 100.0 % | 51 / 170 / 1007 | 0.353 | 0.121 |
| `S4b`: as 'gives up at 0.4 s, strict order', but the floor goes as its own datagram, at most one per 2 ms, taking link capacity | 97.9 % | 97.9 % | 97.9 % | 66 / 174 / never (inf) | 0.2 | 0 |
| `G-T0p4-H0p02`: sender gives up at 0.4 s; receiver holds a hole 20 ms | 97.9 % | 97.9 % | 97.9 % | 51 / 170 / never (inf) | 0.2 | 0.103 |

#### K-L10b

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip) | 99.1 % | 97.0 % | 99.1 % | 62 / 189 / 1519 (never in 1 of 12 seeds) | 0.496 | 0.06 |
| `C3`: never gives up (rho = 1), old tail sweep, resends only per its own gate; receiver strict order | 100.0 % | 97.5 % | 100.0 % | 90 / 202 / 1150 | 0.566 | 0 |
| `C3S`: S4 with lifetime T = infinity: never gives up, old tail sweep, no floor ever sent; receiver strict order (equal to C3 by identity) | 100.0 % | 97.5 % | 100.0 % | 90 / 202 / 1150 | 0.566 | 0 |
| `G-T0p4-H0`: sender gives up on a unit it cannot deliver by 0.4 s (floor = lowest unit it still means to deliver, carried on its data); receiver releases at once, any order (H = 0) | 97.9 % | 97.9 % | 97.9 % | 61 / 187 / never (inf) | 0.41 | 0.104 |
| `G-T0p4-Hinf`: sender gives up as above at 0.4 s; receiver strict order (H = inf), so the floor lets it pass holes | 97.9 % | 97.8 % | 97.9 % | 90 / 202 / never (inf) | 0.41 | 0 |
| `G-T2-H0`: sender gives up at 2 s; receiver any order | 100.0 % | 97.9 % | 100.0 % | 61 / 187 / 1017 | 0.566 | 0.121 |
| `G-T2-Hinf`: sender gives up at 2 s; receiver strict order | 100.0 % | 97.5 % | 100.0 % | 90 / 202 / 1150 | 0.566 | 0 |
| `G-Tinf-H0`: sender never gives up (old tail sweep); receiver any order | 100.0 % | 97.9 % | 100.0 % | 61 / 187 / 1017 | 0.566 | 0.121 |
| `G-T0p4-H0p02`: sender gives up at 0.4 s; receiver holds a hole 20 ms | 97.9 % | 97.9 % | 97.9 % | 61 / 187 / never (inf) | 0.41 | 0.103 |

#### E-I2n

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered |
|---|---|---|---|---|---|---|
| `C1`: today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip) | 99.3 % | 99.3 % | 99.3 % | 12 / 16 / 18 | 0.0342 | 0.0213 |
| `C3`: never gives up (rho = 1), old tail sweep, resends only per its own gate; receiver strict order | 100.0 % | 99.6 % | 100.0 % | 16 / 27 / 40 | 0.274 | 0 |
| `C3S`: S4 with lifetime T = infinity: never gives up, old tail sweep, no floor ever sent; receiver strict order (equal to C3 by identity) | 100.0 % | 99.6 % | 100.0 % | 16 / 27 / 40 | 0.274 | 0 |
| `G-T0p4-H0`: sender gives up on a unit it cannot deliver by 0.4 s (floor = lowest unit it still means to deliver, carried on its data); receiver releases at once, any order (H = 0) | 99.6 % | 99.6 % | 99.6 % | 5.5 / 5.9 / 18 | 0.0916 | 0.0294 |
| `G-T0p4-Hinf`: sender gives up as above at 0.4 s; receiver strict order (H = inf), so the floor lets it pass holes | 99.6 % | 99.6 % | 99.6 % | 16 / 27 / 40 | 0.0916 | 0 |
| `G-T2-H0`: sender gives up at 2 s; receiver any order | 100.0 % | 99.6 % | 100.0 % | 5.5 / 5.9 / 18 | 0.274 | 0.0296 |
| `G-T2-Hinf`: sender gives up at 2 s; receiver strict order | 100.0 % | 99.6 % | 100.0 % | 16 / 27 / 40 | 0.274 | 0 |
| `G-Tinf-H0`: sender never gives up (old tail sweep); receiver any order | 100.0 % | 99.6 % | 100.0 % | 5.5 / 5.9 / 18 | 0.274 | 0.0296 |
| `G-T0p4-H0p02`: sender gives up at 0.4 s; receiver holds a hole 20 ms | 99.6 % | 99.6 % | 99.6 % | 16 / 22 / 26 | 0.0916 | 0.00271 |

#### CL

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered | link busy over 1-10 s | queue drops outside / inside outage | resends refused by the budget |
|---|---|---|---|---|---|---|---|---|---|
| `C1`: today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip) | 98.7 % | 28.6 % | 98.7 % | 724 / 1337 / never (inf) | 0.0604 | 0.0265 | 82.8 % | 0 / 0 | 233298 |
| `C3`: never gives up (rho = 1), old tail sweep, resends only per its own gate; receiver strict order | 100.0 % | 19.9 % | 97.2 % | 982 / 1595 / 2132 | 0.0725 | 0 | 82.0 % | 0 / 0 | 393203 |
| `C3S`: S4 with lifetime T = infinity: never gives up, old tail sweep, no floor ever sent; receiver strict order (equal to C3 by identity) | 100.0 % | 19.9 % | 97.2 % | 982 / 1595 / 2132 | 0.0725 | 0 | 82.0 % | 0 / 0 | 393203 |
| `G-T0p4-H0`: sender gives up on a unit it cannot deliver by 0.4 s (floor = lowest unit it still means to deliver, carried on its data); receiver releases at once, any order (H = 0) | 84.8 % | 84.8 % | 84.8 % | 5.7 / never (inf) / never (inf) | 0.0352 | 0.0297 | 69.8 % | 0 / 0 | 14564 |
| `G-T0p4-Hinf`: sender gives up as above at 0.4 s; receiver strict order (H = inf), so the floor lets it pass holes | 84.8 % | 84.6 % | 84.8 % | 17 / never (inf) / never (inf) | 0.0352 | 0 | 69.8 % | 0 / 0 | 14564 |
| `G-T2-H0`: sender gives up at 2 s; receiver any order | 99.9 % | 21.7 % | 99.8 % | 824 / 1420 / 1618 | 0.0719 | 0.0385 | 82.0 % | 0 / 0 | 386500 |
| `G-T2-Hinf`: sender gives up at 2 s; receiver strict order | 99.9 % | 19.9 % | 99.1 % | 977 / 1586 / 1994 | 0.0719 | 0 | 82.0 % | 0 / 0 | 386500 |
| `G-Tinf-H0`: sender never gives up (old tail sweep); receiver any order | 100.0 % | 21.4 % | 99.8 % | 829 / 1425 / 1626 | 0.0725 | 0.0394 | 82.0 % | 0 / 0 | 393203 |
| `S4b`: as 'gives up at 0.4 s, strict order', but the floor goes as its own datagram, at most one per 2 ms, taking link capacity | 84.8 % | 84.6 % | 84.8 % | 17 / never (inf) / never (inf) | 0.0352 | 0 | 69.8 % | 0 / 0 | 14564 |
| `G-T0p4-H0p02`: sender gives up at 0.4 s; receiver holds a hole 20 ms | 84.8 % | 84.8 % | 84.8 % | 16 / never (inf) / never (inf) | 0.0352 | 0.00292 | 69.8 % | 0 / 0 | 14564 |

#### OV

| arm (sender; receiver) | delivered at all | within 0.4 s | within 2 s | delay p50 / p90 / p99 from hand-over (ms) | resends per delivered | out of order per delivered | link busy over 1-10 s | queue drops outside / inside outage | resends refused by the budget |
|---|---|---|---|---|---|---|---|---|---|
| `C1`: today's engine: gives up on a hole when the 1 % allowance permits; receiver passes holes early (early-skip) | 99.1 % | 11.3 % | 64.4 % | 1568 / 2775 / 3036 | 0.0627 | 0.0207 | 100.0 % | 0 / 0 | 379280 |
| `C3`: never gives up (rho = 1), old tail sweep, resends only per its own gate; receiver strict order | 100.0 % | 6.7 % | 62.7 % | 1634 / 2767 / 3030 | 0.0723 | 0 | 100.0 % | 0 / 0 | 366504 |
| `C3S`: S4 with lifetime T = infinity: never gives up, old tail sweep, no floor ever sent; receiver strict order (equal to C3 by identity) | 100.0 % | 6.7 % | 62.7 % | 1634 / 2767 / 3030 | 0.0723 | 0 | 100.0 % | 0 / 0 | 366504 |
| `G-T0p4-H0`: sender gives up on a unit it cannot deliver by 0.4 s (floor = lowest unit it still means to deliver, carried on its data); receiver releases at once, any order (H = 0) | 83.1 % | 83.1 % | 83.1 % | 390 / never (inf) / never (inf) | 0.0105 | 0.00399 | 100.0 % | 0 / 0 | 38132 |
| `G-T0p4-Hinf`: sender gives up as above at 0.4 s; receiver strict order (H = inf), so the floor lets it pass holes | 83.1 % | 80.1 % | 83.1 % | 390 / never (inf) / never (inf) | 0.0105 | 0 | 100.0 % | 0 / 0 | 38132 |
| `G-T2-H0`: sender gives up at 2 s; receiver any order | 94.0 % | 13.7 % | 94.0 % | 1438 / 1990 / never (inf) | 0.0521 | 0.0213 | 100.0 % | 0 / 0 | 247086 |
| `G-T2-Hinf`: sender gives up at 2 s; receiver strict order | 94.0 % | 6.7 % | 90.8 % | 1634 / 1993 / never (inf) | 0.0521 | 0 | 100.0 % | 0 / 0 | 247086 |
| `G-Tinf-H0`: sender never gives up (old tail sweep); receiver any order | 100.0 % | 13.7 % | 69.6 % | 1438 / 2583 / 2841 | 0.0723 | 0.0299 | 100.0 % | 0 / 0 | 366504 |
| `S4b`: as 'gives up at 0.4 s, strict order', but the floor goes as its own datagram, at most one per 2 ms, taking link capacity | 79.5 % | 76.7 % | 79.5 % | 390 / never (inf) / never (inf) | 0.0103 | 0 | 100.0 % | 0 / 0 | 36074 |
| `G-T0p4-H0p02`: sender gives up at 0.4 s; receiver holds a hole 20 ms | 83.1 % | 83.1 % | 83.1 % | 390 / never (inf) / never (inf) | 0.0105 | 0.00395 | 100.0 % | 0 / 0 | 38132 |


## A/A self-check (frozen before any decision cell)

`nf_stage3c.json` was written once, after the A/A runs (S4 at T = inf, H = inf, phase variant 0 = arm `C3S` and variant 1 = arm `AA-1`, 12 seeds, 12 cells) and after the identity and liveness arms, before any other arm was read; it was not rewritten. NF = p95 of |A - A'| per cell and readout; NF_K = NF_P = that floor. Self-check (each A/A difference read against the T bar only): 324 readings over 12 cells, 240 equivalent, 76 undetermined, 2 shown (X15 `stall_total` +59 ms [13, 109]; X20 `stall_total` -54 ms [-105, -6], bar 1 ms; both are the sum of stall episodes in a 60 s, 50 units-per-second cell, which depend on the clock phase). The frozen table was kept. An unregistered replication with two fresh phase draws (variants 2 and 3, all 12 cells) reads 9 of 324 shown, all post-outage or tail quantiles and tail times at 1 ms bars (E-I2n `stall_total`; E-L10 `dh_p99`, `v_p99`, `tail_time`, `finish`; K-L10 and K-L10b `dh_p99` and `v_p99`, +78 ms): the 1 ms bars on those readouts are below the sweep and report phase. None of the nine is a readout used by a registered prediction against its bar.

## Worker decisions (every reading of an ambiguity or a choice not fixed by the spec)

1. **S4 is a sender name selecting the S2 family; its behaviour comes from `Cfg` fields set by `batch3c.s4_cfg`** (`verdict_evidence = "feedback"`, `floor_law = "protect"`, `floor_carrier`, `deq_expiry`, `rtt_est = "rfc"`, `o_excl_verdict = False`, `tail_elig = "P"`). The name makes S4 part of `s2fam` and `s3` (L2 uses srtt/2) and removes the resume-on-verdict call. `evidence_verdict` is False for S4 (the feedback rule replaces it).
2. **The header carrier sends no floor datagram from `act`; only the sweep's floor repeat does.** 15.6.1 items 2 and 3 can be read as `act` sending a floor under both carriers, which would make S4 cost what S4b costs. Two constraints in the text decide: 15.3 models the header "at zero link cost", and Q3c-3 expects S4b about 5 % below S4. The measured witness: 38.6 floor datagrams per second (S4) against 487.5 (S4b) in OV.
3. **Carrier "datagram" (S4b)** is evaluated at the end of each first send and at the end of each `act`: at most one floor datagram per 2 ms window (`int(now / 0.002)`), when `F_ann > A_known` and `F_ann > last_floor`.
4. **`update_floor` runs at every first send, every copy sent, every report, and at the start of every sweep fire** (before the tail list is built, so the sweep's resends obey the commitment). The scan is exactly 15.6.1 item 2.
5. **The commitment is enforced by filtering candidates below `F_ann` at the top of `act`** (gauge `commit_skip`), so under the protected floor no candidate reaches the verdict branch and `verdicts` is 0 in every S4 run (a witness of the "unreachable" statement of 15.1). The feedback evidence rule is exercised on fabricated state (test 8): `s` listed in `M_known`, the report current (`last_gen > last_send[s]`, using the stored generation of the newest report, which equals `gen` for a report call and is the only defined value on a sweep call), and `last_send[s] < tau_rx`.
6. **One RFC srtt for every arm in the cooldown and in `protected`**: `srtt_eff()` = the RFC 6298 smoothed RTT with the 333 ms seed before the first sample, independent of the sender (`o_hat()` returns the exact one-way delay for S1c, which would give C3 a different cooldown from S4 and break the link-cell identity).
7. **The brake is checked before `send_copy` mutates anything**; `serves`, `resend_by_*` and `verdict_copies` count only copies actually sent. For C1 a verdict whose copy is refused by the budget still stands. A refused copy increments `resend_braked` (and the 1 s window series) each time a decision instant lists it, so the counter counts refusals, not distinct sequence numbers.
8. **The resend gate "cooldown" follows 15.6.1 item 5 literally** (a sequence number with no copy yet is exempt). This is the source of the Q3c-2 misses (see "Things that looked wrong", item 1); the variant without the exemption is the unregistered supplement, `resend_gate = "cooldown_all"`.
9. **`floor_hull_bind`** is gauged against the lowest unresolved sequence number of the last report (the minimum of the missing set, else highest-received plus one): the update binds when that sequence number lies below the old floor and is still protected. It does not rescan from `A_known`. It read 0.000 to 0.002 in every cell.
10. **`F_lag`** is the mean over first sends of `n_sent - F_ann` after the update at that send.
11. **Window series** (per 1 s of event time; printed by `python summarize3c.py windows`, appended below for OV and CL, every arm of the cell tables): first sends, copies sent, copies refused by the budget, copies refused by the cooldown, link busy time (a service interval that crosses a boundary is split), pause time (from the pause log, closed at the end of the run), and "blocked by pause" = the source rate times the paused time, i.e. the hand-over instants that fell inside a pause. `pause_time_w` is still in the output but unused.
12. **Stall parts (o), (oo), (i), (ii), (iii)**: the episode is the longest strict-order stall episode, including one still open at the end of the run, that overlaps the receiver-side outage `[start + owd, end + owd]` (`owd` = the cell's base one-way delay). (i) is the time from the outage end (or the episode start, if later) to the first floor event (header floor or floor datagram) that arrives after the outage; when no floor arrives (C3 sends none) (i) is the rest of the episode and `floor_seen` is false. (ii) is the span from that floor to the last fill inside the episode; (iii) the remainder; every part is clamped at 0. In the E cells the overlapping episode is often one that ends a few ms after the receiver-side outage began (the registration says "overlaps"); those rows are not used by Q3c-10.
13. **Q3c-8's "the run's own median report-driven resend age"** is the median over seeds of each run's median hand-over age at first resends triggered by a report, in the same cell's T = 0.4, H = 0 run (the level run used for the mid-level reading). The same median pooled over the sweep runs is printed beside it (equal to the fourth digit in every cell). The lowest-T bullet counts `repair_sends` (every copy).
14. **Q3c-7's reference** is stage 3's stored S2 K-cell outputs (`out3/ID/K-*.G-T0p4-Hinf-*.json`, 12 seeds), verified by re-running seed 0 of each cell through the stage 3 runner (identical to the stored value). `m` is max(T bar, A/A floor) of the cell.
15. **Nudge cells**: every cell except X15 and X20, i.e. also K-L10b, E-I2n and OV (stage 3b had them in the six stage cells and CL only). The full arm grid, C1 and S4b (OV, CL, K-L10) also run in K-L10b and E-I2n; X15 and X20 carry the identity arms, A/A, the T grid and the T = 0.4, H = 0 level run only.
16. **The identity key list** is fixed in `summarize3c.ID_KEYS` (37 keys: the 20 stage 3 keys, the four shares, three delay quantiles, units, `finish`, pause and queue counters) and in the unit test (`ID_EXCL`: config labels, `srtt`, `srtt_mean`, `R_mean` which an off-link C3 estimates the old way, `gauges`, of which 14 counters are compared, and `F_lag` and `floor_hull_bind`, which are defined only for the new sender). The stored 12-seed comparison and the unit test use the same keys otherwise.
17. **`check_ident3c.py`** compares digests of the trace and the per-sequence arrays (length and SHA-256 of the pickle) instead of storing them, drives the unchanged `batch3.py`/`batch3b.py` builders (checked byte-equal to the reference commit's), and takes the reference from a `git archive` of `d0ce4f9d` run in its own process pool. The stage 3 commit is not run separately: `d0ce4f9d` was itself checked against it by `check_ident3b.py`.
18. **`finished`** is true when the receiver's frontier reached the last unit (`t_front` set) within `t_limit`.
19. **Placement** (`pc_place3c.py`): the registered procedure (steps 1 to 6 of 15.5) with `ceil_1ms(max D + 2 ms)`, usable iff `min D > 0.99 p + 2 ms`; K-hat and P-hat from the stored ages above 0.2 s of the same T = inf run. See deviation 1 for the second placement file.
20. **Tests** (`test_toy2.py`, class `Stage3c`, 11 tests, run red first): defaults and the absolute values of `protected`; S4 equals C3 and T = 1e6 equals inf (cells E-L10, M-I2, K-L10, E-I2n, shortened CL and OV; traces; 3 and 2 seeds); the seam reads 0 and the floor executes (K-L10, OV, CL; `F_ann` advanced, `pass_floor` > 0, floors carried > 0), with a positive control that the seam counts a planted violation; no floor at T = inf and `F_ann == A_known` after every report; the brake (no window above 10, `resend_braked` > 0, and the unbraked control exceeds 10); the deny test (`protected` exempt, a planted `protectedish` flagged, and the source has no `isinf`); the feedback verdict on fabricated state (four cases); the carrier (S4 carries, S4b sends at most 500 per second); the collapse fixed in OV against the stage 3b sender on the same seed; the window series and `finished`. The seam test passes on its 3 cells because it runs seed 1 and a shortened OV and CL; the full-length registered readout is the one that misses (deviation 5).

## Deviations (timing as required)

1. **A second placement file for the positive control, written after the registered one read "0 usable".** `pc_stage3c.json` (written first, after the identity and A/A runs and before any other cell was read) applies the registered procedure literally and finds 0 of 12 seeds usable: the dead-tail set `seq >= dead[0]` contains one unit that is not dead. The source clock accumulates float error, so unit 12097 is sent at 1.2096999999998832 s, just before the outage window starts at 1.2097 s, and is delivered in 13 ms; it sets `min D` far below the ramp. `pc_stage3c_dev.json` (written next, before the main batch was started) takes as dead-tail units those whose first send fell inside the window (49 of the 50). The criterion, the 8-of-12 rule and the formulas are unchanged. It reads 2 of 12 usable, so the control is "not placed" on either file. The PC arms ran on the second file (2 seeds) and are descriptive only. The same off-by-one unit sits in E-I2n in stage 3b (not re-examined).
2. **Test-side changes after the first run of the new unit tests** (before any batch data): `F_lag` and `floor_hull_bind` added to the identity exclusions (None or 0 for C3 by definition); the single-seed OV collapse test tolerance widened from 0.03 to 0.05 (the registered 0.03 is read on 12 seeds); M-I2 dropped from the carrier assertion (no hole survives long enough to need a floor there); the window-series length bound lowered to 2. No prediction, bar or cell changed.
3. **Code added after the identity and A/A arms had run:** the per-first-resend age log (`rs_age`, output keys `rs_age_*`) for Q3c-8, while the ID and AA batch was running (a readout only; the stored ID and AA files lack the `rs_age_*` keys, no reading uses them; `check_ident3c.py` was re-run on the final code). The `resend_gate = "cooldown_all"` option and the SUP arms (deviation 5), after the main batch.
4. **Report-script fixes after a first print:** `report_s3c.py` q6 (the T = inf, H = inf comparison arm is `C3S`), q7 (stage 3's S2 outputs live in `out3/ID`), q9 (both placement files), and an unrelated slip in `summarize3c.py` that dropped its `TAG`/`gname` helpers (the freeze did not use them). A partial print of Q3c-3 was taken while the batch was still running (the S4 arms were missing); it changed nothing.
5. **Unregistered runs:** the A/A replication (arms `AA-2`, `AA-3`), and the supplement `SUP` (16 configurations in each link cell x 12 seeds = 192 runs) that applies the cooldown to first resends, run after the Q3c-2 misses were read, to bound their effect. They are labelled where used and change no registered reading.
6. **No change** to any prediction, bar, cell, arm list or control placement of the registration after seeing data, other than deviation 1, and `nf_stage3c.json` and the two placement files were each written once.

## Things that looked wrong or surprising

1. **The commitment seam misses because the registration contradicts itself, and one copy in a decision can move the floor past the candidates queued behind it.** 15.6.1 item 1 puts the earliest resend of a sequence number at `max(now, last_send + c)`, so a unit just sent is unprotected for one cooldown, which is what lets the floor sit at the send frontier under overload. Item 5 lets a sequence number with `copies == 0` go at once. Trace (OV, seed 5, T = 0.4, H = inf, sweep period 25 ms): at a sweep fire `act` filters its candidates against `F_ann` = 14 559, serves the first (a protected hole), and the floor update that follows that copy scans on and moves `F_ann` to 15 499 inside the same loop; the next candidates (14 562 and on, never resent, first sent 98.7 ms earlier, cooldown 103.5 ms, hand-over age 0.345 s so L2 still passes) are then below the floor and still go out. 143 such copies in 28 runs (every one in a link cell; 13 OV arm groups in 9 seeds, and CL `NT-0p98` in 1 seed); three runs then show a late fill of a floor-passed hole (`late_floor` 4). The supplement (the cooldown on first resends, so the gate and the law agree) shows 0 violations and 0 late fills in 192 runs, `max copies/5ms` 10; the S4 arms at T = 0.4 move by at most +0.0010 (OV H = 0: 0.8318 against 0.8308; H = inf 0.8020 against 0.8011), and the baselines that resend more move more: C3 in OV `share_by_2` 0.6273 to 0.6665 (`share_by_0p4` +0.0027), C1 in CL `share_by_0p4` +0.036, S4 at T = 2, H = 0 in CL +0.034. The conclusions of Q3c-3 do not change. Recommended: item 5 reads "now - last_send >= cooldown" with no exemption in the link cells, and `act` re-checks `s >= F_ann` before each copy.
2. **Q3c-4's limit did not hold.** 15.3 says the floor cannot shorten a strict wait for data that is still being resent, and predicts T = 2 s strict in OV unchanged at about 75 %. It reads 90.8 %, with p50 1.63 s and p90 1.99 s. Four things differ from stage 3b besides the floor: the old sweep instead of the R probe, the engine cooldown instead of the evidence gate, the engine resend budget, and the verdict rule (feedback only, no timer). The effect was not isolated (no arm toggles one of these at a time). The same arm in CL reads `share_by_2` 99.1 % at T = 2, H = inf against 97.2 % at T = inf, which is not equivalent (Q3c-6), so the floor also gives up units that would arrive later than 2 s.
3. **The old sweep's 100 ms cap lets resends go in K-L10b below the crossing.** At T = 0.180 s (60 ms path), 598 to 599 copies per seed are sent. Consistent with the old sweep's eligibility, which is first-send age P = 100 ms (the cap binds: 2 srtt is 240 ms), against about 0.141 s for the report path's first resend: a sweep resend at age 0.1 to 0.12 s still passes L2 (age + srtt/2 < T) and can arrive in time. The bound "zero resends at the lowest T" holds in the other three crossover cells, whose lowest T lies below the sweep's first possible resend. Not isolated by an arm. This is the cap-bind finding of 13.4, now seen as a failed bullet.
4. **C3's collapse on the link is unchanged by the resend budget.** In CL it reads 19.9 % within 0.4 s (stage 3b: 19.9 %), in OV 6.7 % (stage 3b 2.5 %). The budget removes the queue floods (queue drops outside the outage 0 for every arm, against 684 000 in CL for C3 in 3b) but not the backlog wait, so the band 25 to 47 % (the 13.6 drain arithmetic) is not what the arm does. In CL the link carries the late units too (utilisation 0.82 in [1 s, 10 s], against 0.70 for S4 at T = 0.4, which gives up 15 % of the units).
5. **Q3c-10's (o) bound assumed only holes from the last one-way delay before the outage.** (o) reaches 0.179 s at K-L10 (seed 6): a hole opened up to 180 ms before the outage is still protected (the sender means to resend it while age + srtt/2 < 0.4 s), so no floor passes it and the strict receiver holds it through the outage, as in 3b.
6. **S4 H = 0 in OV sits at 0.8308, above the band's top 0.83.** The band came from `(C/lambda)(1 - p0)` = 0.808 plus intake effects; the measured value is within 0.003 of the offered-load ceiling C/lambda = 0.833 (units not delivered: 17 %, i.e. the 16.7 % that the link cannot carry plus 0.2 %).
7. **The registered K control has no cell**, now for a second reason besides 3b's serial recovery: the old sweep recovers the 49 dead-tail units in several sweep rounds of up to 100 ms (the ramp is 5 to 50 ms wide, against 0.01 p = 15 ms), so the "step" is a ramp in 10 of 12 seeds. What spreads the recovery (sweep rounds, the 10-copies-per-5-ms budget, the report timing) was not separated.
8. **About half the copies the old arms send in OV are duplicates, and `resend_braked` counts refusals per decision instant.** Copies sent against duplicate arrivals (seed means, `python summarize3c.py dups`): OV C3 8 676 sent, 4 228 duplicates (0.49); C1 0.57; S4 at T = 0.4 0.52; S4 at T = 2, H = inf 0.49; CL C3 0.28, S4 at T = 0.4 none. The open tail the 25 ms sweep resends is mostly data still queued in the forward link (reports lag the queue by about 100 ms), and the first-resend exemption lets it go again. `resend_braked` counts a refusal at every decision instant that lists the unit, so it reads 366 504 per run for C3 in OV and drops to 2 031 under the cooldown supplement (OV S4 T = 0.4, H = inf: 38 132 to 717); it is a gauge of the budget binding, not a count of delayed units. This qualifies the "fair baselines" reading for every arm: the old arms' resends in the link cells are largely redundant, so a baseline with the cooldown on first resends (the supplement) is the fairer one, and the C3 and C1 bands above are read on the registered gate.

## Recommended next steps (not built)

- Fix 15.6.1 item 5 to apply the cooldown to a first resend, then re-run the link cells (the supplement says nothing else moves).
- Decide the open-tail law for the Realtime cells: the old sweep's 100 ms cap binds in K-L10b; a sweep period tied to the measured RTT (the 13.4 idea, without the serial probe) would remove it.
- Re-place the K control on a cell and sender whose tail recovery is one step, or drop the control and keep the per-seed placement as the diagnostic it is.
- Register the engine-side guard of 15.7: a tunnel datagram stream at Realtime (T = 0.4 s) with an outage and an overload phase, off-Bulk, with execution witnesses (CLAUDE.md, meas-guard-bulk-blind).

## Reading detail (generated: `python summarize3c.py ident|ident-trace|live|q1..q10|tables|aa|rep|sup|windows|dups`)

### [ident]
### Q3c-1: S4 at T = inf (C3S) against C3, and S4 at T = 1e6 against C3S; float-equal on 37 keys, 12 seeds

- E-I2: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- E-I2n: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- E-L10: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- M-I2: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- M-L10: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- K-I2: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- K-L10: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- K-L10b: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- CL: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- OV: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- X15: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- X20: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12

seed-level diffs: 0 of 288
### [ident-trace]
- E-I2: traces identical/identical/identical (seeds [0, 1, 2]; 33161 entries)
- E-I2n: traces identical/identical/identical (seeds [0, 1, 2]; 15462 entries)
- E-L10: traces identical/identical/identical (seeds [0, 1, 2]; 19100 entries)
- M-I2: traces identical/identical/identical (seeds [0, 1, 2]; 127251 entries)
- M-L10: traces identical/identical/identical (seeds [0, 1, 2]; 43415 entries)
- K-I2: traces identical/identical/identical (seeds [0, 1, 2]; 5341 entries)
- K-L10: traces identical/identical/identical (seeds [0, 1, 2]; 4026 entries)
- K-L10b: traces identical/identical/identical (seeds [0, 1, 2]; 4681 entries)
- CL: traces identical/identical/identical (seeds [0, 1, 2]; 106875 entries)
- OV: traces identical/identical/identical (seeds [0, 1, 2]; 128199 entries)
- X15: traces identical/identical/identical (seeds [0, 1, 2]; 3308 entries)
- X20: traces identical/identical/identical (seeds [0, 1, 2]; 3308 entries)
### [live]
finished flag: 3646 runs carry it, 0 did not finish
### [q1]
### Q3c-1: S4 at T = inf (C3S) against C3, and S4 at T = 1e6 against C3S; float-equal on 37 keys, 12 seeds

- E-I2: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- E-I2n: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- E-L10: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- M-I2: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- M-L10: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- K-I2: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- K-L10: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- K-L10b: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- CL: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- OV: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- X15: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12
- X20: C3S vs C3: 12/12; ID-1e6 vs C3S: 12/12

seed-level diffs: 0 of 288

Liveness: `finished` is true in 3646 of 3646 runs that carry it.
  families with an unfinished run: {}
### [q2]
### Q3c-2 the commitment and the brake

runs 3454: commit_viol total 143; late_floor total 4; commit_skip (candidates skipped below the floor) total 67090764
  late_floor != 0 in OV: [('NT-0p98', 8), ('NT-0p99', 4), ('NT-1p02', 3)]

queue_drop_out per arm in CL and OV (sum over 12 seeds), queue_drop_in (mean per seed), max copies in a 5 ms window (max over seeds), resend_braked mean:
  CL   C1             queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked  233297.7
  CL   C3             queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked  393202.6
  CL   C3S            queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked  393202.6
  CL   G-T0p4-H0      queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked   14564.0
  CL   G-T0p4-H0p02   queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked   14564.0
  CL   G-T0p4-Hinf    queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked   14564.0
  CL   G-T2-H0        queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked  386500.2
  CL   G-T2-Hinf      queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked  386500.2
  CL   G-Tinf-H0      queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked  393202.6
  CL   ID-1e6         queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked  393202.6
  CL   S4b            queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked   14564.0
  OV   C1             queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked  379279.7
  OV   C3             queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked  366503.8
  OV   C3S            queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked  366503.8
  OV   G-T0p4-H0      queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked   38131.8
  OV   G-T0p4-H0p02   queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked   38131.8
  OV   G-T0p4-Hinf    queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked   38131.8
  OV   G-T2-H0        queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked  247086.1
  OV   G-T2-Hinf      queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked  247086.1
  OV   G-Tinf-H0      queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked  366503.8
  OV   ID-1e6         queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked  366503.8
  OV   S4b            queue_drop_out      0  queue_drop_in       0.0  max copies/5ms 10  resend_braked   36073.8
overall max copies in any 5 ms window over every CL and OV run: 10
### [q3]
### Q3c-3 the collapse fixed (OV, T = 0.4)

| arm | share_any | share_by_0p4 | share_by_2 | dh_p50 | dh_p90 | dh_p99 | intake_expired | intake_expired_deq | link_util_w | queue_drop_out | resend_braked | F_lag | floor_hull_bind | pass_floor |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| C1 | 0.9907 | 0.1134 | 0.6436 | 1568 | 2775 | 3036 | 0 | 0 | 1 | 0 | 3.793e+05 | nan | 0 | 4.25 |
| C3 | 1 | 0.06742 | 0.6273 | 1634 | 2767 | 3030 | 0 | 0 | 1 | 0 | 3.665e+05 | nan | 0 | 0 |
| C3S | 1 | 0.06742 | 0.6273 | 1634 | 2767 | 3030 | 0 | 0 | 1 | 0 | 3.665e+05 | 2762 | 0 | 0 |
| G-T0p4-H0 | 0.8313 | 0.8308 | 0.8313 | 389.6 | inf | inf | 0 | 1.759e+04 | 1 | 0 | 3.813e+04 | 238.7 | 0.001858 | 2659 |
| G-T0p4-Hinf | 0.8313 | 0.8011 | 0.8313 | 389.6 | inf | inf | 0 | 1.759e+04 | 1 | 0 | 3.813e+04 | 238.7 | 0.001858 | 2659 |
| G-T2-H0 | 0.94 | 0.1374 | 0.9396 | 1438 | 1990 | inf | 0 | 6197 | 1 | 0 | 2.471e+05 | 1889 | 0.0004305 | 1000 |
| G-T2-Hinf | 0.94 | 0.06742 | 0.9077 | 1634 | 1993 | inf | 0 | 6197 | 1 | 0 | 2.471e+05 | 1889 | 0.0004305 | 1000 |
| G-Tinf-H0 | 1 | 0.1374 | 0.6964 | 1438 | 2583 | 2841 | 0 | 0 | 1 | 0 | 3.665e+05 | 2762 | 0 | 0 |
| S4b | 0.7953 | 0.7671 | 0.7953 | 390.3 | inf | inf | 0 | 2.201e+04 | 1 | 0 | 3.607e+04 | 238.8 | 0.002022 | 2549 |

S4 H=0 share_by_0p4 0.8308 in [0.78, 0.83]: FAIL
S4 H=inf share_by_0p4 0.8011 in [0.75, 0.83]: PASS; at least H=0 - 0.03 = 0.8008: PASS
S4b H=inf share_by_0p4 0.7671; S4(H=inf) x 0.95 = 0.7610; within +-0.02: PASS; queue_drop_out sum 0
  floor slots per second on the link (floor datagrams / 10 s) for G-T0p4-Hinf: mean 38.6 (min 38.1, max 39.2); header-carried floors per second 9257
  floor slots per second on the link (floor datagrams / 10 s) for S4b: mean 487.5 (min 464.8, max 507.8); header-carried floors per second 0
S4 H=inf per seed min/max 0.7962 0.8053; H=0 0.8303 0.8314
paired H=inf minus H=0: -0.02976 [-0.03111, -0.02843] vs m=0.0466: equivalent
### [q4]
### Q3c-4 the limit (OV, T = 2)

T=2 H=inf share_by_2 0.9077 in [0.65, 0.85]: FAIL
T=2 H=0 share_by_2 0.9396 in [0.90, 0.99]: PASS
  G-T2-Hinf: dh_p50 1634 dh_p90 1993 share_by_0p4 0.06742; per-seed share_by_2 min/max 0.9027 0.9157
  G-T2-H0: dh_p50 1438 dh_p90 1990 share_by_0p4 0.1374; per-seed share_by_2 min/max 0.9381 0.9418
### [q5]
### Q3c-5 fair baselines (OV)

C3 share_by_0p4 0.0674 in [0.05, 0.20]: PASS
C3S share_by_0p4 0.0674 in [0.05, 0.20]: PASS
S4 T=inf H=0 share_by_0p4 0.1374 in [0.12, 0.21]: PASS
C1 share_by_0p4 0.1134 (reported)

link utilisation over [1, 10] s per arm (min over seeds; the cell is valid for an arm if >= 0.98):
  C1             min 1.0000 mean 1.0000  -> valid
  C3             min 1.0000 mean 1.0000  -> valid
  C3S            min 1.0000 mean 1.0000  -> valid
  G-T0p4-H0      min 1.0000 mean 1.0000  -> valid
  G-T0p4-H0p02   min 1.0000 mean 1.0000  -> valid
  G-T0p4-Hinf    min 1.0000 mean 1.0000  -> valid
  G-T2-H0        min 1.0000 mean 1.0000  -> valid
  G-T2-Hinf      min 1.0000 mean 1.0000  -> valid
  G-Tinf-H0      min 1.0000 mean 1.0000  -> valid
  ID-1e6         min 1.0000 mean 1.0000  -> valid
  S4b            min 1.0000 mean 1.0000  -> valid
### [q6]
### Q3c-6 the drain gain (CL)

| arm | share_any | share_by_0p4 | share_by_2 | dh_p50 | dh_p90 | dh_p99 | queue_drop_in | queue_drop_out | intake_expired | intake_expired_deq | link_util_w | resend_braked | pass_floor |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| C1 | 0.9874 | 0.2863 | 0.9874 | 724.5 | 1337 | inf | 0 | 0 | 0 | 0 | 0.8284 | 2.333e+05 | 140.1 |
| C3 | 1 | 0.199 | 0.9716 | 981.8 | 1595 | 2132 | 0 | 0 | 0 | 0 | 0.8196 | 3.932e+05 | 0 |
| C3S | 1 | 0.199 | 0.9716 | 981.8 | 1595 | 2132 | 0 | 0 | 0 | 0 | 0.8196 | 3.932e+05 | 0 |
| G-T0p4-H0 | 0.8476 | 0.8476 | 0.8476 | 5.666 | inf | inf | 0 | 0 | 1.024e+04 | 3999 | 0.6975 | 1.456e+04 | 1004 |
| G-T0p4-Hinf | 0.8476 | 0.8465 | 0.8476 | 17 | inf | inf | 0 | 0 | 1.024e+04 | 3999 | 0.6975 | 1.456e+04 | 1004 |
| G-T2-H0 | 0.9991 | 0.2167 | 0.9983 | 823.8 | 1420 | 1618 | 0 | 0 | 0 | 0 | 0.8196 | 3.865e+05 | 92.58 |
| G-T2-Hinf | 0.9991 | 0.199 | 0.991 | 977 | 1586 | 1994 | 0 | 0 | 0 | 0 | 0.8196 | 3.865e+05 | 92.58 |
| G-Tinf-H0 | 1 | 0.214 | 0.9981 | 828.6 | 1425 | 1626 | 0 | 0 | 0 | 0 | 0.8196 | 3.932e+05 | 0 |
| S4b | 0.8476 | 0.8465 | 0.8476 | 17 | inf | inf | 0 | 0 | 1.024e+04 | 3999 | 0.6975 | 1.456e+04 | 1004 |

C3 share_by_0p4 0.1990 in [0.25, 0.47]: FAIL; share_by_2 0.9716 >= 0.95: PASS
S4 T=0.4: H=0 0.8476, H=inf 0.8465, both in [0.78, 0.90]: PASS; |H=inf - H=0| = 0.0011 <= 0.02: PASS
  S4 T=2 minus T=inf at H=0 on share_by_0p4: 0.002696 [0.001994, 0.003425] vs m=0.0401: equivalent
  S4 T=2 minus T=inf at H=0 on share_by_2: 0.0002175 [0.0001908, 0.0002458] vs m=0.00321: equivalent
  S4 T=2 minus T=inf at H=inf on share_by_0p4: 2.25e-05 [0, 6.75e-05] vs m=0.0401: equivalent
  S4 T=2 minus T=inf at H=inf on share_by_2: 0.01944 [0.01697, 0.02195] vs m=0.00321: shown larger
  S4 T=0.4 H=0 minus C3 on share_by_0p4: 0.6486 [0.6482, 0.6491] vs m=0.0401: shown larger; larger by more than 0.25: PASS
  S4 T=0.4 H=inf minus C3 on share_by_0p4: 0.6475 [0.6472, 0.6479] vs m=0.0401: shown larger; larger by more than 0.25: PASS
### [q7]
### Q3c-7 no loss of savable data (K cells, T = 0.4, H = inf)

- K-I2: stage 3 S2 (T=0.4, H=inf) share_by_0p4 mean 0.9814 (seed 0 re-run 0.981667, stored 0.981667: match); S4 0.9815; paired S4 - S2 5.556e-05 [0, 0.0001389] vs m=0.000947: equivalent; m = 0.00095; S4 >= S2 - m: PASS
  F_lag (mean over sends of n_sent - F_ann): S4 T=0.4 H=inf 1.505; floor_hull_bind 0.0000; pass_floor 55.4; commit_skip 1262.8
- K-L10: stage 3 S2 (T=0.4, H=inf) share_by_0p4 mean 0.9785 (seed 0 re-run 0.980000, stored 0.980000: match); S4 0.9789; paired S4 - S2 0.0004444 [0.0001111, 0.0008889] vs m=0.00607: equivalent; m = 0.00607; S4 >= S2 - m: PASS
  F_lag (mean over sends of n_sent - F_ann): S4 T=0.4 H=inf 8.320; floor_hull_bind 0.0000; pass_floor 61.8; commit_skip 433.5
- S4b in K-L10: share_by_0p4 0.9789, floors 62.9
### [q8]
### Q3c-8 crossover (S4, old sweep, H = 0)

- X15: share_T at lowest T 0.8992, at T=0.4 1.0000; mid-level crossing 0.0677 s (per-seed 0.0672 0.0689 0.069 0.0672 0.0672 0.0672 0.0672 0.0689 0.0672 0.0673 0.0689 0.0684); 90% crossing 0.06485; median first-resend age (report-driven, T=0.4 H=0 run, median over seeds) 0.0520; srtt_mean/2 0.0158; sum 0.0678 s (crossing - sum = -0.1 ms); pooled sweep-run median age 0.0514
  curve: 0.040:0.8992 0.042:0.8992 0.044:0.8992 0.046:0.8992 0.048:0.8992 0.050:0.8992 0.052:0.8992 0.054:0.8992 0.056:0.8992 0.058:0.8992 0.060:0.8992 0.062:0.8992 0.064:0.8992 0.066:0.9011 0.068:0.9589 0.070:0.9816
  at the lowest T: repair_sends per seed [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
- X20: share_T at lowest T 0.8992, at T=0.4 1.0000; mid-level crossing 0.0825 s (per-seed 0.0826 0.0822 0.0822 0.0837 0.0839 0.0823 0.0825 0.0822 0.0837 0.0823 0.0822 0.0822); 90% crossing 0.0802; median first-resend age (report-driven, T=0.4 H=0 run, median over seeds) 0.0612; srtt_mean/2 0.0207; sum 0.0818 s (crossing - sum = +0.7 ms); pooled sweep-run median age 0.0610
  curve: 0.055:0.8992 0.057:0.8992 0.059:0.8992 0.061:0.8992 0.063:0.8992 0.065:0.8992 0.067:0.8992 0.069:0.8992 0.071:0.8992 0.073:0.8992 0.075:0.8992 0.077:0.8992 0.079:0.8992 0.081:0.9006 0.083:0.9643 0.085:0.9816
  at the lowest T: repair_sends per seed [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
- K-L10: share_T at lowest T 0.8771, at T=0.4 0.9794; mid-level crossing 0.1726 s (per-seed 0.172 0.173 0.173 0.172 0.173 0.173 0.173 0.173 0.173 0.173 0.173 0.173); 90% crossing 0.1534; median first-resend age (report-driven, T=0.4 H=0 run, median over seeds) 0.1212; srtt_mean/2 0.0507; sum 0.1718 s (crossing - sum = +0.7 ms); pooled sweep-run median age 0.1212
  curve: 0.150:0.8771 0.155:0.8834 0.160:0.8875 0.165:0.8908 0.170:0.8949 0.175:0.9595 0.180:0.9600 0.185:0.9604 0.190:0.9609 0.195:0.9661 0.200:0.9661 0.205:0.9662 0.210:0.9663 0.215:0.9670
  at the lowest T: repair_sends per seed [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
- K-L10b: share_T at lowest T 0.8949, at T=0.4 0.9790; mid-level crossing 0.2025 s (per-seed 0.203 0.202 0.203 0.202 0.202 0.203 0.202 0.202 0.202 0.202 0.202 0.203); 90% crossing 0.1686; median first-resend age (report-driven, T=0.4 H=0 run, median over seeds) 0.1412; srtt_mean/2 0.0607; sum 0.2018 s (crossing - sum = +0.6 ms); pooled sweep-run median age 0.1412
  curve: 0.180:0.8949 0.185:0.9010 0.190:0.9053 0.195:0.9084 0.200:0.9125 0.205:0.9617 0.210:0.9622 0.215:0.9626 0.220:0.9631 0.225:0.9666 0.230:0.9668 0.235:0.9669 0.240:0.9671 0.245:0.9674
  at the lowest T: repair_sends per seed [598, 599, 599, 599, 598, 598, 599, 599, 599, 598, 599, 599]
- shift X15 -> X20 (mid): 14.9 ms (band 11 to 19): PASS; 90%: 15.35 ms
- shift K-L10 -> K-L10b (mid): 29.9 ms (band 25 to 35): PASS; 90%: 15.15 ms
### [q9]
### Q3c-9 continuity (K and P at the nudges; m = max(T bar, A/A floor))

  undetermined K-L10 T-nudge share_by_2 K: -0.0002778 [-0.0005278, -1.827e-17] vs m=0.000333: undetermined
  undetermined CL T-nudge share_by_2 K: 0.002913 [0, 0.00874] vs m=0.00321: undetermined
  undetermined CL T-nudge share_by_2 P: 0.001457 [0, 0.00437] vs m=0.00321: undetermined
  undetermined CL T-nudge ooo_per_delivered K: 0.0004643 [0, 0.001393] vs m=1e-05: undetermined
  undetermined CL T-nudge ooo_per_delivered P: 0.0002322 [0, 0.0006965] vs m=1e-05: undetermined
  undetermined OV T-nudge ooo_per_delivered K: -2.436e-05 [-5.558e-05, 9.459e-06] vs m=8.33e-06: undetermined
  undetermined OV T-nudge ooo_per_delivered P: 2.155e-07 [-1.285e-05, 1.018e-05] vs m=8.33e-06: undetermined
  216 K and P statistics: 0 shown, 7 undetermined, 209 equivalent

per-seed positive control in E-I2n:
  literal placement (dead-tail units = every seq >= dead[0]): usable 0 of 12; first reason: ramp too wide: min 0.0134, 0.99p + 2 ms = 1.5652, p = 1.579
  deviation placement (dead-tail units = first send inside the outage): usable 2 of 12: [0, 2]
     skipped seed 1: ramp too wide: min 1.5049, 0.99p + 2 ms = 1.5434, p = 1.557
     skipped seed 3: ramp too wide: min 1.5076, 0.99p + 2 ms = 1.5464, p = 1.560
     skipped seed 4: ramp too wide: min 1.5247, 0.99p + 2 ms = 1.5266, p = 1.540
     skipped seed 5: ramp too wide: min 1.5281, 0.99p + 2 ms = 1.5672, p = 1.581
     skipped seed 6: ramp too wide: min 1.5157, 0.99p + 2 ms = 1.5157, p = 1.529
     skipped seed 7: ramp too wide: min 1.5075, 0.99p + 2 ms = 1.5088, p = 1.522
     skipped seed 8: ramp too wide: min 1.5062, 0.99p + 2 ms = 1.5078, p = 1.521
     skipped seed 9: ramp too wide: min 1.5230, 0.99p + 2 ms = 1.5236, p = 1.537
     skipped seed 10: ramp too wide: min 1.5078, 0.99p + 2 ms = 1.5088, p = 1.522
     skipped seed 11: ramp too wide: min 1.5092, 0.99p + 2 ms = 1.5563, p = 1.570
  at least 8 of 12 seeds usable: NO -> control NOT PLACED (a finding)
   seed  0 p = 1.579, ages in [1.5721, 1.5769] (49 units), K-hat 0.00403 P-hat 0.00202
   seed  2 p = 1.534, ages in [1.5267, 1.5315] (49 units), K-hat 0.00403 P-hat 0.00202
  descriptive (n = 2 seeds): mean K 0.00181 vs mean K-hat 0.00403 (ratio 0.45); read: 0.001811 [-0.0004116, 0.004034] vs m=8.23e-05: undetermined
  mean P 0.00091 vs mean P-hat 0.00202 (ratio 0.45); read: 0.0009056 [-0.0002058, 0.002017] vs m=8.23e-05: undetermined
  share_T at the five points: [0.99597, 0.99708, 1.0, 1.0, 1.0]
### [q10]
### Q3c-10 stall parts (K cells; S4 at T = 0.4, H = inf; C3 reported)

- K-I2 G-T0p4-Hinf (P = 0.025, owd = 0.005; bound (i) = P + owd + 5 ms = 0.035; bound (o) = owd + 1/rate + 2 ms = 0.0270)
   seed  0: no outage episode
   seed  1: no outage episode
   seed  2: no outage episode
   seed  3: no outage episode
   seed  4: no outage episode
   seed  5: no outage episode
   seed  6: no outage episode
   seed  7: no outage episode
   seed  8: no outage episode
   seed  9: no outage episode
   seed 10: episode 20.005 + 1.513 s; (o) 0.0000 (oo) 1.4999 (i) 0.0130 (ii) 0.0000 (iii) 0.0000
   seed 11: no outage episode
   seeds with an outage episode 1; (i) within bound 1; (o) within bound where non-zero 1
- K-I2 C3 (P = 0.025, owd = 0.005; bound (i) = P + owd + 5 ms = 0.035; bound (o) = owd + 1/rate + 2 ms = 0.0270)
   seed  0: no outage episode
   seed  1: no outage episode
   seed  2: no outage episode
   seed  3: no outage episode
   seed  4: no outage episode
   seed  5: no outage episode
   seed  6: no outage episode
   seed  7: no outage episode
   seed  8: no outage episode
   seed  9: no outage episode
   seed 10: episode 20.005 + 1.533 s; (o) 0.0000 (oo) 1.4999 (i) 0.0329 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed 11: no outage episode
   seeds with an outage episode 1; (i) within bound 1; (o) within bound where non-zero 1
- K-L10 G-T0p4-Hinf (P = 0.100, owd = 0.050; bound (i) = P + owd + 5 ms = 0.155; bound (o) = owd + 1/rate + 2 ms = 0.0720)
   seed  0: episode 20.051 + 1.523 s; (o) 0.0000 (oo) 1.4992 (i) 0.0240 (ii) 0.0000 (iii) 0.0000
   seed  1: episode 20.010 + 1.662 s; (o) 0.0397 (oo) 1.5000 (i) 0.0208 (ii) 0.1010 (iii) 0.0000
   seed  2: no outage episode
   seed  3: no outage episode
   seed  4: episode 19.970 + 1.703 s; (o) 0.0796 (oo) 1.5000 (i) 0.0203 (ii) 0.1032 (iii) 0.0000
   seed  5: no outage episode
   seed  6: episode 19.871 + 1.862 s; (o) 0.1791 (oo) 1.5000 (i) 0.0209 (ii) 0.1624 (iii) 0.0000
   seed  7: episode 20.051 + 1.647 s; (o) 0.0000 (oo) 1.4992 (i) 0.0206 (ii) 0.1269 (iii) 0.0000
   seed  8: no outage episode
   seed  9: episode 20.031 + 1.901 s; (o) 0.0191 (oo) 1.5000 (i) 0.0208 (ii) 0.3609 (iii) 0.0000
   seed 10: episode 20.050 + 1.651 s; (o) 0.0000 (oo) 1.4999 (i) 0.0209 (ii) 0.1297 (iii) 0.0000
   seed 11: episode 20.031 + 1.921 s; (o) 0.0195 (oo) 1.5000 (i) 0.0201 (ii) 0.3818 (iii) 0.0000
   seeds with an outage episode 8; (i) within bound 8; (o) within bound where non-zero 6
- K-L10 C3 (P = 0.100, owd = 0.050; bound (i) = P + owd + 5 ms = 0.155; bound (o) = owd + 1/rate + 2 ms = 0.0720)
   seed  0: episode 20.051 + 1.723 s; (o) 0.0000 (oo) 1.4992 (i) 0.2238 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  1: episode 20.010 + 1.685 s; (o) 0.0397 (oo) 1.5000 (i) 0.1456 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  2: no outage episode
   seed  3: no outage episode
   seed  4: episode 19.970 + 1.703 s; (o) 0.0796 (oo) 1.5000 (i) 0.1235 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  5: no outage episode
   seed  6: episode 19.871 + 1.862 s; (o) 0.1791 (oo) 1.5000 (i) 0.1834 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  7: episode 20.051 + 1.647 s; (o) 0.0000 (oo) 1.4992 (i) 0.1475 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  8: no outage episode
   seed  9: episode 20.031 + 1.901 s; (o) 0.0191 (oo) 1.5000 (i) 0.3817 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed 10: episode 20.050 + 1.751 s; (o) 0.0000 (oo) 1.4999 (i) 0.2507 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed 11: episode 20.031 + 1.921 s; (o) 0.0195 (oo) 1.5000 (i) 0.4019 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seeds with an outage episode 8; (i) within bound 3; (o) within bound where non-zero 6
- E-I2 G-T0p4-Hinf (P = 0.025, owd = 0.005; bound (i) = P + owd + 5 ms = 0.035; bound (o) = owd + 1/rate + 2 ms = 0.0071)
   seed  0: episode 1.106 + 0.087 s; (o) 0.0786 (oo) 0.0084 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed  1: episode 1.064 + 0.130 s; (o) 0.1211 (oo) 0.0086 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed  2: episode 1.012 + 0.184 s; (o) 0.1730 (oo) 0.0106 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed  3: episode 1.154 + 0.047 s; (o) 0.0311 (oo) 0.0159 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed  4: episode 1.096 + 0.098 s; (o) 0.0888 (oo) 0.0094 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed  5: episode 1.096 + 0.110 s; (o) 0.0892 (oo) 0.0205 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed  6: episode 0.507 + 0.697 s; (o) 0.6780 (oo) 0.0187 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed  7: episode 1.157 + 0.037 s; (o) 0.0280 (oo) 0.0089 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed  8: episode 1.020 + 0.175 s; (o) 0.1647 (oo) 0.0101 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed  9: episode 1.081 + 0.108 s; (o) 0.1041 (oo) 0.0040 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed 10: episode 1.125 + 0.071 s; (o) 0.0603 (oo) 0.0105 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed 11: episode 0.748 + 0.457 s; (o) 0.4373 (oo) 0.0194 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seeds with an outage episode 12; (i) within bound 12; (o) within bound where non-zero 0
- E-I2 C3 (P = 0.025, owd = 0.005; bound (i) = P + owd + 5 ms = 0.035; bound (o) = owd + 1/rate + 2 ms = 0.0071)
   seed  0: episode 1.106 + 0.087 s; (o) 0.0786 (oo) 0.0084 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  1: episode 1.064 + 0.130 s; (o) 0.1211 (oo) 0.0086 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  2: episode 1.012 + 0.184 s; (o) 0.1730 (oo) 0.0106 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  3: episode 1.154 + 0.047 s; (o) 0.0311 (oo) 0.0159 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  4: episode 1.096 + 0.098 s; (o) 0.0888 (oo) 0.0094 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  5: episode 1.096 + 0.110 s; (o) 0.0892 (oo) 0.0205 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  6: episode 0.507 + 0.697 s; (o) 0.6780 (oo) 0.0187 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  7: episode 1.157 + 0.037 s; (o) 0.0280 (oo) 0.0089 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  8: episode 1.020 + 0.175 s; (o) 0.1647 (oo) 0.0101 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  9: episode 1.081 + 0.108 s; (o) 0.1041 (oo) 0.0040 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed 10: episode 1.125 + 0.071 s; (o) 0.0603 (oo) 0.0105 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed 11: episode 0.748 + 0.457 s; (o) 0.4373 (oo) 0.0194 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seeds with an outage episode 12; (i) within bound 12; (o) within bound where non-zero 0
- E-L10 G-T0p4-Hinf (P = 0.100, owd = 0.050; bound (i) = P + owd + 5 ms = 0.155; bound (o) = owd + 1/rate + 2 ms = 0.0521)
   seed  0: episode 0.050 + 1.424 s; (o) 1.1798 (oo) 0.2437 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed  1: episode 0.051 + 1.370 s; (o) 1.1786 (oo) 0.1916 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed  2: episode 0.051 + 1.472 s; (o) 1.1787 (oo) 0.2937 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed  3: episode 0.051 + 1.464 s; (o) 1.1787 (oo) 0.2848 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed  4: episode 0.051 + 1.370 s; (o) 1.1794 (oo) 0.1903 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed  5: episode 0.050 + 1.471 s; (o) 1.1798 (oo) 0.2914 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed  6: episode 0.051 + 1.396 s; (o) 1.1794 (oo) 0.2169 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed  7: episode 0.052 + 1.368 s; (o) 1.1785 (oo) 0.1897 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed  8: episode 0.052 + 1.377 s; (o) 1.1777 (oo) 0.1989 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed  9: episode 0.051 + 1.423 s; (o) 1.1794 (oo) 0.2432 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed 10: episode 0.052 + 1.366 s; (o) 1.1780 (oo) 0.1877 (i) 0.0000 (ii) 0.0000 (iii) 0.0000
   seed 11: episode 0.052 + 2.754 s; (o) 1.1777 (oo) 1.5000 (i) 0.0766 (ii) 0.0000 (iii) 0.0000
   seeds with an outage episode 12; (i) within bound 12; (o) within bound where non-zero 0
- E-L10 C3 (P = 0.100, owd = 0.050; bound (i) = P + owd + 5 ms = 0.155; bound (o) = owd + 1/rate + 2 ms = 0.0521)
   seed  0: episode 0.050 + 1.424 s; (o) 1.1798 (oo) 0.2437 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  1: episode 0.051 + 1.370 s; (o) 1.1786 (oo) 0.1916 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  2: episode 0.051 + 1.472 s; (o) 1.1787 (oo) 0.2937 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  3: episode 0.051 + 1.464 s; (o) 1.1787 (oo) 0.2848 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  4: episode 0.051 + 1.370 s; (o) 1.1794 (oo) 0.1903 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  5: episode 0.050 + 1.471 s; (o) 1.1798 (oo) 0.2914 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  6: episode 0.051 + 1.396 s; (o) 1.1794 (oo) 0.2169 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  7: episode 0.052 + 1.368 s; (o) 1.1785 (oo) 0.1897 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  8: episode 0.052 + 1.377 s; (o) 1.1777 (oo) 0.1989 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  9: episode 0.051 + 1.423 s; (o) 1.1794 (oo) 0.2432 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed 10: episode 0.052 + 1.366 s; (o) 1.1780 (oo) 0.1877 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed 11: episode 0.052 + 1.520 s; (o) 1.1777 (oo) 0.3421 (i) 0.0000 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seeds with an outage episode 12; (i) within bound 12; (o) within bound where non-zero 0
- M-I2 G-T0p4-Hinf (P = 0.025, owd = 0.005; bound (i) = P + owd + 5 ms = 0.035; bound (o) = owd + 1/rate + 2 ms = 0.0071)
   seed  0: no outage episode
   seed  1: no outage episode
   seed  2: episode 0.398 + 1.607 s; (o) 0.0926 (oo) 1.5000 (i) 0.0143 (ii) 0.0000 (iii) 0.0000
   seed  3: episode 0.313 + 1.680 s; (o) 0.1776 (oo) 1.5000 (i) 0.0019 (ii) 0.0000 (iii) 0.0000
   seed  4: episode 0.352 + 1.660 s; (o) 0.1393 (oo) 1.5000 (i) 0.0208 (ii) 0.0000 (iii) 0.0000
   seed  5: episode 0.406 + 1.608 s; (o) 0.0845 (oo) 1.5000 (i) 0.0232 (ii) 0.0000 (iii) 0.0000
   seed  6: episode 0.486 + 1.515 s; (o) 0.0045 (oo) 1.5000 (i) 0.0103 (ii) 0.0000 (iii) 0.0000
   seed  7: episode 0.444 + 1.549 s; (o) 0.0473 (oo) 1.5000 (i) 0.0016 (ii) 0.0000 (iii) 0.0000
   seed  8: no outage episode
   seed  9: episode 0.373 + 1.639 s; (o) 0.1182 (oo) 1.5000 (i) 0.0204 (ii) 0.0000 (iii) 0.0000
   seed 10: episode 0.057 + 1.936 s; (o) 0.4337 (oo) 1.5000 (i) 0.0022 (ii) 0.0000 (iii) 0.0000
   seed 11: episode 0.466 + 1.554 s; (o) 0.0247 (oo) 1.5000 (i) 0.0291 (ii) 0.0000 (iii) 0.0000
   seeds with an outage episode 9; (i) within bound 9; (o) within bound where non-zero 1
- M-I2 C3 (P = 0.025, owd = 0.005; bound (i) = P + owd + 5 ms = 0.035; bound (o) = owd + 1/rate + 2 ms = 0.0071)
   seed  0: no outage episode
   seed  1: no outage episode
   seed  2: episode 0.398 + 1.791 s; (o) 0.0926 (oo) 1.5000 (i) 0.1988 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  3: episode 0.313 + 2.004 s; (o) 0.1776 (oo) 1.5000 (i) 0.3259 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  4: episode 0.352 + 1.660 s; (o) 0.1393 (oo) 1.5000 (i) 0.0208 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  5: episode 0.406 + 1.783 s; (o) 0.0845 (oo) 1.5000 (i) 0.1986 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  6: episode 0.486 + 1.781 s; (o) 0.0045 (oo) 1.5000 (i) 0.2768 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  7: episode 0.444 + 1.940 s; (o) 0.0473 (oo) 1.5000 (i) 0.3932 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  8: no outage episode
   seed  9: episode 0.373 + 1.809 s; (o) 0.1182 (oo) 1.5000 (i) 0.1907 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed 10: episode 0.057 + 1.936 s; (o) 0.4337 (oo) 1.5000 (i) 0.0022 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed 11: episode 0.466 + 1.528 s; (o) 0.0247 (oo) 1.5000 (i) 0.0038 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seeds with an outage episode 9; (i) within bound 3; (o) within bound where non-zero 1
- M-L10 G-T0p4-Hinf (P = 0.100, owd = 0.050; bound (i) = P + owd + 5 ms = 0.155; bound (o) = owd + 1/rate + 2 ms = 0.0521)
   seed  0: episode 0.050 + 2.024 s; (o) 0.4857 (oo) 1.5000 (i) 0.0384 (ii) 0.0000 (iii) 0.0000
   seed  1: episode 0.051 + 2.044 s; (o) 0.4844 (oo) 1.5000 (i) 0.0598 (ii) 0.0000 (iii) 0.0000
   seed  2: episode 0.051 + 1.998 s; (o) 0.4846 (oo) 1.5000 (i) 0.0134 (ii) 0.0000 (iii) 0.0000
   seed  3: episode 0.051 + 2.049 s; (o) 0.4845 (oo) 1.5000 (i) 0.0641 (ii) 0.0000 (iii) 0.0000
   seed  4: episode 0.051 + 2.023 s; (o) 0.4853 (oo) 1.5000 (i) 0.0377 (ii) 0.0000 (iii) 0.0000
   seed  5: episode 0.050 + 2.135 s; (o) 0.4857 (oo) 1.5000 (i) 0.1492 (ii) 0.0000 (iii) 0.0000
   seed  6: episode 0.051 + 2.083 s; (o) 0.4853 (oo) 1.5000 (i) 0.0973 (ii) 0.0000 (iii) 0.0000
   seed  7: episode 0.052 + 2.046 s; (o) 0.4843 (oo) 1.5000 (i) 0.0616 (ii) 0.0000 (iii) 0.0000
   seed  8: episode 0.052 + 2.042 s; (o) 0.4836 (oo) 1.5000 (i) 0.0585 (ii) 0.0000 (iii) 0.0000
   seed  9: episode 0.051 + 2.022 s; (o) 0.4852 (oo) 1.5000 (i) 0.0369 (ii) 0.0000 (iii) 0.0000
   seed 10: episode 0.052 + 2.048 s; (o) 0.4839 (oo) 1.5000 (i) 0.0641 (ii) 0.0000 (iii) 0.0000
   seed 11: episode 0.052 + 2.154 s; (o) 0.4836 (oo) 1.5000 (i) 0.1707 (ii) 0.0000 (iii) 0.0000
   seeds with an outage episode 12; (i) within bound 11; (o) within bound where non-zero 0
- M-L10 C3 (P = 0.100, owd = 0.050; bound (i) = P + owd + 5 ms = 0.155; bound (o) = owd + 1/rate + 2 ms = 0.0521)
   seed  0: episode 0.050 + 2.873 s; (o) 0.4857 (oo) 1.5000 (i) 0.8874 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  1: episode 0.051 + 3.044 s; (o) 0.4844 (oo) 1.5000 (i) 1.0598 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  2: episode 0.051 + 2.960 s; (o) 0.4846 (oo) 1.5000 (i) 0.9758 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  3: episode 0.051 + 2.961 s; (o) 0.4845 (oo) 1.5000 (i) 0.9770 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  4: episode 0.051 + 2.860 s; (o) 0.4853 (oo) 1.5000 (i) 0.8745 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  5: episode 0.050 + 2.929 s; (o) 0.4857 (oo) 1.5000 (i) 0.9434 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  6: episode 0.051 + 2.905 s; (o) 0.4853 (oo) 1.5000 (i) 0.9196 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  7: episode 0.052 + 2.887 s; (o) 0.4843 (oo) 1.5000 (i) 0.9022 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  8: episode 0.052 + 2.879 s; (o) 0.4836 (oo) 1.5000 (i) 0.8951 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  9: episode 0.051 + 2.947 s; (o) 0.4852 (oo) 1.5000 (i) 0.9619 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed 10: episode 0.052 + 2.867 s; (o) 0.4839 (oo) 1.5000 (i) 0.8832 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed 11: episode 0.052 + 2.954 s; (o) 0.4836 (oo) 1.5000 (i) 0.9706 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seeds with an outage episode 12; (i) within bound 0; (o) within bound where non-zero 0
- CL G-T0p4-Hinf (P = 0.025, owd = 0.005; bound (i) = P + owd + 5 ms = 0.035; bound (o) = owd + 1/rate + 2 ms = 0.0071)
   seed  0: episode 1.975 + 1.536 s; (o) 0.0296 (oo) 1.5000 (i) 0.0069 (ii) 0.0000 (iii) 0.0000
   seed  1: episode 1.762 + 1.755 s; (o) 0.2430 (oo) 1.5000 (i) 0.0121 (ii) 0.0000 (iii) 0.0000
   seed  2: episode 1.745 + 1.785 s; (o) 0.2601 (oo) 1.5000 (i) 0.0250 (ii) 0.0000 (iii) 0.0000
   seed  3: episode 1.996 + 1.522 s; (o) 0.0087 (oo) 1.5000 (i) 0.0133 (ii) 0.0000 (iii) 0.0000
   seed  4: episode 1.991 + 1.521 s; (o) 0.0142 (oo) 1.5000 (i) 0.0063 (ii) 0.0000 (iii) 0.0000
   seed  5: episode 1.846 + 1.668 s; (o) 0.1592 (oo) 1.5000 (i) 0.0092 (ii) 0.0000 (iii) 0.0000
   seed  6: episode 1.950 + 1.576 s; (o) 0.0548 (oo) 1.5000 (i) 0.0215 (ii) 0.0000 (iii) 0.0000
   seed  7: episode 1.982 + 1.535 s; (o) 0.0232 (oo) 1.5000 (i) 0.0122 (ii) 0.0000 (iii) 0.0000
   seed  8: episode 1.715 + 1.802 s; (o) 0.2896 (oo) 1.5000 (i) 0.0119 (ii) 0.0000 (iii) 0.0000
   seed  9: episode 1.999 + 1.512 s; (o) 0.0062 (oo) 1.5000 (i) 0.0058 (ii) 0.0000 (iii) 0.0000
   seed 10: episode 1.928 + 1.590 s; (o) 0.0773 (oo) 1.5000 (i) 0.0130 (ii) 0.0000 (iii) 0.0000
   seed 11: episode 1.645 + 1.875 s; (o) 0.3604 (oo) 1.5000 (i) 0.0145 (ii) 0.0000 (iii) 0.0000
   seeds with an outage episode 12; (i) within bound 12; (o) within bound where non-zero 1
- CL C3 (P = 0.025, owd = 0.005; bound (i) = P + owd + 5 ms = 0.035; bound (o) = owd + 1/rate + 2 ms = 0.0071)
   seed  0: episode 1.975 + 1.548 s; (o) 0.0296 (oo) 1.5000 (i) 0.0186 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  1: episode 1.762 + 1.766 s; (o) 0.2430 (oo) 1.5000 (i) 0.0227 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  2: episode 1.745 + 1.773 s; (o) 0.2601 (oo) 1.5000 (i) 0.0128 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  3: episode 1.996 + 8.458 s; (o) 0.0087 (oo) 1.5000 (i) 6.9496 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  4: episode 1.991 + 8.496 s; (o) 0.0142 (oo) 1.5000 (i) 6.9815 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  5: episode 1.846 + 1.680 s; (o) 0.1592 (oo) 1.5000 (i) 0.0204 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  6: episode 1.950 + 8.487 s; (o) 0.0548 (oo) 1.5000 (i) 6.9325 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  7: episode 1.982 + 1.546 s; (o) 0.0232 (oo) 1.5000 (i) 0.0229 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  8: episode 1.715 + 1.814 s; (o) 0.2896 (oo) 1.5000 (i) 0.0245 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed  9: episode 1.999 + 8.513 s; (o) 0.0062 (oo) 1.5000 (i) 7.0066 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed 10: episode 1.928 + 8.616 s; (o) 0.0773 (oo) 1.5000 (i) 7.0382 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seed 11: episode 1.645 + 1.887 s; (o) 0.3604 (oo) 1.5000 (i) 0.0270 (ii) 0.0000 (iii) 0.0000  [no floor seen after the outage]
   seeds with an outage episode 12; (i) within bound 7; (o) within bound where non-zero 1
### [aa]
### A/A self-check (frozen nf_stage3c.json)

324 readings over 12 cells: 240 equivalent, 76 undetermined, 2 shown
  shown: X15-stall_total 59.28 [13.28, 108.9] vs m=1: shown larger
  shown: X20-stall_total -53.85 [-104.5, -6.299] vs m=1: shown smaller

undetermined (cell-key: mean [lo, hi] vs m):
  E-I2-dh_p99: -9.942 [-34.17, 14.16] vs m=1: undetermined
  E-I2-v_p99: -9.904 [-34.2, 14.16] vs m=1: undetermined
  E-I2-stall_total: 1.867 [-21.39, 25.42] vs m=1: undetermined
  E-I2-tail_time: -6.975 [-31.75, 16.93] vs m=1: undetermined
  E-I2-finish: -6.975 [-31.75, 16.93] vs m=1: undetermined
  E-I2n-stall_total: -5.094 [-28.01, 15.25] vs m=1: undetermined
  E-I2n-tail_time: -4.722 [-28.88, 16.38] vs m=1: undetermined
  E-I2n-finish: -4.722 [-28.88, 16.38] vs m=1: undetermined
  E-L10-share_by_2: -0.001948 [-0.005845, 0] vs m=9.74e-05: undetermined
  E-L10-dh_p99: 15.55 [-22.87, 45.37] vs m=1: undetermined
  E-L10-v_p99: 15.6 [-22.89, 45.53] vs m=1: undetermined
  E-L10-stall_total: 9.845 [0.07845, 27.6] vs m=1: undetermined
  E-L10-tail_time: 22.12 [-3.254, 44.51] vs m=1: undetermined
  E-L10-finish: 22.12 [-3.254, 44.51] vs m=1: undetermined
  M-I2-dh_p50: -2.539 [-9.74, 4.898] vs m=1: undetermined
  M-I2-dh_p90: -2.603 [-11.58, 7.024] vs m=1: undetermined
  M-I2-dh_p99: -5.278 [-15.95, 6.054] vs m=1: undetermined
  M-I2-v_p99: -5.314 [-15.97, 5.987] vs m=1: undetermined
  M-I2-stall_total: -3.755 [-9.828, 2.148] vs m=1: undetermined
  M-I2-finish: -2.572 [-9.68, 4.583] vs m=1: undetermined
  M-I2-resume_delay: -2.52 [-9.718, 4.933] vs m=1: undetermined
  M-L10-share_by_2: -0.02622 [-0.07496, 0.01859] vs m=0.00221: undetermined
  M-L10-dh_p50: 10.55 [-19.1, 41.85] vs m=1: undetermined
  M-L10-dh_p90: 46.74 [-17.81, 122.4] vs m=1: undetermined
  M-L10-dh_p99: 61.34 [-13.04, 153.7] vs m=1: undetermined
  M-L10-v_p50: 1.956 [-6.34, 13.36] vs m=1: undetermined
  M-L10-v_p99: 57.32 [-18.86, 152.4] vs m=1: undetermined
  M-L10-stall_total: 4.113 [-21.29, 29.6] vs m=1: undetermined
  M-L10-tail_time: -1.367 [-3.656, 0.1025] vs m=1: undetermined
  M-L10-finish: 4.113 [-21.29, 29.6] vs m=1: undetermined
  M-L10-resume_delay: 5.48 [-18.92, 30.83] vs m=1: undetermined
  K-I2-dh_p99: 4.082 [-5.315, 15.17] vs m=1: undetermined
  K-I2-v_p99: 4.082 [-5.315, 15.17] vs m=1: undetermined
  K-I2-stall_total: 23.64 [-8.182, 51.22] vs m=1: undetermined
  K-L10-share_by_0p4: 0.001028 [-0.0003056, 0.00275] vs m=0.00117: undetermined
  K-L10-dh_p50: -0.6462 [-3.044, 1.013] vs m=1: undetermined
  K-L10-dh_p90: -0.9738 [-3.288, 0.604] vs m=1: undetermined
  K-L10-dh_p99: -32.85 [-118, 22.56] vs m=1: undetermined
  K-L10-v_mean: -1.377 [-4.056, 0.5633] vs m=3.38: undetermined
  K-L10-v_p50: -0.6729 [-3.012, 0.9233] vs m=1: undetermined
  K-L10-v_p99: -32.88 [-117.9, 22.56] vs m=1: undetermined
  K-L10-w_mean: -1.377 [-4.056, 0.5633] vs m=3.4: undetermined
  K-L10-repair_per_delivered: -0.03375 [-0.07765, 0.007143] vs m=0.0177: undetermined
  K-L10-stall_total: -53.1 [-248.3, 132.6] vs m=1: undetermined
  K-L10-R_mean: -0.3844 [-1.083, 0.3072] vs m=1: undetermined
  K-L10b-share_by_0p4: 0.001222 [-0.0001944, 0.003] vs m=0.00123: undetermined
  K-L10b-dh_p50: 2.227 [-2.549, 7.132] vs m=1: undetermined
  K-L10b-dh_p90: -1.439 [-3.414, -0.1915] vs m=1: undetermined
  K-L10b-dh_p99: -32.85 [-118, 22.56] vs m=1: undetermined
  K-L10b-v_mean: -1.202 [-3.784, 0.9959] vs m=3.75: undetermined
  K-L10b-v_p50: 2.209 [-2.554, 7.057] vs m=1: undetermined
  K-L10b-v_p99: -32.88 [-117.9, 22.56] vs m=1: undetermined
  K-L10b-w_mean: -1.202 [-3.784, 0.9959] vs m=3.78: undetermined
  K-L10b-repair_per_delivered: -0.03011 [-0.07522, 0.01103] vs m=0.0283: undetermined
  K-L10b-stall_total: -18.7 [-415, 358.4] vs m=1: undetermined
  K-L10b-tail_time: 1.227 [-0.6669, 3.684] vs m=1: undetermined
  K-L10b-finish: 1.227 [-0.6669, 3.684] vs m=1: undetermined
  K-L10b-R_mean: -0.4488 [-1.132, 0.2508] vs m=1: undetermined
  CL-dh_p50: -0.7392 [-9.523, 7.427] vs m=1: undetermined
  CL-dh_p90: -2.023 [-10.1, 5.084] vs m=1: undetermined
  CL-dh_p99: -0.6326 [-10.49, 9.722] vs m=1: undetermined
  CL-v_p99: 5.559 [-5.611, 18.04] vs m=1: undetermined
  CL-stall_total: -6.419 [-16.36, 3.65] vs m=1: undetermined
  CL-tail_time: -3.154 [-7.169, 0.9321] vs m=1: undetermined
  CL-finish: -5.173 [-14.44, 4.821] vs m=1: undetermined
  CL-resume_delay: -0.853 [-7.21, 5.597] vs m=1: undetermined
  OV-dh_p50: -1.994 [-12.64, 8.607] vs m=1: undetermined
  OV-dh_p90: -3.314 [-21.39, 15.08] vs m=1: undetermined
  OV-dh_p99: -4.122 [-23.64, 15.79] vs m=1: undetermined
  OV-v_p99: -0.785 [-1.811, 0.3161] vs m=1: undetermined
  OV-stall_total: -0.4745 [-23.46, 22.78] vs m=1: undetermined
  OV-tail_time: 3.045 [-2.356, 8.053] vs m=1: undetermined
  OV-finish: -0.4745 [-23.46, 22.78] vs m=1: undetermined
  X20-dh_p99: 1.448 [-0.3916, 4.598] vs m=1: undetermined
  X20-v_p99: 1.44 [-0.4263, 4.592] vs m=1: undetermined
  X20-R_mean: -0.3416 [-1.067, 0.3618] vs m=1: undetermined

floors (cell: readout = p95 |A - A'|, ms for times):
  E-I2    dh_p50=0.114; dh_p90=0.328; dh_p99=88.3; v_mean=2.53; v_p50=0.1; v_p99=88.4; w_mean=2.53; repair_per_delivered=0.0202; stall_total=83.4; tail_time=89.1; finish=89.1; R_mean=0.0435
  E-I2n   dh_p50=0.103; dh_p90=0.254; dh_p99=1.25; v_mean=0.175; v_p50=0.103; v_p99=1.2; w_mean=0.175; repair_per_delivered=0.00325; stall_total=65.7; tail_time=75.6; finish=75.6; R_mean=0.0439
  E-L10   share_by_0p4=0.00494; share_by_2=0.0105; dh_p50=0.493; dh_p90=1.69; dh_p99=110; v_mean=2.11; v_p50=0.536; v_p99=110; w_mean=2.11; repair_per_delivered=0.031; stall_total=51.4; tail_time=71.1; finish=71.1; R_mean=0.0242
  M-I2    share_by_0p4=0.0027; dh_p50=19.6; dh_p90=23.3; dh_p99=31.2; v_mean=7.67; v_p50=0.199; v_p99=31.2; w_mean=14.9; repair_per_delivered=0.135; stall_total=19.3; tail_time=2.2; finish=19.5; pause_time=0.0197; R_mean=0.0431; resume_delay=19.7
  M-L10   share_by_0p4=0.000741; share_by_2=0.174; dh_p50=97.5; dh_p90=250; dh_p99=303; v_mean=87; v_p50=38.8; v_p99=305; w_mean=110; repair_per_delivered=0.131; stall_total=70.6; tail_time=7.32; finish=70.6; pause_time=0.0681; R_mean=0.0505; resume_delay=68.1
  K-I2    share_by_0p4=0.000483; dh_p50=0.000411; dh_p90=0.000339; dh_p99=38.1; v_mean=0.787; v_p99=38.1; w_mean=0.787; repair_per_delivered=0.0179; stall_total=84.3; R_mean=1.92
  K-L10   share_by_0p4=0.00607; dh_p50=7.52; dh_p90=7.34; dh_p99=246; v_mean=8.66; v_p50=7.36; v_p99=246; w_mean=8.66; repair_per_delivered=0.181; stall_total=541; tail_time=1.15; finish=1.15; R_mean=2.18
  K-L10b  share_by_0p4=0.0064; share_by_2=0.00015; dh_p50=15; dh_p90=6.26; dh_p99=246; v_mean=8.23; v_p50=14.9; v_p99=246; w_mean=8.23; repair_per_delivered=0.177; stall_total=1.17e+03; tail_time=8.68; finish=8.68; R_mean=2.19
  CL      share_by_0p4=0.00124; share_by_2=0.00321; dh_p50=29; dh_p90=23.4; dh_p99=29.7; v_mean=2.67; v_p50=1.47; v_p99=41.4; w_mean=21; repair_per_delivered=0.00393; stall_total=35.7; tail_time=13.5; finish=34.5; pause_time=0.0377; R_mean=0.343; link_util_w=0.00255; resume_delay=16.9; resend_braked=6.65e+03
  OV      share_by_0p4=0.00157; share_by_2=0.0143; dh_p50=30.9; dh_p90=53.4; dh_p99=56.9; v_mean=1.43; v_p50=1.31; v_p99=3.35; w_mean=29.2; repair_per_delivered=0.00477; stall_total=66.3; tail_time=19.4; finish=66.3; pause_time=0.0584; R_mean=1.45; resend_braked=2.44e+03
  X15     dh_p90=1.2; dh_p99=1.04; v_mean=0.243; v_p99=0.994; w_mean=0.243; stall_total=210; tail_time=0.996; finish=0.996; R_mean=1.92
  X20     dh_p50=0.0187; dh_p90=0.911; dh_p99=8.88; v_mean=0.365; v_p99=8.88; w_mean=0.365; stall_total=216; tail_time=1.03; finish=1.03; R_mean=2.26
### [rep]
### A/A replication, phase variants 2 and 3 (unregistered), read against T_bar only

  SHOWN E-I2n stall_total: -13.83 [-28.47, -1.644] vs m=1: shown smaller
  SHOWN E-L10 dh_p99: -29.7 [-65.51, -1.347] vs m=1: shown smaller
  SHOWN E-L10 v_p99: -29.63 [-65.4, -1.353] vs m=1: shown smaller
  SHOWN E-L10 tail_time: -31.44 [-68.69, -1.548] vs m=1: shown smaller
  SHOWN E-L10 finish: -31.44 [-68.69, -1.548] vs m=1: shown smaller
  SHOWN K-L10 dh_p99: 78.46 [24.96, 156.4] vs m=1: shown larger
  SHOWN K-L10 v_p99: 78.41 [24.93, 156.3] vs m=1: shown larger
  SHOWN K-L10b dh_p99: 78.46 [24.96, 156.4] vs m=1: shown larger
  SHOWN K-L10b v_p99: 78.41 [24.93, 156.3] vs m=1: shown larger
  324 readings, 9 shown
### [sup]
### unregistered supplement: the cooldown also binds a seq's first resend (resend_gate cooldown_all), link cells, 12 seeds

| cell | arm | commit_viol (sum) | late_floor (sum) | share_by_0p4 | share_by_2 | registered arm's share_by_0p4 / share_by_2 | max copies/5ms | finished |
|---|---|---|---|---|---|---|---|---|
| OV | SUP-C1 | 0 | 0 | 0.1134 | 0.6436 | 0.1134 / 0.6436 | 10 | True |
| OV | SUP-C3 | 0 | 0 | 0.0701 | 0.6665 | 0.0674 / 0.6273 | 10 | True |
| OV | SUP-G-T0p4-H0 | 0 | 0 | 0.8318 | 0.8323 | 0.8308 / 0.8313 | 10 | True |
| OV | SUP-G-T0p4-Hinf | 0 | 0 | 0.8020 | 0.8323 | 0.8011 / 0.8313 | 10 | True |
| OV | SUP-G-T2-H0 | 0 | 0 | 0.1431 | 0.9469 | 0.1374 / 0.9396 | 10 | True |
| OV | SUP-G-T2-Hinf | 0 | 0 | 0.0701 | 0.9156 | 0.0674 / 0.9077 | 10 | True |
| OV | SUP-G-Tinf-Hinf | 0 | 0 | 0.0701 | 0.6665 | 0.0674 / 0.6273 | 10 | True |
| OV | SUP-S4b | 0 | 0 | 0.7679 | 0.7963 | 0.7671 / 0.7953 | 10 | True |
| CL | SUP-C1 | 0 | 0 | 0.3219 | 0.9874 | 0.2863 / 0.9874 | 10 | True |
| CL | SUP-C3 | 0 | 0 | 0.2020 | 0.9716 | 0.1990 / 0.9716 | 10 | True |
| CL | SUP-G-T0p4-H0 | 0 | 0 | 0.8476 | 0.8476 | 0.8476 / 0.8476 | 10 | True |
| CL | SUP-G-T0p4-Hinf | 0 | 0 | 0.8465 | 0.8476 | 0.8465 / 0.8476 | 10 | True |
| CL | SUP-G-T2-H0 | 0 | 0 | 0.2506 | 0.9983 | 0.2167 / 0.9983 | 10 | True |
| CL | SUP-G-T2-Hinf | 0 | 0 | 0.2029 | 0.9910 | 0.1990 / 0.9910 | 10 | True |
| CL | SUP-G-Tinf-Hinf | 0 | 0 | 0.2020 | 0.9716 | 0.1990 / 0.9716 | 10 | True |
| CL | SUP-S4b | 0 | 0 | 0.8465 | 0.8476 | 0.8465 / 0.8476 | 10 | True |
### [windows]
### per-1 s window series, OV and CL (seed means over 12 seeds; window i = [i, i+1) s): first sends, copies sent, copies refused by the
### budget, pause time (s), link busy (fraction of the window), hand-over instants blocked by the pause, resends refused by the cooldown

#### OV C1

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10136 | 596 | 23160 | 0.661 | 1.000 | 7929 | 104 |
| 1 | 9198 | 590 | 29816 | 1.000 | 1.000 | 12000 | 0 |
| 2 | 9238 | 571 | 29796 | 1.000 | 1.000 | 12000 | 0 |
| 3 | 9243 | 568 | 29771 | 1.000 | 1.000 | 12000 | 0 |
| 4 | 9236 | 569 | 29775 | 1.000 | 1.000 | 12000 | 0 |
| 5 | 9242 | 572 | 29773 | 1.000 | 1.000 | 12000 | 0 |
| 6 | 9230 | 572 | 29776 | 1.000 | 1.000 | 12000 | 0 |
| 7 | 9254 | 565 | 29769 | 1.000 | 1.000 | 12000 | 0 |
| 8 | 9234 | 574 | 29769 | 1.000 | 1.000 | 12000 | 0 |
| 9 | 9246 | 569 | 29762 | 1.000 | 1.000 | 12000 | 0 |
| 10 | 9241 | 566 | 29773 | 1.000 | 1.000 | 12000 | 0 |
| 11 | 9228 | 575 | 29771 | 1.000 | 1.000 | 12000 | 0 |
| 12 | 8273 | 561 | 28518 | 0.894 | 0.989 | 10730 | 0 |
| 13 | 0 | 7 | 52 | 0.000 | 0.009 | 0 | 0 |
| 14 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### OV C3

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10214 | 708 | 22641 | 0.661 | 1.000 | 7937 | 12458 |
| 1 | 9326 | 672 | 29067 | 1.000 | 1.000 | 12000 | 16579 |
| 2 | 9336 | 666 | 29090 | 1.000 | 1.000 | 12000 | 16097 |
| 3 | 9326 | 672 | 29056 | 1.000 | 1.000 | 12000 | 16617 |
| 4 | 9331 | 671 | 29061 | 1.000 | 1.000 | 12000 | 16594 |
| 5 | 9325 | 673 | 29066 | 1.000 | 1.000 | 12000 | 16583 |
| 6 | 9326 | 674 | 29054 | 1.000 | 1.000 | 12000 | 16786 |
| 7 | 9338 | 664 | 29076 | 1.000 | 1.000 | 12000 | 16262 |
| 8 | 9325 | 674 | 29040 | 1.000 | 1.000 | 12000 | 16922 |
| 9 | 9328 | 672 | 29045 | 1.000 | 1.000 | 12000 | 16797 |
| 10 | 9333 | 668 | 29065 | 1.000 | 1.000 | 12000 | 16497 |
| 11 | 9326 | 674 | 29047 | 1.000 | 1.000 | 12000 | 16878 |
| 12 | 7166 | 588 | 24195 | 0.768 | 0.868 | 9218 | 14433 |
| 13 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |
| 14 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### OV C3S

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10214 | 708 | 22641 | 0.661 | 1.000 | 7937 | 12458 |
| 1 | 9326 | 672 | 29067 | 1.000 | 1.000 | 12000 | 16579 |
| 2 | 9336 | 666 | 29090 | 1.000 | 1.000 | 12000 | 16097 |
| 3 | 9326 | 672 | 29056 | 1.000 | 1.000 | 12000 | 16617 |
| 4 | 9331 | 671 | 29061 | 1.000 | 1.000 | 12000 | 16594 |
| 5 | 9325 | 673 | 29066 | 1.000 | 1.000 | 12000 | 16583 |
| 6 | 9326 | 674 | 29054 | 1.000 | 1.000 | 12000 | 16786 |
| 7 | 9338 | 664 | 29076 | 1.000 | 1.000 | 12000 | 16262 |
| 8 | 9325 | 674 | 29040 | 1.000 | 1.000 | 12000 | 16922 |
| 9 | 9328 | 672 | 29045 | 1.000 | 1.000 | 12000 | 16797 |
| 10 | 9333 | 668 | 29065 | 1.000 | 1.000 | 12000 | 16497 |
| 11 | 9326 | 674 | 29047 | 1.000 | 1.000 | 12000 | 16878 |
| 12 | 7166 | 588 | 24195 | 0.768 | 0.868 | 9218 | 14433 |
| 13 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |
| 14 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### OV G-T0p4-H0

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10217 | 697 | 22526 | 0.657 | 1.000 | 7885 | 11930 |
| 1 | 9577 | 346 | 15606 | 1.000 | 1.000 | 12000 | 2586 |
| 2 | 9962 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 3 | 9959 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 4 | 9960 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 5 | 9959 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 6 | 9960 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 7 | 9961 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 8 | 9959 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 9 | 9960 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 10 | 2940 | 0 | 0 | 0.296 | 0.384 | 3548 | 0 |
| 11 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### OV G-T0p4-Hinf

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10217 | 697 | 22526 | 0.657 | 1.000 | 7885 | 11930 |
| 1 | 9577 | 346 | 15606 | 1.000 | 1.000 | 12000 | 2586 |
| 2 | 9962 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 3 | 9959 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 4 | 9960 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 5 | 9959 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 6 | 9960 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 7 | 9961 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 8 | 9959 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 9 | 9960 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 10 | 2940 | 0 | 0 | 0.296 | 0.384 | 3548 | 0 |
| 11 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### OV G-T2-H0

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10214 | 708 | 22641 | 0.661 | 1.000 | 7937 | 12458 |
| 1 | 9326 | 672 | 29067 | 1.000 | 1.000 | 12000 | 16579 |
| 2 | 9336 | 666 | 29090 | 1.000 | 1.000 | 12000 | 16097 |
| 3 | 9326 | 672 | 29056 | 1.000 | 1.000 | 12000 | 16617 |
| 4 | 9331 | 671 | 29061 | 1.000 | 1.000 | 12000 | 16594 |
| 5 | 9325 | 673 | 29066 | 1.000 | 1.000 | 12000 | 16583 |
| 6 | 9326 | 674 | 29054 | 1.000 | 1.000 | 12000 | 16786 |
| 7 | 9333 | 664 | 29081 | 1.000 | 1.000 | 12000 | 16117 |
| 8 | 9451 | 474 | 20970 | 1.000 | 1.000 | 12000 | 5365 |
| 9 | 9960 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 10 | 9960 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 11 | 8916 | 0 | 0 | 0.895 | 0.984 | 10744 | 0 |
| 12 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### OV G-T2-Hinf

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10214 | 708 | 22641 | 0.661 | 1.000 | 7937 | 12458 |
| 1 | 9326 | 672 | 29067 | 1.000 | 1.000 | 12000 | 16579 |
| 2 | 9336 | 666 | 29090 | 1.000 | 1.000 | 12000 | 16097 |
| 3 | 9326 | 672 | 29056 | 1.000 | 1.000 | 12000 | 16617 |
| 4 | 9331 | 671 | 29061 | 1.000 | 1.000 | 12000 | 16594 |
| 5 | 9325 | 673 | 29066 | 1.000 | 1.000 | 12000 | 16583 |
| 6 | 9326 | 674 | 29054 | 1.000 | 1.000 | 12000 | 16786 |
| 7 | 9333 | 664 | 29081 | 1.000 | 1.000 | 12000 | 16117 |
| 8 | 9451 | 474 | 20970 | 1.000 | 1.000 | 12000 | 5365 |
| 9 | 9960 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 10 | 9960 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 11 | 8916 | 0 | 0 | 0.895 | 0.984 | 10744 | 0 |
| 12 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### OV G-Tinf-H0

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10214 | 708 | 22641 | 0.661 | 1.000 | 7937 | 12458 |
| 1 | 9326 | 672 | 29067 | 1.000 | 1.000 | 12000 | 16579 |
| 2 | 9336 | 666 | 29090 | 1.000 | 1.000 | 12000 | 16097 |
| 3 | 9326 | 672 | 29056 | 1.000 | 1.000 | 12000 | 16617 |
| 4 | 9331 | 671 | 29061 | 1.000 | 1.000 | 12000 | 16594 |
| 5 | 9325 | 673 | 29066 | 1.000 | 1.000 | 12000 | 16583 |
| 6 | 9326 | 674 | 29054 | 1.000 | 1.000 | 12000 | 16786 |
| 7 | 9338 | 664 | 29076 | 1.000 | 1.000 | 12000 | 16262 |
| 8 | 9325 | 674 | 29040 | 1.000 | 1.000 | 12000 | 16922 |
| 9 | 9328 | 672 | 29045 | 1.000 | 1.000 | 12000 | 16797 |
| 10 | 9333 | 668 | 29065 | 1.000 | 1.000 | 12000 | 16497 |
| 11 | 9326 | 674 | 29047 | 1.000 | 1.000 | 12000 | 16878 |
| 12 | 7166 | 588 | 24195 | 0.768 | 0.868 | 9218 | 14433 |
| 13 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |
| 14 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### OV S4b

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10184 | 696 | 22641 | 0.660 | 1.000 | 7920 | 11982 |
| 1 | 9225 | 290 | 13433 | 1.000 | 1.000 | 12000 | 2218 |
| 2 | 9480 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 3 | 9478 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 4 | 9478 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 5 | 9478 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 6 | 9479 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 7 | 9480 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 8 | 9477 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 9 | 9478 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 10 | 2750 | 0 | 0 | 0.291 | 0.385 | 3486 | 0 |
| 11 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### OV G-T0p4-H0p02

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10217 | 697 | 22526 | 0.657 | 1.000 | 7885 | 11930 |
| 1 | 9577 | 346 | 15606 | 1.000 | 1.000 | 12000 | 2586 |
| 2 | 9962 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 3 | 9959 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 4 | 9960 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 5 | 9959 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 6 | 9960 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 7 | 9961 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 8 | 9959 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 9 | 9960 | 0 | 0 | 1.000 | 1.000 | 12000 | 0 |
| 10 | 2940 | 0 | 0 | 0.296 | 0.384 | 3548 | 0 |
| 11 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### CL C1

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10000 | 298 | 0 | 0.000 | 0.844 | 0 | 30 |
| 1 | 10000 | 294 | 0 | 0.000 | 0.845 | 0 | 0 |
| 2 | 1000 | 394 | 30934 | 0.900 | 0.115 | 9001 | 0 |
| 3 | 6523 | 548 | 33260 | 1.000 | 0.496 | 10000 | 3550 |
| 4 | 11794 | 709 | 27222 | 1.000 | 1.000 | 10000 | 8109 |
| 5 | 11786 | 710 | 27279 | 1.000 | 1.000 | 10000 | 7489 |
| 6 | 11790 | 712 | 27265 | 1.000 | 1.000 | 10000 | 7581 |
| 7 | 11794 | 708 | 27279 | 1.000 | 1.000 | 10000 | 7473 |
| 8 | 11782 | 716 | 27278 | 1.000 | 1.000 | 10000 | 7594 |
| 9 | 11787 | 713 | 27271 | 1.000 | 1.000 | 10000 | 7590 |
| 10 | 1744 | 163 | 5510 | 0.148 | 0.224 | 1477 | 1742 |
| 11 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### CL C3

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10000 | 322 | 0 | 0.000 | 0.826 | 0 | 1600 |
| 1 | 10000 | 327 | 0 | 0.000 | 0.826 | 0 | 1635 |
| 2 | 996 | 395 | 36530 | 0.901 | 0.111 | 9005 | 20 |
| 3 | 4909 | 1112 | 168482 | 1.000 | 0.439 | 10000 | 9985 |
| 4 | 11781 | 1074 | 45452 | 1.000 | 1.000 | 10000 | 28399 |
| 5 | 11751 | 748 | 26652 | 1.000 | 1.000 | 10000 | 16535 |
| 6 | 11756 | 743 | 26656 | 1.000 | 1.000 | 10000 | 16376 |
| 7 | 11756 | 747 | 26652 | 1.000 | 1.000 | 10000 | 16395 |
| 8 | 11750 | 748 | 26646 | 1.000 | 1.000 | 10000 | 16695 |
| 9 | 11745 | 752 | 26652 | 1.000 | 1.000 | 10000 | 16572 |
| 10 | 3557 | 284 | 9481 | 0.302 | 0.378 | 3020 | 6262 |
| 11 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### CL C3S

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10000 | 322 | 0 | 0.000 | 0.826 | 0 | 1600 |
| 1 | 10000 | 327 | 0 | 0.000 | 0.826 | 0 | 1635 |
| 2 | 996 | 395 | 36530 | 0.901 | 0.111 | 9005 | 20 |
| 3 | 4909 | 1112 | 168482 | 1.000 | 0.439 | 10000 | 9985 |
| 4 | 11781 | 1074 | 45452 | 1.000 | 1.000 | 10000 | 28399 |
| 5 | 11751 | 748 | 26652 | 1.000 | 1.000 | 10000 | 16535 |
| 6 | 11756 | 743 | 26656 | 1.000 | 1.000 | 10000 | 16376 |
| 7 | 11756 | 747 | 26652 | 1.000 | 1.000 | 10000 | 16395 |
| 8 | 11750 | 748 | 26646 | 1.000 | 1.000 | 10000 | 16695 |
| 9 | 11745 | 752 | 26652 | 1.000 | 1.000 | 10000 | 16572 |
| 10 | 3557 | 284 | 9481 | 0.302 | 0.378 | 3020 | 6262 |
| 11 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### CL G-T0p4-H0

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10000 | 317 | 0 | 0.000 | 0.825 | 0 | 1578 |
| 1 | 10000 | 327 | 0 | 0.000 | 0.826 | 0 | 1635 |
| 2 | 996 | 193 | 14564 | 0.901 | 0.097 | 9005 | 20 |
| 3 | 4767 | 148 | 0 | 0.523 | 0.395 | 5234 | 738 |
| 4 | 10000 | 329 | 0 | 0.000 | 0.826 | 0 | 1646 |
| 5 | 10000 | 334 | 0 | 0.000 | 0.827 | 0 | 1674 |
| 6 | 10000 | 332 | 0 | 0.000 | 0.827 | 0 | 1658 |
| 7 | 10000 | 337 | 0 | 0.000 | 0.827 | 0 | 1686 |
| 8 | 10000 | 322 | 0 | 0.000 | 0.826 | 0 | 1612 |
| 9 | 10000 | 342 | 0 | 0.000 | 0.827 | 0 | 1709 |
| 10 | 0 | 4 | 0 | 0.000 | 0.000 | 0 | 22 |
| 11 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### CL G-T0p4-Hinf

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10000 | 317 | 0 | 0.000 | 0.825 | 0 | 1578 |
| 1 | 10000 | 327 | 0 | 0.000 | 0.826 | 0 | 1635 |
| 2 | 996 | 193 | 14564 | 0.901 | 0.097 | 9005 | 20 |
| 3 | 4767 | 148 | 0 | 0.523 | 0.395 | 5234 | 738 |
| 4 | 10000 | 329 | 0 | 0.000 | 0.826 | 0 | 1646 |
| 5 | 10000 | 334 | 0 | 0.000 | 0.827 | 0 | 1674 |
| 6 | 10000 | 332 | 0 | 0.000 | 0.827 | 0 | 1658 |
| 7 | 10000 | 337 | 0 | 0.000 | 0.827 | 0 | 1686 |
| 8 | 10000 | 322 | 0 | 0.000 | 0.826 | 0 | 1612 |
| 9 | 10000 | 342 | 0 | 0.000 | 0.827 | 0 | 1709 |
| 10 | 0 | 4 | 0 | 0.000 | 0.000 | 0 | 22 |
| 11 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### CL G-T2-H0

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10000 | 322 | 0 | 0.000 | 0.826 | 0 | 1600 |
| 1 | 10000 | 327 | 0 | 0.000 | 0.826 | 0 | 1635 |
| 2 | 996 | 395 | 36530 | 0.901 | 0.111 | 9005 | 20 |
| 3 | 4909 | 1112 | 168482 | 1.000 | 0.439 | 10000 | 9985 |
| 4 | 11836 | 1008 | 38862 | 1.000 | 1.000 | 10000 | 21252 |
| 5 | 11753 | 745 | 26653 | 1.000 | 1.000 | 10000 | 16540 |
| 6 | 11753 | 748 | 26654 | 1.000 | 1.000 | 10000 | 16364 |
| 7 | 11756 | 745 | 26655 | 1.000 | 1.000 | 10000 | 16403 |
| 8 | 11747 | 751 | 26647 | 1.000 | 1.000 | 10000 | 16686 |
| 9 | 11745 | 753 | 26660 | 1.000 | 1.000 | 10000 | 16584 |
| 10 | 3504 | 282 | 9357 | 0.297 | 0.373 | 2974 | 6173 |
| 11 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### CL G-T2-Hinf

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10000 | 322 | 0 | 0.000 | 0.826 | 0 | 1600 |
| 1 | 10000 | 327 | 0 | 0.000 | 0.826 | 0 | 1635 |
| 2 | 996 | 395 | 36530 | 0.901 | 0.111 | 9005 | 20 |
| 3 | 4909 | 1112 | 168482 | 1.000 | 0.439 | 10000 | 9985 |
| 4 | 11836 | 1008 | 38862 | 1.000 | 1.000 | 10000 | 21252 |
| 5 | 11753 | 745 | 26653 | 1.000 | 1.000 | 10000 | 16540 |
| 6 | 11753 | 748 | 26654 | 1.000 | 1.000 | 10000 | 16364 |
| 7 | 11756 | 745 | 26655 | 1.000 | 1.000 | 10000 | 16403 |
| 8 | 11747 | 751 | 26647 | 1.000 | 1.000 | 10000 | 16686 |
| 9 | 11745 | 753 | 26660 | 1.000 | 1.000 | 10000 | 16584 |
| 10 | 3504 | 282 | 9357 | 0.297 | 0.373 | 2974 | 6173 |
| 11 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### CL G-Tinf-H0

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10000 | 322 | 0 | 0.000 | 0.826 | 0 | 1600 |
| 1 | 10000 | 327 | 0 | 0.000 | 0.826 | 0 | 1635 |
| 2 | 996 | 395 | 36530 | 0.901 | 0.111 | 9005 | 20 |
| 3 | 4909 | 1112 | 168482 | 1.000 | 0.439 | 10000 | 9985 |
| 4 | 11781 | 1074 | 45452 | 1.000 | 1.000 | 10000 | 28399 |
| 5 | 11751 | 748 | 26652 | 1.000 | 1.000 | 10000 | 16535 |
| 6 | 11756 | 743 | 26656 | 1.000 | 1.000 | 10000 | 16376 |
| 7 | 11756 | 747 | 26652 | 1.000 | 1.000 | 10000 | 16395 |
| 8 | 11750 | 748 | 26646 | 1.000 | 1.000 | 10000 | 16695 |
| 9 | 11745 | 752 | 26652 | 1.000 | 1.000 | 10000 | 16572 |
| 10 | 3557 | 284 | 9481 | 0.302 | 0.378 | 3020 | 6262 |
| 11 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### CL S4b

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10000 | 317 | 0 | 0.000 | 0.826 | 0 | 1578 |
| 1 | 10000 | 327 | 0 | 0.000 | 0.826 | 0 | 1635 |
| 2 | 996 | 193 | 14564 | 0.901 | 0.097 | 9005 | 20 |
| 3 | 4767 | 148 | 0 | 0.523 | 0.395 | 5234 | 738 |
| 4 | 10000 | 329 | 0 | 0.000 | 0.826 | 0 | 1646 |
| 5 | 10000 | 334 | 0 | 0.000 | 0.827 | 0 | 1674 |
| 6 | 10000 | 332 | 0 | 0.000 | 0.827 | 0 | 1658 |
| 7 | 10000 | 337 | 0 | 0.000 | 0.827 | 0 | 1686 |
| 8 | 10000 | 322 | 0 | 0.000 | 0.826 | 0 | 1612 |
| 9 | 10000 | 342 | 0 | 0.000 | 0.827 | 0 | 1709 |
| 10 | 0 | 4 | 0 | 0.000 | 0.000 | 0 | 22 |
| 11 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |

#### CL G-T0p4-H0p02

| window | first sends | copies sent | refused by budget | pause (s) | link busy | blocked by pause | refused by cooldown |
|---|---|---|---|---|---|---|---|
| 0 | 10000 | 317 | 0 | 0.000 | 0.825 | 0 | 1578 |
| 1 | 10000 | 327 | 0 | 0.000 | 0.826 | 0 | 1635 |
| 2 | 996 | 193 | 14564 | 0.901 | 0.097 | 9005 | 20 |
| 3 | 4767 | 148 | 0 | 0.523 | 0.395 | 5234 | 738 |
| 4 | 10000 | 329 | 0 | 0.000 | 0.826 | 0 | 1646 |
| 5 | 10000 | 334 | 0 | 0.000 | 0.827 | 0 | 1674 |
| 6 | 10000 | 332 | 0 | 0.000 | 0.827 | 0 | 1658 |
| 7 | 10000 | 337 | 0 | 0.000 | 0.827 | 0 | 1686 |
| 8 | 10000 | 322 | 0 | 0.000 | 0.826 | 0 | 1612 |
| 9 | 10000 | 342 | 0 | 0.000 | 0.827 | 0 | 1709 |
| 10 | 0 | 4 | 0 | 0.000 | 0.000 | 0 | 22 |
| 11 | 0 | 0 | 0 | 0.000 | 0.000 | 0 | 0 |
### [dups]
### copies sent against duplicate arrivals (seed means): gauges repair_sends and dup; registered arms and the unregistered cooldown_all supplement
| cell | arm | copies sent | duplicate arrivals | duplicates per copy | refused by the budget |
|---|---|---|---|---|---|
| OV | C1 | 7454 | 4252 | 0.571 | 379280 |
| OV | C3 | 8676 | 4228 | 0.487 | 366504 |
| OV | C3S | 8676 | 4228 | 0.487 | 366504 |
| OV | G-T0p4-H0 | 1043 | 539 | 0.517 | 38132 |
| OV | G-T0p4-Hinf | 1043 | 539 | 0.517 | 38132 |
| OV | G-T2-H0 | 5875 | 2893 | 0.492 | 247086 |
| OV | G-T2-Hinf | 5875 | 2893 | 0.492 | 247086 |
| OV | G-Tinf-H0 | 8676 | 4228 | 0.487 | 366504 |
| OV | S4b | 986 | 513 | 0.520 | 36074 |
| OV | G-T0p4-H0p02 | 1043 | 539 | 0.517 | 38132 |
| OV | SUP-C3 | 7179 | 2879 | 0.401 | 2031 |
| OV | SUP-C1 | 7454 | 4247 | 0.570 | 28279 |
| OV | SUP-G-T0p4-Hinf | 940 | 433 | 0.461 | 717 |
| OV | SUP-G-T2-Hinf | 5120 | 2071 | 0.405 | 1584 |
| CL | C1 | 5964 | 2147 | 0.360 | 233298 |
| CL | C3 | 7252 | 2044 | 0.282 | 393203 |
| CL | C3S | 7252 | 2044 | 0.282 | 393203 |
| CL | G-T0p4-H0 | 2986 | 0 | 0.000 | 14564 |
| CL | G-T0p4-Hinf | 2986 | 0 | 0.000 | 14564 |
| CL | G-T2-H0 | 7187 | 2086 | 0.290 | 386500 |
| CL | G-T2-Hinf | 7187 | 2086 | 0.290 | 386500 |
| CL | G-Tinf-H0 | 7252 | 2044 | 0.282 | 393203 |
| CL | S4b | 2986 | 0 | 0.000 | 14564 |
| CL | G-T0p4-H0p02 | 2986 | 0 | 0.000 | 14564 |
| CL | SUP-C3 | 6560 | 1426 | 0.217 | 227025 |
| CL | SUP-C1 | 5166 | 1430 | 0.277 | 54697 |
| CL | SUP-G-T0p4-Hinf | 2986 | 0 | 0.000 | 14564 |
| CL | SUP-G-T2-Hinf | 6476 | 1440 | 0.222 | 219907 |
