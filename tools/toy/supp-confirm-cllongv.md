[14:37:06] CL-long-v supp: 25/36 runs (116 s)
[14:37:30] CL-long-v supp: 36/36 runs (140 s)

### CL-long-v

Builds: A = Revision 3 + clamp + floor repeat (evidenced_overdue) + elig rack; B = A + wait limit, no gate. Steady window per comparison, paired per seed (12.2). Delay p50 is reported only here (delta = 0.005, 12.3).

| row | seeds | whole-run delivered | on time within 2s | recovery (s) | elig_R_bind | floors | repeats | resends (dup) | unfinished | n_tau_lt_T (ticks, mean) | deq_early_would (mean) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A; lifetime T=0p4, hold 0 (S5) | 12 | 0.9412 | 0.9412 | 0.00 | 0.015 | 46 | 46 | 6213 (0) | 0 | 0 | 0 |
| A; lifetime T=0p6, hold 0 (S5) | 12 | 0.9511 | 0.9511 | 0.57 | 0.015 | 38 | 38 | 6316 (0) | 0 | 0 | 0 |
| A; lifetime T=0p8, hold 0 (S5) | 12 | 0.9615 | 0.9615 | 1.51 | 0.016 | 30 | 30 | 6420 (0) | 0 | 0 | 0 |
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
| rt; today's Realtime, delta=0p5 (E-rt) | whole-run on time | 0.0161 [0.0157, 0.0164] | 0.00375 | shown better |
| rt; today's Realtime, delta=0p5 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=0p5 (E-rt) | steady p50 | 0.000233 [0.000228, 0.000242] | 0.000292 | equivalent (p50 reported only) |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | whole-run on time | 0.0161 [0.0157, 0.0164] | 0.00375 | shown better |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=0p5 (E-rt) (engine queue) | steady p50 | 0.000233 [0.000228, 0.000242] | 0.000292 | equivalent (p50 reported only) |
| rt; today's Realtime, delta=5 (E-rt) | whole-run on time | 0.0161 [0.0157, 0.0164] | 0.00375 | shown better |
| rt; today's Realtime, delta=5 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=5 (E-rt) | steady p50 | 0.000233 [0.000228, 0.000242] | 0.000292 | equivalent (p50 reported only) |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | whole-run on time | 0.0161 [0.0157, 0.0164] | 0.00375 | shown better |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=5 (E-rt) (engine queue) | steady p50 | 0.000233 [0.000228, 0.000242] | 0.000292 | equivalent (p50 reported only) |
| rt; today's Realtime, delta=50 (E-rt) | whole-run on time | 0.0161 [0.0157, 0.0164] | 0.00375 | shown better |
| rt; today's Realtime, delta=50 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=50 (E-rt) | steady p50 | 0.000204 [0.000189, 0.000218] | 0.000291 | equivalent (p50 reported only) |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | whole-run on time | 0.0161 [0.0157, 0.0164] | 0.00375 | shown better |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=50 (E-rt) (engine queue) | steady p50 | 0.000204 [0.000189, 0.000218] | 0.000291 | equivalent (p50 reported only) |
| rt; today's Realtime, delta=0p005 (E-rt) | whole-run on time | 0.0188 [0.0185, 0.0192] | 0.00388 | shown better |
| rt; today's Realtime, delta=0p005 (E-rt) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rt; today's Realtime, delta=0p005 (E-rt) | steady p50 | 0.0173 [0.0167, 0.0179] | 0.00115 | shown better (p50 reported only) |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | whole-run on time | 0.0188 [0.0185, 0.0192] | 0.00388 | shown better |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | recovery | 0 [0, 0] | 0.1 | equivalent |
| rteq; today's Realtime, delta=0p005 (E-rt) (engine queue) | steady p50 | 0.0173 [0.0167, 0.0179] | 0.00115 | shown better (p50 reported only) |
| bulk; never gives up, strict order = today's Bulk (C3) | all readouts | identity against S5 at T=inf, H=inf: identical on every key and trace at 12 seeds | - | equivalent (identity) |
undetermined: []
