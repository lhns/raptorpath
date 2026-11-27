# Build B″ (three-knobs-revision4.md 14): results, read with the registered rules (2026-11-27)

**What ran.**
- **The registration** was committed before any B″ run (`f5ec0177`).
- **Runs:** 838 B″ runs plus the registered doublings (CL: 396 runs; CL-long: 420), with the gauge-only pass before them (54).
- **The store:** a copy of the `toy-stage3e` store holding every stored A, B′ and baseline row. No stored row was rerun except where the registered doubling asked for seeds beyond the stored ones.
- **Where:** IDENT and K-OV ran on the Windows host at 2 workers, which is memory-bound. Every other part ran in a Linux container (`python:3.11-slim`, numpy 2.4.3) at 4–5 workers per part.
  - Before any run, one stored B′ row and one stored Realtime row reproduced key for key, in the container and on the host.
- **Driver:** `confirm_rev5.py --only PART`. The per-part sections below are its output, unedited, in run order.

## Verdict

**Against both arms of today's Realtime, B″ is at least as good in every cell**: the eight cells of 12 and both mid-price cells. No registered word against today's Realtime moved toward worse, and the two mid-price cells that read "worse" under B′ now read "dominates".

| cell | B′ against the stored (no-queue) arm | B″ against the stored arm | B″ against the engine-queue arm | how B″'s word is established |
|---|---|---|---|---|
| CL-long | better (on time) | **better** (on time +0.0161, recovery equivalent) | **better** | rerun, 12 seeds plus the doubling |
| CL | better | **better** (on time +0.0322) | **better** | rerun, 12 seeds plus the doubling |
| CLw-long | better | **better** (on time), every word unchanged | **better** | rerun, 12 seeds |
| CLc | equivalent | **equivalent** | **equivalent** | identical to A (IDENT, traces) |
| CLc-v | equivalent / better | **the same** | **the same** | identical to A |
| CL-long-v | on time better; recovery "shown worse" as registered (equivalent on the supplementary set) | **the same** (identical to A) | **the same** | identical to A, seed 11 included |
| OV-long | dominates | **dominates** (share −0.0019, p50 0.078 s better) | **dominates** | rerun, 6 seeds |
| OV-long-v | equivalent | **equivalent** | **equivalent** | identical to B′ (48 of 48 runs) |
| OV-long-m (δ = 0.1) | **worse** (share −0.0090, p50 +0.026 s) | **dominates** (share −0.0063, p50 0.0127 s better) | **dominates** | rerun, 6 seeds |
| OV-long-q (δ = 0.118) | **worse** (share −0.0092, p50 +0.012 s) | **dominates** (share −0.0065, p50 0.0224 s better) | **dominates** | rerun, 6 seeds |

- **Count, on Part 0's two conventions** (`results-recount.md`): 8 of 8 counting CL-long-v on its supplementary set, and 7 of 8 counting its registered recovery word, which is unchanged because B″ ≡ A there. The two mid-price cells now pass as well.
- **What is not "at least as good"** is the comparison with ADR-0084's and Bulk's never-give-up delivery in the overload cells. That is a trade by design: share −0.19, on time +0.80. It is unchanged.
- **Against ADR-0084 in CL and CL-long,** five words moved from "equivalent" to "undetermined", toward better, after the doubling:
  - whole-run on time at ρ ∈ {0.99, 0.995} in CL and CL-long;
  - recovery at ρ = 1 in CL-long.
  - Every interval lies above −bar (for example CL-long on time at ρ = 0.99: +0.0134 [+0.0108, +0.0163] against a bar of 0.0161).
  - By §14.5's rule these read "not worse (undetermined)". They are recorded as changed words.
- **CL at ρ = 1** was undetermined under A, and its doubling was owed. That doubling has now run (B″ arms and ADR-0084 at 24 seeds), and it stays undetermined toward better: +0.0266 [+0.0208, +0.0342] against a bar of 0.0321.

## Predictions against results

| item | registered (14.5) | measured | in band |
|---|---|---|---|
| IDENT, B″ ≡ A (CLc, CLc-v, CL-long-v, seed 11) | identical | 28 of 28 identical | yes |
| OV-long-v, B″ ≡ B′ | identical | 48 of 48 identical | yes |
| outage cells, B″ ≡ A where the gauge read 0 | identical | identical at T ≥ 1.5 in CL and CL-long, and at T = ∞ in all three. This includes the Bulk identity | yes |
| OV-long-m, T = 0.4, against the stored arm | share −0.0061 (−0.0080 to −0.0045); p50 +0.012 s (−0.003 to +0.027); dominates or equivalent | −0.0063; +0.0127 s; **dominates** | yes |
| OV-long-m, against the engine-queue arm | dominates | dominates (share −0.0072, p50 0.576 s better) | yes |
| OV-long-q, T = 0.4, against the stored arm | share −0.0062 (−0.0081 to −0.0046); p50 +0.022 s (+0.007 to +0.037); dominates | −0.0065; +0.0224 s; **dominates** | yes |
| OV-long-q, against the engine-queue arm | dominates | dominates (share −0.0070, p50 0.584 s better) | yes |
| B″ level, OV-long-m, T = 0.4 / 0.6 / 1 / 2 (share; p50) | 0.8021 / 0.7989 / 0.7936 / 0.7896; 0.161 / 0.255 / 0.520 / 1.595 s | 0.8018 / 0.7988 / 0.7936 / 0.7883; 0.160 / 0.255 / 0.522 / 1.611 s | yes, all four |
| B″ level, OV-long-q, the same | 0.8019 / 0.7987 / 0.7930 / 0.7881; 0.150 / 0.231 / 0.461 / 1.370 s | 0.8017 / 0.7986 / 0.7931 / 0.7841; 0.150 / 0.230 / 0.461 / 1.383 s | yes, except the T = 2 share, which sits on the −0.004 edge |
| R5's tail, w·t_ov = (c/x̄)·s_b | within 3 % per window | median 0.03–0.21 %, max 0.93 % (OV-long-m and OV-long-q, every T) | yes |
| OV-long, T = 0.4, against the stored arm | share −0.0016 (−0.004 to 0); p50 +0.078 s; dominates | −0.0019; +0.0784 s; dominates | yes |
| outage cells: whole-run gain at the bound rows | CL-long +0.0011, CL +0.0022, CLw-long +0.0015 / +0.0013 (T = 0.4 / 2), each ± 0.0010 | +0.0016, +0.0032, +0.0004 / +0.0002 | CL-long and CL yes; **CLw-long no** (low by 0.0011, sign right) |
| outage cells: every word against every baseline unchanged | unchanged | Realtime words: all unchanged. ADR-0084: 5 words to "undetermined", toward better | Realtime yes; ADR-0084 **no** (a move toward better) |
| K-OV (reported only) | 0.71–0.73 at p50 0.15–0.19 s | 0.7229 at p50 0.164 s | yes |

**Misses, stated.**
- **CLw-long's gain is a quarter of the predicted size.** The removed drain repairs there decoded more than the 0.2 per repair assumed. The sign is right, and it decides no word.
- **The ADR-0084 words in the CL cells moved, toward better.** §14.5 predicted no change, and the moves are larger than "less than the bars" implied. The registered rule reads them "not worse", and none lies below −bar.
- **OV-long-q at T = 2:** the share sits on the −0.004 band edge. That is the row whose calibration was already 0.002 off.

**Costs that B″ carries, reported and not hidden.**
- **At T ≥ 1 in the mid-price cells,** R5 sheds more of the band inside the window than R1. Against B′, B″'s steady share is equal at T = 1 and 0.006 lower at T = 2, while its p50 is 0.10–0.12 s lower.
  - The whole-run bill at T = 2 is 1 886 units more than B′'s in OV-long-q.
  - Neither row is the best point.
- **In CL** (T = 0.6–1.25), whole-run delivered rises by +0.003, but on time within 0.4 s falls by about 0.003, and recovery is up to 0.07 s later.
  - These rows lie on the curve, and they do not change any Realtime word.


## GAUGE (14.5, reads only): build A with the net clamp's gauge, against the stored A rows

| cell | T | seed | equal to stored A | first sends gauged | net_bind | repair credits removed | stored repairs |
|---|---|---|---|---|---|---|---|
| CL-long | 0p4 | 0 | yes | 188972 | 6490 | 280.7 | 19159 |
| CL-long | 0p4 | 1 | yes | 189153 | 6470 | 285.5 | 19174 |
| CL-long | 0p4 | 2 | yes | 188933 | 6408 | 285.5 | 19168 |
| CL-long | 2 | 0 | yes | 200000 | 0 | 0.0 | 12957 |
| CL-long | 2 | 1 | yes | 200000 | 0 | 0.0 | 13031 |
| CL-long | 2 | 2 | yes | 200000 | 0 | 0.0 | 12747 |
| CL-long | inf | 0 | yes | 200000 | 0 | 0.0 | 12839 |
| CL-long | inf | 1 | yes | 200000 | 0 | 0.0 | 12913 |
| CL-long | inf | 2 | yes | 200000 | 0 | 0.0 | 12617 |
| CL | 0p4 | 0 | yes | 88972 | 6490 | 280.7 | 8179 |
| CL | 0p4 | 1 | yes | 89153 | 6470 | 285.5 | 8191 |
| CL | 0p4 | 2 | yes | 88933 | 6408 | 285.5 | 8180 |
| CL | 2 | 0 | yes | 100000 | 0 | 0.0 | 3279 |
| CL | 2 | 1 | yes | 100000 | 0 | 0.0 | 3302 |
| CL | 2 | 2 | yes | 100000 | 0 | 0.0 | 3266 |
| CL | inf | 0 | yes | 100000 | 0 | 0.0 | 3267 |
| CL | inf | 1 | yes | 100000 | 0 | 0.0 | 3272 |
| CL | inf | 2 | yes | 100000 | 0 | 0.0 | 3274 |
| CLw-long | 0p4 | 0 | yes | 139769 | 5503 | 314.0 | 15888 |
| CLw-long | 0p4 | 1 | yes | 139921 | 5489 | 319.4 | 15917 |
| CLw-long | 0p4 | 2 | yes | 139925 | 5581 | 313.6 | 15913 |
| CLw-long | 2 | 0 | yes | 152647 | 6417 | 264.3 | 13109 |
| CLw-long | 2 | 1 | yes | 152743 | 6410 | 267.4 | 13100 |
| CLw-long | 2 | 2 | yes | 152744 | 6348 | 265.4 | 13099 |
| CLw-long | inf | 0 | yes | 160000 | 0 | 0.0 | 11308 |
| CLw-long | inf | 1 | yes | 160000 | 0 | 0.0 | 11342 |
| CLw-long | inf | 2 | yes | 160000 | 0 | 0.0 | 11283 |
| CLc | 0p4 | 0 | yes | 100000 | 0 | 0.0 | 10979 |
| CLc | 0p4 | 1 | yes | 100000 | 0 | 0.0 | 10984 |
| CLc | 0p4 | 2 | yes | 100000 | 0 | 0.0 | 10982 |
| CLc | 1 | 0 | yes | 100000 | 0 | 0.0 | 10979 |
| CLc | 1 | 1 | yes | 100000 | 0 | 0.0 | 10984 |
| CLc | 1 | 2 | yes | 100000 | 0 | 0.0 | 10982 |
| CLc | inf | 0 | yes | 100000 | 0 | 0.0 | 10979 |
| CLc | inf | 1 | yes | 100000 | 0 | 0.0 | 10984 |
| CLc | inf | 2 | yes | 100000 | 0 | 0.0 | 10982 |
| CLc-v | 1 | 0 | yes | 100000 | 0 | 0.0 | 0 |
| CLc-v | 1 | 1 | yes | 100000 | 0 | 0.0 | 0 |
| CLc-v | 1 | 2 | yes | 100000 | 0 | 0.0 | 0 |
| CLc-v | 2 | 0 | yes | 100000 | 0 | 0.0 | 0 |
| CLc-v | 2 | 1 | yes | 100000 | 0 | 0.0 | 0 |
| CLc-v | 2 | 2 | yes | 100000 | 0 | 0.0 | 0 |
| CLc-v | inf | 0 | yes | 100000 | 0 | 0.0 | 0 |
| CLc-v | inf | 1 | yes | 100000 | 0 | 0.0 | 0 |
| CLc-v | inf | 2 | yes | 100000 | 0 | 0.0 | 0 |
| CL-long-v | 1 | 0 | yes | 195045 | 0 | 0.0 | 0 |
| CL-long-v | 1 | 1 | yes | 195294 | 0 | 0.0 | 0 |
| CL-long-v | 1 | 2 | yes | 195294 | 0 | 0.0 | 0 |
| CL-long-v | 2 | 0 | yes | 200000 | 0 | 0.0 | 0 |
| CL-long-v | 2 | 1 | yes | 200000 | 0 | 0.0 | 0 |
| CL-long-v | 2 | 2 | yes | 200000 | 0 | 0.0 | 0 |
| CL-long-v | inf | 0 | yes | 200000 | 0 | 0.0 | 0 |
| CL-long-v | inf | 1 | yes | 200000 | 0 | 0.0 | 0 |
| CL-long-v | inf | 2 | yes | 200000 | 0 | 0.0 | 0 |

(GAUGE: 54 new runs, 409 s wall at 4 workers)



## IDENT (14.5): B'' against A, trace for trace

| cell | T | seed | identical | net_bind | n_cap_lt_c | y_max |
|---|---|---|---|---|---|---|
| CLc | 0p4 | 0 | yes | 0 | 0 | 0.0000 |
| CLc | 0p4 | 1 | yes | 0 | 0 | 0.0000 |
| CLc | 0p4 | 2 | yes | 0 | 0 | 0.0000 |
| CLc | 1 | 0 | yes | 0 | 0 | 0.0000 |
| CLc | 1 | 1 | yes | 0 | 0 | 0.0000 |
| CLc | 1 | 2 | yes | 0 | 0 | 0.0000 |
| CLc | inf | 0 | yes | 0 | 0 | 0.0000 |
| CLc | inf | 1 | yes | 0 | 0 | 0.0000 |
| CLc | inf | 2 | yes | 0 | 0 | 0.0000 |
| CLc-v | 1 | 0 | yes | 0 | 0 | 0.0000 |
| CLc-v | 1 | 1 | yes | 0 | 0 | 0.0000 |
| CLc-v | 1 | 2 | yes | 0 | 0 | 0.0000 |
| CLc-v | 2 | 0 | yes | 0 | 0 | 0.0000 |
| CLc-v | 2 | 1 | yes | 0 | 0 | 0.0000 |
| CLc-v | 2 | 2 | yes | 0 | 0 | 0.0000 |
| CLc-v | inf | 0 | yes | 0 | 0 | 0.0000 |
| CLc-v | inf | 1 | yes | 0 | 0 | 0.0000 |
| CLc-v | inf | 2 | yes | 0 | 0 | 0.0000 |
| CL-long-v | 1 | 0 | yes | 0 | 0 | 0.0000 |
| CL-long-v | 1 | 1 | yes | 0 | 0 | 0.0000 |
| CL-long-v | 1 | 2 | yes | 0 | 0 | 0.0000 |
| CL-long-v | 2 | 0 | yes | 0 | 0 | 0.0000 |
| CL-long-v | 2 | 1 | yes | 0 | 0 | 0.0000 |
| CL-long-v | 2 | 2 | yes | 0 | 0 | 0.0000 |
| CL-long-v | inf | 0 | yes | 0 | 0 | 0.0000 |
| CL-long-v | inf | 1 | yes | 0 | 0 | 0.0000 |
| CL-long-v | inf | 2 | yes | 0 | 0 | 0.0000 |
| CL-long-v | 2 | 11 | yes | 0 | 135 | 0.0000 |

(IDENT: 28 new runs, 422 s wall at 2 workers)



## OV-long-v (build B'', 14)

B'' = A + R5 (wait_limit_form rev5) + the net repair clamp (clamp_net). A and B' are the stored rows. Dominance on the paired steady window (12.2), best finite-T point per build, on the fixed reader (13.6). Delay p50 is reported only here (delta = 0.005, 12.3).

| row | seeds | whole-run delivered | on time (whole run) | repairs sent | decoded | resends | net_bind (mean) | repair credits removed (mean) | onset_t | y_max | w_min_over_c |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=1, hold 0 (S5) | 6 | 0.8277 | 0.8277 | 0 | 0 | 1528 | - | - | - | - | - |
| A; lifetime T=1p5, hold 0 (S5) | 6 | 0.8377 | 0.8377 | 0 | 0 | 2353 | - | - | - | - | - |
| A; lifetime T=2, hold 0 (S5) | 6 | 0.8476 | 0.8476 | 0 | 0 | 3192 | - | - | - | - | - |
| A; lifetime T=3, hold 0 (S5) | 6 | 0.8676 | 0.2072 | 0 | 0 | 4893 | - | - | - | - | - |
| A; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.2072 | 0 | 0 | 16152 | - | - | - | - | - |
| Bp; lifetime T=1, hold 0 (S5) | 6 | 0.8261 | 0.8261 | 0 | 0 | 12074 | - | - | - | 0.20 | 0.8084 |
| Bp; lifetime T=1p5, hold 0 (S5) | 6 | 0.8360 | 0.8360 | 0 | 0 | 13236 | - | - | - | 0.13 | 0.8076 |
| Bp; lifetime T=2, hold 0 (S5) | 6 | 0.8462 | 0.8462 | 0 | 0 | 13484 | - | - | - | 0.10 | 0.8072 |
| Bp; lifetime T=3, hold 0 (S5) | 6 | 0.8663 | 0.2072 | 0 | 0 | 13795 | - | - | - | 0.06 | 0.8077 |
| Bp; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.2072 | 0 | 0 | 16152 | - | - | - | 0.00 | 0.8077 |
| Bpp; lifetime T=1, hold 0 (S5) | 6 | 0.8261 | 0.8261 | 0 | 0 | 12074 | 0 | 0.0 | - | 0.20 | 0.8084 |
| Bpp; lifetime T=1p5, hold 0 (S5) | 6 | 0.8360 | 0.8360 | 0 | 0 | 13236 | 0 | 0.0 | - | 0.13 | 0.8076 |
| Bpp; lifetime T=2, hold 0 (S5) | 6 | 0.8462 | 0.8462 | 0 | 0 | 13484 | 0 | 0.0 | - | 0.10 | 0.8072 |
| Bpp; lifetime T=3, hold 0 (S5) | 6 | 0.8663 | 0.2072 | 0 | 0 | 13795 | 0 | 0.0 | - | 0.06 | 0.8077 |
| Bpp; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.2072 | 0 | 0 | 16152 | 0 | 0.0 | - | 0.00 | 0.8077 |
| Bpp; lifetime T=1, hold inf (S5) | 6 | 0.8261 | 0.8261 | 0 | 0 | 12074 | 0 | 0.0 | - | 0.20 | 0.8084 |
| Bpp; lifetime T=2, hold inf (S5) | 6 | 0.8462 | 0.8462 | 0 | 0 | 13484 | 0 | 0.0 | - | 0.10 | 0.8072 |
| Bpp; lifetime T=inf, hold inf (S5) | 6 | 1.0000 | 0.1861 | 0 | 0 | 16152 | 0 | 0.0 | - | 0.00 | 0.8077 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 3 | 0.9971 | 0.1848 | 0 | 0 | 14480 | - | - | - | - | - |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 3 | 0.9971 | 0.2062 | 0 | 0 | 14472 | - | - | - | - | - |
| rt; today's Realtime, delta=0p005 (E-rt) | 3 | 0.8108 | 0.8108 | 0 | 0 | 46 | - | - | - | - | - |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | 3 | 0.8249 | 0.8249 | 0 | 0 | 15 | - | - | - | - | - |
| rt; today's Realtime, delta=50 (E-rt) | 3 | 0.8104 | 0.8104 | 292 | 91 | 44 | - | - | - | - | - |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 3 | 0.8243 | 0.8243 | 292 | 91 | 11 | - | - | - | - | - |
| bulk; never gives up, strict order = today's Bulk (C3) | 3 | 1.0000 | 0.1876 | 0 | 0 | 16122 | - | - | - | - | - |

Dominance (12.3):

| build | baseline | best point | share | on time | p50 | outcome |
|---|---|---|---|---|---|---|
| A | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=1 | -0.189 [-0.189, -0.189] | 0.779 [0.779, 0.779] | 4.88 [4.84, 4.9] | not dominated (a trade) (p50 reported only) |
| A | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=1 | -0.189 [-0.189, -0.189] | 0.751 [0.751, 0.751] | 4.27 [4.24, 4.29] | not dominated (a trade) (p50 reported only) |
| A | rt; today's Realtime, delta=0p005 (E-rt) | T=1 | -5.38e-05 [-0.000191, 0.00021] | -5.38e-05 [-0.000191, 0.00021] | -0.821 [-0.821, -0.821] | equivalent (p50 reported only) |
| A | rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | T=1 | -5.47e-05 [-0.000215, 0.000116] | -5.47e-05 [-0.000215, 0.000116] | -0.259 [-0.264, -0.256] | equivalent (p50 reported only) |
| A | rt; today's Realtime, delta=50 (E-rt) | T=1 | -5.38e-05 [-0.000191, 0.00021] | -5.38e-05 [-0.000191, 0.00021] | -0.821 [-0.821, -0.821] | equivalent (p50 reported only) |
| A | rteq; today's Realtime, delta=50 (E-rt) (engine queue) | T=1 | 1.16e-05 [-5.91e-05, 9.41e-05] | 1.16e-05 [-5.91e-05, 9.41e-05] | -0.264 [-0.264, -0.264] | equivalent (p50 reported only) |
| A | bulk; never gives up, strict order = today's Bulk (C3) | T=1 | -0.192 [-0.192, -0.192] | 0.776 [0.775, 0.776] | 4.49 [4.45, 4.5] | not dominated (a trade) (p50 reported only) |
| B' | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=1 | -0.191 [-0.191, -0.19] | 0.747 [0.747, 0.747] | 4.92 [4.9, 4.96] | not dominated (a trade) (p50 reported only) |
| B' | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=1 | -0.191 [-0.191, -0.19] | 0.72 [0.72, 0.72] | 4.32 [4.31, 4.36] | not dominated (a trade) (p50 reported only) |
| B' | rt; today's Realtime, delta=0p005 (E-rt) | T=1 | -0.00167 [-0.00173, -0.00155] | -0.00167 [-0.00173, -0.00155] | -0.649 [-0.649, -0.649] | equivalent (p50 reported only) |
| B' | rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | T=1 | -0.00167 [-0.00179, -0.00158] | -0.00167 [-0.00179, -0.00158] | -0.0876 [-0.0925, -0.0852] | equivalent (p50 reported only) |
| B' | rt; today's Realtime, delta=50 (E-rt) | T=1 | -0.00167 [-0.00173, -0.00155] | -0.00167 [-0.00173, -0.00155] | -0.649 [-0.649, -0.649] | equivalent (p50 reported only) |
| B' | rteq; today's Realtime, delta=50 (E-rt) (engine queue) | T=1 | -0.00159 [-0.00163, -0.00151] | -0.00159 [-0.00163, -0.00151] | -0.0925 [-0.0925, -0.0925] | equivalent (p50 reported only) |
| B' | bulk; never gives up, strict order = today's Bulk (C3) | T=1 | -0.194 [-0.194, -0.193] | 0.744 [0.743, 0.744] | 4.57 [4.57, 4.57] | not dominated (a trade) (p50 reported only) |
| B'' | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=1 | -0.191 [-0.191, -0.19] | 0.747 [0.747, 0.747] | 4.92 [4.9, 4.96] | not dominated (a trade) (p50 reported only) |
| B'' | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=1 | -0.191 [-0.191, -0.19] | 0.72 [0.72, 0.72] | 4.32 [4.31, 4.36] | not dominated (a trade) (p50 reported only) |
| B'' | rt; today's Realtime, delta=0p005 (E-rt) | T=1 | -0.00167 [-0.00173, -0.00155] | -0.00167 [-0.00173, -0.00155] | -0.649 [-0.649, -0.649] | equivalent (p50 reported only) |
| B'' | rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | T=1 | -0.00167 [-0.00179, -0.00158] | -0.00167 [-0.00179, -0.00158] | -0.0876 [-0.0925, -0.0852] | equivalent (p50 reported only) |
| B'' | rt; today's Realtime, delta=50 (E-rt) | T=1 | -0.00167 [-0.00173, -0.00155] | -0.00167 [-0.00173, -0.00155] | -0.649 [-0.649, -0.649] | equivalent (p50 reported only) |
| B'' | rteq; today's Realtime, delta=50 (E-rt) (engine queue) | T=1 | -0.00159 [-0.00163, -0.00151] | -0.00159 [-0.00163, -0.00151] | -0.0925 [-0.0925, -0.0925] | equivalent (p50 reported only) |
| B'' | bulk; never gives up, strict order = today's Bulk (C3) | T=1 | -0.194 [-0.194, -0.193] | 0.744 [0.743, 0.744] | 4.57 [4.57, 4.57] | not dominated (a trade) (p50 reported only) |

B'' against B' and against A at each lifetime, paired steady window (reported only; 'positive favours B''' reads 'positive favours the first'):

| T | readout | B'' - B' [95 %] | bar |
|---|---|---|---|
| 1 | steady share | 0 [0, 0] (positive favours B'') | 0.00968 |
| 1 | steady ot | 0 [0, 0] (positive favours B'') | 0.00968 |
| 1 | steady p50 | 0 [0, 0] (positive favours B'') | 0.0411 |
| 1p5 | steady share | 0 [0, 0] (positive favours B'') | 0.0097 |
| 1p5 | steady ot | 0 [0, 0] (positive favours B'') | 0.0097 |
| 1p5 | steady p50 | 0 [0, 0] (positive favours B'') | 0.0618 |
| 2 | steady share | 0 [0, 0] (positive favours B'') | 0.0097 |
| 2 | steady ot | 0 [0, 0] (positive favours B'') | 0.0097 |
| 2 | steady p50 | 0 [0, 0] (positive favours B'') | 0.0816 |
| 3 | steady share | 0 [0, 0] (positive favours B'') | 0.00969 |
| 3 | steady ot | 0 [0, 0] (positive favours B'') | 0.05 |
| 3 | steady p50 | 0 [0, 0] (positive favours B'') | 0.123 |
| T | readout | B'' - A [95 %] | bar |
|---|---|---|---|
| 1 | steady share | -0.00173 [-0.00178, -0.00165] (positive favours B'') | 0.0096 |
| 1 | steady ot | -0.00173 [-0.00178, -0.00165] (positive favours B'') | 0.0096 |
| 1 | steady p50 | 0.171 [0.171, 0.171] (positive favours B'') | 0.0496 |
| 1p5 | steady share | -0.00194 [-0.00204, -0.00184] (positive favours B'') | 0.0096 |
| 1p5 | steady ot | -0.00194 [-0.00204, -0.00184] (positive favours B'') | 0.0096 |
| 1p5 | steady p50 | 0.257 [0.257, 0.257] (positive favours B'') | 0.0746 |
| 2 | steady share | -0.00188 [-0.00213, -0.00169] (positive favours B'') | 0.0096 |
| 2 | steady ot | -0.00188 [-0.00213, -0.00169] (positive favours B'') | 0.0096 |
| 2 | steady p50 | 0.36 [0.36, 0.36] (positive favours B'') | 0.0996 |
| 3 | steady share | -0.00179 [-0.00212, -0.00142] (positive favours B'') | 0.0096 |
| 3 | steady ot | 0 [0, 0] (positive favours B'') | 0.05 |
| 3 | steady p50 | 0.541 [0.541, 0.541] (positive favours B'') | 0.15 |

Whole-run delivered units, A - B'' and B' - B'' (seed means over the common seeds):

| T | A - B'' | B' - B'' |
|---|---|---|
| 1 | 758 | 0 |
| 1p5 | 776 | 0 |
| 2 | 704 | 0 |
| 3 | 601 | 0 |
| inf | 0 | 0 |

OV-long-v identity B'' = B' (every id3e key): 48 runs, all identical


(OV-long-v: 48 new runs, 2071 s wall at 5 workers)



## OV-long (build B'', 14)

B'' = A + R5 (wait_limit_form rev5) + the net repair clamp (clamp_net). A and B' are the stored rows. Dominance on the paired steady window (12.2), best finite-T point per build, on the fixed reader (13.6). 

| row | seeds | whole-run delivered | on time (whole run) | repairs sent | decoded | resends | net_bind (mean) | repair credits removed (mean) | onset_t | y_max | w_min_over_c |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=0p4, hold 0 (S5) | 6 | 0.8120 | 0.8120 | 2377 | 520 | 400 | - | - | - | - | - |
| A; lifetime T=0p6, hold 0 (S5) | 6 | 0.8160 | 0.0380 | 2381 | 569 | 706 | - | - | - | - | - |
| A; lifetime T=1, hold 0 (S5) | 6 | 0.8239 | 0.0380 | 2386 | 636 | 1340 | - | - | - | - | - |
| A; lifetime T=2, hold 0 (S5) | 6 | 0.8438 | 0.0380 | 2401 | 817 | 2950 | - | - | - | - | - |
| A; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.0380 | 2506 | 2248 | 15768 | - | - | - | - | - |
| Bp; lifetime T=0p4, hold 0 (S5) | 6 | 0.8045 | 0.8045 | 2070 | 1822 | 12675 | - | - | 1.23 | 4689.68 | 0.0003 |
| Bp; lifetime T=0p6, hold 0 (S5) | 6 | 0.8045 | 0.8045 | 2065 | 1820 | 12682 | - | - | 1.24 | 3160.68 | 0.0005 |
| Bp; lifetime T=1, hold 0 (S5) | 6 | 0.8045 | 0.8045 | 2068 | 1824 | 12690 | - | - | 1.24 | 1913.66 | 0.0008 |
| Bp; lifetime T=2, hold 0 (S5) | 6 | 0.8045 | 0.8043 | 2068 | 1824 | 12691 | - | - | 1.26 | 963.39 | 0.0017 |
| Bp; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.0380 | 2506 | 2248 | 15768 | - | - | - | 0.00 | 0.8007 |
| Bpp; lifetime T=0p4, hold 0 (S5) | 6 | 0.8076 | 0.8076 | 313 | 127 | 12933 | 354412 | 3542.1 | 1.23 | 4707.35 | 0.0002 |
| Bpp; lifetime T=0p6, hold 0 (S5) | 6 | 0.8076 | 0.8076 | 313 | 128 | 12954 | 354023 | 3534.7 | 1.24 | 3166.95 | 0.0003 |
| Bpp; lifetime T=1, hold 0 (S5) | 6 | 0.8076 | 0.8075 | 313 | 129 | 12962 | 353664 | 3530.6 | 1.24 | 1916.60 | 0.0004 |
| Bpp; lifetime T=2, hold 0 (S5) | 6 | 0.8076 | 0.8074 | 321 | 137 | 12961 | 351973 | 3513.9 | 1.26 | 964.60 | 0.0008 |
| Bpp; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.0380 | 2506 | 2248 | 15768 | 0 | 0.0 | - | 0.00 | 0.8007 |
| Bpp; lifetime T=0p4, hold inf (S5) | 6 | 0.8076 | 0.8076 | 313 | 127 | 12933 | 354412 | 3542.1 | 1.23 | 4707.35 | 0.0002 |
| Bpp; lifetime T=2, hold inf (S5) | 6 | 0.8076 | 0.7687 | 321 | 137 | 12961 | 351973 | 3513.9 | 1.26 | 964.60 | 0.0008 |
| Bpp; lifetime T=inf, hold inf (S5) | 6 | 1.0000 | 0.0261 | 2506 | 2248 | 15768 | 0 | 0.0 | - | 0.00 | 0.8007 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 3 | 0.9974 | 0.0306 | 659 | 418 | 14467 | - | - | - | - | - |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 3 | 0.9974 | 0.0337 | 706 | 462 | 14467 | - | - | - | - | - |
| rt; today's Realtime, delta=0p5 (E-rt) | 3 | 0.8105 | 0.8099 | 267 | 96 | 42 | - | - | - | - | - |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | 3 | 0.8244 | 0.0403 | 267 | 96 | 9 | - | - | - | - | - |
| rt; today's Realtime, delta=50 (E-rt) | 3 | 0.8104 | 0.8098 | 292 | 91 | 44 | - | - | - | - | - |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 3 | 0.8243 | 0.0400 | 292 | 91 | 11 | - | - | - | - | - |
| bulk; never gives up, strict order = today's Bulk (C3) | 3 | 1.0000 | 0.0266 | 2514 | 2252 | 15739 | - | - | - | - | - |

Dominance (12.3):

| build | baseline | best point | share | on time | p50 | outcome |
|---|---|---|---|---|---|---|
| A | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=0p4 | -0.193 [-0.193, -0.192] | 0.805 [0.805, 0.805] | 5.33 [5.33, 5.33] | not dominated (a trade) |
| A | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=0p4 | -0.193 [-0.193, -0.192] | 0.805 [0.805, 0.805] | 4.74 [4.74, 4.74] | not dominated (a trade) |
| A | rt; today's Realtime, delta=0p5 (E-rt) | T=0p4 | -0.00345 [-0.00358, -0.00326] | -0.00346 [-0.00359, -0.00326] | -0.217 [-0.217, -0.217] | worse |
| A | rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | T=0p4 | -0.0043 [-0.00442, -0.00414] | 0.805 [0.805, 0.805] | 0.34 [0.34, 0.34] | dominates |
| A | rt; today's Realtime, delta=50 (E-rt) | T=0p4 | -0.00345 [-0.00358, -0.00326] | -0.00346 [-0.00359, -0.00326] | -0.217 [-0.217, -0.217] | worse |
| A | rteq; today's Realtime, delta=50 (E-rt) (engine queue) | T=0p4 | -0.00423 [-0.00431, -0.00416] | 0.805 [0.805, 0.805] | 0.34 [0.34, 0.34] | dominates |
| A | bulk; never gives up, strict order = today's Bulk (C3) | T=0p4 | -0.195 [-0.195, -0.195] | 0.805 [0.805, 0.805] | 4.98 [4.95, 5] | not dominated (a trade) |
| B' | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=0p4 | -0.194 [-0.195, -0.194] | 0.803 [0.803, 0.803] | 5.57 [5.57, 5.57] | not dominated (a trade) |
| B' | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=0p4 | -0.194 [-0.195, -0.194] | 0.803 [0.803, 0.803] | 4.98 [4.98, 4.98] | not dominated (a trade) |
| B' | rt; today's Realtime, delta=0p5 (E-rt) | T=0p4 | -0.00513 [-0.00523, -0.00493] | -0.00513 [-0.00523, -0.00493] | 0.0784 [0.0784, 0.0784] | dominates |
| B' | rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | T=0p4 | -0.00593 [-0.00602, -0.00575] | 0.803 [0.803, 0.803] | 0.635 [0.635, 0.635] | dominates |
| B' | rt; today's Realtime, delta=50 (E-rt) | T=0p4 | -0.00513 [-0.00523, -0.00493] | -0.00513 [-0.00523, -0.00493] | 0.0784 [0.0784, 0.0784] | dominates |
| B' | rteq; today's Realtime, delta=50 (E-rt) (engine queue) | T=0p4 | -0.00585 [-0.00592, -0.00577] | 0.803 [0.803, 0.803] | 0.635 [0.635, 0.635] | dominates |
| B' | bulk; never gives up, strict order = today's Bulk (C3) | T=0p4 | -0.197 [-0.197, -0.197] | 0.803 [0.803, 0.803] | 5.22 [5.19, 5.24] | not dominated (a trade) |
| B'' | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=0p4 | -0.191 [-0.191, -0.191] | 0.806 [0.806, 0.807] | 5.57 [5.57, 5.57] | not dominated (a trade) |
| B'' | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=0p4 | -0.191 [-0.191, -0.191] | 0.806 [0.806, 0.807] | 4.98 [4.98, 4.98] | not dominated (a trade) |
| B'' | rt; today's Realtime, delta=0p5 (E-rt) | T=0p4 | -0.00187 [-0.00198, -0.00168] | -0.00187 [-0.00198, -0.00168] | 0.0784 [0.0784, 0.0784] | dominates |
| B'' | rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | T=0p4 | -0.00269 [-0.00279, -0.00254] | 0.806 [0.806, 0.807] | 0.635 [0.635, 0.635] | dominates |
| B'' | rt; today's Realtime, delta=50 (E-rt) | T=0p4 | -0.00187 [-0.00198, -0.00168] | -0.00187 [-0.00198, -0.00168] | 0.0784 [0.0784, 0.0784] | dominates |
| B'' | rteq; today's Realtime, delta=50 (E-rt) (engine queue) | T=0p4 | -0.00262 [-0.00268, -0.00256] | 0.806 [0.806, 0.807] | 0.635 [0.635, 0.635] | dominates |
| B'' | bulk; never gives up, strict order = today's Bulk (C3) | T=0p4 | -0.194 [-0.194, -0.193] | 0.806 [0.806, 0.807] | 5.22 [5.19, 5.24] | not dominated (a trade) |

B'' against B' and against A at each lifetime, paired steady window (reported only; 'positive favours B''' reads 'positive favours the first'):

| T | readout | B'' - B' [95 %] | bar |
|---|---|---|---|
| 0p4 | steady share | 0.00324 [0.00321, 0.00326] (positive favours B'') | 0.00985 |
| 0p4 | steady ot | 0.00324 [0.00321, 0.00326] (positive favours B'') | 0.00985 |
| 0p4 | steady p50 | 0 [0, 0] (positive favours B'') | 0.0047 |
| 0p6 | steady share | 0.00327 [0.00323, 0.0033] (positive favours B'') | 0.00986 |
| 0p6 | steady ot | 0.00325 [0.0032, 0.00329] (positive favours B'') | 0.00986 |
| 0p6 | steady p50 | 0.000939 [0.000939, 0.000939] (positive favours B'') | 0.00474 |
| 1 | steady share | 0.00341 [0.00339, 0.00343] (positive favours B'') | 0.00987 |
| 1 | steady ot | 0.00339 [0.00336, 0.00341] (positive favours B'') | 0.00987 |
| 1 | steady p50 | 0.00189 [0.00189, 0.00189] (positive favours B'') | 0.00479 |
| 2 | steady share | 0.00385 [0.00382, 0.00388] (positive favours B'') | 0.00992 |
| 2 | steady ot | 0.0038 [0.00376, 0.00383] (positive favours B'') | 0.00992 |
| 2 | steady p50 | 0.00397 [0.00397, 0.00397] (positive favours B'') | 0.00508 |
| T | readout | B'' - A [95 %] | bar |
|---|---|---|---|
| 0p4 | steady share | 0.00151 [0.00143, 0.00158] (positive favours B'') | 0.00977 |
| 0p4 | steady ot | 0.00151 [0.00143, 0.00159] (positive favours B'') | 0.00977 |
| 0p4 | steady p50 | 0.296 [0.296, 0.296] (positive favours B'') | 0.0195 |
| 0p6 | steady share | 0.00146 [0.00138, 0.00153] (positive favours B'') | 0.00977 |
| 0p6 | steady ot | 0.806 [0.806, 0.806] (positive favours B'') | 0.05 |
| 0p6 | steady p50 | 0.498 [0.498, 0.498] (positive favours B'') | 0.0296 |
| 1 | steady share | 0.00147 [0.00134, 0.0016] (positive favours B'') | 0.00977 |
| 1 | steady ot | 0.806 [0.806, 0.806] (positive favours B'') | 0.05 |
| 1 | steady p50 | 0.899 [0.899, 0.899] (positive favours B'') | 0.0496 |
| 2 | steady share | 0.0013 [0.00124, 0.00136] (positive favours B'') | 0.00976 |
| 2 | steady ot | 0.806 [0.806, 0.806] (positive favours B'') | 0.05 |
| 2 | steady p50 | 1.9 [1.9, 1.9] (positive favours B'') | 0.0996 |

Whole-run delivered units, A - B'' and B' - B'' (seed means over the common seeds):

| T | A - B'' | B' - B'' |
|---|---|---|
| 0p4 | 2096 | -1489 |
| 0p6 | 4028 | -1482 |
| 1 | 7852 | -1482 |
| 2 | 17398 | -1466 |
| inf | 0 | 0 |

(OV-long: 48 new runs, 3285 s wall at 5 workers)



## OV-long-m (build B'', 14)

B'' = A + R5 (wait_limit_form rev5) + the net repair clamp (clamp_net). A and B' are the stored rows. Dominance on the paired steady window (12.2), best finite-T point per build, on the fixed reader (13.6). 

| row | seeds | whole-run delivered | on time (whole run) | repairs sent | decoded | resends | net_bind (mean) | repair credits removed (mean) | onset_t | y_max | w_min_over_c |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=0p4, hold 0 (S5) | 6 | 0.8122 | 0.8122 | 2274 | 543 | 418 | - | - | - | - | - |
| A; lifetime T=0p6, hold 0 (S5) | 6 | 0.8162 | 0.0391 | 2276 | 584 | 727 | - | - | - | - | - |
| A; lifetime T=1, hold 0 (S5) | 6 | 0.8242 | 0.0390 | 2281 | 657 | 1362 | - | - | - | - | - |
| A; lifetime T=2, hold 0 (S5) | 6 | 0.8441 | 0.0390 | 2295 | 838 | 2972 | - | - | - | - | - |
| A; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.0390 | 2400 | 2257 | 15774 | - | - | - | - | - |
| Bp; lifetime T=0p4, hold 0 (S5) | 6 | 0.8059 | 0.8059 | 2070 | 1597 | 10735 | - | - | 5.43 | 9.53 | 0.1561 |
| Bp; lifetime T=0p6, hold 0 (S5) | 6 | 0.8074 | 0.6133 | 2009 | 1829 | 12488 | - | - | 7.44 | 6.53 | 0.2210 |
| Bp; lifetime T=1, hold 0 (S5) | 6 | 0.8123 | 0.0580 | 2012 | 1840 | 12613 | - | - | 11.41 | 3.92 | 0.3357 |
| Bp; lifetime T=2, hold 0 (S5) | 6 | 0.8309 | 0.0390 | 2022 | 1892 | 13106 | - | - | 21.41 | 1.95 | 0.5535 |
| Bp; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.0390 | 2400 | 2257 | 15774 | - | - | - | 0.00 | 0.8005 |
| Bpp; lifetime T=0p4, hold 0 (S5) | 6 | 0.8085 | 0.8085 | 236 | 148 | 11230 | 353367 | 3552.1 | 5.40 | 9.52 | 0.0864 |
| Bpp; lifetime T=0p6, hold 0 (S5) | 6 | 0.8091 | 0.6556 | 262 | 192 | 12690 | 345025 | 3486.1 | 7.41 | 6.45 | 0.1287 |
| Bpp; lifetime T=1, hold 0 (S5) | 6 | 0.8124 | 0.3338 | 333 | 262 | 12779 | 331106 | 3306.7 | 11.38 | 3.93 | 0.2125 |
| Bpp; lifetime T=2, hold 0 (S5) | 6 | 0.8273 | 0.0390 | 532 | 454 | 13231 | 298195 | 2973.8 | 21.38 | 1.95 | 0.4236 |
| Bpp; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.0390 | 2400 | 2257 | 15774 | 0 | 0.0 | - | 0.00 | 0.8005 |
| rt; today's Realtime, delta=0p1 (E-rt) | 3 | 0.8107 | 0.8101 | 186 | 110 | 44 | - | - | - | - | - |
| rteq; today's Realtime, delta=0p1 (E-rt) (engine queue) | 3 | 0.8246 | 0.0413 | 186 | 110 | 13 | - | - | - | - | - |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 3 | 0.9973 | 0.0316 | 560 | 431 | 14471 | - | - | - | - | - |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 3 | 0.9974 | 0.0349 | 604 | 468 | 14467 | - | - | - | - | - |
| bulk; never gives up, strict order = today's Bulk (C3) | 3 | 1.0000 | 0.0257 | 2405 | 2255 | 15760 | - | - | - | - | - |

Dominance (12.3):

| build | baseline | best point | share | on time | p50 | outcome |
|---|---|---|---|---|---|---|
| A | rt; today's Realtime, delta=0p1 (E-rt) | T=0p4 | -0.00344 [-0.0036, -0.00323] | -0.00344 [-0.0036, -0.00323] | -0.217 [-0.217, -0.217] | worse |
| A | rteq; today's Realtime, delta=0p1 (E-rt) (engine queue) | T=0p4 | -0.00421 [-0.00428, -0.00408] | 0.805 [0.805, 0.805] | 0.34 [0.34, 0.34] | dominates |
| A | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=0p4 | -0.193 [-0.193, -0.192] | 0.805 [0.805, 0.805] | 5.33 [5.33, 5.33] | not dominated (a trade) |
| A | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=0p4 | -0.193 [-0.193, -0.192] | 0.805 [0.805, 0.805] | 4.74 [4.74, 4.74] | not dominated (a trade) |
| A | bulk; never gives up, strict order = today's Bulk (C3) | T=0p4 | -0.195 [-0.195, -0.195] | 0.805 [0.805, 0.805] | 4.96 [4.95, 5] | not dominated (a trade) |
| B' | rt; today's Realtime, delta=0p1 (E-rt) | T=0p4 | -0.00898 [-0.00908, -0.0088] | -0.00899 [-0.00909, -0.00882] | -0.0258 [-0.0258, -0.0258] | worse |
| B' | rteq; today's Realtime, delta=0p1 (E-rt) (engine queue) | T=0p4 | -0.00992 [-0.01, -0.00981] | 0.799 [0.799, 0.799] | 0.538 [0.537, 0.539] | not dominated (a trade) |
| B' | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=0p4 | -0.198 [-0.198, -0.198] | 0.799 [0.799, 0.799] | 5.52 [5.52, 5.52] | not dominated (a trade) |
| B' | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=0p4 | -0.198 [-0.198, -0.198] | 0.799 [0.799, 0.799] | 4.93 [4.93, 4.93] | not dominated (a trade) |
| B' | bulk; never gives up, strict order = today's Bulk (C3) | T=0p4 | -0.201 [-0.201, -0.201] | 0.799 [0.799, 0.799] | 5.16 [5.14, 5.19] | not dominated (a trade) |
| B'' | rt; today's Realtime, delta=0p1 (E-rt) | T=0p4 | -0.00632 [-0.00643, -0.0062] | -0.00632 [-0.00643, -0.0062] | 0.0127 [0.0116, 0.0132] | dominates |
| B'' | rteq; today's Realtime, delta=0p1 (E-rt) (engine queue) | T=0p4 | -0.0072 [-0.00742, -0.00706] | 0.802 [0.802, 0.802] | 0.576 [0.575, 0.576] | dominates |
| B'' | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=0p4 | -0.195 [-0.196, -0.195] | 0.802 [0.802, 0.802] | 5.56 [5.56, 5.56] | not dominated (a trade) |
| B'' | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=0p4 | -0.196 [-0.196, -0.195] | 0.802 [0.802, 0.802] | 4.97 [4.97, 4.97] | not dominated (a trade) |
| B'' | bulk; never gives up, strict order = today's Bulk (C3) | T=0p4 | -0.198 [-0.198, -0.198] | 0.802 [0.802, 0.802] | 5.19 [5.17, 5.23] | not dominated (a trade) |

B'' against B' and against A at each lifetime, paired steady window (reported only; 'positive favours B''' reads 'positive favours the first'):

| T | readout | B'' - B' [95 %] | bar |
|---|---|---|---|
| 0p4 | steady share | 0.00264 [0.0026, 0.00268] (positive favours B'') | 0.01 |
| 0p4 | steady ot | 0.00265 [0.00262, 0.00269] (positive favours B'') | 0.01 |
| 0p4 | steady p50 | 0.0387 [0.0381, 0.0389] (positive favours B'') | 0.0099 |
| 0p6 | steady share | 0.00199 [0.00194, 0.00205] (positive favours B'') | 0.0102 |
| 0p6 | steady ot | 0.0472 [0.0464, 0.0481] (positive favours B'') | 0.0187 |
| 0p6 | steady p50 | 0.0677 [0.0668, 0.0685] (positive favours B'') | 0.0161 |
| 1 | steady share | 0.000364 [0.000304, 0.000415] (positive favours B'') | 0.0103 |
| 1 | steady ot | 0.316 [0.312, 0.319] (positive favours B'') | 0.0491 |
| 1 | steady p50 | 0.108 [0.106, 0.109] (positive favours B'') | 0.0316 |
| 2 | steady share | -0.00543 [-0.00552, -0.00536] (positive favours B'') | 0.0103 |
| 2 | steady ot | 0 [0, 0] (positive favours B'') | 0.05 |
| 2 | steady p50 | -0.0107 [-0.016, -0.00534] (positive favours B'') | 0.08 |
| T | readout | B'' - A [95 %] | bar |
|---|---|---|---|
| 0p4 | steady share | -0.00299 [-0.00308, -0.00289] (positive favours B'') | 0.00976 |
| 0p4 | steady ot | -0.00299 [-0.00308, -0.00289] (positive favours B'') | 0.00976 |
| 0p4 | steady p50 | 0.23 [0.23, 0.231] (positive favours B'') | 0.0195 |
| 0p6 | steady share | -0.00616 [-0.00625, -0.00607] (positive favours B'') | 0.00976 |
| 0p6 | steady ot | 0.683 [0.683, 0.684] (positive favours B'') | 0.05 |
| 0p6 | steady p50 | 0.34 [0.34, 0.34] (positive favours B'') | 0.0296 |
| 1 | steady share | -0.0114 [-0.0115, -0.0113] (positive favours B'') | 0.00977 |
| 1 | steady ot | 0.339 [0.337, 0.34] (positive favours B'') | 0.05 |
| 1 | steady p50 | 0.477 [0.475, 0.478] (positive favours B'') | 0.0496 |
| 2 | steady share | -0.0177 [-0.0179, -0.0175] (positive favours B'') | 0.00976 |
| 2 | steady ot | 0 [0, 0] (positive favours B'') | 0.05 |
| 2 | steady p50 | 0.469 [0.469, 0.469] (positive favours B'') | 0.0996 |

Whole-run delivered units, A - B'' and B' - B'' (seed means over the common seeds):

| T | A - B'' | B' - B'' |
|---|---|---|
| 0p4 | 1753 | -1245 |
| 0p6 | 3384 | -833 |
| 1 | 5667 | -6 |
| 2 | 8040 | 1732 |
| inf | 0 | 0 |

R5's tail (14.3): w t_ov = (c / xb) s_b per window past the onset:

| T | windows | median abs(w t_ov / ((c/xb) s_b) - 1) | max |
|---|---|---|---|
| 0p4 | 420 | 0.0019 | 0.0047 |
| 0p6 | 395 | 0.0014 | 0.0093 |
| 1 | 342 | 0.0007 | 0.0088 |
| 2 | 216 | 0.0003 | 0.0027 |

(OV-long-m: 30 new runs, 2169 s wall at 5 workers)



## OV-long-q (build B'', 14)

B'' = A + R5 (wait_limit_form rev5) + the net repair clamp (clamp_net). A and B' are the stored rows. Dominance on the paired steady window (12.2), best finite-T point per build, on the fixed reader (13.6). 

| row | seeds | whole-run delivered | on time (whole run) | repairs sent | decoded | resends | net_bind (mean) | repair credits removed (mean) | onset_t | y_max | w_min_over_c |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=0p4, hold 0 (S5) | 6 | 0.8122 | 0.8121 | 2282 | 536 | 416 | - | - | - | - | - |
| A; lifetime T=0p6, hold 0 (S5) | 6 | 0.8161 | 0.0390 | 2284 | 566 | 725 | - | - | - | - | - |
| A; lifetime T=1, hold 0 (S5) | 6 | 0.8241 | 0.0389 | 2290 | 649 | 1360 | - | - | - | - | - |
| A; lifetime T=2, hold 0 (S5) | 6 | 0.8440 | 0.0389 | 2303 | 823 | 2972 | - | - | - | - | - |
| A; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.0389 | 2410 | 2259 | 15782 | - | - | - | - | - |
| Bp; lifetime T=0p4, hold 0 (S5) | 6 | 0.8057 | 0.8057 | 2064 | 1642 | 11051 | - | - | 4.76 | 11.23 | 0.1342 |
| Bp; lifetime T=0p6, hold 0 (S5) | 6 | 0.8070 | 0.6453 | 2009 | 1830 | 12524 | - | - | 6.46 | 7.67 | 0.1914 |
| Bp; lifetime T=1, hold 0 (S5) | 6 | 0.8113 | 0.1744 | 2013 | 1838 | 12635 | - | - | 9.84 | 4.63 | 0.2931 |
| Bp; lifetime T=2, hold 0 (S5) | 6 | 0.8281 | 0.0389 | 2022 | 1884 | 13071 | - | - | 18.32 | 2.31 | 0.4941 |
| Bp; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.0389 | 2410 | 2259 | 15782 | - | - | - | 0.00 | 0.8000 |
| Bpp; lifetime T=0p4, hold 0 (S5) | 6 | 0.8084 | 0.8084 | 245 | 148 | 11496 | 353027 | 3546.8 | 4.73 | 11.23 | 0.0732 |
| Bpp; lifetime T=0p6, hold 0 (S5) | 6 | 0.8089 | 0.6812 | 270 | 190 | 12742 | 344833 | 3470.9 | 6.43 | 7.60 | 0.1088 |
| Bpp; lifetime T=1, hold 0 (S5) | 6 | 0.8116 | 0.4074 | 342 | 262 | 12815 | 330853 | 3304.1 | 9.82 | 4.64 | 0.1804 |
| Bpp; lifetime T=2, hold 0 (S5) | 6 | 0.8242 | 0.0389 | 532 | 446 | 13186 | 298336 | 2974.4 | 18.29 | 2.31 | 0.3584 |
| Bpp; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.0389 | 2410 | 2259 | 15782 | 0 | 0.0 | - | 0.00 | 0.8000 |
| rt; today's Realtime, delta=0p118 (E-rt) | 3 | 0.8107 | 0.8100 | 195 | 109 | 44 | - | - | - | - | - |
| rteq; today's Realtime, delta=0p118 (E-rt) (engine queue) | 3 | 0.8246 | 0.0412 | 195 | 109 | 13 | - | - | - | - | - |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 3 | 0.9973 | 0.0315 | 568 | 429 | 14471 | - | - | - | - | - |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 3 | 0.9974 | 0.0348 | 612 | 472 | 14469 | - | - | - | - | - |
| bulk; never gives up, strict order = today's Bulk (C3) | 3 | 1.0000 | 0.0273 | 2417 | 2261 | 15761 | - | - | - | - | - |

Dominance (12.3):

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
| B'' | rt; today's Realtime, delta=0p118 (E-rt) | T=0p4 | -0.00646 [-0.00656, -0.00632] | -0.00646 [-0.00656, -0.00632] | 0.0224 [0.0224, 0.0224] | dominates |
| B'' | rteq; today's Realtime, delta=0p118 (E-rt) (engine queue) | T=0p4 | -0.00697 [-0.00707, -0.00692] | 0.802 [0.802, 0.802] | 0.584 [0.584, 0.585] | dominates |
| B'' | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=0p4 | -0.196 [-0.196, -0.195] | 0.802 [0.802, 0.802] | 5.57 [5.57, 5.57] | not dominated (a trade) |
| B'' | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=0p4 | -0.196 [-0.196, -0.195] | 0.802 [0.802, 0.802] | 4.98 [4.98, 4.98] | not dominated (a trade) |
| B'' | bulk; never gives up, strict order = today's Bulk (C3) | T=0p4 | -0.198 [-0.198, -0.198] | 0.802 [0.802, 0.802] | 5.2 [5.19, 5.24] | not dominated (a trade) |

B'' against B' and against A at each lifetime, paired steady window (reported only; 'positive favours B''' reads 'positive favours the first'):

| T | readout | B'' - B' [95 %] | bar |
|---|---|---|---|
| 0p4 | steady share | 0.00269 [0.00266, 0.00273] (positive favours B'') | 0.0101 |
| 0p4 | steady ot | 0.00271 [0.00267, 0.00274] (positive favours B'') | 0.0101 |
| 0p4 | steady p50 | 0.0348 [0.0348, 0.0348] (positive favours B'') | 0.00924 |
| 0p6 | steady share | 0.00209 [0.00204, 0.00215] (positive favours B'') | 0.0102 |
| 0p6 | steady ot | 0.04 [0.0391, 0.0409] (positive favours B'') | 0.0169 |
| 0p6 | steady p50 | 0.0674 [0.0664, 0.0679] (positive favours B'') | 0.0149 |
| 1 | steady share | 0.00062 [0.000555, 0.000703] (positive favours B'') | 0.0104 |
| 1 | steady ot | 0.271 [0.269, 0.273] (positive favours B'') | 0.0426 |
| 1 | steady p50 | 0.118 [0.118, 0.119] (positive favours B'') | 0.029 |
| 2 | steady share | -0.00588 [-0.00603, -0.00576] (positive favours B'') | 0.0105 |
| 2 | steady ot | 0 [0, 0] (positive favours B'') | 0.05 |
| 2 | steady p50 | 0.0975 [0.0927, 0.1] (positive favours B'') | 0.0742 |
| T | readout | B'' - A [95 %] | bar |
|---|---|---|---|
| 0p4 | steady share | -0.0031 [-0.00318, -0.00302] (positive favours B'') | 0.00977 |
| 0p4 | steady ot | -0.0031 [-0.00318, -0.00302] (positive favours B'') | 0.00977 |
| 0p4 | steady p50 | 0.24 [0.24, 0.24] (positive favours B'') | 0.0195 |
| 0p6 | steady share | -0.00635 [-0.00643, -0.00627] (positive favours B'') | 0.00977 |
| 0p6 | steady ot | 0.712 [0.711, 0.713] (positive favours B'') | 0.05 |
| 0p6 | steady p50 | 0.364 [0.364, 0.365] (positive favours B'') | 0.0296 |
| 1 | steady share | -0.0121 [-0.0122, -0.012] (positive favours B'') | 0.00977 |
| 1 | steady ot | 0.427 [0.426, 0.428] (positive favours B'') | 0.05 |
| 1 | steady p50 | 0.54 [0.54, 0.54] (positive favours B'') | 0.0496 |
| 2 | steady share | -0.0221 [-0.0223, -0.0219] (positive favours B'') | 0.00977 |
| 2 | steady ot | 0 [0, 0] (positive favours B'') | 0.05 |
| 2 | steady p50 | 0.678 [0.674, 0.681] (positive favours B'') | 0.0996 |

Whole-run delivered units, A - B'' and B' - B'' (seed means over the common seeds):

| T | A - B'' | B' - B'' |
|---|---|---|
| 0p4 | 1805 | -1267 |
| 0p6 | 3482 | -913 |
| 1 | 6016 | -121 |
| 2 | 9513 | 1886 |
| inf | 0 | 0 |

R5's tail (14.3): w t_ov = (c / xb) s_b per window past the onset:

| T | windows | median abs(w t_ov / ((c/xb) s_b) - 1) | max |
|---|---|---|---|
| 0p4 | 426 | 0.0021 | 0.0063 |
| 0p6 | 407 | 0.0011 | 0.0087 |
| 1 | 364 | 0.0006 | 0.0085 |
| 2 | 258 | 0.0003 | 0.0024 |

(OV-long-q: 30 new runs, 1824 s wall at 5 workers)



### CL-long


(B'' rows are labelled 'Bpp'.)

Builds: the new curve here is B'' (A + R5 + the net repair clamp, 14); A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). 

Labelled post-data: build A's CL-long rows (and the clamp rows of the new model there) were sized in step 2 (three-knobs-revision4.md 9.1) before this registration; they confirm nothing. What is new here is the baselines under the clamp and elig = rack.
| row | seeds | whole-run delivered | on time within 0.4s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Bpp; lifetime T=0p4, hold 0 (S5) | 12 | 0.9412 | 0.9412 | 0.00 | 0.005 | 46 | 46 | 4588 (3747) | 0 | 0 | 0 |
| Bpp; lifetime T=0p6, hold 0 (S5) | 12 | 0.9515 | 0.9058 | 0.59 | 0.005 | 37 | 37 | 4748 (3600) | 0 | 0 | 0 |
| Bpp; lifetime T=0p8, hold 0 (S5) | 12 | 0.9616 | 0.8568 | 1.54 | 0.006 | 29 | 29 | 4906 (3446) | 0 | 0 | 0 |
| Bpp; lifetime T=1, hold 0 (S5) | 12 | 0.9712 | 0.8074 | 2.53 | 0.006 | 22 | 22 | 5071 (3303) | 0 | 0 | 0 |
| Bpp; lifetime T=1p2, hold 0 (S5) | 12 | 0.9814 | 0.7583 | 3.49 | 0.006 | 14 | 14 | 5221 (3145) | 0 | 0 | 0 |
| Bpp; lifetime T=1p25, hold 0 (S5) | 12 | 0.9841 | 0.7463 | 3.75 | 0.006 | 11 | 11 | 5263 (3107) | 0 | 0 | 0 |
| Bpp; lifetime T=1p3, hold 0 (S5) | 12 | 0.9866 | 0.7341 | 3.99 | 0.006 | 9 | 9 | 5306 (3067) | 0 | 0 | 0 |
| Bpp; lifetime T=1p35, hold 0 (S5) | 12 | 0.9891 | 0.7218 | 4.28 | 0.006 | 7 | 7 | 5332 (3017) | 0 | 0 | 0 |
| Bpp; lifetime T=1p4, hold 0 (S5) | 12 | 0.9908 | 0.7098 | 4.50 | 0.007 | 6 | 6 | 5379 (2985) | 0 | 0 | 0 |
| Bpp; lifetime T=1p45, hold 0 (S5) | 12 | 0.9932 | 0.6967 | 4.74 | 0.005 | 3 | 3 | 5425 (2943) | 0 | 0 | 0 |
| Bpp; lifetime T=1p5, hold 0 (S5) | 12 | 0.9953 | 0.6851 | 4.96 | 0.005 | 2 | 2 | 5567 (2952) | 0 | 0 | 0 |
| Bpp; lifetime T=1p55, hold 0 (S5) | 12 | 0.9956 | 0.6718 | 5.24 | 0.005 | 1 | 1 | 5644 (2908) | 0 | 0 | 0 |
| Bpp; lifetime T=1p6, hold 0 (S5) | 12 | 0.9959 | 0.6609 | 5.47 | 0.004 | 1 | 1 | 5736 (2875) | 0 | 0 | 0 |
| Bpp; lifetime T=2, hold 0 (S5) | 12 | 0.9989 | 0.6358 | 5.96 | 0.002 | 0 | 0 | 6410 (2837) | 0 | 13 | 0 |
| Bpp; lifetime T=inf, hold 0 (S5) | 12 | 1.0000 | 0.6300 | 6.08 | 0.000 | 0 | 0 | 6661 (2824) | 0 | 0 | 0 |
| Bpp; lifetime T=0p4, hold inf (S5) | 12 | 0.9412 | 0.9352 | 0.00 | 0.005 | 46 | 46 | 4588 (3747) | 0 | 0 | 0 |
| Bpp; lifetime T=1p4, hold inf (S5) | 12 | 0.9908 | 0.6818 | 5.09 | 0.007 | 6 | 6 | 5379 (2985) | 0 | 0 | 0 |
| Bpp; lifetime T=inf, hold inf (S5) | 12 | 1.0000 | 0.6015 | 6.69 | 0.000 | 0 | 0 | 6661 (2824) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 12 | 0.9940 | 0.6448 | 5.67 | 0.011 | 4018 | 0 | 5292 (2474) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | 12 | 0.9940 | 0.6610 | 5.35 | 0.011 | 3339 | 0 | 5308 (2570) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | 12 | 0.9941 | 0.6788 | 4.98 | 0.011 | 1954 | 0 | 5236 (2659) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | 12 | 0.9941 | 0.6788 | 4.98 | 0.011 | 429 | 0 | 5241 (2661) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 12 | 0.9941 | 0.6788 | 4.98 | 0.011 | 0 | 0 | 5301 (2663) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p5 (E-rt) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4671 (4181) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4671 (4181) | 0 | 0 | 0 |
| rt; today's Realtime, delta=5 (E-rt) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4554 (4078) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4554 (4078) | 0 | 0 | 0 |
| rt; today's Realtime, delta=50 (E-rt) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4446 (3980) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4446 (3980) | 0 | 0 | 0 |
| bulk; never gives up, strict order = today's Bulk (C3) | 12 | 1.0000 | 0.6015 | 6.69 | 0.000 | 0 | 0 | 6661 (2824) | 0 | 0 | 0 |

Matched share. Outage cell: the axis is the whole-run delivered share and the readouts are the whole-run share on time and the registered recovery time, as in stage 3e; the steady p50 is on the paired window (outage and recovery excluded). The new curve is B''s H = 0 sweep, interpolated per seed at the baseline's share, against the baseline; a baseline below the curve's lowest share reads the lowest point. Positive favours B''. Bars: shares max(0.05 (1 - ref), 1/units, NF), p50 max(NF_p50, 0.05 ref), recovery max(0.05 ref, 0.1 s, NF).

| baseline | readout | difference [95 %] | bar | outcome |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | whole-run on time | 0.0479 [0.045, 0.0511] | 0.0178 | shown better |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery | 0.852 [0.791, 0.916] | 0.283 | shown better |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | 0.000372 [0.000358, 0.000389] | 0.000302 | shown better |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | whole-run on time | 0.0314 [0.0286, 0.0345] | 0.0169 | shown better |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery | 0.53 [0.471, 0.593] | 0.268 | shown better |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | 0.000361 [0.000345, 0.000379] | 0.000302 | shown better |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | whole-run on time | 0.0134 [0.0108, 0.0163] | 0.0161 | undetermined |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery | 0.159 [0.0969, 0.225] | 0.249 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | 0.000282 [0.000267, 0.000294] | 0.0003 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | whole-run on time | 0.0134 [0.0108, 0.0163] | 0.0161 | undetermined |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery | 0.158 [0.0966, 0.225] | 0.249 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | 0.000267 [0.000252, 0.000281] | 0.000299 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | whole-run on time | 0.0134 [0.0104, 0.0172] | 0.0161 | undetermined |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | recovery | 0.157 [0.0811, 0.249] | 0.249 | undetermined |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | 0.000267 [0.000252, 0.000281] | 0.000299 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | whole-run on time | 0.0161 [0.0157, 0.0164] | 0.00375 | shown better |
| rt; today's Realtime, delta=0p5 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | 0.000228 [0.000228, 0.000228] | 0.000292 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | whole-run on time | 0.0161 [0.0157, 0.0164] | 0.00375 | shown better |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | 0.000228 [0.000228, 0.000228] | 0.000292 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | whole-run on time | 0.0161 [0.0157, 0.0165] | 0.00375 | shown better |
| rt; today's Realtime, delta=5 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | 0.000223 [0.000214, 0.000228] | 0.000292 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | whole-run on time | 0.0161 [0.0157, 0.0165] | 0.00375 | shown better |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | 0.000223 [0.000214, 0.000228] | 0.000292 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | whole-run on time | 0.0161 [0.0157, 0.0164] | 0.00375 | shown better |
| rt; today's Realtime, delta=50 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | 0.000209 [0.000194, 0.000223] | 0.000291 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | whole-run on time | 0.0161 [0.0157, 0.0164] | 0.00375 | shown better |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | 0.000209 [0.000194, 0.000223] | 0.000291 | equivalent |
| bulk; never gives up, strict order = today's Bulk (C3) | all readouts | identity against S5 at T=inf, H=inf: identical on every key and trace at 12 seeds | - | equivalent (identity) |

Words against every baseline: build A (stored rows, re-read now with the same reader and seeds) and B'' (14.5: every word unchanged is predicted):

| baseline | readout | A | B'' | changed |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | whole-run on time | shown better | shown better |  |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery | shown better | shown better |  |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | shown better | shown better |  |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | whole-run on time | shown better | shown better |  |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery | shown better | shown better |  |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | shown better | shown better |  |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | whole-run on time | equivalent | undetermined | **changed** |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | whole-run on time | equivalent | undetermined | **changed** |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | whole-run on time | undetermined | undetermined |  |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | recovery | equivalent | undetermined | **changed** |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | equivalent | equivalent |  |
| rt; today's Realtime, delta=0p5 (E-rt) | whole-run on time | shown better | shown better |  |
| rt; today's Realtime, delta=0p5 (E-rt) | recovery | equivalent | equivalent |  |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | equivalent | equivalent |  |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | whole-run on time | shown better | shown better |  |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | recovery | equivalent | equivalent |  |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | equivalent | equivalent |  |
| rt; today's Realtime, delta=5 (E-rt) | whole-run on time | shown better | shown better |  |
| rt; today's Realtime, delta=5 (E-rt) | recovery | equivalent | equivalent |  |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | equivalent | equivalent |  |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | whole-run on time | shown better | shown better |  |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | recovery | equivalent | equivalent |  |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | equivalent | equivalent |  |
| rt; today's Realtime, delta=50 (E-rt) | whole-run on time | shown better | shown better |  |
| rt; today's Realtime, delta=50 (E-rt) | recovery | equivalent | equivalent |  |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | equivalent | equivalent |  |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | whole-run on time | shown better | shown better |  |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | recovery | equivalent | equivalent |  |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | equivalent | equivalent |  |
| bulk; never gives up, strict order = today's Bulk (C3) | all readouts | equivalent (identity) | equivalent (identity) |  |

B'' against the stored A at each row (id3e keys, seeds 0-11); net clamp gauges:

| row | seeds identical | whole-run delivered A / B'' | whole-run on time A / B'' | recovery A / B'' | net_bind (mean) | credits removed (mean) | repairs A / B'' |
|---|---|---|---|---|---|---|---|
| Bpp; lifetime T=0p4, hold 0 (S5) | 0 / 12 | 0.9396 / 0.9412 | 0.9396 / 0.9412 | 0.00 / 0.00 | 9399 | 441.6 | 19159 / 18840 |
| Bpp; lifetime T=0p6, hold 0 (S5) | 0 / 12 | 0.9500 / 0.9515 | 0.9076 / 0.9058 | 0.52 / 0.59 | 9389 | 438.7 | 18156 / 17840 |
| Bpp; lifetime T=0p8, hold 0 (S5) | 0 / 12 | 0.9601 / 0.9616 | 0.8586 / 0.8568 | 1.51 / 1.54 | 9382 | 435.6 | 17147 / 16834 |
| Bpp; lifetime T=1, hold 0 (S5) | 0 / 12 | 0.9697 / 0.9712 | 0.8092 / 0.8074 | 2.50 / 2.53 | 9378 | 432.2 | 16130 / 15818 |
| Bpp; lifetime T=1p2, hold 0 (S5) | 0 / 12 | 0.9799 / 0.9814 | 0.7601 / 0.7583 | 3.46 / 3.49 | 9367 | 429.4 | 15118 / 14812 |
| Bpp; lifetime T=1p25, hold 0 (S5) | 0 / 12 | 0.9826 / 0.9841 | 0.7480 / 0.7463 | 3.71 / 3.75 | 9359 | 428.2 | 14868 / 14564 |
| Bpp; lifetime T=1p3, hold 0 (S5) | 0 / 12 | 0.9851 / 0.9866 | 0.7357 / 0.7341 | 3.97 / 3.99 | 9357 | 427.8 | 14616 / 14311 |
| Bpp; lifetime T=1p35, hold 0 (S5) | 0 / 12 | 0.9876 / 0.9891 | 0.7235 / 0.7218 | 4.22 / 4.28 | 9351 | 426.8 | 14362 / 14059 |
| Bpp; lifetime T=1p4, hold 0 (S5) | 0 / 12 | 0.9897 / 0.9908 | 0.7107 / 0.7098 | 4.46 / 4.50 | 9015 | 342.1 | 14097 / 13862 |
| Bpp; lifetime T=1p45, hold 0 (S5) | 0 / 12 | 0.9931 / 0.9932 | 0.6953 / 0.6967 | 4.78 / 4.74 | 7758 | 151.8 | 13773 / 13715 |
| Bpp; lifetime T=1p5, hold 0 (S5) | 12 / 12 | 0.9953 / 0.9953 | 0.6851 / 0.6851 | 4.96 / 4.96 | 0 | 0.0 | 13524 / 13524 |
| Bpp; lifetime T=1p55, hold 0 (S5) | 12 / 12 | 0.9956 / 0.9956 | 0.6718 / 0.6718 | 5.24 / 5.24 | 0 | 0.0 | 13291 / 13291 |
| Bpp; lifetime T=1p6, hold 0 (S5) | 12 / 12 | 0.9959 / 0.9959 | 0.6609 / 0.6609 | 5.47 / 5.47 | 0 | 0.0 | 13076 / 13076 |
| Bpp; lifetime T=2, hold 0 (S5) | 12 / 12 | 0.9989 / 0.9989 | 0.6358 / 0.6358 | 5.96 / 5.96 | 0 | 0.0 | 12852 / 12852 |
| Bpp; lifetime T=inf, hold 0 (S5) | 12 / 12 | 1.0000 / 1.0000 | 0.6300 / 0.6300 | 6.08 / 6.08 | 0 | 0.0 | 12729 / 12729 |
| Bpp; lifetime T=0p4, hold inf (S5) | 0 / 12 | 0.9396 / 0.9412 | 0.9365 / 0.9352 | 0.00 / 0.00 | 9399 | 441.6 | 19159 / 18840 |
| Bpp; lifetime T=1p4, hold inf (S5) | 0 / 12 | 0.9897 / 0.9908 | 0.6821 / 0.6818 | 5.09 / 5.09 | 9015 | 342.1 | 14097 / 13862 |
| Bpp; lifetime T=inf, hold inf (S5) | 12 / 12 | 1.0000 / 1.0000 | 0.6015 / 0.6015 | 6.69 / 6.69 | 0 | 0.0 | 12729 / 12729 |

### CL-long after the registered doubling (arms: ['CL-long | Bpp | lifetime T=0p4, hold 0 (S5)', 'CL-long | Bpp | lifetime T=0p6, hold 0 (S5)', 'CL-long | Bpp | lifetime T=0p8, hold 0 (S5)', 'CL-long | Bpp | lifetime T=1, hold 0 (S5)', 'CL-long | Bpp | lifetime T=1p2, hold 0 (S5)', 'CL-long | Bpp | lifetime T=1p25, hold 0 (S5)', 'CL-long | Bpp | lifetime T=1p3, hold 0 (S5)', 'CL-long | Bpp | lifetime T=1p35, hold 0 (S5)', 'CL-long | Bpp | lifetime T=1p4, hold 0 (S5)', 'CL-long | Bpp | lifetime T=1p45, hold 0 (S5)', 'CL-long | Bpp | lifetime T=1p5, hold 0 (S5)', 'CL-long | Bpp | lifetime T=1p55, hold 0 (S5)', 'CL-long | Bpp | lifetime T=1p6, hold 0 (S5)', 'CL-long | Bpp | lifetime T=2, hold 0 (S5)', 'CL-long | Bpp | lifetime T=inf, hold 0 (S5)', "CL-long | adr | ADR-0084 sender at rho=0p99, today's receiver (C1)", "CL-long | adr | ADR-0084 sender at rho=0p995, today's receiver (C1)", "CL-long | adr | ADR-0084 sender at rho=1, today's receiver (C1)"])


### CL-long


(B'' rows are labelled 'Bpp'.)

Builds: the new curve here is B'' (A + R5 + the net repair clamp, 14); A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). 

Labelled post-data: build A's CL-long rows (and the clamp rows of the new model there) were sized in step 2 (three-knobs-revision4.md 9.1) before this registration; they confirm nothing. What is new here is the baselines under the clamp and elig = rack.
| row | seeds | whole-run delivered | on time within 0.4s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Bpp; lifetime T=0p4, hold 0 (S5) | 24 | 0.9412 | 0.9412 | 0.00 | 0.005 | 46 | 46 | 4600 (3758) | 0 | 0 | 0 |
| Bpp; lifetime T=0p6, hold 0 (S5) | 24 | 0.9515 | 0.9057 | 0.57 | 0.005 | 37 | 37 | 4759 (3606) | 0 | 0 | 0 |
| Bpp; lifetime T=0p8, hold 0 (S5) | 24 | 0.9614 | 0.8565 | 1.54 | 0.006 | 29 | 29 | 4920 (3455) | 0 | 0 | 0 |
| Bpp; lifetime T=1, hold 0 (S5) | 24 | 0.9713 | 0.8073 | 2.53 | 0.006 | 22 | 22 | 5079 (3307) | 0 | 0 | 0 |
| Bpp; lifetime T=1p2, hold 0 (S5) | 24 | 0.9813 | 0.7583 | 3.51 | 0.006 | 14 | 14 | 5231 (3150) | 0 | 0 | 0 |
| Bpp; lifetime T=1p25, hold 0 (S5) | 24 | 0.9840 | 0.7462 | 3.75 | 0.006 | 11 | 11 | 5272 (3113) | 0 | 0 | 0 |
| Bpp; lifetime T=1p3, hold 0 (S5) | 24 | 0.9864 | 0.7339 | 4.00 | 0.006 | 9 | 9 | 5316 (3076) | 0 | 0 | 0 |
| Bpp; lifetime T=1p35, hold 0 (S5) | 24 | 0.9891 | 0.7217 | 4.28 | 0.006 | 7 | 7 | 5348 (3033) | 0 | 0 | 0 |
| Bpp; lifetime T=1p4, hold 0 (S5) | 24 | 0.9909 | 0.7099 | 4.49 | 0.007 | 5 | 5 | 5394 (3005) | 0 | 0 | 0 |
| Bpp; lifetime T=1p45, hold 0 (S5) | 24 | 0.9931 | 0.6967 | 4.74 | 0.005 | 4 | 4 | 5442 (2963) | 0 | 0 | 0 |
| Bpp; lifetime T=1p5, hold 0 (S5) | 24 | 0.9953 | 0.6849 | 4.97 | 0.005 | 2 | 2 | 5585 (2973) | 0 | 0 | 0 |
| Bpp; lifetime T=1p55, hold 0 (S5) | 24 | 0.9956 | 0.6719 | 5.25 | 0.005 | 1 | 1 | 5662 (2923) | 0 | 0 | 0 |
| Bpp; lifetime T=1p6, hold 0 (S5) | 24 | 0.9959 | 0.6613 | 5.45 | 0.004 | 1 | 1 | 5752 (2890) | 0 | 0 | 0 |
| Bpp; lifetime T=2, hold 0 (S5) | 24 | 0.9990 | 0.6368 | 5.94 | 0.002 | 0 | 0 | 6428 (2856) | 0 | 6 | 0 |
| Bpp; lifetime T=inf, hold 0 (S5) | 24 | 1.0000 | 0.6312 | 6.06 | 0.000 | 0 | 0 | 6673 (2845) | 0 | 0 | 0 |
| Bpp; lifetime T=0p4, hold inf (S5) | 12 | 0.9412 | 0.9352 | 0.00 | 0.005 | 46 | 46 | 4588 (3747) | 0 | 0 | 0 |
| Bpp; lifetime T=1p4, hold inf (S5) | 12 | 0.9908 | 0.6818 | 5.09 | 0.007 | 6 | 6 | 5379 (2985) | 0 | 0 | 0 |
| Bpp; lifetime T=inf, hold inf (S5) | 12 | 1.0000 | 0.6015 | 6.69 | 0.000 | 0 | 0 | 6661 (2824) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 12 | 0.9940 | 0.6448 | 5.67 | 0.011 | 4018 | 0 | 5292 (2474) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | 12 | 0.9940 | 0.6610 | 5.35 | 0.011 | 3339 | 0 | 5308 (2570) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | 12 | 0.9941 | 0.6788 | 4.98 | 0.011 | 1954 | 0 | 5236 (2659) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | 12 | 0.9941 | 0.6788 | 4.98 | 0.011 | 429 | 0 | 5241 (2661) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 12 | 0.9941 | 0.6788 | 4.98 | 0.011 | 0 | 0 | 5301 (2663) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p5 (E-rt) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4671 (4181) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4671 (4181) | 0 | 0 | 0 |
| rt; today's Realtime, delta=5 (E-rt) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4554 (4078) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4554 (4078) | 0 | 0 | 0 |
| rt; today's Realtime, delta=50 (E-rt) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4446 (3980) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4446 (3980) | 0 | 0 | 0 |
| bulk; never gives up, strict order = today's Bulk (C3) | 12 | 1.0000 | 0.6015 | 6.69 | 0.000 | 0 | 0 | 6661 (2824) | 0 | 0 | 0 |

Matched share. Outage cell: the axis is the whole-run delivered share and the readouts are the whole-run share on time and the registered recovery time, as in stage 3e; the steady p50 is on the paired window (outage and recovery excluded). The new curve is B''s H = 0 sweep, interpolated per seed at the baseline's share, against the baseline; a baseline below the curve's lowest share reads the lowest point. Positive favours B''. Bars: shares max(0.05 (1 - ref), 1/units, NF), p50 max(NF_p50, 0.05 ref), recovery max(0.05 ref, 0.1 s, NF).

| baseline | readout | difference [95 %] | bar | outcome |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | whole-run on time | 0.0479 [0.045, 0.0511] | 0.0178 | shown better |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery | 0.852 [0.791, 0.916] | 0.283 | shown better |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | 0.000372 [0.000358, 0.000389] | 0.000302 | shown better |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | whole-run on time | 0.0314 [0.0286, 0.0345] | 0.0169 | shown better |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery | 0.53 [0.471, 0.593] | 0.268 | shown better |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | 0.000361 [0.000345, 0.000379] | 0.000302 | shown better |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | whole-run on time | 0.0134 [0.0108, 0.0163] | 0.0161 | undetermined |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery | 0.159 [0.0969, 0.225] | 0.249 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | 0.000282 [0.000267, 0.000294] | 0.0003 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | whole-run on time | 0.0134 [0.0108, 0.0163] | 0.0161 | undetermined |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery | 0.158 [0.0966, 0.225] | 0.249 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | 0.000267 [0.000252, 0.000281] | 0.000299 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | whole-run on time | 0.0134 [0.0104, 0.0172] | 0.0161 | undetermined |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | recovery | 0.157 [0.0811, 0.249] | 0.249 | undetermined |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | 0.000267 [0.000252, 0.000281] | 0.000299 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | whole-run on time | 0.0161 [0.0157, 0.0164] | 0.00375 | shown better |
| rt; today's Realtime, delta=0p5 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | 0.000228 [0.000228, 0.000228] | 0.000292 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | whole-run on time | 0.0161 [0.0157, 0.0164] | 0.00375 | shown better |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | 0.000228 [0.000228, 0.000228] | 0.000292 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | whole-run on time | 0.0161 [0.0157, 0.0165] | 0.00375 | shown better |
| rt; today's Realtime, delta=5 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | 0.000223 [0.000214, 0.000228] | 0.000292 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | whole-run on time | 0.0161 [0.0157, 0.0165] | 0.00375 | shown better |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | 0.000223 [0.000214, 0.000228] | 0.000292 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | whole-run on time | 0.0161 [0.0157, 0.0164] | 0.00375 | shown better |
| rt; today's Realtime, delta=50 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | 0.000209 [0.000194, 0.000223] | 0.000291 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | whole-run on time | 0.0161 [0.0157, 0.0164] | 0.00375 | shown better |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | 0.000209 [0.000194, 0.000223] | 0.000291 | equivalent |
| bulk; never gives up, strict order = today's Bulk (C3) | all readouts | identity against S5 at T=inf, H=inf: identical on every key and trace at 12 seeds | - | equivalent (identity) |

Words against every baseline: build A (stored rows, re-read now with the same reader and seeds) and B'' (14.5: every word unchanged is predicted):

| baseline | readout | A | B'' | changed |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | whole-run on time | shown better | shown better |  |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery | shown better | shown better |  |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | shown better | shown better |  |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | whole-run on time | shown better | shown better |  |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery | shown better | shown better |  |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | shown better | shown better |  |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | whole-run on time | equivalent | undetermined | **changed** |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | whole-run on time | equivalent | undetermined | **changed** |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | whole-run on time | undetermined | undetermined |  |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | recovery | equivalent | undetermined | **changed** |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | equivalent | equivalent |  |
| rt; today's Realtime, delta=0p5 (E-rt) | whole-run on time | shown better | shown better |  |
| rt; today's Realtime, delta=0p5 (E-rt) | recovery | equivalent | equivalent |  |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | equivalent | equivalent |  |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | whole-run on time | shown better | shown better |  |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | recovery | equivalent | equivalent |  |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | equivalent | equivalent |  |
| rt; today's Realtime, delta=5 (E-rt) | whole-run on time | shown better | shown better |  |
| rt; today's Realtime, delta=5 (E-rt) | recovery | equivalent | equivalent |  |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | equivalent | equivalent |  |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | whole-run on time | shown better | shown better |  |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | recovery | equivalent | equivalent |  |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | equivalent | equivalent |  |
| rt; today's Realtime, delta=50 (E-rt) | whole-run on time | shown better | shown better |  |
| rt; today's Realtime, delta=50 (E-rt) | recovery | equivalent | equivalent |  |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | equivalent | equivalent |  |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | whole-run on time | shown better | shown better |  |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | recovery | equivalent | equivalent |  |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | equivalent | equivalent |  |
| bulk; never gives up, strict order = today's Bulk (C3) | all readouts | equivalent (identity) | equivalent (identity) |  |

B'' against the stored A at each row (id3e keys, seeds 0-11); net clamp gauges:

| row | seeds identical | whole-run delivered A / B'' | whole-run on time A / B'' | recovery A / B'' | net_bind (mean) | credits removed (mean) | repairs A / B'' |
|---|---|---|---|---|---|---|---|
| Bpp; lifetime T=0p4, hold 0 (S5) | 0 / 12 | 0.9396 / 0.9412 | 0.9396 / 0.9412 | 0.00 / 0.00 | 9396 | 441.6 | 19159 / 18837 |
| Bpp; lifetime T=0p6, hold 0 (S5) | 0 / 12 | 0.9500 / 0.9515 | 0.9076 / 0.9058 | 0.52 / 0.59 | 9382 | 438.3 | 18156 / 17836 |
| Bpp; lifetime T=0p8, hold 0 (S5) | 0 / 12 | 0.9601 / 0.9616 | 0.8586 / 0.8568 | 1.51 / 1.54 | 9374 | 435.1 | 17147 / 16827 |
| Bpp; lifetime T=1, hold 0 (S5) | 0 / 12 | 0.9697 / 0.9712 | 0.8092 / 0.8074 | 2.50 / 2.53 | 9372 | 431.8 | 16130 / 15816 |
| Bpp; lifetime T=1p2, hold 0 (S5) | 0 / 12 | 0.9799 / 0.9814 | 0.7601 / 0.7583 | 3.46 / 3.49 | 9361 | 428.7 | 15118 / 14809 |
| Bpp; lifetime T=1p25, hold 0 (S5) | 0 / 12 | 0.9826 / 0.9841 | 0.7480 / 0.7463 | 3.71 / 3.75 | 9353 | 427.6 | 14868 / 14561 |
| Bpp; lifetime T=1p3, hold 0 (S5) | 0 / 12 | 0.9851 / 0.9866 | 0.7357 / 0.7341 | 3.97 / 3.99 | 9355 | 427.0 | 14616 / 14307 |
| Bpp; lifetime T=1p35, hold 0 (S5) | 0 / 12 | 0.9876 / 0.9891 | 0.7235 / 0.7218 | 4.22 / 4.28 | 9348 | 426.1 | 14362 / 14059 |
| Bpp; lifetime T=1p4, hold 0 (S5) | 0 / 12 | 0.9897 / 0.9908 | 0.7107 / 0.7098 | 4.46 / 4.50 | 8984 | 333.8 | 14097 / 13873 |
| Bpp; lifetime T=1p45, hold 0 (S5) | 0 / 12 | 0.9931 / 0.9932 | 0.6953 / 0.6967 | 4.78 / 4.74 | 7808 | 154.5 | 13773 / 13713 |
| Bpp; lifetime T=1p5, hold 0 (S5) | 12 / 12 | 0.9953 / 0.9953 | 0.6851 / 0.6851 | 4.96 / 4.96 | 0 | 0.0 | 13524 / 13523 |
| Bpp; lifetime T=1p55, hold 0 (S5) | 12 / 12 | 0.9956 / 0.9956 | 0.6718 / 0.6718 | 5.24 / 5.24 | 0 | 0.0 | 13291 / 13295 |
| Bpp; lifetime T=1p6, hold 0 (S5) | 12 / 12 | 0.9959 / 0.9959 | 0.6609 / 0.6609 | 5.47 / 5.47 | 0 | 0.0 | 13076 / 13089 |
| Bpp; lifetime T=2, hold 0 (S5) | 12 / 12 | 0.9989 / 0.9989 | 0.6358 / 0.6358 | 5.96 / 5.96 | 0 | 0.0 | 12852 / 12874 |
| Bpp; lifetime T=inf, hold 0 (S5) | 12 / 12 | 1.0000 / 1.0000 | 0.6300 / 0.6300 | 6.08 / 6.08 | 0 | 0.0 | 12729 / 12752 |
| Bpp; lifetime T=0p4, hold inf (S5) | 0 / 12 | 0.9396 / 0.9412 | 0.9365 / 0.9352 | 0.00 / 0.00 | 9399 | 441.6 | 19159 / 18840 |
| Bpp; lifetime T=1p4, hold inf (S5) | 0 / 12 | 0.9897 / 0.9908 | 0.6821 / 0.6818 | 5.09 / 5.09 | 9015 | 342.1 | 14097 / 13862 |
| Bpp; lifetime T=inf, hold inf (S5) | 12 / 12 | 1.0000 / 1.0000 | 0.6015 / 0.6015 | 6.69 / 6.69 | 0 | 0.0 | 12729 / 12729 |

(CL-long: 396 new runs, 9131 s wall at 5 workers)



### CL


(B'' rows are labelled 'Bpp'.)

Builds: the new curve here is B'' (A + R5 + the net repair clamp, 14); A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). 

| row | seeds | whole-run delivered | on time within 0.4s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Bpp; lifetime T=0p4, hold 0 (S5) | 12 | 0.8824 | 0.8824 | 0.00 | 0.012 | 46 | 46 | 2180 (1579) | 0 | 0 | 0 |
| Bpp; lifetime T=0p6, hold 0 (S5) | 12 | 0.9031 | 0.8117 | 0.59 | 0.013 | 37 | 37 | 2339 (1428) | 0 | 0 | 0 |
| Bpp; lifetime T=0p8, hold 0 (S5) | 12 | 0.9233 | 0.7136 | 1.54 | 0.015 | 29 | 29 | 2501 (1279) | 0 | 0 | 0 |
| Bpp; lifetime T=1, hold 0 (S5) | 12 | 0.9425 | 0.6147 | 2.53 | 0.017 | 22 | 22 | 2668 (1141) | 0 | 0 | 0 |
| Bpp; lifetime T=1p2, hold 0 (S5) | 12 | 0.9628 | 0.5166 | 3.49 | 0.019 | 14 | 14 | 2818 (980) | 0 | 0 | 0 |
| Bpp; lifetime T=1p25, hold 0 (S5) | 12 | 0.9682 | 0.4926 | 3.75 | 0.020 | 11 | 11 | 2859 (944) | 0 | 0 | 0 |
| Bpp; lifetime T=1p3, hold 0 (S5) | 12 | 0.9732 | 0.4682 | 12.73 | 0.021 | 9 | 9 | 2900 (905) | 0 | 0 | 0 |
| Bpp; lifetime T=1p35, hold 0 (S5) | 12 | 0.9782 | 0.4436 | 14.50 | 0.022 | 7 | 7 | 2937 (863) | 0 | 0 | 0 |
| Bpp; lifetime T=1p4, hold 0 (S5) | 12 | 0.9817 | 0.4196 | 14.50 | 0.024 | 6 | 6 | 2976 (825) | 0 | 0 | 0 |
| Bpp; lifetime T=1p45, hold 0 (S5) | 12 | 0.9863 | 0.3935 | 14.50 | 0.016 | 3 | 3 | 3027 (784) | 0 | 0 | 0 |
| Bpp; lifetime T=1p5, hold 0 (S5) | 12 | 0.9907 | 0.3701 | 14.50 | 0.017 | 2 | 2 | 3145 (824) | 0 | 0 | 0 |
| Bpp; lifetime T=1p55, hold 0 (S5) | 12 | 0.9912 | 0.3437 | 14.50 | 0.017 | 1 | 1 | 3192 (822) | 0 | 0 | 0 |
| Bpp; lifetime T=1p6, hold 0 (S5) | 12 | 0.9918 | 0.3218 | 14.50 | 0.015 | 1 | 1 | 3265 (825) | 0 | 0 | 0 |
| Bpp; lifetime T=2, hold 0 (S5) | 12 | 0.9979 | 0.2718 | 14.50 | 0.005 | 0 | 0 | 3898 (870) | 0 | 13 | 0 |
| Bpp; lifetime T=inf, hold 0 (S5) | 12 | 1.0000 | 0.2600 | 14.50 | 0.001 | 0 | 0 | 4146 (882) | 0 | 0 | 0 |
| Bpp; lifetime T=0p4, hold inf (S5) | 12 | 0.8824 | 0.8705 | 0.00 | 0.012 | 46 | 46 | 2180 (1579) | 0 | 0 | 0 |
| Bpp; lifetime T=1p4, hold inf (S5) | 12 | 0.9817 | 0.3636 | 14.50 | 0.024 | 6 | 6 | 2976 (825) | 0 | 0 | 0 |
| Bpp; lifetime T=inf, hold inf (S5) | 12 | 1.0000 | 0.2151 | 14.50 | 0.001 | 0 | 0 | 4146 (882) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 12 | 0.9882 | 0.2898 | 14.50 | 0.034 | 2123 | 0 | 2833 (501) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | 12 | 0.9882 | 0.3222 | 14.50 | 0.035 | 1454 | 0 | 2870 (533) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | 12 | 0.9882 | 0.3576 | 14.50 | 0.035 | 430 | 0 | 2834 (513) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | 12 | 0.9882 | 0.3576 | 14.50 | 0.035 | 429 | 0 | 2836 (512) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 12 | 0.9882 | 0.3576 | 14.50 | 0.035 | 0 | 0 | 2893 (513) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p5 (E-rt) | 12 | 0.8501 | 0.8501 | 0.00 | 0.001 | 0 | 0 | 2148 (1913) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | 12 | 0.8501 | 0.8501 | 0.00 | 0.001 | 0 | 0 | 2148 (1913) | 0 | 0 | 0 |
| rt; today's Realtime, delta=5 (E-rt) | 12 | 0.8501 | 0.8501 | 0.00 | 0.001 | 0 | 0 | 2096 (1866) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | 12 | 0.8501 | 0.8501 | 0.00 | 0.001 | 0 | 0 | 2096 (1866) | 0 | 0 | 0 |
| rt; today's Realtime, delta=50 (E-rt) | 12 | 0.8501 | 0.8501 | 0.00 | 0.001 | 0 | 0 | 2048 (1823) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 12 | 0.8501 | 0.8501 | 0.00 | 0.001 | 0 | 0 | 2048 (1823) | 0 | 0 | 0 |
| bulk; never gives up, strict order = today's Bulk (C3) | 12 | 1.0000 | 0.2151 | 14.50 | 0.001 | 0 | 0 | 4146 (882) | 0 | 0 | 0 |

Matched share. Outage cell: the axis is the whole-run delivered share and the readouts are the whole-run share on time and the registered recovery time, as in stage 3e; the steady p50 is on the paired window (outage and recovery excluded). The new curve is B''s H = 0 sweep, interpolated per seed at the baseline's share, against the baseline; a baseline below the curve's lowest share reads the lowest point. Positive favours B''. Bars: shares max(0.05 (1 - ref), 1/units, NF), p50 max(NF_p50, 0.05 ref), recovery max(0.05 ref, 0.1 s, NF).

| baseline | readout | difference [95 %] | bar | outcome |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | whole-run on time | 0.0945 [0.0888, 0.101] | 0.0355 | shown better |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery | 0 [0, 0] | 0.725 | equivalent |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | whole-run on time | 0.062 [0.0563, 0.068] | 0.0339 | shown better |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery | 0 [0, 0] | 0.725 | equivalent |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | whole-run on time | 0.0267 [0.0215, 0.0325] | 0.0321 | undetermined |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery | 0 [0, 0] | 0.725 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | whole-run on time | 0.0266 [0.0214, 0.0323] | 0.0321 | undetermined |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery | 0 [0, 0] | 0.725 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | whole-run on time | 0.0266 [0.0208, 0.0342] | 0.0321 | undetermined |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | recovery | 0 [0, 0] | 0.725 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| rt; today's Realtime, delta=0p5 (E-rt) | whole-run on time | 0.0322 [0.0314, 0.0329] | 0.00749 | shown better |
| rt; today's Realtime, delta=0p5 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | whole-run on time | 0.0322 [0.0314, 0.0329] | 0.00749 | shown better |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| rt; today's Realtime, delta=5 (E-rt) | whole-run on time | 0.0322 [0.0314, 0.0329] | 0.00749 | shown better |
| rt; today's Realtime, delta=5 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | whole-run on time | 0.0322 [0.0314, 0.0329] | 0.00749 | shown better |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| rt; today's Realtime, delta=50 (E-rt) | whole-run on time | 0.0322 [0.0314, 0.0329] | 0.00749 | shown better |
| rt; today's Realtime, delta=50 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | whole-run on time | 0.0322 [0.0314, 0.0329] | 0.00749 | shown better |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| bulk; never gives up, strict order = today's Bulk (C3) | all readouts | identity against S5 at T=inf, H=inf: identical on every key and trace at 12 seeds | - | equivalent (identity) |

Words against every baseline: build A (stored rows, re-read now with the same reader and seeds) and B'' (14.5: every word unchanged is predicted):

| baseline | readout | A | B'' | changed |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | whole-run on time | shown better | shown better |  |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | whole-run on time | shown better | shown better |  |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | whole-run on time | equivalent | undetermined | **changed** |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | whole-run on time | equivalent | undetermined | **changed** |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | whole-run on time | undetermined | undetermined |  |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| rt; today's Realtime, delta=0p5 (E-rt) | whole-run on time | shown better | shown better |  |
| rt; today's Realtime, delta=0p5 (E-rt) | recovery | equivalent | equivalent |  |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | whole-run on time | shown better | shown better |  |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | recovery | equivalent | equivalent |  |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| rt; today's Realtime, delta=5 (E-rt) | whole-run on time | shown better | shown better |  |
| rt; today's Realtime, delta=5 (E-rt) | recovery | equivalent | equivalent |  |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | whole-run on time | shown better | shown better |  |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | recovery | equivalent | equivalent |  |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| rt; today's Realtime, delta=50 (E-rt) | whole-run on time | shown better | shown better |  |
| rt; today's Realtime, delta=50 (E-rt) | recovery | equivalent | equivalent |  |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | whole-run on time | shown better | shown better |  |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | recovery | equivalent | equivalent |  |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| bulk; never gives up, strict order = today's Bulk (C3) | all readouts | equivalent (identity) | equivalent (identity) |  |

B'' against the stored A at each row (id3e keys, seeds 0-11); net clamp gauges:

| row | seeds identical | whole-run delivered A / B'' | whole-run on time A / B'' | recovery A / B'' | net_bind (mean) | credits removed (mean) | repairs A / B'' |
|---|---|---|---|---|---|---|---|
| Bpp; lifetime T=0p4, hold 0 (S5) | 0 / 12 | 0.8792 / 0.8824 | 0.8792 / 0.8824 | 0.00 / 0.00 | 9399 | 441.6 | 8177 / 7857 |
| Bpp; lifetime T=0p6, hold 0 (S5) | 0 / 12 | 0.8999 / 0.9031 | 0.8153 / 0.8117 | 0.52 / 0.59 | 9389 | 438.7 | 7174 / 6858 |
| Bpp; lifetime T=0p8, hold 0 (S5) | 0 / 12 | 0.9202 / 0.9233 | 0.7172 / 0.7136 | 1.51 / 1.54 | 9382 | 435.6 | 6164 / 5852 |
| Bpp; lifetime T=1, hold 0 (S5) | 0 / 12 | 0.9394 / 0.9425 | 0.6184 / 0.6147 | 2.50 / 2.53 | 9378 | 432.2 | 5148 / 4836 |
| Bpp; lifetime T=1p2, hold 0 (S5) | 0 / 12 | 0.9597 / 0.9628 | 0.5202 / 0.5166 | 3.46 / 3.49 | 9367 | 429.4 | 4135 / 3829 |
| Bpp; lifetime T=1p25, hold 0 (S5) | 0 / 12 | 0.9652 / 0.9682 | 0.4959 / 0.4926 | 3.71 / 3.75 | 9359 | 428.2 | 3886 / 3582 |
| Bpp; lifetime T=1p3, hold 0 (S5) | 0 / 12 | 0.9702 / 0.9732 | 0.4714 / 0.4682 | 11.84 / 12.73 | 9357 | 427.8 | 3633 / 3329 |
| Bpp; lifetime T=1p35, hold 0 (S5) | 0 / 12 | 0.9752 / 0.9782 | 0.4470 / 0.4436 | 14.50 / 14.50 | 9351 | 426.8 | 3395 / 3099 |
| Bpp; lifetime T=1p4, hold 0 (S5) | 0 / 12 | 0.9795 / 0.9817 | 0.4215 / 0.4196 | 14.50 / 14.50 | 9015 | 342.1 | 3187 / 2957 |
| Bpp; lifetime T=1p45, hold 0 (S5) | 0 / 12 | 0.9863 / 0.9863 | 0.3905 / 0.3935 | 14.50 / 14.50 | 7758 | 151.8 | 2958 / 2876 |
| Bpp; lifetime T=1p5, hold 0 (S5) | 12 / 12 | 0.9907 / 0.9907 | 0.3701 / 0.3701 | 14.50 / 14.50 | 0 | 0.0 | 2921 / 2921 |
| Bpp; lifetime T=1p55, hold 0 (S5) | 12 / 12 | 0.9912 / 0.9912 | 0.3437 / 0.3437 | 14.50 / 14.50 | 0 | 0.0 | 2966 / 2966 |
| Bpp; lifetime T=1p6, hold 0 (S5) | 12 / 12 | 0.9918 / 0.9918 | 0.3218 / 0.3218 | 14.50 / 14.50 | 0 | 0.0 | 2978 / 2978 |
| Bpp; lifetime T=2, hold 0 (S5) | 12 / 12 | 0.9979 / 0.9979 | 0.2718 / 0.2718 | 14.50 / 14.50 | 0 | 0.0 | 3273 / 3273 |
| Bpp; lifetime T=inf, hold 0 (S5) | 12 / 12 | 1.0000 / 1.0000 | 0.2600 / 0.2600 | 14.50 / 14.50 | 0 | 0.0 | 3271 / 3271 |
| Bpp; lifetime T=0p4, hold inf (S5) | 0 / 12 | 0.8792 / 0.8824 | 0.8730 / 0.8705 | 0.00 / 0.00 | 9399 | 441.6 | 8177 / 7857 |
| Bpp; lifetime T=1p4, hold inf (S5) | 0 / 12 | 0.9795 / 0.9817 | 0.3642 / 0.3636 | 14.50 / 14.50 | 9015 | 342.1 | 3187 / 2957 |
| Bpp; lifetime T=inf, hold inf (S5) | 12 / 12 | 1.0000 / 1.0000 | 0.2151 / 0.2151 | 14.50 / 14.50 | 0 | 0.0 | 3271 / 3271 |

### CL after the registered doubling (arms: ['CL | Bpp | lifetime T=0p4, hold 0 (S5)', 'CL | Bpp | lifetime T=0p6, hold 0 (S5)', 'CL | Bpp | lifetime T=0p8, hold 0 (S5)', 'CL | Bpp | lifetime T=1, hold 0 (S5)', 'CL | Bpp | lifetime T=1p2, hold 0 (S5)', 'CL | Bpp | lifetime T=1p25, hold 0 (S5)', 'CL | Bpp | lifetime T=1p3, hold 0 (S5)', 'CL | Bpp | lifetime T=1p35, hold 0 (S5)', 'CL | Bpp | lifetime T=1p4, hold 0 (S5)', 'CL | Bpp | lifetime T=1p45, hold 0 (S5)', 'CL | Bpp | lifetime T=1p5, hold 0 (S5)', 'CL | Bpp | lifetime T=1p55, hold 0 (S5)', 'CL | Bpp | lifetime T=1p6, hold 0 (S5)', 'CL | Bpp | lifetime T=2, hold 0 (S5)', 'CL | Bpp | lifetime T=inf, hold 0 (S5)', "CL | adr | ADR-0084 sender at rho=0p99, today's receiver (C1)", "CL | adr | ADR-0084 sender at rho=0p995, today's receiver (C1)", "CL | adr | ADR-0084 sender at rho=1, today's receiver (C1)"])


### CL


(B'' rows are labelled 'Bpp'.)

Builds: the new curve here is B'' (A + R5 + the net repair clamp, 14); A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). 

| row | seeds | whole-run delivered | on time within 0.4s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Bpp; lifetime T=0p4, hold 0 (S5) | 24 | 0.8823 | 0.8823 | 0.00 | 0.012 | 46 | 46 | 2193 (1591) | 0 | 0 | 0 |
| Bpp; lifetime T=0p6, hold 0 (S5) | 24 | 0.9030 | 0.8115 | 0.57 | 0.013 | 37 | 37 | 2347 (1433) | 0 | 0 | 0 |
| Bpp; lifetime T=0p8, hold 0 (S5) | 24 | 0.9228 | 0.7130 | 1.54 | 0.015 | 29 | 29 | 2506 (1281) | 0 | 0 | 0 |
| Bpp; lifetime T=1, hold 0 (S5) | 24 | 0.9425 | 0.6146 | 2.53 | 0.017 | 22 | 22 | 2670 (1139) | 0 | 0 | 0 |
| Bpp; lifetime T=1p2, hold 0 (S5) | 24 | 0.9627 | 0.5165 | 3.51 | 0.019 | 14 | 14 | 2823 (981) | 0 | 0 | 0 |
| Bpp; lifetime T=1p25, hold 0 (S5) | 24 | 0.9680 | 0.4924 | 3.75 | 0.020 | 11 | 11 | 2865 (946) | 0 | 0 | 0 |
| Bpp; lifetime T=1p3, hold 0 (S5) | 24 | 0.9729 | 0.4677 | 13.62 | 0.021 | 9 | 9 | 2905 (907) | 0 | 0 | 0 |
| Bpp; lifetime T=1p35, hold 0 (S5) | 24 | 0.9782 | 0.4434 | 14.50 | 0.022 | 7 | 7 | 2942 (869) | 0 | 0 | 0 |
| Bpp; lifetime T=1p4, hold 0 (S5) | 24 | 0.9819 | 0.4199 | 14.50 | 0.025 | 5 | 5 | 2981 (833) | 0 | 0 | 0 |
| Bpp; lifetime T=1p45, hold 0 (S5) | 24 | 0.9861 | 0.3935 | 14.50 | 0.017 | 4 | 4 | 3030 (790) | 0 | 0 | 0 |
| Bpp; lifetime T=1p5, hold 0 (S5) | 24 | 0.9907 | 0.3699 | 14.50 | 0.017 | 2 | 2 | 3152 (831) | 0 | 0 | 0 |
| Bpp; lifetime T=1p55, hold 0 (S5) | 24 | 0.9912 | 0.3438 | 14.50 | 0.016 | 1 | 1 | 3205 (828) | 0 | 0 | 0 |
| Bpp; lifetime T=1p6, hold 0 (S5) | 24 | 0.9919 | 0.3226 | 14.50 | 0.015 | 1 | 1 | 3278 (831) | 0 | 0 | 0 |
| Bpp; lifetime T=2, hold 0 (S5) | 24 | 0.9979 | 0.2735 | 14.50 | 0.005 | 0 | 0 | 3912 (878) | 0 | 6 | 0 |
| Bpp; lifetime T=inf, hold 0 (S5) | 24 | 1.0000 | 0.2625 | 14.50 | 0.001 | 0 | 0 | 4153 (889) | 0 | 0 | 0 |
| Bpp; lifetime T=0p4, hold inf (S5) | 12 | 0.8824 | 0.8705 | 0.00 | 0.012 | 46 | 46 | 2180 (1579) | 0 | 0 | 0 |
| Bpp; lifetime T=1p4, hold inf (S5) | 12 | 0.9817 | 0.3636 | 14.50 | 0.024 | 6 | 6 | 2976 (825) | 0 | 0 | 0 |
| Bpp; lifetime T=inf, hold inf (S5) | 12 | 1.0000 | 0.2151 | 14.50 | 0.001 | 0 | 0 | 4146 (882) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 12 | 0.9882 | 0.2898 | 14.50 | 0.034 | 2123 | 0 | 2833 (501) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | 12 | 0.9882 | 0.3222 | 14.50 | 0.035 | 1454 | 0 | 2870 (533) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | 12 | 0.9882 | 0.3576 | 14.50 | 0.035 | 430 | 0 | 2834 (513) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | 12 | 0.9882 | 0.3576 | 14.50 | 0.035 | 429 | 0 | 2836 (512) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 12 | 0.9882 | 0.3576 | 14.50 | 0.035 | 0 | 0 | 2893 (513) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p5 (E-rt) | 12 | 0.8501 | 0.8501 | 0.00 | 0.001 | 0 | 0 | 2148 (1913) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | 12 | 0.8501 | 0.8501 | 0.00 | 0.001 | 0 | 0 | 2148 (1913) | 0 | 0 | 0 |
| rt; today's Realtime, delta=5 (E-rt) | 12 | 0.8501 | 0.8501 | 0.00 | 0.001 | 0 | 0 | 2096 (1866) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | 12 | 0.8501 | 0.8501 | 0.00 | 0.001 | 0 | 0 | 2096 (1866) | 0 | 0 | 0 |
| rt; today's Realtime, delta=50 (E-rt) | 12 | 0.8501 | 0.8501 | 0.00 | 0.001 | 0 | 0 | 2048 (1823) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 12 | 0.8501 | 0.8501 | 0.00 | 0.001 | 0 | 0 | 2048 (1823) | 0 | 0 | 0 |
| bulk; never gives up, strict order = today's Bulk (C3) | 12 | 1.0000 | 0.2151 | 14.50 | 0.001 | 0 | 0 | 4146 (882) | 0 | 0 | 0 |

Matched share. Outage cell: the axis is the whole-run delivered share and the readouts are the whole-run share on time and the registered recovery time, as in stage 3e; the steady p50 is on the paired window (outage and recovery excluded). The new curve is B''s H = 0 sweep, interpolated per seed at the baseline's share, against the baseline; a baseline below the curve's lowest share reads the lowest point. Positive favours B''. Bars: shares max(0.05 (1 - ref), 1/units, NF), p50 max(NF_p50, 0.05 ref), recovery max(0.05 ref, 0.1 s, NF).

| baseline | readout | difference [95 %] | bar | outcome |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | whole-run on time | 0.0945 [0.0888, 0.101] | 0.0355 | shown better |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery | 0 [0, 0] | 0.725 | equivalent |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | whole-run on time | 0.062 [0.0563, 0.068] | 0.0339 | shown better |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery | 0 [0, 0] | 0.725 | equivalent |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | whole-run on time | 0.0267 [0.0215, 0.0325] | 0.0321 | undetermined |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery | 0 [0, 0] | 0.725 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | whole-run on time | 0.0266 [0.0214, 0.0323] | 0.0321 | undetermined |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery | 0 [0, 0] | 0.725 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | whole-run on time | 0.0266 [0.0208, 0.0342] | 0.0321 | undetermined |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | recovery | 0 [0, 0] | 0.725 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| rt; today's Realtime, delta=0p5 (E-rt) | whole-run on time | 0.0322 [0.0314, 0.0329] | 0.00749 | shown better |
| rt; today's Realtime, delta=0p5 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | whole-run on time | 0.0322 [0.0314, 0.0329] | 0.00749 | shown better |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| rt; today's Realtime, delta=5 (E-rt) | whole-run on time | 0.0322 [0.0314, 0.0329] | 0.00749 | shown better |
| rt; today's Realtime, delta=5 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | whole-run on time | 0.0322 [0.0314, 0.0329] | 0.00749 | shown better |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| rt; today's Realtime, delta=50 (E-rt) | whole-run on time | 0.0322 [0.0314, 0.0329] | 0.00749 | shown better |
| rt; today's Realtime, delta=50 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | whole-run on time | 0.0322 [0.0314, 0.0329] | 0.00749 | shown better |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | no steady window: the paired window of 12.2 is empty at 12 of 12 seeds for at least one build-A point (an arm has not recovered within the run, so the outage exclusion of 12.2 runs to the end) | - | not read (registration finding) |
| bulk; never gives up, strict order = today's Bulk (C3) | all readouts | identity against S5 at T=inf, H=inf: identical on every key and trace at 12 seeds | - | equivalent (identity) |

Words against every baseline: build A (stored rows, re-read now with the same reader and seeds) and B'' (14.5: every word unchanged is predicted):

| baseline | readout | A | B'' | changed |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | whole-run on time | shown better | shown better |  |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | whole-run on time | shown better | shown better |  |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | whole-run on time | equivalent | undetermined | **changed** |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | whole-run on time | equivalent | undetermined | **changed** |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | whole-run on time | undetermined | undetermined |  |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| rt; today's Realtime, delta=0p5 (E-rt) | whole-run on time | shown better | shown better |  |
| rt; today's Realtime, delta=0p5 (E-rt) | recovery | equivalent | equivalent |  |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | whole-run on time | shown better | shown better |  |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | recovery | equivalent | equivalent |  |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| rt; today's Realtime, delta=5 (E-rt) | whole-run on time | shown better | shown better |  |
| rt; today's Realtime, delta=5 (E-rt) | recovery | equivalent | equivalent |  |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | whole-run on time | shown better | shown better |  |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | recovery | equivalent | equivalent |  |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| rt; today's Realtime, delta=50 (E-rt) | whole-run on time | shown better | shown better |  |
| rt; today's Realtime, delta=50 (E-rt) | recovery | equivalent | equivalent |  |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | whole-run on time | shown better | shown better |  |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | recovery | equivalent | equivalent |  |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | not read (registration finding) | not read (registration finding) |  |
| bulk; never gives up, strict order = today's Bulk (C3) | all readouts | equivalent (identity) | equivalent (identity) |  |

B'' against the stored A at each row (id3e keys, seeds 0-11); net clamp gauges:

| row | seeds identical | whole-run delivered A / B'' | whole-run on time A / B'' | recovery A / B'' | net_bind (mean) | credits removed (mean) | repairs A / B'' |
|---|---|---|---|---|---|---|---|
| Bpp; lifetime T=0p4, hold 0 (S5) | 0 / 12 | 0.8792 / 0.8824 | 0.8792 / 0.8824 | 0.00 / 0.00 | 9396 | 441.6 | 8177 / 7856 |
| Bpp; lifetime T=0p6, hold 0 (S5) | 0 / 12 | 0.8999 / 0.9031 | 0.8153 / 0.8117 | 0.52 / 0.59 | 9382 | 438.3 | 7174 / 6855 |
| Bpp; lifetime T=0p8, hold 0 (S5) | 0 / 12 | 0.9202 / 0.9233 | 0.7172 / 0.7136 | 1.51 / 1.54 | 9374 | 435.1 | 6164 / 5846 |
| Bpp; lifetime T=1, hold 0 (S5) | 0 / 12 | 0.9394 / 0.9425 | 0.6184 / 0.6147 | 2.50 / 2.53 | 9372 | 431.8 | 5148 / 4835 |
| Bpp; lifetime T=1p2, hold 0 (S5) | 0 / 12 | 0.9597 / 0.9628 | 0.5202 / 0.5166 | 3.46 / 3.49 | 9361 | 428.7 | 4135 / 3828 |
| Bpp; lifetime T=1p25, hold 0 (S5) | 0 / 12 | 0.9652 / 0.9682 | 0.4959 / 0.4926 | 3.71 / 3.75 | 9353 | 427.6 | 3886 / 3580 |
| Bpp; lifetime T=1p3, hold 0 (S5) | 0 / 12 | 0.9702 / 0.9732 | 0.4714 / 0.4682 | 11.84 / 12.73 | 9355 | 427.0 | 3633 / 3326 |
| Bpp; lifetime T=1p35, hold 0 (S5) | 0 / 12 | 0.9752 / 0.9782 | 0.4470 / 0.4436 | 14.50 / 14.50 | 9348 | 426.1 | 3395 / 3100 |
| Bpp; lifetime T=1p4, hold 0 (S5) | 0 / 12 | 0.9795 / 0.9817 | 0.4215 / 0.4196 | 14.50 / 14.50 | 8984 | 333.8 | 3187 / 2968 |
| Bpp; lifetime T=1p45, hold 0 (S5) | 0 / 12 | 0.9863 / 0.9863 | 0.3905 / 0.3935 | 14.50 / 14.50 | 7808 | 154.5 | 2958 / 2877 |
| Bpp; lifetime T=1p5, hold 0 (S5) | 12 / 12 | 0.9907 / 0.9907 | 0.3701 / 0.3701 | 14.50 / 14.50 | 0 | 0.0 | 2921 / 2921 |
| Bpp; lifetime T=1p55, hold 0 (S5) | 12 / 12 | 0.9912 / 0.9912 | 0.3437 / 0.3437 | 14.50 / 14.50 | 0 | 0.0 | 2966 / 2967 |
| Bpp; lifetime T=1p6, hold 0 (S5) | 12 / 12 | 0.9918 / 0.9918 | 0.3218 / 0.3218 | 14.50 / 14.50 | 0 | 0.0 | 2978 / 2980 |
| Bpp; lifetime T=2, hold 0 (S5) | 12 / 12 | 0.9979 / 0.9979 | 0.2718 / 0.2718 | 14.50 / 14.50 | 0 | 0.0 | 3273 / 3276 |
| Bpp; lifetime T=inf, hold 0 (S5) | 12 / 12 | 1.0000 / 1.0000 | 0.2600 / 0.2600 | 14.50 / 14.50 | 0 | 0.0 | 3271 / 3270 |
| Bpp; lifetime T=0p4, hold inf (S5) | 0 / 12 | 0.8792 / 0.8824 | 0.8730 / 0.8705 | 0.00 / 0.00 | 9399 | 441.6 | 8177 / 7857 |
| Bpp; lifetime T=1p4, hold inf (S5) | 0 / 12 | 0.9795 / 0.9817 | 0.3642 / 0.3636 | 14.50 / 14.50 | 9015 | 342.1 | 3187 / 2957 |
| Bpp; lifetime T=inf, hold inf (S5) | 12 / 12 | 1.0000 / 1.0000 | 0.2151 / 0.2151 | 14.50 / 14.50 | 0 | 0.0 | 3271 / 3271 |

(CL: 396 new runs, 6193 s wall at 4 workers)



### CLw-long


(B'' rows are labelled 'Bpp'.)

Builds: the new curve here is B'' (A + R5 + the net repair clamp, 14); A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). 

| row | seeds | whole-run delivered | on time within 0.4s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Bpp; lifetime T=0p4, hold 0 (S5) | 12 | 0.8678 | 0.8678 | 0.00 | 0.008 | 106 | 106 | 3432 (2898) | 0 | 0 | 0 |
| Bpp; lifetime T=0p6, hold 0 (S5) | 12 | 0.8781 | 0.8510 | 0.00 | 0.009 | 97 | 97 | 3512 (2862) | 0 | 0 | 0 |
| Bpp; lifetime T=0p8, hold 0 (S5) | 12 | 0.8882 | 0.8317 | 0.40 | 0.009 | 89 | 89 | 3602 (2818) | 0 | 0 | 0 |
| Bpp; lifetime T=1, hold 0 (S5) | 12 | 0.8979 | 0.8114 | 0.80 | 0.009 | 81 | 81 | 3693 (2774) | 0 | 0 | 0 |
| Bpp; lifetime T=1p2, hold 0 (S5) | 12 | 0.9082 | 0.7919 | 1.20 | 0.009 | 73 | 73 | 3774 (2727) | 0 | 0 | 0 |
| Bpp; lifetime T=1p25, hold 0 (S5) | 12 | 0.9105 | 0.7869 | 1.30 | 0.009 | 71 | 71 | 3799 (2719) | 0 | 0 | 0 |
| Bpp; lifetime T=1p3, hold 0 (S5) | 12 | 0.9132 | 0.7820 | 1.40 | 0.009 | 69 | 69 | 3815 (2705) | 0 | 0 | 0 |
| Bpp; lifetime T=1p35, hold 0 (S5) | 12 | 0.9157 | 0.7770 | 1.50 | 0.009 | 67 | 67 | 3848 (2700) | 0 | 0 | 0 |
| Bpp; lifetime T=1p4, hold 0 (S5) | 12 | 0.9180 | 0.7719 | 1.60 | 0.009 | 65 | 65 | 3871 (2688) | 0 | 0 | 0 |
| Bpp; lifetime T=1p45, hold 0 (S5) | 12 | 0.9204 | 0.7669 | 1.70 | 0.009 | 63 | 63 | 3884 (2670) | 0 | 0 | 0 |
| Bpp; lifetime T=1p5, hold 0 (S5) | 12 | 0.9230 | 0.7621 | 1.80 | 0.009 | 61 | 61 | 3911 (2666) | 0 | 0 | 0 |
| Bpp; lifetime T=1p55, hold 0 (S5) | 12 | 0.9251 | 0.7567 | 1.90 | 0.009 | 59 | 59 | 3925 (2653) | 0 | 0 | 0 |
| Bpp; lifetime T=1p6, hold 0 (S5) | 12 | 0.9277 | 0.7519 | 2.00 | 0.009 | 57 | 57 | 3941 (2634) | 0 | 0 | 0 |
| Bpp; lifetime T=2, hold 0 (S5) | 12 | 0.9480 | 0.7126 | 2.80 | 0.010 | 41 | 41 | 4125 (2548) | 0 | 0 | 0 |
| Bpp; lifetime T=inf, hold 0 (S5) | 12 | 1.0000 | 0.5926 | 5.20 | 0.004 | 0 | 0 | 5661 (2370) | 0 | 0 | 0 |
| Bpp; lifetime T=0p4, hold inf (S5) | 12 | 0.8678 | 0.8612 | 0.00 | 0.008 | 106 | 106 | 3432 (2898) | 0 | 0 | 0 |
| Bpp; lifetime T=1p4, hold inf (S5) | 12 | 0.9180 | 0.7586 | 1.93 | 0.009 | 65 | 65 | 3871 (2688) | 0 | 0 | 0 |
| Bpp; lifetime T=inf, hold inf (S5) | 12 | 1.0000 | 0.5779 | 5.51 | 0.004 | 0 | 0 | 5661 (2370) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 12 | 0.9928 | 0.6002 | 5.02 | 0.015 | 3385 | 0 | 4305 (2080) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | 12 | 0.9928 | 0.6090 | 4.86 | 0.015 | 2645 | 0 | 4313 (2122) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | 12 | 0.9928 | 0.6157 | 4.72 | 0.015 | 1357 | 0 | 4266 (2121) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | 12 | 0.9928 | 0.6157 | 4.72 | 0.015 | 435 | 0 | 4266 (2122) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 12 | 0.9928 | 0.6154 | 4.73 | 0.015 | 0 | 0 | 4383 (2114) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p5 (E-rt) | 12 | 0.8501 | 0.8501 | 0.00 | 0.000 | 0 | 0 | 3513 (3146) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | 12 | 0.8501 | 0.8501 | 0.00 | 0.000 | 0 | 0 | 3513 (3146) | 0 | 0 | 0 |
| rt; today's Realtime, delta=5 (E-rt) | 12 | 0.8501 | 0.8501 | 0.00 | 0.000 | 0 | 0 | 3427 (3070) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | 12 | 0.8501 | 0.8501 | 0.00 | 0.000 | 0 | 0 | 3427 (3070) | 0 | 0 | 0 |
| rt; today's Realtime, delta=50 (E-rt) | 12 | 0.8501 | 0.8501 | 0.00 | 0.000 | 0 | 0 | 3360 (3009) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 12 | 0.8501 | 0.8501 | 0.00 | 0.000 | 0 | 0 | 3360 (3009) | 0 | 0 | 0 |
| bulk; never gives up, strict order = today's Bulk (C3) | 12 | 1.0000 | 0.5779 | 5.51 | 0.004 | 0 | 0 | 5661 (2370) | 0 | 0 | 0 |

Matched share. Outage cell: the axis is the whole-run delivered share and the readouts are the whole-run share on time and the registered recovery time, as in stage 3e; the steady p50 is on the paired window (outage and recovery excluded). The new curve is B''s H = 0 sweep, interpolated per seed at the baseline's share, against the baseline; a baseline below the curve's lowest share reads the lowest point. Positive favours B''. Bars: shares max(0.05 (1 - ref), 1/units, NF), p50 max(NF_p50, 0.05 ref), recovery max(0.05 ref, 0.1 s, NF).

| baseline | readout | difference [95 %] | bar | outcome |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | whole-run on time | 0.00918 [0.00787, 0.0106] | 0.02 | equivalent |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery | 0.151 [0.134, 0.175] | 0.251 | equivalent |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | 0.000221 [0.000208, 0.00023] | 0.000292 | equivalent |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | whole-run on time | 0.000215 [-0.000862, 0.00143] | 0.0195 | equivalent |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery | -0.00962 [-0.0357, 0.0154] | 0.243 | equivalent |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | 0.000225 [0.000217, 0.000231] | 0.000292 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | whole-run on time | -0.00648 [-0.0076, -0.00523] | 0.0192 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery | -0.152 [-0.169, -0.127] | 0.236 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | 0.000206 [0.000191, 0.00022] | 0.000291 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | whole-run on time | -0.00648 [-0.0076, -0.00523] | 0.0192 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery | -0.152 [-0.169, -0.127] | 0.236 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | 0.000196 [0.000182, 0.000211] | 0.000291 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | whole-run on time | -0.00622 [-0.00696, -0.00555] | 0.0192 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | recovery | -0.144 [-0.168, -0.118] | 0.236 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | 0.000197 [0.000182, 0.000211] | 0.000291 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | whole-run on time | 0.0177 [0.0174, 0.018] | 0.0075 | shown better |
| rt; today's Realtime, delta=0p5 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | 0.00018 [0.00017, 0.000193] | 0.00029 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | whole-run on time | 0.0177 [0.0174, 0.018] | 0.0075 | shown better |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | 0.00018 [0.00017, 0.000193] | 0.00029 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | whole-run on time | 0.0177 [0.0174, 0.018] | 0.0075 | shown better |
| rt; today's Realtime, delta=5 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | 0.00018 [0.00017, 0.000193] | 0.00029 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | whole-run on time | 0.0177 [0.0174, 0.018] | 0.0075 | shown better |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | 0.00018 [0.00017, 0.000193] | 0.00029 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | whole-run on time | 0.0177 [0.0174, 0.018] | 0.0075 | shown better |
| rt; today's Realtime, delta=50 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | 0.000175 [0.00016, 0.000189] | 0.000289 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | whole-run on time | 0.0177 [0.0174, 0.018] | 0.0075 | shown better |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | 0.000175 [0.00016, 0.000189] | 0.000289 | equivalent |
| bulk; never gives up, strict order = today's Bulk (C3) | all readouts | identity against S5 at T=inf, H=inf: identical on every key and trace at 12 seeds | - | equivalent (identity) |

Words against every baseline: build A (stored rows, re-read now with the same reader and seeds) and B'' (14.5: every word unchanged is predicted):

| baseline | readout | A | B'' | changed |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | whole-run on time | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | whole-run on time | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | whole-run on time | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | whole-run on time | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | whole-run on time | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | recovery | equivalent | equivalent |  |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | equivalent | equivalent |  |
| rt; today's Realtime, delta=0p5 (E-rt) | whole-run on time | shown better | shown better |  |
| rt; today's Realtime, delta=0p5 (E-rt) | recovery | equivalent | equivalent |  |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | equivalent | equivalent |  |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | whole-run on time | shown better | shown better |  |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | recovery | equivalent | equivalent |  |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | equivalent | equivalent |  |
| rt; today's Realtime, delta=5 (E-rt) | whole-run on time | shown better | shown better |  |
| rt; today's Realtime, delta=5 (E-rt) | recovery | equivalent | equivalent |  |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | equivalent | equivalent |  |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | whole-run on time | shown better | shown better |  |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | recovery | equivalent | equivalent |  |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | equivalent | equivalent |  |
| rt; today's Realtime, delta=50 (E-rt) | whole-run on time | shown better | shown better |  |
| rt; today's Realtime, delta=50 (E-rt) | recovery | equivalent | equivalent |  |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | equivalent | equivalent |  |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | whole-run on time | shown better | shown better |  |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | recovery | equivalent | equivalent |  |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | equivalent | equivalent |  |
| bulk; never gives up, strict order = today's Bulk (C3) | all readouts | equivalent (identity) | equivalent (identity) |  |

B'' against the stored A at each row (id3e keys, seeds 0-11); net clamp gauges:

| row | seeds identical | whole-run delivered A / B'' | whole-run on time A / B'' | recovery A / B'' | net_bind (mean) | credits removed (mean) | repairs A / B'' |
|---|---|---|---|---|---|---|---|
| Bpp; lifetime T=0p4, hold 0 (S5) | 0 / 12 | 0.8674 / 0.8678 | 0.8674 / 0.8678 | 0.00 / 0.00 | 7835 | 474.5 | 15897 / 15613 |
| Bpp; lifetime T=0p6, hold 0 (S5) | 0 / 12 | 0.8777 / 0.8781 | 0.8490 / 0.8510 | 0.10 / 0.00 | 8343 | 430.2 | 15553 / 15319 |
| Bpp; lifetime T=0p8, hold 0 (S5) | 0 / 12 | 0.8879 / 0.8882 | 0.8294 / 0.8317 | 0.46 / 0.40 | 8331 | 427.1 | 15204 / 14973 |
| Bpp; lifetime T=1, hold 0 (S5) | 0 / 12 | 0.8975 / 0.8979 | 0.8092 / 0.8114 | 0.86 / 0.80 | 8321 | 423.9 | 14846 / 14617 |
| Bpp; lifetime T=1p2, hold 0 (S5) | 0 / 12 | 0.9079 / 0.9082 | 0.7897 / 0.7919 | 1.22 / 1.20 | 8306 | 420.4 | 14501 / 14275 |
| Bpp; lifetime T=1p25, hold 0 (S5) | 0 / 12 | 0.9103 / 0.9105 | 0.7847 / 0.7869 | 1.33 / 1.30 | 8308 | 419.8 | 14412 / 14186 |
| Bpp; lifetime T=1p3, hold 0 (S5) | 0 / 12 | 0.9129 / 0.9132 | 0.7798 / 0.7820 | 1.42 / 1.40 | 8308 | 419.3 | 14325 / 14100 |
| Bpp; lifetime T=1p35, hold 0 (S5) | 0 / 12 | 0.9154 / 0.9157 | 0.7748 / 0.7770 | 1.53 / 1.50 | 8296 | 418.1 | 14238 / 14013 |
| Bpp; lifetime T=1p4, hold 0 (S5) | 0 / 12 | 0.9177 / 0.9180 | 0.7697 / 0.7719 | 1.63 / 1.60 | 8304 | 417.5 | 14147 / 13922 |
| Bpp; lifetime T=1p45, hold 0 (S5) | 0 / 12 | 0.9202 / 0.9204 | 0.7647 / 0.7669 | 1.73 / 1.70 | 8305 | 416.8 | 14059 / 13834 |
| Bpp; lifetime T=1p5, hold 0 (S5) | 0 / 12 | 0.9228 / 0.9230 | 0.7599 / 0.7621 | 1.82 / 1.80 | 8304 | 416.1 | 13974 / 13750 |
| Bpp; lifetime T=1p55, hold 0 (S5) | 0 / 12 | 0.9249 / 0.9251 | 0.7545 / 0.7567 | 1.94 / 1.90 | 8302 | 415.5 | 13878 / 13655 |
| Bpp; lifetime T=1p6, hold 0 (S5) | 0 / 12 | 0.9275 / 0.9277 | 0.7496 / 0.7519 | 2.03 / 2.00 | 8296 | 414.7 | 13792 / 13570 |
| Bpp; lifetime T=2, hold 0 (S5) | 0 / 12 | 0.9478 / 0.9480 | 0.7103 / 0.7126 | 2.81 / 2.80 | 8278 | 408.1 | 13094 / 12877 |
| Bpp; lifetime T=inf, hold 0 (S5) | 12 / 12 | 1.0000 / 1.0000 | 0.5926 / 0.5926 | 5.20 / 5.20 | 0 | 0.0 | 11316 / 11316 |
| Bpp; lifetime T=0p4, hold inf (S5) | 0 / 12 | 0.8674 / 0.8678 | 0.8638 / 0.8612 | 0.00 / 0.00 | 7835 | 474.5 | 15897 / 15613 |
| Bpp; lifetime T=1p4, hold inf (S5) | 0 / 12 | 0.9177 / 0.9180 | 0.7558 / 0.7586 | 2.02 / 1.93 | 8304 | 417.5 | 14147 / 13922 |
| Bpp; lifetime T=inf, hold inf (S5) | 12 / 12 | 1.0000 / 1.0000 | 0.5779 / 0.5779 | 5.51 / 5.51 | 0 | 0.0 | 11316 / 11316 |

(CLw-long: 216 new runs, 4984 s wall at 4 workers)



## K-OV (reported only, 14.5; whole-run readouts)

| build | seeds | delivered | on time within 0.4 s | p50 (whole run) | repairs sent | decoded | net_bind |
|---|---|---|---|---|---|---|---|
| A | 6 | 0.7144 | 0.7143 | 0.322 | 33.8 | 4.5 | - |
| B' | 6 | 0.7191 | 0.7191 | 0.166 | 15.3 | 4.5 | - |
| B'' | 6 | 0.7229 | 0.7229 | 0.164 | 1.7 | 0.2 | 85 |

(K-OV: 6 new runs, 16 s wall at 2 workers)

