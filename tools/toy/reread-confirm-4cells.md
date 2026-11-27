
### CLw-long

Builds: A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). 

| row | seeds | whole-run delivered | on time within 0.4s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=0p4, hold 0 (S5) | 12 | 0.8674 | 0.8674 | 0.00 | 0.010 | 106 | 106 | 3396 (2925) | 0 | 0 | 0 |
| A; lifetime T=0p6, hold 0 (S5) | 12 | 0.8777 | 0.8490 | 0.10 | 0.010 | 97 | 97 | 3482 (2880) | 0 | 0 | 0 |
| A; lifetime T=0p8, hold 0 (S5) | 12 | 0.8879 | 0.8294 | 0.46 | 0.010 | 89 | 89 | 3571 (2838) | 0 | 0 | 0 |
| A; lifetime T=1, hold 0 (S5) | 12 | 0.8975 | 0.8092 | 0.86 | 0.010 | 81 | 81 | 3659 (2792) | 0 | 0 | 0 |
| A; lifetime T=1p2, hold 0 (S5) | 12 | 0.9079 | 0.7897 | 1.22 | 0.010 | 73 | 73 | 3744 (2748) | 0 | 0 | 0 |
| A; lifetime T=1p25, hold 0 (S5) | 12 | 0.9103 | 0.7847 | 1.33 | 0.010 | 71 | 71 | 3770 (2739) | 0 | 0 | 0 |
| A; lifetime T=1p3, hold 0 (S5) | 12 | 0.9129 | 0.7798 | 1.42 | 0.010 | 69 | 69 | 3796 (2731) | 0 | 0 | 0 |
| A; lifetime T=1p35, hold 0 (S5) | 12 | 0.9154 | 0.7748 | 1.53 | 0.010 | 67 | 67 | 3808 (2711) | 0 | 0 | 0 |
| A; lifetime T=1p4, hold 0 (S5) | 12 | 0.9177 | 0.7697 | 1.63 | 0.011 | 65 | 65 | 3828 (2698) | 0 | 0 | 0 |
| A; lifetime T=1p45, hold 0 (S5) | 12 | 0.9202 | 0.7647 | 1.73 | 0.011 | 63 | 63 | 3858 (2697) | 0 | 0 | 0 |
| A; lifetime T=1p5, hold 0 (S5) | 12 | 0.9228 | 0.7599 | 1.82 | 0.011 | 61 | 61 | 3881 (2687) | 0 | 0 | 0 |
| A; lifetime T=1p55, hold 0 (S5) | 12 | 0.9249 | 0.7545 | 1.94 | 0.011 | 59 | 59 | 3901 (2674) | 0 | 0 | 0 |
| A; lifetime T=1p6, hold 0 (S5) | 12 | 0.9275 | 0.7496 | 2.03 | 0.011 | 57 | 57 | 3918 (2662) | 0 | 0 | 0 |
| A; lifetime T=2, hold 0 (S5) | 12 | 0.9478 | 0.7103 | 2.81 | 0.011 | 41 | 41 | 4098 (2573) | 0 | 0 | 0 |
| A; lifetime T=inf, hold 0 (S5) | 12 | 1.0000 | 0.5926 | 5.20 | 0.004 | 0 | 0 | 5661 (2370) | 0 | 0 | 0 |
| A; lifetime T=0p4, hold inf (S5) | 12 | 0.8674 | 0.8638 | 0.00 | 0.010 | 106 | 106 | 3396 (2925) | 0 | 0 | 0 |
| A; lifetime T=1p4, hold inf (S5) | 12 | 0.9177 | 0.7558 | 2.02 | 0.011 | 65 | 65 | 3828 (2698) | 0 | 0 | 0 |
| A; lifetime T=inf, hold inf (S5) | 12 | 1.0000 | 0.5779 | 5.51 | 0.004 | 0 | 0 | 5661 (2370) | 0 | 0 | 0 |
| B; lifetime T=0p4, hold 0 (S5) | 3 | 0.8677 | 0.8677 | 0.00 | 0.010 | 105 | 105 | 3401 (2957) | 0 | 0 | 0 |
| B; lifetime T=2, hold 0 (S5) | 3 | 0.9479 | 0.7104 | 2.80 | 0.011 | 41 | 41 | 4077 (2581) | 0 | 0 | 0 |
| B; lifetime T=inf, hold 0 (S5) | 3 | 1.0000 | 0.5921 | 5.20 | 0.004 | 0 | 0 | 5673 (2405) | 0 | 0 | 0 |
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

Matched share. Outage cell: the axis is the whole-run delivered share and the readouts are the whole-run share on time and the registered recovery time, as in stage 3e; the steady p50 is on the paired window (outage and recovery excluded). The new curve is build A's H = 0 sweep, interpolated per seed at the baseline's share, against the baseline; a baseline below the curve's lowest share reads the lowest point. Positive favours build A. Bars: shares max(0.05 (1 - ref), 1/units, NF), p50 max(NF_p50, 0.05 ref), recovery max(0.05 ref, 0.1 s, NF).

| baseline | readout | difference [95 %] | bar | outcome |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | whole-run on time | 0.00879 [0.00749, 0.0102] | 0.02 | equivalent |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery | 0.148 [0.131, 0.173] | 0.251 | equivalent |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | 0.00022 [0.000208, 0.000228] | 0.000292 | equivalent |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | whole-run on time | -0.000168 [-0.00124, 0.00105] | 0.0195 | equivalent |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery | -0.0121 [-0.0392, 0.0138] | 0.243 | equivalent |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | 0.000225 [0.000216, 0.00023] | 0.000292 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | whole-run on time | -0.00686 [-0.00798, -0.00561] | 0.0192 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery | -0.154 [-0.172, -0.129] | 0.236 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | 0.000205 [0.00019, 0.000219] | 0.000291 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | whole-run on time | -0.00686 [-0.00798, -0.00561] | 0.0192 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery | -0.154 [-0.172, -0.129] | 0.236 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | 0.000196 [0.000181, 0.00021] | 0.000291 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | whole-run on time | -0.00661 [-0.00734, -0.00593] | 0.0192 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | recovery | -0.146 [-0.17, -0.12] | 0.236 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | 0.000196 [0.000181, 0.00021] | 0.000291 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | whole-run on time | 0.0174 [0.0171, 0.0176] | 0.0075 | shown better |
| rt; today's Realtime, delta=0p5 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | 0.000175 [0.00017, 0.000184] | 0.00029 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | whole-run on time | 0.0174 [0.0171, 0.0176] | 0.0075 | shown better |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | 0.000175 [0.00017, 0.000184] | 0.00029 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | whole-run on time | 0.0174 [0.0171, 0.0177] | 0.0075 | shown better |
| rt; today's Realtime, delta=5 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | 0.000175 [0.00017, 0.000184] | 0.00029 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | whole-run on time | 0.0174 [0.0171, 0.0177] | 0.0075 | shown better |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | 0.000175 [0.00017, 0.000184] | 0.00029 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | whole-run on time | 0.0174 [0.0171, 0.0177] | 0.0075 | shown better |
| rt; today's Realtime, delta=50 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | 0.00017 [0.000156, 0.000184] | 0.000289 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | whole-run on time | 0.0174 [0.0171, 0.0177] | 0.0075 | shown better |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | 0.00017 [0.000156, 0.000184] | 0.000289 | equivalent |
| bulk; never gives up, strict order = today's Bulk (C3) | all readouts | identity against S5 at T=inf, H=inf: identical on every key and trace at 12 seeds | - | equivalent (identity) |
undetermined arms (a doubling would run them): []

### CLc

Builds: A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). 

| row | seeds | whole-run delivered | on time within 0.4s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=0p4, hold 0 (S5) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2382 (2147) | 0 | 0 | 0 |
| A; lifetime T=1, hold 0 (S5) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2385 (2147) | 0 | 0 | 0 |
| A; lifetime T=inf, hold 0 (S5) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2385 (2147) | 0 | 0 | 0 |
| A; lifetime T=0p4, hold inf (S5) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2382 (2147) | 0 | 0 | 0 |
| A; lifetime T=1, hold inf (S5) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2385 (2147) | 0 | 0 | 0 |
| A; lifetime T=inf, hold inf (S5) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2385 (2147) | 0 | 0 | 0 |
| B; lifetime T=0p4, hold 0 (S5) | 3 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2357 (2137) | 0 | 0 | 0 |
| B; lifetime T=1, hold 0 (S5) | 3 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2361 (2139) | 0 | 0 | 0 |
| B; lifetime T=inf, hold 0 (S5) | 3 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2361 (2139) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 12 | 1.0000 | 1.0000 | - | 0.000 | 1864 | 0 | 2380 (2141) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | 12 | 1.0000 | 1.0000 | - | 0.000 | 1864 | 0 | 2382 (2143) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | 12 | 1.0000 | 1.0000 | - | 0.000 | 1862 | 0 | 2382 (2142) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | 12 | 1.0000 | 1.0000 | - | 0.000 | 1862 | 0 | 2386 (2147) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2384 (2147) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p5 (E-rt) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2507 (2258) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2507 (2258) | 0 | 0 | 0 |
| rt; today's Realtime, delta=5 (E-rt) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2447 (2201) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2447 (2201) | 0 | 0 | 0 |
| rt; today's Realtime, delta=50 (E-rt) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2382 (2145) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2382 (2145) | 0 | 0 | 0 |
| bulk; never gives up, strict order = today's Bulk (C3) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2385 (2147) | 0 | 0 | 0 |

Matched share. Headroom cell: the axis is the steady delivered share on the paired window. The new curve is build A's H = 0 sweep, interpolated per seed at the baseline's share, against the baseline; a baseline below the curve's lowest share reads the lowest point. Positive favours build A. Bars: shares max(0.05 (1 - ref), 1/units, NF), p50 max(NF_p50, 0.05 ref), recovery max(0.05 ref, 0.1 s, NF).

| baseline | readout | difference [95 %] | bar | outcome |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | 0.000228 [0.000228, 0.000228] | 0.000292 | equivalent |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | 0.000228 [0.000228, 0.000228] | 0.000292 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | 0.000228 [0.000228, 0.000228] | 0.000292 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | 0.000228 [0.000228, 0.000228] | 0.000292 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | 0.000199 [0.000185, 0.000214] | 0.000291 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | 0.000228 [0.000228, 0.000228] | 0.000292 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | 0.000228 [0.000228, 0.000228] | 0.000292 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | 0.000219 [0.000204, 0.000228] | 0.000292 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | 0.000219 [0.000204, 0.000228] | 0.000292 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | 0.000199 [0.000185, 0.000214] | 0.000291 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | 0.000199 [0.000185, 0.000214] | 0.000291 | equivalent |
| bulk; never gives up, strict order = today's Bulk (C3) | all readouts | identity against S5 at T=inf, H=inf: identical on every key and trace at 12 seeds | - | equivalent (identity) |
undetermined arms (a doubling would run them): []

### CLc-v

Builds: A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). Delay p50 is reported only here (delta = 0.005, 12.3).

| row | seeds | whole-run delivered | on time within 2s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=1, hold 0 (S5) | 12 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3308 (0) | 0 | 0 | 0 |
| A; lifetime T=1p5, hold 0 (S5) | 12 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3308 (0) | 0 | 0 | 0 |
| A; lifetime T=2, hold 0 (S5) | 12 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3308 (0) | 0 | 0 | 0 |
| A; lifetime T=3, hold 0 (S5) | 12 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3308 (0) | 0 | 0 | 0 |
| A; lifetime T=inf, hold 0 (S5) | 12 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3308 (0) | 0 | 0 | 0 |
| A; lifetime T=1, hold inf (S5) | 12 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3308 (0) | 0 | 0 | 0 |
| A; lifetime T=2, hold inf (S5) | 12 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3308 (0) | 0 | 0 | 0 |
| A; lifetime T=inf, hold inf (S5) | 12 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3308 (0) | 0 | 0 | 0 |
| B; lifetime T=1, hold 0 (S5) | 3 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3257 (0) | 0 | 0 | 0 |
| B; lifetime T=2, hold 0 (S5) | 3 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3257 (0) | 0 | 0 | 0 |
| B; lifetime T=inf, hold 0 (S5) | 3 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3257 (0) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 12 | 0.9974 | 0.9974 | - | 0.001 | 338 | 0 | 3414 (354) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | 12 | 0.9974 | 0.9974 | - | 0.001 | 338 | 0 | 3414 (354) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | 12 | 0.9974 | 0.9974 | - | 0.001 | 337 | 0 | 3413 (353) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | 12 | 0.9974 | 0.9974 | - | 0.001 | 334 | 0 | 3409 (350) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 12 | 0.9971 | 0.9971 | - | 0.001 | 0 | 0 | 2983 (0) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p5 (E-rt) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2507 (2258) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2507 (2258) | 0 | 0 | 0 |
| rt; today's Realtime, delta=5 (E-rt) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2447 (2201) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2447 (2201) | 0 | 0 | 0 |
| rt; today's Realtime, delta=50 (E-rt) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2382 (2145) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 12 | 1.0000 | 1.0000 | - | 0.000 | 0 | 0 | 2382 (2145) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p005 (E-rt) | 12 | 0.9971 | 0.9971 | - | 0.001 | 0 | 0 | 2983 (0) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | 12 | 0.9971 | 0.9971 | - | 0.001 | 0 | 0 | 2983 (0) | 0 | 0 | 0 |
| bulk; never gives up, strict order = today's Bulk (C3) | 12 | 1.0000 | 1.0000 | - | 0.001 | 0 | 0 | 3308 (0) | 0 | 0 | 0 |

Matched share. Headroom cell: the axis is the steady delivered share on the paired window. The new curve is build A's H = 0 sweep, interpolated per seed at the baseline's share, against the baseline; a baseline below the curve's lowest share reads the lowest point. Positive favours build A. Bars: shares max(0.05 (1 - ref), 1/units, NF), p50 max(NF_p50, 0.05 ref), recovery max(0.05 ref, 0.1 s, NF).

| baseline | readout | difference [95 %] | bar | outcome |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady on time | 0.00259 [0.00252, 0.00267] | 0.00013 | shown better |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | 0.00464 [0.00461, 0.00467] | 0.000513 | shown better (p50 reported only) |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady on time | 0.00259 [0.00252, 0.00267] | 0.00013 | shown better |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | 0.00464 [0.00461, 0.00467] | 0.000513 | shown better (p50 reported only) |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady on time | 0.00259 [0.00252, 0.00267] | 0.00013 | shown better |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | 0.00464 [0.00461, 0.00467] | 0.000513 | shown better (p50 reported only) |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady on time | 0.00259 [0.00252, 0.00267] | 0.00013 | shown better |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | 0.00464 [0.00461, 0.00467] | 0.000513 | shown better (p50 reported only) |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady on time | 0.00295 [0.00286, 0.00304] | 0.000148 | shown better |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | 0.00465 [0.00462, 0.00468] | 0.000513 | shown better (p50 reported only) |
| rt; today's Realtime, delta=0p5 (E-rt) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | 0.000237 [0.000228, 0.000251] | 0.000292 | equivalent (p50 reported only) |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | 0.000237 [0.000228, 0.000251] | 0.000292 | equivalent (p50 reported only) |
| rt; today's Realtime, delta=5 (E-rt) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | 0.000223 [0.000204, 0.000242] | 0.000292 | equivalent (p50 reported only) |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | 0.000223 [0.000204, 0.000242] | 0.000292 | equivalent (p50 reported only) |
| rt; today's Realtime, delta=50 (E-rt) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | 0.000204 [0.000189, 0.000218] | 0.000291 | equivalent (p50 reported only) |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady on time | 0 [0, 0] | 1e-05 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | 0.000204 [0.000189, 0.000218] | 0.000291 | equivalent (p50 reported only) |
| rt; today's Realtime, delta=0p005 (E-rt) | steady on time | 0.00295 [0.00286, 0.00304] | 0.000148 | shown better |
| rt; today's Realtime, delta=0p005 (E-rt) | steady p50 | 0.00465 [0.00462, 0.00468] | 0.000513 | shown better (p50 reported only) |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | steady on time | 0.00295 [0.00286, 0.00304] | 0.000148 | shown better |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | steady p50 | 0.00465 [0.00462, 0.00468] | 0.000513 | shown better (p50 reported only) |
| bulk; never gives up, strict order = today's Bulk (C3) | all readouts | identity against S5 at T=inf, H=inf: identical on every key and trace at 12 seeds | - | equivalent (identity) |
undetermined arms (a doubling would run them): []

### CL-long-v

Builds: A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). Delay p50 is reported only here (delta = 0.005, 12.3).

| row | seeds | whole-run delivered | on time within 2s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=1, hold 0 (S5) | 12 | 0.9708 | 0.9708 | 2.50 | 0.017 | 22 | 22 | 6517 (0) | 0 | 0 | 0 |
| A; lifetime T=1p5, hold 0 (S5) | 12 | 0.9953 | 0.9953 | 4.74 | 0.010 | 2 | 2 | 6907 (0) | 0 | 0 | 0 |
| A; lifetime T=2, hold 0 (S5) | 12 | 0.9993 | 0.9991 | 5.62 | 0.003 | 0 | 0 | 7904 (95) | 0 | 11 | 0 |
| A; lifetime T=3, hold 0 (S5) | 12 | 1.0000 | 0.9989 | 5.67 | 0.003 | 0 | 0 | 8039 (69) | 0 | 11 | 0 |
| A; lifetime T=inf, hold 0 (S5) | 12 | 1.0000 | 0.9989 | 5.67 | 0.003 | 0 | 0 | 8039 (69) | 0 | 0 | 0 |
| A; lifetime T=1, hold inf (S5) | 12 | 0.9708 | 0.9708 | 3.53 | 0.017 | 22 | 22 | 6517 (0) | 0 | 0 | 0 |
| A; lifetime T=2, hold inf (S5) | 12 | 0.9993 | 0.9981 | 6.59 | 0.003 | 0 | 0 | 7904 (95) | 0 | 11 | 0 |
| A; lifetime T=inf, hold inf (S5) | 12 | 1.0000 | 0.9849 | 6.73 | 0.003 | 0 | 0 | 8039 (69) | 0 | 0 | 0 |
| B; lifetime T=1, hold 0 (S5) | 3 | 0.9707 | 0.9707 | 2.50 | 0.016 | 23 | 23 | 6554 (0) | 0 | 0 | 0 |
| B; lifetime T=2, hold 0 (S5) | 3 | 0.9993 | 0.9991 | 5.57 | 0.003 | 0 | 0 | 7936 (101) | 0 | 0 | 0 |
| B; lifetime T=inf, hold 0 (S5) | 3 | 1.0000 | 0.9990 | 5.60 | 0.004 | 0 | 0 | 8055 (79) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 12 | 0.9923 | 0.9923 | 5.50 | 0.012 | 2422 | 0 | 6469 (413) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | 12 | 0.9923 | 0.9923 | 5.13 | 0.012 | 1576 | 0 | 6517 (452) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | 12 | 0.9922 | 0.9922 | 4.83 | 0.014 | 348 | 0 | 6320 (295) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | 12 | 0.9921 | 0.9921 | 4.83 | 0.015 | 128 | 0 | 6043 (69) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 12 | 0.9921 | 0.9921 | 4.80 | 0.015 | 0 | 0 | 6020 (0) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p5 (E-rt) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4671 (4181) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4671 (4181) | 0 | 0 | 0 |
| rt; today's Realtime, delta=5 (E-rt) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4554 (4078) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4554 (4078) | 0 | 0 | 0 |
| rt; today's Realtime, delta=50 (E-rt) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4446 (3980) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 12 | 0.9251 | 0.9251 | 0.00 | 0.000 | 0 | 0 | 4446 (3980) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p005 (E-rt) | 12 | 0.9223 | 0.9223 | 0.00 | 0.001 | 0 | 0 | 5554 (0) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | 12 | 0.9223 | 0.9223 | 0.00 | 0.001 | 0 | 0 | 5554 (0) | 0 | 0 | 0 |
| bulk; never gives up, strict order = today's Bulk (C3) | 12 | 1.0000 | 0.9849 | 6.73 | 0.003 | 0 | 0 | 8039 (69) | 0 | 0 | 0 |

Matched share. Outage cell: the axis is the whole-run delivered share and the readouts are the whole-run share on time and the registered recovery time, as in stage 3e; the steady p50 is on the paired window (outage and recovery excluded). The new curve is build A's H = 0 sweep, interpolated per seed at the baseline's share, against the baseline; a baseline below the curve's lowest share reads the lowest point. Positive favours build A. Bars: shares max(0.05 (1 - ref), 1/units, NF), p50 max(NF_p50, 0.05 ref), recovery max(0.05 ref, 0.1 s, NF).

| baseline | readout | difference [95 %] | bar | outcome |
|---|---|---|---|---|
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | whole-run on time | 0 [0, 0] | 0.000385 | equivalent |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery | 1.03 [0.97, 1.1] | 0.275 | shown better |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | steady p50 | 0.00592 [0.00586, 0.00597] | 0.000579 | shown better (p50 reported only) |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | whole-run on time | 0 [0, 0] | 0.000384 | equivalent |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery | 0.663 [0.593, 0.743] | 0.257 | shown better |
| adr; ADR-0084 sender at rho=0p98, today's receiver (C1) | steady p50 | 0.00568 [0.00561, 0.00575] | 0.000567 | shown better (p50 reported only) |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | whole-run on time | 0 [0, 0] | 0.000389 | equivalent |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery | 0.372 [0.301, 0.451] | 0.242 | shown better |
| adr; ADR-0084 sender at rho=0p99, today's receiver (C1) | steady p50 | 0.00553 [0.0055, 0.00557] | 0.000561 | shown better (p50 reported only) |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | whole-run on time | 0 [0, 0] | 0.000395 | equivalent |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery | 0.382 [0.312, 0.462] | 0.242 | shown better |
| adr; ADR-0084 sender at rho=0p995, today's receiver (C1) | steady p50 | 0.00556 [0.00553, 0.00559] | 0.000562 | shown better (p50 reported only) |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | whole-run on time | 0 [0, 0] | 0.000396 | equivalent |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | recovery | 0.351 [0.301, 0.395] | 0.24 | shown better |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | steady p50 | 0.00556 [0.00553, 0.00559] | 0.000562 | shown better (p50 reported only) |
| rt; today's Realtime, delta=0p5 (E-rt) | whole-run on time | 0.0457 [0.0454, 0.046] | 0.00375 | shown better |
| rt; today's Realtime, delta=0p5 (E-rt) | recovery | -2.5 [-2.5, -2.5] | 0.1 | shown worse |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | 0.000172 [0.000172, 0.000172] | 0.000292 | equivalent (p50 reported only) |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | whole-run on time | 0.0457 [0.0454, 0.046] | 0.00375 | shown better |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | recovery | -2.5 [-2.5, -2.5] | 0.1 | shown worse |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | 0.000172 [0.000172, 0.000172] | 0.000292 | equivalent (p50 reported only) |
| rt; today's Realtime, delta=5 (E-rt) | whole-run on time | 0.0457 [0.0454, 0.046] | 0.00375 | shown better |
| rt; today's Realtime, delta=5 (E-rt) | recovery | -2.5 [-2.5, -2.5] | 0.1 | shown worse |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | 0.000172 [0.000172, 0.000172] | 0.000292 | equivalent (p50 reported only) |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | whole-run on time | 0.0457 [0.0454, 0.046] | 0.00375 | shown better |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | recovery | -2.5 [-2.5, -2.5] | 0.1 | shown worse |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | 0.000172 [0.000172, 0.000172] | 0.000292 | equivalent (p50 reported only) |
| rt; today's Realtime, delta=50 (E-rt) | whole-run on time | 0.0457 [0.0454, 0.046] | 0.00375 | shown better |
| rt; today's Realtime, delta=50 (E-rt) | recovery | -2.5 [-2.5, -2.5] | 0.1 | shown worse |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | 0.000138 [0.000124, 0.000153] | 0.000291 | equivalent (p50 reported only) |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | whole-run on time | 0.0457 [0.0454, 0.046] | 0.00375 | shown better |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | recovery | -2.5 [-2.5, -2.5] | 0.1 | shown worse |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | 0.000138 [0.000124, 0.000153] | 0.000291 | equivalent (p50 reported only) |
| rt; today's Realtime, delta=0p005 (E-rt) | whole-run on time | 0.0484 [0.0481, 0.0487] | 0.00388 | shown better |
| rt; today's Realtime, delta=0p005 (E-rt) | recovery | -2.5 [-2.5, -2.5] | 0.1 | shown worse |
| rt; today's Realtime, delta=0p005 (E-rt) | steady p50 | 0.0169 [0.0162, 0.0175] | 0.00113 | shown better (p50 reported only) |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | whole-run on time | 0.0484 [0.0481, 0.0487] | 0.00388 | shown better |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | recovery | -2.5 [-2.5, -2.5] | 0.1 | shown worse |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | steady p50 | 0.0169 [0.0162, 0.0175] | 0.00113 | shown better (p50 reported only) |
| bulk; never gives up, strict order = today's Bulk (C3) | all readouts | identity against S5 at T=inf, H=inf: identical on every key and trace at 12 seeds | - | equivalent (identity) |
undetermined arms (a doubling would run them): []
