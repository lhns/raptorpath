
### OV-long

Builds: A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). 

| row | seeds | whole-run delivered | on time within 0.4s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=0p4, hold 0 (S5) | 6 | 0.8120 | 0.8120 | - | 0.993 | 0 | 0 | 400 (106) | 0 | 387146 | 386602 |
| A; lifetime T=0p6, hold 0 (S5) | 6 | 0.8160 | 0.0380 | - | 0.993 | 0 | 0 | 706 (144) | 0 | 386852 | 385706 |
| A; lifetime T=1, hold 0 (S5) | 6 | 0.8239 | 0.0380 | - | 0.993 | 0 | 0 | 1340 (232) | 0 | 386320 | 384046 |
| A; lifetime T=2, hold 0 (S5) | 6 | 0.8438 | 0.0380 | - | 0.993 | 0 | 0 | 2950 (425) | 0 | 384762 | 379763 |
| A; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.0380 | - | 0.994 | 0 | 0 | 15768 (1989) | 0 | 0 | 0 |
| A; lifetime T=0p4, hold inf (S5) | 6 | 0.8120 | 0.8120 | - | 0.993 | 0 | 0 | 400 (106) | 0 | 387146 | 386602 |
| A; lifetime T=2, hold inf (S5) | 6 | 0.8438 | 0.0261 | - | 0.993 | 0 | 0 | 2950 (425) | 0 | 384762 | 379763 |
| A; lifetime T=inf, hold inf (S5) | 6 | 1.0000 | 0.0261 | - | 0.994 | 0 | 0 | 15768 (1989) | 0 | 0 | 0 |
| B; lifetime T=0p4, hold 0 (S5) | 6 | 0.8045 | 0.8045 | - | 0.993 | 0 | 0 | 12680 (1610) | 0 | 373677 | 0 |
| B; lifetime T=0p6, hold 0 (S5) | 6 | 0.8045 | 0.8045 | - | 0.993 | 0 | 0 | 12684 (1609) | 0 | 373679 | 0 |
| B; lifetime T=1, hold 0 (S5) | 6 | 0.8045 | 0.8044 | - | 0.993 | 0 | 0 | 12695 (1615) | 0 | 373771 | 0 |
| B; lifetime T=2, hold 0 (S5) | 6 | 0.8045 | 0.8042 | - | 0.993 | 0 | 0 | 12694 (1614) | 0 | 373769 | 0 |
| B; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.0380 | - | 0.994 | 0 | 0 | 15768 (1989) | 0 | 0 | 0 |
| B; lifetime T=0p4, hold inf (S5) | 6 | 0.8045 | 0.8045 | - | 0.993 | 0 | 0 | 12680 (1610) | 0 | 373677 | 0 |
| B; lifetime T=2, hold inf (S5) | 6 | 0.8045 | 0.7908 | - | 0.993 | 0 | 0 | 12694 (1614) | 0 | 373769 | 0 |
| B; lifetime T=inf, hold inf (S5) | 6 | 1.0000 | 0.0261 | - | 0.994 | 0 | 0 | 15768 (1989) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 3 | 0.9974 | 0.0306 | - | 0.995 | 11089 | 0 | 14467 (277) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 3 | 0.9974 | 0.0337 | - | 0.994 | 0 | 0 | 14467 (309) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p5 (E-rt) | 3 | 0.8105 | 0.8099 | - | 0.741 | 0 | 0 | 42 (4) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | 3 | 0.8244 | 0.0403 | - | 0.792 | 0 | 0 | 9 (4) | 1 | 0 | 0 |
| rt; today's Realtime, delta=50 (E-rt) | 3 | 0.8104 | 0.8098 | - | 0.746 | 0 | 0 | 44 (4) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 3 | 0.8243 | 0.0400 | - | 0.749 | 0 | 0 | 11 (4) | 0 | 0 | 0 |
| bulk; never gives up, strict order = today's Bulk (C3) | 3 | 1.0000 | 0.0266 | - | 0.994 | 0 | 0 | 15739 (1989) | 0 | 0 | 0 |

Dominance (12.3): each baseline against the build's best finite-T point per seed pairing (the highest seed-mean steady on-time share; ties within its bar broken by the lower p50, then by the smaller T). Dominates = not worse beyond the bar on any readout and better beyond it on at least one.

| build | baseline | best point | share | on time | p50 | outcome |
|---|---|---|---|---|---|---|
| A | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=0p4 | -0.193 [-0.193, -0.192] | 0.805 [0.805, 0.805] | 5.33 [5.33, 5.33] | not dominated (a trade) |
| A | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=0p4 | -0.193 [-0.193, -0.192] | 0.805 [0.805, 0.805] | 4.74 [4.74, 4.74] | not dominated (a trade) |
| A | rt; today's Realtime, delta=0p5 (E-rt) | T=0p4 | -0.00345 [-0.00358, -0.00326] | -0.00346 [-0.00359, -0.00326] | -0.217 [-0.217, -0.217] | not dominated (a trade) |
| A | rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | T=0p4 | -0.0043 [-0.00442, -0.00414] | 0.805 [0.805, 0.805] | 0.34 [0.34, 0.34] | dominates |
| A | rt; today's Realtime, delta=50 (E-rt) | T=0p4 | -0.00345 [-0.00358, -0.00326] | -0.00346 [-0.00359, -0.00326] | -0.217 [-0.217, -0.217] | not dominated (a trade) |
| A | rteq; today's Realtime, delta=50 (E-rt) (engine queue) | T=0p4 | -0.00423 [-0.00431, -0.00416] | 0.805 [0.805, 0.805] | 0.34 [0.34, 0.34] | dominates |
| A | bulk; never gives up, strict order = today's Bulk (C3) | T=0p4 | -0.195 [-0.195, -0.195] | 0.805 [0.805, 0.805] | 4.98 [4.95, 5] | not dominated (a trade) |
| B | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=0p4 | -0.194 [-0.195, -0.194] | 0.803 [0.803, 0.803] | 5.57 [5.57, 5.57] | not dominated (a trade) |
| B | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=0p4 | -0.194 [-0.195, -0.194] | 0.803 [0.803, 0.803] | 4.98 [4.98, 4.98] | not dominated (a trade) |
| B | rt; today's Realtime, delta=0p5 (E-rt) | T=0p4 | -0.00507 [-0.00516, -0.00488] | -0.00507 [-0.00516, -0.00488] | 0.0784 [0.0784, 0.0784] | dominates |
| B | rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | T=0p4 | -0.00591 [-0.006, -0.00575] | 0.803 [0.803, 0.803] | 0.635 [0.635, 0.635] | dominates |
| B | rt; today's Realtime, delta=50 (E-rt) | T=0p4 | -0.00507 [-0.00516, -0.00488] | -0.00507 [-0.00516, -0.00488] | 0.0784 [0.0784, 0.0784] | dominates |
| B | rteq; today's Realtime, delta=50 (E-rt) (engine queue) | T=0p4 | -0.00584 [-0.00589, -0.00577] | 0.803 [0.803, 0.803] | 0.635 [0.635, 0.635] | dominates |
| B | bulk; never gives up, strict order = today's Bulk (C3) | T=0p4 | -0.197 [-0.197, -0.197] | 0.803 [0.803, 0.803] | 5.22 [5.19, 5.24] | not dominated (a trade) |
undetermined arms (a doubling would run them): []

### OV-long-v

Builds: A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). Delay p50 is reported only here (delta = 0.005, 12.3).

| row | seeds | whole-run delivered | on time within 2s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=1, hold 0 (S5) | 6 | 0.8277 | 0.8277 | - | 0.994 | 0 | 0 | 1528 (89) | 0 | 387518 | 351900 |
| A; lifetime T=1p5, hold 0 (S5) | 6 | 0.8377 | 0.8377 | - | 0.994 | 0 | 0 | 2353 (89) | 0 | 386695 | 326849 |
| A; lifetime T=2, hold 0 (S5) | 6 | 0.8476 | 0.8476 | - | 0.994 | 0 | 0 | 3192 (89) | 0 | 385854 | 301708 |
| A; lifetime T=3, hold 0 (S5) | 6 | 0.8676 | 0.2072 | - | 0.995 | 0 | 0 | 4893 (89) | 0 | 384147 | 252075 |
| A; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.2072 | - | 0.995 | 0 | 0 | 16152 (89) | 0 | 0 | 0 |
| A; lifetime T=1, hold inf (S5) | 6 | 0.8277 | 0.8277 | - | 0.994 | 0 | 0 | 1528 (89) | 0 | 387518 | 351900 |
| A; lifetime T=2, hold inf (S5) | 6 | 0.8476 | 0.8476 | - | 0.994 | 0 | 0 | 3192 (89) | 0 | 385854 | 301708 |
| A; lifetime T=inf, hold inf (S5) | 6 | 1.0000 | 0.1861 | - | 0.995 | 0 | 0 | 16152 (89) | 0 | 0 | 0 |
| B; lifetime T=1, hold 0 (S5) | 6 | 0.8083 | 0.8083 | - | 0.994 | 0 | 0 | 11132 (90) | 0 | 376479 | 0 |
| B; lifetime T=1p5, hold 0 (S5) | 6 | 0.8145 | 0.8145 | - | 0.994 | 0 | 0 | 11375 (89) | 0 | 377769 | 0 |
| B; lifetime T=2, hold 0 (S5) | 6 | 0.8345 | 0.8345 | - | 0.994 | 0 | 0 | 11954 (89) | 0 | 377494 | 0 |
| B; lifetime T=3, hold 0 (S5) | 6 | 0.8659 | 0.2072 | - | 0.994 | 0 | 0 | 13031 (89) | 0 | 376802 | 0 |
| B; lifetime T=inf, hold 0 (S5) | 6 | 1.0000 | 0.2072 | - | 0.995 | 0 | 0 | 16152 (89) | 0 | 0 | 0 |
| B; lifetime T=1, hold inf (S5) | 6 | 0.8083 | 0.8083 | - | 0.994 | 0 | 0 | 11132 (90) | 0 | 376479 | 0 |
| B; lifetime T=2, hold inf (S5) | 6 | 0.8345 | 0.8345 | - | 0.994 | 0 | 0 | 11954 (89) | 0 | 377494 | 0 |
| B; lifetime T=inf, hold inf (S5) | 6 | 1.0000 | 0.1861 | - | 0.995 | 0 | 0 | 16152 (89) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | 3 | 0.9971 | 0.1848 | - | 0.995 | 11086 | 0 | 14480 (6) | 0 | 0 | 0 |
| adr; ADR-0084 sender at rho=1, today's receiver (C1) | 3 | 0.9971 | 0.2062 | - | 0.995 | 0 | 0 | 14472 (0) | 0 | 0 | 0 |
| rt; today's Realtime, delta=0p005 (E-rt) | 3 | 0.8108 | 0.8108 | - | 0.800 | 0 | 0 | 46 (0) | 1 | 0 | 0 |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | 3 | 0.8249 | 0.8249 | - | 0.755 | 0 | 0 | 15 (0) | 0 | 0 | 0 |
| rt; today's Realtime, delta=50 (E-rt) | 3 | 0.8104 | 0.8104 | - | 0.746 | 0 | 0 | 44 (4) | 0 | 0 | 0 |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | 3 | 0.8243 | 0.8243 | - | 0.749 | 0 | 0 | 11 (4) | 0 | 0 | 0 |
| bulk; never gives up, strict order = today's Bulk (C3) | 3 | 1.0000 | 0.1876 | - | 0.995 | 0 | 0 | 16122 (85) | 0 | 0 | 0 |

Dominance (12.3): each baseline against the build's best finite-T point per seed pairing (the highest seed-mean steady on-time share; ties within its bar broken by the lower p50, then by the smaller T). Dominates = not worse beyond the bar on any readout and better beyond it on at least one.

| build | baseline | best point | share | on time | p50 | outcome |
|---|---|---|---|---|---|---|
| A | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=1 | -0.189 [-0.189, -0.189] | 0.779 [0.779, 0.779] | 4.88 [4.84, 4.9] | not dominated (a trade) (p50 reported only) |
| A | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=1 | -0.189 [-0.189, -0.189] | 0.751 [0.751, 0.751] | 4.27 [4.24, 4.29] | not dominated (a trade) (p50 reported only) |
| A | rt; today's Realtime, delta=0p005 (E-rt) | T=1 | -5.38e-05 [-0.000191, 0.00021] | -5.38e-05 [-0.000191, 0.00021] | -0.821 [-0.821, -0.821] | equivalent (p50 reported only) |
| A | rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | T=1 | -5.47e-05 [-0.000215, 0.000116] | -5.47e-05 [-0.000215, 0.000116] | -0.259 [-0.264, -0.256] | equivalent (p50 reported only) |
| A | rt; today's Realtime, delta=50 (E-rt) | T=1 | -5.38e-05 [-0.000191, 0.00021] | -5.38e-05 [-0.000191, 0.00021] | -0.821 [-0.821, -0.821] | equivalent (p50 reported only) |
| A | rteq; today's Realtime, delta=50 (E-rt) (engine queue) | T=1 | 1.16e-05 [-5.91e-05, 9.41e-05] | 1.16e-05 [-5.91e-05, 9.41e-05] | -0.264 [-0.264, -0.264] | equivalent (p50 reported only) |
| A | bulk; never gives up, strict order = today's Bulk (C3) | T=1 | -0.192 [-0.192, -0.192] | 0.776 [0.775, 0.776] | 4.49 [4.45, 4.5] | not dominated (a trade) (p50 reported only) |
| B | adr; ADR-0084 sender at rho=0p95, today's receiver (C1) | T=1 | -0.212 [-0.213, -0.212] | 0.74 [0.74, 0.74] | 5.14 [5.11, 5.16] | not dominated (a trade) (p50 reported only) |
| B | adr; ADR-0084 sender at rho=1, today's receiver (C1) | T=1 | -0.212 [-0.213, -0.212] | 0.713 [0.712, 0.713] | 4.52 [4.51, 4.54] | not dominated (a trade) (p50 reported only) |
| B | rt; today's Realtime, delta=0p005 (E-rt) | T=1 | -0.0235 [-0.0237, -0.0234] | -0.0235 [-0.0237, -0.0234] | -0.503 [-0.515, -0.495] | not dominated (a trade) (p50 reported only) |
| B | rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | T=1 | -0.0235 [-0.0236, -0.0235] | -0.0235 [-0.0236, -0.0235] | 0.0584 [0.0496, 0.0631] | not dominated (a trade) (p50 reported only) |
| B | rt; today's Realtime, delta=50 (E-rt) | T=1 | -0.0235 [-0.0237, -0.0234] | -0.0235 [-0.0237, -0.0234] | -0.503 [-0.515, -0.495] | not dominated (a trade) (p50 reported only) |
| B | rteq; today's Realtime, delta=50 (E-rt) (engine queue) | T=1 | -0.0235 [-0.0235, -0.0234] | -0.0235 [-0.0235, -0.0234] | 0.0535 [0.0423, 0.0625] | not dominated (a trade) (p50 reported only) |
| B | bulk; never gives up, strict order = today's Bulk (C3) | T=1 | -0.215 [-0.215, -0.215] | 0.737 [0.736, 0.737] | 4.75 [4.72, 4.78] | not dominated (a trade) (p50 reported only) |
undetermined arms (a doubling would run them): []
